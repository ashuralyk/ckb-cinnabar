//! Shared byte layouts used by the kernel.
//!
//! These helpers are pure `alloc` math: xUDT amounts and DAO deposit data.
//! Spore / Cluster / cobuild molecule tables live in
//! [`crate::kernel::operation::spore::schema`] when the `spore` feature is on.

use alloc::vec::Vec;

/// xUDT amount → little-endian `u128` cell data.
pub fn encode_xudt_amount(amount: u128) -> Vec<u8> {
    amount.to_le_bytes().to_vec()
}

/// Decode the little-endian `u128` amount from cell data (missing bytes = 0).
pub fn decode_xudt_amount(data: &[u8]) -> u128 {
    let mut buf = [0u8; 16];
    let n = data.len().min(16);
    buf[..n].copy_from_slice(&data[..n]);
    u128::from_le_bytes(buf)
}

/// xUDT type-script args: `issuer_lock_hash || extra_args`.
pub fn xudt_issuer_args(issuer_lock_hash: &[u8], extra_args: &[u8]) -> Vec<u8> {
    let mut args = issuer_lock_hash.to_vec();
    args.extend_from_slice(extra_args);
    args
}

/// DAO deposit cell data: eight zero bytes.
pub fn dao_deposit_data() -> Vec<u8> {
    [0u8; 8].to_vec()
}

/// Encode a molecule `table` from already-encoded field bodies.
pub fn encode_molecule_table(fields: &[&[u8]]) -> Vec<u8> {
    let header_len = 4 * (1 + fields.len());
    let mut offsets = Vec::with_capacity(fields.len());
    let mut cursor = header_len;
    for field in fields {
        offsets.push(cursor as u32);
        cursor += field.len();
    }
    let mut out = Vec::with_capacity(cursor);
    out.extend_from_slice(&(cursor as u32).to_le_bytes());
    for offset in offsets {
        out.extend_from_slice(&offset.to_le_bytes());
    }
    for field in fields {
        out.extend_from_slice(field);
    }
    out
}

/// Molecule `Bytes`: `u32le(len) || data`.
pub fn encode_molecule_bytes(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + data.len());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    out
}

/// Molecule `BytesOpt`: empty if `None`, otherwise the inner `Bytes`.
pub fn encode_molecule_bytes_opt(data: Option<&[u8]>) -> Vec<u8> {
    match data {
        Some(bytes) => encode_molecule_bytes(bytes),
        None => Vec::new(),
    }
}

/// Encode `SporeData` (content type, content, optional cluster id).
pub fn make_spore_data(
    content_type: &str,
    content: &[u8],
    cluster_id: Option<&[u8; 32]>,
) -> Vec<u8> {
    let content_type = encode_molecule_bytes(content_type.as_bytes());
    let content = encode_molecule_bytes(content);
    let cluster = encode_molecule_bytes_opt(cluster_id.map(|id| id.as_slice()));
    encode_molecule_table(&[&content_type, &content, &cluster])
}

/// Encode `ClusterDataV2` (name, description, empty mutant id).
pub fn make_cluster_data(name: &str, description: &[u8]) -> Vec<u8> {
    let name = encode_molecule_bytes(name.as_bytes());
    let description = encode_molecule_bytes(description);
    let mutant = encode_molecule_bytes_opt(None);
    encode_molecule_table(&[&name, &description, &mutant])
}

/// Encode cobuild `table Action` (`Byte32`, `Byte32`, `Bytes`).
pub fn encode_cobuild_action(
    script_info_hash: &[u8; 32],
    script_hash: &[u8; 32],
    data: &[u8],
) -> Vec<u8> {
    let data = encode_molecule_bytes(data);
    encode_molecule_table(&[script_info_hash.as_slice(), script_hash.as_slice(), &data])
}
