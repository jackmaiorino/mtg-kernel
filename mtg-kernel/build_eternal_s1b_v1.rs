//! Card-database recipes for the eternal catalog's Legacy sideboard cards,
//! part 2 (wave S1b, issue #365). E8b's tables (and E5b's modes and E7a's
//! alternative costs) fall through to these, and these fall through to
//! E9b's; runtime programs live in `src/eternal_sideboard_s1b_v1.rs`. Every
//! row is keyed by a name that occurs only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special,
};
use AbilityCostRecipe::{DiscardSelf, ManaCost};

/// A program of `eternal_sideboard_s1b_v1`.
const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Steel Wrecking Ball: "{1}{R}, Discard this card: Destroy target
/// artifact."
const STEEL_WRECKING_BALL: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{1}{R}"), DiscardSelf],
    effect: program("crate::eternal_sideboard_s1b_v1::destroy_target"),
    activation_zone: "Hand",
    sorcery_speed_only: false,
    target_spec: "ArtifactPermanent",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Steel Wrecking Ball" => &STEEL_WRECKING_BALL,
        _ => super::build_eternal_e9b_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs. Deafening Clarion's other choices are `mode2_for` and
/// `mode3_for` below.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Debt to the Deathless" => spell(
            "TargetSpec::None",
            "crate::eternal_sideboard_s1b_v1::debt_to_the_deathless",
        ),
        // Mode 0: "Deafening Clarion deals 3 damage to each creature." Mode 1:
        // lifelink. Mode 2: both ("choose one or both").
        "Deafening Clarion" => spell(
            "TargetSpec::None",
            "crate::eternal_sideboard_s1b_v1::clarion_damage",
        ),
        "Empty the Warrens" => spell(
            "TargetSpec::None",
            "crate::eternal_sideboard_s1b_v1::empty_the_warrens",
        ),
        // Storm is the cast trigger of `eternal_cards_v1::e3`.
        "Grapeshot" => Special::BurnAnyTarget(1),
        // "As this creature enters, choose a nonland card name."
        "Meddling Mage" => spell(
            "TargetSpec::None",
            "crate::eternal_sideboard_s1b_v1::meddling_mage",
        ),
        "Metallic Rebuke" => Special::CounterUnlessPaysGeneric {
            filter: super::StackSpellFilter::Any,
            generic: 3,
        },
        "Peer into the Abyss" => spell(
            "TargetSpec::AnyPlayer",
            "crate::eternal_sideboard_s1b_v1::peer_into_the_abyss",
        ),
        "Shenanigans" => spell(
            "TargetSpec::ArtifactPermanent",
            "crate::eternal_sideboard_s1b_v1::destroy_target",
        ),
        "Sudden Demise" => spell(
            "TargetSpec::None",
            "crate::eternal_sideboard_s1b_v1::sudden_demise",
        ),
        // "As an additional cost to cast this spell, pay X life": X is
        // announced like Sickening Dreams' discard count.
        "Toxic Deluge" => spell(
            "TargetSpec::None",
            "crate::eternal_sideboard_s1b_v1::toxic_deluge",
        ),
        "Whipflare" => spell("TargetSpec::None", "crate::eternal_sideboard_s1b_v1::whipflare"),
        _ => return super::build_eternal_e9b_v1::special_for(name),
    })
}

/// Sailors' Bane: "Ward {4}".
pub(super) fn ward_cost_for(name: &str) -> &'static str {
    match name {
        "Sailors' Bane" => "Some(WardCostDef::Generic(4))",
        _ => "None",
    }
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        _ => super::build_eternal_e9b_v1::keywords_for(name),
    }
}

/// Deafening Clarion's second choice.
pub(super) fn mode2_for(name: &str) -> &'static str {
    match name {
        "Deafening Clarion" => "Some(ModeDef { target_spec: TargetSpec::None, effect: crate::eternal_sideboard_s1b_v1::clarion_lifelink })",
        _ => super::build_eternal_e9b_v1::mode2_for(name),
    }
}

/// Deafening Clarion's third choice, both modes.
pub(super) fn mode3_for(name: &str) -> &'static str {
    match name {
        "Deafening Clarion" => "Some(ModeDef { target_spec: TargetSpec::None, effect: crate::eternal_sideboard_s1b_v1::clarion_both })",
        _ => super::build_eternal_e9b_v1::mode3_for(name),
    }
}

/// Metallic Rebuke: "Improvise", the convoke-style alternative payment
/// (artifacts pay generic mana), as Kappa Cannoneer's is.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Metallic Rebuke" => "Some(AltCostDef { components: &[CostComponent::ConvokeMana(Cost { pips: &[Pip::Colored(ManaColor::U)], generic: 2, x_count: 0 })], condition: AltCostCondition::Always })",
        _ => super::build_eternal_e7b_v1::alt_cost_for(name),
    }
}

/// Card-database tokens for the triggered abilities in
/// `eternal_sideboard_s1b_v1::triggers_for`, its hooks and its statics.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Deafening Silence" => "static:each_player_cant_cast_more_than_one_noncreature_spell_each_turn",
        "Gaddock Teeg" => "static:noncreature_spells_mana_value_4_or_more_cant_be_cast;static:noncreature_spells_with_x_cant_be_cast",
        "Meddling Mage" => "as_enters:choose_nonland_card_name;static:spells_with_chosen_name_cant_be_cast",
        "Sailors' Bane" => "static:costs_1_less_per_instant_sorcery_adventure_card_in_graveyard_and_exile;ward:4",
        "Toxic Deluge" => "additional_cost:pay_x_life;spell:all_creatures_get_minus_x_minus_x_eot",
        "Debt to the Deathless" => "spell:each_opponent_loses_two_x_life:you_gain_life_lost",
        "Deafening Clarion" => "modes:one_or_both:damage_3_each_creature,controlled_creatures_lifelink_eot",
        "Empty the Warrens" => "cast:storm:copies;spell:create_two_goblin_tokens",
        "Grapeshot" => "cast:storm:copies_may_choose_new_targets;spell:damage_1_any_target",
        "Metallic Rebuke" => "improvise;spell:counter_target_spell_unless_pays_3",
        "Paradox Engine" => "controller_casts_spell:untap_all_controlled_nonland_permanents",
        "Peer into the Abyss" => "spell:target_player_draws_half_library_rounded_up:loses_half_life_rounded_up",
        "Shenanigans" => "spell:destroy_target_artifact;dredge:1",
        "Steel Wrecking Ball" => "etb:damage_5_target_creature;hand_activated:destroy_target_artifact",
        "Stony Silence" => "static:artifact_activated_abilities_cant_be_activated",
        "Sudden Demise" => "spell:choose_color:x_damage_each_creature_of_color",
        "Temporary Lockdown" => "etb:exile_each_nonland_permanent_mana_value_2_or_less_until_source_leaves",
        "Whipflare" => "spell:damage_2_each_nonartifact_creature",
        _ => super::build_eternal_e9b_v1::trigger_recipe_for(name),
    }
}
