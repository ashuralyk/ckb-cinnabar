//! Canonical morphology names (`create`, `transfer`, `burn`, plus asset and DAO names).
//!
//! `ckb-cinnabar-calculator` and `ckb-cinnabar-verifier` re-export these constants.
//! When a hop is exactly that morphology, put the same constant on
//! `Instruction::named` and in `cinnabar_main!` so the two sides cannot drift.
//! A hop that is not that morphology uses a domain string from the transition
//! table. Sharing the constants does not require every hop to use one of them.

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
