//! Reproduce the damage frontier that actually created an open operation.
//! A versionless capture must reproduce the entire serialized game under one
//! historical rule set, including earlier answers and packet ordinals.
use super::{as_recorded, has_pending, replay_general, GameState, HallowCheckpointV1};
use serde::{Deserialize, Serialize};
use std::cell::Cell;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(super) enum Order {
    HallowOnly,
    CircleGlobal,
    StampedCircleGlobal,
    SphereOjer,
    Expanded,
    SoulScar,
}

thread_local! {
    static ORDER: Cell<(Order, Option<Order>)> = const {
        Cell::new((Order::SoulScar, Some(Order::SoulScar)))
    };
}

pub(crate) fn legacy_damage_order_v1() -> bool {
    ORDER.with(|mode| !matches!(mode.get().0, Order::Expanded | Order::SoulScar))
}

pub(crate) fn soul_scar_damage_order_v1() -> bool {
    cfg!(feature = "eternal-fixtures") && ORDER.with(|mode| mode.get().0 == Order::SoulScar)
}

fn soul_scar_present(state: &GameState) -> bool {
    let Some(id) = crate::card_def::card_id_by_name(crate::eternal_cube_c7b_v1::SOUL_SCAR_MAGE)
    else {
        return false;
    };
    state
        .objects
        .iter()
        .any(|(_, object)| object.card_def == id)
}

pub(super) fn checkpoint_marker(original: &GameState, checkpoint: &GameState) -> Option<Order> {
    if !cfg!(feature = "eternal-fixtures") {
        return None;
    }
    match ORDER.with(|mode| mode.get().1)? {
        Order::SoulScar => (soul_scar_present(original) || soul_scar_present(checkpoint))
            .then_some(Order::SoulScar),
        other => Some(other),
    }
}

pub(crate) fn legacy_hallow_only_v1() -> bool {
    ORDER.with(|mode| mode.get().0 == Order::HallowOnly)
}

pub(crate) fn legacy_sphere_order_v1() -> bool {
    ORDER.with(|mode| mode.get().0 == Order::SphereOjer)
}

pub(super) fn stamp_checkpoint() -> bool {
    !legacy_damage_order_v1()
}

pub(super) fn stamp_circle_global() -> bool {
    ORDER.with(|mode| mode.get().0 == Order::StampedCircleGlobal)
}

impl Order {
    pub(super) fn with<T>(self, marker: Option<Order>, f: impl FnOnce() -> T) -> T {
        struct Restore((Order, Option<Order>));
        impl Drop for Restore {
            fn drop(&mut self) {
                ORDER.with(|mode| mode.set(self.0));
            }
        }
        let _restore = Restore(ORDER.with(|mode| mode.replace((self, marker))));
        f()
    }
}

pub(super) fn recorded_order(
    state: &GameState,
    pending: &HallowCheckpointV1,
) -> Result<Order, String> {
    let marker = state.engine.linked_aura_operation_v1.damage_order;
    let check = |order: Order| {
        order.with(marker, || {
            as_recorded(|| {
                let (expected, _) = replay_general(
                    &pending.original,
                    &pending.operation,
                    &pending.answers,
                    pending.used_banner,
                )?;
                if expected == *state && has_pending(&expected) {
                    Ok(order)
                } else {
                    Err(
                        "Hallow checkpoint changed its original operation or beneficiary menu"
                            .into(),
                    )
                }
            })
        })
    };
    if let Some(order) = marker {
        return check(order);
    }
    if pending.expanded_damage_order {
        return check(Order::Expanded);
    }
    if pending.circle_global_order {
        return check(Order::StampedCircleGlobal);
    }
    check(Order::HallowOnly)
        .or_else(|_| check(Order::CircleGlobal))
        .or_else(|_| check(Order::SphereOjer))
}
