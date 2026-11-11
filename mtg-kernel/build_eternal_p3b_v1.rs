//! Card-database recipes for the eternal catalog's Premodern sideboard wave
//! P3b. This module is the tail of the recipe chains: wave P3a's activated
//! ability, spell, trigger, kicker and attachment tables fall through to
//! these, as do wave P2b's flashback, alternative cost and keyword tables,
//! and its own defaults are the chains' base values. Runtime programs live
//! in `src/eternal_premodern_p3b_v1.rs`. Every row is keyed by a name that
//! occurs only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, PermanentFilterRecipe, Special,
};
use AbilityCostRecipe::{ManaCost, Tap};

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

/// Goblin Tinkerer: "{R}, {T}: Destroy target artifact. That artifact deals
/// damage equal to its mana value to this creature."
const GOBLIN_TINKERER: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{R}"), Tap],
    AbilityEffectRecipe::StandardProgram("crate::eternal_premodern_p3b_v1::goblin_tinkerer"),
    "ArtifactPermanent",
)];

/// Plaguebearer: "{X}{X}{B}: Destroy target nonblack creature with mana
/// value X." The engine's announced X belongs to a few Standard cards, so
/// each X from 0 to 8 (the largest mana value in the Premodern pool, as for
/// Pernicious Deed) is its own ability, which costs {X}{X}{B} and targets a
/// nonblack creature with mana value exactly X.
macro_rules! plaguebearer {
    ($cost:literal, $x:literal) => {
        instant(
            &[ManaCost($cost)],
            AbilityEffectRecipe::DestroyTarget,
            concat!(
                "StandardV1(crate::standard_cards_v1::StandardTargetV1::NonblackCreatureManaValueV1(",
                $x,
                "))"
            ),
        )
    };
}

const PLAGUEBEARER: [ActivatedAbilityRecipe; 9] = [
    plaguebearer!("{B}", 0),
    plaguebearer!("{2}{B}", 1),
    plaguebearer!("{4}{B}", 2),
    plaguebearer!("{6}{B}", 3),
    plaguebearer!("{8}{B}", 4),
    plaguebearer!("{10}{B}", 5),
    plaguebearer!("{12}{B}", 6),
    plaguebearer!("{14}{B}", 7),
    plaguebearer!("{16}{B}", 8),
];

/// Zuran Orb: "Sacrifice a land: You gain 2 life."
const ZURAN_ORB: [ActivatedAbilityRecipe; 1] = [instant(
    &[AbilityCostRecipe::SacrificeControlled {
        count: 1,
        filter: PermanentFilterRecipe::Land,
    }],
    AbilityEffectRecipe::GainLife(2),
    "None",
)];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Goblin Tinkerer" => &GOBLIN_TINKERER,
        "Plaguebearer" => &PLAGUEBEARER,
        "Zuran Orb" => &ZURAN_ORB,
        _ => &[],
    }
}

/// A spell whose program is a runtime function of `program`'s module.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Haunting Echoes" => spell(
            "TargetSpec::AnyPlayer",
            "crate::eternal_premodern_p3b_v1::haunting_echoes",
        ),
        // Its free cast is `alt_cost_for` below.
        "Mogg Salvage" => spell(
            "TargetSpec::ArtifactPermanent",
            "crate::eternal_sideboard_s1b_v1::destroy_target",
        ),
        // Its flashback is `flashback_for` below.
        "Ray of Revelation" => spell(
            "TargetSpec::EnchantmentPermanent",
            "crate::eternal_e9b_v1::destroy_target",
        ),
        _ => return None,
    })
}

/// No P3b card has kicker.
pub(super) fn kicker_cost_for(_name: &str) -> Option<&'static str> {
    None
}

/// No P3b card is an Aura or Equipment.
pub(super) fn attachment_for(_name: &str) -> &'static str {
    "None"
}

/// Ray of Revelation: "Flashback {G}".
pub(super) fn flashback_for(name: &str) -> String {
    match name {
        "Ray of Revelation" => {
            let (pips, generic, x_count) = super::parse_cost("{G}");
            format!(
                "Some(FlashbackDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }})] }})",
                pips.join(", ")
            )
        }
        _ => "None".to_string(),
    }
}

/// Mogg Salvage: "If an opponent controls an Island and you control a
/// Mountain, you may cast this spell without paying its mana cost."
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Mogg Salvage" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[], generic: 0, x_count: 0 })], condition: AltCostCondition::OpponentIslandYouMountainV1 })",
        _ => "None",
    }
}

/// Printed evergreen keywords.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Xantid Swarm" => &["Keywords::FLYING"],
        _ => &[],
    }
}

/// Card-database tokens for the triggered abilities and statics in
/// `eternal_premodern_p3b_v1`.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Dystopia" => {
            "cumulative_upkeep:pay_1_life;each_upkeep:that_player_sacrifices_green_or_white_permanent"
        }
        "Engineered Plague" => {
            "as_enters:choose_creature_type_any;static:chosen_type_creatures_minus_1_1"
        }
        "Gloom" => {
            "static:white_spells_cost_3_more;static:white_enchantment_abilities_cost_3_more"
        }
        "Goblin Tinkerer" => {
            "activated:r_tap:destroy_target_artifact:artifact_deals_mana_value_damage_to_source"
        }
        "Haunting Echoes" => {
            "spell:exile_graveyard_but_basic_lands:exile_library_cards_with_same_names:shuffle"
        }
        "Mogg Salvage" => "alt_cost:free_if_opponent_island_and_you_mountain;spell:destroy_target_artifact",
        "Plaguebearer" => "activated:xxb:destroy_target_nonblack_creature_mana_value_x",
        "Ray of Revelation" => "spell:destroy_target_enchantment;flashback:g",
        "Xantid Swarm" => "attacks:defending_player_cant_cast_spells_this_turn",
        "Zuran Orb" => "activated:sacrifice_land:gain_2",
        _ => "none",
    }
}
