use eyre::Result;

use crate::{
    operation::Operation,
    rpc::{RpcClient, RPC},
    skeleton::TransactionSkeleton,
};

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

    pub fn push_operation(&mut self, operation: Box<dyn Operation<T>>) {
        self.operations.push(operation);
    }

    pub fn pop_operation(&mut self) -> Option<Box<dyn Operation<T>>> {
        self.operations.pop()
    }

    pub async fn run(self, rpc: &T, skeleton: &mut TransactionSkeleton) -> Result<()> {
        for operation in self.operations {
            operation.run(rpc, skeleton).await?;
        }
        Ok(())
    }
}

/// Taking responsibility for executing instructions and then assembling transaction skeleton
pub struct TransactionCalculator<T: RPC> {
    rpc: T,
    instructions: Vec<Instruction<T>>,
    skeleton: TransactionSkeleton,
}

impl<T: RPC> TransactionCalculator<T> {
    pub fn new(rpc: T, instructions: Vec<Instruction<T>>) -> Self {
        TransactionCalculator {
            rpc,
            instructions,
            skeleton: TransactionSkeleton::default(),
        }
    }

    pub fn new_mainnet(instructions: Vec<DefaultInstruction>) -> TransactionCalculator<RpcClient> {
        let rpc = RpcClient::new_mainnet();
        TransactionCalculator::new(rpc, instructions)
    }

    pub fn new_testnet(instructions: Vec<DefaultInstruction>) -> TransactionCalculator<RpcClient> {
        let rpc = RpcClient::new_testnet();
        TransactionCalculator::new(rpc, instructions)
    }

    pub fn new_devnet(
        rpc_url: &str,
        instructions: Vec<DefaultInstruction>,
    ) -> TransactionCalculator<RpcClient> {
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
