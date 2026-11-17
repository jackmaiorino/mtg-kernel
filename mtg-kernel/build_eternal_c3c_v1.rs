//! Card-database recipes for the eternal catalog's Vintage Cube wave C3c
//! (artifact utilities and their tokens). Wave C3b's tables fall through to these, and
//! these fall through to wave C4a's (`build_eternal_c4a_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c3c_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.
//!
//! Rows of this wave that live in other waves' tables, each one small and
//! additive:
//!
//! - Lightning Greaves' `EquipmentDef` is in wave E9a's `equipment_for`
//!   (Shuko's table); its "Equip {0}" is Shuko's recipe, below.
//! - Relic of Sauron's and The Mightstone and Weakstone's primary mana
//!   abilities are in `build_eternal_v1.rs`'s `mana_ability_def_for` (the
//!   head of that chain) and Relic of Sauron's three mixed-pair abilities in
//!   wave E6a's `additional_mana_abilities_for`.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, PermanentFilterRecipe, Special,
};
use AbilityCostRecipe::{ExileSelf, ManaCost, SacrificeControlled, SacrificeSelf, Tap};

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

/// A program of `eternal_cube_c3c_v1` (or of an earlier wave).
const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Lightning Greaves: "Equip {0}" (702.6a): attach to target creature you
/// control, only as a sorcery (Shuko's recipe).
const LIGHTNING_GREAVES: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    sorcery_speed_only: true,
    ..instant(
        &[ManaCost("{0}")],
        AbilityEffectRecipe::AttachSourceToTarget,
        "ControlledCreature",
    )
}];

/// Pyrite Spellbomb: "{R}, Sacrifice this artifact: It deals 2 damage to any
/// target." and "{1}, Sacrifice this artifact: Draw a card." (Aether
/// Spellbomb's cost pairs).
const PYRITE_SPELLBOMB: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[ManaCost("{R}"), SacrificeSelf],
        AbilityEffectRecipe::DealDamageAnyTarget(2),
        "AnyTarget",
    ),
    instant(
        &[ManaCost("{1}"), SacrificeSelf],
        AbilityEffectRecipe::DrawCards(1),
        "None",
    ),
];

/// Shorikai, Genesis Engine: "{1}, {T}: Draw two cards, then discard a card.
/// Create a 1/1 colorless Pilot creature token with 'This token crews
/// Vehicles as though its power were 2 greater.'" and "Crew 8" (Reckoner
/// Bankbuster's crew ability).
const SHORIKAI: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[ManaCost("{1}"), Tap],
        program("crate::eternal_cube_c3c_v1::shorikai_loot"),
        "None",
    ),
    instant(
        &[AbilityCostRecipe::Crew(8)],
        program("crate::standard_cards_v1::crew_animation"),
        "None",
    ),
];

/// The Echoverse Fulcrum: "{5}, {T}, Exile The Echoverse Fulcrum: Destroy all
/// creatures. Activate only as a sorcery."
const ECHOVERSE_FULCRUM: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    sorcery_speed_only: true,
    ..instant(
        &[ManaCost("{5}"), Tap, ExileSelf],
        program("crate::eternal_legacy_e8a_v1::supreme_verdict"),
        "None",
    )
}];

/// Retrofitter Foundry: "{3}: Untap this artifact." (Basalt Monolith's
/// program), "{2}, {T}: Create a 1/1 colorless Servo artifact creature
/// token.", "{1}, {T}, Sacrifice a Servo: Create a 1/1 colorless Thopter
/// artifact creature token with flying." and "{T}, Sacrifice a Thopter:
/// Create a 4/4 colorless Construct artifact creature token."
const RETROFITTER_FOUNDRY: [ActivatedAbilityRecipe; 4] = [
    instant(
        &[ManaCost("{3}")],
        program("crate::eternal_e5a_v1::untap_source"),
        "None",
    ),
    instant(
        &[ManaCost("{2}"), Tap],
        AbilityEffectRecipe::CreateToken("Servo Token"),
        "None",
    ),
    instant(
        &[
            ManaCost("{1}"),
            Tap,
            SacrificeControlled {
                count: 1,
                filter: PermanentFilterRecipe::Servo,
            },
        ],
        AbilityEffectRecipe::CreateToken("Thopter Token"),
        "None",
    ),
    instant(
        &[
            Tap,
            SacrificeControlled {
                count: 1,
                filter: PermanentFilterRecipe::Thopter,
            },
        ],
        AbilityEffectRecipe::CreateToken("Construct Token"),
        "None",
    ),
];

/// Thopter Foundry: "{1}, Sacrifice a nontoken artifact: Create a 1/1 blue
/// Thopter artifact creature token with flying. You gain 1 life."
const THOPTER_FOUNDRY: [ActivatedAbilityRecipe; 1] = [instant(
    &[
        ManaCost("{1}"),
        SacrificeControlled {
            count: 1,
            filter: PermanentFilterRecipe::NontokenArtifact,
        },
    ],
    program("crate::eternal_cube_c3c_v1::thopter_foundry"),
    "None",
)];

/// Relic of Sauron: "{3}, {T}: Draw two cards, then discard a card." Its
/// "{T}: Add two mana in any combination of {U}, {B}, and/or {R}." is the
/// mana abilities named in the module header.
const RELIC_OF_SAURON: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{3}"), Tap],
    AbilityEffectRecipe::DrawThenDiscard {
        draw: 2,
        discard: 1,
    },
    "None",
)];

/// Currency Converter: "{2}, {T}: Draw a card, then discard a card." and
/// "{T}: Put a card exiled with this artifact into its owner's graveyard. If
/// it's a land card, create a Treasure token. If it's a nonland card, create
/// a 2/2 black Rogue creature token."
const CURRENCY_CONVERTER: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[ManaCost("{2}"), Tap],
        AbilityEffectRecipe::DrawThenDiscard {
            draw: 1,
            discard: 1,
        },
        "None",
    ),
    instant(
        &[Tap],
        program("crate::eternal_cube_c3c_v1::converter_release"),
        "None",
    ),
];

/// Staff of the Storyteller: "{W}, {T}, Remove a story counter from this
/// artifact: Draw a card." Story counters are charge counters (Extinguisher
/// Battleship's station counters are the same stand-in).
const STAFF_OF_THE_STORYTELLER: [ActivatedAbilityRecipe; 1] = [instant(
    &[
        ManaCost("{W}"),
        Tap,
        AbilityCostRecipe::RemoveChargeCounterFromSelf,
    ],
    AbilityEffectRecipe::DrawCards(1),
    "None",
)];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Lightning Greaves" => &LIGHTNING_GREAVES,
        "Pyrite Spellbomb" => &PYRITE_SPELLBOMB,
        "Shorikai, Genesis Engine" => &SHORIKAI,
        "The Echoverse Fulcrum" => &ECHOVERSE_FULCRUM,
        "Retrofitter Foundry" => &RETROFITTER_FOUNDRY,
        "Thopter Foundry" => &THOPTER_FOUNDRY,
        "Relic of Sauron" => &RELIC_OF_SAURON,
        "Currency Converter" => &CURRENCY_CONVERTER,
        "Staff of the Storyteller" => &STAFF_OF_THE_STORYTELLER,
        _ => super::build_eternal_c4a_v1::activated_ability_recipes_for(name),
    }
}

/// No C3c spell program: every card of this wave is a permanent.
pub(super) fn special_for(name: &str) -> Option<Special> {
    super::build_eternal_c4a_v1::special_for(name)
}

/// No C3c card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c4a_v1::kicker_cost_for(name)
}

/// No C3c card is an Aura (Lightning Greaves' Equipment profile is wave
/// E9a's `equipment_for`).
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c4a_v1::attachment_for(name)
}

/// No C3c card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c4a_v1::flashback_for(name)
}

/// No C3c card has an alternative cost.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c4a_v1::alt_cost_for(name)
}

/// Printed evergreen keywords: the Thopter tokens fly.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Thopter Token" | "Blue Thopter Token" => &["Keywords::FLYING"],
        _ => super::build_eternal_c4a_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Lightning Greaves" => "equipment:haste_shroud;equip:0",
        "Pyrite Spellbomb" => {
            "activated:r_sacrifice_self_deal_2_any_target;activated:1_sacrifice_self_draw_1"
        }
        "Shorikai, Genesis Engine" => {
            "vehicle:8_8;crew:8;activated:1_tap_draw_2_discard_1_create_pilot_token"
        }
        "The Echoverse Fulcrum" => {
            "etb:draw_1_discard_1;activated:5_tap_exile_self_destroy_all_creatures_sorcery_speed"
        }
        "Retrofitter Foundry" => {
            "activated:3_untap_self;activated:2_tap_create_servo_token;activated:1_tap_sacrifice_servo_create_thopter_token;activated:tap_sacrifice_thopter_create_construct_token"
        }
        "Thopter Foundry" => {
            "activated:1_sacrifice_nontoken_artifact_create_blue_thopter_token_gain_1_life"
        }
        "Relic of Sauron" => "mana:two_in_any_combination_of_u_b_r;activated:3_tap_draw_2_discard_1",
        "Currency Converter" => {
            "discard:you_may_exile_that_card_from_graveyard;activated:2_tap_draw_1_discard_1;activated:tap_put_linked_exiled_card_into_graveyard_land_treasure_nonland_rogue_token"
        }
        "Staff of the Storyteller" => {
            "etb:create_white_flying_spirit_token;creature_token_created:one_or_more_put_story_counter_on_self;activated:w_tap_remove_story_counter_draw_1"
        }
        "The Mightstone and Weakstone" => {
            "etb:choose_one_draw_2_or_target_creature_minus_5_minus_5_until_end_of_turn;mana:cc_not_for_nonartifact_spells"
        }
        _ => super::build_eternal_c4a_v1::trigger_recipe_for(name),
    }
}
