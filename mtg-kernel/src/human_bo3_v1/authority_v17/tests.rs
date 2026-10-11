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
fn expected(session: &HumanAuthoritySessionV17) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(decision) = session.current_response() else {
        panic!("real decision");
    };
    decision
}
fn action(prompt: &HumanDecisionAuthorityV17, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanAuthoritySessionV17::from_session(base).unwrap();
        let current = expected(&session);
        let prior = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&old, current)
            .unwrap();
        let mut projector = HumanAuthorityProjectorV17::new(actor.into());
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
        let hidden = HumanAuthoritySessionV17::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            HumanAuthorityProjectorV17::new(actor.into())
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
        let mut session = HumanAuthoritySessionV17::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Lightning Bolt")),
        )
        .unwrap();
        let mut projector = HumanAuthorityProjectorV17::new(actor.into());
        let first = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let snapshot = session.snapshot_v17();
        let paid = action(&first, "Play Forest");
        projector.submit(&mut session, paid.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_request = action(&future, "Pass priority");
        session.restore_v17(&snapshot).unwrap();
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
        let foreign = HumanAuthoritySessionV17::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            session.restore_v17(&foreign.snapshot_v17()),
            Err(Error::StaleDecision)
        );
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v17(&snapshot),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod printed {
    use super::*;
    use crate::authority_public_v1::{observe_v1, EntryLifecycleV1};
    use crate::engine::{self, Action, Decision};
    use crate::event::{self, CommittedEvent, ProposedEvent};
    use crate::ids::ObjectId;
    use crate::mana::ManaColor;
    use crate::state::{GameState, Step};

    fn next(state: &mut GameState) -> Decision {
        let decision = engine::advance_until_decision(state);
        assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
        decision
    }
    fn settle(state: &mut GameState) {
        for _ in 0..80 {
            match next(state) {
                Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
                Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
                Decision::OrderTriggers { pending, .. } => {
                    engine::step(state, Action::OrderTriggers((0..pending.len()).collect()))
                        .unwrap()
                }
                other => panic!("unexpected real resolution {other:?}"),
            }
        }
        panic!("real resolution did not finish");
    }
    fn cast(state: &mut GameState, caster: PlayerId, name: &str, color: ManaColor) -> ObjectId {
        let source = put(state, caster, name, Zone::Hand);
        state.players[caster.index()].mana_pool[color.pool_index()] = 1;
        assert!(
            matches!(next(state), Decision::CastSpellOrPass { player, .. } if player == caster)
        );
        engine::step(state, Action::CastSpell(source)).unwrap();
        assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
        assert!(state.engine.pending_cast.is_none());
        source
    }
    fn actual_entry(
        actor: PlayerId,
        reverse: bool,
        hidden: &str,
    ) -> (GameState, ObjectId, ObjectId) {
        let forest = crate::card_def::card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_417,
            actor,
        );
        state.step = Step::Main1;
        put(&mut state, actor, hidden, Zone::Hand);
        if reverse {
            put(&mut state, actor, "Mountain", Zone::Battlefield);
        }
        let source = cast(&mut state, actor, "Authority of the Consuls", ManaColor::W);
        settle(&mut state);
        if !reverse {
            put(&mut state, actor, "Mountain", Zone::Battlefield);
        }
        // Traverse an ordinary complete turn, then actually cast the entrant.
        for _ in 0..150 {
            let decision = next(&mut state);
            if state.active_player == actor.opponent()
                && state.step == Step::Main1
                && state.stack.is_empty()
            {
                break;
            }
            match decision {
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                Decision::DeclareAttackers { .. } => {
                    engine::step(&mut state, Action::DeclareAttackers(vec![])).unwrap()
                }
                Decision::DeclareBlockers { .. } => {
                    engine::step(&mut state, Action::DeclareBlockers(vec![])).unwrap()
                }
                other => panic!("unexpected ordinary turn {other:?}"),
            }
        }
        assert_eq!(state.active_player, actor.opponent());
        assert_eq!(state.step, Step::Main1);
        let entrant = cast(&mut state, actor.opponent(), "Llanowar Elves", ManaColor::G);
        for _ in 0..50 {
            let decision = next(&mut state);
            if state.stack.iter().any(|item| {
                matches!(
                    item.inline_effect,
                    Some(crate::effect::EffectOp::AuthorityEntryGainLifeV1 { .. })
                )
            }) {
                assert_eq!(state.objects.get(entrant).zone, Zone::Battlefield);
                assert!(state.objects.get(entrant).tapped);
                return (state, source, entrant);
            }
            match decision {
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                Decision::OrderTriggers { pending, .. } => engine::step(
                    &mut state,
                    Action::OrderTriggers((0..pending.len()).collect()),
                )
                .unwrap(),
                other => panic!("unexpected actual entrant {other:?}"),
            }
        }
        panic!("actual captured gain missing");
    }
    fn observe(
        state: &GameState,
        viewer: PlayerId,
    ) -> crate::authority_public_v1::AuthorityObservationV1 {
        observe_v1(
            state,
            &crate::policy_surface_v5::PolicySurfaceV5::new_for_session(),
            viewer,
            0,
            0,
            0,
            1,
        )
        .unwrap()
    }
    fn transfer(state: &mut GameState, id: ObjectId, controller: PlayerId) {
        let old = state.objects.get(id).controller;
        state.players[old.index()]
            .battlefield
            .retain(|object| *object != id);
        state.players[controller.index()].battlefield.push(id);
        state.objects.get_mut(id).controller = controller;
    }
    fn replay(state: &GameState) -> GameState {
        serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
    }

    #[test]
    fn genuine_authority_entry_refuses_foreign_successor_storage_without_mutation_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, _, _) = actual_entry(actor, false, "Lightning Bolt");
            let surface = crate::policy_surface_v5::PolicySurfaceV5::new_for_session();
            let before = serde_json::to_vec(&state).unwrap();
            for storage in [
                crate::rl::sower_observation_storage_v1,
                crate::rl::frenzied_observation_storage_v1,
                crate::rl::wildwood_observation_storage_v1,
                crate::rl::employment_observation_storage_v1,
                crate::rl::cerberus_observation_storage_v1,
            ] {
                assert!(storage(&state, &surface, actor, 0, 0, (0, 1)).is_err());
                assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            }
            assert!(crate::rl::authority_observation_storage_v1(
                &state,
                &surface,
                actor,
                0,
                0,
                (0, 1),
            )
            .is_ok());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }

    #[test]
    fn actual_entry_original_beneficiary_source_control_departure_hidden_entrant_and_closed_gain() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, source, entrant) = actual_entry(actor, false, "Lightning Bolt");
            let before = observe(&state, actor);
            assert_eq!(before.entry_occurrences.len(), 1);
            assert_eq!(
                before.entry_occurrences[0].lifecycle,
                EntryLifecycleV1::Published
            );
            assert_eq!(
                before.active_sources[0].affected_entry_controller,
                actor.opponent().into()
            );
            assert_eq!(before.roots.len(), 2);
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
            transfer(&mut state, source, actor.opponent());
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(entrant, Zone::Library),
            );
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
            let public = observe(&state, actor);
            let row = &public.entry_occurrences[0];
            assert_eq!(row.beneficiary, actor.into());
            assert_eq!(
                row.entrant_at_entry,
                before.entry_occurrences[0].entrant_at_entry
            );
            assert_eq!(
                row.observer_at_entry,
                before.entry_occurrences[0].observer_at_entry
            );
            assert!(!row.original_source_present);
            assert!(public.active_sources.is_empty());
            let mut restored = replay(&state);
            for current in [&mut state, &mut restored] {
                settle(current);
                assert_eq!(current.players[actor.index()].life, 21);
                assert_eq!(
                    observe(current, actor).entry_occurrences[0].lifecycle,
                    EntryLifecycleV1::Gained
                );
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }

    #[test]
    fn actual_entry_safe_handles_allocation_and_hidden_substitutions_do_not_change_human_prompt() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut prompts = Vec::new();
            for (reverse, hidden) in [(false, "Lightning Bolt"), (true, "Island")] {
                let (state, _, _) = actual_entry(actor, reverse, hidden);
                let session = HumanAuthoritySessionV17::from_session(
                    FastActorSessionV1::from_authority_v17_fixture_state(state),
                )
                .unwrap();
                let current = expected(&session);
                let prompt = HumanAuthorityProjectorV17::new(current.acting_player)
                    .project_current(&session, current)
                    .unwrap();
                safe(&serde_json::to_value(&prompt).unwrap());
                assert_eq!(
                    prompt.state.extensions.entry_occurrences[0].beneficiary,
                    actor.into()
                );
                prompts.push(prompt);
            }
            assert_eq!(prompts[0], prompts[1]);
        }
    }

    #[test]
    fn actual_entry_public_snapshot_erasure_redirection_duplicate_and_mask_forgery_refuse_readonly()
    {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, _, _) = actual_entry(actor, false, "Lightning Bolt");
            let index = state
                .engine
                .event_history
                .iter()
                .position(|event| {
                    matches!(event, CommittedEvent::AuthorityEntryPublicFactsV1 { .. })
                })
                .unwrap();
            for mode in 0..5 {
                let mut corrupted = replay(&state);
                match mode {
                    0 => {
                        corrupted.engine.event_history.remove(index);
                    }
                    1 => {
                        let event = corrupted.engine.event_history[index].clone();
                        corrupted.engine.event_history.push(event);
                    }
                    2 => {
                        let CommittedEvent::AuthorityEntryPublicFactsV1 { capture } =
                            &mut corrupted.engine.event_history[index]
                        else {
                            unreachable!()
                        };
                        capture.observer_authority_index += 1;
                    }
                    3 => {
                        let CommittedEvent::AuthorityEntryPublicFactsV1 { capture } =
                            &mut corrupted.engine.event_history[index]
                        else {
                            unreachable!()
                        };
                        capture.entrant.as_mut().unwrap().card_db_id =
                            crate::card_def::card_id_by_name("Forest").unwrap();
                    }
                    _ => {
                        corrupted
                            .engine
                            .event_log
                            .push(corrupted.engine.event_history[index].clone());
                    }
                }
                corrupted = replay(&corrupted);
                let before = serde_json::to_vec(&corrupted).unwrap();
                assert!(observe_v1(
                    &corrupted,
                    &crate::policy_surface_v5::PolicySurfaceV5::new_for_session(),
                    actor,
                    0,
                    0,
                    0,
                    1
                )
                .is_err());
                assert!(engine::step(&mut corrupted, Action::Pass).is_err());
                assert_eq!(serde_json::to_vec(&corrupted).unwrap(), before);
                let session = FastActorSessionV1::from_authority_v17_fixture_state(corrupted);
                assert!(matches!(
                    session.current_response(),
                    FastActorResponseV1::Terminal(_)
                ));
            }
        }
    }

    #[test]
    fn actual_entry_cached_human_request_refuses_coherent_capture_change_before_mutation() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, _, _) = actual_entry(actor, false, "Lightning Bolt");
            let mut session = HumanAuthoritySessionV17::from_session(
                FastActorSessionV1::from_authority_v17_fixture_state(state),
            )
            .unwrap();
            let current = expected(&session);
            let mut projector = HumanAuthorityProjectorV17::new(current.acting_player);
            let prompt = projector.project_current(&session, current).unwrap();
            let request = action(&prompt, "Pass priority");
            let snapshot = session.snapshot_v17();
            for event in &mut session
                .inner
                .authority_fixture_state_mut()
                .engine
                .event_history
            {
                if let CommittedEvent::AuthorityEntryCaptureV1 { capture } = event {
                    capture.source.controller = capture.source.controller.opponent();
                    break;
                }
            }
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, request.clone()),
                Err(Error::StaleDecision)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            session.restore_v17(&snapshot).unwrap();
            assert_eq!(
                projector.submit(&mut session, request),
                Err(Error::StaleDecision)
            );
            let refreshed = projector
                .project_current(&session, expected(&session))
                .unwrap();
            assert!(refreshed.prompt_seq > prompt.prompt_seq);
            let request = action(&refreshed, "Pass priority");
            let receipt = projector.submit(&mut session, request.clone()).unwrap();
            let after = session.inner.privileged_core_environment_hash();
            assert_eq!(projector.submit(&mut session, request).unwrap(), receipt);
            assert_eq!(session.inner.privileged_core_environment_hash(), after);
        }
    }

    #[test]
    fn actual_entry_uncollected_pending_legend_and_deferred_typed_owner_projections() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, _, _) = actual_entry(actor, false, "Lightning Bolt");
            let extra = put(&mut state, actor.opponent(), "Llanowar Elves", Zone::Hand);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(extra, Zone::Battlefield),
            );
            assert_eq!(
                observe(&state, actor)
                    .entry_occurrences
                    .last()
                    .unwrap()
                    .lifecycle,
                EntryLifecycleV1::Uncollected
            );
            let pending = crate::trigger::capture_triggers_without_sba(&mut state);
            assert_eq!(pending.len(), 1);
            for mode in 0..3 {
                // Typed owner-storage boundary using a genuine committed entry,
                // not a claim of a printed resolution-play/legend producer.
                let mut probe = replay(&state);
                match mode {
                    0 => probe.engine.pending_triggers = pending.clone(),
                    1 => probe.legend_pending_v1 = Some(pending.clone()),
                    _ => {
                        probe
                            .standard_v1
                            .get_or_insert_with(Default::default)
                            .resolution_play =
                            Some(crate::engine::resolution_cast_v1::ResolutionPlayV1 {
                                parent: Default::default(),
                                parent_index: 0,
                                suspended: None,
                                permission: None,
                                deferred_triggers: pending.clone(),
                                kicked_source: None,
                                authorization: None,
                            })
                    }
                }
                let projection = observe(&probe, actor);
                assert_eq!(
                    projection.entry_occurrences.last().unwrap().lifecycle,
                    [
                        EntryLifecycleV1::Pending,
                        EntryLifecycleV1::LegendWaiting,
                        EntryLifecycleV1::ResolutionDeferred
                    ][mode]
                );
                assert_eq!(projection, observe(&replay(&probe), actor));
                if mode == 0 {
                    probe.engine.pending_triggers.clear();
                } else if mode == 1 {
                    probe.legend_pending_v1 = None;
                } else {
                    probe
                        .standard_v1
                        .as_mut()
                        .unwrap()
                        .resolution_play
                        .as_mut()
                        .unwrap()
                        .deferred_triggers
                        .clear();
                }
                let before = serde_json::to_vec(&probe).unwrap();
                assert!(observe_v1(
                    &probe,
                    &crate::policy_surface_v5::PolicySurfaceV5::new_for_session(),
                    actor,
                    0,
                    0,
                    0,
                    1
                )
                .is_err());
                assert!(engine::step(&mut probe, Action::Pass).is_err());
                assert_eq!(serde_json::to_vec(&probe).unwrap(), before);
            }
        }
    }

    #[test]
    fn actual_entry_counter_operation_closes_without_gain_and_reintroduced_live_owner_refuses() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, _, _) = actual_entry(actor, false, "Lightning Bolt");
            let item = state.stack.last().unwrap().clone();
            // Genuine accepted counter primitive, not a claimed printed FDN
            // card which counters triggered abilities.
            engine::counter_stack_item_by_id(&mut state, item.v4.stack_item_id).unwrap();
            assert_eq!(state.players[actor.index()].life, 20);
            assert_eq!(
                observe(&state, actor).entry_occurrences[0].lifecycle,
                EntryLifecycleV1::Countered
            );
            let mut replayed = replay(&state);
            assert_eq!(observe(&state, actor), observe(&replayed, actor));
            replayed.stack.push(item);
            let before = serde_json::to_vec(&replayed).unwrap();
            assert!(observe_v1(
                &replayed,
                &crate::policy_surface_v5::PolicySurfaceV5::new_for_session(),
                actor,
                0,
                0,
                0,
                1
            )
            .is_err());
            assert!(engine::step(&mut replayed, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&replayed).unwrap(), before);
        }
    }

    #[test]
    fn actual_two_entries_same_observer_ordering_labels_bind_each_original_entry() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, _, _) = actual_entry(actor, false, "Lightning Bolt");
            for _ in 0..2 {
                let elf = put(&mut state, actor.opponent(), "Llanowar Elves", Zone::Hand);
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(elf, Zone::Battlefield),
                );
            }
            let pending = crate::trigger::capture_triggers_without_sba(&mut state);
            assert_eq!(pending.len(), 2);
            state.engine.pending_triggers = pending;
            assert!(matches!(next(&mut state), Decision::OrderTriggers { .. }));
            let mut session = HumanAuthoritySessionV17::from_session(
                FastActorSessionV1::from_authority_v17_fixture_state(state),
            )
            .unwrap();
            let current = expected(&session);
            let mut projector = HumanAuthorityProjectorV17::new(current.acting_player);
            let prompt = projector.project_current(&session, current).unwrap();
            safe(&serde_json::to_value(&prompt).unwrap());
            assert_eq!(prompt.actions.len(), 2);
            for action in &prompt.actions {
                assert!(action.label.contains("Gain 1 life"));
                assert!(action.label.contains("entry #2"));
                assert!(action.label.contains("entry #3"));
            }
            let request = HumanActionRequestV1 {
                prompt_seq: prompt.prompt_seq,
                action_index: 0,
            };
            let snapshot = session.snapshot_v17();
            let receipt = projector.submit(&mut session, request.clone()).unwrap();
            let after = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, request.clone()).unwrap(),
                receipt
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), after);
            session.restore_v17(&snapshot).unwrap();
            assert_eq!(
                projector.submit(&mut session, request),
                Err(Error::StaleDecision)
            );
        }
    }
}
