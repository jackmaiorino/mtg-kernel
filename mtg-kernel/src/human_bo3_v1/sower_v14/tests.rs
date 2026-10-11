use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::{GameState, Zone};

fn expected(session: &HumanSowerSessionV14) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("real fixture must reach a decision");
    };
    expected
}
fn owned(state: GameState) -> HumanSowerSessionV14 {
    HumanSowerSessionV14::from_session(FastActorSessionV1::from_sower_v14_fixture_state(state))
        .unwrap()
}
fn basic(actor: PlayerId, hidden: &str) -> GameState {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    state
}
fn request(prompt: &HumanDecisionSowerV14, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanSowerSessionV14::from_session(base).unwrap();
        let current = expected(&session);
        let old = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&predecessor, current)
            .unwrap();
        let mut projector = HumanSowerProjectorV14::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.actions, old.actions);
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert!(prompt.state.extensions.cannot_block_restrictions.is_empty());
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let hidden = owned(basic(actor, "Island"));
        assert_eq!(
            HumanSowerProjectorV14::new(actor.into())
                .project_current(&hidden, expected(&hidden))
                .unwrap(),
            prompt
        );
        assert_eq!(
            HumanSowerProjectorV14::new(actor.opponent().into()).project_current(&session, current),
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
        let mut projector = HumanSowerProjectorV14::new(actor.into());
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let saved = session.snapshot_v14();
        let mut stale = HumanSowerProjectorV14::new(actor.into());
        stale.project_current(&session, expected(&session)).unwrap();
        let input = request(&prompt, "Play Forest");
        projector.submit(&mut session, input.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_input = request(&future, "Pass priority");
        session.restore_v14(&saved).unwrap();
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
        let foreign = owned(basic(actor, "Lightning Bolt")).snapshot_v14();
        assert_eq!(session.restore_v14(&foreign), Err(Error::StaleDecision));
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v14(&saved),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn actual_printed_activation_label_target_payment_resolution_and_restored_selection() {
    use crate::mana::ManaColor;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = basic(actor, "Lightning Bolt");
        put(&mut state, actor, "Sower of Chaos", Zone::Battlefield);
        let recipient = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        for owner in [actor, actor.opponent()] {
            for _ in 0..4 {
                put(&mut state, owner, "Forest", Zone::Library);
            }
        }
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 3;
        let base = FastActorSessionV1::from_human_v8_fixture_state(state);
        let old = HumanRaidSessionV9::from_session(base.clone()).unwrap();
        let mut session = HumanSowerSessionV14::from_session(base).unwrap();
        let current = expected(&session);
        let (observation, mut forged_actions, _) = session
            .inner
            .sower_current_decision_input_v14(current, actor.into())
            .unwrap();
        let forged = forged_actions
            .iter_mut()
            .find(|action| matches!(action, ActionSemanticV1::ActivateAbility { .. }))
            .expect("actual Sower activation");
        let ActionSemanticV1::ActivateAbility { ability_index, .. } = forged else {
            unreachable!()
        };
        *ability_index = 1;
        assert!(matches!(
            project_base(&observation, &forged_actions, actor.into()),
            Err(Error::UnsupportedPrompt)
        ));
        assert_eq!(
            HumanDecisionProjectorV9::new(actor.into()).project_current(&old, current),
            Err(Error::UnsupportedPrompt)
        );
        let mut projector = HumanSowerProjectorV14::new(actor.into());
        let prompt = projector.project_current(&session, current).unwrap();
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        let activation = request(&prompt, "Activate Sower of Chaos");
        let label = &prompt.actions[activation.action_index as usize].label;
        assert!(label.contains("{2}{R}"));
        assert!(label.contains("cannot block"));
        projector.submit(&mut session, activation).unwrap();
        let target_prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let saved = session.snapshot_v14();
        let target_input = request(&target_prompt, "Target Llanowar Elves");
        projector
            .submit(&mut session, target_input.clone())
            .unwrap();
        let finalized = session.inner.privileged_core_environment_hash();
        let public = session.public_observation(expected(&session)).unwrap();
        assert_eq!(public.projection.surface.mana_pools[actor.index()], [0; 6]);
        assert_eq!(public.projection.surface.stack.len(), 1);
        session.restore_v14(&saved).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, target_input),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let restored = projector
            .project_current(&session, expected(&session))
            .unwrap();
        projector
            .submit(&mut session, request(&restored, "Target Llanowar Elves"))
            .unwrap();
        assert_eq!(session.inner.privileged_core_environment_hash(), finalized);
        for _ in 0..12 {
            let current = expected(&session);
            if session
                .public_observation(current)
                .unwrap()
                .projection
                .surface
                .stack
                .is_empty()
            {
                break;
            }
            let mut acting = HumanSowerProjectorV14::new(current.acting_player);
            let prompt = acting.project_current(&session, current).unwrap();
            acting
                .submit(&mut session, request(&prompt, "Pass priority"))
                .unwrap();
        }
        let public = session.public_observation(expected(&session)).unwrap();
        assert!(public.projection.surface.stack.is_empty());
        assert_eq!(public.restrictions.len(), 1);
        assert_eq!(public.restrictions[0].recipient.arena_id, recipient.0);
        let current = expected(&session);
        let prompt = HumanSowerProjectorV14::new(current.acting_player)
            .project_current(&session, current)
            .unwrap();
        assert!(prompt.state.extensions.cannot_block_restrictions[0].effective);
        assert_safe(&serde_json::to_value(&prompt).unwrap());
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn installed_relation_and_forged_public_binding_refuse_without_resource_changes() {
    use crate::event::CommittedEvent;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, _, _) = crate::sower_public_v1::tests::activated(actor);
        let mut session = owned(state);
        let current = expected(&session);
        let mut projector = HumanSowerProjectorV14::new(current.acting_player);
        let prompt = projector.project_current(&session, current).unwrap();
        let input = request(&prompt, "Pass priority");
        let before = session.inner.privileged_core_environment_hash();
        projector.pending.as_mut().unwrap().public.restrictions[0].effective = false;
        assert_eq!(
            projector.submit(&mut session, input),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let mut projector = HumanSowerProjectorV14::new(current.acting_player);
        let prompt = projector.project_current(&session, current).unwrap();
        let input = request(&prompt, "Pass priority");
        session
            .inner
            .sower_fixture_state_mut()
            .engine
            .event_history
            .retain(|event| !matches!(event, CommittedEvent::SowerRestrictionInstalledV1 { .. }));
        let damaged = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), damaged);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn actual_attack_blocker_menu_agrees_with_effective_public_restriction() {
    use crate::engine::{self, Action, Decision};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, recipient) = crate::sower_public_v1::tests::activated(actor);
        state.step = crate::state::Step::DeclareAttackers;
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::DeclareAttackers { .. }
        ));
        engine::step(&mut state, Action::DeclareAttackers(vec![source])).unwrap();
        let mut found = false;
        for _ in 0..8 {
            let decision = engine::advance_until_decision(&mut state);
            if matches!(decision, Decision::DeclareBlockers { .. }) {
                found = true;
                break;
            }
            assert!(
                matches!(decision, Decision::CastSpellOrPass { .. }),
                "{decision:?}"
            );
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert!(found);
        let mut session = owned(state);
        let current = expected(&session);
        assert_eq!(current.acting_player, actor.opponent().into());
        let mut projector = HumanSowerProjectorV14::new(current.acting_player);
        let prompt = projector.project_current(&session, current).unwrap();
        assert!(prompt.state.extensions.cannot_block_restrictions[0].effective);
        assert!(!prompt
            .actions
            .iter()
            .any(|action| action.label.contains("with Llanowar Elves")));
        let before = session.inner.privileged_core_environment_hash();
        // The engine independently rejects direct use of the forbidden blocker.
        let mut physical = session.inner.sower_fixture_state_mut().clone();
        let physical_before = serde_json::to_vec(&physical).unwrap();
        assert!(engine::step(
            &mut physical,
            Action::DeclareBlockers(vec![(recipient, source)])
        )
        .is_err());
        assert_eq!(serde_json::to_vec(&physical).unwrap(), physical_before);
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        assert_safe(&serde_json::to_value(&prompt).unwrap());
    }
}
