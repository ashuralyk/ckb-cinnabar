//! Spend recipes for the timelock and threshold-multisig example locks.
//!
//! Tests seed live cells and the contract cell dep. These recipes only
//! describe the spend: collect each input by its previous out point (the
//! same lookup a real RPC serves) and, for the threshold lock, fill the
//! witness that was appended with that input.

use ckb_cinnabar_calculator::{
    error::{CalculatorError, Result},
    instruction::Instruction,
    operation::{
        basic::{AddInputCellByOutPoint, AddOutputCell},
        Log, Operation,
    },
    rpc::RPC,
    skeleton::{ScriptEx, TransactionSkeleton},
    types::Hash256,
};

/// One timelock input, looked up by the out point that created it.
pub struct TimelockInput {
    /// Transaction that created the cell.
    pub tx_hash: Hash256,
    /// Output index inside `tx_hash`.
    pub index: u32,
    /// RFC 0017 `since` placed on this input.
    pub since: u64,
}

/// Fill `WitnessArgs.lock` on the witness paired with the input just added.
///
/// `AddInputCellByOutPoint` appends a default witness in lockstep with the
/// input. Overwriting the last witness keeps that pairing when earlier
/// instructions have already pushed inputs, instead of assuming index 0.
pub struct SetLastWitness {
    /// `WitnessArgs.lock` bytes (concatenated 32-byte preimages).
    pub lock: Vec<u8>,
}

impl<C> Operation<C> for SetLastWitness {
    fn run(
        self: Box<Self>,
        _ctx: &C,
        skeleton: &mut TransactionSkeleton,
        _log: &mut Log,
    ) -> Result<()> {
        let witness = skeleton
            .witnesses
            .last_mut()
            .ok_or_else(|| CalculatorError::Other("no witness to fill".into()))?;
        witness.lock = self.lock;
        Ok(())
    }
}

/// Spend one or more timelock cells into an output with the same lock.
///
/// Each [`TimelockInput`] is collected with `AddInputCellByOutPoint`, so
/// `since` is set on the cell the caller named. Callers seed those cells
/// (FakeRpc in tests, a live indexer otherwise) before running the instruction.
pub fn spend_timelock<T: RPC>(
    lock: ScriptEx,
    inputs: Vec<TimelockInput>,
    capacity_out: u64,
) -> Instruction<T> {
    let mut ops: Vec<Box<dyn Operation<T>>> = inputs
        .into_iter()
        .map(|input| {
            Box::new(AddInputCellByOutPoint {
                tx_hash: input.tx_hash,
                index: input.index,
                since: Some(input.since),
            }) as Box<dyn Operation<T>>
        })
        .collect();
    ops.push(Box::new(AddOutputCell {
        lock_script: lock,
        type_script: None,
        data: Vec::new(),
        capacity: capacity_out,
        absolute_capacity: true,
        type_id: false,
    }));
    Instruction::new(ops)
}

/// Spend one threshold-multisig cell. `preimages` is the witness lock field
/// (concatenated 32-byte preimages) on the input this recipe appends.
pub fn spend_threshold<T: RPC>(
    lock: ScriptEx,
    tx_hash: Hash256,
    index: u32,
    preimages: Vec<u8>,
    capacity_out: u64,
) -> Instruction<T> {
    Instruction::new(vec![
        Box::new(AddInputCellByOutPoint {
            tx_hash,
            index,
            since: None,
        }),
        Box::new(SetLastWitness { lock: preimages }),
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
