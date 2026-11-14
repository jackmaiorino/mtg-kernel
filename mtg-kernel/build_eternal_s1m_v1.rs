//! Card-database recipes for the eternal catalog's Legacy sideboard wave
//! S1m. Wave S1l's tables (and wave P1a's second and third modes, wave E5b's
//! other modes) fall through to these, and these fall through to wave P3h's
//! (`build_eternal_p3h_v1.rs`), the tail of the recipe chains. Runtime
//! programs live in `src/eternal_sideboard_s1m_v1.rs`. Every row is keyed by
//! a name that occurs only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};

/// Extinguisher Battleship's station (702.184a): "Tap another untapped
/// creature you control: Put a number of charge counters on this permanent
/// equal to the tapped creature's power. Activate only as a sorcery."
const EXTINGUISHER_BATTLESHIP: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[AbilityCostRecipe::TapOtherUntappedControlledCreature],
    effect: AbilityEffectRecipe::StandardProgram("crate::eternal_sideboard_s1m_v1::station"),
    activation_zone: "Battlefield",
    sorcery_speed_only: true,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Extinguisher Battleship" => &EXTINGUISHER_BATTLESHIP,
        _ => super::build_eternal_p3h_v1::activated_ability_recipes_for(name),
    }
}

/// Fiery Confluence: "Choose three. You may choose the same mode more than
/// once." Its ten multisets of modes are ten modes, in
/// `eternal_legacy_e5b_v1::CONFLUENCE_MODES` order (0: 1 damage to each
/// creature, 1: 2 damage to each opponent, 2: destroy target artifact). The
/// first is the spell's program; the other nine are `mode2_for`,
/// `mode3_for` and `additional_modes_for` below.
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Fiery Confluence" => Some(Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_sideboard_s1m_v1::fiery_confluence_0",
        }),
        _ => super::build_eternal_p3h_v1::special_for(name),
    }
}

/// Fiery Confluence's second mode choice: 1 damage to each creature twice
/// and 2 damage to each opponent.
pub(super) fn mode2_for(name: &str) -> &'static str {
    match name {
        "Fiery Confluence" => "Some(ModeDef { target_spec: TargetSpec::None, effect: crate::eternal_sideboard_s1m_v1::fiery_confluence_1 })",
        _ => "None",
    }
}

/// Fiery Confluence's third mode choice: 1 damage to each creature twice
/// and destroy target artifact.
pub(super) fn mode3_for(name: &str) -> &'static str {
    match name {
        "Fiery Confluence" => "Some(ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::RepeatableArtifactsV1(1)), effect: crate::eternal_sideboard_s1m_v1::fiery_confluence_2 })",
        _ => "None",
    }
}

/// Fiery Confluence's remaining mode choices, one "target artifact" slot
/// per destroy mode.
pub(super) fn additional_modes_for(name: &str) -> &'static str {
    match name {
        "Fiery Confluence" => "&[ModeDef { target_spec: TargetSpec::None, effect: crate::eternal_sideboard_s1m_v1::fiery_confluence_3 }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::RepeatableArtifactsV1(1)), effect: crate::eternal_sideboard_s1m_v1::fiery_confluence_4 }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::RepeatableArtifactsV1(2)), effect: crate::eternal_sideboard_s1m_v1::fiery_confluence_5 }, ModeDef { target_spec: TargetSpec::None, effect: crate::eternal_sideboard_s1m_v1::fiery_confluence_6 }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::RepeatableArtifactsV1(1)), effect: crate::eternal_sideboard_s1m_v1::fiery_confluence_7 }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::RepeatableArtifactsV1(2)), effect: crate::eternal_sideboard_s1m_v1::fiery_confluence_8 }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::RepeatableArtifactsV1(3)), effect: crate::eternal_sideboard_s1m_v1::fiery_confluence_9 }]",
        _ => "&[]",
    }
}

/// No S1m card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_p3h_v1::kicker_cost_for(name)
}

/// No S1m card is an Aura or an Equipment.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_p3h_v1::attachment_for(name)
}

/// No S1m card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_p3h_v1::flashback_for(name)
}

/// No S1m card has an alternative cost.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_p3h_v1::alt_cost_for(name)
}

/// No S1m card has a printed evergreen keyword: Extinguisher Battleship's
/// flying and trample are its station 5+ abilities
/// (`eternal_sideboard_s1m_v1::granted_keywords`).
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    super::build_eternal_p3h_v1::keywords_for(name)
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Extinguisher Battleship" => {
            "etb:destroy_target_noncreature_permanent_then_self_deals_4_each_creature_lki;station:tap_another_untapped_controlled_creature:charge_counters_equal_power_lki:sorcery;station_5:creature_base_10_10_flying_trample:layer_timestamps"
        }
        "Fiery Confluence" => {
            "modes:choose_three_with_repeats:damage_each_creature_1|damage_each_opponent_2|destroy_target_artifact:printed_order:separate_damage_events"
        }
        _ => super::build_eternal_p3h_v1::trigger_recipe_for(name),
    }
}
