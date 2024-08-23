use std::{fs, path::PathBuf};

use chrono::prelude::Utc;
use ckb_cinnabar_calculator::{
    instruction::{Instruction, TransactionCalculator},
    re_exports::{ckb_hash::blake2b_256, ckb_jsonrpc_types::OutputsValidator, ckb_sdk, eyre},
    rpc::{Network, RpcClient, RPC},
};
use ckb_sdk::Address;

use crate::object::*;

pub fn generate_contract_deployment_path(
    network: &Network,
    contract_name: &str,
    deployment_path: &str,
) -> eyre::Result<PathBuf> {
    Ok(PathBuf::new()
        .join(deployment_path)
        .join(network.to_string())
        .join(format!("{contract_name}.json")))
}

pub fn load_contract_deployment(
    network: &Network,
    contract_name: &str,
    deployment_path: &str,
    version: Option<&str>,
) -> eyre::Result<Option<DeploymentRecord>> {
    let path = generate_contract_deployment_path(network, contract_name, deployment_path)?;
    if path.exists() {
        let file = fs::File::open(&path)?;
        let deployments: Vec<DeploymentRecord> = serde_json::from_reader(file)?;
        if let Some(version) = version {
            Ok(deployments.into_iter().find(|r| r.version == version))
        } else {
            Ok(deployments.into_iter().last())
        }
    } else {
        Ok(None)
    }
}

pub fn load_contract_binary(
    contract_name: &str,
    binary_path: &str,
) -> eyre::Result<(Vec<u8>, [u8; 32])> {
    let contract_path = PathBuf::new().join(binary_path).join(contract_name);
    let contract_binary = fs::read(&contract_path)
        .map_err(|e| eyre::eyre!("{e}:{}", contract_path.to_string_lossy()))?;
    let contract_hash = blake2b_256(&contract_binary);
    Ok((contract_binary, contract_hash))
}

pub fn create_rpc_from_network(network: &Network) -> eyre::Result<RpcClient> {
    match network {
        Network::Mainnet => Ok(RpcClient::new_mainnet()),
        Network::Testnet => Ok(RpcClient::new_testnet()),
        Network::Fake => Err(eyre::eyre!("fake network")),
        Network::Custom(url) => Ok(RpcClient::new(url.as_str(), None)),
    }
}

pub async fn send_and_record_transaction<T: RPC>(
    rpc: T,
    instructions: Vec<Instruction<T>>,
    tx_path: PathBuf,
    operation: &str,
    contract_name: String,
    version: String,
    contract_hash: Option<[u8; 32]>,
    payer_address: Address,
    contract_owner_address: Option<Address>,
) -> eyre::Result<()> {
    let (skeleton, _) = TransactionCalculator::new(instructions)
        .new_skeleton(&rpc)
        .await?;
    let occupied_capacity = skeleton.outputs[0].occupied_capacity().as_u64();
    let type_id = skeleton.outputs[0].calc_type_hash();
    let tx_hash = rpc
        .send_transaction(
            skeleton.into_transaction_view().data().into(),
            Some(OutputsValidator::Passthrough),
        )
        .await?;
    println!("Transaction hash: {}", tx_hash);
    let deployment_record = DeploymentRecord {
        name: contract_name,
        date: Utc::now().to_rfc3339(),
        operation: operation.to_string(),
        version,
        tx_hash,
        out_index: 0,
        data_hash: contract_hash.map(Into::into),
        occupied_capacity,
        payer_address: payer_address.into(),
        contract_owner_address: contract_owner_address.into(),
        type_id: type_id.map(Into::into),
        comment: None,
    };
    save_contract_deployment(tx_path, deployment_record)
}

fn save_contract_deployment(path: PathBuf, record: DeploymentRecord) -> eyre::Result<()> {
    let mut records: Vec<DeploymentRecord> = if path.exists() {
        let content = fs::read(&path)?;
        serde_json::from_slice(&content)?
    } else {
        fs::create_dir_all(path.parent().unwrap())?;
        Vec::new()
    };
    records.push(record);
    let new_content = serde_json::to_string_pretty(&records)?;
    fs::write(path, new_content)?;
    Ok(())
}
