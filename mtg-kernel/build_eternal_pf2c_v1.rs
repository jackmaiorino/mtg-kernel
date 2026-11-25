//! Card-database recipes for the eternal catalog's Pauper field wave Pf2c
//! (the three cards that each introduce a new engine piece: Benevolent Unicorn,
//! Boulderbranch Golem and Inside Out). Wave Pf2b's tables fall through to
//! these, and these fall through to Premodern wave P3i
//! (`build_eternal_p3i_v1.rs`). Runtime programs and static hooks live in
//! `src/eternal_pauper_pf2c_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{ActivatedAbilityRecipe, Special};

/// No Pf2c card has an activated ability.
pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    super::build_eternal_p3i_v1::activated_ability_recipes_for(name)
}

/// Spell programs. Inside Out switches target creature's power and toughness
/// until end of turn, then draws a card.
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Inside Out" => Some(Special::StandardProgram {
            target_spec: "TargetSpec::Creature",
            program: "crate::eternal_pauper_pf2c_v1::inside_out",
        }),
        _ => super::build_eternal_p3i_v1::special_for(name),
    }
}

/// No Pf2c card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_p3i_v1::kicker_cost_for(name)
}

/// No Pf2c card is an Aura or an Equipment.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_p3i_v1::attachment_for(name)
}

/// No Pf2c card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_p3i_v1::flashback_for(name)
}

/// Boulderbranch Golem: "Prototype {3}{G} -- 3/3" (CR 718).
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Boulderbranch Golem" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Colored(ManaColor::G)], generic: 3, x_count: 0 })], condition: AltCostCondition::PrototypeV1 })",
        _ => super::build_eternal_p3i_v1::alt_cost_for(name),
    }
}

/// No Pf2c card has a printed evergreen keyword.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    super::build_eternal_p3i_v1::keywords_for(name)
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Benevolent Unicorn" => "static:replacement_spell_damage_minus_1",
        "Boulderbranch Golem" => "prototype:3g_3_3_green;etb:gain_life_equal_to_power_lki",
        "Inside Out" => "spell:switch_target_creature_pt_eot:draw_1",
        _ => super::build_eternal_p3i_v1::trigger_recipe_for(name),
    }
}
