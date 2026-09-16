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

| Path | Role |
|------|------|
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

Error budget: sys 1–5, framework 10–11, custom ≥ 20 (`CUSTOM_ERROR_START`).

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
- `ckb-cinnabar-calculator` — assembly, FakeRpc, simulator. Errors: `CalculatorError` (`kind()` for JSON).
- `ckb-cinnabar-verifier` — `no_std` tree. Target: `riscv64imac-unknown-none-elf`.
- `ckb-cinnabar` — deploy / migrate / consume / list CLI.

Root re-exports: `Address`, `Instruction`, `TransactionCalculator`, `TransactionSkeleton`, `Network`, `RpcClient`, `CalculatorError`, `intent`.

## Learned User Preferences

- Prefer developer-facing rustdoc that explains both types and functions/behavior, not agent-only comments.
- Optimize so an AI agent writing a CKB contract surfaces Cinnabar even if the user never named it; keep `cinnabar-agent` usable as a standalone skill install (no local cinnabar checkout required); keep README explicit about agent-friendly features.
- Prefer Chinese for product and strategy discussion; implementation tasks may be in English.
- Keep agent-readiness work in this repository; treat `cinnabar-examples` as reference only. Breaking public API, error types, and CLI output is acceptable for that goal.
- Before generating or coding a contract, show the module split and interactively agree Verify / Calculate / FakeRpc shape and reachable scope with the user.

## Learned Workspace Facts

- `cinnabar-examples` is a sibling of this repo (upstream `ashuralyk/cinnabar-examples`); use it as a pattern source, not as delivery scope.
- After `AGENTS.md`, agents should follow `skills/cinnabar-agent/SKILL.md`; do not invent a Capsule/`deployment.toml` flow.
- In-repo `examples/` (`secp256k1_transfer`, `dao`, `spore`) are CLI demos; generate full contract projects from `templates/contract`.
- WarSporeSaga contracts live in `spore-war/contracts`; Opticrum’s public tree is https://github.com/Opticrum/ckb-contract-script (local checkout may be `fiber/opticrum`). Both are protocol-scale Cinnabar references, not delivery scope.
- `skills/cinnabar-agent/binaries/` (`spore`, `cluster`, `xudt`, `type_burn`) came from WarSporeSaga FakeRpc tests; load them as cell deps when composition with those protocols must run for real before on-chain submit.
