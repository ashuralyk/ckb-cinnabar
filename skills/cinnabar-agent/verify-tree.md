# Split rules into modules, then a Verify tree

Read this when designing on-chain nodes. Keep `SKILL.md` as the workflow.

Cinnabar’s original method: **minimal modules → relations between them →
shared logic with outputs folded into `Context`**. The tree is that method
made executable. It is a **flowchart** (each node once), not an RPC router
and not a mandatory `intent::*` list.

## Walk semantics

`cinnabar_main!` starts at `TREE_ROOT`. Each node is visited **once** (removed
after run). Returning `Ok(Some(name))` hops to a still-registered node.
Returning a name twice, or a cycle, yields framework error 11.

## What a module is

Split **CKB physics** and **domain rules** into the smallest pieces that can
fail or be reused:

| Kind      | Examples                                                              |
| --------- | --------------------------------------------------------------------- |
| Identity  | “this lock is an order”, “args[0] means session”, type-id unique cell |
| Payload   | molecule / packed args+data, amounts, unoccupied capacity             |
| Actor     | buyer / seller / owner / issuer — stored as `lock_hash` in args       |
| Time      | input `since`, header number/timestamp on the cell or tip             |
| Neighbor  | Spore, Cluster, xUDT, type-burn, Fiber-like channel in CellDeps       |
| Predicate | conservation, args frozen, DNA, rent formula, window elapsed          |

Do not start from user verbs. Verbs become **recipes** after the relation
table exists.

Compose this tree only after the ingredients in [confirm.md](confirm.md) are
enough, then show the tree and wait. The catalog there is the agent’s bar
for “have we thought about it”, not a list to read to the user.

## Relations

A relation is a **legal change of one identity’s instance**, plus the
neighbors that change must see.

```
instance = (place, args, data, type?, capacity)
tx       = (this script in inputs?, in outputs?, other cells, deps, headers, witnesses)

classify(old, new, neighbors, auth) → hop_name | error
```

CKB `ScriptPattern::Create | Transfer | Burn` only answers “is this script in
inputs and/or outputs?”. Use it as an **input** to classification, not as the
business name, when identity is richer than “one role”.

**Lock Create does not execute this script.** If the cell only appears in
outputs, Root never runs. Creation is then someone else’s lock (the payer)
plus a recipe that writes the right args/data. Either omit a Create hop or
document that it is off-script.

**Auth relation (default):** store `lock_hash` in args; predicate is “that
hash appears on some input lock”. Signature checking is that lock’s job.

**Neighbor relation:** if a predicate `load`s another script’s cell, that
script is a module. Tests must give it a real binary (or a mock that matches
the on-chain identification rule).

Write the table explicitly:

```
old identity (+ payload) → new identity (+ payload) → hop
anything else            → Err(Unknown / forbidden)
```

Same morphology, two businesses → one hop that **internally** branches on
Context (auth who is present, or whether a neighbor exists). That is still
one relation with two predicate paths, not a missing intent string.

## Shared logic → Context

If a computation is used by more than one predicate, or is expensive (cell
load, occupied capacity, decode), it is shared logic. If it has a **result**,
that result is a `Context` field.

Rules:

- `#[derive(Default)]` is required.
- **Root (or the first hop) fills Context** — parse args/data, compute
  unoccupied capacity, detect xUDT, load headers you will need, set auth
  flags.
- Later nodes **read Context**; they do not reload the same cells.
- Off-chain uses the **same byte types**. Put them in `protocol/` /
  `core/common/` when more than one crate needs them. Decode with the serde
  plan recorded on the accepted tree. The default is **serde_molecule**
  (`from_slice` into Context). Their own plan uses the entry point they
  named. Any decode failure returns the project's single molecule
  `define_errors!` code. Dual-written constants (block windows, code hashes)
  stay in sync on purpose.

```rust
#[derive(Default)]
struct Context {
    old: Option<ParsedCell>,
    new: Option<ParsedCell>,
    unoccupied_in: u64,
    unoccupied_out: u64,
    // header clocks, auth flags, neighbor handles, decoded amounts…
}
```

Context is how the method **lowers complexity**: classification and I/O live
in one place; the tree stays a list of cheap predicates.

## Root classifies (and does I/O)

Root is not “forbidden from business”. Root **must not hide predicates** that
need their own exit codes, but it **should** parse and classify.

Morphology-scale (one role, hop = Create/Transfer/Burn):

```rust
impl Verification<Context> for Root {
    fn verify(&mut self, _name: &str, _ctx: &mut Context) -> Result<Option<&str>> {
        match this_script_pattern(ScriptPlace::Lock)? {
            ScriptPattern::Create => Ok(Some(intent::CREATE)),
            ScriptPattern::Transfer => Ok(Some(intent::TRANSFER)),
            ScriptPattern::Burn => Ok(Some(intent::BURN)),
        }
    }
}
```

Use `ScriptPlace::Type` when this crate is a type script. Forbidden
morphology → `Err`, not a hop.

Protocol-scale: parse identity from args (flag, length, …), parse old/new
payloads into Context, then hop to a **domain** name from the transition
table. `this_script_pattern` may still be the first split when the lock has
one place and several modes after Transfer.

One RISC-V binary may be Lock **and** Type: first args byte (or similar)
selects the module; then input/output counts select the transition.

## When to add a child node

Split when the check:

- can fail independently (own exit code), or
- is reused from two hops, or
- needs a name you can point to in a test (`assert_verify!(…, 21)`).

Keep it inside the hop when it is one `if` on data already in `Context`.

Child names are `'static` (`check_since`, `order_match`). Do not reuse
`create` / `transfer` / … for children unless you are on the morphology
shortcut and that string **is** the hop.

```rust
cinnabar_main!(
    Context,
    (TREE_ROOT, Root),
    (intent::TRANSFER, Transfer),
    ("check_since", CheckSince),
    ("check_owner", CheckOwner),
);
```

## Optional SSRI door

Add this only when section 0 of the accepted tree is **SSRI**
([confirm.md](confirm.md)). That choice also puts `calculator/` in kernel
mode ([calculate.md](calculate.md)); guest wrappers call those kernel
`Instruction`s. Feature `ssri` is the verifier default. The **non-SSRI**
choice stays hop-only and does not use this section.

Empty `argv` runs the verify tree (`should_fallback`). `argv` together with
raw `vm_version == u64::MAX` runs the `SSRI { }` block. `ssri_methods!`
always emits `SSRI.version`, `SSRI.get_methods`, and `SSRI.has_methods`.
The block lists the contract's methods. Write that table in `cinnabar_main!`.
Each name is a string literal in the block (`"UDT.mint"`). `cinnabar_main!`
expands `program_entry`, `should_fallback`, and `ssri_methods!`.

Each right-hand side is an expression `export` turns into bytes: a guest
function, `&[u8]`, `u8`, `Vec<u8>`, or `Result`. Hop `verify()` stays on the
tree. `SsriSource` implements kernel `RPC` for `network`, `get_live_cell`,
`get_header`, `get_header_by_number`, `get_block_hash`,
`get_transaction_block_hash`, and `get_cells`. `get_tip_header`,
`get_tip_block_number`, `min_fee_rate`, and `get_transactions` return
`SourceUnavailable`. `Source` lookups still come from that `RPC`. `SsriArgs` holds
hex-decoded slots. Slot 0 is the method path. Later slots follow that
method's own definition; read them with `SsriArgs::bytes(index)`.

The method body is a guest wrapper. It decodes `SsriArgs` and runs a kernel
`Instruction` (verifier feature `ssri` depends on the calculator kernel).
A `std` host recipe is a shell type and stays on the host.

```rust
use alloc::vec::Vec;
use ckb_cinnabar_verifier::{
    ssri::{SsriArgs, SsriSource},
    Result,
};

fn mint(_source: &SsriSource, args: SsriArgs) -> Result<Vec<u8>> {
    let _to = args.bytes(1)?;
    let _amount = args.bytes(2)?;
    // assemble with a kernel Instruction, then return wire bytes
    Ok(Vec::new())
}

cinnabar_main!(
    Context,
    (TREE_ROOT, Root),
    (intent::MINT, Mint),
    SSRI {
        "UDT.name" => "Example",
        "UDT.decimals" => 8u8,
        "UDT.mint" => mint,
    },
);
```

## Predicate catalog

| Requirement           | Typical check                                | Module           |
| --------------------- | -------------------------------------------- | ---------------- |
| Only owner can spend  | owner `lock_hash` in inputs                  | Actor            |
| Anyone after time     | `since` or `tip - cell_header` vs args       | Time             |
| Cannot change rules   | input args == output args                    | Identity frozen  |
| One cell in, one out  | `this_script_count`                          | Morphology       |
| New unique cell       | Create; type-id args = `calc_type_id(index)` | Identity         |
| Supply conserved      | sum amounts in Context                       | Payload          |
| Only issuer mints     | issuer lock in inputs                        | Actor            |
| Cannot destroy        | Burn → error                                 | Transition table |
| Data well-formed      | decode into Context                          | Payload          |
| Foreign asset/NFT     | load type/lock; code hash in allow-list      | Neighbor         |
| Linear rent / windows | header number × rate; compare capacity       | Time + payload   |

Load with `ckb_std::high_level`. Helpers: `this_script_args`,
`this_script_indices`, `this_script_count`, `this_script_pattern`,
`calc_type_id`.

On-chain: `u64` / fixed bytes. Do not use `f64` as consensus.

## Worked split: time-lock (morphology-scale)

User: “CKB 锁仓，到期任何人可取；到期前只有 owner 能取消并拿回。”

- Place: **Lock**
- Identity: one role; args = owner hash + unlock point
- Transitions: Create (payer’s lock runs, this script may not); Transfer =
  spend; Burn forbidden or treated as spend-to-owner
- Context: parsed args, optional header/`since`
- Predicates: args frozen; `since` ≥ unlock **or** owner lock in inputs
- Recipes: `create_lock` / `unlock` / `cancel` — `Instruction::named` with
  `intent::*` is appropriate here

## Worked split: issuer-minted token (morphology-scale type)

User: “只有 issuer 能增发；转账数量守恒；不能销毁。”

- Place: **Type**
- Transitions: Create → `intent::MINT`; Transfer → `TRANSFER`; Burn →
  `Err(CannotBurn)`
- Context: `in_amount` / `out_amount` (shared conservation logic)
- Recipes: `mint_xudt` / `transfer_xudt` or custom named instructions

## Worked split: two identities, one lock (protocol-scale)

User: “先挂单，再成交变成持仓；持仓上抽租或加仓；抽空后销毁。可选 xUDT。”

- Place: **Lock** (identity in args length or a flag; type optional for xUDT)
- Modules: Order identity, Position identity, buyer/seller actors, header
  clock, optional xUDT neighbor, optional channel celldep
- Relations (example):

```
Order  → ∅        → cancel          (buyer lock in inputs)
Order  → Position → match           (seller lock; neighbor checks)
Position → Position → update        (internal: seller vs buyer)
Position → ∅      → destroy         (exhausted + seller)
Create Order      → no Verify hop   (lock only in outputs)
```

- Context: parsed old/new, unoccupied capacity, xUDT pair, header-derived
  elapsed
- Recipes: `Instruction::new` per row. Calculator does not re-check rent.
- Tests: always-success user locks; this contract binary; xUDT binary if that
  type group runs; headers linked to cells. Each case calls the calculator
  recipe, then CKB-VM checks that transaction.

The same pattern covers a game global + session in one binary: args select
identity, input/output counts select create/update/settle, Context holds
decoded molecule and header nonce.

Complete source for this scale: fetch
https://github.com/Opticrum/ckb-contract-script (`contracts/opticrum`,
`opticrum-protocol`, `calculator/opticrum`, `tests`). Pattern only — do not
paste their marketplace rules.

## Error budget

```rust
define_errors!(TokenError, {
    CannotBurn = CUSTOM_ERROR_START, // 20
    NotIssuer,                       // 21
    AmountMismatch,                  // 22
});
```

Match tests with `assert_verify!(&rpc, ixs, 22)` or
`script_exit_code() == Some(22)`. `ixs` includes the calculator recipe;
CKB-VM runs the transaction that recipe assembled.

Molecule decode and encode failures (`serde_molecule` or the plan they
named) share **one** custom code per generated project, for example
`InvalidData`. A short buffer, a bad table header, and a type mismatch all
return that same `i8`. Do not declare one `define_errors!` variant per
molecule error. Business failures (wrong issuer, bad amount) stay their own
codes.

SSRI framework codes (feature `ssri`):

| Code | `Error` |
| ---- | ------- |
| 12 | `SSRIMethodsNotFound` |
| 13 | `SSRIMethodsArgsInvalid` |
| 14 | `SSRIMethodsNotImplemented` |
| 15 | `SSRIMethodRequireHigherLevel` |
| 16 | `InvalidVmVersion` |
| 17 | `SSRIAssembleFailed` |
| 18 | `SSRISourceUnavailable` |
