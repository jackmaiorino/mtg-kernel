//! Event-time eligibility and mandatory custody of the two printed Raid entries.
use crate::event::CommittedEvent;
use crate::ids::{PlayerId, StackItemId};
use crate::state::{
    AbilitySourceContractV4, AttackDeclarationTurnV1, GameState, StackItem, StackItemKind, Zone,
};
use crate::trigger::PendingTrigger;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CaptureV1 {
    pub index: u32,
    pub entry_capture: usize,
    pub source: AbilitySourceContractV4,
    pub turn: u32,
    pub active: PlayerId,
    pub attacked: Option<AttackDeclarationTurnV1>,
    pub printed_active: bool,
    pub face_index: u8,
    pub pending: Option<Box<PendingTrigger>>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PublishedV1 {
    pub index: u32,
    pub capture: u32,
    pub item: Box<StackItem>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DroppedV1 {
    pub index: u32,
    pub capture: u32,
    pub pending: Box<PendingTrigger>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CounteredV1 {
    pub index: u32,
    pub publication: u32,
    pub item: Box<StackItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StartedV1 {
    pub index: u32,
    pub publication: u32,
    pub first_boundary: u32,
    pub item: Box<StackItem>,
}
pub(crate) fn record_started(state: &mut GameState, id: StackItemId) -> Result<(), String> {
    let Some(pending) = state.engine.pending_effect.as_ref() else {
        return Err("Raid start lacks actual pending owner".into());
    };
    let item = &pending.resolving_item;
    if item.v4.stack_item_id != id
        || item.v4.ability_source_contract.is_none_or(|s| {
            !definition(s)
                || crate::card_def::CARD_DEFS[usize::from(s.card_def)].name != "Gutless Plunderer"
        })
    {
        return Ok(());
    }
    let p = published_for_id(state, id)?;
    let publication = p.index;
    if !published_matches_base(state, item, p)
        || state.engine.event_history.iter().any(|e| {
            matches!(e,
  CommittedEvent::RaidEntryStartedV1{started:s} if s.publication==publication)
        })
    {
        return Err("Gutless begin changed or replayed its original published owner".into());
    }
    let first_boundary = item
        .v4
        .squad_rallier_started_v1
        .0
        .ok_or("Gutless start lost real first-boundary guard")?
        .first_receipt_history_index;
    let item = Box::new(item.clone());
    let index =
        u32::try_from(state.engine.event_history.len()).map_err(|_| "Raid start overflow")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::RaidEntryStartedV1 {
            started: Box::new(StartedV1 {
                index,
                publication,
                first_boundary,
                item,
            }),
        });
    Ok(())
}

pub(crate) struct CounterOwnerV1 {
    publication: u32,
    item: StackItem,
}
fn definition(source: AbilitySourceContractV4) -> bool {
    crate::raid_entry_v1::owns_definition(source.card_def)
}
fn expected(source: AbilitySourceContractV4) -> PendingTrigger {
    let effect = (crate::trigger::triggers_for(source.card_def)[0].effect)();
    PendingTrigger {
        controller: source.controller,
        source: source.source,
        granted_by: None,
        target_spec: crate::trigger::target_spec_for_trigger(source.card_def, &effect)
            .expect("selected printed Raid target"),
        effect,
        is_madness_offer: false,
        kicked: false,
        targets: vec![],
        target_contracts: vec![],
        placement_ordered: false,
        source_contract: Some(source),
        optional_additional_cost_paid: None,
        paid_cost_refs: vec![],
    }
}
pub(crate) fn capture_entry(state: &mut GameState, entry_capture: usize) {
    let Some(CommittedEvent::RaidEntrySourceV1 { source, .. }) =
        state.engine.event_history.get(entry_capture)
    else {
        return;
    };
    let source = *source;
    if !definition(source) {
        return;
    }
    if state.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::RaidEntryCaptureV1 {capture} if capture.entry_capture==entry_capture)){return;}
    let live = state.objects.get(source.source);
    let face_index = live.v4.face_index;
    let printed_active = live.zone == Zone::Battlefield
        && live.zone_change_count == source.zone_change_count
        && crate::continuous_characteristics_v1::printed_abilities_active(state, source.source);
    let turn = state.turn_key_v1();
    let active = state.active_player;
    let attacked = state.attack_declaration_turn_v1;
    let eligible =
        printed_active && face_index == 0 && state.player_attacked_this_turn_v1(source.controller);
    let pending = eligible.then(|| Box::new(expected(source)));
    let index =
        u32::try_from(state.engine.event_history.len()).expect("bounded Raid entry history");
    state
        .engine
        .event_history
        .push(CommittedEvent::RaidEntryCaptureV1 {
            capture: Box::new(CaptureV1 {
                index,
                entry_capture,
                source,
                turn,
                active,
                attacked,
                printed_active,
                face_index,
                pending: pending.clone(),
            }),
        });
    if let Some(pending) = pending {
        state
            .raid_waiting_v1
            .get_or_insert_with(Vec::new)
            .push(*pending);
    }
}
pub(crate) fn capture_batch_entries(state: &mut GameState, from: usize) {
    let entries = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .skip(from)
        .filter_map(|(index, e)| {
            matches!(e, CommittedEvent::RaidEntrySourceV1 { .. }).then_some(index)
        })
        .collect::<Vec<_>>();
    for index in entries {
        capture_entry(state, index);
    }
}
pub(crate) fn take_captures(state: &mut GameState) -> Vec<PendingTrigger> {
    state.raid_waiting_v1.take().unwrap_or_default()
}
fn capture(state: &GameState, index: u32) -> Result<&CaptureV1, String> {
    let Some(CommittedEvent::RaidEntryCaptureV1 { capture: c }) =
        state.engine.event_history.get(index as usize)
    else {
        return Err("Raid entry lost eligibility capture".into());
    };
    let Some(CommittedEvent::RaidEntrySourceV1 { source, .. }) =
        state.engine.event_history.get(c.entry_capture)
    else {
        return Err("Raid eligibility lost genuine entry authority".into());
    };
    if c.index != index
        || c.entry_capture >= index as usize
        || c.source != *source
        || !definition(c.source)
    {
        return Err("Raid original eligible occurrence changed".into());
    }
    let met = c.attacked.is_some_and(|attack| {
        attack.turn == c.turn
            && attack.active_player == c.active
            && attack.active_player == c.source.controller
    });
    let eligible = c.printed_active && c.face_index == 0 && met;
    if c.pending.as_deref() != eligible.then(|| expected(c.source)).as_ref() {
        return Err(
            "Raid eligibility differs from captured original controller/declaration/abilities"
                .into(),
        );
    }
    crate::raid_entry_v1::validate_source_for_program(
        state,
        &expected(c.source).effect,
        Some(c.source),
    )?;
    Ok(c)
}
fn capture_for_source(
    state: &GameState,
    source: AbilitySourceContractV4,
) -> Result<&CaptureV1, String> {
    let rows = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::RaidEntryCaptureV1 { capture: c } if c.source == source => {
                Some(c.as_ref())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [c] = rows.as_slice() else {
        return Err("Raid source lost unique eligible entry".into());
    };
    let c = capture(state, c.index)?;
    if c.pending.is_none() {
        return Err("Raid trigger came from an ineligible entry".into());
    }
    Ok(c)
}
fn pending_matches(pending: &PendingTrigger, capture: &CaptureV1) -> bool {
    let Some(expected) = capture.pending.as_deref() else {
        return false;
    };
    let mut normalized = pending.clone();
    normalized.targets.clear();
    normalized.target_contracts.clear();
    normalized.placement_ordered = false;
    normalized == *expected
}
pub(crate) fn publish(state: &mut GameState, id: StackItemId) -> Result<(), String> {
    let row = state
        .stack
        .iter()
        .find(|i| i.v4.stack_item_id == id)
        .ok_or("Raid publication lacks live row")?;
    let Some(source) = row
        .v4
        .ability_source_contract
        .filter(|source| definition(*source))
    else {
        return Ok(());
    };
    if row.kind != StackItemKind::TriggeredAbility
        || row.v4.granted_by.is_some()
        || row.inline_effect.as_ref() != Some(&expected(source).effect)
    {
        return Ok(());
    }
    let c = capture_for_source(state, source)?;
    let capture = c.index;
    if state.engine.event_history.iter().any(|event|matches!(event,CommittedEvent::RaidEntryPublishedV1{publication:p} if p.capture==capture)) {
  return Err("Raid entry was already published".into());
 }
    let index =
        u32::try_from(state.engine.event_history.len()).map_err(|_| "Raid publication overflow")?;
    let item = Box::new(row.clone());
    state
        .engine
        .event_history
        .push(CommittedEvent::RaidEntryPublishedV1 {
            publication: Box::new(PublishedV1 {
                index,
                capture,
                item,
            }),
        });
    Ok(())
}
fn published(state: &GameState, index: u32) -> Result<&PublishedV1, String> {
    let Some(CommittedEvent::RaidEntryPublishedV1 { publication: p }) =
        state.engine.event_history.get(index as usize)
    else {
        return Err("Raid lost immutable publication".into());
    };
    let c = capture(state, p.capture)?;
    let expected = c
        .pending
        .as_deref()
        .ok_or("Raid published ineligible entry")?;
    if p.index != index
        || p.capture >= index
        || p.item.kind != StackItemKind::TriggeredAbility
        || p.item.source != c.source.source
        || p.item.controller != c.source.controller
        || p.item.v4.ability_source_contract != Some(c.source)
        || p.item.inline_effect.as_ref() != Some(&expected.effect)
        || p.item.v4.target_spec
            != (expected.target_spec != crate::card_def::TargetSpec::None)
                .then_some(expected.target_spec)
        || p.item.targets.len()
            != if expected.target_spec == crate::card_def::TargetSpec::None {
                0
            } else {
                1
            }
        || p.item.targets.len() != p.item.v4.target_contracts.len()
        || p.item
            .targets
            .iter()
            .zip(&p.item.v4.target_contracts)
            .any(|(t, c)| *t != c.target())
        || !p.item.v4.squad_rallier_started_v1.is_empty()
    {
        return Err("Raid publication changed the printed pending occurrence".into());
    }
    Ok(p)
}
fn published_matches_base(state: &GameState, item: &StackItem, p: &PublishedV1) -> bool {
    let mut normalized = item.clone();
    if let Some(started) = item.v4.squad_rallier_started_v1.0 {
        if !matches!(state.engine.event_history.get(started.first_receipt_history_index as usize),
   Some(CommittedEvent::SquadRallierResolutionBoundaryV1 {receipt})
   if receipt.history_index==started.first_receipt_history_index && receipt.previous_history_index.is_none()
   && receipt.ordinal==0 && receipt.stack_item_id==item.v4.stack_item_id && receipt.source==started.source
   && receipt.pending.as_ref().is_some_and(|pending|pending.resolving_item==*item))
        {
            return false;
        }
        normalized.v4.squad_rallier_started_v1 = p.item.v4.squad_rallier_started_v1;
    }
    normalized == *p.item
}

fn published_matches(state: &GameState, item: &StackItem, p: &PublishedV1) -> bool {
    let starts = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::RaidEntryStartedV1 { started } if started.publication == p.index => {
                Some(started.as_ref())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    match starts.as_slice() {
        [] => item.v4.squad_rallier_started_v1.is_empty() && published_matches_base(state, item, p),
        [started] => started.item.as_ref() == item && published_matches_base(state, item, p),
        _ => false,
    }
}

fn published_for_id(state: &GameState, id: StackItemId) -> Result<&PublishedV1, String> {
    let rows = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            CommittedEvent::RaidEntryPublishedV1 { publication: p }
                if p.item.v4.stack_item_id == id =>
            {
                Some(p.as_ref())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [p] = rows.as_slice() else {
        return Err("Raid lost unique published owner".into());
    };
    published(state, p.index)
}
pub(crate) fn counter_owner(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<CounterOwnerV1>, String> {
    let selected = item.kind == StackItemKind::TriggeredAbility
        && item.v4.granted_by.is_none()
        && item.v4.ability_source_contract.is_some_and(|s| {
            definition(s) && item.inline_effect.as_ref() == Some(&expected(s).effect)
        })
        && item.v4.ability_source_contract.is_some_and(definition);
    if !selected && !state.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::RaidEntryPublishedV1{publication:p} if p.item.v4.stack_item_id==item.v4.stack_item_id)){return Ok(None);}
    let p = published_for_id(state, item.v4.stack_item_id)?;
    if !published_matches(state, item, p)
        || state
            .stack
            .iter()
            .filter(|i| i.v4.stack_item_id == item.v4.stack_item_id)
            .count()
            != 1
        || !state.stack.iter().any(|i| i == item)
    {
        return Err("Raid counter lacks genuine full live owner".into());
    }
    Ok(Some(CounterOwnerV1 {
        publication: p.index,
        item: item.clone(),
    }))
}
pub(crate) fn record_countered(
    state: &mut GameState,
    owner: Option<CounterOwnerV1>,
) -> Result<(), String> {
    let Some(owner) = owner else {
        return Ok(());
    };
    if state
        .stack
        .iter()
        .any(|i| i.v4.stack_item_id == owner.item.v4.stack_item_id)
    {
        return Err("Raid counter did not remove its actual owned row".into());
    }
    let index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "Raid counter history overflow")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::RaidEntryCounteredV1 {
            countered: Box::new(CounteredV1 {
                index,
                publication: owner.publication,
                item: Box::new(owner.item),
            }),
        });
    Ok(())
}
pub(crate) fn record_drop(state: &mut GameState, pending: &PendingTrigger) -> Result<(), String> {
    let Some(source) = pending.source_contract.filter(|source| {
        definition(*source)
            && pending.granted_by.is_none()
            && pending.effect == expected(*source).effect
    }) else {
        return Ok(());
    };
    let c = capture_for_source(state, source)?;
    if !pending_matches(pending, c)
        || crate::engine::pending_trigger_targets_can_complete(pending, state)
    {
        return Err("Raid no-target removal lacks actual impossible target completion".into());
    }
    let capture = c.index;
    let index =
        u32::try_from(state.engine.event_history.len()).map_err(|_| "Raid drop overflow")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::RaidEntryDroppedV1 {
            dropped: Box::new(DroppedV1 {
                index,
                capture,
                pending: Box::new(pending.clone()),
            }),
        });
    Ok(())
}
fn private_record(e: &CommittedEvent) -> bool {
    matches!(
        e,
        CommittedEvent::RaidEntryStartedV1 { .. }
            | CommittedEvent::RaidEntryCaptureV1 { .. }
            | CommittedEvent::RaidEntryPublishedV1 { .. }
            | CommittedEvent::RaidEntryDroppedV1 { .. }
            | CommittedEvent::RaidEntryCounteredV1 { .. }
    )
}
fn waiting(state: &GameState) -> Vec<&PendingTrigger> {
    state
        .raid_waiting_v1
        .iter()
        .flatten()
        .chain(state.engine.pending_triggers.iter())
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
                .filter_map(|s| s.resolution_play.as_ref())
                .flat_map(|r| r.deferred_triggers.iter()),
        )
        .collect()
}
pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    if state.engine.event_log.iter().any(private_record) {
        return Err("Raid custody private records in public event log".into());
    }
    let waiting = waiting(state);
    for pending in &waiting {
        if let Some(source) = pending.source_contract.filter(|source| {
            definition(*source)
                && pending.granted_by.is_none()
                && pending.effect == expected(*source).effect
        }) {
            let c = capture_for_source(state, source)?;
            if !pending_matches(pending, c) {
                return Err("Raid waiting row differs from captured occurrence".into());
            }
        }
    }
    for (index, event) in state.engine.event_history.iter().enumerate() {
        match event {
            CommittedEvent::RaidEntrySourceV1 { source, .. } if definition(*source) => {
                let count=state.engine.event_history.iter().filter(|e|matches!(e,CommittedEvent::RaidEntryCaptureV1{capture:c} if c.entry_capture==index)).count();
                if count != 1 {
                    return Err(
                        "Raid actual entry lost its mandatory eligibility occurrence".into(),
                    );
                }
            }
            CommittedEvent::RaidEntryCaptureV1 { capture: c } => {
                if c.index as usize != index {
                    return Err("Raid eligibility moved in history".into());
                }
                let c = capture(state, c.index)?;
                let held = waiting
                    .iter()
                    .filter(|p| {
                        p.source_contract == Some(c.source)
                            && p.granted_by.is_none()
                            && p.effect == expected(c.source).effect
                    })
                    .count();
                let publications = state
                    .engine
                    .event_history
                    .iter()
                    .filter_map(|e| match e {
                        CommittedEvent::RaidEntryPublishedV1 { publication: p }
                            if p.capture == c.index =>
                        {
                            Some(p.as_ref())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let dropped = state
                    .engine
                    .event_history
                    .iter()
                    .filter_map(|e| match e {
                        CommittedEvent::RaidEntryDroppedV1 { dropped: d }
                            if d.capture == c.index =>
                        {
                            Some(d.as_ref())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if c.pending.is_none() {
                    if held != 0 || !publications.is_empty() || !dropped.is_empty() {
                        return Err("ineligible Raid entry acquired a trigger owner".into());
                    }
                } else if !matches!(
                    (held, publications.len(), dropped.len()),
                    (1, 0, 0) | (0, 1, 0) | (0, 0, 1)
                ) {
                    return Err("eligible Raid instruction lost or duplicated mandatory waiting/published/removal owner".into());
                }
            }
            CommittedEvent::RaidEntryPublishedV1 { publication: p } => {
                if p.index as usize != index {
                    return Err("Raid publication moved".into());
                }
                let p = published(state, p.index)?;
                let live = state
                    .stack
                    .iter()
                    .filter(|i| i.v4.stack_item_id == p.item.v4.stack_item_id)
                    .collect::<Vec<_>>();
                let counters = state
                    .engine
                    .event_history
                    .iter()
                    .filter_map(|e| match e {
                        CommittedEvent::RaidEntryCounteredV1 { countered: c }
                            if c.publication == p.index =>
                        {
                            Some(c.as_ref())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let gorehorn = crate::card_def::CARD_DEFS
                    [usize::from(p.item.v4.ability_source_contract.unwrap().card_def)]
                .name
                    == "Gorehorn Raider";
                let completed = if gorehorn {
                    state
                        .engine
                        .event_history
                        .iter()
                        .filter(|e| {
                            matches!(e,CommittedEvent::GorehornOperationV1{operation:o}
      if o.item.v4.stack_item_id==p.item.v4.stack_item_id)
                        })
                        .count()
                        == 1
                } else {
                    let receipts = state
                        .engine
                        .event_history
                        .iter()
                        .filter_map(|e| match e {
                            CommittedEvent::SquadRallierResolutionBoundaryV1 { receipt }
                                if receipt.stack_item_id == p.item.v4.stack_item_id =>
                            {
                                Some(receipt)
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    if let Some(first) = receipts.first() {
                        let initial = first
                            .pending
                            .as_ref()
                            .ok_or("Gutless first boundary lost actual owned row")?;
                        if !published_matches(state, &initial.resolving_item, p) {
                            return Err(
                                "Gutless first boundary differs from original publication".into()
                            );
                        }
                    }
                    receipts.last().is_some_and(|r| r.pending.is_none())
                };
                if counters.len() > 1 || (!gorehorn && !counters.is_empty() && completed) {
                    return Err("Raid published instruction has duplicate closure".into());
                }
                if completed || !counters.is_empty() {
                    if !live.is_empty() {
                        return Err("closed Raid instruction was reintroduced".into());
                    }
                } else if !matches!(live.as_slice(),[item] if published_matches(state,item,p)) {
                    return Err("Raid publication lost exact original full live owner".into());
                }
            }

            CommittedEvent::RaidEntryStartedV1 { started: s } => {
                let p = published(state, s.publication)?;
                if s.index as usize != index
                    || s.first_boundary <= p.index
                    || s.first_boundary >= s.index
                    || crate::card_def::CARD_DEFS
                        [usize::from(p.item.v4.ability_source_contract.unwrap().card_def)]
                    .name
                        != "Gutless Plunderer"
                    || !published_matches_base(state, &s.item, p)
                    || s.item.v4.squad_rallier_started_v1.0.is_none_or(|binding| {
                        binding.first_receipt_history_index != s.first_boundary
                    })
                {
                    return Err("Gutless actual begin lost its immutable original publication/first boundary".into());
                }
            }

            CommittedEvent::RaidEntryDroppedV1 { dropped: d } => {
                let c = capture(state, d.capture)?;
                if d.index as usize != index
                    || d.index <= c.index
                    || !pending_matches(&d.pending, c)
                {
                    return Err("Raid no-target removal changed original held occurrence".into());
                }
            }
            CommittedEvent::RaidEntryCounteredV1 { countered: c } => {
                let p = published(state, c.publication)?;
                if c.index as usize != index
                    || c.index <= p.index
                    || !published_matches(state, &c.item, p)
                {
                    return Err("Raid counter removal changed actual full published owner".into());
                }
            }
            _ => {}
        }
    }
    for item in &state.stack {
        if item.kind == StackItemKind::TriggeredAbility
            && item.v4.granted_by.is_none()
            && item.v4.ability_source_contract.is_some_and(|s| {
                definition(s) && item.inline_effect.as_ref() == Some(&expected(s).effect)
            })
            && item.v4.ability_source_contract.is_some_and(definition)
        {
            let p = published_for_id(state, item.v4.stack_item_id)?;
            if !published_matches(state, item, p) {
                return Err("Raid live row changed published ownership".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_registered_path_preserves_bytes_hash_and_omitted_queue_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = crate::policy_observation_v6::tests::ready_state();
            state.active_player = actor;
            state.priority_player = actor;
            crate::policy_observation_v6::tests::put(
                &mut state,
                actor,
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let before = serde_json::to_vec(&state).unwrap();
            let hash = state.state_hash();
            capture_entry(&mut state, usize::MAX);
            capture_batch_entries(&mut state, 0);
            validate(&state).unwrap();
            assert!(take_captures(&mut state).is_empty());
            assert_eq!(before, serde_json::to_vec(&state).unwrap());
            assert_eq!(hash, state.state_hash());
            assert!(serde_json::to_value(&state)
                .unwrap()
                .get("raid_waiting_v1")
                .is_none());
        }
    }
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn entered(actor: PlayerId, name: &str, attacks: bool) -> (GameState, crate::ids::ObjectId) {
        let mut state = crate::policy_observation_v6::tests::ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        state.step = crate::state::Step::DeclareAttackers;
        let attacker = crate::policy_observation_v6::tests::put(
            &mut state,
            actor,
            "Llanowar Elves",
            Zone::Battlefield,
        );
        crate::policy_observation_v6::tests::put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        crate::policy_observation_v6::tests::put(
            &mut state,
            actor.opponent(),
            "Lightning Bolt",
            Zone::Hand,
        );
        crate::policy_observation_v6::tests::put(&mut state, actor, "Mountain", Zone::Battlefield);
        crate::policy_observation_v6::tests::put(
            &mut state,
            actor.opponent(),
            "Mountain",
            Zone::Battlefield,
        );
        assert!(matches!(
            crate::engine::advance_until_decision(&mut state),
            crate::engine::Decision::DeclareAttackers { .. }
        ));
        crate::engine::step(
            &mut state,
            crate::engine::Action::DeclareAttackers(if attacks { vec![attacker] } else { vec![] }),
        )
        .unwrap();
        // Typed entry primitive after a genuine declaration. No printed cast claim.
        state.step = crate::state::Step::Main2;
        let source = crate::policy_observation_v6::tests::put(&mut state, actor, name, Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        (state, source)
    }
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    fn actual_declared_raid_entry_fresh_queue_erasure_and_ineligible_entry_restore_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for name in ["Gorehorn Raider", "Gutless Plunderer"] {
                for attacks in [false, true] {
                    let (state, _) = entered(actor, name, attacks);
                    assert_eq!(
                        state.raid_waiting_v1.as_ref().map_or(0, Vec::len),
                        usize::from(attacks)
                    );
                    validate(&state).unwrap();
                    let bytes = serde_json::to_vec(&state).unwrap();
                    let restored: GameState = serde_json::from_slice(&bytes).unwrap();
                    assert_eq!(bytes, serde_json::to_vec(&restored).unwrap());
                    assert_eq!(state.state_hash(), restored.state_hash());
                    if attacks {
                        let mut bad = restored;
                        bad.raid_waiting_v1 = None;
                        let before = serde_json::to_vec(&bad).unwrap();
                        assert!(crate::engine::step(&mut bad, crate::engine::Action::Pass).is_err());
                        assert_eq!(before, serde_json::to_vec(&bad).unwrap());
                        assert_advance_halt_only(bad);
                    }
                }
            }
        }
    }
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    fn real_gutless_unstarted_publication_counter_primitive_closes_once_and_restores_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, source) = entered(actor, "Gutless Plunderer", true);
            assert!(matches!(
                crate::engine::advance_until_decision(&mut state),
                crate::engine::Decision::CastSpellOrPass { .. }
            ));
            let item = state
                .stack
                .iter()
                .find(|i| i.source == source)
                .unwrap()
                .clone();
            assert!(state.engine.pending_effect.is_none());
            validate(&state).unwrap();
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            crate::engine::counter_stack_item_by_id(&mut state, item.v4.stack_item_id)
                .unwrap()
                .unwrap();
            crate::engine::counter_stack_item_by_id(&mut restored, item.v4.stack_item_id)
                .unwrap()
                .unwrap();
            validate(&state).unwrap();
            crate::effect::validate_pending_effect_choice(&state).unwrap();
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
            assert!(
                crate::engine::counter_stack_item_by_id(&mut state, item.v4.stack_item_id)
                    .unwrap()
                    .is_none()
            );
            let before = serde_json::to_vec(&state).unwrap();
            state
                .engine
                .event_history
                .retain(|e| !matches!(e, CommittedEvent::RaidEntryCounteredV1 { .. }));
            let malformed = serde_json::to_vec(&state).unwrap();
            assert_ne!(before, malformed);
            assert!(crate::engine::step(&mut state, crate::engine::Action::Pass).is_err());
            assert_eq!(malformed, serde_json::to_vec(&state).unwrap());
        }
    }

    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    fn real_legend_suspension_keeps_exact_raid_waiting_owner_and_refuses_erasure_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, source) = entered(actor, "Gutless Plunderer", true);
            let first = crate::policy_observation_v6::tests::put(
                &mut state,
                actor,
                "Dwynen, Gilt-Leaf Daen",
                Zone::Battlefield,
            );
            crate::policy_observation_v6::tests::put(
                &mut state,
                actor,
                "Dwynen, Gilt-Leaf Daen",
                Zone::Battlefield,
            );
            assert!(matches!(crate::engine::advance_until_decision(&mut state),
    crate::engine::Decision::ChooseLegendPermanent{player,..} if player==actor));
            assert!(state
                .pending_legend_rule_v1
                .as_ref()
                .unwrap()
                .waiting_triggers()
                .iter()
                .any(|pending| pending.source == source));
            validate(&state).unwrap();
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            let mut bad = restored.clone();
            bad.pending_legend_rule_v1 = None;
            let before = serde_json::to_vec(&bad).unwrap();
            assert!(crate::engine::step(&mut bad, crate::engine::Action::Pass).is_err());
            assert_eq!(before, serde_json::to_vec(&bad).unwrap());
            for current in [&mut state, &mut restored] {
                crate::engine::step(current, crate::engine::Action::ChooseLegendPermanent(first))
                    .unwrap();
                let _ = crate::engine::advance_until_decision(current);
                validate(current).unwrap();
                assert!(current.stack.iter().any(|item| item.source == source));
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }

    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn assert_advance_halt_only(mut bad: GameState) {
        let mut expected = bad.clone();
        expected.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            crate::ids::ObjectId(0),
        ));
        assert!(matches!(
            crate::engine::advance_until_decision(&mut bad),
            crate::engine::Decision::Halted {
                mechanic: crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                source: crate::ids::ObjectId(0)
            }
        ));
        assert_eq!(
            serde_json::to_vec(&expected).unwrap(),
            serde_json::to_vec(&bad).unwrap()
        );
    }
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    fn unstarted_published_raid_owner_erasure_refuses_advance_before_mutation_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for name in ["Gorehorn Raider", "Gutless Plunderer"] {
                let (mut state, source) = entered(actor, name, true);
                loop {
                    match crate::engine::advance_until_decision(&mut state) {
                        crate::engine::Decision::ChooseTargets { legal_targets, .. } => {
                            crate::engine::step(
                                &mut state,
                                crate::engine::Action::ChooseTarget(legal_targets[0]),
                            )
                            .unwrap();
                        }
                        crate::engine::Decision::CastSpellOrPass { .. } => break,
                        other => panic!("expected real Raid publication, got {other:?}"),
                    }
                }
                assert!(state.engine.pending_effect.is_none());
                let item = state
                    .stack
                    .iter()
                    .find(|i| i.source == source)
                    .unwrap()
                    .clone();
                assert!(state.engine.event_history.iter().any(|e| matches!(
                    e, CommittedEvent::RaidEntryPublishedV1 { publication }
                    if publication.item.v4.stack_item_id == item.v4.stack_item_id
                )));
                let mut bad: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                bad.stack
                    .retain(|i| i.v4.stack_item_id != item.v4.stack_item_id);
                let before = serde_json::to_vec(&bad).unwrap();
                assert!(crate::engine::step(&mut bad, crate::engine::Action::Pass).is_err());
                assert_eq!(before, serde_json::to_vec(&bad).unwrap());
                assert_advance_halt_only(bad);
            }
        }
    }
}
