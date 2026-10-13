use super::*;
use crate::policy_observation_v6::tests::{put, ready_state, ready_state_for};

#[test]
#[cfg(not(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
)))]
fn foreign_restored_capture_refuses_readonly_before_begin_index_both_seats() {
    use crate::event::CommittedEvent;
    for actor in [PlayerId::P0, PlayerId::P1] {
        for begin in [0, u64::MAX] {
            let mut state = ready_state();
            let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let capture = crate::cerberus_capture_v1::CerberusCaptureV1 {
                history_index: 0,
                begin_history_index: begin,
                observer_authority_index: 0,
                source: crate::state::AbilitySourceContractV4::capture(&state, source),
            };
            state
                .engine
                .event_history
                .push(CommittedEvent::CerberusCaptureV1 { capture });
            let replay: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            let before = serde_json::to_vec(&replay).unwrap();
            assert!(crate::cerberus_capture_v1::validate_state(&replay).is_err());
            assert!(observe_v1(&replay, &PolicySurfaceV5::new(), actor, 0, 0, 0, 1).is_err());
            assert_eq!(serde_json::to_vec(&replay).unwrap(), before);
        }
    }
}

#[test]
fn ordinary_predecessor_parts_hidden_identity_and_artifact_identity_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        put(&mut state, actor, "Forest", Zone::Hand);
        let hidden = put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
        let surface = PolicySurfaceV5::new();
        let old = crate::rl::observe_policy_v8(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        let public = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        public.validate_artifact().unwrap();
        assert_eq!(public.projection, old.projection);
        assert_eq!(public.extensions, old.extensions);
        assert!(public.triggers.is_empty());
        assert!(public.untap_restrictions.is_empty());
        let definition = crate::card_def::card_id_by_name("Island").unwrap();
        let object = state.objects.get_mut(hidden);
        object.card_def = definition;
        object.name = "Island".into();
        object.v4 = crate::state::ObjectStateV4::from_card_def(definition);
        assert_eq!(
            observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap(),
            public
        );
        let mut bad = public.clone();
        bad.schema.push('x');
        assert!(bad.validate_artifact().is_err());
        let mut bad = public;
        bad.producer_sha256.clear();
        assert!(bad.validate_artifact().is_err());
    }
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn printed_capture_completion_source_departure_and_legacy_refusal_restore_both_seats() {
    use crate::cerberus_capture_v1::tests::{actual_trigger, resolve_actual};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = actual_trigger(actor);
        let surface = PolicySurfaceV5::new();
        let public = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        assert_eq!(public.triggers.len(), 1);
        assert_eq!(public.untap_restrictions.len(), 1);
        assert_eq!(public.triggers[0].source_at_trigger.arena_id, source.0);
        assert_eq!(public.triggers[0].captured_controller, actor.into());
        assert!(public.triggers[0].morbid_at_capture);
        assert_eq!(public.triggers[0].phase, "published");
        assert!(crate::rl::observe_policy_v8(&state, &surface, actor, 0, 0, 0, 1).is_err());
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::zone_change(source, Zone::Hand),
        );
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        state.objects.get_mut(source).tapped = true;
        let old_capture = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        assert!(!old_capture.triggers[0].original_source_present);
        assert_ne!(
            old_capture.triggers[0].source_at_trigger.zone_change_count,
            state.objects.get(source).zone_change_count
        );
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        resolve_actual(&mut state);
        resolve_actual(&mut replay);
        let completed = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        assert_eq!(completed.triggers[0].phase, "completed");
        assert!(matches!(
            completed.triggers[0].outcome,
            Some(OutcomeV1::Resolved {
                before_tapped: None,
                after_tapped: None,
                ..
            })
        ));
        assert!(state.objects.get(source).tapped);
        assert_eq!(
            completed,
            observe_v1(&replay, &surface, actor, 0, 0, 0, 1).unwrap()
        );
        state.engine.event_history.retain(|event| {
            !matches!(
                event,
                crate::event::CommittedEvent::CerberusOperationV1 { .. }
            )
        });
        let before = serde_json::to_vec(&state).unwrap();
        assert!(observe_v1(&state, &surface, actor, 0, 0, 0, 1).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
}
