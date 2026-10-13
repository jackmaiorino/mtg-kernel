use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::put;
use crate::state::{GameState, Zone};

fn expected(session: &HumanGourmandSessionV29) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("real fixture must reach a decision");
    };
    expected
}
fn owned(state: GameState) -> HumanGourmandSessionV29 {
    let base = FastActorSessionV1::from_gourmand_v29_fixture_state(state);
    let FastActorResponseV1::Decision(expected) = base.current_response() else {
        panic!("actual actor decision");
    };
    HumanGourmandSessionV29::from_session(base, expected.acting_player).unwrap()
}

fn basic(actor: PlayerId, hidden: &str) -> GameState {
    let mut state = crate::policy_observation_v6::tests::ready_state_for(actor);
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    state
}
fn request(prompt: &HumanDecisionGourmandV29, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanGourmandSessionV29::from_session(base, actor.into()).unwrap();
        let current = expected(&session);
        let old = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&predecessor, current)
            .unwrap();
        let mut projector = HumanGourmandProjectorV29::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.actions, old.actions);
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert!(prompt
            .state
            .extensions
            .cannot_be_blocked_restrictions
            .is_empty());
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let hidden = owned(basic(actor, "Island"));
        assert_eq!(
            HumanGourmandProjectorV29::new(actor.into())
                .project_current(&hidden, expected(&hidden))
                .unwrap(),
            prompt
        );
        assert_eq!(
            HumanGourmandProjectorV29::new(actor.opponent().into())
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
        let mut projector = HumanGourmandProjectorV29::new(actor.into());
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let saved = session.snapshot_v29();
        let mut stale = HumanGourmandProjectorV29::new(actor.into());
        stale.project_current(&session, expected(&session)).unwrap();
        let input = request(&prompt, "Play Forest");
        projector.submit(&mut session, input.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_input = request(&future, "Pass priority");
        session.restore_v29(&saved).unwrap();
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
        let foreign = owned(basic(actor, "Lightning Bolt")).snapshot_v29();
        assert_eq!(session.restore_v29(&foreign), Err(Error::StaleDecision));
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v29(&saved),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
    }
}

fn ward_state(actor: PlayerId) -> GameState {
    use crate::engine::{self, Action, Decision};
    use crate::mana::ManaColor;
    use crate::state::Target;
    let mut state = crate::policy_observation_v6::tests::ready_state_for(actor);
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
        let mut session = HumanGourmandSessionV29::from_session(base, actor.into()).unwrap();
        let e = expected(&session);
        let old = HumanDecisionProjectorV9::new(e.acting_player)
            .project_current(&previous, e)
            .unwrap();
        let mut projector = HumanGourmandProjectorV29::new(e.acting_player);
        let prompt = projector.project_current(&session, e).unwrap();
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert_eq!(prompt.actions, old.actions);
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        let prefix = if pay { "Pay" } else { "Decline to pay" };
        let input = request(&prompt, prefix);
        let snapshot = session.snapshot_v29();
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v29(&snapshot).unwrap();
        let mut replay = HumanGourmandProjectorV29::new(e.acting_player);
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
            FastActorSessionV1::from_gourmand_v29_fixture_state(basic(actor, "Lightning Bolt"));
        let mut session =
            HumanGourmandSessionV29::from_session(core, actor.opponent().into()).unwrap();
        let current = expected(&session);
        assert_eq!(current.acting_player, actor.into());
        assert!(session.public_observation(current).is_err());
        let mut projector = HumanGourmandProjectorV29::new(actor.opponent().into());
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
    use crate::engine::{self, Action, Decision, EffectDuration, Layers, UntilEndOfTurnEffect};
    use crate::mana::ManaColor;
    use crate::state::Step;
    fn next(state: &mut GameState) -> Decision {
        let decision = engine::advance_until_decision(state);
        assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
        decision
    }
    fn actual_cost(actor: PlayerId) -> (GameState, crate::ids::ObjectId, crate::ids::ObjectId) {
        let forest = crate::card_def::card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            292_001,
            actor,
        );
        state.step = Step::Main1;
        for player in [actor, actor.opponent()] {
            put(&mut state, player, "Mountain", Zone::Battlefield);
            put(&mut state, player, "Lightning Bolt", Zone::Hand);
        }
        let source = put(&mut state, actor, "Vampire Gourmand", Zone::Hand);
        let other = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 2;
        assert!(
            matches!(next(&mut state),Decision::CastSpellOrPass {player,castable_spells,..} if player==actor && castable_spells.contains(&source))
        );
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        for _ in 0..30 {
            let decision = next(&mut state);
            if state.objects.get(source).zone == Zone::Battlefield && state.stack.is_empty() {
                break;
            }
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
        // Explicit physical haste grant, without fabricating a printed attack root.
        let timestamp = engine::next_timestamp(&mut state);
        state
            .engine
            .until_end_of_turn
            .push(UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
                object_id: source,
                object_zone_change_count: state.objects.get(source).zone_change_count,
                layer: Layers::ABILITY_ADDING,
                timestamp,
                duration: EffectDuration::EndOfTurn,
                keywords: crate::card_def::Keywords::HASTE,
            });
        for _ in 0..30 {
            match next(&mut state) {
                Decision::DeclareAttackers { eligible, .. } => {
                    assert!(eligible.contains(&source));
                    engine::step(&mut state, Action::DeclareAttackers(vec![source])).unwrap();
                    break;
                }
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                other => panic!("actual combat {other:?}"),
            }
        }
        for _ in 0..30 {
            match next(&mut state) {
                Decision::ChooseEffectTargets {
                    player,
                    source: id,
                    min_targets,
                    max_targets,
                    can_finish,
                    ..
                } => {
                    assert_eq!(
                        (player, id, min_targets, max_targets, can_finish),
                        (actor, source, 0, 1, true)
                    );
                    return (state, source, other);
                }
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                other => panic!("actual cost {other:?}"),
            }
        }
        panic!("actual Gourmand cost absent");
    }
    #[test]
    fn actual_cast_attack_optional_cost_safe_labels_submit_restore_and_actor_privacy() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for pay in [false, true] {
                let (state, source, other) = actual_cost(actor);
                let original_other = state.objects.get(other).zone_change_count;
                let mut session = owned(state.clone());
                let current = expected(&session);
                let public = session.public_observation(current).unwrap();
                assert_eq!(public.roots.len(), 1);
                let menu = public.sacrifice_menu.as_ref().unwrap();
                assert_eq!(menu.payer, actor.into());
                assert_eq!((menu.minimum, menu.maximum), (0, 1));
                assert!(menu
                    .legal
                    .iter()
                    .any(|r| r.arena_id == other.0 && r.zone_change_count == original_other));
                assert!(!menu.legal.iter().any(|r| r.arena_id == source.0));
                let mut projector = HumanGourmandProjectorV29::new(actor.into());
                let prompt = projector.project_current(&session, current).unwrap();
                assert_safe(&serde_json::to_value(&prompt).unwrap());
                assert!(prompt.state.extensions.sacrifice_menu.is_some());
                let mut hidden = state.clone();
                let foreign = hidden.players[actor.opponent().index()].hand[0];
                let definition = crate::card_def::card_id_by_name("Mountain").unwrap();
                let object = hidden.objects.get_mut(foreign);
                object.card_def = definition;
                object.name = "Mountain".into();
                object.v4 = crate::state::ObjectStateV4::from_card_def(definition);
                let hidden = owned(hidden);
                assert_eq!(
                    HumanGourmandProjectorV29::new(actor.into())
                        .project_current(&hidden, expected(&hidden))
                        .unwrap(),
                    prompt
                );
                let input = request(
                    &prompt,
                    if pay {
                        "Sacrifice Llanowar Elves"
                    } else {
                        "Decline the sacrifice"
                    },
                );
                let hand_before = public.own_hand.len();
                let snapshot = session.snapshot_v29();
                let receipt = projector.submit(&mut session, input.clone()).unwrap();
                let after_public = session.public_observation(expected(&session)).unwrap();
                assert_eq!(after_public.own_hand.len(), hand_before + usize::from(pay));
                assert_eq!(after_public.restrictions.len(), usize::from(pay));
                assert_eq!(after_public.resolutions.last().unwrap().paid, pay);
                assert_eq!(after_public.resolutions.last().unwrap().draw_attempted, pay);
                assert_eq!(
                    after_public.projection.surface.graveyards[actor.index()]
                        .iter()
                        .any(|c| c.stable.arena_id == other.0),
                    pay
                );
                let after = session.inner.privileged_core_environment_hash();
                assert_eq!(
                    projector.submit(&mut session, input.clone()).unwrap(),
                    receipt
                );
                assert_eq!(session.inner.privileged_core_environment_hash(), after);
                session.restore_v29(&snapshot).unwrap();
                let before = session.inner.privileged_core_environment_hash();
                assert_eq!(
                    projector.submit(&mut session, input.clone()),
                    Err(Error::StaleDecision)
                );
                assert_eq!(session.inner.privileged_core_environment_hash(), before);
                let mut replay = HumanGourmandProjectorV29::new(actor.into());
                let restored = replay
                    .project_current(&session, expected(&session))
                    .unwrap();
                assert_eq!(restored, prompt);
                replay.submit(&mut session, input).unwrap();
                assert_eq!(session.inner.privileged_core_environment_hash(), after);
            }
        }
    }
}
