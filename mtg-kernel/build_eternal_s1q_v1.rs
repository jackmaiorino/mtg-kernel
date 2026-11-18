//! Card-database recipes for the eternal catalog's sideboard wave S1q. Wave
//! S1o's tables fall through to these, and these fall through to Vintage Cube
//! wave C1a's (`build_eternal_c1a_v1.rs`); the mana-ability table is reached
//! from the tail of wave P2b's and the Saga table from the tail of the Vintage
//! table's. Runtime programs, triggers and static hooks live in
//! `src/eternal_sideboard_s1q_v1.rs`. Every row is keyed by a name that
//! occurs only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, PermanentFilterRecipe, Special,
};
use AbilityCostRecipe::{ManaCost, TapControlled};
use PermanentFilterRecipe::ArtifactOrCreature;

/// Cityscape Leveler: "Unearth {8}" (Phyrexian Dragon Engine's recipe).
const CITYSCAPE_LEVELER: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{8}")],
    effect: AbilityEffectRecipe::Unearth,
    activation_zone: "Graveyard",
    sorcery_speed_only: true,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// One payment of Invasion Submersible's "Exhaust - Waterbend {3}: This
/// Vehicle becomes an artifact creature. Put three +1/+1 counters on it."
/// Waterbend lets each generic mana of the {3} be paid by tapping an untapped
/// artifact or creature the activator controls; `taps` of them pay that many
/// and `mana` pays the rest. The four payments are one exhaust ability: they
/// share its once-per-object record (`eternal_sideboard_s1q_v1`).
const fn waterbend(cost: &'static [AbilityCostRecipe]) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect: AbilityEffectRecipe::StandardProgram(
            "crate::eternal_sideboard_s1q_v1::submersible_exhaust",
        ),
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

/// Invasion Submersible's exhaust ability with zero to three waterbend taps.
const INVASION_SUBMERSIBLE: [ActivatedAbilityRecipe; 4] = [
    waterbend(&[ManaCost("{3}")]),
    waterbend(&[
        ManaCost("{2}"),
        TapControlled {
            count: 1,
            filter: ArtifactOrCreature,
        },
    ]),
    waterbend(&[
        ManaCost("{1}"),
        TapControlled {
            count: 2,
            filter: ArtifactOrCreature,
        },
    ]),
    waterbend(&[
        ManaCost("{0}"),
        TapControlled {
            count: 3,
            filter: ArtifactOrCreature,
        },
    ]),
];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Cityscape Leveler" => &CITYSCAPE_LEVELER,
        "Invasion Submersible" => &INVASION_SUBMERSIBLE,
        _ => super::build_eternal_c1a_v1::activated_ability_recipes_for(name),
    }
}

/// No S1q card is an instant or a sorcery.
pub(super) fn special_for(name: &str) -> Option<Special> {
    super::build_eternal_c1a_v1::special_for(name)
}

/// No S1q card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c1a_v1::kicker_cost_for(name)
}

/// No S1q card is an Aura or an Equipment.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c1a_v1::attachment_for(name)
}

/// No S1q card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c1a_v1::flashback_for(name)
}

/// No S1q card has an alternative cost.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c1a_v1::alt_cost_for(name)
}

/// Printed evergreen keywords on S1q permanents.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Cityscape Leveler" => &["Keywords::TRAMPLE"],
        "Summon: Bahamut" => &["Keywords::FLYING"],
        _ => super::build_eternal_c1a_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Cityscape Leveler" => {
            "cast_self_and_attacks:destroy_up_to_one_target_nonland_permanent_its_controller_creates_tapped_powerstone_token_even_if_not_destroyed;unearth:8;unearth_next_end_step:exile"
        }
        "Invasion Submersible" => {
            "etb:return_up_to_one_other_target_nonland_permanent_to_owners_hand;exhaust_once_per_object:waterbend_3_tap_artifacts_or_creatures_per_generic_up_to_reduced_total:becomes_artifact_creature_no_duration_put_three_plus_one_counters"
        }
        "Summon: Bahamut" => {
            "saga_creature:4;chapter_1_2:destroy_up_to_one_target_nonland_permanent;chapter_3:draw_2;chapter_4:source_damage_each_opponent_total_mana_value_other_controlled_permanents_on_resolution"
        }
        "Powerstone Token" => "mana:c_not_for_nonartifact_spells",
        _ => super::build_eternal_c1a_v1::trigger_recipe_for(name),
    }
}

/// Summon: Bahamut's four chapters.
pub(super) fn saga_for(name: &str) -> &'static str {
    match name {
        "Summon: Bahamut" => "Some(SagaDef { chapter_effects: &[crate::eternal_sideboard_s1q_v1::bahamut_chapter_one, crate::eternal_sideboard_s1q_v1::bahamut_chapter_two, crate::eternal_sideboard_s1q_v1::bahamut_chapter_three, crate::eternal_sideboard_s1q_v1::bahamut_chapter_four] })",
        _ => "None",
    }
}

/// The Powerstone token's "{T}: Add {C}." It is a mana ability definition, as
/// The Mightstone and Weakstone's is, so the automatic spell planner never
/// taps it; the runtime turns the {C} into a restricted unit
/// (`eternal_sideboard_s1q_v1::restrict_produced_mana`).
pub(super) fn mana_ability_def_for(name: &str) -> &'static str {
    match name {
        "Powerstone Token" => "Some(ManaAbilityDef { cost: ManaAbilityCostDef::TapSelf, amount: ManaAbilityAmountDef::Fixed(1), controller_damage: 0, max_activations_per_turn: None })",
        _ => "None",
    }
}
