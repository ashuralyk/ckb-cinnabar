use core::panic::Location;
use std::{collections::HashMap, sync::Arc};

use crate::{
    error::{CalculatorError, Result as CalcResult},
    instruction::Instruction,
    operation::Log,
    rpc::RPC,
    skeleton::TransactionSkeleton,
    types::hash_to_h256,
    TYPE_ID_CODE_HASH,
};
use ckb_chain_spec::consensus::{Consensus, ConsensusBuilder};
use ckb_script::{ScriptError, ScriptGroup, TransactionScriptsVerifier, TxVerifyEnv};
use ckb_traits::{CellDataProvider, ExtensionProvider, HeaderProvider};
use ckb_types::{
    bytes::Bytes,
    core::{
        cell::{CellMeta, ResolvedTransaction},
        hardfork::{HardForks, CKB2021, CKB2023},
        Cycle, HeaderBuilder, HeaderView, ScriptHashType, TransactionInfo,
    },
    packed::{self, Byte32, OutPoint},
    prelude::{Entity, IntoHeaderView, Unpack},
    H256,
};

/// Default cycle cap for [`TransactionSimulator::verify`] (10M, ~ CKB testnet
/// per-transaction limit).
pub const DEFAULT_MAX_CYCLES: u64 = 10_000_000;

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

/// Native CKB-VM runner for a self-custody resolved transaction.
///
/// Used by contract tests: assemble instructions against a [`RPC`] (usually
/// [`crate::simulation::FakeRpcClient`]), resolve cells, and execute every
/// script group. Script rejections become
/// [`CalculatorError::ScriptValidation`].
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
        let tip = HeaderBuilder::default().number(0).build();
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

fn calculator_error_from_script_error(error: ScriptError) -> CalculatorError {
    match error {
        ScriptError::ValidationFailure(script, exit_code) => CalculatorError::ScriptValidation {
            exit_code,
            message: format!("script {script} exited with code {exit_code}"),
        },
        error => CalculatorError::Simulation(error.to_string()),
    }
}

impl TransactionSimulator {
    /// Print the assembled transaction skeleton JSON to stdout before verifying.
    pub fn print_tx(mut self, print_tx: bool) -> Self {
        self.print_tx = print_tx;
        self
    }

    /// Start from a pre-built skeleton instead of an empty one.
    pub fn skeleton(mut self, skeleton: TransactionSkeleton) -> Self {
        self.skeleton = Some(skeleton);
        self
    }

    /// Link cell out-points to their committing block headers, filling
    /// `TransactionInfo` (required for `since` / time-locked scripts, e.g. DAO).
    pub fn link_cell_to_header(mut self, link: Vec<(OutPoint, HeaderView)>) -> Self {
        link.into_iter().for_each(|(outpoint, header)| {
            self.outpoint_to_headers.insert(outpoint, header);
        });
        self
    }

    /// Synchronous wrapper of [`TransactionSimulator::async_verify`] on a
    /// fresh current-thread runtime.
    #[track_caller]
    pub fn verify<T: RPC>(
        self,
        rpc: &T,
        instructions: Vec<Instruction<T>>,
        max_cycles: u64,
    ) -> CalcResult<Cycle> {
        let caller = Location::caller();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| CalculatorError::Other(e.to_string()))?
            .block_on(self.async_verify_at(caller, rpc, instructions, max_cycles))
    }

    /// Run `instructions` against `rpc`, resolve the resulting transaction,
    /// and execute every script group in a native CKB-VM. Returns consumed
    /// cycles on success; script rejections surface as
    /// [`CalculatorError::ScriptValidation`].
    ///
    /// Prints `[ckb-vm] file:line consumed cycles: N`, including when a script
    /// exits non-zero. `file:line` is this function when called directly;
    /// [`crate::assert_verify`] prints the test line.
    pub async fn async_verify<T: RPC>(
        self,
        rpc: &T,
        instructions: Vec<Instruction<T>>,
        max_cycles: u64,
    ) -> CalcResult<Cycle> {
        self.async_verify_at(vm_call_site(), rpc, instructions, max_cycles)
            .await
    }

    async fn async_verify_at<T: RPC>(
        self,
        caller: &Location<'static>,
        rpc: &T,
        instructions: Vec<Instruction<T>>,
        max_cycles: u64,
    ) -> CalcResult<Cycle> {
        let mut skeleton = self.skeleton.unwrap_or_default();
        let mut log = Log::new();
        for instruction in instructions {
            instruction.run(rpc, &mut skeleton, &mut log)?;
        }
        if self.print_tx {
            println!("transaction skeleton: {}", skeleton);
        }
        log.into_iter()
            .for_each(|(name, msg)| println!("[calculate log] {name} -> {}", hex::encode(msg)));
        let headers = skeleton
            .headerdeps
            .iter()
            .map(|v| (hash_to_h256(&v.block_hash), v.header.clone().into_view()))
            .collect();
        let resolved_tx = {
            let mut resolved_tx = skeleton
                .into_resolved_transaction(rpc)
                .map_err(CalculatorError::from)?;
            complete_resolved_tx(self.outpoint_to_headers, &mut resolved_tx);
            Arc::new(resolved_tx)
        };
        let context = Context::new(resolved_tx.clone(), headers);
        let consensus = Arc::new(self.consensus.clone());
        let env = Arc::new(self.env.clone());
        let verifier = TransactionScriptsVerifier::new_with_debug_printer(
            resolved_tx,
            context,
            consensus,
            env,
            Arc::new(|_id, msg| {
                println!("[contract debug] {}", msg);
            }),
        );
        let (cycles, result) = execute_groups(&verifier, max_cycles);
        println!(
            "[ckb-vm] {}:{} consumed cycles: {cycles}",
            caller.file(),
            caller.line()
        );
        result
    }
}

/// Run each script group. A non-zero exit still counts that group's cycles.
fn execute_groups(
    verifier: &TransactionScriptsVerifier<Context>,
    max_cycles: u64,
) -> (u64, CalcResult<Cycle>) {
    let groups: Vec<ScriptGroup> = verifier.groups().map(|(_, group)| group.clone()).collect();
    let mut cycles = 0u64;
    for group in groups {
        let remain = match max_cycles.checked_sub(cycles) {
            Some(remain) => remain,
            None => {
                return (
                    cycles,
                    Err(CalculatorError::Simulation(format!(
                        "cycles overflow after {cycles}"
                    ))),
                );
            }
        };
        match run_group(verifier, &group, remain) {
            Ok(used) => match cycles.checked_add(used) {
                Some(total) => cycles = total,
                None => {
                    return (
                        cycles,
                        Err(CalculatorError::Simulation(format!(
                            "cycles overflow after {cycles}"
                        ))),
                    );
                }
            },
            Err((used, error)) => {
                let total = cycles.saturating_add(used);
                return (total, Err(error));
            }
        }
    }
    (cycles, Ok(cycles))
}

fn run_group(
    verifier: &TransactionScriptsVerifier<Context>,
    group: &ScriptGroup,
    max_cycles: u64,
) -> Result<u64, (u64, CalculatorError)> {
    if is_type_id(&group.script) {
        let hash = group.script.calc_script_hash();
        return match verifier.verify_single(group.group_type, &hash, max_cycles) {
            Ok(used) => Ok(used),
            Err(error) => Err((0, calculator_error_from_script_error(error))),
        };
    }
    match verifier.detailed_run(group, max_cycles) {
        Ok(result) if result.exit_code == 0 => Ok(result.consumed_cycles),
        Ok(result) => Err((
            result.consumed_cycles,
            calculator_error_from_script_error(ScriptError::validation_failure(
                &group.script,
                result.exit_code,
            )),
        )),
        Err(error) => Err((0, calculator_error_from_script_error(error))),
    }
}

fn is_type_id(script: &packed::Script) -> bool {
    let hash_type: u8 = script.hash_type().into();
    script.code_hash().as_slice() == TYPE_ID_CODE_HASH.as_slice()
        && hash_type == ScriptHashType::Type as u8
}

/// Location of the VM run that should appear in the cycles line.
///
/// Synchronous so `#[track_caller]` records the caller. [`crate::assert_verify`]
/// calls this at the test.
#[doc(hidden)]
#[track_caller]
pub fn vm_call_site() -> &'static Location<'static> {
    Location::caller()
}

/// Assemble `instructions` and assert the CKB-VM exit code.
///
/// Returns consumed cycles when the exit code matches. `0` is success.
/// Non-zero matches an on-chain script `i8` (see `define_errors!`).
/// The VM run prints `[ckb-vm] file:line consumed cycles: N`.
pub async fn expect_verify<T: RPC>(
    rpc: &T,
    instructions: Vec<Instruction<T>>,
    expected_exit_code: i8,
) -> CalcResult<u64> {
    expect_verify_at(vm_call_site(), rpc, instructions, expected_exit_code).await
}

/// [`expect_verify`] with an explicit callsite for the cycles line.
#[doc(hidden)]
pub async fn expect_verify_at<T: RPC>(
    caller: &'static Location<'static>,
    rpc: &T,
    instructions: Vec<Instruction<T>>,
    expected_exit_code: i8,
) -> CalcResult<u64> {
    let result = TransactionSimulator::default()
        .async_verify_at(caller, rpc, instructions, DEFAULT_MAX_CYCLES)
        .await;
    match (expected_exit_code, result) {
        (0, Ok(cycles)) => Ok(u64::from(cycles)),
        (0, Err(err)) => Err(CalculatorError::Simulation(format!(
            "expected success, got {err}"
        ))),
        (code, Ok(cycles)) => Err(CalculatorError::Simulation(format!(
            "expected exit {code}, got success ({cycles} cycles)"
        ))),
        (code, Err(err)) => match err.script_exit_code() {
            Some(got) if got == code => Ok(0),
            Some(got) => Err(CalculatorError::Simulation(format!(
                "expected exit {code}, got {got} ({err})"
            ))),
            None => Err(CalculatorError::Simulation(format!(
                "expected exit {code}, got non-validation error: {err}"
            ))),
        },
    }
}

#[allow(clippy::mutable_key_type)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use ckb_script::ScriptError;

    #[test]
    fn validation_failure_keeps_structured_exit_code() {
        let error = calculator_error_from_script_error(ScriptError::ValidationFailure(
            "test-script".to_string(),
            20,
        ));

        assert_eq!(error.kind(), "script_validation");
        assert_eq!(error.script_exit_code(), Some(20));
        assert!(error.message().contains("test-script"));
        let json = serde_json::to_value(error).expect("serialize CalculatorError");
        assert_eq!(json["exit_code"], 20);
    }
}
