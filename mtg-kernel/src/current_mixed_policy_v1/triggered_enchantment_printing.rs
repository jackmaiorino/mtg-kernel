//! Closed upkeep/cast observers with checked native programs and token meanings.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, Keywords, TargetSpec},
    effect::{EffectOp, ObjectRef, PlayerRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
    trigger::{TriggerCondition, TriggeredAbilityDef},
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(name, "Phyrexian Arena" | "Rite of the Dragoncaller")
}

/// All unrelated CardDef fields must still pass baseline_recipe's shared guard.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let arena = d.name == "Phyrexian Arena";
    let cost = Cost {
        generic: if arena { 1 } else { 4 },
        pips: if arena {
            &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::B)]
        } else {
            &[Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::R)]
        },
        x_count: 0,
    };
    let (condition, program, event, ordered) = if arena {
        (
            TriggerCondition::BeginningOfUpkeep {
                controller_only: true,
            },
            EffectOp::Sequence(vec![
                EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 1,
                },
                EffectOp::LoseLife {
                    player: PlayerRef::Controller,
                    amount: 1,
                },
            ]),
            json!({"operation":"BeginningOfUpkeep","player":"CurrentSourceController","opponents_upkeep_does_not_trigger":true}),
            vec![
                json!({"operation":"DrawCards","recipient":"CapturedTriggerController","count":1,"normal_draw_replacements_and_empty_library_rules_apply":true}),
                json!({"operation":"LoseLife","recipient":"CapturedTriggerController","amount":1,"is_not_damage":true,"does_not_require_successful_draw":true}),
            ],
        )
    } else {
        let token = crate::card_def::card_id_by_name("Dragon 5/5 Token")
            .ok_or(Error::UnsupportedPrintedRoot(id))?;
        (
            TriggerCondition::CastInstantOrSorcery,
            EffectOp::CreateToken {
                token_def: token,
                controller: PlayerRef::Controller,
            },
            json!({"operation":"CastInstantOrSorcery","caster":"CurrentSourceController","uses_actual_cast_event":true,
            "creature_or_other_nonmatching_casts_do_not_trigger":true,"spell_copy_without_cast_does_not_trigger":true,
            "opponent_cast_does_not_trigger":true,"cast_need_not_resolve":true}),
            vec![
                json!({"operation":"CreateToken","controller":"CapturedTriggerController","count":1,
            "token_definition":super::baseline(token)?,"entry_tapped":false,"normal_token_replacements_and_entry_triggers_apply":true}),
            ],
        )
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != if arena { 3 } else { 6 }
        || d.types != [CardType::Enchantment]
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || d.colors != [if arena { ManaColor::B } else { ManaColor::R }]
        || d.power.is_some()
        || d.toughness.is_some()
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
        condition: actual,
        home_zone,
        intervening_if_kicked,
        intervening_if_controls_another_source_card,
        face_index,
        effect,
    } = trigger;
    if *actual != condition
        || *home_zone != Zone::Battlefield
        || *face_index != 0
        || *intervening_if_kicked
        || *intervening_if_controls_another_source_card
        || effect() != program
        || crate::trigger::target_spec_for_trigger(id, &program) != Some(TargetSpec::None)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let cast = json!({"method":"Normal","route":"Hand","origin":"OwnHand","timing":"OwnMainPhaseEmptyStackWithPriority",
        "ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],"targets":[],
        "ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
        "countering_skips_entry_and_does_not_refund_cost":true,"failed_or_countered_spell_destination":"OwnersGraveyard",
        "normal_zone_replacements_and_entry_triggers_apply":true,"other_cast_permissions_require_current_candidate_authority":true});
    let triggers = json!([{"event":event,"home_zone":"Battlefield","source_face":0,"requires_active_printed_abilities":true,
        "intervening_if":null,"mandatory":true,"uses_stack":true,"targets":[],"controller":"ObserverControllerAtEvent",
        "captures_original_source_incarnation_and_controller":true,"survives_source_departure_and_later_control_change":true,
        "ordered_resolution":ordered,"no_priority_between_resolution_instructions":true}]);
    Ok(Some(json!({"card_id":id,"triggered_enchantment_printing":{
        "schema":"printed-upkeep-cast-enchantment/v1","cost":cost,"mana_value":d.mana_value,"types":d.types,
        "subtypes":[],"supertypes":[],"colors":d.colors,"power":null,"toughness":null,"keywords":0,"token":false,
        "ordered_cast_roots":[cast],"ordered_triggers":triggers}})))
}
