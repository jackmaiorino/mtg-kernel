//! Static abilities and engine hooks of the wave P3b cards: Gloom's taxes,
//! Engineered Plague's creature type and -1/-1, Mogg Salvage's free cast
//! condition and Dystopia's age counters.

use super::{p3b_state, read_state, ENGINEERED_PLAGUE, GLOOM};
use crate::card_def::{CardDef, CardType, Subtype, CARD_DEFS};
use crate::eternal_legacy_e9a_v1::{active_named, battlefield, live_link};
use crate::eternal_premodern_p3a_v1::statics::{active_with_id, form_colors};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::state::{CastMethodV4, GameState, Zone};

fn gloom_id() -> Option<u16> {
    static ID: std::sync::OnceLock<Option<u16>> = std::sync::OnceLock::new();
    *ID.get_or_init(|| crate::card_def::card_id_by_name(GLOOM))
}

const fn white() -> u8 {
    crate::card_def::mana_color_mask(ManaColor::W)
}

/// {3} for each Gloom whose abilities apply.
fn gloom_count_tax(state: &GameState) -> u32 {
    let glooms = active_with_id(state, gloom_id(), GLOOM, None).count();
    u32::try_from(glooms).map_or(u32::MAX, |count| count.saturating_mul(3))
}

// ---- Gloom ------------------------------------------------------------------------------

/// Gloom: "White spells cost {3} more to cast." The generic increase for
/// casting `definition` as `method`'s form, by that form's colors, as for
/// wave P3a's Chill.
pub(crate) fn gloom_spell_tax(
    state: &GameState,
    definition: &CardDef,
    method: CastMethodV4,
) -> u32 {
    if gloom_id().is_none() || form_colors(state, definition, method) & white() == 0 {
        return 0;
    }
    gloom_count_tax(state)
}

/// Gloom: "Activated abilities of white enchantments cost {3} more to
/// activate." `source`'s current colors and types count. Mana abilities,
/// which the engine activates without this cost increase, are a known gap.
pub(crate) fn gloom_activation_tax(state: &GameState, source: ObjectId) -> u8 {
    if gloom_id().is_none()
        || !state
            .objects
            .try_get(source)
            .is_some_and(|live| live.zone == Zone::Battlefield)
        || crate::engine::object_color_mask(state, source) & white() == 0
        || !crate::engine::object_has_type(state, source, CardType::Enchantment)
    {
        return 0;
    }
    u8::try_from(gloom_count_tax(state)).unwrap_or(u8::MAX)
}

// ---- Mogg Salvage ---------------------------------------------------------------------

/// Mogg Salvage: "If an opponent controls an Island and you control a
/// Mountain, you may cast this spell without paying its mana cost." Land
/// subtypes as they are now: a dual land with both counts for either.
pub(crate) fn mogg_salvage_free(state: &GameState, player: PlayerId) -> bool {
    let controls = |controller: PlayerId, subtype: Subtype| {
        battlefield(state).any(|object| {
            state.objects.get(object).controller == controller
                && crate::engine::has_effective_subtype(state, object, subtype)
        })
    };
    controls(player.opponent(), Subtype::Island) && controls(player, Subtype::Mountain)
}

// ---- Dystopia ---------------------------------------------------------------------------

/// The age counters on `object`'s current incarnation.
pub(crate) fn age_counters(state: &GameState, object: ObjectId) -> u16 {
    let link = live_link(state, object);
    read_state(state)
        .and_then(|p3b| {
            p3b.age_counters
                .iter()
                .find(|(host, _)| *host == link)
                .map(|(_, count)| *count)
        })
        .unwrap_or(0)
}

// ---- Engineered Plague ----------------------------------------------------------------

/// The creature types an Engineered Plague may name: every creature type
/// printed in the card pool, once each (205.3m), in stable-id order. The
/// list doesn't depend on the game, so it reveals nothing hidden. Naming a
/// type no card in the pool has is the same as naming none of them.
pub fn plague_type_options() -> &'static [Subtype] {
    static OPTIONS: std::sync::OnceLock<Vec<Subtype>> = std::sync::OnceLock::new();
    OPTIONS.get_or_init(|| {
        let mut types: Vec<Subtype> = Vec::new();
        for def in CARD_DEFS.iter() {
            for &subtype in def.subtypes {
                if subtype.is_creature_type()
                    && !types.iter().any(|known| known.same_subtype_as(subtype))
                {
                    types.push(subtype);
                }
            }
        }
        types.sort_by_key(|subtype| subtype.stable_id());
        types
    })
}

/// The Engineered Plague that just entered and has not chosen a type.
fn plague_needing_type(state: &GameState) -> Option<ObjectId> {
    battlefield(state).find(|&object| {
        let live = state.objects.get(object);
        live.v4.face_index == 0
            && CARD_DEFS
                .get(usize::from(live.card_def))
                .is_some_and(|def| def.name == ENGINEERED_PLAGUE)
            && !read_state(state).is_some_and(|p3b| {
                p3b.plague_types
                    .iter()
                    .any(|(link, _)| *link == live_link(state, object))
            })
    })
}

/// "As this enchantment enters, choose a creature type": the pending choice,
/// one option per creature type.
pub(crate) fn entry_choice_decision(state: &GameState) -> Option<crate::engine::Decision> {
    let plague = plague_needing_type(state)?;
    Some(crate::engine::Decision::ChooseEffectOption {
        player: state.objects.get(plague).controller,
        source: plague,
        option_count: u16::try_from(plague_type_options().len()).unwrap_or(u16::MAX),
    })
}

/// Answers the pending Engineered Plague choice. `Ok(false)` when none is
/// choosing.
pub(crate) fn answer_entry_choice(state: &mut GameState, option: u16) -> Result<bool, String> {
    let Some(plague) = plague_needing_type(state) else {
        return Ok(false);
    };
    let subtype = *plague_type_options()
        .get(usize::from(option))
        .ok_or("creature type option is out of range")?;
    let link = live_link(state, plague);
    let types = &mut p3b_state(state).plague_types;
    types.retain(|(host, _)| host.object != plague);
    types.push((link, subtype));
    Ok(true)
}

/// The type `plague`'s current incarnation chose, if it has.
pub fn plague_chosen_type(state: &GameState, plague: ObjectId) -> Option<Subtype> {
    let link = live_link(state, plague);
    read_state(state)?
        .plague_types
        .iter()
        .find(|(host, _)| *host == link)
        .map(|(_, subtype)| *subtype)
}

/// Engineered Plague: "All creatures of the chosen type get -1/-1." The
/// change to `object`'s power and toughness from every Engineered Plague.
pub(crate) fn plague_boost(state: &GameState, object: ObjectId) -> (i128, i128) {
    let Some(p3b) = read_state(state) else {
        return (0, 0);
    };
    if p3b.plague_types.is_empty()
        || !state
            .objects
            .try_get(object)
            .is_some_and(|live| live.zone == Zone::Battlefield)
        || !crate::engine::object_has_type(state, object, CardType::Creature)
    {
        return (0, 0);
    }
    let count = p3b
        .plague_types
        .iter()
        .filter(|(link, subtype)| {
            state.objects.try_get(link.object).is_some_and(|live| {
                live.zone_change_count == link.zone_change_count
                    && active_named(state, link.object, ENGINEERED_PLAGUE)
            }) && crate::engine::has_effective_subtype(state, object, *subtype)
        })
        .count() as i128;
    (-count, -count)
}
