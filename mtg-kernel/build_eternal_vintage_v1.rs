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

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Strip Mine" => &STRIP_MINE,
        "Karakas" => &KARAKAS,
        "Manifold Key" => &MANIFOLD_KEY,
        "Vexing Bauble" => &VEXING_BAUBLE,
        "Time Vault" => &TIME_VAULT,
        _ => &[],
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
        _ => return None,
    })
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Archon of Emeria" | "Chancellor of the Annex" | "Clarion Conqueror" => {
            &["Keywords::FLYING"]
        }
        "Anointed Peacekeeper" => &["Keywords::VIGILANCE"],
        _ => &[],
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
        _ => "none",
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
        _ => "None",
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
        _ => "None",
    }
}
