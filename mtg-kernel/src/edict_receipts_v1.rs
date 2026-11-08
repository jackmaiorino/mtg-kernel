//! Private custody for Edict's native payment and simultaneous sacrifices.
use crate::{
    effect::EffectObjectBinding,
    event::CommittedEvent as E,
    event_history_v1::EventKindV1,
    ids::{ObjectId, PlayerId, StackItemId},
    state::{GameState, StackItem, StackSourceContractV4, Zone},
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
    Paid(PaymentV1),
    Batch {
        item: Box<StackItem>,
        active: PlayerId,
        victims: Vec<VictimV1>,
        sacrifice_start: u64,
        move_start: u64,
    },
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

pub(crate) fn capture_victims(
    state: &GameState,
    bindings: &[EffectObjectBinding],
) -> Result<Vec<VictimV1>, String> {
    bindings.iter().map(|binding| {
        let live = state.objects.try_get(binding.object).ok_or("Edict original sacrifice absent")?;
        if live.zone != Zone::Battlefield || binding.expected_zone != Zone::Battlefield
            || live.zone_change_count != binding.expected_zone_change_count
        {
            return Err("Edict original sacrifice incarnation changed".into());
        }
        Ok(VictimV1 {
            binding: *binding,
            owner: live.owner,
            controller: live.controller,
            battlefield_ordinal: u32::try_from(
                state.players[live.controller.index()].battlefield.iter()
                    .position(|&object| object == binding.object)
                    .ok_or("Edict original battlefield position absent")?
            ).map_err(|_| "Edict original battlefield position exceeds u32")?,
            public_printing: crate::rl::projected_card_def(state, binding.object, live.zone_change_count),
        })
    }).collect()
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

pub(crate) fn record_batch(
    state: &mut GameState,
    item: &StackItem,
    active: PlayerId,
    victims: Vec<VictimV1>,
    sacrifice_start: usize,
    move_start: usize,
) -> Result<(), String> {
    push(state, JournalV1::Batch {
        item: Box::new(item.clone()),
        active,
        victims,
        sacrifice_start: u64::try_from(sacrifice_start).map_err(|_| "Edict sacrifice offset width")?,
        move_start: u64::try_from(move_start).map_err(|_| "Edict move offset width")?,
    });
    Ok(())
}

fn all_payments(state: &GameState) -> Result<BTreeMap<StackItemId, (usize, PaymentV1)>, String> {
    let mut result = BTreeMap::new();
    let history = &state.engine.event_history;
    for (at, receipt) in records(state) {
        let JournalV1::Paid(payment) = receipt else { continue };
        let source = payment.source;
        let cast_at = usize::try_from(payment.cast_at).map_err(|_| "Edict cast history width")?;
        let method = source.spell_cast_origin.and_then(|origin| origin.finalized_method);
        if !is_printing(source.card_def) || source.zone != Zone::Stack || source.spell_copy_origin.is_some()
            || method != Some(source.cast_method)
            || !matches!(source.cast_method, crate::state::CastMethodV4::Normal | crate::state::CastMethodV4::Alternative)
            || source.cast_method == crate::state::CastMethodV4::Alternative
                && payment.creatures_before < LIMIT as u32
            || payment.stack_item_id == StackItemId::default()
            || cast_at >= at
            || !matches!(history.get(cast_at),
                Some(E::SpellCast { spell, controller }) if *spell == source.source && *controller == source.controller)
            || result.insert(payment.stack_item_id, (at, *payment)).is_some()
        {
            return Err("Edict committed payment lost original spell or alternate condition".into());
        }
        crate::engine::validate_historical_ability_source_contract(state, crate::state::AbilitySourceContractV4 {
            source: source.source,
            card_def: source.card_def,
            owner: source.owner,
            controller: source.controller,
            zone: source.zone,
            zone_change_count: source.zone_change_count,
            attached_to: None,
        })?;
    }
    Ok(result)
}

pub(crate) fn payment_for(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<PaymentV1>, String> {
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

pub(crate) struct OutcomeV1 {
    pub item: StackSourceContractV4,
    pub copied_spell: bool,
    pub active: PlayerId,
    pub victims: Vec<(VictimV1, Option<Zone>)>,
    pub graveyard: [Vec<ObjectId>; 2],
    pub payment: Option<PaymentV1>,
}

pub(crate) fn outcomes(state: &GameState) -> Result<Vec<OutcomeV1>, String> {
    let history = &state.engine.event_history;
    let payments = all_payments(state)?;
    let mut ids = BTreeSet::new();
    let mut outcomes = vec![];
    for (at, receipt) in records(state) {
        let JournalV1::Batch { item, active, victims, sacrifice_start, move_start } = receipt else { continue };
        let source = item.v4.source_contract.ok_or("Edict resolution lost original stack source")?;
        if !is_printing(source.card_def) || source.zone != Zone::Stack
            || item.source != source.source || item.controller != source.controller
            || item.kind != crate::state::StackItemKind::Spell
            || item.v4.target_spec != Some(crate::card_def::TargetSpec::None) || !item.targets.is_empty()
            || item.is_copy != source.spell_copy_origin.is_some()
            || item.v4.stack_item_id == StackItemId::default()
            || !ids.insert(item.v4.stack_item_id)
        {
            return Err("Edict completed receipt lost exact original occurrence".into());
        }
        let start = usize::try_from(*sacrifice_start).map_err(|_| "Edict sacrifice offset width")?;
        let movement = usize::try_from(*move_start).map_err(|_| "Edict movement offset width")?;
        let payment = if item.is_copy {
            None
        } else {
            let (paid_at, payment) = payments.get(&item.v4.stack_item_id)
                .ok_or("Edict resolution requires original payment")?;
            if *paid_at >= start || payment.source != source {
                return Err("Edict resolution payment changed or reordered".into());
            }
            Some(*payment)
        };
        let sacrifice_rows = history.get(start..movement).ok_or("Edict sacrifice span changed")?;
        if sacrifice_rows.len() != victims.len() {
            return Err("Edict actual sacrifice count changed".into());
        }
        let mut destinations = BTreeMap::new();
        let mut active_seen_last = false;
        for (row, victim) in sacrifice_rows.iter().zip(victims) {
            let id = victim.binding.object;
            if !matches!(row, E::Sacrificed { object, controller_before, .. } if *object == id && *controller_before == victim.controller)
                || victim.binding.expected_zone != Zone::Battlefield
                || state.objects.try_get(id).is_none_or(|live| live.owner != victim.owner
                    || live.zone_change_count < victim.binding.expected_zone_change_count)
                || crate::card_def::CARD_DEFS.get(usize::from(victim.public_printing)).is_none()
                || destinations.insert(id, (victim, None)).is_some()
                || (active_seen_last && victim.controller == *active)
            {
                return Err("Edict actual sacrifice receipt lost original public victim or order".into());
            }
            active_seen_last |= victim.controller != *active;
        }
        let mut graveyard = [vec![], vec![]];
        for row in history.get(movement..at).ok_or("Edict committed movement span changed")? {
            let E::ZoneChange { object, from: Zone::Battlefield, to, controller_before } = row else { continue };
            if let Some((victim, destination)) = destinations.get_mut(object) {
                if *controller_before != victim.controller || destination.replace(*to).is_some() {
                    return Err("Edict original sacrifice has duplicate or foreign movement".into());
                }
                if *to == Zone::Graveyard {
                    graveyard[victim.owner.index()].push(*object);
                }
            }
        }
        let mut actual = victims.iter().map(|victim| (*victim, destinations[&victim.binding.object].1))
            .collect::<Vec<_>>();
        // Sacrifice is simultaneous. Publish each controller's original
        // battlefield order without exposing the order of their prompt clicks.
        actual.sort_by_key(|(victim, _)| (victim.controller != *active, victim.battlefield_ordinal));
        outcomes.push(OutcomeV1 {
            item: source,
            copied_spell: item.is_copy,
            active: *active,
            victims: actual,
            graveyard,
            payment,
        });
    }
    Ok(outcomes)
}
