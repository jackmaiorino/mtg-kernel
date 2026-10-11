use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::{GameState, Zone};

fn expected(session: &HumanEmploymentSessionV18) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("real fixture must reach a decision");
    };
    expected
}
fn owned(state: GameState) -> HumanEmploymentSessionV18 {
    HumanEmploymentSessionV18::from_session(FastActorSessionV1::from_employment_v18_fixture_state(
        state,
    ))
    .unwrap()
}
fn basic(actor: PlayerId, hidden: &str) -> GameState {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    state
}
fn request(prompt: &HumanDecisionEmploymentV18, prefix: &str) -> HumanActionRequestV1 {
    let row = prompt
        .actions
        .iter()
        .find(|row| row.label.starts_with(prefix))
        .unwrap_or_else(|| panic!("missing {prefix}: {:?}", prompt.actions));
    HumanActionRequestV1 {
        prompt_seq: prompt.prompt_seq,
        action_index: row.action_index,
    }
}
fn assert_safe(value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, child) in fields {
                assert!(!matches!(
                    key.as_str(),
                    "arena_id"
                        | "zone_change_count"
                        | "card_db_id"
                        | "recipient_snapshot"
                        | "event_history"
                        | "engine_indexes"
                        | "environment_revision"
                        | "producer_sha256"
                        | "projection_sha256"
                        | "binding"
                ));
                assert_safe(child);
            }
        }
        serde_json::Value::Array(rows) => {
            for row in rows {
                assert_safe(row);
            }
        }
        _ => {}
    }
}

#[test]
fn ordinary_menu_state_privacy_and_real_submit_retry_parity_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let base = FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Lightning Bolt"));
        let predecessor = HumanRaidSessionV9::from_session(base.clone()).unwrap();
        let mut session = HumanEmploymentSessionV18::from_session(base).unwrap();
        let current = expected(&session);
        let old = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&predecessor, current)
            .unwrap();
        let mut projector = HumanEmploymentProjectorV18::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.actions, old.actions);
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert!(prompt
            .state
            .extensions
            .temporary_control_installations
            .is_empty());
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let hidden = owned(basic(actor, "Island"));
        assert_eq!(
            HumanEmploymentProjectorV18::new(actor.into())
                .project_current(&hidden, expected(&hidden))
                .unwrap(),
            prompt
        );
        assert_eq!(
            HumanEmploymentProjectorV18::new(actor.opponent().into())
                .project_current(&session, current),
            Err(Error::NotHumanTurn)
        );
        let input = request(&prompt, "Play Forest");
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(projector.submit(&mut session, input).unwrap(), receipt);
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
    }
}

#[test]
fn owning_restore_rejects_old_pending_accepted_future_and_foreign_owner_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut session = owned(basic(actor, "Lightning Bolt"));
        let mut projector = HumanEmploymentProjectorV18::new(actor.into());
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let saved = session.snapshot_v18();
        let mut stale = HumanEmploymentProjectorV18::new(actor.into());
        stale.project_current(&session, expected(&session)).unwrap();
        let input = request(&prompt, "Play Forest");
        projector.submit(&mut session, input.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_input = request(&future, "Pass priority");
        session.restore_v18(&saved).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()),
            Err(Error::StaleDecision)
        );
        assert_eq!(stale.submit(&mut session, input), Err(Error::StaleDecision));
        assert_eq!(
            projector.submit(&mut session, future_input.clone()),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let fresh = projector
            .project_current(&session, expected(&session))
            .unwrap();
        assert!(fresh.prompt_seq > future.prompt_seq);
        projector
            .submit(&mut session, request(&fresh, "Play Forest"))
            .unwrap();
        // Recreate exactly the formerly issued future core. Authority remains
        // newer even when the same action is still a legal next choice.
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, future_input),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let foreign = owned(basic(actor, "Lightning Bolt")).snapshot_v18();
        assert_eq!(session.restore_v18(&foreign), Err(Error::StaleDecision));
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v18(&saved),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
    }
}

#[test]
fn registered_control_primitive_safe_dto_foreign_menu_complete_binding_and_restore() {
    use crate::event::CommittedEvent;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, _, _) = crate::employment_public_v1::tests::primitive(actor);
        let mut session = owned(state.clone());
        let current = expected(&session);
        let mut projector = HumanEmploymentProjectorV18::new(current.acting_player);
        let prompt = projector.project_current(&session, current).unwrap();
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        let row = &prompt.state.extensions.temporary_control_installations[0];
        assert_eq!(row.current_tapped, Some(false));
        assert_eq!(row.current_has_haste, Some(true));
        let input = request(&prompt, "Pass priority");
        let mut foreign = owned(state);
        let before = foreign.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut foreign, input.clone()),
            Err(Error::StaleDecision)
        );
        assert_eq!(foreign.inner.privileged_core_environment_hash(), before);
        let saved = session.snapshot_v18();
        projector.pending.as_mut().unwrap().public.installations[0].expired = true;
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        session.restore_v18(&saved).unwrap();
        let mut projector = HumanEmploymentProjectorV18::new(current.acting_player);
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let input = request(&prompt, "Pass priority");
        session
            .inner
            .employment_fixture_state_mut()
            .engine
            .event_history
            .retain(|event| !matches!(event, CommittedEvent::TemporaryControlInstalledV1 { .. }));
        let damaged = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), damaged);
        session.restore_v18(&saved).unwrap();
        assert!(session.public_observation(expected(&session)).is_ok());
    }
}

#[test]
fn registered_ward_uses_real_predecessor_labels_and_state() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let state = crate::employment_public_v1::tests::ward_state(actor);
        let base = FastActorSessionV1::from_human_v8_fixture_state(state);
        let old = HumanRaidSessionV9::from_session(base.clone()).unwrap();
        let session = HumanEmploymentSessionV18::from_session(base).unwrap();
        let current = expected(&session);
        let predecessor = HumanDecisionProjectorV9::new(current.acting_player)
            .project_current(&old, current)
            .unwrap();
        let prompt = HumanEmploymentProjectorV18::new(current.acting_player)
            .project_current(&session, current)
            .unwrap();
        assert_eq!(prompt.actions, predecessor.actions);
        assert_eq!(
            prompt.state.extensions.predecessor,
            predecessor.state.extensions
        );
        assert_eq!(prompt.state.public, predecessor.state.public);
        assert_safe(&serde_json::to_value(&prompt).unwrap());
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn actual_future_printed_human_cast_target_restore_resolution_and_expiry() {
    use crate::mana::ManaColor;
    use crate::state::Step;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = basic(actor, "Lightning Bolt");
        put(&mut state, actor, "Involuntary Employment", Zone::Hand);
        let recipient = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        state.objects.get_mut(recipient).tapped = true;
        for owner in [actor, actor.opponent()] {
            for _ in 0..8 {
                put(&mut state, owner, "Forest", Zone::Library);
            }
        }
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        // Opposing response availability preserves the finalized stack decision.
        state.players[actor.opponent().index()].mana_pool[ManaColor::R.pool_index()] = 1;
        let base = FastActorSessionV1::from_human_v8_fixture_state(state);
        let predecessor = HumanRaidSessionV9::from_session(base.clone()).unwrap();
        let mut session = HumanEmploymentSessionV18::from_session(base).unwrap();
        let current = expected(&session);
        assert!(HumanDecisionProjectorV9::new(actor.into())
            .project_current(&predecessor, current)
            .is_err());
        let mut projector = HumanEmploymentProjectorV18::new(actor.into());
        let prompt = projector.project_current(&session, current).unwrap();
        let cast = request(&prompt, "Cast Involuntary Employment");
        assert!(prompt.actions[cast.action_index as usize]
            .label
            .contains("{3}{R}"));
        projector.submit(&mut session, cast).unwrap();
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let saved = session.snapshot_v18();
        let target = request(&prompt, "Target Llanowar Elves");
        projector.submit(&mut session, target.clone()).unwrap();
        let public = session.public_observation(expected(&session)).unwrap();
        assert_eq!(public.projection.surface.stack.len(), 1);
        session.restore_v18(&saved).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, target),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        projector
            .submit(&mut session, request(&prompt, "Target Llanowar Elves"))
            .unwrap();
        for _ in 0..16 {
            let current = expected(&session);
            if session
                .public_observation(current)
                .unwrap()
                .projection
                .surface
                .stack
                .is_empty()
            {
                break;
            }
            let mut acting = HumanEmploymentProjectorV18::new(current.acting_player);
            let prompt = acting.project_current(&session, current).unwrap();
            acting
                .submit(&mut session, request(&prompt, "Pass priority"))
                .unwrap();
        }
        let public = session.public_observation(expected(&session)).unwrap();
        assert_eq!(public.installations.len(), 1);
        assert_eq!(public.installations[0].current_has_haste, Some(true));
        assert_eq!(public.installations[0].current_tapped, Some(false));
        let current = expected(&session);
        let prompt = HumanEmploymentProjectorV18::new(current.acting_player)
            .project_current(&session, current)
            .unwrap();
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        assert!(!prompt.state.extensions.temporary_control_installations[0].expired);
        // Advance real Cleanup from a fresh owning session after actual resolution.
        session.inner.employment_fixture_state_mut().step = Step::End;
        let mut physical = session.inner.employment_fixture_state_mut().clone();
        crate::employment_public_v1::tests::cleanup(&mut physical);
        let session = owned(physical);
        let current = expected(&session);
        let prompt = HumanEmploymentProjectorV18::new(current.acting_player)
            .project_current(&session, current)
            .unwrap();
        let row = &prompt.state.extensions.temporary_control_installations[0];
        assert!(row.expired);
        assert_eq!(row.current_has_haste, Some(false));
        assert_eq!(
            row.current_recipient.as_ref().unwrap().controller,
            actor.opponent().into()
        );
    }
}
