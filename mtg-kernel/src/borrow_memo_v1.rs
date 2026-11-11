//! Answers derived from one immutably borrowed `GameState`, remembered while
//! that borrow lasts.
//!
//! A validator enters a [`ScopeV1`] for the state it borrows. The state cannot
//! change while the scope lives, so a pure function of it gives the same
//! answer every time it is asked during that scope. [`cached`] remembers such
//! answers in the innermost scope, and only for the same state. A nested scope
//! for the same state shares the outer answers. A scope for another state
//! hides them until it ends, and its own answers go with it. Without a scope
//! for the state, every call computes its answer as before.

use crate::event::CommittedEvent;
use crate::event_history_v1::EventKindV1;
use crate::state::GameState;
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;

struct MemoV1 {
    state: *const GameState,
    slots: HashMap<TypeId, Box<dyn Any>>,
}

thread_local! {
    static MEMO_V1: RefCell<Option<MemoV1>> = const { RefCell::new(None) };
}

/// Keeps a memo for one borrowed state until dropped, restoring any memo of
/// another state that it replaced. It holds the borrow, so the state cannot
/// change or move while its answers are kept.
#[must_use]
pub(crate) struct ScopeV1<'a>(Option<Option<MemoV1>>, PhantomData<&'a GameState>);

impl<'a> ScopeV1<'a> {
    pub(crate) fn enter(state: &'a GameState) -> Self {
        MEMO_V1.with(|memo| {
            let mut memo = memo.borrow_mut();
            if memo
                .as_ref()
                .is_some_and(|memo| std::ptr::eq(memo.state, state))
            {
                return Self(None, PhantomData);
            }
            Self(
                Some(memo.replace(MemoV1 {
                    state,
                    slots: HashMap::new(),
                })),
                PhantomData,
            )
        })
    }
}

impl Drop for ScopeV1<'_> {
    fn drop(&mut self) {
        if let Some(outer) = self.0.take() {
            MEMO_V1.with(|memo| *memo.borrow_mut() = outer);
        }
    }
}

/// Whether a scope for `state` is the innermost one.
pub(crate) fn active(state: &GameState) -> bool {
    MEMO_V1.with(|memo| {
        memo.borrow()
            .as_ref()
            .is_some_and(|memo| std::ptr::eq(memo.state, state))
    })
}

/// One remembered function: `M` names it, `K` is its argument, `V` its answer.
struct SlotV1<M, K, V>(HashMap<K, V>, PhantomData<fn() -> M>);

/// `compute`'s answer for `key`, remembered under `M` while a scope for
/// `state` lives. `compute` must depend only on `state` and `key`. It runs
/// with no memo borrowed, so it may itself use remembered answers.
pub(crate) fn cached<M: 'static, K: Eq + Hash + 'static, V: Clone + 'static>(
    state: &GameState,
    key: K,
    compute: impl FnOnce() -> V,
) -> V {
    let found = MEMO_V1.with(|memo| {
        let memo = memo.borrow();
        let memo = memo
            .as_ref()
            .filter(|memo| std::ptr::eq(memo.state, state))?;
        memo.slots
            .get(&TypeId::of::<SlotV1<M, K, V>>())?
            .downcast_ref::<SlotV1<M, K, V>>()?
            .0
            .get(&key)
            .cloned()
    });
    if let Some(found) = found {
        return found;
    }
    let value = compute();
    MEMO_V1.with(|memo| {
        let mut memo = memo.borrow_mut();
        let Some(memo) = memo.as_mut().filter(|memo| std::ptr::eq(memo.state, state)) else {
            return;
        };
        if let Some(slot) = memo
            .slots
            .entry(TypeId::of::<SlotV1<M, K, V>>())
            .or_insert_with(|| Box::new(SlotV1::<M, K, V>(HashMap::new(), PhantomData)))
            .downcast_mut::<SlotV1<M, K, V>>()
        {
            slot.0.insert(key, value.clone());
        }
    });
    value
}

/// The history rows that `pick` accepts, with their positions. With `index`,
/// a remembered index of the state, they are the rows at the positions that
/// `indexed` reads from it; without one, the rows a scan of `kinds` yields.
pub(crate) fn rows_v1<'a, I, T>(
    state: &'a GameState,
    index: Option<std::rc::Rc<I>>,
    indexed: impl FnOnce(&I) -> Option<&Vec<usize>>,
    kinds: &[EventKindV1],
    mut pick: impl FnMut(usize, &'a CommittedEvent) -> Option<T>,
) -> Vec<T> {
    match index {
        Some(index) => indexed(&index)
            .into_iter()
            .flatten()
            .filter_map(|at| pick(*at, state.engine.event_history.get(*at)?))
            .collect(),
        None => state
            .engine
            .event_history
            .of_kinds_v2(kinds)
            .filter_map(|(at, event)| pick(at, event))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct CountV1;

    fn counted(state: &GameState, key: u32, runs: &Cell<u32>) -> u32 {
        cached::<CountV1, u32, u32>(state, key, || {
            runs.set(runs.get() + 1);
            key + state.turn
        })
    }

    #[test]
    fn answers_are_kept_only_while_a_scope_for_the_same_state_lives() {
        let first = GameState::new_from_libraries(&[], &[], |_| String::new(), 0);
        let mut second = first.clone();
        second.turn = first.turn + 7;
        let runs = Cell::new(0);

        let base = first.turn;
        assert_eq!(counted(&first, 1, &runs), base + 1);
        assert_eq!(counted(&first, 1, &runs), base + 1);
        assert_eq!(runs.get(), 2, "no scope: every call computes");

        {
            let _scope = ScopeV1::enter(&first);
            assert!(active(&first) && !active(&second));
            assert_eq!(counted(&first, 1, &runs), base + 1);
            assert_eq!(counted(&first, 1, &runs), base + 1);
            assert_eq!(runs.get(), 3, "scope: the answer is remembered");
            {
                let _same = ScopeV1::enter(&first);
                assert_eq!(counted(&first, 1, &runs), base + 1);
                assert_eq!(runs.get(), 3, "a nested scope shares the outer answers");
            }
            {
                let _other = ScopeV1::enter(&second);
                assert!(!active(&first) && active(&second));
                assert_eq!(counted(&first, 1, &runs), base + 1);
                assert_eq!(runs.get(), 4, "another state's scope hides them");
                assert_eq!(counted(&second, 1, &runs), base + 8);
                assert_eq!(counted(&second, 1, &runs), base + 8);
                assert_eq!(runs.get(), 5);
            }
            assert_eq!(counted(&first, 1, &runs), base + 1);
            assert_eq!(runs.get(), 5, "the outer scope is restored");
        }
        assert!(!active(&first));
        assert_eq!(counted(&first, 1, &runs), base + 1);
        assert_eq!(runs.get(), 6, "answers end with their scope");
    }
}
