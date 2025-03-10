use async_trait::async_trait;
use ckb_types::{core::DepType, h256, H256};
use eyre::Result;

use crate::{
    operation::{Log, Operation},
    rpc::{Network, RPC},
    skeleton::TransactionSkeleton,
};

use super::basic::AddCellDep;

/// Component-use simple scripts
///
/// note: migrations please refer to https://github.com/ckb-ecofund/ckb-proxy-locks/tree/main/migrations
pub mod hardcoded {
    use crate::simulation::random_hash;

    use super::*;

    pub const XUDT_MAINNET_TX_HASH: H256 =
        h256!("0xc07844ce21b38e4b071dd0e1ee3b0e27afd8d7532491327f39b786343f558ab7");
    pub const XUDT_TESTNET_TX_HASH: H256 =
        h256!("0xbf6fb538763efec2a70a6a3dcb7242787087e1030c4e7d86585bc63a9d337f5f");

    lazy_static::lazy_static! {
        pub static ref XUDT_FAKENET_TX_HASH: H256 = random_hash().into();
    }

    pub const XUDT_CODE_HASH: H256 =
        h256!("0x50bd8d6680b8b9cf98b73f3c08faf8b2a21914311954118ad6609be6e78a1b95");

    pub fn xudt_tx_hash(network: Network) -> H256 {
        match network {
            Network::Mainnet => XUDT_MAINNET_TX_HASH,
            Network::Testnet => XUDT_TESTNET_TX_HASH,
            _ => XUDT_FAKENET_TX_HASH.clone(),
        }
    }
}

pub struct AddXudtCelldep {}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddXudtCelldep {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        Box::new(AddCellDep {
            name: "xudt".to_string(),
            tx_hash: hardcoded::xudt_tx_hash(rpc.network()),
            index: 0,
            dep_type: DepType::Code,
            with_data: false,
        })
        .run(rpc, skeleton, log)
        .await
    }
}
