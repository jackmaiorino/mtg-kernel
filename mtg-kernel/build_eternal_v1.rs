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
    DiscardSelf, ExileSelf, ManaCost, PayEnergy, PayLife, ReturnControlledUnblockedAttacker,
    SacrificeSelf, Tap,
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

/// A fetchland: "{T}, Pay 1 life, Sacrifice this land: Search your library
/// for a <first> or <second> card, put it onto the battlefield, then
/// shuffle." Any card with either land type qualifies (dual lands and
/// surveil lands included), and it enters untapped unless it says otherwise.
macro_rules! fetchland {
    ($name:literal, $first:literal, $second:literal) => {
        [instant(
            &[Tap, PayLife(1), SacrificeSelf],
            AbilityEffectRecipe::Program {
                name: $name,
                effect: concat!(
                    "EffectOp::SearchLibraryCardsToDestination { player: PlayerRef::Controller, filter: crate::effect::LibraryCardFilter::CardWithAnySubtypeV1([Subtype::",
                    $first,
                    ", Subtype::",
                    $second,
                    "]), max_targets: 1, destination: crate::effect::LibrarySearchDestinationV1::Battlefield { tapped: false } }"
                ),
            },
        )]
    };
}

const BLOODSTAINED_MIRE: [ActivatedAbilityRecipe; 1] =
    fetchland!("fetch_swamp_or_mountain", "Swamp", "Mountain");
const FLOODED_STRAND: [ActivatedAbilityRecipe; 1] =
    fetchland!("fetch_plains_or_island", "Plains", "Island");
const MARSH_FLATS: [ActivatedAbilityRecipe; 1] =
    fetchland!("fetch_plains_or_swamp", "Plains", "Swamp");
const MISTY_RAINFOREST: [ActivatedAbilityRecipe; 1] =
    fetchland!("fetch_forest_or_island", "Forest", "Island");
const POLLUTED_DELTA: [ActivatedAbilityRecipe; 1] =
    fetchland!("fetch_island_or_swamp", "Island", "Swamp");
const SCALDING_TARN: [ActivatedAbilityRecipe; 1] =
    fetchland!("fetch_island_or_mountain", "Island", "Mountain");

/// Wasteland: "{T}, Sacrifice this land: Destroy target nonbasic land." Its
/// `{T}: Add {C}` is the ordinary mana ability.
const WASTELAND: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[Tap, SacrificeSelf],
    effect: AbilityEffectRecipe::DestroyTarget,
    activation_zone: "Battlefield",
    sorcery_speed_only: false,
    target_spec: "StandardV1(crate::standard_cards_v1::StandardTargetV1::NonbasicLandV1)",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Mishra's Bauble: "{T}, Sacrifice this artifact: Look at the top card of
/// target player's library. Draw a card at the beginning of the next turn's
/// upkeep."
const MISHRAS_BAUBLE: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[Tap, SacrificeSelf],
    effect: AbilityEffectRecipe::StandardProgram("crate::eternal_cards_v1::mishras_bauble"),
    activation_zone: "Battlefield",
    sorcery_speed_only: false,
    target_spec: "AnyPlayer",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Urza's Bauble: "{T}, Sacrifice this artifact: Look at a card at random
/// in target player's hand. You draw a card at the beginning of the next
/// turn's upkeep."
const URZAS_BAUBLE: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[Tap, SacrificeSelf],
    effect: AbilityEffectRecipe::StandardProgram("crate::eternal_cards_v1::urzas_bauble"),
    activation_zone: "Battlefield",
    sorcery_speed_only: false,
    target_spec: "AnyPlayer",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// A planeswalker loyalty ability: sorcery speed, once per turn through its
/// loyalty cost.
const fn loyalty(
    cost: &'static [AbilityCostRecipe],
    program: &'static str,
    target_spec: &'static str,
) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect: AbilityEffectRecipe::StandardProgram(program),
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec,
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

/// Tamiyo, Seasoned Scholar (Tamiyo, Inquisitive Student's back face).
const TAMIYO: [ActivatedAbilityRecipe; 3] = [
    loyalty(
        &[AbilityCostRecipe::Loyalty(2)],
        "crate::eternal_cards_v1::tamiyo_plus_two",
        "None",
    ),
    loyalty(
        &[AbilityCostRecipe::Loyalty(-3)],
        "crate::eternal_cards_v1::tamiyo_minus_three",
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::InstantOrSorceryCardInOwnGraveyard)",
    ),
    loyalty(
        &[AbilityCostRecipe::Loyalty(-7)],
        "crate::eternal_cards_v1::tamiyo_minus_seven",
        "None",
    ),
];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Tamiyo, Inquisitive Student" => &TAMIYO,
        "Melded Moxite" => &MELDED_MOXITE,
        "Inventor's Axe" => &INVENTORS_AXE,
        "Giant's Boulder" => &GIANTS_BOULDER,
        "Mukotai Ambusher" => &MUKOTAI_AMBUSHER,
        "Seething Landscape" => &SEETHING_LANDSCAPE,
        "Campfire" => &CAMPFIRE,
        "Bloodstained Mire" => &BLOODSTAINED_MIRE,
        "Flooded Strand" => &FLOODED_STRAND,
        "Marsh Flats" => &MARSH_FLATS,
        "Misty Rainforest" => &MISTY_RAINFOREST,
        "Polluted Delta" => &POLLUTED_DELTA,
        "Scalding Tarn" => &SCALDING_TARN,
        "Wasteland" => &WASTELAND,
        "Mishra's Bauble" => &MISHRAS_BAUBLE,
        "Urza's Bauble" => &URZAS_BAUBLE,
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
        "Thoughtseize" => Special::StandardProgram {
            target_spec: "TargetSpec::AnyPlayer",
            program: "crate::eternal_cards_v1::thoughtseize",
        },
        "Fatal Push" => Special::StandardProgram {
            target_spec: "TargetSpec::Creature",
            program: "crate::eternal_cards_v1::fatal_push",
        },
        "Unholy Heat" => Special::StandardProgram {
            target_spec: "TargetSpec::CreatureOrPlaneswalker",
            program: "crate::eternal_cards_v1::unholy_heat",
        },
        // Its alternative cost is `alt_cost_for` below.
        "Daze" => Special::CounterUnlessPaysGeneric {
            filter: StackSpellFilter::Any,
            generic: 1,
        },
        "Force of Will" => Special::CounterTarget(StackSpellFilter::Any),
        // Modes two and three are `mode2_for` and `mode3_for` below.
        "Prismari Charm" => Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_cards_v1::prismari_charm_surveil_draw",
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
        "Brazen Borrower" => &["Keywords::FLASH", "Keywords::FLYING"],
        "Murktide Regent" => &["Keywords::FLYING"],
        "Moonshadow" => &["Keywords::MENACE"],
        "Orcish Bowmasters" => &["Keywords::FLASH"],
        "Tamiyo, Inquisitive Student" => &["Keywords::FLYING"],
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
        "Dragon's Rage Channeler" => "cast_noncreature:surveil:1",
        "Murktide Regent" => {
            "enters_with_plus_one_counter_per_instant_or_sorcery_delve_exiled;instant_or_sorcery_card_leaves_controller_graveyard:plus_one_counter_on_source_incarnation"
        }
        "Moonshadow" => {
            "enters_with_minus_one_counters:6;one_or_more_permanent_cards_to_controller_graveyard_per_collection_while_minus_one_counter:remove_minus_one_counter_from_source_incarnation"
        }
        "Orcish Bowmasters" => {
            "etb:any_target_damage_1_then_amass_orcs_1;opponent_draws_except_first_in_their_draw_step:any_target_damage_1_then_amass_orcs_1"
        }
        "Mishra's Bauble" | "Urza's Bauble" => {
            "delayed:next_upkeep_began:activator_draws_1:source_incarnation_contract"
        }
        "Tamiyo, Inquisitive Student" => {
            "attacks:investigate;draw_nth:3:exile_then_return_transformed_under_owner;face1_plus2:until_controllers_next_turn:each_creature_attacking_controller_gets_minus1_0_eot;face1_minus7:emblem_no_maximum_hand_size"
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

/// Alternative costs. Daze: "You may return an Island you control to its
/// owner's hand rather than pay this spell's mana cost." Force of Will: "You
/// may pay 1 life and exile a blue card from your hand rather than pay this
/// spell's mana cost."
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Daze" => "Some(AltCostDef { components: &[CostComponent::ReturnControlledPermanentToOwnersHand(PermanentFilterDef::LandWithSubtype(Subtype::Island))], condition: AltCostCondition::Always })",
        "Force of Will" => "Some(AltCostDef { components: &[CostComponent::PayLife(1), CostComponent::ExileCardFromHandWithColorV1(ManaColor::U)], condition: AltCostCondition::Always })",
        _ => "None",
    }
}

/// Prismari Charm's second mode: "Prismari Charm deals 1 damage to each of
/// one or two targets."
pub(super) fn mode2_for(name: &str) -> &'static str {
    match name {
        "Prismari Charm" => "Some(ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::OneOrTwoAnyTargetsV1), effect: crate::eternal_cards_v1::prismari_charm_damage })",
        _ => "None",
    }
}

/// Prismari Charm's third mode: "Return target nonland permanent to its
/// owner's hand."
pub(super) fn mode3_for(name: &str) -> &'static str {
    match name {
        "Prismari Charm" => "Some(ModeDef { target_spec: TargetSpec::NonlandPermanent, effect: crate::eternal_cards_v1::return_target_to_owners_hand })",
        _ => "None",
    }
}

/// Brazen Borrower's Petty Theft {1}{U}: "Return target nonland permanent
/// an opponent controls to its owner's hand."
pub(super) fn adventure_for(name: &str) -> &'static str {
    match name {
        "Brazen Borrower" => "Some(AdventureDef { name: \"Petty Theft\", cost: Cost { pips: &[Pip::Colored(ManaColor::U)], generic: 1, x_count: 0 }, types: &[CardType::Instant], target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::OpponentNonlandPermanent), effect: ability_effect_program_eternal_cards_v1_return_target_to_owners_hand })",
        _ => "None",
    }
}

pub(super) fn adventure_effect_recipe_for(name: &str) -> Option<AbilityEffectRecipe> {
    match name {
        "Brazen Borrower" => Some(AbilityEffectRecipe::StandardProgram(
            "crate::eternal_cards_v1::return_target_to_owners_hand",
        )),
        _ => None,
    }
}

pub(super) fn adventure_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Brazen Borrower" => Some("Petty Theft"),
        _ => None,
    }
}

/// Delve (702.66).
pub(super) fn delve_for(name: &str) -> bool {
    name == "Murktide Regent"
}

/// Nethergoyf: "Escape--{2}{B}, Exile any number of other cards from your
/// graveyard with four or more card types among them."
pub(super) fn escape_for(name: &str) -> String {
    match name {
        "Nethergoyf" => {
            let (pips, generic, x_count) = super::parse_cost("{2}{B}");
            format!(
                "Some(EscapeDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }}), CostComponent::ExileOtherGraveyardCardsWithCardTypesV1(4)] }})",
                pips.join(", ")
            )
        }
        _ => "None".to_string(),
    }
}

/// Tamiyo's back face, a planeswalker with the card's supertypes and a
/// green-and-blue color indicator.
pub(super) fn transform_face_for(name: &str) -> &'static str {
    match name {
        "Tamiyo, Inquisitive Student" => "Some(TransformFaceDef { name: \"Tamiyo, Seasoned Scholar\", types: &[CardType::Planeswalker], subtypes: &[Subtype::Tamiyo], colors: &[ManaColor::U, ManaColor::G], power: None, toughness: None, keywords: Keywords::NONE })",
        _ => "None",
    }
}

pub(super) fn transform_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Tamiyo, Inquisitive Student" => Some("Tamiyo, Seasoned Scholar"),
        _ => None,
    }
}

/// Activated abilities printed on one face only.
pub(super) fn activated_ability_face_for(name: &str, _index: usize) -> Option<u8> {
    match name {
        "Tamiyo, Inquisitive Student" => Some(1),
        _ => None,
    }
}
