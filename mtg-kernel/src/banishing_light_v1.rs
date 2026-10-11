//! Selected Banishing Light entry and linked-exile authority.
use crate::card_def::{TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, ObjectRef};
use crate::event::CommittedEvent;
use crate::ids::ObjectId;
use crate::state::{
    AbilitySourceContractV4, GameState, LinkedExileRecordV4, StackItem, StackItemKind, Zone,
};
use crate::trigger::PendingTrigger;
use serde::{Deserialize, Serialize};

pub fn recipe() -> EffectOp {
    EffectOp::ExileTargetLinkedToSource {
        object: ObjectRef::Target(0),
    }
}
pub(crate) fn owns_definition(id: u16) -> bool {
    CARD_DEFS.get(usize::from(id)).is_some_and(|def| {
        def.name == "Banishing Light"
            && def.is_executable()
            && crate::trigger::triggers_for(id).iter().any(|trigger| {
                trigger.condition == crate::trigger::TriggerCondition::Etb
                    && (trigger.effect)() == recipe()
            })
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RemovalV1 {
    Resolving {
        source_present: bool,
        target_legal: bool,
    },
    Countered,
}

fn pending_owners(state: &GameState) -> impl Iterator<Item = &PendingTrigger> {
    state
        .engine
        .pending_triggers
        .iter()
        .chain(state.legend_pending_v1.iter().flatten())
        .chain(
            state
                .pending_legend_rule_v1
                .iter()
                .flat_map(|pending| pending.waiting_triggers()),
        )
        .chain(
            state
                .standard_v1
                .iter()
                .filter_map(|standard| standard.resolution_play.as_ref())
                .flat_map(|resolution| resolution.deferred_triggers.iter()),
        )
}

/// Called immediately after the genuine entry has been committed.
pub(crate) fn record_entry(state: &mut GameState, entry: usize) {
    let Some(id) = state
        .engine
        .event_history
        .get(entry)
        .and_then(crate::trigger::battlefield_entry_object)
    else {
        return;
    };
    let object = state.objects.get(id);
    if !owns_definition(object.card_def)
        || object.zone != Zone::Battlefield
        || object.v4.face_index != 0
        || !crate::continuous_characteristics_v1::printed_abilities_active(state, id)
    {
        return;
    }
    let source = AbilitySourceContractV4::capture(state, id);
    state
        .engine
        .event_history
        .push(CommittedEvent::BanishingEntryV1 {
            source,
            entry_history_index: u64::try_from(entry).expect("bounded entry history"),
        });
}

pub(crate) fn validate_source(
    state: &GameState,
    source: AbilitySourceContractV4,
) -> Result<usize, String> {
    let rows: Vec<_> = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, event)| {
            if let CommittedEvent::BanishingEntryV1 {
                source: original,
                entry_history_index,
            } = event
            {
                (original.source == source.source
                    && original.zone_change_count == source.zone_change_count)
                    .then_some((at, *original, *entry_history_index))
            } else {
                None
            }
        })
        .collect();
    let [(at, original, entry)] = rows.as_slice() else {
        return Err("Banishing source lost unique original entry".into());
    };
    let entry = usize::try_from(*entry).map_err(|_| "entry history exceeds usize")?;
    if *original != source
        || source.zone != Zone::Battlefield
        || source.attached_to.is_some()
        || !owns_definition(source.card_def)
        || entry >= *at
        || state.engine.event_history.get(entry).is_none_or(|event| {
            crate::trigger::battlefield_entry_object(event) != Some(source.source)
        })
        || matches!(state.engine.event_history.get(entry),Some(CommittedEvent::CreateToken {token_def,controller,..})
            if *token_def!=source.card_def || *controller!=source.controller || *controller!=source.owner)
    {
        return Err(
            "Banishing source/controller differs from independently committed entry".into(),
        );
    }
    Ok(*at)
}

pub(crate) fn collect(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Result<Vec<PendingTrigger>, ObjectId> {
    let entries: Vec<_> = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| {
            if let CommittedEvent::BanishingEntryV1 {
                source,
                entry_history_index,
            } = event
            {
                Some((*source, *entry_history_index))
            } else {
                None
            }
        })
        .collect();
    let mut result = Vec::new();
    for (source, entry) in entries {
        let collected=state.engine.event_history.iter().filter(|event|
            matches!(event,CommittedEvent::BanishingCollectedV1 {source:captured} if *captured==source)).count();
        if collected != 0 {
            continue;
        }
        let actual = usize::try_from(entry)
            .ok()
            .and_then(|at| state.engine.event_history.get(at));
        if actual.is_none_or(|actual| !events.contains(actual)) {
            continue;
        }
        validate_source(state, source).map_err(|_| source.source)?;
        state
            .engine
            .event_history
            .push(CommittedEvent::BanishingCollectedV1 { source });
        result.push(PendingTrigger {
            source: source.source,
            controller: source.controller,
            effect: recipe(),
            target_spec: TargetSpec::OpponentNonlandPermanent,
            targets: Vec::new(),
            target_contracts: Vec::new(),
            is_madness_offer: false,
            kicked: false,
            placement_ordered: false,
            source_contract: Some(source),
            granted_by: None,
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        });
    }
    Ok(result)
}

fn source_of(item: &StackItem) -> Option<AbilitySourceContractV4> {
    item.v4.ability_source_contract.filter(|source| {
        owns_definition(source.card_def)
            && (item.inline_effect.as_ref() == Some(&recipe())
                || item.v4.target_spec == Some(TargetSpec::OpponentNonlandPermanent))
    })
}
fn exact_item(item: &StackItem, source: AbilitySourceContractV4) -> bool {
    item.kind == StackItemKind::TriggeredAbility
        && item.source == source.source
        && item.controller == source.controller
        && item.inline_effect.as_ref() == Some(&recipe())
        && item.v4.ability_source_contract == Some(source)
        && item.v4.granted_by.is_none()
        && item.v4.source_contract.is_none()
        && item.v4.target_spec == Some(TargetSpec::OpponentNonlandPermanent)
        && item.targets.len() == 1
        && item.v4.target_contracts.len() == 1
        && !item.is_copy
}
pub(crate) fn publish(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    let Some(source) = source_of(item) else {
        return Ok(());
    };
    validate_source(state, source)?;
    if !exact_item(item, source) {
        return Err("Banishing published row changed its printed root".into());
    }
    state
        .engine
        .event_history
        .push(CommittedEvent::BanishingPublishedV1 {
            item: Box::new(item.clone()),
        });
    Ok(())
}
pub(crate) fn discard_without_targets(state: &mut GameState, pending: &PendingTrigger) {
    if pending
        .source_contract
        .is_some_and(|source| owns_definition(source.card_def))
        && pending.effect == recipe()
        && !crate::engine::pending_trigger_targets_can_complete(pending, state)
    {
        state
            .engine
            .event_history
            .push(CommittedEvent::BanishingNoTargetsV1 {
                source: pending.source_contract.expect("selected original entry"),
            });
    }
}

pub(crate) struct RemovalOwnerV1 {
    item: StackItem,
}
pub(crate) fn acquire(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<RemovalOwnerV1>, String> {
    let Some(source) = source_of(item) else {
        return Ok(None);
    };
    validate_source(state, source)?;
    crate::engine::validated_stack_item_target_spec(item, state)?;
    let rows: Vec<_> = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| {
            if let CommittedEvent::BanishingPublishedV1 { item: published } = event {
                (published.v4.stack_item_id == item.v4.stack_item_id).then_some(published.as_ref())
            } else {
                None
            }
        })
        .collect();
    if rows.as_slice() != [item]
        || !exact_item(item, source)
        || state
            .stack
            .iter()
            .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
            .collect::<Vec<_>>()
            .as_slice()
            != [item]
    {
        return Err("Banishing lost its genuine unique published live owner".into());
    }
    Ok(Some(RemovalOwnerV1 { item: item.clone() }))
}
pub(crate) fn record_removed(
    state: &mut GameState,
    owner: Option<RemovalOwnerV1>,
    outcome: RemovalV1,
) {
    if let Some(owner) = owner {
        debug_assert!(!state
            .stack
            .iter()
            .any(|row| row.v4.stack_item_id == owner.item.v4.stack_item_id));
        state
            .engine
            .event_history
            .push(CommittedEvent::BanishingRemovedV1 {
                item: Box::new(owner.item),
                outcome,
            });
    }
}
pub(crate) fn source_present(state: &GameState, source: AbilitySourceContractV4) -> bool {
    state.objects.try_get(source.source).is_some_and(|live| {
        live.owner == source.owner
            && live.zone == Zone::Battlefield
            && live.zone_change_count == source.zone_change_count
            && (live.card_def == source.card_def
                || crate::standard_cards_v1::had_card_def(
                    state,
                    source.source,
                    source.zone_change_count,
                    source.card_def,
                ))
    })
}

pub(crate) fn record_install(state: &mut GameState, record: LinkedExileRecordV4, start: usize) {
    if !owns_definition(record.source.card_def) {
        return;
    }
    let actual=state.engine.event_history[start..].iter().position(|event|
        matches!(event,CommittedEvent::ZoneChange {object,from:Zone::Battlefield,to:Zone::Exile,..}
            if *object==record.exiled)).map(|offset|start+offset);
    if let Some(actual) = actual {
        state
            .engine
            .event_history
            .push(CommittedEvent::BanishingLinkInstalledV1 {
                record,
                exile_history_index: u64::try_from(actual).expect("bounded exile history"),
            });
    }
}

pub(crate) fn record_expiry(state: &mut GameState, record: LinkedExileRecordV4, start: usize) {
    if !owns_definition(record.source.card_def) {
        return;
    }
    let departure=validate_source(state,record.source).ok().and_then(|entry| {
        state.engine.event_history[entry+1..start].iter().position(|event|
            matches!(event,CommittedEvent::ZoneChange {object,from:Zone::Battlefield,..} if *object==record.source.source))
            .map(|offset|entry+1+offset)
    });
    let returned=state.engine.event_history[start..].iter().position(|event|
        matches!(event,CommittedEvent::ZoneChange {object,from:Zone::Exile,to:Zone::Battlefield,..}
            if *object==record.exiled)).map(|offset|start+offset);
    if let Some(departure) = departure {
        state
            .engine
            .event_history
            .push(CommittedEvent::BanishingLinkExpiredV1 {
                record,
                departure_history_index: u64::try_from(departure)
                    .expect("bounded source departure"),
                return_history_index: returned
                    .map(|index| u64::try_from(index).expect("bounded return history")),
            });
    }
}

pub(crate) fn record_recipient_ceased(state: &mut GameState, records: &[LinkedExileRecordV4]) {
    for record in records
        .iter()
        .filter(|record| owns_definition(record.source.card_def))
    {
        state
            .engine
            .event_history
            .push(CommittedEvent::BanishingRecipientCeasedV1 { record: *record });
    }
}

pub(crate) fn finish(state: &mut GameState, item: &StackItem) {
    if source_of(item).is_some() {
        state
            .engine
            .event_history
            .push(CommittedEvent::BanishingFinishedV1 {
                stack_item_id: item.v4.stack_item_id,
            });
    }
}

pub(crate) fn requires_successor(state: &GameState) -> bool {
    state.engine.event_history.iter().any(is_authority)
}
pub(crate) fn is_authority(event: &CommittedEvent) -> bool {
    matches!(
        event,
        CommittedEvent::BanishingEntryV1 { .. }
            | CommittedEvent::BanishingCollectedV1 { .. }
            | CommittedEvent::BanishingPublishedV1 { .. }
            | CommittedEvent::BanishingRemovedV1 { .. }
            | CommittedEvent::BanishingNoTargetsV1 { .. }
            | CommittedEvent::BanishingFinishedV1 { .. }
            | CommittedEvent::BanishingLinkInstalledV1 { .. }
            | CommittedEvent::BanishingLinkExpiredV1 { .. }
            | CommittedEvent::BanishingRecipientCeasedV1 { .. }
    )
}

pub fn validate_state(state: &GameState) -> Result<(), String> {
    if state.engine.event_log.iter().any(is_authority) {
        return Err("Banishing authority is history-only".into());
    }
    // Ordinary entry events have no generation payload. Match log multiplicity
    // against still-uncollected actual entry indices, allowing a genuine later
    // reentry whose ordinary ZoneChange value equals an earlier one.
    for logged in &state.engine.event_log {
        if crate::trigger::battlefield_entry_object(logged).is_none() {
            continue;
        }
        let waiting = state.engine.event_history.iter().filter(|event| {
            let CommittedEvent::BanishingEntryV1 {source,entry_history_index}=event else {return false};
            let actual=usize::try_from(*entry_history_index).ok().and_then(|at|state.engine.event_history.get(at));
            actual==Some(logged) && !state.engine.event_history.iter().any(|event|
                matches!(event,CommittedEvent::BanishingCollectedV1 {source:original} if original==source))
        }).count();
        let previously_tracked = state.engine.event_history.iter().any(|event| {
            let CommittedEvent::BanishingEntryV1 {
                entry_history_index,
                ..
            } = event
            else {
                return false;
            };
            usize::try_from(*entry_history_index)
                .ok()
                .and_then(|at| state.engine.event_history.get(at))
                == Some(logged)
        });
        if previously_tracked
            && state
                .engine
                .event_log
                .iter()
                .filter(|event| *event == logged)
                .count()
                > waiting
        {
            return Err("Banishing collected entry was reintroduced into mutable log".into());
        }
    }
    for (at, event) in state.engine.event_history.iter().enumerate() {
        match event {
            CommittedEvent::BanishingEntryV1 {source,entry_history_index}=> {
                validate_source(state,*source)?;
                let collected=state.engine.event_history.iter().filter(|event|matches!(event,
                    CommittedEvent::BanishingCollectedV1 {source:original} if original==source)).count();
                let no_targets=state.engine.event_history.iter().filter(|event|matches!(event,
                    CommittedEvent::BanishingNoTargetsV1 {source:original} if original==source)).count();
                let published=state.engine.event_history.iter().filter(|event|matches!(event,
                    CommittedEvent::BanishingPublishedV1 {item} if item.v4.ability_source_contract==Some(*source))).count();
                let pending=pending_owners(state).filter(|pending|pending.source_contract==Some(*source)
                    && pending.effect==recipe() && pending.controller==source.controller
                    && pending.target_spec==TargetSpec::OpponentNonlandPermanent).count();
                let logged=usize::try_from(*entry_history_index).ok().and_then(|index|state.engine.event_history.get(index))
                    .is_some_and(|event|state.engine.event_log.contains(event));
                if collected>1 || no_targets>1 || published>1
                    || (collected==0 && (!logged || pending+published+no_targets!=0))
                    || (collected==1 && pending+published+no_targets!=1) {
                    return Err("Banishing original entry lost or replayed its mandatory dispatch owner".into());
                }
            }
            CommittedEvent::BanishingPublishedV1 {item}=> {
                let source=item.v4.ability_source_contract.ok_or("Banishing publication lost source")?;
                validate_source(state,source)?;
                if !exact_item(item,source) {return Err("Banishing publication root changed".into())}
                let removals:Vec<_>=state.engine.event_history.iter().enumerate().filter_map(|(index,event)|
                    if let CommittedEvent::BanishingRemovedV1 {item:removed,outcome}=event {
                        (removed.v4.stack_item_id==item.v4.stack_item_id).then_some((index,removed.as_ref(),outcome))
                    } else {None}).collect();
                let live:Vec<_>=state.stack.iter().filter(|live|live.v4.stack_item_id==item.v4.stack_item_id).collect();
                let closed=state.engine.event_history.iter().filter(|event|matches!(event,
                    CommittedEvent::BanishingFinishedV1 {stack_item_id} if *stack_item_id==item.v4.stack_item_id)).count();
                match removals.as_slice() {
                    [] if live.as_slice()==[item.as_ref()] && closed==0=>{},
                    [(removed_at,original,outcome)] if *removed_at>at && *original==item.as_ref() && live.is_empty() && closed==1=> {
                        let finished_at=state.engine.event_history.iter().position(|event|matches!(event,
                            CommittedEvent::BanishingFinishedV1 {stack_item_id} if *stack_item_id==item.v4.stack_item_id)).expect("one finish");
                        if finished_at<=*removed_at {return Err("Banishing completion preceded actual removal".into())}
                        if let RemovalV1::Resolving {source_present:true,target_legal:true}=outcome {
                            let crate::state::Target::Object(target)=item.targets[0] else {return Err("Banishing target is not a permanent".into())};
                            for (offset,event) in state.engine.event_history[removed_at+1..finished_at].iter().enumerate() {
                                if matches!(event,CommittedEvent::ZoneChange {object,from:Zone::Battlefield,to:Zone::Exile,..} if *object==target) {
                                    let actual=removed_at+1+offset;
                                    let installs=state.engine.event_history.iter().filter(|event|matches!(event,
                                        CommittedEvent::BanishingLinkInstalledV1 {record,exile_history_index}
                                            if record.source==source && record.exiled==target && usize::try_from(*exile_history_index).ok()==Some(actual))).count();
                                    if installs!=1 {return Err("Banishing successful exile lost its original installation".into())}
                                }
                            }
                        }
                    },
                    _=>return Err("Banishing published trigger was dropped, changed or replayed".into()),
                }
            }
            CommittedEvent::BanishingLinkInstalledV1 {record,exile_history_index}=> {
                validate_source(state,record.source)?;
                if state.engine.event_history.iter().filter(|event|matches!(event,
                    CommittedEvent::BanishingLinkInstalledV1 {record:original,..} if original==record)).count()!=1 {
                    return Err("Banishing original installation duplicated".into());
                }
                let exile=usize::try_from(*exile_history_index).map_err(|_|"exile index exceeds usize")?;
                if exile>=at || !matches!(state.engine.event_history.get(exile),Some(CommittedEvent::ZoneChange {
                    object,from:Zone::Battlefield,to:Zone::Exile,..}) if *object==record.exiled) {
                    return Err("Banishing link lost its actual successful exile".into());
                }
                let original = state.engine.event_history[..exile].iter().rev().find_map(|event| {
                    let CommittedEvent::BanishingRemovedV1 {item,outcome:RemovalV1::Resolving {source_present:true,target_legal:true}}=event else {return None};
                    (item.v4.ability_source_contract==Some(record.source) && item.targets.first()==Some(&crate::state::Target::Object(record.exiled)))
                        .then_some(item.as_ref())
                }).ok_or("Banishing link has no genuinely resolving original trigger")?;
                let Some(crate::state::StackTargetContractV4::Object {object,owner,zone:Zone::Battlefield,zone_change_count,..})=original.v4.target_contracts.first() else {
                    return Err("Banishing link lost original permanent target".into())
                };
                if *object!=record.exiled || *owner!=record.exiled_owner || zone_change_count.checked_add(1)!=Some(record.exiled_zone_change_count) {
                    return Err("Banishing link changed original target owner or incarnation".into())
                }
                let expiry:Vec<_>=state.engine.event_history.iter().enumerate().filter_map(|(index,event)|
                    if let CommittedEvent::BanishingLinkExpiredV1 {record:expired,departure_history_index,return_history_index}=event {
                        (*expired==*record).then_some((index,*departure_history_index,*return_history_index))
                    } else {None}).collect();
                let ceased=state.engine.event_history.iter().filter(|event|matches!(event,
                    CommittedEvent::BanishingRecipientCeasedV1 {record:original} if original==record)).count();
                let active=state.engine.linked_exile_records.iter().filter(|active|**active==*record).count();
                match expiry.as_slice() {
                    [] if ceased==0 && active==1=>{},
                    [] if ceased==1 && active==0 && state.objects.try_get(record.exiled).is_some_and(|object|object.v4.is_token)
                        && !state.exile.contains(&record.exiled)=>{},
                    [(expired_at,departure,returned)] if *expired_at>at && active==0 && ceased==0=> {
                        let departure=usize::try_from(*departure).map_err(|_|"departure index exceeds usize")?;
                        let original_entry=validate_source(state,record.source)?;
                        let first_departure=state.engine.event_history[original_entry+1..*expired_at].iter().position(|event|
                            matches!(event,CommittedEvent::ZoneChange {object,from:Zone::Battlefield,..} if *object==record.source.source))
                            .map(|offset|original_entry+1+offset);
                        if first_departure!=Some(departure) || departure>=*expired_at || !matches!(state.engine.event_history.get(departure),
                            Some(CommittedEvent::ZoneChange {object,from:Zone::Battlefield,..}) if *object==record.source.source) {
                            return Err("Banishing expiry lost actual source departure".into());
                        }
                        if let Some(returned)=returned {
                            let returned=usize::try_from(*returned).map_err(|_|"return index exceeds usize")?;
                            if returned<=departure || returned>=*expired_at || !matches!(state.engine.event_history.get(returned),
                                Some(CommittedEvent::ZoneChange {object,from:Zone::Exile,to:Zone::Battlefield,..}) if *object==record.exiled) {
                                return Err("Banishing expiry lost immediate actual return".into());
                            }
                        } else if !state.engine.event_history[exile+1..departure].iter().any(|event|matches!(event,
                            CommittedEvent::ZoneChange {object,from:Zone::Exile,..} if *object==record.exiled)) {
                            return Err("Banishing expiry skipped a live exact exile recipient".into());
                        }
                    }
                    _=>return Err("Banishing link was erased, rewritten or replayed".into()),
                }
            }
            CommittedEvent::BanishingCollectedV1 {source} | CommittedEvent::BanishingNoTargetsV1 {source} => {
                let entry=validate_source(state,*source)?;
                if entry>=at {return Err("Banishing dispatch preceded original entry".into())}
            },
            CommittedEvent::BanishingRemovedV1 {..} | CommittedEvent::BanishingFinishedV1 {..} => {
                let id=match event {CommittedEvent::BanishingRemovedV1 {item,..}=>item.v4.stack_item_id,
                    CommittedEvent::BanishingFinishedV1 {stack_item_id}=>*stack_item_id,_=>unreachable!()};
                if state.engine.event_history[..at].iter().filter(|event|matches!(event,
                    CommittedEvent::BanishingPublishedV1 {item} if item.v4.stack_item_id==id)).count()!=1 {
                    return Err("Banishing removal/closure has no original publication".into())
                }
            },
            CommittedEvent::BanishingLinkExpiredV1 {record,..} | CommittedEvent::BanishingRecipientCeasedV1 {record} => {
                if state.engine.event_history[..at].iter().filter(|event|matches!(event,
                    CommittedEvent::BanishingLinkInstalledV1 {record:original,..} if original==record)).count()!=1 {
                    return Err("Banishing expiry/cease has no original installation".into())
                }
            },
            _=>{},
        }
    }
    for record in state
        .engine
        .linked_exile_records
        .iter()
        .filter(|record| owns_definition(record.source.card_def))
    {
        if state
            .engine
            .event_history
            .iter()
            .filter(|event| {
                matches!(event,
            CommittedEvent::BanishingLinkInstalledV1 {record:original,..} if original==record)
            })
            .count()
            != 1
        {
            return Err("Banishing active link has no independent original installation".into());
        }
    }
    Ok(())
}
