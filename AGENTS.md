# AGENTS.md

Cinnabar is a CKB contract framework: **Calculate** assembles transactions off-chain, **Verify** checks them on-chain. Use this file as the golden path when generating a contract project. Maintainer conventions live in [CONTRIBUTING.md](CONTRIBUTING.md). The interview method is [skills/cinnabar-agent/SKILL.md](skills/cinnabar-agent/SKILL.md).

## Mental model

Split CKB physics and the user’s rules into **minimal modules**, design how those
modules **relate** (identities, legal transitions, auth, neighbors, time), then
fold shared parse/compute **outputs into `Context`**. Verify is that flowchart
on-chain; Calculate is an `Operation` pipeline that lands on one relation.
A product sentence is not enough to start. One product question at a time,
until the user has emptied every absent product detail in their own words.
Only then show the verification tree and wait for confirmation before
generating or coding (`skills/cinnabar-agent/confirm.md`). Calculate recipes
and FakeRpc cases come from the accepted tree.

```
product demands
 → one question at a time until every absent product detail is said
 → user confirms the tree
 → cinnabar_main! hops (intent::* only when morphology = business)
 → Instruction::new (or named on that shortcut)
 → FakeRpc + TransactionSimulator
 → make build && make test (hard accept; every change)
 → ckb-cinnabar --json --dry-run deploy
```

`Instruction` is a pipeline of `Operation`s (Inputs / Outputs / CellDeps / Headers /
Witnesses). Coupling is the shared byte layout and the transition table, not a
mandatory name match.

## Generate a project

Scaffold from `templates/contract` in this repository. The old
`ckb-cinnabar-scripts-template` repository is not the path.

```bash
# inside a checkout of this repository (absolute path to the checkout)
cargo generate --path templates/contract --name my-lock \
  --define use_local_cinnabar=true --define cinnabar_path="$PWD"
cd my-lock
make prepare   # rustup target add riscv64imac-unknown-none-elf
make build     # writes build/release/<crate>
make test
```

The template default is git dependencies (`use_local_cinnabar=false`).
Pass both defines above to depend on a local checkout. `cinnabar_path`
must be absolute: Cargo resolves it from the generated project, and the
old relative default `../cinnabar` does not point at this repository.

Hard accept: after generate and after every later change, `make build` and
`make test` (full suite) must both exit 0 before the work is done.

Or copy `templates/contract` and replace `{{placeholders}}`.

| Path | Role |
| --- | --- |
| `contracts/<name>/` | `no_std` Verify script (`cinnabar_main!`) |
| `calculator/` | Off-chain `Instruction` helpers |
| `tests/` | `FakeRpcClient` + `assert_verify!` |
| `deployment/` | JSON records from `ckb-cinnabar` |
| `build/release/` | RISC-V binaries (`--contract-path` default) |

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

Target: `riscv64imac-unknown-none-elf`. Error codes: [docs/error-codes.md](docs/error-codes.md).

SSRI is the verifier feature `ssri` (on by default): `SSRI { "Wire.name" => expr }` on `cinnabar_main!`. Do not register hop `verify()` as an SSRI method. `ssri_methods!` already emits `SSRI.version`, `get_methods`, and `has_methods`. The feature depends on git `ckb-ssri-std` at `8c2c245`.

## Write the off-chain assembler

```rust
use ckb_cinnabar_calculator::{
    address::Address, intent, instruction::Instruction,
    operation::basic::{AddInputCellByAddress, AddOutputCell}, rpc::RPC,
};

/// `capacity` is absolute shannons. Shell profile (`std`) only:
/// `AddInputCellByAddress` is not in the kernel (`--no-default-features`).
pub fn transfer<T: RPC>(from: Address, to: Address, capacity: u64) -> Instruction<T> {
    Instruction::named(
        intent::TRANSFER, // morphology-scale only; protocol-scale uses Instruction::new
        vec![
            Box::new(AddInputCellByAddress { address: from }),
            Box::new(AddOutputCell {
                lock_script: to.into(),
                type_script: None,
                data: vec![],
                capacity,
                absolute_capacity: true,
                type_id: false,
            }),
        ],
    )
}
```

Import `instruction`, `operation`, `rpc`, and `skeleton` from the crate root. `--no-default-features` is the kernel. Default `std` is the shell (FakeRpc, recipes, `assert_verify!`).

Predefined recipes (native, not wasm): `secp256k1_sighash_transfer`, `dao_deposit`, `dao_withdraw_phase_one`, `dao_withdraw_phase_two`, `mint_xudt`, `transfer_xudt`. Spore helpers are experimental (`--features spore`).

One recipe per hop. Log keys live in `ckb_cinnabar_calculator::intent::log`.

## Local simulation (no chain)

```rust
use ckb_cinnabar_calculator::{
    assert_verify,
    instruction::Instruction,
    operation::basic::AddOutputCell,
    simulation::{
        always_success_script, AddFakeAlwaysSuccessCelldep, AddFakeContractCelldepByName,
        AddFakeInputCell, FakeRpcClient,
    },
};

#[tokio::test]
async fn transfer_ok() {
    let rpc = FakeRpcClient::default();
    let lock = always_success_script(vec![]);
    let prepare = Instruction::new(vec![
        Box::new(AddFakeAlwaysSuccessCelldep {}),
        Box::new(AddFakeContractCelldepByName {
            contract: "my_lock".into(),
            type_id_args: None,
            contract_binary_path: "../build/release".into(),
        }),
        Box::new(AddFakeInputCell {
            lock_script: lock.clone().into(),
            type_script: None,
            data: vec![],
            capacity: 200_000_000_000,
            absolute_capacity: true,
        }),
    ]);
    let transfer_ix = Instruction::new(vec![Box::new(AddOutputCell {
        lock_script: lock.into(),
        type_script: None,
        data: vec![],
        capacity: 100_000_000_000,
        absolute_capacity: true,
        type_id: false,
    })]);
    assert_verify!(&rpc, vec![prepare, transfer_ix], 0).unwrap();
}
```

`0` = success. Non-zero = on-chain `i8` from `define_errors!`.
Script failures use `CalculatorError::ScriptValidation`; inspect
`script_exit_code()` instead of parsing the display message.

`assert_verify!` prints `[ckb-vm] <file>:<line> consumed cycles: N`. The template `make test` passes `--nocapture`.

## Deploy (headless)

```bash
ckb-cinnabar --json --dry-run --privkey-env CINNABAR_PRIVKEY \
  deploy --contract-name my_lock --tag v0.1.0 \
  --payer-address ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsq28phxutezqvjgfv5q38gn5kwek4m9km3cmajeqs

ckb-cinnabar --json list --contract-name my_lock
```

Global flags (`--json`, `--dry-run`, `--privkey-env`, `-n`) go before the subcommand.

`--privkey-env` reads a hex secp256k1 key. Without it, live send still uses interactive `ckb-cli`. `--dry-run` skips send and skips ckb-cli. It still asks the network indexer for the payer's live cells, so a node must be reachable. A successful JSON object includes `record`. The file `deployment/<network>/<name>.json` is written only when the transaction is sent.

With `--json`, success and failure both print one JSON object to stdout. Failures set `ok: false`, include `error.kind` / `error.message`, keep stderr empty, and exit non-zero (`2` for invalid input, `1` otherwise). Contract validation failures also include `error.exit_code`. Kinds: [docs/error-codes.md](docs/error-codes.md).

## Error codes

System 1–5, verify tree 10–11, SSRI 12–18, custom ≥ 20 (`CUSTOM_ERROR_START`). Full table: [docs/error-codes.md](docs/error-codes.md). `CalculatorError::kind()` is the stable string for assembly failures (`cell_dep_not_found`, `script_validation`, …).

## Public crates

- `ckb-cinnabar-core` — shared `no_std` intent names.
- `ckb-cinnabar-calculator` — assembly, FakeRpc, simulator.
- `ckb-cinnabar-verifier` — `no_std` verification tree. Feature `ssri` (default) is the guest door.
- `ckb-cinnabar-verifier-macros` — SSRI wire-name proc macro.
- `ckb-cinnabar` — deploy / migrate / consume / list CLI. Install with `cargo install --git https://github.com/ashuralyk/ckb-cinnabar`.

Root re-exports: `Address`, `Instruction`, `TransactionCalculator`, `TransactionSkeleton`, `Network`, `RpcClient`, `CalculatorError`, `intent`.

## Don'ts

- Do not invent a Capsule flow or a `deployment.toml`. Deploy with `ckb-cinnabar`.
- Do not treat `examples/*.rs` in this repo as the contract scaffold. They are CLI demos (`secp256k1_transfer`, `dao`, `spore`). Full projects come from `templates/contract`. Worked locks with CKB-VM tests: `examples/scenarios`.
- Do not use `intent::*` for a hop that is not exactly that morphology. Protocol-scale hops use domain strings and `Instruction::new`.
- Do not use FakeRpc or `assert_verify!` on the kernel profile (`--no-default-features`).
- Do not shrink `RPC` to the SSRI subset. Tip, fee, and `get_transactions` stay on the trait and return `SourceUnavailable` from `SsriSource`.
- Do not add recipe-level `insert_index` or `relative_index`.
- Do not map each molecule error code. One shared custom error (for example `InvalidData`) covers decode and encode failures.
- Do not call the work done until `make build` and the full `make test` both exit 0.
- Foreign Spore, Cluster, xUDT, and type-burn binaries are not committed. Run `skills/cinnabar-agent/binaries/download.sh` and load them as cell deps when a test must execute those scripts.
