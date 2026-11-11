//! Card-database recipes for the eternal catalog's Premodern sideboard wave
//! P3c. Wave P3b's tables fall through to these, and these fall through to
//! wave P3d's (`build_eternal_p3d_v1.rs`).
//! Runtime programs live in `src/eternal_premodern_p3c_v1.rs`. Every row is
//! keyed by a name that occurs only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{ManaCost, RemovePlusOneCounterFromSelf, SacrificeSelf, Tap};

/// An activated ability from the battlefield, at instant speed.
const fn instant(
    cost: &'static [AbilityCostRecipe],
    effect: AbilityEffectRecipe,
    target_spec: &'static str,
) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect,
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec,
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

/// Druid Lyrist: "{G}, {T}, Sacrifice this creature: Destroy target
/// enchantment."
const DRUID_LYRIST: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{G}"), Tap, SacrificeSelf],
    AbilityEffectRecipe::DestroyTarget,
    "EnchantmentPermanent",
)];

/// Spike Feeder: "{2}, Remove a +1/+1 counter from Spike Feeder: Put a
/// +1/+1 counter on target creature." and "Remove a +1/+1 counter from
/// Spike Feeder: You gain 2 life." Its two entry counters are
/// `eternal_premodern_p2c_v1::statics::apply_entry_counters`.
const SPIKE_FEEDER: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[ManaCost("{2}"), RemovePlusOneCounterFromSelf],
        AbilityEffectRecipe::AddPlusOnePlusOneCounters(1),
        "Creature",
    ),
    instant(
        &[RemovePlusOneCounterFromSelf],
        AbilityEffectRecipe::GainLife(2),
        "None",
    ),
];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Druid Lyrist" => &DRUID_LYRIST,
        "Spike Feeder" => &SPIKE_FEEDER,
        _ => super::build_eternal_p3d_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function of
/// `eternal_premodern_p3c_v1`.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Crumble" => spell(
            "TargetSpec::ArtifactPermanent",
            "crate::eternal_premodern_p3c_v1::crumble",
        ),
        // "With mana value X or less" is the cast target hook
        // `eternal_premodern_p3c_v1::statics::x_bounded_creature_target`.
        "Dominate" => spell(
            "TargetSpec::Creature",
            "crate::eternal_premodern_p3c_v1::dominate",
        ),
        _ => return super::build_eternal_p3d_v1::special_for(name),
    })
}

/// No P3c card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_p3d_v1::kicker_cost_for(name)
}

/// No P3c card is an Aura or Equipment.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_p3d_v1::attachment_for(name)
}

/// No P3c card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_p3d_v1::flashback_for(name)
}

/// No P3c card has an alternative cost.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_p3d_v1::alt_cost_for(name)
}

/// Printed evergreen keywords. Ihsan's Shade's and Iridescent Angel's
/// protection is `eternal_premodern_p3c_v1::statics::protection_mask`.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Iridescent Angel" => &["Keywords::FLYING"],
        "Multani, Maro-Sorcerer" => &["Keywords::SHROUD"],
        _ => super::build_eternal_p3d_v1::keywords_for(name),
    }
}

/// Card-database tokens for the triggered abilities and statics in
/// `eternal_premodern_p3c_v1`.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Crumble" => {
            "spell:destroy_target_artifact_cant_be_regenerated:its_controller_gains_life_mana_value"
        }
        "Dominate" => "spell:gain_control_of_target_creature_mana_value_x_or_less",
        "Druid Lyrist" => "activated:g_tap_sacrifice:destroy_target_enchantment",
        "Ihsan's Shade" => "protection_from_white",
        "Iridescent Angel" => "flying;protection_from_all_colors",
        "Multani, Maro-Sorcerer" => "shroud;cda:power_toughness_cards_in_all_hands",
        "Spike Feeder" => {
            "enters_with_2_plus_one_counters;activated:2_remove_counter:plus_one_counter_on_target_creature;activated:remove_counter:gain_2"
        }
        "Uktabi Orangutan" => "etb:destroy_target_artifact",
        _ => super::build_eternal_p3d_v1::trigger_recipe_for(name),
    }
}
