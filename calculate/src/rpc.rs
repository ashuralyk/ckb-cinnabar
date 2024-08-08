use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use ckb_jsonrpc_types::{
    BlockNumber, BlockView, CellWithStatus, JsonBytes, OutPoint, OutputsValidator, Transaction,
    TxPoolInfo, Uint32,
};
use ckb_sdk::rpc::ckb_indexer::{Cell, Order, Pagination, SearchKey};
use ckb_types::H256;
use eyre::{eyre, Error};
use jsonrpc_core::futures::FutureExt;
use jsonrpc_core::response::Output;
use reqwest::{Client, Url};

pub type Rpc<T> = Pin<Box<dyn Future<Output = Result<T, Error>> + Send + 'static>>;

pub const MAINNET_RPC_URL: &str = "https://mainnet.ckb.dev";
pub const TESTNET_RPC_URL: &str = "https://testnet.ckbapp.dev";

#[allow(clippy::upper_case_acronyms)]
enum Target {
    CKB,
    Indexer,
}

macro_rules! jsonrpc {
    ($method:expr, $id:expr, $self:ident, $return:ty$(, $params:ident$(,)?)*) => {{
        let data = format!(
            r#"{{"id": {}, "jsonrpc": "2.0", "method": "{}", "params": {}}}"#,
            $self.id.load(Ordering::Relaxed),
            $method,
            serde_json::to_value(($($params,)*)).unwrap()
        );
        $self.id.fetch_add(1, Ordering::Relaxed);

        let req_json: serde_json::Value = serde_json::from_str(&data).unwrap();

        let url = match $id {
            Target::CKB => $self.ckb_uri.clone(),
            Target::Indexer => $self.indexer_uri.clone(),
        };
        let c = $self.raw.post(url).json(&req_json);
        async {
            let resp = c
                .send()
                .await
                .map_err::<Error, _>(|_| eyre!("bad ckb request url"))?;
            let output = resp
                .json::<Output>()
                .await
                .map_err::<Error, _>(|_| eyre!("failed to parse json response"))?;

            match output {
                Output::Success(success) => {
                    Ok(serde_json::from_value::<$return>(success.result).unwrap())
                }
                Output::Failure(_) => {
                    Err(eyre!("failed to get response from ckb rpc"))
                }
            }
        }
    }}
}

#[allow(clippy::upper_case_acronyms)]
pub trait RPC: Clone + Send + Sync {
    fn get_live_cell(&self, out_point: &OutPoint, with_data: bool) -> Rpc<CellWithStatus>;
    fn get_cells(
        &self,
        search_key: SearchKey,
        limit: u32,
        cursor: Option<JsonBytes>,
    ) -> Rpc<Pagination<Cell>>;
    fn get_block_by_number(&self, number: BlockNumber) -> Rpc<Option<BlockView>>;
    fn tx_pool_info(&self) -> Rpc<TxPoolInfo>;
    fn send_transaction(
        &self,
        tx: Transaction,
        outputs_validator: Option<OutputsValidator>,
    ) -> Rpc<H256>;
}

#[derive(Clone)]
pub struct FakeRpcClient {}

impl RPC for FakeRpcClient {
    fn get_live_cell(&self, _out_point: &OutPoint, _with_data: bool) -> Rpc<CellWithStatus> {
        unimplemented!()
    }
    fn get_cells(
        &self,
        _search_key: SearchKey,
        _limit: u32,
        _cursor: Option<JsonBytes>,
    ) -> Rpc<Pagination<Cell>> {
        unimplemented!()
    }
    fn get_block_by_number(&self, _number: BlockNumber) -> Rpc<Option<BlockView>> {
        unimplemented!()
    }
    fn tx_pool_info(&self) -> Rpc<TxPoolInfo> {
        unimplemented!()
    }
    fn send_transaction(
        &self,
        _tx: Transaction,
        _outputs_validator: Option<OutputsValidator>,
    ) -> Rpc<H256> {
        unimplemented!()
    }
}

#[derive(Clone)]
pub struct RpcClient {
    raw: Client,
    ckb_uri: Url,
    indexer_uri: Url,
    id: Arc<AtomicU64>,
}

impl RpcClient {
    pub fn new(ckb_uri: &str, indexer_uri: Option<&str>) -> Self {
        let indexer_uri = Url::parse(indexer_uri.unwrap_or(ckb_uri))
            .expect("ckb uri, e.g. \"http://127.0.0.1:8116\"");
        let ckb_uri = Url::parse(ckb_uri).expect("ckb uri, e.g. \"http://127.0.0.1:8114\"");

        RpcClient {
            raw: Client::new(),
            ckb_uri,
            indexer_uri,
            id: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn new_mainnet() -> Self {
        RpcClient::new(MAINNET_RPC_URL, None)
    }

    pub fn new_testnet() -> Self {
        RpcClient::new(TESTNET_RPC_URL, None)
    }
}

impl RPC for RpcClient {
    fn get_live_cell(&self, out_point: &OutPoint, with_data: bool) -> Rpc<CellWithStatus> {
        jsonrpc!(
            "get_live_cell",
            Target::CKB,
            self,
            CellWithStatus,
            out_point,
            with_data
        )
        .boxed()
    }

    fn get_cells(
        &self,
        search_key: SearchKey,
        limit: u32,
        cursor: Option<JsonBytes>,
    ) -> Rpc<Pagination<Cell>> {
        let order = Order::Asc;
        let limit = Uint32::from(limit);

        jsonrpc!(
            "get_cells",
            Target::Indexer,
            self,
            Pagination<Cell>,
            search_key,
            order,
            limit,
            cursor,
        )
        .boxed()
    }

    fn get_block_by_number(&self, number: BlockNumber) -> Rpc<Option<BlockView>> {
        jsonrpc!(
            "get_block_by_number",
            Target::CKB,
            self,
            Option<BlockView>,
            number
        )
        .boxed()
    }

    fn tx_pool_info(&self) -> Rpc<TxPoolInfo> {
        jsonrpc!("tx_pool_info", Target::CKB, self, TxPoolInfo).boxed()
    }

    fn send_transaction(
        &self,
        tx: Transaction,
        outputs_validator: Option<OutputsValidator>,
    ) -> Rpc<H256> {
        jsonrpc!(
            "send_transaction",
            Target::CKB,
            self,
            H256,
            tx,
            outputs_validator
        )
        .boxed()
    }
}

/// A wrapper of get_cells rpc call, it will automatically cross over live cells in interation
pub struct GetCellsIter<'a, T: RPC> {
    rpc: &'a T,
    search_key: SearchKey,
    cursor: Option<JsonBytes>,
}

impl<'a, T: RPC> GetCellsIter<'a, T> {
    pub fn new(rpc: &'a T, search_key: SearchKey) -> Self {
        GetCellsIter {
            rpc,
            search_key,
            cursor: None,
        }
    }

    pub async fn next_batch(&mut self, limit: u32) -> eyre::Result<Option<Vec<Cell>>> {
        let cells = self
            .rpc
            .get_cells(self.search_key.clone(), limit, self.cursor.clone())
            .await?;
        if cells.objects.is_empty() {
            return Ok(None);
        }
        self.cursor = Some(cells.last_cursor);
        Ok(Some(cells.objects))
    }

    pub async fn next(&mut self) -> eyre::Result<Option<Cell>> {
        Ok(self.next_batch(1).await?.map(|v| v[0].clone()))
    }
}
