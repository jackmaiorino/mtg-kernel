//! Entry modes chosen before targets, and an intervening other-Elf condition.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS},
    effect::{EffectCond, EffectOp, ObjectRef, PlayerRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
    trigger::{TriggerCondition, TriggeredAbilityDef},
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(name, "Apothecary Stomper" | "Dwynen's Elite")
}

pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let stomper = d.name == "Apothecary Stomper";
    let cost = Cost {
        generic: if stomper { 4 } else { 1 },
        pips: if stomper {
            &[Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::G)]
        } else {
            &[Pip::Colored(ManaColor::G)]
        },
        x_count: 0,
    };
    let subtypes = if stomper {
        &[Subtype::Elephant][..]
    } else {
        &[Subtype::Elf, Subtype::Warrior][..]
    };
    let size = if stomper { 4 } else { 2 };
    let keywords = if stomper {
        Keywords::VIGILANCE
    } else {
        Keywords::NONE
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != if stomper { 6 } else { 2 }
        || d.types != [CardType::Creature]
        || d.subtypes != subtypes
        || !d.supertypes.is_empty()
        || d.colors != [ManaColor::G]
        || d.power != Some(size)
        || d.toughness != Some(size)
        || d.keywords != keywords
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
    let counter = EffectOp::AddPlusOnePlusOneCounters {
        object: ObjectRef::Target(0),
        count: 2,
    };
    let gain = EffectOp::GainLife {
        player: PlayerRef::Controller,
        amount: 4,
    };
    let mut token_meaning = None;
    let program = if stomper {
        EffectOp::Choice {
            controller: PlayerRef::Controller,
            options: vec![counter.clone(), gain.clone()],
        }
    } else {
        let token_id = crate::card_def::card_id_by_name("Elf Warrior Token")
            .ok_or(Error::UnsupportedPrintedRoot(id))?;
        let token = &CARD_DEFS[usize::from(token_id)];
        if !token.is_token
            || token.types != [CardType::Creature]
            || token.subtypes != [Subtype::Elf, Subtype::Warrior]
            || token.colors != [ManaColor::G]
            || token.power != Some(1)
            || token.toughness != Some(1)
            || token.cost != Cost::zero()
            || token.mana_value != 0
            || token.keywords != Keywords::NONE
        {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
        token_meaning = Some(super::baseline(token_id)?);
        EffectOp::Conditional {
            cond: EffectCond::ControlsOtherIncarnationSubtypeCount {
                subtype: Subtype::Elf,
                minimum_count: 1,
            },
            then: Box::new(EffectOp::CreateToken {
                token_def: token_id,
                controller: PlayerRef::Controller,
            }),
            else_: Box::new(EffectOp::Sequence(vec![])),
        }
    };
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
    let expected_condition = if stomper {
        TriggerCondition::Etb
    } else {
        TriggerCondition::EtbControlsOtherSubtypeCount {
            subtype: Subtype::Elf,
            minimum_count: 1,
        }
    };
    if *condition != expected_condition
        || *home_zone != Zone::Battlefield
        || *face_index != 0
        || *intervening_if_kicked
        || *intervening_if_controls_another_source_card
        || effect() != program
        || crate::trigger::target_spec_for_trigger(id, &program) != Some(TargetSpec::None)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let modes = crate::trigger::unselected_trigger_modes(id, &program);
    if stomper {
        if modes
            != Some(vec![
                (TargetSpec::ControlledCreature, counter.clone()),
                (TargetSpec::None, gain.clone()),
            ])
            || crate::trigger::target_spec_for_trigger(id, &counter)
                != Some(TargetSpec::ControlledCreature)
            || crate::trigger::target_spec_for_trigger(id, &gain) != Some(TargetSpec::None)
        {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
    } else if modes.is_some() {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let cast = json!({"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,
        "timing":"OwnMainPhaseEmptyStackWithPriority","ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],
        "targets":[],"ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
        "countering_skips_entry_and_does_not_refund_cost":true,"countered_spell_destination":"OwnersGraveyard",
        "normal_zone_replacements_and_entry_triggers_apply":true,"other_cast_permissions_require_current_candidate_authority":true});
    let trigger = if stomper {
        json!({"event":"ThisPermanentEntersBattlefield","intervening_if":null,"mandatory":true,"uses_stack":true,
        "controller":"CapturedSourceControllerAtEntry","home_zone":"Battlefield","source_face":0,
        "requires_active_printed_abilities_at_capture":true,"captures_original_source_incarnation":true,
        "survives_source_departure_and_later_control_change":true,
        "choose_exactly_one_mode":{"at":"TriggerStackPlacementBeforeTargets","chooser":"CapturedTriggerController",
            "locked_after_selection":true,"no_mana_cost":true,"options":[
                {"index":0,"legal_only_when_required_targets_can_be_chosen":true,
                    "targets":[{"slot":0,"kind":"Object","zone":"Battlefield","type":"Creature",
                        "relation":"ControlledByCapturedTriggerController","includes_source_if_legal":true,
                        "uses_current_effective_type":true,"minimum":1,"maximum":1,
                        "binding":"OriginalSelectedObjectIncarnation","rechecked_at_resolution":true,
                        "normal_target_legality_and_protection_apply":true}],
                    "all_chosen_targets_illegal_skips_resolution":true,"does_not_fall_back_to_life_mode":true,
                    "ordered_resolution":[{"operation":"AddPlusOnePlusOneCounters","object_target_slot":0,"count":2,
                        "requires_original_battlefield_incarnation":true,"normal_counter_replacements_apply":true}]},
                {"index":1,"targets":[],"ordered_resolution":[{"operation":"GainLife","recipient":"CapturedTriggerController","amount":4,
                    "normal_life_gain_replacements_and_triggers_apply":true}]}
            ]}})
    } else {
        let another = json!({"zone":"BattlefieldOfCapturedTriggerController","subtype":"Elf","minimum_count":1,
            "uses_current_effective_subtypes":true,"excludes":"OriginalTriggerSourceIncarnation",
            "new_incarnation_of_same_arena_object_can_count":true,"does_not_require_creature_type":true});
        json!({"event":"ThisPermanentEntersBattlefield","trigger_if":another.clone(),"intervening_if":another,
            "check_condition_at":["TriggerCapture","TriggerResolution"],"false_at_capture_creates_no_trigger":true,
            "false_at_resolution_performs_no_instructions":true,"mandatory":true,"uses_stack":true,"targets":[],
            "controller":"CapturedSourceControllerAtEntry","home_zone":"Battlefield","source_face":0,
            "requires_active_printed_abilities_at_capture":true,"captures_original_source_incarnation":true,
            "survives_source_departure_and_later_control_change":true,
            "ordered_resolution":[{"operation":"CreateToken","count":1,"controller":"CapturedTriggerController",
                "token_definition":token_meaning,"normal_token_replacements_and_entry_triggers_apply":true}]})
    };
    Ok(Some(json!({"card_id":id,"entry_branch_printing":{
        "schema":"printed-entry-branches/v1","cost":cost,"mana_value":d.mana_value,"types":d.types,"subtypes":d.subtypes,
        "supertypes":[],"colors":d.colors,"power":size,"toughness":size,"keywords":keywords.0,"token":false,
        "ordered_cast_roots":[cast],"ordered_triggers":[trigger]}})))
}
