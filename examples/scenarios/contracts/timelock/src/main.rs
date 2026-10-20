#![no_std]
#![no_main]

//! Absolute unix-time lock.
//!
//! Args are 8 little-endian bytes: the earliest unix second at which the
//! cell may be spent. Every input in this lock group must carry an RFC 0017
//! absolute timestamp `since` greater than or equal to that second.

use ckb_cinnabar_verifier::{
    cinnabar_main, define_errors, this_script_args, Result, Verification, CUSTOM_ERROR_START,
    TREE_ROOT,
};
use ckb_std::{ckb_constants::Source, ckb_types::prelude::Unpack, high_level::load_input};

define_errors!(
    TimelockError,
    {
        BadArgs = CUSTOM_ERROR_START,
        SinceNotMet,
    }
);

/// No shared parse. Each run reads args and `since` directly.
#[derive(Default)]
struct Context {}

/// Single node: a lock only runs when its cell is spent.
#[derive(Default)]
struct Root {}

impl Verification<Context> for Root {
    fn verify(&mut self, _name: &str, _ctx: &mut Context) -> Result<Option<&str>> {
        let unlock_at = unlock_at()?;
        let input = load_input(0, Source::GroupInput)?;
        let since: u64 = input.since().unpack();
        if !since_met(since, unlock_at) {
            return Err(TimelockError::SinceNotMet.into());
        }
        Ok(None)
    }
}

fn unlock_at() -> Result<u64> {
    let args = this_script_args()?;
    if args.len() != 8 {
        return Err(TimelockError::BadArgs.into());
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&args);
    Ok(u64::from_le_bytes(buf))
}

fn since_met(since: u64, unlock_at: u64) -> bool {
    const RELATIVE: u64 = 1 << 63;
    const METRIC_MASK: u64 = 0x6000_0000_0000_0000;
    const TIMESTAMP: u64 = 0x4000_0000_0000_0000;
    const VALUE_MASK: u64 = 0x00FF_FFFF_FFFF_FFFF;
    if since & RELATIVE != 0 || (since & METRIC_MASK) != TIMESTAMP {
        return false;
    }
    (since & VALUE_MASK) >= unlock_at
}

cinnabar_main!(Context, (TREE_ROOT, Root));
