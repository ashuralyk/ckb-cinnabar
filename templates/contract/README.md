Generated Cinnabar contract project. Agent instructions: see Cinnabar's `AGENTS.md`.

This template replaces the deprecated `ckb-cinnabar-scripts-template` repository (treat that repo as archived).
Dependencies are git dependencies on
`https://github.com/ashuralyk/ckb-cinnabar` unless `use_local_cinnabar` is
true. Local checkouts must pass an absolute `cinnabar_path` (the directory
that contains `calculate/` and `verify/`).

Cinnabar splits **Calculate** (off-chain `Instruction`s in `calculator/`) from
**Verify** (on-chain tree in `contracts/{{crate_name}}`). Name both sides with
the same `intent::*` constants (`create` / `transfer` / `burn` / …).

```bash
make prepare
make build
make test

# dry-run deploy (flags before the subcommand; needs a reachable indexer)
cargo run -- --json --dry-run deploy \
  --contract-name {{crate_name}} --tag v0.1.0 \
  --payer-address ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsq28phxutezqvjgfv5q38gn5kwek4m9km3cmajeqs
```

`--dry-run` prints `record` in the JSON object and does not write
`deployment/`. A real send writes `deployment/<network>/<name>.json`.

- `contracts/{{crate_name}}` — on-chain Verify tree (`cinnabar_main!`)
- `calculator` — off-chain Instructions named with `intent::*`
- `tests` — FakeRpc + `assert_verify!` (`0` = success)
- `build/release` — RISC-V binary (`ckb-cinnabar --contract-path` default)
- `deployment/` — JSON records written by `ckb-cinnabar`
