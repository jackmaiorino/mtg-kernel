//! Exact attacking-source power, sampled at resolution. No predecessor path records it.
use crate::card_def::{CardType, Keywords, CARD_DEFS};
use crate::effect::{EffectObjectBinding, EffectOp, ExecCtx};
use crate::event::{CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Target, Zone};

pub(crate) fn root() -> EffectOp {
    EffectOp::BindAshrootAttackBoostV1
}
fn owns_definition(id: u16) -> bool {
    CARD_DEFS.get(usize::from(id)).is_some_and(|d| {
        d.is_executable()
            && crate::trigger::triggers_for(id)
                .iter()
                .any(|t| (t.effect)() == root())
    })
}
pub(crate) fn owns_item(item: &crate::state::StackItem) -> bool {
    matches!(
        item.inline_effect,
        Some(EffectOp::AshrootAttackBoostV1 { .. })
    ) && item
        .v4
        .ability_source_contract
        .is_some_and(|source| owns_definition(source.card_def))
}
/// Accepted copy history changes characteristics, not the targeted incarnation.
pub(crate) fn normalize_target(
    state: &GameState,
    contract: crate::state::StackTargetContractV4,
) -> crate::state::StackTargetContractV4 {
    crate::effect::temporary_control_v1::normalize_historical_target(state, contract)
}
fn same_source(source: AbilitySourceContractV4, id: ObjectId, generation: u32) -> bool {
    source.source == id
        && source.zone == Zone::Battlefield
        && source.zone_change_count == generation
        && owns_definition(source.card_def)
}
fn needs_departure(state: &GameState, id: ObjectId) -> bool {
    let Some(live) = state.objects.try_get(id) else {
        return false;
    };
    if live.zone != Zone::Battlefield {
        return false;
    }
    owns_definition(live.card_def)
        || state.stack.iter().any(|item| {
            item.v4
                .ability_source_contract
                .is_some_and(|source| same_source(source, id, live.zone_change_count))
        })
        || state.engine.pending_effect.as_ref().is_some_and(|pending| {
            pending
                .ctx
                .ability_source_contract
                .is_some_and(|source| same_source(source, id, live.zone_change_count))
        })
        || state
            .engine
            .pending_triggers
            .iter()
            .chain(state.legend_pending_v1.iter().flatten())
            .chain(
                state
                    .pending_legend_rule_v1
                    .iter()
                    .flat_map(|p| p.waiting_triggers()),
            )
            .chain(
                state
                    .standard_v1
                    .iter()
                    .filter_map(|p| p.resolution_play.as_ref())
                    .flat_map(|p| p.deferred_triggers.iter()),
            )
            .any(|p| {
                p.source_contract
                    .is_some_and(|source| same_source(source, id, live.zone_change_count))
            })
}
/// Before any member of a simultaneous departure group changes continuous effects.
pub(crate) fn before_batch_departures(state: &mut GameState, events: &[ProposedEvent]) {
    let captures: Vec<_> = events
        .iter()
        .filter_map(|event| {
            let ProposedEvent::ZoneChange(change) = event else {
                return None;
            };
            if !needs_departure(state, change.object) {
                return None;
            }
            let live = state.objects.get(change.object);
            Some((
                ObjectLinkV4 {
                    object: change.object,
                    zone_change_count: live.zone_change_count,
                },
                live.owner,
                crate::engine::effective_power(state, change.object),
            ))
        })
        .collect();
    for (source, owner, power) in captures {
        record(state, source, owner, power);
    }
}
fn record(state: &mut GameState, source: ObjectLinkV4, owner: PlayerId, power: i32) {
    if state.engine.event_history.iter().any(|event| matches!(event,CommittedEvent::AshrootPowerBeforeDepartureV1 { source: old,.. } if *old == source)) { return; }
    state
        .engine
        .event_history
        .push(CommittedEvent::AshrootPowerBeforeDepartureV1 {
            source,
            owner,
            power,
        });
}
pub(crate) fn before_departure(state: &mut GameState, id: ObjectId) {
    if !needs_departure(state, id) {
        return;
    }
    let live = state.objects.get(id);
    let source = ObjectLinkV4 {
        object: id,
        zone_change_count: live.zone_change_count,
    };
    let owner = live.owner;
    let power = crate::engine::effective_power(state, id);
    record(state, source, owner, power);
}
pub(crate) fn validate_trigger(
    state: &GameState,
    op: &EffectOp,
    id: ObjectId,
    controller: PlayerId,
    contract: Option<AbilitySourceContractV4>,
) -> Result<(), String> {
    let EffectOp::AshrootAttackBoostV1 { source: binding } = op else {
        if contract.is_some_and(|c| owns_definition(c.card_def)) {
            return Err("Ashroot trigger changed its definition-owned root".into());
        }
        return Ok(());
    };
    let source = contract.ok_or("Ashroot attack lost original source")?;
    if !owns_definition(source.card_def)
        || source.source != id
        || source.controller != controller
        || source.zone != Zone::Battlefield
        || binding.object != source.source
        || binding.expected_zone != source.zone
        || binding.expected_zone_change_count != source.zone_change_count
    {
        return Err("Ashroot attack changed original source binding".into());
    }
    let declared = state
        .engine
        .event_history
        .iter()
        .rev()
        .find_map(|e| match e {
            CommittedEvent::DeclaredAttacker {
                source: attacker,
                source_zone_change_count,
                controller,
            } if *attacker == id && *source_zone_change_count == source.zone_change_count => {
                Some(*controller)
            }
            _ => None,
        });
    if declared != Some(controller) {
        return Err("Ashroot attack lacks original declaration controller".into());
    }
    Ok(())
}
/// Read the same battlefield incarnation, or a unique genuine departure snapshot.
pub(crate) fn sample_power(
    state: &GameState,
    source: AbilitySourceContractV4,
) -> Result<i32, String> {
    let live = state
        .objects
        .try_get(source.source)
        .ok_or("Ashroot physical source disappeared")?;
    if live.owner != source.owner {
        return Err("Ashroot physical source changed owner".into());
    }
    if live.zone == Zone::Battlefield && live.zone_change_count == source.zone_change_count {
        return Ok(crate::engine::effective_power(state, source.source).max(0));
    }
    let rows: Vec<_> = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            CommittedEvent::AshrootPowerBeforeDepartureV1 {
                source: link,
                owner,
                power,
            } if link.object == source.source
                && link.zone_change_count == source.zone_change_count =>
            {
                Some((index, *owner, *power))
            }
            _ => None,
        })
        .collect();
    let [(index, owner, power)] = rows.as_slice() else {
        return Err("Ashroot lost unique departing source power".into());
    };
    if *owner != source.owner
        || !state.engine.event_history[*index + 1..]
            .iter()
            .find_map(|e| match e {
                CommittedEvent::ZoneChange { object, from, .. } if *object == source.source => {
                    Some(*from == Zone::Battlefield)
                }
                _ => None,
            })
            .unwrap_or(false)
    {
        return Err("Ashroot power snapshot lacks actual departure".into());
    }
    Ok((*power).max(0))
}
/// History-only snapshots are authenticated at every external action boundary.
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    for item in &state.stack {
        if item.inline_effect.as_ref().is_some_and(|op| {
            matches!(
                op,
                EffectOp::AshrootAttackBoostV1 { .. } | EffectOp::BindAshrootAttackBoostV1
            )
        }) || item
            .v4
            .ability_source_contract
            .is_some_and(|source| owns_definition(source.card_def))
        {
            crate::engine::validated_stack_item_target_spec(item, state)?;
        }
    }
    for pending in state
        .engine
        .pending_triggers
        .iter()
        .chain(state.legend_pending_v1.iter().flatten())
        .chain(
            state
                .pending_legend_rule_v1
                .iter()
                .flat_map(|p| p.waiting_triggers()),
        )
        .chain(
            state
                .standard_v1
                .iter()
                .filter_map(|p| p.resolution_play.as_ref())
                .flat_map(|p| p.deferred_triggers.iter()),
        )
    {
        if matches!(
            pending.effect,
            EffectOp::AshrootAttackBoostV1 { .. } | EffectOp::BindAshrootAttackBoostV1
        ) || pending
            .source_contract
            .is_some_and(|source| owns_definition(source.card_def))
        {
            crate::engine::validate_pending_trigger_identity(state, pending)?;
        }
    }

    if state
        .engine
        .event_log
        .iter()
        .any(|e| matches!(e, CommittedEvent::AshrootPowerBeforeDepartureV1 { .. }))
    {
        return Err("Ashroot private power entered public trigger log".into());
    }
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::AshrootPowerBeforeDepartureV1 { source, owner, .. } = event else {
            continue;
        };
        let Some(live) = state.objects.try_get(source.object) else {
            return Err("Ashroot departed physical object is missing".into());
        };
        if live.owner != *owner || state.engine.event_history.iter().filter(|e| matches!(e,CommittedEvent::AshrootPowerBeforeDepartureV1 {source:other,..} if other==source)).count()!=1 {
            return Err("Ashroot departure snapshot is duplicated or changed owner".into());
        }
        let changes = state.engine.event_history[index + 1..]
            .iter()
            .filter_map(|e| match e {
                CommittedEvent::ZoneChange { object, from, .. } if *object == source.object => {
                    Some(*from)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if changes.first() != Some(&Zone::Battlefield)
            || u32::try_from(changes.len())
                .ok()
                .and_then(|count| source.zone_change_count.checked_add(count))
                != Some(live.zone_change_count)
        {
            return Err(
                "Ashroot departure snapshot lacks its exact actual zone transitions".into(),
            );
        }
    }
    Ok(())
}
pub(crate) fn execute(
    state: &mut GameState,
    ctx: &ExecCtx,
    binding: EffectObjectBinding,
) -> Result<(), String> {
    validate_trigger(
        state,
        &EffectOp::AshrootAttackBoostV1 { source: binding },
        ctx.source,
        ctx.controller,
        ctx.ability_source_contract,
    )?;
    let source = ctx
        .ability_source_contract
        .ok_or("Ashroot source missing")?;
    let [Target::Object(target)] = ctx.targets.as_slice() else {
        return Err("Ashroot lost sole creature target".into());
    };
    let [contract] = ctx.target_contracts.as_slice() else {
        return Err("Ashroot lost exact target contract".into());
    };
    let live = state
        .objects
        .try_get(*target)
        .ok_or("Ashroot target missing")?;
    if (*target == source.source && live.zone_change_count == source.zone_change_count)
        || live.zone != Zone::Battlefield
        || live.controller != ctx.controller
        || !crate::engine::object_has_type(state, *target, CardType::Creature)
    {
        return Ok(());
    }
    let crate::state::StackTargetContractV4::Object {
        object,
        zone,
        zone_change_count,
        ..
    } = contract
    else {
        return Err("Ashroot target contract is not an object".into());
    };
    if *object != *target
        || *zone != Zone::Battlefield
        || *zone_change_count != live.zone_change_count
    {
        return Ok(());
    }
    let amount = sample_power(state, source)?;
    let recipient = EffectObjectBinding {
        object: *target,
        expected_zone: Zone::Battlefield,
        expected_zone_change_count: live.zone_change_count,
    };
    crate::effect::install_temporary_boost(state, recipient, amount, amount, Keywords::TRAMPLE);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registered_live_power_is_signed_and_old_absence_never_records() {
        use crate::policy_observation_v6::tests::{put, ready_state};
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state();
            let id = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let source = AbilitySourceContractV4::capture(&state, id);
            assert_eq!(sample_power(&state, source).unwrap(), 1);
            let before = serde_json::to_vec(&state).unwrap();
            before_departure(&mut state, id);
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            crate::effect::install_temporary_boost(
                &mut state,
                EffectObjectBinding {
                    object: id,
                    expected_zone: Zone::Battlefield,
                    expected_zone_change_count: source.zone_change_count,
                },
                -4,
                0,
                Keywords::NONE,
            );
            assert_eq!(sample_power(&state, source).unwrap(), 0);
            crate::event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(id, Zone::Hand),
            );
            assert!(sample_power(&state, source).is_err());
            assert!(!state
                .engine
                .event_history
                .iter()
                .any(|e| matches!(e, CommittedEvent::AshrootPowerBeforeDepartureV1 { .. })));
        }
    }
}
