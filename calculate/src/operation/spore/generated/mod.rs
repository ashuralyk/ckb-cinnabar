mod molecule;
pub use molecule::*;

// Generated types casting for code simplicity
mod casting {
    use super::*;
    use ckb_types::{packed::Script, prelude::*};

    impl From<Script> for Address {
        fn from(value: Script) -> Self {
            Address::new_builder()
                .set(AddressUnion::Script(value))
                .build()
        }
    }

    impl From<TransferSpore> for SporeAction {
        fn from(value: TransferSpore) -> Self {
            SporeAction::new_builder()
                .set(SporeActionUnion::TransferSpore(value))
                .build()
        }
    }

    impl From<MintSpore> for SporeAction {
        fn from(value: MintSpore) -> Self {
            SporeAction::new_builder()
                .set(SporeActionUnion::MintSpore(value))
                .build()
        }
    }

    impl From<BurnSpore> for SporeAction {
        fn from(value: BurnSpore) -> Self {
            SporeAction::new_builder()
                .set(SporeActionUnion::BurnSpore(value))
                .build()
        }
    }

    impl From<MintCluster> for SporeAction {
        fn from(value: MintCluster) -> Self {
            SporeAction::new_builder()
                .set(SporeActionUnion::MintCluster(value))
                .build()
        }
    }

    impl From<TransferCluster> for SporeAction {
        fn from(value: TransferCluster) -> Self {
            SporeAction::new_builder()
                .set(SporeActionUnion::TransferCluster(value))
                .build()
        }
    }

    impl From<(Script, SporeAction)> for Action {
        fn from(value: (Script, SporeAction)) -> Self {
            let (script, spore_action) = value;
            Action::new_builder()
                .script_hash(script.calc_script_hash())
                .data(spore_action.as_slice().pack())
                .build()
        }
    }

    impl From<Vec<Action>> for WitnessLayout {
        fn from(value: Vec<Action>) -> Self {
            let actions = ActionVec::new_builder().set(value).build();
            let message = Message::new_builder().actions(actions).build();
            let sighash_all = SighashAll::new_builder().message(message).build();
            WitnessLayout::new_builder()
                .set(WitnessLayoutUnion::SighashAll(sighash_all))
                .build()
        }
    }
}
