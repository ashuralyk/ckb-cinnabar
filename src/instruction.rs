use eyre::Result;

use crate::{
    operation::Operation,
    rpc::{RpcClient, RPC},
    skeleton::TransactionSkeleton,
};

/// Instruction is a collection of operations that can be executed in sequence, to assemble transaction skeleton
pub struct Instruction<T: Operation> {
    operations: Vec<T>,
}

impl<T: Operation> Instruction<T> {
    pub fn new(operations: Vec<T>) -> Self {
        Instruction { operations }
    }

    pub fn push_operation(&mut self, operation: T) {
        self.operations.push(operation);
    }

    pub fn pop_operation(&mut self) -> Option<T> {
        self.operations.pop()
    }

    pub async fn run<R: RPC>(self, rpc: &R, skeleton: &mut TransactionSkeleton) -> Result<()> {
        for operation in self.operations {
            operation.run(rpc, skeleton).await?;
        }
        Ok(())
    }
}

/// Taking responsibility for executing instructions and then assembling transaction skeleton
pub struct TransactionCalculator<T: Operation, R: RPC> {
    rpc: R,
    instructions: Vec<Instruction<T>>,
    skeleton: TransactionSkeleton,
}

impl<T: Operation, R: RPC> TransactionCalculator<T, R> {
    pub fn new(rpc: R, instructions: Vec<Instruction<T>>) -> Self {
        TransactionCalculator {
            rpc,
            instructions,
            skeleton: TransactionSkeleton::default(),
        }
    }

    pub fn new_mainnet(instructions: Vec<Instruction<T>>) -> TransactionCalculator<T, RpcClient> {
        let rpc = RpcClient::new_mainnet();
        TransactionCalculator::new(rpc, instructions)
    }

    pub fn new_testnet(instructions: Vec<Instruction<T>>) -> TransactionCalculator<T, RpcClient> {
        let rpc = RpcClient::new_testnet();
        TransactionCalculator::new(rpc, instructions)
    }

    pub fn new_devnet(
        rpc_url: &str,
        instructions: Vec<Instruction<T>>,
    ) -> TransactionCalculator<T, RpcClient> {
        let rpc = RpcClient::new(rpc_url, None);
        TransactionCalculator::new(rpc, instructions)
    }

    pub fn instruction(&mut self, instruction: Instruction<T>) {
        self.instructions.push(instruction);
    }

    pub async fn run(mut self) -> Result<TransactionSkeleton> {
        for instruction in self.instructions {
            instruction.run(&self.rpc, &mut self.skeleton).await?;
        }
        Ok(self.skeleton)
    }
}
