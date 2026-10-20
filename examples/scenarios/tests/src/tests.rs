//! CKB-VM tests for the scenario locks. Run `make build` first.

use std::{fs, path::Path};

use ckb_cinnabar_calculator::{
    assert_verify,
    instruction::Instruction,
    re_exports::ckb_hash::blake2b_256,
    scenario::absolute_timestamp_since,
    simulation::{AddFakeContractCelldepByName, FakeRpcClient},
    skeleton::ScriptEx,
};
use scenarios_calculator::{spend_threshold, spend_timelock};

const IN: u64 = 200_000_000_000;
const OUT: u64 = 100_000_000_000;

fn script(name: &str, args: Vec<u8>) -> ScriptEx {
    let path = format!("../build/release/{name}");
    assert!(Path::new(&path).exists(), "missing {path}; run make build");
    let binary = fs::read(&path).expect("binary");
    ScriptEx::new_code(blake2b_256(&binary).into(), args)
}

fn prepare(name: &str) -> Instruction<FakeRpcClient> {
    Instruction::new(vec![Box::new(AddFakeContractCelldepByName {
        contract: name.into(),
        type_id_args: None,
        contract_binary_path: "../build/release".into(),
    })])
}

#[tokio::test]
async fn timelock_allows_absolute_since_at_unlock() {
    let unlock_at = 1_700_000_000u64;
    let lock = script("timelock", unlock_at.to_le_bytes().to_vec());
    let rpc = FakeRpcClient::default();
    let spend = spend_timelock(lock, absolute_timestamp_since(unlock_at), IN, OUT);
    assert_verify!(&rpc, vec![prepare("timelock"), spend], 0).unwrap();
}

#[tokio::test]
async fn timelock_rejects_early_since() {
    let unlock_at = 1_700_000_000u64;
    let lock = script("timelock", unlock_at.to_le_bytes().to_vec());
    let rpc = FakeRpcClient::default();
    let spend = spend_timelock(lock, absolute_timestamp_since(1_600_000_000), IN, OUT);
    assert_verify!(&rpc, vec![prepare("timelock"), spend], 21).unwrap();
}

#[tokio::test]
async fn timelock_rejects_short_args() {
    let lock = script("timelock", vec![1, 2, 3]);
    let rpc = FakeRpcClient::default();
    let spend = spend_timelock(lock, absolute_timestamp_since(1), IN, OUT);
    assert_verify!(&rpc, vec![prepare("timelock"), spend], 20).unwrap();
}

#[tokio::test]
async fn threshold_accepts_one_matching_preimage() {
    let preimage = [0xABu8; 32];
    let hash = blake2b_256(&preimage);
    let mut args = vec![1, 1];
    args.extend_from_slice(&hash);
    let lock = script("threshold_multisig", args);
    let rpc = FakeRpcClient::default();
    let spend = spend_threshold(lock, preimage.to_vec(), IN, OUT);
    assert_verify!(&rpc, vec![prepare("threshold_multisig"), spend], 0).unwrap();
}

#[tokio::test]
async fn threshold_rejects_when_short() {
    let first = [0xABu8; 32];
    let second = [0xCDu8; 32];
    let mut args = vec![2, 2];
    args.extend_from_slice(&blake2b_256(first));
    args.extend_from_slice(&blake2b_256(second));
    let lock = script("threshold_multisig", args);
    let rpc = FakeRpcClient::default();
    let spend = spend_threshold(lock, first.to_vec(), IN, OUT);
    assert_verify!(&rpc, vec![prepare("threshold_multisig"), spend], 22).unwrap();
}

#[tokio::test]
async fn threshold_rejects_empty_args() {
    let lock = script("threshold_multisig", vec![]);
    let rpc = FakeRpcClient::default();
    let spend = spend_threshold(lock, vec![0; 32], IN, OUT);
    assert_verify!(&rpc, vec![prepare("threshold_multisig"), spend], 20).unwrap();
}
