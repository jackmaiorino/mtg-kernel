//! Prepared subtype cost reducers; admission and gameplay qualification remain pending.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, DynamicCountDef, GenericCostReductionDef, Subtype,
    CARD_DEFS,
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
        837,
        player,
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

fn pass(state: &mut GameState, decision: Decision) {
    let action = match decision {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        other => panic!("unexpected decision {other:?}"),
    };
    engine::step(state, action).unwrap();
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
        pass(state, decision);
    }
    panic!("resolution did not settle");
}

fn copy(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

#[test]
fn printed_definitions_and_reduction_programs_are_exact() {
    for (name, id, color, count) in [
        (
            "Arcane Epiphany",
            362,
            ManaColor::U,
            DynamicCountDef::ControllerHasPermanentSubtype(Subtype::Wizard),
        ),
        (
            "Claws Out",
            363,
            ManaColor::W,
            DynamicCountDef::ControllerBattlefieldSubtype(Subtype::Cat),
        ),
    ] {
        let card = card_id_by_name(name).unwrap();
        assert_eq!(card, id);
        let def = &CARD_DEFS[card as usize];
        assert_eq!(def.capability, CardCapability::Full);
        assert_eq!(def.types, &[CardType::Instant]);
        assert_eq!(def.mana_value, 5);
        assert_eq!(def.cost.generic, 3);
        assert_eq!(def.cost.pips, &[Pip::Colored(color), Pip::Colored(color)]);
        assert_eq!(
            def.generic_cost_reduction,
            Some(GenericCostReductionDef {
                generic_per_count: 1,
                count
            })
        );
        assert_eq!(def.target_spec, mtg_kernel::card_def::TargetSpec::None);
    }
}

#[test]
fn actual_payment_caps_wizard_presence_counts_cats_and_preserves_pips() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for (name, color, helper) in [
            ("Arcane Epiphany", ManaColor::U, "Faerie Seer"),
            ("Claws Out", ManaColor::W, "Cat Token"),
        ] {
            for count in [0u8, 1, 2, 3, 5] {
                let mut state = ready(player);
                for _ in 0..count {
                    put(&mut state, player, helper, Zone::Battlefield);
                }
                put(&mut state, player.opponent(), helper, Zone::Battlefield);
                let source = put(&mut state, player, name, Zone::Hand);
                let generic = 3u8.saturating_sub(if name == "Arcane Epiphany" {
                    u8::from(count != 0)
                } else {
                    count
                });
                state.players[player.index()].mana_pool[5] = generic;
                state.players[player.index()].mana_pool[color.pool_index()] = 1;
                next(&mut state);
                let before = serde_json::to_vec(&state).unwrap();
                assert!(engine::step(&mut state, Action::CastSpell(source)).is_err());
                assert_eq!(serde_json::to_vec(&state).unwrap(), before);
                state.players[player.index()].mana_pool[color.pool_index()] = 2;
                engine::step(&mut state, Action::CastSpell(source)).unwrap();
                settle(&mut state);
                assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
                assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
            }
        }
    }
}

#[test]
fn offers_and_payment_recompute_after_control_and_subtype_changes() {
    let mut state = ready(PlayerId::P0);
    let wizard = put(&mut state, PlayerId::P0, "Faerie Seer", Zone::Battlefield);
    let source = put(&mut state, PlayerId::P0, "Arcane Epiphany", Zone::Hand);
    state.players[0].mana_pool[5] = 2;
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 2;
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&source))
    );
    state.objects.get_mut(wizard).v4.effective_subtype_ids = vec![Subtype::Cat.stable_id()];
    let before = serde_json::to_vec(&state).unwrap();
    assert!(engine::step(&mut state, Action::CastSpell(source)).is_err());
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    state.objects.get_mut(wizard).v4.effective_subtype_ids = vec![Subtype::Wizard.stable_id()];
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(wizard, Zone::Exile));
    assert!(engine::step(&mut state, Action::CastSpell(source)).is_err());

    let mut state = ready(PlayerId::P0);
    let cat = put(&mut state, PlayerId::P1, "Cat Token", Zone::Battlefield);
    let source = put(&mut state, PlayerId::P0, "Claws Out", Zone::Hand);
    state.players[0].mana_pool[5] = 2;
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 2;
    next(&mut state);
    assert!(engine::step(&mut state, Action::CastSpell(source)).is_err());
    state.players[1].battlefield.retain(|&id| id != cat);
    state.players[0].battlefield.push(cat);
    state.objects.get_mut(cat).controller = PlayerId::P0;
    engine::step(&mut state, Action::CastSpell(source)).unwrap();
    settle(&mut state);
    assert_eq!(engine::effective_power(&state, cat), 3);
}

#[test]
fn draw_three_and_resolution_sampled_team_boost_restore_identically() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for (name, color) in [
            ("Arcane Epiphany", ManaColor::U),
            ("Claws Out", ManaColor::W),
        ] {
            let mut state = ready(player);
            let recipient = put(&mut state, player, "Faerie Miscreant", Zone::Battlefield);
            let other = put(
                &mut state,
                player.opponent(),
                "Faerie Miscreant",
                Zone::Battlefield,
            );
            let source = put(&mut state, player, name, Zone::Hand);
            state.players[player.index()].mana_pool[5] = 3;
            state.players[player.index()].mana_pool[color.pool_index()] = 2;
            next(&mut state);
            engine::step(&mut state, Action::CastSpell(source)).unwrap();
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            let mut restored = copy(&state);
            for game in [&mut state, &mut restored] {
                settle(game);
                assert_eq!(
                    game.players[player.index()].hand.len(),
                    if name == "Arcane Epiphany" { 3 } else { 0 }
                );
                assert_eq!(
                    engine::effective_power(game, recipient),
                    if name == "Claws Out" { 3 } else { 1 }
                );
                assert_eq!(engine::effective_power(game, other), 1);
                if name == "Claws Out" {
                    let late = put(game, player, "Faerie Miscreant", Zone::Battlefield);
                    assert_eq!(engine::effective_power(game, late), 1);
                    event::propose_and_commit(
                        game,
                        ProposedEvent::zone_change(recipient, Zone::Exile),
                    );
                    event::propose_and_commit(
                        game,
                        ProposedEvent::zone_change(recipient, Zone::Battlefield),
                    );
                    assert_eq!(engine::effective_power(game, recipient), 1);
                }
            }
            assert_eq!(state.state_hash(), restored.state_hash());
            assert_eq!(
                state.diagnostic_state_hash(),
                restored.diagnostic_state_hash()
            );
        }
    }
}
