//! Host wrappers for the `ckb-proxy-locks` component scripts.
//!
//! Output construction, cell-dep fetch, and indexer search live in
//! [`crate::kernel::operation::component`]. See
//! <https://github.com/ckb-ecofund/ckb-proxy-locks>.

pub use crate::kernel::operation::component::*;

use ckb_types::{h256, H256};

use crate::{rpc::Network, skeleton::ScriptEx, types::hash_to_h256};

/// Component-use simple scripts
///
/// note: migrations please refer to https://github.com/ckb-ecofund/ckb-proxy-locks/tree/main/migrations
pub mod hardcoded {
    use super::*;

    pub const COMPONENT_MAINNET_TX_HASH: H256 =
        h256!("0x10d63a996157d32c01078058000052674ca58d15f921bec7f1dcdac2160eb66b");
    pub const COMPONENT_TESTNET_TX_HASH: H256 =
        h256!("0xb4f171c9c9caf7401f54a8e56225ae21d95032150a87a4678eac3f66a3137b93");

    pub use super::Name;

    /// Component script for `network`. Fake/custom networks use a
    /// `ScriptEx::Reference` resolved from a named cell dep.
    pub fn component_script(network: Network, name: Name, args: &[u8]) -> ScriptEx {
        super::component_script(&network, name, args)
    }

    /// Deployment tx hash of the proxy-locks bundle for `network`.
    pub fn component_tx_hash(network: Network) -> H256 {
        hash_to_h256(&super::component_tx_hash(&network))
    }
}
