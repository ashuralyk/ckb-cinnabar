#![allow(clippy::mutable_key_type)]

use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

use async_trait::async_trait;
use ckb_jsonrpc_types::{JsonBytes, Transaction};
use ckb_sdk::{
    constants::TYPE_ID_CODE_HASH,
    rpc::ckb_indexer::{SearchKey, SearchMode},
    traits::{CellQueryOptions, DefaultCellDepResolver, ValueRangeOption},
    transaction::signer::{SignContexts, TransactionSigner},
    types::transaction_with_groups::TransactionWithScriptGroupsBuilder,
    Address, NetworkInfo,
};
use ckb_types::{
    core::{Capacity, DepType},
    h256,
    packed::CellOutput,
    prelude::{Builder, Entity, Pack, Unpack},
    H160, H256,
};
use eyre::{eyre, Result};
use secp256k1::SecretKey;
use serde_json::Value;

use crate::{
    operation::{Log, Operation},
    rpc::{GetCellsIter, Network, RPC},
    skeleton::{
        CellDepEx, CellInputEx, CellOutputEx, ChangeReceiver, HeaderDepEx, ScriptEx,
        TransactionSkeleton, WitnessEx,
    },
};

/// Operation that add cell dep to transaction skeleton by tx hash with index
pub struct AddCellDep {
    pub name: String,
    pub tx_hash: H256,
    pub index: u32,
    pub dep_type: DepType,
    pub with_data: bool,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddCellDep {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        if skeleton.get_celldep_by_name(&self.name).is_none() {
            let cell_dep = CellDepEx::new_from_outpoint(
                rpc,
                self.name,
                self.tx_hash,
                self.index,
                self.dep_type,
                self.with_data,
            )
            .await?;
            skeleton.celldep(cell_dep);
        }
        Ok(())
    }
}

/// Operation that add cell dep to transaction skeleton by type script, which is type id for specific
pub struct AddCellDepByType {
    pub name: String,
    pub type_script: ScriptEx,
    pub dep_type: DepType,
    pub with_data: bool,
}

impl AddCellDepByType {
    fn search_key(&self, skeleton: &TransactionSkeleton) -> Result<SearchKey> {
        let mut query = CellQueryOptions::new_type(self.type_script.clone().to_script(skeleton)?);
        query.script_search_mode = Some(SearchMode::Exact);
        if self.with_data {
            query.with_data = Some(true);
        }
        Ok(query.into())
    }
}

#[async_trait]
impl<T: RPC> Operation<T> for AddCellDepByType {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        if skeleton.get_celldep_by_name(&self.name).is_none() {
            let mut find_avaliable = false;
            let mut iter = GetCellsIter::new(rpc, self.search_key(skeleton)?);
            if let Some(cell) = iter.next().await? {
                let cell_dep = CellDepEx::new_from_indexer_cell(self.name, cell, self.dep_type);
                find_avaliable = true;
                skeleton.celldep(cell_dep);
            }
            if !find_avaliable {
                return Err(eyre!("cell dep not found"));
            }
        }
        Ok(())
    }
}

/// Operation that add secp256k1_sighash_all cell dep to transaction skeleton
pub struct AddSecp256k1SighashCellDep {}

#[async_trait]
impl<T: RPC> Operation<T> for AddSecp256k1SighashCellDep {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let celldep = match rpc.network() {
            Network::Custom(_) => {
                let genesis = rpc.get_block_by_number(0.into()).await?.unwrap();
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
            Network::Testnet => {
                CellDepEx::new_from_outpoint(
                    rpc,
                    "secp256k1_sighash_all".to_string(),
                    h256!("0xf8de3bb47d055cdf460d93a2a6e1b05f7432f9777c8c474abf4eec1d4aee5d37"),
                    0,
                    DepType::DepGroup,
                    false,
                )
                .await?
            }
            Network::Mainnet => {
                CellDepEx::new_from_outpoint(
                    rpc,
                    "secp256k1_sighash_all".to_string(),
                    h256!("0x71a7ba8fc96349fea0ed3a5c47992e3b4084b031a42264a018e0072e8172e46c"),
                    0,
                    DepType::DepGroup,
                    false,
                )
                .await?
            }
            _ => return Err(eyre!("secp256k1_sighash_all not valid for fake network")),
        };
        skeleton.celldep(celldep);
        Ok(())
    }
}

/// Operation that add a standalone header dep to transaction without linking to any input cell
pub struct AddHeaderDep {
    pub block_hash: H256,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddHeaderDep {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let header_dep = HeaderDepEx::new(rpc, self.block_hash, None).await?;
        skeleton.headerdep(header_dep);
        Ok(())
    }
}

/// Operation that add a header dep to transaction by block number
pub struct AddHeaderDepByBlockNumber {
    pub block_number: u64,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddHeaderDepByBlockNumber {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let block_hash = rpc
            .get_block_hash(self.block_number.into())
            .await?
            .ok_or(eyre!(
                "block hash not found for block number {}",
                self.block_number
            ))?;
        let header_dep = HeaderDepEx::new(rpc, block_hash, None).await?;
        skeleton.headerdep(header_dep);
        Ok(())
    }
}

/// Operation that add a header dep to transaction by input index, which will link to that input cell
pub struct AddHeaderDepByInputIndex {
    pub input_index: usize,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddHeaderDepByInputIndex {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let input = skeleton.get_input_by_index(self.input_index)?;
        let cell_outpoint = input.input.previous_output();
        skeleton.headerdep(HeaderDepEx::new_from_outpoint(rpc, cell_outpoint).await?);
        Ok(())
    }
}

/// Operation that add input cell to transaction skeleton by lock script
///
/// # Parameters
/// - `count`: u32, the count of input cells to add that searching coming out of ckb-indexer
pub struct AddInputCell {
    pub lock_script: ScriptEx,
    pub type_script: Option<ScriptEx>,
    pub count: u32,
    pub search_mode: SearchMode,
}

impl AddInputCell {
    fn search_key(&self, skeleton: &TransactionSkeleton) -> Result<SearchKey> {
        let mut query = CellQueryOptions::new_lock(self.lock_script.clone().to_script(skeleton)?);
        if let Some(type_script) = &self.type_script {
            query.secondary_script = Some(type_script.clone().to_script(skeleton)?);
        } else {
            query.secondary_script_len_range = Some(ValueRangeOption::new(0, 1));
            query.data_len_range = Some(ValueRangeOption::new(0, 1));
        }
        query.with_data = Some(true);
        query.script_search_mode = Some(self.search_mode.clone());
        Ok(query.into())
    }
}

#[async_trait]
impl<T: RPC> Operation<T> for AddInputCell {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let mut iter = GetCellsIter::new(rpc, self.search_key(skeleton)?);
        let mut find_avaliable = false;
        while let Some(cells) = iter.next_batch(self.count).await? {
            cells.into_iter().try_for_each(|cell| {
                let cell_input = CellInputEx::new_from_indexer_cell(cell, None);
                find_avaliable = true;
                skeleton.input(cell_input)?.witness(Default::default());
                Result::<()>::Ok(())
            })?;
        }
        if !find_avaliable {
            return Err(eyre!("input cell not found"));
        }
        Ok(())
    }
}

/// Operation that add input cell to transaction skeleton by out point directly
pub struct AddInputCellByOutPoint {
    pub tx_hash: H256,
    pub index: u32,
    pub since: Option<u64>,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddInputCellByOutPoint {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let cell_input =
            CellInputEx::new_from_outpoint(rpc, self.tx_hash, self.index, self.since, true).await?;
        skeleton.input(cell_input)?.witness(Default::default());
        Ok(())
    }
}

/// Operation that add input cell to transaction skeleton by user address
pub struct AddInputCellByAddress {
    pub address: Address,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddInputCellByAddress {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        skeleton
            .input_from_address(rpc, self.address.clone())
            .await?
            .witness(Default::default());
        Ok(())
    }
}

/// Operation that add input cell to transaction skeleton by type script
pub struct AddInputCellByType {
    pub type_script: ScriptEx,
    pub count: u32,
    pub search_mode: SearchMode,
}

impl AddInputCellByType {
    fn search_key(&self, skeleton: &TransactionSkeleton) -> Result<SearchKey> {
        let mut query = CellQueryOptions::new_type(self.type_script.clone().to_script(skeleton)?);
        query.script_search_mode = Some(self.search_mode.clone());
        query.with_data = Some(true);
        Ok(query.into())
    }
}

#[async_trait]
impl<T: RPC> Operation<T> for AddInputCellByType {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let mut iter = GetCellsIter::new(rpc, self.search_key(skeleton)?);
        let mut find_avaliable = false;
        while let Some(cells) = iter.next_batch(self.count).await? {
            cells.into_iter().try_for_each(|cell| {
                let cell_input = CellInputEx::new_from_indexer_cell(cell, None);
                find_avaliable = true;
                skeleton.input(cell_input)?.witness(Default::default());
                Result::<()>::Ok(())
            })?;
        }
        if !find_avaliable {
            return Err(eyre!("input cell not found"));
        }
        Ok(())
    }
}

/// Operation that add output cell to transaction skeleton
///
/// # Parameters
/// - `absolute_capacity` bool, wether mark the `capacity` as absolute value or additional
/// - `type_id`: bool, if true, calculate type id and override into type script if provided
#[derive(Default)]
pub struct AddOutputCell {
    pub lock_script: ScriptEx,
    pub type_script: Option<ScriptEx>,
    pub capacity: u64,
    pub data: Vec<u8>,
    pub absolute_capacity: bool,
    pub type_id: bool,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddOutputCell {
    async fn run(
        self: Box<Self>,
        _: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let type_script = if self.type_id {
            let type_id = skeleton.calc_type_id(skeleton.outputs.len())?;
            let type_script = self
                .type_script
                .map(|v| v.set_args(type_id.as_bytes().to_vec()))
                .unwrap_or(ScriptEx::new_type(
                    TYPE_ID_CODE_HASH.clone(),
                    type_id.as_bytes().to_vec(),
                ));
            Some(type_script.to_script(skeleton)?)
        } else {
            self.type_script
                .map(|v| v.to_script(skeleton))
                .transpose()?
        };
        let mut output = CellOutput::new_builder()
            .lock(self.lock_script.to_script(skeleton)?)
            .type_(type_script.pack())
            .build();
        output = output
            .as_builder()
            .build_exact_capacity(Capacity::bytes(self.data.len())?)?;
        let minimal_capacity: u64 = output.capacity().unpack();
        if !self.absolute_capacity {
            let capacity = minimal_capacity + self.capacity;
            output = output.as_builder().capacity(capacity.pack()).build();
        } else if self.capacity > minimal_capacity {
            output = output.as_builder().capacity(self.capacity.pack()).build();
        } else {
            return Err(eyre!("capacity not enough"));
        }
        let cell_output = CellOutputEx::new(output, self.data);
        skeleton.output(cell_output);
        Ok(())
    }
}

/// Operation that add output cell to transaction skeleton by address
pub struct AddOutputCellByAddress {
    pub address: Address,
    pub data: Vec<u8>,
    pub add_type_id: bool,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddOutputCellByAddress {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        Box::new(AddOutputCell {
            lock_script: self.address.payload().into(),
            type_script: None,
            capacity: 0,
            data: self.data,
            absolute_capacity: false,
            type_id: self.add_type_id,
        })
        .run(rpc, skeleton, log)
        .await
    }
}

/// Operation that add output cell to transaction skeleton by copying input cell from target position
///
/// # Parameters
/// - `input_index`: usize, the index of input cell in inputs, if it is usize::MAX, copy the last one
/// - `adjust_capacity`: bool, if true, adjust the capacity if `data` provided
#[derive(Default)]
pub struct AddOutputCellByInputIndex {
    pub input_index: usize,
    pub data: Option<Vec<u8>>,
    pub lock_script: Option<ScriptEx>,
    pub type_script: Option<Option<ScriptEx>>,
    pub adjust_capacity: bool,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddOutputCellByInputIndex {
    async fn run(
        self: Box<Self>,
        _: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let cell_input = skeleton.get_input_by_index(self.input_index)?;
        let mut cell_output = cell_input.output.clone();
        let mut output_builder = cell_output.output.as_builder();
        if let Some(data) = self.data {
            cell_output.data = data;
        }
        if let Some(lock_script) = self.lock_script {
            output_builder = output_builder.lock(lock_script.to_script(skeleton)?);
        }
        if let Some(type_script) = self.type_script {
            if let Some(type_script) = type_script {
                output_builder =
                    output_builder.type_(Some(type_script.to_script(skeleton)?).pack());
            } else {
                output_builder = output_builder.type_(None.pack());
            }
        }
        cell_output.output = if self.adjust_capacity {
            output_builder.build_exact_capacity(Capacity::bytes(cell_output.data.len())?)?
        } else {
            output_builder.build()
        };
        skeleton.output(cell_output);
        Ok(())
    }
}

/// Operation that add wintess in form of WitnessArgs to transaction skeleton
///
/// `witness_index`: Option<usize>, the index of witness to update, if None, add a new witness
pub struct AddWitnessArgs {
    pub witness_index: Option<usize>,
    pub lock: Vec<u8>,
    pub input_type: Vec<u8>,
    pub output_type: Vec<u8>,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddWitnessArgs {
    async fn run(
        self: Box<Self>,
        _: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        if let Some(witness_index) = self.witness_index {
            if witness_index >= skeleton.witnesses.len() {
                return Err(eyre!("witness index out of range"));
            }
            let witness = &mut skeleton.witnesses[witness_index];
            witness.lock = self.lock;
            witness.input_type = self.input_type;
            witness.output_type = self.output_type;
        } else {
            let witness = WitnessEx::new(self.lock, self.input_type, self.output_type);
            skeleton.witness(witness);
        }
        Ok(())
    }
}

/// Operation that sign and add secp256k1_sighash_all signatures to transaction skeleton
pub struct AddSecp256k1SighashSignatures {
    pub user_lock_scripts: Vec<ScriptEx>,
    pub user_private_keys: Vec<SecretKey>,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddSecp256k1SighashSignatures {
    async fn run(
        self: Box<Self>,
        _: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let tx = skeleton.clone().into_transaction_view();
        let mut tx_groups_builder = TransactionWithScriptGroupsBuilder::default().set_tx_view(tx);
        for lock_script in self.user_lock_scripts {
            let (input_indices, _) = skeleton.lock_script_groups(&lock_script);
            tx_groups_builder = tx_groups_builder
                .add_lock_script_group(&lock_script.to_script(skeleton)?, &input_indices);
        }
        let mut tx_groups = tx_groups_builder.build();
        let signer = TransactionSigner::new(&NetworkInfo::mainnet()); // network info is not used here
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

/// Copy from https://github.com/nervosnetwork/ckb-cli/blob/develop/src/subcommands/tx.rs#L783
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ReprMultisigConfig {
    pub sighash_addresses: Vec<String>,
    pub require_first_n: u8,
    pub threshold: u8,
}

/// Copy from https://github.com/nervosnetwork/ckb-cli/blob/develop/src/subcommands/tx.rs#L710
#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct ReprTxHelper {
    pub transaction: Transaction,
    pub multisig_configs: HashMap<H160, ReprMultisigConfig>,
    pub signatures: HashMap<JsonBytes, Vec<JsonBytes>>,
}

/// Operation that sign and add secp256k1_sighash_all signatures to transaction skeleton with ckb-cli
///
/// note: this operation requires `ckb-cli` installed and available in PATH, refer to https://github.com/nervosnetwork/ckb-cli
pub struct AddSecp256k1SighashSignaturesWithCkbCli {
    pub signer_address: Address,
    pub cache_path: PathBuf,
    pub keep_cache_file: bool,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddSecp256k1SighashSignaturesWithCkbCli {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        // complete witness if not enough
        let (signer_groups, _) = skeleton.lock_script_groups(&self.signer_address.payload().into());
        let witness_index = signer_groups
            .first()
            .cloned()
            .ok_or(eyre!("no signer address found"))?;
        if skeleton.witnesses.len() <= witness_index {
            return Err(eyre!("witnesses count not match all of inputs"));
        }
        // generate persisted tx file in cahce directory for ckb-cli
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
        let tx_content = serde_json::to_string_pretty(&ckb_cli_tx)?;
        let tx_file = cache_dir.join(format!("tx-{tx_hash}-{witness_index}.json"));
        fs::write(&tx_file, tx_content)?;
        // read password for unlocking ckb-cli
        let password = rpassword::prompt_password("Enter password to unlock ckb-cli: ")?;
        // run ckb-cli to sign the tx
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
            .ok_or(eyre!("stdin not available"))?
            .write_all(password.as_bytes())?;
        let output = ckb_cli.wait_with_output()?;
        if !output.status.success() {
            let error = String::from_utf8(output.stderr)?;
            return Err(eyre!("ckb-cli error: {error}"));
        }
        if !self.keep_cache_file {
            fs::remove_file(&tx_file)?;
        }
        // fill in signature
        let ckb_cli_result = String::from_utf8(output.stdout)?;
        let signature_json: Vec<Value> =
            serde_json::from_str(ckb_cli_result.trim_start_matches("Password:").trim())?;
        let signature = signature_json
            .first()
            .ok_or(eyre!("signature not generated"))?
            .get("signature")
            .ok_or(eyre!("signature not found"))?
            .as_str()
            .ok_or(eyre!("signature not string format"))?;
        let signature_bytes = hex::decode(signature.trim_start_matches("0x"))?;
        skeleton.witnesses[witness_index].lock = signature_bytes;
        Ok(())
    }
}

/// Operation that balance transaction skeleton
pub struct BalanceTransaction {
    pub balancer: ScriptEx,
    pub change_receiver: ChangeReceiver,
    pub additional_fee_rate: u64,
}

#[async_trait]
impl<T: RPC> Operation<T> for BalanceTransaction {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let fee = skeleton.fee(rpc, self.additional_fee_rate).await?;
        skeleton
            .balance(rpc, fee, self.balancer, self.change_receiver)
            .await?;
        (skeleton.witnesses.len()..skeleton.inputs.len()).for_each(|_| {
            skeleton.witness(Default::default());
        });
        Ok(())
    }
}
