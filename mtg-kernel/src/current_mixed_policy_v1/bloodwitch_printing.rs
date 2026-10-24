//! Exact targeted death observer; life gain is skipped if its only target is illegal.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, Keywords, Subtype, TargetSpec},
    effect::{EffectOp, ObjectRef, PlayerRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
    trigger::{TriggerCondition, TriggeredAbilityDef},
};
use serde_json::{json, Value};
pub(super) fn handles(name: &str) -> bool {
    name == "Vengeful Bloodwitch"
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let cost = Cost {
        generic: 1,
        pips: &[Pip::Colored(ManaColor::B)],
        x_count: 0,
    };
    let program = EffectOp::Sequence(vec![
        EffectOp::LoseLife {
            player: PlayerRef::Target(0),
            amount: 1,
        },
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 1,
        },
    ]);
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != 2
        || d.types != [CardType::Creature]
        || d.subtypes != [Subtype::Vampire, Subtype::Warlock]
        || !d.supertypes.is_empty()
        || d.colors != [ManaColor::B]
        || d.power != Some(1)
        || d.toughness != Some(1)
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
        intervening_if_kicked,
        intervening_if_controls_another_source_card,
        face_index,
        effect,
    } = trigger;
    if *condition != TriggerCondition::ThisOrControlledCreatureDies
        || *home_zone != Zone::Battlefield
        || *face_index != 0
        || *intervening_if_kicked
        || *intervening_if_controls_another_source_card
        || effect() != program
        || crate::trigger::target_spec_for_trigger(id, &program) != Some(TargetSpec::TargetOpponent)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let cast = json!({"method":"Normal","route":"Hand","origin":"OwnHand","timing":"OwnMainPhaseEmptyStackWithPriority",
        "ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],"targets":[],
        "ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
        "countering_skips_entry_and_does_not_refund_cost":true,"failed_or_countered_spell_destination":"OwnersGraveyard",
        "normal_zone_replacements_and_entry_triggers_apply":true,"other_cast_permissions_require_current_candidate_authority":true});
    let ordered = json!([{"operation":"LoseLife","target_slot":0,"amount":1,"is_damage":false,"normal_life_loss_rules_apply":true},
        {"operation":"GainLife","recipient":"CapturedTriggerController","amount":1,"does_not_require_successful_life_loss":true,
        "normal_life_gain_replacements_apply":true}]);
    let roots = json!([{"event":"CreatureDies","home_zone":"Battlefield","source_face":0,"requires_active_printed_abilities_at_capture":true,
        "filter":"ThisSourceOrCreatureControlledBySourceControllerImmediatelyBeforeDeath",
        "requires_battlefield_to_graveyard_transition":true,"uses_effective_creature_type_and_previous_controller":true,
        "tokens_count":true,"each_matching_death_is_a_separate_occurrence":true,
        "source_dying_in_same_batch_still_observes_all_matching_deaths":true,"intervening_if":null,
        "controller":"CapturedSourceControllerBeforeDeathBatch","captures_original_source_incarnation":true,
        "survives_source_departure_and_later_control_change":true,"mandatory":true,"uses_stack":true,
        "targets":[{"slot":0,"kind":"Player","relation":"OpponentOfCapturedTriggerController","minimum":1,"maximum":1,
            "normal_target_legality_and_protection_apply":true,"rechecked_at_resolution":true}],
        "all_targets_illegal_skips_entire_ordered_resolution_including_life_gain":true,
        "ordered_resolution":ordered,"no_priority_between_resolution_instructions":true}]);
    Ok(Some(json!({"card_id":id,"bloodwitch_printing":{
        "schema":"printed-targeted-death-observer/v1","cost":cost,"mana_value":2,"types":d.types,"subtypes":d.subtypes,
        "supertypes":[],"colors":d.colors,"power":1,"toughness":1,"keywords":0,"token":false,
        "ordered_cast_roots":[cast],"ordered_triggers":roots}})))
}
