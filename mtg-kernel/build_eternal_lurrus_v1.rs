//! Card-database recipes for the eternal catalog's Vintage Lurrus lists
//! (wave E4b: companions and the Lurrus decks). `build_eternal_vintage_v1`
//! falls through to these tables; runtime programs live in
//! `src/eternal_lurrus_v1.rs`. Every row is keyed by a name that occurs only
//! in the eternal registry.

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

/// Lurrus of the Dream-Den's companion reminder: "you may put it into your
/// hand from outside the game for {3} as a sorcery." The engine performs it
/// as a special action (702.139c) from the companion's place in exile; it
/// never uses the stack.
const LURRUS: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{3}")],
    effect: AbilityEffectRecipe::StandardProgram("crate::eternal_lurrus_v1::companion_to_hand"),
    activation_zone: "Exile",
    sorcery_speed_only: true,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Aether Spellbomb: "{U}, Sacrifice this artifact: Return target creature
/// to its owner's hand." and "{1}, Sacrifice this artifact: Draw a card."
const AETHER_SPELLBOMB: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[ManaCost("{U}"), SacrificeSelf],
        AbilityEffectRecipe::MoveAllTargetsToHand,
        "Creature",
    ),
    instant(
        &[ManaCost("{1}"), SacrificeSelf],
        AbilityEffectRecipe::DrawCards(1),
        "None",
    ),
];

/// Soul-Guide Lantern: "{T}, Sacrifice this artifact: Exile each opponent's
/// graveyard." and "{1}, {T}, Sacrifice this artifact: Draw a card."
const SOUL_GUIDE_LANTERN: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[Tap, SacrificeSelf],
        AbilityEffectRecipe::StandardProgram("crate::eternal_lurrus_v1::exile_opponents_graveyard"),
        "None",
    ),
    instant(
        &[ManaCost("{1}"), Tap, SacrificeSelf],
        AbilityEffectRecipe::DrawCards(1),
        "None",
    ),
];

/// Psychic Frog: "Discard a card: Put a +1/+1 counter on this creature."
/// and "Exile three cards from your graveyard: This creature gains flying
/// until end of turn."
const PSYCHIC_FROG: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[AbilityCostRecipe::DiscardCards(1)],
        AbilityEffectRecipe::PutPlusOneCounterOnSource,
        "None",
    ),
    instant(
        &[AbilityCostRecipe::ExileGraveyardCards(3)],
        AbilityEffectRecipe::StandardProgram("crate::eternal_lurrus_v1::psychic_frog_flying"),
        "None",
    ),
];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Lurrus of the Dream-Den" => &LURRUS,
        "Aether Spellbomb" => &AETHER_SPELLBOMB,
        "Soul-Guide Lantern" => &SOUL_GUIDE_LANTERN,
        "Psychic Frog" => &PSYCHIC_FROG,
        _ => super::build_eternal_e5a_v1::activated_ability_recipes_for(name),
    }
}

/// A spell whose program is a runtime function of `eternal_lurrus_v1`.
const fn program(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Demonic Tutor" => program(
            "TargetSpec::None",
            "crate::eternal_lurrus_v1::demonic_tutor",
        ),
        "Merchant Scroll" => program(
            "TargetSpec::None",
            "crate::eternal_lurrus_v1::merchant_scroll",
        ),
        "Flow State" => program("TargetSpec::None", "crate::eternal_lurrus_v1::flow_state"),
        // Delve is `delve_for` below.
        "Dig Through Time" => program(
            "TargetSpec::None",
            "crate::eternal_lurrus_v1::dig_through_time",
        ),
        "Treasure Cruise" => Special::DrawCards(3),
        // Its alternative cost is `alt_cost_for` below.
        "Gush" => Special::DrawCards(2),
        // Its alternative cost is `alt_cost_for` below.
        "Force of Negation" => program(
            "TargetSpec::NoncreatureSpellOnStack",
            "crate::eternal_lurrus_v1::force_of_negation",
        ),
        "Gitaxian Probe" => program(
            "TargetSpec::AnyPlayer",
            "crate::eternal_lurrus_v1::gitaxian_probe",
        ),
        "Swords to Plowshares" => program(
            "TargetSpec::Creature",
            "crate::eternal_lurrus_v1::swords_to_plowshares",
        ),
        "Prismatic Ending" => program(
            "TargetSpec::NonlandPermanent",
            "crate::eternal_lurrus_v1::prismatic_ending",
        ),
        "Dismember" => Special::PumpCreature {
            power: -5,
            toughness: -5,
        },
        // Storm is its cast trigger (`eternal_cards_v1::e3`).
        "Brain Freeze" => program(
            "TargetSpec::AnyPlayer",
            "crate::eternal_lurrus_v1::brain_freeze",
        ),
        "Mana Drain" => program(
            "TargetSpec::AnySpellOnStack",
            "crate::eternal_lurrus_v1::mana_drain",
        ),
        "Forth Eorlingas!" => program(
            "TargetSpec::None",
            "crate::eternal_lurrus_v1::forth_eorlingas",
        ),
        // Modes two to six are `mode2_for`, `mode3_for` and
        // `additional_modes_for` below.
        "Flame of Anor" => program(
            "TargetSpec::AnyPlayer",
            "crate::eternal_lurrus_v1::flame_of_anor_draw",
        ),
        _ => return super::build_eternal_e5a_v1::special_for(name),
    })
}

/// Flame of Anor's second mode: "Destroy target artifact."
pub(super) fn mode2_for(name: &str) -> &'static str {
    match name {
        "Flame of Anor" => "Some(ModeDef { target_spec: TargetSpec::ArtifactPermanent, effect: crate::eternal_lurrus_v1::flame_of_anor_artifact })",
        _ => super::build_eternal_e5a_v1::mode2_for(name),
    }
}

/// Flame of Anor's third mode: "Flame of Anor deals 5 damage to target
/// creature."
pub(super) fn mode3_for(name: &str) -> &'static str {
    match name {
        "Flame of Anor" => "Some(ModeDef { target_spec: TargetSpec::Creature, effect: crate::eternal_lurrus_v1::flame_of_anor_creature })",
        _ => super::build_eternal_e5a_v1::mode3_for(name),
    }
}

/// Flame of Anor's pairs of modes, offered only to a caster who controls a
/// Wizard (`eternal_lurrus_v1::spell_mode_allowed`).
pub(super) fn additional_modes_for(name: &str) -> &'static str {
    match name {
        "Flame of Anor" => "&[ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::PlayerThenArtifactV1), effect: crate::eternal_lurrus_v1::flame_of_anor_draw_artifact }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::PlayerThenCreatureV1), effect: crate::eternal_lurrus_v1::flame_of_anor_draw_creature }, ModeDef { target_spec: TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::ArtifactThenCreatureV1), effect: crate::eternal_lurrus_v1::flame_of_anor_artifact_creature }]",
        _ => super::build_eternal_e5a_v1::additional_modes_for(name),
    }
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Lurrus of the Dream-Den" => &["Keywords::LIFELINK"],
        "Snapcaster Mage" => &["Keywords::FLASH"],
        "Human Knight Token" => &["Keywords::TRAMPLE", "Keywords::HASTE"],
        _ => super::build_eternal_e5a_v1::keywords_for(name),
    }
}

/// Card-database tokens for the triggered abilities in
/// `eternal_lurrus_v1::triggers_for`, its queued triggers and its statics.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Lurrus of the Dream-Den" => {
            "companion:each_permanent_card_in_starting_deck_mana_value_at_most:2:special_action_pay_3_to_hand_as_sorcery_from_exile;static:once_each_controller_turn_may_cast_permanent_spell_mana_value_at_most_2_from_graveyard"
        }
        "Thundertrap Trainer" | "Thundertrap Trainer Offspring Token" => {
            "etb_if_offspring_paid:create_one_one_token_copy;etb:look_top_4:may_reveal_noncreature_nonland_to_hand:rest_bottom_random"
        }
        "Snapcaster Mage" => {
            "etb:target_instant_or_sorcery_card_in_own_graveyard:gains_flashback_equal_to_mana_cost_until_end_of_turn"
        }
        "Portable Hole" => {
            "etb:target_opponent_nonland_permanent_mana_value_at_most_2:exile_until_source_leaves"
        }
        "Soul-Guide Lantern" => "etb:target_card_in_a_graveyard:exile",
        "Underworld Breach" => {
            "static:controller_graveyard_nonland_cards_have_escape:mana_cost_plus_exile_3_other;each_end_step:sacrifice_source"
        }
        "Psychic Frog" => "combat_damage_player_or_planeswalker:draw:1",
        "Hexing Squelcher" => {
            "spell_cant_be_countered;ward_pay_life:2;static:controller_spells_cant_be_countered;static:other_controlled_creatures_have_ward_pay_life:2"
        }
        "Null Rod" => "static:artifact_activated_abilities_cant_be_activated",
        "Pithing Needle" => {
            "as_enters:choose_visible_card_name;static:named_sources_non_mana_abilities_cant_be_activated"
        }
        "Mox Opal" => "mana:metalcraft:tap_add_one_any_color_if_controls_three_artifacts",
        "Mana Drain" => "spell:counter_target_spell;delayed:controller_next_main_phase:add_colorless_equal_countered_mana_value",
        "Forth Eorlingas!" => {
            "spell:create_x_human_knight_tokens;delayed:this_turn_controlled_creatures_combat_damage_players:become_monarch"
        }
        "Flame of Anor" => "modes:choose_one_or_two_if_controls_wizard_as_cast",
        "Prismatic Ending" => "converge:colors_spent_bound_target_mana_value",
        "Brain Freeze" => "cast:storm:copies_may_choose_new_targets",
        _ => super::build_eternal_e5a_v1::trigger_recipe_for(name),
    }
}

/// Alternative costs. Force of Negation: "If it's not your turn, you may
/// exile a blue card from your hand rather than pay this spell's mana
/// cost." Gush: "You may return two Islands you control to their owner's
/// hand rather than pay this spell's mana cost."
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Force of Negation" => "Some(AltCostDef { components: &[CostComponent::ExileCardFromHandWithColorV1(ManaColor::U)], condition: AltCostCondition::NotYourTurnV1 })",
        "Gush" => "Some(AltCostDef { components: &[CostComponent::ReturnControlledPermanentsToOwnersHandV1 { count: 2, filter: PermanentFilterDef::LandWithSubtype(Subtype::Island) }], condition: AltCostCondition::Always })",
        _ => "None",
    }
}

/// Delve (702.66).
pub(super) fn delve_for(name: &str) -> bool {
    matches!(name, "Treasure Cruise" | "Dig Through Time")
}

/// Offspring {4} reuses kicker's optional additional cost.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    match name {
        "Thundertrap Trainer" => Some("{4}"),
        _ => None,
    }
}

/// Hexing Squelcher: "Ward--Pay 2 life."
pub(super) fn ward_cost_for(name: &str) -> &'static str {
    match name {
        "Hexing Squelcher" => "Some(WardCostDef::PayLife(2))",
        _ => "None",
    }
}

/// The offspring token copies Thundertrap Trainer's name.
pub(super) fn object_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Thundertrap Trainer Offspring Token" => Some("Thundertrap Trainer"),
        _ => super::build_eternal_e5a_v1::object_name_for(name),
    }
}
