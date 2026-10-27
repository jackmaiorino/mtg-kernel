//! Card-database recipes for the eternal catalog's Legacy aggro, Eldrazi
//! and control lists (wave E5b). `build_eternal_lurrus_v1` falls through to
//! these tables; runtime programs live in `src/eternal_legacy_e5b_v1.rs`.
//! Every row is keyed by a name that occurs only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{PayLife, SacrificeSelf, Tap};

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

/// A fetchland: "{T}, Pay 1 life, Sacrifice this land: Search your library
/// for a <first> or <second> card, put it onto the battlefield, then
/// shuffle." Any card with either land type qualifies.
macro_rules! fetchland {
    ($name:literal, $first:literal, $second:literal) => {
        [instant(
            &[Tap, PayLife(1), SacrificeSelf],
            AbilityEffectRecipe::Program {
                name: $name,
                effect: concat!(
                    "EffectOp::SearchLibraryCardsToDestination { player: PlayerRef::Controller, filter: crate::effect::LibraryCardFilter::CardWithAnySubtypeV1([Subtype::",
                    $first,
                    ", Subtype::",
                    $second,
                    "]), max_targets: 1, destination: crate::effect::LibrarySearchDestinationV1::Battlefield { tapped: false } }"
                ),
            },
            "None",
        )]
    };
}

const ARID_MESA: [ActivatedAbilityRecipe; 1] =
    fetchland!("fetch_mountain_or_plains", "Mountain", "Plains");
const WINDSWEPT_HEATH: [ActivatedAbilityRecipe; 1] =
    fetchland!("fetch_forest_or_plains", "Forest", "Plains");

/// Goblin Bombardment: "Sacrifice a creature: This enchantment deals 1
/// damage to any target."
const GOBLIN_BOMBARDMENT: [ActivatedAbilityRecipe; 1] = [instant(
    &[AbilityCostRecipe::SacrificeControlled {
        count: 1,
        filter: super::PermanentFilterRecipe::Creature,
    }],
    AbilityEffectRecipe::DealDamageAnyTarget(1),
    "AnyTarget",
)];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Arid Mesa" => &ARID_MESA,
        "Windswept Heath" => &WINDSWEPT_HEATH,
        "Goblin Bombardment" => &GOBLIN_BOMBARDMENT,
        _ => &[],
    }
}

/// A spell whose program is a runtime function of `eternal_legacy_e5b_v1`.
const fn program(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Stifle" => program(
            "TargetSpec::StackAbility",
            "crate::eternal_legacy_e5b_v1::stifle",
        ),
        _ => return None,
    })
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(_name: &str) -> &'static [&'static str] {
    &[]
}

/// Card-database tokens for the triggered abilities in
/// `eternal_legacy_e5b_v1::triggers_for`, its queued triggers and its
/// statics.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Up the Beanstalk" => "etb:draw:1;cast_spell_mv_at_least_5:draw:1",
        "Stifle" => "spell:counter_target_activated_or_triggered_ability",
        _ => "none",
    }
}
