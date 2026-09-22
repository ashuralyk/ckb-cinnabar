//! Nervos DAO host wrappers and phase-two withdraw (ckb-sdk math).
//!
//! Deposit, cell-dep, and phase-one collect live in [`crate::kernel::operation::dao`].
//! Pair with `dao_deposit` / `dao_withdraw_phase_one` /
//! `dao_withdraw_phase_two` in `crate::instruction::predefined`.

pub use crate::kernel::operation::dao::*;

use ckb_sdk::{
    util::{calculate_dao_maximum_withdraw4, minimal_unlock_point},
    Since, SinceType,
};
use ckb_types::{h256, prelude::IntoHeaderView, H256};
use eyre::{eyre, Result};

use crate::{
    error,
    indexer::{CellQueryOptions, GetCellsIter, SearchKey},
    operation::{Log, Operation},
    rpc::{Network, RPC},
    skeleton::{CellInputEx, CellOutputEx, HeaderDepEx, ScriptEx, TransactionSkeleton, WitnessEx},
    types::hash_to_h256,
};

pub mod hardcoded {
    //! Hardcoded Nervos DAO script deployment per network.

    use super::*;

    /// Cell-dep name used in the skeleton for the DAO script.
    pub const DAO_NAME: &str = super::DAO_NAME;
    /// Genesis out-point tx hash of the DAO script on mainnet.
    pub const DAO_MAINNET_TX_HASH: H256 =
        h256!("0xe2fb199810d49a4d8beec56718ba2593b665db9d52299a0f9e6e75416d73ff5c");
    /// Genesis out-point tx hash of the DAO script on testnet.
    pub const DAO_TESTNET_TX_HASH: H256 =
        h256!("0x8f8c79eb6671709633fe6a46de93c0fedc9c1b8a6527a18d3983879542635c9f");
    /// Type hash of the DAO type script (same on all networks).
    pub const DAO_TYPE_HASH: H256 =
        h256!("0x82d76d1b75fe2fd9a27dfbaa65a039221a380d76c926f378d3f81cf3e7e13f2e");

    /// DAO deployment tx hash for `network` (sentinel hash under fake networks).
    pub fn dao_tx_hash(network: Network) -> H256 {
        hash_to_h256(&super::dao_tx_hash(&network))
    }

    /// DAO type script for `network`; a fake `ScriptEx::Reference` on
    /// fake/custom networks so simulation can resolve it from a cell dep.
    pub fn dao_script(network: Network) -> ScriptEx {
        super::dao_script(&network)
    }
}

/// [`Log`] keys emitted by the DAO operations.
pub mod hookkey {
    pub use crate::intent::log::{DAO_WITHDRAW_PHASE_ONE, DAO_WITHDRAW_PHASE_TWO};
}

/// Consume withdraw cells of phase one and generate a ordinary cell to receive the withdraw capacity
///
/// # Parameters
/// - `maximal_withdraw_capacity`: The maximal capacity to withdraw
/// - `owner`: The owner of the DAO deposit cell
/// - `transfer_to`: The lock script that receives all of capacities from searched withdraw cells, if None, use owner instead
pub struct AddDaoWithdrawPhaseTwoCells {
    /// Stop searching once this much capacity is collected.
    pub maximal_withdraw_capacity: u64,
    /// Lock script that owns the phase-one withdraw cells.
    pub owner: ScriptEx,
    /// Receiver of the unlocked capacity; defaults to `owner` when `None`.
    pub transfer_to: Option<ScriptEx>,
    /// Fail with `no available DAO withdraw cells` when nothing is found.
    pub throw_if_no_available: bool,
}

impl AddDaoWithdrawPhaseTwoCells {
    fn search_key(&self, network: Network, skeleton: &TransactionSkeleton) -> Result<SearchKey> {
        let dao_type_script = hardcoded::dao_script(network);
        let mut query = CellQueryOptions::new_lock(self.owner.clone().to_script(skeleton)?);
        query.with_data = Some(true);
        query.secondary_script = Some(dao_type_script.to_script(skeleton)?);
        Ok(query.into())
    }

    fn minimum_since(deposit_headerdep: &HeaderDepEx, withdraw_headerdep: &HeaderDepEx) -> u64 {
        let since_unlock = minimal_unlock_point(
            &deposit_headerdep.header.clone().into_view(),
            &withdraw_headerdep.header.clone().into_view(),
        );
        let since = Since::new(
            SinceType::EpochNumberWithFraction,
            since_unlock.full_value(),
            false,
        );
        since.value()
    }

    fn maximum_withdraw_capacity(
        deposit_headerdep: &HeaderDepEx,
        withdraw_headerdep: &HeaderDepEx,
        withdraw_cell: &CellInputEx,
    ) -> u64 {
        calculate_dao_maximum_withdraw4(
            &deposit_headerdep.header.clone().into_view(),
            &withdraw_headerdep.header.clone().into_view(),
            &withdraw_cell.output.output,
            withdraw_cell.output.occupied_capacity().as_u64(),
        )
    }
}

impl<T: RPC> Operation<T> for AddDaoWithdrawPhaseTwoCells {
    fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> error::Result<()> {
        let mut searched_capacity = 0u64;
        let mut search = GetCellsIter::new(rpc, self.search_key(rpc.network(), skeleton)?);
        let mut output_capacity = 0u64;
        let mut withdraw_headerdeps = vec![];
        while let Some(cell) = search.next()? {
            if cell.output_data.len() < 8 {
                continue;
            }
            let deposit_block_number =
                u64::from_le_bytes(cell.output_data[..8].try_into().unwrap());
            if deposit_block_number == 0 {
                continue;
            }
            let deposit_headerdep = HeaderDepEx::new_from_block_number(rpc, deposit_block_number)?;
            let withdraw_headerdep = HeaderDepEx::new_from_outpoint(rpc, cell.out_point.clone())?;
            let since = Self::minimum_since(&deposit_headerdep, &withdraw_headerdep);
            let withdraw_cell = CellInputEx::new_from_live_cell(cell, Some(since));
            searched_capacity += withdraw_cell.output.capacity().as_u64();
            if searched_capacity >= self.maximal_withdraw_capacity {
                break;
            }
            let headerdep_idx = skeleton
                .headerdeps
                .iter()
                .position(|v| v == &deposit_headerdep)
                .unwrap_or(skeleton.headerdeps.len());
            let witness_args = WitnessEx::new(vec![], headerdep_idx.to_le_bytes().to_vec(), vec![]);
            output_capacity += Self::maximum_withdraw_capacity(
                &deposit_headerdep,
                &withdraw_headerdep,
                &withdraw_cell,
            );
            skeleton
                .input(withdraw_cell)?
                .witness(witness_args)
                .headerdep(deposit_headerdep);
            if !withdraw_headerdeps.contains(&withdraw_headerdep) {
                withdraw_headerdeps.push(withdraw_headerdep);
            }
        }
        log.push((
            hookkey::DAO_WITHDRAW_PHASE_TWO,
            output_capacity.to_le_bytes().to_vec(),
        ));
        if output_capacity == 0 {
            if self.throw_if_no_available {
                return Err(eyre!("no available DAO withdraw cells").into());
            }
            return Ok(());
        }
        skeleton.headerdeps.extend(withdraw_headerdeps);
        let transfer_lock_script = if let Some(transfer_to) = self.transfer_to {
            transfer_to.to_script(skeleton)?
        } else {
            self.owner.to_script(skeleton)?
        };
        let withdraw_output = CellOutputEx::new_from_scripts(
            transfer_lock_script,
            None,
            vec![],
            Some(output_capacity),
        )?;
        if withdraw_output.capacity() < withdraw_output.occupied_capacity() {
            return Err(eyre!("withdraw capacity cannot cover minimal requirement").into());
        }
        skeleton.output(withdraw_output);
        Box::new(AddDaoCelldep {}).run(rpc, skeleton, log)?;
        Ok(())
    }
}
