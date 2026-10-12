//! Exact original-incarnation damage for the printed Gorehorn trigger.
use crate::card_def::CARD_DEFS;
use crate::effect::{EffectOp, ExecCtx};
use crate::event::{CommittedEvent, HistoricalDamageSourceV1};
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::state::{
    AbilitySourceContractV4, AttackDeclarationTurnV1, GameObject, GameState, StackItem,
    StackItemKind, Zone,
};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DepartureV1 {
    pub index: u32,
    pub snapshot: GameObject,
    pub facts: HistoricalDamageSourceV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PublicationV1 {
    pub index: u32,
    pub item: Box<StackItem>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PreparedV1 {
    pub index: u32,
    pub publication: u32,
    pub ctx: ExecCtx,
    pub facts: HistoricalDamageSourceV1,
    pub departure: Option<u32>,
    pub snapshot: Option<GameObject>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DamageOperationV1 {
    pub index: u32,
    pub preparation: u32,
    pub replacements_before: Vec<crate::event::ActiveReplacement>,
    pub output: Option<DamageResultV1>,
    pub damage_index: Option<u32>,
    pub turn: u32,
    pub active: PlayerId,
    pub prevention_overridden: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DamageResultV1 {
    pub source: ObjectId,
    pub target: crate::state::Target,
    pub amount: i32,
    pub touched: Vec<u32>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OutcomeV1 {
    Resolved,
    Countered,
    NoLegalTarget,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperationV1 {
    pub index: u32,
    pub publication: u32,
    pub item: Box<StackItem>,
    pub outcome: OutcomeV1,
    pub turn: u32,
    pub active: PlayerId,
    pub attacked: Option<AttackDeclarationTurnV1>,
    pub raid_result: Option<bool>,
    pub damage_operation: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GorehornSourceV1 {
    pub stack_ordinal: u32,
    pub printed_card_db_id: u16,
    pub ability_controller: crate::rl::PlayerSeatV1,
    pub damage_controller: crate::rl::PlayerSeatV1,
    pub historical: bool,
    pub color_mask: u8,
    pub lifelink: bool,
    pub deathtouch: bool,
}
pub(crate) fn public_sources(state: &GameState) -> Result<Vec<GorehornSourceV1>, String> {
    validate_state(state)?;
    state
        .stack
        .iter()
        .enumerate()
        .filter(|(_, item)| selected(item))
        .map(|(ordinal, item)| {
            let source = item
                .v4
                .ability_source_contract
                .ok_or("Gorehorn public original source missing")?;
            let facts = source_facts(state, source)?;
            let historical = state.objects.get(source.source).zone != Zone::Battlefield
                || state.objects.get(source.source).zone_change_count != source.zone_change_count;
            let snapshot = if historical {
                state
                    .engine
                    .event_history
                    .iter()
                    .find_map(|event| match event {
                        CommittedEvent::GorehornDepartureV1 { departure: d }
                            if d.facts.source.source == source.source
                                && d.facts.source.zone_change_count == source.zone_change_count =>
                        {
                            Some(&d.snapshot)
                        }
                        _ => None,
                    })
                    .ok_or("Gorehorn public departure missing")?
            } else {
                state.objects.get(source.source)
            };
            if facts.source.card_def != source.card_def
                || snapshot.v4.face_down_v1.is_some()
                || snapshot.spell_copy_origin.is_some()
            {
                return Err(
                    "RaidEntry V21 cannot represent copied or hidden Gorehorn damage identity"
                        .into(),
                );
            }
            Ok(GorehornSourceV1 {
                stack_ordinal: u32::try_from(ordinal)
                    .map_err(|_| "Gorehorn public ordinal overflow")?,
                printed_card_db_id: source.card_def,
                ability_controller: source.controller.into(),
                damage_controller: facts.source.controller.into(),
                historical,
                color_mask: facts.color_mask,
                lifelink: facts.lifelink,
                deathtouch: facts.deathtouch,
            })
        })
        .collect()
}

pub(crate) struct OwnerV1 {
    item: StackItem,
    publication: u32,
}
fn enabled() -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))
}
pub(crate) fn owns_definition(id: u16) -> bool {
    enabled()
        && CARD_DEFS
            .get(usize::from(id))
            .is_some_and(|d| d.name == "Gorehorn Raider" && d.is_executable())
}
pub(crate) fn selected(item: &StackItem) -> bool {
    enabled()
        && item.kind == StackItemKind::TriggeredAbility
        && item.v4.granted_by.is_none()
        && item.inline_effect.as_ref() == Some(&crate::trigger::gorehorn_raider_effect())
        && item
            .v4
            .ability_source_contract
            .is_some_and(|s| owns_definition(s.card_def))
}
pub(crate) fn owns_context(state: &GameState, ctx: &ExecCtx) -> bool {
    ctx.stack_item_id.is_some_and(|id| {
        has_publication(state, id)
            || state
                .stack
                .iter()
                .any(|item| item.v4.stack_item_id == id && selected(item))
    })
}
fn tracked(state: &GameState, item: &StackItem) -> bool {
    selected(item)
        || item.v4.gorehorn_published_v1.0.is_some()
        || state.engine.event_history.iter().any(|e| {
            matches!(e,CommittedEvent::GorehornPublishedV1{publication:p}
  if p.item.v4.stack_item_id==item.v4.stack_item_id)
        })
}
fn is_record(e: &CommittedEvent) -> bool {
    matches!(
        e,
        CommittedEvent::GorehornDepartureV1 { .. }
            | CommittedEvent::GorehornPublishedV1 { .. }
            | CommittedEvent::GorehornPreparedV1 { .. }
            | CommittedEvent::GorehornDamageOperationV1 { .. }
            | CommittedEvent::GorehornOperationV1 { .. }
    )
}

pub(crate) fn before_batch_departures(
    state: &mut GameState,
    events: &[crate::event::ProposedEvent],
) {
    if !enabled() {
        return;
    }
    let mut seen = Vec::new();
    for event in events {
        if let crate::event::ProposedEvent::ZoneChange(change) = event {
            if change.to_zone != Zone::Battlefield
                && state
                    .objects
                    .try_get(change.object)
                    .is_some_and(|o| o.zone == Zone::Battlefield)
                && !seen.contains(&change.object)
            {
                seen.push(change.object);
                before_departure(state, change.object);
            }
        }
    }
}

pub(crate) fn before_departure(state: &mut GameState, id: ObjectId) {
    if !enabled() {
        return;
    }
    let live = state.objects.get(id);
    let same = |s: AbilitySourceContractV4| {
        s.source == id
            && s.zone == Zone::Battlefield
            && s.zone_change_count == live.zone_change_count
            && owns_definition(s.card_def)
    };
    let owned = owns_definition(live.card_def)
        || state
            .stack
            .iter()
            .any(|i| i.v4.ability_source_contract.is_some_and(same))
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
            .any(|p| p.source_contract.is_some_and(same));
    if !owned {
        return;
    }
    // A simultaneous batch froze this incarnation before any member moved.
    if state
        .engine
        .event_history
        .iter()
        .enumerate()
        .any(|(index, event)| {
            matches!(event,
        CommittedEvent::GorehornDepartureV1 { departure } if departure.facts.source.source==id
        && departure.facts.source.zone_change_count==live.zone_change_count
        && !state.engine.event_history[index+1..].iter().any(|later| matches!(later,
            CommittedEvent::ZoneChange { object, .. } if *object==id)))
        })
    {
        return;
    }
    let index = u32::try_from(state.engine.event_history.len())
        .expect("bounded Gorehorn departure history");
    let snapshot = live.clone();
    let facts = crate::source_power_activation_v1::capture_damage_source(state, id);
    state
        .engine
        .event_history
        .push(CommittedEvent::GorehornDepartureV1 {
            departure: Box::new(DepartureV1 {
                index,
                snapshot,
                facts,
            }),
        });
}
pub(crate) fn publish(state: &mut GameState, id: StackItemId) -> Result<(), String> {
    let row = state
        .stack
        .iter()
        .find(|i| i.v4.stack_item_id == id)
        .ok_or("Gorehorn publication lacks actual live row")?;
    if !selected(row) {
        return Ok(());
    }
    let source = row
        .v4
        .ability_source_contract
        .ok_or("Gorehorn publication lost source")?;
    crate::raid_entry_v1::validate_source_for_program(
        state,
        &crate::trigger::gorehorn_raider_effect(),
        Some(source),
    )?;
    if row.inline_effect.as_ref()!=Some(&crate::trigger::gorehorn_raider_effect())
  || row.controller!=source.controller || row.targets.len()!=1 || row.v4.target_contracts.len()!=1
  || row.v4.granted_by.is_some() || !row.v4.gorehorn_published_v1.is_empty()
  || state.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::GorehornPublishedV1{publication:p} if p.item.v4.stack_item_id==id)) {
  return Err("Gorehorn publication changed printed root, target or original controller".into());
 }
    let index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "Gorehorn publication overflow")?;
    let row = state
        .stack
        .iter_mut()
        .find(|i| i.v4.stack_item_id == id)
        .unwrap();
    row.v4.gorehorn_published_v1 = crate::state::GorehornPublishedV1(Some(index));
    let item = Box::new(row.clone());
    state
        .engine
        .event_history
        .push(CommittedEvent::GorehornPublishedV1 {
            publication: Box::new(PublicationV1 { index, item }),
        });
    Ok(())
}
fn publication(state: &GameState, id: StackItemId) -> Result<&PublicationV1, String> {
    let mut rows = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            CommittedEvent::GorehornPublishedV1 { publication: p }
                if p.item.v4.stack_item_id == id =>
            {
                Some((i, p.as_ref()))
            }
            _ => None,
        });
    let (i, p) = rows
        .next()
        .ok_or("Gorehorn lost mandatory original publication")?;
    let s = p
        .item
        .v4
        .ability_source_contract
        .ok_or("Gorehorn publication source missing")?;
    if rows.next().is_some()
        || p.index as usize != i
        || !selected(&p.item)
        || p.item.v4.gorehorn_published_v1.0 != Some(p.index)
        || p.item.source != s.source
        || p.item.controller != s.controller
        || p.item.inline_effect.as_ref() != Some(&crate::trigger::gorehorn_raider_effect())
        || p.item.v4.target_spec != Some(crate::card_def::TargetSpec::AnyTarget)
        || p.item.targets.len() != 1
        || p.item.v4.target_contracts.len() != 1
        || p.item.v4.target_contracts.first().map(|c| c.target()) != p.item.targets.first().copied()
        || p.item.v4.granted_by.is_some()
    {
        return Err("Gorehorn original published row changed".into());
    }
    crate::raid_entry_v1::validate_source_for_program(
        state,
        p.item.inline_effect.as_ref().unwrap(),
        Some(s),
    )?;
    Ok(p)
}
pub(crate) fn publication_matches(_state: &GameState, item: &StackItem, saved: &StackItem) -> bool {
    item == saved
}
pub(crate) fn validate_item(state: &GameState, item: &StackItem) -> Result<(), String> {
    if !tracked(state, item) {
        return Ok(());
    }
    let p = publication(state, item.v4.stack_item_id)?;
    if !publication_matches(state,item,&p.item) || state.stack.iter().filter(|i|i.v4.stack_item_id==item.v4.stack_item_id).count()!=1
  || state.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::GorehornOperationV1{operation:o} if o.publication==p.index)) {
  return Err("Gorehorn lacks exact unclosed published owner".into());
 }
    Ok(())
}
pub(crate) fn acquire_owner(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<OwnerV1>, String> {
    if !tracked(state, item) {
        return Ok(None);
    }
    validate_item(state, item)?;
    if !state.stack.iter().any(|i| i == item) {
        return Err("Gorehorn owner lacks actual complete live row".into());
    }
    Ok(Some(OwnerV1 {
        item: item.clone(),
        publication: publication(state, item.v4.stack_item_id)?.index,
    }))
}
fn context(item: &StackItem) -> ExecCtx {
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
fn departure(state: &GameState, index: u32) -> Result<&DepartureV1, String> {
    let Some(CommittedEvent::GorehornDepartureV1 { departure: d }) =
        state.engine.event_history.get(index as usize)
    else {
        return Err("Gorehorn departure missing".into());
    };
    if d.index != index
        || d.snapshot.zone != Zone::Battlefield
        || d.facts.source.zone != Zone::Battlefield
        || d.snapshot.card_def != d.facts.source.card_def
        || d.snapshot.owner != d.facts.source.owner
        || d.snapshot.controller != d.facts.source.controller
        || d.snapshot.zone_change_count != d.facts.source.zone_change_count
        || !state.engine.event_history[index as usize + 1..]
            .iter()
            .any(|e| {
                matches!(e,
   CommittedEvent::ZoneChange{object,from:Zone::Battlefield,controller_before,..}
    if *object==d.facts.source.source && *controller_before==d.facts.source.controller)
            })
    {
        return Err("Gorehorn departure lost actual original-incarnation zone change".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, d.facts.source)?;
    Ok(d)
}
pub(crate) fn source_facts(
    state: &GameState,
    s: AbilitySourceContractV4,
) -> Result<HistoricalDamageSourceV1, String> {
    if state.objects.try_get(s.source).is_some_and(|o| {
        o.zone == Zone::Battlefield
            && o.owner == s.owner
            && o.zone_change_count == s.zone_change_count
    }) {
        return Ok(crate::source_power_activation_v1::capture_damage_source(
            state, s.source,
        ));
    }
    let rows = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            CommittedEvent::GorehornDepartureV1 { departure: d }
                if d.facts.source.source == s.source
                    && d.facts.source.zone_change_count == s.zone_change_count =>
            {
                Some(d.as_ref())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [d] = rows.as_slice() else {
        return Err("Gorehorn original source lost unique departure LKI".into());
    };
    Ok(departure(state, d.index)?.facts)
}

pub(crate) fn legal_targets(
    state: &GameState,
    source: ObjectId,
    card_def: u16,
    generation: u32,
    controller: PlayerId,
    chosen: &[crate::state::Target],
) -> Result<Vec<crate::state::Target>, String> {
    let object = state
        .objects
        .try_get(source)
        .ok_or("Gorehorn target source missing")?;
    let facts = source_facts(
        state,
        AbilitySourceContractV4 {
            source,
            card_def,
            owner: object.owner,
            controller,
            zone: Zone::Battlefield,
            zone_change_count: generation,
            attached_to: None,
        },
    )?;
    Ok(crate::engine::legal_targets_for_controller(
        crate::card_def::TargetSpec::AnyTarget,
        chosen,
        controller,
        state,
    )
    .into_iter()
    .filter(|target| match target {
        crate::state::Target::Object(target) => {
            !crate::source_power_activation_v1::protected(state, facts, *target)
                && !crate::standard_legends_v1::hexproof_from_historical_color(
                    state,
                    *target,
                    facts.color_mask,
                    controller,
                )
        }
        _ => true,
    })
    .collect())
}

fn prepared(state: &GameState, id: StackItemId) -> Result<&PreparedV1, String> {
    let p = publication(state, id)?;
    let s = p.item.v4.ability_source_contract.unwrap();
    let mut rows = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            CommittedEvent::GorehornPreparedV1 { prepared: r } if r.publication == p.index => {
                Some((i, r.as_ref()))
            }
            _ => None,
        });
    let (i, r) = rows
        .next()
        .ok_or("Gorehorn lost actual prepared damage facts")?;
    if rows.next().is_some()
        || r.index as usize != i
        || r.index <= p.index
        || r.ctx != context(&p.item)
        || r.facts.source.source != s.source
        || r.facts.source.owner != s.owner
        || r.facts.source.zone_change_count != s.zone_change_count
    {
        return Err("Gorehorn prepared source/context differs from publication".into());
    }
    match (r.departure, r.snapshot.as_ref()) {
        (Some(index), None) => {
            let d = departure(state, index)?;
            if d.facts != r.facts
                || index >= r.index
                || !state.engine.event_history[index as usize + 1..i]
                    .iter()
                    .any(|e| {
                        matches!(e,
    CommittedEvent::ZoneChange{object,from:Zone::Battlefield,..} if *object==s.source)
                    })
            {
                return Err("Gorehorn preparation preceded departure".into());
            }
        }
        (None, Some(snapshot))
            if snapshot.zone == Zone::Battlefield
                && snapshot.owner == s.owner
                && snapshot.zone_change_count == s.zone_change_count
                && snapshot.card_def == r.facts.source.card_def
                && snapshot.controller == r.facts.source.controller => {}
        _ => {
            return Err(
                "Gorehorn damage has neither original live facts nor exact departure".into(),
            )
        }
    }
    Ok(r)
}
pub(crate) fn has_publication(state: &GameState, id: StackItemId) -> bool {
    state.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::GorehornPublishedV1{publication:p} if p.item.v4.stack_item_id==id))
}
pub(crate) fn damage_source(
    state: &GameState,
    id: StackItemId,
    source: ObjectId,
) -> Result<HistoricalDamageSourceV1, String> {
    let p = prepared(state, id)?;
    if p.ctx.source != source {
        return Err("Gorehorn historical damage source changed".into());
    }
    Ok(p.facts)
}
impl OwnerV1 {
    pub(crate) fn permits_damage(
        &self,
        state: &GameState,
        d: &crate::event::DamageProposed,
    ) -> bool {
        state
            .stack
            .iter()
            .all(|i| i.v4.stack_item_id != self.item.v4.stack_item_id)
            && d.source == self.item.source
            && d.target == self.item.targets[0]
            && d.amount == 2
            && !d.is_combat
            && d.historical_source == Some(self.item.v4.stack_item_id)
            && prepared(state, self.item.v4.stack_item_id).is_ok()
    }
}
pub(crate) fn execute_owned(
    state: &mut GameState,
    op: &EffectOp,
    ctx: &ExecCtx,
    owner: &OwnerV1,
) -> Result<(), String> {
    if *op != crate::trigger::gorehorn_raider_effect()
        || *ctx != context(&owner.item)
        || state
            .stack
            .iter()
            .any(|i| i.v4.stack_item_id == owner.item.v4.stack_item_id)
        || publication(state, owner.item.v4.stack_item_id)?.index != owner.publication
    {
        return Err("Gorehorn execution lost actual removed-row authority".into());
    }
    let met = state.player_attacked_this_turn_v1(ctx.controller);
    let turn = state.turn_key_v1();
    let active = state.active_player;
    let attacked = state.attack_declaration_turn_v1;
    let mut damage_operation = None;
    if met {
        let s = owner.item.v4.ability_source_contract.unwrap();
        let facts = source_facts(state, s)?;
        let live = state.objects.try_get(s.source).filter(|o| {
            o.zone == Zone::Battlefield
                && o.owner == s.owner
                && o.zone_change_count == s.zone_change_count
        });
        let snapshot = live.cloned();
        let departure = if snapshot.is_some() {
            None
        } else {
            Some(
                state
                    .engine
                    .event_history
                    .iter()
                    .find_map(|e| match e {
                        CommittedEvent::GorehornDepartureV1 { departure: d }
                            if d.facts.source.source == s.source
                                && d.facts.source.zone_change_count == s.zone_change_count =>
                        {
                            Some(d.index)
                        }
                        _ => None,
                    })
                    .ok_or("Gorehorn departure index missing")?,
            )
        };
        let index = u32::try_from(state.engine.event_history.len())
            .map_err(|_| "Gorehorn preparation overflow")?;
        state
            .engine
            .event_history
            .push(CommittedEvent::GorehornPreparedV1 {
                prepared: Box::new(PreparedV1 {
                    index,
                    publication: owner.publication,
                    ctx: ctx.clone(),
                    facts,
                    departure,
                    snapshot,
                }),
            });
        let d = crate::event::DamageProposed {
            source: ctx.source,
            target: ctx.targets[0],
            amount: 2,
            touched_by: vec![],
            is_combat: false,
            historical_source: Some(owner.item.v4.stack_item_id),
        };
        damage_operation = Some(crate::event::propose_and_commit_gorehorn_owned(
            state, d, index, owner,
        )?);
    }
    record_operation(
        state,
        owner,
        OutcomeV1::Resolved,
        Some((turn, active, attacked, met)),
        damage_operation,
    )
}
fn record_operation(
    state: &mut GameState,
    owner: &OwnerV1,
    outcome: OutcomeV1,
    raid: Option<(u32, PlayerId, Option<AttackDeclarationTurnV1>, bool)>,
    damage_operation: Option<u32>,
) -> Result<(), String> {
    if state.stack.iter().any(|i|i.v4.stack_item_id==owner.item.v4.stack_item_id) || state.engine.event_history.iter().any(|e|
  matches!(e,CommittedEvent::GorehornOperationV1{operation:o} if o.publication==owner.publication)) {
  return Err("Gorehorn removal ownership changed or replayed".into());
 }
    let (turn, active, attacked, result) = raid.map_or(
        (state.turn_key_v1(), state.active_player, None, None),
        |(t, a, h, r)| (t, a, h, Some(r)),
    );
    let index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "Gorehorn operation overflow")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::GorehornOperationV1 {
            operation: Box::new(OperationV1 {
                index,
                publication: owner.publication,
                item: Box::new(owner.item.clone()),
                outcome,
                turn,
                active,
                attacked,
                raid_result: result,
                damage_operation,
            }),
        });
    Ok(())
}
pub(crate) fn record_removed(
    state: &mut GameState,
    owner: Option<OwnerV1>,
    outcome: OutcomeV1,
) -> Result<(), String> {
    let Some(owner) = owner else {
        return Ok(());
    };
    if outcome == OutcomeV1::Resolved {
        return Err("Gorehorn resolved removal requires actual owned execution".into());
    }
    record_operation(state, &owner, outcome, None, None)
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if state.engine.event_log.iter().any(is_record) {
        return Err("Gorehorn private authority in transient log".into());
    }
    for item in &state.stack {
        validate_item(state, item)?;
    }
    for (index, e) in state.engine.event_history.iter().enumerate() {
        match e {
            CommittedEvent::GorehornDepartureV1 { departure: d } => {
                if d.index as usize != index {
                    return Err("Gorehorn departure index changed".into());
                }
                departure(state, d.index)?;
            }
            CommittedEvent::GorehornPublishedV1 { publication: p } => {
                let p = publication(state, p.item.v4.stack_item_id)?;
                let live = state
                    .stack
                    .iter()
                    .filter(|i| i.v4.stack_item_id == p.item.v4.stack_item_id)
                    .count();
                let closed = state
                    .engine
                    .event_history
                    .iter()
                    .filter_map(|e| match e {
                        CommittedEvent::GorehornOperationV1 { operation: o }
                            if o.publication == p.index =>
                        {
                            Some(o.as_ref())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if !matches!((live, closed.as_slice()), (1, []) | (0, [_])) {
                    return Err("Gorehorn owner erased or reintroduced".into());
                }
            }
            CommittedEvent::GorehornPreparedV1 { prepared: r } => {
                prepared(
                    state,
                    r.ctx
                        .stack_item_id
                        .ok_or("Gorehorn prepared stack missing")?,
                )?;
                if r.index as usize != index
                    || state
                        .engine
                        .event_history
                        .iter()
                        .filter(|e| {
                            matches!(e,
      CommittedEvent::GorehornDamageOperationV1{operation:o} if o.preparation==r.index)
                        })
                        .count()
                        != 1
                {
                    return Err("Gorehorn preparation lost unique damage operation".into());
                }
            }
            CommittedEvent::GorehornDamageOperationV1 { operation: o } => {
                validate_damage_operation(state, index, o)?;
            }
            CommittedEvent::GorehornOperationV1 { operation: o } => {
                let Some(CommittedEvent::GorehornPublishedV1 { publication: p }) =
                    state.engine.event_history.get(o.publication as usize)
                else {
                    return Err("Gorehorn closure lost publication".into());
                };
                if o.index as usize != index || o.index <= p.index || o.item != p.item {
                    return Err("Gorehorn closure changed original complete row".into());
                }
                if o.outcome == OutcomeV1::Resolved {
                    let met = o.attacked.is_some_and(|a| {
                        a.turn == o.turn
                            && a.active_player == o.active
                            && a.active_player == o.item.controller
                    });
                    if o.raid_result != Some(met) || met != o.damage_operation.is_some() {
                        return Err("Gorehorn closure lost actual Raid/damage outcome".into());
                    }
                    if let Some(i) = o.damage_operation {
                        let Some(CommittedEvent::GorehornDamageOperationV1 { operation: d }) =
                            state.engine.event_history.get(i as usize)
                        else {
                            return Err("Gorehorn closure lost independent actual damage".into());
                        };
                        let r = prepared(state, o.item.v4.stack_item_id)?;
                        if d.index >= o.index || d.preparation != r.index {
                            return Err("Gorehorn closed before actual damage".into());
                        }
                    }
                } else if o.raid_result.is_some()
                    || o.damage_operation.is_some()
                    || state.engine.event_history.iter().any(|e| {
                        matches!(e,
     CommittedEvent::GorehornPreparedV1{prepared:r} if r.publication==p.index)
                    })
                {
                    return Err("Gorehorn counter/fizzle closure hides execution".into());
                }
            }
            _ => {}
        }
    }
    Ok(())
}
fn validate_damage_operation(
    state: &GameState,
    index: usize,
    o: &DamageOperationV1,
) -> Result<(), String> {
    let Some(CommittedEvent::GorehornPreparedV1 { prepared: r }) =
        state.engine.event_history.get(o.preparation as usize)
    else {
        return Err("Gorehorn damage operation lost actual preparation".into());
    };
    if o.index as usize != index || o.index <= r.index {
        return Err("Gorehorn damage operation order changed".into());
    }
    let mut expected_amount = 2i32;
    if !o.prevention_overridden {
        for replacement in &o.replacements_before {
            match replacement.kind {
                crate::event::ReplacementEffectKind::PreventNextDamage { target, remaining }
                    if target == r.ctx.targets[0] && remaining > 0 =>
                {
                    expected_amount -= remaining.min(expected_amount)
                }
                crate::event::ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn {
                    color,
                    turn,
                    active_player,
                } if turn == o.turn
                    && active_player == o.active
                    && r.facts.color_mask & crate::card_def::mana_color_mask(color) != 0 =>
                {
                    expected_amount = 0
                }
                _ => {}
            }
            if expected_amount == 0 {
                break;
            }
        }
    }
    if o.output.as_ref().map_or(0, |result| result.amount) != expected_amount {
        return Err(
            "Gorehorn output differs from independently captured replacement outcome".into(),
        );
    }

    match (&o.output, o.damage_index) {
        (Some(result), Some(i))
            if i > r.index
                && i < o.index
                && result.source == r.ctx.source
                && result.target == r.ctx.targets[0]
                && result.amount > 0
                && result.amount <= 2 =>
        {
            if !matches!(state.engine.event_history.get(i as usize),Some(CommittedEvent::Damage{source,target,amount,..})
    if *source==result.source && *target==result.target && *amount==result.amount)
            {
                return Err("Gorehorn lost actual surviving damage".into());
            }
        }
        (None, None) => {
            let mut prevented = 0;
            for replacement in &o.replacements_before {
                match replacement.kind {
                    crate::event::ReplacementEffectKind::PreventNextDamage {
                        target,
                        remaining,
                    } if target == r.ctx.targets[0] => prevented += remaining.max(0),
                    crate::event::ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn {
                        color,
                        turn,
                        active_player,
                    } if turn == o.turn
                        && active_player == o.active
                        && r.facts.color_mask & crate::card_def::mana_color_mask(color) != 0 =>
                    {
                        prevented = 2
                    }
                    _ => {}
                }
            }
            if prevented < 2 {
                return Err("Gorehorn positive damage closed without actual prevention".into());
            }
        }
        _ => return Err("Gorehorn damage outcome changed class".into()),
    }
    if state
        .engine
        .event_history
        .iter()
        .filter(|e| {
            matches!(e,CommittedEvent::GorehornOperationV1{operation:c}
  if c.damage_operation==Some(o.index))
        })
        .count()
        != 1
    {
        return Err("Gorehorn damage operation lost unique closed owner".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registered_nonraid_absence_does_not_mutate_bytes_or_hash_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = crate::policy_observation_v6::tests::ready_state();
            state.active_player = actor;
            state.priority_player = actor;
            let object = crate::policy_observation_v6::tests::put(
                &mut state,
                actor,
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let before = serde_json::to_vec(&state).unwrap();
            let hash = state.state_hash();
            before_departure(&mut state, object);
            validate_state(&state).unwrap();
            assert!(public_sources(&state).unwrap().is_empty());
            assert_eq!(before, serde_json::to_vec(&state).unwrap());
            assert_eq!(hash, state.state_hash());
        }
    }
    #[test]
    fn empty_publication_wrapper_is_omitted_and_contributes_no_hash_input() {
        #[derive(Default)]
        struct Trace(Vec<u8>);
        impl std::hash::Hasher for Trace {
            fn finish(&self) -> u64 {
                0
            }
            fn write(&mut self, bytes: &[u8]) {
                self.0.extend(bytes);
            }
        }
        let mut trace = Trace::default();
        std::hash::Hash::hash(&crate::state::GorehornPublishedV1::default(), &mut trace);
        assert!(trace.0.is_empty());
        let value = serde_json::to_value(crate::state::StackStateV4::default()).unwrap();
        assert!(value.get("gorehorn_published_v1").is_none());
    }
}
