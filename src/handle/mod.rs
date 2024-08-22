#![allow(clippy::too_many_arguments)]

use ckb_cinnabar_calculator::{
    instruction::DefaultInstruction,
    operation::{
        AddInputCellByAddress, AddInputCellByOutPoint, AddOutputCellByAddress,
        AddOutputCellByInputIndex, AddSecp256k1SighashCellDep,
        AddSecp256k1SighashSignaturesWithCkbCli, BalanceTransaction,
    },
    re_exports::{ckb_sdk, eyre},
    rpc::Network,
    skeleton::ChangeReceiver,
};
use ckb_sdk::Address;

use crate::object::*;

mod helper;
pub use helper::*;

/// Create a new contract version on-chain
pub async fn deploy_contract(
    network: Network,
    contract_name: String,
    version: String,
    payer_address: Address,
    contract_owner_address: Option<Address>,
    type_id: bool,
    deployment_path: String,
    binary_path: String,
) -> eyre::Result<()> {
    let deployment =
        load_contract_deployment(&network, &contract_name, &deployment_path, Some(&version))?;
    if deployment.is_some() {
        return Err(eyre::eyre!("version already exists"));
    }
    let rpc = create_rpc_from_network(&network)?;
    let (contract_binary, contract_hash) = load_contract_binary(&contract_name, &binary_path)?;
    let contract_owner_address = contract_owner_address.unwrap_or(payer_address.clone());
    let deploy_contract = DefaultInstruction::new(vec![
        Box::new(AddSecp256k1SighashCellDep {}),
        Box::new(AddInputCellByAddress {
            address: payer_address.clone(),
        }),
        Box::new(AddOutputCellByAddress {
            address: contract_owner_address.clone(),
            data: contract_binary,
            add_type_id: type_id,
        }),
        Box::new(BalanceTransaction {
            balancer: payer_address.clone().into(),
            change_receiver: ChangeReceiver::Address(payer_address.clone()),
            additional_fee_rate: 2000,
        }),
        Box::new(AddSecp256k1SighashSignaturesWithCkbCli {
            signer_address: payer_address.clone(),
            cache_path: format!("{deployment_path}/txs").into(),
            keep_cache_file: true,
        }),
    ]);
    let tx_path = generate_contract_deployment_path(&network, &contract_name, &deployment_path)?;
    send_and_record_transaction(
        rpc,
        vec![deploy_contract],
        tx_path,
        "deploy",
        contract_name,
        version,
        Some(contract_hash),
        payer_address,
        Some(contract_owner_address),
    )
    .await
}

/// Migrate a contract to a new version
pub async fn migrate_contract(
    network: Network,
    contract_name: String,
    from_version: String,
    version: String,
    contract_owner_address: Option<Address>,
    type_id_mode: TypeIdMode,
    deployment_path: String,
    binary_path: String,
) -> eyre::Result<()> {
    let deployment = load_contract_deployment(
        &network,
        &contract_name,
        &deployment_path,
        Some(&from_version),
    )?
    .ok_or(eyre::eyre!("version not exists"))?;
    if deployment.operation == "consume" {
        return Err(eyre::eyre!("version already consumed"));
    }
    let rpc = create_rpc_from_network(&network)?;
    let (contract_binary, contract_hash) = load_contract_binary(&contract_name, &binary_path)?;
    let payer_address: Address = deployment.contract_owner_address.clone().try_into()?;
    let contract_owner_address: Address = contract_owner_address.unwrap_or(payer_address.clone());
    let mut migrate_contract = DefaultInstruction::new(vec![
        Box::new(AddSecp256k1SighashCellDep {}),
        Box::new(AddInputCellByOutPoint {
            tx_hash: deployment.tx_hash.into(),
            index: deployment.out_index,
            since: None,
        }),
    ]);
    match type_id_mode {
        TypeIdMode::Keep => {
            migrate_contract.push(Box::new(AddOutputCellByInputIndex {
                input_index: 0,
                data: Some(contract_binary),
                lock_script: Some(contract_owner_address.clone().into()),
                type_script: None,
                adjust_capacity: true,
            }));
        }
        TypeIdMode::Remove => {
            migrate_contract.push(Box::new(AddOutputCellByInputIndex {
                input_index: 0,
                data: Some(contract_binary),
                lock_script: Some(contract_owner_address.clone().into()),
                type_script: Some(None),
                adjust_capacity: true,
            }));
        }
        TypeIdMode::New => {
            migrate_contract.push(Box::new(AddOutputCellByAddress {
                address: contract_owner_address.clone(),
                data: contract_binary,
                add_type_id: true,
            }));
        }
    }
    migrate_contract.append(vec![
        Box::new(BalanceTransaction {
            balancer: payer_address.clone().into(),
            change_receiver: ChangeReceiver::Address(payer_address.clone()),
            additional_fee_rate: 2000,
        }),
        Box::new(AddSecp256k1SighashSignaturesWithCkbCli {
            signer_address: payer_address.clone(),
            cache_path: format!("{deployment_path}/txs").into(),
            keep_cache_file: true,
        }),
    ]);
    let tx_path = generate_contract_deployment_path(&network, &contract_name, &deployment_path)?;
    send_and_record_transaction(
        rpc,
        vec![migrate_contract],
        tx_path,
        "migrate",
        contract_name,
        version,
        Some(contract_hash),
        payer_address,
        Some(contract_owner_address),
    )
    .await
}

/// Consume a contract
pub async fn consume_contract(
    network: Network,
    contract_name: String,
    version: String,
    receiver_address: Option<Address>,
    deployment_path: String,
) -> eyre::Result<()> {
    let deployment =
        load_contract_deployment(&network, &contract_name, &deployment_path, Some(&version))?
            .ok_or(eyre::eyre!("version not exists"))?;
    if deployment.operation == "consume" {
        return Err(eyre::eyre!("version already consumed"));
    }
    let payer_address: Address = deployment.contract_owner_address.clone().try_into()?;
    let receiver_address: Address = receiver_address.unwrap_or(payer_address.clone());
    let rpc = create_rpc_from_network(&network)?;
    let consume_contract = DefaultInstruction::new(vec![
        Box::new(AddSecp256k1SighashCellDep {}),
        Box::new(AddInputCellByOutPoint {
            tx_hash: deployment.tx_hash.into(),
            index: deployment.out_index,
            since: None,
        }),
        Box::new(BalanceTransaction {
            balancer: payer_address.payload().into(),
            change_receiver: ChangeReceiver::Address(receiver_address),
            additional_fee_rate: 2000,
        }),
        Box::new(AddSecp256k1SighashSignaturesWithCkbCli {
            signer_address: payer_address.clone(),
            cache_path: format!("{deployment_path}/txs").into(),
            keep_cache_file: true,
        }),
    ]);
    let tx_path = generate_contract_deployment_path(&network, &contract_name, &deployment_path)?;
    send_and_record_transaction(
        rpc,
        vec![consume_contract],
        tx_path,
        "consume",
        contract_name,
        "".into(),
        None,
        payer_address,
        Default::default(),
    )
    .await
}
