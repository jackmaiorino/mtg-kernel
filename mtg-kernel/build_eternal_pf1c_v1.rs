//! Card-database recipes for the eternal catalog's Pauper field wave Pf1c
//! (four cards that each need a new engine piece). Wave Pf1b's tables fall through to these, and
//! this module is the tail of the recipe chains: its own
//! defaults are the chains' base values. Runtime programs live in
//! `src/eternal_pauper_pf1c_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{ActivatedAbilityRecipe, Special};

/// No Pf1c card has an activated ability yet.
pub(super) fn activated_ability_recipes_for(_name: &str) -> &'static [ActivatedAbilityRecipe] {
    &[]
}

/// No Pf1c spell program yet.
pub(super) fn special_for(_name: &str) -> Option<Special> {
    None
}

/// No Pf1c card has kicker yet.
pub(super) fn kicker_cost_for(_name: &str) -> Option<&'static str> {
    None
}

/// No Pf1c card is an Aura or an Equipment yet.
pub(super) fn attachment_for(_name: &str) -> &'static str {
    "None"
}

/// No Pf1c card has flashback yet.
pub(super) fn flashback_for(_name: &str) -> String {
    "None".to_string()
}

/// No Pf1c card has an alternative cost yet.
pub(super) fn alt_cost_for(_name: &str) -> &'static str {
    "None"
}

/// Boarding Action's Klingon Warrior token has haste. (No Pf1c card has a
/// printed evergreen keyword yet.)
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Klingon Warrior Token" => &["Keywords::HASTE"],
        _ => &[],
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(_name: &str) -> &'static str {
    "none"
}
