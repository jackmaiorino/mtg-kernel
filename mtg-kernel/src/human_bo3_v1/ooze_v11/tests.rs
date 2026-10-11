use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::{GameState, Zone};

fn expected(session: &HumanOozeSessionV11) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("actual fixture must reach its decision");
    };
    expected
}
fn basic(actor: PlayerId, hidden: &str) -> GameState {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor, "Mountain", Zone::Battlefield);
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    state
}
fn owned(state: GameState) -> HumanOozeSessionV11 {
    HumanOozeSessionV11::from_session(FastActorSessionV1::from_human_v8_fixture_state(state))
        .unwrap()
}
fn request(prompt: &HumanDecisionOozeV11, prefix: &str) -> HumanActionRequestV1 {
    let row = prompt
        .actions
        .iter()
        .find(|row| row.label.starts_with(prefix))
        .unwrap_or_else(|| panic!("expected actual action {prefix}: {:?}", prompt.actions));
    HumanActionRequestV1 {
        prompt_seq: prompt.prompt_seq,
        action_index: row.action_index,
    }
}
#[test]
fn predecessor_parity_privacy_restore_and_retry_use_real_registered_decisions() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let base = FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Lightning Bolt"));
        let mut old_session = HumanRaidSessionV9::from_session(base.clone()).unwrap();
        let mut session = HumanOozeSessionV11::from_session(base).unwrap();
        let current = expected(&session);
        let old = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&old_session, current)
            .unwrap();
        let mut projector = HumanOozeProjectorV11::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(old.actions, prompt.actions);
        assert_eq!(old.state, prompt.state);
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        assert_eq!(
            HumanOozeProjectorV11::new(actor.opponent().into()).project_current(&session, current),
            Err(Error::NotHumanTurn)
        );
        let hidden = owned(basic(actor, "Fireblast"));
        assert_eq!(
            HumanOozeProjectorV11::new(actor.into())
                .project_current(&hidden, expected(&hidden))
                .unwrap(),
            prompt
        );
        let public = session.public_observation(current).unwrap();
        crate::ooze_public_v1::validate(&public).unwrap();
        let public_bytes = serde_json::to_vec(&public).unwrap();
        assert_eq!(
            public_bytes,
            serde_json::to_vec(&session.public_observation(current).unwrap()).unwrap()
        );
        let old_request = HumanActionRequestV1 {
            prompt_seq: old.prompt_seq,
            action_index: old
                .actions
                .iter()
                .find(|row| row.label.starts_with("Play Forest"))
                .unwrap()
                .action_index,
        };
        let mut old_projector = HumanDecisionProjectorV9::new(actor.into());
        let old_bound = old_projector
            .project_current(&old_session, current)
            .unwrap();
        assert_eq!(old_bound.actions, old.actions);
        let snapshot = session.snapshot_v11();
        let mut stale_projector = HumanOozeProjectorV11::new(actor.into());
        stale_projector.project_current(&session, current).unwrap();
        let input = request(&prompt, "Play Forest");
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            stale_projector.submit(&mut session, request(&prompt, "Play Forest")),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        assert_eq!(projector.submit(&mut session, input).unwrap(), receipt);
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        old_projector.submit(&mut old_session, old_request).unwrap();
        session.restore_v11(&snapshot).unwrap();
        let mut replay = HumanOozeProjectorV11::new(actor.into());
        let restored_prompt = replay
            .project_current(&session, expected(&session))
            .unwrap();
        assert_eq!(restored_prompt, prompt);
        replay
            .submit(&mut session, request(&restored_prompt, "Play Forest"))
            .unwrap();
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        let unchanged = session.inner.privileged_core_environment_hash();
        assert_eq!(
            replay.submit(
                &mut session,
                HumanActionRequestV1 {
                    prompt_seq: restored_prompt.prompt_seq,
                    action_index: u32::MAX,
                }
            ),
            Err(Error::ConflictingRetry)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), unchanged);
        let mut forged = public.clone();
        forged.predecessor.schema_version = 8;
        assert!(crate::ooze_public_v1::validate(&forged).is_err());
        let mut forged = public;
        forged.schema.push('x');
        assert!(crate::ooze_public_v1::validate(&forged).is_err());
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn future_printed_ooze_activation_public_meaning_human_target_and_exact_replay_both_seats() {
    use crate::mana::ManaColor;
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (target_name, grows) in [("Llanowar Elves", true), ("Lightning Bolt", false)] {
            let mut state = basic(actor, "Fireblast");
            let source = put(&mut state, actor, "Scavenging Ooze", Zone::Battlefield);
            let target = put(&mut state, actor.opponent(), target_name, Zone::Graveyard);
            state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
            let base = FastActorSessionV1::from_human_v8_fixture_state(state);
            let predecessor = HumanRaidSessionV9::from_session(base.clone()).unwrap();
            let mut session = HumanOozeSessionV11::from_session(base).unwrap();
            let current = expected(&session);
            assert_eq!(
                HumanDecisionProjectorV9::new(actor.into()).project_current(&predecessor, current),
                Err(Error::UnsupportedPrompt)
            );
            let mut projector = HumanOozeProjectorV11::new(actor.into());
            let prompt = projector.project_current(&session, current).unwrap();
            let atomic = session.public_observation(current).unwrap();
            assert_eq!(atomic.atomic_activation_roots.len(), 1);
            crate::rules_meaning_ooze_v1::validate(&atomic.atomic_activation_roots[0]).unwrap();
            let activation = request(&prompt, "Activate Scavenging Ooze");
            assert!(prompt
                .actions
                .iter()
                .find(|row| row.action_index == activation.action_index)
                .unwrap()
                .label
                .contains("still gain life"));
            projector.submit(&mut session, activation).unwrap();
            let current = expected(&session);
            let mut selector = HumanOozeProjectorV11::new(actor.into());
            let target_prompt = selector.project_current(&session, current).unwrap();
            assert!(!target_prompt
                .actions
                .iter()
                .any(|row| row.label.contains("Finish")));
            let snapshot = session.snapshot_v11();
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(
                selector.submit(
                    &mut session,
                    HumanActionRequestV1 {
                        prompt_seq: target_prompt.prompt_seq,
                        action_index: u32::MAX,
                    }
                ),
                Err(Error::InvalidAction)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            let choice = request(&target_prompt, &format!("Target {target_name}"));
            selector.submit(&mut session, choice.clone()).unwrap();
            let announced = session.inner.privileged_core_environment_hash();
            session.restore_v11(&snapshot).unwrap();
            assert_eq!(
                selector.submit(&mut session, choice.clone()),
                Err(Error::StaleDecision)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            let mut selector = HumanOozeProjectorV11::new(actor.into());
            assert_eq!(
                selector
                    .project_current(&session, expected(&session))
                    .unwrap(),
                target_prompt
            );
            selector.submit(&mut session, choice).unwrap();
            assert_eq!(session.inner.privileged_core_environment_hash(), announced);
            // Core printed fixtures check exact source counter and captured gain.
            // Here actual Human Pass resolves the real printed activation.
            for _ in 0..30 {
                let FastActorResponseV1::Decision(e) = session.current_response() else {
                    break;
                };
                let mut actor_projector = HumanOozeProjectorV11::new(e.acting_player);
                let decision = actor_projector.project_current(&session, e).unwrap();
                let public = session.public_observation(e).unwrap();
                if public.predecessor.projection.surface.stack.is_empty() {
                    break;
                }
                actor_projector
                    .submit(&mut session, request(&decision, "Pass priority"))
                    .unwrap();
            }
            let final_public = session.public_observation(expected(&session)).unwrap();
            assert!(final_public.predecessor.projection.surface.stack.is_empty());
            let rows = &final_public.predecessor.projection.surface;
            assert!(rows.exile.iter().any(|row| row.stable.card_db_id
                == crate::card_def::card_id_by_name(target_name).unwrap()));
            assert!(rows.exile.iter().any(|row| row.stable.arena_id == target.0));
            let source_row = rows.battlefield[actor.index()]
                .iter()
                .find(|row| row.stable.arena_id == source.0)
                .unwrap();
            assert_eq!(source_row.counters.plus1_plus1, i16::from(grows));
            assert_eq!(rows.life_totals[actor.index()], 20 + i32::from(grows));
            assert_eq!(rows.life_totals[actor.opponent().index()], 20);
        }
    }
}

fn assert_safe(value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, child) in fields {
                assert!(
                    !matches!(
                        key.as_str(),
                        "arena_id"
                            | "zone_change_count"
                            | "visible_projection_hash"
                            | "physical_decision_id"
                            | "environment_revision"
                            | "episode_id"
                            | "seed"
                            | "timestamp"
                            | "binding"
                            | "engine_indexes"
                            | "logits"
                            | "value"
                    ),
                    "private Human key {key}"
                );
                assert_safe(child);
            }
        }
        serde_json::Value::Array(rows) => {
            for child in rows {
                assert_safe(child);
            }
        }
        _ => {}
    }
}
#[test]
fn inherited_registered_generic_ward_menu_is_preserved_and_submission_replays() {
    for pay in [false, true] {
        let (mut state, _, _) = crate::policy_observation_v6::tests::ward_multi_targeter_state();
        crate::policy_observation_v6::tests::reach_ward_payment(&mut state);
        let base = FastActorSessionV1::from_human_v8_fixture_state(state);
        let previous = HumanRaidSessionV9::from_session(base.clone()).unwrap();
        let mut session = HumanOozeSessionV11::from_session(base).unwrap();
        let e = expected(&session);
        let old = HumanDecisionProjectorV9::new(e.acting_player)
            .project_current(&previous, e)
            .unwrap();
        let mut projector = HumanOozeProjectorV11::new(e.acting_player);
        let prompt = projector.project_current(&session, e).unwrap();
        assert_eq!(prompt.state, old.state);
        assert_eq!(prompt.actions, old.actions);
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        let prefix = if pay { "Pay" } else { "Decline to pay" };
        let input = request(&prompt, prefix);
        let snapshot = session.snapshot_v11();
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v11(&snapshot).unwrap();
        let mut replay = HumanOozeProjectorV11::new(e.acting_player);
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
fn owning_restore_invalidates_same_projector_receipts_pending_and_rebuilt_future_both_seats() {
    use crate::mana::ManaColor;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = basic(actor, "Fireblast");
        put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        let mut session = owned(state);
        let snapshot = session.snapshot_v11();
        let initial = session.inner.privileged_core_environment_hash();
        let mut accepted = HumanOozeProjectorV11::new(actor.into());
        let prompt = accepted
            .project_current(&session, expected(&session))
            .unwrap();
        let input = request(&prompt, "Play Forest");
        let mut pending = HumanOozeProjectorV11::new(actor.into());
        assert_eq!(
            pending
                .project_current(&session, expected(&session))
                .unwrap(),
            prompt
        );
        accepted.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_ne!(initial, after);
        let mut future = HumanOozeProjectorV11::new(actor.into());
        let future_prompt = future
            .project_current(&session, expected(&session))
            .unwrap();
        let future_input = request(&future_prompt, "Begin casting Lightning Bolt");
        session.restore_v11(&snapshot).unwrap();
        assert_eq!(session.inner.privileged_core_environment_hash(), initial);
        assert_eq!(
            accepted.submit(&mut session, input.clone()),
            Err(Error::StaleDecision)
        );
        assert_eq!(
            pending.submit(&mut session, input.clone()),
            Err(Error::StaleDecision)
        );
        assert_eq!(
            future.submit(&mut session, future_input.clone()),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), initial);
        let refreshed = accepted
            .project_current(&session, expected(&session))
            .unwrap();
        assert!(refreshed.prompt_seq > prompt.prompt_seq);
        accepted
            .submit(&mut session, request(&refreshed, "Play Forest"))
            .unwrap();
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        // Even the exact formerly issued future state does not revive its prompt.
        assert_eq!(
            future.submit(&mut session, future_input),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        let refreshed_receipt_request = request(&refreshed, "Play Forest");
        assert!(accepted
            .submit(&mut session, refreshed_receipt_request)
            .is_ok());
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
    }
}
#[test]
fn owning_identity_and_restore_generation_exhaustion_are_read_only() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let first = owned(basic(actor, "Fireblast"));
        let mut foreign = owned(basic(actor, "Fireblast"));
        let mut projector = HumanOozeProjectorV11::new(actor.into());
        let prompt = projector.project_current(&first, expected(&first)).unwrap();
        let before = foreign.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut foreign, request(&prompt, "Play Forest")),
            Err(Error::StaleDecision)
        );
        assert_eq!(foreign.inner.privileged_core_environment_hash(), before);
        let snapshot = foreign.snapshot_v11();
        foreign.authority.generation = u64::MAX;
        assert_eq!(
            foreign.restore_v11(&snapshot),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(foreign.inner.privileged_core_environment_hash(), before);
    }
}
