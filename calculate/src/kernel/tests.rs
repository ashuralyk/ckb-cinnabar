//! Kernel hard-accept: encodings plus inject → operate → pack.

extern crate alloc;

use alloc::{boxed::Box, string::ToString, vec, vec::Vec};

use core::str::FromStr;

use super::{
    address::{Address, AddressPayload},
    network::Network,
};

use crate::{
    instruction::Instruction,
    operation::{
        basic::{AddOutputCell, AddWitnessArgs},
        layout::{
            decode_xudt_amount, encode_cobuild_action, encode_xudt_amount, make_cluster_data,
            make_spore_data,
        },
        Log,
    },
    skeleton::{CellInputEx, CellOutputEx, ScriptEx, TransactionSkeleton, WitnessEx},
    source::UnsupportedSource,
    types::{occupied_capacity_shannons, pack_hash, packed, Builder, Entity, Pack, ScriptHashType},
};

#[test]
fn address_round_trips_full_format() {
    let payload = AddressPayload::new_full(ScriptHashType::Data1, [0x11; 32], vec![0x01, 0x02]);
    let address = Address::new(Network::Mainnet, payload.clone());
    let text = address.to_string();
    assert!(text.starts_with("ckb1"));

    let parsed = Address::from_str(&text).unwrap();
    assert_eq!(parsed, address);
    assert_eq!(AddressPayload::from(packed::Script::from(&parsed)), payload);

    let script_ex = ScriptEx::from(&payload);
    let via_script = script_ex.to_address(Network::Testnet).unwrap();
    assert!(via_script.to_string().starts_with("ckt1"));
    assert_eq!(via_script.payload(), &payload);

    let fake = Address::new(Network::Fake, payload);
    let decoded = Address::from_str(&fake.to_string()).unwrap();
    assert_eq!(decoded.network(), &Network::Testnet);

    assert!(Address::from_str("ckb1qqqq").is_err());
    assert!(Address::from_str("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4").is_err());
    assert!(ScriptEx::Reference("named".into(), vec![1])
        .to_address(Network::Mainnet)
        .is_err());
}

fn dummy_script(args: Vec<u8>) -> packed::Script {
    packed::Script::new_builder()
        .code_hash(pack_hash(&[0x11u8; 32]))
        .hash_type(ScriptHashType::Data1)
        .args(args.pack())
        .build()
}

fn dummy_output(capacity: u64, data: Vec<u8>) -> (packed::CellOutput, Vec<u8>) {
    let draft = packed::CellOutput::new_builder()
        .lock(dummy_script(Vec::new()))
        .build();
    let cap = capacity.max(occupied_capacity_shannons(&draft, data.len()));
    (draft.as_builder().capacity(cap).build(), data)
}

fn dummy_input() -> CellInputEx {
    let out_point = packed::OutPoint::new_builder()
        .tx_hash(pack_hash(&[0x22u8; 32]))
        .index(0u32)
        .build();
    let input = packed::CellInput::new_builder()
        .previous_output(out_point)
        .since(0u64)
        .build();
    let (output, data) = dummy_output(200_0000_0000, Vec::new());
    CellInputEx::new(input, output, Some(data))
}

#[test]
fn calc_type_id_matches_blake2b_formula() {
    let mut skeleton = TransactionSkeleton::default();
    skeleton.input(dummy_input()).unwrap();
    let type_id = skeleton.calc_type_id(0).unwrap();

    let mut hasher = ckb_hash::Blake2bBuilder::new(32)
        .personal(b"ckb-default-hash")
        .build();
    hasher.update(skeleton.inputs[0].input.as_slice());
    hasher.update(&0u64.to_le_bytes());
    let mut expected = [0u8; 32];
    hasher.finalize(&mut expected);
    assert_eq!(type_id, expected);

    let empty = TransactionSkeleton::default();
    assert!(empty.calc_type_id(0).is_err());
}

#[test]
fn capacity_arithmetic_saturates() {
    let mut skeleton = TransactionSkeleton::default();
    skeleton.input(dummy_input()).unwrap();
    let (output, data) = dummy_output(50_0000_0000, Vec::new());
    skeleton.output(CellOutputEx::new(output, data));
    assert!(skeleton.total_inputs_capacity() > skeleton.total_outputs_capacity());
    assert_eq!(skeleton.needed_capacity(), 0);
    assert!(skeleton.exceeded_capacity() > 0);
}

#[test]
fn xudt_amount_is_little_endian_u128() {
    let amount = 1_000_000u128;
    let bytes = encode_xudt_amount(amount);
    assert_eq!(bytes.len(), 16);
    assert_eq!(bytes[0], 0x40);
    assert_eq!(bytes[1], 0x42);
    assert_eq!(bytes[2], 0x0f);
    assert_eq!(decode_xudt_amount(&bytes), amount);
    assert_eq!(decode_xudt_amount(&[]), 0);
}

#[test]
fn spore_and_cluster_encode_are_deterministic() {
    let a = make_spore_data("text/plain", b"hi", None);
    let b = make_spore_data("text/plain", b"hi", None);
    let c = make_spore_data("text/plain", b"ho", None);
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert!(a.len() >= 16);

    let cluster = make_cluster_data("alpha", b"desc");
    assert_eq!(cluster, make_cluster_data("alpha", b"desc"));
    assert_ne!(cluster, make_cluster_data("beta", b"desc"));

    let action = encode_cobuild_action(&[1u8; 32], &[2u8; 32], b"payload");
    assert_eq!(
        action,
        encode_cobuild_action(&[1u8; 32], &[2u8; 32], b"payload")
    );
}

#[test]
fn inject_operate_and_pack() {
    let mut skeleton = TransactionSkeleton::default();
    skeleton.input(dummy_input()).unwrap();

    let lock = ScriptEx::new_code([0x33u8; 32], vec![0x01]);
    let ops = Instruction::new(vec![
        Box::new(AddOutputCell {
            lock_script: lock,
            type_script: None,
            capacity: 0,
            data: b"hello".to_vec(),
            absolute_capacity: false,
            type_id: true,
        }),
        Box::new(AddWitnessArgs {
            witness_index: None,
            lock: vec![0xaa; 65],
            input_type: Vec::new(),
            output_type: Vec::new(),
        }),
    ]);
    let mut log = Log::new();
    ops.run(&UnsupportedSource, &mut skeleton, &mut log)
        .unwrap();

    assert_eq!(skeleton.outputs.len(), 1);
    assert_eq!(skeleton.witnesses.len(), 1);
    let packed = skeleton.into_packed_transaction();
    assert_eq!(packed.raw().inputs().len(), 1);
    assert_eq!(packed.raw().outputs().len(), 1);
    assert_eq!(packed.raw().outputs_data().len(), 1);
    assert!(!packed.as_slice().is_empty());
}

#[test]
fn from_packed_transaction_round_trips_fields() {
    let mut skeleton = TransactionSkeleton::default();
    skeleton.input(dummy_input()).unwrap();
    let (output, data) = dummy_output(50_0000_0000, b"hello".to_vec());
    skeleton.output(CellOutputEx::new(output, data));
    skeleton.witness(WitnessEx::new(vec![0xaa; 65], Vec::new(), Vec::new()));
    let packed = skeleton.into_packed_transaction();

    let restored = TransactionSkeleton::from_packed_transaction(&packed);
    assert_eq!(restored.inputs.len(), 1);
    assert_eq!(restored.outputs.len(), 1);
    assert_eq!(restored.outputs[0].data, b"hello");
    assert_eq!(restored.witnesses.len(), 1);
    assert_eq!(restored.witnesses[0].lock, vec![0xaa; 65]);

    let packed2 = restored.into_packed_transaction();
    assert_eq!(
        packed.raw().inputs().as_slice(),
        packed2.raw().inputs().as_slice()
    );
    assert_eq!(
        packed.raw().outputs().as_slice(),
        packed2.raw().outputs().as_slice()
    );
    assert_eq!(
        packed.raw().outputs_data().as_slice(),
        packed2.raw().outputs_data().as_slice()
    );
    assert_eq!(
        packed.witnesses().as_slice(),
        packed2.witnesses().as_slice()
    );
}
