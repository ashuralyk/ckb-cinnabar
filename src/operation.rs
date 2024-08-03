use async_trait::async_trait;
use ckb_sdk::{
    constants::TYPE_ID_CODE_HASH,
    rpc::ckb_indexer::{SearchKey, SearchMode},
    traits::{CellQueryOptions, DefaultCellDepResolver},
    transaction::signer::{SignContexts, TransactionSigner},
    types::transaction_with_groups::TransactionWithScriptGroupsBuilder,
    Address, NetworkInfo,
};
use ckb_types::{
    core::{Capacity, DepType, ScriptHashType},
    packed::{CellOutput, Script},
    prelude::{Builder, Entity, Pack, Unpack},
    H256,
};
use eyre::{eyre, Result};
use secp256k1::SecretKey;

use crate::{
    rpc::{GetCellsIter, RPC},
    skeleton::{
        CellDepEx, CellInputEx, CellOutputEx, ChangeReceiver, TransactionSkeleton, WitnessArgsEx,
    },
};

#[async_trait]
pub trait Operation {
    fn search_key(&self) -> SearchKey {
        unimplemented!("search_key not implemented");
    }
    async fn run<T: RPC>(self, rpc: &T, skeleton: &mut TransactionSkeleton) -> Result<()>;
}

/// Operation that add cell dep to transaction skeleton by tx hash with index
pub struct AddCellDep {
    pub tx_hash: H256,
    pub index: u32,
    pub dep_type: DepType,
    pub with_data: bool,
}

#[async_trait]
impl Operation for AddCellDep {
    async fn run<T: RPC>(self, rpc: &T, skeleton: &mut TransactionSkeleton) -> Result<()> {
        let cell_dep = CellDepEx::new_from_outpoint(
            rpc,
            self.tx_hash,
            self.index,
            self.dep_type,
            self.with_data,
        )
        .await?;
        skeleton.celldep(cell_dep);
        Ok(())
    }
}

/// Operation that add cell dep to transaction skeleton by type script, which is type id for specific
pub struct AddCellDepByType {
    pub type_script: Script,
    pub dep_type: DepType,
    pub with_data: bool,
}

#[async_trait]
impl Operation for AddCellDepByType {
    fn search_key(&self) -> SearchKey {
        let mut query = CellQueryOptions::new_type(self.type_script.clone());
        query.script_search_mode = Some(SearchMode::Exact);
        if self.with_data {
            query.with_data = Some(true);
        }
        query.into()
    }

    async fn run<T: RPC>(self, rpc: &T, skeleton: &mut TransactionSkeleton) -> Result<()> {
        let mut find_avaliable = false;
        let mut iter = GetCellsIter::new(rpc, self.search_key());
        if let Some(cell) = iter.next().await? {
            let cell_dep = CellDepEx::new_from_indexer_cell(cell, self.dep_type);
            find_avaliable = true;
            skeleton.celldep(cell_dep);
        }
        if !find_avaliable {
            return Err(eyre!("cell dep not found"));
        }
        Ok(())
    }
}

/// Operation that add secp256k1_sighash_all cell dep to transaction skeleton
pub struct AddSecp256k1SighashCellDep {}

#[async_trait]
impl Operation for AddSecp256k1SighashCellDep {
    async fn run<T: RPC>(self, rpc: &T, skeleton: &mut TransactionSkeleton) -> Result<()> {
        let genesis = rpc.get_block_by_number(0.into()).await?.unwrap();
        let resolver = DefaultCellDepResolver::from_genesis(&genesis.into()).expect("genesis");
        let (sighash_celldep, _) = resolver.sighash_dep().expect("sighash dep");
        skeleton.celldep(CellDepEx {
            cell_dep: sighash_celldep.clone(),
            output: None,
        });
        Ok(())
    }
}

/// Operation that add input cell to transaction skeleton by lock script
///
/// `count`: u32, the count of input cells to add that searching coming out of ckb-indexer
/// `skip_exist`: bool, if true, skip the input cell if it already exists in skeleton, rather than return error
pub struct AddInputCell {
    pub lock_script: Script,
    pub type_script: Option<Script>,
    pub count: u32,
    pub search_mode: SearchMode,
    pub skip_exist: bool,
}

#[async_trait]
impl Operation for AddInputCell {
    fn search_key(&self) -> SearchKey {
        let mut query = CellQueryOptions::new_lock(self.lock_script.clone());
        if let Some(type_script) = &self.type_script {
            query.secondary_script = Some(type_script.clone());
        }
        query.with_data = Some(true);
        query.script_search_mode = Some(self.search_mode.clone());
        query.into()
    }

    async fn run<T: RPC>(self, rpc: &T, skeleton: &mut TransactionSkeleton) -> Result<()> {
        let mut iter = GetCellsIter::new(rpc, self.search_key());
        let mut find_avaliable = false;
        while let Some(cells) = iter.next_batch(self.count).await? {
            cells.into_iter().try_for_each(|cell| {
                let cell_input = CellInputEx::new_from_indexer_cell(cell);
                find_avaliable = true;
                if let Err(err) = skeleton.input(cell_input) {
                    if !self.skip_exist {
                        return Err(err);
                    }
                }
                Result::<()>::Ok(())
            })?;
        }
        if !find_avaliable {
            return Err(eyre!("input cell not found"));
        }
        Ok(())
    }
}

/// Operation that add input cell to transaction skeleton by user address
pub struct AddInputCellByAddress {
    pub address: Address,
}

#[async_trait]
impl Operation for AddInputCellByAddress {
    async fn run<T: RPC>(self, rpc: &T, skeleton: &mut TransactionSkeleton) -> Result<()> {
        skeleton
            .input_from_address(rpc, self.address.clone())
            .await?;
        Ok(())
    }
}

/// Operation that add input cell to transaction skeleton by type script
pub struct AddCellInputByType {
    pub type_script: Script,
    pub count: u32,
    pub search_mode: SearchMode,
    pub skip_exist: bool,
}

#[async_trait]
impl Operation for AddCellInputByType {
    fn search_key(&self) -> SearchKey {
        let mut query = CellQueryOptions::new_type(self.type_script.clone());
        query.script_search_mode = Some(self.search_mode.clone());
        query.with_data = Some(true);
        query.into()
    }

    async fn run<T: RPC>(self, rpc: &T, skeleton: &mut TransactionSkeleton) -> Result<()> {
        let mut iter = GetCellsIter::new(rpc, self.search_key());
        let mut find_avaliable = false;
        while let Some(cells) = iter.next_batch(self.count).await? {
            cells.into_iter().try_for_each(|cell| {
                let cell_input = CellInputEx::new_from_indexer_cell(cell);
                find_avaliable = true;
                if let Err(err) = skeleton.input(cell_input) {
                    if !self.skip_exist {
                        return Err(err);
                    }
                }
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
/// `mark_capacity_extra`: bool, if true, the capacity of output cell will be minimal capacity plus `capacity`
/// `user_type_id`: bool, if true, calculate type id and override into type script if provided
#[derive(Default)]
pub struct AddCellOutput {
    pub lock_script: Script,
    pub type_script: Option<Script>,
    pub capacity: u64,
    pub data: Vec<u8>,
    pub mark_capacity_extra: bool,
    pub use_type_id: bool,
}

#[async_trait]
impl Operation for AddCellOutput {
    async fn run<T: RPC>(self, _: &T, skeleton: &mut TransactionSkeleton) -> Result<()> {
        let type_script = if self.use_type_id {
            let type_id = skeleton.calc_type_id(skeleton.outputs.len())?;
            let type_script = self
                .type_script
                .map(|v| v.as_builder().args(type_id.as_bytes().pack()).build())
                .unwrap_or(
                    Script::new_builder()
                        .code_hash(TYPE_ID_CODE_HASH.pack())
                        .hash_type(ScriptHashType::Type.into())
                        .args(type_id.as_bytes().pack())
                        .build(),
                );
            Some(type_script)
        } else {
            self.type_script
        };
        let mut output = CellOutput::new_builder()
            .lock(self.lock_script)
            .type_(type_script.pack())
            .build();
        output = output
            .as_builder()
            .build_exact_capacity(Capacity::bytes(self.data.len())?)?;
        let minimal_capacity: u64 = output.capacity().unpack();
        if self.mark_capacity_extra {
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
}

#[async_trait]
impl Operation for AddOutputCellByAddress {
    async fn run<T: RPC>(self, _: &T, skeleton: &mut TransactionSkeleton) -> Result<()> {
        skeleton.output_from_address(self.address.clone());
        Ok(())
    }
}

/// Operation that add output cell to transaction skeleton by copying input cell from target position
///
/// `input_index`: usize, the index of input cell in inputs, if it is usize::MAX, copy the last one
#[derive(Default)]
pub struct AddOutputCellByInputIndex {
    pub input_index: usize,
    pub data: Option<Vec<u8>>,
    pub adjust_capacity: bool,
}

#[async_trait]
impl Operation for AddOutputCellByInputIndex {
    async fn run<T: RPC>(self, _: &T, skeleton: &mut TransactionSkeleton) -> Result<()> {
        let cell_input = if self.input_index != usize::MAX {
            skeleton
                .inputs
                .get(self.input_index)
                .ok_or(eyre!("input not found"))?
        } else {
            skeleton.inputs.last().ok_or(eyre!("input not found"))?
        };
        let mut cell_output = cell_input.output.clone();
        if let Some(data) = self.data {
            if self.adjust_capacity {
                cell_output.output = cell_output
                    .output
                    .as_builder()
                    .build_exact_capacity(Capacity::bytes(data.len())?)?;
            }
            cell_output.data = data;
        }
        skeleton.output(cell_output);
        Ok(())
    }
}

/// Operation that add wintess in form of WitnessArgs to transaction skeleton
pub struct AddWitnessArgs {
    pub lock: Vec<u8>,
    pub input_type: Vec<u8>,
    pub output_type: Vec<u8>,
}

#[async_trait]
impl Operation for AddWitnessArgs {
    async fn run<T: RPC>(self, _: &T, skeleton: &mut TransactionSkeleton) -> Result<()> {
        let witness = WitnessArgsEx::new(self.lock, self.input_type, self.output_type);
        skeleton.witness(witness);
        Ok(())
    }
}

/// Operation that sign and add secp256k1_sighash_all signatures to transaction skeleton
pub struct AddSecp256k1SighashSignatures {
    pub user_lock_scripts: Vec<Script>,
    pub user_private_keys: Vec<SecretKey>,
}

#[async_trait]
impl Operation for AddSecp256k1SighashSignatures {
    async fn run<T: RPC>(self, _: &T, skeleton: &mut TransactionSkeleton) -> Result<()> {
        let tx = skeleton.clone().into_transaction_view();
        let mut tx_groups_builder = TransactionWithScriptGroupsBuilder::default().set_tx_view(tx);
        for lock_script in self.user_lock_scripts {
            let (input_indices, _) = skeleton.lock_script_groups(&lock_script);
            tx_groups_builder =
                tx_groups_builder.add_lock_script_group(&lock_script, &input_indices);
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

/// Operation that balance transaction skeleton
pub struct BalanceTransaction {
    pub balancer: Script,
    pub change_receiver: ChangeReceiver,
    pub additinal_fee_rate: u64,
}

#[async_trait]
impl Operation for BalanceTransaction {
    async fn run<T: RPC>(self, rpc: &T, skeleton: &mut TransactionSkeleton) -> Result<()> {
        let fee = skeleton.fee(rpc, self.additinal_fee_rate).await?;
        skeleton
            .balance(rpc, fee, self.balancer, self.change_receiver)
            .await?;
        Ok(())
    }
}
