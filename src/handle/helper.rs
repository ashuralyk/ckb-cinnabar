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
    rpc::{Host, Network, RpcClient, RPC},
    skeleton::TransactionSkeleton,
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

/// Load one record: the latest row with `version`, or the last row if `version` is `None`.
///
/// Returns `Ok(None)` when the file does not exist or the tag is absent.
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
            Ok(deployments.into_iter().rev().find(|r| r.version == version))
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
    Ok(match mode {
        ListMode::All => records,
        ListMode::Deployed => records_at_latest_tag(records, false),
        ListMode::Consumed => records_at_latest_tag(records, true),
    })
}

/// Keep the last row of each `(name, version)`.
///
/// `consumed` selects tags whose latest operation is `consume`. Otherwise the
/// latest operation is `deploy` or `migrate`.
fn records_at_latest_tag(records: Vec<DeploymentRecord>, consumed: bool) -> Vec<DeploymentRecord> {
    use std::collections::HashMap;
    let mut latest_index = HashMap::<(String, String), usize>::new();
    for (index, record) in records.iter().enumerate() {
        latest_index.insert((record.name.clone(), record.version.clone()), index);
    }
    records
        .into_iter()
        .enumerate()
        .filter(|(index, record)| {
            let key = (record.name.clone(), record.version.clone());
            latest_index.get(&key).copied() == Some(*index)
                && (record.operation == "consume") == consumed
        })
        .map(|(_, record)| record)
        .collect()
}

/// Contract-cell fields stored on a [`DeploymentRecord`].
pub(crate) struct RecordedCell {
    out_index: u32,
    occupied_capacity: u64,
    type_id: Option<H256>,
    type_id_args: Option<H256>,
}

/// How [`send_and_record_transaction`] fills those fields.
pub(crate) enum CellSnapshot {
    /// Contract cell is `outputs[0]` after assembly (deploy / migrate).
    FirstOutput,
    /// Contract cell was spent as an input. Do not copy the change output.
    Consumed { occupied_capacity: u64 },
}

/// Consume record: keep the spent cell's capacity, and leave type id empty
/// so the row is not mistaken for the change cell.
pub(crate) fn recorded_cell_for_consume(occupied_capacity: u64) -> RecordedCell {
    RecordedCell {
        out_index: 0,
        occupied_capacity,
        type_id: None,
        type_id_args: None,
    }
}

fn recorded_cell_from_first_output(skeleton: &TransactionSkeleton) -> eyre::Result<RecordedCell> {
    let output = skeleton
        .outputs
        .first()
        .ok_or_else(|| eyre::eyre!("contract output missing"))?;
    let type_id_args = output
        .type_script()
        .map(|script| {
            H256::from_slice(&script.args().raw_data())
                .map_err(|err| eyre::eyre!("type id args: {err}"))
        })
        .transpose()?;
    Ok(RecordedCell {
        out_index: 0,
        occupied_capacity: output.occupied_capacity().as_u64(),
        type_id: output.calc_type_hash().map(Into::into),
        type_id_args,
    })
}

/// Read a compiled RISC-V binary from `binary_path/<contract_name>` and return
/// `(bytes, blake2b_256)`.
pub fn load_contract_binary(
    contract_name: &str,
    binary_path: &str,
) -> eyre::Result<(Vec<u8>, [u8; 32])> {
    let contract_path = PathBuf::new().join(binary_path).join(contract_name);
    let contract_binary = fs::read(&contract_path).map_err(|e| {
        eyre::Report::new(CalculatorError::Io(format!(
            "{e}:{}",
            contract_path.to_string_lossy()
        )))
    })?;
    let contract_hash = blake2b_256(&contract_binary);
    Ok((contract_binary, contract_hash))
}

/// JSON-RPC client for `network`. [`Network::Fake`] is rejected — use
/// `FakeRpcClient` in tests instead of the CLI.
pub fn create_rpc_from_network(network: &Network) -> eyre::Result<RpcClient> {
    match network {
        Network::Mainnet => Ok(RpcClient::new_mainnet()),
        Network::Testnet => Ok(RpcClient::new_testnet()),
        Network::Fake => Err(CliError::FakeNetwork.into()),
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
pub async fn send_and_record_transaction<T: RPC + Host>(
    rpc: T,
    instructions: Vec<Instruction<T>>,
    tx_path: PathBuf,
    operation: &str,
    contract_name: String,
    version: String,
    contract_hash: Option<[u8; 32]>,
    payer_address: Address,
    contract_owner_address: Option<Address>,
    cell_snapshot: CellSnapshot,
    dry_run: bool,
    json: bool,
) -> eyre::Result<()> {
    let (skeleton, _) = TransactionCalculator::new(instructions).new_skeleton(&rpc)?;
    let cell = match cell_snapshot {
        CellSnapshot::FirstOutput => recorded_cell_from_first_output(&skeleton)?,
        CellSnapshot::Consumed { occupied_capacity } => {
            recorded_cell_for_consume(occupied_capacity)
        }
    };
    let tx_view = skeleton.into_transaction_view();
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
        out_index: cell.out_index,
        data_hash: contract_hash.map(Into::into),
        occupied_capacity: cell.occupied_capacity,
        payer_address: payer_address.into(),
        contract_owner_address: contract_owner_address.into(),
        type_id: cell.type_id,
        type_id_args: cell.type_id_args,
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
        let parent = path.parent().ok_or_else(|| {
            eyre::Report::new(CalculatorError::Io(format!(
                "deployment record path has no parent: {}",
                path.to_string_lossy()
            )))
        })?;
        fs::create_dir_all(parent)?;
        Vec::new()
    };
    records.push(record);
    let new_content = serde_json::to_string_pretty(&records)?;
    fs::write(path, new_content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(version: &str, operation: &str) -> DeploymentRecord {
        DeploymentRecord {
            name: "my_lock".into(),
            version: version.into(),
            operation: operation.into(),
            occupied_capacity: 100,
            ..DeploymentRecord::default()
        }
    }

    fn write_history(dir: &std::path::Path, records: &[DeploymentRecord]) {
        let path =
            generate_contract_deployment_path(&Network::Testnet, "my_lock", &dir.to_string_lossy());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_string_pretty(records).unwrap()).unwrap();
    }

    #[test]
    fn load_by_tag_returns_the_latest_record() {
        let dir = std::env::temp_dir().join(format!("cinnabar-ledger-load-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        write_history(
            &dir,
            &[record("v0.1.0", "deploy"), record("v0.1.0", "consume")],
        );

        let found = load_contract_deployment(
            &Network::Testnet,
            "my_lock",
            &dir.to_string_lossy(),
            Some("v0.1.0"),
        )
        .unwrap()
        .unwrap();

        assert_eq!(found.operation, "consume");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_deployed_keeps_only_tags_whose_latest_record_is_live() {
        let dir = std::env::temp_dir().join(format!("cinnabar-ledger-list-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        write_history(
            &dir,
            &[
                record("v0.1.0", "deploy"),
                record("v0.1.0", "consume"),
                record("v0.2.0", "deploy"),
            ],
        );

        let deployed = load_all_deployments(
            &Network::Testnet,
            &dir.to_string_lossy(),
            Some("my_lock"),
            ListMode::Deployed,
        )
        .unwrap();
        let consumed = load_all_deployments(
            &Network::Testnet,
            &dir.to_string_lossy(),
            Some("my_lock"),
            ListMode::Consumed,
        )
        .unwrap();

        assert_eq!(
            deployed
                .iter()
                .map(|r| r.version.as_str())
                .collect::<Vec<_>>(),
            vec!["v0.2.0"]
        );
        assert_eq!(
            consumed
                .iter()
                .map(|r| r.version.as_str())
                .collect::<Vec<_>>(),
            vec!["v0.1.0"]
        );
        assert_eq!(consumed[0].operation, "consume");
        let all = load_all_deployments(
            &Network::Testnet,
            &dir.to_string_lossy(),
            Some("my_lock"),
            ListMode::All,
        )
        .unwrap();
        assert_eq!(all.len(), 3);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_rejects_a_record_path_with_no_parent() {
        let err = save_contract_deployment(PathBuf::new(), record("v0.1.0", "deploy")).unwrap_err();
        let body = CliErrorBody::from_report(&err);
        assert_eq!(body.kind, "io");
        assert!(body.message.contains("no parent"));
    }

    #[test]
    fn consume_record_keeps_the_tag_and_not_a_change_cell() {
        let cell = recorded_cell_for_consume(42);
        assert_eq!(cell.occupied_capacity, 42);
        assert_eq!(cell.out_index, 0);
        assert!(cell.type_id.is_none());
        assert!(cell.type_id_args.is_none());
    }
}
