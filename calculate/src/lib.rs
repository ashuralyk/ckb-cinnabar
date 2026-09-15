//! Off-chain CKB transaction assembly for the Cinnabar framework.
//!
//! Compose [`operation::Operation`] values into an
//! [`instruction::Instruction`], then run them through
//! [`TransactionCalculator`] to produce a [`skeleton::TransactionSkeleton`].
//!
//! Intent names in [`intent`] must match on-chain verification nodes from
//! `ckb_cinnabar_verifier::intent`. See the workspace `AGENTS.md` for the
//! generate → verify → simulate → deploy path.
//!
//! ```
//! use ckb_cinnabar_calculator::intent;
//! assert_eq!(intent::TRANSFER, "transfer");
//! assert_eq!(intent::CREATE, "create");
//! ```

pub mod address;
pub mod error;
pub mod indexer;
pub mod instruction;
pub mod intent;
pub mod operation;
pub mod rpc;
pub mod simulation;
pub mod skeleton;

pub use address::Address;
pub use error::{script_exit_code, CalculatorError, Result};
pub use instruction::{DefaultInstruction, Instruction, TransactionCalculator};
pub use rpc::{Network, RpcClient, MAINNET_RPC_URL, RPC, TESTNET_RPC_URL};
pub use skeleton::{ScriptEx, TransactionSkeleton, TYPE_ID_CODE_HASH};

/// Assert CKB-VM exit code after assembling `instructions` against `rpc`.
///
/// Must be used inside an `async` test or runtime. `0` means success.
///
/// ```ignore
/// ckb_cinnabar_calculator::assert_verify!(&rpc, instructions, 0).unwrap();
/// ```
#[macro_export]
macro_rules! assert_verify {
    ($rpc:expr, $instructions:expr, $expected_exit:expr) => {
        $crate::simulation::expect_verify($rpc, $instructions, $expected_exit).await
    };
}

// Re-exports to eliminate the need for downstream dependencies to specify the version of ckb_* crates
pub mod re_exports {
    pub use async_trait;
    pub use ckb_hash;
    pub use ckb_jsonrpc_types;
    pub use ckb_types;
    pub use eyre;
    pub use secp256k1;
    pub use tokio;

    #[cfg(not(target_arch = "wasm32"))]
    pub use ckb_sdk;
}
