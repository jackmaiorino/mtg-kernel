//! Two complete printed routes to the same private surveil-one instruction.
use super::{Error, Result};
use crate::{
    card_def::{
        ActivationTargetFilter, CardDef, CardType, CostComponent, Keywords, Subtype, TargetSpec,
    },
    effect::{EffectOp, ObjectRef, PlayerRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
    trigger::{TriggerCondition, TriggeredAbilityDef},
};
use serde_json::{json, Value};
pub(super) fn handles(name: &str) -> bool {
    matches!(name, "Wary Thespian" | "Rune-Sealed Wall")
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let wall = d.name == "Rune-Sealed Wall";
    let (cost, types, subtypes, colors, power, toughness, keywords) = if wall {
        (
            Cost {
                generic: 2,
                pips: &[Pip::Colored(ManaColor::U)],
                x_count: 0,
            },
            vec![CardType::Artifact, CardType::Creature],
            vec![Subtype::Wall],
            vec![ManaColor::U],
            0,
            6,
            Keywords::DEFENDER,
        )
    } else {
        (
            Cost {
                generic: 1,
                pips: &[Pip::Colored(ManaColor::G)],
                x_count: 0,
            },
            vec![CardType::Creature],
            vec![Subtype::Cat, Subtype::Druid],
            vec![ManaColor::G],
            3,
            1,
            Keywords::NONE,
        )
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != if wall { 3 } else { 2 }
        || d.types != types
        || d.subtypes != subtypes
        || d.colors != colors
        || !d.supertypes.is_empty()
        || d.power != Some(power)
        || d.toughness != Some(toughness)
        || d.keywords != keywords
        || d.target_spec != TargetSpec::None
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
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
    let program = EffectOp::Surveil {
        player: PlayerRef::Controller,
        count: 1,
    };
    if wall {
        let [ability] = d.activated_abilities else {
            return Err(Error::UnsupportedPrintedRoot(id));
        };
        if !crate::trigger::triggers_for(id).is_empty()
            || ability.cost != [CostComponent::Tap]
            || ability.target_spec != TargetSpec::None
            || ability.activation_zone != Zone::Battlefield
            || ability.sorcery_speed_only
            || ability.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
            || ability.max_activations_per_turn.is_some()
            || ability.face.is_some()
            || (ability.effect)() != program
        {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
    } else {
        if !d.activated_abilities.is_empty() {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
        let [entry, death] = crate::trigger::triggers_for(id) else {
            return Err(Error::UnsupportedPrintedRoot(id));
        };
        for (trigger, condition, zone) in [
            (entry, TriggerCondition::Etb, Zone::Battlefield),
            (
                death,
                TriggerCondition::LeftBattlefieldToGraveyard,
                Zone::Graveyard,
            ),
        ] {
            let TriggeredAbilityDef {
                condition: actual,
                home_zone,
                face_index,
                intervening_if_kicked,
                intervening_if_controls_another_source_card,
                effect,
            } = trigger;
            if *actual != condition
                || *home_zone != zone
                || *face_index != 0
                || *intervening_if_kicked
                || *intervening_if_controls_another_source_card
                || effect() != program
                || crate::trigger::target_spec_for_trigger(id, &program) != Some(TargetSpec::None)
                || crate::trigger::unselected_trigger_modes(id, &program).is_some()
            {
                return Err(Error::UnsupportedPrintedRoot(id));
            }
        }
    }
    let surveil = json!({"operation":"Surveil","count":1,"player":"CapturedAbilityController","selection_is_not_targeting":true,
        "ordered_instructions":[{"operation":"LookAtTopCards","maximum":1,"clamp_to_current_library_size":true,"visibility":"ChooserOnly","capture_original_live_card_incarnation":true},
            {"operation":"ChoosePlacement","chooser":"CapturedAbilityController","options":[{"index":0,"operation":"KeepOriginalTopCardOnTop"},{"index":1,"operation":"MoveOriginalTopCardToOwnersGraveyard","normal_zone_replacements_apply":true}]},
            {"operation":"KeepAnyUnmovedCardInItsOriginalLibraryOrder"}],
        "empty_library_skips_choice_without_drawing_or_losing":true,"not_a_draw":true,"not_a_search":true,"does_not_shuffle":true,
        "public_graveyard_move_reveals_only_the_moved_card":true,"source_departure_or_later_control_change_does_not_remove_captured_ability":true});
    let mut descriptor = json!({"schema":"printed-private-surveil-one/v1","cost":cost,"mana_value":d.mana_value,"types":d.types,"subtypes":d.subtypes,"supertypes":[],"colors":d.colors,"power":power,"toughness":toughness,"keywords":keywords.0,"token":false,
        "ordered_cast_roots":[{"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,"timing":"OwnMainPhaseEmptyStackWithPriority","targets":[],"ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],"ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],"countering_skips_entry_and_does_not_refund_cost":true,"countered_spell_creates_no_death_trigger":true,"normal_zone_replacements_and_entry_triggers_apply":true,"other_cast_permissions_require_current_candidate_authority":true}],
        "ordered_activated_roots":[],"ordered_triggers":[],"ordered_static_roots":[]});
    if wall {
        descriptor["ordered_static_roots"] =
            json!([{"keyword":"Defender","normal_defender_attack_restriction_applies":true}]);
        descriptor["ordered_activated_roots"] = json!([{"ability_index":0,"source_zone":"Battlefield","requires_active_printed_ability":true,"timing":"WithPriorityOnEitherPlayersTurn","uses_stack":true,"targets":[],"controller":"CapturedAbilityController","ordered_costs":[{"operation":"TapOriginalAbilitySource","requires_untapped":true,"normal_tap_cost_summoning_sickness_and_haste_rules_apply":true}],"no_mana_payment":true,"printed_activation_limit":null,"all_costs_paid_before_ability_publication":true,"failed_or_countered_ability_does_not_refund_costs":true,"ordered_resolution":[surveil]}]);
    } else {
        descriptor["ordered_triggers"] = json!([
            {"event":"ThisPermanentEntersBattlefield","home_zone":"Battlefield","source_face":0,"controller":"CapturedSourceControllerAtEntry","requires_active_printed_ability_at_capture":true,"mandatory_trigger":true,"uses_stack":true,"targets":[],"ordered_resolution":[surveil]},
            {"event":"OriginalSourceLeavesBattlefieldForGraveyard","home_zone":"Graveyard","source_face":0,"controller":"CapturedSourceControllerBeforeDeparture","requires_active_printed_ability_before_departure":true,"uses_departure_last_known_information":true,"other_departures_do_not_trigger":true,"mandatory_trigger":true,"uses_stack":true,"targets":[],"ordered_resolution":[surveil]}]);
    }
    Ok(Some(
        json!({"card_id":id,"surveil_one_printing":descriptor}),
    ))
}
