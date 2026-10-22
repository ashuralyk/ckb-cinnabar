# Contributing

Agent-facing instructions are [AGENTS.md](AGENTS.md). This file is for people
changing the framework itself.

## Build

```bash
rustup target add riscv64imac-unknown-none-elf
cargo fmt --all -- --check
cargo test --workspace --exclude ckb-cinnabar-verifier
cargo test -p ckb-cinnabar-calculator --features spore
cargo test -p ckb-cinnabar-calculator --no-default-features
cargo test -p ckb-cinnabar-verifier --features ssri
cargo check -p ckb-cinnabar-verifier --features ssri --target riscv64imac-unknown-none-elf
cargo check -p ckb-cinnabar-verifier --no-default-features --target riscv64imac-unknown-none-elf
cargo check --workspace --examples --exclude ckb-cinnabar-verifier
cargo check -p ckb-cinnabar --example spore --features spore
cargo clippy --workspace --all-targets -- -D warnings
make -C examples/scenarios
```

`TARGET_CC=clang` is required for the RISC-V `ckb-std` build. The contract
Makefile passes `-C passes=lower-atomic`.

Install the CLI from git: `cargo install --git https://github.com/ashuralyk/ckb-cinnabar`.

Regenerate the inlined agent bundle after editing `README.md`, `AGENTS.md`,
`docs/error-codes.md`, or this file, and commit the result. CI runs the same
generator and fails when `llms-full.txt` differs:

```bash
make llms-full
```

## Style

- Collapse sibling `use` items that share a prefix into one braced import.
  Inline type paths are at most two segments. The first segment is never
  `crate`. Longer paths belong in `use`.
- Rustdoc explains behavior. A comment that only repeats the item name does
  not count. Generated calculators and verifiers get `//!` on the crate and
  each identity module, and `///` on each recipe, custom operation,
  `Context`, and hop. An SSRI guest function’s rustdoc lists its arguments
  (index, meaning, converter or encoding) matching the `SsriArgs` reads.
  `argv[0]` is the method id.
- Prefer Chinese for product and strategy discussion. Implementation tasks
  may be in English.
- When a generated calculator covers more than one identity, split Verify,
  Calculate, and tests on that cut. Each identity module holds its operations
  and one recipe per hop. `calculator/src/lib.rs` only re-exports. A single
  Create/Transfer/Burn identity stays in one file.
- Skill users may state only a product demand. Interview one product-language
  question per message until every absent product detail has been said.
  “ok”, “you decide”, a batch, a sketched tree, or “this product is simple
  enough” leaves the interview open. Show the tree only when that list is
  empty, then wait. Shell versus kernel, serde, and FakeRpc are recorded on
  the tree, not asked: non-SSRI and `serde_molecule` unless a demand needs
  on-chain methods or they already named a codec and both entry points.
- `SKILL.md` ends with an after-generation checklist. Walk it after the
  project exists and after every later change.
- Keep `templates/`, `AGENTS.md`, and `README.md` aligned with current
  Calculate APIs. Do not add recipe-level `insert_index` or `relative_index`.
- Breaking public API, error types, and CLI output is acceptable when it
  makes the framework easier for an agent to discover and use. Keep
  agent-readiness work in this repository. Treat `cinnabar-examples` as
  reference only.

## Layout

Each layer is one complete runnable instance, packed in one tree and named
for what it is (`kernel`, `shell`). Adapters live outside the kernel tree.
Do not split a type from its impl, reimplement the same type per environment,
or wrap an already-layer-owned file in nested `mod` / `cfg` theater.

Calculate packs the `no_std` assembler into `calculate/src/kernel/` and the
`std` host into `calculate/src/shell/`. Crate-root `instruction` /
`operation` / `rpc` / `skeleton` alias the shell on `std` and the kernel on
`--no-default-features`. Application code does not name `kernel::` or
`shell::`. One `impl<C: RPC>` (or `S: Source` when only the three lookups
run) per kernel type. Do not XOR-compile a second `impl<T: RPC>` on the same
struct. Shell operation files only compile on `std`.

`Address` (ckb2021 bech32m) lives in the kernel. `Source` is a derivable
subset of `Node` + `Indexer`. `SsriSource` implements kernel `RPC` for
`network`, `get_live_cell`, headers, block hashes, and `get_cells`.

`TransactionSkeleton` getters accept Lua-style `-n` as `(-n) as usize`.
`resolve_get_index` returns `(Option<usize>, empty)` and callers own the
error strings.

`Indexer::get_cells` and `get_transactions` take `Order` (`Asc = 0`,
`Desc = 1`). Helpers and recipes keep requesting `Asc`.
`CellQueryOptions::order` is not copied onto `SearchKey`.

Experimental `--features spore` encodes with `serde_molecule` via
`kernel::operation::spore::schema`. `authority_mode` lives on
`AddClusterCelldepByClusterId`. Named spore recipes are shell-only.

`TYPE_ID_CODE_HASH` matches `h256!("0x545950455f4944")`: 25 leading zero
bytes, then ASCII `TYPE_ID`.

## SSRI

`cinnabar_main!`’s `SSRI { }` arm is verifier feature `ssri` (default). It
depends on git `https://github.com/ashuralyk/ckb-ssri-std` at `8c2c245`
(ckb-std 1.1). Empty `argv` is on-chain verify.
`argv` plus raw `vm_version == u64::MAX` is SSRI. Do not hand-write
`program_entry` for generated SSRI contracts.

`SsriArgs` keeps each slot as hex and decodes it when a reader runs.
`method_path()` reads `argv[0]` as the 8-byte method id. Argument `n` is
`argv[offset + n]`. `bytes(index, convert)` is
`FnOnce(&[u8]) -> Result<T, E>`. Built-in converters: `hash`, `script`,
`address`, `capacity`, `utf8`, `transaction`.

## References that are not delivery scope

- `cinnabar-examples` (upstream `ashuralyk/cinnabar-examples`).
- WarSporeSaga contracts and Opticrum’s public tree
  (https://github.com/Opticrum/ckb-contract-script).
- Local SSRI prototypes under a maintainer checkout (`ssri-udt`,
  `pausable-udt`). Use them as a pattern source only.

Protocol binaries for FakeRpc composition are downloaded, not committed.
See [skills/cinnabar-agent/binaries/README.md](skills/cinnabar-agent/binaries/README.md).
