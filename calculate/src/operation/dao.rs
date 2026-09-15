//! Nervos DAO deposit and two-phase withdraw operations.
//!
//! Pair with `dao_deposit` / `dao_withdraw_phase_one` / `dao_withdraw_phase_two`
//! in `crate::instruction::predefined`. Phase-one marks a deposit; phase-two
//! actually unlocks capacity after the lockup period.
//! Log keys: [`crate::intent::log::DAO_WITHDRAW_PHASE_ONE`] /
//! [`crate::intent::log::DAO_WITHDRAW_PHASE_TWO`].

use async_trait::async_trait;
use ckb_jsonrpc_types::JsonBytes;
use ckb_sdk::{
    util::{calculate_dao_maximum_withdraw4, minimal_unlock_point},
    Since, SinceType,
};
use ckb_types::{
    core::{Capacity, DepType},
    h256, H256,
};
use eyre::{eyre, Result};

use crate::{
    indexer::{CellQueryOptions, SearchKey, SearchKeyFilter, SearchMode},
    operation::{basic::AddCellDep, Log, Operation},
    rpc::{GetCellsIter, Network, RPC},
    skeleton::{CellInputEx, CellOutputEx, HeaderDepEx, ScriptEx, TransactionSkeleton, WitnessEx},
};

pub mod hardcoded {
    //! Hardcoded Nervos DAO script deployment per network.

    use super::*;
    use crate::simulation::random_hash;

    /// Cell-dep name used in the skeleton for the DAO script.
    pub const DAO_NAME: &str = "dao";
    /// Genesis out-point tx hash of the DAO script on mainnet.
    pub const DAO_MAINNET_TX_HASH: H256 =
        h256!("0xe2fb199810d49a4d8beec56718ba2593b665db9d52299a0f9e6e75416d73ff5c");
    /// Genesis out-point tx hash of the DAO script on testnet.
    pub const DAO_TESTNET_TX_HASH: H256 =
        h256!("0x8f8c79eb6671709633fe6a46de93c0fedc9c1b8a6527a18d3983879542635c9f");
    /// Type hash of the DAO type script (same on all networks).
    pub const DAO_TYPE_HASH: H256 =
        h256!("0x82d76d1b75fe2fd9a27dfbaa65a039221a380d76c926f378d3f81cf3e7e13f2e");

    lazy_static::lazy_static! {
        pub static ref DAO_FAKENET_TX_HASH: H256 = random_hash().into();
    }

    /// DAO deployment tx hash for `network` (random under fake networks).
    pub fn dao_tx_hash(network: Network) -> H256 {
        match network {
            Network::Mainnet => DAO_MAINNET_TX_HASH,
            Network::Testnet => DAO_TESTNET_TX_HASH,
            _ => DAO_FAKENET_TX_HASH.clone(),
        }
    }

    /// DAO type script for `network`; a fake `ScriptEx::Reference` on
    /// fake/custom networks so simulation can resolve it from a cell dep.
    pub fn dao_script(network: Network) -> ScriptEx {
        match network {
            Network::Mainnet | Network::Testnet => {
                ScriptEx::new_type(hardcoded::DAO_TYPE_HASH, vec![])
            }
            _ => (DAO_NAME.to_string(), vec![]).into(),
        }
    }
}

/// [`Log`] keys emitted by the DAO operations.
pub mod hookkey {
    pub use crate::intent::log::{DAO_WITHDRAW_PHASE_ONE, DAO_WITHDRAW_PHASE_TWO};
}

/// Add DAO celldep to the transaction
pub struct AddDaoCelldep {}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddDaoCelldep {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        Box::new(AddCellDep {
            name: hardcoded::DAO_NAME.to_string(),
            tx_hash: hardcoded::dao_tx_hash(rpc.network()),
            index: 2,
            dep_type: DepType::Code,
            with_data: false,
        })
        .run(rpc, skeleton, log)
        .await
    }
}

/// Add a DAO deposit output (8 zero bytes of data + DAO type script).
///
/// Capacity is absolute (shannons). Also injects the DAO cell dep.
pub struct AddDaoDepositOutputCell {
    /// Lock script that owns the deposit.
    pub owner: ScriptEx,
    /// Total capacity of the deposit cell, in shannons.
    pub deposit_capacity: u64,
}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddDaoDepositOutputCell {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let dao_type_script = hardcoded::dao_script(rpc.network());
        skeleton.output(CellOutputEx::new_from_scripts(
            self.owner.to_script(skeleton)?,
            Some(dao_type_script.to_script(skeleton)?),
            vec![0u8; 8],
            Some(Capacity::shannons(self.deposit_capacity)),
        )?);
        Box::new(AddDaoCelldep {}).run(rpc, skeleton, log).await
    }
}

/// Inject DAO phase one withdraw related cells, including previous deposit cells and withdraw cells of phase one
///
/// # Parameters
/// - `maximal_withdraw_capacity`: The maximal capacity to withdraw
/// - `upperbound_timestamp`: The timestamp to figure out the maturity of deposit cells
/// - `owner`: The owner of the DAO deposit cell
/// - `transfer_to`: The lock script of the withdraw cell, if not provided, use the same lock script in deposit cell
/// - `throw_if_no_available`: If true, throw an error if no available DAO deposit cells
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
    /// Fail with `no available DAO deposit cells` when nothing is found.
    pub throw_if_no_available: bool,
}

impl AddDaoWithdrawPhaseOneCells {
    fn search_key(&self, network: Network, skeleton: &TransactionSkeleton) -> Result<SearchKey> {
        let dao_type_script = hardcoded::dao_script(network);
        let mut search_key: SearchKey =
            CellQueryOptions::new_lock(self.owner.clone().to_script(skeleton)?).into();
        search_key.with_data = Some(true);
        search_key.filter = Some(SearchKeyFilter {
            script: Some(dao_type_script.to_script(skeleton)?.into()),
            output_data: Some(JsonBytes::from_vec(vec![0u8; 8])),
            output_data_filter_mode: Some(SearchMode::Exact),
            ..Default::default()
        });
        Ok(search_key)
    }

    async fn check_deposit_timestamp<T: RPC>(
        &self,
        rpc: &T,
        deposit_block_number: u64,
    ) -> Result<bool> {
        let Some(deposit_header) = rpc
            .get_header_by_number(deposit_block_number.into())
            .await?
        else {
            return Ok(false);
        };
        let deposit_timestamp: u64 = deposit_header.inner.timestamp.into();
        Ok(deposit_timestamp <= self.upperbound_timestamp)
    }
}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddDaoWithdrawPhaseOneCells {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let mut searched_capacity = 0u64;
        let mut search = GetCellsIter::new(rpc, self.search_key(rpc.network(), skeleton)?);
        let transfer_lock_script = if let Some(transfer_to) = self.transfer_to.clone() {
            Some(transfer_to.to_script(skeleton)?)
        } else {
            None
        };
        while let Some(cell) = search.next().await? {
            let mature_deposit = self
                .check_deposit_timestamp(rpc, cell.block_number.into())
                .await?;
            if !mature_deposit {
                continue;
            }
            let deposit_cell = CellInputEx::new_from_indexer_cell(cell, None);
            let capacity = deposit_cell.output.capacity();
            searched_capacity += capacity.as_u64();
            if searched_capacity >= self.maximal_withdraw_capacity {
                break;
            }
            let deposit_header_dep =
                HeaderDepEx::new_from_outpoint(rpc, deposit_cell.input.previous_output()).await?;
            let block_number = deposit_header_dep.header.number();
            let withdraw_cell = CellOutputEx::new_from_scripts(
                transfer_lock_script
                    .clone()
                    .unwrap_or(deposit_cell.output.lock_script()),
                deposit_cell.output.type_script(),
                block_number.to_le_bytes().to_vec(),
                Some(capacity),
            )?;
            skeleton
                .input(deposit_cell)?
                .output(withdraw_cell)
                .headerdep(deposit_header_dep)
                .witness(Default::default());
        }
        log.push((
            hookkey::DAO_WITHDRAW_PHASE_ONE,
            searched_capacity.to_le_bytes().to_vec(),
        ));
        if searched_capacity == 0 {
            if self.throw_if_no_available {
                return Err(eyre!("no available DAO deposit cells"));
            }
            Ok(())
        } else {
            Box::new(AddDaoCelldep {}).run(rpc, skeleton, log).await
        }
    }
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
        let since_unlock =
            minimal_unlock_point(&deposit_headerdep.header, &withdraw_headerdep.header);
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
            &deposit_headerdep.header,
            &withdraw_headerdep.header,
            &withdraw_cell.output.output,
            withdraw_cell.output.occupied_capacity().as_u64(),
        )
    }
}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddDaoWithdrawPhaseTwoCells {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let mut searched_capacity = 0u64;
        let mut search = GetCellsIter::new(rpc, self.search_key(rpc.network(), skeleton)?);
        let mut output_capacity = 0u64;
        let mut withdraw_headerdeps = vec![];
        while let Some(cell) = search.next().await? {
            let data = cell.output_data.as_ref().unwrap();
            let deposit_block_number = u64::from_le_bytes(data.as_bytes().try_into().unwrap());
            if deposit_block_number == 0 {
                continue;
            }
            let deposit_headerdep =
                HeaderDepEx::new_from_block_number(rpc, deposit_block_number).await?;
            let withdraw_headerdep =
                HeaderDepEx::new_from_outpoint(rpc, cell.out_point.clone().into()).await?;
            let since = Self::minimum_since(&deposit_headerdep, &withdraw_headerdep);
            let withdraw_cell = CellInputEx::new_from_indexer_cell(cell, Some(since));
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
                return Err(eyre!("no available DAO withdraw cells"));
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
            Some(Capacity::shannons(output_capacity)),
        )?;
        if withdraw_output.capacity() < withdraw_output.occupied_capacity() {
            return Err(eyre!("withdraw capacity cannot cover minimal requirement"));
        }
        skeleton.output(withdraw_output);
        Box::new(AddDaoCelldep {}).run(rpc, skeleton, log).await
    }
}
