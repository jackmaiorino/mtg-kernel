//! Card-database recipes for the eternal catalog's Vintage Cube wave C9c
//! (Elspeth, Storm Slayer; Grist, the Hunger Tide; Nissa, Who Shakes the
//! World; Tamiyo, Collector of Tales; Through the Breach; and Black Green
//! Insect Token). Wave C9b's tables fall through to these, and this module is
//! the tail of the recipe chains: its own defaults are the chains' base values.
//! Runtime programs live in `src/eternal_cube_c9c_v1.rs`. Every row is keyed
//! by a name that occurs only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};

/// Elspeth, Storm Slayer (`starting_loyalty: 5`):
/// - `+1`: Create a 1/1 white Soldier creature token.
/// - `0`: Put a +1/+1 counter on each creature you control. Those creatures
///   gain flying until your next turn.
/// - `-3`: Destroy target creature an opponent controls with mana value 3 or
///   greater.
const ELSPETH_STORM_SLAYER: [ActivatedAbilityRecipe; 3] = [
    ActivatedAbilityRecipe {
        cost: &[AbilityCostRecipe::Loyalty(1)],
        effect: AbilityEffectRecipe::StandardProgram("crate::eternal_cube_c9c_v1::elspeth_plus_one"),
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
    ActivatedAbilityRecipe {
        cost: &[AbilityCostRecipe::Loyalty(0)],
        effect: AbilityEffectRecipe::StandardProgram("crate::eternal_cube_c9c_v1::elspeth_zero"),
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
    ActivatedAbilityRecipe {
        cost: &[AbilityCostRecipe::Loyalty(-3)],
        effect: AbilityEffectRecipe::StandardProgram(
            "crate::eternal_cube_c9c_v1::elspeth_minus_three",
        ),
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec: "StandardV1(crate::standard_cards_v1::StandardTargetV1::C9cV1(crate::eternal_cube_c9c_v1::targets::C9cTargetV1::OpponentCreatureManaValueThreeOrGreaterV1))",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
];

/// Grist, the Hunger Tide (`starting_loyalty: 3`):
/// - `+1`: Create a 1/1 black and green Insect creature token, then mill a
///   card. If an Insect card was milled this way, put a loyalty counter on
///   Grist and repeat this process.
/// - `-2`: You may sacrifice a creature. When you do, destroy target creature
///   or planeswalker.
/// - `-5`: Each opponent loses life equal to the number of creature cards in
///   your graveyard.
const GRIST_THE_HUNGER_TIDE: [ActivatedAbilityRecipe; 3] = [
    ActivatedAbilityRecipe {
        cost: &[AbilityCostRecipe::Loyalty(1)],
        effect: AbilityEffectRecipe::StandardProgram("crate::eternal_cube_c9c_v1::grist_plus_one"),
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
    ActivatedAbilityRecipe {
        cost: &[AbilityCostRecipe::Loyalty(-2)],
        effect: AbilityEffectRecipe::StandardProgram("crate::eternal_cube_c9c_v1::grist_minus_two"),
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
    ActivatedAbilityRecipe {
        cost: &[AbilityCostRecipe::Loyalty(-5)],
        effect: AbilityEffectRecipe::StandardProgram(
            "crate::eternal_cube_c9c_v1::grist_minus_five",
        ),
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
];

/// Nissa, Who Shakes the World (`starting_loyalty: 5`):
/// - `+1`: Put three +1/+1 counters on up to one target noncreature land you
///   control. Untap it. It becomes a 0/0 Elemental creature with vigilance and
///   haste that's still a land.
/// - `-8`: You get an emblem with "Lands you control have indestructible."
///   Search your library for any number of Forest cards, put them onto the
///   battlefield tapped, then shuffle.
const NISSA_WHO_SHAKES_THE_WORLD: [ActivatedAbilityRecipe; 2] = [
    ActivatedAbilityRecipe {
        cost: &[AbilityCostRecipe::Loyalty(1)],
        effect: AbilityEffectRecipe::StandardProgram("crate::eternal_cube_c9c_v1::nissa_plus_one"),
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec: "StandardV1(crate::standard_cards_v1::StandardTargetV1::C9cV1(crate::eternal_cube_c9c_v1::targets::C9cTargetV1::UpToOneControlledNoncreatureLandV1))",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
    ActivatedAbilityRecipe {
        cost: &[AbilityCostRecipe::Loyalty(-8)],
        effect: AbilityEffectRecipe::StandardProgram(
            "crate::eternal_cube_c9c_v1::nissa_minus_eight",
        ),
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
];

/// Tamiyo, Collector of Tales (`starting_loyalty: 5`):
/// - `+1`: Choose a nonland card name, then reveal the top four cards of your
///   library. Put all cards with the chosen name from among them into your
///   hand and the rest into your graveyard.
/// - `-3`: Return target card from your graveyard to your hand.
const TAMIYO_COLLECTOR_OF_TALES: [ActivatedAbilityRecipe; 2] = [
    ActivatedAbilityRecipe {
        cost: &[AbilityCostRecipe::Loyalty(1)],
        effect: AbilityEffectRecipe::StandardProgram("crate::eternal_cube_c9c_v1::tamiyo_plus_one"),
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
    ActivatedAbilityRecipe {
        cost: &[AbilityCostRecipe::Loyalty(-3)],
        effect: AbilityEffectRecipe::StandardProgram(
            "crate::eternal_cube_c9c_v1::tamiyo_minus_three",
        ),
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec: "StandardV1(crate::standard_cards_v1::StandardTargetV1::CardInOwnGraveyardV1)",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
];

/// Loyalty abilities of the four C9c planeswalkers.
pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Elspeth, Storm Slayer" => &ELSPETH_STORM_SLAYER,
        "Grist, the Hunger Tide" => &GRIST_THE_HUNGER_TIDE,
        "Nissa, Who Shakes the World" => &NISSA_WHO_SHAKES_THE_WORLD,
        "Tamiyo, Collector of Tales" => &TAMIYO_COLLECTOR_OF_TALES,
        _ => &[],
    }
}

/// Spell programs for the four C9c planeswalkers and Through the Breach.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Elspeth, Storm Slayer"
        | "Grist, the Hunger Tide"
        | "Nissa, Who Shakes the World"
        | "Tamiyo, Collector of Tales" => Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_cube_c9c_v1::planeswalker_enter",
        },
        "Through the Breach" => Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_cube_c9c_v1::through_the_breach",
        },
        _ => return None,
    })
}

/// No C9c card has kicker.
pub(super) fn kicker_cost_for(_name: &str) -> Option<&'static str> {
    None
}

/// No C9c card is an Aura or an Equipment.
pub(super) fn attachment_for(_name: &str) -> &'static str {
    "None"
}

/// No C9c card has flashback.
pub(super) fn flashback_for(_name: &str) -> String {
    "None".to_string()
}

/// No C9c card has an alternative cost.
pub(super) fn alt_cost_for(_name: &str) -> &'static str {
    "None"
}

/// No C9c card has a printed evergreen keyword.
pub(super) fn keywords_for(_name: &str) -> &'static [&'static str] {
    &[]
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Elspeth, Storm Slayer" => "static:double_controlled_tokens_created;loyalty:+1_soldier_token;0_plus1_counter_and_flying_until_next_turn;-3_destroy_opponent_creature_mv_3_plus",
        "Grist, the Hunger Tide" => "cda:nonbattlefield_1_1_insect_creature;loyalty:+1_insect_token_mill_repeat_on_insect;-2_may_sac_creature_reflexive_destroy_creature_or_pw;-5_opponents_lose_life_per_gy_creature",
        "Nissa, Who Shakes the World" => "triggered_mana:tap_controlled_forest_add_g;loyalty:+1_three_counters_untap_animate_0_0_elemental_vigilance_haste;-8_emblem_lands_indestructible_search_any_forests_tapped",
        "Tamiyo, Collector of Tales" => "static:opponent_effects_cant_cause_discard_or_sacrifice;loyalty:+1_name_nonland_reveal_4_matching_to_hand_rest_gy;-3_regrowth_own_gy_card",
        "Through the Breach" => "may_put_hand_creature_onto_battlefield_haste_sacrifice_next_end_step;splice_onto_arcane:2rr",
        _ => "none",
    }
}
