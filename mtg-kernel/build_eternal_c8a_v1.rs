//! Card-database recipes for the eternal catalog's Vintage Cube wave C8a
//! (Adventure, transform DFCs, modal DFC, and prepare cards). Wave C7c's
//! tables fall through to these, and these fall through to wave C8b's
//! (`build_eternal_c8b_v1.rs`). Runtime programs live in
//! `src/eternal_cube_c8a_v1.rs`. Every row is keyed by a name that occurs
//! only in the eternal registry.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{Loyalty, ManaCost, Tap};

const fn program(path: &'static str) -> AbilityEffectRecipe {
    AbilityEffectRecipe::StandardProgram(path)
}

const fn sorcery_transform(cost: &'static [AbilityCostRecipe]) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect: program("crate::eternal_cube_c8a_v1::transform_front_to_back"),
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

const fn loyalty(
    cost: &'static [AbilityCostRecipe],
    path: &'static str,
    target_spec: &'static str,
) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect: program(path),
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec,
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

/// Concealing Curtains: "{2}{B}: Transform this creature. Activate only as a
/// sorcery." Printed on face 0 only.
const CONCEALING_CURTAINS: [ActivatedAbilityRecipe; 1] = [sorcery_transform(&[ManaCost("{2}{B}")])];

/// Jace, Vryn's Prodigy // Jace, Telepath Unbound:
/// - index 0 (face 0): "{T}: Draw a card, then discard a card. If there are
///   five or more cards in your graveyard, exile Jace, then return him to the
///   battlefield transformed under his owner's control."
/// - index 1 (face 1, +1): "Up to one target creature gets -2/-0 until your
///   next turn."
/// - index 2 (face 1, -3): "You may cast target instant or sorcery card from
///   your graveyard this turn. If that spell would be put into your graveyard,
///   exile it instead."
/// - index 3 (face 1, -9): "You get an emblem with 'Whenever you cast a spell,
///   target opponent mills five cards.'"
const JACE_VRYNS_PRODIGY: [ActivatedAbilityRecipe; 4] = [
    ActivatedAbilityRecipe {
        cost: &[Tap],
        effect: program("crate::eternal_cube_c8a_v1::jace_loot_transform"),
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    },
    loyalty(
        &[Loyalty(1)],
        "crate::eternal_cube_c8a_v1::jace_plus_one",
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::UpToOneCreature)",
    ),
    loyalty(
        &[Loyalty(-3)],
        "crate::eternal_cube_c8a_v1::jace_minus_three",
        "StandardV1(crate::standard_cards_v1::StandardTargetV1::InstantOrSorceryCardInOwnGraveyard)",
    ),
    loyalty(
        &[Loyalty(-9)],
        "crate::eternal_cube_c8a_v1::jace_minus_nine",
        "None",
    ),
];

/// Jennifer Walters: "{3}{G}{W}{W}: Transform Jennifer Walters. Activate only
/// as a sorcery." Printed on face 0 only.
const JENNIFER_WALTERS: [ActivatedAbilityRecipe; 1] =
    [sorcery_transform(&[ManaCost("{3}{G}{W}{W}")])];

/// Activated abilities of wave C8a cards.
pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Concealing Curtains" => &CONCEALING_CURTAINS,
        "Jace, Vryn's Prodigy" => &JACE_VRYNS_PRODIGY,
        "Jennifer Walters" => &JENNIFER_WALTERS,
        _ => super::build_eternal_c8b_v1::activated_ability_recipes_for(name),
    }
}

/// Face restriction for wave C8a activated abilities.
pub(super) fn activated_ability_face_for(name: &str, index: usize) -> Option<u8> {
    match name {
        "Concealing Curtains" | "Jennifer Walters" => Some(0),
        "Jace, Vryn's Prodigy" => Some(if index == 0 { 0 } else { 1 }),
        _ => super::build_eternal_c9a_v1::activated_ability_face_for(name, index),
    }
}

/// Spell programs: Regrowth (Emeritus of Abundance's prepare spell).
pub(super) fn special_for(name: &str) -> Option<Special> {
    match name {
        "Regrowth" => Some(Special::StandardProgram {
            target_spec:
                "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::CardInOwnGraveyardV1)",
            program: "crate::eternal_cube_c8a_v1::regrowth",
        }),
        _ => super::build_eternal_c8b_v1::special_for(name),
    }
}

/// No C8a card has kicker.
pub(super) fn kicker_cost_for(name: &str) -> Option<&'static str> {
    super::build_eternal_c8b_v1::kicker_cost_for(name)
}

/// No C8a card is an Aura or an Equipment.
pub(super) fn attachment_for(name: &str) -> &'static str {
    super::build_eternal_c8b_v1::attachment_for(name)
}

/// No C8a card has flashback.
pub(super) fn flashback_for(name: &str) -> String {
    super::build_eternal_c8b_v1::flashback_for(name)
}

/// Modal DFC back-face cast: Jennifer Walters may be cast as The Sensational
/// She-Hulk for {3}{G}{W}{W}.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Jennifer Walters" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::W), Pip::Colored(ManaColor::W)], generic: 3, x_count: 0 })], condition: AltCostCondition::Always })",
        _ => super::build_eternal_c8b_v1::alt_cost_for(name),
    }
}

/// Bonecrusher Giant's Instant Adventure: Stomp {1}{R}.
pub(super) fn adventure_for(name: &str) -> &'static str {
    match name {
        "Bonecrusher Giant" => "Some(AdventureDef { name: \"Stomp\", cost: Cost { pips: &[Pip::Colored(ManaColor::R)], generic: 1, x_count: 0 }, types: &[CardType::Instant], target_spec: TargetSpec::AnyTarget, effect: ability_effect_program_eternal_cube_c8a_v1_stomp })",
        _ => "None",
    }
}

pub(super) fn adventure_effect_recipe_for(name: &str) -> Option<AbilityEffectRecipe> {
    match name {
        "Bonecrusher Giant" => Some(program("crate::eternal_cube_c8a_v1::stomp")),
        _ => None,
    }
}

pub(super) fn adventure_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Bonecrusher Giant" => Some("Stomp"),
        _ => None,
    }
}

/// Back faces of wave C8a double-faced cards.
pub(super) fn transform_face_for(name: &str) -> &'static str {
    match name {
        "Concealing Curtains" => "Some(TransformFaceDef { name: \"Revealing Eye\", types: &[CardType::Creature], subtypes: &[Subtype::Eye, Subtype::Horror], colors: &[ManaColor::B], power: Some(3), toughness: Some(4), keywords: Keywords::MENACE })",
        "Jace, Vryn's Prodigy" => "Some(TransformFaceDef { name: \"Jace, Telepath Unbound\", types: &[CardType::Planeswalker], subtypes: &[Subtype::Jace], colors: &[ManaColor::U], power: None, toughness: None, keywords: Keywords::NONE })",
        "Jennifer Walters" => "Some(TransformFaceDef { name: \"The Sensational She-Hulk\", types: &[CardType::Creature], subtypes: &[Subtype::Gamma, Subtype::Hero], colors: &[ManaColor::G, ManaColor::W], power: Some(6), toughness: Some(6), keywords: Keywords(Keywords::REACH.0 | Keywords::TRAMPLE.0) })",
        _ => super::build_eternal_c9a_v1::transform_face_for(name),
    }
}

pub(super) fn transform_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Concealing Curtains" => Some("Revealing Eye"),
        "Jace, Vryn's Prodigy" => Some("Jace, Telepath Unbound"),
        "Jennifer Walters" => Some("The Sensational She-Hulk"),
        _ => super::build_eternal_c9a_v1::transform_face_name_for(name),
    }
}

/// Emeritus of Ideation: "Ward {2}".
pub(super) fn ward_cost_for(name: &str) -> &'static str {
    match name {
        "Emeritus of Ideation" => "Some(WardCostDef::Generic(2))",
        _ => "None",
    }
}

/// Printed evergreen keywords on front faces, in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Concealing Curtains" => &["Keywords::DEFENDER"],
        "Emeritus of Abundance" => &["Keywords::VIGILANCE"],
        "Emeritus of Ideation" => &["Keywords::FLYING"],
        _ => super::build_eternal_c8b_v1::keywords_for(name),
    }
}

/// Card-database tokens for this wave's abilities.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Bonecrusher Giant" => "becomes_target_of_spell:damage_spells_controller:2;adventure:stomp:1r:instant:damage_cant_be_prevented_this_turn:damage_any_target:2",
        "Concealing Curtains" => "defender;activated_sorcery:2b:transform;back_face:revealing_eye:3_4:eye_horror:menace:transforms_into:target_opponent_reveals_hand_may_choose_nonland_discard_then_draw_1",
        "Jace, Vryn's Prodigy" => "activated_tap:draw_1_then_discard_1_if_five_or_more_in_graveyard_exile_return_transformed_under_owner;back_face:jace_telepath_unbound:loyalty_start:5:plus1_up_to_one_creature_minus2_0_until_your_next_turn:minus3_cast_target_instant_or_sorcery_from_graveyard_this_turn_exile_instead:minus9_emblem_cast_spell_target_opponent_mills_5",
        "Jennifer Walters" => "static:opponents_cant_cast_spells_during_your_turn;activated_sorcery:3gww:transform;modal_dfc_back_face:the_sensational_she_hulk:3gww:6_6:gw:gamma_hero:reach:trample:static_opponents_cant_cast_spells_during_your_turn:controlled_creature_dealt_damage_may_deal_that_much_damage_any_target_once_each_turn",
        "Emeritus of Abundance" => "vigilance;enters_prepared:regrowth;attacks_if_eight_or_more_lands:become_prepared",
        "Emeritus of Ideation" => "flying;ward:2;enters_prepared:ancestral_recall;attacks:may_exile_eight_cards_from_your_graveyard_if_you_do_become_prepared",
        "Regrowth" => "spell:return_target_card_from_your_graveyard_to_your_hand;prepare_spell_copy_only",
        _ => super::build_eternal_c8b_v1::trigger_recipe_for(name),
    }
}
