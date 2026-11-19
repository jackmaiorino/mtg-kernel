//! Card-database recipes for the eternal catalog's Vintage Cube wave C5a
//! (statics, mana and counter-state creatures). Wave C4c's tables fall through to these, and
//! these fall through to wave C5b's
//! (`build_eternal_c5b_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c5a_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.
//!
//! Rows of this wave that live in other waves' tables, each one small and
//! additive:
//!
//! - Delighted Halfling's restricted mana ability and the ferocious ability's
//!   activation condition of Fanatic of Rhonas and its token are in
//!   `build_support/standard_lands_v1.rs`'s `fields` (Plaza of Heroes' rows).
//! - The ferocious ability itself (an additional mana ability) is in wave
//!   E6a's `additional_mana_abilities_for` (Relic of Sauron's rows).
//! - The object names of the Baloth Beast, Gingerbrute and Eternalized
//!   Fanatic of Rhonas tokens are `build.rs`'s `object_name_for`.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, PermanentFilterRecipe, Special,
};
use AbilityCostRecipe::{ExileSelf, ManaCost, SacrificeControlled, SacrificeSelf, Tap};

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

/// A program of `eternal_cube_c5a_v1` (or of an earlier wave).
const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Fanatic of Rhonas: "Eternalize {2}{G}{G}" ({2}{G}{G}, exile this card from
/// your graveyard: create a token that's a copy of it, except it's a 4/4 black
/// Zombie Snake Druid with no mana cost; only as a sorcery). Sacred Cat's embalm
/// recipe, with the token's own row.
const FANATIC_OF_RHONAS: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{2}{G}{G}"), ExileSelf],
    effect: AbilityEffectRecipe::CreateToken("Fanatic of Rhonas Eternalized Token"),
    activation_zone: "Graveyard",
    sorcery_speed_only: true,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Keen-Eyed Curator: "{1}: Exile target card from a graveyard." (Ghost
/// Vacuum's first ability, as a mana cost.)
const KEEN_EYED_CURATOR: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}")],
    program("crate::eternal_cube_c5a_v1::curator_exile"),
    "StandardV1(crate::standard_cards_v1::StandardTargetV1::CardInAGraveyard)",
)];

/// Baloth Prime: "{4}, Sacrifice a land: You gain 2 life." (Edge of Autumn's
/// sacrifice-a-land cost.)
const BALOTH_PRIME: [ActivatedAbilityRecipe; 1] = [instant(
    &[
        ManaCost("{4}"),
        SacrificeControlled {
            count: 1,
            filter: PermanentFilterRecipe::Land,
        },
    ],
    AbilityEffectRecipe::GainLife(2),
    "None",
)];

/// Alpha Deathclaw: "{5}{B}{G}: Monstrosity 4."
const ALPHA_DEATHCLAW: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{5}{B}{G}")],
    program("crate::eternal_cube_c5a_v1::monstrosity_4"),
    "None",
)];

/// Ginger, Queen of Sweets: "{2}, {T}, Sacrifice Ginger: You gain 6 life."
const GINGER: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{2}"), Tap, SacrificeSelf],
    AbilityEffectRecipe::GainLife(6),
    "None",
)];

/// The Gingerbrute token: "{1}: This token can't be blocked this turn except by
/// creatures with haste." and "{2}, {T}, Sacrifice this token: You gain 3
/// life." (the Standard card Gingerbrute's two abilities).
const GINGERBRUTE_TOKEN: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[ManaCost("{1}")],
        AbilityEffectRecipe::CreatureUpgrade("GingerEvasion"),
        "None",
    ),
    instant(
        &[ManaCost("{2}"), Tap, SacrificeSelf],
        AbilityEffectRecipe::GainLife(3),
        "None",
    ),
];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Fanatic of Rhonas" => &FANATIC_OF_RHONAS,
        "Keen-Eyed Curator" => &KEEN_EYED_CURATOR,
        "Baloth Prime" => &BALOTH_PRIME,
        "Alpha Deathclaw" => &ALPHA_DEATHCLAW,
        "Ginger, Queen of Sweets" => &GINGER,
        "Gingerbrute Token" => &GINGERBRUTE_TOKEN,
        _ => super::build_eternal_c5b_v1::activated_ability_recipes_for(name),
    }
}

/// No C5a spell program: every card of this wave is a permanent.
pub(super) fn special_for(name: &str) -> Option<Special> {
    super::build_eternal_c5b_v1::special_for(name)
}

/// No C5a card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c5b_v1::kicker_cost_for(name)
}

/// No C5a card is an Aura or an Equipment.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c5b_v1::attachment_for(name)
}

/// No C5a card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c5b_v1::flashback_for(name)
}

/// No C5a card has an alternative cost.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c5b_v1::alt_cost_for(name)
}

/// Printed evergreen keywords: Alpha Deathclaw has menace and trample, the
/// Gingerbrute token haste.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Alpha Deathclaw" => &["Keywords::MENACE", "Keywords::TRAMPLE"],
        "Gingerbrute Token" => &["Keywords::HASTE"],
        _ => super::build_eternal_c5b_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Delighted Halfling" => {
            "mana:tap_add_c;mana:tap_add_any_color_only_for_legendary_spells_that_spell_cant_be_countered"
        }
        "Fanatic of Rhonas" => {
            "mana:tap_add_g;mana:ferocious_tap_add_gggg_only_with_power_4_or_greater_creature;eternalize:2gg_exile_self_create_4_4_black_zombie_snake_druid_token_copy"
        }
        "Fanatic of Rhonas Eternalized Token" => {
            "copy:fanatic_of_rhonas_4_4_black_zombie_snake_druid;mana:tap_add_g;mana:ferocious_tap_add_gggg_only_with_power_4_or_greater_creature"
        }
        "Carnage Interpreter" => {
            "etb:discard_hand_then_investigate_4;static:one_or_fewer_cards_in_hand_plus_2_plus_2_menace"
        }
        "Keen-Eyed Curator" => {
            "activated:1_exile_target_card_from_a_graveyard_linked;static:four_or_more_card_types_among_exiled_with_plus_4_plus_4_trample"
        }
        "Baloth Prime" => {
            "enters_tapped_with_6_stun_counters;trigger:sacrifice_land_create_tapped_4_4_green_beast_token_untap_self;activated:4_sacrifice_land_gain_2_life"
        }
        "Baloth Beast Token" => "token:4_4_green_beast",
        "Alpha Deathclaw" => {
            "menace;trample;etb_or_becomes_monstrous:destroy_target_permanent;activated:5bg_monstrosity_4"
        }
        "Ginger, Queen of Sweets" => {
            "etb:become_the_monarch;activated:2_tap_sacrifice_self_gain_6_life;each_upkeep_if_monarch:create_gingerbrute_token"
        }
        "Gingerbrute Token" => {
            "token:1_1_food_golem_artifact_creature;haste;activated:1_cant_be_blocked_except_by_haste_creatures_this_turn;activated:2_tap_sacrifice_self_gain_3_life"
        }
        _ => super::build_eternal_c5b_v1::trigger_recipe_for(name),
    }
}
