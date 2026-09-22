//! Off-chain intent names shared with `ckb_cinnabar_verifier::intent`.
//!
//! Put these on [`Instruction::named`](crate::instruction::Instruction::named)
//! and on `cinnabar_main!` verification nodes so Calculate and Verify share a
//! vocabulary.

pub use ckb_cinnabar_core::intent::*;

/// Unstructured [`crate::operation::Log`] keys. Keep in sync with on-chain
/// debug / off-chain consumers.
pub mod log {
    /// DAO phase-one withdraw: searched deposit capacity (le u64).
    pub const DAO_WITHDRAW_PHASE_ONE: &str = "DAO_WITHDRAW_PHASE_ONE";
    /// DAO phase-two withdraw: actual unlocked capacity (le u64).
    pub const DAO_WITHDRAW_PHASE_TWO: &str = "DAO_WITHDRAW_PHASE_TWO";
    /// xUDT mint/transfer amount (le u128).
    pub const XUDT_AMOUNT: &str = "XUDT_AMOUNT";
    /// Spore: lock script of the cluster owner.
    pub const CLUSTER_CELL_OWNER_LOCK: &str = "CLUSTER_CELL_OWNER_LOCK";
    /// Spore: id of a freshly minted cluster.
    pub const NEW_CLUSTER_ID: &str = "NEW_CLUSTER_ID";
    /// Spore: id of a freshly minted spore.
    pub const NEW_SPORE_ID: &str = "NEW_SPORE_ID";
}
