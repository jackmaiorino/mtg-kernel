//! Card-database recipes for the eternal catalog's Premodern wave P2d:
//! White Weenie and Mono-Red Control. They sit between wave P1b's tables and
//! wave P1a's: every fall-through that reached `build_eternal_p1a_v1` reaches
//! these first, and unknown names continue to P1a. Runtime programs live in
//! `src/eternal_premodern_p2d_v1.rs`. Every row is keyed by a name that
//! occurs only in the eternal registry.

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

/// A program of `eternal_premodern_p2d_v1`.
const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Devoted Caretaker: "{W}, {T}: Target permanent you control gains
/// protection from instant spells and from sorcery spells until end of
/// turn."
const DEVOTED_CARETAKER: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{W}"), Tap],
    program("crate::eternal_premodern_p2d_v1::devoted_caretaker"),
    "StandardV1(crate::standard_cards_v1::StandardTargetV1::ControlledPermanentV1)",
)];

/// Ghitu Encampment: "{1}{R}: This land becomes a 2/1 red Warrior creature
/// with first strike until end of turn. It's still a land." The animation is
/// `standard_lands::fields`; "{T}: Add {R}." is the ordinary mana ability.
const GHITU_ENCAMPMENT: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}{R}")],
    AbilityEffectRecipe::AnimateSource,
    "None",
)];

/// Mother of Runes: "{T}: Target creature you control gains protection from
/// the color of your choice until end of turn."
const MOTHER_OF_RUNES: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap],
    program("crate::eternal_premodern_p2d_v1::mother_of_runes"),
    "ControlledCreature",
)];

/// Nova Cleric: "{2}{W}, {T}, Sacrifice this creature: Destroy all
/// enchantments."
const NOVA_CLERIC: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{2}{W}"), Tap, SacrificeSelf],
    program("crate::eternal_premodern_p2d_v1::nova_cleric"),
    "None",
)];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Devoted Caretaker" => &DEVOTED_CARETAKER,
        "Ghitu Encampment" => &GHITU_ENCAMPMENT,
        "Mother of Runes" => &MOTHER_OF_RUNES,
        "Nova Cleric" => &NOVA_CLERIC,
        _ => super::build_eternal_p1a_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function of `eternal_premodern_p2d_v1`.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        // Its X is the {X} of its mana cost.
        "Earthquake" => spell(
            "TargetSpec::None",
            "crate::eternal_premodern_p2d_v1::earthquake",
        ),
        // Its flashback is `flashback_for` below.
        "Firebolt" => Special::BurnAnyTarget(2),
        "Pillage" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::ArtifactOrLandV1)",
            "crate::eternal_premodern_p2d_v1::pillage",
        ),
        "Stone Rain" => Special::DestroyLand,
        _ => return super::build_eternal_p1a_v1::special_for(name),
    })
}

/// Printed evergreen keywords, in printed order. Shadow and protection are
/// `eternal_premodern_p2d_v1::statics`.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Avalanche Riders" => &["Keywords::HASTE"],
        _ => super::build_eternal_p1a_v1::keywords_for(name),
    }
}

pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_p1a_v1::alt_cost_for(name)
}

pub(super) fn additional_cost_for(name: &str) -> &'static str {
    super::build_eternal_p1a_v1::additional_cost_for(name)
}

/// Firebolt: "Flashback {4}{R}".
pub(super) fn flashback_for(name: &str) -> String {
    match name {
        "Firebolt" => {
            let (pips, generic, x_count) = super::parse_cost("{4}{R}");
            format!(
                "Some(FlashbackDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }})] }})",
                pips.join(", ")
            )
        }
        _ => super::build_eternal_p1a_v1::flashback_for(name),
    }
}

pub(super) fn mana_ability_def_for(name: &str) -> &'static str {
    super::build_eternal_p1a_v1::mana_ability_def_for(name)
}

pub(super) fn adventure_for(name: &str) -> &'static str {
    super::build_eternal_p1a_v1::adventure_for(name)
}

pub(super) fn adventure_effect_recipe_for(name: &str) -> Option<AbilityEffectRecipe> {
    super::build_eternal_p1a_v1::adventure_effect_recipe_for(name)
}

pub(super) fn adventure_face_name_for(name: &str) -> Option<&'static str> {
    super::build_eternal_p1a_v1::adventure_face_name_for(name)
}

pub(super) fn mode2_for(name: &str) -> &'static str {
    super::build_eternal_p1a_v1::mode2_for(name)
}

pub(super) fn mode3_for(name: &str) -> &'static str {
    super::build_eternal_p1a_v1::mode3_for(name)
}

/// Card-database tokens for the triggered abilities and statics in
/// `eternal_premodern_p2d_v1`.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Avalanche Riders" => {
            "haste;echo:3r:upkeep_if_entered_since_last_upkeep;etb:destroy_target_land"
        }
        "Beloved Chaplain" => "protection_from_creatures",
        "Devoted Caretaker" => {
            "activated:w_tap:target_controlled_permanent_protection_from_instants_and_sorceries"
        }
        "Earthquake" => "spell:x_damage_each_creature_without_flying_and_each_player",
        "Firebolt" => "spell:damage_2_any_target;flashback:4r",
        "Ghitu Encampment" => "enters_tapped;mana:r;activated:animate_2_1_red_warrior_first_strike",
        "Glowrider" => "static:noncreature_spells_cost_1_more",
        "Mother of Runes" => "activated:tap:target_controlled_creature_protection_from_chosen_color",
        "Nova Cleric" => "activated:2w_tap_sacrifice_self:destroy_all_enchantments",
        "Pillage" => "spell:destroy_target_artifact_or_land:cant_be_regenerated",
        "Shared Triumph" => "as_enters:choose_creature_type;static:chosen_type_creatures_plus_1_1",
        "Soltari Monk" => "protection_from_black;shadow",
        "Soltari Priest" => "protection_from_red;shadow",
        "Stone Rain" => "spell:destroy_target_land",
        "Veteran Brawlers" => {
            "static:cant_attack_if_defender_controls_untapped_land;cant_block_if_controller_controls_untapped_land"
        }
        _ => super::build_eternal_p1a_v1::trigger_recipe_for(name),
    }
}
