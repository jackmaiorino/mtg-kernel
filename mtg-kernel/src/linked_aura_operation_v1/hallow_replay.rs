//! Reproduce the damage frontier that actually created an open operation.
//! A versionless capture must reproduce the entire serialized game under one
//! historical rule set, including earlier answers and packet ordinals.
use super::{as_recorded, has_pending, replay_general, GameState, HallowCheckpointV1};
use std::cell::Cell;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Order {
    HallowOnly,
    CircleGlobal,
    SphereOjer,
    Expanded,
}

thread_local! {
    static ORDER: Cell<Order> = const { Cell::new(Order::Expanded) };
}

pub(crate) fn legacy_damage_order_v1() -> bool {
    ORDER.with(|mode| mode.get() != Order::Expanded)
}

pub(crate) fn legacy_hallow_only_v1() -> bool {
    ORDER.with(|mode| mode.get() == Order::HallowOnly)
}

pub(crate) fn legacy_sphere_order_v1() -> bool {
    ORDER.with(|mode| mode.get() == Order::SphereOjer)
}

pub(super) fn stamp_checkpoint() -> bool {
    ORDER.with(|mode| mode.get() == Order::Expanded)
}

impl Order {
    pub(super) fn with<T>(self, f: impl FnOnce() -> T) -> T {
        struct Restore(Order);
        impl Drop for Restore {
            fn drop(&mut self) {
                ORDER.with(|mode| mode.set(self.0));
            }
        }
        let _restore = Restore(ORDER.with(|mode| mode.replace(self)));
        f()
    }
}

pub(super) fn recorded_order(
    state: &GameState,
    pending: &HallowCheckpointV1,
) -> Result<Order, String> {
    let check = |order: Order| {
        order.with(|| {
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
    if pending.expanded_damage_order {
        return check(Order::Expanded);
    }
    check(Order::HallowOnly)
        .or_else(|_| check(Order::CircleGlobal))
        .or_else(|_| check(Order::SphereOjer))
}
