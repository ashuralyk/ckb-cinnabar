use std::{fmt::Display, str::FromStr};

use ckb_cinnabar_calculator::{
    re_exports::{
        ckb_sdk,
        ckb_types::{core, packed, prelude::*, H256},
        eyre,
    },
    skeleton::ScriptEx,
};
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq)]
pub enum TypeIdMode {
    Keep,
    Remove,
    New,
}

impl TryFrom<String> for TypeIdMode {
    type Error = eyre::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "keep" => Ok(TypeIdMode::Keep),
            "remove" => Ok(TypeIdMode::Remove),
            "new" => Ok(TypeIdMode::New),
            _ => Err(eyre::eyre!("invalid type_id_mode")),
        }
    }
}

#[derive(PartialEq, Eq)]
pub enum ListMode {
    All,
    Deployed,
    Consumed,
}

impl TryFrom<String> for ListMode {
    type Error = eyre::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "all" => Ok(ListMode::All),
            "deployed" => Ok(ListMode::Deployed),
            "consumed" => Ok(ListMode::Consumed),
            _ => Err(eyre::eyre!("invalid list mode")),
        }
    }
}

#[derive(Clone, Default)]
pub struct Hash256(H256);

impl From<Hash256> for H256 {
    fn from(value: Hash256) -> Self {
        value.0
    }
}

impl From<H256> for Hash256 {
    fn from(value: H256) -> Self {
        Hash256(value)
    }
}

impl FromStr for Hash256 {
    type Err = eyre::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        hex::decode(value.trim_start_matches("0x"))
            .map_err(|_| eyre::eyre!("invalid hex string"))
            .and_then(|bytes| {
                if bytes.len() != 32 {
                    Err(eyre::eyre!("invalid hash length"))
                } else {
                    let mut inner = [0u8; 32];
                    inner.copy_from_slice(&bytes);
                    Ok(Hash256(H256::from(inner)))
                }
            })
    }
}

impl Display for Hash256 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

impl<'de> Deserialize<'de> for Hash256 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

impl Serialize for Hash256 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

#[derive(Clone, Default)]
pub struct CkbAddress(Option<ckb_sdk::Address>);

impl TryFrom<CkbAddress> for ckb_sdk::Address {
    type Error = eyre::Error;

    fn try_from(value: CkbAddress) -> Result<Self, Self::Error> {
        value.0.ok_or_else(|| eyre::eyre!("empty ckb address"))
    }
}

impl From<ckb_sdk::Address> for CkbAddress {
    fn from(value: ckb_sdk::Address) -> Self {
        CkbAddress(Some(value))
    }
}

impl From<Option<ckb_sdk::Address>> for CkbAddress {
    fn from(value: Option<ckb_sdk::Address>) -> Self {
        CkbAddress(value)
    }
}

impl FromStr for CkbAddress {
    type Err = eyre::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        ckb_sdk::Address::from_str(value)
            .map(|v| CkbAddress(Some(v)))
            .map_err(|_| eyre::eyre!("invalid ckb address"))
    }
}

impl Display for CkbAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.0.as_ref().map(|v| v.to_string()).unwrap_or_default();
        write!(f, "{inner}",)
    }
}

impl<'de> Deserialize<'de> for CkbAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if value.is_empty() {
            Ok(CkbAddress(None))
        } else {
            value.parse().map_err(serde::de::Error::custom)
        }
    }
}

impl Serialize for CkbAddress {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
pub struct DeploymentRecord {
    pub name: String,
    pub date: String,
    pub operation: String,
    pub version: String,
    pub tx_hash: Hash256,
    pub out_index: u32,
    pub data_hash: Option<Hash256>,
    pub occupied_capacity: u64,
    pub payer_address: CkbAddress,
    pub contract_owner_address: CkbAddress,
    pub type_id: Option<Hash256>,
    // This field is not required, so you can edit in your <contract>.json file to add comment for cooperations
    #[serde(default, rename = "__comment")]
    pub comment: Option<String>,
}

impl DeploymentRecord {
    pub fn generate_script(&self, args: Vec<u8>) -> eyre::Result<ScriptEx> {
        let mut script = packed::Script::new_builder().args(args.pack());
        if let Some(type_id) = self.type_id.clone() {
            script = script
                .code_hash(type_id.0.pack())
                .hash_type(core::ScriptHashType::Type.into());
        } else {
            let Some(data_hash) = self.data_hash.clone() else {
                return Err(eyre::eyre!("contract consumed"));
            };
            script = script
                .code_hash(data_hash.0.pack())
                .hash_type(core::ScriptHashType::Data2.into());
        }
        Ok(script.build().into())
    }
}
