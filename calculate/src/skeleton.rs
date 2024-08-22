use std::{fmt::Display, time::Duration};

use ckb_hash::{blake2b_256, Blake2bBuilder};
use ckb_jsonrpc_types::{OutputsValidator, Status};
use ckb_sdk::{
    rpc::ckb_indexer::{Cell, SearchMode},
    traits::{CellQueryOptions, ValueRangeOption},
    Address, AddressPayload, NetworkType,
};
use ckb_types::{
    core::{
        cell::{CellMetaBuilder, ResolvedTransaction},
        Capacity, DepType, ScriptHashType, TransactionView,
    },
    packed::{Bytes, CellDep, CellInput, CellOutput, OutPoint, OutPointVec, Script, WitnessArgs},
    prelude::{Builder, Entity, Pack, Unpack},
    H256,
};
use eyre::{eyre, Result};
use futures::future::join_all;

use crate::rpc::{GetCellsIter, RPC};

/// A wrapper of packed Script
///
/// `Reference` branch: point to a celldep in the transaction, if `usize` is MAX, point to the last one
#[derive(Clone, PartialEq, Eq)]
pub enum ScriptEx {
    Script(H256, ScriptHashType, Vec<u8>),
    Reference(String, Vec<u8>),
}

impl Default for ScriptEx {
    fn default() -> Self {
        ScriptEx::Script(H256::default(), ScriptHashType::Data, Vec::new())
    }
}

impl PartialEq<Script> for ScriptEx {
    fn eq(&self, other: &Script) -> bool {
        let Ok(script) = Script::try_from(self.clone()) else {
            return false;
        };
        &script == other
    }
}

impl ScriptEx {
    /// Initialize a ScriptEx of `Data1`
    pub fn new_code(code_hash: H256, args: Vec<u8>) -> Self {
        ScriptEx::Script(code_hash, ScriptHashType::Data1, args)
    }

    /// Initialize a ScriptEx of `Type`
    pub fn new_type(type_hash: H256, args: Vec<u8>) -> Self {
        ScriptEx::Script(type_hash, ScriptHashType::Type, args)
    }

    /// Get `code_hash` of ScriptEx
    pub fn code_hash(&self) -> eyre::Result<H256> {
        match self {
            ScriptEx::Script(code_hash, _, _) => Ok(code_hash.clone()),
            _ => Err(eyre!("reference script")),
        }
    }

    /// Get `hash_type` of ScriptEx
    pub fn hash_type(&self) -> eyre::Result<ScriptHashType> {
        match self {
            ScriptEx::Script(_, hash_type, _) => Ok(*hash_type),
            _ => Err(eyre!("reference script")),
        }
    }

    /// Get `args` of ScriptEx
    pub fn args(&self) -> Vec<u8> {
        match self {
            ScriptEx::Script(_, _, args) => args.clone(),
            ScriptEx::Reference(_, args) => args.clone(),
        }
    }

    /// Change `args` of ScriptEx
    pub fn set_args(self, args: Vec<u8>) -> Self {
        match self {
            ScriptEx::Script(code_hash, hash_type, _) => {
                ScriptEx::Script(code_hash, hash_type, args)
            }
            ScriptEx::Reference(name, _) => ScriptEx::Reference(name, args),
        }
    }

    /// Calculate blake2b hash of the script
    pub fn script_hash(&self) -> Result<H256> {
        Script::try_from(self.clone()).map(|v| v.calc_script_hash().unpack())
    }

    /// Turn into CKB address
    pub fn to_address(self, network: NetworkType) -> Result<Address> {
        let payload = Script::try_from(self)?.into();
        Ok(Address::new(network, payload, true))
    }

    /// Build packed Script from ScriptEx and TransactionSkeleton
    pub fn to_script(self, skeleton: &TransactionSkeleton) -> Result<Script> {
        if let ScriptEx::Reference(_, _) = &self {
            let (_, value) = skeleton
                .find_celldep_by_script(&self)
                .ok_or(eyre!("celldep not found"))?;
            if value.celldep.dep_type() == DepType::DepGroup.into() {
                return Err(eyre!("no support for group celldep"));
            }
            let output = &value.output;
            let mut script = Script::new_builder().args(self.args().pack());
            if let Some(celldep_type_hash) = output.calc_type_hash() {
                script = script
                    .code_hash(celldep_type_hash.pack())
                    .hash_type(ScriptHashType::Type.into());
            } else {
                if !value.with_data {
                    return Err(eyre!("celldep without data, cannot calculate data hash"));
                }
                script = script
                    .code_hash(output.data_hash().pack())
                    .hash_type(ScriptHashType::Data1.into());
            }
            Ok(script.build())
        } else {
            self.try_into()
        }
    }

    /// Build packed Script from ScriptEx, throw error if failed
    pub fn to_script_unchecked(self) -> Script {
        self.try_into().expect("unchecked to_script")
    }
}

impl TryFrom<ScriptEx> for Script {
    type Error = eyre::Error;

    fn try_from(value: ScriptEx) -> Result<Self> {
        match value {
            ScriptEx::Script(code_hash, hash_type, args) => Ok(Script::new_builder()
                .code_hash(code_hash.pack())
                .hash_type(hash_type.into())
                .args(args.pack())
                .build()),
            ScriptEx::Reference(_, _) => Err(eyre!("reference script")),
        }
    }
}

impl From<Script> for ScriptEx {
    fn from(value: Script) -> Self {
        ScriptEx::Script(
            value.code_hash().unpack(),
            value.hash_type().try_into().expect("hash type"),
            value.args().raw_data().to_vec(),
        )
    }
}

impl From<Address> for ScriptEx {
    fn from(value: Address) -> Self {
        value.payload().into()
    }
}

impl From<&AddressPayload> for ScriptEx {
    fn from(value: &AddressPayload) -> Self {
        Script::from(value).into()
    }
}

impl From<(String, Vec<u8>)> for ScriptEx {
    fn from((celldep_name, args): (String, Vec<u8>)) -> Self {
        ScriptEx::Reference(celldep_name, args)
    }
}

/// CellInput for transaction skeleton, which contains output cell and data
#[derive(Debug, Clone)]
pub struct CellInputEx {
    pub input: CellInput,
    pub output: CellOutputEx,
    pub with_data: bool,
}

impl PartialEq for CellInputEx {
    fn eq(&self, other: &Self) -> bool {
        self.input.as_bytes() == other.input.as_bytes()
    }
}

impl CellInputEx {
    /// Directly initialize a CellInputEx
    pub fn new(input: CellInput, output: CellOutput, data: Option<Vec<u8>>) -> Self {
        if let Some(data) = data {
            CellInputEx {
                input,
                output: CellOutputEx::new(output, data),
                with_data: true,
            }
        } else {
            CellInputEx {
                input,
                output: CellOutputEx::new(output, Vec::new()),
                with_data: false,
            }
        }
    }

    /// Initialize a CellInputEx from out point via CKB RPC
    pub async fn new_from_outpoint<T: RPC>(
        rpc: &T,
        tx_hash: H256,
        index: u32,
        since: Option<u64>,
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
        let input = CellInput::new_builder()
            .previous_output(out_point)
            .since(since.unwrap_or(0).pack())
            .build();
        let output = live_cell.output.into();
        let data = live_cell.data.map(|v| v.content.into_bytes().to_vec());
        Ok(Self::new(input, output, data))
    }

    /// Initialize a CellInputEx from the ckb-indexer specific cell
    pub fn new_from_indexer_cell(indexer_cell: Cell) -> Self {
        let input = CellInput::new_builder()
            .previous_output(indexer_cell.out_point.into())
            .build();
        let data = indexer_cell.output_data.map(|v| v.into_bytes().to_vec());
        Self::new(input, indexer_cell.output.into(), data)
    }

    /// Turn a CelldepEx into CellInputEx
    pub fn new_from_celldep(celldep: &CellDepEx) -> Self {
        let input = CellInput::new_builder()
            .previous_output(celldep.celldep.out_point())
            .build();
        let data = if celldep.with_data {
            Some(celldep.output.data.clone())
        } else {
            None
        };
        Self::new(input, celldep.output.output.clone(), data)
    }
}

/// CellOutput for transaction skeleton, which contains cell data
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellOutputEx {
    pub output: CellOutput,
    pub data: Vec<u8>,
}

impl CellOutputEx {
    /// Directly initialize a CellOutputEx
    pub fn new(output: CellOutput, data: Vec<u8>) -> Self {
        CellOutputEx { output, data }
    }

    /// Initialize a CellOutputEx from inner types
    pub fn new_from_scripts(
        lock_script: Script,
        type_script: Option<Script>,
        data: Vec<u8>,
        capacity: Option<Capacity>,
    ) -> Result<Self> {
        let builder = CellOutput::new_builder()
            .lock(lock_script)
            .type_(type_script.pack());
        let output = if let Some(capacity) = capacity {
            builder.capacity(capacity.pack()).build()
        } else {
            builder.build_exact_capacity(Capacity::bytes(data.len())?)?
        };
        Ok(CellOutputEx::new(output, data))
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
    pub name: String,
    pub celldep: CellDep,
    pub output: CellOutputEx,
    pub with_data: bool,
}

impl PartialEq for CellDepEx {
    fn eq(&self, other: &Self) -> bool {
        self.celldep.as_bytes() == other.celldep.as_bytes()
    }
}

impl CellDepEx {
    /// Directly initialize a CellDepEx
    pub fn new(name: String, cell_dep: CellDep, output: CellOutput, data: Option<Vec<u8>>) -> Self {
        if let Some(data) = data {
            CellDepEx {
                name,
                celldep: cell_dep,
                output: CellOutputEx::new(output, data),
                with_data: true,
            }
        } else {
            CellDepEx {
                name,
                celldep: cell_dep,
                output: CellOutputEx::new(output, Vec::new()),
                with_data: false,
            }
        }
    }

    /// Initialize a CellDepEx from out point via CKB RPC
    pub async fn new_from_outpoint<T: RPC>(
        rpc: &T,
        name: String,
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
        let data = live_cell.data.map(|v| v.content.into_bytes().to_vec());
        Ok(Self::new(name, cell_dep, output, data))
    }

    /// Initialize a CellDepEx from the ckb-indexer specific cell
    pub fn new_from_indexer_cell(name: String, indexer_cell: Cell, dep_type: DepType) -> Self {
        let out_point = indexer_cell.out_point.into();
        let cell_dep = CellDep::new_builder()
            .out_point(out_point)
            .dep_type(dep_type.into())
            .build();
        let output = indexer_cell.output.into();
        let data = indexer_cell.output_data.map(|v| v.into_bytes().into());
        Self::new(name, cell_dep, output, data)
    }

    /// Retrive cell dep's on-chain output if there's None output field
    pub async fn refresh_cell_output<T: RPC>(&mut self, rpc: &T) -> Result<()> {
        let out_point = self.celldep.out_point().to_owned();
        let new_cell_dep = Self::new_from_outpoint(
            rpc,
            self.name.clone(),
            out_point.tx_hash().unpack(),
            out_point.index().unpack(),
            self.celldep.dep_type().try_into().unwrap(),
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
pub struct WitnessEx {
    pub empty: bool,
    pub traditional: bool,
    pub lock: Vec<u8>,
    pub input_type: Vec<u8>,
    pub output_type: Vec<u8>,
}

impl Default for WitnessEx {
    fn default() -> Self {
        WitnessEx {
            empty: true,
            traditional: true,
            lock: Vec::new(),
            input_type: Vec::new(),
            output_type: Vec::new(),
        }
    }
}

impl WitnessEx {
    /// Directly initialize a WitnessArgsEx
    pub fn new(lock: Vec<u8>, input_type: Vec<u8>, output_type: Vec<u8>) -> Self {
        WitnessEx {
            empty: false,
            traditional: true,
            lock,
            input_type,
            output_type,
        }
    }

    /// Initialize a WitnessArgsEx and mark it non-traditional
    pub fn new_plain(plain_bytes: Vec<u8>) -> Self {
        WitnessEx {
            empty: false,
            traditional: false,
            lock: plain_bytes,
            input_type: Vec::new(),
            output_type: Vec::new(),
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
    pub fn into_packed_bytes(mut self) -> Bytes {
        if !self.lock.is_empty() || !self.input_type.is_empty() || !self.output_type.is_empty() {
            self.empty = false;
        }
        if self.empty {
            Bytes::default()
        } else {
            if self.traditional {
                self.into_witness_args().as_bytes().pack()
            } else {
                self.into_packed_plain_bytes()
            }
        }
    }

    /// Turn into packed raw bytes, which is normally not in format of WitnessArgs
    pub fn into_packed_plain_bytes(self) -> Bytes {
        let bytes = self
            .lock
            .into_iter()
            .chain(self.input_type)
            .chain(self.output_type)
            .collect::<Vec<_>>();
        bytes.pack()
    }
}

/// TransactionSkeleton for building transaction
#[derive(Default, Clone, Debug)]
pub struct TransactionSkeleton {
    pub inputs: Vec<CellInputEx>,
    pub outputs: Vec<CellOutputEx>,
    pub celldeps: Vec<CellDepEx>,
    pub witnesses: Vec<WitnessEx>,
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
                CellInputEx::new_from_outpoint(rpc, tx_hash, index, Some(since), true)
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
            .enumerate()
            .map(|(i, cell_dep)| {
                let name = format!("unknown-{i}");
                let out_point = cell_dep.out_point();
                let tx_hash: H256 = out_point.tx_hash().unpack();
                let index: u32 = out_point.index().unpack();
                let dep_type = cell_dep.dep_type().try_into().expect("dep type");
                CellDepEx::new_from_outpoint(rpc, name, tx_hash, index, dep_type, false)
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
                Ok(WitnessEx::new(
                    lock.raw_data().to_vec(),
                    input_type.raw_data().to_vec(),
                    output_type.raw_data().to_vec(),
                ))
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
        lock_script: ScriptEx,
    ) -> Result<&mut Self> {
        let mut search_key = CellQueryOptions::new_lock(lock_script.to_script(self)?);
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
    pub fn remove_input(&mut self, index: usize) -> Result<CellInputEx> {
        if self.inputs.len() <= index {
            return Err(eyre!("input index out of range"));
        }
        Ok(self.inputs.remove(index))
    }

    /// Pop the last input cell, which may fail if no input cell
    pub fn pop_input(&mut self) -> Result<CellInputEx> {
        self.inputs.pop().ok_or(eyre!("no input to pop"))
    }

    /// Push a single output cell
    pub fn output(&mut self, cell_output: CellOutputEx) -> &mut Self {
        self.outputs.push(cell_output);
        self
    }

    /// Push a output cell from ckb address, which is majorly used to receive capacity change
    pub fn output_from_address(&mut self, address: Address, data: Vec<u8>) -> Result<&mut Self> {
        self.output_from_script(address.payload().into(), data)
    }

    /// Push a output cell from lock script
    pub fn output_from_script(
        &mut self,
        lock_script: ScriptEx,
        data: Vec<u8>,
    ) -> Result<&mut Self> {
        let output = CellOutput::new_builder()
            .lock(lock_script.to_script(&self)?)
            .build_exact_capacity(Capacity::zero())
            .expect("build exact capacity");
        Ok(self.output(CellOutputEx::new(output, data)))
    }

    /// Push a batch of output cells
    pub fn outputs(&mut self, cell_outputs: Vec<CellOutputEx>) -> &mut Self {
        self.outputs.extend(cell_outputs);
        self
    }

    /// Remove output cell by index, which may fail if index out of range
    pub fn remove_output(&mut self, index: usize) -> Result<CellOutputEx> {
        if self.outputs.len() <= index {
            return Err(eyre!("output index out of range"));
        }
        Ok(self.outputs.remove(index))
    }

    /// Pop the last output cell, which may fail if no output cell
    pub fn pop_output(&mut self) -> Result<CellOutputEx> {
        self.outputs.pop().ok_or(eyre!("no output to pop"))
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

    /// Check if cell dep exists by name
    pub fn get_celldep_by_name(&self, name: &str) -> Option<&CellDepEx> {
        self.celldeps.iter().find(|celldep| &celldep.name == name)
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
    pub fn witness(&mut self, witness: WitnessEx) -> &mut Self {
        self.witnesses.push(witness);
        self
    }

    /// Push a batch of witnesses
    pub fn witnesses(&mut self, witnesses: Vec<WitnessEx>) -> &mut Self {
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
    pub fn lock_script_groups(&self, lock_script: &ScriptEx) -> (Vec<usize>, Vec<usize>) {
        let mut input_groups = Vec::new();
        let mut output_groups = Vec::new();
        for (i, input) in self.inputs.iter().enumerate() {
            if lock_script == &input.output.lock_script() {
                input_groups.push(i);
            }
        }
        for (i, output) in self.outputs.iter().enumerate() {
            if lock_script == &output.lock_script() {
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

    /// Find CelldepEx by script, support both type and data hash
    pub fn find_celldep_by_script(&self, script: &ScriptEx) -> Option<(usize, &CellDepEx)> {
        if let ScriptEx::Reference(name, _) = script {
            return self
                .celldeps
                .iter()
                .enumerate()
                .find_map(|(index, celldep)| {
                    if &celldep.name == name {
                        Some((index, celldep))
                    } else {
                        None
                    }
                });
        }
        let index = self
            .celldeps
            .iter()
            .enumerate()
            .find_map(|(index, celldep)| {
                let expected_code_hash =
                    match (script.hash_type(), &celldep.output, celldep.with_data) {
                        (Ok(ScriptHashType::Type), output, _) => {
                            if let Some(type_hash) = output.calc_type_hash() {
                                type_hash
                            } else {
                                H256::default()
                            }
                        }
                        (Ok(_), output, true) => output.data_hash(),
                        _ => H256::default(),
                    };
                if script.code_hash().unwrap_or_default() == expected_code_hash {
                    Some(index)
                } else {
                    None
                }
            });
        index.map(|index| (index, &self.celldeps[index]))
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
        balancer: ScriptEx,
        change_receiver: ChangeReceiver,
    ) -> Result<&mut Self> {
        let change_cell_index = match change_receiver {
            ChangeReceiver::Address(changer) => {
                self.output_from_address(changer, Default::default())?;
                self.outputs.len() - 1
            }
            ChangeReceiver::Script(changer) => {
                self.output_from_script(changer.into(), Default::default())?;
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

    /// Turn into ResolvedTransaction for contracts native debugging
    pub async fn into_resolved_transaction<T: RPC>(self, rpc: &T) -> Result<ResolvedTransaction> {
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
                v.refresh_cell_output(rpc).await?;
            }
            let output = v.output;
            if v.celldep.dep_type() == DepType::DepGroup.into() {
                // dep group data is a list of out points
                let sub_out_points = OutPointVec::from_slice(&output.data)
                    .map_err(|_| eyre!("invalid dep group"))?;
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
                    )
                    .await?;
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
        TransactionView::new_advanced_builder()
            .inputs(inputs)
            .outputs(outputs)
            .outputs_data(outputs_data)
            .cell_deps(celldeps)
            .witnesses(witnesses)
            .build()
    }

    /// Consume and send this transaction, and then wait for confirmation
    ///
    /// `confirm_count`: wait how many blocks to firm confirmation, if 0, return immidiently after sending
    /// `wait_timeout`: wait how much time until throwing timeout error, if None, no timeout
    pub async fn send_and_wait<T: RPC>(
        self,
        rpc: &T,
        confirm_count: u8,
        wait_timeout: Option<Duration>,
    ) -> Result<H256> {
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
                    return Err(eyre!("timeout waiting tx: {hash:#x}"));
                }
                time_used += interval;
            }
            tokio::time::sleep(interval).await;
            let tx = rpc
                .get_transaction(&hash)
                .await?
                .ok_or(eyre!("no tx found: {hash:#x}"))?;
            if tx.tx_status.status == Status::Rejected {
                let reason = tx.tx_status.reason.unwrap_or_else(|| "unknown".to_string());
                return Err(eyre!("tx {hash:#x} rejected, reason: {reason}"));
            }
            if tx.tx_status.status != Status::Committed {
                continue;
            }
            if block_number == 0 {
                if let Some(number) = tx.tx_status.block_number {
                    block_number = number.into();
                }
            } else {
                let tip_number = rpc.get_tip_header().await?.inner.number;
                if u64::from(tip_number) >= block_number + confirm_count as u64 {
                    break;
                }
            }
        }
        Ok(hash)
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
