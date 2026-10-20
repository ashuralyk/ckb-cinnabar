//! In-process FakeRpc simulation for the MCP `simulate` tool.
//!
//! The supported recipe is `always_success_transfer`: an always-success lock
//! moves capacity from one cell to another. It needs no RISC-V contract
//! binary. Contract projects should keep using `assert_verify!` in `make test`.

use ckb_cinnabar_calculator::{
    instruction::Instruction,
    operation::basic::AddOutputCell,
    simulation::{
        always_success_script, AddFakeAlwaysSuccessCelldep, AddFakeInputCell, FakeRpcClient,
        TransactionSimulator, DEFAULT_MAX_CYCLES,
    },
};
use serde_json::{json, Value};

/// Result of one local VM run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationReport {
    /// `0` when the script accepts the transaction.
    pub exit_code: i8,
    /// Cycles consumed on success. `0` when the script rejects.
    pub cycles: u64,
    /// `CalculatorError::kind()` when assembly or the VM fails outside a script exit.
    pub kind: Option<String>,
    /// Human-readable detail.
    pub message: String,
}

/// Run `always_success_transfer`.
///
/// `capacity_in` and `capacity_out` are shannons. The always-success script
/// does not enforce conservation; a caller that needs a capacity failure
/// should use a real contract test.
pub async fn always_success_transfer(capacity_in: u64, capacity_out: u64) -> SimulationReport {
    let rpc = FakeRpcClient::default();
    let lock = always_success_script(Vec::new());
    let instruction = Instruction::new(vec![
        Box::new(AddFakeAlwaysSuccessCelldep {}),
        Box::new(AddFakeInputCell {
            lock_script: lock.clone().into(),
            type_script: None,
            data: Vec::new(),
            capacity: capacity_in,
            absolute_capacity: true,
        }),
        Box::new(AddOutputCell {
            lock_script: lock.into(),
            type_script: None,
            data: Vec::new(),
            capacity: capacity_out,
            absolute_capacity: true,
            type_id: false,
        }),
    ]);
    match TransactionSimulator::default()
        .async_verify(&rpc, vec![instruction], DEFAULT_MAX_CYCLES)
        .await
    {
        Ok(cycles) => SimulationReport {
            exit_code: 0,
            cycles: u64::from(cycles),
            kind: None,
            message: "always-success script accepted the transaction".into(),
        },
        Err(err) => SimulationReport {
            exit_code: err.script_exit_code().unwrap_or(-1),
            cycles: 0,
            kind: Some(err.kind().to_string()),
            message: err.message().to_string(),
        },
    }
}

/// Dispatch `simulate` from a tool argument object.
pub async fn simulate(arguments: &Value) -> Result<Value, String> {
    let recipe = arguments
        .get("recipe")
        .and_then(Value::as_str)
        .unwrap_or("always_success_transfer");
    if recipe != "always_success_transfer" {
        return Err(
            "unsupported recipe; use always_success_transfer (capacity_in, capacity_out)".into(),
        );
    }
    let capacity_in = arguments
        .get("capacity_in")
        .and_then(Value::as_u64)
        .unwrap_or(200_000_000_000);
    let capacity_out = arguments
        .get("capacity_out")
        .and_then(Value::as_u64)
        .unwrap_or(100_000_000_000);
    let report = always_success_transfer(capacity_in, capacity_out).await;
    Ok(json!({
        "recipe": recipe,
        "exit_code": report.exit_code,
        "cycles": report.cycles,
        "kind": report.kind,
        "message": report.message,
    }))
}
