use std::{
    fmt::Display,
    future::Future,
    pin::Pin,
    str::FromStr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use ckb_jsonrpc_types::{
    BlockNumber, BlockView, CellWithStatus, HeaderView, JsonBytes, OutPoint, OutputsValidator,
    Transaction, TransactionWithStatusResponse, TxPoolInfo, Uint32,
};
use ckb_sdk::rpc::ckb_indexer::{Cell, Order, Pagination, SearchKey};
use ckb_types::H256;
use eyre::{eyre, Error};
use jsonrpc_core::{futures::FutureExt, response::Output};
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
                .map_err::<Error, _>(|e| eyre!("bad ckb request url: {}", e))?;
            let output = resp
                .json::<Output>()
                .await
                .map_err::<Error, _>(|e| eyre!("failed to parse json response: {}", e))?;

            match output {
                Output::Success(success) => {
                    Ok(serde_json::from_value::<$return>(success.result).unwrap())
                }
                Output::Failure(e) => {
                    Err(eyre!("failed to get response from ckb rpc: {:?}", e))
                }
            }
        }
    }}
}

#[derive(PartialEq, Eq, Clone)]
pub enum Network {
    Mainnet,
    Testnet,
    Custom(Url),
    Fake,
}

impl Display for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Network::Mainnet => write!(f, "mainnet"),
            Network::Testnet => write!(f, "testnet"),
            Network::Fake => write!(f, "fake"),
            Network::Custom(url) => write!(f, "{}", url),
        }
    }
}

impl FromStr for Network {
    type Err = eyre::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "mainnet" => Ok(Network::Mainnet),
            "testnet" => Ok(Network::Testnet),
            "fake" => Ok(Network::Fake),
            _ => Ok(Network::Custom(value.parse()?)),
        }
    }
}

#[allow(clippy::upper_case_acronyms)]
pub trait RPC: Clone + Send + Sync {
    fn network(&self) -> Network {
        Network::Fake
    }
    fn url(&self) -> (String, String);
    fn get_live_cell(&self, out_point: &OutPoint, with_data: bool) -> Rpc<CellWithStatus>;
    fn get_cells(
        &self,
        search_key: SearchKey,
        limit: u32,
        cursor: Option<JsonBytes>,
    ) -> Rpc<Pagination<Cell>>;
    fn get_block_by_number(&self, number: BlockNumber) -> Rpc<Option<BlockView>>;
    fn get_block(&self, hash: &H256) -> Rpc<Option<BlockView>>;
    fn get_header(&self, hash: &H256) -> Rpc<Option<HeaderView>>;
    fn get_header_by_number(&self, number: BlockNumber) -> Rpc<Option<HeaderView>>;
    fn get_block_hash(&self, number: BlockNumber) -> Rpc<Option<H256>>;
    fn get_tip_block_number(&self) -> Rpc<BlockNumber>;
    fn get_tip_header(&self) -> Rpc<HeaderView>;
    fn tx_pool_info(&self) -> Rpc<TxPoolInfo>;
    fn get_transaction(&self, hash: &H256) -> Rpc<Option<TransactionWithStatusResponse>>;
    fn send_transaction(
        &self,
        tx: Transaction,
        outputs_validator: Option<OutputsValidator>,
    ) -> Rpc<H256>;
}

#[derive(Clone)]
pub struct RpcClient {
    network: Network,
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
            network: Network::Custom(ckb_uri.clone()),
            raw: Client::new(),
            ckb_uri,
            indexer_uri,
            id: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn new_mainnet() -> Self {
        let mut rpc = RpcClient::new(MAINNET_RPC_URL, None);
        rpc.network = Network::Mainnet;
        rpc
    }

    pub fn new_testnet() -> Self {
        let mut rpc = RpcClient::new(TESTNET_RPC_URL, None);
        rpc.network = Network::Testnet;
        rpc
    }
}

impl RPC for RpcClient {
    fn network(&self) -> Network {
        self.network.clone()
    }

    fn url(&self) -> (String, String) {
        (self.ckb_uri.to_string(), self.indexer_uri.to_string())
    }

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

    fn get_block(&self, hash: &H256) -> Rpc<Option<BlockView>> {
        jsonrpc!("get_block", Target::CKB, self, Option<BlockView>, hash).boxed()
    }

    fn get_header(&self, hash: &H256) -> Rpc<Option<HeaderView>> {
        jsonrpc!("get_header", Target::CKB, self, Option<HeaderView>, hash).boxed()
    }

    fn get_header_by_number(&self, number: BlockNumber) -> Rpc<Option<HeaderView>> {
        jsonrpc!(
            "get_header_by_number",
            Target::CKB,
            self,
            Option<HeaderView>,
            number
        )
        .boxed()
    }

    fn get_block_hash(&self, number: BlockNumber) -> Rpc<Option<H256>> {
        jsonrpc!("get_block_hash", Target::CKB, self, Option<H256>, number).boxed()
    }

    fn get_tip_block_number(&self) -> Rpc<BlockNumber> {
        jsonrpc!("get_tip_block_number", Target::CKB, self, BlockNumber).boxed()
    }

    fn get_tip_header(&self) -> Rpc<HeaderView> {
        jsonrpc!("get_tip_header", Target::CKB, self, HeaderView).boxed()
    }

    fn tx_pool_info(&self) -> Rpc<TxPoolInfo> {
        jsonrpc!("tx_pool_info", Target::CKB, self, TxPoolInfo).boxed()
    }

    fn get_transaction(&self, hash: &H256) -> Rpc<Option<TransactionWithStatusResponse>> {
        jsonrpc!(
            "get_transaction",
            Target::CKB,
            self,
            Option<TransactionWithStatusResponse>,
            hash
        )
        .boxed()
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

pub type Filter = Box<dyn Fn(&Cell) -> bool + Send + Sync>;

/// A wrapper of get_cells rpc call, it will automatically cross over live cells in interation
pub struct GetCellsIter<'a, T: RPC> {
    rpc: &'a T,
    search_key: SearchKey,
    cursor: Option<JsonBytes>,
    filter: Option<Filter>,
}

impl<'a, T: RPC> GetCellsIter<'a, T> {
    pub fn new(rpc: &'a T, search_key: SearchKey) -> Self {
        GetCellsIter {
            rpc,
            search_key,
            cursor: None,
            filter: None,
        }
    }

    pub fn filter(mut self, filter: Filter) -> Self {
        self.filter = Some(filter);
        self
    }

    pub async fn next_batch(&mut self, limit: u32) -> eyre::Result<Option<Vec<Cell>>> {
        let cells = self
            .rpc
            .get_cells(self.search_key.clone(), limit, self.cursor.clone())
            .await?;
        let objects = if let Some(filter) = &self.filter {
            cells.objects.into_iter().filter(filter).collect()
        } else {
            cells.objects
        };
        if objects.is_empty() {
            return Ok(None);
        }
        self.cursor = Some(cells.last_cursor);
        Ok(Some(objects))
    }

    pub async fn next(&mut self) -> eyre::Result<Option<Cell>> {
        Ok(self.next_batch(1).await?.map(|v| v[0].clone()))
    }
}
