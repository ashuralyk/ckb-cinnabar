# Cinnabar

Follow [AGENTS.md](../AGENTS.md) when generating or editing CKB contracts in this repository.

Calculate assembles transactions off-chain (`Instruction` of `Operation`s). Verify checks them on-chain (`cinnabar_main!`). Scaffold from `templates/contract`. The old `ckb-cinnabar-scripts-template` repository is not the path.

Hard accept: `make build` and the full `make test` both exit 0. Deploy with `ckb-cinnabar --json --dry-run`. Do not invent Capsule or `deployment.toml`.

Error codes: [docs/error-codes.md](../docs/error-codes.md). Maintainer conventions: [CONTRIBUTING.md](../CONTRIBUTING.md).
