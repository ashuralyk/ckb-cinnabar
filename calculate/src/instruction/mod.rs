//! Instructions: named sequences of [`crate::operation::Operation`]s that assemble one
//! "contract method" worth of a CKB transaction.
//!
//! An [`Instruction`] is the off-chain counterpart of a Verify-tree node.
//! Tag it with [`Instruction::named`] and a [`crate::intent`] constant so
//! Calculate and Verify share a vocabulary. [`TransactionCalculator`] runs
//! one or more instructions against a [`crate::skeleton::TransactionSkeleton`].
//!
//! Native (non-wasm) recipes such as `secp256k1_sighash_transfer` live in
//! `predefined`.

use crate::{
    error::{CalculatorError, Result},
    operation::{Log, Operation},
    rpc::{RpcClient, RPC},
    skeleton::TransactionSkeleton,
};

#[cfg(not(target_arch = "wasm32"))]
pub mod predefined;

/// [`Instruction`] bound to the default JSON-RPC [`RpcClient`].
pub type DefaultInstruction = Instruction<RpcClient>;

/// Instruction is a collection of operations executed in sequence to assemble
/// a [`TransactionSkeleton`].
///
/// Set [`Instruction::name`] to a [`crate::intent`] constant so off-chain
/// recipes line up with on-chain verification-tree nodes. Unnamed instructions
/// (`Instruction::new`) are for setup / balancing / signing, not Verify hops.
pub struct Instruction<T: RPC> {
    /// Verify-tree node this instruction corresponds to. Empty if unnamed.
    pub name: &'static str,
    operations: Vec<Box<dyn Operation<T>>>,
}

impl<T: RPC> Default for Instruction<T> {
    fn default() -> Self {
        Instruction {
            name: "",
            operations: Vec::new(),
        }
    }
}

impl<T: RPC> Instruction<T> {
    /// Create an unnamed instruction from a list of operations.
    ///
    /// Prefer [`Instruction::named`] for contract-facing recipes so the
    /// instruction maps to a Verify-tree node.
    pub fn new(operations: Vec<Box<dyn Operation<T>>>) -> Self {
        Instruction {
            name: "",
            operations,
        }
    }

    /// Like [`Instruction::new`] but tagged with a [`crate::intent`] name.
    pub fn named(name: &'static str, operations: Vec<Box<dyn Operation<T>>>) -> Self {
        Instruction { name, operations }
    }

    /// Append one operation to the end of the execution sequence.
    pub fn push(&mut self, operation: Box<dyn Operation<T>>) -> &mut Self {
        self.operations.push(operation);
        self
    }

    /// Remove and return the last operation, if any.
    pub fn pop(&mut self) -> Option<Box<dyn Operation<T>>> {
        self.operations.pop()
    }

    /// Remove and return the operation at `index` (panics if out of bounds).
    pub fn remove(&mut self, index: usize) -> Box<dyn Operation<T>> {
        self.operations.remove(index)
    }

    /// Append a batch of operations.
    pub fn append(&mut self, operations: Vec<Box<dyn Operation<T>>>) -> &mut Self {
        self.operations.extend(operations);
        self
    }

    /// Move all operations out of `instruction` and append them here.
    pub fn merge(&mut self, instruction: Instruction<T>) -> &mut Self {
        self.operations.extend(instruction.operations);
        self
    }

    /// Execute all operations in sequence against `skeleton`.
    ///
    /// Stops at the first operation error. Individual operations still return
    /// `eyre::Result`; this method maps them to [`CalculatorError`].
    pub async fn run(
        self,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        for operation in self.operations {
            operation
                .run(rpc, skeleton, log)
                .await
                .map_err(CalculatorError::from)?;
        }
        Ok(())
    }
}

/// Runs a list of [`Instruction`]s against a [`TransactionSkeleton`].
///
/// `new_skeleton` starts empty; `apply_skeleton` continues an existing one
/// (for example a tx rebuilt from chain). The accumulated [`Log`] is returned
/// alongside the skeleton.
pub struct TransactionCalculator<T: RPC> {
    instructions: Vec<Instruction<T>>,
    log: Log,
}

impl<T: RPC> Default for TransactionCalculator<T> {
    fn default() -> Self {
        TransactionCalculator {
            instructions: Vec::new(),
            log: Log::new(),
        }
    }
}

impl<T: RPC> TransactionCalculator<T> {
    /// Create a calculator from a list of instructions.
    pub fn new(instructions: Vec<Instruction<T>>) -> Self {
        TransactionCalculator {
            instructions,
            log: Log::new(),
        }
    }

    /// Builder-style: append one instruction.
    pub fn instruction(mut self, instruction: Instruction<T>) -> Self {
        self.instructions.push(instruction);
        self
    }

    /// Run all instructions against an empty skeleton, returning the
    /// assembled skeleton plus the accumulated [`Log`].
    pub async fn new_skeleton(self, rpc: &T) -> Result<(TransactionSkeleton, Log)> {
        let mut skeleton = TransactionSkeleton::default();
        let log = self.apply_skeleton(rpc, &mut skeleton).await?;
        Ok((skeleton, log))
    }

    /// Run all instructions against an existing skeleton (e.g. one rebuilt
    /// from an on-chain transaction), returning the accumulated [`Log`].
    pub async fn apply_skeleton(self, rpc: &T, skeleton: &mut TransactionSkeleton) -> Result<Log> {
        let mut log = self.log;
        for instruction in self.instructions {
            instruction.run(rpc, skeleton, &mut log).await?;
        }
        Ok(log)
    }
}
