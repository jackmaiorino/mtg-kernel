//! Last-known source facts for the physical Bombardment activation.
use super::*;
use crate::card_def::{CostComponent, PermanentFilter, TargetSpec};
use crate::state::{AbilitySourceContractV4, BombardmentLkiV1, BombardmentPaidV1};

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
        && state
            .objects
            .try_get(item.source)
            .is_some_and(|live| !live.v4.is_token)
        && CARD_DEFS
            .get(usize::from(source.card_def))
            .is_some_and(|def| {
                def.name == "Goblin Bombardment"
                    && def.capability.is_fully_supported()
                    && def.activated_abilities.first().is_some_and(|ability| {
                        ability.cost
                            == [CostComponent::SacrificeControlled {
                                count: 1,
                                filter: PermanentFilter::Creature,
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
            && (!item.v4.bombardment_lki_v1.is_empty() || captured(state, source).next().is_some())
    })
}

pub(crate) fn mark_paid(
    state: &mut GameState,
    id: StackItemId,
    start: usize,
) -> Result<(), String> {
    if !cfg!(feature = "eternal-fixtures") {
        return Ok(());
    }
    let Some(item) = state
        .stack
        .last()
        .filter(|item| item.v4.stack_item_id == id)
    else {
        return Err("Bombardment payment lost the actual new stack row".into());
    };
    if !selected(state, item) {
        return Ok(());
    }
    let [cost] = item.v4.paid_cost_refs.as_slice() else {
        return Err("Bombardment lost its native creature payment".into());
    };
    if cost.object == item.source {
        return Ok(());
    }
    let mut payment = state.engine.event_history.iter().enumerate().skip(start);
    let (sacrifice_at, _) = payment
        .find(|(_, event)| {
            matches!(event, CommittedEvent::Sacrificed { object, controller_before, .. }
            if *object == cost.object && *controller_before == item.controller)
        })
        .ok_or("Bombardment native payment lost its sacrifice")?;
    let (move_at, _) = payment.find(|(_, event)| {
        matches!(event, CommittedEvent::ZoneChange { object, from: Zone::Battlefield, to, controller_before }
            if *object == cost.object && *to == cost.zone && *controller_before == item.controller)
    }).ok_or("Bombardment native payment lost its zone change")?;
    let paid = BombardmentPaidV1 {
        sacrifice_at: u32::try_from(sacrifice_at)
            .map_err(|_| "Bombardment sacrifice index overflow")?,
        move_at: u32::try_from(move_at).map_err(|_| "Bombardment move index overflow")?,
        cost: *cost,
    };
    state
        .stack
        .last_mut()
        .expect("actual paid stack row")
        .v4
        .bombardment_lki_v1 = BombardmentLkiV1(Some(paid));
    Ok(())
}

fn same_incarnation(a: AbilitySourceContractV4, b: AbilitySourceContractV4) -> bool {
    a.source == b.source
        && a.zone == b.zone
        && a.owner == b.owner
        && a.card_def == b.card_def
        && a.zone_change_count == b.zone_change_count
}

fn validate_payment(
    state: &GameState,
    item: &StackItem,
    paid: BombardmentPaidV1,
) -> Result<(), String> {
    let history = &state.engine.event_history;
    let cost = paid.cost;
    if item.v4.paid_cost_refs.as_slice() != [cost]
        || paid.sacrifice_at >= paid.move_at
        || cost.object == item.source
        || cost.zone_change_count == 0
        || state.objects.try_get(cost.object).is_none_or(|live| live.owner != cost.owner)
        || item.v4.target_spec != Some(TargetSpec::AnyTarget)
        || item.targets.len() != 1
        || item.v4.target_contracts.len() != 1
        || item.inline_effect.as_ref() != Some(&crate::fixed_source_damage_v3::printed_root())
        || !matches!(history.get(paid.sacrifice_at as usize),
            Some(CommittedEvent::Sacrificed { object, controller_before, .. })
                if *object == cost.object && *controller_before == item.controller)
        || !matches!(history.get(paid.move_at as usize),
            Some(CommittedEvent::ZoneChange { object, from: Zone::Battlefield, to, controller_before })
                if *object == cost.object && *to == cost.zone && *controller_before == item.controller)
        || history[paid.sacrifice_at as usize + 1..paid.move_at as usize].iter().any(|event| {
            matches!(event, CommittedEvent::ZoneChange { object, .. } if *object == cost.object)
        })
    {
        return Err("Bombardment source, target or sacrifice proof changed".into());
    }
    Ok(())
}

fn captured(
    state: &GameState,
    source: AbilitySourceContractV4,
) -> impl Iterator<Item = (usize, HistoricalDamageSourceV1)> + '_ {
    state
        .engine
        .event_history
        .scan_for_v1(&[EventKindV1::SourceDamageBeforeLeavingV1])
        .iter()
        .enumerate()
        .filter_map(move |(index, event)| match event {
            CommittedEvent::SourceDamageBeforeLeavingV1 { damage_source }
                if same_incarnation(damage_source.source, source) =>
            {
                Some((index, *damage_source))
            }
            _ => None,
        })
}

pub(crate) fn departed_source(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<HistoricalDamageSourceV1>, String> {
    let source = item
        .v4
        .ability_source_contract
        .ok_or("Bombardment lost its source")?;
    crate::engine::validate_historical_ability_source_contract(state, source)?;
    let paid = item.v4.bombardment_lki_v1.0;
    if let Some(paid) = paid {
        validate_payment(state, item, paid)?;
    }
    if !needs_lki(state, item) {
        return Ok(None);
    }
    let mut captures = captured(state, source);
    let Some((capture_at, facts)) = captures.next() else {
        return Err("departed Bombardment has no native last-known source receipt".into());
    };
    let history = &state.engine.event_history;
    let moved_source = |event: &CommittedEvent| matches!(event, CommittedEvent::ZoneChange { object, .. } if *object == source.source);
    if captures.next().is_some()
        || !matches!(history[capture_at + 1..].iter().find(|event| moved_source(event)),
            Some(CommittedEvent::ZoneChange { from: Zone::Battlefield, to, controller_before, .. })
                if *to != Zone::Battlefield && *controller_before == facts.source.controller)
        || paid.is_some_and(|paid| {
            let moved_at = paid.move_at as usize;
            moved_at >= capture_at || history[moved_at + 1..capture_at].iter().any(moved_source)
        })
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
    if state.stack.iter().any(|item| {
        !item.v4.bombardment_lki_v1.is_empty()
            && selected(state, item)
            && item
                .v4
                .ability_source_contract
                .is_some_and(|saved| same_incarnation(saved, source))
    }) && captured(state, source).next().is_none()
    {
        let damage_source = capture_damage_source(state, object);
        state
            .engine
            .event_history
            .push(CommittedEvent::SourceDamageBeforeLeavingV1 { damage_source });
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
