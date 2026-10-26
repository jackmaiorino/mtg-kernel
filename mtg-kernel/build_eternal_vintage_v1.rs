//! Card-database recipes for the eternal catalog's Vintage cards (wave E4:
//! Jewel Shops and Mono-White Initiative). `build_eternal_v1` falls through
//! to these tables; runtime programs live in `src/eternal_vintage_v1.rs`.
//! Every row is keyed by a name that occurs only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{SacrificeSelf, Tap};

/// Strip Mine: "{T}, Sacrifice this land: Destroy target land." Its
/// `{T}: Add {C}` is the ordinary mana ability.
const STRIP_MINE: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[Tap, SacrificeSelf],
    effect: AbilityEffectRecipe::DestroyTarget,
    activation_zone: "Battlefield",
    sorcery_speed_only: false,
    target_spec: "Land",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Karakas: "{T}: Return target legendary creature to its owner's hand."
/// Its `{T}: Add {W}` is the ordinary mana ability.
const KARAKAS: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[Tap],
    effect: AbilityEffectRecipe::MoveAllTargetsToHand,
    activation_zone: "Battlefield",
    sorcery_speed_only: false,
    target_spec: "LegendaryCreature",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Strip Mine" => &STRIP_MINE,
        "Karakas" => &KARAKAS,
        _ => &[],
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Ancestral Recall" => Special::TargetPlayerDraw { draw: 3 },
        // The extra-turn scheduler owns "Take an extra turn after this one"
        // for Temporal Manipulation; Time Walk is admitted beside it.
        "Time Walk" => Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::extra_turn_v1::recipe",
        },
        _ => return None,
    })
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Archon of Emeria" | "Chancellor of the Annex" | "Clarion Conqueror" => {
            &["Keywords::FLYING"]
        }
        "Anointed Peacekeeper" => &["Keywords::VIGILANCE"],
        _ => &[],
    }
}

/// Card-database tokens for the triggered abilities in
/// `eternal_vintage_v1::triggers_for`.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Mana Crypt" => "upkeep:controller_only:flip_coin:lost:source_damages_controller:3",
        _ => "none",
    }
}

/// Single printed mana abilities that are not "tap: add one".
pub(super) fn mana_ability_def_for(name: &str) -> &'static str {
    match name {
        // "{T}, Sacrifice this artifact: Add three mana of any one color."
        "Black Lotus" => "Some(ManaAbilityDef { cost: ManaAbilityCostDef::TapAndSacrificeSelf, amount: ManaAbilityAmountDef::Fixed(3), controller_damage: 0, max_activations_per_turn: None })",
        _ => "None",
    }
}

/// Tap-for-colorless rocks and Tolarian Academy: the amount one activation
/// of the primary mana ability adds. The payment planner and the explicit
/// mana action read the same value.
pub(super) fn conditional_tap_yield_for(name: &str) -> &'static str {
    match name {
        // "{T}: Add {C}{C}."
        "Sol Ring" | "Mana Crypt" => "Some(DynamicValueDef::Fixed(2))",
        // "{T}: Add {U} for each artifact you control."
        "Tolarian Academy" => {
            "Some(DynamicValueDef::ControlledPermanentsWithType(CardType::Artifact))"
        }
        _ => "None",
    }
}

pub(super) fn conditional_tap_yield_program_for(name: &str) -> Option<&'static str> {
    match name {
        "Sol Ring" => Some("mana_ability_add_sol_ring"),
        "Mana Crypt" => Some("mana_ability_add_mana_crypt"),
        "Tolarian Academy" => Some("mana_ability_add_tolarian_academy"),
        _ => None,
    }
}
