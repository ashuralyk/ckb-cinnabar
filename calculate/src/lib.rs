//! Off-chain CKB transaction assembly for the Cinnabar framework.
//!
//! Two layers (additive, not exclusive):
//!
//! - **Kernel (always on)** — [`kernel`]: `no_std` + `alloc` packed skeleton,
//!   sync [`rpc::RPC`] / [`indexer::Indexer`], [`source::Source`],
//!   [`operation::Operation`] / [`instruction::Instruction`]. Enable the
//!   kernel profile with `--no-default-features`. HTTP and SSRI are adapters.
//! - **Shell (`std`, default)** — [`shell`]: host adapter packed on top of
//!   the kernel ([`rpc::RpcClient`], tokio, FakeRpc, signing, simulation).
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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod kernel;

#[cfg(feature = "std")]
pub mod shell;

#[cfg(feature = "std")]
#[doc(inline)]
pub use shell::{indexer, instruction, operation, rpc, simulation, skeleton};

#[cfg(not(feature = "std"))]
#[doc(inline)]
pub use kernel::{indexer, instruction, operation, rpc, skeleton};

#[cfg(feature = "std")]
pub use kernel::error::script_exit_code;
pub use kernel::{
    address, error,
    error::{script_exit_code_from_str, CalculatorError, Result},
    indexer::Indexer,
    intent, network,
    network::Network,
    rpc::{Node, RPC},
    source,
    source::{Source, UnsupportedSource},
    types,
    types::{occupied_capacity_shannons, Hash256},
};

pub use address::{Address, AddressPayload};

#[cfg(feature = "std")]
pub use rpc::{Host, RpcClient, MAINNET_RPC_URL, TESTNET_RPC_URL};

#[cfg(feature = "std")]
pub use instruction::DefaultInstruction;
pub use instruction::{Instruction, TransactionCalculator};
pub use skeleton::{ScriptEx, TransactionSkeleton};
pub use types::{SHANNONS_PER_BYTE, TYPE_ID_CODE_HASH};

/// Assert CKB-VM exit code after assembling `instructions` against `rpc`.
///
/// Must be used inside an `async` test or runtime. `0` means success.
///
/// ```ignore
/// ckb_cinnabar_calculator::assert_verify!(&rpc, instructions, 0).unwrap();
/// ```
#[cfg(feature = "std")]
#[macro_export]
macro_rules! assert_verify {
    ($rpc:expr, $instructions:expr, $expected_exit:expr) => {
        $crate::simulation::expect_verify($rpc, $instructions, $expected_exit).await
    };
}

/// Re-exports to eliminate the need for downstream dependencies to specify the
/// version of host `ckb_*` crates.
#[cfg(feature = "std")]
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
