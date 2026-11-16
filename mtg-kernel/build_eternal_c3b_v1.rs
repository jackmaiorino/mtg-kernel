//! Card-database recipes for the eternal catalog's Vintage Cube wave C3b
//! (creatures with small new pieces, and exalted). Wave C3a's tables fall through to these, and
//! these fall through to wave C3c's
//! (`build_eternal_c3c_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c3b_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, LibrarySearchFilterRecipe,
    Special,
};
use AbilityCostRecipe::{DiscardSelf, ManaCost};

/// Oliphaunt: "Mountaincycling {1}" (702.29e): activated from hand,
/// discarding the card as part of its cost, to search for a card with the
/// Mountain subtype and put it into hand (Eagles of the North's shape).
const OLIPHAUNT: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{1}"), DiscardSelf],
    effect: AbilityEffectRecipe::SearchLibraryToHand {
        filter: LibrarySearchFilterRecipe::LandWithSubtype("Mountain"),
        min_targets: 0,
        max_targets: 1,
        reveal_selected: true,
        shuffle: true,
    },
    activation_zone: "Hand",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Marwyn, the Preserver: "{2}: Return target land card from your graveyard
/// to your hand."
const MARWYN: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{2}")],
    effect: AbilityEffectRecipe::StandardProgram("crate::eternal_cube_c3b_v1::marwyn_return"),
    activation_zone: "Battlefield",
    sorcery_speed_only: false,
    target_spec: "StandardV1(crate::standard_cards_v1::StandardTargetV1::LandCardInOwnGraveyardV1)",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Walking Ballista: "{4}: Put a +1/+1 counter on this creature." and
/// "Remove a +1/+1 counter from this creature: It deals 1 damage to any
/// target." (Triskelion's cost and effect).
const WALKING_BALLISTA: [ActivatedAbilityRecipe; 2] = [
    ActivatedAbilityRecipe {
        cost: &[ManaCost("{4}")],
        effect: AbilityEffectRecipe::PutPlusOneCounterOnSource,
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
    ActivatedAbilityRecipe {
        cost: &[AbilityCostRecipe::RemovePlusOneCounterFromSelf],
        effect: AbilityEffectRecipe::DealDamageAnyTarget(1),
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec: "AnyTarget",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Oliphaunt" => &OLIPHAUNT,
        "Marwyn, the Preserver" => &MARWYN,
        "Walking Ballista" => &WALKING_BALLISTA,
        _ => super::build_eternal_c3c_v1::activated_ability_recipes_for(name),
    }
}

/// Walking Ballista: "This creature enters with X +1/+1 counters on it."
/// (Stonecoil Serpent's program; its cost is `{X}{X}`, two X symbols.)
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Walking Ballista" => Some(Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_cube_c3b_v1::walking_ballista",
        }),
        _ => super::build_eternal_c3c_v1::special_for(name),
    }
}

/// No C3b card has kicker yet.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c3c_v1::kicker_cost_for(name)
}

/// No C3b card is an Aura or an Equipment yet.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c3c_v1::attachment_for(name)
}

/// No C3b card has flashback yet.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c3c_v1::flashback_for(name)
}

/// No C3b card has an alternative cost yet.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c3c_v1::alt_cost_for(name)
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Oliphaunt" => &["Keywords::TRAMPLE"],
        "Stingcaster Mage" => &["Keywords::HASTE"],
        "Spider-Woman, Stunning Savior" => &["Keywords::FLYING"],
        _ => super::build_eternal_c3c_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Oliphaunt" => {
            "attack:another_target_controlled_creature:plus_2_plus_0_and_trample_until_end_of_turn;cycling:mountaincycling:1"
        }
        "Stingcaster Mage" => {
            "etb:target_instant_or_sorcery_card_in_own_graveyard:gains_flashback_equal_to_mana_cost_until_end_of_turn"
        }
        "Third Path Iconoclast" => {
            "cast_noncreature_spell:create_soldier_artifact_token_1_1_colorless"
        }
        "Tireless Tracker" => "landfall:investigate;sacrifice_clue:plus_one_counter_on_source",
        "Samut, Hazoret's Champion" => "static:controlled_creatures_have_haste",
        "Marwyn, the Preserver" => {
            "static:controlled_lands_have_hexproof;activated:2:return_target_land_card_from_own_graveyard_to_hand"
        }
        "Spellseeker" => {
            "etb:may_search_library:instant_or_sorcery_mana_value_at_most:2:reveal_to_hand:shuffle:fail_to_find_allowed"
        }
        "Walking Ballista" => {
            "enters_with_x_counters:x_x_cost;activated:4:plus_one_counter_on_source;activated:remove_plus_one_counter:damage_1_any_target"
        }
        "Noble Hierarch" => "exalted;mana:tap_add_one_of_white_blue_green",
        "Ignoble Hierarch" => "exalted;mana:tap_add_one_of_black_red_green",
        "Spider-Woman, Stunning Savior" => "static:opponent_artifacts_and_creatures_enter_tapped",
        _ => super::build_eternal_c3c_v1::trigger_recipe_for(name),
    }
}
