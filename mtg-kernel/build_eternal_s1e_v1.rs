//! Card-database recipes for the eternal catalog's Vintage sideboard wave
//! S1e. Wave P3f's tables fall through to these, and these fall through to
//! wave S1f's (`build_eternal_s1f_v1.rs`).
//! Runtime programs live in `src/eternal_sideboard_s1e_v1.rs`. Every row is
//! keyed by a name that occurs only in the eternal registry.

use super::{ActivatedAbilityRecipe, Special};

/// No S1e card has an activated ability.
pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    super::build_eternal_s1f_v1::activated_ability_recipes_for(name)
}

/// Suspend: "Exile target creature and put two time counters on it."
/// Sickening Shoal: "Target creature gets -X/-X until end of turn." Unable
/// to Scream is an enchant creature Aura, cast as Essence Flare is; its
/// statics are `attachment_for` below. Azure Beastbinder's and Portal to
/// Phyrexia's triggers are printed triggers of `eternal_sideboard_s1e_v1`.
pub(super) fn special_for(name: &str) -> Option<Special> {
    let program = match name {
        "Suspend" => "crate::eternal_sideboard_s1e_v1::suspend",
        "Sickening Shoal" => "crate::eternal_sideboard_s1e_v1::sickening_shoal",
        "Unable to Scream" => return Some(Special::BindTheMonster),
        _ => return super::build_eternal_s1f_v1::special_for(name),
    };
    Some(Special::StandardProgram {
        target_spec: "TargetSpec::Creature",
        program,
    })
}

/// No S1e card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_s1f_v1::kicker_cost_for(name)
}

/// Unable to Scream: "Enchant creature." Its static abilities are hooks on
/// the exact host (`eternal_sideboard_s1e_v1`).
pub(super) fn attachment_for(name: &str) -> &'static str {
    match name {
        "Unable to Scream" => "Some(AttachmentDef::AuraCreature { prevents_untap: false })",
        _ => super::build_eternal_s1f_v1::attachment_for(name),
    }
}

/// No S1e card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_s1f_v1::flashback_for(name)
}

/// Sickening Shoal: "You may exile a black card with mana value X from your
/// hand rather than pay this spell's mana cost." Contagion's black-card
/// exile without the life; the exiled card announces X
/// (`eternal_sideboard_s1e_v1::shoal_forced_x`).
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Sickening Shoal" => "Some(AltCostDef { components: &[CostComponent::ExileCardFromHandWithColorV1(ManaColor::B)], condition: AltCostCondition::Always })",
        _ => super::build_eternal_s1f_v1::alt_cost_for(name),
    }
}

/// Printed evergreen keywords. Azure Beastbinder: "Vigilance".
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Azure Beastbinder" => &["Keywords::VIGILANCE"],
        _ => super::build_eternal_s1f_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Azure Beastbinder" => "static:cant_be_blocked_by_power_2_or_greater;attacks:up_to_one_target_opponent_artifact_creature_or_planeswalker_loses_all_abilities_until_your_next_turn_creature_base_2_2",
        "Suspend" => "spell:exile_target_creature_two_time_counters_gains_suspend;suspend_cast_creature_haste",
        "Sickening Shoal" => "alt_cost:exile_black_card_mana_value_x_from_hand;spell:target_creature_minus_x_minus_x_until_end_of_turn",
        "Unable to Scream" => "static:enchanted_creature_loses_all_abilities_toy_artifact_creature_base_0_2;static:face_down_enchanted_creature_cant_be_turned_face_up",
        "Portal to Phyrexia" => "etb:each_opponent_sacrifices_three_creatures;upkeep:target_creature_card_from_a_graveyard_onto_the_battlefield_phyrexian",
        _ => super::build_eternal_s1f_v1::trigger_recipe_for(name),
    }
}
