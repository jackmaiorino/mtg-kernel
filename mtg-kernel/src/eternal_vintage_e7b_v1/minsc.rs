//! Native provenance for Minsc & Boo's damage-then-draw reflexive trigger.

use super::{E7bOpV1, MINSC_AND_BOO};
use crate::card_def::{card_id_by_name, Subtype, TargetSpec};
use crate::effect::{EffectContinuation, EffectOp, ExecCtx};
use crate::eternal_cards_v1::owned_draws_v1::{record, JournalV1};
use crate::eternal_cards_v1::EternalOpV1;
use crate::event::{self, CommittedEvent, HistoricalDamageSourceV1, ProposedEvent};
use crate::event_history_v1::EventKindV1;
use crate::ids::{ObjectId, StackItemId};
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{
    AbilitySourceContractV4, GameState, StackItem, StackItemKind, StackTargetContractV4, Target,
    Zone,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SacrificeV1 {
    activation: StackItemId,
    source: AbilitySourceContractV4,
    creature: AbilitySourceContractV4,
    sacrifice_at: usize,
    move_at: usize,
    power: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationV1 {
    item: StackItemId,
    sacrifice: usize,
    source: AbilitySourceContractV4,
    target: Target,
    contract: StackTargetContractV4,
}

fn is_minsc(source: AbilitySourceContractV4) -> bool {
    cfg!(feature = "eternal-fixtures")
        && source.zone == Zone::Battlefield
        && card_id_by_name(MINSC_AND_BOO) == Some(source.card_def)
}

fn journal(state: &GameState, at: usize) -> Option<JournalV1> {
    match state.engine.event_history.get(at)? {
        CommittedEvent::EternalOwnedDrawV1 { journal } => Some(**journal),
        _ => None,
    }
}

fn actual_sacrifice(state: &GameState, proof: SacrificeV1, before: usize) -> bool {
    let history = &state.engine.event_history;
    is_minsc(proof.source)
        && proof.activation != StackItemId::default()
        && proof.creature.zone == Zone::Battlefield
        && proof.creature.controller == proof.source.controller
        && proof.power > 0
        && proof.sacrifice_at < proof.move_at && proof.move_at < before
        && matches!(history.get(proof.sacrifice_at),
            Some(CommittedEvent::Sacrificed { object, controller_before, effective_subtype_ids_before })
                if *object == proof.creature.source && *controller_before == proof.source.controller
                    && effective_subtype_ids_before.contains(&Subtype::Hamster.stable_id()))
        && matches!(history.get(proof.move_at),
            Some(CommittedEvent::ZoneChange { object, from: Zone::Battlefield, to, controller_before })
                if *object == proof.creature.source && *controller_before == proof.source.controller
                    && *to != Zone::Battlefield)
        && !history[proof.sacrifice_at + 1..proof.move_at].iter().any(|event| {
            matches!(event, CommittedEvent::ZoneChange { object, .. } if *object == proof.creature.source)
        })
}

pub(crate) fn record_sacrifice(
    state: &mut GameState,
    pending: &EffectContinuation,
    creature: AbilitySourceContractV4,
    sacrifice_at: usize,
    power: u32,
) -> Result<Option<usize>, String> {
    let row = &pending.resolving_item;
    let Some(source) = row
        .v4
        .ability_source_contract
        .filter(|&source| is_minsc(source))
    else {
        return Ok(None);
    };
    if row.kind != StackItemKind::ActivatedAbility
        || row.v4.activated_ability_index != Some(1)
        || row.inline_effect.as_ref() != Some(&super::minsc_minus_two())
        || pending.ctx.stack_item_id != Some(row.v4.stack_item_id)
        || pending.ctx.ability_source_contract != Some(source)
        || source.source != pending.ctx.source
        || source.controller != pending.ctx.controller
    {
        return Err("Minsc & Boo lost its actual loyalty activation".into());
    }
    let history = &state.engine.event_history;
    let Some((move_at, _)) =
        history
            .iter()
            .enumerate()
            .skip(sacrifice_at + 1)
            .find(|(_, event)| {
                matches!(event, CommittedEvent::ZoneChange { object, .. }
            if *object == creature.source)
            })
    else {
        return Ok(None);
    };
    let at = history.len();
    let proof = SacrificeV1 {
        activation: row.v4.stack_item_id,
        source,
        creature,
        sacrifice_at,
        move_at,
        power,
    };
    if !actual_sacrifice(state, proof, at) {
        return Err("Minsc & Boo lost its actual Hamster sacrifice".into());
    }
    record(state, JournalV1::MinscSacrifice(proof));
    Ok(Some(at))
}

fn sacrifice(state: &GameState, at: usize) -> Option<SacrificeV1> {
    let JournalV1::MinscSacrifice(proof) = journal(state, at)? else {
        return None;
    };
    actual_sacrifice(state, proof, at).then_some(proof)
}

fn announced(item: &StackItem) -> Option<PublicationV1> {
    let EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E7bV1(
        E7bOpV1::MinscDamageBoundV1 { sacrifice },
    ))) = item.inline_effect.as_ref()?
    else {
        return None;
    };
    let source = item.v4.ability_source_contract?;
    let [target] = item.targets.as_slice() else {
        return None;
    };
    let [contract] = item.v4.target_contracts.as_slice() else {
        return None;
    };
    (item.kind == StackItemKind::TriggeredAbility
        && item.v4.target_spec == Some(TargetSpec::AnyTarget)
        && source.source == item.source
        && source.controller == item.controller
        && item.v4.stack_item_id != StackItemId::default())
    .then_some(PublicationV1 {
        item: item.v4.stack_item_id,
        sacrifice: *sacrifice,
        source,
        target: *target,
        contract: *contract,
    })
}

pub(crate) fn publish(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    let Some(publication) = announced(item) else {
        return Ok(());
    };
    if sacrifice(state, publication.sacrifice)
        .is_none_or(|proof| proof.source != publication.source)
    {
        return Err("Minsc & Boo reflexive trigger lost its original sacrifice".into());
    }
    record(state, JournalV1::MinscTarget(publication));
    Ok(())
}

fn publication(
    state: &GameState,
    item: &StackItem,
    sacrifice: usize,
) -> Option<(usize, PublicationV1)> {
    let current = announced(item)?;
    if current.sacrifice != sacrifice {
        return None;
    }
    let mut entries = state
        .engine
        .event_history
        .scan_for_v1(&[EventKindV1::EternalOwnedDrawV1])
        .iter()
        .enumerate()
        .filter_map(|(at, event)| match event {
            CommittedEvent::EternalOwnedDrawV1 { journal } => match **journal {
                JournalV1::MinscTarget(saved) if saved.item == current.item => Some((at, saved)),
                _ => None,
            },
            _ => None,
        });
    let (at, saved) = entries.next()?;
    (entries.next().is_none() && saved == current && at > sacrifice).then_some((at, saved))
}

pub(crate) fn owns(state: &GameState, item: &StackItem, at: usize) -> bool {
    publication(state, item, at).is_some_and(|(_, row)| {
        sacrifice(state, at).is_some_and(|proof| proof.source == row.source)
    })
}

pub(crate) fn count(state: &GameState, ctx: &ExecCtx, at: usize) -> Option<u32> {
    let proof = sacrifice(state, at)?;
    (ctx.ability_source_contract == Some(proof.source)
        && ctx.controller == proof.source.controller
        && ctx.source == proof.source.source)
        .then_some(proof.power)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepartureV1 {
    pub(crate) facts: HistoricalDamageSourceV1,
    types: crate::source_type_hexproof_v1::SourceTypesV1,
}

pub(crate) fn departure(state: &GameState, object: ObjectId) -> Option<DepartureV1> {
    let live = state.objects.try_get(object)?;
    (cfg!(feature = "eternal-fixtures")
        && live.zone == Zone::Battlefield
        && live.v4.face_down_v1.is_none()
        && card_id_by_name(MINSC_AND_BOO) == Some(live.card_def)
        && state.stack.iter().any(|item| {
            item.v4.ability_source_contract.is_some_and(|source| {
                source.source == object
                    && source.zone_change_count == live.zone_change_count
                    && is_minsc(source)
            }) && (item.inline_effect.as_ref() == Some(&super::minsc_minus_two())
                || announced(item).is_some())
        }))
    .then_some(())?;
    Some(DepartureV1 {
        facts: crate::source_power_activation_v1::capture_damage_source(state, object),
        types: crate::source_type_hexproof_v1::current_types(state, object).ok()?,
    })
}

pub(crate) fn capture_batch(state: &GameState, events: &[ProposedEvent]) -> Vec<DepartureV1> {
    if !cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    events
        .iter()
        .filter_map(|event| match event {
            ProposedEvent::ZoneChange(change) => departure(state, change.object),
            _ => None,
        })
        .collect()
}

pub(crate) fn record_departure(state: &mut GameState, at: usize, departure: DepartureV1) {
    record(state, JournalV1::MinscDeparture { at, departure });
}

pub(crate) struct DamageLoanV1(HistoricalDamageSourceV1);

impl DamageLoanV1 {
    pub(crate) fn facts(self) -> HistoricalDamageSourceV1 {
        self.0
    }
}

fn departed_source(
    state: &GameState,
    source: AbilitySourceContractV4,
) -> Result<Option<DepartureV1>, String> {
    let live = state
        .objects
        .try_get(source.source)
        .ok_or("Minsc & Boo lost its physical damage source")?;
    if live.zone == Zone::Battlefield && live.zone_change_count == source.zone_change_count {
        return Ok(None);
    }
    let mut departures = state.engine.event_history
        .scan_for_v1(&[EventKindV1::EternalOwnedDrawV1]).iter().enumerate().filter_map(|(index, event)| {
            let CommittedEvent::EternalOwnedDrawV1 { journal } = event else { return None; };
            let JournalV1::MinscDeparture { at, departure } = **journal else { return None; };
            let facts = departure.facts;
            (departure.types.valid() && facts.source.source == source.source && facts.source.zone_change_count == source.zone_change_count
                && facts.source.owner == source.owner && facts.source.card_def == source.card_def
                && facts.source.zone == Zone::Battlefield && at.checked_add(1) == Some(index)
                && matches!(state.engine.event_history.get(at),
                    Some(CommittedEvent::ZoneChange { object, from: Zone::Battlefield, to, controller_before })
                        if *object == source.source && *to != Zone::Battlefield
                            && *controller_before == facts.source.controller))
                .then_some(departure)
        });
    let departure = departures
        .next()
        .ok_or("Minsc & Boo lost its original departure")?;
    if departures.next().is_some() {
        return Err("Minsc & Boo has duplicate departure evidence".into());
    }
    Ok(Some(departure))
}

/// Minsc's original native target with source characteristics from departure.
pub(crate) fn departed_target_legal(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<bool>, String> {
    let Some(row) = announced(item) else {
        return Ok(None);
    };
    let Some(departure) = departed_source(state, row.source)? else {
        return Ok(None);
    };
    let facts = departure.facts;
    let colors = facts.color_mask;
    let controller = item.controller;
    let legal =
        crate::fixed_source_damage_v3::legal_targets_with_controller(state, facts, controller)
            .contains(&row.target)
            && !crate::eternal_cards_v1::e3::veil_protects_from_colors(
                state, row.target, controller, colors,
            )
            && !crate::eternal_e5a_v1::statics::protects_from_departed_types(
                state,
                row.target,
                departure.types,
            )
            && match row.target {
                Target::Object(object) => {
                    !crate::eternal_pauper_e6b_v1::protected_from_colors(state, object, colors)
                        && !((colors & 0x1f).count_ones() > 1
                            && super::statics::has_protection_from_multicolored(state, object))
                        && !(departure
                            .types
                            .contains(crate::card_def::CardType::Creature)
                            && crate::eternal_vintage_v1::protected_from_creatures(state, object))
                }
                Target::Player(_) => true,
                Target::StackItem(_) => false,
            };
    Ok(Some(legal))
}

pub(crate) fn damage_once(
    state: &mut GameState,
    pending: &EffectContinuation,
    sacrifice: usize,
) -> Result<u32, String> {
    let (published_at, row) = publication(state, &pending.resolving_item, sacrifice)
        .ok_or("Minsc & Boo lost its native reflexive target")?;
    let power = count(state, &pending.ctx, sacrifice)
        .ok_or("Minsc & Boo lost its original Hamster power")?;
    if !crate::engine::stack_targets_still_legal(&pending.resolving_item, state)? {
        return Ok(0);
    }
    let mut proposal = event::damage_from_exec_ctx_v1(
        &pending.ctx,
        row.target,
        i32::try_from(power).map_err(|_| "Minsc & Boo damage overflow")?,
    );
    if let (ProposedEvent::Damage(damage), Some(proof)) =
        (&mut proposal, departed_source(state, row.source)?)
    {
        damage.eternal_damage_lki_v1 = Some(crate::effect::EternalDamageLkiV1::for_minsc(
            DamageLoanV1(proof.facts),
        ));
    }
    event::propose_and_commit(state, proposal);
    if state.engine.halted.is_some() || crate::linked_aura_operation_v1::has_pending(state) {
        return Err("Minsc & Boo damage did not complete".into());
    }
    record(
        state,
        JournalV1::MinscDamageCompleted {
            item: row.item,
            published_at,
        },
    );
    Ok(power)
}

pub(crate) fn completed_damage(
    state: &GameState,
    item: &StackItem,
    sacrifice: usize,
    before: usize,
) -> bool {
    let Some((published_at, row)) = publication(state, item, sacrifice) else {
        return false;
    };
    let Some(at) = before.checked_sub(1) else {
        return false;
    };
    let mut completions = state
        .engine
        .event_history
        .scan_for_v1(&[EventKindV1::EternalOwnedDrawV1])
        .iter()
        .enumerate()
        .filter_map(|(index, event)| {
            let CommittedEvent::EternalOwnedDrawV1 { journal } = event else {
                return None;
            };
            let JournalV1::MinscDamageCompleted { item, published_at } = **journal else {
                return None;
            };
            (item == row.item).then_some((index, published_at))
        });
    completions.next() == Some((at, published_at)) && completions.next().is_none()
}
