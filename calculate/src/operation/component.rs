use async_trait::async_trait;
use ckb_sdk::{
    rpc::ckb_indexer::{SearchKey, SearchMode},
    traits::CellQueryOptions,
};
use ckb_types::{
    core::{DepType, ScriptHashType},
    h256,
    packed::Script,
    prelude::{Builder, Entity, Pack},
    H256,
};
use eyre::{eyre, Result};

use crate::{
    operation::{basic::AddOutputCell, Log, Operation},
    rpc::{GetCellsIter, Network, RPC},
    skeleton::{CellDepEx, CellInputEx, ScriptEx, TransactionSkeleton},
};

/// Component-use simple scripts
///
/// note: migrations please refer to https://github.com/ckb-ecofund/ckb-proxy-locks/tree/main/migrations
pub mod hardcoded {
    use super::*;

    pub const COMPONENT_MAINNET_TX_HASH: H256 =
        h256!("0x10d63a996157d32c01078058000052674ca58d15f921bec7f1dcdac2160eb66b");
    pub const COMPONENT_TESTNET_TX_HASH: H256 =
        h256!("0xb4f171c9c9caf7401f54a8e56225ae21d95032150a87a4678eac3f66a3137b93");

    pub const ALWAYS_SUCCESS_CODE_HASH: H256 =
        h256!("0x3b521cc4b552f109d092d8cc468a8048acb53c5952dbe769d2b2f9cf6e47f7f1");
    pub const INPUT_TYPE_PROXY_CODE_HASH: H256 =
        h256!("0x5123908965c711b0ffd8aec642f1ede329649bda1ebdca6bd24124d3796f768a");
    pub const OUTPUT_TYPE_PROXY_CODE_HASH: H256 =
        h256!("0x2df53b592db3ae3685b7787adcfef0332a611edb83ca3feca435809964c3aff2");
    pub const LOCK_PROXY_CODE_HASH: H256 =
        h256!("0x2df53b592db3ae3685b7787adcfef0332a611edb83ca3feca435809964c3aff2");
    pub const SINGLE_USE_CODE_HASH: H256 =
        h256!("0x8290467a512e5b9a6b816469b0edabba1f4ac474e28ffdd604c2a7c76446bbaf");
    pub const TYPE_BURN_CODE_HASH: H256 =
        h256!("0xff78bae0abf17d7a404c0be0f9ad9c9185b3f88dcc60403453d5ba8e1f22f53a");

    #[repr(u32)]
    pub enum Name {
        AlwaysSuccess = 0,
        InputTypeProxy,
        OutputTypeProxy,
        LockProxy,
        SingleUse,
        TypeBurn,
    }

    impl std::fmt::Display for Name {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Name::AlwaysSuccess => write!(f, "always_success"),
                Name::InputTypeProxy => write!(f, "input_type_proxy"),
                Name::OutputTypeProxy => write!(f, "output_type_proxy"),
                Name::LockProxy => write!(f, "lock_proxy"),
                Name::SingleUse => write!(f, "single_use"),
                Name::TypeBurn => write!(f, "type_burn"),
            }
        }
    }

    pub fn build_script(name: Name, args: &[u8]) -> Result<Script> {
        Ok(Script::new_builder()
            .code_hash(
                match name {
                    Name::AlwaysSuccess => ALWAYS_SUCCESS_CODE_HASH,
                    Name::InputTypeProxy => INPUT_TYPE_PROXY_CODE_HASH,
                    Name::LockProxy => LOCK_PROXY_CODE_HASH,
                    Name::OutputTypeProxy => OUTPUT_TYPE_PROXY_CODE_HASH,
                    Name::SingleUse => SINGLE_USE_CODE_HASH,
                    Name::TypeBurn => TYPE_BURN_CODE_HASH,
                }
                .pack(),
            )
            .hash_type(ScriptHashType::Data1.into())
            .args(args.pack())
            .build())
    }

    pub fn component_tx_hash(network: Network) -> Result<H256> {
        match network {
            Network::Mainnet => Ok(COMPONENT_MAINNET_TX_HASH),
            Network::Testnet => Ok(COMPONENT_TESTNET_TX_HASH),
            _ => Err(eyre::eyre!("unsupported network")),
        }
    }
}

/// Add `ckb-proxy-locks` celldep
///
/// # Parameters
/// - `name`: component name in `ckb-proxy-locks`
pub struct AddComponentCelldep {
    pub name: hardcoded::Name,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddComponentCelldep {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        skeleton.celldep(
            CellDepEx::new_from_outpoint(
                rpc,
                self.name.to_string(),
                hardcoded::component_tx_hash(rpc.network())?,
                self.name as u32,
                DepType::Code,
                false,
            )
            .await?,
        );
        Ok(())
    }
}

/// Add `type-burn-lock` output cell with or without type script
///
/// # Parameters
/// - `output_index`: reference output index, which is choosed to calculate type hash
/// - `type_script`: optional type script
/// - `data`: cell data
pub struct AddTypeBurnOutputCell {
    pub output_index: usize,
    pub type_script: Option<ScriptEx>,
    pub data: Vec<u8>,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddTypeBurnOutputCell {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let reference_output = skeleton.get_output_by_index(self.output_index)?;
        let reference_type_hash = reference_output
            .calc_type_hash()
            .ok_or(eyre!("reference output has no type script"))?;
        let type_burn_lock_script =
            hardcoded::build_script(hardcoded::Name::TypeBurn, reference_type_hash.as_bytes())?;
        Box::new(AddOutputCell {
            lock_script: type_burn_lock_script.into(),
            type_script: self.type_script,
            capacity: 0,
            data: self.data,
            absolute_capacity: false,
            type_id: false,
        })
        .run(rpc, skeleton, log)
        .await
    }
}

/// Search and add `type-burn-lock` input cell
///
/// # Parameters
/// - `type_hash`: the reference type script hash
/// - `count`: max number of cells to add
pub struct AddTypeBurnInputCell {
    pub type_hash: H256,
    pub count: usize,
}

impl AddTypeBurnInputCell {
    pub fn search_key(&self) -> Result<SearchKey> {
        let type_burn_lock_script =
            hardcoded::build_script(hardcoded::Name::TypeBurn, self.type_hash.as_bytes())?;
        let mut query = CellQueryOptions::new_lock(type_burn_lock_script);
        query.with_data = Some(true);
        Ok(query.into())
    }
}

#[async_trait]
impl<T: RPC> Operation<T> for AddTypeBurnInputCell {
    async fn run(
        mut self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let search_key = self.search_key()?;
        while let Some(indexer_cell) = GetCellsIter::new(rpc, search_key.clone()).next().await? {
            let input = CellInputEx::new_from_indexer_cell(indexer_cell, None);
            skeleton.input(input)?.witness(Default::default());
            self.count -= 1;
            if self.count == 0 {
                break;
            }
        }
        Ok(())
    }
}

/// Add `type-burn-lock` input cell by input index
pub struct AddTypeBurnInputCellByInputIndex {
    pub input_index: usize,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddTypeBurnInputCellByInputIndex {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let type_hash = skeleton
            .get_input_by_index(self.input_index)?
            .output
            .calc_type_hash()
            .ok_or(eyre!("input cell has no type script"))?;
        Box::new(AddTypeBurnInputCell {
            type_hash,
            count: 1,
        })
        .run(rpc, skeleton, log)
        .await
    }
}

/// Add `lock-proxy` output cell with or without type script
///
/// # Parameters
/// - `lock_hash`: the proxied lock hash
/// - `lock_script`: wether the script is used as lock script, otherwise type script
/// - `type_script`: optional type script
/// - `data`: cell data
pub struct AddLockProxyOutputCell {
    pub lock_hash: H256,
    pub lock_script: bool,
    pub second_script: Option<ScriptEx>,
    pub data: Vec<u8>,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddLockProxyOutputCell {
    async fn run(
        mut self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let lock_proxy_script =
            hardcoded::build_script(hardcoded::Name::LockProxy, self.lock_hash.as_bytes())?;
        if self.lock_script {
            Box::new(AddOutputCell {
                lock_script: lock_proxy_script.into(),
                type_script: self.second_script,
                capacity: 0,
                data: self.data,
                absolute_capacity: false,
                type_id: false,
            })
            .run(rpc, skeleton, log)
            .await
        } else {
            Box::new(AddOutputCell {
                lock_script: self.second_script.ok_or(eyre!("missing second script"))?,
                type_script: Some(lock_proxy_script.into()),
                capacity: 0,
                data: self.data,
                absolute_capacity: false,
                type_id: false,
            })
            .run(rpc, skeleton, log)
            .await
        }
    }
}

/// Search and add `lock-proxy` input cell (fake supported)
///
/// # Parameters
/// - `lock_hash`: the proxied lock hash
/// - `lock_script`: wether the script is used as lock script, otherwise type script
/// - `count`: max number of cells to add
pub struct AddLockProxyInputCell {
    pub lock_hash: H256,
    pub lock_script: bool,
    pub count: usize,
}

impl AddLockProxyInputCell {
    pub fn search_key(&self) -> Result<SearchKey> {
        let lock_proxy_script =
            hardcoded::build_script(hardcoded::Name::LockProxy, self.lock_hash.as_bytes())?;
        let mut query = if self.lock_script {
            CellQueryOptions::new_lock(lock_proxy_script)
        } else {
            CellQueryOptions::new_type(lock_proxy_script)
        };
        query.with_data = Some(true);
        query.script_search_mode = Some(SearchMode::Exact);
        Ok(query.into())
    }
}

#[async_trait]
impl<T: RPC> Operation<T> for AddLockProxyInputCell {
    async fn run(
        mut self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        Box::new(AddComponentCelldep {
            name: hardcoded::Name::LockProxy,
        })
        .run(rpc, skeleton, log)
        .await?;
        let search_key = self.search_key()?;
        while let Some(indexer_cell) = GetCellsIter::new(rpc, search_key.clone()).next().await? {
            let input = CellInputEx::new_from_indexer_cell(indexer_cell, None);
            skeleton.input(input)?.witness(Default::default());
            self.count -= 1;
            if self.count == 0 {
                break;
            }
        }
        Ok(())
    }
}
