//! Update tail overlap (`update_tail_overlap_v1`, an opt-in run setting): the
//! next iteration's collection plays its games while the finished update is
//! written out and committed.
//!
//! Without the setting, an update's learner step is followed in series by its
//! checkpoint publication and readback, `update.json`, the driver's
//! revalidation of the collection and of the update, and `complete.json`;
//! only then does the next iteration begin. None of that work changes the
//! model. With the setting, the update hands over its checkpoint's pin as
//! soon as the checkpoint's bytes exist in memory
//! (`expanded_deck_training_v1::execute_with_handoff_v1`), and the driver
//! starts the next iteration's collection on its own thread ([`PrefetchV1`])
//! while the update and the driver go on exactly as before on theirs.
//!
//! What stays as it is without the setting:
//!
//! - Bytes. The collection is the ordinary collection of the ordinary
//!   command. It loads its learner through the ordinary loader, which reads
//!   the checkpoint's pin from the bytes being published, never from the file,
//!   while that publication is in flight (`verified_bytes_cache_v1`, pending
//!   publications). Nothing it computes depends on how far the write has got.
//! - Traces. The collection's gate opens only once the finished update is
//!   committed: its `complete.json` is durable and its stdout line printed.
//!   Until then the collection plays its games in memory and leaves no trace:
//!   it creates no directory and writes no file (the write gate of
//!   `phase1_parallel_collection`), and its stderr lines and panic messages
//!   are held in memory (`held_output_v1`), to be written in their order when
//!   the gate opens. So nothing of an iteration exists before the iteration
//!   before it is committed, and every state a crash or a failure leaves is
//!   one a run without the setting can leave.
//! - The order of commits. The driver joins the collection's thread at the
//!   top of the next iteration, after the commit, and only then validates the
//!   collection and runs the next update: an update never starts before the
//!   one before it is committed, and at most one update's write is in flight.
//! - Failures. An update, a revalidation or a receipt that fails returns its
//!   own error, as before. The collection is abandoned before the error leaves
//!   the driver: its thread stops at its next write and is joined, and its
//!   held output is dropped. It wrote nothing, and no receipt names it.
//! - Boundary readings. The driver reads the process CPU and the chain ledger
//!   for a receipt where it reads them without the setting, after the update
//!   is checked. The next collection's threads may be running by then, so a
//!   reading that is cumulative (the process CPU, the ledger's counter) can
//!   include the work they have done so far.
//! - Stops. The handoff starts a collection only for an iteration the loop
//!   will run (the next index is in the schedule and the invocation limit is
//!   not reached), decided before any boundary reading, so the last update of
//!   a run has no collection beside it and reads exactly as without the
//!   setting. A chain ledger stop is decided on the readings, so a run whose
//!   ledger carries a stop value is refused ([`LEDGER_STOP_REFUSAL_V1`]): at
//!   admission, before any work, and at every boundary, before its receipt.
//!   A ledger-stopped run stays serial.
//!
//! The one difference a failure can show: when an update fails after its
//! handoff, games of the next collection may already have been played. Their
//! results are dropped with their thread; nothing of them is written or
//! printed.

use super::{check, err, publish, NativeExpandedTrainingRunV1};
use crate::expanded_deck_training_v1::{execute_collection_gated_v1, ExpandedTrainingCommandV1};
use crate::held_output_v1::{self, HeldOutputV1};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread::JoinHandle;
use std::time::Instant;

/// The name of a new iteration's first attempt, as the driver numbers it.
pub(super) const FIRST_ATTEMPT_V1: &str = "attempt-000000";

/// The refusal of the setting, at config validation, outside the runs it
/// serves: parallel collection and a prepared CPU update of an ordinary run.
pub(super) const VALIDATION_REFUSAL_V1: &str =
    "update_tail_overlap_v1 requires parallel collection and a prepared CPU update, \
     without line (b), E, SQ1, search targets, EX1 or an S learner";

/// The refusal of the setting for a run whose chain ledger carries a stop
/// value.
pub(super) const LEDGER_STOP_REFUSAL_V1: &str =
    "update_tail_overlap_v1 is refused when the chain ledger carries a stop value \
     (stop_at_usec): a ledger-stopped run stays serial";

/// The collection's model load and dispatch run on this thread (its games
/// run on the collection's own workers), where without the setting they run
/// on the driver's thread: at least that thread's stack on every system.
const PREFETCH_STACK_BYTES_V1: usize = 16 * 1024 * 1024;

/// Refuses the setting, before any work, for a run whose chain ledger source
/// carries a stop value: the stop value lives in that file, not in the
/// config, so this is the first check that can see it.
pub(super) fn refuse_ledger_stop_v1(config: &NativeExpandedTrainingRunV1) -> Result<(), String> {
    match (&config.chain_ledger, config.update_tail_overlap_v1) {
        (Some(ledger), true) => check(ledger.stop_at_usec_v1()?.is_none(), LEDGER_STOP_REFUSAL_V1),
        _ => Ok(()),
    }
}

#[cfg(test)]
thread_local! {
    /// Test-only: collections this thread's drivers took from a prefetch.
    static COLLECTED_FOR_TEST_V1: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Test-only: whether this thread's drivers hold each update at its
    /// handoff until the collection it started waits at its gate.
    static HOLD_AT_GATE_FOR_TEST_V1: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Test-only: where the held output of the collections this thread's
    /// drivers start goes, instead of stderr.
    static RECORDER_FOR_TEST_V1: std::cell::RefCell<Option<held_output_v1::RecorderV1>> =
        const { std::cell::RefCell::new(None) };
}

/// Test-only: how many collections the drivers run on this thread took from
/// a prefetch, so a test can tell an overlapped run from a serial one.
#[cfg(test)]
pub(super) fn collected_for_test_v1() -> usize {
    COLLECTED_FOR_TEST_V1.with(std::cell::Cell::get)
}

/// Test-only: every update a driver on this thread runs from now on waits at
/// its handoff, before its checkpoint's first write, until the collection it
/// started has played a game and asked to write it (or has ended). So every
/// later step of the update and of its commit meets a collection that holds
/// played games at its closed gate.
#[cfg(test)]
pub(super) fn hold_at_gate_for_test_v1() {
    HOLD_AT_GATE_FOR_TEST_V1.with(|hold| hold.set(true));
}

/// Test-only: the held output of every collection a driver on this thread
/// starts from now on goes to `recorder` instead of stderr.
#[cfg(test)]
pub(super) fn record_held_output_for_test_v1(recorder: held_output_v1::RecorderV1) {
    RECORDER_FOR_TEST_V1.with(|slot| *slot.borrow_mut() = Some(recorder));
}

/// Test-only: the driver's hold ([`hold_at_gate_for_test_v1`]), when armed.
#[cfg(test)]
pub(super) fn held_at_gate_for_test_v1(prefetch: &PrefetchV1) {
    if HOLD_AT_GATE_FOR_TEST_V1.with(std::cell::Cell::get) {
        let mut state = prefetch.shared.lock_v1();
        while state.asked == 0 && !state.ended {
            state = prefetch
                .shared
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// A new collection's held output: recorded in a test that asked for it.
fn new_held_output_v1() -> Arc<HeldOutputV1> {
    #[cfg(test)]
    if let Some(recorder) = RECORDER_FOR_TEST_V1.with(|slot| slot.borrow().clone()) {
        return HeldOutputV1::recorded_v1(recorder);
    }
    HeldOutputV1::new_v1()
}

/// Where the collection's gate stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GateV1 {
    /// The finished update is not committed yet: every write waits.
    Closed,
    /// The finished update is committed: writes go ahead.
    Open,
    /// The update, a revalidation or a receipt failed, or the driver is
    /// leaving: every write is refused.
    Abandoned,
}

struct GateStateV1 {
    gate: GateV1,
    /// Test-only: writes asked for so far (the test hold waits for the first).
    #[cfg(test)]
    asked: usize,
    /// Test-only: whether the collection's thread has ended.
    #[cfg(test)]
    ended: bool,
}

/// What the driver and the collection's thread share.
struct SharedV1 {
    state: Mutex<GateStateV1>,
    changed: Condvar,
    /// The collection's stderr lines and panic messages until its commit.
    held: Arc<HeldOutputV1>,
}

impl SharedV1 {
    fn lock_v1(&self) -> MutexGuard<'_, GateStateV1> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set_v1(&self, gate: GateV1) {
        self.lock_v1().gate = gate;
        self.changed.notify_all();
    }

    /// Asked before every write: waits while the gate is closed; then whether
    /// it is open.
    fn admit_v1(&self) -> bool {
        let mut state = self.lock_v1();
        #[cfg(test)]
        {
            state.asked += 1;
            self.changed.notify_all();
        }
        while state.gate == GateV1::Closed {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        state.gate == GateV1::Open
    }
}

/// Test-only: marks the collection's thread as ended when it drops, also on
/// a panic, for the test hold.
#[cfg(test)]
struct EndedV1(Arc<SharedV1>);

#[cfg(test)]
impl Drop for EndedV1 {
    fn drop(&mut self) {
        self.0.lock_v1().ended = true;
        self.0.changed.notify_all();
    }
}

/// What became of the iteration's directory, its first attempt and the
/// attempt's published command: the driver's steps before a collection.
enum OpenedV1 {
    Published,
    /// The iteration's directory could not be created new. Nothing was
    /// written; the driver runs the iteration itself, as without the setting.
    NotNew,
    /// The driver's own error, as without the setting.
    Failed(String),
}

/// A finished prefetch.
enum PrefetchedV1 {
    /// The collection is published in the iteration's first attempt.
    Collected { attempt: PathBuf, seconds: f64 },
    /// [`OpenedV1::NotNew`].
    NotNew,
}

/// The next iteration's collection, running on its own thread from the
/// finished update's handoff. Dropping it abandons the collection, drops its
/// held output and joins its thread, so on every way out of the driver (an
/// error, a panic) no collection outlives the run, writes or prints after it.
pub(super) struct PrefetchV1 {
    index: usize,
    shared: Arc<SharedV1>,
    thread: Option<JoinHandle<Result<Option<PrefetchedV1>, String>>>,
}

impl PrefetchV1 {
    /// Starts the collection `command` of iteration `index`, whose directory
    /// `directory` must not exist yet, behind a closed gate. `None` when its
    /// thread cannot start: the driver then runs the iteration itself.
    pub(super) fn start_v1(
        index: usize,
        directory: PathBuf,
        command: ExpandedTrainingCommandV1,
    ) -> Option<Self> {
        let shared = Arc::new(SharedV1 {
            state: Mutex::new(GateStateV1 {
                gate: GateV1::Closed,
                #[cfg(test)]
                asked: 0,
                #[cfg(test)]
                ended: false,
            }),
            changed: Condvar::new(),
            held: new_held_output_v1(),
        });
        let thread_shared = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name(format!("expanded-prefetch-{index}"))
            .stack_size(PREFETCH_STACK_BYTES_V1)
            .spawn(move || {
                #[cfg(test)]
                let _ended = EndedV1(Arc::clone(&thread_shared));
                let _held = held_output_v1::install_v1(&thread_shared.held);
                collect_v1(&thread_shared, directory, command)
            })
            .ok()?;
        Some(Self {
            index,
            shared,
            thread: Some(thread),
        })
    }

    /// The iteration this collection belongs to.
    pub(super) fn index_v1(&self) -> usize {
        self.index
    }

    /// The finished update is committed: the collection's held output is
    /// written, in its order, and then the collection may write.
    pub(super) fn committed_v1(&self) {
        self.shared.held.release_v1();
        self.shared.set_v1(GateV1::Open);
    }

    /// Waits for the collection, once the finished update is committed. The
    /// attempt it collected in and its seconds; `None` when it wrote nothing
    /// and the driver runs the iteration itself; or the error the driver or
    /// the collection would have returned without the setting.
    pub(super) fn join_v1(mut self) -> Result<Option<(PathBuf, f64)>, String> {
        let Some(thread) = self.thread.take() else {
            return Ok(None);
        };
        match thread.join() {
            Ok(Ok(Some(PrefetchedV1::Collected { attempt, seconds }))) => {
                #[cfg(test)]
                COLLECTED_FOR_TEST_V1.with(|collected| collected.set(collected.get() + 1));
                Ok(Some((attempt, seconds)))
            }
            Ok(Ok(Some(PrefetchedV1::NotNew))) => Ok(None),
            // Only an abandoned collection ends without a result, and only
            // dropping this handle abandons one.
            Ok(Ok(None)) => Err("the collection prefetch was abandoned".into()),
            Ok(Err(error)) => Err(error),
            // As a panic of the collection on the driver's own thread (its
            // message went through the held output).
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
}

impl Drop for PrefetchV1 {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.shared.held.discard_v1();
            self.shared.set_v1(GateV1::Abandoned);
            let _ = thread.join();
        }
    }
}

/// The collection thread: the driver's steps before a collection (the
/// iteration's directory, its first attempt, the published command) and the
/// collection itself, with every filesystem write behind the gate.
fn collect_v1(
    shared: &SharedV1,
    directory: PathBuf,
    command: ExpandedTrainingCommandV1,
) -> Result<Option<PrefetchedV1>, String> {
    let started = Instant::now();
    let attempt = directory.join(FIRST_ATTEMPT_V1);
    let published = command.clone();
    let opened = OnceLock::new();
    let open = || {
        if fs::create_dir(&directory).is_err() {
            return OpenedV1::NotNew;
        }
        match fs::create_dir(&attempt)
            .map_err(err)
            .and_then(|()| publish(&attempt, "collect-command.json", &published))
        {
            Ok(_) => OpenedV1::Published,
            Err(error) => OpenedV1::Failed(error),
        }
    };
    // Asked before every write of the collection: the first admitted one
    // makes the driver's steps, once, and an abandoned collection writes no
    // more.
    let gate = || {
        if !shared.admit_v1() {
            return Err("the collection prefetch was abandoned".to_owned());
        }
        match opened.get_or_init(&open) {
            OpenedV1::Published => Ok(()),
            OpenedV1::NotNew => Err("the iteration directory is not new".to_owned()),
            OpenedV1::Failed(error) => Err(error.clone()),
        }
    };
    let collected = execute_collection_gated_v1(command, &gate);
    // Without the setting the command is published before the collection
    // runs, so it also is when the collection failed before its first write.
    let _ = gate();
    let seconds = started.elapsed().as_secs_f64();
    match opened.get() {
        None => Ok(None),
        Some(OpenedV1::NotNew) => Ok(Some(PrefetchedV1::NotNew)),
        Some(OpenedV1::Failed(error)) => Err(error.clone()),
        Some(OpenedV1::Published) => {
            collected.map(|_| Some(PrefetchedV1::Collected { attempt, seconds }))
        }
    }
}
