use std::{collections::HashMap, sync::Arc};

use ckb_chain_spec::consensus::{Consensus, ConsensusBuilder};
use ckb_script::{TransactionScriptsVerifier, TxVerifyEnv};
use ckb_traits::{CellDataProvider, ExtensionProvider, HeaderProvider};
use ckb_types::{
    bytes::Bytes,
    core::{
        cell::{CellMeta, ResolvedTransaction},
        hardfork::{HardForks, CKB2021, CKB2023},
        Cycle, HeaderBuilder, HeaderView, TransactionInfo,
    },
    packed::{self, Byte32, OutPoint},
    prelude::{Pack, Unpack},
    H256,
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
    headers: HashMap<H256, HeaderView>,
}

impl Context {
    pub fn new(resolved_tx: Arc<ResolvedTransaction>, headers: HashMap<H256, HeaderView>) -> Self {
        Context {
            resolved_tx,
            headers,
        }
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
    fn get_header(&self, hash: &Byte32) -> Option<HeaderView> {
        self.headers.get(&hash.unpack()).cloned()
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
    outpoint_to_headers: HashMap<OutPoint, HeaderView>,
    skeleton: Option<TransactionSkeleton>,
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
            outpoint_to_headers: HashMap::new(),
            skeleton: None,
        }
    }
}

impl TransactionSimulator {
    pub fn print_tx(mut self, print_tx: bool) -> Self {
        self.print_tx = print_tx;
        self
    }

    pub fn skeleton(mut self, skeleton: TransactionSkeleton) -> Self {
        self.skeleton = Some(skeleton);
        self
    }

    pub fn link_cell_to_header(mut self, outpoint: OutPoint, header: HeaderView) -> Self {
        self.outpoint_to_headers.insert(outpoint, header);
        self
    }

    pub fn verify<T: RPC>(
        self,
        rpc: &T,
        instructions: Vec<Instruction<T>>,
        max_cycles: u64,
    ) -> Result<Cycle> {
        let rt = tokio::runtime::Runtime::new()?;
        let await_result = self.async_verify(rpc, instructions, max_cycles);
        rt.block_on(await_result)
    }

    pub async fn async_verify<T: RPC>(
        self,
        rpc: &T,
        instructions: Vec<Instruction<T>>,
        max_cycles: u64,
    ) -> Result<Cycle> {
        let mut skeleton = self.skeleton.unwrap_or_default();
        let mut log = Log::new();
        for instruction in instructions {
            instruction.run(rpc, &mut skeleton, &mut log).await?;
        }
        if self.print_tx {
            println!("transaction skeleton: {}", skeleton);
        }
        let headers = skeleton
            .headerdeps
            .iter()
            .map(|v| (v.block_hash.clone(), v.header.clone()))
            .collect();
        let resolved_tx = {
            let mut resolved_tx = skeleton.into_resolved_transaction(rpc).await?;
            complete_resolved_tx(self.outpoint_to_headers, &mut resolved_tx);
            Arc::new(resolved_tx)
        };
        let context = Context::new(resolved_tx.clone(), headers);
        let consensus = Arc::new(self.consensus.clone());
        let env = Arc::new(self.env.clone());
        let mut verifier = TransactionScriptsVerifier::new(resolved_tx, context, consensus, env);
        verifier.set_debug_printer(|_id, msg| {
            println!("[contract debug] {}", msg);
        });
        Ok(verifier.verify(max_cycles)?)
    }
}

fn complete_resolved_tx(
    outpoint_to_headers: HashMap<OutPoint, HeaderView>,
    resolved_tx: &mut ResolvedTransaction,
) {
    let complete_cell_meta = |cell_meta: &mut CellMeta| {
        if let Some(header) = outpoint_to_headers.get(&cell_meta.out_point) {
            cell_meta.transaction_info = Some(TransactionInfo {
                block_number: header.number(),
                block_epoch: header.epoch(),
                block_hash: header.hash(),
                index: 0,
            });
        }
    };
    for resolved_input in resolved_tx.resolved_inputs.iter_mut() {
        complete_cell_meta(resolved_input);
    }
    for resolved_cell_dep in resolved_tx.resolved_cell_deps.iter_mut() {
        complete_cell_meta(resolved_cell_dep);
    }
}
