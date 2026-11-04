use ckb_cinnabar_calculator::{
    assert_verify,
    instruction::Instruction,
    operation::basic::AddOutputCell,
    simulation::{
        always_success_script, AddFakeAlwaysSuccessCelldep, AddFakeInputCell, FakeRpcClient,
    },
};

#[tokio::test]
async fn always_success_lock_verifies() {
    let rpc = FakeRpcClient::default();
    let lock = always_success_script(vec![]);
    let prepare = Instruction::new(vec![
        Box::new(AddFakeAlwaysSuccessCelldep {}),
        Box::new(AddFakeInputCell {
            lock_script: lock.clone().into(),
            type_script: None,
            data: vec![],
            capacity: 200_000_000_000,
            absolute_capacity: true,
        }),
        Box::new(AddOutputCell {
            lock_script: lock.into(),
            type_script: None,
            data: vec![],
            capacity: 100_000_000_000,
            absolute_capacity: true,
            type_id: false,
        }),
    ]);
    let cycles = assert_verify!(&rpc, vec![prepare], 0).expect("always-success verify");
    assert!(cycles > 0);
}

#[tokio::test]
async fn intent_names_are_stable() {
    use ckb_cinnabar_calculator::intent;
    assert_eq!(intent::CREATE, "create");
    assert_eq!(intent::TRANSFER, "transfer");
    assert_eq!(intent::BURN, "burn");
    assert_eq!(intent::MINT, "mint");
    assert_eq!(intent::DEPOSIT, "deposit");
    assert_eq!(intent::WITHDRAW, "withdraw");
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn signing_works_with_data_hash_scriptid() {
    use ckb_cinnabar_calculator::{
        instruction::Instruction,
        operation::basic::{AddOutputCell, AddSecp256k1SighashSignatures},
        re_exports::ckb_hash::blake2b_256,
        simulation::{AddFakeInputCell, FakeRpcClient},
        skeleton::ScriptEx,
        types::{Hash256, ScriptHashType},
        TransactionCalculator,
    };
    use secp256k1::SecretKey;
    use std::str::FromStr;

    // Create a Data-hash lock script (hash_type = 0x00, not Type = 0x01)
    // This simulates FakeRpc / custom deployments where scripts have Data hash_type
    let privkey_bytes = [1u8; 32]; // Simple test private key
    let privkey = SecretKey::from_slice(&privkey_bytes).unwrap();
    let pubkey = secp256k1::PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), &privkey);
    let pubkey_hash = &blake2b_256(&pubkey.serialize()[..])[0..20];
    
    // Use Data hash_type (0x00) instead of official Type (0x01)
    let fake_sighash_code_hash = blake2b_256(b"fake_sighash_lock");
    let lock = ScriptEx::Script(
        Hash256::from(fake_sighash_code_hash),
        ScriptHashType::Data,
        pubkey_hash.to_vec(),
    );
    
    let rpc = FakeRpcClient::default();
    
    // Build and execute transaction with Data-hash lock
    let prepare = Instruction::new(vec![
        Box::new(AddFakeInputCell {
            lock_script: lock.clone().into(),
            type_script: None,
            data: vec![],
            capacity: 200_000_000_000,
            absolute_capacity: true,
        }),
        Box::new(AddOutputCell {
            lock_script: lock.clone(),
            type_script: None,
            data: vec![],
            capacity: 100_000_000_000,
            absolute_capacity: true,
            type_id: false,
        }),
        Box::new(AddSecp256k1SighashSignatures {
            user_lock_scripts: vec![lock],
            user_private_keys: vec![privkey],
        }),
    ]);
    
    // Execute the instruction
    let calculator = TransactionCalculator::new(vec![prepare]);
    let result = calculator.new_skeleton(&rpc);
    assert!(result.is_ok(), "execution failed: {:?}", result.err());
    
    let (skeleton, _log) = result.unwrap();
    // Verify the signature was added to the witness
    assert!(
        !skeleton.witnesses.is_empty(),
        "witnesses should not be empty after signing"
    );
    assert_eq!(
        skeleton.witnesses[0].lock.len(),
        65,
        "witness lock should be 65 bytes (secp256k1 signature), got {}",
        skeleton.witnesses[0].lock.len()
    );
}
