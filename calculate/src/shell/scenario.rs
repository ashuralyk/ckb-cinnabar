//! Pure helpers for the xUDT, multisig, and timelock examples.
//!
//! Nothing here contacts a node. Live-cell recipes stay in
//! [`crate::instruction::predefined`]. The timelock and threshold-multisig
//! contracts under `examples/scenarios` check the same byte layouts on chain.

use ckb_hash::blake2b_256;

use crate::error::{CalculatorError, Result};

/// RFC 0017 absolute unix-time `since`.
///
/// Bit 63 is clear (absolute). Bits 61–62 select the timestamp metric
/// (`0x4000_0000_0000_0000`). The low 56 bits are `unix_seconds`.
pub fn absolute_timestamp_since(unix_seconds: u64) -> u64 {
    const TIMESTAMP_METRIC: u64 = 0x4000_0000_0000_0000;
    const VALUE_MASK: u64 = 0x00FF_FFFF_FFFF_FFFF;
    TIMESTAMP_METRIC | (unix_seconds & VALUE_MASK)
}

/// Witness prefix of the genesis secp256k1 multisig lock, without signatures.
///
/// Layout: `0x00 | require_first_n | threshold | pubkey_count | blake160 * N`.
/// `N` must fit in `u8`. `threshold` is at least 1 and at most `N`.
/// `require_first_n` is at most `threshold`.
pub fn encode_multisig_script(
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
pub fn multisig_lock_args(
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
