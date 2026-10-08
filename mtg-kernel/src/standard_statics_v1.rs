//! Static abilities of the MageZero Standard family G creatures. Every query
//! here is recomputed from the live board, like the existing self and lord
//! boosts in `engine`, and only exists in `standard-magezero-fixtures`
//! builds.

use crate::card_def::{CardType, Keywords, Subtype, CARD_DEFS};
use crate::ids::ObjectId;
use crate::state::{GameState, Zone};

fn battlefield_definition_name(state: &GameState, id: ObjectId) -> Option<&'static str> {
    let object = state.objects.try_get(id)?;
    let definition = CARD_DEFS.get(object.card_def as usize)?;
    (object.zone == Zone::Battlefield
        && definition.is_executable()
        && crate::continuous_characteristics_v1::printed_abilities_active(state, id))
    .then_some(definition.name)
}

/// Keywords a permanent grants itself while a printed condition holds.
pub(crate) fn conditional_self_keywords(state: &GameState, id: ObjectId) -> Keywords {
    let Some(name) = battlefield_definition_name(state, id) else {
        return Keywords::NONE;
    };
    let controller = state.objects.get(id).controller;
    match name {
        // "This creature has first strike during your turn."
        "Razorkin Needlehead" if state.active_player == controller => Keywords::FIRST_STRIKE,
        _ => Keywords::NONE,
    }
}

/// Counters a permanent enters with beyond `CardDef::enters_with_plus_one_counters`,
/// applied in the same entry step before triggers and state-based actions.
pub(crate) fn apply_conditional_entry_counters(state: &mut GameState, id: ObjectId) {
    let Some(name) = battlefield_definition_name(state, id) else {
        return;
    };
    let controller = state.objects.get(id).controller;
    match name {
        // "This creature enters with a +1/+1 counter on it if you control a
        // permanent with mana value 4 or greater."
        "Ascendant Packleader" => {
            let controls_big_permanent =
                state.players[controller.index()]
                    .battlefield
                    .iter()
                    .any(|&other| {
                        other != id
                            && CARD_DEFS[state.objects.get(other).card_def as usize].mana_value >= 4
                    });
            if controls_big_permanent {
                state.objects.get_mut(id).counters.plus1_plus1 += 1;
            }
        }
        // "This creature enters with an oil counter on it."
        "Evolving Adaptive" => state.objects.get_mut(id).counters.oil += 1,
        _ => {}
    }
}

/// Power/toughness a permanent's own static gives itself from its counters.
pub(crate) fn self_counter_boost(state: &GameState, id: ObjectId) -> (i32, i32) {
    match battlefield_definition_name(state, id) {
        // "This creature gets +1/+1 for each oil counter on it."
        Some("Evolving Adaptive") => {
            let oil = i32::from(state.objects.get(id).counters.oil);
            (oil, oil)
        }
        _ => (0, 0),
    }
}

/// Definitions whose leave-the-battlefield abilities read the counters the
/// permanent had (`GameState::counter_lki_v1`).
pub(crate) fn reads_counter_lki(card_def: u16) -> bool {
    matches!(
        CARD_DEFS.get(card_def as usize).map(|def| def.name),
        Some("Quirion Beastcaller" | "Unstoppable Slasher")
    )
}

/// Generic ward costs other permanents grant `id`, one entry per granting
/// permanent: "Each other Human you control ... has ward {1}" (Coppercoat
/// Vanguard).
pub(crate) fn granted_ward_generics(state: &GameState, id: ObjectId) -> Vec<u8> {
    let Some(object) = state.objects.try_get(id) else {
        return Vec::new();
    };
    if object.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, id, CardType::Creature)
        || !crate::engine::has_effective_subtype(state, id, Subtype::Human)
    {
        return Vec::new();
    }
    state.players[object.controller.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&source| {
            source != id
                && battlefield_definition_name(state, source) == Some("Coppercoat Vanguard")
        })
        .map(|_| 1)
        .collect()
}

/// Whether a ward trigger costing `generic` on a permanent of `card_def` can
/// come from a grant above. The grant may have ended by the time the trigger
/// is validated on the stack, so this checks only the printed shape.
pub(crate) fn may_have_granted_ward(card_def: u16, generic: u8) -> bool {
    generic == 1
        && CARD_DEFS.get(card_def as usize).is_some_and(|def| {
            def.has_type(CardType::Creature) && def.subtypes.contains(&Subtype::Human)
        })
}
