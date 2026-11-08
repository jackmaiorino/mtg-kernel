//! Native occurrence bindings for Lathril's realized token and life outcomes.
use crate::{
    card_def::{self, PermanentFilter, Subtype, TargetSpec},
    effect::{EffectOp, PlayerRef},
    event::CommittedEvent as E,
    event_history_v1::EventKindV1,
    ids::{ObjectId, PlayerId, StackItemId},
    state::{GameState, StackItem, StackItemKind, Zone},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub(crate) const PRINTING: u16 = 468;
pub(crate) const ELVES: u8 = 10;

pub(crate) fn recipe() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::LoseLife { player: PlayerRef::Opponent, amount: 10 },
        EffectOp::GainLife { player: PlayerRef::Controller, amount: 10 },
    ])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum EndV1 {
    Resolved { effect_start: u64 },
    Countered,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum JournalV1 {
    Tokens { capture: u64, batch_start: u64 },
    Paid { item: Box<StackItem>, tap_start: u64, public_printings: Vec<u16> },
    DrainEnded { id: StackItemId, end: EndV1 },
}

fn records(state: &GameState) -> impl Iterator<Item = (usize, &JournalV1)> {
    state.engine.event_history.scan_for_v1(&[EventKindV1::LathrilReceiptV1])
        .iter().enumerate().filter_map(|(index, event)| match event {
            E::LathrilReceiptV1 { journal } => Some((index, journal.as_ref())),
            _ => None,
        })
}

pub(crate) fn selected(item: &StackItem) -> bool {
    cfg!(all(feature = "limited-fdn-fixtures", not(feature = "standard-magezero-fixtures")))
        && item.kind == StackItemKind::ActivatedAbility
        && item.v4.ability_source_contract.is_some_and(|source| {
            source.card_def == PRINTING && crate::event::is_lathril_v1(source.card_def)
        })
        && item.v4.activated_ability_index == Some(0)
        && item.v4.granted_by.is_none()
        && item.v4.cauldron_grant.is_empty()
}

pub(crate) fn payer_filter() -> PermanentFilter {
    PermanentFilter::AnotherWithSubtype(Subtype::Elf)
}

fn push(state: &mut GameState, journal: JournalV1) {
    state.engine.event_history.push(E::LathrilReceiptV1 { journal: Box::new(journal) });
}

fn offset(index: usize) -> Result<u64, String> {
    u64::try_from(index).map_err(|_| "Lathril history index exceeds u64".into())
}

fn span(state: &GameState, start: u64, end: usize) -> Result<&[E], String> {
    let start = usize::try_from(start).map_err(|_| "Lathril history index exceeds usize")?;
    state.engine.event_history.get(start..end).ok_or_else(|| "Lathril history span changed".into())
}

pub(crate) fn record_tokens(
    state: &mut GameState,
    capture: u64,
    batch_start: usize,
) -> Result<(), String> {
    push(state, JournalV1::Tokens { capture, batch_start: offset(batch_start)? });
    Ok(())
}

pub(crate) fn validate_combat_links(state: &GameState) -> Result<(), String> {
    use EventKindV1 as K;
    let history = &state.engine.event_history;
    for (index, row) in history.scan_for_v1(&[
        K::LathrilCombatCollectedV1, K::LathrilCombatPublishedV1, K::LathrilCombatResolvedV1,
        K::LathrilCombatCounteredV1, K::LathrilCombatObservedV1, K::LathrilReceiptV1,
    ]).iter().enumerate() {
        let capture = match row {
            E::LathrilCombatCollectedV1 { capture_history_index }
            | E::LathrilCombatPublishedV1 { capture_history_index, .. }
            | E::LathrilCombatResolvedV1 { capture_history_index }
            | E::LathrilCombatCounteredV1 { capture_history_index }
            | E::LathrilCombatObservedV1 { capture_history_index, .. } => *capture_history_index,
            E::LathrilReceiptV1 { journal } => {
                let JournalV1::Tokens { capture, .. } = journal.as_ref() else { continue };
                if !matches!(history.get(index + 1),
                    Some(E::LathrilCombatResolvedV1 { capture_history_index }) if capture_history_index == capture)
                {
                    return Err("Lathril token batch lacks its actual resolved terminal".into());
                }
                *capture
            }
            _ => continue,
        };
        let capture = usize::try_from(capture).map_err(|_| "Lathril capture width")?;
        if capture >= index || !matches!(history.get(capture), Some(E::LathrilCombatDamageV1 { .. })) {
            return Err("Lathril occurrence lost its original combat capture".into());
        }
    }
    Ok(())
}

pub(crate) fn token_batch(
    state: &GameState,
    capture: u64,
    resolved: usize,
    controller: PlayerId,
    token_def: u16,
) -> Result<Vec<ObjectId>, String> {
    let mut matches = records(state).filter_map(|(position, receipt)| match receipt {
        JournalV1::Tokens { capture: owner, batch_start } if *owner == capture => Some((position, *batch_start)),
        _ => None,
    });
    let (receipt, start) = matches.next().ok_or("Lathril resolved before actual token batch custody")?;
    if matches.next().is_some() || receipt.checked_add(1) != Some(resolved)
        || !state.engine.event_history.get(usize::try_from(capture).map_err(|_| "Lathril capture width")?..receipt)
            .is_some_and(|prefix| prefix.iter().enumerate().any(|(relative, event)| {
                matches!(event, E::LathrilCombatPublishedV1 { capture_history_index, .. } if *capture_history_index == capture)
                    && capture.checked_add(relative as u64).is_some_and(|published| published < start)
            }))
    {
        return Err("Lathril actual token batch lost original publication".into());
    }
    let mut created = Vec::new();
    let mut seen = BTreeSet::new();
    for event in span(state, start, receipt)? {
        if let E::CreateToken { object, token_def: actual, controller: recipient, .. } = event {
            if *actual != token_def || *recipient != controller || !seen.insert(*object)
                || state.objects.try_get(*object).is_none_or(|live| live.owner != controller || !live.v4.is_token)
            {
                return Err("Lathril actual Elf Warrior token batch changed".into());
            }
            created.push(*object);
        }
    }
    Ok(created)
}

fn verify_paid(state: &GameState, item: &StackItem, start: u64, end: usize) -> Result<(), String> {
    let source = item.v4.ability_source_contract.ok_or("Lathril paid source absent")?;
    if !selected(item)
        || item.source != source.source
        || item.controller != source.controller
        || item.v4.stack_item_id == StackItemId::default()
        || item.v4.target_spec != Some(TargetSpec::None)
        || item.inline_effect != Some(recipe())
        || !item.targets.is_empty()
        || !item.v4.target_contracts.is_empty()
        || !item.discarded.is_empty()
        || item.v4.paid_cost_refs.len() != usize::from(ELVES)
    {
        return Err("Lathril exact paid activation changed".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, source)?;
    for (index, paid) in item.v4.paid_cost_refs.iter().enumerate() {
        if paid.object == source.source || paid.zone != Zone::Battlefield
            || paid.controller != source.controller || paid.visible_to_mask != 3
            || paid.power_lki.is_some()
            || card_def::CARD_DEFS.get(usize::from(paid.card_def)).is_none()
            || state.objects.try_get(paid.object).is_none_or(|live|
                live.owner != paid.owner || live.zone_change_count < paid.zone_change_count
                    || (live.zone_change_count == paid.zone_change_count && live.zone != Zone::Battlefield))
            || item.v4.paid_cost_refs[..index].iter().any(|earlier| earlier.object == paid.object)
        {
            return Err("Lathril original Elf payment incarnation changed".into());
        }
    }
    let taps = span(state, start, end)?.iter().filter_map(|event| match event {
        E::Tap { object } => Some(*object),
        _ => None,
    });
    if !taps.eq(std::iter::once(source.source).chain(item.v4.paid_cost_refs.iter().map(|paid| paid.object))) {
        return Err("Lathril paid Elf roster lost actual native taps".into());
    }
    Ok(())
}

pub(crate) fn record_paid(state: &mut GameState, tap_start: usize) -> Result<(), String> {
    let Some(item) = state.stack.last().filter(|item| selected(item)) else { return Ok(()) };
    let tap_start = offset(tap_start)?;
    verify_paid(state, item, tap_start, state.engine.event_history.len())?;
    let public_printings = item.v4.paid_cost_refs.iter().map(|paid| {
        crate::rl::projected_card_def(state, paid.object, paid.zone_change_count)
    }).collect();
    push(state, JournalV1::Paid { item: Box::new(item.clone()), tap_start, public_printings });
    Ok(())
}

fn paid(state: &GameState, id: StackItemId) -> Result<Option<(usize, &StackItem, u64, &[u16])>, String> {
    let mut matched = records(state).filter_map(|(position, journal)| match journal {
        JournalV1::Paid { item, tap_start, public_printings } if item.v4.stack_item_id == id =>
            Some((position, item.as_ref(), *tap_start, public_printings.as_slice())),
        _ => None,
    });
    let record = matched.next();
    if matched.next().is_some() {
        return Err("Lathril paid activation duplicated".into());
    }
    Ok(record)
}

pub(crate) fn validate_live(state: &GameState, item: &StackItem) -> Result<bool, String> {
    // Legacy raw worlds may contain a valid activation predating the receipt.
    // The typed consumer requires the new receipt at a live or observed row.
    let Some((at, original, start, public_printings)) = paid(state, item.v4.stack_item_id)? else { return Ok(false) };
    if original != item || ending(state, item.v4.stack_item_id)?.is_some() {
        return Err("Lathril live activation changed or already consumed".into());
    }
    verify_paid(state, original, start, at)?;
    validate_public_printings(original, public_printings)?;
    Ok(true)
}

fn validate_public_printings(item: &StackItem, public_printings: &[u16]) -> Result<(), String> {
    if public_printings.len() != item.v4.paid_cost_refs.len()
        || item.v4.paid_cost_refs.iter().zip(public_printings).any(|(paid, printing)| {
            paid.card_def != *printing && !card_def::CARD_DEFS.get(usize::from(*printing))
                .is_some_and(|def| matches!(def.name, "Face-down creature" | "Face-down card"))
        })
    {
        return Err("Lathril payment event-time visible printing changed".into());
    }
    Ok(())
}

pub(crate) fn payment_printings<'a>(state: &'a GameState, item: &StackItem) -> Result<Option<&'a [u16]>, String> {
    let Some((_, original, _, printings)) = paid(state, item.v4.stack_item_id)? else { return Ok(None) };
    if original != item {
        return Err("Lathril payment projection lost native row".into());
    }
    validate_public_printings(original, printings)?;
    Ok(Some(printings))
}

pub(crate) fn record_ended(
    state: &mut GameState,
    item: &StackItem,
    effect_start: Option<usize>,
) -> Result<(), String> {
    if !selected(item) || !validate_live(state, item)? {
        return Ok(());
    }
    if state.stack.iter().any(|live| live.v4.stack_item_id == item.v4.stack_item_id)
        || state.engine.halted.is_some()
    {
        return Err("Lathril outcome requires native completed removal".into());
    }
    let end = match effect_start {
        Some(start) => EndV1::Resolved { effect_start: offset(start)? },
        None => EndV1::Countered,
    };
    push(state, JournalV1::DrainEnded { id: item.v4.stack_item_id, end });
    Ok(())
}

fn ending(state: &GameState, id: StackItemId) -> Result<Option<(usize, EndV1)>, String> {
    let mut matched = records(state).filter_map(|(position, receipt)| match receipt {
        JournalV1::DrainEnded { id: owner, end } if *owner == id => Some((position, *end)),
        _ => None,
    });
    let end = matched.next();
    if matched.next().is_some() {
        return Err("Lathril drain already ended".into());
    }
    Ok(end)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DrainOutcomeV1 {
    Live(usize),
    RemovedWithoutResolution,
    Countered,
    Resolved { opponent_lost: u64, controller_gained: u64 },
}

pub(crate) fn drains(state: &GameState) -> Result<Vec<(&StackItem, DrainOutcomeV1)>, String> {
    for (at, receipt) in records(state) {
        if let JournalV1::DrainEnded { id, .. } = receipt {
            if paid(state, *id)?.is_none_or(|(payment, ..)| payment >= at) {
                return Err("Lathril drain outcome lost earlier payment".into());
            }
        }
    }
    let mut rows = vec![];
    for (at, receipt) in records(state) {
        let JournalV1::Paid { item, tap_start, public_printings } = receipt else { continue };
        let id = item.v4.stack_item_id;
        let (original_at, ..) = paid(state, id)?.ok_or("Lathril paid activation absent")?;
        if original_at != at {
            return Err("Lathril original payment order changed".into());
        }
        verify_paid(state, item, *tap_start, at)?;
        validate_public_printings(item, public_printings)?;
        let mut live = state.stack.iter().enumerate().filter(|(_, live)| live.v4.stack_item_id == id);
        let first = live.next();
        let end = ending(state, id)?;
        if live.next().is_some() || first.is_some_and(|(_, row)| row != item.as_ref() || end.is_some()) {
            return Err("Lathril current paid activation changed".into());
        }
        let outcome = match (first, end) {
            (Some((position, _)), None) => DrainOutcomeV1::Live(position),
            (None, None) => DrainOutcomeV1::RemovedWithoutResolution,
            (None, Some((_, EndV1::Countered))) => DrainOutcomeV1::Countered,
            (None, Some((end, EndV1::Resolved { effect_start }))) => {
                if effect_start <= offset(at)? {
                    return Err("Lathril life outcome predates payment".into());
                }
                let mut lost = 0u64;
                let mut gained = 0u64;
                let controller = item.controller;
                for event in span(state, effect_start, end)? {
                    let (total, amount) = match event {
                        E::LifeLoss { player, amount } if *player == controller.opponent() =>
                            (&mut lost, u64::try_from(*amount).map_err(|_| "negative Lathril life loss")?),
                        E::LifeGain { player, amount } if *player == controller =>
                            (&mut gained, u64::try_from(*amount).map_err(|_| "negative Lathril life gain")?),
                        E::LifeLossWideV59 { player, amount } if *player == controller.opponent() =>
                            (&mut lost, u64::from(*amount)),
                        E::LifeGainWideV59 { player, amount } if *player == controller =>
                            (&mut gained, u64::from(*amount)),
                        _ => continue,
                    };
                    *total = total.checked_add(amount).ok_or("Lathril actual life outcome overflow")?;
                }
                DrainOutcomeV1::Resolved { opponent_lost: lost, controller_gained: gained }
            }
            (Some(_), Some(_)) => return Err("Lathril ended activation still live".into()),
        };
        rows.push((item.as_ref(), outcome));
    }
    Ok(rows)
}
