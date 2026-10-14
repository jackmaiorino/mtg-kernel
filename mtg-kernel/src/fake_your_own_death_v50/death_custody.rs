//! Original gained-ability collection for the still-unregistered printing.
//! Collection is not publication or permission to execute the private root.

use super::spell_custody::{emit, journal, JournalV50};
use crate::effect::EffectOp;
use crate::event::CommittedEvent;
use crate::ids::ObjectId;
use crate::state::GameState;
use crate::trigger::PendingTrigger;

fn captures_before(state: &GameState, end: usize) -> Vec<u64> {
    state.engine.event_history.iter().take(end).enumerate().filter_map(|(index, event)| {
        (matches!(journal(event), Some(JournalV50::DeathCaptured { .. }))
            && !state.engine.event_history.iter().take(end).any(|event|
                matches!(journal(event), Some(JournalV50::DeathCollected {capture,..}) if *capture==index as u64)))
        .then_some(index as u64)
    }).collect()
}

/// Every original collection input consumes its complete earlier uncollected
/// capture roster, contiguously and once. No erased mutable receipt can make an
/// already collected gained ability available again.
pub(crate) fn validate_collection(state: &GameState) -> Result<(), String> {
    for (index, event) in state.engine.event_history.iter().enumerate() {
        match journal(event) {
            Some(JournalV50::CollectionInput { captures }) => {
                if captures.is_empty() || *captures != captures_before(state, index) {
                    return Err(
                        "Fake collection changed its complete original uncollected roster".into(),
                    );
                }
                for (ordinal, capture) in captures.iter().enumerate() {
                    if !matches!(state.engine.event_history.get(index+1+ordinal).and_then(journal),
                        Some(JournalV50::DeathCollected {input,ordinal:actual,capture:c})
                        if *input==index as u64 && *actual as usize==ordinal && c==capture)
                    {
                        return Err(
                            "Fake collection lost an original ordered capture consumption".into(),
                        );
                    }
                }
                if !matches!(state.engine.event_history.get(index+1+captures.len()).and_then(journal),
                    Some(JournalV50::CollectionCompleted {input}) if *input==index as u64)
                {
                    return Err(
                        "Fake collection lost its adjacent complete-roster completion".into(),
                    );
                }
            }
            Some(JournalV50::DeathCollected {
                input,
                ordinal,
                capture,
            }) => {
                let input_index =
                    usize::try_from(*input).map_err(|_| "Fake collection input index overflow")?;
                if input_index
                    .checked_add(1)
                    .and_then(|i| i.checked_add(*ordinal as usize))
                    != Some(index)
                    || !matches!(state.engine.event_history.get(input_index).and_then(journal),
                        Some(JournalV50::CollectionInput {captures}) if captures.get(*ordinal as usize)==Some(capture))
                {
                    return Err(
                        "Fake consumed capture lost its exact original collection position".into(),
                    );
                }
            }
            Some(JournalV50::CollectionCompleted { input }) => {
                let input_index = usize::try_from(*input)
                    .map_err(|_| "Fake collection completion index overflow")?;
                let Some(JournalV50::CollectionInput { captures }) = state
                    .engine
                    .event_history
                    .get(input_index)
                    .and_then(journal)
                else {
                    return Err("Fake collection completion lost its original input".into());
                };
                if input_index
                    .checked_add(1)
                    .and_then(|i| i.checked_add(captures.len()))
                    != Some(index)
                {
                    return Err("Fake collection completion changed its original adjacency".into());
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Derive mandatory gained triggers from independent committed history. Every
/// uncollected input and capture must appear exactly once in the current log;
/// consumed ones must be absent. Validate the complete batch before journaling
/// any collection or returning pending triggers for ordinary APNAP placement.
pub(crate) fn collect(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Result<Vec<PendingTrigger>, ObjectId> {
    if !state
        .engine
        .event_history
        .iter()
        .any(|e| journal(e).is_some())
        && !events.iter().any(|e| journal(e).is_some())
    {
        return Ok(Vec::new());
    }
    super::spell_custody::validate_grant_lifetime(state).map_err(|_| ObjectId(0))?;
    let captures = captures_before(state, state.engine.event_history.len());
    u32::try_from(captures.len()).map_err(|_| ObjectId(0))?;
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let needed = match journal(event) {
            Some(JournalV50::DeathCaptured { .. }) => captures.contains(&(index as u64)),
            Some(JournalV50::DeathInput { .. }) => captures.iter().any(|capture| {
                matches!(state.engine.event_history.get(*capture as usize).and_then(journal),
                    Some(JournalV50::DeathCaptured {input,..}) if *input==index as u64)
            }),
            _ => continue,
        };
        if events.iter().filter(|e| *e == event).count() != usize::from(needed) {
            return Err(ObjectId(0));
        }
    }
    for event in events.iter().filter(|e| journal(e).is_some()) {
        if !matches!(
            journal(event),
            Some(JournalV50::DeathInput { .. } | JournalV50::DeathCaptured { .. })
        ) || state
            .engine
            .event_history
            .iter()
            .filter(|e| *e == event)
            .count()
            != 1
        {
            return Err(ObjectId(0));
        }
    }
    let mut pending = Vec::with_capacity(captures.len());
    for capture in &captures {
        let Some(JournalV50::DeathCaptured { lookback, .. }) = state
            .engine
            .event_history
            .get(*capture as usize)
            .and_then(journal)
        else {
            return Err(ObjectId(0));
        };
        let source = lookback.battlefield_source;
        pending.push(PendingTrigger {
            controller: source.controller,
            source: source.source,
            effect: EffectOp::FakeDeathFromCaptureV50 { capture: *capture },
            is_madness_offer: false,
            kicked: false,
            target_spec: crate::card_def::TargetSpec::None,
            targets: Vec::new(),
            target_contracts: Vec::new(),
            placement_ordered: false,
            source_contract: Some(source),
            granted_by: None,
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        });
    }
    if !captures.is_empty() {
        let input = emit(
            state,
            JournalV50::CollectionInput {
                captures: captures.clone(),
            },
        );
        for (ordinal, capture) in captures.into_iter().enumerate() {
            emit(
                state,
                JournalV50::DeathCollected {
                    input,
                    ordinal: ordinal as u32,
                    capture,
                },
            );
        }
        emit(state, JournalV50::CollectionCompleted { input });
    }
    Ok(pending)
}
