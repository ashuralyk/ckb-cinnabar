//! Stdio entry for `ckb-cinnabar-mcp`.
//!
//! One JSON-RPC message per LSP `Content-Length` frame. A bare JSON line is
//! also accepted.

use std::io::{self, Write};

use ckb_cinnabar_mcp::{frame, handle_message, read_frame};

fn main() {
    let stdin = io::stdin();
    let mut reader = io::BufReader::new(stdin.lock());
    let mut stdout = io::stdout();
    loop {
        match read_frame(&mut reader) {
            Ok(None) => break,
            Ok(Some(message)) => {
                if let Some(response) = handle_message(&message) {
                    let bytes = frame(&response);
                    if stdout.write_all(&bytes).is_err() {
                        break;
                    }
                    let _ = stdout.flush();
                }
            }
            Err(error) => {
                eprintln!("ckb-cinnabar-mcp: {error}");
                break;
            }
        }
    }
}
