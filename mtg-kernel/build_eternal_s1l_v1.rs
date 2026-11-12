//! Card-database recipes for the eternal catalog's Legacy sideboard wave
//! S1l. This module is the tail of the recipe chains: wave P3g's tables
//! fall through to these, and its own defaults are the chains' base values.
//! Runtime programs live in `src/eternal_sideboard_s1l_v1.rs`. Every row is
//! keyed by a name that occurs only in the eternal registry.

use super::{ActivatedAbilityRecipe, Special};

/// No S1l card has an activated ability.
pub(super) fn activated_ability_recipes_for(_name: &str) -> &'static [ActivatedAbilityRecipe] {
    &[]
}

/// No S1l card is an instant, a sorcery or an Aura.
pub(super) fn special_for(_name: &str) -> Option<Special> {
    None
}

/// No S1l card has kicker.
pub(super) fn kicker_cost_for(_name: &str) -> Option<&'static str> {
    None
}

/// No S1l card is an Aura or an Equipment.
pub(super) fn attachment_for(_name: &str) -> &'static str {
    "None"
}

/// No S1l card has flashback.
pub(super) fn flashback_for(_name: &str) -> String {
    "None".to_string()
}

/// No S1l card has an alternative cost in the card database: Triumph of
/// Saint Katherine's miracle is wave E5b's draw trigger.
pub(super) fn alt_cost_for(_name: &str) -> &'static str {
    "None"
}

/// Printed evergreen keywords: Triumph of Saint Katherine's "Lifelink".
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Triumph of Saint Katherine" => &["Keywords::LIFELINK"],
        _ => &[],
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Triumph of Saint Katherine" => {
            "lifelink;dies_to_owners_graveyard:exile_self_and_top_six_face_down_shuffle_pile_on_top;miracle:{1}{W}:first_draw_reveal_choice:trigger_pay_then_cast"
        }
        _ => "none",
    }
}
