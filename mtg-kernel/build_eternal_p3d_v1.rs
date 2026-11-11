//! Card-database recipes for the eternal catalog's Premodern sideboard wave
//! P3d. This module is the tail of the recipe chains: wave P3c's tables
//! fall through to these, and its own defaults are the chains' base values.
//! Runtime programs live in `src/eternal_premodern_p3d_v1.rs`. Every row is
//! keyed by a name that occurs only in the eternal registry.

use super::{ActivatedAbilityRecipe, Special};

/// No P3d card has an activated ability.
pub(super) fn activated_ability_recipes_for(_name: &str) -> &'static [ActivatedAbilityRecipe] {
    &[]
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
        _ => return None,
    })
}

/// No P3d card has kicker.
pub(super) fn kicker_cost_for(_name: &str) -> Option<&'static str> {
    None
}

/// No P3d card is an Aura or Equipment.
pub(super) fn attachment_for(_name: &str) -> &'static str {
    "None"
}

/// No P3d card has flashback.
pub(super) fn flashback_for(_name: &str) -> String {
    "None".to_string()
}

/// No P3d card has an alternative cost.
pub(super) fn alt_cost_for(_name: &str) -> &'static str {
    "None"
}

/// No P3d card has a printed evergreen keyword.
pub(super) fn keywords_for(_name: &str) -> &'static [&'static str] {
    &[]
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
        _ => "none",
    }
}
