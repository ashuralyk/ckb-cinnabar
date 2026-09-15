//! ckb-indexer JSON-RPC types and a richer [`CellQueryOptions`] builder.
//!
//! [`SearchKey`] is the wire type passed to `get_cells` / `get_transactions`.
//! Convert from [`CellQueryOptions`] when an operation needs extra filters
//! (secondary script, capacity range, search mode).

use ckb_types::{
    bytes::Bytes,
    packed::{CellOutput, OutPoint, Script},
    H256,
};

mod json_stuff {
    //! serde mirrors of the ckb-indexer JSON-RPC wire types.
    use super::*;
    use ckb_jsonrpc_types::{
        BlockNumber, Capacity, CellOutput, JsonBytes, OutPoint, Script, Uint32, Uint64,
    };
    use serde::{Deserialize, Serialize};

    /// Indexer `search_key` parameter for `get_cells` / `get_transactions`.
    #[derive(Serialize, Deserialize, Clone, Debug)]
    pub struct SearchKey {
        /// Primary script (lock or type, see `script_type`).
        pub script: Script,
        /// Whether `script` is matched as lock or type.
        pub script_type: ScriptType,
        /// Args matching strategy; `None` is treated as prefix by some nodes,
        /// exact by [`crate::simulation::FakeRpcClient`].
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

    impl From<CellQueryOptions> for SearchKey {
        fn from(opts: CellQueryOptions) -> SearchKey {
            let convert_range =
                |range: ValueRangeOption| [Uint64::from(range.start), Uint64::from(range.end)];
            let filter = if opts.secondary_script.is_none()
                && opts.secondary_script_len_range.is_none()
                && opts.data_len_range.is_none()
                && opts.capacity_range.is_none()
                && opts.block_range.is_none()
            {
                None
            } else {
                Some(SearchKeyFilter {
                    script: opts.secondary_script.map(|v| v.into()),
                    script_len_range: opts.secondary_script_len_range.map(convert_range),
                    output_data: None,
                    output_data_filter_mode: None,
                    output_data_len_range: opts.data_len_range.map(convert_range),
                    output_capacity_range: opts.capacity_range.map(convert_range),
                    block_range: opts.block_range.map(convert_range),
                })
            };
            SearchKey {
                script: opts.primary_script.into(),
                script_type: opts.primary_type.into(),
                script_search_mode: opts.script_search_mode,
                filter,
                with_data: opts.with_data,
                group_by_transaction: None,
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

    impl From<PrimaryScriptType> for ScriptType {
        fn from(t: PrimaryScriptType) -> ScriptType {
            match t {
                PrimaryScriptType::Lock => ScriptType::Lock,
                PrimaryScriptType::Type => ScriptType::Type,
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
                    .map(|data| data.into_bytes())
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

pub use json_stuff::*;

/// A live cell with native (non-JSON-RPC) types, converted from [`Cell`].
#[derive(Debug, Clone)]
pub struct LiveCell {
    /// Packed output (capacity + lock + type).
    pub output: CellOutput,
    /// Cell data (`Bytes::new()` when the indexer omitted it).
    pub output_data: Bytes,
    /// Out-point locating this cell.
    pub out_point: OutPoint,
    /// Block that committed the creating transaction.
    pub block_number: u64,
    /// Index of the creating transaction inside that block.
    pub tx_index: u32,
}

/// The value range option: `start <= value < end`
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ValueRangeOption {
    /// Inclusive lower bound.
    pub start: u64,
    /// Exclusive upper bound.
    pub end: u64,
}

impl ValueRangeOption {
    /// Range `[start, end)`.
    pub fn new(start: u64, end: u64) -> ValueRangeOption {
        ValueRangeOption { start, end }
    }

    /// Range matching exactly `value` (i.e. `[value, value + 1)`).
    pub fn new_exact(value: u64) -> ValueRangeOption {
        ValueRangeOption {
            start: value,
            end: value + 1,
        }
    }

    /// Range `[start, u64::MAX)` — at least `start`.
    pub fn new_min(start: u64) -> ValueRangeOption {
        ValueRangeOption {
            start,
            end: u64::MAX,
        }
    }

    /// Whether `value` falls inside the range.
    pub fn match_value(&self, value: u64) -> bool {
        self.start <= value && value < self.end
    }
}

/// The primary serach script type
///   * if primary script type is `lock` then secondary script type is `type`
///   * if primary script type is `type` then secondary script type is `lock`
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum PrimaryScriptType {
    /// Search by lock script; secondary filters apply to the type script.
    Lock,
    /// Search by type script; secondary filters apply to the lock script.
    Type,
}

/// DAO-deposit style maturity filter for cell queries.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum MaturityOption {
    /// Only cells whose lockup has matured.
    Mature,
    /// Only cells still locked.
    Immature,
    /// No maturity filtering.
    Both,
}

/// Sort order for [`CellQueryOptions`].
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum QueryOrder {
    /// Newest first.
    Desc,
    /// Oldest first.
    Asc,
}

/// Rich cell query description, convertible into an indexer [`SearchKey`]
/// (see [`json_stuff::SearchKey::from`]).
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CellQueryOptions {
    /// Script matched as lock or type (`primary_type`).
    pub primary_script: Script,
    /// Whether `primary_script` is lock or type.
    pub primary_type: PrimaryScriptType,
    /// Include cell data in indexer results.
    pub with_data: Option<bool>,
    /// Result order by block number.
    pub order: QueryOrder,
    /// Page size hint for the indexer.
    pub limit: Option<u32>,

    /// Secondary script (type if primary is lock, and vice versa).
    pub secondary_script: Option<Script>,
    /// Length range of the secondary script (`start <= len < end`).
    pub secondary_script_len_range: Option<ValueRangeOption>,
    /// Length range of cell data.
    pub data_len_range: Option<ValueRangeOption>,
    /// Capacity range in shannons.
    pub capacity_range: Option<ValueRangeOption>,
    /// Block-number range of the creating transaction.
    pub block_range: Option<ValueRangeOption>,

    /// Filter cell by its maturity
    pub maturity: MaturityOption,

    /// Try to collect at least `min_total_capacity` shannons of cells, if
    /// satisfied will stop collecting. The default value is 1 shannon means
    /// collect only one cell at most.
    pub min_total_capacity: u64,
    /// Indexer args-matching strategy (`exact` / `prefix` / `partial`).
    pub script_search_mode: Option<SearchMode>,
}

impl CellQueryOptions {
    /// Query by an arbitrary primary script; prefer [`CellQueryOptions::new_lock`]
    /// / [`CellQueryOptions::new_type`].
    pub fn new(primary_script: Script, primary_type: PrimaryScriptType) -> CellQueryOptions {
        CellQueryOptions {
            primary_script,
            primary_type,
            secondary_script: None,
            secondary_script_len_range: None,
            data_len_range: None,
            capacity_range: None,
            block_range: None,
            with_data: None,
            order: QueryOrder::Asc,
            limit: None,
            maturity: MaturityOption::Mature,
            min_total_capacity: 1,
            script_search_mode: None,
        }
    }

    /// Query cells by lock script (secondary filters then apply to the type script).
    pub fn new_lock(primary_script: Script) -> CellQueryOptions {
        CellQueryOptions::new(primary_script, PrimaryScriptType::Lock)
    }

    /// Query cells by type script (secondary filters then apply to the lock script).
    pub fn new_type(primary_script: Script) -> CellQueryOptions {
        CellQueryOptions::new(primary_script, PrimaryScriptType::Type)
    }
}
