//! Card-database recipes for the eternal catalog's Vintage Cube wave C5b
//! (token makers and mana creatures). Wave C5a's tables fall through to these, and
//! these fall through to wave C5c's
//! (`build_eternal_c5c_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c5b_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.
//!
//! Rows of this wave that live in other waves' tables, each one small and
//! additive:
//!
//! - The Moloid, Wurm and Elemental tokens' object names are `build.rs`'s
//!   `object_name_for`.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{ManaCost, TapOtherUntappedControlledPermanentWithSubtype};

/// Otharri, Suns' Glory: "{2}{R}{W}, Tap an untapped Rebel you control:
/// Return this card from your graveyard to the battlefield tapped"
/// (Reassembling Skeleton's graveyard return with Heap Gate's tap-another
/// cost).
const OTHARRI: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[
        ManaCost("{2}{R}{W}"),
        TapOtherUntappedControlledPermanentWithSubtype("Rebel"),
    ],
    effect: AbilityEffectRecipe::ReturnSourceFromGraveyard { tapped: true },
    activation_zone: "Graveyard",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Otharri, Suns' Glory" => &OTHARRI,
        _ => super::build_eternal_c5c_v1::activated_ability_recipes_for(name),
    }
}

/// No C5b spell program: every card of this wave is a permanent.
pub(super) fn special_for(name: &str) -> Option<Special> {
    super::build_eternal_c5c_v1::special_for(name)
}

/// No C5b card has kicker yet.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c5c_v1::kicker_cost_for(name)
}

/// No C5b card is an Aura or an Equipment yet.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c5c_v1::attachment_for(name)
}

/// No C5b card has flashback yet.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c5c_v1::flashback_for(name)
}

/// No C5b card has an alternative cost yet.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c5c_v1::alt_cost_for(name)
}

/// Printed evergreen keywords: Worldspine Wurm and its Wurm tokens trample;
/// Otharri has flying, lifelink and haste.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Worldspine Wurm" | "Green Wurm Token" => &["Keywords::TRAMPLE"],
        "Otharri, Suns' Glory" => &["Keywords::FLYING", "Keywords::LIFELINK", "Keywords::HASTE"],
        _ => super::build_eternal_c5c_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Worldspine Wurm" => {
            "dies:create_three_green_wurm_5_5_trample_tokens;put_into_graveyard_from_anywhere:shuffle_self_into_owners_library"
        }
        "Myr Battlesphere" => {
            "etb:create_four_myr_1_1_artifact_creature_tokens;attacks:may_tap_x_untapped_myr:plus_x_0_and_x_damage_to_attacked_player_or_planeswalker"
        }
        "Mole Man, Moloid Master" => {
            "static:play_lands_from_graveyard;landfall:create_moloid_1_1_green_minion_token"
        }
        "Moloid Token" => "attacks:may_mill_1",
        "Torsten, Founder of Benalia" => {
            "etb:reveal_top7:any_number_creature_or_land_to_hand:rest_bottom_random;dies:create_seven_white_soldier_1_1_tokens"
        }
        "Titania, Protector of Argoth" => {
            "etb:return_target_land_card_from_graveyard_to_battlefield;controlled_land_put_into_graveyard_from_battlefield:create_green_elemental_5_3_token"
        }
        "Otharri, Suns' Glory" => {
            "attacks:experience_counter_then_rebel_tokens_tapped_attacking_per_counter;graveyard:2rw_tap_untapped_rebel_return_self_tapped"
        }
        _ => super::build_eternal_c5c_v1::trigger_recipe_for(name),
    }
}
