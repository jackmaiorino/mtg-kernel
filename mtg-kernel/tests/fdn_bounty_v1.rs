//! Prepared serial v70 Brass's Bounty admission cases.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, CardType, DynamicValueDef, CARD_DEFS};
use mtg_kernel::effect::{EffectOp, PlayerRef};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Zone};

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
        if matches!(d, Decision::CastSpellOrPass { .. }) && state.stack.is_empty() {
            return;
        }
        assert!(matches!(d, Decision::CastSpellOrPass { .. }));
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("spell did not resolve")
}

fn cast(state: &mut GameState, player: PlayerId) -> ObjectId {
    let spell = put(state, player, "Brass's Bounty", Zone::Hand);
    state.players[player.index()].mana_pool[5] = 6;
    state.players[player.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    next(state);
    engine::step(state, Action::CastSpell(spell)).unwrap();
    assert_eq!(state.objects.get(spell).zone, Zone::Stack);
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(
        state.stack.len(),
        1,
        "finalized spell awaits priority passes"
    );
    spell
}

fn treasures(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    let token_def = card_id_by_name("Treasure Token").unwrap();
    state
        .objects
        .iter()
        .filter_map(|(id, object)| {
            (object.zone == Zone::Battlefield
                && object.controller == player
                && object.card_def == token_def)
                .then_some(id)
        })
        .collect()
}

#[test]
fn bounty_characteristics_cost_and_exact_dynamic_program_are_pinned() {
    let id = card_id_by_name("Brass's Bounty").unwrap();
    assert_eq!(id, 344);
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.types, &[CardType::Sorcery]);
    assert_eq!(def.mana_value, 7);
    assert_eq!(def.cost.generic, 6);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(def.colors, &[ManaColor::R]);
    assert!(
        matches!((def.spell_effect)(), Some(EffectOp::CreateTokensDynamic {
        token_def, controller: PlayerRef::Controller,
        count: DynamicValueDef::ControlledPermanentsWithType(CardType::Land),
        tapped: false,
    }) if token_def == card_id_by_name("Treasure Token").unwrap())
    );
    let mut state = ready(PlayerId::P0);
    let spell = put(&mut state, PlayerId::P0, "Brass's Bounty", Zone::Hand);
    state.players[0].mana_pool[5] = 5;
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
    next(&mut state);
    let before = state.state_hash();
    assert!(engine::step(&mut state, Action::CastSpell(spell)).is_err());
    assert_eq!(state.state_hash(), before);
    state.players[0].mana_pool[5] += 1;
    engine::step(&mut state, Action::CastSpell(spell)).unwrap();
    settle(&mut state);
    assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
    assert!(treasures(&state, PlayerId::P0).is_empty());
}

#[test]
fn bounty_counts_zero_small_and_more_than_u8_lands_for_both_players() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for count in [0, 1, 257] {
            let mut state = ready(player);
            for _ in 0..count {
                put(&mut state, player, "Forest", Zone::Battlefield);
            }
            put(&mut state, player.opponent(), "Forest", Zone::Battlefield);
            put(&mut state, player, "Faerie Miscreant", Zone::Battlefield);
            cast(&mut state, player);
            settle(&mut state);
            let tokens = treasures(&state, player);
            assert_eq!(tokens.len(), count);
            assert!(treasures(&state, player.opponent()).is_empty());
            for token in tokens {
                let object = state.objects.get(token);
                assert!(object.v4.is_token);
                assert!(!object.tapped);
                assert_eq!(object.owner, player);
                assert!(CARD_DEFS[object.card_def as usize].has_full_support());
            }
        }
    }
}

#[test]
fn bounty_samples_current_control_and_lands_after_priority_with_restore() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let departed = put(&mut state, player, "Forest", Zone::Battlefield);
        let borrowed = put(
            &mut state,
            player.opponent(),
            "Great Furnace",
            Zone::Battlefield,
        );
        let returned = put(&mut state, player, "Forest", Zone::Battlefield);
        state.objects.get_mut(returned).controller = player.opponent();
        let spell = cast(&mut state, player);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(departed, Zone::Hand));
        state.objects.get_mut(borrowed).controller = player;
        let late = put(&mut state, player, "Forest", Zone::Battlefield);
        state.objects.get_mut(late).v4.is_token = true;
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut replay] {
            settle(branch);
            assert_eq!(treasures(branch, player).len(), 2);
            assert_eq!(branch.objects.get(spell).zone, Zone::Graveyard);
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}
