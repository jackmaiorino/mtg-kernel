//! Card-database recipes for the eternal catalog's Vintage Cube wave C6b
//! (discard counts, dies copies and graveyard plays). Wave C6a's tables fall through to these, and
//! these fall through to wave C6c's
//! (`build_eternal_c6c_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c6b_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};

const CRYOMANCER: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[
        AbilityCostRecipe::ManaCost("{3}{U}{U}"),
        AbilityCostRecipe::ExileSelf,
    ],
    effect: AbilityEffectRecipe::StandardProgram("crate::eternal_cube_c6b_v1::draw_two"),
    activation_zone: "Graveyard",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

const PYROMANCER: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[
        AbilityCostRecipe::ManaCost("{3}{R}{R}"),
        AbilityCostRecipe::ExileSelf,
    ],
    effect: AbilityEffectRecipe::StandardProgram("crate::eternal_cube_c6b_v1::two_elementals"),
    activation_zone: "Graveyard",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Graveyard activations use Bramble Wurm's exile-self cost.
pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Seasoned Cryomancer" => &CRYOMANCER,
        "Seasoned Pyromancer" => &PYROMANCER,
        _ => super::build_eternal_c6c_v1::activated_ability_recipes_for(name),
    }
}

/// No C6b spell program yet.
pub(super) fn special_for(name: &str) -> Option<Special> {
    super::build_eternal_c6c_v1::special_for(name)
}

/// No C6b card has kicker yet.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c6c_v1::kicker_cost_for(name)
}

/// No C6b card is an Aura or an Equipment yet.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c6c_v1::attachment_for(name)
}

/// No C6b card has flashback yet.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c6c_v1::flashback_for(name)
}

/// No C6b card has an alternative cost yet.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c6c_v1::alt_cost_for(name)
}

/// Printed evergreen keywords.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Sphinx of False Conclusions" => &["Keywords::FLASH", "Keywords::FLYING"],
        "Metamorphosis Fanatic" => &["Keywords::LIFELINK"],
        "Night Nurse, Healer of Heroes" => &["Keywords::FLASH", "Keywords::LIFELINK"],
        "Six" => &["Keywords::REACH"],
        "Ursine Monstrosity" => &["Keywords::TRAMPLE"],
        _ => super::build_eternal_c6c_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Seasoned Cryomancer" => "etb:draw_two_then_discard_two;reflexive:nonlands_discarded_this_way:tap_up_to_that_many_target_creatures:one_stun_counter_each;graveyard_ability:3uu:exile_self:draw_two",
        "Seasoned Pyromancer" => "etb:discard_two_then_draw_two:create_one_1_1_red_elemental_per_nonland_discarded_this_way;graveyard_ability:3rr:exile_self:create_two_1_1_red_elementals",
        "Sphinx of False Conclusions" => "flash;flying;attacks:draw_one_then_discard_one;dies:if_nontoken:copy_source_original_battlefield_departure_copiable_values:captured_trigger_controller:replacement_batch",
        "Metamorphosis Fanatic" => "lifelink;etb:return_up_to_one_target_creature_card_from_own_graveyard_to_battlefield:lifelink_counter_on_returned_incarnation;miracle:1b:first_draw_reveal_choice:trigger_pay_then_cast",
        "Night Nurse, Healer of Heroes" => "flash;lifelink;etb:return_target_permanent_card_in_own_graveyard_put_there_this_turn:exact_incarnation:to_hand",
        "Six" => "reach;attacks:mill_three:may_put_one_land_from_among_them_into_hand;own_turn:nonland_permanent_cards_in_own_graveyard:retrace:additional_discard_land_cost",
        "Ursine Monstrosity" => "trample;beginning_of_controller_combat:mill_one:choose_random_opponent:source_attacks_that_player_this_combat_if_able:until_eot_indestructible_and_plus_one_plus_one_per_card_type_in_own_graveyard_at_resolution",
        _ => super::build_eternal_c6c_v1::trigger_recipe_for(name),
    }
}
