use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state_for};
use crate::state::Zone;

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn expected(session: &HumanFaebloomSessionV43) -> FastActorDecisionV1 {
    let response = session.current_response();
    let FastActorResponseV1::Decision(decision) = response else {
        panic!("actual current decision: {response:?}");
    };
    decision
}

#[test]
fn default_and_standard_refuse_v43_without_mutating_predecessor_state() {
    if cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return;
    }
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        put(&mut state, actor, "Forest", Zone::Hand);
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let before = session.privileged_core_environment_hash();
        assert!(session.clone().into_faebloom_v43().is_err());
        assert_eq!(session.privileged_core_environment_hash(), before);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod limited {
    use super::*;
    fn ordinary(actor: PlayerId) -> HumanFaebloomSessionV43 {
        let mut state = ready_state_for(actor);
        put(&mut state, actor, "Forest", Zone::Hand);
        HumanFaebloomSessionV43::from_session(
            FastActorSessionV1::from_faebloom_v43_fixture_state(state),
            actor.into(),
        )
        .unwrap()
    }

    #[test]
    fn registered_real_ward_payment_preserves_predecessor_menu_and_private_extensions() {
        let (mut state, _, _) = crate::policy_observation_v6::tests::ward_multi_targeter_state();
        crate::policy_observation_v6::tests::reach_ward_payment(&mut state);
        let base = FastActorSessionV1::from_human_v8_fixture_state(state);
        let FastActorResponseV1::Decision(current) = base.current_response() else {
            panic!("actual Ward payment");
        };
        let actor = current.acting_player;
        let old = HumanRaidSessionV9::from_session(base.clone()).unwrap();
        let old = HumanDecisionProjectorV9::new(actor)
            .project_current(&old, current)
            .unwrap();
        let new = HumanFaebloomSessionV43::from_session(base, actor).unwrap();
        let new = HumanFaebloomProjectorV43::new(actor)
            .project_current(&new, current)
            .unwrap();
        assert_eq!(new.actions, old.actions);
        assert_eq!(new.state.public, old.state.public);
        assert_eq!(new.state.extensions.predecessor, old.state.extensions);
        assert_eq!(
            new.state.known_face_down_cards,
            old.state.known_face_down_cards
        );
        assert_eq!(new.state.poison_counters, old.state.poison_counters);
        assert_eq!(new.state.poison_prevention, old.state.poison_prevention);
        assert_eq!(
            new.state.creatures_attacked_this_turn,
            old.state.creatures_attacked_this_turn
        );
        assert_eq!(new.state.ninja_emblems, old.state.ninja_emblems);
        assert_eq!(new.state.restricted_mana, old.state.restricted_mana);
    }

    #[test]
    fn typed_actual_shroud_fact_is_explicit_while_predecessor_refuses_readonly_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            put(&mut state, actor, "Forest", Zone::Hand);
            let creature = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let binding = crate::effect::EffectObjectBinding {
                object: creature,
                expected_zone: Zone::Battlefield,
                expected_zone_change_count: state.objects.get(creature).zone_change_count,
            };
            crate::effect::install_temporary_boost(
                &mut state,
                binding,
                0,
                0,
                crate::card_def::Keywords::SHROUD,
            );
            let before = serde_json::to_vec(&state).unwrap();
            assert!(crate::rl::observe_v2(
                &state,
                &crate::surface_v2::HarnessSurfaceV2::new(),
                actor,
                0
            )
            .is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            let session = HumanFaebloomSessionV43::from_session(
                FastActorSessionV1::from_faebloom_v43_fixture_state(state),
                actor.into(),
            )
            .unwrap();
            let current = expected(&session);
            let observation = session.public_observation(current).unwrap();
            assert!(observation
                .creature_target_traits
                .iter()
                .any(|row| row.creature.arena_id == creature.0 && row.shroud));
            let menu = HumanFaebloomProjectorV43::new(actor.into())
                .project_current(&session, current)
                .unwrap();
            assert!(menu
                .state
                .extensions
                .creature_target_traits
                .iter()
                .any(|row| row.shroud));
        }
    }

    #[test]
    fn ordinary_owning_restore_retry_foreign_ordered_input_and_fixed_seat_are_readonly_both_seats()
    {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut session = ordinary(actor);
            let mut projector = HumanFaebloomProjectorV43::new(actor.into());
            let initial = expected(&session);
            let menu = projector.project_current(&session, initial).unwrap();
            assert!(menu.state.extensions.faebloom.spells.is_empty());
            assert!(menu.state.extensions.faebloom.reflexives.is_empty());
            let (observation, _, binding) = session
                .inner
                .faebloom_current_decision_input_v43(initial, actor.into())
                .unwrap();
            assert_eq!(observation.own_hand.len(), 1);
            let snapshot = session.snapshot_v43();
            let before = session.inner.privileged_core_environment_hash();
            let mut foreign = ordinary(actor);
            assert!(foreign.restore_v43(&snapshot).is_err());
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            for _ in 0..3 {
                session.restore_v43(&snapshot).unwrap();
                assert!(session
                    .inner
                    .faebloom_validate_bound_decision_v43(initial, actor.into(), &binding)
                    .is_err());
                assert!(projector
                    .submit(
                        &mut session,
                        HumanActionRequestV1 {
                            prompt_seq: menu.prompt_seq,
                            action_index: 0
                        }
                    )
                    .is_err());
                let now = expected(&session);
                let new_menu = projector.project_current(&session, now).unwrap();
                assert!(new_menu.prompt_seq > menu.prompt_seq);
                let (_, _, mut forged) = session
                    .inner
                    .faebloom_current_decision_input_v43(now, actor.into())
                    .unwrap();
                // Mutate only the incoming private binding, retaining all real state and journals.
                forged.families.reverse();
                let before = session.inner.privileged_core_environment_hash();
                assert!(session
                    .inner
                    .faebloom_validate_bound_decision_v43(now, actor.into(), &forged)
                    .is_err());
                assert_eq!(session.inner.privileged_core_environment_hash(), before);
                assert!(session
                    .public_observation(FastActorDecisionV1 {
                        acting_player: actor.opponent().into(),
                        ..now
                    })
                    .is_err());
            }
            let now = expected(&session);
            let menu = projector.project_current(&session, now).unwrap();
            let land = menu
                .actions
                .iter()
                .find(|a| a.label.contains("Play") && a.label.contains("Forest"))
                .unwrap();
            let request = HumanActionRequestV1 {
                prompt_seq: menu.prompt_seq,
                action_index: land.action_index,
            };
            let accepted = projector.submit(&mut session, request.clone()).unwrap();
            assert_eq!(projector.submit(&mut session, request).unwrap(), accepted);
            let json = serde_json::to_string(&menu).unwrap();
            for forbidden in [
                "arena_id",
                "card_db_hash",
                "root_sha256",
                "source_contract",
                "event_history",
                "journal",
            ] {
                assert!(!json.contains(forbidden), "{forbidden}");
            }
        }
    }

    fn submit_label(
        session: &mut HumanFaebloomSessionV43,
        projector: &mut HumanFaebloomProjectorV43,
        prefix: &str,
    ) {
        let current = expected(session);
        let menu = projector.project_current(session, current).unwrap();
        let action = menu
            .actions
            .iter()
            .find(|a| a.label.starts_with(prefix))
            .unwrap_or_else(|| panic!("missing {prefix}: {menu:?}"));
        projector
            .submit(
                session,
                HumanActionRequestV1 {
                    prompt_seq: menu.prompt_seq,
                    action_index: action.action_index,
                },
            )
            .unwrap();
    }
    fn pass(
        session: &mut HumanFaebloomSessionV43,
        projector: &mut HumanFaebloomProjectorV43,
        actor: PlayerId,
    ) {
        let current = expected(session);
        if current.acting_player == actor.into() {
            submit_label(session, projector, "Pass");
        } else {
            assert!(session.public_observation(current).is_err());
            session
                .step_opponent_with(current, |_, actions| {
                    actions
                        .iter()
                        .position(|a| matches!(a, ActionSemanticV1::Pass { .. }))
                        .map(|i| i as u32)
                        .ok_or_else(|| {
                            crate::rl::RlContractError("actual opponent Pass absent".into())
                        })
                })
                .unwrap();
        }
    }

    #[test]
    fn paid_targetless_creation_separate_reflexive_source_targets_owning_restore_privacy_and_real_tap_both_seats(
    ) {
        use crate::mana::ManaColor;
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = crate::extra_turn_v1::tests::ready(actor);
            let trick = put(&mut state, actor, "Faebloom Trick", Zone::Hand);
            let target = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
            let mut session = HumanFaebloomSessionV43::from_session(
                FastActorSessionV1::from_faebloom_v43_fixture_state(state),
                actor.into(),
            )
            .unwrap();
            let mut projector = HumanFaebloomProjectorV43::new(actor.into());
            submit_label(&mut session, &mut projector, "Begin casting Faebloom Trick");
            let paid = session
                .inner
                .faebloom_fixture_state()
                .stack
                .last()
                .unwrap()
                .clone();
            assert_eq!(paid.source, trick);
            assert!(paid.targets.is_empty());
            assert_eq!(
                session.inner.faebloom_fixture_state().players[actor.index()].mana_pool,
                [0; 6]
            );
            let mut reached = false;
            for _ in 0..12 {
                let current = expected(&session);
                if current.acting_player == actor.into() {
                    let observation = session.public_observation(current).unwrap();
                    if observation
                        .faebloom
                        .reflexives
                        .first()
                        .is_some_and(|r| r.stage == crate::faebloom_public_v43::StageV43::Waiting)
                    {
                        reached = true;
                        assert_eq!(
                            observation.faebloom.spells[0].original_source.zone,
                            Zone::Stack
                        );
                        assert_eq!(
                            observation.faebloom.spells[0]
                                .created_tokens
                                .iter()
                                .flatten()
                                .count(),
                            2
                        );
                        assert_eq!(
                            observation
                                .projection
                                .surface
                                .engine_context
                                .pending_triggers[0]
                                .source
                                .as_ref(),
                            Some(&observation.faebloom.spells[0].original_source)
                        );
                        break;
                    }
                }
                pass(&mut session, &mut projector, actor);
            }
            assert!(reached);
            assert_eq!(
                session
                    .inner
                    .faebloom_fixture_state()
                    .objects
                    .get(trick)
                    .zone,
                Zone::Graveyard
            );
            assert!(
                !session
                    .inner
                    .faebloom_fixture_state()
                    .objects
                    .get(target)
                    .tapped
            );
            let snapshot = session.snapshot_v43();
            let current = expected(&session);
            let (_, _, original_binding) = session
                .inner
                .faebloom_current_decision_input_v43(current, actor.into())
                .unwrap();
            let menu = projector.project_current(&session, current).unwrap();
            let json = serde_json::to_string(&menu).unwrap();
            assert!(
                !json.contains("Lightning Bolt")
                    && !json.contains("arena_id")
                    && !json.contains("event_history")
            );
            assert!(menu.actions.iter().all(
                |a| a.label.contains("reflexive trigger") && a.label.contains("Llanowar Elves")
            ));
            session.restore_v43(&snapshot).unwrap();
            assert!(session
                .inner
                .faebloom_validate_bound_decision_v43(current, actor.into(), &original_binding)
                .is_err());
            let now = expected(&session);
            let menu = projector.project_current(&session, now).unwrap();
            projector
                .submit(
                    &mut session,
                    HumanActionRequestV1 {
                        prompt_seq: menu.prompt_seq,
                        action_index: 0,
                    },
                )
                .unwrap();
            let finalized = session
                .inner
                .faebloom_fixture_state()
                .stack
                .last()
                .unwrap()
                .clone();
            assert_eq!(
                finalized.targets,
                vec![crate::state::Target::Object(target)]
            );
            let finalized_snapshot = session.snapshot_v43();
            for _ in 0..12 {
                if !session
                    .inner
                    .faebloom_fixture_state()
                    .stack
                    .iter()
                    .any(|r| r.v4.stack_item_id == finalized.v4.stack_item_id)
                {
                    break;
                }
                pass(&mut session, &mut projector, actor);
            }
            assert!(
                session
                    .inner
                    .faebloom_fixture_state()
                    .objects
                    .get(target)
                    .tapped
            );
            let first = session.inner.privileged_core_environment_hash();
            session.restore_v43(&finalized_snapshot).unwrap();
            for _ in 0..12 {
                if !session
                    .inner
                    .faebloom_fixture_state()
                    .stack
                    .iter()
                    .any(|r| r.v4.stack_item_id == finalized.v4.stack_item_id)
                {
                    break;
                }
                pass(&mut session, &mut projector, actor);
            }
            assert_eq!(session.inner.privileged_core_environment_hash(), first);
            crate::faebloom_trick_v43::validate_state(session.inner.faebloom_fixture_state())
                .unwrap();
        }
    }
}
