//! xUDT (sUDT-compatible) host wrappers.
//!
//! Mint, cell-dep, and transfer collect live in [`crate::kernel::operation::udt`].
//! Pair with [`crate::intent::MINT`] / [`crate::intent::TRANSFER`].
//! Log key: [`crate::intent::log::XUDT_AMOUNT`].

pub use crate::kernel::operation::udt::*;

use ckb_types::{h256, H256};

use crate::{rpc::Network, skeleton::ScriptEx, types::hash_to_h256};

/// Hardcoded xUDT deployment out-points and code hash per network.
///
/// On `Fake`/custom networks the tx hash is a sentinel and the script becomes a
/// `ScriptEx::Reference` resolved from a fake cell dep.
pub mod hardcoded {
    use super::*;

    /// Cell-dep name used in the skeleton for the xUDT contract.
    pub const XUDT_NAME: &str = super::XUDT_NAME;
    /// Genesis out-point tx hash of the xUDT script on mainnet.
    pub const XUDT_MAINNET_TX_HASH: H256 =
        h256!("0xc07844ce21b38e4b071dd0e1ee3b0e27afd8d7532491327f39b786343f558ab7");
    /// Genesis out-point tx hash of the xUDT script on testnet.
    pub const XUDT_TESTNET_TX_HASH: H256 =
        h256!("0xbf6fb538763efec2a70a6a3dcb7242787087e1030c4e7d86585bc63a9d337f5f");
    /// Data1 code hash of the xUDT script (same on mainnet and testnet).
    pub const XUDT_CODE_HASH: H256 =
        h256!("0x50bd8d6680b8b9cf98b73f3c08faf8b2a21914311954118ad6609be6e78a1b95");

    /// xUDT deployment tx hash for `network` (sentinel hash under fake networks).
    pub fn xudt_tx_hash(network: Network) -> H256 {
        hash_to_h256(&super::xudt_tx_hash(&network))
    }

    /// xUDT type script for `network`. Fake/custom networks use a
    /// `ScriptEx::Reference` resolved from the `"xudt"` cell dep.
    pub fn xudt_script(network: Network, args: Vec<u8>) -> ScriptEx {
        super::xudt_script(&network, args)
    }
}
