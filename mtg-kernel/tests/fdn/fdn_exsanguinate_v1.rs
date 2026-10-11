//! Prepared v72 Exsanguinate admission cases.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, CardType, CARD_DEFS};
use mtg_kernel::effect::EffectOp;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::CommittedEvent;
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Zone};

fn ready(player: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        720,
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
        if matches!(d, Decision::GameOver { .. }) {
            return;
        }
        if matches!(d, Decision::CastSpellOrPass { .. }) && state.stack.is_empty() {
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }));
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("spell did not resolve")
}

fn begin(state: &mut GameState, player: PlayerId, available_x: u8) -> ObjectId {
    let source = put(state, player, "Exsanguinate", Zone::Hand);
    state.players[player.index()].mana_pool[ManaColor::B.pool_index()] = 2;
    state.players[player.index()].mana_pool[5] = available_x;
    next(state);
    engine::step(state, Action::CastSpell(source)).unwrap();
    source
}

#[test]
fn exsanguinate_characteristics_x_cost_and_effect_are_exact() {
    let id = card_id_by_name("Exsanguinate").unwrap();
    assert_eq!(id, 346);
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.types, &[CardType::Sorcery]);
    assert_eq!(def.mana_value, 2);
    assert_eq!(def.cost.generic, 0);
    assert_eq!(def.cost.x_count, 1);
    assert_eq!(
        def.cost.pips,
        &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::B)]
    );
    assert!(matches!(
        (def.spell_effect)(),
        Some(EffectOp::LoseOpponentsLifeXThenGainLifeLost)
    ));
    let mut state = ready(PlayerId::P0);
    let source = put(&mut state, PlayerId::P0, "Exsanguinate", Zone::Hand);
    state.players[0].mana_pool[ManaColor::B.pool_index()] = 1;
    state.players[0].mana_pool[5] = 10;
    next(&mut state);
    let before = state.state_hash();
    assert!(engine::step(&mut state, Action::CastSpell(source)).is_err());
    assert_eq!(state.state_hash(), before);
}

#[test]
fn exsanguinate_x_choice_and_final_stack_restore_for_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for x in [0, 3, 255] {
            let mut state = ready(player);
            state.players[player.opponent().index()].life = 2;
            let source = begin(&mut state, player, 255);
            assert!(matches!(next(&mut state), Decision::ChooseEffectOption {
                player: actor, source: object, option_count: 256,
            } if actor == player && object == source));
            let before = state.state_hash();
            assert!(engine::step(&mut state, Action::ChooseEffectOption(256)).is_err());
            assert_eq!(state.state_hash(), before);
            let mut pending_restore: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut pending_restore] {
                engine::step(branch, Action::ChooseEffectOption(x)).unwrap();
                assert!(matches!(next(branch), Decision::CastSpellOrPass { .. }));
                assert!(branch.engine.pending_cast.is_none());
                assert_eq!(branch.stack.len(), 1);
            }
            assert_eq!(state.state_hash(), pending_restore.state_hash());
            let mut stack_restore: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut pending_restore, &mut stack_restore] {
                settle(branch);
                if x >= 2 {
                    assert!(
                        matches!(next(branch), Decision::GameOver { winner: Some(winner) }
                        if winner == player)
                    );
                } else {
                    assert!(matches!(next(branch), Decision::CastSpellOrPass { .. }));
                }
                assert_eq!(branch.players[player.index()].life, 20 + i32::from(x));
                assert_eq!(
                    branch.players[player.opponent().index()].life,
                    2 - i32::from(x)
                );
                assert_eq!(branch.objects.get(source).zone, Zone::Graveyard);
                assert!(!branch
                    .engine
                    .event_history
                    .iter()
                    .any(|event| matches!(event, CommittedEvent::Damage { .. })));
            }
            assert_eq!(state.state_hash(), pending_restore.state_hash());
            assert_eq!(state.state_hash(), stack_restore.state_hash());
        }
    }
}

#[test]
fn exsanguinate_zero_x_is_castable_with_only_two_black_mana() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = begin(&mut state, player, 0);
        settle(&mut state);
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        assert_eq!(state.players[player.index()].life, 20);
        assert_eq!(state.players[player.opponent().index()].life, 20);
        assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
    }
}
