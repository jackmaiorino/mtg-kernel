//! Card-database recipes for the eternal catalog's Vintage Cube wave C4b
//! (magecraft, per-turn resolution counters and cast triggers). Wave C4a's tables fall through to these, and
//! these fall through to wave C4c's
//! (`build_eternal_c4c_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c4b_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.
//!
//! Rows of this wave that live in other waves' tables, each one small and
//! additive:
//!
//! - Sedgemoor Witch's "Ward-Pay 3 life" is in `build_eternal_lurrus_v1.rs`'s
//!   `ward_cost_for` (Hexing Squelcher's Ward-Pay 2 life).
//! - Improvised Act (Pompous Battlemage's prepare spell) is listed by
//!   `build_eternal_e7b_v1.rs`'s `is_prepare_spell`, since it is in no deck.

use super::{ActivatedAbilityRecipe, Special};

/// No C4b card has an activated ability.
pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    super::build_eternal_c4c_v1::activated_ability_recipes_for(name)
}

/// Improvised Act, Pompous Battlemage's prepare spell: "You may discard a
/// card. If you do, draw a card." (cast only as a copy of a prepared creature's
/// spell, like Enroot).
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Improvised Act" => Some(Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_cube_c4b_v1::improvised_act",
        }),
        _ => super::build_eternal_c4c_v1::special_for(name),
    }
}

/// No C4b card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c4c_v1::kicker_cost_for(name)
}

/// No C4b card is an Aura or an Equipment.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c4c_v1::attachment_for(name)
}

/// No C4b card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c4c_v1::flashback_for(name)
}

/// No C4b card has an alternative cost.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c4c_v1::alt_cost_for(name)
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Sedgemoor Witch" => &["Keywords::MENACE"],
        "Scythecat Cub" => &["Keywords::TRAMPLE"],
        "Sage of the Skies" => &["Keywords::FLYING", "Keywords::LIFELINK"],
        "Screaming Nemesis" => &["Keywords::HASTE"],
        _ => super::build_eternal_c4c_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Sedgemoor Witch" => {
            "ward:pay_3_life;magecraft:cast_or_copy_instant_or_sorcery:create_pest_token_1_1_black_green_dies_gain_1_life"
        }
        "Witherbloom Apprentice" => {
            "magecraft:cast_or_copy_instant_or_sorcery:each_opponent_loses_1_life_you_gain_1_life"
        }
        "Omnath, Locus of Creation" => {
            "etb:draw_1;landfall:nth_resolution_this_turn:1_gain_4_life:2_add_r_g_w_u:3_deal_4_damage_to_each_opponent_and_each_planeswalker_you_dont_control"
        }
        "Scythecat Cub" => {
            "landfall:target_controlled_creature:plus_one_counter_or_second_resolution_this_turn_double_its_plus_one_counters"
        }
        "Pompous Battlemage" => "prowess;enters_prepared:improvised_act",
        "Improvised Act" => "spell:may_discard_a_card_if_you_do_draw_a_card;prepare_spell_copy_only",
        "Sage of the Skies" => {
            "cast_self:if_another_spell_cast_this_turn:copy_this_spell_copy_becomes_token"
        }
        "Okoye, Mighty and Adored" => {
            "etb:become_monarch;beginning_of_combat_yours:target_creature_plus_one_counter:that_creature_attacks_monarch_this_turn_double_strike_trample_until_end_of_turn"
        }
        "Ouroboroid" => {
            "beginning_of_combat_yours:plus_one_counters_on_each_controlled_creature_x_is_source_power"
        }
        "Screaming Nemesis" => {
            "dealt_damage:deals_that_much_damage_to_any_other_target:player_dealt_cant_gain_life_rest_of_game"
        }
        _ => super::build_eternal_c4c_v1::trigger_recipe_for(name),
    }
}
