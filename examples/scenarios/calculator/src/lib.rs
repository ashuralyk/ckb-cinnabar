//! Spend recipes for the timelock and threshold-multisig example locks.
//!
//! The cell dep that carries the RISC-V binary is added by the test, the
//! same way `templates/contract` separates `prepare` from the business
//! instruction.

use ckb_cinnabar_calculator::{
    error::{CalculatorError, Result},
    instruction::Instruction,
    operation::{
        basic::{AddOutputCell, AddWitnessArgs},
        Log, Operation,
    },
    rpc::RPC,
    simulation::AddFakeInputCell,
    skeleton::{ScriptEx, TransactionSkeleton},
    types::{Builder, Entity},
};

/// Overwrite `since` on an input `AddFakeInputCell` just pushed.
pub struct SetInputSince {
    /// Index in `skeleton.inputs`.
    pub index: usize,
    /// RFC 0017 `since` word.
    pub since: u64,
}

impl<C> Operation<C> for SetInputSince {
    fn run(
        self: Box<Self>,
        _ctx: &C,
        skeleton: &mut TransactionSkeleton,
        _log: &mut Log,
    ) -> Result<()> {
        let cell = skeleton
            .inputs
            .get_mut(self.index)
            .ok_or_else(|| CalculatorError::InputCellNotFound("since index".into()))?;
        cell.input = cell.input.clone().as_builder().since(self.since).build();
        Ok(())
    }
}

/// Spend a timelock cell into an output with the same lock.
pub fn spend_timelock<T: RPC>(
    lock: ScriptEx,
    since: u64,
    capacity_in: u64,
    capacity_out: u64,
) -> Instruction<T> {
    Instruction::new(vec![
        Box::new(AddFakeInputCell {
            lock_script: lock.clone(),
            type_script: None,
            data: Vec::new(),
            capacity: capacity_in,
            absolute_capacity: true,
        }),
        Box::new(SetInputSince { index: 0, since }),
        Box::new(AddOutputCell {
            lock_script: lock,
            type_script: None,
            data: Vec::new(),
            capacity: capacity_out,
            absolute_capacity: true,
            type_id: false,
        }),
    ])
}

/// Spend a threshold-multisig cell. `preimages` is the witness lock field
/// (concatenated 32-byte preimages).
pub fn spend_threshold<T: RPC>(
    lock: ScriptEx,
    preimages: Vec<u8>,
    capacity_in: u64,
    capacity_out: u64,
) -> Instruction<T> {
    Instruction::new(vec![
        Box::new(AddFakeInputCell {
            lock_script: lock.clone(),
            type_script: None,
            data: Vec::new(),
            capacity: capacity_in,
            absolute_capacity: true,
        }),
        Box::new(AddWitnessArgs {
            witness_index: Some(0),
            lock: preimages,
            input_type: Vec::new(),
            output_type: Vec::new(),
        }),
        Box::new(AddOutputCell {
            lock_script: lock,
            type_script: None,
            data: Vec::new(),
            capacity: capacity_out,
            absolute_capacity: true,
            type_id: false,
        }),
    ])
}
