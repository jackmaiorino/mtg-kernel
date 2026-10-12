use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::{GameState, Zone};

fn expected(session: &HumanStrongboxSessionV32) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("real fixture must reach a decision");
    };
    expected
}
fn owned(state: GameState) -> HumanStrongboxSessionV32 {
    let base = FastActorSessionV1::from_strongbox_v32_fixture_state(state);
    let FastActorResponseV1::Decision(expected) = base.current_response() else {
        panic!("actual actor decision");
    };
    HumanStrongboxSessionV32::from_session(base, expected.acting_player).unwrap()
}

fn basic(actor: PlayerId, hidden: &str) -> GameState {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    state
}
fn request(prompt: &HumanDecisionStrongboxV32, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanStrongboxSessionV32::from_session(base, actor.into()).unwrap();
        let current = expected(&session);
        let old = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&predecessor, current)
            .unwrap();
        let mut projector = HumanStrongboxProjectorV32::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.actions, old.actions);
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert!(prompt.state.extensions.entries.is_empty());
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let hidden = owned(basic(actor, "Island"));
        assert_eq!(
            HumanStrongboxProjectorV32::new(actor.into())
                .project_current(&hidden, expected(&hidden))
                .unwrap(),
            prompt
        );
        assert_eq!(
            HumanStrongboxProjectorV32::new(actor.opponent().into())
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
        let mut projector = HumanStrongboxProjectorV32::new(actor.into());
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let saved = session.snapshot_v32();
        let mut stale = HumanStrongboxProjectorV32::new(actor.into());
        stale.project_current(&session, expected(&session)).unwrap();
        let input = request(&prompt, "Play Forest");
        projector.submit(&mut session, input.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_input = request(&future, "Pass priority");
        session.restore_v32(&saved).unwrap();
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
        let foreign = owned(basic(actor, "Lightning Bolt")).snapshot_v32();
        assert_eq!(session.restore_v32(&foreign), Err(Error::StaleDecision));
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v32(&saved),
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
        let mut session = HumanStrongboxSessionV32::from_session(base, actor.into()).unwrap();
        let e = expected(&session);
        let old = HumanDecisionProjectorV9::new(e.acting_player)
            .project_current(&previous, e)
            .unwrap();
        let mut projector = HumanStrongboxProjectorV32::new(e.acting_player);
        let prompt = projector.project_current(&session, e).unwrap();
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert_eq!(prompt.actions, old.actions);
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        let prefix = if pay { "Pay" } else { "Decline to pay" };
        let input = request(&prompt, prefix);
        let snapshot = session.snapshot_v32();
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v32(&snapshot).unwrap();
        let mut replay = HumanStrongboxProjectorV32::new(e.acting_player);
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
        let core =
            FastActorSessionV1::from_strongbox_v32_fixture_state(basic(actor, "Lightning Bolt"));
        let mut session =
            HumanStrongboxSessionV32::from_session(core, actor.opponent().into()).unwrap();
        let current = expected(&session);
        assert_eq!(current.acting_player, actor.into());
        assert!(session.public_observation(current).is_err());
        let mut projector = HumanStrongboxProjectorV32::new(actor.opponent().into());
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
    use crate::card_def::{card_id_by_name, CARD_DEFS};
    use crate::engine::{self, Action, Decision};
    use crate::ids::ObjectId;
    use crate::mana::ManaColor;
    use crate::state::{Step, Target};
    fn ready(actor: PlayerId, size: usize) -> GameState {
        let cards = ["Forest", "Plains", "Island", "Mountain", "Swamp"]
            .map(|name| card_id_by_name(name).unwrap());
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &cards[..size],
            &cards[..size],
            |id| CARD_DEFS[id as usize].name.into(),
            11006,
            actor,
        );
        state.step = Step::Main1;
        for seat in [actor, actor.opponent()] {
            put(&mut state, seat, "Mountain", Zone::Battlefield);
            put(&mut state, seat, "Lightning Bolt", Zone::Hand);
        }
        state
    }
    fn next(state: &mut GameState) -> Decision {
        let decision = engine::advance_until_decision(state);
        assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
        decision
    }
    fn settle(state: &mut GameState) -> Option<Decision> {
        for _ in 0..40 {
            let decision = next(state);
            match decision {
                Decision::CastSpellOrPass { .. }
                    if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
                {
                    return None
                }
                Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
                Decision::OrderTriggers { pending, .. } => {
                    engine::step(state, Action::OrderTriggers((0..pending.len()).collect()))
                        .unwrap()
                }
                other => return Some(other),
            }
        }
        panic!("resolution did not settle");
    }
    fn raid(state: &mut GameState, actor: PlayerId, nonempty: bool) {
        let attacker = put(state, actor, "Llanowar Elves", Zone::Battlefield);
        state.step = Step::DeclareAttackers;
        assert!(
            matches!(next(state), Decision::DeclareAttackers { player, .. } if player == actor)
        );
        engine::step(
            state,
            Action::DeclareAttackers(if nonempty { vec![attacker] } else { vec![] }),
        )
        .unwrap();
        for _ in 0..40 {
            let decision = next(state);
            if state.step == Step::Main2
                && matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor)
            {
                return;
            }
            match decision {
                Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
                Decision::DeclareBlockers { .. } => {
                    engine::step(state, Action::DeclareBlockers(vec![])).unwrap()
                }
                other => panic!("unexpected combat decision {other:?}"),
            }
        }
        panic!("Main2 did not retain a castable decision");
    }
    fn cast(state: &mut GameState, actor: PlayerId) -> ObjectId {
        let source = put(state, actor, "Strongbox Raider", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 2;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
        assert!(
            matches!(next(state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&source))
        );
        engine::step(state, Action::CastSpell(source)).unwrap();
        assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
        assert!(state.stack.iter().any(|item| item.source == source));
        source
    }
    fn replay(state: &GameState) -> GameState {
        let bytes = serde_json::to_vec(state).unwrap();
        let restored: GameState = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(bytes, serde_json::to_vec(&restored).unwrap());
        assert_eq!(state.state_hash(), restored.state_hash());
        restored
    }

    fn actual_menu(actor: PlayerId) -> (GameState, ObjectId) {
        let mut state = ready(actor, 5);
        raid(&mut state, actor, true);
        let source = cast(&mut state, actor);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectTargets {
                min_targets: 1,
                max_targets: 1,
                ..
            })
        ));
        (state, source)
    }
    #[test]
    #[ignore = "Strongbox definition remains unregistered; root owns qualification"]
    fn actual_exile_choice_safe_labels_private_binding_restore_and_permission_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, source) = actual_menu(actor);
            assert!(crate::rl::observe_policy_v6(
                &state,
                &crate::policy_surface_v5::PolicySurfaceV5::new_for_session(),
                actor,
                0,
                0,
                0,
                1
            )
            .is_err());
            let mut session = owned(replay(&state));
            let e = expected(&session);
            let public = session.public_observation(e).unwrap();
            assert_eq!(public.roots.len(), 1);
            assert_eq!(public.entries.len(), 1);
            let menu = public.selection_menu.as_ref().unwrap();
            assert_eq!(
                (
                    menu.chooser,
                    menu.minimum,
                    menu.maximum,
                    menu.can_finish,
                    menu.legal.len()
                ),
                (actor.into(), 1, 1, false, 2)
            );
            assert_eq!(menu.source_at_entry.arena_id, source.0);
            let mut projector = HumanStrongboxProjectorV32::new(actor.into());
            let prompt = projector.project_current(&session, e).unwrap();
            assert_safe(&serde_json::to_value(&prompt).unwrap());
            assert!(prompt
                .actions
                .iter()
                .all(|row| row.label.starts_with("Choose ")));
            assert!(prompt.actions.iter().all(|row| row
                .label
                .contains("ordinary timing and costs until the end of your next turn")));
            let snapshot = session.snapshot_v32();
            let input = HumanActionRequestV1 {
                prompt_seq: prompt.prompt_seq,
                action_index: 0,
            };
            let receipt = projector.submit(&mut session, input.clone()).unwrap();
            let after = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, input.clone()).unwrap(),
                receipt
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), after);
            let applied = session.public_observation(expected(&session)).unwrap();
            assert!(applied.selection_menu.is_none());
            let permission = applied.entries[0].ordinary_permission.as_ref().unwrap();
            assert_eq!(permission.holder, actor.into());
            assert!(
                permission.play_land
                    && !permission.cast_spell
                    && permission.exact_current_exile
                    && !permission.expired
            );
            assert!(applied.entries[0].exiled.contains(&permission.card));
            session.restore_v32(&snapshot).unwrap();
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, input.clone()),
                Err(Error::StaleDecision)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            let mut replay = HumanStrongboxProjectorV32::new(actor.into());
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
    #[ignore = "Strongbox definition remains unregistered; root owns qualification"]
    fn actual_choice_hidden_substitution_foreign_label_and_corrupt_capture_refuse_readonly() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, _) = actual_menu(actor);
            let mut session = owned(state.clone());
            let mut projector = HumanStrongboxProjectorV32::new(actor.into());
            let prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            let mut hidden = state.clone();
            let foreign = hidden.players[actor.opponent().index()].hand[0];
            let def = card_id_by_name("Island").unwrap();
            let object = hidden.objects.get_mut(foreign);
            object.card_def = def;
            object.name = "Island".into();
            object.v4 = crate::state::ObjectStateV4::from_card_def(def);
            let hidden = owned(hidden);
            assert_eq!(
                HumanStrongboxProjectorV32::new(actor.into())
                    .project_current(&hidden, expected(&hidden))
                    .unwrap(),
                prompt
            );
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(
                    &mut session,
                    HumanActionRequestV1 {
                        prompt_seq: prompt.prompt_seq,
                        action_index: u32::MAX
                    }
                ),
                Err(Error::InvalidAction)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            let damaged = session.inner.strongbox_fixture_state_mut();
            damaged.engine.event_history.retain(|event| {
                !matches!(
                    event,
                    crate::event::CommittedEvent::ExilePlaySelectionCapturedV1 { .. }
                )
            });
            let before = session.inner.privileged_core_environment_hash();
            assert!(projector
                .submit(
                    &mut session,
                    HumanActionRequestV1 {
                        prompt_seq: prompt.prompt_seq,
                        action_index: 0
                    }
                )
                .is_err());
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
        }
    }

    #[test]
    #[ignore = "Strongbox definition remains unregistered; root owns qualification"]
    fn actual_short_library_automatic_choice_and_exiled_spell_uses_real_costs() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for size in [0, 1] {
                let mut state = ready(actor, size);
                raid(&mut state, actor, true);
                cast(&mut state, actor);
                assert!(settle(&mut state).is_none());
                let session = owned(state);
                let public = session.public_observation(expected(&session)).unwrap();
                assert!(public.selection_menu.is_none());
                assert_eq!(public.entries[0].exiled.len(), size);
                assert_eq!(public.entries[0].ordinary_permission.is_some(), size == 1);
            }
            // Real physical library replacement through a new deck, before play.
            let cards = ["Lightning Bolt", "Forest", "Island", "Mountain", "Swamp"]
                .map(|name| card_id_by_name(name).unwrap());
            let mut deck = GameState::new_from_libraries_with_starting_player_v1(
                &cards,
                &cards,
                crate::rl::card_name,
                11032,
                actor,
            );
            deck.step = Step::Main1;
            for seat in [actor, actor.opponent()] {
                put(&mut deck, seat, "Mountain", Zone::Battlefield);
                put(&mut deck, seat, "Lightning Bolt", Zone::Hand);
            }
            let mut state = deck;
            raid(&mut state, actor, true);
            cast(&mut state, actor);
            assert!(matches!(
                settle(&mut state),
                Some(Decision::ChooseEffectTargets { .. })
            ));
            let selected = state.players[actor.index()]
                .exile
                .iter()
                .copied()
                .find(|id| state.objects.get(*id).name == "Lightning Bolt")
                .unwrap();
            engine::step(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(selected)),
            )
            .unwrap();
            assert!(settle(&mut state).is_none());
            assert!(state
                .engine
                .exile_play_permissions
                .iter()
                .all(|permission| !permission.cast_without_paying_mana_cost));
            assert!(
                matches!(next(&mut state),Decision::CastSpellOrPass {castable_spells,..} if !castable_spells.contains(&selected))
            );
            state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
            assert!(
                matches!(next(&mut state),Decision::CastSpellOrPass {player,castable_spells,..} if player==actor && castable_spells.contains(&selected))
            );
            engine::step(&mut state, Action::CastSpell(selected)).unwrap();
            engine::step(
                &mut state,
                Action::ChooseTarget(Target::Player(actor.opponent())),
            )
            .unwrap();
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            assert_eq!(
                state.players[actor.index()].mana_pool[ManaColor::R.pool_index()],
                0
            );
            assert_eq!(state.objects.get(selected).zone, Zone::Stack);
            let life = state.players[actor.opponent().index()].life;
            assert!(settle(&mut state).is_none());
            assert_eq!(state.players[actor.opponent().index()].life, life - 3);
            assert_eq!(state.objects.get(selected).zone, Zone::Graveyard);
            let session = owned(replay(&state));
            let public = session.public_observation(expected(&session)).unwrap();
            let permission = public.entries[0].ordinary_permission.as_ref().unwrap();
            assert!(
                permission.cast_spell && !permission.play_land && !permission.exact_current_exile
            );
            assert_eq!(
                permission.card.card_db_id,
                card_id_by_name("Lightning Bolt").unwrap()
            );
        }
    }
    #[test]
    #[ignore = "Strongbox definition remains unregistered; root owns qualification"]
    fn actual_original_controller_survives_source_transfer_departure_and_unstarted_counter_closes()
    {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, source) = actual_menu(actor);
            crate::standard_cards_v1::gain_control(&mut state, source, actor.opponent());
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::zone_change(source, Zone::Graveyard),
            );
            let mut session = owned(replay(&state));
            let e = expected(&session);
            assert_eq!(e.acting_player, actor.into());
            let mut projector = HumanStrongboxProjectorV32::new(actor.into());
            let prompt = projector.project_current(&session, e).unwrap();
            assert_eq!(
                prompt
                    .state
                    .extensions
                    .selection_menu
                    .as_ref()
                    .unwrap()
                    .chooser,
                actor.into()
            );
            let saved = session.snapshot_v32();
            projector
                .submit(
                    &mut session,
                    HumanActionRequestV1 {
                        prompt_seq: prompt.prompt_seq,
                        action_index: 0,
                    },
                )
                .unwrap();
            let public = session.public_observation(expected(&session)).unwrap();
            assert_eq!(
                public.entries[0]
                    .ordinary_permission
                    .as_ref()
                    .unwrap()
                    .holder,
                actor.into()
            );
            session.restore_v32(&saved).unwrap();
            let mut replay = HumanStrongboxProjectorV32::new(actor.into());
            assert_eq!(
                replay
                    .project_current(&session, expected(&session))
                    .unwrap(),
                prompt
            );
            let mut state = ready(actor, 5);
            raid(&mut state, actor, true);
            let source = cast(&mut state, actor);
            let stack_id = loop {
                let decision = next(&mut state);
                if let Some(row) = state.stack.iter().find(|row| {
                    row.source == source
                        && row.kind == crate::state::StackItemKind::TriggeredAbility
                }) {
                    break row.v4.stack_item_id;
                }
                assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
                engine::step(&mut state, Action::Pass).unwrap();
            };
            let removed = engine::counter_stack_item_by_id(&mut state, stack_id)
                .unwrap()
                .unwrap();
            let closed = replay(&state);
            assert!(crate::raid_entry_v1::validate_state(&closed).is_ok());
            assert!(closed.engine.exile_play_permissions.is_empty());
            let mut damaged = closed.clone();
            damaged.stack.push(removed);
            let before = serde_json::to_vec(&damaged).unwrap();
            assert!(engine::step(&mut damaged, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&damaged).unwrap(), before);
        }
    }
}
