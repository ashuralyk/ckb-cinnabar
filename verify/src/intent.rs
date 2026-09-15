//! Canonical intent names shared with `ckb_cinnabar_calculator::intent`.
//!
//! Register verification-tree nodes with these constants:
//!
//! ```ignore
//! cinnabar_main!(
//!     Context,
//!     (TREE_ROOT, Root),
//!     (intent::CREATE, Create),
//!     (intent::TRANSFER, Transfer),
//!     (intent::BURN, Burn),
//! );
//! ```

pub use ckb_cinnabar_core::intent::*;
