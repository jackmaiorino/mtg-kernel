use super::*;

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
#[ignore = "requires actual Garruk/Banishing registration; source-only composition"]
fn genuine_later_apnap_aura_owner_has_v30_public_choices_restore_and_retry_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for suppress in [false, true] {
            let (state, _uprising, creature, own_aura, _other_aura, host) =
                crate::garruks_uprising_v1::attachment_tests::pending_returns(
                    actor, suppress, true,
                );
            let return_generation = state.objects.get(creature).zone_change_count + 1;
            let base = FastActorSessionV1::from_garruk_v30_fixture_state(state.clone());
            let mut session = HumanGarrukSessionV30::from_session(base).unwrap();
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("actual second APNAP attachment");
            };
            assert_eq!(expected.acting_player, actor.opponent().into());
            let public = session.public_observation(expected).unwrap();
            assert_eq!(public.linked_exile.confirmed_attachments.len(), 1);
            assert_eq!(
                public.linked_exile.confirmed_attachments[0].aura.arena_id,
                own_aura.0
            );
            assert_eq!(
                public.linked_exile.confirmed_attachments[0]
                    .current_chosen_host
                    .as_ref()
                    .unwrap()
                    .arena_id,
                host.0
            );
            assert_eq!(
                public
                    .linked_exile
                    .attachment_choice
                    .as_ref()
                    .unwrap()
                    .owner,
                actor.opponent().into()
            );
            let mut projector = HumanGarrukProjectorV30::new(actor.opponent().into());
            let prompt = projector.project_current(&session, expected).unwrap();
            assert_eq!(
                prompt
                    .state
                    .extensions
                    .linked_exile
                    .confirmed_attachments
                    .len(),
                1
            );
            safe(&serde_json::to_value(&prompt).unwrap());
            let choice = prompt
                .actions
                .iter()
                .find(|row| {
                    row.label.starts_with("Attach returning Twinblade Blessing")
                        && row.label.contains("to Empyrean Eagle")
                })
                .unwrap();
            let request = HumanActionRequestV1 {
                prompt_seq: prompt.prompt_seq,
                action_index: choice.action_index,
            };
            let snapshot = session.snapshot_v30();
            let receipt = projector.submit(&mut session, request.clone()).unwrap();
            let after = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, request.clone()).unwrap(),
                receipt
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), after);
            let FastActorResponseV1::Decision(next) = session.current_response() else {
                panic!("post-entry priority or draw ordering");
            };
            let completed = session.public_observation(next).unwrap();
            assert!(completed.linked_exile.attachment_choice.is_none());
            assert_eq!(completed.linked_exile.confirmed_attachments.len(), 2);
            let entries = completed
                .entry_occurrences
                .iter()
                .filter(|row| {
                    row.entrant_at_entry.arena_id == creature.0
                        && row.entrant_at_entry.zone_change_count == return_generation
                })
                .collect::<Vec<_>>();
            assert_eq!(entries.len(), usize::from(!suppress));
            if let Some(row) = entries.first() {
                assert_eq!(row.event_time_power, Some(4));
            }
            session.restore_v30(&snapshot).unwrap();
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, request),
                Err(Error::StaleDecision)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            let FastActorResponseV1::Decision(restored) = session.current_response() else {
                panic!("restored actual owner choice");
            };
            let refreshed = projector.project_current(&session, restored).unwrap();
            assert!(refreshed.prompt_seq > prompt.prompt_seq);
            assert_eq!(refreshed.actions, prompt.actions);
            let request = HumanActionRequestV1 {
                prompt_seq: refreshed.prompt_seq,
                action_index: choice.action_index,
            };
            projector.submit(&mut session, request).unwrap();
            assert_eq!(session.inner.privileged_core_environment_hash(), after);
            for rewrite_owner in [false, true] {
                let base = FastActorSessionV1::from_garruk_v30_fixture_state(state.clone());
                let mut bad = HumanGarrukSessionV30::from_session(base).unwrap();
                let FastActorResponseV1::Decision(expected) = bad.current_response() else {
                    panic!("genuine pending owner");
                };
                let mut projector = HumanGarrukProjectorV30::new(actor.opponent().into());
                let prompt = projector.project_current(&bad, expected).unwrap();
                let action = prompt.actions.first().unwrap();
                let request = HumanActionRequestV1 {
                    prompt_seq: prompt.prompt_seq,
                    action_index: action.action_index,
                };
                let pending = bad
                    .inner
                    .garruk_fixture_state_mut()
                    .engine
                    .linked_aura_operation_v1
                    .pending
                    .as_mut()
                    .unwrap();
                if rewrite_owner {
                    pending.choice.owner = actor;
                } else {
                    pending.choice.original_candidates.reverse();
                }
                let before = bad.inner.privileged_core_environment_hash();
                assert!(projector.submit(&mut bad, request).is_err());
                assert_eq!(bad.inner.privileged_core_environment_hash(), before);
            }
        }
    }
}
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
fn expected(session: &HumanGarrukSessionV30) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(decision) = session.current_response() else {
        panic!("real decision");
    };
    decision
}
fn action(prompt: &HumanDecisionGarrukV30, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanGarrukSessionV30::from_session(base).unwrap();
        let current = expected(&session);
        let prior = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&old, current)
            .unwrap();
        let mut projector = HumanGarrukProjectorV30::new(actor.into());
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
        let hidden = HumanGarrukSessionV30::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            HumanGarrukProjectorV30::new(actor.into())
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
        let mut session = HumanGarrukSessionV30::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Lightning Bolt")),
        )
        .unwrap();
        let mut projector = HumanGarrukProjectorV30::new(actor.into());
        let first = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let snapshot = session.snapshot_v30();
        let paid = action(&first, "Play Forest");
        projector.submit(&mut session, paid.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_request = action(&future, "Pass priority");
        session.restore_v30(&snapshot).unwrap();
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
        let foreign = HumanGarrukSessionV30::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            session.restore_v30(&foreign.snapshot_v30()),
            Err(Error::StaleDecision)
        );
        session.garruk.generation = u64::MAX;
        assert_eq!(
            session.restore_v30(&snapshot),
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
    let session = HumanGarrukSessionV30::from_session(base).unwrap();
    let current = expected(&session);
    let actor = current.acting_player;
    let old = HumanDecisionProjectorV9::new(actor)
        .project_current(&old, current)
        .unwrap();
    let prompt = HumanGarrukProjectorV30::new(actor)
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
mod future_printed {
    use super::*;
    use crate::engine;
    use crate::event::{self, ProposedEvent};
    use crate::mana::ManaColor;
    use crate::state::{GameState, Step};
    fn pass_until(session: &mut HumanGarrukSessionV30, done: impl Fn(&GameState) -> bool) {
        for _ in 0..50 {
            if done(session.inner.garruk_fixture_state_mut()) {
                return;
            }
            let current = expected(session);
            let mut projector = HumanGarrukProjectorV30::new(current.acting_player);
            let prompt = projector.project_current(session, current).unwrap();
            projector
                .submit(session, action(&prompt, "Pass priority"))
                .unwrap();
        }
        panic!("actual owning priority traversal did not settle");
    }
    #[test]
    fn actual_human_cast_trample_and_original_draw_public_facts_restore_and_stale_bindings() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = basic(actor, "Island");
            state.step = Step::Main1;
            let source = put(&mut state, actor, "Garruk's Uprising", Zone::Hand);
            let elf = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let large = put(&mut state, actor, "Myr Enforcer", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 9;
            let mut session = HumanGarrukSessionV30::from_session(
                FastActorSessionV1::from_garruk_v30_fixture_state(state),
            )
            .unwrap();
            let mut projector = HumanGarrukProjectorV30::new(actor.into());
            let prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            projector
                .submit(
                    &mut session,
                    action(&prompt, "Begin casting Garruk's Uprising"),
                )
                .unwrap();
            // Genuine spell priority and resolution. The physical engine
            // fixture is owned by this backend, never supplied as fake V9.
            pass_until(&mut session, |state| state.stack.is_empty());
            assert_eq!(
                session
                    .inner
                    .garruk_fixture_state_mut()
                    .objects
                    .get(source)
                    .zone,
                Zone::Battlefield
            );
            let prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            assert!(prompt.state.extensions.active_sources.iter().any(|row| row
                .controlled_creatures_have_trample
                .iter()
                .any(|card| card.name == "Llanowar Elves")));
            assert!(engine::has_effective_keyword(
                session.inner.garruk_fixture_state_mut(),
                elf,
                crate::card_def::Keywords::TRAMPLE
            ));
            assert!(crate::policy_observation_v9::observe_policy_v9(
                session.inner.garruk_fixture_state_mut(),
                &crate::policy_surface_v5::PolicySurfaceV5::new(),
                actor,
                0,
                0,
                0,
                1
            )
            .is_err());
            projector
                .submit(&mut session, action(&prompt, "Begin casting Myr Enforcer"))
                .unwrap();
            pass_until(&mut session, |state| {
                state.objects.get(large).zone == Zone::Battlefield
            });
            let prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            safe(&serde_json::to_value(&prompt).unwrap());
            assert_eq!(prompt.state.extensions.entry_occurrences.len(), 1);
            let snapshot = session.snapshot_v30();
            let request = action(&prompt, "Pass priority");
            projector.submit(&mut session, request.clone()).unwrap();
            session.restore_v30(&snapshot).unwrap();
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
            pass_until(&mut session, |state| state.stack.is_empty());
            let public = session.public_observation(expected(&session)).unwrap();
            assert_eq!(
                public.entry_occurrences[0].lifecycle,
                crate::garruk_public_v1::EntryLifecycleV1::DrawAttempted
            );
            assert_eq!(public.entry_occurrences[0].card_was_drawn, Some(true));
        }
    }
    #[test]
    fn actual_same_source_two_roots_have_distinct_safe_order_labels_and_owning_binding() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = basic(actor, "Island");
            state.step = Step::Main1;
            let source = put(&mut state, actor, "Garruk's Uprising", Zone::Hand);
            let entrant = put(&mut state, actor, "Myr Enforcer", Zone::Hand);
            event::propose_and_commit_batch(
                &mut state,
                vec![
                    ProposedEvent::zone_change(source, Zone::Battlefield),
                    ProposedEvent::zone_change(entrant, Zone::Battlefield),
                ],
            );
            let mut session = HumanGarrukSessionV30::from_session(
                FastActorSessionV1::from_garruk_v30_fixture_state(state),
            )
            .unwrap();
            let mut projector = HumanGarrukProjectorV30::new(actor.into());
            let prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            assert_eq!(prompt.actions.len(), 2);
            assert!(prompt.actions.iter().all(|row| row
                .label
                .contains("still controls a creature with power at least 4")
                && row.label.contains("entry power 4")));
            assert_ne!(prompt.actions[0].label, prompt.actions[1].label);
            safe(&serde_json::to_value(&prompt).unwrap());
            let request = HumanActionRequestV1 {
                prompt_seq: prompt.prompt_seq,
                action_index: 0,
            };
            let receipt = projector.submit(&mut session, request.clone()).unwrap();
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(projector.submit(&mut session, request).unwrap(), receipt);
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
        }
    }
}
