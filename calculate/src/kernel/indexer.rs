//! Sync live-cell search used by Calculate operations.
//!
//! [`Indexer`] is the guest-facing search surface (HTTP and a future SSRI
//! adapter both implement it). Types are packed molecule values plus `u64` /
//! `Vec<u8>` — no JSON-RPC wire format.

use alloc::vec::Vec;

use crate::kernel::{
    error::Result,
    types::packed::{CellOutput, OutPoint, Script},
};

/// How the indexer matches script args.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum SearchMode {
    /// Script args start with the given bytes (default).
    #[default]
    Prefix,
    /// Full script equality.
    Exact,
    /// Given bytes appear anywhere in script args.
    Partial,
}

/// Whether the primary script of a search is the lock or the type script.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ScriptType {
    /// Match the cell's lock script.
    Lock,
    /// Match the cell's type script.
    Type,
}

/// Alias of [`ScriptType`] used by [`CellQueryOptions`].
pub type PrimaryScriptType = ScriptType;

/// Inclusive/exclusive `u64` range: `start <= value < end`.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ValueRangeOption {
    /// Inclusive lower bound.
    pub start: u64,
    /// Exclusive upper bound.
    pub end: u64,
}

impl ValueRangeOption {
    /// Range `[start, end)`.
    pub fn new(start: u64, end: u64) -> Self {
        ValueRangeOption { start, end }
    }

    /// Range matching exactly `value` (i.e. `[value, value + 1)`).
    pub fn new_exact(value: u64) -> Self {
        ValueRangeOption {
            start: value,
            end: value + 1,
        }
    }

    /// Range `[start, u64::MAX)` — at least `start`.
    pub fn new_min(start: u64) -> Self {
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

/// Optional secondary filter of an indexer [`SearchKey`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchKeyFilter {
    /// Secondary script (type if primary is lock, and vice versa).
    pub script: Option<Script>,
    /// Length range of the secondary script (`start <= len < end`).
    pub script_len_range: Option<ValueRangeOption>,
    /// Match cell data (mode in `output_data_filter_mode`).
    pub output_data: Option<Vec<u8>>,
    /// How `output_data` is compared.
    pub output_data_filter_mode: Option<SearchMode>,
    /// Length range of cell data.
    pub output_data_len_range: Option<ValueRangeOption>,
    /// Capacity range in shannons.
    pub output_capacity_range: Option<ValueRangeOption>,
    /// Block-number range of the creating transaction.
    pub block_range: Option<ValueRangeOption>,
}

/// Indexer search key for [`Indexer::get_cells`] / [`Indexer::get_transactions`].
#[derive(Clone, Debug, PartialEq, Eq)]
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

/// One live cell as returned by indexer search (assembler type).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveCell {
    /// Packed output (capacity + lock + type).
    pub output: CellOutput,
    /// Cell data (`Vec::new()` when the indexer omitted it).
    pub output_data: Vec<u8>,
    /// Out-point locating this cell.
    pub out_point: OutPoint,
    /// Block that committed the creating transaction (`0` if unknown).
    pub block_number: u64,
    /// Index of the creating transaction inside that block.
    pub tx_index: u32,
}

/// A `get_transactions` hit (hash + position). Grouping is flattened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tx {
    /// Transaction hash.
    pub tx_hash: [u8; 32],
    /// Block that committed the transaction.
    pub block_number: u64,
    /// Index of the transaction inside that block.
    pub tx_index: u32,
}

/// Paginated indexer response envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pagination<T> {
    /// Page of results.
    pub objects: Vec<T>,
    /// Opaque cursor to pass as `cursor` for the next page.
    pub last_cursor: Vec<u8>,
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

/// Rich cell query description, convertible into an indexer [`SearchKey`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellQueryOptions {
    /// Script matched as lock or type (`primary_type`).
    pub primary_script: Script,
    /// Whether `primary_script` is lock or type.
    pub primary_type: ScriptType,
    /// Include cell data in indexer results.
    pub with_data: Option<bool>,
    /// Result order by block number (hint; adapters may ignore it).
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
    /// Filter cell by its maturity (hint; adapters may ignore it).
    pub maturity: MaturityOption,
    /// Stop collecting once this many shannons are gathered (hint).
    pub min_total_capacity: u64,
    /// Indexer args-matching strategy (`exact` / `prefix` / `partial`).
    pub script_search_mode: Option<SearchMode>,
}

impl CellQueryOptions {
    /// Query by an arbitrary primary script; prefer [`CellQueryOptions::new_lock`]
    /// / [`CellQueryOptions::new_type`].
    pub fn new(primary_script: Script, primary_type: ScriptType) -> Self {
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
    pub fn new_lock(primary_script: Script) -> Self {
        CellQueryOptions::new(primary_script, ScriptType::Lock)
    }

    /// Query cells by type script (secondary filters then apply to the lock script).
    pub fn new_type(primary_script: Script) -> Self {
        CellQueryOptions::new(primary_script, ScriptType::Type)
    }
}

impl From<CellQueryOptions> for SearchKey {
    fn from(opts: CellQueryOptions) -> SearchKey {
        let filter = if opts.secondary_script.is_none()
            && opts.secondary_script_len_range.is_none()
            && opts.data_len_range.is_none()
            && opts.capacity_range.is_none()
            && opts.block_range.is_none()
        {
            None
        } else {
            Some(SearchKeyFilter {
                script: opts.secondary_script,
                script_len_range: opts.secondary_script_len_range,
                output_data: None,
                output_data_filter_mode: None,
                output_data_len_range: opts.data_len_range,
                output_capacity_range: opts.capacity_range,
                block_range: opts.block_range,
            })
        };
        SearchKey {
            script: opts.primary_script,
            script_type: opts.primary_type,
            script_search_mode: opts.script_search_mode,
            filter,
            with_data: opts.with_data,
            group_by_transaction: None,
        }
    }
}

/// Sync live-cell / transaction search.
pub trait Indexer {
    /// Paginated live-cell search (spent cells excluded).
    fn get_cells(
        &self,
        search_key: &SearchKey,
        limit: u32,
        cursor: Option<&[u8]>,
    ) -> Result<Pagination<LiveCell>>;

    /// Transaction search — txs where the script appears as input or output,
    /// including spent cells (unlike [`Indexer::get_cells`], which is live-only).
    fn get_transactions(
        &self,
        search_key: &SearchKey,
        limit: u32,
        cursor: Option<&[u8]>,
    ) -> Result<Pagination<Tx>>;
}

/// Walks [`Indexer::get_cells`] pages until a batch comes back empty.
pub struct GetCellsIter<'a, I: Indexer> {
    indexer: &'a I,
    search_key: SearchKey,
    cursor: Option<Vec<u8>>,
}

impl<'a, I: Indexer> GetCellsIter<'a, I> {
    /// Create an iterator over all live cells matching `search_key`.
    pub fn new(indexer: &'a I, search_key: SearchKey) -> Self {
        GetCellsIter {
            indexer,
            search_key,
            cursor: None,
        }
    }

    /// Fetch the next page of up to `limit` cells. Returns `Ok(None)` once a
    /// page comes back empty (iterator exhausted).
    pub fn next_batch(&mut self, limit: u32) -> Result<Option<Vec<LiveCell>>> {
        let page = self
            .indexer
            .get_cells(&self.search_key, limit, self.cursor.as_deref())?;
        if page.objects.is_empty() {
            return Ok(None);
        }
        self.cursor = Some(page.last_cursor);
        Ok(Some(page.objects))
    }

    /// Fetch the next single cell; `Ok(None)` when exhausted.
    pub fn next(&mut self) -> Result<Option<LiveCell>> {
        Ok(self.next_batch(1)?.and_then(|mut v| v.pop()))
    }
}
