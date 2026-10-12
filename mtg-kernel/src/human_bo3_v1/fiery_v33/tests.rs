use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::{GameState, Zone};

fn expected(session: &HumanFierySessionV33) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("real fixture must reach a decision");
    };
    expected
}
fn owned(state: GameState) -> HumanFierySessionV33 {
    let base = FastActorSessionV1::from_fiery_v33_fixture_state(state);
    let FastActorResponseV1::Decision(expected) = base.current_response() else {
        panic!("actual actor decision");
    };
    HumanFierySessionV33::from_session(base, expected.acting_player).unwrap()
}

fn basic(actor: PlayerId, hidden: &str) -> GameState {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    state
}
fn request(prompt: &HumanDecisionFieryV33, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanFierySessionV33::from_session(base, actor.into()).unwrap();
        let current = expected(&session);
        let old = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&predecessor, current)
            .unwrap();
        let mut projector = HumanFieryProjectorV33::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.actions, old.actions);
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert!(prompt.state.extensions.death_replacements.is_empty());
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let hidden = owned(basic(actor, "Island"));
        assert_eq!(
            HumanFieryProjectorV33::new(actor.into())
                .project_current(&hidden, expected(&hidden))
                .unwrap(),
            prompt
        );
        assert_eq!(
            HumanFieryProjectorV33::new(actor.opponent().into()).project_current(&session, current),
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
        let mut projector = HumanFieryProjectorV33::new(actor.into());
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let saved = session.snapshot_v33();
        let mut stale = HumanFieryProjectorV33::new(actor.into());
        stale.project_current(&session, expected(&session)).unwrap();
        let input = request(&prompt, "Play Forest");
        projector.submit(&mut session, input.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_input = request(&future, "Pass priority");
        session.restore_v33(&saved).unwrap();
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
        let foreign = owned(basic(actor, "Lightning Bolt")).snapshot_v33();
        assert_eq!(session.restore_v33(&foreign), Err(Error::StaleDecision));
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v33(&saved),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
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
        let mut session = HumanFierySessionV33::from_session(base, actor.into()).unwrap();
        let e = expected(&session);
        let old = HumanDecisionProjectorV9::new(e.acting_player)
            .project_current(&previous, e)
            .unwrap();
        let mut projector = HumanFieryProjectorV33::new(e.acting_player);
        let prompt = projector.project_current(&session, e).unwrap();
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert_eq!(prompt.actions, old.actions);
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        let prefix = if pay { "Pay" } else { "Decline to pay" };
        let input = request(&prompt, prefix);
        let snapshot = session.snapshot_v33();
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v33(&snapshot).unwrap();
        let mut replay = HumanFieryProjectorV33::new(e.acting_player);
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
fn fixed_human_seat_has_no_opponent_public_hand_view_and_trusted_callback_binds_real_menu() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let core = FastActorSessionV1::from_fiery_v33_fixture_state(basic(actor, "Lightning Bolt"));
        let mut session =
            HumanFierySessionV33::from_session(core, actor.opponent().into()).unwrap();
        let current = expected(&session);
        assert_eq!(current.acting_player, actor.into());
        assert!(session.public_observation(current).is_err());
        let mut projector = HumanFieryProjectorV33::new(actor.opponent().into());
        assert_eq!(
            projector.project_current(&session, current),
            Err(Error::NotHumanTurn)
        );
        let before = session.inner.privileged_core_environment_hash();
        assert!(session
            .step_opponent_with(current, |_, actions| Ok(actions.len() as u32))
            .is_err());
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        session
            .step_opponent_with(current, |observation, actions| {
                assert_eq!(observation.acting_player, actor.into());
                let index = actions
                    .iter()
                    .position(|a| matches!(a, ActionSemanticV1::Pass { .. }))
                    .unwrap();
                Ok(index as u32)
            })
            .unwrap();
        assert_ne!(session.inner.privileged_core_environment_hash(), before);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod future_printed {
    use super::*;
    use crate::card_def::card_id_by_name;
    use crate::engine::{self, Action, Decision};
    use crate::event::{self, ProposedEvent};
    use crate::ids::ObjectId;
    use crate::mana::ManaColor;
    use crate::state::{GameObject, ObjectStateV4, Step, Target};
    fn ready(player: PlayerId) -> GameState {
        let forest = card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            |_| "Forest".into(),
            11033,
            player,
        );
        state.step = Step::Main1;
        state
    }

    fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
        let card_def =
            card_id_by_name(name).expect("reviewed Fiery Annihilation catalog admission required");
        let id = state.objects.push(GameObject {
            card_def,
            name: name.into(),
            owner: player,
            controller: player,
            zone,
            tapped: false,
            summoning_sick: false,
            damage: 0,
            counters: Default::default(),
            attachments: Vec::new(),
            v4: ObjectStateV4::from_card_def(card_def),
            spell_copy_origin: None,
            plotted_turn: None,
            zone_change_count: 0,
        });
        match zone {
            Zone::Battlefield => state.players[player.index()].battlefield.push(id),
            Zone::Hand => state.players[player.index()].hand.push(id),
            _ => panic!("fixture zone"),
        }
        id
    }

    fn next(state: &mut GameState) -> Decision {
        let decision = engine::advance_until_decision(state);
        assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
        decision
    }

    fn settle(state: &mut GameState) {
        for _ in 0..100 {
            let decision = next(state);
            if matches!(decision, Decision::CastSpellOrPass { .. })
                && state.stack.is_empty()
                && state.engine.pending_triggers.is_empty()
            {
                return;
            }
            let action = match decision {
                Decision::CastSpellOrPass { .. } => Action::Pass,
                Decision::OrderTriggers { pending, .. } => {
                    Action::OrderTriggers((0..pending.len()).collect())
                }
                Decision::ChooseEffectOption {
                    option_count: 5, ..
                } => Action::ChooseEffectOption(3), // Prismatic Strands: red
                other => panic!("unexpected Fiery Annihilation decision: {other:?}"),
            };
            engine::step(state, action).unwrap();
        }
        panic!("Fiery Annihilation resolution did not settle");
    }

    fn copy(state: &GameState) -> GameState {
        serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
    }

    fn equipped(
        caster: PlayerId,
        owner: PlayerId,
        survives: bool,
    ) -> (GameState, ObjectId, ObjectId) {
        let mut state = ready(caster);
        let creature = put(&mut state, owner, "Llanowar Elves", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change_to_battlefield_under_controller(creature, caster),
        );
        if survives {
            state.objects.get_mut(creature).counters.plus1_plus1 = 6;
        }
        let equipment = put(&mut state, caster, "Viridian Longbow", Zone::Battlefield);
        state.players[caster.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        engine::step(&mut state, Action::ActivateAbility(equipment, 0)).unwrap();
        assert!(
            matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(creature)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(creature))).unwrap();
        settle(&mut state);
        assert_eq!(
            state.objects.get(equipment).v4.attached_to.unwrap().object,
            creature
        );
        state.players[caster.index()].mana_pool.fill(0);
        (state, creature, equipment)
    }

    fn begin(state: &mut GameState, caster: PlayerId, creature: ObjectId) -> ObjectId {
        let spell = put(state, caster, "Fiery Annihilation", Zone::Hand);
        state.players[caster.index()].mana_pool[ManaColor::R.pool_index()] = 3;
        assert!(
            matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell))
        );
        engine::step(state, Action::CastSpell(spell)).unwrap();
        assert!(
            matches!(next(state), Decision::ChooseTargets { can_finish: false, legal_targets, .. } if legal_targets.contains(&Target::Object(creature)))
        );
        spell
    }

    fn announce(
        state: &mut GameState,
        caster: PlayerId,
        creature: ObjectId,
        equipment: Option<ObjectId>,
    ) -> ObjectId {
        let spell = begin(state, caster, creature);
        engine::step(state, Action::ChooseTarget(Target::Object(creature))).unwrap();
        assert!(matches!(
            next(state),
            Decision::ChooseTargets {
                can_finish: true,
                ..
            }
        ));
        engine::step(
            state,
            equipment
                .map(|equipment| Action::ChooseTarget(Target::Object(equipment)))
                .unwrap_or(Action::FinishEffectSelection),
        )
        .unwrap();
        assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
        assert!(state.stack.iter().any(|item| item.source == spell));
        spell
    }

    fn shield_count(state: &GameState) -> usize {
        state
            .engine
            .active_replacements
            .iter()
            .filter(|replacement| {
                matches!(
                    replacement.kind,
                    event::ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 { .. }
                )
            })
            .count()
    }

    fn pass_until_resolved(
        session: &mut HumanFierySessionV33,
        projector: &mut HumanFieryProjectorV33,
        actor: PlayerId,
        source: ObjectId,
    ) {
        for _ in 0..30 {
            if session
                .inner
                .fiery_fixture_state_mut()
                .objects
                .get(source)
                .zone
                == Zone::Graveyard
            {
                return;
            }
            let e = expected(session);
            if e.acting_player == actor.into() {
                let prompt = projector.project_current(session, e).unwrap();
                projector
                    .submit(session, request(&prompt, "Pass priority"))
                    .unwrap();
            } else {
                session
                    .step_opponent_with(e, |_, actions| {
                        Ok(actions
                            .iter()
                            .position(|a| matches!(a, ActionSemanticV1::Pass { .. }))
                            .unwrap() as u32)
                    })
                    .unwrap();
            }
        }
        panic!("actual Fiery resolution never completed");
    }
    #[test]
    #[ignore = "Fiery unregistered; root owns actual consumer qualification"]
    fn actual_mandatory_creature_optional_equipment_decline_payment_restore_and_replacements_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for optional in [false, true] {
                let (mut state, target, equipment) = equipped(actor, actor.opponent(), true);
                let source = begin(&mut state, actor, target);
                let mut session = owned(copy(&state));
                let mut projector = HumanFieryProjectorV33::new(actor.into());
                let e = expected(&session);
                let public = session.public_observation(e).unwrap();
                assert_eq!(public.roots.len(), 1);
                assert_eq!(public.cast_menu.as_ref().unwrap().remaining, 2);
                let prompt = projector.project_current(&session, e).unwrap();
                assert_safe(&serde_json::to_value(&prompt).unwrap());
                assert_eq!(
                    prompt
                        .state
                        .extensions
                        .pending_targets
                        .as_ref()
                        .unwrap()
                        .phase,
                    crate::fiery_public_v1::CastTargetPhaseV1::MandatoryCreature
                );
                assert!(prompt
                    .actions
                    .iter()
                    .any(|r| r.label.contains("creature target")));
                assert!(!prompt
                    .actions
                    .iter()
                    .any(|r| r.label.starts_with("Finish ")));
                let input = request(&prompt, "Choose Llanowar Elves");
                let saved = session.snapshot_v33();
                let receipt = projector.submit(&mut session, input.clone()).unwrap();
                let after = session.inner.privileged_core_environment_hash();
                assert_eq!(
                    projector.submit(&mut session, input.clone()).unwrap(),
                    receipt
                );
                assert_eq!(session.inner.privileged_core_environment_hash(), after);
                let optional_prompt = projector
                    .project_current(&session, expected(&session))
                    .unwrap();
                let menu = optional_prompt
                    .state
                    .extensions
                    .pending_targets
                    .as_ref()
                    .unwrap();
                assert_eq!(
                    menu.phase,
                    crate::fiery_public_v1::CastTargetPhaseV1::OptionalAttachedEquipment
                );
                assert_eq!(menu.selected.len(), 1);
                assert!(menu.can_finish_without_equipment);
                assert_eq!(menu.legal.len(), 1);
                session.restore_v33(&saved).unwrap();
                let before = session.inner.privileged_core_environment_hash();
                assert_eq!(
                    projector.submit(&mut session, input),
                    Err(Error::StaleDecision)
                );
                assert_eq!(session.inner.privileged_core_environment_hash(), before);
                let mut replay = HumanFieryProjectorV33::new(actor.into());
                let prompt = replay
                    .project_current(&session, expected(&session))
                    .unwrap();
                replay
                    .submit(&mut session, request(&prompt, "Choose Llanowar Elves"))
                    .unwrap();
                let prompt = replay
                    .project_current(&session, expected(&session))
                    .unwrap();
                let input = request(
                    &prompt,
                    if optional {
                        "Choose Viridian Longbow"
                    } else {
                        "Finish Fiery Annihilation"
                    },
                );
                replay.submit(&mut session, input).unwrap();
                pass_until_resolved(&mut session, &mut replay, actor, source);
                let state = session.inner.fiery_fixture_state_mut();
                assert_eq!(state.objects.get(target).damage, 5);
                assert_eq!(
                    state.objects.get(equipment).zone,
                    if optional {
                        Zone::Exile
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(
                    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()],
                    0
                );
                let state = state.clone();
                event::validate_exile_if_dies_replacements(&state).unwrap();
                let public = crate::fiery_observation_v1::observe_v1(
                    &state,
                    &crate::policy_surface_v5::PolicySurfaceV5::new(),
                    actor,
                    0,
                    0,
                    0,
                    1,
                )
                .unwrap();
                assert_eq!(public.facts.death_replacements.len(), 1);
                assert!(crate::rl::observe_policy_v6(
                    &state,
                    &crate::policy_surface_v5::PolicySurfaceV5::new(),
                    actor,
                    0,
                    0,
                    0,
                    1
                )
                .is_err());
            }
        }
    }
    #[test]
    #[ignore = "Fiery unregistered; root owns actual consumer qualification"]
    fn actual_prevented_damage_original_source_departure_hidden_target_and_real_cleanup_restore() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, target, _) = equipped(actor, actor.opponent(), true);
            // Accepted registered color-prevention primitive, not a fake Fiery root.
            let prevention = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
            event::install_color_damage_prevention(&mut state, prevention, ManaColor::R).unwrap();
            let source = announce(&mut state, actor, target, None);
            settle(&mut state);
            assert_eq!(state.objects.get(target).damage, 0);
            assert_eq!(shield_count(&state), 1);
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(target, Zone::Hand));
            let public = crate::fiery_observation_v1::observe_v1(
                &state,
                &crate::policy_surface_v5::PolicySurfaceV5::new(),
                actor,
                0,
                0,
                0,
                1,
            )
            .unwrap();
            assert!(!public.facts.death_replacements[0].original_target_present);
            let (human, _, _) = project_base(
                &public,
                &[ActionSemanticV1::Pass {
                    actor: actor.into(),
                }],
                actor.into(),
            )
            .unwrap();
            assert!(human.extensions.death_replacements[0]
                .original_target
                .current_visible_card
                .is_none());
            assert_safe(&serde_json::to_value(&human).unwrap());
            // Real End-to-Cleanup traversal, including an actual discard if required.
            state.step = Step::End;
            let mut replay = copy(&state);
            for branch in [&mut state, &mut replay] {
                for _ in 0..40 {
                    if branch.active_player != actor {
                        break;
                    }
                    match next(branch) {
                        Decision::CastSpellOrPass { .. } => {
                            engine::step(branch, Action::Pass).unwrap()
                        }
                        Decision::Discard { count, choices, .. } => engine::step(
                            branch,
                            Action::Discard(choices.into_iter().take(count as usize).collect()),
                        )
                        .unwrap(),
                        other => panic!("real Cleanup {other:?}"),
                    }
                }
                assert_ne!(branch.active_player, actor);
                assert_eq!(shield_count(branch), 0);
                let public = crate::fiery_observation_v1::observe_v1(
                    branch,
                    &crate::policy_surface_v5::PolicySurfaceV5::new(),
                    actor,
                    0,
                    0,
                    0,
                    1,
                )
                .unwrap();
                assert_eq!(
                    public.facts.death_replacements[0].lifecycle,
                    crate::fiery_public_v1::LifecycleV1::ExpiredAtCleanup
                );
                assert!(crate::rl::observe_policy_v6(
                    branch,
                    &crate::policy_surface_v5::PolicySurfaceV5::new(),
                    actor,
                    0,
                    0,
                    0,
                    1
                )
                .is_err());
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
    #[test]
    #[ignore = "Fiery unregistered; root owns actual consumer qualification"]
    fn actual_bound_input_rejects_coherent_pending_rewrite_and_erased_shield_readonly() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, target, _) = equipped(actor, actor, true);
            begin(&mut state, actor, target);
            let mut session = owned(state);
            let mut projector = HumanFieryProjectorV33::new(actor.into());
            let prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            let saved = session.snapshot_v33();
            let pending = session
                .inner
                .fiery_fixture_state_mut()
                .engine
                .pending_cast
                .as_mut()
                .unwrap();
            pending.target_spec = crate::card_def::TargetSpec::None;
            pending.source_contract.card_def = card_id_by_name("Lightning Bolt").unwrap();
            let before = session.inner.privileged_core_environment_hash();
            assert!(projector
                .submit(&mut session, request(&prompt, "Choose Llanowar Elves"))
                .is_err());
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            session.restore_v33(&saved).unwrap();
            let (mut state, target, _) = equipped(actor, actor, true);
            announce(&mut state, actor, target, None);
            settle(&mut state);
            let mut session = owned(state);
            let e = expected(&session);
            let mut projector = HumanFieryProjectorV33::new(e.acting_player);
            let prompt = projector.project_current(&session, e).unwrap();
            session
                .inner
                .fiery_fixture_state_mut()
                .engine
                .active_replacements
                .clear();
            let before = session.inner.privileged_core_environment_hash();
            assert!(projector
                .submit(&mut session, request(&prompt, "Pass priority"))
                .is_err());
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
        }
    }
}
