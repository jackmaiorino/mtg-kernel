//! Private event-time authority for the Wildwood successor. Never a public DTO.
use crate::card_def::CARD_DEFS;
use crate::effect::{EffectOp, ExecCtx};
use crate::event::{self, CommittedEvent};
use crate::ids::{ObjectId, StackItemId};
use crate::state::{
    AbilitySourceContractV4, GameState, StackItem, StackItemKind, StackTargetContractV4, Zone,
};
use crate::trigger::PendingTrigger;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureV1 {
    pub(crate) event_index: usize,
    pub(crate) event: Box<CommittedEvent>,
    pub(crate) recipient: StackTargetContractV4,
    pub(crate) recipient_face_down: bool,
    pub(crate) recipient_disguised: bool,
    pub(crate) recipient_is_creature: bool,
    pub(crate) recipient_is_hydra: bool,
    pub(crate) observers: Vec<AbilitySourceContractV4>,
}

pub(crate) fn owns_definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && CARD_DEFS
        .get(usize::from(id))
        .is_some_and(|def| def.is_executable() && def.name == "Wildwood Scourge")
        && matches!(crate::trigger::triggers_for(id), [ability] if ability.condition == crate::trigger::TriggerCondition::AnotherControlledNonHydraCreatureGetsPlusOneCounters && (ability.effect)() == crate::wildwood_scourge_v1::counter_program_v1())
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) fn capture(
    state: &mut GameState,
    event: &CommittedEvent,
    observers: Vec<AbilitySourceContractV4>,
) -> usize {
    use crate::card_def::{CardType, Subtype};
    use crate::state::Target;
    let CommittedEvent::PlusOneCountersAdded { object, .. } = event else {
        unreachable!("counter capture hook")
    };
    let record = CaptureV1 {
        event_index: state.engine.event_history.len() - 1,
        event: Box::new(event.clone()),
        recipient: StackTargetContractV4::capture(state, Target::Object(*object)),
        recipient_face_down: state.objects.get(*object).v4.face_down_v1.is_some(),
        recipient_disguised: state
            .objects
            .get(*object)
            .v4
            .face_down_v1
            .is_some_and(|face| face.disguised),
        recipient_is_creature: crate::engine::object_has_type(state, *object, CardType::Creature),
        recipient_is_hydra: crate::engine::has_effective_subtype(state, *object, Subtype::Hydra),
        observers,
    };
    let index = state.engine.event_history.len();
    state
        .engine
        .event_history
        .push(CommittedEvent::WildwoodCounterCaptureV1 {
            capture: Box::new(record),
        });
    index
}

fn key(op: &EffectOp) -> Option<(usize, usize)> {
    let EffectOp::WildwoodCounterFromReceiptV1 {
        capture_history_index,
        observer_index,
    } = op
    else {
        return None;
    };
    Some((
        usize::try_from(*capture_history_index).ok()?,
        usize::try_from(*observer_index).ok()?,
    ))
}

fn record(state: &GameState, capture_index: usize) -> Result<&CaptureV1, String> {
    let Some(CommittedEvent::WildwoodCounterCaptureV1 { capture }) =
        state.engine.event_history.get(capture_index)
    else {
        return Err("Wildwood lost event-time capture".into());
    };
    let CommittedEvent::PlusOneCountersAdded {
        object,
        zone_change_count,
        count,
        ..
    } = capture.event.as_ref()
    else {
        return Err("Wildwood capture is not a counter occurrence".into());
    };
    if capture.event_index.checked_add(1) != Some(capture_index)
        || state.engine.event_history.get(capture.event_index) != Some(capture.event.as_ref())
        || *count <= 0
        || !capture.recipient_is_creature
        || capture.recipient_is_hydra
        || capture.observers.is_empty()
    {
        return Err("Wildwood event-time counter facts changed".into());
    }
    let StackTargetContractV4::Object {
        object: recipient,
        controller,
        zone: Zone::Battlefield,
        zone_change_count: generation,
        ..
    } = capture.recipient
    else {
        return Err("Wildwood capture lost battlefield recipient".into());
    };
    if recipient != *object || generation != *zone_change_count {
        return Err("Wildwood recipient incarnation changed".into());
    }
    let mut seen = Vec::new();
    for observer in &capture.observers {
        if !owns_definition(observer.card_def)
            || observer.zone != Zone::Battlefield
            || observer.controller != controller
            || observer.source == recipient
            || seen.contains(&observer.source)
        {
            return Err("Wildwood original observer relation changed".into());
        }
        seen.push(observer.source);
    }
    Ok(capture)
}

pub(crate) fn validate_trigger(
    state: &GameState,
    op: &EffectOp,
    source: ObjectId,
    controller: crate::ids::PlayerId,
    contract: Option<AbilitySourceContractV4>,
) -> Result<(), String> {
    let Some((capture, observer)) = key(op) else {
        if contract.is_some_and(|value| owns_definition(value.card_def)) {
            return Err("Wildwood trigger erased its occurrence binding".into());
        }
        return Ok(());
    };
    let expected = record(state, capture)?
        .observers
        .get(observer)
        .ok_or("Wildwood observer ordinal invalid")?;
    if contract != Some(*expected) || source != expected.source || controller != expected.controller
    {
        return Err("Wildwood original observer changed".into());
    }
    Ok(())
}

fn item_key(item: &StackItem) -> Option<(usize, usize)> {
    item.inline_effect.as_ref().and_then(key)
}

pub(crate) fn publish(state: &mut GameState, item: &StackItem) {
    if item_key(item).is_some() {
        state
            .engine
            .event_history
            .push(CommittedEvent::WildwoodTriggerPublishedV1 {
                item: Box::new(item.clone()),
            });
    }
}

fn published(state: &GameState, id: StackItemId) -> Result<(usize, &StackItem), String> {
    let mut found = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            CommittedEvent::WildwoodTriggerPublishedV1 { item } if item.v4.stack_item_id == id => {
                Some((index, item.as_ref()))
            }
            _ => None,
        });
    let row = found.next().ok_or("Wildwood lost finalized publication")?;
    if found.next().is_some() {
        return Err("Wildwood publication duplicated".into());
    }
    Ok(row)
}

pub(crate) fn begin(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    if item_key(item).is_none() {
        return Ok(());
    }
    let (_, saved) = published(state, item.v4.stack_item_id)?;
    if saved != item
        || state
            .stack
            .iter()
            .filter(|live| live.v4.stack_item_id == item.v4.stack_item_id)
            .count()
            != 1
    {
        return Err("Wildwood resolution lost live published item".into());
    }
    state
        .engine
        .event_history
        .push(CommittedEvent::WildwoodTriggerResolvingV1 {
            stack_item_id: item.v4.stack_item_id,
        });
    Ok(())
}

pub(crate) fn execute(state: &mut GameState, ctx: &ExecCtx, op: &EffectOp) -> Result<(), String> {
    let id = ctx
        .stack_item_id
        .ok_or("Wildwood operation lost stack incarnation")?;
    let (publication_index, saved) = published(state, id)?;
    if saved.inline_effect.as_ref() != Some(op)
        || saved.source != ctx.source
        || saved.controller != ctx.controller
        || saved.v4.ability_source_contract != ctx.ability_source_contract
    {
        return Err("Wildwood execution context changed".into());
    }
    validate_trigger(
        state,
        op,
        ctx.source,
        ctx.controller,
        ctx.ability_source_contract,
    )?;
    let starts = state.engine.event_history.iter().enumerate().filter(|(_, event)| matches!(event, CommittedEvent::WildwoodTriggerResolvingV1 { stack_item_id } if *stack_item_id == id)).collect::<Vec<_>>();
    if !matches!(starts.as_slice(), [(index, _)] if *index > publication_index)
        || state.stack.iter().any(|live| live.v4.stack_item_id == id)
        || state.engine.event_history.iter().any(|event| matches!(event, CommittedEvent::WildwoodTriggerOperationV1 { stack_item_id, .. } | CommittedEvent::WildwoodTriggerClosedV1 { stack_item_id, .. } if *stack_item_id == id))
    { return Err("Wildwood operation restarted outside owned resolution".into()); }
    let source = ctx
        .ability_source_contract
        .ok_or("Wildwood lost source binding")?;
    let present = state.objects.try_get(source.source).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.zone_change_count == source.zone_change_count
            && live.owner == source.owner
    });
    let counter_event_index = if present {
        let before = state.engine.event_history.len();
        event::add_plus_one_counters(state, source.source, ctx.controller, 1)?;
        Some(before)
    } else {
        None
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::WildwoodTriggerOperationV1 {
            stack_item_id: id,
            counter_event_index,
        });
    Ok(())
}

pub(crate) fn close(state: &mut GameState, item: &StackItem, countered: bool) {
    if item_key(item).is_some() {
        if countered {
            state
                .engine
                .event_history
                .push(CommittedEvent::WildwoodTriggerCounteredV1 {
                    item: Box::new(item.clone()),
                });
        }
        state
            .engine
            .event_history
            .push(CommittedEvent::WildwoodTriggerClosedV1 {
                stack_item_id: item.v4.stack_item_id,
                countered,
            });
    }
}

fn pending_rows(state: &GameState) -> Vec<&PendingTrigger> {
    let mut rows = state.engine.pending_triggers.iter().collect::<Vec<_>>();
    if let Some(waiting) = &state.wildwood_waiting_v1 {
        rows.extend(waiting);
    }
    if let Some(waiting) = &state.legend_pending_v1 {
        rows.extend(waiting);
    }
    if let Some(legend) = &state.pending_legend_rule_v1 {
        rows.extend(legend.waiting_triggers());
    }
    if let Some(standard) = &state.standard_v1 {
        if let Some(resolution) = &standard.resolution_play {
            rows.extend(&resolution.deferred_triggers);
        }
    }
    rows
}

pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    let marker = |event: &CommittedEvent| {
        matches!(
            event,
            CommittedEvent::WildwoodCounterCaptureV1 { .. }
                | CommittedEvent::WildwoodTriggerPublishedV1 { .. }
                | CommittedEvent::WildwoodTriggerResolvingV1 { .. }
                | CommittedEvent::WildwoodTriggerOperationV1 { .. }
                | CommittedEvent::WildwoodTriggerClosedV1 { .. }
                | CommittedEvent::WildwoodTriggerCounteredV1 { .. }
        )
    };
    if state.engine.event_log.iter().any(marker) {
        return Err("Wildwood private receipt injected into dispatch log".into());
    }
    let pending = pending_rows(state);
    for row in &pending {
        validate_trigger(
            state,
            &row.effect,
            row.source,
            row.controller,
            row.source_contract,
        )?;
        if key(&row.effect).is_some()
            && (row.is_madness_offer
                || row.kicked
                || row.target_spec != crate::card_def::TargetSpec::None
                || !row.targets.is_empty()
                || !row.target_contracts.is_empty()
                || row.granted_by.is_some()
                || row.optional_additional_cost_paid.is_some()
                || !row.paid_cost_refs.is_empty())
        {
            return Err("Wildwood waiting metadata changed".into());
        }
    }
    for item in &state.stack {
        if item_key(item).is_some()
            || item
                .v4
                .ability_source_contract
                .is_some_and(|source| owns_definition(source.card_def))
                && item.kind == StackItemKind::TriggeredAbility
        {
            validate_trigger(
                state,
                item.inline_effect
                    .as_ref()
                    .ok_or("Wildwood erased live program")?,
                item.source,
                item.controller,
                item.v4.ability_source_contract,
            )?;
            let (_, saved) = published(state, item.v4.stack_item_id)?;
            if saved != item {
                return Err("Wildwood live item changed after publication".into());
            }
        }
    }
    let mut publications = Vec::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        if let CommittedEvent::WildwoodCounterCaptureV1 { .. } = event {
            let capture = record(state, index)?;
            for (observer, _) in capture.observers.iter().enumerate() {
                let wanted = (index, observer);
                let waiting = pending
                    .iter()
                    .filter(|row| key(&row.effect) == Some(wanted))
                    .count();
                let published_count = state.engine.event_history.iter().filter(|event| matches!(event, CommittedEvent::WildwoodTriggerPublishedV1 { item } if item_key(item) == Some(wanted))).count();
                if !matches!((waiting, published_count), (1, 0) | (0, 1)) {
                    return Err(
                        "Wildwood captured occurrence ownership erased or duplicated".into(),
                    );
                }
            }
        }
        let CommittedEvent::WildwoodTriggerPublishedV1 { item } = event else {
            continue;
        };
        let id = item.v4.stack_item_id;
        if id.0 == 0
            || publications.contains(&id)
            || item.kind != StackItemKind::TriggeredAbility
            || !item.targets.is_empty()
            || item.v4.target_spec.is_some()
        {
            return Err("Wildwood finalized publication changed".into());
        }
        publications.push(id);
        let op = item
            .inline_effect
            .as_ref()
            .ok_or("Wildwood publication erased program")?;
        let (capture, _) = key(op).ok_or("Wildwood publication changed program")?;
        if capture >= index {
            return Err("Wildwood publication precedes capture".into());
        }
        validate_trigger(
            state,
            op,
            item.source,
            item.controller,
            item.v4.ability_source_contract,
        )?;
        let starts = state.engine.event_history.iter().enumerate().filter(|(_, e)| matches!(e, CommittedEvent::WildwoodTriggerResolvingV1 { stack_item_id } if *stack_item_id == id)).collect::<Vec<_>>();
        let operations = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(i, e)| match e {
                CommittedEvent::WildwoodTriggerOperationV1 {
                    stack_item_id,
                    counter_event_index,
                } if *stack_item_id == id => Some((i, *counter_event_index)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let closures = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(i, e)| match e {
                CommittedEvent::WildwoodTriggerClosedV1 {
                    stack_item_id,
                    countered,
                } if *stack_item_id == id => Some((i, *countered)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let counters = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(i, e)| match e {
                CommittedEvent::WildwoodTriggerCounteredV1 { item: removed }
                    if removed.v4.stack_item_id == id =>
                {
                    Some((i, removed.as_ref()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let live = state
            .stack
            .iter()
            .filter(|candidate| candidate.v4.stack_item_id == id)
            .count();
        match closures.as_slice() {
            [] if live == 1
                && starts.is_empty()
                && operations.is_empty()
                && counters.is_empty() => {}
            [(closed, true)] if live == 0 && starts.is_empty() && operations.is_empty() => {
                if !matches!(counters.as_slice(), [(counter, removed)] if index < *counter && *counter < *closed && *removed == item.as_ref())
                {
                    return Err("Wildwood closure lost actual counter operation".into());
                }
            }
            [(closed, false)] if live == 0 && counters.is_empty() => {
                let [(start, _)] = starts.as_slice() else {
                    return Err("Wildwood completion lost resolution ownership".into());
                };
                let [(operation, added)] = operations.as_slice() else {
                    return Err("Wildwood completion lost actual operation".into());
                };
                if !(index < *start && *start < *operation && *operation < *closed) {
                    return Err("Wildwood lifecycle order changed".into());
                }
                if let Some(added) = added {
                    let source = item
                        .v4
                        .ability_source_contract
                        .ok_or("Wildwood operation source missing")?;
                    if !(*start < *added
                        && *added < *operation
                        && matches!(state.engine.event_history.get(*added), Some(CommittedEvent::PlusOneCountersAdded { object, zone_change_count, player, count }) if *object == source.source && *zone_change_count == source.zone_change_count && *player == source.controller && crate::doubling_season_v48::expected_plus_at(state,*added as u64,*object,*player,1).ok()==Some(*count)))
                    {
                        return Err("Wildwood operation counter occurrence changed".into());
                    }
                } else {
                    let source = item
                        .v4
                        .ability_source_contract
                        .ok_or("Wildwood absent operation lost source")?;
                    // The capture proves this exact incarnation was on the
                    // battlefield. Its first later committed departure makes
                    // that incarnation permanently unavailable, even on return.
                    if !state.engine.event_history[capture + 1..*start].iter().any(|event| {
                        matches!(event, CommittedEvent::ZoneChange { object, from: Zone::Battlefield, .. }
                            if *object == source.source)
                    }) {
                        return Err("Wildwood no-op lacks a committed source departure".into());
                    }
                }
            }
            _ => return Err("Wildwood resolution owner erased or reintroduced".into()),
        }
    }
    for event in &state.engine.event_history {
        if let CommittedEvent::WildwoodTriggerCounteredV1 { item } = event {
            if !publications.contains(&item.v4.stack_item_id) {
                return Err("Wildwood orphan counter operation".into());
            }
        }
        if let CommittedEvent::WildwoodTriggerResolvingV1 { stack_item_id }
        | CommittedEvent::WildwoodTriggerOperationV1 { stack_item_id, .. }
        | CommittedEvent::WildwoodTriggerClosedV1 { stack_item_id, .. } = event
        {
            if !publications.contains(stack_item_id) {
                return Err("Wildwood orphan lifecycle marker".into());
            }
        }
    }
    Ok(())
}

/// Only current owners need a historical public relation. Completed receipts
/// remain private authority and do not disclose a later hidden card roster.
pub(crate) fn active(state: &GameState) -> Result<Vec<(usize, usize, &CaptureV1)>, String> {
    validate_state(state)?;
    let mut result = Vec::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::WildwoodCounterCaptureV1 { capture } = event else {
            continue;
        };
        for observer in 0..capture.observers.len() {
            let closed = state.engine.event_history.iter().any(|event| {
                let CommittedEvent::WildwoodTriggerPublishedV1 { item } = event else { return false; };
                item_key(item) == Some((index, observer)) && state.engine.event_history.iter().any(|event| matches!(event, CommittedEvent::WildwoodTriggerClosedV1 { stack_item_id, .. } if *stack_item_id == item.v4.stack_item_id))
            });
            if !closed {
                result.push((index, observer, capture.as_ref()));
            }
        }
    }
    Ok(result)
}

pub(crate) fn refuse_legacy(state: &GameState) -> Result<(), crate::rl::RlContractError> {
    if !active(state)
        .map_err(crate::rl::RlContractError)?
        .is_empty()
    {
        return Err(crate::rl::RlContractError(
            "Wildwood event-time capture requires its public successor".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::{put, ready_state};

    #[test]
    fn registered_absence_emits_only_the_existing_counter_event_and_preserves_serde_hash_restore() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state();
            let recipient = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let before = state.engine.event_history.len();
            event::add_plus_one_counters(&mut state, recipient, actor.opponent(), 2).unwrap();
            assert_eq!(state.objects.get(recipient).counters.plus1_plus1, 2);
            if cfg!(feature = "limited-fdn-fixtures") {
                assert_eq!(state.engine.event_history.len(), before + 1);
                assert!(
                    matches!(state.engine.event_history.last(), Some(CommittedEvent::PlusOneCountersAdded { object, count: 2, .. }) if *object == recipient)
                );
            } else {
                // Configurations without Limited emit no counter receipt.
                // Preserve that absence and the unchanged placement.
                assert_eq!(state.engine.event_history.len(), before);
            }
            assert!(state.wildwood_waiting_v1.is_none());
            assert!(active(&state).unwrap().is_empty());
            refuse_legacy(&state).unwrap();
            let bytes = serde_json::to_vec(&state).unwrap();
            let replay: GameState = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(serde_json::to_vec(&replay).unwrap(), bytes);
            assert_eq!(
                state.diagnostic_state_hash(),
                replay.diagnostic_state_hash()
            );
        }
    }
}
