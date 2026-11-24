//! Card-database recipes for the eternal catalog's Vintage Cube wave C8c
//! (Pyrogoyf, Elite Spellbinder, Wan Shi Tong, Thieving Skydiver, Zirda,
//! and Absorbing Man). Wave C8b's tables fall through to these, and this
//! module is the tail of the recipe chains: its own defaults are the chains'
//! base values. Runtime programs live in `src/eternal_cube_c8c_v1.rs`. Every
//! row is keyed by a name that occurs only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};

/// Zirda, the Dawnwaker:
/// - Slot 0: companion special action (702.139c) from exile (`{3}` as a
///   sorcery to put into hand, matching Lurrus, Yorion, and Lutri).
/// - Slot 1: "{1}, {T}: Target creature can't block this turn."
const ZIRDA: [ActivatedAbilityRecipe; 2] = [
    ActivatedAbilityRecipe {
        cost: &[AbilityCostRecipe::ManaCost("{3}")],
        effect: AbilityEffectRecipe::StandardProgram("crate::eternal_lurrus_v1::companion_to_hand"),
        activation_zone: "Exile",
        sorcery_speed_only: true,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
    ActivatedAbilityRecipe {
        cost: &[AbilityCostRecipe::ManaCost("{1}"), AbilityCostRecipe::Tap],
        effect: AbilityEffectRecipe::StandardProgram(
            "crate::eternal_cube_c8c_v1::zirda_cant_block",
        ),
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec: "Creature",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Zirda, the Dawnwaker" => &ZIRDA,
        _ => &[],
    }
}

/// Spell programs for the two C8c creatures whose cast-time X value is
/// preserved onto their entering battlefield incarnation for their ETB
/// triggers (Wan Shi Tong, Librarian and kicked Thieving Skydiver).
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Wan Shi Tong, Librarian" => Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_cube_c8c_v1::librarian_enter",
        },
        "Thieving Skydiver" => Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_cube_c8c_v1::skydiver_enter",
        },
        _ => return None,
    })
}

/// Thieving Skydiver: "Kicker {X}. X can't be 0."
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    match name {
        "Thieving Skydiver" => Some("{X}"),
        _ => None,
    }
}

/// No C8c card is an Aura or an Equipment.
pub(super) fn attachment_for(_name: &str) -> &'static str {
    "None"
}

/// No C8c card has flashback.
pub(super) fn flashback_for(_name: &str) -> String {
    "None".to_string()
}

/// No C8c card has an alternative cost.
pub(super) fn alt_cost_for(_name: &str) -> &'static str {
    "None"
}

/// Printed evergreen keywords of this wave's cards.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Elite Spellbinder" | "Wan Shi Tong, All-Knowing" | "Thieving Skydiver" => {
            &["Keywords::FLYING"]
        }
        "Wan Shi Tong, Librarian" => {
            &["Keywords::FLASH", "Keywords::FLYING", "Keywords::VIGILANCE"]
        }
        "Absorbing Man" => &["Keywords::VIGILANCE"],
        _ => &[],
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Pyrogoyf" => "cda:power_card_types_all_graveyards_toughness_plus_1;this_or_another_controlled_lhurgoyf_enters:that_creature_deals_power_damage_to_any_target",
        "Elite Spellbinder" => "etb:target_opponent:look_hand:may_exile_nonland:owner_may_play_as_long_as_exiled_plus_2",
        "Wan Shi Tong, All-Knowing" => "etb:target_nonland_permanent:owner_puts_second_from_top_or_bottom;one_or_more_cards_put_into_library:create_two_1_1_spirit_tokens",
        "Wan Shi Tong Spirit Token" => "static:cant_block_or_be_blocked_by_non_spirit_creatures",
        "Wan Shi Tong, Librarian" => "etb:put_x_plus1_counters_then_draw_half_x_rounded_down;opponent_searches_library:put_plus1_counter_and_draw_1",
        "Thieving Skydiver" => "kicker_x_nonzero;etb_if_kicked:gain_control_target_artifact_mv_at_most_x:if_equipment_attach_to_self",
        "Zirda, the Dawnwaker" => "companion:permanent_cards_have_activated_ability;static:nonmana_activated_abilities_cost_2_less_min_1",
        "Absorbing Man" => "beginning_precombat_main:until_next_turn_copy_up_to_one_artifact_non_aura_enchantment_or_land_as_4_4_legendary_human_villain_vigilance",
        _ => "none",
    }
}
