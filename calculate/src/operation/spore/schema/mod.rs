//! Spore, Cluster, and cobuild layouts encoded with `serde_molecule`.
//!
//! These types replace the generated molecule Entity/Builder/Reader stack.
//! Field order matches the `.mol` files in this directory. `ActionVec` is a
//! molecule dynvec of tables, so [`Message::actions`] is annotated with
//! `serde_molecule::dynvec_serde`.
//!
//! `WitnessLayout` uses cobuild's customized union ids (`0xFF000001` …), not
//! sequential variant indexes. [`SighashAll`] puts `seal` before `message` to
//! match deployed spore/cluster binaries (cobuild-poc), as documented in
//! `cobuild.mol`.

use ckb_types::prelude::Unpack;
use eyre::{eyre, Result};
use serde::{de, ser, Deserialize, Serialize};
use serde_molecule::{de::MoleculeDeserializer, from_slice, struct_serde::CollectData, to_vec};

/// cobuild `WitnessLayout` item id for `SighashAll` (`4278190081`).
pub const WITNESS_LAYOUT_SIGHASH_ALL: u32 = 0xFF00_0001;

/// Encode `value` as a molecule table (or union, for [`WitnessLayout`]).
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    to_vec(value, false).map_err(|err| eyre!("molecule encode failed: {err}"))
}

/// Decode a molecule table (compatible extra fields allowed).
pub fn decode<'de, T: Deserialize<'de>>(bytes: &'de [u8]) -> Result<T> {
    from_slice(bytes, false).map_err(|err| eyre!("molecule decode failed: {err}"))
}

/// `table SporeData` in `spore.mol`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SporeData {
    /// MIME-like content type bytes, e.g. `text/plain`.
    pub content_type: Vec<u8>,
    /// Raw spore content.
    pub content: Vec<u8>,
    /// Parent cluster id (`BytesOpt`); `None` is a standalone spore.
    pub cluster_id: Option<Vec<u8>>,
}

/// `table ClusterDataV2` in `spore.mol`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClusterDataV2 {
    /// Cluster display name.
    pub name: Vec<u8>,
    /// Cluster description bytes.
    pub description: Vec<u8>,
    /// Optional mutant id; mint helpers leave this empty.
    pub mutant_id: Option<Vec<u8>>,
}

/// `table Script` used by Spore cobuild `Address`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Script {
    pub code_hash: [u8; 32],
    pub hash_type: u8,
    pub args: Vec<u8>,
}

impl From<ckb_types::packed::Script> for Script {
    fn from(script: ckb_types::packed::Script) -> Self {
        Self {
            code_hash: script.code_hash().unpack(),
            hash_type: script.hash_type().into(),
            args: script.args().raw_data().to_vec(),
        }
    }
}

/// `union Address { Script }` in `action.mol`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Address {
    Script(Script),
}

impl From<ckb_types::packed::Script> for Address {
    fn from(script: ckb_types::packed::Script) -> Self {
        Address::Script(script.into())
    }
}

/// `table MintSpore` in `action.mol`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MintSpore {
    pub spore_id: [u8; 32],
    pub to: Address,
    pub data_hash: [u8; 32],
}

/// `table TransferSpore` in `action.mol`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferSpore {
    pub spore_id: [u8; 32],
    pub from: Address,
    pub to: Address,
}

/// `table BurnSpore` in `action.mol`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BurnSpore {
    pub spore_id: [u8; 32],
    pub from: Address,
}

/// `table MintCluster` in `action.mol`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MintCluster {
    pub cluster_id: [u8; 32],
    pub to: Address,
    pub data_hash: [u8; 32],
}

/// `table TransferCluster` in `action.mol`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferCluster {
    pub cluster_id: [u8; 32],
    pub from: Address,
    pub to: Address,
}

/// `union SporeAction` in `action.mol`. Variant order is the molecule item id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SporeAction {
    MintSpore(MintSpore),
    TransferSpore(TransferSpore),
    BurnSpore(BurnSpore),
    MintCluster(MintCluster),
    TransferCluster(TransferCluster),
}

/// cobuild `table Action`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    pub script_info_hash: [u8; 32],
    pub script_hash: [u8; 32],
    pub data: Vec<u8>,
}

impl Action {
    /// Bind `spore_action` to `script` (`script_info_hash` left zeroed).
    pub fn spore(script: &ckb_types::packed::Script, spore_action: &SporeAction) -> Result<Self> {
        Ok(Self {
            script_info_hash: [0u8; 32],
            script_hash: script.calc_script_hash().unpack(),
            data: encode(spore_action)?,
        })
    }
}

/// cobuild `table Message`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    #[serde(with = "serde_molecule::dynvec_serde")]
    pub actions: Vec<Action>,
}

/// cobuild `table SighashAll` as deployed by spore/cluster (seal then message).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SighashAll {
    pub seal: Vec<u8>,
    pub message: Message,
}

/// cobuild `union WitnessLayout` with customized item ids. Assembly only
/// writes [`WitnessLayout::SighashAll`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WitnessLayout {
    SighashAll(SighashAll),
}

impl Serialize for WitnessLayout {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> core::result::Result<S::Ok, S::Error> {
        match self {
            WitnessLayout::SighashAll(inner) => {
                let mut data = WITNESS_LAYOUT_SIGHASH_ALL.to_le_bytes().to_vec();
                data.extend(
                    to_vec(inner, false)
                        .map_err(|_| ser::Error::custom("failed to serialize SighashAll"))?,
                );
                serializer.serialize_bytes(&data)
            }
        }
    }
}

impl<'de> Deserialize<'de> for WitnessLayout {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> core::result::Result<Self, D::Error> {
        let data = CollectData::deserialize(deserializer)?.data;
        if data.len() < 4 {
            return Err(de::Error::custom("WitnessLayout too short"));
        }
        let id = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let mut de = MoleculeDeserializer::new(&data[4..]);
        match id {
            WITNESS_LAYOUT_SIGHASH_ALL => {
                let inner = SighashAll::deserialize(&mut de).map_err(de::Error::custom)?;
                Ok(WitnessLayout::SighashAll(inner))
            }
            other => Err(de::Error::custom(format!(
                "unsupported WitnessLayout id {other}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spore_data_encodes_as_molecule_table() {
        let data = SporeData {
            content_type: b"text/plain".to_vec(),
            content: b"hi".to_vec(),
            cluster_id: None,
        };
        let bytes = encode(&data).unwrap();
        // table header (total + 3 offsets) + Bytes("text/plain") + Bytes("hi") + empty option
        let expected = [
            36, 0, 0, 0, 16, 0, 0, 0, 30, 0, 0, 0, 36, 0, 0, 0, 10, 0, 0, 0, b't', b'e', b'x',
            b't', b'/', b'p', b'l', b'a', b'i', b'n', 2, 0, 0, 0, b'h', b'i',
        ];
        assert_eq!(bytes, expected);
        assert_eq!(decode::<SporeData>(&bytes).unwrap(), data);
    }

    #[test]
    fn cluster_data_v2_roundtrip_empty_mutant() {
        let data = ClusterDataV2 {
            name: b"n".to_vec(),
            description: b"d".to_vec(),
            mutant_id: None,
        };
        let bytes = encode(&data).unwrap();
        assert_eq!(decode::<ClusterDataV2>(&bytes).unwrap(), data);
    }

    #[test]
    fn witness_layout_uses_custom_union_id_and_seal_first() {
        let layout = WitnessLayout::SighashAll(SighashAll {
            seal: vec![],
            message: Message { actions: vec![] },
        });
        let bytes = encode(&layout).unwrap();
        assert_eq!(&bytes[..4], &WITNESS_LAYOUT_SIGHASH_ALL.to_le_bytes());
        // SighashAll table: seal (empty fixvec) then message (empty ActionVec)
        let sighash_all = &bytes[4..];
        assert_eq!(
            sighash_all,
            &[
                28, 0, 0, 0, 12, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 8, 0, 0, 0, 4, 0,
                0, 0,
            ]
        );
        assert_eq!(decode::<WitnessLayout>(&bytes).unwrap(), layout);
    }

    #[test]
    fn spore_action_union_ids_are_sequential() {
        let mint = SporeAction::MintSpore(MintSpore {
            spore_id: [1u8; 32],
            to: Address::Script(Script::default()),
            data_hash: [2u8; 32],
        });
        let bytes = encode(&mint).unwrap();
        assert_eq!(&bytes[..4], &0u32.to_le_bytes());

        let transfer_cluster = SporeAction::TransferCluster(TransferCluster {
            cluster_id: [3u8; 32],
            from: Address::Script(Script::default()),
            to: Address::Script(Script::default()),
        });
        let bytes = encode(&transfer_cluster).unwrap();
        assert_eq!(&bytes[..4], &4u32.to_le_bytes());
    }
}
