//! Card-database recipes for the eternal catalog's Legacy sideboard wave
//! S1g. This module is the tail of the recipe chains: wave S1f's tables
//! fall through to these, and its own defaults are the chains' base values.
//! Runtime programs live in `src/eternal_sideboard_s1g_v1.rs`. Every row is
//! keyed by a name that occurs only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{ManaCost, SacrificeSelf, Tap};

/// Ghost Vacuum: "{T}: Exile target card from a graveyard." and "{6}, {T},
/// Sacrifice this artifact: Put each creature card exiled with this
/// artifact onto the battlefield under your control with a flying counter
/// on it. Each of them is a 1/1 Spirit in addition to its other types.
/// Activate only as a sorcery." The module records each exile with the
/// Vacuum's battlefield incarnation.
const GHOST_VACUUM: [ActivatedAbilityRecipe; 2] = [
    ActivatedAbilityRecipe {
        cost: &[Tap],
        effect: AbilityEffectRecipe::StandardProgram(
            "crate::eternal_sideboard_s1g_v1::vacuum_exile",
        ),
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec: "StandardV1(crate::standard_cards_v1::StandardTargetV1::CardInAGraveyard)",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
    ActivatedAbilityRecipe {
        cost: &[ManaCost("{6}"), Tap, SacrificeSelf],
        effect: AbilityEffectRecipe::StandardProgram(
            "crate::eternal_sideboard_s1g_v1::vacuum_return",
        ),
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Ghost Vacuum" => &GHOST_VACUUM,
        _ => &[],
    }
}

/// No S1g card is an instant, a sorcery or an Aura.
pub(super) fn special_for(_name: &str) -> Option<Special> {
    None
}

/// No S1g card has kicker.
pub(super) fn kicker_cost_for(_name: &str) -> Option<&'static str> {
    None
}

/// No S1g card is an Aura or an Equipment.
pub(super) fn attachment_for(_name: &str) -> &'static str {
    "None"
}

/// No S1g card has flashback.
pub(super) fn flashback_for(_name: &str) -> String {
    "None".to_string()
}

/// No S1g card has an alternative cost.
pub(super) fn alt_cost_for(_name: &str) -> &'static str {
    "None"
}

/// No S1g card has an evergreen keyword.
pub(super) fn keywords_for(_name: &str) -> &'static [&'static str] {
    &[]
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Damping Sphere" => "static:land_tapped_for_two_or_more_mana_produces_one_colorless;static:spells_cost_one_more_per_other_spell_caster_cast_this_turn",
        "Drop of Honey" => "upkeep:destroy_creature_with_least_power_cant_be_regenerated_you_choose_ties;state:no_creatures_on_battlefield_sacrifice_this_enchantment",
        "Alpine Moon" => "etb:choose_nonbasic_land_card_name;static:opponent_lands_with_chosen_name_lose_land_types_and_abilities_gain_tap_add_any_color",
        "Ghost Vacuum" => "activated:tap_exile_target_card_from_a_graveyard;activated:six_tap_sacrifice_return_creature_cards_exiled_with_this_flying_counter_1_1_spirit_sorcery_speed",
        _ => "none",
    }
}
