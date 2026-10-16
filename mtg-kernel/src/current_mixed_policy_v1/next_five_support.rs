//! Exact ordinary Aura support required by the expanded Replication path.
//! Private validation covers every CardDef field; the descriptor is not executable.
use super::{Error, Result};
use crate::card_def::{CardCapability, CardDef, CardType, Keywords, Subtype, TargetSpec};
use crate::effect::{EffectOp, ObjectRef};
use crate::mana::{Cost, ManaColor, Pip};
use serde_json::{json, Value};
pub(super) fn printing(id: u16) -> Result<Option<Value>> {
    let d = crate::card_def::CARD_DEFS
        .get(usize::from(id))
        .ok_or(Error::UnsupportedPrintedRoot(id))?;
    if d.name != "Witness Protection" {
        return Ok(None);
    }
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
    } = d;
    if d.object_name != d.name
        || d.capability != CardCapability::Full
        || d.cost
            != (Cost {
                generic: 0,
                pips: &[Pip::Colored(ManaColor::U)],
                x_count: 0,
            })
        || d.mana_value != 1
        || d.types != [CardType::Enchantment]
        || d.subtypes != [Subtype::Aura]
        || d.colors != [ManaColor::U]
        || !d.supertypes.is_empty()
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != Keywords::NONE
        || !d.activated_abilities.is_empty()
        || d.is_land
        || d.is_token
        || d.minimum_blockers != 1
        || d.target_spec != TargetSpec::Creature
        || (d.spell_effect)()
            != Some(EffectOp::PutSourceOntoBattlefieldAttachedToTarget {
                target: ObjectRef::Target(0),
            })
        || !d.produces_mana.is_empty()
        || (d.mana_ability)().is_some()
        || !d.mana_ability_choices.is_empty()
        || d.generic_cost_reduction.is_some()
        || d.ward_cost.is_some()
        || d.spell_cannot_be_countered
        || d.records_mana_spent
        || d.equipment.is_some()
        || d.attachment
            != Some(crate::card_def::AttachmentDef::AuraCreatureOverride(
                crate::card_def::CreatureCharacteristicsOverrideDef {
                    name: "Legitimate Businessperson",
                    subtype: Subtype::Citizen,
                    colors: &[ManaColor::G, ManaColor::W],
                    power: 1,
                    toughness: 1,
                    loses_abilities: true,
                },
            ))
        || d.starting_loyalty.is_some()
        || d.enters_with_plus_one_counters.is_some()
        || d.controlled_counter_keyword.is_some()
        || d.controlled_creature_grant.is_some()
        || d.game_outcome_protection_v1.is_some()
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
        || d.enters_battlefield_tapped
        || d.mana_ability_def.is_some()
        || d.omen.is_some()
        || d.mana_ability_includes_chosen_color
        || d.as_enters_choose_color_other_than.is_some()
        || !d.additional_mana_abilities.is_empty()
        || d.enters_battlefield_tapped_unless.is_some()
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
        || d.animation.is_some()
        || !d.activated_ability_generic_reductions.is_empty()
        || d.combat_static_rules != crate::card_def::CombatStaticRulesDef::NONE
        || d.as_enters_color_choice_v1.is_some()
        || d.chosen_color_creature_boost_v1.is_some()
        || d.damage_replacement_v1.is_some()
        || d.token_counter_replacement_v48.is_some()
        || !d.starting_life_self_bonuses_v1.is_empty()
        || d.source_type_hexproof_v1.is_some()
        || d.other_controlled_nontoken_death_v1.is_some()
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }

    if !crate::trigger::triggers_for(id).is_empty() {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    Ok(Some(json!({"card_id":id,"replication_aura_printing":{
        "characteristics":{"types":d.types,"subtypes":d.subtypes,"colors":d.colors,"mana_value":1,"token":false},
        "ordered_cast_rules":["CasterMainPhaseEmptyStackPriorityFromHand", {"PayPrintedMana":d.cost},
            "TargetOneCurrentCreatureOfEitherController", "RecheckExactTargetIncarnationBeforeResolution",
            "MoveOriginalSpellToBattlefieldUnderCasterAttachedToOriginalTargetBeforeEntryObservers"],
        "all_targets_illegal_sends_physical_spell_to_owner_graveyard":true,
        "countered_spell_sends_physical_spell_to_owner_graveyard":true,
        "ordinary_zone_replacements_apply":true,
        "continuous_characteristics":{"bound_current_host_only":true,"timestamp_layer_order":true,
            "set_name":"LegitimateBusinessperson","replace_subtypes":["Citizen"],"replace_colors":["G","W"],
            "base_power":1,"base_toughness":1,"remove_all_abilities":true,
            "counters_and_later_effects_still_apply":true,"until_source_unattached_or_leaves":true},
        "copied_aura_entry":{"untargeted_current_legal_creature_host":true,"one_choice_per_expanded_copy":true,
            "repeated_host_allowed":true,"no_host_no_entry":true,"attachment_before_entry_observers":true}
    }})))
}
