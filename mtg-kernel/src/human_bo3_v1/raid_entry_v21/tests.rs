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
fn expected(session: &HumanRaidEntrySessionV21) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(decision) = session.current_response() else {
        panic!("real decision");
    };
    decision
}
fn action(prompt: &HumanDecisionRaidEntryV21, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanRaidEntrySessionV21::from_session(base).unwrap();
        let current = expected(&session);
        let prior = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&old, current)
            .unwrap();
        let mut projector = HumanRaidEntryProjectorV21::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.actions, prior.actions);
        assert_eq!(prompt.state.public, prior.state.public);
        assert_eq!(prompt.state.extensions.predecessor, prior.state.extensions);
        assert!(prompt.state.extensions.keep_one.is_none());
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let hidden = HumanRaidEntrySessionV21::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            HumanRaidEntryProjectorV21::new(actor.into())
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
        let mut session = HumanRaidEntrySessionV21::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Lightning Bolt")),
        )
        .unwrap();
        let mut projector = HumanRaidEntryProjectorV21::new(actor.into());
        let first = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let snapshot = session.snapshot_v21();
        let paid = action(&first, "Play Forest");
        projector.submit(&mut session, paid.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_request = action(&future, "Pass priority");
        session.restore_v21(&snapshot).unwrap();
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
        let foreign = HumanRaidEntrySessionV21::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            session.restore_v21(&foreign.snapshot_v21()),
            Err(Error::StaleDecision)
        );
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v21(&snapshot),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
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
    use crate::state::{GameObject, GameState, ObjectStateV4, Step};
    fn insert(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
        let card_def = card_id_by_name(name)
            .unwrap_or_else(|| panic!("printed acceptance requires registration: {name}"));
        let id = state.objects.push(GameObject {
            card_def,
            name: name.into(),
            owner,
            controller: owner,
            zone,
            tapped: false,
            summoning_sick: false,
            damage: 0,
            counters: Default::default(),
            attachments: vec![],
            v4: ObjectStateV4::from_card_def(card_def),
            spell_copy_origin: None,
            plotted_turn: None,
            zone_change_count: 0,
        });
        match zone {
            Zone::Hand => state.players[owner.index()].hand.push(id),
            Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
            _ => panic!("fixture zone"),
        }
        id
    }
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
            insert(&mut state, seat, "Mountain", Zone::Battlefield);
            insert(&mut state, seat, "Lightning Bolt", Zone::Hand);
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
        let attacker = insert(state, actor, "Llanowar Elves", Zone::Battlefield);
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
        let source = insert(state, actor, "Gutless Plunderer", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 1;
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

    #[test]
    fn actual_gutless_cast_private_keep_one_context_submit_and_rollback_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor, 5);
            raid(&mut state, actor, true);
            cast(&mut state, actor);
            assert!(
                matches!(settle(&mut state),Some(Decision::ChooseEffectTargets {player,min_targets:2,max_targets:3,..}) if player==actor)
            );
            let original = state.players[actor.index()].library.clone();
            let old = HumanRaidSessionV9::from_session(
                FastActorSessionV1::from_human_v8_fixture_state(state.clone()),
            )
            .unwrap();
            let mut session = HumanRaidEntrySessionV21::from_session(
                FastActorSessionV1::from_raid_entry_v21_fixture_state(state),
            )
            .unwrap();
            let mut projector = HumanRaidEntryProjectorV21::new(actor.into());
            let snapshot = session.snapshot_v21();
            let initial = projector
                .project_current(&session, expected(&session))
                .unwrap();
            let prior = HumanDecisionProjectorV9::new(actor.into())
                .project_current(&old, expected(&session))
                .unwrap();
            assert_eq!(initial.actions, prior.actions);
            safe(&serde_json::to_value(&initial).unwrap());
            assert_eq!(
                initial.state.extensions.keep_one,
                Some(crate::raid_entry_public_v1::KeepOneContextV1 {
                    looked_count: 3,
                    graveyard_min: 2,
                    graveyard_max: 3,
                    retained_max: 1,
                    graveyard_ordered: true
                })
            );
            for _ in 0..2 {
                let prompt = projector
                    .project_current(&session, expected(&session))
                    .unwrap();
                projector
                    .submit(&mut session, action(&prompt, "Select "))
                    .unwrap();
            }
            let prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            let request = action(&prompt, "Finish selection");
            let receipt = projector.submit(&mut session, request.clone()).unwrap();
            assert_eq!(
                session.inner.raid_entry_fixture_state().players[actor.index()]
                    .library
                    .len(),
                original.len() - 2
            );
            assert_eq!(
                session.inner.raid_entry_fixture_state().players[actor.index()]
                    .graveyard
                    .len(),
                2
            );
            let hash = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, request.clone()).unwrap(),
                receipt
            );
            assert_eq!(hash, session.inner.privileged_core_environment_hash());
            session.restore_v21(&snapshot).unwrap();
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, request),
                Err(Error::StaleDecision)
            );
            assert_eq!(before, session.inner.privileged_core_environment_hash());
        }
    }
    #[test]
    fn actual_gutless_erased_choice_and_receipts_cannot_restart_owned_resolution() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor, 5);
            raid(&mut state, actor, true);
            cast(&mut state, actor);
            settle(&mut state).unwrap();
            for erased in [false, true] {
                let mut bad = replay(&state);
                if erased {
                    bad.engine.pending_effect = None;
                    bad.engine.event_history.retain(|row| {
                        !matches!(
                            row,
                            crate::event::CommittedEvent::SquadRallierResolutionBoundaryV1 { .. }
                        )
                    });
                } else {
                    let pending = bad.engine.pending_effect.as_mut().unwrap();
                    pending.choice = None;
                    pending.frames.clear();
                    pending.answered_choice_guard = None;
                }
                let before = serde_json::to_vec(&bad).unwrap();
                assert!(engine::step(&mut bad, Action::Pass).is_err());
                assert_eq!(before, serde_json::to_vec(&bad).unwrap());
            }
        }
    }
    #[test]
    fn actual_gorehorn_cast_targets_physical_partial_ajani_through_distinct_input() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor, 5);
            raid(&mut state, actor, true);
            // Physical recipient setup only: no admitted Ajani cast or loyalty ability.
            let walker = insert(
                &mut state,
                actor.opponent(),
                "Ajani, Caller of the Pride",
                Zone::Hand,
            );
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::zone_change(walker, Zone::Battlefield),
            );
            assert_eq!(crate::planeswalker_v1::loyalty(&state, walker), Some(4));
            let source = insert(&mut state, actor, "Gorehorn Raider", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
            assert!(
                matches!(next(&mut state),Decision::CastSpellOrPass {player,castable_spells,..} if player==actor && castable_spells.contains(&source))
            );
            engine::step(&mut state, Action::CastSpell(source)).unwrap();
            next(&mut state);
            assert!(
                matches!(settle(&mut state),Some(Decision::ChooseTargets {player,legal_targets,..}) if player==actor && legal_targets.contains(&crate::state::Target::Object(walker)))
            );
            let old = HumanRaidSessionV9::from_session(
                FastActorSessionV1::from_human_v8_fixture_state(state.clone()),
            )
            .unwrap();
            let mut session = HumanRaidEntrySessionV21::from_session(
                FastActorSessionV1::from_raid_entry_v21_fixture_state(state),
            )
            .unwrap();
            let current = expected(&session);
            assert!(HumanDecisionProjectorV9::new(actor.into())
                .project_current(&old, current)
                .is_err());
            let mut projector = HumanRaidEntryProjectorV21::new(actor.into());
            let prompt = projector.project_current(&session, current).unwrap();
            safe(&serde_json::to_value(&prompt).unwrap());
            let row = prompt
                .actions
                .iter()
                .find(|row| row.label.contains("Ajani, Caller of the Pride"))
                .unwrap();
            projector
                .submit(
                    &mut session,
                    HumanActionRequestV1 {
                        prompt_seq: prompt.prompt_seq,
                        action_index: row.action_index,
                    },
                )
                .unwrap();
            let snapshot = session.snapshot_v21();
            let mut decisions = 0;
            while crate::planeswalker_v1::loyalty(&session.inner.raid_entry_fixture_state(), walker)
                == Some(4)
                && decisions < 12
            {
                let e = expected(&session);
                let (_, actions, _) = session
                    .inner
                    .raid_entry_current_decision_input_v21(e, e.acting_player)
                    .unwrap();
                let index = actions
                    .iter()
                    .position(|semantic| {
                        matches!(semantic, crate::rl::ActionSemanticV1::Pass { .. })
                    })
                    .unwrap();
                session
                    .inner
                    .step(e.episode_id, e.step, index as u32)
                    .unwrap();
                decisions += 1;
            }
            assert_eq!(
                crate::planeswalker_v1::loyalty(&session.inner.raid_entry_fixture_state(), walker),
                Some(2)
            );
            let first = session.inner.privileged_core_environment_hash();
            session.restore_v21(&snapshot).unwrap();
            decisions = 0;
            while crate::planeswalker_v1::loyalty(&session.inner.raid_entry_fixture_state(), walker)
                == Some(4)
                && decisions < 12
            {
                let e = expected(&session);
                let (_, actions, _) = session
                    .inner
                    .raid_entry_current_decision_input_v21(e, e.acting_player)
                    .unwrap();
                let index = actions
                    .iter()
                    .position(|semantic| {
                        matches!(semantic, crate::rl::ActionSemanticV1::Pass { .. })
                    })
                    .unwrap();
                session
                    .inner
                    .step(e.episode_id, e.step, index as u32)
                    .unwrap();
                decisions += 1;
            }
            assert_eq!(first, session.inner.privileged_core_environment_hash());
        }
    }

    #[test]
    fn actual_gorehorn_historical_source_public_human_full_binding_retry_restore_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor, 5);
            raid(&mut state, actor, true);
            let source = insert(&mut state, actor, "Gorehorn Raider", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
            assert!(
                matches!(next(&mut state),Decision::CastSpellOrPass{player,..} if player==actor)
            );
            engine::step(&mut state, Action::CastSpell(source)).unwrap();
            let _ = next(&mut state);
            assert!(matches!(
                settle(&mut state),
                Some(Decision::ChooseTargets { .. })
            ));
            engine::step(
                &mut state,
                Action::ChooseTarget(crate::state::Target::Player(actor.opponent())),
            )
            .unwrap();
            let _ = next(&mut state);
            crate::effect::execute(
                &crate::effect::EffectOp::GrantKeywordTargetUntilEndOfTurn {
                    object: crate::effect::ObjectRef::ThisSource,
                    keyword: crate::card_def::Keywords::LIFELINK,
                },
                &crate::effect::ExecCtx::no_targets(source, actor),
                &mut state,
            );
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::zone_change(source, Zone::Hand),
            );
            let attack = state.attack_declaration_turn_v1.take();
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::zone_change_to_battlefield_under_controller(
                    source,
                    actor.opponent(),
                ),
            );
            state.attack_declaration_turn_v1 = attack;
            let mut hidden = state.clone();
            insert(&mut hidden, actor.opponent(), "Island", Zone::Hand);
            insert(&mut state, actor.opponent(), "Forest", Zone::Hand);
            let mut session = HumanRaidEntrySessionV21::from_session(
                FastActorSessionV1::from_raid_entry_v21_fixture_state(replay(&state)),
            )
            .unwrap();
            let mut projector = HumanRaidEntryProjectorV21::new(actor.into());
            let current = expected(&session);
            let public = session.public_observation(current).unwrap();
            let [facts] = public.gorehorn_sources.as_slice() else {
                panic!("one real Gorehorn source");
            };
            assert!(facts.historical && facts.lifelink);
            assert_eq!(facts.ability_controller, actor.into());
            assert_eq!(facts.damage_controller, actor.into());
            let prompt = projector.project_current(&session, current).unwrap();
            safe(&serde_json::to_value(&prompt).unwrap());
            let [human] = prompt.state.extensions.gorehorn_sources.as_slice() else {
                panic!("one safe Gorehorn source");
            };
            assert_eq!(human.printed_card_name, "Gorehorn Raider");
            assert!(human.historical && human.lifelink);
            let other = HumanRaidEntrySessionV21::from_session(
                FastActorSessionV1::from_raid_entry_v21_fixture_state(hidden),
            )
            .unwrap();
            assert_eq!(
                prompt,
                HumanRaidEntryProjectorV21::new(actor.into())
                    .project_current(&other, expected(&other))
                    .unwrap()
            );
            let mut forged = public.clone();
            forged.gorehorn_sources[0].damage_controller = actor.opponent().into();
            assert!(forged.validate_artifact().is_err());
            let snapshot = session.snapshot_v21();
            let request = action(&prompt, "Pass");
            let receipt = projector.submit(&mut session, request.clone()).unwrap();
            let hash = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, request.clone()).unwrap(),
                receipt
            );
            assert_eq!(hash, session.inner.privileged_core_environment_hash());
            session.restore_v21(&snapshot).unwrap();
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, request),
                Err(Error::StaleDecision)
            );
            assert_eq!(before, session.inner.privileged_core_environment_hash());
        }
    }
}
