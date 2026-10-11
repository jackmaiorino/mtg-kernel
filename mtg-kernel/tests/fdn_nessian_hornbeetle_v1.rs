//! Registered candidate; full gameplay/profile admission remains pending.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, Subtype, CARD_DEFS,
};
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

fn settle(state: &mut GameState) {
    for _ in 0..64 {
        match next(state) {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap();
            }
            choice => panic!("unexpected resolution decision {choice:?}"),
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

fn begin_combat(state: &mut GameState) {
    for _ in 0..16 {
        let decision = next(state);
        if state.step == Step::BeginCombat {
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            choice => panic!("unexpected combat transition {choice:?}"),
        }
    }
    panic!("combat did not begin");
}

fn qualifying(state: &mut GameState, player: PlayerId) -> ObjectId {
    let creature = put(state, player, "Llanowar Elves", Zone::Battlefield);
    state.objects.get_mut(creature).counters.plus1_plus1 = 3;
    creature
}

#[test]
fn printed_metadata_and_actual_cast_then_own_combat_counter() {
    let def = &CARD_DEFS[card_id_by_name("Nessian Hornbeetle").unwrap() as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Insect]);
    assert_eq!(
        (def.power, def.toughness, def.mana_value),
        (Some(2), Some(2), 2)
    );
    assert_eq!(def.colors, &[ManaColor::G]);
    assert_eq!(def.cost.generic, 1);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::G)]);
    assert_eq!(def.keywords.0, Keywords::default().0);
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Nessian Hornbeetle", Zone::Hand);
        qualifying(&mut state, player);
        state.players[player.index()].mana_pool[4] = 1;
        state.players[player.index()].mana_pool[5] = 1;
        priority(&mut state, player);
        cast(&mut state, source, None);
        settle(&mut state);
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(source).counters.plus1_plus1, 0);
        begin_combat(&mut state);
        assert!(!state.stack.is_empty());
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            settle(current);
            assert_eq!(current.objects.get(source).counters.plus1_plus1, 1);
        }
        same(&state, &replay);
    }
}

#[test]
fn combat_gate_excludes_self_opponent_and_three_power_and_opponent_turn() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for (own_counters, source_counters, own_turn, expected) in [
            (0, 2, true, false),
            (2, 0, true, false),
            (3, 0, false, false),
            (3, 0, true, true),
        ] {
            let mut state = ready(if own_turn { player } else { player.opponent() });
            let source = put(&mut state, player, "Nessian Hornbeetle", Zone::Battlefield);
            state.objects.get_mut(source).counters.plus1_plus1 = source_counters;
            let own = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
            state.objects.get_mut(own).counters.plus1_plus1 = own_counters;
            qualifying(&mut state, player.opponent());
            begin_combat(&mut state);
            assert_eq!(!state.stack.is_empty(), expected);
            settle(&mut state);
            assert_eq!(
                state.objects.get(source).counters.plus1_plus1,
                source_counters + i32::from(expected)
            );
        }
    }
}

#[test]
fn intervening_condition_rechecks_power_zone_and_captured_controller_after_restore() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for change in 0..5 {
            let mut state = ready(player);
            let source = put(&mut state, player, "Nessian Hornbeetle", Zone::Battlefield);
            let other = qualifying(&mut state, player);
            begin_combat(&mut state);
            assert!(!state.stack.is_empty());
            match change {
                1 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(other, Zone::Graveyard),
                    );
                }
                2 => state.objects.get_mut(other).counters.plus1_plus1 = 0,
                3 | 4 => {
                    let object = if change == 3 { other } else { source };
                    state.objects.get_mut(object).controller = player.opponent();
                    state.players[player.index()]
                        .battlefield
                        .retain(|id| *id != object);
                    state.players[player.opponent().index()]
                        .battlefield
                        .push(object);
                }
                _ => {}
            }
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                settle(current);
                assert_eq!(
                    current.objects.get(source).counters.plus1_plus1,
                    i32::from(change == 0 || change == 4)
                );
            }
            same(&state, &replay);
        }
    }
}

#[test]
fn returned_source_is_another_object_but_old_trigger_cannot_counter_it() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Nessian Hornbeetle", Zone::Battlefield);
        let other = qualifying(&mut state, player);
        begin_combat(&mut state);
        assert!(!state.stack.is_empty());
        for (object, zone) in [
            (source, Zone::Hand),
            (source, Zone::Battlefield),
            (other, Zone::Graveyard),
        ] {
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(object, zone));
        }
        state.objects.get_mut(source).counters.plus1_plus1 = 2;
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            settle(current);
            assert_eq!(current.objects.get(source).counters.plus1_plus1, 2);
        }
        same(&state, &replay);
    }
}
#[test]
fn actual_witness_suppresses_printed_trigger_and_qualifying_power_until_removed() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for (enchant_source, remove, expected) in [
            (true, false, false),
            (false, false, false),
            (true, true, true),
        ] {
            let mut state = ready(player);
            let source = put(&mut state, player, "Nessian Hornbeetle", Zone::Battlefield);
            let other = put(&mut state, player, "Serra Angel", Zone::Battlefield);
            let aura = put(&mut state, player, "Witness Protection", Zone::Hand);
            state.players[player.index()].mana_pool[1] = 1;
            priority(&mut state, player);
            cast(
                &mut state,
                aura,
                Some(Target::Object(if enchant_source { source } else { other })),
            );
            settle(&mut state);
            assert_eq!(state.objects.get(aura).zone, Zone::Battlefield);
            if remove {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(aura, Zone::Graveyard),
                );
            }
            begin_combat(&mut state);
            assert_eq!(!state.stack.is_empty(), expected);
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                settle(current);
                assert_eq!(
                    current.objects.get(source).counters.plus1_plus1,
                    i32::from(expected)
                );
            }
            same(&state, &replay);
        }
    }
}
