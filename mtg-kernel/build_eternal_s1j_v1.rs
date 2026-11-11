//! Card-database recipes for the eternal catalog's Vintage sideboard wave
//! S1j. This module is the tail of the recipe chains: wave S1g's tables
//! fall through to these, and its own defaults are the chains' base values.
//! Runtime hooks live in `src/eternal_sideboard_s1j_v1.rs`. Every row is
//! keyed by a name that occurs only in the eternal registry.

use super::{ActivatedAbilityRecipe, Special};

/// No S1j card has an activated ability.
pub(super) fn activated_ability_recipes_for(_name: &str) -> &'static [ActivatedAbilityRecipe] {
    &[]
}

/// No S1j card is an instant, a sorcery or an Aura.
pub(super) fn special_for(_name: &str) -> Option<Special> {
    None
}

/// No S1j card has kicker.
pub(super) fn kicker_cost_for(_name: &str) -> Option<&'static str> {
    None
}

/// No S1j card is an Aura or an Equipment.
pub(super) fn attachment_for(_name: &str) -> &'static str {
    "None"
}

/// No S1j card has flashback.
pub(super) fn flashback_for(_name: &str) -> String {
    "None".to_string()
}

/// No S1j card has an alternative cost.
pub(super) fn alt_cost_for(_name: &str) -> &'static str {
    "None"
}

/// Printed evergreen keywords: Archon of Absolution's "Flying".
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Archon of Absolution" => &["Keywords::FLYING"],
        _ => &[],
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Archon of Absolution" => "flying;protection_from_white;static:creatures_cant_attack_you_or_your_planeswalkers_unless_controller_pays_1_for_each",
        _ => "none",
    }
}
