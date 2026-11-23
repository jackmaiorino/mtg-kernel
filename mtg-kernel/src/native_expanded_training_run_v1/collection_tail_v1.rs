//! Collection tail (`collection_tail_v1`, an opt-in run setting on top of
//! `update_tail_overlap_v1`): the serial work between a finished collection
//! and the next update's learner runs where its data allow it to.
//!
//! Two parts, both under the one setting:
//!
//! 1. Persistent collection state
//!    (`expanded_deck_training_v1::PersistentCollectionV1`). A run keeps its
//!    learner's template from one collection to the next, so every
//!    collection receives its new learner by the loader's exact restore of
//!    the new checkpoint's bytes (the bytes `update_tail_overlap_v1` hands
//!    over) instead of loading the import from scratch, and checks its fixed
//!    opponents beside that restore. Worker threads, their policies and
//!    opponent caches start fresh in every collection, as without the
//!    setting, so no worker carries anything from one update into the next.
//! 2. Overlap after the collection ([`run_v1`]). Without the setting the
//!    driver pins and validates the collection, publishes `update-input.json`
//!    and `update-command.json`, and only then starts the update, which reads
//!    and checks its inputs and prepares its learner groups before its
//!    learner step. With the setting a helper thread does the validation and
//!    the two publications, in that order, while the update reads and
//!    prepares on the driver's thread from the trajectories the collection's
//!    receipt names. The update waits at its learner gate (before its learner
//!    step, its first write and its first stderr line after the input read)
//!    until the helper has published both documents and its validated
//!    command is byte for byte the one the update runs; its stderr lines and
//!    panic messages until then are held (`held_output_v1`).
//!
//! What stays as it is without the setting:
//!
//! - Bytes. The update is the ordinary update of the ordinary command; the
//!   command it starts with is the one the helper validates and publishes,
//!   compared byte for byte before the learner runs. If they ever differed,
//!   the speculative update is dropped at its gate and the published command
//!   runs as without the setting.
//! - Order and durability. The helper publishes `update-input.json` and then
//!   `update-command.json`, each durable before the next step, and the update
//!   writes nothing before both are durable: so the publications start in the
//!   same order as without the setting and no thread publishes beside them.
//! - Traces. Nothing the update prints before its gate is written until the
//!   gate opens; the helper's own line (the validation's) is written where it
//!   is without the setting, before them.
//! - Failures. A failed validation or publication returns its own error and
//!   drops the update's held output: nothing of the update is written or
//!   printed, as without the setting, where the update never started. An
//!   update that fails before its gate fails only once the helper has
//!   published both documents; its held lines are then written and its own
//!   error returned, as without the setting. A panic of either is resumed
//!   on the driver's thread in the same way.

use crate::expanded_deck_training_v1::{ExpandedTrainingCommandV1, PinnedFileV1};
use crate::held_output_v1::{self, HeldOutputV1};
use std::cell::RefCell;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::{Arc, Condvar, Mutex, PoisonError};

/// The refusal of the setting, at config validation, without
/// `update_tail_overlap_v1`.
pub(super) const VALIDATION_REFUSAL_V1: &str = "collection_tail_v1 requires update_tail_overlap_v1";

/// The error a speculative update's gate returns when its command is not the
/// published one or the helper failed; never returned by [`run_v1`].
const NOT_ADMITTED_V1: &str = "the speculative update was not admitted";

/// What the helper validated and published: the validated trajectories, the
/// seconds their validation took, and the published update command.
pub(super) struct CommittedInputV1 {
    pub(super) trajectories: Vec<PinnedFileV1>,
    pub(super) validation_seconds: f64,
    pub(super) command: ExpandedTrainingCommandV1,
}

/// The trajectories a collection's pinned receipt names, read before the
/// receipt is validated, or `None` when it cannot be read or does not name
/// them (the validation then reports its own error, without the overlap).
pub(super) fn speculative_trajectories_v1(collection: &PinnedFileV1) -> Option<Vec<PinnedFileV1>> {
    let document = super::read_pin(collection).ok()?;
    serde_json::from_value(document.get("trajectories")?.clone()).ok()
}

#[cfg(test)]
thread_local! {
    /// Test-only: updates this thread's drivers ran ahead of their helper and
    /// whose gate admitted them.
    static ADMITTED_FOR_TEST_V1: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Test-only: whether this thread's drivers start each helper only once
    /// the speculative update has reached its gate or returned.
    static HOLD_HELPER_FOR_TEST_V1: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Test-only: learners the drivers on this thread restored from their
    /// persistent collection state's template, counted when a run ends.
    static RESTORES_FOR_TEST_V1: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Test-only: where the held output of the speculative updates this
    /// thread's drivers run goes, instead of stderr.
    static RECORDER_FOR_TEST_V1: RefCell<Option<held_output_v1::RecorderV1>> =
        const { RefCell::new(None) };
}

/// Test-only: how many speculative updates the drivers on this thread ran
/// ahead of their helper and admitted at their gate.
#[cfg(test)]
pub(super) fn admitted_for_test_v1() -> usize {
    ADMITTED_FOR_TEST_V1.with(std::cell::Cell::get)
}

/// Test-only: adds a finished run's template restores to this thread's count.
#[cfg(test)]
pub(super) fn count_restores_for_test_v1(restores: usize) {
    RESTORES_FOR_TEST_V1.with(|count| count.set(count.get() + restores));
}

/// Test-only: how many learners the runs a driver on this thread finished
/// restored from their persistent collection state's template.
#[cfg(test)]
pub(super) fn restores_for_test_v1() -> usize {
    RESTORES_FOR_TEST_V1.with(std::cell::Cell::get)
}

/// Test-only: every helper a driver on this thread starts from now on waits,
/// before it validates anything, until the speculative update has read and
/// prepared its inputs and reached its gate, or has returned. So the whole
/// input read and preparation of every update runs before its collection is
/// validated and its documents published.
#[cfg(test)]
pub(super) fn hold_helper_for_test_v1() {
    HOLD_HELPER_FOR_TEST_V1.with(|hold| hold.set(true));
}

/// Test-only: the held output of every speculative update a driver on this
/// thread runs from now on goes to `recorder` instead of stderr.
#[cfg(test)]
pub(super) fn record_update_output_for_test_v1(recorder: held_output_v1::RecorderV1) {
    RECORDER_FOR_TEST_V1.with(|slot| *slot.borrow_mut() = Some(recorder));
}

/// A speculative update's held output: recorded in a test that asked for it.
fn new_held_output_v1() -> Arc<HeldOutputV1> {
    #[cfg(test)]
    if let Some(recorder) = RECORDER_FOR_TEST_V1.with(|slot| slot.borrow().clone()) {
        return HeldOutputV1::recorded_v1(recorder);
    }
    HeldOutputV1::new_v1()
}

/// Whether the speculative update has reached its gate or returned: a
/// held helper (test-only) waits for it.
struct ReachedV1 {
    reached: Mutex<bool>,
    changed: Condvar,
}

impl ReachedV1 {
    fn set_v1(&self) {
        *self.reached.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.changed.notify_all();
    }

    fn wait_v1(&self) {
        let mut reached = self.reached.lock().unwrap_or_else(PoisonError::into_inner);
        while !*reached {
            reached = self
                .changed
                .wait(reached)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// Runs a finished collection's tail: `helper` (the collection's validation
/// and the publications of `update-input.json` and `update-command.json`, in
/// that order, returning what it published) on its own thread, and `update`
/// on this one, first with `speculative` (the update command built from the
/// trajectories the collection's receipt names) and the learner gate, then,
/// only if the published command differs from it, again with the published
/// command and no gate. Returns what the helper published, or the error (or
/// panic) the run without the setting returns at the same point. See the
/// module documentation.
pub(super) fn run_v1<H, U>(
    helper: H,
    speculative: ExpandedTrainingCommandV1,
    mut update: U,
) -> Result<CommittedInputV1, String>
where
    H: FnOnce() -> Result<CommittedInputV1, String> + Send,
    U: FnMut(
        ExpandedTrainingCommandV1,
        Option<crate::expanded_deck_training_v1::UpdateLearnerGateV1<'_>>,
    ) -> Result<(), String>,
{
    let same_command = |command: &ExpandedTrainingCommandV1| {
        matches!(
            (serde_json::to_vec(command), serde_json::to_vec(&speculative)),
            (Ok(published), Ok(ran)) if published == ran
        )
    };
    #[cfg(test)]
    let hold = HOLD_HELPER_FOR_TEST_V1.with(std::cell::Cell::get);
    #[cfg(not(test))]
    let hold = false;
    let reached = ReachedV1 {
        reached: Mutex::new(false),
        changed: Condvar::new(),
    };
    let slot = Mutex::new(Some(helper));
    std::thread::scope(|scope| {
        let spawned = std::thread::Builder::new()
            .name("expanded-collection-tail".into())
            .spawn_scoped(scope, || {
                if hold {
                    reached.wait_v1();
                }
                let helper = slot
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take()
                    .ok_or("the collection tail helper ran twice")?;
                helper()
            });
        let Ok(handle) = spawned else {
            // No thread: the tail runs in series, as without the setting.
            let helper = slot
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
                .ok_or("the collection tail helper ran twice")?;
            let committed = helper()?;
            update(committed.command.clone(), None)?;
            return Ok(committed);
        };
        let handle = RefCell::new(Some(handle));
        let joined = RefCell::new(None);
        let join = || {
            reached.set_v1();
            if let Some(handle) = handle.borrow_mut().take() {
                *joined.borrow_mut() = Some(handle.join());
            }
        };
        let held = new_held_output_v1();
        let installed = RefCell::new(Some(held_output_v1::install_v1(&held)));
        let outcome = {
            let mut gate = || -> Result<(), String> {
                join();
                let admitted = matches!(
                    joined.borrow().as_ref(),
                    Some(Ok(Ok(committed))) if same_command(&committed.command)
                );
                if !admitted {
                    return Err(NOT_ADMITTED_V1.into());
                }
                // Both documents are durable and the command is the one
                // running: the held lines are written, and every later one
                // at once.
                held.release_v1();
                drop(installed.borrow_mut().take());
                #[cfg(test)]
                ADMITTED_FOR_TEST_V1.with(|admitted| admitted.set(admitted.get() + 1));
                Ok(())
            };
            catch_unwind(AssertUnwindSafe(|| {
                update(speculative.clone(), Some(&mut gate))
            }))
        };
        join();
        let joined = joined
            .into_inner()
            .ok_or("the collection tail helper was not joined")?;
        match joined {
            // The helper's own panic or error, as without the setting, where
            // the update never started: nothing of the update is written.
            Err(panic) => {
                held.discard_v1();
                drop(installed.borrow_mut().take());
                resume_unwind(panic)
            }
            Ok(Err(error)) => {
                held.discard_v1();
                drop(installed.borrow_mut().take());
                Err(error)
            }
            Ok(Ok(committed)) if same_command(&committed.command) => {
                // The update ran the published command: its held lines (if it
                // ended before its gate) are written, and its own result or
                // panic is the run's.
                held.release_v1();
                drop(installed.borrow_mut().take());
                match outcome {
                    Ok(result) => result.map(|()| committed),
                    Err(panic) => resume_unwind(panic),
                }
            }
            Ok(Ok(committed)) => {
                // Never expected (the validation returns the trajectories the
                // receipt names): the speculative update stopped at its gate
                // or before, wrote and printed nothing, and the published
                // command runs as without the setting.
                held.discard_v1();
                drop(installed.borrow_mut().take());
                update(committed.command.clone(), None)?;
                Ok(committed)
            }
        }
    })
}
