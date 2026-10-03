# Interview, then the verification tree

Read this while executing the **hard gate** in `SKILL.md`. Skill users may
only have a product sentence. They are not expected to know the cell model
or a script formula.

Do not implement Verify, Calculate, or tests until they explicitly accept
the verification tree (or a revised tree after a new ingredient).

If they write in Chinese, run this dialogue in Chinese.

If the product looks protocol-scale (several identities, domain hops) and you
need a complete example of tree + recipes + tests, fetch
https://github.com/Opticrum/ckb-contract-script before composing the tree.
Use it as structure, not as the user’s product. Do not show that example
instead of the interview.

## Hard gate

Do not head to the verification tree until the user has emptied every absent
product detail, in product language, out of their own mouth.

<HARD-GATE>
One product question per message. Wait for the answer. Then ask the next
absent detail. Do not show the tree, the worksheet, SSRI, serde, Calculate,
or tests in that message. Do not scaffold. This applies even when the
product sounds small.
</HARD-GATE>

A detail is emptied only when they said it, or when they said yes to a
stricter rule you proposed in product language. A guess you could write
onto the tree is still absent. Silence does not close it.

## Anti-pattern: "I can already draw the tree"

Every product goes through this interview. A one-line lock, a blind box, a
token — all of them. "Simple" is where an unspoken rule becomes the wrong
node. You do not get to skip ahead because the first sentence was clear
enough to sketch hops.

These thoughts mean stop. You are rationalizing your way to the tree:

| Thought | Reality |
| --- | --- |
| "I can already sketch the tree" | The missing rules are still in your head, not theirs. Ask. |
| "This product is too simple to interview" | Simple products hide the rule that changes a node. |
| "I'll mark it assumption and show the tree" | An assumption is an absent detail. Stay in the interview. |
| "A batch of questions is faster" | One question. Their answer reveals the next absence. |
| "They said you decide" | Propose the stricter rule in their words and wait for yes. |
| "CKB physics covers it, no need to ask" | If it changes what the product does, ask in product language. |
| "They said ok" | "ok" did not state the detail. Ask the next absence. |

## How to ask

Keep a private list of absent product details. Each message pulls exactly
one, in their language. Prefer a multiple-choice question when the answers
are a few real product options; otherwise one open question. Do not dump
the situation catalog. Do not ask them to choose Lock versus Type, hop
strings, molecule, SSRI, or FakeRpc.

Translate the gap into the product. "Who is allowed to write the first
args" is "who is allowed to issue the first box, and does anyone else get
to". Ask that. Do not teach the cell model in order to get the answer.

If they will not decide, offer the **stricter** rule as the recommended
choice ("only you can issue a box; anyone holding it can open it") and
wait. Do not pick the loose rule for them.

Stop only when the private list is empty: the ingredient rows below are
closed by their words, and a pass over the situation catalog finds no
remaining product rule they have not stated. Then, and only then, compose
the tree and show it.

Framework choices with no product fork stay yours: shell versus kernel,
serde_molecule, FakeRpc, exit codes. State any of those that the user
would notice, in their words, on the tree.

## Ingredients (each absence is one question)

Each row is closed only by their answer or by their yes to your proposal.
Walk whatever is still absent, one question at a time. Do not close a row
by inference.

| Ingredient | You need | Still open, so keep asking |
| --- | --- | --- |
| Things | The distinct objects (a sealed box, a revealed artwork, a collection) | “Art” with no count, no sealed-versus-open |
| Actors | Who may create, change, open, or destroy each thing | “Users” with no maker versus holder |
| Stories | Legal changes, and changes that must fail | A verb (“issue”) with no forbidden cases |
| Checks | What must be true on-chain for each story (who pays, uniqueness, hidden until open, creator frozen) | A story with no pass/fail rule |
| Off-chain | Pictures, shop text, fiat prices, anything that must not be a check | Not stated, and a node might otherwise store it |
| Neighbors | Other on-chain things a story must read (a payment coin, an existing art format) | “Issue art” that might be a new script or Spore, and they have not accepted one |

If a cell-model fact changes the product (who may create the first item,
what happens when two of them move together, what is rejected), it is an
absent product detail. Ask it in their words before the tree. Mechanics
that do not change the product wait until you compose.

## Show the verification tree

When every ingredient is closed, show the tree in their language and stop.
Example shape, for a blind box — use their product:

```
Root
  → issue — only the maker can seal a box; the artwork stays hidden
  → open — the holder can reveal that box once; the art matches what was sealed
  → transfer — the holder can give a sealed or revealed box away
  → burn — the holder can destroy it
Never checked here: the picture file, the shop price
Rejected: opening someone else's box, revealing twice, the maker rewriting a sealed box
```

Under it, the implementable nodes: place, `cinnabar_main!` hop names,
Context fields, and one error name per node that can fail on its own. Hop
names are their words (`open`, `issue`), or `intent::*` when a hop is exactly
Create, Transfer, or Burn of this script.

One footer sentence, not a quiz: ordinary shell assembler and
serde_molecule, unless an ingredient needs other on-chain callers to invoke
methods (then SSRI and a kernel calculator) or they already named a codec
and both entry points. Accepting the tree accepts the footer.

They confirm with “按这个实现”, “implement as above”, or an explicit yes to
this tree. “看起来行” / “ok” while a node is still marked **assumption** is
not acceptance. If their reply opens an ingredient, go back to the interview
and show a revised tree only when it is closed again.

Calculate recipes and FakeRpc cases are derived from the accepted tree. Do
not open a second interview about them. Scaffold after this yes, unless they
already have a repo and only asked for a design review.

## Version and serde (you record them, you do not ask)

Fill section 0 and section 0b when you compose the tree. Do not
`cargo generate` before they accept the tree.

| Their demands | What you record |
| --- | --- |
| No on-chain methods | **non-SSRI**. Shell calculator. Hop-only `cinnabar_main!`. |
| Other on-chain callers must invoke methods on this contract | **SSRI**. Kernel calculator (`default-features = false`). `SSRI { }` arm; guest wrappers call those kernel `Instruction`s. |
| They named no codec | **serde_molecule**. `Serialize` / `Deserialize`. Calculate calls `serde_molecule::to_vec`. Verify calls `serde_molecule::from_slice`. |
| They named a crate and both entry points | That plan only. A one-word “custom” leaves the codec ingredient open: ask which crate and which functions, in their language. |

serde_molecule’s second argument is `is_struct`: `false` maps the Rust
struct to a molecule **table** (the usual cell payload; extra fields can be
tolerated on decode), `true` maps it to a molecule **struct**. Pass the same
value on both sides. Field order is the molecule field order.

## Worksheet (do not show this during the interview)

Copy this outline and fill it while composing. Empty rows are not allowed —
write `n/a` and why, or go back to the interview. The user sees the tree,
not this worksheet.

### 0. Version (you record it)

| Choice | Calculator profile | Verify entry |
|--------|--------------------|--------------|
| SSRI / non-SSRI | kernel (`default-features = false`) or shell (default `std`) | `SSRI { }` wire table, or hop-only |

Fill this from the version table above. `open` means the on-chain-methods
ingredient is still open: go back to the interview. Do not show the tree.

### 0b. Serde plan (you record it)

| Plan | Where it lives | Encode / decode |
|------|----------------|-----------------|
| serde_molecule / their crate | `protocol/` or `core/common/` (`no_std`) | `to_vec` / `from_slice`, or the entry points they named |

Fill this from the version table above. `open` means they said “custom”
without a crate and both entry points: ask, do not show the tree. One
payload does not get two codecs. Raw integers with no struct still get
serde_molecule for any later struct, or their named plan. Spore / Cluster
cells keep `operation::spore::schema`; do not re-encode those bytes.

### 1. Modules

| Module | Kind | Encoding / where it lives | Notes |
|--------|------|---------------------------|-------|
| … | identity / payload / actor / time / neighbor / predicate | args / data / header / celldep / witness | … |

### 2. Relations (transition table)

| Old | New | Neighbors / auth / time | Hop | Illegal look-alikes (must `Err`) |
|-----|-----|-------------------------|-----|----------------------------------|
| … | … | … | … | … |

Include **Create**. If this is a lock and Create does not execute, say
“no Verify hop; payer lock + recipe only”.

### 3. Context

Fields Root (or first hop) will fill; which predicates read them; what is
**not** in Context (one-shot `if`).

### 4. Verify — shape

- Place: Lock / Type / one binary both (discriminator).
- Root: morphology-only vs parse-then-domain hops.
- Hop list (`cinnabar_main!`) and child predicates (own `i8` or inlined).
- Auth rule (which `lock_hash`, both/neither/wrong party).
- Time rule (`since` vs header; exact boundary).
- Frozen vs mutable args/data fields.
- Neighbor identification (code hash / type-id / cluster id).
- `define_errors!` draft (names, not necessarily numbers yet). All molecule
  / serde failures share **one** custom code for this project. Do not list
  one code per molecule error variant.
- SSRI follows section 0. **non-SSRI:** hop-only, no `SSRI { }` arm.
  **SSRI:** wire names in `cinnabar_main!`'s `SSRI { }` arm (string literals),
  each RHS (`&[u8]` / `u8` / guest fn that calls a kernel `Instruction`),
  argv slots (`method_path()` is argv[0]; `bytes(index, convert)` reads
  `argv[offset + index]`, offset defaults to 0), and
  `SsriSource` as kernel `RPC` (`network`, `get_live_cell`, headers, block
  hashes, `get_cells`; tip, fee, and `get_transactions` are unavailable).
- **Out of Verify (will not check on-chain):** e.g. UX strings, APY text,
  display DNA, Fiber multiaddr in witness.

### 5. Verify — reachable scope

What the RISC-V script **can** see this tx (group input/output, celldeps,
headers, witnesses) vs what it **cannot** (other txs, off-chain DB, future
blocks except via `since`/header deps you actually attach).

State which morphologies and identities Verify **never runs** (typical: lock
Create).

### 6. Calculate — shape

- Profile from section 0. **SSRI:** kernel (`default-features = false`,
  `#![no_std]` calculator; guest wrappers call those recipes). **non-SSRI:**
  shell (default `std`).
- Serde plan from section 0b. **serde_molecule** (default): `to_vec` here,
  `from_slice` in Verify. **Their plan:** only the entry points they named.
- One recipe function per user action; which table row it realizes.
- `Instruction::new` vs `named(intent::*)`.
- Custom `Operation`s vs basic ones.
- Header deps / witnesses / type-id celldep / xUDT celldep.
- Normalization (e.g. zero xUDT amount on CKB-only path).
- Signing / balance / change.
- Scan/read helpers or CLI bins, if any.
- **Out of Calculate:** no duplicated predicates; human units convert here.

### 7. Calculate — reachable scope

Fake network vs testnet vs mainnet: how the contract celldep is resolved
(`Reference` name vs `AddCellDepByTypeId`). What recipes cannot do without a
live indexer/node (search by type, Fiber channel discovery).

### 8. Simulation — shape

Each case calls a calculator recipe, then CKB-VM runs that transaction.
`assert_verify!` or `TransactionSimulator::async_verify` is the runner.
The test seeds the universe; it does not rebuild the business transaction.

- Universe: always-success user locks; this binary; **which** foreign
  binaries from this skill’s `binaries/` (or named mocks and why).
- Seeded cells and headers (block numbers, linking cells to headers).
- Happy path: one VM success (`0`) per recipe that Verify actually runs.
  The instruction is the calculator function, not an inlined copy of its ops.
- Failure: same recipe with a bad argument or a bad seeded cell, then the
  VM `i8` via `script_exit_code`. Do not hand-build a parallel tx in `tests/`.
- Skeleton/witness checks only as extras on that same VM run, when Calculate
  puts metadata off-script. They do not replace the VM exit code.

### 9. Simulation — reachable scope

| In FakeRpc VM | Not in this test suite (say why) |
|---------------|----------------------------------|
| This contract + listed neighbors | e.g. real Fiber node, live indexer, `ckb-debugger` decoder, secp signatures if using always-success |

Be explicit when a neighbor is a **hash mock** (must match a constant in the
contract) rather than a bundled RISC-V file.

### 10. Open questions

Numbered. Each maps to an ingredient or a worksheet row. A non-empty list
means the ingredients are not enough: keep asking, do not show the tree.
Do not implement while this list is non-empty unless they accept your
stricter proposal for that item in writing.

## Situation catalog (yours; do not read it out)

Use this after the ingredient rows, still inside the interview, to find
product details they have not said. For every item: **on-chain / off-chain
assemble / FakeRpc / none**. If the item is a product rule they have not
stated, it is one more question, in product language, before the tree.
“none” is still their line to speak (“the picture is never checked”).
Framework-only items (FakeRpc, exit codes, codec) you resolve while
composing.

### Cell physics

- This script Lock, Type, or both in one binary.
- 0, 1, or many cells of this script in inputs and in outputs (1-1, 1-n, n-1,
  n-m, create, burn).
- Extra unrelated cells in the same tx (change, fee, donations).
- Type script present vs absent (CKB-only vs xUDT or other typed asset).
- Capacity: occupied vs unoccupied; shrinking/growing; type-id extra occupied.
- Args/data: too short, too long, unknown discriminator, frozen field mutated.
  Codec is section 0b (serde_molecule by default, or the plan they named).
- Lock Create does not run this script — who is allowed to write the first
  args/data, and is that a problem?

### Actors and auth

- Each actor’s `lock_hash` in args vs inferred from input.
- Exactly one required signer; two required; either-or; owner override.
- Both parties present; neither present; wrong party; replay with another
  lock that happens to be in the tx.
- Test always-success vs production secp (Verify must not assume the test
  lock).

### Time

- `since` vs cell producing header vs tip header vs extra header deps.
- Before / **equal** / after the threshold (off-by-one).
- Missing header dep; header not linked to the cell in FakeRpc.
- First update vs later updates (clock anchor moves or not).

### Neighbors and composition

- Spore / Cluster / xUDT / type-burn / DAO / other: execute real script or
  only check code hash?
- Missing celldep; wrong args; burned or spent neighbor; lock-proxy vs
  cluster cell.
- Bundled skill binaries vs network type-id vs test-only mock hash.
- Optional neighbor (feature flag) vs mandatory.

### Money and conservation

- CKB unoccupied vs xUDT amount vs both; mixing them by mistake.
- Mint / burn / transfer conservation; issuer-only mint.
- Partial withdraw vs all-or-nothing; dust / occupied floor.

### Calculate-only vs Verify-only

- Human APY, names, multiaddrs, render DNA: Calculate or witness metadata,
  not RISC-V (unless the user wants them on-chain — challenge that).
- Indexer search, “latest cell”, channel discovery: Calculate + RPC, not
  Verify.
- Game/engine simulation off-chain vs settlement on-chain.

### Tests

- Every case: calculator recipe builds the transaction, then CKB-VM
  (`assert_verify!` / `TransactionSimulator`) runs it. Seed ops only in the test.
- Every legal hop has a success tx (`0`) that Verify will actually run.
- Every important `Err` has a calculator-built tx that fails with that `i8`.
- Illegal look-alikes in the transition table have at least one negative test,
  still produced by the recipe (bad argument or bad seeded cell).
- Foreign protocol groups that Verify executes are in the universe.
- What you will **not** test (signatures, live Fiber, mainnet binaries) is
  written in section 9.

## After confirmation

Implement **only** what the accepted tree allows. Derive Calculate and tests
from it. If coding reveals a new product case, stop, reopen the interview,
and show a revised tree — do not silently widen Verify, Calculate, or test
scope.

After generate and after every implementation change: `make build` and
`make test` (all cases) must exit 0. That is project acceptance. Do not
treat the tree as delivered while either command fails.
