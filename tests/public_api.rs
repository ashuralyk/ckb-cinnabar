use ckb_cinnabar::{
    intent, Address, CalculatorError, Instruction, Network, RpcClient, TransactionCalculator,
    TransactionSkeleton,
};

#[test]
fn root_reexports_agent_facing_api() {
    let _: Option<Address> = None;
    let _: Option<Instruction<RpcClient>> = None;
    let _: Option<TransactionCalculator<RpcClient>> = None;
    let _: Option<TransactionSkeleton> = None;
    let _: Option<CalculatorError> = None;
    let _: Option<Network> = None;
    assert_eq!(intent::TRANSFER, "transfer");
}
