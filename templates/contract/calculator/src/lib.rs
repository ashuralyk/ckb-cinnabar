//! Off-chain Calculate recipes for this contract.
//!
//! Each public function returns an [`Instruction`] for one hop. This
//! contract's hops are exactly create, transfer, and burn, so each recipe is
//! tagged with the same [`intent`] constant as the Verify-tree node in
//! `contracts/{{crate_name}}`. A hop that is not that morphology uses a domain
//! string and `Instruction::new`. Compose operations to fill Inputs / Outputs /
//! CellDeps / Witnesses; callers (tests, wallets) then run them through
//! `TransactionCalculator` or `assert_verify!`.

use ckb_cinnabar_calculator::{
    address::Address,
    instruction::Instruction,
    intent,
    operation::basic::{AddInputCellByAddress, AddOutputCell},
    rpc::RPC,
};

/// Transfer CKB from `from` to `to`. This hop is exactly transfer, so the
/// instruction is named [`intent::TRANSFER`] to match the on-chain node.
///
/// `capacity` is absolute shannons. This recipe does not balance or sign;
/// tests using `FakeRpcClient` typically skip that, while live senders append
/// `BalanceTransaction` + a secp256k1 signature operation.
pub fn transfer<T: RPC>(from: Address, to: Address, capacity: u64) -> Instruction<T> {
    Instruction::named(
        intent::TRANSFER,
        vec![
            Box::new(AddInputCellByAddress { address: from }),
            Box::new(AddOutputCell {
                lock_script: to.into(),
                type_script: None,
                data: vec![],
                capacity,
                absolute_capacity: true,
                type_id: false,
            }),
        ],
    )
}
