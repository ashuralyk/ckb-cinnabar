# Protocol binaries

RISC-V scripts used when a FakeRpc test must execute Spore, Cluster, xUDT, or
the `ckb-proxy-locks` type-burn lock. They are not committed. From this
directory:

```bash
./download.sh
```

The script downloads the blobs from commit `cacde02d7fffa37ffcd5a853c9fd785679580850`
(the last revision that stored them in git) and checks `checksums.sha256`.

| File | Script |
| --- | --- |
| `spore` | Spore |
| `cluster` | Cluster |
| `xudt` | xUDT |
| `type_burn` | type-burn lock proxy |

Copy the checked files into a project’s `tests/binaries/` and load them with
`AddFakeContractCelldepByName`. See `skills/cinnabar-agent/calculate.md`.
