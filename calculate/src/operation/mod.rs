pub mod basic;
pub mod component;
#[cfg(feature = "spore")]
pub mod spore;
pub mod udt;
pub use common::{Log, Operation};

#[cfg(not(target_arch = "wasm32"))]
pub mod dao;

mod common {
    use crate::{rpc::RPC, skeleton::TransactionSkeleton};

    pub type Log = Vec<(&'static str, Vec<u8>)>;

    #[async_trait::async_trait(?Send)]
    pub trait Operation<T: RPC> {
        async fn run(
            self: Box<Self>,
            rpc: &T,
            skeleton: &mut TransactionSkeleton,
            log: &mut Log,
        ) -> eyre::Result<()>;
    }
}
