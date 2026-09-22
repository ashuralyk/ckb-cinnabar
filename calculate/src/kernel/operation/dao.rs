//! Nervos DAO deposit output and phase-one collect for the kernel.
//!
//! Phase-two withdraw math uses `ckb-sdk` and stays on the host
//! (`AddDaoWithdrawPhaseTwoCells` in the shell). Pair with
//! `dao_deposit` / `dao_withdraw_phase_one` in `crate::instruction::predefined`.

use alloc::{boxed::Box, string::String, vec::Vec};

use crate::kernel::{
    error::{CalculatorError, Result},
    indexer::{CellQueryOptions, GetCellsIter, SearchKey, SearchKeyFilter, SearchMode},
    intent::log::DAO_WITHDRAW_PHASE_ONE,
    network::Network,
    operation::{basic::AddCellDep, layout::dao_deposit_data, Log, Operation},
    rpc::RPC,
    skeleton::{CellInputEx, CellOutputEx, HeaderDepEx, ScriptEx, TransactionSkeleton},
    source::Source,
    types::{DepType, Hash256, Unpack},
};

/// Cell-dep name used in the skeleton for the DAO script.
pub const DAO_NAME: &str = "dao";

/// Type hash of the Nervos DAO type script (all networks).
pub const DAO_TYPE_HASH: Hash256 = [
    0x82, 0xd7, 0x6d, 0x1b, 0x75, 0xfe, 0x2f, 0xd9, 0xa2, 0x7d, 0xfb, 0xaa, 0x65, 0xa0, 0x39, 0x22,
    0x1a, 0x38, 0x0d, 0x76, 0xc9, 0x26, 0xf3, 0x78, 0xd3, 0xf8, 0x1c, 0xf3, 0xe7, 0xe1, 0x3f, 0x2e,
];

/// Genesis out-point tx hash of the DAO script on mainnet.
pub const DAO_MAINNET_TX_HASH: Hash256 = [
    0xe2, 0xfb, 0x19, 0x98, 0x10, 0xd4, 0x9a, 0x4d, 0x8b, 0xee, 0xc5, 0x67, 0x18, 0xba, 0x25, 0x93,
    0xb6, 0x65, 0xdb, 0x9d, 0x52, 0x29, 0x9a, 0x0f, 0x9e, 0x6e, 0x75, 0x41, 0x6d, 0x73, 0xff, 0x5c,
];

/// Genesis out-point tx hash of the DAO script on testnet.
pub const DAO_TESTNET_TX_HASH: Hash256 = [
    0x8f, 0x8c, 0x79, 0xeb, 0x66, 0x71, 0x70, 0x96, 0x33, 0xfe, 0x6a, 0x46, 0xde, 0x93, 0xc0, 0xfe,
    0xdc, 0x9c, 0x1b, 0x8a, 0x65, 0x27, 0xa1, 0x8d, 0x39, 0x83, 0x87, 0x95, 0x42, 0x63, 0x5c, 0x9f,
];

/// Sentinel out-point hash used only on fake / custom networks.
pub const DAO_FAKENET_TX_HASH: Hash256 = [0xda; 32];

/// DAO deployment tx hash for `network`.
pub fn dao_tx_hash(network: &Network) -> Hash256 {
    match network {
        Network::Mainnet => DAO_MAINNET_TX_HASH,
        Network::Testnet => DAO_TESTNET_TX_HASH,
        _ => DAO_FAKENET_TX_HASH,
    }
}

/// DAO type script for `network`. Fake / custom use a [`ScriptEx::Reference`]
/// resolved from the `"dao"` cell dep.
pub fn dao_script(network: &Network) -> ScriptEx {
    match network {
        Network::Mainnet | Network::Testnet => ScriptEx::new_type(DAO_TYPE_HASH, Vec::new()),
        _ => (String::from(DAO_NAME), Vec::new()).into(),
    }
}

/// Add a DAO deposit output (8 zero bytes of data + DAO type script).
///
/// Capacity is absolute (shannons).
pub struct AddDaoDepositOutputCell {
    /// Lock script that owns the deposit.
    pub owner: ScriptEx,
    /// Total capacity of the deposit cell, in shannons.
    pub deposit_capacity: u64,
    /// Network used to pick the DAO type script (canonical hash vs named dep).
    pub network: Network,
}

impl<S: Source> Operation<S> for AddDaoDepositOutputCell {
    fn run(
        self: Box<Self>,
        _source: &S,
        skeleton: &mut TransactionSkeleton,
        _log: &mut Log,
    ) -> Result<()> {
        let dao_type = dao_script(&self.network);
        skeleton.output(CellOutputEx::new_from_scripts(
            self.owner.to_script(skeleton)?,
            Some(dao_type.to_script(skeleton)?),
            dao_deposit_data(),
            Some(self.deposit_capacity),
        )?);
        Ok(())
    }
}

/// Add the Nervos DAO type script as a cell dep.
pub struct AddDaoCelldep {}

impl<C: RPC> Operation<C> for AddDaoCelldep {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        Box::new(AddCellDep {
            name: String::from(DAO_NAME),
            tx_hash: dao_tx_hash(&rpc.network()),
            index: 2,
            dep_type: DepType::Code,
            with_data: false,
        })
        .run(rpc, skeleton, log)
    }
}

/// Collect mature DAO deposits and emit phase-one withdraw cells.
pub struct AddDaoWithdrawPhaseOneCells {
    /// Stop searching once this much deposit capacity is collected.
    pub maximal_withdraw_capacity: u64,
    /// Only deposits made at or before this timestamp are picked.
    pub upperbound_timestamp: u64,
    /// Lock script that owns the deposit cells.
    pub owner: ScriptEx,
    /// Lock script of the phase-one withdraw cells; defaults to the deposit
    /// cell's own lock when `None`.
    pub transfer_to: Option<ScriptEx>,
    /// Fail when no mature deposit cells are found.
    pub throw_if_no_available: bool,
}

impl AddDaoWithdrawPhaseOneCells {
    fn search_key(&self, network: &Network, skeleton: &TransactionSkeleton) -> Result<SearchKey> {
        let dao_type = dao_script(network).to_script(skeleton)?;
        let mut search_key: SearchKey =
            CellQueryOptions::new_lock(self.owner.clone().to_script(skeleton)?).into();
        search_key.with_data = Some(true);
        search_key.filter = Some(SearchKeyFilter {
            script: Some(dao_type),
            output_data: Some(dao_deposit_data()),
            output_data_filter_mode: Some(SearchMode::Exact),
            ..Default::default()
        });
        Ok(search_key)
    }

    fn check_deposit_timestamp<C: RPC>(&self, rpc: &C, deposit_block_number: u64) -> Result<bool> {
        let Some(header) = rpc.get_header_by_number(deposit_block_number)? else {
            return Ok(false);
        };
        let timestamp: u64 = header.raw().timestamp().unpack();
        Ok(timestamp <= self.upperbound_timestamp)
    }
}

impl<C: RPC> Operation<C> for AddDaoWithdrawPhaseOneCells {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let mut searched_capacity = 0u64;
        let mut search = GetCellsIter::new(rpc, self.search_key(&rpc.network(), skeleton)?);
        let transfer_lock_script = if let Some(transfer_to) = self.transfer_to.clone() {
            Some(transfer_to.to_script(skeleton)?)
        } else {
            None
        };
        while let Some(cell) = search.next()? {
            if !self.check_deposit_timestamp(rpc, cell.block_number)? {
                continue;
            }
            let deposit_cell = CellInputEx::new_from_live_cell(cell, None);
            let capacity = deposit_cell.output.capacity_shannons();
            searched_capacity += capacity;
            if searched_capacity >= self.maximal_withdraw_capacity {
                break;
            }
            let deposit_header_dep =
                HeaderDepEx::new_from_outpoint(rpc, deposit_cell.input.previous_output())?;
            let withdraw_cell = CellOutputEx::new_from_scripts(
                transfer_lock_script
                    .clone()
                    .unwrap_or(deposit_cell.output.lock_script()),
                deposit_cell.output.type_script(),
                deposit_header_dep.number().to_le_bytes().to_vec(),
                Some(capacity),
            )?;
            skeleton
                .input(deposit_cell)?
                .output(withdraw_cell)
                .headerdep(deposit_header_dep)
                .witness(Default::default());
        }
        log.push((
            DAO_WITHDRAW_PHASE_ONE,
            searched_capacity.to_le_bytes().to_vec(),
        ));
        if searched_capacity == 0 {
            if self.throw_if_no_available {
                return Err(CalculatorError::NoAvailableCells(
                    "no available DAO deposit cells".into(),
                ));
            }
            return Ok(());
        }
        Box::new(AddDaoCelldep {}).run(rpc, skeleton, log)
    }
}
