//! Static abilities and engine hooks of the wave P3a cards: Crimson
//! Acolyte's printed protection from red, True Believer's player shroud,
//! Cursed Totem's prohibition, Chill's tax on red spells and Sphere of Law's
//! prevention.

use super::{CHILL, CRIMSON_ACOLYTE, CURSED_TOTEM, SPHERE_OF_LAW, TRUE_BELIEVER};
use crate::card_def::{AltCostCondition, CardDef, CardType};
use crate::eternal_legacy_e9a_v1::active_named;
use crate::event::DamageProposed;
use crate::ids::{ObjectId, PlayerId};
use crate::mana::{Cost, ManaColor, Pip};
use crate::state::{CastMethodV4, GameState, Target, Zone};

/// The definition ids of the cards whose statics the engine asks about on
/// hot paths, so those checks compare ids before names.
struct StaticIds {
    chill: Option<u16>,
    cursed_totem: Option<u16>,
    sphere_of_law: Option<u16>,
    true_believer: Option<u16>,
}

fn static_ids() -> &'static StaticIds {
    static IDS: std::sync::OnceLock<StaticIds> = std::sync::OnceLock::new();
    IDS.get_or_init(|| StaticIds {
        chill: crate::card_def::card_id_by_name(CHILL),
        cursed_totem: crate::card_def::card_id_by_name(CURSED_TOTEM),
        sphere_of_law: crate::card_def::card_id_by_name(SPHERE_OF_LAW),
        true_believer: crate::card_def::card_id_by_name(TRUE_BELIEVER),
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
        .flat_map(move |player| state.players[player.index()].battlefield.iter().copied())
        .filter(move |&object| {
            id.is_some_and(|id| state.objects.get(object).card_def == id)
                && active_named(state, object, name)
                && controller.is_none_or(|player| state.objects.get(object).controller == player)
        })
}

const fn red() -> u8 {
    crate::card_def::mana_color_mask(ManaColor::R)
}

// ---- Protection and shroud --------------------------------------------------------------

/// Crimson Acolyte: "Protection from red", as a color mask. Wave E6b's
/// checks apply it to targeting, blocking, damage and attachments.
pub(crate) fn protection_mask(state: &GameState, host: ObjectId) -> u8 {
    if active_named(state, host, CRIMSON_ACOLYTE) {
        red()
    } else {
        0
    }
}

/// True Believer: "You have shroud." Whether `player` controls a True
/// Believer whose ability applies.
pub(crate) fn player_has_shroud(state: &GameState, player: PlayerId) -> bool {
    active_with_id(
        state,
        static_ids().true_believer,
        TRUE_BELIEVER,
        Some(player),
    )
    .next()
    .is_some()
}

// ---- Cursed Totem -----------------------------------------------------------------------

/// Cursed Totem: "Activated abilities of creatures can't be activated."
/// Whether it stops `object`'s: a creature on the battlefield (an animated
/// land or enchantment while animated), mana abilities included. Creature
/// cards in other zones keep theirs (cycling, ninjutsu).
pub(crate) fn activations_prohibited(state: &GameState, object: ObjectId) -> bool {
    state
        .objects
        .try_get(object)
        .is_some_and(|live| live.zone == Zone::Battlefield)
        && active_with_id(state, static_ids().cursed_totem, CURSED_TOTEM, None)
            .next()
            .is_some()
        && crate::engine::object_has_type(state, object, CardType::Creature)
}

// ---- Chill -------------------------------------------------------------------------------

/// Chill: "Red spells cost {2} more to cast." The generic increase for
/// casting `definition` as `method`'s form: {2} for each Chill whose ability
/// applies, when that form is red. It joins the spell's other increases, so
/// it also applies to a spell cast without paying its mana cost.
pub(crate) fn chill_tax(state: &GameState, definition: &CardDef, method: CastMethodV4) -> u32 {
    let chills = active_with_id(state, static_ids().chill, CHILL, None).count();
    if chills == 0 || form_colors(state, definition, method) & red() == 0 {
        return 0;
    }
    u32::try_from(chills).map_or(u32::MAX, |count| count.saturating_mul(2))
}

/// The colors of the spell `definition` becomes when cast as `method`'s form
/// (601.2a): a split half's or an Adventure's own cost's, none face down
/// (disguise and morph), with Painter's Servant's color.
fn form_colors(state: &GameState, definition: &CardDef, method: CastMethodV4) -> u8 {
    let face_down = method == CastMethodV4::Alternative
        && definition
            .alt_cost
            .is_some_and(|alt| alt.condition == AltCostCondition::DisguiseFromHand);
    let own = if face_down {
        0
    } else if method == CastMethodV4::Omen {
        definition.omen_spell_form().map_or_else(
            || crate::card_def::mana_colors_mask(definition.colors),
            |(cost, _)| cost_colors(cost),
        )
    } else if crate::eternal_premodern_p1a_v1::statics::split_card(definition.name) {
        cost_colors(definition.cost)
    } else {
        crate::card_def::mana_colors_mask(definition.colors)
    };
    own | crate::eternal_legacy_e9a_v1::statics::painter_mask(state)
}

/// The colors of `cost`'s mana symbols (202.2), as a mask.
fn cost_colors(cost: Cost) -> u8 {
    let color_mask = |color: ManaColor| {
        if color == ManaColor::C {
            0
        } else {
            crate::card_def::mana_color_mask(color)
        }
    };
    cost.pips.iter().fold(0, |mask, pip| {
        mask | match *pip {
            Pip::Colored(color) | Pip::Phyrexian(color) | Pip::PhyrexianAnyColor(color) => {
                color_mask(color)
            }
            Pip::Hybrid(first, second) => color_mask(first) | color_mask(second),
        }
    })
}

// ---- Sphere of Law -----------------------------------------------------------------------

/// Sphere of Law: "If a red source would deal damage to you, prevent 2 of
/// that damage." Reduces `damage` by 2 for each Sphere of Law whose ability
/// applies that its target player controls, when the damage's source is red.
/// Whether that prevents all of it. The caller skips it when damage can't be
/// prevented.
pub(crate) fn sphere_of_law_prevents(state: &GameState, damage: &mut DamageProposed) -> bool {
    let Target::Player(player) = damage.target else {
        return false;
    };
    if damage.amount <= 0 {
        return false;
    }
    let spheres = active_with_id(
        state,
        static_ids().sphere_of_law,
        SPHERE_OF_LAW,
        Some(player),
    )
    .count();
    if spheres == 0 || crate::event::damage_color_mask(state, damage) & red() == 0 {
        return false;
    }
    let prevented = i32::try_from(spheres).map_or(i32::MAX, |count| count.saturating_mul(2));
    damage.amount = damage.amount.saturating_sub(prevented).max(0);
    damage.amount == 0
}
