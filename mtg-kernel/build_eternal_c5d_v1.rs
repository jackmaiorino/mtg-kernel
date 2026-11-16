//! Card-database recipes for the eternal catalog's Vintage Cube wave C5d
//! (each-player decision spells and hidden-zone artifacts). Wave C5c's tables fall through to these, and
//! this module is the tail of the recipe chains: its own
//! defaults are the chains' base values. Runtime programs live in
//! `src/eternal_cube_c5d_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{ActivatedAbilityRecipe, Special};

/// No C5d card has an activated ability yet.
pub(super) fn activated_ability_recipes_for(_name: &str) -> &'static [ActivatedAbilityRecipe] {
    &[]
}

/// No C5d spell program yet.
pub(super) fn special_for(_name: &str) -> Option<Special> {
    None
}

/// No C5d card has kicker yet.
pub(super) fn kicker_cost_for(_name: &str) -> Option<&'static str> {
    None
}

/// No C5d card is an Aura or an Equipment yet.
pub(super) fn attachment_for(_name: &str) -> &'static str {
    "None"
}

/// No C5d card has flashback yet.
pub(super) fn flashback_for(_name: &str) -> String {
    "None".to_string()
}

/// No C5d card has an alternative cost yet.
pub(super) fn alt_cost_for(_name: &str) -> &'static str {
    "None"
}

/// No C5d card has a printed evergreen keyword yet.
pub(super) fn keywords_for(_name: &str) -> &'static [&'static str] {
    &[]
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(_name: &str) -> &'static str {
    "none"
}
