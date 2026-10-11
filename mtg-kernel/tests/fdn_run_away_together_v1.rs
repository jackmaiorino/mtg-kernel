//! Prepared printed-card acceptance. This card remains unregistered; run only
//! after a reviewed candidate catalog admits its real definition, without hooks.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardType, Keywords, TargetSpec, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready(player: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        11032,
        player,
    );
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def =
        card_id_by_name(name).expect("reviewed Run Away Together catalog admission required");
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
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
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Hand => state.players[player.index()].hand.push(id),
        _ => panic!("fixture zone"),
    }
    id
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn settle(state: &mut GameState) {
    for _ in 0..100 {
        let decision = next(state);
        if matches!(decision, Decision::CastSpellOrPass { .. })
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        let action = match decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            other => panic!("unexpected Run Away Together decision: {other:?}"),
        };
        engine::step(state, action).unwrap();
    }
    panic!("Run Away Together resolution did not settle");
}

fn copy(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn pair(caster: PlayerId, swapped: bool) -> (GameState, [ObjectId; 2]) {
    let mut state = ready(caster);
    let targets = std::array::from_fn(|index| {
        let controller = if index == 0 {
            caster
        } else {
            caster.opponent()
        };
        let owner = if swapped {
            controller.opponent()
        } else {
            controller
        };
        let object = put(&mut state, owner, "Llanowar Elves", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change_to_battlefield_under_controller(object, controller),
        );
        object
    });
    (state, targets)
}

fn transfer(state: &mut GameState, object: ObjectId, controller: PlayerId) {
    let former = state.objects.get(object).controller;
    state.players[former.index()]
        .battlefield
        .retain(|&id| id != object);
    state.players[controller.index()].battlefield.push(object);
    state.objects.get_mut(object).controller = controller;
}

fn grant(state: &mut GameState, object: ObjectId, keywords: Keywords) {
    let generation = state.objects.get(object).zone_change_count;
    state.engine.until_end_of_turn.push(
        engine::UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
            object_id: object,
            object_zone_change_count: generation,
            layer: engine::Layers::ABILITY_ADDING,
            timestamp: 999,
            duration: engine::EffectDuration::EndOfTurn,
            keywords,
        },
    );
}

fn begin(state: &mut GameState, caster: PlayerId) -> ObjectId {
    let spell = put(state, caster, "Run Away Together", Zone::Hand);
    state.players[caster.index()].mana_pool[ManaColor::U.pool_index()] = 2;
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell))
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    assert!(matches!(
        next(state),
        Decision::ChooseTargets {
            remaining: 2,
            can_finish: false,
            ..
        }
    ));
    spell
}

fn announce(
    state: &mut GameState,
    caster: PlayerId,
    targets: [ObjectId; 2],
    reverse: bool,
) -> ObjectId {
    let spell = begin(state, caster);
    let ordered = if reverse {
        [targets[1], targets[0]]
    } else {
        targets
    };
    for object in ordered {
        engine::step(state, Action::ChooseTarget(Target::Object(object))).unwrap();
        next(state);
    }
    assert!(state
        .stack
        .iter()
        .any(|item| item.source == spell && item.targets.len() == 2));
    spell
}

#[test]
fn run_away_printed_metadata_owner_hand_moves_and_both_pending_target_snapshots_restore() {
    let def = &CARD_DEFS[usize::from(
        card_id_by_name("Run Away Together").expect("actual reviewed admission required"),
    )];
    assert!(def.is_executable());
    assert_eq!(def.types, &[CardType::Instant]);
    assert_eq!(def.colors, &[ManaColor::U]);
    assert!(def.subtypes.is_empty());
    assert_eq!(def.mana_value, 2);
    assert_eq!(def.cost.generic, 1);
    assert_eq!(
        def.cost.pips,
        &[mtg_kernel::mana::Pip::Colored(ManaColor::U)]
    );
    assert_eq!(
        def.target_spec,
        TargetSpec::TwoCreaturesWithDifferentControllers
    );
    assert_eq!(def.target_spec.stable_id(), 62);
    for caster in [PlayerId::P0, PlayerId::P1] {
        for swapped in [false, true] {
            for reverse in [false, true] {
                let (mut state, targets) = pair(caster, swapped);
                let same_controller = put(&mut state, caster, "Savannah Lions", Zone::Battlefield);
                let spell = begin(&mut state, caster);
                state = copy(&state); // pending first pick
                let ordered = if reverse {
                    [targets[1], targets[0]]
                } else {
                    targets
                };
                engine::step(&mut state, Action::ChooseTarget(Target::Object(ordered[0]))).unwrap();
                let legal = match next(&mut state) {
                    Decision::ChooseTargets {
                        remaining: 1,
                        can_finish: false,
                        legal_targets,
                        ..
                    } => legal_targets,
                    other => panic!("{other:?}"),
                };
                assert!(legal.contains(&Target::Object(ordered[1])));
                assert!(!legal.contains(&Target::Object(ordered[0])));
                if !reverse {
                    assert!(!legal.contains(&Target::Object(same_controller)));
                }
                let before = serde_json::to_vec(&state).unwrap();
                for invalid in [
                    Action::ChooseTarget(Target::Object(ordered[0])),
                    Action::ChooseTarget(Target::Player(caster)),
                    Action::FinishEffectSelection,
                ] {
                    assert!(engine::step(&mut state, invalid).is_err());
                    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
                }
                if !reverse {
                    assert!(engine::step(
                        &mut state,
                        Action::ChooseTarget(Target::Object(same_controller))
                    )
                    .is_err());
                    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
                }
                let mut restored = copy(&state); // pending second pick
                for current in [&mut state, &mut restored] {
                    engine::step(current, Action::ChooseTarget(Target::Object(ordered[1])))
                        .unwrap();
                    assert!(matches!(next(current), Decision::CastSpellOrPass { .. }));
                    assert_eq!(
                        current.players[caster.index()].mana_pool[ManaColor::U.pool_index()],
                        0
                    );
                    settle(current);
                    for target in targets {
                        let owner = current.objects.get(target).owner;
                        assert_eq!(current.objects.get(target).zone, Zone::Hand);
                        assert_eq!(current.objects.get(target).controller, owner);
                        assert!(current.players[owner.index()].hand.contains(&target));
                    }
                    assert_eq!(current.objects.get(same_controller).zone, Zone::Battlefield);
                    assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
                }
                assert_eq!(
                    state.diagnostic_state_hash(),
                    restored.diagnostic_state_hash()
                );
            }
        }
    }
}

#[test]
fn run_away_no_offer_without_a_complete_pair_respects_protection_and_opposing_hexproof() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        for unavailable in 0..4 {
            let (mut state, targets) = pair(caster, false);
            match unavailable {
                0 => event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(targets[1], Zone::Hand),
                ),
                1 => transfer(&mut state, targets[1], caster),
                2 => grant(&mut state, targets[1], Keywords::HEXPROOF),
                _ => grant(
                    &mut state,
                    targets[1],
                    Keywords::PROTECTION_FROM_MONOCOLORED,
                ),
            }
            let spell = put(&mut state, caster, "Run Away Together", Zone::Hand);
            state.players[caster.index()].mana_pool[ManaColor::U.pool_index()] = 2;
            assert!(
                !matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell))
            );
            let before = serde_json::to_vec(&state).unwrap();
            assert!(engine::step(&mut state, Action::CastSpell(spell)).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
        let (mut state, targets) = pair(caster, false);
        grant(&mut state, targets[0], Keywords::HEXPROOF);
        let spell = announce(&mut state, caster, targets, false);
        settle(&mut state);
        assert!(targets
            .iter()
            .all(|&target| state.objects.get(target).zone == Zone::Hand));
        assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
        // A printed protection source is excluded as well.
        let (mut state, targets) = pair(caster, false);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(targets[1], Zone::Hand),
        );
        put(
            &mut state,
            caster.opponent(),
            "Guardian of the Guildpact",
            Zone::Battlefield,
        );
        let spell = put(&mut state, caster, "Run Away Together", Zone::Hand);
        state.players[caster.index()].mana_pool[ManaColor::U.pool_index()] = 2;
        assert!(
            !matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell))
        );
    }
}

#[test]
fn run_away_actual_bolt_response_leaves_one_legal_target_in_either_order_after_restore() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        for reverse in [false, true] {
            for departed_index in 0..2 {
                let (mut state, targets) = pair(caster, true);
                let spell = announce(&mut state, caster, targets, reverse);
                let bolt = put(&mut state, caster, "Lightning Bolt", Zone::Hand);
                state.players[caster.index()].mana_pool[ManaColor::R.pool_index()] = 1;
                engine::step(&mut state, Action::CastSpell(bolt)).unwrap();
                assert!(
                    matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(targets[departed_index])))
                );
                engine::step(
                    &mut state,
                    Action::ChooseTarget(Target::Object(targets[departed_index])),
                )
                .unwrap();
                assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
                let mut restored = copy(&state);
                for current in [&mut state, &mut restored] {
                    settle(current);
                    assert_eq!(
                        current.objects.get(targets[departed_index]).zone,
                        Zone::Graveyard
                    );
                    let survivor = targets[1 - departed_index];
                    let owner = current.objects.get(survivor).owner;
                    assert_eq!(current.objects.get(survivor).zone, Zone::Hand);
                    assert!(current.players[owner.index()].hand.contains(&survivor));
                    assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
                    assert_eq!(current.objects.get(bolt).zone, Zone::Graveyard);
                    assert_eq!((current.players[0].life, current.players[1].life), (20, 20));
                }
                assert_eq!(
                    state.diagnostic_state_hash(),
                    restored.diagnostic_state_hash()
                );
            }
        }
    }
}

#[test]
fn run_away_controller_change_making_pair_same_controller_fizzles_both_without_order_bias() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        for reverse in [false, true] {
            let (mut state, targets) = pair(caster, true);
            let spell = announce(&mut state, caster, targets, reverse);
            // Consistent live-control fixture; no printed control-spell support is claimed.
            transfer(&mut state, targets[1], caster);
            let mut restored = copy(&state);
            for current in [&mut state, &mut restored] {
                settle(current);
                assert!(targets
                    .iter()
                    .all(|&target| current.objects.get(target).zone == Zone::Battlefield));
                assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
            }
            assert_eq!(
                state.diagnostic_state_hash(),
                restored.diagnostic_state_hash()
            );
        }
    }
}

#[test]
fn run_away_stale_reentered_target_does_not_constrain_original_survivor_and_both_departed_fizzle() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        for reverse in [false, true] {
            for departed_index in 0..2 {
                let (mut state, targets) = pair(caster, true);
                let spell = announce(&mut state, caster, targets, reverse);
                let departed = targets[departed_index];
                let survivor = targets[1 - departed_index];
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(departed, Zone::Hand),
                );
                let survivor_controller = state.objects.get(survivor).controller;
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change_to_battlefield_under_controller(
                        departed,
                        survivor_controller,
                    ),
                );
                let generation = state.objects.get(departed).zone_change_count;
                let mut restored = copy(&state);
                for current in [&mut state, &mut restored] {
                    settle(current);
                    assert_eq!(current.objects.get(departed).zone, Zone::Battlefield);
                    assert_eq!(current.objects.get(departed).zone_change_count, generation);
                    assert_eq!(current.objects.get(survivor).zone, Zone::Hand);
                    assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
                }
                assert_eq!(
                    state.diagnostic_state_hash(),
                    restored.diagnostic_state_hash()
                );
            }
            let (mut state, targets) = pair(caster, false);
            let spell = announce(&mut state, caster, targets, reverse);
            for target in targets {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(target, Zone::Exile),
                );
            }
            state = copy(&state);
            settle(&mut state);
            assert!(targets
                .iter()
                .all(|&target| state.objects.get(target).zone == Zone::Exile));
            assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
        }
    }
}

#[test]
fn run_away_rechecks_protection_and_hexproof_before_any_return_in_either_order() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        for reverse in [false, true] {
            for keyword in [Keywords::PROTECTION_FROM_MONOCOLORED, Keywords::HEXPROOF] {
                let (mut state, targets) = pair(caster, true);
                announce(&mut state, caster, targets, reverse);
                grant(&mut state, targets[1], keyword);
                state = copy(&state);
                settle(&mut state);
                assert_eq!(state.objects.get(targets[1]).zone, Zone::Battlefield);
                assert_eq!(state.objects.get(targets[0]).zone, Zone::Hand);
            }
        }
    }
}
