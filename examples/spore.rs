use std::sync::Arc;

use ckb_cinnabar_calculator::{
    instruction::predefined::{
        balance_and_sign_with_ckb_cli, burn_spores, mint_clusters, mint_spores, transfer_clusters,
        transfer_spores, Cluster, Spore,
    },
    re_exports::{ckb_sdk::Address, ckb_types::H256, tokio},
    rpc::RpcClient,
    TransactionCalculator,
};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Operations on minting, transferring or burning a Spore
    Spore(SporeCli),
    /// Operations on minting or transferring a Cluster
    Cluster(ClusterCli),
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct SporeCli {
    #[command(subcommand)]
    pub command: SporeCommands,
}

#[derive(Subcommand)]
pub enum SporeCommands {
    Mint {
        /// The address to mint Spore
        #[arg(long, value_name = "address")]
        minter: Address,
        /// The content type of the Spore
        #[arg(long, value_name = "string")]
        content_type: String,
        /// The content of the Spore (UTF8 or HEX format)
        #[arg(long, value_name = "string or hex")]
        content: String,
        /// The cluster id of the Spore
        #[arg(long, value_name = "h256")]
        cluster_id: Option<String>,
    },
    Transfer {
        /// The unique id of the Spore to transfer
        #[arg(long, value_name = "h256")]
        spore_id: String,
        /// The address to send Spore
        #[arg(long, value_name = "address")]
        from: Address,
        /// The address to receive Spore
        #[arg(long, value_name = "address")]
        to: Address,
    },
    Burn {
        /// The Spore to burn
        #[arg(long, value_name = "h256")]
        spore_id: String,
        /// The address to burn Spore
        #[arg(long, value_name = "address")]
        owner: Address,
    },
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct ClusterCli {
    #[command(subcommand)]
    pub command: ClusterCommands,
}

#[derive(Subcommand)]
pub enum ClusterCommands {
    Mint {
        /// The address to mint Cluster
        #[arg(long, value_name = "address")]
        minter: Address,
        /// The cluster name
        #[arg(long, value_name = "string")]
        cluster_name: String,
        /// The cluster description (UTF8 or HEX format)
        #[arg(long, value_name = "string or hex")]
        cluster_description: String,
    },
    Transfer {
        /// The uniqie id of the Cluster to transfer
        #[arg(long, value_name = "hex")]
        cluster_id: String,
        /// The address to send Cluster
        #[arg(long, value_name = "address")]
        from: Address,
        /// The address to receive Cluster
        #[arg(long, value_name = "address")]
        to: Address,
    },
}

fn h256(value: String) -> H256 {
    let bytes32: [u8; 32] = hex::decode(value.trim_start_matches("0x"))
        .expect("hex")
        .try_into()
        .expect("h256");
    bytes32.into()
}

fn bytify(value: String) -> Vec<u8> {
    if let Ok(value) = hex::decode(value.trim_start_matches("0x")) {
        value
    } else {
        value.into_bytes()
    }
}

#[tokio::main]
pub async fn main() {
    let cmd = Cli::parse();
    let (spore, signer) = match cmd.command {
        Commands::Spore(spore) => match spore.command {
            SporeCommands::Mint {
                minter,
                content_type,
                content,
                cluster_id,
            } => {
                let spore = Spore {
                    owner: None,
                    content_type,
                    content: bytify(content),
                    cluster_id: cluster_id.map(h256),
                };
                let mint = mint_spores(
                    &minter,
                    vec![spore],
                    false,
                    Some(Arc::new(|spore_id| {
                        println!("Spore id: {:#x}", spore_id);
                    })),
                );
                (mint, minter)
            }
            SporeCommands::Transfer { spore_id, from, to } => {
                let transfer = transfer_spores(&from, vec![(to, h256(spore_id))]);
                (transfer, from)
            }
            SporeCommands::Burn { spore_id, owner } => {
                let burn = burn_spores(&owner, vec![h256(spore_id)]);
                (burn, owner)
            }
        },
        Commands::Cluster(cluster) => match cluster.command {
            ClusterCommands::Mint {
                minter,
                cluster_name,
                cluster_description,
            } => {
                let cluster = Cluster {
                    owner: None,
                    cluster_name,
                    cluster_description: bytify(cluster_description),
                };
                let mint = mint_clusters(
                    &minter,
                    vec![cluster],
                    Some(Arc::new(|cluster_id| {
                        println!("Cluster id: {:#x}", cluster_id);
                    })),
                );
                (mint, minter)
            }
            ClusterCommands::Transfer {
                cluster_id,
                from,
                to,
            } => {
                let transfer = transfer_clusters(&from, vec![(to, h256(cluster_id))]);
                (transfer, from)
            }
        },
    };

    // build transaction
    let rpc = RpcClient::new_testnet();
    let mut calculator = TransactionCalculator::default();
    calculator
        .instruction(spore)
        .instruction(balance_and_sign_with_ckb_cli(&signer, 2000, None));
    let skeleton = calculator.new_skeleton(&rpc).await.expect("calculate");

    // send transaction without any block confirmations
    let hash = skeleton.send_and_wait(&rpc, 0, None).await.expect("send");
    println!("Transaction hash: {hash:#x}");
}
