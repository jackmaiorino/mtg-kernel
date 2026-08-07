//! Bounded-staleness async inference: an opt-in overlap of rollout episode
//! generation with trainer weight publication.
//!
//! Reference: `BOUNDED-STALENESS-DECISION-DRAFT.md` (collab root) and the
//! adopt-with-conditions stance recorded as `CLAUDE #191` in
//! `collab/TO-CODEX.md`. The three binding conditions from that message:
//!
//! 1. Hybrid architecture: the fully synchronous trainer path is retained
//!    unchanged (structural default, condition (a)); evaluation panels stay
//!    synchronous CRN and are untouched by this module (condition (b)).
//! 2. A matched A/B validation gate must exist before async results feed any
//!    formal claim (condition (d); see `bounded_staleness_async_harness_v1`).
//! 3. Typed, explicit, opt-in configuration; never the default (condition (c)).
//!
//! This module supplies the mechanism the decision package calls "the cheap
//! part, not the scheduler": a generic bounded-staleness producer/consumer
//! scheduler, a per-episode staleness ledger, and the typed config plus
//! honest run-metadata/reproducibility-claim records. What it deliberately
//! does **not** do is reach into the existing evidence-chain training
//! executor (`native_training_executor_v1`, `native_training_store_prepared_
//! segment_v2`): that integration is the decision package's "expensive part"
//! (episode provenance schema, replay/evidence-chain adaptation) and is left
//! for a follow-up with its own frozen sheet and countersign, per condition
//! 3 of CLAUDE #191. See the top-level implementation report for the exact
//! boundary.
//!
//! ## Staleness bound, precisely
//!
//! An episode batch is dispatched for production carrying two version
//! numbers fixed at dispatch time and never revised afterward:
//! `scoring_weight_version` (the trainer update count already published when
//! production of this batch began) and `consuming_update_version` (the
//! trainer update index this batch's data is destined to train). Staleness
//! is `consuming_update_version - scoring_weight_version`. [`
//! BoundedStalenessSchedulerV1`] enforces the declared bound `K`
//! structurally: it never dispatches a batch whose staleness-at-dispatch
//! would already exceed `K`, and because both version numbers are fixed at
//! dispatch, staleness cannot grow afterward while production runs. [`
//! StalenessLedgerV1`] independently re-checks every consumed batch as a
//! defensive, fail-closed second gate.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// Typed selection between the structural default (fully synchronous,
/// byte-identical to every prior commit's trainer behavior) and the opt-in
/// bounded-staleness async path. `Default` is [`Self::SynchronousV1`];
/// nothing in this crate selects the async variant implicitly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrainingExecutionModeV1 {
    SynchronousV1,
    BoundedStalenessAsyncV1 { max_staleness_updates: u32 },
}

impl Default for TrainingExecutionModeV1 {
    fn default() -> Self {
        Self::SynchronousV1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrainingExecutionModeValidationErrorV1 {
    /// `max_staleness_updates == 0` degrades to synchronous scheduling (a
    /// producer may never run ahead) while still paying the async plumbing's
    /// nondeterminism cost and mislabeling the run as async. Callers who
    /// want zero overlap should select `SynchronousV1` explicitly.
    ZeroStalenessBound,
}

impl TrainingExecutionModeV1 {
    pub fn validate_v1(self) -> Result<(), TrainingExecutionModeValidationErrorV1> {
        match self {
            Self::SynchronousV1 => Ok(()),
            Self::BoundedStalenessAsyncV1 {
                max_staleness_updates,
            } => {
                if max_staleness_updates == 0 {
                    Err(TrainingExecutionModeValidationErrorV1::ZeroStalenessBound)
                } else {
                    Ok(())
                }
            }
        }
    }

    pub const fn is_synchronous_v1(self) -> bool {
        matches!(self, Self::SynchronousV1)
    }

    pub const fn max_staleness_updates_v1(self) -> Option<u32> {
        match self {
            Self::SynchronousV1 => None,
            Self::BoundedStalenessAsyncV1 {
                max_staleness_updates,
            } => Some(max_staleness_updates),
        }
    }
}

/// Honest determinism claim strings for run metadata. Async runs must say,
/// in their own record, that they are not bit-reproducible; the exact
/// mechanism is documented so a reader knows what evidence trail to use
/// instead of seed replay.
pub const SYNCHRONOUS_REPRODUCIBILITY_CLAIM_V1: &str =
    "bit-reproducible: synchronous mode is structurally the pre-existing \
     trainer path, unmodified by the bounded-staleness async module; a fixed \
     seed reproduces byte-identical Store output.";

pub const BOUNDED_STALENESS_ASYNC_REPRODUCIBILITY_CLAIM_V1: &str =
    "NOT bit-reproducible: bounded-staleness async mode's episode-to-weight- \
     version binding depends on wall-clock thread interleaving between the \
     rollout producer and the trainer's publish calls, so the same seed can \
     legitimately bind different episodes to different weight versions \
     across separate runs. Every binding still obeys the declared staleness \
     bound (see the run's staleness ledger); use the ledger, not seed \
     replay, as the evidence trail for this run.";

/// Additive run-metadata record for the selected execution mode. Deliberately
/// a new, standalone record rather than a field grafted onto the production
/// `TrainRunV2` schema: see the implementation report for why the deep Store
/// schema extension (following the `opponent_population_slot`-style additive
/// pattern from commit 1d817d7) is left as a follow-up rather than done here.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExecutionModeRunMetadataV1 {
    pub execution_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_staleness_updates: Option<u32>,
    pub reproducibility_claim: String,
}

pub fn execution_mode_run_metadata_v1(mode: TrainingExecutionModeV1) -> ExecutionModeRunMetadataV1 {
    match mode {
        TrainingExecutionModeV1::SynchronousV1 => ExecutionModeRunMetadataV1 {
            execution_mode: "synchronous_v1".to_owned(),
            max_staleness_updates: None,
            reproducibility_claim: SYNCHRONOUS_REPRODUCIBILITY_CLAIM_V1.to_owned(),
        },
        TrainingExecutionModeV1::BoundedStalenessAsyncV1 {
            max_staleness_updates,
        } => ExecutionModeRunMetadataV1 {
            execution_mode: "bounded_staleness_async_v1".to_owned(),
            max_staleness_updates: Some(max_staleness_updates),
            reproducibility_claim: BOUNDED_STALENESS_ASYNC_REPRODUCIBILITY_CLAIM_V1.to_owned(),
        },
    }
}

/// One episode's scoring identity: which trainer update consumed it, and
/// which weight version scored it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StalenessLedgerEntryV1 {
    pub episode_id: u64,
    pub scoring_weight_version: u64,
    pub consuming_update_version: u64,
}

impl StalenessLedgerEntryV1 {
    /// `consuming_update_version - scoring_weight_version`, saturating at
    /// zero. Callers that need to distinguish "zero staleness" from "scored
    /// by a weight version newer than the consuming update" (always a caller
    /// bug, see [`check_staleness_bound_v1`]) should compare the raw fields.
    pub const fn staleness_v1(self) -> u64 {
        self.consuming_update_version
            .saturating_sub(self.scoring_weight_version)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StalenessBoundViolationV1 {
    pub entry: StalenessLedgerEntryV1,
    pub max_staleness_updates: u32,
}

/// The staleness bound enforcement function. Structurally independent of any
/// scheduler timing: given one recorded entry and the declared bound, decide
/// whether it is admissible. An entry scored by a weight version newer than
/// the update consuming it is always rejected regardless of `K` (that is not
/// staleness, it is a causality violation and cannot happen under correct
/// scheduling).
pub fn check_staleness_bound_v1(
    entry: StalenessLedgerEntryV1,
    max_staleness_updates: u32,
) -> Result<(), StalenessBoundViolationV1> {
    let violates_causality = entry.scoring_weight_version > entry.consuming_update_version;
    let violates_bound = entry.staleness_v1() > u64::from(max_staleness_updates);
    if violates_causality || violates_bound {
        Err(StalenessBoundViolationV1 {
            entry,
            max_staleness_updates,
        })
    } else {
        Ok(())
    }
}

/// Append-only per-episode evidence trail. Recording never refuses an entry
/// (an append-only ledger that silently dropped violations would hide the
/// exact failure it exists to surface); [`Self::record_v1`] both appends and
/// returns the bound-check outcome so callers can fail closed without losing
/// the evidence.
#[derive(Debug, Clone, Default)]
pub struct StalenessLedgerV1 {
    entries: Vec<StalenessLedgerEntryV1>,
}

impl StalenessLedgerV1 {
    pub fn new_v1() -> Self {
        Self::default()
    }

    pub fn record_v1(
        &mut self,
        entry: StalenessLedgerEntryV1,
        max_staleness_updates: u32,
    ) -> Result<(), StalenessBoundViolationV1> {
        let outcome = check_staleness_bound_v1(entry, max_staleness_updates);
        self.entries.push(entry);
        outcome
    }

    pub fn entries_v1(&self) -> &[StalenessLedgerEntryV1] {
        &self.entries
    }

    pub fn len_v1(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty_v1(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn max_observed_staleness_v1(&self) -> Option<u64> {
        self.entries.iter().map(|entry| entry.staleness_v1()).max()
    }

    /// Re-checks every recorded entry against `max_staleness_updates`,
    /// stopping at the first violation. Used both as a whole-run audit and
    /// directly by unit tests exercising the enforcement function in
    /// isolation from any scheduler timing.
    pub fn enforce_all_v1(
        &self,
        max_staleness_updates: u32,
    ) -> Result<(), StalenessBoundViolationV1> {
        for &entry in &self.entries {
            check_staleness_bound_v1(entry, max_staleness_updates)?;
        }
        Ok(())
    }
}

/// One completed batch handed back to the trainer, with its ledger-relevant
/// identity attached.
#[derive(Debug)]
pub struct BoundedStalenessConsumedBatchV1<B> {
    pub batch: B,
    pub consuming_update_version: u64,
    pub scoring_weight_version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundedStalenessConsumeErrorV1 {
    /// No batch became ready within the caller-supplied timeout. Not
    /// necessarily an error (the caller may choose to wait longer), but the
    /// scheduler itself never blocks indefinitely on a hostile/deadlocked
    /// producer without the caller's consent.
    Timeout,
    /// A consumed batch failed the ledger's own bound re-check. This should
    /// be unreachable given correct dispatch gating; surfacing it as a typed
    /// error rather than a panic keeps the scheduler fail-closed without
    /// taking down the caller's process on what would otherwise be a defect
    /// in this module, not the caller's.
    BoundViolation(StalenessBoundViolationV1),
    /// The producer thread ended (panicked or exhausted its declared work)
    /// while the caller still expected more batches.
    ProducerEnded,
}

struct SchedulerStateV1<W> {
    /// Number of trainer updates already trained + published. Advanced only
    /// by `publish_v1`.
    published_version: u64,
    /// Weight snapshot corresponding to `published_version`, read by the
    /// producer under the same lock so a snapshot and the version number it
    /// is paired with can never observe a torn update.
    published_weights: Arc<W>,
    /// Update index the next dispatched batch will be produced for.
    next_dispatch_update: u64,
    stopped: bool,
}

struct ReadyBatchV1<B> {
    consuming_update_version: u64,
    scoring_weight_version: u64,
    batch: B,
    episode_ids: Vec<u64>,
}

/// Generic bounded-staleness producer/consumer scheduler.
///
/// `W` is the published weights type; `B` is the caller's batch type (an
/// `AsyncFlatScoredRolloutResultV1`, a synthetic test payload, or anything
/// else). The scheduler owns one background producer thread. [`Self::
/// publish_v1`] is the trainer-side call: it never waits on the producer.
/// [`Self::next_batch_v1`] is the trainer-side consume call: it blocks (up to
/// a caller-supplied timeout) until a batch is ready, then records it into
/// the shared ledger.
pub struct BoundedStalenessSchedulerV1<W, B> {
    state: Arc<Mutex<SchedulerStateV1<W>>>,
    dispatch_cv: Arc<Condvar>,
    ready: Arc<Mutex<VecDeque<ReadyBatchV1<B>>>>,
    ready_cv: Arc<Condvar>,
    ledger: Arc<Mutex<StalenessLedgerV1>>,
    max_staleness_updates: u32,
    worker: Option<JoinHandle<()>>,
}

impl<W, B> BoundedStalenessSchedulerV1<W, B>
where
    W: Send + Sync + 'static,
    B: Send + 'static,
{
    /// Spawns the background producer thread immediately. `produce` is
    /// called once per dispatched batch with `(scoring_weight_version,
    /// weight snapshot)` and must return the produced batch plus the episode
    /// identifiers it scored. `total_updates` bounds how many batches the
    /// producer will ever dispatch (both the smoke harness and the A/B
    /// harness know this figure up front; an open-ended producer is not
    /// needed for a bounded-duration measurement).
    pub fn spawn_v1(
        initial_weights: W,
        max_staleness_updates: u32,
        total_updates: u64,
        mut produce: impl FnMut(u64, Arc<W>) -> (B, Vec<u64>) + Send + 'static,
    ) -> Self {
        assert!(
            max_staleness_updates >= 1,
            "BoundedStalenessSchedulerV1 requires max_staleness_updates >= 1; \
             select TrainingExecutionModeV1::SynchronousV1 for zero overlap"
        );
        let state = Arc::new(Mutex::new(SchedulerStateV1 {
            published_version: 0,
            published_weights: Arc::new(initial_weights),
            next_dispatch_update: 1,
            stopped: false,
        }));
        let dispatch_cv = Arc::new(Condvar::new());
        let ready = Arc::new(Mutex::new(VecDeque::new()));
        let ready_cv = Arc::new(Condvar::new());

        let worker_state = Arc::clone(&state);
        let worker_dispatch_cv = Arc::clone(&dispatch_cv);
        let worker_ready = Arc::clone(&ready);
        let worker_ready_cv = Arc::clone(&ready_cv);
        let worker = std::thread::spawn(move || {
            for _ in 0..total_updates {
                let (target_update, scoring_version, weights_snapshot) = {
                    let mut guard = worker_state.lock().unwrap();
                    loop {
                        if guard.stopped {
                            return;
                        }
                        let target_update = guard.next_dispatch_update;
                        let staleness_if_dispatched_now =
                            target_update.saturating_sub(guard.published_version);
                        if staleness_if_dispatched_now <= u64::from(max_staleness_updates) {
                            let scoring_version = guard.published_version;
                            let weights_snapshot = Arc::clone(&guard.published_weights);
                            guard.next_dispatch_update = target_update + 1;
                            break (target_update, scoring_version, weights_snapshot);
                        }
                        guard = worker_dispatch_cv.wait(guard).unwrap();
                    }
                };
                let (batch, episode_ids) = produce(scoring_version, weights_snapshot);
                worker_ready.lock().unwrap().push_back(ReadyBatchV1 {
                    consuming_update_version: target_update,
                    scoring_weight_version: scoring_version,
                    batch,
                    episode_ids,
                });
                worker_ready_cv.notify_one();
            }
        });

        Self {
            state,
            dispatch_cv,
            ready,
            ready_cv,
            ledger: Arc::new(Mutex::new(StalenessLedgerV1::new_v1())),
            max_staleness_updates,
            worker: Some(worker),
        }
    }

    /// Trainer-side publish: records that `updates_completed` trainer
    /// updates have now been trained and committed, using `weights` as the
    /// result. Never waits on the producer; the whole critical section is a
    /// version bump, a weight swap, and a condvar notify.
    pub fn publish_v1(&self, updates_completed: u64, weights: W) {
        let mut guard = self.state.lock().unwrap();
        guard.published_version = updates_completed;
        guard.published_weights = Arc::new(weights);
        drop(guard);
        self.dispatch_cv.notify_all();
    }

    /// Trainer-side consume: blocks up to `timeout` for the next ready
    /// batch, records it into the ledger (defensive re-check of the bound),
    /// and returns it in strict `consuming_update_version` order (the
    /// producer dispatches and the ready queue is FIFO, so order is
    /// preserved without any extra bookkeeping here).
    pub fn next_batch_v1(
        &self,
        timeout: Duration,
    ) -> Result<BoundedStalenessConsumedBatchV1<B>, BoundedStalenessConsumeErrorV1> {
        let guard = self.ready.lock().unwrap();
        let (mut guard, wait_result) = self
            .ready_cv
            .wait_timeout_while(guard, timeout, |queue| queue.is_empty())
            .unwrap();
        let Some(ready_batch) = guard.pop_front() else {
            if wait_result.timed_out() {
                if self
                    .worker
                    .as_ref()
                    .is_some_and(JoinHandle::is_finished)
                {
                    return Err(BoundedStalenessConsumeErrorV1::ProducerEnded);
                }
                return Err(BoundedStalenessConsumeErrorV1::Timeout);
            }
            return Err(BoundedStalenessConsumeErrorV1::ProducerEnded);
        };
        drop(guard);

        let entry_outcome = {
            let mut ledger = self.ledger.lock().unwrap();
            for &episode_id in &ready_batch.episode_ids {
                let entry = StalenessLedgerEntryV1 {
                    episode_id,
                    scoring_weight_version: ready_batch.scoring_weight_version,
                    consuming_update_version: ready_batch.consuming_update_version,
                };
                if let Err(violation) = ledger.record_v1(entry, self.max_staleness_updates) {
                    // Still finish recording the remaining episodes in this
                    // batch (append-only evidence discipline) before
                    // returning the first violation.
                    for &remaining_episode_id in ready_batch
                        .episode_ids
                        .iter()
                        .skip_while(|&&id| id != episode_id)
                        .skip(1)
                    {
                        let _ = ledger.record_v1(
                            StalenessLedgerEntryV1 {
                                episode_id: remaining_episode_id,
                                scoring_weight_version: ready_batch.scoring_weight_version,
                                consuming_update_version: ready_batch.consuming_update_version,
                            },
                            self.max_staleness_updates,
                        );
                    }
                    return Err(BoundedStalenessConsumeErrorV1::BoundViolation(violation));
                }
            }
            Ok(())
        };
        entry_outcome?;

        Ok(BoundedStalenessConsumedBatchV1 {
            batch: ready_batch.batch,
            consuming_update_version: ready_batch.consuming_update_version,
            scoring_weight_version: ready_batch.scoring_weight_version,
        })
    }

    /// A clone of the ledger as it stands right now. Cheap enough for
    /// periodic reporting; the scheduler is not designed for a hot per-batch
    /// full-ledger clone.
    pub fn ledger_snapshot_v1(&self) -> StalenessLedgerV1 {
        self.ledger.lock().unwrap().clone()
    }

    pub fn max_staleness_updates_v1(&self) -> u32 {
        self.max_staleness_updates
    }

    /// Signals the producer to stop and joins it. Safe to call after the
    /// producer has already exhausted its declared `total_updates` (the
    /// thread will already have returned; `join` is then immediate).
    pub fn stop_and_join_v1(mut self) {
        if let Ok(mut guard) = self.state.lock() {
            guard.stopped = true;
        }
        self.dispatch_cv.notify_all();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl<W, B> Drop for BoundedStalenessSchedulerV1<W, B> {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.state.lock() {
            guard.stopped = true;
        }
        self.dispatch_cv.notify_all();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Instant;

    // ---- config plumbing ----------------------------------------------

    #[test]
    fn default_execution_mode_is_synchronous() {
        assert_eq!(
            TrainingExecutionModeV1::default(),
            TrainingExecutionModeV1::SynchronousV1
        );
    }

    #[test]
    fn synchronous_mode_always_validates() {
        assert_eq!(TrainingExecutionModeV1::SynchronousV1.validate_v1(), Ok(()));
        assert!(TrainingExecutionModeV1::SynchronousV1.is_synchronous_v1());
        assert_eq!(
            TrainingExecutionModeV1::SynchronousV1.max_staleness_updates_v1(),
            None
        );
    }

    #[test]
    fn zero_staleness_bound_is_rejected_at_config_level() {
        let mode = TrainingExecutionModeV1::BoundedStalenessAsyncV1 {
            max_staleness_updates: 0,
        };
        assert_eq!(
            mode.validate_v1(),
            Err(TrainingExecutionModeValidationErrorV1::ZeroStalenessBound)
        );
    }

    #[test]
    fn nonzero_staleness_bound_validates_and_is_not_synchronous() {
        let mode = TrainingExecutionModeV1::BoundedStalenessAsyncV1 {
            max_staleness_updates: 4,
        };
        assert_eq!(mode.validate_v1(), Ok(()));
        assert!(!mode.is_synchronous_v1());
        assert_eq!(mode.max_staleness_updates_v1(), Some(4));
    }

    #[test]
    fn run_metadata_records_mode_and_honest_reproducibility_claim() {
        let sync = execution_mode_run_metadata_v1(TrainingExecutionModeV1::SynchronousV1);
        assert_eq!(sync.execution_mode, "synchronous_v1");
        assert_eq!(sync.max_staleness_updates, None);
        assert!(!sync.reproducibility_claim.starts_with("NOT"));

        let async_metadata = execution_mode_run_metadata_v1(
            TrainingExecutionModeV1::BoundedStalenessAsyncV1 {
                max_staleness_updates: 2,
            },
        );
        assert_eq!(async_metadata.execution_mode, "bounded_staleness_async_v1");
        assert_eq!(async_metadata.max_staleness_updates, Some(2));
        assert!(async_metadata.reproducibility_claim.starts_with("NOT"));
    }

    #[test]
    fn run_metadata_round_trips_through_json() {
        let metadata = execution_mode_run_metadata_v1(
            TrainingExecutionModeV1::BoundedStalenessAsyncV1 {
                max_staleness_updates: 8,
            },
        );
        let encoded = serde_json::to_string(&metadata).unwrap();
        let decoded: ExecutionModeRunMetadataV1 = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, metadata);

        // Synchronous metadata omits max_staleness_updates entirely, so a
        // record written before this field existed (or a synchronous run's
        // record) decodes and re-encodes without inventing a null key.
        let sync_metadata = execution_mode_run_metadata_v1(TrainingExecutionModeV1::SynchronousV1);
        let sync_encoded = serde_json::to_string(&sync_metadata).unwrap();
        assert!(!sync_encoded.contains("max_staleness_updates"));
    }

    // ---- ledger / bound enforcement ------------------------------------

    #[test]
    fn staleness_is_zero_when_scored_by_the_consuming_updates_own_version() {
        let entry = StalenessLedgerEntryV1 {
            episode_id: 1,
            scoring_weight_version: 10,
            consuming_update_version: 10,
        };
        assert_eq!(entry.staleness_v1(), 0);
        assert_eq!(check_staleness_bound_v1(entry, 0), Ok(()));
    }

    #[test]
    fn entries_within_bound_pass_the_check() {
        let entry = StalenessLedgerEntryV1 {
            episode_id: 1,
            scoring_weight_version: 5,
            consuming_update_version: 8,
        };
        assert_eq!(entry.staleness_v1(), 3);
        assert_eq!(check_staleness_bound_v1(entry, 3), Ok(()));
        assert_eq!(check_staleness_bound_v1(entry, 10), Ok(()));
    }

    #[test]
    fn entries_exceeding_the_bound_are_rejected() {
        let entry = StalenessLedgerEntryV1 {
            episode_id: 1,
            scoring_weight_version: 5,
            consuming_update_version: 9,
        };
        assert_eq!(entry.staleness_v1(), 4);
        assert_eq!(
            check_staleness_bound_v1(entry, 3),
            Err(StalenessBoundViolationV1 {
                entry,
                max_staleness_updates: 3,
            })
        );
        assert_eq!(check_staleness_bound_v1(entry, 4), Ok(()));
    }

    #[test]
    fn entries_scored_by_weights_newer_than_the_consuming_update_are_always_rejected() {
        // scoring_weight_version > consuming_update_version is a causality
        // violation, not staleness: no value of K should ever admit it.
        let entry = StalenessLedgerEntryV1 {
            episode_id: 1,
            scoring_weight_version: 11,
            consuming_update_version: 10,
        };
        for bound in [0_u32, 1, 100, u32::MAX] {
            assert!(check_staleness_bound_v1(entry, bound).is_err());
        }
    }

    #[test]
    fn ledger_records_every_entry_even_on_violation() {
        let mut ledger = StalenessLedgerV1::new_v1();
        let ok_entry = StalenessLedgerEntryV1 {
            episode_id: 1,
            scoring_weight_version: 9,
            consuming_update_version: 10,
        };
        let bad_entry = StalenessLedgerEntryV1 {
            episode_id: 2,
            scoring_weight_version: 5,
            consuming_update_version: 10,
        };
        assert!(ledger.record_v1(ok_entry, 1).is_ok());
        assert!(ledger.record_v1(bad_entry, 1).is_err());
        // Both entries are present in the evidence trail regardless of the
        // second one's outcome.
        assert_eq!(ledger.len_v1(), 2);
        assert_eq!(ledger.entries_v1(), &[ok_entry, bad_entry]);
    }

    #[test]
    fn ledger_max_observed_staleness_tracks_the_worst_entry() {
        let mut ledger = StalenessLedgerV1::new_v1();
        assert_eq!(ledger.max_observed_staleness_v1(), None);
        for (scoring, consuming) in [(10, 10), (8, 10), (10, 11)] {
            ledger
                .record_v1(
                    StalenessLedgerEntryV1 {
                        episode_id: scoring, // arbitrary, distinct-enough id
                        scoring_weight_version: scoring,
                        consuming_update_version: consuming,
                    },
                    100,
                )
                .unwrap();
        }
        assert_eq!(ledger.max_observed_staleness_v1(), Some(2));
    }

    #[test]
    fn enforce_all_reports_the_first_violation_and_ignores_bound_when_clean() {
        let mut ledger = StalenessLedgerV1::new_v1();
        for update in 1..=5_u64 {
            ledger
                .record_v1(
                    StalenessLedgerEntryV1 {
                        episode_id: update,
                        scoring_weight_version: update - 1,
                        consuming_update_version: update,
                    },
                    1,
                )
                .unwrap();
        }
        assert_eq!(ledger.enforce_all_v1(1), Ok(()));
        assert!(ledger.enforce_all_v1(0).is_err());
    }

    // ---- scheduler: real threads, mock W = u64 weight identity ---------

    /// Deterministic pseudo-random microsecond delay in `[0, 200)`, standing
    /// in for variable-length rollout production time without depending on
    /// wall-clock noise for correctness (only for throughput, which these
    /// tests do not assert on).
    fn synthetic_delay_v1(seed: u64) -> Duration {
        let mixed = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(31);
        Duration::from_micros(mixed % 200)
    }

    fn run_scheduler_and_collect_ledger_v1(
        max_staleness_updates: u32,
        total_updates: u64,
    ) -> StalenessLedgerV1 {
        let produced_count = Arc::new(AtomicU64::new(0));
        let produced_count_for_closure = Arc::clone(&produced_count);
        let scheduler: BoundedStalenessSchedulerV1<u64, Vec<u64>> =
            BoundedStalenessSchedulerV1::spawn_v1(
                0_u64,
                max_staleness_updates,
                total_updates,
                move |_scoring_version, _weights| {
                    let ordinal = produced_count_for_closure.fetch_add(1, Ordering::SeqCst);
                    std::thread::sleep(synthetic_delay_v1(ordinal));
                    let episode_id = ordinal;
                    (vec![episode_id], vec![episode_id])
                },
            );

        for update in 1..=total_updates {
            let consumed = scheduler
                .next_batch_v1(Duration::from_secs(10))
                .expect("producer must keep up within the test timeout");
            assert_eq!(consumed.consuming_update_version, update);
            // "Train": publish new weights equal to the update index,
            // immediately, exactly like the trainer publishing without
            // waiting on the next rollout batch.
            scheduler.publish_v1(update, update);
        }

        let ledger = scheduler.ledger_snapshot_v1();
        scheduler.stop_and_join_v1();
        ledger
    }

    #[test]
    fn scheduler_never_exceeds_the_declared_staleness_bound_k1() {
        let ledger = run_scheduler_and_collect_ledger_v1(1, 60);
        assert_eq!(ledger.len_v1(), 60);
        assert!(ledger.max_observed_staleness_v1().unwrap() <= 1);
        assert_eq!(ledger.enforce_all_v1(1), Ok(()));
    }

    #[test]
    fn scheduler_never_exceeds_the_declared_staleness_bound_k3() {
        let ledger = run_scheduler_and_collect_ledger_v1(3, 60);
        assert_eq!(ledger.len_v1(), 60);
        assert!(ledger.max_observed_staleness_v1().unwrap() <= 3);
        assert_eq!(ledger.enforce_all_v1(3), Ok(()));
    }

    #[test]
    fn scheduler_never_exceeds_the_declared_staleness_bound_k8() {
        let ledger = run_scheduler_and_collect_ledger_v1(8, 80);
        assert_eq!(ledger.len_v1(), 80);
        assert!(ledger.max_observed_staleness_v1().unwrap() <= 8);
        assert_eq!(ledger.enforce_all_v1(8), Ok(()));
    }

    #[test]
    #[should_panic(expected = "max_staleness_updates >= 1")]
    fn scheduler_construction_rejects_k0() {
        let _scheduler: BoundedStalenessSchedulerV1<u64, Vec<u64>> =
            BoundedStalenessSchedulerV1::spawn_v1(0_u64, 0, 1, |_v, _w| (Vec::new(), Vec::new()));
    }

    #[test]
    fn scheduler_produces_updates_in_strict_order_with_no_gaps() {
        let total_updates = 40;
        let scheduler: BoundedStalenessSchedulerV1<u64, u64> =
            BoundedStalenessSchedulerV1::spawn_v1(0_u64, 4, total_updates, |version, _weights| {
                (version, Vec::new())
            });
        let mut seen = Vec::new();
        for update in 1..=total_updates {
            let consumed = scheduler
                .next_batch_v1(Duration::from_secs(10))
                .expect("producer must keep up");
            seen.push(consumed.consuming_update_version);
            scheduler.publish_v1(update, update);
        }
        scheduler.stop_and_join_v1();
        let expected: Vec<u64> = (1..=total_updates).collect();
        assert_eq!(seen, expected);
    }

    #[test]
    fn scheduler_publish_does_not_block_on_a_slow_producer() {
        let scheduler: BoundedStalenessSchedulerV1<u64, ()> =
            BoundedStalenessSchedulerV1::spawn_v1(0_u64, 4, 1, |_version, _weights| {
                // Deliberately much slower than any reasonable publish call.
                std::thread::sleep(Duration::from_millis(300));
                ((), Vec::new())
            });
        let started = Instant::now();
        scheduler.publish_v1(0, 0_u64);
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_millis(50),
            "publish_v1 took {elapsed:?}, expected it to return immediately \
             regardless of producer progress"
        );
        scheduler.stop_and_join_v1();
    }

    #[test]
    fn scheduler_reports_bound_violation_when_a_forged_ready_batch_is_too_stale() {
        // The scheduler's own dispatch gate should make this unreachable in
        // practice; this test exercises the ledger's independent defensive
        // check directly, by constructing the violating entry the way
        // `next_batch_v1` would and confirming `record_v1` (the same call
        // path the scheduler uses) rejects it.
        let mut ledger = StalenessLedgerV1::new_v1();
        let forged_entry = StalenessLedgerEntryV1 {
            episode_id: 1,
            scoring_weight_version: 0,
            consuming_update_version: 10,
        };
        let outcome = ledger.record_v1(forged_entry, 1);
        assert_eq!(
            outcome,
            Err(StalenessBoundViolationV1 {
                entry: forged_entry,
                max_staleness_updates: 1,
            })
        );
        assert_eq!(ledger.len_v1(), 1, "the violation is still recorded");
    }

    #[test]
    fn scheduler_next_batch_times_out_when_producer_is_starved_of_publishes() {
        // K=1: the producer can dispatch update 1 immediately (published
        // version starts at 0), but update 2 needs a publish it will never
        // receive in this test, so the second `next_batch_v1` call must time
        // out rather than hang.
        let scheduler: BoundedStalenessSchedulerV1<u64, u64> =
            BoundedStalenessSchedulerV1::spawn_v1(0_u64, 1, 2, |version, _weights| (version, Vec::new()));
        let first = scheduler
            .next_batch_v1(Duration::from_secs(5))
            .expect("first batch should be ready without any publish");
        assert_eq!(first.consuming_update_version, 1);
        let second = scheduler.next_batch_v1(Duration::from_millis(150));
        assert_eq!(second.unwrap_err(), BoundedStalenessConsumeErrorV1::Timeout);
        scheduler.stop_and_join_v1();
    }
}
