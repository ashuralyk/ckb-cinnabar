//! Encode the genesis secp256k1 multisig lock (no node, no secp verify).
//!
//! The on-chain secp256k1 multisig script is the one deployed in the CKB
//! genesis cell, not a script this repository reimplements. What wallets
//! put in the lock args is blake160 of the witness prefix built here.
//! A separate preimage threshold lock, with CKB-VM tests, lives in
//! `examples/scenarios`.
//!
//! ```text
//! cargo run --example multisig
//! ```

use ckb_cinnabar_calculator::scenario::{encode_multisig_script, multisig_lock_args};

fn main() {
    let keys = [[0x11; 20], [0x22; 20], [0x33; 20]];
    let script = encode_multisig_script(1, 2, &keys).expect("script");
    let args = multisig_lock_args(1, 2, &keys).expect("args");
    println!("witness prefix ({} bytes): {}", script.len(), hex(&script));
    println!("lock args (blake160): {}", hex(&args));
    println!("require_first_n=1 threshold=2 pubkeys=3");
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
