#![allow(dead_code)]

mod command;
mod handle;
mod object;

pub use ckb_cinnabar_calculator as calculator;
pub use handle::load_contract_deployment;
pub use object::DeploymentRecord;

/// Wrap for scripts-manager runner
pub fn dispatch() {
    tokio::runtime::Runtime::new()
        .expect("tokio runtime")
        .block_on(command::dispatch_commands())
        .expect("dispatch commands");
}
