//! Card-database recipes for the eternal catalog's Vintage Cube wave C7b
//! (extended mana abilities, damage modifiers and Palace Jailer). Wave C7a's
//! tables fall through to these, and these fall through to wave C7c's
//! (`build_eternal_c7c_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c7b_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.
//!
//! Mana ability definitions for Orcish Lumberjack, Urza, Lord High Artificer
//! and Vivi Ornitier live in `build_eternal_v1.rs` (`mana_ability_def_for`),
//! with Orcish Lumberjack's mixed RRG/RGG bundles in `build_eternal_e6a_v1.rs`
//! (`additional_mana_abilities_for`) and Vivi Ornitier's single-action color
//! override in `build.rs` (`primary_mana_ability_colors_for`).

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::ManaCost;

const fn instant(
    cost: &'static [AbilityCostRecipe],
    effect: AbilityEffectRecipe,
    target_spec: &'static str,
) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect,
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec,
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Urza, Lord High Artificer: "{5}: Shuffle your library, then exile the top
/// card. Until end of turn, you may play that card without paying its mana
/// cost."
const URZA_LORD_HIGH_ARTIFICER: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{5}")],
    program("crate::eternal_cube_c7b_v1::urza_exile"),
    "None",
)];

/// Activated abilities of this wave's cards.
pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Urza, Lord High Artificer" => &URZA_LORD_HIGH_ARTIFICER,
        _ => super::build_eternal_c7c_v1::activated_ability_recipes_for(name),
    }
}

/// No C7b spell program: every card in this wave is a creature.
pub(super) fn special_for(name: &str) -> Option<Special> {
    super::build_eternal_c7c_v1::special_for(name)
}

/// No C7b card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c7c_v1::kicker_cost_for(name)
}

/// No C7b card is an Aura or an Equipment.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c7c_v1::attachment_for(name)
}

/// No C7b card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c7c_v1::flashback_for(name)
}

/// No C7b card has an alternative cost.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c7c_v1::alt_cost_for(name)
}

/// Printed evergreen keywords of this wave's cards.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Black Widow, Super Spy" => &["Keywords::MENACE"],
        "Questing Beast" => &[
            "Keywords::VIGILANCE",
            "Keywords::DEATHTOUCH",
            "Keywords::HASTE",
        ],
        _ => super::build_eternal_c7c_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Orcish Lumberjack" => "mana:tap_sacrifice_forest:three_in_any_combination_of_r_g",
        "Urza, Lord High Artificer" => {
            "etb:create_karn_construct_token;mana:tap_untapped_artifact:u;activated:5_shuffle_exile_top_play_free_eot"
        }
        "Vivi Ornitier" => {
            "mana:0_own_turn_once_per_turn_source_power_any_combination_u_r;cast_noncreature:plus1_counter_self_and_1_damage_opponent"
        }
        "Black Widow, Super Spy" => {
            "combat_damage_player:exile_until_nonland_may_plus1_counter_else_cast_eot_any_mana_type"
        }
        "Soul-Scar Mage" => {
            "prowess;static:noncombat_damage_to_opponent_creature_becomes_minus1_counters"
        }
        "Questing Beast" => {
            "static:cant_be_blocked_by_power_2_or_less;static:controlled_creature_combat_damage_cant_be_prevented;combat_damage_opponent:damage_target_opponent_planeswalker"
        }
        "Palace Jailer" => {
            "etb:become_monarch;etb:exile_target_opponent_creature_until_opponent_becomes_monarch"
        }
        _ => super::build_eternal_c7c_v1::trigger_recipe_for(name),
    }
}
