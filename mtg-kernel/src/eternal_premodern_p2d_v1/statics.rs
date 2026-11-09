//! Static abilities and engine hooks of the wave P2d cards: printed
//! protection, shadow, protection from instant and sorcery spells, Glowrider's
//! tax, Shared Triumph's chosen type and anthem, and Veteran Brawlers'
//! restrictions.

use super::{
    p2d_state, read_state, BELOVED_CHAPLAIN, GLOWRIDER, SHARED_TRIUMPH, SOLTARI_MONK,
    SOLTARI_PRIEST, VETERAN_BRAWLERS,
};
use crate::card_def::{CardType, Subtype, CARD_DEFS};
use crate::eternal_legacy_e9a_v1::{
    active_named, active_permanents, battlefield, definition_name, live_link, TurnV1,
};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::state::{GameState, Zone};

// ---- Protection and shadow ---------------------------------------------------------------

/// Printed protection from a color as a color mask: Soltari Monk ("Protection
/// from black") and Soltari Priest ("Protection from red"). Wave E6b's checks
/// apply it to targeting, blocking, damage and attachments.
pub(crate) fn protection_mask(state: &GameState, host: ObjectId) -> u8 {
    let color = match definition_name(state, host) {
        Some(SOLTARI_MONK) => ManaColor::B,
        Some(SOLTARI_PRIEST) => ManaColor::R,
        _ => return 0,
    };
    let name = definition_name(state, host).unwrap_or_default();
    if active_named(state, host, name) {
        crate::card_def::mana_color_mask(color)
    } else {
        0
    }
}

/// Beloved Chaplain: "Protection from creatures." Wave E7's protection from
/// creatures (targeting, damage and blocking) asks this.
pub(crate) fn printed_protection_from_creatures(state: &GameState, object: ObjectId) -> bool {
    active_named(state, object, BELOVED_CHAPLAIN)
}

/// Shadow (702.28): Soltari Monk and Soltari Priest. Wave E9b's blocking rule
/// asks this with Dauthi Voidwalker's shadow.
pub(crate) fn has_shadow(state: &GameState, object: ObjectId) -> bool {
    active_named(state, object, SOLTARI_MONK) || active_named(state, object, SOLTARI_PRIEST)
}

/// Whether `object` has protection from instant and sorcery spells until
/// end of turn (Devoted Caretaker).
fn has_spell_protection(state: &GameState, object: ObjectId) -> bool {
    let Some(p2d) = read_state(state) else {
        return false;
    };
    if p2d.spell_protection.is_empty() {
        return false;
    }
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    let now = TurnV1::now(state);
    live.zone == Zone::Battlefield
        && p2d.spell_protection.iter().any(|(link, turn)| {
            link.object == object
                && link.zone_change_count == live.zone_change_count
                && *turn == now
        })
}

/// Whether `source` is an instant or sorcery spell: on the stack, or a
/// nonland card about to be cast (its targets are checked before it becomes a
/// spell).
fn source_is_instant_or_sorcery_spell(state: &GameState, source: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(source) else {
        return false;
    };
    let Some(def) = CARD_DEFS.get(usize::from(live.card_def)) else {
        return false;
    };
    if !(def.has_type(CardType::Instant) || def.has_type(CardType::Sorcery)) {
        return false;
    }
    match live.zone {
        // A card on the stack is a spell, also while it resolves (its item has
        // left the stack by then).
        Zone::Stack | Zone::Hand | Zone::Graveyard | Zone::Exile | Zone::Library => true,
        _ => false,
    }
}

/// Devoted Caretaker's protection: whether `object` can't be targeted, nor
/// dealt damage, by `source`, an instant or sorcery spell.
pub(crate) fn protected_from_source(state: &GameState, object: ObjectId, source: ObjectId) -> bool {
    has_spell_protection(state, object) && source_is_instant_or_sorcery_spell(state, source)
}

// ---- Costs and statics -----------------------------------------------------------------

/// Glowrider: "Noncreature spells cost {1} more to cast." The generic
/// increase for a spell with `types`, from every Glowrider.
pub(crate) fn spell_tax(state: &GameState, types: &[CardType]) -> u32 {
    if types.contains(&CardType::Creature) {
        return 0;
    }
    u32::try_from(active_permanents(state, GLOWRIDER, None).count()).unwrap_or(u32::MAX)
}

/// Shared Triumph: "Creatures of the chosen type get +1/+1." The boost of
/// `object` from every Shared Triumph.
pub(crate) fn controlled_boost(state: &GameState, object: ObjectId) -> (i128, i128) {
    let Some(p2d) = read_state(state) else {
        return (0, 0);
    };
    if p2d.triumph_types.is_empty()
        || !state
            .objects
            .try_get(object)
            .is_some_and(|live| live.zone == Zone::Battlefield)
        || !crate::engine::object_has_type(state, object, CardType::Creature)
    {
        return (0, 0);
    }
    let count = p2d
        .triumph_types
        .iter()
        .filter(|(link, subtype)| {
            state.objects.try_get(link.object).is_some_and(|live| {
                live.zone_change_count == link.zone_change_count
                    && active_named(state, link.object, SHARED_TRIUMPH)
            }) && crate::engine::has_effective_subtype(state, object, *subtype)
        })
        .count() as i128;
    (count, count)
}

fn controls_untapped_land(state: &GameState, player: PlayerId) -> bool {
    battlefield(state).any(|object| {
        let live = state.objects.get(object);
        live.controller == player
            && !live.tapped
            && crate::engine::object_has_type(state, object, CardType::Land)
    })
}

/// Veteran Brawlers: "This creature can't attack if defending player
/// controls an untapped land."
pub(crate) fn cant_attack(state: &GameState, object: ObjectId) -> bool {
    active_named(state, object, VETERAN_BRAWLERS)
        && controls_untapped_land(state, state.objects.get(object).controller.opponent())
}

/// Veteran Brawlers: "This creature can't block if you control an untapped
/// land."
pub(crate) fn blocker_allowed(state: &GameState, blocker: ObjectId) -> bool {
    !(active_named(state, blocker, VETERAN_BRAWLERS)
        && controls_untapped_land(state, state.objects.get(blocker).controller))
}

// ---- Shared Triumph's as-enters choice ---------------------------------------------------

/// The creature types a Shared Triumph entering under `player` may name:
/// those of the creature cards `player` owns (not tokens or copies), as for
/// Cavern of Souls, so the number of options reveals nothing hidden in the
/// opponent's hand or library. Naming any other type is equivalent, so these
/// are the distinct choices.
pub fn triumph_type_options(state: &GameState, player: PlayerId) -> Vec<Subtype> {
    crate::eternal_cards_v1::e3::cavern_type_options(state, player)
}

/// The Shared Triumph that just entered and has not chosen a type.
fn triumph_needing_type(state: &GameState) -> Option<ObjectId> {
    battlefield(state).find(|&object| {
        state.objects.get(object).v4.face_index == 0
            && definition_name(state, object) == Some(SHARED_TRIUMPH)
            && !read_state(state).is_some_and(|p2d| {
                p2d.triumph_types
                    .iter()
                    .any(|(link, _)| *link == live_link(state, object))
            })
    })
}

/// "As this enchantment enters, choose a creature type": the pending choice,
/// one option per creature type.
pub(crate) fn entry_choice_decision(state: &GameState) -> Option<crate::engine::Decision> {
    let triumph = triumph_needing_type(state)?;
    Some(crate::engine::Decision::ChooseEffectOption {
        player: state.objects.get(triumph).controller,
        source: triumph,
        option_count: u16::try_from(
            triumph_type_options(state, state.objects.get(triumph).controller).len(),
        )
        .unwrap_or(u16::MAX),
    })
}

/// Answers the pending Shared Triumph choice. `Ok(false)` when none is
/// choosing.
pub(crate) fn answer_entry_choice(state: &mut GameState, option: u16) -> Result<bool, String> {
    let Some(triumph) = triumph_needing_type(state) else {
        return Ok(false);
    };
    let subtype = *triumph_type_options(state, state.objects.get(triumph).controller)
        .get(usize::from(option))
        .ok_or("creature type option is out of range")?;
    let link = live_link(state, triumph);
    let types = &mut p2d_state(state).triumph_types;
    types.retain(|(host, _)| host.object != triumph);
    types.push((link, subtype));
    Ok(true)
}
