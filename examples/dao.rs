use chrono::prelude::Utc;
use ckb_cinnabar_calculator::{
    instruction::predefined::{
        balance_and_sign_with_ckb_cli, dao_deposit, dao_withdraw_phase_one, dao_withdraw_phase_two,
    },
    re_exports::ckb_sdk::{Address, HumanCapacity},
    rpc::RpcClient,
    TransactionCalculator,
};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    /// The address to deposit into, or withdraw, unlock from Nervos DAO
    #[arg(long, value_name = "address")]
    operator: Address,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Deposit some capacity into Nervos DAO from <operator>
    Deposit {
        /// The amount of capacity to deposit
        #[arg(long, value_name = "ckb")]
        ckb: HumanCapacity,
    },
    /// Search and mark deposited Nervos DAO cells under <operator> with flag of withdrawing
    Withdraw {
        /// The maximum amount of capacity to withdraw
        #[arg(long, value_name = "ckb")]
        max_ckb: Option<HumanCapacity>,
        /// The withdrawn capacity must be deposited for such days
        #[arg(long, value_name = "amount")]
        min_deposit_days: Option<u64>,
        /// The address to receive the withdrawn capacity
        #[arg(long, value_name = "address")]
        to: Option<Address>,
    },
    /// Require to unlock withdrawing cells of under <operator> in a queue and wait unlocked when time reached
    Unlock {
        /// The maximum amount of capacity to unlock
        #[arg(long, value_name = "ckb")]
        max_ckb: Option<HumanCapacity>,
        /// The address to receive the unlocked capacity
        #[arg(long, value_name = "address")]
        to: Option<Address>,
    },
}

#[tokio::main]
pub async fn main() {
    let cli = Cli::parse();
    let dao = match cli.command {
        Commands::Deposit { ckb } => dao_deposit(&cli.operator, ckb),
        Commands::Withdraw {
            max_ckb,
            min_deposit_days,
            to,
        } => {
            let timestamp = min_deposit_days.map(|day| Utc::now().timestamp() as u64 - day * 3600);
            dao_withdraw_phase_one(&cli.operator, max_ckb, timestamp, to.as_ref())
        }
        Commands::Unlock { max_ckb, to } => {
            dao_withdraw_phase_two(&cli.operator, max_ckb, to.as_ref())
        }
    };
    let balance_and_sign = balance_and_sign_with_ckb_cli(&cli.operator, 2000, None);

    // build transaction
    let rpc = RpcClient::new_testnet();
    let (skeleton, _) = TransactionCalculator::default()
        .instruction(dao)
        .instruction(balance_and_sign)
        .new_skeleton(&rpc)
        .await
        .expect("build tx");

    // send transaction without any block confirmations
    let hash = skeleton.send_and_wait(&rpc, 0, None).await.expect("send");
    println!("Transaction hash: {hash:#x}");
}
