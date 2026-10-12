use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::Zone;

fn basic(actor: PlayerId, hidden: &str) -> crate::state::GameState {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    state
}
fn expected(session: &HumanArahboSessionV31) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(decision) = session.current_response() else {
        panic!("real decision");
    };
    decision
}
fn action(prompt: &HumanDecisionArahboV31, prefix: &str) -> HumanActionRequestV1 {
    let row = prompt
        .actions
        .iter()
        .find(|row| row.label.starts_with(prefix))
        .unwrap();
    HumanActionRequestV1 {
        prompt_seq: prompt.prompt_seq,
        action_index: row.action_index,
    }
}
fn safe(value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, child) in fields {
                assert!(!matches!(
                    key.as_str(),
                    "arena_id"
                        | "zone_change_count"
                        | "event_history"
                        | "event_index"
                        | "card_db_id"
                        | "binding"
                        | "engine_indexes"
                        | "projection_sha256"
                        | "producer_sha256"
                ));
                safe(child);
            }
        }
        serde_json::Value::Array(rows) => {
            for row in rows {
                safe(row);
            }
        }
        _ => {}
    }
}
#[test]
fn registered_ordinary_predecessor_menu_privacy_submit_and_retry_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let base = FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Lightning Bolt"));
        let old = HumanRaidSessionV9::from_session(base.clone()).unwrap();
        let mut session = HumanArahboSessionV31::from_session(base).unwrap();
        let current = expected(&session);
        let prior = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&old, current)
            .unwrap();
        let mut projector = HumanArahboProjectorV31::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.actions, prior.actions);
        assert_eq!(prompt.state.public, prior.state.public);
        assert_eq!(prompt.state.extensions.predecessor, prior.state.extensions);
        assert!(prompt.state.extensions.entry_occurrences.is_empty());
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let hidden = HumanArahboSessionV31::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            HumanArahboProjectorV31::new(actor.into())
                .project_current(&hidden, expected(&hidden))
                .unwrap(),
            prompt
        );
        let request = action(&prompt, "Play Forest");
        let receipt = projector.submit(&mut session, request.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(projector.submit(&mut session, request).unwrap(), receipt);
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
    }
}
#[test]
fn registered_owning_restore_foreign_owner_generation_and_old_future_requests_refuse_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut session = HumanArahboSessionV31::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Lightning Bolt")),
        )
        .unwrap();
        let mut projector = HumanArahboProjectorV31::new(actor.into());
        let first = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let snapshot = session.snapshot_v31();
        let paid = action(&first, "Play Forest");
        projector.submit(&mut session, paid.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_request = action(&future, "Pass priority");
        session.restore_v31(&snapshot).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, paid),
            Err(Error::StaleDecision)
        );
        assert_eq!(
            projector.submit(&mut session, future_request),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let refreshed = projector
            .project_current(&session, expected(&session))
            .unwrap();
        assert!(refreshed.prompt_seq > future.prompt_seq);
        let foreign = HumanArahboSessionV31::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            session.restore_v31(&foreign.snapshot_v31()),
            Err(Error::StaleDecision)
        );
        session.arahbo.generation = u64::MAX;
        assert_eq!(
            session.restore_v31(&snapshot),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
    }
}

#[test]
fn registered_real_ward_payment_preserves_predecessor_public_menu_and_fields() {
    let (mut state, _, _) = crate::policy_observation_v6::tests::ward_multi_targeter_state();
    crate::policy_observation_v6::tests::reach_ward_payment(&mut state);
    let base = FastActorSessionV1::from_human_v8_fixture_state(state);
    let old = HumanRaidSessionV9::from_session(base.clone()).unwrap();
    let session = HumanArahboSessionV31::from_session(base).unwrap();
    let current = expected(&session);
    let actor = current.acting_player;
    let old = HumanDecisionProjectorV9::new(actor)
        .project_current(&old, current)
        .unwrap();
    let prompt = HumanArahboProjectorV31::new(actor)
        .project_current(&session, current)
        .unwrap();
    assert_eq!(prompt.actions, old.actions);
    assert_eq!(prompt.state.public, old.state.public);
    assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
    assert_eq!(prompt.state.restricted_mana, old.state.restricted_mana);
    assert_eq!(
        prompt.state.known_face_down_cards,
        old.state.known_face_down_cards
    );
    safe(&serde_json::to_value(prompt).unwrap());
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
#[ignore = "requires genuine Arahbo registration; owning transport is source-only"]
fn actual_human_cast_created_token_anthem_controller_restore_and_coherent_binding_both_seats() {
    use crate::engine;
    use crate::event::{CommittedEvent, ProposedEvent};
    use crate::mana::ManaColor;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::arahbo_v1::future_tests::ready(actor);
        let source = put(&mut state, actor, "Arahbo, the First Fang", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
        let mut session = HumanArahboSessionV31::from_session(
            FastActorSessionV1::from_arahbo_v31_fixture_state(state),
        )
        .unwrap();
        let mut projector = HumanArahboProjectorV31::new(actor.into());
        let initial = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let start = action(&initial, "Begin casting Arahbo, the First Fang");
        let snapshot = session.snapshot_v31();
        let paid = projector.submit(&mut session, start.clone()).unwrap();
        assert_eq!(projector.submit(&mut session, start.clone()).unwrap(), paid);
        for _ in 0..30 {
            let state = session.inner.arahbo_fixture_state_mut();
            if state.objects.get(source).zone == Zone::Battlefield && state.stack.is_empty() {
                break;
            }
            let current = expected(&session);
            let mut owning = HumanArahboProjectorV31::new(current.acting_player);
            let prompt = owning.project_current(&session, current).unwrap();
            safe(&serde_json::to_value(&prompt).unwrap());
            owning
                .submit(&mut session, action(&prompt, "Pass priority"))
                .unwrap();
        }
        let current = expected(&session);
        let facts = session.public_observation(current).unwrap();
        assert_eq!(facts.entry_occurrences.len(), 1);
        assert_eq!(
            facts.entry_occurrences[0].lifecycle,
            crate::arahbo_public_v1::EntryLifecycleV1::TokenCreated
        );
        let token = facts.entry_occurrences[0].created_token.as_ref().unwrap();
        let state = session.inner.arahbo_fixture_state_mut();
        assert_eq!(
            engine::effective_power(state, crate::ids::ObjectId(token.arena_id)),
            2
        );
        assert_eq!(
            engine::effective_toughness(state, crate::ids::ObjectId(token.arena_id)),
            2
        );
        assert!(crate::policy_observation_v9::observe_policy_v9(
            state,
            &crate::policy_surface_v5::PolicySurfaceV5::new(),
            actor,
            0,
            0,
            0,
            1
        )
        .is_err());
        let before = session.inner.privileged_core_environment_hash();
        let future = projector.project_current(&session, current).unwrap();
        let old_future = action(&future, "Pass priority");
        session.restore_v31(&snapshot).unwrap();
        let restored = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, start),
            Err(Error::StaleDecision)
        );
        assert_eq!(
            projector.submit(&mut session, old_future),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), restored);
        assert_ne!(before, restored);
        // Genuine published entry receipt and exact private rederivation.
        let mut state = crate::arahbo_v1::future_tests::ready(actor);
        crate::arahbo_v1::future_tests::cast(&mut state, actor);
        let cat = put(&mut state, actor, "Savannah Lions", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(cat, Zone::Battlefield),
        );
        crate::arahbo_v1::future_tests::published(&mut state);
        let mut session = HumanArahboSessionV31::from_session(
            FastActorSessionV1::from_arahbo_v31_fixture_state(state),
        )
        .unwrap();
        let current = expected(&session);
        let mut owning = HumanArahboProjectorV31::new(current.acting_player);
        let prompt = owning.project_current(&session, current).unwrap();
        safe(&serde_json::to_value(&prompt).unwrap());
        let request = action(&prompt, "Pass priority");
        let state = session.inner.arahbo_fixture_state_mut();
        let capture = state
            .engine
            .event_history
            .iter_mut()
            .rev()
            .find_map(|event| match event {
                CommittedEvent::ArahboTokenCaptureV1 { capture } => Some(capture),
                _ => None,
            })
            .unwrap();
        capture.source.controller = actor.opponent();
        let before = session.inner.privileged_core_environment_hash();
        assert!(owning.submit(&mut session, request).is_err());
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
    }
}
