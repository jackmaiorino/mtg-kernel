//! Replay the damage frontier that recorded the open operation.
use super::{as_recorded, has_pending, replay_general, GameState, HallowCheckpointV1};
use std::cell::Cell;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Order {
    HallowOnly,
    CircleGlobal,
    StampedCircleGlobal,
}

thread_local! {
    static ORDER: Cell<Order> = const { Cell::new(Order::StampedCircleGlobal) };
}

pub(crate) fn legacy_hallow_only_v1() -> bool {
    ORDER.with(|mode| mode.get() == Order::HallowOnly)
}

pub(super) fn stamp_circle_global() -> bool {
    ORDER.with(|mode| mode.get() == Order::StampedCircleGlobal)
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
    if pending.circle_global_order {
        return check(Order::StampedCircleGlobal);
    }
    // Unstamped W1 saves may share the first menu with later rules, so try
    // their original frontier first before recognizing unstamped Circle saves.
    check(Order::HallowOnly).or_else(|_| check(Order::CircleGlobal))
}
