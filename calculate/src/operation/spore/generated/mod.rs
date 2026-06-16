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

    impl From<(Script, SporeAction)> for Action {
        fn from(value: (Script, SporeAction)) -> Self {
            let (script, spore_action) = value;
            Action::new_builder()
                .script_hash(script.calc_script_hash())
                .data(spore_action.as_slice().to_vec())
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
