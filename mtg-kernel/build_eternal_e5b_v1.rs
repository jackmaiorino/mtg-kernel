//! Card-database recipes for the eternal catalog's Legacy aggro, Eldrazi
//! and control lists (wave E5b). `build_eternal_lurrus_v1` and the other
//! eternal tables fall through to these; runtime programs live in
//! `src/eternal_legacy_e5b_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{ManaCost, PayLife, SacrificeSelf, Tap};

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

/// A fetchland: "{T}, Pay 1 life, Sacrifice this land: Search your library
/// for a <first> or <second> card, put it onto the battlefield, then
/// shuffle." Any card with either land type qualifies.
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
            "None",
        )]
    };
}

const ARID_MESA: [ActivatedAbilityRecipe; 1] =
    fetchland!("fetch_mountain_or_plains", "Mountain", "Plains");
const WINDSWEPT_HEATH: [ActivatedAbilityRecipe; 1] =
    fetchland!("fetch_forest_or_plains", "Forest", "Plains");

/// Goblin Bombardment: "Sacrifice a creature: This enchantment deals 1
/// damage to any target."
const GOBLIN_BOMBARDMENT: [ActivatedAbilityRecipe; 1] = [instant(
    &[AbilityCostRecipe::SacrificeControlled {
        count: 1,
        filter: super::PermanentFilterRecipe::Creature,
    }],
    AbilityEffectRecipe::DealDamageAnyTarget(1),
    "AnyTarget",
)];

/// Abundant Countryside: "{6}, {T}: Create a 1/1 colorless Shapeshifter
/// creature token with changeling." Its `{T}: Add {C}` is the ordinary mana
/// ability and its creature-spell mana a restricted ability.
const ABUNDANT_COUNTRYSIDE: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{6}"), Tap],
    AbilityEffectRecipe::StandardProgram(
        "crate::eternal_legacy_e5b_v1::abundant_countryside_token",
    ),
    "None",
)];

/// Eye of Ugin: "{7}, {T}: Search your library for a colorless creature
/// card, reveal it, put it into your hand, then shuffle."
const EYE_OF_UGIN: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{7}"), Tap],
    AbilityEffectRecipe::StandardProgram("crate::eternal_legacy_e5b_v1::eye_of_ugin_search"),
    "None",
)];

/// Lazotep Quarry: "{X}{2}, {T}, Sacrifice a Desert: Exile target creature
/// card with mana value X from your graveyard. Create a token that's a copy
/// of it, except it's a 4/4 black Zombie. Activate only as a sorcery." X is
/// the target's mana value, as Gorilla Shaman's. Its two mana abilities are
/// the ordinary `{T}: Add {C}` and an additional sacrifice ability.
const LAZOTEP_QUARRY: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[
        AbilityCostRecipe::VariableMana {
            generic: 2,
            x_count: 1,
        },
        Tap,
        AbilityCostRecipe::SacrificeControlled {
            count: 1,
            filter: super::PermanentFilterRecipe::Desert,
        },
    ],
    effect: AbilityEffectRecipe::StandardProgram("crate::eternal_legacy_e5b_v1::lazotep_zombie"),
    activation_zone: "Battlefield",
    sorcery_speed_only: true,
    target_spec: "CreatureCardInOwnGraveyard",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// A planeswalker loyalty ability: sorcery speed, once per turn through its
/// loyalty cost.
const fn loyalty(
    cost: &'static [AbilityCostRecipe],
    program: &'static str,
) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect: AbilityEffectRecipe::StandardProgram(program),
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

/// Ajani, Nacatl Avenger (Ajani, Nacatl Pariah's back face): +2, 0 and -4.
const AJANI: [ActivatedAbilityRecipe; 3] = [
    loyalty(
        &[AbilityCostRecipe::Loyalty(2)],
        "crate::eternal_legacy_e5b_v1::ajani_plus_two",
    ),
    loyalty(
        &[AbilityCostRecipe::Loyalty(0)],
        "crate::eternal_legacy_e5b_v1::ajani_zero",
    ),
    loyalty(
        &[AbilityCostRecipe::Loyalty(-4)],
        "crate::eternal_legacy_e5b_v1::ajani_minus_four",
    ),
];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Arid Mesa" => &ARID_MESA,
        "Windswept Heath" => &WINDSWEPT_HEATH,
        "Goblin Bombardment" => &GOBLIN_BOMBARDMENT,
        "Abundant Countryside" => &ABUNDANT_COUNTRYSIDE,
        "Eye of Ugin" => &EYE_OF_UGIN,
        "Lazotep Quarry" => &LAZOTEP_QUARRY,
        "Ajani, Nacatl Pariah" => &AJANI,
        _ => super::build_eternal_e6a_v1::activated_ability_recipes_for(name),
    }
}

/// Activated abilities printed on one face only: Ajani's loyalty abilities
/// are its back face's.
pub(super) fn activated_ability_face_for(name: &str, index: usize) -> Option<u8> {
    (name == "Ajani, Nacatl Pariah")
        .then_some(1)
        .or_else(|| super::build_eternal_e6a_v1::activated_ability_face_for(name, index))
}

/// A spell whose program is a runtime function of `eternal_legacy_e5b_v1`.
const fn program(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Stifle" => program(
            "TargetSpec::StackAbility",
            "crate::eternal_legacy_e5b_v1::stifle",
        ),
        // Replicate is `eternal_legacy_e5b_v1::replicate_cost`.
        "Consign to Memory" => program(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::TriggeredAbilityOrColorlessSpellV1)",
            "crate::eternal_legacy_e5b_v1::consign_to_memory",
        ),
        "Galvanic Discharge" => program(
            "TargetSpec::CreatureOrPlaneswalker",
            "crate::eternal_legacy_e5b_v1::galvanic_discharge",
        ),
        "Wrath of the Skies" => program(
            "TargetSpec::None",
            "crate::eternal_legacy_e5b_v1::wrath_of_the_skies",
        ),
        // Miracle is its draw trigger in `eternal_legacy_e5b_v1`.
        "Terminus" => program("TargetSpec::None", "crate::eternal_legacy_e5b_v1::terminus"),
        // The other nine mode choices are `mode2_for`, `mode3_for` and
        // `additional_modes_for` below.
        "Eldrazi Confluence" => program(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::CreaturesThenNonlandPermanentsV1 { creatures: 3, permanents: 0 })",
            "crate::eternal_legacy_e5b_v1::eldrazi_confluence_0",
        ),
        _ => return super::build_eternal_e6a_v1::special_for(name),
    })
}

/// Eldrazi Confluence's second mode choice: +3/-3 twice and a flicker.
pub(super) fn mode2_for(name: &str) -> &'static str {
    match name {
        "Eldrazi Confluence" => "Some(ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::CreaturesThenNonlandPermanentsV1 { creatures: 2, permanents: 1 }), effect: crate::eternal_legacy_e5b_v1::eldrazi_confluence_1 })",
        _ => "None",
    }
}

/// Eldrazi Confluence's third mode choice: +3/-3 twice and a Scion.
pub(super) fn mode3_for(name: &str) -> &'static str {
    match name {
        "Eldrazi Confluence" => "Some(ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::CreaturesThenNonlandPermanentsV1 { creatures: 2, permanents: 0 }), effect: crate::eternal_legacy_e5b_v1::eldrazi_confluence_2 })",
        _ => "None",
    }
}

/// Eldrazi Confluence's remaining mode choices, in
/// `eternal_legacy_e5b_v1::CONFLUENCE_MODES` order.
pub(super) fn additional_modes_for(name: &str) -> &'static str {
    match name {
        "Eldrazi Confluence" => "&[ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::CreaturesThenNonlandPermanentsV1 { creatures: 1, permanents: 2 }), effect: crate::eternal_legacy_e5b_v1::eldrazi_confluence_3 }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::CreaturesThenNonlandPermanentsV1 { creatures: 1, permanents: 1 }), effect: crate::eternal_legacy_e5b_v1::eldrazi_confluence_4 }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::CreaturesThenNonlandPermanentsV1 { creatures: 1, permanents: 0 }), effect: crate::eternal_legacy_e5b_v1::eldrazi_confluence_5 }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::CreaturesThenNonlandPermanentsV1 { creatures: 0, permanents: 3 }), effect: crate::eternal_legacy_e5b_v1::eldrazi_confluence_6 }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::CreaturesThenNonlandPermanentsV1 { creatures: 0, permanents: 2 }), effect: crate::eternal_legacy_e5b_v1::eldrazi_confluence_7 }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::CreaturesThenNonlandPermanentsV1 { creatures: 0, permanents: 1 }), effect: crate::eternal_legacy_e5b_v1::eldrazi_confluence_8 }, ModeDef { target_spec: TargetSpec::None, effect: crate::eternal_legacy_e5b_v1::eldrazi_confluence_9 }]",
        _ => "&[]",
    }
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Amped Raptor" => &["Keywords::FIRST_STRIKE"],
        "Eldrazi Linebreaker" => &["Keywords::TRAMPLE"],
        "Leyline Binding" => &["Keywords::FLASH"],
        "Ocelot Pride" => &["Keywords::FIRST_STRIKE", "Keywords::LIFELINK"],
        "Phelia, Exuberant Shepherd" => &["Keywords::FLASH"],
        "Quantum Riddler" => &["Keywords::FLYING"],
        "Reality Smasher" => &["Keywords::TRAMPLE", "Keywords::HASTE"],
        _ => super::build_eternal_e6a_v1::keywords_for(name),
    }
}

/// Shapeshifter tokens have changeling.
pub(super) fn changeling_for(name: &str) -> bool {
    name == "Shapeshifter Token"
}

/// Single printed mana abilities that are not "tap: add one".
pub(super) fn mana_ability_def_for(name: &str) -> &'static str {
    match name {
        // "Sacrifice this token: Add {C}."
        "Eldrazi Scion Token" => "Some(ManaAbilityDef { cost: ManaAbilityCostDef::SacrificeSelf, amount: ManaAbilityAmountDef::Fixed(1), controller_damage: 0, max_activations_per_turn: None })",
        _ => super::build_eternal_e6b_v1::mana_ability_def_for(name),
    }
}

/// Additional mana abilities beyond the primary one.
pub(super) fn additional_mana_abilities_for(name: &str) -> &'static str {
    match name {
        // Lazotep Quarry: "{T}, Sacrifice a creature: Add one mana of any
        // color."
        "Lazotep Quarry" => "&[AdditionalManaAbilityDef { colors: &[ManaColor::W, ManaColor::U, ManaColor::B, ManaColor::R, ManaColor::G], mana_cost: Cost { pips: &[], generic: 0, x_count: 0 }, ability: ManaAbilityDef { cost: ManaAbilityCostDef::TapSelfSacrificeControlledCreatureV1, amount: ManaAbilityAmountDef::Fixed(1), controller_damage: 0, max_activations_per_turn: None } }]",
        _ => super::build_eternal_e6a_v1::additional_mana_abilities_for(name),
    }
}

/// Alternative costs. Quantum Riddler: "Warp {1}{U}". Wastescape
/// Battlemage's "{1}{U}" kicker is its mana cost plus {1}{U}, cast as an
/// alternative cost so it combines with its {G} kicker.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Quantum Riddler" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Colored(ManaColor::U)], generic: 1, x_count: 0 })], condition: AltCostCondition::WarpFromHand })",
        "Wastescape Battlemage" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Colored(ManaColor::C), Pip::Colored(ManaColor::U)], generic: 2, x_count: 0 })], condition: AltCostCondition::Always })",
        _ => super::build_eternal_e6a_v1::alt_cost_for(name),
    }
}

/// Wastescape Battlemage's "Kicker {G}".
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    match name {
        "Wastescape Battlemage" => Some("{G}"),
        _ => super::build_eternal_e6a_v1::kicker_cost_for(name),
    }
}

/// Phlage: "Escape--{R}{R}{W}{W}, Exile five other cards from your
/// graveyard."
pub(super) fn escape_for(name: &str) -> String {
    match name {
        "Phlage, Titan of Fire's Fury" => {
            let (pips, generic, x_count) = super::parse_cost("{R}{R}{W}{W}");
            format!(
                "Some(EscapeDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }}), CostComponent::ExileOtherCardsFromOwnGraveyard(5)] }})",
                pips.join(", ")
            )
        }
        _ => "None".to_string(),
    }
}

/// Ajani, Nacatl Pariah's back face, a red and white planeswalker.
pub(super) fn transform_face_for(name: &str) -> &'static str {
    match name {
        "Ajani, Nacatl Pariah" => "Some(TransformFaceDef { name: \"Ajani, Nacatl Avenger\", types: &[CardType::Planeswalker], subtypes: &[Subtype::Ajani], colors: &[ManaColor::R, ManaColor::W], power: None, toughness: None, keywords: Keywords::NONE })",
        _ => super::build_eternal_e6a_v1::transform_face_for(name),
    }
}

pub(super) fn transform_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Ajani, Nacatl Pariah" => Some("Ajani, Nacatl Avenger"),
        _ => super::build_eternal_e6a_v1::transform_face_name_for(name),
    }
}

/// Card-database tokens for the triggered abilities in
/// `eternal_legacy_e5b_v1::triggers_for`, its queued triggers and its
/// statics.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Up the Beanstalk" => "etb:draw:1;cast_spell_mv_at_least_5:draw:1",
        "Stifle" => "spell:counter_target_activated_or_triggered_ability",
        "Consign to Memory" => {
            "replicate:{1}:announced_as_x;spell:counter_target_triggered_ability_or_colorless_spell"
        }
        "Galvanic Discharge" => "spell:get_energy_3:pay_any_energy:damage_target_equal_paid",
        "Wrath of the Skies" => {
            "spell:get_energy_x:pay_any_energy:destroy_artifacts_creatures_enchantments_mv_at_most_paid"
        }
        "Terminus" => {
            "spell:all_creatures_to_owners_library_bottom_owner_orders;miracle:{W}:first_draw_reveal_choice:trigger_pay_then_cast"
        }
        "Eldrazi Confluence" => {
            "modes:choose_three_with_repeats:plus3_minus3|flicker_tapped_owner|eldrazi_scion"
        }
        "Ajani, Nacatl Pariah" => {
            "etb:create_cat_warrior_2_1;other_cats_you_control_die_batch:may_exile_return_transformed;face1_zero:reflexive_if_other_red_permanent:damage_any_target_equal_creatures;face1_minus4:each_opponent_keeps_artifact_creature_enchantment_planeswalker_sacrifices_rest"
        }
        "Amped Raptor" => {
            "etb:get_energy_2:if_cast_from_hand_exile_until_nonland:may_pay_energy_equal_mana_value_cast"
        }
        "Leyline Binding" => {
            "domain:cost_generic_less_per_basic_land_type;etb:target_opponent_nonland_permanent:exile_until_source_leaves"
        }
        "Static Prison" => {
            "etb:target_opponent_nonland_permanent:exile_until_source_leaves:get_energy_2;your_first_main_phase:sacrifice_unless_pay_energy_1"
        }
        "Guide of Souls" => {
            "other_controlled_creature_enters:gain_life_1:get_energy_1;controller_attacks:may_pay_energy_3:reflexive_target_attacking_creature:two_plus_one_counters_flying_counter_becomes_angel"
        }
        "Eldrazi Linebreaker" => {
            "beginning_of_your_combat:target_controlled_creature:haste_plus_x_0_x_eldrazi_you_control"
        }
        "Ocelot Pride" => {
            "ascend;your_end_step_if_gained_life:create_cat_1_1:if_city_blessing_copy_each_token_entered_this_turn"
        }
        "Phelia, Exuberant Shepherd" => {
            "attacks:up_to_one_other_target_nonland_permanent:exile:return_next_end_step_under_owner:counter_if_yours"
        }
        "Phlage, Titan of Fire's Fury" => {
            "etb:sacrifice_unless_escaped;etb_or_attacks:any_target_damage_3_gain_life_3"
        }
        "Quantum Riddler" => {
            "etb:draw:1;static:draw_replacement_plus_one_while_hand_at_most_1;warp_next_end_step:exile_cast_later_turn"
        }
        "Reality Smasher" => {
            "becomes_target_of_opponent_spell:counter_unless_controller_discards_card"
        }
        "Sand Scout" => {
            "etb_if_opponent_more_lands:search_desert_to_battlefield_tapped;land_cards_to_your_graveyard_once_each_turn:create_sand_warrior"
        }
        "Thought-Knot Seer" => {
            "etb:target_opponent:reveal_hand:choose_nonland:exile;leaves:target_opponent:draw:1"
        }
        "Petrified Hamlet" => {
            "etb:choose_land_card_name;static:named_sources_non_mana_abilities_cant_be_activated;static:named_lands_tap_add_c"
        }
        "Voice of Victory" => {
            "attacks:mobilize:2:sacrifice_next_end_step;static:opponents_cant_cast_spells_your_turn"
        }
        "Wastescape Battlemage" => {
            "cast_if_kicked_g:target_opponent_artifact_or_enchantment:exile;cast_if_kicked_1u:target_opponent_creature:return_to_hand"
        }
        "Eye of Ugin" => "static:colorless_eldrazi_spells_cost_generic_less:2",
        "It That Heralds the End" => {
            "static:colorless_spells_mv_at_least_7_cost_generic_less:1;static:other_colorless_creatures_you_control_plus_1_plus_1"
        }
        "Eldrazi Temple" => "mana:tap_add_cc_for_colorless_eldrazi_spells",
        "Lazotep Quarry" => "zombie_copy:4_4_black_zombie",
        _ => super::build_eternal_e6a_v1::trigger_recipe_for(name),
    }
}
