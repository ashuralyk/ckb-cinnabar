//! Spore / Cluster host wrappers. **Experimental** (`--features spore`).
//!
//! Encode, collect, cell-dep fetch, and cobuild actions live in
//! [`crate::kernel::operation::spore`]. Pair mint/transfer/burn with
//! [`crate::intent::MINT`] / [`crate::intent::TRANSFER`] / [`crate::intent::BURN`].

pub use crate::kernel::operation::spore::*;

use ckb_types::{h256, H256};

use crate::{rpc::Network, skeleton::ScriptEx, types::hash_to_h256};

/// The latest Spore and Cluster contract version
///
/// note: detail refers to https://github.com/sporeprotocol/spore-contract/blob/master/docs/VERSIONS.md
pub mod hardcoded {
    use super::*;

    pub const SPORE_MAINNET_TX_HASH: H256 =
        h256!("0x96b198fb5ddbd1eed57ed667068f1f1e55d07907b4c0dbd38675a69ea1b69824");
    pub const SPORE_MAINNET_CODE_HASH: H256 =
        h256!("0x4a4dce1df3dffff7f8b2cd7dff7303df3b6150c9788cb75dcf6747247132b9f5");

    pub const SPORE_TESTNET_TX_HASH: H256 =
        h256!("0x5e8d2a517d50fd4bb4d01737a7952a1f1d35c8afc77240695bb569cd7d9d5a1f");
    pub const SPORE_TESTNET_CODE_HASH: H256 =
        h256!("0x685a60219309029d01310311dba953d67029170ca4848a4ff638e57002130a0d");

    pub const CLUSTER_MAINNET_TX_HASH: H256 =
        h256!("0xe464b7fb9311c5e2820e61c99afc615d6b98bdefbe318c34868c010cbd0dc938");
    pub const CLUSTER_MAINNET_CODE_HASH: H256 =
        h256!("0x7366a61534fa7c7e6225ecc0d828ea3b5366adec2b58206f2ee84995fe030075");

    pub const CLUSTER_TESTNET_TX_HASH: H256 =
        h256!("0xcebb174d6e300e26074aea2f5dbd7f694bb4fe3de52b6dfe205e54f90164510a");
    pub const CLUSTER_TESTNET_CODE_HASH: H256 =
        h256!("0x0bbe768b519d8ea7b96d58f1182eb7e6ef96c541fbd9526975077ee09f049058");

    /// Latest Spore deployment tx hash for `network` (sentinel hash under fake networks).
    pub fn spore_tx_hash(network: Network) -> H256 {
        hash_to_h256(&super::spore_tx_hash(&network))
    }

    /// Spore type script for `network`; fake/custom use `ScriptEx::Reference("spore", args)`.
    pub fn spore_script(network: Network, args: Vec<u8>) -> ScriptEx {
        super::spore_script(&network, args)
    }

    /// Latest Cluster deployment tx hash for `network` (sentinel hash under fake networks).
    pub fn cluster_tx_hash(network: Network) -> H256 {
        hash_to_h256(&super::cluster_tx_hash(&network))
    }

    /// Cluster type script for `network`; fake/custom use `ScriptEx::Reference("cluster", args)`.
    pub fn cluster_script(network: Network, args: Vec<u8>) -> ScriptEx {
        super::cluster_script(&network, args)
    }
}

pub mod hookkey {
    pub use crate::intent::log::{CLUSTER_CELL_OWNER_LOCK, NEW_CLUSTER_ID, NEW_SPORE_ID};
}
