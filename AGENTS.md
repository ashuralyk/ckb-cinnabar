# AGENTS.md

Cinnabar is a CKB contract framework: **Calculate** assembles transactions off-chain, **Verify** checks them on-chain. Use this file as the golden path when generating a contract project.

## Mental model

Split CKB physics and the user’s rules into **minimal modules**, design how those
modules **relate** (identities, legal transitions, auth, neighbors, time), then
fold shared parse/compute **outputs into `Context`**. Verify is that flowchart
on-chain; Calculate is an `Operation` pipeline that lands on one relation.
**Show the split and agree Verify / Calculate / FakeRpc shape and reachable
scope with the user before generating or coding** (`skills/cinnabar-agent/confirm.md`).

```
modules + relations + Context
    → user confirms pack (verifier / calculator / tests + scope)
    → cinnabar_main! hops (intent::* only when morphology = business)
    → Instruction::new (or named on that shortcut)
    → FakeRpc + TransactionSimulator
    → make build && make test (hard accept; every change)
    → ckb-cinnabar deploy --json --dry-run
```

`Instruction` is a pipeline of `Operation`s (Inputs / Outputs / CellDeps / Headers /
Witnesses). Coupling is the shared byte layout and the transition table, not a
mandatory name match. Full method: `skills/cinnabar-agent/SKILL.md`.

## Generate a project

```bash
cargo generate --path /path/to/cinnabar/templates/contract --name my-lock
cd my-lock
make prepare   # rustup target add riscv64imac-unknown-none-elf
make build     # writes build/release/<crate>
make test
```

Hard accept: after generate and after every later change, `make build` and
`make test` (full suite) must both exit 0 before the work is done.

Or copy `templates/contract` and replace `{{placeholders}}`.

Layout:

| Path                | Role                                        |
| ------------------- | ------------------------------------------- |
| `contracts/<name>/` | `no_std` Verify script (`cinnabar_main!`)   |
| `calculator/`       | Off-chain `Instruction` helpers             |
| `tests/`            | `FakeRpcClient` + `assert_verify!`          |
| `deployment/`       | JSON records from `ckb-cinnabar`            |
| `build/release/`    | RISC-V binaries (`--contract-path` default) |

## Write the on-chain script

1. `#![no_std] #![no_main]` crate depending on `ckb-cinnabar-verifier`.
2. `define_errors!(MyError, { First = CUSTOM_ERROR_START, Second, });`
3. `#[derive(Default)] struct Context { ... }` — Root fills shared parses; children read it.
4. One struct per node, `impl Verification<Context>`. Return `Ok(Some("hop"))` or `Ok(None)` or `Err(...)`.
5. Register Root plus every hop. Use `intent::*` when each hop is exactly Create/Transfer/Burn of this script; otherwise domain strings from the transition table:

```rust
use ckb_cinnabar_verifier::{
    cinnabar_main, define_errors, intent, this_script_pattern, Result, ScriptPattern,
    ScriptPlace, Verification, CUSTOM_ERROR_START, TREE_ROOT,
};

cinnabar_main!(
    Context,
    (TREE_ROOT, Root),
    (intent::CREATE, Create),
    (intent::TRANSFER, Transfer),
    (intent::BURN, Burn),
);
```

Morphology-scale Root dispatches with `this_script_pattern(ScriptPlace::Lock)` or `ScriptPlace::Type`. Protocol-scale Root parses identity from args/data into `Context` first.

Error budget: sys 1–5, framework 10–11 (verify tree) and 12–18 (optional SSRI), custom ≥ 20 (`CUSTOM_ERROR_START`).

## Write the off-chain assembler

```rust
use ckb_cinnabar_calculator::{
    intent, instruction::Instruction, operation::basic::*, rpc::RPC,
};

pub fn transfer<T: RPC>(from: Address, to: Address, ckb: u64) -> Instruction<T> {
    Instruction::named(
        intent::TRANSFER, // morphology-scale only; protocol-scale uses Instruction::new
        vec![
            Box::new(AddInputCellByAddress { address: from }),
            Box::new(AddOutputCell { /* ... */ }),
        ],
    )
}
```

Predefined recipes (native, not wasm): `secp256k1_sighash_transfer`, `dao_deposit`, `dao_withdraw_phase_one`, `dao_withdraw_phase_two`, `mint_xudt`, `transfer_xudt`. Spore helpers are **experimental** (`--features spore`).

Log keys live in `ckb_cinnabar_calculator::intent::log`.

## Local simulation (no chain)

```rust
use ckb_cinnabar_calculator::{
    assert_verify,
    instruction::Instruction,
    simulation::{
        AddFakeAlwaysSuccessCelldep, AddFakeContractCelldepByName, AddFakeInputCell,
        FakeRpcClient,
    },
};

#[tokio::test]
async fn transfer_ok() {
    let rpc = FakeRpcClient::default();
    let prepare = Instruction::new(vec![
        Box::new(AddFakeContractCelldepByName {
            contract: "my_lock".into(),
            type_id_args: None,
            contract_binary_path: "../build/release".into(),
        }),
        Box::new(AddFakeInputCell { /* lock = the contract */ .. }),
    ]);
    assert_verify!(&rpc, vec![prepare, transfer_ix], 0).unwrap();
}
```

`0` = success. Non-zero = on-chain `i8` from `define_errors!`.
Script failures use `CalculatorError::ScriptValidation`; inspect
`script_exit_code()` instead of parsing the display message.

## Deploy (headless)

```bash
# dry-run JSON, no send, no ckb-cli prompt
ckb-cinnabar --json --dry-run --privkey-env CINNABAR_PRIVKEY \
  deploy --contract-name my_lock --tag v0.1.0 --payer-address ckt1...

ckb-cinnabar --json list --contract-name my_lock
```

`--privkey-env` reads a hex secp256k1 key. Without it, live send still uses interactive `ckb-cli`. `--dry-run` skips send and skips ckb-cli. Records go to `deployment/<network>/<name>.json`.

With `--json`, success and failure both print one JSON object to stdout. Failures
set `ok: false`, include `error.kind` / `error.message`, keep stderr empty, and
exit non-zero. Contract validation failures also include `error.exit_code`.

## Public crates

- `ckb-cinnabar-core` — shared `no_std` intent vocabulary used by Calculate and Verify.
- `ckb-cinnabar-calculator` — assembly, FakeRpc, simulator. Errors: `CalculatorError` (`kind()` for JSON). `kernel` is the always-on `no_std` assembler: sync [`rpc::RPC`] (`Node` + `Indexer`), packed skeleton, `Operation` / `Instruction`. `Source` is a derivable three-lookup subset of that surface (inject → operate → pack). `shell` is the `std` host packed as one tree on top (`RpcClient` HTTP adapter, tokio, FakeRpc, CKB-VM, plus std facades of instruction/operation/skeleton); crate-root `instruction` / `operation` / `rpc` paths alias the shell. HTTP `block_on` lives inside `RpcClient`; operations stay sequential and sync. A future SSRI guest implements the same kernel `RPC` via syscalls — do not size the trait to today's three SSRI find_* methods. Do not use FakeRpc or `assert_verify!` on the kernel profile (`--no-default-features`). Spore molecule types live in `kernel::operation::spore::schema` (`serde_molecule` + `alloc`) when `--features spore` is on.
- `ckb-cinnabar-verifier` — `no_std` tree. Target: `riscv64imac-unknown-none-elf`. Optional feature `ssri` adds a guest SSRI door (`cinnabar_main!` trailing `SSRI { "Wire.name" => expr }`, `SsriSource`, kernel assemble) on top of [`ckb-ssri-std`](https://github.com/ashuralyk/ckb-ssri-std) (`ssri_methods!`, `should_fallback`, `find_*`).
- `ckb-cinnabar` — deploy / migrate / consume / list CLI.

Root re-exports: `Address`, `Instruction`, `TransactionCalculator`, `TransactionSkeleton`, `Network`, `RpcClient`, `CalculatorError`, `intent`.

## Learned User Preferences

- Rust paths: collapse sibling `use` items that share a prefix into one braced import (`use crate::{abc, bcd}`, nested braces for deeper siblings). Inline type paths (signatures, locals, turbofish) are at most two segments (`module::Type` or `Type`); the first segment is never `crate`. Longer paths belong in `use` at the top of the file, then the short form in the body.
- Prefer developer-facing rustdoc that explains both types and functions/behavior, not agent-only comments.
- Optimize so an AI agent writing a CKB contract surfaces Cinnabar even if the user never named it; keep `cinnabar-agent` usable as a standalone skill install (no local cinnabar checkout required); keep README explicit about agent-friendly features.
- Prefer Chinese for product and strategy discussion; implementation tasks may be in English.
- Keep agent-readiness work in this repository; treat `cinnabar-examples` as reference only. Breaking public API, error types, and CLI output is acceptable for that goal.
- Before generating or coding a contract, show the module split and interactively agree Verify / Calculate / FakeRpc shape and reachable scope with the user.
- Keep `templates/`, `AGENTS.md`, and `README.md` aligned with current Calculate APIs; do not add recipe-level `insert_index` or `relative_index`.
- Treat directory layout as the architecture. Each layer is one complete runnable instance, packed in one tree and named for what it is (`kernel`, `shell`). Adapters live *outside* the kernel tree and compose extra capability. Do not split a type from its impl, reimplement the same type per environment, or wrap an already-layer-owned file in nested `mod` / `cfg` theater (`std_impl`, parallel `host/`/`ssri/` trees).
- Calculate layout follows that: pack the complete `no_std` assembler into `calculate/src/kernel/` (types, skeleton, `Instruction`, sync `RPC`/`Indexer`, and `Operation` impls together). Pack the complete `std` host into `calculate/src/shell/` (`RpcClient` / FakeRpc adapters, JSON indexer wire types, signing, predefined recipes, plus std facades of instruction/operation/skeleton). Developers write one formula (`Instruction` of `Operation`s → `TransactionCalculator` → `TransactionSkeleton`) via crate-root `instruction` / `operation` / `rpc` / `skeleton`; `std` vs `--no-default-features` only aliases those names to shell or kernel — no `kernel::` / `shell::` in application code. `--no-default-features` is that packed kernel, not a second copy of Calculate. `Address` (ckb2021 bech32m) lives in the kernel (`no_std` + `alloc`). Host extras (recipes, FakeRpc, `assert_verify!`) drop when `std` is off. `Source` is a derivable subset of `Node` + `Indexer`; SSRI later implements kernel `RPC`.
- One always-on `impl<C: RPC>` (or `S: Source` when only the three lookups run) per kernel type. Do not XOR-compile a second `impl<T: RPC>` on the same struct (`RPC: Source` already covers the host). Shell adds sign / Fake celldep / DAO phase-two types; recipes compose them with the kernel output. The shell `address` module re-exports kernel `Address`.
- Shell operation files only compile on `std`; put host items at module level and re-export the matching kernel module. On the kernel profile, crate-root `pub use crate::kernel::operation` (and `instruction` / `skeleton`). Do not XOR-compile the same file for both profiles.
- Pair Verify with SSRI via an optional trailing `SSRI { "Wire.name" => expr, ... }` on `cinnabar_main!` (crate feature `ssri`). The block is the door plus an explicit wire table, not a protocol identity. Each RHS is any expression `export` turns into bytes (fn / `&[u8]` / `u8` / `Vec<u8>` / `Result`). `ssri_methods!` always emits `SSRI.version` / `get_methods` / `has_methods`; do not list those wire strings in the block. Do not register hop `verify()` as SSRI methods or merge Verify `Context` with Calculate `C: RPC`. Kernel `Instruction`s compile to RISC-V and are the assemble face; hops stay the on-chain face. Typed host recipes are not the RHS as-is — the method writer supplies a guest wrapper that decodes `SsriArgs` and calls the typed calculator. How callers lay out method arguments inside `argv` is unknown to the framework: `SsriArgs` only holds decoded argv bytes and helpers to convert a chosen slot (`get` for `FromSsriArg`, `molecule` for any molecule `Entity`); it must not assume `argv[1]` is a held transaction or `argv[2..]` is the rest.

## Learned Workspace Facts

- `cinnabar-examples` is a sibling of this repo (upstream `ashuralyk/cinnabar-examples`); use it as a pattern source, not as delivery scope.
- After `AGENTS.md`, agents should follow `skills/cinnabar-agent/SKILL.md`; do not invent a Capsule/`deployment.toml` flow.
- In-repo `examples/` (`secp256k1_transfer`, `dao`, `spore`) are CLI demos; generate full contract projects from `templates/contract`.
- WarSporeSaga contracts live in `spore-war/contracts`; Opticrum’s public tree is https://github.com/Opticrum/ckb-contract-script (local checkout may be `fiber/opticrum`). Both are protocol-scale Cinnabar references, not delivery scope.
- `skills/cinnabar-agent/binaries/` (`spore`, `cluster`, `xudt`, `type_burn`) came from WarSporeSaga FakeRpc tests; load them as cell deps when composition with those protocols must run for real before on-chain submit.
- Calculate `kernel/` is the complete `no_std` assembler (sync `RPC`/`Indexer`/`Source`, packed skeleton, `Operation` / `Instruction`, ckb2021 `Address`). `shell/` is the complete `std` host (`RpcClient`, JSON indexer, FakeRpc, simulation, facades of `instruction` / `operation` / `skeleton`). Crate-root `pub use`s the shell modules on `std` and the kernel modules on `--no-default-features`. `shell/operation/{basic,dao,udt,component,spore}.rs` re-export the matching kernel module plus host-only ops.
- `TransactionSkeleton` `get_input_by_index` / `get_output_by_index` / `get_celldep_by_index` accept Lua-style `-n` as `(-n) as usize` (`-1` last, `-2` second-to-last); `resolve_get_index` returns `(Option<usize>, empty)` and callers own error strings.
- Experimental `--features spore` encodes with `serde_molecule` via `kernel::operation::spore::schema` (re-exported as `operation::spore::schema`); `.mol` files stay as layout notes, and `operation::spore::generated` is gone.
- Local SSRI references live under `~/Cryptape/ssri/`: `ssri-udt` (stripped) and sibling `pausable-udt` (fuller). Feature `ssri` depends on git `https://github.com/ashuralyk/ckb-ssri-std` (ckb-std 1.1); crates.io `ckb-ssri-std` 0.0.1 still pins ckb-std 0.16. Empty `argv` is on-chain verify; `argv` plus raw `vm_version == u64::MAX` is SSRI. `ssri_methods!` owns the three builtins. `SsriSource` implements kernel `Source` via `find_*` only (not full `RPC`). Packed syscall bytes are `from_slice`'d into `ckb-gen-types`. Use those repos as a pattern source, not delivery scope.
