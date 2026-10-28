//! Complete entry surveil plus the independently printed prowess/threshold root.
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
    matches!(name, "Lightshell Duo" | "Cephalid Inkmage")
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let duo = d.name == "Lightshell Duo";
    let cost = Cost {
        generic: if duo { 3 } else { 2 },
        pips: &[Pip::Colored(ManaColor::U)],
        x_count: 0,
    };
    let subtypes = if duo {
        &[Subtype::Rat, Subtype::Otter][..]
    } else {
        &[Subtype::Octopus, Subtype::Wizard][..]
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != if duo { 4 } else { 3 }
        || d.types != [CardType::Creature]
        || d.subtypes != subtypes
        || !d.supertypes.is_empty()
        || d.colors != [ManaColor::U]
        || d.power != Some(if duo { 3 } else { 2 })
        || d.toughness != Some(if duo { 4 } else { 2 })
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
        || crate::engine::static_graveyard_threshold_keyword_for(d.name)
            != if duo {
                None
            } else {
                Some((7, Keywords::CANT_BE_BLOCKED))
            }
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let count = if duo { 2 } else { 3 };
    let mut expected = vec![(
        TriggerCondition::Etb,
        EffectOp::Surveil {
            player: PlayerRef::Controller,
            count,
        },
    )];
    if duo {
        expected.push((
            TriggerCondition::CastNoncreatureSpell,
            EffectOp::BindTemporaryBoostToTriggerSource {
                power: 1,
                toughness: 1,
            },
        ));
    }
    let triggers = crate::trigger::triggers_for(id);
    if triggers.len() != expected.len() {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    for (trigger, (condition, program)) in triggers.iter().zip(expected) {
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
    let resolution = json!([{"operation":"Surveil","count":count,"player":"CapturedTriggerController","not_a_draw":true,"not_a_search":true,"does_not_shuffle":true,
        "ordered_instructions":[{"operation":"PrivatelyLookAtTopCards","maximum":count,"clamp_to_available_card_count":true,"visibility":"ChooserOnly","capture_original_live_card_incarnations":true},
            {"operation":"ChooseCardsForGraveyardAndTheirOrder","minimum":0,"maximum":"NumberActuallyLookedAt","chooser":"CapturedTriggerController","selection_is_not_targeting":true,"may_choose_none_some_or_all":true,"original_objects_cannot_be_repeated":true},
            {"operation":"ChooseOrderOfAllRemainingLookedAtCards","chooser":"CapturedTriggerController","minimum":"AllRemainingCards","maximum":"AllRemainingCards","skip_choice_when_zero_or_one_remains":true},
            {"operation":"MoveChosenCardsToOwnersGraveyardInChosenOrder","normal_zone_replacements_apply":true,"reveal_only_cards_actually_moved_to_public_zone":true},
            {"operation":"ReturnRemainingLookedAtCardsToTopInChosenOrder","preserve_unlooked_at_tail_order":true}],
        "no_card_moves_before_complete_native_selection_and_ordering":true,"empty_library_skips_without_drawing_or_losing":true,
        "opponent_projection_hides_private_candidates_subsets_and_orders":true,"source_departure_does_not_remove_captured_ability":true}]);
    let mut trigger_meanings = vec![
        json!({"event":"ThisPermanentEntersBattlefield","home_zone":"Battlefield","source_face":0,"controller":"CapturedSourceControllerAtEntry","requires_active_printed_ability_at_capture":true,"mandatory_trigger":true,"uses_stack":true,"targets":[],"ordered_resolution":resolution}),
    ];
    let mut statics = vec![];
    if duo {
        let boost = json!({"operation":"ModifyOriginalTriggerSource","binds_original_battlefield_incarnation_at_trigger_capture":true,"same_incarnation_must_still_exist_at_resolution":true,"power_delta":1,"toughness_delta":1,"places_no_counters":true,"duration":"UntilCleanupExpirationAfterDiscard","normal_continuous_layers_and_timestamps_apply":true,"later_incarnations_are_not_modified":true});
        trigger_meanings.push(json!({"event":"SourceControllerCastsNoncreatureSpell","home_zone":"Battlefield","source_face":0,"requires_active_printed_ability_at_capture":true,"controller":"CapturedSourceControllerAtCast","uses_current_spell_characteristics":true,"copies_without_cast_do_not_count":true,"trigger_count_per_cast":1,"mandatory_trigger":true,"uses_stack":true,"targets":[],"countering_the_spell_does_not_remove_trigger":true,"ordered_resolution":[boost]}));
    } else {
        statics.push(json!({"operation":"GrantKeywordToThisSource","keyword_bits":Keywords::CANT_BE_BLOCKED.0,"requires_active_printed_source_ability":true,"source_zone":"Battlefield","condition":{"operation":"CurrentControllerGraveyardPhysicalCardCountAtLeast","minimum":7,"all_card_types_count":true,"counts_no_tokens":true},"continuously_recheck_current_graveyard_and_controller":true,"ability_loss_disables_this_grant":true,"current_effective_copied_printing_selects_rule":true,"no_stack_or_trigger":true}));
    }
    let casts = json!([{"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,"timing":"OwnMainPhaseEmptyStackWithPriority","targets":[],"ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],"ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],"countering_skips_entry_and_does_not_refund_cost":true,"normal_zone_replacements_and_entry_triggers_apply":true,"other_cast_permissions_require_current_candidate_authority":true}]);
    Ok(Some(
        json!({"card_id":id,"surveil_many_printing":{"schema":"printed-entry-surveil-and-independent-root/v1","cost":cost,"mana_value":d.mana_value,"types":d.types,"subtypes":d.subtypes,"supertypes":[],"colors":d.colors,"power":d.power,"toughness":d.toughness,"keywords":0,"token":false,"ordered_cast_roots":casts,"ordered_triggers":trigger_meanings,"ordered_static_roots":statics}}),
    ))
}
