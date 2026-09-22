//! Kernel transaction-field fill-ins consumed by [`crate::kernel::instruction::Instruction`].
//!
//! An [`Operation`] is one step: add an input, an output, a cell dep, a
//! witness, a header dep, or a signature. Compose them into an instruction
//! named with a [`crate::kernel::intent`] constant.
//!
//! Chain-read ops bound [`crate::kernel::rpc::RPC`]. Inject-only ops bind
//! [`crate::kernel::source::Source`] (a derivable subset of `RPC`).

use alloc::{boxed::Box, vec::Vec};

use crate::kernel::{error::Result, skeleton::TransactionSkeleton};

/// Unstructured key/value log emitted while operations run.
///
/// Keys are [`crate::kernel::intent::log`] constants; values are raw bytes
/// (e.g. little-endian amounts) for the caller to decode.
pub type Log = Vec<(&'static str, Vec<u8>)>;

/// Shared byte layouts (xUDT, DAO deposit). Spore molecule tables live in
/// [`spore::schema`] when the `spore` feature is on.
pub mod layout;

pub mod basic;
pub mod component;
pub mod dao;
pub mod udt;

#[cfg(feature = "spore")]
pub mod spore;

/// A single step of transaction assembly.
///
/// Implementations fill one or more fields of the [`TransactionSkeleton`]
/// (inputs / outputs / cell deps / witnesses / header deps) and may push
/// entries into `log`. Operations are consumed by
/// [`crate::kernel::instruction::Instruction`].
pub trait Operation<C> {
    /// Apply this operation to the skeleton. Consumes `self` so fields can
    /// be moved into the skeleton without cloning.
    fn run(
        self: Box<Self>,
        ctx: &C,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()>;
}
