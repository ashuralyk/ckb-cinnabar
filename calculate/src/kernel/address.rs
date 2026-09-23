//! CKB 2021 full addresses (bech32m): a lock script bound to a [`Network`].
//!
//! [`Address`] parses `ckb1…` / `ckt1…` strings and converts to/from packed
//! [`Script`]. [`AddressPayload`] is the lock-script triple without the HRP.
//! Both types are `no_std` + `alloc`; the shell re-exports them.

extern crate alloc;

use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::{fmt, str::FromStr};

use bech32::{convert_bits, ToBase32, Variant};

use crate::kernel::{
    error::Result,
    network::Network,
    skeleton::ScriptEx,
    types::{
        format_hash, pack_hash, packed::Script, unpack_hash, Builder, Entity, Hash256, Pack,
        ScriptHashType,
    },
};

/// Payload of a CKB full address (ckb2021 format): the lock script triple.
///
/// Encoded as `0x00 | code_hash | hash_type | args` under bech32m; see
/// [RFC-0021](https://github.com/nervosnetwork/rfcs/blob/master/rfcs/0021-ckb-address-format/0021-ckb-address-format.md).
#[derive(Hash, Eq, PartialEq, Clone)]
pub struct AddressPayload {
    hash_type: ScriptHashType,
    code_hash: Hash256,
    args: Vec<u8>,
}

impl AddressPayload {
    /// Build a payload from the three lock-script components.
    pub fn new_full(
        hash_type: ScriptHashType,
        code_hash: Hash256,
        args: Vec<u8>,
    ) -> AddressPayload {
        Self {
            hash_type,
            code_hash,
            args,
        }
    }

    /// Lock script hash type (`Data`, `Type`, `Data1`, `Data2`).
    pub fn hash_type(&self) -> ScriptHashType {
        self.hash_type
    }

    /// Lock script code hash.
    pub fn code_hash(&self) -> Hash256 {
        self.code_hash
    }

    /// Lock script args.
    pub fn args(&self) -> Vec<u8> {
        self.args.clone()
    }

    /// Bech32m-encode this payload for the given network (`ckb1...` / `ckt1...`).
    pub fn display_with_network(&self, network: &Network) -> String {
        // payload = 0x00 | code_hash | hash_type | args
        let mut data = vec![0u8; 34 + self.args.len()];
        data[0] = 0x00;
        data[1..33].copy_from_slice(&self.code_hash);
        data[33] = self.hash_type as u8;
        data[34..].copy_from_slice(&self.args);
        bech32::encode(network.to_prefix(), data.to_base32(), Variant::Bech32m)
            .unwrap_or_else(|_| panic!("Encode address failed: payload={self:?}"))
    }
}

fn hash_type_name(hash_type: ScriptHashType) -> &'static str {
    match hash_type {
        ScriptHashType::Type => "type",
        ScriptHashType::Data => "data",
        ScriptHashType::Data1 => "data1",
        ScriptHashType::Data2 => "data2",
        _ => "unknown",
    }
}

impl fmt::Debug for AddressPayload {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("AddressPayload")
            .field("hash_type", &hash_type_name(self.hash_type))
            .field("code_hash", &format_hash(&self.code_hash))
            .field("args", &self.args)
            .finish()
    }
}

impl From<&AddressPayload> for Script {
    fn from(payload: &AddressPayload) -> Script {
        Script::new_builder()
            .hash_type(payload.hash_type)
            .code_hash(pack_hash(&payload.code_hash))
            .args(payload.args.pack())
            .build()
    }
}

impl From<Script> for AddressPayload {
    fn from(lock: Script) -> AddressPayload {
        let hash_type_byte: u8 = lock.hash_type().into();
        let hash_type = ScriptHashType::from_repr(hash_type_byte).expect("Invalid hash_type");
        Self {
            hash_type,
            code_hash: unpack_hash(&lock.code_hash()),
            args: lock.args().raw_data().to_vec(),
        }
    }
}

impl From<&AddressPayload> for ScriptEx {
    fn from(value: &AddressPayload) -> Self {
        ScriptEx::Script(value.code_hash, value.hash_type, value.args.clone())
    }
}

/// A CKB address: an [`AddressPayload`] bound to a [`Network`].
///
/// Parses from / formats to the ckb2021 bech32m full-address string via
/// [`FromStr`] / [`fmt::Display`]. Convertible to/from [`Script`] (lock only).
/// `Network::Fake` encodes with the testnet HRP `ckt`; decoding `ckt` yields
/// [`Network::Testnet`].
#[derive(Hash, Eq, PartialEq, Clone)]
pub struct Address {
    network: Network,
    payload: AddressPayload,
}

impl Address {
    /// Bind a payload to a network.
    pub fn new(network: Network, payload: AddressPayload) -> Address {
        Address { network, payload }
    }

    /// Network this address belongs to (decides the bech32m HRP).
    pub fn network(&self) -> &Network {
        &self.network
    }

    /// The lock-script payload carried by this address.
    pub fn payload(&self) -> &AddressPayload {
        &self.payload
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Address")
            .field("network", &self.network)
            .field("hash_type", &hash_type_name(self.payload.hash_type))
            .field("code_hash", &format_hash(&self.payload.code_hash))
            .field("args", &self.payload.args)
            .finish()
    }
}

impl From<&Address> for Script {
    fn from(addr: &Address) -> Script {
        Script::from(addr.payload())
    }
}

impl From<Address> for ScriptEx {
    fn from(value: Address) -> Self {
        value.payload().into()
    }
}

impl ScriptEx {
    /// Full address for this script on `network`.
    ///
    /// A [`ScriptEx::Reference`] has no concrete code hash yet, so conversion
    /// fails until the script is resolved.
    pub fn to_address(self, network: Network) -> Result<Address> {
        let payload = Script::try_from(self)?.into();
        Ok(Address::new(network, payload))
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.payload.display_with_network(&self.network))
    }
}

impl FromStr for Address {
    type Err = String;

    fn from_str(input: &str) -> core::result::Result<Self, Self::Err> {
        let (hrp, data, variant) = bech32::decode(input).map_err(|err| err.to_string())?;
        let network = Network::from_prefix(&hrp).ok_or_else(|| format!("Invalid hrp: {hrp}"))?;
        let data = convert_bits(&data, 5, 8, false).map_err(|err| err.to_string())?;
        if variant != Variant::Bech32m {
            return Err("ckb2021 format full address must use bech32m encoding".to_string());
        }
        if data.len() < 34 {
            return Err(format!("Insufficient data length: {}", data.len()));
        }
        let mut code_hash = [0u8; 32];
        code_hash.copy_from_slice(&data[1..33]);
        let hash_type = ScriptHashType::from_repr(data[33])
            .ok_or_else(|| format!("invalid hash type: {}", data[33]))?;
        let payload = AddressPayload {
            hash_type,
            code_hash,
            args: data[34..].to_vec(),
        };
        Ok(Address { network, payload })
    }
}
