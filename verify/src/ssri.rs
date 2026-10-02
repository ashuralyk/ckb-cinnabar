//! Guest glue around [`ckb-ssri-std`](https://github.com/ashuralyk/ckb-ssri-std).
//!
//! [`ssri_methods!`](ckb_ssri_std::ssri_methods) owns dispatch, including
//! `SSRI.version`, `SSRI.get_methods`, and `SSRI.has_methods`. This module
//! adapts a `SSRI { }` RHS into wire bytes and implements kernel [`RPC`] for
//! the SSRI syscall catalog (`network`, live cell, headers, block hashes,
//! `get_cells`). Tip, fee rate, and `get_transactions` are not in that catalog.

use alloc::{borrow::Cow, format, string::String, vec, vec::Vec};
use core::ffi::CStr;

use ckb_cinnabar_calculator::{
    indexer::{
        Indexer, LiveCell, Order, Pagination, ScriptType, SearchKey, SearchKeyFilter, SearchMode,
        Tx, ValueRangeOption,
    },
    network::Network,
    rpc::{Node, RPC},
    types::{packed, Builder, Entity, Hash256, Pack, ScriptHashType},
    CalculatorError, Result as CalcResult,
};
use ckb_ssri_std::{
    high_level::{self, Network as SsriNetwork},
    indexer as ssri_ix,
};
use ckb_std::{
    ckb_types::{
        packed::{Byte32 as StdByte32, Header as StdHeader},
        prelude::Entity as StdEntity,
    },
    env::Arg,
    error::SysError,
    high_level::decode_hex,
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

/// Guest [`RPC`]: the SSRI syscall catalog. Not tip, fee, or `get_transactions`.
///
/// [`Source`](ckb_cinnabar_calculator::Source) comes from the `Node + Indexer`
/// blanket impl, so `find_*` goes through `get_cells` and `get_live_cell`.
#[derive(Clone, Copy, Debug, Default)]
pub struct SsriSource;

impl Node for SsriSource {
    fn network(&self) -> Network {
        match high_level::network() {
            Ok(network) => kernel_network(network),
            Err(_) => Network::Fake,
        }
    }

    fn get_live_cell(&self, out_point: &packed::OutPoint, with_data: bool) -> CalcResult<LiveCell> {
        let point = to_std(out_point)?;
        let live = high_level::get_live_cell(point, with_data).map_err(map_sys)?;
        let Some(output) = live.output else {
            return Err(CalculatorError::InputCellNotFound(
                "ssri get_live_cell: cell not found".into(),
            ));
        };
        Ok(LiveCell {
            output: pack_cell_output(&output),
            output_data: live.data.unwrap_or_default(),
            out_point: out_point.clone(),
            block_number: 0,
            tx_index: 0,
        })
    }

    fn get_header(&self, hash: &Hash256) -> CalcResult<Option<packed::Header>> {
        optional_header(high_level::get_header(byte32(hash)?))
    }

    fn get_header_by_number(&self, number: u64) -> CalcResult<Option<packed::Header>> {
        optional_header(high_level::get_header_by_number(number))
    }

    fn get_tip_header(&self) -> CalcResult<packed::Header> {
        Err(unavailable("get_tip_header"))
    }

    fn get_block_hash(&self, number: u64) -> CalcResult<Option<Hash256>> {
        optional_hash(high_level::get_block_hash(number))
    }

    fn get_tip_block_number(&self) -> CalcResult<u64> {
        Err(unavailable("get_tip_block_number"))
    }

    fn get_transaction_block_hash(&self, tx_hash: &Hash256) -> CalcResult<Option<Hash256>> {
        optional_hash(high_level::get_transaction_block_hash(byte32(tx_hash)?))
    }

    fn min_fee_rate(&self) -> CalcResult<u64> {
        Err(unavailable("min_fee_rate"))
    }
}

impl Indexer for SsriSource {
    fn get_cells(
        &self,
        search_key: &SearchKey,
        order: Order,
        limit: u32,
        cursor: Option<&[u8]>,
    ) -> CalcResult<Pagination<LiveCell>> {
        let page = high_level::get_cells(
            &search_key_to_ssri(search_key),
            ssri_order(order),
            u64::from(limit),
            cursor.unwrap_or(&[]),
        )
        .map_err(map_sys)?;
        let mut objects = Vec::with_capacity(page.objects.len());
        for cell in page.objects {
            objects.push(indexer_cell_to_live(&cell));
        }
        Ok(Pagination {
            objects,
            last_cursor: page.last_cursor,
        })
    }

    fn get_transactions(
        &self,
        _search_key: &SearchKey,
        _order: Order,
        _limit: u32,
        _cursor: Option<&[u8]>,
    ) -> CalcResult<Pagination<Tx>> {
        Err(unavailable("get_transactions"))
    }
}

impl RPC for SsriSource {}

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

fn kernel_network(network: SsriNetwork) -> Network {
    match network {
        SsriNetwork::Mainnet => Network::Mainnet,
        SsriNetwork::Testnet => Network::Testnet,
        SsriNetwork::Unknown => Network::Fake,
    }
}

fn unavailable(method: &str) -> CalculatorError {
    CalculatorError::SourceUnavailable(format!("ssri has no {method} syscall"))
}

fn map_sys(err: SysError) -> CalculatorError {
    match err {
        SysError::ItemMissing => CalculatorError::InputCellNotFound("ssri: cell not found".into()),
        SysError::Encoding => CalculatorError::Other("ssri: encoding".into()),
        other => CalculatorError::SourceUnavailable(format!("ssri: {other:?}")),
    }
}

fn optional_header(
    result: core::result::Result<StdHeader, SysError>,
) -> CalcResult<Option<packed::Header>> {
    match result {
        Ok(header) => Ok(Some(from_std(&header)?)),
        Err(SysError::ItemMissing) => Ok(None),
        Err(err) => Err(map_sys(err)),
    }
}

fn optional_hash(result: core::result::Result<StdByte32, SysError>) -> CalcResult<Option<Hash256>> {
    match result {
        Ok(hash) => {
            let mut out = [0u8; 32];
            out.copy_from_slice(hash.as_slice());
            Ok(Some(out))
        }
        Err(SysError::ItemMissing) => Ok(None),
        Err(err) => Err(map_sys(err)),
    }
}

fn byte32(hash: &Hash256) -> CalcResult<StdByte32> {
    StdEntity::from_slice(hash).map_err(|_| CalculatorError::Other("ssri packed bytes".into()))
}

fn ssri_order(order: Order) -> ssri_ix::Order {
    match order {
        Order::Asc => ssri_ix::Order::Asc,
        Order::Desc => ssri_ix::Order::Desc,
    }
}

fn ssri_script_type(script_type: ScriptType) -> ssri_ix::ScriptType {
    match script_type {
        ScriptType::Lock => ssri_ix::ScriptType::Lock,
        ScriptType::Type => ssri_ix::ScriptType::Type,
    }
}

fn ssri_search_mode(mode: SearchMode) -> ssri_ix::SearchMode {
    match mode {
        SearchMode::Prefix => ssri_ix::SearchMode::Prefix,
        SearchMode::Exact => ssri_ix::SearchMode::Exact,
        SearchMode::Partial => ssri_ix::SearchMode::Partial,
    }
}

fn range_pair(range: &ValueRangeOption) -> [u64; 2] {
    [range.start, range.end]
}

fn pack_script(script: &ssri_ix::Script) -> packed::Script {
    let hash_type = match script.hash_type {
        0 => ScriptHashType::Data,
        1 => ScriptHashType::Type,
        2 => ScriptHashType::Data1,
        4 => ScriptHashType::Data2,
        other => ScriptHashType::from_repr(other).unwrap_or(ScriptHashType::Data),
    };
    packed::Script::new_builder()
        .code_hash(script.code_hash.pack())
        .hash_type(hash_type)
        .args(script.args.pack())
        .build()
}

fn unpack_script(script: &packed::Script) -> ssri_ix::Script {
    let mut code_hash = [0u8; 32];
    code_hash.copy_from_slice(script.code_hash().as_slice());
    ssri_ix::Script {
        code_hash,
        hash_type: u8::from(script.hash_type()),
        args: script.args().raw_data().to_vec(),
    }
}

fn pack_cell_output(output: &ssri_ix::CellOutput) -> packed::CellOutput {
    let type_script = output.type_.as_ref().map(pack_script);
    packed::CellOutput::new_builder()
        .capacity(output.capacity)
        .lock(pack_script(&output.lock))
        .type_(type_script.pack())
        .build()
}

fn pack_out_point(out_point: &ssri_ix::OutPoint) -> packed::OutPoint {
    packed::OutPoint::new_builder()
        .tx_hash(out_point.tx_hash.pack())
        .index(out_point.index)
        .build()
}

fn search_key_to_ssri(key: &SearchKey) -> ssri_ix::SearchKey {
    ssri_ix::SearchKey {
        script: unpack_script(&key.script),
        script_type: ssri_script_type(key.script_type),
        script_search_mode: key.script_search_mode.map(ssri_search_mode),
        filter: key.filter.as_ref().map(filter_to_ssri),
        with_data: key.with_data,
        group_by_transaction: key.group_by_transaction,
    }
}

fn filter_to_ssri(filter: &SearchKeyFilter) -> ssri_ix::SearchKeyFilter {
    ssri_ix::SearchKeyFilter {
        script: filter.script.as_ref().map(unpack_script),
        script_len_range: filter.script_len_range.as_ref().map(range_pair),
        output_data: filter.output_data.clone(),
        output_data_filter_mode: filter.output_data_filter_mode.map(ssri_search_mode),
        output_data_len_range: filter.output_data_len_range.as_ref().map(range_pair),
        output_capacity_range: filter.output_capacity_range.as_ref().map(range_pair),
        block_range: filter.block_range.as_ref().map(range_pair),
    }
}

fn indexer_cell_to_live(cell: &ssri_ix::IndexerCell) -> LiveCell {
    LiveCell {
        output: pack_cell_output(&cell.output),
        output_data: cell.output_data.clone().unwrap_or_default(),
        out_point: pack_out_point(&cell.out_point),
        block_number: cell.block_number,
        tx_index: cell.tx_index,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::{
        indexer_cell_to_live, kernel_network, pack_script, range_pair, ssri_ix, ssri_order,
        unpack_script,
    };
    use alloc::boxed::Box;
    use ckb_cinnabar_calculator::types::Unpack;
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

    #[test]
    fn network_and_order_match_kernel() {
        assert_eq!(kernel_network(SsriNetwork::Mainnet), Network::Mainnet);
        assert_eq!(kernel_network(SsriNetwork::Testnet), Network::Testnet);
        assert_eq!(kernel_network(SsriNetwork::Unknown), Network::Fake);
        assert_eq!(ssri_order(Order::Asc) as u64, 0);
        assert_eq!(ssri_order(Order::Desc) as u64, 1);
        assert_eq!(range_pair(&ValueRangeOption::new(1, 4)), [1, 4]);
    }

    #[test]
    fn script_and_indexer_cell_round_trip() {
        let script = packed::Script::new_builder()
            .code_hash([7u8; 32].pack())
            .hash_type(ScriptHashType::Type)
            .args(vec![1u8, 2, 3].pack())
            .build();
        let packed_again = pack_script(&unpack_script(&script));
        assert_eq!(packed_again.as_slice(), script.as_slice());

        let cell = ssri_ix::IndexerCell {
            output: ssri_ix::CellOutput {
                capacity: 100,
                lock: unpack_script(&script),
                type_: None,
            },
            output_data: None,
            out_point: ssri_ix::OutPoint {
                tx_hash: [9u8; 32],
                index: 2,
            },
            block_number: 42,
            tx_index: 3,
        };
        let live = indexer_cell_to_live(&cell);
        assert!(live.output_data.is_empty());
        assert_eq!(live.block_number, 42);
        assert_eq!(live.tx_index, 3);
        let index: u32 = live.out_point.index().unpack();
        assert_eq!(index, 2);
        assert_eq!(live.output.lock().as_slice(), script.as_slice());
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
