use async_trait::async_trait;
use ckb_sdk::{
    rpc::ckb_indexer::{SearchKey, SearchMode},
    traits::CellQueryOptions,
};
use ckb_types::{
    core::DepType,
    h256,
    prelude::{Builder, Entity, Pack, Unpack},
    H256,
};
use eyre::{eyre, Result};

use crate::{
    operation::{basic::AddOutputCell, Log, Operation},
    rpc::{GetCellsIter, Network, RPC},
    skeleton::{CellDepEx, CellInputEx, CellOutputEx, ScriptEx, TransactionSkeleton, WitnessEx},
};

pub mod generated;
use generated::*;

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

    pub fn spore_tx_hash(network: Network) -> H256 {
        match network {
            Network::Mainnet => SPORE_MAINNET_TX_HASH,
            Network::Testnet => SPORE_TESTNET_TX_HASH,
            _ => SPORE_FAKENET_TX_HASH.clone(),
        }
    }

    pub fn spore_script(network: Network, args: Vec<u8>) -> ScriptEx {
        match network {
            Network::Mainnet => ScriptEx::new_code(SPORE_MAINNET_CODE_HASH, args),
            Network::Testnet => ScriptEx::new_code(SPORE_TESTNET_CODE_HASH, args),
            _ => ("spore".to_string(), args).into(),
        }
    }

    pub fn cluster_tx_hash(network: Network) -> H256 {
        match network {
            Network::Mainnet => CLUSTER_MAINNET_TX_HASH,
            Network::Testnet => CLUSTER_TESTNET_TX_HASH,
            _ => CLUSTER_FAKENET_TX_HASH.clone(),
        }
    }

    pub fn cluster_script(network: Network, args: Vec<u8>) -> ScriptEx {
        match network {
            Network::Mainnet => ScriptEx::new_code(CLUSTER_MAINNET_CODE_HASH, args),
            Network::Testnet => ScriptEx::new_code(CLUSTER_TESTNET_CODE_HASH, args),
            _ => ("cluster".to_string(), args).into(),
        }
    }
}

pub mod hookkey {
    /// The owner lock script of cluster cell that put in transaction's Inputs and Outputs field, which means it
    /// should have matched signature in Witnesses
    pub const CLUSTER_CELL_OWNER_LOCK: &str = "CLUSTER_CELL_OWNER_LOCK";
    /// The new generated cluster unique id when creating new cluster cell in Outputs field
    pub const NEW_CLUSTER_ID: &str = "NEW_CLUSTER_ID";
    /// The new generated spore unique id when creating new spore cell in Outputs field
    pub const NEW_SPORE_ID: &str = "NEW_SPORE_ID";
}

/// Add the lastest Spore deployment cell into transaction skeleton according to the network type.
pub struct AddSporeCelldep {}

#[async_trait]
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

#[async_trait]
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

#[derive(Clone)]
pub enum ClusterAuthorityMode {
    LockProxy,
    ClusterCell,
    Skip,
}

/// Search and add cluster cell under the latest contract version with unique cluster_id
///
/// # Parameters
/// - `cluster_id`: The unique identifier of the cluster cell
/// - `authority_mode`: Indicate how to provide cluster authority while operating Spore
pub struct AddClusterCelldepByClusterId {
    pub cluster_id: H256,
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

#[async_trait]
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
            log.insert(
                hookkey::CLUSTER_CELL_OWNER_LOCK,
                cluster_owner_lock_script
                    .clone()
                    .to_script_unchecked()
                    .as_slice()
                    .to_vec(),
            );
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

/// Search and add spore cell under the latest contract version with unique spore_id
///
/// # Parameters
/// - `spore_id`: The unique identifier of the spore cell
/// - `check_owner`: The owner lock script to check if the spore cell is owned by the passed owner
pub struct AddSporeInputCellBySporeId {
    pub spore_id: H256,
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

#[async_trait]
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
    pub lock_script: ScriptEx,
    pub content_type: String,
    pub content: Vec<u8>,
    pub cluster_id: Option<H256>,
    pub authority_mode: ClusterAuthorityMode,
}

pub fn make_spore_data(content_type: &str, content: &[u8], cluster_id: Option<&H256>) -> Vec<u8> {
    let molecule_spore_data = SporeData::new_builder()
        .content_type(content_type.as_bytes().pack())
        .content(content.pack())
        .cluster_id(cluster_id.map(|v| v.as_bytes().pack()).pack())
        .build();
    molecule_spore_data.as_bytes().to_vec()
}

#[async_trait]
impl<T: RPC> Operation<T> for AddSporeOutputCell {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
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
        log.insert(hookkey::NEW_SPORE_ID, spore_id.as_bytes().to_vec());
        if let Some(cluster_id) = self.cluster_id {
            Box::new(AddClusterCelldepByClusterId {
                cluster_id,
                authority_mode: self.authority_mode,
            })
            .run(rpc, skeleton, log)
            .await?;
        }
        Box::new(AddSporeCelldep {}).run(rpc, skeleton, log).await
    }
}

/// Search and add cluster cell from transaction skeleton's input cells by index
///
/// # Parameters
/// - `input_index`: The index of input cell in transaction skeleton
pub struct AddClusterInputCellByClusterId {
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

#[async_trait]
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
    pub lock_script: ScriptEx,
    pub name: String,
    pub description: Vec<u8>,
}

pub fn make_cluster_data(name: &str, description: &[u8]) -> Vec<u8> {
    let molecule_cluster_data = ClusterDataV2::new_builder()
        .name(name.as_bytes().pack())
        .description(description.pack())
        .build();
    molecule_cluster_data.as_bytes().to_vec()
}

#[async_trait]
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
        log.insert(hookkey::NEW_CLUSTER_ID, cluster_id.as_bytes().to_vec());
        Box::new(AddClusterCelldep {}).run(rpc, skeleton, log).await
    }
}

/// Search spore related cells from transaction skeleton and parse the operations' intention to spore actions
///
/// note: this is essential for a historical issue of co-build project
pub struct AddSporeActions {}

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

#[async_trait]
impl<T: RPC> Operation<T> for AddSporeActions {
    async fn run(
        self: Box<Self>,
        rpc: &T,
        skeleton: &mut TransactionSkeleton,
        _: &mut Log,
    ) -> Result<()> {
        let mut spore_actions: Vec<Action> = vec![];
        // prepare spore related action parameters
        let spore_code_hash = hardcoded::spore_script(rpc.network(), vec![])
            .to_script(skeleton)?
            .code_hash()
            .unpack();
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
                let transfer_action = TransferSpore::new_builder()
                    .from(input.lock_script().into())
                    .to(output.lock_script().into())
                    .spore_id(spore_id.pack())
                    .build();
                spore_actions.push((output.type_script().unwrap(), transfer_action.into()).into());
                spore_output_cells.remove(i);
            } else {
                let burn_action = BurnSpore::new_builder()
                    .spore_id(spore_id.pack())
                    .from(input.lock_script().into())
                    .build();
                spore_actions.push((input.type_script().unwrap(), burn_action.into()).into());
            }
        }
        // handle spore mints
        for (output, spore_id) in spore_output_cells {
            let mint_action = MintSpore::new_builder()
                .spore_id(spore_id.pack())
                .to(output.lock_script().into())
                .data_hash(output.data_hash().pack())
                .build();
            spore_actions.push((output.type_script().unwrap(), mint_action.into()).into());
        }
        // prepare cluster related action parameters
        let cluster_code_hash = hardcoded::cluster_script(rpc.network(), vec![])
            .to_script(skeleton)?
            .code_hash()
            .unpack();
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
                let transfer_action = TransferCluster::new_builder()
                    .from(input.lock_script().into())
                    .to(output.lock_script().into())
                    .cluster_id(cluster_id.pack())
                    .build();
                spore_actions.push((output.type_script().unwrap(), transfer_action.into()).into());
                cluster_output_cells.remove(i);
            }
        }
        // handle cluster mints
        for (output, cluster_id) in cluster_output_cells {
            let mint_action = MintCluster::new_builder()
                .cluster_id(cluster_id.pack())
                .to(output.lock_script().into())
                .data_hash(output.data_hash().pack())
                .build();
            spore_actions.push((output.type_script().unwrap(), mint_action.into()).into());
        }
        // add spore actions into skeleton's witness field
        let witness_layout: WitnessLayout = spore_actions.into();
        skeleton.witness(WitnessEx::new_plain(witness_layout.as_slice().to_vec()));
        Ok(())
    }
}
