//! The printed graveyard permission is separate from every granted permission.
use super::*;
use crate::effect::greatwurm_v1::{self as native, UnitV1};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceV1 {
    /// None while the two actual routes are offered; true is the printed cost.
    pub printed_route: Option<bool>,
    pub units: Vec<UnitV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaidV1 {
    pub source: StackSourceContractV4,
    pub stack_item_id: StackItemId,
    pub units: [UnitV1; 6],
    pub original_counts: [u32; 6],
}
pub(super) fn allowed(state: &GameState, id: ObjectId, player: PlayerId) -> bool {
    state.objects.try_get(id).is_some_and(|o| {
        o.zone == Zone::Graveyard
            && o.owner == player
            && !o.v4.is_token
            && o.spell_copy_origin.is_none()
            && o.v4.face_down_v1.is_none()
            && native::definition(o.card_def)
    }) && enough(state, player, &[])
}
fn enough(state: &GameState, player: PlayerId, prefix: &[UnitV1]) -> bool {
    let Ok(candidates) = native::units(state, player, prefix) else {
        return false;
    };
    let mut available = 0_u32;
    for unit in candidates {
        let Ok(count) = native::count(state, unit.permanent.object, unit.kind) else {
            return false;
        };
        let used = prefix.iter().filter(|old| **old == unit).count() as u32;
        available = available.saturating_add(count.saturating_sub(used));
    }
    available >= 6_u32.saturating_sub(prefix.len() as u32)
}
pub(super) fn seed(state: &GameState, id: ObjectId, player: PlayerId) -> Option<ChoiceV1> {
    allowed(state, id, player).then(|| ChoiceV1 {
        printed_route: if graveyard_permission_zone_change_count(player, id, state).is_some() {
            None
        } else {
            Some(true)
        },
        units: vec![],
    })
}
pub(super) fn waiting(p: &PendingCast) -> bool {
    p.greatwurm_v1.as_ref().is_some_and(|c| {
        c.printed_route.is_none() || c.printed_route == Some(true) && c.units.len() < 6
    })
}
pub(super) fn options(state: &GameState, p: &PendingCast) -> Result<u16, String> {
    let c = p.greatwurm_v1.as_ref().ok_or("Greatwurm stage absent")?;
    if c.printed_route.is_none() {
        return Ok(2);
    }
    if c.printed_route != Some(true) || c.units.len() >= 6 {
        return Err("Greatwurm selection complete".into());
    }
    u16::try_from(native::units(state, p.controller, &c.units)?.len())
        .map_err(|_| "counter menu width exceeded".into())
}
pub(super) fn validate_pending(state: &GameState, p: &PendingCast) -> Result<(), String> {
    let printed = p.source_contract.spell_cast_origin.is_some_and(|origin| {
        matches!(origin.route, SpellCastRouteV4::GreatwurmGraveyardV1 { .. })
    });
    let Some(c) = &p.greatwurm_v1 else {
        return if printed {
            Err("Greatwurm stage erased".into())
        } else {
            Ok(())
        };
    };
    let origin = p
        .source_contract
        .spell_cast_origin
        .ok_or("cast origin absent")?;
    if !native::definition(p.source_contract.card_def)
        || p.origin_zone != Zone::Graveyard
        || origin.origin_zone != Zone::Graveyard
        || p.source_contract.owner != p.controller
        || p.source_contract.cast_method != CastMethodV4::Normal
        || c.units.len() > 6
        || (c.printed_route != Some(false)) != printed
        || c.printed_route != Some(true) && !c.units.is_empty()
    {
        return Err("Greatwurm route/stage changed".into());
    }
    if c.printed_route.is_none()
        && !crate::standard_cards_v1::graveyard_cast_granted(
            state,
            p.spell,
            p.controller,
            origin.origin_zone_change_count,
        )
    {
        return Err("alternative graveyard permission changed".into());
    }
    for (i, unit) in c.units.iter().enumerate() {
        if !native::units(state, p.controller, &c.units[..i])?.contains(unit) {
            return Err("counter cost incarnation/controller/kind/prefix changed".into());
        }
    }
    if c.printed_route != Some(false)
        && (!enough(state, p.controller, &c.units) || c.units.len() < 6 && options(state, p)? == 0)
    {
        return Err("six-counter cost no longer payable".into());
    }
    Ok(())
}
pub(super) fn answer(state: &mut GameState, p: &PendingCast, index: u16) -> Result<(), String> {
    validate_pending(state, p)?;
    let c = p.greatwurm_v1.as_ref().ok_or("counter stage absent")?;
    if c.printed_route.is_none() {
        if index > 1 {
            return Err("graveyard route index outside menu".into());
        }
        let printed = index == 1;
        let mut contract = p.source_contract;
        let origin = contract
            .spell_cast_origin
            .as_mut()
            .ok_or("cast origin absent")?;
        origin.route = if printed {
            SpellCastRouteV4::GreatwurmGraveyardV1 {
                holder: p.controller,
                graveyard_generation: origin.origin_zone_change_count,
            }
        } else {
            SpellCastRouteV4::GraveyardPermissionV1 {
                holder: p.controller,
                permission_zone_change_count: origin.origin_zone_change_count,
            }
        };
        state.objects.get_mut(p.spell).v4.spell_cast_origin = contract.spell_cast_origin;
        state
            .stack
            .last_mut()
            .ok_or("cast placeholder absent")?
            .v4
            .source_contract = Some(contract);
        let live = state
            .engine
            .pending_cast
            .as_mut()
            .ok_or("cast pending absent")?;
        live.source_contract = contract;
        live.greatwurm_v1.as_mut().unwrap().printed_route = Some(printed);
        return Ok(());
    }
    if c.printed_route != Some(true) || c.units.len() >= 6 {
        return Err("counter stage complete".into());
    }
    let unit = *native::units(state, p.controller, &c.units)?
        .get(usize::from(index))
        .ok_or("counter cost index outside menu")?;
    state
        .engine
        .pending_cast
        .as_mut()
        .ok_or("cast pending absent")?
        .greatwurm_v1
        .as_mut()
        .unwrap()
        .units
        .push(unit);
    Ok(())
}
/// Nonserializable, consumed only by the complete spell payment.
pub(super) struct PaymentV1 {
    units: [UnitV1; 6],
}
impl PaymentV1 {
    pub(super) fn apply(&self, state: &mut GameState) {
        for unit in self.units {
            native::remove_unit(state, unit);
        }
    }
}
pub(super) fn prepare(state: &GameState, p: &PendingCast) -> Option<PaymentV1> {
    let c = p.greatwurm_v1.as_ref()?;
    if c.printed_route != Some(true) {
        return None;
    }
    validate_pending(state, p).ok()?;
    Some(PaymentV1 {
        units: c.units.clone().try_into().ok()?,
    })
}
pub(super) fn record_input(state: &mut GameState, p: &PendingCast) -> Option<u64> {
    let plan = prepare(state, p)?;
    let original_counts = plan
        .units
        .map(|u| native::count(state, u.permanent.object, u.kind).expect("prepared count"));
    let paid = PaidV1 {
        source: p.source_contract,
        stack_item_id: state
            .stack
            .last()
            .expect("validated placeholder")
            .v4
            .stack_item_id,
        units: plan.units,
        original_counts,
    };
    let index = u64::try_from(state.engine.event_history.len()).expect("native history width");
    state
        .engine
        .event_history
        .push(CommittedEvent::GreatwurmCounterInputV1 {
            input: Box::new(paid),
        });
    Some(index)
}
pub(super) fn record_paid(
    state: &mut GameState,
    input_index: Option<u64>,
) -> Result<Option<u64>, String> {
    let Some(input_history_index) = input_index else {
        return Ok(None);
    };
    let i = usize::try_from(input_history_index).map_err(|_| "counter input width")?;
    let Some(CommittedEvent::GreatwurmCounterInputV1 { input }) = state.engine.event_history.get(i)
    else {
        return Err("counter input missing".into());
    };
    let paid = input.clone();
    for (n, unit) in paid.units.iter().enumerate() {
        let used = u32::try_from(paid.units.iter().filter(|u| *u == unit).count())
            .map_err(|_| "unit width")?;
        let object = state
            .objects
            .try_get(unit.permanent.object)
            .ok_or("paid counter object missing")?;
        if object.zone != unit.permanent.expected_zone
            || object.zone_change_count != unit.permanent.expected_zone_change_count
            || object.controller != paid.source.controller
            || native::count(state, unit.permanent.object, unit.kind)?
                != paid.original_counts[n]
                    .checked_sub(used)
                    .ok_or("counter input overspent")?
        {
            return Err("actual counter subtraction differs from original selected cost".into());
        }
    }
    let index =
        u64::try_from(state.engine.event_history.len()).map_err(|_| "counter paid width")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::GreatwurmCounterPaymentV1 {
            paid,
            input_history_index,
        });
    Ok(Some(index))
}
fn validated_paid(
    state: &GameState,
    i: usize,
    expected: Option<(StackSourceContractV4, StackItemId)>,
) -> Result<&PaidV1, String> {
    let Some(CommittedEvent::GreatwurmCounterPaymentV1 {
        paid,
        input_history_index,
    }) = state.engine.event_history.get(i)
    else {
        return Err("counter payment receipt erased".into());
    };
    let input = usize::try_from(*input_history_index).map_err(|_| "counter input width")?;
    if input>=i || !matches!(state.engine.event_history.get(input),Some(CommittedEvent::GreatwurmCounterInputV1{input})if input==paid)
        || state.engine.event_history.iter().filter(|e|matches!(e,CommittedEvent::GreatwurmCounterInputV1{input}if input.stack_item_id==paid.stack_item_id)).count()!=1 {
        return Err("counter payment differs from its original selected input".into());
    }
    if expected.is_some_and(|(source, item)| paid.source != source || paid.stack_item_id != item)
        || state.engine.event_history.iter().filter(|e| matches!(e, CommittedEvent::GreatwurmCounterPaymentV1{paid:p,..} if p.stack_item_id == paid.stack_item_id)).count() != 1 {
        return Err("counter payment source/controller/receipt binding changed".into());
    }
    for (n, unit) in paid.units.iter().enumerate() {
        let used = paid.units[..=n].iter().filter(|u| *u == unit).count() as u32;
        if unit.permanent.expected_zone != Zone::Battlefield
            || paid.original_counts[n] < used
            || paid.units.iter().enumerate().any(|(m, other)| {
                other == unit && paid.original_counts[m] != paid.original_counts[n]
            })
        {
            return Err("counter payment units/counts changed".into());
        }
    }
    Ok(paid)
}

/// Chronological debit for the existing bait ledger. This shares the original
/// independent selection/payment checks, never trusts the live counter alone,
/// and does not supply spell execution or permission authority.
pub(crate) fn bait_cost_balance(
    state: &GameState,
    at: usize,
    object: ObjectId,
    generation: u32,
) -> Result<Option<(i32, i32)>, String> {
    if !matches!(state.engine.event_history.get(at), Some(CommittedEvent::GreatwurmCounterPaymentV1 { paid, .. })
        if paid.units.iter().any(|unit| unit.kind == native::CounterKindV1::Bait
            && unit.permanent.object == object && unit.permanent.expected_zone_change_count == generation))
    {
        return Ok(None);
    }
    let paid = validated_paid(state, at, None)?;
    let origin = paid
        .source
        .spell_cast_origin
        .ok_or("bait cost cast origin absent")?;
    if !native::definition(paid.source.card_def)
        || paid.source.zone != Zone::Stack
        || paid.source.owner != paid.source.controller
        || paid.source.spell_copy_origin.is_some()
        || paid.source.cast_method != CastMethodV4::Normal
        || paid.source.finalized_cast_binding.is_some()
        || origin.origin_zone != Zone::Graveyard
        || origin.finalized_method.is_some()
        || !matches!(origin.route, SpellCastRouteV4::GreatwurmGraveyardV1 { holder, graveyard_generation }
            if holder == paid.source.controller && graveyard_generation == origin.origin_zone_change_count)
    {
        return Err("bait cost lost original printed graveyard source".into());
    }
    // The selected input and payment are independent cost records, but their
    // equality alone cannot authenticate a jointly relabeled physical caster.
    // The real finalized cast publishes its own identity after payment.
    let live = state
        .objects
        .try_get(paid.source.source)
        .ok_or("bait cost physical caster absent")?;
    if live.owner != paid.source.owner
        || live.zone_change_count < paid.source.zone_change_count
        || (live.zone_change_count == paid.source.zone_change_count
            && (live.zone != Zone::Stack || live.card_def != paid.source.card_def))
        || origin.origin_zone_change_count.checked_add(1) != Some(paid.source.zone_change_count)
    {
        return Err("bait cost physical caster incarnation changed".into());
    }
    let publications = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            CommittedEvent::PublishedCastIdentityV1 { receipt }
                if receipt.stack_item_id == paid.stack_item_id
                    || (receipt.spell == paid.source.source
                        && receipt.stack_zone_change_count == paid.source.zone_change_count) =>
            {
                Some((index, receipt))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(published, receipt)] = publications.as_slice() else {
        return Err("bait cost lost unique original cast publication".into());
    };
    let cast_at = published
        .checked_add(1)
        .ok_or("bait cast history index overflow")?;
    if at >= *published
        || receipt.stack_item_id != paid.stack_item_id
        || receipt.spell != paid.source.source
        || receipt.owner != paid.source.owner
        || receipt.controller != paid.source.controller
        || receipt.origin_zone != origin.origin_zone
        || receipt.origin_zone_change_count != origin.origin_zone_change_count
        || receipt.stack_zone_change_count != paid.source.zone_change_count
        || receipt.public_identity
            != (crate::cast_identity_v1::PublicIdentityV1::Known {
                card_def: paid.source.card_def,
            })
        || receipt.spell_cast_history_index
            != u64::try_from(cast_at).map_err(|_| "bait cast history index width")?
        || state.engine.event_history.get(cast_at)
            != Some(&CommittedEvent::SpellCast {
                spell: paid.source.source,
                controller: paid.source.controller,
            })
    {
        return Err("bait cost differs from actual finalized cast identity".into());
    }
    let mut before = None;
    let mut removed = 0_i32;
    for (n, unit) in paid.units.iter().enumerate() {
        if unit.kind == native::CounterKindV1::Bait
            && unit.permanent.object == object
            && unit.permanent.expected_zone_change_count == generation
        {
            before = Some(i32::try_from(paid.original_counts[n]).map_err(|_| "bait input width")?);
            removed += 1;
        }
    }
    before
        .map(|before| {
            let after = before
                .checked_sub(removed)
                .filter(|after| *after >= 0)
                .ok_or("bait cost overspent original count")?;
            Ok((before, after))
        })
        .transpose()
}

pub(super) fn validate_finalized(
    state: &GameState,
    item: &StackItem,
    pending: bool,
) -> Result<(), String> {
    let source = item.v4.source_contract.ok_or("cast source missing")?;
    let paid_index = source
        .finalized_cast_binding
        .and_then(|b| b.greatwurm_payment_v1);
    let printed = source
        .spell_cast_origin
        .is_some_and(|o| matches!(o.route, SpellCastRouteV4::GreatwurmGraveyardV1 { .. }));
    if pending {
        return if paid_index.is_none() {
            Ok(())
        } else {
            Err("pending counter payment forged".into())
        };
    }
    if !printed {
        return if paid_index.is_none() {
            Ok(())
        } else {
            Err("counter payment on unrelated route".into())
        };
    }
    let i = usize::try_from(paid_index.ok_or("printed graveyard cost receipt missing")?)
        .map_err(|_| "counter receipt index width")?;
    let mut expected = source;
    expected.finalized_cast_binding = None;
    expected
        .spell_cast_origin
        .as_mut()
        .ok_or("cast origin missing")?
        .finalized_method = None;
    validated_paid(state, i, Some((expected, item.v4.stack_item_id)))?;
    Ok(())
}
