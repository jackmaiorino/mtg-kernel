//! Prepared Raise the Past cases; registration and gameplay qualification are pending.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, CardType, TargetSpec, CARD_DEFS};
use mtg_kernel::effect::EffectOp;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Zone};

fn ready(player: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        800,
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
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
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
    for _ in 0..64 {
        let action = match next(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
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

fn cast(state: &mut GameState, player: PlayerId) -> ObjectId {
    let spell = put(state, player, "Raise the Past", Zone::Hand);
    state.players[player.index()].mana_pool[5] = 2;
    state.players[player.index()].mana_pool[ManaColor::W.pool_index()] = 2;
    next(state);
    engine::step(state, Action::CastSpell(spell)).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert_eq!(state.stack.len(), 1);
    assert!(state.engine.pending_cast.is_none());
    spell
}

#[test]
fn raise_past_has_printed_sorcery_cost_and_untargeted_current_graveyard_program() {
    let id = card_id_by_name("Raise the Past").unwrap();
    assert_eq!(id, 355);
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.types, &[CardType::Sorcery]);
    assert_eq!(def.mana_value, 4);
    assert_eq!(def.cost.generic, 2);
    assert_eq!(
        def.cost.pips,
        &[Pip::Colored(ManaColor::W), Pip::Colored(ManaColor::W)]
    );
    assert_eq!(def.target_spec, TargetSpec::None);
    assert_eq!(
        (def.spell_effect)(),
        Some(EffectOp::ReturnOwnGraveyardCreaturesManaValueAtMost { max_mana_value: 2 })
    );
    let mut state = ready(PlayerId::P0);
    let spell = put(&mut state, PlayerId::P0, "Raise the Past", Zone::Hand);
    state.players[0].mana_pool[5] = 4;
    next(&mut state);
    let before = serde_json::to_vec(&state).unwrap();
    assert!(engine::step(&mut state, Action::CastSpell(spell)).is_err());
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
}

#[test]
fn raise_past_returns_only_current_own_small_creature_cards_and_restores_stack() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let small = put(&mut state, player, "Ajani's Pridemate", Zone::Graveyard);
        let large = put(&mut state, player, "Tolarian Terror", Zone::Graveyard);
        let land = put(&mut state, player, "Forest", Zone::Graveyard);
        let noncreature = put(&mut state, player, "Counterspell", Zone::Graveyard);
        let opponent = put(
            &mut state,
            player.opponent(),
            "Ajani's Pridemate",
            Zone::Graveyard,
        );
        let hand = put(&mut state, player, "Ajani's Pridemate", Zone::Hand);
        let spell = cast(&mut state, player);
        // The effect samples resolution-time contents, rather than cast-time contents.
        let later = put(&mut state, player, "Goblin Bushwhacker", Zone::Graveyard);
        state.objects.get_mut(small).controller = player.opponent();
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut restored] {
            settle(branch);
            for id in [small, later] {
                let object = branch.objects.get(id);
                assert_eq!(object.zone, Zone::Battlefield);
                assert_eq!(object.controller, player);
                assert_eq!(object.zone_change_count, 1);
                assert!(!object.tapped);
                assert!(object.summoning_sick);
            }
            for id in [large, land, noncreature, opponent] {
                assert_eq!(branch.objects.get(id).zone, Zone::Graveyard);
            }
            assert_eq!(branch.objects.get(hand).zone, Zone::Hand);
            assert_eq!(branch.objects.get(spell).zone, Zone::Graveyard);
        }
        assert_eq!(state.state_hash(), restored.state_hash());
    }
}

#[test]
fn raise_past_simultaneous_faeries_both_trigger_and_restore_pending_order() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let cards = [
            put(&mut state, player, "Faerie Miscreant", Zone::Graveyard),
            put(&mut state, player, "Faerie Miscreant", Zone::Graveyard),
        ];
        cast(&mut state, player);
        let mut found = false;
        for _ in 0..32 {
            match next(&mut state) {
                Decision::OrderTriggers { pending, .. } => {
                    assert_eq!(pending.len(), 2);
                    found = true;
                    break;
                }
                Decision::CastSpellOrPass { .. } if !state.stack.is_empty() => {
                    engine::step(&mut state, Action::Pass).unwrap()
                }
                other => panic!("missing simultaneous ETB ordering {other:?}"),
            }
        }
        assert!(found);
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut restored] {
            settle(branch);
            assert_eq!(branch.players[player.index()].hand.len(), 2);
            for id in cards {
                assert_eq!(branch.objects.get(id).zone, Zone::Battlefield);
            }
            assert_eq!(branch.engine.event_history.iter().filter(|event| matches!(event, mtg_kernel::event::CommittedEvent::Draw { player: actor, .. } if *actor == player)).count(), 2);
        }
        assert_eq!(state.state_hash(), restored.state_hash());
    }
}
