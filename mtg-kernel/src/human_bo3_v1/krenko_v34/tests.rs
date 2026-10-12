use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::{GameState, Zone};

fn expected(session: &HumanKrenkoSessionV34) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("real fixture must reach a decision");
    };
    expected
}
fn owned(state: GameState) -> HumanKrenkoSessionV34 {
    let base = FastActorSessionV1::from_krenko_v34_fixture_state(state);
    let FastActorResponseV1::Decision(expected) = base.current_response() else {
        panic!("actual actor decision");
    };
    HumanKrenkoSessionV34::from_session(base, expected.acting_player).unwrap()
}

fn basic(actor: PlayerId, hidden: &str) -> GameState {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    state
}
fn request(prompt: &HumanDecisionKrenkoV34, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanKrenkoSessionV34::from_session(base, actor.into()).unwrap();
        let current = expected(&session);
        let old = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&predecessor, current)
            .unwrap();
        let mut projector = HumanKrenkoProjectorV34::new(actor.into());
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
            HumanKrenkoProjectorV34::new(actor.into())
                .project_current(&hidden, expected(&hidden))
                .unwrap(),
            prompt
        );
        assert_eq!(
            HumanKrenkoProjectorV34::new(actor.opponent().into())
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
        let mut projector = HumanKrenkoProjectorV34::new(actor.into());
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let saved = session.snapshot_v34();
        let mut stale = HumanKrenkoProjectorV34::new(actor.into());
        stale.project_current(&session, expected(&session)).unwrap();
        let input = request(&prompt, "Play Forest");
        projector.submit(&mut session, input.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_input = request(&future, "Pass priority");
        session.restore_v34(&saved).unwrap();
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
        let foreign = owned(basic(actor, "Lightning Bolt")).snapshot_v34();
        assert_eq!(session.restore_v34(&foreign), Err(Error::StaleDecision));
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v34(&saved),
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
        let mut session = HumanKrenkoSessionV34::from_session(base, actor.into()).unwrap();
        let e = expected(&session);
        let old = HumanDecisionProjectorV9::new(e.acting_player)
            .project_current(&previous, e)
            .unwrap();
        let mut projector = HumanKrenkoProjectorV34::new(e.acting_player);
        let prompt = projector.project_current(&session, e).unwrap();
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert_eq!(prompt.actions, old.actions);
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        let prefix = if pay { "Pay" } else { "Decline to pay" };
        let input = request(&prompt, prefix);
        let snapshot = session.snapshot_v34();
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v34(&snapshot).unwrap();
        let mut replay = HumanKrenkoProjectorV34::new(e.acting_player);
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
            FastActorSessionV1::from_krenko_v34_fixture_state(basic(actor, "Lightning Bolt"));
        let mut session =
            HumanKrenkoSessionV34::from_session(core, actor.opponent().into()).unwrap();
        let current = expected(&session);
        assert_eq!(current.acting_player, actor.into());
        assert!(session.public_observation(current).is_err());
        let mut projector = HumanKrenkoProjectorV34::new(actor.opponent().into());
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
    use crate::mana::ManaColor;
    use crate::state::Step;

    fn actual_cast_and_natural_untap(actor: PlayerId) -> GameState {
        let forest = card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            |_| "Forest".into(),
            3434,
            actor,
        );
        state.step = Step::Main1;
        let source = put(&mut state, actor, "Krenko, Mob Boss", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 4;
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        for _ in 0..100 {
            let decision = engine::advance_until_decision(&mut state);
            if state.objects.get(source).zone == Zone::Battlefield
                && !state.objects.get(source).summoning_sick
                && state.priority_player == actor
                && state.active_player == actor
                && matches!(decision, Decision::CastSpellOrPass { .. })
            {
                return state;
            }
            match decision {
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                Decision::DeclareAttackers { .. } => {
                    engine::step(&mut state, Action::DeclareAttackers(vec![])).unwrap()
                }
                other => panic!("genuine cast/Untap traversal {other:?}"),
            }
        }
        panic!("actual Krenko cast and subsequent Untap did not finish");
    }
    fn resolve(session: &mut HumanKrenkoSessionV34, projector: &mut HumanKrenkoProjectorV34) {
        for _ in 0..12 {
            if session.inner.krenko_fixture_state_mut().stack.is_empty() {
                return;
            }
            let current = expected(session);
            if current.acting_player == session.human_seat {
                let prompt = projector.project_current(session, current).unwrap();
                projector
                    .submit(session, request(&prompt, "Pass priority"))
                    .unwrap();
            } else {
                session
                    .step_opponent_with(current, |_, actions| {
                        Ok(actions
                            .iter()
                            .position(|a| matches!(a, ActionSemanticV1::Pass { .. }))
                            .unwrap() as u32)
                    })
                    .unwrap();
            }
        }
        panic!("genuine owning activation resolution did not finish");
    }

    #[test]

    fn actual_cast_tap_public_count_complete_tokens_safe_labels_restore_and_old_refusal_both_seats()
    {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let state = actual_cast_and_natural_untap(actor);
            let bytes = serde_json::to_vec(&state).unwrap();
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
            assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
            let mut session = owned(state);
            let mut projector = HumanKrenkoProjectorV34::new(actor.into());
            let prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            assert_safe(&serde_json::to_value(&prompt).unwrap());
            assert_eq!(
                prompt.state.extensions.sources[0].current_controlled_goblin_permanents,
                1
            );
            let action = request(&prompt, "Tap Krenko, Mob Boss");
            let before = session.snapshot_v34();
            let receipt = projector.submit(&mut session, action.clone()).unwrap();
            assert_eq!(
                projector.submit(&mut session, action.clone()).unwrap(),
                receipt
            );
            let public = session.public_observation(expected(&session)).unwrap();
            assert_eq!(
                public.facts.activations[0].lifecycle,
                crate::krenko_public_v1::LifecycleV1::PaidOnStack
            );
            let paid = session.snapshot_v34();
            resolve(&mut session, &mut projector);
            let final_hash = session.inner.privileged_core_environment_hash();
            let after = projector
                .project_current(&session, expected(&session))
                .unwrap();
            assert_safe(&serde_json::to_value(&after).unwrap());
            assert_eq!(
                after.state.extensions.activations[0].sampled_goblins,
                Some(1)
            );
            assert_eq!(
                after.state.extensions.activations[0].created_tokens.len(),
                1
            );
            assert!(after.state.extensions.activations[0].created_tokens[0]
                .current_visible_card
                .is_some());
            session.restore_v34(&paid).unwrap();
            assert_eq!(
                projector.submit(&mut session, action.clone()),
                Err(Error::StaleDecision)
            );
            resolve(&mut session, &mut projector);
            assert_eq!(session.inner.privileged_core_environment_hash(), final_hash);
            session.restore_v34(&before).unwrap();
            let fresh = projector
                .project_current(&session, expected(&session))
                .unwrap();
            projector
                .submit(&mut session, request(&fresh, "Tap Krenko, Mob Boss"))
                .unwrap();
            resolve(&mut session, &mut projector);
            assert_eq!(session.inner.privileged_core_environment_hash(), final_hash);
        }
    }

    #[test]

    fn actual_paid_snapshot_erasure_and_coherent_token_receipt_rewrite_reject_before_submit_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut session = owned(actual_cast_and_natural_untap(actor));
            let mut projector = HumanKrenkoProjectorV34::new(actor.into());
            let prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            projector
                .submit(&mut session, request(&prompt, "Tap Krenko, Mob Boss"))
                .unwrap();
            let pending = projector
                .project_current(&session, expected(&session))
                .unwrap();
            let pass = request(&pending, "Pass priority");
            let valid = session.snapshot_v34();
            session.inner.krenko_fixture_state_mut().stack.clear();
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, pass),
                Err(Error::StaleDecision)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            assert!(session.public_observation(expected(&session)).is_err());
            session.restore_v34(&valid).unwrap();
            resolve(&mut session, &mut projector);
            let completed = session.snapshot_v34();
            let core = session.inner.krenko_fixture_state_mut();
            let token = core
                .engine
                .event_history
                .iter_mut()
                .find(|e| matches!(e, crate::event::CommittedEvent::CreateToken { .. }))
                .unwrap();
            let crate::event::CommittedEvent::CreateToken { controller, .. } = token else {
                unreachable!()
            };
            *controller = actor.opponent();
            let malformed = session.snapshot_v34();
            session.restore_v34(&completed).unwrap();
            let before = session.inner.privileged_core_environment_hash();
            let generation = session.authority.generation;
            assert_eq!(session.restore_v34(&malformed), Err(Error::StaleDecision));
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            assert_eq!(session.authority.generation, generation);
        }
    }
}
