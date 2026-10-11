use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::{GameState, Zone};

fn expected(session: &FastActorSessionV1) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("registered input fixture must reach a decision");
    };
    expected
}
fn fixture(actor: PlayerId) -> GameState {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor, "Mountain", Zone::Hand);
    put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
    state
}

#[test]
fn ordinary_input_preserves_frozen_observation_and_complete_ordered_menu_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let old = FastActorSessionV1::from_human_v8_fixture_state(fixture(actor));
        let next = old.clone().into_heartfire_v12().unwrap();
        let e = expected(&next);
        let (old_observation, old_actions, _) = old
            .human_current_decision_input_v8(e, actor.into())
            .unwrap();
        let (observation, actions, binding) = next
            .heartfire_current_decision_input_v12(e, actor.into())
            .unwrap();
        assert_eq!(observation, old_observation);
        assert_eq!(actions, old_actions);
        assert!(binding.candidates.len() > 1);
        next.heartfire_validate_bound_decision_v12(e, actor.into(), &binding)
            .unwrap();
        let before = next.privileged_core_environment_hash();
        let mut reordered = binding.clone();
        reordered.candidates.reverse();
        assert!(next
            .heartfire_validate_bound_decision_v12(e, actor.into(), &reordered)
            .is_err());
        let mut forged_observation = binding.clone();
        forged_observation.observation.visible_projection_hash ^= 1;
        assert!(next
            .heartfire_validate_bound_decision_v12(e, actor.into(), &forged_observation)
            .is_err());
        for case in 0..8 {
            let mut stale = e;
            match case {
                0 => stale.episode_id += 1,
                1 => stale.environment_revision += 1,
                2 => stale.step += 1,
                3 => stale.physical_decision_id += 1,
                4 => stale.substep_index += 1,
                5 => stale.substep_count += 1,
                6 => stale.acting_player = actor.opponent().into(),
                7 => stale.legal_action_count += 1,
                _ => unreachable!(),
            }
            assert!(next
                .heartfire_current_decision_input_v12(stale, actor.into())
                .is_err());
        }
        assert!(next
            .heartfire_current_decision_input_v12(e, actor.opponent().into())
            .is_err());
        assert!(old
            .heartfire_current_decision_input_v12(e, actor.into())
            .is_err());
        assert_eq!(next.privileged_core_environment_hash(), before);
        // A changed stored menu must also fail, even with an unmodified binding.
        let mut corrupted = next.clone();
        corrupted.current.as_mut().unwrap().candidates.reverse();
        let corrupt_before = corrupted.privileged_core_environment_hash();
        assert!(corrupted
            .heartfire_validate_bound_decision_v12(e, actor.into(), &binding)
            .is_err());
        assert_eq!(corrupted.privileged_core_environment_hash(), corrupt_before);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn registered_partial_walker_recipient_uses_new_input_and_public_path_only_both_seats() {
    use crate::event::{self, ProposedEvent};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = fixture(actor);
        let walker = put(
            &mut state,
            actor.opponent(),
            "Ajani, Caller of the Pride",
            Zone::Hand,
        );
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(walker, Zone::Battlefield),
        );
        assert_eq!(crate::planeswalker_v1::loyalty(&state, walker), Some(4));
        let old = FastActorSessionV1::from_human_v8_fixture_state(state);
        let next = old.clone().into_heartfire_v12().unwrap();
        let e = expected(&next);
        assert!(matches!(
            old.human_current_decision_input_v8(e, actor.into()),
            Err(FlatActionDecisionSliceErrorV1::UnsupportedActionSemantic)
        ));
        assert!(old.human_raid_observation_v9(e, actor.into()).is_err());
        let (_, _, binding) = next
            .heartfire_current_decision_input_v12(e, actor.into())
            .unwrap();
        let public = next.human_heartfire_public_v12(e, actor.into()).unwrap();
        crate::heartfire_public_v1::validate(&public).unwrap();
        assert_eq!(
            public.predecessor,
            crate::policy_observation_v9::observe_policy_v9(
                &next.state,
                &next.surface,
                actor,
                e.step,
                e.physical_decision_id,
                e.substep_index,
                e.substep_count
            )
            .unwrap()
        );
        let mut changed = next.clone();
        crate::planeswalker_v1::change_loyalty(&mut changed.state, walker, -1);
        let before = serde_json::to_vec(&changed.state).unwrap();
        assert!(changed
            .heartfire_validate_bound_decision_v12(e, actor.into(), &binding)
            .is_err());
        assert_eq!(serde_json::to_vec(&changed.state).unwrap(), before);
        // Rebuilding a genuine current binding uses the actual public loyalty.
        assert!(changed
            .heartfire_current_decision_input_v12(e, actor.into())
            .is_ok());
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn missing_stale_or_hidden_recipient_ledger_refuses_without_state_mutation_both_seats() {
    use crate::event::{self, ProposedEvent};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = fixture(actor);
        let walker = put(
            &mut state,
            actor.opponent(),
            "Ajani, Caller of the Pride",
            Zone::Hand,
        );
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(walker, Zone::Battlefield),
        );
        let base = FastActorSessionV1::from_human_v8_fixture_state(state)
            .into_heartfire_v12()
            .unwrap();
        let e = expected(&base);
        for case in 0..4 {
            let mut malformed = base.clone();
            match case {
                0 => malformed.state.planeswalkers_v1 = None,
                1 => malformed.state.objects.get_mut(walker).zone_change_count += 1,
                2 => malformed.state.objects.get_mut(walker).v4.face_index = 1,
                3 => {
                    malformed.state.planeswalkers_v1 =
                        Some(crate::planeswalker_v1::PlaneswalkersV1::default())
                }
                _ => unreachable!(),
            }
            let before = serde_json::to_vec(&malformed.state).unwrap();
            assert!(malformed
                .heartfire_current_decision_input_v12(e, actor.into())
                .is_err());
            assert!(malformed
                .human_heartfire_public_v12(e, actor.into())
                .is_err());
            assert_eq!(serde_json::to_vec(&malformed.state).unwrap(), before);
            assert!(malformed.clone().into_heartfire_v12().is_err());
            assert_eq!(serde_json::to_vec(&malformed.state).unwrap(), before);
        }
    }
}
