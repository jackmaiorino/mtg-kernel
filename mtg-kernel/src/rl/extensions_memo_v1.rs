//! Exact memo of the policy observation extensions inside one scoring call
//! (speed lever 31, slice 1).
//!
//! One V4 scoring request (`FlatDecisionEncoderV2::build_scoring_owned_v4`)
//! computes `policy_observation_extensions_conqueror_inner_v49_fake_v50` with
//! the same arguments three times: for the action slice, inside the V6
//! observation, and for the V7 extensions. The function is a pure read of an
//! immutable `&GameState`, so the second and third calls can return a clone
//! of the first result (an `Err` included) at the same program point.
//!
//! Exactness argument (the same frame discipline as
//! `continuous_source_frame_v1`):
//! - [`with_extensions_memo_v1`] runs its closure while one shared borrow of
//!   the `GameState` is held, so no `&mut` to that state can exist and it
//!   cannot change while the memo is open.
//! - An entry is used only for the same state object (address equality while
//!   the borrow is alive: no other live object can share the address) and the
//!   same full argument tuple ([`ExtensionsKeyV1`]). Calls with a proof
//!   argument are never memoized.
//! - The memo is dropped when the closure returns (including on unwind); a
//!   memo already open on another state is set aside and restored, one on the
//!   same state is joined.
//!
//! `MTG_KERNEL_OBS_MEMO=0` turns the memo off. `MTG_KERNEL_OBS_MEMO_CHECK=1`
//! recomputes on every hit and panics if the kept value differs.

use std::cell::RefCell;

use super::{HistoricalSourcesV47, ObservationTextModeV2, Result};
use crate::ids::PlayerId;
use crate::policy_observation_v6::PolicyObservationExtensionsV6;
use crate::state::GameState;

/// Bound on distinct argument tuples kept in one memo (one is expected).
const MAX_ENTRIES_V1: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ExtensionsKeyV1 {
    pub(super) acting_player: PlayerId,
    pub(super) text_mode: ObservationTextModeV2,
    pub(super) accept_life_ward: bool,
    pub(super) accept_fishing: bool,
    pub(super) accepted_sources: HistoricalSourcesV47,
    pub(super) accept_conqueror: bool,
    pub(super) accept_fake: bool,
}

struct MemoV1 {
    state: *const GameState,
    entries: Vec<(ExtensionsKeyV1, Result<PolicyObservationExtensionsV6>)>,
}

thread_local! {
    static MEMO_V1: RefCell<Option<MemoV1>> = const { RefCell::new(None) };
}

#[cfg(test)]
thread_local! {
    static TEST_COUNTS_V1: std::cell::Cell<(u64, u64)> = const { std::cell::Cell::new((0, 0)) };
    static TEST_DISABLED_V1: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static TEST_CHECK_V1: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn memo_enabled_v1() -> bool {
    #[cfg(test)]
    if TEST_DISABLED_V1.with(std::cell::Cell::get) {
        return false;
    }
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("MTG_KERNEL_OBS_MEMO").map_or(true, |v| v != "0"))
}

fn check_enabled_v1() -> bool {
    #[cfg(test)]
    if TEST_CHECK_V1.with(std::cell::Cell::get) {
        return true;
    }
    static CHECK: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CHECK.get_or_init(|| std::env::var("MTG_KERNEL_OBS_MEMO_CHECK").is_ok_and(|v| v == "1"))
}

/// Restores the enclosing memo (or none) when a newly opened memo ends,
/// including on unwind. A joined memo restores nothing.
struct Restore(Option<Option<MemoV1>>);
impl Drop for Restore {
    fn drop(&mut self) {
        if let Some(previous) = self.0.take() {
            let _ = MEMO_V1.try_with(|memo| *memo.borrow_mut() = previous);
        }
    }
}

/// Run `run` with the extensions memo open on `state`, which stays borrowed
/// for the whole call.
pub(crate) fn with_extensions_memo_v1<T>(state: &GameState, run: impl FnOnce() -> T) -> T {
    let opened = if memo_enabled_v1() {
        MEMO_V1
            .try_with(|memo| {
                let mut memo = memo.borrow_mut();
                if memo
                    .as_ref()
                    .is_some_and(|open| std::ptr::eq(open.state, state))
                {
                    None
                } else {
                    Some(memo.replace(MemoV1 {
                        state: std::ptr::from_ref(state),
                        entries: Vec::new(),
                    }))
                }
            })
            .ok()
            .flatten()
    } else {
        None
    };
    let _restore = Restore(opened);
    run()
}

/// The kept result for (`state`, `key`) in the open memo, or `compute()`.
/// No borrow of the memo is held while `compute` runs.
pub(super) fn memoized_v1(
    state: &GameState,
    key: ExtensionsKeyV1,
    compute: impl Fn() -> Result<PolicyObservationExtensionsV6>,
) -> Result<PolicyObservationExtensionsV6> {
    let kept = MEMO_V1
        .try_with(|memo| {
            memo.borrow()
                .as_ref()
                .filter(|open| std::ptr::eq(open.state, state))
                .and_then(|open| {
                    open.entries
                        .iter()
                        .find(|(entry, _)| *entry == key)
                        .map(|(_, value)| value.clone())
                })
        })
        .ok()
        .flatten();
    if let Some(value) = kept {
        #[cfg(test)]
        TEST_COUNTS_V1.with(|c| c.set((c.get().0 + 1, c.get().1)));
        if check_enabled_v1() {
            let fresh = compute();
            assert!(
                fresh == value,
                "policy observation extensions memo differs from a rebuild: key {key:?}"
            );
        }
        return value;
    }
    let value = compute();
    #[cfg(test)]
    TEST_COUNTS_V1.with(|c| c.set((c.get().0, c.get().1 + 1)));
    let _ = MEMO_V1.try_with(|memo| {
        if let Some(open) = memo
            .borrow_mut()
            .as_mut()
            .filter(|open| std::ptr::eq(open.state, state))
        {
            if open.entries.len() < MAX_ENTRIES_V1 && !open.entries.iter().any(|(e, _)| *e == key) {
                open.entries.push((key, value.clone()));
            }
        }
    });
    value
}

/// Test controls: (hits, misses) since the last call, and switches.
#[cfg(test)]
pub(crate) mod test_controls_v1 {
    use super::{TEST_CHECK_V1, TEST_COUNTS_V1, TEST_DISABLED_V1};

    pub(crate) fn take_counts() -> (u64, u64) {
        TEST_COUNTS_V1.with(|c| c.replace((0, 0)))
    }

    pub(crate) fn set_disabled(disabled: bool) {
        TEST_DISABLED_V1.with(|c| c.set(disabled));
    }

    pub(crate) fn set_check(check: bool) {
        TEST_CHECK_V1.with(|c| c.set(check));
    }
}
