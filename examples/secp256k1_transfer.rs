use ckb_cinnabar_calculator::{
    instruction::{
        predefined::{balance_and_sign_with_ckb_cli, secp256k1_sighash_transfer},
        TransactionCalculator,
    },
    re_exports::{
        ckb_sdk::{Address, HumanCapacity},
        secp256k1::SecretKey,
        tokio,
    },
    rpc::RpcClient,
};

const ADDITIONAL_FEE_RATE: u64 = 1000;

/// Transfer CKB from one address to another address
///
/// Usage: cargo run --example secp256k1_transfer <from> <to> <ckb> [secret_key]
#[tokio::main]
pub async fn main() {
    let mut args = std::env::args();
    args.next(); // skip program name
    let (from, to, ckb, secret_key) = match (args.next(), args.next(), args.next(), args.next()) {
        (Some(from), Some(to), Some(ckb), secret_key) => (from, to, ckb, secret_key),
        _ => panic!("args invalid"),
    };

    // prepare transfer parameters
    let from: Address = from.parse().expect("from address");
    let to: Address = to.parse().expect("to address");
    let ckb: HumanCapacity = ckb.parse().expect("ckb");
    let secret_key: Option<SecretKey> = secret_key.map(|k| k.parse().expect("secret_key"));
    let rpc = RpcClient::new_testnet();

    // build transfer instruction
    let mut calculator = TransactionCalculator::default();
    if let Some(secret_key) = secret_key {
        let signed_transfer =
            secp256k1_sighash_transfer(&from, &to, ckb, ADDITIONAL_FEE_RATE, Some(secret_key));
        calculator.instruction(signed_transfer);
    } else {
        let unsigned_transfer =
            secp256k1_sighash_transfer(&from, &to, ckb, ADDITIONAL_FEE_RATE, None);
        let balance_and_sign = balance_and_sign_with_ckb_cli(&from, ADDITIONAL_FEE_RATE, None);
        calculator
            .instruction(unsigned_transfer)
            .instruction(balance_and_sign);
    };

    // apply transfer instructio and build transaction
    let skeleton = calculator.new_skeleton(&rpc).await.expect("calculate");

    // send transaction without any block confirmations
    let hash = skeleton.send_and_wait(&rpc, 0, None).await.expect("send");
    println!("Transaction hash: {hash:#x}");
}
