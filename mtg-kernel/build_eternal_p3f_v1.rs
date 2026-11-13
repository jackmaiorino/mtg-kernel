//! Card-database recipes for the eternal catalog's Premodern sideboard wave
//! P3f. Wave P3e's tables fall through to these, and these to wave S1e's
//! (`build_eternal_s1e_v1.rs`), the tail of the recipe chains. Runtime
//! programs live in `src/eternal_premodern_p3f_v1.rs`. Every row is keyed by
//! a name that occurs only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{ManaCost, Tap};

/// Phyrexian Processor: "{4}, {T}: Create an X/X black Phyrexian Minion
/// creature token, where X is the life paid as this artifact entered."
const PHYREXIAN_PROCESSOR: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{4}"), Tap],
    effect: AbilityEffectRecipe::StandardProgram(
        "crate::eternal_premodern_p3f_v1::processor_token",
    ),
    activation_zone: "Battlefield",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Phyrexian Processor" => &PHYREXIAN_PROCESSOR,
        _ => super::build_eternal_s1e_v1::activated_ability_recipes_for(name),
    }
}

/// Contagion: "Distribute two -2/-1 counters among one or two target
/// creatures." Lurking Skirge's trigger and Phyrexian Processor's payment
/// are hooks of `eternal_premodern_p3f_v1`.
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Contagion" => Some(Special::StandardProgram {
            target_spec: "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::DividedTwoCountersCreaturesV1)",
            program: "crate::eternal_premodern_p3f_v1::contagion",
        }),
        _ => super::build_eternal_s1e_v1::special_for(name),
    }
}

/// No P3f card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_s1e_v1::kicker_cost_for(name)
}

/// No P3f card is an Aura or Equipment.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_s1e_v1::attachment_for(name)
}

/// No P3f card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_s1e_v1::flashback_for(name)
}

/// Contagion: "You may pay 1 life and exile a black card from your hand
/// rather than pay this spell's mana cost." Force of Will's alternative
/// cost, with black for blue.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Contagion" => "Some(AltCostDef { components: &[CostComponent::PayLife(1), CostComponent::ExileCardFromHandWithColorV1(ManaColor::B)], condition: AltCostCondition::Always })",
        _ => super::build_eternal_s1e_v1::alt_cost_for(name),
    }
}

/// No P3f card has a printed evergreen keyword: Lurking Skirge gains flying
/// as it becomes a creature (`eternal_premodern_p3f_v1::granted_keywords`).
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    super::build_eternal_s1e_v1::keywords_for(name)
}

/// Card-database tokens for this wave's abilities. Lurking Skirge's trigger
/// is queued by the module's hook.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Contagion" => "alt_cost:pay_1_life_exile_black_card_from_hand;spell:distribute_two_minus_2_minus_1_counters_among_one_or_two_target_creatures",
        "Lurking Skirge" => "creature_to_opponent_graveyard_from_battlefield:if_enchantment:becomes_3_2_phyrexian_imp_creature_with_flying",
        "Phyrexian Processor" => "as_enters:pay_any_amount_of_life;activated:create_x_x_black_phyrexian_minion_x_life_paid",
        "Phyrexian Minion Token" => "characteristic:power_toughness_equal_life_paid",
        _ => super::build_eternal_s1e_v1::trigger_recipe_for(name),
    }
}
