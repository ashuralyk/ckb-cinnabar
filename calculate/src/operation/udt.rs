//! xUDT (sUDT-compatible) mint and transfer operations.
//!
//! Type-script args are `issuer_lock_hash || extra_args`. Amounts are
//! little-endian `u128` in cell data. Pair with [`crate::intent::MINT`] /
//! [`crate::intent::TRANSFER`]. Log key: [`crate::intent::log::XUDT_AMOUNT`].

use async_trait::async_trait;
use ckb_types::{core::DepType, h256, packed::Script, H256};
use eyre::{eyre, Result};

use crate::{
    indexer::{CellQueryOptions, SearchKey, SearchMode},
    intent,
    operation::{Log, Operation},
    rpc::{GetCellsIter, Network, RPC},
    skeleton::{CellInputEx, CellOutputEx, ScriptEx, TransactionSkeleton},
};

use super::basic::AddCellDep;

/// Hardcoded xUDT deployment out-points and code hash per network.
///
/// On `Fake`/custom networks the tx hash is random and the script becomes a
/// `ScriptEx::Reference` resolved from a fake cell dep.
pub mod hardcoded {
    use crate::simulation::random_hash;

    use super::*;

    /// Cell-dep name used in the skeleton for the xUDT contract.
    pub const XUDT_NAME: &str = "xudt";
    /// Genesis out-point tx hash of the xUDT script on mainnet.
    pub const XUDT_MAINNET_TX_HASH: H256 =
        h256!("0xc07844ce21b38e4b071dd0e1ee3b0e27afd8d7532491327f39b786343f558ab7");
    /// Genesis out-point tx hash of the xUDT script on testnet.
    pub const XUDT_TESTNET_TX_HASH: H256 =
        h256!("0xbf6fb538763efec2a70a6a3dcb7242787087e1030c4e7d86585bc63a9d337f5f");

    lazy_static::lazy_static! {
        pub static ref XUDT_FAKENET_TX_HASH: H256 = random_hash().into();
    }

    /// Data1 code hash of the xUDT script (same on mainnet and testnet).
    pub const XUDT_CODE_HASH: H256 =
        h256!("0x50bd8d6680b8b9cf98b73f3c08faf8b2a21914311954118ad6609be6e78a1b95");

    /// xUDT deployment tx hash for `network` (random under fake networks).
    pub fn xudt_tx_hash(network: Network) -> H256 {
        match network {
            Network::Mainnet => XUDT_MAINNET_TX_HASH,
            Network::Testnet => XUDT_TESTNET_TX_HASH,
            _ => XUDT_FAKENET_TX_HASH.clone(),
        }
    }

    /// xUDT type script for `network`. Fake/custom networks use a
    /// `ScriptEx::Reference` resolved from the `"xudt"` cell dep.
    pub fn xudt_script(network: Network, args: Vec<u8>) -> ScriptEx {
        match network {
            Network::Mainnet | Network::Testnet => ScriptEx::new_code(XUDT_CODE_HASH, args),
            _ => (XUDT_NAME.to_string(), args).into(),
        }
    }
}

fn issuer_args(issuer: &Script, extra_args: &[u8]) -> Vec<u8> {
    let mut args = issuer.calc_script_hash().raw_data().to_vec();
    args.extend_from_slice(extra_args);
    args
}

/// xUDT amount → little-endian u128 cell data.
fn encode_amount(amount: u128) -> Vec<u8> {
    amount.to_le_bytes().to_vec()
}

/// Decode the little-endian u128 amount from cell data (missing bytes = 0).
fn decode_amount(data: &[u8]) -> u128 {
    let mut buf = [0u8; 16];
    let n = data.len().min(16);
    buf[..n].copy_from_slice(&data[..n]);
    u128::from_le_bytes(buf)
}

/// Add the xUDT contract cell dep for the current network.
pub struct AddXudtCelldep {}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddXudtCelldep {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        Box::new(AddCellDep {
            name: hardcoded::XUDT_NAME.to_string(),
            tx_hash: hardcoded::xudt_tx_hash(rpc.network()),
            index: 0,
            dep_type: DepType::Code,
            with_data: false,
        })
        .run(rpc, skeleton, log)
        .await
    }
}

/// Create an xUDT output cell for `lock_script` (holder). Type args = issuer lock hash + extra.
pub struct AddXudtOutputCell {
    /// Lock script of the holder receiving the tokens.
    pub lock_script: ScriptEx,
    /// Issuer lock script; its hash prefixes the type args.
    pub issuer: ScriptEx,
    /// Token amount (little-endian u128 in cell data).
    pub amount: u128,
    /// Extra bytes appended to the type args after the issuer hash.
    pub extra_args: Vec<u8>,
}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddXudtOutputCell {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let issuer = self.issuer.to_script(skeleton)?;
        let args = issuer_args(&issuer, &self.extra_args);
        let type_script = hardcoded::xudt_script(rpc.network(), args);
        skeleton.output(CellOutputEx::new_from_scripts(
            self.lock_script.to_script(skeleton)?,
            Some(type_script.to_script(skeleton)?),
            encode_amount(self.amount),
            None,
        )?);
        log.push((intent::log::XUDT_AMOUNT, encode_amount(self.amount)));
        Box::new(AddXudtCelldep {}).run(rpc, skeleton, log).await
    }
}

/// Consume xUDT cells from `from` and emit `to` + optional change.
pub struct AddXudtTransferCells {
    /// Lock script of the sender cells (also receives change).
    pub from: ScriptEx,
    /// Lock script of the receiver.
    pub to: ScriptEx,
    /// Issuer lock script identifying the token.
    pub issuer: ScriptEx,
    /// Amount to transfer.
    pub amount: u128,
    /// Extra bytes appended to the type args after the issuer hash.
    pub extra_args: Vec<u8>,
    /// Fail when collected inputs cannot cover `amount` (otherwise no-op).
    pub throw_if_no_available: bool,
}

impl AddXudtTransferCells {
    fn search_key(&self, network: Network, skeleton: &TransactionSkeleton) -> Result<SearchKey> {
        let issuer = self.issuer.clone().to_script(skeleton)?;
        let args = issuer_args(&issuer, &self.extra_args);
        let type_script = hardcoded::xudt_script(network, args).to_script(skeleton)?;
        let mut query = CellQueryOptions::new_lock(self.from.clone().to_script(skeleton)?);
        query.with_data = Some(true);
        query.script_search_mode = Some(SearchMode::Exact);
        query.secondary_script = Some(type_script);
        Ok(query.into())
    }
}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddXudtTransferCells {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let mut collected = 0u128;
        let mut search = GetCellsIter::new(rpc, self.search_key(rpc.network(), skeleton)?);
        while let Some(cell) = search.next().await? {
            let bytes = cell
                .output_data
                .as_ref()
                .map(|d| d.as_bytes().to_vec())
                .unwrap_or_default();
            let amount = decode_amount(&bytes);
            collected = collected.saturating_add(amount);
            skeleton
                .input(CellInputEx::new_from_indexer_cell(cell, None))?
                .witness(Default::default());
            if collected >= self.amount {
                break;
            }
        }
        if collected < self.amount {
            if self.throw_if_no_available {
                return Err(eyre!(
                    "no available xUDT cells: need {}, collected {}",
                    self.amount,
                    collected
                ));
            }
            return Ok(());
        }
        let issuer = self.issuer.to_script(skeleton)?;
        let args = issuer_args(&issuer, &self.extra_args);
        let type_script = hardcoded::xudt_script(rpc.network(), args).to_script(skeleton)?;
        skeleton.output(CellOutputEx::new_from_scripts(
            self.to.to_script(skeleton)?,
            Some(type_script.clone()),
            encode_amount(self.amount),
            None,
        )?);
        let change = collected - self.amount;
        if change > 0 {
            skeleton.output(CellOutputEx::new_from_scripts(
                self.from.to_script(skeleton)?,
                Some(type_script),
                encode_amount(change),
                None,
            )?);
        }
        log.push((intent::log::XUDT_AMOUNT, encode_amount(self.amount)));
        Box::new(AddXudtCelldep {}).run(rpc, skeleton, log).await
    }
}
