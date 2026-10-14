//! Paid once-only activation authority for the selected printed Librarian root.
use crate::card_def::{TargetSpec, CARD_DEFS};
use crate::event::CommittedEvent;
use crate::ids::StackItemId;
use crate::state::{GameState, StackItem, StackItemKind, Zone};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum OperationV1 {
    Resolved {
        draw_history_index: u64,
        counter_history_index: Option<u64>,
    },
    Countered {
        removal_history_index: u64,
    },
}

fn definition_owns(item: &StackItem) -> bool {
    item.kind == StackItemKind::ActivatedAbility
        && item.v4.activated_ability_index == Some(0)
        && item.v4.ability_source_contract.is_some_and(|source| {
            CARD_DEFS
                .get(usize::from(source.card_def))
                .is_some_and(|def| {
                    def.is_executable()
                        && def.name == "Mild-Mannered Librarian"
                        && def.activated_abilities.len() == 1
                        && (def.activated_abilities[0].effect)()
                            == crate::mild_mannered_librarian_v1::program_v1()
                })
        })
}

fn shape(item: &StackItem) -> Result<(), String> {
    let source = item
        .v4
        .ability_source_contract
        .ok_or("Librarian source lost")?;
    if !definition_owns(item)
        || item.source != source.source
        || item.controller != source.controller
        || source.zone != Zone::Battlefield
        || item.inline_effect != Some(crate::mild_mannered_librarian_v1::program_v1())
        || item.v4.stack_item_id == StackItemId::default()
        || item.is_copy
        || !item.targets.is_empty()
        || !item.v4.target_contracts.is_empty()
        || item.v4.target_spec != Some(TargetSpec::None)
        || item.v4.granted_by.is_some()
        || item.v4.cauldron_grant.0.is_some()
        || !item.v4.paid_cost_refs.is_empty()
    {
        return Err("Librarian activation changed its printed paid root".into());
    }
    Ok(())
}

/// Called only after genuine full payment and finalized stack publication.
pub(crate) fn record_paid(state: &mut GameState, id: StackItemId) {
    let Some(item) = state.stack.iter().find(|item| item.v4.stack_item_id == id) else {
        return;
    };
    if definition_owns(item) {
        state
            .engine
            .event_history
            .push(CommittedEvent::LibrarianActivationCommittedV1 {
                item: Box::new(item.clone()),
            });
    }
}

/// Private pre-removal ownership. External snapshots cannot construct this
/// token; only the authenticated exact live counter primitive acquires it.
pub(crate) struct CounterRemovalOwnerV1 {
    item: StackItem,
}
pub(crate) fn acquire_counter_owner(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<CounterRemovalOwnerV1>, String> {
    if !definition_owns(item) {
        return Ok(None);
    }
    validate_item(state, item)?;
    let live: Vec<_> = state
        .stack
        .iter()
        .filter(|live| live.v4.stack_item_id == item.v4.stack_item_id)
        .collect();
    if live.as_slice() != [item] {
        return Err("Librarian counter lost exact live ownership".into());
    }
    Ok(Some(CounterRemovalOwnerV1 { item: item.clone() }))
}

pub(crate) fn record_countered(
    state: &mut GameState,
    item: &StackItem,
    owner: Option<CounterRemovalOwnerV1>,
) -> Result<(), String> {
    if !definition_owns(item) {
        return Ok(());
    }
    let owner = owner.ok_or("Librarian counter lost private pre-removal ownership")?;
    if owner.item != *item
        || state
            .stack
            .iter()
            .any(|live| live.v4.stack_item_id == item.v4.stack_item_id)
    {
        return Err("Librarian counter did not remove its exact owned row".into());
    }
    let removal_history_index = u64::try_from(state.engine.event_history.len())
        .map_err(|_| "removal occurrence exceeds u64")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::LibrarianActivationCounterRemovalV1 {
            item: Box::new(owner.item),
        });
    state
        .engine
        .event_history
        .push(CommittedEvent::LibrarianActivationOperationV1 {
            item: Box::new(item.clone()),
            operation: OperationV1::Countered {
                removal_history_index,
            },
        });
    Ok(())
}

/// Called only after the exact atomic printed program executes. The ordinary
/// draw and counter journals independently bind the actual effects.
pub(crate) fn record_resolved(
    state: &mut GameState,
    item: &StackItem,
    start: usize,
) -> Result<(), String> {
    if !definition_owns(item) {
        return Ok(());
    }
    if state.engine.halted.is_some() {
        return Err("Librarian execution halted".into());
    }
    let source = item
        .v4
        .ability_source_contract
        .ok_or("Librarian source lost")?;
    let rows = &state.engine.event_history[start..];
    let draws: Vec<_> = rows
        .iter()
        .enumerate()
        .filter_map(|(offset, event)| {
            matches!(event, CommittedEvent::Draw { player, .. } if *player == source.controller)
                .then_some(start + offset)
        })
        .collect();
    let [draw] = draws.as_slice() else {
        return Err("Librarian did not commit exactly one draw attempt".into());
    };
    let counters: Vec<_> = rows.iter().enumerate().filter_map(|(offset, event)| {
        matches!(event, CommittedEvent::PlusOneCountersAdded { object, zone_change_count, player, count }
            if *object == source.source && *zone_change_count == source.zone_change_count
                && *player == source.controller && *count == 2).then_some(start + offset)
    }).collect();
    let surviving = state.objects.try_get(source.source).filter(|live| {
        live.zone == Zone::Battlefield && live.zone_change_count == source.zone_change_count
    });
    let counter = match (surviving, counters.as_slice()) {
        (Some(live), [counter])
            if live.v4.creature_upgrade.as_ref().is_some_and(|upgrade| {
                upgrade.creature_types.as_ref().is_some_and(|(types, _)| {
                    types.as_slice() == [crate::card_def::Subtype::Werewolf.stable_id()]
                })
            }) =>
        {
            Some(u64::try_from(*counter).map_err(|_| "counter occurrence exceeds u64")?)
        }
        (None, []) => None,
        _ => {
            return Err("Librarian did not commit its surviving source upgrade and counters".into())
        }
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::LibrarianActivationOperationV1 {
            item: Box::new(item.clone()),
            operation: OperationV1::Resolved {
                draw_history_index: u64::try_from(*draw)
                    .map_err(|_| "draw occurrence exceeds u64")?,
                counter_history_index: counter,
            },
        });
    Ok(())
}

pub(crate) fn finish(state: &mut GameState, item: &StackItem) {
    if !definition_owns(item) {
        return;
    }
    let operations: Vec<_> = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(index, event)| {
            matches!(event, CommittedEvent::LibrarianActivationOperationV1 { item: saved, .. }
            if saved.as_ref() == item)
            .then_some(index)
        })
        .collect();
    let [operation] = operations.as_slice() else {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            item.source,
        ));
        return;
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::LibrarianActivationFinishedV1 {
            stack_item_id: item.v4.stack_item_id,
            operation_history_index: u64::try_from(*operation).expect("bounded history"),
        });
}

pub(crate) fn validate_item(state: &GameState, item: &StackItem) -> Result<(), String> {
    let rows: Vec<_> = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::LibrarianActivationCommittedV1 { item: saved }
                if saved.v4.stack_item_id == item.v4.stack_item_id =>
            {
                Some(saved.as_ref())
            }
            _ => None,
        })
        .collect();
    if !definition_owns(item)
        && item.inline_effect != Some(crate::mild_mannered_librarian_v1::program_v1())
        && rows.is_empty()
    {
        return Ok(());
    }
    shape(item)?;
    if rows.as_slice() != [item]
        || state.engine.event_history.iter().any(|event| {
            matches!(event,
            CommittedEvent::LibrarianActivationFinishedV1 { stack_item_id, .. }
                if *stack_item_id == item.v4.stack_item_id)
        })
    {
        return Err("Librarian activation lost payment or replayed completion".into());
    }
    Ok(())
}

/// Read-only common action/restore boundary. Existing upgrades remain the sole
/// gameplay storage; the immutable payment row authenticates their once mark.
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !*ENABLED.get_or_init(|| {
        cfg!(all(
            feature = "limited-fdn-fixtures",
            not(feature = "standard-magezero-fixtures")
        )) && CARD_DEFS
            .iter()
            .any(|def| def.is_executable() && def.name == "Mild-Mannered Librarian")
    }) {
        return Ok(());
    }
    for item in &state.stack {
        validate_item(state, item)?;
    }
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::LibrarianActivationCommittedV1 { .. }
                | CommittedEvent::LibrarianActivationFinishedV1 { .. }
                | CommittedEvent::LibrarianActivationOperationV1 { .. }
                | CommittedEvent::LibrarianActivationCounterRemovalV1 { .. }
        )
    }) {
        return Err("Librarian paid authority entered transient trigger log".into());
    }
    let mut ids = Vec::new();
    let mut sources = Vec::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::LibrarianActivationCommittedV1 { item } = event else {
            continue;
        };
        shape(item)?;
        let source = item.v4.ability_source_contract.unwrap();
        let key = (source.source, source.zone_change_count);
        if ids.contains(&item.v4.stack_item_id) || sources.contains(&key) {
            return Err("Librarian activated twice in the same incarnation".into());
        }
        ids.push(item.v4.stack_item_id);
        sources.push(key);
        let finishes: Vec<_> = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(at, event)| {
                matches!(event, CommittedEvent::LibrarianActivationFinishedV1 { stack_item_id, .. }
                if *stack_item_id == item.v4.stack_item_id)
                .then_some(at)
            })
            .collect();
        let live: Vec<_> = state
            .stack
            .iter()
            .filter(|live| live.v4.stack_item_id == item.v4.stack_item_id)
            .collect();
        let operations: Vec<_> = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(at, event)| match event {
                CommittedEvent::LibrarianActivationOperationV1 {
                    item: original,
                    operation,
                } if original.v4.stack_item_id == item.v4.stack_item_id => {
                    Some((at, original.as_ref(), operation))
                }
                _ => None,
            })
            .collect();
        let removals: Vec<_> = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(at, event)| match event {
                CommittedEvent::LibrarianActivationCounterRemovalV1 { item: original }
                    if original.v4.stack_item_id == item.v4.stack_item_id =>
                {
                    Some((at, original.as_ref()))
                }
                _ => None,
            })
            .collect();
        match (finishes.as_slice(), live.as_slice(), operations.as_slice()) {
            ([], [live], []) if **live == **item && removals.is_empty() => {}
            ([finished], [], [(operation_index, original, operation)])
                if index < *operation_index
                    && *operation_index < *finished
                    && *original == item.as_ref() =>
            {
                let Some(CommittedEvent::LibrarianActivationFinishedV1 {
                    operation_history_index,
                    ..
                }) = state.engine.event_history.get(*finished)
                else {
                    unreachable!()
                };
                if usize::try_from(*operation_history_index).ok() != Some(*operation_index) {
                    return Err("Librarian completion redirected its original operation".into());
                }
                match operation {
                    OperationV1::Countered {
                        removal_history_index,
                    } => {
                        let [(removed_at, removed)] = removals.as_slice() else {
                            return Err(
                                "Librarian counter lost its independent actual removal".into()
                            );
                        };
                        if *removed != item.as_ref()
                            || *removed_at <= index
                            || *removed_at >= *operation_index
                            || usize::try_from(*removal_history_index).ok() != Some(*removed_at)
                        {
                            return Err("Librarian counter redirected its original removal".into());
                        }
                    }
                    OperationV1::Resolved { .. } if !removals.is_empty() => {
                        return Err(
                            "Librarian resolution unexpectedly carries counter removal".into()
                        );
                    }
                    OperationV1::Resolved { .. } => {}
                }
                if let OperationV1::Resolved {
                    draw_history_index,
                    counter_history_index,
                } = operation
                {
                    let draw = usize::try_from(*draw_history_index)
                        .map_err(|_| "draw occurrence exceeds usize")?;
                    if draw <= index
                        || draw >= *operation_index
                        || !matches!(state.engine.event_history.get(draw),
                        Some(CommittedEvent::Draw { player, .. }) if *player == source.controller)
                    {
                        return Err("Librarian completion lost its actual draw attempt".into());
                    }
                    if let Some(counter) = counter_history_index {
                        let counter = usize::try_from(*counter).map_err(|_| "counter occurrence exceeds usize")?;
                        if counter <= index || counter >= draw || !matches!(state.engine.event_history.get(counter),
                            Some(CommittedEvent::PlusOneCountersAdded { object, zone_change_count, player, count })
                            if *object == source.source && *zone_change_count == source.zone_change_count
                                && *player == source.controller && *count == 2) {
                            return Err("Librarian completion lost its actual source counter placement".into());
                        }
                    } else if !state.engine.event_history[index+1..draw].iter().any(|event|
                        matches!(event, CommittedEvent::ZoneChange { object, from: Zone::Battlefield, .. }
                            if *object == source.source)) {
                        return Err("Librarian source-free completion lost its actual departure".into());
                    }
                }
            }
            _ => return Err("Librarian paid activation ownership was erased or replayed".into()),
        }
        if let Some(live) = state.objects.try_get(source.source).filter(|live| {
            live.zone == Zone::Battlefield && live.zone_change_count == source.zone_change_count
        }) {
            if live.owner != source.owner
                || live.v4.creature_upgrade.as_ref().is_none_or(|upgrade| {
                    upgrade
                        .once_activated
                        .iter()
                        .filter(|index| **index == 0)
                        .count()
                        != 1
                })
            {
                return Err("Librarian original incarnation lost its paid once mark".into());
            }
        }
    }
    if state.engine.event_history.iter().any(|event| match event {
        CommittedEvent::LibrarianActivationFinishedV1 { stack_item_id, .. } => {
            !ids.contains(stack_item_id)
        }
        CommittedEvent::LibrarianActivationOperationV1 { item, .. }
        | CommittedEvent::LibrarianActivationCounterRemovalV1 { item } => {
            !ids.contains(&item.v4.stack_item_id)
        }
        _ => false,
    }) {
        return Err("Librarian completion has no genuine payment".into());
    }
    for (id, object) in state.objects.iter() {
        if object.zone == Zone::Battlefield
            && crate::mild_mannered_librarian_v1::is_librarian(state, id, 0)
            && object
                .v4
                .creature_upgrade
                .as_ref()
                .is_some_and(|upgrade| upgrade.once_activated.contains(&0))
            && !sources.contains(&(id, object.zone_change_count))
        {
            return Err("Librarian once mark lost its original payment".into());
        }
    }
    Ok(())
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests {
    use super::*;
    use crate::policy_observation_v6::tests::{put, ready_state};
    #[test]
    fn genuine_counter_primitive_closes_paid_ability_once_without_upgrading_or_drawing_both_seats()
    {
        for actor in [crate::ids::PlayerId::P0, crate::ids::PlayerId::P1] {
            let mut state = ready_state();
            state.active_player = actor;
            state.priority_player = actor;
            let source = put(
                &mut state,
                actor,
                "Mild-Mannered Librarian",
                Zone::Battlefield,
            );
            state.players[actor.index()].mana_pool[crate::mana::ManaColor::G.pool_index()] = 4;
            assert!(matches!(crate::engine::advance_until_decision(&mut state),
                crate::engine::Decision::CastSpellOrPass { activatable_abilities, .. }
                if activatable_abilities.contains(&(source, 0))));
            crate::engine::step(
                &mut state,
                crate::engine::Action::ActivateAbility(source, 0),
            )
            .unwrap();
            crate::engine::advance_until_decision(&mut state);
            let original = state
                .stack
                .iter()
                .find(|item| item.source == source)
                .unwrap()
                .clone();
            let hand = state.players[actor.index()].hand.len();
            assert_eq!(
                crate::engine::counter_stack_item_by_id(&mut state, original.v4.stack_item_id)
                    .unwrap(),
                Some(original.clone())
            );
            validate_state(&state).unwrap();
            assert!(
                crate::engine::counter_stack_item_by_id(&mut state, original.v4.stack_item_id)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                state
                    .engine
                    .event_history
                    .iter()
                    .filter(|event| matches!(event,
                CommittedEvent::LibrarianActivationFinishedV1 { stack_item_id, .. }
                    if *stack_item_id == original.v4.stack_item_id))
                    .count(),
                1
            );
            assert_eq!(state.objects.get(source).counters.plus1_plus1, 0);
            assert_eq!(state.players[actor.index()].hand.len(), hand);
            assert!(!crate::mild_mannered_librarian_v1::activation_allowed(
                &state, source, 0
            ));
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            validate_state(&restored).unwrap();
            for alteration in 0..2 {
                let mut forged = restored.clone();
                if alteration == 0 {
                    forged.engine.event_history.retain(|event| {
                        !matches!(
                            event,
                            CommittedEvent::LibrarianActivationCounterRemovalV1 { .. }
                        )
                    });
                } else {
                    let removal = forged
                        .engine
                        .event_history
                        .iter_mut()
                        .find(|event| {
                            matches!(
                                event,
                                CommittedEvent::LibrarianActivationCounterRemovalV1 { .. }
                            )
                        })
                        .unwrap();
                    let CommittedEvent::LibrarianActivationCounterRemovalV1 { item } = removal
                    else {
                        unreachable!()
                    };
                    item.controller = actor.opponent();
                }
                let bytes = serde_json::to_vec(&forged).unwrap();
                assert!(crate::engine::step(&mut forged, crate::engine::Action::Pass).is_err());
                assert_eq!(serde_json::to_vec(&forged).unwrap(), bytes);
            }
        }
    }
}
