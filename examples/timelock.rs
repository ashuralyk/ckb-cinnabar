//! Print an absolute unix-time `since` for the timelock example.
//!
//! The value is the RFC 0017 encoding checked by `examples/scenarios`
//! (`contracts/timelock`). Bit 63 stays clear. The timestamp metric is
//! `0x4000_0000_0000_0000`. The low 56 bits are the unix second.
//!
//! ```text
//! cargo run --example timelock
//! ```

use ckb_cinnabar_calculator::scenario::absolute_timestamp_since;

fn main() {
    let unlock_at = 1_700_000_000u64;
    let since = absolute_timestamp_since(unlock_at);
    println!("unlock_at unix seconds: {unlock_at}");
    println!("input since: 0x{since:016x}");
}
