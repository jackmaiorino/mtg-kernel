use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::{GameState, Zone};

fn expected(session: &HumanZombieSessionV13) -> FastActorDecisionV1 {
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
fn owned(state: GameState) -> HumanZombieSessionV13 {
    HumanZombieSessionV13::from_session(FastActorSessionV1::from_human_v8_fixture_state(state))
        .unwrap()
}
fn request(prompt: &HumanDecisionZombieV13, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanZombieSessionV13::from_session(base).unwrap();
        let current = expected(&session);
        let old = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&old_session, current)
            .unwrap();
        let mut projector = HumanZombieProjectorV13::new(actor.into());
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
            HumanZombieProjectorV13::new(actor.opponent().into())
                .project_current(&session, current),
            Err(Error::NotHumanTurn)
        );
        let hidden = owned(basic(actor, "Fireblast"));
        assert_eq!(
            HumanZombieProjectorV13::new(actor.into())
                .project_current(&hidden, expected(&hidden))
                .unwrap(),
            prompt
        );
        let public = session.public_observation(current).unwrap();
        crate::soul_shackled_zombie_public_v1::validate(&public).unwrap();
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
        let snapshot = session.snapshot_v13();
        let mut stale_projector = HumanZombieProjectorV13::new(actor.into());
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
        session.restore_v13(&snapshot).unwrap();
        let mut replay = HumanZombieProjectorV13::new(actor.into());
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
        assert!(crate::soul_shackled_zombie_public_v1::validate(&forged).is_err());
        let mut forged = public;
        forged.schema.push('x');
        assert!(crate::soul_shackled_zombie_public_v1::validate(&forged).is_err());
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
        let mut session = HumanZombieSessionV13::from_session(base).unwrap();
        let e = expected(&session);
        let old = HumanDecisionProjectorV9::new(e.acting_player)
            .project_current(&previous, e)
            .unwrap();
        let mut projector = HumanZombieProjectorV13::new(e.acting_player);
        let prompt = projector.project_current(&session, e).unwrap();
        assert_eq!(prompt.state, old.state);
        assert_eq!(prompt.actions, old.actions);
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        let prefix = if pay { "Pay" } else { "Decline to pay" };
        let input = request(&prompt, prefix);
        let snapshot = session.snapshot_v13();
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v13(&snapshot).unwrap();
        let mut replay = HumanZombieProjectorV13::new(e.acting_player);
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
        let snapshot = session.snapshot_v13();
        let initial = session.inner.privileged_core_environment_hash();
        let mut accepted = HumanZombieProjectorV13::new(actor.into());
        let prompt = accepted
            .project_current(&session, expected(&session))
            .unwrap();
        let input = request(&prompt, "Play Forest");
        let mut pending = HumanZombieProjectorV13::new(actor.into());
        assert_eq!(
            pending
                .project_current(&session, expected(&session))
                .unwrap(),
            prompt
        );
        accepted.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_ne!(initial, after);
        let mut future = HumanZombieProjectorV13::new(actor.into());
        let future_prompt = future
            .project_current(&session, expected(&session))
            .unwrap();
        let future_input = request(&future_prompt, "Begin casting Lightning Bolt");
        session.restore_v13(&snapshot).unwrap();
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
        let mut projector = HumanZombieProjectorV13::new(actor.into());
        let prompt = projector.project_current(&first, expected(&first)).unwrap();
        let before = foreign.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut foreign, request(&prompt, "Play Forest")),
            Err(Error::StaleDecision)
        );
        assert_eq!(foreign.inner.privileged_core_environment_hash(), before);
        let snapshot = foreign.snapshot_v13();
        foreign.authority.generation = u64::MAX;
        assert_eq!(
            foreign.restore_v13(&snapshot),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(foreign.inner.privileged_core_environment_hash(), before);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod printed {
    use super::*;
    use crate::engine::{self, Action, Decision};
    use crate::event::{self, ProposedEvent};
    use crate::ids::ObjectId;
    use crate::mana::ManaColor;
    use crate::state::Target;

    fn to_etb(actor: PlayerId, hidden: &str) -> (GameState, ObjectId, [ObjectId; 3]) {
        let mut state = basic(actor, hidden);
        let mut graveyard = Vec::new();
        for (owner, name) in [
            (actor.opponent(), "Llanowar Elves"),
            (actor.opponent(), "Forest"),
            (actor, "Forest"),
        ] {
            let target = put(&mut state, owner, name, Zone::Hand);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(target, Zone::Graveyard),
            );
            graveyard.push(target);
        }
        let source = put(&mut state, actor, "Soul-Shackled Zombie", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 4;
        engine::advance_until_decision(&mut state);
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        for _ in 0..30 {
            match engine::advance_until_decision(&mut state) {
                Decision::ChooseTargets {
                    spell,
                    can_finish: true,
                    remaining: 2,
                    ..
                } if spell == source => return (state, source, graveyard.try_into().unwrap()),
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                other => panic!("actual cast did not reach Zombie ETB: {other:?}"),
            }
        }
        panic!("actual cast did not resolve");
    }
    fn settle(session: &mut HumanZombieSessionV13) {
        for _ in 0..30 {
            if session.inner.zombie_fixture_state().stack.is_empty() {
                return;
            }
            let e = expected(session);
            let mut p = HumanZombieProjectorV13::new(e.acting_player);
            let prompt = p.project_current(session, e).unwrap();
            p.submit(session, request(&prompt, "Pass priority"))
                .unwrap();
        }
        panic!("actual Human priority did not settle");
    }
    #[test]
    fn real_cast_optional_zero_one_two_shared_graveyard_restore_and_conditional_resolution_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for count in 0..=2 {
                let (state, source, cards) = to_etb(actor, "Fireblast");
                let (hidden, _, _) = to_etb(actor, "Lightning Bolt");
                let mut session = owned(state);
                let hidden = owned(hidden);
                let mut projector = HumanZombieProjectorV13::new(actor.into());
                let initial = projector
                    .project_current(&session, expected(&session))
                    .unwrap();
                assert_safe(&serde_json::to_value(&initial).unwrap());
                assert_eq!(
                    initial,
                    HumanZombieProjectorV13::new(actor.into())
                        .project_current(&hidden, expected(&hidden))
                        .unwrap()
                );
                let public = session.public_observation(expected(&session)).unwrap();
                assert_eq!(public.etb_roots.len(), 1);
                crate::rules_meaning_soul_shackled_zombie_v1::validate(&public.etb_roots[0])
                    .unwrap();
                let context = &public.contexts[0];
                assert_eq!((context.minimum, context.maximum), (0, 2));
                assert!(context.can_finish);
                assert_eq!(context.captured_definition_colors, vec![ManaColor::B]);
                assert!(context.selected.is_empty());
                assert_eq!(context.legal_choices.len(), 3);
                for i in 0..count {
                    let prompt = projector
                        .project_current(&session, expected(&session))
                        .unwrap();
                    let snapshot = session.snapshot_v13();
                    let before = session.inner.privileged_core_environment_hash();
                    let input = request(
                        &prompt,
                        if i == 0 {
                            "Exile Llanowar Elves"
                        } else {
                            "Exile Forest"
                        },
                    );
                    projector.submit(&mut session, input.clone()).unwrap();
                    let announced = session.inner.privileged_core_environment_hash();
                    session.restore_v13(&snapshot).unwrap();
                    assert_eq!(
                        projector.submit(&mut session, input),
                        Err(Error::StaleDecision)
                    );
                    assert_eq!(session.inner.privileged_core_environment_hash(), before);
                    let fresh = projector
                        .project_current(&session, expected(&session))
                        .unwrap();
                    assert!(fresh.prompt_seq > prompt.prompt_seq);
                    projector
                        .submit(
                            &mut session,
                            request(
                                &fresh,
                                if i == 0 {
                                    "Exile Llanowar Elves"
                                } else {
                                    "Exile Forest"
                                },
                            ),
                        )
                        .unwrap();
                    assert_eq!(session.inner.privileged_core_environment_hash(), announced);
                    if i == 0 && count < 2 {
                        let c = session
                            .public_observation(expected(&session))
                            .unwrap()
                            .contexts
                            .remove(0);
                        assert_eq!(c.selected[0].arena_id, cards[0].0);
                        assert_eq!(c.selected_graveyard_owner, Some(actor.opponent().into()));
                        assert_eq!(c.legal_choices.len(), 1);
                        assert_eq!(c.legal_choices[0].arena_id, cards[1].0);
                    }
                }
                if count < 2 {
                    let prompt = projector
                        .project_current(&session, expected(&session))
                        .unwrap();
                    projector
                        .submit(
                            &mut session,
                            request(&prompt, "Finish Soul-Shackled Zombie"),
                        )
                        .unwrap();
                }
                // A restored finalized trigger discloses captured identities and
                // current legality, with no fabricated observation or source root.
                let snapshot = session.snapshot_v13();
                session.restore_v13(&snapshot).unwrap();
                let public = session.public_observation(expected(&session)).unwrap();
                assert_eq!(public.contexts[0].selected.len(), count);
                assert!(!public.contexts[0].can_finish);
                assert_eq!(public.contexts[0].source.arena_id, source.0);
                settle(&mut session);
                let amount = if count == 0 { 0 } else { 2 };
                assert_eq!(
                    session.inner.zombie_fixture_state().players[actor.index()].life,
                    20 + amount
                );
                assert_eq!(
                    session.inner.zombie_fixture_state().players[actor.opponent().index()].life,
                    20 - amount
                );
            }
        }
    }
    #[test]
    fn public_captured_controller_stale_target_and_coherent_forgery_are_bound_read_only() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, source, cards) = to_etb(actor, "Fireblast");
            engine::step(&mut state, Action::ChooseTarget(Target::Object(cards[0]))).unwrap();
            engine::advance_until_decision(&mut state);
            engine::step(&mut state, Action::FinishEffectSelection).unwrap();
            engine::advance_until_decision(&mut state);
            crate::standard_cards_v1::gain_control(&mut state, source, actor.opponent());
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(cards[0], Zone::Hand));
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(cards[0], Zone::Graveyard),
            );
            let session = owned(state);
            let e = expected(&session);
            let mut projector = HumanZombieProjectorV13::new(e.acting_player);
            let prompt = projector.project_current(&session, e).unwrap();
            assert_safe(&serde_json::to_value(&prompt).unwrap());
            let public = session.public_observation(e).unwrap();
            assert_eq!(public.contexts[0].original_controller, actor.into());
            assert_eq!(public.contexts[0].source.controller, actor.into());
            assert_eq!(public.contexts[0].selected_live_legal, vec![false]);
            for corrupt in 0..4 {
                let mut forged = public.clone();
                match corrupt {
                    0 => forged.contexts[0].original_controller = actor.opponent().into(),
                    1 => forged.contexts[0].selected_graveyard_owner = Some(actor.into()),
                    2 => forged.contexts[0].maximum = 3,
                    _ => forged.contexts[0].selected_live_legal.clear(),
                }
                assert!(crate::soul_shackled_zombie_public_v1::validate(&forged).is_err());
            }
            let mut session = session;
            let before = session.inner.privileged_core_environment_hash();
            let item = session
                .inner
                .zombie_fixture_state_mut()
                .stack
                .last_mut()
                .unwrap();
            item.controller = actor.opponent();
            item.v4.ability_source_contract.as_mut().unwrap().controller = actor.opponent();
            let forged_before = session.inner.privileged_core_environment_hash();
            assert_ne!(before, forged_before);
            assert_eq!(
                projector.submit(&mut session, request(&prompt, "Pass priority")),
                Err(Error::StaleDecision)
            );
            assert_eq!(
                session.inner.privileged_core_environment_hash(),
                forged_before
            );
        }
    }

    #[test]
    fn real_noncreature_only_selection_exiles_without_drain_and_changed_prefix_refuses() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, _, cards) = to_etb(actor, "Fireblast");
            engine::step(&mut state, Action::ChooseTarget(Target::Object(cards[1]))).unwrap();
            engine::advance_until_decision(&mut state);
            let mut session = owned(state);
            let mut projector = HumanZombieProjectorV13::new(actor.into());
            let prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            let public = session.public_observation(expected(&session)).unwrap();
            assert_eq!(public.contexts[0].selected[0].arena_id, cards[1].0);
            assert_eq!(public.contexts[0].selected_live_legal, vec![true]);
            let snapshot = session.snapshot_v13();
            // Both target and contract are coherently changed to another legal
            // card in the same physical graveyard. It still changes the binding.
            let replacement = crate::state::StackTargetContractV4::capture(
                session.inner.zombie_fixture_state(),
                Target::Object(cards[0]),
            );
            let pending = session
                .inner
                .zombie_fixture_state_mut()
                .engine
                .pending_triggers
                .first_mut()
                .unwrap();
            pending.targets[0] = Target::Object(cards[0]);
            pending.target_contracts[0] = replacement;
            let forged = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(
                    &mut session,
                    request(&prompt, "Finish Soul-Shackled Zombie")
                ),
                Err(Error::StaleDecision)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), forged);
            session.restore_v13(&snapshot).unwrap();
            let prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            projector
                .submit(
                    &mut session,
                    request(&prompt, "Finish Soul-Shackled Zombie"),
                )
                .unwrap();
            settle(&mut session);
            assert_eq!(
                session
                    .inner
                    .zombie_fixture_state()
                    .objects
                    .get(cards[1])
                    .zone,
                Zone::Exile
            );
            assert_eq!(
                session.inner.zombie_fixture_state().players[actor.index()].life,
                20
            );
            assert_eq!(
                session.inner.zombie_fixture_state().players[actor.opponent().index()].life,
                20
            );
        }
    }
}
