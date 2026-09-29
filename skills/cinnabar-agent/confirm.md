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
2. Show the full pack in one message. Ask the version question (section 0)
   and the serde-plan question (section 0b) in that message. Then ask the
   rest, clustered — do not dump 40 unrelated questions. Walk **Verify →
   Calculate → tests**, and for each: shape first, then reachable scope,
   then edge cases still open.
3. After each cluster of answers, **reprint only the changed sections**.
4. Stop only on an explicit go-ahead (“按这个实现” / “implement as above” /
   checking the whole pack). “看起来行” / “ok” on a vague paragraph is not
   enough if assumptions remain.
5. If they refuse to decide, propose the **stricter on-chain** default, say
   so, and wait. Do not silently pick the loose default.
6. Scaffold (`cargo generate`) **after** this gate, unless they already have
   a repo and only asked for a design review. Section 0 (SSRI or non-SSRI)
   and section 0b (serde_molecule or their own plan) must both be explicit
   picks before that command.

## Version before generate (hard question)

Ask this in the first pack message, in their language. Do not `cargo generate`
while the answer is `open`, assumed, or silence.

> Before I generate the project, which version do you want?
>
> - **SSRI** — the calculator crate is kernel mode
>   (`default-features = false`). On-chain methods call those kernel
>   `Instruction`s.
> - **non-SSRI** — the calculator crate is shell mode (the default `std`
>   build). Verify is hop-only; tests and host recipes stay on the shell.

Chinese:

> 生成项目之前，你要哪一版？
>
> - **SSRI** — calculator 用 kernel 模式（`default-features = false`），链上方法调用这些 kernel `Instruction`。
> - **非 SSRI** — calculator 用默认 shell 模式。Verify 只有跳转；测试和宿主配方走 shell。

If they defer, propose **non-SSRI + shell**, say that is the default, and
wait. Do not generate on that proposal until they accept it.

## Serde plan before generate (hard question)

Ask this in the same first pack message, in their language. Do not
`cargo generate`, and do not start a layout crate, while the answer is
`open`, assumed, or silence.

The default plan is **serde_molecule**. Structured args, data, and witnesses
are `Serialize` / `Deserialize` types. Calculate calls
`serde_molecule::to_vec`. Verify calls `serde_molecule::from_slice`. The
second argument is `is_struct`: `false` maps the Rust struct to a molecule
**table** (the usual cell payload; extra fields can be tolerated on decode),
`true` maps it to a molecule **struct**. Pass the same value on both sides.
Field order is the molecule field order.

> Args, data, and witnesses need one serde plan on both sides. Which do you want?
>
> - **serde_molecule** (default) — shared `no_std` types, `to_vec` / `from_slice`.
> - **Your own** — name the crate and the encode/decode functions. Calculate and Verify use only that plan.

Chinese:

> args、data、witness 两边要用同一套 serde 方案。你选哪个？
>
> - **serde_molecule**（默认）— 共用 `no_std` 类型，`to_vec` / `from_slice`。
> - **你自己的方案** — 说出 crate 和编解码入口。Calculate 和 Verify 只用这一套。

If they defer, propose **serde_molecule**, say that is the default, and
wait. If they name their own plan, record the crate and both entry points
in section 0b before generating. A one-word “custom” is still `open`.

## Pack to show (required sections)

Copy this outline into the reply and fill it. Empty rows are not allowed —
write `n/a` and why, or `open` and the question.

### 0. Version (blocks generate)

| Choice | Calculator profile | Verify entry |
|--------|--------------------|--------------|
| SSRI / non-SSRI / `open` | kernel (`default-features = false`) or shell (default `std`) | `SSRI { }` wire table, or hop-only |

Fill this from their answer. `open` blocks `cargo generate`.

### 0b. Serde plan (blocks generate)

| Plan | Where it lives | Encode / decode |
|------|----------------|-----------------|
| serde_molecule / their crate / `open` | `protocol/` or `core/common/` (`no_std`) | `to_vec` / `from_slice`, or the entry points they named |

Fill this from their answer. `open` blocks `cargo generate`. One payload
does not get two codecs. Raw integers with no struct still get an answer:
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
  argv slots (`SsriArgs::bytes`; slot 0 is the method path), and
  `SsriSource` lookups (`find_out_point_by_type`, `find_cell_by_out_point`,
  `find_cell_data_by_out_point`).
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
