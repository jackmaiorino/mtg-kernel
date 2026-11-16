//! Card-database recipes for the eternal catalog's Vintage Cube wave C5c
//! (reanimation spells and enchantments, a sweeper and a land). Wave C5b's tables fall through to these, and
//! these fall through to wave C5d's
//! (`build_eternal_c5d_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c5c_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.
//!
//! Rows of this wave that live in other waves' tables, each one small and
//! additive:
//!
//! - Shifting Woodland's "enters tapped unless you control a Forest" is in
//!   wave E8a's `enters_battlefield_tapped_unless_for`.
//! - Aang's Iceberg's "until this enchantment leaves the battlefield" exile
//!   is `effect.rs`'s `battlefield_exile_until_source_leaves`.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, PermanentFilterRecipe, Special,
};
use AbilityCostRecipe::{ManaCost, ReturnSelfToOwnersHand, SacrificeControlled, TapControlled};

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

/// An activated ability from the battlefield, only as a sorcery.
const fn sorcery(
    cost: &'static [AbilityCostRecipe],
    effect: AbilityEffectRecipe,
    target_spec: &'static str,
) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        sorcery_speed_only: true,
        ..instant(cost, effect, target_spec)
    }
}

/// A program of `eternal_cube_c5c_v1`.
const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Recurring Nightmare: "Sacrifice a creature, Return this enchantment to its
/// owner's hand: Return target creature card from your graveyard to the
/// battlefield. Activate only as a sorcery."
const RECURRING_NIGHTMARE: [ActivatedAbilityRecipe; 1] = [sorcery(
    &[
        SacrificeControlled {
            count: 1,
            filter: PermanentFilterRecipe::Creature,
        },
        ReturnSelfToOwnersHand,
    ],
    program("crate::eternal_cube_c5c_v1::nightmare_return"),
    "CreatureCardInOwnGraveyard",
)];

/// Aang's Iceberg: "Waterbend {3}: Sacrifice this enchantment. If you do,
/// scry 2. (While paying a waterbend cost, you can tap your artifacts and
/// creatures to help. Each one pays for {1}.)" One ability for each number of
/// artifacts and creatures tapped to help (none to three), each paying the
/// rest of the {3} with mana: tapping is a cost choice, so the player picks
/// the ability that matches the payment they want. Creatures tap for this
/// even when summoning sick, since it is not {T}.
const AANGS_ICEBERG: [ActivatedAbilityRecipe; 4] = [
    instant(
        &[ManaCost("{3}")],
        program("crate::eternal_cube_c5c_v1::iceberg_waterbend"),
        "None",
    ),
    instant(
        &[
            ManaCost("{2}"),
            TapControlled {
                count: 1,
                filter: PermanentFilterRecipe::ArtifactOrCreature,
            },
        ],
        program("crate::eternal_cube_c5c_v1::iceberg_waterbend"),
        "None",
    ),
    instant(
        &[
            ManaCost("{1}"),
            TapControlled {
                count: 2,
                filter: PermanentFilterRecipe::ArtifactOrCreature,
            },
        ],
        program("crate::eternal_cube_c5c_v1::iceberg_waterbend"),
        "None",
    ),
    instant(
        &[TapControlled {
            count: 3,
            filter: PermanentFilterRecipe::ArtifactOrCreature,
        }],
        program("crate::eternal_cube_c5c_v1::iceberg_waterbend"),
        "None",
    ),
];

/// Shifting Woodland: "Delirium -- {2}{G}{G}: This land becomes a copy of
/// target permanent card in your graveyard until end of turn. Activate only
/// if there are four or more card types among cards in your graveyard." Its
/// mana ability is the registry's `produces_mana`.
const SHIFTING_WOODLAND: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{2}{G}{G}")],
    program("crate::eternal_cube_c5c_v1::woodland_copy"),
    "PermanentCardInOwnGraveyard",
)];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Recurring Nightmare" => &RECURRING_NIGHTMARE,
        "Aang's Iceberg" => &AANGS_ICEBERG,
        "Shifting Woodland" => &SHIFTING_WOODLAND,
        _ => super::build_eternal_c5d_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function of `eternal_cube_c5c_v1`.
const fn spell(program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec: "TargetSpec::None",
        program,
    }
}

/// Corpse Dance and Sunfall. The permanents of this wave are ordinary
/// permanent spells.
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Corpse Dance" => Some(spell("crate::eternal_cube_c5c_v1::corpse_dance")),
        "Sunfall" => Some(spell("crate::eternal_cube_c5c_v1::sunfall")),
        _ => super::build_eternal_c5d_v1::special_for(name),
    }
}

/// Corpse Dance: "Buyback {2}" is carried as the engine's optional additional
/// mana cost (kicker, as Offspring does); its program reads whether it was
/// paid.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    match name {
        "Corpse Dance" => Some("{2}"),
        _ => super::build_eternal_c5d_v1::kicker_cost_for(name),
    }
}

/// No C5c card is an Aura or an Equipment (Necromancy only becomes an Aura
/// as its entry trigger resolves, which is runtime state).
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c5d_v1::attachment_for(name)
}

/// No C5c card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c5d_v1::flashback_for(name)
}

/// No C5c card has an alternative cost.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c5d_v1::alt_cost_for(name)
}

/// Printed evergreen keywords: Aang's Iceberg has flash, and Necromancy is
/// cast as though it had flash.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Aang's Iceberg" | "Necromancy" => &["Keywords::FLASH"],
        _ => super::build_eternal_c5d_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Corpse Dance" => {
            "buyback:2;spell:top_creature_card_own_graveyard_to_battlefield_haste:exile_next_end_step:if_buyback_paid_return_to_hand_as_it_resolves"
        }
        "Necromancy" => {
            "cast_as_though_flash:if_cast_when_sorcery_not_castable_sacrifice_next_cleanup;etb_if_on_battlefield:becomes_aura:target_creature_card_any_graveyard_to_battlefield_under_your_control_attach_self;leaves:that_creatures_controller_sacrifices_it"
        }
        "Recurring Nightmare" => {
            "activated_sorcery:sacrifice_creature_return_self_to_owners_hand:return_target_creature_card_own_graveyard_to_battlefield"
        }
        "Aang's Iceberg" => {
            "flash;etb:up_to_one_other_target_nonland_permanent:exile_until_source_leaves;activated:waterbend_3:sacrifice_self_if_you_do_scry_2"
        }
        "Shifting Woodland" => {
            "enters_tapped_unless_controls_forest;mana_ability:g;activated:delirium_2gg:becomes_copy_of_target_permanent_card_own_graveyard_until_end_of_turn"
        }
        "Sunfall" => "spell:exile_all_creatures:incubate_per_creature_exiled",
        _ => super::build_eternal_c5d_v1::trigger_recipe_for(name),
    }
}
