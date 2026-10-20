Generated Cinnabar contract project. Agent instructions: see Cinnabar's `AGENTS.md`.

This template replaces the old `ckb-cinnabar-scripts-template` repository.
Dependencies are a path into a local checkout when `use_local_cinnabar` is
true (the prompt default, and what to use until `0.1.0` is on crates.io).
Otherwise they are `version = "<cinnabar_version>"`. The generated
`Cargo.toml` comments the git fallback for that case.

Cinnabar splits **Calculate** (off-chain `Instruction`s in `calculator/`) from
**Verify** (on-chain tree in `contracts/{{crate_name}}`). Name both sides with
the same `intent::*` constants (`create` / `transfer` / `burn` / …).

```bash
make prepare
make build
make test

# dry-run deploy
cargo run -- --json --dry-run deploy \
  --contract-name {{crate_name}} --tag v0.1.0 \
  --payer-address ckt1...
```

- `contracts/{{crate_name}}` — on-chain Verify tree (`cinnabar_main!`)
- `calculator` — off-chain Instructions named with `intent::*`
- `tests` — FakeRpc + `assert_verify!` (`0` = success)
- `build/release` — RISC-V binary (`ckb-cinnabar --contract-path` default)
- `deployment/` — JSON records written by `ckb-cinnabar`
