//! Guest glue around [`ckb-ssri-std`](https://github.com/ashuralyk/ckb-ssri-std).
//!
//! [`ssri_methods!`](ckb_ssri_std::ssri_methods) owns dispatch, including
//! `SSRI.version`, `SSRI.get_methods`, and `SSRI.has_methods`. This module
//! adapts a `SSRI { }` RHS into wire bytes and implements kernel [`RPC`] for
//! the SSRI syscall catalog (`network`, live cell, headers, block hashes,
//! `get_cells`). Tip, fee rate, and `get_transactions` are not in that catalog.

use alloc::{
    borrow::Cow,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::{
    cell::{Ref, RefCell},
    ffi::CStr,
    fmt, str,
};

use ckb_cinnabar_calculator::{
    indexer::{
        Indexer, LiveCell, Order, Pagination, ScriptType, SearchKey, SearchKeyFilter, SearchMode,
        Tx, ValueRangeOption,
    },
    network::Network,
    rpc::{Node, RPC},
    types::{packed, Builder, Entity, Hash256, Pack, ScriptHashType},
    Address, CalculatorError, Result as CalcResult,
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

/// Slot failure from [`SsriArgs`], or the error returned by a converter.
///
/// A missing slot and a bad hex string are [`ArgError::Args`]. Whatever the
/// converter returns is [`ArgError::Convert`] and is not rewritten.
#[derive(Debug)]
pub enum ArgError<E> {
    /// Missing slot, offset overflow, or hex that does not decode.
    Args(Error),
    /// `convert` rejected the decoded bytes.
    Convert(E),
}

impl<E> ArgError<E> {
    /// Keep a slot failure. Map a converter failure with `f`.
    pub fn map_convert(self, f: impl FnOnce(E) -> Error) -> Error {
        match self {
            ArgError::Args(err) => err,
            ArgError::Convert(err) => f(err),
        }
    }
}

impl<E> From<ArgError<E>> for Error
where
    Error: From<E>,
{
    fn from(value: ArgError<E>) -> Self {
        match value {
            ArgError::Args(err) => err,
            ArgError::Convert(err) => err.into(),
        }
    }
}

/// Build `T` from one decoded argument slot.
pub trait FromSsriArg: Sized {
    /// Failure owned by this conversion.
    type Error;

    /// Decode `bytes` into `Self`.
    fn from_ssri_arg(bytes: &[u8]) -> core::result::Result<Self, Self::Error>;
}

/// 32-byte hash whose length was not 32.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidHash {
    /// Length of the decoded slot.
    pub len: usize,
}

impl fmt::Display for InvalidHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "hash must be 32 bytes, got {}", self.len)
    }
}

/// CKB capacity in shannons.
///
/// Wire form is molecule `uint64`: exactly 8 little-endian bytes, the same
/// layout as a cell output's capacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capacity(u64);

impl Capacity {
    /// Shannons stored in this capacity.
    pub fn shannons(self) -> u64 {
        self.0
    }
}

impl From<Capacity> for u64 {
    fn from(value: Capacity) -> Self {
        value.0
    }
}

/// Capacity slot whose length was not 8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidCapacity {
    /// Length of the decoded slot.
    pub len: usize,
}

impl fmt::Display for InvalidCapacity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "capacity must be 8 bytes, got {}", self.len)
    }
}

/// Bech32m address text that did not parse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidAddress {
    detail: String,
}

impl InvalidAddress {
    /// Parser message.
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for InvalidAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

/// Molecule `Entity::from_slice` rejected the slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoleculeError {
    detail: String,
}

impl MoleculeError {
    /// Molecule verifier message.
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for MoleculeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

fn molecule_error(err: impl fmt::Display) -> MoleculeError {
    MoleculeError {
        detail: err.to_string(),
    }
}

/// 32 raw bytes into [`Hash256`].
pub fn hash(bytes: &[u8]) -> core::result::Result<Hash256, InvalidHash> {
    bytes
        .try_into()
        .map_err(|_| InvalidHash { len: bytes.len() })
}

/// Molecule `Script`.
pub fn script(bytes: &[u8]) -> core::result::Result<packed::Script, MoleculeError> {
    packed::Script::from_slice(bytes).map_err(molecule_error)
}

/// UTF-8 bech32m address (`ckb1…` / `ckt1…`).
pub fn address(bytes: &[u8]) -> core::result::Result<Address, InvalidAddress> {
    let text = str::from_utf8(bytes).map_err(|err| InvalidAddress {
        detail: err.to_string(),
    })?;
    text.parse().map_err(|detail| InvalidAddress { detail })
}

/// Molecule `uint64` capacity, in shannons.
pub fn capacity(bytes: &[u8]) -> core::result::Result<Capacity, InvalidCapacity> {
    let raw: [u8; 8] = bytes
        .try_into()
        .map_err(|_| InvalidCapacity { len: bytes.len() })?;
    Ok(Capacity(u64::from_le_bytes(raw)))
}

/// UTF-8 text.
pub fn utf8(bytes: &[u8]) -> core::result::Result<String, str::Utf8Error> {
    str::from_utf8(bytes).map(ToString::to_string)
}

/// Molecule `Transaction`.
pub fn transaction(bytes: &[u8]) -> core::result::Result<packed::Transaction, MoleculeError> {
    packed::Transaction::from_slice(bytes).map_err(molecule_error)
}

impl FromSsriArg for Hash256 {
    type Error = InvalidHash;

    fn from_ssri_arg(bytes: &[u8]) -> core::result::Result<Self, Self::Error> {
        hash(bytes)
    }
}

impl FromSsriArg for packed::Script {
    type Error = MoleculeError;

    fn from_ssri_arg(bytes: &[u8]) -> core::result::Result<Self, Self::Error> {
        script(bytes)
    }
}

impl FromSsriArg for Address {
    type Error = InvalidAddress;

    fn from_ssri_arg(bytes: &[u8]) -> core::result::Result<Self, Self::Error> {
        address(bytes)
    }
}

impl FromSsriArg for Capacity {
    type Error = InvalidCapacity;

    fn from_ssri_arg(bytes: &[u8]) -> core::result::Result<Self, Self::Error> {
        capacity(bytes)
    }
}

impl FromSsriArg for String {
    type Error = str::Utf8Error;

    fn from_ssri_arg(bytes: &[u8]) -> core::result::Result<Self, Self::Error> {
        utf8(bytes)
    }
}

impl FromSsriArg for packed::Transaction {
    type Error = MoleculeError;

    fn from_ssri_arg(bytes: &[u8]) -> core::result::Result<Self, Self::Error> {
        transaction(bytes)
    }
}

#[derive(Clone, Debug)]
struct Slot {
    hex: Vec<u8>,
    decoded: RefCell<Option<Vec<u8>>>,
}

/// SSRI `argv` as an argument extractor.
///
/// `from_argv` copies each slot's hex text and does not decode it. Decoding
/// runs when [`Self::method_path`], [`Self::as_bytes`], or [`Self::bytes`]
/// reads that slot.
///
/// [`Self::method_path`] always reads `argv[0]` as the 8-byte method id.
/// Argument `index` selects `argv[offset + index]`. `offset` starts at 0.
/// One call reads one slot. [`Self::bytes`] passes the decoded bytes to a
/// converter (`FnOnce(&[u8]) -> Result<T, E>`). The converter's error is
/// returned unchanged. [`Self::get`] and [`Self::molecule`] are that call for
/// [`FromSsriArg`] and any molecule [`Entity`].
///
/// Built-in converters: [`hash`], [`script`], [`address`], [`capacity`],
/// [`utf8`], [`transaction`].
///
/// # Example
/// ```ignore
/// fn mint(_source: &SsriSource, args: SsriArgs) -> Result<Vec<u8>> {
///     let _path = args.method_path()?;
///     let args = args.with_offset(1);
///     let _to = args
///         .bytes(0, address)
///         .map_err(|err| err.map_convert(|_| Error::Custom(20)))?;
///     let _ckb = args
///         .bytes(1, capacity)
///         .map_err(|err| err.map_convert(|_| Error::Custom(20)))?;
///     Ok(Vec::new())
/// }
/// ```
#[derive(Clone, Debug, Default)]
pub struct SsriArgs {
    slots: Vec<Slot>,
    offset: usize,
}

impl SsriArgs {
    /// Copy each `argv` slot's hex text. Decoding waits until a reader runs.
    pub fn from_argv(argv: &[Arg]) -> Result<Self> {
        let slots = argv
            .iter()
            .map(|arg| Slot {
                hex: unsafe { CStr::from_ptr(arg.as_ptr()) }.to_bytes().to_vec(),
                decoded: RefCell::new(None),
            })
            .collect();
        Ok(SsriArgs { slots, offset: 0 })
    }

    /// Method id in `argv[0]`: 8 little-endian bytes, the value `ssri_methods!` matches.
    ///
    /// This does not use [`Self::offset`].
    pub fn method_path(&self) -> Result<u64> {
        let bytes = self.decode_slot(0)?;
        let raw: [u8; 8] = (&*bytes)
            .try_into()
            .map_err(|_| Error::SSRIMethodsArgsInvalid)?;
        Ok(u64::from_le_bytes(raw))
    }

    /// Base added to an argument index before choosing an `argv` slot.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Set the argument base. Argument `0` then reads `argv[offset]`.
    pub fn set_offset(&mut self, offset: usize) {
        self.offset = offset;
    }

    /// [`Self::set_offset`] and return `self`.
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    /// How many slots are addressable at the current offset.
    pub fn len(&self) -> usize {
        self.slots.len().saturating_sub(self.offset)
    }

    /// `true` when [`Self::len`] is 0.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Hex-decode argument `index` and return the bytes.
    pub fn as_bytes(&self, index: usize) -> Result<Ref<'_, [u8]>> {
        self.decode_slot(self.argv_index(index)?)
    }

    /// Hex-decode argument `index`, then `convert`.
    ///
    /// `convert` receives one slot. Its `Err` is [`ArgError::Convert`]. A
    /// missing slot or bad hex is [`ArgError::Args`] and `convert` is not called.
    pub fn bytes<T, E, F>(&self, index: usize, convert: F) -> core::result::Result<T, ArgError<E>>
    where
        F: FnOnce(&[u8]) -> core::result::Result<T, E>,
    {
        let raw = self.as_bytes(index).map_err(ArgError::Args)?;
        convert(&raw).map_err(ArgError::Convert)
    }

    /// [`Self::bytes`] with [`FromSsriArg`].
    pub fn get<T: FromSsriArg>(&self, index: usize) -> core::result::Result<T, ArgError<T::Error>> {
        self.bytes(index, T::from_ssri_arg)
    }

    /// [`Self::bytes`] with molecule [`Entity::from_slice`].
    pub fn molecule<T: Entity>(
        &self,
        index: usize,
    ) -> core::result::Result<T, ArgError<MoleculeError>> {
        self.bytes(index, |raw| T::from_slice(raw).map_err(molecule_error))
    }

    fn argv_index(&self, index: usize) -> Result<usize> {
        self.offset
            .checked_add(index)
            .ok_or(Error::SSRIMethodsArgsInvalid)
    }

    fn decode_slot(&self, argv_index: usize) -> Result<Ref<'_, [u8]>> {
        let slot = self
            .slots
            .get(argv_index)
            .ok_or(Error::SSRIMethodsArgsInvalid)?;
        {
            let mut cell = slot.decoded.borrow_mut();
            if cell.is_none() {
                *cell = Some(decode_stored(&slot.hex)?);
            }
        }
        let borrowed = slot.decoded.borrow();
        Ref::filter_map(borrowed, |cell| cell.as_deref()).map_err(|_| Error::SSRIMethodsArgsInvalid)
    }
}

fn decode_stored(hex: &[u8]) -> Result<Vec<u8>> {
    if str::from_utf8(hex).is_err() {
        return Err(Error::SSRIMethodsArgsInvalid);
    }
    let mut buf = Vec::with_capacity(hex.len() + 1);
    buf.extend_from_slice(hex);
    buf.push(0);
    let cstr = CStr::from_bytes_with_nul(&buf).map_err(|_| Error::SSRIMethodsArgsInvalid)?;
    decode_hex(cstr).map_err(|_| Error::SSRIMethodsArgsInvalid)
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

    fn argv_of(parts: &[&[u8]]) -> SsriArgs {
        let argv: Vec<Arg> = parts.iter().copied().map(hex_of).collect();
        SsriArgs::from_argv(&argv).unwrap()
    }

    #[test]
    fn args_decode_when_read_and_offset_starts_at_zero() {
        let path = 0x1122_3344_5566_7788u64.to_le_bytes();
        let args = argv_of(&[&path, b"hello"]);
        assert_eq!(args.offset(), 0);
        assert_eq!(args.len(), 2);
        assert!(!args.is_empty());
        assert_eq!(args.method_path().unwrap(), 0x1122_3344_5566_7788);
        assert_eq!(&*args.as_bytes(0).unwrap(), &path);
        assert_eq!(args.bytes(1, utf8).unwrap(), "hello");

        let shifted = args.with_offset(1);
        assert_eq!(shifted.method_path().unwrap(), 0x1122_3344_5566_7788);
        assert_eq!(shifted.len(), 1);
        assert_eq!(&*shifted.as_bytes(0).unwrap(), b"hello");
        assert!(shifted.as_bytes(1).is_err());
    }

    #[test]
    fn bad_hex_stays_a_slot_error_until_read() {
        let args = SsriArgs::from_argv(&[leak_cstr("0g")]).unwrap();
        let err = args.bytes(0, hash).unwrap_err();
        assert!(matches!(err, ArgError::Args(Error::SSRIMethodsArgsInvalid)));
    }

    #[test]
    fn converter_error_is_left_intact() {
        let args = argv_of(&[b"nope"]);
        let err = args.bytes(0, |_raw| Err::<u8, &str>("nope")).unwrap_err();
        assert!(matches!(err, ArgError::Convert("nope")));

        let mapped = args
            .bytes(0, hash)
            .unwrap_err()
            .map_convert(|_| Error::Custom(20));
        assert_eq!(i8::from(mapped), 20);
    }

    #[test]
    fn builtin_converters_round_trip() {
        use ckb_cinnabar_calculator::{AddressPayload, Network};

        let digest = [0xabu8; 32];
        let lock = packed::Script::new_builder()
            .code_hash(digest.pack())
            .hash_type(ScriptHashType::Type)
            .args(vec![1u8, 2, 3].pack())
            .build();
        let tx = packed::Transaction::new_builder().build();
        let ckb = Capacity(1_000);
        let payload = AddressPayload::new_full(ScriptHashType::Data1, digest, vec![9, 8]);
        let addr = Address::new(Network::Testnet, payload);
        let addr_text = addr.to_string();

        let args = argv_of(&[
            &digest,
            lock.as_slice(),
            addr_text.as_bytes(),
            &1_000u64.to_le_bytes(),
            b"name",
            tx.as_slice(),
        ]);

        assert_eq!(args.bytes(0, hash).unwrap(), digest);
        assert_eq!(args.get::<Hash256>(0).unwrap(), digest);
        assert_eq!(args.bytes(1, script).unwrap().as_slice(), lock.as_slice());
        assert_eq!(
            args.molecule::<packed::Script>(1).unwrap().as_slice(),
            lock.as_slice()
        );
        assert_eq!(args.bytes(2, address).unwrap(), addr);
        assert_eq!(args.get::<Address>(2).unwrap(), addr);
        assert_eq!(args.bytes(3, capacity).unwrap().shannons(), 1_000);
        assert_eq!(args.get::<Capacity>(3).unwrap(), ckb);
        assert_eq!(args.bytes(4, utf8).unwrap(), "name");
        assert_eq!(args.get::<String>(4).unwrap(), "name");
        assert_eq!(
            args.bytes(5, transaction).unwrap().as_slice(),
            tx.as_slice()
        );
        assert_eq!(
            args.get::<packed::Transaction>(5).unwrap().as_slice(),
            tx.as_slice()
        );
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
