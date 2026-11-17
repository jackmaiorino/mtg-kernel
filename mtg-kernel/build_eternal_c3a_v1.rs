//! Card-database recipes for the eternal catalog's Vintage Cube wave C3a
//! (cheap creatures with existing analogs). Wave C2c's tables fall through to these, and
//! these fall through to wave C3b's
//! (`build_eternal_c3b_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c3a_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.
//!
//! Rows of this wave that live in other waves' tables: Bloodbraid
//! Challenger's escape (the tail of `escape_for`, wave E9a's), Embereth
//! Shieldbreaker's Adventure (the tail of the Adventure tables, wave C1c's).

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{DiscardCards, DiscardSelf, ManaCost, Tap};

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

/// A program of `eternal_cube_c3a_v1` (or of an earlier wave).
const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Auriok Salvagers: "{1}{W}: Return target artifact card with mana value 1
/// or less from your graveyard to your hand." (Goblin Engineer's target
/// filter with the bound 1, and the plain return-to-hand effect.)
const AURIOK_SALVAGERS: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}{W}")],
    AbilityEffectRecipe::MoveAllTargetsToHand,
    "StandardV1(crate::standard_cards_v1::StandardTargetV1::ArtifactCardInOwnGraveyardManaValueAtMostV1(1))",
)];

/// Formidable Speaker: "{1}, {T}: Untap another target permanent."
const FORMIDABLE_SPEAKER: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}"), Tap],
    AbilityEffectRecipe::UntapTarget,
    "StandardV1(crate::standard_cards_v1::StandardTargetV1::AnotherPermanentV1)",
)];

/// Iron-Shield Elf: "Discard a card: This creature gains indestructible
/// until end of turn. Tap it." (Wild Mongrel's discard cost.)
const IRON_SHIELD_ELF: [ActivatedAbilityRecipe; 1] = [instant(
    &[DiscardCards(1)],
    program("crate::eternal_cube_c3a_v1::iron_shield_elf"),
    "None",
)];

/// Giver of Runes: "{T}: Another target creature you control gains
/// protection from colorless or from the color of your choice until end of
/// turn."
const GIVER_OF_RUNES: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap],
    program("crate::eternal_cube_c3a_v1::giver_of_runes"),
    "AnotherControlledCreature",
)];

/// Harvester of Misery: "{1}{B}, Discard this card: Target creature gets
/// -2/-2 until end of turn." (Activated from the hand, as Bannerhide
/// Krushok's reinforce is.)
const HARVESTER_OF_MISERY: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{1}{B}"), DiscardSelf],
    effect: AbilityEffectRecipe::PumpTargetUntilEndOfTurn {
        power: -2,
        toughness: -2,
    },
    activation_zone: "Hand",
    sorcery_speed_only: false,
    target_spec: "Creature",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Devoted Druid: "Put a -1/-1 counter on this creature: Untap this
/// creature." The counter is the cost, put as the ability is activated
/// (`eternal_cube_c3a_v1::activation_paid`), so the printed cost here is
/// {0}, as Parallax Wave's removed counter is. Its "{T}: Add {G}." is the
/// ordinary mana ability.
const DEVOTED_DRUID: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{0}")],
    program("crate::eternal_e5a_v1::untap_source"),
    "None",
)];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Auriok Salvagers" => &AURIOK_SALVAGERS,
        "Formidable Speaker" => &FORMIDABLE_SPEAKER,
        "Iron-Shield Elf" => &IRON_SHIELD_ELF,
        "Giver of Runes" => &GIVER_OF_RUNES,
        "Harvester of Misery" => &HARVESTER_OF_MISERY,
        "Devoted Druid" => &DEVOTED_DRUID,
        _ => super::build_eternal_c3b_v1::activated_ability_recipes_for(name),
    }
}

/// No C3a card has a spell program of its own: its creatures and its
/// Adventure are plain permanent spells.
pub(super) fn special_for(name: &str) -> Option<Special> {
    super::build_eternal_c3b_v1::special_for(name)
}

/// No C3a card has kicker yet.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c3b_v1::kicker_cost_for(name)
}

/// No C3a card is an Aura or an Equipment yet.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c3b_v1::attachment_for(name)
}

/// No C3a card has flashback yet.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c3b_v1::flashback_for(name)
}

/// Death-Greeter's Champion: "Dash {3}{R}" (Ragavan's alternative cost).
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Death-Greeter's Champion" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Colored(ManaColor::R)], generic: 3, x_count: 0 })], condition: AltCostCondition::DashV1 })",
        _ => super::build_eternal_c3b_v1::alt_cost_for(name),
    }
}

/// Embereth Shieldbreaker's Battle Display {R}: "Destroy target artifact."
/// Reached from the Adventure tables' tail (`build_eternal_p1a_v1`).
pub(super) fn adventure_for(name: &str) -> &'static str {
    match name {
        "Embereth Shieldbreaker" => "Some(AdventureDef { name: \"Battle Display\", cost: Cost { pips: &[Pip::Colored(ManaColor::R)], generic: 0, x_count: 0 }, types: &[CardType::Sorcery], target_spec: TargetSpec::ArtifactPermanent, effect: ability_effect_destroy_target })",
        _ => "None",
    }
}

pub(super) fn adventure_effect_recipe_for(name: &str) -> Option<AbilityEffectRecipe> {
    match name {
        "Embereth Shieldbreaker" => Some(AbilityEffectRecipe::DestroyTarget),
        _ => None,
    }
}

pub(super) fn adventure_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Embereth Shieldbreaker" => Some("Battle Display"),
        _ => None,
    }
}

/// Printed evergreen keywords, in printed order. Bloodbraid Challenger's
/// cascade and escape, Death-Greeter's Champion's dash and backup and
/// Harvester of Misery's enters trigger are listed elsewhere.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Bloodbraid Challenger" => &["Keywords::HASTE"],
        "Death-Greeter's Champion" => &["Keywords::DOUBLE_STRIKE"],
        "Harvester of Misery" => &["Keywords::MENACE"],
        _ => super::build_eternal_c3b_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Auriok Salvagers" => {
            "activated:1w:return_target_artifact_card_mana_value_at_most_1_from_your_graveyard_to_hand"
        }
        "Bloodbraid Challenger" => {
            "cast_self:cascade:exile_until_nonland_less_mana_value:may_cast_free:rest_bottom_random;haste;escape:3rg:exile_three_other_graveyard_cards"
        }
        "Death-Greeter's Champion" => {
            "dash:3r:haste:return_next_end_step;etb:backup_1:target_creature:if_another_gains_double_strike_until_end_of_turn;double_strike"
        }
        "Embereth Shieldbreaker" => "adventure:battle_display:r:sorcery:destroy_target_artifact",
        "Formidable Speaker" => {
            "etb:may_discard_a_card:if_you_do_search_creature_card:reveal:hand:shuffle;activated:1_tap:untap_another_target_permanent"
        }
        "Iron-Shield Elf" => {
            "activated:discard_a_card:source_gains_indestructible_until_end_of_turn:tap_it"
        }
        "Luminarch Aspirant" => {
            "controller_beginning_of_combat:target_controlled_creature:put_plus_one_counter"
        }
        "Giver of Runes" => {
            "activated:tap:another_target_controlled_creature:protection_from_colorless_or_chosen_color_until_end_of_turn"
        }
        "Harvester of Misery" => {
            "menace;etb:other_creatures_get_minus_2_minus_2_until_end_of_turn;activated_from_hand:1b_discard_self:target_creature_minus_2_minus_2_until_end_of_turn"
        }
        "Devoted Druid" => {
            "mana:g;activated:cost_put_minus_one_minus_one_counter_on_self:untap_this_creature"
        }
        _ => super::build_eternal_c3b_v1::trigger_recipe_for(name),
    }
}
