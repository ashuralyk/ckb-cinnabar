use ckb_sdk::{Address, HumanCapacity};
use secp256k1::SecretKey;

use crate::{
    instruction::DefaultInstruction,
    operation::{
        AddInputCellByAddress, AddOutputCell, AddSecp256k1SighashCellDep,
        AddSecp256k1SighashSignatures, BalanceTransaction,
    },
};

/// Transfer CKB from one address to another
///
/// # Parameters
/// - `from`: The address to transfer CKB from
/// - `to`: The address to transfer CKB to
/// - `ckb`: The amount of CKB to transfer, e.g. "100.5 CKB"
/// - `additional_fee_rate`: The additional fee rate to add
/// - `sign`: The private key to sign the transaction, if not provided, transaction won't balance and sign
pub fn secp256k1_sighash_transfer(
    from: &Address,
    to: &Address,
    ckb: HumanCapacity,
    additional_fee_rate: u64,
    sign: Option<SecretKey>,
) -> DefaultInstruction {
    let mut transfer = DefaultInstruction::new(vec![
        Box::new(AddSecp256k1SighashCellDep {}),
        Box::new(AddInputCellByAddress {
            address: from.clone(),
        }),
        Box::new(AddOutputCell {
            lock_script: to.payload().into(),
            type_script: None,
            data: Vec::new(),
            capacity: ckb.into(),
            use_additional_capacity: false,
            use_type_id: false,
        }),
    ]);
    if let Some(privkey) = sign {
        transfer.merge(balance_and_sign(from, privkey, additional_fee_rate));
    }
    transfer
}

/// Balance transaction with capacity and then sign it
///
/// # Parameters
/// - `signer`: The address who is supposed to provide capacity to balance, in the meantime, receive the change
/// - `privkey`: The private key to sign the transaction
/// - `additional_fee_rate`: The additional fee rate to add
pub fn balance_and_sign(
    signer: &Address,
    privkey: SecretKey,
    additional_fee_rate: u64,
) -> DefaultInstruction {
    DefaultInstruction::new(vec![
        Box::new(BalanceTransaction {
            balancer: signer.payload().into(),
            change_receiver: signer.clone().into(),
            additional_fee_rate,
        }),
        Box::new(AddSecp256k1SighashSignatures {
            user_lock_scripts: vec![signer.payload().into()],
            user_private_keys: vec![privkey],
        }),
    ])
}

// TODO: Add more predefined instructions here, e.g. xUDT and Spore
