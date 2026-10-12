//! Captured fixed damage for the actual Firebrand sacrifice payment.
//! Opt-in is catalog-owned; no new record is emitted by historical catalogs.
use crate::card_def::{CostComponent, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, TargetRef};
use crate::event::{CommittedEvent, HistoricalDamageSourceV1};
use crate::ids::{ObjectId, StackItemId};
use crate::state::{
    AbilitySourceContractV4, GameObject, GameState, StackItem, StackItemKind, Target, Zone,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DepartureV3 {
    pub history_index: u32,
    pub snapshot: GameObject,
    pub facts: HistoricalDamageSourceV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaidV3 {
    pub history_index: u32,
    pub departure: u32,
    pub item: Box<StackItem>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OutcomeV3 {
    Resolved,
    Countered,
    NoLegalTarget,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RemovedV3 {
    pub history_index: u32,
    pub payment: u32,
    pub item: Box<StackItem>,
    pub outcome: OutcomeV3,
}
// Neither authority can be serialized or constructed from a restored journal.
pub(crate) struct PaymentOwnerV3 {
    source: AbilitySourceContractV4,
}
pub(crate) struct RemovalOwnerV3 {
    item: StackItem,
    payment: u32,
}

fn enabled() -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && CARD_DEFS
        .iter()
        .any(|d| d.name == "Crystal Barricade" && d.is_executable())
}
pub(crate) fn printed_root() -> EffectOp {
    EffectOp::DealDamage {
        target: TargetRef::Target(0),
        amount: 1,
    }
}
fn printed_definition(def: u16, index: u8) -> bool {
    CARD_DEFS.get(usize::from(def)).is_some_and(|d| {
        d.name == "Fanatical Firebrand"
            && d.is_executable()
            && index == 0
            && d.activated_abilities
                .get(usize::from(index))
                .is_some_and(|a| {
                    a.cost == [CostComponent::Tap, CostComponent::SacrificeSelf]
                        && a.activation_zone == Zone::Battlefield
                        && a.target_spec == TargetSpec::AnyTarget
                        && (a.effect)() == printed_root()
                })
    })
}
pub(crate) fn selected(item: &StackItem) -> bool {
    enabled()
        && item.kind == StackItemKind::ActivatedAbility
        && item
            .v4
            .ability_source_contract
            .zip(item.v4.activated_ability_index)
            .is_some_and(|(s, i)| printed_definition(s.card_def, i))
}
pub(crate) fn tracked(state: &GameState, item: &StackItem) -> bool {
    selected(item) || item.v4.fixed_damage_paid_v3.0.is_some()
        || state.engine.event_history.iter().any(|e| matches!(e,
            CommittedEvent::FixedDamagePaidV3 { payment } if payment.item.v4.stack_item_id==item.v4.stack_item_id))
}
pub(crate) fn requires_successor(state: &GameState) -> bool {
    state.stack.iter().any(|i| tracked(state, i))
        || state.engine.event_history.iter().any(is_record)
}
fn is_record(e: &CommittedEvent) -> bool {
    matches!(
        e,
        CommittedEvent::FixedDamageDepartureV3 { .. }
            | CommittedEvent::FixedDamagePaidV3 { .. }
            | CommittedEvent::FixedDamageRemovedV3 { .. }
    )
}
pub(crate) fn acquire_payment(state: &GameState) -> Result<Option<PaymentOwnerV3>, String> {
    if !enabled() {
        return Ok(None);
    }
    let p = state
        .engine
        .pending_activation
        .as_ref()
        .ok_or("fixed damage payment has no actual pending activation")?;
    let source = AbilitySourceContractV4::capture(state, p.source);
    if !printed_definition(source.card_def, p.ability_index) {
        // A borrowed Firebrand recipe needs its own departing-source capture.
        if crate::engine::resolved_activated_ability(
            source.card_def,
            p.ability_index,
            state,
            p.source,
        )
        .is_some_and(|a| {
            (a.effect)() == printed_root()
                && a.cost == [CostComponent::Tap, CostComponent::SacrificeSelf]
        }) {
            return Err(
                "borrowed fixed damage requires an authenticated departure successor".into(),
            );
        }
        return Ok(None);
    }
    if p.cauldron_grant.0.is_some()
        || source.zone != Zone::Battlefield
        || source.controller != p.controller
        || source.zone_change_count != p.source_zone_change_count
    {
        return Err("fixed damage payment lost actual original source".into());
    }
    Ok(Some(PaymentOwnerV3 { source }))
}
pub(crate) fn capture_after_tap(
    state: &mut GameState,
    owner: &PaymentOwnerV3,
) -> Result<u32, String> {
    if AbilitySourceContractV4::capture(state, owner.source.source) != owner.source
        || !state.objects.get(owner.source.source).tapped
    {
        return Err("fixed damage departure is not the actual tapped payment source".into());
    }
    let history_index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "fixed damage history overflow")?;
    let facts =
        crate::source_power_activation_v1::capture_damage_source(state, owner.source.source);
    let departure = DepartureV3 {
        history_index,
        snapshot: state.objects.get(owner.source.source).clone(),
        facts,
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::FixedDamageDepartureV3 {
            departure: Box::new(departure),
        });
    Ok(history_index)
}
pub(crate) fn record_paid(state: &mut GameState, id: StackItemId, departure: Option<u32>) {
    let Some(departure) = departure else {
        return;
    };
    let history_index =
        u32::try_from(state.engine.event_history.len()).expect("bounded fixed payment history");
    let item = state
        .stack
        .iter_mut()
        .find(|i| i.v4.stack_item_id == id)
        .expect("actual paid activation");
    item.v4.fixed_damage_paid_v3 = crate::state::FixedDamagePaidV3(Some(history_index));
    let payment = PaidV3 {
        history_index,
        departure,
        item: Box::new(item.clone()),
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::FixedDamagePaidV3 {
            payment: Box::new(payment),
        });
}
fn payment(state: &GameState, id: StackItemId) -> Result<&PaidV3, String> {
    let mut rows = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            CommittedEvent::FixedDamagePaidV3 { payment }
                if payment.item.v4.stack_item_id == id =>
            {
                Some((i, payment.as_ref()))
            }
            _ => None,
        });
    let (index, p) = rows
        .next()
        .ok_or("fixed damage activation lost mandatory payment")?;
    if rows.next().is_some()
        || p.history_index as usize != index
        || !selected(&p.item)
        || p.item.v4.stack_item_id == StackItemId::default()
        || p.item.inline_effect.as_ref() != Some(&printed_root())
        || p.item.v4.fixed_damage_paid_v3.0 != Some(p.history_index)
        || !p.item.v4.crystal_prevention_started_v1.is_empty()
        || p.item.v4.granted_by.is_some()
        || p.item.v4.cauldron_grant.0.is_some()
    {
        return Err("fixed damage payment differs from its original printed activation".into());
    }
    let Some(CommittedEvent::FixedDamageDepartureV3 { departure }) =
        state.engine.event_history.get(p.departure as usize)
    else {
        return Err("fixed damage payment lost departure".into());
    };
    let s = p
        .item
        .v4
        .ability_source_contract
        .ok_or("fixed damage has no captured source")?;
    let snapshot = &departure.snapshot;
    let mut normalized = departure.facts.source;
    normalized.attached_to = None;
    if departure.history_index != p.departure
        || p.departure >= p.history_index
        || normalized != s
        || snapshot.zone != Zone::Battlefield
        || !snapshot.tapped
        || snapshot.card_def != s.card_def
        || snapshot.owner != s.owner
        || snapshot.controller != s.controller
        || snapshot.zone_change_count != s.zone_change_count
        || p.item.source != s.source
        || p.item.controller != s.controller
        || p.item.v4.target_spec != Some(TargetSpec::AnyTarget)
        || p.item.targets.len() != 1
        || p.item
            .v4
            .paid_cost_refs
            .iter()
            .filter(|r| r.object == s.source)
            .count()
            != 1
    {
        return Err("fixed damage payment changed departure identity or cost".into());
    }
    let between = &state.engine.event_history[p.departure as usize + 1..index];
    let sacrifice = between
        .iter()
        .position(|e| {
            matches!(e,CommittedEvent::Sacrificed{object,controller_before,..}
        if *object==s.source && *controller_before==s.controller)
        })
        .ok_or("fixed damage lost actual sacrifice")?;
    if !between[sacrifice + 1..].iter().any(|e| {
        matches!(e,CommittedEvent::ZoneChange {object,from:Zone::Battlefield,controller_before,..}
        if *object==s.source && *controller_before==s.controller)
    }) {
        return Err("fixed damage lost actual post-sacrifice departure".into());
    }
    if state
        .engine
        .event_history
        .iter()
        .filter(|e| {
            matches!(e,CommittedEvent::FixedDamagePaidV3 {payment}
        if payment.departure==p.departure)
        })
        .count()
        != 1
    {
        return Err("fixed damage departure has duplicate paid owners".into());
    }
    Ok(p)
}
pub(crate) fn damage_source(
    state: &GameState,
    source: ObjectId,
    id: StackItemId,
) -> Result<HistoricalDamageSourceV1, String> {
    let p = payment(state, id)?;
    if p.item.source != source {
        return Err("fixed historical source differs from paid activation".into());
    }
    match &state.engine.event_history[p.departure as usize] {
        CommittedEvent::FixedDamageDepartureV3 { departure } => Ok(departure.facts),
        _ => Err("fixed historical departure disappeared".into()),
    }
}
pub(crate) fn has_payment(state: &GameState, id: StackItemId) -> bool {
    state.engine.event_history.iter().any(|e| {
        matches!(e,CommittedEvent::FixedDamagePaidV3 {payment}
        if payment.item.v4.stack_item_id==id)
    })
}
pub(crate) fn validate_item(state: &GameState, item: &StackItem) -> Result<(), String> {
    if !tracked(state, item) {
        return Ok(());
    }
    let p = payment(state, item.v4.stack_item_id)?;
    if !crate::effect::crystal_prevention_v1::source_power_payment_matches(state, item, &p.item)
        || state
            .stack
            .iter()
            .filter(|i| i.v4.stack_item_id == item.v4.stack_item_id)
            .count()
            != 1
        || state.engine.event_history.iter().any(|e| {
            matches!(e,CommittedEvent::FixedDamageRemovedV3 {removal}
            if removal.payment==p.history_index)
        })
    {
        return Err("fixed damage lacks its exact unique unclosed live owner".into());
    }
    Ok(())
}
pub(crate) fn acquire_removal(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<RemovalOwnerV3>, String> {
    if !tracked(state, item) {
        return Ok(None);
    }
    validate_item(state, item)?;
    if !state.stack.iter().any(|i| i == item) {
        return Err("fixed removal does not own actual live row".into());
    }
    Ok(Some(RemovalOwnerV3 {
        item: item.clone(),
        payment: payment(state, item.v4.stack_item_id)?.history_index,
    }))
}
fn completed_item(state: &GameState, payment: &PaidV3) -> Result<StackItem, String> {
    let rows = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            CommittedEvent::CrystalPreventionBoundaryV1 { boundary }
                if boundary.item.v4.stack_item_id == payment.item.v4.stack_item_id =>
            {
                Some(boundary.as_ref())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let last = rows
        .last()
        .ok_or("fixed damage resolution has no owned execution")?;
    if last.pending.is_some()
        || last.answer.is_some()
        || !crate::effect::crystal_prevention_v1::source_power_payment_matches(
            state,
            &last.item,
            &payment.item,
        )
    {
        return Err("fixed damage has no genuine closed owned execution".into());
    }
    Ok(*last.item.clone())
}
pub(crate) fn record_removed(
    state: &mut GameState,
    owner: Option<RemovalOwnerV3>,
    outcome: OutcomeV3,
) -> Result<(), String> {
    let Some(owner) = owner else {
        return Ok(());
    };
    let p = payment(state, owner.item.v4.stack_item_id)?;
    if p.history_index != owner.payment
        || state
            .stack
            .iter()
            .any(|i| i.v4.stack_item_id == owner.item.v4.stack_item_id)
        || state.engine.event_history.iter().any(|e| {
            matches!(e,CommittedEvent::FixedDamageRemovedV3{removal}
            if removal.payment==owner.payment)
        })
    {
        return Err("fixed damage removal no longer owns its original paid row".into());
    }
    let item = if outcome == OutcomeV3::Resolved {
        completed_item(state, p)?
    } else {
        if owner.item.v4.crystal_prevention_started_v1.0.is_some()
            || state.engine.event_history.iter().any(|e| {
                matches!(e,CommittedEvent::CrystalPreventionBoundaryV1{boundary}
                if boundary.item.v4.stack_item_id==owner.item.v4.stack_item_id)
            })
        {
            return Err("started fixed damage cannot be downgraded to unstarted removal".into());
        }
        owner.item
    };
    let history_index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "fixed removal history overflow")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::FixedDamageRemovedV3 {
            removal: Box::new(RemovedV3 {
                history_index,
                payment: owner.payment,
                item: Box::new(item),
                outcome,
            }),
        });
    Ok(())
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if state.engine.event_log.iter().any(is_record) {
        return Err("fixed payment authority in transient log".into());
    }
    for item in &state.stack {
        validate_item(state, item)?;
    }
    for (i, e) in state.engine.event_history.iter().enumerate() {
        match e {
            CommittedEvent::FixedDamageDepartureV3{departure}=>{
                if departure.history_index as usize!=i || state.engine.event_history.iter().filter(|e|
                    matches!(e,CommittedEvent::FixedDamagePaidV3{payment} if payment.departure as usize==i)).count()!=1 {
                    return Err("fixed departure lost its unique paid owner".into());
                }
            }
            CommittedEvent::FixedDamagePaidV3{payment:p}=>{
                let p=payment(state,p.item.v4.stack_item_id)?;
                let removed=state.engine.event_history.iter().filter_map(|e|match e {
                    CommittedEvent::FixedDamageRemovedV3{removal} if removal.payment==p.history_index=>Some(removal),
                    _=>None,
                }).collect::<Vec<_>>();
                let live=state.stack.iter().filter(|i|i.v4.stack_item_id==p.item.v4.stack_item_id).count();
                match (live,removed.as_slice()) {
                    (1,[])=>{},
                    (0,[r]) if r.history_index>p.history_index && (r.history_index as usize)<state.engine.event_history.len()=>{
                        if r.outcome==OutcomeV3::Resolved {
                            if completed_item(state,p)?!=*r.item || state.engine.event_history[r.history_index as usize..].iter().any(|e|
                                matches!(e,CommittedEvent::CrystalPreventionBoundaryV1{boundary}
                                    if boundary.item.v4.stack_item_id==p.item.v4.stack_item_id)) {
                                return Err("fixed resolved closure changed or preceded execution".into());
                            }
                        } else if r.item!=p.item || state.engine.event_history.iter().any(|e|
                            matches!(e,CommittedEvent::CrystalPreventionBoundaryV1{boundary}
                                if boundary.item.v4.stack_item_id==p.item.v4.stack_item_id)) {
                            return Err("fixed unstarted closure changed".into());
                        }
                    },
                    _=>return Err("fixed damage owner was erased, reintroduced or replayed".into()),
                }
            }
            CommittedEvent::FixedDamageRemovedV3{removal:r}=>{
                if r.history_index as usize!=i || !matches!(state.engine.event_history.get(r.payment as usize),
                    Some(CommittedEvent::FixedDamagePaidV3{payment}) if payment.item.v4.stack_item_id==r.item.v4.stack_item_id) {
                    return Err("fixed removal lost exact paid owner".into());
                }
            }
            _=>{},
        }
    }
    Ok(())
}
pub(crate) fn legal_targets(state: &GameState, facts: HistoricalDamageSourceV1) -> Vec<Target> {
    crate::engine::legal_targets_for_controller(
        TargetSpec::AnyTarget,
        &[],
        facts.source.controller,
        state,
    )
    .into_iter()
    .filter(|target| match *target {
        Target::Player(_) => true,
        Target::StackItem(_) => false,
        Target::Object(id) => {
            !crate::source_power_activation_v1::protected(state, facts, id)
                && !crate::standard_legends_v1::hexproof_from_historical_color(
                    state,
                    id,
                    facts.color_mask,
                    facts.source.controller,
                )
        }
    })
    .collect()
}

#[cfg(test)]
#[path = "fixed_source_damage_v3_tests.rs"]
mod tests;
