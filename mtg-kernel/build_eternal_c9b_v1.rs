//! Card-database recipes for the eternal catalog's Vintage Cube wave C9b
//! (Figure of Destiny, Figure of Fable, Hexdrinker, Nadu, Winged Wisdom,
//! Serra Paragon, and Swift Reconfiguration). Wave C9a's tables fall through
//! to these, and these fall through to wave C9c's
//! (`build_eternal_c9c_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c9b_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};

/// Figure of Destiny's three permanent characteristic-setting activations.
const FIGURE_OF_DESTINY: [ActivatedAbilityRecipe; 3] = [
    ActivatedAbilityRecipe {
        cost: &[AbilityCostRecipe::ManaCost("{R/W}")],
        effect: AbilityEffectRecipe::StandardProgram(
            "crate::eternal_cube_c9b_v1::figure_of_destiny_tier_1",
        ),
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
    ActivatedAbilityRecipe {
        cost: &[AbilityCostRecipe::ManaCost("{R/W}{R/W}{R/W}")],
        effect: AbilityEffectRecipe::StandardProgram(
            "crate::eternal_cube_c9b_v1::figure_of_destiny_tier_2",
        ),
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
    ActivatedAbilityRecipe {
        cost: &[AbilityCostRecipe::ManaCost(
            "{R/W}{R/W}{R/W}{R/W}{R/W}{R/W}",
        )],
        effect: AbilityEffectRecipe::StandardProgram(
            "crate::eternal_cube_c9b_v1::figure_of_destiny_tier_3",
        ),
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
];

/// Figure of Fable's three permanent characteristic-setting activations.
const FIGURE_OF_FABLE: [ActivatedAbilityRecipe; 3] = [
    ActivatedAbilityRecipe {
        cost: &[AbilityCostRecipe::ManaCost("{G/W}")],
        effect: AbilityEffectRecipe::StandardProgram(
            "crate::eternal_cube_c9b_v1::figure_of_fable_tier_1",
        ),
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
    ActivatedAbilityRecipe {
        cost: &[AbilityCostRecipe::ManaCost("{1}{G/W}{G/W}")],
        effect: AbilityEffectRecipe::StandardProgram(
            "crate::eternal_cube_c9b_v1::figure_of_fable_tier_2",
        ),
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
    ActivatedAbilityRecipe {
        cost: &[AbilityCostRecipe::ManaCost("{3}{G/W}{G/W}{G/W}")],
        effect: AbilityEffectRecipe::StandardProgram(
            "crate::eternal_cube_c9b_v1::figure_of_fable_tier_3",
        ),
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
];

/// Hexdrinker: "Level up {1}" (702.87a, sorcery speed only).
const HEXDRINKER: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[AbilityCostRecipe::ManaCost("{1}")],
    effect: AbilityEffectRecipe::StandardProgram("crate::eternal_cube_c9b_v1::hexdrinker_level_up"),
    activation_zone: "Battlefield",
    sorcery_speed_only: true,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Figure of Destiny" => &FIGURE_OF_DESTINY,
        "Figure of Fable" => &FIGURE_OF_FABLE,
        "Hexdrinker" => &HEXDRINKER,
        _ => super::build_eternal_c9c_v1::activated_ability_recipes_for(name),
    }
}

/// Swift Reconfiguration's Aura entry (enchant creature or Vehicle).
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Swift Reconfiguration" => Special::StandardProgram {
            target_spec: "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::CreatureOrVehicleV1)",
            program: "crate::standard_cards_v1::aura_enters",
        },
        _ => return super::build_eternal_c9c_v1::special_for(name),
    })
}

/// No C9b card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c9c_v1::kicker_cost_for(name)
}

/// Swift Reconfiguration uses `standard_cards_v1::is_aura` / `aura_host_legal`.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c9c_v1::attachment_for(name)
}

/// No C9b card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c9c_v1::flashback_for(name)
}

/// No C9b card has an alternative cost.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c9c_v1::alt_cost_for(name)
}

/// Printed evergreen keywords of this wave's cards.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Nadu, Winged Wisdom" | "Serra Paragon" => &["Keywords::FLYING"],
        "Swift Reconfiguration" => &["Keywords::FLASH"],
        _ => super::build_eternal_c9c_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Figure of Destiny" => "activated:becomes_2_2_kithkin_spirit;activated_if_spirit:becomes_4_4_kithkin_spirit_warrior;activated_if_warrior:becomes_8_8_kithkin_spirit_warrior_avatar_flying_first_strike",
        "Figure of Fable" => "activated:becomes_2_3_kithkin_scout;activated_if_scout:becomes_4_5_kithkin_soldier;activated_if_soldier:becomes_7_8_kithkin_avatar_protection_from_each_opponent",
        "Hexdrinker" => "level_up:1;level_3_7:4_4_protection_from_instants;level_8_plus:6_6_protection_from_everything",
        "Nadu, Winged Wisdom" => "static:controlled_creatures_have_becomes_target_reveal_top_land_battlefield_else_hand_max_twice_each_turn",
        "Serra Paragon" => "static:once_each_your_turn_play_land_or_cast_permanent_mv_le_3_from_graveyard_gains_dies_exile_and_gain_2_life",
        "Swift Reconfiguration" => "enchant:creature_or_vehicle;static:enchanted_permanent_is_vehicle_artifact_with_crew_5_loses_other_card_types",
        _ => super::build_eternal_c9c_v1::trigger_recipe_for(name),
    }
}
