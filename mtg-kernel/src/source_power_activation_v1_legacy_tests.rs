//! Actual predecessor activation checks the new guard's unrelated-item path.
use super::*;
use crate::card_def::card_id_by_name;
use crate::engine::{Action, Decision};
use crate::ids::PlayerId;
use std::hash::{Hash, Hasher};

#[test]
fn registered_ordinary_activation_keeps_legacy_receipt_absence_and_pass_replay() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let dragon = card_id_by_name("Shivan Dragon").unwrap();
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[dragon; 40],
            &[dragon; 40],
            |id| CARD_DEFS[usize::from(id)].object_name.into(),
            49_095,
            actor,
        );
        let source = state.players[actor.index()].library[0];
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        state.step = crate::state::Step::Main1;
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 2;
        crate::engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
        assert!(matches!(
            crate::engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        assert_eq!(state.stack.len(), 1);
        assert!(!is_program(state.stack[0].inline_effect.as_ref()));
        let bytes = serde_json::to_vec(&state).unwrap();
        let mut before = std::collections::hash_map::DefaultHasher::new();
        state.hash(&mut before);
        validate_state(&state).unwrap();
        let mut after = std::collections::hash_map::DefaultHasher::new();
        state.hash(&mut after);
        assert_eq!(before.finish(), after.finish());
        assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
        let mut replay: GameState = serde_json::from_slice(&bytes).unwrap();
        crate::engine::step(&mut state, Action::Pass).unwrap();
        crate::engine::step(&mut replay, Action::Pass).unwrap();
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        assert!(!state.engine.event_history.iter().any(|event| matches!(
            event,
            CommittedEvent::SourcePowerActivationCommittedV1 { .. }
        )));
    }
}
