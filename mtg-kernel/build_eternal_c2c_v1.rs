//! Card-database recipes for the eternal catalog's Vintage Cube wave C2c
//! (mana artifacts and Fastbond). Wave C2b's tables fall through to these, and
//! this module is the tail of the recipe chains: its own
//! defaults are the chains' base values. Runtime programs live in
//! `src/eternal_cube_c2c_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.
//!
//! Rows of this wave that live in other waves' tables (the Talismans in
//! `standard_lands_v1.rs`, Basalt Monolith's yield in wave E5a, Pentad Prism's
//! additional mana ability in wave P1b, Everflowing Chalice's mana ability in
//! wave P2b) are listed in the module header of
//! `src/eternal_cube_c2c_v1.rs`.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{ManaCost, SacrificeSelf, Tap};

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

/// A program of `eternal_cube_c2c_v1` (or of an earlier wave).
const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Mind Stone: "{1}, {T}, Sacrifice this artifact: Draw a card." Its
/// "{T}: Add {C}." is the ordinary mana ability.
const MIND_STONE: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}"), Tap, SacrificeSelf],
    AbilityEffectRecipe::DrawCards(1),
    "None",
)];

/// Basalt Monolith: "{3}: Untap this artifact." Its "{T}: Add {C}{C}{C}." is
/// the generated yield program, "doesn't untap" a runtime name check.
const BASALT_MONOLITH: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{3}")],
    program("crate::eternal_e5a_v1::untap_source"),
    "None",
)];

/// Candelabra of Tawnos: "{X}, {T}: Untap X target lands." The engine has no
/// free X for an activation, so each X from 0 to 8 is its own ability (Blast
/// Zone's enumeration): ability `k` costs `{k}` and targets exactly `k`
/// distinct lands (`eternal_cube_c2c_v1::targets`).
const CANDELABRA_OF_TAWNOS: [ActivatedAbilityRecipe; 9] = [
    instant(
        &[Tap],
        program("crate::eternal_cube_c2c_v1::candelabra_untap_0"),
        "None",
    ),
    instant(
        &[ManaCost("{1}"), Tap],
        program("crate::eternal_cube_c2c_v1::candelabra_untap_1"),
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::C2cV1(crate::eternal_cube_c2c_v1::targets::C2cTargetV1::LandsV1(1)))",
    ),
    instant(
        &[ManaCost("{2}"), Tap],
        program("crate::eternal_cube_c2c_v1::candelabra_untap_2"),
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::C2cV1(crate::eternal_cube_c2c_v1::targets::C2cTargetV1::LandsV1(2)))",
    ),
    instant(
        &[ManaCost("{3}"), Tap],
        program("crate::eternal_cube_c2c_v1::candelabra_untap_3"),
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::C2cV1(crate::eternal_cube_c2c_v1::targets::C2cTargetV1::LandsV1(3)))",
    ),
    instant(
        &[ManaCost("{4}"), Tap],
        program("crate::eternal_cube_c2c_v1::candelabra_untap_4"),
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::C2cV1(crate::eternal_cube_c2c_v1::targets::C2cTargetV1::LandsV1(4)))",
    ),
    instant(
        &[ManaCost("{5}"), Tap],
        program("crate::eternal_cube_c2c_v1::candelabra_untap_5"),
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::C2cV1(crate::eternal_cube_c2c_v1::targets::C2cTargetV1::LandsV1(5)))",
    ),
    instant(
        &[ManaCost("{6}"), Tap],
        program("crate::eternal_cube_c2c_v1::candelabra_untap_6"),
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::C2cV1(crate::eternal_cube_c2c_v1::targets::C2cTargetV1::LandsV1(6)))",
    ),
    instant(
        &[ManaCost("{7}"), Tap],
        program("crate::eternal_cube_c2c_v1::candelabra_untap_7"),
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::C2cV1(crate::eternal_cube_c2c_v1::targets::C2cTargetV1::LandsV1(7)))",
    ),
    instant(
        &[ManaCost("{8}"), Tap],
        program("crate::eternal_cube_c2c_v1::candelabra_untap_8"),
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::C2cV1(crate::eternal_cube_c2c_v1::targets::C2cTargetV1::LandsV1(8)))",
    ),
];

/// Coalition Relic: "{T}: Put a charge counter on this artifact." Its
/// "{T}: Add one mana of any color." is the ordinary mana ability and its
/// first-main-phase release a delayed trigger of the runtime module.
const COALITION_RELIC: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap],
    program("crate::eternal_legacy_e7a_v1::blast_zone_charge_1"),
    "None",
)];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Mind Stone" => &MIND_STONE,
        "Basalt Monolith" => &BASALT_MONOLITH,
        "Candelabra of Tawnos" => &CANDELABRA_OF_TAWNOS,
        "Coalition Relic" => &COALITION_RELIC,
        _ => &[],
    }
}

/// A spell whose program is a runtime function.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs: the two artifacts that enter with charge counters.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        // Sunburst.
        "Pentad Prism" => spell(
            "TargetSpec::None",
            "crate::eternal_cube_c2c_v1::pentad_prism",
        ),
        // Multikicker {2}: the number of kicks is the announced count, stored
        // as the spell's X value like Replicate's.
        "Everflowing Chalice" => spell(
            "TargetSpec::None",
            "crate::eternal_cube_c2c_v1::everflowing_chalice",
        ),
        _ => return None,
    })
}

/// No C2c card has kicker (multikicker is `multikicker_cost` of the runtime
/// module).
pub(super) fn kicker_cost_for(_name: &str) -> Option<&'static str> {
    None
}

/// No C2c card is an Aura or an Equipment yet.
pub(super) fn attachment_for(_name: &str) -> &'static str {
    "None"
}

/// No C2c card has flashback yet.
pub(super) fn flashback_for(_name: &str) -> String {
    "None".to_string()
}

/// No C2c card has an alternative cost yet.
pub(super) fn alt_cost_for(_name: &str) -> &'static str {
    "None"
}

/// No C2c card has a printed evergreen keyword yet.
pub(super) fn keywords_for(_name: &str) -> &'static [&'static str] {
    &[]
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Talisman of Creativity" => "mana:c;mana:u_or_r_deals_1_damage_to_you",
        "Talisman of Curiosity" => "mana:c;mana:g_or_u_deals_1_damage_to_you",
        "Talisman of Dominance" => "mana:c;mana:u_or_b_deals_1_damage_to_you",
        "Talisman of Progress" => "mana:c;mana:w_or_u_deals_1_damage_to_you",
        "Mind Stone" => "mana:c;activated:1_tap_sacrifice_draw_1",
        "Basalt Monolith" => {
            "static:doesnt_untap_during_untap_step;mana:ccc;activated:3_untap_this_artifact"
        }
        "Candelabra of Tawnos" => "activated:x_tap_untap_up_to_x_lands",
        "Coalition Relic" => {
            "mana:any_color;activated:tap_put_charge_counter;first_main_phase:remove_all_charge_counters_add_any_color_mana_for_each"
        }
        "Fastbond" => {
            "static:any_number_of_land_plays;play_land:if_not_first_deals_1_damage_to_you"
        }
        "Everflowing Chalice" => {
            "multikicker:2;enters_with_charge_counter_per_kick;mana:c_for_each_charge_counter"
        }
        "Pentad Prism" => "sunburst;mana:remove_charge_counter_add_any_color",
        _ => "none",
    }
}
