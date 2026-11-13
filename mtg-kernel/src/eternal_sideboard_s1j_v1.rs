//! Vintage sideboard card hooks of the eternal catalog's wave S1j
//! (`docs/design/eternal_formats_support_v1.md`): Archon of Absolution's
//! cost to attack, which completes Vintage Mono-White Initiative. Its flying
//! is a printed keyword, and its protection from white is in wave P1a's
//! printed protection table.
//!
//! "Creatures can't attack you or planeswalkers you control unless their
//! controller pays {1} for each of those creatures." The cost is a cost to
//! attack (508.1g). `engine::validate_declare_attackers` refuses a
//! declaration whose cost can't be paid, the requirement helpers name no
//! creature while attacks have a cost (508.1d), and the declaration pays it
//! once its attackers are tapped (508.1f, then 508.1i). The engine calls
//! these hooks only in `eternal-fixtures` builds.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::card_def::Keywords;
use crate::eternal_legacy_e9a_v1::{active_named, battlefield};
use crate::ids::ObjectId;
use crate::state::GameState;

pub(crate) const ARCHON_OF_ABSOLUTION: &str = "Archon of Absolution";

/// The generic mana each attacking creature costs the active player: {1}
/// for each Archon of Absolution the defending player controls whose
/// abilities apply. In a two-player game every attacking creature attacks
/// the defending player or a planeswalker they control. The active
/// player's own Archons cost them nothing, and a Phyrexian Metamorph
/// copying an Archon counts, since a copy takes the copied definition.
pub(crate) fn attack_cost_per_creature(state: &GameState) -> usize {
    let defender = state.active_player.opponent();
    battlefield(state)
        .filter(|&object| {
            state.objects.get(object).controller == defender
                && active_named(state, object, ARCHON_OF_ABSOLUTION)
        })
        .count()
}

/// Whether every attack the active player could declare has a cost. While
/// it does, no creature is required to attack: a player is never required
/// to pay a cost to attack (508.1d), and the card's 2019-10-04 ruling lets
/// a creature that attacks "if able" stay home.
pub(crate) fn attacks_have_a_cost(state: &GameState) -> bool {
    attack_cost_per_creature(state) > 0
}

/// The total cost to attack with `attackers` (508.1g), as generic mana, or
/// `None` past the largest amount one payment can name.
fn attack_cost(state: &GameState, attackers: &[ObjectId]) -> Option<u8> {
    let cost = attack_cost_per_creature(state).checked_mul(attackers.len())?;
    u8::try_from(cost).ok()
}

/// Whether the active player can pay the cost to attack with `attackers`.
/// The declaration taps its attackers without vigilance (508.1f) before
/// the cost is paid (508.1h and 508.1i), so an attacking mana creature
/// can't pay its own cost and a vigilant one can: the effect-mana check
/// runs on a copy of the state with those attackers tapped.
pub(crate) fn can_pay_attack_cost(state: &GameState, attackers: &[ObjectId]) -> bool {
    let Some(cost) = attack_cost(state, attackers) else {
        return false;
    };
    if cost == 0 {
        return true;
    }
    let mut tapped = state.clone();
    for &attacker in attackers {
        if !crate::engine::has_effective_keyword(state, attacker, Keywords::VIGILANCE) {
            tapped.objects.get_mut(attacker).tapped = true;
        }
    }
    crate::engine::can_pay_effect_mana(state.active_player, &[], cost, &tapped)
}

/// Pays the cost to attack with `attackers` after the declaration has
/// tapped them, with the effect-mana plan (the ordinary mana plan, then
/// Treasures). The declaration's check found that plan on the same state,
/// so this returns `false` only if that check was skipped.
pub(crate) fn pay_attack_cost(state: &mut GameState, attackers: &[ObjectId]) -> bool {
    let Some(cost) = attack_cost(state, attackers) else {
        return false;
    };
    cost == 0 || crate::engine::pay_effect_mana(state.active_player, &[], cost, state)
}
