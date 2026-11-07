//! Card-database recipes for the eternal catalog's sideboard wave S1a:
//! the Legacy sideboard cards that complete the most decks per card
//! (graveyard hate, mass removal, artifact and enchantment hate, taxing
//! statics). E8a's and E5b's tables and E8b's triggers fall through to these; runtime programs
//! live in `src/eternal_sideboard_s1a_v1.rs`. Every row is keyed by a name
//! that occurs only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{DiscardSelf, ManaCost, SacrificeSelf, Tap};

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

/// Haywire Mite: "{G}, Sacrifice this creature: Exile target noncreature
/// artifact or noncreature enchantment."
const HAYWIRE_MITE: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{G}"), SacrificeSelf],
    AbilityEffectRecipe::MoveAllTargetsToExile,
    "StandardV1(crate::standard_cards_v1::StandardTargetV1::NoncreatureArtifactOrEnchantmentV1)",
)];

/// Tormod's Crypt: "{T}, Sacrifice this artifact: Exile all cards from
/// target player's graveyard."
const TORMODS_CRYPT: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap, SacrificeSelf],
    AbilityEffectRecipe::ExileTargetPlayersGraveyard,
    "AnyPlayer",
)];

/// Twinshot Sniper: "Channel -- {1}{R}, Discard this card: It deals 2
/// damage to any target."
const TWINSHOT_SNIPER: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{1}{R}"), DiscardSelf],
    effect: AbilityEffectRecipe::DealDamageAnyTarget(2),
    activation_zone: "Hand",
    sorcery_speed_only: false,
    target_spec: "AnyTarget",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Haywire Mite" => &HAYWIRE_MITE,
        "Tormod's Crypt" => &TORMODS_CRYPT,
        "Twinshot Sniper" => &TWINSHOT_SNIPER,
        _ => super::build_eternal_s1b_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function of `eternal_sideboard_s1a_v1`.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs. Modal spells' other modes are `mode2_for` and
/// `mode3_for` below.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        // Its alternative cost is `alt_cost_for` below.
        "Massacre" => spell(
            "TargetSpec::None",
            "crate::eternal_sideboard_s1a_v1::massacre",
        ),
        // Its name choice, tax and prohibition are
        // `eternal_sideboard_s1a_v1::statics`.
        "Disruptor Flute" => spell(
            "TargetSpec::None",
            "crate::eternal_sideboard_s1a_v1::disruptor_flute",
        ),
        "Meltdown" => spell(
            "TargetSpec::None",
            "crate::eternal_sideboard_s1a_v1::meltdown",
        ),
        // Rough; Tumble is the split card's second half (`adventure_for`
        // below).
        "Rough // Tumble" => spell(
            "TargetSpec::None",
            "crate::eternal_sideboard_s1a_v1::rough",
        ),
        "Warping Wail" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::CreaturePowerOrToughnessAtMostOneV1)",
            "crate::eternal_sideboard_s1a_v1::exile_target",
        ),
        // Its alternative cost is `alt_cost_for` below.
        "Force of Vigor" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::UpToTwoArtifactsOrEnchantmentsV1)",
            "crate::eternal_sideboard_s1a_v1::force_of_vigor",
        ),
        "Pyroclasm" => spell(
            "TargetSpec::None",
            "crate::eternal_sideboard_s1a_v1::pyroclasm",
        ),
        "Brotherhood's End" => spell(
            "TargetSpec::None",
            "crate::eternal_sideboard_s1a_v1::brotherhoods_end_damage",
        ),
        "Celestial Purge" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::BlackOrRedPermanentV1)",
            "crate::eternal_sideboard_s1a_v1::exile_target",
        ),
        "Rip Apart" => spell(
            "TargetSpec::CreatureOrPlaneswalker",
            "crate::eternal_sideboard_s1a_v1::rip_apart_damage",
        ),
        // Its target is a card in a graveyard other than a basic land card.
        "Surgical Extraction" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::NonbasicLandCardInAGraveyardV1)",
            "crate::eternal_sideboard_s1a_v1::surgical_extraction",
        ),
        "Swan Song" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::EnchantmentInstantOrSorcerySpellV1)",
            "crate::eternal_sideboard_s1a_v1::swan_song",
        ),
        _ => return super::build_eternal_s1b_v1::special_for(name),
    })
}

/// Rip Apart's second mode: "Destroy target artifact or enchantment."
/// Brotherhood's End's second mode: "Destroy all artifacts with mana value 3
/// or less." Warping Wail's second mode: "Counter target sorcery spell."
pub(super) fn mode2_for(name: &str) -> &'static str {
    match name {
        "Rip Apart" => "Some(ModeDef { target_spec: TargetSpec::ArtifactOrEnchantmentPermanent, effect: crate::eternal_sideboard_s1a_v1::destroy_target })",
        "Brotherhood's End" => "Some(ModeDef { target_spec: TargetSpec::None, effect: crate::eternal_sideboard_s1a_v1::brotherhoods_end_destroy })",
        "Warping Wail" => "Some(ModeDef { target_spec: TargetSpec::SorcerySpellOnStack, effect: crate::eternal_sideboard_s1a_v1::warping_wail_counter })",
        _ => super::build_eternal_s1b_v1::mode2_for(name),
    }
}

/// Warping Wail's third mode: "Create a 1/1 colorless Eldrazi Scion
/// creature token. It has 'Sacrifice this token: Add {C}.'"
pub(super) fn mode3_for(name: &str) -> &'static str {
    match name {
        "Warping Wail" => "Some(ModeDef { target_spec: TargetSpec::None, effect: crate::eternal_sideboard_s1a_v1::warping_wail_scion })",
        _ => super::build_eternal_s1b_v1::mode3_for(name),
    }
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Containment Priest" | "Disruptor Flute" => &["Keywords::FLASH"],
        "Swan Song Bird Token" => &["Keywords::FLYING"],
        "Twinshot Sniper" => &["Keywords::REACH"],
        _ => super::build_eternal_s1b_v1::keywords_for(name),
    }
}

/// Massacre: "If an opponent controls a Plains and you control a Swamp, you
/// may cast this spell without paying its mana cost." Force of Vigor: "If
/// it's not your turn, you may exile a green card from your hand rather
/// than pay this spell's mana cost."
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Massacre" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[], generic: 0, x_count: 0 })], condition: AltCostCondition::OpponentPlainsYouSwampV1 })",
        "Force of Vigor" => "Some(AltCostDef { components: &[CostComponent::ExileCardFromHandWithColorV1(ManaColor::G)], condition: AltCostCondition::NotYourTurnV1 })",
        _ => super::build_eternal_e9a_v1::alt_cost_for(name),
    }
}

/// Rough // Tumble's second half, Tumble {5}{R}: "Tumble deals 6 damage to
/// each creature with flying." Split cards (708) reuse the Adventure form,
/// as Fire // Ice does.
pub(super) fn adventure_for(name: &str) -> &'static str {
    match name {
        "Rough // Tumble" => "Some(AdventureDef { name: \"Tumble\", cost: Cost { pips: &[Pip::Colored(ManaColor::R)], generic: 5, x_count: 0 }, types: &[CardType::Sorcery], target_spec: TargetSpec::None, effect: ability_effect_program_eternal_sideboard_s1a_v1_tumble })",
        _ => "None",
    }
}

pub(super) fn adventure_effect_recipe_for(name: &str) -> Option<AbilityEffectRecipe> {
    match name {
        "Rough // Tumble" => Some(AbilityEffectRecipe::StandardProgram(
            "crate::eternal_sideboard_s1a_v1::tumble",
        )),
        _ => None,
    }
}

pub(super) fn adventure_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Rough // Tumble" => Some("Tumble"),
        _ => None,
    }
}

/// Card-database tokens for the triggered abilities, statics and programs
/// in `eternal_sideboard_s1a_v1`.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Brotherhood's End" => {
            "modal:damage_3_each_creature_and_planeswalker|destroy_all_artifacts_mana_value_3_or_less"
        }
        "Carpet of Flowers" => {
            "main_phase_begin:if_not_added_this_turn:may_add_up_to_islands_opponent_controls_mana_of_one_color"
        }
        "Celestial Purge" => "spell:exile_target_black_or_red_permanent",
        "Choke" => "static:islands_dont_untap",
        "Containment Priest" => {
            "flash;static:replace_nontoken_creature_entering_not_cast_with_exile"
        }
        "Defense Grid" => "static:spells_cost_3_more_except_during_their_controllers_turn",
        "Disruptor Flute" => {
            "flash;as_enters:choose_card_name;static:named_spells_cost_3_more;static:named_non_mana_activations_prohibited"
        }
        "Dread of Night" => "static:white_creatures_get_minus_1_minus_1",
        "Force of Vigor" => {
            "alt_cost:not_your_turn:exile_green_card_from_hand;spell:destroy_up_to_two_target_artifacts_and_or_enchantments"
        }
        "Haywire Mite" => {
            "dies:gain_2;activated:g_sacrifice_self:exile_target_noncreature_artifact_or_enchantment"
        }
        "Lavinia, Azorius Renegade" => {
            "static:opponents_cant_cast_noncreature_spells_mana_value_above_their_lands;opponent_casts_spell:if_no_mana_spent:counter_it"
        }
        "Leyline of the Void" => {
            "opening_hand:may_begin_on_battlefield;static:cards_to_opponents_graveyards_exiled_instead"
        }
        "Massacre" => {
            "alt_cost:opponent_controls_plains_you_control_swamp:free;spell:all_creatures_minus_2_minus_2_until_end_of_turn"
        }
        "Meltdown" => "spell:destroy_each_artifact_mana_value_x_or_less",
        "Pyroclasm" => "spell:damage_2_each_creature",
        "Rest in Peace" => {
            "etb:exile_all_graveyards;static:cards_and_tokens_to_graveyards_exiled_instead"
        }
        "Rip Apart" => "modal:damage_3_creature_or_planeswalker|destroy_artifact_or_enchantment",
        "Rough // Tumble" => {
            "split:rough:damage_2_each_creature_without_flying;tumble:damage_6_each_creature_with_flying"
        }
        "Serenity" => "controller_upkeep:destroy_all_artifacts_and_enchantments",
        "Surgical Extraction" => {
            "phyrexian_mana;spell:exile_target_card_and_all_same_name_in_owners_graveyard_hand_library:shuffle"
        }
        "Swan Song" => "spell:counter_target_enchantment_instant_or_sorcery:its_controller_creates_2_2_flying_bird",
        "Swan Song Bird Token" => "token:2_2_blue_bird_flying",
        "Tormod's Crypt" => "activated:tap_sacrifice_self:exile_target_players_graveyard",
        "Twinshot Sniper" => "reach;etb:damage_2_any_target;channel:1r_discard_self:damage_2_any_target",
        "Warping Wail" => {
            "modal:exile_creature_power_or_toughness_1_or_less|counter_target_sorcery|create_eldrazi_scion"
        }
        _ => super::build_eternal_s1b_v1::trigger_recipe_for(name),
    }
}
