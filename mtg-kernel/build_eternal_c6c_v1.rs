//! Card-database recipes for the eternal catalog's Vintage Cube wave C6c
//! (eight planeswalkers). Wave C6b's tables fall through to these, and
//! this module is the tail of the recipe chains: its own
//! defaults are the chains' base values. Runtime programs live in
//! `src/eternal_cube_c6c_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};

/// Uses the same loyalty timing and shared once-per-turn cost as the
/// abilities of Jace, Wielder of Mysteries.
const fn loyalty(
    cost: &'static [AbilityCostRecipe],
    program: &'static str,
    target_spec: &'static str,
) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect: AbilityEffectRecipe::StandardProgram(program),
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec,
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

const JACE: [ActivatedAbilityRecipe; 4] = [
    loyalty(
        &[AbilityCostRecipe::Loyalty(2)],
        "crate::eternal_cube_c6c_v1::jace_plus_two",
        "AnyPlayer",
    ),
    loyalty(
        &[AbilityCostRecipe::Loyalty(0)],
        "crate::eternal_cube_c6c_v1::jace_zero",
        "None",
    ),
    loyalty(
        &[AbilityCostRecipe::Loyalty(-1)],
        "crate::eternal_cube_c6c_v1::jace_minus_one",
        "Creature",
    ),
    loyalty(
        &[AbilityCostRecipe::Loyalty(-12)],
        "crate::eternal_cube_c6c_v1::jace_minus_twelve",
        "AnyPlayer",
    ),
];

const NARSET: [ActivatedAbilityRecipe; 1] = [loyalty(
    &[AbilityCostRecipe::Loyalty(-2)],
    "crate::eternal_cube_c6c_v1::narset_minus_two",
    "None",
)];

const KARN: [ActivatedAbilityRecipe; 3] = [
    loyalty(
        &[AbilityCostRecipe::Loyalty(1)],
        "crate::eternal_cube_c6c_v1::karn_plus_one",
        "None",
    ),
    loyalty(
        &[AbilityCostRecipe::Loyalty(-1)],
        "crate::eternal_cube_c6c_v1::karn_minus_one",
        "None",
    ),
    loyalty(
        &[AbilityCostRecipe::Loyalty(-2)],
        "crate::eternal_cube_c6c_v1::karn_minus_two",
        "None",
    ),
];

const TEFERI: [ActivatedAbilityRecipe; 3] = [
    loyalty(
        &[AbilityCostRecipe::Loyalty(1)],
        "crate::eternal_cube_c6c_v1::teferi_plus_one",
        "None",
    ),
    loyalty(
        &[AbilityCostRecipe::Loyalty(-3)],
        "crate::eternal_cube_c6c_v1::teferi_minus_three",
        "NonlandPermanent",
    ),
    loyalty(
        &[AbilityCostRecipe::Loyalty(-8)],
        "crate::eternal_cube_c6c_v1::teferi_minus_eight",
        "None",
    ),
];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Jace, the Mind Sculptor" => &JACE,
        "Narset, Parter of Veils" => &NARSET,
        "Karn, Scion of Urza" => &KARN,
        "Teferi, Hero of Dominaria" => &TEFERI,
        _ => &[],
    }
}

pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Jace, the Mind Sculptor"
        | "Narset, Parter of Veils"
        | "Karn, Scion of Urza"
        | "Teferi, Hero of Dominaria" => Some(Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_cube_c6c_v1::jace",
        }),
        _ => None,
    }
}

/// No C6c card has kicker yet.
pub(super) fn kicker_cost_for(_name: &str) -> Option<&'static str> {
    None
}

/// No C6c card is an Aura or an Equipment yet.
pub(super) fn attachment_for(_name: &str) -> &'static str {
    "None"
}

/// No C6c card has flashback yet.
pub(super) fn flashback_for(_name: &str) -> String {
    "None".to_string()
}

/// No C6c card has an alternative cost yet.
pub(super) fn alt_cost_for(_name: &str) -> &'static str {
    "None"
}

/// No C6c card has a printed evergreen keyword yet.
pub(super) fn keywords_for(_name: &str) -> &'static [&'static str] {
    &[]
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Jace, the Mind Sculptor" => "loyalty_start:3;plus2:controller_privately_looks_at_target_player_library_top_may_bottom;zero:draw3_then_put2_hand_cards_on_library_top_in_any_order;minus1:return_target_creature_to_owner_hand;minus12:exile_target_player_library_simultaneously_then_shuffle_their_hand_into_library",
        "Narset, Parter of Veils" => "loyalty_start:5;static:opponents_cannot_draw_more_than_one_card_each_turn;minus2:look_top4_may_reveal_noncreature_nonland_to_hand_rest_bottom_random",
        "Karn, Scion of Urza" => "loyalty_start:5;plus1:reveal_top2_opponent_chooses_one_to_hand_exile_other_with_silver_counter;minus1:owned_silver_counter_card_from_exile_to_hand;minus2:create_0_0_colorless_construct_artifact_creature_plus1_plus1_per_controlled_artifact",
        "Teferi, Hero of Dominaria" => "loyalty_start:4;plus1:draw1_then_next_end_step_untap_up_to_two_lands;minus3:target_nonland_permanent_owner_library_third_from_top;minus8:persistent_emblem_whenever_you_draw_exile_target_opponent_permanent",
        _ => "none",
    }
}
