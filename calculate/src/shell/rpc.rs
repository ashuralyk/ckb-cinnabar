//! HTTP adapter for the kernel [`RPC`](crate::kernel::rpc::RPC) surface.
//!
//! [`RpcClient`] talks JSON-RPC over reqwest. Each kernel method `block_on`s
//! one async HTTP region. Host-only methods (send, full blocks, chain info)
//! live on [`Host`].

use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use ckb_jsonrpc_types::{
    BlockNumber, BlockView, CellWithStatus, ChainInfo, HeaderView, JsonBytes, OutPoint,
    OutputsValidator, Transaction, TransactionWithStatusResponse, TxPoolInfo, Uint32,
};
use ckb_types::H256;
use eyre::{eyre, Error};
#[cfg(not(target_arch = "wasm32"))]
use futures::executor::block_on as block_on_future;
use futures::FutureExt;
use reqwest::{Client, Url};
use serde::Deserialize;

#[cfg(not(target_arch = "wasm32"))]
use tokio::{
    runtime::{Builder, Handle, RuntimeFlavor},
    task::block_in_place,
};

use crate::{
    error::{self, CalculatorError},
    indexer::{
        json::{self, Order as JsonOrder},
        Indexer, LiveCell, Order, Pagination, SearchKey, Tx,
    },
    types::{h256_to_hash, hash_to_h256, packed, Hash256},
};

pub use crate::kernel::rpc::{Node, RPC};
pub use crate::network::Network;

#[cfg(target_arch = "wasm32")]
pub type Rpc<T> = Pin<Box<dyn Future<Output = Result<T, Error>>>>;

/// Boxed RPC future. `Send` on native targets so it can cross `tokio` worker
/// threads; plain (non-`Send`) on wasm32 where everything is single-threaded.
#[cfg(not(target_arch = "wasm32"))]
pub type Rpc<T> = Pin<Box<dyn Future<Output = Result<T, Error>> + Send + 'static>>;

/// Public RPC endpoint of CKB mainnet (also serves the indexer API).
pub const MAINNET_RPC_URL: &str = "https://mainnet.ckb.dev";
/// Public RPC endpoint of CKB testnet (also serves the indexer API).
pub const TESTNET_RPC_URL: &str = "https://testnet.ckbapp.dev";

#[derive(Deserialize)]
#[serde(untagged)]
enum Output {
    Success(JsonSuccess),
    Failure(JsonError),
}

#[derive(Deserialize)]
struct JsonSuccess {
    pub result: serde_json::Value,
}

#[derive(Deserialize, Debug)]
struct JsonError {
    pub error: serde_json::Value,
}

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
                    Err(eyre!("failed to get response from ckb rpc: {:?}", e.error))
                }
            }
        }
    }}
}

/// Host-only JSON-RPC methods (submit, full blocks, chain identity).
///
/// Assembly uses [`RPC`]. These stay async because send/wait and genesis
/// walks are HTTP-only.
pub trait Host {
    /// `(ckb_node_url, indexer_url)` pair, used e.g. when shelling out to ckb-cli.
    fn url(&self) -> (String, String);
    /// `get_blockchain_info` — chain identity and sync state.
    fn get_blockchain_info(&self) -> Rpc<ChainInfo>;
    /// `get_block` by height.
    fn get_block_by_number(&self, number: BlockNumber) -> Rpc<Option<BlockView>>;
    /// `get_block` by hash.
    fn get_block(&self, hash: &H256) -> Rpc<Option<BlockView>>;
    /// `get_transaction` with its on-chain status.
    fn get_transaction(&self, hash: &H256) -> Rpc<Option<TransactionWithStatusResponse>>;
    /// `send_transaction`; returns the transaction hash.
    fn send_transaction(
        &self,
        tx: Transaction,
        outputs_validator: Option<OutputsValidator>,
    ) -> Rpc<H256>;
}

/// JSON-RPC client backed by `reqwest`, talking to a CKB node and (optionally
/// separate) ckb-indexer endpoint.
#[derive(Clone)]
pub struct RpcClient {
    network: Network,
    raw: Client,
    ckb_uri: Url,
    indexer_uri: Url,
    id: Arc<AtomicU64>,
}

impl RpcClient {
    /// Create a client for custom endpoints. When `indexer_uri` is `None`, the
    /// CKB node URL is reused for indexer calls (public nodes serve both).
    ///
    /// The initial network is [`Network::Custom`]; call
    /// [`RpcClient::update_network`] to auto-detect mainnet/testnet.
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

    /// Client bound to [`MAINNET_RPC_URL`].
    pub fn new_mainnet() -> Self {
        let mut rpc = RpcClient::new(MAINNET_RPC_URL, None);
        rpc.network = Network::Mainnet;
        rpc
    }

    /// Client bound to [`TESTNET_RPC_URL`].
    pub fn new_testnet() -> Self {
        let mut rpc = RpcClient::new(TESTNET_RPC_URL, None);
        rpc.network = Network::Testnet;
        rpc
    }

    /// Detect the network from on-chain `get_blockchain_info`, resolving
    /// `Network::Custom(url)` to `Mainnet`/`Testnet`. No-op for unknown chains.
    pub async fn update_network(&mut self) -> eyre::Result<()> {
        let chain_info = self.get_blockchain_info().await?;
        match chain_info.chain.as_str() {
            "ckb" => self.network = Network::Mainnet,
            "ckb_testnet" => self.network = Network::Testnet,
            _ => return Ok(()),
        };
        Ok(())
    }

    /// Explicitly set the network (e.g. a configured fallback when detection fails).
    pub fn set_network(&mut self, network: Network) {
        self.network = network;
    }

    fn request_live_cell(&self, out_point: &OutPoint, with_data: bool) -> Rpc<CellWithStatus> {
        let future = jsonrpc!(
            "get_live_cell",
            Target::CKB,
            self,
            CellWithStatus,
            out_point,
            with_data
        );
        #[cfg(not(target_arch = "wasm32"))]
        return future.boxed();
        #[cfg(target_arch = "wasm32")]
        return future.boxed_local();
    }

    fn request_cells(
        &self,
        search_key: json::SearchKey,
        order: Order,
        limit: u32,
        cursor: Option<JsonBytes>,
    ) -> Rpc<json::Pagination<json::Cell>> {
        let order = json_order(order);
        let limit = Uint32::from(limit);
        let future = jsonrpc!(
            "get_cells",
            Target::Indexer,
            self,
            json::Pagination<json::Cell>,
            search_key,
            order,
            limit,
            cursor,
        );
        #[cfg(not(target_arch = "wasm32"))]
        return future.boxed();
        #[cfg(target_arch = "wasm32")]
        return future.boxed_local();
    }

    fn request_transactions(
        &self,
        search_key: json::SearchKey,
        order: Order,
        limit: u32,
        cursor: Option<JsonBytes>,
    ) -> Rpc<json::Pagination<json::Tx>> {
        let order = json_order(order);
        let limit = Uint32::from(limit);
        let future = jsonrpc!(
            "get_transactions",
            Target::Indexer,
            self,
            json::Pagination<json::Tx>,
            search_key,
            order,
            limit,
            cursor,
        );
        #[cfg(not(target_arch = "wasm32"))]
        return future.boxed();
        #[cfg(target_arch = "wasm32")]
        return future.boxed_local();
    }

    fn request_header(&self, hash: &H256) -> Rpc<Option<HeaderView>> {
        let future = jsonrpc!("get_header", Target::CKB, self, Option<HeaderView>, hash);
        #[cfg(not(target_arch = "wasm32"))]
        return future.boxed();
        #[cfg(target_arch = "wasm32")]
        return future.boxed_local();
    }

    fn request_header_by_number(&self, number: BlockNumber) -> Rpc<Option<HeaderView>> {
        let future = jsonrpc!(
            "get_header_by_number",
            Target::CKB,
            self,
            Option<HeaderView>,
            number
        );
        #[cfg(not(target_arch = "wasm32"))]
        return future.boxed();
        #[cfg(target_arch = "wasm32")]
        return future.boxed_local();
    }

    fn request_block_hash(&self, number: BlockNumber) -> Rpc<Option<H256>> {
        let future = jsonrpc!("get_block_hash", Target::CKB, self, Option<H256>, number);
        #[cfg(not(target_arch = "wasm32"))]
        return future.boxed();
        #[cfg(target_arch = "wasm32")]
        return future.boxed_local();
    }

    fn request_tip_block_number(&self) -> Rpc<BlockNumber> {
        let future = jsonrpc!("get_tip_block_number", Target::CKB, self, BlockNumber);
        #[cfg(not(target_arch = "wasm32"))]
        return future.boxed();
        #[cfg(target_arch = "wasm32")]
        return future.boxed_local();
    }

    fn request_tip_header(&self) -> Rpc<HeaderView> {
        let future = jsonrpc!("get_tip_header", Target::CKB, self, HeaderView);
        #[cfg(not(target_arch = "wasm32"))]
        return future.boxed();
        #[cfg(target_arch = "wasm32")]
        return future.boxed_local();
    }

    fn request_tx_pool_info(&self) -> Rpc<TxPoolInfo> {
        let future = jsonrpc!("tx_pool_info", Target::CKB, self, TxPoolInfo);
        #[cfg(not(target_arch = "wasm32"))]
        return future.boxed();
        #[cfg(target_arch = "wasm32")]
        return future.boxed_local();
    }

    fn request_transaction(&self, hash: &H256) -> Rpc<Option<TransactionWithStatusResponse>> {
        let future = jsonrpc!(
            "get_transaction",
            Target::CKB,
            self,
            Option<TransactionWithStatusResponse>,
            hash
        );
        #[cfg(not(target_arch = "wasm32"))]
        return future.boxed();
        #[cfg(target_arch = "wasm32")]
        return future.boxed_local();
    }
}

impl Host for RpcClient {
    fn url(&self) -> (String, String) {
        (self.ckb_uri.to_string(), self.indexer_uri.to_string())
    }

    fn get_blockchain_info(&self) -> Rpc<ChainInfo> {
        let future = jsonrpc!("get_blockchain_info", Target::CKB, self, ChainInfo);
        #[cfg(not(target_arch = "wasm32"))]
        return future.boxed();
        #[cfg(target_arch = "wasm32")]
        return future.boxed_local();
    }

    fn get_block_by_number(&self, number: BlockNumber) -> Rpc<Option<BlockView>> {
        let future = jsonrpc!(
            "get_block_by_number",
            Target::CKB,
            self,
            Option<BlockView>,
            number
        );
        #[cfg(not(target_arch = "wasm32"))]
        return future.boxed();
        #[cfg(target_arch = "wasm32")]
        return future.boxed_local();
    }

    fn get_block(&self, hash: &H256) -> Rpc<Option<BlockView>> {
        let future = jsonrpc!("get_block", Target::CKB, self, Option<BlockView>, hash);
        #[cfg(not(target_arch = "wasm32"))]
        return future.boxed();
        #[cfg(target_arch = "wasm32")]
        return future.boxed_local();
    }

    fn get_transaction(&self, hash: &H256) -> Rpc<Option<TransactionWithStatusResponse>> {
        self.request_transaction(hash)
    }

    fn send_transaction(
        &self,
        tx: Transaction,
        outputs_validator: Option<OutputsValidator>,
    ) -> Rpc<H256> {
        let future = jsonrpc!(
            "send_transaction",
            Target::CKB,
            self,
            H256,
            tx,
            outputs_validator
        );
        #[cfg(not(target_arch = "wasm32"))]
        return future.boxed();
        #[cfg(target_arch = "wasm32")]
        return future.boxed_local();
    }
}

/// Run one async RPC region to completion from a sync facade.
///
/// Multi-thread tokio: `block_in_place` + `Handle::block_on`. Current-thread
/// / tests: `futures::executor::block_on`. No runtime: build a current-thread
/// runtime.
pub fn block_on_rpc<T>(fut: impl Future<Output = eyre::Result<T>>) -> error::Result<T> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let result = match Handle::try_current() {
            Ok(handle) if handle.runtime_flavor() == RuntimeFlavor::MultiThread => {
                block_in_place(|| handle.block_on(fut))
            }
            Ok(_) => block_on_future(fut),
            Err(_) => Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| CalculatorError::Other(e.to_string()))?
                .block_on(fut),
        };
        result.map_err(CalculatorError::from)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = fut;
        Err(CalculatorError::SourceUnavailable(
            "RpcClient sync methods are not available on wasm32".into(),
        ))
    }
}

fn packed_header(header: HeaderView) -> packed::Header {
    header.inner.into()
}

fn json_order(order: Order) -> JsonOrder {
    match order {
        Order::Asc => JsonOrder::Asc,
        Order::Desc => JsonOrder::Desc,
    }
}

impl Node for RpcClient {
    fn network(&self) -> Network {
        self.network.clone()
    }

    fn get_live_cell(
        &self,
        out_point: &packed::OutPoint,
        with_data: bool,
    ) -> error::Result<LiveCell> {
        let json_op: OutPoint = out_point.clone().into();
        let live = block_on_rpc(self.request_live_cell(&json_op, with_data))?;
        let cell = live
            .cell
            .ok_or_else(|| CalculatorError::InputCellNotFound("live cell not found".into()))?;
        Ok(LiveCell {
            output: cell.output.into(),
            output_data: cell
                .data
                .map(|d| d.content.into_bytes().to_vec())
                .unwrap_or_default(),
            out_point: out_point.clone(),
            block_number: 0,
            tx_index: 0,
        })
    }

    fn get_header(&self, hash: &Hash256) -> error::Result<Option<packed::Header>> {
        let header = block_on_rpc(self.request_header(&hash_to_h256(hash)))?;
        Ok(header.map(packed_header))
    }

    fn get_header_by_number(&self, number: u64) -> error::Result<Option<packed::Header>> {
        let header = block_on_rpc(self.request_header_by_number(number.into()))?;
        Ok(header.map(packed_header))
    }

    fn get_tip_header(&self) -> error::Result<packed::Header> {
        Ok(packed_header(block_on_rpc(self.request_tip_header())?))
    }

    fn get_block_hash(&self, number: u64) -> error::Result<Option<Hash256>> {
        Ok(block_on_rpc(self.request_block_hash(number.into()))?.map(|h| h256_to_hash(&h)))
    }

    fn get_tip_block_number(&self) -> error::Result<u64> {
        Ok(u64::from(block_on_rpc(self.request_tip_block_number())?))
    }

    fn get_transaction_block_hash(&self, tx_hash: &Hash256) -> error::Result<Option<Hash256>> {
        let tx = block_on_rpc(self.request_transaction(&hash_to_h256(tx_hash)))?;
        Ok(tx
            .and_then(|t| t.tx_status.block_hash)
            .map(|h| h256_to_hash(&h)))
    }

    fn min_fee_rate(&self) -> error::Result<u64> {
        Ok(u64::from(
            block_on_rpc(self.request_tx_pool_info())?.min_fee_rate,
        ))
    }
}

impl Indexer for RpcClient {
    fn get_cells(
        &self,
        search_key: &SearchKey,
        order: Order,
        limit: u32,
        cursor: Option<&[u8]>,
    ) -> error::Result<Pagination<LiveCell>> {
        let json_key: json::SearchKey = search_key.clone().into();
        let cursor = cursor.map(|c| JsonBytes::from_vec(c.to_vec()));
        let page = block_on_rpc(self.request_cells(json_key, order, limit, cursor))?;
        Ok(Pagination {
            objects: page.objects.into_iter().map(Into::into).collect(),
            last_cursor: page.last_cursor.into_bytes().to_vec(),
        })
    }

    fn get_transactions(
        &self,
        search_key: &SearchKey,
        order: Order,
        limit: u32,
        cursor: Option<&[u8]>,
    ) -> error::Result<Pagination<Tx>> {
        let json_key: json::SearchKey = search_key.clone().into();
        let cursor = cursor.map(|c| JsonBytes::from_vec(c.to_vec()));
        let page = block_on_rpc(self.request_transactions(json_key, order, limit, cursor))?;
        Ok(Pagination {
            objects: page
                .objects
                .into_iter()
                .map(json::Tx::into_kernel)
                .collect(),
            last_cursor: page.last_cursor.into_bytes().to_vec(),
        })
    }
}

impl RPC for RpcClient {}
