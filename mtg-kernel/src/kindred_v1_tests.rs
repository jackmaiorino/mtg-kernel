//! The Kindred card type in the frozen public forms and type bits (#716).
//! Each earlier card keeps its output; a Kindred card is either listed with
//! its Kindred type or, in a fixed bit set without a Kindred bit, left out.

use super::*;
use crate::ids::{ObjectId, PlayerId};
use crate::state::{CastMethodV4, GameObject, GameState, ObjectStateV4, Zone};

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

/// A game with `name` in P0's graveyard.
fn in_graveyard(name: &str) -> (GameState, ObjectId) {
    let island = id("Island");
    let mut state = GameState::new_from_libraries(
        &[island; 10],
        &[island; 10],
        |card| CARD_DEFS[card as usize].object_name.into(),
        0x4b49_4e44,
    );
    let card_def = id(name);
    let object = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner: PlayerId::P0,
        controller: PlayerId::P0,
        zone: Zone::Graveyard,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    state.players[PlayerId::P0.index()].graveyard.push(object);
    (state, object)
}

#[test]
fn a_kindred_card_has_both_of_its_types() {
    for name in ["Tarfire", "Kozilek's Command", "Faerie Trickery"] {
        let (state, card) = in_graveyard(name);
        let types: Vec<CardType> = CardType::ALL
            .into_iter()
            .filter(|&card_type| crate::engine::object_has_type(&state, card, card_type))
            .collect();
        assert_eq!(types, [CardType::Instant, CardType::Kindred], "{name}");
    }
}

/// The public forms list every type `object_has_type` reports, from
/// `CardType::ALL`, so a Kindred card's form names Kindred too.
#[test]
fn public_forms_list_the_kindred_type() {
    let (state, tarfire) = in_graveyard("Tarfire");
    let form = crate::optional_trigger_public_v1::checked_form(&state, tarfire).unwrap();
    assert_eq!(form.types, [CardType::Instant, CardType::Kindred]);
    let (state, bolt) = in_graveyard("Lightning Bolt");
    let form = crate::optional_trigger_public_v1::checked_form(&state, bolt).unwrap();
    assert_eq!(form.types, [CardType::Instant]);
}

/// `SourceTypesV1` has seven fixed bits and no Kindred bit: a Kindred
/// instant's spell types are the same bits as any other instant's.
#[test]
fn source_type_bits_leave_kindred_out() {
    for name in ["Tarfire", "Kozilek's Command", "Faerie Trickery"] {
        let types = crate::source_type_hexproof_v1::spell_types(
            &CARD_DEFS[id(name) as usize],
            CastMethodV4::Normal,
        )
        .unwrap();
        assert!(types.valid(), "{name}");
        assert_eq!(types.card_types(), [CardType::Instant], "{name}");
        assert!(!types.contains(CardType::Kindred), "{name}");
        assert_eq!(
            types,
            crate::source_type_hexproof_v1::spell_types(
                &CARD_DEFS[id("Lightning Bolt") as usize],
                CastMethodV4::Normal,
            )
            .unwrap(),
            "{name}"
        );
    }
}

/// The frozen rules vector refuses a Kindred card rather than describing
/// it as an instant.
#[test]
#[should_panic(expected = "the Kindred card type requires its precise successor")]
fn rules_vector_v1_refuses_a_kindred_card() {
    crate::rules_vector_v1::sources::card_rules(id("Tarfire"));
}
