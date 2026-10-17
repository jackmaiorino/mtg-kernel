//! Genuine paid Time Stop sampling. Initial library reordering and virtual spell
//! copying below are explicitly labelled native setup/copy primitives.
use super::*;
use crate::{
    engine::{self, time_stop_v1::tests as native, Action, Decision},
    ids::PlayerId,
    policy_observation_v6::tests::put,
    state::{StackItemKind, Target, Zone},
};
fn owner(state: GameState) -> EndTurnSessionV68 {
    EndTurnSessionV68::from_session(FastActorSessionV1::from_end_turn_v68_fixture_state(state))
        .unwrap()
}
fn discard_state(p: PlayerId, reverse_initial_hidden: bool) -> GameState {
    let mut state = native::ready(p);
    put(&mut state, p.opponent(), "Forest", Zone::Library);
    put(&mut state, p.opponent(), "Counterspell", Zone::Library);
    // Two independently initialized native worlds. No history exists yet;
    // definitions, physical IDs, RNG and current hidden pool are identical.
    assert!(!state
        .engine
        .event_history
        .has_any(&[crate::event_history_v1::EventKindV1::TimeStopJournalV1]));
    if reverse_initial_hidden {
        state.players[p.opponent().index()].library.reverse();
    }
    native::cast(&mut state, p, "Think Twice", &[], 2);
    for _ in 0..12 {
        put(&mut state, p, "Forest", Zone::Hand);
    }
    native::cast(&mut state, p, "Time Stop", &[], 6);
    for _ in 0..80 {
        let d = native::next(&mut state);
        if state.engine.pending_discard.is_some() {
            break;
        }
        native::pass(&mut state, d);
    }
    assert_eq!(state.engine.pending_discard.as_ref().unwrap().count, 5);
    state
}
fn packet(session: EndTurnSessionV68) -> crate::current_mixed_policy_v1::Packet {
    let model = crate::current_mixed_policy_v1::end_turn::EndTurnSession::from_end_turn(session);
    let FastActorResponseV1::Decision(expected) = model.current_response() else {
        panic!("paid decision absent")
    };
    model.encode(expected).unwrap().packet().clone()
}
#[test]
fn genuine_paid_sampling_preserves_archives_discard_restore_and_second_stop_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let original = owner(discard_state(p, false));
        let d = original.decision_v68().unwrap();
        assert!(crate::engine::time_stop_v1::sampler_conflicts_v68(
            &original.inner.state,
            &original.inner.state.players[p.index()].hand
        )
        .unwrap());
        assert!(!crate::engine::time_stop_v1::sampler_conflicts_v68(
            &original.inner.state,
            &original.inner.state.players[p.opponent().index()].library
        )
        .unwrap());
        let original_bytes = serde_json::to_vec(&original.inner.state).unwrap();
        let mut sampled = original.sampled_clone_v68(&d, 8172).unwrap();
        let mut replay = original.sampled_clone_v68(&d, 8172).unwrap();
        assert_eq!(sampled.inner.state, replay.inner.state);
        assert_eq!(
            sampled.inner.state.engine.event_history,
            original.inner.state.engine.event_history
        );
        assert_eq!(
            sampled.inner.state.players[p.index()].hand,
            original.inner.state.players[p.index()].hand
        );
        assert_ne!(
            sampled.inner.state.players[p.opponent().index()].library,
            original.inner.state.players[p.opponent().index()].library
        );
        assert_eq!(
            serde_json::to_vec(&original.inner.state).unwrap(),
            original_bytes
        );
        assert!(sampled.validate_decision_v68(&d).is_err());
        let first = sampled.decision_v68().unwrap();
        assert_eq!(first.candidates_v68(), d.candidates_v68());
        let snapshot = sampled.snapshot_v68();
        let discard = first
            .actions()
            .iter()
            .position(|a| matches!(a, ActionSemanticV1::Discard { .. }))
            .unwrap() as u32;
        sampled.step_v68(&first, discard).unwrap();
        sampled.restore_v68(&snapshot).unwrap();
        assert!(sampled.validate_decision_v68(&first).is_err());
        assert_eq!(sampled.inner.state, replay.inner.state);
        for _ in 0..8 {
            if sampled.inner.state.engine.pending_discard.is_none() {
                break;
            }
            let a = sampled.decision_v68().unwrap();
            let b = replay.decision_v68().unwrap();
            let index = a
                .actions()
                .iter()
                .position(|a| matches!(a, ActionSemanticV1::Discard { .. }))
                .unwrap() as u32;
            assert_eq!(a.candidates_v68(), b.candidates_v68());
            assert_eq!(
                sampled.step_v68(&a, index).unwrap(),
                replay.step_v68(&b, index).unwrap()
            );
            assert_eq!(sampled.inner.state, replay.inner.state);
        }
        assert!(sampled.inner.state.engine.pending_discard.is_none());
        let mut second = sampled.inner.state.clone();
        native::end_turn(&mut second);
        native::cast(&mut second, p, "Time Stop", &[], 6);
        native::end_turn(&mut second);
        assert_eq!(
            crate::time_stop_public_v68::capture(&second, p)
                .unwrap()
                .disposals
                .len(),
            2
        );
        let second = owner(second);
        let current = second.decision_v68().unwrap();
        let again = second.sampled_clone_v68(&current, 9381).unwrap();
        assert_eq!(
            again.decision_v68().unwrap().candidates_v68(),
            current.candidates_v68()
        );
    }
}
#[test]
fn independently_paid_private_archives_do_not_filter_placement_menu_tensor_or_key_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let a = owner(discard_state(p, false));
        let b = owner(discard_state(p, true));
        assert_ne!(
            a.inner.state.engine.event_history,
            b.inner.state.engine.event_history
        );
        let da = a.decision_v68().unwrap();
        let db = b.decision_v68().unwrap();
        assert_eq!(da.candidates_v68(), db.candidates_v68());
        let sa = a.sampled_clone_v68(&da, 8172).unwrap();
        let sb = b.sampled_clone_v68(&db, 8172).unwrap();
        for player in [p, p.opponent()] {
            assert_eq!(
                sa.inner.state.players[player.index()].hand,
                sb.inner.state.players[player.index()].hand
            );
            assert_eq!(
                sa.inner.state.players[player.index()].library,
                sb.inner.state.players[player.index()].library
            );
        }
        assert_eq!(
            sa.decision_v68().unwrap().observation().projection_sha256,
            sb.decision_v68().unwrap().observation().projection_sha256
        );
        assert_eq!(
            sa.decision_v68().unwrap().candidates_v68(),
            sb.decision_v68().unwrap().candidates_v68()
        );
        assert_eq!(packet(sa), packet(sb));
    }
}
#[test]
fn genuine_paid_mixed_physical_ability_virtual_disposal_sampling_and_tamper_refusal_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut state = native::ready(p);
        let target = put(
            &mut state,
            p.opponent(),
            "Axgard Cavalry",
            Zone::Battlefield,
        );
        let source = native::cast(&mut state, p, "Heartfire Immolator", &[], 2);
        native::finish(&mut state);
        native::priority(&mut state, p);
        let before_taps = native::tapped(&state, p);
        assert!(
            matches!(native::next(&mut state),Decision::CastSpellOrPass{activatable_abilities,..} if activatable_abilities.contains(&(source,0)))
        );
        engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
        assert!(
            matches!(native::next(&mut state),Decision::ChooseTargets{legal_targets,..} if legal_targets.contains(&Target::Object(target)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        assert!(
            matches!(native::next(&mut state),Decision::CastSpellOrPass{player,..} if player==p)
        );
        assert!(state.engine.pending_activation.is_none());
        assert_eq!(native::tapped(&state, p) - before_taps, 1);
        assert_eq!(state.players[p.index()].mana_pool, [0; 6]);
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        let ability = state
            .stack
            .iter()
            .find(|r| r.source == source && r.kind == StackItemKind::ActivatedAbility)
            .unwrap()
            .clone();
        native::cast(&mut state, p, "Think Twice", &[], 2);
        let stop = native::cast(&mut state, p, "Time Stop", &[], 6);
        // Existing genuine native spell-copy primitive, with real ancestry.
        let (copy, _) = engine::copy_spell_for(&mut state, stop, p).unwrap();
        assert!(state.objects.get(copy).spell_copy_origin.is_some());
        native::end_turn(&mut state);
        assert!(crate::engine::time_stop_v1::disposed(&state, &ability).unwrap());
        assert!(crate::engine::time_stop_v1::sampler_conflicts_v68(&state, &[copy]).unwrap());
        let original = owner(state);
        let current = original.decision_v68().unwrap();
        let sampled = original.sampled_clone_v68(&current, 8172).unwrap();
        assert_eq!(
            sampled.inner.state.engine.event_history,
            original.inner.state.engine.event_history
        );
        assert_eq!(
            sampled.decision_v68().unwrap().candidates_v68(),
            current.candidates_v68()
        );
        let restored = native::restore(&sampled.inner.state);
        crate::source_power_activation_v1::validate_state(&restored).unwrap();
        for log in [false, true] {
            let mut bad = restored.clone();
            let journal = bad
                .engine
                .event_history
                .iter()
                .find(|e| matches!(e, crate::event::CommittedEvent::TimeStopJournalV1 { .. }))
                .unwrap()
                .clone();
            if log {
                bad.engine.event_log.push(journal);
            } else {
                bad.engine.event_history.push(journal);
            }
            let before = serde_json::to_vec(&bad).unwrap();
            assert!(
                super::super::flat_action_v4::redeterminize_end_turn_v68(&mut bad, p, 8172)
                    .is_err()
            );
            assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
        }
    }
}
