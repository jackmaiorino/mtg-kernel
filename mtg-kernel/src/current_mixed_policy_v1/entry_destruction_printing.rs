//! Targeted entry destruction, including the Sage's separate resolution choice.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, Keywords, Subtype, TargetSpec},
    effect::{EffectCond, EffectOp, ObjectRef, PlayerRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
    trigger::{TriggerCondition, TriggeredAbilityDef},
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(name, "Meteor Golem" | "Reclamation Sage")
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let sage = d.name == "Reclamation Sage";
    let cost = Cost {
        generic: if sage { 2 } else { 7 },
        pips: if sage {
            &[Pip::Colored(ManaColor::G)]
        } else {
            &[]
        },
        x_count: 0,
    };
    let (types, subtypes, colors, power, toughness, spec) = if sage {
        (
            &[CardType::Creature][..],
            &[Subtype::Elf, Subtype::Shaman][..],
            &[ManaColor::G][..],
            2,
            1,
            TargetSpec::ArtifactOrEnchantmentPermanent,
        )
    } else {
        (
            &[CardType::Artifact, CardType::Creature][..],
            &[Subtype::Golem][..],
            &[][..],
            3,
            3,
            TargetSpec::OpponentNonlandPermanent,
        )
    };
    let destroy = EffectOp::Conditional {
        cond: EffectCond::TargetInZone(0, Zone::Battlefield),
        then: Box::new(EffectOp::DestroyObject {
            object: ObjectRef::Target(0),
        }),
        else_: Box::new(EffectOp::Sequence(vec![])),
    };
    let program = if sage {
        EffectOp::Choice {
            controller: PlayerRef::Controller,
            options: vec![EffectOp::Sequence(vec![]), destroy],
        }
    } else {
        destroy
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != if sage { 3 } else { 7 }
        || d.types != types
        || d.subtypes != subtypes
        || !d.supertypes.is_empty()
        || d.colors != colors
        || d.power != Some(power)
        || d.toughness != Some(toughness)
        || d.keywords != Keywords::NONE
        || d.target_spec != TargetSpec::None
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || !d.activated_abilities.is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let [trigger] = crate::trigger::triggers_for(id) else {
        return Err(Error::UnsupportedPrintedRoot(id));
    };
    let TriggeredAbilityDef {
        condition,
        home_zone,
        face_index,
        intervening_if_kicked,
        intervening_if_controls_another_source_card,
        effect,
    } = trigger;
    if *condition != TriggerCondition::Etb
        || *home_zone != Zone::Battlefield
        || *face_index != 0
        || *intervening_if_kicked
        || *intervening_if_controls_another_source_card
        || effect() != program
        || crate::trigger::target_spec_for_trigger(id, &program) != Some(spec)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let cast = json!({"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,
        "timing":"OwnMainPhaseEmptyStackWithPriority","ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],
        "targets":[],"ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
        "countering_skips_entry_and_does_not_refund_cost":true,"countered_spell_destination":"OwnersGraveyard",
        "normal_zone_replacements_and_entry_triggers_apply":true,"other_cast_permissions_require_current_candidate_authority":true});
    let destruction = json!({"operation":"DestroyPermanent","target_slot":0,"requires_original_battlefield_incarnation":true,
        "respects_indestructible_and_normal_replacements":true,"is_not_sacrifice":true});
    let ordered = if sage {
        vec![
            json!({"operation":"ChoosePrintedEffectOption","chooser":"CapturedTriggerController","at":"TriggerResolution",
            "options":[{"index":0,"ordered_instructions":[]},{"index":1,"ordered_instructions":[destruction]}],
            "does_not_reselect_targets":true,"source_departure_does_not_change_chooser":true}),
        ]
    } else {
        vec![destruction]
    };
    let target = json!({"slot":0,"kind":"Object","zone":"Battlefield","minimum":1,"maximum":1,
        "filter":if sage {json!({"has_any_current_type":["Artifact","Enchantment"],"relation":"AnyController"})}
            else {json!({"is_permanent":true,"lacks_current_type":"Land","relation":"ControlledByOpponentOfCapturedTriggerController"})},
        "uses_current_effective_types":true,"chosen_at":"TriggerStackPlacement","chooser":"CapturedTriggerController",
        "normal_target_legality_and_protection_apply":true,"rechecked_at_resolution":true,"binding":"OriginalSelectedObjectIncarnation"});
    let trigger = json!({"event":"ThisPermanentEntersBattlefield","home_zone":"Battlefield","source_face":0,
        "requires_active_printed_abilities_at_capture":true,"intervening_if":null,"mandatory":true,"uses_stack":true,
        "controller":"CapturedSourceControllerAtEntry","captures_original_source_incarnation":true,
        "survives_source_departure_and_later_control_change":true,"targets":[target],
        "optional_resolution_does_not_make_target_selection_optional":sage,"required_target_absence_prevents_stack_placement":true,
        "all_targets_illegal_skips_entire_resolution_including_optional_choice":true,"ordered_resolution":ordered});
    Ok(Some(json!({"card_id":id,"entry_destruction_printing":{
        "schema":"printed-targeted-entry-destruction/v1","cost":cost,"mana_value":d.mana_value,"types":d.types,
        "subtypes":d.subtypes,"supertypes":[],"colors":d.colors,"power":power,"toughness":toughness,
        "keywords":0,"token":false,"ordered_cast_roots":[cast],"ordered_triggers":[trigger]}})))
}
