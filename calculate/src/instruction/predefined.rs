use std::{path::PathBuf, usize};

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
/// - `cache_path`: The path to store the transaction cache file, default is `/tmp`
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
            keep_cache_file: true,
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
pub fn mint_spores(
    minter: &Address,
    spores: Vec<Spore>,
    cluster_lock_proxy: bool,
) -> DefaultInstruction {
    let mut mint = DefaultInstruction::new(vec![
        Box::new(AddSecp256k1SighashCellDep {}),
        // Used to calculate the spore unique id
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
    let mut transfer = DefaultInstruction::new(vec![Box::new(AddSecp256k1SighashCellDep {})]);
    for (to, spore_id) in spores {
        transfer
            .push(Box::new(AddSporeInputCellBySporeId {
                spore_id,
                check_owner: Some(from.clone().into()),
            }))
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

/// Burn multiple spore cells
///
/// # Parameters
/// - `owner`: The address to burn Spore from
/// - `spores`: The Spores to burn
pub fn burn_spores(owner: &Address, spores: Vec<H256>) -> DefaultInstruction {
    let mut burn = DefaultInstruction::new(vec![Box::new(AddSecp256k1SighashCellDep {})]);
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
pub fn mint_clusters(minter: &Address, clusters: Vec<Cluster>) -> DefaultInstruction {
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

/// Deposit capacity to Nervos DAO
///
/// # Parameters
/// - `depositer`: The address to deposit capacity
/// - `ckb`: The amount of CKB to deposit, e.g. "100.5 CKB"
pub fn dao_deposit(depositer: &Address, ckb: HumanCapacity) -> DefaultInstruction {
    DefaultInstruction::new(vec![
        Box::new(AddSecp256k1SighashCellDep {}),
        Box::new(AddDaoDepositOutputCell {
            owner: depositer.clone().into(),
            deposit_capacity: ckb.into(),
        }),
    ])
}

/// Withdraw capacity from Nervos DAO, which only makes a mark as phase one
///
/// # Parameters
/// - `depositer`: The address to withdraw capacity
/// - `upperbound_capacity`: The maximum capacity to withdraw from Nervos DAO
/// - `upperbound_timestamp`: The upperbound timestamp that only choose cells before it
/// - `transfer_to`: if provided, the capacity will be transferred to this address
pub fn dao_withdraw_phase_one(
    depositer: &Address,
    upperbound_capacity: Option<HumanCapacity>,
    upperbound_timestamp: Option<u64>,
    transfer_to: Option<&Address>,
) -> DefaultInstruction {
    DefaultInstruction::new(vec![
        Box::new(AddSecp256k1SighashCellDep {}),
        Box::new(AddDaoWithdrawPhaseOneCells {
            maximal_withdraw_capacity: upperbound_capacity.map(Into::into).unwrap_or(u64::MAX),
            upperbound_timesamp: upperbound_timestamp.unwrap_or(u64::MAX),
            owner: depositer.clone().into(),
            transfer_to: transfer_to.map(|v| v.clone().into()),
            throw_if_no_avaliable: true,
        }),
    ])
}

/// Withdraw capacity from Nervos DAO, which actually withdraws the capacity
///
/// # Parameters
/// - `withdrawer`: The address to withdraw capacity
/// - `upperbound_capacity`: The maximum capacity to withdraw from Nervos DAO
/// - `transfer_to`: if provided, the capacity will be transferred to this address
pub fn dao_withdraw_phase_two(
    withdrawer: &Address,
    upperbound_capacity: Option<HumanCapacity>,
    transfer_to: Option<&Address>,
) -> DefaultInstruction {
    DefaultInstruction::new(vec![
        Box::new(AddSecp256k1SighashCellDep {}),
        Box::new(AddDaoWithdrawPhaseTwoCells {
            maximal_withdraw_capacity: upperbound_capacity.map(Into::into).unwrap_or(u64::MAX),
            owner: withdrawer.clone().into(),
            transfer_to: transfer_to.map(|v| v.clone().into()),
            throw_if_no_avaliable: true,
        }),
    ])
}

// TODO: Add more predefined instructions here, e.g. xUDT
