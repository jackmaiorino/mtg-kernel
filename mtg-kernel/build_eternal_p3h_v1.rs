//! Card-database recipes for the eternal catalog's Premodern sideboard wave
//! P3h. This module is the tail of the recipe chains: wave S1m's tables fall
//! through to these, and its own defaults are the chains' base values.
//! Runtime programs live in `src/eternal_premodern_p3h_v1.rs`. Every row is
//! keyed by a name that occurs only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, PermanentFilterRecipe, Special,
};
use AbilityCostRecipe::{DiscardSelf, ManaCost};

/// Overgrown Estate: "Sacrifice a land: You gain 3 life."
const OVERGROWN_ESTATE: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[AbilityCostRecipe::SacrificeControlled {
        count: 1,
        filter: PermanentFilterRecipe::Land,
    }],
    effect: AbilityEffectRecipe::GainLife(3),
    activation_zone: "Battlefield",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Decree of Pain: "Cycling {3}{B}{B}" (702.29): activated from hand,
/// discarding the card as part of its cost, to draw a card. Its "When you
/// cycle this card" trigger is queued as the cost is paid.
const DECREE_OF_PAIN: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{3}{B}{B}"), DiscardSelf],
    effect: AbilityEffectRecipe::DrawCards(1),
    activation_zone: "Hand",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Overgrown Estate" => &OVERGROWN_ESTATE,
        "Decree of Pain" => &DECREE_OF_PAIN,
        _ => &[],
    }
}

/// A spell whose program is a runtime function of `eternal_premodern_p3h_v1`.
const fn spell(program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec: "TargetSpec::None",
        program,
    }
}

/// Decree of Pain and Enlightened Tutor.
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Decree of Pain" => Some(spell("crate::eternal_premodern_p3h_v1::decree_of_pain")),
        "Enlightened Tutor" => Some(spell("crate::eternal_premodern_p3h_v1::enlightened_tutor")),
        _ => None,
    }
}

/// No P3h card has kicker.
pub(super) fn kicker_cost_for(_name: &str) -> Option<&'static str> {
    None
}

/// No P3h card is an Aura or an Equipment.
pub(super) fn attachment_for(_name: &str) -> &'static str {
    "None"
}

/// No P3h card has flashback.
pub(super) fn flashback_for(_name: &str) -> String {
    "None".to_string()
}

/// No P3h card has an alternative cost.
pub(super) fn alt_cost_for(_name: &str) -> &'static str {
    "None"
}

/// No P3h card has a printed evergreen keyword; Ghitu Slinger's echo is
/// wave P2d's `echo_cost`.
pub(super) fn keywords_for(_name: &str) -> &'static [&'static str] {
    &[]
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Overgrown Estate" => "activated:sacrifice_land:gain_life_3",
        "Decree of Pain" => {
            "spell:destroy_all_creatures_no_regeneration_draw_per_destroyed;cycling:3BB;when_cycled:all_creatures_minus_2_minus_2"
        }
        "Enlightened Tutor" => {
            "spell:search_library_artifact_or_enchantment_reveal_shuffle_put_on_top"
        }
        "Auramancer" => "etb:may_return_target_enchantment_card_from_own_graveyard_to_hand",
        "Monk Realist" => "etb:destroy_target_enchantment",
        "Ghitu Slinger" => "echo:2R;etb:deal_2_damage_any_target",
        _ => "none",
    }
}
