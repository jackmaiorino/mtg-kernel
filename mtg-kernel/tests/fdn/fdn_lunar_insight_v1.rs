//! Prepared serial v78 Lunar Insight admission cases.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, CardType, DynamicValueDef, CARD_DEFS};
use mtg_kernel::effect::{EffectOp, PlayerRef};
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
    let spell = put(state, player, "Lunar Insight", Zone::Hand);
    state.players[player.index()].mana_pool[5] = 2;
    state.players[player.index()].mana_pool[ManaColor::U.pool_index()] = 1;
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

#[test]
fn lunar_insight_has_exact_printed_metadata_cost_and_dynamic_program() {
    let id = card_id_by_name("Lunar Insight").unwrap();
    assert_eq!(id, 353);
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.types, &[CardType::Sorcery]);
    assert_eq!(def.mana_value, 3);
    assert_eq!(def.cost.generic, 2);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::U)]);
    assert!(matches!(
        (def.spell_effect)(),
        Some(EffectOp::DrawCardsDynamic {
            player: PlayerRef::Controller,
            count: DynamicValueDef::DistinctManaValuesAmongControlledNonlandPermanents,
        })
    ));
    let mut state = ready(PlayerId::P0);
    let spell = put(&mut state, PlayerId::P0, "Lunar Insight", Zone::Hand);
    state.players[0].mana_pool[5] = 3;
    next(&mut state);
    let before = serde_json::to_vec(&state).unwrap();
    assert!(engine::step(&mut state, Action::CastSpell(spell)).is_err());
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
}

#[test]
fn lunar_insight_samples_distinct_nonland_values_at_resolution_with_restore() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        put(&mut state, player, "Faerie Miscreant", Zone::Battlefield);
        put(&mut state, player, "Faerie Miscreant", Zone::Battlefield);
        put(&mut state, player, "Great Furnace", Zone::Battlefield);
        let token = put(&mut state, player, "Treasure Token", Zone::Battlefield);
        state.objects.get_mut(token).v4.is_token = true;
        let borrowed = put(
            &mut state,
            player.opponent(),
            "Tolarian Terror",
            Zone::Battlefield,
        );
        let source = cast(&mut state, player);
        // Newly controlled mana value seven participates only at resolution.
        state.players[player.opponent().index()]
            .battlefield
            .retain(|&id| id != borrowed);
        state.players[player.index()].battlefield.push(borrowed);
        state.objects.get_mut(borrowed).controller = player;
        let before_hand = state.players[player.index()].hand.len();
        let before_library = state.players[player.index()].library.len();
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(state.state_hash(), replay.state_hash());
        settle(&mut state);
        settle(&mut replay);
        assert_eq!(state.players[player.index()].hand.len(), before_hand + 3);
        assert_eq!(
            state.players[player.index()].library.len(),
            before_library - 3
        );
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}

#[test]
fn lunar_insight_with_only_lands_draws_nothing() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        put(&mut state, player, "Forest", Zone::Battlefield);
        put(&mut state, player, "Great Furnace", Zone::Battlefield);
        put(
            &mut state,
            player.opponent(),
            "Tolarian Terror",
            Zone::Battlefield,
        );
        let source = cast(&mut state, player);
        let before = state.players[player.index()].hand.len();
        let library_before = state.players[player.index()].library.len();
        settle(&mut state);
        assert_eq!(state.players[player.index()].hand.len(), before);
        assert_eq!(state.players[player.index()].library.len(), library_before);
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
    }
}
