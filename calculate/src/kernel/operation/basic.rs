//! Generic cell / witness operations used by every kernel instruction.
//!
//! These fill Inputs, Outputs, CellDeps, and Witnesses without assuming a
//! particular type script. Lookups that need a live chain go through
//! [`Source`] or [`RPC`]. Host-only address / signing ops live in
//! [`crate::shell::operation::basic`].

use alloc::{boxed::Box, string::String, vec::Vec};

use crate::kernel::{
    error::{CalculatorError, Result},
    indexer::{CellQueryOptions, GetCellsIter, SearchKey, SearchMode, ValueRangeOption},
    operation::{Log, Operation},
    rpc::RPC,
    skeleton::{
        CellDepEx, CellInputEx, CellOutputEx, HeaderDepEx, ScriptEx, TransactionSkeleton,
        WitnessEx, TYPE_ID_CODE_HASH,
    },
    source::Source,
    types::{
        occupied_capacity_shannons, pack_hash,
        packed::{CellDep, CellInput, CellOutput, OutPoint, Script},
        Builder, DepType, Entity, Hash256, Pack,
    },
};

/// Operation that add input cell to transaction skeleton by out point directly.
pub struct AddInputCellByOutPoint {
    /// Hash of the transaction that created the cell.
    pub tx_hash: Hash256,
    /// Output index inside `tx_hash`.
    pub index: u32,
    /// Optional `since` constraint placed on the input.
    pub since: Option<u64>,
}

/// Operation that add output cell to transaction skeleton.
///
/// # Parameters
/// - `absolute_capacity` bool, whether mark the `capacity` as absolute value or additional
/// - `type_id`: bool, if true, calculate type id and override into type script if provided
#[derive(Default)]
pub struct AddOutputCell {
    /// Lock script of the new cell.
    pub lock_script: ScriptEx,
    /// Optional type script of the new cell.
    pub type_script: Option<ScriptEx>,
    /// Capacity in shannons; absolute value when `absolute_capacity` is true,
    /// otherwise added on top of the cell's minimal occupied capacity.
    pub capacity: u64,
    /// Cell data.
    pub data: Vec<u8>,
    /// Treat `capacity` as the final value instead of an extra over occupied.
    pub absolute_capacity: bool,
    /// Compute a type id from the first input + this output's index and use it
    /// as the type script args.
    pub type_id: bool,
}

/// How [`AddOutputCellByInputIndex`] rewrites the copied cell's capacity.
#[derive(Default)]
pub enum CapacityAdjustment {
    /// Keep the input cell's capacity unchanged.
    #[default]
    Keep,
    /// Rebuild with the exact occupied capacity for the (possibly new) data.
    BuildExact,
    /// Add shannons to the input cell's capacity.
    Add(u64),
    /// Subtract shannons from the input cell's capacity (saturating at 0).
    Subtract(u64),
}

/// Operation that add output cell to transaction skeleton by copying input cell from target position.
///
/// # Parameters
/// - `input_index`: usize, the index of input cell in inputs, if it is usize::MAX, copy the last one
/// - `adjust_capacity`: how to rewrite capacity when `data` / scripts change
#[derive(Default)]
pub struct AddOutputCellByInputIndex {
    /// Index into `skeleton.inputs`; `usize::MAX` copies the last input.
    pub input_index: usize,
    /// Replace the copied cell's data when set.
    pub data: Option<Vec<u8>>,
    /// Replace the copied cell's lock script when set.
    pub lock_script: Option<ScriptEx>,
    /// `Some(Some(t))` sets type script `t`; `Some(None)` removes the type
    /// script; `None` keeps the input's type script.
    pub type_script: Option<Option<ScriptEx>>,
    /// How to adjust the capacity of the copied cell.
    pub adjust_capacity: CapacityAdjustment,
}

/// Operation that add witness in form of WitnessArgs to transaction skeleton.
///
/// `witness_index`: Option<usize>, the index of witness to update, if None, add a new witness
pub struct AddWitnessArgs {
    /// Index of an existing witness to overwrite; `None` appends a new one.
    pub witness_index: Option<usize>,
    /// `WitnessArgs.lock` field (e.g. a signature).
    pub lock: Vec<u8>,
    /// `WitnessArgs.input_type` field.
    pub input_type: Vec<u8>,
    /// `WitnessArgs.output_type` field.
    pub output_type: Vec<u8>,
}

/// Push a pre-built input (inject path).
pub struct AddInjectedInput {
    pub input: CellInputEx,
}

impl<S: Source> Operation<S> for AddInjectedInput {
    fn run(
        self: Box<Self>,
        _source: &S,
        skeleton: &mut TransactionSkeleton,
        _log: &mut Log,
    ) -> Result<()> {
        skeleton.input(self.input)?.witness(Default::default());
        Ok(())
    }
}

/// Push a pre-built named cell dep (inject path).
pub struct AddInjectedCellDep {
    pub celldep: CellDepEx,
}

impl<S: Source> Operation<S> for AddInjectedCellDep {
    fn run(
        self: Box<Self>,
        _source: &S,
        skeleton: &mut TransactionSkeleton,
        _log: &mut Log,
    ) -> Result<()> {
        skeleton.celldep(self.celldep);
        Ok(())
    }
}

impl<S: Source> Operation<S> for AddInputCellByOutPoint {
    fn run(
        self: Box<Self>,
        source: &S,
        skeleton: &mut TransactionSkeleton,
        _log: &mut Log,
    ) -> Result<()> {
        let out_point = OutPoint::new_builder()
            .tx_hash(pack_hash(&self.tx_hash))
            .index(self.index)
            .build();
        let output = source.find_cell_by_out_point(&out_point)?;
        let data = source.find_cell_data_by_out_point(&out_point)?;
        let input = CellInput::new_builder()
            .previous_output(out_point)
            .since(self.since.unwrap_or(0))
            .build();
        skeleton
            .input(CellInputEx::new(input, output, Some(data)))?
            .witness(Default::default());
        Ok(())
    }
}

/// Add a cell dep by type script via [`Source`].
pub struct AddCellDepByType {
    pub name: String,
    pub type_script: ScriptEx,
    pub dep_type: DepType,
    pub with_data: bool,
}

impl<S: Source> Operation<S> for AddCellDepByType {
    fn run(
        self: Box<Self>,
        source: &S,
        skeleton: &mut TransactionSkeleton,
        _log: &mut Log,
    ) -> Result<()> {
        if skeleton.get_celldep_by_name(&self.name).is_some() {
            return Ok(());
        }
        let type_script = self.type_script.to_script(skeleton)?;
        let out_point = source.find_out_point_by_type(&type_script)?;
        let output = source.find_cell_by_out_point(&out_point)?;
        let data = if self.with_data {
            Some(source.find_cell_data_by_out_point(&out_point)?)
        } else {
            None
        };
        let cell_dep = CellDep::new_builder()
            .out_point(out_point)
            .dep_type(self.dep_type)
            .build();
        skeleton.celldep(CellDepEx::new(self.name, cell_dep, output, data));
        Ok(())
    }
}

impl AddOutputCell {
    /// Apply without touching [`Source`].
    pub fn apply(self, skeleton: &mut TransactionSkeleton) -> Result<()> {
        let type_script = if self.type_id {
            let type_id = skeleton.calc_type_id(skeleton.outputs.len())?;
            let type_script = self
                .type_script
                .map(|v| v.set_args(type_id.to_vec()))
                .unwrap_or_else(|| ScriptEx::new_type(TYPE_ID_CODE_HASH, type_id.to_vec()));
            Some(type_script.to_script(skeleton)?)
        } else {
            self.type_script
                .map(|v| v.to_script(skeleton))
                .transpose()?
        };
        let draft = CellOutput::new_builder()
            .lock(self.lock_script.to_script(skeleton)?)
            .type_(type_script.pack())
            .build();
        let occupied = occupied_capacity_shannons(&draft, self.data.len());
        let cap = if self.absolute_capacity {
            if self.capacity < occupied {
                return Err(CalculatorError::InsufficientCapacity(
                    "capacity is less than minimal capacity".into(),
                ));
            }
            self.capacity
        } else {
            occupied.saturating_add(self.capacity)
        };
        let output = draft.as_builder().capacity(cap).build();
        skeleton.output(CellOutputEx::new(output, self.data));
        Ok(())
    }
}

impl<S: Source> Operation<S> for AddOutputCell {
    fn run(
        self: Box<Self>,
        _source: &S,
        skeleton: &mut TransactionSkeleton,
        _log: &mut Log,
    ) -> Result<()> {
        self.apply(skeleton)
    }
}

impl AddOutputCellByInputIndex {
    pub fn apply(self, skeleton: &mut TransactionSkeleton) -> Result<()> {
        let cell_input = skeleton.get_input_by_index(self.input_index)?;
        let mut cell_output = cell_input.output.clone();
        let old_capacity = cell_output.capacity_shannons();
        let mut output_builder = cell_output.output.as_builder();
        if let Some(data) = self.data {
            cell_output.data = data;
        }
        if let Some(lock_script) = self.lock_script {
            output_builder = output_builder.lock(lock_script.to_script(skeleton)?);
        }
        if let Some(type_script) = self.type_script {
            if let Some(type_script) = type_script {
                output_builder =
                    output_builder.type_(Some(type_script.to_script(skeleton)?).pack());
            } else {
                output_builder = output_builder.type_(None::<Script>.pack());
            }
        }
        let output = output_builder.build();
        cell_output.output = match self.adjust_capacity {
            CapacityAdjustment::Keep => output,
            CapacityAdjustment::BuildExact => {
                let cap = occupied_capacity_shannons(&output, cell_output.data.len());
                output.as_builder().capacity(cap).build()
            }
            CapacityAdjustment::Add(change) => output
                .as_builder()
                .capacity(old_capacity.saturating_add(change))
                .build(),
            CapacityAdjustment::Subtract(change) => output
                .as_builder()
                .capacity(old_capacity.saturating_sub(change))
                .build(),
        };
        skeleton.output(cell_output);
        Ok(())
    }
}

impl<S: Source> Operation<S> for AddOutputCellByInputIndex {
    fn run(
        self: Box<Self>,
        _source: &S,
        skeleton: &mut TransactionSkeleton,
        _log: &mut Log,
    ) -> Result<()> {
        self.apply(skeleton)
    }
}

impl AddWitnessArgs {
    pub fn apply(self, skeleton: &mut TransactionSkeleton) -> Result<()> {
        if let Some(witness_index) = self.witness_index {
            if witness_index >= skeleton.witnesses.len() {
                return Err(CalculatorError::Other("witness index out of range".into()));
            }
            let witness = &mut skeleton.witnesses[witness_index];
            witness.lock = self.lock;
            witness.input_type = self.input_type;
            witness.output_type = self.output_type;
        } else {
            skeleton.witness(WitnessEx::new(self.lock, self.input_type, self.output_type));
        }
        Ok(())
    }
}

impl<S: Source> Operation<S> for AddWitnessArgs {
    fn run(
        self: Box<Self>,
        _source: &S,
        skeleton: &mut TransactionSkeleton,
        _log: &mut Log,
    ) -> Result<()> {
        self.apply(skeleton)
    }
}

/// Add a cell dep by transaction hash and output index.
pub struct AddCellDep {
    /// Unique name in the skeleton; later `ScriptEx::Reference` lookups use it.
    pub name: String,
    /// Hash of the transaction containing the dep cell.
    pub tx_hash: Hash256,
    /// Output index inside `tx_hash`.
    pub index: u32,
    /// `DepType::Code` or `DepType::DepGroup`.
    pub dep_type: DepType,
    /// Fetch and keep the cell data (needed when a script references the dep
    /// by `Data1` code hash).
    pub with_data: bool,
}

impl<C: RPC> Operation<C> for AddCellDep {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        if skeleton.get_celldep_by_name(&self.name).is_none() {
            let cell_dep = CellDepEx::new_from_outpoint(
                rpc,
                self.name,
                self.tx_hash,
                self.index,
                self.dep_type,
                self.with_data,
            )?;
            skeleton.celldep(cell_dep);
        }
        Ok(())
    }
}

/// Add a cell dep whose type script is a type-id with `type_args`.
pub struct AddCellDepByTypeId {
    /// Unique name in the skeleton.
    pub name: String,
    /// Type-ID args of the dep cell's type script.
    pub type_args: Hash256,
    /// `DepType::Code` or `DepType::DepGroup`.
    pub dep_type: DepType,
    /// Fetch and keep the cell data.
    pub with_data: bool,
}

impl<C: RPC> Operation<C> for AddCellDepByTypeId {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        Box::new(AddCellDepByType {
            name: self.name,
            type_script: ScriptEx::new_type(TYPE_ID_CODE_HASH, self.type_args.to_vec()),
            dep_type: self.dep_type,
            with_data: self.with_data,
        })
        .run(rpc, skeleton, log)
    }
}

/// Add a standalone header dep by block hash.
pub struct AddHeaderDep {
    /// Hash of the block whose header is added.
    pub block_hash: Hash256,
}

impl<C: RPC> Operation<C> for AddHeaderDep {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        skeleton.headerdep(HeaderDepEx::new(rpc, self.block_hash, Vec::new())?);
        Ok(())
    }
}

/// Add a header dep by block number.
pub struct AddHeaderDepByBlockNumber {
    /// Height of the block whose header is added.
    pub block_number: u64,
}

impl<C: RPC> Operation<C> for AddHeaderDepByBlockNumber {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        skeleton.headerdep(HeaderDepEx::new_from_block_number(rpc, self.block_number)?);
        Ok(())
    }
}

/// Add a header dep for the block that committed `skeleton.inputs[input_index]`.
pub struct AddHeaderDepByInputIndex {
    /// Index into `skeleton.inputs`.
    pub input_index: usize,
}

impl<C: RPC> Operation<C> for AddHeaderDepByInputIndex {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let cell_outpoint = skeleton
            .get_input_by_index(self.input_index)?
            .input
            .previous_output();
        skeleton.headerdep(HeaderDepEx::new_from_outpoint(rpc, cell_outpoint)?);
        Ok(())
    }
}

/// Add a header dep for the block that committed `skeleton.celldeps[celldep_index]`.
pub struct AddHeaderDepByCellDepIndex {
    /// Index into `skeleton.celldeps`.
    pub celldep_index: usize,
}

impl<C: RPC> Operation<C> for AddHeaderDepByCellDepIndex {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let out_point = skeleton
            .get_celldep_by_index(self.celldep_index)?
            .celldep
            .out_point();
        skeleton.headerdep(HeaderDepEx::new_from_outpoint(rpc, out_point)?);
        Ok(())
    }
}

/// Add input cells whose lock script matches, via indexer search.
pub struct AddInputCell {
    /// Lock script cells must carry.
    pub lock_script: ScriptEx,
    /// `Some(Some(t))` requires type script `t`; `Some(None)` requires *no*
    /// type script; `None` ignores the type script.
    pub type_script: Option<Option<ScriptEx>>,
    /// Page size when iterating the indexer (max cells added per batch).
    pub count: u32,
    /// Only match cells with empty data.
    pub skip_data: bool,
    /// How the indexer matches script args (exact / prefix / partial).
    pub search_mode: SearchMode,
}

impl AddInputCell {
    fn search_key(&self, skeleton: &TransactionSkeleton) -> Result<SearchKey> {
        let mut query = CellQueryOptions::new_lock(self.lock_script.clone().to_script(skeleton)?);
        if let Some(type_script) = &self.type_script {
            if let Some(type_script) = type_script {
                query.secondary_script = Some(type_script.clone().to_script(skeleton)?);
            } else {
                query.secondary_script_len_range = Some(ValueRangeOption::new(0, 1));
            }
        }
        if self.skip_data {
            query.data_len_range = Some(ValueRangeOption::new(0, 1));
        }
        query.with_data = Some(true);
        query.script_search_mode = Some(self.search_mode);
        Ok(query.into())
    }
}

impl<C: RPC> Operation<C> for AddInputCell {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let mut iter = GetCellsIter::new(rpc, self.search_key(skeleton)?);
        let mut find_available = false;
        while let Some(cells) = iter.next_batch(self.count)? {
            for cell in cells {
                find_available = true;
                skeleton
                    .input(CellInputEx::new_from_live_cell(cell, None))?
                    .witness(Default::default());
            }
        }
        if !find_available {
            return Err(CalculatorError::InputCellNotFound(
                "input cell not found".into(),
            ));
        }
        Ok(())
    }
}

/// Add input cells whose type script matches, via indexer search.
pub struct AddInputCellByType {
    /// Type script the cells must carry.
    pub type_script: ScriptEx,
    /// Page size when iterating the indexer.
    pub count: u32,
    /// How the indexer matches script args.
    pub search_mode: SearchMode,
}

impl AddInputCellByType {
    fn search_key(&self, skeleton: &TransactionSkeleton) -> Result<SearchKey> {
        let mut query = CellQueryOptions::new_type(self.type_script.clone().to_script(skeleton)?);
        query.script_search_mode = Some(self.search_mode);
        query.with_data = Some(true);
        Ok(query.into())
    }
}

impl<C: RPC> Operation<C> for AddInputCellByType {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let mut iter = GetCellsIter::new(rpc, self.search_key(skeleton)?);
        let mut find_available = false;
        while let Some(cells) = iter.next_batch(self.count)? {
            for cell in cells {
                find_available = true;
                skeleton
                    .input(CellInputEx::new_from_live_cell(cell, None))?
                    .witness(Default::default());
            }
        }
        if !find_available {
            return Err(CalculatorError::InputCellNotFound(
                "input cell not found".into(),
            ));
        }
        Ok(())
    }
}
