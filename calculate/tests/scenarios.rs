//! Offline checks for the xUDT recipes and the multisig / timelock layouts.
//!
//! These do not open a socket and do not need a RISC-V binary. The matching
//! on-chain timelock and threshold multisig live in `examples/scenarios`.

use ckb_cinnabar_calculator::{
    address::Address,
    instruction::predefined::{mint_xudt, transfer_xudt},
    intent,
    scenario::{absolute_timestamp_since, encode_multisig_script, multisig_lock_args},
};

fn addr(text: &str) -> Address {
    text.parse().expect("address")
}

#[test]
fn xudt_recipes_use_mint_and_transfer_names() {
    let issuer = addr("ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqfqmf4hphl9jkrw3934mwe6m3a2nx88rzgdlw820");
    let holder = addr("ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqtz32u8mgzk57zdtt6n62z4y2zyh8egkdcahyxk3");
    let mint = mint_xudt(&issuer, &holder, 1_000, vec![7]);
    let transfer = transfer_xudt(&holder, &issuer, &issuer, 40, vec![7]);
    assert_eq!(mint.name, intent::MINT);
    assert_eq!(transfer.name, intent::TRANSFER);
}

#[test]
fn absolute_timestamp_since_sets_metric_and_value() {
    let since = absolute_timestamp_since(1_700_000_000);
    assert_eq!(since & 0x8000_0000_0000_0000, 0, "absolute, not relative");
    assert_eq!(since & 0x6000_0000_0000_0000, 0x4000_0000_0000_0000);
    assert_eq!(since & 0x00FF_FFFF_FFFF_FFFF, 1_700_000_000);
}

#[test]
fn multisig_script_layout_and_lock_args() {
    let keys = [[0x11; 20], [0x22; 20], [0x33; 20]];
    let script = encode_multisig_script(1, 2, &keys).unwrap();
    assert_eq!(&script[..4], &[0, 1, 2, 3]);
    assert_eq!(&script[4..24], &[0x11; 20]);
    assert_eq!(script.len(), 4 + 60);
    let args = multisig_lock_args(1, 2, &keys).unwrap();
    assert_eq!(args.len(), 20);
    assert!(encode_multisig_script(3, 2, &keys).is_err());
    assert!(encode_multisig_script(0, 0, &keys).is_err());
}
