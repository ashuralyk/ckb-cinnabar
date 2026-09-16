# Confirm the split before writing code

Read this while executing the **hard gate** in `SKILL.md`. Do not implement
Verify, Calculate, or tests until the user explicitly accepts the pack (or a
revised pack after Q&A).

The point is not a short summary. It is to **surface every module, relation,
and non-goal**, then **interactively** fix Verify / Calculate / FakeRpc
**shape** and **reachable scope** with the user.

If they write in Chinese, run this dialogue in Chinese.

If the split looks protocol-scale (several identities, domain hops) and you
need a complete example of tree + recipes + tests, fetch
https://github.com/Opticrum/ckb-contract-script before filling the pack.
Use it as structure, not as the user’s product.

## How to interact

1. Draft the pack below from the user’s words **and** from CKB physics they
   did not mention (Create-does-not-run-lock, extra cells, missing headers,
   both/neither auth, CKB vs xUDT, …). Mark guesses as **assumption**.
2. Show the full pack in one message. Then ask, clustered — do not dump 40
   unrelated questions. Walk **Verify → Calculate → tests**, and for each:
   shape first, then reachable scope, then edge cases still open.
3. After each cluster of answers, **reprint only the changed sections**.
4. Stop only on an explicit go-ahead (“按这个实现” / “implement as above” /
   checking the whole pack). “看起来行” / “ok” on a vague paragraph is not
   enough if assumptions remain.
5. If they refuse to decide, propose the **stricter on-chain** default, say
   so, and wait. Do not silently pick the loose default.
6. Scaffold (`cargo generate`) **after** this gate, unless they already have
   a repo and only asked for a design review.

## Pack to show (required sections)

Copy this outline into the reply and fill it. Empty rows are not allowed —
write `n/a` and why, or `open` and the question.

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
- `define_errors!` draft (names, not necessarily numbers yet).
- **Out of Verify (will not check on-chain):** e.g. UX strings, APY text,
  display DNA, Fiber multiaddr in witness.

### 5. Verify — reachable scope

What the RISC-V script **can** see this tx (group input/output, celldeps,
headers, witnesses) vs what it **cannot** (other txs, off-chain DB, future
blocks except via `since`/header deps you actually attach).

State which morphologies and identities Verify **never runs** (typical: lock
Create).

### 6. Calculate — shape

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

- Universe: always-success user locks; this binary; **which** foreign
  binaries from this skill’s `binaries/` (or named mocks and why).
- Seeded cells and headers (block numbers, linking cells to headers).
- Happy-path tests (one per recipe that Verify actually runs).
- Failure tests (`script_exit_code` per important `i8`).
- Skeleton/witness assertions if Calculate puts metadata off-script.
- `assert_verify!` vs `TransactionSimulator` + `new_skeleton`.

### 9. Simulation — reachable scope

| In FakeRpc VM | Not in this test suite (say why) |
|---------------|----------------------------------|
| This contract + listed neighbors | e.g. real Fiber node, live indexer, `ckb-debugger` decoder, secp signatures if using always-success |

Be explicit when a neighbor is a **hash mock** (must match a constant in the
contract) rather than a bundled RISC-V file.

### 10. Open questions

Numbered. Each maps to a row above. Do not implement while this list is
non-empty unless the user defers a numbered item in writing.

## Situation catalog (probe these; do not skip silently)

Use this as a checklist while filling the pack. For every item: **on-chain /
off-chain assemble / FakeRpc / none**. If “none”, that is a reachable-scope
decision the user must see.

### Cell physics

- This script Lock, Type, or both in one binary.
- 0, 1, or many cells of this script in inputs and in outputs (1-1, 1-n, n-1,
  n-m, create, burn).
- Extra unrelated cells in the same tx (change, fee, donations).
- Type script present vs absent (CKB-only vs xUDT or other typed asset).
- Capacity: occupied vs unoccupied; shrinking/growing; type-id extra occupied.
- Args/data: too short, too long, unknown discriminator, frozen field mutated.
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

- Every legal hop has a success tx Verify will actually run.
- Every important `Err` has a tx that should fail with that `i8`.
- Illegal look-alikes in the transition table have at least one negative test.
- Foreign protocol groups that Verify executes are in the universe.
- What you will **not** test (signatures, live Fiber, mainnet binaries) is
  written in section 9.

## After confirmation

Implement **only** what the pack allows. If coding reveals a new case, stop
and reopen the pack — do not silently widen Verify, Calculate, or test
scope.

After generate and after every implementation change: `make build` and
`make test` (all cases) must exit 0. That is project acceptance. Do not
treat the pack as delivered while either command fails.
