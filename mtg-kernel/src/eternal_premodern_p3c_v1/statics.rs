//! Static abilities and engine hooks of the Premodern wave P3c cards.

use super::{DOMINATE, IHSANS_SHADE, IRIDESCENT_ANGEL, MULTANI};
use crate::card_def::{mana_color_mask, CardDef, CARD_DEFS};
use crate::eternal_legacy_e9a_v1::active_named;
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::state::GameState;

/// Every color (W, U, B, R and G), as a mask.
const ALL_COLORS: u8 = 0x1f;

fn name_of(state: &GameState, object: ObjectId) -> Option<&'static str> {
    let live = state.objects.try_get(object)?;
    CARD_DEFS
        .get(usize::from(live.card_def))
        .map(|def| def.name)
}

/// The printed protection of `name`, as a color mask: Ihsan's Shade
/// ("Protection from white") and Iridescent Angel ("protection from all
/// colors", which is protection from each of the five).
fn printed_mask(name: &str) -> u8 {
    match name {
        IHSANS_SHADE => mana_color_mask(ManaColor::W),
        IRIDESCENT_ANGEL => ALL_COLORS,
        _ => 0,
    }
}

/// Printed protection from colors, as a color mask. Wave E6b's checks apply
/// it to targeting, blocking, damage and attachments. A Volrath's
/// Shapeshifter with one of these cards' full text has it too, as for wave
/// P2c's Akroma.
pub(crate) fn protection_mask(state: &GameState, host: ObjectId) -> u8 {
    if let Some(text) = crate::full_text_v1::active_definition(state, host) {
        if !crate::continuous_characteristics_v1::printed_abilities_active(state, host) {
            return 0;
        }
        return printed_mask(text.name);
    }
    match name_of(state, host) {
        Some(name) if printed_mask(name) != 0 && active_named(state, host, name) => {
            printed_mask(name)
        }
        _ => 0,
    }
}

/// Multani, Maro-Sorcerer: "Multani's power and toughness are each equal to
/// the total number of cards in all players' hands." A characteristic-
/// defining ability works in every zone.
pub(crate) fn characteristic_defining_power(state: &GameState, id: ObjectId) -> Option<i32> {
    let live = state.objects.try_get(id)?;
    if live.v4.face_down_v1.is_some() || name_of(state, id)? != MULTANI {
        return None;
    }
    let cards: usize = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .map(|player| state.players[player.index()].hand.len())
        .sum();
    i32::try_from(cards).ok()
}

/// Dominate's "target creature with mana value X or less", as for Blue
/// Sun's Twilight (`standard_cards_v1::cast_target_allowed`).
pub(crate) fn x_bounded_creature_target(def: &CardDef) -> bool {
    def.cost.x_count != 0 && def.name == DOMINATE
}
