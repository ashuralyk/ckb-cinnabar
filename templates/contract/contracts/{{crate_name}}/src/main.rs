#![no_main]
#![no_std]

//! On-chain Verify tree for this contract.
//!
//! `Root` classifies the running lock as create / transfer / burn via
//! `this_script_pattern`, then hops to the matching `intent::*` node.
//! Keep those strings identical to `Instruction::named` in `calculator/`.
//!
//! Return `Ok(None)` to succeed, `Ok(Some(next))` to continue, `Err` to
//! fail. Custom errors start at `CUSTOM_ERROR_START` (20).

use ckb_cinnabar_verifier::{
    cinnabar_main, define_errors, intent, this_script_pattern, Result, ScriptPattern, ScriptPlace,
    Verification, CUSTOM_ERROR_START, TREE_ROOT,
};

define_errors!(
    ScriptError,
    {
        UnknownPattern = CUSTOM_ERROR_START,
    }
);

/// Per-run state threaded through every verification node. Put parsed cell
/// data, counts, or hashes here so later nodes do not re-load the same cells.
#[derive(Default)]
struct Context {}

/// Tree entry: dispatch on whether this lock appears in inputs, outputs, or both.
#[derive(Default)]
struct Root {}

impl Verification<Context> for Root {
    fn verify(&mut self, _name: &str, _ctx: &mut Context) -> Result<Option<&str>> {
        match this_script_pattern(ScriptPlace::Lock)? {
            ScriptPattern::Create => Ok(Some(intent::CREATE)),
            ScriptPattern::Transfer => Ok(Some(intent::TRANSFER)),
            ScriptPattern::Burn => Ok(Some(intent::BURN)),
        }
    }
}

/// Script only in outputs: a new cell locked by this contract is created.
#[derive(Default)]
struct Create {}

impl Verification<Context> for Create {
    fn verify(&mut self, _name: &str, _ctx: &mut Context) -> Result<Option<&str>> {
        Ok(None)
    }
}

/// Script in both inputs and outputs: cells are transferred or updated.
#[derive(Default)]
struct Transfer {}

impl Verification<Context> for Transfer {
    fn verify(&mut self, _name: &str, _ctx: &mut Context) -> Result<Option<&str>> {
        Ok(None)
    }
}

/// Script only in inputs: cells locked by this contract are destroyed.
#[derive(Default)]
struct Burn {}

impl Verification<Context> for Burn {
    fn verify(&mut self, _name: &str, _ctx: &mut Context) -> Result<Option<&str>> {
        Ok(None)
    }
}

cinnabar_main!(
    Context,
    (TREE_ROOT, Root),
    (intent::CREATE, Create),
    (intent::TRANSFER, Transfer),
    (intent::BURN, Burn)
);
