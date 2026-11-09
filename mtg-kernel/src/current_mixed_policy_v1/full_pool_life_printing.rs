//! Exact ordinary casts and printed life-gain trigger programs for FullPool.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, Keywords, Subtype, TargetSpec},
    effect::{EffectOp, ObjectRef},
    limited_full_pool_consumer_v1::life_triggers::{
        is_printing, ProgramV1, COLLECTOR, EXEMPLAR, VANGUARD,
    },
    mana::{Cost, ManaColor, Pip},
    state::Zone,
    trigger::{self, TriggerCondition},
};
use serde_json::{json, Value};

pub(super) fn printing(id: u16, def: &CardDef) -> Result<Option<Value>> {
    use ProgramV1::*;
    use TriggerCondition::*;
    let (printing, generic, white_pips, subtypes, keywords, toughness, roots) = match def.name {
        "Exemplar of Light" => (
            EXEMPLAR,
            2,
            &[Pip::Colored(ManaColor::W); 2][..],
            &[Subtype::Angel][..],
            Keywords::FLYING,
            3,
            &[
                (ControllerGainsLife, ExemplarCounter),
                (
                    ControllerAddedPlusOneCountersToSelf {
                        max_per_turn: Some(1),
                    },
                    ExemplarDraw,
                ),
            ][..],
        ),
        "Vanguard Seraph" => (
            VANGUARD,
            3,
            &[Pip::Colored(ManaColor::W)][..],
            &[Subtype::Angel, Subtype::Warrior][..],
            Keywords::FLYING,
            3,
            &[(
                ControllerFirstLifeGain {
                    own_turn_only: false,
                },
                VanguardSurveil,
            )][..],
        ),
        "Cat Collector" => (
            COLLECTOR,
            2,
            &[Pip::Colored(ManaColor::W)][..],
            &[Subtype::Human, Subtype::Citizen][..],
            Keywords::NONE,
            2,
            &[
                (Etb, CollectorFood),
                (
                    ControllerFirstLifeGain {
                        own_turn_only: true,
                    },
                    CollectorCat,
                ),
            ][..],
        ),
        _ => return Ok(None),
    };
    let cost = Cost {
        generic,
        pips: white_pips,
        x_count: 0,
    };
    if id != printing
        || !is_printing(id)
        || crate::card_def::card_id_by_name(def.name) != Some(id)
        || def.cost != cost
        || def.mana_value != u16::from(generic) + u16::try_from(white_pips.len()).unwrap()
        || def.types != [CardType::Creature]
        || def.subtypes != subtypes
        || !def.supertypes.is_empty()
        || def.colors != [ManaColor::W]
        || def.power != Some(3)
        || def.toughness != Some(toughness)
        || def.keywords != keywords
        || def.is_token
        || def.is_land
        || def.target_spec != TargetSpec::None
        || !def.activated_abilities.is_empty()
        || !def.produces_mana.is_empty()
        || !def.mana_ability_choices.is_empty()
        || (def.mana_ability)().is_some()
        || (def.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }

    let triggered = trigger::triggers_for(id);
    if triggered.len() != roots.len() {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let mut meanings = Vec::with_capacity(roots.len());
    for (ordinal, (def, &(condition, program))) in triggered.iter().zip(roots).enumerate() {
        let effect = program
            .template()
            .ok_or(Error::UnsupportedPrintedRoot(id))?;
        if def.condition != condition
            || def.home_zone != Zone::Battlefield
            || def.face_index != 0
            || def.intervening_if_kicked
            || def.intervening_if_controls_another_source_card
            || (def.effect)() != effect
            || trigger::target_spec_for_trigger(id, &effect) != Some(TargetSpec::None)
            || trigger::unselected_trigger_modes(id, &effect).is_some()
        {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
        let event = match condition {
            ControllerGainsLife => json!({"event":"CapturedControllerGainsPositiveLife",
                "per_actual_committed_gain_event":true,"ignores_amount_as_trigger_multiplier":true}),
            ControllerAddedPlusOneCountersToSelf { max_per_turn } => {
                json!({"event":"ControllerPutsPositivePlusOneCountersOnOriginalSource",
                    "requires_original_battlefield_incarnation":true,"max_per_turn":max_per_turn,
                    "per_committed_counter_event":true,"consumed_when_trigger_captured":true})
            }
            ControllerFirstLifeGain { own_turn_only } => {
                json!({"event":"CapturedControllerFirstPositiveLifeGainOfActualTurn",
                    "requires_own_turn":own_turn_only,"earlier_gain_without_source_still_consumes_first":true,
                    "uses_event_time_source_incarnation_controller_and_active_player":true})
            }
            Etb => json!({"event":"OriginalPermanentEntersBattlefield"}),
            _ => return Err(Error::UnsupportedPrintedRoot(id)),
        };
        let resolution = match program {
            ExemplarCounter => json!({"operation":"PutPlusOnePlusOneCounters","count":1,
                "recipient":"OriginalBattlefieldSourceIncarnation","requires_recipient_still_present":true,
                "normal_counter_replacements_apply":true}),
            ExemplarDraw => json!({"operation":"DrawCards","count":1,"player":"CapturedController",
                "normal_draw_replacements_and_empty_library_loss_apply":true,
                "drawn_identity_private_to_authorized_viewers":true}),
            VanguardSurveil => {
                json!({"operation":"Surveil","count":1,"player":"CapturedController",
                "selection_is_not_targeting":true,"look_at_current_top_is_chooser_only":true,
                "options":[{"index":0,"operation":"KeepOriginalTopCardOnTop"},
                    {"index":1,"operation":"MoveOriginalTopCardToOwnersGraveyard"}],
                "empty_library_skips_choice":true,"does_not_draw_or_shuffle":true,
                "normal_zone_replacements_apply":true})
            }
            CollectorFood | CollectorCat => {
                let token = crate::card_def::card_id_by_name(
                    program
                        .token_name()
                        .ok_or(Error::UnsupportedPrintedRoot(id))?,
                )
                .ok_or(Error::UnsupportedPrintedRoot(id))?;
                let printing = if program == CollectorFood {
                    super::super::next_five::printing(token)?
                        .ok_or(Error::UnsupportedPrintedRoot(token))?
                } else {
                    super::baseline(token)?
                };
                json!({"operation":"CreateToken","count":1,"controller":"CapturedController",
                    "token_definition":printing,"normal_token_replacements_and_entry_triggers_apply":true})
            }
            Granted => return Err(Error::UnsupportedPrintedRoot(id)),
        };
        meanings.push(json!({"ordinal":ordinal,"program":program,"event":event,
            "source_zone":"Battlefield","source_face":0,"requires_active_printed_ability_at_capture":true,
            "mandatory":true,"uses_stack":true,"targets":[],"controller":"CapturedController",
            "source_departure_does_not_erase_captured_trigger":true,"resolution":resolution}));
    }
    Ok(Some(json!({"card_id":id,"full_pool_life_printing":{
        "schema":"printed-full-pool-life-trigger/v1","cost":cost,"mana_value":def.mana_value,
        "types":def.types,"subtypes":def.subtypes,"supertypes":[],"colors":def.colors,
        "power":def.power,"toughness":def.toughness,"keywords":def.keywords.0,"token":false,
        "ordinary_cast":{"origin":"OwnHand","timing":"OwnMainPhaseEmptyStackWithPriority",
            "ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],"targets":[],
            "resolution":"MoveOriginalSpellToBattlefieldUnderCasterControl",
            "countering_skips_entry_and_does_not_refund_cost":true,
            "normal_zone_replacements_and_entry_triggers_apply":true,
            "other_cast_permissions_require_current_candidate_authority":true},
        "ordered_triggers":meanings,
    }})))
}
