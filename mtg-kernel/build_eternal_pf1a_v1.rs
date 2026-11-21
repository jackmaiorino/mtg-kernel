//! Card-database recipes for the eternal catalog's Pauper field wave Pf1a
//! (permanents with simple text). Wave C6c's tables fall through to these, and
//! these fall through to wave Pf1b's
//! (`build_eternal_pf1b_v1.rs`). Runtime programs live in
//! `src/eternal_pauper_pf1a_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};

/// Vulshok Sorcerer: "{T}: This creature deals 1 damage to any target."
const VULSHOK_SORCERER: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[AbilityCostRecipe::Tap],
    effect: AbilityEffectRecipe::DealDamageAnyTarget(1),
    activation_zone: "Battlefield",
    sorcery_speed_only: false,
    target_spec: "AnyTarget",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Vulshok Sorcerer's tap ability. Jaspera Sentinel's mana ability is
/// `mana_ability_def_for` (wave S1q's table, the chain's tail).
pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Vulshok Sorcerer" => &VULSHOK_SORCERER,
        _ => super::build_eternal_pf1b_v1::activated_ability_recipes_for(name),
    }
}

/// Spell programs. "Enchant creature" Auras enter attached to their target;
/// their statics are `attachment_for` below. Creatures, lands and the
/// artifact need no spell program.
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Lifelink" | "Mask of Law and Grace" => Some(Special::BindTheMonster),
        _ => super::build_eternal_pf1b_v1::special_for(name),
    }
}

/// Kor Sanctifiers: "Kicker {W}".
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    match name {
        "Kor Sanctifiers" => Some("{W}"),
        _ => super::build_eternal_pf1b_v1::kicker_cost_for(name),
    }
}

/// "Enchant creature" Aura statics. Lifelink: "Enchanted creature has
/// lifelink." Mask of Law and Grace's protection from black and from red is
/// `eternal_pauper_pf1a_v1::protection_mask`, read by the engine's protection
/// checks through wave E9a's mask chain.
pub(super) fn attachment_for(name: &str) -> &'static str {
    match name {
        "Lifelink" => "Some(AttachmentDef::AuraCreatureStatic(AuraCreatureStaticDef { power: 0, toughness: 0, keywords: Keywords::LIFELINK, per_controlled_subtype: None }))",
        "Mask of Law and Grace" => "Some(AttachmentDef::AuraCreatureStatic(AuraCreatureStaticDef { power: 0, toughness: 0, keywords: Keywords::NONE, per_controlled_subtype: None }))",
        _ => super::build_eternal_pf1b_v1::attachment_for(name),
    }
}

pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_pf1b_v1::flashback_for(name)
}

pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_pf1b_v1::alt_cost_for(name)
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Primordial Pachyderm" => &["Keywords::REACH", "Keywords::TRAMPLE"],
        "Jaspera Sentinel" => &["Keywords::REACH"],
        "Vulshok Sorcerer" => &["Keywords::HASTE"],
        _ => super::build_eternal_pf1b_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities (their programs are
/// `eternal_pauper_pf1a_v1::triggers_for` and its hooks).
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Wooded Ridgeline" => "enters_tapped;basic_land_types:mountain_forest",
        "Kabira Crossroads" => "enters_tapped;etb:gain_life:2",
        "Jaspera Sentinel" => "mana:tap_self_and_untapped_controlled_creature:any_color",
        "Kor Sanctifiers" => "kicker:W;etb_if_kicked:target_artifact_or_enchantment:destroy",
        "Vulshok Sorcerer" => "activated:tap:damage_1:any_target",
        "Primordial Pachyderm" => "etb:gain_life:2",
        "Lifelink" => "static:enchanted_creature_has_lifelink",
        "Mask of Law and Grace" => "static:enchanted_creature_protection_from_black_and_red",
        "Mycosynth Wellspring" => "etb_and_dies:may_search_basic_land_to_hand",
        "K'un-Lun Warrior" => "etb:may_sacrifice_artifact_or_discard_card:if_you_do_draw:1",
        _ => super::build_eternal_pf1b_v1::trigger_recipe_for(name),
    }
}
