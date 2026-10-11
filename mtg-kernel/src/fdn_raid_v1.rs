//! Raid entry replacement counters, applied before entry triggers and SBAs.
use crate::card_def::CARD_DEFS;
use crate::ids::ObjectId;
use crate::state::{GameState, Zone};

pub(crate) fn entry_counter_count_for(name: &str) -> Option<u8> {
    match name {
        "Goblin Boarders" => Some(1),
        _ => None,
    }
}

pub(crate) fn apply_entry_counters(state: &mut GameState, id: ObjectId) {
    let object = state.objects.get(id);
    let definition = &CARD_DEFS[object.card_def as usize];
    if object.zone != Zone::Battlefield || !definition.is_executable() {
        return;
    }
    let Some(count) = entry_counter_count_for(definition.name) else {
        return;
    };
    if state.player_attacked_this_turn_v1(object.controller)
        && crate::continuous_characteristics_v1::printed_abilities_active(state, id)
    {
        state.objects.get_mut(id).counters.plus1_plus1 += i32::from(count);
    }
}
