#![no_std]
#![no_main]

//! Compiles the SSRI arm of [`cinnabar_main!`] against `ckb-cinnabar-verifier-ssri`.
//!
//! Empty `argv` walks the hop tree. An SSRI VM (`argv` set and
//! `vm_version == u64::MAX`) serves `UDT.name` and `UDT.decimals`.

use ckb_cinnabar_verifier::{cinnabar_main, intent, Result, Verification, TREE_ROOT};

/// Unused per-run state. This smoke contract does not parse cells.
#[derive(Default)]
struct Context {}

/// Always hop to transfer. The point of this binary is the SSRI table.
#[derive(Default)]
struct Root {}

impl Verification<Context> for Root {
    fn verify(&mut self, _name: &str, _ctx: &mut Context) -> Result<Option<&str>> {
        Ok(Some(intent::TRANSFER))
    }
}

/// Accept any transfer. Not a real asset rule.
#[derive(Default)]
struct Transfer {}

impl Verification<Context> for Transfer {
    fn verify(&mut self, _name: &str, _ctx: &mut Context) -> Result<Option<&str>> {
        Ok(None)
    }
}

cinnabar_main!(
    Context,
    (TREE_ROOT, Root),
    (intent::TRANSFER, Transfer),
    SSRI {
        "UDT.name" => b"smoke".as_slice(),
        "UDT.decimals" => 8u8,
    }
);
