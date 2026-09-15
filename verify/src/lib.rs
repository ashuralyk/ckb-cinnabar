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
//! are 1–5 and framework codes 10–11.
//!
//! See the workspace `AGENTS.md` for the generate / test / deploy path.

#![no_std]
extern crate alloc;

mod error;
pub mod intent;
mod utils;
mod verification;

pub use error::*;
pub use utils::*;
pub use verification::*;

pub mod re_exports {
    pub use ckb_std;
}
