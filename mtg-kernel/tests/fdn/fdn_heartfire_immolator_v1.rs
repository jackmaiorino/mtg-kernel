//! Future printed acceptance, requiring real FDN admission. No synthetic
//! definition, catalog pin, skip, or registration bypass is supplied here.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CostComponent, Keywords, Subtype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready(actor: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |id| CARD_DEFS[id as usize].object_name.into(),
        110_308,
        actor,
    );
    state.step = Step::Main1;
    state
}
fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).expect("requires actual FDN admission");
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner,
        controller: owner,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[owner.index()].hand.push(id),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
        _ => panic!("fixture zone"),
    }
    id
}
fn restore(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}
fn next(state: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(state);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}
fn settle(state: &mut GameState) {
    for _ in 0..60 {
        match next(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected {other:?}"),
        }
    }
    panic!("did not settle");
}
fn cast_creature(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let source = put(state, actor, "Heartfire Immolator", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 2;
    next(state);
    engine::step(state, Action::CastSpell(source)).unwrap();
    settle(state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    assert!(state.objects.get(source).summoning_sick);
    source
}
fn activate(state: &mut GameState, source: ObjectId, target: ObjectId) {
    let actor = state.objects.get(source).controller;
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    assert!(
        matches!(next(state),Decision::CastSpellOrPass{activatable_abilities,..}
        if activatable_abilities.contains(&(source,0)))
    );
    engine::step(state, Action::ActivateAbility(source, 0)).unwrap();
    assert!(matches!(next(state), Decision::ChooseTargets { .. }));
    *state = restore(state);
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    next(state);
    assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
    assert_eq!(
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()],
        0
    );
    assert!(state.stack.iter().any(|item| item.source == source));
}
fn damage(state: &GameState, source: ObjectId, target: ObjectId) -> i32 {
    state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::Damage {
                source: s,
                target: Target::Object(t),
                amount,
            } if *s == source && *t == target => Some(*amount),
            _ => None,
        })
        .sum()
}

#[test]
fn printed_definition_and_real_prowess_modified_sacrifice_both_seats() {
    let def = &CARD_DEFS
        [card_id_by_name("Heartfire Immolator").expect("future FDN registration") as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(
        (def.power, def.toughness, def.mana_value),
        (Some(2), Some(2), 2)
    );
    assert_eq!(def.subtypes, &[Subtype::Human, Subtype::Wizard]);
    assert_eq!(def.activated_abilities.len(), 1);
    let ability = &def.activated_abilities[0];
    assert_eq!(ability.target_spec, TargetSpec::CreatureOrPlaneswalker);
    assert!(ability.cost.contains(&CostComponent::SacrificeSelf));
    assert!(!ability.sorcery_speed_only);
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast_creature(&mut state, actor);
        // Actual noncreature cast triggers prowess, then its ordinary draw.
        let spell = put(&mut state, actor, "Think Twice", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
        next(&mut state);
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        settle(&mut state);
        assert_eq!(engine::effective_power(&state, source), 3);
        // Another real creature cast does not trigger prowess.
        let elf = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
        engine::step(&mut state, Action::CastSpell(elf)).unwrap();
        settle(&mut state);
        assert_eq!(engine::effective_power(&state, source), 3);
        state.objects.get_mut(source).counters.plus1_plus1 = 2;
        let target = put(
            &mut state,
            actor.opponent(),
            "Shivan Dragon",
            Zone::Battlefield,
        );
        activate(&mut state, source, target);
        let receipt = state
            .engine
            .event_history
            .iter()
            .find_map(|event| match event {
                CommittedEvent::SourcePowerActivationCommittedV1 { power, .. } => Some(*power),
                _ => None,
            });
        assert_eq!(receipt, Some(5));
        // Same arena object becomes a different incarnation and gains new power.
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        state.objects.get_mut(source).counters.plus1_plus1 = 20;
        state = restore(&state);
        settle(&mut state);
        assert_eq!(damage(&state, source, target), 5);
        assert_eq!(state.objects.get(target).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    }
}

#[test]
fn source_owner_controller_are_distinct_and_target_recheck_is_independent() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for leave_and_return in [false, true] {
            let mut state = ready(actor);
            let source = put(
                &mut state,
                actor.opponent(),
                "Heartfire Immolator",
                Zone::Battlefield,
            );
            state.players[actor.opponent().index()]
                .battlefield
                .retain(|id| *id != source);
            state.players[actor.index()].battlefield.push(source);
            state.objects.get_mut(source).controller = actor;
            let target = put(
                &mut state,
                actor.opponent(),
                "Shivan Dragon",
                Zone::Battlefield,
            );
            activate(&mut state, source, target);
            assert!(state.players[actor.opponent().index()]
                .graveyard
                .contains(&source));
            let item = state.stack.last().unwrap();
            assert_eq!(item.controller, actor);
            assert_eq!(
                item.v4.ability_source_contract.unwrap().owner,
                actor.opponent()
            );
            if leave_and_return {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(target, Zone::Hand),
                );
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(target, Zone::Battlefield),
                );
            }
            state = restore(&state);
            settle(&mut state);
            assert_eq!(
                damage(&state, source, target),
                if leave_and_return { 0 } else { 2 }
            );
        }
    }
}

#[test]
fn actual_opponent_response_protection_fizzles_and_prevention_reduces_damage() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for response in [true, false] {
            let mut state = ready(actor);
            let source = cast_creature(&mut state, actor);
            let target = put(
                &mut state,
                actor.opponent(),
                "Shivan Dragon",
                Zone::Battlefield,
            );
            activate(&mut state, source, target);
            if response {
                engine::step(&mut state, Action::Pass).unwrap();
                assert!(
                    matches!(next(&mut state),Decision::CastSpellOrPass{player,..} if player==actor.opponent())
                );
                let spell = put(&mut state, actor.opponent(), "Giant Growth", Zone::Hand);
                state.players[actor.opponent().index()].mana_pool[ManaColor::G.pool_index()] = 1;
                engine::step(&mut state, Action::CastSpell(spell)).unwrap();
                assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
                engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
                next(&mut state);
                settle(&mut state);
                assert_eq!(damage(&state, source, target), 2);
                assert_eq!(engine::effective_toughness(&state, target), 8);
            } else {
                // Physical primitive: a postannouncement protection grant.
                let generation = state.objects.get(target).zone_change_count;
                state.engine.until_end_of_turn.push(
                    engine::UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
                        object_id: target,
                        object_zone_change_count: generation,
                        keywords: Keywords::PROTECTION_FROM_MONOCOLORED,
                        layer: engine::Layers::ABILITY_ADDING,
                        timestamp: 1_000,
                        duration: engine::EffectDuration::EndOfTurn,
                    },
                );
                state = restore(&state);
                settle(&mut state);
                assert_eq!(damage(&state, source, target), 0);
            }
        }
    }
}

#[test]
fn committed_power_activation_forgeries_refuse_readonly_after_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut valid = ready(actor);
        let source = cast_creature(&mut valid, actor);
        let target = put(
            &mut valid,
            actor.opponent(),
            "Shivan Dragon",
            Zone::Battlefield,
        );
        activate(&mut valid, source, target);
        for case in 0..10 {
            let mut state = restore(&valid);
            match case {
                0 => state.creature_stats_lki_v1.as_mut().unwrap()[0].power = 99,
                1 => state.stack.last_mut().unwrap().controller = actor.opponent(),
                2 => {
                    state
                        .stack
                        .last_mut()
                        .unwrap()
                        .v4
                        .ability_source_contract
                        .as_mut()
                        .unwrap()
                        .zone_change_count += 1
                }
                3 => state.stack.last_mut().unwrap().targets[0] = Target::Player(actor.opponent()),
                4 => state.engine.event_history.retain(|event| {
                    !matches!(
                        event,
                        CommittedEvent::SourcePowerActivationCommittedV1 { .. }
                    )
                }),
                5 => {
                    let event = state
                        .engine
                        .event_history
                        .iter()
                        .find(|event| {
                            matches!(
                                event,
                                CommittedEvent::SourcePowerActivationCommittedV1 { .. }
                            )
                        })
                        .unwrap()
                        .clone();
                    state.engine.event_history.push(event);
                }
                6 => {
                    let event = state
                        .engine
                        .event_history
                        .iter()
                        .find(|event| {
                            matches!(
                                event,
                                CommittedEvent::SourcePowerActivationCommittedV1 { .. }
                            )
                        })
                        .unwrap()
                        .clone();
                    state.engine.event_log.push(event);
                }
                7 | 8 => {
                    state.engine.event_history.retain(|event| {
                        !matches!(
                            event,
                            CommittedEvent::SourcePowerActivationCommittedV1 { .. }
                        )
                    });
                    state.stack.last_mut().unwrap().inline_effect = if case == 7 {
                        None
                    } else {
                        Some(mtg_kernel::effect::EffectOp::Sequence(vec![]))
                    };
                }
                9 => {
                    // Retained immutable receipt still owns this incarnation
                    // if both mutable root selectors are erased.
                    state.stack.last_mut().unwrap().inline_effect = None;
                    state.stack.last_mut().unwrap().v4.ability_source_contract = None;
                }
                _ => unreachable!(),
            }
            // Restore the modified authority, then retry the same retained state.
            state = restore(&state);
            let before = serde_json::to_vec(&state).unwrap();
            assert!(
                engine::step(&mut state, Action::Pass).is_err(),
                "case {case}"
            );
            assert_eq!(serde_json::to_vec(&state).unwrap(), before, "case {case}");
            assert!(
                engine::step(&mut state, Action::Pass).is_err(),
                "retry case {case}"
            );
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                before,
                "retry case {case}"
            );
        }
        let mut stale = ready(actor);
        let source = cast_creature(&mut stale, actor);
        let target = put(
            &mut stale,
            actor.opponent(),
            "Shivan Dragon",
            Zone::Battlefield,
        );
        stale.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        engine::step(&mut stale, Action::ActivateAbility(source, 0)).unwrap();
        next(&mut stale);
        event::propose_and_commit(&mut stale, ProposedEvent::zone_change(source, Zone::Hand));
        stale = restore(&stale);
        let before = serde_json::to_vec(&stale).unwrap();
        assert!(engine::step(&mut stale, Action::ChooseTarget(Target::Object(target))).is_err());
        assert_eq!(serde_json::to_vec(&stale).unwrap(), before);
    }
}

#[test]
fn physical_partial_planeswalker_recipient_and_color_prevention_both_seats() {
    // Ajani's existing Partial card supplies only the physical recipient.
    // This is not a claim that its loyalty abilities or combat are Full.
    for actor in [PlayerId::P0, PlayerId::P1] {
        for prevent in [false, true] {
            let mut state = ready(actor);
            let source = cast_creature(&mut state, actor);
            let target = put(
                &mut state,
                actor.opponent(),
                "Ajani, Caller of the Pride",
                Zone::Hand,
            );
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(target, Zone::Battlefield),
            );
            assert_eq!(
                mtg_kernel::planeswalker_v1::loyalty(&state, target),
                Some(4)
            );
            activate(&mut state, source, target);
            if prevent {
                event::install_color_damage_prevention(&mut state, target, ManaColor::R).unwrap();
            }
            state = restore(&state);
            settle(&mut state);
            assert_eq!(
                mtg_kernel::planeswalker_v1::loyalty(&state, target),
                Some(if prevent { 4 } else { 2 })
            );
            assert_eq!(damage(&state, source, target), if prevent { 0 } else { 2 });
        }
    }
}

#[test]
fn stolen_departed_source_keeps_granted_lifelink_deathtouch_and_color_after_reentry() {
    // Explicit physical keyword-counter and temporary grants are a primitive boundary.
    // No fabricated definition or nonexistent printed producer is used.
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..3 {
            let mut state = ready(actor);
            let source = put(
                &mut state,
                actor.opponent(),
                "Heartfire Immolator",
                Zone::Battlefield,
            );
            state.players[actor.opponent().index()]
                .battlefield
                .retain(|id| *id != source);
            state.players[actor.index()].battlefield.push(source);
            state.objects.get_mut(source).controller = actor;
            state.objects.get_mut(source).v4.lifelink_keyword_counters = 1;
            let source_generation = state.objects.get(source).zone_change_count;
            state.engine.until_end_of_turn.push(
                engine::UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
                    object_id: source,
                    object_zone_change_count: source_generation,
                    keywords: Keywords::DEATHTOUCH,
                    layer: engine::Layers::ABILITY_ADDING,
                    timestamp: 999,
                    duration: engine::EffectDuration::EndOfTurn,
                },
            );
            let target = put(
                &mut state,
                actor.opponent(),
                "Shivan Dragon",
                Zone::Battlefield,
            );
            let lives = [state.players[0].life, state.players[1].life];
            activate(&mut state, source, target);
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
            // The new source is colorless with no granted damage keywords.
            state.objects.get_mut(source).v4.effective_color_mask = 0;
            state.objects.get_mut(source).v4.lifelink_keyword_counters = 0;
            if mode == 1 {
                event::install_color_damage_prevention(&mut state, target, ManaColor::R).unwrap();
            } else if mode == 2 {
                let generation = state.objects.get(target).zone_change_count;
                state.engine.until_end_of_turn.push(
                    engine::UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
                        object_id: target,
                        object_zone_change_count: generation,
                        keywords: Keywords::PROTECTION_FROM_MONOCOLORED,
                        layer: engine::Layers::ABILITY_ADDING,
                        timestamp: 1_001,
                        duration: engine::EffectDuration::EndOfTurn,
                    },
                );
            }
            state = restore(&state);
            settle(&mut state);
            let dealt = if mode == 0 { 2 } else { 0 };
            assert_eq!(damage(&state, source, target), dealt);
            assert_eq!(
                state.players[actor.index()].life,
                lives[actor.index()] + dealt
            );
            assert_eq!(
                state.players[actor.opponent().index()].life,
                lives[actor.opponent().index()]
            );
            assert_eq!(
                state.objects.get(target).zone,
                if mode == 0 {
                    Zone::Graveyard
                } else {
                    Zone::Battlefield
                }
            );
            assert_eq!(state.objects.get(source).controller, actor.opponent());
        }
    }
}

#[test]
fn historical_damage_declaration_authority_cannot_be_rewritten_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut valid = ready(actor);
        let source = cast_creature(&mut valid, actor);
        let target = put(
            &mut valid,
            actor.opponent(),
            "Shivan Dragon",
            Zone::Battlefield,
        );
        activate(&mut valid, source, target);
        for case in 0..5 {
            let mut state = restore(&valid);
            let receipt = state
                .engine
                .event_history
                .iter_mut()
                .find(|event| {
                    matches!(
                        event,
                        CommittedEvent::SourcePowerActivationCommittedV1 { .. }
                    )
                })
                .unwrap();
            if let CommittedEvent::SourcePowerActivationCommittedV1 { damage_source, .. } = receipt
            {
                match case {
                    0 => damage_source.color_mask = 0,
                    1 => damage_source.source.controller = actor.opponent(),
                    2 => damage_source.lifelink = true,
                    3 => damage_source.deathtouch = true,
                    4 => damage_source.source.zone_change_count += 1,
                    _ => unreachable!(),
                }
            }
            let before = serde_json::to_vec(&state).unwrap();
            assert!(engine::step(&mut state, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
}

#[test]
fn signed_power_and_finished_activation_are_not_replayed_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for delta in [-2, -5] {
            let mut state = ready(actor);
            let source = cast_creature(&mut state, actor);
            let generation = state.objects.get(source).zone_change_count;
            state.engine.until_end_of_turn.push(
                engine::UntilEndOfTurnEffect::ResolvedObjectEffect {
                    object_id: source,
                    object_zone_change_count: generation,
                    layer: engine::Layers::POWER_TOUGHNESS,
                    timestamp: 1_010,
                    duration: engine::EffectDuration::EndOfTurn,
                    power: delta,
                    toughness: 0,
                    grant_haste: false,
                },
            );
            let target = put(
                &mut state,
                actor.opponent(),
                "Shivan Dragon",
                Zone::Battlefield,
            );
            activate(&mut state, source, target);
            let original = state.stack.last().unwrap().clone();
            state = restore(&state);
            settle(&mut state);
            assert_eq!(damage(&state, source, target), 0);
            assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
            // Restore legitimate completion, then coherently resurrect its
            // formerly valid stack payload with all captures/history retained.
            state = restore(&state);
            state.stack.push(original);
            let before = serde_json::to_vec(&state).unwrap();
            assert!(engine::step(&mut state, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
}
