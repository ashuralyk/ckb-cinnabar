#!/usr/bin/env python3
"""Regenerate llms-full.txt from the agent-facing docs.

Sources, in order: a short Core API header kept in this script, then
README.md, AGENTS.md, docs/error-codes.md, and CONTRIBUTING.md.
Run `make llms-full` after editing any of those files.
"""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

HEADER = """# Cinnabar — full agent context

This file inlines the repository docs an agent needs to scaffold, verify, calculate, simulate, and deploy a CKB contract. Prefer `llms.txt` when the context window is small. Context7 indexing is configured in `context7.json`.

## Core API

Install the CLI with `cargo install --git https://github.com/ashuralyk/ckb-cinnabar`. Contract projects depend on the same git repository, or on a local path when `use_local_cinnabar` is true and `cinnabar_path` is the absolute checkout root.

- `ckb_cinnabar_core::intent` — shared hop names (`CREATE`, `TRANSFER`, `BURN`, `MINT`, `DEPOSIT`, `WITHDRAW`). Use them only when the hop is exactly that morphology.
- `cinnabar_main!(Context, (TREE_ROOT, Root), ...)` — registers the on-chain tree. Optional trailing `SSRI { "Wire.name" => expr }` needs verifier feature `ssri` (on by default).
- `define_errors!(Name, { First = CUSTOM_ERROR_START, Second, })` — custom `i8` codes from 20.
- `Instruction::new` / `Instruction::named` — a pipeline of `Operation`s. Import `instruction`, `operation`, `rpc`, `skeleton` from the calculator crate root.
- `assert_verify!(&rpc, instructions, expected_exit)` — FakeRpc + native CKB-VM. `0` is success.
- `ckb-cinnabar --json --dry-run deploy` — global flags before the subcommand. Dry-run still needs an indexer. The JSON field `record` is the deployment record; `deployment/<network>/<name>.json` is written only when the transaction is sent.
- Script exits: sys 1–5, tree 10–11, SSRI 12–18, custom ≥ 20. `CalculatorError::kind()` is the library string. CLI JSON also uses `configuration` and `invalid_input`. See `docs/error-codes.md`.

Target: `riscv64imac-unknown-none-elf`, `clang` 16+ (the Dockerfile installs clang 18), `RUSTFLAGS` include `-C passes=lower-atomic`.
"""

SOURCES = [
    "README.md",
    "AGENTS.md",
    "docs/error-codes.md",
    "CONTRIBUTING.md",
]


def main() -> None:
    parts = [HEADER.rstrip() + "\n\n"]
    for rel in SOURCES:
        body = (ROOT / rel).read_text()
        if not body.endswith("\n"):
            body += "\n"
        parts.append(f"# {rel}\n\n")
        parts.append(body)
        if not body.endswith("\n\n"):
            parts.append("\n")
    text = "".join(parts)
    if not text.endswith("\n"):
        text += "\n"
    (ROOT / "llms-full.txt").write_text(text)


if __name__ == "__main__":
    main()
