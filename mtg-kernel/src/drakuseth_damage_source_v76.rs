//! Original-source authority for the native positional Drakuseth damage batch.
//! This does not admit the printing to an owning policy consumer.
use crate::{
    card_def::{TargetSpec, CARD_DEFS},
    effect::{EffectOp, ExecCtx},
    event::{self, CommittedEvent as E, HistoricalDamageSourceV1, ProposedEvent},
    event_history_v1::EventKindV1,
    ids::{ObjectId, PlayerId, StackItemId},
    standard_cards_v1::{self, StandardTargetV1},
    state::{
        AbilitySourceContractV4, GameObject, GameState, StackItem, StackItemKind, Target, Zone,
    },
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OutcomeV76 {
    Resolved,
    Countered,
    NoLegalTarget,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JournalV76 {
    Departure {
        snapshot: Box<GameObject>,
        facts: HistoricalDamageSourceV1,
    },
    Published {
        item: Box<StackItem>,
    },
    Removed {
        publication: u32,
        item: Box<StackItem>,
        outcome: OutcomeV76,
    },
    CrystalResolved {
        publication: u32,
        boundary: u32,
        item: Box<StackItem>,
    },
}
// Only the native resolver may hold this loan across its actual stack removal.
// A serialized journal is data, not an executable loan.
pub(crate) struct OwnerV76 {
    publication: u32,
    item: Box<StackItem>,
}
fn rows(state: &GameState) -> impl Iterator<Item = (usize, &JournalV76)> {
    state
        .engine
        .event_history
        .scan_for_v1(&[EventKindV1::DrakusethDamageSourceV76])
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            E::DrakusethDamageSourceV76 { journal } => Some((i, journal.as_ref())),
            _ => None,
        })
}
fn append(state: &mut GameState, journal: JournalV76) {
    state
        .engine
        .event_history
        .push(E::DrakusethDamageSourceV76 {
            journal: Box::new(journal),
        });
}
pub(crate) fn owns_definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && CARD_DEFS.get(usize::from(id)).is_some_and(|d| {
        d.name == "Drakuseth, Maw of Flames"
            && d.is_executable()
            && crate::card_def::card_id_by_name(d.name) == Some(id)
    })
}
fn spec() -> TargetSpec {
    TargetSpec::StandardV1(StandardTargetV1::OneToThreeDistinctAnyTargetsV1)
}
pub(crate) fn selected(item: &StackItem) -> bool {
    item.kind == StackItemKind::TriggeredAbility
        && item.v4.granted_by.is_none()
        && item.inline_effect == Some(standard_cards_v1::drakuseth_attack_v1())
        && item
            .v4
            .ability_source_contract
            .is_some_and(|s| owns_definition(s.card_def))
}
fn tracked(state: &GameState, item: &StackItem) -> bool {
    selected(item)
        || !item.v4.drakuseth_published_v76.is_empty()
        || has_publication(state, item.v4.stack_item_id)
}
pub(crate) fn has_publication(state: &GameState, id: StackItemId) -> bool {
    rows(state).any(|(_, j)| matches!(j,JournalV76::Published{item} if item.v4.stack_item_id==id))
}
fn publication(state: &GameState, id: StackItemId) -> Result<(u32, &StackItem), String> {
    let found = rows(state)
        .filter_map(|(i, j)| match j {
            JournalV76::Published { item } if item.v4.stack_item_id == id => {
                Some((i, item.as_ref()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(at, item)] = found.as_slice() else {
        return Err("Drakuseth unique original publication absent".into());
    };
    let at = u32::try_from(*at).map_err(|_| "Drakuseth publication overflow")?;
    let source = item
        .v4
        .ability_source_contract
        .ok_or("Drakuseth original source absent")?;
    if !selected(item)
        || item.v4.drakuseth_published_v76.0 != Some(at)
        || source.source != item.source
        || source.controller != item.controller
        || source.zone != Zone::Battlefield
        || item.v4.target_spec != Some(spec())
        || !(1..=3).contains(&item.targets.len())
        || item.v4.target_contracts.len() != item.targets.len()
        || item
            .targets
            .iter()
            .zip(&item.v4.target_contracts)
            .any(|(t, c)| *t != c.target())
        || item
            .targets
            .iter()
            .enumerate()
            .any(|(i, t)| matches!(t, Target::StackItem(_)) || item.targets[..i].contains(t))
        || !item.v4.crystal_prevention_started_v1.is_empty()
    {
        return Err(
            "Drakuseth published source, positional targets or printed program changed".into(),
        );
    }
    crate::engine::validate_historical_ability_source_contract(state, source)?;
    // Consistency with a genuine declaration is not an occurrence join. The
    // publication itself is emitted by native placement, never reconstructed
    // from this descriptive history or its cardinality.
    if !state.engine.event_history[..at as usize].iter().any(|e| {
        matches!(e,
        E::DeclaredAttacker{source:s,source_zone_change_count:g,controller:c}
        if *s==source.source && *g==source.zone_change_count && *c==source.controller)
    }) {
        return Err("Drakuseth publication lacks its original declaration".into());
    }
    Ok((at, item))
}
pub(crate) fn publish(state: &mut GameState, id: StackItemId) -> Result<(), String> {
    let item = state
        .stack
        .iter()
        .find(|i| i.v4.stack_item_id == id)
        .ok_or("Drakuseth native placement absent")?;
    if !selected(item) {
        return Ok(());
    }
    if !item.v4.drakuseth_published_v76.is_empty() || has_publication(state, id) {
        return Err("Drakuseth publication repeated".into());
    }
    let at = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "Drakuseth publication overflow")?;
    let item = state
        .stack
        .iter_mut()
        .find(|i| i.v4.stack_item_id == id)
        .unwrap();
    item.v4.drakuseth_published_v76 = crate::state::DrakusethPublishedV76(Some(at));
    let saved = Box::new(item.clone());
    append(state, JournalV76::Published { item: saved });
    publication(state, id)?;
    Ok(())
}
pub(crate) fn before_batch_departures(state: &mut GameState, events: &[ProposedEvent]) {
    let mut seen = Vec::new();
    for e in events {
        if let ProposedEvent::ZoneChange(z) = e {
            if z.to_zone != Zone::Battlefield && !seen.contains(&z.object) {
                seen.push(z.object);
                before_departure(state, z.object);
            }
        }
    }
}
pub(crate) fn before_departure(state: &mut GameState, id: ObjectId) {
    let Some(live) = state
        .objects
        .try_get(id)
        .filter(|o| o.zone == Zone::Battlefield)
    else {
        return;
    };
    let original = |s: AbilitySourceContractV4| {
        s.source == id
            && s.zone_change_count == live.zone_change_count
            && owns_definition(s.card_def)
    };
    if !owns_definition(live.card_def)
        && !state
            .stack
            .iter()
            .any(|i| i.v4.ability_source_contract.is_some_and(original))
        && !state
            .engine
            .pending_triggers
            .iter()
            .any(|p| p.source_contract.is_some_and(original))
    {
        return;
    }
    // Freeze an entire simultaneous departure set before the first move.
    if rows(state).any(|(i,j)|matches!(j,JournalV76::Departure{facts,..}
        if facts.source.source==id && facts.source.zone_change_count==live.zone_change_count
        && !state.engine.event_history[i+1..].iter().any(|e|matches!(e,E::ZoneChange{object,..}if *object==id))))
    {return}
    let snapshot = Box::new(live.clone());
    let facts = crate::source_power_activation_v1::capture_damage_source(state, id);
    append(state, JournalV76::Departure { snapshot, facts });
}
fn departure(
    state: &GameState,
    at: usize,
    snapshot: &GameObject,
    facts: HistoricalDamageSourceV1,
) -> Result<(), String> {
    let s = facts.source;
    if snapshot.zone != Zone::Battlefield
        || s.zone != Zone::Battlefield
        || snapshot.card_def != s.card_def
        || snapshot.owner != s.owner
        || snapshot.controller != s.controller
        || snapshot.zone_change_count != s.zone_change_count
        || !state.engine.event_history[at + 1..].iter().any(|e| {
            matches!(e,
            E::ZoneChange{object,from:Zone::Battlefield,to,controller_before}
            if *object==s.source && *to!=Zone::Battlefield && *controller_before==s.controller)
        })
    {
        return Err("Drakuseth departure lost original incarnation or actual zone move".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, s)
}
fn source_facts(
    state: &GameState,
    source: AbilitySourceContractV4,
) -> Result<HistoricalDamageSourceV1, String> {
    crate::engine::validate_historical_ability_source_contract(state, source)?;
    if state.objects.try_get(source.source).is_some_and(|o| {
        o.zone == Zone::Battlefield
            && o.owner == source.owner
            && o.zone_change_count == source.zone_change_count
    }) {
        return Ok(crate::source_power_activation_v1::capture_damage_source(
            state,
            source.source,
        ));
    }
    let found = rows(state)
        .filter_map(|(i, j)| match j {
            JournalV76::Departure { snapshot, facts }
                if facts.source.source == source.source
                    && facts.source.owner == source.owner
                    && facts.source.zone_change_count == source.zone_change_count =>
            {
                Some((i, snapshot.as_ref(), *facts))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(at, snapshot, facts)] = found.as_slice() else {
        return Err("Drakuseth exact departing source absent or duplicated".into());
    };
    departure(state, *at, snapshot, *facts)?;
    Ok(*facts)
}
pub(crate) fn damage_source(
    state: &GameState,
    source: ObjectId,
    id: StackItemId,
) -> Result<HistoricalDamageSourceV1, String> {
    let (_, item) = publication(state, id)?;
    if item.source != source {
        return Err("Drakuseth damage source substituted".into());
    }
    source_facts(state, item.v4.ability_source_contract.unwrap())
}
pub(crate) fn legal_targets(
    state: &GameState,
    source: ObjectId,
    card_def: u16,
    generation: u32,
    controller: PlayerId,
    chosen: &[Target],
) -> Result<Vec<Target>, String> {
    let object = state
        .objects
        .try_get(source)
        .ok_or("Drakuseth target source absent")?;
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
        TargetSpec::AnyTarget,
        chosen,
        controller,
        state,
    )
    .into_iter()
    .filter(|t| !chosen.contains(t))
    .filter(|t| match t {
        Target::Object(t) => {
            !crate::source_power_activation_v1::protected(state, facts, *t)
                && !crate::source_type_hexproof_v1::protects_from_source(
                    state,
                    *t,
                    controller,
                    facts.source,
                )
                .unwrap_or(true)
                && !crate::standard_legends_v1::hexproof_from_historical_color(
                    state,
                    *t,
                    facts.color_mask,
                    controller,
                )
        }
        _ => true,
    })
    .collect())
}
fn context(item: &StackItem) -> ExecCtx {
    ExecCtx {
        niv_damage_origin_v1: None,
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
fn closed(state: &GameState, at: u32) -> usize {
    rows(state)
        .filter(|(_, j)| matches!(j,JournalV76::Removed{publication,..}|JournalV76::CrystalResolved{publication,..}if *publication==at))
        .count()
}
pub(crate) fn acquire_owner(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<OwnerV76>, String> {
    if !tracked(state, item) {
        return Ok(None);
    }
    let (at, saved) = publication(state, item.v4.stack_item_id)?;
    if !crate::effect::crystal_prevention_v1::source_power_payment_matches(state, item, saved)
        || closed(state, at) != 0
        || state
            .stack
            .iter()
            .filter(|i| i.v4.stack_item_id == item.v4.stack_item_id)
            .count()
            != 1
        || crate::engine::time_stop_v1::disposed(state, saved)?
    {
        return Err("Drakuseth actual unclosed stack row changed".into());
    }
    source_facts(state, item.v4.ability_source_contract.unwrap())?;
    Ok(Some(OwnerV76 {
        publication: at,
        item: Box::new(saved.clone()),
    }))
}
pub(crate) fn record_removed(
    state: &mut GameState,
    owner: Option<OwnerV76>,
    outcome: OutcomeV76,
) -> Result<(), String> {
    let Some(owner) = owner else { return Ok(()) };
    let (at, saved) = publication(state, owner.item.v4.stack_item_id)?;
    if at != owner.publication
        || saved != owner.item.as_ref()
        || closed(state, at) != 0
        || state
            .stack
            .iter()
            .any(|i| i.v4.stack_item_id == owner.item.v4.stack_item_id)
    {
        return Err("Drakuseth removed-row loan lost its exact publication".into());
    }
    append(
        state,
        JournalV76::Removed {
            publication: at,
            item: owner.item,
            outcome,
        },
    );
    Ok(())
}
pub(crate) fn execute_owned(
    state: &mut GameState,
    op: &EffectOp,
    ctx: &ExecCtx,
    owner: OwnerV76,
) -> Result<(), String> {
    if *op != standard_cards_v1::drakuseth_attack_v1()
        || !ctx.same_except_niv_damage_origin_v1(&context(&owner.item))
        || state
            .stack
            .iter()
            .any(|i| i.v4.stack_item_id == owner.item.v4.stack_item_id)
        || publication(state, owner.item.v4.stack_item_id)?.0 != owner.publication
        || closed(state, owner.publication) != 0
    {
        return Err("Drakuseth positional damage lost its native removed-row loan".into());
    }
    let proposals = positional_damage(state, ctx)?
        .into_iter()
        .map(|(target, amount)| {
            let mut proposal = event::damage_from_exec_ctx_v1(ctx, target, amount);
            let ProposedEvent::Damage(damage) = &mut proposal else {
                unreachable!()
            };
            damage.historical_source = ctx.stack_item_id;
            proposal
        })
        .collect();
    // Original positions and all legal recipients are committed together.
    event::propose_and_commit_batch(state, proposals);
    if state.engine.halted.is_some() {
        return Err("Drakuseth damage batch did not complete".into());
    }
    record_removed(state, Some(owner), OutcomeV76::Resolved)
}
pub(crate) fn positional_damage(
    state: &GameState,
    ctx: &ExecCtx,
) -> Result<Vec<(Target, i32)>, String> {
    let id = ctx
        .stack_item_id
        .ok_or("Drakuseth positional context lost stack identity")?;
    let (_, saved) = publication(state, id)?;
    if !ctx.same_except_niv_damage_origin_v1(&context(saved)) {
        return Err("Drakuseth positional context differs from original publication".into());
    }
    damage_source(state, ctx.source, id)?;
    let source = ctx.ability_source_contract.unwrap();
    let legal = legal_targets(
        state,
        ctx.source,
        source.card_def,
        source.zone_change_count,
        ctx.controller,
        &[],
    )?;
    Ok(ctx
        .targets
        .iter()
        .enumerate()
        .filter(|(slot, t)| ctx.target_incarnation_matches(*slot, state) && legal.contains(t))
        .map(|(slot, t)| (*t, if slot == 0 { 4 } else { 3 }))
        .collect())
}
fn validate_crystal_completion(
    state: &GameState,
    at: usize,
    publication_at: u32,
    boundary: u32,
    item: &StackItem,
) -> Result<(), String> {
    let (index, saved) = publication(state, item.v4.stack_item_id)?;
    let Some(E::CrystalPreventionBoundaryV1 { boundary: b }) =
        state.engine.event_history.get(boundary as usize)
    else {
        return Err("Drakuseth Crystal completion lost actual final boundary".into());
    };
    if index != publication_at
        || index >= boundary
        || boundary as usize >= at
        || b.history_index != boundary
        || b.item.as_ref() != item
        || b.root != standard_cards_v1::drakuseth_attack_v1()
        || b.pending.is_some()
        || b.previous.is_none()
        || !crate::effect::crystal_prevention_v1::source_power_payment_matches(state, item, saved)
        || closed(state, index) != 1
    {
        return Err("Drakuseth Crystal completion changed its original native owner".into());
    }
    Ok(())
}
pub(crate) fn finish_crystal_owned(
    state: &mut GameState,
    owner: &OwnerV76,
    item: &StackItem,
) -> Result<(), String> {
    if closed(state, owner.publication) != 0
        || state.engine.pending_effect.is_some()
        || state
            .stack
            .iter()
            .any(|i| i.v4.stack_item_id == item.v4.stack_item_id)
        || !crate::effect::crystal_prevention_v1::source_power_payment_matches(
            state,
            item,
            &owner.item,
        )
    {
        return Err("Drakuseth Crystal completion lost its private removed-row loan".into());
    }
    let boundary = state
        .engine
        .event_history
        .scan_for_v1(&[EventKindV1::CrystalPreventionBoundaryV1])
        .iter()
        .rev()
        .find_map(|e| match e {
            E::CrystalPreventionBoundaryV1 { boundary }
                if boundary.item.v4.stack_item_id == item.v4.stack_item_id =>
            {
                Some(boundary.history_index)
            }
            _ => None,
        })
        .ok_or("Drakuseth Crystal final boundary absent")?;
    let at = state.engine.event_history.len();
    append(
        state,
        JournalV76::CrystalResolved {
            publication: owner.publication,
            boundary,
            item: Box::new(item.clone()),
        },
    );
    validate_crystal_completion(state, at, owner.publication, boundary, item)
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if state
        .engine
        .event_log
        .iter()
        .any(|e| matches!(e, E::DrakusethDamageSourceV76 { .. }))
    {
        return Err("Drakuseth private source journal leaked into transient log".into());
    }
    for item in &state.stack {
        if tracked(state, item) {
            acquire_owner(state, item)?;
        }
    }
    for (at, j) in rows(state) {
        match j {
            JournalV76::Departure { snapshot, facts } => departure(state, at, snapshot, *facts)?,
            JournalV76::Published { item } => {
                let (index, saved) = publication(state, item.v4.stack_item_id)?;
                let live = state
                    .stack
                    .iter()
                    .filter(|i| i.v4.stack_item_id == saved.v4.stack_item_id)
                    .count();
                if !matches!(
                    (
                        live,
                        closed(state, index),
                        crate::engine::time_stop_v1::disposed(state, saved)?
                    ),
                    (1, 0, false) | (0, 1, false) | (0, 0, true)
                ) {
                    return Err(
                        "Drakuseth publication was erased, replayed or doubly closed".into(),
                    );
                }
            }
            JournalV76::Removed {
                publication: original,
                item,
                ..
            } => {
                let (index, saved) = publication(state, item.v4.stack_item_id)?;
                if index != *original
                    || index as usize >= at
                    || item.as_ref() != saved
                    || closed(state, index) != 1
                {
                    return Err("Drakuseth removal lost its earlier exact publication".into());
                }
            }
            JournalV76::CrystalResolved {
                publication,
                boundary,
                item,
            } => {
                validate_crystal_completion(state, at, *publication, *boundary, item)?;
            }
        }
    }
    Ok(())
}
