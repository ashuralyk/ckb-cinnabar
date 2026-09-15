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
