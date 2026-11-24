//! Card-database recipes for the eternal catalog's Vintage Cube wave C7c
//! (evoke, linked exile/duration, persist, backup and Sanctum Lurker). Wave
//! C7b's tables fall through to these, and these fall through to wave C8a's
//! (`build_eternal_c8a_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c7c_v1.rs`. Every row is keyed by a name that occurs only
//! in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};

/// Sanctum Lurker: 'Planeswalkers you control have "[+2]: This planeswalker
/// deals 1 damage to each opponent and you gain 1 life."' The granted ability
/// is defined here and offered on each planeswalker its controller controls
/// (`eternal_cube_c7c_v1::granted_lurker_ability`); Sanctum Lurker itself is
/// not a planeswalker and is barred by `eternal_cube_c7c_v1::activation_allowed`.
const SANCTUM_LURKER: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[AbilityCostRecipe::Loyalty(2)],
    effect: AbilityEffectRecipe::StandardProgram("crate::eternal_cube_c7c_v1::lurker_plus_two"),
    activation_zone: "Battlefield",
    sorcery_speed_only: true,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Sanctum Lurker" => &SANCTUM_LURKER,
        _ => super::build_eternal_c8a_v1::activated_ability_recipes_for(name),
    }
}

/// All C7c cards are creature spells resolving through the default permanent
/// entry program.
pub(super) fn special_for(name: &str) -> Option<Special> {
    super::build_eternal_c8a_v1::special_for(name)
}

/// No C7c card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c8a_v1::kicker_cost_for(name)
}

/// No C7c card is an Aura or an Equipment.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c8a_v1::attachment_for(name)
}

/// No C7c card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c8a_v1::flashback_for(name)
}

/// Alternative costs: Evoke {R/G}{R/G} on Vibrance and Evoke {G/U}{G/U} on
/// Wistfulness (following Mulldrifter's mana evoke shape in wave E6a).
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Vibrance" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Hybrid(ManaColor::R, ManaColor::G), Pip::Hybrid(ManaColor::R, ManaColor::G)], generic: 0, x_count: 0 })], condition: AltCostCondition::Always })",
        "Wistfulness" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Hybrid(ManaColor::G, ManaColor::U), Pip::Hybrid(ManaColor::G, ManaColor::U)], generic: 0, x_count: 0 })], condition: AltCostCondition::Always })",
        _ => super::build_eternal_c8a_v1::alt_cost_for(name),
    }
}

pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "The Wondrous Wasp" => &["Keywords::FLASH", "Keywords::FLYING"],
        "Woodfall Primus" => &["Keywords::TRAMPLE"],
        "Guardian Scalelord" => &["Keywords::FLYING"],
        _ => super::build_eternal_c8a_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Vibrance" => "evoke:rg_rg;etb_if_rr_spent:any_target:damage:3;etb_if_gg_spent:search_land_reveal_to_hand:gain_life:2;etb_if_evoked:sacrifice_source",
        "Wistfulness" => "evoke:gu_gu;etb_if_gg_spent:target_opponent_artifact_or_enchantment:exile;etb_if_uu_spent:draw:2:discard:1;etb_if_evoked:sacrifice_source",
        "Tidehollow Sculler" => "etb:target_opponent:reveal_hand:choose_nonland:exile;leaves:return_linked_exiled_card_to_owners_hand",
        "The Wondrous Wasp" => "etb:up_to_one_target_creature:tap:loses_all_abilities_while_source_remains_on_battlefield",
        "Woodfall Primus" => "etb:target_noncreature_permanent:destroy;persist:dies_without_minus1_counters:return_under_owner_with_minus1_counter",
        "Sanctum Lurker" => "etb:empower_jace:1;static:controlled_planeswalkers_survive_zero_loyalty;static:controlled_planeswalkers_have_plus2_damage_each_opponent_1_gain_1",
        "Guardian Scalelord" => "backup:1:flying_and_attack_reanimate_until_end_of_turn;attacks:target_nonland_permanent_in_own_graveyard_mv_at_most_source_power:return_to_battlefield",
        _ => super::build_eternal_c8a_v1::trigger_recipe_for(name),
    }
}
