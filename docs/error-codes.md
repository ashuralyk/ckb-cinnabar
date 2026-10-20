# Error codes

On-chain scripts return an `i8`. `define_errors!` starts custom codes at
`CUSTOM_ERROR_START` (20) so they do not collide with system or framework
codes. `assert_verify!`’s third argument is this `i8`. Assembly failures that
never reach the VM use `CalculatorError::kind()` instead.

## System (ckb-std `SysError`)

| Code | `Error` | Meaning |
| --- | --- | --- |
| 1 | `IndexOutOfBound` | A syscall index was out of range. |
| 2 | `ItemMissing` | The requested item (cell, witness, header, …) is absent. |
| 3 | `LengthNotEnough` | A buffer was shorter than the syscall needed. |
| 4 | `Encoding` | Molecule or syscall bytes did not decode. |
| 5 | `UnknownSystemError` | Any other `SysError`. |

## Verify tree

| Code | `Error` | Meaning |
| --- | --- | --- |
| 10 | `NotFoundRootVerifier` | No node was registered under `"root"` (`TREE_ROOT`). |
| 11 | `NotFoundBranchVerifier` | A node returned a next name that is not registered, or the walk cycled. |

## SSRI

Used when verifier feature `ssri` is on (the default) and the run is an SSRI
call (`argv` set and `vm_version == u64::MAX`). A hop-only script that never
enters that arm does not return these.

| Code | `Error` | Meaning |
| --- | --- | --- |
| 12 | `SSRIMethodsNotFound` | `argv[0]` is not in the `SSRI { }` table. |
| 13 | `SSRIMethodsArgsInvalid` | A method argument failed to decode. |
| 14 | `SSRIMethodsNotImplemented` | The method id is known but this contract does not implement it. |
| 15 | `SSRIMethodRequireHigherLevel` | The host execution level is too low. |
| 16 | `InvalidVmVersion` | `argv` is set but `vm_version` is not `u64::MAX`. |
| 17 | `SSRIAssembleFailed` | A kernel `Instruction` failed while serving the method. |
| 18 | `SSRISourceUnavailable` | A guest `Source` lookup failed. |

## Custom

| Code | Meaning |
| --- | --- |
| ≥ 20 | Declared with `define_errors!`. The first variant should be `= CUSTOM_ERROR_START`. Later variants increment by one. The contract’s own rustdoc or error enum is the lookup table. |

## Assembly (`CalculatorError::kind()`)

These strings are the library classifier. They are not script exit codes.
A failed script is `script_validation` plus `script_exit_code()`. The CLI
section below says when `--json` uses one of these strings.

| `kind()` | When |
| --- | --- |
| `cell_dep_not_found` | A cell dep could not be located. |
| `input_cell_not_found` | No live input matched the query. |
| `output_cell_not_found` | An output index is missing. |
| `insufficient_capacity` | Inputs cannot cover outputs and the fee. |
| `no_available_cells` | The indexer returned nothing usable. |
| `network` | RPC or transport failure. |
| `signing` | Key or ckb-cli signing failure. |
| `simulation` | Skeleton or VM failure without a script exit code. |
| `script_validation` | The script returned a non-zero `i8` (`exit_code`). |
| `fake_rpc` | Misuse of `FakeRpcClient`. |
| `io` | Filesystem failure (binary, deployment record). |
| `source_unavailable` | `Source` is unset or the lookup is not implemented. |
| `other` | Anything else. |

## CLI JSON (`--json`)

`error.kind` is chosen in this order:

1. A typed `CalculatorError` uses the table above. `script_validation` also sets `error.exit_code` to the script `i8`.
2. A `std::io::Error` is `io` (missing contract binary, unreadable deployment file). `load_contract_binary` returns `CalculatorError::Io`, so a missing `build/release/<name>` is `io` by rule 1.
3. CLI-only failures, which are not in the table above:
   - `configuration` — for example `--privkey-env` names a variable that is unset. Process exit `1`.
   - `invalid_input` — bad flag or unparseable address. Process exit `2`.
4. Any other report is passed through `CalculatorError::from_message`. HTTP transport (`bad ckb request`, connection refused, a JSON-RPC body the node rejected) becomes `network`. Text that is a missing file becomes `io`. Anything left is `other`.

`--dry-run` still contacts the node indexer to find the payer's live cells. Success puts the deployment record in the JSON field `record`. The file `deployment/<network>/<name>.json` is written only when the transaction is sent.

## FAQ

**The script exited 11.** A `verify` returned `Ok(Some(name))` and `name` is
not registered in `cinnabar_main!`, or the walk visited that name twice.

**The script exited 4.** A molecule decode failed inside a syscall. Generated
contracts should map their own codec failures to one custom code such as
`InvalidData` (20 or above), not to code 4.

**`kind` is `script_validation` but I parsed the display string.** Read
`error.exit_code` in `--json` output, or `CalculatorError::script_exit_code()`.

**`kind` is `configuration` or `invalid_input`.** The process never reached
assembly. Fix the flag or the environment variable. These two strings are
CLI-only.
