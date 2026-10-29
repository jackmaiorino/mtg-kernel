//! Card-database recipes for the eternal catalog's Pauper Bogles, white
//! aggro, Rakdos Madness, Boros Moxite and Gates lists (wave E6b).
//! `build_eternal_e6a_v1` and the other eternal tables fall through to
//! these; runtime programs live in `src/eternal_pauper_e6b_v1.rs`. Every row
//! is keyed by a name that occurs only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, LibrarySearchFilterRecipe,
    Special, StackSpellFilter,
};
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

/// "Equip [cost]" (702.6a): attach to target creature you control, only as
/// a sorcery.
const fn equip(cost: &'static [AbilityCostRecipe]) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect: AbilityEffectRecipe::AttachSourceToTarget,
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec: "ControlledCreature",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

/// A landcycling ability (702.29e): activated from hand, discarding the card
/// as part of its cost, to search for a matching card and put it into hand.
const fn landcycling(
    cost: &'static [AbilityCostRecipe],
    filter: LibrarySearchFilterRecipe,
) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect: AbilityEffectRecipe::SearchLibraryToHand {
            filter,
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
    }
}

/// Ash Barrens: "Basic landcycling {1}".
const ASH_BARRENS: [ActivatedAbilityRecipe; 1] = [landcycling(
    &[ManaCost("{1}"), DiscardSelf],
    LibrarySearchFilterRecipe::BasicLand,
)];

/// Eagles of the North: "Plainscycling {1}".
const EAGLES_OF_THE_NORTH: [ActivatedAbilityRecipe; 1] = [landcycling(
    &[ManaCost("{1}"), DiscardSelf],
    LibrarySearchFilterRecipe::LandWithSubtype("Plains"),
)];

/// Sheltering Landscape: "{T}, Sacrifice this land: Search your library for
/// a basic Mountain, Forest, or Plains card, put it onto the battlefield
/// tapped, then shuffle." and "Cycling {R}{G}{W}". Its `{T}: Add {C}` is the
/// ordinary mana ability.
const SHELTERING_LANDSCAPE: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[Tap, SacrificeSelf],
        AbilityEffectRecipe::SearchLibraryToBattlefieldTapped {
            filter: LibrarySearchFilterRecipe::BasicLandWithAnySubtype([
                "Mountain", "Forest", "Plains",
            ]),
        },
        "None",
    ),
    ActivatedAbilityRecipe {
        cost: &[ManaCost("{R}{G}{W}"), DiscardSelf],
        effect: AbilityEffectRecipe::DrawCards(1),
        activation_zone: "Hand",
        sorcery_speed_only: false,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
];

/// Greatsword of Tyr: "Equip {W}".
const GREATSWORD_OF_TYR: [ActivatedAbilityRecipe; 1] = [equip(&[ManaCost("{W}")])];

/// Ragged Short Spear: "Equip {3}".
const RAGGED_SHORT_SPEAR: [ActivatedAbilityRecipe; 1] = [equip(&[ManaCost("{3}")])];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Ash Barrens" => &ASH_BARRENS,
        "Eagles of the North" => &EAGLES_OF_THE_NORTH,
        "Sheltering Landscape" => &SHELTERING_LANDSCAPE,
        "Greatsword of Tyr" => &GREATSWORD_OF_TYR,
        "Ragged Short Spear" => &RAGGED_SHORT_SPEAR,
        _ => super::build_eternal_e7a_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function of `eternal_pauper_e6b_v1`.
const fn program(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        // Madness is `madness_cost_for` below.
        "Alms of the Vein" => program(
            "TargetSpec::TargetOpponent",
            "crate::eternal_pauper_e6b_v1::alms_of_the_vein",
        ),
        // Flashback (tap three untapped white creatures) is `flashback_for`.
        "Battle Screech" => Special::CreateTokens {
            token: "Bird Token",
            count: 2,
        },
        // "As an additional cost to cast this spell, sacrifice an artifact
        // or discard a card": discarding is the additional cost, and the
        // sacrifice is the alternative cost of the same mana (`alt_cost_for`
        // below), which then skips the discard.
        "Demand Answers" => Special::DrawCards(2),
        "Guardians' Pledge" => program(
            "TargetSpec::None",
            "crate::eternal_pauper_e6b_v1::guardians_pledge",
        ),
        "Malevolent Rumble" => program(
            "TargetSpec::None",
            "crate::eternal_pauper_e6b_v1::malevolent_rumble",
        ),
        "Mana Tithe" => Special::CounterUnlessPaysGeneric {
            filter: StackSpellFilter::Any,
            generic: 1,
        },
        // Flashback is `flashback_for` below.
        "Pursue the Past" => program(
            "TargetSpec::None",
            "crate::eternal_pauper_e6b_v1::pursue_the_past",
        ),
        // Its creature-tapping alternative cost is `alt_cost_for` below.
        "Ramosian Rally" => Special::BoostControlledCreatures {
            power: 1,
            toughness: 1,
            keyword: "NONE",
        },
        "Vampire's Kiss" => program(
            "TargetSpec::AnyPlayer",
            "crate::eternal_pauper_e6b_v1::vampires_kiss",
        ),
        // "Enchant creature" Auras enter attached to their target; their
        // statics are `attachment_for` below.
        "Angelic Gift" | "Armadillo Cloak" | "Ancestral Mask" | "Ethereal Armor" | "Rancor"
        | "Spirit Link" => Special::BindTheMonster,
        // "As this Aura enters, choose a color."
        "Benevolent Blessing" => program(
            "TargetSpec::Creature",
            "crate::eternal_pauper_e6b_v1::benevolent_blessing",
        ),
        // "Enchant creature you control": the Standard Aura entry, whose
        // enchant rule is `eternal_pauper_e6b_v1::aura_host_legal`.
        "Cartouche of Solidarity" => program(
            "TargetSpec::ControlledCreature",
            "crate::standard_cards_v1::aura_enters",
        ),
        // "Enchant land".
        "Abundant Growth" => program("TargetSpec::Land", "crate::standard_cards_v1::aura_enters"),
        // "Enchant Forest" and "As this Aura enters, choose a color."
        "Utopia Sprawl" => program(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::ForestV1)",
            "crate::eternal_pauper_e6b_v1::utopia_sprawl",
        ),
        _ => return super::build_eternal_e7a_v1::special_for(name),
    })
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Benevolent Blessing" => &["Keywords::FLASH"],
        "Eagles of the North" => &["Keywords::FLYING"],
        "Gladecover Scout" | "Silhana Ledgewalker" | "Slippery Bogle" => &["Keywords::HEXPROOF"],
        "Kitchen Imp" => &["Keywords::FLYING", "Keywords::HASTE"],
        "Bird Token" => &["Keywords::FLYING"],
        "White Warrior Token" => &["Keywords::VIGILANCE"],
        _ => super::build_eternal_e7a_v1::keywords_for(name),
    }
}

/// Alternative costs. Leonardo: "Sneak {W}"; Spider-Man: "Web-slinging
/// {W}"; Lunarch Veteran: "Disturb {1}{W}"; Martyr's Soul: convoke; Ramosian
/// Rally: "If you control a Plains, you may tap an untapped creature you
/// control rather than pay this spell's mana cost."; Demand Answers' artifact
/// sacrifice branch (see `special_for`).
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Leonardo, Big Brother" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Colored(ManaColor::W)], generic: 0, x_count: 0 }), CostComponent::ReturnControlledPermanentToOwnersHand(PermanentFilterDef::UnblockedAttackingCreatureV1)], condition: AltCostCondition::SneakV1 })",
        "Spider-Man, Web-Slinger" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Colored(ManaColor::W)], generic: 0, x_count: 0 }), CostComponent::ReturnControlledPermanentToOwnersHand(PermanentFilterDef::TappedCreatureV1)], condition: AltCostCondition::WebSlingingV1 })",
        "Lunarch Veteran" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Colored(ManaColor::W)], generic: 1, x_count: 0 })], condition: AltCostCondition::DisturbV1 })",
        "Martyr's Soul" => "Some(AltCostDef { components: &[CostComponent::ConvokeMana(Cost { pips: &[Pip::Colored(ManaColor::W)], generic: 2, x_count: 0 })], condition: AltCostCondition::Always })",
        "Ramosian Rally" => "Some(AltCostDef { components: &[CostComponent::TapUntappedControlledPermanent(PermanentFilterDef::CreatureV1)], condition: AltCostCondition::ControlsPermanentWithSubtype(Subtype::Plains) })",
        "Demand Answers" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Colored(ManaColor::R)], generic: 1, x_count: 0 }), CostComponent::SacrificeControlled { count: 1, filter: PermanentFilter::Artifact }], condition: AltCostCondition::Always })",
        _ => super::build_eternal_e7a_v1::alt_cost_for(name),
    }
}

/// Mandatory additional costs. Demand Answers: "discard a card" (its
/// sacrifice branch is the alternative cost).
pub(super) fn additional_cost_for(name: &str) -> &'static str {
    match name {
        "Demand Answers" => "Some(&[CostComponent::DiscardCards(1)])",
        _ => super::build_eternal_e7a_v1::additional_cost_for(name),
    }
}

/// Flashback costs. Battle Screech: "Flashback--Tap three untapped white
/// creatures you control." Pursue the Past: "Flashback {2}{R}{W}".
pub(super) fn flashback_for(name: &str) -> String {
    match name {
        "Battle Screech" => "Some(FlashbackDef { cost: &[CostComponent::TapUntappedControlledPermanentsV1 { count: 3, filter: PermanentFilterDef::CreatureWithColor(ManaColor::W) }] })".to_string(),
        "Pursue the Past" => {
            let (pips, generic, x_count) = super::parse_cost("{2}{R}{W}");
            format!(
                "Some(FlashbackDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }})] }})",
                pips.join(", ")
            )
        }
        _ => super::build_eternal_e7a_v1::flashback_for(name),
    }
}

/// Madness costs: Kitchen Imp's and Alms of the Vein's "Madness {B}".
pub(super) fn madness_cost_for(name: &str) -> Option<&'static str> {
    match name {
        "Kitchen Imp" | "Alms of the Vein" => Some("{B}"),
        _ => super::build_eternal_e7b_v1::madness_cost_for(name),
    }
}

/// Generic cost reductions. Salt Road Packbeast: "Affinity for creatures".
/// Search Party Captain: "This spell costs {1} less to cast for each
/// creature you attacked with this turn."
pub(super) fn generic_cost_reduction_for(name: &str) -> &'static str {
    match name {
        "Salt Road Packbeast" => "Some(GenericCostReductionDef { generic_per_count: 1, count: DynamicCountDef::ControllerBattlefieldAnyType(&[CardType::Creature]) })",
        "Search Party Captain" => "Some(GenericCostReductionDef { generic_per_count: 1, count: DynamicCountDef::CreaturesAttackedThisTurn })",
        _ => super::build_eternal_e7b_v1::generic_cost_reduction_for(name),
    }
}

/// Fixed colors of the primary printed mana ability, where they differ from
/// `produces_mana`: the karoos' one action identifies their whole bundle,
/// and the chosen-color Gates expose only their fixed color.
pub(super) fn primary_mana_ability_colors(name: &str) -> Option<Vec<&'static str>> {
    match name {
        "Boros Garrison" => Some(vec!["R"]),
        "Rakdos Carnarium" => Some(vec!["B"]),
        "Cliffgate" => Some(vec!["R"]),
        "Manor Gate" => Some(vec!["G"]),
        _ => super::build_eternal_e8b_v1::primary_mana_ability_colors(name),
    }
}

/// Gates whose mana ability adds their chosen color.
pub(super) fn mana_ability_includes_chosen_color(name: &str) -> bool {
    matches!(name, "Cliffgate" | "Manor Gate")
        || super::build_eternal_e9b_v1::mana_ability_includes_chosen_color(name)
}

/// "As this land enters, choose a color other than red/green."
pub(super) fn as_enters_choose_color_other_than(name: &str) -> &'static str {
    match name {
        "Cliffgate" => "Some(ManaColor::R)",
        "Manor Gate" => "Some(ManaColor::G)",
        _ => super::build_eternal_e9b_v1::as_enters_choose_color_other_than(name),
    }
}

/// Single printed mana abilities that are not "tap: add one".
pub(super) fn mana_ability_def_for(name: &str) -> &'static str {
    match name {
        // "{T}: Add {R}{W}." One activation adds both colors at once.
        "Boros Garrison" => "Some(ManaAbilityDef { cost: ManaAbilityCostDef::TapSelf, amount: ManaAbilityAmountDef::FixedColorsV1(&[ManaColor::R, ManaColor::W]), controller_damage: 0, max_activations_per_turn: None })",
        // "{T}: Add {B}{R}."
        "Rakdos Carnarium" => "Some(ManaAbilityDef { cost: ManaAbilityCostDef::TapSelf, amount: ManaAbilityAmountDef::FixedColorsV1(&[ManaColor::B, ManaColor::R]), controller_damage: 0, max_activations_per_turn: None })",
        _ => super::build_eternal_e8b_v1::mana_ability_def_for(name),
    }
}

/// Idyllic Grange: "This land enters tapped unless you control three or
/// more other Plains."
pub(super) fn enters_battlefield_tapped_unless_for(name: &str) -> &'static str {
    match name {
        "Idyllic Grange" => "Some(EntersBattlefieldTappedUnlessDef { controller_controls_other_subtype: Subtype::Plains, minimum_count: 3 })",
        _ => super::build_eternal_e8b_v1::enters_battlefield_tapped_unless_for(name),
    }
}

/// "Enchant creature" Aura statics. Ancestral Mask's and Ethereal Armor's
/// bonuses are multiplied by their enchantment counts
/// (`eternal_pauper_e6b_v1::aura_multiplier`); Armadillo Cloak's and Spirit
/// Link's damage triggers and Benevolent Blessing's protection are engine
/// hooks.
pub(super) fn attachment_for(name: &str) -> &'static str {
    match name {
        "Angelic Gift" => "Some(AttachmentDef::AuraCreatureStatic(AuraCreatureStaticDef { power: 0, toughness: 0, keywords: Keywords::FLYING, per_controlled_subtype: None }))",
        "Armadillo Cloak" => "Some(AttachmentDef::AuraCreatureStatic(AuraCreatureStaticDef { power: 2, toughness: 2, keywords: Keywords::TRAMPLE, per_controlled_subtype: None }))",
        "Ancestral Mask" => "Some(AttachmentDef::AuraCreatureStatic(AuraCreatureStaticDef { power: 2, toughness: 2, keywords: Keywords::NONE, per_controlled_subtype: None }))",
        "Ethereal Armor" => "Some(AttachmentDef::AuraCreatureStatic(AuraCreatureStaticDef { power: 1, toughness: 1, keywords: Keywords::FIRST_STRIKE, per_controlled_subtype: None }))",
        "Rancor" => "Some(AttachmentDef::AuraCreatureStatic(AuraCreatureStaticDef { power: 2, toughness: 0, keywords: Keywords::TRAMPLE, per_controlled_subtype: None }))",
        "Spirit Link" | "Benevolent Blessing" => "Some(AttachmentDef::AuraCreatureStatic(AuraCreatureStaticDef { power: 0, toughness: 0, keywords: Keywords::NONE, per_controlled_subtype: None }))",
        _ => super::build_eternal_e7a_v1::attachment_for(name),
    }
}

/// Equipment statics, and Cartouche of Solidarity's ("Enchanted creature
/// gets +1/+1 and has first strike"), which the Standard Aura entry carries
/// the way Sheltered by Ghosts does.
pub(super) fn equipment_for(name: &str) -> &'static str {
    match name {
        "Greatsword of Tyr" => "Some(EquipmentDef { power_delta: 0, toughness_delta: 0, add_subtype: None, controller_turn_keywords: Keywords::NONE, other_turn_keywords: Keywords::NONE, noncreature_spell_damage_to_each_opponent: 0, job_select: false, granted_activated_ability: None, pt_controller_turn_only: false })",
        "Ragged Short Spear" => "Some(EquipmentDef { power_delta: 2, toughness_delta: 0, add_subtype: None, controller_turn_keywords: Keywords::NONE, other_turn_keywords: Keywords::NONE, noncreature_spell_damage_to_each_opponent: 0, job_select: false, granted_activated_ability: None, pt_controller_turn_only: false })",
        "Cartouche of Solidarity" => "Some(EquipmentDef { power_delta: 1, toughness_delta: 1, add_subtype: None, controller_turn_keywords: Keywords::FIRST_STRIKE, other_turn_keywords: Keywords::FIRST_STRIKE, noncreature_spell_damage_to_each_opponent: 0, job_select: false, granted_activated_ability: None, pt_controller_turn_only: false })",
        _ => super::build_eternal_e7a_v1::equipment_for(name),
    }
}

/// Lunarch Veteran's back face, Luminous Phantom: a 1/1 white Spirit Cleric
/// with flying.
pub(super) fn transform_face_for(name: &str) -> &'static str {
    match name {
        "Lunarch Veteran" => "Some(TransformFaceDef { name: \"Luminous Phantom\", types: &[CardType::Creature], subtypes: &[Subtype::Spirit, Subtype::Cleric], colors: &[ManaColor::W], power: Some(1), toughness: Some(1), keywords: Keywords::FLYING })",
        _ => super::build_eternal_e9b_v1::transform_face_for(name),
    }
}

pub(super) fn transform_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Lunarch Veteran" => Some("Luminous Phantom"),
        _ => super::build_eternal_e9b_v1::transform_face_name_for(name),
    }
}

/// Card-database tokens for the triggered abilities in
/// `eternal_pauper_e6b_v1::triggers_for`, its hooks and its statics.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Asgardian Citadel" => "etb:gain_life:1",
        "Jagged Barrens" => "etb:target_opponent:damage:1",
        "Boros Garrison" | "Rakdos Carnarium" => {
            "etb:return_chosen_controlled_land_to_owners_hand:untargeted"
        }
        "Idyllic Grange" => "etb_untapped:target_controlled_creature:plus_one_counter:1",
        "Angelic Gift" | "Abundant Growth" => "etb:draw:1",
        "Salt Road Packbeast" | "Search Party Captain" => "etb:draw:1",
        "Cartouche of Solidarity" => "etb:create_token:White Warrior Token",
        "Thraben Inspector" => "etb:investigate",
        "Rancor" => "put_into_graveyard_from_battlefield:return_to_owners_hand",
        "Dawnbringer Cleric" => {
            "etb:modal:gain_life_2|destroy_target_enchantment|exile_target_card_from_a_graveyard"
        }
        "Eagles of the North" => "etb:controlled_creatures_plus_1_plus_0_first_strike_eot",
        "Fortress Kin-Guard" => "etb:endure:1",
        "Heliod's Pilgrim" => "etb:may_search_aura_card_to_hand",
        "Lunarch Veteran" => {
            "other_controlled_creature_enters:gain_life:1;face1:other_controlled_creature_leaves:gain_life:1;face1:graveyard_from_anywhere:exile_instead"
        }
        "Martyr's Soul" => "etb_if_no_tapped_lands:recheck:two_plus_one_counters_on_self",
        "Raffine's Informant" => "etb:connive",
        "Ragged Short Spear" => "etb:may_discard:draw:2",
        "Greatsword of Tyr" => {
            "equipped_creature_attacks:plus_one_counter_on_it:tap_up_to_one_target_defending_creature"
        }
        "Armadillo Cloak" | "Spirit Link" => {
            "enchanted_creature_deals_damage:aura_controller_gains_that_much"
        }
        "Ancestral Mask" => "static:plus_2_plus_2_per_other_enchantment_on_battlefield",
        "Ethereal Armor" => "static:plus_1_plus_1_per_enchantment_you_control",
        "Benevolent Blessing" => {
            "as_enters_choose_color:protection_from_chosen_color:keeps_own_attachments"
        }
        "Silhana Ledgewalker" => "static:blocked_only_by_flying",
        "Utopia Sprawl" => {
            "as_enters_choose_color:enchanted_forest_tapped_for_mana:add_one_chosen_color"
        }
        "Leonardo, Big Brother" => {
            "sneak:declare_blockers_step:enters_tapped_and_attacking;static:plus_1_plus_0_per_other_controlled_creature"
        }
        "Spider-Man, Web-Slinger" => "web_slinging:return_tapped_controlled_creature",
        _ => super::build_eternal_e7a_v1::trigger_recipe_for(name),
    }
}
