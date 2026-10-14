//! Public Panda occurrences. Original replay worlds and executable owners remain private.
use crate::effect::EffectOp;
use crate::event::CommittedEvent;
use crate::fiendish_panda_v1::{
    self as panda,
    custody::{JournalV1, RemovalV1},
};
use crate::rl::{CardStableRefV1, PlayerSeatV1, RlContractError};
use crate::rules_meaning_fiendish_panda_v1 as meaning;
use crate::state::{AbilitySourceContractV4, GameState, StackItem, StackTargetContractV4, Zone};
use crate::trigger::PendingTrigger;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/fiendish-panda/public/v47";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhaseV47 {
    Fresh,
    Held,
    Unavailable,
    Published,
    Resolved,
    Countered,
    Fizzled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncarnationV47 {
    pub object: u32,
    pub generation: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OccurrenceV47 {
    /// Ordinal among public Panda occurrences, never an executable history index.
    pub occurrence: usize,
    pub source: CardStableRefV1,
    pub current_original_source: Option<CardStableRefV1>,
    pub original_controller: PlayerSeatV1,
    pub root: meaning::RootV1,
    pub phase: PhaseV47,
    pub life_gained: Option<i32>,
    pub captured_power: Option<i32>,
    /// Identity that was public in the graveyard when the actual target was selected.
    pub original_target: Option<CardStableRefV1>,
    pub operation_attempted: bool,
    /// Actual replacement-adjusted committed placement, distinct from life amount.
    pub actual_counters_added: i32,
    /// Actual returned object incarnation; no later hidden definition is consulted.
    pub returned_incarnation: Option<IncarnationV47>,
    pub return_destination: Option<Zone>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerOrderV47 {
    pub controller: PlayerSeatV1,
    pub group_position: usize,
    pub occurrence: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV47 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub meanings: Vec<meaning::MeaningV1>,
    pub occurrences: Vec<OccurrenceV47>,
    pub trigger_order: Vec<TriggerOrderV47>,
}
pub fn producer_sha256() -> String {
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("panda_public_v47.rs"),
        include_str!("sower_restriction_capture_v1.rs"),
    ] {
        hash.update((source.len() as u64).to_le_bytes());
        hash.update(source.as_bytes());
    }
    hash.update(meaning::producer_sha256().as_bytes());
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
fn binding(effect: &EffectOp) -> Option<u64> {
    match effect {
        EffectOp::FiendishPandaCounterFromCaptureV1 {
            capture_history_index,
        }
        | EffectOp::FiendishPandaReturnFromCaptureV1 {
            capture_history_index,
            ..
        } => Some(*capture_history_index),
        _ => None,
    }
}
/// Shape selector only. Public source helpers require the actual validated owner.
pub fn selected(item: &StackItem) -> bool {
    item.inline_effect.as_ref().and_then(binding).is_some()
}
fn captured_source(source: AbilitySourceContractV4) -> CardStableRefV1 {
    CardStableRefV1 {
        arena_id: source.source.0,
        card_db_id: source.card_def,
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: source.zone,
        zone_change_count: source.zone_change_count,
    }
}
fn event(state: &GameState, index: u64) -> Result<&CommittedEvent, RlContractError> {
    state
        .engine
        .event_history
        .get(
            usize::try_from(index)
                .map_err(|_| RlContractError("Panda history index exceeds usize".into()))?,
        )
        .ok_or_else(|| RlContractError("Panda original public occurrence absent".into()))
}
fn journal(state: &GameState, index: u64) -> Result<&JournalV1, RlContractError> {
    match event(state, index)? {
        CommittedEvent::FiendishPandaJournalV1 { journal } => Ok(journal.as_ref()),
        _ => Err(RlContractError(
            "Panda public occurrence lost its private validated journal".into(),
        )),
    }
}
fn source_at(state: &GameState, index: u64) -> Result<AbilitySourceContractV4, RlContractError> {
    match event(state, index)? {
        CommittedEvent::FiendishPandaLifeGainCaptureV1 { source, .. }
        | CommittedEvent::FiendishPandaDeathCaptureV1 { source, .. } => Ok(*source),
        _ => Err(RlContractError(
            "Panda public source names a foreign capture".into(),
        )),
    }
}
fn waiting(state: &GameState) -> impl Iterator<Item = &PendingTrigger> {
    state
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
}
pub fn stack_source_reference(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    let Some(capture) = item.inline_effect.as_ref().and_then(binding) else {
        return Ok(None);
    };
    panda::validate_state(state).map_err(RlContractError)?;
    if !state.stack.iter().any(|original| original == item) {
        return Err(RlContractError(
            "Panda public stack source requires its genuine complete live row".into(),
        ));
    }
    Ok(Some(captured_source(source_at(state, capture)?)))
}
pub fn pending_source_reference(
    state: &GameState,
    pending: &PendingTrigger,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    let Some(capture) = binding(&pending.effect) else {
        return Ok(None);
    };
    panda::validate_state(state).map_err(RlContractError)?;
    if !waiting(state).any(|original| original == pending) {
        return Err(RlContractError(
            "Panda public waiting source requires its genuine complete pending owner".into(),
        ));
    }
    Ok(Some(captured_source(source_at(state, capture)?)))
}
fn original_target(
    contracts: &[StackTargetContractV4],
) -> Result<Option<CardStableRefV1>, RlContractError> {
    match contracts {
        [] => Ok(None),
        [StackTargetContractV4::Object {
            object,
            card_def,
            owner,
            controller,
            zone: Zone::Graveyard,
            zone_change_count,
            ..
        }] => Ok(Some(CardStableRefV1 {
            arena_id: object.0,
            card_db_id: *card_def,
            owner: (*owner).into(),
            controller: (*controller).into(),
            zone: Zone::Graveyard,
            zone_change_count: *zone_change_count,
        })),
        _ => Err(RlContractError(
            "Panda public target lost its original graveyard incarnation".into(),
        )),
    }
}
pub fn capture(state: &GameState) -> Result<FactsV47, RlContractError> {
    panda::validate_state(state).map_err(RlContractError)?;
    let mut meanings = Vec::new();
    for (id, _) in crate::card_def::CARD_DEFS.iter().enumerate() {
        let id =
            u16::try_from(id).map_err(|_| RlContractError("Panda catalog exceeds u16".into()))?;
        if panda::owns_definition(id) {
            for root in [
                meaning::RootV1::CreatureSpell,
                meaning::RootV1::ControllerLifeGain,
                meaning::RootV1::SourceDeath,
            ] {
                meanings.push(meaning::printed_meaning(id, root).map_err(RlContractError)?);
            }
        }
    }
    let captures = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, entry)| {
            matches!(
                entry,
                CommittedEvent::FiendishPandaLifeGainCaptureV1 { .. }
                    | CommittedEvent::FiendishPandaDeathCaptureV1 { .. }
            )
            .then_some(at as u64)
        })
        .collect::<Vec<_>>();
    let mut occurrences = Vec::new();
    for (occurrence, capture) in captures.iter().copied().enumerate() {
        let source = source_at(state, capture)?;
        let (root, life_gained, captured_power) = match event(state, capture)? {
            CommittedEvent::FiendishPandaLifeGainCaptureV1 { amount, .. } => {
                (meaning::RootV1::ControllerLifeGain, Some(*amount), None)
            }
            CommittedEvent::FiendishPandaDeathCaptureV1 { power, .. } => {
                (meaning::RootV1::SourceDeath, None, Some(*power))
            }
            _ => unreachable!("filtered validated Panda capture"),
        };
        let pending = waiting(state).find(|p| binding(&p.effect) == Some(capture));
        let mut row = OccurrenceV47 {
            occurrence,
            source: captured_source(source),
            current_original_source: state
                .objects
                .try_get(source.source)
                .filter(|live| {
                    live.zone == Zone::Battlefield
                        && live.owner == source.owner
                        && live.zone_change_count == source.zone_change_count
                })
                .map(|live| crate::sower_restriction_capture_v1::masked_ref(source.source, live))
                .transpose()?,
            original_controller: source.controller.into(),
            root,
            phase: if pending.is_some() {
                PhaseV47::Held
            } else {
                PhaseV47::Fresh
            },
            life_gained,
            captured_power,
            original_target: pending
                .map(|p| original_target(&p.target_contracts))
                .transpose()?
                .flatten(),
            operation_attempted: false,
            actual_counters_added: 0,
            returned_incarnation: None,
            return_destination: None,
        };
        let final_phase = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .find_map(|(at, entry)| match entry {
                CommittedEvent::FiendishPandaJournalV1 { journal } => match journal.as_ref() {
                    JournalV1::Unavailable {
                        capture: original, ..
                    }
                    | JournalV1::Published {
                        capture: original, ..
                    } if *original == capture => Some((at as u64, journal.as_ref())),
                    _ => None,
                },
                _ => None,
            });
        if let Some((publication, final_phase)) = final_phase {
            match final_phase {
                JournalV1::Unavailable { .. } => row.phase = PhaseV47::Unavailable,
                JournalV1::Published { item, .. } => {
                    row.phase = PhaseV47::Published;
                    row.original_target = original_target(&item.v4.target_contracts)?;
                    let removal =
                        state
                            .engine
                            .event_history
                            .iter()
                            .enumerate()
                            .find_map(|(at, entry)| match entry {
                                CommittedEvent::FiendishPandaJournalV1 { journal } => {
                                    match journal.as_ref() {
                                        JournalV1::Removed {
                                            publication: original,
                                            kind,
                                            ..
                                        } if *original == publication => Some((at as u64, *kind)),
                                        _ => None,
                                    }
                                }
                                _ => None,
                            });
                    if let Some((removal, kind)) = removal {
                        row.phase = match kind {
                            RemovalV1::Resolved => PhaseV47::Resolved,
                            RemovalV1::Countered => PhaseV47::Countered,
                            RemovalV1::NoLegalTarget => PhaseV47::Fizzled,
                        };
                        let operation = state
                            .engine
                            .event_history
                            .iter()
                            .find_map(|entry| match entry {
                                CommittedEvent::FiendishPandaJournalV1 { journal } => {
                                    match journal.as_ref() {
                                        JournalV1::Closed {
                                            removal: original,
                                            operation,
                                        } if *original == removal => Some(*operation),
                                        _ => None,
                                    }
                                }
                                _ => None,
                            })
                            .ok_or_else(|| {
                                RlContractError(
                                    "Panda public resolution lost validated closure".into(),
                                )
                            })?;
                        if let Some(operation) = operation {
                            row.operation_attempted = true;
                            let JournalV1::Operation { input, after } = journal(state, operation)?
                            else {
                                return Err(RlContractError(
                                    "Panda public operation changed".into(),
                                ));
                            };
                            if root == meaning::RootV1::ControllerLifeGain {
                                let start = usize::try_from(*input).map_err(|_| {
                                    RlContractError("Panda input exceeds usize".into())
                                })?;
                                let end = usize::try_from(operation).map_err(|_| {
                                    RlContractError("Panda operation exceeds usize".into())
                                })?;
                                for entry in &state.engine.event_history[start + 1..end] {
                                    if let CommittedEvent::PlusOneCountersAdded {
                                        object,
                                        zone_change_count,
                                        count,
                                        ..
                                    } = entry
                                    {
                                        if *object == source.source
                                            && *zone_change_count == source.zone_change_count
                                        {
                                            row.actual_counters_added = row
                                                .actual_counters_added
                                                .checked_add(*count)
                                                .ok_or_else(|| {
                                                    RlContractError(
                                                        "Panda public counter count overflow"
                                                            .into(),
                                                    )
                                                })?;
                                        }
                                    }
                                }
                            } else if let Some(after) = after {
                                let target = row.original_target.as_ref().ok_or_else(|| {
                                    RlContractError(
                                        "Panda return operation has no public original target"
                                            .into(),
                                    )
                                })?;
                                row.return_destination = Some(after.zone);
                                if after.zone == Zone::Battlefield {
                                    row.returned_incarnation = Some(IncarnationV47 {
                                        object: target.arena_id,
                                        generation: after.zone_change_count,
                                    });
                                }
                            }
                        }
                    }
                }
                _ => unreachable!("filtered validated final Panda phase"),
            }
        }
        occurrences.push(row);
    }
    let mut positions = [0usize; 2];
    let mut trigger_order = Vec::new();
    for pending in &state.engine.pending_triggers {
        let group_position = positions[pending.controller.index()];
        positions[pending.controller.index()] += 1;
        if let Some(capture) = binding(&pending.effect) {
            let occurrence = captures
                .iter()
                .position(|original| *original == capture)
                .ok_or_else(|| {
                    RlContractError("Panda ordering original occurrence absent".into())
                })?;
            trigger_order.push(TriggerOrderV47 {
                controller: pending.controller.into(),
                group_position,
                occurrence,
            });
        }
    }
    Ok(FactsV47 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        meanings,
        occurrences,
        trigger_order,
    })
}
pub fn validate(state: &GameState, facts: &FactsV47) -> Result<(), RlContractError> {
    if capture(state)? != *facts {
        return Err(RlContractError(
            "Panda V47 complete public facts changed".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
