//! Closed source-return and controlled-creature counter activations.
use super::{Error, Result};
use crate::{
    card_def::{
        ActivationTargetFilter, CardDef, CardType, CostComponent, Keywords, Subtype, TargetSpec,
    },
    effect::{EffectOp, ObjectRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(name, "Reassembling Skeleton" | "Treetop Snarespinner")
}

/// The caller checks all fields unrelated to these exact printed abilities.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let skeleton = d.name == "Reassembling Skeleton";
    let color = if skeleton { ManaColor::B } else { ManaColor::G };
    let pips = if skeleton {
        &[Pip::Colored(ManaColor::B)][..]
    } else {
        &[Pip::Colored(ManaColor::G)][..]
    };
    let cost = Cost {
        generic: if skeleton { 1 } else { 3 },
        pips,
        x_count: 0,
    };
    let payment = Cost {
        generic: if skeleton { 1 } else { 2 },
        pips,
        x_count: 0,
    };
    let keywords = if skeleton {
        Keywords::NONE
    } else {
        Keywords::REACH | Keywords::DEATHTOUCH
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.mana_value != if skeleton { 2 } else { 4 }
        || d.types != [CardType::Creature]
        || d.subtypes
            != if skeleton {
                &[Subtype::Skeleton, Subtype::Warrior][..]
            } else {
                &[Subtype::Spider][..]
            }
        || !d.supertypes.is_empty()
        || d.colors != [color]
        || d.power != Some(1)
        || d.toughness != Some(if skeleton { 1 } else { 4 })
        || d.keywords != keywords
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || d.target_spec != TargetSpec::None
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || !crate::trigger::triggers_for(id).is_empty()
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let [ability] = d.activated_abilities else {
        return Err(Error::UnsupportedPrintedRoot(id));
    };
    let program = if skeleton {
        EffectOp::ReturnAbilitySourceFromGraveyard { tapped: true }
    } else {
        EffectOp::AddCountersToTarget {
            target_index: 0,
            optional: false,
            plus1_plus1: 1,
            lifelink: 0,
            stun: 0,
        }
    };
    if ability.cost != [CostComponent::Mana(payment)]
        || ability.target_spec
            != if skeleton {
                TargetSpec::None
            } else {
                TargetSpec::ControlledCreature
            }
        || ability.activation_zone
            != if skeleton {
                Zone::Graveyard
            } else {
                Zone::Battlefield
            }
        || ability.sorcery_speed_only == skeleton
        || ability.activation_target_filter != ActivationTargetFilter::TargetSpecOnly
        || ability.max_activations_per_turn.is_some()
        || ability.face.is_some()
        || (ability.effect)() != program
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let targets = if skeleton {
        json!([])
    } else {
        json!([{"slot":0,"minimum":1,"maximum":1,"chooser":"ActivatingPlayer",
            "filter":{"zone":"Battlefield","effective_type":"Creature","controller":"ActivatingPlayer","source_itself_allowed":true},
            "selected_before_cost_payment":true,"normal_target_legality_and_protection_apply":true,
            "rechecked_at_resolution":true,"binding":"OriginalSelectedBattlefieldIncarnation"}])
    };
    let resolution = if skeleton {
        json!([{"operation":"ReturnOriginalAbilitySourceFromGraveyardToBattlefieldTapped",
            "binding":"CapturedGraveyardIncarnation","requires_same_source_still_in_graveyard":true,
            "no_target":true,"controller":"OriginalCardOwner","creates_new_battlefield_incarnation":true,
            "normal_zone_replacements_and_entry_triggers_apply":true,
            "missing_exiled_or_returned_then_dead_source_is_not_returned":true}])
    } else {
        json!([{"operation":"PutPlusOnePlusOneCounterOnOriginalLegalTarget","count":1,
            "requires_original_target_still_controlled_creature":true,"normal_counter_replacements_and_triggers_apply":true,
            "counter_has_no_cleanup_expiration":true,"source_departure_does_not_prevent_counter":true}])
    };
    Ok(Some(
        json!({"card_id":id,"counter_return_creature_printing":{
            "schema":"printed-counter-return-creature/v1","cost":cost,"mana_value":d.mana_value,
            "types":d.types,"subtypes":d.subtypes,"supertypes":[],"colors":d.colors,
            "power":d.power,"toughness":d.toughness,"keywords":keywords.0,"token":false,
            "ordered_cast_roots":[{"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,
                "timing":"OwnMainPhaseEmptyStackWithPriority","targets":[],
                "ordered_costs":[{"operation":"PayNativeAdjustedPrintedMana","printed_cost":cost}],
                "ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
                "failed_or_countered_spell_destination":"OwnersGraveyard","countering_does_not_refund_cost":true,
                "normal_zone_replacements_and_entry_triggers_apply":true,"other_cast_permissions_require_current_candidate_authority":true}],
            "ordered_activated_roots":[{"ability_index":0,"source_zone":ability.activation_zone,
                "requires_active_printed_ability":true,"timing":if skeleton {"WithPriorityOnEitherPlayersTurn"} else {"OwnMainPhaseEmptyStackWithPriority"},
                "uses_stack":true,"controller":"CapturedAbilityControllerAtActivation","targets":targets,
                "ordered_costs":[{"operation":"PayMana","cost":payment}],"tap_cost":false,
                "summoning_sickness_does_not_prohibit_activation":true,"printed_activation_limit":null,
                "each_activation_is_independently_paid_before_publication":true,
                "ordered_resolution":resolution,"source_departure_does_not_remove_published_ability":true,
                "illegal_target_or_missing_original_source_skips_affected_resolution":true,
                "failed_or_countered_ability_does_not_refund_cost":true}]
        }}),
    ))
}
