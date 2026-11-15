//! Card-database recipes for the eternal catalog's Vintage Cube wave C1a
//! (white and blue instants and sorceries). Wave P3h's tables fall through to these, and
//! these fall through to wave C1b's
//! (`build_eternal_c1b_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c1a_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special, StackSpellFilter,
};
use AbilityCostRecipe::{DiscardSelf, ManaCost};

/// Miscalculation: "Cycling {2}" (702.29): activated from hand, discarding
/// the card as part of its cost, to draw a card.
const MISCALCULATION: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{2}"), DiscardSelf],
    effect: AbilityEffectRecipe::DrawCards(1),
    activation_zone: "Hand",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Miscalculation's cycling.
pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Miscalculation" => &MISCALCULATION,
        _ => super::build_eternal_c1b_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function of `eternal_cube_c1a_v1`.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs. Timetwister is Echo of Eons' program without its
/// flashback.
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Memory Lapse" => Some(spell(
            "TargetSpec::AnySpellOnStack",
            "crate::eternal_cube_c1a_v1::memory_lapse",
        )),
        // Mana Leak's program with {2}; its cycling is
        // `activated_ability_recipes_for` above.
        "Miscalculation" => Some(Special::CounterUnlessPaysGeneric {
            filter: StackSpellFilter::Any,
            generic: 2,
        }),
        // Its overload is `overload_for` below.
        "Damn" => Some(spell(
            "TargetSpec::Creature",
            "crate::eternal_premodern_p2c_v1::destroy_target_creature_no_regeneration",
        )),
        // Its ten modes are `MYSTIC_MODE_2`, `MYSTIC_MODE_3` and
        // `MYSTIC_ADDITIONAL_MODES` below; this is the first (three counters).
        "Mystic Confluence" => Some(spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::C1aV1(crate::eternal_cube_c1a_v1::targets::C1aTargetV1::MysticSlotsV1 { spells: 3, creatures: 0 }))",
            "crate::eternal_cube_c1a_v1::mystic_confluence_0",
        )),
        "Occult Epiphany" => Some(spell(
            "TargetSpec::None",
            "crate::eternal_cube_c1a_v1::occult_epiphany",
        )),
        "Oust" => Some(spell(
            "TargetSpec::Creature",
            "crate::eternal_cube_c1a_v1::oust",
        )),
        "Path to Exile" => Some(spell(
            "TargetSpec::Creature",
            "crate::eternal_cube_c1a_v1::path_to_exile",
        )),
        "Remand" => Some(spell(
            "TargetSpec::AnySpellOnStack",
            "crate::eternal_cube_c1a_v1::remand",
        )),
        "Reprieve" => Some(spell(
            "TargetSpec::AnySpellOnStack",
            "crate::eternal_cube_c1a_v1::reprieve",
        )),
        "Stern Scolding" => Some(spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::CreatureSpellPowerOrToughnessAtMostTwoV1)",
            "crate::eternal_cube_c1a_v1::stern_scolding",
        )),
        "Time Spiral" => Some(spell(
            "TargetSpec::None",
            "crate::eternal_cube_c1a_v1::time_spiral",
        )),
        "Timetwister" => Some(spell(
            "TargetSpec::None",
            "crate::eternal_legacy_e7a_v1::echo_of_eons",
        )),
        "Unexpectedly Absent" => Some(spell(
            "TargetSpec::NonlandPermanent",
            "crate::eternal_cube_c1a_v1::unexpectedly_absent",
        )),
        "Winds of Abandon" => Some(spell(
            "TargetSpec::OpponentControlledCreature",
            "crate::eternal_cube_c1a_v1::winds_of_abandon",
        )),
        _ => super::build_eternal_c1b_v1::special_for(name),
    }
}

/// Overload (702.96): the paid second form of the card, as for Vandalblast
/// and Electrickery (`build_eternal_s1d_v1.rs`, which calls these three).
/// Damn {2}{W}{W}; Winds of Abandon {4}{W}{W}.
pub(super) fn overload_for(name: &str) -> &'static str {
    match name {
        "Damn" => "Some(AdventureDef { name: \"Damn Overload\", cost: Cost { pips: &[Pip::Colored(ManaColor::W), Pip::Colored(ManaColor::W)], generic: 2, x_count: 0 }, types: &[CardType::Sorcery], target_spec: TargetSpec::None, effect: ability_effect_program_eternal_cube_c1a_v1_damn_overload })",
        "Winds of Abandon" => "Some(AdventureDef { name: \"Winds of Abandon Overload\", cost: Cost { pips: &[Pip::Colored(ManaColor::W), Pip::Colored(ManaColor::W)], generic: 4, x_count: 0 }, types: &[CardType::Sorcery], target_spec: TargetSpec::None, effect: ability_effect_program_eternal_cube_c1a_v1_winds_of_abandon_overload })",
        _ => "None",
    }
}

/// The overloaded effect of `overload_for`'s card.
pub(super) fn overload_effect_for(name: &str) -> Option<AbilityEffectRecipe> {
    match name {
        "Damn" => Some(AbilityEffectRecipe::StandardProgram(
            "crate::eternal_cube_c1a_v1::damn_overload",
        )),
        "Winds of Abandon" => Some(AbilityEffectRecipe::StandardProgram(
            "crate::eternal_cube_c1a_v1::winds_of_abandon_overload",
        )),
        _ => None,
    }
}

/// The name of the overloaded form.
pub(super) fn overload_face_for(name: &str) -> Option<&'static str> {
    match name {
        "Damn" => Some("Damn Overload"),
        "Winds of Abandon" => Some("Winds of Abandon Overload"),
        _ => None,
    }
}

// Mystic Confluence: "Choose three. You may choose the same mode more than
// once." Its modes are the ten multisets of three of "counter target spell
// unless its controller pays {3}" (c), "return target creature to its owner's
// hand" (b) and "draw a card" (d), in the order ccc, ccb, ccd, cbb, cbd, cdd,
// bbb, bbd, bdd, ddd. Each filter takes one target per chosen counter or
// bounce, spells first (`eternal_cube_c1a_v1::targets`). The first is
// `special_for`'s program; `build_eternal_s1m_v1.rs` calls these three.

/// ccb: two counters and a bounce.
pub(super) const MYSTIC_MODE_2: &str = "Some(ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::C1aV1(crate::eternal_cube_c1a_v1::targets::C1aTargetV1::MysticSlotsV1 { spells: 2, creatures: 1 })), effect: crate::eternal_cube_c1a_v1::mystic_confluence_1 })";

/// ccd: two counters and a draw.
pub(super) const MYSTIC_MODE_3: &str = "Some(ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::C1aV1(crate::eternal_cube_c1a_v1::targets::C1aTargetV1::MysticSlotsV1 { spells: 2, creatures: 0 })), effect: crate::eternal_cube_c1a_v1::mystic_confluence_2 })";

/// cbb, cbd, cdd, bbb, bbd, bdd and ddd.
pub(super) const MYSTIC_ADDITIONAL_MODES: &str = "&[ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::C1aV1(crate::eternal_cube_c1a_v1::targets::C1aTargetV1::MysticSlotsV1 { spells: 1, creatures: 2 })), effect: crate::eternal_cube_c1a_v1::mystic_confluence_3 }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::C1aV1(crate::eternal_cube_c1a_v1::targets::C1aTargetV1::MysticSlotsV1 { spells: 1, creatures: 1 })), effect: crate::eternal_cube_c1a_v1::mystic_confluence_4 }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::C1aV1(crate::eternal_cube_c1a_v1::targets::C1aTargetV1::MysticSlotsV1 { spells: 1, creatures: 0 })), effect: crate::eternal_cube_c1a_v1::mystic_confluence_5 }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::C1aV1(crate::eternal_cube_c1a_v1::targets::C1aTargetV1::MysticSlotsV1 { spells: 0, creatures: 3 })), effect: crate::eternal_cube_c1a_v1::mystic_confluence_6 }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::C1aV1(crate::eternal_cube_c1a_v1::targets::C1aTargetV1::MysticSlotsV1 { spells: 0, creatures: 2 })), effect: crate::eternal_cube_c1a_v1::mystic_confluence_7 }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::C1aV1(crate::eternal_cube_c1a_v1::targets::C1aTargetV1::MysticSlotsV1 { spells: 0, creatures: 1 })), effect: crate::eternal_cube_c1a_v1::mystic_confluence_8 }, ModeDef { target_spec: TargetSpec::None, effect: crate::eternal_cube_c1a_v1::mystic_confluence_9 }]";

/// No C1a card has kicker yet.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c1b_v1::kicker_cost_for(name)
}

/// No C1a card is an Aura or an Equipment yet.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c1b_v1::attachment_for(name)
}

/// No C1a card has flashback yet.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c1b_v1::flashback_for(name)
}

/// No C1a card has an alternative cost yet.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    super::build_eternal_c1b_v1::alt_cost_for(name)
}

/// No C1a card has a printed evergreen keyword yet.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    super::build_eternal_c1b_v1::keywords_for(name)
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Damn" => "spell:destroy_target_creature:cant_be_regenerated;overload:{2}{W}{W}:destroy_each_creature:cant_be_regenerated",
        "Memory Lapse" => "spell:counter_target_spell:put_on_top_of_owners_library_instead",
        "Miscalculation" => "spell:counter_target_spell_unless_its_controller_pays_2;cycling:2",
        "Mystic Confluence" => "spell:choose_three_modes_repeatable:counter_target_spell_unless_its_controller_pays_3+return_target_creature_to_owners_hand+draw_1",
        "Occult Epiphany" => "spell:draw_x:discard_x:create_1_1_white_flying_spirit_per_card_type_among_discarded",
        "Oust" => {
            "spell:put_target_creature_second_from_top_of_owners_library:its_controller_gains_3"
        }
        "Path to Exile" => {
            "spell:exile_target_creature:its_controller_may_search_basic_land_to_battlefield_tapped"
        }
        "Remand" => "spell:counter_target_spell:put_into_owners_hand_instead:draw_1",
        "Reprieve" => "spell:return_target_spell_to_owners_hand:draw_1",
        "Stern Scolding" => "spell:counter_target_creature_spell_power_or_toughness_at_most_2",
        "Time Spiral" => "spell:exile_self:each_player_shuffles_hand_and_graveyard_into_library_draws_7:untap_up_to_6_lands",
        "Timetwister" => "spell:each_player_shuffles_hand_and_graveyard_into_library_draws_7",
        "Unexpectedly Absent" => "spell:put_target_nonland_permanent_into_owners_library_beneath_top_x",
        "Winds of Abandon" => "spell:exile_target_creature_you_dont_control:its_controller_searches_basic_land_to_battlefield_tapped;overload:{4}{W}{W}:each",
        _ => super::build_eternal_c1b_v1::trigger_recipe_for(name),
    }
}
