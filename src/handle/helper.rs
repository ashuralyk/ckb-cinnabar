//! Filesystem helpers for `deployment/<network>/<contract>.json` records.

use std::{fs, path::PathBuf};

use chrono::prelude::Utc;
use ckb_cinnabar_calculator::{
    address::Address,
    error::CalculatorError,
    instruction::{Instruction, TransactionCalculator},
    re_exports::{
        ckb_hash::blake2b_256,
        ckb_jsonrpc_types::OutputsValidator,
        ckb_types::{prelude::Unpack, H256},
        eyre,
    },
    rpc::{Network, RpcClient, RPC},
};

use crate::object::*;

/// Path of the JSON record file for `contract_name` on `network`.
pub fn generate_contract_deployment_path(
    network: &Network,
    contract_name: &str,
    deployment_path: &str,
) -> PathBuf {
    PathBuf::new()
        .join(deployment_path)
        .join(network.to_string())
        .join(format!("{contract_name}.json"))
}

/// Load one record: a specific `version`, or the last record if `version` is `None`.
///
/// Returns `Ok(None)` when the file does not exist.
pub fn load_contract_deployment(
    network: &Network,
    contract_name: &str,
    deployment_path: &str,
    version: Option<&str>,
) -> eyre::Result<Option<DeploymentRecord>> {
    let path = generate_contract_deployment_path(network, contract_name, deployment_path);
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

/// Load every JSON record in the network directory, optionally filtered by name and [`ListMode`].
pub fn load_all_deployments(
    network: &Network,
    deployment_path: &str,
    contract_name: Option<&str>,
    mode: ListMode,
) -> eyre::Result<Vec<DeploymentRecord>> {
    let dir = PathBuf::new()
        .join(deployment_path)
        .join(network.to_string());
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut records = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Some(name) = contract_name {
            if path.file_stem().and_then(|s| s.to_str()) != Some(name) {
                continue;
            }
        }
        let file = fs::File::open(&path)?;
        let deployments: Vec<DeploymentRecord> = serde_json::from_reader(file)?;
        records.extend(deployments);
    }
    records.retain(|r| match mode {
        ListMode::All => true,
        ListMode::Deployed => r.operation != "consume",
        ListMode::Consumed => r.operation == "consume",
    });
    Ok(records)
}

/// Read a compiled RISC-V binary from `binary_path/<contract_name>` and return
/// `(bytes, blake2b_256)`.
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

/// JSON-RPC client for `network`. [`Network::Fake`] is rejected — use
/// `FakeRpcClient` in tests instead of the CLI.
pub fn create_rpc_from_network(network: &Network) -> eyre::Result<RpcClient> {
    match network {
        Network::Mainnet => Ok(RpcClient::new_mainnet()),
        Network::Testnet => Ok(RpcClient::new_testnet()),
        Network::Fake => Err(eyre::eyre!("fake network")),
        Network::Custom(url) => Ok(RpcClient::new(url.as_str(), None)),
    }
}

/// Print a [`CliResponse`]: pretty JSON when `json` is set, otherwise a one-line
/// hash, a TSV list, or an `kind: message` error on stderr.
pub fn print_response(json: bool, response: &CliResponse) {
    if json {
        println!("{}", serde_json::to_string_pretty(response).expect("json"));
    } else if let Some(hash) = &response.transaction_hash {
        println!("Transaction hash: {hash:#x}");
    } else if let Some(records) = &response.records {
        for r in records {
            println!(
                "{}\t{}\t{}\t{:#x}",
                r.name, r.version, r.operation, r.tx_hash
            );
        }
    } else if let Some(err) = &response.error {
        eprintln!("{}: {}", err.kind, err.message);
    }
}

/// Assemble `instructions`, optionally send, and persist a [`DeploymentRecord`].
///
/// `--dry-run` still returns a (unsigned, local) transaction hash and record
/// in the JSON envelope but does not write the file or broadcast.
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
    dry_run: bool,
    json: bool,
) -> eyre::Result<()> {
    let (skeleton, _) = TransactionCalculator::new(instructions)
        .new_skeleton(&rpc)
        .await?;
    let occupied_capacity = skeleton.outputs[0].occupied_capacity().as_u64();
    let type_id = skeleton.outputs[0].calc_type_hash();
    let type_id_args = skeleton.outputs[0]
        .type_script()
        .map(|s| H256::from_slice(&s.args().raw_data()).unwrap());
    let tx_view = skeleton.clone().into_transaction_view();
    let tx_hash = if dry_run {
        tx_view.hash().unpack()
    } else {
        rpc.send_transaction(tx_view.data().into(), Some(OutputsValidator::Passthrough))
            .await
            .map_err(CalculatorError::from)?
    };
    let deployment_record = DeploymentRecord {
        name: contract_name,
        date: Utc::now().to_rfc3339(),
        operation: operation.to_string(),
        version,
        tx_hash: tx_hash.clone(),
        out_index: 0,
        data_hash: contract_hash.map(Into::into),
        occupied_capacity,
        payer_address: payer_address.into(),
        contract_owner_address: contract_owner_address.into(),
        type_id,
        type_id_args,
        comment: None,
    };
    if !dry_run {
        save_contract_deployment(tx_path, deployment_record.clone())?;
    }
    let mut response = CliResponse::ok(operation, dry_run);
    response.transaction_hash = Some(tx_hash);
    response.record = Some(deployment_record);
    print_response(json, &response);
    Ok(())
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
