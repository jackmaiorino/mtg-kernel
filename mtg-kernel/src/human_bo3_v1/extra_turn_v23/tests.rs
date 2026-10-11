use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::Zone;

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn future_actual_cast_queue_restore_order_privacy_and_old_projection_refusal_both_seats() {
    use crate::extra_turn_v1::tests::{
        future_printed::{cast, next_turn, resolve},
        ready,
    };
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let original = cast(&mut state, actor);
        let (_, copy_id) =
            crate::engine::copy_spell_for(&mut state, original.source, actor.opponent()).unwrap();
        resolve(&mut state);
        let core = FastActorSessionV1::from_extra_turn_v23_fixture_state(state.clone());
        let old = HumanRaidSessionV9::from_session(core.clone());
        let mut session = HumanExtraTurnSessionV23::from_session(core).unwrap();
        let current = expected(&session);
        if let Ok(old) = old {
            assert!(HumanDecisionProjectorV9::new(actor.into())
                .project_current(&old, current)
                .is_err());
        }
        let public = session.public_observation(current).unwrap();
        let queue = &public.turn_schedule.as_ref().unwrap().queued_next_first;
        assert_eq!(
            queue
                .iter()
                .map(|row| row.captured_controller)
                .collect::<Vec<_>>(),
            vec![actor.into(), actor.opponent().into()]
        );
        assert!(!queue[0].spell_copy);
        assert!(queue[1].spell_copy);
        assert_eq!(
            queue
                .iter()
                .map(|row| row.creation_ordinal)
                .collect::<Vec<_>>(),
            vec![1, 0]
        );
        let mut projector = HumanExtraTurnProjectorV23::new(actor.into());
        let prompt = projector.project_current(&session, current).unwrap();
        safe(&serde_json::to_value(&prompt).unwrap());
        let snapshot = session.snapshot_v23();
        let request = action(&prompt, "Pass priority");
        projector.submit(&mut session, request.clone()).unwrap();
        session.restore_v23(&snapshot).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, request),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let refreshed = projector
            .project_current(&session, expected(&session))
            .unwrap();
        assert!(refreshed.prompt_seq > prompt.prompt_seq);
        session
            .inner
            .extra_turn_fixture_state_mut()
            .extra_turn_v1
            .as_mut()
            .unwrap()
            .queued
            .reverse();
        let forged_before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, action(&refreshed, "Pass priority")),
            Err(Error::StaleDecision)
        );
        assert_eq!(
            session.inner.privileged_core_environment_hash(),
            forged_before
        );
        session.restore_v23(&snapshot).unwrap();
        for _ in 0..2 {
            next_turn(&mut state);
        }
        assert_eq!(
            state.extra_turn_v1.as_ref().unwrap().current_extra,
            Some(copy_id)
        );
        let extra = HumanExtraTurnSessionV23::from_session(
            FastActorSessionV1::from_extra_turn_v23_fixture_state(state),
        )
        .unwrap();
        let projected = HumanExtraTurnProjectorV23::new(actor.opponent().into())
            .project_current(&extra, expected(&extra))
            .unwrap();
        assert_eq!(
            projected
                .state
                .extensions
                .turn_schedule
                .unwrap()
                .current_extra
                .unwrap()
                .captured_controller,
            actor.opponent().into()
        );
    }
}

fn basic(actor: PlayerId, hidden: &str) -> crate::state::GameState {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    state
}
fn expected(session: &HumanExtraTurnSessionV23) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(decision) = session.current_response() else {
        panic!("real decision");
    };
    decision
}
fn action(prompt: &HumanDecisionExtraTurnV23, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanExtraTurnSessionV23::from_session(base).unwrap();
        let current = expected(&session);
        let prior = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&old, current)
            .unwrap();
        let mut projector = HumanExtraTurnProjectorV23::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.actions, prior.actions);
        assert_eq!(prompt.state.public, prior.state.public);
        assert_eq!(prompt.state.extensions.predecessor, prior.state.extensions);
        assert!(prompt.state.extensions.turn_schedule.is_none());
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let hidden = HumanExtraTurnSessionV23::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            HumanExtraTurnProjectorV23::new(actor.into())
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
        let mut session = HumanExtraTurnSessionV23::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Lightning Bolt")),
        )
        .unwrap();
        let mut projector = HumanExtraTurnProjectorV23::new(actor.into());
        let first = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let snapshot = session.snapshot_v23();
        let paid = action(&first, "Play Forest");
        projector.submit(&mut session, paid.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_request = action(&future, "Pass priority");
        session.restore_v23(&snapshot).unwrap();
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
        let foreign = HumanExtraTurnSessionV23::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            session.restore_v23(&foreign.snapshot_v23()),
            Err(Error::StaleDecision)
        );
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v23(&snapshot),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
    }
}

// Future actual printed groups. These require the genuine ExtraTurn definition;
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
        let mut session = HumanExtraTurnSessionV23::from_session(base).unwrap();
        let e = expected(&session);
        let old = HumanDecisionProjectorV9::new(e.acting_player)
            .project_current(&previous, e)
            .unwrap();
        let mut projector = HumanExtraTurnProjectorV23::new(e.acting_player);
        let prompt = projector.project_current(&session, e).unwrap();
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert_eq!(prompt.actions, old.actions);
        safe(&serde_json::to_value(&prompt).unwrap());
        let prefix = if pay { "Pay" } else { "Decline to pay" };
        let input = action(&prompt, prefix);
        let snapshot = session.snapshot_v23();
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v23(&snapshot).unwrap();
        let mut replay = HumanExtraTurnProjectorV23::new(e.acting_player);
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
