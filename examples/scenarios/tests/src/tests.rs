//! CKB-VM tests for the scenario locks. Run `make build` first.
//!
//! Live cells are seeded here. The calculator recipes only collect those
//! cells by out point and describe the spend.

use std::{fs, path::Path};

use ckb_cinnabar_calculator::{
    assert_verify,
    instruction::Instruction,
    re_exports::{
        ckb_hash::blake2b_256,
        ckb_types::{prelude::Unpack, H256},
    },
    scenario::absolute_timestamp_since,
    simulation::{fake_outpoint, AddFakeContractCelldepByName, FakeRpcClient},
    skeleton::{CellOutputEx, ScriptEx},
    types::{h256_to_hash, Hash256},
};
use scenarios_calculator::{spend_threshold, spend_timelock, TimelockInput};

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

/// Insert one live cell locked by `lock` and return the out point the recipe
/// will collect.
fn seed_cell(rpc: &mut FakeRpcClient, lock: &ScriptEx, capacity: u64) -> (Hash256, u32) {
    let out_point = fake_outpoint();
    let cell = CellOutputEx::new_from_scripts(
        lock.clone().to_script_unchecked(),
        None,
        Vec::new(),
        Some(capacity),
    )
    .expect("fake input cell");
    let tx_hash: H256 = out_point.tx_hash().unpack();
    let index: u32 = out_point.index().unpack();
    rpc.insert_fake_cell(out_point, cell, None);
    (h256_to_hash(&tx_hash), index)
}

fn timelock_spend(lock: ScriptEx, cell: (Hash256, u32), since: u64) -> Instruction<FakeRpcClient> {
    let (tx_hash, index) = cell;
    spend_timelock(
        lock,
        vec![TimelockInput {
            tx_hash,
            index,
            since,
        }],
        OUT,
    )
}

#[tokio::test]
async fn timelock_allows_absolute_since_at_unlock() {
    let unlock_at = 1_700_000_000u64;
    let lock = script("timelock", unlock_at.to_le_bytes().to_vec());
    let mut rpc = FakeRpcClient::default();
    let cell = seed_cell(&mut rpc, &lock, IN);
    let spend = timelock_spend(lock, cell, absolute_timestamp_since(unlock_at));
    assert_verify!(&rpc, vec![prepare("timelock"), spend], 0).unwrap();
}

#[tokio::test]
async fn timelock_rejects_early_since() {
    let unlock_at = 1_700_000_000u64;
    let lock = script("timelock", unlock_at.to_le_bytes().to_vec());
    let mut rpc = FakeRpcClient::default();
    let cell = seed_cell(&mut rpc, &lock, IN);
    let spend = timelock_spend(lock, cell, absolute_timestamp_since(1_600_000_000));
    assert_verify!(&rpc, vec![prepare("timelock"), spend], 21).unwrap();
}

#[tokio::test]
async fn timelock_rejects_short_args() {
    let lock = script("timelock", vec![1, 2, 3]);
    let mut rpc = FakeRpcClient::default();
    let cell = seed_cell(&mut rpc, &lock, IN);
    let spend = timelock_spend(lock, cell, absolute_timestamp_since(1));
    assert_verify!(&rpc, vec![prepare("timelock"), spend], 20).unwrap();
}

/// `SinceNotMet` is the variant after `BadArgs = CUSTOM_ERROR_START` (20), so
/// the on-chain exit code is 21. A valid `since` on the first group input
/// must not let a second same-lock input through with `since` 0.
#[tokio::test]
async fn timelock_rejects_second_input_with_unrestricted_since() {
    let unlock_at = 1_700_000_000u64;
    let lock = script("timelock", unlock_at.to_le_bytes().to_vec());
    let mut rpc = FakeRpcClient::default();
    let (first_hash, first_index) = seed_cell(&mut rpc, &lock, IN);
    let (second_hash, second_index) = seed_cell(&mut rpc, &lock, IN);
    let spend = spend_timelock(
        lock,
        vec![
            TimelockInput {
                tx_hash: first_hash,
                index: first_index,
                since: absolute_timestamp_since(unlock_at),
            },
            TimelockInput {
                tx_hash: second_hash,
                index: second_index,
                since: 0,
            },
        ],
        OUT,
    );
    assert_verify!(&rpc, vec![prepare("timelock"), spend], 21).unwrap();
}

#[tokio::test]
async fn threshold_accepts_one_matching_preimage() {
    let preimage = [0xABu8; 32];
    let hash = blake2b_256(&preimage);
    let mut args = vec![1, 1];
    args.extend_from_slice(&hash);
    let lock = script("threshold_multisig", args);
    let mut rpc = FakeRpcClient::default();
    let (tx_hash, index) = seed_cell(&mut rpc, &lock, IN);
    let spend = spend_threshold(lock, tx_hash, index, preimage.to_vec(), OUT);
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
    let mut rpc = FakeRpcClient::default();
    let (tx_hash, index) = seed_cell(&mut rpc, &lock, IN);
    let spend = spend_threshold(lock, tx_hash, index, first.to_vec(), OUT);
    assert_verify!(&rpc, vec![prepare("threshold_multisig"), spend], 22).unwrap();
}

#[tokio::test]
async fn threshold_rejects_empty_args() {
    let lock = script("threshold_multisig", vec![]);
    let mut rpc = FakeRpcClient::default();
    let (tx_hash, index) = seed_cell(&mut rpc, &lock, IN);
    let spend = spend_threshold(lock, tx_hash, index, vec![0; 32], OUT);
    assert_verify!(&rpc, vec![prepare("threshold_multisig"), spend], 20).unwrap();
}
