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

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Jace, the Mind Sculptor" => &JACE,
        _ => &[],
    }
}

pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Jace, the Mind Sculptor" => Some(Special::StandardProgram {
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
        _ => "none",
    }
}
