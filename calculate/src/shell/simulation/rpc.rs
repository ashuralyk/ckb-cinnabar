use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::{
    error::{self, CalculatorError},
    indexer::{json, Indexer, LiveCell, Pagination, ScriptType, SearchKey, SearchMode, Tx},
    rpc::{Host, Network, Node, Rpc, RPC},
    skeleton::CellOutputEx,
    types::{h256_to_hash, hash_to_h256, Hash256},
};
use ckb_jsonrpc_types::{
    BlockNumber, BlockView, CellData, CellInfo, CellWithStatus, ChainInfo, HeaderView, JsonBytes,
    OutPoint, OutputsValidator, ResponseFormat, Status, Transaction, TransactionView,
    TransactionWithStatusResponse, TxStatus,
};
use ckb_types::{
    core, packed,
    prelude::{IntoTransactionView, Unpack},
    H256,
};

/// In-memory chain state backing [`FakeRpcClient`].
#[derive(Default, Clone)]
pub struct FakeProvider {
    /// Live cells addressable by out-point.
    pub fake_cells: Vec<(OutPoint, CellOutputEx)>,
    /// Headers addressable by block hash.
    pub fake_headers: HashMap<H256, HeaderView>,
    /// Maps a cell's out-point to the header of the block that committed it
    /// (used to fill `TransactionInfo` during simulation).
    pub fake_outpoint_headers: HashMap<OutPoint, core::HeaderView>,
    /// Committed fake transactions by hash.
    pub fake_transaction: HashMap<H256, (TxStatus, Transaction)>,
    /// Indexer `get_transactions` search results (tx hash + io markers).
    pub fake_txs: Vec<json::Tx>,
    /// Reported tx-pool min fee rate (shannons/byte).
    pub fake_feerate: u64,
    /// Reported tip block number.
    pub fake_tipnumber: u64,
    /// Reported tip header.
    pub fake_tipheader: HeaderView,
}

fn live_from_fake(out_point: &OutPoint, cell: &CellOutputEx) -> LiveCell {
    LiveCell {
        output: cell.output.clone(),
        output_data: cell.data.clone(),
        out_point: out_point.clone().into(),
        block_number: 0,
        tx_index: 0,
    }
}

fn script_prefix_equal(a: Option<&packed::Script>, b: Option<&packed::Script>) -> bool {
    if let (Some(a), Some(b)) = (a, b) {
        a.code_hash() == b.code_hash()
            && a.hash_type() == b.hash_type()
            && a.args().raw_data().starts_with(&b.args().raw_data())
    } else {
        false
    }
}

fn script_partial_equal(
    haystack: Option<&packed::Script>,
    needle: Option<&packed::Script>,
) -> bool {
    let (Some(haystack), Some(needle)) = (haystack, needle) else {
        return false;
    };
    if haystack.code_hash() != needle.code_hash() || haystack.hash_type() != needle.hash_type() {
        return false;
    }
    let h = haystack.args().raw_data();
    let n = needle.args().raw_data();
    if n.is_empty() {
        return true;
    }
    h.windows(n.len()).any(|w| w == n.as_ref())
}

impl FakeProvider {
    fn get_cells_by_search_key(
        &self,
        search_key: &SearchKey,
        limit: usize,
        cursor: Option<&[u8]>,
    ) -> (Vec<LiveCell>, usize) {
        if limit == 0 {
            return (vec![], 0);
        }
        let mut offset = cursor
            .and_then(|v| <[u8; 8]>::try_from(v).ok())
            .map(usize::from_le_bytes)
            .unwrap_or_default();
        let mut objects = vec![];
        for (out_point, cell) in self.fake_cells.iter().skip(offset) {
            offset += 1;
            let (primary_script, script_a, secondary_script, script_b) =
                match search_key.script_type {
                    ScriptType::Lock => {
                        let secondary_script: Option<Option<packed::Script>> =
                            search_key.filter.clone().map(|v| v.script);
                        let lock_script = cell.lock_script();
                        let type_script = cell.type_script();
                        (
                            search_key.script.clone(),
                            Some(lock_script),
                            secondary_script,
                            type_script,
                        )
                    }
                    ScriptType::Type => {
                        let secondary_script: Option<Option<packed::Script>> =
                            search_key.filter.clone().map(|v| v.script);
                        let lock_script = cell.lock_script();
                        let type_script = cell.type_script();
                        (
                            search_key.script.clone(),
                            type_script,
                            secondary_script,
                            Some(lock_script),
                        )
                    }
                };
            match search_key.script_search_mode {
                Some(SearchMode::Exact) | None => {
                    if Some(primary_script) == script_a {
                        if let Some(script) = secondary_script {
                            if script != script_b {
                                continue;
                            }
                        }
                        objects.push(live_from_fake(out_point, cell));
                    }
                }
                Some(SearchMode::Prefix) => {
                    if script_prefix_equal(script_a.as_ref(), Some(&primary_script)) {
                        if let Some(script) = secondary_script {
                            if !script_prefix_equal(script_b.as_ref(), script.as_ref()) {
                                continue;
                            }
                        }
                        objects.push(live_from_fake(out_point, cell))
                    }
                }
                Some(SearchMode::Partial) => {
                    if script_partial_equal(script_a.as_ref(), Some(&primary_script)) {
                        if let Some(script) = secondary_script {
                            if !script_partial_equal(script_b.as_ref(), script.as_ref()) {
                                continue;
                            }
                        }
                        objects.push(live_from_fake(out_point, cell));
                    }
                }
            }
            if objects.len() >= limit {
                break;
            }
        }
        (objects, offset)
    }

    fn get_cell_by_outpoint(&self, out_point: &OutPoint) -> Option<CellWithStatus> {
        let (_, cell) = self
            .fake_cells
            .iter()
            .find(|(value, _)| value == out_point)?;
        let cell_with_status = CellWithStatus {
            cell: Some(CellInfo {
                data: Some(CellData {
                    content: JsonBytes::from_vec(cell.data.clone()),
                    hash: H256::default(),
                }),
                output: cell.output.clone().into(),
            }),
            status: "live".to_owned(),
            // Fake cells have no recorded block; newer ckb-jsonrpc-types
            // requires the field, so leave it unknown.
            block_hash: None,
        };
        Some(cell_with_status)
    }

    fn get_header_by_hash(&self, block_hash: &H256) -> Option<HeaderView> {
        self.fake_headers.get(block_hash).cloned()
    }

    fn get_header_by_number(&self, block_number: u64) -> Option<HeaderView> {
        self.fake_headers
            .iter()
            .find(|(_, header)| header.inner.number == block_number.into())
            .map(|(_, header)| header.clone())
    }

    fn get_transaction_by_hash(&self, hash: &H256) -> Option<TransactionWithStatusResponse> {
        self.fake_transaction
            .get(hash)
            .map(|(status, tx)| TransactionWithStatusResponse {
                transaction: Some(ResponseFormat::json(TransactionView {
                    inner: tx.clone(),
                    hash: hash.clone(),
                })),
                cycles: None,
                time_added_to_pool: None,
                fee: None,
                min_replace_fee: None,
                tx_status: status.clone(),
            })
    }
}

/// Offline [`RPC`] implementation backed by [`FakeProvider`].
///
/// Supports the full search/send surface so instructions and the
/// [`crate::simulation::TransactionSimulator`] can run without a node:
/// `send_transaction` consumes inputs and indexes outputs, `get_cells`
/// honors `SearchMode::{Exact,Prefix,Partial}`.
#[derive(Clone)]
pub struct FakeRpcClient {
    inner: Arc<Mutex<FakeProvider>>,
    /// Reported network. Defaults to `Fake`; tests targeting the secp256k1
    /// sighash cell dep (which needs a real network) can set `Testnet`.
    pub network: Network,
}

impl Default for FakeRpcClient {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(FakeProvider::default())),
            network: Network::Fake,
        }
    }
}

impl FakeRpcClient {
    fn lock(&self) -> std::sync::MutexGuard<'_, FakeProvider> {
        self.inner.lock().expect("fake rpc mutex")
    }

    /// Clone of the current in-memory state (for assertions in tests).
    pub fn provider(&self) -> FakeProvider {
        self.lock().clone()
    }

    /// Override the reported network.
    pub fn set_network(&mut self, network: Network) -> &mut Self {
        self.network = network;
        self
    }

    /// Set the reported tip number and header.
    pub fn set_fake_tip(&mut self, tip_number: u64, tip_header: HeaderView) -> &mut Self {
        let mut p = self.lock();
        p.fake_tipnumber = tip_number;
        p.fake_tipheader = tip_header;
        drop(p);
        self
    }

    /// Seed an indexer `get_transactions` search result.
    pub fn insert_fake_tx(&mut self, tx: json::Tx) -> &mut Self {
        self.lock().fake_txs.push(tx);
        self
    }

    /// Add a live fake cell. With `header`, also records the committing
    /// transaction/header so header deps and DAO `since` checks resolve.
    /// No-op if a cell already exists at `out_point`.
    pub fn insert_fake_cell(
        &mut self,
        out_point: packed::OutPoint,
        cell: CellOutputEx,
        header: Option<core::HeaderView>,
    ) -> &mut Self {
        let out_point: OutPoint = out_point.into();
        let exists = self.lock().fake_cells.iter().any(|(v, _)| v == &out_point);
        if exists {
            return self;
        }
        self.lock().fake_cells.push((out_point.clone(), cell));
        if let Some(header) = header {
            let tx_hash = out_point.tx_hash.clone();
            self.insert_fake_transaction(
                tx_hash,
                header.hash().unpack(),
                header.number(),
                Default::default(),
            )
            .insert_fake_header(header.clone());
            self.lock().fake_outpoint_headers.insert(out_point, header);
        }
        self
    }

    /// Record a committed transaction (hash → status + body).
    pub fn insert_fake_transaction(
        &mut self,
        tx_hash: H256,
        block_hash: H256,
        block_number: u64,
        tx: Transaction,
    ) -> &mut Self {
        self.lock().fake_transaction.insert(
            tx_hash,
            (
                TxStatus {
                    status: Status::Committed,
                    block_hash: Some(block_hash),
                    block_number: Some(block_number.into()),
                    reason: None,
                    tx_index: None,
                },
                tx,
            ),
        );
        self
    }

    /// Record a header, indexed by its hash.
    pub fn insert_fake_header(&mut self, header: core::HeaderView) -> &mut Self {
        self.lock()
            .fake_headers
            .insert(header.hash().unpack(), header.into());
        self
    }

    /// All out-point → committing-header links, for
    /// [`crate::simulation::TransactionSimulator::link_cell_to_header`].
    pub fn get_outpoint_to_headers(&self) -> Vec<(packed::OutPoint, core::HeaderView)> {
        self.lock()
            .fake_outpoint_headers
            .iter()
            .map(|(k, v)| (k.clone().into(), v.clone()))
            .collect()
    }
}

fn block_from_header(header: HeaderView) -> BlockView {
    BlockView {
        header,
        ..Default::default()
    }
}

fn apply_sent_transaction(provider: &mut FakeProvider, tx: &Transaction, hash: &H256) {
    for input in &tx.inputs {
        provider
            .fake_cells
            .retain(|(op, _)| op != &input.previous_output);
    }
    for (i, (output, data)) in tx.outputs.iter().zip(tx.outputs_data.iter()).enumerate() {
        let out_point = OutPoint {
            tx_hash: hash.clone(),
            index: (i as u32).into(),
        };
        let packed_output: packed::CellOutput = output.clone().into();
        let cell = CellOutputEx::new(packed_output, data.as_bytes().to_vec());
        provider.fake_cells.push((out_point, cell));
    }
    provider.fake_transaction.insert(
        hash.clone(),
        (
            TxStatus {
                status: Status::Committed,
                block_hash: None,
                block_number: None,
                reason: None,
                tx_index: None,
            },
            tx.clone(),
        ),
    );
}

impl Host for FakeRpcClient {
    fn url(&self) -> (String, String) {
        ("fake://ckb".into(), "fake://indexer".into())
    }

    fn get_blockchain_info(&self) -> Rpc<ChainInfo> {
        let info = ChainInfo {
            chain: "ckb_fake".into(),
            median_time: Default::default(),
            epoch: Default::default(),
            difficulty: Default::default(),
            is_initial_block_download: false,
            alerts: vec![],
        };
        Box::pin(async move { Ok(info) })
    }

    fn get_block_by_number(&self, number: BlockNumber) -> Rpc<Option<BlockView>> {
        let block = self
            .lock()
            .get_header_by_number(number.into())
            .map(block_from_header);
        Box::pin(async move { Ok(block) })
    }

    fn get_block(&self, hash: &H256) -> Rpc<Option<BlockView>> {
        let block = self.lock().get_header_by_hash(hash).map(block_from_header);
        Box::pin(async move { Ok(block) })
    }

    fn get_transaction(&self, hash: &H256) -> Rpc<Option<TransactionWithStatusResponse>> {
        let transaction = self.lock().get_transaction_by_hash(hash);
        Box::pin(async move { Ok(transaction) })
    }

    fn send_transaction(
        &self,
        tx: Transaction,
        _outputs_validator: Option<OutputsValidator>,
    ) -> Rpc<H256> {
        let packed_tx: packed::Transaction = tx.clone().into();
        let hash: H256 = packed_tx.into_view().hash().unpack();
        apply_sent_transaction(&mut self.lock(), &tx, &hash);
        Box::pin(async move { Ok(hash) })
    }
}

impl Node for FakeRpcClient {
    fn network(&self) -> Network {
        self.network.clone()
    }

    fn get_live_cell(
        &self,
        out_point: &packed::OutPoint,
        with_data: bool,
    ) -> error::Result<LiveCell> {
        let json_op: OutPoint = out_point.clone().into();
        let cell = self
            .lock()
            .fake_cells
            .iter()
            .find(|(op, _)| op == &json_op)
            .map(|(_, cell)| cell.clone())
            .ok_or_else(|| CalculatorError::InputCellNotFound("live cell not found".into()))?;
        Ok(LiveCell {
            output: cell.output,
            output_data: if with_data { cell.data } else { Vec::new() },
            out_point: out_point.clone(),
            block_number: 0,
            tx_index: 0,
        })
    }

    fn get_header(&self, hash: &Hash256) -> error::Result<Option<packed::Header>> {
        Ok(self
            .lock()
            .get_header_by_hash(&hash_to_h256(hash))
            .map(|h| h.inner.into()))
    }

    fn get_header_by_number(&self, number: u64) -> error::Result<Option<packed::Header>> {
        Ok(self
            .lock()
            .get_header_by_number(number)
            .map(|h| h.inner.into()))
    }

    fn get_tip_header(&self) -> error::Result<packed::Header> {
        Ok(self.lock().fake_tipheader.inner.clone().into())
    }

    fn get_block_hash(&self, number: u64) -> error::Result<Option<Hash256>> {
        Ok(self
            .lock()
            .get_header_by_number(number)
            .map(|h| h256_to_hash(&h.hash)))
    }

    fn get_tip_block_number(&self) -> error::Result<u64> {
        Ok(self.lock().fake_tipnumber)
    }

    fn get_transaction_block_hash(&self, tx_hash: &Hash256) -> error::Result<Option<Hash256>> {
        Ok(self
            .lock()
            .get_transaction_by_hash(&hash_to_h256(tx_hash))
            .and_then(|tx| tx.tx_status.block_hash)
            .map(|h| h256_to_hash(&h)))
    }

    fn min_fee_rate(&self) -> error::Result<u64> {
        Ok(self.lock().fake_feerate)
    }
}

impl Indexer for FakeRpcClient {
    fn get_cells(
        &self,
        search_key: &SearchKey,
        limit: u32,
        cursor: Option<&[u8]>,
    ) -> error::Result<Pagination<LiveCell>> {
        let (cells, next) = self
            .lock()
            .get_cells_by_search_key(search_key, limit as usize, cursor);
        Ok(Pagination {
            objects: cells,
            last_cursor: next.to_le_bytes().to_vec(),
        })
    }

    fn get_transactions(
        &self,
        _search_key: &SearchKey,
        _limit: u32,
        _cursor: Option<&[u8]>,
    ) -> error::Result<Pagination<Tx>> {
        Ok(Pagination {
            objects: self
                .lock()
                .fake_txs
                .clone()
                .into_iter()
                .map(json::Tx::into_kernel)
                .collect(),
            last_cursor: Vec::new(),
        })
    }
}

impl RPC for FakeRpcClient {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::Host;

    #[tokio::test]
    async fn fake_rpc_url_and_chain_info() {
        let rpc = FakeRpcClient::default();
        assert_eq!(rpc.url().0, "fake://ckb");
        let info = rpc.get_blockchain_info().await.unwrap();
        assert_eq!(info.chain, "ckb_fake");
    }

    #[tokio::test]
    async fn fake_rpc_send_transaction_indexes_outputs() {
        let rpc = FakeRpcClient::default();
        let tx = Transaction::default();
        let hash = rpc.send_transaction(tx, None).await.unwrap();
        assert!(rpc.get_transaction(&hash).await.unwrap().is_some());
    }
}
