//! Card-database recipes for the eternal catalog's Vintage-only sideboard
//! cards (wave S1c of issue #365). E8a's tables fall through to these, and
//! these fall through to E9b's (E9a's for alternative costs); runtime
//! programs live in `src/eternal_sideboard_s1c_v1.rs`. Every row is keyed by
//! a name that occurs only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special, StackSpellFilter,
};
use AbilityCostRecipe::{DiscardSelf, ManaCost};

/// A cycling ability (702.29): activated from hand, discarding the card as
/// part of its cost, to draw a card.
const fn cycling(cost: &'static [AbilityCostRecipe]) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect: AbilityEffectRecipe::DrawCards(1),
        activation_zone: "Hand",
        sorcery_speed_only: false,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

/// Rebuild: "Cycling {2}".
const REBUILD: [ActivatedAbilityRecipe; 1] = [cycling(&[ManaCost("{2}"), DiscardSelf])];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Rebuild" => &REBUILD,
        _ => super::build_eternal_e9b_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs. "This spell can't be countered" (Long Goodbye,
/// Lithomantic Barrage) is the card's `cant_be_countered` mechanic.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Spell Snare" => Special::CounterTarget(StackSpellFilter::ManaValueTwoV1),
        "Mystical Dispute" => Special::CounterUnlessPaysGeneric {
            filter: StackSpellFilter::Any,
            generic: 3,
        },
        "Hurkyl's Recall" => spell(
            "TargetSpec::AnyPlayer",
            "crate::eternal_sideboard_s1c_v1::hurkyls_recall",
        ),
        "Rebuild" => spell(
            "TargetSpec::None",
            "crate::eternal_sideboard_s1c_v1::rebuild",
        ),
        "Virtue's Ruin" => spell(
            "TargetSpec::None",
            "crate::eternal_sideboard_s1c_v1::virtues_ruin",
        ),
        "Feed the Swarm" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::OpponentCreatureOrEnchantmentV1)",
            "crate::eternal_sideboard_s1c_v1::feed_the_swarm",
        ),
        "Lithomantic Barrage" => spell(
            "TargetSpec::CreatureOrPlaneswalker",
            "crate::eternal_sideboard_s1c_v1::lithomantic_barrage",
        ),
        "Long Goodbye" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::CreatureOrPlaneswalkerManaValueAtMostV1(3))",
            "crate::eternal_sideboard_s1c_v1::long_goodbye",
        ),
        // Its blue-card exile is `alt_cost_for` below.
        "Snapback" => spell(
            "TargetSpec::Creature",
            "crate::eternal_sideboard_s1c_v1::snapback",
        ),
        "Dark Petition" => spell(
            "TargetSpec::None",
            "crate::eternal_sideboard_s1c_v1::dark_petition",
        ),
        _ => return super::build_eternal_e9b_v1::special_for(name),
    })
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Argentum Masticore" => &["Keywords::FIRST_STRIKE"],
        "Wurmcoil Engine" => &["Keywords::DEATHTOUCH", "Keywords::LIFELINK"],
        "Deathtouch Wurm Token" => &["Keywords::DEATHTOUCH"],
        "Lifelink Wurm Token" => &["Keywords::LIFELINK"],
        "Hullbreacher" => &["Keywords::FLASH"],
        "Sphinx of the Steel Wind" => &[
            "Keywords::FLYING",
            "Keywords::FIRST_STRIKE",
            "Keywords::VIGILANCE",
            "Keywords::LIFELINK",
        ],
        _ => super::build_eternal_e9b_v1::keywords_for(name),
    }
}

/// Alternative costs. Snapback: "You may exile a blue card from your hand
/// rather than pay this spell's mana cost."
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Snapback" => "Some(AltCostDef { components: &[CostComponent::ExileCardFromHandWithColorV1(ManaColor::U)], condition: AltCostCondition::Always })",
        _ => super::build_eternal_e9a_v1::alt_cost_for(name),
    }
}

/// Generic cost reductions. Mystical Dispute: "This spell costs {2} less to
/// cast if it targets a blue spell."
pub(super) fn generic_cost_reduction_for(name: &str) -> &'static str {
    match name {
        "Mystical Dispute" => "Some(GenericCostReductionDef { generic_per_count: 2, count: DynamicCountDef::SpellTargetsBlueSpellV1 })",
        _ => "None",
    }
}

/// Descriptive tokens for the triggered abilities in
/// `eternal_sideboard_s1c_v1::triggers_for` and its statics.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Wurmcoil Engine" => "dies:create_token:Deathtouch Wurm Token;create_token:Lifelink Wurm Token",
        "Argentum Masticore" => "upkeep:sacrifice_unless_discard;discard_this_way:destroy_target_opponent_nonland_permanent_mana_value_at_most_discarded;static:protection_from_multicolored",
        "Light of Day" => "static:black_creatures_cant_attack_or_block",
        "Urborg, Tomb of Yawgmoth" => "static:each_land_is_a_swamp",
        "Hullbreacher" => "replacement:opponent_draw_except_first_in_draw_step:create_treasure",
        "Sphinx of the Steel Wind" => "static:protection_from_red_and_green",
        _ => super::build_eternal_e9b_v1::trigger_recipe_for(name),
    }
}