//! Card-database recipes for the eternal catalog's Vintage sideboard wave
//! S1f. Wave S1e's tables fall through to these, and these fall through to
//! wave S1g's (`build_eternal_s1g_v1.rs`), the tail of the recipe chains.
//! Runtime programs live in `src/eternal_sideboard_s1f_v1.rs`. Every row is
//! keyed by a name that occurs only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};

/// The Junk token: "{T}, Sacrifice this artifact: Exile the top card of
/// your library. You may play that card this turn. Activate only as a
/// sorcery." The engine's impulse draw gives the exiled card its
/// until-end-of-turn play permission, so costs and timing still apply.
const JUNK: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[AbilityCostRecipe::Tap, AbilityCostRecipe::SacrificeSelf],
    effect: AbilityEffectRecipe::ImpulseDraw {
        count: 1,
        duration: "EndOfTurn",
    },
    activation_zone: "Battlefield",
    sorcery_speed_only: true,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Junk Token" => &JUNK,
        _ => super::build_eternal_s1g_v1::activated_ability_recipes_for(name),
    }
}

/// Overencumbered: "Enchant opponent." The Aura spell targets an opponent
/// and enters enchanting that player; its triggers are queued by
/// `eternal_sideboard_s1f_v1`.
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Overencumbered" => Some(Special::StandardProgram {
            target_spec: "TargetSpec::TargetOpponent",
            program: "crate::eternal_sideboard_s1f_v1::overencumbered",
        }),
        _ => super::build_eternal_s1g_v1::special_for(name),
    }
}

/// No S1f card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_s1g_v1::kicker_cost_for(name)
}

/// Overencumbered enchants a player, which `AttachmentDef` doesn't
/// describe; the module records it.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_s1g_v1::attachment_for(name)
}

/// No S1f card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_s1g_v1::flashback_for(name)
}

/// No S1f card has an alternative cost.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_s1g_v1::alt_cost_for(name)
}

/// No S1f card has an evergreen keyword.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    super::build_eternal_s1g_v1::keywords_for(name)
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Mycosynth Lattice" => "static:all_permanents_are_artifacts;static:all_objects_are_colorless;static:players_spend_mana_as_though_any_color",
        "Overencumbered" => "enchant:opponent;etb:enchanted_opponent_creates_clue_food_junk;combat:enchanted_opponents_turn_may_pay_one_per_artifact_else_creatures_cant_attack",
        "Junk Token" => "activated:tap_sacrifice_exile_top_card_play_this_turn_sorcery_speed",
        _ => super::build_eternal_s1g_v1::trigger_recipe_for(name),
    }
}
