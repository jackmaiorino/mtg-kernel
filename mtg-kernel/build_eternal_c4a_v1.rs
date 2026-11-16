//! Card-database recipes for the eternal catalog's Vintage Cube wave C4a
//! (spell-cast, draw and copy trigger creatures). Wave C3c's tables fall through to these, and
//! these fall through to wave C4b's
//! (`build_eternal_c4b_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c4a_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.
//!
//! Rows of this wave that live in other waves' tables: none. Displacer
//! Kitten, Forensic Gadgeteer and Ledger Shredder have only triggered
//! abilities (`triggers_for` in the runtime module), and Forensic Gadgeteer's
//! static cost reduction is read where an activation's cost is computed.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::ManaCost;

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

/// A program of `eternal_cube_c4a_v1` (or of an earlier wave).
const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Faerie Mastermind: "{3}{U}: Each player draws a card."
const FAERIE_MASTERMIND: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{3}{U}")],
    program("crate::eternal_cube_c4a_v1::mastermind_each_player_draws"),
    "None",
)];

/// Bristly Bill, Spine Sower: "{3}{G}{G}: Double the number of +1/+1
/// counters on each creature you control."
const BRISTLY_BILL: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{3}{G}{G}")],
    program("crate::eternal_cube_c4a_v1::bill_double"),
    "None",
)];

/// Lutri, the Spellchaser's companion reminder: "you may put it into your
/// hand from outside the game for {3} as a sorcery." The engine performs it
/// as a special action (702.139c) from the companion's place in exile; it
/// never uses the stack (Lurrus' and Yorion's recipe).
const LUTRI: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{3}")],
    effect: AbilityEffectRecipe::StandardProgram("crate::eternal_lurrus_v1::companion_to_hand"),
    activation_zone: "Exile",
    sorcery_speed_only: true,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Caustic Bronco: "Saddle 3 (Tap any number of other creatures you control
/// with total power 3 or more: This Mount becomes saddled until end of turn.
/// Saddle only as a sorcery.)" Crew's cost (other untapped creatures, any of
/// them even when summoning sick) at sorcery speed.
const CAUSTIC_BRONCO: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    sorcery_speed_only: true,
    ..instant(
        &[AbilityCostRecipe::Crew(3)],
        program("crate::eternal_cube_c4a_v1::saddle"),
        "None",
    )
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Faerie Mastermind" => &FAERIE_MASTERMIND,
        "Bristly Bill, Spine Sower" => &BRISTLY_BILL,
        "Lutri, the Spellchaser" => &LUTRI,
        "Caustic Bronco" => &CAUSTIC_BRONCO,
        _ => super::build_eternal_c4b_v1::activated_ability_recipes_for(name),
    }
}

/// Jacked Rabbit: "Ravenous (This creature enters with X +1/+1 counters on
/// it. ...)" Stonecoil Serpent's entry program (the announced X is also
/// recorded for the draw at 5 or more).
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Jacked Rabbit" => Some(Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_cube_c4a_v1::jacked_rabbit",
        }),
        _ => super::build_eternal_c4b_v1::special_for(name),
    }
}

/// No C4a card has kicker yet.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c4b_v1::kicker_cost_for(name)
}

/// No C4a card is an Aura or an Equipment yet.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c4b_v1::attachment_for(name)
}

/// No C4a card has flashback yet.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c4b_v1::flashback_for(name)
}

/// No C4a card has an alternative cost yet.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c4b_v1::alt_cost_for(name)
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Faerie Mastermind" => &["Keywords::FLASH", "Keywords::FLYING"],
        "Ledger Shredder" => &["Keywords::FLYING"],
        "Lutri, the Spellchaser" => &["Keywords::FLASH"],
        _ => super::build_eternal_c4b_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Displacer Kitten" => {
            "cast_noncreature_spell:up_to_one_target_nonland_permanent_you_control:exile_then_return_to_battlefield_under_owner_control"
        }
        "Faerie Mastermind" => {
            "flash;flying;opponent_draws_second_card_each_turn:controller_draws_a_card;activated:3u:each_player_draws_a_card"
        }
        "Ledger Shredder" => {
            "flying;player_casts_second_spell_each_turn:source_connives:draw_then_discard_nonland_plus_one_counter"
        }
        "Forensic Gadgeteer" => {
            "cast_artifact_spell:investigate;static:activated_abilities_of_controlled_artifacts_cost_1_less_not_below_one_mana"
        }
        "Lutri, the Spellchaser" => {
            "companion:each_nonland_card_has_a_different_name:3_to_hand;flash;etb_if_cast:copy_target_controlled_instant_or_sorcery_spell:may_choose_new_targets"
        }
        "Caustic Bronco" => {
            "attacks:reveal_top_card_to_hand:lose_life_equal_to_mana_value_unless_saddled_else_each_opponent_loses_it;saddle_3:tap_other_creatures_total_power_3:sorcery"
        }
        "Jacked Rabbit" => {
            "ravenous:enters_with_x_plus_one_counters:x_cost;ravenous:x_at_least_5_etb_draw_a_card;attacks:create_rabbit_tokens_equal_to_power"
        }
        "Bristly Bill, Spine Sower" => {
            "landfall:put_plus_one_counter_on_target_creature;activated:3gg:double_plus_one_counters_on_each_controlled_creature"
        }
        _ => super::build_eternal_c4b_v1::trigger_recipe_for(name),
    }
}
