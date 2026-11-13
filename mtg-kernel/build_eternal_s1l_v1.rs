//! Card-database recipes for the eternal catalog's Legacy sideboard wave
//! S1l. Wave P3g's tables fall through to these, and these to wave S1m's
//! (`build_eternal_s1m_v1.rs`), the tail of the recipe chains. Runtime
//! programs live in `src/eternal_sideboard_s1l_v1.rs`. Every row is keyed
//! by a name that occurs only in the eternal registry.

use super::{ActivatedAbilityRecipe, Special};

/// No S1l card has an activated ability.
pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    super::build_eternal_s1m_v1::activated_ability_recipes_for(name)
}

/// No S1l card is an instant, a sorcery or an Aura.
pub(super) fn special_for(name: &str) -> Option<Special> {
    super::build_eternal_s1m_v1::special_for(name)
}

/// No S1l card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_s1m_v1::kicker_cost_for(name)
}

/// No S1l card is an Aura or an Equipment.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_s1m_v1::attachment_for(name)
}

/// No S1l card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_s1m_v1::flashback_for(name)
}

/// No S1l card has an alternative cost in the card database: Triumph of
/// Saint Katherine's miracle is wave E5b's draw trigger.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_s1m_v1::alt_cost_for(name)
}

/// Printed evergreen keywords: Triumph of Saint Katherine's "Lifelink".
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Triumph of Saint Katherine" => &["Keywords::LIFELINK"],
        _ => super::build_eternal_s1m_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Triumph of Saint Katherine" => {
            "lifelink;dies_to_owners_graveyard:exile_self_and_top_six_face_down_shuffle_pile_on_top;miracle:{1}{W}:first_draw_reveal_choice:trigger_pay_then_cast"
        }
        _ => super::build_eternal_s1m_v1::trigger_recipe_for(name),
    }
}
