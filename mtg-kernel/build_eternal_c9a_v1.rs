//! Card-database recipes for the eternal catalog's Vintage Cube wave C9a
//! (Aang, Swift Savior // Aang and La, Ocean's Fury; Badgermole Cub; Batroc
//! the Leaper; Blightsteel Colossus; Crabomination; and Fear of Missing Out).
//! Wave C8c's tables fall through to these, and these fall through to wave
//! C9b's (`build_eternal_c9b_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c9a_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, PermanentFilterRecipe, Special,
};
use AbilityCostRecipe::{ManaCost, TapControlled};
use PermanentFilterRecipe::ArtifactOrCreature;

/// One payment of Aang, Swift Savior's "Waterbend {8}: Transform Aang."
/// Printed on face 0 only. Waterbend lets each generic mana of the {8} be paid
/// by tapping an untapped artifact or creature the activator controls; each
/// recipe carries zero to eight taps and the remaining mana.
const fn aang_waterbend(cost: &'static [AbilityCostRecipe]) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect: AbilityEffectRecipe::StandardProgram(
            "crate::eternal_cube_c8a_v1::transform_front_to_back",
        ),
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

/// Aang, Swift Savior's "Waterbend {8}: Transform Aang." with zero to eight
/// waterbend taps.
const AANG_SWIFT_SAVIOR: [ActivatedAbilityRecipe; 9] = [
    aang_waterbend(&[ManaCost("{8}")]),
    aang_waterbend(&[
        ManaCost("{7}"),
        TapControlled {
            count: 1,
            filter: ArtifactOrCreature,
        },
    ]),
    aang_waterbend(&[
        ManaCost("{6}"),
        TapControlled {
            count: 2,
            filter: ArtifactOrCreature,
        },
    ]),
    aang_waterbend(&[
        ManaCost("{5}"),
        TapControlled {
            count: 3,
            filter: ArtifactOrCreature,
        },
    ]),
    aang_waterbend(&[
        ManaCost("{4}"),
        TapControlled {
            count: 4,
            filter: ArtifactOrCreature,
        },
    ]),
    aang_waterbend(&[
        ManaCost("{3}"),
        TapControlled {
            count: 5,
            filter: ArtifactOrCreature,
        },
    ]),
    aang_waterbend(&[
        ManaCost("{2}"),
        TapControlled {
            count: 6,
            filter: ArtifactOrCreature,
        },
    ]),
    aang_waterbend(&[
        ManaCost("{1}"),
        TapControlled {
            count: 7,
            filter: ArtifactOrCreature,
        },
    ]),
    aang_waterbend(&[
        ManaCost("{0}"),
        TapControlled {
            count: 8,
            filter: ArtifactOrCreature,
        },
    ]),
];

/// Activated abilities of wave C9a cards.
pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Aang, Swift Savior" => &AANG_SWIFT_SAVIOR,
        _ => super::build_eternal_c9b_v1::activated_ability_recipes_for(name),
    }
}

/// Face restriction for wave C9a activated abilities: Aang's Waterbend {8}
/// is printed on face 0 only.
pub(super) fn activated_ability_face_for(name: &str, _index: usize) -> Option<u8> {
    match name {
        "Aang, Swift Savior" => Some(0),
        _ => None,
    }
}

/// Back faces of wave C9a double-faced cards.
pub(super) fn transform_face_for(name: &str) -> &'static str {
    match name {
        "Aang, Swift Savior" => "Some(TransformFaceDef { name: \"Aang and La, Ocean's Fury\", types: &[CardType::Creature], subtypes: &[Subtype::Avatar, Subtype::Spirit, Subtype::Ally], colors: &[], power: Some(5), toughness: Some(5), keywords: Keywords(Keywords::REACH.0 | Keywords::TRAMPLE.0) })",
        _ => "None",
    }
}

pub(super) fn transform_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Aang, Swift Savior" => Some("Aang and La, Ocean's Fury"),
        _ => None,
    }
}

/// Spell programs: Batroc the Leaper preserves his multikicker count across
/// stack resolution so his entry replacement puts that many +1/+1 counters on
/// him and his ETB trigger targets up to X targets.
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Batroc the Leaper" => Some(Special::StandardProgram {
            target_spec: "TargetSpec::None",
            program: "crate::eternal_cube_c9a_v1::batroc_enter",
        }),
        _ => super::build_eternal_c9b_v1::special_for(name),
    }
}

/// No C9a card has single kicker (Batroc uses multikicker).
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c9b_v1::kicker_cost_for(name)
}

/// No C9a card is an Aura or an Equipment.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c9b_v1::attachment_for(name)
}

/// No C9a card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c9b_v1::flashback_for(name)
}

/// Crabomination: "Emerge from artifact {5}{B}{B}".
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Crabomination" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::B)], generic: 5, x_count: 0 }), CostComponent::SacrificeControlled { count: 1, filter: PermanentFilter::Artifact }], condition: AltCostCondition::Always })",
        _ => super::build_eternal_c9b_v1::alt_cost_for(name),
    }
}

/// Printed evergreen keywords on front faces, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Aang, Swift Savior" => &["Keywords::FLASH", "Keywords::FLYING"],
        // `"indestructible"` in `cards_v1.json` already emits `Keywords::INDESTRUCTIBLE`.
        "Blightsteel Colossus" => &["Keywords::TRAMPLE", "Keywords::INFECT"],
        _ => super::build_eternal_c9b_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Aang, Swift Savior" => "flash;flying;etb:airbend_up_to_one_other_creature_or_spell_cast_for_2;waterbend_8:transform;back_face:aang_and_la_oceans_fury:5_5:colorless:avatar_spirit_ally:reach:trample:attacks:put_plus1_counter_on_each_tapped_controlled_creature",
        "Badgermole Cub" => "etb:earthbend_1_target_controlled_land_0_0_haste_still_land_plus1_counter_return_tapped_on_die_or_exile;triggered_mana:tap_controlled_creature_for_mana_add_g",
        "Batroc the Leaper" => "multikicker:2;enters_with_plus1_counter_per_kick;etb:deals_power_damage_to_each_of_up_to_kicks_any_targets",
        "Blightsteel Colossus" => "trample;infect;indestructible;replacement:would_go_to_graveyard_from_anywhere_reveal_shuffle_into_owners_library",
        "Crabomination" => "emerge_from_artifact:5bb;etb:target_opponent_exiles_top_library_random_graveyard_random_hand_may_cast_one_without_mana_cost",
        "Fear of Missing Out" => "etb:discard_1_then_draw_1;delirium:first_attack_each_turn_untap_target_creature_additional_combat_phase",
        _ => super::build_eternal_c9b_v1::trigger_recipe_for(name),
    }
}
