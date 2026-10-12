//! Original source facts for the actual equipment-granted damage trigger.
use crate::card_def::CARD_DEFS;
use crate::effect::{EffectOp, ExecCtx, TargetRef};
use crate::event::{CommittedEvent, HistoricalDamageSourceV1, ProposedEvent};
use crate::ids::ObjectId;
use crate::state::{
    AbilitySourceContractV4, GameObject, GameState, StackItem, StackItemId, StackItemKind, Zone,
};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PublicationV4 {
    pub index: u32,
    pub item: Box<StackItem>,
    pub public_snapshot: GameObject,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DepartureV4 {
    pub index: u32,
    pub snapshot: GameObject,
    pub facts: HistoricalDamageSourceV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PreparationV4 {
    pub index: u32,
    pub publication: u32,
    pub ctx: ExecCtx,
    pub facts: HistoricalDamageSourceV1,
    pub departure: Option<u32>,
    pub snapshot: Option<GameObject>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OutcomeV4 {
    Resolved,
    Countered,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClosedV4 {
    pub index: u32,
    pub publication: u32,
    pub item: Box<StackItem>,
    pub outcome: OutcomeV4,
    pub boundary: Option<u32>,
}
/// Nonserializable loan of the genuine complete row before actual removal.
pub(crate) struct OwnerV4 {
    item: StackItem,
    publication: u32,
}
fn enabled() -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && CARD_DEFS
        .iter()
        .any(|d| d.name == "Gorehorn Raider" && d.is_executable())
}
pub(crate) fn selected(item: &StackItem) -> bool {
    enabled()
        && item.kind == StackItemKind::TriggeredAbility
        && item.v4.granted_by.is_some_and(|grant| {
            CARD_DEFS
                .get(usize::from(grant.card_def))
                .and_then(|d| d.equipment)
                .is_some_and(|e| e.noncreature_spell_damage_to_each_opponent > 0)
        })
}
pub(crate) fn printed_root(item: &StackItem) -> Result<EffectOp, String> {
    let grant = item.v4.granted_by.ok_or("equipment damage lost donor")?;
    let equipment = CARD_DEFS
        .get(usize::from(grant.card_def))
        .and_then(|d| d.equipment)
        .ok_or("equipment damage lost descriptor")?;
    if equipment.noncreature_spell_damage_to_each_opponent == 0 {
        return Err("equipment damage descriptor is empty".into());
    }
    Ok(EffectOp::DealDamage {
        target: TargetRef::Opponent,
        amount: i32::from(equipment.noncreature_spell_damage_to_each_opponent),
    })
}
fn metadata(state: &GameState, item: &StackItem) -> Result<(), String> {
    let source = item
        .v4
        .ability_source_contract
        .ok_or("equipment damage lost host")?;
    let grant = item.v4.granted_by.ok_or("equipment damage lost donor")?;
    let root = printed_root(item)?;
    if !selected(item)
        || !CARD_DEFS
            .get(usize::from(source.card_def))
            .is_some_and(|d| d.is_executable())
        || item.source != source.source
        || item.controller != source.controller
        || item.inline_effect.as_ref() != Some(&root)
        || item.is_copy
        || item.madness_offer
        || item.v4.target_spec.is_some()
        || !item.targets.is_empty()
        || !item.v4.target_contracts.is_empty()
        || item.v4.activated_ability_index.is_some()
        || item.v4.source_contract.is_some()
        || !item.v4.gorehorn_published_v1.is_empty()
    {
        return Err("equipment damage changed genuine granted producer metadata".into());
    }
    crate::engine::validate_equipment_granted_trigger_contract(
        state,
        source,
        grant,
        item.controller,
        &root,
    )
}
pub(crate) fn context(item: &StackItem) -> ExecCtx {
    ExecCtx {
        stack_item_id: Some(item.v4.stack_item_id),
        source: item.source,
        controller: item.controller,
        targets: item.targets.clone(),
        target_contracts: item.v4.target_contracts.clone(),
        discarded: item.discarded.clone(),
        paid_cost_refs: item.v4.paid_cost_refs.clone(),
        hidden_ability_source: item.v4.hidden_ability_source,
        ability_source_contract: item.v4.ability_source_contract,
        cauldron_grant: item.v4.cauldron_grant,
        kicked: item.kicked,
        optional_additional_cost_paid: item.v4.optional_additional_cost_paid,
        x_value: item.v4.x_value,
    }
}
pub(crate) fn publish(state: &mut GameState, id: StackItemId) -> Result<(), String> {
    let item = state
        .stack
        .iter()
        .find(|i| i.v4.stack_item_id == id)
        .ok_or("equipment publication has no live row")?;
    if !selected(item) {
        return Ok(());
    }
    metadata(state, item)?;
    if !item.v4.equipment_damage_published_v4.is_empty() || has_publication(state, id) {
        return Err("equipment damage publication replayed".into());
    }
    let (_, departure, snapshot) = facts(state, item.v4.ability_source_contract.unwrap())?;
    let public_snapshot = match (departure, snapshot) {
        (None, Some(snapshot)) => snapshot,
        (Some(index), None) => self::departure(state, index)?.snapshot.clone(),
        _ => return Err("equipment publication lost original source snapshot".into()),
    };
    let index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "equipment publication overflow")?;
    let item = state
        .stack
        .iter_mut()
        .find(|i| i.v4.stack_item_id == id)
        .unwrap();
    item.v4.equipment_damage_published_v4 = crate::state::EquipmentDamagePublishedV4(Some(index));
    let item = Box::new(item.clone());
    state
        .engine
        .event_history
        .push(CommittedEvent::EquipmentDamagePublishedV4 {
            publication: Box::new(PublicationV4 {
                index,
                item,
                public_snapshot,
            }),
        });
    Ok(())
}
fn publication(state: &GameState, id: StackItemId) -> Result<&PublicationV4, String> {
    let rows = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            CommittedEvent::EquipmentDamagePublishedV4 { publication }
                if publication.item.v4.stack_item_id == id =>
            {
                Some((index, publication.as_ref()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(index, p)] = rows.as_slice() else {
        return Err("equipment damage lost unique publication".into());
    };
    if p.index as usize != *index
        || p.item.v4.equipment_damage_published_v4.0 != Some(p.index)
        || !p.item.v4.crystal_prevention_started_v1.is_empty()
    {
        return Err("equipment publication changed original row/index".into());
    }
    metadata(state, &p.item)?;
    let source = p.item.v4.ability_source_contract.unwrap();
    if p.public_snapshot.card_def != source.card_def
        || p.public_snapshot.zone != Zone::Battlefield
        || p.public_snapshot.owner != source.owner
        || p.public_snapshot.zone_change_count != source.zone_change_count
    {
        return Err("equipment publication lost original public source".into());
    }
    Ok(p)
}
pub(crate) fn published_root(state: &GameState, id: StackItemId) -> Result<EffectOp, String> {
    printed_root(&publication(state, id)?.item)
}
pub(crate) fn has_publication(state: &GameState, id: StackItemId) -> bool {
    state.engine.event_history.iter().any(|e| {
        matches!(e,CommittedEvent::EquipmentDamagePublishedV4 {publication}
        if publication.item.v4.stack_item_id==id)
    })
}
pub(crate) fn tracked(state: &GameState, item: &StackItem) -> bool {
    selected(item)
        || !item.v4.equipment_damage_published_v4.is_empty()
        || has_publication(state, item.v4.stack_item_id)
}
pub(crate) fn publication_matches(state: &GameState, item: &StackItem, saved: &StackItem) -> bool {
    if item.v4.crystal_prevention_started_v1.is_empty()
        && state.engine.event_history.iter().any(|e| {
            matches!(e,CommittedEvent::CrystalPreventionBoundaryV1 {boundary}
            if boundary.item.v4.stack_item_id==item.v4.stack_item_id)
        })
    {
        return false;
    }
    crate::effect::crystal_prevention_v1::source_power_payment_matches(state, item, saved)
}
pub(crate) fn validate_published_metadata(
    state: &GameState,
    item: &StackItem,
) -> Result<(), String> {
    let p = publication(state, item.v4.stack_item_id)?;
    if !publication_matches(state, item, &p.item) {
        return Err("equipment damage changed full published row".into());
    }
    Ok(())
}
fn closed_count(state: &GameState, id: StackItemId) -> usize {
    state
        .engine
        .event_history
        .iter()
        .filter(|e| {
            matches!(e,CommittedEvent::EquipmentDamageClosedV4 {closed}
        if closed.item.v4.stack_item_id==id)
        })
        .count()
}
pub(crate) fn validate_item(state: &GameState, item: &StackItem) -> Result<(), String> {
    if !tracked(state, item) {
        return Ok(());
    }
    validate_published_metadata(state, item)?;
    if state
        .stack
        .iter()
        .filter(|i| i.v4.stack_item_id == item.v4.stack_item_id)
        .count()
        != 1
        || closed_count(state, item.v4.stack_item_id) != 0
    {
        return Err("equipment damage lost mandatory live owner".into());
    }
    Ok(())
}
pub(crate) fn acquire_owner(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<OwnerV4>, String> {
    if !tracked(state, item) {
        return Ok(None);
    }
    validate_item(state, item)?;
    if !state.stack.iter().any(|i| i == item) {
        return Err("equipment source loan has no actual complete row".into());
    }
    Ok(Some(OwnerV4 {
        item: item.clone(),
        publication: publication(state, item.v4.stack_item_id)?.index,
    }))
}
pub(crate) fn counter_owner(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<OwnerV4>, String> {
    if tracked(state, item) && !item.v4.crystal_prevention_started_v1.is_empty() {
        return Err("resolving equipment damage cannot be countered through public removal".into());
    }
    acquire_owner(state, item)
}
fn same_original(state: &GameState, source: AbilitySourceContractV4) -> bool {
    state.objects.try_get(source.source).is_some_and(|o| {
        o.zone == Zone::Battlefield
            && o.owner == source.owner
            && o.zone_change_count == source.zone_change_count
    })
}
pub(crate) fn before_departure(state: &mut GameState, id: ObjectId) {
    before_departure_with_waiting(state, id, &[])
}
fn before_departure_with_waiting(
    state: &mut GameState,
    id: ObjectId,
    waiting: &[crate::trigger::PendingTrigger],
) {
    if !enabled() {
        return;
    }
    let live = state.objects.get(id);
    if live.zone != Zone::Battlefield {
        return;
    }
    let same = |source: AbilitySourceContractV4| {
        source.source == id
            && source.owner == live.owner
            && source.zone_change_count == live.zone_change_count
            && source.zone == Zone::Battlefield
    };
    let pending_grant = |p: &crate::trigger::PendingTrigger| {
        p.source_contract.is_some_and(same)
            && p.granted_by.is_some_and(|grant| {
                CARD_DEFS
                    .get(usize::from(grant.card_def))
                    .and_then(|d| d.equipment)
                    .is_some_and(|e| e.noncreature_spell_damage_to_each_opponent > 0)
            })
    };
    let owned = state
        .stack
        .iter()
        .any(|i| tracked(state, i) && i.v4.ability_source_contract.is_some_and(same))
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
                    .filter_map(|s| s.resolution_play.as_ref())
                    .flat_map(|r| r.deferred_triggers.iter()),
            )
            .chain(waiting.iter())
            .any(pending_grant);
    if !owned {
        return;
    }
    if state.engine.event_history.iter().enumerate().any(|(index,event)|matches!(event,
        CommittedEvent::EquipmentDamageDepartureV4 {departure} if departure.facts.source.source==id
            && departure.snapshot.zone_change_count==live.zone_change_count
            && !state.engine.event_history[index+1..].iter().any(|e|matches!(e,CommittedEvent::ZoneChange {object,..} if *object==id))))
    {return;}
    let index = u32::try_from(state.engine.event_history.len())
        .expect("bounded equipment departure history");
    let snapshot = live.clone();
    let facts = crate::source_power_activation_v1::capture_damage_source(state, id);
    state
        .engine
        .event_history
        .push(CommittedEvent::EquipmentDamageDepartureV4 {
            departure: Box::new(DepartureV4 {
                index,
                snapshot,
                facts,
            }),
        });
}
pub(crate) fn before_protected_departures(
    state: &mut GameState,
    objects: &[ObjectId],
    waiting: &[crate::trigger::PendingTrigger],
) {
    for &id in objects {
        before_departure_with_waiting(state, id, waiting);
    }
}
pub(crate) fn before_batch_departures(state: &mut GameState, events: &[ProposedEvent]) {
    for event in events {
        if let ProposedEvent::ZoneChange(z) = event {
            if state
                .objects
                .try_get(z.object)
                .is_some_and(|o| o.zone == Zone::Battlefield)
                && z.to_zone != Zone::Battlefield
            {
                before_departure(state, z.object);
            }
        }
    }
}
fn departure(state: &GameState, index: u32) -> Result<&DepartureV4, String> {
    let Some(CommittedEvent::EquipmentDamageDepartureV4 { departure: d }) =
        state.engine.event_history.get(index as usize)
    else {
        return Err("equipment source lost departure".into());
    };
    let s = d.facts.source;
    if d.index != index
        || d.snapshot.zone != Zone::Battlefield
        || s.zone != Zone::Battlefield
        || d.snapshot.card_def != s.card_def
        || d.snapshot.owner != s.owner
        || d.snapshot.controller != s.controller
        || d.snapshot.zone_change_count != s.zone_change_count
        || !state.engine.event_history[index as usize + 1..]
            .iter()
            .any(|e| {
                matches!(e,
            CommittedEvent::ZoneChange {object,from:Zone::Battlefield,controller_before,..}
                if *object==s.source && *controller_before==s.controller)
            })
    {
        return Err("equipment departure lost actual source incarnation".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, s)?;
    Ok(d)
}
fn facts(
    state: &GameState,
    source: AbilitySourceContractV4,
) -> Result<(HistoricalDamageSourceV1, Option<u32>, Option<GameObject>), String> {
    if same_original(state, source) {
        return Ok((
            crate::source_power_activation_v1::capture_damage_source(state, source.source),
            None,
            Some(state.objects.get(source.source).clone()),
        ));
    }
    let rows = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            CommittedEvent::EquipmentDamageDepartureV4 { departure: d }
                if d.facts.source.source == source.source
                    && d.facts.source.owner == source.owner
                    && d.facts.source.zone_change_count == source.zone_change_count =>
            {
                Some(d.as_ref())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [d] = rows.as_slice() else {
        return Err("equipment source lost unique original departure facts".into());
    };
    departure(state, d.index)?;
    Ok((d.facts, Some(d.index), None))
}
pub(crate) fn prepare_owned(
    state: &mut GameState,
    owner: &OwnerV4,
    ctx: &ExecCtx,
    op: &EffectOp,
) -> Result<(), String> {
    if *ctx != context(&owner.item)
        || *op != printed_root(&owner.item)?
        || state
            .stack
            .iter()
            .any(|i| i.v4.stack_item_id == owner.item.v4.stack_item_id)
        || state.engine.event_history.iter().any(|e| {
            matches!(e,CommittedEvent::EquipmentDamagePreparedV4 {prepared}
            if prepared.publication==owner.publication)
        })
    {
        return Err("equipment source preparation changed or replayed".into());
    }
    let (facts, departure, snapshot) =
        facts(state, owner.item.v4.ability_source_contract.unwrap())?;
    let index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "equipment preparation overflow")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::EquipmentDamagePreparedV4 {
            prepared: Box::new(PreparationV4 {
                index,
                publication: owner.publication,
                ctx: ctx.clone(),
                facts,
                departure,
                snapshot,
            }),
        });
    Ok(())
}
fn prepared(state: &GameState, id: StackItemId) -> Result<&PreparationV4, String> {
    let p = publication(state, id)?;
    let rows = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            CommittedEvent::EquipmentDamagePreparedV4 { prepared }
                if prepared.publication == p.index =>
            {
                Some((index, prepared.as_ref()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(index, r)] = rows.as_slice() else {
        return Err("equipment damage lost unique actual source preparation".into());
    };
    let source = p.item.v4.ability_source_contract.unwrap();
    if r.index as usize != *index
        || r.index <= p.index
        || r.ctx != context(&p.item)
        || r.facts.source.source != source.source
        || r.facts.source.owner != source.owner
        || r.facts.source.zone_change_count != source.zone_change_count
    {
        return Err("equipment prepared source changed".into());
    }
    match (r.departure, r.snapshot.as_ref()) {
        (Some(index), None) => {
            let d = departure(state, index)?;
            if index>=r.index || d.facts!=r.facts || !state.engine.event_history[index as usize+1..r.index as usize]
                .iter().any(|e|matches!(e,CommittedEvent::ZoneChange {object,from:Zone::Battlefield,..} if *object==source.source)) {
                return Err("equipment preparation preceded source departure".into());
            }
        }
        (None, Some(snapshot))
            if snapshot.zone == Zone::Battlefield
                && snapshot.owner == source.owner
                && snapshot.zone_change_count == source.zone_change_count
                && snapshot.card_def == r.facts.source.card_def
                && snapshot.controller == r.facts.source.controller => {}
        _ => return Err("equipment preparation has no original live/departure facts".into()),
    }
    Ok(r)
}
pub(crate) fn damage_source(
    state: &GameState,
    source: ObjectId,
    id: StackItemId,
) -> Result<HistoricalDamageSourceV1, String> {
    let r = prepared(state, id)?;
    if r.ctx.source != source {
        return Err("equipment historical packet changed source".into());
    }
    Ok(r.facts)
}
pub(crate) fn record_countered(
    state: &mut GameState,
    owner: Option<OwnerV4>,
) -> Result<(), String> {
    let Some(owner) = owner else {
        return Ok(());
    };
    close(state, &owner, &owner.item, OutcomeV4::Countered, None)
}
pub(crate) fn finish_owned(
    state: &mut GameState,
    owner: &OwnerV4,
    item: &StackItem,
) -> Result<(), String> {
    let boundary = state
        .engine
        .event_history
        .iter()
        .rev()
        .find_map(|e| match e {
            CommittedEvent::CrystalPreventionBoundaryV1 { boundary }
                if boundary.item.v4.stack_item_id == item.v4.stack_item_id =>
            {
                Some(boundary.history_index)
            }
            _ => None,
        })
        .ok_or("equipment completion lost actual execution boundary")?;
    close(state, owner, item, OutcomeV4::Resolved, Some(boundary))
}
fn close(
    state: &mut GameState,
    owner: &OwnerV4,
    item: &StackItem,
    outcome: OutcomeV4,
    boundary: Option<u32>,
) -> Result<(), String> {
    if state
        .stack
        .iter()
        .any(|i| i.v4.stack_item_id == item.v4.stack_item_id)
        || !publication_matches(
            state,
            item,
            &publication(state, item.v4.stack_item_id)?.item,
        )
        || context(item) != context(&owner.item)
        || closed_count(state, item.v4.stack_item_id) != 0
    {
        return Err("equipment completion lost genuine removed owner".into());
    }
    let index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "equipment completion overflow")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::EquipmentDamageClosedV4 {
            closed: Box::new(ClosedV4 {
                index,
                publication: owner.publication,
                item: Box::new(item.clone()),
                outcome,
                boundary,
            }),
        });
    validate_state(state)
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if state.engine.event_log.iter().any(|e| {
        matches!(
            e,
            CommittedEvent::EquipmentDamagePublishedV4 { .. }
                | CommittedEvent::EquipmentDamageDepartureV4 { .. }
                | CommittedEvent::EquipmentDamagePreparedV4 { .. }
                | CommittedEvent::EquipmentDamageClosedV4 { .. }
        )
    }) {
        return Err("private equipment source authority entered transient log".into());
    }
    for item in &state.stack {
        validate_item(state, item)?;
    }
    for (index, event) in state.engine.event_history.iter().enumerate() {
        match event {
            CommittedEvent::EquipmentDamageDepartureV4 { departure: d } => {
                if d.index as usize != index {
                    return Err("equipment departure index changed".into());
                }
                departure(state, d.index)?;
            }
            CommittedEvent::EquipmentDamagePublishedV4 { publication: p } => {
                let p = publication(state, p.item.v4.stack_item_id)?;
                let live = state
                    .stack
                    .iter()
                    .filter(|i| i.v4.stack_item_id == p.item.v4.stack_item_id)
                    .count();
                if !matches!(
                    (live, closed_count(state, p.item.v4.stack_item_id)),
                    (1, 0) | (0, 1)
                ) {
                    return Err("equipment damage owner erased or reintroduced".into());
                }
            }
            CommittedEvent::EquipmentDamagePreparedV4 { prepared: r } => {
                prepared(
                    state,
                    r.ctx
                        .stack_item_id
                        .ok_or("equipment preparation lost stack")?,
                )?;
                if r.index as usize != index {
                    return Err("equipment preparation index changed".into());
                }
            }
            CommittedEvent::EquipmentDamageClosedV4 { closed: c } => {
                let p = publication(state, c.item.v4.stack_item_id)?;
                if c.index as usize != index
                    || c.publication != p.index
                    || c.index <= p.index
                    || !publication_matches(state, &c.item, &p.item)
                {
                    return Err("equipment closure changed publication".into());
                }
                match (c.outcome,c.boundary) {
                    (OutcomeV4::Countered,None)=>{
                        if c.item!=p.item || state.engine.event_history.iter().any(|e|matches!(e,
                            CommittedEvent::EquipmentDamagePreparedV4 {prepared} if prepared.publication==p.index)) {
                            return Err("equipment counter hides execution".into());
                        }
                    }
                    (OutcomeV4::Resolved,Some(at))=>{
                        let Some(CommittedEvent::CrystalPreventionBoundaryV1 {boundary:b})=state.engine.event_history.get(at as usize)
                            else{return Err("equipment closure lost real boundary".into());};
                        let Some(CommittedEvent::CrystalExecutionV1 {execution:e})=at.checked_sub(1).and_then(|i|state.engine.event_history.get(i as usize))
                            else{return Err("equipment closure lost independent actual execution".into());};
                        if at>=c.index || b.item!=c.item || b.pending.is_some() || b.answer.is_some()
                            || e.item!=c.item || e.after.is_some() || e.history_index+1!=b.history_index {
                            return Err("equipment completion changed actual final execution".into());
                        }
                        let r=prepared(state,c.item.v4.stack_item_id)?;
                        if r.index>=at {return Err("equipment closed before actual source preparation".into());}
                    }
                    _=>return Err("equipment closure changed outcome".into()),
                }
            }
            _ => {}
        }
    }
    Ok(())
}
pub(crate) fn requires_successor(state: &GameState) -> bool {
    state.stack.iter().any(|i| tracked(state, i))
        || state.engine.event_history.iter().any(|e| {
            matches!(
                e,
                CommittedEvent::EquipmentDamagePublishedV4 { .. }
                    | CommittedEvent::EquipmentDamagePreparedV4 { .. }
                    | CommittedEvent::EquipmentDamageClosedV4 { .. }
            )
        })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicSourceV4 {
    pub stack_ordinal: u32,
    pub source: crate::rl::CardStableRefV1,
    pub equipment: crate::rl::CardStableRefV1,
    pub ability_controller: crate::rl::PlayerSeatV1,
    pub damage_controller: crate::rl::PlayerSeatV1,
    pub historical: bool,
    pub color_mask: u8,
    pub lifelink: bool,
    pub deathtouch: bool,
}
pub(crate) fn public_sources(state: &GameState) -> Result<Vec<PublicSourceV4>, String> {
    validate_state(state)?;
    state
        .stack
        .iter()
        .enumerate()
        .filter(|(_, item)| tracked(state, item))
        .map(|(ordinal, item)| {
            let p = publication(state, item.v4.stack_item_id)?;
            let source = p.item.v4.ability_source_contract.unwrap();
            let grant = p.item.v4.granted_by.unwrap();
            let (facts, departure, snapshot) = if state.engine.event_history.iter().any(|e| {
                matches!(e,
            CommittedEvent::EquipmentDamagePreparedV4 {prepared} if prepared.publication==p.index)
            }) {
                let r = prepared(state, item.v4.stack_item_id)?;
                (r.facts, r.departure, r.snapshot.clone())
            } else {
                facts(state, source)?
            };
            let snapshot = match (departure, snapshot) {
                (None, Some(snapshot)) => snapshot,
                (Some(index), None) => self::departure(state, index)?.snapshot.clone(),
                _ => return Err("equipment public source lost actual snapshot".into()),
            };
            if p.public_snapshot.card_def != source.card_def
                || p.public_snapshot.v4.face_down_v1.is_some()
                || p.public_snapshot.spell_copy_origin.is_some()
                || facts.source.card_def != source.card_def
                || snapshot.v4.face_down_v1.is_some()
                || snapshot.spell_copy_origin.is_some()
            {
                return Err(
                    "Crystal V22 cannot expose copied or hidden equipment damage identity".into(),
                );
            }
            Ok(PublicSourceV4 {
                stack_ordinal: u32::try_from(ordinal)
                    .map_err(|_| "equipment public ordinal overflow")?,
                source: crate::rl::CardStableRefV1 {
                    arena_id: source.source.0,
                    card_db_id: source.card_def,
                    owner: source.owner.into(),
                    controller: source.controller.into(),
                    zone: source.zone,
                    zone_change_count: source.zone_change_count,
                },
                equipment: crate::rl::CardStableRefV1 {
                    arena_id: grant.source.0,
                    card_db_id: grant.card_def,
                    owner: grant.owner.into(),
                    controller: grant.controller.into(),
                    zone: grant.zone,
                    zone_change_count: grant.zone_change_count,
                },
                ability_controller: source.controller.into(),
                damage_controller: facts.source.controller.into(),
                historical: departure.is_some(),
                color_mask: facts.color_mask,
                lifelink: facts.lifelink,
                deathtouch: facts.deathtouch,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "equipment_damage_source_v4_tests.rs"]
mod tests;
