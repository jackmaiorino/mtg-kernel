//! Card-database recipes for the eternal catalog's Premodern sideboard wave
//! P3e. This module is the tail of the recipe chains: wave P3d's tables
//! fall through to these, and its own defaults are the chains' base values.
//! Runtime programs live in `src/eternal_premodern_p3e_v1.rs`. Every row is
//! keyed by a name that occurs only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{ManaCost, Tap};

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

/// Kor Haven: "{1}{W}, {T}: Prevent all combat damage that would be dealt
/// by target attacking creature this turn." Its "{T}: Add {C}." is the
/// registry's mana ability.
const KOR_HAVEN: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}{W}"), Tap],
    AbilityEffectRecipe::StandardProgram("crate::eternal_premodern_p3e_v1::kor_haven"),
    "AttackingCreature",
)];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Kor Haven" => &KOR_HAVEN,
        _ => &[],
    }
}

/// No P3e card is a spell with a program: Absolute Law and Winter Orb are
/// statics (`eternal_premodern_p3e_v1::protection_mask` and
/// `untap_limited`).
pub(super) fn special_for(_name: &str) -> Option<Special> {
    None
}

/// No P3e card has kicker.
pub(super) fn kicker_cost_for(_name: &str) -> Option<&'static str> {
    None
}

/// No P3e card is an Aura or Equipment.
pub(super) fn attachment_for(_name: &str) -> &'static str {
    "None"
}

/// No P3e card has flashback.
pub(super) fn flashback_for(_name: &str) -> String {
    "None".to_string()
}

/// No P3e card has an alternative cost.
pub(super) fn alt_cost_for(_name: &str) -> &'static str {
    "None"
}

/// No P3e card has a printed evergreen keyword.
pub(super) fn keywords_for(_name: &str) -> &'static [&'static str] {
    &[]
}

/// Card-database tokens for this wave's abilities. None of them is a
/// triggered ability.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Absolute Law" => "static:all_creatures_have_protection_from_red",
        "Kor Haven" => "activated:prevent_combat_damage_dealt_by_target_attacking_creature",
        "Winter Orb" => "static:while_untapped_players_untap_at_most_one_land_during_untap_steps",
        _ => "none",
    }
}
