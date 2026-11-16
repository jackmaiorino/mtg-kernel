//! Card-database recipes for the eternal catalog's sideboard wave S1n. Wave
//! P3h's tables fall through to these, and these fall through to Vintage Cube
//! wave C1a's (`build_eternal_c1a_v1.rs`). Runtime programs
//! and static hooks live in `src/eternal_sideboard_s1n_v1.rs`. Every row is
//! keyed by a name that occurs only in the eternal registry.

use super::{ActivatedAbilityRecipe, Special};

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    super::build_eternal_c1a_v1::activated_ability_recipes_for(name)
}

/// Apostle's Blessing, From the Ashes and Kiora's Dismissal.
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Apostle's Blessing" => Some(Special::StandardProgram {
            target_spec:
                "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::ControlledArtifactOrCreatureV1)",
            program: "crate::eternal_sideboard_s1n_v1::apostles_blessing",
        }),
        "From the Ashes" => Some(Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_sideboard_s1n_v1::from_the_ashes",
        }),
        "Kiora's Dismissal" => Some(Special::StandardProgram {
            target_spec:
                "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::AnyNumberEnchantmentsV1)",
            program: "crate::eternal_sideboard_s1n_v1::kioras_dismissal",
        }),
        _ => super::build_eternal_c1a_v1::special_for(name),
    }
}

/// No S1n card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c1a_v1::kicker_cost_for(name)
}

/// No S1n card is an Aura or an Equipment.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c1a_v1::attachment_for(name)
}

/// No S1n card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c1a_v1::flashback_for(name)
}

/// No S1n card has an alternative cost; Kiora's Dismissal's strive is an
/// additional cost per target beyond the first (`spell_costs_v1`).
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c1a_v1::alt_cost_for(name)
}

/// Printed evergreen keywords on S1n permanents.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Doorkeeper Thrull" => &["Keywords::FLASH", "Keywords::FLYING"],
        "Darksteel Angel" => &["Keywords::FLYING", "Keywords::INDESTRUCTIBLE"],
        _ => super::build_eternal_c1a_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Apostle's Blessing" => {
            "spell:target_controlled_artifact_or_creature_protection_from_artifacts_or_color"
        }
        "Doorkeeper Thrull" => "static:artifacts_and_creatures_entering_suppress_triggers",
        "Darksteel Angel" => {
            "static:controller_cannot_lose_opponents_cannot_win;static:controlled_creatures_prevent_minus_1_minus_1_counters"
        }
        "From the Ashes" => {
            "spell:destroy_all_nonbasic_lands_each_controller_may_search_basic_lands"
        }
        "Kiora's Dismissal" => "strive:U;spell:return_any_number_target_enchantments_to_hand",
        _ => super::build_eternal_c1a_v1::trigger_recipe_for(name),
    }
}
