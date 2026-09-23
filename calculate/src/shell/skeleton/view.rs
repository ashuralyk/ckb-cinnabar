//! RPC fetch, `TransactionView`, send, `Display`, and [`ChangeReceiver`].

use std::{
    fmt::{self, Display},
    time::Duration,
};

use ckb_jsonrpc_types::{OutputsValidator, Status};
use ckb_types::{
    core::{
        cell::{CellMetaBuilder, ResolvedTransaction},
        Capacity, TransactionView,
    },
    packed::OutPointVec,
};
use tokio::time::sleep;

use crate::{
    address::Address,
    error::{CalculatorError, Result},
    kernel::skeleton::{
        CellDepEx, CellInputEx, CellOutputEx, ScriptEx, TransactionSkeleton, WitnessEx,
    },
    rpc::{Host, RPC},
    types::{
        packed::{Script, WitnessArgs},
        Builder, DepType, Entity, Hash256, Pack, Unpack,
    },
};

impl CellOutputEx {
    /// Exactly occupied capacity of the cell
    pub fn occupied_capacity(&self) -> Capacity {
        self.output
            .occupied_capacity(Capacity::bytes(self.data.len()).unwrap())
            .unwrap()
    }

    /// Declared capacity of the cell
    pub fn capacity(&self) -> Capacity {
        self.output.capacity().unpack()
    }
}

impl TransactionSkeleton {
    /// Initialize a TransactionSkeleton from packed TransactionView via CKB RPC
    pub fn new_from_transaction_view<T: RPC>(rpc: &T, tx: &TransactionView) -> Result<Self> {
        let mut skeleton = TransactionSkeleton::default();
        skeleton
            .update_inputs_from_transaction_view(rpc, tx)?
            .update_celldeps_from_transaction_view(rpc, tx)?
            .update_headerdeps_from_transaction_view(rpc, tx)?
            .update_outputs_from_transaction_view(tx)
            .update_witnesses_from_transaction_view(tx)?;
        Ok(skeleton)
    }

    /// Override Inputs part of TransactionSkeleton from packed TransactionView
    pub fn update_inputs_from_transaction_view<T: RPC>(
        &mut self,
        rpc: &T,
        tx: &TransactionView,
    ) -> Result<&mut Self> {
        let mut inputs = Vec::new();
        for input in tx.inputs() {
            let out_point = input.previous_output();
            let tx_hash: Hash256 = out_point.tx_hash().unpack();
            let index: u32 = out_point.index().unpack();
            let since: u64 = input.since().unpack();
            inputs.push(CellInputEx::new_from_outpoint(
                rpc,
                tx_hash,
                index,
                Some(since),
                true,
            )?);
        }
        self.inputs = inputs;
        Ok(self)
    }

    /// Override CellDeps part of TransactionSkeleton from packed TransactionView
    pub fn update_celldeps_from_transaction_view<T: RPC>(
        &mut self,
        rpc: &T,
        tx: &TransactionView,
    ) -> Result<&mut Self> {
        let mut celldeps = Vec::new();
        for (i, cell_dep) in tx.cell_deps().into_iter().enumerate() {
            let name = format!("unknown-{i}");
            let out_point = cell_dep.out_point();
            let tx_hash: Hash256 = out_point.tx_hash().unpack();
            let index: u32 = out_point.index().unpack();
            let dep_type = cell_dep.dep_type().try_into().expect("dep type");
            celldeps.push(CellDepEx::new_from_outpoint(
                rpc, name, tx_hash, index, dep_type, false,
            )?);
        }
        self.celldeps = celldeps;
        Ok(self)
    }

    /// Override HeaderDeps part of TransactionSkeleton from packed TransactionView
    pub fn update_headerdeps_from_transaction_view<T: RPC>(
        &mut self,
        rpc: &T,
        tx: &TransactionView,
    ) -> Result<&mut Self> {
        use crate::kernel::skeleton::HeaderDepEx;
        let mut headerdeps = vec![];
        for header_dep in tx.header_deps_iter() {
            let block_hash: Hash256 = header_dep.unpack();
            headerdeps.push(HeaderDepEx::new(rpc, block_hash, vec![])?);
        }
        self.headerdeps = headerdeps;
        Ok(self)
    }

    /// Override Outputs part of TransactionSkeleton from packed TransactionView
    pub fn update_outputs_from_transaction_view(&mut self, tx: &TransactionView) -> &mut Self {
        self.outputs = tx
            .outputs_with_data_iter()
            .map(|(output, data)| CellOutputEx::new(output, data.to_vec()))
            .collect();
        self
    }

    /// Override Witnesses part of TransactionSkeleton from packed TransactionView
    pub fn update_witnesses_from_transaction_view(
        &mut self,
        tx: &TransactionView,
    ) -> Result<&mut Self> {
        self.witnesses = tx
            .witnesses()
            .into_iter()
            .map(|witness| {
                if let Ok(witness_args) = WitnessArgs::from_slice(&witness.raw_data()) {
                    let lock = witness_args.lock().to_opt().unwrap_or_default();
                    let input_type = witness_args.input_type().to_opt().unwrap_or_default();
                    let output_type = witness_args.output_type().to_opt().unwrap_or_default();
                    WitnessEx::new(
                        lock.raw_data().to_vec(),
                        input_type.raw_data().to_vec(),
                        output_type.raw_data().to_vec(),
                    )
                } else if witness.raw_data().is_empty() {
                    WitnessEx::default()
                } else {
                    WitnessEx::new_plain(witness.raw_data().to_vec())
                }
            })
            .collect::<Vec<_>>();
        Ok(self)
    }

    /// Push a input cell from ckb address via CKB RPC, which is majorly used to inject capacity
    pub fn input_from_address<T: RPC>(&mut self, rpc: &T, address: Address) -> Result<&mut Self> {
        self.input_from_script(rpc, address.payload().into())
    }

    /// Push a output cell from ckb address, which is majorly used to receive capacity change
    pub fn output_from_address(&mut self, address: Address, data: Vec<u8>) -> Result<&mut Self> {
        self.output_from_script(address.payload().into(), data)
    }

    /// Calculate transaction fee based on current minimal fee rate and additional fee rate
    pub fn fee<T: RPC>(&self, rpc: &T, additinal_fee_rate: u64) -> Result<Capacity> {
        let fee_rate = rpc.min_fee_rate()? + additinal_fee_rate;
        let tx = self.clone().into_transaction_view();
        let tx_fee = tx.data().as_slice().len() as u64 * fee_rate / 1000;
        Ok(Capacity::shannons(tx_fee))
    }

    /// Balance the transaction by adding input cells until the needed capacity is satisfied
    ///
    /// Support two modes:
    /// 1. Balance by adding an extra change cell for receiving the change capacity - ChangeReceiver::Address
    /// 2. Balance by choosing an existing output cell as the change cell - ChangeReceiver::Output
    pub fn balance<T: RPC>(
        &mut self,
        rpc: &T,
        fee: Capacity,
        balancer: ScriptEx,
        change_receiver: ChangeReceiver,
    ) -> Result<&mut Self> {
        let change_cell_index = match change_receiver {
            ChangeReceiver::Address(changer) => {
                self.output_from_address(changer, Default::default())?;
                self.outputs.len() - 1
            }
            ChangeReceiver::Script(changer) => {
                self.output_from_script(changer, Default::default())?;
                self.outputs.len() - 1
            }
            ChangeReceiver::Output(index) => {
                if self.outputs.len() <= index {
                    return Err(CalculatorError::OutputCellNotFound(
                        "change output index out of range".into(),
                    ));
                }
                index
            }
        };
        while self.exceeded_capacity() < fee.as_u64() {
            self.input_from_script(rpc, balancer.clone())?;
        }
        let exceeded_capacity_beyond_fee = self.exceeded_capacity().saturating_sub(fee.as_u64());
        let old_capacity: u64 = self.outputs[change_cell_index].capacity_shannons();
        let new_capacity = old_capacity.saturating_add(exceeded_capacity_beyond_fee);
        self.outputs[change_cell_index].output = self.outputs[change_cell_index]
            .output
            .clone()
            .as_builder()
            .capacity(new_capacity)
            .build();
        if self.exceeded_capacity() != fee.as_u64() {
            return Err(CalculatorError::InsufficientCapacity(
                "failed to balance transaction".into(),
            ));
        }
        Ok(self)
    }

    /// Turn into ResolvedTransaction for contracts native debugging
    pub fn into_resolved_transaction<T: RPC>(self, rpc: &T) -> Result<ResolvedTransaction> {
        let tx = self.clone().into_transaction_view();
        let mut resolved_inputs = vec![];
        for v in self.inputs {
            let out_point = v.input.previous_output();
            let meta = CellMetaBuilder::from_cell_output(v.output.output, v.output.data.into())
                .out_point(out_point)
                .build();
            resolved_inputs.push(meta);
        }
        let mut resolved_cell_deps = vec![];
        let mut resolved_dep_groups = vec![];
        for mut v in self.celldeps {
            if !v.with_data {
                v.refresh_cell_output(rpc)?;
            }
            let output = v.output;
            if v.celldep.dep_type() == DepType::DepGroup.into() {
                // dep group data is a list of out points
                let sub_out_points = OutPointVec::from_slice(&output.data)
                    .map_err(|_| CalculatorError::Other("invalid dep group".into()))?;
                for sub_out_point in sub_out_points {
                    let tx_hash = sub_out_point.tx_hash().unpack();
                    let index = sub_out_point.index().unpack();
                    let sub_celldep = CellDepEx::new_from_outpoint(
                        rpc,
                        "".to_string(),
                        tx_hash,
                        index,
                        DepType::Code,
                        true,
                    )?;
                    let sub_output = sub_celldep.output;
                    let meta = CellMetaBuilder::from_cell_output(
                        sub_output.output,
                        sub_output.data.into(),
                    )
                    .out_point(sub_out_point)
                    .build();
                    resolved_cell_deps.push(meta);
                }
                let meta = CellMetaBuilder::from_cell_output(output.output, output.data.into())
                    .out_point(v.celldep.out_point())
                    .build();
                resolved_dep_groups.push(meta);
            } else {
                let meta = CellMetaBuilder::from_cell_output(output.output, output.data.into())
                    .out_point(v.celldep.out_point())
                    .build();
                resolved_cell_deps.push(meta);
            }
        }
        Ok(ResolvedTransaction {
            transaction: tx,
            resolved_cell_deps,
            resolved_inputs,
            resolved_dep_groups,
        })
    }

    /// Turn into packed TransactionView
    pub fn into_transaction_view(self) -> TransactionView {
        let inputs = self.inputs.into_iter().map(|v| v.input).collect::<Vec<_>>();
        let celldeps = self
            .celldeps
            .into_iter()
            .map(|v| v.celldep)
            .collect::<Vec<_>>();
        let mut outputs = vec![];
        let mut outputs_data = vec![];
        self.outputs.into_iter().for_each(|v| {
            outputs.push(v.output);
            outputs_data.push(v.data.pack());
        });
        let witnesses = self
            .witnesses
            .into_iter()
            .map(|v| v.into_packed_bytes())
            .collect::<Vec<_>>();
        let headers = self
            .headerdeps
            .into_iter()
            .map(|v| v.block_hash.pack())
            .collect::<Vec<_>>();
        TransactionView::new_advanced_builder()
            .inputs(inputs)
            .outputs(outputs)
            .outputs_data(outputs_data)
            .cell_deps(celldeps)
            .witnesses(witnesses)
            .header_deps(headers)
            .build()
    }

    /// Consume and send this transaction, and then wait for confirmation
    ///
    /// `confirm_count`: wait how many blocks to firm confirmation, if 0, return immidiently after sending
    /// `wait_timeout`: wait how much time until throwing timeout error, if None, no timeout
    pub async fn send_and_wait<T: RPC + Host>(
        self,
        rpc: &T,
        confirm_count: u8,
        wait_timeout: Option<Duration>,
    ) -> Result<ckb_types::H256> {
        let hash = rpc
            .send_transaction(self.into(), Some(OutputsValidator::Passthrough))
            .await?;
        if confirm_count == 0 {
            return Ok(hash);
        }
        let mut block_number = 0u64;
        let mut time_used = Duration::from_secs(0);
        let interval = Duration::from_secs(3);
        loop {
            if let Some(timeout) = wait_timeout {
                if time_used > timeout {
                    return Err(CalculatorError::Network(format!(
                        "timeout waiting tx: {hash:#x}"
                    )));
                }
                time_used += interval;
            }
            sleep(interval).await;
            let tx = rpc
                .get_transaction(&hash)
                .await?
                .ok_or_else(|| CalculatorError::Network(format!("no tx found: {hash:#x}")))?;
            if tx.tx_status.status == Status::Rejected {
                let reason = tx.tx_status.reason.unwrap_or_else(|| "unknown".to_string());
                return Err(CalculatorError::Network(format!(
                    "tx {hash:#x} rejected, reason: {reason}"
                )));
            }
            if tx.tx_status.status != Status::Committed {
                continue;
            }
            if block_number == 0 {
                if let Some(number) = tx.tx_status.block_number {
                    block_number = number.into();
                }
            } else {
                let tip_number = rpc.get_tip_block_number()?;
                if tip_number >= block_number + confirm_count as u64 {
                    break;
                }
            }
        }
        Ok(hash)
    }
}

impl Display for TransactionSkeleton {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tx = self.clone().into_transaction_view();
        let tx_json = ckb_jsonrpc_types::TransactionView::from(tx);
        f.write_fmt(format_args!(
            "{}",
            serde_json::to_string_pretty(&tx_json).unwrap()
        ))
    }
}

impl From<TransactionSkeleton> for TransactionView {
    fn from(value: TransactionSkeleton) -> Self {
        value.into_transaction_view()
    }
}

impl From<TransactionSkeleton> for ckb_jsonrpc_types::Transaction {
    fn from(value: TransactionSkeleton) -> Self {
        let view: TransactionView = value.into();
        view.data().into()
    }
}

/// Indicate how to receive the change capacity while balancing transaction
pub enum ChangeReceiver {
    /// Balance by adding an extra change cell from ckb address
    Address(Address),
    /// Balance by adding an extra change cell from lock script
    Script(ScriptEx),
    /// Balance by choosing an existing output cell
    Output(usize),
}

impl From<Address> for ChangeReceiver {
    fn from(value: Address) -> Self {
        ChangeReceiver::Address(value)
    }
}

impl From<Script> for ChangeReceiver {
    fn from(value: Script) -> Self {
        ChangeReceiver::Script(value.into())
    }
}

impl From<ScriptEx> for ChangeReceiver {
    fn from(value: ScriptEx) -> Self {
        ChangeReceiver::Script(value)
    }
}

impl From<usize> for ChangeReceiver {
    fn from(value: usize) -> Self {
        ChangeReceiver::Output(value)
    }
}
