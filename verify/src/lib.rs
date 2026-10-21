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
//! are 1–5 and framework codes 10–11 (verify tree) or 12–18 (optional SSRI).
//!
//! # Target
//!
//! Build scripts with `riscv64imac-unknown-none-elf`. The contract Makefile
//! passes `-C passes=lower-atomic`.
//!
//! # SSRI
//!
//! Feature `ssri` (on by default) adds the `SSRI { }` arm of [`cinnabar_main!`],
//! [`ssri::SsriSource`], and [`ssri::SsriArgs`]. It depends on git
//! [`ckb-ssri-std`](https://github.com/ashuralyk/ckb-ssri-std) at `8c2c245`
//! (ckb-std 1.1).
//!
//! See the workspace `AGENTS.md` for the generate / test / deploy path.

#![no_std]
#![warn(missing_docs)]
extern crate alloc;

mod error;
pub mod intent;
#[cfg(feature = "ssri")]
pub mod ssri;
#[cfg(feature = "ssri")]
#[doc(hidden)]
pub use ckb_cinnabar_verifier_macros::expand_ssri_methods;
#[cfg(feature = "ssri")]
pub use ckb_ssri_std::ssri_methods;
mod utils;
mod verification;

pub use error::*;
pub use utils::*;
pub use verification::*;

/// Shared types a contract can import without taking its own dependency.
pub mod re_exports {
    pub use ckb_cinnabar_calculator::{SHANNONS_PER_BYTE, TYPE_ID_CODE_HASH};
    pub use ckb_ssri_std;
    pub use ckb_std;
}
