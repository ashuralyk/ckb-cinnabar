use std::fmt::Display;

use ckb_hash::{blake2b_256, Blake2bBuilder};
use ckb_sdk::{
    rpc::ckb_indexer::{Cell, SearchMode},
    traits::{CellQueryOptions, ValueRangeOption},
    Address,
};
use ckb_types::{
    core::{Capacity, DepType, TransactionView},
    packed::{Bytes, CellDep, CellInput, CellOutput, OutPoint, Script, WitnessArgs},
    prelude::{Builder, Entity, Pack, Unpack},
    H256,
};
use eyre::{eyre, Result};
use futures::future::join_all;

use crate::rpc::{GetCellsIter, RPC};

/// CellInput for transaction skeleton, which contains output cell and data
#[derive(Debug, Clone)]
pub struct CellInputEx {
    pub input: CellInput,
    pub output: CellOutputEx,
}

impl PartialEq for CellInputEx {
    fn eq(&self, other: &Self) -> bool {
        self.input.as_bytes() == other.input.as_bytes()
    }
}

impl CellInputEx {
    /// Directly initialize a CellInputEx
    pub fn new(input: CellInput, output: CellOutput, data: Vec<u8>) -> Self {
        CellInputEx {
            input,
            output: CellOutputEx::new(output, data),
        }
    }

    /// Initialize a CellInputEx from out point via CKB RPC
    pub async fn new_from_outpoint<T: RPC>(
        rpc: &T,
        tx_hash: H256,
        index: usize,
        since: Option<u64>,
    ) -> Result<Self> {
        let out_point = OutPoint::new_builder()
            .tx_hash(tx_hash.pack())
            .index(index.pack())
            .build();
        let live_cell = rpc
            .get_live_cell(&out_point.clone().into(), true)
            .await?
            .cell
            .ok_or(eyre!(
                "cell not found at ({}:{index})",
                hex::encode(&tx_hash)
            ))?;
        let input = CellInput::new_builder()
            .previous_output(out_point)
            .since(since.unwrap_or(0).pack())
            .build();
        let output = live_cell.output.into();
        let data = live_cell.data.unwrap().content.into_bytes().to_vec();
        Ok(Self::new(input, output, data))
    }

    /// Initialize a CellInputEx from the ckb-indexer specific cell
    pub fn new_from_indexer_cell(indexer_cell: Cell) -> Self {
        let input = CellInput::new_builder()
            .previous_output(indexer_cell.out_point.into())
            .build();
        let data = indexer_cell
            .output_data
            .unwrap_or_default()
            .into_bytes()
            .to_vec();
        Self::new(input, indexer_cell.output.into(), data)
    }
}

/// CellOutput for transaction skeleton, which contains cell data
#[derive(Debug, Clone)]
pub struct CellOutputEx {
    pub output: CellOutput,
    pub data: Vec<u8>,
}

impl CellOutputEx {
    /// Directly initialize a CellOutputEx
    pub fn new(output: CellOutput, data: Vec<u8>) -> Self {
        CellOutputEx { output, data }
    }

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

    /// Cell's lock script
    pub fn lock_script(&self) -> Script {
        self.output.lock()
    }

    /// Cell's type script
    pub fn type_script(&self) -> Option<Script> {
        self.output.type_().to_opt()
    }

    /// Calculate blake2b hash of lock script
    pub fn calc_lock_hash(&self) -> H256 {
        self.lock_script().calc_script_hash().unpack()
    }

    /// Calculate blake2b hash of type script
    pub fn calc_type_hash(&self) -> Option<H256> {
        self.type_script()
            .map(|script| script.calc_script_hash().unpack())
    }

    /// Calculate blake2b hash of cell data
    pub fn data_hash(&self) -> H256 {
        blake2b_256(&self.data).into()
    }
}

/// CellDep for transaction skeleton, which contains output cell and data
#[derive(Debug, Clone)]
pub struct CellDepEx {
    pub cell_dep: CellDep,
    pub output: Option<CellOutputEx>,
}

impl PartialEq for CellDepEx {
    fn eq(&self, other: &Self) -> bool {
        self.cell_dep.as_bytes() == other.cell_dep.as_bytes()
    }
}

impl CellDepEx {
    /// Directly initialize a CellDepEx
    pub fn new(cell_dep: CellDep, output: CellOutput, data: Vec<u8>) -> Self {
        CellDepEx {
            cell_dep,
            output: Some(CellOutputEx::new(output, data)),
        }
    }

    /// Initialize a CellDepEx from out point via CKB RPC
    pub async fn new_from_outpoint<T: RPC>(
        rpc: &T,
        tx_hash: H256,
        index: u32,
        dep_type: DepType,
        with_data: bool,
    ) -> Result<Self> {
        let out_point = OutPoint::new_builder()
            .tx_hash(tx_hash.pack())
            .index(index.pack())
            .build();
        let live_cell = rpc
            .get_live_cell(&out_point.clone().into(), with_data)
            .await?
            .cell
            .ok_or(eyre!(
                "cell not found at ({}:{index})",
                hex::encode(&tx_hash)
            ))?;
        let cell_dep = CellDep::new_builder()
            .out_point(out_point)
            .dep_type(dep_type.into())
            .build();
        let output = live_cell.output.into();
        let data = live_cell.data.unwrap().content.into_bytes().to_vec();
        Ok(Self::new(cell_dep, output, data))
    }

    /// Initialize a CellDepEx from the ckb-indexer specific cell
    pub fn new_from_indexer_cell(indexer_cell: Cell, dep_type: DepType) -> Self {
        let out_point = indexer_cell.out_point.into();
        let cell_dep = CellDep::new_builder()
            .out_point(out_point)
            .dep_type(dep_type.into())
            .build();
        let output = indexer_cell.output.into();
        let data = indexer_cell
            .output_data
            .unwrap_or_default()
            .into_bytes()
            .to_vec();
        Self::new(cell_dep, output, data)
    }

    /// Retrive cell dep's on-chain output if there's None output field
    pub async fn refresh_cell_output<T: RPC>(&mut self, rpc: &T) -> Result<()> {
        let out_point = self.cell_dep.out_point().to_owned();
        let new_cell_dep = Self::new_from_outpoint(
            rpc,
            out_point.tx_hash().unpack(),
            out_point.index().unpack(),
            self.cell_dep.dep_type().try_into().unwrap(),
            true,
        )
        .await?;
        self.output = new_cell_dep.output;
        Ok(())
    }
}

/// Traditional witness args that contains lock, input_type and output_type, which
/// splited for better composability
#[derive(Debug, Clone)]
pub struct WitnessArgsEx {
    pub lock: Vec<u8>,
    pub input_type: Vec<u8>,
    pub output_type: Vec<u8>,
}

impl WitnessArgsEx {
    /// Directly initialize a TsWitnessArgs
    pub fn new(lock: Vec<u8>, input_type: Vec<u8>, output_type: Vec<u8>) -> Self {
        WitnessArgsEx {
            lock,
            input_type,
            output_type,
        }
    }

    /// Turn into packed WitnessArgs
    pub fn into_witness_args(self) -> WitnessArgs {
        let bytes_opt = |bytes: Vec<u8>| {
            if bytes.is_empty() {
                None
            } else {
                Some(
                    Bytes::new_builder()
                        .set(bytes.into_iter().map(Into::into).collect())
                        .build(),
                )
            }
        };
        WitnessArgs::new_builder()
            .lock(bytes_opt(self.lock).pack())
            .input_type(bytes_opt(self.input_type).pack())
            .output_type(bytes_opt(self.output_type).pack())
            .build()
    }

    /// Turn into packed bytes of WitnessArgs
    pub fn into_packed_bytes(self) -> Bytes {
        self.into_witness_args().as_bytes().pack()
    }
}

/// TransactionSkeleton for building transaction
#[derive(Default, Clone, Debug)]
pub struct TransactionSkeleton {
    pub inputs: Vec<CellInputEx>,
    pub outputs: Vec<CellOutputEx>,
    pub celldeps: Vec<CellDepEx>,
    pub witnesses: Vec<WitnessArgsEx>,
}

impl TransactionSkeleton {
    /// Initialize a TransactionSkeleton from packed TransactionView via CKB RPC
    pub async fn new_from_transaction_view<T: RPC>(rpc: &T, tx: &TransactionView) -> Result<Self> {
        let mut skeleton = TransactionSkeleton::default();
        skeleton
            .update_inputs_from_transaction_view(rpc, tx)
            .await?
            .update_celldeps_from_transaction_view(rpc, tx)
            .await?
            .update_outputs_from_transaction_view(tx)
            .update_witnesses_from_transaction_view(tx)?;
        Ok(skeleton)
    }

    /// Override Inputs part of TransactionSkeleton from packed TransactionView
    pub async fn update_inputs_from_transaction_view<T: RPC>(
        &mut self,
        rpc: &T,
        tx: &TransactionView,
    ) -> Result<&mut Self> {
        let inputs = tx
            .inputs()
            .into_iter()
            .map(|input| {
                let out_point = input.previous_output();
                let tx_hash: H256 = out_point.tx_hash().unpack();
                let index: u32 = out_point.index().unpack();
                let since: u64 = input.since().unpack();
                CellInputEx::new_from_outpoint(rpc, tx_hash, index as usize, Some(since))
            })
            .collect::<Vec<_>>();
        self.inputs = join_all(inputs).await.into_iter().collect::<Result<_>>()?;
        Ok(self)
    }

    /// Override CellDeps part of TransactionSkeleton from packed TransactionView
    pub async fn update_celldeps_from_transaction_view<T: RPC>(
        &mut self,
        rpc: &T,
        tx: &TransactionView,
    ) -> Result<&mut Self> {
        let celldeps = tx
            .cell_deps()
            .into_iter()
            .map(|cell_dep| {
                let out_point = cell_dep.out_point();
                let tx_hash: H256 = out_point.tx_hash().unpack();
                let index: u32 = out_point.index().unpack();
                let dep_type = cell_dep.dep_type().try_into().expect("dep type");
                CellDepEx::new_from_outpoint(rpc, tx_hash, index, dep_type, false)
            })
            .collect::<Vec<_>>();
        self.celldeps = join_all(celldeps)
            .await
            .into_iter()
            .collect::<Result<_>>()?;
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
                let witness_args = WitnessArgs::from_slice(&witness.raw_data())
                    .map_err(|_| eyre!("invalid witness args"))?;
                let lock = witness_args.lock().to_opt().unwrap_or_default();
                let input_type = witness_args.input_type().to_opt().unwrap_or_default();
                let output_type = witness_args.output_type().to_opt().unwrap_or_default();
                Ok(WitnessArgsEx {
                    lock: lock.raw_data().to_vec(),
                    input_type: input_type.raw_data().to_vec(),
                    output_type: output_type.raw_data().to_vec(),
                })
            })
            .collect::<Result<_>>()?;
        Ok(self)
    }

    /// Push a single input cell
    pub fn input(&mut self, cell_input: CellInputEx) -> Result<&mut Self> {
        if self.contains_input(&cell_input) {
            return Err(eyre!("input already exists"));
        }
        self.inputs.push(cell_input);
        Ok(self)
    }

    /// Push a input cell from lock script via CKB RPC
    pub async fn input_from_script<T: RPC>(
        &mut self,
        rpc: &T,
        lock_script: Script,
    ) -> Result<&mut Self> {
        let mut search_key = CellQueryOptions::new_lock(lock_script);
        search_key.secondary_script_len_range = Some(ValueRangeOption::new(0, 1));
        search_key.data_len_range = Some(ValueRangeOption::new(0, 1));
        search_key.script_search_mode = Some(SearchMode::Exact);
        let mut find_available_input = false;
        let mut iter = GetCellsIter::new(rpc, search_key.into());
        while let Some(cell) = iter.next().await? {
            let cell_input = CellInputEx::new_from_indexer_cell(cell);
            if self.contains_input(&cell_input) {
                continue;
            }
            self.inputs.push(cell_input);
            find_available_input = true;
            break;
        }
        if !find_available_input {
            return Err(eyre!("no available input"));
        }
        Ok(self)
    }

    /// Push a input cell from ckb address via CKB RPC, which is majorly used to inject capacity
    pub async fn input_from_address<T: RPC>(
        &mut self,
        rpc: &T,
        address: Address,
    ) -> Result<&mut Self> {
        self.input_from_script(rpc, address.payload().into()).await
    }

    /// Push a batch of input cells
    pub fn inputs(&mut self, cell_inputs: Vec<CellInputEx>) -> Result<&mut Self> {
        for cell_input in &cell_inputs {
            if self.contains_input(cell_input) {
                return Err(eyre!("input already exists"));
            }
        }
        self.inputs.extend(cell_inputs);
        Ok(self)
    }

    /// Check if input cell exists
    pub fn contains_input(&self, cell_input: &CellInputEx) -> bool {
        self.inputs.contains(cell_input)
    }

    /// Remove input cell by index, which may fail if index out of range
    pub fn remove_input(&mut self, index: usize) -> Result<&mut Self> {
        if self.inputs.len() <= index {
            return Err(eyre!("input index out of range"));
        }
        self.inputs.remove(index);
        Ok(self)
    }

    /// Pop the last input cell, which may fail if no input cell
    pub fn pop_input(&mut self) -> Result<&mut Self> {
        self.inputs.pop().ok_or(eyre!("no input to pop"))?;
        Ok(self)
    }

    /// Push a single output cell
    pub fn output(&mut self, cell_output: CellOutputEx) -> &mut Self {
        self.outputs.push(cell_output);
        self
    }

    /// Push a output cell from ckb address, which is majorly used to receive capacity change
    pub fn output_from_address(&mut self, address: Address) -> &mut Self {
        self.output_from_script(address.payload().into())
    }

    /// Push a output cell from lock script
    pub fn output_from_script(&mut self, lock_script: Script) -> &mut Self {
        let output = CellOutput::new_builder()
            .lock(lock_script)
            .build_exact_capacity(Capacity::zero())
            .expect("build exact capacity");
        self.output(CellOutputEx::new(output, Vec::new()))
    }

    /// Push a batch of output cells
    pub fn outputs(&mut self, cell_outputs: Vec<CellOutputEx>) -> &mut Self {
        self.outputs.extend(cell_outputs);
        self
    }

    /// Remove output cell by index, which may fail if index out of range
    pub fn remove_output(&mut self, index: usize) -> Result<&mut Self> {
        if self.outputs.len() <= index {
            return Err(eyre!("output index out of range"));
        }
        self.outputs.remove(index);
        Ok(self)
    }

    /// Pop the last output cell, which may fail if no output cell
    pub fn pop_output(&mut self) -> Result<&mut Self> {
        self.outputs.pop().ok_or(eyre!("no output to pop"))?;
        Ok(self)
    }

    /// Push a single cell dep
    pub fn celldep(&mut self, cell_dep: CellDepEx) -> &mut Self {
        if !self.celldeps.contains(&cell_dep) {
            self.celldeps.push(cell_dep);
        }
        self
    }

    /// Check if cell dep exists
    pub fn contains_celldep(&self, cell_dep: &CellDepEx) -> bool {
        self.celldeps.contains(cell_dep)
    }

    /// Push a batch of cell deps
    pub fn celldeps(&mut self, cell_deps: Vec<CellDepEx>) -> &mut Self {
        cell_deps.into_iter().for_each(|v| {
            if !self.celldeps.contains(&v) {
                self.celldeps.push(v);
            }
        });
        self
    }

    /// Push a single witness
    pub fn witness(&mut self, witness: WitnessArgsEx) -> &mut Self {
        self.witnesses.push(witness);
        self
    }

    /// Push a batch of witnesses
    pub fn witnesses(&mut self, witnesses: Vec<WitnessArgsEx>) -> &mut Self {
        self.witnesses.extend(witnesses);
        self
    }

    /// Accumulate total input cells' capacity
    pub fn total_inputs_capacity(&self) -> Capacity {
        self.inputs
            .iter()
            .map(|input| input.output.capacity())
            .fold(Capacity::zero(), |acc, x| acc.safe_add(x).unwrap())
    }

    /// Accumulate total output cells' capacity
    pub fn total_outputs_capacity(&self) -> Capacity {
        self.outputs
            .iter()
            .map(|output| output.capacity())
            .fold(Capacity::zero(), |acc, x| acc.safe_add(x).unwrap())
    }

    /// Return the difference between total outputs capacity and total inputs capacity, saturating at zero
    pub fn needed_capacity(&self) -> Capacity {
        let inputs_capacity = self.total_inputs_capacity();
        let outputs_capacity = self.total_outputs_capacity();
        if inputs_capacity > outputs_capacity {
            Capacity::zero()
        } else {
            outputs_capacity.safe_sub(inputs_capacity).unwrap()
        }
    }

    /// Return the difference between total inputs capacity and total outputs capacity, saturating at zero
    pub fn exceeded_capacity(&self) -> Capacity {
        let inputs_capacity = self.total_inputs_capacity();
        let outputs_capacity = self.total_outputs_capacity();
        if inputs_capacity > outputs_capacity {
            inputs_capacity.safe_sub(outputs_capacity).unwrap()
        } else {
            Capacity::zero()
        }
    }

    /// Lock script groups of input and output cells
    pub fn lock_script_groups(&self, lock_script: &Script) -> (Vec<usize>, Vec<usize>) {
        let mut input_groups = Vec::new();
        let mut output_groups = Vec::new();
        for (i, input) in self.inputs.iter().enumerate() {
            if &input.output.lock_script() == lock_script {
                input_groups.push(i);
            }
        }
        for (i, output) in self.outputs.iter().enumerate() {
            if &output.lock_script() == lock_script {
                output_groups.push(i);
            }
        }
        (input_groups, output_groups)
    }

    /// Type script groups of input and output cells
    pub fn type_script_groups(&self, type_script: &Script) -> (Vec<usize>, Vec<usize>) {
        let mut input_groups = Vec::new();
        let mut output_groups = Vec::new();
        for (i, input) in self.inputs.iter().enumerate() {
            if input.output.type_script() == Some(type_script.clone()) {
                input_groups.push(i);
            }
        }
        for (i, output) in self.outputs.iter().enumerate() {
            if output.type_script() == Some(type_script.clone()) {
                output_groups.push(i);
            }
        }
        (input_groups, output_groups)
    }

    /// Calculate type id based on the first input cell and output index
    pub fn calc_type_id(&self, out_index: usize) -> Result<H256> {
        let Some(first_input) = self.inputs.first() else {
            return Err(eyre!("empty input"));
        };
        let mut hasher = Blake2bBuilder::new(32)
            .personal(b"ckb-default-hash")
            .build();
        hasher.update(first_input.input.as_slice());
        hasher.update(&out_index.to_le_bytes());
        let mut type_id = [0u8; 32];
        hasher.finalize(&mut type_id);
        Ok(type_id.into())
    }

    /// Calculate transaction fee based on current minimal fee rate and additional fee rate
    pub async fn fee<T: RPC>(&self, rpc: &T, additinal_fee_rate: u64) -> Result<Capacity> {
        let fee_rate = u64::from(rpc.tx_pool_info().await?.min_fee_rate) + additinal_fee_rate;
        let tx = self.clone().into_transaction_view();
        let tx_fee = tx.data().as_slice().len() as u64 * fee_rate / 1000;
        Ok(Capacity::shannons(tx_fee))
    }

    /// Balance the transaction by adding input cells until the needed capacity is satisfied
    ///
    /// Support two modes:
    /// 1. Balance by adding an extra change cell for receiving the change capacity - ChangeReceiver::Address
    /// 2. Balance by choosing an existing output cell as the change cell - ChangeReceiver::Output
    pub async fn balance<T: RPC>(
        &mut self,
        rpc: &T,
        fee: Capacity,
        balancer: Script,
        change_receiver: ChangeReceiver,
    ) -> Result<&mut Self> {
        let change_cell_index = match change_receiver {
            ChangeReceiver::Address(changer) => {
                self.output_from_address(changer);
                self.outputs.len() - 1
            }
            ChangeReceiver::Script(changer) => {
                self.output_from_script(changer);
                self.outputs.len() - 1
            }
            ChangeReceiver::Output(index) => {
                if self.outputs.len() <= index {
                    return Err(eyre!("change output index out of range"));
                }
                index
            }
        };
        while self.exceeded_capacity() < fee {
            self.input_from_script(rpc, balancer.clone()).await?;
        }
        let exceeded_capacity_beyond_fee = self.exceeded_capacity().safe_sub(fee).unwrap();
        let old_capacity: Capacity = self.outputs[change_cell_index].output.capacity().unpack();
        let new_capacity = old_capacity.safe_add(exceeded_capacity_beyond_fee).unwrap();
        self.outputs[change_cell_index].output = self.outputs[change_cell_index]
            .output
            .clone()
            .as_builder()
            .capacity(new_capacity.pack())
            .build();
        if self.exceeded_capacity() != fee {
            return Err(eyre!("failed to balance transaction"));
        }
        Ok(self)
    }

    /// Turn into packed TransactionView
    pub fn into_transaction_view(self) -> TransactionView {
        let inputs = self.inputs.into_iter().map(|v| v.input).collect::<Vec<_>>();
        let outputs = self
            .outputs
            .into_iter()
            .map(|v| v.output)
            .collect::<Vec<_>>();
        let celldeps = self
            .celldeps
            .into_iter()
            .map(|v| v.cell_dep)
            .collect::<Vec<_>>();
        let witnesses = self
            .witnesses
            .into_iter()
            .map(|v| v.into_packed_bytes())
            .collect::<Vec<_>>();
        TransactionView::new_advanced_builder()
            .inputs(inputs)
            .outputs(outputs)
            .cell_deps(celldeps)
            .witnesses(witnesses)
            .build()
    }
}

impl Display for TransactionSkeleton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tx = self.clone().into_transaction_view();
        let tx_json = ckb_jsonrpc_types::TransactionView::from(tx);
        f.write_fmt(format_args!(
            "{}",
            serde_json::to_string_pretty(&tx_json).unwrap()
        ))
    }
}

/// Indicate how to receive the change capacity while balancing transaction
pub enum ChangeReceiver {
    /// Balance by adding an extra change cell from ckb address
    Address(Address),
    /// Balance by adding an extra change cell from lock script
    Script(Script),
    /// Balance by choosing an existing output cell
    Output(usize),
}

impl From<TransactionSkeleton> for TransactionView {
    fn from(value: TransactionSkeleton) -> Self {
        value.into_transaction_view()
    }
}
