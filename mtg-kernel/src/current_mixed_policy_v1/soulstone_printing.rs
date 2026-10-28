//! Closed Soulstone Sanctuary land-play and persistent-animation recipe.
//! Current effective characteristics remain sourced from the native public Dto.
use super::*;
use crate::card_def::{CardType, Keywords, Subtype};
use crate::effect::{EffectOp, ObjectRef, PlayerRef};
use crate::mana::{Cost, ManaColor};
use crate::state::Zone;

fn validate_shape(id: u16, d: &CardDef) -> Result<()> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) || d.name != "Soulstone Sanctuary"
        || CARD_DEFS
            .get(usize::from(id))
            .is_none_or(|actual| actual.name != d.name)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    // Keep this exhaustive: a new CardDef field cannot silently enter this
    // authenticated ordinary-land recipe with an unauthenticated default.
    let crate::card_def::CardDef {
        name: _,
        capability: _,
        cost: _,
        generic_cost_reduction: _,
        ward_cost: _,
        spell_cannot_be_countered: _,
        records_mana_spent: _,
        equipment: _,
        types: _,
        subtypes: _,
        supertypes: _,
        power: _,
        toughness: _,
        starting_loyalty: _,
        enters_with_plus_one_counters: _,
        controlled_counter_keyword: _,
        game_outcome_protection_v1: _,
        controlled_creature_grant: _,
        is_land: _,
        produces_mana: _,
        colors: _,
        target_spec: _,
        keywords: _,
        spell_effect: _,
        mana_ability: _,
        alt_cost: _,
        kicker_cost: _,
        additional_cost: _,
        flashback: _,
        activated_abilities: _,
        plot_cost: _,
        madness_cost: _,
        mode2: _,
        mode3: _,
        additional_modes: _,
        kicked_target_spec: _,
        is_token: _,
        escape: _,
        mana_ability_choices: _,
        enters_battlefield_tapped: _,
        mana_ability_def: _,
        minimum_blockers: _,
        omen: _,
        mana_value: _,
        mana_ability_includes_chosen_color: _,
        as_enters_choose_color_other_than: _,
        additional_mana_abilities: _,
        object_name: _,
        enters_battlefield_tapped_unless: _,
        attachment: _,
        transform_face: _,
        saga: _,
        optional_additional_cost: _,
        changeling: _,
        bestow: _,
        delve: _,
        adventure: _,
        cant_be_blocked_by_monarchs_creatures: _,
        conditional_tap_yield: _,
        enters_tapped_unless_controller: _,
        additional_mana_ability_conditions: _,
        restricted_mana_abilities: _,
        animation: _,
        activated_ability_generic_reductions: _,
        combat_static_rules: _,
        as_enters_color_choice_v1: _,
        chosen_color_creature_boost_v1: _,
        damage_replacement_v1: _,
        token_counter_replacement_v48: _,
        starting_life_self_bonuses_v1: _,
        source_type_hexproof_v1: _,
        other_controlled_nontoken_death_v1: _,
        creature_type_entry_v1: _,
    } = d;

    let land_mana = Some(EffectOp::Sequence(vec![
        EffectOp::TapObject {
            object: ObjectRef::ThisSource,
        },
        EffectOp::AddMana {
            player: PlayerRef::Controller,
            colors: vec![ManaColor::C],
        },
    ]));
    let animation_cost = Cost {
        generic: 4,
        pips: &[],
        x_count: 0,
    };
    let activation = d.activated_abilities;
    let activation_matches = activation.len() == 1
        && activation[0].cost == [crate::card_def::CostComponent::Mana(animation_cost)]
        && activation[0].target_spec == TargetSpec::None
        && (activation[0].effect)() == EffectOp::AnimateSourcePermanentlyV1
        && activation[0].activation_zone == Zone::Battlefield
        && !activation[0].sorcery_speed_only
        && activation[0].activation_target_filter
            == crate::card_def::ActivationTargetFilter::TargetSpecOnly
        && activation[0].max_activations_per_turn.is_none()
        && activation[0].face.is_none();

    if d.object_name != d.name
        || d.capability != CardCapability::Full
        || d.cost != Cost::zero()
        || d.mana_value != 0
        || d.types != [CardType::Land]
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || !d.colors.is_empty()
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != Keywords::NONE
        || d.target_spec != TargetSpec::None
        || !d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || d.minimum_blockers != 1
        || d.produces_mana != [ManaColor::C]
        || d.mana_ability_choices != [ManaColor::C]
        || (d.mana_ability)() != land_mana
        || d.mana_ability_def.is_some()
        || !d.additional_mana_abilities.is_empty()
        || d.generic_cost_reduction.is_some()
        || d.ward_cost.is_some()
        || d.spell_cannot_be_countered
        || d.records_mana_spent
        || d.equipment.is_some()
        || d.starting_loyalty.is_some()
        || d.enters_with_plus_one_counters.is_some()
        || d.controlled_counter_keyword.is_some()
        || d.game_outcome_protection_v1.is_some()
        || d.controlled_creature_grant.is_some()
        || (d.spell_effect)().is_some()
        || d.alt_cost.is_some()
        || d.kicker_cost.is_some()
        || d.additional_cost.is_some()
        || d.flashback.is_some()
        || d.plot_cost.is_some()
        || d.madness_cost.is_some()
        || d.mode2.is_some()
        || d.mode3.is_some()
        || !d.additional_modes.is_empty()
        || d.kicked_target_spec.is_some()
        || d.escape.is_some()
        || d.omen.is_some()
        || d.mana_ability_includes_chosen_color
        || d.as_enters_choose_color_other_than.is_some()
        || d.enters_battlefield_tapped_unless.is_some()
        || d.attachment.is_some()
        || d.transform_face.is_some()
        || d.saga.is_some()
        || d.optional_additional_cost.is_some()
        || d.changeling
        || d.bestow.is_some()
        || d.delve
        || d.adventure.is_some()
        || d.cant_be_blocked_by_monarchs_creatures
        || d.conditional_tap_yield.is_some()
        || d.enters_tapped_unless_controller.is_some()
        || !d.additional_mana_ability_conditions.is_empty()
        || !d.restricted_mana_abilities.is_empty()
        || !activation_matches
        || d.animation.is_none_or(|animation| {
            animation.until_end_of_turn
                || animation.power != 3
                || animation.toughness != 3
                || animation.artifact
                || !animation.colors.is_empty()
                || animation.subtypes != Subtype::CREATURE_TYPES
                || animation.keywords != Keywords::VIGILANCE
        })
        || !d.activated_ability_generic_reductions.is_empty()
        || d.combat_static_rules != crate::card_def::CombatStaticRulesDef::NONE
        || d.as_enters_color_choice_v1.is_some()
        || d.chosen_color_creature_boost_v1.is_some()
        || d.damage_replacement_v1.is_some()
        || d.token_counter_replacement_v48.is_some()
        || !d.starting_life_self_bonuses_v1.is_empty()
        || d.source_type_hexproof_v1.is_some()
        || (d.other_controlled_nontoken_death_v1.is_some() || d.creature_type_entry_v1.is_some())
        || !crate::trigger::triggers_for(id).is_empty()
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    Ok(())
}

#[derive(serde::Serialize)]
enum LandPlayTiming {
    ActivePlayerMainPhaseWithPriority,
}
#[derive(serde::Serialize)]
enum LandEntry {
    PutOriginalLandOnBattlefieldUnderItsController,
}
#[derive(serde::Serialize)]
struct OrdinaryLandPlay {
    origin: Zone,
    timing: LandPlayTiming,
    requires_empty_stack: bool,
    consumes_current_land_play_permission: bool,
    is_a_spell_or_stack_object: bool,
    land_playing_player_becomes_controller: bool,
    printed_owner_is_preserved: bool,
    ordered_resolution: [LandEntry; 1],
    ordinary_land_permissions_and_zone_replacements_apply: bool,
}
#[derive(serde::Serialize)]
struct ManaAbility {
    ordered_costs: [ManaCost; 1],
    recipient: ManaRecipient,
    colors: Vec<ManaColor>,
    amount: u8,
}
#[derive(serde::Serialize)]
enum ManaCost {
    TapOriginalSource,
}
#[derive(serde::Serialize)]
enum ManaRecipient {
    CurrentAbilityController,
}
#[derive(serde::Serialize)]
enum ActivationTiming {
    InstantWithPriority,
}
#[derive(serde::Serialize)]
enum ActivationTarget {
    None,
}
#[derive(serde::Serialize)]
enum ActivationCost {
    PrintedMana(Cost),
}
#[derive(serde::Serialize)]
enum ActivationEffect {
    AnimateOriginalSourceIncarnation,
}
#[derive(serde::Serialize)]
struct OrdinaryActivation {
    ability_index: u8,
    activation_zone: Zone,
    timing: ActivationTiming,
    targets: ActivationTarget,
    ordered_costs: [ActivationCost; 1],
    requires_source_untapped: bool,
    sorcery_speed_only: bool,
    ordered_resolution: [ActivationEffect; 1],
    resolves_only_on_original_source_battlefield_incarnation: bool,
    source_controller_pays_cost: bool,
}
#[derive(serde::Serialize)]
struct AnimationCharacteristics {
    base_power: i16,
    base_toughness: i16,
    artifact: bool,
    colors: Vec<ManaColor>,
    empty_animation_colors_preserve_current_color: bool,
    adds_creature_type: bool,
    creature_subtypes: Vec<Subtype>,
    vigilance: bool,
    preserves_printed_land_type: bool,
    duration: AnimationDuration,
    applies_to_original_source_incarnation: bool,
    summoning_sickness_rules_apply_normally: bool,
}
#[derive(serde::Serialize)]
enum AnimationDuration {
    WhileOriginalSourceRemainsOnBattlefield,
}

pub(super) fn baseline(id: u16, d: &CardDef) -> Result<Value> {
    validate_shape(id, d)?;
    Ok(json!({"card_id":id,"primitive_printing":{
        "cost":d.cost,"types":[CardType::Land],"subtypes":[],"supertypes":[],
        "colors":d.colors,"power":null,"toughness":null,"keywords":d.keywords.0,
        "mana_value":0,"token":false,"ordered_instructions":[{"operation":"PlayLand"}],
        "mana_ability":{"cost":"TapSelf","recipient":"Controller","colors":[ManaColor::C],"amount":1}
    }}))
}

pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    validate_shape(id, d)?;
    let animation = d.animation.ok_or(Error::UnsupportedPrintedRoot(id))?;
    let activated_cost = match d.activated_abilities[0].cost {
        [crate::card_def::CostComponent::Mana(cost)] => *cost,
        _ => return Err(Error::UnsupportedPrintedRoot(id)),
    };
    let land_play = OrdinaryLandPlay {
        origin: Zone::Hand,
        timing: LandPlayTiming::ActivePlayerMainPhaseWithPriority,
        requires_empty_stack: true,
        consumes_current_land_play_permission: true,
        is_a_spell_or_stack_object: false,
        land_playing_player_becomes_controller: true,
        printed_owner_is_preserved: true,
        ordered_resolution: [LandEntry::PutOriginalLandOnBattlefieldUnderItsController],
        ordinary_land_permissions_and_zone_replacements_apply: true,
    };
    let mana = ManaAbility {
        ordered_costs: [ManaCost::TapOriginalSource],
        recipient: ManaRecipient::CurrentAbilityController,
        colors: d.produces_mana.to_vec(),
        amount: 1,
    };
    let activation = OrdinaryActivation {
        ability_index: 0,
        activation_zone: Zone::Battlefield,
        timing: ActivationTiming::InstantWithPriority,
        targets: ActivationTarget::None,
        ordered_costs: [ActivationCost::PrintedMana(activated_cost)],
        requires_source_untapped: false,
        sorcery_speed_only: false,
        ordered_resolution: [ActivationEffect::AnimateOriginalSourceIncarnation],
        resolves_only_on_original_source_battlefield_incarnation: true,
        source_controller_pays_cost: true,
    };
    let animation_meaning = AnimationCharacteristics {
        base_power: animation.power,
        base_toughness: animation.toughness,
        artifact: animation.artifact,
        colors: animation.colors.to_vec(),
        empty_animation_colors_preserve_current_color: animation.colors.is_empty(),
        adds_creature_type: true,
        creature_subtypes: animation.subtypes.to_vec(),
        vigilance: animation.keywords == Keywords::VIGILANCE,
        preserves_printed_land_type: true,
        duration: AnimationDuration::WhileOriginalSourceRemainsOnBattlefield,
        applies_to_original_source_incarnation: true,
        summoning_sickness_rules_apply_normally: true,
    };
    Ok(Some(json!({"card_id":id,"registered_permanent_printing":{
        "characteristics":{"types":d.types,"subtypes":d.subtypes,"supertypes":[],
            "colors":d.colors,"mana_value":d.mana_value,"power":d.power,"toughness":d.toughness,
            "ordered_keywords":[],"minimum_blockers":d.minimum_blockers,"token":d.is_token,
            "land":d.is_land,"enters_tapped":d.enters_battlefield_tapped,
            "printed_mana_abilities":1,"printed_activated_abilities":1},
        "ordinary_land_play":land_play,
        "printed_mana_abilities":[mana],
        "printed_activated_abilities":[activation],
        "animation":animation_meaning
    }})))
}

#[cfg(test)]
#[path = "tests/soulstone_printing.rs"]
mod tests;
