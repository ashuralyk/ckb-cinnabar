//! Canonical intent names shared with `ckb_cinnabar_calculator::intent`.
//!
//! Register a node with one of these constants only when that hop is exactly
//! that morphology. Any other hop uses a domain string on both sides.
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
