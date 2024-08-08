use alloc::collections::BTreeMap;
use ckb_std::debug;

use crate::error::{Error, Result};

/// Where the verification tree starts
pub const TREE_ROOT: &str = "ROOT";

pub trait Context {}

pub trait Verification {
    type CTX: Context;

    fn verify(&self, verifier_name: &str, ctx: &mut Self::CTX) -> Result<Option<&str>>;
}

/// Construct a batch of transaction verifiers in form of tree
#[derive(Default)]
pub struct TransactionVerifier<T, V>
where
    T: Context,
    V: Verification<CTX = T>,
{
    verification_tree: BTreeMap<&'static str, V>,
}

impl<T, V> TransactionVerifier<T, V>
where
    T: Context,
    V: Verification<CTX = T>,
{
    pub fn add_verifier(&mut self, name: &'static str, verifier: V) -> &mut Self {
        self.verification_tree.insert(name, verifier);
        self
    }

    pub fn run(self, ctx: &mut T) -> Result<()> {
        let root = self
            .verification_tree
            .get(TREE_ROOT)
            .ok_or(Error::NotFoundRootVerifier)?;
        let mut branch = root.verify(TREE_ROOT, ctx)?;
        while let Some(name) = branch {
            let verifier = self.verification_tree.get(name).ok_or_else(|| {
                debug!("verifier not found: {}", name);
                Error::NotFoundBranchVerifier
            })?;
            branch = verifier.verify(name, ctx)?;
        }
        Ok(())
    }
}

#[macro_export]
macro_rules! cinnabar_main {
    ($ctx:ty, $(($name:expr, $verifier:ty) $(,)?)+) => {
        ckb_std::default_alloc!();
        ckb_std::entry!(program_entry);

        pub fn program_entry() -> i8 {
            let mut ctx = <$ctx>::default();
            let mut verifier = TransactionVerifier::default();
            $(
                verifier.add_verifier($name, <$verifier>::default());
            )+
            match verifier.run(&mut ctx) {
                Ok(_) => 0,
                Err(err) => err.into(),
            }
        }
    };
}
