//! Card-database recipes for the eternal catalog's Vintage sideboard wave
//! S1j. Wave S1g's tables fall through to these, and these to wave P3g's
//! (`build_eternal_p3g_v1.rs`), the tail of the recipe chains. Runtime
//! hooks live in `src/eternal_sideboard_s1j_v1.rs`. Every row is keyed by a
//! name that occurs only in the eternal registry.

use super::{ActivatedAbilityRecipe, Special};

/// No S1j card has an activated ability.
pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    super::build_eternal_p3g_v1::activated_ability_recipes_for(name)
}

/// No S1j card is an instant, a sorcery or an Aura.
pub(super) fn special_for(name: &str) -> Option<Special> {
    super::build_eternal_p3g_v1::special_for(name)
}

/// No S1j card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_p3g_v1::kicker_cost_for(name)
}

/// No S1j card is an Aura or an Equipment.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_p3g_v1::attachment_for(name)
}

/// No S1j card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_p3g_v1::flashback_for(name)
}

/// No S1j card has an alternative cost.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_p3g_v1::alt_cost_for(name)
}

/// Printed evergreen keywords: Archon of Absolution's "Flying".
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Archon of Absolution" => &["Keywords::FLYING"],
        _ => super::build_eternal_p3g_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Archon of Absolution" => "flying;protection_from_white;static:creatures_cant_attack_you_or_your_planeswalkers_unless_controller_pays_1_for_each",
        _ => super::build_eternal_p3g_v1::trigger_recipe_for(name),
    }
}
