//! Card-database recipes for the eternal catalog's Premodern sideboard wave
//! P3a. This module is the tail of the recipe chains: wave P2b's tables fall
//! through to these, as do the kicker table of wave E9a and the attachment
//! table of wave E7a, and its own defaults are the chains' base values.
//! Runtime programs live in `src/eternal_premodern_p3a_v1.rs`. Every row is
//! keyed by a name that occurs only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};

/// Crimson Acolyte: "{W}: Target creature gains protection from red until
/// end of turn." Its printed protection from red is
/// `eternal_premodern_p3a_v1::statics::protection_mask`.
const CRIMSON_ACOLYTE: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[AbilityCostRecipe::ManaCost("{W}")],
    effect: AbilityEffectRecipe::StandardProgram(
        "crate::eternal_premodern_p3a_v1::crimson_acolyte",
    ),
    activation_zone: "Battlefield",
    sorcery_speed_only: false,
    target_spec: "Creature",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Crimson Acolyte" => &CRIMSON_ACOLYTE,
        _ => &[],
    }
}

/// A spell whose program is a runtime function of `eternal_premodern_p3a_v1`.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs. Essence Flare is an enchant creature Aura; its static
/// is `attachment_for` below.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Anarchy" => spell(
            "TargetSpec::None",
            "crate::eternal_premodern_p3a_v1::anarchy",
        ),
        "Essence Flare" => Special::BindTheMonster,
        // Its kicker is `kicker_cost_for` below.
        "Overload" => spell(
            "TargetSpec::ArtifactPermanent",
            "crate::eternal_premodern_p3a_v1::overload",
        ),
        "Tranquil Domain" => spell(
            "TargetSpec::None",
            "crate::eternal_premodern_p3a_v1::tranquil_domain",
        ),
        _ => return None,
    })
}

/// Overload: "Kicker {2}".
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    match name {
        "Overload" => Some("{2}"),
        _ => None,
    }
}

/// Essence Flare: "Enchanted creature gets +2/+0."
pub(super) fn attachment_for(name: &str) -> &'static str {
    match name {
        "Essence Flare" => "Some(AttachmentDef::AuraCreatureStatic(AuraCreatureStaticDef { power: 2, toughness: 0, keywords: Keywords::NONE, per_controlled_subtype: None }))",
        _ => "None",
    }
}

/// Card-database tokens for the triggered abilities and statics in
/// `eternal_premodern_p3a_v1`.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Anarchy" => "spell:destroy_all_white_permanents",
        "Chill" => "static:red_spells_cost_2_more_by_cast_form_color",
        "Crimson Acolyte" => {
            "static:protection_from_red;activated:w:target_creature_protection_from_red_until_end_of_turn"
        }
        "Cursed Totem" => "static:activated_abilities_of_creatures_cant_be_activated",
        "Essence Flare" => {
            "aura:enchant_creature;static:plus_2_plus_0;upkeep_of_enchanted_creature_controller:minus_0_minus_1_counter_on_that_creature"
        }
        "Overload" => "kicker:2;spell:destroy_target_artifact_mana_value_at_most_2_or_5_if_kicked",
        "Pyrostatic Pillar" => "any_player_casts_spell_mana_value_at_most_3:2_damage_to_that_player",
        "Sphere of Law" => "static:prevent_2_damage_from_each_red_source_to_you",
        "Tranquil Domain" => "spell:destroy_all_non_aura_enchantments",
        "True Believer" => "static:you_have_shroud",
        "Warmth" => "opponent_casts_red_spell:gain_2_life",
        _ => "none",
    }
}
