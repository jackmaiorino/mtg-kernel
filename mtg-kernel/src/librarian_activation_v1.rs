//! Paid once-only activation authority for the selected printed Librarian root.
use crate::card_def::{TargetSpec, CARD_DEFS};
use crate::event::CommittedEvent;
use crate::ids::StackItemId;
use crate::state::{GameState, StackItem, StackItemKind, Zone};

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

pub(crate) fn finish(state: &mut GameState, item: &StackItem) {
    if definition_owns(item) {
        state
            .engine
            .event_history
            .push(CommittedEvent::LibrarianActivationFinishedV1 {
                stack_item_id: item.v4.stack_item_id,
            });
    }
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
            CommittedEvent::LibrarianActivationFinishedV1 { stack_item_id }
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
                matches!(event, CommittedEvent::LibrarianActivationFinishedV1 { stack_item_id }
                if *stack_item_id == item.v4.stack_item_id)
                .then_some(at)
            })
            .collect();
        let live: Vec<_> = state
            .stack
            .iter()
            .filter(|live| live.v4.stack_item_id == item.v4.stack_item_id)
            .collect();
        match (finishes.as_slice(), live.as_slice()) {
            ([], [live]) if **live == **item => {}
            ([finished], []) if *finished > index => {}
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
    if state.engine.event_history.iter().any(|event| matches!(event,
        CommittedEvent::LibrarianActivationFinishedV1 { stack_item_id } if !ids.contains(stack_item_id)))
    {
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
