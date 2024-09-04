pub mod basic;
pub mod component;
pub mod dao;
pub mod spore;
pub use common::{Log, Operation};

mod common {
    use crate::{rpc::RPC, skeleton::TransactionSkeleton};

    pub type Log = std::collections::HashMap<&'static str, Vec<u8>>;

    #[async_trait::async_trait]
    pub trait Operation<T: RPC> {
        async fn run(
            self: Box<Self>,
            rpc: &T,
            skeleton: &mut TransactionSkeleton,
            log: &mut Log,
        ) -> eyre::Result<()>;
    }
}
