mod molecule;
pub use molecule::*;

// Generated types casting for code simplicity
mod casting {
    use super::molecule::Script;
    use super::*;
    use ckb_types::prelude::*;

    impl From<ckb_types::packed::Script> for Address {
        fn from(value: ckb_types::packed::Script) -> Self {
            Address::new_builder()
                .set(AddressUnion::Script(Script::new_unchecked(value.as_bytes())))
                .build()
        }
    }

    impl From<(ckb_types::packed::Script, SporeAction)> for Action {
        fn from(value: (ckb_types::packed::Script, SporeAction)) -> Self {
            let (script, spore_action) = value;
            Action::new_builder()
                .script_hash(Byte32::new_unchecked(script.calc_script_hash().as_bytes()))
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
