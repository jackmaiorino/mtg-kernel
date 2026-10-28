//! Card-database recipes for the eternal catalog's Pauper and Vintage long
//! tail (wave E9b): Turbo Fog, Gruul Ponza and Ramp, Food Gardens, 4c
//! Pestilence, Esper Faeries and 4c Ephemerate, and Vintage Scam, Eldrazi,
//! Red Prison, Grixis Lurrus, Lurrus Vault Key and Underworld Breach. E8b's
//! tables (and E5b's modes, E6a's kicker costs and E6b's Gates and land
//! faces) fall through to these; runtime programs live in
//! `src/eternal_e9b_v1.rs`. Every row is keyed by a name that occurs only in
//! the eternal registry.

use super::{
    AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, PermanentFilterRecipe, Special,
};
use AbilityCostRecipe::{DiscardSelf, ExileSelf, ManaCost, SacrificeSelf, Tap};

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

/// A program of `eternal_e9b_v1`.
const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

/// Arbor Elf: "{T}: Untap target Forest."
const ARBOR_ELF: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap],
    AbilityEffectRecipe::UntapTarget,
    "StandardV1(crate::standard_cards_v1::StandardTargetV1::ForestV1)",
)];

/// Bannerhide Krushok: "Reinforce 2--{1}{G}" ({1}{G}, discard this card: put
/// two +1/+1 counters on target creature) and "Scavenge {5}{G}{G}" ({5}{G}{G},
/// exile this card from your graveyard: put a number of +1/+1 counters equal
/// to this card's power, 4, on target creature; activate only as a sorcery).
const BANNERHIDE_KRUSHOK: [ActivatedAbilityRecipe; 2] = [
    ActivatedAbilityRecipe {
        cost: &[ManaCost("{1}{G}"), DiscardSelf],
        effect: AbilityEffectRecipe::AddPlusOnePlusOneCounters(2),
        activation_zone: "Hand",
        sorcery_speed_only: false,
        target_spec: "Creature",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
    ActivatedAbilityRecipe {
        cost: &[ManaCost("{5}{G}{G}"), ExileSelf],
        effect: AbilityEffectRecipe::AddPlusOnePlusOneCounters(4),
        activation_zone: "Graveyard",
        sorcery_speed_only: true,
        target_spec: "Creature",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
];

/// Cauldron Familiar: "Sacrifice a Food: Return this card from your
/// graveyard to the battlefield."
const CAULDRON_FAMILIAR: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[AbilityCostRecipe::SacrificeControlled {
        count: 1,
        filter: PermanentFilterRecipe::AnotherFood,
    }],
    effect: AbilityEffectRecipe::ReturnSourceFromGraveyard { tapped: false },
    activation_zone: "Graveyard",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Dauthi Voidwalker: "{T}, Sacrifice this creature: Choose an exiled card
/// an opponent owns with a void counter on it. You may play it this turn
/// without paying its mana cost."
const DAUTHI_VOIDWALKER: [ActivatedAbilityRecipe; 1] = [instant(
    &[Tap, SacrificeSelf],
    program("crate::eternal_e9b_v1::dauthi_voidwalker"),
    "None",
)];

/// Elvish Spirit Guide: "Exile this card from your hand: Add {G}." A mana
/// ability: the engine performs it without the stack
/// (`eternal_e9b_v1::special_activation`), offered as ability 0.
const ELVISH_SPIRIT_GUIDE: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ExileSelf],
    effect: program("crate::eternal_e9b_v1::spirit_guide_mana"),
    activation_zone: "Hand",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Momentum Breaker: "{2}, Sacrifice this enchantment: You gain life equal
/// to your speed."
const MOMENTUM_BREAKER: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{2}"), SacrificeSelf],
    program("crate::eternal_e9b_v1::momentum_breaker_life"),
    "None",
)];

/// Nutrient Block: "{2}, {T}, Sacrifice this artifact: You gain 3 life."
const NUTRIENT_BLOCK: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{2}"), Tap, SacrificeSelf],
    AbilityEffectRecipe::GainLife(3),
    "None",
)];

/// Oswald Fiddlebender: "Magical Tinkering--{W}, {T}, Sacrifice an
/// artifact: Search your library for an artifact card with mana value equal
/// to 1 plus the sacrificed artifact's mana value, put it onto the
/// battlefield, then shuffle. Activate only as a sorcery."
const OSWALD_FIDDLEBENDER: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[
        ManaCost("{W}"),
        Tap,
        AbilityCostRecipe::SacrificeControlled {
            count: 1,
            filter: PermanentFilterRecipe::Artifact,
        },
    ],
    effect: program("crate::eternal_e9b_v1::oswald_fiddlebender"),
    activation_zone: "Battlefield",
    sorcery_speed_only: true,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

/// Pestilence: "{B}: This enchantment deals 1 damage to each creature and
/// each player." Crypt Rats' X = 1 program.
const PESTILENCE: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{B}")],
    program("crate::eternal_pauper_e8b_v1::crypt_rats_1"),
    "None",
)];

/// Step Through: "Wizardcycling {2}" ({2}, discard this card: search your
/// library for a Wizard card, reveal it, put it into your hand, then
/// shuffle).
const STEP_THROUGH: [ActivatedAbilityRecipe; 1] = [ActivatedAbilityRecipe {
    cost: &[ManaCost("{2}"), DiscardSelf],
    effect: program("crate::eternal_e9b_v1::wizardcycling"),
    activation_zone: "Hand",
    sorcery_speed_only: false,
    target_spec: "None",
    activation_target_filter: "TargetSpecOnly",
    max_activations_per_turn: None,
}];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Arbor Elf" => &ARBOR_ELF,
        "Bannerhide Krushok" => &BANNERHIDE_KRUSHOK,
        "Cauldron Familiar" => &CAULDRON_FAMILIAR,
        "Dauthi Voidwalker" => &DAUTHI_VOIDWALKER,
        "Elvish Spirit Guide" => &ELVISH_SPIRIT_GUIDE,
        "Momentum Breaker" => &MOMENTUM_BREAKER,
        "Nutrient Block" => &NUTRIENT_BLOCK,
        "Oswald Fiddlebender" => &OSWALD_FIDDLEBENDER,
        "Pestilence" => &PESTILENCE,
        "Step Through" => &STEP_THROUGH,
        _ => &[],
    }
}

/// A spell whose program is a runtime function.
const fn spell(target_spec: &'static str, program: &'static str) -> Special {
    Special::StandardProgram {
        target_spec,
        program,
    }
}

/// Spell programs. Modal spells' other modes are `mode2_for` and
/// `mode3_for` below.
pub(super) fn special_for(name: &str) -> Option<Special> {
    Some(match name {
        "Arcane Denial" => spell(
            "TargetSpec::AnySpellOnStack",
            "crate::eternal_e9b_v1::arcane_denial",
        ),
        "Boomerang Basics" => spell(
            "TargetSpec::NonlandPermanent",
            "crate::eternal_e9b_v1::boomerang_basics",
        ),
        "Bridgeworks Battle" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::ControlledCreatureThenUpToOneOpponentCreatureV1)",
            "crate::eternal_e9b_v1::bridgeworks_battle",
        ),
        "Castigate" => spell("TargetSpec::TargetOpponent", "crate::eternal_e9b_v1::castigate"),
        "Echoing Truth" => spell(
            "TargetSpec::NonlandPermanent",
            "crate::eternal_e9b_v1::echoing_truth",
        ),
        // Moment's Peace's program, without its flashback.
        "Fog" => spell(
            "TargetSpec::None",
            "crate::eternal_pauper_e6a_v1::moments_peace",
        ),
        // "Target artifact or creature": the spell is never a permanent, so
        // "another" excludes nothing.
        "Grapple with Death" => spell(
            "TargetSpec::AnotherArtifactOrCreature",
            "crate::eternal_e9b_v1::grapple_with_death",
        ),
        "Growth Spiral" => spell("TargetSpec::None", "crate::eternal_e9b_v1::growth_spiral"),
        "Heritage Reclamation" => spell(
            "TargetSpec::ArtifactPermanent",
            "crate::eternal_e9b_v1::destroy_target",
        ),
        "Mwonvuli Acid-Moss" => spell(
            "TargetSpec::Land",
            "crate::eternal_e9b_v1::mwonvuli_acid_moss",
        ),
        "Mystical Tutor" => spell("TargetSpec::None", "crate::eternal_e9b_v1::mystical_tutor"),
        "Pieces of the Puzzle" => spell(
            "TargetSpec::None",
            "crate::eternal_e9b_v1::pieces_of_the_puzzle",
        ),
        "Ram Through" => spell(
            "TargetSpec::ControlledCreatureThenOpponentCreature",
            "crate::eternal_e9b_v1::ram_through",
        ),
        "Respite" => spell("TargetSpec::None", "crate::eternal_e9b_v1::respite"),
        "Step Through" => spell(
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::TwoDistinctCreaturesV1)",
            "crate::eternal_e9b_v1::step_through",
        ),
        "Thermokarst" => spell("TargetSpec::Land", "crate::eternal_e9b_v1::thermokarst"),
        "Vraska's Final Mercy" => spell(
            "TargetSpec::CreatureOrPlaneswalker",
            "crate::eternal_e9b_v1::vraskas_final_mercy_destroy",
        ),
        // "Enchant land".
        "Wild Growth" => spell("TargetSpec::Land", "crate::standard_cards_v1::aura_enters"),
        "You Meet in a Tavern" => spell(
            "TargetSpec::None",
            "crate::eternal_e9b_v1::you_meet_in_a_tavern_party",
        ),
        _ => return None,
    })
}

/// Heritage Reclamation's second mode, Vraska's Final Mercy's and You Meet
/// in a Tavern's second mode.
pub(super) fn mode2_for(name: &str) -> &'static str {
    match name {
        "Heritage Reclamation" => "Some(ModeDef { target_spec: TargetSpec::EnchantmentPermanent, effect: crate::eternal_e9b_v1::destroy_target })",
        "Vraska's Final Mercy" => "Some(ModeDef { target_spec: TargetSpec::None, effect: crate::eternal_e9b_v1::vraskas_final_mercy_empower })",
        "You Meet in a Tavern" => "Some(ModeDef { target_spec: TargetSpec::None, effect: crate::eternal_e9b_v1::you_meet_in_a_tavern_brawl })",
        _ => "None",
    }
}

/// Heritage Reclamation's third mode: "Exile up to one target card from a
/// graveyard. Draw a card."
pub(super) fn mode3_for(name: &str) -> &'static str {
    match name {
        "Heritage Reclamation" => "Some(ModeDef { target_spec: TargetSpec::UpToOneCardInGraveyards, effect: crate::eternal_e9b_v1::heritage_reclamation_exile })",
        _ => "None",
    }
}

/// Printed evergreen keywords, in printed order. Nutrient Block's
/// indestructible comes from its registry mechanics; Dauthi Voidwalker's
/// shadow is `eternal_e9b_v1::blocker_allowed`.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Annoyed Altisaur" => &["Keywords::REACH", "Keywords::TRAMPLE"],
        "Bannerhide Krushok" => &["Keywords::TRAMPLE"],
        "Barrowgoyf" => &["Keywords::DEATHTOUCH", "Keywords::LIFELINK"],
        "Boarding Party" => &["Keywords::HASTE"],
        "Jewel Thief" => &["Keywords::VIGILANCE", "Keywords::TRAMPLE"],
        "Mai, Scornful Striker" => &["Keywords::FIRST_STRIKE"],
        "Vampire Sovereign" => &["Keywords::FLYING"],
        _ => &[],
    }
}

/// Sowing Mycospawn: "Kicker {1}{C}".
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    match name {
        "Sowing Mycospawn" => Some("{1}{C}"),
        _ => None,
    }
}

/// The modal double-faced cards' land faces, played from hand.
pub(super) fn transform_face_for(name: &str) -> &'static str {
    match name {
        "Boggart Trawler" => "Some(TransformFaceDef { name: \"Boggart Bog\", types: &[CardType::Land], subtypes: &[], colors: &[], power: None, toughness: None, keywords: Keywords::NONE })",
        "Bridgeworks Battle" => "Some(TransformFaceDef { name: \"Tanglespan Bridgeworks\", types: &[CardType::Land], subtypes: &[], colors: &[], power: None, toughness: None, keywords: Keywords::NONE })",
        _ => "None",
    }
}

pub(super) fn transform_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Boggart Trawler" => Some("Boggart Bog"),
        "Bridgeworks Battle" => Some("Tanglespan Bridgeworks"),
        _ => None,
    }
}

/// Fixed colors of the primary printed mana ability, where they differ from
/// `produces_mana`: Simic Growth Chamber's one action identifies its whole
/// bundle, and Black Dragon Gate's fixed option is {B}.
pub(super) fn primary_mana_ability_colors(name: &str) -> Option<Vec<&'static str>> {
    match name {
        "Simic Growth Chamber" => Some(vec!["G"]),
        "Black Dragon Gate" => Some(vec!["B"]),
        _ => None,
    }
}

/// Gates whose mana ability adds their chosen color.
pub(super) fn mana_ability_includes_chosen_color(name: &str) -> bool {
    name == "Black Dragon Gate"
}

/// "As this land enters, choose a color other than black."
pub(super) fn as_enters_choose_color_other_than(name: &str) -> &'static str {
    match name {
        "Black Dragon Gate" => "Some(ManaColor::B)",
        _ => "None",
    }
}

/// Single printed mana abilities that are not "tap: add one".
pub(super) fn mana_ability_def_for(name: &str) -> &'static str {
    match name {
        // "{T}: Add {G}{U}." One activation adds both colors at once.
        "Simic Growth Chamber" => "Some(ManaAbilityDef { cost: ManaAbilityCostDef::TapSelf, amount: ManaAbilityAmountDef::FixedColorsV1(&[ManaColor::G, ManaColor::U]), controller_damage: 0, max_activations_per_turn: None })",
        _ => "None",
    }
}

/// Card-database tokens for the triggered abilities in
/// `eternal_e9b_v1::triggers_for`, its hooks and its statics.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Annoyed Altisaur" | "Boarding Party" => {
            "cast_self:cascade:exile_until_nonland_less_mana_value:may_cast_free:rest_bottom_random"
        }
        "Arcane Denial" => {
            "spell:counter_target_spell;next_upkeep:its_controller_may_draw_up_to_2;next_upkeep:you_draw_1"
        }
        "Barrowgoyf" => {
            "cda:power_card_types_in_all_graveyards:toughness_plus_1;combat_damage_player:may_mill_that_many:may_creature_to_hand"
        }
        "Black Dragon Gate" => "as_enters_choose_color_other_than_black",
        "Boggart Trawler" => {
            "etb:exile_target_players_graveyard;land_face_play_from_hand:as_enters_may_pay_3_life_else_tapped"
        }
        "Bridgeworks Battle" => {
            "spell:pump_2_2_then_fight_up_to_one;land_face_play_from_hand:as_enters_may_pay_3_life_else_tapped"
        }
        "Castigate" => "spell:target_opponent_reveals_hand:choose_nonland:exile",
        "Cauldron Familiar" => "etb:each_opponent_loses_1:gain_1",
        "Dauthi Voidwalker" => {
            "static:shadow;replacement:opponent_card_to_graveyard_exiled_with_void_counter;ability:play_void_card_free_this_turn"
        }
        "Echoing Truth" => "spell:return_target_and_same_name_to_hand",
        "Eldrazi Repurposer" => "cast_self:create_token:Eldrazi Spawn Token;dies:create_token:Eldrazi Spawn Token",
        "Elvish Spirit Guide" => "hand_mana:exile_self:add_g",
        "Growth Spiral" => "spell:draw_1:may_put_land_from_hand",
        "Jewel Thief" => "etb:create_token:Treasure Token",
        "Mai, Scornful Striker" => "any_player_casts_noncreature_spell:that_player_loses_2",
        "Momentum Breaker" => {
            "start_your_engines;etb:each_opponent_sacrifices_creature_or_vehicle_else_discards;ability:gain_life_equal_to_speed"
        }
        "Nutrient Block" => "put_into_graveyard_from_battlefield:draw_1",
        "Oswald Fiddlebender" => "ability:search_artifact_mana_value_sacrificed_plus_1_to_battlefield",
        "Pestilence" => "beginning_end_step:if_no_creatures:sacrifice_self",
        "Pieces of the Puzzle" => "spell:reveal_top_5:up_to_2_instant_or_sorcery_to_hand:rest_to_graveyard",
        "Radiant Fountain" => "etb:gain_life:2",
        "Respite" => "spell:prevent_all_combat_damage_this_turn:gain_1_per_attacking_creature",
        "Simic Growth Chamber" => "etb:return_chosen_controlled_land_to_owners_hand:untargeted",
        "Sowing Mycospawn" => {
            "cast_self:search_land_to_battlefield;cast_self_if_kicked:exile_target_land"
        }
        "Thermokarst" => "spell:destroy_target_land:gain_1_if_snow",
        "Thief of Existence" => {
            "cast_self:exile_up_to_one_opponent_noncreature_nonland_mana_value_4:gains_leaves_target_opponent_draws"
        }
        "Vampire Sovereign" => "etb:target_opponent_loses_3:gain_3",
        "Wild Growth" => "enchanted_land_tapped_for_mana:add_g",
        "You Meet in a Tavern" => "spell:look_top_5:reveal_creatures_to_hand:rest_bottom_random;mode2:creatures_you_control_plus_2_2",
        _ => "none",
    }
}
