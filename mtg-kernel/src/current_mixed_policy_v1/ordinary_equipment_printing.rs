//! Closed ordinary Equipment. Native recipes are checked before publishing meaning.
use super::{Error, Result};
use crate::{
    card_def::{
        ActivationTargetFilter, CardDef, CardType, CostComponent, Keywords, Subtype, TargetSpec,
    },
    effect::{EffectOp, ObjectRef},
    mana::Cost,
    state::Zone,
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(name, "Swiftfoot Boots" | "Quick-Draw Katana")
}

/// The caller applies the shared exclusion guard to every unrelated field.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let katana = d.name == "Quick-Draw Katana";
    let cost = Cost {
        generic: 2,
        pips: &[],
        x_count: 0,
    };
    let equip_cost = Cost {
        generic: if katana { 2 } else { 1 },
        pips: &[],
        x_count: 0,
    };
    let own_keywords = if katana {
        Keywords::FIRST_STRIKE
    } else {
        Keywords(Keywords::HASTE.0 | Keywords::HEXPROOF.0)
    };
    let other_keywords = if katana { Keywords::NONE } else { own_keywords };
    let Some(equipment) = d.equipment else {
        return Err(Error::UnsupportedPrintedRoot(id));
    };
    let [ability] = d.activated_abilities else {
        return Err(Error::UnsupportedPrintedRoot(id));
    };
    let equip_program = EffectOp::AttachSourceToTarget {
        object: ObjectRef::Target(0),
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != 2
        || d.types != [CardType::Artifact]
        || d.subtypes != [Subtype::Equipment]
        || !d.supertypes.is_empty()
        || !d.colors.is_empty()
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != Keywords::NONE
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || d.target_spec != TargetSpec::None
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
        || !crate::trigger::triggers_for(id).is_empty()
        || equipment.power_delta != if katana { 2 } else { 0 }
        || equipment.toughness_delta != 0
        || equipment.pt_controller_turn_only != katana
        || equipment.add_subtype.is_some()
        || equipment.controller_turn_keywords != own_keywords
        || equipment.other_turn_keywords != other_keywords
        || equipment.noncreature_spell_damage_to_each_opponent != 0
        || equipment.job_select
        || equipment.granted_activated_ability.is_some()
        || ability.cost != [CostComponent::Mana(equip_cost)]
        || ability.target_spec != TargetSpec::ControlledCreature
        || (ability.effect)() != equip_program
        || ability.activation_zone != Zone::Battlefield
        || !ability.sorcery_speed_only
        || ability.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
        || ability.max_activations_per_turn.is_some()
        || ability.face.is_some()
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    Ok(Some(json!({"card_id":id,"ordinary_equipment_printing":{
        "schema":"printed-equipment/v1","cost":cost,"mana_value":2,"types":d.types,
        "subtypes":d.subtypes,"supertypes":[],"colors":[],"power":null,"toughness":null,
        "keywords":0,"token":false,
        "ordered_cast_roots":[{"method":"Normal","route":"Hand","origin":"OwnHand",
            "timing":"OwnMainPhaseEmptyStackWithPriority","ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],
            "targets":[],"ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
            "countered_or_failed_spell_destination":"OwnersGraveyard","normal_zone_replacements_apply":true,
            "other_cast_permissions_require_current_candidate_authority":true}],
        "ordered_activation_roots":[{"ability_index":0,"activation_zone":"Battlefield",
            "timing":"ControllerMainPhaseEmptyStackWithPriority","uses_stack":true,
            "ordered_costs":[{"operation":"PayPrintedMana","cost":equip_cost}],"tap_cost":false,
            "target_slot":0,"target_filter":{"type":"Creature","controller":"AbilityController","zone":"Battlefield"},
            "captures_original_ability_controller":true,"target_legality_rechecked_at_resolution":true,
            "requires_original_source_and_target_incarnations":true,
            "ordered_resolution":[{"operation":"AttachOriginalEquipmentToOriginalTarget",
                "source_must_still_be_original_battlefield_incarnation":true,
                "normal_attachment_legality_and_state_based_unattachment_apply":true}],
            "previous_attachment_unchanged_if_target_illegal":true,"countered_or_failed_ability_does_not_refund_cost":true}],
        "ordered_static_roots":[{"recipient":"CurrentExactAttachedCreature","requires_active_printed_abilities":true,
            "power_delta":equipment.power_delta,"toughness_delta":0,"power_toughness_controller_turn_only":katana,
            "controller_turn_basis":"EquipmentController","controller_turn_keyword_bits":own_keywords.0,
            "other_turn_keyword_bits":other_keywords.0,"rechecked_from_current_active_player_and_equipment_controller":true,
            "normal_continuous_layers_and_ability_removal_apply":true,
            "ends_when_attachment_ends_or_original_source_leaves":true}]
    }})))
}
