//! Host JSON-RPC wire types for the CKB indexer.
//!
//! Assembler types live in [`crate::kernel::indexer`]. This module maps them
//! to serde JSON for HTTP.

pub use crate::kernel::indexer::{
    CellQueryOptions, GetCellsIter, Indexer, LiveCell, MaturityOption, Order, Pagination,
    PrimaryScriptType, QueryOrder, ScriptType, SearchKey, SearchKeyFilter, SearchMode, Tx,
    ValueRangeOption,
};

/// serde mirrors of the ckb-indexer JSON-RPC wire types.
pub mod json {
    use ckb_jsonrpc_types::{
        BlockNumber, Capacity, CellOutput, JsonBytes, OutPoint, Script, Uint32, Uint64,
    };
    use ckb_types::H256;
    use serde::{Deserialize, Serialize};

    use crate::kernel::indexer::{
        LiveCell, ScriptType as KernelScriptType, SearchKey as KernelSearchKey,
        SearchKeyFilter as KernelSearchKeyFilter, SearchMode as KernelSearchMode, Tx as KernelTx,
        ValueRangeOption,
    };
    use crate::types::{h256_to_hash, hash_to_h256};

    /// Indexer `search_key` parameter for `get_cells` / `get_transactions`.
    #[derive(Serialize, Deserialize, Clone, Debug)]
    pub struct SearchKey {
        /// Primary script (lock or type, see `script_type`).
        pub script: Script,
        /// Whether `script` is matched as lock or type.
        pub script_type: ScriptType,
        /// Args matching strategy.
        pub script_search_mode: Option<SearchMode>,
        /// Optional secondary-script / capacity / block filters.
        pub filter: Option<SearchKeyFilter>,
        /// Include `output_data` in each result cell.
        pub with_data: Option<bool>,
        /// Group `get_transactions` hits by transaction hash.
        pub group_by_transaction: Option<bool>,
    }

    /// How the indexer matches script args.
    #[derive(Serialize, Deserialize, Default, Clone, Debug, Eq, PartialEq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum SearchMode {
        /// Script args start with the given bytes (default).
        #[default]
        Prefix,
        /// Full script equality.
        Exact,
        /// Given bytes appear anywhere in script args.
        Partial,
    }

    impl From<KernelSearchMode> for SearchMode {
        fn from(mode: KernelSearchMode) -> Self {
            match mode {
                KernelSearchMode::Prefix => SearchMode::Prefix,
                KernelSearchMode::Exact => SearchMode::Exact,
                KernelSearchMode::Partial => SearchMode::Partial,
            }
        }
    }

    impl From<SearchMode> for KernelSearchMode {
        fn from(mode: SearchMode) -> Self {
            match mode {
                SearchMode::Prefix => KernelSearchMode::Prefix,
                SearchMode::Exact => KernelSearchMode::Exact,
                SearchMode::Partial => KernelSearchMode::Partial,
            }
        }
    }

    /// Optional secondary filter of an indexer [`SearchKey`].
    #[derive(Serialize, Deserialize, Default, Clone, Debug)]
    pub struct SearchKeyFilter {
        /// Secondary script (type if primary is lock, and vice versa).
        pub script: Option<Script>,
        /// Inclusive/exclusive length range of the secondary script.
        pub script_len_range: Option<[Uint64; 2]>,
        /// Match cell data (mode in `output_data_filter_mode`).
        pub output_data: Option<JsonBytes>,
        /// How `output_data` is compared.
        pub output_data_filter_mode: Option<SearchMode>,
        /// Inclusive/exclusive length range of cell data.
        pub output_data_len_range: Option<[Uint64; 2]>,
        /// Inclusive/exclusive capacity range (shannons).
        pub output_capacity_range: Option<[Uint64; 2]>,
        /// Inclusive/exclusive block-number range.
        pub block_range: Option<[BlockNumber; 2]>,
    }

    fn range_from_json(range: [Uint64; 2]) -> ValueRangeOption {
        ValueRangeOption::new(range[0].into(), range[1].into())
    }

    fn range_to_json(range: ValueRangeOption) -> [Uint64; 2] {
        [Uint64::from(range.start), Uint64::from(range.end)]
    }

    impl From<KernelSearchKeyFilter> for SearchKeyFilter {
        fn from(filter: KernelSearchKeyFilter) -> Self {
            SearchKeyFilter {
                script: filter.script.map(Into::into),
                script_len_range: filter.script_len_range.map(range_to_json),
                output_data: filter.output_data.map(JsonBytes::from_vec),
                output_data_filter_mode: filter.output_data_filter_mode.map(Into::into),
                output_data_len_range: filter.output_data_len_range.map(range_to_json),
                output_capacity_range: filter.output_capacity_range.map(range_to_json),
                block_range: filter.block_range.map(range_to_json),
            }
        }
    }

    impl From<SearchKeyFilter> for KernelSearchKeyFilter {
        fn from(filter: SearchKeyFilter) -> Self {
            KernelSearchKeyFilter {
                script: filter.script.map(Into::into),
                script_len_range: filter.script_len_range.map(range_from_json),
                output_data: filter.output_data.map(|d| d.into_bytes().to_vec()),
                output_data_filter_mode: filter.output_data_filter_mode.map(Into::into),
                output_data_len_range: filter.output_data_len_range.map(range_from_json),
                output_capacity_range: filter.output_capacity_range.map(range_from_json),
                block_range: filter.block_range.map(range_from_json),
            }
        }
    }

    /// Whether the primary script of a search is the lock or the type script.
    #[derive(Serialize, Deserialize, Clone, Debug)]
    #[serde(rename_all = "snake_case")]
    pub enum ScriptType {
        /// Match the cell's lock script.
        Lock,
        /// Match the cell's type script.
        Type,
    }

    impl From<KernelScriptType> for ScriptType {
        fn from(t: KernelScriptType) -> Self {
            match t {
                KernelScriptType::Lock => ScriptType::Lock,
                KernelScriptType::Type => ScriptType::Type,
            }
        }
    }

    impl From<ScriptType> for KernelScriptType {
        fn from(t: ScriptType) -> Self {
            match t {
                ScriptType::Lock => KernelScriptType::Lock,
                ScriptType::Type => KernelScriptType::Type,
            }
        }
    }

    impl From<KernelSearchKey> for SearchKey {
        fn from(key: KernelSearchKey) -> Self {
            SearchKey {
                script: key.script.into(),
                script_type: key.script_type.into(),
                script_search_mode: key.script_search_mode.map(Into::into),
                filter: key.filter.map(Into::into),
                with_data: key.with_data,
                group_by_transaction: key.group_by_transaction,
            }
        }
    }

    impl From<SearchKey> for KernelSearchKey {
        fn from(key: SearchKey) -> Self {
            KernelSearchKey {
                script: key.script.into(),
                script_type: key.script_type.into(),
                script_search_mode: key.script_search_mode.map(Into::into),
                filter: key.filter.map(Into::into),
                with_data: key.with_data,
                group_by_transaction: key.group_by_transaction,
            }
        }
    }

    /// Indexer result ordering by block number.
    #[derive(Serialize, Deserialize, Clone, Debug)]
    #[serde(rename_all = "snake_case")]
    pub enum Order {
        /// Newest first.
        Desc,
        /// Oldest first.
        Asc,
    }

    /// JSON-RPC `get_tip` result.
    #[derive(Serialize, Deserialize, Clone, Debug)]
    pub struct Tip {
        /// Hash of the tip block.
        pub block_hash: H256,
        /// Height of the tip block.
        pub block_number: BlockNumber,
    }

    /// JSON-RPC `get_cells_capacity` result.
    #[derive(Serialize, Deserialize, Clone, Debug)]
    pub struct CellsCapacity {
        /// Sum of matched live-cell capacities.
        pub capacity: Capacity,
        /// Tip hash at the time of the query.
        pub block_hash: H256,
        /// Tip height at the time of the query.
        pub block_number: BlockNumber,
    }

    /// One live cell as returned by indexer `get_cells`.
    #[derive(Serialize, Deserialize, Clone, Debug)]
    pub struct Cell {
        /// Packed output (capacity + lock + type).
        pub output: CellOutput,
        /// Cell data when `with_data` was requested.
        pub output_data: Option<JsonBytes>,
        /// Out-point locating this cell.
        pub out_point: OutPoint,
        /// Block that committed the creating transaction.
        pub block_number: BlockNumber,
        /// Index of the creating transaction inside that block.
        pub tx_index: Uint32,
    }

    impl From<Cell> for LiveCell {
        fn from(cell: Cell) -> LiveCell {
            LiveCell {
                output: cell.output.into(),
                output_data: cell
                    .output_data
                    .map(|data| data.into_bytes().to_vec())
                    .unwrap_or_default(),
                out_point: cell.out_point.into(),
                block_number: cell.block_number.value(),
                tx_index: cell.tx_index.value(),
            }
        }
    }

    /// Indexer `get_transactions` result entry: ungrouped (one io marker per
    /// entry) or grouped by transaction.
    #[derive(Serialize, Deserialize, Clone, Debug)]
    #[serde(untagged)]
    pub enum Tx {
        Ungrouped(TxWithCell),
        Grouped(TxWithCells),
    }

    impl Tx {
        /// Transaction hash regardless of grouping.
        pub fn tx_hash(&self) -> H256 {
            match self {
                Tx::Ungrouped(tx) => tx.tx_hash.clone(),
                Tx::Grouped(tx) => tx.tx_hash.clone(),
            }
        }

        /// Flatten to the kernel assembler [`KernelTx`].
        pub fn into_kernel(self) -> KernelTx {
            match self {
                Tx::Ungrouped(tx) => KernelTx {
                    tx_hash: h256_to_hash(&tx.tx_hash),
                    block_number: tx.block_number.value(),
                    tx_index: tx.tx_index.value(),
                },
                Tx::Grouped(tx) => KernelTx {
                    tx_hash: h256_to_hash(&tx.tx_hash),
                    block_number: tx.block_number.value(),
                    tx_index: tx.tx_index.value(),
                },
            }
        }
    }

    impl From<KernelTx> for Tx {
        fn from(tx: KernelTx) -> Self {
            Tx::Ungrouped(TxWithCell {
                tx_hash: hash_to_h256(&tx.tx_hash),
                block_number: tx.block_number.into(),
                tx_index: tx.tx_index.into(),
                io_index: 0.into(),
                io_type: CellType::Output,
            })
        }
    }

    /// One io marker for an ungrouped `get_transactions` hit.
    #[derive(Serialize, Deserialize, Clone, Debug)]
    pub struct TxWithCell {
        /// Transaction hash.
        pub tx_hash: H256,
        /// Block that committed the transaction.
        pub block_number: BlockNumber,
        /// Index of the transaction inside that block.
        pub tx_index: Uint32,
        /// Input or output index inside the transaction.
        pub io_index: Uint32,
        /// Whether the script appeared as an input or an output.
        pub io_type: CellType,
    }

    /// Grouped `get_transactions` hit: one transaction with all matching io markers.
    #[derive(Serialize, Deserialize, Clone, Debug)]
    pub struct TxWithCells {
        /// Transaction hash.
        pub tx_hash: H256,
        /// Block that committed the transaction.
        pub block_number: BlockNumber,
        /// Index of the transaction inside that block.
        pub tx_index: Uint32,
        /// Matching (io type, io index) pairs.
        pub cells: Vec<(CellType, Uint32)>,
    }

    /// Whether an io marker refers to a transaction input or output cell.
    #[derive(Serialize, Deserialize, Clone, Debug)]
    #[serde(rename_all = "snake_case")]
    pub enum CellType {
        /// Script appeared on an input cell.
        Input,
        /// Script appeared on an output cell.
        Output,
    }

    /// Alias of [`CellType`] kept for indexer JSON compatibility.
    #[derive(Serialize, Deserialize, Clone, Debug)]
    #[serde(rename_all = "snake_case")]
    pub enum IOType {
        Input,
        Output,
    }

    /// Paginated indexer response envelope.
    #[derive(Serialize, Deserialize)]
    pub struct Pagination<T> {
        /// Page of results.
        pub objects: Vec<T>,
        /// Opaque cursor to pass as `cursor` for the next page.
        pub last_cursor: JsonBytes,
    }
}
