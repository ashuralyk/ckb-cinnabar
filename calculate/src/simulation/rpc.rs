use std::collections::HashMap;

use ckb_jsonrpc_types::{
    BlockNumber, BlockView, CellData, CellInfo, CellWithStatus, HeaderView, JsonBytes, OutPoint,
    OutputsValidator, Status, Transaction, TransactionWithStatusResponse, TxPoolInfo, TxStatus,
};
use ckb_sdk::rpc::ckb_indexer::{Cell, Pagination, ScriptType, SearchKey, SearchMode};
use ckb_types::{core, packed, prelude::Unpack, H256};
use eyre::eyre;

use crate::{
    rpc::{Rpc, RPC},
    skeleton::CellOutputEx,
};

#[derive(Default, Clone)]
pub struct FakeProvider {
    pub fake_cells: Vec<(OutPoint, CellOutputEx)>,
    pub fake_headers: HashMap<H256, HeaderView>,
    pub fake_transaction_status: HashMap<H256, TxStatus>,
    pub fake_feerate: u64,
    pub fake_tipnumber: u64,
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
                    panic!("partial search mode is not supported");
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
        self.fake_transaction_status
            .get(hash)
            .map(|status| TransactionWithStatusResponse {
                transaction: None,
                cycles: None,
                time_added_to_pool: None,
                fee: None,
                min_replace_fee: None,
                tx_status: status.clone(),
            })
    }
}

#[derive(Clone, Default)]
pub struct FakeRpcClient {
    pub fake_provider: FakeProvider,
}

impl FakeRpcClient {
    pub fn insert_fake_cell(
        &mut self,
        out_point: packed::OutPoint,
        cell: CellOutputEx,
    ) -> &mut Self {
        self.fake_provider.fake_cells.push((out_point.into(), cell));
        self
    }

    pub fn insert_tx_status(
        &mut self,
        hash: H256,
        block_hash: H256,
        block_number: u64,
    ) -> &mut Self {
        self.fake_provider.fake_transaction_status.insert(
            hash,
            TxStatus {
                status: Status::Committed,
                block_hash: Some(block_hash),
                block_number: Some(block_number.into()),
                reason: None,
            },
        );
        self
    }

    pub fn insert_fake_header(&mut self, header: core::HeaderView) -> &mut Self {
        self.fake_provider
            .fake_headers
            .insert(header.hash().unpack(), header.into());
        self
    }
}

unsafe impl Send for FakeRpcClient {}
unsafe impl Sync for FakeRpcClient {}

impl RPC for FakeRpcClient {
    fn url(&self) -> (String, String) {
        unimplemented!("fake url method")
    }

    fn get_live_cell(&self, out_point: &OutPoint, _with_data: bool) -> Rpc<CellWithStatus> {
        let cell = self
            .fake_provider
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
            self.fake_provider
                .get_cells_by_search_key(search_key, limit as usize, cursor);
        let result = Pagination::<Cell> {
            objects: cells,
            last_cursor: JsonBytes::from_vec(cursor.to_le_bytes().to_vec()),
        };
        Box::pin(async move { Ok(result) })
    }

    fn get_block_by_number(&self, _number: BlockNumber) -> Rpc<Option<BlockView>> {
        unimplemented!("fake get_block_by_number method")
    }

    fn get_block(&self, _hash: &H256) -> Rpc<Option<BlockView>> {
        unimplemented!("fake get_block method")
    }

    fn get_header(&self, hash: &H256) -> Rpc<Option<HeaderView>> {
        let header = self.fake_provider.get_header_by_hash(hash);
        Box::pin(async move { Ok(header) })
    }

    fn get_header_by_number(&self, number: BlockNumber) -> Rpc<Option<HeaderView>> {
        let header = self.fake_provider.get_header_by_number(number.into());
        Box::pin(async move { Ok(header) })
    }

    fn get_block_hash(&self, _number: BlockNumber) -> Rpc<Option<H256>> {
        unimplemented!("fake get_block_hash method")
    }

    fn get_tip_block_number(&self) -> Rpc<BlockNumber> {
        let tip_number = self.fake_provider.fake_tipnumber;
        Box::pin(async move { Ok(tip_number.into()) })
    }

    fn get_tip_header(&self) -> Rpc<HeaderView> {
        unimplemented!("fake get_tip_header method")
    }

    fn tx_pool_info(&self) -> Rpc<TxPoolInfo> {
        let pool = TxPoolInfo {
            min_fee_rate: self.fake_provider.fake_feerate.into(),
            ..Default::default()
        };
        Box::pin(async move { Ok(pool) })
    }

    fn get_transaction(&self, hash: &H256) -> Rpc<Option<TransactionWithStatusResponse>> {
        let transaction = self.fake_provider.get_transaction_by_hash(hash);
        Box::pin(async move { Ok(transaction) })
    }

    fn send_transaction(
        &self,
        _tx: Transaction,
        _outputs_validator: Option<OutputsValidator>,
    ) -> Rpc<H256> {
        unimplemented!("fake send_transaction method")
    }
}
