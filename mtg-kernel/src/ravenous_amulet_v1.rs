//! Prepayment custody for the unregistered Ravenous Amulet candidate.
//! This records actual engine publication, not resolution or Full admission.
use crate::{
    card_def::{
        ActivationTargetFilter, CardType, CostComponent, PermanentFilter, TargetSpec, CARD_DEFS,
    },
    effect::{EffectOp, PlayerRef},
    engine::PendingActivation,
    event::CommittedEvent,
    ids::StackItemId,
    mana::Cost,
    state::{AbilitySourceContractV4, GameState, StackItem, StackItemKind, StackStateV4, Zone},
};
use serde::{Deserialize, Serialize};

pub fn draw_program() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        EffectOp::AddSoulCounterToAbilitySourceV1,
    ])
}
pub fn loss_program() -> EffectOp {
    EffectOp::LoseLifeEachOpponentEqualSourceSoulV1
}

fn definition(id: u16) -> bool {
    let Some(d) = CARD_DEFS.get(usize::from(id)) else {
        return false;
    };
    let [draw, loss] = d.activated_abilities else {
        return false;
    };
    let mana = |generic| Cost {
        pips: &[],
        generic,
        x_count: 0,
    };
    d.is_executable()
        && d.name == "Ravenous Amulet"
        && d.object_name == d.name
        && d.types == [CardType::Artifact]
        && d.colors.is_empty()
        && d.subtypes.is_empty()
        && d.supertypes.is_empty()
        && !d.is_land
        && !d.is_token
        && d.cost == mana(2)
        && d.mana_value == 2
        && d.power.is_none()
        && d.toughness.is_none()
        && draw.cost
            == [
                CostComponent::Mana(mana(1)),
                CostComponent::Tap,
                CostComponent::SacrificeControlled {
                    count: 1,
                    filter: PermanentFilter::Creature,
                },
            ]
        && loss.cost
            == [
                CostComponent::Mana(mana(4)),
                CostComponent::Tap,
                CostComponent::SacrificeSelf,
            ]
        && (draw.effect)() == draw_program()
        && (loss.effect)() == loss_program()
        && draw.sorcery_speed_only
        && !loss.sorcery_speed_only
        && [draw, loss].iter().all(|a| {
            a.activation_zone == Zone::Battlefield
                && a.target_spec == TargetSpec::None
                && a.activation_target_filter == ActivationTargetFilter::TargetSpecOnly
                && a.max_activations_per_turn.is_none()
                && a.face.is_none()
        })
}

/// Never serialized or reconstructed from a pending snapshot. The payment
/// branch obtains this before moving its cost objects, then consumes it once.
pub(crate) struct PaymentOwnerV1 {
    source: AbilitySourceContractV4,
    sacrifice: AbilitySourceContractV4,
    ability_index: u8,
    original_soul_count: i32,
    history_start: usize,
}

pub(crate) fn capture_payment(
    state: &GameState,
    pending: &PendingActivation,
) -> Result<Option<PaymentOwnerV1>, String> {
    let live = state
        .objects
        .try_get(pending.source)
        .ok_or("activation source absent")?;
    let Some(d) = CARD_DEFS.get(usize::from(live.card_def)) else {
        return Ok(None);
    };
    if d.name != "Ravenous Amulet"
        || pending.ability_index > 1
        || pending.cauldron_grant.0.is_some()
    {
        return Ok(None);
    }
    if !definition(live.card_def)
        || live.zone != Zone::Battlefield
        || live.zone_change_count != pending.source_zone_change_count
        || live.controller != pending.controller
        || live.tapped
        || live.counters.soul < 0
        || pending.target_spec != TargetSpec::None
        || !pending.targets_chosen.is_empty()
        || !pending.target_contracts.is_empty()
    {
        return Err("Amulet changed its original printed payment input".into());
    }
    let sacrifice = if pending.ability_index == 0 {
        let [chosen] = pending.object_cost_chosen.as_slice() else {
            return Err("Amulet needs one creature cost".into());
        };
        let creature = state
            .objects
            .try_get(chosen.object)
            .ok_or("Amulet creature cost absent")?;
        if chosen.expected_zone != Zone::Battlefield
            || creature.zone != Zone::Battlefield
            || creature.zone_change_count != chosen.expected_zone_change_count
            || creature.controller != pending.controller
            || !crate::engine::object_has_type(state, chosen.object, CardType::Creature)
        {
            return Err("Amulet creature payment changed incarnation/controller/type".into());
        }
        AbilitySourceContractV4::capture(state, chosen.object)
    } else {
        if !pending.object_cost_chosen.is_empty() {
            return Err("Amulet self cost gained a chosen object".into());
        }
        AbilitySourceContractV4::capture(state, pending.source)
    };
    Ok(Some(PaymentOwnerV1 {
        source: AbilitySourceContractV4::capture(state, pending.source),
        sacrifice,
        ability_index: pending.ability_index,
        original_soul_count: live.counters.soul,
        history_start: state.engine.event_history.len(),
    }))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaidV1 {
    pub index: u64,
    pub original_source: AbilitySourceContractV4,
    pub original_sacrifice: AbilitySourceContractV4,
    pub original_soul_count: i32,
    pub payment_history_start: u64,
    pub tap_history_index: u64,
    pub sacrificed_history_index: u64,
    pub sacrifice_history_index: u64,
    pub item: Box<StackItem>,
}

fn printed_item(item: &StackItem) -> bool {
    item.v4
        .activated_ability_index
        .is_some_and(|index| index < 2)
        && item.v4.cauldron_grant.0.is_none()
        && item.v4.granted_by.is_none()
        && item
            .v4
            .ability_source_contract
            .is_some_and(|source| definition(source.card_def))
}

/// Historical payment structure is checked without following current source
/// control or a returned incarnation. This is not an operation-completion proof.
fn validate_paid(state: &GameState, paid: &PaidV1) -> Result<(), String> {
    let item = paid.item.as_ref();
    let source = paid.original_source;
    let sacrifice = paid.original_sacrifice;
    let [cost] = item.v4.paid_cost_refs.as_slice() else {
        return Err("Amulet receipt lost its single sacrifice".into());
    };
    let mut published_source = source;
    if sacrifice.source == source.source {
        published_source.attached_to = None;
    }
    let role = item
        .v4
        .activated_ability_index
        .ok_or("Amulet receipt lost printed index")?;
    let expected = StackItem {
        kind: StackItemKind::ActivatedAbility,
        source: source.source,
        controller: source.controller,
        targets: Vec::new(),
        is_copy: false,
        inline_effect: Some(if role == 0 {
            draw_program()
        } else {
            loss_program()
        }),
        discarded: Vec::new(),
        is_flashback: false,
        mode_chosen: 0,
        madness_offer: false,
        kicked: false,
        v4: StackStateV4 {
            stack_item_id: item.v4.stack_item_id,
            paid_cost_refs: item.v4.paid_cost_refs.clone(),
            ability_source_contract: Some(published_source),
            target_spec: Some(TargetSpec::None),
            activated_ability_index: Some(role),
            ..Default::default()
        },
    };
    if !printed_item(item)
        || item != &expected
        || item.v4.stack_item_id == StackItemId::default()
        || source.zone != Zone::Battlefield
        || sacrifice.zone != Zone::Battlefield
        || sacrifice.controller != source.controller
        || paid.original_soul_count < 0
        || (role == 1 && sacrifice != source)
        || (sacrifice.source == source.source && sacrifice != source)
        || cost.object != sacrifice.source
        || cost.owner != sacrifice.owner
        || sacrifice.zone_change_count.checked_add(1) != Some(cost.zone_change_count)
        || cost.zone == Zone::Battlefield
        || !(paid.payment_history_start <= paid.tap_history_index
            && paid.tap_history_index < paid.sacrificed_history_index
            && paid.sacrificed_history_index < paid.sacrifice_history_index
            && paid.sacrifice_history_index < paid.index)
    {
        return Err("Amulet payment receipt changed its printed source/root/cost".into());
    }
    let at = |index| {
        usize::try_from(index)
            .ok()
            .and_then(|i| state.engine.event_history.get(i))
    };
    if !matches!(at(paid.index), Some(CommittedEvent::RavenousPaymentV1 { paid: original }) if original.as_ref() == paid)
        || !matches!(at(paid.tap_history_index), Some(CommittedEvent::Tap { object }) if *object == source.source)
        || !matches!(at(paid.sacrificed_history_index), Some(CommittedEvent::Sacrificed { object, controller_before, .. }) if *object == sacrifice.source && *controller_before == source.controller)
        || !matches!(at(paid.sacrifice_history_index), Some(CommittedEvent::ZoneChange { object, from: Zone::Battlefield, to, controller_before }) if *object == sacrifice.source && *to == cost.zone && *controller_before == source.controller)
    {
        return Err("Amulet payment receipt lost its actual indexed cost occurrences".into());
    }
    // A self-sacrificed original generation has stable LKI even after return.
    // Other-creature payments may later gain more soul counters legitimately.
    if source.source == sacrifice.source
        && state.soul_count_for_ability_source_v1(source)? != paid.original_soul_count
    {
        return Err("Amulet self-sacrifice rewrote its original soul count".into());
    }
    Ok(())
}

pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    let mut ids = Vec::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::RavenousPaymentV1 { paid } = event else {
            continue;
        };
        if paid.index != index as u64 || ids.contains(&paid.item.v4.stack_item_id) {
            return Err("Amulet payment has duplicate/reordered authority".into());
        }
        validate_paid(state, paid)?;
        ids.push(paid.item.v4.stack_item_id);
        let live = state
            .stack
            .iter()
            .filter(|item| item.v4.stack_item_id == paid.item.v4.stack_item_id)
            .collect::<Vec<_>>();
        if !live.is_empty() && live.as_slice() != [paid.item.as_ref()] {
            return Err("Amulet current activation changed its whole paid row".into());
        }
    }
    for item in &state.stack {
        if printed_item(item) && !ids.contains(&item.v4.stack_item_id) {
            return Err("Amulet current activation lost its actual payment receipt".into());
        }
    }
    if state
        .engine
        .event_log
        .iter()
        .any(|event| matches!(event, CommittedEvent::RavenousPaymentV1 { .. }))
    {
        return Err("private Amulet payment entered trigger log".into());
    }
    Ok(())
}

/// Called after the generic engine has successfully collected the entire cost.
/// Payment failure never calls this, and a copied/borrowed activation cannot
/// manufacture the private original source/count token.
pub(crate) fn record_paid(
    state: &mut GameState,
    id: StackItemId,
    owner: Option<PaymentOwnerV1>,
) -> Result<(), String> {
    let mut published = state.stack.iter().filter(|i| i.v4.stack_item_id == id);
    let item = published.next().ok_or("paid activation absent")?;
    if published.next().is_some() {
        return Err("paid activation has duplicate finalized identities".into());
    }
    let selected = printed_item(item);
    let Some(owner) = owner else {
        return if selected {
            Err("Amulet lost private prepayment capture".into())
        } else {
            Ok(())
        };
    };
    let mut source = owner.source;
    if state.objects.get(source.source).zone_change_count != source.zone_change_count {
        source.attached_to = None;
    }
    let program = if owner.ability_index == 0 {
        draw_program()
    } else {
        loss_program()
    };
    let [cost] = item.v4.paid_cost_refs.as_slice() else {
        return Err("Amulet lost its single paid sacrifice".into());
    };
    if !selected
        || item.kind != StackItemKind::ActivatedAbility
        || item.is_copy
        || item.source != owner.source.source
        || item.controller != owner.source.controller
        || item.v4.ability_source_contract != Some(source)
        || item.v4.activated_ability_index != Some(owner.ability_index)
        || item.inline_effect != Some(program)
        || !item.targets.is_empty()
        || !item.v4.target_contracts.is_empty()
        || item.v4.target_spec != Some(TargetSpec::None)
        || item.v4.granted_by.is_some()
        || item.v4.cauldron_grant.0.is_some()
        || id == StackItemId::default()
        || cost.object != owner.sacrifice.source
        || cost.owner != owner.sacrifice.owner
        || owner.sacrifice.zone_change_count.checked_add(1) != Some(cost.zone_change_count)
        || cost.zone == Zone::Battlefield
        || state.soul_count_for_ability_source_v1(owner.source)? != owner.original_soul_count
    {
        return Err("Amulet finalized publication changed its private payment".into());
    }
    let mut taps = Vec::new();
    let mut markers = Vec::new();
    let mut sacrifices = Vec::new();
    for (offset, event) in state.engine.event_history[owner.history_start..]
        .iter()
        .enumerate()
    {
        match event {
            CommittedEvent::Tap { object } if *object == owner.source.source => {
                taps.push(owner.history_start + offset)
            }
            CommittedEvent::Sacrificed {
                object,
                controller_before,
                ..
            } if *object == owner.sacrifice.source
                && *controller_before == owner.source.controller =>
            {
                markers.push(owner.history_start + offset)
            }
            CommittedEvent::ZoneChange {
                object,
                from: Zone::Battlefield,
                to,
                controller_before,
            } if *object == owner.sacrifice.source
                && *to == cost.zone
                && *controller_before == owner.source.controller =>
            {
                sacrifices.push(owner.history_start + offset)
            }
            _ => {}
        }
    }
    let ([tap], [marker], [sacrifice]) =
        (taps.as_slice(), markers.as_slice(), sacrifices.as_slice())
    else {
        return Err("Amulet payment lost unique tap/departure".into());
    };
    if tap >= marker || marker >= sacrifice {
        return Err("Amulet sacrifice preceded its tap cost".into());
    }
    let paid = PaidV1 {
        index: state.engine.event_history.len() as u64,
        original_source: owner.source,
        original_sacrifice: owner.sacrifice,
        original_soul_count: owner.original_soul_count,
        payment_history_start: owner.history_start as u64,
        tap_history_index: *tap as u64,
        sacrificed_history_index: *marker as u64,
        sacrifice_history_index: *sacrifice as u64,
        item: Box::new(item.clone()),
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::RavenousPaymentV1 {
            paid: Box::new(paid),
        });
    Ok(())
}
