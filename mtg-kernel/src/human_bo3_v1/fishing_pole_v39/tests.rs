use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state_for};
use crate::state::{GameState, Zone};

fn expected(session: &HumanFishingPoleSessionV39) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("real fixture must reach a decision");
    };
    expected
}
fn owned(state: GameState) -> HumanFishingPoleSessionV39 {
    let base = FastActorSessionV1::from_fishing_pole_v39_fixture_state(state);
    let FastActorResponseV1::Decision(expected) = base.current_response() else {
        panic!("actual actor decision");
    };
    HumanFishingPoleSessionV39::from_session(base, expected.acting_player).unwrap()
}

fn basic(actor: PlayerId, hidden: &str) -> GameState {
    let mut state = ready_state_for(actor);
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    state
}
fn request(prompt: &HumanDecisionFishingPoleV39, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanFishingPoleSessionV39::from_session(base, actor.into()).unwrap();
        let current = expected(&session);
        let old = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&predecessor, current)
            .unwrap();
        let mut projector = HumanFishingPoleProjectorV39::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.actions, old.actions);
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert!(prompt.state.extensions.operations.is_empty());
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let hidden = owned(basic(actor, "Island"));
        assert_eq!(
            HumanFishingPoleProjectorV39::new(actor.into())
                .project_current(&hidden, expected(&hidden))
                .unwrap(),
            prompt
        );
        assert_eq!(
            HumanFishingPoleProjectorV39::new(actor.opponent().into())
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
        let mut projector = HumanFishingPoleProjectorV39::new(actor.into());
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let saved = session.snapshot_v39();
        let mut stale = HumanFishingPoleProjectorV39::new(actor.into());
        stale.project_current(&session, expected(&session)).unwrap();
        let input = request(&prompt, "Play Forest");
        projector.submit(&mut session, input.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_input = request(&future, "Pass priority");
        session.restore_v39(&saved).unwrap();
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
        let foreign = owned(basic(actor, "Lightning Bolt")).snapshot_v39();
        assert_eq!(session.restore_v39(&foreign), Err(Error::StaleDecision));
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v39(&saved),
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
        let mut session = HumanFishingPoleSessionV39::from_session(base, actor.into()).unwrap();
        let e = expected(&session);
        let old = HumanDecisionProjectorV9::new(e.acting_player)
            .project_current(&previous, e)
            .unwrap();
        let mut projector = HumanFishingPoleProjectorV39::new(e.acting_player);
        let prompt = projector.project_current(&session, e).unwrap();
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert_eq!(prompt.actions, old.actions);
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        let prefix = if pay { "Pay" } else { "Decline to pay" };
        let input = request(&prompt, prefix);
        let snapshot = session.snapshot_v39();
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v39(&snapshot).unwrap();
        let mut replay = HumanFishingPoleProjectorV39::new(e.acting_player);
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
            FastActorSessionV1::from_fishing_pole_v39_fixture_state(basic(actor, "Lightning Bolt"));
        let mut session =
            HumanFishingPoleSessionV39::from_session(core, actor.opponent().into()).unwrap();
        let current = expected(&session);
        assert_eq!(current.acting_player, actor.into());
        assert!(session.public_observation(current).is_err());
        let mut projector = HumanFishingPoleProjectorV39::new(actor.opponent().into());
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
    use crate::engine;
    use crate::event::{self, ProposedEvent};
    use crate::mana::ManaColor;
    use crate::state::Step;
    fn ready(
        actor: PlayerId,
        hidden: &str,
    ) -> (GameState, crate::ids::ObjectId, crate::ids::ObjectId) {
        let forest = crate::card_def::card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            |_| "Forest".into(),
            3939,
            actor,
        );
        state.step = Step::Main1;
        for seat in [actor, actor.opponent()] {
            put(&mut state, seat, "Forest", Zone::Battlefield);
        }
        put(&mut state, actor.opponent(), hidden, Zone::Hand);
        let host = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let pole = put(&mut state, actor, "Fishing Pole", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
        (state, host, pole)
    }
    fn apply(
        session: &mut HumanFishingPoleSessionV39,
        projector: &mut HumanFishingPoleProjectorV39,
        prefix: &str,
    ) {
        let prompt = projector
            .project_current(session, expected(session))
            .unwrap();
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        projector.submit(session, request(&prompt, prefix)).unwrap();
    }
    fn resolve(
        session: &mut HumanFishingPoleSessionV39,
        projector: &mut HumanFishingPoleProjectorV39,
    ) {
        for _ in 0..30 {
            if session
                .inner
                .fishing_pole_fixture_state_mut()
                .stack
                .is_empty()
                && session
                    .inner
                    .fishing_pole_fixture_state_mut()
                    .engine
                    .pending_activation
                    .is_none()
                && session
                    .inner
                    .fishing_pole_fixture_state_mut()
                    .engine
                    .pending_triggers
                    .is_empty()
            {
                return;
            }
            let current = expected(session);
            if current.acting_player == session.human_seat {
                apply(session, projector, "Pass priority");
            } else {
                session
                    .step_opponent_with(current, |_, actions| {
                        actions
                            .iter()
                            .position(|a| matches!(a, crate::rl::ActionSemanticV1::Pass { .. }))
                            .map(|i| u32::try_from(i).unwrap())
                            .ok_or_else(|| {
                                crate::rl::RlContractError("real opponent Pass missing".into())
                            })
                    })
                    .unwrap();
            }
        }
        panic!("actual Human Fishing stack did not finish");
    }
    fn equipped(
        actor: PlayerId,
        hidden: &str,
    ) -> (
        HumanFishingPoleSessionV39,
        HumanFishingPoleProjectorV39,
        crate::ids::ObjectId,
        crate::ids::ObjectId,
    ) {
        let (state, host, pole) = ready(actor, hidden);
        let mut session = owned(state);
        let mut projector = HumanFishingPoleProjectorV39::new(actor.into());
        apply(&mut session, &mut projector, "Begin casting Fishing Pole");
        resolve(&mut session, &mut projector);
        assert_eq!(
            session
                .inner
                .fishing_pole_fixture_state_mut()
                .objects
                .get(pole)
                .zone,
            Zone::Battlefield
        );
        apply(&mut session, &mut projector, "Activate Fishing Pole");
        apply(&mut session, &mut projector, "Target Llanowar Elves");
        resolve(&mut session, &mut projector);
        assert_eq!(
            session
                .inner
                .fishing_pole_fixture_state_mut()
                .objects
                .get(pole)
                .v4
                .attached_to
                .unwrap()
                .object,
            host
        );
        (session, projector, host, pole)
    }
    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn real_cast_equip_named_bait_menu_restore_retry_and_old_mode_refusal_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut session, mut projector, host, pole) = equipped(actor, "Lightning Bolt");
            assert!(crate::rl::observe_policy_v8(
                session.inner.fishing_pole_fixture_state_mut(),
                &crate::policy_surface_v5::PolicySurfaceV5::new_for_session(),
                actor,
                0,
                0,
                0,
                1
            )
            .is_err());
            let initial = projector
                .project_current(&session, expected(&session))
                .unwrap();
            assert_eq!(initial.state.extensions.poles[0].bait, 0);
            let start = request(
                &initial,
                "Activate Fishing Pole-granted ability of Llanowar Elves",
            );
            projector.submit(&mut session, start.clone()).unwrap();
            let cost = projector
                .project_current(&session, expected(&session))
                .unwrap();
            let saved = session.snapshot_v39();
            let tap = request(&cost, "Tap Fishing Pole");
            let receipt = projector.submit(&mut session, tap.clone()).unwrap();
            let paid = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, tap.clone()).unwrap(),
                receipt
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), paid);
            assert!(
                session
                    .inner
                    .fishing_pole_fixture_state_mut()
                    .objects
                    .get(host)
                    .tapped
            );
            assert!(
                session
                    .inner
                    .fishing_pole_fixture_state_mut()
                    .objects
                    .get(pole)
                    .tapped
            );
            session.restore_v39(&saved).unwrap();
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, tap),
                Err(Error::StaleDecision)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            apply(&mut session, &mut projector, "Tap Fishing Pole");
            resolve(&mut session, &mut projector);
            assert_eq!(
                session
                    .inner
                    .fishing_pole_fixture_state_mut()
                    .objects
                    .get(pole)
                    .counters
                    .bait,
                1
            );
            let visible = projector
                .project_current(&session, expected(&session))
                .unwrap();
            assert_eq!(visible.state.extensions.operations.len(), 1);
            assert_eq!(visible.state.extensions.operations[0].bait_after, Some(1));
            assert_safe(&serde_json::to_value(visible).unwrap());
        }
    }
    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn genuine_untap_fish_public_lifecycle_departed_host_historical_relation_and_private_forgeries_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut session, mut projector, host, pole) = equipped(actor, "Island");
            apply(
                &mut session,
                &mut projector,
                "Activate Fishing Pole-granted ability",
            );
            apply(&mut session, &mut projector, "Tap Fishing Pole");
            resolve(&mut session, &mut projector);
            let mut core = session.inner.fishing_pole_fixture_state_mut().clone();
            assert!(engine::attempt_untap(&mut core, host));
            event::propose_and_commit(&mut core, ProposedEvent::zone_change(host, Zone::Hand));
            let mut session = owned(core);
            let mut projector = HumanFishingPoleProjectorV39::new(actor.into());
            let prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            assert_safe(&serde_json::to_value(&prompt).unwrap());
            let trigger = prompt
                .state
                .extensions
                .operations
                .iter()
                .find(|row| row.trigger)
                .unwrap();
            assert!(trigger.original_host.current_visible_card.is_none());
            let saved = session.snapshot_v39();
            let pass = request(&prompt, "Pass priority");
            session.inner.fishing_pole_fixture_state_mut().stack.clear();
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, pass),
                Err(Error::StaleDecision)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            session.restore_v39(&saved).unwrap();
            resolve(&mut session, &mut projector);
            let facts = session
                .public_observation(expected(&session))
                .unwrap()
                .facts;
            let trigger = facts.operations.iter().find(|row| row.trigger).unwrap();
            assert_eq!(
                trigger.stage,
                crate::fishing_pole_public_v39::StageV39::Resolved
            );
            assert!(trigger.created_fish.is_some());
            assert_eq!(
                session
                    .inner
                    .fishing_pole_fixture_state_mut()
                    .objects
                    .get(pole)
                    .counters
                    .bait,
                0
            );
            let malformed = session.snapshot_v39();
            let core = session.inner.fishing_pole_fixture_state_mut();
            let original = core.engine.event_history.clone();
            core.objects.get_mut(pole).counters.bait = 1;
            let bad = session.snapshot_v39();
            session.restore_v39(&malformed).unwrap();
            let before = session.inner.privileged_core_environment_hash();
            let generation = session.authority.generation;
            assert_eq!(session.restore_v39(&bad), Err(Error::StaleDecision));
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            assert_eq!(session.authority.generation, generation);
            assert_eq!(
                session
                    .inner
                    .fishing_pole_fixture_state_mut()
                    .engine
                    .event_history,
                original
            );
        }
    }
    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn actual_attached_fishing_hidden_hand_substitution_and_foreign_menu_readonly_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut first, mut projector, _, _) = equipped(actor, "Lightning Bolt");
            let (second, _, _, _) = equipped(actor, "Island");
            let prompt = projector.project_current(&first, expected(&first)).unwrap();
            let other = HumanFishingPoleProjectorV39::new(actor.into())
                .project_current(&second, expected(&second))
                .unwrap();
            assert_eq!(prompt, other);
            let foreign = second.snapshot_v39();
            let before = first.inner.privileged_core_environment_hash();
            assert_eq!(first.restore_v39(&foreign), Err(Error::StaleDecision));
            assert_eq!(first.inner.privileged_core_environment_hash(), before);
            assert!(first
                .public_observation(FastActorDecisionV1 {
                    acting_player: actor.opponent().into(),
                    ..expected(&first)
                })
                .is_err());
            assert_safe(&serde_json::to_value(prompt).unwrap());
        }
    }
}
