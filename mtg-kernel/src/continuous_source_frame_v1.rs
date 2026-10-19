//! Lists of the objects that can supply one scanned continuous effect, kept
//! only while one immutable `GameState` borrow is open.
//!
//! Several layer queries scan every object for the few whose definitions
//! carry one static ability, and public projections repeat those queries for
//! every card and keyword. Inside [`with_frame`] each list is collected once,
//! in the scan's own order, by the definition-only test the scan applies
//! first; a query then visits only the listed objects, which are exactly the
//! ones its scan could count. The frame's borrow keeps that whole state
//! immutable, so the list is the one the scan would see. A query on any other
//! state, outside a frame, or without thread storage runs its original scan.

use crate::ids::ObjectId;
use crate::state::GameState;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceKindV1 {
    ControlledCreatureGrant,
    ControlledCreatureBoost,
    /// One controller's battlefield list, by player index.
    ControlledSubtypeBoost(usize),
    ChosenColorCreatureBoost,
    GarruksUprising,
}

struct Frame {
    state: *const GameState,
    lists: Vec<(SourceKindV1, Rc<[ObjectId]>)>,
}

thread_local! {
    static FRAME: RefCell<Option<Frame>> = const { RefCell::new(None) };
}

/// Restores the enclosing frame (or none) when a newly opened frame ends,
/// including on unwind. A joined frame restores nothing.
struct Restore(Option<Option<Frame>>);
impl Drop for Restore {
    fn drop(&mut self) {
        if let Some(previous) = self.0.take() {
            let _ = FRAME.try_with(|frame| *frame.borrow_mut() = previous);
        }
    }
}

/// Run `run` while `state` stays borrowed. A frame already open on the same
/// state is joined, so its lists stay shared; a frame on another state is set
/// aside until this one ends.
pub(crate) fn with_frame<T>(state: &GameState, run: impl FnOnce() -> T) -> T {
    let opened = FRAME
        .try_with(|frame| {
            let mut frame = frame.borrow_mut();
            if frame
                .as_ref()
                .is_some_and(|open| std::ptr::eq(open.state, state))
            {
                None
            } else {
                Some(frame.replace(Frame {
                    state: std::ptr::from_ref(state),
                    lists: Vec::new(),
                }))
            }
        })
        .ok()
        .flatten();
    let _restore = Restore(opened);
    run()
}

/// The frame's list of `kind` for `state`, collecting it on first use, or
/// `None` when no frame is open on this state. No borrow of the frame is held
/// while `collect` runs.
pub(crate) fn sources(
    state: &GameState,
    kind: SourceKindV1,
    collect: impl FnOnce() -> Vec<ObjectId>,
) -> Option<Rc<[ObjectId]>> {
    let recorded = FRAME
        .try_with(|frame| {
            frame
                .borrow()
                .as_ref()
                .filter(|open| std::ptr::eq(open.state, state))
                .map(|open| {
                    open.lists
                        .iter()
                        .find(|(recorded, _)| *recorded == kind)
                        .map(|(_, list)| Rc::clone(list))
                })
        })
        .ok()
        .flatten()?;
    if recorded.is_some() {
        return recorded;
    }
    let list: Rc<[ObjectId]> = collect().into();
    let _ = FRAME.try_with(|frame| {
        if let Some(open) = frame
            .borrow_mut()
            .as_mut()
            .filter(|open| std::ptr::eq(open.state, state))
        {
            if !open.lists.iter().any(|(recorded, _)| *recorded == kind) {
                open.lists.push((kind, Rc::clone(&list)));
            }
        }
    });
    Some(list)
}
