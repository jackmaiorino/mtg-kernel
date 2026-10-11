//! Registered v68 candidate; receipts are in fdn_conditional_flash_power_admission_v1.md.
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

fn ready(player: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        957,
        player,
    );
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let object = state.objects.push(GameObject {
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
        Zone::Hand => state.players[player.index()].hand.push(object),
        Zone::Battlefield => state.players[player.index()].battlefield.push(object),
        _ => panic!("fixture zone"),
    }
    object
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn offered(state: &mut GameState, spell: ObjectId) -> bool {
    matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. }
        if castable_spells.contains(&spell))
}

fn settle(state: &mut GameState) {
    for _ in 0..64 {
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
            other => panic!("unexpected resolution decision {other:?}"),
        };
        engine::step(state, action).unwrap();
    }
    panic!("resolution did not settle");
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn priority(state: &mut GameState, player: PlayerId) {
    for _ in 0..4 {
        match next(state) {
            Decision::CastSpellOrPass {
                player: current, ..
            } if current == player => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected priority {other:?}"),
        }
    }
    panic!("desired priority not reached");
}

#[test]
fn trickster_printed_definition_and_opponents_turn_flash_cast() {
    let definition = &CARD_DEFS[card_id_by_name("High Fae Trickster").unwrap() as usize];
    assert_eq!(definition.capability, CardCapability::Full);
    assert_eq!(definition.types, &[CardType::Creature]);
    assert_eq!(definition.subtypes, &[Subtype::Faerie, Subtype::Wizard]);
    assert_eq!(definition.colors, &[ManaColor::U]);
    assert_eq!(definition.cost.generic, 3);
    assert_eq!(definition.cost.pips, vec![Pip::Colored(ManaColor::U)]);
    assert_eq!((definition.power, definition.toughness), (Some(4), Some(2)));
    assert!(definition.keywords.has(Keywords::FLYING));
    assert!(definition.keywords.has(Keywords::FLASH));
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player.opponent());
        let trickster = put(&mut state, player, "High Fae Trickster", Zone::Hand);
        state.players[player.index()].mana_pool = [0, 1, 0, 0, 0, 3];
        priority(&mut state, player);
        assert!(offered(&mut state, trickster));
        engine::step(&mut state, Action::CastSpell(trickster)).unwrap();
        settle(&mut state);
        assert_eq!(state.objects.get(trickster).zone, Zone::Battlefield);
        assert_eq!(state.active_player, player.opponent());
        assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
    }
}

#[test]
fn controller_permission_casts_creature_sorcery_and_artifact_without_granting_flash() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for name in ["Llanowar Elves", "Boltwave", "Swiftfoot Boots"] {
            let mut state = ready(player.opponent());
            let trickster = put(&mut state, player, "High Fae Trickster", Zone::Battlefield);
            let spell = put(&mut state, player, name, Zone::Hand);
            state.players[player.index()].mana_pool = [2; 6];
            priority(&mut state, player);
            assert!(offered(&mut state, spell), "{name}");
            assert!(!engine::has_effective_keyword(
                &state,
                spell,
                Keywords::FLASH
            ));
            let mut resumed = restored(&state);
            for game in [&mut state, &mut resumed] {
                engine::step(game, Action::CastSpell(spell)).unwrap();
                settle(game);
                assert_ne!(game.objects.get(spell).zone, Zone::Hand);
                assert_eq!(game.objects.get(trickster).zone, Zone::Battlefield);
                assert_eq!(game.active_player, player.opponent());
            }
            assert_eq!(
                serde_json::to_value(state).unwrap(),
                serde_json::to_value(resumed).unwrap()
            );
        }
    }
}

#[test]
fn permission_does_not_supply_mana_land_play_or_sorcery_activation() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player.opponent());
        put(&mut state, player, "High Fae Trickster", Zone::Battlefield);
        let elf = put(&mut state, player, "Llanowar Elves", Zone::Hand);
        let land = put(&mut state, player, "Forest", Zone::Hand);
        let synthesizer = put(
            &mut state,
            player,
            "Experimental Synthesizer",
            Zone::Battlefield,
        );
        state.players[player.index()].mana_pool = [0, 0, 0, 3, 0, 9];
        priority(&mut state, player);
        match next(&mut state) {
            Decision::CastSpellOrPass {
                castable_spells,
                land_drops,
                activatable_abilities,
                ..
            } => {
                assert!(!castable_spells.contains(&elf));
                assert!(!land_drops.contains(&land));
                assert!(!activatable_abilities.contains(&(synthesizer, 0)));
            }
            other => panic!("expected priority {other:?}"),
        }
        let before = serde_json::to_value(&state).unwrap();
        assert!(engine::step(&mut state, Action::CastSpell(elf)).is_err());
        assert_eq!(serde_json::to_value(state).unwrap(), before);
    }
}

#[test]
fn current_controller_and_source_departure_control_permission_after_restore() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for source_zone in [Zone::Hand, Zone::Battlefield] {
            let mut state = ready(player.opponent());
            let source = put(&mut state, player, "High Fae Trickster", source_zone);
            let spell = put(&mut state, player, "Llanowar Elves", Zone::Hand);
            state.players[player.index()].mana_pool[4] = 1;
            priority(&mut state, player);
            assert_eq!(offered(&mut state, spell), source_zone == Zone::Battlefield);
            if source_zone != Zone::Battlefield {
                continue;
            }
            let mut changed_controller = restored(&state);
            changed_controller.players[player.index()]
                .battlefield
                .retain(|&id| id != source);
            changed_controller.players[player.opponent().index()]
                .battlefield
                .push(source);
            changed_controller.objects.get_mut(source).controller = player.opponent();
            assert!(!offered(&mut changed_controller, spell));
            let mut resumed = restored(&state);
            for game in [&mut state, &mut resumed] {
                event::propose_and_commit(
                    game,
                    ProposedEvent::zone_change(source, Zone::Graveyard),
                );
                assert!(!offered(game, spell));
                event::propose_and_commit(
                    game,
                    ProposedEvent::zone_change(source, Zone::Battlefield),
                );
                assert!(offered(game, spell));
            }
            assert_eq!(
                serde_json::to_value(state).unwrap(),
                serde_json::to_value(resumed).unwrap()
            );
        }
    }
}

#[test]
fn witness_protection_removes_static_permission_but_pending_aura_cast_completes() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player.opponent());
        let source = put(&mut state, player, "High Fae Trickster", Zone::Battlefield);
        let aura = put(&mut state, player, "Witness Protection", Zone::Hand);
        let elf = put(&mut state, player, "Llanowar Elves", Zone::Hand);
        state.players[player.index()].mana_pool = [0, 1, 0, 0, 1, 0];
        priority(&mut state, player);
        assert!(offered(&mut state, aura));
        engine::step(&mut state, Action::CastSpell(aura)).unwrap();
        assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
        let mut resumed = restored(&state);
        for game in [&mut state, &mut resumed] {
            engine::step(game, Action::ChooseTarget(Target::Object(source))).unwrap();
            settle(game);
            assert_eq!(game.objects.get(aura).zone, Zone::Battlefield);
            priority(game, player);
            assert!(!offered(game, elf));
            assert!(!engine::has_effective_keyword(
                game,
                source,
                Keywords::FLYING
            ));
            event::propose_and_commit(game, ProposedEvent::zone_change(aura, Zone::Graveyard));
            assert!(offered(game, elf));
        }
        assert_eq!(
            serde_json::to_value(state).unwrap(),
            serde_json::to_value(resumed).unwrap()
        );
    }
}

#[test]
fn restored_adventure_form_choice_retains_flash_permission_for_both_forms() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for mode in [0, 1] {
            let mut state = ready(player.opponent());
            put(&mut state, player, "High Fae Trickster", Zone::Battlefield);
            let dragon = put(&mut state, player, "Fang Dragon", Zone::Hand);
            state.players[player.index()].mana_pool = [0, 0, 0, 2, 0, 5];
            priority(&mut state, player);
            assert!(offered(&mut state, dragon));
            engine::step(&mut state, Action::CastSpell(dragon)).unwrap();
            match next(&mut state) {
                Decision::ChooseSpellMode { legal_modes, .. } => {
                    assert_eq!(legal_modes, vec![0, 1]);
                }
                other => panic!("expected Adventure form choice {other:?}"),
            }
            let mut resumed = restored(&state);
            for game in [&mut state, &mut resumed] {
                engine::step(game, Action::ChooseSpellMode(mode)).unwrap();
                settle(game);
                assert_eq!(
                    game.objects.get(dragon).zone,
                    if mode == 0 {
                        Zone::Battlefield
                    } else {
                        Zone::Exile
                    }
                );
                assert_eq!(game.active_player, player.opponent());
                if mode == 1 {
                    priority(game, player);
                    game.players[player.index()].mana_pool = [0, 0, 0, 2, 0, 5];
                    assert!(offered(game, dragon));
                    engine::step(game, Action::CastSpell(dragon)).unwrap();
                    settle(game);
                    assert_eq!(game.objects.get(dragon).zone, Zone::Battlefield);
                }
            }
            assert_eq!(
                serde_json::to_value(state).unwrap(),
                serde_json::to_value(resumed).unwrap()
            );
        }
    }
}
