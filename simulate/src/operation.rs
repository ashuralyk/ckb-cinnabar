use std::{fs, path::PathBuf};

use ckb_always_success_script::ALWAYS_SUCCESS;
use ckb_cinnabar_calculator::{
    operation::Operation,
    re_exports::{
        async_trait::async_trait,
        ckb_sdk::constants::TYPE_ID_CODE_HASH,
        ckb_types::{
            core::{Capacity, DepType, ScriptHashType},
            packed::{CellDep, CellInput, CellOutput, OutPoint, Script},
            prelude::{Builder, Entity, Pack},
        },
        eyre::{eyre, Result},
    },
    rpc::RPC,
    skeleton::{CellDepEx, CellInputEx, TransactionSkeleton},
};
use rand::{thread_rng, Rng};

fn random_hash() -> [u8; 32] {
    let mut rng = thread_rng();
    let mut buf = [0u8; 32];
    rng.fill(&mut buf);
    buf
}

/// Add a custom contract celldep to the transaction skeleton
pub struct AddCustomContractCelldep {
    pub contract_data: Vec<u8>,
    pub with_type_id: bool,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddCustomContractCelldep {
    async fn run(self: Box<Self>, _: &T, skeleton: &mut TransactionSkeleton) -> Result<()> {
        let celldep_out_point = OutPoint::new(random_hash().pack(), 0);
        let celldep = CellDep::new_builder()
            .out_point(celldep_out_point)
            .dep_type(DepType::Code.into())
            .build();
        let mut output = CellOutput::new_builder();
        if self.with_type_id {
            let args = random_hash();
            let type_script = Script::new_builder()
                .code_hash(TYPE_ID_CODE_HASH.pack())
                .hash_type(ScriptHashType::Type.into())
                .args(args.to_vec().pack())
                .build();
            output = output.type_(Some(type_script).pack());
        }
        skeleton.celldep(CellDepEx::new(celldep, output.build(), self.contract_data));
        Ok(())
    }
}

pub struct AddCustomContractCelldepByName {
    pub contract: &'static str,
    pub with_type_id: bool,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddCustomContractCelldepByName {
    async fn run(self: Box<Self>, rpc: &T, skeleton: &mut TransactionSkeleton) -> Result<()> {
        let mut contract_path = PathBuf::new();
        contract_path.push("../build/release");
        contract_path.push(self.contract);
        let contract_data = fs::read(contract_path)?;
        Box::new(AddCustomContractCelldep {
            contract_data,
            with_type_id: self.with_type_id,
        })
        .run(rpc, skeleton)
        .await
    }
}

/// Add always success celldep to the transaction skeleton
pub struct AddAlwaysSuccessCelldep {}

#[async_trait]
impl<T: RPC> Operation<T> for AddAlwaysSuccessCelldep {
    async fn run(self: Box<Self>, _: &T, skeleton: &mut TransactionSkeleton) -> Result<()> {
        let always_success_out_point = OutPoint::new(random_hash().pack(), 0);
        let celldep = CellDep::new_builder()
            .out_point(always_success_out_point)
            .dep_type(DepType::Code.into())
            .build();
        skeleton.celldep(CellDepEx::new(
            celldep,
            CellOutput::default(),
            ALWAYS_SUCCESS.to_vec(),
        ));
        Ok(())
    }
}

pub struct ReferenceScript {
    pub celldep_index: usize,
    pub args: Vec<u8>,
}

impl From<(usize, Vec<u8>)> for ReferenceScript {
    fn from((celldep_index, args): (usize, Vec<u8>)) -> Self {
        ReferenceScript {
            celldep_index,
            args,
        }
    }
}

/// Add a custom cell input to the transaction skeleton, which has primary and second scripts
pub struct AddCustomCellInput {
    pub lock_script: ReferenceScript,
    pub type_script: Option<ReferenceScript>,
    pub data: Vec<u8>,
}

impl AddCustomCellInput {
    fn build_script_from_celldep(
        &self,
        script: &ReferenceScript,
        skeleton: &TransactionSkeleton,
    ) -> Result<Script> {
        let celldep = skeleton
            .celldeps
            .get(script.celldep_index)
            .ok_or(eyre!("celldep index out of range"))?;
        let output = celldep.output.clone().expect("binary celldep");
        let mut script = Script::new_builder().args(script.args.pack());
        if let Some(celldep_type_hash) = output.calc_type_hash() {
            script = script
                .code_hash(celldep_type_hash.pack())
                .hash_type(ScriptHashType::Type.into());
        } else {
            script = script
                .code_hash(output.data_hash().pack())
                .hash_type(ScriptHashType::Data2.into());
        }
        Ok(script.build())
    }
}

#[async_trait]
impl<T: RPC> Operation<T> for AddCustomCellInput {
    async fn run(self: Box<Self>, _: &T, skeleton: &mut TransactionSkeleton) -> Result<()> {
        let primary_script = self.build_script_from_celldep(&self.lock_script, skeleton)?;
        let second_script = if let Some(ref second) = self.type_script {
            Some(self.build_script_from_celldep(second, skeleton)?)
        } else {
            None
        };
        let output = CellOutput::new_builder()
            .lock(primary_script)
            .type_(second_script.pack())
            .build_exact_capacity(Capacity::bytes(self.data.len())?)?;
        let custom_out_point = OutPoint::new(random_hash().pack(), 0);
        let input = CellInput::new_builder()
            .previous_output(custom_out_point)
            .build();
        skeleton.input(CellInputEx::new(input, output, self.data))?;
        Ok(())
    }
}
