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

#[derive(Clone)]
pub struct FakeRpcClient {}

impl RPC for FakeRpcClient {
    fn get_live_cell(&self, _out_point: &OutPoint, _with_data: bool) -> Rpc<CellWithStatus> {
        unimplemented!("fake get_live_cell method")
    }

    fn get_cells(
        &self,
        _search_key: SearchKey,
        _limit: u32,
        _cursor: Option<JsonBytes>,
    ) -> Rpc<Pagination<Cell>> {
        unimplemented!("fake get_cells method")
    }

    fn get_block_by_number(&self, _number: BlockNumber) -> Rpc<Option<BlockView>> {
        unimplemented!("fake get_block_by_number method")
    }

    fn tx_pool_info(&self) -> Rpc<TxPoolInfo> {
        unimplemented!("fake tx_pool_info method")
    }

    fn send_transaction(
        &self,
        _tx: Transaction,
        _outputs_validator: Option<OutputsValidator>,
    ) -> Rpc<H256> {
        unimplemented!("fake send_transaction method")
    }
}
