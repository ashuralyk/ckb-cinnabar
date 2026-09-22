//! Sync chain-read surface used by Calculate operations.
//!
//! [`Node`] is known-identity reads (live cell, header, tip, fee). [`RPC`]
//! combines [`Node`] with [`Indexer`](crate::kernel::indexer::Indexer). HTTP
//! and a future SSRI adapter implement these traits; they are not `std` /
//! `async` themselves.

use crate::kernel::{
    error::Result,
    indexer::LiveCell,
    network::Network,
    types::{packed, Hash256},
};

/// Known-identity chain reads (node RPC, or an SSRI syscall that maps to it).
pub trait Node {
    /// Network this client is targeting; defaults to [`Network::Fake`].
    fn network(&self) -> Network {
        Network::Fake
    }

    /// Live cell at `out_point`. Errors if the cell is missing or not live.
    fn get_live_cell(&self, out_point: &packed::OutPoint, with_data: bool) -> Result<LiveCell>;

    /// Header by block hash.
    fn get_header(&self, hash: &Hash256) -> Result<Option<packed::Header>>;

    /// Header by block number.
    fn get_header_by_number(&self, number: u64) -> Result<Option<packed::Header>>;

    /// Current tip header.
    fn get_tip_header(&self) -> Result<packed::Header>;

    /// Block hash at `number`.
    fn get_block_hash(&self, number: u64) -> Result<Option<Hash256>>;

    /// Current tip height.
    fn get_tip_block_number(&self) -> Result<u64>;

    /// Block hash that committed `tx_hash`, if the transaction is in a block.
    fn get_transaction_block_hash(&self, tx_hash: &Hash256) -> Result<Option<Hash256>>;

    /// Minimal fee rate in shannons/byte (today `tx_pool_info.min_fee_rate`).
    fn min_fee_rate(&self) -> Result<u64>;
}

/// Assembler-facing chain access: [`Node`] + [`Indexer`](crate::kernel::indexer::Indexer).
///
/// Implementors also get [`Source`](crate::kernel::source::Source) via the
/// blanket impl on `Node + Indexer`.
pub trait RPC: Node + crate::kernel::indexer::Indexer {}
