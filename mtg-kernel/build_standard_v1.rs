//! Card-database recipes for the MageZero Standard non-creature permanent,
//! planeswalker and transforming-legend families (inventory families E and
//! F). Runtime programs live in `src/standard_cards_v1.rs`; this module only
//! names them, so they are part of the generated definitions and catalog
//! identity.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{Loyalty, ManaCost, Tap};
use AbilityEffectRecipe::{DrawCards, Program};

/// A planeswalker loyalty ability: sorcery-speed, from the battlefield.
const fn loyalty(
    cost: &'static [AbilityCostRecipe],
    effect: AbilityEffectRecipe,
    target_spec: &'static str,
) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect,
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec,
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

const TEFERI: [ActivatedAbilityRecipe; 3] = [
    loyalty(&[Loyalty(0)], DrawCards(1), "None"),
    loyalty(
        &[Loyalty(-2)],
        Program("crate::standard_cards_v1::teferi_create_spirit"),
        "None",
    ),
    loyalty(
        &[Loyalty(-12)],
        Program("crate::standard_cards_v1::teferi_ultimate"),
        "TargetOpponent",
    ),
];

/// "Transform this. Activate only as a sorcery."
const fn sorcery_transform(cost: &'static [AbilityCostRecipe]) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect: Program("crate::standard_cards_v1::transform_source"),
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

const POLUKRANOS: [ActivatedAbilityRecipe; 1] = [sorcery_transform(&[ManaCost("{6}{W/P}")])];

/// Temple of Power's "{2}{R}, {T}: Transform"; its red-damage condition is
/// `standard_cards_v1::activation_allowed`.
const OJER: [ActivatedAbilityRecipe; 1] = [sorcery_transform(&[ManaCost("{2}{R}"), Tap])];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Teferi, Temporal Pilgrim" => &TEFERI,
        "Polukranos Reborn" => &POLUKRANOS,
        "Ojer Axonil, Deepest Might" => &OJER,
        _ => &[],
    }
}

/// The transforming-card face an activated ability is printed on (`None`
/// for a single-faced card).
pub(super) fn activated_ability_face_for(name: &str, _index: usize) -> Option<u8> {
    match name {
        "Polukranos Reborn" => Some(0),
        "Ojer Axonil, Deepest Might" => Some(1),
        _ => None,
    }
}

/// Back faces of this module's transforming cards. Each face keeps the
/// card's supertypes.
pub(super) fn transform_face_for(name: &str) -> &'static str {
    match name {
        "Cecil, Dark Knight" => "Some(TransformFaceDef { name: \"Cecil, Redeemed Paladin\", types: &[CardType::Creature], subtypes: &[Subtype::Human, Subtype::Knight], colors: &[ManaColor::W], power: Some(4), toughness: Some(4), keywords: Keywords::LIFELINK })",
        "Polukranos Reborn" => "Some(TransformFaceDef { name: \"Polukranos, Engine of Ruin\", types: &[CardType::Creature], subtypes: &[Subtype::Phyrexian, Subtype::Hydra], colors: &[ManaColor::W, ManaColor::G], power: Some(6), toughness: Some(6), keywords: Keywords(Keywords::REACH.0 | Keywords::LIFELINK.0) })",
        "Ojer Axonil, Deepest Might" => "Some(TransformFaceDef { name: \"Temple of Power\", types: &[CardType::Land], subtypes: &[], colors: &[], power: None, toughness: None, keywords: Keywords::NONE })",
        _ => "None",
    }
}

pub(super) fn transform_face_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Cecil, Dark Knight" => Some("Cecil, Redeemed Paladin"),
        "Polukranos Reborn" => Some("Polukranos, Engine of Ruin"),
        "Ojer Axonil, Deepest Might" => Some("Temple of Power"),
        _ => None,
    }
}

/// Card-database tokens for the triggered abilities in
/// `standard_cards_v1::triggers_for`.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Teferi, Temporal Pilgrim" => "controller_draws:add_loyalty_to_bound_source:1",
        "Teferi Spirit Token" => "controller_draws:plus_one_counter_on_bound_source:1",
        "Cecil, Dark Knight" => {
            "deals_damage:lose_that_much_life_then_untap_transform_at_half_life;back:attacks:other_attackers_gain_indestructible"
        }
        "Polukranos Reborn" => "back:this_or_another_nontoken_hydra_you_control_dies:two_phyrexian_hydras",
        "Ojer Axonil, Deepest Might" => "dies:return_tapped_transformed",
        _ => "none",
    }
}

/// Printed keywords (front face), in printed order.
pub(super) fn keywords_for(name: &str) -> &'static [&'static str] {
    match name {
        "Teferi Spirit Token" => &["Keywords::VIGILANCE"],
        "Cecil, Dark Knight" => &["Keywords::DEATHTOUCH"],
        "Polukranos Reborn" | "Phyrexian Hydra Reach Token" => &["Keywords::REACH"],
        "Ojer Axonil, Deepest Might" => &["Keywords::TRAMPLE"],
        "Phyrexian Hydra Lifelink Token" => &["Keywords::LIFELINK"],
        _ => &[],
    }
}

/// The in-game name of a token definition whose registry name carries a
/// disambiguating suffix.
pub(super) fn object_name_for(name: &str) -> Option<&'static str> {
    match name {
        "Teferi Spirit Token" => Some("Spirit"),
        "Phyrexian Hydra Reach Token" | "Phyrexian Hydra Lifelink Token" => {
            Some("Phyrexian Hydra")
        }
        _ => None,
    }
}

/// Spells whose program is one named function in `standard_cards_v1`.
pub(super) fn special_for(name: &str) -> Option<Special> {
    let (target_spec, program) = match name {
        "Blue Sun's Twilight" => (
            "TargetSpec::Creature",
            "crate::standard_cards_v1::blue_suns_twilight",
        ),
        _ => return None,
    };
    Some(Special::StandardProgram {
        target_spec,
        program,
    })
}
