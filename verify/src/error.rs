//! On-chain error types and the [`define_errors!`] helper.
//!
//! The script entry point converts [`Error`] to `i8`. Off-chain simulation
//! surfaces that same code as `CalculatorError::ScriptValidation { exit_code, .. }`.

use ckb_std::error::SysError;

/// Declare a custom contract error enum.
///
/// Generates `#[repr(i8)] enum $name` plus `From<$name> for Error`, so
/// verification nodes can return `Err(MyError::Foo.into())`. Start custom
/// codes at [`CUSTOM_ERROR_START`] to stay clear of system / framework codes:
///
/// ```ignore
/// define_errors!(MyError, { First = CUSTOM_ERROR_START, Second, });
/// ```
#[macro_export]
macro_rules! define_errors {
    ($name:ident, {$($err:ident $(= $val:ident)? ,)+}) => {
        #[repr(i8)]
        pub enum $name {
            $($err $(= $val)? ,)+
        }

        impl From<$name> for ckb_cinnabar_verifier::Error {
            fn from(value: $name) -> Self {
                ckb_cinnabar_verifier::Error::Custom(value as i8)
            }
        }
    };
}

/// First exit code available to contract-defined errors. Codes 1–5 are
/// system errors and 10–11 are framework errors.
pub const CUSTOM_ERROR_START: i8 = 20;

/// Unified on-chain error type; converts into the `i8` exit code returned by
/// the script entry point.
pub enum Error {
    /// CKB-VM `IndexOutOfBound` (exit `1`).
    IndexOutOfBound,
    /// CKB-VM `ItemMissing` (exit `2`).
    ItemMissing,
    /// CKB-VM `LengthNotEnough` (exit `3`).
    LengthNotEnough,
    /// CKB-VM `Encoding` (exit `4`).
    Encoding,
    /// Any other CKB-VM [`SysError`] (exit `5`).
    UnknownSystemError,

    /// The `"root"` node was not registered with `cinnabar_main!` (exit `10`).
    NotFoundRootVerifier,
    /// A node returned a next-name that is not registered, or the walk cycled (exit `11`).
    NotFoundBranchVerifier,

    /// Contract-defined failure, carrying the raw `i8` from [`define_errors!`] (must be ≥ [`CUSTOM_ERROR_START`]).
    Custom(i8),
}

impl From<SysError> for Error {
    fn from(value: SysError) -> Self {
        match value {
            SysError::IndexOutOfBound => Self::IndexOutOfBound,
            SysError::ItemMissing => Self::ItemMissing,
            SysError::LengthNotEnough(_) => Self::LengthNotEnough,
            SysError::Encoding => Self::Encoding,
            _ => Self::UnknownSystemError,
        }
    }
}

impl From<Error> for i8 {
    fn from(value: Error) -> i8 {
        match value {
            Error::IndexOutOfBound => 1,
            Error::ItemMissing => 2,
            Error::LengthNotEnough => 3,
            Error::Encoding => 4,
            Error::UnknownSystemError => 5,
            Error::NotFoundRootVerifier => 10,
            Error::NotFoundBranchVerifier => 11,
            Error::Custom(value) => value,
        }
    }
}

/// On-chain result type used by every [`crate::Verification`] node.
pub type Result<T> = core::result::Result<T, Error>;
