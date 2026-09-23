//! Guest glue around [`ckb-ssri-std`](https://github.com/ashuralyk/ckb-ssri-std).
//!
//! [`ssri_methods!`](ckb_ssri_std::ssri_methods) owns dispatch, including
//! `SSRI.version`, `SSRI.get_methods`, and `SSRI.has_methods`. This module
//! only adapts a `SSRI { }` RHS into wire bytes and kernel `Source` lookups
//! onto calculator `ckb-gen-types`.

use alloc::{borrow::Cow, format, string::String, vec, vec::Vec};
use core::ffi::CStr;

use ckb_cinnabar_calculator::{
    source::Source,
    types::{packed, Entity},
    CalculatorError, Result as CalcResult,
};
use ckb_ssri_std::utils::high_level::{
    find_cell_by_out_point as ssri_find_cell, find_cell_data_by_out_point as ssri_find_data,
    find_out_point_by_type as ssri_find_out_point,
};
use ckb_std::{
    ckb_types::prelude::Entity as StdEntity, env::Arg, error::SysError, high_level::decode_hex,
};

use crate::{Error, Result};

pub use ckb_ssri_std::SSRIError;

impl From<SSRIError> for Error {
    fn from(value: SSRIError) -> Self {
        match value {
            SSRIError::SSRIMethodsNotFound => Error::SSRIMethodsNotFound,
            SSRIError::SSRIMethodsArgsInvalid => Error::SSRIMethodsArgsInvalid,
            SSRIError::SSRIMethodsNotImplemented => Error::SSRIMethodsNotImplemented,
            SSRIError::SSRIMethodRequireHigherLevel => Error::SSRIMethodRequireHigherLevel,
            SSRIError::InvalidVmVersion => Error::InvalidVmVersion,
        }
    }
}

impl From<CalculatorError> for Error {
    fn from(value: CalculatorError) -> Self {
        match value {
            CalculatorError::SourceUnavailable(_) => Error::SSRISourceUnavailable,
            CalculatorError::ScriptValidation { exit_code, .. } => Error::Custom(exit_code),
            _ => Error::SSRIAssembleFailed,
        }
    }
}

/// Turn one `SSRI { }` RHS into the bytes `ssri_methods!` writes back.
pub fn export(argv: &[Arg], rhs: impl SsriExport) -> Result<Cow<'static, [u8]>> {
    rhs.export(argv)
}

/// RHS of `"Wire.name" => expr`: a guest fn, bytes, a `u8`, or a [`Result`].
pub trait SsriExport {
    /// Evaluate this RHS against `argv`.
    fn export(self, argv: &[Arg]) -> Result<Cow<'static, [u8]>>;
}

impl<F, R, E> SsriExport for F
where
    F: FnOnce(&SsriSource, SsriArgs) -> core::result::Result<R, E>,
    R: SsriExport,
    Error: From<E>,
{
    fn export(self, argv: &[Arg]) -> Result<Cow<'static, [u8]>> {
        self(&SsriSource, SsriArgs::from_argv(argv)?)?.export(argv)
    }
}

impl<T: SsriExport, E> SsriExport for core::result::Result<T, E>
where
    Error: From<E>,
{
    fn export(self, argv: &[Arg]) -> Result<Cow<'static, [u8]>> {
        self?.export(argv)
    }
}

impl SsriExport for u8 {
    fn export(self, _argv: &[Arg]) -> Result<Cow<'static, [u8]>> {
        Ok(Cow::Owned(vec![self]))
    }
}

impl SsriExport for Vec<u8> {
    fn export(self, _argv: &[Arg]) -> Result<Cow<'static, [u8]>> {
        Ok(Cow::Owned(self))
    }
}

impl SsriExport for &[u8] {
    fn export(self, _argv: &[Arg]) -> Result<Cow<'static, [u8]>> {
        Ok(Cow::Owned(self.to_vec()))
    }
}

impl<const N: usize> SsriExport for &[u8; N] {
    fn export(self, _argv: &[Arg]) -> Result<Cow<'static, [u8]>> {
        Ok(Cow::Owned(self.to_vec()))
    }
}

impl SsriExport for &str {
    fn export(self, _argv: &[Arg]) -> Result<Cow<'static, [u8]>> {
        Ok(Cow::Owned(self.as_bytes().to_vec()))
    }
}

impl SsriExport for String {
    fn export(self, _argv: &[Arg]) -> Result<Cow<'static, [u8]>> {
        Ok(Cow::Owned(self.into_bytes()))
    }
}

impl SsriExport for Cow<'static, [u8]> {
    fn export(self, _argv: &[Arg]) -> Result<Cow<'static, [u8]>> {
        Ok(self)
    }
}

/// Guest `Source`: the three SSRI `find_*` calls only. Not a full `RPC`.
#[derive(Clone, Copy, Debug, Default)]
pub struct SsriSource;

impl Source for SsriSource {
    fn find_out_point_by_type(&self, type_script: &packed::Script) -> CalcResult<packed::OutPoint> {
        let script = to_std(type_script)?;
        let out = ssri_find_out_point(script).map_err(map_find)?;
        from_std(&out)
    }

    fn find_cell_by_out_point(
        &self,
        out_point: &packed::OutPoint,
    ) -> CalcResult<packed::CellOutput> {
        let point = to_std(out_point)?;
        let cell = ssri_find_cell(point).map_err(map_find)?;
        from_std(&cell)
    }

    fn find_cell_data_by_out_point(&self, out_point: &packed::OutPoint) -> CalcResult<Vec<u8>> {
        let point = to_std(out_point)?;
        ssri_find_data(point).map_err(map_find)
    }
}

/// Parsed, hex-decoded SSRI argument slots. Slot `0` is always the method path.
///
/// The mapping from slot indices to argument semantics is determined by each method's
/// definition. This structure stores the argument bytes only. Access argument slot
/// data directly via [`Self::bytes`], which returns the decoded bytes for a given slot.
///
/// # Example
/// ```ignore
/// fn mint(_source: &SsriSource, args: SsriArgs) -> Result<Vec<u8>> {
///     let tx_bytes = args.bytes(1)?;
///     let to_bytes = args.bytes(2)?;
///     let amount_bytes = args.bytes(3)?;
///     // decode/parse as required here
///     // ...
///     Ok(Vec::new()) // placeholder
/// }
/// ```
#[derive(Clone, Debug, Default)]
pub struct SsriArgs {
    argv: Vec<Vec<u8>>,
}

impl SsriArgs {
    /// Hex-decode every `argv` slot.
    pub fn from_argv(argv: &[Arg]) -> Result<Self> {
        let argv = argv
            .iter()
            .map(|v| unsafe {
                decode_hex(CStr::from_ptr(v.as_ptr())).map_err(|_| Error::SSRIMethodsArgsInvalid)
            })
            .collect::<Result<_>>()?;
        Ok(SsriArgs { argv })
    }

    /// Number of slots, including the method path.
    pub fn len(&self) -> usize {
        self.argv.len()
    }

    /// `true` when [`Self::from_argv`] received an empty `argv`.
    pub fn is_empty(&self) -> bool {
        self.argv.is_empty()
    }

    /// Raw bytes of slot `index`.
    pub fn bytes(&self, index: usize) -> Result<&[u8]> {
        self.argv
            .get(index)
            .map(Vec::as_slice)
            .ok_or(Error::SSRIMethodsArgsInvalid)
    }
}

fn to_std<T: Entity, U: StdEntity>(value: &T) -> CalcResult<U> {
    U::from_slice(value.as_slice()).map_err(|_| CalculatorError::Other("ssri packed bytes".into()))
}

fn from_std<T: StdEntity, U: Entity>(value: &T) -> CalcResult<U> {
    U::from_slice(value.as_slice()).map_err(|_| CalculatorError::Other("ssri packed bytes".into()))
}

fn map_find(err: SysError) -> CalculatorError {
    match err {
        SysError::ItemMissing => {
            CalculatorError::CellDepNotFound("ssri find_*: cell not found".into())
        }
        SysError::Encoding => CalculatorError::Other("ssri find_*: encoding".into()),
        other => CalculatorError::SourceUnavailable(format!("ssri find_*: {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use core::ffi::CStr;

    fn leak_cstr(text: &str) -> Arg {
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(0);
        let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        Arg::from(CStr::from_bytes_with_nul(leaked).expect("nul"))
    }

    fn hex_of(bytes: &[u8]) -> Arg {
        let mut text = String::new();
        for byte in bytes {
            use core::fmt::Write;
            write!(&mut text, "{byte:02x}").unwrap();
        }
        leak_cstr(&text)
    }

    fn method_arg() -> Arg {
        hex_of(&[0u8; 8])
    }

    fn mint(_source: &SsriSource, _args: SsriArgs) -> Result<Vec<u8>> {
        Ok(b"minted".to_vec())
    }

    #[test]
    fn ssri_error_maps_honestly() {
        assert_eq!(i8::from(Error::from(SSRIError::SSRIMethodsNotFound)), 12);
        assert_eq!(i8::from(Error::from(SSRIError::SSRIMethodsArgsInvalid)), 13);
        assert_eq!(
            i8::from(Error::from(SSRIError::SSRIMethodsNotImplemented)),
            14
        );
        assert_eq!(
            i8::from(Error::from(SSRIError::SSRIMethodRequireHigherLevel)),
            15
        );
        assert_eq!(i8::from(Error::from(SSRIError::InvalidVmVersion)), 16);
        assert_eq!(
            i8::from(Error::from(CalculatorError::SourceUnavailable("x".into()))),
            18
        );
        assert_eq!(
            i8::from(Error::from(CalculatorError::Other("assemble".into()))),
            17
        );
    }

    #[test]
    fn export_fn_bytes_u8_and_vec() {
        const NAME: &[u8] = b"Test UDT";
        let argv = [method_arg()];
        assert_eq!(&*export(&argv, mint).unwrap(), b"minted");
        assert_eq!(&*export(&[], NAME).unwrap(), b"Test UDT");
        assert_eq!(&*export(&[], 8u8).unwrap(), &[8]);
        let icon = alloc::vec![1u8, 2, 3];
        assert_eq!(&*export(&[], icon).unwrap(), &[1, 2, 3]);
    }

    #[allow(dead_code)]
    fn ssri_methods_accepts_export_rhs() -> Result<Cow<'static, [u8]>> {
        let argv = ckb_std::env::argv();
        use core::result::Result;
        ckb_ssri_std::ssri_methods!(
            argv: &argv,
            invalid_method: Error::SSRIMethodsNotFound,
            invalid_args: Error::SSRIMethodsArgsInvalid,
            "UDT.name" => export(&argv, "Test UDT"),
            "UDT.decimals" => export(&argv, 8u8),
            "UDT.mint" => export(&argv, mint),
        )
    }
}
