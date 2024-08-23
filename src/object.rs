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

#[derive(PartialEq, Eq, Clone)]
pub enum TypeIdMode {
    Keep,
    Remove,
    New,
}

impl FromStr for TypeIdMode {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "keep" => Ok(TypeIdMode::Keep),
            "remove" => Ok(TypeIdMode::Remove),
            "new" => Ok(TypeIdMode::New),
            _ => Err(eyre::eyre!("invalid type_id_mode")),
        }
    }
}

impl Display for TypeIdMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = match self {
            TypeIdMode::Keep => "keep",
            TypeIdMode::Remove => "remove",
            TypeIdMode::New => "new",
        };
        write!(f, "{inner}",)
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
    pub tx_hash: H256,
    pub out_index: u32,
    pub data_hash: Option<H256>,
    pub occupied_capacity: u64,
    pub payer_address: CkbAddress,
    pub contract_owner_address: CkbAddress,
    pub type_id: Option<H256>,
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
