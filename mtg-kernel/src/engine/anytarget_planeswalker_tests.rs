//! Recipient primitive tests. Ajani remains Partial; these do not admit its gameplay.
use super::*;
use crate::policy_observation_v6::tests::{put, ready_state};

#[test]
fn anytarget_recipient_scope_matches_catalog_precedence_and_existing_order() {
    let mut state = ready_state();
    // Registered definitions only, including any planeswalkers in this catalog.
    for player in [PlayerId::P0, PlayerId::P1] {
        for def in card_def::CARD_DEFS.iter() {
            if [
                CardType::Creature,
                CardType::Planeswalker,
                CardType::Land,
                CardType::Artifact,
            ]
            .iter()
            .any(|&kind| def.has_type(kind))
            {
                put(&mut state, player, def.name, Zone::Battlefield);
            }
        }
    }
    let fdn = cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ));
    let mut expected = vec![Target::Player(PlayerId::P0), Target::Player(PlayerId::P1)];
    for player in [PlayerId::P0, PlayerId::P1] {
        for &object in &state.players[player.index()].battlefield {
            if object_has_type(&state, object, CardType::Creature)
                || (fdn && object_has_type(&state, object, CardType::Planeswalker))
            {
                expected.push(Target::Object(object));
            }
        }
    }
    for actor in [PlayerId::P0, PlayerId::P1] {
        let offered = legal_targets_for_controller(TargetSpec::AnyTarget, &[], actor, &state);
        // Account for existing opponent hexproof filtering in every feature configuration.
        let expected: Vec<_> = expected
            .iter()
            .copied()
            .filter(|target| match target {
                Target::Player(_) => true,
                Target::Object(object) => {
                    state.objects.get(*object).controller == actor
                        || !has_effective_keyword(&state, *object, Keywords::HEXPROOF)
                }
            })
            .collect();
        assert_eq!(offered, expected);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod fdn {
    use super::*;
    use crate::card_def::CardCapability;
    use crate::effect::{EffectBooleanChoicePurpose, PendingEffectChoice};
    use crate::event::{self, ProposedEvent};
    use crate::planeswalker_v1;
    use crate::surface_v2::{
        HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SurfaceAction, SurfaceDecision,
    };

    fn surface() -> HarnessSurfaceV2 {
        HarnessSurfaceV2::new_with_priority_mode_v1(
            PriorityModeV1::EngineWindowsV1,
            SuppressionAuditMode::Full,
        )
    }

    fn next(surface: &mut HarnessSurfaceV2, state: &mut GameState) -> Decision {
        match surface.next_decision(state) {
            SurfaceDecision::Decision(decision @ Decision::Halted { .. }) => panic!("{decision:?}"),
            SurfaceDecision::Decision(decision) => decision,
            other => panic!("unexpected surface result {other:?}"),
        }
    }

    fn apply(surface: &mut HarnessSurfaceV2, state: &mut GameState, action: Action) {
        surface.apply(state, SurfaceAction::Action(action)).unwrap();
    }

    fn ready(actor: PlayerId) -> GameState {
        let island = card_def::card_id_by_name("Island").unwrap();
        let mut state =
            GameState::new_from_libraries(&[island; 40], &[island; 40], |_| "Island".into(), 110);
        state.step = Step::Main1;
        state.active_player = actor;
        state.priority_player = actor;
        // Both seats retain an actual castable decision after resolution.
        for player in [PlayerId::P0, PlayerId::P1] {
            put(&mut state, player, "Mountain", Zone::Battlefield);
            put(&mut state, player, "Lightning Bolt", Zone::Hand);
        }
        state
    }

    fn ajani_recipient(state: &mut GameState, owner: PlayerId) -> ObjectId {
        let ajani = put(state, owner, "Ajani, Caller of the Pride", Zone::Hand);
        assert_eq!(
            card_def::CARD_DEFS[state.objects.get(ajani).card_def as usize].capability,
            CardCapability::Partial
        );
        // Actual zone event initializes registered starting loyalty; no casting/admission bypass.
        event::propose_and_commit(state, ProposedEvent::zone_change(ajani, Zone::Battlefield));
        assert_eq!(planeswalker_v1::loyalty(state, ajani), Some(4));
        ajani
    }

    fn announce_bolt(
        surface: &mut HarnessSurfaceV2,
        state: &mut GameState,
        actor: PlayerId,
    ) -> (ObjectId, Vec<Target>) {
        let bolt = put(state, actor, "Lightning Bolt", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        if matches!(next(surface, state), Decision::CastSpellOrPass { player, .. } if player != actor)
        {
            apply(surface, state, Action::Pass);
        }
        assert!(
            matches!(next(surface, state), Decision::CastSpellOrPass { player, castable_spells, .. }
            if player == actor && castable_spells.contains(&bolt))
        );
        apply(surface, state, Action::CastSpell(bolt));
        match next(surface, state) {
            Decision::ChooseTargets {
                player,
                legal_targets,
                ..
            } => {
                assert_eq!(player, actor);
                (bolt, legal_targets)
            }
            other => panic!("expected Bolt target choice {other:?}"),
        }
    }

    fn cast_bolt(
        surface: &mut HarnessSurfaceV2,
        state: &mut GameState,
        actor: PlayerId,
        target: Target,
    ) -> ObjectId {
        let (bolt, offered) = announce_bolt(surface, state, actor);
        assert!(offered.contains(&target));
        apply(surface, state, Action::ChooseTarget(target));
        assert!(state
            .stack
            .iter()
            .any(|item| item.source == bolt && item.targets == [target]));
        bolt
    }

    fn drain(
        surface: &mut HarnessSurfaceV2,
        state: &mut GameState,
        payment: Option<bool>,
    ) -> usize {
        let mut choices = 0;
        for _ in 0..100 {
            let decision = next(surface, state);
            if matches!(decision, Decision::CastSpellOrPass { .. }) && state.stack.is_empty() {
                assert!(state.engine.pending_triggers.is_empty());
                return choices;
            }
            let action = match decision {
                Decision::CastSpellOrPass { .. } => Action::Pass,
                Decision::OrderTriggers { pending, .. } => {
                    Action::OrderTriggers((0..pending.len()).collect())
                }
                Decision::ChooseEffectBoolean { .. } => {
                    choices += 1;
                    Action::ChooseEffectBoolean(payment.expect("explicit Ward answer"))
                }
                other => panic!("unexpected resolution decision {other:?}"),
            };
            apply(surface, state, action);
        }
        panic!("resolution did not finish");
    }

    fn restored(state: &GameState) -> GameState {
        let bytes = serde_json::to_vec(state).unwrap();
        let restored: GameState = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(bytes, serde_json::to_vec(&restored).unwrap());
        assert_eq!(state.state_hash(), restored.state_hash());
        restored
    }

    fn drain_restored(
        state: &mut GameState,
        surface: &mut HarnessSurfaceV2,
        payment: Option<bool>,
    ) -> usize {
        let mut replay = restored(state);
        let choices = drain(surface, state, payment);
        assert_eq!(drain(&mut self::surface(), &mut replay, payment), choices);
        assert_eq!(
            serde_json::to_vec(state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        assert_eq!(state.state_hash(), replay.state_hash());
        choices
    }

    fn grant(state: &mut GameState, source: ObjectId, recipient: ObjectId, keyword: Keywords) {
        effect::execute(
            &EffectOp::GrantKeywordTargetUntilEndOfTurn {
                object: ObjectRef::Target(0),
                keyword,
            },
            &ExecCtx {
                stack_item_id: None,
                source,
                controller: state.objects.get(source).controller,
                targets: vec![Target::Object(recipient)],
                target_contracts: vec![],
                discarded: vec![],
                paid_cost_refs: vec![],
                hidden_ability_source: None,
                ability_source_contract: None,
                kicked: false,
                optional_additional_cost_paid: None,
                x_value: 0,
            },
            state,
        );
        assert!(has_effective_keyword(state, recipient, keyword));
    }

    #[test]
    fn actual_bolt_offers_both_players_creatures_and_partial_planeswalker_recipients() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let walkers = [actor, actor.opponent()].map(|owner| ajani_recipient(&mut state, owner));
            let creature = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let land = put(&mut state, actor, "Island", Zone::Battlefield);
            let (_, offered) = announce_bolt(&mut surface(), &mut state, actor);
            assert_eq!(
                &offered[..2],
                &[Target::Player(PlayerId::P0), Target::Player(PlayerId::P1)]
            );
            for object in walkers.into_iter().chain([creature]) {
                assert_eq!(
                    offered
                        .iter()
                        .filter(|&&target| target == Target::Object(object))
                        .count(),
                    1
                );
            }
            assert!(!offered.contains(&Target::Object(land)));
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                step(current, Action::ChooseTarget(Target::Object(walkers[0]))).unwrap();
                drain(&mut surface(), current, None);
            }
            assert_eq!(state.state_hash(), replay.state_hash());
            assert_eq!(planeswalker_v1::loyalty(&state, walkers[0]), Some(1));
        }
    }

    #[test]
    fn actual_bolt_damage_loyalty_and_zero_loyalty_sba_restore_for_both_seats_and_controllers() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for owner in [actor, actor.opponent()] {
                let mut state = ready(actor);
                let ajani = ajani_recipient(&mut state, owner);
                let mut surface = surface();
                let bolt = cast_bolt(&mut surface, &mut state, actor, Target::Object(ajani));
                assert_eq!(drain_restored(&mut state, &mut surface, None), 0);
                assert_eq!(planeswalker_v1::loyalty(&state, ajani), Some(1));
                assert_eq!(state.objects.get(ajani).damage, 0);
                assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
                assert_eq!(state.players[owner.index()].life, 20);
                cast_bolt(&mut surface, &mut state, actor, Target::Object(ajani));
                drain_restored(&mut state, &mut surface, None);
                assert_eq!(state.objects.get(ajani).zone, Zone::Graveyard);
                assert_eq!(planeswalker_v1::loyalty(&state, ajani), None);
                assert!(state.players[owner.index()].graveyard.contains(&ajani));
            }
        }
    }

    #[test]
    fn departed_or_reentered_planeswalker_recipient_invalidates_old_bolt_contract() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for reenter in [false, true] {
                let mut state = ready(actor);
                let ajani = ajani_recipient(&mut state, actor.opponent());
                let mut surface = surface();
                let bolt = cast_bolt(&mut surface, &mut state, actor, Target::Object(ajani));
                let old_generation = state.objects.get(ajani).zone_change_count;
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(ajani, Zone::Exile),
                );
                if reenter {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(ajani, Zone::Battlefield),
                    );
                    assert_eq!(planeswalker_v1::loyalty(&state, ajani), Some(4));
                }
                assert!(state.objects.get(ajani).zone_change_count > old_generation);
                assert!(!stack_targets_still_legal(state.stack.last().unwrap(), &state).unwrap());
                drain_restored(&mut state, &mut surface, None);
                assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
                assert_eq!(
                    planeswalker_v1::loyalty(&state, ajani),
                    reenter.then_some(4)
                );
                if reenter {
                    cast_bolt(&mut surface, &mut state, actor, Target::Object(ajani));
                    drain_restored(&mut state, &mut surface, None);
                    assert_eq!(planeswalker_v1::loyalty(&state, ajani), Some(1));
                }
            }
        }
    }

    #[test]
    fn partial_planeswalker_protection_blocks_announcement_rechecks_resolution_and_prevents_damage()
    {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for added_after_announcement in [false, true] {
                let mut state = ready(actor);
                let ajani = ajani_recipient(&mut state, actor.opponent());
                let source = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
                let mut surface = surface();
                let bolt = if added_after_announcement {
                    Some(cast_bolt(
                        &mut surface,
                        &mut state,
                        actor,
                        Target::Object(ajani),
                    ))
                } else {
                    None
                };
                // Existing temporary-keyword primitive, not an invented printed Ajani ability.
                grant(
                    &mut state,
                    source,
                    ajani,
                    Keywords::PROTECTION_FROM_MONOCOLORED,
                );
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::damage(source, Target::Object(ajani), 3),
                );
                assert_eq!(planeswalker_v1::loyalty(&state, ajani), Some(4));
                if let Some(bolt) = bolt {
                    assert!(
                        !stack_targets_still_legal(state.stack.last().unwrap(), &state).unwrap()
                    );
                    drain_restored(&mut state, &mut surface, None);
                    assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
                } else {
                    let (_, offered) = announce_bolt(&mut surface, &mut state, actor);
                    assert!(!offered.contains(&Target::Object(ajani)));
                    let before = serde_json::to_vec(&state).unwrap();
                    assert!(step(&mut state, Action::ChooseTarget(Target::Object(ajani))).is_err());
                    assert_eq!(before, serde_json::to_vec(&state).unwrap());
                    apply(
                        &mut surface,
                        &mut state,
                        Action::ChooseTarget(Target::Player(actor.opponent())),
                    );
                    drain_restored(&mut state, &mut surface, None);
                }
                assert_eq!(planeswalker_v1::loyalty(&state, ajani), Some(4));
                // The old generation's protection must not attach to the reentered recipient.
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(ajani, Zone::Exile),
                );
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(ajani, Zone::Battlefield),
                );
                assert!(!has_effective_keyword(
                    &state,
                    ajani,
                    Keywords::PROTECTION_FROM_MONOCOLORED
                ));
                cast_bolt(&mut surface, &mut state, actor, Target::Object(ajani));
                drain_restored(&mut state, &mut surface, None);
                assert_eq!(planeswalker_v1::loyalty(&state, ajani), Some(1));
            }
        }
    }

    #[test]
    fn partial_planeswalker_hexproof_filters_opponent_only_and_rechecks_existing_target() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for owner in [actor, actor.opponent()] {
                let mut state = ready(actor);
                let ajani = ajani_recipient(&mut state, owner);
                let source = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
                let mut surface = surface();
                cast_bolt(&mut surface, &mut state, actor, Target::Object(ajani));
                grant(&mut state, source, ajani, Keywords::HEXPROOF);
                assert_eq!(
                    stack_targets_still_legal(state.stack.last().unwrap(), &state).unwrap(),
                    owner == actor
                );
                drain_restored(&mut state, &mut surface, None);
                assert_eq!(
                    planeswalker_v1::loyalty(&state, ajani),
                    Some(if owner == actor { 1 } else { 4 })
                );
                let (_, offered) = announce_bolt(&mut surface, &mut state, actor);
                assert_eq!(offered.contains(&Target::Object(ajani)), owner == actor);
            }
        }
    }

    #[test]
    fn adding_planeswalker_recipient_preserves_life_ward_targeter_source_and_controller_bindings() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for pay in [false, true] {
                let mut state = ready(actor);
                let ajani = ajani_recipient(&mut state, actor.opponent());
                let sire = put(
                    &mut state,
                    actor.opponent(),
                    "Sire of Seven Deaths",
                    Zone::Battlefield,
                );
                let mut surface = surface();
                let bolt = cast_bolt(&mut surface, &mut state, actor, Target::Object(sire));
                let targeter = state
                    .stack
                    .iter()
                    .find(|item| item.source == bolt)
                    .unwrap()
                    .v4
                    .stack_item_id;
                for _ in 0..32 {
                    match next(&mut surface, &mut state) {
                        Decision::ChooseEffectBoolean {
                            player,
                            source,
                            purpose:
                                EffectBooleanChoicePurpose::CounterUnlessPaysLife {
                                    life: 7,
                                    player: payer,
                                    targeting_stack_item,
                                    ..
                                },
                            ..
                        } => {
                            assert_eq!(player, actor);
                            assert_eq!(payer, actor);
                            assert_eq!(source, sire);
                            assert_eq!(targeting_stack_item, targeter);
                            break;
                        }
                        Decision::CastSpellOrPass { .. } => {
                            apply(&mut surface, &mut state, Action::Pass)
                        }
                        Decision::OrderTriggers { pending, .. } => apply(
                            &mut surface,
                            &mut state,
                            Action::OrderTriggers((0..pending.len()).collect()),
                        ),
                        other => panic!("unexpected Ward decision {other:?}"),
                    }
                }
                assert!(matches!(
                    state.engine.pending_effect.as_ref().unwrap().choice,
                    Some(PendingEffectChoice::ChooseBoolean { .. })
                ));
                // Ward's captured source remains authenticated after departing and reentering.
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(sire, Zone::Exile),
                );
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(sire, Zone::Battlefield),
                );
                assert_eq!(drain_restored(&mut state, &mut surface, Some(pay)), 1);
                assert_eq!(state.players[actor.index()].life, if pay { 13 } else { 20 });
                assert_eq!(state.objects.get(sire).damage, 0); // stale Bolt target never damages its return.
                assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
                assert_eq!(planeswalker_v1::loyalty(&state, ajani), Some(4));
                // Targeting Ajani creates no Ward merely because Sire is also a recipient.
                cast_bolt(&mut surface, &mut state, actor, Target::Object(ajani));
                assert_eq!(drain_restored(&mut state, &mut surface, None), 0);
                assert_eq!(planeswalker_v1::loyalty(&state, ajani), Some(1));
            }
        }
    }
}
