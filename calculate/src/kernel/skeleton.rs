//! Mutable CKB transaction being assembled off-chain.
//!
//! [`TransactionSkeleton`] holds the same five fields as a packed
//! transaction (inputs / outputs / cell deps / witnesses / header deps),
//! plus enough resolved cell data to compute capacity, type ids, and script
//! groups. [`ScriptEx::Reference`] defers code-hash resolution until a named
//! [`CellDepEx`] is present.
//!
//! Operations mutate a skeleton; [`crate::TransactionCalculator`] consumes it.
//! Host RPC / `TransactionView` / send helpers live in [`crate::skeleton`] (shell).

use alloc::{format, string::String, vec, vec::Vec};

use ckb_hash::{blake2b_256, Blake2bBuilder};

use crate::kernel::{
    error::{CalculatorError, Result},
    indexer::{CellQueryOptions, GetCellsIter, Indexer, LiveCell, SearchMode, ValueRangeOption},
    rpc::Node,
    types::{
        occupied_capacity_shannons, pack_hash,
        packed::{self, Bytes, CellDep, CellInput, CellOutput, OutPoint, Script, WitnessArgs},
        unpack_hash, Builder, DepType, Entity, Hash256, HeaderView, Pack, PackVec, ScriptHashType,
        Unpack,
    },
};

/// Well-known code hash of the type-id system script (`TYPE_ID` in ASCII).
pub use crate::types::TYPE_ID_CODE_HASH;

/// A wrapper of packed Script
///
/// `Reference` branch: point to a celldep in the transaction, if `usize` is MAX, point to the last one
#[derive(Clone, PartialEq, Eq)]
pub enum ScriptEx {
    /// Concrete script triple: code hash, hash type, args.
    Script(Hash256, ScriptHashType, Vec<u8>),
    /// Indirect script: resolved at build time from the named cell dep's data
    /// hash (`Data1`) or type hash (`Type`); second element is the args.
    Reference(String, Vec<u8>),
}

impl Default for ScriptEx {
    fn default() -> Self {
        ScriptEx::Script(Hash256::default(), ScriptHashType::Data, Vec::new())
    }
}

impl PartialEq<Script> for ScriptEx {
    fn eq(&self, other: &Script) -> bool {
        let Ok(script) = Script::try_from(self.clone()) else {
            return false;
        };
        &script == other
    }
}

impl ScriptEx {
    /// Initialize a ScriptEx of `Data1`
    pub fn new_code(code_hash: Hash256, args: Vec<u8>) -> Self {
        ScriptEx::Script(code_hash, ScriptHashType::Data1, args)
    }

    /// Initialize a ScriptEx of `Type`
    pub fn new_type(type_hash: Hash256, args: Vec<u8>) -> Self {
        ScriptEx::Script(type_hash, ScriptHashType::Type, args)
    }

    /// Initialize a ScriptEx of `TypeId`
    pub fn new_type_id(args: Hash256) -> Self {
        ScriptEx::new_type(TYPE_ID_CODE_HASH, args.as_ref().to_vec())
    }

    /// Get `code_hash` of ScriptEx
    pub fn code_hash(&self) -> Result<Hash256> {
        match self {
            ScriptEx::Script(code_hash, _, _) => Ok(code_hash.clone()),
            _ => Err(CalculatorError::Other("reference script".into())),
        }
    }

    /// Get `hash_type` of ScriptEx
    pub fn hash_type(&self) -> Result<ScriptHashType> {
        match self {
            ScriptEx::Script(_, hash_type, _) => Ok(*hash_type),
            _ => Err(CalculatorError::Other("reference script".into())),
        }
    }

    /// Get `args` of ScriptEx
    pub fn args(&self) -> Vec<u8> {
        match self {
            ScriptEx::Script(_, _, args) => args.clone(),
            ScriptEx::Reference(_, args) => args.clone(),
        }
    }

    /// Change `args` of ScriptEx
    pub fn set_args(self, args: Vec<u8>) -> Self {
        match self {
            ScriptEx::Script(code_hash, hash_type, _) => {
                ScriptEx::Script(code_hash, hash_type, args)
            }
            ScriptEx::Reference(name, _) => ScriptEx::Reference(name, args),
        }
    }

    /// Calculate blake2b hash of the script
    pub fn script_hash(&self) -> Result<Hash256> {
        Script::try_from(self.clone()).map(|v| unpack_hash(&v.calc_script_hash()))
    }

    /// Build packed Script from ScriptEx and TransactionSkeleton
    pub fn to_script(self, skeleton: &TransactionSkeleton) -> Result<Script> {
        if let ScriptEx::Reference(name, _) = &self {
            let (_, value) = skeleton.find_celldep_by_script(&self).ok_or_else(|| {
                CalculatorError::CellDepNotFound(format!("celldep {name} not found"))
            })?;
            if value.celldep.dep_type().as_slice() == [DepType::DepGroup as u8] {
                return Err(CalculatorError::Other(
                    "no support for group celldep".into(),
                ));
            }
            let output = &value.output;
            let mut script = Script::new_builder().args(self.args().pack());
            if let Some(celldep_type_hash) = output.calc_type_hash() {
                script = script
                    .code_hash(pack_hash(&celldep_type_hash))
                    .hash_type(ScriptHashType::Type);
            } else {
                if !value.with_data {
                    return Err(CalculatorError::CellDepNotFound(
                        "celldep without data, cannot calculate data hash".into(),
                    ));
                }
                script = script
                    .code_hash(pack_hash(&output.data_hash()))
                    .hash_type(ScriptHashType::Data1);
            }
            Ok(script.build())
        } else {
            self.try_into()
        }
    }

    /// Build packed Script from ScriptEx, throw error if failed
    pub fn to_script_unchecked(self) -> Script {
        self.try_into().expect("unchecked to_script")
    }
}

impl TryFrom<ScriptEx> for Script {
    type Error = CalculatorError;

    fn try_from(value: ScriptEx) -> Result<Self> {
        match value {
            ScriptEx::Script(code_hash, hash_type, args) => Ok(Script::new_builder()
                .code_hash(pack_hash(&code_hash))
                .hash_type(hash_type)
                .args(args.pack())
                .build()),
            ScriptEx::Reference(_, _) => Err(CalculatorError::Other("reference script".into())),
        }
    }
}

fn script_hash_type_from_byte(byte: u8) -> ScriptHashType {
    match byte {
        0 => ScriptHashType::Data,
        1 => ScriptHashType::Type,
        2 => ScriptHashType::Data1,
        4 => ScriptHashType::Data2,
        other => ScriptHashType::from_repr(other).unwrap_or(ScriptHashType::Data),
    }
}

impl From<Script> for ScriptEx {
    fn from(value: Script) -> Self {
        ScriptEx::Script(
            unpack_hash(&value.code_hash()),
            script_hash_type_from_byte(value.hash_type().into()),
            value.args().raw_data().to_vec(),
        )
    }
}

impl From<(String, Vec<u8>)> for ScriptEx {
    fn from((celldep_name, args): (String, Vec<u8>)) -> Self {
        ScriptEx::Reference(celldep_name, args)
    }
}

/// CellInput for transaction skeleton, which contains output cell and data
#[derive(Debug, Clone)]
pub struct CellInputEx {
    /// The packed input (out-point + since).
    pub input: CellInput,
    /// Full content of the consumed cell.
    pub output: CellOutputEx,
    /// Whether `output.data` was actually fetched (vs. left empty).
    pub with_data: bool,
}

impl PartialEq for CellInputEx {
    fn eq(&self, other: &Self) -> bool {
        self.input.as_bytes() == other.input.as_bytes()
    }
}

impl CellInputEx {
    /// Directly initialize a CellInputEx
    pub fn new(input: CellInput, output: CellOutput, data: Option<Vec<u8>>) -> Self {
        if let Some(data) = data {
            CellInputEx {
                input,
                output: CellOutputEx::new(output, data),
                with_data: true,
            }
        } else {
            CellInputEx {
                input,
                output: CellOutputEx::new(output, Vec::new()),
                with_data: false,
            }
        }
    }

    /// Turn a CelldepEx into CellInputEx
    pub fn new_from_celldep(celldep: &CellDepEx, since: Option<u64>) -> Self {
        let input = CellInput::new_builder()
            .previous_output(celldep.celldep.out_point())
            .since(since.unwrap_or(0))
            .build();
        let data = if celldep.with_data {
            Some(celldep.output.data.clone())
        } else {
            None
        };
        Self::new(input, celldep.output.output.clone(), data)
    }

    /// Build an input from a known out-point via [`Node::get_live_cell`].
    pub fn new_from_outpoint<N: Node>(
        node: &N,
        tx_hash: Hash256,
        index: u32,
        since: Option<u64>,
        with_data: bool,
    ) -> Result<Self> {
        let out_point = OutPoint::new_builder()
            .tx_hash(pack_hash(&tx_hash))
            .index(index)
            .build();
        let live = node.get_live_cell(&out_point, with_data)?;
        let input = CellInput::new_builder()
            .previous_output(out_point)
            .since(since.unwrap_or(0))
            .build();
        let data = if with_data {
            Some(live.output_data)
        } else {
            None
        };
        Ok(Self::new(input, live.output, data))
    }

    /// Build an input from an indexer [`LiveCell`].
    pub fn new_from_live_cell(cell: LiveCell, since: Option<u64>) -> Self {
        let input = CellInput::new_builder()
            .previous_output(cell.out_point)
            .since(since.unwrap_or(0))
            .build();
        Self::new(input, cell.output, Some(cell.output_data))
    }
}

/// CellOutput for transaction skeleton, which contains cell data
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellOutputEx {
    /// The packed cell output (capacity + lock + type).
    pub output: CellOutput,
    /// Cell data (kept separately, like `outputs_data` in a transaction).
    pub data: Vec<u8>,
}

impl CellOutputEx {
    /// Directly initialize a CellOutputEx
    pub fn new(output: CellOutput, data: Vec<u8>) -> Self {
        CellOutputEx { output, data }
    }

    /// Initialize a CellOutputEx from inner types (capacity in shannons).
    pub fn new_from_scripts(
        lock_script: Script,
        type_script: Option<Script>,
        data: Vec<u8>,
        capacity: Option<u64>,
    ) -> Result<Self> {
        let draft = CellOutput::new_builder()
            .lock(lock_script)
            .type_(type_script.pack())
            .build();
        let occupied = occupied_capacity_shannons(&draft, data.len());
        let cap = capacity.unwrap_or(occupied);
        if cap < occupied {
            return Err(CalculatorError::InsufficientCapacity(
                "capacity is less than minimal capacity".into(),
            ));
        }
        let output = draft.as_builder().capacity(cap).build();
        Ok(CellOutputEx::new(output, data))
    }

    /// Correct capacity if the declared is less than the occupied
    pub fn correct_capacity(mut self) -> Self {
        let declared = self.capacity_shannons();
        let occupied = occupied_capacity_shannons(&self.output, self.data.len());
        let cap = declared.max(occupied);
        self.output = self.output.clone().as_builder().capacity(cap).build();
        self
    }

    /// Occupied capacity in shannons.
    pub fn capacity_shannons(&self) -> u64 {
        let cap: packed::Uint64 = self.output.capacity();
        let bytes = cap.raw_data();
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&bytes);
        u64::from_le_bytes(buf)
    }

    /// Cell's lock script
    pub fn lock_script(&self) -> Script {
        self.output.lock()
    }

    /// Cell's type script
    pub fn type_script(&self) -> Option<Script> {
        self.output.type_().to_opt()
    }

    /// Calculate blake2b hash of lock script
    pub fn calc_lock_hash(&self) -> Hash256 {
        unpack_hash(&self.lock_script().calc_script_hash())
    }

    /// Calculate blake2b hash of type script
    pub fn calc_type_hash(&self) -> Option<Hash256> {
        self.type_script()
            .map(|script| unpack_hash(&script.calc_script_hash()))
    }

    /// Calculate blake2b hash of cell data
    pub fn data_hash(&self) -> Hash256 {
        blake2b_256(&self.data)
    }
}

/// CellDep for transaction skeleton, which contains output cell and data
#[derive(Debug, Clone)]
pub struct CellDepEx {
    /// Unique name, referenced by `ScriptEx::Reference`.
    pub name: String,
    /// The packed cell dep (out-point + dep type).
    pub celldep: CellDep,
    /// Full content of the dep cell.
    pub output: CellOutputEx,
    /// Whether `output.data` was actually fetched.
    pub with_data: bool,
}

impl PartialEq for CellDepEx {
    fn eq(&self, other: &Self) -> bool {
        self.celldep.as_bytes() == other.celldep.as_bytes()
    }
}

impl CellDepEx {
    /// Directly initialize a CellDepEx
    pub fn new(name: String, cell_dep: CellDep, output: CellOutput, data: Option<Vec<u8>>) -> Self {
        if let Some(data) = data {
            CellDepEx {
                name,
                celldep: cell_dep,
                output: CellOutputEx::new(output, data),
                with_data: true,
            }
        } else {
            CellDepEx {
                name,
                celldep: cell_dep,
                output: CellOutputEx::new(output, Vec::new()),
                with_data: false,
            }
        }
    }

    /// Build a named cell dep from a known out-point via [`Node::get_live_cell`].
    pub fn new_from_outpoint<N: Node>(
        node: &N,
        name: String,
        tx_hash: Hash256,
        index: u32,
        dep_type: DepType,
        with_data: bool,
    ) -> Result<Self> {
        let out_point = OutPoint::new_builder()
            .tx_hash(pack_hash(&tx_hash))
            .index(index)
            .build();
        let live = node.get_live_cell(&out_point, with_data)?;
        let cell_dep = CellDep::new_builder()
            .out_point(out_point)
            .dep_type(dep_type)
            .build();
        let data = if with_data {
            Some(live.output_data)
        } else {
            None
        };
        Ok(Self::new(name, cell_dep, live.output, data))
    }

    /// Build a named cell dep from an indexer [`LiveCell`].
    pub fn new_from_live_cell(name: String, cell: LiveCell, dep_type: DepType) -> Self {
        let cell_dep = CellDep::new_builder()
            .out_point(cell.out_point)
            .dep_type(dep_type)
            .build();
        Self::new(name, cell_dep, cell.output, Some(cell.output_data))
    }

    /// Re-fetch this dep's output and data from the node.
    pub fn refresh_cell_output<N: Node>(&mut self, node: &N) -> Result<()> {
        let out_point = self.celldep.out_point();
        let tx_hash = unpack_hash(&out_point.tx_hash());
        let index = {
            let bytes = out_point.index().raw_data();
            let mut buf = [0u8; 4];
            buf.copy_from_slice(&bytes);
            u32::from_le_bytes(buf)
        };
        let dep_type = match u8::from(self.celldep.dep_type()) {
            0 => DepType::Code,
            1 => DepType::DepGroup,
            _ => return Err(CalculatorError::Other("invalid dep type".into())),
        };
        let refreshed =
            Self::new_from_outpoint(node, self.name.clone(), tx_hash, index, dep_type, true)?;
        self.output = refreshed.output;
        self.with_data = true;
        Ok(())
    }
}

/// Traditional witness args that contains lock, input_type and output_type, which
/// splited for better composability
#[derive(Debug, Clone)]
pub struct WitnessEx {
    /// All fields empty — serialized as empty bytes, not as WitnessArgs.
    pub empty: bool,
    /// Serialize as molecule `WitnessArgs` (true) or as raw concatenated bytes (false).
    pub traditional: bool,
    /// `WitnessArgs.lock` (or the whole payload when non-traditional).
    pub lock: Vec<u8>,
    /// `WitnessArgs.input_type`.
    pub input_type: Vec<u8>,
    /// `WitnessArgs.output_type`.
    pub output_type: Vec<u8>,
}

impl Default for WitnessEx {
    fn default() -> Self {
        WitnessEx {
            empty: true,
            traditional: true,
            lock: Vec::new(),
            input_type: Vec::new(),
            output_type: Vec::new(),
        }
    }
}

impl WitnessEx {
    /// Directly initialize a WitnessArgsEx
    pub fn new(lock: Vec<u8>, input_type: Vec<u8>, output_type: Vec<u8>) -> Self {
        WitnessEx {
            empty: false,
            traditional: true,
            lock,
            input_type,
            output_type,
        }
    }

    /// Initialize a WitnessArgsEx and mark it non-traditional
    pub fn new_plain(plain_bytes: Vec<u8>) -> Self {
        WitnessEx {
            empty: false,
            traditional: false,
            lock: plain_bytes,
            input_type: Vec::new(),
            output_type: Vec::new(),
        }
    }

    /// Turn into packed WitnessArgs
    pub fn into_witness_args(self) -> WitnessArgs {
        let bytes_opt = |bytes: Vec<u8>| {
            if bytes.is_empty() {
                None
            } else {
                Some(
                    Bytes::new_builder()
                        .set(bytes.into_iter().map(Into::into).collect())
                        .build(),
                )
            }
        };
        WitnessArgs::new_builder()
            .lock(bytes_opt(self.lock))
            .input_type(bytes_opt(self.input_type))
            .output_type(bytes_opt(self.output_type))
            .build()
    }

    /// Turn into packed bytes of WitnessArgs
    pub fn into_packed_bytes(mut self) -> Bytes {
        if !self.lock.is_empty() || !self.input_type.is_empty() || !self.output_type.is_empty() {
            self.empty = false;
        }
        if self.empty {
            Bytes::default()
        } else if self.traditional {
            self.into_witness_args().as_bytes().pack()
        } else {
            self.into_packed_plain_bytes()
        }
    }

    /// Turn into packed raw bytes, which is normally not in format of WitnessArgs
    pub fn into_packed_plain_bytes(self) -> Bytes {
        let bytes = self
            .lock
            .into_iter()
            .chain(self.input_type)
            .chain(self.output_type)
            .collect::<Vec<_>>();
        bytes.pack()
    }
}

/// A block hash wrapper that contains the link to a cell input
#[derive(Clone, Debug)]
pub struct HeaderDepEx {
    /// Hash of the dep'd block.
    pub block_hash: Hash256,
    /// Full header, fetched for off-chain checks (e.g. DAO maturity).
    pub header: HeaderView,
    /// Out-points of inputs/celldeps committed in this block.
    pub cellinput_outpoints: Vec<OutPoint>,
}

impl HeaderDepEx {
    /// Build a header dep from already-resolved parts (inject path).
    pub fn from_parts(
        block_hash: Hash256,
        header: HeaderView,
        cellinput_outpoints: Vec<OutPoint>,
    ) -> Self {
        HeaderDepEx {
            block_hash,
            header,
            cellinput_outpoints,
        }
    }

    /// Fetch the header for `block_hash` and link it to `outpoints`.
    pub fn new<N: Node>(node: &N, block_hash: Hash256, outpoints: Vec<OutPoint>) -> Result<Self> {
        let header = node
            .get_header(&block_hash)?
            .ok_or_else(|| CalculatorError::Other("header not found".into()))?;
        Ok(HeaderDepEx::from_parts(block_hash, header, outpoints))
    }

    /// Header dep for the block that committed the transaction containing `outpoint`.
    pub fn new_from_outpoint<N: Node>(node: &N, outpoint: OutPoint) -> Result<Self> {
        let tx_hash = unpack_hash(&outpoint.tx_hash());
        let block_hash = node.get_transaction_block_hash(&tx_hash)?.ok_or_else(|| {
            CalculatorError::InputCellNotFound("transaction not found by input outpoint".into())
        })?;
        HeaderDepEx::new(node, block_hash, vec![outpoint])
    }

    /// Header dep for the block at `block_number`.
    pub fn new_from_block_number<N: Node>(node: &N, block_number: u64) -> Result<Self> {
        let block_hash = node
            .get_block_hash(block_number)?
            .ok_or_else(|| CalculatorError::Other("block not found".into()))?;
        HeaderDepEx::new(node, block_hash, vec![])
    }

    /// Block number from the packed header.
    pub fn number(&self) -> u64 {
        let number: u64 = self.header.raw().number().unpack();
        number
    }

    /// Block timestamp (ms) from the packed header.
    pub fn timestamp(&self) -> u64 {
        let timestamp: u64 = self.header.raw().timestamp().unpack();
        timestamp
    }
}

impl PartialEq for HeaderDepEx {
    fn eq(&self, other: &Self) -> bool {
        self.block_hash == other.block_hash
    }
}

/// Decode a 0-based get index, including Lua-style negatives encoded as
/// `(-n) as usize`: `-1` (also `usize::MAX`) is last, `-2` second-to-last.
///
/// Returns `(Some(at), _)` on success. On failure returns `(None, empty)`
/// where `empty` is `true` when `len == 0`.
fn resolve_get_index(index: usize, len: usize) -> (Option<usize>, bool) {
    let empty = len == 0;
    let signed = index as isize;
    let resolved = if signed >= 0 {
        Some(index)
    } else {
        len.checked_sub(signed.unsigned_abs())
    };
    match resolved {
        Some(at) if at < len => (Some(at), false),
        _ => (None, empty),
    }
}

/// Transaction under construction: the five CKB fields plus resolved cell data.
///
/// Operations append to these vectors. Input/output/cell-dep getters accept a
/// 0-based index, or a Lua-style relative index encoded as `(-n) as usize`:
/// `-1` (also `usize::MAX`) is the last item, `-2` the second-to-last.
#[derive(Default, Clone, Debug)]
pub struct TransactionSkeleton {
    /// Consumed cells (`CellInput` + resolved output/data).
    pub inputs: Vec<CellInputEx>,
    /// Created cells (`CellOutput` + data).
    pub outputs: Vec<CellOutputEx>,
    /// Named cell deps; [`ScriptEx::Reference`] resolves against these names.
    pub celldeps: Vec<CellDepEx>,
    /// Witnesses, typically one per input (secp256k1 lock group uses the first).
    pub witnesses: Vec<WitnessEx>,
    /// Header deps, optionally linked to the inputs/cell-deps they commit.
    pub headerdeps: Vec<HeaderDepEx>,
}

impl TransactionSkeleton {
    /// Get input cell by index, which may fail if index out of range
    ///
    /// `index` is 0-based, or a Lua-style relative value: `-1` (also
    /// `usize::MAX`) is the last input, `-2` the second-to-last.
    pub fn get_input_by_index(&self, input_index: usize) -> Result<&CellInputEx> {
        match resolve_get_index(input_index, self.inputs.len()) {
            (Some(at), _) => Ok(&self.inputs[at]),
            (None, true) => Err(CalculatorError::InputCellNotFound(
                "transaction input empty".into(),
            )),
            (None, false) => Err(CalculatorError::InputCellNotFound(
                "transaction input index out of range".into(),
            )),
        }
    }

    /// Push a single input cell
    pub fn input(&mut self, cell_input: CellInputEx) -> Result<&mut Self> {
        if self.contains_input(&cell_input) {
            return Err(CalculatorError::Other("input already exists".into()));
        }
        self.inputs.push(cell_input);
        Ok(self)
    }

    /// Push a batch of input cells
    pub fn inputs(&mut self, cell_inputs: Vec<CellInputEx>) -> Result<&mut Self> {
        for cell_input in &cell_inputs {
            if self.contains_input(cell_input) {
                return Err(CalculatorError::Other("input already exists".into()));
            }
        }
        self.inputs.extend(cell_inputs);
        Ok(self)
    }

    /// Check if input cell exists
    pub fn contains_input(&self, cell_input: &CellInputEx) -> bool {
        self.inputs.contains(cell_input)
    }

    /// Push one live capacity cell whose lock matches `lock_script` (no type,
    /// empty data). Used to inject balancer / change inputs.
    pub fn input_from_script<I: Indexer>(
        &mut self,
        indexer: &I,
        lock_script: ScriptEx,
    ) -> Result<&mut Self> {
        let mut query = CellQueryOptions::new_lock(lock_script.to_script(self)?);
        query.secondary_script_len_range = Some(ValueRangeOption::new(0, 1));
        query.data_len_range = Some(ValueRangeOption::new(0, 1));
        query.script_search_mode = Some(SearchMode::Exact);
        let mut iter = GetCellsIter::new(indexer, query.into());
        while let Some(cell) = iter.next()? {
            let cell_input = CellInputEx::new_from_live_cell(cell, None);
            if self.contains_input(&cell_input) {
                continue;
            }
            self.inputs.push(cell_input);
            return Ok(self);
        }
        Err(CalculatorError::InputCellNotFound(
            "no available input".into(),
        ))
    }

    /// Remove input cell by index, which may fail if index out of range
    pub fn remove_input(&mut self, index: usize) -> Result<CellInputEx> {
        if self.inputs.len() <= index {
            return Err(CalculatorError::InputCellNotFound(
                "input index out of range".into(),
            ));
        }
        Ok(self.inputs.remove(index))
    }

    /// Pop the last input cell, which may fail if no input cell
    pub fn pop_input(&mut self) -> Result<CellInputEx> {
        self.inputs
            .pop()
            .ok_or_else(|| CalculatorError::InputCellNotFound("no input to pop".into()))
    }

    /// Get output cell by index, which may fail if index out of range
    ///
    /// `index` is 0-based, or a Lua-style relative value: `-1` (also
    /// `usize::MAX`) is the last output, `-2` the second-to-last.
    pub fn get_output_by_index(&self, output_index: usize) -> Result<&CellOutputEx> {
        match resolve_get_index(output_index, self.outputs.len()) {
            (Some(at), _) => Ok(&self.outputs[at]),
            (None, true) => Err(CalculatorError::OutputCellNotFound("no output".into())),
            (None, false) => Err(CalculatorError::OutputCellNotFound(
                "output index out of range".into(),
            )),
        }
    }

    /// Push a single output cell
    pub fn output(&mut self, cell_output: CellOutputEx) -> &mut Self {
        self.outputs.push(cell_output);
        self
    }

    /// Push a output cell from lock script
    pub fn output_from_script(
        &mut self,
        lock_script: ScriptEx,
        data: Vec<u8>,
    ) -> Result<&mut Self> {
        let lock = lock_script.to_script(self)?;
        let draft = CellOutput::new_builder().lock(lock).build();
        let cap = occupied_capacity_shannons(&draft, data.len());
        let output = draft.as_builder().capacity(cap).build();
        Ok(self.output(CellOutputEx::new(output, data)))
    }

    /// Push a batch of output cells
    pub fn outputs(&mut self, cell_outputs: Vec<CellOutputEx>) -> &mut Self {
        self.outputs.extend(cell_outputs);
        self
    }

    /// Remove output cell by index, which may fail if index out of range
    pub fn remove_output(&mut self, index: usize) -> Result<CellOutputEx> {
        if self.outputs.len() <= index {
            return Err(CalculatorError::OutputCellNotFound(
                "output index out of range".into(),
            ));
        }
        Ok(self.outputs.remove(index))
    }

    /// Pop the last output cell, which may fail if no output cell
    pub fn pop_output(&mut self) -> Result<CellOutputEx> {
        self.outputs
            .pop()
            .ok_or_else(|| CalculatorError::OutputCellNotFound("no output to pop".into()))
    }

    /// Get cell dep by index, which may fail if index out of range
    ///
    /// `index` is 0-based, or a Lua-style relative value: `-1` (also
    /// `usize::MAX`) is the last cell dep, `-2` the second-to-last.
    pub fn get_celldep_by_index(&self, celldep_index: usize) -> Result<&CellDepEx> {
        match resolve_get_index(celldep_index, self.celldeps.len()) {
            (Some(at), _) => Ok(&self.celldeps[at]),
            (None, true) => Err(CalculatorError::CellDepNotFound(
                "transaction celldep empty".into(),
            )),
            (None, false) => Err(CalculatorError::CellDepNotFound(
                "transaction celldep index out of range".into(),
            )),
        }
    }

    /// Push a single cell dep
    pub fn celldep(&mut self, cell_dep: CellDepEx) -> &mut Self {
        if !self.celldeps.contains(&cell_dep) {
            self.celldeps.push(cell_dep);
        }
        self
    }

    /// Check if cell dep exists
    pub fn contains_celldep(&self, cell_dep: &CellDepEx) -> bool {
        self.celldeps.contains(cell_dep)
    }

    /// Check if cell dep exists by name
    pub fn get_celldep_by_name(&self, name: &str) -> Option<&CellDepEx> {
        self.celldeps.iter().find(|celldep| celldep.name == name)
    }

    /// Push a batch of cell deps
    pub fn celldeps(&mut self, cell_deps: Vec<CellDepEx>) -> &mut Self {
        cell_deps.into_iter().for_each(|v| {
            if !self.celldeps.contains(&v) {
                self.celldeps.push(v);
            }
        });
        self
    }

    /// Push a single header dep
    pub fn headerdep(&mut self, header_dep: HeaderDepEx) -> &mut Self {
        if let Some(headerdep) = self.headerdeps.iter_mut().find(|v| v == &&header_dep) {
            header_dep
                .cellinput_outpoints
                .into_iter()
                .for_each(|outpoint| {
                    if !headerdep.cellinput_outpoints.contains(&outpoint) {
                        headerdep.cellinput_outpoints.push(outpoint);
                    }
                });
        } else {
            self.headerdeps.push(header_dep);
        }
        self
    }

    /// Push a single witness
    pub fn witness(&mut self, witness: WitnessEx) -> &mut Self {
        self.witnesses.push(witness);
        self
    }

    /// Push a batch of witnesses
    pub fn witnesses(&mut self, witnesses: Vec<WitnessEx>) -> &mut Self {
        self.witnesses.extend(witnesses);
        self
    }

    /// Accumulate total input cells' capacity in shannons.
    pub fn total_inputs_capacity(&self) -> u64 {
        self.inputs
            .iter()
            .map(|input| input.output.capacity_shannons())
            .fold(0u64, |acc, x| acc.saturating_add(x))
    }

    /// Accumulate total output cells' capacity in shannons.
    pub fn total_outputs_capacity(&self) -> u64 {
        self.outputs
            .iter()
            .map(|output| output.capacity_shannons())
            .fold(0u64, |acc, x| acc.saturating_add(x))
    }

    /// Outputs minus inputs, saturating at zero (shannons).
    pub fn needed_capacity(&self) -> u64 {
        self.total_outputs_capacity()
            .saturating_sub(self.total_inputs_capacity())
    }

    /// Inputs minus outputs, saturating at zero (shannons).
    pub fn exceeded_capacity(&self) -> u64 {
        self.total_inputs_capacity()
            .saturating_sub(self.total_outputs_capacity())
    }

    /// Lock script groups of input and output cells
    pub fn lock_script_groups(&self, lock_script: &ScriptEx) -> (Vec<usize>, Vec<usize>) {
        let mut input_groups = Vec::new();
        let mut output_groups = Vec::new();
        for (i, input) in self.inputs.iter().enumerate() {
            if lock_script == &input.output.lock_script() {
                input_groups.push(i);
            }
        }
        for (i, output) in self.outputs.iter().enumerate() {
            if lock_script == &output.lock_script() {
                output_groups.push(i);
            }
        }
        (input_groups, output_groups)
    }

    /// Type id for `outputs[out_index]`: blake2b(first_input || le64(out_index)).
    pub fn calc_type_id(&self, out_index: usize) -> Result<Hash256> {
        let Some(first_input) = self.inputs.first() else {
            return Err(CalculatorError::InputCellNotFound("empty input".into()));
        };
        let mut hasher = Blake2bBuilder::new(32)
            .personal(b"ckb-default-hash")
            .build();
        hasher.update(first_input.input.as_slice());
        hasher.update(&(out_index as u64).to_le_bytes());
        let mut type_id = [0u8; 32];
        hasher.finalize(&mut type_id);
        Ok(type_id)
    }

    /// Find CelldepEx by script, support both type and data hash
    pub fn find_celldep_by_script(&self, script: &ScriptEx) -> Option<(usize, &CellDepEx)> {
        if let ScriptEx::Reference(name, _) = script {
            return self
                .celldeps
                .iter()
                .enumerate()
                .find_map(|(index, celldep)| {
                    if &celldep.name == name {
                        Some((index, celldep))
                    } else {
                        None
                    }
                });
        }
        let index = self
            .celldeps
            .iter()
            .enumerate()
            .find_map(|(index, celldep)| {
                let expected_code_hash =
                    match (script.hash_type(), &celldep.output, celldep.with_data) {
                        (Ok(ScriptHashType::Type), output, _) => {
                            output.calc_type_hash().unwrap_or_default()
                        }
                        (Ok(_), output, true) => output.data_hash(),
                        _ => Hash256::default(),
                    };
                if script.code_hash().unwrap_or_default() == expected_code_hash {
                    Some(index)
                } else {
                    None
                }
            });
        index.map(|index| (index, &self.celldeps[index]))
    }

    /// Pack the five CKB fields as a molecule `Transaction` (no host view).
    pub fn into_packed_transaction(self) -> packed::Transaction {
        let inputs: Vec<CellInput> = self.inputs.into_iter().map(|v| v.input).collect();
        let cell_deps: Vec<CellDep> = self.celldeps.into_iter().map(|v| v.celldep).collect();
        let mut outputs = Vec::new();
        let mut outputs_data = Vec::new();
        for v in self.outputs {
            outputs.push(v.output);
            outputs_data.push(v.data.pack());
        }
        let witnesses: Vec<Bytes> = self
            .witnesses
            .into_iter()
            .map(|v| v.into_packed_bytes())
            .collect();
        let header_deps: Vec<packed::Byte32> = self
            .headerdeps
            .into_iter()
            .map(|v| pack_hash(&v.block_hash))
            .collect();
        packed::Transaction::new_builder()
            .raw(
                packed::RawTransaction::new_builder()
                    .cell_deps(cell_deps.pack())
                    .header_deps(header_deps.pack())
                    .inputs(inputs.pack())
                    .outputs(outputs.pack())
                    .outputs_data(outputs_data.pack())
                    .build(),
            )
            .witnesses(witnesses.pack())
            .build()
    }
}
