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

#[test]
fn closed_source_power_proof_rejects_live_state_tamper_after_cached_cursor() {
    use crate::event_history_v1::{proof_tests_v1, ProvedV1};
    use crate::policy_observation_v6::tests::{put, ready_state_for};

    let mut state = ready_state_for(PlayerId::P0);
    let immolator = put(
        &mut state,
        PlayerId::P0,
        "Heartfire Immolator",
        Zone::Battlefield,
    );
    let target = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    state.players[PlayerId::P0.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 1;
    assert!(matches!(
        crate::engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    crate::engine::step(&mut state, Action::ActivateAbility(immolator, 0)).unwrap();
    assert!(matches!(
        crate::engine::advance_until_decision(&mut state),
        Decision::ChooseTargets { .. }
    ));
    crate::engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    let _ = crate::engine::advance_until_decision(&mut state);
    let item = state
        .stack
        .iter()
        .find(|row| row.source == immolator)
        .unwrap()
        .clone();
    crate::engine::step(&mut state, Action::Pass).unwrap();
    crate::engine::step(&mut state, Action::Pass).unwrap();
    let _ = crate::engine::advance_until_decision(&mut state);
    assert!(state.stack.is_empty());

    validate_state(&state).unwrap();
    let initial_len = state.engine.event_history.len();
    assert_eq!(
        state
            .engine
            .event_history
            .proof_memo_v1::<SourcePowerProofV1>(ProvedV1::SourcePowerActivation)
            .map(|(covered, _)| covered),
        Some(initial_len)
    );

    for _ in 0..70u32 {
        state
            .engine
            .event_history
            .push(CommittedEvent::UpkeepBegan {
                player: PlayerId::P0,
            });
    }
    let uses_before = proof_tests_v1::uses();
    validate_state(&state).unwrap();
    assert_eq!(proof_tests_v1::uses(), uses_before + 1);

    // (a) Reintroduce finished Heartfire activation onto live stack.
    let mut reintroduced = state.clone();
    reintroduced.stack.push(item.clone());
    let uses_before = proof_tests_v1::uses();
    assert!(validate_state(&reintroduced).is_err());
    assert_eq!(proof_tests_v1::uses(), uses_before + 1);

    // (b) Strip inline_effect on a live selected activation so program tamper is caught.
    let mut stripped = state.clone();
    let mut forged = item.clone();
    forged.v4.stack_item_id = StackItemId(999_999);
    forged.inline_effect = None;
    stripped.stack.push(forged);
    let uses_before = proof_tests_v1::uses();
    assert!(validate_state(&stripped).is_err());
    assert_eq!(proof_tests_v1::uses(), uses_before + 1);

    // (c) Inject finished receipt into live event_log.
    let finished_event = state
        .engine
        .event_history
        .iter()
        .find(|e| matches!(e, CommittedEvent::SourcePowerActivationFinishedV1 { .. }))
        .unwrap()
        .clone();
    let mut forged_log = state.clone();
    forged_log.engine.event_log.push(finished_event);
    let uses_before = proof_tests_v1::uses();
    assert!(validate_state(&forged_log).is_err());
    assert_eq!(proof_tests_v1::uses(), uses_before + 1);
}
