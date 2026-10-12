//! Mandatory ownership of each genuine captured end-step occurrence.
use crate::effect::{EffectCond, EffectOp};
use crate::event::CommittedEvent;
use crate::ids::ObjectId;
use crate::state::{
    AbilitySourceContractV4, GameObject, GameState, StackItem, StackItemKind, Target, Zone,
};
use crate::trigger::PendingTrigger;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResolutionInputV2 {
    pub target_snapshot: Option<GameObject>,
    pub target_legal: bool,
    pub loss_flags: [bool; 2],
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OutcomeV2 {
    NoTargets,
    Countered,
    Resolved { counter_event_index: Option<u64> },
}

pub(crate) fn bound_root(capture: u64) -> EffectOp {
    EffectOp::Conditional {
        cond: EffectCond::OpponentLostLifeThisTurn,
        then: Box::new(EffectOp::StromkirkCounterFromCaptureV2 {
            capture_history_index: capture,
        }),
        else_: Box::new(EffectOp::Sequence(Vec::new())),
    }
}
pub(crate) fn capture_of(op: &EffectOp) -> Option<u64> {
    let EffectOp::Conditional {
        cond: EffectCond::OpponentLostLifeThisTurn,
        then,
        else_,
    } = op
    else {
        return None;
    };
    let EffectOp::StromkirkCounterFromCaptureV2 {
        capture_history_index,
    } = **then
    else {
        return None;
    };
    matches!(else_.as_ref(), EffectOp::Sequence(operations) if operations.is_empty())
        .then_some(capture_history_index)
}
pub(crate) fn source_at(
    state: &GameState,
    capture: u64,
) -> Result<AbilitySourceContractV4, String> {
    let at = usize::try_from(capture).map_err(|_| "Stromkirk capture index overflow")?;
    let Some(CommittedEvent::StromkirkTriggerCaptureV2 {
        observers_history_index,
        observer_index,
    }) = state.engine.event_history.get(at)
    else {
        return Err("Stromkirk capture occurrence is missing".into());
    };
    let observers_at = usize::try_from(*observers_history_index)
        .map_err(|_| "Stromkirk observer index overflow")?;
    let Some(CommittedEvent::StromkirkEndStepObserversV1 { observers, .. }) =
        state.engine.event_history.get(observers_at)
    else {
        return Err("Stromkirk capture lost original observer journal".into());
    };
    if observers_at >= at {
        return Err("Stromkirk capture precedes its original observer".into());
    }
    observers
        .get(*observer_index as usize)
        .copied()
        .ok_or_else(|| "Stromkirk observer ordinal changed".into())
}
#[cfg(any(
    feature = "limited-fdn-fixtures",
    feature = "standard-magezero-fixtures"
))]
pub(crate) fn emit_captures(state: &mut GameState, observers_at: usize) {
    let Some(CommittedEvent::StromkirkEndStepObserversV1 { observers, .. }) =
        state.engine.event_history.get(observers_at)
    else {
        return;
    };
    let count = observers.len();
    for observer in 0..count {
        let event = CommittedEvent::StromkirkTriggerCaptureV2 {
            observers_history_index: observers_at as u64,
            observer_index: observer as u32,
        };
        state.engine.event_history.push(event.clone());
        state.engine.event_log.push(event);
    }
}
fn capture_indices(state: &GameState) -> Vec<u64> {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, e)| {
            matches!(e, CommittedEvent::StromkirkTriggerCaptureV2 { .. }).then_some(at as u64)
        })
        .collect()
}
fn capture_event(state: &GameState, capture: u64) -> &CommittedEvent {
    &state.engine.event_history[capture as usize]
}
fn collected_indices(state: &GameState, capture: u64) -> Vec<usize> {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, e)| match e {
            CommittedEvent::StromkirkCollectedV2 {
                capture_history_index,
            } if *capture_history_index == capture => Some(at),
            _ => None,
        })
        .collect()
}
fn validate_journals(state: &GameState) -> Result<(), String> {
    for (at, event) in state.engine.event_history.iter().enumerate() {
        if let CommittedEvent::StromkirkEndStepObserversV1 { observers, .. } = event {
            for ordinal in 0..observers.len() {
                let count = state.engine.event_history.iter().filter(|e| matches!(e,
                    CommittedEvent::StromkirkTriggerCaptureV2 { observers_history_index, observer_index }
                        if *observers_history_index == at as u64 && *observer_index == ordinal as u32)).count();
                if count != 1 {
                    return Err(
                        "Stromkirk original observer lost or duplicated its mandatory capture"
                            .into(),
                    );
                }
            }
        }
        if matches!(event, CommittedEvent::StromkirkTriggerCaptureV2 { .. }) {
            let source = source_at(state, at as u64)?;
            crate::engine::validate_historical_ability_source_contract(state, source)?;
        }
    }
    Ok(())
}
pub(crate) fn collect(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Result<Vec<PendingTrigger>, ObjectId> {
    validate_journals(state).map_err(|_| ObjectId(0))?;
    let captures = capture_indices(state);
    let mut used = Vec::new();
    let mut pending = Vec::new();
    for capture in captures {
        let source = source_at(state, capture).map_err(|_| ObjectId(0))?;
        let already = collected_indices(state, capture);
        let count = events
            .iter()
            .filter(|e| *e == capture_event(state, capture))
            .count();
        if already.len() > 1
            || (already.is_empty() && count != 1)
            || (!already.is_empty() && count != 0)
        {
            return Err(source.source);
        }
        if already.is_empty() {
            used.push(capture);
            pending.push(PendingTrigger {
                controller: source.controller,
                source: source.source,
                source_contract: Some(source),
                granted_by: None,
                effect: bound_root(capture),
                is_madness_offer: false,
                kicked: false,
                target_spec: crate::stromkirk_bloodthief_v1::target_spec(),
                targets: Vec::new(),
                target_contracts: Vec::new(),
                placement_ordered: false,
                optional_additional_cost_paid: None,
                paid_cost_refs: Vec::new(),
            });
        }
    }
    // No malformed or forged collection row may be ignored.
    for event in events {
        if matches!(event, CommittedEvent::StromkirkTriggerCaptureV2 { .. })
            && !used
                .iter()
                .any(|capture| capture_event(state, *capture) == event)
        {
            return Err(ObjectId(0));
        }
    }
    for capture in used {
        state
            .engine
            .event_history
            .push(CommittedEvent::StromkirkCollectedV2 {
                capture_history_index: capture,
            });
    }
    Ok(pending)
}
fn publication(state: &GameState, capture: u64) -> Result<Option<(usize, &StackItem)>, String> {
    let rows: Vec<_> = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, e)| match e {
            CommittedEvent::StromkirkPublishedV2 {
                capture_history_index,
                item,
            } if *capture_history_index == capture => Some((at, item.as_ref())),
            _ => None,
        })
        .collect();
    match rows.as_slice() {
        [] => Ok(None),
        [row] => Ok(Some(*row)),
        _ => Err("Stromkirk repeated its publication".into()),
    }
}
fn validate_item(state: &GameState, capture: u64, item: &StackItem) -> Result<(), String> {
    let source = source_at(state, capture)?;
    if item.source != source.source
        || item.controller != source.controller
        || item.kind != StackItemKind::TriggeredAbility
        || item.v4.ability_source_contract != Some(source)
        || item.inline_effect != Some(bound_root(capture))
        || item.v4.granted_by.is_some()
        || item.v4.stack_item_id == crate::ids::StackItemId::default()
        || item.is_copy
        || item.madness_offer
        || item.kicked
        || item.is_flashback
        || !item.discarded.is_empty()
        || item.v4.optional_additional_cost_paid.is_some()
        || !item.v4.paid_cost_refs.is_empty()
        || item.targets.len() != 1
        || item.v4.target_contracts.len() != 1
    {
        return Err("Stromkirk publication changed its exact original targeted trigger".into());
    }
    let crate::state::StackTargetContractV4::Object {
        object,
        zone: Zone::Battlefield,
        zone_change_count,
        ..
    } = item.v4.target_contracts[0]
    else {
        return Err("Stromkirk publication lost its targeted permanent".into());
    };
    if item.targets[0] != Target::Object(object) || !state.engine.event_history.iter().any(|e| matches!(e,
        CommittedEvent::Targeted { target, target_zone_change_count, targeting_stack_item, targeting_controller }
            if *target == object && *target_zone_change_count == zone_change_count
                && *targeting_stack_item == item.v4.stack_item_id && *targeting_controller == item.controller)) {
        return Err("Stromkirk publication lost actual Targeted authority".into());
    }
    Ok(())
}
fn validate_open_turn(state: &GameState, capture: u64) -> Result<(), String> {
    let CommittedEvent::StromkirkTriggerCaptureV2 {
        observers_history_index,
        ..
    } = capture_event(state, capture)
    else {
        unreachable!()
    };
    let CommittedEvent::StromkirkEndStepObserversV1 { turn, .. } =
        &state.engine.event_history[*observers_history_index as usize]
    else {
        unreachable!()
    };
    if state.turn != *turn || state.active_player != source_at(state, capture)?.controller {
        return Err("Stromkirk open occurrence crossed its original turn".into());
    }
    Ok(())
}
fn historical_loss_flags(state: &GameState, before: usize) -> Result<[bool; 2], String> {
    let anchor = state.engine.event_history[..before]
        .iter()
        .rposition(|e| matches!(e, CommittedEvent::LifeLossTurnBeganV1 { .. }))
        .ok_or("Stromkirk resolution lost actual life-loss anchor")?;
    let mut flags = [false; 2];
    for event in &state.engine.event_history[anchor + 1..before] {
        if let Some(player) = crate::life_loss_turn_v1::committed_loss_player(event) {
            flags[player.index()] = true;
        }
    }
    Ok(flags)
}
pub(crate) fn publish(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    let Some(capture) = item.inline_effect.as_ref().and_then(capture_of) else {
        return Ok(());
    };
    validate_item(state, capture, item)?;
    if publication(state, capture)?.is_some() {
        return Err("Stromkirk publication replay".into());
    }
    state
        .engine
        .event_history
        .push(CommittedEvent::StromkirkPublishedV2 {
            capture_history_index: capture,
            item: Box::new(item.clone()),
        });
    Ok(())
}
fn complete(state: &mut GameState, capture: u64, operation: usize) {
    state
        .engine
        .event_history
        .push(CommittedEvent::StromkirkCompletedV2 {
            capture_history_index: capture,
            operation_history_index: operation as u64,
        });
}
pub(crate) fn unavailable(state: &mut GameState, pending: &PendingTrigger) -> Result<(), String> {
    let Some(capture) = capture_of(&pending.effect) else {
        return Ok(());
    };
    if crate::engine::pending_trigger_targets_can_complete(pending, state) {
        return Err("Stromkirk legal target cannot be silently removed".into());
    }
    let at = state.engine.event_history.len();
    state
        .engine
        .event_history
        .push(CommittedEvent::StromkirkPlacementRemovedV2 {
            capture_history_index: capture,
            pending: Box::new(pending.clone()),
        });
    let operation = state.engine.event_history.len();
    state
        .engine
        .event_history
        .push(CommittedEvent::StromkirkOperationV2 {
            capture_history_index: capture,
            removal_history_index: at as u64,
            outcome: OutcomeV2::NoTargets,
        });
    complete(state, capture, operation);
    Ok(())
}

/// Only engine construction before actual removal supplies this token.
pub(crate) struct RemovedOwnerV2 {
    item: StackItem,
    capture: u64,
}
pub(crate) fn acquire(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<RemovedOwnerV2>, String> {
    let Some(capture) = item.inline_effect.as_ref().and_then(capture_of) else {
        return Ok(None);
    };
    validate_state(state)?;
    validate_item(state, capture, item)?;
    if publication(state, capture)?.map(|p| p.1) != Some(item)
        || state
            .stack
            .iter()
            .filter(|live| live.v4.stack_item_id == item.v4.stack_item_id)
            .collect::<Vec<_>>()
            != [item]
    {
        return Err("Stromkirk removed owner lost exact live publication".into());
    }
    Ok(Some(RemovedOwnerV2 {
        item: item.clone(),
        capture,
    }))
}
fn input(state: &GameState, item: &StackItem) -> Result<ResolutionInputV2, String> {
    let source = item
        .v4
        .ability_source_contract
        .ok_or("Stromkirk source absent")?;
    let contract = crate::effect::temporary_control_v1::normalize_historical_target(
        state,
        item.v4.target_contracts[0],
    );
    let target_snapshot = match item.targets[0] {
        Target::Object(id) => state.objects.try_get(id).cloned(),
        _ => None,
    };
    let target_legal =
        crate::engine::target_contract_matches_live(state, item.targets[0], contract)
            && crate::engine::effect_target_is_legal_from_ability_source(
                state,
                source,
                item.controller,
                crate::stromkirk_bloodthief_v1::target_spec(),
                &item.targets,
                0,
            );
    let loss_flags = crate::life_loss_turn_v1::flags(state)?;
    if target_legal
        && loss_flags[item.controller.opponent().index()]
        && target_snapshot
            .as_ref()
            .unwrap()
            .counters
            .plus1_plus1
            .checked_add(1)
            .is_none()
    {
        return Err("Stromkirk permanent counter overflow".into());
    }
    Ok(ResolutionInputV2 {
        target_snapshot,
        target_legal,
        loss_flags,
    })
}
pub(crate) fn preflight_resolution(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<RemovedOwnerV2>, String> {
    let owner = acquire(state, item)?;
    if owner.is_some() {
        input(state, item)?;
    }
    Ok(owner)
}
pub(crate) fn resolve_owned(state: &mut GameState, owner: &RemovedOwnerV2) -> Result<(), String> {
    if state
        .stack
        .iter()
        .any(|i| i.v4.stack_item_id == owner.item.v4.stack_item_id)
    {
        return Err("Stromkirk row was not actually removed".into());
    }
    let input = input(state, &owner.item)?;
    let removal = state.engine.event_history.len();
    state
        .engine
        .event_history
        .push(CommittedEvent::StromkirkResolutionRemovedV2 {
            capture_history_index: owner.capture,
            item: Box::new(owner.item.clone()),
            input: Box::new(input.clone()),
        });
    let counter_event_index =
        if input.target_legal && input.loss_flags[owner.item.controller.opponent().index()] {
            let Target::Object(target) = owner.item.targets[0] else {
                return Err("Stromkirk target changed".into());
            };
            let at = state.engine.event_history.len();
            crate::event::add_plus_one_counters(state, target, owner.item.controller, 1)?;
            Some(at as u64)
        } else {
            None
        };
    let operation = state.engine.event_history.len();
    state
        .engine
        .event_history
        .push(CommittedEvent::StromkirkOperationV2 {
            capture_history_index: owner.capture,
            removal_history_index: removal as u64,
            outcome: OutcomeV2::Resolved {
                counter_event_index,
            },
        });
    complete(state, owner.capture, operation);
    Ok(())
}
pub(crate) fn counter_removed(
    state: &mut GameState,
    owner: Option<&RemovedOwnerV2>,
    item: &StackItem,
) -> Result<(), String> {
    let Some(owner) = owner else {
        return Ok(());
    };
    if *item != owner.item
        || state
            .stack
            .iter()
            .any(|i| i.v4.stack_item_id == item.v4.stack_item_id)
    {
        return Err("Stromkirk counter did not remove its exact loaned row".into());
    }
    let removal = state.engine.event_history.len();
    state
        .engine
        .event_history
        .push(CommittedEvent::StromkirkCounterRemovedV2 {
            capture_history_index: owner.capture,
            item: Box::new(item.clone()),
        });
    let operation = state.engine.event_history.len();
    state
        .engine
        .event_history
        .push(CommittedEvent::StromkirkOperationV2 {
            capture_history_index: owner.capture,
            removal_history_index: removal as u64,
            outcome: OutcomeV2::Countered,
        });
    complete(state, owner.capture, operation);
    Ok(())
}

pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    validate_journals(state)?;
    let captures = capture_indices(state);
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let referenced = match event {
            CommittedEvent::StromkirkCollectedV2 {
                capture_history_index,
            }
            | CommittedEvent::StromkirkPublishedV2 {
                capture_history_index,
                ..
            }
            | CommittedEvent::StromkirkPlacementRemovedV2 {
                capture_history_index,
                ..
            }
            | CommittedEvent::StromkirkResolutionRemovedV2 {
                capture_history_index,
                ..
            }
            | CommittedEvent::StromkirkCounterRemovedV2 {
                capture_history_index,
                ..
            }
            | CommittedEvent::StromkirkOperationV2 {
                capture_history_index,
                ..
            }
            | CommittedEvent::StromkirkCompletedV2 {
                capture_history_index,
                ..
            } => Some(*capture_history_index),
            _ => None,
        };
        if referenced.is_some_and(|c| !captures.contains(&c) || c as usize >= at) {
            return Err("Stromkirk lifecycle receipt is orphaned or out of order".into());
        }
    }
    for event in &state.engine.event_log {
        if matches!(
            event,
            CommittedEvent::StromkirkCollectedV2 { .. }
                | CommittedEvent::StromkirkPublishedV2 { .. }
                | CommittedEvent::StromkirkPlacementRemovedV2 { .. }
                | CommittedEvent::StromkirkResolutionRemovedV2 { .. }
                | CommittedEvent::StromkirkCounterRemovedV2 { .. }
                | CommittedEvent::StromkirkOperationV2 { .. }
                | CommittedEvent::StromkirkCompletedV2 { .. }
        ) {
            return Err("private Stromkirk lifecycle entered mutable trigger log".into());
        }
        if matches!(event, CommittedEvent::StromkirkTriggerCaptureV2 { .. })
            && !captures.iter().any(|c| capture_event(state, *c) == event)
        {
            return Err("forged Stromkirk dispatch log capture".into());
        }
    }
    let waiting: Vec<_> = state
        .engine
        .pending_triggers
        .iter()
        .chain(state.legend_pending_v1.iter().flatten())
        .chain(
            state
                .pending_legend_rule_v1
                .iter()
                .flat_map(|p| p.waiting_triggers()),
        )
        .chain(
            state
                .standard_v1
                .iter()
                .filter_map(|s| s.resolution_play.as_ref())
                .flat_map(|p| p.deferred_triggers.iter()),
        )
        .collect();
    for capture in captures {
        let source = source_at(state, capture)?;
        let collected = collected_indices(state, capture);
        let logged = state
            .engine
            .event_log
            .iter()
            .filter(|e| *e == capture_event(state, capture))
            .count();
        let held: Vec<_> = waiting
            .iter()
            .filter(|p| capture_of(&p.effect) == Some(capture))
            .copied()
            .collect();
        for pending in &held {
            crate::engine::validate_pending_trigger_identity(state, pending)?;
            if pending.source_contract != Some(source)
                || pending.source != source.source
                || pending.controller != source.controller
                || pending.granted_by.is_some()
                || pending.target_spec != crate::stromkirk_bloodthief_v1::target_spec()
            {
                return Err("Stromkirk held owner changed original source/controller".into());
            }
        }
        let published = publication(state, capture)?;
        let completions: Vec<_> = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(at, e)| match e {
                CommittedEvent::StromkirkCompletedV2 {
                    capture_history_index,
                    operation_history_index,
                } if *capture_history_index == capture => Some((at, *operation_history_index)),
                _ => None,
            })
            .collect();
        let operations: Vec<_> = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(at, e)| match e {
                CommittedEvent::StromkirkOperationV2 {
                    capture_history_index,
                    removal_history_index,
                    outcome,
                } if *capture_history_index == capture => {
                    Some((at, *removal_history_index, outcome))
                }
                _ => None,
            })
            .collect();
        let removals: Vec<_> = state.engine.event_history.iter().enumerate().filter(|(_,e)| matches!(e,
            CommittedEvent::StromkirkPlacementRemovedV2 { capture_history_index, .. }
            | CommittedEvent::StromkirkResolutionRemovedV2 { capture_history_index, .. }
            | CommittedEvent::StromkirkCounterRemovedV2 { capture_history_index, .. } if *capture_history_index == capture)).collect();
        let live: Vec<_> = state
            .stack
            .iter()
            .filter(|i| {
                i.inline_effect.as_ref().and_then(capture_of) == Some(capture)
                    || published.is_some_and(|p| i.v4.stack_item_id == p.1.v4.stack_item_id)
            })
            .collect();
        if collected.is_empty() {
            validate_open_turn(state, capture)?;
            if logged != 1
                || !held.is_empty()
                || published.is_some()
                || !completions.is_empty()
                || !operations.is_empty()
                || !removals.is_empty()
                || !live.is_empty()
            {
                return Err(
                    "Stromkirk uncollected occurrence lost mandatory waiting ownership".into(),
                );
            }
            continue;
        }
        if collected.len() != 1 || collected[0] <= capture as usize || logged != 0 {
            return Err("Stromkirk collection repeated or mutable dispatch replayed".into());
        }
        if let Some((at, item)) = published {
            if at <= collected[0] {
                return Err("Stromkirk publication precedes collection".into());
            }
            validate_item(state, capture, item)?;
        }
        if completions.is_empty() {
            validate_open_turn(state, capture)?;
            if !operations.is_empty() || !removals.is_empty() {
                return Err("Stromkirk operation lacks exact completion".into());
            }
            match published {
                None if held.len() == 1 && live.is_empty() => {}
                Some((_, item)) if held.is_empty() && live.as_slice() == [item] => {}
                _ => {
                    return Err(
                        "Stromkirk open capture lost or duplicated its pending/published owner"
                            .into(),
                    )
                }
            }
            continue;
        }
        if !held.is_empty()
            || !live.is_empty()
            || completions.len() != 1
            || operations.len() != 1
            || removals.len() != 1
        {
            return Err(
                "Stromkirk completed occurrence was erased, replayed or reintroduced".into(),
            );
        }
        let (closed_at, operation_ref) = completions[0];
        let (operation_at, removal_ref, outcome) = operations[0];
        let (removal_at, removed) = removals[0];
        if operation_ref != operation_at as u64
            || removal_ref != removal_at as u64
            || removal_at <= collected[0]
            || operation_at <= removal_at
            || closed_at <= operation_at
        {
            return Err("Stromkirk completion lost independent ordered operation/removal".into());
        }
        match (removed, outcome, published) {
            (
                CommittedEvent::StromkirkPlacementRemovedV2 { pending, .. },
                OutcomeV2::NoTargets,
                None,
            ) if pending.source_contract == Some(source)
                && pending.controller == source.controller
                && pending.effect == bound_root(capture)
                && pending.target_spec == crate::stromkirk_bloodthief_v1::target_spec()
                && pending.targets.is_empty()
                && pending.target_contracts.is_empty() => {}
            (
                CommittedEvent::StromkirkCounterRemovedV2 { item, .. },
                OutcomeV2::Countered,
                Some((pub_at, original)),
            ) if item.as_ref() == original && removal_at > pub_at => {}
            (
                CommittedEvent::StromkirkResolutionRemovedV2 { item, input, .. },
                OutcomeV2::Resolved {
                    counter_event_index,
                },
                Some((pub_at, original)),
            ) if item.as_ref() == original && removal_at > pub_at => {
                if input.loss_flags != historical_loss_flags(state, removal_at)? {
                    return Err("Stromkirk resolution changed committed loss flags".into());
                }
                let should_counter =
                    input.target_legal && input.loss_flags[item.controller.opponent().index()];
                if should_counter != counter_event_index.is_some() {
                    return Err("Stromkirk completion skipped its actual counter outcome".into());
                }
                if let Some(index) = counter_event_index {
                    let Target::Object(target) = item.targets[0] else {
                        return Err("Stromkirk counter recipient absent".into());
                    };
                    if *index as usize <= removal_at
                        || *index as usize >= operation_at
                        || !matches!(state.engine.event_history.get(*index as usize),
                        Some(CommittedEvent::PlusOneCountersAdded { object, zone_change_count, player, count:1 })
                            if *object==target && *player==item.controller
                                && input.target_snapshot.as_ref().is_some_and(|snapshot|
                                    snapshot.zone == Zone::Battlefield && snapshot.zone_change_count == *zone_change_count))
                    {
                        return Err(
                            "Stromkirk resolved counter lost actual placement occurrence".into(),
                        );
                    }
                }
            }
            _ => return Err("Stromkirk completion changed actual outcome or original item".into()),
        }
    }
    Ok(())
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod future_printed_tests {
    use super::*;
    use crate::engine::{self, Action, Decision};
    use crate::ids::PlayerId;
    use crate::mana::ManaColor;
    use crate::policy_observation_v6::tests::{put, ready_state};

    #[test]
    fn actual_exact_id_counter_closes_original_trigger_and_refuses_saved_publication_replay() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state();
            state.active_player = actor;
            state.priority_player = actor;
            state.step = crate::state::Step::Main2;
            // Fixture state uses actual catalog turn initialization, not forged history.
            crate::life_loss_turn_v1::initialize_for_catalog(&mut state);
            let source = put(&mut state, actor, "Stromkirk Bloodthief", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
            state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 1;
            assert!(
                matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, castable_spells, .. }
                if player == actor && castable_spells.contains(&source))
            );
            engine::step(&mut state, Action::CastSpell(source)).unwrap();
            for _ in 0..30 {
                assert!(matches!(
                    engine::advance_until_decision(&mut state),
                    Decision::CastSpellOrPass { .. }
                ));
                if state.objects.get(source).zone == Zone::Battlefield && state.stack.is_empty() {
                    break;
                }
                engine::step(&mut state, Action::Pass).unwrap();
            }
            assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::life_loss(actor.opponent(), 1),
            );
            for _ in 0..30 {
                match engine::advance_until_decision(&mut state) {
                    Decision::ChooseTargets { legal_targets, .. } => {
                        assert!(legal_targets.contains(&Target::Object(source)));
                        engine::step(&mut state, Action::ChooseTarget(Target::Object(source)))
                            .unwrap();
                        break;
                    }
                    Decision::CastSpellOrPass { .. } => {
                        engine::step(&mut state, Action::Pass).unwrap()
                    }
                    other => panic!("actual end-step placement {other:?}"),
                }
            }
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            let saved = state.stack.last().unwrap().clone();
            assert!(crate::stromkirk_bloodthief_v1::owns_item(&saved));
            // Explicit existing engine exact-ID counter primitive, not a fake printed counter spell.
            engine::counter_stack_item_by_id(&mut state, saved.v4.stack_item_id).unwrap();
            validate_state(&state).unwrap();
            assert_eq!(state.objects.get(source).counters.plus1_plus1, 0);
            let bytes = serde_json::to_vec(&state).unwrap();
            for case in 0..3 {
                let mut bad: GameState = serde_json::from_slice(&bytes).unwrap();
                match case {
                    0 => bad.stack.push(saved.clone()),
                    1 => bad
                        .engine
                        .event_history
                        .retain(|e| !matches!(e, CommittedEvent::StromkirkCounterRemovedV2 { .. })),
                    _ => bad
                        .engine
                        .event_history
                        .retain(|e| !matches!(e, CommittedEvent::StromkirkOperationV2 { .. })),
                }
                let before = serde_json::to_vec(&bad).unwrap();
                assert!(engine::step(&mut bad, Action::Pass).is_err());
                assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
            }
        }
    }
}

#[cfg(test)]
mod absence_tests {
    use super::*;
    #[test]
    fn old_catalog_dispatch_hooks_preserve_state_and_primary_hash() {
        if crate::life_loss_turn_v1::enabled() {
            return;
        }
        let mut state = crate::policy_observation_v6::tests::ready_state();
        let bytes = serde_json::to_vec(&state).unwrap();
        let hash = state.diagnostic_state_hash();
        validate_state(&state).unwrap();
        assert!(collect(&mut state, &[]).unwrap().is_empty());
        assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
        assert_eq!(state.diagnostic_state_hash(), hash);
    }
}
