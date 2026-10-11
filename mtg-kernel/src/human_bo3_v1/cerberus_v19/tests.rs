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
#[track_caller]
fn expected(session: &HumanCerberusSessionV19) -> FastActorDecisionV1 {
    let response = session.current_response();
    let FastActorResponseV1::Decision(decision) = response else {
        panic!("real decision, observed {response:?}");
    };
    decision
}
fn action(prompt: &HumanDecisionCerberusV19, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanCerberusSessionV19::from_session(base).unwrap();
        let current = expected(&session);
        let prior = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&old, current)
            .unwrap();
        let mut projector = HumanCerberusProjectorV19::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.actions, prior.actions);
        assert_eq!(prompt.state.public, prior.state.public);
        assert_eq!(prompt.state.extensions.predecessor, prior.state.extensions);
        assert!(prompt.state.extensions.triggers.is_empty());
        assert!(prompt.state.extensions.untap_restrictions.is_empty());
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let hidden = HumanCerberusSessionV19::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            HumanCerberusProjectorV19::new(actor.into())
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
        let mut session = HumanCerberusSessionV19::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Lightning Bolt")),
        )
        .unwrap();
        let mut projector = HumanCerberusProjectorV19::new(actor.into());
        let first = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let snapshot = session.snapshot_v19();
        let paid = action(&first, "Play Forest");
        projector.submit(&mut session, paid.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_request = action(&future, "Pass priority");
        session.restore_v19(&snapshot).unwrap();
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
        let foreign = HumanCerberusSessionV19::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            session.restore_v19(&foreign.snapshot_v19()),
            Err(Error::StaleDecision)
        );
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v19(&snapshot),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
    }
}

// Future actual printed groups. These require the genuine Cerberus definition;
// they are compiled source preparation, not registered gameplay qualification.

fn ward_state(actor: PlayerId) -> crate::state::GameState {
    use crate::engine::{self, Action, Decision};
    use crate::mana::ManaColor;
    use crate::state::Target;
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    let terror = put(
        &mut state,
        actor.opponent(),
        "Tolarian Terror",
        Zone::Battlefield,
    );
    let spells =
        ["Lightning Bolt", "Lightning Bolt"].map(|name| put(&mut state, actor, name, Zone::Hand));
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 8;
    for spell in spells {
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        engine::step(&mut state, Action::ChooseTarget(Target::Object(terror))).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
    }
    state
}
#[test]
fn inherited_registered_generic_ward_menu_is_preserved_and_submission_replays() {
    for (actor, pay) in [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|actor| [false, true].map(|pay| (actor, pay)))
    {
        let mut state = ward_state(actor);
        crate::policy_observation_v6::tests::reach_ward_payment(&mut state);
        let base = FastActorSessionV1::from_human_v8_fixture_state(state);
        let previous = HumanRaidSessionV9::from_session(base.clone()).unwrap();
        let mut session = HumanCerberusSessionV19::from_session(base).unwrap();
        let e = expected(&session);
        let old = HumanDecisionProjectorV9::new(e.acting_player)
            .project_current(&previous, e)
            .unwrap();
        let mut projector = HumanCerberusProjectorV19::new(e.acting_player);
        let prompt = projector.project_current(&session, e).unwrap();
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert_eq!(prompt.actions, old.actions);
        safe(&serde_json::to_value(&prompt).unwrap());
        let prefix = if pay { "Pay" } else { "Decline to pay" };
        let input = action(&prompt, prefix);
        let snapshot = session.snapshot_v19();
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v19(&snapshot).unwrap();
        let mut replay = HumanCerberusProjectorV19::new(e.acting_player);
        assert_eq!(
            replay
                .project_current(&session, expected(&session))
                .unwrap(),
            prompt
        );
        replay.submit(&mut session, input).unwrap();
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn actual_printed_trigger_public_submission_restore_retry_and_completed_capture_both_seats() {
    use crate::cerberus_capture_v1::tests::actual_trigger;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = actual_trigger(actor);
        // Public submission advances through the End step after resolution.
        // Keep both seats alive across Draw so the completed trigger can be
        // inspected and restored at the next real decision.
        for seat in [PlayerId::P0, PlayerId::P1] {
            for _ in 0..8 {
                put(&mut state, seat, "Forest", Zone::Library);
            }
        }
        // Inspect the actual End-step priority window before the harness's
        // automatic passes can resolve the trigger and advance to another turn.
        let base = FastActorSessionV1::from_cerberus_v19_engine_priority_fixture_state(state);
        let mut session = HumanCerberusSessionV19::from_session(base).unwrap();
        let current = expected(&session);
        let mut projector = HumanCerberusProjectorV19::new(current.acting_player);
        let prompt = projector.project_current(&session, current).unwrap();
        safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.state.extensions.triggers.len(), 1);
        assert_eq!(
            prompt.state.extensions.triggers[0].captured_controller,
            actor.into()
        );
        assert!(prompt.state.extensions.creature_died_this_turn);
        assert_eq!(
            session.public_observation(current).unwrap().triggers[0].phase,
            "published"
        );
        let snapshot = session.snapshot_v19();
        let before = session.inner.privileged_core_environment_hash();
        let input = action(&prompt, "Pass");
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        session.restore_v19(&snapshot).unwrap();
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        assert_eq!(
            projector.submit(&mut session, input),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        for _ in 0..8 {
            let current = expected(&session);
            let observation = session.public_observation(current).unwrap();
            if observation.triggers[0].phase == "completed" {
                break;
            }
            let mut fresh = HumanCerberusProjectorV19::new(current.acting_player);
            let prompt = fresh.project_current(&session, current).unwrap();
            let input = action(&prompt, "Pass");
            fresh.submit(&mut session, input).unwrap();
        }
        let current = expected(&session);
        let public = session.public_observation(current).unwrap();
        assert_eq!(public.triggers[0].phase, "completed");
        assert!(matches!(
            public.triggers[0].outcome,
            Some(crate::cerberus_public_v1::OutcomeV1::Resolved {
                morbid: true,
                before_tapped: Some(true),
                after_tapped: Some(false),
                ..
            })
        ));
        assert_eq!(public.triggers[0].source_at_trigger.arena_id, source.0);
        let snapshot = session.snapshot_v19();
        session.restore_v19(&snapshot).unwrap();
        assert_eq!(
            session.public_observation(expected(&session)).unwrap(),
            public
        );
    }
}
