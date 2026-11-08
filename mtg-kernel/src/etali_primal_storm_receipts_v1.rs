//! Native Etali attack, original top-exile batch, and actual resolution casts.
use crate::{
    cast_identity_v1::PublicIdentityV1,
    effect::{EffectObjectBinding, EffectOp},
    etali_primal_storm_public_v1 as printing,
    event::{CommittedEvent as E, ProposedEvent},
    event_history_v1::EventKindV1,
    ids::{ObjectId, StackItemId},
    state::{AbilitySourceContractV4, GameState, SpellCastRouteV4, StackItem, StackItemKind,
        StackSourceContractV4, Zone},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExileV1 {
    pub original: AbilitySourceContractV4,
    pub arrival: AbilitySourceContractV4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV1 {
    pub cast_at: u64,
    pub original: EffectObjectBinding,
    pub child: StackItemId,
    pub source: StackSourceContractV4,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum JournalV1 {
    Attacked { source: AbilitySourceContractV4 },
    Published { attack_at: u64, item: Box<StackItem> },
    Started { id: StackItemId, expected_moves: u8 },
    BatchCommitted { id: StackItemId, native_start: u64 },
    TopExileVisibility { id: StackItemId, arrival: AbilitySourceContractV4 },
    TopExiled { id: StackItemId, row: ExileV1 },
    Cast { id: StackItemId, row: CastV1 },
    Finished { id: StackItemId, declined: bool },
    Countered { id: StackItemId },
}

fn push(state: &mut GameState, journal: JournalV1) {
    state.engine.event_history.push(E::EtaliPrimalStormReceiptV1 { journal: Box::new(journal) });
}

pub(crate) fn selected(item: &StackItem) -> bool {
    item.inline_effect == Some(EffectOp::ExileTopEachPlayerThenCastV1)
}

pub(crate) fn record_attack(state: &mut GameState, object: ObjectId) {
    if !state.etali_commit_receipts_v1 {
        return;
    }
    let live = state.objects.get(object);
    if !printing::definition(live.card_def) || live.v4.face_down_v1.is_some()
        || !crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    {
        return;
    }
    let source = AbilitySourceContractV4::capture(state, object);
    push(state, JournalV1::Attacked { source });
}

pub(crate) fn observed_attack(state: &GameState, source: AbilitySourceContractV4) -> bool {
    state.engine.event_history.iter().enumerate().any(|(at, event)| {
        matches!(event, E::EtaliPrimalStormReceiptV1 { journal }
            if matches!(journal.as_ref(), JournalV1::Attacked { source: observed } if *observed == source))
            && at.checked_sub(1).and_then(|at| state.engine.event_history.get(at)).is_some_and(|event|
                matches!(event, E::DeclaredAttacker { source: actual, source_zone_change_count, controller }
                    if *actual == source.source && *source_zone_change_count == source.zone_change_count
                        && *controller == source.controller))
    })
}

pub(crate) fn record_published(state: &mut GameState) -> Result<(), String> {
    if !state.etali_commit_receipts_v1 {
        return Ok(());
    }
    let Some(item) = state.stack.last().filter(|item| selected(item)).cloned() else { return Ok(()) };
    let source = original_source(state, &item)?;
    let attack_at = state.engine.event_history.iter().rposition(|event| matches!(
        event,
        E::EtaliPrimalStormReceiptV1 { journal }
            if matches!(journal.as_ref(), JournalV1::Attacked { source: captured } if *captured == source)
    )).ok_or("Etali native declaration witness absent")?;
    push(state, JournalV1::Published {
        attack_at: u64::try_from(attack_at).map_err(|_| "Etali declaration index width")?,
        item: Box::new(item),
    });
    Ok(())
}

pub(crate) fn record_batch_started(state: &mut GameState, id: StackItemId, expected: usize) -> Result<(), String> {
    let expected_moves = u8::try_from(expected).map_err(|_| "Etali native top batch width")?;
    push(state, JournalV1::Started { id, expected_moves });
    Ok(())
}

pub(crate) fn record_batch_committed(state: &mut GameState, id: StackItemId, native_start: usize) -> Result<(), String> {
    let native_start = u64::try_from(native_start).map_err(|_| "Etali top batch index width")?;
    push(state, JournalV1::BatchCommitted { id, native_start });
    Ok(())
}

pub(crate) fn exile_proposal(object: ObjectId, id: StackItemId) -> ProposedEvent {
    let ProposedEvent::ZoneChange(mut change) = ProposedEvent::zone_change(object, Zone::Exile)
    else { unreachable!("native zone-change constructor") };
    change.etali_parent = Some(id);
    ProposedEvent::ZoneChange(change)
}

pub(crate) fn observe_exile_visibility(
    state: &mut GameState,
    id: StackItemId,
    object: ObjectId,
    event_at: usize,
) {
    if matches!(state.engine.event_history.get(event_at),
        Some(E::ZoneChange { object: actual, from: Zone::Library, to: Zone::Exile, .. })
            if *actual == object)
        && state.objects.get(object).v4.face_down_v1.is_none()
    {
        let arrival = AbilitySourceContractV4::capture(state, object);
        push(state, JournalV1::TopExileVisibility { id, arrival });
    }
}

pub(crate) fn record_exiled(
    state: &mut GameState,
    id: StackItemId,
    original: AbilitySourceContractV4,
    event_at: usize,
) {
    if !matches!(state.engine.event_history.get(event_at),
        Some(E::ZoneChange { object, from: Zone::Library, to: Zone::Exile, .. })
            if *object == original.source)
        || state.objects.get(original.source).v4.face_down_v1.is_some()
    {
        return;
    }
    let arrival = AbilitySourceContractV4::capture(state, original.source);
    push(state, JournalV1::TopExiled { id, row: ExileV1 { original, arrival } });
}

pub(crate) fn owns_pending_cast(state: &GameState, spell: ObjectId) -> bool {
    state.etali_commit_receipts_v1
        && state.standard_v1.as_ref().and_then(|standard| standard.resolution_play.as_ref())
            .is_some_and(|context|
                context.permission.is_some_and(|permit| permit.card.object == spell)
                    && context.suspended.as_ref()
                        .is_some_and(|pending| selected(&pending.resolving_item)))
}

pub(crate) fn record_cast(
    state: &mut GameState,
    child: StackItemId,
    source: StackSourceContractV4,
    cast_at: usize,
) -> Result<(), String> {
    if !state.etali_commit_receipts_v1 {
        return Ok(());
    }
    let Some(context) = state.standard_v1.as_ref().and_then(|standard| standard.resolution_play.as_ref())
        .filter(|context| context.suspended.as_ref()
            .is_some_and(|pending| selected(&pending.resolving_item)))
    else { return Ok(()) };
    let permission = context.permission.ok_or("Etali native child cast lost its permission")?;
    if permission.land || permission.maximum_mana_value.is_some() || permission.card.object != source.source
        || permission.controller != source.controller || permission.card.expected_zone != Zone::Exile
        || !state.stack.iter().any(|item| item.v4.stack_item_id == child && item.v4.x_value == 0
            && item.v4.source_contract == Some(source))
    {
        return Err("Etali actual native child changed free Exile route".into());
    }
    let id = context.parent;
    let cast_at = u64::try_from(cast_at).map_err(|_| "Etali cast index width")?;
    push(state, JournalV1::Cast {
        id, row: CastV1 { cast_at, original: permission.card, child, source },
    });
    Ok(())
}

pub(crate) fn record_finished(state: &mut GameState, item: &StackItem, declined: bool) {
    if state.etali_commit_receipts_v1 && selected(item) {
        push(state, JournalV1::Finished { id: item.v4.stack_item_id, declined });
    }
}

pub(crate) fn record_countered(state: &mut GameState, item: &StackItem) {
    if state.etali_commit_receipts_v1 && selected(item) {
        push(state, JournalV1::Countered { id: item.v4.stack_item_id });
    }
}

pub(crate) fn has_receipts(state: &GameState) -> bool {
    state.etali_commit_receipts_v1
        || state.engine.event_history.has_any(&[EventKindV1::EtaliPrimalStormReceiptV1])
}

fn original_source(state: &GameState, item: &StackItem) -> Result<AbilitySourceContractV4, String> {
    let source = item.v4.ability_source_contract.ok_or("Etali original native source absent")?;
    if !selected(item) || item.kind != StackItemKind::TriggeredAbility || item.is_copy
        || item.v4.granted_by.is_some() || !item.targets.is_empty()
        || !item.v4.target_contracts.is_empty() || item.v4.target_spec.is_some()
        || item.v4.stack_item_id == StackItemId::default()
        || source.zone != Zone::Battlefield || !printing::definition(source.card_def)
        || source.source != item.source || source.controller != item.controller
    {
        return Err("Etali actual trigger changed exact printed source".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, source)?;
    Ok(source)
}

#[derive(Default)]
struct Operations<'a> {
    published: Option<(usize, u64, &'a StackItem)>,
    started: Option<(usize, u8)>,
    committed: Option<(usize, u64)>,
    exiled: Vec<(usize, ExileV1)>,
    visibility_count: usize,
    casts: Vec<(usize, CastV1)>,
    finished: Option<(usize, bool)>,
    countered: Option<usize>,
}

pub(crate) struct Occurrence {
    pub source: AbilitySourceContractV4,
    pub stack_position: Option<u32>,
    pub id: StackItemId,
    pub exiled: Vec<ExileV1>,
    pub casts: Vec<CastV1>,
    pub started: bool,
    pub finished: Option<bool>,
    pub countered: bool,
}

pub(crate) fn occurrences(state: &GameState) -> Result<Vec<Occurrence>, String> {
    if !state.etali_commit_receipts_v1 {
        return Err("Etali requires its scoped native receipt mode".into());
    }
    let history = &state.engine.event_history;
    let mut rows = BTreeMap::<StackItemId, Operations<'_>>::new();
    for (at, event) in history.scan_for_v1(&[EventKindV1::EtaliPrimalStormReceiptV1]).iter().enumerate() {
        let E::EtaliPrimalStormReceiptV1 { journal } = event else { continue };
        let duplicate = match journal.as_ref() {
            JournalV1::Attacked { .. } => false,
            JournalV1::Published { attack_at, item } =>
                rows.entry(item.v4.stack_item_id).or_default()
                    .published.replace((at, *attack_at, item.as_ref())).is_some(),
            JournalV1::Started { id, expected_moves } =>
                rows.entry(*id).or_default().started.replace((at, *expected_moves)).is_some(),
            JournalV1::BatchCommitted { id, native_start } =>
                rows.entry(*id).or_default().committed.replace((at, *native_start)).is_some(),
            JournalV1::TopExileVisibility { id, .. } => {
                rows.entry(*id).or_default().visibility_count += 1;
                false
            }
            JournalV1::TopExiled { id, row } => {
                rows.entry(*id).or_default().exiled.push((at, *row));
                false
            }
            JournalV1::Cast { id, row } => {
                rows.entry(*id).or_default().casts.push((at, *row));
                false
            }
            JournalV1::Finished { id, declined } =>
                rows.entry(*id).or_default().finished.replace((at, *declined)).is_some(),
            JournalV1::Countered { id } => rows.entry(*id).or_default().countered.replace(at).is_some(),
        };
        if duplicate {
            return Err("Etali duplicated its original native occurrence".into());
        }
    }
    let mut attacks = BTreeSet::new();
    let mut result = Vec::with_capacity(rows.len());
    for (id, ops) in rows {
        let (published, attack_at, item) = ops.published.ok_or("Etali original publication absent")?;
        let source = original_source(state, item)?;
        let attack = usize::try_from(attack_at).map_err(|_| "Etali attack index width")?;
        if attack == 0 || attack >= published || !attacks.insert(attack)
            || !matches!(history.get(attack),
                Some(E::EtaliPrimalStormReceiptV1 { journal })
                    if matches!(journal.as_ref(), JournalV1::Attacked { source: observed } if *observed == source))
            || !matches!(history.get(attack - 1),
                Some(E::DeclaredAttacker { source: actual, source_zone_change_count, controller })
                    if *actual == source.source && *source_zone_change_count == source.zone_change_count
                        && *controller == source.controller)
        {
            return Err("Etali original publication does not bind a unique actual declaration".into());
        }
        let mut live = state.stack.iter().enumerate().filter(|(_, row)| row.v4.stack_item_id == id);
        let stack_position = match live.next() {
            Some((at, original)) if original == item && live.next().is_none() =>
                Some(u32::try_from(at).map_err(|_| "Etali stack position width")?),
            None => None,
            _ => return Err("Etali live stack row changed".into()),
        };
        let terminal = ops.finished.map(|(at, _)| at).or(ops.countered);
        if terminal.is_some() == stack_position.is_some()
            || ops.countered.is_some_and(|at| at <= published || ops.started.is_some() || ops.finished.is_some())
            || ops.finished.is_some() && ops.started.is_none()
            || ops.visibility_count != ops.exiled.len()
            || ops.started.is_none() && (!ops.exiled.is_empty() || !ops.casts.is_empty())
        {
            return Err("Etali original occurrence changed terminal lifetime".into());
        }
        let batch_end = match (ops.started, ops.committed) {
            (Some((start, count)), Some((end, native_start))) => {
                if start <= published || start >= end
                    || usize::try_from(native_start).ok() != Some(start)
                    || terminal.is_some_and(|terminal| terminal <= end)
                    || count > 2 || usize::from(count) != ops.exiled.len()
                {
                    return Err("Etali actual native top batch lost original roster".into());
                }
                let actual_moves = history[start + 1..end].iter().enumerate()
                    .filter_map(|(offset, event)| match event {
                        E::ZoneChange { object, from: Zone::Library, to: Zone::Exile, .. } =>
                            Some((start + 1 + offset, *object)),
                        _ => None,
                    });
                if !actual_moves.eq(ops.exiled.iter().map(|(at, row)| (at.saturating_sub(2), row.original.source))) {
                    return Err("Etali typed top exile roster omits or changes native batch moves".into());
                }
                Some(end)
            }
            (None, None) => None,
            _ => return Err("Etali native top batch lost its commit boundary".into()),
        };
        let mut owners = BTreeSet::new();
        let mut exiled = Vec::with_capacity(ops.exiled.len());
        let mut last = ops.started.map_or(published, |(at, _)| at);
        for (at, row) in ops.exiled {
            let original = row.original;
            let arrival = row.arrival;
            if at <= last || at < 2 || terminal.is_some_and(|end| end <= at)
                || !owners.insert(original.owner)
                || original.zone != Zone::Library || arrival.zone != Zone::Exile
                || original.source != arrival.source || original.card_def != arrival.card_def
                || original.owner != arrival.owner || arrival.owner != arrival.controller
                || original.zone_change_count.checked_add(1) != Some(arrival.zone_change_count)
                || !matches!(history.get(at - 1), Some(E::EtaliPrimalStormReceiptV1 { journal })
                    if matches!(journal.as_ref(), JournalV1::TopExileVisibility { id: observed, arrival: face }
                        if *observed == id && *face == arrival))
                || !matches!(history.get(at - 2), Some(E::ZoneChange { object, from: Zone::Library, to: Zone::Exile, controller_before })
                    if *object == original.source && *controller_before == original.controller)
                || state.objects.try_get(arrival.source).is_none_or(|live| live.owner != arrival.owner
                    || (live.zone_change_count == arrival.zone_change_count
                        && (live.zone != Zone::Exile || live.card_def != arrival.card_def || live.v4.face_down_v1.is_some())))
                || crate::card_def::CARD_DEFS.get(usize::from(arrival.card_def)).is_none()
            {
                return Err("Etali actual top exile changed public owner, incarnation or printing".into());
            }
            last = at;
            exiled.push(row);
        }
        last = batch_end.unwrap_or(last);
        let mut cast_objects = BTreeSet::new();
        let mut casts = Vec::with_capacity(ops.casts.len());
        for (at, row) in ops.casts {
            let Some(top) = exiled.iter().find(|top| top.arrival.source == row.original.object)
            else { return Err("Etali child cast was not in original exile batch".into()) };
            let spell = row.source;
            let Some(origin) = spell.spell_cast_origin else { return Err("Etali actual cast origin absent".into()) };
            let cast_at = usize::try_from(row.cast_at).map_err(|_| "Etali cast index width")?;
            if cast_at <= last || cast_at >= at || terminal.is_some_and(|end| end <= at)
                || !cast_objects.insert(spell.source)
                || row.original.expected_zone != Zone::Exile
                || row.original.expected_zone_change_count != top.arrival.zone_change_count
                || spell.card_def != top.arrival.card_def || spell.owner != top.arrival.owner
                || spell.source != row.original.object || spell.controller != source.controller
                || spell.zone != Zone::Stack || spell.spell_copy_origin.is_some()
                || spell.finalized_cast_binding.is_none_or(|binding| binding.x_value != 0)
                || origin.origin_zone != Zone::Exile
                || origin.origin_zone_change_count != row.original.expected_zone_change_count
                || origin.route != (SpellCastRouteV4::ResolvingEffectV1 {
                    holder: source.controller, maximum_mana_value: None,
                })
                || !matches!(history.get(cast_at), Some(E::SpellCast { spell: actual, controller })
                    if *actual == spell.source && *controller == source.controller)
                || !matches!(history.get(cast_at - 1), Some(E::PublishedCastIdentityV1 { receipt })
                    if receipt.spell_cast_history_index == row.cast_at
                        && receipt.spell == spell.source && receipt.stack_item_id == row.child
                        && receipt.owner == spell.owner && receipt.controller == spell.controller
                        && receipt.origin_zone == Zone::Exile
                        && receipt.origin_zone_change_count == row.original.expected_zone_change_count
                        && receipt.stack_zone_change_count == spell.zone_change_count
                        && receipt.public_identity == (PublicIdentityV1::Known { card_def: spell.card_def }))
                || crate::card_def::CARD_DEFS[usize::from(spell.card_def)].is_land
            {
                return Err("Etali child is not the exact actually published free cast".into());
            }
            last = at;
            casts.push(row);
        }
        result.push(Occurrence {
            source, stack_position, id, exiled, casts, started: ops.started.is_some(),
            finished: ops.finished.map(|(_, declined)| declined), countered: ops.countered.is_some(),
        });
    }
    if state.stack.iter().any(|item| selected(item)
        && !result.iter().any(|row| row.id == item.v4.stack_item_id))
    {
        return Err("Etali live trigger lost its native publication".into());
    }
    Ok(result)
}

/// Bind the saved open or answered native batch to the exact committed top
/// moves and to real casts. The authorization may reserve one current child
/// before that child's native SpellCast is committed.
pub(crate) fn validate_current(state: &GameState, rows: &[Occurrence]) -> Result<(), String> {
    use crate::effect::{standard_exile as native, EffectFrame, EffectTargetSelectionPurpose, PendingEffectChoice};
    let context = state.standard_v1.as_ref().and_then(|standard| standard.resolution_play.as_ref());
    let suspended = context.and_then(|context| context.suspended.as_deref())
        .filter(|pending| native::top_batch_pending(pending));
    let active = state.engine.pending_effect.as_ref()
        .filter(|pending| native::top_batch_pending(pending));
    let (pending, reserved) = match (active, suspended) {
        (Some(pending), None) => {
            crate::effect::validate_pending_effect_choice(state)?;
            (pending, None)
        }
        (None, Some(pending)) => {
            let child = state.engine.pending_cast.as_ref().ok_or("Etali suspended child absent")?;
            crate::engine::validate_pending_cast(state, child)?;
            let permission = context.and_then(|context| context.permission)
                .ok_or("Etali suspended child lacks its original permission")?;
            (pending, Some(permission.card))
        }
        (None, None) => {
            if rows.iter().any(|row| row.started && row.stack_position.is_some()) {
                return Err("Etali active batch lost its native continuation".into());
            }
            return Ok(());
        }
        (Some(_), Some(_)) => return Err("Etali native continuation is duplicated".into()),
    };
    let row = rows.iter().find(|row| row.id == pending.resolving_item.v4.stack_item_id
        && row.started && row.stack_position.is_some())
        .ok_or("Etali continuation lost its exact native operation")?;
    let choice = match pending.choice.as_ref() {
        Some(PendingEffectChoice::SelectTargets {
            purpose: EffectTargetSelectionPurpose::ExileBatch { choice }, legal, selected, ..
        }) => {
            if !selected.is_empty() || !legal.iter().map(|candidate| candidate.expected_object)
                .eq(native::batch_candidates(state, pending, choice).into_iter().map(Some))
            {
                return Err("Etali native open candidate order changed".into());
            }
            choice
        }
        None => pending.frames.iter().find_map(|frame| match frame {
            EffectFrame::ExileBatchSelect { choice, .. } | EffectFrame::ExileBatchResume { choice } => Some(choice),
            _ => None,
        }).ok_or("Etali continuation lost original exile stage")?,
        _ => return Err("Etali continuation has an unrelated choice".into()),
    };
    let observed = row.exiled.iter().map(|row| EffectObjectBinding {
        object: row.arrival.source, expected_zone: Zone::Exile,
        expected_zone_change_count: row.arrival.zone_change_count,
    });
    let paid = row.casts.iter().map(|cast| cast.original).chain(reserved);
    if !choice.scans.iter().flat_map(|scan| scan.exiled.iter().copied()).eq(observed)
        || !choice.cast.iter().copied().eq(paid)
    {
        return Err("Etali current original top or paid cast roster changed".into());
    }
    Ok(())
}
