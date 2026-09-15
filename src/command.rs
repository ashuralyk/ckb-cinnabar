//! CLI argument parsing and command dispatch for `ckb-cinnabar`.
//!
//! Global flags (`--json`, `--dry-run`, `--privkey-env`) apply to every
//! subcommand. With `--json`, [`report_error`] prints a structured
//! [`CliResponse`] on stdout so agents can branch on `error.kind` /
//! `error.exit_code` without scraping logs.

use ckb_cinnabar_calculator::{
    address::Address,
    re_exports::{eyre, secp256k1::SecretKey},
    rpc::Network,
};
use clap::{error::ErrorKind, Parser, Subcommand};

use crate::{
    handle::{consume_contract, deploy_contract, list_contracts, migrate_contract, print_response},
    object::{CliError, CliResponse, ExecOpts, ListMode, TypeIdMode},
};

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
/// `ckb-cinnabar` CLI: deploy / migrate / consume / list contract cells.
pub struct Cli {
    /// CKB network, options are `mainnet`, `testnet` or URL (e.g. http://localhost:8114)
    #[arg(short, long, default_value_t = Network::Testnet)]
    network: Network,

    /// Directory of the contract deployment information
    #[arg(long, default_value_t = String::from("deployment"))]
    deployment_path: String,

    /// Directory of the compiled contract binary
    #[arg(long, default_value_t = String::from("build/release"))]
    contract_path: String,

    /// Print a JSON [`crate::object::CliResponse`] instead of a one-line hash.
    /// Agents and scripts should always pass this.
    #[arg(long, default_value_t = false)]
    json: bool,

    /// Assemble (and optionally sign) without sending.
    /// Also skips interactive `ckb-cli` when no `--privkey-env` is set.
    #[arg(long, default_value_t = false)]
    dry_run: bool,

    /// Environment variable holding a hex secp256k1 private key (non-interactive signing)
    #[arg(long)]
    privkey_env: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Upload contract to CKB
    Deploy {
        /// Contract that will be deployed
        #[arg(long)]
        contract_name: String,
        /// Version of the contract that used to distinguish different contracts, e.g. `v0.1.8`
        #[arg(long)]
        tag: String,
        /// Who pays the capacity and transaction fee
        #[arg(long)]
        payer_address: Address,
        /// Who owns the contract cell, if None, <payer_address> will be in charge
        #[arg(long)]
        contract_owner_address: Option<Address>,
        /// Whether to deploy contract with `type_id`
        #[arg(long, default_value_t = false)]
        type_id: bool,
    },
    /// Update on-chain contract from old version to new version
    Migrate {
        /// Contract that will be migrated
        #[arg(long)]
        contract_name: String,
        /// Previous deployed contract version
        #[arg(long)]
        from_tag: String,
        /// New contract version
        #[arg(long)]
        to_tag: String,
        /// Who owns the new contract cell, if None, previous contract owner of <from_tag> will be in charge
        #[arg(long)]
        contract_owner_address: Option<Address>,
        /// How to process the `type_id` of migrated contract, operation is `keep`, `remove` or `new`
        #[arg(long, default_value_t = TypeIdMode::Keep)]
        type_id_mode: TypeIdMode,
    },
    /// Consume on-chain contract to release the capacity
    Consume {
        /// Contract that will be consumed
        #[arg(long)]
        contract_name: String,
        /// Version of the consuming contract
        #[arg(long)]
        tag: String,
        /// Who receives the released capacity, if None, previous contract owner of <tag> will be in charge
        #[arg(long)]
        receiver_address: Option<Address>,
    },
    /// List deployment records from `--deployment-path`
    List {
        /// Optional contract name filter
        #[arg(long)]
        contract_name: Option<String>,
        /// `all`, `deployed`, or `consumed`
        #[arg(long, default_value_t = ListMode::All)]
        mode: ListMode,
    },
}

fn secret_key_from_env(var: &str) -> Result<SecretKey, CliError> {
    let raw = std::env::var(var)
        .map_err(|_| CliError::Configuration(format!("environment variable {var} not set")))?;
    let raw = raw.trim().trim_start_matches("0x");
    raw.parse()
        .map_err(|e| CliError::InvalidInput(format!("invalid secret key in {var}: {e}")))
}

fn operation_from_args(args: &[String]) -> Option<&str> {
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if matches!(arg.as_str(), "deploy" | "migrate" | "consume" | "list") {
            return Some(arg);
        }
        if matches!(
            arg.as_str(),
            "-n" | "--network" | "--deployment-path" | "--contract-path" | "--privkey-env"
        ) {
            index += 2;
        } else {
            index += 1;
        }
    }
    None
}

/// Print a CLI failure and return the process exit code (`2` invalid input, `1` otherwise).
///
/// When `--json` is present, writes a [`CliResponse`] (`ok: false`) to stdout
/// and keeps stderr empty so agents can parse a single envelope.
pub fn report_error(error: &eyre::Report) -> u8 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|arg| arg == "--json");
    let dry_run = args.iter().any(|arg| arg == "--dry-run");
    let operation = operation_from_args(&args).unwrap_or("unknown");

    if json {
        print_response(true, &CliResponse::error(operation, dry_run, error));
    } else {
        eprintln!("Error: {error:#}");
    }

    match error.downcast_ref::<CliError>() {
        Some(CliError::InvalidInput(_)) => 2,
        _ => 1,
    }
}

/// Parse and dispatch commands
pub async fn dispatch_commands() -> eyre::Result<()> {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            error.print()?;
            return Ok(());
        }
        Err(error) => return Err(CliError::InvalidInput(error.to_string()).into()),
    };
    let privkey = match cli.privkey_env.as_deref() {
        Some(var) => Some(secret_key_from_env(var)?),
        None => None,
    };
    let opts = ExecOpts {
        network: cli.network,
        deployment_path: cli.deployment_path,
        contract_path: cli.contract_path,
        json: cli.json,
        dry_run: cli.dry_run,
        privkey,
    };
    match cli.command {
        Commands::Deploy {
            contract_name,
            tag,
            payer_address,
            contract_owner_address,
            type_id,
        } => {
            deploy_contract(
                opts,
                contract_name,
                tag,
                payer_address,
                contract_owner_address,
                type_id,
            )
            .await
        }
        Commands::Migrate {
            contract_name,
            from_tag,
            to_tag,
            contract_owner_address,
            type_id_mode,
        } => {
            migrate_contract(
                opts,
                contract_name,
                from_tag,
                to_tag,
                contract_owner_address,
                type_id_mode,
            )
            .await
        }
        Commands::Consume {
            contract_name,
            tag,
            receiver_address,
        } => consume_contract(opts, contract_name, tag, receiver_address).await,
        Commands::List {
            contract_name,
            mode,
        } => list_contracts(opts, contract_name, mode).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_json_dry_run_list() {
        let cli = Cli::try_parse_from([
            "ckb-cinnabar",
            "--json",
            "--dry-run",
            "list",
            "--contract-name",
            "demo",
        ])
        .expect("parse");
        assert!(cli.json);
        assert!(cli.dry_run);
        match cli.command {
            Commands::List { contract_name, .. } => {
                assert_eq!(contract_name.as_deref(), Some("demo"));
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn operation_name_ignores_global_option_values() {
        let args = [
            "--deployment-path",
            "list",
            "--json",
            "deploy",
            "--contract-name",
            "demo",
        ]
        .map(str::to_string);

        assert_eq!(operation_from_args(&args), Some("deploy"));
    }
}
