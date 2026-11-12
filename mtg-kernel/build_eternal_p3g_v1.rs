//! Card-database recipes for the eternal catalog's Premodern sideboard wave
//! P3g. Wave S1j's tables fall through to these, and these to wave S1l's
//! (`build_eternal_s1l_v1.rs`), the tail of the recipe chains. Runtime
//! programs live in `src/eternal_premodern_p3g_v1.rs`. Every row is keyed by
//! a name that occurs only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{DiscardSelf, ManaCost};

/// Circle of Protection: Red: "{1}: The next time a red source of your
/// choice would deal damage to you this turn, prevent that damage."
const CIRCLE_OF_PROTECTION_RED: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{1}")],
    effect: AbilityEffectRecipe::StandardProgram(
        "crate::eternal_premodern_p3g_v1::circle_of_protection_red",
    ),
    activation_zone: "Battlefield",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Akroma's Vengeance: "Cycling {3}" (702.29): activated from hand,
/// discarding the card as part of its cost, to draw a card.
const AKROMAS_VENGEANCE: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{3}"), DiscardSelf],
    effect: AbilityEffectRecipe::DrawCards(1),
    activation_zone: "Hand",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Circle of Protection: Red" => &CIRCLE_OF_PROTECTION_RED,
        "Akroma's Vengeance" => &AKROMAS_VENGEANCE,
        _ => super::build_eternal_s1l_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function of `eternal_premodern_p3g_v1`.
const fn spell(program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec: "TargetSpec::None",
        program,
    }
}

/// Infest: "All creatures get -2/-2 until end of turn." Akroma's Vengeance:
/// "Destroy all artifacts, creatures, and enchantments."
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Infest" => Some(spell("crate::eternal_premodern_p3g_v1::infest")),
        "Akroma's Vengeance" => Some(spell("crate::eternal_premodern_p3g_v1::akromas_vengeance")),
        _ => super::build_eternal_s1l_v1::special_for(name),
    }
}

/// No P3g card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_s1l_v1::kicker_cost_for(name)
}

/// No P3g card is an Aura or an Equipment.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_s1l_v1::attachment_for(name)
}

/// No P3g card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_s1l_v1::flashback_for(name)
}

/// No P3g card has an alternative cost.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_s1l_v1::alt_cost_for(name)
}

/// No P3g card has a printed evergreen keyword.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    super::build_eternal_s1l_v1::keywords_for(name)
}

/// Card-database tokens for this wave's abilities. Arcane Laboratory's limit
/// is Archon of Emeria's (`eternal_vintage_v1::cast_prohibited`).
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Circle of Protection: Red" => {
            "ability:choose_red_source:prevent_next_damage_to_you_from_it_this_turn"
        }
        "Arcane Laboratory" => "static:each_player_cant_cast_more_than_one_spell_each_turn",
        "Infest" => "spell:all_creatures_get_minus_2_minus_2_until_end_of_turn",
        "Akroma's Vengeance" => "spell:destroy_all_artifacts_creatures_enchantments;cycling:3",
        _ => super::build_eternal_s1l_v1::trigger_recipe_for(name),
    }
}
