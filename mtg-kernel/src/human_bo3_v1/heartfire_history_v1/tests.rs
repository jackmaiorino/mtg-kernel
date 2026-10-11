use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::{GameState, Zone};

fn expected(session: &HumanHeartfireHistorySessionV1) -> FastActorDecisionV1 {
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
fn owned(state: GameState) -> HumanHeartfireHistorySessionV1 {
    HumanHeartfireHistorySessionV1::from_session(
        FastActorSessionV1::from_heartfire_history_fixture_state(state),
    )
    .unwrap()
}
fn request(prompt: &HumanDecisionHeartfireHistoryV1, prefix: &str) -> HumanActionRequestV1 {
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
        let base = FastActorSessionV1::from_heartfire_history_fixture_state(basic(
            actor,
            "Lightning Bolt",
        ));
        let mut old_session = HumanRaidSessionV9::from_session(base.clone()).unwrap();
        let mut session = HumanHeartfireHistorySessionV1::from_session(base).unwrap();
        let current = expected(&session);
        let old = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&old_session, current)
            .unwrap();
        let mut projector = HumanHeartfireHistoryProjectorV1::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(old.actions, prompt.actions);
        assert_eq!(old.state, predecessor_state(&prompt));
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        assert_eq!(
            HumanHeartfireHistoryProjectorV1::new(actor.opponent().into())
                .project_current(&session, current),
            Err(Error::NotHumanTurn)
        );
        let hidden = owned(basic(actor, "Fireblast"));
        assert_eq!(
            HumanHeartfireHistoryProjectorV1::new(actor.into())
                .project_current(&hidden, expected(&hidden))
                .unwrap(),
            prompt
        );
        let public = session.public_observation(current).unwrap();
        crate::heartfire_history_v1::validate(&public).unwrap();
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
        let snapshot = session.snapshot_v1();
        let mut stale_projector = HumanHeartfireHistoryProjectorV1::new(actor.into());
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
        session.restore_v1(&snapshot).unwrap();
        let mut replay = HumanHeartfireHistoryProjectorV1::new(actor.into());
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
        forged.source_sha256.push('x');
        assert!(crate::heartfire_history_v1::validate(&forged).is_err());
        let mut forged = public;
        forged.schema.push('x');
        assert!(crate::heartfire_history_v1::validate(&forged).is_err());
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
fn ward_state(actor: PlayerId) -> GameState {
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
        let base = FastActorSessionV1::from_heartfire_history_fixture_state(state);
        let previous = HumanRaidSessionV9::from_session(base.clone()).unwrap();
        let mut session = HumanHeartfireHistorySessionV1::from_session(base).unwrap();
        let e = expected(&session);
        let old = HumanDecisionProjectorV9::new(e.acting_player)
            .project_current(&previous, e)
            .unwrap();
        let mut projector = HumanHeartfireHistoryProjectorV1::new(e.acting_player);
        let prompt = projector.project_current(&session, e).unwrap();
        assert_eq!(predecessor_state(&prompt), old.state);
        assert_eq!(prompt.actions, old.actions);
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        let prefix = if pay { "Pay" } else { "Decline to pay" };
        let input = request(&prompt, prefix);
        let snapshot = session.snapshot_v1();
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v1(&snapshot).unwrap();
        let mut replay = HumanHeartfireHistoryProjectorV1::new(e.acting_player);
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
        let snapshot = session.snapshot_v1();
        let initial = session.inner.privileged_core_environment_hash();
        let mut accepted = HumanHeartfireHistoryProjectorV1::new(actor.into());
        let prompt = accepted
            .project_current(&session, expected(&session))
            .unwrap();
        let input = request(&prompt, "Play Forest");
        let mut pending = HumanHeartfireHistoryProjectorV1::new(actor.into());
        assert_eq!(
            pending
                .project_current(&session, expected(&session))
                .unwrap(),
            prompt
        );
        accepted.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_ne!(initial, after);
        let mut future = HumanHeartfireHistoryProjectorV1::new(actor.into());
        let future_prompt = future
            .project_current(&session, expected(&session))
            .unwrap();
        let future_input = request(&future_prompt, "Begin casting Lightning Bolt");
        session.restore_v1(&snapshot).unwrap();
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
        let mut projector = HumanHeartfireHistoryProjectorV1::new(actor.into());
        let prompt = projector.project_current(&first, expected(&first)).unwrap();
        let before = foreign.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut foreign, request(&prompt, "Play Forest")),
            Err(Error::StaleDecision)
        );
        assert_eq!(foreign.inner.privileged_core_environment_hash(), before);
        let snapshot = foreign.snapshot_v1();
        foreign.authority.generation = u64::MAX;
        assert_eq!(
            foreign.restore_v1(&snapshot),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(foreign.inner.privileged_core_environment_hash(), before);
    }
}

fn predecessor_state(
    prompt: &HumanDecisionHeartfireHistoryV1,
) -> HumanVisibleStateV1<HumanExtensionsV9> {
    let state = &prompt.state;
    HumanVisibleStateV1 {
        public: state.public.clone(),
        own_hand: state.own_hand.clone(),
        known_library_cards: state.known_library_cards.clone(),
        known_hand_cards: state.known_hand_cards.clone(),
        extensions: state.extensions.common.clone(),
        policy_context: state.policy_context.clone(),
        known_face_down_cards: state.known_face_down_cards.clone(),
        poison_counters: state.poison_counters,
        poison_prevention: state.poison_prevention,
        creatures_attacked_this_turn: state.creatures_attacked_this_turn,
        ninja_emblems: state.ninja_emblems,
        restricted_mana: state.restricted_mana.clone(),
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod copied_printed {
    use super::*;
    use crate::effect::{EffectObjectBinding, ExecCtx};
    use crate::engine::{self, Action, Decision};
    use crate::event::{self, ProposedEvent};
    use crate::ids::ObjectId;
    use crate::mana::ManaColor;
    use crate::state::{AbilitySourceContractV4, ObjectLinkV4, Target};

    fn copied(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
        let mut state = basic(actor, "Fireblast");
        let donor = put(&mut state, actor, "Heartfire Immolator", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 2;
        engine::step(&mut state, Action::CastSpell(donor)).unwrap();
        for _ in 0..30 {
            let decision = engine::advance_until_decision(&mut state);
            if matches!(decision, Decision::CastSpellOrPass { .. }) && state.stack.is_empty() {
                break;
            }
            assert!(
                matches!(decision, Decision::CastSpellOrPass { .. }),
                "{decision:?}"
            );
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert_eq!(state.objects.get(donor).zone, Zone::Battlefield);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(donor, Zone::Exile));
        let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let equipment = put(&mut state, actor, "Viridian Longbow", Zone::Battlefield);
        let host = ObjectLinkV4 {
            object: source,
            zone_change_count: state.objects.get(source).zone_change_count,
        };
        // A physical registered Equipment is the stand-in for the typed copy
        // executor. This does not claim a printed Longbow copy ability or a
        // naturally printed mixed-catalog Aegis spell.
        state.objects.get_mut(equipment).v4.attached_to = Some(host);
        state.objects.get_mut(equipment).v4.layer_timestamp = Some(17);
        state.objects.get_mut(source).attachments.push(equipment);
        let source_contract = AbilitySourceContractV4::capture(&state, equipment);
        state.objects.get_mut(donor).v4.exiled_by = Some(ObjectLinkV4 {
            object: equipment,
            zone_change_count: source_contract.zone_change_count,
        });
        let mut context = ExecCtx::no_targets(equipment, actor);
        context.ability_source_contract = Some(source_contract);
        let choice = EffectObjectBinding {
            object: donor,
            expected_zone: Zone::Exile,
            expected_zone_change_count: state.objects.get(donor).zone_change_count,
        };
        assert!(
            crate::standard_cards_v1::aegis_copy_candidates(&state, &context, host, 17)
                .contains(&choice)
        );
        crate::standard_cards_v1::apply_aegis_copy(&mut state, &context, host, 17, choice);
        assert_eq!(
            state.objects.get(source).card_def,
            state.objects.get(donor).card_def
        );
        assert!(crate::standard_cards_v1::had_card_def(
            &state,
            source,
            host.zone_change_count,
            state.objects.get(donor).card_def
        ));
        assert_eq!(engine::effective_power(&state, source), 2);
        let target = put(
            &mut state,
            actor.opponent(),
            "Shivan Dragon",
            Zone::Battlefield,
        );
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        (state, source, target)
    }
    fn announced(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
        let (mut state, source, target) = copied(actor);
        engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::ChooseTargets { .. }
        ));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        assert_eq!(
            state.objects.get(source).card_def,
            crate::card_def::card_id_by_name("Llanowar Elves").unwrap()
        );
        assert!(crate::source_power_activation_v1::validate_state(&state).is_ok());
        (state, source, target)
    }
    fn finish(session: &mut HumanHeartfireHistorySessionV1) -> HeartfireHistoryObservationV1 {
        for _ in 0..40 {
            let current = expected(session);
            let view = session.public_observation(current).unwrap();
            crate::heartfire_history_v1::validate(&view).unwrap();
            if view.projection.surface.stack.is_empty() {
                return view;
            }
            let mut projector = HumanHeartfireHistoryProjectorV1::new(current.acting_player);
            let prompt = projector.project_current(session, current).unwrap();
            assert_safe(&serde_json::to_value(&prompt).unwrap());
            projector
                .submit(session, request(&prompt, "Pass priority"))
                .unwrap();
        }
        panic!("actual copied ability did not resolve");
    }
    #[test]
    fn future_actual_copied_activation_sacrifice_human_restore_retry_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, source, target) = copied(actor);
            let mut session = owned(state);
            let mut announcer = HumanHeartfireHistoryProjectorV1::new(actor.into());
            let prompt = announcer
                .project_current(&session, expected(&session))
                .unwrap();
            announcer
                .submit(
                    &mut session,
                    request(&prompt, "Activate Heartfire Immolator"),
                )
                .unwrap();
            let mut selector = HumanHeartfireHistoryProjectorV1::new(actor.into());
            let prompt = selector
                .project_current(&session, expected(&session))
                .unwrap();
            let before_payment = session.snapshot_v1();
            let input = request(&prompt, "Target Shivan Dragon");
            let receipt = selector.submit(&mut session, input.clone()).unwrap();
            let view = session.public_observation(expected(&session)).unwrap();
            assert_eq!(view.schema, crate::heartfire_history_v1::SCHEMA);
            assert_eq!(view.saved_damage[0].signed_departing_power, 2);
            assert_eq!(
                view.projection.surface.stack[0].source.card_db_id,
                crate::card_def::card_id_by_name("Heartfire Immolator").unwrap()
            );
            assert!(view.projection.surface.graveyards[actor.index()]
                .iter()
                .any(|card| card.stable.arena_id == source.0 && card.name == "Llanowar Elves"));
            let mut responder = HumanHeartfireHistoryProjectorV1::new(actor.into());
            let safe = responder
                .project_current(&session, expected(&session))
                .unwrap();
            assert_safe(&serde_json::to_value(&safe).unwrap());
            assert_eq!(
                safe.state.extensions.saved_damage[0].source.name,
                "Heartfire Immolator"
            );
            let saved_handle = &safe.state.extensions.saved_damage[0].source.handle;
            let grave = safe.state.public.graveyards[actor.index()]
                .iter()
                .find(|card| card.stable.name == "Llanowar Elves")
                .unwrap();
            assert_ne!(saved_handle, &grave.stable.handle);
            let paid = session.inner.privileged_core_environment_hash();
            assert_eq!(
                selector.submit(&mut session, input.clone()).unwrap(),
                receipt
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), paid);
            let paid_snapshot = session.snapshot_v1();
            let final_view = finish(&mut session);
            assert!(final_view.saved_damage.is_empty());
            assert_eq!(
                final_view.projection.surface.battlefield[actor.opponent().index()]
                    .iter()
                    .find(|card| card.stable.arena_id == target.0)
                    .unwrap()
                    .damage,
                2
            );
            let final_hash = session.inner.privileged_core_environment_hash();
            session.restore_v1(&paid_snapshot).unwrap();
            assert_eq!(
                session.public_observation(expected(&session)).unwrap(),
                view
            );
            finish(&mut session);
            assert_eq!(session.inner.privileged_core_environment_hash(), final_hash);
            session.restore_v1(&before_payment).unwrap();
            let unpaid = session.inner.privileged_core_environment_hash();
            assert_eq!(
                selector.submit(&mut session, input),
                Err(Error::StaleDecision)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), unpaid);
        }
    }
    #[test]
    fn future_copy_history_reentry_foreign_hand_privacy_and_forgery_are_read_only() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, source, target) = announced(actor);
            let surface = crate::policy_surface_v5::PolicySurfaceV5::new();
            assert!(crate::policy_observation_v9::observe_policy_v9(
                &state, &surface, actor, 0, 0, 0, 1
            )
            .is_err());
            assert!(crate::rl::observe_policy_v8(&state, &surface, actor, 0, 0, 0, 1).is_err());
            for reentered in [false, true] {
                let mut branch = state.clone();
                if reentered {
                    event::propose_and_commit(
                        &mut branch,
                        ProposedEvent::zone_change(source, Zone::Hand),
                    );
                    event::propose_and_commit(
                        &mut branch,
                        ProposedEvent::zone_change(source, Zone::Battlefield),
                    );
                }
                let hash = branch.state_hash();
                let view =
                    crate::heartfire_history_v1::observe(&branch, &surface, actor, 0, 0, 0, 1)
                        .unwrap();
                crate::heartfire_history_v1::validate(&view).unwrap();
                assert_eq!(branch.state_hash(), hash);
                assert_eq!(
                    view.projection.surface.stack[0].source.card_db_id,
                    crate::card_def::card_id_by_name("Heartfire Immolator").unwrap()
                );
                assert!(
                    view.projection.surface.stack[0].source.zone_change_count
                        < branch.objects.get(source).zone_change_count
                );
                let restored: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&branch).unwrap()).unwrap();
                assert_eq!(
                    crate::heartfire_history_v1::observe(&restored, &surface, actor, 0, 0, 0, 1)
                        .unwrap(),
                    view
                );
                let own = owned(branch.clone());
                let safe = HumanHeartfireHistoryProjectorV1::new(actor.into())
                    .project_current(&own, expected(&own))
                    .unwrap();
                assert_safe(&serde_json::to_value(&safe).unwrap());
                let hidden = branch.players[actor.opponent().index()].hand[0];
                branch.objects.get_mut(hidden).card_def =
                    crate::card_def::card_id_by_name("Lightning Bolt").unwrap();
                branch.objects.get_mut(hidden).name = "Lightning Bolt".into();
                let other = owned(branch);
                assert_eq!(
                    HumanHeartfireHistoryProjectorV1::new(actor.into())
                        .project_current(&other, expected(&other))
                        .unwrap(),
                    safe
                );
            }
            // Removing committed typed copy provenance must refuse the captured root.
            let mut json = serde_json::to_value(&state).unwrap();
            json["standard_v1"]["copied_card_defs"] = serde_json::json!([]);
            let missing: GameState = serde_json::from_value(json).unwrap();
            let hash = missing.state_hash();
            assert!(
                crate::heartfire_history_v1::observe(&missing, &surface, actor, 0, 0, 0, 1)
                    .is_err()
            );
            assert_eq!(missing.state_hash(), hash);
            for event in &mut state.engine.event_history {
                if let crate::event::CommittedEvent::SourcePowerActivationCommittedV1 {
                    power,
                    ..
                } = event
                {
                    *power += 1;
                }
            }
            let hash = state.state_hash();
            assert!(
                crate::heartfire_history_v1::observe(&state, &surface, actor, 0, 0, 0, 1).is_err()
            );
            assert_eq!(state.state_hash(), hash);
            assert_eq!(state.objects.get(target).damage, 0);
        }
    }
}
