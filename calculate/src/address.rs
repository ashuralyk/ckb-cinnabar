use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::str::FromStr;

use bech32::{self, convert_bits, ToBase32, Variant};
use ckb_types::{
    bytes::Bytes,
    core::ScriptHashType,
    packed::{Byte32, Script},
    prelude::*,
};

use crate::rpc::Network;

#[derive(Hash, Eq, PartialEq, Clone)]
pub struct AddressPayload {
    hash_type: ScriptHashType,
    code_hash: Byte32,
    args: Bytes,
}

impl AddressPayload {
    pub fn new_full(hash_type: ScriptHashType, code_hash: Byte32, args: Bytes) -> AddressPayload {
        Self {
            hash_type,
            code_hash,
            args,
        }
    }

    pub fn hash_type(&self) -> ScriptHashType {
        self.hash_type
    }

    pub fn code_hash(&self) -> Byte32 {
        self.code_hash.clone()
    }

    pub fn args(&self) -> Bytes {
        self.args.clone()
    }

    pub fn display_with_network(&self, network: &Network) -> String {
        // payload = 0x00 | code_hash | hash_type | args
        let code_hash = self.code_hash();
        let hash_type = self.hash_type();
        let args = self.args();
        let mut data = vec![0u8; 34 + args.len()];
        data[0] = 0x00;
        data[1..33].copy_from_slice(code_hash.as_slice());
        data[33] = hash_type as u8;
        data[34..].copy_from_slice(args.as_ref());
        bech32::encode(
            network.to_prefix(),
            data.to_base32(),
            bech32::Variant::Bech32m,
        )
        .unwrap_or_else(|_| panic!("Encode address failed: payload={:?}", self))
    }
}

impl fmt::Debug for AddressPayload {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let hash_type = match self.hash_type() {
            ScriptHashType::Type => "type",
            ScriptHashType::Data => "data",
            ScriptHashType::Data1 => "data1",
            ScriptHashType::Data2 => "data2",
        };
        f.debug_struct("AddressPayload")
            .field("hash_type", &hash_type)
            .field("code_hash", &self.code_hash())
            .field("args", &self.args())
            .finish()
    }
}

impl From<&AddressPayload> for Script {
    fn from(payload: &AddressPayload) -> Script {
        Script::new_builder()
            .hash_type(payload.hash_type().into())
            .code_hash(payload.code_hash())
            .args(payload.args().pack())
            .build()
    }
}

impl From<Script> for AddressPayload {
    fn from(lock: Script) -> AddressPayload {
        let hash_type: ScriptHashType = lock.hash_type().try_into().expect("Invalid hash_type");
        let code_hash = lock.code_hash();
        let args = lock.args().raw_data();
        Self {
            hash_type,
            code_hash,
            args,
        }
    }
}

#[derive(Hash, Eq, PartialEq, Clone)]
pub struct Address {
    network: Network,
    payload: AddressPayload,
}

impl Address {
    pub fn new(network: Network, payload: AddressPayload) -> Address {
        Address { network, payload }
    }

    pub fn network(&self) -> &Network {
        &self.network
    }

    pub fn payload(&self) -> &AddressPayload {
        &self.payload
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let hash_type = match self.payload.hash_type() {
            ScriptHashType::Type => "type",
            ScriptHashType::Data => "data",
            ScriptHashType::Data1 => "data1",
            ScriptHashType::Data2 => "data2",
        };
        f.debug_struct("Address")
            .field("network", &self.network)
            .field("hash_type", &hash_type)
            .field("code_hash", &self.payload.code_hash())
            .field("args", &self.payload.args())
            .finish()
    }
}

impl From<&Address> for Script {
    fn from(addr: &Address) -> Script {
        Script::new_builder()
            .hash_type(addr.payload.hash_type().into())
            .code_hash(addr.payload.code_hash())
            .args(addr.payload.args().pack())
            .build()
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}", self.payload.display_with_network(&self.network))
    }
}

impl FromStr for Address {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let (hrp, data, variant) = bech32::decode(input).map_err(|err| err.to_string())?;
        let network = Network::from_prefix(&hrp).ok_or_else(|| format!("Invalid hrp: {}", hrp))?;
        let data = convert_bits(&data, 5, 8, false).unwrap();
        if variant != Variant::Bech32m {
            return Err("ckb2021 format full address must use bech32m encoding".to_string());
        }
        if data.len() < 34 {
            return Err(format!("Insufficient data length: {}", data.len()));
        }
        let code_hash = Byte32::from_slice(&data[1..33]).unwrap();
        let hash_type = ScriptHashType::try_from(data[33]).map_err(|err| err.to_string())?;
        let args = Bytes::from(data[34..].to_vec());
        let payload = AddressPayload {
            hash_type,
            code_hash,
            args,
        };
        Ok(Address { network, payload })
    }
}
