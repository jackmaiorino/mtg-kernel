//! End-step observer authority for the selected printed Cerberus trigger.
//! Independent capture, collection, publication, operation and completion records.
use crate::card_def::CARD_DEFS;
use crate::effect::{EffectOp, ExecCtx};
use crate::event::CommittedEvent;
use crate::ids::ObjectId;
use crate::state::{AbilitySourceContractV4, GameState, Zone};
use crate::trigger::{PendingTrigger, TriggerCondition};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CerberusCaptureV1 {
    pub history_index: u64,
    pub begin_history_index: u64,
    pub observer_authority_index: u64,
    pub source: AbilitySourceContractV4,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CerberusOperationKindV1 {
    Resolved {
        morbid: bool,
        before_tapped: Option<bool>,
        before_stun: Option<i16>,
        after_tapped: Option<bool>,
        after_stun: Option<i16>,
    },
    CounterCommitted,
}
fn pending_owners(state: &GameState) -> impl Iterator<Item = &PendingTrigger> {
    state
        .engine
        .pending_triggers
        .iter()
        .chain(state.legend_pending_v1.iter().flatten())
        .chain(
            state
                .pending_legend_rule_v1
                .iter()
                .flat_map(|pending| pending.waiting_triggers()),
        )
        .chain(
            state
                .standard_v1
                .iter()
                .filter_map(|standard| standard.resolution_play.as_ref())
                .flat_map(|resolution| resolution.deferred_triggers.iter()),
        )
}

fn owns_definition(card_def: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && CARD_DEFS.get(usize::from(card_def)).is_some_and(|def| {
        def.name == "Slumbering Cerberus"
            && def.is_executable()
            && crate::trigger::triggers_for(card_def)
                .iter()
                .any(|ability| {
                    ability.condition == TriggerCondition::BeginningAnyEndStepIfCreatureDied
                        && ability.home_zone == Zone::Battlefield
                        && (ability.effect)() == crate::slumbering_cerberus_v1::program_v1()
                })
    })
}
fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        CARD_DEFS
            .iter()
            .enumerate()
            .any(|(id, _)| owns_definition(id as u16))
    })
}
fn active(state: &GameState, source: ObjectId) -> bool {
    let object = state.objects.get(source);
    object.zone == Zone::Battlefield
        && object.v4.face_index == 0
        && owns_definition(object.card_def)
        && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
}
/// Called immediately after the genuine BeginningEndStep commitment.
pub(crate) fn record_begin(state: &mut GameState, begin_history_index: usize) {
    if !enabled()
        || !matches!(
            state.engine.event_history.get(begin_history_index),
            Some(CommittedEvent::BeginningEndStep {
                creature_died_this_turn: true,
                ..
            })
        )
    {
        return;
    }
    let sources: Vec<_> = state
        .objects
        .iter()
        .filter(|(id, _)| active(state, *id))
        .map(|(id, _)| AbilitySourceContractV4::capture(state, id))
        .collect();
    if sources.is_empty() {
        return;
    }
    let observer_authority_index = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(CommittedEvent::CerberusObserversV1 {
            begin_history_index: begin_history_index as u64,
            sources: sources.clone(),
        });
    for source in sources {
        let capture = CerberusCaptureV1 {
            history_index: state.engine.event_history.len() as u64,
            begin_history_index: begin_history_index as u64,
            observer_authority_index,
            source,
        };
        let event = CommittedEvent::CerberusCaptureV1 { capture };
        state.engine.event_log.push(event.clone());
        state.engine.event_history.push(event);
    }
}
pub(crate) fn capture_at(state: &GameState, index: u64) -> Result<&CerberusCaptureV1, String> {
    let index = usize::try_from(index).map_err(|_| "Cerberus capture index exceeds usize")?;
    let Some(CommittedEvent::CerberusCaptureV1 { capture }) = state.engine.event_history.get(index)
    else {
        return Err("Cerberus trigger lost its exact capture".into());
    };
    let begin =
        usize::try_from(capture.begin_history_index).map_err(|_| "begin index exceeds usize")?;
    let observer = usize::try_from(capture.observer_authority_index)
        .map_err(|_| "observer index exceeds usize")?;
    let Some(CommittedEvent::CerberusObserversV1 {
        begin_history_index,
        sources,
    }) = state.engine.event_history.get(observer)
    else {
        return Err("Cerberus capture lost original observers".into());
    };
    if begin >= observer
        || observer >= index
        || capture.history_index as usize != index
        || *begin_history_index != capture.begin_history_index
        || !owns_definition(capture.source.card_def)
        || capture.source.zone != Zone::Battlefield
        || sources
            .iter()
            .filter(|source| **source == capture.source)
            .count()
            != 1
        || !matches!(
            state.engine.event_history.get(begin),
            Some(CommittedEvent::BeginningEndStep {
                creature_died_this_turn: true,
                ..
            })
        )
    {
        return Err("Cerberus capture changed its end-step occurrence or source".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, capture.source)?;
    Ok(capture)
}
fn validate_observed_end_steps(state: &GameState) -> Result<(), ObjectId> {
    let mut seen_beginnings = std::collections::BTreeSet::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::CerberusObserversV1 {
            begin_history_index,
            sources,
        } = event
        else {
            continue;
        };
        if sources.is_empty()
            || !seen_beginnings.insert(*begin_history_index)
            || (*begin_history_index as usize).checked_add(1) != Some(index)
            || !matches!(
                state
                    .engine
                    .event_history
                    .get(*begin_history_index as usize),
                Some(CommittedEvent::BeginningEndStep {
                    creature_died_this_turn: true,
                    ..
                })
            )
        {
            return Err(sources.first().map_or(ObjectId(0), |source| source.source));
        }
        for (ordinal, source) in sources.iter().enumerate() {
            let captures: Vec<_> = state
                .engine
                .event_history
                .iter()
                .filter_map(|event| match event {
                    CommittedEvent::CerberusCaptureV1 { capture }
                        if capture.observer_authority_index as usize == index
                            && capture.source.source == source.source =>
                    {
                        Some(capture)
                    }
                    _ => None,
                })
                .collect();
            if !owns_definition(source.card_def)
                || source.zone != Zone::Battlefield
                || sources[..ordinal]
                    .iter()
                    .any(|prior| prior.source == source.source)
                || captures.len() != 1
                || captures[0].source != *source
                || captures[0].history_index as usize != index + 1 + ordinal
                || capture_at(state, captures[0].history_index).ok() != Some(captures[0])
            {
                return Err(source.source);
            }
        }
    }
    if state
        .engine
        .event_log
        .iter()
        .any(|event| matches!(event, CommittedEvent::CerberusObserversV1 { .. }))
    {
        return Err(ObjectId(0));
    }
    Ok(())
}

pub(crate) fn collect(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Result<Vec<PendingTrigger>, ObjectId> {
    if !enabled() {
        return Ok(Vec::new());
    }
    validate_observed_end_steps(state)?;
    let mut pending = Vec::new();
    let mut used = std::collections::BTreeSet::new();
    // A mutable collection log cannot silently omit an outstanding committed
    // capture. Collection receipts authenticate each completed dispatch once.
    for event in &state.engine.event_history {
        let CommittedEvent::CerberusCaptureV1 { capture } = event else {
            continue;
        };
        let collected = state
            .engine
            .event_history
            .iter()
            .filter(|event| {
                matches!(event, CommittedEvent::CerberusCollectedV1 { capture_history_index }
                if *capture_history_index == capture.history_index)
            })
            .count();
        if collected > 1 || (collected == 0 && !events.contains(event)) {
            return Err(capture.source.source);
        }
    }
    for event in events {
        let CommittedEvent::CerberusCaptureV1 { capture } = event else {
            continue;
        };
        let invalid = || capture.source.source;
        let authority = capture_at(state, capture.history_index).map_err(|_| invalid())?;
        if authority != capture
            || !used.insert(capture.history_index)
            || state.engine.event_history.iter().any(|event| {
                matches!(event, CommittedEvent::CerberusCollectedV1 { capture_history_index }
                    if *capture_history_index == capture.history_index)
            })
        {
            return Err(invalid());
        }
        pending.push(PendingTrigger {
            source: capture.source.source,
            controller: capture.source.controller,
            source_contract: Some(capture.source),
            granted_by: None,
            effect: EffectOp::CerberusUntapV1 {
                capture_history_index: capture.history_index,
            },
            is_madness_offer: false,
            kicked: false,
            target_spec: crate::card_def::TargetSpec::None,
            targets: Vec::new(),
            target_contracts: Vec::new(),
            placement_ordered: false,
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        });
    }
    for capture_history_index in used {
        state
            .engine
            .event_history
            .push(CommittedEvent::CerberusCollectedV1 {
                capture_history_index,
            });
    }
    Ok(pending)
}

pub(crate) fn validate_source(
    state: &GameState,
    effect: &EffectOp,
    source: Option<AbilitySourceContractV4>,
) -> Result<(), String> {
    let index = match effect {
        EffectOp::CerberusUntapV1 {
            capture_history_index,
        } => *capture_history_index,
        _ if source.is_some_and(|source| owns_definition(source.card_def))
            && *effect == crate::slumbering_cerberus_v1::program_v1() =>
        {
            return Err("Cerberus printed trigger has no captured occurrence".into())
        }
        _ => return Ok(()),
    };
    let capture = capture_at(state, index)?;
    if source != Some(capture.source) || state.engine.event_history.iter().filter(|event| {
        matches!(event, CommittedEvent::CerberusCollectedV1 { capture_history_index } if *capture_history_index == index)
    }).count() != 1 { return Err("Cerberus end-step trigger changed its captured source or collection".into()); }
    if state.engine.event_history.iter().any(|event| matches!(event,
        CommittedEvent::CerberusCompletedV1 { capture_history_index, .. } if *capture_history_index == index)) {
        return Err("completed Cerberus end-step trigger was replayed".into());
    }
    Ok(())
}

fn publication(
    state: &GameState,
    capture_index: u64,
) -> Result<(usize, &crate::state::StackItem), String> {
    let publications: Vec<_> = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            CommittedEvent::CerberusPublishedV1 {
                capture_history_index,
                item,
            } if *capture_history_index == capture_index => Some((index, item)),
            _ => None,
        })
        .collect();
    let [published] = publications.as_slice() else {
        return Err("Cerberus end-step trigger lost its exact publication".into());
    };
    let capture = capture_at(state, capture_index)?;
    let item = published.1;
    if item.source != capture.source.source
        || item.controller != capture.source.controller
        || item.v4.ability_source_contract != Some(capture.source)
        || item.v4.stack_item_id == crate::ids::StackItemId::default()
        || item.inline_effect
            != Some(EffectOp::CerberusUntapV1 {
                capture_history_index: capture_index,
            })
        || item.kind != crate::state::StackItemKind::TriggeredAbility
        || !item.targets.is_empty()
        || !item.v4.target_contracts.is_empty()
        || item.is_copy
        || item.is_flashback
        || item.madness_offer
    {
        return Err("Cerberus end-step publication changed its original finalized trigger".into());
    }
    Ok(*published)
}

/// Called only after the genuine engine trigger publisher finalized the row.
pub(crate) fn publish(state: &mut GameState, item: &crate::state::StackItem) -> Result<(), String> {
    let Some(
        effect @ EffectOp::CerberusUntapV1 {
            capture_history_index,
        },
    ) = item.inline_effect.as_ref()
    else {
        return Ok(());
    };
    validate_source(state, effect, item.v4.ability_source_contract)?;
    if state.engine.event_history.iter().any(|event| matches!(event,
        CommittedEvent::CerberusPublishedV1 { capture_history_index: index, .. } if index == capture_history_index)) {
        return Err("Cerberus end-step trigger was published twice".into());
    }
    state
        .engine
        .event_history
        .push(CommittedEvent::CerberusPublishedV1 {
            capture_history_index: *capture_history_index,
            item: item.clone(),
        });
    Ok(())
}

/// Called only at actual engine resolution/counter completion after removal.
pub(crate) fn finish(state: &mut GameState, item: &crate::state::StackItem) {
    let Some(
        effect @ EffectOp::CerberusUntapV1 {
            capture_history_index,
        },
    ) = item.inline_effect.as_ref()
    else {
        return;
    };
    if state.engine.halted.is_some() {
        return;
    }
    if validate_source(state, effect, item.v4.ability_source_contract).is_err()
        || item.v4.stack_item_id == crate::ids::StackItemId::default()
        || publication(state, *capture_history_index).map_or(true, |(_, original)| original != item)
    {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            item.source,
        ));
        return;
    }
    let operations: Vec<_> = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(index, event)| {
            matches!(event, CommittedEvent::CerberusOperationV1 {
            capture_history_index: capture, stack_item_id, source, ..
        } if capture == capture_history_index && *stack_item_id == item.v4.stack_item_id
            && Some(*source) == item.v4.ability_source_contract)
            .then_some(index)
        })
        .collect();
    let [operation_index] = operations.as_slice() else {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            item.source,
        ));
        return;
    };
    if validate_operation(state, *operation_index).is_err() {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            item.source,
        ));
        return;
    }
    state
        .engine
        .event_history
        .push(CommittedEvent::CerberusCompletedV1 {
            capture_history_index: *capture_history_index,
            stack_item_id: item.v4.stack_item_id,
            operation_history_index: u64::try_from(*operation_index).expect("bounded history"),
        });
}

/// The accepted counter primitive calls this with its validated removed item.
/// A completion marker alone cannot impersonate this committed operation.
pub(crate) fn record_countered(state: &mut GameState, item: &crate::state::StackItem) {
    let Some(EffectOp::CerberusUntapV1 {
        capture_history_index,
    }) = item.inline_effect.as_ref()
    else {
        return;
    };
    let Some(source) = item.v4.ability_source_contract else {
        return;
    };
    if publication(state, *capture_history_index).map_or(true, |(_, original)| original != item) {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            item.source,
        ));
        return;
    }
    state
        .engine
        .event_history
        .push(CommittedEvent::CerberusOperationV1 {
            capture_history_index: *capture_history_index,
            stack_item_id: item.v4.stack_item_id,
            source,
            kind: CerberusOperationKindV1::CounterCommitted,
        });
}

pub(crate) fn resolve(state: &mut GameState, ctx: &ExecCtx, capture_history_index: u64) {
    let effect = EffectOp::CerberusUntapV1 {
        capture_history_index,
    };
    if validate_source(state, &effect, ctx.ability_source_contract).is_err() {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
        return;
    }
    let capture = capture_at(state, capture_history_index).unwrap();
    if ctx.source != capture.source.source
        || ctx.controller != capture.source.controller
        || !ctx.targets.is_empty()
        || publication(state, capture_history_index).map_or(true, |(_, item)| {
            Some(item.v4.stack_item_id) != ctx.stack_item_id
                || item.v4.ability_source_contract != ctx.ability_source_contract
                || item.v4.target_contracts != ctx.target_contracts
        })
        || ctx
            .stack_item_id
            .is_none_or(|id| id == crate::ids::StackItemId::default())
    {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
        return;
    }
    let source = capture.source;
    let morbid = state.creature_died_this_turn_v1();
    let same_source = state.objects.try_get(source.source).filter(|live| {
        live.zone == Zone::Battlefield
            && live.owner == source.owner
            && live.zone_change_count == source.zone_change_count
    });
    let before_tapped = same_source.map(|live| live.tapped);
    let before_stun = same_source.map(|live| live.counters.stun);
    if morbid && same_source.is_some() {
        crate::slumbering_cerberus_v1::untap_captured_source(ctx, state);
    }
    if state.engine.halted.is_some() {
        return;
    }
    let after = state.objects.try_get(source.source).filter(|live| {
        live.zone == Zone::Battlefield
            && live.owner == source.owner
            && live.zone_change_count == source.zone_change_count
    });
    state
        .engine
        .event_history
        .push(CommittedEvent::CerberusOperationV1 {
            capture_history_index,
            stack_item_id: ctx.stack_item_id.unwrap(),
            source,
            kind: CerberusOperationKindV1::Resolved {
                morbid,
                before_tapped,
                before_stun,
                after_tapped: after.map(|live| live.tapped),
                after_stun: after.map(|live| live.counters.stun),
            },
        });
}

fn validate_operation(state: &GameState, index: usize) -> Result<(), String> {
    let Some(CommittedEvent::CerberusOperationV1 {
        capture_history_index,
        stack_item_id,
        source,
        kind,
    }) = state.engine.event_history.get(index)
    else {
        return Err("completion lost its committed untap/counter operation".into());
    };
    let capture = capture_at(state, *capture_history_index)?;
    let (published_index, published) = publication(state, *capture_history_index)?;
    let collections: Vec<_> = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(index, event)| {
            matches!(event, CommittedEvent::CerberusCollectedV1 { capture_history_index: capture }
            if capture == capture_history_index)
            .then_some(index)
        })
        .collect();
    let [collection] = collections.as_slice() else {
        return Err("operation lost its exact collection".into());
    };
    if *collection >= index
        || published_index <= *collection
        || published_index >= index
        || published.v4.stack_item_id != *stack_item_id
        || *source != capture.source
        || *stack_item_id == crate::ids::StackItemId::default()
    {
        return Err("operation changed its original source/stack/collection occurrence".into());
    }
    if let CerberusOperationKindV1::Resolved {
        morbid,
        before_tapped,
        before_stun,
        after_tapped,
        after_stun,
    } = kind
    {
        let expected = match (*morbid, *before_tapped, *before_stun) {
            (_, None, None) => (None, None),
            (false, Some(tapped), Some(stun)) => (Some(tapped), Some(stun)),
            (true, Some(false), Some(stun)) => (Some(false), Some(stun)),
            (true, Some(true), Some(stun)) if stun > 0 => (Some(true), Some(stun - 1)),
            (true, Some(true), Some(stun)) if stun <= 0 => (Some(false), Some(stun)),
            _ => return Err("Cerberus operation source facts are incomplete".into()),
        };
        if (*after_tapped, *after_stun) != expected {
            return Err("Cerberus operation changed its actual untap outcome".into());
        }
    }

    Ok(())
}

pub(crate) fn requires_successor(state: &GameState) -> bool {
    if !enabled() {
        return false;
    }
    state
        .objects
        .iter()
        .any(|(source, _)| active(state, source))
        || state.stack.iter().any(|item| {
            item.inline_effect
                .as_ref()
                .is_some_and(|op| matches!(op, EffectOp::CerberusUntapV1 { .. }))
        })
        || pending_owners(state)
            .any(|pending| matches!(pending.effect, EffectOp::CerberusUntapV1 { .. }))
        || state
            .engine
            .event_log
            .iter()
            .any(|event| matches!(event, CommittedEvent::CerberusCaptureV1 { .. }))
        || state
            .engine
            .event_history
            .iter()
            .any(|event| matches!(event, CommittedEvent::CerberusCaptureV1 { .. }))
}

/// Common read-only action/restore boundary. Mutable pending and stack rows
/// must retain the independently committed original observer capture.
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if !enabled() {
        return Ok(());
    }
    validate_observed_end_steps(state)
        .map_err(|_| "Cerberus end-step observer authority is incomplete")?;
    for event in &state.engine.event_history {
        let CommittedEvent::CerberusCaptureV1 { capture } = event else {
            continue;
        };
        if capture_at(state, capture.history_index)? != capture {
            return Err("Cerberus end-step capture changed".into());
        }
        let collected = state
            .engine
            .event_history
            .iter()
            .filter(|event| {
                matches!(event,
            CommittedEvent::CerberusCollectedV1 { capture_history_index }
                if *capture_history_index == capture.history_index)
            })
            .count();
        if collected > 1
            || (collected == 1 && state.engine.event_log.iter().any(|logged| logged == event))
            || (collected == 0
                && state
                    .engine
                    .event_log
                    .iter()
                    .filter(|logged| *logged == event)
                    .count()
                    != 1)
        {
            return Err("Cerberus end-step capture collection is missing or repeated".into());
        }
        let completed = state
            .engine
            .event_history
            .iter()
            .filter(|event| {
                matches!(event,
            CommittedEvent::CerberusCompletedV1 { capture_history_index, .. }
            if *capture_history_index == capture.history_index)
            })
            .count();
        let operations = state
            .engine
            .event_history
            .iter()
            .filter(|event| {
                matches!(event,
            CommittedEvent::CerberusOperationV1 { capture_history_index, .. }
            if *capture_history_index == capture.history_index)
            })
            .count();
        let pending = pending_owners(state)
            .filter(|pending| {
                matches!(pending.effect,
            EffectOp::CerberusUntapV1 { capture_history_index }
            if capture_history_index == capture.history_index)
            })
            .count();
        let live = state
            .stack
            .iter()
            .filter(|item| {
                matches!(item.inline_effect,
            Some(EffectOp::CerberusUntapV1 { capture_history_index })
            if capture_history_index == capture.history_index)
            })
            .count();
        let publications = state
            .engine
            .event_history
            .iter()
            .filter(|event| {
                matches!(event,
            CommittedEvent::CerberusPublishedV1 { capture_history_index, .. }
            if *capture_history_index == capture.history_index)
            })
            .count();
        if completed > 1
            || operations != completed
            || publications != usize::from(live + completed > 0)
            || (collected == 0 && (completed != 0 || pending + live != 0))
            || (collected == 1 && pending + live != usize::from(completed == 0))
        {
            return Err("Cerberus end-step trigger lost or replayed its dispatch ownership".into());
        }
    }
    for (completion_index, event) in state.engine.event_history.iter().enumerate() {
        if let CommittedEvent::CerberusCompletedV1 {
            capture_history_index,
            stack_item_id,
            operation_history_index,
        } = event
        {
            capture_at(state, *capture_history_index)?;
            let operation_index = usize::try_from(*operation_history_index)
                .map_err(|_| "operation index exceeds usize")?;
            validate_operation(state, operation_index)?;
            if operation_index >= completion_index
                || !matches!(state.engine.event_history.get(operation_index),
                Some(CommittedEvent::CerberusOperationV1 { capture_history_index: capture, stack_item_id: id, .. })
                if capture == capture_history_index && id == stack_item_id)
            {
                return Err("completion changed its exact committed operation".into());
            }
        }
        if matches!(event, CommittedEvent::CerberusOperationV1 { .. }) {
            validate_operation(state, completion_index)?;
        }
        if let CommittedEvent::CerberusPublishedV1 {
            capture_history_index,
            ..
        } = event
        {
            let (index, _) = publication(state, *capture_history_index)?;
            if index != completion_index || state.engine.event_history[..index].iter().filter(|event|
                matches!(event, CommittedEvent::CerberusCollectedV1 { capture_history_index: capture }
                    if capture == capture_history_index)).count() != 1 {
                return Err("Cerberus end-step publication lost its prior collection".into());
            }
        }
    }
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::CerberusCompletedV1 { .. }
                | CommittedEvent::CerberusOperationV1 { .. }
                | CommittedEvent::CerberusCollectedV1 { .. }
                | CommittedEvent::CerberusPublishedV1 { .. }
        )
    }) {
        return Err("Cerberus end-step completion was inserted into dispatch log".into());
    }
    for event in &state.engine.event_log {
        if let CommittedEvent::CerberusCaptureV1 { capture } = event {
            if capture_at(state, capture.history_index)? != capture {
                return Err("dispatch log contains a forged Cerberus end-step capture".into());
            }
        }
    }
    for pending in pending_owners(state) {
        if let EffectOp::CerberusUntapV1 {
            capture_history_index,
        } = &pending.effect
        {
            validate_source(state, &pending.effect, pending.source_contract)?;
            let capture = capture_at(state, *capture_history_index)?;
            if pending.source != capture.source.source
                || pending.controller != capture.source.controller
            {
                return Err("Cerberus end-step pending trigger changed its beneficiary".into());
            }
            crate::engine::validate_pending_trigger_identity(state, pending)?;
            if pending.placement_ordered {
                crate::engine::validate_pending_trigger(state, pending)?;
            }
        }
    }
    for item in &state.stack {
        if let Some(EffectOp::CerberusUntapV1 {
            capture_history_index,
        }) = &item.inline_effect
        {
            let capture = capture_at(state, *capture_history_index)?;
            if publication(state, *capture_history_index)?.1 != item {
                return Err(
                    "live Cerberus end-step trigger changed its committed publication".into(),
                );
            }
            if item.source != capture.source.source || item.controller != capture.source.controller
            {
                return Err("Cerberus end-step stack trigger changed its beneficiary".into());
            }
            crate::engine::validated_stack_item_target_spec(item, state)?;
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
