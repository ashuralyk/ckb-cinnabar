use alloc::vec::Vec;
use blake2b_ref::Blake2bBuilder;
use ckb_std::{
    ckb_constants::Source,
    ckb_types::prelude::{Entity, Unpack},
    high_level::{load_cell, load_input, load_script, QueryIter},
};

use crate::Error;

pub const CKB_HASH_PERSONALIZATION: &[u8] = b"ckb-default-hash";

pub fn calc_type_id(out_index: usize) -> Result<[u8; 32], Error> {
    let input = load_input(0, Source::Input)?;
    let mut hash = Blake2bBuilder::new(32)
        .personal(CKB_HASH_PERSONALIZATION)
        .build();
    hash.update(input.as_slice());
    hash.update(&(out_index as u32).to_le_bytes());
    let mut type_id = [0u8; 32];
    hash.finalize(&mut type_id);
    Ok(type_id)
}

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

pub fn this_script_args() -> Result<Vec<u8>, Error> {
    let script = load_script()?;
    let args = script.args().unpack();
    Ok(args)
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum ScriptPlace {
    Lock,
    Type,
}

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

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum ScriptPattern {
    Create,
    Transfer,
    Burn,
}

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

pub fn this_script_count(source: Source, place: ScriptPlace) -> Result<usize, Error> {
    let indices = this_script_indices(source, place)?;
    Ok(indices.len())
}
