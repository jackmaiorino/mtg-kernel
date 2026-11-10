//! Static abilities and engine hooks of the sideboard wave S1a cards:
//! Choke's untap restriction, Defense Grid's and Disruptor Flute's spell
//! tax, Disruptor Flute's activation prohibition, Dread of Night's
//! power/toughness modifier, the zone-change replacements of Rest in Peace,
//! Leyline of the Void and Containment Priest, and Massacre's alternative
//! cost condition.

use super::{
    active_named, battlefield, link_live_in, live_link, read_state, s1a_state, CHOKE,
    CONTAINMENT_PRIEST, DEFENSE_GRID, DISRUPTOR_FLUTE, DREAD_OF_NIGHT, LAVINIA,
    LEYLINE_OF_THE_VOID, REST_IN_PEACE,
};
use crate::card_def::{CardType, Subtype, CARD_DEFS};
use crate::event;
use crate::ids::{ObjectId, PlayerId};
use crate::state::CastMethodV4;
use crate::state::{GameState, Zone};

/// The definition ids of the cards whose statics the engine asks about on
/// hot paths, so those checks compare ids before names.
struct StaticIds {
    choke: Option<u16>,
    defense_grid: Option<u16>,
    dread_of_night: Option<u16>,
}

fn static_ids() -> &'static StaticIds {
    static IDS: std::sync::OnceLock<StaticIds> = std::sync::OnceLock::new();
    IDS.get_or_init(|| StaticIds {
        choke: crate::card_def::card_id_by_name(CHOKE),
        defense_grid: crate::card_def::card_id_by_name(DEFENSE_GRID),
        dread_of_night: crate::card_def::card_id_by_name(DREAD_OF_NIGHT),
    })
}

/// The battlefield permanents with definition `id`, named `name`, whose
/// printed abilities apply. Compares definition ids first.
fn active_with_id<'a>(
    state: &'a GameState,
    id: Option<u16>,
    name: &'a str,
) -> impl Iterator<Item = ObjectId> + 'a {
    battlefield(state).filter(move |&object| {
        id.is_some_and(|id| state.objects.get(object).card_def == id)
            && active_named(state, object, name)
    })
}

// ---- Choke -------------------------------------------------------------------------

/// Choke: "Islands don't untap during their controllers' untap steps."
pub(crate) fn prevents_untap_step(state: &GameState, object: ObjectId) -> bool {
    if active_with_id(state, static_ids().choke, CHOKE)
        .next()
        .is_none()
    {
        return false;
    }
    crate::engine::object_has_type(state, object, CardType::Land)
        && crate::engine::has_effective_subtype(state, object, Subtype::Island)
}

// ---- Spell taxes -------------------------------------------------------------------

/// Defense Grid: "Each spell costs {3} more to cast except during its
/// controller's turn." The generic increase for a spell `caster` casts.
pub(crate) fn spell_tax(state: &GameState, caster: PlayerId) -> u32 {
    if caster == state.active_player {
        return 0;
    }
    let grids = active_with_id(state, static_ids().defense_grid, DEFENSE_GRID).count();
    u32::try_from(grids).unwrap_or(u32::MAX).saturating_mul(3)
}

/// The definitions whose names live Disruptor Flutes chose.
fn flute_names(state: &GameState) -> Vec<u16> {
    let Some(s1a) = read_state(state) else {
        return Vec::new();
    };
    s1a.flute_names
        .iter()
        .filter(|(flute, _)| {
            link_live_in(state, *flute, Zone::Battlefield)
                && active_named(state, flute.object, DISRUPTOR_FLUTE)
        })
        .map(|(_, named)| *named)
        .collect()
}

/// Disruptor Flute: "Spells with the chosen name cost {3} more to cast." The
/// generic increase for a spell named `name`.
pub(crate) fn flute_spell_tax(state: &GameState, name: &str) -> u32 {
    let count = flute_names(state)
        .into_iter()
        .filter(|&named| CARD_DEFS[usize::from(named)].object_name == name)
        .count();
    u32::try_from(count).unwrap_or(u32::MAX).saturating_mul(3)
}

/// Disruptor Flute: "Activated abilities of sources with the chosen name
/// can't be activated unless they're mana abilities." Whether a non-mana
/// ability of `source` may be activated.
pub(crate) fn activation_allowed(state: &GameState, source: ObjectId) -> bool {
    let names = flute_names(state);
    if names.is_empty() {
        return true;
    }
    let source_name = crate::engine::effective_name(state, source);
    !names
        .into_iter()
        .any(|named| CARD_DEFS[usize::from(named)].object_name == source_name)
}

// ---- Dread of Night ----------------------------------------------------------------

/// Dread of Night: "White creatures get -1/-1." The modifier to `object`'s
/// power and toughness (each).
pub(crate) fn pt_modifier(state: &GameState, object: ObjectId) -> i32 {
    let Some(live) = state.objects.try_get(object) else {
        return 0;
    };
    if live.zone != Zone::Battlefield
        || crate::engine::object_color_mask(state, object)
            & crate::card_def::mana_color_mask(crate::mana::ManaColor::W)
            == 0
        || !crate::engine::object_has_type(state, object, CardType::Creature)
    {
        return 0;
    }
    let count = active_with_id(state, static_ids().dread_of_night, DREAD_OF_NIGHT).count();
    -i32::try_from(count).unwrap_or(i32::MAX / 2)
}

// ---- Zone-change replacements --------------------------------------------------------

/// Whether any live permanent named `name` is controlled by a player for
/// whom `wanted` holds.
fn any_active_controlled_by(
    state: &GameState,
    name: &str,
    wanted: impl Fn(PlayerId) -> bool,
) -> bool {
    battlefield(state).any(|object| {
        active_named(state, object, name) && wanted(state.objects.get(object).controller)
    })
}

/// Rest in Peace: "If a card or token would be put into a graveyard from
/// anywhere, exile it instead." Leyline of the Void: "If a card would be put
/// into an opponent's graveyard from anywhere, exile it instead." Containment
/// Priest: "If a nontoken creature would enter and it wasn't cast, exile it
/// instead." Adjusts the proposed zone change.
pub(crate) fn replace_zone_change(state: &GameState, change: &mut event::ZoneChangeProposed) {
    let Some(live) = state.objects.try_get(change.object) else {
        return;
    };
    // A spell copy is not a card and is never put anywhere.
    if live.spell_copy_origin.is_some() {
        return;
    }
    let is_token = live.v4.is_token;
    match change.to_zone {
        Zone::Graveyard => {
            let owner = live.owner;
            if any_active_controlled_by(state, REST_IN_PEACE, |_| true)
                || (!is_token
                    && any_active_controlled_by(state, LEYLINE_OF_THE_VOID, |controller| {
                        controller != owner
                    }))
            {
                change.to_zone = Zone::Exile;
            }
        }
        Zone::Battlefield => {
            if is_token || live.zone == Zone::Stack || live.zone == Zone::Battlefield {
                return;
            }
            let printing = crate::linked_aura_operation_v1::metamorph_mage_copy(state, change.object)
                .map_or(live.card_def, |copy| copy.copied.card_def);
            let Some(def) = CARD_DEFS.get(usize::from(printing)) else {
                return;
            };
            if !change.battlefield_manifested_v1 && !def.has_type(CardType::Creature) {
                return;
            }
            // The Priest is on the battlefield; it does not exile itself.
            let priest = battlefield(state).any(|object| {
                object != change.object && active_named(state, object, CONTAINMENT_PRIEST)
            });
            if priest {
                change.to_zone = Zone::Exile;
            }
        }
        _ => {}
    }
}

// ---- Massacre ----------------------------------------------------------------------

/// Massacre: "If an opponent controls a Plains and you control a Swamp, you
/// may cast this spell without paying its mana cost."
pub(crate) fn massacre_free(state: &GameState, player: PlayerId) -> bool {
    let controls = |who: PlayerId, subtype: Subtype| {
        state.players[who.index()]
            .battlefield
            .iter()
            .any(|&object| {
                state.objects.get(object).zone == Zone::Battlefield
                    && crate::engine::has_effective_subtype(state, object, subtype)
            })
    };
    controls(player.opponent(), Subtype::Plains) && controls(player, Subtype::Swamp)
}

// ---- Lavinia, Azorius Renegade ------------------------------------------------------

/// The live permanents named `name` controlled by `controller`.
pub(crate) fn active_controlled_by(
    state: &GameState,
    name: &str,
    controller: PlayerId,
) -> Vec<ObjectId> {
    battlefield(state)
        .filter(|&object| {
            active_named(state, object, name) && state.objects.get(object).controller == controller
        })
        .collect()
}

/// The lands `player` controls.
fn lands_controlled_by(state: &GameState, player: PlayerId) -> usize {
    battlefield(state)
        .filter(|&object| {
            state.objects.get(object).controller == player
                && crate::engine::object_has_type(state, object, CardType::Land)
        })
        .count()
}

/// The Islands `player` controls.
pub(crate) fn islands_controlled_by(state: &GameState, player: PlayerId) -> u8 {
    let count = battlefield(state)
        .filter(|&object| {
            state.objects.get(object).controller == player
                && crate::engine::object_has_type(state, object, CardType::Land)
                && crate::engine::has_effective_subtype(state, object, Subtype::Island)
        })
        .count();
    u8::try_from(count).unwrap_or(u8::MAX)
}

/// Shared quote and payment restriction for Lavinia and Meddling Mage.
/// Lavinia checks the selected form and announced X. Alternative costs and
/// additional payments do not change mana value.
pub(crate) fn cast_prohibited(
    state: &GameState,
    player: PlayerId,
    card: ObjectId,
    method: CastMethodV4,
    x: u16,
) -> bool {
    let Some(def) = state
        .objects
        .try_get(card)
        .and_then(|live| CARD_DEFS.get(usize::from(live.card_def)))
    else {
        return false;
    };
    if crate::eternal_sideboard_s1b_v1::mage_prohibits(state, def, method) {
        return true;
    }
    if active_controlled_by(state, LAVINIA, player.opponent()).is_empty() {
        return false;
    }
    let Ok(types) = crate::source_type_hexproof_v1::spell_types(def, method) else {
        return true;
    };
    if types.contains(CardType::Creature) {
        return false;
    }
    // Overload uses the second-form machinery, but its printed mana cost
    // stays unchanged. Genuine split halves and Omen/Adventure use their own.
    let (base, x_count) = if crate::eternal_sideboard_s1d_v1::statics::is_overload(def.name) {
        (usize::from(def.mana_value), def.cost.x_count)
    } else if let Some(value) = crate::standard_cards_v1::room_cast_mana_value(def, method) {
        (usize::from(value), 0)
    } else if method == CastMethodV4::Omen {
        let Some((cost, _)) = def.omen_spell_form() else {
            return true;
        };
        (usize::from(cost.generic) + cost.pips.len(), cost.x_count)
    } else if crate::eternal_premodern_p1a_v1::statics::split_card(def.name) {
        (
            usize::from(def.cost.generic) + def.cost.pips.len(),
            def.cost.x_count,
        )
    } else {
        (usize::from(def.mana_value), def.cost.x_count)
    };
    let mana_value = base + usize::from(x) * usize::from(x_count);
    mana_value > lands_controlled_by(state, player)
}

/// Lavinia, Azorius Renegade: records that `spell` was cast with no mana
/// spent while an opponent's Lavinia was in play, for her counter trigger.
pub(crate) fn note_mana_spent(
    state: &mut GameState,
    spell: ObjectId,
    controller: PlayerId,
    spent: u32,
) {
    if spent != 0 || active_controlled_by(state, LAVINIA, controller.opponent()).is_empty() {
        return;
    }
    let link = live_link(state, spell);
    let unpaid = &mut s1a_state(state).unpaid_casts;
    if !unpaid.contains(&link) {
        unpaid.push(link);
    }
}
