//! Closed two-target, one-way creature damage spell meanings.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, Keywords, TargetSpec},
    effect::EffectOp,
    mana::{Cost, ManaColor, Pip},
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(name, "Bite Down" | "Felling Blow")
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let felling = d.name == "Felling Blow";
    let cost = Cost {
        generic: if felling { 2 } else { 1 },
        pips: &[Pip::Colored(ManaColor::G)],
        x_count: 0,
    };
    let target = if felling {
        TargetSpec::ControlledCreatureThenOpponentCreature
    } else {
        TargetSpec::ControlledCreatureThenOpponentCreatureOrPlaneswalker
    };
    let program = EffectOp::CreatureTargetPowerDamage {
        source_index: 0,
        target_index: 1,
        plus1_plus1: i16::from(felling),
        target_spec: target,
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != if felling { 3 } else { 2 }
        || d.types
            != [if felling {
                CardType::Sorcery
            } else {
                CardType::Instant
            }]
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || d.colors != [ManaColor::G]
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != Keywords::NONE
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || d.target_spec != target
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)() != Some(program)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let cast_resolution = json!([{"operation":"CreaturePowerDamage",
                "creature_source_slot":0,"damage_recipient_slot":1,
                "when_original_source_target_still_legal":{"add_plus_one_plus_one_counters":i32::from(felling),
                    "normal_counter_replacements_apply":true,"counter_addition_does_not_require_legal_recipient":true,
                    "when_original_recipient_target_also_legal":{"damage_source":"OriginalTargetZeroCreature",
                        "amount":"CurrentEffectiveSourcePowerAfterCounterAdditionClampedAtZero",
                        "damage_class":"Noncombat","zero_power_deals_no_damage_event":true,
                        "normal_source_keywords_replacements_prevention_and_state_based_actions_apply":true}},
                "illegal_source_cannot_deal_damage_or_receive_counter":true,
                "no_retaliation_or_fight_damage":true,"no_priority_between_counter_and_damage":true}]);
    let cast_roots = json!([{"method":"Normal","route":"Hand","origin":"OwnHand",
            "timing":if felling{"OwnMainPhaseEmptyStackWithPriority"}else{"InstantWithPriority"},
            "ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],"target_count":2,
            "ordered_targets":[{"slot":0,"zone":"Battlefield","type":"Creature","controller":"Caster"},
                {"slot":1,"zone":"Battlefield","types_any_of":if felling{json!(["Creature"])}else{json!(["Creature","Planeswalker"])},"controller":"OpponentOfCaster"}],
            "target_types_and_control_use_current_effective_characteristics":true,
            "each_original_target_incarnation_and_filter_rechecked_at_resolution":true,
            "illegal_target_slots_are_not_compacted_or_reassigned":true,
            "all_targets_illegal_skips_entire_resolution":true,
            "ordered_resolution":cast_resolution,
            "physical_spell_destination_after_resolution_or_counter":"OwnersGraveyard",
            "virtual_spell_copy_departure":"CeaseWithoutEnteringCardZone","countering_skips_all_resolution_instructions":true,
            "countering_does_not_refund_cost":true,"other_cast_permissions_require_current_candidate_authority":true,
            "normal_zone_replacements_apply":true}]);
    Ok(Some(json!({"card_id":id,"creature_power_spell_printing":{
        "schema":"printed-two-target-creature-power-spell/v1","cost":cost,"mana_value":d.mana_value,
        "types":d.types,"subtypes":[],"supertypes":[],"colors":d.colors,"power":null,
        "toughness":null,"keywords":0,"token":false,
        "ordered_cast_roots":cast_roots
    }})))
}
