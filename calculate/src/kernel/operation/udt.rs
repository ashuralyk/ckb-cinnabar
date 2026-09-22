//! xUDT (sUDT-compatible) mint output and transfer collect for the kernel.
//!
//! Type-script args are `issuer_lock_hash || extra_args`. Amounts are
//! little-endian `u128` in cell data. Pair with [`crate::kernel::intent::MINT`]
//! / [`crate::kernel::intent::TRANSFER`]. Log key:
//! [`crate::kernel::intent::log::XUDT_AMOUNT`].

use alloc::{boxed::Box, format, string::String, vec::Vec};

use crate::kernel::{
    error::{CalculatorError, Result},
    indexer::{CellQueryOptions, GetCellsIter, SearchKey, SearchMode},
    intent::log::XUDT_AMOUNT,
    network::Network,
    operation::{
        basic::AddCellDep,
        layout::{decode_xudt_amount, encode_xudt_amount, xudt_issuer_args},
        Log, Operation,
    },
    rpc::RPC,
    skeleton::{CellInputEx, CellOutputEx, ScriptEx, TransactionSkeleton},
    source::Source,
    types::{unpack_hash, DepType, Hash256},
};

/// Cell-dep name used in the skeleton for the xUDT contract.
pub const XUDT_NAME: &str = "xudt";

/// Data1 code hash of the xUDT script (same on mainnet and testnet).
pub const XUDT_CODE_HASH: Hash256 = [
    0x50, 0xbd, 0x8d, 0x66, 0x80, 0xb8, 0xb9, 0xcf, 0x98, 0xb7, 0x3f, 0x3c, 0x08, 0xfa, 0xf8, 0xb2,
    0xa2, 0x19, 0x14, 0x31, 0x19, 0x54, 0x11, 0x8a, 0xd6, 0x60, 0x9b, 0xe6, 0xe7, 0x8a, 0x1b, 0x95,
];

/// Genesis out-point tx hash of the xUDT script on mainnet.
pub const XUDT_MAINNET_TX_HASH: Hash256 = [
    0xc0, 0x78, 0x44, 0xce, 0x21, 0xb3, 0x8e, 0x4b, 0x07, 0x1d, 0xd0, 0xe1, 0xee, 0x3b, 0x0e, 0x27,
    0xaf, 0xd8, 0xd7, 0x53, 0x24, 0x91, 0x32, 0x7f, 0x39, 0xb7, 0x86, 0x34, 0x3f, 0x55, 0x8a, 0xb7,
];

/// Genesis out-point tx hash of the xUDT script on testnet.
pub const XUDT_TESTNET_TX_HASH: Hash256 = [
    0xbf, 0x6f, 0xb5, 0x38, 0x76, 0x3e, 0xfe, 0xc2, 0xa7, 0x0a, 0x6a, 0x3d, 0xcb, 0x72, 0x42, 0x78,
    0x70, 0x87, 0xe1, 0x03, 0x0c, 0x4e, 0x7d, 0x86, 0x58, 0x5b, 0xc6, 0x3a, 0x9d, 0x33, 0x7f, 0x5f,
];

/// Sentinel out-point hash used only on fake / custom networks.
pub const XUDT_FAKENET_TX_HASH: Hash256 = [0xd7; 32];

/// xUDT deployment tx hash for `network`.
pub fn xudt_tx_hash(network: &Network) -> Hash256 {
    match network {
        Network::Mainnet => XUDT_MAINNET_TX_HASH,
        Network::Testnet => XUDT_TESTNET_TX_HASH,
        _ => XUDT_FAKENET_TX_HASH,
    }
}

/// xUDT type script for `network`. Fake / custom use a [`ScriptEx::Reference`]
/// resolved from the `"xudt"` cell dep.
pub fn xudt_script(network: &Network, args: Vec<u8>) -> ScriptEx {
    match network {
        Network::Mainnet | Network::Testnet => ScriptEx::new_code(XUDT_CODE_HASH, args),
        _ => (String::from(XUDT_NAME), args).into(),
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
    /// Network used to pick the xUDT type script (canonical hash vs named dep).
    pub network: Network,
}

impl<S: Source> Operation<S> for AddXudtOutputCell {
    fn run(
        self: Box<Self>,
        _source: &S,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let issuer = self.issuer.to_script(skeleton)?;
        let args = xudt_issuer_args(&unpack_hash(&issuer.calc_script_hash()), &self.extra_args);
        let type_script = xudt_script(&self.network, args);
        skeleton.output(CellOutputEx::new_from_scripts(
            self.lock_script.to_script(skeleton)?,
            Some(type_script.to_script(skeleton)?),
            encode_xudt_amount(self.amount),
            None,
        )?);
        log.push((XUDT_AMOUNT, encode_xudt_amount(self.amount)));
        Ok(())
    }
}

/// Add the xUDT contract cell dep for the current network.
pub struct AddXudtCelldep {}

impl<C: RPC> Operation<C> for AddXudtCelldep {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        Box::new(AddCellDep {
            name: String::from(XUDT_NAME),
            tx_hash: xudt_tx_hash(&rpc.network()),
            index: 0,
            dep_type: DepType::Code,
            with_data: false,
        })
        .run(rpc, skeleton, log)
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
    fn search_key(&self, network: &Network, skeleton: &TransactionSkeleton) -> Result<SearchKey> {
        let issuer = self.issuer.clone().to_script(skeleton)?;
        let args = xudt_issuer_args(&unpack_hash(&issuer.calc_script_hash()), &self.extra_args);
        let type_script = xudt_script(network, args).to_script(skeleton)?;
        let mut query = CellQueryOptions::new_lock(self.from.clone().to_script(skeleton)?);
        query.with_data = Some(true);
        query.script_search_mode = Some(SearchMode::Exact);
        query.secondary_script = Some(type_script);
        Ok(query.into())
    }
}

impl<C: RPC> Operation<C> for AddXudtTransferCells {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let mut collected = 0u128;
        let mut search = GetCellsIter::new(rpc, self.search_key(&rpc.network(), skeleton)?);
        while let Some(cell) = search.next()? {
            collected = collected.saturating_add(decode_xudt_amount(&cell.output_data));
            skeleton
                .input(CellInputEx::new_from_live_cell(cell, None))?
                .witness(Default::default());
            if collected >= self.amount {
                break;
            }
        }
        if collected < self.amount {
            if self.throw_if_no_available {
                return Err(CalculatorError::NoAvailableCells(format!(
                    "no available xUDT cells: need {}, collected {}",
                    self.amount, collected
                )));
            }
            return Ok(());
        }
        let issuer = self.issuer.to_script(skeleton)?;
        let args = xudt_issuer_args(&unpack_hash(&issuer.calc_script_hash()), &self.extra_args);
        let type_script = xudt_script(&rpc.network(), args).to_script(skeleton)?;
        skeleton.output(CellOutputEx::new_from_scripts(
            self.to.to_script(skeleton)?,
            Some(type_script.clone()),
            encode_xudt_amount(self.amount),
            None,
        )?);
        let change = collected - self.amount;
        if change > 0 {
            skeleton.output(CellOutputEx::new_from_scripts(
                self.from.to_script(skeleton)?,
                Some(type_script),
                encode_xudt_amount(change),
                None,
            )?);
        }
        log.push((XUDT_AMOUNT, encode_xudt_amount(self.amount)));
        Box::new(AddXudtCelldep {}).run(rpc, skeleton, log)
    }
}
