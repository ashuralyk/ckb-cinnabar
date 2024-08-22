use std::{path::PathBuf, sync::Arc, usize};

use ckb_sdk::{Address, HumanCapacity};
use ckb_types::H256;
use secp256k1::SecretKey;

use crate::{instruction::DefaultInstruction, operation::*};

/// Transfer CKB from one address to another
///
/// # Parameters
/// - `from`: The address to transfer CKB from
/// - `to`: The address to transfer CKB to
/// - `ckb`: The amount of CKB to transfer, e.g. "100.5 CKB"
/// - `sign`:
///     - 0: privkey => The private key to sign the transaction, if not provided, transaction won't balance and sign
///     - 1: additional_fee_rate => The additional fee rate to add
pub fn secp256k1_sighash_transfer(
    from: &Address,
    to: &Address,
    ckb: HumanCapacity,
) -> DefaultInstruction {
    DefaultInstruction::new(vec![
        Box::new(AddSecp256k1SighashCellDep {}),
        Box::new(AddInputCellByAddress {
            address: from.clone(),
        }),
        Box::new(AddOutputCell {
            lock_script: to.payload().into(),
            type_script: None,
            data: Vec::new(),
            capacity: ckb.into(),
            absolute_capacity: true,
            type_id: false,
        }),
    ])
}

/// Balance transaction with capacity and then sign it
///
/// # Parameters
/// - `signer`: The address who is supposed to provide capacity to balance, in the meantime, receive the change
/// - `privkey`: The private key to sign the transaction
/// - `additional_fee_rate`: The additional fee rate to add
pub fn balance_and_sign(
    signer: &Address,
    privkey: SecretKey,
    additional_fee_rate: u64,
) -> DefaultInstruction {
    DefaultInstruction::new(vec![
        Box::new(BalanceTransaction {
            balancer: signer.payload().into(),
            change_receiver: signer.clone().into(),
            additional_fee_rate,
        }),
        Box::new(AddSecp256k1SighashSignatures {
            user_lock_scripts: vec![signer.payload().into()],
            user_private_keys: vec![privkey],
        }),
    ])
}

/// Balance transaction with capacity and then sign it with native CKB-CLI
///
/// # Parameters
/// - `signer`: The address who is supposed to provide capacity to balance, in the meantime, receive the change
/// - `additional_fee_rate`: The additional fee rate to add
/// - `cache_path`: The path to store the transaction cache file
pub fn balance_and_sign_with_ckb_cli(
    signer: &Address,
    additional_fee_rate: u64,
    cache_path: Option<PathBuf>,
) -> DefaultInstruction {
    DefaultInstruction::new(vec![
        Box::new(BalanceTransaction {
            balancer: signer.payload().into(),
            change_receiver: signer.clone().into(),
            additional_fee_rate,
        }),
        Box::new(AddSecp256k1SighashSignaturesWithCkbCli {
            signer_address: signer.clone(),
            cache_path: cache_path.unwrap_or_else(|| PathBuf::from("/tmp")),
            keep_cache_file: false,
        }),
    ])
}

pub struct Spore {
    pub owner: Option<Address>, // if None, use minter as owner
    pub content_type: String,
    pub content: Vec<u8>,
    pub cluster_id: Option<H256>,
}

/// Mint multiple spore cells
///
/// # Parameters
/// - `minter`: The address to mint Spore
/// - `spores`: The Spores to mint
/// - `cluster_lock_proxy`: Whether to use cluster lock proxy
/// - `spore_id_collector`: The callback to collect spore id, e.g. collect for persistence
pub fn mint_spores(
    minter: &Address,
    spores: Vec<Spore>,
    cluster_lock_proxy: bool,
    spore_id_collector: Option<Arc<dyn Fn(H256) + Send + Sync>>,
) -> DefaultInstruction {
    let mut mint = DefaultInstruction::new(vec![
        Box::new(AddSecp256k1SighashCellDep {}),
        Box::new(AddInputCellByAddress {
            address: minter.clone(),
        }),
    ]);
    let authority_mode = if cluster_lock_proxy {
        ClusterAuthorityMode::LockProxy
    } else {
        ClusterAuthorityMode::ClusterCell
    };
    for Spore {
        owner,
        content_type,
        content,
        cluster_id,
    } in spores
    {
        mint.push(Box::new(AddSporeOutputCell {
            lock_script: owner.unwrap_or_else(|| minter.clone()).into(),
            content_type,
            content,
            cluster_id,
            authority_mode: authority_mode.clone(),
            spore_id_collector: spore_id_collector.clone(),
        }));
    }
    mint.push(Box::new(AddSporeActions {}));
    mint
}

/// Transfer multiple spore cells
///
/// # Parameters
/// - `from`: The address to transfer Spore from
/// - `spores`: The Spores to transfer
///     - `0`: The address to transfer Spore to
///     - `1`: The Spore ID to transfer
pub fn transfer_spores(from: &Address, spores: Vec<(Address, H256)>) -> DefaultInstruction {
    let mut transfer = DefaultInstruction::new(vec![
        Box::new(AddSecp256k1SighashCellDep {}),
        Box::new(AddInputCellByAddress {
            address: from.clone(),
        }),
    ]);
    for (to, spore_id) in spores {
        transfer
            .push(Box::new(AddSporeInputCellBySporeId {
                spore_id,
                check_owner: Some(from.clone().into()),
            }))
            .push(Box::new(AddSporeOutputCellByInputIndex {
                input_index: usize::MAX,
                lock_script: Some(to.into()),
                authority_mode: ClusterAuthorityMode::Skip,
            }));
    }
    transfer.push(Box::new(AddSporeActions {}));
    transfer
}

/// Burn multiple spore cells
///
/// # Parameters
/// - `owner`: The address to burn Spore from
/// - `spores`: The Spores to burn
pub fn burn_spores(owner: &Address, spores: Vec<H256>) -> DefaultInstruction {
    let mut burn = DefaultInstruction::new(vec![
        Box::new(AddSecp256k1SighashCellDep {}),
        Box::new(AddInputCellByAddress {
            address: owner.clone(),
        }),
    ]);
    spores.into_iter().for_each(|spore_id| {
        burn.push(Box::new(AddSporeInputCellBySporeId {
            spore_id,
            check_owner: Some(owner.clone().into()),
        }));
    });
    burn.push(Box::new(AddSporeActions {}));
    burn
}

pub struct Cluster {
    pub owner: Option<Address>, // if None, use minter as owner
    pub cluster_name: String,
    pub cluster_description: Vec<u8>,
}

/// Mint multiple cluster cells
///
/// # Parameters
/// - `minter`: The address to mint Cluster
/// - `clusters`: The Clusters to mint
/// - `cluster_id_collector`: The callback to collect cluster id, e.g. collect for persistence
pub fn mint_clusters(
    minter: &Address,
    clusters: Vec<Cluster>,
    cluster_id_collector: Option<Arc<dyn Fn(H256) + Send + Sync>>,
) -> DefaultInstruction {
    let mut mint = DefaultInstruction::new(vec![
        Box::new(AddSecp256k1SighashCellDep {}),
        Box::new(AddInputCellByAddress {
            address: minter.clone(),
        }),
    ]);
    for Cluster {
        owner,
        cluster_name,
        cluster_description,
    } in clusters
    {
        mint.push(Box::new(AddClusterOutputCell {
            lock_script: owner.unwrap_or_else(|| minter.clone()).into(),
            name: cluster_name,
            description: cluster_description,
            cluster_id_collector: cluster_id_collector.clone(),
        }));
    }
    mint.push(Box::new(AddSporeActions {}));
    mint
}

/// Transfer multiple cluster cells
///
/// # Parameters
/// - `from`: The address to transfer Cluster from
/// - `clusters`: The Clusters to transfer
pub fn transfer_clusters(from: &Address, clusters: Vec<(Address, H256)>) -> DefaultInstruction {
    let mut transfer = DefaultInstruction::new(vec![
        Box::new(AddSecp256k1SighashCellDep {}),
        Box::new(AddInputCellByAddress {
            address: from.clone(),
        }),
    ]);
    for (to, cluster_id) in clusters {
        transfer
            .push(Box::new(AddClusterInputCellByClusterId { cluster_id }))
            .push(Box::new(AddOutputCellByInputIndex {
                input_index: usize::MAX,
                lock_script: Some(to.into()),
                type_script: None,
                data: None,
                adjust_capacity: true,
            }));
    }
    transfer.push(Box::new(AddSporeActions {}));
    transfer
}

// TODO: Add more predefined instructions here, e.g. xUDT and DAO
