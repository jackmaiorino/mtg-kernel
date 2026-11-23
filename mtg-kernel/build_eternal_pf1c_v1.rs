//! Card-database recipes for the eternal catalog's Pauper field wave Pf1c
//! (four cards that each need a new engine piece). Wave Pf1b's tables fall through to these, and
//! these fall through to wave C7a's (`build_eternal_c7a_v1.rs`). Runtime programs live in
//! `src/eternal_pauper_pf1c_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.
//!
//! Rows of this wave that live in other waves' tables, each one small and
//! additive:
//!
//! - Fling's additional cost (sacrifice a creature) is in
//!   `build_eternal_p1a_v1.rs`'s `additional_cost_for`.

use super::{ActivatedAbilityRecipe, Special};

/// No Pf1c card has an activated ability yet.
pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    super::build_eternal_c7a_v1::activated_ability_recipes_for(name)
}

/// A spell whose program is a runtime function of `eternal_pauper_pf1c_v1`.
const fn program(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs. Boarding Action's "Beam me up" is `flashback_for`;
/// Fling's sacrifice is `additional_cost_for` in wave P1a's table.
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Boarding Action" => Some(Special::CreateTokens {
            token: "Klingon Warrior Token",
            count: 1,
        }),
        "Fling" => Some(program(
            "TargetSpec::AnyTarget",
            "crate::eternal_pauper_pf1c_v1::fling",
        )),
        "Hallow" => Some(program(
            "TargetSpec::AnySpellOnStack",
            "crate::eternal_pauper_pf1c_v1::hallow",
        )),
        "Kaervek's Torch" => Some(program(
            "TargetSpec::AnyTarget",
            "crate::eternal_pauper_pf1c_v1::kaerveks_torch",
        )),
        _ => super::build_eternal_c7a_v1::special_for(name),
    }
}

/// No Pf1c card has kicker yet.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c7a_v1::kicker_cost_for(name)
}

/// No Pf1c card is an Aura or an Equipment yet.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c7a_v1::attachment_for(name)
}

/// Graveyard casts. Boarding Action: "Beam me up {2}{R} (You may cast this
/// card from your graveyard for {2}{R} if you also return a creature you
/// control to its owner's hand. Then exile this spell.)" It is a flashback
/// cost of mana plus Deprive's return component (the ruling: the cost is
/// paid rather than the mana cost, and the spell is exiled whenever it
/// leaves the stack).
pub(super) fn flashback_for(name: &str) -> String {
    match name {
        "Boarding Action" => {
            let (pips, generic, x_count) = super::parse_cost("{2}{R}");
            format!(
                "Some(FlashbackDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }}), CostComponent::ReturnControlledPermanentToOwnersHand(PermanentFilterDef::CreatureV1)] }})",
                pips.join(", ")
            )
        }
        _ => super::build_eternal_c7a_v1::flashback_for(name),
    }
}

/// No Pf1c card has an alternative cost yet.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c7a_v1::alt_cost_for(name)
}

/// Boarding Action's Klingon Warrior token has haste. (No Pf1c card has a
/// printed evergreen keyword.)
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Klingon Warrior Token" => &["Keywords::HASTE"],
        _ => super::build_eternal_c7a_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Boarding Action" => {
            "spell:create_1_klingon_warrior_2_1_haste;beam_me_up:2r_return_controlled_creature_to_owners_hand:exile"
        }
        "Fling" => "additional_cost:sacrifice_creature;spell:damage_any_target_sacrificed_power_lki",
        "Hallow" => "spell:target_spell:prevent_all_its_damage_this_turn:gain_life_prevented",
        "Kaervek's Torch" => {
            "static_on_stack:spells_targeting_it_cost_2_more;spell:damage_any_target_x"
        }
        _ => super::build_eternal_c7a_v1::trigger_recipe_for(name),
    }
}
