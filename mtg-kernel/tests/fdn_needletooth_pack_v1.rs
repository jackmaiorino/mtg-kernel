//! Unregistered Needletooth Pack fixtures; gameplay admission remains pending.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Subtype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::effect::{EffectOp, ObjectRef};
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
        1110,
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
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn settle(state: &mut GameState) -> Decision {
    for _ in 0..64 {
        let choice = next(state);
        match choice {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return choice;
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

fn end(state: &mut GameState) -> Decision {
    assert!(state.stack.is_empty());
    state.step = Step::Main2;
    for _ in 0..32 {
        let choice = next(state);
        if state.step == Step::End {
            return choice;
        }
        assert!(matches!(choice, Decision::CastSpellOrPass { .. }));
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("end step did not begin");
}

fn death(state: &mut GameState, player: PlayerId, name: &str) {
    let victim = put(state, player, name, Zone::Battlefield);
    event::propose_and_commit(state, ProposedEvent::zone_change(victim, Zone::Graveyard));
}

fn choose(state: &mut GameState, player: PlayerId, target: ObjectId) {
    assert!(matches!(next(state), Decision::ChooseTargets {
        player: chooser, remaining: 1, legal_targets, ..
    } if chooser == player && legal_targets.contains(&Target::Object(target))));
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn transfer(state: &mut GameState, object: ObjectId, to: PlayerId) {
    let from = state.objects.get(object).controller;
    state.players[from.index()]
        .battlefield
        .retain(|&id| id != object);
    state.players[to.index()].battlefield.push(object);
    state.objects.get_mut(object).controller = to;
}

#[test]
fn printed_definition_and_actual_five_mana_cast_are_exact() {
    let id = card_id_by_name("Needletooth Pack").unwrap();
    let card = &CARD_DEFS[id as usize];
    assert_eq!(card.capability, CardCapability::Full);
    assert_eq!(
        (card.power, card.toughness, card.mana_value),
        (Some(4), Some(5), 5)
    );
    assert_eq!(card.types, &[CardType::Creature]);
    assert_eq!(card.subtypes, &[Subtype::Dinosaur]);
    assert_eq!(card.cost.generic, 3);
    assert_eq!(
        card.cost.pips,
        &[Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::G)]
    );
    for player in [PlayerId::P0, PlayerId::P1] {
        for (green, blue, legal) in [(1, 4, false), (2, 2, false), (2, 3, true)] {
            let mut state = ready(player);
            let spell = put(&mut state, player, card.name, Zone::Hand);
            state.players[player.index()].mana_pool[ManaColor::G.pool_index()] = green;
            state.players[player.index()].mana_pool[ManaColor::U.pool_index()] = blue;
            assert!(
                matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. }
                if castable_spells.contains(&spell) == legal)
            );
            let before = state.state_hash();
            if legal {
                engine::step(&mut state, Action::CastSpell(spell)).unwrap();
                assert!(matches!(
                    settle(&mut state),
                    Decision::CastSpellOrPass { .. }
                ));
                assert_eq!(state.objects.get(spell).zone, Zone::Battlefield);
                assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
            } else {
                assert!(engine::step(&mut state, Action::CastSpell(spell)).is_err());
                assert_eq!(state.state_hash(), before);
            }
        }
    }
}

#[test]
fn actual_deaths_trigger_once_while_false_positives_and_opponents_end_do_not() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for (name, from, to, morbid) in [
            ("Llanowar Elves", Zone::Battlefield, Zone::Graveyard, true),
            (
                "Koma's Coil Token",
                Zone::Battlefield,
                Zone::Graveyard,
                true,
            ),
            ("Forest", Zone::Battlefield, Zone::Graveyard, false),
            ("Llanowar Elves", Zone::Hand, Zone::Graveyard, false),
            ("Llanowar Elves", Zone::Battlefield, Zone::Hand, false),
            ("Llanowar Elves", Zone::Battlefield, Zone::Exile, false),
        ] {
            for victim_owner in [player, player.opponent()] {
                let mut state = ready(player);
                let source = put(&mut state, player, "Needletooth Pack", Zone::Battlefield);
                let victim = put(&mut state, victim_owner, name, from);
                event::propose_and_commit(&mut state, ProposedEvent::zone_change(victim, to));
                assert_eq!(state.creature_died_this_turn_v1(), morbid);
                let choice = end(&mut state);
                if morbid {
                    assert!(matches!(choice, Decision::ChooseTargets { .. }));
                    choose(&mut state, player, source);
                    settle(&mut state);
                    assert_eq!(state.objects.get(source).counters.plus1_plus1, 2);
                    next(&mut state);
                    assert!(state.stack.is_empty());
                } else {
                    assert!(matches!(choice, Decision::CastSpellOrPass { .. }));
                    assert!(state.engine.pending_triggers.is_empty());
                    assert_eq!(state.objects.get(source).counters.plus1_plus1, 0);
                }
            }
        }
        for active in [player, player.opponent()] {
            let mut state = ready(active);
            put(&mut state, player, "Needletooth Pack", Zone::Battlefield);
            if active != player {
                death(&mut state, active, "Llanowar Elves");
            }
            assert!(matches!(end(&mut state), Decision::CastSpellOrPass { .. }));
            assert!(state.engine.pending_triggers.is_empty());
        }
    }
}

#[test]
fn mandatory_controlled_targets_and_pending_and_stack_restores_match() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Needletooth Pack", Zone::Battlefield);
        let own = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        let other = put(
            &mut state,
            player.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let land = put(&mut state, player, "Forest", Zone::Battlefield);
        death(&mut state, player.opponent(), "Llanowar Elves");
        assert!(
            matches!(end(&mut state), Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(source))
                && legal_targets.contains(&Target::Object(own))
                && !legal_targets.contains(&Target::Object(other))
                && !legal_targets.contains(&Target::Object(land)))
        );
        for illegal in [
            Target::Object(other),
            Target::Object(land),
            Target::Player(player),
        ] {
            let before = state.state_hash();
            assert!(engine::step(&mut state, Action::ChooseTarget(illegal)).is_err());
            assert_eq!(state.state_hash(), before);
        }
        let mut pending = restored(&state);
        choose(&mut state, player, own);
        choose(&mut pending, player, own);
        let mut stacked = restored(&state);
        for current in [&mut state, &mut pending, &mut stacked] {
            settle(current);
            assert_eq!(current.objects.get(own).counters.plus1_plus1, 2);
            assert_eq!(current.objects.get(source).counters.plus1_plus1, 0);
        }
        assert_eq!(
            serde_json::to_value(&state).unwrap(),
            serde_json::to_value(&pending).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&state).unwrap(),
            serde_json::to_value(&stacked).unwrap()
        );
    }
}

#[test]
fn source_departure_keeps_captured_controller_and_illegal_target_fizzles() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for change in 0..4 {
            let mut state = ready(player);
            let source = put(&mut state, player, "Needletooth Pack", Zone::Battlefield);
            let target = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
            death(&mut state, player.opponent(), "Llanowar Elves");
            end(&mut state);
            choose(&mut state, player, target);
            match change {
                0 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Hand),
                    );
                }
                1 => transfer(&mut state, source, player.opponent()),
                2 => transfer(&mut state, target, player.opponent()),
                3 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(target, Zone::Hand),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(target, Zone::Battlefield),
                    );
                }
                _ => unreachable!(),
            }
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                settle(current);
                assert_eq!(
                    current.objects.get(target).counters.plus1_plus1,
                    if change < 2 { 2 } else { 0 }
                );
            }
            assert_eq!(
                serde_json::to_value(&state).unwrap(),
                serde_json::to_value(&replay).unwrap()
            );
        }
    }
}

#[test]
fn removed_ability_prevents_capture_and_forged_pending_roots_halt() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Needletooth Pack", Zone::Battlefield);
        let witness = put(&mut state, player, "Witness Protection", Zone::Hand);
        state.players[player.index()].mana_pool[ManaColor::U.pool_index()] = 1;
        next(&mut state);
        engine::step(&mut state, Action::CastSpell(witness)).unwrap();
        assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(source))).unwrap();
        settle(&mut state);
        death(&mut state, player.opponent(), "Llanowar Elves");
        assert!(matches!(end(&mut state), Decision::CastSpellOrPass { .. }));
        assert!(state.engine.pending_triggers.is_empty());
        assert_eq!(state.objects.get(source).counters.plus1_plus1, 0);

        let mut state = ready(player);
        put(&mut state, player, "Needletooth Pack", Zone::Battlefield);
        death(&mut state, player.opponent(), "Llanowar Elves");
        assert!(matches!(end(&mut state), Decision::ChooseTargets { .. }));
        for forge_effect in [false, true] {
            let mut forged = restored(&state);
            if forge_effect {
                forged.engine.pending_triggers[0].effect = EffectOp::AddPlusOnePlusOneCounters {
                    object: ObjectRef::Target(0),
                    count: 3,
                };
            } else {
                forged.engine.pending_triggers[0].target_spec = TargetSpec::None;
            }
            let mut replay = restored(&forged);
            assert!(matches!(
                engine::advance_until_decision(&mut replay),
                Decision::Halted { .. }
            ));
        }
    }
}
