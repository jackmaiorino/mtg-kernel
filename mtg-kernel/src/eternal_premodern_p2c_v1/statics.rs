//! Static abilities and engine hooks of the Premodern wave P2c cards:
//! Akroma's printed protection and Triskelion's "enters with" counters.

use super::{AKROMA, TRISKELION};
use crate::card_def::{mana_color_mask, CARD_DEFS};
use crate::ids::ObjectId;
use crate::mana::ManaColor;
use crate::state::GameState;

fn name_of(state: &GameState, object: ObjectId) -> Option<&'static str> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS
        .get(usize::from(live.card_def))
        .map(|def| def.name)
}

/// Akroma's "protection from black and from red", as a color mask.
pub(crate) fn protection_mask(state: &GameState, host: ObjectId) -> u8 {
    if name_of(state, host) == Some(AKROMA)
        && crate::eternal_premodern_p1b_v1::active_named(state, host, AKROMA)
    {
        mana_color_mask(ManaColor::B) | mana_color_mask(ManaColor::R)
    } else {
        0
    }
}

/// Triskelion: "enters with three +1/+1 counters on it."
pub(crate) fn apply_entry_counters(
    state: &mut GameState,
    object: ObjectId,
    input: Option<&crate::doubling_season_v48::ReplacementInputV48>,
) {
    if name_of(state, object) != Some(TRISKELION) {
        return;
    }
    let live = state.objects.get(object);
    if live.zone != crate::state::Zone::Battlefield || live.v4.face_index != 0 {
        return;
    }
    let controller = live.controller;
    match crate::doubling_season_v48::entry_amount(state, input, controller, 3).and_then(|amount| {
        state
            .objects
            .get(object)
            .counters
            .plus1_plus1
            .checked_add(amount)
            .ok_or_else(|| "entry counter overflow".to_string())
    }) {
        Ok(total) => state.objects.get_mut(object).counters.plus1_plus1 = total,
        Err(_) => {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                object,
            ))
        }
    }
}
