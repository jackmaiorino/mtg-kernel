//! Card-database recipes for the eternal catalog's Premodern wave P1b:
//! Enchantress, Replenish, Oath of Druids, Landstill, Psychatog and Grow.
//! `build_eternal_e9a_v1` and the other eternal tables fall through to
//! these; runtime programs live in `src/eternal_premodern_p1b_v1.rs`. Every
//! row is keyed by a name that occurs only in the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, PermanentFilterRecipe, Special,
};
use AbilityCostRecipe::{DiscardSelf, ManaCost, SacrificeSelf, Tap};

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

/// A cycling ability (702.29): activated from hand, discarding the card as
/// part of its cost, to draw a card. The Decrees' "When you cycle" triggers
/// are queued as the ability is paid
/// (`eternal_premodern_p1b_v1::statics::activation_paid`).
const fn cycling(cost: &'static [AbilityCostRecipe]) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect: AbilityEffectRecipe::DrawCards(1),
        activation_zone: "Hand",
        sorcery_speed_only: false,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

/// A program of `eternal_premodern_p1b_v1`.
const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Attunement: "Return this enchantment to its owner's hand: Draw three
/// cards, then discard four cards." The return is paid as the ability is put
/// on the stack (`eternal_premodern_p1b_v1::statics::activation_paid`); the
/// printed cost here is the empty mana cost.
const ATTUNEMENT: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{0}")],
    program("crate::eternal_premodern_p1b_v1::attunement"),
    "None",
)];

/// Aura of Silence: "Sacrifice this enchantment: Destroy target artifact or
/// enchantment." Seal of Cleansing has the same ability.
const SACRIFICE_DESTROY_ARTIFACT_OR_ENCHANTMENT: [ActivatedAbilityRecipe; 1] = [instant(
    &[SacrificeSelf],
    AbilityEffectRecipe::DestroyTarget,
    "ArtifactOrEnchantmentPermanent",
)];

/// Decree of Justice: "Cycling {2}{W}".
const DECREE_OF_JUSTICE: [ActivatedAbilityRecipe; 1] =
    [cycling(&[ManaCost("{2}{W}"), DiscardSelf])];

/// Decree of Silence: "Cycling {4}{U}{U}".
const DECREE_OF_SILENCE: [ActivatedAbilityRecipe; 1] =
    [cycling(&[ManaCost("{4}{U}{U}"), DiscardSelf])];

/// Dust Bowl: "{3}, {T}, Sacrifice a land: Destroy target nonbasic land." Its
/// "{T}: Add {C}." is the ordinary mana ability.
const DUST_BOWL: [ActivatedAbilityRecipe; 1] = [instant(
    &[
        ManaCost("{3}"),
        Tap,
        AbilityCostRecipe::SacrificeControlled {
            count: 1,
            filter: PermanentFilterRecipe::Land,
        },
    ],
    AbilityEffectRecipe::DestroyTarget,
    "StandardV1(crate::standard_cards_v1::StandardTargetV1::NonbasicLandV1)",
)];

/// Legacy's Allure: "Sacrifice this enchantment: Gain control of target
/// creature with power less than or equal to the number of treasure counters
/// on this enchantment."
const LEGACYS_ALLURE: [ActivatedAbilityRecipe; 1] = [instant(
    &[SacrificeSelf],
    program("crate::eternal_premodern_p1b_v1::legacys_allure"),
    "StandardV1(crate::standard_cards_v1::StandardTargetV1::CreaturePowerAtMostTreasureCountersV1)",
)];

/// Mishra's Factory: "{1}: This land becomes a 2/2 Assembly-Worker artifact
/// creature until end of turn. It's still a land." and "{T}: Target
/// Assembly-Worker creature gets +1/+1 until end of turn." Its "{T}: Add
/// {C}." is the ordinary mana ability; the animation is
/// `standard_lands::fields`.
const MISHRAS_FACTORY: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[ManaCost("{1}")],
        AbilityEffectRecipe::AnimateSource,
        "None",
    ),
    instant(
        &[Tap],
        AbilityEffectRecipe::PumpTargetUntilEndOfTurn {
            power: 1,
            toughness: 1,
        },
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::AssemblyWorkerCreatureV1)",
    ),
];

/// Parallax Wave: "Remove a fade counter from this enchantment: Exile target
/// creature." The fade counter is removed as the ability is put on the stack
/// (`eternal_premodern_p1b_v1::statics::activation_paid`), and the ability
/// exists only while one remains (`activation_allowed`).
const PARALLAX_WAVE: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{0}")],
    program("crate::eternal_premodern_p1b_v1::parallax_wave_exile"),
    "Creature",
)];

/// Phyrexian Furnace: "{T}: Exile the bottom card of target player's
/// graveyard." and "{1}, Sacrifice this artifact: Exile target card from a
/// graveyard. Draw a card."
const PHYREXIAN_FURNACE: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[Tap],
        program("crate::eternal_premodern_p1b_v1::furnace_exile_bottom"),
        "AnyPlayer",
    ),
    instant(
        &[ManaCost("{1}"), SacrificeSelf],
        program("crate::eternal_premodern_p1b_v1::furnace_exile_and_draw"),
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::CardInAGraveyard)",
    ),
];

/// Psychatog: "Discard a card: This creature gets +1/+1 until end of turn."
/// and "Exile two cards from your graveyard: This creature gets +1/+1 until
/// end of turn."
const PSYCHATOG: [ActivatedAbilityRecipe; 2] = [
    instant(
        &[AbilityCostRecipe::DiscardCards(1)],
        AbilityEffectRecipe::PumpSourceUntilEndOfTurn {
            power: 1,
            toughness: 1,
        },
        "None",
    ),
    instant(
        &[AbilityCostRecipe::ExileGraveyardCards(2)],
        AbilityEffectRecipe::PumpSourceUntilEndOfTurn {
            power: 1,
            toughness: 1,
        },
        "None",
    ),
];

/// Sterling Grove: "{1}, Sacrifice this enchantment: Search your library for
/// an enchantment card and reveal that card. Shuffle, then put the card on
/// top." Its shroud grant is `eternal_premodern_p1b_v1::statics`.
const STERLING_GROVE: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}"), SacrificeSelf],
    program("crate::eternal_premodern_p1b_v1::sterling_grove_search"),
    "None",
)];

/// Treetop Village: "{1}{G}: This land becomes a 3/3 green Ape creature with
/// trample until end of turn. It's still a land." The animation is
/// `standard_lands::fields`.
const TREETOP_VILLAGE: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{1}{G}")],
    AbilityEffectRecipe::AnimateSource,
    "None",
)];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Attunement" => &ATTUNEMENT,
        "Aura of Silence" | "Seal of Cleansing" => &SACRIFICE_DESTROY_ARTIFACT_OR_ENCHANTMENT,
        "Decree of Justice" => &DECREE_OF_JUSTICE,
        "Decree of Silence" => &DECREE_OF_SILENCE,
        "Dust Bowl" => &DUST_BOWL,
        "Legacy's Allure" => &LEGACYS_ALLURE,
        "Mishra's Factory" => &MISHRAS_FACTORY,
        "Parallax Wave" => &PARALLAX_WAVE,
        "Phyrexian Furnace" => &PHYREXIAN_FURNACE,
        "Psychatog" => &PSYCHATOG,
        "Sterling Grove" => &STERLING_GROVE,
        "Treetop Village" => &TREETOP_VILLAGE,
        _ => &[],
    }
}

/// A spell whose program is a runtime function of `eternal_premodern_p1b_v1`.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Accumulated Knowledge" => spell(
            "TargetSpec::None",
            "crate::eternal_premodern_p1b_v1::accumulated_knowledge",
        ),
        // Its X is the doubled {X}{X} of its mana cost.
        "Decree of Justice" => spell(
            "TargetSpec::None",
            "crate::eternal_premodern_p1b_v1::decree_of_justice",
        ),
        "Fact or Fiction" => spell(
            "TargetSpec::None",
            "crate::eternal_premodern_p1b_v1::fact_or_fiction",
        ),
        "Frantic Search" => spell(
            "TargetSpec::None",
            "crate::eternal_premodern_p1b_v1::frantic_search",
        ),
        "Intuition" => spell(
            "TargetSpec::TargetOpponent",
            "crate::eternal_premodern_p1b_v1::intuition",
        ),
        "Mana Leak" => Special::CounterUnlessPaysGeneric {
            filter: super::StackSpellFilter::Any,
            generic: 3,
        },
        "Replenish" => spell(
            "TargetSpec::None",
            "crate::eternal_premodern_p1b_v1::replenish",
        ),
        "Rewind" => spell(
            "TargetSpec::AnySpellOnStack",
            "crate::eternal_premodern_p1b_v1::rewind",
        ),
        // Its "discard X cards" is announced like X
        // (`eternal_premodern_p1b_v1::statics::announces_discard_x`).
        "Sickening Dreams" => spell(
            "TargetSpec::None",
            "crate::eternal_premodern_p1b_v1::sickening_dreams",
        ),
        "Winter's Grasp" => Special::DestroyLand,
        "Wrath of God" => spell(
            "TargetSpec::None",
            "crate::eternal_premodern_p1b_v1::wrath_of_god",
        ),
        _ => return None,
    })
}

/// Printed evergreen keywords, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Argothian Enchantress" => &["Keywords::SHROUD"],
        "Angel Token" => &["Keywords::FLYING"],
        _ => &[],
    }
}

/// Sickening Dreams: "As an additional cost to cast this spell, discard X
/// cards." The count is the announced X
/// (`eternal_premodern_p1b_v1::statics::additional_discard_count`).
pub(super) fn additional_cost_for(name: &str) -> &'static str {
    match name {
        "Sickening Dreams" => "Some(&[CostComponent::DiscardCards(0)])",
        _ => "None",
    }
}

/// Serra's Sanctum: "{T}: Add {W} for each enchantment you control."
pub(super) fn conditional_tap_yield_for(name: &str) -> &'static str {
    match name {
        "Serra's Sanctum" => {
            "Some(DynamicValueDef::ControlledPermanentsWithType(CardType::Enchantment))"
        }
        _ => "None",
    }
}

pub(super) fn conditional_tap_yield_program_for(name: &str) -> Option<&'static str> {
    match name {
        "Serra's Sanctum" => Some("mana_ability_add_serras_sanctum"),
        _ => None,
    }
}

/// Skycloud Expanse: "{1}, {T}: Add {W}{U}." A paid additional mana ability
/// adding a fixed bundle; it has no free mana ability.
pub(super) fn additional_mana_abilities_for(name: &str) -> &'static str {
    match name {
        "Skycloud Expanse" => "&[AdditionalManaAbilityDef { colors: &[ManaColor::W], mana_cost: Cost { pips: &[], generic: 1, x_count: 0 }, ability: ManaAbilityDef { cost: ManaAbilityCostDef::TapSelf, amount: ManaAbilityAmountDef::FixedColorsV1(&[ManaColor::W, ManaColor::U]), controller_damage: 0, max_activations_per_turn: None } }]",
        _ => "&[]",
    }
}

/// Skycloud Expanse has no free mana ability: its only one is the paid
/// filter ability, never an automatic payment source.
pub(super) fn primary_mana_ability_colors(name: &str) -> Option<Vec<&'static str>> {
    match name {
        "Skycloud Expanse" => Some(vec![]),
        _ => None,
    }
}

/// Lands whose only mana ability is an additional one: Skycloud Expanse's
/// paid filter ability.
pub(super) fn static_only_land(name: &str) -> bool {
    name == "Skycloud Expanse"
}

/// Card-database tokens for the triggered abilities in
/// `eternal_premodern_p1b_v1::triggers_for`, its hooks and its statics.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Accumulated Knowledge" => "spell:draw_1_then_draw_per_card_named_this_in_all_graveyards",
        "Argothian Enchantress" => "shroud;controller_casts_enchantment_spell:draw_1",
        "Attunement" => "activated:return_self_to_hand_cost:draw_3_then_discard_4",
        "Aura of Silence" => {
            "static:opponent_artifact_and_enchantment_spells_cost_2_more;activated:sacrifice_self:destroy_target_artifact_or_enchantment"
        }
        "City of Brass" => "becomes_tapped:damage_controller_1;mana:any_color",
        "Decree of Justice" => {
            "spell:x_angel_4_4_flying_tokens;cycling:2w;cycled:may_pay_x:x_soldier_1_1_tokens"
        }
        "Decree of Silence" => {
            "opponent_casts_spell:counter_it:depletion_counter:sacrifice_at_3;cycling:4uu;cycled:may_counter_target_spell"
        }
        "Dust Bowl" => "activated:3_tap_sacrifice_land:destroy_target_nonbasic_land",
        "Enchantress's Presence" => "controller_casts_enchantment_spell:draw_1",
        "Fact or Fiction" => {
            "spell:reveal_top_5:opponent_separates_two_piles:controller_chooses_pile_to_hand_rest_graveyard"
        }
        "Frantic Search" => "spell:draw_2_discard_2:untap_up_to_3_lands",
        "Intuition" => {
            "spell:search_3_reveal:target_opponent_chooses_one_to_hand_rest_graveyard:shuffle"
        }
        "Legacy's Allure" => {
            "controller_upkeep:may_treasure_counter;activated:sacrifice_self:gain_control_creature_power_at_most_treasure_counters"
        }
        "Mirri's Guile" => "controller_upkeep:may_look_top_3_reorder",
        "Mishra's Factory" => "activated:animate_2_2_assembly_worker_artifact;activated:tap:pump_assembly_worker",
        "Opalescence" => "static:other_non_aura_enchantments_are_creatures_base_pt_mana_value",
        "Parallax Wave" => {
            "fading_5;controller_upkeep:remove_fade_counter_or_sacrifice;activated:remove_fade_counter:exile_target_creature;leaves:owners_return_exiled_cards"
        }
        "Phyrexian Furnace" => {
            "activated:tap:exile_bottom_card_target_player_graveyard;activated:1_sacrifice_self:exile_target_graveyard_card:draw_1"
        }
        "Quirion Dryad" => "controller_casts_white_blue_black_or_red_spell:plus_one_counter",
        "Replenish" => "spell:return_all_own_graveyard_enchantments:auras_choose_hosts",
        "Rewind" => "spell:counter_target_spell:untap_up_to_4_lands",
        "Sickening Dreams" => "additional_cost:discard_x;spell:x_damage_each_creature_and_player",
        "Skycloud Expanse" => "mana:1_tap:wu",
        "Solitary Confinement" => {
            "controller_upkeep:sacrifice_unless_discard;static:skip_draw_step;static:controller_shroud;static:prevent_damage_to_controller"
        }
        "Standstill" => "any_player_casts_spell:sacrifice_self:if_sacrificed_opponents_of_caster_draw_3",
        "Sterling Grove" => {
            "static:other_controlled_enchantments_shroud;activated:1_sacrifice_self:search_enchantment_reveal_shuffle_top"
        }
        "Treetop Village" => "enters_tapped;activated:animate_3_3_green_ape_trample",
        "Tsabo's Web" => "etb:draw_1;static:lands_with_nonmana_abilities_dont_untap",
        "Wrath of God" => "spell:destroy_all_creatures:cant_be_regenerated",
        _ => "none",
    }
}
