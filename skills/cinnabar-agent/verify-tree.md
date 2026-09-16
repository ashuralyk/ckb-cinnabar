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

| Kind | Examples |
|------|----------|
| Identity | “this lock is an order”, “args[0] means session”, type-id unique cell |
| Payload | molecule / packed args+data, amounts, unoccupied capacity |
| Actor | buyer / seller / owner / issuer — stored as `lock_hash` in args |
| Time | input `since`, header number/timestamp on the cell or tip |
| Neighbor | Spore, Cluster, xUDT, type-burn, Fiber-like channel in CellDeps |
| Predicate | conservation, args frozen, DNA, rent formula, window elapsed |

Do not start from user verbs. Verbs become **recipes** after the relation
table exists.

Show this split in the confirmation pack ([confirm.md](confirm.md)) and wait.
The catalog there is the bar for “have we thought about it”.

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
  `core/common/` when more than one crate needs them. Dual-written constants
  (block windows, code hashes) stay in sync on purpose.

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

## Predicate catalog

| Requirement | Typical check | Module |
|-------------|----------------|--------|
| Only owner can spend | owner `lock_hash` in inputs | Actor |
| Anyone after time | `since` or `tip - cell_header` vs args | Time |
| Cannot change rules | input args == output args | Identity frozen |
| One cell in, one out | `this_script_count` | Morphology |
| New unique cell | Create; type-id args = `calc_type_id(index)` | Identity |
| Supply conserved | sum amounts in Context | Payload |
| Only issuer mints | issuer lock in inputs | Actor |
| Cannot destroy | Burn → error | Transition table |
| Data well-formed | decode into Context | Payload |
| Foreign asset/NFT | load type/lock; code hash in allow-list | Neighbor |
| Linear rent / windows | header number × rate; compare capacity | Time + payload |

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
  type group runs; headers linked to cells

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
`script_exit_code() == Some(22)`.
