//! Update tail overlap (`update_tail_overlap_v1`, an opt-in run setting): the
//! next iteration's collection starts while the finished update is written
//! out.
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
//! - The order of commits. The collection thread writes nothing until the
//!   finished update's `update.json` is durable, so until then a crash leaves
//!   the tree it leaves without the setting. From then on the thread may
//!   write what an interrupted collection writes (the iteration's directory,
//!   its first attempt, the attempt's command and trajectories), which a
//!   resume takes as an interrupted attempt of an iteration whose input
//!   checkpoint is published. The driver joins the thread only after the
//!   finished update's `complete.json` is published, and only then validates
//!   the collection and runs the next update: an update never starts before
//!   the one before it is committed, and at most one update's write is in
//!   flight.
//! - Failures. An update or a receipt that fails returns its own error, as
//!   before. The collection thread is then told, stops at its next write and
//!   is joined before the error leaves the driver; if the failure came before
//!   `update.json` was durable, it has written nothing. Its result is dropped
//!   and no receipt names it.
//! - Stops. The handoff decides whether a next iteration runs by the rule
//!   the loop applies after the commit (the invocation limit and the chain
//!   ledger's stop value), from the ledger reading the receipt then records
//!   ([`BoundaryV1`]). No collection starts for an iteration the run does not
//!   run.
//!
//! The one difference a failure can show: when an update fails after its
//! handoff, games of the next collection may already have been played. They
//! are discarded with their thread, as above.

use super::{
    err, publish, ChainLedgerOptionsV1, ChainLedgerReadingV1, NativeExpandedTrainingRunV1,
};
use crate::expanded_deck_training_v1::{execute_collection_gated_v1, ExpandedTrainingCommandV1};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};
use std::thread::JoinHandle;
use std::time::Instant;

/// The name of a new iteration's first attempt, as the driver numbers it.
pub(super) const FIRST_ATTEMPT_V1: &str = "attempt-000000";

/// The collection's model load and dispatch run on this thread (its games
/// run on the collection's own workers), where without the setting they run
/// on the driver's thread: at least that thread's stack on every system.
const PREFETCH_STACK_BYTES_V1: usize = 16 * 1024 * 1024;

#[cfg(test)]
thread_local! {
    /// Test-only: collections this thread's drivers took from a prefetch.
    static COLLECTED_FOR_TEST_V1: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Test-only: how many collections the drivers run on this thread took from
/// a prefetch, so a test can tell an overlapped run from a serial one.
#[cfg(test)]
pub(super) fn collected_for_test_v1() -> usize {
    COLLECTED_FOR_TEST_V1.with(std::cell::Cell::get)
}

/// An update's boundary telemetry (the driver's process CPU and the chain
/// ledger), as its receipt records it. With the setting it is read at the
/// handoff, after the learner step and before the next collection may start,
/// so it never counts the next iteration's games, and the reading that
/// decides whether the run stops is the one the receipt records.
pub(super) struct BoundaryV1 {
    /// `Some` exactly when the run records its process CPU.
    pub(super) process_cpu: Result<Option<f64>, String>,
    /// `Some` exactly when the run has a chain ledger.
    pub(super) ledger: Result<Option<ChainLedgerReadingV1>, String>,
}

impl BoundaryV1 {
    /// The receipt's readings, in the receipt's order.
    pub(super) fn read_v1(config: &NativeExpandedTrainingRunV1) -> Self {
        let process_cpu = if config.record_process_cpu {
            crate::regret_census_v1::stage4a::process_cpu::seconds().map(Some)
        } else {
            Ok(None)
        };
        // As in the receipt: a failed process CPU reading ends the iteration
        // before the ledger is read.
        let ledger = match &process_cpu {
            Ok(_) => config
                .chain_ledger
                .as_ref()
                .map(ChainLedgerOptionsV1::read_v1)
                .transpose(),
            Err(_) => Ok(None),
        };
        Self {
            process_cpu,
            ledger,
        }
    }

    /// Whether the run goes on after this boundary: both readings succeeded
    /// (a failed one ends the run at the receipt) and the ledger has not
    /// reached its stop value (the loop's own rule).
    pub(super) fn continues_v1(&self) -> bool {
        self.process_cpu.is_ok()
            && match &self.ledger {
                Ok(None) => true,
                Ok(Some(reading)) => {
                    // The loop's own test after the commit.
                    let stops = reading
                        .stop_at_usec
                        .is_some_and(|stop| reading.usec >= stop);
                    !stops
                }
                Err(_) => false,
            }
    }
}

/// Where the finished update's write stands, as the collection thread sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TailV1 {
    /// The checkpoint or `update.json` is still being written.
    Writing,
    /// `update.json` is durable.
    Durable,
    /// The update or a receipt failed, or the driver is leaving.
    Abandoned,
}

struct TailSignalV1 {
    state: Mutex<TailV1>,
    changed: Condvar,
}

impl TailSignalV1 {
    fn set_v1(&self, state: TailV1) {
        *self.state.lock().unwrap_or_else(PoisonError::into_inner) = state;
        self.changed.notify_all();
    }

    /// Waits while the write is in flight; then whether it is durable.
    fn durable_v1(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        while *state == TailV1::Writing {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        *state == TailV1::Durable
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
/// finished update's handoff. Dropping it abandons the collection and joins
/// its thread, so on every way out of the driver (an error, a panic) no
/// collection outlives the run or writes after it.
pub(super) struct PrefetchV1 {
    index: usize,
    signal: Arc<TailSignalV1>,
    thread: Option<JoinHandle<Result<Option<PrefetchedV1>, String>>>,
}

impl PrefetchV1 {
    /// Starts the collection `command` of iteration `index`, whose directory
    /// `directory` must not exist yet. `None` when its thread cannot start:
    /// the driver then runs the iteration itself.
    pub(super) fn start_v1(
        index: usize,
        directory: PathBuf,
        command: ExpandedTrainingCommandV1,
    ) -> Option<Self> {
        let signal = Arc::new(TailSignalV1 {
            state: Mutex::new(TailV1::Writing),
            changed: Condvar::new(),
        });
        let thread_signal = Arc::clone(&signal);
        let thread = std::thread::Builder::new()
            .name(format!("expanded-prefetch-{index}"))
            .stack_size(PREFETCH_STACK_BYTES_V1)
            .spawn(move || collect_v1(&thread_signal, directory, command))
            .ok()?;
        Some(Self {
            index,
            signal,
            thread: Some(thread),
        })
    }

    /// The iteration this collection belongs to.
    pub(super) fn index_v1(&self) -> usize {
        self.index
    }

    /// The finished update's `update.json` is durable: the collection may
    /// write.
    pub(super) fn durable_v1(&self) {
        self.signal.set_v1(TailV1::Durable);
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
            // As a panic of the collection on the driver's own thread.
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
}

impl Drop for PrefetchV1 {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.signal.set_v1(TailV1::Abandoned);
            let _ = thread.join();
        }
    }
}

/// The collection thread: the driver's steps before a collection (the
/// iteration's directory, its first attempt, the published command) and the
/// collection itself, with every filesystem write behind the tail's signal.
fn collect_v1(
    signal: &TailSignalV1,
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
        if !signal.durable_v1() {
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
