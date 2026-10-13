//! Owning mixed-family consumer traversals. No synthetic root or predecessor observation.
use super::*;
use crate::{
    engine::{self, Action, Decision},
    ids::{ObjectId, PlayerId},
    mana::ManaColor,
    policy_observation_v6::tests::{put, ready_state_for},
    state::{GameState, Zone},
};
fn held(actor: PlayerId, memorial: bool) -> (GameState, ObjectId, ObjectId) {
    let mut state = ready_state_for(actor);
    put(&mut state, actor, "Lightning Bolt", Zone::Hand);
    put(&mut state, actor.opponent(), "Forest", Zone::Hand);
    let target = put(
        &mut state,
        actor.opponent(),
        "Llanowar Elves",
        Zone::Battlefield,
    );
    if memorial {
        let source = put(&mut state, actor, "Akroma's Memorial", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 7;
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        resolve(&mut state, source);
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    }
    let spell = put(&mut state, actor, "Involuntary Employment", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    engine::step(&mut state, Action::CastSpell(spell)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseTargets { .. }
    ));
    engine::step(
        &mut state,
        Action::ChooseTarget(crate::target::Target::Object(target)),
    )
    .unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(state.engine.pending_cast.is_none());
    assert!(state
        .stack
        .iter()
        .any(|item| item.source == spell && item.controller == actor));
    (state, spell, target)
}
fn resolve(state: &mut GameState, source: ObjectId) {
    for _ in 0..40 {
        let next = engine::advance_until_decision(state);
        assert!(matches!(next, Decision::CastSpellOrPass { .. }), "{next:?}");
        if state.engine.pending_cast.is_none()
            && !state.stack.iter().any(|item| item.source == source)
        {
            return;
        }
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("actual resolution did not finish");
}
fn owning(state: GameState, actor: PlayerId) -> HumanAkromaSessionV40 {
    HumanAkromaSessionV40::from_session(
        FastActorSessionV1::from_akroma_v40_fixture_state(state),
        actor.into(),
    )
    .unwrap()
}
fn check(memorial: bool) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, spell, target) = held(actor, memorial);
        let mut session = owning(state.clone(), actor);
        let snapshot = session.snapshot_v40();
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("actual held priority");
        };
        let mut projector = HumanAkromaProjectorV40::new(actor.into());
        let menu = projector.project_current(&session, expected).unwrap();
        assert_eq!(menu.state.extensions.grants.len(), usize::from(memorial));
        assert!(menu.state.extensions.families.iter().any(|row| matches!(
            row,
            super::super::akroma_families_v40::HumanFamilyV40::Employment(_)
        )));
        let json = serde_json::to_string(&menu).unwrap();
        for hidden in [
            "private_snapshot_json",
            "source_contract",
            "event_history",
            "arena_id",
            "Forest",
        ] {
            assert!(!json.contains(hidden), "{hidden}");
        }
        let pass = menu
            .actions
            .iter()
            .find(|row| row.label.to_lowercase().contains("pass"))
            .unwrap();
        let request = HumanActionRequestV1 {
            prompt_seq: menu.prompt_seq,
            action_index: pass.action_index,
        };
        let receipt = projector.submit(&mut session, request.clone()).unwrap();
        assert_eq!(
            projector.submit(&mut session, request.clone()).unwrap(),
            receipt
        );
        session.restore_v40(&snapshot).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, request),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("restored held priority");
        };
        let replay = projector.project_current(&session, expected).unwrap();
        assert_eq!(replay.state, menu.state);
        assert_eq!(replay.actions, menu.actions);
        resolve(&mut state, spell);
        assert_eq!(state.objects.get(target).controller, actor);
        let session = owning(state, actor);
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("actual resolved priority");
        };
        let menu = HumanAkromaProjectorV40::new(actor.into())
            .project_current(&session, expected)
            .unwrap();
        let installation = menu
            .state
            .extensions
            .families
            .iter()
            .find_map(|row| match row {
                super::super::akroma_families_v40::HumanFamilyV40::Employment(o) => {
                    o.installations.first()
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(installation.original_controller, actor.opponent().into());
        assert_eq!(installation.captured_controller, actor.into());
        if memorial {
            let grant = &menu.state.extensions.grants[0];
            assert_eq!(grant.keyword_mask, 61);
            assert_eq!(grant.protection_color_mask, 12);
            assert!(grant
                .recipients
                .iter()
                .any(|recipient| Some(recipient) == installation.current_recipient.as_ref()));
        }
    }
}
#[test]
fn registered_employment_held_resolved_safe_facts_retry_and_rollback_both_seats() {
    check(false);
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn actual_akroma_employment_union_canonical_control_facts_and_owning_replay_both_seats() {
    check(true);
}

fn current_v40(session: &HumanAkromaSessionV40) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("genuine V40 decision: {:?}", session.current_response());
    };
    expected
}
fn choose_v40(prompt: &HumanDecisionAkromaV40, text: &str) -> HumanActionRequestV1 {
    let row = prompt
        .actions
        .iter()
        .find(|row| row.label.contains(text))
        .unwrap_or_else(|| panic!("missing {text}: {:?}", prompt.actions));
    HumanActionRequestV1 {
        prompt_seq: prompt.prompt_seq,
        action_index: row.action_index,
    }
}
fn twin_v40(
    prompt: &HumanDecisionAkromaV40,
) -> &super::super::twinflame_v38::HumanTwinflameExtensionsV38 {
    prompt
        .state
        .extensions
        .families
        .iter()
        .find_map(|row| match row {
            super::super::akroma_families_v40::HumanFamilyV40::Twinflame(o) => Some(o.as_ref()),
            _ => None,
        })
        .unwrap()
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn actual_employment_twinflame_union_order_privacy_retry_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let state =
            crate::twinflame_observation_v1::tests::at_scalar_with_employment(actor, true, true);
        let original = serde_json::to_vec(&state).unwrap();
        let mut session = owning(state, actor.opponent());
        let mut projector = HumanAkromaProjectorV40::new(actor.opponent().into());
        let prompt = projector
            .project_current(&session, current_v40(&session))
            .unwrap();
        assert_eq!(twin_v40(&prompt).multiplier_sources.len(), 1);
        assert_eq!(
            twin_v40(&prompt)
                .prevention_menu
                .as_ref()
                .unwrap()
                .remaining_damage,
            3
        );
        assert!(prompt.state.extensions.families.iter().any(|row| matches!(row, super::super::akroma_families_v40::HumanFamilyV40::Employment(o) if o.installations.len() == 1 && o.installations[0].captured_controller == actor.into())));
        let json = serde_json::to_string(&prompt).unwrap();
        for hidden in [
            "source_contract",
            "event_history",
            "arena_id",
            "zone_change_count",
            "card_db_id",
            "engine_indexes",
            "producer_sha256",
        ] {
            assert!(!json.contains(hidden), "{hidden}");
        }
        let saved = session.snapshot_v40();
        let doubled = choose_v40(&prompt, "double the remaining damage");
        let receipt = projector.submit(&mut session, doubled.clone()).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, doubled.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let after = projector
            .project_current(&session, current_v40(&session))
            .unwrap();
        assert_eq!(
            twin_v40(&after)
                .prevention_menu
                .as_ref()
                .unwrap()
                .remaining_damage,
            6
        );
        projector
            .submit(&mut session, choose_v40(&after, "shield"))
            .unwrap();
        assert_eq!(
            session.inner.akroma_fixture_state().players[actor.opponent().index()].life,
            17
        );
        session.restore_v40(&saved).unwrap();
        assert_eq!(
            serde_json::to_vec(session.inner.akroma_fixture_state()).unwrap(),
            original
        );
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, doubled),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let replay = projector
            .project_current(&session, current_v40(&session))
            .unwrap();
        assert_eq!(replay.state, prompt.state);
        assert_eq!(replay.actions, prompt.actions);
        projector
            .submit(&mut session, choose_v40(&replay, "shield"))
            .unwrap();
        assert_eq!(
            session.inner.akroma_fixture_state().players[actor.opponent().index()].life,
            20
        );
        assert!(session
            .inner
            .akroma_fixture_state()
            .engine
            .pending_effect
            .is_none());
    }
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn actual_twinflame_shared_combat_allocation_in_union_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let state = crate::twinflame_observation_v1::tests::at_combat(actor);
        let lives = state.players.each_ref().map(|p| p.life);
        let mut session = owning(state, actor.opponent());
        let mut projector = HumanAkromaProjectorV40::new(actor.opponent().into());
        let prompt = projector
            .project_current(&session, current_v40(&session))
            .unwrap();
        let allocation = twin_v40(&prompt).combat_allocation.as_ref().unwrap();
        assert_eq!(allocation.packets.len(), 2);
        let saved = session.snapshot_v40();
        projector
            .submit(&mut session, choose_v40(&prompt, "3 combat damage"))
            .unwrap();
        let menu = projector
            .project_current(&session, current_v40(&session))
            .unwrap();
        assert!(twin_v40(&menu).prevention_menu.as_ref().unwrap().is_combat);
        let doubled = choose_v40(&menu, "double the remaining damage");
        projector.submit(&mut session, doubled.clone()).unwrap();
        let finished = session.inner.akroma_fixture_state();
        assert!(!crate::twinflame_tyrant_v1::combat::has_pending(finished));
        assert_eq!(
            finished.players[actor.opponent().index()].life,
            lives[actor.opponent().index()] - 13
        );
        assert_eq!(
            finished.players[actor.index()].life,
            lives[actor.index()] + 13
        );
        session.restore_v40(&saved).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, doubled),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let prompt = projector
            .project_current(&session, current_v40(&session))
            .unwrap();
        projector
            .submit(&mut session, choose_v40(&prompt, "3 combat damage"))
            .unwrap();
        let menu = projector
            .project_current(&session, current_v40(&session))
            .unwrap();
        projector
            .submit(&mut session, choose_v40(&menu, "shield"))
            .unwrap();
        let finished = session.inner.akroma_fixture_state();
        assert!(!crate::twinflame_tyrant_v1::combat::has_pending(finished));
        assert_eq!(
            finished.players[actor.opponent().index()].life,
            lives[actor.opponent().index()] - 10
        );
        assert_eq!(
            finished.players[actor.index()].life,
            lives[actor.index()] + 10
        );
    }
}
