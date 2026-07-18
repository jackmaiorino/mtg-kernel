//! Fixed-capacity multi-session rollout scheduler prototype.
//!
//! V1 deliberately proved the shared-memory action handoff with one actor
//! session per OS worker.  That layout preserves actor throughput, but a fast
//! broker normally observes only one ready learner decision at a time.  V2
//! keeps several independent [`FastActorSessionV1`] values on each worker. V2
//! assigns episodes to logical lanes by a fixed stride and advances in global
//! quiescent rounds. The broker collects the complete round, sorts it by a
//! stable decision key, deterministically chunks it, publishes every chunk,
//! and only then wakes workers for the next round. Batch boundaries therefore
//! do not depend on OS scheduling for a fixed configuration.
//!
//! This module still selects seeded-uniform actions.  It validates scheduling,
//! binding, parity, and the achievable batch-width/actor-throughput envelope;
//! it does not include feature encoding, inference, learning, or persistence.
//!
//! Scheduler waits have a cooperative wall-clock deadline and use bounded
//! timeout parks, so a lost notification cannot leave a sleeping thread stuck
//! forever. Rust cannot safely kill a thread that never returns from an engine
//! call. A hard wall-clock guarantee for that case requires future process
//! isolation; the in-process deadline deliberately does not claim otherwise.

use crate::async_rollout::{
    AsyncRolloutEpisodeV1, AsyncRolloutTerminalV1, AsyncRolloutWorkerPhaseV1,
};
use crate::rl::{
    derive_env_seed, derive_policy_seed, PlayerSeatV1, TerminalClassificationV1, TerminalSafeCodeV2,
};
use crate::rl_session::{
    FastActorDecisionKindV1, FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1,
    RlSessionTerminalV1, SessionDeckIdsV1,
};
use crate::state::SplitMix64;
use std::array;
use std::cell::UnsafeCell;
use std::fmt;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle, Thread};
use std::time::{Duration, Instant};

pub const ASYNC_ROLLOUT_MAX_WORKERS_V2: usize = 16;
pub const ASYNC_ROLLOUT_MAX_SESSIONS_PER_WORKER_V2: usize = 64;
pub const ASYNC_ROLLOUT_MAX_LOGICAL_LANES_V2: usize =
    ASYNC_ROLLOUT_MAX_WORKERS_V2 * ASYNC_ROLLOUT_MAX_SESSIONS_PER_WORKER_V2;

const MAILBOX_WORKER_OWNED: u8 = 0;
const MAILBOX_DECISION_READY: u8 = 1;
const MAILBOX_ACTION_READY: u8 = 2;
const MAILBOX_TERMINAL_READY: u8 = 3;

const FNV1A64_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV1A64_PRIME: u64 = 0x0000_0100_0000_01b3;
const WORKER_SPIN_BEFORE_PARK_V2: usize = 2_048;
const SCHEDULER_PARK_POLL_V2: Duration = Duration::from_millis(10);
const BATCH_DIGEST_DOMAIN_V2: &[u8] = b"mtg-kernel/async-rollout-v2/batch-membership/v1";

#[derive(Debug, Clone)]
struct BatchMembershipDigestV2 {
    states: [u64; 4],
}

impl BatchMembershipDigestV2 {
    fn new() -> Self {
        let mut digest = Self {
            states: [
                FNV1A64_OFFSET,
                FNV1A64_OFFSET ^ 0x9e37_79b9_7f4a_7c15,
                FNV1A64_OFFSET ^ 0xd1b5_4a32_d192_ed03,
                FNV1A64_OFFSET ^ 0x94d0_49bb_1331_11eb,
            ],
        };
        digest.update(BATCH_DIGEST_DOMAIN_V2);
        digest
    }

    fn update(&mut self, bytes: impl AsRef<[u8]>) {
        for &byte in bytes.as_ref() {
            for (lane, state) in self.states.iter_mut().enumerate() {
                *state ^= u64::from(byte).wrapping_add((lane as u64) << 8);
                *state = state.wrapping_mul(FNV1A64_PRIME.wrapping_add(2 * lane as u64));
                *state ^= state.rotate_right(11 + lane as u32);
            }
        }
    }

    fn finalize(self) -> [u8; 32] {
        let mut encoded = [0u8; 32];
        for (lane, state) in self.states.into_iter().enumerate() {
            encoded[lane * 8..(lane + 1) * 8].copy_from_slice(&state.to_le_bytes());
        }
        encoded
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncRolloutConfigV2 {
    pub deck_ids: SessionDeckIdsV1,
    pub learner_seat: PlayerSeatV1,
    pub environment_seed: u64,
    pub opponent_policy_seed: u64,
    pub learner_policy_seed: u64,
    pub max_physical_decisions: u64,
    pub max_policy_steps: u64,
    pub worker_count: usize,
    pub sessions_per_worker: usize,
    pub broker_batch_target: usize,
    pub first_episode_id: u64,
    pub episode_count: u64,
    /// Cooperative deadline for the complete public API. Scheduler waits poll
    /// this deadline; a non-returning engine call still requires process
    /// isolation to enforce a hard kill.
    pub scheduler_timeout: Duration,
    pub measure_broker_service_time: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AsyncRolloutMetricsV2 {
    /// Full successful API latency, including validation, allocation, thread
    /// lifecycle, service, joins, canonicalization, and aggregate accounting.
    pub total_elapsed_ns: u64,
    pub batch_publication_count: u64,
    pub batch_width_sum: u64,
    pub max_batch_width: u32,
    pub learner_action_count: u64,
    pub terminal_notifications: u64,
    /// Calls to `park_timeout`; an already-present token can make a call return
    /// without blocking, so this is intentionally an attempt count.
    pub worker_park_attempt_count: u64,
    /// Broker calls to `park_timeout`, not confirmed blocking events.
    pub broker_park_attempt_count: u64,
    pub quiescent_flush_count: u64,
    pub target_flush_count: u64,
    pub broker_service_ns: u64,
    pub complete_round_count: u64,
    /// Domain-separated 256-bit digest over the ordered complete-round, chunk,
    /// terminal, decision, and selected-action membership contract. This is a
    /// reproducibility binding, not a cryptographic authenticity primitive.
    pub batch_membership_digest: [u8; 32],
}

impl AsyncRolloutMetricsV2 {
    pub fn mean_batch_width(self) -> f64 {
        if self.batch_publication_count == 0 {
            0.0
        } else {
            self.batch_width_sum as f64 / self.batch_publication_count as f64
        }
    }

    pub fn mean_broker_service_ns(self) -> f64 {
        if self.batch_publication_count == 0 {
            0.0
        } else {
            self.broker_service_ns as f64 / self.batch_publication_count as f64
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncRolloutResultV2 {
    pub episodes: Vec<AsyncRolloutEpisodeV1>,
    pub policy_step_count: u64,
    pub physical_decision_count: u64,
    pub metrics: AsyncRolloutMetricsV2,
}

impl AsyncRolloutResultV2 {
    pub fn all_natural(&self) -> bool {
        self.episodes.iter().all(|episode| {
            episode.terminal.terminal_classification == TerminalClassificationV1::Natural
                && episode.terminal.terminal_code == TerminalSafeCodeV2::NaturalGameOver
        })
    }

    pub fn games_per_second(&self) -> f64 {
        let seconds = self.metrics.total_elapsed_ns as f64 / 1_000_000_000.0;
        if seconds > 0.0 {
            self.episodes.len() as f64 / seconds
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncRolloutErrorV2 {
    InvalidWorkerCount {
        requested: usize,
    },
    InvalidSessionsPerWorker {
        requested: usize,
    },
    InvalidBrokerBatchTarget {
        requested: usize,
        logical_lanes: usize,
    },
    InvalidSchedulerTimeout,
    EmptyEpisodeRange,
    EpisodeRangeOverflow,
    EpisodeCountExceedsAddressSpace {
        requested: u64,
    },
    ResultAllocationFailed {
        requested: u64,
    },
    WorkerSpawnFailed {
        worker_id: usize,
    },
    WorkerFailed {
        worker_id: usize,
        logical_lane_id: usize,
        episode_id: u64,
        phase: AsyncRolloutWorkerPhaseV1,
    },
    ActionBindingMismatch {
        logical_lane_id: usize,
        episode_id: u64,
    },
    BrokerProtocolViolation,
    SchedulerDeadlineExceeded,
    WorkerPanicked {
        worker_id: usize,
    },
}

impl fmt::Display for AsyncRolloutErrorV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWorkerCount { requested } => write!(
                formatter,
                "worker_count {requested} is outside 1..={ASYNC_ROLLOUT_MAX_WORKERS_V2}"
            ),
            Self::InvalidSessionsPerWorker { requested } => write!(
                formatter,
                "sessions_per_worker {requested} is outside 1..={ASYNC_ROLLOUT_MAX_SESSIONS_PER_WORKER_V2}"
            ),
            Self::InvalidBrokerBatchTarget {
                requested,
                logical_lanes,
            } => write!(
                formatter,
                "broker_batch_target {requested} is outside 1..={logical_lanes}"
            ),
            Self::InvalidSchedulerTimeout => {
                write!(formatter, "scheduler_timeout must be positive and representable")
            }
            Self::EmptyEpisodeRange => write!(formatter, "episode_count must be positive"),
            Self::EpisodeRangeOverflow => write!(formatter, "episode id range overflows u64"),
            Self::EpisodeCountExceedsAddressSpace { requested } => write!(
                formatter,
                "episode_count {requested} cannot be represented by this process"
            ),
            Self::ResultAllocationFailed { requested } => write!(
                formatter,
                "could not reserve canonical result storage for {requested} episodes"
            ),
            Self::WorkerSpawnFailed { worker_id } => {
                write!(formatter, "failed to spawn rollout worker {worker_id}")
            }
            Self::WorkerFailed {
                worker_id,
                logical_lane_id,
                episode_id,
                phase,
            } => write!(
                formatter,
                "rollout worker {worker_id} failed on logical lane {logical_lane_id} in {phase:?} for episode {episode_id}"
            ),
            Self::ActionBindingMismatch {
                logical_lane_id,
                episode_id,
            } => write!(
                formatter,
                "learner action binding for logical lane {logical_lane_id} does not match episode {episode_id}"
            ),
            Self::BrokerProtocolViolation => {
                write!(formatter, "asynchronous rollout broker protocol violation")
            }
            Self::SchedulerDeadlineExceeded => {
                write!(formatter, "cooperative asynchronous rollout scheduler deadline exceeded")
            }
            Self::WorkerPanicked { worker_id } => {
                write!(formatter, "rollout worker {worker_id} panicked")
            }
        }
    }
}

impl std::error::Error for AsyncRolloutErrorV2 {}

#[derive(Debug, Clone, Copy)]
enum WorkerMessageV2 {
    Decision(FastActorDecisionV1),
    Terminal {
        terminal: AsyncRolloutTerminalV1,
        learner_action_count: u64,
        learner_trace_hash: u64,
    },
}

#[derive(Debug, Clone, Copy)]
struct ActionEnvelopeV2 {
    episode_id: u64,
    revision: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
    legal_action_count: u32,
    selected_index: u32,
}

#[repr(align(64))]
struct LaneMailboxV2 {
    state: AtomicU8,
    current_episode_id: AtomicU64,
    message: UnsafeCell<MaybeUninit<WorkerMessageV2>>,
    action: UnsafeCell<MaybeUninit<ActionEnvelopeV2>>,
}

// SAFETY: each mailbox has one worker endpoint and the broker endpoint.  The
// Release/Acquire transition on `state` transfers exclusive access to the
// corresponding payload.
unsafe impl Sync for LaneMailboxV2 {}

impl LaneMailboxV2 {
    fn new() -> Self {
        Self {
            state: AtomicU8::new(MAILBOX_WORKER_OWNED),
            current_episode_id: AtomicU64::new(u64::MAX),
            message: UnsafeCell::new(MaybeUninit::uninit()),
            action: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    fn worker_publish(&self, message: WorkerMessageV2, state: u8) {
        debug_assert!(matches!(
            state,
            MAILBOX_DECISION_READY | MAILBOX_TERMINAL_READY
        ));
        // SAFETY: only the owning worker writes while the mailbox is owned.
        unsafe { (*self.message.get()).write(message) };
        self.state.store(state, Ordering::Release);
    }

    fn broker_read(&self) -> WorkerMessageV2 {
        // SAFETY: the broker observed a ready state with Acquire and the worker
        // cannot modify the Copy payload before acknowledgement/action.
        unsafe { (*self.message.get()).assume_init_read() }
    }

    fn broker_write_action(&self, action: ActionEnvelopeV2) {
        // SAFETY: the worker remains blocked in DECISION_READY until the
        // broker's Release publication of ACTION_READY.
        unsafe { (*self.action.get()).write(action) };
    }

    fn worker_read_action(&self) -> ActionEnvelopeV2 {
        // SAFETY: the worker observed ACTION_READY with Acquire.  The broker
        // no longer accesses the payload before worker ownership resumes.
        unsafe { (*self.action.get()).assume_init_read() }
    }
}

#[repr(align(64))]
struct WorkerFailureCellV2 {
    logical_lane_id: AtomicUsize,
    episode_id: AtomicU64,
    phase: AtomicU8,
}

impl WorkerFailureCellV2 {
    fn new() -> Self {
        Self {
            logical_lane_id: AtomicUsize::new(usize::MAX),
            episode_id: AtomicU64::new(u64::MAX),
            phase: AtomicU8::new(0),
        }
    }
}

#[repr(align(64))]
struct SharedCountersV2 {
    started_workers: AtomicUsize,
    start: AtomicBool,
    cancel: AtomicBool,
    deadline_exceeded: AtomicBool,
    quiescent_workers: AtomicU32,
    done_workers: AtomicU32,
    failed_workers: AtomicU32,
    worker_park_attempt_count: AtomicU64,
}

struct SharedRolloutV2 {
    lanes: [LaneMailboxV2; ASYNC_ROLLOUT_MAX_LOGICAL_LANES_V2],
    failures: [WorkerFailureCellV2; ASYNC_ROLLOUT_MAX_WORKERS_V2],
    worker_threads: [OnceLock<Thread>; ASYNC_ROLLOUT_MAX_WORKERS_V2],
    broker_thread: OnceLock<Thread>,
    counters: SharedCountersV2,
    first_episode_id: u64,
    end_episode_id: u64,
    logical_lane_count: usize,
    worker_count: usize,
    sessions_per_worker: usize,
    scheduler_deadline: Instant,
}

impl SharedRolloutV2 {
    fn new(
        first_episode_id: u64,
        end_episode_id: u64,
        logical_lane_count: usize,
        worker_count: usize,
        sessions_per_worker: usize,
        scheduler_deadline: Instant,
    ) -> Self {
        Self {
            lanes: array::from_fn(|_| LaneMailboxV2::new()),
            failures: array::from_fn(|_| WorkerFailureCellV2::new()),
            worker_threads: array::from_fn(|_| OnceLock::new()),
            broker_thread: OnceLock::new(),
            counters: SharedCountersV2 {
                started_workers: AtomicUsize::new(0),
                start: AtomicBool::new(false),
                cancel: AtomicBool::new(false),
                deadline_exceeded: AtomicBool::new(false),
                quiescent_workers: AtomicU32::new(0),
                done_workers: AtomicU32::new(0),
                failed_workers: AtomicU32::new(0),
                worker_park_attempt_count: AtomicU64::new(0),
            },
            first_episode_id,
            end_episode_id,
            logical_lane_count,
            worker_count,
            sessions_per_worker,
            scheduler_deadline,
        }
    }

    fn worker_for_lane(&self, logical_lane_id: usize) -> usize {
        logical_lane_id / self.sessions_per_worker
    }

    fn notify_broker_control(&self) {
        if let Some(broker) = self.broker_thread.get() {
            broker.unpark();
        }
    }

    fn wake_worker(&self, worker_id: usize) {
        self.counters
            .quiescent_workers
            .fetch_and(!(1u32 << worker_id), Ordering::Release);
        if let Some(worker) = self.worker_threads[worker_id].get() {
            worker.unpark();
        }
    }

    fn cancel_and_wake_all(&self) {
        self.counters.cancel.store(true, Ordering::Release);
        self.counters.start.store(true, Ordering::Release);
        for worker_id in 0..self.worker_count {
            if let Some(worker) = self.worker_threads[worker_id].get() {
                worker.unpark();
            }
        }
        self.notify_broker_control();
    }

    fn deadline_reached(&self) -> bool {
        Instant::now() >= self.scheduler_deadline
    }

    fn signal_deadline_and_cancel(&self) {
        self.counters
            .deadline_exceeded
            .store(true, Ordering::Release);
        self.cancel_and_wake_all();
    }

    fn stop_requested(&self) -> bool {
        if self.counters.cancel.load(Ordering::Acquire) {
            return true;
        }
        if self.deadline_reached() {
            self.signal_deadline_and_cancel();
            return true;
        }
        false
    }

    fn park_poll_duration(&self) -> Duration {
        self.scheduler_deadline
            .saturating_duration_since(Instant::now())
            .min(SCHEDULER_PARK_POLL_V2)
    }

    fn record_failure(&self, worker_id: usize, failure: WorkerFailureV2) {
        let cell = &self.failures[worker_id];
        cell.logical_lane_id
            .store(failure.logical_lane_id, Ordering::Relaxed);
        cell.episode_id.store(failure.episode_id, Ordering::Relaxed);
        cell.phase
            .store(worker_phase_code(failure.phase), Ordering::Release);
        self.counters
            .failed_workers
            .fetch_or(1u32 << worker_id, Ordering::Release);
        self.notify_broker_control();
    }
}

#[derive(Debug, Clone, Copy)]
struct WorkerFailureV2 {
    logical_lane_id: usize,
    episode_id: u64,
    phase: AsyncRolloutWorkerPhaseV1,
}

#[cfg(test)]
const TEST_INJECT_RESET_V2: u8 = 1;
#[cfg(test)]
const TEST_INJECT_LEARNER_STEP_V2: u8 = 2;
#[cfg(test)]
const TEST_INJECT_OPPONENT_STEP_V2: u8 = 3;
#[cfg(test)]
const TEST_INJECT_PANIC_V2: u8 = 4;
#[cfg(test)]
static TEST_INJECTION_V2: AtomicU8 = AtomicU8::new(0);
#[cfg(test)]
static TEST_ACTIVE_WORKERS_V2: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
struct TestActiveWorkerGuardV2;

#[cfg(test)]
impl TestActiveWorkerGuardV2 {
    fn enter() -> Self {
        TEST_ACTIVE_WORKERS_V2.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

#[cfg(test)]
impl Drop for TestActiveWorkerGuardV2 {
    fn drop(&mut self) {
        TEST_ACTIVE_WORKERS_V2.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
fn maybe_inject_worker_failure(
    injection: u8,
    failure: WorkerFailureV2,
) -> Result<(), WorkerFailureV2> {
    if TEST_INJECTION_V2
        .compare_exchange(injection, 0, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        if injection == TEST_INJECT_PANIC_V2 {
            panic!("injected async rollout v2 worker panic");
        }
        return Err(failure);
    }
    Ok(())
}

struct LocalLaneV2 {
    logical_lane_id: usize,
    next_episode_id: Option<u64>,
    episode_id: u64,
    session: Option<FastActorSessionV1>,
    response: Option<FastActorResponseV1>,
    waiting_decision: Option<FastActorDecisionV1>,
    waiting_terminal: bool,
    opponent_policy: SplitMix64,
    learner_action_count: u64,
    learner_trace_hash: u64,
}

impl LocalLaneV2 {
    fn vacant(logical_lane_id: usize, first_episode_id: u64, end_episode_id: u64) -> Self {
        let next_episode_id = first_episode_id
            .checked_add(logical_lane_id as u64)
            .filter(|episode_id| *episode_id < end_episode_id);
        Self {
            logical_lane_id,
            next_episode_id,
            episode_id: u64::MAX,
            session: None,
            response: None,
            waiting_decision: None,
            waiting_terminal: false,
            opponent_policy: SplitMix64::seed(0),
            learner_action_count: 0,
            learner_trace_hash: FNV1A64_OFFSET,
        }
    }

    fn is_active(&self) -> bool {
        self.session.is_some() || self.waiting_terminal
    }

    fn has_future_episode(&self) -> bool {
        self.next_episode_id.is_some()
    }

    fn fill(
        &mut self,
        shared: &SharedRolloutV2,
        config: &AsyncRolloutConfigV2,
    ) -> Result<bool, WorkerFailureV2> {
        if self.is_active() {
            return Ok(false);
        }
        let Some(episode_id) = self.next_episode_id else {
            return Ok(false);
        };
        self.next_episode_id = episode_id
            .checked_add(shared.logical_lane_count as u64)
            .filter(|next| *next < shared.end_episode_id);
        #[cfg(test)]
        maybe_inject_worker_failure(
            TEST_INJECT_RESET_V2,
            WorkerFailureV2 {
                logical_lane_id: self.logical_lane_id,
                episode_id,
                phase: AsyncRolloutWorkerPhaseV1::Reset,
            },
        )?;
        let session = FastActorSessionV1::reset_with_decks_and_limits(
            episode_id,
            derive_env_seed(config.environment_seed, episode_id),
            config.max_physical_decisions,
            config.max_policy_steps,
            config.deck_ids.clone(),
        )
        .map_err(|_| WorkerFailureV2 {
            logical_lane_id: self.logical_lane_id,
            episode_id,
            phase: AsyncRolloutWorkerPhaseV1::Reset,
        })?;
        let response = session.current_response();
        self.session = Some(session);
        self.episode_id = episode_id;
        self.response = Some(response);
        self.waiting_decision = None;
        self.waiting_terminal = false;
        self.opponent_policy =
            SplitMix64::seed(derive_policy_seed(config.opponent_policy_seed, episode_id));
        self.learner_action_count = 0;
        self.learner_trace_hash = hash_bytes(FNV1A64_OFFSET, &episode_id.to_le_bytes());
        shared.lanes[self.logical_lane_id]
            .current_episode_id
            .store(episode_id, Ordering::Relaxed);
        Ok(true)
    }

    fn progress(
        &mut self,
        shared: &SharedRolloutV2,
        config: &AsyncRolloutConfigV2,
    ) -> Result<bool, WorkerFailureV2> {
        let mailbox = &shared.lanes[self.logical_lane_id];
        if let Some(decision) = self.waiting_decision {
            return match mailbox.state.load(Ordering::Acquire) {
                MAILBOX_ACTION_READY => {
                    let action = mailbox.worker_read_action();
                    if !validate_action_binding(decision, action) {
                        return Err(WorkerFailureV2 {
                            logical_lane_id: self.logical_lane_id,
                            episode_id: decision.episode_id,
                            phase: AsyncRolloutWorkerPhaseV1::LearnerActionBinding,
                        });
                    }
                    mailbox.state.store(MAILBOX_WORKER_OWNED, Ordering::Release);
                    #[cfg(test)]
                    maybe_inject_worker_failure(
                        TEST_INJECT_LEARNER_STEP_V2,
                        WorkerFailureV2 {
                            logical_lane_id: self.logical_lane_id,
                            episode_id: decision.episode_id,
                            phase: AsyncRolloutWorkerPhaseV1::LearnerStep,
                        },
                    )?;
                    let response = self
                        .session
                        .as_mut()
                        .expect("an action-waiting lane owns a session")
                        .step(action.episode_id, action.revision, action.selected_index)
                        .map_err(|_| WorkerFailureV2 {
                            logical_lane_id: self.logical_lane_id,
                            episode_id: decision.episode_id,
                            phase: AsyncRolloutWorkerPhaseV1::LearnerStep,
                        })?;
                    self.response = Some(response);
                    self.waiting_decision = None;
                    self.learner_action_count =
                        self.learner_action_count
                            .checked_add(1)
                            .ok_or(WorkerFailureV2 {
                                logical_lane_id: self.logical_lane_id,
                                episode_id: decision.episode_id,
                                phase: AsyncRolloutWorkerPhaseV1::LearnerStep,
                            })?;
                    self.learner_trace_hash =
                        record_trace(self.learner_trace_hash, decision, action.selected_index);
                    Ok(true)
                }
                MAILBOX_DECISION_READY => Ok(false),
                _ => Err(WorkerFailureV2 {
                    logical_lane_id: self.logical_lane_id,
                    episode_id: decision.episode_id,
                    phase: AsyncRolloutWorkerPhaseV1::LearnerActionBinding,
                }),
            };
        }
        if self.waiting_terminal {
            return match mailbox.state.load(Ordering::Acquire) {
                MAILBOX_TERMINAL_READY => Ok(false),
                MAILBOX_WORKER_OWNED => {
                    self.session = None;
                    self.episode_id = u64::MAX;
                    self.response = None;
                    self.waiting_terminal = false;
                    mailbox
                        .current_episode_id
                        .store(u64::MAX, Ordering::Relaxed);
                    Ok(true)
                }
                _ => Err(WorkerFailureV2 {
                    logical_lane_id: self.logical_lane_id,
                    episode_id: mailbox.current_episode_id.load(Ordering::Relaxed),
                    phase: AsyncRolloutWorkerPhaseV1::Panic,
                }),
            };
        }
        if self.session.is_none() {
            return Ok(false);
        }

        loop {
            if shared.stop_requested() {
                return Ok(false);
            }
            let response = self
                .response
                .take()
                .expect("a running lane always carries its current response");
            match response {
                FastActorResponseV1::Terminal(terminal) => {
                    mailbox.worker_publish(
                        WorkerMessageV2::Terminal {
                            terminal: compact_terminal(&terminal),
                            learner_action_count: self.learner_action_count,
                            learner_trace_hash: self.learner_trace_hash,
                        },
                        MAILBOX_TERMINAL_READY,
                    );
                    self.waiting_terminal = true;
                    return Ok(true);
                }
                FastActorResponseV1::Decision(decision)
                    if decision.acting_player == config.learner_seat =>
                {
                    mailbox.worker_publish(
                        WorkerMessageV2::Decision(decision),
                        MAILBOX_DECISION_READY,
                    );
                    self.waiting_decision = Some(decision);
                    return Ok(true);
                }
                FastActorResponseV1::Decision(decision) => {
                    let selected_index =
                        uniform_index(&mut self.opponent_policy, decision.legal_action_count);
                    #[cfg(test)]
                    maybe_inject_worker_failure(
                        TEST_INJECT_OPPONENT_STEP_V2,
                        WorkerFailureV2 {
                            logical_lane_id: self.logical_lane_id,
                            episode_id: decision.episode_id,
                            phase: AsyncRolloutWorkerPhaseV1::OpponentStep,
                        },
                    )?;
                    let next = self
                        .session
                        .as_mut()
                        .expect("a running lane owns a session")
                        .step(decision.episode_id, decision.step, selected_index)
                        .map_err(|_| WorkerFailureV2 {
                            logical_lane_id: self.logical_lane_id,
                            episode_id: decision.episode_id,
                            phase: AsyncRolloutWorkerPhaseV1::OpponentStep,
                        })?;
                    self.response = Some(next);
                }
            }
        }
    }
}

fn worker_loop(
    shared: &SharedRolloutV2,
    config: &AsyncRolloutConfigV2,
    worker_id: usize,
) -> Result<(), WorkerFailureV2> {
    let current_thread = thread::current();
    shared.worker_threads[worker_id]
        .set(current_thread)
        .map_err(|_| WorkerFailureV2 {
            logical_lane_id: worker_id * config.sessions_per_worker,
            episode_id: u64::MAX,
            phase: AsyncRolloutWorkerPhaseV1::Panic,
        })?;
    shared
        .counters
        .started_workers
        .fetch_add(1, Ordering::Release);
    shared.notify_broker_control();
    while !shared.counters.start.load(Ordering::Acquire) {
        if shared.stop_requested() {
            return Ok(());
        }
        thread::park_timeout(shared.park_poll_duration());
    }

    let first_lane = worker_id * config.sessions_per_worker;
    let mut lanes: Vec<LocalLaneV2> = (0..config.sessions_per_worker)
        .map(|slot| {
            LocalLaneV2::vacant(
                first_lane + slot,
                shared.first_episode_id,
                shared.end_episode_id,
            )
        })
        .collect();
    #[cfg(test)]
    maybe_inject_worker_failure(
        TEST_INJECT_PANIC_V2,
        WorkerFailureV2 {
            logical_lane_id: first_lane,
            episode_id: u64::MAX,
            phase: AsyncRolloutWorkerPhaseV1::Panic,
        },
    )?;

    'worker: loop {
        if shared.stop_requested() {
            return Ok(());
        }
        let mut progressed = false;
        for lane in &mut lanes {
            let failure_cell = &shared.failures[worker_id];
            failure_cell
                .logical_lane_id
                .store(lane.logical_lane_id, Ordering::Relaxed);
            failure_cell.episode_id.store(
                shared.lanes[lane.logical_lane_id]
                    .current_episode_id
                    .load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
            progressed |= lane.fill(shared, config)?;
            progressed |= lane.progress(shared, config)?;
        }
        let any_active = lanes.iter().any(LocalLaneV2::is_active);
        let exhausted = !lanes.iter().any(LocalLaneV2::has_future_episode);
        if !any_active && exhausted {
            shared
                .counters
                .done_workers
                .fetch_or(1u32 << worker_id, Ordering::Release);
            shared.notify_broker_control();
            return Ok(());
        }
        if progressed {
            continue;
        }

        shared
            .counters
            .quiescent_workers
            .fetch_or(1u32 << worker_id, Ordering::Release);
        shared.notify_broker_control();
        if shared.stop_requested() {
            return Ok(());
        }
        let worker_bit = 1u32 << worker_id;
        for _ in 0..WORKER_SPIN_BEFORE_PARK_V2 {
            if shared.stop_requested() {
                return Ok(());
            }
            if shared.counters.quiescent_workers.load(Ordering::Acquire) & worker_bit == 0 {
                // Consume the unpark token paired with the cleared bit if it
                // has already arrived. A zero timeout never blocks.
                thread::park_timeout(Duration::ZERO);
                continue 'worker;
            }
            std::hint::spin_loop();
        }
        // Timeout and spurious wakes do not transfer round ownership. Only
        // the broker clears this worker's quiescent bit after every action
        // chunk and terminal acknowledgement for the round are published.
        loop {
            if shared.stop_requested() {
                return Ok(());
            }
            if shared.counters.quiescent_workers.load(Ordering::Acquire) & worker_bit == 0 {
                thread::park_timeout(Duration::ZERO);
                continue 'worker;
            }
            shared
                .counters
                .worker_park_attempt_count
                .fetch_add(1, Ordering::Relaxed);
            thread::park_timeout(shared.park_poll_duration());
        }
    }
}

fn worker_entry(shared: Arc<SharedRolloutV2>, config: AsyncRolloutConfigV2, worker_id: usize) {
    #[cfg(test)]
    let _active_worker = TestActiveWorkerGuardV2::enter();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        worker_loop(&shared, &config, worker_id)
    }));
    if shared.counters.cancel.load(Ordering::Relaxed) {
        return;
    }
    let failure = match result {
        Ok(Ok(())) => return,
        Ok(Err(failure)) => failure,
        Err(_) => WorkerFailureV2 {
            logical_lane_id: shared.failures[worker_id]
                .logical_lane_id
                .load(Ordering::Relaxed),
            episode_id: shared.failures[worker_id]
                .episode_id
                .load(Ordering::Relaxed),
            phase: AsyncRolloutWorkerPhaseV1::Panic,
        },
    };
    shared.record_failure(worker_id, failure);
}

#[derive(Debug, Clone, Copy)]
struct BrokerEpisodeV2 {
    active: bool,
    episode_id: u64,
    learner_policy: SplitMix64,
}

impl BrokerEpisodeV2 {
    fn empty() -> Self {
        Self {
            active: false,
            episode_id: 0,
            learner_policy: SplitMix64::seed(0),
        }
    }

    fn bind(
        &mut self,
        learner_policy_seed: u64,
        episode_id: u64,
    ) -> Result<(), AsyncRolloutErrorV2> {
        if !self.active {
            self.active = true;
            self.episode_id = episode_id;
            self.learner_policy =
                SplitMix64::seed(derive_policy_seed(learner_policy_seed, episode_id));
        }
        if self.episode_id != episode_id {
            return Err(AsyncRolloutErrorV2::BrokerProtocolViolation);
        }
        Ok(())
    }

    fn finish(
        &mut self,
        learner_policy_seed: u64,
        terminal: AsyncRolloutTerminalV1,
        learner_action_count: u64,
        learner_trace_hash: u64,
    ) -> Result<AsyncRolloutEpisodeV1, AsyncRolloutErrorV2> {
        self.bind(learner_policy_seed, terminal.episode_id)?;
        let episode = AsyncRolloutEpisodeV1 {
            terminal,
            learner_action_count,
            learner_trace_hash,
        };
        *self = Self::empty();
        Ok(episode)
    }
}

#[derive(Debug, Clone, Copy)]
struct BrokerRequestV2 {
    logical_lane_id: usize,
    decision: FastActorDecisionV1,
    action: ActionEnvelopeV2,
}

#[derive(Debug, Clone, Copy)]
struct RoundDecisionV2 {
    logical_lane_id: usize,
    decision: FastActorDecisionV1,
}

#[derive(Debug, Clone, Copy)]
struct RoundTerminalV2 {
    logical_lane_id: usize,
    terminal: AsyncRolloutTerminalV1,
    learner_action_count: u64,
    learner_trace_hash: u64,
}

fn stable_decision_key(request: &RoundDecisionV2) -> (u64, u64, u64, u32, u32, u8, u32, usize) {
    let decision = request.decision;
    (
        decision.episode_id,
        decision.step,
        decision.physical_decision_id,
        decision.substep_index,
        decision.substep_count,
        decision_kind_code(decision.decision_kind),
        decision.legal_action_count,
        request.logical_lane_id,
    )
}

fn validate_action_batch(
    shared: &SharedRolloutV2,
    requests: &[BrokerRequestV2],
) -> Result<(), AsyncRolloutErrorV2> {
    for request in requests {
        if request.logical_lane_id >= ASYNC_ROLLOUT_MAX_LOGICAL_LANES_V2
            || shared.lanes[request.logical_lane_id]
                .state
                .load(Ordering::Acquire)
                != MAILBOX_DECISION_READY
            || !validate_action_binding(request.decision, request.action)
        {
            return Err(AsyncRolloutErrorV2::ActionBindingMismatch {
                logical_lane_id: request.logical_lane_id,
                episode_id: request.decision.episode_id,
            });
        }
    }
    Ok(())
}

fn publish_action_chunk(shared: &SharedRolloutV2, requests: &[BrokerRequestV2]) -> u32 {
    let mut workers = 0u32;
    for request in requests {
        shared.lanes[request.logical_lane_id].broker_write_action(request.action);
    }
    for request in requests {
        shared.lanes[request.logical_lane_id]
            .state
            .store(MAILBOX_ACTION_READY, Ordering::Release);
        workers |= 1u32 << shared.worker_for_lane(request.logical_lane_id);
    }
    workers
}

struct WorkerJoinGuardV2 {
    shared: Arc<SharedRolloutV2>,
    handles: [Option<JoinHandle<()>>; ASYNC_ROLLOUT_MAX_WORKERS_V2],
    armed: bool,
}

impl WorkerJoinGuardV2 {
    fn new(shared: Arc<SharedRolloutV2>) -> Self {
        Self {
            shared,
            handles: array::from_fn(|_| None),
            armed: true,
        }
    }

    fn install(&mut self, worker_id: usize, handle: JoinHandle<()>) {
        self.handles[worker_id] = Some(handle);
    }

    fn join_every_worker(&mut self) -> Result<(), AsyncRolloutErrorV2> {
        let mut first_panicked_worker = None;
        for (worker_id, handle) in self.handles.iter_mut().enumerate() {
            if let Some(handle) = handle.take() {
                if handle.join().is_err() && first_panicked_worker.is_none() {
                    first_panicked_worker = Some(worker_id);
                }
            }
        }
        self.armed = false;
        match first_panicked_worker {
            Some(worker_id) => Err(AsyncRolloutErrorV2::WorkerPanicked { worker_id }),
            None => Ok(()),
        }
    }
}

impl Drop for WorkerJoinGuardV2 {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.shared.cancel_and_wake_all();
        let _ = self.join_every_worker();
    }
}

/// Run a finite seeded-uniform schedule through the multi-session batching
/// scheduler.  The successful elapsed time covers the full public API.
pub fn run_seeded_uniform_async_rollout_v2(
    config: AsyncRolloutConfigV2,
) -> Result<AsyncRolloutResultV2, AsyncRolloutErrorV2> {
    let api_started = Instant::now();
    if !(1..=ASYNC_ROLLOUT_MAX_WORKERS_V2).contains(&config.worker_count) {
        return Err(AsyncRolloutErrorV2::InvalidWorkerCount {
            requested: config.worker_count,
        });
    }
    if !(1..=ASYNC_ROLLOUT_MAX_SESSIONS_PER_WORKER_V2).contains(&config.sessions_per_worker) {
        return Err(AsyncRolloutErrorV2::InvalidSessionsPerWorker {
            requested: config.sessions_per_worker,
        });
    }
    let logical_lane_count = config
        .worker_count
        .checked_mul(config.sessions_per_worker)
        .ok_or(AsyncRolloutErrorV2::BrokerProtocolViolation)?;
    if !(1..=logical_lane_count).contains(&config.broker_batch_target) {
        return Err(AsyncRolloutErrorV2::InvalidBrokerBatchTarget {
            requested: config.broker_batch_target,
            logical_lanes: logical_lane_count,
        });
    }
    if config.scheduler_timeout.is_zero() {
        return Err(AsyncRolloutErrorV2::InvalidSchedulerTimeout);
    }
    let scheduler_deadline = api_started
        .checked_add(config.scheduler_timeout)
        .ok_or(AsyncRolloutErrorV2::InvalidSchedulerTimeout)?;
    if config.episode_count == 0 {
        return Err(AsyncRolloutErrorV2::EmptyEpisodeRange);
    }
    let end_episode_id = config
        .first_episode_id
        .checked_add(config.episode_count)
        .ok_or(AsyncRolloutErrorV2::EpisodeRangeOverflow)?;
    let episode_count_usize = usize::try_from(config.episode_count).map_err(|_| {
        AsyncRolloutErrorV2::EpisodeCountExceedsAddressSpace {
            requested: config.episode_count,
        }
    })?;
    let mut episodes = Vec::new();
    episodes
        .try_reserve_exact(episode_count_usize)
        .map_err(|_| AsyncRolloutErrorV2::ResultAllocationFailed {
            requested: config.episode_count,
        })?;

    let shared = Arc::new(SharedRolloutV2::new(
        config.first_episode_id,
        end_episode_id,
        logical_lane_count,
        config.worker_count,
        config.sessions_per_worker,
        scheduler_deadline,
    ));
    shared
        .broker_thread
        .set(thread::current())
        .map_err(|_| AsyncRolloutErrorV2::BrokerProtocolViolation)?;
    let mut worker_guard = WorkerJoinGuardV2::new(Arc::clone(&shared));
    for worker_id in 0..config.worker_count {
        let worker_shared = Arc::clone(&shared);
        let worker_config = config.clone();
        let handle = thread::Builder::new()
            .name(format!("mtg-async-rollout-v2-{worker_id}"))
            .spawn(move || worker_entry(worker_shared, worker_config, worker_id));
        match handle {
            Ok(handle) => worker_guard.install(worker_id, handle),
            Err(_) => return Err(AsyncRolloutErrorV2::WorkerSpawnFailed { worker_id }),
        }
    }
    while shared.counters.started_workers.load(Ordering::Acquire) != config.worker_count {
        if shared.counters.failed_workers.load(Ordering::Acquire) != 0 {
            break;
        }
        if shared.deadline_reached() {
            shared.signal_deadline_and_cancel();
            return Err(AsyncRolloutErrorV2::SchedulerDeadlineExceeded);
        }
        thread::park_timeout(shared.park_poll_duration());
    }

    let active_worker_mask = (1u32 << config.worker_count) - 1;
    let mut broker_episodes: [BrokerEpisodeV2; ASYNC_ROLLOUT_MAX_LOGICAL_LANES_V2] =
        array::from_fn(|_| BrokerEpisodeV2::empty());
    let mut metrics = AsyncRolloutMetricsV2::default();
    let mut batch_digest = BatchMembershipDigestV2::new();
    digest_u64(&mut batch_digest, config.first_episode_id);
    digest_u64(&mut batch_digest, config.episode_count);
    digest_u64(&mut batch_digest, logical_lane_count as u64);
    digest_u64(&mut batch_digest, config.broker_batch_target as u64);
    shared.counters.start.store(true, Ordering::Release);
    for worker_id in 0..config.worker_count {
        shared.wake_worker(worker_id);
    }

    let mut round_decisions = Vec::with_capacity(logical_lane_count);
    let mut round_terminals = Vec::with_capacity(logical_lane_count);
    let mut requests = Vec::with_capacity(logical_lane_count);
    let broker_result = (|| -> Result<(), AsyncRolloutErrorV2> {
        loop {
            if shared.counters.deadline_exceeded.load(Ordering::Acquire)
                || shared.deadline_reached()
            {
                shared.signal_deadline_and_cancel();
                return Err(AsyncRolloutErrorV2::SchedulerDeadlineExceeded);
            }
            let failed =
                shared.counters.failed_workers.load(Ordering::Acquire) & active_worker_mask;
            if failed != 0 {
                let worker_id = failed.trailing_zeros() as usize;
                let cell = &shared.failures[worker_id];
                let phase = worker_phase_from_code(cell.phase.load(Ordering::Acquire))
                    .ok_or(AsyncRolloutErrorV2::BrokerProtocolViolation)?;
                return Err(AsyncRolloutErrorV2::WorkerFailed {
                    worker_id,
                    logical_lane_id: cell.logical_lane_id.load(Ordering::Relaxed),
                    episode_id: cell.episode_id.load(Ordering::Relaxed),
                    phase,
                });
            }

            let done = shared.counters.done_workers.load(Ordering::Acquire) & active_worker_mask;
            if done == active_worker_mask {
                if episodes.len() != episode_count_usize {
                    return Err(AsyncRolloutErrorV2::BrokerProtocolViolation);
                }
                break;
            }
            let quiescent =
                shared.counters.quiescent_workers.load(Ordering::Acquire) & active_worker_mask;
            if (done | quiescent) != active_worker_mask {
                checked_metric_add(&mut metrics.broker_park_attempt_count, 1)?;
                thread::park_timeout(shared.park_poll_duration());
                continue;
            }

            let service_started = config.measure_broker_service_time.then(Instant::now);
            let round_index = metrics.complete_round_count;
            checked_metric_add(&mut metrics.complete_round_count, 1)?;
            round_decisions.clear();
            round_terminals.clear();
            requests.clear();
            let mut ready_workers = 0u32;
            for logical_lane_id in 0..logical_lane_count {
                let mailbox = &shared.lanes[logical_lane_id];
                match mailbox.state.load(Ordering::Acquire) {
                    MAILBOX_WORKER_OWNED => {}
                    MAILBOX_DECISION_READY => {
                        let WorkerMessageV2::Decision(decision) = mailbox.broker_read() else {
                            return Err(AsyncRolloutErrorV2::BrokerProtocolViolation);
                        };
                        round_decisions.push(RoundDecisionV2 {
                            logical_lane_id,
                            decision,
                        });
                        ready_workers |= 1u32 << shared.worker_for_lane(logical_lane_id);
                    }
                    MAILBOX_TERMINAL_READY => {
                        let WorkerMessageV2::Terminal {
                            terminal,
                            learner_action_count,
                            learner_trace_hash,
                        } = mailbox.broker_read()
                        else {
                            return Err(AsyncRolloutErrorV2::BrokerProtocolViolation);
                        };
                        round_terminals.push(RoundTerminalV2 {
                            logical_lane_id,
                            terminal,
                            learner_action_count,
                            learner_trace_hash,
                        });
                        ready_workers |= 1u32 << shared.worker_for_lane(logical_lane_id);
                    }
                    _ => return Err(AsyncRolloutErrorV2::BrokerProtocolViolation),
                }
            }
            for worker_id in 0..config.worker_count {
                let worker_bit = 1u32 << worker_id;
                let is_done = done & worker_bit != 0;
                let has_message = ready_workers & worker_bit != 0;
                if is_done == has_message {
                    return Err(AsyncRolloutErrorV2::BrokerProtocolViolation);
                }
            }

            round_decisions.sort_unstable_by_key(stable_decision_key);
            round_terminals.sort_unstable_by_key(|terminal| {
                (terminal.terminal.episode_id, terminal.logical_lane_id)
            });
            for round_decision in &round_decisions {
                let logical_lane_id = round_decision.logical_lane_id;
                let decision = round_decision.decision;
                broker_episodes[logical_lane_id]
                    .bind(config.learner_policy_seed, decision.episode_id)?;
                let selected_index = uniform_index(
                    &mut broker_episodes[logical_lane_id].learner_policy,
                    decision.legal_action_count,
                );
                requests.push(BrokerRequestV2 {
                    logical_lane_id,
                    decision,
                    action: ActionEnvelopeV2 {
                        episode_id: decision.episode_id,
                        revision: decision.step,
                        physical_decision_id: decision.physical_decision_id,
                        substep_index: decision.substep_index,
                        substep_count: decision.substep_count,
                        legal_action_count: decision.legal_action_count,
                        selected_index,
                    },
                });
            }
            for round_terminal in &round_terminals {
                if episodes.len() == episode_count_usize {
                    return Err(AsyncRolloutErrorV2::BrokerProtocolViolation);
                }
                episodes.push(broker_episodes[round_terminal.logical_lane_id].finish(
                    config.learner_policy_seed,
                    round_terminal.terminal,
                    round_terminal.learner_action_count,
                    round_terminal.learner_trace_hash,
                )?);
            }

            validate_action_batch(&shared, &requests)?;
            digest_complete_round(
                &mut batch_digest,
                round_index,
                &round_terminals,
                &requests,
                config.broker_batch_target,
            );
            let mut published_workers = 0u32;
            for chunk in requests.chunks(config.broker_batch_target) {
                if shared.deadline_reached() {
                    shared.signal_deadline_and_cancel();
                    return Err(AsyncRolloutErrorV2::SchedulerDeadlineExceeded);
                }
                let width = u32::try_from(chunk.len())
                    .map_err(|_| AsyncRolloutErrorV2::BrokerProtocolViolation)?;
                published_workers |= publish_action_chunk(&shared, chunk);
                checked_metric_add(&mut metrics.batch_publication_count, 1)?;
                checked_metric_add(&mut metrics.batch_width_sum, u64::from(width))?;
                checked_metric_add(&mut metrics.learner_action_count, u64::from(width))?;
                metrics.max_batch_width = metrics.max_batch_width.max(width);
                if chunk.len() == config.broker_batch_target {
                    checked_metric_add(&mut metrics.target_flush_count, 1)?;
                } else {
                    checked_metric_add(&mut metrics.quiescent_flush_count, 1)?;
                }
            }
            checked_metric_add(
                &mut metrics.terminal_notifications,
                round_terminals.len() as u64,
            )?;
            if shared.deadline_reached() {
                shared.signal_deadline_and_cancel();
                return Err(AsyncRolloutErrorV2::SchedulerDeadlineExceeded);
            }

            // Every action chunk is published before any terminal is
            // acknowledged or any worker can enter the next global round.
            for round_terminal in &round_terminals {
                shared.lanes[round_terminal.logical_lane_id]
                    .state
                    .store(MAILBOX_WORKER_OWNED, Ordering::Release);
            }
            let terminal_workers = round_terminals.iter().fold(0u32, |workers, terminal| {
                workers | (1u32 << shared.worker_for_lane(terminal.logical_lane_id))
            });
            if published_workers | terminal_workers != ready_workers {
                return Err(AsyncRolloutErrorV2::BrokerProtocolViolation);
            }
            let mut remaining_workers = ready_workers;
            while remaining_workers != 0 {
                let worker_id = remaining_workers.trailing_zeros() as usize;
                remaining_workers &= remaining_workers - 1;
                shared.wake_worker(worker_id);
            }
            if let Some(service_started) = service_started {
                metrics.broker_service_ns = metrics.broker_service_ns.saturating_add(
                    u64::try_from(service_started.elapsed().as_nanos()).unwrap_or(u64::MAX),
                );
            }
        }
        Ok(())
    })();

    if broker_result.is_err() {
        shared.cancel_and_wake_all();
    }
    let join_result = worker_guard.join_every_worker();
    broker_result?;
    join_result?;
    if shared.counters.deadline_exceeded.load(Ordering::Acquire) || shared.deadline_reached() {
        return Err(AsyncRolloutErrorV2::SchedulerDeadlineExceeded);
    }

    episodes.sort_unstable_by_key(|episode| episode.terminal.episode_id);
    if episodes.len() != episode_count_usize
        || episodes.iter().enumerate().any(|(index, episode)| {
            episode.terminal.episode_id != config.first_episode_id + index as u64
        })
    {
        return Err(AsyncRolloutErrorV2::BrokerProtocolViolation);
    }
    let policy_step_count =
        checked_episode_sum(&episodes, |episode| episode.terminal.policy_step_count)?;
    let physical_decision_count = checked_episode_sum(&episodes, |episode| {
        episode.terminal.physical_decision_count
    })?;
    let episode_learner_actions =
        checked_episode_sum(&episodes, |episode| episode.learner_action_count)?;
    let classified_publications = metrics
        .target_flush_count
        .checked_add(metrics.quiescent_flush_count)
        .ok_or(AsyncRolloutErrorV2::BrokerProtocolViolation)?;
    if metrics.batch_width_sum != metrics.learner_action_count
        || classified_publications != metrics.batch_publication_count
        || metrics.terminal_notifications != config.episode_count
        || episode_learner_actions != metrics.learner_action_count
        || metrics.max_batch_width as usize > config.broker_batch_target
    {
        return Err(AsyncRolloutErrorV2::BrokerProtocolViolation);
    }
    metrics.worker_park_attempt_count = shared
        .counters
        .worker_park_attempt_count
        .load(Ordering::Relaxed);
    metrics.batch_membership_digest = batch_digest.finalize();
    if shared.deadline_reached() {
        return Err(AsyncRolloutErrorV2::SchedulerDeadlineExceeded);
    }
    metrics.total_elapsed_ns = u64::try_from(api_started.elapsed().as_nanos()).unwrap_or(u64::MAX);
    Ok(AsyncRolloutResultV2 {
        episodes,
        policy_step_count,
        physical_decision_count,
        metrics,
    })
}

fn compact_terminal(terminal: &RlSessionTerminalV1) -> AsyncRolloutTerminalV1 {
    AsyncRolloutTerminalV1 {
        episode_id: terminal.episode_id,
        terminal_outcome: terminal.terminal_outcome,
        terminal_classification: terminal.terminal_classification,
        terminal_code: terminal.terminal_code,
        winner: terminal.winner,
        terminal_reward: terminal.terminal_reward,
        policy_step_count: terminal.policy_step_count,
        physical_decision_count: terminal.physical_decision_count,
    }
}

fn uniform_index(rng: &mut SplitMix64, legal_action_count: u32) -> u32 {
    debug_assert!(legal_action_count > 0);
    let bound = u64::from(legal_action_count);
    let threshold = bound.wrapping_neg() % bound;
    loop {
        let sample = rng.next_u64();
        if sample >= threshold {
            return (sample % bound) as u32;
        }
    }
}

fn decision_kind_code(kind: FastActorDecisionKindV1) -> u8 {
    match kind {
        FastActorDecisionKindV1::Surface => 0,
        FastActorDecisionKindV1::AttackerInclusion => 1,
        FastActorDecisionKindV1::BlockerInclusion => 2,
    }
}

fn checked_metric_add(target: &mut u64, value: u64) -> Result<(), AsyncRolloutErrorV2> {
    *target = target
        .checked_add(value)
        .ok_or(AsyncRolloutErrorV2::BrokerProtocolViolation)?;
    Ok(())
}

fn checked_episode_sum(
    episodes: &[AsyncRolloutEpisodeV1],
    value: impl Fn(&AsyncRolloutEpisodeV1) -> u64,
) -> Result<u64, AsyncRolloutErrorV2> {
    episodes.iter().try_fold(0u64, |total, episode| {
        total
            .checked_add(value(episode))
            .ok_or(AsyncRolloutErrorV2::BrokerProtocolViolation)
    })
}

fn digest_u32(digest: &mut BatchMembershipDigestV2, value: u32) {
    digest.update(value.to_le_bytes());
}

fn digest_u64(digest: &mut BatchMembershipDigestV2, value: u64) {
    digest.update(value.to_le_bytes());
}

fn digest_complete_round(
    digest: &mut BatchMembershipDigestV2,
    round_index: u64,
    terminals: &[RoundTerminalV2],
    requests: &[BrokerRequestV2],
    batch_target: usize,
) {
    digest.update([0x52]);
    digest_u64(digest, round_index);
    digest_u64(digest, terminals.len() as u64);
    for terminal in terminals {
        digest_u64(digest, terminal.logical_lane_id as u64);
        digest_u64(digest, terminal.terminal.episode_id);
    }
    digest_u64(digest, requests.len() as u64);
    let chunk_count = requests.len().div_ceil(batch_target);
    digest_u64(digest, chunk_count as u64);
    for (chunk_index, chunk) in requests.chunks(batch_target).enumerate() {
        digest.update([0x42]);
        digest_u64(digest, chunk_index as u64);
        digest_u64(digest, chunk.len() as u64);
        for request in chunk {
            let decision = request.decision;
            digest_u64(digest, request.logical_lane_id as u64);
            digest_u64(digest, decision.episode_id);
            digest_u64(digest, decision.step);
            digest_u64(digest, decision.physical_decision_id);
            digest_u32(digest, decision.substep_index);
            digest_u32(digest, decision.substep_count);
            digest.update([decision_kind_code(decision.decision_kind)]);
            digest_u32(digest, decision.legal_action_count);
            digest_u32(digest, request.action.selected_index);
        }
    }
}

fn validate_action_binding(decision: FastActorDecisionV1, action: ActionEnvelopeV2) -> bool {
    action.episode_id == decision.episode_id
        && action.revision == decision.step
        && action.physical_decision_id == decision.physical_decision_id
        && action.substep_index == decision.substep_index
        && action.substep_count == decision.substep_count
        && action.legal_action_count == decision.legal_action_count
        && action.selected_index < decision.legal_action_count
}

fn record_trace(mut trace_hash: u64, decision: FastActorDecisionV1, selected_index: u32) -> u64 {
    trace_hash = hash_bytes(trace_hash, &decision.step.to_le_bytes());
    trace_hash = hash_bytes(trace_hash, &decision.physical_decision_id.to_le_bytes());
    trace_hash = hash_bytes(trace_hash, &decision.substep_index.to_le_bytes());
    trace_hash = hash_bytes(trace_hash, &decision.substep_count.to_le_bytes());
    trace_hash = hash_bytes(trace_hash, &[decision_kind_code(decision.decision_kind)]);
    trace_hash = hash_bytes(trace_hash, &decision.legal_action_count.to_le_bytes());
    hash_bytes(trace_hash, &selected_index.to_le_bytes())
}

fn hash_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV1A64_PRIME);
    }
    hash
}

fn worker_phase_code(phase: AsyncRolloutWorkerPhaseV1) -> u8 {
    match phase {
        AsyncRolloutWorkerPhaseV1::Reset => 1,
        AsyncRolloutWorkerPhaseV1::LearnerActionBinding => 2,
        AsyncRolloutWorkerPhaseV1::LearnerStep => 3,
        AsyncRolloutWorkerPhaseV1::OpponentStep => 4,
        AsyncRolloutWorkerPhaseV1::Panic => 5,
    }
}

fn worker_phase_from_code(code: u8) -> Option<AsyncRolloutWorkerPhaseV1> {
    match code {
        1 => Some(AsyncRolloutWorkerPhaseV1::Reset),
        2 => Some(AsyncRolloutWorkerPhaseV1::LearnerActionBinding),
        3 => Some(AsyncRolloutWorkerPhaseV1::LearnerStep),
        4 => Some(AsyncRolloutWorkerPhaseV1::OpponentStep),
        5 => Some(AsyncRolloutWorkerPhaseV1::Panic),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_rollout::{run_seeded_uniform_async_rollout_v1, AsyncRolloutConfigV1};
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn config() -> AsyncRolloutConfigV2 {
        AsyncRolloutConfigV2 {
            deck_ids: ["Rally".to_string(), "Rally".to_string()],
            learner_seat: PlayerSeatV1::P0,
            environment_seed: 71_501,
            opponent_policy_seed: 72_501,
            learner_policy_seed: 73_501,
            max_physical_decisions: 5_000,
            max_policy_steps: 640_000,
            worker_count: 2,
            sessions_per_worker: 2,
            broker_batch_target: 4,
            first_episode_id: 0,
            episode_count: 16,
            scheduler_timeout: Duration::from_secs(30),
            measure_broker_service_time: true,
        }
    }

    fn shared_for_test(
        first_episode_id: u64,
        end_episode_id: u64,
        logical_lane_count: usize,
        worker_count: usize,
        sessions_per_worker: usize,
    ) -> Arc<SharedRolloutV2> {
        Arc::new(SharedRolloutV2::new(
            first_episode_id,
            end_episode_id,
            logical_lane_count,
            worker_count,
            sessions_per_worker,
            Instant::now().checked_add(Duration::from_secs(10)).unwrap(),
        ))
    }

    fn assert_exact_accounting(result: &AsyncRolloutResultV2, batch_target: usize) {
        let episode_actions: u64 = result
            .episodes
            .iter()
            .map(|episode| episode.learner_action_count)
            .sum();
        assert_eq!(
            result.metrics.batch_width_sum,
            result.metrics.learner_action_count
        );
        assert_eq!(episode_actions, result.metrics.learner_action_count);
        assert_eq!(
            result.metrics.target_flush_count + result.metrics.quiescent_flush_count,
            result.metrics.batch_publication_count
        );
        assert_eq!(
            result.metrics.terminal_notifications,
            result.episodes.len() as u64
        );
        assert!(result.metrics.max_batch_width as usize <= batch_target);
        assert_ne!(result.metrics.batch_membership_digest, [0; 32]);
    }

    fn assert_injected_failure(injection: u8, expected_phase: AsyncRolloutWorkerPhaseV1) {
        let baseline = TEST_ACTIVE_WORKERS_V2.load(Ordering::SeqCst);
        TEST_INJECTION_V2.store(injection, Ordering::SeqCst);
        let mut injected = config();
        injected.episode_count = 4;
        injected.measure_broker_service_time = false;
        let error = run_seeded_uniform_async_rollout_v2(injected).unwrap_err();
        assert!(matches!(
            error,
            AsyncRolloutErrorV2::WorkerFailed { phase, .. } if phase == expected_phase
        ));
        assert_eq!(TEST_INJECTION_V2.load(Ordering::SeqCst), 0);
        assert_eq!(TEST_ACTIVE_WORKERS_V2.load(Ordering::SeqCst), baseline);
    }

    #[test]
    fn rejects_invalid_capacity_before_spawning() {
        let _lock = TEST_LOCK.lock().unwrap();
        let baseline = TEST_ACTIVE_WORKERS_V2.load(Ordering::SeqCst);
        let mut invalid = config();
        invalid.worker_count = 0;
        assert!(matches!(
            run_seeded_uniform_async_rollout_v2(invalid),
            Err(AsyncRolloutErrorV2::InvalidWorkerCount { .. })
        ));

        let mut invalid = config();
        invalid.sessions_per_worker = ASYNC_ROLLOUT_MAX_SESSIONS_PER_WORKER_V2 + 1;
        assert!(matches!(
            run_seeded_uniform_async_rollout_v2(invalid),
            Err(AsyncRolloutErrorV2::InvalidSessionsPerWorker { .. })
        ));

        let mut invalid = config();
        invalid.broker_batch_target = 5;
        assert!(matches!(
            run_seeded_uniform_async_rollout_v2(invalid),
            Err(AsyncRolloutErrorV2::InvalidBrokerBatchTarget { .. })
        ));

        let mut invalid = config();
        invalid.scheduler_timeout = Duration::ZERO;
        assert!(matches!(
            run_seeded_uniform_async_rollout_v2(invalid),
            Err(AsyncRolloutErrorV2::InvalidSchedulerTimeout)
        ));
        assert_eq!(TEST_ACTIVE_WORKERS_V2.load(Ordering::SeqCst), baseline);
    }

    #[test]
    fn multi_session_schedule_matches_v1_episode_and_trace_results() {
        let _lock = TEST_LOCK.lock().unwrap();
        let v2_config = config();
        let v1_config = AsyncRolloutConfigV1 {
            deck_ids: v2_config.deck_ids.clone(),
            learner_seat: v2_config.learner_seat,
            environment_seed: v2_config.environment_seed,
            opponent_policy_seed: v2_config.opponent_policy_seed,
            learner_policy_seed: v2_config.learner_policy_seed,
            max_physical_decisions: v2_config.max_physical_decisions,
            max_policy_steps: v2_config.max_policy_steps,
            worker_count: 4,
            first_episode_id: v2_config.first_episode_id,
            episode_count: v2_config.episode_count,
            measure_broker_service_time: true,
        };
        let v1 = run_seeded_uniform_async_rollout_v1(v1_config).unwrap();
        let v2 = run_seeded_uniform_async_rollout_v2(v2_config).unwrap();
        assert_eq!(v2.episodes, v1.episodes);
        assert_eq!(v2.policy_step_count, v1.policy_step_count);
        assert_eq!(v2.physical_decision_count, v1.physical_decision_count);
        assert!(v2.all_natural());
        assert_eq!(
            v2.metrics.learner_action_count,
            v1.metrics.learner_action_count
        );
        assert_exact_accounting(&v2, 4);
    }

    #[test]
    fn quiescence_flushes_a_short_tail_without_a_timer() {
        let _lock = TEST_LOCK.lock().unwrap();
        let mut short = config();
        short.worker_count = 4;
        short.sessions_per_worker = 4;
        short.broker_batch_target = 16;
        short.episode_count = 3;
        let result = run_seeded_uniform_async_rollout_v2(short).unwrap();
        assert_eq!(result.episodes.len(), 3);
        assert!(result.all_natural());
        assert!(result.metrics.quiescent_flush_count > 0);
        assert_exact_accounting(&result, 16);
    }

    #[test]
    fn repeated_schedule_has_identical_round_batch_digest_and_shape() {
        let _lock = TEST_LOCK.lock().unwrap();
        let mut repeated = config();
        repeated.worker_count = 3;
        repeated.sessions_per_worker = 3;
        repeated.broker_batch_target = 4;
        repeated.episode_count = 12;
        repeated.measure_broker_service_time = false;
        let expected = run_seeded_uniform_async_rollout_v2(repeated.clone()).unwrap();
        assert_exact_accounting(&expected, repeated.broker_batch_target);
        for _ in 0..4 {
            let observed = run_seeded_uniform_async_rollout_v2(repeated.clone()).unwrap();
            assert_eq!(observed.episodes, expected.episodes);
            assert_eq!(
                observed.metrics.batch_membership_digest,
                expected.metrics.batch_membership_digest
            );
            assert_eq!(
                (
                    observed.metrics.complete_round_count,
                    observed.metrics.batch_publication_count,
                    observed.metrics.batch_width_sum,
                    observed.metrics.max_batch_width,
                    observed.metrics.target_flush_count,
                    observed.metrics.quiescent_flush_count,
                ),
                (
                    expected.metrics.complete_round_count,
                    expected.metrics.batch_publication_count,
                    expected.metrics.batch_width_sum,
                    expected.metrics.max_batch_width,
                    expected.metrics.target_flush_count,
                    expected.metrics.quiescent_flush_count,
                )
            );
        }
    }

    #[test]
    fn injected_reset_step_and_panic_failures_cancel_and_join_all_workers() {
        let _lock = TEST_LOCK.lock().unwrap();
        assert_injected_failure(TEST_INJECT_RESET_V2, AsyncRolloutWorkerPhaseV1::Reset);
        assert_injected_failure(
            TEST_INJECT_LEARNER_STEP_V2,
            AsyncRolloutWorkerPhaseV1::LearnerStep,
        );
        assert_injected_failure(
            TEST_INJECT_OPPONENT_STEP_V2,
            AsyncRolloutWorkerPhaseV1::OpponentStep,
        );
        assert_injected_failure(TEST_INJECT_PANIC_V2, AsyncRolloutWorkerPhaseV1::Panic);
    }

    #[test]
    fn cooperative_deadline_fails_closed_and_joins_spawned_workers() {
        let _lock = TEST_LOCK.lock().unwrap();
        let baseline = TEST_ACTIVE_WORKERS_V2.load(Ordering::SeqCst);
        let mut expired = config();
        expired.scheduler_timeout = Duration::from_nanos(1);
        assert!(matches!(
            run_seeded_uniform_async_rollout_v2(expired),
            Err(AsyncRolloutErrorV2::SchedulerDeadlineExceeded)
        ));
        assert_eq!(TEST_ACTIVE_WORKERS_V2.load(Ordering::SeqCst), baseline);
    }

    #[test]
    fn broker_owned_quiescence_blocks_early_action_then_cancellation_wakes_worker() {
        let _lock = TEST_LOCK.lock().unwrap();
        let baseline = TEST_ACTIVE_WORKERS_V2.load(Ordering::SeqCst);
        let shared = shared_for_test(0, 1, 1, 1, 1);
        shared.broker_thread.set(thread::current()).unwrap();
        let worker_config = AsyncRolloutConfigV2 {
            worker_count: 1,
            sessions_per_worker: 1,
            broker_batch_target: 1,
            first_episode_id: 0,
            episode_count: 1,
            scheduler_timeout: Duration::from_secs(10),
            measure_broker_service_time: false,
            ..config()
        };
        let mut guard = WorkerJoinGuardV2::new(Arc::clone(&shared));
        let worker_shared = Arc::clone(&shared);
        guard.install(
            0,
            thread::spawn(move || worker_entry(worker_shared, worker_config, 0)),
        );
        while shared.counters.started_workers.load(Ordering::Acquire) != 1 {
            thread::yield_now();
        }
        shared.counters.start.store(true, Ordering::Release);
        shared.wake_worker(0);
        let wait_deadline = Instant::now() + Duration::from_secs(5);
        while shared.counters.quiescent_workers.load(Ordering::Acquire) & 1 == 0 {
            assert!(
                Instant::now() < wait_deadline,
                "worker did not reach quiescence"
            );
            thread::park_timeout(Duration::from_millis(1));
        }
        assert_eq!(
            shared.lanes[0].state.load(Ordering::Acquire),
            MAILBOX_DECISION_READY
        );
        let WorkerMessageV2::Decision(decision) = shared.lanes[0].broker_read() else {
            panic!("the parked worker must publish a learner decision");
        };
        shared.lanes[0].broker_write_action(ActionEnvelopeV2 {
            episode_id: decision.episode_id,
            revision: decision.step,
            physical_decision_id: decision.physical_decision_id,
            substep_index: decision.substep_index,
            substep_count: decision.substep_count,
            legal_action_count: decision.legal_action_count,
            selected_index: 0,
        });
        shared.lanes[0]
            .state
            .store(MAILBOX_ACTION_READY, Ordering::Release);
        // Inject a stale/spurious wake after quiescence is broker-owned. The
        // worker must re-enter its wait loop without consuming the action.
        shared.worker_threads[0]
            .get()
            .expect("the started worker must publish its thread handle")
            .unpark();
        let ownership_hold = Instant::now() + SCHEDULER_PARK_POLL_V2 * 5;
        while Instant::now() < ownership_hold {
            thread::park_timeout(Duration::from_millis(1));
        }
        assert_eq!(
            shared.counters.quiescent_workers.load(Ordering::Acquire) & 1,
            1,
            "timeouts and stale tokens must not let a worker clear broker-owned quiescence"
        );
        assert_eq!(
            shared.lanes[0].state.load(Ordering::Acquire),
            MAILBOX_ACTION_READY,
            "the worker must not consume an action before the complete round is released"
        );
        shared.cancel_and_wake_all();
        guard.join_every_worker().unwrap();
        assert_eq!(TEST_ACTIVE_WORKERS_V2.load(Ordering::SeqCst), baseline);
    }

    #[test]
    fn rejected_complete_round_publishes_no_partial_action_chunk() {
        let _lock = TEST_LOCK.lock().unwrap();
        let shared = shared_for_test(1, 3, 2, 1, 2);
        let decision_a = FastActorDecisionV1 {
            episode_id: 1,
            step: 7,
            physical_decision_id: 6,
            substep_index: 0,
            substep_count: 1,
            acting_player: PlayerSeatV1::P0,
            decision_kind: FastActorDecisionKindV1::Surface,
            legal_action_count: 2,
        };
        let decision_b = FastActorDecisionV1 {
            episode_id: 2,
            ..decision_a
        };
        shared.lanes[0].worker_publish(
            WorkerMessageV2::Decision(decision_a),
            MAILBOX_DECISION_READY,
        );
        shared.lanes[1].worker_publish(
            WorkerMessageV2::Decision(decision_b),
            MAILBOX_DECISION_READY,
        );
        let requests = [
            BrokerRequestV2 {
                logical_lane_id: 0,
                decision: decision_a,
                action: ActionEnvelopeV2 {
                    episode_id: 1,
                    revision: 7,
                    physical_decision_id: 6,
                    substep_index: 0,
                    substep_count: 1,
                    legal_action_count: 2,
                    selected_index: 1,
                },
            },
            BrokerRequestV2 {
                logical_lane_id: 1,
                decision: decision_b,
                action: ActionEnvelopeV2 {
                    episode_id: 2,
                    revision: 8,
                    physical_decision_id: 6,
                    substep_index: 0,
                    substep_count: 1,
                    legal_action_count: 2,
                    selected_index: 1,
                },
            },
        ];
        assert!(matches!(
            validate_action_batch(&shared, &requests),
            Err(AsyncRolloutErrorV2::ActionBindingMismatch {
                logical_lane_id: 1,
                episode_id: 2
            })
        ));
        assert_eq!(
            shared.lanes[0].state.load(Ordering::Acquire),
            MAILBOX_DECISION_READY
        );
        assert_eq!(
            shared.lanes[1].state.load(Ordering::Acquire),
            MAILBOX_DECISION_READY
        );
    }

    #[test]
    fn join_guard_reports_first_panic_only_after_joining_every_handle() {
        let _lock = TEST_LOCK.lock().unwrap();
        let shared = shared_for_test(1, 2, 2, 2, 1);
        let completed = Arc::new(AtomicBool::new(false));
        let mut guard = WorkerJoinGuardV2::new(shared);
        guard.install(0, thread::spawn(|| panic!("first worker panic")));
        let completed_worker = Arc::clone(&completed);
        guard.install(
            1,
            thread::spawn(move || completed_worker.store(true, Ordering::Release)),
        );
        assert!(matches!(
            guard.join_every_worker(),
            Err(AsyncRolloutErrorV2::WorkerPanicked { worker_id: 0 })
        ));
        assert!(completed.load(Ordering::Acquire));
    }

    #[test]
    fn quiescent_round_handoff_stress_covers_short_and_full_chunks() {
        let _lock = TEST_LOCK.lock().unwrap();
        for iteration in 0..16usize {
            let mut stress = config();
            stress.worker_count = 2;
            stress.sessions_per_worker = 2;
            stress.broker_batch_target = 1 + iteration % 4;
            stress.first_episode_id = 10_000 + iteration as u64 * 8;
            stress.episode_count = 1 + (iteration % 4) as u64;
            stress.measure_broker_service_time = false;
            let result = run_seeded_uniform_async_rollout_v2(stress.clone()).unwrap();
            assert!(result.all_natural());
            assert_exact_accounting(&result, stress.broker_batch_target);
        }
    }
}
