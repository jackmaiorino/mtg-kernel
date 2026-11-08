//! Native committed debits and final trigger outcomes for the finite Greatwurm owner.
use crate::{
    effect::greatwurm_v1::UnitV1,
    engine::greatwurm_cast_v1::{self, PaidV1},
    event::CommittedEvent as E,
    event_history_v1::EventKindV1,
    ids::{PlayerId, StackItemId},
    state::{AbilitySourceContractV4, GameState},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebitV1 {
    pub unit: UnitV1,
    pub owner: PlayerId,
    pub controller: PlayerId,
    pub public_printing: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_face_down: Option<bool>,
    pub before: u32,
    pub after: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum EndV1 {
    Countered,
    OriginalCreatureGone,
    Resolved {
        controller: PlayerId,
        before: i32,
        after: i32,
        placement_history_index: Option<u64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum DispatchV1 {
    Countered,
    Resolve { recipient: Option<(PlayerId, i32)> },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum JournalV1 {
    Debited {
        id: StackItemId,
        units: Box<[DebitV1; 6]>,
    },
    Ended {
        id: StackItemId,
        result: EndV1,
    },
    Dispatched {
        id: StackItemId,
        operation: DispatchV1,
    },
}

fn record(state: &mut GameState, journal: JournalV1) {
    state
        .engine
        .event_history
        .push(E::GreatwurmCommitReceiptV1 {
            journal: Box::new(journal),
        });
}

pub(crate) fn record_debit(state: &mut GameState, id: StackItemId, units: [DebitV1; 6]) {
    record(
        state,
        JournalV1::Debited {
            id,
            units: Box::new(units),
        },
    );
}

pub(crate) fn record_end(state: &mut GameState, id: StackItemId, result: EndV1) {
    if state.greatwurm_commit_receipts_v1 {
        record(state, JournalV1::Ended { id, result });
    }
}

pub(crate) fn record_dispatch(state: &mut GameState, id: StackItemId, operation: DispatchV1) {
    if state.greatwurm_commit_receipts_v1 {
        record(state, JournalV1::Dispatched { id, operation });
    }
}

pub(crate) fn capture_placement(
    state: &GameState,
    creature: AbilitySourceContractV4,
    controller: PlayerId,
    start: usize,
    before: i32,
) -> Result<EndV1, String> {
    let live = state.objects.get(creature.source);
    let after = live.counters.plus1_plus1;
    let difference = after
        .checked_sub(before)
        .filter(|&n| n >= 0)
        .ok_or("Greatwurm actual +1/+1 placement decreased")?;
    let mut candidates = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .skip(start)
        .filter_map(|(at, event)| match event {
            E::PlusOneCountersAdded {
                object,
                zone_change_count,
                player,
                count,
                ..
            } if *object == creature.source
                && *zone_change_count == creature.zone_change_count
                && *player == controller =>
            {
                Some((at, *count))
            }
            _ => None,
        });
    let placement_history_index = match (difference, candidates.next(), candidates.next()) {
        (0, None, None) => None,
        (actual, Some((at, count)), None) if actual == count => {
            Some(u64::try_from(at).map_err(|_| "Greatwurm actual placement history width")?)
        }
        _ => return Err("Greatwurm realized placement differs from native counter event".into()),
    };
    Ok(EndV1::Resolved {
        controller,
        before,
        after,
        placement_history_index,
    })
}

fn validate_debit_face(state: &GameState, at: usize, debit: &DebitV1) -> Result<(), String> {
    let invalid = || "Greatwurm debit lost its event-time public face".to_string();
    let binding = debit.unit.permanent;
    if let Some(disguised) = debit.original_face_down {
        let name = if disguised {
            "Face-down creature"
        } else {
            "Face-down card"
        };
        if crate::card_def::card_id_by_name(name) != Some(debit.public_printing) {
            return Err(invalid());
        }
    }
    let live = state.objects.try_get(binding.object).ok_or_else(invalid)?;
    if live.zone_change_count != binding.expected_zone_change_count {
        return Ok(());
    }
    let turned_up = state.engine.event_history.iter().skip(at + 1).any(|event| matches!(
        event, E::TurnedFaceUp { object, zone_change_count }
            if *object == binding.object && *zone_change_count == binding.expected_zone_change_count
    ));
    if turned_up {
        if debit.original_face_down.is_none() {
            return Err(invalid());
        }
    } else if debit.original_face_down != live.v4.face_down_v1.map(|face| face.disguised)
        || crate::rl::projected_card_def(state, binding.object, binding.expected_zone_change_count)
            != debit.public_printing
            && !(debit.original_face_down.is_none()
                && crate::standard_cards_v1::had_card_def(
                    state,
                    binding.object,
                    binding.expected_zone_change_count,
                    debit.public_printing,
                ))
    {
        return Err(invalid());
    }
    Ok(())
}

pub(crate) fn debits(state: &GameState) -> Result<Vec<(&PaidV1, &[DebitV1; 6])>, String> {
    let history = state.engine.event_history.scan_for_v1(&[
        EventKindV1::GreatwurmCounterInputV1,
        EventKindV1::GreatwurmCounterPaymentV1,
        EventKindV1::GreatwurmCommitReceiptV1,
    ]);
    let mut debited = BTreeMap::new();
    let mut input = BTreeSet::new();
    let mut paid = BTreeSet::new();
    let mut output = Vec::new();
    for (at, event) in history.iter().enumerate() {
        match event {
            E::GreatwurmCounterInputV1 { input: original } => {
                if !input.insert(original.stack_item_id) {
                    return Err("Greatwurm original counter input was duplicated".into());
                }
            }
            E::GreatwurmCommitReceiptV1 { journal } => {
                if let JournalV1::Debited { id, units } = journal.as_ref() {
                    if !input.contains(id) || debited.insert(*id, (at, units.as_ref())).is_some() {
                        return Err("Greatwurm actual debit lost unique native input".into());
                    }
                }
            }
            E::GreatwurmCounterPaymentV1 { paid: receipt, .. } => {
                let actual = greatwurm_cast_v1::authenticated_paid(state, at)?;
                let id = receipt.stack_item_id;
                let (debit_at, units) = debited
                    .remove(&id)
                    .ok_or("Greatwurm paid spell lacks the actual six debits")?;
                if debit_at.checked_add(1) != Some(at) || !paid.insert(id) {
                    return Err("Greatwurm native debit is not paired with its own cast".into());
                }
                for (index, unit) in units.iter().enumerate() {
                    let previous = units[..index]
                        .iter()
                        .rev()
                        .find(|earlier| earlier.unit == unit.unit);
                    let original_count =
                        previous.map_or(actual.original_counts[index], |earlier| earlier.after);
                    if unit.unit != actual.units[index]
                        || unit.before != original_count
                        || unit.before.checked_sub(1) != Some(unit.after)
                        || unit.controller != actual.source.controller
                        || state
                            .objects
                            .try_get(unit.unit.permanent.object)
                            .is_none_or(|live| {
                                live.owner != unit.owner
                                    || live.zone_change_count
                                        < unit.unit.permanent.expected_zone_change_count
                            })
                        || crate::card_def::CARD_DEFS
                            .get(usize::from(unit.public_printing))
                            .is_none()
                        || previous.is_some_and(|earlier| {
                            earlier.owner != unit.owner
                                || earlier.public_printing != unit.public_printing
                                || earlier.original_face_down != unit.original_face_down
                        })
                    {
                        return Err(
                            "Greatwurm actual debit changed incarnation, face or full-width count"
                                .into(),
                        );
                    }
                    validate_debit_face(state, debit_at, unit)?;
                }
                output.push((actual, units));
            }
            _ => {}
        }
    }
    if input != paid || !debited.is_empty() {
        return Err("Greatwurm cost history is incomplete".into());
    }
    Ok(output)
}

pub(crate) fn end(
    state: &GameState,
    id: StackItemId,
    published_at: usize,
    finished_at: usize,
    creature: AbilitySourceContractV4,
) -> Result<EndV1, String> {
    let history = state
        .engine
        .event_history
        .scan_for_v1(&[EventKindV1::GreatwurmCommitReceiptV1]);
    let mut endings = history
        .iter()
        .enumerate()
        .filter_map(|(at, row)| match row {
            E::GreatwurmCommitReceiptV1 { journal } => match journal.as_ref() {
                JournalV1::Ended { id: source, result } if *source == id => Some((at, *result)),
                _ => None,
            },
            _ => None,
        });
    let (at, result) = endings
        .next()
        .ok_or("Greatwurm terminal lacks its actual native result")?;
    if published_at >= at || at.checked_add(1) != Some(finished_at) || endings.next().is_some() {
        return Err("Greatwurm terminal lost its exact original occurrence".into());
    }
    let mut dispatches = history
        .iter()
        .enumerate()
        .filter_map(|(index, row)| match row {
            E::GreatwurmCommitReceiptV1 { journal } => match journal.as_ref() {
                JournalV1::Dispatched {
                    id: source,
                    operation,
                } if *source == id => Some((index, *operation)),
                _ => None,
            },
            _ => None,
        });
    let (dispatched, operation) = dispatches
        .next()
        .ok_or("Greatwurm result lacks native dispatch")?;
    if dispatched <= published_at || dispatched >= at || dispatches.next().is_some() {
        return Err("Greatwurm native dispatch changed original lifetime".into());
    }
    let actual = match operation {
        DispatchV1::Countered if dispatched.checked_add(1) == Some(at) => EndV1::Countered,
        DispatchV1::Resolve { recipient: None } if dispatched.checked_add(1) == Some(at) => {
            EndV1::OriginalCreatureGone
        }
        DispatchV1::Resolve {
            recipient: Some((controller, before)),
        } => {
            let mut placements =
                history[dispatched + 1..at]
                    .iter()
                    .enumerate()
                    .filter_map(|(offset, event)| match event {
                        E::PlusOneCountersAdded {
                            object,
                            zone_change_count,
                            player,
                            count,
                            ..
                        } if *object == creature.source
                            && *zone_change_count == creature.zone_change_count
                            && *player == controller =>
                        {
                            Some((dispatched + 1 + offset, *count))
                        }
                        _ => None,
                    });
            let (after, placement_history_index) = match (placements.next(), placements.next()) {
                (None, None) => (before, None),
                (Some((index, amount)), None) if amount > 0 => (
                    before
                        .checked_add(amount)
                        .ok_or("Greatwurm actual placement width")?,
                    Some(u64::try_from(index).map_err(|_| "Greatwurm placement index width")?),
                ),
                _ => return Err("Greatwurm actual native placement duplicated or changed".into()),
            };
            EndV1::Resolved {
                controller,
                before,
                after,
                placement_history_index,
            }
        }
        _ => {
            return Err(
                "Greatwurm counter or missing-creature dispatch contains extra operations".into(),
            )
        }
    };
    if result != actual {
        return Err(
            "Greatwurm terminal disagrees with actual native dispatch and placement".into(),
        );
    }
    Ok(result)
}
