//! Card-database recipes for the MageZero Standard non-creature permanent,
//! planeswalker and transforming-legend families (inventory families E and
//! F). Runtime programs live in `src/standard_cards_v1.rs`; this module only
//! names them, so they are part of the generated definitions and catalog
//! identity.

use super::{AbilityCostRecipe, AbilityEffectRecipe, ActivatedAbilityRecipe, Special};
use AbilityCostRecipe::{Loyalty, ManaCost, PayLife, SacrificeSelf, Tap};
use AbilityEffectRecipe::AttachSourceToTarget;
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

/// A Room's unlock special actions, left door then right door, each at its
/// half's mana cost and sorcery timing (709.5e). Each works only while its
/// door is locked (`standard_cards_v1::activation_allowed`).
const fn unlock(cost: &'static [AbilityCostRecipe], program: &'static str) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect: Program(program),
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

const UNHOLY_ANNEX: [ActivatedAbilityRecipe; 2] = [
    unlock(&[ManaCost("{2}{B}")], "crate::standard_cards_v1::unlock_left_door"),
    unlock(&[ManaCost("{3}{B}{B}")], "crate::standard_cards_v1::unlock_right_door"),
];

/// "Equip {N}": attach to target creature you control, as a sorcery.
const fn equip(cost: &'static [AbilityCostRecipe]) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect: AttachSourceToTarget,
        activation_zone: "Battlefield",
        sorcery_speed_only: true,
        target_spec: "ControlledCreature",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

const BASILISK_COLLAR: [ActivatedAbilityRecipe; 1] = [equip(&[ManaCost("{2}")])];

/// An instant-speed activated ability from the battlefield.
const fn instant(cost: &'static [AbilityCostRecipe], effect: AbilityEffectRecipe) -> ActivatedAbilityRecipe {
    ActivatedAbilityRecipe {
        cost,
        effect,
        activation_zone: "Battlefield",
        sorcery_speed_only: false,
        target_spec: "None",
        activation_target_filter: "TargetSpecOnly",
        max_activations_per_turn: None,
    }
}

const CANDY_TRAIL: [ActivatedAbilityRecipe; 1] = [instant(
    &[ManaCost("{2}"), Tap, SacrificeSelf],
    Program("crate::standard_cards_v1::candy_trail_sacrifice"),
)];

const LUNAR_CONVOCATION: [ActivatedAbilityRecipe; 1] =
    [instant(&[ManaCost("{1}{B}"), PayLife(2)], DrawCards(1))];

pub(super) fn activated_ability_recipes_for(name: &str) -> &'static [ActivatedAbilityRecipe] {
    match name {
        "Candy Trail" => &CANDY_TRAIL,
        "Lunar Convocation" => &LUNAR_CONVOCATION,
        "Basilisk Collar" => &BASILISK_COLLAR,
        "Unholy Annex // Ritual Chamber" => &UNHOLY_ANNEX,
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
/// `standard_cards_v1::triggers_for` and the static abilities in
/// `standard_cards_v1::controlled_boost`.
pub(super) fn trigger_recipe_for(name: &str) -> &'static str {
    match name {
        "Teferi, Temporal Pilgrim" => "controller_draws:add_loyalty_to_bound_source:1",
        "Teferi Spirit Token" => "controller_draws:plus_one_counter_on_bound_source:1",
        "Cecil, Dark Knight" => {
            "deals_damage:lose_that_much_life_then_untap_transform_at_half_life;back:attacks:other_attackers_gain_indestructible"
        }
        "Polukranos Reborn" => "back:this_or_another_nontoken_hydra_you_control_dies:two_phyrexian_hydras",
        "Ojer Axonil, Deepest Might" => "dies:return_tapped_transformed",
        "Seam Rip" | "Dusk Rose Reliquary" | "Sheltered by Ghosts" | "Hardlight Containment" => {
            "etb:exile_target_until_source_leaves;ltb:return_exiled_by_source"
        }
        "Unholy Annex // Ritual Chamber" => {
            "door0:controller_end_step:draw_then_drain_2_if_demon_else_lose_2;door1:unlock_this_door:create_demon_6_6_flying"
        }
        "Candy Trail" => "etb:scry_2",
        "Warleader's Call" => {
            "static:controlled_creatures_plus_1_1;controlled_creature_enters:damage_each_opponent_1"
        }
        "Karn Construct Token" => "static:plus_1_1_per_controlled_artifact",
        "Lunar Convocation" => {
            "controller_end_step_if_gained_life:each_opponent_loses_1;controller_end_step_if_gained_and_lost_life:create_bat_1_1_flying"
        }
        "Simulacrum Synthesizer" => {
            "etb:scry_2;another_controlled_artifact_mv_3_enters:create_karn_construct"
        }
        "Case of the Gateway Express" => {
            "etb:each_controlled_creature_deals_1_to_target_opponent_creature;controller_end_step_solve:three_creatures_attacked;solved_static:controlled_creatures_plus_1_0"
        }
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
        "Demon Flying Token" | "Bat Flying Token" => &["Keywords::FLYING"],
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
        "Demon Flying Token" => Some("Demon"),
        "Bat Flying Token" => Some("Bat"),
        "Karn Construct Token" => Some("Construct"),
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
        "Sheltered by Ghosts" => (
            "TargetSpec::ControlledCreature",
            "crate::standard_cards_v1::aura_enters",
        ),
        "Hardlight Containment" => (
            "TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::ControlledArtifact)",
            "crate::standard_cards_v1::aura_enters",
        ),
        "Unholy Annex // Ritual Chamber" => (
            "TargetSpec::None",
            "crate::standard_cards_v1::room_enters_program",
        ),
        _ => return None,
    };
    Some(Special::StandardProgram {
        target_spec,
        program,
    })
}

/// Alternative costs. A Room's right half is cast for its own mana cost,
/// modeled as the card's alternative cost; the left half is the normal cost.
pub(super) fn alt_cost_for(name: &str) -> &'static str {
    match name {
        "Unholy Annex // Ritual Chamber" => "Some(AltCostDef { components: &[CostComponent::Mana(Cost { pips: &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::B)], generic: 3, x_count: 0 })], condition: AltCostCondition::Always })",
        _ => "None",
    }
}

/// Printed ward. Sheltered by Ghosts and Hardlight Containment grant ward
/// to the permanent they enchant (`standard_cards_v1::granted_wards`).
pub(super) fn ward_cost_for(name: &str) -> &'static str {
    match name {
        "Dusk Rose Reliquary" => "Some(WardCostDef::Generic(2))",
        _ => "None",
    }
}

/// Static grants of an attached permanent: Equipment, and Sheltered by
/// Ghosts' "+1/+0 and lifelink" (its ward is granted separately).
pub(super) fn equipment_for(name: &str) -> &'static str {
    match name {
        "Basilisk Collar" => "Some(EquipmentDef { power_delta: 0, toughness_delta: 0, add_subtype: None, controller_turn_keywords: Keywords(Keywords::DEATHTOUCH.0 | Keywords::LIFELINK.0), other_turn_keywords: Keywords(Keywords::DEATHTOUCH.0 | Keywords::LIFELINK.0), noncreature_spell_damage_to_each_opponent: 0, job_select: false, granted_activated_ability: None, pt_controller_turn_only: false })",
        "Sheltered by Ghosts" => "Some(EquipmentDef { power_delta: 1, toughness_delta: 0, add_subtype: None, controller_turn_keywords: Keywords::LIFELINK, other_turn_keywords: Keywords::LIFELINK, noncreature_spell_damage_to_each_opponent: 0, job_select: false, granted_activated_ability: None, pt_controller_turn_only: false })",
        _ => "None",
    }
}

/// Mandatory additional costs.
pub(super) fn additional_cost_for(name: &str) -> &'static str {
    match name {
        "Dusk Rose Reliquary" => {
            "Some(&[CostComponent::SacrificeControlled { count: 1, filter: PermanentFilter::ArtifactOrCreature }])"
        }
        _ => "None",
    }
}
