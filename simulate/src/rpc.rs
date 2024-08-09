use std::sync::Arc;

use ckb_cinnabar_calculator::{
    re_exports::{
        ckb_jsonrpc_types::{
            BlockNumber, BlockView, CellWithStatus, JsonBytes, OutPoint, OutputsValidator,
            Transaction, TxPoolInfo,
        },
        ckb_sdk::rpc::ckb_indexer::{Cell, Pagination, SearchKey},
        ckb_types::H256,
    },
    rpc::{Rpc, RPC},
};

type FnGetLiveCell = Box<dyn Fn(OutPoint, bool) -> CellWithStatus + Send + Sync>;
type FnGetCells = Box<dyn Fn(SearchKey, u32, Option<JsonBytes>) -> Pagination<Cell> + Send + Sync>;
type FnGetBlockByNumber = Box<dyn Fn(BlockNumber) -> Option<BlockView> + Send + Sync>;
type FnTxPoolInfo = Box<dyn Fn() -> TxPoolInfo + Send + Sync>;
type FnSendTransaction = Box<dyn Fn(Transaction, Option<OutputsValidator>) -> H256 + Send + Sync>;

#[derive(Clone, Default)]
pub struct FakeRpcClient {
    pub method_get_live_cell: Option<Arc<FnGetLiveCell>>,
    pub method_get_cells: Option<Arc<FnGetCells>>,
    pub method_get_block_by_number: Option<Arc<FnGetBlockByNumber>>,
    pub method_tx_pool_info: Option<Arc<FnTxPoolInfo>>,
    pub method_send_transaction: Option<Arc<FnSendTransaction>>,
}

unsafe impl Send for FakeRpcClient {}
unsafe impl Sync for FakeRpcClient {}

impl RPC for FakeRpcClient {
    fn url(&self) -> (String, String) {
        unimplemented!("fake url method")
    }

    fn get_live_cell(&self, out_point: &OutPoint, with_data: bool) -> Rpc<CellWithStatus> {
        let Some(get_live_cell) = self.method_get_live_cell.clone() else {
            unimplemented!("fake get_live_cell method")
        };
        let out_point = out_point.clone();
        Box::pin(async move { Ok(get_live_cell(out_point, with_data)) })
    }

    fn get_cells(
        &self,
        search_key: SearchKey,
        limit: u32,
        cursor: Option<JsonBytes>,
    ) -> Rpc<Pagination<Cell>> {
        let Some(get_cells) = self.method_get_cells.clone() else {
            unimplemented!("fake get_cells method")
        };
        Box::pin(async move { Ok(get_cells(search_key, limit, cursor)) })
    }

    fn get_block_by_number(&self, number: BlockNumber) -> Rpc<Option<BlockView>> {
        let Some(get_block_by_number) = self.method_get_block_by_number.clone() else {
            unimplemented!("fake get_block_by_number method")
        };
        Box::pin(async move { Ok(get_block_by_number(number)) })
    }

    fn tx_pool_info(&self) -> Rpc<TxPoolInfo> {
        let Some(tx_pool_info) = self.method_tx_pool_info.clone() else {
            unimplemented!("fake tx_pool_info method")
        };
        Box::pin(async move { Ok(tx_pool_info()) })
    }

    fn send_transaction(
        &self,
        tx: Transaction,
        outputs_validator: Option<OutputsValidator>,
    ) -> Rpc<H256> {
        let Some(send_transaction) = self.method_send_transaction.clone() else {
            unimplemented!("fake send_transaction method")
        };
        Box::pin(async move { Ok(send_transaction(tx, outputs_validator)) })
    }
}
