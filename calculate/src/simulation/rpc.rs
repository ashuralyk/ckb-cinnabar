use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use ckb_jsonrpc_types::{
    BlockNumber, BlockView, CellData, CellInfo, CellWithStatus, ChainInfo, HeaderView, JsonBytes,
    OutPoint, OutputsValidator, ResponseFormat, Status, Transaction, TransactionView,
    TransactionWithStatusResponse, TxPoolInfo, TxStatus,
};
use ckb_types::{
    core, packed,
    prelude::{IntoTransactionView, Unpack},
    H256,
};
use eyre::eyre;

use crate::{
    indexer::{Cell, Pagination, ScriptType, SearchKey, SearchMode, Tx},
    rpc::{Network, Rpc, RPC},
    skeleton::CellOutputEx,
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
    pub fake_txs: Vec<Tx>,
    /// Reported tx-pool min fee rate (shannons/byte).
    pub fake_feerate: u64,
    /// Reported tip block number.
    pub fake_tipnumber: u64,
    /// Reported tip header.
    pub fake_tipheader: HeaderView,
}

fn indexer_cell(out_point: &OutPoint, cell: &CellOutputEx) -> Cell {
    Cell {
        block_number: 0.into(),
        out_point: out_point.clone(),
        output: cell.output.clone().into(),
        tx_index: 0.into(),
        output_data: Some(JsonBytes::from_vec(cell.data.clone())),
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
        search_key: SearchKey,
        limit: usize,
        cursor: Option<JsonBytes>,
    ) -> (Vec<Cell>, usize) {
        if limit == 0 {
            return (vec![], 0);
        }
        let mut offset = cursor
            .map(|v| usize::from_le_bytes(v.into_bytes().to_vec().try_into().unwrap()))
            .unwrap_or_default();
        let mut objects = vec![];
        for (out_point, cell) in self.fake_cells.iter().skip(offset) {
            offset += 1;
            let (primary_script, script_a, secondary_script, script_b) =
                match search_key.script_type {
                    ScriptType::Lock => {
                        let primary_script: packed::Script = search_key.script.clone().into();
                        let secondary_script: Option<Option<packed::Script>> =
                            search_key.filter.clone().map(|v| v.script.map(Into::into));
                        let lock_script = cell.lock_script();
                        let type_script = cell.type_script();
                        (
                            primary_script,
                            Some(lock_script),
                            secondary_script,
                            type_script,
                        )
                    }
                    ScriptType::Type => {
                        let primary_script: packed::Script = search_key.script.clone().into();
                        let secondary_script: Option<Option<packed::Script>> =
                            search_key.filter.clone().map(|v| v.script.map(Into::into));
                        let lock_script = cell.lock_script();
                        let type_script = cell.type_script();
                        (
                            primary_script,
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
                        objects.push(indexer_cell(out_point, cell));
                    }
                }
                Some(SearchMode::Prefix) => {
                    if script_prefix_equal(script_a.as_ref(), Some(&primary_script)) {
                        if let Some(script) = secondary_script {
                            if !script_prefix_equal(script_b.as_ref(), script.as_ref()) {
                                continue;
                            }
                        }
                        objects.push(indexer_cell(out_point, cell))
                    }
                }
                Some(SearchMode::Partial) => {
                    if script_partial_equal(script_a.as_ref(), Some(&primary_script)) {
                        if let Some(script) = secondary_script {
                            if !script_partial_equal(script_b.as_ref(), script.as_ref()) {
                                continue;
                            }
                        }
                        objects.push(indexer_cell(out_point, cell));
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
    pub fn insert_fake_tx(&mut self, tx: Tx) -> &mut Self {
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
        let cell = crate::skeleton::CellOutputEx::new(packed_output, data.as_bytes().to_vec());
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

impl RPC for FakeRpcClient {
    fn network(&self) -> Network {
        self.network.clone()
    }

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

    fn get_live_cell(&self, out_point: &OutPoint, _with_data: bool) -> Rpc<CellWithStatus> {
        let cell = self
            .lock()
            .get_cell_by_outpoint(out_point)
            .ok_or(eyre!("no live cell found"));
        Box::pin(async move { cell })
    }

    fn get_cells(
        &self,
        search_key: SearchKey,
        limit: u32,
        cursor: Option<JsonBytes>,
    ) -> Rpc<Pagination<Cell>> {
        let (cells, cursor) =
            self.lock()
                .get_cells_by_search_key(search_key, limit as usize, cursor);
        let result = Pagination::<Cell> {
            objects: cells,
            last_cursor: JsonBytes::from_vec(cursor.to_le_bytes().to_vec()),
        };
        Box::pin(async move { Ok(result) })
    }

    fn get_transactions(
        &self,
        _search_key: SearchKey,
        _limit: u32,
        _cursor: Option<JsonBytes>,
    ) -> Rpc<Pagination<Tx>> {
        let result = Pagination::<Tx> {
            objects: self.lock().fake_txs.clone(),
            last_cursor: JsonBytes::default(),
        };
        Box::pin(async move { Ok(result) })
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

    fn get_header(&self, hash: &H256) -> Rpc<Option<HeaderView>> {
        let header = self.lock().get_header_by_hash(hash);
        Box::pin(async move { Ok(header) })
    }

    fn get_header_by_number(&self, number: BlockNumber) -> Rpc<Option<HeaderView>> {
        let header = self.lock().get_header_by_number(number.into());
        Box::pin(async move { Ok(header) })
    }

    fn get_block_hash(&self, number: BlockNumber) -> Rpc<Option<H256>> {
        let header = self.lock().get_header_by_number(number.into());
        Box::pin(async move { Ok(header.map(|h| h.hash)) })
    }

    fn get_tip_block_number(&self) -> Rpc<BlockNumber> {
        let tip_number = self.lock().fake_tipnumber;
        Box::pin(async move { Ok(tip_number.into()) })
    }

    fn get_tip_header(&self) -> Rpc<HeaderView> {
        let tip_header = self.lock().fake_tipheader.clone();
        Box::pin(async move { Ok(tip_header) })
    }

    fn tx_pool_info(&self) -> Rpc<TxPoolInfo> {
        let pool = TxPoolInfo {
            min_fee_rate: self.lock().fake_feerate.into(),
            ..Default::default()
        };
        Box::pin(async move { Ok(pool) })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::RPC;

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
