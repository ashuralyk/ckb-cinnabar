//! Always-on Calculate kernel: a complete `no_std` + `alloc` assembler.
//!
//! Enable the kernel profile with `--no-default-features`. Chain reads go
//! through sync [`rpc::RPC`] / [`indexer::Indexer`] (HTTP and SSRI are
//! adapters). Inject-only assembly still uses [`source::UnsupportedSource`].
//!
//! Host I/O (HTTP client, tokio, FakeRpc, CKB-VM) lives in [`crate::shell`].

pub mod address;
pub mod error;
pub mod indexer;
pub mod instruction;
pub mod intent;
pub mod network;
pub mod operation;
pub mod rpc;
pub mod skeleton;
pub mod source;
pub mod types;

#[cfg(all(test, not(feature = "std")))]
mod tests;
