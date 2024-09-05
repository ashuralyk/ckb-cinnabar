use std::{fs, path::PathBuf};

use async_trait::async_trait;
use ckb_hash::blake2b_256;
use ckb_sdk::constants::TYPE_ID_CODE_HASH;
use ckb_types::{
    core::{Capacity, DepType, HeaderView},
    packed::{CellOutput, Header, RawHeader},
    prelude::{Builder, Entity, IntoHeaderView, Pack, Unpack},
    H256,
};
use eyre::Result;

use crate::{
    operation::{Log, Operation},
    rpc::{Network, RPC},
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

pub fn always_success_script(args: Vec<u8>) -> Script {
    Script::new_builder()
        .code_hash(blake2b_256(ALWAYS_SUCCESS).pack())
        .hash_type(ScriptHashType::Data1.into())
        .args(args.pack())
        .build()
}

pub fn fake_header_view(block_number: u64, timestamp: u64, epoch: u64) -> HeaderView {
    let header = RawHeader::new_builder()
        .number(block_number.pack())
        .timestamp(timestamp.pack())
        .epoch(epoch.pack())
        .build();
    Header::new_builder().raw(header).build().into_view()
}

pub const ALWAYS_SUCCESS_NAME: &str = "always_success";

/// Add a custom contract celldep to the transaction skeleton
pub struct AddFakeContractCelldep {
    pub name: String,
    pub contract_data: Vec<u8>,
    pub type_id_args: Option<H256>,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddFakeContractCelldep {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        if rpc.network() != Network::Fake {
            return Err(eyre::eyre!("only support fake network"));
        }
        let celldep_out_point = fake_outpoint();
        let celldep = CellDep::new_builder()
            .out_point(celldep_out_point)
            .dep_type(DepType::Code.into())
            .build();
        let mut output = CellOutput::new_builder();
        if let Some(args) = self.type_id_args {
            let type_script = Script::new_builder()
                .code_hash(TYPE_ID_CODE_HASH.pack())
                .hash_type(ScriptHashType::Type.into())
                .args(args.as_bytes().pack())
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
    pub type_id_args: Option<H256>,
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
            type_id_args: self.type_id_args,
        })
        .run(rpc, skeleton, log)
        .await
    }
}

/// Add always success celldep to the transaction skeleton
pub struct AddFakeAlwaysSuccessCelldep {}

#[async_trait]
impl<T: RPC> Operation<T> for AddFakeAlwaysSuccessCelldep {
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
pub struct AddFakeInputCell {
    pub lock_script: ScriptEx,
    pub type_script: Option<ScriptEx>,
    pub data: Vec<u8>,
    pub capacity: u64,
    pub absolute_capacity: bool,
}

#[async_trait]
impl<T: RPC> Operation<T> for AddFakeInputCell {
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
        let output = if self.absolute_capacity {
            CellOutput::new_builder()
                .lock(primary_script)
                .type_(second_script.pack())
                .capacity(self.capacity.pack())
                .build()
        } else {
            let output = CellOutput::new_builder()
                .lock(primary_script)
                .type_(second_script.pack())
                .build_exact_capacity(Capacity::bytes(self.data.len())?)?;
            let minimal_capacity: u64 = output.capacity().unpack();
            output
                .as_builder()
                .capacity((minimal_capacity + self.capacity).pack())
                .build()
        };
        skeleton
            .input(CellInputEx::new(fake_input(), output, Some(self.data)))?
            .witness(Default::default());
        Ok(())
    }
}
