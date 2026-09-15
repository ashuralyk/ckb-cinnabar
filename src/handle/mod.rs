//! Deploy / migrate / consume / list contract cells on CKB.
//!
//! Each command builds a `DefaultInstruction` of basic operations, then
//! [`send_and_record_transaction`] assembles, optionally sends, and writes a
//! [`DeploymentRecord`] under `deployment/<network>/<name>.json`.
//!
//! Signing: `--privkey-env` uses in-process secp256k1 signatures; otherwise
//! live send shells out to interactive `ckb-cli`. `--dry-run` skips both send
//! and `ckb-cli`.

#![allow(clippy::too_many_arguments)]

use ckb_cinnabar_calculator::{
    address::Address,
    instruction::DefaultInstruction,
    operation::basic::{
        AddInputCellByAddress, AddInputCellByOutPoint, AddOutputCellByAddress,
        AddOutputCellByInputIndex, AddSecp256k1SighashCellDep, AddSecp256k1SighashSignatures,
        AddSecp256k1SighashSignaturesWithCkbCli, BalanceTransaction, CapacityAdjustment,
    },
    re_exports::{eyre, secp256k1::SecretKey},
    skeleton::ChangeReceiver,
};

use crate::object::*;

mod helper;
pub use helper::*;

/// Append fee-balancing plus a signature operation.
///
/// With a privkey, signs in-process. Without one, live send uses interactive
/// `ckb-cli`; `--dry-run` skips signing entirely.
fn append_balance_and_sign(
    instruction: &mut DefaultInstruction,
    signer: &Address,
    deployment_path: &str,
    privkey: Option<SecretKey>,
    dry_run: bool,
) {
    instruction.push(Box::new(BalanceTransaction {
        balancer: signer.clone().into(),
        change_receiver: ChangeReceiver::Address(signer.clone()),
        additional_fee_rate: 2000,
    }));
    if let Some(key) = privkey {
        instruction.push(Box::new(AddSecp256k1SighashSignatures {
            user_lock_scripts: vec![signer.payload().into()],
            user_private_keys: vec![key],
        }));
    } else if !dry_run {
        instruction.push(Box::new(AddSecp256k1SighashSignaturesWithCkbCli {
            signer_address: signer.clone(),
            cache_path: format!("{deployment_path}/txs").into(),
            keep_cache_file: true,
        }));
    }
}

/// Create a new contract version on-chain
pub async fn deploy_contract(
    opts: ExecOpts,
    contract_name: String,
    version: String,
    payer_address: Address,
    contract_owner_address: Option<Address>,
    type_id: bool,
) -> eyre::Result<()> {
    let deployment = load_contract_deployment(
        &opts.network,
        &contract_name,
        &opts.deployment_path,
        Some(&version),
    )?;
    if deployment.is_some() {
        return Err(eyre::eyre!("version already exists"));
    }
    let rpc = create_rpc_from_network(&opts.network)?;
    let (contract_binary, contract_hash) =
        load_contract_binary(&contract_name, &opts.contract_path)?;
    let contract_owner_address = contract_owner_address.unwrap_or(payer_address.clone());
    let mut deploy_contract = DefaultInstruction::new(vec![
        Box::new(AddSecp256k1SighashCellDep {}),
        Box::new(AddInputCellByAddress {
            address: payer_address.clone(),
        }),
        Box::new(AddOutputCellByAddress {
            address: contract_owner_address.clone(),
            data: contract_binary,
            add_type_id: type_id,
        }),
    ]);
    append_balance_and_sign(
        &mut deploy_contract,
        &payer_address,
        &opts.deployment_path,
        opts.privkey,
        opts.dry_run,
    );
    let tx_path =
        generate_contract_deployment_path(&opts.network, &contract_name, &opts.deployment_path);
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
        opts.dry_run,
        opts.json,
    )
    .await
}

/// Migrate a contract to a new version
pub async fn migrate_contract(
    opts: ExecOpts,
    contract_name: String,
    from_version: String,
    version: String,
    contract_owner_address: Option<Address>,
    type_id_mode: TypeIdMode,
) -> eyre::Result<()> {
    let deployment = load_contract_deployment(
        &opts.network,
        &contract_name,
        &opts.deployment_path,
        Some(&from_version),
    )?
    .ok_or(eyre::eyre!("version not exists"))?;
    if deployment.operation == "consume" {
        return Err(eyre::eyre!("version already consumed"));
    }
    let rpc = create_rpc_from_network(&opts.network)?;
    let (contract_binary, contract_hash) =
        load_contract_binary(&contract_name, &opts.contract_path)?;
    let payer_address: Address = deployment.contract_owner_address.clone().try_into()?;
    let contract_owner_address: Address = contract_owner_address.unwrap_or(payer_address.clone());
    let mut migrate_contract = DefaultInstruction::new(vec![
        Box::new(AddSecp256k1SighashCellDep {}),
        Box::new(AddInputCellByOutPoint {
            tx_hash: deployment.tx_hash,
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
                adjust_capacity: CapacityAdjustment::BuildExact,
            }));
        }
        TypeIdMode::Remove => {
            migrate_contract.push(Box::new(AddOutputCellByInputIndex {
                input_index: 0,
                data: Some(contract_binary),
                lock_script: Some(contract_owner_address.clone().into()),
                type_script: Some(None),
                adjust_capacity: CapacityAdjustment::BuildExact,
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
    append_balance_and_sign(
        &mut migrate_contract,
        &payer_address,
        &opts.deployment_path,
        opts.privkey,
        opts.dry_run,
    );
    let tx_path =
        generate_contract_deployment_path(&opts.network, &contract_name, &opts.deployment_path);
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
        opts.dry_run,
        opts.json,
    )
    .await
}

/// Consume a deployed contract cell and return its capacity to `receiver_address`
/// (defaults to the previous contract owner).
pub async fn consume_contract(
    opts: ExecOpts,
    contract_name: String,
    version: String,
    receiver_address: Option<Address>,
) -> eyre::Result<()> {
    let deployment = load_contract_deployment(
        &opts.network,
        &contract_name,
        &opts.deployment_path,
        Some(&version),
    )?
    .ok_or(eyre::eyre!("version not exists"))?;
    if deployment.operation == "consume" {
        return Err(eyre::eyre!("version already consumed"));
    }
    let payer_address: Address = deployment.contract_owner_address.clone().try_into()?;
    let receiver_address: Address = receiver_address.unwrap_or(payer_address.clone());
    let rpc = create_rpc_from_network(&opts.network)?;
    let mut consume_contract = DefaultInstruction::new(vec![
        Box::new(AddSecp256k1SighashCellDep {}),
        Box::new(AddInputCellByOutPoint {
            tx_hash: deployment.tx_hash,
            index: deployment.out_index,
            since: None,
        }),
    ]);
    consume_contract.push(Box::new(BalanceTransaction {
        balancer: payer_address.payload().into(),
        change_receiver: ChangeReceiver::Address(receiver_address),
        additional_fee_rate: 2000,
    }));
    if let Some(key) = opts.privkey {
        consume_contract.push(Box::new(AddSecp256k1SighashSignatures {
            user_lock_scripts: vec![payer_address.payload().into()],
            user_private_keys: vec![key],
        }));
    } else if !opts.dry_run {
        consume_contract.push(Box::new(AddSecp256k1SighashSignaturesWithCkbCli {
            signer_address: payer_address.clone(),
            cache_path: format!("{}/txs", opts.deployment_path).into(),
            keep_cache_file: true,
        }));
    }
    let tx_path =
        generate_contract_deployment_path(&opts.network, &contract_name, &opts.deployment_path);
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
        opts.dry_run,
        opts.json,
    )
    .await
}

/// List deployment records under `--deployment-path` for this network.
///
/// `contract_name` filters to one JSON file; `mode` keeps live, consumed, or all.
pub async fn list_contracts(
    opts: ExecOpts,
    contract_name: Option<String>,
    mode: ListMode,
) -> eyre::Result<()> {
    let records = load_all_deployments(
        &opts.network,
        &opts.deployment_path,
        contract_name.as_deref(),
        mode,
    )?;
    let mut response = CliResponse::ok("list", opts.dry_run);
    response.records = Some(records);
    print_response(opts.json, &response);
    Ok(())
}
