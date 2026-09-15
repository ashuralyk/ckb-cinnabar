//! Helpers for inspecting the running script inside a verification node.
//!
//! Use [`this_script_pattern`] at the tree root to dispatch create / transfer /
//! burn. [`calc_type_id`] matches the type-id formula used off-chain by
//! `TransactionSkeleton::calc_type_id`.

use alloc::vec::Vec;
use blake2b_ref::Blake2bBuilder;
use ckb_std::{
    ckb_constants::Source,
    ckb_types::prelude::{Entity, Unpack},
    high_level::{load_cell, load_input, load_script, QueryIter},
};

use crate::Error;

/// Blake2b personalization string required by CKB (`ckb-default-hash`).
pub const CKB_HASH_PERSONALIZATION: &[u8] = b"ckb-default-hash";

/// Compute the type id for the output at `out_index`: blake2b of the first
/// input's `CellInput` molecule bytes followed by the little-endian index.
pub fn calc_type_id(out_index: usize) -> Result<[u8; 32], Error> {
    let input = load_input(0, Source::Input)?;
    let mut hash = Blake2bBuilder::new(32)
        .personal(CKB_HASH_PERSONALIZATION)
        .build();
    hash.update(input.as_slice());
    hash.update(&(out_index as u64).to_le_bytes());
    let mut type_id = [0u8; 32];
    hash.finalize(&mut type_id);
    Ok(type_id)
}

/// blake2b (with CKB personalization) over the concatenation of `updates`,
/// output size `N`.
pub fn calc_blake2b_hash<const N: usize, T: AsRef<[u8]>>(updates: &[T]) -> [u8; N] {
    let mut hash = Blake2bBuilder::new(N)
        .personal(CKB_HASH_PERSONALIZATION)
        .build();
    for update in updates {
        hash.update(update.as_ref());
    }
    let mut result = [0u8; N];
    hash.finalize(&mut result);
    result
}

/// Args of the script currently being executed.
pub fn this_script_args() -> Result<Vec<u8>, Error> {
    let script = load_script()?;
    let args = script.args().unpack();
    Ok(args)
}

/// Which slot of a cell the running script occupies.
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum ScriptPlace {
    /// The running script is a lock script.
    Lock,
    /// The running script is a type script.
    Type,
}

/// Indices of cells carrying the running script within `source`
/// ([`Source::Input`], [`Source::Output`], ...).
pub fn this_script_indices(source: Source, place: ScriptPlace) -> Result<Vec<usize>, Error> {
    let script = load_script()?;
    let indices = QueryIter::new(load_cell, source)
        .enumerate()
        .filter_map(|(i, cell)| {
            if place == ScriptPlace::Lock && cell.lock() == script {
                return Some(i);
            }
            if let Some(type_) = cell.type_().to_opt() {
                if place == ScriptPlace::Type && type_ == script {
                    return Some(i);
                }
            }
            None
        })
        .collect();
    Ok(indices)
}

/// Cell-morphology of the running script within the transaction, the usual
/// root dispatch of a verification tree. Matches the [`crate::intent`] names
/// `create` / `transfer` / `burn`.
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum ScriptPattern {
    /// Script appears in outputs only — new cells are being created.
    Create,
    /// Script appears in both inputs and outputs — cells are being moved/changed.
    Transfer,
    /// Script appears in inputs only — cells are being destroyed.
    Burn,
}

/// Classify the running script's [`ScriptPattern`] at `place`.
pub fn this_script_pattern(place: ScriptPlace) -> Result<ScriptPattern, Error> {
    let in_input = this_script_count(Source::Input, place)? > 0;
    let in_output = this_script_count(Source::Output, place)? > 0;
    match (in_input, in_output) {
        (true, true) => Ok(ScriptPattern::Transfer),
        (true, false) => Ok(ScriptPattern::Burn),
        (false, true) => Ok(ScriptPattern::Create),
        _ => unreachable!("never touch here"),
    }
}

/// Number of cells carrying the running script within `source`.
pub fn this_script_count(source: Source, place: ScriptPlace) -> Result<usize, Error> {
    let indices = this_script_indices(source, place)?;
    Ok(indices.len())
}
