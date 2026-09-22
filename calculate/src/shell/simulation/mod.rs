//! Offline chain: in-memory RPC plus a native CKB-VM runner.
//!
//! Typical contract test:
//! 1. Seed [`FakeRpcClient`] with live cells (`insert_fake_cell`) and/or
//!    operations such as [`AddFakeContractCelldepByName`].
//! 2. Assemble with [`crate::instruction::Instruction`]s.
//! 3. Assert the VM exit code via [`expect_verify`] / `assert_verify!`
//!    (`0` = success; non-zero = on-chain `define_errors!` `i8`).
//!
//! Script rejections are [`crate::error::CalculatorError::ScriptValidation`];
//! inspect [`crate::error::CalculatorError::script_exit_code`] instead of
//! parsing display strings.

mod operation;
mod rpc;

#[cfg(not(target_arch = "wasm32"))]
mod simulator;

pub use operation::*;
pub use rpc::*;

#[cfg(not(target_arch = "wasm32"))]
pub use simulator::*;
