//! Source-bound native Edict payment and simultaneous departure receipts.
use crate::{
    cast_identity_v1::PublicIdentityV1,
    effect::EffectObjectBinding,
    event::CommittedEvent as E,
    event_history_v1::EventKindV1,
    ids::{ObjectId, PlayerId, StackItemId},
    state::{CastMethodV4, GameState, StackItem, StackItemKind, StackSourceContractV4, Zone},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const PRINTING: u16 = 469;
pub(crate) const LIMIT: usize = 13;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VictimV1 {
    pub binding: EffectObjectBinding,
    pub owner: PlayerId,
    pub controller: PlayerId,
    pub battlefield_ordinal: u32,
    pub public_printing: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentV1 {
    pub stack_item_id: StackItemId,
    pub source: StackSourceContractV4,
    pub creatures_before: u32,
    pub mana_spent: u32,
    pub cast_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum JournalV1 {
    ManaCommitted {
        id: StackItemId, original: StackSourceContractV4, method: CastMethodV4,
        creatures: u32, quoted: u32, spent: u32,
    },
    Paid(PaymentV1),
    BatchBegin { item: Box<StackItem>, active: PlayerId },
    Sacrifice { id: StackItemId, event_at: usize, victim: VictimV1 },
    Move { id: StackItemId, event_at: usize, binding: EffectObjectBinding, destination: Zone },
    BatchEnded { id: StackItemId, sacrifices: usize, moves: usize },
}

pub(crate) fn is_printing(id: u16) -> bool {
    cfg!(all(feature = "limited-fdn-fixtures", not(feature = "standard-magezero-fixtures")))
        && id == PRINTING && crate::effect::blasphemous_edict_v1::definition(id)
}

pub(crate) fn creature_count(state: &GameState) -> Result<u32, String> {
    let count = state.objects.iter().filter(|(id, object)| {
        object.zone == Zone::Battlefield
            && crate::engine::object_has_type(state, *id, crate::card_def::CardType::Creature)
    }).count();
    u32::try_from(count).map_err(|_| "Edict creature count exceeds u32".into())
}

fn records(state: &GameState) -> impl Iterator<Item = (usize, &JournalV1)> {
    state.engine.event_history.scan_for_v1(&[EventKindV1::BlasphemousEdictReceiptV1])
        .iter().enumerate().filter_map(|(position, event)| match event {
            E::BlasphemousEdictReceiptV1 { journal } => Some((position, journal.as_ref())),
            _ => None,
        })
}

fn push(state: &mut GameState, journal: JournalV1) {
    state.engine.event_history.push(E::BlasphemousEdictReceiptV1 { journal: Box::new(journal) });
}

pub(crate) fn record_mana_committed(
    state: &mut GameState,
    pending: &crate::engine::PendingCast,
    method: CastMethodV4,
    creatures: u32,
    quoted: u32,
    spent: u32,
) -> Result<(), String> {
    let id = state.stack.last().filter(|item|
        item.source == pending.spell && item.v4.source_contract == Some(pending.source_contract))
        .map(|item| item.v4.stack_item_id)
        .ok_or("Edict mana commitment lost original cast placeholder")?;
    if quoted != spent {
        return Err("Edict actual mana commitment differs from the prepared native plan".into());
    }
    push(state, JournalV1::ManaCommitted {
        id, original: pending.source_contract, method, creatures, quoted, spent,
    });
    Ok(())
}

pub(crate) fn record_paid(
    state: &mut GameState,
    stack_item_id: StackItemId,
    source: StackSourceContractV4,
    creatures_before: u32,
    mana_spent: u32,
    cast_at: usize,
) -> Result<(), String> {
    let cast_at = u64::try_from(cast_at).map_err(|_| "Edict cast history width")?;
    push(state, JournalV1::Paid(PaymentV1 { stack_item_id, source, creatures_before, mana_spent, cast_at }));
    Ok(())
}

fn all_payments(state: &GameState) -> Result<BTreeMap<StackItemId, (usize, PaymentV1)>, String> {
    let mut result = BTreeMap::new();
    let history = &state.engine.event_history;
    for (at, receipt) in records(state) {
        let JournalV1::Paid(payment) = receipt else { continue };
        let source = payment.source;
        let cast_at = usize::try_from(payment.cast_at).map_err(|_| "Edict cast history width")?;
        let origin = source.spell_cast_origin.ok_or("Edict paid spell lost native origin")?;
        let mut commits = records(state).filter_map(|(position, row)| match row {
            JournalV1::ManaCommitted { id, original, method, creatures, quoted, spent } if *id == payment.stack_item_id =>
                Some((position, original, *method, *creatures, *quoted, *spent)),
            _ => None,
        });
        let (committed_at, original, method, creatures, quoted, spent) =
            commits.next().ok_or("Edict paid spell lost actual native mana commitment")?;
        let published = cast_at.checked_sub(1).and_then(|position| history.get(position));
        if !is_printing(source.card_def) || source.zone != Zone::Stack || source.spell_copy_origin.is_some()
            || origin.finalized_method != Some(source.cast_method)
            || !matches!(source.cast_method, CastMethodV4::Normal | CastMethodV4::Alternative)
            || source.cast_method == CastMethodV4::Alternative && payment.creatures_before < LIMIT as u32
            || payment.stack_item_id == StackItemId::default()
            || commits.next().is_some() || committed_at >= cast_at || cast_at >= at
            || method != source.cast_method || creatures != payment.creatures_before
            || quoted != spent || spent != payment.mana_spent
            || original.source != source.source || original.owner != source.owner
            || original.controller != source.controller || original.card_def != source.card_def
            || original.zone != source.zone || original.zone_change_count != source.zone_change_count
            || original.spell_copy_origin.is_some()
            || original.spell_cast_origin.is_none_or(|original_origin|
                original_origin.origin_zone != origin.origin_zone
                    || original_origin.origin_zone_change_count != origin.origin_zone_change_count
                    || original_origin.route != origin.route
                    || original_origin.finalized_method.is_some())
            || !matches!(published, Some(E::PublishedCastIdentityV1 { receipt })
                if receipt.spell_cast_history_index == payment.cast_at && receipt.spell == source.source
                    && receipt.owner == source.owner && receipt.controller == source.controller
                    && receipt.stack_item_id == payment.stack_item_id
                    && receipt.stack_zone_change_count == source.zone_change_count
                    && receipt.origin_zone == origin.origin_zone
                    && receipt.origin_zone_change_count == origin.origin_zone_change_count
                    && receipt.public_identity == (PublicIdentityV1::Known { card_def: PRINTING }))
            || !matches!(history.get(cast_at),
                Some(E::SpellCast { spell, controller }) if *spell == source.source && *controller == source.controller)
            || result.insert(payment.stack_item_id, (at, *payment)).is_some()
        {
            return Err("Edict paid receipt changed original spell, native payment or alternate condition".into());
        }
        crate::engine::validate_historical_ability_source_contract(state, crate::state::AbilitySourceContractV4 {
            source: source.source, card_def: source.card_def, owner: source.owner, controller: source.controller,
            zone: source.zone, zone_change_count: source.zone_change_count, attached_to: None,
        })?;
    }
    for (position, row) in records(state) {
        if let JournalV1::ManaCommitted { id, .. } = row {
            if result.get(id).is_none_or(|(paid_at, _)| *paid_at <= position) {
                return Err("Edict native mana commitment has no finalized cast".into());
            }
        }
    }
    for event in history.scan_for_v1(&[EventKindV1::PublishedCastIdentityV1]) {
        if let E::PublishedCastIdentityV1 { receipt } = event {
            if receipt.public_identity == (PublicIdentityV1::Known { card_def: PRINTING })
                && result.get(&receipt.stack_item_id).is_none_or(|(_, payment)|
                    payment.cast_at != receipt.spell_cast_history_index)
            {
                return Err("Edict historical cast requires its native payment receipt".into());
            }
        }
    }
    Ok(result)
}

pub(crate) fn payment_for(state: &GameState, item: &StackItem) -> Result<Option<PaymentV1>, String> {
    if item.is_copy {
        return Ok(None);
    }
    let payments = all_payments(state)?;
    let (_, payment) = payments.get(&item.v4.stack_item_id).ok_or("Edict physical spell requires its paid receipt")?;
    if item.v4.source_contract != Some(payment.source) {
        return Err("Edict paid receipt no longer names exact live source".into());
    }
    Ok(Some(*payment))
}

pub(crate) fn record_batch_begin(state: &mut GameState, item: &StackItem, active: PlayerId) {
    push(state, JournalV1::BatchBegin { item: Box::new(item.clone()), active });
}

pub(crate) fn record_sacrifice(
    state: &mut GameState, id: StackItemId, binding: EffectObjectBinding,
) -> Result<(), String> {
    let live = state.objects.try_get(binding.object).ok_or("Edict original victim absent")?;
    if live.zone != Zone::Battlefield || binding.expected_zone != Zone::Battlefield
        || live.zone_change_count != binding.expected_zone_change_count
    {
        return Err("Edict original victim changed incarnation".into());
    }
    let victim = VictimV1 {
        binding, owner: live.owner, controller: live.controller,
        battlefield_ordinal: u32::try_from(state.players[live.controller.index()].battlefield.iter()
            .position(|&object| object == binding.object).ok_or("Edict original battlefield ordinal absent")?)
            .map_err(|_| "Edict original battlefield ordinal exceeds u32")?,
        public_printing: crate::rl::projected_card_def(state, binding.object, live.zone_change_count),
    };
    let event_at = state.engine.event_history.len();
    crate::event::log_sacrifice(state, binding.object);
    push(state, JournalV1::Sacrifice { id, event_at, victim });
    Ok(())
}

pub(crate) fn record_move(
    state: &mut GameState, id: StackItemId, binding: EffectObjectBinding, event_at: usize,
) -> Result<(), String> {
    let Some(E::ZoneChange { object, from: Zone::Battlefield, to, .. }) =
        state.engine.event_history.get(event_at) else {
            return Err("Edict actual movement is not a committed battlefield departure".into());
        };
    if *object != binding.object {
        return Err("Edict actual movement lost original victim".into());
    }
    let destination = *to;
    push(state, JournalV1::Move { id, event_at, binding, destination });
    Ok(())
}

fn batch_start(state: &GameState, id: StackItemId) -> Result<(usize, &StackItem, PlayerId), String> {
    let mut starts = records(state).filter_map(|(index, receipt)| match receipt {
        JournalV1::BatchBegin { item, active } if item.v4.stack_item_id == id =>
            Some((index, item.as_ref(), *active)),
        _ => None,
    });
    let start = starts.next().ok_or("Edict actual batch lost original native start")?;
    if starts.next().is_some() {
        return Err("Edict native batch start duplicated".into());
    }
    Ok(start)
}

type ActualBatch = (Vec<(VictimV1, Option<Zone>)>, [Vec<ObjectId>; 2], usize);

fn actual_batch(state: &GameState, id: StackItemId, begin: usize, end: usize, active: PlayerId) -> Result<ActualBatch, String> {
    let history = &state.engine.event_history;
    let mut victims = BTreeMap::new();
    let mut seen_ordinals = BTreeSet::new();
    let mut active_seen_last = false;
    let mut moves = 0;
    let mut graveyard = [vec![], vec![]];
    for (at, receipt) in records(state) {
        match receipt {
            JournalV1::Sacrifice { id: bound, event_at, victim } if *bound == id => {
                let object = victim.binding.object;
                if *event_at <= begin || at >= end || event_at.checked_add(1) != Some(at)
                    || !matches!(history.get(*event_at), Some(E::Sacrificed { object: actual, controller_before, .. })
                        if *actual == object && *controller_before == victim.controller)
                    || moves != 0 || victim.binding.expected_zone != Zone::Battlefield
                    || !seen_ordinals.insert((victim.controller, victim.battlefield_ordinal))
                    || state.objects.try_get(object).is_none_or(|live| live.owner != victim.owner
                        || live.zone_change_count < victim.binding.expected_zone_change_count)
                    || crate::card_def::CARD_DEFS.get(usize::from(victim.public_printing)).is_none()
                    || victims.insert(object, (*victim, None)).is_some()
                    || (active_seen_last && victim.controller == active)
                {
                    return Err("Edict original committed sacrifice event changed".into());
                }
                active_seen_last |= victim.controller != active;
            }
            JournalV1::Move { id: bound, event_at, binding, destination } if *bound == id => {
                let (victim, seen) = victims.get_mut(&binding.object).ok_or("Edict move has no original sacrifice")?;
                // Niv's own older receipt retains its adjacent departure marker.
                let directly_bound = event_at.checked_add(1) == Some(at)
                    || event_at.checked_add(2) == Some(at)
                        && matches!(history.get(at - 1),
                            Some(E::NivMizzetVisionaryDamageSourceDepartureV1 { source, departure_history_index })
                                if source.source == binding.object && source.zone_change_count == binding.expected_zone_change_count
                                    && usize::try_from(*departure_history_index).ok() == Some(*event_at));
                if *event_at <= begin || at >= end || !directly_bound
                    || *binding != victim.binding || seen.replace(*destination).is_some()
                    || !matches!(history.get(*event_at), Some(E::ZoneChange {
                        object, from: Zone::Battlefield, to, controller_before,
                    }) if *object == binding.object && *to == *destination && *controller_before == victim.controller)
                {
                    return Err("Edict actual departure event changed".into());
                }
                moves += 1;
                if *destination == Zone::Graveyard {
                    graveyard[victim.owner.index()].push(binding.object);
                }
            }
            _ => {}
        }
    }
    if victims.len() > LIMIT * 2 || [PlayerId::P0, PlayerId::P1].into_iter().any(|seat|
        victims.values().filter(|(victim, _)| victim.controller == seat).count() > LIMIT)
    {
        return Err("Edict native batch exceeds thirteen per controller".into());
    }
    let mut sacrifices = victims.into_values().collect::<Vec<_>>();
    // Preserve public original battlefield order within each APNAP group.
    // The actual graveyard order is independently read from committed moves.
    sacrifices.sort_by_key(|(victim, _)| (victim.controller != active, victim.battlefield_ordinal));
    Ok((sacrifices, graveyard, moves))
}

pub(crate) fn record_batch_ended(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    let id = item.v4.stack_item_id;
    let (begin, original, active) = batch_start(state, id)?;
    if original != item {
        return Err("Edict completed native batch changed its original row".into());
    }
    let (victims, _, moves) = actual_batch(state, id, begin, state.engine.event_history.len(), active)?;
    push(state, JournalV1::BatchEnded { id, sacrifices: victims.len(), moves });
    Ok(())
}

pub(crate) struct OutcomeV1 {
    pub item: StackSourceContractV4,
    pub copied_spell: bool,
    pub active: PlayerId,
    pub victims: Vec<(VictimV1, Option<Zone>)>,
    pub graveyard: [Vec<ObjectId>; 2],
    pub payment: Option<PaymentV1>,
}

pub(crate) fn outcomes(state: &GameState) -> Result<Vec<OutcomeV1>, String> {
    let payments = all_payments(state)?;
    let mut ids = BTreeSet::new();
    let mut outcomes = vec![];
    for (at, receipt) in records(state) {
        let JournalV1::BatchEnded { id, sacrifices, moves } = receipt else { continue };
        let (begin, item, active) = batch_start(state, *id)?;
        let source = item.v4.source_contract.ok_or("Edict resolution lost original stack source")?;
        if begin >= at || !is_printing(source.card_def) || source.zone != Zone::Stack
            || item.source != source.source || item.controller != source.controller
            || item.kind != StackItemKind::Spell
            || item.v4.target_spec != Some(crate::card_def::TargetSpec::None) || !item.targets.is_empty()
            || item.is_copy != source.spell_copy_origin.is_some()
            || *id == StackItemId::default() || !ids.insert(*id)
        {
            return Err("Edict completed receipt lost exact original occurrence".into());
        }
        let payment = if item.is_copy {
            None
        } else {
            let (paid_at, payment) = payments.get(id).ok_or("Edict resolution requires original payment")?;
            if *paid_at >= begin || payment.source != source {
                return Err("Edict resolution payment changed or reordered".into());
            }
            Some(*payment)
        };
        let (victims, graveyard, actual_moves) = actual_batch(state, *id, begin, at, active)?;
        if victims.len() != *sacrifices || actual_moves != *moves {
            return Err("Edict terminal lost actual source-bound operations".into());
        }
        outcomes.push(OutcomeV1 { item: source, copied_spell: item.is_copy, active, victims, graveyard, payment });
    }
    for (_, row) in records(state) {
        let id = match row {
            JournalV1::BatchBegin { item, .. } => item.v4.stack_item_id,
            JournalV1::Sacrifice { id, .. } | JournalV1::Move { id, .. } => *id,
            _ => continue,
        };
        if !ids.contains(&id) {
            return Err("Edict native batch operation lacks its completed terminal".into());
        }
    }
    Ok(outcomes)
}
