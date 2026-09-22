//! Instructions: named sequences of [`crate::operation::Operation`]s that assemble one
//! "contract method" worth of a CKB transaction.
//!
//! Kernel types live in [`crate::kernel::instruction`]. This module re-exports
//! them and adds native recipes such as `secp256k1_sighash_transfer`.

pub use crate::kernel::instruction::*;

use crate::rpc::RpcClient;

#[cfg(not(target_arch = "wasm32"))]
pub mod predefined;

/// [`Instruction`] bound to the default JSON-RPC [`RpcClient`].
pub type DefaultInstruction = Instruction<RpcClient>;
