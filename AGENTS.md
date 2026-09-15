# AGENTS.md

Cinnabar is a CKB contract framework: **Calculate** assembles transactions off-chain, **Verify** checks them on-chain. Use this file as the golden path when generating a contract project.

## Mental model

```
intent (create | transfer | burn | mint | deposit | withdraw)
    → Verify tree node (same string)
    → Calculate Instruction::named(intent, operations)
    → FakeRpc + TransactionSimulator
    → ckb-cinnabar deploy --json --dry-run
```

`Instruction` ≈ an account-model contract method. `Operation` fills Inputs / Outputs / CellDeps / Witnesses.

## Generate a project

```bash
cargo generate --path /path/to/cinnabar/templates/contract --name my-lock
cd my-lock
make prepare   # rustup target add riscv64imac-unknown-none-elf
make build     # writes build/release/<crate>
make test
```

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
3. `#[derive(Default)] struct Context { ... }`
4. One struct per node, `impl Verification<Context>`. Return `Ok(Some(intent::TRANSFER))` or `Ok(None)` or `Err(...)`.
5. Register with matching **intent constants** (never invent parallel strings):

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

Dispatch with `this_script_pattern(ScriptPlace::Lock)` or `ScriptPlace::Type`.

Error budget: sys 1–5, framework 10–11, custom ≥ 20 (`CUSTOM_ERROR_START`).

## Write the off-chain assembler

```rust
use ckb_cinnabar_calculator::{
    intent, instruction::Instruction, operation::basic::*, rpc::RPC,
};

pub fn transfer<T: RPC>(from: Address, to: Address, ckb: u64) -> Instruction<T> {
    Instruction::named(
        intent::TRANSFER, // MUST equal the Verify node name
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
