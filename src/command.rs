use ckb_cinnabar_calculator::re_exports::eyre;
use clap::{Parser, Subcommand};

use crate::handle::{consume_contract, deploy_contract, migrate_contract};

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// CKB network, options are `mainnet`, `testnet` or URL (e.g. http://localhost:8114)
    #[arg(short, long, default_value_t = String::from("testnet"))]
    network: String,

    /// Directory of the contract deployment information
    #[arg(long, default_value_t = String::from("deployment"))]
    deployment_path: String,

    /// Directory of the compiled contract binary
    #[arg(long, default_value_t = String::from("build/release"))]
    contract_path: String,

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
        payer_address: String,
        /// Who owns the contract cell, if None, <payer_address> will be in charge
        #[arg(long)]
        contract_owner_address: Option<String>,
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
        /// Who onws the new contract cell, if None, previous contract owner of <from_tag> will be in charge
        #[arg(long)]
        contract_owner_address: Option<String>,
        /// How to process the `type_id` of migrated contract, operation is `keep`, `remove` or `new`
        #[arg(long, default_value_t = String::from("keep"))]
        type_id_mode: String,
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
        receiver_address: Option<String>,
    },
}

/// Parse and dispatch commands
pub async fn dispatch_commands() -> eyre::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Deploy {
            contract_name,
            tag,
            payer_address,
            contract_owner_address,
            type_id,
        } => {
            deploy_contract(
                cli.network,
                contract_name,
                tag,
                payer_address,
                contract_owner_address,
                type_id,
                cli.deployment_path,
                cli.contract_path,
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
                cli.network,
                contract_name,
                from_tag,
                to_tag,
                contract_owner_address,
                type_id_mode,
                cli.deployment_path,
                cli.contract_path,
            )
            .await
        }
        Commands::Consume {
            contract_name,
            tag,
            receiver_address,
        } => {
            consume_contract(
                cli.network,
                contract_name,
                tag,
                receiver_address,
                cli.deployment_path,
            )
            .await
        }
    }
}
