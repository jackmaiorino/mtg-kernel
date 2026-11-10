//! Captured fixed damage for actual Firebrand, Fanatic and Bombardment payments.
//! Each catalog owns its exact printed activation.
use crate::card_def::{CostComponent, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, ExecCtx, TargetRef};
use crate::event::{CommittedEvent, HistoricalDamageSourceV1};
use crate::event_history_v1::EventKindV1;
use crate::ids::{ObjectId, StackItemId};
use crate::source_power_activation_v1::bombardment;
use crate::state::{
    AbilitySourceContractV4, GameObject, GameState, StackItem, StackItemKind, Target, Zone,
};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepartureV3 {
    pub history_index: u32,
    pub snapshot: GameObject,
    pub facts: HistoricalDamageSourceV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metamorph_physical_def: Option<u16>,
}
impl Hash for DepartureV3 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.history_index.hash(state);
        self.snapshot.hash(state);
        self.facts.hash(state);
        if let Some(definition) = self.metamorph_physical_def {
            b"bombardment-metamorph/v1".hash(state);
            definition.hash(state);
        }
    }
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
    tap: bool,
    bombardment_self: bool,
    metamorph_physical_def: Option<u16>,
}
pub(crate) struct RemovalOwnerV3 {
    item: StackItem,
    payment: u32,
}

fn firebrand_enabled() -> bool {
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
fn printed_definition(def: u16, index: u8, name: &str, cost: &[CostComponent]) -> bool {
    let Some(d) = CARD_DEFS.get(usize::from(def)) else {
        return false;
    };
    d.name == name
        && d.is_executable()
        && index == 0
        && d.activated_abilities.first().is_some_and(|a| {
            a.cost == cost
                && a.activation_zone == Zone::Battlefield
                && a.target_spec == TargetSpec::AnyTarget
                && (a.effect)() == printed_root()
        })
}
fn fanatic(def: u16, index: u8) -> bool {
    cfg!(feature = "eternal-fixtures")
        && printed_definition(def, index, "Mogg Fanatic", &[CostComponent::SacrificeSelf])
}
fn firebrand(def: u16, index: u8) -> bool {
    firebrand_enabled()
        && printed_definition(
            def,
            index,
            "Fanatical Firebrand",
            &[CostComponent::Tap, CostComponent::SacrificeSelf],
        )
}
fn paid_fanatic(item: &StackItem) -> bool {
    item.v4.fixed_damage_paid_v3.0.is_some()
        && item
            .v4
            .ability_source_contract
            .zip(item.v4.activated_ability_index)
            .is_some_and(|(s, i)| fanatic(s.card_def, i))
}
pub(crate) fn paid_bombardment_self(item: &StackItem) -> bool {
    item.v4.fixed_damage_paid_v3.0.is_some()
        && item
            .v4
            .ability_source_contract
            .zip(item.v4.activated_ability_index)
            .is_some_and(|(s, i)| bombardment::printed(s.card_def, i))
}

pub(crate) fn waiting_bombardment_source(state: &GameState, source: ObjectId) -> bool {
    if !cfg!(feature = "eternal-fixtures") {
        return false;
    }
    state
        .stack
        .iter()
        .chain(state.engine.pending_effect.iter().map(|p| &p.resolving_item))
        .any(|item| {
            item.source == source
                && (paid_bombardment_self(item)
                    || (bombardment::selected(state, item)
                        && has_payment(state, item.v4.stack_item_id)))
        })
}

fn native_ordinary_damage(item: &StackItem) -> bool {
    paid_fanatic(item) || paid_bombardment_self(item)
}

pub(crate) fn selected(item: &StackItem) -> bool {
    item.kind == StackItemKind::ActivatedAbility
        && (native_ordinary_damage(item)
            || item
                .v4
                .ability_source_contract
                .zip(item.v4.activated_ability_index)
                .is_some_and(|(s, i)| firebrand(s.card_def, i)))
}
pub(crate) fn tracked(state: &GameState, item: &StackItem) -> bool {
    selected(item) || item.v4.fixed_damage_paid_v3.0.is_some()
        || state.engine.event_history.scan_for_v1(&[EventKindV1::FixedDamagePaidV3]).iter().any(|e| matches!(e,
            CommittedEvent::FixedDamagePaidV3 { payment } if payment.item.v4.stack_item_id==item.v4.stack_item_id))
}
pub(crate) fn requires_crystal_execution(state: &GameState, item: &StackItem) -> bool {
    !native_ordinary_damage(item) && tracked(state, item)
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
    if !cfg!(feature = "eternal-fixtures") && !firebrand_enabled() {
        return Ok(None);
    }
    let p = state
        .engine
        .pending_activation
        .as_ref()
        .ok_or("fixed damage payment has no actual pending activation")?;
    let source = AbilitySourceContractV4::capture(state, p.source);
    let live = state.objects.get(p.source);
    let fanatic_recipe = fanatic(source.card_def, p.ability_index);
    let bombardment_recipe = bombardment::printed(source.card_def, p.ability_index)
        && matches!(p.object_cost_chosen.as_slice(), [cost] if cost.object == p.source);
    let eligible_card = !live.v4.is_token && live.spell_copy_origin.is_none();
    let physical_fanatic = fanatic_recipe
        && eligible_card
        && !crate::eternal_vintage_v1::active_copy(state, p.source);
    let bombardment_self = bombardment_recipe && eligible_card;
    let metamorph_physical_def = if bombardment_self {
        capture_metamorph_definition(state, source)?
    } else {
        None
    };
    let tap = firebrand(source.card_def, p.ability_index);
    if !physical_fanatic && !tap && !bombardment_self {
        // A borrowed Firebrand recipe needs its own departing-source capture.
        if firebrand_enabled()
            && crate::engine::resolved_activated_ability(
                source.card_def,
                p.ability_index,
                state,
                p.source,
            )
            .is_some_and(|a| {
                (a.effect)() == printed_root()
                    && a.cost == [CostComponent::Tap, CostComponent::SacrificeSelf]
            })
        {
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
    Ok(Some(PaymentOwnerV3 {
        source,
        tap,
        bombardment_self,
        metamorph_physical_def,
    }))
}

fn recorded_metamorph_definition(state: &GameState, source: AbilitySourceContractV4) -> Option<u16> {
    crate::card_def::card_id_by_name("Phyrexian Metamorph").filter(|&physical| {
        [physical, source.card_def].into_iter().all(|definition| {
            crate::standard_cards_v1::had_card_def(
                state,
                source.source,
                source.zone_change_count,
                definition,
            )
        })
    })
}

fn capture_metamorph_definition(
    state: &GameState,
    source: AbilitySourceContractV4,
) -> Result<Option<u16>, String> {
    let mut copies = state.eternal_v1.iter().flat_map(|eternal| {
        eternal.vintage.copies.iter().filter(|copy| {
            copy.host.object == source.source
                && copy.host.zone_change_count == source.zone_change_count
        })
    });
    let active = copies.next().map(|copy| copy.card_def);
    if copies.next().is_some() || active != recorded_metamorph_definition(state, source) {
        return Err("Bombardment has no unique original Metamorph copy form".into());
    }
    Ok(active)
}
impl PaymentOwnerV3 {
    pub(crate) fn requires_tap(&self) -> bool {
        self.tap
    }

    pub(crate) fn sacrifice_cost(&self) -> CostComponent {
        if self.bombardment_self {
            bombardment::COST
        } else {
            CostComponent::SacrificeSelf
        }
    }
}
pub(crate) fn capture_departure(
    state: &mut GameState,
    owner: &PaymentOwnerV3,
) -> Result<u32, String> {
    if AbilitySourceContractV4::capture(state, owner.source.source) != owner.source
        || (owner.tap && !state.objects.get(owner.source.source).tapped)
    {
        return Err("fixed damage departure is not the actual payment source".into());
    }
    let history_index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "fixed damage history overflow")?;
    let facts =
        crate::source_power_activation_v1::capture_damage_source(state, owner.source.source);
    let departure = DepartureV3 {
        history_index,
        snapshot: state.objects.get(owner.source.source).clone(),
        facts,
        metamorph_physical_def: owner.metamorph_physical_def,
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
        .scan_for_v1(&[EventKindV1::FixedDamagePaidV3])
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
    let metamorph_physical_def = if paid_bombardment_self(&p.item) {
        recorded_metamorph_definition(state, s)
    } else {
        None
    };
    if departure.metamorph_physical_def != metamorph_physical_def {
        return Err("Bombardment has no retained original Metamorph copy form".into());
    }
    let paid_definition = metamorph_physical_def.unwrap_or(s.card_def);
    let mut normalized = departure.facts.source;
    normalized.attached_to = None;
    if departure.history_index != p.departure
        || p.departure >= p.history_index
        || normalized != s
        || snapshot.zone != Zone::Battlefield
        || (!native_ordinary_damage(&p.item) && !snapshot.tapped)
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
    let departure_to = between[sacrifice + 1..].iter().find_map(|e| match e {
        CommittedEvent::ZoneChange {
            object,
            from: Zone::Battlefield,
            to,
            controller_before,
        } if *object == s.source && *controller_before == s.controller => Some(*to),
        _ => None,
    });
    let Some(departure_to) = departure_to else {
        return Err("fixed damage lost actual post-sacrifice departure".into());
    };
    if paid_bombardment_self(&p.item) {
        let first_source_move = between.iter().enumerate().find(|(_, event)| {
            matches!(event, CommittedEvent::ZoneChange { object, .. } if *object == s.source)
        });
        if sacrifice != 0
            || !matches!(first_source_move,
            Some((at, CommittedEvent::ZoneChange { from: Zone::Battlefield, to, controller_before, .. }))
                if at > sacrifice && *to != Zone::Battlefield && *to == departure_to && *controller_before == s.controller)
            || p.item.v4.paid_cost_refs.len() != 1
            || p.item.v4.target_contracts.len() != 1
            || !p.item.v4.bombardment_lki_v1.is_empty()
            || p.item.is_copy
        {
            return Err("Bombardment self-sacrifice lost its exact native payment".into());
        }
    }
    if native_ordinary_damage(&p.item)
        && (snapshot.v4.is_token
            || snapshot.spell_copy_origin.is_some()
            || !p.item.v4.paid_cost_refs.iter().any(|r| {
                r.object == s.source
                    && r.card_def == paid_definition
                    && r.owner == s.owner
                    && r.zone == departure_to
                    && s.zone_change_count.checked_add(1) == Some(r.zone_change_count)
            }))
    {
        return Err("fixed damage payment differs from its actual physical departure".into());
    }
    if state
        .engine
        .event_history
        .scan_for_v1(&[EventKindV1::FixedDamagePaidV3])
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
    if paid_bombardment_self(&p.item) {
        return Err("Bombardment self-sacrifice requires its native execution owner".into());
    }
    if p.item.source != source {
        return Err("fixed historical source differs from paid activation".into());
    }
    match &state.engine.event_history[p.departure as usize] {
        CommittedEvent::FixedDamageDepartureV3 { departure } => Ok(departure.facts),
        _ => Err("fixed historical departure disappeared".into()),
    }
}

pub(crate) fn bombardment_source(
    state: &GameState,
    item: &StackItem,
) -> Result<HistoricalDamageSourceV1, String> {
    let paid = payment(state, item.v4.stack_item_id)?;
    if !paid_bombardment_self(&paid.item) || paid.item.as_ref() != item {
        return Err("Bombardment source differs from its actual self-sacrifice".into());
    }
    match &state.engine.event_history[paid.departure as usize] {
        CommittedEvent::FixedDamageDepartureV3 { departure } => Ok(departure.facts),
        _ => Err("Bombardment self-sacrifice departure disappeared".into()),
    }
}
pub(crate) fn has_payment(state: &GameState, id: StackItemId) -> bool {
    state
        .engine
        .event_history
        .scan_for_v1(&[EventKindV1::FixedDamagePaidV3])
        .iter()
        .any(|e| {
            matches!(e,CommittedEvent::FixedDamagePaidV3 {payment}
        if payment.item.v4.stack_item_id==id)
        })
}
pub(crate) fn native_fanatic_damage(
    state: &GameState,
    ctx: &ExecCtx,
    op: &EffectOp,
) -> Result<Option<StackItemId>, String> {
    let Some((source, id)) = ctx.ability_source_contract.zip(ctx.stack_item_id) else {
        return Ok(None);
    };
    if !fanatic(source.card_def, 0) || !has_payment(state, id) {
        return Ok(None);
    }
    let p = payment(state, id)?;
    if !paid_fanatic(&p.item)
        || p.item.v4.ability_source_contract != Some(source)
        || p.item.source != ctx.source
        || p.item.controller != ctx.controller
        || p.item.targets != ctx.targets
        || p.item.v4.target_contracts != ctx.target_contracts
        || *op != printed_root()
    {
        return Err("Fanatic damage differs from its exact paid activation".into());
    }
    Ok(Some(id))
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
        || state
            .engine
            .event_history
            .scan_for_v1(&[EventKindV1::FixedDamageRemovedV3])
            .iter()
            .any(|e| {
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
        .scan_for_v1(&[EventKindV1::CrystalPreventionBoundaryV1])
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
    let Some(last) = rows.last() else {
        if native_ordinary_damage(&payment.item) {
            return Ok(*payment.item.clone());
        }
        return Err("fixed damage resolution has no owned execution".into());
    };
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
        || state
            .engine
            .event_history
            .scan_for_v1(&[EventKindV1::FixedDamageRemovedV3])
            .iter()
            .any(|e| {
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
            || state
                .engine
                .event_history
                .scan_for_v1(&[EventKindV1::CrystalPreventionBoundaryV1])
                .iter()
                .any(|e| {
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
    for (i, e) in state
        .engine
        .event_history
        .scan_for_v1(&[
            EventKindV1::FixedDamageDepartureV3,
            EventKindV1::FixedDamagePaidV3,
            EventKindV1::FixedDamageRemovedV3,
        ])
        .iter()
        .enumerate()
    {
        match e {
            CommittedEvent::FixedDamageDepartureV3{departure}=>{
                if departure.history_index as usize!=i || state.engine.event_history.scan_for_v1(&[EventKindV1::FixedDamagePaidV3]).iter().filter(|e|
                    matches!(e,CommittedEvent::FixedDamagePaidV3{payment} if payment.departure as usize==i)).count()!=1 {
                    return Err("fixed departure lost its unique paid owner".into());
                }
            }
            CommittedEvent::FixedDamagePaidV3{payment:p}=>{
                let p=payment(state,p.item.v4.stack_item_id)?;
                let removed=state.engine.event_history.scan_for_v1(&[EventKindV1::FixedDamageRemovedV3]).iter().filter_map(|e|match e {
                    CommittedEvent::FixedDamageRemovedV3{removal} if removal.payment==p.history_index=>Some(removal),
                    _=>None,
                }).collect::<Vec<_>>();
                let live=state.stack.iter().filter(|i|i.v4.stack_item_id==p.item.v4.stack_item_id).count();
                let disposed=crate::engine::time_stop_v1::disposed(state,&p.item)?;
                match (live,removed.as_slice(),disposed) {
                    (1,[],false)=>{},
                    (0,[],true)=>{},
                    (0,[r],false) if r.history_index>p.history_index && (r.history_index as usize)<state.engine.event_history.len()=>{
                        if r.outcome==OutcomeV3::Resolved {
                            if completed_item(state,p)?!=*r.item || state.engine.event_history[r.history_index as usize..].iter().any(|e|
                                matches!(e,CommittedEvent::CrystalPreventionBoundaryV1{boundary}
                                    if boundary.item.v4.stack_item_id==p.item.v4.stack_item_id)) {
                                return Err("fixed resolved closure changed or preceded execution".into());
                            }
                        } else if r.item!=p.item || state.engine.event_history.scan_for_v1(&[EventKindV1::CrystalPreventionBoundaryV1]).iter().any(|e|
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
    legal_targets_with_controller(state, facts, facts.source.controller)
}

pub(crate) fn legal_targets_with_controller(
    state: &GameState,
    facts: HistoricalDamageSourceV1,
    controller: crate::ids::PlayerId,
) -> Vec<Target> {
    crate::engine::legal_targets_for_controller(TargetSpec::AnyTarget, &[], controller, state)
        .into_iter()
        .filter(|target| match *target {
            Target::Player(_) => true,
            Target::StackItem(_) => false,
            Target::Object(id) => {
                !crate::source_power_activation_v1::protected(state, facts, id)
                    && !crate::source_type_hexproof_v1::protects_from_source(
                        state,
                        id,
                        controller,
                        facts.source,
                    )
                    .unwrap_or(true)
                    && !crate::standard_legends_v1::hexproof_from_historical_color(
                        state,
                        id,
                        facts.color_mask,
                        controller,
                    )
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "fixed_source_damage_v3_tests.rs"]
mod tests;
