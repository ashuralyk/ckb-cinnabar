use std::{fs, path::PathBuf};

use chrono::prelude::Utc;
use ckb_cinnabar_calculator::{
    instruction::{Instruction, TransactionCalculator},
    re_exports::{
        ckb_hash::blake2b_256, ckb_jsonrpc_types::OutputsValidator, ckb_sdk, ckb_types::H256, eyre,
    },
    rpc::{RpcClient, RPC},
};
use ckb_sdk::Address;

use crate::object::*;

pub fn generate_deployment_record_path(
    network: &str,
    contract_name: &str,
    migration_path: &str,
) -> eyre::Result<PathBuf> {
    let path = PathBuf::new().join(migration_path).join(network);
    if !path.exists() {
        fs::create_dir_all(&path)?;
    }
    Ok(path.join(format!("{contract_name}.json")))
}

pub fn load_deployment_record(path: &PathBuf) -> eyre::Result<DeploymentRecord> {
    let file = fs::File::open(path)?;
    let records: Vec<DeploymentRecord> = serde_json::from_reader(file)?;
    records.last().cloned().ok_or(eyre::eyre!("empty record"))
}

pub fn save_deployment_record(path: PathBuf, record: DeploymentRecord) -> eyre::Result<()> {
    let mut records: Vec<DeploymentRecord> = if path.exists() {
        let content = fs::read(&path)?;
        serde_json::from_slice(&content)?
    } else {
        Vec::new()
    };
    records.push(record);
    let new_content = serde_json::to_string_pretty(&records)?;
    fs::write(path, new_content)?;
    Ok(())
}

pub fn load_contract_binary(contract_name: &str) -> eyre::Result<(Vec<u8>, [u8; 32])> {
    let contract_path = PathBuf::new().join("build/release").join(contract_name);
    let contract_binary = fs::read(&contract_path)
        .map_err(|e| eyre::eyre!("{e}:{}", contract_path.to_string_lossy()))?;
    let contract_hash = blake2b_256(&contract_binary);
    Ok((contract_binary, contract_hash))
}

pub fn create_rpc_from_network(network: &str) -> eyre::Result<RpcClient> {
    match network.parse()? {
        Network::Mainnet => Ok(RpcClient::new_mainnet()),
        Network::Testnet => Ok(RpcClient::new_testnet()),
        Network::Custom(url) => Ok(RpcClient::new(url.as_str(), None)),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn send_and_record_transaction<T: RPC>(
    rpc: T,
    instructions: Vec<Instruction<T>>,
    tx_record_path: PathBuf,
    operation: &str,
    contract_name: String,
    version: String,
    contract_hash: Option<[u8; 32]>,
    payer_address: Address,
    owner_address: Option<Address>,
) -> eyre::Result<()> {
    let skeleton = TransactionCalculator::new(rpc.clone(), instructions)
        .run()
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
        tx_hash: tx_hash.into(),
        out_index: 0,
        data_hash: contract_hash.map(|v| H256::from(v).into()),
        occupied_capacity,
        payer_address: payer_address.into(),
        owner_address: owner_address.map(Into::into),
        type_id: type_id.map(Into::into),
        comment: None,
    };
    save_deployment_record(tx_record_path, deployment_record)
}
