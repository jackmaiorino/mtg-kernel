//! Static abilities and continuous effects of the wave E5a cards, read by
//! engine hooks: Grim Monolith's untap restriction, Council of Reeds' legend
//! rule exemption, Serra's Emissary's protection, Oko's Elks, The
//! Fantasticar's animation, Inventors' Fair's activation condition and
//! Urza's Workshop's metalcraft yield.

use super::{
    active_named, active_permanents, controls_three_artifacts, e5a_state, live_link, read_state,
    COUNCIL_OF_REEDS, GRIM_MONOLITH, INVENTORS_FAIR, SERRAS_EMISSARY,
};
use crate::card_def::{CardType, Subtype, CARD_DEFS};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{GameState, ObjectLinkV4, Target, Zone};

/// Grim Monolith: "This artifact doesn't untap during your untap step."
pub(crate) fn prevents_untap_step(state: &GameState, object: ObjectId) -> bool {
    active_named(state, object, GRIM_MONOLITH)
}

/// Council of Reeds: "The 'legend rule' doesn't apply to creatures you
/// control."
pub(crate) fn legend_rule_exempt(state: &GameState, object: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    live.zone == Zone::Battlefield
        && crate::engine::object_has_type(state, object, CardType::Creature)
        && active_permanents(state, COUNCIL_OF_REEDS, Some(live.controller))
            .next()
            .is_some()
}

/// Inventors' Fair: "Activate only if you control three or more
/// artifacts."
pub(crate) fn activation_allowed(state: &GameState, source: ObjectId, name: &str) -> bool {
    name != INVENTORS_FAIR || controls_three_artifacts(state, state.objects.get(source).controller)
}

/// Urza's Workshop: "{T}: Add {C}." and "Metalcraft -- {T}: Add {C} for
/// each Urza's land you control. Activate only if you control three or
/// more artifacts." Both add only colorless mana, so the engine offers one
/// ability adding the larger amount: one, or with metalcraft the number of
/// Urza's lands `controller` controls (at least one, the Workshop itself).
pub(crate) fn urzas_workshop_yield(state: &GameState, controller: PlayerId) -> i32 {
    if !controls_three_artifacts(state, controller) {
        return 1;
    }
    let urzas = super::controlled(state, controller)
        .filter(|&object| {
            crate::engine::object_has_type(state, object, CardType::Land)
                && crate::engine::has_effective_subtype(state, object, Subtype::Urzas)
        })
        .count();
    i32::try_from(urzas.max(1)).unwrap_or(i32::MAX)
}

// ---- Serra's Emissary -----------------------------------------------------------

/// The card types Serra's Emissary may name, by index (205.2a's card types
/// that cards in this catalog have).
pub(crate) const PROTECTION_TYPES: [CardType; 7] = [
    CardType::Artifact,
    CardType::Creature,
    CardType::Enchantment,
    CardType::Instant,
    CardType::Land,
    CardType::Planeswalker,
    CardType::Sorcery,
];

/// The Serra's Emissary that just entered and has not chosen a card type.
fn emissary_needing_type(state: &GameState) -> Option<ObjectId> {
    let id = crate::card_def::card_id_by_name(SERRAS_EMISSARY)?;
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
        .find(|&object| {
            let live = state.objects.get(object);
            live.card_def == id
                && live.v4.face_index == 0
                && !read_state(state).is_some_and(|e5a| {
                    e5a.emissary_types
                        .iter()
                        .any(|(host, _)| *host == live_link(state, object))
                })
        })
}

/// "As this creature enters, choose a card type": the pending choice, one
/// option per card type.
pub(crate) fn entry_choice_decision(state: &GameState) -> Option<crate::engine::Decision> {
    let emissary = emissary_needing_type(state)?;
    Some(crate::engine::Decision::ChooseEffectOption {
        player: state.objects.get(emissary).controller,
        source: emissary,
        option_count: PROTECTION_TYPES.len() as u16,
    })
}

/// Answers the pending Serra's Emissary card type choice. `Ok(false)` when
/// no Emissary is choosing.
pub(crate) fn answer_entry_choice(state: &mut GameState, option: u16) -> Result<bool, String> {
    let Some(emissary) = emissary_needing_type(state) else {
        return Ok(false);
    };
    let index = u8::try_from(option)
        .ok()
        .filter(|&index| usize::from(index) < PROTECTION_TYPES.len())
        .ok_or("card type option is out of range")?;
    let link = live_link(state, emissary);
    let types = &mut e5a_state(state).emissary_types;
    types.retain(|(host, _)| host.object != emissary);
    types.push((link, index));
    Ok(true)
}

/// The card types `player` and the creatures they control have protection
/// from.
fn protection_types(state: &GameState, player: PlayerId) -> Vec<CardType> {
    let Some(e5a) = read_state(state) else {
        return Vec::new();
    };
    e5a.emissary_types
        .iter()
        .filter(|(host, _)| {
            state.objects.try_get(host.object).is_some_and(|live| {
                live.zone_change_count == host.zone_change_count
                    && live.controller == player
                    && active_named(state, host.object, SERRAS_EMISSARY)
            })
        })
        .filter_map(|(_, index)| PROTECTION_TYPES.get(usize::from(*index)).copied())
        .collect()
}

/// Whether a source object has a card type: a permanent's current types,
/// or the printed types of a spell or a card elsewhere.
fn source_has_type(state: &GameState, source: ObjectId, card_type: CardType) -> bool {
    let Some(live) = state.objects.try_get(source) else {
        return false;
    };
    if live.zone == Zone::Battlefield {
        return crate::engine::object_has_type(state, source, card_type);
    }
    CARD_DEFS
        .get(live.card_def as usize)
        .is_some_and(|def| def.has_type(card_type))
}

fn protected_from(state: &GameState, player: PlayerId, source: ObjectId) -> bool {
    protection_types(state, player)
        .into_iter()
        .any(|card_type| source_has_type(state, source, card_type))
}

/// Serra's Emissary: "You ... have protection from the chosen card type."
pub(crate) fn player_protected_from(state: &GameState, player: PlayerId, source: ObjectId) -> bool {
    read_state(state).is_some_and(|e5a| !e5a.emissary_types.is_empty())
        && protected_from(state, player, source)
}

/// Serra's Emissary: "... and creatures you control have protection from
/// the chosen card type."
pub(crate) fn object_protected_from(state: &GameState, object: ObjectId, source: ObjectId) -> bool {
    if read_state(state).is_none_or(|e5a| e5a.emissary_types.is_empty()) {
        return false;
    }
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    live.zone == Zone::Battlefield
        && crate::engine::object_has_type(state, object, CardType::Creature)
        && protected_from(state, live.controller, source)
}

/// Damage protection prevents (702.16e).
pub(crate) fn damage_prevented(state: &GameState, source: ObjectId, target: Target) -> bool {
    match target {
        Target::Player(player) => player_protected_from(state, player, source),
        Target::Object(object) => object_protected_from(state, object, source),
        Target::StackItem(_) => false,
    }
}

/// Protection: a creature can't be blocked by creatures that have a chosen
/// card type (702.16f).
pub(crate) fn blocker_allowed(state: &GameState, attacker: ObjectId, blocker: ObjectId) -> bool {
    !object_protected_from(state, attacker, blocker)
}

// ---- Oko's Elks and The Fantasticar ------------------------------------------------

fn elk_timestamp(state: &GameState, object: ObjectId) -> Option<u64> {
    let live = state.objects.try_get(object)?;
    if live.zone != Zone::Battlefield {
        return None;
    }
    let link = ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    read_state(state)?
        .elks
        .iter()
        .find(|(host, _)| *host == link)
        .map(|(_, timestamp)| *timestamp)
}

fn animated_until_end_of_turn(state: &GameState, object: ObjectId) -> bool {
    let Some(e5a) = read_state(state) else {
        return false;
    };
    if e5a.animated.is_empty() {
        return false;
    }
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    live.zone == Zone::Battlefield
        && e5a.animated.iter().any(|(host, effect)| {
            host.object == object
                && host.zone_change_count == live.zone_change_count
                && effect.current(state)
        })
}

/// Card-type changes from this module. An Elk is a creature and nothing
/// else ("becomes a green Elk creature" replaces its card types); The
/// Fantasticar becomes an artifact creature until end of turn.
pub(crate) fn has_type_override(
    state: &GameState,
    object: ObjectId,
    card_type: CardType,
) -> Option<bool> {
    let e5a = read_state(state)?;
    if e5a.elks.is_empty() && e5a.animated.is_empty() {
        return None;
    }
    if elk_timestamp(state, object).is_some() {
        return Some(card_type == CardType::Creature);
    }
    (card_type == CardType::Creature && animated_until_end_of_turn(state, object)).then_some(true)
}

/// The Elk's base power and toughness 3/3, a layer 7b setting with the
/// effect's timestamp.
pub(crate) fn base_power_toughness(state: &GameState, object: ObjectId) -> Option<(i16, i16, u64)> {
    elk_timestamp(state, object).map(|timestamp| (3, 3, timestamp))
}

/// "Loses all abilities": the timestamp at which an Elk lost its printed
/// abilities.
pub(crate) fn removal_timestamp(state: &GameState, object: ObjectId) -> Option<u64> {
    if read_state(state).is_none_or(|e5a| e5a.elks.is_empty()) {
        return None;
    }
    elk_timestamp(state, object)
}
