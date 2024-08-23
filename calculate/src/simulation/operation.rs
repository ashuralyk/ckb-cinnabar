use std::{fs, path::PathBuf};

use async_trait::async_trait;
use ckb_sdk::constants::TYPE_ID_CODE_HASH;
use ckb_types::{
    core::{Capacity, DepType},
    packed::CellOutput,
    prelude::{Builder, Entity, Pack},
};
use eyre::Result;

use crate::{
    operation::{Log, Operation},
    rpc::RPC,
    skeleton::{CellDepEx, CellInputEx, ScriptEx, TransactionSkeleton},
};

pub use ckb_always_success_script::ALWAYS_SUCCESS;
use ckb_types::{
    core::ScriptHashType,
    packed::{CellDep, CellInput, OutPoint, Script},
};
use rand::Rng;

pub fn random_hash() -> [u8; 32] {
    let mut rng = rand::thread_rng();
    let mut buf = [0u8; 32];
    rng.fill(&mut buf);
    buf
}

pub fn fake_outpoint() -> OutPoint {
    OutPoint::new(random_hash().pack(), 0)
}

pub fn fake_input() -> CellInput {
    CellInput::new(fake_outpoint(), 0)
}

pub const ALWAYS_SUCCESS_NAME: &str = "always_success";

/// Add a custom contract celldep to the transaction skeleton
pub struct AddFakeContractCelldep {
    pub name: String,
    pub contract_data: Vec<u8>,
    pub with_type_id: bool,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddFakeContractCelldep {
    async fn run(
        self: Box<Self>,
        _: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let celldep_out_point = fake_outpoint();
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
        skeleton.celldep(CellDepEx::new(
            self.name,
            celldep,
            output.build(),
            Some(self.contract_data),
        ));
        Ok(())
    }
}

/// Add a custom contract celldep to the transaction skeleton by loading compiled native contract
pub struct AddFakeContractCelldepByName {
    pub contract: String,
    pub with_type_id: bool,
    pub contract_binary_path: String,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddFakeContractCelldepByName {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let contract_path = PathBuf::new()
            .join(self.contract_binary_path)
            .join(&self.contract);
        let contract_data = fs::read(contract_path)?;
        Box::new(AddFakeContractCelldep {
            name: self.contract,
            contract_data,
            with_type_id: self.with_type_id,
        })
        .run(rpc, skeleton, log)
        .await
    }
}

/// Add always success celldep to the transaction skeleton
pub struct AddAlwaysSuccessCelldep {}

#[async_trait]
impl<T: RPC> Operation<T> for AddAlwaysSuccessCelldep {
    async fn run(
        self: Box<Self>,
        _: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let always_success_out_point = fake_outpoint();
        let celldep = CellDep::new_builder()
            .out_point(always_success_out_point)
            .dep_type(DepType::Code.into())
            .build();
        skeleton.celldep(CellDepEx::new(
            ALWAYS_SUCCESS_NAME.to_string(),
            celldep,
            CellOutput::default(),
            Some(ALWAYS_SUCCESS.to_vec()),
        ));
        Ok(())
    }
}

/// Add a custom cell input to the transaction skeleton, which has primary and second scripts
pub struct AddFakeCellInput {
    pub lock_script: ScriptEx,
    pub type_script: Option<ScriptEx>,
    pub data: Vec<u8>,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddFakeCellInput {
    async fn run(
        self: Box<Self>,
        _: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let primary_script = self.lock_script.to_script(skeleton)?;
        let second_script = if let Some(second) = self.type_script {
            Some(second.to_script(skeleton)?)
        } else {
            None
        };
        let output = CellOutput::new_builder()
            .lock(primary_script)
            .type_(second_script.pack())
            .build_exact_capacity(Capacity::bytes(self.data.len())?)?;
        let custom_out_point = fake_outpoint();
        let input = CellInput::new_builder()
            .previous_output(custom_out_point)
            .build();
        skeleton.input(CellInputEx::new(input, output, Some(self.data)))?;
        Ok(())
    }
}
