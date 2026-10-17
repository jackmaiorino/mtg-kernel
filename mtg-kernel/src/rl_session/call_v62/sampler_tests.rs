//! Paid Call decisions with a native, later-hidden Alesha departure source.
use super::*;
use crate::{
    call_observation_v62::tests::ready,
    event::{self, CommittedEvent, ProposedEvent},
    ids::PlayerId,
    policy_observation_v6::tests::put,
    rl::ActionSemanticV1,
    state::Zone,
};

fn paid_archived_source(actor: PlayerId, reverse_private_library: bool) -> GameState {
    let mut state = ready(actor, "Forest");
    // The opponent's paid draw records a different private physical card in
    // the two worlds. Their current unknown slot set remains identical.
    put(&mut state, actor.opponent(), "Island", Zone::Library);
    if reverse_private_library {
        state.players[actor.opponent().index()].library.reverse();
    }
    crate::graveyard_return_public_v61::tests::paid(
        &mut state,
        actor.opponent(),
        "Think Twice",
        None,
    );
    let source = crate::graveyard_return_public_v61::tests::paid(
        &mut state,
        actor,
        "Alesha, Who Laughs at Fate",
        None,
    );
    crate::graveyard_return_public_v61::tests::paid(&mut state, actor, "Snap", Some(source));
    assert_eq!(state.objects.get(source).zone, Zone::Hand);
    assert!(state.engine.event_history.iter().any(
        |row| matches!(row, CommittedEvent::AleshaPowerLkiV1 { source: id, .. } if *id == source)
    ));
    // Labelled native zone-change setup after the genuine paid cast/bounce.
    // The original departure receipt stays immutable while this incarnation
    // becomes an eligible, unknown library slot.
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(source, Zone::Library),
    );
    assert!(crate::trigger::collect_and_process(&mut state).is_empty());
    assert!(state.engine.event_log.is_empty());
    assert_eq!(state.objects.get(source).zone, Zone::Library);
    assert!(state.players[actor.index()].library.contains(&source));
    crate::call_observation_v62::validate_state(&state).unwrap();
    state
}

fn owner(state: GameState) -> CallSessionV62 {
    CallSessionV62::from_session(FastActorSessionV1::from_call_v62_fixture_state(state)).unwrap()
}

#[test]
fn paid_archived_alesha_current_slot_sampling_is_history_independent_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let a = owner(paid_archived_source(actor, false));
        let b = owner(paid_archived_source(actor, true));
        let da = a.decision_v62().unwrap();
        let db = b.decision_v62().unwrap();
        assert_ne!(
            a.inner.state.engine.event_history,
            b.inner.state.engine.event_history
        );
        assert_eq!(da.candidates_v62(), db.candidates_v62());
        let before = serde_json::to_vec(&a.inner.state).unwrap();
        let mut sampled = a.sampled_clone_v62(&da, 8172).unwrap();
        let replay = a.sampled_clone_v62(&da, 8172).unwrap();
        let independent = b.sampled_clone_v62(&db, 8172).unwrap();
        assert_eq!(sampled.inner.state, replay.inner.state);
        assert_eq!(
            sampled.inner.state.engine.event_history,
            a.inner.state.engine.event_history
        );
        assert_eq!(serde_json::to_vec(&a.inner.state).unwrap(), before);
        assert_ne!(
            sampled.inner.state.players[actor.index()].library,
            a.inner.state.players[actor.index()].library
        );
        for player in [actor, actor.opponent()] {
            assert_eq!(
                sampled.inner.state.players[player.index()].hand,
                independent.inner.state.players[player.index()].hand
            );
            assert_eq!(
                sampled.inner.state.players[player.index()].library,
                independent.inner.state.players[player.index()].library
            );
        }
        assert!(sampled.validate_decision_v62(&da).is_err());
        let current = sampled.decision_v62().unwrap();
        assert_eq!(current.candidates_v62(), da.candidates_v62());
        let snapshot = sampled.snapshot_v62();
        let pass = current
            .actions()
            .iter()
            .position(|action| matches!(action, ActionSemanticV1::Pass { .. }))
            .unwrap() as u32;
        sampled.step_v62(&current, pass).unwrap();
        sampled.restore_v62(&snapshot).unwrap();
        assert!(sampled.validate_decision_v62(&current).is_err());
        assert_eq!(sampled.inner.state, replay.inner.state);
    }
}

#[test]
fn paid_archived_alesha_suffix_forgery_refuses_readonly_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let original = paid_archived_source(actor, false);
        let receipt = original
            .engine
            .event_history
            .iter()
            .find(|row| matches!(row, CommittedEvent::AleshaPowerLkiV1 { .. }))
            .unwrap()
            .clone();
        for forged in [receipt.clone(), {
            let mut shifted = receipt;
            if let CommittedEvent::AleshaPowerLkiV1 {
                departure_history_index,
                zone_change_count,
                ..
            } = &mut shifted
            {
                *departure_history_index += 1;
                *zone_change_count += 1;
            }
            shifted
        }] {
            let mut state = original.clone();
            state.engine.event_history.push(forged);
            let before = state.clone();
            assert!(crate::call_observation_v62::validate_state(&state).is_err());
            assert!(
                super::super::flat_action_v4::redeterminize_call_v62(&mut state, actor, 8172)
                    .is_err()
            );
            assert_eq!(state, before);
        }
    }
}

#[test]
fn paid_archived_alesha_remains_sampleable_in_validated_end_turn_scope_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let state = paid_archived_source(actor, false);
        let owner = super::super::end_turn_v68::EndTurnSessionV68::from_session(
            FastActorSessionV1::from_end_turn_v68_fixture_state(state),
        )
        .unwrap();
        let decision = owner.decision_v68().unwrap();
        let sampled = owner.sampled_clone_v68(&decision, 8172).unwrap();
        assert_eq!(
            sampled.decision_v68().unwrap().candidates_v68(),
            decision.candidates_v68()
        );
        assert!(sampled.validate_decision_v68(&decision).is_err());
    }
}
