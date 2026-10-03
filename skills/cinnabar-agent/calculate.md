# Off-chain Calculate, FakeRpc, and CLI

Recipes and the FakeRpc universe are derived from the verification tree the
user accepted ([confirm.md](confirm.md)). Use this file to implement them.

Calculate **assembles** a skeleton that will be classified by Verify. It does
**not** duplicate on-chain predicates. Coupling is the shared **layout** and
the **transition table**, not a mandatory `Instruction` name.

## Which profile to generate

The profile is the one recorded on the accepted tree
([confirm.md](confirm.md) section 0). Default is non-SSRI. SSRI only when
that tree needs on-chain methods. The template is the same; set
`calculator/` to that profile before writing recipes.

| Version | Calculator crate | Verify |
| ------- | ---------------- | ------ |
| non-SSRI | Shell, the default. Leave `ckb-cinnabar-calculator = { workspace = true }`. | Hop-only `cinnabar_main!`. No `SSRI { }` arm. |
| SSRI | Kernel. `ckb-cinnabar-calculator = { workspace = true, default-features = false }`. `#![no_std]` + `extern crate alloc`. | `SSRI { }` guest wrappers call these kernel `Instruction`s. |

SSRI `calculator/Cargo.toml`:

```toml
[dependencies]
ckb-cinnabar-calculator = { workspace = true, default-features = false }
```

SSRI `calculator/src/lib.rs` starts with `#![no_std]` and `extern crate alloc`.
Recipes use kernel operations only. In the contract crate, depend on that
calculator (`path = "../../calculator"` from `contracts/<name>/`) and call
the recipes from the guest wrappers. Do not put `RpcClient`, FakeRpc,
`assert_verify!`, signing, or predefined host recipes in that crate.

`tests/Cargo.toml` keeps the default shell dependency
(`ckb-cinnabar-calculator = { workspace = true }`) so FakeRpc and
`assert_verify!` exist. Do not turn `std` on inside `calculator/` to satisfy
tests.

non-SSRI leaves the generated calculator dependency as the workspace default
(shell). The contract does not need to depend on `calculator/` for a guest
entry.

## Layers

Generated contracts import crate-root paths only:

`instruction`, `operation`, `rpc`, `skeleton`, `address`, `intent`.

The version above picks which layer those names resolve to. Do not import
`kernel::` or `shell::`.

| Profile | Cargo | When | Contents |
| ------- | ----- | ---- | -------- |
| Shell | default `std` | non-SSRI calculator, and every `tests/` crate | Kernel plus `RpcClient`, FakeRpc, CKB-VM, signing, predefined recipes, `assert_verify!` |
| Kernel | `--no-default-features` | SSRI calculator crate, and the guest that calls it | `no_std` + `alloc` assembler: sync `RPC` (`Node` + `Indexer`), `Source`, packed skeleton, operations, ckb2021 `Address` |

`Address` / `AddressPayload` parse full ckb2021 addresses (`ckb1…` / `ckt1…`,
bech32m, RFC-0021). They compile on both profiles.

`Source` is three lookups: `find_out_point_by_type`, `find_cell_by_out_point`,
`find_cell_data_by_out_point`. Every `RPC` implements `Source`.
`UnsupportedSource` answers every lookup with `SourceUnavailable`. Inject-only
assembly uses it when the caller already has the cells to place on the
skeleton. Guest `SsriSource` is a kernel `RPC` for `network`, `get_live_cell`,
headers, block hashes, and `get_cells`. `get_tip_header`,
`get_tip_block_number`, `min_fee_rate`, and `get_transactions` return
`SourceUnavailable`.

FakeRpc, `assert_verify!`, and the predefined recipes exist on the shell
profile. An SSRI guest links the kernel profile (verifier feature `ssri`
depends on `ckb-cinnabar-calculator` with default features off) and calls the
kernel calculator crate from the version table above.

`TransactionSkeleton::get_input_by_index`, `get_output_by_index`, and
`get_celldep_by_index` take a `usize`. Encode a Lua-style relative index as
`(-n) as usize` (`-1` is the last cell, `-2` the second-to-last).

## Layout first

Put args/data encode–decode in one place both crates use (`protocol/` or
`core/common/`, `no_std` + alloc). Calculator serializes; Verify parses into
`Context`. Constants that appear on both sides (windows, code hashes, type
ids) are explicit sync points.

The accepted tree names the serde plan ([confirm.md](confirm.md)
section 0b). **serde_molecule is the default.** Apply that plan before
writing recipes. If they named another plan, use only the crate and entry
points they gave. Do not add a second codec for the same bytes.

serde_molecule dependencies (`no_std` + alloc, so a kernel calculator and
the verifier can both link them):

```toml
serde = { version = "1", default-features = false, features = ["derive", "alloc"] }
serde_molecule = { version = "1.1", default-features = false, features = ["alloc"] }
```

```rust
use serde::{Deserialize, Serialize};
use serde_molecule::{from_slice, to_vec};

#[derive(Serialize, Deserialize)]
struct CellData {
    amount: u64,
}

// `false` = molecule table. `true` = molecule struct. Same flag on both calls.
let bytes = to_vec(&CellData { amount }, false).unwrap();
let parsed: CellData = from_slice(&bytes, false).unwrap();
```

`is_struct` is that second argument. The in-tree Spore schema passes
`false` (table). A molecule dynvec field uses
`#[serde(with = "serde_molecule::dynvec_serde")]`. Spore and Cluster cells
that already have `operation::spore::schema` stay on that schema.

On the Verify side of a generated project, fold every molecule failure into
one `define_errors!` code (`InvalidData` or the name on the tree). Do not
map `serde_molecule` error variants one by one. The same collapse applies
if they named another serde plan.

## Instruction pipelines

Default: `Instruction::new(ops)` — one function per user action, producing
exactly one cell of the transition table.

`Instruction::named(intent, ops)` is the morphology-scale shortcut when the
Verify hop **is** `intent::*`. Unnamed instructions are also correct for
prepare, balance, signing, and protocol-scale recipes.

Solid recipe + FakeRpc layout: https://github.com/Opticrum/ckb-contract-script
(`calculator/opticrum`, `tests`). Load that repo when the user’s actions are
a state machine, not a single Create/Transfer/Burn.

Typical stack:

```
[prepare deps] + [business ops] + [balance / signatures]
```

```rust
use ckb_cinnabar_calculator::{
    address::Address, instruction::Instruction,
    operation::basic::{AddInputCellByAddress, AddOutputCell}, rpc::RPC,
};

pub fn transfer<T: RPC>(from: Address, to: Address, capacity: u64) -> Instruction<T> {
    Instruction::new(vec![
        Box::new(AddInputCellByAddress { address: from }),
        Box::new(AddOutputCell {
            lock_script: to.into(),
            type_script: None,
            data: vec![],
            capacity,
            absolute_capacity: true,
            type_id: false,
        }),
    ])
}
```

If you are on the shortcut, swap `Instruction::new` for
`Instruction::named(intent::TRANSFER, …)`.

Capacity: `absolute_capacity: true` means `capacity` is final shannons;
`false` adds on top of occupied.

Live contract code cell: wrap `AddCellDepByTypeId` (name + type-id for the
network). Fake network: `ScriptEx::Reference("my_contract".into(), args)`
after `AddFakeContractCelldepByName`.

### Operations as Calculate modules

Basic (`operation::basic`): `AddInputCellByAddress`, `AddInputCellByOutPoint`,
`AddOutputCell`, `AddOutputCellByInputIndex`, `AddCellDep`,
`AddCellDepByTypeId`, `AddHeaderDepByBlockNumber`, `AddHeaderDepByInputIndex`,
`AddHeaderDepByCellDepIndex`, `AddSecp256k1SighashCellDep`,
`BalanceTransaction`, `AddSecp256k1SighashSignatures`.

DAO / xUDT: `operation::dao`, `operation::udt` (`AddXudtCelldep`).

If a step is domain-specific (build session molecule, set output witness,
attach this contract by type-id), write a small `Operation` — that is the
Calculate-side module. Compose them; do not dump a raw skeleton in the
recipe function.

Spore/Cluster **assembly** helpers: `operation::spore` behind `--features
spore` (experimental). Molecule bytes go through `serde_molecule` in
`operation::spore::schema`. Use them when the user’s cells are Spore/Cluster.
**VM execution** of those scripts still needs the binaries below.

Time: if Verify reads headers, the recipe must add the matching header deps.
Off-chain may convert APY → `u64` per block; the chain only sees the `u64`.

## When the recipe list is too large

One identity whose hops are Create / Transfer / Burn stays in
`calculator/src/lib.rs`, as the template does. Split when a second identity
appears, or when one file would hold recipes from more than one identity.
The cut is the verification tree’s identities, and **every** side uses that
same cut: Verify, Calculate, and tests.

Each identity module has the same shape. Read any one of them the same way:

1. Shared layout for this identity (`protocol/` or `core/common/`).
2. Custom `Operation`s this identity needs. The type and its `impl` stay in
   this file. Do not collect every operation into one file and every recipe
   into another.
3. One public recipe per hop of this identity. Each returns an `Instruction`
   of those operations. No other identity’s recipes.

`calculator/src/lib.rs` is only the index: `mod` plus `pub use` of the
recipes. No recipe bodies there. Its `//!` still states which identities
this crate assembles. Each identity file’s `//!`, and each recipe’s `///`,
state that identity’s purpose and what the recipe places. See the rustdoc
rule in [SKILL.md](SKILL.md).

```text
calculator/src/lib.rs          # pub mod issue; pub use issue::seal;
calculator/src/box_.rs         # seal, open, transfer, burn
calculator/src/collection.rs   # create_collection
contracts/<name>/src/main.rs   # Context, define_errors!, cinnabar_main!
contracts/<name>/src/box_.rs   # this identity’s Verify nodes
contracts/<name>/src/collection.rs
tests/src/box_.rs              # calls box_ recipes, then CKB-VM
tests/src/collection.rs
```

A module that needs a step from another identity calls that identity’s
recipe or operation. It does not copy the pipeline.

## Local simulation (calculator, then CKB-VM)

Every contract test does these two steps, in order. The transaction under
test is the one the **calculator** assembles. **CKB-VM** is what accepts or
rejects it.

1. **Seed** the FakeRpc universe (setup only, not the business transaction):
   - `always-success` for **user** locks (auth module: “hash in inputs”).
   - This contract’s RISC-V from `../build/release` (often with `type_id_args`).
   - Real RISC-V for foreign protocols Verify executes (see binaries).
   - `insert_fake_cell` / `insert_fake_header` so queries and header clocks exist.
2. **Calculate.** Call the project's recipe (`transfer(...)`, `mint(...)`, …).
   That `Instruction` builds the transaction. Do not rebuild the same inputs,
   outputs, data, or witnesses by hand in the test. A negative case still
   calls the recipe: pass a bad argument or seed a bad cell so the assembler
   emits a transaction the VM should reject.
3. **Verify in CKB-VM.** Pass the seed instructions plus that recipe to
   `assert_verify!` or `TransactionSimulator::async_verify`. Both run the
   instructions, then execute every script group in the native CKB-VM. The
   assertion is the VM exit code.

```rust
let rpc = FakeRpcClient::default();
let prepare = Instruction::new(vec![
    Box::new(AddFakeAlwaysSuccessCelldep {}),
    Box::new(AddFakeContractCelldepByName {
        contract: "my_lock".into(),
        type_id_args: Some(Default::default()),
        contract_binary_path: "../build/release".into(),
    }),
]);
// `transfer` is the calculator recipe. The test does not rebuild its cells.
let transfer_ix = my_lock_calculator::transfer::<FakeRpcClient>(from, to, capacity);
assert_verify!(&rpc, vec![prepare, transfer_ix], 0).unwrap();
```

`0` = VM success. Non-zero = `define_errors!` `i8`
(`assert_verify!(&rpc, vec![prepare, transfer_ix], 22)`).
`CalculatorError::ScriptValidation` → `script_exit_code()`.
`kind()` is stable (`script_validation`, `cell_dep_not_found`, …).

`assert_verify!` is sugar over assembling those instructions and
`TransactionSimulator`, which runs native `ckb-script` (CKB-VM).
`TransactionSimulator::skeleton` may hold cells the recipe expects already
placed; `async_verify` still runs the calculator instructions and then the
VM. A skeleton that already contains the business outputs is not a stand-in
for the recipe.

`instruction.run` may print a skeleton while debugging. It is not the
assertion. Do not call `verify()` from host Rust, and do not replace CKB-VM
with a mock. Always `make build` before loading `build/release/<crate>`.

Acceptance: after generate and after every change, run `make build` then
`make test` in the project root. Both must pass the **full** suite. A green
filtered `cargo test <name>` is not acceptance.

## Bundled protocol binaries (`binaries/`)

This skill ships RISC-V next to `SKILL.md`:

| File                 | Protocol                    |
| -------------------- | --------------------------- |
| `binaries/spore`     | Spore                       |
| `binaries/cluster`   | Cluster                     |
| `binaries/xudt`      | xUDT                        |
| `binaries/type_burn` | `ckb-proxy-locks` type-burn |

They were used to finish FakeRpc cases for **WarSporeSaga**, an on-chain game
built on Cinnabar. When the contract must compose with Spore / Cluster or
xUDT issuance (or type-burn lock proxies), load these as cell deps so the
native CKB-VM runs the **real** scripts before on-chain submit.

Prefer the production test layout (`tests/binaries/` + by-name load):

```bash
# skill root = directory that contains SKILL.md
mkdir -p tests/binaries
cp <skill-root>/binaries/{spore,cluster,xudt,type_burn} tests/binaries/
```

```rust
Box::new(AddFakeContractCelldepByName {
    contract: "spore".into(),
    type_id_args: None,
    contract_binary_path: "./binaries".into(),
})
```

Same names: `"cluster"`, `"xudt"`, `"type_burn"`. Reference from scripts via
`ScriptEx::Reference("spore".into(), args)`.

Alternatively `AddFakeContractCelldep { name, contract_data: fs::read(...),
type_id_args }` if you copy into `tests/fixtures/` instead.

Do not use always-success for these neighbors. Always-success is for **user**
locks only.

## Headless deploy

```bash
ckb-cinnabar --json --dry-run --privkey-env CINNABAR_PRIVKEY \
  deploy --contract-name my_lock --tag v0.1.0 --payer-address ckt1...
```

`--json`: one object on stdout; failures `ok: false`, empty stderr, non-zero
exit; script failures include `error.exit_code`.
`--dry-run`: no send, no `ckb-cli`.
`--privkey-env`: hex secp256k1; live send without it prompts `ckb-cli`.
Records: `deployment/<network>/<name>.json`.

Install CLI: `cargo install --git https://github.com/ashuralyk/ckb-cinnabar`.

Multi-crate workspaces often expose the same commands via
`ckb_cinnabar::dispatch()` in a root `runner` binary (`cargo run -- deploy …`).
