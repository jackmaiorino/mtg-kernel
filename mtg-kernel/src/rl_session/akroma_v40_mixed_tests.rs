//! Real mixed-family V40 tests. Printed Akroma cases await its genuine registration.
use super::*;
use crate::{
    engine::{self, Action, Decision},
    ids::{ObjectId, PlayerId},
    mana::ManaColor,
    policy_observation_v6::tests::{put, ready_state_for},
    state::{GameState, Zone},
};
fn decision(state: &mut GameState) -> Decision {
    engine::advance_until_decision(state)
}
fn cast_target(state: &mut GameState, actor: PlayerId, card: ObjectId, target: ObjectId) {
    assert_eq!(state.priority_player, actor);
    engine::step(state, Action::CastSpell(card)).unwrap();
    assert!(matches!(decision(state), Decision::ChooseTargets { .. }));
    engine::step(
        state,
        Action::ChooseTarget(crate::state::Target::Object(target)),
    )
    .unwrap();
    assert!(matches!(decision(state), Decision::CastSpellOrPass { .. }));
    assert!(state.engine.pending_cast.is_none());
    assert!(state
        .stack
        .iter()
        .any(|item| item.source == card && item.controller == actor));
}
fn resolve(state: &mut GameState, card: ObjectId) {
    for _ in 0..40 {
        let next = decision(state);
        assert!(matches!(next, Decision::CastSpellOrPass { .. }), "{next:?}");
        if !state.stack.iter().any(|item| item.source == card)
            && state.engine.pending_cast.is_none()
        {
            return;
        }
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("real spell did not finish");
}
fn setup(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
    let mut state = ready_state_for(actor);
    for player in [actor, actor.opponent()] {
        put(&mut state, player, "Lightning Bolt", Zone::Hand);
    }
    let target = put(
        &mut state,
        actor.opponent(),
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let spell = put(&mut state, actor, "Involuntary Employment", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
    (state, spell, target)
}
fn input(
    state: GameState,
) -> (
    FastActorSessionV1,
    FastActorDecisionV1,
    AkromaDecisionBindingV40,
) {
    let session = FastActorSessionV1::from_akroma_v40_fixture_state(state);
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("genuine held decision");
    };
    let (_, _, binding) = session
        .akroma_current_decision_input_v40(expected, expected.acting_player)
        .unwrap();
    (session, expected, binding)
}
#[test]
fn registered_employment_real_held_and_resolved_binding_forgery_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, spell, target) = setup(actor);
        cast_target(&mut state, actor, spell, target);
        let held = state.clone();
        let (session, expected, binding) = input(held.clone());
        assert!(binding.observation.families.iter().any(|fact| matches!(
            fact,
            crate::akroma_family_public_v40::FamilyFactsV40::Employment(_)
        )));
        let mut forged = binding.clone();
        forged.families.reverse();
        let before = session.privileged_core_environment_hash();
        assert!(session
            .akroma_validate_bound_decision_v40(expected, expected.acting_player, &forged)
            .is_err());
        assert_eq!(session.privileged_core_environment_hash(), before);
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&held).unwrap()).unwrap();
        assert_eq!(input(restored).2.observation, binding.observation);
        resolve(&mut state, spell);
        assert_eq!(state.objects.get(target).controller, actor);
        let (session, expected, binding) = input(state.clone());
        let installation = binding
            .observation
            .families
            .iter()
            .find_map(|fact| match fact {
                crate::akroma_family_public_v40::FamilyFactsV40::Employment(o) => {
                    o.installations.first()
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(installation.original_controller, actor.opponent().into());
        assert_eq!(installation.captured_controller, actor.into());
        assert_eq!(
            installation.current_recipient.as_ref().unwrap().controller,
            actor.into()
        );
        let mut forged = binding.clone();
        for fact in &mut forged.observation.families {
            if let crate::akroma_family_public_v40::FamilyFactsV40::Employment(o) = fact {
                o.installations[0].captured_controller = actor.opponent().into();
            }
        }
        let mut value = serde_json::to_value(&forged.observation).unwrap();
        value.as_object_mut().unwrap().remove("projection_sha256");
        use sha2::Digest;
        forged.observation.projection_sha256 =
            sha2::Sha256::digest(serde_json::to_vec(&value).unwrap())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
        forged.observation.validate_artifact().unwrap();
        let before = session.privileged_core_environment_hash();
        assert!(session
            .akroma_validate_bound_decision_v40(expected, expected.acting_player, &forged)
            .is_err());
        assert_eq!(session.privileged_core_environment_hash(), before);
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(input(restored).2.observation, binding.observation);
    }
}
#[test]
fn registered_partial_ajani_recipient_full_loyalty_binding_remains_scoped_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        let walker = put(
            &mut state,
            actor.opponent(),
            "Ajani, Caller of the Pride",
            Zone::Battlefield,
        );
        assert_eq!(crate::planeswalker_v1::loyalty(&state, walker), Some(4));
        let (mut session, expected, binding) = input(state);
        assert_eq!(
            binding
                .observation
                .projection
                .surface
                .engine_context
                .planeswalkers
                .as_ref()
                .unwrap()
                .len(),
            1
        );
        crate::planeswalker_v1::change_loyalty(session.akroma_fixture_state_mut(), walker, 1);
        let before = session.privileged_core_environment_hash();
        assert!(session
            .akroma_validate_bound_decision_v40(expected, expected.acting_player, &binding)
            .is_err());
        assert_eq!(session.privileged_core_environment_hash(), before);
        assert_eq!(
            crate::planeswalker_v1::remove_loyalty_row_for_test_v1(
                session.akroma_fixture_state_mut(),
                walker
            ),
            Some(5)
        );
        let before = session.privileged_core_environment_hash();
        assert!(session
            .akroma_current_decision_input_v40(expected, expected.acting_player)
            .is_err());
        assert_eq!(session.privileged_core_environment_hash(), before);
    }
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn actual_memorial_and_employment_control_grants_full_union_and_legacy_refusal_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, spell, target) = setup(actor);
        let memorial = put(&mut state, actor, "Akroma's Memorial", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 7;
        engine::step(&mut state, Action::CastSpell(memorial)).unwrap();
        resolve(&mut state, memorial);
        assert_eq!(state.objects.get(memorial).zone, Zone::Battlefield);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        cast_target(&mut state, actor, spell, target);
        let (session, expected, held) = input(state.clone());
        assert!(held.observation.grants[0]
            .recipients
            .iter()
            .all(|row| row.arena_id != target.0));
        assert_eq!(expected.acting_player, actor.into());
        assert!(crate::rl::observe_policy_v8(
            &state,
            &crate::policy_surface_v5::PolicySurfaceV5::new_for_session(),
            actor,
            0,
            0,
            0,
            1
        )
        .is_err());
        assert!(flat_validate_expected_decision_v1(
            &session,
            session.current.as_ref().unwrap(),
            expected
        )
        .is_err());
        resolve(&mut state, spell);
        let (_, _, binding) = input(state.clone());
        let recipient = binding.observation.grants[0]
            .recipients
            .iter()
            .find(|row| row.arena_id == target.0)
            .unwrap();
        assert_eq!(recipient.controller, actor.into());
        assert_eq!(binding.observation.grants[0].keyword_mask, 61);
        assert_eq!(binding.observation.grants[0].protection_color_mask, 12);
        assert!(binding.observation.families.iter().any(|row|matches!(row,crate::akroma_family_public_v40::FamilyFactsV40::Employment(o) if o.installations.len()==1)));
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(input(restored).2.observation, binding.observation);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn actual_employment_twinflame_full_family_and_ordered_owner_binding_both_seats() {
    use crate::akroma_family_public_v40::{FamilyFactsV40, FamilyProjectionV40};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let state =
            crate::twinflame_observation_v1::tests::at_scalar_with_employment(actor, true, true);
        let bytes = serde_json::to_vec(&state).unwrap();
        let (session, expected, binding) = input(state.clone());
        assert_eq!(expected.acting_player, actor.opponent().into());
        assert!(binding.observation.families.iter().any(|row| matches!(row, FamilyFactsV40::Employment(o) if o.installations.len()==1 && o.installations[0].captured_controller == actor.into())));
        let twin = binding
            .observation
            .families
            .iter()
            .find_map(|row| match row {
                FamilyFactsV40::Twinflame(o) => Some(o),
                _ => None,
            })
            .unwrap();
        assert_eq!(twin.multiplier_sources.len(), 1);
        assert_eq!(twin.prevention_menu.as_ref().unwrap().remaining_damage, 3);
        assert!(twin
            .prevention_menu
            .as_ref()
            .unwrap()
            .options
            .iter()
            .any(|o| o.kind == "multiplier"));
        assert!(binding
            .observation
            .families
            .iter()
            .any(|row| matches!(row, FamilyFactsV40::Crystal(o) if o.prevention_menu.is_none())));
        for mutation in 0..3 {
            let mut forged = binding.clone();
            match mutation {
                0 => {
                    let part = forged
                        .families
                        .iter_mut()
                        .find_map(|row| match row {
                            FamilyProjectionV40::Twinflame(o) => Some(o),
                            _ => None,
                        })
                        .unwrap();
                    part.prevention_menu.as_mut().unwrap().options.reverse();
                }
                1 => {
                    let part = forged
                        .observation
                        .families
                        .iter_mut()
                        .find_map(|row| match row {
                            FamilyFactsV40::Twinflame(o) => Some(o),
                            _ => None,
                        })
                        .unwrap();
                    part.prevention_menu.as_mut().unwrap().remaining_damage = 6;
                }
                _ => forged.candidates.reverse(),
            }
            let before = session.privileged_core_environment_hash();
            assert!(session
                .akroma_validate_bound_decision_v40(expected, expected.acting_player, &forged)
                .is_err());
            assert_eq!(session.privileged_core_environment_hash(), before);
        }
        let restored: GameState = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(input(restored).2.observation, binding.observation);
        assert_eq!(
            serde_json::to_vec(session.akroma_fixture_state()).unwrap(),
            bytes
        );
    }
}
