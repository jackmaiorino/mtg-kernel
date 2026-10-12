use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::{GameState, Zone};

fn expected(session: &HumanFrenziedSessionV15) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("real fixture must reach a decision");
    };
    expected
}
fn owned(state: GameState) -> HumanFrenziedSessionV15 {
    HumanFrenziedSessionV15::from_session(FastActorSessionV1::from_frenzied_v15_fixture_state(
        state,
    ))
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
fn request(prompt: &HumanDecisionFrenziedV15, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanFrenziedSessionV15::from_session(base).unwrap();
        let current = expected(&session);
        let old = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&predecessor, current)
            .unwrap();
        let mut projector = HumanFrenziedProjectorV15::new(actor.into());
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
            HumanFrenziedProjectorV15::new(actor.into())
                .project_current(&hidden, expected(&hidden))
                .unwrap(),
            prompt
        );
        assert_eq!(
            HumanFrenziedProjectorV15::new(actor.opponent().into())
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
        let mut projector = HumanFrenziedProjectorV15::new(actor.into());
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let saved = session.snapshot_v15();
        let mut stale = HumanFrenziedProjectorV15::new(actor.into());
        stale.project_current(&session, expected(&session)).unwrap();
        let input = request(&prompt, "Play Forest");
        projector.submit(&mut session, input.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_input = request(&future, "Pass priority");
        session.restore_v15(&saved).unwrap();
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
        let foreign = owned(basic(actor, "Lightning Bolt")).snapshot_v15();
        assert_eq!(session.restore_v15(&foreign), Err(Error::StaleDecision));
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v15(&saved),
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
        let mut session = HumanFrenziedSessionV15::from_session(base).unwrap();
        let e = expected(&session);
        let old = HumanDecisionProjectorV9::new(e.acting_player)
            .project_current(&previous, e)
            .unwrap();
        let mut projector = HumanFrenziedProjectorV15::new(e.acting_player);
        let prompt = projector.project_current(&session, e).unwrap();
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert_eq!(prompt.actions, old.actions);
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        let prefix = if pay { "Pay" } else { "Decline to pay" };
        let input = request(&prompt, prefix);
        let snapshot = session.snapshot_v15();
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v15(&snapshot).unwrap();
        let mut replay = HumanFrenziedProjectorV15::new(e.acting_player);
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

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn future_actual_attack_target_and_optional_red_payment_own_restore_replay_both_seats() {
    use crate::mana::ManaColor;
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay in [false, true] {
            let (mut state, _, _) = crate::frenzied_public_v1::tests::actual_attack(actor);
            state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
            let mut session = owned(state);
            let mut projector = HumanFrenziedProjectorV15::new(actor.into());
            let target = projector
                .project_current(&session, expected(&session))
                .unwrap();
            assert_safe(&serde_json::to_value(&target).unwrap());
            assert!(target
                .actions
                .iter()
                .any(|row| row.label.contains("Llanowar Elves")));
            let selected = target
                .actions
                .iter()
                .find(|row| row.label.starts_with("Target Llanowar Elves"))
                .unwrap();
            projector
                .submit(
                    &mut session,
                    HumanActionRequestV1 {
                        prompt_seq: target.prompt_seq,
                        action_index: selected.action_index,
                    },
                )
                .unwrap();
            for _ in 0..10 {
                let current = expected(&session);
                let mut next_projector = HumanFrenziedProjectorV15::new(current.acting_player);
                let prompt = next_projector.project_current(&session, current).unwrap();
                if prompt
                    .state
                    .extensions
                    .attack_resolutions
                    .iter()
                    .any(|row| row.phase == "optional_red_payment")
                {
                    assert_eq!(current.acting_player, actor.into());
                    assert_eq!(
                        prompt.state.extensions.attack_resolutions[0].captured_payer,
                        actor.into()
                    );
                    assert!(session.public_observation(current).unwrap().resolutions[0]
                        .live_stack_index
                        .is_some());
                    assert_eq!(
                        HumanDecisionProjectorV9::new(actor.into()).project_current(
                            &HumanRaidSessionV9::from_session(session.inner.clone()).unwrap(),
                            current
                        ),
                        Err(Error::UnsupportedPrompt)
                    );
                    let saved = session.snapshot_v15();
                    let input =
                        request(&prompt, if pay { "Pay {R}" } else { "Decline to pay {R}" });
                    let receipt = next_projector.submit(&mut session, input.clone()).unwrap();
                    let after = session.inner.privileged_core_environment_hash();
                    assert_eq!(
                        next_projector.submit(&mut session, input.clone()).unwrap(),
                        receipt
                    );
                    assert_eq!(session.inner.privileged_core_environment_hash(), after);
                    session.restore_v15(&saved).unwrap();
                    let before = session.inner.privileged_core_environment_hash();
                    assert_eq!(
                        next_projector.submit(&mut session, input.clone()),
                        Err(Error::StaleDecision)
                    );
                    assert_eq!(session.inner.privileged_core_environment_hash(), before);
                    let mut replay = HumanFrenziedProjectorV15::new(actor.into());
                    assert_eq!(
                        replay
                            .project_current(&session, expected(&session))
                            .unwrap(),
                        prompt
                    );
                    replay.submit(&mut session, input).unwrap();
                    assert_eq!(session.inner.privileged_core_environment_hash(), after);
                    let public = session.public_observation(expected(&session)).unwrap();
                    assert_eq!(public.restrictions.len(), usize::from(pay));
                    assert_eq!(public.resolutions[0].red_paid, pay);
                    assert_eq!(
                        public.resolutions[0].outcome,
                        Some(if pay {
                            crate::effect::FrenziedGoblinOutcomeV1::Paid
                        } else {
                            crate::effect::FrenziedGoblinOutcomeV1::Declined
                        })
                    );
                    break;
                }
                let input = request(&prompt, "Pass priority");
                next_projector.submit(&mut session, input).unwrap();
            }
            assert!(session
                .public_observation(expected(&session))
                .unwrap()
                .resolutions[0]
                .outcome
                .is_some());
        }
    }
}
