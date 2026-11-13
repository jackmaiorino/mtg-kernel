//! Card-database recipes for the eternal catalog's Premodern sideboard wave
//! P3d. Wave P3c's tables fall through to these, and these fall through to
//! wave P3e's (`build_eternal_p3e_v1.rs`).
//! Runtime programs live in `src/eternal_premodern_p3d_v1.rs`. Every row is
//! keyed by a name that occurs only in the eternal registry.

use super::{ActivatedAbilityRecipe, Special};

/// No P3d card has an activated ability.
pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    super::build_eternal_p3e_v1::activated_ability_recipes_for(name)
}

/// A spell whose program is a runtime function of
/// `eternal_premodern_p3d_v1`.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        // The bans are `eternal_premodern_p3d_v1::abeyance_prohibits_cast`
        // and `abeyance_active`.
        "Abeyance" => spell(
            "TargetSpec::AnyPlayer",
            "crate::eternal_premodern_p3d_v1::abeyance",
        ),
        _ => return super::build_eternal_p3e_v1::special_for(name),
    })
}

/// No P3d card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_p3e_v1::kicker_cost_for(name)
}

/// No P3d card is an Aura or Equipment.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_p3e_v1::attachment_for(name)
}

/// No P3d card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_p3e_v1::flashback_for(name)
}

/// No P3d card has an alternative cost.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_p3e_v1::alt_cost_for(name)
}

/// No P3d card has a printed evergreen keyword.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    super::build_eternal_p3e_v1::keywords_for(name)
}

/// Card-database tokens for the triggered abilities in
/// `eternal_premodern_p3d_v1`. Karmic Justice's and Sacred Ground's
/// triggers are queued by its departure hook.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Abeyance" => {
            "spell:target_player_cant_cast_instant_sorcery_or_activate_nonmana_this_turn;draw_1"
        }
        "Karmic Justice" => {
            "opponent_spell_or_ability_destroys_your_noncreature_permanent:may_destroy_target_opponent_permanent"
        }
        "Sacred Ground" => {
            "opponent_spell_or_ability_puts_your_land_into_graveyard:return_it_to_battlefield"
        }
        _ => super::build_eternal_p3e_v1::trigger_recipe_for(name),
    }
}
