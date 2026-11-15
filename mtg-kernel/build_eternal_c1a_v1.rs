//! Card-database recipes for the eternal catalog's Vintage Cube wave C1a
//! (white and blue instants and sorceries). Wave P3h's tables fall through to these, and
//! these fall through to wave C1b's
//! (`build_eternal_c1b_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c1a_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{ActivatedAbilityRecipe, Special};

/// No C1a card has an activated ability yet.
pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    super::build_eternal_c1b_v1::activated_ability_recipes_for(name)
}

/// No C1a spell program yet.
pub(super) fn special_for(name: &str) -> Option<Special> {
    super::build_eternal_c1b_v1::special_for(name)
}

/// No C1a card has kicker yet.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c1b_v1::kicker_cost_for(name)
}

/// No C1a card is an Aura or an Equipment yet.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c1b_v1::attachment_for(name)
}

/// No C1a card has flashback yet.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c1b_v1::flashback_for(name)
}

/// No C1a card has an alternative cost yet.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c1b_v1::alt_cost_for(name)
}

/// No C1a card has a printed evergreen keyword yet.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    super::build_eternal_c1b_v1::keywords_for(name)
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    super::build_eternal_c1b_v1::trigger_recipe_for(name)
}
