//! Card-database recipes for the eternal catalog's Pauper field wave Pf2a
//! (existing-mechanics permanents and spells). Wave C9c's tables fall through
//! to these, and these fall through to wave Pf2b's
//! (`build_eternal_pf2b_v1.rs`). Runtime programs live in
//! `src/eternal_pauper_pf2a_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, LibrarySearchFilterRecipe,
    Special,
};
use AbilityCostRecipe::{ManaCost, SacrificeSelf, Tap};

/// Escape Tunnel:
/// - `{T}, Sacrifice this land: Search your library for a basic land card, put
///   it onto the battlefield tapped, then shuffle.`
/// - `{T}, Sacrifice this land: Target creature with power 2 or less can't be
///   blocked this turn.`
const ESCAPE_TUNNEL: [ActivatedAbilityRecipe; 2] = [
    ActivatedAbilityRecipe {
        cost: &[Tap, SacrificeSelf],
        effect: AbilityEffectRecipe::SearchLibraryToBattlefieldTapped {
            filter: LibrarySearchFilterRecipe::BasicLand,
        },
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
    ActivatedAbilityRecipe {
        cost: &[Tap, SacrificeSelf],
        effect: AbilityEffectRecipe::StandardProgram(
            "crate::eternal_pauper_pf2a_v1::escape_tunnel_unblockable",
        ),
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec: "StandardV1(crate::standard_cards_v1::StandardTargetV1::S1dV1(crate::eternal_sideboard_s1d_v1::S1dTargetV1::CreaturePowerAtMostV1(2)))",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
];

/// Granger Guildmage:
/// - `{R}, {T}: This creature deals 1 damage to any target and 1 damage to you.`
/// - `{W}, {T}: Target creature gains first strike until end of turn.`
const GRANGER_GUILDMAGE: [ActivatedAbilityRecipe; 2] = [
    ActivatedAbilityRecipe {
        cost: &[ManaCost("{R}"), Tap],
        effect: AbilityEffectRecipe::StandardProgram(
            "crate::eternal_pauper_pf2a_v1::granger_guildmage_ping",
        ),
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec: "AnyTarget",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
    ActivatedAbilityRecipe {
        cost: &[ManaCost("{W}"), Tap],
        effect: AbilityEffectRecipe::Program {
            name: "granger_guildmage_first_strike_pf2a",
            effect: "EffectOp::GrantKeywordTargetUntilEndOfTurn { object: ObjectRef::Target(0), keyword: Keywords::FIRST_STRIKE }",
        },
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec: "Creature",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
];

/// Obsidian Acolyte: `{W}: Target creature gains protection from black until
/// end of turn.`
const OBSIDIAN_ACOLYTE: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{W}")],
    effect: AbilityEffectRecipe::StandardProgram(
        "crate::eternal_pauper_pf2a_v1::obsidian_acolyte_protection",
    ),
    activation_zone: "Battlefield",
    sorcery_speed_only: false,
    target_spec: "Creature",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Escape Tunnel" => &ESCAPE_TUNNEL,
        "Granger Guildmage" => &GRANGER_GUILDMAGE,
        "Obsidian Acolyte" => &OBSIDIAN_ACOLYTE,
        _ => super::build_eternal_pf2b_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function of `eternal_pauper_pf2a_v1`.
const fn program(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Keep Watch" => Some(program(
            "TargetSpec::None",
            "crate::eternal_pauper_pf2a_v1::keep_watch",
        )),
        "Embrace the Paradox" => Some(program(
            "TargetSpec::None",
            "crate::eternal_pauper_pf2a_v1::embrace_the_paradox",
        )),
        "Thrilling Discovery" => Some(program(
            "TargetSpec::None",
            "crate::eternal_pauper_pf2a_v1::thrilling_discovery",
        )),
        "Rites of Initiation" => Some(program(
            "TargetSpec::None",
            "crate::eternal_pauper_pf2a_v1::rites_of_initiation",
        )),
        _ => super::build_eternal_pf2b_v1::special_for(name),
    }
}

pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_pf2b_v1::kicker_cost_for(name)
}

pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_pf2b_v1::attachment_for(name)
}

pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_pf2b_v1::flashback_for(name)
}

pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_pf2b_v1::alt_cost_for(name)
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Tuktuk Rubblefort" => &["Keywords::DEFENDER", "Keywords::REACH"],
        _ => super::build_eternal_pf2b_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Radiant Grove" => "enters_tapped;basic_land_types:forest_plains",
        "Escape Tunnel" => {
            "activated:tap_sacrifice:search_basic_land_tapped;activated:tap_sacrifice:target_creature_power_at_most_2_cant_be_blocked_eot"
        }
        "Tuktuk Rubblefort" => "defender;reach;static:controlled_creatures_have_haste",
        "Granger Guildmage" => {
            "activated:r_tap:damage_1_any_target_and_1_to_you;activated:w_tap:target_creature_first_strike_eot"
        }
        "Obsidian Acolyte" => {
            "static:protection_from_black;activated:w:target_creature_protection_from_black_eot"
        }
        "Spirit Mascot" => "trigger:one_or_more_cards_leave_your_graveyard:plus1_counter_self",
        "Keep Watch" => "spell:draw_per_attacking_creature",
        "Embrace the Paradox" => "spell:draw_3:may_put_land_from_hand_battlefield_tapped",
        "Thrilling Discovery" => "spell:gain_life_2:may_discard_2_if_you_do_draw_3",
        "Rites of Initiation" => {
            "spell:discard_any_number_at_random:controlled_creatures_plus_1_0_per_discarded_eot"
        }
        _ => super::build_eternal_pf2b_v1::trigger_recipe_for(name),
    }
}
