//! Verification tree: named nodes walked from [`TREE_ROOT`].
//!
//! Register nodes with [`cinnabar_main!`]. Each [`Verification::verify`]
//! returns `Ok(Some(next))` to hop (prefer [`crate::intent`] constants),
//! `Ok(None)` to succeed, or `Err` to fail the script.

use alloc::{borrow::ToOwned, boxed::Box, collections::BTreeMap, string::String};
use ckb_std::debug;

use crate::error::{Error, Result};

/// Where the verification tree starts. Always register a node under this name
/// as the first hop of [`cinnabar_main!`].
pub const TREE_ROOT: &str = "root";

/// One node of the verification tree.
///
/// `T` is the contract-defined context, created fresh per run and threaded
/// through every visited node.
pub trait Verification<T: Default> {
    /// Check the transaction from this node's perspective.
    ///
    /// - `Ok(Some(next))` — continue to the node registered under `next`
    ///   (use [`crate::intent`] constants for cross-contract hops).
    /// - `Ok(None)` — verification succeeds, the walk stops.
    /// - `Err(e)` — verification fails; becomes the script exit code.
    fn verify(&mut self, verifier_name: &str, ctx: &mut T) -> Result<Option<&str>>;
}

/// Construct a batch of transaction verifiers in form of tree
#[derive(Default)]
pub struct TransactionVerifier<T: Default> {
    verification_tree: BTreeMap<String, Box<dyn Verification<T>>>,
}

impl<T: Default> TransactionVerifier<T> {
    /// Register `verifier` under `name`; later registrations overwrite
    /// earlier ones with the same name.
    pub fn add_verifier(
        &mut self,
        name: &'static str,
        verifier: Box<dyn Verification<T>>,
    ) -> &mut Self {
        self.verification_tree.insert(name.to_owned(), verifier);
        self
    }

    /// Walk the tree from [`TREE_ROOT`] until a node returns `None` (success)
    /// or an error. Each node is removed as it is visited, so cycles fail with
    /// [`Error::NotFoundBranchVerifier`].
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
/// ```ignore
/// use ckb_cinnabar_verifier::{
///     cinnabar_main, define_errors, intent, Result, Verification, CUSTOM_ERROR_START, TREE_ROOT,
/// };
///
/// define_errors!(CustomError, {
///     MyError1 = CUSTOM_ERROR_START,
///     MyError2,
/// });
///
/// #[derive(Default)]
/// struct GlobalContext {}
///
/// #[derive(Default)]
/// struct RootVerifier;
///
/// impl Verification<GlobalContext> for RootVerifier {
///     fn verify(&mut self, _name: &str, _ctx: &mut GlobalContext) -> Result<Option<&str>> {
///         Ok(Some(intent::TRANSFER))
///     }
/// }
///
/// #[derive(Default)]
/// struct BranchVerifier;
///
/// impl Verification<GlobalContext> for BranchVerifier {
///     fn verify(&mut self, _name: &str, _ctx: &mut GlobalContext) -> Result<Option<&str>> {
///         Ok(None)
///     }
/// }
///
/// cinnabar_main!(
///     GlobalContext,
///     (TREE_ROOT, RootVerifier),
///     (intent::TRANSFER, BranchVerifier)
/// );
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
