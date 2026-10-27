//! Card-database recipes for the eternal catalog's Vintage cards (wave E4:
//! Jewel Shops and Mono-White Initiative). `build_eternal_v1` falls through
//! to these tables; runtime programs live in `src/eternal_vintage_v1.rs`.
//! Every row is keyed by a name that occurs only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special, StackSpellFilter,
};
use AbilityCostRecipe::{ManaCost, SacrificeSelf, Tap};

/// Strip Mine: "{T}, Sacrifice this land: Destroy target land." Its
/// `{T}: Add {C}` is the ordinary mana ability.
const STRIP_MINE: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[Tap, SacrificeSelf],
    effect: AbilityEffectRecipe::DestroyTarget,
    activation_zone: "Battlefield",
    sorcery_speed_only: false,
    target_spec: "Land",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Karakas: "{T}: Return target legendary creature to its owner's hand."
/// Its `{T}: Add {W}` is the ordinary mana ability.
const KARAKAS: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[Tap],
    effect: AbilityEffectRecipe::MoveAllTargetsToHand,
    activation_zone: "Battlefield",
    sorcery_speed_only: false,
    target_spec: "LegendaryCreature",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

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

/// Manifold Key: "{1}, {T}: Untap another target artifact." and "{3},
/// {T}: Target creature can't be blocked this turn."
const MANIFOLD_KEY: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[ManaCost("{1}"), Tap],
        AbilityEffectRecipe::UntapTarget,
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::AnotherArtifactV1)",
    ),
    instant(
        &[ManaCost("{3}"), Tap],
        AbilityEffectRecipe::Program {
            name: "target_creature_cant_be_blocked_this_turn",
            effect: "EffectOp::GrantKeywordTargetUntilEndOfTurn { object: ObjectRef::Target(0), keyword: Keywords::CANT_BE_BLOCKED }",
        },
        "Creature",
    ),
];

/// Vexing Bauble: "{1}, {T}, Sacrifice this artifact: Draw a card."
const VEXING_BAUBLE: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}"), Tap, SacrificeSelf],
    AbilityEffectRecipe::DrawCards(1),
    "None",
)];

/// Time Vault: "{T}: Take an extra turn after this one."
const TIME_VAULT: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap],
    AbilityEffectRecipe::StandardProgram("crate::extra_turn_v1::recipe"),
    "None",
)];

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

/// Karn, the Great Creator: "+1: Until your next turn, up to one target
/// noncreature artifact becomes an artifact creature with power and
/// toughness each equal to its mana value." and "-2: You may reveal an
/// artifact card you own from outside the game or choose a face-up artifact
/// card you own in exile. Put that card into your hand."
const KARN: [ActivatedAbilityRecipe; 2] = [
    loyalty(
        &[AbilityCostRecipe::Loyalty(1)],
        "crate::eternal_vintage_v1::karn_plus_one",
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::UpToOneNoncreatureArtifactV1)",
    ),
    loyalty(
        &[AbilityCostRecipe::Loyalty(-2)],
        "crate::eternal_vintage_v1::karn_minus_two",
        "None",
    ),
];

/// Tezzeret, Cruel Captain: "0: Untap target artifact or creature. If it's
/// an artifact creature, put a +1/+1 counter on it.", "-3: Search your
/// library for an artifact card with mana value 1 or less, reveal it, put
/// it into your hand, then shuffle." and "-7: You get an emblem with ..."
const TEZZERET: [ActivatedAbilityRecipe; 3] = [
    loyalty(
        &[AbilityCostRecipe::Loyalty(0)],
        "crate::eternal_vintage_v1::tezzeret_zero",
        "AnotherArtifactOrCreature",
    ),
    loyalty(
        &[AbilityCostRecipe::Loyalty(-3)],
        "crate::eternal_vintage_v1::tezzeret_minus_three",
        "None",
    ),
    loyalty(
        &[AbilityCostRecipe::Loyalty(-7)],
        "crate::eternal_vintage_v1::tezzeret_minus_seven",
        "None",
    ),
];

/// Urza's Saga's chapter II ability: "{2}, {T}: Create a 0/0 colorless
/// Construct artifact creature token with 'This token gets +1/+1 for each
/// artifact you control.'" The runtime offers it once chapter II resolved.
const URZAS_SAGA: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{2}"), Tap],
    AbilityEffectRecipe::CreateToken("Karn Construct Token"),
    "None",
)];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Karn, the Great Creator" => &KARN,
        "Tezzeret, Cruel Captain" => &TEZZERET,
        "Urza's Saga" => &URZAS_SAGA,
        "Strip Mine" => &STRIP_MINE,
        "Karakas" => &KARAKAS,
        "Manifold Key" => &MANIFOLD_KEY,
        "Vexing Bauble" => &VEXING_BAUBLE,
        "Time Vault" => &TIME_VAULT,
        _ => super::build_eternal_lurrus_v1::activated_ability_recipes_for(name),
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Ancestral Recall" => Special::TargetPlayerDraw { draw: 3 },
        "Mental Misstep" => Special::CounterTarget(StackSpellFilter::ManaValueOneV1),
        // Its sacrifice is `additional_cost_for` below.
        "Tinker" => Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_vintage_v1::tinker",
        },
        // Both enter through a choice made as they resolve.
        "Phyrexian Metamorph" => Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_vintage_v1::phyrexian_metamorph",
        },
        "Anointed Peacekeeper" => Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_vintage_v1::anointed_peacekeeper",
        },
        "Paradoxical Outcome" => Special::StandardProgram {
            target_spec: "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::AnyNumberControlledNonlandNontokenPermanentsV1)",
            program: "crate::eternal_vintage_v1::paradoxical_outcome",
        },
        // The extra-turn scheduler owns "Take an extra turn after this one"
        // for Temporal Manipulation; Time Walk is admitted beside it.
        "Time Walk" => Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::extra_turn_v1::recipe",
        },
        _ => return super::build_eternal_lurrus_v1::special_for(name),
    })
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Archon of Emeria" | "Chancellor of the Annex" | "Clarion Conqueror" => {
            &["Keywords::FLYING"]
        }
        "Anointed Peacekeeper" => &["Keywords::VIGILANCE"],
        _ => super::build_eternal_lurrus_v1::keywords_for(name),
    }
}

/// Card-database tokens for the triggered abilities in
/// `eternal_vintage_v1::triggers_for`.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Mana Crypt" => "upkeep:controller_only:flip_coin:lost:source_damages_controller:3",
        "Mana Vault" => {
            "upkeep:controller_only:may_pay:{4}:untap_source_incarnation;draw_step_began:controller:if_source_tapped:source_damages_controller:1"
        }
        "Time Vault" => {
            "end_step_before_controllers_turn:if_source_tapped:may_skip_that_turn:untap_source_when_skipped"
        }
        "Coveted Jewel" => {
            "etb:draw:3;opponent_creatures_attack_controller_unblocked:attacking_player_draws_3_gains_control_of_source_untap_it"
        }
        "Vexing Bauble" => "any_player_casts_spell:if_no_mana_spent:counter_that_spell",
        "Void Mirror" => "any_player_casts_spell:if_no_colored_mana_spent:counter_that_spell",
        "White Plume Adventurer" => {
            "etb:take_initiative;opponent_upkeep:untap_chosen_controlled_creature_or_all_if_completed_dungeon"
        }
        "Seasoned Dungeoneer" => {
            "etb:take_initiative;controller_attacks:target_attacking_cleric_rogue_warrior_or_wizard:protection_from_creatures_eot:explores"
        }
        "Tezzeret, Cruel Captain" => {
            "controlled_artifact_enters:loyalty_counter_on_source:1;emblem:beginning_of_owner_combat:target_controlled_artifact:plus_one_counters:3:noncreature_becomes_0_0_robot_artifact_creature"
        }
        "Chancellor of the Annex" => {
            "opening_hand:may_reveal_at_first_upkeep:opponent_first_spell_of_game:counter_unless_pays:{1};opponent_casts_spell:counter_unless_pays:{1}"
        }
        "Witch Enchanter" => "etb:target_opponent_artifact_or_enchantment:destroy",
        "Karn, the Great Creator" => {
            "static:opponents_artifact_activated_abilities_cant_be_activated"
        }
        "Anointed Peacekeeper" => {
            "as_enters:look_at_opponent_hand:choose_card_name;static:opponent_spells_named_cost_generic_more:2;static:named_non_mana_activations_cost_generic_more:2"
        }
        "Phyrexian Metamorph" => "as_enters:may_copy_artifact_or_creature:plus_artifact_type",
        "Urza's Saga" => {
            "saga:i:gains_tap_add_c;ii:gains_construct_ability;iii:search_artifact_mana_cost_0_or_1_to_battlefield"
        }
        _ => super::build_eternal_lurrus_v1::trigger_recipe_for(name),
    }
}

/// Single printed mana abilities that are not "tap: add one".
pub(super) fn mana_ability_def_for(name: &str) -> &'static str {
    match name {
        // "{T}, Sacrifice this artifact: Add three mana of any one color."
        "Black Lotus" => "Some(ManaAbilityDef { cost: ManaAbilityCostDef::TapAndSacrificeSelf, amount: ManaAbilityAmountDef::Fixed(3), controller_damage: 0, max_activations_per_turn: None })",
        // "{T}: Add three mana of any one color."
        "Coveted Jewel" => "Some(ManaAbilityDef { cost: ManaAbilityCostDef::TapSelf, amount: ManaAbilityAmountDef::Fixed(3), controller_damage: 0, max_activations_per_turn: None })",
        // "{T}: Add {C}{C}{C}. Spend this mana only to cast artifact
        // spells." The runtime turns the three mana into restricted units.
        "Mishra's Workshop" => "Some(ManaAbilityDef { cost: ManaAbilityCostDef::TapSelf, amount: ManaAbilityAmountDef::Fixed(3), controller_damage: 0, max_activations_per_turn: None })",
        _ => super::build_eternal_e5b_v1::mana_ability_def_for(name),
    }
}

/// Tap-for-colorless rocks and Tolarian Academy: the amount one activation
/// of the primary mana ability adds. The payment planner and the explicit
/// mana action read the same value.
pub(super) fn conditional_tap_yield_for(name: &str) -> &'static str {
    match name {
        // "{T}: Add {C}{C}."
        "Sol Ring" | "Mana Crypt" => "Some(DynamicValueDef::Fixed(2))",
        // "{T}: Add {C}{C}{C}."
        "Mana Vault" => "Some(DynamicValueDef::Fixed(3))",
        // "{T}: Add {U} for each artifact you control."
        "Tolarian Academy" => {
            "Some(DynamicValueDef::ControlledPermanentsWithType(CardType::Artifact))"
        }
        _ => "None",
    }
}

pub(super) fn conditional_tap_yield_program_for(name: &str) -> Option<&'static str> {
    match name {
        "Sol Ring" => Some("mana_ability_add_sol_ring"),
        "Mana Crypt" => Some("mana_ability_add_mana_crypt"),
        "Mana Vault" => Some("mana_ability_add_mana_vault"),
        "Tolarian Academy" => Some("mana_ability_add_tolarian_academy"),
        _ => None,
    }
}

/// Mandatory additional costs. Tinker: "As an additional cost to cast this
/// spell, sacrifice an artifact."
pub(super) fn additional_cost_for(name: &str) -> &'static str {
    match name {
        "Tinker" => {
            "Some(&[CostComponent::SacrificeControlled { count: 1, filter: PermanentFilter::Artifact }])"
        }
        _ => super::build_eternal_e6a_v1::additional_cost_for(name),
    }
}

/// Saga chapters. Urza's Saga's first two chapters give it abilities that
/// the runtime gates on the chapters resolved.
pub(super) fn saga_for(name: &str) -> &'static str {
    match name {
        "Urza's Saga" => "Some(SagaDef { chapter_effects: &[crate::eternal_vintage_v1::urzas_saga_one, crate::eternal_vintage_v1::urzas_saga_two, crate::eternal_vintage_v1::urzas_saga_three] })",
        _ => "None",
    }
}

/// Modal double-faced cards' land faces, carried as the card's alternate
/// face: Witch Enchanter // Witch-Blessed Meadow.
pub(super) fn transform_face_for(name: &str) -> &'static str {
    match name {
        "Witch Enchanter" => "Some(TransformFaceDef { name: \"Witch-Blessed Meadow\", types: &[CardType::Land], subtypes: &[], colors: &[], power: None, toughness: None, keywords: Keywords::NONE })",
        _ => super::build_eternal_e5b_v1::transform_face_for(name),
    }
}

pub(super) fn transform_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Witch Enchanter" => Some("Witch-Blessed Meadow"),
        _ => super::build_eternal_e5b_v1::transform_face_name_for(name),
    }
}
