//! Operations for the `ckb-proxy-locks` component scripts (always-success,
//! type-burn, lock-proxy, …).
//!
//! Deployment out-points live in [`hardcoded`]. See
//! <https://github.com/ckb-ecofund/ckb-proxy-locks>.

use async_trait::async_trait;
use ckb_types::{
    core::{DepType, ScriptHashType},
    h256,
    packed::Script,
    prelude::{Builder, Entity, Pack},
    H256,
};
use eyre::{eyre, Result};

use crate::{
    indexer::{CellQueryOptions, SearchKey, SearchMode},
    operation::{
        basic::{AddCellDep, AddOutputCell},
        Log, Operation,
    },
    rpc::{GetCellsIter, Network, RPC},
    skeleton::{CellInputEx, ScriptEx, TransactionSkeleton},
};

/// Component-use simple scripts
///
/// note: migrations please refer to https://github.com/ckb-ecofund/ckb-proxy-locks/tree/main/migrations
pub mod hardcoded {
    use crate::simulation::random_hash;

    use super::*;

    pub const COMPONENT_MAINNET_TX_HASH: H256 =
        h256!("0x10d63a996157d32c01078058000052674ca58d15f921bec7f1dcdac2160eb66b");
    pub const COMPONENT_TESTNET_TX_HASH: H256 =
        h256!("0xb4f171c9c9caf7401f54a8e56225ae21d95032150a87a4678eac3f66a3137b93");

    lazy_static::lazy_static! {
        pub static ref COMPONENT_FAKENET_TX_HASH: H256 = random_hash().into();
    }

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

    /// Output index of each component script inside the proxy-locks dep transaction.
    #[repr(u32)]
    pub enum Name {
        /// Always-success lock (index 0).
        AlwaysSuccess = 0,
        /// Input-type proxy (index 1).
        InputTypeProxy,
        /// Output-type proxy (index 2).
        OutputTypeProxy,
        /// Lock proxy (index 3).
        LockProxy,
        /// Single-use lock (index 4).
        SingleUse,
        /// Type-burn lock (index 5).
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

    /// Component script for `network`. Fake/custom networks use a
    /// `ScriptEx::Reference` resolved from a named cell dep.
    pub fn component_script(network: Network, name: Name, args: &[u8]) -> ScriptEx {
        match network {
            Network::Mainnet | Network::Testnet => Script::new_builder()
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
                .hash_type(ScriptHashType::Data1)
                .args(args.pack())
                .build()
                .into(),
            _ => (name.to_string(), args.to_owned()).into(),
        }
    }

    /// Deployment tx hash of the proxy-locks bundle for `network`.
    pub fn component_tx_hash(network: Network) -> H256 {
        match network {
            Network::Mainnet => COMPONENT_MAINNET_TX_HASH,
            Network::Testnet => COMPONENT_TESTNET_TX_HASH,
            _ => COMPONENT_FAKENET_TX_HASH.clone(),
        }
    }
}

/// Add the `ckb-proxy-locks` cell dep for `name` (index = `name as u32`).
pub struct AddComponentCelldep {
    /// Which component script inside the proxy-locks transaction.
    pub name: hardcoded::Name,
}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddComponentCelldep {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        Box::new(AddCellDep {
            name: self.name.to_string(),
            tx_hash: hardcoded::component_tx_hash(rpc.network()),
            index: self.name as u32,
            dep_type: DepType::Code,
            with_data: false,
        })
        .run(rpc, skeleton, log)
        .await
    }
}

/// Add a type-burn-lock output whose args are the type hash of another output.
pub struct AddTypeBurnOutputCell {
    /// Output whose type hash becomes the type-burn lock args.
    pub output_index: usize,
    /// Optional type script of the new cell.
    pub type_script: Option<ScriptEx>,
    /// Cell data.
    pub data: Vec<u8>,
}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddTypeBurnOutputCell {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        Box::new(AddComponentCelldep {
            name: hardcoded::Name::TypeBurn,
        })
        .run(rpc, skeleton, log)
        .await?;
        let reference_output = skeleton.get_output_by_index(self.output_index)?;
        let reference_type_hash = reference_output
            .calc_type_hash()
            .ok_or(eyre!("reference output has no type script"))?;
        let type_burn_lock_script = hardcoded::component_script(
            rpc.network(),
            hardcoded::Name::TypeBurn,
            reference_type_hash.as_bytes(),
        );
        Box::new(AddOutputCell {
            lock_script: type_burn_lock_script,
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

/// Search and add type-burn-lock input cells whose args equal `type_hash`.
pub struct AddTypeBurnInputCell {
    /// Type-script hash encoded as the type-burn lock args.
    pub type_hash: H256,
    /// Maximum number of matching cells to consume.
    pub count: usize,
}

impl AddTypeBurnInputCell {
    /// Indexer search key: type-burn lock args = `type_hash`.
    pub fn search_key(
        &self,
        network: Network,
        skeleton: &TransactionSkeleton,
    ) -> Result<SearchKey> {
        let type_burn_lock_script = hardcoded::component_script(
            network,
            hardcoded::Name::TypeBurn,
            self.type_hash.as_bytes(),
        );
        let mut query = CellQueryOptions::new_lock(type_burn_lock_script.to_script(skeleton)?);
        query.with_data = Some(true);
        Ok(query.into())
    }
}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddTypeBurnInputCell {
    async fn run(
        mut self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        Box::new(AddComponentCelldep {
            name: hardcoded::Name::TypeBurn,
        })
        .run(rpc, skeleton, log)
        .await?;
        let search_key = self.search_key(rpc.network(), skeleton)?;
        let mut iter = GetCellsIter::new(rpc, search_key.clone());
        while let Some(indexer_cell) = iter.next().await? {
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

/// Consume a type-burn-lock cell whose args are the type hash of `skeleton.inputs[input_index]`.
pub struct AddTypeBurnInputCellByInputIndex {
    /// Index of the input whose type hash is searched; `usize::MAX` = last input.
    pub input_index: usize,
}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddTypeBurnInputCellByInputIndex {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        Box::new(AddComponentCelldep {
            name: hardcoded::Name::TypeBurn,
        })
        .run(rpc, skeleton, log)
        .await?;
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

/// Add a lock-proxy cell whose args are `lock_hash`.
pub struct AddLockProxyOutputCell {
    /// Hash of the lock being proxied.
    pub lock_hash: H256,
    /// `true`: proxy is the lock script; `false`: proxy is the type script
    /// (then `second_script` is required as the actual lock).
    pub lock_script: bool,
    /// The other script on the cell (type when `lock_script`, lock otherwise).
    pub second_script: Option<ScriptEx>,
    /// Cell data.
    pub data: Vec<u8>,
}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddLockProxyOutputCell {
    async fn run(
        mut self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let lock_proxy_script = hardcoded::component_script(
            rpc.network(),
            hardcoded::Name::LockProxy,
            self.lock_hash.as_bytes(),
        );
        if self.lock_script {
            Box::new(AddOutputCell {
                lock_script: lock_proxy_script,
                type_script: self.second_script,
                capacity: 0,
                data: self.data,
                absolute_capacity: false,
                type_id: false,
            })
            .run(rpc, skeleton, log)
            .await
        } else {
            Box::new(AddComponentCelldep {
                name: hardcoded::Name::LockProxy,
            })
            .run(rpc, skeleton, log)
            .await?;
            Box::new(AddOutputCell {
                lock_script: self.second_script.ok_or(eyre!("missing second script"))?,
                type_script: Some(lock_proxy_script),
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

/// Search and consume lock-proxy cells whose args equal `lock_hash`.
pub struct AddLockProxyInputCell {
    /// Hash of the lock being proxied.
    pub lock_hash: H256,
    /// `true` search as lock script; `false` search as type script.
    pub lock_script: bool,
    /// Maximum number of matching cells to consume.
    pub count: usize,
}

impl AddLockProxyInputCell {
    /// Indexer search key: lock-proxy args = `lock_hash`, as lock or type.
    pub fn search_key(
        &self,
        network: Network,
        skeleton: &TransactionSkeleton,
    ) -> Result<SearchKey> {
        let lock_proxy_script = hardcoded::component_script(
            network,
            hardcoded::Name::LockProxy,
            self.lock_hash.as_bytes(),
        );
        let mut query = if self.lock_script {
            CellQueryOptions::new_lock(lock_proxy_script.to_script(skeleton)?)
        } else {
            CellQueryOptions::new_type(lock_proxy_script.to_script(skeleton)?)
        };
        query.with_data = Some(true);
        query.script_search_mode = Some(SearchMode::Exact);
        Ok(query.into())
    }
}

#[async_trait(?Send)]
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
        let search_key = self.search_key(rpc.network(), skeleton)?;
        let mut iter = GetCellsIter::new(rpc, search_key.clone());
        while let Some(indexer_cell) = iter.next().await? {
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
