//! Card-database recipes for the eternal catalog's Pauper field wave Pf2b
//! (existing-mechanics cards using cycling, replicate, graveyard triggers,
//! changeling, suspend, adventure and omen). Wave Pf2a's tables fall through
//! to these, and these fall through to wave Pf2c's
//! (`build_eternal_pf2c_v1.rs`). Runtime programs live in
//! `src/eternal_pauper_pf2b_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{DiscardSelf, ManaCost, SacrificeSelf};

/// A cycling ability (702.29): activated from hand, discarding the card as
/// part of its cost, to draw a card.
const fn cycling(cost: &'static [AbilityCostRecipe]) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect: AbilityEffectRecipe::DrawCards(1),
        activation_zone: "Hand",
        sorcery_speed_only: false,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

/// Memory Leak: "Cycling {1}".
const MEMORY_LEAK: [ActivatedAbilityRecipe; 1] = [cycling(&[ManaCost("{1}"), DiscardSelf])];

/// Aurora Eidolon: "{W}, Sacrifice this creature: Prevent the next 3 damage
/// that would be dealt to any target this turn."
const AURORA_EIDOLON: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{W}"), SacrificeSelf],
    effect: AbilityEffectRecipe::StandardProgram(
        "crate::eternal_pauper_pf2b_v1::aurora_eidolon_prevent",
    ),
    activation_zone: "Battlefield",
    sorcery_speed_only: false,
    target_spec: "AnyTarget",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Sandstorm Eidolon: "{R}, Sacrifice this creature: Target creature can't
/// block this turn."
const SANDSTORM_EIDOLON: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{R}"), SacrificeSelf],
    effect: AbilityEffectRecipe::StandardProgram("crate::eternal_cube_c8c_v1::zirda_cant_block"),
    activation_zone: "Battlefield",
    sorcery_speed_only: false,
    target_spec: "Creature",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Rift Sower: "Suspend 2--{G}" (702.62a): a special action from the hand any
/// time its owner could cast it, performed without the stack
/// (`eternal_legacy_e7a_v1::suspend`). Its `{T}: Add one mana of any color` is
/// generated from `produces_mana` in `cards_v1.json`.
const RIFT_SOWER: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{G}")],
    effect: AbilityEffectRecipe::StandardProgram("crate::eternal_legacy_e7a_v1::suspend_action"),
    activation_zone: "Hand",
    sorcery_speed_only: true,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Stormshriek Feral: "{1}{R}: This creature gets +1/+0 until end of turn."
const STORMSHRIEK_FERAL: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{1}{R}")],
    effect: AbilityEffectRecipe::PumpSourceUntilEndOfTurn {
        power: 1,
        toughness: 0,
    },
    activation_zone: "Battlefield",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Memory Leak" => &MEMORY_LEAK,
        "Aurora Eidolon" => &AURORA_EIDOLON,
        "Sandstorm Eidolon" => &SANDSTORM_EIDOLON,
        "Rift Sower" => &RIFT_SOWER,
        "Stormshriek Feral" => &STORMSHRIEK_FERAL,
        _ => super::build_eternal_pf2c_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function of `eternal_pauper_pf2b_v1`.
const fn program(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs. Gigadrowse's Replicate is `eternal_cards_v1::replicate_cost`.
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Memory Leak" => Some(program(
            "TargetSpec::Opponent",
            "crate::eternal_pauper_pf2b_v1::memory_leak",
        )),
        "Gigadrowse" => Some(program(
            "TargetSpec::AnyPermanent",
            "crate::eternal_pauper_pf2b_v1::gigadrowse",
        )),
        _ => super::build_eternal_pf2c_v1::special_for(name),
    }
}

pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_pf2c_v1::kicker_cost_for(name)
}

pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_pf2c_v1::attachment_for(name)
}

pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_pf2c_v1::flashback_for(name)
}

pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_pf2c_v1::alt_cost_for(name)
}

/// Pegasus Guardian's Instant Adventure: Rescue the Foal `{1}{W}`.
pub(super) fn adventure_for(name: &str) -> &'static str {
    match name {
        "Pegasus Guardian" => "Some(AdventureDef { name: \"Rescue the Foal\", cost: Cost { pips: &[Pip::Colored(ManaColor::W)], generic: 1, x_count: 0 }, types: &[CardType::Instant], target_spec: TargetSpec::ControlledCreature, effect: ability_effect_program_eternal_pauper_pf2b_v1_rescue_the_foal })",
        _ => "None",
    }
}

pub(super) fn adventure_effect_recipe_for(name: &str) -> Option<AbilityEffectRecipe> {
    match name {
        "Pegasus Guardian" => Some(AbilityEffectRecipe::StandardProgram(
            "crate::eternal_pauper_pf2b_v1::rescue_the_foal",
        )),
        _ => None,
    }
}

pub(super) fn adventure_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Pegasus Guardian" => Some("Rescue the Foal"),
        _ => None,
    }
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Rooftop Percher" | "Pegasus Guardian" | "Pegasus Token" => &["Keywords::FLYING"],
        "Stormshriek Feral" => &["Keywords::FLYING", "Keywords::HASTE"],
        _ => super::build_eternal_pf2c_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Memory Leak" => {
            "spell:target_opponent_reveals_hand_choose_nonland_from_hand_or_gy_exile;cycling:1"
        }
        "Gigadrowse" => "replicate:{U}:announced_as_x;spell:tap_target_permanent",
        "Aurora Eidolon" => {
            "activated:w_sacrifice_self:prevent_next_3_damage_any_target_eot;graveyard_trigger:cast_multicolored_spell:may_return_to_hand"
        }
        "Sandstorm Eidolon" => {
            "activated:r_sacrifice_self:target_creature_cant_block_eot;graveyard_trigger:cast_multicolored_spell:may_return_to_hand"
        }
        "Rooftop Percher" => {
            "changeling;flying;etb:exile_up_to_two_target_cards_from_graveyards:gain_life_3"
        }
        "Rift Sower" => "mana:any_color;suspend:2:G",
        "Pegasus Guardian" => {
            "flying;your_end_step_if_controlled_permanent_left_battlefield_this_turn:create_1_1_white_pegasus_flying;adventure:rescue_the_foal:1w:instant:flicker_target_controlled_creature_under_owners_control"
        }
        "Pegasus Token" => "flying",
        "Stormshriek Feral" => {
            "flying;haste;activated:1r:pump_self_1_0_eot;omen:flush_out:1r:sorcery:discard_1_if_you_do_draw_2_shuffle_into_library"
        }
        _ => super::build_eternal_pf2c_v1::trigger_recipe_for(name),
    }
}
