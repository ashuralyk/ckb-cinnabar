//! Offline checks for the xUDT recipes and the multisig / timelock layouts.
//!
//! These do not open a socket and do not need a RISC-V binary. The matching
//! on-chain timelock and threshold multisig live in `examples/scenarios`.
//! The byte helpers below are test-local: they are not part of the calculator
//! API.

use ckb_cinnabar_calculator::{
    address::Address,
    error::{CalculatorError, Result},
    instruction::predefined::{mint_xudt, transfer_xudt},
    intent,
    re_exports::ckb_hash::blake2b_256,
};

fn addr(text: &str) -> Address {
    text.parse().expect("address")
}

/// RFC 0017 absolute unix-time `since`.
///
/// Bit 63 is clear (absolute). Bits 61–62 select the timestamp metric
/// (`0x4000_0000_0000_0000`). The low 56 bits are `unix_seconds`.
fn absolute_timestamp_since(unix_seconds: u64) -> u64 {
    const TIMESTAMP_METRIC: u64 = 0x4000_0000_0000_0000;
    const VALUE_MASK: u64 = 0x00FF_FFFF_FFFF_FFFF;
    TIMESTAMP_METRIC | (unix_seconds & VALUE_MASK)
}

/// Witness prefix of the genesis secp256k1 multisig lock, without signatures.
///
/// Layout: `0x00 | require_first_n | threshold | pubkey_count | blake160 * N`.
/// `N` must fit in `u8`. `threshold` is at least 1 and at most `N`.
/// `require_first_n` is at most `threshold`.
fn encode_multisig_script(
    require_first_n: u8,
    threshold: u8,
    pubkey_blake160s: &[[u8; 20]],
) -> Result<Vec<u8>> {
    let count = pubkey_blake160s.len();
    if count > u8::MAX as usize || threshold == 0 || usize::from(threshold) > count {
        return Err(CalculatorError::Other(
            "multisig threshold must be 1..=pubkey count, and count must fit in u8".into(),
        ));
    }
    if require_first_n > threshold {
        return Err(CalculatorError::Other(
            "multisig require_first_n cannot exceed threshold".into(),
        ));
    }
    let mut out = Vec::with_capacity(4 + 20 * count);
    out.push(0);
    out.push(require_first_n);
    out.push(threshold);
    out.push(count as u8);
    for hash in pubkey_blake160s {
        out.extend_from_slice(hash);
    }
    Ok(out)
}

/// Args of the genesis multisig lock: the first 20 bytes of blake2b over
/// [`encode_multisig_script`].
fn multisig_lock_args(
    require_first_n: u8,
    threshold: u8,
    pubkey_blake160s: &[[u8; 20]],
) -> Result<[u8; 20]> {
    let script = encode_multisig_script(require_first_n, threshold, pubkey_blake160s)?;
    let hash = blake2b_256(&script);
    let mut args = [0u8; 20];
    args.copy_from_slice(&hash[..20]);
    Ok(args)
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
