//! Shared `no_std` vocabulary used by both off-chain Calculate and on-chain Verify.
//!
//! Keep intent strings in this crate so an [`intent`] name cannot drift between
//! `Instruction::named` and a `cinnabar_main!` verification-tree node.

#![no_std]

pub mod intent;
