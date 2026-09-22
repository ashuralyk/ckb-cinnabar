//! Type-burn / lock-proxy output and indexer collect for the kernel.
//!
//! See <https://github.com/ckb-ecofund/ckb-proxy-locks>.

use alloc::{boxed::Box, string::ToString, vec::Vec};
use core::fmt;

use crate::kernel::{
    error::{CalculatorError, Result},
    indexer::{CellQueryOptions, GetCellsIter, SearchKey, SearchMode},
    network::Network,
    operation::{
        basic::{AddCellDep, AddOutputCell},
        Log, Operation,
    },
    rpc::RPC,
    skeleton::{CellInputEx, ScriptEx, TransactionSkeleton},
    source::Source,
    types::{DepType, Hash256},
};

/// Always-success lock code hash (`Data1`).
pub const ALWAYS_SUCCESS_CODE_HASH: Hash256 = [
    0x3b, 0x52, 0x1c, 0xc4, 0xb5, 0x52, 0xf1, 0x09, 0xd0, 0x92, 0xd8, 0xcc, 0x46, 0x8a, 0x80, 0x48,
    0xac, 0xb5, 0x3c, 0x59, 0x52, 0xdb, 0xe7, 0x69, 0xd2, 0xb2, 0xf9, 0xcf, 0x6e, 0x47, 0xf7, 0xf1,
];
/// Input-type proxy code hash (`Data1`).
pub const INPUT_TYPE_PROXY_CODE_HASH: Hash256 = [
    0x51, 0x23, 0x90, 0x89, 0x65, 0xc7, 0x11, 0xb0, 0xff, 0xd8, 0xae, 0xc6, 0x42, 0xf1, 0xed, 0xe3,
    0x29, 0x64, 0x9b, 0xda, 0x1e, 0xbd, 0xca, 0x6b, 0xd2, 0x41, 0x24, 0xd3, 0x79, 0x6f, 0x76, 0x8a,
];
/// Output-type proxy code hash (`Data1`).
pub const OUTPUT_TYPE_PROXY_CODE_HASH: Hash256 = [
    0x2d, 0xf5, 0x3b, 0x59, 0x2d, 0xb3, 0xae, 0x36, 0x85, 0xb7, 0x78, 0x7a, 0xdc, 0xfe, 0xf0, 0x33,
    0x2a, 0x61, 0x1e, 0xdb, 0x83, 0xca, 0x3f, 0xec, 0xa4, 0x35, 0x80, 0x99, 0x64, 0xc3, 0xaf, 0xf2,
];
/// Lock-proxy code hash (`Data1`).
pub const LOCK_PROXY_CODE_HASH: Hash256 = [
    0x2d, 0xf5, 0x3b, 0x59, 0x2d, 0xb3, 0xae, 0x36, 0x85, 0xb7, 0x78, 0x7a, 0xdc, 0xfe, 0xf0, 0x33,
    0x2a, 0x61, 0x1e, 0xdb, 0x83, 0xca, 0x3f, 0xec, 0xa4, 0x35, 0x80, 0x99, 0x64, 0xc3, 0xaf, 0xf2,
];
/// Single-use lock code hash (`Data1`).
pub const SINGLE_USE_CODE_HASH: Hash256 = [
    0x82, 0x90, 0x46, 0x7a, 0x51, 0x2e, 0x5b, 0x9a, 0x6b, 0x81, 0x64, 0x69, 0xb0, 0xed, 0xab, 0xba,
    0x1f, 0x4a, 0xc4, 0x74, 0xe2, 0x8f, 0xfd, 0xd6, 0x04, 0xc2, 0xa7, 0xc7, 0x64, 0x46, 0xbb, 0xaf,
];
/// Type-burn lock code hash (`Data1`).
pub const TYPE_BURN_CODE_HASH: Hash256 = [
    0xff, 0x78, 0xba, 0xe0, 0xab, 0xf1, 0x7d, 0x7a, 0x40, 0x4c, 0x0b, 0xe0, 0xf9, 0xad, 0x9c, 0x91,
    0x85, 0xb3, 0xf8, 0x8d, 0xcc, 0x60, 0x40, 0x34, 0x53, 0xd5, 0xba, 0x8e, 0x1f, 0x22, 0xf5, 0x3a,
];

/// Proxy-locks bundle tx hash on mainnet.
pub const COMPONENT_MAINNET_TX_HASH: Hash256 = [
    0x10, 0xd6, 0x3a, 0x99, 0x61, 0x57, 0xd3, 0x2c, 0x01, 0x07, 0x80, 0x58, 0x00, 0x00, 0x52, 0x67,
    0x4c, 0xa5, 0x8d, 0x15, 0xf9, 0x21, 0xbe, 0xc7, 0xf1, 0xdc, 0xda, 0xc2, 0x16, 0x0e, 0xb6, 0x6b,
];

/// Proxy-locks bundle tx hash on testnet.
pub const COMPONENT_TESTNET_TX_HASH: Hash256 = [
    0xb4, 0xf1, 0x71, 0xc9, 0xc9, 0xca, 0xf7, 0x40, 0x1f, 0x54, 0xa8, 0xe5, 0x62, 0x25, 0xae, 0x21,
    0xd9, 0x50, 0x32, 0x15, 0x0a, 0x87, 0xa4, 0x67, 0x8e, 0xac, 0x3f, 0x66, 0xa3, 0x13, 0x7b, 0x93,
];

/// Sentinel out-point hash used only on fake / custom networks.
pub const COMPONENT_FAKENET_TX_HASH: Hash256 = [0xc0; 32];

/// Deployment tx hash of the proxy-locks bundle for `network`.
pub fn component_tx_hash(network: &Network) -> Hash256 {
    match network {
        Network::Mainnet => COMPONENT_MAINNET_TX_HASH,
        Network::Testnet => COMPONENT_TESTNET_TX_HASH,
        _ => COMPONENT_FAKENET_TX_HASH,
    }
}

/// Output index of each component script inside the proxy-locks dep transaction.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Name {
    /// Always-success lock (index 0).
    AlwaysSuccess = 0,
    /// Input-type proxy (index 1).
    InputTypeProxy,
    /// Output-type proxy (index 2).
    OutputTypeProxy,
    /// Lock proxy (index 3).
    LockProxy,
    /// Single-use lock (index 4).
    SingleUse,
    /// Type-burn lock (index 5).
    TypeBurn,
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Name::AlwaysSuccess => f.write_str("always_success"),
            Name::InputTypeProxy => f.write_str("input_type_proxy"),
            Name::OutputTypeProxy => f.write_str("output_type_proxy"),
            Name::LockProxy => f.write_str("lock_proxy"),
            Name::SingleUse => f.write_str("single_use"),
            Name::TypeBurn => f.write_str("type_burn"),
        }
    }
}

fn component_code_hash(name: Name) -> Hash256 {
    match name {
        Name::AlwaysSuccess => ALWAYS_SUCCESS_CODE_HASH,
        Name::InputTypeProxy => INPUT_TYPE_PROXY_CODE_HASH,
        Name::OutputTypeProxy => OUTPUT_TYPE_PROXY_CODE_HASH,
        Name::LockProxy => LOCK_PROXY_CODE_HASH,
        Name::SingleUse => SINGLE_USE_CODE_HASH,
        Name::TypeBurn => TYPE_BURN_CODE_HASH,
    }
}

/// Component script for `network`. Fake / custom use a [`ScriptEx::Reference`]
/// resolved from a named cell dep.
pub fn component_script(network: &Network, name: Name, args: &[u8]) -> ScriptEx {
    match network {
        Network::Mainnet | Network::Testnet => {
            ScriptEx::new_code(component_code_hash(name), args.to_vec())
        }
        _ => (name.to_string(), args.to_vec()).into(),
    }
}

/// Add a type-burn-lock output whose args are the type hash of another output.
pub struct AddTypeBurnOutputCell {
    /// Output whose type hash becomes the type-burn lock args.
    pub output_index: usize,
    /// Optional type script of the new cell.
    pub type_script: Option<ScriptEx>,
    /// Cell data.
    pub data: Vec<u8>,
    /// Network used to pick the type-burn lock (canonical hash vs named dep).
    pub network: Network,
}

/// Add a lock-proxy cell whose args are `lock_hash`.
pub struct AddLockProxyOutputCell {
    /// Hash of the lock being proxied.
    pub lock_hash: Hash256,
    /// `true`: proxy is the lock script; `false`: proxy is the type script
    /// (then `second_script` is required as the actual lock).
    pub lock_script: bool,
    /// The other script on the cell (type when `lock_script`, lock otherwise).
    pub second_script: Option<ScriptEx>,
    /// Cell data.
    pub data: Vec<u8>,
    /// Network used to pick the lock-proxy script (canonical hash vs named dep).
    pub network: Network,
}

impl<S: Source> Operation<S> for AddTypeBurnOutputCell {
    fn run(
        self: Box<Self>,
        source: &S,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let reference_type_hash = skeleton
            .get_output_by_index(self.output_index)?
            .calc_type_hash()
            .ok_or_else(|| CalculatorError::Other("reference output has no type script".into()))?;
        Box::new(AddOutputCell {
            lock_script: component_script(
                &self.network,
                Name::TypeBurn,
                reference_type_hash.as_slice(),
            ),
            type_script: self.type_script,
            capacity: 0,
            data: self.data,
            absolute_capacity: false,
            type_id: false,
        })
        .run(source, skeleton, log)
    }
}

impl<S: Source> Operation<S> for AddLockProxyOutputCell {
    fn run(
        self: Box<Self>,
        source: &S,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        let proxy = component_script(&self.network, Name::LockProxy, self.lock_hash.as_slice());
        if self.lock_script {
            Box::new(AddOutputCell {
                lock_script: proxy,
                type_script: self.second_script,
                capacity: 0,
                data: self.data,
                absolute_capacity: false,
                type_id: false,
            })
            .run(source, skeleton, log)
        } else {
            Box::new(AddOutputCell {
                lock_script: self
                    .second_script
                    .ok_or_else(|| CalculatorError::Other("missing second script".into()))?,
                type_script: Some(proxy),
                capacity: 0,
                data: self.data,
                absolute_capacity: false,
                type_id: false,
            })
            .run(source, skeleton, log)
        }
    }
}

/// Add the `ckb-proxy-locks` cell dep for `name` (index = `name as u32`).
pub struct AddComponentCelldep {
    /// Which component script inside the proxy-locks transaction.
    pub name: Name,
}

impl<C: RPC> Operation<C> for AddComponentCelldep {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        Box::new(AddCellDep {
            name: self.name.to_string(),
            tx_hash: component_tx_hash(&rpc.network()),
            index: self.name as u32,
            dep_type: DepType::Code,
            with_data: false,
        })
        .run(rpc, skeleton, log)
    }
}

/// Search and add type-burn-lock input cells whose args equal `type_hash`.
pub struct AddTypeBurnInputCell {
    /// Type-script hash encoded as the type-burn lock args.
    pub type_hash: Hash256,
    /// Maximum number of matching cells to consume.
    pub count: usize,
}

impl AddTypeBurnInputCell {
    /// Indexer search key: type-burn lock args = `type_hash`.
    pub fn search_key(
        &self,
        network: &Network,
        skeleton: &TransactionSkeleton,
    ) -> Result<SearchKey> {
        let type_burn = component_script(network, Name::TypeBurn, self.type_hash.as_slice());
        let mut query = CellQueryOptions::new_lock(type_burn.to_script(skeleton)?);
        query.with_data = Some(true);
        Ok(query.into())
    }
}

impl<C: RPC> Operation<C> for AddTypeBurnInputCell {
    fn run(
        mut self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        Box::new(AddComponentCelldep {
            name: Name::TypeBurn,
        })
        .run(rpc, skeleton, log)?;
        let search_key = self.search_key(&rpc.network(), skeleton)?;
        let mut iter = GetCellsIter::new(rpc, search_key);
        while let Some(cell) = iter.next()? {
            skeleton
                .input(CellInputEx::new_from_live_cell(cell, None))?
                .witness(Default::default());
            self.count -= 1;
            if self.count == 0 {
                break;
            }
        }
        Ok(())
    }
}

/// Consume a type-burn-lock cell whose args are the type hash of `skeleton.inputs[input_index]`.
pub struct AddTypeBurnInputCellByInputIndex {
    /// Index of the input whose type hash is searched; `usize::MAX` = last input.
    pub input_index: usize,
}

impl<C: RPC> Operation<C> for AddTypeBurnInputCellByInputIndex {
    fn run(
        self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        Box::new(AddComponentCelldep {
            name: Name::TypeBurn,
        })
        .run(rpc, skeleton, log)?;
        let type_hash = skeleton
            .get_input_by_index(self.input_index)?
            .output
            .calc_type_hash()
            .ok_or_else(|| CalculatorError::Other("input cell has no type script".into()))?;
        Box::new(AddTypeBurnInputCell {
            type_hash,
            count: 1,
        })
        .run(rpc, skeleton, log)
    }
}

/// Search and consume lock-proxy cells whose args equal `lock_hash`.
pub struct AddLockProxyInputCell {
    /// Hash of the lock being proxied.
    pub lock_hash: Hash256,
    /// `true` search as lock script; `false` search as type script.
    pub lock_script: bool,
    /// Maximum number of matching cells to consume.
    pub count: usize,
}

impl AddLockProxyInputCell {
    /// Indexer search key: lock-proxy args = `lock_hash`, as lock or type.
    pub fn search_key(
        &self,
        network: &Network,
        skeleton: &TransactionSkeleton,
    ) -> Result<SearchKey> {
        let proxy = component_script(network, Name::LockProxy, self.lock_hash.as_slice());
        let mut query = if self.lock_script {
            CellQueryOptions::new_lock(proxy.to_script(skeleton)?)
        } else {
            CellQueryOptions::new_type(proxy.to_script(skeleton)?)
        };
        query.with_data = Some(true);
        query.script_search_mode = Some(SearchMode::Exact);
        Ok(query.into())
    }
}

impl<C: RPC> Operation<C> for AddLockProxyInputCell {
    fn run(
        mut self: Box<Self>,
        rpc: &C,
        skeleton: &mut TransactionSkeleton,
        log: &mut Log,
    ) -> Result<()> {
        Box::new(AddComponentCelldep {
            name: Name::LockProxy,
        })
        .run(rpc, skeleton, log)?;
        let search_key = self.search_key(&rpc.network(), skeleton)?;
        let mut iter = GetCellsIter::new(rpc, search_key);
        while let Some(cell) = iter.next()? {
            skeleton
                .input(CellInputEx::new_from_live_cell(cell, None))?
                .witness(Default::default());
            self.count -= 1;
            if self.count == 0 {
                break;
            }
        }
        Ok(())
    }
}
