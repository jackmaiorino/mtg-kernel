use super::*;
use crate::ids::PlayerId;
use crate::inspiration_observation_v1::tests::ready_state_for;
use crate::policy_observation_v6::tests::put;
use crate::state::{GameState, Zone};

fn expected(session: &HumanInspirationSessionV37) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("real fixture must reach a decision");
    };
    expected
}
fn owned(state: GameState) -> HumanInspirationSessionV37 {
    let base = FastActorSessionV1::from_inspiration_v37_fixture_state(state);
    let FastActorResponseV1::Decision(expected) = base.current_response() else {
        panic!("actual actor decision");
    };
    HumanInspirationSessionV37::from_session(base, expected.acting_player).unwrap()
}

fn basic(actor: PlayerId, hidden: &str) -> GameState {
    let mut state = ready_state_for(actor);
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    state
}
fn request(prompt: &HumanDecisionInspirationV37, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanInspirationSessionV37::from_session(base, actor.into()).unwrap();
        let current = expected(&session);
        let old = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&predecessor, current)
            .unwrap();
        let mut projector = HumanInspirationProjectorV37::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.actions, old.actions);
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert!(prompt.state.extensions.choice.is_none());
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let hidden = owned(basic(actor, "Island"));
        assert_eq!(
            HumanInspirationProjectorV37::new(actor.into())
                .project_current(&hidden, expected(&hidden))
                .unwrap(),
            prompt
        );
        assert_eq!(
            HumanInspirationProjectorV37::new(actor.opponent().into())
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
        let mut projector = HumanInspirationProjectorV37::new(actor.into());
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let saved = session.snapshot_v37();
        let mut stale = HumanInspirationProjectorV37::new(actor.into());
        stale.project_current(&session, expected(&session)).unwrap();
        let input = request(&prompt, "Play Forest");
        projector.submit(&mut session, input.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_input = request(&future, "Pass priority");
        session.restore_v37(&saved).unwrap();
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
        let foreign = owned(basic(actor, "Lightning Bolt")).snapshot_v37();
        assert_eq!(session.restore_v37(&foreign), Err(Error::StaleDecision));
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v37(&saved),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
    }
}

fn ward_state(actor: PlayerId) -> GameState {
    use crate::engine::{self, Action, Decision};
    use crate::mana::ManaColor;
    use crate::state::Target;
    let mut state = ready_state_for(actor);
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
        let mut session = HumanInspirationSessionV37::from_session(base, actor.into()).unwrap();
        let e = expected(&session);
        let old = HumanDecisionProjectorV9::new(e.acting_player)
            .project_current(&previous, e)
            .unwrap();
        let mut projector = HumanInspirationProjectorV37::new(e.acting_player);
        let prompt = projector.project_current(&session, e).unwrap();
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert_eq!(prompt.actions, old.actions);
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        let prefix = if pay { "Pay" } else { "Decline to pay" };
        let input = request(&prompt, prefix);
        let snapshot = session.snapshot_v37();
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v37(&snapshot).unwrap();
        let mut replay = HumanInspirationProjectorV37::new(e.acting_player);
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
            FastActorSessionV1::from_inspiration_v37_fixture_state(basic(actor, "Lightning Bolt"));
        let mut session =
            HumanInspirationSessionV37::from_session(core, actor.opponent().into()).unwrap();
        let current = expected(&session);
        assert_eq!(current.acting_player, actor.into());
        assert!(session.public_observation(current).is_err());
        let mut projector = HumanInspirationProjectorV37::new(actor.opponent().into());
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
#[test]
fn genuine_unknown_mill_and_return_human_privacy_restore_retry_and_binding_both_seats() {
    use crate::inspiration_observation_v1::PhaseV1;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut prompts = Vec::new();
        for hidden in ["Lightning Bolt", "Island"] {
            let core = crate::inspiration_observation_v1::tests::registered::at_mill(actor, hidden);
            let mut session = owned(core);
            let mut projector = HumanInspirationProjectorV37::new(actor.into());
            let prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            assert_safe(&serde_json::to_value(&prompt).unwrap());
            assert_eq!(prompt.actions.len(), 3);
            let context = prompt.state.extensions.choice.as_ref().unwrap();
            assert_eq!(context.phase, PhaseV1::MillOrder);
            assert_eq!((context.minimum, context.maximum), (3, 3));
            assert_eq!(context.legal.len(), 3);
            assert!(!context.instant_or_sorcery_only && context.choice_is_not_a_target);
            prompts.push(prompt.clone());
            let saved = session.snapshot_v37();
            let input = request(&prompt, "Order ");
            let receipt = projector.submit(&mut session, input.clone()).unwrap();
            let after = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, input.clone()).unwrap(),
                receipt
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), after);
            session.restore_v37(&saved).unwrap();
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, input),
                Err(Error::StaleDecision)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            for _ in 0..5 {
                let current = projector
                    .project_current(&session, expected(&session))
                    .unwrap();
                let row = current.state.extensions.choice.as_ref().unwrap();
                if row.phase == PhaseV1::ReturnCard {
                    break;
                }
                projector
                    .submit(&mut session, request(&current, "Order "))
                    .unwrap();
            }
            let current = projector
                .project_current(&session, expected(&session))
                .unwrap();
            let row = current.state.extensions.choice.as_ref().unwrap();
            assert_eq!(row.phase, PhaseV1::ReturnCard);
            assert!(row.instant_or_sorcery_only && row.choice_is_not_a_target);
            assert_eq!((row.minimum, row.maximum), (1, 1));
            assert!(!row.can_finish);
            assert!(current
                .actions
                .iter()
                .all(|a| a.label.starts_with("Return ") && a.label.contains("not a target")));
            let saved = session.snapshot_v37();
            let mut corrupt = saved.clone();
            FastActorSessionV1::inspiration_fixture_snapshot_minimum(&mut corrupt.snapshot, 0);
            let before = session.inner.privileged_core_environment_hash();
            assert!(session.restore_v37(&corrupt).is_err());
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            projector
                .submit(&mut session, request(&current, "Return "))
                .unwrap();
        }
        assert_eq!(prompts[0], prompts[1]);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod integration_v1;
