//! Native occurrence bindings for Lathril's realized token and life outcomes.
use crate::{
    card_def::{self, PermanentFilter, Subtype, TargetSpec},
    effect::{EffectOp, PlayerRef},
    event::CommittedEvent as E,
    event_history_v1::EventKindV1,
    ids::{ObjectId, PlayerId, StackItemId},
    state::{AbilitySourceContractV4, GameState, PaidCostRefV4, StackItem, StackItemKind, Zone},
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
    Resolved,
    Countered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PayerInputV1 {
    reference: PaidCostRefV4,
    face_down_disguised: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentInputV1 {
    source: AbilitySourceContractV4,
    roster: Vec<PayerInputV1>,
    tap_start: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifeOutcomeV1 {
    opponent_lost: u64,
    controller_gained: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum JournalV1 {
    TokenBegin { capture: u64 },
    Tokens { capture: u64, created: Vec<ObjectId> },
    PaymentInput { id: StackItemId, input: Box<PaymentInputV1> },
    Paid { item: Box<StackItem>, public_printings: Vec<u16> },
    DrainBegin { id: StackItemId },
    DrainEnded { id: StackItemId, end: EndV1, actual: Option<LifeOutcomeV1> },
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

pub(crate) fn pending_selected(state: &GameState, pending: &crate::engine::PendingActivation) -> bool {
    cfg!(all(feature = "limited-fdn-fixtures", not(feature = "standard-magezero-fixtures")))
        && pending.ability_index == 0
        && pending.cauldron_grant.is_empty()
        && state.objects.try_get(pending.source).is_some_and(|object|
            object.card_def == PRINTING && crate::event::is_lathril_v1(object.card_def))
}

fn push(state: &mut GameState, journal: JournalV1) {
    state.engine.event_history.push(E::LathrilReceiptV1 { journal: Box::new(journal) });
}

fn span(state: &GameState, start: usize, end: usize) -> Result<&[E], String> {
    state.engine.event_history.get(start..end).ok_or_else(|| "Lathril history span changed".into())
}

pub(crate) fn record_token_begin(state: &mut GameState, capture: u64) {
    push(state, JournalV1::TokenBegin { capture });
}

fn token_begin(state: &GameState, capture: u64) -> Result<usize, String> {
    let mut begins = records(state).filter_map(|(position, receipt)|
        matches!(receipt, JournalV1::TokenBegin { capture: owner } if *owner == capture).then_some(position));
    let begin = begins.next().ok_or("Lathril token batch lost its native start")?;
    if begins.next().is_some() {
        return Err("Lathril token batch start duplicated".into());
    }
    Ok(begin)
}

pub(crate) fn record_tokens(state: &mut GameState, capture: u64) -> Result<(), String> {
    let begin = token_begin(state, capture)?;
    let created = span(state, begin + 1, state.engine.event_history.len())?.iter().filter_map(|event| match event {
        E::CreateToken { object, .. } => Some(*object),
        _ => None,
    }).collect();
    push(state, JournalV1::Tokens { capture, created });
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
                match journal.as_ref() {
                    JournalV1::TokenBegin { capture } => {
                        if !records(state).any(|(end, entry)| end > index
                            && matches!(entry, JournalV1::Tokens { capture: original, .. } if original == capture))
                        {
                            return Err("Lathril token batch lost its terminal".into());
                        }
                        *capture
                    }
                    JournalV1::Tokens { capture, .. } => {
                        if !matches!(history.get(index + 1),
                            Some(E::LathrilCombatResolvedV1 { capture_history_index }) if capture_history_index == capture)
                        {
                            return Err("Lathril token batch lacks its actual resolved terminal".into());
                        }
                        *capture
                    }
                    _ => continue,
                }
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
    let mut receipts = records(state).filter_map(|(position, receipt)| match receipt {
        JournalV1::Tokens { capture: owner, created } if *owner == capture => Some((position, created)),
        _ => None,
    });
    let (receipt, recorded) = receipts.next().ok_or("Lathril resolved before actual token batch custody")?;
    let begin = token_begin(state, capture)?;
    let capture_at = usize::try_from(capture).map_err(|_| "Lathril capture width")?;
    if receipts.next().is_some() || receipt.checked_add(1) != Some(resolved) || begin >= receipt
        || !state.engine.event_history.get(capture_at..begin).is_some_and(|prefix| {
            prefix.iter().any(|event|
                matches!(event, E::LathrilCombatPublishedV1 { capture_history_index, .. } if *capture_history_index == capture))
        })
    {
        return Err("Lathril actual token batch lost original publication".into());
    }
    let mut created = Vec::new();
    let mut seen = BTreeSet::new();
    for event in span(state, begin + 1, receipt)? {
        if let E::CreateToken { object, token_def: actual, controller: recipient, .. } = event {
            if *actual != token_def || *recipient != controller || !seen.insert(*object)
                || state.objects.try_get(*object).is_none_or(|live| live.owner != controller || !live.v4.is_token)
            {
                return Err("Lathril actual Elf Warrior token batch changed".into());
            }
            created.push(*object);
        }
    }
    if created != *recorded {
        return Err("Lathril committed token roster differs from its native terminal".into());
    }
    Ok(created)
}

/// Capture the original cost operands before any tap can change the live scene.
/// Successful payment later publishes this independently of the stack row.
pub(crate) fn freeze_payment(
    state: &GameState,
    pending: &crate::engine::PendingActivation,
    source: AbilitySourceContractV4,
) -> Option<PaymentInputV1> {
    pending_selected(state, pending).then(|| PaymentInputV1 {
        source,
        tap_start: state.engine.event_history.len(),
        roster: pending.object_cost_chosen.iter().map(|binding| PayerInputV1 {
            reference: PaidCostRefV4::capture(state, binding.object),
            face_down_disguised: state.objects.get(binding.object).v4.face_down_v1
                .map(|face| face.disguised),
        }).collect(),
    })
}

fn verify_paid(state: &GameState, item: &StackItem, input: &PaymentInputV1, end: usize) -> Result<(), String> {
    let source = item.v4.ability_source_contract.ok_or("Lathril paid source absent")?;
    if !selected(item)
        || source != input.source
        || item.source != source.source
        || item.controller != source.controller
        || item.v4.stack_item_id == StackItemId::default()
        || item.v4.target_spec != Some(TargetSpec::None)
        || item.inline_effect != Some(recipe())
        || !item.targets.is_empty()
        || !item.v4.target_contracts.is_empty()
        || !item.discarded.is_empty()
        || item.v4.paid_cost_refs.len() != usize::from(ELVES)
        || !input.roster.iter().map(|payer| payer.reference).eq(item.v4.paid_cost_refs.iter().copied())
    {
        return Err("Lathril exact paid activation changed from original prepayment".into());
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
    let taps = span(state, input.tap_start, end)?.iter().filter_map(|event| match event {
        E::Tap { object } => Some(*object),
        _ => None,
    });
    if !taps.eq(std::iter::once(source.source).chain(item.v4.paid_cost_refs.iter().map(|paid| paid.object))) {
        return Err("Lathril paid Elf roster lost actual native taps".into());
    }
    Ok(())
}

pub(crate) fn record_paid(state: &mut GameState, input: Option<PaymentInputV1>) -> Result<(), String> {
    let item = state.stack.last().filter(|item| selected(item));
    let (item, input) = match (item, input) {
        (Some(item), Some(input)) => (item, input),
        (None, None) => return Ok(()),
        _ => return Err("Lathril paid publication lost original prepayment".into()),
    };
    verify_paid(state, item, &input, state.engine.event_history.len())?;
    let public_printings = item.v4.paid_cost_refs.iter().map(|paid| {
        crate::rl::projected_card_def(state, paid.object, paid.zone_change_count)
    }).collect::<Vec<_>>();
    validate_public_printings(&input, &public_printings)?;
    let item = Box::new(item.clone());
    push(state, JournalV1::PaymentInput { id: item.v4.stack_item_id, input: Box::new(input) });
    push(state, JournalV1::Paid { item, public_printings });
    Ok(())
}

fn paid(state: &GameState, id: StackItemId) -> Result<Option<(usize, &StackItem, &[u16])>, String> {
    let mut matched = records(state).filter_map(|(position, journal)| match journal {
        JournalV1::Paid { item, public_printings } if item.v4.stack_item_id == id =>
            Some((position, item.as_ref(), public_printings.as_slice())),
        _ => None,
    });
    let record = matched.next();
    if matched.next().is_some() {
        return Err("Lathril paid activation duplicated".into());
    }
    Ok(record)
}

fn payment_input<'a>(state: &'a GameState, at: usize, item: &StackItem) -> Result<&'a PaymentInputV1, String> {
    let input_at = at.checked_sub(1).ok_or("Lathril payment has no preceding input")?;
    let Some(E::LathrilReceiptV1 { journal }) = state.engine.event_history.get(input_at) else {
        return Err("Lathril paid activation missing native prepayment".into());
    };
    let JournalV1::PaymentInput { id, input } = journal.as_ref() else {
        return Err("Lathril paid activation has foreign native prepayment".into());
    };
    if *id != item.v4.stack_item_id {
        return Err("Lathril native prepayment names a different activation".into());
    }
    verify_paid(state, item, input, input_at)?;
    Ok(input)
}

pub(crate) fn validate_live(state: &GameState, item: &StackItem) -> Result<bool, String> {
    // Legacy raw worlds may contain a valid activation predating the receipt.
    // The typed consumer requires the new receipt at a live or observed row.
    let Some((at, original, public_printings)) = paid(state, item.v4.stack_item_id)? else { return Ok(false) };
    if original != item || ending(state, item.v4.stack_item_id)?.is_some() {
        return Err("Lathril live activation changed or already consumed".into());
    }
    let input = payment_input(state, at, item)?;
    validate_public_printings(input, public_printings)?;
    Ok(true)
}

fn validate_public_printings(input: &PaymentInputV1, public_printings: &[u16]) -> Result<(), String> {
    if public_printings.len() != input.roster.len()
        || input.roster.iter().zip(public_printings).any(|(payer, printing)| {
            let expected = match payer.face_down_disguised {
                Some(true) => card_def::card_id_by_name("Face-down creature"),
                Some(false) => card_def::card_id_by_name("Face-down card"),
                None => Some(payer.reference.card_def),
            };
            expected != Some(*printing)
        })
    {
        return Err("Lathril payment event-time visible printing changed".into());
    }
    Ok(())
}

pub(crate) fn payment_printings<'a>(state: &'a GameState, item: &StackItem) -> Result<Option<&'a [u16]>, String> {
    let Some((at, original, printings)) = paid(state, item.v4.stack_item_id)? else { return Ok(None) };
    if original != item {
        return Err("Lathril payment projection lost native row".into());
    }
    let input = payment_input(state, at, item)?;
    validate_public_printings(input, printings)?;
    Ok(Some(printings))
}

fn drain_begin(state: &GameState, id: StackItemId) -> Result<Option<usize>, String> {
    let mut begins = records(state).filter_map(|(position, receipt)|
        matches!(receipt, JournalV1::DrainBegin { id: owner } if *owner == id).then_some(position));
    let begin = begins.next();
    if begins.next().is_some() {
        return Err("Lathril drain start duplicated".into());
    }
    Ok(begin)
}

pub(crate) fn record_drain_begin(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    if !selected(item) || !validate_live(state, item)? {
        return Ok(());
    }
    let id = item.v4.stack_item_id;
    if drain_begin(state, id)?.is_some() || state.stack.iter().any(|row| row.v4.stack_item_id == id) {
        return Err("Lathril drain start does not own a unique removed row".into());
    }
    push(state, JournalV1::DrainBegin { id });
    Ok(())
}

pub(crate) fn record_ended(
    state: &mut GameState,
    item: &StackItem,
    end: EndV1,
) -> Result<(), String> {
    if !selected(item) || !validate_live(state, item)? {
        return Ok(());
    }
    if state.stack.iter().any(|live| live.v4.stack_item_id == item.v4.stack_item_id)
        || state.engine.halted.is_some()
    {
        return Err("Lathril outcome requires native completed removal".into());
    }
    let actual = match (end, drain_begin(state, item.v4.stack_item_id)?) {
        (EndV1::Countered, None) => None,
        (EndV1::Resolved, Some(begin)) => Some(life_outcome(
            state, item.controller, begin + 1, state.engine.event_history.len(),
        )?),
        _ => return Err("Lathril drain terminal disagrees with original native start".into()),
    };
    push(state, JournalV1::DrainEnded { id: item.v4.stack_item_id, end, actual });
    Ok(())
}

fn life_outcome(state: &GameState, controller: PlayerId, start: usize, end: usize) -> Result<LifeOutcomeV1, String> {
    let mut outcome = LifeOutcomeV1 { opponent_lost: 0, controller_gained: 0 };
    for event in span(state, start, end)? {
        let (total, amount) = match event {
            E::LifeLoss { player, amount } if *player == controller.opponent() =>
                (&mut outcome.opponent_lost, u64::try_from(*amount).map_err(|_| "negative Lathril life loss")?),
            E::LifeGain { player, amount } if *player == controller =>
                (&mut outcome.controller_gained, u64::try_from(*amount).map_err(|_| "negative Lathril life gain")?),
            E::LifeLossWideV59 { player, amount } if *player == controller.opponent() =>
                (&mut outcome.opponent_lost, u64::from(*amount)),
            E::LifeGainWideV59 { player, amount } if *player == controller =>
                (&mut outcome.controller_gained, u64::from(*amount)),
            _ => continue,
        };
        *total = total.checked_add(amount).ok_or("Lathril actual life outcome overflow")?;
    }
    Ok(outcome)
}

fn ending(state: &GameState, id: StackItemId) -> Result<Option<(usize, EndV1, Option<LifeOutcomeV1>)>, String> {
    let mut matched = records(state).filter_map(|(position, receipt)| match receipt {
        JournalV1::DrainEnded { id: owner, end, actual } if *owner == id =>
            Some((position, *end, *actual)),
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
        match receipt {
            JournalV1::DrainBegin { id } | JournalV1::DrainEnded { id, .. } => {
                if paid(state, *id)?.is_none_or(|(payment, ..)| payment >= at) {
                    return Err("Lathril drain outcome lost earlier payment".into());
                }
            }
            JournalV1::PaymentInput { id, .. } => {
                if paid(state, *id)?.is_none_or(|(payment, ..)| at.checked_add(1) != Some(payment)) {
                    return Err("Lathril native prepayment lost its subsequent paid row".into());
                }
            }
            _ => {}
        }
    }
    let mut rows = vec![];
    for (at, receipt) in records(state) {
        let JournalV1::Paid { item, public_printings } = receipt else { continue };
        let id = item.v4.stack_item_id;
        let (original_at, ..) = paid(state, id)?.ok_or("Lathril paid activation absent")?;
        if original_at != at {
            return Err("Lathril original payment order changed".into());
        }
        let input = payment_input(state, at, item)?;
        validate_public_printings(input, public_printings)?;
        let mut live = state.stack.iter().enumerate().filter(|(_, live)| live.v4.stack_item_id == id);
        let first = live.next();
        let end = ending(state, id)?;
        let begin = drain_begin(state, id)?;
        if live.next().is_some() || first.is_some_and(|(_, row)| row != item.as_ref() || end.is_some()) {
            return Err("Lathril current paid activation changed".into());
        }
        let outcome = match (first, begin, end) {
            (Some((position, _)), None, None) => DrainOutcomeV1::Live(position),
            (None, None, None) => DrainOutcomeV1::RemovedWithoutResolution,
            (None, None, Some((_, EndV1::Countered, None))) => DrainOutcomeV1::Countered,
            (None, Some(begin), Some((end, EndV1::Resolved, Some(actual)))) => {
                if begin <= at || begin >= end {
                    return Err("Lathril life outcome reordered its native start or payment".into());
                }
                if life_outcome(state, item.controller, begin + 1, end)? != actual {
                    return Err("Lathril life span differs from its native terminal".into());
                }
                DrainOutcomeV1::Resolved {
                    opponent_lost: actual.opponent_lost,
                    controller_gained: actual.controller_gained,
                }
            }
            _ => return Err("Lathril drain lifecycle lost its exact start or terminal".into()),
        };
        rows.push((item.as_ref(), outcome));
    }
    Ok(rows)
}
