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
fn expected(session: &HumanWildwoodSessionV16) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(decision) = session.current_response() else {
        panic!("real decision");
    };
    decision
}
fn action(prompt: &HumanDecisionWildwoodV16, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanWildwoodSessionV16::from_session(base).unwrap();
        let current = expected(&session);
        let prior = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&old, current)
            .unwrap();
        let mut projector = HumanWildwoodProjectorV16::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.actions, prior.actions);
        assert_eq!(prompt.state.public, prior.state.public);
        assert_eq!(prompt.state.extensions.predecessor, prior.state.extensions);
        assert!(prompt.state.extensions.counter_occurrences.is_empty());
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let hidden = HumanWildwoodSessionV16::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            HumanWildwoodProjectorV16::new(actor.into())
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
        let mut session = HumanWildwoodSessionV16::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Lightning Bolt")),
        )
        .unwrap();
        let mut projector = HumanWildwoodProjectorV16::new(actor.into());
        let first = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let snapshot = session.snapshot_v16();
        let paid = action(&first, "Play Forest");
        projector.submit(&mut session, paid.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_request = action(&future, "Pass priority");
        session.restore_v16(&snapshot).unwrap();
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
        let foreign = HumanWildwoodSessionV16::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            session.restore_v16(&foreign.snapshot_v16()),
            Err(Error::StaleDecision)
        );
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v16(&snapshot),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
    }
}

// Future actual printed groups. These require the genuine Wildwood definition;
// they are compiled source preparation, not registered gameplay qualification.
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn actual_capture(
    actor: PlayerId,
    reverse: bool,
    hidden: &str,
) -> (
    crate::state::GameState,
    crate::ids::ObjectId,
    crate::ids::ObjectId,
) {
    use crate::engine::{self, Action, Decision};
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    let recipient = if reverse {
        put(&mut state, actor, "Forest", Zone::Battlefield);
        put(&mut state, actor, "Llanowar Elves", Zone::Battlefield)
    } else {
        let elf = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        put(&mut state, actor, "Forest", Zone::Battlefield);
        elf
    };
    let source = put(&mut state, actor, "Wildwood Scourge", Zone::Hand);
    state.players[actor.index()].mana_pool[crate::mana::ManaColor::G.pool_index()] = 1;
    state.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] = 2;
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    engine::step(&mut state, Action::CastSpell(source)).unwrap();
    assert!(
        matches!(engine::advance_until_decision(&mut state), Decision::ChooseEffectOption { option_count, .. } if option_count > 2)
    );
    engine::step(&mut state, Action::ChooseEffectOption(2)).unwrap();
    for _ in 0..24 {
        let decision = engine::advance_until_decision(&mut state);
        if state.stack.is_empty() {
            break;
        }
        assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
        engine::step(&mut state, Action::Pass).unwrap();
    }
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(source).counters.plus1_plus1, 2);
    crate::event::add_plus_one_counters(&mut state, recipient, actor.opponent(), 7).unwrap();
    (state, source, recipient)
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn actual_printed_event_time_controller_recipient_departure_public_capture_and_safe_renumbering() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, recipient) = actual_capture(actor, false, "Lightning Bolt");
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::zone_change(recipient, Zone::Graveyard),
        );
        state.objects.get_mut(source).controller = actor.opponent();
        let session = HumanWildwoodSessionV16::from_session(
            FastActorSessionV1::from_wildwood_v16_fixture_state(state),
        )
        .unwrap();
        let current = expected(&session);
        let observation = session.public_observation(current).unwrap();
        let [row] = observation.counter_occurrences.as_slice() else {
            panic!("one genuine captured occurrence");
        };
        assert_eq!(row.amount, 7);
        assert_eq!(row.placing_player, actor.opponent().into());
        assert_eq!(row.captured_controller, actor.into());
        assert_eq!(row.recipient_at_event.zone, Zone::Battlefield);
        assert!(
            row.original_source_present && row.recipient_was_creature && !row.recipient_was_hydra
        );
        let prompt = HumanWildwoodProjectorV16::new(current.acting_player)
            .project_current(&session, current)
            .unwrap();
        safe(&serde_json::to_value(&prompt).unwrap());
        let (mut other, second_source, second_recipient) = actual_capture(actor, true, "Island");
        crate::event::propose_and_commit(
            &mut other,
            crate::event::ProposedEvent::zone_change(second_recipient, Zone::Graveyard),
        );
        other.objects.get_mut(second_source).controller = actor.opponent();
        let other = HumanWildwoodSessionV16::from_session(
            FastActorSessionV1::from_wildwood_v16_fixture_state(other),
        )
        .unwrap();
        let other_current = expected(&other);
        let other_prompt = HumanWildwoodProjectorV16::new(other_current.acting_player)
            .project_current(&other, other_current)
            .unwrap();
        assert_eq!(prompt, other_prompt);
    }
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn actual_printed_legacy_refusal_bound_relation_forgery_and_restore_refuse_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, _, _) = actual_capture(actor, false, "Lightning Bolt");
        assert!(crate::wildwood_capture_v1::refuse_legacy(&state).is_err());
        let mut session = HumanWildwoodSessionV16::from_session(
            FastActorSessionV1::from_wildwood_v16_fixture_state(state),
        )
        .unwrap();
        let current = expected(&session);
        let mut projector = HumanWildwoodProjectorV16::new(current.acting_player);
        let prompt = projector.project_current(&session, current).unwrap();
        let request = action(&prompt, "Pass priority");
        let snapshot = session.snapshot_v16();
        let history = &mut session
            .inner
            .wildwood_fixture_state_mut()
            .engine
            .event_history;
        let capture = history
            .iter_mut()
            .find_map(|event| match event {
                crate::event::CommittedEvent::WildwoodCounterCaptureV1 { capture } => Some(capture),
                _ => None,
            })
            .unwrap();
        capture.recipient_is_hydra = true;
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, request.clone()),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        session.restore_v16(&snapshot).unwrap();
        assert_eq!(
            projector.submit(&mut session, request),
            Err(Error::StaleDecision)
        );
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        projector
            .submit(&mut session, action(&prompt, "Pass priority"))
            .unwrap();
    }
}
