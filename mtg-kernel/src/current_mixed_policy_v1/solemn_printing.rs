//! Two separately captured triggers with independent optional resolutions.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, Keywords, Subtype, TargetSpec},
    effect::{EffectOp, LibraryCardFilter, ObjectRef, PlayerRef},
    mana::Cost,
    state::Zone,
    trigger::{TriggerCondition, TriggeredAbilityDef},
};
use serde_json::{json, Value};
pub(super) fn handles(name: &str) -> bool {
    name == "Solemn Simulacrum"
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let cost = Cost {
        generic: 4,
        ..Cost::zero()
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != 4
        || d.types != [CardType::Artifact, CardType::Creature]
        || d.subtypes != [Subtype::Golem]
        || !d.supertypes.is_empty()
        || !d.colors.is_empty()
        || d.power != Some(2)
        || d.toughness != Some(2)
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
    let optional = |instruction| EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: vec![EffectOp::Sequence(vec![]), instruction],
    };
    let entry = optional(EffectOp::SearchLibraryToBattlefieldTapped {
        player: PlayerRef::Controller,
        filter: LibraryCardFilter::BasicLand,
    });
    let death = optional(EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    });
    let [entry_trigger, death_trigger] = crate::trigger::triggers_for(id) else {
        return Err(Error::UnsupportedPrintedRoot(id));
    };
    for (trigger, condition, zone, program) in [
        (
            entry_trigger,
            TriggerCondition::Etb,
            Zone::Battlefield,
            &entry,
        ),
        (
            death_trigger,
            TriggerCondition::LeftBattlefieldToGraveyard,
            Zone::Graveyard,
            &death,
        ),
    ] {
        let TriggeredAbilityDef {
            condition: actual_condition,
            home_zone,
            face_index,
            intervening_if_kicked,
            intervening_if_controls_another_source_card,
            effect,
        } = trigger;
        if *actual_condition != condition
            || *home_zone != zone
            || *face_index != 0
            || *intervening_if_kicked
            || *intervening_if_controls_another_source_card
            || effect() != *program
            || crate::trigger::target_spec_for_trigger(id, program) != Some(TargetSpec::None)
            || crate::trigger::unselected_trigger_modes(id, program).is_some()
        {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
    }
    let entry_resolution = json!([{"operation":"ChooseWhetherToSearch","chooser":"CapturedTriggerController","at":"TriggerResolution","options":[
                {"index":0,"ordered_instructions":[],"searches":false,"shuffles":false,"preserves_library_order_knowledge_and_randomness":true},
                {"index":1,"ordered_instructions":[{"operation":"SearchOwnLibrary","player":"CapturedTriggerController","choice_visibility":"ChooserOnly","filter":"BasicLandCard","minimum":0,"maximum":1,"may_fail_to_find_even_when_matches_exist":true,"selection_is_not_targeting":true,"binds_original_live_selected_incarnation":true},
                    {"operation":"MoveSelectedCardToBattlefieldTappedIfAny","controller":"CapturedTriggerController","normal_zone_change_entry_replacements_and_triggers_apply":true,"selected_card_becomes_public":true},
                    {"operation":"ShuffleOwnLibrary","required_even_when_no_card_found":true}]}]}]);
    let death_resolution = json!([{"operation":"ChooseWhetherToDraw","chooser":"CapturedTriggerController","at":"TriggerResolution","options":[{"index":0,"ordered_instructions":[]},{"index":1,"ordered_instructions":[{"operation":"DrawCards","recipient":"CapturedTriggerController","count":1,"normal_draw_replacements_and_empty_library_rules_apply":true,"drawn_identity_is_visible_only_to_authorized_viewers":true}]}]}]);
    let casts = json!([{"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,"timing":"OwnMainPhaseEmptyStackWithPriority","targets":[],"ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],"ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],"countering_skips_entry_and_does_not_refund_cost":true,"countering_spell_creates_no_death_trigger":true,"normal_zone_replacements_and_entry_triggers_apply":true,"other_cast_permissions_require_current_candidate_authority":true}]);
    let triggers = json!([
        {"event":"ThisPermanentEntersBattlefield","home_zone":"Battlefield","source_face":0,"controller":"CapturedSourceControllerAtEntry","requires_active_printed_ability_at_capture":true,"mandatory_trigger":true,"uses_stack":true,"targets":[],"survives_source_departure_and_later_control_change":true,
        "ordered_resolution":entry_resolution},
        {"event":"OriginalSourceLeavesBattlefieldForGraveyard","home_zone":"Graveyard","source_face":0,"controller":"CapturedSourceControllerBeforeDeparture","requires_active_printed_ability_before_departure":true,"uses_departure_last_known_information":true,"mandatory_trigger":true,"uses_stack":true,"targets":[],"other_departure_destinations_do_not_trigger":true,"survives_original_source_leaving_graveyard":true,
        "ordered_resolution":death_resolution}
    ]);
    Ok(Some(json!({"card_id":id,"solemn_simulacrum_printing":{
        "schema":"printed-optional-entry-search-death-draw/v1","cost":cost,"mana_value":4,"types":d.types,"subtypes":d.subtypes,"supertypes":[],"colors":[],"power":2,"toughness":2,"keywords":0,"token":false,
        "ordered_cast_roots":casts,
        "ordered_triggers":triggers
    }})))
}
