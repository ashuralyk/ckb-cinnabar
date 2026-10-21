# Scenario contracts

Two small locks exercised with FakeRpc and the native CKB-VM. They are not
part of the framework workspace. From this directory:

```bash
make prepare   # once: rustup target add riscv64imac-unknown-none-elf
make           # build both RISC-V binaries, then cargo test
```

| Contract | What it checks |
| --- | --- |
| `timelock` | Lock args are an 8-byte little-endian unix second. Spending is allowed only when every input in the lock group carries an absolute timestamp `since` (RFC 0017) at or after that second. A valid `since` on the first input does not excuse `since` 0 on another cell of the same lock (`SinceNotMet`, exit 21). |
| `threshold_multisig` | Lock args are `threshold \|\| count \|\| blake2b-256 hashes`. The matching witness `lock` field is 32-byte preimages. At least `threshold` distinct preimages must hit the hash list. |

This multisig is a preimage threshold so the VM test does not need secp256k1.
The genesis secp256k1 multisig witness prefix
(`0x00 | require_first_n | threshold | pubkey_count | blake160 * N`) is
covered by the host test `calculate/tests/scenarios.rs`.

xUDT mint and transfer are calculator recipes (`mint_xudt`, `transfer_xudt`),
not a script in this folder. `calculate/tests/scenarios.rs` checks those
intent names without a binary.
