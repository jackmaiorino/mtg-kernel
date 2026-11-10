//! Last-known source facts for the physical Bombardment activation.
use super::*;
use crate::card_def::{CostComponent, PermanentFilter, TargetSpec};
use crate::state::AbilitySourceContractV4;

pub(crate) fn selected(state: &GameState, item: &StackItem) -> bool {
    if !cfg!(feature = "eternal-fixtures")
        || item.kind != StackItemKind::ActivatedAbility
        || item.v4.activated_ability_index != Some(0)
        || item.is_copy
        || item.v4.granted_by.is_some()
        || !item.v4.cauldron_grant.is_empty()
    {
        return false;
    }
    let Some(source) = item.v4.ability_source_contract else {
        return false;
    };
    source.source == item.source
        && source.zone == Zone::Battlefield
        && state.objects.try_get(item.source).is_some_and(|live| !live.v4.is_token)
        && CARD_DEFS.get(usize::from(source.card_def)).is_some_and(|def| {
            def.name == "Goblin Bombardment"
                && def.capability.is_fully_supported()
                && def.activated_abilities.first().is_some_and(|ability| {
                    ability.cost == [CostComponent::SacrificeControlled {
                        count: 1, filter: PermanentFilter::Creature,
                    }]
                        && ability.activation_zone == Zone::Battlefield
                        && ability.target_spec == TargetSpec::AnyTarget
                        && (ability.effect)() == crate::fixed_source_damage_v3::printed_root()
                })
        })
}

pub(crate) fn needs_lki(state: &GameState, item: &StackItem) -> bool {
    item.v4.ability_source_contract.is_some_and(|source| {
        state.objects.get(item.source).zone_change_count != source.zone_change_count
    }) && !matches!(item.v4.paid_cost_refs.as_slice(), [cost] if cost.object == item.source)
}

fn same_incarnation(a: AbilitySourceContractV4, b: AbilitySourceContractV4) -> bool {
    a.source == b.source
        && a.zone == b.zone
        && a.owner == b.owner
        && a.card_def == b.card_def
        && a.zone_change_count == b.zone_change_count
}

fn zone_change_into(
    state: &GameState,
    object: ObjectId,
    generation: u32,
) -> Option<(usize, &CommittedEvent)> {
    let later = state.objects.try_get(object)?.zone_change_count.checked_sub(generation)?;
    state.engine.event_history.scan_for_v1(&[EventKindV1::ZoneChange])
        .iter().enumerate().rev()
        .filter(|(_, event)| matches!(event, CommittedEvent::ZoneChange { object: id, .. }
            if *id == object))
        .nth(usize::try_from(later).ok()?)
}

fn validate_payment(state: &GameState, item: &StackItem) -> Result<(), String> {
    let [cost] = item.v4.paid_cost_refs.as_slice() else {
        return Err("Bombardment lost its native creature payment".into());
    };
    let Some((moved_at, CommittedEvent::ZoneChange { from: Zone::Battlefield, to, controller_before, .. })) =
        zone_change_into(state, cost.object, cost.zone_change_count)
    else {
        return Err("Bombardment creature payment lost its actual move".into());
    };
    if cost.zone_change_count == 0 || cost.zone != *to || *controller_before != item.controller
        || state.objects.get(cost.object).owner != cost.owner
        || item.v4.target_spec != Some(TargetSpec::AnyTarget)
        || item.targets.len() != 1
        || item.v4.target_contracts.len() != 1
        || item.inline_effect.as_ref() != Some(&crate::fixed_source_damage_v3::printed_root())
        || !state.engine.event_history[..moved_at].iter().rev()
            .take_while(|event| !matches!(event, CommittedEvent::ZoneChange { object, .. }
                if *object == cost.object))
            .any(|event| matches!(event, CommittedEvent::Sacrificed { object, controller_before, .. }
                if *object == cost.object && *controller_before == item.controller))
    {
        return Err("Bombardment source, target or sacrifice proof changed".into());
    }
    Ok(())
}

fn captured(state: &GameState, source: AbilitySourceContractV4) -> impl Iterator<Item = (usize, HistoricalDamageSourceV1)> + '_ {
    state.engine.event_history.scan_for_v1(&[EventKindV1::SourceDamageBeforeLeavingV1])
        .iter().enumerate().filter_map(move |(index, event)| match event {
            CommittedEvent::SourceDamageBeforeLeavingV1 { damage_source }
                if same_incarnation(damage_source.source, source) => Some((index, *damage_source)),
            _ => None,
        })
}

pub(crate) fn departed_source(state: &GameState, item: &StackItem) -> Result<Option<HistoricalDamageSourceV1>, String> {
    let source = item.v4.ability_source_contract.ok_or("Bombardment lost its source")?;
    crate::engine::validate_historical_ability_source_contract(state, source)?;
    if state.objects.get(source.source).zone_change_count == source.zone_change_count {
        return Ok(None);
    }
    validate_payment(state, item)?;
    if !needs_lki(state, item) {
        // This lane is for a waiting enchantment that later leaves. An
        // animated Bombardment sacrificed as its own cost is a separate path.
        return Ok(None);
    }
    let generation = source.zone_change_count.checked_add(1).ok_or("Bombardment generation overflow")?;
    let Some((left_at, CommittedEvent::ZoneChange { from: Zone::Battlefield, controller_before, .. })) =
        zone_change_into(state, source.source, generation)
    else {
        return Err("Bombardment lost its original native battlefield departure".into());
    };
    let mut captures = captured(state, source);
    let Some((capture_at, facts)) = captures.next() else {
        return Err("departed Bombardment has no native last-known source receipt".into());
    };
    let entry_at = if source.zone_change_count == 0 { None } else {
        zone_change_into(state, source.source, source.zone_change_count).map(|(index, _)| index)
    };
    if captures.next().is_some() || capture_at >= left_at
        || entry_at.is_some_and(|entry| entry >= capture_at)
        || facts.source.controller != *controller_before
    {
        return Err("Bombardment last-known source differs from its actual departure".into());
    }
    Ok(Some(facts))
}

pub(crate) fn before_departure(state: &mut GameState, object: ObjectId) {
    if !cfg!(feature = "eternal-fixtures") || state.stack.is_empty() {
        return;
    }
    let source = AbilitySourceContractV4::capture(state, object);
    if source.zone != Zone::Battlefield {
        return;
    }
    if state.stack.iter().any(|item| selected(state, item)
        && item.v4.ability_source_contract.is_some_and(|saved| same_incarnation(saved, source)))
        && captured(state, source).next().is_none()
    {
        let damage_source = capture_damage_source(state, object);
        state.engine.event_history.push(CommittedEvent::SourceDamageBeforeLeavingV1 { damage_source });
    }
}

pub(crate) fn before_batch_departures(state: &mut GameState, events: &[ProposedEvent]) {
    for event in events {
        if let ProposedEvent::ZoneChange(change) = event {
            if change.to_zone != Zone::Battlefield {
                before_departure(state, change.object);
            }
        }
    }
}
