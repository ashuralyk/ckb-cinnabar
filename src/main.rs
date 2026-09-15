#![allow(dead_code)]

#[tokio::main]
pub async fn main() -> std::process::ExitCode {
    match ckb_cinnabar::dispatch_async().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => std::process::ExitCode::from(ckb_cinnabar::report_cli_error(&error)),
    }
}
