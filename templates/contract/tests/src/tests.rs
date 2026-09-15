//! Local simulation tests: [`FakeRpcClient`] + [`assert_verify!`].
//!
//! `0` is CKB-VM success. Non-zero matches an on-chain `define_errors!` `i8`.
//! Run `make build` first so `../build/release/{{crate_name}}` exists.

use std::{fs, path::Path};

use ckb_cinnabar_calculator::{
    assert_verify,
    instruction::Instruction,
    operation::basic::AddOutputCell,
    re_exports::{ckb_hash::blake2b_256, ckb_types::core::Capacity},
    rpc::Network,
    simulation::{
        always_success_script, AddFakeAlwaysSuccessCelldep, AddFakeContractCelldepByName,
        fake_outpoint, AddFakeInputCell, FakeRpcClient,
    },
    skeleton::{CellOutputEx, ScriptEx},
};
use {{crate_name}}_calculator::transfer;

const CONTRACT: &str = "{{crate_name}}";
const BINARY_PATH: &str = "../build/release";

/// Smoke test: always-success lock verifies without the generated contract.
#[tokio::test]
async fn always_success_simulates() {
    let rpc = FakeRpcClient::default();
    let lock = always_success_script(vec![]);
    let prepare = Instruction::new(vec![
        Box::new(AddFakeAlwaysSuccessCelldep {}),
        Box::new(AddFakeInputCell {
            lock_script: lock.clone().into(),
            type_script: None,
            data: vec![],
            capacity: 200_000_000_000,
            absolute_capacity: true,
        }),
        Box::new(AddOutputCell {
            lock_script: lock.into(),
            type_script: None,
            data: vec![],
            capacity: 100_000_000_000,
            absolute_capacity: true,
            type_id: false,
        }),
    ]);
    assert_verify!(&rpc, vec![prepare], 0).expect("always-success should verify");
}

/// Load the RISC-V binary, seed a fake input locked by this contract, then
/// run the calculator `transfer` instruction through the native CKB-VM.
#[tokio::test]
async fn generated_contract_verifies_when_built() {
    let bin = format!("{BINARY_PATH}/{CONTRACT}");
    assert!(
        Path::new(&bin).exists(),
        "contract binary missing; run `make build` first ({bin})"
    );
    let contract_binary = fs::read(&bin).expect("read generated contract");
    let contract_script = ScriptEx::new_code(blake2b_256(&contract_binary).into(), vec![]);
    let contract_address = contract_script
        .to_address(Network::Fake)
        .expect("contract address");
    let input_cell = CellOutputEx::new_from_scripts(
        contract_address.payload().into(),
        None,
        vec![],
        Some(Capacity::shannons(200_000_000_000)),
    )
    .expect("fake input cell");

    let mut rpc = FakeRpcClient::default();
    rpc.insert_fake_cell(fake_outpoint(), input_cell, None);
    let prepare = Instruction::new(vec![Box::new(AddFakeContractCelldepByName {
        contract: CONTRACT.to_string(),
        type_id_args: None,
        contract_binary_path: BINARY_PATH.to_string(),
    })]);
    let transfer = transfer::<FakeRpcClient>(
        contract_address.clone(),
        contract_address,
        100_000_000_000,
    );

    assert_verify!(&rpc, vec![prepare, transfer], 0).expect("generated transfer should verify");
}
