//! Complete mill/loot triggers and continuously recomputed threshold statics.
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
    matches!(name, "Billowing Shriekmass" | "Dreadwing Scavenger")
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let shriek = d.name == "Billowing Shriekmass";
    let cost = if shriek {
        Cost {
            generic: 3,
            pips: &[Pip::Colored(ManaColor::B)],
            x_count: 0,
        }
    } else {
        Cost {
            generic: 1,
            pips: &[Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::B)],
            x_count: 0,
        }
    };
    let colors = if shriek {
        &[ManaColor::B][..]
    } else {
        &[ManaColor::U, ManaColor::B][..]
    };
    let subtypes = if shriek {
        &[Subtype::Spirit][..]
    } else {
        &[Subtype::Nightmare, Subtype::Bird][..]
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != if shriek { 4 } else { 3 }
        || d.types != [CardType::Creature]
        || d.subtypes != subtypes
        || d.colors != colors
        || !d.supertypes.is_empty()
        || d.power != Some(2)
        || d.toughness != Some(if shriek { 3 } else { 2 })
        || d.keywords != Keywords::FLYING
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
        || crate::engine::GRAVEYARD_THRESHOLD_MINIMUM_V1 != 7
        || crate::engine::static_graveyard_threshold_self_boost_for(d.name)
            != Some(if shriek { (2, 1) } else { (1, 1) })
        || crate::engine::static_graveyard_threshold_keyword_for(d.name)
            != if shriek {
                None
            } else {
                Some((7, Keywords::DEATHTOUCH))
            }
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let program = if shriek {
        EffectOp::MillCards {
            player: PlayerRef::Controller,
            count: 3,
        }
    } else {
        EffectOp::Sequence(vec![
            EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            },
            EffectOp::DiscardCards {
                player: PlayerRef::Controller,
                count: 1,
            },
        ])
    };
    let expected = if shriek {
        vec![TriggerCondition::Etb]
    } else {
        vec![TriggerCondition::Etb, TriggerCondition::Attacks]
    };
    let triggers = crate::trigger::triggers_for(id);
    if triggers.len() != expected.len() {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    for (trigger, condition) in triggers.iter().zip(expected) {
        let TriggeredAbilityDef {
            condition: actual,
            home_zone,
            face_index,
            intervening_if_kicked,
            intervening_if_controls_another_source_card,
            effect,
        } = trigger;
        if *actual != condition
            || *home_zone != Zone::Battlefield
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
    let resolution = if shriek {
        json!([{"operation":"MillCards","player":"CapturedTriggerController","count":3,"ordered_instructions":[{"operation":"TakeTopAvailableCards","maximum":3,"capture_exact_original_library_prefix_incarnations":true},{"operation":"ChooseGraveyardOrderOfAllCapturedCards","chooser":"PhysicalCardOwner","visibility":"ChooserOnlyBeforeMoves","minimum":"AllCapturedCards","maximum":"AllCapturedCards","selection_is_not_targeting":true,"cannot_choose_a_subset":true,"forced_final_card_needs_no_extra_action":true},{"operation":"MoveThoseCardsToOwnersGraveyard","normal_zone_replacements_apply":true,"chosen_order_is_preserved":true}],"no_zone_moves_before_ordering_completes":true,"short_or_empty_library_moves_only_available_cards":true,"not_a_draw":true,"does_not_set_empty_library_draw_loss":true,"does_not_shuffle":true,"public_zone_moves_reveal_moved_cards":true}])
    } else {
        json!([
        {"operation":"DrawCards","recipient":"CapturedTriggerController","count":1,"normal_draw_replacements_and_empty_library_rules_apply":true,"drawn_identity_visible_only_to_authorized_viewers":true},
        {"operation":"DiscardCards","player":"CapturedTriggerController","requested_count":1,"clamp_to_available_hand":true,"private_choice_by_controller":true,"may_discard_just_drawn_card":true,"not_a_cast_or_activation_cost":true,"normal_discard_replacements_and_triggers_apply":true,"continues_after_an_unsuccessful_draw":true}])
    };
    let entry = json!({"event":"ThisPermanentEntersBattlefield","home_zone":"Battlefield","source_face":0,"controller":"CapturedSourceControllerAtEntry","requires_active_printed_ability_at_capture":true,"mandatory_trigger":true,"uses_stack":true,"targets":[],"source_departure_or_later_control_change_does_not_remove_trigger":true,"ordered_resolution":resolution});
    let mut trigger_meanings = vec![entry];
    if !shriek {
        trigger_meanings.push(json!({"event":"ThisSourceIsDeclaredAsAttacker","requires_actual_attack_declaration":true,"entering_attacking_does_not_trigger":true,"trigger_count_per_declaration":1,"home_zone":"Battlefield","source_face":0,"controller":"CapturedSourceControllerAtAttack","requires_active_printed_ability_at_capture":true,"mandatory_trigger":true,"uses_stack":true,"targets":[],"source_departure_does_not_remove_trigger":true,"ordered_resolution":resolution}));
    }
    let statics = json!([{"operation":"ContinuousThresholdSelfCharacteristics","source_zone":"Battlefield","requires_active_printed_source_abilities":true,
        "condition":{"operation":"CurrentControllerGraveyardPhysicalCardCountAtLeast","minimum":7,"all_card_types_count":true,"tokens_do_not_count":true},
        "power_delta":if shriek{2}else{1},"toughness_delta":1,"keyword_bits":if shriek{0}else{Keywords::DEATHTOUCH.0},"power_toughness_layer":"7c","places_no_counters":true,"is_not_a_trigger":true,"uses_no_stack":true,"continuously_recheck_current_graveyard_and_control":true,"current_effective_copied_printing_selects_rule":true,"normal_continuous_layers_and_printed_ability_suppression_apply":true,"ends_when_source_leaves_battlefield":true}]);
    let casts = json!([{"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,"timing":"OwnMainPhaseEmptyStackWithPriority","targets":[],"ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],"ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],"countering_skips_entry_and_does_not_refund_cost":true,"normal_zone_replacements_and_entry_triggers_apply":true,"other_cast_permissions_require_current_candidate_authority":true}]);
    Ok(Some(
        json!({"card_id":id,"threshold_loot_printing":{"schema":"printed-mill-or-loot-with-threshold/v1","cost":cost,"mana_value":d.mana_value,"types":d.types,"subtypes":d.subtypes,"supertypes":[],"colors":d.colors,"power":2,"toughness":d.toughness,"keywords":Keywords::FLYING.0,"token":false,"ordered_cast_roots":casts,"ordered_triggers":trigger_meanings,"ordered_static_roots":statics}}),
    ))
}
