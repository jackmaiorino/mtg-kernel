//! Card-database recipes for the eternal catalog's Vintage Cube wave C1b
//! (black and green instants and sorceries). Wave C1a's tables fall through to these, and
//! these fall through to wave C1c's
//! (`build_eternal_c1c_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c1b_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{ActivatedAbilityRecipe, Special};

/// No C1b card has an activated ability yet.
pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    super::build_eternal_c1c_v1::activated_ability_recipes_for(name)
}

/// A spell whose program is a runtime function of `eternal_cube_c1b_v1`.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Damnation, Entomb, Hymn to Tourach, Imperial Seal, Infernal Grasp, Mind
/// Twist, Night's Whisper, Mutagenic Growth, Bloodchief's Thirst, Bone
/// Shards, Chain of Smog, Collective Brutality, Natural Order, Pest
/// Infestation and Tamiyo's Safekeeping.
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        // Its kicker ({2}{B}) is `kicker_cost_for`, and its kicked target
        // spec (any creature or planeswalker) is `kicked_target_spec` in
        // `build.rs`.
        "Bloodchief's Thirst" => Some(spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::CreatureOrPlaneswalkerManaValueAtMostV1(2))",
            "crate::eternal_cube_c1b_v1::bloodchiefs_thirst",
        )),
        // Its sacrifice-or-discard additional cost is its two modes
        // (`build_eternal_v1::mode2_for`, `eternal_cost_choice_v1`).
        "Bone Shards" => Some(spell(
            "TargetSpec::CreatureOrPlaneswalker",
            "crate::eternal_cube_c1b_v1::bone_shards",
        )),
        "Chain of Smog" => Some(spell(
            "TargetSpec::AnyPlayer",
            "crate::eternal_cube_c1b_v1::chain_of_smog",
        )),
        // Escalate: its first mode is this program, the rest are the mode
        // tables below.
        "Collective Brutality" => Some(spell(
            "TargetSpec::TargetOpponent",
            "crate::eternal_cube_c1b_v1::collective_brutality",
        )),
        "Natural Order" => Some(spell(
            "TargetSpec::None",
            "crate::eternal_cube_c1b_v1::natural_order",
        )),
        "Pest Infestation" => Some(spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::C1bV1(crate::eternal_cube_c1b_v1::targets::C1bTargetV1::ArtifactsAndEnchantmentsV1))",
            "crate::eternal_cube_c1b_v1::pest_infestation",
        )),
        "Tamiyo's Safekeeping" => Some(spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::ControlledPermanentV1)",
            "crate::eternal_cube_c1b_v1::tamiyos_safekeeping",
        )),
        "Damnation" => Some(spell(
            "TargetSpec::None",
            "crate::eternal_cube_c1b_v1::damnation",
        )),
        "Entomb" => Some(spell(
            "TargetSpec::None",
            "crate::eternal_cube_c1b_v1::entomb",
        )),
        "Hymn to Tourach" => Some(spell(
            "TargetSpec::AnyPlayer",
            "crate::eternal_cube_c1b_v1::hymn_to_tourach",
        )),
        "Imperial Seal" => Some(spell(
            "TargetSpec::None",
            "crate::eternal_cube_c1b_v1::imperial_seal",
        )),
        "Infernal Grasp" => Some(spell(
            "TargetSpec::Creature",
            "crate::eternal_cube_c1b_v1::infernal_grasp",
        )),
        "Mind Twist" => Some(spell(
            "TargetSpec::AnyPlayer",
            "crate::eternal_cube_c1b_v1::mind_twist",
        )),
        "Night's Whisper" => Some(spell(
            "TargetSpec::None",
            "crate::eternal_cube_c1b_v1::nights_whisper",
        )),
        // {G/P} is its printed cost; the pump is Giant Growth's recipe.
        "Mutagenic Growth" => Some(Special::PumpCreature {
            power: 2,
            toughness: 2,
        }),
        _ => super::build_eternal_c1c_v1::special_for(name),
    }
}

/// Bloodchief's Thirst: "Kicker {2}{B}".
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    match name {
        "Bloodchief's Thirst" => Some("{2}{B}"),
        _ => super::build_eternal_c1c_v1::kicker_cost_for(name),
    }
}

/// No C1b card is an Aura or an Equipment yet.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c1c_v1::attachment_for(name)
}

/// No C1b card has flashback yet.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c1c_v1::flashback_for(name)
}

/// No C1b card has an alternative cost. Bone Shards' "sacrifice a creature
/// or discard a card" is a choice between two additional costs, carried as
/// its two modes (`eternal_cost_choice_v1`).
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c1c_v1::alt_cost_for(name)
}

/// Mandatory additional costs. Bone Shards: the discard of its choice
/// between two additional costs, which opens the cast's discard stage; the
/// chosen mode names the cost really paid (`eternal_cost_choice_v1`).
/// Collective Brutality: its escalate cost,
/// "discard a card" for each mode beyond the first; the marker discard here
/// is replaced by the mode's own count
/// (`eternal_cube_c1b_v1::additional_discard_count`). Natural Order:
/// "sacrifice a green creature".
pub(super) fn additional_cost_for(name: &str) -> &'static str {
    match name {
        "Bone Shards" | "Collective Brutality" => "Some(&[CostComponent::DiscardCards(1)])",
        "Natural Order" => {
            "Some(&[CostComponent::SacrificeControlled { count: 1, filter: PermanentFilter::GreenCreatureV1 }])"
        }
        _ => super::build_eternal_e6a_v1::additional_cost_for(name),
    }
}

// Collective Brutality: "Choose one or more" with escalate, as the seven
// nonempty subsets of its three modes (reveal and discard an instant or
// sorcery, -2/-2, drain), in the order A, B, AB, C, AC, BC, ABC. Each
// filter takes one target per chosen targeting mode, in printed order
// (`eternal_cube_c1b_v1::targets`). The first is `special_for`'s program;
// `build_eternal_s1m_v1.rs` calls these three.

/// B: target creature gets -2/-2.
pub(super) const BRUTALITY_MODE_2: &str = "Some(ModeDef { target_spec: TargetSpec::Creature, effect: crate::eternal_cube_c1b_v1::collective_brutality_1 })";

/// AB: reveal and shrink.
pub(super) const BRUTALITY_MODE_3: &str = "Some(ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::C1bV1(crate::eternal_cube_c1b_v1::targets::C1bTargetV1::BrutalitySlotsV1(3))), effect: crate::eternal_cube_c1b_v1::collective_brutality_2 })";

/// C, AC, BC and ABC.
pub(super) const BRUTALITY_ADDITIONAL_MODES: &str = "&[ModeDef { target_spec: TargetSpec::TargetOpponent, effect: crate::eternal_cube_c1b_v1::collective_brutality_3 }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::C1bV1(crate::eternal_cube_c1b_v1::targets::C1bTargetV1::BrutalitySlotsV1(5))), effect: crate::eternal_cube_c1b_v1::collective_brutality_4 }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::C1bV1(crate::eternal_cube_c1b_v1::targets::C1bTargetV1::BrutalitySlotsV1(6))), effect: crate::eternal_cube_c1b_v1::collective_brutality_5 }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::C1bV1(crate::eternal_cube_c1b_v1::targets::C1bTargetV1::BrutalitySlotsV1(7))), effect: crate::eternal_cube_c1b_v1::collective_brutality_6 }]";

/// No C1b card has a printed evergreen keyword yet.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    super::build_eternal_c1c_v1::keywords_for(name)
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Damnation" => "spell:destroy_all_creatures:cant_be_regenerated",
        "Entomb" => "spell:search_any_card_put_into_graveyard_shuffle_hidden",
        "Hymn to Tourach" => "spell:target_player_discards_two_at_random",
        "Imperial Seal" => "spell:search_any_card_top_after_shuffle_hidden;lose_life:2",
        "Infernal Grasp" => "spell:destroy_target_creature;lose_life:2",
        "Mind Twist" => "x_cost;spell:target_player_discards_x_at_random",
        "Night's Whisper" => "spell:draw_2;lose_life:2",
        "Mutagenic Growth" => {
            "phyrexian_mana:G;spell:target_creature_gets_plus_2_plus_2_until_end_of_turn"
        }
        "Bloodchief's Thirst" => {
            "kicker:2B;spell:destroy_target_creature_or_planeswalker_mana_value_at_most_2;kicked:destroy_target_creature_or_planeswalker"
        }
        "Bone Shards" => {
            "additional_cost:sacrifice_a_creature_or_discard_a_card;spell:destroy_target_creature_or_planeswalker"
        }
        "Chain of Smog" => {
            "spell:target_player_discards_two;that_player_may_copy_with_new_target"
        }
        "Collective Brutality" => {
            "escalate:discard_a_card;choose_one_or_more:target_opponent_reveals_hand_you_choose_instant_or_sorcery_discards+target_creature_gets_minus_2_minus_2_until_end_of_turn+target_opponent_loses_2_you_gain_2"
        }
        "Natural Order" => {
            "additional_cost:sacrifice_a_green_creature;spell:search_green_creature_put_onto_battlefield_shuffle"
        }
        "Pest Infestation" => {
            "x_cost:xx;spell:destroy_up_to_x_target_artifacts_and_or_enchantments;create_twice_x_pest_tokens"
        }
        "Pest Token" => "dies:gain_1",
        "Tamiyo's Safekeeping" => {
            "spell:target_permanent_you_control_gains_hexproof_and_indestructible_until_end_of_turn;gain_life:2"
        }
        _ => super::build_eternal_c1c_v1::trigger_recipe_for(name),
    }
}
