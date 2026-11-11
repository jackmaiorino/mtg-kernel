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

/// Program of `eternal_premodern_p2c_v1`.
const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Volrath's Shapeshifter retains this ability with or without a text donor.
const VOLRATH: [ActivatedAbilityRecipe; 1] = [ability(
    &[ManaCost("{2}")],
    program("crate::full_text_v1::retained_discard"),
    "Battlefield",
    "None",
)];

/// Avatar of Woe: "{T}: Destroy target creature. It can't be regenerated."
const AVATAR_OF_WOE: [ActivatedAbilityRecipe; 1] = [ability(
    &[AbilityCostRecipe::Tap],
    program("crate::eternal_premodern_p2c_v1::destroy_target_creature_no_regeneration"),
    "Battlefield",
    "Creature",
)];

/// Hermit Druid: "{G}, {T}: Reveal cards from the top of your library until
/// you reveal a basic land card. ..."
const HERMIT_DRUID: [ActivatedAbilityRecipe; 1] = [ability(
    &[ManaCost("{G}"), AbilityCostRecipe::Tap],
    program("crate::eternal_premodern_p2c_v1::hermit_druid"),
    "Battlefield",
    "None",
)];

/// Petradon: "{R}: Petradon gets +1/+0 until end of turn."
const PETRADON: [ActivatedAbilityRecipe; 1] = [ability(
    &[ManaCost("{R}")],
    AbilityEffectRecipe::PumpSourceUntilEndOfTurn {
        power: 1,
        toughness: 0,
    },
    "Battlefield",
    "None",
)];

/// Putrid Imp: "Discard a card: Putrid Imp gains flying until end of turn."
const PUTRID_IMP: [ActivatedAbilityRecipe; 1] = [ability(
    &[AbilityCostRecipe::DiscardCards(1)],
    program("crate::eternal_premodern_p2c_v1::imp_flying"),
    "Battlefield",
    "None",
)];

/// Battlefield Scrounger: "Threshold - Put three cards from your graveyard
/// on the bottom of your library: Battlefield Scrounger gets +3/+3 until end
/// of turn. Activate this ability only once each turn, and only if seven or
/// more cards are in your graveyard."
const BATTLEFIELD_SCROUNGER: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[AbilityCostRecipe::PutGraveyardCardsOnLibraryBottom(3)],
    effect: AbilityEffectRecipe::PumpSourceUntilEndOfTurn {
        power: 3,
        toughness: 3,
    },
    activation_zone: "Battlefield",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: Some(1),
}];

/// Phyrexian Devourer: "Exile the top card of your library: Put X +1/+1
/// counters on Phyrexian Devourer, where X is the exiled card's mana value."
const PHYREXIAN_DEVOURER: [ActivatedAbilityRecipe; 1] = [ability(
    &[AbilityCostRecipe::ExileTopCardOfLibrary],
    program("crate::eternal_premodern_p2c_v1::devourer_counters"),
    "Battlefield",
    "None",
)];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Avatar of Woe" => &AVATAR_OF_WOE,
        "Hermit Druid" => &HERMIT_DRUID,
        "Petradon" => &PETRADON,
        "Putrid Imp" => &PUTRID_IMP,
        "Battlefield Scrounger" => &BATTLEFIELD_SCROUNGER,
        "Phyrexian Devourer" => &PHYREXIAN_DEVOURER,
        "Palinchron" => &PALINCHRON,
        "Triskelion" => &TRISKELION,
        "Unearth" => &UNEARTH,
        "Volrath's Shapeshifter" => &VOLRATH,
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
        "Exhume" => Some(Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_premodern_p2c_v1::exhume",
        }),
        // Its second half, Death, is `adventure_for` below.
        "Life // Death" => Some(Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_premodern_p2c_v1::life",
        }),
        // Its flashback is `flashback_for` below.
        "Krosan Reclamation" => Some(Special::StandardProgram {
            target_spec: "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::PlayerThenUpToTwoOfTheirGraveyardCardsV1)",
            program: "crate::eternal_premodern_p2c_v1::reclamation",
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
        "Palinchron" | "Karmic Guide" => &["Keywords::FLYING"],
        "Phantom Nishoba" => &["Keywords::TRAMPLE"],
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
        "Avatar of Woe" => {
            "fear;cost_reduction:ten_creature_cards_in_all_graveyards;activated:tap:destroy_target_creature_cant_be_regenerated"
        }
        "Battlefield Scrounger" => {
            "activated:threshold_once_each_turn:put_three_graveyard_cards_on_library_bottom:plus_3_3"
        }
        "Exhume" => "spell:each_player_puts_creature_card_from_graveyard_onto_battlefield",
        "Hermit Druid" => "activated:g_tap:reveal_until_basic_land_to_hand_rest_to_graveyard",
        "Karmic Guide" => {
            "flying;protection_from_black;echo:3ww:upkeep_if_entered_since_last_upkeep;etb:return_target_creature_card_from_graveyard"
        }
        "Krosan Reclamation" => {
            "spell:target_player_shuffles_up_to_two_target_graveyard_cards;flashback:1g"
        }
        "Life // Death" => {
            "split:life:lands_become_1_1_creatures_until_end_of_turn;death:return_target_creature_card_from_graveyard:lose_life_mana_value"
        }
        "Petradon" => {
            "etb:exile_two_target_lands;leaves:return_exiled_cards;activated:r:plus_1_0"
        }
        "Phantom Nishoba" => {
            "trample;enters_with_7_plus_one_counters;deals_damage:gain_that_much_life;prevent_damage_remove_counter"
        }
        "Phyrexian Devourer" => {
            "state:power_7_or_greater_sacrifice;activated:exile_top_card:x_plus_one_counters"
        }
        "Putrid Imp" => "activated:discard:flying_until_end_of_turn;threshold:plus_1_1_cant_block",
        "Reflecting Pool" => "mana:any_type_another_land_could_produce",
        "Volrath's Shapeshifter" => {
            "layer3:top_creature_card_of_controller_graveyard_full_text;activated:2:discard"
        }
        "Undiscovered Paradise" => "mana:any_color;return_to_hand_next_untap_step",
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

/// Krosan Reclamation: "Flashback {1}{G}".
pub(super) fn flashback_for(name: &str) -> String {
    match name {
        "Krosan Reclamation" => {
            let (pips, generic, x_count) = super::parse_cost("{1}{G}");
            format!(
                "Some(FlashbackDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }})] }})",
                pips.join(", ")
            )
        }
        _ => super::build_eternal_e6a_v1::flashback_for(name),
    }
}

/// Avatar of Woe: "If there are ten or more creature cards total in all
/// graveyards, this spell costs {6} less to cast."
pub(super) fn generic_cost_reduction_for(name: &str) -> &'static str {
    match name {
        "Avatar of Woe" => "Some(GenericCostReductionDef { generic_per_count: 6, count: DynamicCountDef::TenCreatureCardsInAllGraveyardsV1 })",
        _ => super::build_eternal_e6b_v1::generic_cost_reduction_for(name),
    }
}

/// Life // Death's second half, Death {1}{B}: "Return target creature card
/// from your graveyard to the battlefield. You lose life equal to its mana
/// value." Split cards reuse the Adventure form (see wave P1a's Fire // Ice).
pub(super) fn adventure_for(name: &str) -> String {
    match name {
        "Life // Death" => "Some(AdventureDef { name: \"Death\", cost: Cost { pips: &[Pip::Colored(ManaColor::B)], generic: 1, x_count: 0 }, types: &[CardType::Sorcery], target_spec: TargetSpec::CreatureCardInOwnGraveyard, effect: ability_effect_program_eternal_premodern_p2c_v1_death })".to_string(),
        _ => super::build_eternal_v1::adventure_for(name).to_string(),
    }
}

pub(super) fn adventure_effect_recipe_for(name: &str) -> Option<AbilityEffectRecipe> {
    match name {
        "Life // Death" => Some(program("crate::eternal_premodern_p2c_v1::death")),
        _ => super::build_eternal_v1::adventure_effect_recipe_for(name),
    }
}

pub(super) fn adventure_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Life // Death" => Some("Death"),
        _ => super::build_eternal_v1::adventure_face_name_for(name),
    }
}

/// Lands whose only printed rules are mana abilities with no unconditional
/// program: Reflecting Pool's abilities are all gated on another land.
pub(super) fn static_only_land(name: &str) -> bool {
    name == "Reflecting Pool"
}
