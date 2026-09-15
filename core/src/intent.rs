//! Canonical Cinnabar intent names.
//!
//! Both `ckb-cinnabar-calculator` and `ckb-cinnabar-verifier` re-export these
//! constants, so an instruction and its verification-tree node cannot drift.
//! Put the same constant on `Instruction::named` and in `cinnabar_main!`.

/// Create a cell (script in outputs only).
pub const CREATE: &str = "create";
/// Transfer a cell (script in inputs and outputs).
pub const TRANSFER: &str = "transfer";
/// Burn a cell (script in inputs only).
pub const BURN: &str = "burn";
/// Mint a new asset.
pub const MINT: &str = "mint";
/// Nervos DAO deposit.
pub const DEPOSIT: &str = "deposit";
/// Nervos DAO withdraw (phase one or two).
pub const WITHDRAW: &str = "withdraw";
