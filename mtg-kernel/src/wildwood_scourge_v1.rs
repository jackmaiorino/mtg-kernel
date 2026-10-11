//! Definition-owned Wildwood Scourge preparation, not FDN registration.
use crate::card_def::{CardType, Subtype};
use crate::effect::EffectOp;
use crate::event::CommittedEvent;
use crate::ids::{ObjectId, PlayerId};
use crate::state::{GameState, Zone};

// Referenced by the named build recipe after root-owned registration.
#[allow(dead_code)]
pub(crate) fn spell_program_v1() -> EffectOp {
    EffectOp::PutSourceOntoBattlefieldWithXPlusOneCounters
}
pub(crate) fn counter_program_v1() -> EffectOp {
    EffectOp::BindPlusOnePlusOneCounterToTriggerSource
}

/// Read the actual recipient, not the identity of the player placing counters.
/// Called before the next instruction can change its controller, type or zone.
pub(crate) fn matches_counter_event(
    state: &GameState,
    source: ObjectId,
    controller: PlayerId,
    event: &CommittedEvent,
) -> bool {
    let CommittedEvent::PlusOneCountersAdded {
        object,
        zone_change_count,
        count,
        ..
    } = event
    else {
        return false;
    };
    *count > 0
        && *object != source
        && state.objects.try_get(*object).is_some_and(|live| {
            live.zone == Zone::Battlefield
                && live.zone_change_count == *zone_change_count
                && live.controller == controller
                && crate::engine::object_has_type(state, *object, CardType::Creature)
                && !crate::engine::has_effective_subtype(state, *object, Subtype::Hydra)
        })
}

/// Share the engine's existing waiting-trigger transport. No SBA or placement
/// occurs here; ordinary checkpoints retain APNAP and controller ordering.
/// The new condition is excluded from later event-batch collection, so a
/// captured trigger cannot duplicate or be reinterpreted after another op.
pub(crate) fn capture_counter_event(state: &mut GameState, event: &CommittedEvent) {
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    {
        use crate::card_def::TargetSpec;
        use crate::state::AbilitySourceContractV4;
        use crate::trigger::{PendingTrigger, TriggerCondition};
        let captures = state
            .objects
            .iter()
            .filter_map(|(id, object)| {
                if object.zone != Zone::Battlefield
                    || object.v4.face_index != 0
                    || !crate::continuous_characteristics_v1::printed_abilities_active(state, id)
                {
                    return None;
                }
                let definition = crate::card_def::CARD_DEFS.get(usize::from(object.card_def))?;
                if !definition.is_executable() || definition.name != "Wildwood Scourge" {
                    return None;
                }
                let [ability] = crate::trigger::triggers_for(object.card_def) else {
                    return None;
                };
                if ability.condition
                    != TriggerCondition::AnotherControlledNonHydraCreatureGetsPlusOneCounters
                    || !matches_counter_event(state, id, object.controller, event)
                {
                    return None;
                }
                let contract = AbilitySourceContractV4::capture(state, id);
                Some(PendingTrigger {
                    source: id,
                    controller: contract.controller,
                    effect: crate::trigger::materialize_trigger_effect(ability, id, state),
                    is_madness_offer: false,
                    kicked: false,
                    target_spec: TargetSpec::None,
                    targets: Vec::new(),
                    target_contracts: Vec::new(),
                    placement_ordered: false,
                    source_contract: Some(contract),
                    granted_by: None,
                    optional_additional_cost_paid: None,
                    paid_cost_refs: Vec::new(),
                })
            })
            .collect::<Vec<_>>();
        state.engine.pending_triggers.extend(captures);
    }
    #[cfg(any(
        not(feature = "limited-fdn-fixtures"),
        feature = "standard-magezero-fixtures"
    ))]
    {
        let _ = (state, event);
    }
}

#[cfg(test)]
#[path = "wildwood_scourge_v1_tests.rs"]
mod tests;
