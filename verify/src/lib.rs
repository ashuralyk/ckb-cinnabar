#![no_std]
extern crate alloc;

pub mod error;
pub mod verification;

/// Re-exports internal modules for conflict-less integration
pub mod re_exports {
    pub use ckb_std;
}
