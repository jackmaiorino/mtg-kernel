//! Card-database recipes for the Pauper, Legacy and Vintage catalog
//! (`data/eternal_v1/cards_v1.json`). Runtime programs live in
//! `src/eternal_cards_v1.rs`; this module names them so they are part of the
//! generated definitions and the eternal catalog identity. Every row is keyed
//! by a name that occurs only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, LibrarySearchFilterRecipe,
    Special, StackSpellFilter,
};
use AbilityCostRecipe::{
    DiscardSelf, ExileSelf, ManaCost, PayEnergy, ReturnControlledUnblockedAttacker, SacrificeSelf,
    Tap,
};

/// An activated ability from the battlefield, at instant speed and without
/// a target.
const fn instant(
    cost: &'static [AbilityCostRecipe],
    effect: AbilityEffectRecipe,
) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect,
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

/// Melded Moxite: "{3}, Sacrifice this artifact: Create a tapped 2/2
/// colorless Robot artifact creature token."
const MELDED_MOXITE: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{3}"), SacrificeSelf],
    AbilityEffectRecipe::Program {
        name: "create_tapped_robot_token",
        effect: "EffectOp::CreateTokensDynamic { token_def: crate::card_def::card_id_by_name(\"Robot Token\").expect(\"Robot Token in CARD_DEFS\"), controller: PlayerRef::Controller, count: DynamicValueDef::Fixed(1), tapped: true }",
    },
)];

/// Inventor's Axe: "Equip--Pay {E}{E}" (attach to target creature you
/// control, as a sorcery).
const INVENTORS_AXE: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[PayEnergy(2)],
    effect: AbilityEffectRecipe::AttachSourceToTarget,
    activation_zone: "Battlefield",
    sorcery_speed_only: true,
    target_spec: "ControlledCreature",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Giant's Boulder: "{7}, {T}, Sacrifice this artifact: Destroy target
/// permanent." Its mana ability is an additional mana ability.
const GIANTS_BOULDER: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{7}"), Tap, SacrificeSelf],
    effect: AbilityEffectRecipe::DestroyTarget,
    activation_zone: "Battlefield",
    sorcery_speed_only: false,
    target_spec: "AnyPermanent",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Mukotai Ambusher: "Ninjutsu {1}{B}".
const MUKOTAI_AMBUSHER: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[
        AbilityCostRecipe::Mana {
            colored: Some("B"),
            generic: 1,
        },
        ReturnControlledUnblockedAttacker,
    ],
    effect: AbilityEffectRecipe::PutSourceOntoBattlefieldTappedAndAttacking,
    activation_zone: "Hand",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Seething Landscape: "{T}, Sacrifice this land: Search your library for a
/// basic Island, Swamp, or Mountain card, put it onto the battlefield
/// tapped, then shuffle." and "Cycling {U}{B}{R}". Its `{T}: Add {C}` is the
/// ordinary mana ability.
const SEETHING_LANDSCAPE: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[Tap, SacrificeSelf],
        AbilityEffectRecipe::SearchLibraryToBattlefieldTapped {
            filter: LibrarySearchFilterRecipe::BasicLandWithAnySubtype([
                "Island", "Swamp", "Mountain",
            ]),
        },
    ),
    ActivatedAbilityRecipe {
        cost: &[ManaCost("{U}{B}{R}"), DiscardSelf],
        effect: AbilityEffectRecipe::DrawCards(1),
        activation_zone: "Hand",
        sorcery_speed_only: false,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
];

/// Campfire: "{1}, {T}: You gain 2 life." and "{2}, {T}, Exile Campfire:
/// Put all commanders you own from the command zone and from your graveyard
/// into your hand. Then shuffle your graveyard into your library."
const CAMPFIRE: [ActivatedAbilityRecipe; 2] = [
    instant(&[ManaCost("{1}"), Tap], AbilityEffectRecipe::GainLife(2)),
    instant(
        &[ManaCost("{2}"), Tap, ExileSelf],
        AbilityEffectRecipe::StandardProgram("crate::eternal_cards_v1::campfire_shuffle_graveyard"),
    ),
];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Melded Moxite" => &MELDED_MOXITE,
        "Inventor's Axe" => &INVENTORS_AXE,
        "Giant's Boulder" => &GIANTS_BOULDER,
        "Mukotai Ambusher" => &MUKOTAI_AMBUSHER,
        "Seething Landscape" => &SEETHING_LANDSCAPE,
        "Campfire" => &CAMPFIRE,
        _ => &[],
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Agony Warp" => Special::StandardProgram {
            target_spec:
                "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::TwoCreaturesV1)",
            program: "crate::eternal_cards_v1::agony_warp",
        },
        "Plunder the Trollshaws" => Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_cards_v1::plunder_the_trollshaws",
        },
        // Replicate is `eternal_cards_v1::replicate_cost`, named in this
        // card's trigger token below.
        "Lose Focus" => Special::CounterUnlessPaysGeneric {
            filter: StackSpellFilter::Any,
            generic: 2,
        },
        _ => return None,
    })
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Inventor's Axe" => &["Keywords::FLASH"],
        "Baleful Strix" => &["Keywords::FLYING", "Keywords::DEATHTOUCH"],
        "Utrom Monitor" => &["Keywords::FLYING"],
        "Mukotai Ambusher" => &["Keywords::LIFELINK"],
        "Thorn of the Black Rose" => &["Keywords::DEATHTOUCH"],
        _ => &[],
    }
}

/// Card-database tokens for the triggered abilities in
/// `eternal_cards_v1::triggers_for` and the replicate table.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Melded Moxite" => "etb:may_discard_one_then_draw_two",
        "Inventor's Axe" => {
            "etb:controller_gets_energy:2;etb:attach_source_to_target_controlled_creature"
        }
        "Baleful Strix" => "etb:draw:1",
        "Giant's Boulder" => "etb:scry_2",
        "Augur of Bolas" => {
            "etb:private_look_top3:may_reveal_one_instant_or_sorcery_to_hand:rest_bottom_any_order"
        }
        "Thorn of the Black Rose" => "etb:become_monarch",
        "Dimir Aqueduct" => "etb:return_chosen_controlled_land_to_owners_hand:untargeted",
        "Lose Focus" => {
            "replicate:{U}:announced_as_x;cast:copy_once_per_payment:may_choose_new_targets"
        }
        _ => "none",
    }
}

/// Equipment statics.
pub(super) fn equipment_for(name: &str) -> &'static str {
    match name {
        "Inventor's Axe" => "Some(EquipmentDef { power_delta: 2, toughness_delta: 0, add_subtype: None, controller_turn_keywords: Keywords::NONE, other_turn_keywords: Keywords::NONE, noncreature_spell_damage_to_each_opponent: 0, job_select: false, granted_activated_ability: None, pt_controller_turn_only: false })",
        _ => "None",
    }
}

/// Flashback costs.
pub(super) fn flashback_for(name: &str) -> String {
    match name {
        "Plunder the Trollshaws" => {
            let (pips, generic, x_count) = super::parse_cost("{3}{U}");
            format!(
                "Some(FlashbackDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }})] }})",
                pips.join(", ")
            )
        }
        _ => "None".to_string(),
    }
}

/// Single printed mana abilities that are not "tap: add one".
pub(super) fn mana_ability_def_for(name: &str) -> &'static str {
    match name {
        // "{T}: Add {U}{B}." One activation adds both colors at once.
        "Dimir Aqueduct" => "Some(ManaAbilityDef { cost: ManaAbilityCostDef::TapSelf, amount: ManaAbilityAmountDef::FixedColorsV1(&[ManaColor::U, ManaColor::B]), controller_damage: 0, max_activations_per_turn: None })",
        _ => "None",
    }
}
