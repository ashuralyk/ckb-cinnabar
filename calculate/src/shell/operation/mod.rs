//! Transaction-field fill-ins consumed by [`crate::instruction::Instruction`].
//!
//! Kernel types and [`Operation`] live in [`crate::kernel::operation`]. The
//! submodules below re-export those types and add indexer / RPC / signing
//! ops on top.

pub use crate::kernel::operation::{layout, Log, Operation};

pub mod basic;
pub mod component;
pub mod dao;
pub mod udt;

#[cfg(feature = "spore")]
pub mod spore;
