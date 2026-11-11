//! Card-database recipes for the eternal catalog's Premodern wave P2b:
//! Survival, Elves (partly) and Madness Threshold. Wave P1a's tables fall
//! through to these, and the activated ability, spell and trigger tables
//! continue to sideboard wave P3a's, the chain's tail. The other tables'
//! own defaults are the chain's base values. Runtime programs live in
//! `src/eternal_premodern_p2b_v1.rs`. Every row is keyed by a name that
//! occurs only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{DiscardCards, ManaCost, Tap};

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

/// A program of `eternal_premodern_p2b_v1`.
const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Glory: "{2}{W}: Choose a color. Creatures you control gain protection
/// from the chosen color until end of turn. Activate only if this card is in
/// your graveyard."
const GLORY: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    activation_zone: "Graveyard",
    ..instant(
        &[ManaCost("{2}{W}")],
        program("crate::eternal_premodern_p2b_v1::glory_protection"),
        "None",
    )
}];

/// Masticore: "{2}: This creature deals 1 damage to target creature." and
/// "{2}: Regenerate this creature." Its upkeep cost is a trigger.
const MASTICORE: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[ManaCost("{2}")],
        AbilityEffectRecipe::DamageTarget(1),
        "Creature",
    ),
    instant(
        &[ManaCost("{2}")],
        program("crate::eternal_vintage_e7b_v1::regenerate_source"),
        "None",
    ),
];

/// Survival of the Fittest: "{G}, Discard a creature card: Search your
/// library for a creature card, reveal it, put it into your hand, then
/// shuffle." The discard is any card in the cost; that it must be a creature
/// card is `eternal_premodern_p2b_v1::statics::discard_cost_allows`.
const SURVIVAL_OF_THE_FITTEST: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{G}"), DiscardCards(1)],
    program("crate::eternal_premodern_p2b_v1::survival_search"),
    "None",
)];

/// Tireless Tribe: "Discard a card: This creature gets +0/+4 until end of
/// turn."
const TIRELESS_TRIBE: [ActivatedAbilityRecipe; 1] = [instant(
    &[DiscardCards(1)],
    AbilityEffectRecipe::PumpSourceUntilEndOfTurn {
        power: 0,
        toughness: 4,
    },
    "None",
)];

/// Wirewood Symbiote: "Return an Elf you control to its owner's hand: Untap
/// target creature. Activate only once each turn."
const WIREWOOD_SYMBIOTE: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    max_activations_per_turn: Some(1),
    ..instant(
        &[AbilityCostRecipe::ReturnControlledPermanentWithSubtype(
            "Elf",
        )],
        AbilityEffectRecipe::UntapTarget,
        "Creature",
    )
}];

/// Yavimaya Hollow: "{G}, {T}: Regenerate target creature." Its "{T}: Add
/// {C}." is the mana ability.
const YAVIMAYA_HOLLOW: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{G}"), Tap],
    program("crate::eternal_premodern_p2b_v1::regenerate_target"),
    "Creature",
)];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Glory" => &GLORY,
        "Masticore" => &MASTICORE,
        "Survival of the Fittest" => &SURVIVAL_OF_THE_FITTEST,
        "Tireless Tribe" => &TIRELESS_TRIBE,
        "Wirewood Symbiote" => &WIREWOOD_SYMBIOTE,
        "Yavimaya Hollow" => &YAVIMAYA_HOLLOW,
        _ => super::build_eternal_p3a_v1::activated_ability_recipes_for(name),
    }
}

/// Spell programs: Chatter of the Squirrel ("Create a 1/1 green Squirrel
/// creature token."); its flashback is `flashback_for` below.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Chatter of the Squirrel" => Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_premodern_p2b_v1::chatter_of_the_squirrel",
        },
        _ => return super::build_eternal_p3a_v1::special_for(name),
    })
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Arrogant Wurm" => &["Keywords::TRAMPLE"],
        "Birds of Paradise" | "Glory" => &["Keywords::FLYING"],
        "Blastoderm" => &["Keywords::SHROUD"],
        "Caller of the Claw" => &["Keywords::FLASH"],
        _ => &[],
    }
}

/// Morph (702.37): cast face down as a 2/2
/// creature for {3} through the face-down cast that disguise uses, without
/// disguise's ward; turning it face up is
/// `eternal_premodern_p2b_v1::statics::morph_cost_of`.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Nantuko Vigilante" | "Birchlore Rangers" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[], generic: 3, x_count: 0 })], condition: AltCostCondition::DisguiseFromHand })",
        _ => "None",
    }
}

/// Chatter of the Squirrel: "Flashback {1}{G}".
pub(super) fn flashback_for(name: &str) -> String {
    match name {
        "Chatter of the Squirrel" => {
            let (pips, generic, x_count) = super::parse_cost("{1}{G}");
            format!(
                "Some(FlashbackDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }})] }})",
                pips.join(", ")
            )
        }
        _ => "None".to_string(),
    }
}

/// Arrogant Wurm: "Madness {2}{G}".
pub(super) fn madness_cost_for(name: &str) -> Option<&'static str> {
    match name {
        "Arrogant Wurm" => Some("{2}{G}"),
        _ => None,
    }
}

/// Single printed mana abilities that are not "tap: add one". Gaea's Cradle:
/// "{T}: Add {G} for each creature you control."
pub(super) fn mana_ability_def_for(name: &str) -> &'static str {
    match name {
        "Gaea's Cradle" => "Some(ManaAbilityDef { cost: ManaAbilityCostDef::TapSelf, amount: ManaAbilityAmountDef::Dynamic(DynamicValueDef::ControlledPermanentsWithType(CardType::Creature)), controller_damage: 0, max_activations_per_turn: None })",
        _ => "None",
    }
}

/// Card-database tokens for the triggered abilities and statics in
/// `eternal_premodern_p2b_v1`.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Thornscape Battlemage" => {
            "independent_kickers:r:w;etb_if_r:2_damage_any_target;etb_if_w:destroy_artifact"
        }
        "Arrogant Wurm" => "madness:2g",
        "Birds of Paradise" => "mana:any_color",
        "Birchlore Rangers" => {
            "mana:tap_two_controlled_elves:any_color;morph:g:face_down_2_2_no_ward"
        }
        "Blastoderm" => "fading:3",
        "Caller of the Claw" => {
            "etb:bear_tokens_per_nontoken_creature_put_into_graveyard_this_turn"
        }
        "Chatter of the Squirrel" => "spell:squirrel_token;flashback:1g",
        "Deranged Hermit" => {
            "echo:3gg;etb:four_squirrel_tokens;static:squirrels_get_plus_one_plus_one"
        }
        "Gaea's Cradle" => "mana:g_per_creature_controlled",
        "Glory" => "graveyard_activated:2w:choose_color:creatures_gain_protection",
        "Masticore" => {
            "upkeep:sacrifice_unless_discard;activated:damage_1_creature;activated:regenerate"
        }
        "Multani's Acolyte" => "echo:gg;etb:draw_1",
        "Nantuko Vigilante" => {
            "morph:1g:face_down_2_2_no_ward;turned_face_up:destroy_artifact_or_enchantment"
        }
        "Survival of the Fittest" => {
            "activated:g_discard_creature_card:search_creature_card_to_hand"
        }
        "Tireless Tribe" => "activated:discard:pump_0_4",
        "Vintara Snapper" => "static:shroud_while_no_untapped_lands",
        "Wirewood Symbiote" => "activated:return_elf:untap_target_creature:once_per_turn",
        "Yavimaya Granger" => "echo:2g;etb:may_search_basic_land_to_battlefield_tapped",
        "Yavimaya Hollow" => "mana:c;activated:g_tap:regenerate_target_creature",
        _ => super::build_eternal_p3a_v1::trigger_recipe_for(name),
    }
}
