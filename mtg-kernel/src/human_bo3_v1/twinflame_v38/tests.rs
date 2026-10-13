use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::put;
use crate::policy_observation_v6::tests::ready_state_for;
use crate::state::{GameState, Zone};

fn expected(session: &HumanTwinflameSessionV38) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("real fixture must reach a decision");
    };
    expected
}
fn owned(state: GameState) -> HumanTwinflameSessionV38 {
    let base = FastActorSessionV1::from_twinflame_v38_fixture_state(state);
    let FastActorResponseV1::Decision(expected) = base.current_response() else {
        panic!("actual actor decision");
    };
    HumanTwinflameSessionV38::from_session(base, expected.acting_player).unwrap()
}

fn basic(actor: PlayerId, hidden: &str) -> GameState {
    let mut state = ready_state_for(actor);
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    state
}
fn request(prompt: &HumanDecisionTwinflameV38, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanTwinflameSessionV38::from_session(base, actor.into()).unwrap();
        let current = expected(&session);
        let old = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&predecessor, current)
            .unwrap();
        let mut projector = HumanTwinflameProjectorV38::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.actions, old.actions);
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert!(prompt.state.extensions.prevention_menu.is_none());
        assert!(prompt.state.extensions.combat_allocation.is_none());
        assert!(prompt.state.extensions.multiplier_sources.is_empty());
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let hidden = owned(basic(actor, "Island"));
        assert_eq!(
            HumanTwinflameProjectorV38::new(actor.into())
                .project_current(&hidden, expected(&hidden))
                .unwrap(),
            prompt
        );
        assert_eq!(
            HumanTwinflameProjectorV38::new(actor.opponent().into())
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
        let mut projector = HumanTwinflameProjectorV38::new(actor.into());
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let saved = session.snapshot_v38();
        let mut stale = HumanTwinflameProjectorV38::new(actor.into());
        stale.project_current(&session, expected(&session)).unwrap();
        let input = request(&prompt, "Play Forest");
        projector.submit(&mut session, input.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_input = request(&future, "Pass priority");
        session.restore_v38(&saved).unwrap();
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
        let foreign = owned(basic(actor, "Lightning Bolt")).snapshot_v38();
        assert_eq!(session.restore_v38(&foreign), Err(Error::StaleDecision));
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v38(&saved),
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
        let mut session = HumanTwinflameSessionV38::from_session(base, actor.into()).unwrap();
        let e = expected(&session);
        let old = HumanDecisionProjectorV9::new(e.acting_player)
            .project_current(&previous, e)
            .unwrap();
        let mut projector = HumanTwinflameProjectorV38::new(e.acting_player);
        let prompt = projector.project_current(&session, e).unwrap();
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert_eq!(prompt.actions, old.actions);
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        let prefix = if pay { "Pay" } else { "Decline to pay" };
        let input = request(&prompt, prefix);
        let snapshot = session.snapshot_v38();
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v38(&snapshot).unwrap();
        let mut replay = HumanTwinflameProjectorV38::new(e.acting_player);
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
            FastActorSessionV1::from_twinflame_v38_fixture_state(basic(actor, "Lightning Bolt"));
        let mut session =
            HumanTwinflameSessionV38::from_session(core, actor.opponent().into()).unwrap();
        let current = expected(&session);
        assert_eq!(current.acting_player, actor.into());
        assert!(session.public_observation(current).is_err());
        let mut projector = HumanTwinflameProjectorV38::new(actor.opponent().into());
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

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn genuine_scalar_owning_human_order_target_mask_restore_and_readonly_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for player_target in [false, true] {
            let state = crate::twinflame_observation_v1::tests::at_scalar(actor, player_target);
            let initial = serde_json::to_vec(&state).unwrap();
            let mut session = owned(state);
            assert_eq!(session.human_seat, actor.opponent().into());
            let mut projector = HumanTwinflameProjectorV38::new(actor.opponent().into());
            let current = expected(&session);
            let prompt = projector.project_current(&session, current).unwrap();
            assert_safe(&serde_json::to_value(&prompt).unwrap());
            let menu = prompt.state.extensions.prevention_menu.as_ref().unwrap();
            assert_eq!(menu.remaining_damage, 3);
            assert!(!menu.is_combat);
            assert_eq!(
                matches!(menu.recipient, HumanDamageRecipientV38::You),
                player_target
            );
            assert_eq!(prompt.state.extensions.multiplier_sources.len(), 1);
            assert!(
                prompt.state.extensions.multiplier_sources[0]
                    .doubles_controlled_source_damage_to_opponents
            );
            let snapshot = session.snapshot_v38();
            let doubled_request = prompt
                .actions
                .iter()
                .find(|a| a.label.contains("double the remaining damage"))
                .map(|a| HumanActionRequestV1 {
                    prompt_seq: prompt.prompt_seq,
                    action_index: a.action_index,
                })
                .unwrap();
            let receipt = projector
                .submit(&mut session, doubled_request.clone())
                .unwrap();
            let after = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector
                    .submit(&mut session, doubled_request.clone())
                    .unwrap(),
                receipt
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), after);
            let next = projector
                .project_current(&session, expected(&session))
                .unwrap();
            assert_eq!(
                next.state
                    .extensions
                    .prevention_menu
                    .as_ref()
                    .unwrap()
                    .remaining_damage,
                6
            );
            let shield = request(&next, "Apply ");
            session.restore_v38(&snapshot).unwrap();
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, shield),
                Err(Error::StaleDecision)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            let fresh = projector
                .project_current(&session, expected(&session))
                .unwrap();
            let index = fresh
                .actions
                .iter()
                .find(|a| a.label.contains("shield"))
                .unwrap()
                .action_index;
            projector
                .submit(
                    &mut session,
                    HumanActionRequestV1 {
                        prompt_seq: fresh.prompt_seq,
                        action_index: index,
                    },
                )
                .unwrap();
            assert_eq!(
                session.inner.twinflame_fixture_state().players[actor.opponent().index()].life,
                20
            );
            let mut malformed: GameState = serde_json::from_slice(&initial).unwrap();
            malformed.engine.pending_effect.as_mut().unwrap().choice = None;
            let before = serde_json::to_vec(&malformed).unwrap();
            let rejected = FastActorSessionV1::from_twinflame_v38_fixture_state(malformed);
            assert!(matches!(
                rejected.current_response(),
                FastActorResponseV1::Terminal(_)
            ));
            assert_eq!(
                serde_json::to_vec(rejected.twinflame_fixture_state()).unwrap(),
                before
            );
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn genuine_shared_shield_human_combat_allocation_completion_restore_and_readonly_both_seats() {
    use crate::twinflame_tyrant_v1::combat;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let state = crate::twinflame_observation_v1::tests::at_combat(actor);
        let saved = serde_json::to_vec(&state).unwrap();
        let lives = state.players.each_ref().map(|p| p.life);
        let mut session = owned(state);
        assert_eq!(session.human_seat, actor.opponent().into());
        let mut projector = HumanTwinflameProjectorV38::new(actor.opponent().into());
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        let allocation = prompt.state.extensions.combat_allocation.as_ref().unwrap();
        assert_eq!(allocation.packets.len(), 2);
        assert_eq!(allocation.packets[0].amount, 3);
        assert_eq!(allocation.packets[1].amount, 5);
        assert!(allocation
            .packets
            .iter()
            .all(|packet| matches!(packet.recipient, HumanDamageRecipientV38::You)));
        let snapshot = session.snapshot_v38();
        let allocation_request = request(&prompt, "Apply replacements to 3 combat damage");
        let receipt = projector
            .submit(&mut session, allocation_request.clone())
            .unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector
                .submit(&mut session, allocation_request.clone())
                .unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let menu = projector
            .project_current(&session, expected(&session))
            .unwrap();
        assert!(menu.state.extensions.combat_allocation.is_none());
        assert!(
            menu.state
                .extensions
                .prevention_menu
                .as_ref()
                .unwrap()
                .is_combat
        );
        assert_eq!(
            menu.state
                .extensions
                .prevention_menu
                .as_ref()
                .unwrap()
                .remaining_damage,
            3
        );
        let doubling = menu
            .actions
            .iter()
            .find(|a| a.label.contains("double the remaining damage"))
            .unwrap();
        let doubled = HumanActionRequestV1 {
            prompt_seq: menu.prompt_seq,
            action_index: doubling.action_index,
        };
        projector.submit(&mut session, doubled.clone()).unwrap();
        let current = session.inner.twinflame_fixture_state();
        assert!(!combat::has_pending(current));
        combat::validate(current).unwrap();
        assert_eq!(
            current.players[actor.opponent().index()].life,
            lives[actor.opponent().index()] - 13
        );
        assert_eq!(
            current.players[actor.index()].life,
            lives[actor.index()] + 13
        );
        session.restore_v38(&snapshot).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, doubled),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let fresh = projector
            .project_current(&session, expected(&session))
            .unwrap();
        projector
            .submit(
                &mut session,
                request(&fresh, "Apply replacements to 3 combat damage"),
            )
            .unwrap();
        let fresh = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let shield = fresh
            .actions
            .iter()
            .find(|a| a.label.contains("shield"))
            .unwrap();
        projector
            .submit(
                &mut session,
                HumanActionRequestV1 {
                    prompt_seq: fresh.prompt_seq,
                    action_index: shield.action_index,
                },
            )
            .unwrap();
        let current = session.inner.twinflame_fixture_state();
        assert!(!combat::has_pending(current));
        combat::validate(current).unwrap();
        assert_eq!(
            current.players[actor.opponent().index()].life,
            lives[actor.opponent().index()] - 10
        );
        assert_eq!(
            current.players[actor.index()].life,
            lives[actor.index()] + 10
        );
        for mutation in 0..2 {
            let mut malformed: GameState = serde_json::from_slice(&saved).unwrap();
            if mutation == 0 {
                malformed.engine.twinflame_combat_v38 = Default::default();
            } else {
                malformed
                    .engine
                    .twinflame_combat_v38
                    .0
                    .as_mut()
                    .unwrap()
                    .remaining
                    .reverse();
            }
            let bytes = serde_json::to_vec(&malformed).unwrap();
            let rejected = FastActorSessionV1::from_twinflame_v38_fixture_state(malformed);
            assert!(matches!(
                rejected.current_response(),
                FastActorResponseV1::Terminal(_)
            ));
            assert_eq!(
                serde_json::to_vec(rejected.twinflame_fixture_state()).unwrap(),
                bytes
            );
        }
    }
}
