//! Card-database recipes for the eternal catalog's Pauper field wave Pf1b
//! (spells and library effects). Wave Pf1a's tables fall through to these, and
//! these fall through to wave Pf1c's
//! (`build_eternal_pf1c_v1.rs`). Runtime programs live in
//! `src/eternal_pauper_pf1b_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.
//!
//! Rows of this wave that live in other waves' tables, each one small and
//! additive:
//!
//! - Deprive's additional cost (return a land you control to its owner's
//!   hand) is in `build_eternal_p1a_v1.rs`'s `additional_cost_for`.
//! - Rejoinder (Elite Interceptor's prepare spell) is listed by
//!   `build_eternal_e7b_v1.rs`'s `is_prepare_spell`, since it is in no deck.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special, StackSpellFilter,
};
use AbilityCostRecipe::{ManaCost, SacrificeSelf};

/// Bitter Reunion: "{1}, Sacrifice this enchantment: Creatures you control
/// gain haste until end of turn." Instant speed, from the battlefield.
const BITTER_REUNION: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{1}"), SacrificeSelf],
    effect: AbilityEffectRecipe::Program {
        name: "bitter_reunion_haste_pf1b",
        effect: "EffectOp::BoostControlledCreaturesUntilEndOfTurn { power: 0, toughness: 0, keywords: Keywords::HASTE }",
    },
    activation_zone: "Battlefield",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Bitter Reunion" => &BITTER_REUNION,
        _ => super::build_eternal_pf1c_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function of `eternal_pauper_pf1b_v1`.
const fn program(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs. Deprive's additional cost is `additional_cost_for` in
/// wave P1a's table; Scattershot's Storm is wave E3's table.
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Deprive" => Some(Special::CounterTarget(StackSpellFilter::Any)),
        "Exclude" => Some(program(
            "TargetSpec::CreatureSpellOnStack",
            "crate::eternal_pauper_pf1b_v1::exclude",
        )),
        "Fiery Cannonade" => Some(program(
            "TargetSpec::None",
            "crate::eternal_pauper_pf1b_v1::fiery_cannonade",
        )),
        "Kruphix's Insight" => Some(program(
            "TargetSpec::None",
            "crate::eternal_pauper_pf1b_v1::kruphixs_insight",
        )),
        // Elite Interceptor's prepare spell, cast only as a copy.
        "Rejoinder" => Some(program(
            "TargetSpec::Creature",
            "crate::eternal_pauper_pf1b_v1::rejoinder",
        )),
        "Scattershot" => Some(program(
            "TargetSpec::Creature",
            "crate::eternal_pauper_pf1b_v1::scattershot",
        )),
        _ => super::build_eternal_pf1c_v1::special_for(name),
    }
}

/// No Pf1b card has kicker yet.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_pf1c_v1::kicker_cost_for(name)
}

/// No Pf1b card is an Aura or an Equipment yet.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_pf1c_v1::attachment_for(name)
}

/// No Pf1b card has flashback yet.
pub(super) fn flashback_for(_name: &str) -> String {
    "None".to_string()
}

/// No Pf1b card has an alternative cost yet.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_pf1c_v1::alt_cost_for(name)
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Militia Bugler" => &["Keywords::VIGILANCE"],
        _ => super::build_eternal_pf1c_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Bitter Reunion" => {
            "etb:may_discard:draw:2;activated:1_sacrifice_self_controlled_creatures_gain_haste_eot"
        }
        "Kruphix's Insight" => {
            "spell:reveal_top_6:up_to_3_enchantment_cards_to_hand:rest_to_graveyard"
        }
        "Scattershot" => "cast:storm:copies_may_choose_new_targets;spell:damage_1_target_creature",
        "Deprive" => "additional_cost:return_controlled_land_to_owners_hand;spell:counter_target",
        "Exclude" => "spell:counter_target_creature_spell:draw_1",
        "Fiery Cannonade" => "spell:damage_2_each_non_pirate_creature",
        "Militia Bugler" => {
            "vigilance;etb:look_top_4:may_reveal_creature_power_at_most_2_to_hand:rest_bottom_random_order"
        }
        "Mob Lookout" => "etb:target_controlled_creature:connive",
        "Elite Interceptor" => "enters_prepared:rejoinder",
        "Rejoinder" => "spell:may_tap_or_untap_target_creature:draw_1;prepare_spell_copy_only",
        _ => super::build_eternal_pf1c_v1::trigger_recipe_for(name),
    }
}
