//! Spore / Cluster cell operations. **Experimental** (`--features spore`).
//!
//! After the ckb-types-1 upgrade these helpers are feature-gated. Do not
//! enable them unless the user asked. Pair mint/transfer/burn with
//! [`crate::intent::MINT`] / [`crate::intent::TRANSFER`] / [`crate::intent::BURN`].

use async_trait::async_trait;
use ckb_types::{
    core::DepType,
    h256,
    prelude::{Entity, Unpack},
    H256,
};
use eyre::{eyre, Result};

use crate::{
    indexer::{CellQueryOptions, SearchKey, SearchMode},
    operation::{basic::AddOutputCell, Log, Operation},
    rpc::{GetCellsIter, Network, RPC},
    skeleton::{CellDepEx, CellInputEx, CellOutputEx, ScriptEx, TransactionSkeleton, WitnessEx},
};

/// Molecule tables/unions for Spore, Cluster, and cobuild witnesses.
pub mod schema;
use schema::{
    encode, Action, BurnSpore, ClusterDataV2, Message, MintCluster, MintSpore, SighashAll,
    SporeAction, SporeData, TransferCluster, TransferSpore, WitnessLayout,
};

use super::basic::AddCellDep;

/// The latest Spore and Cluster contract version
///
/// note: detail refers to https://github.com/sporeprotocol/spore-contract/blob/master/docs/VERSIONS.md
pub mod hardcoded {
    use crate::simulation::random_hash;

    use super::*;

    pub const SPORE_MAINNET_TX_HASH: H256 =
        h256!("0x96b198fb5ddbd1eed57ed667068f1f1e55d07907b4c0dbd38675a69ea1b69824");
    pub const SPORE_MAINNET_CODE_HASH: H256 =
        h256!("0x4a4dce1df3dffff7f8b2cd7dff7303df3b6150c9788cb75dcf6747247132b9f5");

    pub const SPORE_TESTNET_TX_HASH: H256 =
        h256!("0x5e8d2a517d50fd4bb4d01737a7952a1f1d35c8afc77240695bb569cd7d9d5a1f");
    pub const SPORE_TESTNET_CODE_HASH: H256 =
        h256!("0x685a60219309029d01310311dba953d67029170ca4848a4ff638e57002130a0d");

    pub const CLUSTER_MAINNET_TX_HASH: H256 =
        h256!("0xe464b7fb9311c5e2820e61c99afc615d6b98bdefbe318c34868c010cbd0dc938");
    pub const CLUSTER_MAINNET_CODE_HASH: H256 =
        h256!("0x7366a61534fa7c7e6225ecc0d828ea3b5366adec2b58206f2ee84995fe030075");

    pub const CLUSTER_TESTNET_TX_HASH: H256 =
        h256!("0xcebb174d6e300e26074aea2f5dbd7f694bb4fe3de52b6dfe205e54f90164510a");
    pub const CLUSTER_TESTNET_CODE_HASH: H256 =
        h256!("0x0bbe768b519d8ea7b96d58f1182eb7e6ef96c541fbd9526975077ee09f049058");

    lazy_static::lazy_static! {
        pub static ref SPORE_FAKENET_TX_HASH: H256 = random_hash().into();
        pub static ref CLUSTER_FAKENET_TX_HASH: H256 = random_hash().into();
    }

    /// Latest Spore deployment tx hash for `network` (random under fake networks).
    pub fn spore_tx_hash(network: Network) -> H256 {
        match network {
            Network::Mainnet => SPORE_MAINNET_TX_HASH,
            Network::Testnet => SPORE_TESTNET_TX_HASH,
            _ => SPORE_FAKENET_TX_HASH.clone(),
        }
    }

    /// Spore type script for `network`; fake/custom use `ScriptEx::Reference("spore", args)`.
    pub fn spore_script(network: Network, args: Vec<u8>) -> ScriptEx {
        match network {
            Network::Mainnet => ScriptEx::new_code(SPORE_MAINNET_CODE_HASH, args),
            Network::Testnet => ScriptEx::new_code(SPORE_TESTNET_CODE_HASH, args),
            _ => ("spore".to_string(), args).into(),
        }
    }

    /// Latest Cluster deployment tx hash for `network` (random under fake networks).
    pub fn cluster_tx_hash(network: Network) -> H256 {
        match network {
            Network::Mainnet => CLUSTER_MAINNET_TX_HASH,
            Network::Testnet => CLUSTER_TESTNET_TX_HASH,
            _ => CLUSTER_FAKENET_TX_HASH.clone(),
        }
    }

    /// Cluster type script for `network`; fake/custom use `ScriptEx::Reference("cluster", args)`.
    pub fn cluster_script(network: Network, args: Vec<u8>) -> ScriptEx {
        match network {
            Network::Mainnet => ScriptEx::new_code(CLUSTER_MAINNET_CODE_HASH, args),
            Network::Testnet => ScriptEx::new_code(CLUSTER_TESTNET_CODE_HASH, args),
            _ => ("cluster".to_string(), args).into(),
        }
    }
}

pub mod hookkey {
    pub use crate::intent::log::{CLUSTER_CELL_OWNER_LOCK, NEW_CLUSTER_ID, NEW_SPORE_ID};
}

/// Add the lastest Spore deployment cell into transaction skeleton according to the network type.
pub struct AddSporeCelldep {}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddSporeCelldep {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        Box::new(AddCellDep {
            name: "spore".to_string(),
            tx_hash: hardcoded::spore_tx_hash(rpc.network()),
            index: 0,
            dep_type: DepType::Code,
            with_data: false,
        })
        .run(rpc, skeleton, log)
        .await
    }
}

/// Add the lastest Cluster deployment cell into transaction skeleton according to the network type.
pub struct AddClusterCelldep {}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddClusterCelldep {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        Box::new(AddCellDep {
            name: "cluster".to_string(),
            tx_hash: hardcoded::cluster_tx_hash(rpc.network()),
            index: 0,
            dep_type: DepType::Code,
            with_data: false,
        })
        .run(rpc, skeleton, log)
        .await
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

/// Search and add cluster cell under the latest contract version with unique cluster_id
///
/// # Parameters
/// - `cluster_id`: The unique identifier of the cluster cell
/// - `authority_mode`: Indicate how to provide cluster authority while operating Spore
pub struct AddClusterCelldepByClusterId {
    /// Cluster type-script args (unique cluster id).
    pub cluster_id: H256,
    /// How the cluster owner proves authority for a Spore operation.
    pub authority_mode: ClusterAuthorityMode,
}

impl AddClusterCelldepByClusterId {
    fn search_key<T: RPC>(&self, rpc: &T, skeleton: &TransactionSkeleton) -> Result<SearchKey> {
        let args = self.cluster_id.as_bytes().to_vec();
        let cluster_type_script = hardcoded::cluster_script(rpc.network(), args);
        let mut query = CellQueryOptions::new_type(cluster_type_script.to_script(skeleton)?);
        query.script_search_mode = Some(SearchMode::Exact);
        Ok(query.into())
    }
}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddClusterCelldepByClusterId {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let name = format!("cluster-{:#x}", self.cluster_id);
        let cluster_celldep = if let Some(celldep) = skeleton.get_celldep_by_name(&name) {
            celldep
        } else {
            let search_key = self.search_key(rpc, skeleton)?;
            let Some(indexer_cell) = GetCellsIter::new(rpc, search_key).next().await? else {
                return Err(eyre!("no cluster cell (id: {:#x})", self.cluster_id));
            };
            let celldep =
                CellDepEx::new_from_indexer_cell(name, indexer_cell.clone(), DepType::Code);
            skeleton.celldep(celldep);
            skeleton.celldeps.last().unwrap()
        };
        let cluster_owner_lock_script: ScriptEx = cluster_celldep.output.lock_script().into();
        let (inputs, outputs) = skeleton.lock_script_groups(&cluster_owner_lock_script);
        // ignore the case of only one legit cell in Inputs or Outputs
        if inputs.is_empty() || outputs.is_empty() {
            log.push((
                hookkey::CLUSTER_CELL_OWNER_LOCK,
                cluster_owner_lock_script
                    .clone()
                    .to_script_unchecked()
                    .as_slice()
                    .to_vec(),
            ));
            match self.authority_mode {
                ClusterAuthorityMode::LockProxy => {
                    skeleton
                        .input_from_script(rpc, cluster_owner_lock_script.clone())
                        .await?
                        .output_from_script(cluster_owner_lock_script, vec![])?
                        .witness(Default::default());
                }
                ClusterAuthorityMode::ClusterCell => {
                    let cluster_input_cell = CellInputEx::new_from_celldep(cluster_celldep, None);
                    let cluster_output_cell = cluster_input_cell.output.clone();
                    skeleton
                        .input(cluster_input_cell)?
                        .output(cluster_output_cell)
                        .witness(Default::default());
                    Box::new(AddClusterCelldep {})
                        .run(rpc, skeleton, log)
                        .await?;
                }
                ClusterAuthorityMode::Skip => {} // do nothing
            }
        }
        Ok(())
    }
}

/// Search and add spore cell under the latest contract version with unique cluster_id
///
/// # Parameters
/// - `lock_script`: The spore owner lock script
/// - `cluster_id`: The unique identifier of the cluster cell
/// - `count`: The number of spore cells to search and add
pub struct AddSporeInputCellByClusterId {
    /// Owner lock of the spores to consume.
    pub lock_script: ScriptEx,
    /// Parent cluster id encoded in spore data.
    pub cluster_id: H256,
    /// Maximum number of matching spore cells to consume.
    pub count: usize,
}

impl AddSporeInputCellByClusterId {
    fn search_key<T: RPC>(&self, rpc: &T, skeleton: &TransactionSkeleton) -> Result<SearchKey> {
        let partial_spore_type_script = hardcoded::spore_script(rpc.network(), vec![]);
        let mut query = CellQueryOptions::new_lock(self.lock_script.clone().to_script(skeleton)?);
        query.secondary_script = Some(partial_spore_type_script.to_script(skeleton)?);
        query.with_data = Some(true);
        query.script_search_mode = Some(SearchMode::Prefix);
        Ok(query.into())
    }
}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddSporeInputCellByClusterId {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let search_key = self.search_key(rpc, skeleton)?;
        let mut searched = 0usize;
        let mut iter = GetCellsIter::new(rpc, search_key);
        while let Some(indexer_cell) = iter.next().await? {
            let spore_cell = CellInputEx::new_from_indexer_cell(indexer_cell, None);
            let spore_data: SporeData = schema::decode(&spore_cell.output.data)?;
            if spore_data.cluster_id.as_deref() != Some(self.cluster_id.as_bytes()) {
                continue;
            }
            skeleton.input(spore_cell)?.witness(Default::default());
            searched += 1;
            if searched >= self.count {
                break;
            }
        }
        Box::new(AddSporeCelldep {}).run(rpc, skeleton, log).await
    }
}

/// Search and add spore cell under the latest contract version with unique spore_id
///
/// # Parameters
/// - `spore_id`: The unique identifier of the spore cell
/// - `check_owner`: The owner lock script to check if the spore cell is owned by the passed owner
pub struct AddSporeInputCellBySporeId {
    /// Spore type-script args (unique spore id).
    pub spore_id: H256,
    /// When set, fail if the cell's lock is not this script.
    pub check_owner: Option<ScriptEx>,
}

impl AddSporeInputCellBySporeId {
    fn search_key<T: RPC>(&self, rpc: &T, skeleton: &TransactionSkeleton) -> Result<SearchKey> {
        let args = self.spore_id.as_bytes().to_vec();
        let spore_type_script = hardcoded::spore_script(rpc.network(), args);
        let mut query = CellQueryOptions::new_type(spore_type_script.to_script(skeleton)?);
        query.with_data = Some(true);
        query.script_search_mode = Some(SearchMode::Exact);
        Ok(query.into())
    }
}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddSporeInputCellBySporeId {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let search_key = self.search_key(rpc, skeleton)?;
        let Some(indexer_cell) = GetCellsIter::new(rpc, search_key).next().await? else {
            return Err(eyre!("no spore cell (id: {:#x})", self.spore_id));
        };
        let spore_cell = CellInputEx::new_from_indexer_cell(indexer_cell, None);
        if let Some(owner) = self.check_owner {
            if spore_cell.output.lock_script() != owner.to_script(skeleton)? {
                return Err(eyre!(
                    "spore cell (id: {:#x}) owner mismatch",
                    self.spore_id
                ));
            }
        }
        skeleton.input(spore_cell)?.witness(Default::default());
        Box::new(AddSporeCelldep {}).run(rpc, skeleton, log).await
    }
}

/// Add a new Spore cell under specific cluster id or not
///
/// # Parameters
/// - `lock_script`: The owner lock script
/// - `content_type`: The type of content under spore procotol, e.q. "plain/text", "text/json"
/// - `content`: The concrete content in bytes
/// - `cluster_id`: The unique identifier of the cluster cell to create from
/// - `authority_mode`: The cluster authority mode
pub struct AddSporeOutputCell {
    /// Holder lock of the new spore.
    pub lock_script: ScriptEx,
    /// MIME-like content type, e.g. `"text/plain"`.
    pub content_type: std::string::String,
    /// Spore content bytes.
    pub content: Vec<u8>,
    /// Parent cluster; `None` mints a standalone spore.
    pub cluster_id: Option<H256>,
    /// How to attach cluster authority when `cluster_id` is set.
    pub authority_mode: ClusterAuthorityMode,
}

/// Encode `SporeData` molecule bytes (content type, content, optional cluster id).
pub fn make_spore_data(content_type: &str, content: &[u8], cluster_id: Option<&H256>) -> Vec<u8> {
    encode(&SporeData {
        content_type: content_type.as_bytes().to_vec(),
        content: content.to_vec(),
        cluster_id: cluster_id.map(|id| id.as_bytes().to_vec()),
    })
    .expect("SporeData is always encodable")
}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddSporeOutputCell {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        if let Some(cluster_id) = self.cluster_id.clone() {
            Box::new(AddClusterCelldepByClusterId {
                cluster_id,
                authority_mode: self.authority_mode,
            })
            .run(rpc, skeleton, log)
            .await?;
        }
        let spore_data =
            make_spore_data(&self.content_type, &self.content, self.cluster_id.as_ref());
        let spore_type_script = hardcoded::spore_script(rpc.network(), vec![]); // later on, args will be filled with type_id
        Box::new(AddOutputCell {
            lock_script: self.lock_script,
            type_script: Some(spore_type_script),
            data: spore_data,
            capacity: 0,
            absolute_capacity: false,
            type_id: true,
        })
        .run(rpc, skeleton, log)
        .await?;
        let spore_id = skeleton.calc_type_id(skeleton.outputs.len() - 1)?;
        log.push((hookkey::NEW_SPORE_ID, spore_id.as_bytes().to_vec()));
        Box::new(AddSporeCelldep {}).run(rpc, skeleton, log).await
    }
}

/// Search and add cluster cell from transaction skeleton's input cells by index
///
/// # Parameters
/// - `input_index`: The index of input cell in transaction skeleton
pub struct AddClusterInputCellByClusterId {
    /// Cluster type-script args (unique cluster id).
    pub cluster_id: H256,
}

impl AddClusterInputCellByClusterId {
    fn search_key<T: RPC>(&self, rpc: &T, skeleton: &TransactionSkeleton) -> Result<SearchKey> {
        let args = self.cluster_id.as_bytes().to_vec();
        let cluster_type_script = hardcoded::cluster_script(rpc.network(), args);
        let mut query = CellQueryOptions::new_type(cluster_type_script.to_script(skeleton)?);
        query.with_data = Some(true);
        query.script_search_mode = Some(SearchMode::Exact);
        Ok(query.into())
    }
}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddClusterInputCellByClusterId {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let search_key = self.search_key(rpc, skeleton)?;
        let Some(indexer_cell) = GetCellsIter::new(rpc, search_key).next().await? else {
            return Err(eyre!("no cluster cell (id: {:#x})", self.cluster_id));
        };
        let cluster_cell = CellInputEx::new_from_indexer_cell(indexer_cell, None);
        skeleton.input(cluster_cell)?.witness(Default::default());
        Box::new(AddClusterCelldep {}).run(rpc, skeleton, log).await
    }
}

/// Add a new Cluster cell
///
/// # Parameters
/// - `lock_script`: The owner lock script
/// - `name`: The name of the cluster
/// - `description`: The description of the cluster
/// - `cluster_id_collector`: The callback function to collect the generated cluster id
pub struct AddClusterOutputCell {
    /// Holder lock of the new cluster.
    pub lock_script: ScriptEx,
    /// Cluster display name.
    pub name: std::string::String,
    /// Cluster description bytes.
    pub description: Vec<u8>,
}

/// Encode `ClusterDataV2` molecule bytes (name + description, empty mutant id).
pub fn make_cluster_data(name: &str, description: &[u8]) -> Vec<u8> {
    encode(&ClusterDataV2 {
        name: name.as_bytes().to_vec(),
        description: description.to_vec(),
        mutant_id: None,
    })
    .expect("ClusterDataV2 is always encodable")
}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddClusterOutputCell {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let cluster_data = make_cluster_data(&self.name, &self.description);
        let cluster_type_script = hardcoded::cluster_script(rpc.network(), vec![]); // later on, args will be filled with type_id
        Box::new(AddOutputCell {
            lock_script: self.lock_script,
            type_script: Some(cluster_type_script),
            data: cluster_data,
            capacity: 0,
            absolute_capacity: false,
            type_id: true,
        })
        .run(rpc, skeleton, log)
        .await?;
        let cluster_id = skeleton.calc_type_id(skeleton.outputs.len() - 1)?;
        log.push((hookkey::NEW_CLUSTER_ID, cluster_id.as_bytes().to_vec()));
        Box::new(AddClusterCelldep {}).run(rpc, skeleton, log).await
    }
}

/// Search spore related cells from transaction skeleton and parse the operations' intention to spore actions
///
/// note: this is essential for a historical issue of co-build project
pub struct AddSporeActions {
    /// When true, fail if no spore/cluster action could be inferred.
    pub restrict: bool,
}

impl AddSporeActions {
    fn compare_code_hash(cell: &CellOutputEx, code_hash: &H256) -> Option<(CellOutputEx, H256)> {
        if let Some(type_script) = cell.type_script() {
            if &Unpack::<H256>::unpack(&type_script.code_hash()) == code_hash {
                let unique_id: [u8; 32] =
                    type_script.args().raw_data().to_vec().try_into().unwrap();
                return Some((cell.clone(), unique_id.into()));
            }
        }
        None
    }
}

#[async_trait(?Send)]
impl<T: RPC> Operation<T> for AddSporeActions {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let mut spore_actions: Vec<Action> = vec![];
        // prepare spore related action parameters
        if let Ok(spore) = hardcoded::spore_script(rpc.network(), vec![]).to_script(skeleton) {
            let spore_code_hash = spore.code_hash().unpack();
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
            // handle spore transfers and burns
            for (input, spore_id) in spore_input_cells {
                if let Some((i, (output, _))) = spore_output_cells
                    .iter()
                    .enumerate()
                    .find(|(_, (output, _))| output.type_script() == input.type_script())
                {
                    let transfer_action = SporeAction::TransferSpore(TransferSpore {
                        from: input.lock_script().into(),
                        to: output.lock_script().into(),
                        spore_id: spore_id.0,
                    });
                    spore_actions.push(Action::spore(
                        &output.type_script().unwrap(),
                        &transfer_action,
                    )?);
                    spore_output_cells.remove(i);
                } else {
                    let burn_action = SporeAction::BurnSpore(BurnSpore {
                        spore_id: spore_id.0,
                        from: input.lock_script().into(),
                    });
                    spore_actions.push(Action::spore(&input.type_script().unwrap(), &burn_action)?);
                }
            }
            // handle spore mints
            for (output, spore_id) in spore_output_cells {
                let mint_action = SporeAction::MintSpore(MintSpore {
                    spore_id: spore_id.0,
                    to: output.lock_script().into(),
                    data_hash: output.data_hash().0,
                });
                spore_actions.push(Action::spore(&output.type_script().unwrap(), &mint_action)?);
            }
        }
        // prepare cluster related action parameters
        if let Ok(cluster) = hardcoded::cluster_script(rpc.network(), vec![]).to_script(skeleton) {
            let cluster_code_hash = cluster.code_hash().unpack();
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
            // handle cluster transfers
            for (input, cluster_id) in cluster_input_cells {
                if let Some((i, (output, _))) = cluster_output_cells
                    .iter()
                    .enumerate()
                    .find(|(_, (output, _))| output.type_script() == input.type_script())
                {
                    let transfer_action = SporeAction::TransferCluster(TransferCluster {
                        from: input.lock_script().into(),
                        to: output.lock_script().into(),
                        cluster_id: cluster_id.0,
                    });
                    spore_actions.push(Action::spore(
                        &output.type_script().unwrap(),
                        &transfer_action,
                    )?);
                    cluster_output_cells.remove(i);
                }
            }
            // handle cluster mints
            for (output, cluster_id) in cluster_output_cells {
                let mint_action = SporeAction::MintCluster(MintCluster {
                    cluster_id: cluster_id.0,
                    to: output.lock_script().into(),
                    data_hash: output.data_hash().0,
                });
                spore_actions.push(Action::spore(&output.type_script().unwrap(), &mint_action)?);
            }
        }
        if spore_actions.is_empty() {
            if self.restrict {
                return Err(eyre!("no spore/cluster actions found"));
            } else {
                return Ok(());
            }
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
