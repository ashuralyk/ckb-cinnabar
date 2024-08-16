use std::sync::Arc;

use ckb_jsonrpc_types::{
    BlockNumber, BlockView, CellWithStatus, HeaderView, JsonBytes, OutPoint, OutputsValidator,
    Transaction, TransactionWithStatusResponse, TxPoolInfo,
};
use ckb_sdk::rpc::ckb_indexer::{Cell, Pagination, SearchKey};
use ckb_types::H256;

use crate::rpc::{Rpc, RPC};

type FnGetLiveCell = Box<dyn Fn(OutPoint, bool) -> CellWithStatus + Send + Sync>;
type FnGetCells = Box<dyn Fn(SearchKey, u32, Option<JsonBytes>) -> Pagination<Cell> + Send + Sync>;
type FnGetBlockByNumber = Box<dyn Fn(BlockNumber) -> Option<BlockView> + Send + Sync>;
type FnGetBlock = Box<dyn Fn(H256) -> Option<BlockView> + Send + Sync>;
type FnGetHeader = Box<dyn Fn(H256) -> Option<HeaderView> + Send + Sync>;
type FnGetHeaderByNumber = Box<dyn Fn(BlockNumber) -> Option<HeaderView> + Send + Sync>;
type FnGetTipBlockNumber = Box<dyn Fn() -> BlockNumber + Send + Sync>;
type FnGetTipHeader = Box<dyn Fn() -> HeaderView + Send + Sync>;
type FnTxPoolInfo = Box<dyn Fn() -> TxPoolInfo + Send + Sync>;
type FnGetTransaction = Box<dyn Fn(H256) -> Option<TransactionWithStatusResponse> + Send + Sync>;
type FnSendTransaction = Box<dyn Fn(Transaction, Option<OutputsValidator>) -> H256 + Send + Sync>;

#[derive(Clone, Default)]
pub struct FakeRpcClient {
    pub method_get_live_cell: Option<Arc<FnGetLiveCell>>,
    pub method_get_cells: Option<Arc<FnGetCells>>,
    pub method_get_block_by_number: Option<Arc<FnGetBlockByNumber>>,
    pub method_get_block: Option<Arc<FnGetBlock>>,
    pub method_get_header: Option<Arc<FnGetHeader>>,
    pub method_get_header_by_number: Option<Arc<FnGetHeaderByNumber>>,
    pub method_get_tip_block_number: Option<Arc<FnGetTipBlockNumber>>,
    pub method_get_tip_header: Option<Arc<FnGetTipHeader>>,
    pub method_tx_pool_info: Option<Arc<FnTxPoolInfo>>,
    pub method_get_transaction: Option<Arc<FnGetTransaction>>,
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

    fn get_block(&self, hash: &H256) -> Rpc<Option<BlockView>> {
        let Some(get_block) = self.method_get_block.clone() else {
            unimplemented!("fake get_block method")
        };
        let hash = hash.clone();
        Box::pin(async move { Ok(get_block(hash)) })
    }

    fn get_header(&self, hash: &H256) -> Rpc<Option<HeaderView>> {
        let Some(get_header) = self.method_get_header.clone() else {
            unimplemented!("fake get_header method")
        };
        let hash = hash.clone();
        Box::pin(async move { Ok(get_header(hash)) })
    }

    fn get_header_by_number(&self, number: BlockNumber) -> Rpc<Option<HeaderView>> {
        let Some(get_header_by_number) = self.method_get_header_by_number.clone() else {
            unimplemented!("fake get_header_by_number method")
        };
        Box::pin(async move { Ok(get_header_by_number(number)) })
    }

    fn get_tip_block_number(&self) -> Rpc<BlockNumber> {
        let Some(get_tip_block_number) = self.method_get_tip_block_number.clone() else {
            unimplemented!("fake get_tip_block_number method")
        };
        Box::pin(async move { Ok(get_tip_block_number()) })
    }

    fn get_tip_header(&self) -> Rpc<HeaderView> {
        let Some(get_tip_header) = self.method_get_tip_header.clone() else {
            unimplemented!("fake get_tip_header method")
        };
        Box::pin(async move { Ok(get_tip_header()) })
    }

    fn tx_pool_info(&self) -> Rpc<TxPoolInfo> {
        let Some(tx_pool_info) = self.method_tx_pool_info.clone() else {
            let pool = TxPoolInfo {
                min_fee_rate: 1000.into(),
                ..Default::default()
            };
            return Box::pin(async move { Ok(pool) });
        };
        Box::pin(async move { Ok(tx_pool_info()) })
    }

    fn get_transaction(&self, hash: &H256) -> Rpc<Option<TransactionWithStatusResponse>> {
        let Some(get_transaction) = self.method_get_transaction.clone() else {
            unimplemented!("fake get_transaction method")
        };
        let hash = hash.clone();
        Box::pin(async move { Ok(get_transaction(hash)) })
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
