use eyre::Result;

use crate::{
    operation::Operation,
    rpc::{RpcClient, RPC},
    skeleton::TransactionSkeleton,
};

pub mod predefined;

pub type DefaultInstruction = Instruction<RpcClient>;

/// Instruction is a collection of operations that can be executed in sequence, to assemble transaction skeleton
pub struct Instruction<T: RPC> {
    operations: Vec<Box<dyn Operation<T>>>,
}

impl<T: RPC> Default for Instruction<T> {
    fn default() -> Self {
        Instruction {
            operations: Vec::new(),
        }
    }
}

impl<T: RPC> Instruction<T> {
    pub fn new(operations: Vec<Box<dyn Operation<T>>>) -> Self {
        Instruction { operations }
    }

    pub fn push(&mut self, operation: Box<dyn Operation<T>>) {
        self.operations.push(operation);
    }

    pub fn pop(&mut self) -> Option<Box<dyn Operation<T>>> {
        self.operations.pop()
    }

    pub fn remove(&mut self, index: usize) -> Box<dyn Operation<T>> {
        self.operations.remove(index)
    }

    pub fn append(&mut self, operations: Vec<Box<dyn Operation<T>>>) {
        self.operations.extend(operations);
    }

    pub fn merge(&mut self, instruction: Instruction<T>) {
        self.operations.extend(instruction.operations);
    }

    /// Execute all operations in sequence to assemble transaction skeleton
    pub async fn run(self, rpc: &T, skeleton: &mut TransactionSkeleton) -> Result<()> {
        for operation in self.operations {
            operation.run(rpc, skeleton).await?;
        }
        Ok(())
    }
}

/// Take responsibility for executing instructions and then assemble transaction skeleton
pub struct TransactionCalculator<T: RPC> {
    instructions: Vec<Instruction<T>>,
}

impl<T: RPC> Default for TransactionCalculator<T> {
    fn default() -> Self {
        TransactionCalculator {
            instructions: Vec::new(),
        }
    }
}

impl<T: RPC> TransactionCalculator<T> {
    pub fn new(instructions: Vec<Instruction<T>>) -> Self {
        TransactionCalculator { instructions }
    }

    pub fn instruction(&mut self, instruction: Instruction<T>) -> &mut Self {
        self.instructions.push(instruction);
        self
    }

    pub async fn new_skeleton(self, rpc: &T) -> Result<TransactionSkeleton> {
        let mut skeleton = TransactionSkeleton::default();
        self.apply_skeleton(rpc, &mut skeleton).await?;
        Ok(skeleton)
    }

    pub async fn apply_skeleton(self, rpc: &T, skeleton: &mut TransactionSkeleton) -> Result<()> {
        for instruction in self.instructions {
            instruction.run(rpc, skeleton).await?;
        }
        Ok(())
    }
}
