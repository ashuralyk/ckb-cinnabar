//! Spore / Cluster encode, output, indexer collect, and cobuild actions.
//!
//! **Experimental** (`--features spore`). Pair mint/transfer/burn with
//! [`crate::kernel::intent::MINT`] / [`TRANSFER`](crate::kernel::intent::TRANSFER)
//! / [`BURN`](crate::kernel::intent::BURN).

/// Molecule tables/unions for Spore, Cluster, and cobuild witnesses.
pub mod schema;

use alloc::{boxed::Box, format, string::String, vec::Vec};

use crate::kernel::{
    error::{CalculatorError, Result},
    indexer::{CellQueryOptions, GetCellsIter, SearchKey, SearchMode},
    intent::log::{CLUSTER_CELL_OWNER_LOCK, NEW_CLUSTER_ID, NEW_SPORE_ID},
    network::Network,
    operation::{
        basic::{AddCellDep, AddOutputCell, AddWitnessArgs},
        Log, Operation,
    },
    rpc::RPC,
    skeleton::{CellDepEx, CellInputEx, CellOutputEx, ScriptEx, TransactionSkeleton, WitnessEx},
    source::Source,
    types::{format_hash, unpack_hash, DepType, Entity, Hash256},
};

use schema::{
    decode, encode, encode_cobuild_action, make_cluster_data, make_spore_data, Action, BurnSpore,
    Message, MintCluster, MintSpore, SighashAll, SporeAction, SporeData, TransferCluster,
    TransferSpore, WitnessLayout,
};

/// Mainnet Spore type-script code hash (`Data1`).
pub const SPORE_MAINNET_CODE_HASH: Hash256 = [
    0x4a, 0x4d, 0xce, 0x1d, 0xf3, 0xdf, 0xff, 0xf7, 0xf8, 0xb2, 0xcd, 0x7d, 0xff, 0x73, 0x03, 0xdf,
    0x3b, 0x61, 0x50, 0xc9, 0x78, 0x8c, 0xb7, 0x5d, 0xcf, 0x67, 0x47, 0x24, 0x71, 0x32, 0xb9, 0xf5,
];
/// Testnet Spore type-script code hash (`Data1`).
pub const SPORE_TESTNET_CODE_HASH: Hash256 = [
    0x68, 0x5a, 0x60, 0x21, 0x93, 0x09, 0x02, 0x9d, 0x01, 0x31, 0x03, 0x11, 0xdb, 0xa9, 0x53, 0xd6,
    0x70, 0x29, 0x17, 0x0c, 0xa4, 0x84, 0x8a, 0x4f, 0xf6, 0x38, 0xe5, 0x70, 0x02, 0x13, 0x0a, 0x0d,
];
/// Mainnet Cluster type-script code hash (`Data1`).
pub const CLUSTER_MAINNET_CODE_HASH: Hash256 = [
    0x73, 0x66, 0xa6, 0x15, 0x34, 0xfa, 0x7c, 0x7e, 0x62, 0x25, 0xec, 0xc0, 0xd8, 0x28, 0xea, 0x3b,
    0x53, 0x66, 0xad, 0xec, 0x2b, 0x58, 0x20, 0x6f, 0x2e, 0xe8, 0x49, 0x95, 0xfe, 0x03, 0x00, 0x75,
];
/// Testnet Cluster type-script code hash (`Data1`).
pub const CLUSTER_TESTNET_CODE_HASH: Hash256 = [
    0x0b, 0xbe, 0x76, 0x8b, 0x51, 0x9d, 0x8e, 0xa7, 0xb9, 0x6d, 0x58, 0xf1, 0x18, 0x2e, 0xb7, 0xe6,
    0xef, 0x96, 0xc5, 0x41, 0xfb, 0xd9, 0x52, 0x69, 0x75, 0x07, 0x7e, 0xe0, 0x9f, 0x04, 0x90, 0x58,
];

const FAKE_CODE_HASH: Hash256 = [0xfa; 32];

/// Latest Spore deployment tx hash on mainnet.
pub const SPORE_MAINNET_TX_HASH: Hash256 = [
    0x96, 0xb1, 0x98, 0xfb, 0x5d, 0xdb, 0xd1, 0xee, 0xd5, 0x7e, 0xd6, 0x67, 0x06, 0x8f, 0x1f, 0x1e,
    0x55, 0xd0, 0x79, 0x07, 0xb4, 0xc0, 0xdb, 0xd3, 0x86, 0x75, 0xa6, 0x9e, 0xa1, 0xb6, 0x98, 0x24,
];

/// Latest Spore deployment tx hash on testnet.
pub const SPORE_TESTNET_TX_HASH: Hash256 = [
    0x5e, 0x8d, 0x2a, 0x51, 0x7d, 0x50, 0xfd, 0x4b, 0xb4, 0xd0, 0x17, 0x37, 0xa7, 0x95, 0x2a, 0x1f,
    0x1d, 0x35, 0xc8, 0xaf, 0xc7, 0x72, 0x40, 0x69, 0x5b, 0xb5, 0x69, 0xcd, 0x7d, 0x9d, 0x5a, 0x1f,
];

/// Sentinel Spore out-point hash used only on fake / custom networks.
pub const SPORE_FAKENET_TX_HASH: Hash256 = [0x50; 32];

/// Latest Cluster deployment tx hash on mainnet.
pub const CLUSTER_MAINNET_TX_HASH: Hash256 = [
    0xe4, 0x64, 0xb7, 0xfb, 0x93, 0x11, 0xc5, 0xe2, 0x82, 0x0e, 0x61, 0xc9, 0x9a, 0xfc, 0x61, 0x5d,
    0x6b, 0x98, 0xbd, 0xef, 0xbe, 0x31, 0x8c, 0x34, 0x86, 0x8c, 0x01, 0x0c, 0xbd, 0x0d, 0xc9, 0x38,
];

/// Latest Cluster deployment tx hash on testnet.
pub const CLUSTER_TESTNET_TX_HASH: Hash256 = [
    0xce, 0xbb, 0x17, 0x4d, 0x6e, 0x30, 0x0e, 0x26, 0x07, 0x4a, 0xea, 0x2f, 0x5d, 0xbd, 0x7f, 0x69,
    0x4b, 0xb4, 0xfe, 0x3d, 0xe5, 0x2b, 0x6d, 0xfe, 0x20, 0x5e, 0x54, 0xf9, 0x01, 0x64, 0x51, 0x0a,
];

/// Sentinel Cluster out-point hash used only on fake / custom networks.
pub const CLUSTER_FAKENET_TX_HASH: Hash256 = [0xc1; 32];

/// Latest Spore deployment tx hash for `network`.
pub fn spore_tx_hash(network: &Network) -> Hash256 {
    match network {
        Network::Mainnet => SPORE_MAINNET_TX_HASH,
        Network::Testnet => SPORE_TESTNET_TX_HASH,
        _ => SPORE_FAKENET_TX_HASH,
    }
}

/// Latest Cluster deployment tx hash for `network`.
pub fn cluster_tx_hash(network: &Network) -> Hash256 {
    match network {
        Network::Mainnet => CLUSTER_MAINNET_TX_HASH,
        Network::Testnet => CLUSTER_TESTNET_TX_HASH,
        _ => CLUSTER_FAKENET_TX_HASH,
    }
}

/// Spore type script for `network`. Fake / custom use a [`ScriptEx::Reference`].
pub fn spore_script(network: &Network, args: Vec<u8>) -> ScriptEx {
    match network {
        Network::Mainnet => ScriptEx::new_code(SPORE_MAINNET_CODE_HASH, args),
        Network::Testnet => ScriptEx::new_code(SPORE_TESTNET_CODE_HASH, args),
        _ => (String::from("spore"), args).into(),
    }
}

/// Cluster type script for `network`. Fake / custom use a [`ScriptEx::Reference`].
pub fn cluster_script(network: &Network, args: Vec<u8>) -> ScriptEx {
    match network {
        Network::Mainnet => ScriptEx::new_code(CLUSTER_MAINNET_CODE_HASH, args),
        Network::Testnet => ScriptEx::new_code(CLUSTER_TESTNET_CODE_HASH, args),
        _ => (String::from("cluster"), args).into(),
    }
}

/// Placeholder code hash used only in documentation / Fake fallbacks.
pub fn fake_code_hash() -> Hash256 {
    FAKE_CODE_HASH
}

/// Mint a spore output. Cluster lookup is **not** performed; inject the cluster
/// cell first when `cluster_id` is set.
pub struct AddSporeOutputCell {
    /// Holder lock of the new spore.
    pub lock_script: ScriptEx,
    /// MIME-like content type, e.g. `"text/plain"`.
    pub content_type: String,
    /// Spore content bytes.
    pub content: Vec<u8>,
    /// Parent cluster; `None` mints a standalone spore.
    pub cluster_id: Option<Hash256>,
    /// Network used to pick the Spore type script (canonical hash vs named dep).
    pub network: Network,
}

impl<S: Source> Operation<S> for AddSporeOutputCell {
    fn run(
        self: Box<Self>,
        source: &S,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let spore_data =
            make_spore_data(&self.content_type, &self.content, self.cluster_id.as_ref());
        Box::new(AddOutputCell {
            lock_script: self.lock_script,
            type_script: Some(spore_script(&self.network, Vec::new())),
            data: spore_data,
            capacity: 0,
            absolute_capacity: false,
            type_id: true,
        })
        .run(source, skeleton, log)?;
        let spore_id = skeleton.calc_type_id(skeleton.outputs.len() - 1)?;
        log.push((NEW_SPORE_ID, spore_id.to_vec()));
        Ok(())
    }
}

/// Mint a cluster output from already-known name/description bytes.
pub struct AddClusterOutputCell {
    /// Holder lock of the new cluster.
    pub lock_script: ScriptEx,
    /// Cluster display name.
    pub name: String,
    /// Cluster description bytes.
    pub description: Vec<u8>,
    /// Network used to pick the Cluster type script (canonical hash vs named dep).
    pub network: Network,
}

impl<S: Source> Operation<S> for AddClusterOutputCell {
    fn run(
        self: Box<Self>,
        source: &S,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let cluster_data = make_cluster_data(&self.name, &self.description);
        Box::new(AddOutputCell {
            lock_script: self.lock_script,
            type_script: Some(cluster_script(&self.network, Vec::new())),
            data: cluster_data,
            capacity: 0,
            absolute_capacity: false,
            type_id: true,
        })
        .run(source, skeleton, log)?;
        let cluster_id = skeleton.calc_type_id(skeleton.outputs.len() - 1)?;
        log.push((NEW_CLUSTER_ID, cluster_id.to_vec()));
        Ok(())
    }
}

/// Pack cobuild `Action` bytes for a script already on the skeleton.
pub struct AddCobuildActionBytes {
    pub script_info_hash: Hash256,
    pub script_hash: Hash256,
    pub data: Vec<u8>,
}

impl<S: Source> Operation<S> for AddCobuildActionBytes {
    fn run(
        self: Box<Self>,
        _source: &S,
        skeleton: &mut TransactionSkeleton,
        _log: &mut Log,
    ) -> Result<()> {
        let bytes = encode_cobuild_action(&self.script_info_hash, &self.script_hash, &self.data);
        AddWitnessArgs {
            witness_index: None,
            lock: Vec::new(),
            input_type: Vec::new(),
            output_type: bytes,
        }
        .apply(skeleton)
    }
}

/// Add the latest Spore deployment cell dep for the current network.
pub struct AddSporeCelldep {}

impl<C: RPC> Operation<C> for AddSporeCelldep {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        Box::new(AddCellDep {
            name: String::from("spore"),
            tx_hash: spore_tx_hash(&rpc.network()),
            index: 0,
            dep_type: DepType::Code,
            with_data: false,
        })
        .run(rpc, skeleton, log)
    }
}

/// Add the latest Cluster deployment cell dep for the current network.
pub struct AddClusterCelldep {}

impl<C: RPC> Operation<C> for AddClusterCelldep {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        Box::new(AddCellDep {
            name: String::from("cluster"),
            tx_hash: cluster_tx_hash(&rpc.network()),
            index: 0,
            dep_type: DepType::Code,
            with_data: false,
        })
        .run(rpc, skeleton, log)
    }
}

/// How a Spore mint proves cluster authority.
#[derive(Clone)]
pub enum ClusterAuthorityMode {
    /// Put a lock-proxy of the cluster owner into cell deps.
    LockProxy,
    /// Put the cluster cell itself into cell deps (and consume it as input if needed).
    ClusterCell,
    /// Do not attach cluster authority (standalone spore, or already present).
    Skip,
}

/// Search and add a cluster cell by unique cluster id.
pub struct AddClusterCelldepByClusterId {
    /// Cluster type-script args (unique cluster id).
    pub cluster_id: Hash256,
    /// How the cluster owner proves authority for a Spore operation.
    pub authority_mode: ClusterAuthorityMode,
}

impl AddClusterCelldepByClusterId {
    fn search_key(&self, network: &Network, skeleton: &TransactionSkeleton) -> Result<SearchKey> {
        let cluster_type = cluster_script(network, self.cluster_id.to_vec());
        let mut query = CellQueryOptions::new_type(cluster_type.to_script(skeleton)?);
        query.script_search_mode = Some(SearchMode::Exact);
        Ok(query.into())
    }
}

impl<C: RPC> Operation<C> for AddClusterCelldepByClusterId {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let name = format!("cluster-{}", format_hash(&self.cluster_id));
        let cluster_celldep = if let Some(celldep) = skeleton.get_celldep_by_name(&name) {
            celldep.clone()
        } else {
            let search_key = self.search_key(&rpc.network(), skeleton)?;
            let Some(cell) = GetCellsIter::new(rpc, search_key).next()? else {
                return Err(CalculatorError::NoAvailableCells(format!(
                    "no cluster cell (id: {})",
                    format_hash(&self.cluster_id)
                )));
            };
            let celldep = CellDepEx::new_from_live_cell(name, cell, DepType::Code);
            skeleton.celldep(celldep.clone());
            celldep
        };
        let cluster_owner_lock_script: ScriptEx = cluster_celldep.output.lock_script().into();
        let (inputs, outputs) = skeleton.lock_script_groups(&cluster_owner_lock_script);
        if inputs.is_empty() || outputs.is_empty() {
            log.push((
                CLUSTER_CELL_OWNER_LOCK,
                cluster_owner_lock_script
                    .clone()
                    .to_script_unchecked()
                    .as_slice()
                    .to_vec(),
            ));
            match self.authority_mode {
                ClusterAuthorityMode::LockProxy => {
                    skeleton
                        .input_from_script(rpc, cluster_owner_lock_script.clone())?
                        .output_from_script(cluster_owner_lock_script, Vec::new())?
                        .witness(Default::default());
                }
                ClusterAuthorityMode::ClusterCell => {
                    let cluster_input = CellInputEx::new_from_celldep(&cluster_celldep, None);
                    let cluster_output = cluster_input.output.clone();
                    skeleton
                        .input(cluster_input)?
                        .output(cluster_output)
                        .witness(Default::default());
                    Box::new(AddClusterCelldep {}).run(rpc, skeleton, log)?;
                }
                ClusterAuthorityMode::Skip => {}
            }
        }
        Ok(())
    }
}

/// Search and add spore cells owned by `lock_script` that belong to `cluster_id`.
pub struct AddSporeInputCellByClusterId {
    /// Owner lock of the spores to consume.
    pub lock_script: ScriptEx,
    /// Parent cluster id encoded in spore data.
    pub cluster_id: Hash256,
    /// Maximum number of matching spore cells to consume.
    pub count: usize,
}

impl AddSporeInputCellByClusterId {
    fn search_key(&self, network: &Network, skeleton: &TransactionSkeleton) -> Result<SearchKey> {
        let partial_spore = spore_script(network, Vec::new());
        let mut query = CellQueryOptions::new_lock(self.lock_script.clone().to_script(skeleton)?);
        query.secondary_script = Some(partial_spore.to_script(skeleton)?);
        query.with_data = Some(true);
        query.script_search_mode = Some(SearchMode::Prefix);
        Ok(query.into())
    }
}

impl<C: RPC> Operation<C> for AddSporeInputCellByClusterId {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let search_key = self.search_key(&rpc.network(), skeleton)?;
        let mut searched = 0usize;
        let mut iter = GetCellsIter::new(rpc, search_key);
        while let Some(cell) = iter.next()? {
            let spore_cell = CellInputEx::new_from_live_cell(cell, None);
            let spore_data: SporeData = decode(&spore_cell.output.data)?;
            if spore_data.cluster_id.as_deref() != Some(self.cluster_id.as_slice()) {
                continue;
            }
            skeleton.input(spore_cell)?.witness(Default::default());
            searched += 1;
            if searched >= self.count {
                break;
            }
        }
        Box::new(AddSporeCelldep {}).run(rpc, skeleton, log)
    }
}

/// Search and add a spore cell by unique spore id.
pub struct AddSporeInputCellBySporeId {
    /// Spore type-script args (unique spore id).
    pub spore_id: Hash256,
    /// When set, fail if the cell's lock is not this script.
    pub check_owner: Option<ScriptEx>,
}

impl AddSporeInputCellBySporeId {
    fn search_key(&self, network: &Network, skeleton: &TransactionSkeleton) -> Result<SearchKey> {
        let spore_type = spore_script(network, self.spore_id.to_vec());
        let mut query = CellQueryOptions::new_type(spore_type.to_script(skeleton)?);
        query.with_data = Some(true);
        query.script_search_mode = Some(SearchMode::Exact);
        Ok(query.into())
    }
}

impl<C: RPC> Operation<C> for AddSporeInputCellBySporeId {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let search_key = self.search_key(&rpc.network(), skeleton)?;
        let Some(cell) = GetCellsIter::new(rpc, search_key).next()? else {
            return Err(CalculatorError::NoAvailableCells(format!(
                "no spore cell (id: {})",
                format_hash(&self.spore_id)
            )));
        };
        let spore_cell = CellInputEx::new_from_live_cell(cell, None);
        if let Some(owner) = self.check_owner {
            if spore_cell.output.lock_script() != owner.to_script(skeleton)? {
                return Err(CalculatorError::Other(format!(
                    "spore cell (id: {}) owner mismatch",
                    format_hash(&self.spore_id)
                )));
            }
        }
        skeleton.input(spore_cell)?.witness(Default::default());
        Box::new(AddSporeCelldep {}).run(rpc, skeleton, log)
    }
}

/// Search and add a cluster cell by unique cluster id.
pub struct AddClusterInputCellByClusterId {
    /// Cluster type-script args (unique cluster id).
    pub cluster_id: Hash256,
}

impl AddClusterInputCellByClusterId {
    fn search_key(&self, network: &Network, skeleton: &TransactionSkeleton) -> Result<SearchKey> {
        let cluster_type = cluster_script(network, self.cluster_id.to_vec());
        let mut query = CellQueryOptions::new_type(cluster_type.to_script(skeleton)?);
        query.with_data = Some(true);
        query.script_search_mode = Some(SearchMode::Exact);
        Ok(query.into())
    }
}

impl<C: RPC> Operation<C> for AddClusterInputCellByClusterId {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let search_key = self.search_key(&rpc.network(), skeleton)?;
        let Some(cell) = GetCellsIter::new(rpc, search_key).next()? else {
            return Err(CalculatorError::NoAvailableCells(format!(
                "no cluster cell (id: {})",
                format_hash(&self.cluster_id)
            )));
        };
        skeleton
            .input(CellInputEx::new_from_live_cell(cell, None))?
            .witness(Default::default());
        Box::new(AddClusterCelldep {}).run(rpc, skeleton, log)
    }
}

/// Infer cobuild Spore/Cluster actions from skeleton inputs and outputs.
pub struct AddSporeActions {
    /// When true, fail if no spore/cluster action could be inferred.
    pub restrict: bool,
}

impl AddSporeActions {
    fn compare_code_hash(
        cell: &CellOutputEx,
        code_hash: &Hash256,
    ) -> Option<(CellOutputEx, Hash256)> {
        let type_script = cell.type_script()?;
        if unpack_hash(&type_script.code_hash()) != *code_hash {
            return None;
        }
        let unique_id: Hash256 = type_script.args().raw_data().as_ref().try_into().ok()?;
        Some((cell.clone(), unique_id))
    }
}

impl<C: RPC> Operation<C> for AddSporeActions {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let mut spore_actions: Vec<Action> = Vec::new();
        if let Ok(spore) = spore_script(&rpc.network(), Vec::new()).to_script(skeleton) {
            let spore_code_hash = unpack_hash(&spore.code_hash());
            let mut spore_output_cells = skeleton
                .outputs
                .iter()
                .filter_map(|cell| Self::compare_code_hash(cell, &spore_code_hash))
                .collect::<Vec<_>>();
            let spore_input_cells = skeleton
                .inputs
                .iter()
                .filter_map(|cell| Self::compare_code_hash(&cell.output, &spore_code_hash))
                .collect::<Vec<_>>();
            for (input, spore_id) in spore_input_cells {
                if let Some((i, (output, _))) = spore_output_cells
                    .iter()
                    .enumerate()
                    .find(|(_, (output, _))| output.type_script() == input.type_script())
                {
                    let transfer_action = SporeAction::TransferSpore(TransferSpore {
                        from: input.lock_script().into(),
                        to: output.lock_script().into(),
                        spore_id,
                    });
                    spore_actions.push(Action::spore(
                        &output.type_script().unwrap(),
                        &transfer_action,
                    )?);
                    spore_output_cells.remove(i);
                } else {
                    let burn_action = SporeAction::BurnSpore(BurnSpore {
                        spore_id,
                        from: input.lock_script().into(),
                    });
                    spore_actions.push(Action::spore(&input.type_script().unwrap(), &burn_action)?);
                }
            }
            for (output, spore_id) in spore_output_cells {
                let mint_action = SporeAction::MintSpore(MintSpore {
                    spore_id,
                    to: output.lock_script().into(),
                    data_hash: output.data_hash(),
                });
                spore_actions.push(Action::spore(&output.type_script().unwrap(), &mint_action)?);
            }
        }
        if let Ok(cluster) = cluster_script(&rpc.network(), Vec::new()).to_script(skeleton) {
            let cluster_code_hash = unpack_hash(&cluster.code_hash());
            let mut cluster_output_cells = skeleton
                .outputs
                .iter()
                .filter_map(|cell| Self::compare_code_hash(cell, &cluster_code_hash))
                .collect::<Vec<_>>();
            let cluster_input_cells = skeleton
                .inputs
                .iter()
                .filter_map(|cell| Self::compare_code_hash(&cell.output, &cluster_code_hash))
                .collect::<Vec<_>>();
            for (input, cluster_id) in cluster_input_cells {
                if let Some((i, (output, _))) = cluster_output_cells
                    .iter()
                    .enumerate()
                    .find(|(_, (output, _))| output.type_script() == input.type_script())
                {
                    let transfer_action = SporeAction::TransferCluster(TransferCluster {
                        from: input.lock_script().into(),
                        to: output.lock_script().into(),
                        cluster_id,
                    });
                    spore_actions.push(Action::spore(
                        &output.type_script().unwrap(),
                        &transfer_action,
                    )?);
                    cluster_output_cells.remove(i);
                }
            }
            for (output, cluster_id) in cluster_output_cells {
                let mint_action = SporeAction::MintCluster(MintCluster {
                    cluster_id,
                    to: output.lock_script().into(),
                    data_hash: output.data_hash(),
                });
                spore_actions.push(Action::spore(&output.type_script().unwrap(), &mint_action)?);
            }
        }
        if spore_actions.is_empty() {
            if self.restrict {
                return Err(CalculatorError::Other(
                    "no spore/cluster actions found".into(),
                ));
            }
            return Ok(());
        }
        let witness_layout = WitnessLayout::SighashAll(SighashAll {
            seal: Vec::new(),
            message: Message {
                actions: spore_actions,
            },
        });
        skeleton.witness(WitnessEx::new_plain(encode(&witness_layout)?));
        Ok(())
    }
}
