//! Card-database recipes for the eternal catalog's Vintage Cube wave C5d
//! (each-player decision spells and hidden-zone artifacts). Wave C5c's tables fall through to these, and
//! these fall through to wave C6a's (`build_eternal_c6a_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c5d_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.
//!
//! Proft's Eidetic Memory and Palantir of Orthanc have only triggered and
//! static abilities, which are runtime tables (`triggers_for`, and the hand
//! size check in `eternal_cards_v1::no_maximum_hand_size`), so they have a
//! trigger recipe token and no other row.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{SacrificeSelf, Tap};

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

/// Memory Jar: "{T}, Sacrifice this artifact: Each player exiles all cards
/// from their hand face down and draws seven cards. At the beginning of the
/// next end step, each player discards their hand and returns to their hand
/// each card they exiled this way." (Soul-Guide Lantern's cost.)
const MEMORY_JAR: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap, SacrificeSelf],
    AbilityEffectRecipe::StandardProgram("crate::eternal_cube_c5d_v1::memory_jar"),
    "None",
)];

/// Memory Jar's activated ability.
pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Memory Jar" => &MEMORY_JAR,
        _ => super::build_eternal_c6a_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function of `eternal_cube_c5d_v1`.
const fn program(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Balance, Council's Judgment and Fractured Identity.
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Balance" => Some(program(
            "TargetSpec::None",
            "crate::eternal_cube_c5d_v1::balance",
        )),
        "Council's Judgment" => Some(program(
            "TargetSpec::None",
            "crate::eternal_cube_c5d_v1::councils_judgment",
        )),
        "Fractured Identity" => Some(program(
            "TargetSpec::NonlandPermanent",
            "crate::eternal_cube_c5d_v1::fractured_identity",
        )),
        _ => super::build_eternal_c6a_v1::special_for(name),
    }
}

/// No C5d card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c6a_v1::kicker_cost_for(name)
}

/// No C5d card is an Aura or an Equipment.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c6a_v1::attachment_for(name)
}

/// No C5d card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c6a_v1::flashback_for(name)
}

/// No C5d card has an alternative cost.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c6a_v1::alt_cost_for(name)
}

/// No C5d card has a printed evergreen keyword.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    super::build_eternal_c6a_v1::keywords_for(name)
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Balance" => {
            "spell:each_player_chooses_lands_equal_to_fewest_sacrifices_rest;cards_in_hand_discard_the_same_way;creatures_sacrifice_the_same_way"
        }
        "Council's Judgment" => {
            "spell:will_of_the_council_starting_with_you_each_player_votes_for_nonland_permanent_you_dont_control;exile_each_permanent_with_most_votes_or_tied;no_targets"
        }
        "Memory Jar" => {
            "activated:tap_sacrifice_each_player_exiles_hand_face_down_draws_7;delayed:next_end_step_each_player_discards_hand_returns_exiled_cards"
        }
        "Palantir of Orthanc" => {
            "beginning_of_your_end_step:influence_counter_scry_2_target_opponent_may_have_you_draw_else_mill_x_and_they_lose_life_equal_to_total_mana_value"
        }
        "Proft's Eidetic Memory" => {
            "etb:draw_1;static:no_maximum_hand_size;beginning_of_combat_yours_if_drew_more_than_one_card:target_controlled_creature_x_plus_one_counters_x_is_cards_drawn_minus_one"
        }
        "Fractured Identity" => {
            "spell:exile_target_nonland_permanent;each_player_other_than_its_controller_creates_token_copy"
        }
        _ => super::build_eternal_c6a_v1::trigger_recipe_for(name),
    }
}
