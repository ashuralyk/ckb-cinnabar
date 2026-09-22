//! Address, signing, and balance operations (host-only).
//!
//! Kernel types (`AddOutputCell`, `AddCellDep`, indexer collect, header deps)
//! live in [`crate::kernel::operation::basic`]. This module re-exports them.

#![allow(clippy::mutable_key_type)]

pub use crate::kernel::operation::basic::*;

use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

use ckb_jsonrpc_types::{JsonBytes, Transaction};
use ckb_types::{packed::CellOutput, prelude::Unpack, H160};
use serde_json::Value;

#[cfg(not(target_arch = "wasm32"))]
use ckb_sdk::{
    traits::DefaultCellDepResolver,
    transaction::signer::{SignContexts, TransactionSigner},
    types::transaction_with_groups::TransactionWithScriptGroupsBuilder,
    NetworkInfo,
};

#[cfg(not(target_arch = "wasm32"))]
use ckb_types::h256;

#[cfg(not(target_arch = "wasm32"))]
use secp256k1::SecretKey;

use crate::{
    address::Address,
    error::{self, CalculatorError},
    operation::{Log, Operation},
    rpc::{block_on_rpc, Host, RPC},
    skeleton::{CellDepEx, CellOutputEx, ChangeReceiver, ScriptEx, TransactionSkeleton},
    types::{self, h256_to_hash},
};

#[cfg(not(target_arch = "wasm32"))]
use crate::rpc::Network;

#[cfg(not(target_arch = "wasm32"))]
/// Operation that add secp256k1_sighash_all cell dep to transaction skeleton
pub struct AddSecp256k1SighashCellDep {}

#[cfg(not(target_arch = "wasm32"))]
impl<T: RPC + Host> Operation<T> for AddSecp256k1SighashCellDep {
    fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> error::Result<()> {
        let celldep = match rpc.network() {
            Network::Custom(_) => {
                let genesis = block_on_rpc(rpc.get_block_by_number(0.into()))?.unwrap();
                let resolver =
                    DefaultCellDepResolver::from_genesis(&genesis.clone().into()).expect("genesis");
                let (sighash_celldep, _) = resolver.sighash_dep().expect("sighash dep");
                let output: CellOutput = {
                    let tx_hash = sighash_celldep.out_point().tx_hash().unpack();
                    let tx = genesis
                        .transactions
                        .into_iter()
                        .find(|tx| tx.hash == tx_hash)
                        .unwrap();
                    let out_index: u32 = sighash_celldep.out_point().index().unpack();
                    tx.inner.outputs[out_index as usize].clone().into()
                };
                CellDepEx {
                    name: "secp256k1_sighash_all".to_string(),
                    celldep: sighash_celldep.clone(),
                    output: CellOutputEx::new(output, vec![]),
                    with_data: false,
                }
            }
            Network::Testnet => CellDepEx::new_from_outpoint(
                rpc,
                "secp256k1_sighash_all".to_string(),
                h256_to_hash(&h256!(
                    "0xf8de3bb47d055cdf460d93a2a6e1b05f7432f9777c8c474abf4eec1d4aee5d37"
                )),
                0,
                types::DepType::DepGroup,
                false,
            )?,
            Network::Mainnet => CellDepEx::new_from_outpoint(
                rpc,
                "secp256k1_sighash_all".to_string(),
                h256_to_hash(&h256!(
                    "0x71a7ba8fc96349fea0ed3a5c47992e3b4084b031a42264a018e0072e8172e46c"
                )),
                0,
                types::DepType::DepGroup,
                false,
            )?,
            _ => return Ok(()),
        };
        skeleton.celldep(celldep);
        Ok(())
    }
}

/// Operation that add input cell to transaction skeleton by user address
pub struct AddInputCellByAddress {
    /// Address whose lock script is searched; only plain capacity cells
    /// (no type script, empty data) are picked.
    pub address: Address,
}

impl<T: RPC> Operation<T> for AddInputCellByAddress {
    fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> error::Result<()> {
        skeleton
            .input_from_address(rpc, self.address.clone())?
            .witness(Default::default());
        Ok(())
    }
}

/// Operation that add output cell to transaction skeleton by address
pub struct AddOutputCellByAddress {
    /// Receiver address; used as the lock script.
    pub address: Address,
    /// Cell data.
    pub data: Vec<u8>,
    /// Attach a freshly computed type id as the type script.
    pub add_type_id: bool,
}

impl<T: RPC> Operation<T> for AddOutputCellByAddress {
    fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> error::Result<()> {
        Box::new(AddOutputCell {
            lock_script: self.address.payload().into(),
            type_script: None,
            capacity: 0,
            data: self.data,
            absolute_capacity: false,
            type_id: self.add_type_id,
        })
        .run(rpc, skeleton, log)
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// Operation that sign and add secp256k1_sighash_all signatures to transaction skeleton
pub struct AddSecp256k1SighashSignatures {
    /// Lock scripts to group inputs/outputs by; one signature per script group.
    pub user_lock_scripts: Vec<ScriptEx>,
    /// Private keys corresponding to the sighash args in `user_lock_scripts`.
    pub user_private_keys: Vec<SecretKey>,
}

#[cfg(not(target_arch = "wasm32"))]
impl<T: RPC> Operation<T> for AddSecp256k1SighashSignatures {
    fn run(
        self: Box<Self>,
        _: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> error::Result<()> {
        let tx = skeleton.clone().into_transaction_view();
        let mut tx_groups_builder = TransactionWithScriptGroupsBuilder::default().set_tx_view(tx);
        for lock_script in self.user_lock_scripts {
            let (input_indices, _) = skeleton.lock_script_groups(&lock_script);
            tx_groups_builder = tx_groups_builder
                .add_lock_script_group(&lock_script.to_script(skeleton)?, &input_indices);
        }
        let mut tx_groups = tx_groups_builder.build();
        let signer = TransactionSigner::new(&NetworkInfo::mainnet());
        signer
            .sign_transaction(
                &mut tx_groups,
                &SignContexts::new_sighash(self.user_private_keys),
            )
            .expect("sign");
        let tx = tx_groups.get_tx_view();
        skeleton.update_witnesses_from_transaction_view(tx)?;
        Ok(())
    }
}

/// Multisig config section of a ckb-cli tx file (JSON mirror).
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ReprMultisigConfig {
    /// Sighash addresses that participate in the multisig.
    pub sighash_addresses: Vec<String>,
    /// First N addresses that must always sign.
    pub require_first_n: u8,
    /// Signature threshold.
    pub threshold: u8,
}

/// ckb-cli `tx` file format used to hand a transaction over for signing.
///
/// Copy from https://github.com/nervosnetwork/ckb-cli/blob/develop/src/subcommands/tx.rs#L710
#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct ReprTxHelper {
    /// Packed transaction body in JSON-RPC form.
    pub transaction: Transaction,
    /// Multisig configs keyed by the sighash address hash160.
    pub multisig_configs: HashMap<H160, ReprMultisigConfig>,
    /// Collected signatures keyed by lock-script hash.
    pub signatures: HashMap<JsonBytes, Vec<JsonBytes>>,
}

/// Operation that sign and add secp256k1_sighash_all signatures to transaction skeleton with ckb-cli
///
/// note: this operation requires `ckb-cli` installed and available in PATH, refer to https://github.com/nervosnetwork/ckb-cli
pub struct AddSecp256k1SighashSignaturesWithCkbCli {
    /// Address that pays/unlocks; the first input group with its lock is signed.
    pub signer_address: Address,
    /// Directory where the intermediate tx JSON file is written.
    pub cache_path: PathBuf,
    /// Keep the intermediate tx file after signing (useful for debugging).
    pub keep_cache_file: bool,
}

#[cfg(not(target_arch = "wasm32"))]
fn get_cli_password() -> error::Result<String> {
    rpassword::prompt_password("Enter password to unlock ckb-cli: ")
        .map_err(|e| CalculatorError::Signing(e.to_string()))
}

#[cfg(target_arch = "wasm32")]
fn get_cli_password() -> error::Result<String> {
    Err(CalculatorError::Signing(
        "ckb-cli is not supported in wasm environment".into(),
    ))
}

impl<T: RPC + Host> Operation<T> for AddSecp256k1SighashSignaturesWithCkbCli {
    fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> error::Result<()> {
        let (signer_groups, _) = skeleton.lock_script_groups(&self.signer_address.payload().into());
        let witness_index = signer_groups
            .first()
            .cloned()
            .ok_or_else(|| CalculatorError::Signing("no signer address found".into()))?;
        if skeleton.witnesses.len() <= witness_index {
            return Err(CalculatorError::Signing(
                "witnesses count not match all of inputs".into(),
            ));
        }
        let tx = skeleton.clone().into_transaction_view();
        let tx_hash = hex::encode(tx.hash().raw_data());
        let cache_dir = PathBuf::new().join(self.cache_path);
        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir)?;
        }
        let ckb_cli_tx = ReprTxHelper {
            transaction: tx.data().into(),
            ..Default::default()
        };
        let tx_content = serde_json::to_string_pretty(&ckb_cli_tx)
            .map_err(|e| CalculatorError::Signing(e.to_string()))?;
        let tx_file = cache_dir.join(format!("tx-{tx_hash}-{witness_index}.json"));
        fs::write(&tx_file, tx_content)?;
        let password = get_cli_password()?;
        let (url, _) = rpc.url();
        let mut ckb_cli = Command::new("ckb-cli")
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .args(["--url", &url])
            .args(["tx", "sign-inputs"])
            .args(["--tx-file", tx_file.to_str().unwrap()])
            .args(["--from-account", &self.signer_address.to_string()])
            .args(["--output-format", "json"])
            .arg("--add-signatures")
            .spawn()?;
        ckb_cli
            .stdin
            .as_mut()
            .ok_or_else(|| CalculatorError::Signing("stdin not available".into()))?
            .write_all(password.as_bytes())?;
        let output = ckb_cli.wait_with_output()?;
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(CalculatorError::Signing(format!("ckb-cli error: {error}")));
        }
        if !self.keep_cache_file {
            fs::remove_file(&tx_file)?;
        }
        let ckb_cli_result = String::from_utf8(output.stdout)
            .map_err(|e| CalculatorError::Signing(e.to_string()))?;
        let signature_json: Vec<Value> =
            serde_json::from_str(ckb_cli_result.trim_start_matches("Password:").trim())
                .map_err(|e| CalculatorError::Signing(e.to_string()))?;
        let signature = signature_json
            .first()
            .ok_or_else(|| CalculatorError::Signing("signature not generated".into()))?
            .get("signature")
            .ok_or_else(|| CalculatorError::Signing("signature not found".into()))?
            .as_str()
            .ok_or_else(|| CalculatorError::Signing("signature not string format".into()))?;
        let signature_bytes = hex::decode(signature.trim_start_matches("0x"))
            .map_err(|e| CalculatorError::Signing(e.to_string()))?;
        skeleton.witnesses[witness_index].lock = signature_bytes;
        Ok(())
    }
}

/// Operation that balance transaction skeleton
pub struct BalanceTransaction {
    /// Lock script used to search for extra capacity input cells.
    pub balancer: ScriptEx,
    /// Where the change capacity goes (new cell by address/script, or an
    /// existing output index).
    pub change_receiver: ChangeReceiver,
    /// Extra fee rate (shannons/byte) added on top of the node's min fee rate.
    pub additional_fee_rate: u64,
}

impl<T: RPC> Operation<T> for BalanceTransaction {
    fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> error::Result<()> {
        let fee = skeleton.fee(rpc, self.additional_fee_rate)?;
        skeleton.balance(rpc, fee, self.balancer, self.change_receiver)?;
        (skeleton.witnesses.len()..skeleton.inputs.len()).for_each(|_| {
            skeleton.witness(Default::default());
        });
        Ok(())
    }
}
