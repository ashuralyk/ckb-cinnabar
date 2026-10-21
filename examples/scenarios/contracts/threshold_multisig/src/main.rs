#![no_std]
#![no_main]

//! Preimage threshold lock (M-of-N without secp256k1).
//!
//! Args: `threshold: u8 || count: u8 || blake2b-256(preimage) * count`,
//! using the CKB personalization `ckb-default-hash`. The witness `lock`
//! field of the first input in the group is a concatenation of 32-byte
//! preimages. At least `threshold` distinct preimages must match the list.
//!
//! The genesis secp256k1 multisig lock is a different encoding. Its witness
//! prefix is covered by the host test `calculate/tests/scenarios.rs`.

use alloc::vec;

use blake2b_ref::Blake2bBuilder;
use ckb_cinnabar_verifier::{
    cinnabar_main, define_errors, this_script_args, this_script_indices, Result, ScriptPlace,
    Verification, CUSTOM_ERROR_START, TREE_ROOT,
};
use ckb_std::{ckb_constants::Source, ckb_types::prelude::Unpack, high_level::load_witness_args};

define_errors!(
    MultiError,
    {
        BadArgs = CUSTOM_ERROR_START,
        BadWitness,
        ThresholdNotMet,
    }
);

/// No shared parse beyond the args read in [`Root`].
#[derive(Default)]
struct Context {}

/// Count matching preimages and compare with the threshold in the args.
#[derive(Default)]
struct Root {}

impl Verification<Context> for Root {
    fn verify(&mut self, _name: &str, _ctx: &mut Context) -> Result<Option<&str>> {
        let (threshold, hashes) = parse_args()?;
        let preimages = witness_preimages()?;
        let mut seen = vec![false; hashes.len()];
        let mut matched = 0u8;
        for preimage in preimages.chunks_exact(32) {
            let digest = blake2b_256(preimage);
            for (index, hash) in hashes.chunks_exact(32).enumerate() {
                if !seen[index] && hash == digest.as_slice() {
                    seen[index] = true;
                    matched += 1;
                }
            }
        }
        if matched < threshold {
            return Err(MultiError::ThresholdNotMet.into());
        }
        Ok(None)
    }
}

fn parse_args() -> Result<(u8, alloc::vec::Vec<u8>)> {
    let args = this_script_args()?;
    if args.len() < 2 || (args.len() - 2) % 32 != 0 {
        return Err(MultiError::BadArgs.into());
    }
    let threshold = args[0];
    let count = args[1] as usize;
    if count == 0 || threshold == 0 || threshold as usize > count || count * 32 != args.len() - 2 {
        return Err(MultiError::BadArgs.into());
    }
    Ok((threshold, args[2..].to_vec()))
}

fn witness_preimages() -> Result<alloc::vec::Vec<u8>> {
    let indices = this_script_indices(Source::Input, ScriptPlace::Lock)?;
    let index = indices.first().copied().ok_or(MultiError::BadWitness)?;
    let witness = load_witness_args(index, Source::Input).map_err(|_| MultiError::BadWitness)?;
    let lock = witness.lock().to_opt().ok_or(MultiError::BadWitness)?;
    let bytes: alloc::vec::Vec<u8> = lock.unpack();
    if bytes.is_empty() || bytes.len() % 32 != 0 {
        return Err(MultiError::BadWitness.into());
    }
    Ok(bytes)
}

fn blake2b_256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Blake2bBuilder::new(32)
        .personal(b"ckb-default-hash")
        .build();
    hasher.update(data);
    let mut out = [0u8; 32];
    hasher.finalize(&mut out);
    out
}

cinnabar_main!(Context, (TREE_ROOT, Root));
