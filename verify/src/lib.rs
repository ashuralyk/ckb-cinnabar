//! On-chain verification tree for CKB scripts (`no_std`).
//!
//! Split a contract into named nodes, register them with [`cinnabar_main!`],
//! and return the next node name (use [`intent`] constants) or `None` on
//! success. Pair off-chain `ckb_cinnabar_calculator::intent` names with the
//! same strings.
//!
//! Typical root node: call [`this_script_pattern`] at [`ScriptPlace::Lock`]
//! or [`ScriptPlace::Type`], then hop to `intent::CREATE` / `TRANSFER` /
//! `BURN`. Custom errors start at [`CUSTOM_ERROR_START`] (20); system codes
//! are 1–5 and framework codes 10–11 (verify tree) or 12–18 (SSRI).
//!
//! # Target
//!
//! Build scripts with `riscv64imac-unknown-none-elf`. The contract Makefile
//! passes `-C passes=lower-atomic`. This crate is `no_std` and also
//! type-checks on the host, which is what docs.rs renders.
//!
//! # SSRI
//!
//! The `SSRI { }` arm of [`cinnabar_main!`] expands against
//! `ckb-cinnabar-verifier-ssri`. That crate is not published: crates.io
//! `ckb-ssri-std` 0.0.1 still pins ckb-std 0.16, so the door tracks the
//! ckb-std 1.1 git fork. Hop-only contracts do not depend on it.
//!
//! See the workspace `AGENTS.md` for the generate / test / deploy path.

#![no_std]
#![warn(missing_docs)]
extern crate alloc;

mod error;
pub mod intent;
mod utils;
mod verification;

pub use error::*;
pub use utils::*;
pub use verification::*;

/// `ckb-std` re-export so a contract can stay on this crate's version.
pub mod re_exports {
    pub use ckb_std;
}
