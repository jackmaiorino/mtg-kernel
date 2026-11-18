//! Card-database recipes for the eternal catalog's sideboard wave S1o. Wave
//! S1n's tables fall through to these, and these fall through to sideboard
//! wave S1q's (`build_eternal_s1q_v1.rs`); the transform-face tables are
//! reached from the tail of wave E9a's. Runtime programs, triggers and
//! static hooks live in `src/eternal_sideboard_s1o_v1.rs`. Every row is keyed
//! by a name that occurs only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{ManaCost, SacrificeSelf, Tap};

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

/// Outland Liberator and Frenzied Trapbreaker: "{1}, Sacrifice this
/// creature: Destroy target artifact or enchantment." Both faces print it,
/// so the ability has no face restriction (Cathar Commando's recipe).
const OUTLAND_LIBERATOR: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}"), SacrificeSelf],
    AbilityEffectRecipe::DestroyTarget,
    "ArtifactOrEnchantmentPermanent",
)];

/// Unlicensed Hearse: "{T}: Exile up to two target cards from a single
/// graveyard." and "Crew 2" (Reckoner Bankbuster's crew ability).
const UNLICENSED_HEARSE: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[Tap],
        AbilityEffectRecipe::StandardProgram("crate::eternal_sideboard_s1o_v1::hearse_exile"),
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::S1oV1(crate::eternal_sideboard_s1o_v1::targets::S1oTargetV1::UpToTwoCardsFromOneGraveyardV1))",
    ),
    instant(
        &[AbilityCostRecipe::Crew(2)],
        AbilityEffectRecipe::StandardProgram("crate::standard_cards_v1::crew_animation"),
        "None",
    ),
];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Outland Liberator" => &OUTLAND_LIBERATOR,
        "Unlicensed Hearse" => &UNLICENSED_HEARSE,
        _ => super::build_eternal_s1q_v1::activated_ability_recipes_for(name),
    }
}

/// Stronghold Gambit.
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Stronghold Gambit" => Some(Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_sideboard_s1o_v1::stronghold_gambit",
        }),
        _ => super::build_eternal_s1q_v1::special_for(name),
    }
}

/// No S1o card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_s1q_v1::kicker_cost_for(name)
}

/// No S1o card is an Aura or an Equipment.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_s1q_v1::attachment_for(name)
}

/// No S1o card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_s1q_v1::flashback_for(name)
}

/// No S1o card has an alternative cost.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_s1q_v1::alt_cost_for(name)
}

/// Printed evergreen keywords on S1o permanents.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Eowyn, Fearless Knight" => &["Keywords::HASTE"],
        _ => super::build_eternal_s1q_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Stronghold Gambit" => {
            "spell:each_player_chooses_hand_card_apnap_private:reveal_all:lowest_mana_value_creature_cards_enter_simultaneously_under_owners:pre_entry_containment_priest:noncast_metamorph_copy_menu_refused"
        }
        "Eowyn, Fearless Knight" => {
            "etb:exile_target_opponent_creature_power_greater_than_source_current_or_last_known;controlled_legendary_creatures_protection_from_each_color_of_exiled_creature_last_known_until_end_of_turn"
        }
        "Outland Liberator" => {
            "activated:1_sacrifice_self_destroy_target_artifact_or_enchantment;daybound;back_face_nightbound_attacks:destroy_target_artifact_or_enchantment_defending_player_controls"
        }
        "Unlicensed Hearse" => {
            "vehicle:cda_power_toughness_cards_exiled_with_this_incarnation;crew:2;activated:tap_exile_up_to_two_target_cards_from_single_graveyard"
        }
        _ => super::build_eternal_s1q_v1::trigger_recipe_for(name),
    }
}

/// Outland Liberator's back face, Frenzied Trapbreaker.
pub(super) fn transform_face_for(name: &str) -> &'static str {
    match name {
        "Outland Liberator" => "Some(TransformFaceDef { name: \"Frenzied Trapbreaker\", types: &[CardType::Creature], subtypes: &[Subtype::Werewolf], colors: &[ManaColor::G], power: Some(3), toughness: Some(3), keywords: Keywords::NONE })",
        _ => "None",
    }
}

pub(super) fn transform_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Outland Liberator" => Some("Frenzied Trapbreaker"),
        _ => None,
    }
}
