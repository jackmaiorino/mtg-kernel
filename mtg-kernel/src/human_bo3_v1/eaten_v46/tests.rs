use super::*;
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use crate::ids::PlayerId;
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use crate::policy_observation_v6::tests::{put, ready_state_for};
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use crate::state::Zone;

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn expected(session: &HumanEatenSessionV46) -> FastActorDecisionV1 {
    let response = session.current_response();
    let FastActorResponseV1::Decision(decision) = response else {
        panic!("actual current decision: {response:?}");
    };
    decision
}

#[test]
fn default_and_standard_refuse_v46_without_mutating_predecessor_state() {
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
        assert!(session.clone().into_eaten_v46().is_err());
        assert_eq!(session.privileged_core_environment_hash(), before);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod limited {
    use super::*;
    fn ordinary(actor: PlayerId) -> HumanEatenSessionV46 {
        let mut state = ready_state_for(actor);
        put(&mut state, actor, "Forest", Zone::Hand);
        HumanEatenSessionV46::from_session(
            FastActorSessionV1::from_eaten_v46_fixture_state(state),
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
        let new = HumanEatenSessionV46::from_session(base, actor).unwrap();
        let new = HumanEatenProjectorV46::new(actor)
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
            let session = HumanEatenSessionV46::from_session(
                FastActorSessionV1::from_eaten_v46_fixture_state(state),
                actor.into(),
            )
            .unwrap();
            let current = expected(&session);
            let observation = session.public_observation(current).unwrap();
            assert!(observation
                .creature_target_traits
                .iter()
                .any(|row| row.creature.arena_id == creature.0 && row.shroud));
            let menu = HumanEatenProjectorV46::new(actor.into())
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
            let mut projector = HumanEatenProjectorV46::new(actor.into());
            let initial = expected(&session);
            let menu = projector.project_current(&session, initial).unwrap();
            assert!(menu.state.extensions.faebloom.spells.is_empty());
            assert!(menu.state.extensions.faebloom.reflexives.is_empty());
            let (observation, _, binding) = session
                .inner
                .eaten_current_decision_input_v46(initial, actor.into())
                .unwrap();
            assert_eq!(observation.own_hand.len(), 1);
            let snapshot = session.snapshot_v46();
            let before = session.inner.privileged_core_environment_hash();
            let mut foreign = ordinary(actor);
            assert!(foreign.restore_v46(&snapshot).is_err());
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            for _ in 0..3 {
                session.restore_v46(&snapshot).unwrap();
                assert!(session
                    .inner
                    .eaten_validate_bound_decision_v46(initial, actor.into(), &binding)
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
                    .eaten_current_decision_input_v46(now, actor.into())
                    .unwrap();
                // Mutate only the incoming private binding, retaining all real state and journals.
                forged.families.reverse();
                let before = session.inner.privileged_core_environment_hash();
                assert!(session
                    .inner
                    .eaten_validate_bound_decision_v46(now, actor.into(), &forged)
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
        session: &mut HumanEatenSessionV46,
        projector: &mut HumanEatenProjectorV46,
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
        session: &mut HumanEatenSessionV46,
        projector: &mut HumanEatenProjectorV46,
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
            let mut session = HumanEatenSessionV46::from_session(
                FastActorSessionV1::from_eaten_v46_fixture_state(state),
                actor.into(),
            )
            .unwrap();
            let mut projector = HumanEatenProjectorV46::new(actor.into());
            submit_label(&mut session, &mut projector, "Begin casting Faebloom Trick");
            let paid = session
                .inner
                .eaten_fixture_state()
                .stack
                .last()
                .unwrap()
                .clone();
            assert_eq!(paid.source, trick);
            assert!(paid.targets.is_empty());
            assert_eq!(
                session.inner.eaten_fixture_state().players[actor.index()].mana_pool,
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
                session.inner.eaten_fixture_state().objects.get(trick).zone,
                Zone::Graveyard
            );
            assert!(
                !session
                    .inner
                    .eaten_fixture_state()
                    .objects
                    .get(target)
                    .tapped
            );
            let snapshot = session.snapshot_v46();
            let current = expected(&session);
            let (_, _, original_binding) = session
                .inner
                .eaten_current_decision_input_v46(current, actor.into())
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
            session.restore_v46(&snapshot).unwrap();
            assert!(session
                .inner
                .eaten_validate_bound_decision_v46(current, actor.into(), &original_binding)
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
                .eaten_fixture_state()
                .stack
                .last()
                .unwrap()
                .clone();
            assert_eq!(
                finalized.targets,
                vec![crate::state::Target::Object(target)]
            );
            let finalized_snapshot = session.snapshot_v46();
            for _ in 0..12 {
                if !session
                    .inner
                    .eaten_fixture_state()
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
                    .eaten_fixture_state()
                    .objects
                    .get(target)
                    .tapped
            );
            let first = session.inner.privileged_core_environment_hash();
            session.restore_v46(&finalized_snapshot).unwrap();
            for _ in 0..12 {
                if !session
                    .inner
                    .eaten_fixture_state()
                    .stack
                    .iter()
                    .any(|r| r.v4.stack_item_id == finalized.v4.stack_item_id)
                {
                    break;
                }
                pass(&mut session, &mut projector, actor);
            }
            assert_eq!(session.inner.privileged_core_environment_hash(), first);
            crate::faebloom_trick_v43::validate_state(session.inner.eaten_fixture_state()).unwrap();
        }
    }
    fn select_contains(
        session: &mut HumanEatenSessionV46,
        projector: &mut HumanEatenProjectorV46,
        phrase: &str,
    ) {
        let menu = projector
            .project_current(session, expected(session))
            .unwrap();
        let actions = menu
            .actions
            .iter()
            .filter(|a| a.label.contains(phrase))
            .collect::<Vec<_>>();
        assert_eq!(actions.len(), 1, "exact genuine action: {menu:?}");
        projector
            .submit(
                session,
                HumanActionRequestV1 {
                    prompt_seq: menu.prompt_seq,
                    action_index: actions[0].action_index,
                },
            )
            .unwrap();
    }
    fn eaten_state(
        actor: PlayerId,
    ) -> (
        crate::state::GameState,
        crate::ids::ObjectId,
        crate::ids::ObjectId,
        crate::ids::ObjectId,
    ) {
        let forest = crate::card_def::card_id_by_name("Forest").unwrap();
        let mut state = crate::state::GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 20],
            &[forest; 20],
            |id| {
                crate::card_def::CARD_DEFS[usize::from(id)]
                    .object_name
                    .into()
            },
            110_460,
            actor,
        );
        state.step = crate::state::Step::Main1;
        // Real mana activations keep both response windows observable after costs.
        for seat in [PlayerId::P0, PlayerId::P1] {
            let land = put(&mut state, seat, "Forest", Zone::Hand);
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::zone_change(land, Zone::Battlefield),
            );
        }
        put(&mut state, actor, "Forest", Zone::Hand);

        let spell = put(&mut state, actor, "Eaten Alive", Zone::Hand);
        let sacrifice = put(&mut state, actor, "Savannah Lions", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::zone_change(sacrifice, Zone::Battlefield),
        );
        let target = put(&mut state, actor.opponent(), "Llanowar Elves", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::zone_change(target, Zone::Battlefield),
        );
        put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::B.pool_index()] = 2;
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] = 3;
        (state, spell, target, sacrifice)
    }
    #[test]
    #[ignore = "requires serial Eaten Alive registration and native qualification"]
    fn actual_mandatory_two_branches_paid_publication_human_restore_and_exile_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for pay_mana in [false, true] {
                let (state, spell, target, sacrifice) = eaten_state(actor);
                let mut session = HumanEatenSessionV46::from_session(
                    FastActorSessionV1::from_eaten_v46_fixture_state(state),
                    actor.into(),
                )
                .unwrap();
                let mut projector = HumanEatenProjectorV46::new(actor.into());
                select_contains(&mut session, &mut projector, "Begin casting Eaten Alive");
                select_contains(&mut session, &mut projector, "Llanowar Elves");
                let old = expected(&session);
                let public = session.public_observation(old).unwrap();
                let pending = public.eaten.pending.as_ref().unwrap();
                assert_eq!(
                    pending.payable_branches,
                    [
                        crate::eaten_public_v46::BranchV46::SacrificeCreature,
                        crate::eaten_public_v46::BranchV46::AdditionalMana3B
                    ]
                );
                let menu = projector.project_current(&session, old).unwrap();
                let json = serde_json::to_string(&menu).unwrap();
                assert!(
                    !json.contains("Lightning Bolt")
                        && !json.contains("arena_id")
                        && !json.contains("event_history")
                        && !json.contains("stack_item_id")
                );
                let saved = session.snapshot_v46();
                let old_request = HumanActionRequestV1 {
                    prompt_seq: menu.prompt_seq,
                    action_index: 0,
                };
                for _ in 0..3 {
                    session.restore_v46(&saved).unwrap();
                    let before = session.inner.privileged_core_environment_hash();
                    assert!(projector
                        .submit(
                            &mut session,
                            HumanActionRequestV1 {
                                prompt_seq: old_request.prompt_seq,
                                action_index: old_request.action_index
                            }
                        )
                        .is_err());
                    assert_eq!(session.inner.privileged_core_environment_hash(), before);
                }
                select_contains(
                    &mut session,
                    &mut projector,
                    if pay_mana {
                        "additional cost: choose printed {3}{B}"
                    } else {
                        "additional cost: sacrifice one controlled creature"
                    },
                );
                if !pay_mana {
                    select_contains(
                        &mut session,
                        &mut projector,
                        "additional cost: sacrifice Savannah Lions",
                    );
                }
                let paid = session.public_observation(expected(&session)).unwrap();
                let receipt = paid.eaten.payments.last().unwrap();
                assert_eq!(
                    receipt.branch,
                    if pay_mana {
                        crate::eaten_public_v46::BranchV46::AdditionalMana3B
                    } else {
                        crate::eaten_public_v46::BranchV46::SacrificeCreature
                    }
                );
                assert!(!receipt.original_spell_departed);
                assert_eq!(
                    session
                        .inner
                        .eaten_fixture_state()
                        .objects
                        .get(sacrifice)
                        .zone,
                    if pay_mana {
                        Zone::Battlefield
                    } else {
                        Zone::Graveyard
                    }
                );
                assert_eq!(
                    session.inner.eaten_fixture_state().players[actor.index()].mana_pool
                        [crate::mana::ManaColor::B.pool_index()],
                    if pay_mana { 0 } else { 1 }
                );
                let paid_snapshot = session.snapshot_v46();
                for _ in 0..24 {
                    if !session
                        .inner
                        .eaten_fixture_state()
                        .stack
                        .iter()
                        .any(|item| item.source == spell)
                    {
                        break;
                    }
                    pass(&mut session, &mut projector, actor);
                }
                assert_eq!(
                    session.inner.eaten_fixture_state().objects.get(target).zone,
                    Zone::Exile
                );
                assert_eq!(
                    session.inner.eaten_fixture_state().objects.get(spell).zone,
                    Zone::Graveyard
                );
                let first = session.inner.privileged_core_environment_hash();
                session.restore_v46(&paid_snapshot).unwrap();
                for _ in 0..24 {
                    if !session
                        .inner
                        .eaten_fixture_state()
                        .stack
                        .iter()
                        .any(|item| item.source == spell)
                    {
                        break;
                    }
                    pass(&mut session, &mut projector, actor);
                }
                assert_eq!(session.inner.privileged_core_environment_hash(), first);
            }
        }
    }
    #[test]
    #[ignore = "requires serial Eaten Alive registration and native qualification"]
    fn actual_full_ajani_recipient_mixed_facts_cost_menu_and_paid_exile_restore_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, spell, _, _) = eaten_state(actor);
            // Real registered planeswalker entry primitive, not a synthetic card or cast receipt.
            let ajani = put(
                &mut state,
                actor.opponent(),
                "Ajani, Caller of the Pride",
                Zone::Hand,
            );
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::zone_change(ajani, Zone::Battlefield),
            );
            assert_eq!(crate::planeswalker_v1::loyalty(&state, ajani), Some(4));
            let mut session = HumanEatenSessionV46::from_session(
                FastActorSessionV1::from_eaten_v46_fixture_state(state),
                actor.into(),
            )
            .unwrap();
            let mut projector = HumanEatenProjectorV46::new(actor.into());
            let public = session.public_observation(expected(&session)).unwrap();
            assert!(public
                .ajani
                .permanents
                .iter()
                .any(|row| row.source.arena_id == ajani.0 && row.loyalty == 4));
            select_contains(&mut session, &mut projector, "Begin casting Eaten Alive");
            select_contains(&mut session, &mut projector, "Ajani, Caller of the Pride");
            select_contains(
                &mut session,
                &mut projector,
                "additional cost: choose printed {3}{B}",
            );
            let saved = session.snapshot_v46();
            for replay in 0..2 {
                if replay != 0 {
                    session.restore_v46(&saved).unwrap();
                }
                for _ in 0..24 {
                    if !session
                        .inner
                        .eaten_fixture_state()
                        .stack
                        .iter()
                        .any(|item| item.source == spell)
                    {
                        break;
                    }
                    pass(&mut session, &mut projector, actor);
                }
                assert_eq!(
                    session.inner.eaten_fixture_state().objects.get(ajani).zone,
                    Zone::Exile
                );
                assert_eq!(
                    session.inner.eaten_fixture_state().objects.get(spell).zone,
                    Zone::Graveyard
                );
                crate::engine::mandatory_cast_cost_v1::validate_state(
                    session.inner.eaten_fixture_state(),
                )
                .unwrap();
            }
        }
    }
    #[test]
    #[ignore = "requires serial Eaten Alive registration and native qualification"]
    fn actual_paid_eaten_history_and_typed_kicked_divine_share_full_mixed_owning_input_both_seats()
    {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, spell, target, _) = eaten_state(actor);
            let divine = put(&mut state, actor, "Divine Resilience", Zone::Hand);
            for _ in 0..2 {
                let creature = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
                crate::event::propose_and_commit(
                    &mut state,
                    crate::event::ProposedEvent::zone_change(creature, Zone::Battlefield),
                );
            }
            state.players[actor.index()].mana_pool[crate::mana::ManaColor::W.pool_index()] = 2;
            state.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] = 5;
            let mut session = HumanEatenSessionV46::from_session(
                FastActorSessionV1::from_eaten_v46_fixture_state(state),
                actor.into(),
            )
            .unwrap();
            let mut projector = HumanEatenProjectorV46::new(actor.into());
            select_contains(&mut session, &mut projector, "Begin casting Eaten Alive");
            // Choose the original opponent recipient from the complete private candidate binding.
            let current = expected(&session);
            let (observation, actions, binding) = session
                .inner
                .eaten_current_decision_input_v46(current, actor.into())
                .unwrap();
            let index = actions.iter().position(|a| matches!(a, ActionSemanticV1::ChooseTarget { target: crate::rl::TargetRefV1::Object { object }, .. } if object.arena_id == target.0)).unwrap();
            let (_, _, engine_indexes) =
                project_base(&observation, &actions, actor.into(), &binding.families).unwrap();
            let human_index = engine_indexes
                .iter()
                .position(|candidate| *candidate == index as u32)
                .unwrap();
            let menu = projector.project_current(&session, current).unwrap();
            projector
                .submit(
                    &mut session,
                    HumanActionRequestV1 {
                        prompt_seq: menu.prompt_seq,
                        action_index: human_index as u32,
                    },
                )
                .unwrap();
            select_contains(
                &mut session,
                &mut projector,
                "additional cost: choose printed {3}{B}",
            );
            for _ in 0..24 {
                if !session
                    .inner
                    .eaten_fixture_state()
                    .stack
                    .iter()
                    .any(|item| item.source == spell)
                {
                    break;
                }
                pass(&mut session, &mut projector, actor);
            }
            assert_eq!(
                session.inner.eaten_fixture_state().objects.get(target).zone,
                Zone::Exile
            );
            select_contains(
                &mut session,
                &mut projector,
                "Begin casting Divine Resilience",
            );
            select_contains(&mut session, &mut projector, ": pay kicker");
            for _ in 0..3 {
                let menu = projector
                    .project_current(&session, expected(&session))
                    .unwrap();
                let option = menu
                    .actions
                    .iter()
                    .find(|a| a.label.contains(": select "))
                    .unwrap();
                projector
                    .submit(
                        &mut session,
                        HumanActionRequestV1 {
                            prompt_seq: menu.prompt_seq,
                            action_index: option.action_index,
                        },
                    )
                    .unwrap();
            }
            let current = expected(&session);
            let public = session.public_observation(current).unwrap();
            assert_eq!(public.divine.pending.as_ref().unwrap().selected.len(), 3);
            assert_eq!(public.eaten.payments.len(), 1);
            assert!(public.eaten.payments[0].original_spell_departed);
            let save = session.snapshot_v46();
            session.restore_v46(&save).unwrap();
            let restored = session.public_observation(expected(&session)).unwrap();
            assert_eq!(restored.eaten, public.eaten);
            assert_eq!(restored.divine, public.divine);
            select_contains(&mut session, &mut projector, "finish with 3");
            for _ in 0..24 {
                if !session
                    .inner
                    .eaten_fixture_state()
                    .stack
                    .iter()
                    .any(|item| item.source == divine)
                {
                    break;
                }
                pass(&mut session, &mut projector, actor);
            }
            assert_eq!(
                session.inner.eaten_fixture_state().objects.get(divine).zone,
                Zone::Graveyard
            );
            for id in session.inner.eaten_fixture_state().players[actor.index()]
                .battlefield
                .iter()
                .copied()
            {
                if crate::engine::object_has_type(
                    session.inner.eaten_fixture_state(),
                    id,
                    crate::card_def::CardType::Creature,
                ) {
                    assert!(crate::engine::has_effective_keyword(
                        session.inner.eaten_fixture_state(),
                        id,
                        crate::card_def::Keywords::INDESTRUCTIBLE
                    ));
                }
            }
        }
    }
}
