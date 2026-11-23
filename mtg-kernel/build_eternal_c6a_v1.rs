//! Card-database recipes for the eternal catalog's Vintage Cube wave C6a
//! (discard, exile and impulse-draw trigger creatures). Wave C5d's tables fall through to these, and
//! these fall through to wave C6b's
//! (`build_eternal_c6b_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c6a_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};

/// Fallen Shinobi uses Ninja of the Deep Hours' hand-zone ninjutsu recipe.
pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Fallen Shinobi" => &[ActivatedAbilityRecipe {
            cost: &[
                AbilityCostRecipe::ManaCost("{2}{U}{B}"),
                AbilityCostRecipe::ReturnControlledUnblockedAttacker,
            ],
            effect: AbilityEffectRecipe::PutSourceOntoBattlefieldTappedAndAttacking,
            activation_zone: "Hand",
            sorcery_speed_only: false,
            target_spec: "None",
            activation_target_filter: "TargetSpecOnly",
            max_activations_per_turn: None,
        }],
        _ => super::build_eternal_c6b_v1::activated_ability_recipes_for(name),
    }
}

/// Creature spells resolve as permanents; their abilities are trigger programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Ivora, Insatiable Heir"
        | "Gau, Feral Youth"
        | "Laelia, the Blade Reforged"
        | "Fallen Shinobi"
        | "Generous Plunderer"
        | "Gut, True Soul Zealot" => Some(Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_cube_c6a_v1::permanent",
        }),
        _ => super::build_eternal_c6b_v1::special_for(name),
    }
}

/// No C6a card has kicker yet.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c6b_v1::kicker_cost_for(name)
}

/// No C6a card is an Aura or an Equipment yet.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c6b_v1::attachment_for(name)
}

/// No C6a card has flashback yet.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c6b_v1::flashback_for(name)
}

/// No C6a card has an alternative cost yet.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c6b_v1::alt_cost_for(name)
}

/// Printed evergreen keywords.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Ivora, Insatiable Heir" => &["Keywords::TRAMPLE"],
        "Laelia, the Blade Reforged" => &["Keywords::HASTE"],
        "Generous Plunderer" => &["Keywords::MENACE"],
        _ => super::build_eternal_c6b_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Ivora, Insatiable Heir" => "etb:create_blood_token:1;deals_combat_damage_to_player:create_blood_token:1;controller_discards_card:source_plus_one_counter:1",
        "Gau, Feral Youth" => "attacks:source_plus_one_counter:1;each_end_step:if_card_left_your_graveyard_this_turn:damage_each_opponent_source_power",
        "Laelia, the Blade Reforged" => "haste;attacks:impulse_top_one_end_of_turn;one_or_more_cards_exiled_from_your_library_or_graveyard:source_plus_one_counter:1",
        "Fallen Shinobi" => "ninjutsu:2ub;combat_damage_player:that_player_exiles_top_two:controller_may_play_this_turn_without_mana_cost",
        "Generous Plunderer" => "menace;your_upkeep:may_create_treasure:when_you_do:target_opponent_creates_tapped_treasure;attacks:damage_defending_player_equal_artifacts_they_control",
        "Gut, True Soul Zealot" => "controller_attacks:may_sacrifice_another_creature_or_artifact:if_paid:create_black_4_1_skeleton_menace_tapped_attacking;choose_a_background",
        _ => super::build_eternal_c6b_v1::trigger_recipe_for(name),
    }
}
