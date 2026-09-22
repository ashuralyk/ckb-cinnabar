//! Calculate **shell**: the complete `std` host packed as one tree.
//!
//! Kernel types live in [`crate::kernel`]. This layer re-exports them and
//! adds HTTP [`rpc::RpcClient`], indexer JSON wire types, FakeRpc, CKB-VM
//! simulation, signing, and predefined recipes. `block_on` lives inside
//! the HTTP adapter; operations stay sequential and sync.
//!
//! Crate-root `instruction` / `operation` / `skeleton` / `rpc` paths are
//! aliases of these modules so existing imports keep working.

pub mod address;
pub mod indexer;
pub mod instruction;
pub mod operation;
pub mod rpc;
pub mod simulation;
pub mod skeleton;
