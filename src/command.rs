use ckb_cinnabar_calculator::re_exports::eyre;
use clap::{Parser, Subcommand};

use crate::handle::{consume_contract, deploy_contract, migrate_contract};

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// The network connect to, options are `mainnet`, `testnet`, `http://localhost:8114`
    #[arg(short, long, default_value_t = String::from("testnet"))]
    network: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Deploy a new contract to the CKB blockchain
    Deploy {
        /// Contract name in `build/release` directory
        #[arg(long)]
        contract_name: String,
        /// The version of the contract, which is used to distinguish different contract cells, e.g. `v0.1.8`
        #[arg(long)]
        tag: String,
        /// Who will pay the capacity and transaction fee
        #[arg(long)]
        payer_address: String,
        /// The owner that owns the contract cell, if None, payer will own instead
        #[arg(long)]
        owner_address: Option<String>,
        /// Whether to deploy contract with `type_id` enabled, which brings the seemless upgradibility
        #[arg(long, default_value_t = false)]
        type_id: bool,
        /// The path to store the deployed contract cell information
        #[arg(long, default_value_t = String::from("migration"))]
        migration_path: String,
    },
    /// Migrate an existed contract to a new version
    Migrate {
        /// The contract name that will be migrated
        #[arg(long)]
        contract_name: String,
        /// The previous deployed version that will be consumed and migrated to the new one
        #[arg(long)]
        from_tag: String,
        /// The version of the contract, which is used to distinguish different contract cells, e.g. `v0.1.8`
        #[arg(long)]
        to_tag: String,
        /// The payer address must be the same as the previous owner address
        #[arg(long)]
        payer_address: String,
        /// The new owner address of that migrated contract cell
        #[arg(long)]
        owner_address: Option<String>,
        /// The mode that how to handle the `type_id` of the contract cell, options are `keep`, `remove`, `new`
        #[arg(long, default_value_t = String::from("keep"))]
        type_id_mode: String,
        /// The path to store the deployed contract cell information
        #[arg(long, default_value_t = String::from("migration"))]
        migration_path: String,
    },
    /// Consume a contract cell to release the capacity
    Consume {
        /// The contract name that will be consumed
        #[arg(long)]
        contract_name: String,
        /// The version of the contract, which is used to distinguish different contract cells, e.g. `v0.1.8`
        #[arg(long)]
        tag: String,
        /// The payer address that will pay the transaction fee
        #[arg(long)]
        payer_address: String,
        /// The receiver address that will receive the released capacity, if None, payer will receive instead
        #[arg(long)]
        receive_address: Option<String>,
        /// The path to store the transaction cache file
        #[arg(long, default_value_t = String::from("migration"))]
        migration_path: String,
    },
}

pub async fn dispatch_commands() -> eyre::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Deploy {
            contract_name,
            tag,
            payer_address,
            owner_address,
            type_id,
            migration_path,
        } => {
            deploy_contract(
                cli.network,
                contract_name,
                tag,
                payer_address.parse().expect("payer address"),
                owner_address.map(|s| s.parse().expect("owner address")),
                type_id,
                migration_path,
            )
            .await
        }
        Commands::Migrate {
            contract_name,
            from_tag,
            to_tag,
            payer_address,
            owner_address,
            type_id_mode,
            migration_path,
        } => {
            migrate_contract(
                cli.network,
                contract_name,
                from_tag,
                to_tag,
                payer_address.parse().expect("payer address"),
                owner_address.map(|s| s.parse().expect("owner address")),
                type_id_mode,
                migration_path,
            )
            .await
        }
        Commands::Consume {
            contract_name,
            tag,
            payer_address,
            receive_address,
            migration_path,
        } => {
            consume_contract(
                cli.network,
                contract_name,
                tag,
                payer_address.parse().expect("payer address"),
                receive_address.map(|s| s.parse().expect("owner address")),
                migration_path,
            )
            .await
        }
    }
}
