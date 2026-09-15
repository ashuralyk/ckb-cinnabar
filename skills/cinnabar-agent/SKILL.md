---
name: cinnabar-agent
description: Generate Nervos CKB contract projects with Cinnabar (Calculate + Verify). Use when creating a CKB lock/type script, assembling transactions, simulating with FakeRpc, or deploying with ckb-cinnabar.
---

# Cinnabar agent skill

Read workspace [AGENTS.md](../../AGENTS.md) first. Follow that golden path; do not invent a Capsule/`deployment.toml` flow.

## Recipe

1. `cargo generate --path templates/contract --name <project>`
2. Edit `contracts/<crate>/src/main.rs`: `define_errors!`, `Verification` nodes named with `ckb_cinnabar_verifier::intent::*`.
3. Edit `calculator/src/lib.rs`: `Instruction::named(intent::..., ops)` using the **same** intent strings.
4. `make prepare && make build && make test` — tests call `assert_verify!(&rpc, instructions, expected_i8)`.
5. Deploy without a TTY: `ckb-cinnabar --json --dry-run --privkey-env CINNABAR_PRIVKEY deploy ...`

## Do

- Pair Calculate instruction names with Verify node names via `intent::{CREATE,TRANSFER,BURN,MINT,DEPOSIT,WITHDRAW}`.
- Use `CalculatorError::kind()` / CLI `--json` instead of scraping log lines.
- For script failures, branch on `CalculatorError::script_exit_code()`; CLI JSON
  includes `error.kind`, `error.message`, `error.exit_code`, and a non-zero
  process status.
- Load binaries from `build/release` (CLI default `--contract-path`).

## Don't

- Invent a second set of node name strings.
- Call interactive `ckb-cli` in automation; use `--privkey-env` or `--dry-run`.
- Enable `--features spore` unless the user asked; it is experimental after the ckb-types-1 upgrade.
- Hand-write RISC-V linker scripts; the template Makefile from ckb-script-templates is the build.
