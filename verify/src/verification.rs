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

/// Register the verification tree, and optionally an SSRI door.
///
/// Hop-only form is unchanged. Enable crate feature `ssri` and append
/// `SSRI { "Wire.name" => expr, ... }` to emit a second entry: empty `argv`
/// walks the hop tree; SSRI VM (`argv` + `vm_version == u64::MAX`) dispatches
/// the wire table. `SSRI { }` is the door plus an explicit wire table, not a
/// protocol identity. Each left-hand side is a string literal written in this
/// block; that literal is the method name `ssri_methods!` matches. Keep the
/// wire table here. Hop `verify()` stays off the wire.
///
/// Each RHS is any expression `ssri::export` can turn into bytes: a
/// `fn(&SsriSource, SsriArgs) -> Result<R>`, a `&[u8]` / `str` constant, a
/// `u8`, a `Vec<u8>` local, or a `Result`. The guest fn reads each slot with
/// `SsriArgs::get` or `SsriArgs::molecule`.
/// `ssri_methods!` always emits `SSRI.version`, `SSRI.get_methods`, and
/// `SSRI.has_methods`. Do not repeat those wire strings in the block.
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
///
/// const NAME: &[u8] = b"Test UDT";
/// let decimals: u8 = 8;
///
/// cinnabar_main!(
///     GlobalContext,
///     (TREE_ROOT, RootVerifier),
///     (intent::TRANSFER, BranchVerifier),
///     SSRI {
///         "UDT.mint"     => mint,
///         "UDT.name"     => NAME,
///         "UDT.decimals" => decimals,
///     }
/// );
/// ```
#[macro_export]
macro_rules! cinnabar_main {
    (
        $ctx:ty,
        $(($name:expr, $verifier:ty)),+ $(,)?
        SSRI { $($wire:literal => $entry:expr),+ $(,)? } $(,)?
    ) => {
        ckb_std::default_alloc!();
        ckb_std::entry!(program_entry);

        pub fn program_entry() -> i8 {
            match program_entry_inner() {
                Ok(()) => 0,
                Err(err) => err.into(),
            }
        }

        fn program_entry_inner() -> ckb_cinnabar_verifier::Result<()> {
            if ckb_cinnabar_verifier::re_exports::ckb_ssri_std::should_fallback()
                .map_err(ckb_cinnabar_verifier::Error::from)?
            {
                let mut ctx = <$ctx>::default();
                let mut verifier = ckb_cinnabar_verifier::TransactionVerifier::default();
                $(
                    verifier.add_verifier($name, alloc::boxed::Box::new(<$verifier>::default()));
                )+
                verifier.run(&mut ctx)
            } else {
                let argv = ckb_std::env::argv();
                let bytes = {
                    use ckb_cinnabar_verifier::Error;
                    // `ssri_methods!` emits `Result<T, Error>`. Shadow the
                    // verifier's one-argument `Result` alias if the contract imported it.
                    use core::result::Result;
                    ckb_cinnabar_verifier::expand_ssri_methods!(
                        argv: &argv,
                        invalid_method: Error::SSRIMethodsNotFound,
                        invalid_args: Error::SSRIMethodsArgsInvalid,
                        $($wire => ckb_cinnabar_verifier::ssri::export(&argv, $entry),)+
                    )?
                };
                let pipe = ckb_std::syscalls::pipe()?;
                ckb_std::syscalls::write(pipe.1, &bytes)?;
                Ok(())
            }
        }
    };

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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Ctx;

    #[derive(Default)]
    struct Root;

    impl Verification<Ctx> for Root {
        fn verify(&mut self, _name: &str, _ctx: &mut Ctx) -> Result<Option<&str>> {
            Ok(None)
        }
    }

    #[test]
    fn hop_only_tree_type_checks() {
        let mut ctx = Ctx;
        let mut verifier = TransactionVerifier::default();
        verifier.add_verifier(TREE_ROOT, Box::new(Root));
        verifier.run(&mut ctx).unwrap();
    }
}
