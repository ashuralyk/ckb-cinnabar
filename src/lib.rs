#![allow(dead_code)]

//! Cinnabar CLI and library: deploy, migrate, consume, and list CKB contracts.
//!
//! # Calculate vs Verify
//!
//! Off-chain assembly lives in [`calculator`]; on-chain checks live in
//! `ckb_cinnabar_verifier`. This crate is the **deployment** surface: it
//! re-exports the Calculate types a contract project typically needs, and
//! persists [`DeploymentRecord`] JSON under `deployment/<network>/<name>.json`.
//!
//! Intent names (`create` / `transfer` / `burn` / …) come from [`intent`] and
//! must match verification-tree nodes. See the workspace `AGENTS.md`.
//!
//! # Headless CLI
//!
//! The `ckb-cinnabar` binary is a thin wrapper around [`dispatch_async`].
//! Scripts should pass `--json` so both success and failure print one object
//! on stdout. Use `--dry-run` to skip send, and `--privkey-env` for non-
//! interactive secp256k1 signing (otherwise live send still prompts `ckb-cli`).

mod command;
mod handle;
mod object;

pub use calculator::{
    intent, Address, CalculatorError, Instruction, Network, RpcClient, TransactionCalculator,
    TransactionSkeleton,
};
pub use ckb_cinnabar_calculator as calculator;
pub use handle::load_contract_deployment;
pub use object::DeploymentRecord;

/// Classify an eyre report for the CLI process exit code.
///
/// With `--json`, also prints a JSON error envelope (`ok: false`) to stdout
/// and keeps stderr empty. Returns `2` for invalid CLI input, `1` otherwise.
pub fn report_cli_error(error: &ckb_cinnabar_calculator::re_exports::eyre::Report) -> u8 {
    command::report_error(error)
}

/// Async CLI entry (used by the binary).
pub async fn dispatch_async() -> ckb_cinnabar_calculator::re_exports::eyre::Result<()> {
    command::dispatch_commands().await
}

/// Wrap for scripts-manager / embedding: run [`dispatch_async`] on a current
/// runtime and `exit` with [`report_cli_error`] on failure.
pub fn dispatch() {
    let result = tokio::runtime::Runtime::new()
        .expect("tokio runtime")
        .block_on(dispatch_async());
    if let Err(error) = result {
        std::process::exit(i32::from(report_cli_error(&error)));
    }
}
