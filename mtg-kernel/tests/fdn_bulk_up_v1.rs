//! Registered candidate; full gameplay/profile admission remains pending.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, CardType, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready(active: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        1109,
        active,
    );
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
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
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Library => state.players[player.index()].library.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        _ => panic!("fixture zone"),
    }
    id
}

fn next(state: &mut GameState) -> Decision {
    let choice = engine::advance_until_decision(state);
    assert!(!matches!(choice, Decision::Halted { .. }), "{choice:?}");
    choice
}

fn priority(state: &mut GameState, player: PlayerId) {
    let Decision::CastSpellOrPass {
        player: chooser, ..
    } = next(state)
    else {
        panic!("priority decision absent");
    };
    if chooser != player {
        engine::step(state, Action::Pass).unwrap();
        assert!(
            matches!(next(state), Decision::CastSpellOrPass { player: chooser, .. } if chooser == player)
        );
    }
}

fn cast(state: &mut GameState, spell: ObjectId, target: Option<Target>) {
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell))
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    for _ in 0..16 {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                let target = target.expect("fixture target");
                assert!(legal_targets.contains(&target));
                engine::step(state, Action::ChooseTarget(target)).unwrap();
            }
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap();
            }
            Decision::CastSpellOrPass { .. } if state.engine.pending_cast.is_none() => return,
            choice => panic!("unexpected cast decision {choice:?}"),
        }
    }
    panic!("cast did not finish");
}

fn settle(state: &mut GameState) -> Decision {
    for _ in 0..64 {
        let choice = next(state);
        match choice {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return choice
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { ref pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap();
            }
            _ => return choice,
        }
    }
    panic!("resolution did not settle");
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn same(state: &GameState, replay: &GameState) {
    assert_eq!(
        serde_json::to_value(state).unwrap(),
        serde_json::to_value(replay).unwrap()
    );
}

fn cleanup(state: &mut GameState) {
    assert!(state.stack.is_empty());
    let active = state.active_player;
    state.step = Step::End;
    state.priority_player = active;
    state.engine.priority_passes = [false, false];
    for _ in 0..8 {
        assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
        engine::step(state, Action::Pass).unwrap();
        if state.active_player != active {
            return;
        }
    }
    panic!("cleanup did not finish");
}

#[test]
fn printed_definition_normal_and_flashback_costs_match_pinned_constructor() {
    use mtg_kernel::card_def::CostComponent;
    use mtg_kernel::mana::Cost;
    let def = &CARD_DEFS[card_id_by_name("Bulk Up").unwrap() as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.types, &[CardType::Instant]);
    assert!(def.subtypes.is_empty());
    assert_eq!(def.mana_value, 2);
    assert_eq!(def.colors, &[ManaColor::R]);
    assert_eq!(def.cost.generic, 1);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(
        def.flashback.as_ref().unwrap().cost,
        &[CostComponent::Mana(Cost {
            generic: 4,
            pips: &[Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::R)],
            x_count: 0,
        })]
    );
}

#[test]
fn actual_cast_samples_power_at_resolution_and_cleans_up_after_restore() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let target = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        let spell = put(&mut state, player, "Bulk Up", Zone::Hand);
        state.players[player.index()].mana_pool[3] = 1;
        state.players[player.index()].mana_pool[5] = 1;
        priority(&mut state, player);
        cast(&mut state, spell, Some(Target::Object(target)));
        // The announced spell has not resolved; current counters affect
        // the sampled power at resolution, not at announcement.
        state.objects.get_mut(target).counters.plus1_plus1 = 2;
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            settle(current);
            assert_eq!(engine::effective_power(current, target), 6);
            assert_eq!(engine::effective_toughness(current, target), 3);
            assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
            current.objects.get_mut(target).counters.plus1_plus1 += 1;
            assert_eq!(engine::effective_power(current, target), 7);
            cleanup(current);
            assert_eq!(engine::effective_power(current, target), 4);
            assert_eq!(engine::effective_toughness(current, target), 4);
        }
        same(&state, &replay);
    }
}

#[test]
fn two_actual_doublers_stack_as_separate_sampled_modifiers() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let target = put(&mut state, player, "Serra Angel", Zone::Battlefield);
        for expected in [8, 16] {
            let spell = put(&mut state, player, "Bulk Up", Zone::Hand);
            state.players[player.index()].mana_pool[3] = 1;
            state.players[player.index()].mana_pool[5] = 1;
            priority(&mut state, player);
            cast(&mut state, spell, Some(Target::Object(target)));
            settle(&mut state);
            assert_eq!(engine::effective_power(&state, target), expected);
            assert_eq!(engine::effective_toughness(&state, target), 4);
        }
        cleanup(&mut state);
        assert_eq!(engine::effective_power(&state, target), 4);
    }
}

#[test]
fn actual_flashback_requires_colored_pips_and_exiles_after_restore() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for (red, generic, expected) in [(2, 4, true), (1, 5, false), (2, 3, false), (3, 3, true)] {
            let mut state = ready(player);
            let target = put(&mut state, player, "Serra Angel", Zone::Battlefield);
            let spell = put(&mut state, player, "Bulk Up", Zone::Graveyard);
            state.players[player.index()].mana_pool[3] = red;
            state.players[player.index()].mana_pool[5] = generic;
            priority(&mut state, player);
            let Decision::CastSpellOrPass {
                castable_spells, ..
            } = next(&mut state)
            else {
                panic!("priority")
            };
            assert_eq!(castable_spells.contains(&spell), expected);
            if !expected {
                let before = serde_json::to_vec(&state).unwrap();
                assert!(engine::step(&mut state, Action::CastSpell(spell)).is_err());
                assert_eq!(serde_json::to_vec(&state).unwrap(), before);
                continue;
            }
            cast(&mut state, spell, Some(Target::Object(target)));
            assert!(state.stack.last().unwrap().is_flashback);
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                settle(current);
                assert_eq!(engine::effective_power(current, target), 8);
                assert_eq!(current.objects.get(spell).zone, Zone::Exile);
                assert_eq!(current.players[player.index()].mana_pool, [0; 6]);
            }
            same(&state, &replay);
        }
    }
}

#[test]
fn blinked_target_refuses_old_incarnation_for_normal_and_flashback() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for flashback in [false, true] {
            let mut state = ready(player);
            let target = put(&mut state, player, "Serra Angel", Zone::Battlefield);
            let spell = put(
                &mut state,
                player,
                "Bulk Up",
                if flashback {
                    Zone::Graveyard
                } else {
                    Zone::Hand
                },
            );
            state.players[player.index()].mana_pool[3] = if flashback { 2 } else { 1 };
            state.players[player.index()].mana_pool[5] = if flashback { 4 } else { 1 };
            priority(&mut state, player);
            cast(&mut state, spell, Some(Target::Object(target)));
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(target, Zone::Graveyard),
            );
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(target, Zone::Battlefield),
            );
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                settle(current);
                assert_eq!(engine::effective_power(current, target), 4);
                assert!(current.engine.until_end_of_turn.is_empty());
                assert_eq!(
                    current.objects.get(spell).zone,
                    if flashback {
                        Zone::Exile
                    } else {
                        Zone::Graveyard
                    }
                );
            }
            same(&state, &replay);
        }
    }
}
