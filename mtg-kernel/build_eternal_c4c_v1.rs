//! Card-database recipes for the eternal catalog's Vintage Cube wave C4c
//! (Equipment and Vehicles). Wave C4b's tables fall through to these, and
//! these fall through to wave C5a's (`build_eternal_c5a_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c4c_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.
//!
//! Rows of this wave that live in other waves' tables, each one small and
//! additive:
//!
//! - The `EquipmentDef` of every Equipment (its fixed statistics and keywords)
//!   is in wave E9a's `equipment_for`.
//! - The Green Cat token's object name is `build.rs`'s `object_name_for`, and
//!   Iron Man Armor's `AnimationDef` is in
//!   `build_support/standard_lands_v1.rs`.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{Crew, ManaCost, RemoveChargeCounterFromSelf};

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

/// A program of `eternal_cube_c4c_v1` (or of an earlier wave).
const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// "Equip {cost}" (702.6a): attach to target creature you control, only as a
/// sorcery (Shuko's and Lightning Greaves's recipe).
const fn equip(cost: &'static [AbilityCostRecipe]) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        sorcery_speed_only: true,
        ..instant(
            cost,
            AbilityEffectRecipe::AttachSourceToTarget,
            "ControlledCreature",
        )
    }
}

/// Skullclamp: "Equip {1}".
const SKULLCLAMP: [ActivatedAbilityRecipe; 1] = [equip(&[ManaCost("{1}")])];

/// Sword of the Meek: "Equip {2}".
const SWORD_OF_THE_MEEK: [ActivatedAbilityRecipe; 1] = [equip(&[ManaCost("{2}")])];

/// Umezawa's Jitte: "Remove a charge counter from Umezawa's Jitte: Choose
/// one", as three abilities that each remove a counter: "Equipped creature
/// gets +2/+2 until end of turn.", "Target creature gets -1/-1 until end of
/// turn." and "You gain 2 life.", then "Equip {2}".
const UMEZAWAS_JITTE: [ActivatedAbilityRecipe; 4] = [
    instant(
        &[RemoveChargeCounterFromSelf],
        program("crate::eternal_cube_c4c_v1::jitte_boost"),
        "None",
    ),
    instant(
        &[RemoveChargeCounterFromSelf],
        AbilityEffectRecipe::PumpTargetUntilEndOfTurn {
            power: -1,
            toughness: -1,
        },
        "Creature",
    ),
    instant(
        &[RemoveChargeCounterFromSelf],
        AbilityEffectRecipe::GainLife(2),
        "None",
    ),
    equip(&[ManaCost("{2}")]),
];

/// Nettlecyst: "Equip {2}".
const NETTLECYST: [ActivatedAbilityRecipe; 1] = [equip(&[ManaCost("{2}")])];

/// Kaldra Compleat: "Equip {7}".
const KALDRA_COMPLEAT: [ActivatedAbilityRecipe; 1] = [equip(&[ManaCost("{7}")])];

/// Glimmer Lens: "Equip {1}{W}".
const GLIMMER_LENS: [ActivatedAbilityRecipe; 1] = [equip(&[ManaCost("{1}{W}")])];

/// Iron Man Armor: "{2}: If this Equipment isn't a creature, it becomes a 0/0
/// Construct Hero artifact creature with flying and 'This creature gets +1/+1
/// for each artifact you control' until end of turn" (Mishra's Factory's
/// animation, behind its own program that does nothing while the Armor is
/// already a creature), then "Equip {2}" (its own program: the Armor may be a
/// creature that names itself).
const IRON_MAN_ARMOR: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[ManaCost("{2}")],
        program("crate::eternal_cube_c4c_v1::iron_man_animate"),
        "None",
    ),
    ActivatedAbilityRecipe {
        sorcery_speed_only: true,
        ..instant(
            &[ManaCost("{2}")],
            program("crate::eternal_cube_c4c_v1::iron_man_equip"),
            "ControlledCreature",
        )
    },
];

/// Astrologian's Planisphere: "Diana -- Equip {2}".
const ASTROLOGIANS_PLANISPHERE: [ActivatedAbilityRecipe; 1] = [equip(&[ManaCost("{2}")])];

/// Smuggler's Copter: "Crew 1" (Reckoner Bankbuster's and Shorikai's crew
/// ability).
const SMUGGLERS_COPTER: [ActivatedAbilityRecipe; 1] = [instant(
    &[Crew(1)],
    program("crate::standard_cards_v1::crew_animation"),
    "None",
)];

/// Esika's Chariot: "Crew 4".
const ESIKAS_CHARIOT: [ActivatedAbilityRecipe; 1] = [instant(
    &[Crew(4)],
    program("crate::standard_cards_v1::crew_animation"),
    "None",
)];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Skullclamp" => &SKULLCLAMP,
        "Sword of the Meek" => &SWORD_OF_THE_MEEK,
        "Umezawa's Jitte" => &UMEZAWAS_JITTE,
        "Nettlecyst" => &NETTLECYST,
        "Kaldra Compleat" => &KALDRA_COMPLEAT,
        "Glimmer Lens" => &GLIMMER_LENS,
        "Iron Man Armor" => &IRON_MAN_ARMOR,
        "Astrologian's Planisphere" => &ASTROLOGIANS_PLANISPHERE,
        "Smuggler's Copter" => &SMUGGLERS_COPTER,
        "Esika's Chariot" => &ESIKAS_CHARIOT,
        _ => super::build_eternal_c5a_v1::activated_ability_recipes_for(name),
    }
}

/// No C4c spell program: every card of this wave is a permanent.
pub(super) fn special_for(name: &str) -> Option<Special> {
    super::build_eternal_c5a_v1::special_for(name)
}

/// No C4c card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c5a_v1::kicker_cost_for(name)
}

/// No C4c card is an Aura (the Equipment profiles are wave E9a's
/// `equipment_for`).
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c5a_v1::attachment_for(name)
}

/// No C4c card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c5a_v1::flashback_for(name)
}

/// No C4c card has an alternative cost.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c5a_v1::alt_cost_for(name)
}

/// Printed evergreen keywords: Smuggler's Copter flies.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Smuggler's Copter" => &["Keywords::FLYING"],
        _ => super::build_eternal_c5a_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Skullclamp" => {
            "equipment:plus_1_minus_1;trigger:equipped_creature_dies_draw_2;equip:1"
        }
        "Sword of the Meek" => {
            "equipment:plus_1_plus_2;equip:2;graveyard_trigger:1_1_creature_you_control_enters_may_return_self_then_attach"
        }
        "Umezawa's Jitte" => {
            "trigger:equipped_creature_deals_combat_damage_2_charge_counters;activated:remove_charge_equipped_creature_plus_2_plus_2_until_end_of_turn;activated:remove_charge_target_creature_minus_1_minus_1_until_end_of_turn;activated:remove_charge_gain_2_life;equip:2"
        }
        "Nettlecyst" => {
            "living_weapon:phyrexian_germ_token;equipment:plus_1_plus_1_per_artifact_or_enchantment_you_control;equip:2"
        }
        "Kaldra Compleat" => {
            "living_weapon:phyrexian_germ_token;equipment:self_indestructible;equipment:plus_5_plus_5_first_strike_trample_indestructible_haste;granted_trigger:combat_damage_to_creature_exile_it;equip:7"
        }
        "Glimmer Lens" => {
            "for_mirrodin:rebel_token;trigger:equipped_creature_and_another_attack_draw_1;equip:1w"
        }
        "Iron Man Armor" => {
            "etb:attach_to_target_controlled_creature;equipment:plus_2_plus_1_flying;activated:2_becomes_0_0_construct_hero_artifact_creature_flying_plus_1_plus_1_per_artifact_until_end_of_turn;equip:2"
        }
        "Astrologian's Planisphere" => {
            "job_select:create_hero_token_attach_self;equipment:wizard;granted_trigger:cast_noncreature_spell_or_draw_third_card_each_turn_plus_1_plus_1_counter_on_creature;equip:2"
        }
        "Smuggler's Copter" => {
            "vehicle:3_3;flying;attacks_or_blocks:may_draw_1_then_discard_1;crew:1"
        }
        "Esika's Chariot" => {
            "vehicle:4_4;etb:create_two_green_cat_tokens;attacks:create_token_copy_of_target_token_you_control;crew:4"
        }
        _ => super::build_eternal_c5a_v1::trigger_recipe_for(name),
    }
}
