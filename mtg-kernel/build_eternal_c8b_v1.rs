//! Card-database recipes for the eternal catalog's Vintage Cube wave C8b
//! (Copy creatures, dies copies, Saheeli, and bestow cards). Wave C8a's tables
//! fall through to these, and these fall through to wave C8c's
//! (`build_eternal_c8c_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c8b_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};

/// Saheeli, Sublime Artificer: "-2: Target artifact you control becomes a copy
/// of another target artifact or creature you control until end of turn, except
/// it's an artifact in addition to its other types."
const SAHEELI: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[AbilityCostRecipe::Loyalty(-2)],
    effect: AbilityEffectRecipe::StandardProgram("crate::eternal_cube_c8b_v1::saheeli_minus_two"),
    activation_zone: "Battlefield",
    sorcery_speed_only: true,
    target_spec: "StandardV1(crate::standard_cards_v1::StandardTargetV1::ControlledArtifactThenAnotherControlledArtifactOrCreatureV1)",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Saheeli, Sublime Artificer" => &SAHEELI,
        _ => super::build_eternal_c8c_v1::activated_ability_recipes_for(name),
    }
}

/// Planeswalker entry program mirrors waves C6c and C7a; all other C8b
/// nonlands are ordinary creatures or enchantment creatures entering through
/// the default permanent program (as-enters copy choices for Phantasmal Image
/// and Superior Spider-Man run via `entry_choice_decision` across both cast
/// and non-cast entries).
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Saheeli, Sublime Artificer" => Some(Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_cube_c6c_v1::jace",
        }),
        _ => super::build_eternal_c8c_v1::special_for(name),
    }
}

/// No C8b card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c8c_v1::kicker_cost_for(name)
}

/// Bestow cards keep `attachment = None` (mirroring Nyxborn Hydra) so they are
/// never treated as permanent Auras when unattached; their enchanted-creature
/// bonuses apply through the Bestow hooks in `engine.rs`.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c8c_v1::attachment_for(name)
}

/// No C8b card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c8c_v1::flashback_for(name)
}

/// No C8b card uses `AltCostDef`; Bestow uses `BestowDef`.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c8c_v1::alt_cost_for(name)
}

/// Bestow definitions for Springheart Nantuko ({1}{G}) and Detective's Phoenix
/// ({R}, plus Collect evidence 6 enforced by `eternal_cube_c8b_v1`).
pub(super) fn bestow_for(name: &str) -> String {
    let cost_str = match name {
        "Springheart Nantuko" => "{1}{G}",
        "Detective's Phoenix" => "{R}",
        _ => return "None".to_string(),
    };
    let (pips, generic, x_count) = super::parse_cost(cost_str);
    format!(
        "Some(BestowDef {{ cost: Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }}, target_spec: TargetSpec::Creature }})",
        pips.join(", ")
    )
}

pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Vaultborn Tyrant" => &["Keywords::TRAMPLE"],
        "Detective's Phoenix" => &["Keywords::FLYING", "Keywords::HASTE"],
        _ => super::build_eternal_c8c_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Phantasmal Image" => {
            "as_enters:may_copy_creature:plus_illusion_subtype:gains_becomes_target_sacrifice"
        }
        "Superior Spider-Man" => {
            "as_enters:may_copy_creature_card_in_a_graveyard:name_superior_spider_man:base_4_4:plus_spider_human_hero:when_you_do_exile_that_card"
        }
        "Vaultborn Tyrant" => {
            "trample;etb_self_or_controlled_creature_power_4_or_greater:gain_life:3:draw:1;dies_if_nontoken:create_artifact_token_copy"
        }
        "Saheeli, Sublime Artificer" => {
            "cast_noncreature_spell:create_token:servo;loyalty_minus_2:controlled_artifact_becomes_copy_of_another_controlled_artifact_or_creature_until_end_of_turn:plus_artifact_type"
        }
        "Springheart Nantuko" => {
            "bestow:1g;enchanted_creature_plus_1_plus_1;landfall:if_attached_to_controlled_creature_may_pay_1g_token_copy_else_green_insect_token"
        }
        "Detective's Phoenix" => {
            "bestow:r_collect_evidence_6;flying;haste;enchanted_creature_plus_2_plus_2_flying_haste;cast_from_graveyard_using_bestow"
        }
        _ => super::build_eternal_c8c_v1::trigger_recipe_for(name),
    }
}
