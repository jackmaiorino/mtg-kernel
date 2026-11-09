//! Exact placement-time modes and Kiora's independent resolution-time choice.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, Keywords, Subtype, Supertype, TargetSpec, CARD_DEFS},
    effect::{EffectOp, ObjectRef},
    limited_full_pool_consumer_v1::trigger_programs::{
        modal_programs, ProgramV1, KIORA, SYLVAN, WARDENS,
    },
    mana::{Cost, ManaColor, Pip},
    state::Zone,
    trigger::{self, TriggerCondition},
};
use serde_json::{json, Value};

pub(super) const SCION: u16 = 228;

pub(super) fn printing(id: u16, def: &CardDef) -> Result<Option<Value>> {
    use ManaColor::*;
    use ProgramV1::*;
    use TriggerCondition::*;
    let (printing, generic, pips, types, subtypes, supertypes, colors, power, toughness, triggers) =
        match def.name {
            "Kiora, the Rising Tide" => (
                KIORA, 2, &[Pip::Colored(U)][..], &[CardType::Creature][..],
                &[Subtype::Merfolk, Subtype::Noble][..], &[Supertype::Legendary][..], &[U][..],
                Some(3), Some(2), &[(Etb, KioraDrawDiscard),
                    (AttacksWithControllerGraveyardCardCountAtLeast(7), KioraThreshold)][..],
            ),
            "Sylvan Scavenging" => (
                SYLVAN, 1, &[Pip::Colored(G); 2][..], &[CardType::Enchantment][..],
                &[][..], &[][..], &[G][..], None, None,
                &[(BeginningControllerEndStep, SylvanChooseMode)][..],
            ),
            "Wardens of the Cycle" => (
                WARDENS, 1, &[Pip::Colored(B), Pip::Colored(G), Pip::Colored(G)][..],
                &[CardType::Creature][..], &[Subtype::Elf, Subtype::Warlock][..], &[][..], &[B, G][..],
                Some(3), Some(4), &[(BeginningControllerEndStepIfCreatureDied, WardensChooseMode)][..],
            ),
            "Scion of the Deep Token" => (
                SCION, 0, &[][..], &[CardType::Creature][..], &[Subtype::Octopus][..],
                &[Supertype::Legendary][..], &[U][..], Some(8), Some(8), &[][..],
            ),
            _ => return Ok(None),
        };
    let token = id == SCION;
    let cost = Cost { generic, pips, x_count: 0 };
    if id != printing
        || crate::card_def::card_id_by_name(def.name) != Some(id)
        || def.cost != cost
        || def.mana_value != u16::from(generic) + u16::try_from(pips.len()).unwrap()
        || def.types != types
        || def.subtypes != subtypes
        || def.supertypes != supertypes
        || def.colors != colors
        || def.power != power
        || def.toughness != toughness
        || def.keywords != Keywords::NONE
        || def.is_token != token
        || def.is_land
        || def.target_spec != TargetSpec::None
        || !def.activated_abilities.is_empty()
        || !def.produces_mana.is_empty()
        || !def.mana_ability_choices.is_empty()
        || (def.mana_ability)().is_some()
        || (def.spell_effect)() != (!token).then_some(EffectOp::MoveObject {
            object: ObjectRef::ThisSource, to_zone: Zone::Battlefield,
        })
        || def.object_name != if token { "Scion of the Deep" } else { def.name }
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let native = trigger::triggers_for(id);
    if native.len() != triggers.len() {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    for (actual, &(condition, program)) in native.iter().zip(triggers) {
        let effect = program.template().ok_or(Error::UnsupportedPrintedRoot(id))?;
        let branches = modal_programs::branches(program)
            .iter().map(|&(target, branch)| {
                let effect = branch.template().ok_or(Error::UnsupportedPrintedRoot(id))?;
                if trigger::target_spec_for_trigger(id, &effect) != Some(target) {
                    return Err(Error::UnsupportedPrintedRoot(id));
                }
                Ok((target, effect))
            }).collect::<Result<Vec<_>>>()?;
        let branches = (!branches.is_empty()).then_some(branches);
        if actual.condition != condition
            || actual.home_zone != Zone::Battlefield
            || actual.face_index != 0
            || actual.intervening_if_kicked
            || actual.intervening_if_controls_another_source_card
            || (actual.effect)() != effect
            || trigger::target_spec_for_trigger(id, &effect) != Some(TargetSpec::None)
            || trigger::unselected_trigger_modes(id, &effect) != branches
        {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
    }
    let cast = (!token).then(|| json!({"origin":"OwnHand","timing":"OwnMainPhaseEmptyStackWithPriority",
        "ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],"targets":[],
        "resolution":"MoveOriginalSpellToBattlefieldUnderCasterControl",
        "normal_zone_replacements_and_entry_triggers_apply":true,
        "countering_skips_entry_and_does_not_refund_cost":true,
        "other_cast_permissions_require_current_candidate_authority":true}));
    let meaning = match id {
        KIORA => {
            let scion = super::full_pool_modal_printing(SCION)?.ok_or(Error::UnsupportedPrintedRoot(SCION))?;
            json!([
                {"program":KioraDrawDiscard,"event":"OriginalPermanentEntersBattlefield",
                    "ordered_resolution":[
                        {"operation":"DrawCards","count":2,"recipient":"CapturedController","normal_draw_rules_apply":true},
                        {"operation":"DiscardCards","count":2,"chooser":"CapturedController",
                            "from":"CurrentHand","private_until_actual_discard":true}]},
                {"program":KioraThreshold,"event":"OriginalCreatureDeclaredAttacking",
                    "intervening_if":{"condition":"CapturedControllerGraveyardCardCountAtLeast","count":7,
                        "check_at_declaration_and_resolution":true},
                    "resolution_choice":{"chooser":"CapturedController","options":[
                        {"index":0,"operation":"Decline"},
                        {"index":1,"operation":"CreateToken","count":1,"token_definition":scion,
                            "normal_token_replacements_and_entry_triggers_apply":true}]}}
            ])
        }
        SYLVAN => {
            let token_id = crate::card_def::card_id_by_name("Raccoon Token")
                .ok_or(Error::UnsupportedPrintedRoot(id))?;
            let token = &CARD_DEFS[usize::from(token_id)];
            if !token.is_token || token.types != [CardType::Creature]
                || token.subtypes != [Subtype::Raccoon] || token.colors != [G]
                || token.power != Some(3) || token.toughness != Some(3)
                || token.keywords != Keywords::NONE || !token.supertypes.is_empty()
                || token.cost != Cost::zero() || token.mana_value != 0
                || !trigger::triggers_for(token_id).is_empty()
            {
                return Err(Error::UnsupportedPrintedRoot(id));
            }
            let token = super::baseline(token_id)?;
            json!([{"program":SylvanChooseMode,"event":"BeginningCapturedControllerEndStep",
                "placement_choice":{"chooser":"CapturedController","select_before_targets":true,
                    "options":[{"index":0,"program":SylvanCounter,"target":"CreatureControlledByCapturedController",
                        "requires_legal_target_to_offer":true,"target_rechecked_at_resolution":true,
                        "resolution":{"operation":"PutPlusOnePlusOneCounters","count":1,"target_slot":0}},
                        {"index":1,"program":SylvanRaccoon,"targets":[],
                            "condition_at_resolution":{"condition":"ControlsCreatureOfCurrentPowerAtLeast","power":4},
                            "resolution":{"operation":"CreateToken","count":1,"token_definition":token}}]}}])
        }
        WARDENS => json!([{"program":WardensChooseMode,"event":"BeginningCapturedControllerEndStep",
            "intervening_if":{"condition":"AnyCreatureDiedThisActualTurn",
                "check_at_trigger_event_and_resolution":true},
            "placement_choice":{"chooser":"CapturedController","options":[
                {"index":0,"program":WardensGainLife,"ordered_resolution":[{"operation":"GainLife","amount":2}]},
                {"index":1,"program":WardensDrawLoseLife,"ordered_resolution":[
                    {"operation":"DrawCards","count":1},{"operation":"LoseLife","amount":1}]}]}}]),
        SCION => json!([]),
        _ => return Err(Error::UnsupportedPrintedRoot(id)),
    };
    Ok(Some(json!({"card_id":id,"full_pool_modal_printing":{
        "schema":"printed-full-pool-modal-trigger/v1","cost":cost,"mana_value":def.mana_value,
        "types":def.types,"subtypes":def.subtypes,"colors":def.colors,
        "power":def.power,"toughness":def.toughness,"keywords":def.keywords.0,"token":token,
        "legend_rule_applies":!supertypes.is_empty(),"ordinary_cast":cast,
        "ordered_triggers":meaning,"trigger_contract":{"source_zone":"Battlefield","source_face":0,
            "requires_active_printed_ability_at_capture":true,"mandatory":true,"uses_stack":true,
            "controller":"CapturedController","source_departure_does_not_erase_captured_trigger":true},
    }})))
}
