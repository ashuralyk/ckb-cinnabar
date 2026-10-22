//! Shared `no_std` vocabulary used by both off-chain Calculate and on-chain Verify.
//!
//! These [`intent`] constants are morphology names. Use the same constant on
//! `Instruction::named` and in `cinnabar_main!` only when a hop is exactly that
//! morphology, so those names cannot drift. Any other hop uses a domain string.

#![no_std]
#![warn(missing_docs)]

pub mod intent;
