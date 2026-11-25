//! Card-database recipes for the eternal catalog's Premodern wave P3i
//! (`docs/design/premodern_shapeshifter_alter_reality_v1.md`, step C2):
//! Alter Reality. Wave Pf2c's tables fall through to these, and this module is
//! the tail of the recipe chains: its own defaults are the chains' base
//! values. Runtime programs live in `src/eternal_premodern_p3i_v1.rs`. Every
//! row is keyed by a name that occurs only in the eternal registry.

use super::{ActivatedAbilityRecipe, Special};

/// Alter Reality has no activated ability.
pub(super) fn activated_ability_recipes_for(_name: &str) -> &'static [ActivatedAbilityRecipe] {
    &[]
}

/// Spell program: Alter Reality targets any spell or permanent and changes all
/// instances of one color word in its text to another indefinitely.
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Alter Reality" => Some(Special::StandardProgram {
            target_spec: "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::SpellOrPermanentV1)",
            program: "crate::eternal_premodern_p3i_v1::alter_reality",
        }),
        _ => None,
    }
}

/// Alter Reality has no kicker.
pub(super) fn kicker_cost_for(_name: &str) -> Option<&'static str> {
    None
}

/// Alter Reality is neither an Aura nor an Equipment.
pub(super) fn attachment_for(_name: &str) -> &'static str {
    "None"
}

/// Alter Reality: "Flashback {1}{U}".
pub(super) fn flashback_for(name: &str) -> String {
    match name {
        "Alter Reality" => {
            let (pips, generic, x_count) = super::parse_cost("{1}{U}");
            format!(
                "Some(FlashbackDef {{ cost: &[CostComponent::Mana(Cost {{ pips: &[{}], generic: {generic}, x_count: {x_count} }})] }})",
                pips.join(", ")
            )
        }
        _ => "None".to_string(),
    }
}

/// Alter Reality has no alternate casting cost other than flashback.
pub(super) fn alt_cost_for(_name: &str) -> &'static str {
    "None"
}

/// Alter Reality has no printed evergreen keyword.
pub(super) fn keywords_for(_name: &str) -> &'static [&'static str] {
    &[]
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Alter Reality" => "spell:change_text_color_word_permanently;flashback:1U",
        _ => "none",
    }
}
