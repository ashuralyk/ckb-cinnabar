use std::sync::Arc;

use ckb_chain_spec::consensus::{Consensus, ConsensusBuilder};
use ckb_cinnabar_calculator::{
    instruction::Instruction,
    re_exports::{
        ckb_types::{
            bytes::Bytes,
            core::{
                cell::ResolvedTransaction,
                hardfork::{HardForks, CKB2021, CKB2023},
                Cycle, HeaderBuilder, HeaderView,
            },
            packed::{self, Byte32, OutPoint},
            prelude::Pack,
        },
        eyre::Result,
    },
    rpc::RPC,
    skeleton::TransactionSkeleton,
};
use ckb_script::{TransactionScriptsVerifier, TxVerifyEnv};
use ckb_traits::{CellDataProvider, ExtensionProvider, HeaderProvider};

use crate::rpc::FakeRpcClient;

pub const DEFUALT_MAX_CYCLES: u64 = 10_000_000;

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

pub struct TransactionSimulator {
    consensus: Consensus,
    env: TxVerifyEnv,
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
        Self { consensus, env }
    }
}

impl TransactionSimulator {
    pub fn verify(
        &self,
        instructions: Vec<Instruction<FakeRpcClient>>,
        max_cycles: u64,
    ) -> Result<Cycle> {
        let rt = tokio::runtime::Runtime::new()?;
        let fake_rpc = FakeRpcClient::default();
        let await_result = self.async_verify(&fake_rpc, instructions, max_cycles);
        rt.block_on(await_result)
    }

    pub async fn async_verify<T: RPC>(
        &self,
        rpc: &T,
        instructions: Vec<Instruction<T>>,
        max_cycles: u64,
    ) -> Result<Cycle> {
        let mut skeleton = TransactionSkeleton::default();
        for instruction in instructions {
            instruction.run(rpc, &mut skeleton).await?;
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
