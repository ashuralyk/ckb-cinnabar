# ckb-cinnabar-mcp

Stdio [MCP](https://modelcontextprotocol.io) server for the Cinnabar CKB
contract framework. It is a scaffold: the tools call this repository’s
toolchain and return JSON. They do not replace `AGENTS.md`.

```bash
cargo run -p ckb-cinnabar-mcp
# or, after publish: cargo install ckb-cinnabar-mcp
```

Point the client at the `ckb-cinnabar-mcp` binary. Messages are LSP-style
`Content-Length` frames. A single JSON line is also accepted, which makes
local probes easy.

Set `CINNABAR_ROOT` to the framework checkout when the process is not started
inside it. `get_docs` and `scaffold_project` read that tree.

## Tools

| Tool | Role |
| --- | --- |
| `scaffold_project` | `cargo generate` command for `templates/contract`. `kind` is `lock`, `type`, or `contract`. |
| `build` | `make build` in a generated project. |
| `test` | `make test` in a generated project. |
| `simulate` | In-process FakeRpc run. `recipe` `always_success_transfer` returns `exit_code` and `cycles`. |
| `explain_error` | Script `i8` or a note that codes ≥ 20 are contract-defined. |
| `list_operations` | Calculate operation names agents compose by hand. |
| `list_recipes` | Named recipes in `instruction::predefined`. |
| `get_docs` | A topic from this repo (`agents`, `skill`, `errors`, `llms`, …). |
| `deploy_dry_run` | `ckb-cinnabar --json --dry-run deploy …`. |
| `list_deployments` | JSON files under a `deployment/` directory. |
| `query_cells` | Build a cell query. `execute: false` skips RPC. |

## Resources

`cinnabar://AGENTS.md`, `cinnabar://skills/cinnabar-agent/SKILL.md`,
`cinnabar://docs/error-codes.md`, `cinnabar://llms.txt`, and the example
sources under `cinnabar://examples/`.
