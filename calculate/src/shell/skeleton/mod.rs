//! Packed transaction workspace plus RPC helpers and [`ChangeReceiver`].
//!
//! Packed types and local math live in [`crate::kernel::skeleton`].

pub use crate::kernel::skeleton::*;

mod view;
pub use view::ChangeReceiver;
