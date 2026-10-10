//! Prepared serial v79 Hidetsugu's Second Rite admission cases.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, CardType, CARD_DEFS};
use mtg_kernel::effect::{EffectCond, EffectOp};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::CommittedEvent;
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready(player: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        700,
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
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Hand => state.players[player.index()].hand.push(id),
        _ => panic!("fixture zone"),
    }
    id
}

fn next(state: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(state);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}

fn settle(state: &mut GameState) {
    for _ in 0..64 {
        let d = next(state);
        if matches!(d, Decision::GameOver { .. })
            || (matches!(d, Decision::CastSpellOrPass { .. }) && state.stack.is_empty())
        {
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }));
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("spell did not resolve")
}

fn cast(state: &mut GameState, player: PlayerId, target: PlayerId) -> ObjectId {
    let spell = put(state, player, "Hidetsugu's Second Rite", Zone::Hand);
    state.players[player.index()].mana_pool[5] = 3;
    state.players[player.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    next(state);
    engine::step(state, Action::CastSpell(spell)).unwrap();
    assert!(matches!(
        next(state),
        Decision::ChooseTargets { remaining: 1, .. }
    ));
    engine::step(state, Action::ChooseTarget(Target::Player(target))).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert_eq!(state.stack.len(), 1);
    assert!(state.engine.pending_cast.is_none());
    spell
}

#[test]
fn second_rite_has_exact_printed_instant_cost_and_conditional_program() {
    let id = card_id_by_name("Hidetsugu's Second Rite").unwrap();
    assert_eq!(id, 354);
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.types, &[CardType::Instant]);
    assert_eq!(def.mana_value, 4);
    assert_eq!(def.cost.generic, 3);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(def.target_spec, mtg_kernel::card_def::TargetSpec::AnyPlayer);
    assert!(matches!(
        (def.spell_effect)(),
        Some(EffectOp::Conditional {
            cond: EffectCond::TargetPlayerLifeTotalEquals { index: 0, life: 10 },
            ..
        })
    ));
    let mut state = ready(PlayerId::P0);
    let spell = put(
        &mut state,
        PlayerId::P0,
        "Hidetsugu's Second Rite",
        Zone::Hand,
    );
    state.players[0].mana_pool[5] = 4;
    next(&mut state);
    let before = serde_json::to_vec(&state).unwrap();
    assert!(engine::step(&mut state, Action::CastSpell(spell)).is_err());
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
}

#[test]
fn second_rite_checks_resolution_life_for_either_player_and_restores_lethal_damage() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for target in [player, player.opponent()] {
            for announced_life in [9, 10, 11] {
                for resolution_life in [9, 10, 11] {
                    let mut state = ready(player);
                    state.players[target.index()].life = announced_life;
                    let source = cast(&mut state, player, target);
                    state.players[target.index()].life = resolution_life;
                    let mut replay: GameState =
                        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                    assert_eq!(state.state_hash(), replay.state_hash());
                    settle(&mut state);
                    settle(&mut replay);
                    let deals_damage = resolution_life == 10;
                    assert_eq!(
                        state.players[target.index()].life,
                        if deals_damage { 0 } else { resolution_life }
                    );
                    assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
                    let damage_count = state
                        .engine
                        .event_history
                        .iter()
                        .filter(|event| matches!(event, CommittedEvent::Damage { .. }))
                        .count();
                    assert_eq!(damage_count, usize::from(deals_damage));
                    if deals_damage {
                        assert!(
                            matches!(next(&mut state), Decision::GameOver { winner: Some(winner) } if winner == target.opponent())
                        );
                    }
                    assert_eq!(state.state_hash(), replay.state_hash());
                }
            }
        }
    }
}
