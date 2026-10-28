//! Exact targeted entry exile: one chosen card or a player's current graveyard.
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
    matches!(name, "Ambush Wolf" | "Angel of Finality")
}

/// All unrelated CardDef fields remain subject to the caller's common guard.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let wolf = d.name == "Ambush Wolf";
    let (color, subtypes, power, toughness, keywords, target_spec) = if wolf {
        (
            ManaColor::G,
            &[Subtype::Wolf][..],
            4,
            2,
            Keywords::FLASH,
            TargetSpec::UpToOneCardInGraveyards,
        )
    } else {
        (
            ManaColor::W,
            &[Subtype::Angel][..],
            3,
            4,
            Keywords::FLYING,
            TargetSpec::AnyPlayer,
        )
    };
    let cost = Cost {
        generic: if wolf { 2 } else { 3 },
        pips: if wolf {
            &[Pip::Colored(ManaColor::G)]
        } else {
            &[Pip::Colored(ManaColor::W)]
        },
        x_count: 0,
    };
    let program = if wolf {
        EffectOp::MoveAllTargets {
            to_zone: Zone::Exile,
        }
    } else {
        EffectOp::ExilePlayersGraveyard {
            player: PlayerRef::Target(0),
        }
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != if wolf { 3 } else { 4 }
        || d.types != [CardType::Creature]
        || d.subtypes != subtypes
        || !d.supertypes.is_empty()
        || d.colors != [color]
        || d.power != Some(power)
        || d.toughness != Some(toughness)
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
        || crate::trigger::target_spec_for_trigger(id, &program) != Some(target_spec)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let cast = json!({"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,
        "timing":if wolf {"InstantWithPriority"} else {"OwnMainPhaseEmptyStackWithPriority"},
        "ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],"targets":[],
        "ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
        "countering_skips_entry_and_does_not_refund_cost":true,"countered_spell_destination":"OwnersGraveyard",
        "normal_zone_replacements_and_entry_triggers_apply":true,"other_cast_permissions_require_current_candidate_authority":true});
    let (target, resolution) = if wolf {
        (
            json!({"slot":0,"kind":"Object","minimum":0,"maximum":1,
                "filter":{"kind":"Card","zone":"AnyPlayersGraveyard","owner_relation":"Any","card_types":"Any"},
                "binding":"OriginalSelectedObjectIncarnation","rechecked_at_resolution":true}),
            json!({"operation":"ExileEachLegalChosenCard","target_slot":0,
                "requires_original_graveyard_incarnation":true,"does_not_select_a_replacement_card":true,
                "zero_chosen_targets_perform_no_moves":true,"normal_zone_replacements_apply":true}),
        )
    } else {
        (
            json!({"slot":0,"kind":"Player","minimum":1,"maximum":1,
                "relation":"AnyPlayerIncludingCapturedTriggerController","graveyard_may_be_empty":true,
                "binding":"SelectedPlayerSeat","rechecked_at_resolution":true}),
            json!({"operation":"ExileAllCardsFromTargetPlayersGraveyard","player_target_slot":0,
                "read_graveyard_contents_at":"Resolution","includes_cards_arriving_after_target_selection":true,
                "simultaneous_zone_change_batch":true,
                "individual_graveyard_cards_are_not_targets":true,"preserves_other_players_graveyard":true,
                "normal_zone_replacements_apply":true}),
        )
    };
    let trigger = json!({"event":"ThisPermanentEntersBattlefield","home_zone":"Battlefield","source_face":0,
        "requires_active_printed_abilities_at_capture":true,"intervening_if":null,"mandatory":true,"uses_stack":true,
        "controller":"CapturedSourceControllerAtEntry","chooser":"CapturedTriggerController",
        "selected_at":"TriggerStackPlacement","captures_original_source_incarnation":true,
        "survives_source_departure_and_later_control_change":true,"targets":[target],
        "zero_targets_allowed":wolf,"zero_targets_perform_no_moves":wolf,
        "required_target_absence_prevents_stack_placement":!wolf,"normal_target_legality_and_protection_apply":true,
        "all_chosen_targets_illegal_skips_resolution":true,"ordered_resolution":[resolution]});
    Ok(Some(json!({"card_id":id,"entry_exile_printing":{
        "schema":"printed-entry-exile/v1","cost":cost,"mana_value":d.mana_value,
        "types":d.types,"subtypes":d.subtypes,"supertypes":[],"colors":d.colors,
        "power":power,"toughness":toughness,"keywords":keywords.0,"token":false,
        "ordered_cast_roots":[cast],"ordered_triggers":[trigger]}})))
}
