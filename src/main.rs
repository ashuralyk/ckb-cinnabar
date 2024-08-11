#![allow(dead_code)]

mod command;
mod handle;
mod object;

#[tokio::main]
pub async fn main() -> ckb_cinnabar_calculator::re_exports::eyre::Result<()> {
    command::dispatch_commands().await
}
