//! Kernel instructions: named sequences of [`crate::kernel::operation::Operation`]s
//! that assemble one "contract method" worth of a CKB transaction.
//!
//! An [`Instruction`] is the off-chain counterpart of a Verify-tree node.
//! Tag it with [`Instruction::named`] and a [`crate::kernel::intent`] constant so
//! Calculate and Verify share a vocabulary. [`TransactionCalculator`] runs
//! one or more instructions against a [`crate::kernel::skeleton::TransactionSkeleton`].

use alloc::{boxed::Box, vec::Vec};

use crate::kernel::{
    error::Result,
    operation::{Log, Operation},
    skeleton::TransactionSkeleton,
};

/// Instruction is a collection of operations executed in sequence to assemble
/// a [`TransactionSkeleton`].
///
/// Set [`Instruction::name`] to a [`crate::kernel::intent`] constant so off-chain
/// recipes line up with on-chain verification-tree nodes. Unnamed instructions
/// (`Instruction::new`) are for setup / balancing / signing, not Verify hops.
pub struct Instruction<C> {
    /// Verify-tree node this instruction corresponds to. Empty if unnamed.
    pub name: &'static str,
    operations: Vec<Box<dyn Operation<C>>>,
}

impl<C> Default for Instruction<C> {
    fn default() -> Self {
        Instruction {
            name: "",
            operations: Vec::new(),
        }
    }
}

impl<C> Instruction<C> {
    /// Create an unnamed instruction from a list of operations.
    ///
    /// Prefer [`Instruction::named`] for contract-facing recipes so the
    /// instruction maps to a Verify-tree node.
    pub fn new(operations: Vec<Box<dyn Operation<C>>>) -> Self {
        Instruction {
            name: "",
            operations,
        }
    }

    /// Like [`Instruction::new`] but tagged with a [`crate::kernel::intent`] name.
    pub fn named(name: &'static str, operations: Vec<Box<dyn Operation<C>>>) -> Self {
        Instruction { name, operations }
    }

    /// Append one operation to the end of the execution sequence.
    pub fn push(&mut self, operation: Box<dyn Operation<C>>) -> &mut Self {
        self.operations.push(operation);
        self
    }

    /// Remove and return the last operation, if any.
    pub fn pop(&mut self) -> Option<Box<dyn Operation<C>>> {
        self.operations.pop()
    }

    /// Remove and return the operation at `index` (panics if out of bounds).
    pub fn remove(&mut self, index: usize) -> Box<dyn Operation<C>> {
        self.operations.remove(index)
    }

    /// Append a batch of operations.
    pub fn append(&mut self, operations: Vec<Box<dyn Operation<C>>>) -> &mut Self {
        self.operations.extend(operations);
        self
    }

    /// Move all operations out of `instruction` and append them here.
    pub fn merge(&mut self, instruction: Instruction<C>) -> &mut Self {
        self.operations.extend(instruction.operations);
        self
    }

    /// Execute all operations in sequence against `skeleton`.
    pub fn run(self, ctx: &C, skeleton: &mut TransactionSkeleton, log: &mut Log) -> Result<()> {
        for operation in self.operations {
            operation.run(ctx, skeleton, log)?;
        }
        Ok(())
    }
}

/// Runs a list of [`Instruction`]s against a [`TransactionSkeleton`].
///
/// `new_skeleton` starts empty; `apply_skeleton` continues an existing one
/// (for example a tx rebuilt from chain). The accumulated [`Log`] is returned
/// alongside the skeleton.
pub struct TransactionCalculator<C> {
    instructions: Vec<Instruction<C>>,
    log: Log,
}

impl<C> Default for TransactionCalculator<C> {
    fn default() -> Self {
        TransactionCalculator {
            instructions: Vec::new(),
            log: Log::new(),
        }
    }
}

impl<C> TransactionCalculator<C> {
    /// Create a calculator from a list of instructions.
    pub fn new(instructions: Vec<Instruction<C>>) -> Self {
        TransactionCalculator {
            instructions,
            log: Log::new(),
        }
    }

    /// Builder-style: append one instruction.
    pub fn instruction(mut self, instruction: Instruction<C>) -> Self {
        self.instructions.push(instruction);
        self
    }

    /// Run all instructions against an empty skeleton, returning the
    /// assembled skeleton plus the accumulated [`Log`].
    pub fn new_skeleton(self, ctx: &C) -> Result<(TransactionSkeleton, Log)> {
        let mut skeleton = TransactionSkeleton::default();
        let log = self.apply_skeleton(ctx, &mut skeleton)?;
        Ok((skeleton, log))
    }

    /// Run all instructions against an existing skeleton (e.g. one rebuilt
    /// from an on-chain transaction), returning the accumulated [`Log`].
    pub fn apply_skeleton(self, ctx: &C, skeleton: &mut TransactionSkeleton) -> Result<Log> {
        let mut log = self.log;
        for instruction in self.instructions {
            instruction.run(ctx, skeleton, &mut log)?;
        }
        Ok(log)
    }
}
