use alloc::{borrow::ToOwned, boxed::Box, collections::BTreeMap, string::String};
use ckb_std::debug;

use crate::error::{Error, Result};

/// Where the verification tree starts
pub const TREE_ROOT: &str = "root";

pub trait Verification<T: Default> {
    fn verify(&mut self, verifier_name: &str, ctx: &mut T) -> Result<Option<&str>>;
}

/// Construct a batch of transaction verifiers in form of tree
#[derive(Default)]
pub struct TransactionVerifier<T: Default> {
    verification_tree: BTreeMap<String, Box<dyn Verification<T>>>,
}

impl<T: Default> TransactionVerifier<T> {
    pub fn add_verifier(
        &mut self,
        name: &'static str,
        verifier: Box<dyn Verification<T>>,
    ) -> &mut Self {
        self.verification_tree.insert(name.to_owned(), verifier);
        self
    }

    pub fn run(mut self, ctx: &mut T) -> Result<()> {
        let mut root = self
            .verification_tree
            .remove(TREE_ROOT)
            .ok_or(Error::NotFoundRootVerifier)?;
        let mut branch = root.verify(TREE_ROOT, ctx)?.map(ToOwned::to_owned);
        while let Some(name) = branch {
            let mut verifier = self.verification_tree.remove(&name).ok_or_else(|| {
                debug!("verifier not found: {}", name);
                Error::NotFoundBranchVerifier
            })?;
            branch = verifier.verify(&name, ctx)?.map(ToOwned::to_owned);
        }
        Ok(())
    }
}

/// Examples:
///
/// ```
/// use ckb_cinnabar_verifier::{cinnabar_main, TREE_ROOT, define_errors, CUSTOM_ERROR_START};
///
/// // define custom errors
/// define_errors!(CustomError, {
///     MyError1 = CUSTOM_ERROR_START,
///     MyError2,
///     // more errors
/// });
///
/// // contains the global context data
/// struct GlobalContext {
///    // some fields
/// }
///
/// // the entry verifier
/// #[derive(Default)]
/// struct RootVerifier;
///
/// // must implement Verification trait
/// impl Verification<GlobalContext> for RootVerifier {
///    fn verify(&mut self, verifier_name: &str, ctx: &mut GlobalContext) -> Result<Option<&str>> {
///         // ...
///         Ok(Some("branch")) // head to branch verifier
///     }
/// }
///
/// // the branch verifier
/// #[derive(Default)]
/// struct BranchVerifier;
///
/// // must implement Verification trait
/// impl Verification<GlobalContext> for RootVerifier {
///    fn verify(&mut self, verifier_name: &str, ctx: &mut GlobalContext) -> Result<Option<&str>> {
///         // ...
///         Ok(None) // end of the verification
///     }
/// }
///
/// cinnabar_main!(GlobalContext, (TREE_ROOT, RootVerifier), ("branch", BranchVerifier));
/// ```
#[macro_export]
macro_rules! cinnabar_main {
    ($ctx:ty, $(($name:expr, $verifier:ty) $(,)?)+) => {
        ckb_std::default_alloc!();
        ckb_std::entry!(program_entry);

        pub fn program_entry() -> i8 {
            let mut ctx = <$ctx>::default();
            let mut verifier = ckb_cinnabar_verifier::TransactionVerifier::default();
            $(
                verifier.add_verifier($name, alloc::boxed::Box::new(<$verifier>::default()));
            )+
            match verifier.run(&mut ctx) {
                Ok(_) => 0,
                Err(err) => err.into(),
            }
        }
    };
}
