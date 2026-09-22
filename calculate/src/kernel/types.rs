//! Packed-type aliases for the no_std kernel (`ckb-gen-types`).

extern crate alloc;

use alloc::string::String;

pub use ckb_gen_types::{
    core::ScriptHashType,
    packed,
    prelude::{Builder, Entity, Pack, PackVec, Unpack},
};

#[cfg(feature = "std")]
use ckb_types::{core::DepType as CkbDepType, H256};

#[cfg(feature = "std")]
use crate::error::{CalculatorError, Result};

/// 32-byte hash used by the kernel skeleton and operations.
pub type Hash256 = [u8; 32];

/// How a cell dep is referenced (`0` = code, `1` = dep group).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DepType {
    Code = 0,
    DepGroup = 1,
}

impl From<DepType> for packed::Byte {
    fn from(value: DepType) -> Self {
        (value as u8).into()
    }
}

#[cfg(feature = "std")]
impl From<CkbDepType> for DepType {
    fn from(value: CkbDepType) -> Self {
        match value {
            CkbDepType::Code => DepType::Code,
            CkbDepType::DepGroup => DepType::DepGroup,
        }
    }
}

#[cfg(feature = "std")]
impl From<DepType> for CkbDepType {
    fn from(value: DepType) -> Self {
        match value {
            DepType::Code => CkbDepType::Code,
            DepType::DepGroup => CkbDepType::DepGroup,
        }
    }
}

#[cfg(feature = "std")]
impl TryFrom<packed::Byte> for DepType {
    type Error = CalculatorError;

    fn try_from(value: packed::Byte) -> Result<Self> {
        match u8::from(value) {
            0 => Ok(DepType::Code),
            1 => Ok(DepType::DepGroup),
            _ => Err(CalculatorError::Other("invalid dep type".into())),
        }
    }
}

/// Header stored on the skeleton (packed molecule `Header`).
pub type HeaderView = packed::Header;

/// TYPE_ID code hash (`TYPE_ID` ASCII, zero-padded).
pub const TYPE_ID_CODE_HASH: Hash256 = [
    0x54, 0x59, 0x50, 0x45, 0x5f, 0x49, 0x44, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// One CKByte in shannons.
pub const SHANNONS_PER_BYTE: u64 = 100_000_000;

/// Occupied capacity in shannons (CKB `Capacity::bytes` units).
pub fn occupied_capacity_shannons(output: &packed::CellOutput, data_len: usize) -> u64 {
    let lock = output.lock();
    let mut bytes = 8 + 32 + 1 + lock.args().raw_data().len() + data_len;
    if let Some(type_script) = output.type_().to_opt() {
        bytes += 32 + 1 + type_script.args().raw_data().len();
    }
    bytes as u64 * SHANNONS_PER_BYTE
}

/// Pack a 32-byte hash into `Byte32`.
pub fn pack_hash(hash: &Hash256) -> packed::Byte32 {
    hash.pack()
}

/// Unpack `Byte32` into [`Hash256`].
pub fn unpack_hash(hash: &packed::Byte32) -> Hash256 {
    let data = hash.raw_data();
    let mut out = [0u8; 32];
    out.copy_from_slice(&data);
    out
}

/// Lowercase `0x`-prefixed hex of a [`Hash256`].
pub fn format_hash(hash: &Hash256) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(66);
    s.push_str("0x");
    for b in hash {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

/// Convert kernel [`Hash256`] to host `H256`.
#[cfg(feature = "std")]
pub fn hash_to_h256(hash: &Hash256) -> H256 {
    (*hash).into()
}

/// Convert host `H256` to kernel [`Hash256`].
#[cfg(feature = "std")]
pub fn h256_to_hash(hash: &H256) -> Hash256 {
    hash.0
}
