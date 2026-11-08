//! Native occurrence bindings for Lathril's realized token and life outcomes.
use crate::{
    card_def::{self, PermanentFilter, Subtype, TargetSpec},
    effect::{EffectOp, ExecCtx, PlayerRef},
    event::{CommittedEvent as E, ProposedEvent},
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
    pub(crate) reference: PaidCostRefV4,
    pub(crate) face_down_disguised: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentInputV1 {
    source: AbilitySourceContractV4,
    pub(crate) roster: Vec<PayerInputV1>,
    tap_start: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifeOutcomeV1 {
    pub(crate) opponent_lost: u64,
    pub(crate) controller_gained: u64,
}

/// Temporary ownership follows the specific native proposal, including any
/// replacement copies, without tagging other operations in the same batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum OperationOwnerV1 {
    Tokens { capture: u64 },
    Drain { id: StackItemId },
    Payment { source: ObjectId, start: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ActualOperationV1 {
    Token { object: ObjectId, definition: u16, controller: PlayerId },
    Life { player: PlayerId, gain: bool, amount: u64 },
    Tap(PayerInputV1),
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
    Operation { owner: OperationOwnerV1, event_at: usize, actual: ActualOperationV1 },
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

pub(crate) fn owned_proposal(mut event: ProposedEvent, owner: Option<OperationOwnerV1>) -> ProposedEvent {
    if let Some(owner) = owner {
        match &mut event {
            ProposedEvent::CreateToken(event) => event.lathril_owner = Some(owner),
            ProposedEvent::LifeLoss(event) => event.lathril_owner = Some(owner),
            ProposedEvent::LifeGain(event) => event.lathril_owner = Some(owner),
            ProposedEvent::Tap(event) => event.lathril_owner = Some(owner),
            _ => unreachable!("Lathril binds only its native token, life and tap operations"),
        }
    }
    event
}

pub(crate) fn payment_owner(state: &GameState, source: ObjectId, cost: &[card_def::CostComponent]) -> Option<OperationOwnerV1> {
    use card_def::CostComponent;
    (state.objects.try_get(source).is_some_and(|live|
        live.zone == Zone::Battlefield && live.card_def == PRINTING && crate::event::is_lathril_v1(live.card_def))
        && cost == [CostComponent::Tap, CostComponent::TapControlled { count: ELVES, filter: payer_filter() }])
        .then_some(OperationOwnerV1::Payment { source, start: state.engine.event_history.len() })
}

pub(crate) fn life_owner(state: &GameState, ctx: &ExecCtx) -> Option<OperationOwnerV1> {
    if ctx.ability_source_contract.is_none_or(|source| source.card_def != PRINTING) {
        return None;
    }
    let id = ctx.stack_item_id?;
    let (_, item, _) = paid(state, id).ok().flatten()?;
    (item.source == ctx.source
        && item.controller == ctx.controller
        && item.v4.ability_source_contract == ctx.ability_source_contract
        && drain_begin(state, id).ok().flatten().is_some()
        && ending(state, id).ok().is_some_and(|end| end.is_none()))
        .then_some(OperationOwnerV1::Drain { id })
}

pub(crate) struct CommitOperationV1 {
    owner: OperationOwnerV1,
    payer: Option<PayerInputV1>,
}

fn payer_at_event(state: &GameState, object: ObjectId) -> PayerInputV1 {
    PayerInputV1 {
        reference: PaidCostRefV4::capture(state, object),
        face_down_disguised: state.objects.get(object).v4.face_down_v1.map(|face| face.disguised),
    }
}

pub(crate) fn capture_operation(state: &GameState, event: &ProposedEvent) -> Option<CommitOperationV1> {
    let (owner, payer) = match event {
        ProposedEvent::CreateToken(event) => (event.lathril_owner?, None),
        ProposedEvent::LifeLoss(event) => (event.lathril_owner?, None),
        ProposedEvent::LifeGain(event) => (event.lathril_owner?, None),
        ProposedEvent::Tap(event) => (event.lathril_owner?, Some(payer_at_event(state, event.object))),
        _ => return None,
    };
    Some(CommitOperationV1 { owner, payer })
}

impl CommitOperationV1 {
    pub(crate) fn record(self, state: &mut GameState, event_at: usize) -> Result<(), String> {
        let actual = match (self.owner, state.engine.event_history.get(event_at), self.payer) {
            (OperationOwnerV1::Tokens { .. }, Some(E::CreateToken { object, token_def, controller, .. }), None) =>
                ActualOperationV1::Token { object: *object, definition: *token_def, controller: *controller },
            (OperationOwnerV1::Drain { .. }, Some(E::LifeLoss { player, amount }), None) =>
                ActualOperationV1::Life { player: *player, gain: false, amount: u64::try_from(*amount).map_err(|_| "negative Lathril loss")? },
            (OperationOwnerV1::Drain { .. }, Some(E::LifeGain { player, amount }), None) =>
                ActualOperationV1::Life { player: *player, gain: true, amount: u64::try_from(*amount).map_err(|_| "negative Lathril gain")? },
            (OperationOwnerV1::Payment { .. }, Some(E::Tap { object }), Some(payer))
                if *object == payer.reference.object => ActualOperationV1::Tap(payer),
            _ => return Err("Lathril native operation changed its actual event type".into()),
        };
        push(state, JournalV1::Operation { owner: self.owner, event_at, actual });
        Ok(())
    }
}

fn operations(
    state: &GameState,
    owner: OperationOwnerV1,
    start: usize,
    end: usize,
) -> Result<Vec<ActualOperationV1>, String> {
    let mut actuals = vec![];
    for (at, row) in records(state) {
        let JournalV1::Operation { owner: bound, event_at, actual } = row else { continue };
        if *bound != owner {
            continue;
        }
        let original = state.engine.event_history.get(*event_at);
        let matches = match (actual, original) {
            (ActualOperationV1::Token { object, definition, controller },
                Some(E::CreateToken { object: actual, token_def, controller: recipient, .. })) =>
                object == actual && definition == token_def && controller == recipient,
            (ActualOperationV1::Life { player, gain: false, amount }, Some(E::LifeLoss { player: actual, amount: value }))
            | (ActualOperationV1::Life { player, gain: true, amount }, Some(E::LifeGain { player: actual, amount: value })) =>
                player == actual && u64::try_from(*value).ok() == Some(*amount),
            (ActualOperationV1::Tap(payer), Some(E::Tap { object })) => payer.reference.object == *object,
            _ => false,
        };
        if !matches || event_at.checked_add(1) != Some(at) || *event_at < start || at >= end {
            return Err("Lathril native operation lost original committed event or span".into());
        }
        actuals.push(*actual);
    }
    Ok(actuals)
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
    let created = operations(state, OperationOwnerV1::Tokens { capture }, begin + 1, state.engine.event_history.len())?
        .into_iter().map(|actual| match actual {
            ActualOperationV1::Token { object, .. } => Ok(object),
            _ => Err("Lathril token batch contains foreign native operation"),
        }).collect::<Result<_, _>>()?;
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
                    JournalV1::Operation { owner: OperationOwnerV1::Tokens { capture }, .. } => {
                        if token_begin(state, *capture)? >= index {
                            return Err("Lathril token operation predates its native batch".into());
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
    for actual in operations(state, OperationOwnerV1::Tokens { capture }, begin + 1, receipt)? {
        let ActualOperationV1::Token { object, definition, controller: recipient } = actual else {
            return Err("Lathril token batch contains foreign native operation".into());
        };
        if definition != token_def || recipient != controller || !seen.insert(object)
            || state.objects.try_get(object).is_none_or(|live| live.owner != controller || !live.v4.is_token)
        {
            return Err("Lathril actual Elf Warrior token batch changed".into());
        }
        created.push(object);
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
        roster: pending.object_cost_chosen.iter().map(|binding| payer_at_event(state, binding.object)).collect(),
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
    let taps = operations(state, OperationOwnerV1::Payment {
        source: source.source, start: input.tap_start,
    }, input.tap_start, end)?;
    let Some((ActualOperationV1::Tap(source_tap), other_taps)) = taps.split_first() else {
        return Err("Lathril paid roster lost native source tap".into());
    };
    let expected_source = PaidCostRefV4 {
        object: source.source, card_def: source.card_def, owner: source.owner,
        controller: source.controller, zone: source.zone, zone_change_count: source.zone_change_count,
        visible_to_mask: 3, power_lki: None,
    };
    if source_tap.reference != expected_source || source_tap.face_down_disguised.is_some()
        || !input.roster.iter().copied().map(ActualOperationV1::Tap).eq(other_taps.iter().copied())
    {
        return Err("Lathril paid Elf roster changed from actual native tap incarnations or faces".into());
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
            state, item.v4.stack_item_id, item.controller, begin + 1, state.engine.event_history.len(),
        )?),
        _ => return Err("Lathril drain terminal disagrees with original native start".into()),
    };
    push(state, JournalV1::DrainEnded { id: item.v4.stack_item_id, end, actual });
    Ok(())
}

fn life_outcome(state: &GameState, id: StackItemId, controller: PlayerId, start: usize, end: usize) -> Result<LifeOutcomeV1, String> {
    let mut outcome = LifeOutcomeV1 { opponent_lost: 0, controller_gained: 0 };
    for actual in operations(state, OperationOwnerV1::Drain { id }, start, end)? {
        let ActualOperationV1::Life { player, gain, amount } = actual else {
            return Err("Lathril drain contains foreign native operation".into());
        };
        let total = match (player, gain) {
            (player, false) if player == controller.opponent() => &mut outcome.opponent_lost,
            (player, true) if player == controller => &mut outcome.controller_gained,
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
            JournalV1::Operation { owner: OperationOwnerV1::Drain { id }, .. } => {
                if drain_begin(state, *id)?.is_none_or(|begin| begin >= at) {
                    return Err("Lathril life operation lacks its original drain".into());
                }
            }
            JournalV1::Operation { owner: OperationOwnerV1::Payment { source, start }, .. } => {
                if records(state).filter(|(later, row)| *later > at && matches!(row,
                    JournalV1::PaymentInput { input, .. }
                        if input.source.source == *source && input.tap_start == *start
                )).count() != 1 {
                    return Err("Lathril actual tap lacks its unique original payment".into());
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
                if life_outcome(state, id, item.controller, begin + 1, end)? != actual {
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
