//! Closed simultaneous graveyard-return spell meanings.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, Keywords, TargetSpec},
    effect::EffectOp,
    mana::{Cost, ManaColor, Pip},
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(name, "Raise the Past" | "Rise of the Dark Realms")
}

/// All remaining CardDef fields pass the caller's shared exclusion guard.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let rise = d.name == "Rise of the Dark Realms";
    let cost = Cost {
        generic: if rise { 7 } else { 2 },
        pips: if rise {
            &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::B)]
        } else {
            &[Pip::Colored(ManaColor::W), Pip::Colored(ManaColor::W)]
        },
        x_count: 0,
    };
    let color = if rise { ManaColor::B } else { ManaColor::W };
    let program = if rise {
        EffectOp::ReturnAllGraveyardCreaturesUnderController
    } else {
        EffectOp::ReturnOwnGraveyardCreaturesManaValueAtMost { max_mana_value: 2 }
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || usize::from(d.mana_value) != usize::from(cost.generic) + cost.pips.len()
        || d.types != [CardType::Sorcery]
        || !d.subtypes.is_empty()
        || !d.supertypes.is_empty()
        || d.colors != [color]
        || d.power.is_some()
        || d.toughness.is_some()
        || d.keywords != Keywords::NONE
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || d.target_spec != TargetSpec::None
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)() != Some(program)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let cast_resolution = json!([{"operation":"ReturnCurrentGraveyardCreatureCardSetToBattlefield",
                "graveyards":if rise {"BothPlayers"} else {"CapturedSpellController"},
                "filter":{"type":"CreatureCard","characteristics":"CurrentGraveyardCardCharacteristics",
                    "mana_value_at_most":if rise {Value::Null} else {json!(2)},
                    "mana_value_basis":"PrintedCardManaValueInGraveyard",
                    "exclude_tokens_and_virtual_spell_copies":true,
                    "graveyard_vector_and_current_zone_incarnation_must_agree":true},
                "select_set_once_at_resolution":true,"includes_cards_entering_graveyard_after_cast":true,
                "all_eligible_cards_mandatory":true,"does_not_target":true,
                "battlefield_controller":"CapturedSpellController","original_card_owner_preserved":true,
                "entry_batch":"SimultaneousBeforeEntryTriggerCollection",
                "all_returned_cards_are_new_battlefield_incarnations":true,
                "ordinary_zone_change_reset_of_counters_tapped_state_and_summoning_sickness":true,
                "normal_entry_replacements_trigger_collection_and_state_based_actions_apply":true}]);
    let cast_roots = json!([{"method":"Normal","route":"Hand","origin":"OwnHand",
            "timing":"OwnMainPhaseEmptyStackWithPriority","ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],
            "targets":[],"ordered_resolution":cast_resolution,
            "normal_trigger_order_and_resolution_follow_completed_entry_batch":true,
            "physical_spell_destination_after_resolution_or_counter":"OwnersGraveyard",
            "virtual_spell_copy_departure":"CeaseWithoutEnteringCardZone",
            "countering_skips_all_resolution_instructions":true,"countering_does_not_refund_cost":true,
            "other_cast_permissions_require_current_candidate_authority":true,
            "normal_zone_replacements_apply":true}]);
    Ok(Some(json!({"card_id":id,"graveyard_batch_spell_printing":{
        "schema":"printed-graveyard-batch-spell/v1","cost":cost,"mana_value":d.mana_value,
        "types":d.types,"subtypes":[],"supertypes":[],"colors":d.colors,"power":null,
        "toughness":null,"keywords":0,"token":false,
        "ordered_cast_roots":cast_roots
    }})))
}
