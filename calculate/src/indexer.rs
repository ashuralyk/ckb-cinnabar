use ckb_types::{
    bytes::Bytes,
    packed::{CellOutput, OutPoint, Script},
    H256,
};

mod json_stuff {
    use super::*;
    use ckb_jsonrpc_types::{
        BlockNumber, Capacity, CellOutput, JsonBytes, OutPoint, Script, Uint32, Uint64,
    };
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Clone, Debug)]
    pub struct SearchKey {
        pub script: Script,
        pub script_type: ScriptType,
        pub script_search_mode: Option<SearchMode>,
        pub filter: Option<SearchKeyFilter>,
        pub with_data: Option<bool>,
        pub group_by_transaction: Option<bool>,
    }

    #[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum SearchMode {
        Prefix,
        Exact,
        Partial,
    }

    impl Default for SearchMode {
        fn default() -> Self {
            Self::Prefix
        }
    }

    #[derive(Serialize, Deserialize, Default, Clone, Debug)]
    pub struct SearchKeyFilter {
        pub script: Option<Script>,
        pub script_len_range: Option<[Uint64; 2]>,
        pub output_data: Option<JsonBytes>,
        pub output_data_filter_mode: Option<SearchMode>,
        pub output_data_len_range: Option<[Uint64; 2]>,
        pub output_capacity_range: Option<[Uint64; 2]>,
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

    #[derive(Serialize, Deserialize, Clone, Debug)]
    #[serde(rename_all = "snake_case")]
    pub enum ScriptType {
        Lock,
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

    #[derive(Serialize, Deserialize, Clone, Debug)]
    #[serde(rename_all = "snake_case")]
    pub enum Order {
        Desc,
        Asc,
    }

    #[derive(Serialize, Deserialize, Clone, Debug)]
    pub struct Tip {
        pub block_hash: H256,
        pub block_number: BlockNumber,
    }

    #[derive(Serialize, Deserialize, Clone, Debug)]
    pub struct CellsCapacity {
        pub capacity: Capacity,
        pub block_hash: H256,
        pub block_number: BlockNumber,
    }

    #[derive(Serialize, Deserialize, Clone, Debug)]
    pub struct Cell {
        pub output: CellOutput,
        pub output_data: Option<JsonBytes>,
        pub out_point: OutPoint,
        pub block_number: BlockNumber,
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

    #[derive(Serialize, Deserialize, Clone, Debug)]
    #[serde(untagged)]
    pub enum Tx {
        Ungrouped(TxWithCell),
        Grouped(TxWithCells),
    }

    #[derive(Serialize, Deserialize, Clone, Debug)]
    pub struct TxWithCell {
        pub tx_hash: H256,
        pub block_number: BlockNumber,
        pub tx_index: Uint32,
        pub io_index: Uint32,
        pub io_type: CellType,
    }

    #[derive(Serialize, Deserialize, Clone, Debug)]
    pub struct TxWithCells {
        pub tx_hash: H256,
        pub block_number: BlockNumber,
        pub tx_index: Uint32,
        pub cells: Vec<(CellType, Uint32)>,
    }

    #[derive(Serialize, Deserialize, Clone, Debug)]
    #[serde(rename_all = "snake_case")]
    pub enum CellType {
        Input,
        Output,
    }

    #[derive(Serialize, Deserialize, Clone, Debug)]
    #[serde(rename_all = "snake_case")]
    pub enum IOType {
        Input,
        Output,
    }

    #[derive(Serialize, Deserialize)]
    pub struct Pagination<T> {
        pub objects: Vec<T>,
        pub last_cursor: JsonBytes,
    }
}

pub use json_stuff::*;

#[derive(Debug, Clone)]
pub struct LiveCell {
    pub output: CellOutput,
    pub output_data: Bytes,
    pub out_point: OutPoint,
    pub block_number: u64,
    pub tx_index: u32,
}

/// The value range option: `start <= value < end`
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ValueRangeOption {
    pub start: u64,
    pub end: u64,
}

impl ValueRangeOption {
    pub fn new(start: u64, end: u64) -> ValueRangeOption {
        ValueRangeOption { start, end }
    }

    pub fn new_exact(value: u64) -> ValueRangeOption {
        ValueRangeOption {
            start: value,
            end: value + 1,
        }
    }

    pub fn new_min(start: u64) -> ValueRangeOption {
        ValueRangeOption {
            start,
            end: u64::MAX,
        }
    }

    pub fn match_value(&self, value: u64) -> bool {
        self.start <= value && value < self.end
    }
}

/// The primary serach script type
///   * if primary script type is `lock` then secondary script type is `type`
///   * if primary script type is `type` then secondary script type is `lock`
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum PrimaryScriptType {
    Lock,
    Type,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum MaturityOption {
    Mature,
    Immature,
    Both,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum QueryOrder {
    Desc,
    Asc,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CellQueryOptions {
    pub primary_script: Script,
    pub primary_type: PrimaryScriptType,
    pub with_data: Option<bool>,
    pub order: QueryOrder,
    pub limit: Option<u32>,

    // Options for SearchKeyFilter
    pub secondary_script: Option<Script>,
    pub secondary_script_len_range: Option<ValueRangeOption>,
    pub data_len_range: Option<ValueRangeOption>,
    pub capacity_range: Option<ValueRangeOption>,
    pub block_range: Option<ValueRangeOption>,

    /// Filter cell by its maturity
    pub maturity: MaturityOption,

    /// Try to collect at least `min_total_capacity` shannons of cells, if
    /// satisfied will stop collecting. The default value is 1 shannon means
    /// collect only one cell at most.
    pub min_total_capacity: u64,
    pub script_search_mode: Option<SearchMode>,
}

impl CellQueryOptions {
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

    pub fn new_lock(primary_script: Script) -> CellQueryOptions {
        CellQueryOptions::new(primary_script, PrimaryScriptType::Lock)
    }

    pub fn new_type(primary_script: Script) -> CellQueryOptions {
        CellQueryOptions::new(primary_script, PrimaryScriptType::Type)
    }
}
