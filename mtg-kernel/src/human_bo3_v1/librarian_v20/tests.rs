use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::{GameState, Zone};

fn expected(session: &HumanLibrarianSessionV20) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("real fixture must reach a decision");
    };
    expected
}
fn owned(state: GameState) -> HumanLibrarianSessionV20 {
    HumanLibrarianSessionV20::from_session(FastActorSessionV1::from_librarian_v20_fixture_state(
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
fn request(prompt: &HumanDecisionLibrarianV20, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanLibrarianSessionV20::from_session(base).unwrap();
        let current = expected(&session);
        let old = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&predecessor, current)
            .unwrap();
        let mut projector = HumanLibrarianProjectorV20::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.actions, old.actions);
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert!(prompt.state.extensions.activations.is_empty());
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let hidden = owned(basic(actor, "Island"));
        assert_eq!(
            HumanLibrarianProjectorV20::new(actor.into())
                .project_current(&hidden, expected(&hidden))
                .unwrap(),
            prompt
        );
        assert_eq!(
            HumanLibrarianProjectorV20::new(actor.opponent().into())
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
        let mut projector = HumanLibrarianProjectorV20::new(actor.into());
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let saved = session.snapshot_v20();
        let mut stale = HumanLibrarianProjectorV20::new(actor.into());
        stale.project_current(&session, expected(&session)).unwrap();
        let input = request(&prompt, "Play Forest");
        projector.submit(&mut session, input.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_input = request(&future, "Pass priority");
        session.restore_v20(&saved).unwrap();
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
        let foreign = owned(basic(actor, "Lightning Bolt")).snapshot_v20();
        assert_eq!(session.restore_v20(&foreign), Err(Error::StaleDecision));
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v20(&saved),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
    }
}

#[test]
fn registered_real_ward_preserves_predecessor_labels_and_all_typed_state() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let base = FastActorSessionV1::from_human_v8_fixture_state(
            crate::librarian_public_v1::tests::ward_state(actor),
        );
        let old = HumanRaidSessionV9::from_session(base.clone()).unwrap();
        let session = HumanLibrarianSessionV20::from_session(base).unwrap();
        let current = expected(&session);
        let predecessor = HumanDecisionProjectorV9::new(current.acting_player)
            .project_current(&old, current)
            .unwrap();
        let prompt = HumanLibrarianProjectorV20::new(current.acting_player)
            .project_current(&session, current)
            .unwrap();
        assert_eq!(prompt.actions, predecessor.actions);
        assert_eq!(
            prompt.state.extensions.predecessor,
            predecessor.state.extensions
        );
        assert_eq!(prompt.state.public, predecessor.state.public);
        assert_eq!(
            prompt.state.known_face_down_cards,
            predecessor.state.known_face_down_cards
        );
        assert_eq!(
            prompt.state.poison_counters,
            predecessor.state.poison_counters
        );
        assert_eq!(
            prompt.state.poison_prevention,
            predecessor.state.poison_prevention
        );
        assert_eq!(
            prompt.state.creatures_attacked_this_turn,
            predecessor.state.creatures_attacked_this_turn
        );
        assert_eq!(prompt.state.ninja_emblems, predecessor.state.ninja_emblems);
        assert_eq!(
            prompt.state.restricted_mana,
            predecessor.state.restricted_mana
        );
        assert_safe(&serde_json::to_value(&prompt).unwrap());
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn genuine_future_paid_public_facts_foreign_menu_and_coherent_binding_forgery_refuse_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, source) = crate::librarian_public_v1::tests::paid(actor);
        let mut session = owned(state.clone());
        let current = expected(&session);
        let mut projector = HumanLibrarianProjectorV20::new(current.acting_player);
        let prompt = projector.project_current(&session, current).unwrap();
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        let row = &prompt.state.extensions.activations[0];
        assert_eq!(row.captured_controller, actor.into());
        assert_eq!(row.lifecycle, crate::librarian_public_v1::LifecycleV1::Paid);
        assert_eq!(row.current_once_used, Some(true));
        let input = request(&prompt, "Pass priority");
        let mut foreign = owned(state);
        let before = foreign.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut foreign, input.clone()),
            Err(Error::StaleDecision)
        );
        assert_eq!(foreign.inner.privileged_core_environment_hash(), before);
        let saved = session.snapshot_v20();
        projector.pending.as_mut().unwrap().public.activations[0].captured_controller =
            actor.opponent().into();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        session.restore_v20(&saved).unwrap();
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let input = request(&prompt, "Pass priority");
        session
            .inner
            .librarian_fixture_state_mut()
            .objects
            .get_mut(source)
            .v4
            .creature_upgrade
            .as_mut()
            .unwrap()
            .once_activated
            .clear();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        session.restore_v20(&saved).unwrap();
        assert!(session.public_observation(expected(&session)).is_ok());
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn pass_until_source_is_on_battlefield(
    session: &mut HumanLibrarianSessionV20,
    source: crate::ids::ObjectId,
    actor: PlayerId,
) {
    for _ in 0..30 {
        let current = expected(session);
        if session
            .inner
            .librarian_fixture_state_mut()
            .objects
            .get(source)
            .zone
            == Zone::Battlefield
            && current.acting_player == actor.into()
        {
            return;
        }
        let (_, actions, _) = session
            .inner
            .librarian_current_decision_input_v20(current, current.acting_player)
            .unwrap();
        let index = actions
            .iter()
            .position(|semantic| matches!(semantic, ActionSemanticV1::Pass { .. }))
            .expect("actual priority pass");
        session
            .inner
            .step(
                current.episode_id,
                current.step,
                u32::try_from(index).unwrap(),
            )
            .unwrap();
    }
    panic!("real printed creature resolution failed");
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn genuine_future_human_cast_and_activate_real_payment_have_precise_labels_once_and_restored_priority(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = basic(actor, "Lightning Bolt");
        let source = put(&mut state, actor, "Mild-Mannered Librarian", Zone::Hand);
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::G.pool_index()] = 5;
        state.players[actor.opponent().index()].mana_pool[crate::mana::ManaColor::R.pool_index()] =
            1;
        let mut session = owned(state);
        let mut projector = HumanLibrarianProjectorV20::new(actor.into());
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let cast = request(&prompt, "Begin casting Mild-Mannered Librarian");
        assert!(prompt.actions[cast.action_index as usize]
            .label
            .contains("printed cost {G}"));
        projector.submit(&mut session, cast).unwrap();
        pass_until_source_is_on_battlefield(&mut session, source, actor);
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let activation = request(&prompt, "Activate Mild-Mannered Librarian");
        assert!(prompt.actions[activation.action_index as usize]
            .label
            .contains("activate only once"));
        assert!(prompt.actions[activation.action_index as usize]
            .label
            .contains("printed cost {3}{G}"));
        let saved = session.snapshot_v20();
        projector.submit(&mut session, activation.clone()).unwrap();
        let public = session.public_observation(expected(&session)).unwrap();
        assert_eq!(public.activations.len(), 1);
        assert!(public.activations[0].once_consumed_on_payment);
        assert_eq!(
            session.inner.librarian_fixture_state_mut().players[actor.index()].mana_pool
                [crate::mana::ManaColor::G.pool_index()],
            0
        );
        session.restore_v20(&saved).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, activation),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let fresh = projector
            .project_current(&session, expected(&session))
            .unwrap();
        projector
            .submit(
                &mut session,
                request(&fresh, "Activate Mild-Mannered Librarian"),
            )
            .unwrap();
        assert!(
            session
                .public_observation(expected(&session))
                .unwrap()
                .activations[0]
                .once_consumed_on_payment
        );
    }
}
