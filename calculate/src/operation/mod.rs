//! Transaction-field fill-ins consumed by [`crate::instruction::Instruction`].
//!
//! An [`Operation`] is one step: add an input, an output, a cell dep, a
//! witness, a header dep, or a signature. Compose them into an instruction
//! named with a [`crate::intent`] constant.
//!
//! Submodules: [`basic`] (generic cells), [`dao`], [`udt`], [`component`]
//! (`ckb-proxy-locks`), and `spore` (feature-gated, experimental).

pub mod basic;
/// Operations for the `ckb-proxy-locks` component scripts (always-success,
/// type-burn, lock-proxy, ...).
pub mod component;
/// Operations for Spore / Cluster cells. **Experimental** (`--features spore`).
#[cfg(feature = "spore")]
pub mod spore;
/// Operations for xUDT (sUDT-compatible) token cells.
pub mod udt;
pub use common::{Log, Operation};

/// Operations for Nervos DAO deposit / withdraw.
#[cfg(not(target_arch = "wasm32"))]
pub mod dao;

mod common {
    use crate::{rpc::RPC, skeleton::TransactionSkeleton};

    /// Unstructured key/value log emitted while operations run.
    ///
    /// Keys are [`crate::intent::log`] constants; values are raw bytes (e.g.
    /// little-endian amounts) for the caller to decode.
    pub type Log = Vec<(&'static str, Vec<u8>)>;

    /// A single step of transaction assembly.
    ///
    /// Implementations fill one or more fields of the [`TransactionSkeleton`]
    /// (inputs / outputs / cell deps / witnesses / header deps) and may push
    /// entries into `log`. Operations are consumed by [`crate::instruction::Instruction`].
    #[async_trait::async_trait(?Send)]
    pub trait Operation<T: RPC> {
        /// Apply this operation to the skeleton. Consumes `self` so fields can
        /// be moved into the skeleton without cloning.
        async fn run(
            self: Box<Self>,
            rpc: &T,
            skeleton: &mut TransactionSkeleton,
            log: &mut Log,
        ) -> eyre::Result<()>;
    }
}
