//! Static abilities and engine hooks of the Premodern wave P1b cards:
//! Opalescence's type and base power/toughness change, fade, treasure and
//! depletion counters, costs paid as an ability is put on the stack
//! (Attunement's return, Parallax Wave's fade counter), the Decrees'
//! cycling triggers, Sterling Grove's granted shroud, Solitary
//! Confinement's player shroud, damage prevention and skipped draw step,
//! Tsabo's Web's untap restriction, Aura of Silence's spell tax and
//! Sickening Dreams' "discard X cards".

use super::{
    active_named, definition, definition_name, link_live_in, live_link, p1b, p1b_state,
    queue_trigger, read_state, P1bOpV1, ATTUNEMENT, AURA_OF_SILENCE, DECREE_OF_JUSTICE,
    DECREE_OF_SILENCE, LEGACYS_ALLURE, OPALESCENCE, PARALLAX_WAVE, SICKENING_DREAMS,
    SOLITARY_CONFINEMENT, STERLING_GROVE, TSABOS_WEB,
};
use crate::card_def::{CardDef, CardType, Keywords, Subtype, TargetSpec};
use crate::event::{self, DamageProposed, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Target, Zone};

/// The definition ids of the cards whose statics the engine asks about on
/// hot paths, so those checks compare ids before names.
struct StaticIds {
    sterling_grove: Option<u16>,
    solitary_confinement: Option<u16>,
    tsabos_web: Option<u16>,
    aura_of_silence: Option<u16>,
}

fn static_ids() -> &'static StaticIds {
    static IDS: std::sync::OnceLock<StaticIds> = std::sync::OnceLock::new();
    IDS.get_or_init(|| StaticIds {
        sterling_grove: crate::card_def::card_id_by_name(STERLING_GROVE),
        solitary_confinement: crate::card_def::card_id_by_name(SOLITARY_CONFINEMENT),
        tsabos_web: crate::card_def::card_id_by_name(TSABOS_WEB),
        aura_of_silence: crate::card_def::card_id_by_name(AURA_OF_SILENCE),
    })
}

/// The battlefield permanents with definition `id`, named `name`, whose
/// printed abilities apply, controlled by `controller` if given. Compares
/// definition ids first.
fn active_with_id<'a>(
    state: &'a GameState,
    id: Option<u16>,
    name: &'a str,
    controller: Option<PlayerId>,
) -> impl Iterator<Item = ObjectId> + 'a {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .filter(move |&player| controller.is_none_or(|wanted| wanted == player))
        .flat_map(move |player| state.players[player.index()].battlefield.iter().copied())
        .filter(move |&object| {
            id.is_some_and(|id| state.objects.get(object).card_def == id)
                && active_named(state, object, name)
                && controller.is_none_or(|player| state.objects.get(object).controller == player)
        })
}

// ---- Opalescence --------------------------------------------------------------------

/// The latest timestamp of an Opalescence other than `object` whose ability
/// applies, if `object` is a non-Aura enchantment permanent it animates.
/// Opalescence: "Each other non-Aura enchantment is a creature in addition
/// to its other types and has base power and base toughness each equal to
/// its mana value."
fn opalescence_timestamp(state: &GameState, object: ObjectId) -> Option<u64> {
    let p1b = read_state(state)?;
    if p1b.opalescences.is_empty() {
        return None;
    }
    let live = state.objects.try_get(object)?;
    if live.zone != Zone::Battlefield {
        return None;
    }
    let def = definition(state, object)?;
    let face_types = def.types_for_face(live.v4.face_index);
    if !face_types.contains(&CardType::Enchantment)
        || def
            .subtypes_for_face(live.v4.face_index)
            .contains(&Subtype::Aura)
    {
        return None;
    }
    p1b.opalescences
        .iter()
        .filter(|(host, _)| {
            host.object != object
                && link_live_in(state, *host, Zone::Battlefield)
                && active_named(state, host.object, OPALESCENCE)
        })
        .map(|(_, timestamp)| *timestamp)
        .max()
}

/// Card-type changes from this module: Opalescence makes the other non-Aura
/// enchantments creatures in addition to their other types.
pub(crate) fn has_type_override(
    state: &GameState,
    object: ObjectId,
    card_type: CardType,
) -> Option<bool> {
    (card_type == CardType::Creature && opalescence_timestamp(state, object).is_some())
        .then_some(true)
}

/// Opalescence's base power and toughness, a layer 7b setting with its
/// timestamp: each equal to the enchantment's mana value.
pub(crate) fn base_power_toughness(state: &GameState, object: ObjectId) -> Option<(i16, i16, u64)> {
    let timestamp = opalescence_timestamp(state, object)?;
    let value = i16::try_from(definition(state, object)?.mana_value).unwrap_or(i16::MAX);
    Some((value, value, timestamp))
}

// ---- Counters -----------------------------------------------------------------------

fn counters_on<T: Copy + Default>(entries: &[(ObjectLinkV4, T)], link: ObjectLinkV4) -> T {
    entries
        .iter()
        .find(|(host, _)| *host == link)
        .map_or_else(T::default, |(_, count)| *count)
}

/// The fade counters on `object`'s battlefield incarnation.
pub fn fade_counters(state: &GameState, object: ObjectId) -> u8 {
    if !link_live_in(state, live_link(state, object), Zone::Battlefield) {
        return 0;
    }
    read_state(state).map_or(0, |p1b| {
        counters_on(&p1b.fade_counters, live_link(state, object))
    })
}

/// The treasure counters on `object`'s battlefield incarnation.
pub fn treasure_counters(state: &GameState, object: ObjectId) -> u16 {
    if state
        .objects
        .try_get(object)
        .is_none_or(|live| live.zone != Zone::Battlefield)
    {
        return 0;
    }
    read_state(state).map_or(0, |p1b| {
        counters_on(&p1b.treasure_counters, live_link(state, object))
    })
}

/// The depletion counters on `object`'s battlefield incarnation.
pub fn depletion_counters(state: &GameState, object: ObjectId) -> u8 {
    if state
        .objects
        .try_get(object)
        .is_none_or(|live| live.zone != Zone::Battlefield)
    {
        return 0;
    }
    read_state(state).map_or(0, |p1b| {
        counters_on(&p1b.depletion_counters, live_link(state, object))
    })
}

/// The treasure counters a Legacy's Allure incarnation has, or had as it was
/// sacrificed to activate its ability.
pub(crate) fn allure_counters_for_link(state: &GameState, allure: ObjectLinkV4) -> u16 {
    let Some(p1b) = read_state(state) else {
        return 0;
    };
    if link_live_in(state, allure, Zone::Battlefield) {
        return counters_on(&p1b.treasure_counters, allure);
    }
    counters_on(&p1b.sacrificed_allures, allure)
}

/// "Enters with" counters: Parallax Wave's five fade counters (fading 5).
pub(crate) fn apply_entry_counters(state: &mut GameState, object: ObjectId) {
    if definition_name(state, object) != Some(PARALLAX_WAVE) {
        return;
    }
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield || live.v4.face_index != 0 {
        return;
    }
    let link = live_link(state, object);
    let counters = &mut p1b_state(state).fade_counters;
    counters.retain(|(host, _)| host.object != object);
    counters.push((link, super::FADE_COUNTERS));
}

// ---- Activations --------------------------------------------------------------------

/// Activation restrictions: Parallax Wave's ability needs a fade counter to
/// remove.
pub(crate) fn activation_allowed(
    state: &GameState,
    source: ObjectId,
    name: &str,
    ability_index: usize,
) -> bool {
    match (name, ability_index) {
        (PARALLAX_WAVE, 0) => fade_counters(state, source) > 0,
        _ => true,
    }
}

/// Costs paid as an ability is put on the stack, and the triggers its
/// payment sets off: Attunement returns to its owner's hand; Parallax Wave
/// loses a fade counter; a sacrificed Legacy's Allure keeps its counters for
/// its ability; cycling a Decree triggers it ("When you cycle this card").
pub(crate) fn activation_paid(state: &mut GameState, source: ObjectId, ability_index: u8) {
    let Some(name) = definition_name(state, source) else {
        return;
    };
    let live = state.objects.get(source);
    match (name, ability_index) {
        (ATTUNEMENT, 0) if live.zone == Zone::Battlefield => {
            event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Hand));
        }
        (PARALLAX_WAVE, 0) if live.zone == Zone::Battlefield => {
            let link = live_link(state, source);
            if let Some(entry) = p1b_state(state)
                .fade_counters
                .iter_mut()
                .find(|(host, _)| *host == link)
            {
                entry.1 = entry.1.saturating_sub(1);
            }
        }
        (LEGACYS_ALLURE, 0) => {
            // Sacrificed as the cost: its last battlefield incarnation.
            let departed = ObjectLinkV4 {
                object: source,
                zone_change_count: live.zone_change_count.saturating_sub(1),
            };
            let count =
                read_state(state).map_or(0, |p1b| counters_on(&p1b.treasure_counters, departed));
            let sacrificed = &mut p1b_state(state).sacrificed_allures;
            sacrificed.retain(|(host, _)| *host != departed);
            sacrificed.push((departed, count));
            // Keep only the latest few: an ability countered on the stack
            // leaves its record behind.
            if sacrificed.len() > 8 {
                sacrificed.remove(0);
            }
        }
        (DECREE_OF_JUSTICE, 0) if live.zone == Zone::Graveyard => {
            let owner = live.owner;
            let contract = AbilitySourceContractV4::capture(state, source);
            queue_trigger(
                state,
                contract,
                owner,
                p1b(P1bOpV1::DecreeSoldiersOfferV1),
                None,
            );
        }
        (DECREE_OF_SILENCE, 0) if live.zone == Zone::Graveyard => {
            let owner = live.owner;
            let contract = AbilitySourceContractV4::capture(state, source);
            queue_trigger(
                state,
                contract,
                owner,
                super::decree_of_silence_cycled(),
                Some(TargetSpec::AnySpellOnStack),
            );
        }
        _ => {}
    }
}

// ---- Keywords, shroud and damage ------------------------------------------------------

/// Sterling Grove: "Other enchantments you control have shroud." The
/// keywords this module grants `object`.
pub(crate) fn granted_keywords(state: &GameState, object: ObjectId) -> Keywords {
    if read_state(state).is_none() {
        return Keywords::NONE;
    }
    let Some(live) = state.objects.try_get(object) else {
        return Keywords::NONE;
    };
    if live.zone != Zone::Battlefield {
        return Keywords::NONE;
    }
    let controller = live.controller;
    let granted = active_with_id(
        state,
        static_ids().sterling_grove,
        STERLING_GROVE,
        Some(controller),
    )
    .any(|grove| grove != object);
    if granted && crate::engine::object_has_type(state, object, CardType::Enchantment) {
        Keywords::SHROUD
    } else {
        Keywords::NONE
    }
}

/// Solitary Confinement: "You have shroud." Whether `player` has shroud.
pub(crate) fn player_has_shroud(state: &GameState, player: PlayerId) -> bool {
    read_state(state).is_some()
        && active_with_id(
            state,
            static_ids().solitary_confinement,
            SOLITARY_CONFINEMENT,
            Some(player),
        )
        .next()
        .is_some()
}

/// Solitary Confinement: "Prevent all damage that would be dealt to you."
pub(crate) fn prevents_damage(state: &GameState, damage: &DamageProposed) -> bool {
    match damage.target {
        Target::Player(player) => player_has_shroud(state, player),
        _ => false,
    }
}

/// Solitary Confinement: "Skip your draw step."
pub(crate) fn skips_draw_step(state: &GameState, player: PlayerId) -> bool {
    player_has_shroud(state, player)
}

/// Tsabo's Web: "Each land with an activated ability that isn't a mana
/// ability doesn't untap during its controller's untap step."
pub(crate) fn prevents_untap_step(state: &GameState, object: ObjectId) -> bool {
    if read_state(state).is_none()
        || active_with_id(state, static_ids().tsabos_web, TSABOS_WEB, None)
            .next()
            .is_none()
    {
        return false;
    }
    crate::engine::object_has_type(state, object, CardType::Land)
        && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
        && definition(state, object).is_some_and(|def| !def.activated_abilities.is_empty())
}

// ---- Spell costs --------------------------------------------------------------------

/// Aura of Silence: "Artifact and enchantment spells your opponents cast
/// cost {2} more to cast." The generic increase for `caster`'s spell with
/// `types`.
pub(crate) fn spell_tax(state: &GameState, types: &[CardType], caster: PlayerId) -> u32 {
    if !(types.contains(&CardType::Artifact) || types.contains(&CardType::Enchantment)) {
        return 0;
    }
    let auras = active_with_id(
        state,
        static_ids().aura_of_silence,
        AURA_OF_SILENCE,
        Some(caster.opponent()),
    )
    .count();
    u32::try_from(auras).unwrap_or(u32::MAX).saturating_mul(2)
}

/// Sickening Dreams: "As an additional cost to cast this spell, discard X
/// cards." Its X is announced like a mana X.
pub(crate) fn announces_discard_x(def: &CardDef) -> bool {
    def.name == SICKENING_DREAMS
}

/// The most X Sickening Dreams may announce: the cards in its caster's hand.
pub(crate) fn discard_x_cap(
    def: &CardDef,
    spell: ObjectId,
    player: PlayerId,
    state: &GameState,
) -> Option<u8> {
    if !announces_discard_x(def) {
        return None;
    }
    let cards = state.players[player.index()]
        .hand
        .iter()
        .filter(|&&card| card != spell)
        .count();
    Some(u8::try_from(cards).unwrap_or(u8::MAX))
}

/// Sickening Dreams' additional discard: X cards, none for X = 0. `None`
/// for every other card.
pub(crate) fn additional_discard_count(def: &CardDef, x: Option<u16>) -> Option<Option<u8>> {
    announces_discard_x(def).then(|| {
        x.filter(|&x| x > 0)
            .map(|x| u8::try_from(x).unwrap_or(u8::MAX))
    })
}
