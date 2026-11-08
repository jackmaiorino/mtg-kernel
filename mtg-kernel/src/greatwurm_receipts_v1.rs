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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum JournalV1 {
    Debited { id: StackItemId, units: Box<[DebitV1; 6]> },
    Ended { id: StackItemId, result: EndV1 },
}

fn record(state: &mut GameState, journal: JournalV1) {
    state.engine.event_history.push(E::GreatwurmCommitReceiptV1 { journal: Box::new(journal) });
}

pub(crate) fn record_debit(state: &mut GameState, id: StackItemId, units: [DebitV1; 6]) {
    record(state, JournalV1::Debited { id, units: Box::new(units) });
}

pub(crate) fn record_end(state: &mut GameState, id: StackItemId, result: EndV1) {
    if state.greatwurm_commit_receipts_v1 {
        record(state, JournalV1::Ended { id, result });
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
    let difference = after.checked_sub(before).filter(|&n| n >= 0)
        .ok_or("Greatwurm actual +1/+1 placement decreased")?;
    let mut candidates = state.engine.event_history.iter().enumerate().skip(start)
        .filter_map(|(at, event)| match event {
            E::PlusOneCountersAdded { object, zone_change_count, player, count, .. }
                if *object == creature.source && *zone_change_count == creature.zone_change_count
                    && *player == controller =>
                Some((at, *count)),
            _ => None,
        });
    let placement_history_index = match (difference, candidates.next(), candidates.next()) {
        (0, None, None) => None,
        (actual, Some((at, count)), None) if actual == count =>
            Some(u64::try_from(at).map_err(|_| "Greatwurm actual placement history width")?),
        _ => return Err("Greatwurm realized placement differs from native counter event".into()),
    };
    Ok(EndV1::Resolved { controller, before, after, placement_history_index })
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
                let (debit_at, units) = debited.remove(&id)
                    .ok_or("Greatwurm paid spell lacks the actual six debits")?;
                if debit_at.checked_add(1) != Some(at) || !paid.insert(id) {
                    return Err("Greatwurm native debit is not paired with its own cast".into());
                }
                for (index, unit) in units.iter().enumerate() {
                    let previous = units[..index].iter().rev().find(|earlier| earlier.unit == unit.unit);
                    let original_count = previous.map_or(actual.original_counts[index], |earlier| earlier.after);
                    if unit.unit != actual.units[index] || unit.before != original_count
                        || unit.before.checked_sub(1) != Some(unit.after)
                        || unit.controller != actual.source.controller
                        || state.objects.try_get(unit.unit.permanent.object).is_none_or(|live|
                            live.owner != unit.owner
                                || live.zone_change_count < unit.unit.permanent.expected_zone_change_count)
                        || crate::card_def::CARD_DEFS.get(usize::from(unit.public_printing)).is_none()
                        || previous.is_some_and(|earlier| earlier.owner != unit.owner
                            || earlier.public_printing != unit.public_printing)
                    {
                        return Err("Greatwurm actual debit changed incarnation, face or full-width count".into());
                    }
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
    let mut endings = state.engine.event_history.scan_for_v1(&[EventKindV1::GreatwurmCommitReceiptV1])
        .iter().enumerate().filter_map(|(at, row)| match row {
            E::GreatwurmCommitReceiptV1 { journal } => match journal.as_ref() {
                JournalV1::Ended { id: source, result } if *source == id => Some((at, *result)),
                _ => None,
            },
            _ => None,
        });
    let (at, result) = endings.next().ok_or("Greatwurm terminal lacks its actual native result")?;
    if published_at >= at || at.checked_add(1) != Some(finished_at) || endings.next().is_some() {
        return Err("Greatwurm terminal lost its exact original occurrence".into());
    }
    if let EndV1::Resolved { controller, before, after, placement_history_index } = result {
        let count = after.checked_sub(before).filter(|&n| n >= 0)
            .ok_or("Greatwurm actual counter total changed")?;
        match (count, placement_history_index) {
            (0, None) => {}
            (actual, Some(index)) if actual > 0 => {
                let index = usize::try_from(index).map_err(|_| "Greatwurm placement index width")?;
                if index <= published_at || index >= at || !matches!(state.engine.event_history.get(index),
                    Some(E::PlusOneCountersAdded { object, zone_change_count, player, count, .. })
                        if *object == creature.source && *zone_change_count == creature.zone_change_count
                            && *player == controller && *count == actual)
                {
                    return Err("Greatwurm result lost its actual native placement".into());
                }
            }
            _ => return Err("Greatwurm placement presence changed".into()),
        }
    }
    Ok(result)
}
