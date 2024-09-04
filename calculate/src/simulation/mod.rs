use std::sync::Arc;

use ckb_chain_spec::consensus::{Consensus, ConsensusBuilder};
use ckb_script::{TransactionScriptsVerifier, TxVerifyEnv};
use ckb_traits::{CellDataProvider, ExtensionProvider, HeaderProvider};
use ckb_types::{
    bytes::Bytes,
    core::{
        cell::ResolvedTransaction,
        hardfork::{HardForks, CKB2021, CKB2023},
        Cycle, HeaderBuilder, HeaderView,
    },
    packed::{self, Byte32, OutPoint},
    prelude::Pack,
};
use eyre::Result;

use crate::{instruction::Instruction, operation::Log, rpc::RPC, skeleton::TransactionSkeleton};

mod operation;
mod rpc;

pub use operation::*;
pub use rpc::*;

pub const DEFUALT_MAX_CYCLES: u64 = 10_000_000;

/// Context for a self-custody resolved transaction
#[derive(Clone)]
struct Context {
    resolved_tx: Arc<ResolvedTransaction>,
}

impl Context {
    pub fn new(resolved_tx: Arc<ResolvedTransaction>) -> Self {
        Context { resolved_tx }
    }
}

impl CellDataProvider for Context {
    fn get_cell_data(&self, out_point: &OutPoint) -> Option<Bytes> {
        let metas = [
            self.resolved_tx.resolved_inputs.clone(),
            self.resolved_tx.resolved_cell_deps.clone(),
        ]
        .concat();
        metas.into_iter().find_map(|v| {
            if &v.out_point == out_point {
                Some(v.mem_cell_data.expect("cell data meta"))
            } else {
                None
            }
        })
    }

    fn get_cell_data_hash(&self, out_point: &OutPoint) -> Option<Byte32> {
        let metas = [
            self.resolved_tx.resolved_inputs.clone(),
            self.resolved_tx.resolved_cell_deps.clone(),
        ]
        .concat();
        metas.into_iter().find_map(|v| {
            if &v.out_point == out_point {
                Some(v.mem_cell_data_hash.expect("cell data hash meta"))
            } else {
                None
            }
        })
    }
}

impl HeaderProvider for Context {
    fn get_header(&self, _hash: &Byte32) -> Option<HeaderView> {
        None
    }
}

impl ExtensionProvider for Context {
    fn get_block_extension(&self, _hash: &Byte32) -> Option<packed::Bytes> {
        None
    }
}

/// Onwn a native CKB-VM runner to verify a self-custody resolved transaction
pub struct TransactionSimulator {
    consensus: Consensus,
    env: TxVerifyEnv,
    print_tx: bool,
}

impl Default for TransactionSimulator {
    fn default() -> Self {
        let consensus = ConsensusBuilder::default()
            .hardfork_switch(HardForks {
                ckb2021: CKB2021::new_dev_default(),
                ckb2023: CKB2023::new_dev_default(),
            })
            .build();
        let tip = HeaderBuilder::default().number(0.pack()).build();
        let env = TxVerifyEnv::new_submit(&tip);
        Self {
            consensus,
            env,
            print_tx: false,
        }
    }
}

impl TransactionSimulator {
    pub fn print_tx(&mut self, print_tx: bool) -> &mut Self {
        self.print_tx = print_tx;
        self
    }

    pub fn verify<T: RPC>(
        &self,
        rpc: &T,
        instructions: Vec<Instruction<T>>,
        max_cycles: u64,
        skeleton: Option<TransactionSkeleton>,
    ) -> Result<Cycle> {
        let rt = tokio::runtime::Runtime::new()?;
        let await_result = self.async_verify(rpc, instructions, max_cycles, skeleton);
        rt.block_on(await_result)
    }

    pub async fn async_verify<T: RPC>(
        &self,
        rpc: &T,
        instructions: Vec<Instruction<T>>,
        max_cycles: u64,
        skeleton: Option<TransactionSkeleton>,
    ) -> Result<Cycle> {
        let mut skeleton = skeleton.unwrap_or_default();
        let mut log = Log::new();
        for instruction in instructions {
            instruction.run(rpc, &mut skeleton, &mut log).await?;
        }
        if self.print_tx {
            println!("transaction skeleton: {}", skeleton);
        }
        let resolved_tx = Arc::new(skeleton.into_resolved_transaction(rpc).await?);
        let context = Context::new(resolved_tx.clone());
        let consensus = Arc::new(self.consensus.clone());
        let env = Arc::new(self.env.clone());
        let mut verifier = TransactionScriptsVerifier::new(resolved_tx, context, consensus, env);
        verifier.set_debug_printer(|_id, msg| {
            println!("[contract debug] {}", msg);
        });
        Ok(verifier.verify(max_cycles)?)
    }
}
