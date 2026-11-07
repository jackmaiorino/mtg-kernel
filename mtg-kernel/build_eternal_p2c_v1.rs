//! Card-database recipes for the eternal catalog's Premodern wave P2c
//! (Reanimator and Full English Breakfast cards). `build.rs` asks this
//! table first; every other name falls through to `build_eternal_v1`.
//! Runtime programs live in `src/eternal_premodern_p2c_v1.rs`. Every row is
//! keyed by a name that occurs only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{DiscardSelf, ManaCost};

const fn ability(
    cost: &'static [AbilityCostRecipe],
    effect: AbilityEffectRecipe,
    activation_zone: &'static str,
    target_spec: &'static str,
) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect,
        activation_zone,
        sorcery_speed_only: false,
        target_spec,
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

/// Palinchron: "{2}{U}{U}: Return Palinchron to its owner's hand."
const PALINCHRON: [ActivatedAbilityRecipe; 1] = [ability(
    &[ManaCost("{2}{U}{U}")],
    AbilityEffectRecipe::StandardProgram("crate::eternal_premodern_p2c_v1::return_source_to_hand"),
    "Battlefield",
    "None",
)];

/// Triskelion: "Remove a +1/+1 counter from Triskelion: Triskelion deals 1
/// damage to any target."
const TRISKELION: [ActivatedAbilityRecipe; 1] = [ability(
    &[AbilityCostRecipe::RemovePlusOneCounterFromSelf],
    AbilityEffectRecipe::DealDamageAnyTarget(1),
    "Battlefield",
    "AnyTarget",
)];

/// Unearth: "Cycling {2}".
const UNEARTH: [ActivatedAbilityRecipe; 1] = [ability(
    &[ManaCost("{2}"), DiscardSelf],
    AbilityEffectRecipe::DrawCards(1),
    "Hand",
    "None",
)];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Palinchron" => &PALINCHRON,
        "Triskelion" => &TRISKELION,
        "Unearth" => &UNEARTH,
        _ => super::build_eternal_v1::activated_ability_recipes_for(name),
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Careful Study" => Some(Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_premodern_p2c_v1::careful_study",
        }),
        "Unearth" => Some(Special::StandardProgram {
            target_spec: "TargetSpec::CreatureCardInOwnGraveyardManaValueAtMost(3)",
            program: "crate::eternal_premodern_p2c_v1::unearth",
        }),
        _ => super::build_eternal_v1::special_for(name),
    }
}

/// Printed evergreen keywords, in printed order. Akroma's protection from
/// black and from red is `eternal_premodern_p2c_v1::statics::protection_mask`.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Akroma, Angel of Wrath" => &[
            "Keywords::FLYING",
            "Keywords::FIRST_STRIKE",
            "Keywords::VIGILANCE",
            "Keywords::TRAMPLE",
            "Keywords::HASTE",
        ],
        "Palinchron" => &["Keywords::FLYING"],
        _ => super::build_eternal_v1::keywords_for(name),
    }
}

/// Card-database tokens for the triggered abilities in
/// `eternal_premodern_p2c_v1::triggers_for` and its statics.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Akroma, Angel of Wrath" => "protection_from_black_and_red",
        "Careful Study" => "spell:draw_2_discard_2",
        "Palinchron" => "etb:untap_up_to_7_lands;activated:2uu:return_self_to_hand",
        "Treva's Ruins" => "etb:sacrifice_unless_return_non_lair_land;mana:g_w_u",
        "Triskelion" => {
            "enters_with_3_plus_one_counters;activated:remove_counter:damage_1_any_target"
        }
        "Unearth" => {
            "spell:target_creature_card_mv_3_or_less_own_graveyard_to_battlefield;cycling:2"
        }
        "Verdant Force" => "each_upkeep:create_saproling_token",
        _ => super::build_eternal_v1::trigger_recipe_for(name),
    }
}
