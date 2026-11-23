//! Card-database recipes for the eternal catalog's Vintage Cube wave C7a
//! (partial promotions, deferred C6 cards and Brain Freeze Storm fix). Wave
//! Pf1c's tables fall through to these, and these fall through to wave C7b's
//! (`build_eternal_c7b_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c7a_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};

/// Uses the same loyalty timing and shared once-per-turn cost as wave C6c's
/// planeswalkers.
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

const CHANDRA: [ActivatedAbilityRecipe; 4] = [
    loyalty(
        &[AbilityCostRecipe::Loyalty(1)],
        "crate::eternal_cube_c7a_v1::chandra_plus_one_exile",
        "None",
    ),
    loyalty(
        &[AbilityCostRecipe::Loyalty(1)],
        "crate::eternal_cube_c7a_v1::chandra_plus_one_mana",
        "None",
    ),
    loyalty(
        &[AbilityCostRecipe::Loyalty(-3)],
        "crate::eternal_cube_c7a_v1::chandra_minus_three",
        "Creature",
    ),
    loyalty(
        &[AbilityCostRecipe::Loyalty(-7)],
        "crate::eternal_cube_c7a_v1::chandra_minus_seven",
        "None",
    ),
];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Chandra, Torch of Defiance" => &CHANDRA,
        _ => super::build_eternal_c7b_v1::activated_ability_recipes_for(name),
    }
}

pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Chandra, Torch of Defiance" => Some(Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_cube_c6c_v1::jace",
        }),
        _ => super::build_eternal_c7b_v1::special_for(name),
    }
}

/// No C7a card has kicker yet.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c7b_v1::kicker_cost_for(name)
}

/// No C7a card is an Aura or an Equipment yet.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c7b_v1::attachment_for(name)
}

/// No C7a card has flashback yet.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c7b_v1::flashback_for(name)
}

/// No C7a card has an alternative cost yet.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c7b_v1::alt_cost_for(name)
}

/// Gandalf, Goblins' Bane's Sorcery Adventure: Flameshape {1}{R}.
pub(super) fn adventure_for(name: &str) -> &'static str {
    match name {
        "Gandalf, Goblins' Bane" => "Some(AdventureDef { name: \"Flameshape\", cost: Cost { pips: &[Pip::Colored(ManaColor::R)], generic: 1, x_count: 0 }, types: &[CardType::Sorcery], target_spec: TargetSpec::None, effect: ability_effect_program_eternal_cube_c7a_v1_flameshape })",
        _ => "None",
    }
}

pub(super) fn adventure_effect_recipe_for(name: &str) -> Option<AbilityEffectRecipe> {
    match name {
        "Gandalf, Goblins' Bane" => Some(AbilityEffectRecipe::StandardProgram(
            "crate::eternal_cube_c7a_v1::flameshape",
        )),
        _ => None,
    }
}

pub(super) fn adventure_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Gandalf, Goblins' Bane" => Some("Flameshape"),
        _ => None,
    }
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Headliner Scarlett" => &["Keywords::HASTE"],
        "Malcolm, Alluring Scoundrel" => &["Keywords::FLASH", "Keywords::FLYING"],
        _ => super::build_eternal_c7b_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Headliner Scarlett" => "haste;etb:target_players_creatures_cannot_block_this_turn;your_upkeep:exile_top_one_face_down:controller_may_look_and_play_this_turn",
        "Gandalf, Goblins' Bane" => "cast_noncreature:source_plus1_plus1_until_end_of_turn:damage_each_opponent:1;adventure:flameshape:1r:sorcery:look_top_two:exile_face_down:may_play_while_exiled_if_you_control_wizard",
        "Inti, Seneschal of the Sun" => "controller_attacks:may_discard_a_card:when_you_do:target_attacking_creature_plus1_counter_and_trample_until_eot;controller_discards_one_or_more:impulse_top_one_until_owners_next_end_step",
        "Malcolm, Alluring Scoundrel" => "flash;flying;deals_combat_damage_to_player:chorus_counter:draw_one_then_discard_one:if_four_or_more_chorus_may_cast_discarded_free",
        "Chandra, Torch of Defiance" => "loyalty_start:4;plus1_exile:exile_top_one_may_cast_paying_mana_cost_else_2_damage_each_opponent;plus1_mana:add_rr;minus3:4_damage_target_creature;minus7:persistent_emblem_whenever_you_cast_a_spell_5_damage_any_target",
        _ => super::build_eternal_c7b_v1::trigger_recipe_for(name),
    }
}
