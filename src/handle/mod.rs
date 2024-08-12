use ckb_cinnabar_calculator::{
    instruction::DefaultInstruction,
    operation::{
        AddInputCellByAddress, AddInputCellByOutPoint, AddOutputCellByAddress,
        AddOutputCellByInputIndex, AddSecp256k1SighashCellDep,
        AddSecp256k1SighashSignaturesWithCkbCli, BalanceTransaction,
    },
    re_exports::{ckb_sdk, eyre},
    skeleton::ChangeReceiver,
};
use ckb_sdk::Address;

use crate::object::*;

mod util;
use util::*;

/// Load the latest contract deployment record from the local migration directory
pub fn load_latest_contract_deployment(
    network: Network,
    contract_name: &str,
    migration_path: Option<&str>,
) -> eyre::Result<DeploymentRecord> {
    let path = generate_deployment_record_path(
        &network.to_string(),
        contract_name,
        migration_path.unwrap_or("../migration"),
    )?;
    load_deployment_record(&path)
}

/// Deploy a new contract version to the chain
pub async fn deploy_contract(
    network: String,
    contract_name: String,
    version: String,
    payer_address: Address,
    owner_address: Option<Address>,
    type_id: bool,
    migration_path: String,
) -> eyre::Result<()> {
    let rpc = create_rpc_from_network(&network)?;
    let (contract_binary, contract_hash) = load_contract_binary(&contract_name)?;
    let deploy_contract = DefaultInstruction::new(vec![
        Box::new(AddSecp256k1SighashCellDep {}),
        Box::new(AddInputCellByAddress {
            address: payer_address.clone(),
        }),
        Box::new(AddOutputCellByAddress {
            address: owner_address.clone().unwrap_or(payer_address.clone()),
            data: contract_binary,
            add_type_id: type_id,
        }),
        Box::new(BalanceTransaction {
            balancer: payer_address.payload().into(),
            change_receiver: ChangeReceiver::Address(
                owner_address.clone().unwrap_or(payer_address.clone()),
            ),
            additional_fee_rate: 2000,
        }),
        Box::new(AddSecp256k1SighashSignaturesWithCkbCli {
            signer_address: payer_address.clone(),
            tx_cache_path: "migration/txs".into(),
            keep_tx_file: true,
        }),
    ]);
    let tx_record_path =
        generate_deployment_record_path(&network, &contract_name, &migration_path)?;
    send_and_record_transaction(
        rpc,
        vec![deploy_contract],
        tx_record_path,
        "deploy",
        contract_name,
        version,
        Some(contract_hash),
        payer_address,
        owner_address,
    )
    .await
}

/// Migrate a contract to a new version
#[allow(clippy::too_many_arguments)]
pub async fn migrate_contract(
    network: String,
    contract_name: String,
    from_version: String,
    version: String,
    payer_address: Address,
    owner_address: Option<Address>,
    type_id_mode: String,
    migration_path: String,
) -> eyre::Result<()> {
    let tx_record_path =
        generate_deployment_record_path(&network, &contract_name, &migration_path)?;
    if !tx_record_path.exists() {
        return Err(eyre::eyre!("record file not exists"));
    }
    let record = load_deployment_record(&tx_record_path)?;
    if record.operation == "consume" {
        return Err(eyre::eyre!("version already consumed"));
    }
    if record.contract_owner_address() != payer_address.to_string() {
        return Err(eyre::eyre!("payer address not match the contract owner"));
    }
    if record.version != from_version {
        return Err(eyre::eyre!("from_version not match"));
    }
    let rpc = create_rpc_from_network(&network)?;
    let (contract_binary, contract_hash) = load_contract_binary(&contract_name)?;
    let contract_address = owner_address.clone().unwrap_or(payer_address.clone());
    let mut migrate_contract = DefaultInstruction::new(vec![
        Box::new(AddSecp256k1SighashCellDep {}),
        Box::new(AddInputCellByOutPoint {
            tx_hash: record.tx_hash.into(),
            index: record.out_index,
            since: None,
        }),
    ]);
    match type_id_mode.try_into()? {
        TypeIdMode::Keep => {
            migrate_contract.push(Box::new(AddOutputCellByInputIndex {
                input_index: 0,
                data: Some(contract_binary),
                lock_script: Some(contract_address.payload().into()),
                type_script: None,
                adjust_capacity: true,
            }));
        }
        TypeIdMode::Remove => {
            migrate_contract.push(Box::new(AddOutputCellByInputIndex {
                input_index: 0,
                data: Some(contract_binary),
                lock_script: Some(contract_address.payload().into()),
                type_script: Some(None),
                adjust_capacity: true,
            }));
        }
        TypeIdMode::New => {
            migrate_contract.push(Box::new(AddOutputCellByAddress {
                address: contract_address.clone(),
                data: contract_binary,
                add_type_id: true,
            }));
        }
    }
    migrate_contract.append(vec![
        Box::new(BalanceTransaction {
            balancer: payer_address.payload().into(),
            change_receiver: ChangeReceiver::Address(contract_address.clone()),
            additional_fee_rate: 2000,
        }),
        Box::new(AddSecp256k1SighashSignaturesWithCkbCli {
            signer_address: payer_address.clone(),
            tx_cache_path: "migration/txs".into(),
            keep_tx_file: true,
        }),
    ]);
    send_and_record_transaction(
        rpc,
        vec![migrate_contract],
        tx_record_path,
        "migrate",
        contract_name,
        version,
        Some(contract_hash),
        payer_address,
        owner_address,
    )
    .await
}

/// Consume a contract
pub async fn consume_contract(
    network: String,
    contract_name: String,
    version: String,
    payer_address: Address,
    receive_address: Option<Address>,
    migration_path: String,
) -> eyre::Result<()> {
    let tx_record_path =
        generate_deployment_record_path(&network, &contract_name, &migration_path)?;
    if !tx_record_path.exists() {
        return Err(eyre::eyre!("version not exists"));
    }
    let record = load_deployment_record(&tx_record_path)?;
    if record.operation == "consume" {
        return Err(eyre::eyre!("version already consumed"));
    }
    if record.contract_owner_address() != payer_address.to_string() {
        return Err(eyre::eyre!("payer address not match the contract owner"));
    }
    if record.version != version {
        return Err(eyre::eyre!("version not match"));
    }
    let rpc = create_rpc_from_network(&network)?;
    let consume_contract = DefaultInstruction::new(vec![
        Box::new(AddSecp256k1SighashCellDep {}),
        Box::new(AddInputCellByOutPoint {
            tx_hash: record.tx_hash.into(),
            index: record.out_index,
            since: None,
        }),
        Box::new(BalanceTransaction {
            balancer: payer_address.payload().into(),
            change_receiver: ChangeReceiver::Address(
                receive_address.clone().unwrap_or(payer_address.clone()),
            ),
            additional_fee_rate: 2000,
        }),
        Box::new(AddSecp256k1SighashSignaturesWithCkbCli {
            signer_address: payer_address.clone(),
            tx_cache_path: format!("{migration_path}/txs").into(),
            keep_tx_file: true,
        }),
    ]);
    send_and_record_transaction(
        rpc,
        vec![consume_contract],
        tx_record_path,
        "consume",
        contract_name,
        "".into(),
        None,
        payer_address,
        None,
    )
    .await
}
