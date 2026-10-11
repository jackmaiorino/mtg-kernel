use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::{GameState, Zone};

fn expected(session: &HumanHeartfireSessionV12) -> FastActorDecisionV1 {
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
fn owned(state: GameState) -> HumanHeartfireSessionV12 {
    HumanHeartfireSessionV12::from_session(FastActorSessionV1::from_human_v8_fixture_state(state))
        .unwrap()
}
fn request(prompt: &HumanDecisionHeartfireV12, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanHeartfireSessionV12::from_session(base).unwrap();
        let current = expected(&session);
        let old = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&old_session, current)
            .unwrap();
        let mut projector = HumanHeartfireProjectorV12::new(actor.into());
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
            HumanHeartfireProjectorV12::new(actor.opponent().into())
                .project_current(&session, current),
            Err(Error::NotHumanTurn)
        );
        let hidden = owned(basic(actor, "Fireblast"));
        assert_eq!(
            HumanHeartfireProjectorV12::new(actor.into())
                .project_current(&hidden, expected(&hidden))
                .unwrap(),
            prompt
        );
        let public = session.public_observation(current).unwrap();
        crate::heartfire_public_v1::validate(&public).unwrap();
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
        let snapshot = session.snapshot_v12();
        let mut stale_projector = HumanHeartfireProjectorV12::new(actor.into());
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
        session.restore_v12(&snapshot).unwrap();
        let mut replay = HumanHeartfireProjectorV12::new(actor.into());
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
        assert!(crate::heartfire_public_v1::validate(&forged).is_err());
        let mut forged = public;
        forged.schema.push('x');
        assert!(crate::heartfire_public_v1::validate(&forged).is_err());
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
        let base = FastActorSessionV1::from_human_v8_fixture_state(state);
        let previous = HumanRaidSessionV9::from_session(base.clone()).unwrap();
        let mut session = HumanHeartfireSessionV12::from_session(base).unwrap();
        let e = expected(&session);
        let old = HumanDecisionProjectorV9::new(e.acting_player)
            .project_current(&previous, e)
            .unwrap();
        let mut projector = HumanHeartfireProjectorV12::new(e.acting_player);
        let prompt = projector.project_current(&session, e).unwrap();
        assert_eq!(predecessor_state(&prompt), old.state);
        assert_eq!(prompt.actions, old.actions);
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        let prefix = if pay { "Pay" } else { "Decline to pay" };
        let input = request(&prompt, prefix);
        let snapshot = session.snapshot_v12();
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v12(&snapshot).unwrap();
        let mut replay = HumanHeartfireProjectorV12::new(e.acting_player);
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
        let snapshot = session.snapshot_v12();
        let initial = session.inner.privileged_core_environment_hash();
        let mut accepted = HumanHeartfireProjectorV12::new(actor.into());
        let prompt = accepted
            .project_current(&session, expected(&session))
            .unwrap();
        let input = request(&prompt, "Play Forest");
        let mut pending = HumanHeartfireProjectorV12::new(actor.into());
        assert_eq!(
            pending
                .project_current(&session, expected(&session))
                .unwrap(),
            prompt
        );
        accepted.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_ne!(initial, after);
        let mut future = HumanHeartfireProjectorV12::new(actor.into());
        let future_prompt = future
            .project_current(&session, expected(&session))
            .unwrap();
        let future_input = request(&future_prompt, "Begin casting Lightning Bolt");
        session.restore_v12(&snapshot).unwrap();
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
        let mut projector = HumanHeartfireProjectorV12::new(actor.into());
        let prompt = projector.project_current(&first, expected(&first)).unwrap();
        let before = foreign.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut foreign, request(&prompt, "Play Forest")),
            Err(Error::StaleDecision)
        );
        assert_eq!(foreign.inner.privileged_core_environment_hash(), before);
        let snapshot = foreign.snapshot_v12();
        foreign.authority.generation = u64::MAX;
        assert_eq!(
            foreign.restore_v12(&snapshot),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(foreign.inner.privileged_core_environment_hash(), before);
    }
}

fn predecessor_state(prompt: &HumanDecisionHeartfireV12) -> HumanVisibleStateV1<HumanExtensionsV9> {
    let state = &prompt.state;
    HumanVisibleStateV1 {
        public: state.public.clone(),
        own_hand: state.own_hand.clone(),
        known_library_cards: state.known_library_cards.clone(),
        known_hand_cards: state.known_hand_cards.clone(),
        extensions: state.extensions.predecessor.clone(),
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
mod printed {
    use super::*;
    use crate::card_def::{card_id_by_name, CARD_DEFS};
    use crate::engine::{self, Action, Decision};
    use crate::event::{self, ProposedEvent};
    use crate::ids::ObjectId;
    use crate::mana::ManaColor;
    use crate::state::Target;

    fn ready(actor: PlayerId) -> GameState {
        let forest = card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            |id| CARD_DEFS[id as usize].object_name.into(),
            110_312,
            actor,
        );
        state.step = crate::state::Step::Main1;
        put(&mut state, actor, "Forest", Zone::Hand);
        put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        put(&mut state, actor, "Mountain", Zone::Battlefield);
        put(&mut state, actor.opponent(), "Fireblast", Zone::Hand);
        // The inherited surface force-passes the activator's fresh own item.
        // Keep a real opponent response so its authenticated stack facts can
        // be inspected before both players pass and resolve it.
        put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
        state.players[actor.opponent().index()].mana_pool[ManaColor::R.pool_index()] = 1;
        state
    }
    fn settle_raw(state: &mut GameState) {
        for _ in 0..50 {
            match engine::advance_until_decision(state) {
                Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
                Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
                other => panic!("actual fixture resolution {other:?}"),
            }
        }
        panic!("actual stack did not settle");
    }
    fn cast(state: &mut GameState, actor: PlayerId) -> ObjectId {
        let source = put(state, actor, "Heartfire Immolator", Zone::Hand);
        state.active_player = actor;
        state.priority_player = actor;
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 2;
        assert!(matches!(
            engine::advance_until_decision(state),
            Decision::CastSpellOrPass { .. }
        ));
        engine::step(state, Action::CastSpell(source)).unwrap();
        settle_raw(state);
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
        source
    }
    fn real_prowess(state: &mut GameState, actor: PlayerId, source: ObjectId) {
        let spell = put(state, actor, "Think Twice", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
        engine::step(state, Action::CastSpell(spell)).unwrap();
        settle_raw(state);
        assert_eq!(engine::effective_power(state, source), 3);
    }
    fn resolve_human(
        session: &mut HumanHeartfireSessionV12,
    ) -> crate::heartfire_public_v1::HeartfirePublicObservationV1 {
        for _ in 0..40 {
            let e = expected(session);
            let public = session.public_observation(e).unwrap();
            if public.predecessor.projection.surface.stack.is_empty() {
                return public;
            }
            let mut projector = HumanHeartfireProjectorV12::new(e.acting_player);
            let prompt = projector.project_current(session, e).unwrap();
            assert_safe(&serde_json::to_value(&prompt).unwrap());
            projector
                .submit(session, request(&prompt, "Pass priority"))
                .unwrap();
        }
        panic!("Human actual stack did not settle");
    }

    #[test]
    fn future_actual_cast_prowess_sacrifice_and_creature_or_partial_walker_damage_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for walker in [false, true] {
                for negative in [false, true] {
                    let mut state = ready(actor);
                    let source = cast(&mut state, actor);
                    real_prowess(&mut state, actor, source);
                    if negative {
                        let generation = state.objects.get(source).zone_change_count;
                        // Existing typed boost primitive, not a fabricated printed producer.
                        crate::effect::install_temporary_boost(
                            &mut state,
                            crate::effect::EffectObjectBinding {
                                object: source,
                                expected_zone: Zone::Battlefield,
                                expected_zone_change_count: generation,
                            },
                            -5,
                            0,
                            crate::card_def::Keywords::NONE,
                        );
                    }
                    let power = engine::effective_power(&state, source);
                    assert_eq!(power, if negative { -2 } else { 3 });
                    let target_name = if walker {
                        "Ajani, Caller of the Pride"
                    } else {
                        "Shivan Dragon"
                    };
                    // Registered Partial Ajani supplies only a physical recipient.
                    // No claim is made for its printed loyalty abilities/combat.
                    let target = if walker {
                        let target = put(&mut state, actor.opponent(), target_name, Zone::Hand);
                        event::propose_and_commit(
                            &mut state,
                            ProposedEvent::zone_change(target, Zone::Battlefield),
                        );
                        assert_eq!(crate::planeswalker_v1::loyalty(&state, target), Some(4));
                        target
                    } else {
                        put(&mut state, actor.opponent(), target_name, Zone::Battlefield)
                    };
                    let color = engine::object_color_mask(&state, source);
                    // Leave one mana for the real Bolt response after the
                    // activation cost, preserving an observable priority menu.
                    // A pass-only fast-session menu would resolve the ability
                    // before this fixture inspects its saved stack damage.
                    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 2;
                    let base = FastActorSessionV1::from_human_v8_fixture_state(state);
                    let predecessor = HumanRaidSessionV9::from_session(base.clone()).unwrap();
                    let mut session = HumanHeartfireSessionV12::from_session(base).unwrap();
                    let e = expected(&session);
                    assert_eq!(
                        HumanDecisionProjectorV9::new(actor.into())
                            .project_current(&predecessor, e),
                        Err(Error::UnsupportedPrompt)
                    );
                    let mut projector = HumanHeartfireProjectorV12::new(actor.into());
                    let prompt = projector.project_current(&session, e).unwrap();
                    assert_safe(&serde_json::to_value(&prompt).unwrap());
                    projector
                        .submit(
                            &mut session,
                            request(&prompt, "Activate Heartfire Immolator"),
                        )
                        .unwrap();
                    let mut selector = HumanHeartfireProjectorV12::new(actor.into());
                    let target_prompt = selector
                        .project_current(&session, expected(&session))
                        .unwrap();
                    assert!(!target_prompt
                        .actions
                        .iter()
                        .any(|row| row.label.contains("Finish")));
                    assert!(target_prompt.state.extensions.saved_damage.is_empty());
                    let selection_snapshot = session.snapshot_v12();
                    let input = request(&target_prompt, &format!("Target {target_name}"));
                    selector.submit(&mut session, input.clone()).unwrap();
                    let announced_hash = session.inner.privileged_core_environment_hash();
                    let announced = session.public_observation(expected(&session)).unwrap();
                    assert_eq!(
                        announced.saved_damage.len(),
                        1,
                        "actor={actor:?} walker={walker} negative={negative} current={:?}",
                        expected(&session)
                    );
                    let saved = &announced.saved_damage[0];
                    assert_eq!(saved.signed_departing_power, power);
                    assert_eq!(saved.damage_if_target_legal_before_prevention, power.max(0));
                    assert_eq!(saved.captured_controller, actor.into());
                    assert_eq!(saved.captured_color_mask, color);
                    assert!(
                        announced.predecessor.projection.surface.graveyards[actor.index()]
                            .iter()
                            .any(|row| row.stable.arena_id == source.0)
                    );
                    let mut responder =
                        HumanHeartfireProjectorV12::new(expected(&session).acting_player);
                    let pending = responder
                        .project_current(&session, expected(&session))
                        .unwrap();
                    assert_safe(&serde_json::to_value(&pending).unwrap());
                    assert_eq!(pending.state.extensions.saved_damage.len(), 1);
                    assert_eq!(
                        pending.state.extensions.saved_damage[0].source.name,
                        "Heartfire Immolator"
                    );
                    let announced_snapshot = session.snapshot_v12();
                    let final_public = resolve_human(&mut session);
                    assert!(final_public.saved_damage.is_empty());
                    if walker {
                        let target_row = final_public
                            .predecessor
                            .projection
                            .surface
                            .engine_context
                            .planeswalkers
                            .as_ref()
                            .unwrap()
                            .iter()
                            .find(|row| row.permanent.arena_id == target.0)
                            .unwrap();
                        assert_eq!(target_row.loyalty, 4 - power.max(0) as u32);
                    } else {
                        let target_row = final_public.predecessor.projection.surface.battlefield
                            [actor.opponent().index()]
                        .iter()
                        .find(|row| row.stable.arena_id == target.0)
                        .unwrap();
                        assert_eq!(i32::from(target_row.damage), power.max(0));
                    }
                    let final_hash = session.inner.privileged_core_environment_hash();
                    session.restore_v12(&announced_snapshot).unwrap();
                    assert_eq!(
                        session.inner.privileged_core_environment_hash(),
                        announced_hash
                    );
                    assert_eq!(
                        session.public_observation(expected(&session)).unwrap(),
                        announced
                    );
                    resolve_human(&mut session);
                    assert_eq!(session.inner.privileged_core_environment_hash(), final_hash);
                    session.restore_v12(&selection_snapshot).unwrap();
                    let before = session.inner.privileged_core_environment_hash();
                    assert_eq!(
                        selector.submit(&mut session, input),
                        Err(Error::StaleDecision)
                    );
                    assert_eq!(session.inner.privileged_core_environment_hash(), before);
                }
            }
        }
    }

    #[test]
    fn future_actual_activation_saved_controller_keywords_and_color_survive_source_reentry() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let source = cast(&mut state, actor.opponent());
            // Physical control/keyword primitives exercise the actual printed
            // activation. They do not assert a fabricated control/grant producer.
            state.players[actor.opponent().index()]
                .battlefield
                .retain(|id| *id != source);
            state.players[actor.index()].battlefield.push(source);
            state.objects.get_mut(source).controller = actor;
            state.objects.get_mut(source).v4.lifelink_keyword_counters = 1;
            let generation = state.objects.get(source).zone_change_count;
            state.engine.until_end_of_turn.push(
                engine::UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
                    object_id: source,
                    object_zone_change_count: generation,
                    keywords: crate::card_def::Keywords::DEATHTOUCH,
                    layer: engine::Layers::ABILITY_ADDING,
                    timestamp: 111,
                    duration: engine::EffectDuration::EndOfTurn,
                },
            );
            let target = put(
                &mut state,
                actor.opponent(),
                "Shivan Dragon",
                Zone::Battlefield,
            );
            state.active_player = actor;
            state.priority_player = actor;
            state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
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
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
            state.objects.get_mut(source).v4.effective_color_mask = 0;
            state.objects.get_mut(source).v4.lifelink_keyword_counters = 0;
            let base = crate::policy_observation_v9::observe_policy_v9(
                &state,
                &crate::policy_surface_v5::PolicySurfaceV5::new(),
                actor,
                0,
                0,
                0,
                1,
            )
            .unwrap();
            let original_hash = state.state_hash();
            let original =
                crate::heartfire_public_v1::from_authenticated_v9(&state, base.clone()).unwrap();
            assert_eq!(state.state_hash(), original_hash);
            let saved = &original.saved_damage[0];
            assert_eq!(saved.captured_controller, actor.into());
            assert!(
                saved.captured_lifelink
                    && saved.captured_deathtouch
                    && saved.captured_color_mask != 0
            );
            // The public companion refuses corrupted committed power before
            // projection, using the independent production source/cache validator.
            let mut forged = state.clone();
            for event in &mut forged.engine.event_history {
                if let crate::event::CommittedEvent::SourcePowerActivationCommittedV1 {
                    power,
                    ..
                } = event
                {
                    *power += 1;
                }
            }
            let forged_hash = forged.state_hash();
            assert!(crate::heartfire_public_v1::from_authenticated_v9(&forged, base).is_err());
            assert_eq!(forged.state_hash(), forged_hash);
            let mut session = owned(state);
            let mut projector = HumanHeartfireProjectorV12::new(actor.into());
            let prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            assert_safe(&serde_json::to_value(&prompt).unwrap());
            assert!(prompt.state.extensions.saved_damage[0].captured_lifelink);
            assert!(prompt.state.extensions.saved_damage[0].captured_deathtouch);
            let final_public = resolve_human(&mut session);
            assert_eq!(
                final_public.predecessor.projection.surface.life_totals[actor.index()],
                22
            );
            assert_eq!(
                final_public.predecessor.projection.surface.life_totals[actor.opponent().index()],
                20
            );
            assert!(final_public.predecessor.projection.surface.graveyards
                [actor.opponent().index()]
            .iter()
            .any(|row| row.stable.arena_id == target.0));
        }
    }
}
