//! Card-database recipes for the eternal catalog's wave E5a (Vintage Raker
//! Shops, Oath of Druids, Paradoxical Outcome, Blue Tinker and Beseech
//! Storm, Legacy Mystic Forge). `build_eternal_lurrus_v1` falls through to
//! these tables; runtime programs live in `src/eternal_e5a_v1.rs`. Every
//! row is keyed by a name that occurs only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, PermanentFilterRecipe, Special,
};
use AbilityCostRecipe::{Loyalty, ManaCost, PayLife, SacrificeSelf, Tap};

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

/// A planeswalker loyalty ability: sorcery speed, once per turn through its
/// loyalty cost.
const fn loyalty(
    cost: &'static [AbilityCostRecipe],
    program: &'static str,
    target_spec: &'static str,
) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect: AbilityEffectRecipe::StandardProgram(program),
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec,
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

/// Grim Monolith: "{4}: Untap this artifact." Its "{T}: Add {C}{C}{C}." is
/// the conditional tap yield below.
const GRIM_MONOLITH: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{4}")],
    AbilityEffectRecipe::StandardProgram("crate::eternal_e5a_v1::untap_source"),
    "None",
)];

/// Voltaic Key: "{1}, {T}: Untap target artifact."
const VOLTAIC_KEY: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}"), Tap],
    AbilityEffectRecipe::UntapTarget,
    "ArtifactPermanent",
)];

/// Sensei's Divining Top: "{1}: Look at the top three cards of your
/// library, then put them back in any order." and "{T}: Draw a card, then
/// put this artifact on top of its owner's library."
const SENSEIS_DIVINING_TOP: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[ManaCost("{1}")],
        AbilityEffectRecipe::StandardProgram("crate::eternal_e5a_v1::top_look"),
        "None",
    ),
    instant(
        &[Tap],
        AbilityEffectRecipe::StandardProgram("crate::eternal_e5a_v1::top_draw"),
        "None",
    ),
];

/// Inventors' Fair: "{4}, {T}, Sacrifice Inventors' Fair: Search your
/// library for an artifact card, reveal it, put it into your hand, then
/// shuffle. Activate only if you control three or more artifacts." The
/// condition is `eternal_e5a_v1::statics::activation_allowed`.
const INVENTORS_FAIR: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{4}"), Tap, SacrificeSelf],
    AbilityEffectRecipe::StandardProgram("crate::eternal_e5a_v1::inventors_fair_search"),
    "None",
)];

/// Mystic Forge: "{T}, Pay 1 life: Exile the top card of your library."
const MYSTIC_FORGE: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap, PayLife(1)],
    AbilityEffectRecipe::StandardProgram("crate::eternal_e5a_v1::exile_top_card"),
    "None",
)];

/// Bolas's Citadel: "{T}, Sacrifice ten nonland permanents: Each opponent
/// loses 10 life."
const BOLAS_CITADEL: [ActivatedAbilityRecipe; 1] = [instant(
    &[
        Tap,
        AbilityCostRecipe::SacrificeControlled {
            count: 10,
            filter: PermanentFilterRecipe::Nonland,
        },
    ],
    AbilityEffectRecipe::StandardProgram("crate::eternal_e5a_v1::citadel_drain"),
    "None",
)];

/// Oko, Thief of Crowns: "+2: Create a Food token.", "+1: Target artifact or
/// creature loses all abilities and becomes a green Elk creature with base
/// power and toughness 3/3." and "-5: Exchange control of target artifact
/// or creature you control and target creature an opponent controls with
/// power 3 or less."
const OKO: [ActivatedAbilityRecipe; 3] = [
    loyalty(&[Loyalty(2)], "crate::eternal_e5a_v1::oko_food", "None"),
    loyalty(&[Loyalty(1)], "crate::eternal_e5a_v1::oko_elk", "AnotherArtifactOrCreature"),
    loyalty(
        &[Loyalty(-5)],
        "crate::eternal_e5a_v1::oko_exchange",
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::ControlledArtifactOrCreatureThenOpponentSmallCreatureV1)",
    ),
];

/// Ugin, Eye of the Storms: "+2: You gain 3 life and draw a card.", "0: Add
/// {C}{C}{C}." and "-11: Search your library for any number of colorless
/// nonland cards, exile them, then shuffle. Until end of turn, you may cast
/// those cards without paying their mana costs."
const UGIN: [ActivatedAbilityRecipe; 3] = [
    loyalty(
        &[Loyalty(2)],
        "crate::eternal_e5a_v1::ugin_plus_two",
        "None",
    ),
    loyalty(&[Loyalty(0)], "crate::eternal_e5a_v1::ugin_zero", "None"),
    loyalty(
        &[Loyalty(-11)],
        "crate::eternal_e5a_v1::ugin_minus_eleven",
        "None",
    ),
];

/// Talon Gates of Madara: "{4}: Put this card from your hand onto the
/// battlefield." Its mana abilities are the ordinary {T}: Add {C} and the
/// additional "{1}, {T}: Add one mana of any color."
const TALON_GATES: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{4}")],
    effect: AbilityEffectRecipe::StandardProgram("crate::eternal_e5a_v1::talon_gates_put"),
    activation_zone: "Hand",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Grim Monolith" => &GRIM_MONOLITH,
        "Voltaic Key" => &VOLTAIC_KEY,
        "Sensei's Divining Top" => &SENSEIS_DIVINING_TOP,
        "Inventors' Fair" => &INVENTORS_FAIR,
        "Mystic Forge" => &MYSTIC_FORGE,
        "Bolas's Citadel" => &BOLAS_CITADEL,
        "Oko, Thief of Crowns" => &OKO,
        "Ugin, Eye of the Storms" => &UGIN,
        "Talon Gates of Madara" => &TALON_GATES,
        _ => super::build_eternal_e5b_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function of `eternal_e5a_v1`.
const fn program(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        // Bargain is `optional_additional_cost_for` below.
        "Beseech the Mirror" => program(
            "TargetSpec::None",
            "crate::eternal_e5a_v1::beseech_the_mirror",
        ),
        // A permanent spell that enters with X charge counters.
        "Chalice of the Void" => program(
            "TargetSpec::None",
            "crate::eternal_e5a_v1::chalice_of_the_void",
        ),
        "Extrapolate the Impossible" => program(
            "TargetSpec::None",
            "crate::eternal_e5a_v1::extrapolate_the_impossible",
        ),
        "Flash" => program("TargetSpec::None", "crate::eternal_e5a_v1::flash"),
        // "Choose two": the six pairs are modes 0 to 5 (`mode2_for`,
        // `mode3_for`, `additional_modes_for`).
        "Kozilek's Command" => program(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::PlayerThenPlayerV1)",
            "crate::eternal_e5a_v1::kozilek_spawn_scry",
        ),
        // Its second mode is `mode2_for` below.
        "Suplex" => program(
            "TargetSpec::Creature",
            "crate::eternal_e5a_v1::suplex_damage",
        ),
        // Storm is its cast trigger (`eternal_cards_v1::e3`).
        "Tendrils of Agony" => program(
            "TargetSpec::AnyPlayer",
            "crate::eternal_e5a_v1::tendrils_of_agony",
        ),
        "Vampiric Tutor" => program(
            "TargetSpec::None",
            "crate::eternal_e5a_v1::vampiric_tutor",
        ),
        "Yawgmoth's Will" => program(
            "TargetSpec::None",
            "crate::eternal_e5a_v1::yawgmoths_will",
        ),
        _ => return super::build_eternal_e5b_v1::special_for(name),
    })
}

/// Kozilek's Command's second pair, Suplex's "Exile target artifact."
pub(super) fn mode2_for(name: &str) -> &'static str {
    match name {
        "Kozilek's Command" => "Some(ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::PlayerThenCreatureV1), effect: crate::eternal_e5a_v1::kozilek_spawn_creature })",
        "Suplex" => "Some(ModeDef { target_spec: TargetSpec::ArtifactPermanent, effect: crate::eternal_e5a_v1::suplex_exile })",
        _ => super::build_eternal_e5b_v1::mode2_for(name),
    }
}

/// Kozilek's Command's third pair.
pub(super) fn mode3_for(name: &str) -> &'static str {
    match name {
        "Kozilek's Command" => "Some(ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::PlayerThenGraveyardCardsV1), effect: crate::eternal_e5a_v1::kozilek_spawn_cards })",
        _ => super::build_eternal_e5b_v1::mode3_for(name),
    }
}

/// Kozilek's Command's other three pairs.
pub(super) fn additional_modes_for(name: &str) -> &'static str {
    match name {
        "Kozilek's Command" => "&[ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::PlayerThenCreatureV1), effect: crate::eternal_e5a_v1::kozilek_scry_creature }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::PlayerThenGraveyardCardsV1), effect: crate::eternal_e5a_v1::kozilek_scry_cards }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::CreatureThenGraveyardCardsV1), effect: crate::eternal_e5a_v1::kozilek_creature_cards }]",
        _ => super::build_eternal_e5b_v1::additional_modes_for(name),
    }
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Serra's Emissary" | "The Fantasticar" => &["Keywords::FLYING"],
        "Fantasticar Construct Token" => &["Keywords::FLYING", "Keywords::HASTE"],
        _ => super::build_eternal_e5b_v1::keywords_for(name),
    }
}

/// Card-database tokens for the triggered abilities in
/// `eternal_e5a_v1::triggers_for`, its queued triggers and its statics.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Beseech the Mirror" => {
            "bargain:sacrifice_artifact_enchantment_or_token;spell:search_any_card_hidden:if_bargained_may_cast_free_mana_value_at_most:4:else_hand"
        }
        "Bolas's Citadel" => {
            "static:controller_may_look_at_top_and_play_lands_and_cast_spells_from_top:pay_life_equal_mana_value_instead_of_mana_cost"
        }
        "Chalice of the Void" => {
            "enters_with_x_charge_counters;any_player_casts_spell:mana_value_equals_charge_counters:counter_that_spell"
        }
        "Council of Reeds" => {
            "static:legend_rule_doesnt_apply_to_controlled_creatures;controller_combat_if_cast_noncreature_this_turn:create_token_copy_of_source"
        }
        "Extrapolate the Impossible" => "spell:wish_two_outside_game:nothing_outside_game_in_best_of_one",
        "Flash" => {
            "spell:may_put_creature_from_hand:sacrifice_unless_pay_mana_cost_reduced_generic:2"
        }
        "Forbidden Orchard" => "tapped_for_mana:target_opponent_creates_colorless_spirit_1_1",
        "Glaring Fleshraker" => {
            "controller_casts_colorless_spell:create_eldrazi_spawn;other_colorless_controlled_creature_enters:damage_each_opponent:1"
        }
        "Grim Monolith" => "static:doesnt_untap_during_untap_step",
        "Inventors' Fair" => {
            "controller_upkeep_if_three_artifacts:gain_life:1;activation_condition:controls_three_artifacts"
        }
        "Kozilek's Command" => {
            "modes:choose_two:spawn_x,scry_x_draw,exile_creature_mana_value_at_most_x,exile_up_to_x_graveyard_cards:x_at_least_target_needs"
        }
        "Mystic Forge" => {
            "static:controller_may_look_at_top_and_cast_artifact_or_colorless_spells_from_top"
        }
        "Oath of Druids" => {
            "each_upkeep:target_opponent_with_more_creatures:upkeep_player_may_reveal_until_creature:battlefield_rest_graveyard"
        }
        "Oko, Thief of Crowns" => {
            "plus1:target_loses_all_abilities_becomes_green_elk_creature_base_3_3;minus5:exchange_control"
        }
        "Planar Nexus" => "static:every_nonbasic_land_type",
        "Scrawling Crawler" => {
            "controller_upkeep:each_player_draws:1;opponent_draws:that_player_loses_life:1"
        }
        "Sensei's Divining Top" => "activated:draw_then_source_to_library_top",
        "Serra's Emissary" => {
            "as_enters:choose_card_type;static:controller_and_controlled_creatures_protection_from_chosen_type"
        }
        "Suplex" => "spell:damage_target_creature:3:exile_if_dies_this_turn",
        "Talon Gates of Madara" => "etb:up_to_one_target_creature_phases_out",
        "Tendrils of Agony" => "cast:storm:copies_may_choose_new_targets",
        "The Endstone" => {
            "controller_plays_land_or_casts_spell:draw:1;controller_end_step:life_becomes_half_starting_rounded_up"
        }
        "The Fantasticar" => {
            "controller_casts_noncreature_spell:may_become_artifact_creature_eot;controller_casts_fourth_noncreature_spell_each_turn:may_sacrifice_create_four_construct_4_4_flying_haste"
        }
        "Ugin, Eye of the Storms" => {
            "cast:exile_up_to_one_target_colored_permanent;controller_casts_colorless_spell:exile_up_to_one_target_colored_permanent;minus11:free_cast_until_end_of_turn"
        }
        "Urza's Workshop" => "mana:metalcraft:add_c_per_controlled_urzas_land",
        "Vampiric Tutor" => "spell:search_any_card_top_after_shuffle_hidden;lose_life:2",
        "Yawgmoth's Will" => {
            "spell:until_end_of_turn_play_lands_and_cast_spells_from_graveyard;replacement:cards_to_graveyard_exiled_instead"
        }
        _ => super::build_eternal_e5b_v1::trigger_recipe_for(name),
    }
}

/// Bargain (702.166).
pub(super) fn optional_additional_cost_for(name: &str) -> &'static str {
    match name {
        "Beseech the Mirror" => "Some(OptionalAdditionalCostDef::Bargain)",
        _ => "None",
    }
}

/// Tap-for-colorless permanents with a fixed or board-dependent yield.
pub(super) fn conditional_tap_yield_for(name: &str) -> &'static str {
    match name {
        // "{T}: Add {C}{C}{C}."
        "Grim Monolith" => "Some(DynamicValueDef::Fixed(3))",
        // Its two colorless abilities, offered as one adding the larger
        // amount (`eternal_e5a_v1::statics::urzas_workshop_yield`).
        "Urza's Workshop" => "Some(DynamicValueDef::UrzasLandsIfMetalcraftV1)",
        _ => "None",
    }
}

pub(super) fn conditional_tap_yield_program_for(name: &str) -> Option<&'static str> {
    match name {
        "Grim Monolith" => Some("mana_ability_add_grim_monolith"),
        "Urza's Workshop" => Some("mana_ability_add_urzas_workshop"),
        _ => None,
    }
}

/// "{1}, {T}: Add one mana of any color." (Planar Nexus, Talon Gates of
/// Madara), Heap Gate's paid any-color ability.
pub(super) fn additional_mana_abilities_for(name: &str) -> Option<&'static str> {
    match name {
        "Planar Nexus" | "Talon Gates of Madara" => Some("&[AdditionalManaAbilityDef { colors: &[ManaColor::W, ManaColor::U, ManaColor::B, ManaColor::R, ManaColor::G], mana_cost: Cost { pips: &[], generic: 1, x_count: 0 }, ability: ManaAbilityDef { cost: ManaAbilityCostDef::TapSelf, amount: ManaAbilityAmountDef::Fixed(1), controller_damage: 0, max_activations_per_turn: None } }]"),
        _ => None,
    }
}

/// The Construct tokens keep the token's printed name.
pub(super) fn object_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Fantasticar Construct Token" => Some("Construct Token"),
        _ => super::build_eternal_e7a_v1::object_name_for(name),
    }
}
