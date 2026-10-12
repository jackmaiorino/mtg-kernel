use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::{GameState, Zone};

fn expected(session: &HumanZulAshurSessionV35) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("real fixture must reach a decision");
    };
    expected
}
fn owned(state: GameState) -> HumanZulAshurSessionV35 {
    let base = FastActorSessionV1::from_zul_ashur_v35_fixture_state(state);
    let FastActorResponseV1::Decision(expected) = base.current_response() else {
        panic!("actual actor decision");
    };
    HumanZulAshurSessionV35::from_session(base, expected.acting_player).unwrap()
}

fn basic(actor: PlayerId, hidden: &str) -> GameState {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    state
}
fn request(prompt: &HumanDecisionZulAshurV35, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanZulAshurSessionV35::from_session(base, actor.into()).unwrap();
        let current = expected(&session);
        let old = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&predecessor, current)
            .unwrap();
        let mut projector = HumanZulAshurProjectorV35::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.actions, old.actions);
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert!(prompt.state.extensions.permissions.is_empty());
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let hidden = owned(basic(actor, "Island"));
        assert_eq!(
            HumanZulAshurProjectorV35::new(actor.into())
                .project_current(&hidden, expected(&hidden))
                .unwrap(),
            prompt
        );
        assert_eq!(
            HumanZulAshurProjectorV35::new(actor.opponent().into())
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
        let mut projector = HumanZulAshurProjectorV35::new(actor.into());
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let saved = session.snapshot_v35();
        let mut stale = HumanZulAshurProjectorV35::new(actor.into());
        stale.project_current(&session, expected(&session)).unwrap();
        let input = request(&prompt, "Play Forest");
        projector.submit(&mut session, input.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_input = request(&future, "Pass priority");
        session.restore_v35(&saved).unwrap();
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
        let foreign = owned(basic(actor, "Lightning Bolt")).snapshot_v35();
        assert_eq!(session.restore_v35(&foreign), Err(Error::StaleDecision));
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v35(&saved),
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
        let mut session = HumanZulAshurSessionV35::from_session(base, actor.into()).unwrap();
        let e = expected(&session);
        let old = HumanDecisionProjectorV9::new(e.acting_player)
            .project_current(&previous, e)
            .unwrap();
        let mut projector = HumanZulAshurProjectorV35::new(e.acting_player);
        let prompt = projector.project_current(&session, e).unwrap();
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert_eq!(prompt.actions, old.actions);
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        let prefix = if pay { "Pay" } else { "Decline to pay" };
        let input = request(&prompt, prefix);
        let snapshot = session.snapshot_v35();
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v35(&snapshot).unwrap();
        let mut replay = HumanZulAshurProjectorV35::new(e.acting_player);
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
            FastActorSessionV1::from_zul_ashur_v35_fixture_state(basic(actor, "Lightning Bolt"));
        let mut session =
            HumanZulAshurSessionV35::from_session(core, actor.opponent().into()).unwrap();
        let current = expected(&session);
        assert_eq!(current.acting_player, actor.into());
        assert!(session.public_observation(current).is_err());
        let mut projector = HumanZulAshurProjectorV35::new(actor.opponent().into());
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
    use crate::engine::{Action, Decision};
    use crate::ids::ObjectId;
    use crate::mana::ManaColor;
    use crate::state::{Step, Target};
    fn session(actor: PlayerId) -> (HumanZulAshurSessionV35, ObjectId, ObjectId) {
        let mut state = ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        state.step = Step::Main1;
        let source = put(&mut state, actor, "Zul Ashur, Lich Lord", Zone::Battlefield);
        let target = put(&mut state, actor, "Diregraf Ghoul", Zone::Graveyard);
        put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
        for seat in [actor, actor.opponent()] {
            for _ in 0..8 {
                put(&mut state, seat, "Forest", Zone::Library);
            }
        }
        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 1;
        let base = FastActorSessionV1::from_zul_ashur_v35_engine_priority_fixture_state(state);
        (
            HumanZulAshurSessionV35::from_session(base, actor.into()).unwrap(),
            source,
            target,
        )
    }
    fn pass_until_empty(
        session: &mut HumanZulAshurSessionV35,
        projector: &mut HumanZulAshurProjectorV35,
        actor: PlayerId,
    ) {
        for _ in 0..30 {
            if session.inner.zul_ashur_fixture_state_mut().stack.is_empty() {
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
        panic!("actual Zul resolution");
    }
    #[test]
    #[ignore = "Zul Ashur is unregistered; actual owning Human qualification remains root-owned"]
    fn actual_activation_own_graveyard_cast_public_facts_safe_labels_and_owning_rollback_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut session, source, target) = session(actor);
            let mut projector = HumanZulAshurProjectorV35::new(actor.into());
            let e = expected(&session);
            assert_eq!(session.public_observation(e).unwrap().roots.len(), 3);
            let prompt = projector.project_current(&session, e).unwrap();
            assert_safe(&serde_json::to_value(&prompt).unwrap());
            let snapshot = session.snapshot_v35();
            let activation = request(&prompt, "Activate Zul Ashur");
            let accepted = projector.submit(&mut session, activation.clone()).unwrap();
            assert_eq!(
                projector.submit(&mut session, activation.clone()).unwrap(),
                accepted
            );
            let target_prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            assert!(target_prompt
                .actions
                .iter()
                .any(|a| a.label.contains("Diregraf Ghoul")));
            session.restore_v35(&snapshot).unwrap();
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, activation),
                Err(Error::StaleDecision)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            let prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            projector
                .submit(&mut session, request(&prompt, "Activate Zul Ashur"))
                .unwrap();
            let target_prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            projector
                .submit(
                    &mut session,
                    request(&target_prompt, "Target Diregraf Ghoul"),
                )
                .unwrap();
            assert!(
                session
                    .inner
                    .zul_ashur_fixture_state_mut()
                    .objects
                    .get(source)
                    .tapped
            );
            pass_until_empty(&mut session, &mut projector, actor);
            let public = session.public_observation(expected(&session)).unwrap();
            assert_eq!(public.facts.len(), 1);
            assert!(
                public.facts[0].granted_this_turn && public.facts[0].original_card_in_graveyard
            );
            let prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            assert_safe(&serde_json::to_value(&prompt).unwrap());
            assert_eq!(prompt.state.extensions.permissions.len(), 1);
            let before = session.snapshot_v35();
            let cast = request(&prompt, "Cast Diregraf Ghoul from your graveyard");
            projector.submit(&mut session, cast.clone()).unwrap();
            assert_eq!(
                session
                    .inner
                    .zul_ashur_fixture_state_mut()
                    .objects
                    .get(target)
                    .zone,
                Zone::Stack
            );
            session.restore_v35(&before).unwrap();
            let hash = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, cast),
                Err(Error::StaleDecision)
            );
            assert_eq!(hash, session.inner.privileged_core_environment_hash());
            let prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            projector
                .submit(
                    &mut session,
                    request(&prompt, "Cast Diregraf Ghoul from your graveyard"),
                )
                .unwrap();
            pass_until_empty(&mut session, &mut projector, actor);
            assert_eq!(
                session
                    .inner
                    .zul_ashur_fixture_state_mut()
                    .objects
                    .get(target)
                    .zone,
                Zone::Battlefield
            );
            assert!(
                session
                    .inner
                    .zul_ashur_fixture_state_mut()
                    .objects
                    .get(target)
                    .tapped
            );
        }
    }
    #[test]
    #[ignore = "Zul Ashur is unregistered; actual source custody/public refusal qualification remains root-owned"]
    fn actual_published_owner_erasure_and_immutable_input_header_order_forgeries_are_readonly_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut session, source, target) = session(actor);
            let state = session.inner.zul_ashur_fixture_state_mut();
            crate::engine::step(state, Action::ActivateAbility(source, 0)).unwrap();
            assert!(matches!(
                crate::engine::advance_until_decision(state),
                Decision::ChooseTargets { .. }
            ));
            crate::engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
            let _ = crate::engine::advance_until_decision(state);
            let original = state.clone();
            let mut bad = original.clone();
            bad.engine
                .event_history
                .retain(|e| !matches!(e, crate::event::CommittedEvent::ZulAshurPublishedV1 { .. }));
            let before = serde_json::to_vec(&bad).unwrap();
            let result = crate::zul_ashur_observation_v1::observe_v1(
                &bad,
                &crate::policy_surface_v5::PolicySurfaceV5::new_with_engine_priority_v1(),
                actor,
                0,
                0,
                0,
                1,
            );
            assert!(result.is_err());
            assert_eq!(before, serde_json::to_vec(&bad).unwrap());
            let actual = crate::zul_ashur_observation_v1::observe_v1(
                &original,
                &crate::policy_surface_v5::PolicySurfaceV5::new_with_engine_priority_v1(),
                actor,
                0,
                0,
                0,
                1,
            )
            .unwrap();
            actual.validate_artifact().unwrap();
            let mut forged = actual;
            forged.roots.reverse();
            assert!(forged.validate_artifact().is_err());
        }
    }
}
