//! Static abilities and engine hooks of the wave P1a cards: printed
//! protection from a color, threshold, Goblin Warchief's haste and cost
//! reduction, Goblin Goon's combat restrictions, swampwalk, Terravore's
//! size, "can't be regenerated", Sulfuric Vortex's life gain prohibition,
//! Goblin Vandal's lack of combat damage, Goblin Sharpshooter's untap step,
//! activation restrictions, Gemstone Mine's mining counters, Wild Mongrel's
//! color, Vision Charm's land types, split cards on the stack and morph.

use super::{
    active_named, active_permanents, battlefield, definition, definition_name, live_link,
    p1a_state, read_state, TurnV1, BARBARIAN_RING, EXALTED_ANGEL, FIRE_ICE, GEMSTONE_MINE,
    GOBLIN_GOON, GOBLIN_PILEDRIVER, GOBLIN_SHARPSHOOTER, GOBLIN_WARCHIEF, KNIGHT_OF_STROMGALD,
    MYSTIC_ENFORCER, TERRAVORE, WEATHERED_WAYFARER,
};
use crate::card_def::{CardType, Keywords, Subtype, CARD_DEFS};
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::{Cost, ManaColor, Pip};
use crate::state::{CastMethodV4, GameState, ObjectLinkV4, StackItem, StackItemKind, Zone};

/// The basic land types with the color of the mana each one's intrinsic
/// ability adds (305.6), in WUBRG order.
pub(crate) const BASIC_LAND_TYPES: [(Subtype, ManaColor); 5] = [
    (Subtype::Plains, ManaColor::W),
    (Subtype::Island, ManaColor::U),
    (Subtype::Swamp, ManaColor::B),
    (Subtype::Mountain, ManaColor::R),
    (Subtype::Forest, ManaColor::G),
];

fn is_creature(state: &GameState, object: ObjectId) -> bool {
    crate::engine::object_has_type(state, object, CardType::Creature)
}

/// Threshold: seven or more cards in `player`'s graveyard.
fn threshold(state: &GameState, player: PlayerId) -> bool {
    crate::effect::controller_graveyard_card_count(state, player) >= 7
}

// ---- Protection ---------------------------------------------------------------------

/// Printed protection from a color: Goblin Piledriver ("Protection from
/// blue"), Knight of Stromgald ("Protection from white") and Mystic
/// Enforcer ("Protection from black"), as a color mask. Wave E6b's
/// Benevolent Blessing checks apply it to targeting, blocking, damage and
/// attachments.
pub(crate) fn protection_mask(state: &GameState, host: ObjectId) -> u8 {
    let color = match definition_name(state, host) {
        Some(GOBLIN_PILEDRIVER) => ManaColor::U,
        Some(KNIGHT_OF_STROMGALD) => ManaColor::W,
        Some(MYSTIC_ENFORCER) => ManaColor::B,
        _ => return 0,
    };
    let name = definition_name(state, host).unwrap_or_default();
    if active_named(state, host, name) {
        crate::card_def::mana_color_mask(color)
    } else {
        0
    }
}

// ---- Keywords and power -------------------------------------------------------------

/// The keywords this module grants `object`: Goblin Warchief ("Goblins you
/// control have haste.") and Mystic Enforcer's threshold flying.
pub(crate) fn granted_keywords(state: &GameState, object: ObjectId) -> Keywords {
    let Some(live) = state.objects.try_get(object) else {
        return Keywords::NONE;
    };
    if live.zone != Zone::Battlefield || !is_creature(state, object) {
        return Keywords::NONE;
    }
    let controller = live.controller;
    let mut keywords = Keywords::NONE;
    if crate::engine::has_effective_subtype(state, object, Subtype::Goblin)
        && active_permanents(state, GOBLIN_WARCHIEF, Some(controller))
            .next()
            .is_some()
    {
        keywords = keywords | Keywords::HASTE;
    }
    if active_named(state, object, MYSTIC_ENFORCER) && threshold(state, controller) {
        keywords = keywords | Keywords::FLYING;
    }
    keywords
}

/// Mystic Enforcer: "Threshold -- As long as seven or more cards are in your
/// graveyard, this creature gets +3/+3 and has flying."
pub(crate) fn controlled_boost(state: &GameState, object: ObjectId) -> (i128, i128) {
    let Some(live) = state.objects.try_get(object) else {
        return (0, 0);
    };
    if active_named(state, object, MYSTIC_ENFORCER) && threshold(state, live.controller) {
        (3, 3)
    } else {
        (0, 0)
    }
}

/// Terravore: "Terravore's power and toughness are each equal to the number
/// of land cards in all graveyards." A characteristic-defining ability works
/// in every zone.
pub(crate) fn characteristic_defining_power(state: &GameState, id: ObjectId) -> Option<i32> {
    let live = state.objects.try_get(id)?;
    if live.v4.face_down_v1.is_some() || definition_name(state, id)? != TERRAVORE {
        return None;
    }
    let lands = [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].graveyard.iter().copied())
        .filter(|&card| {
            let card_live = state.objects.get(card);
            !card_live.v4.is_token
                && card_live.spell_copy_origin.is_none()
                && definition(state, card).is_some_and(|def| def.has_type(CardType::Land))
        })
        .count();
    i32::try_from(lands).ok()
}

// ---- Combat -------------------------------------------------------------------------

/// The creatures `player` controls.
fn creatures_controlled(state: &GameState, player: PlayerId) -> usize {
    battlefield(state)
        .filter(|&object| {
            state.objects.get(object).controller == player && is_creature(state, object)
        })
        .count()
}

/// Goblin Goon: "This creature can't attack unless you control more
/// creatures than defending player."
pub(crate) fn cant_attack(state: &GameState, object: ObjectId) -> bool {
    if !active_named(state, object, GOBLIN_GOON) {
        return false;
    }
    let controller = state.objects.get(object).controller;
    creatures_controlled(state, controller) <= creatures_controlled(state, controller.opponent())
}

/// Blocking restrictions: Goblin Goon ("This creature can't block unless you
/// control more creatures than attacking player.") and Funeral Charm's
/// swampwalk (it can't be blocked as long as defending player controls a
/// Swamp).
pub(crate) fn blocker_allowed(state: &GameState, attacker: ObjectId, blocker: ObjectId) -> bool {
    let defender = state.objects.get(blocker).controller;
    if active_named(state, blocker, GOBLIN_GOON)
        && creatures_controlled(state, defender) <= creatures_controlled(state, defender.opponent())
    {
        return false;
    }
    !(has_swampwalk(state, attacker)
        && battlefield(state).any(|permanent| {
            state.objects.get(permanent).controller == defender
                && crate::engine::object_has_type(state, permanent, CardType::Land)
                && crate::engine::has_effective_subtype(state, permanent, Subtype::Swamp)
        }))
}

/// Whether `object` has swampwalk until end of turn.
pub fn has_swampwalk(state: &GameState, object: ObjectId) -> bool {
    let Some(p1a) = read_state(state) else {
        return false;
    };
    if p1a.swampwalk.is_empty() || state.objects.try_get(object).is_none() {
        return false;
    }
    let link = live_link(state, object);
    let now = TurnV1::now(state);
    p1a.swampwalk.contains(&(link, now))
}

/// Goblin Vandal: "this creature assigns no combat damage this turn."
pub(crate) fn assigns_no_combat_damage(state: &GameState, object: ObjectId) -> bool {
    let Some(p1a) = read_state(state) else {
        return false;
    };
    if p1a.no_combat_damage.is_empty() || state.objects.try_get(object).is_none() {
        return false;
    }
    let link = live_link(state, object);
    p1a.no_combat_damage.contains(&(link, TurnV1::now(state)))
}

/// Goblin Sharpshooter: "This creature doesn't untap during your untap
/// step."
pub(crate) fn prevents_untap_step(state: &GameState, object: ObjectId) -> bool {
    active_named(state, object, GOBLIN_SHARPSHOOTER)
}

// ---- Activating, casting and regeneration ------------------------------------------------

/// Activation restrictions: Barbarian Ring's threshold ("Activate only if
/// seven or more cards are in your graveyard.") and Weathered Wayfarer's
/// "Activate only if an opponent controls more lands than you."
pub(crate) fn activation_allowed(
    state: &GameState,
    source: ObjectId,
    name: &str,
    ability_index: usize,
) -> bool {
    let controller = state.objects.get(source).controller;
    match (name, ability_index) {
        (BARBARIAN_RING, 0) => threshold(state, controller),
        (WEATHERED_WAYFARER, 0) => {
            let lands = |player: PlayerId| {
                battlefield(state)
                    .filter(|&object| {
                        state.objects.get(object).controller == player
                            && crate::engine::object_has_type(state, object, CardType::Land)
                    })
                    .count()
            };
            lands(controller.opponent()) > lands(controller)
        }
        _ => true,
    }
}

/// Goblin Warchief: "Goblin spells you cast cost {1} less to cast." The
/// generic mana a spell of `player`'s with Goblin among `spell_subtypes`
/// costs less.
pub(crate) fn spell_cost_reduction(
    state: &GameState,
    spell_subtypes: &[u16],
    player: PlayerId,
) -> u32 {
    if !spell_subtypes.contains(&Subtype::Goblin.stable_id()) {
        return 0;
    }
    u32::try_from(active_permanents(state, GOBLIN_WARCHIEF, Some(player)).count())
        .unwrap_or(u32::MAX)
}

/// Incinerate and Smother: whether `creature` can't be regenerated now.
pub(crate) fn cant_be_regenerated(state: &GameState, creature: ObjectId) -> bool {
    let Some(p1a) = read_state(state) else {
        return false;
    };
    if p1a.no_regeneration.is_empty() || state.objects.try_get(creature).is_none() {
        return false;
    }
    let link = live_link(state, creature);
    let now = TurnV1::now(state);
    p1a.no_regeneration.contains(&(link, now))
}

/// Sulfuric Vortex: "If a player would gain life, that player gains no life
/// instead." Whether a Sulfuric Vortex's ability applies.
pub(crate) fn prevents_life_gain(state: &GameState) -> bool {
    cfg!(feature = "eternal-fixtures")
        && active_permanents(state, super::SULFURIC_VORTEX, None)
            .next()
            .is_some()
}

// ---- Gemstone Mine ---------------------------------------------------------------------

/// Gemstone Mine: "This land enters with three mining counters on it."
pub(crate) fn apply_entry_counters(state: &mut GameState, object: ObjectId) {
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield
        || live.v4.face_index != 0
        || definition_name(state, object) != Some(GEMSTONE_MINE)
    {
        return;
    }
    let link = live_link(state, object);
    let mining = &mut p1a_state(state).mining;
    mining.retain(|(land, _)| land.object != object);
    mining.push((link, 3));
}

/// The mining counters on `land`'s current incarnation.
pub fn mining_counters(state: &GameState, land: ObjectId) -> u8 {
    let Some(live) = state.objects.try_get(land) else {
        return 0;
    };
    let link = ObjectLinkV4 {
        object: land,
        zone_change_count: live.zone_change_count,
    };
    read_state(state).map_or(0, |p1a| {
        p1a.mining
            .iter()
            .find(|(counted, _)| *counted == link)
            .map_or(0, |(_, count)| *count)
    })
}

/// Gemstone Mine's mana ability costs "Remove a mining counter from this
/// land": without one it can't be activated.
pub(crate) fn mana_ability_blocked(state: &GameState, source: ObjectId) -> bool {
    definition_name(state, source) == Some(GEMSTONE_MINE) && mining_counters(state, source) == 0
}

/// After a Gemstone Mine was tapped for mana: remove a mining counter, then
/// "If there are no mining counters on this land, sacrifice it."
pub(crate) fn after_tapped_for_mana(state: &mut GameState, land: ObjectId) {
    if definition_name(state, land) != Some(GEMSTONE_MINE)
        || state.objects.get(land).zone != Zone::Battlefield
    {
        return;
    }
    let link = live_link(state, land);
    let mut remaining = 0;
    if let Some(entry) = p1a_state(state)
        .mining
        .iter_mut()
        .find(|(counted, _)| *counted == link)
    {
        entry.1 = entry.1.saturating_sub(1);
        remaining = entry.1;
    }
    if remaining == 0 {
        event::log_sacrifice(state, land);
        event::propose_and_commit(state, ProposedEvent::zone_change(land, Zone::Graveyard));
    }
}

/// The fuse counters on Powder Keg `keg`'s current incarnation.
pub fn fuse_counters(state: &GameState, keg: ObjectId) -> u16 {
    let Some(live) = state.objects.try_get(keg) else {
        return 0;
    };
    let link = ObjectLinkV4 {
        object: keg,
        zone_change_count: live.zone_change_count,
    };
    read_state(state).map_or(0, |p1a| {
        p1a.fuse
            .iter()
            .find(|(counted, _)| *counted == link)
            .map_or(0, |(_, count)| *count)
    })
}

// ---- Colors and land types --------------------------------------------------------------

/// The colors this module sets for `id`, as a mask: Wild Mongrel's chosen
/// color (the latest), and a split card's half on the stack (708.3: only
/// that half's characteristics).
pub(crate) fn color_override(state: &GameState, id: ObjectId) -> Option<u8> {
    if let Some(mask) = split_spell_colors(state, id) {
        return Some(mask);
    }
    let p1a = read_state(state)?;
    if p1a.colors.is_empty() {
        return None;
    }
    let live = state.objects.try_get(id)?;
    let now = TurnV1::now(state);
    p1a.colors
        .iter()
        .filter(|record| {
            record.object.object == id
                && record.object.zone_change_count == live.zone_change_count
                && record.turn == now
                && live.zone == Zone::Battlefield
        })
        .max_by_key(|record| record.timestamp)
        .map(|record| crate::card_def::mana_color_mask(record.color))
}

/// Vision Charm: the latest land type change on `object`, its timestamp and
/// the color of the mana ability the new basic land type gives. Wave E8a's
/// land-type change applies it with Harbinger of the Seas and the moons
/// (305.7).
pub(crate) fn land_type_setter(
    state: &GameState,
    object: ObjectId,
) -> Option<(u64, Subtype, ManaColor)> {
    let p1a = read_state(state)?;
    if p1a.land_types.is_empty() {
        return None;
    }
    let live = state.objects.try_get(object)?;
    if live.zone != Zone::Battlefield {
        return None;
    }
    let now = TurnV1::now(state);
    let record = p1a
        .land_types
        .iter()
        .filter(|record| {
            record.land.object == object
                && record.land.zone_change_count == live.zone_change_count
                && record.turn == now
        })
        .max_by_key(|record| record.timestamp)?;
    let color = BASIC_LAND_TYPES
        .iter()
        .find(|(subtype, _)| *subtype == record.to)
        .map(|(_, color)| *color)?;
    Some((record.timestamp, record.to, color))
}

// ---- Split cards ------------------------------------------------------------------------

/// Split cards (708): Fire // Ice. The second half is cast as the card's
/// Adventure form, and goes to the graveyard as it resolves.
pub(crate) fn split_card(name: &str) -> bool {
    name == FIRE_ICE
}

/// The split card spell `item`'s half: `true` for the second.
/// While the spell is being cast, the half its caster chose (601.2b);
/// `None` before the choice, when the card still has both halves'
/// characteristics.
fn second_half(state: &GameState, item: &StackItem) -> Option<bool> {
    if let Some(pending) = state
        .engine
        .pending_cast
        .as_ref()
        .filter(|pending| pending.spell == item.source)
    {
        return pending.mode_chosen.map(|mode| mode == 1);
    }
    Some(item.v4.cast_method == Some(CastMethodV4::Omen))
}

/// The mana value of a split card spell on the stack: only its half's.
pub(crate) fn split_spell_mana_value(state: &GameState, item: &StackItem) -> Option<u16> {
    let def = definition(state, item.source)?;
    if !split_card(def.name) || item.kind != StackItemKind::Spell {
        return None;
    }
    let cost = if second_half(state, item)? {
        def.adventure.as_ref()?.cost
    } else {
        def.cost
    };
    Some(u16::from(cost.generic).saturating_add(u16::try_from(cost.pips.len()).unwrap_or(0)))
}

/// The colors of a split card spell on the stack: only its half's.
fn split_spell_colors(state: &GameState, id: ObjectId) -> Option<u8> {
    let live = state.objects.try_get(id)?;
    if live.zone != Zone::Stack {
        return None;
    }
    let def = CARD_DEFS.get(usize::from(live.card_def))?;
    if !split_card(def.name) {
        return None;
    }
    let pending_half = state
        .engine
        .pending_cast
        .as_ref()
        .filter(|pending| pending.spell == id)
        .map(|pending| pending.mode_chosen.map(|mode| mode == 1));
    let second = match pending_half {
        Some(half) => half?,
        None => {
            let item = state
                .stack
                .iter()
                .find(|item| item.kind == StackItemKind::Spell && item.source == id)?;
            second_half(state, item)?
        }
    };
    let cost = if second {
        def.adventure.as_ref()?.cost
    } else {
        def.cost
    };
    Some(cost.pips.iter().fold(0, |mask, pip| match *pip {
        Pip::Colored(color) if color != ManaColor::C => {
            mask | crate::card_def::mana_color_mask(color)
        }
        _ => mask,
    }))
}

// ---- Morph ---------------------------------------------------------------------------

/// Morph (702.37): Exalted Angel. Cast face down through disguise's
/// face-down cast, without disguise's ward.
pub(crate) fn is_morph(name: &str) -> bool {
    name == EXALTED_ANGEL
}

/// Exalted Angel's morph cost, {2}{W}{W}.
const EXALTED_ANGEL_MORPH: Cost = Cost {
    pips: &[Pip::Colored(ManaColor::W), Pip::Colored(ManaColor::W)],
    generic: 2,
    x_count: 0,
};

/// The cost of turning the face-down morph permanent `id` face up (a
/// special action, 702.37e), or `None` when it is not one.
pub(crate) fn morph_face_up_cost(state: &GameState, id: ObjectId) -> Option<Cost> {
    let live = state.objects.try_get(id)?;
    if live.zone != Zone::Battlefield || !live.v4.face_down_v1.is_some_and(|face| face.disguised) {
        return None;
    }
    let def = CARD_DEFS.get(usize::from(live.card_def))?;
    (def.name == EXALTED_ANGEL).then_some(EXALTED_ANGEL_MORPH)
}
