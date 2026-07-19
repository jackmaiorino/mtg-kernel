//! Asynchronous, Rust-native rollout prototype for the fast actor surface.
//!
//! One worker owns one [`FastActorSessionV1`] at a time. Learner decisions and
//! actions cross a fixed shared-memory mailbox; opponent decisions remain
//! worker-local. The broker snapshots whichever lanes are ready instead of
//! imposing an all-lane barrier. Episode-local seed streams make trajectories
//! independent of worker assignment and scheduling.

use crate::rl::{
    derive_env_seed, derive_policy_seed, PlayerSeatV1, TerminalClassificationV1, TerminalOutcomeV1,
    TerminalSafeCodeV2,
};
use crate::rl_session::{
    FastActorDecisionKindV1, FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1,
    RlSessionTerminalV1, SessionDeckIdsV1,
};
use crate::state::SplitMix64;
use std::cell::UnsafeCell;
use std::fmt;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

pub const ASYNC_ROLLOUT_MAX_LANES_V1: usize = 16;

const MAILBOX_WORKER_OWNED: u8 = 0;
const MAILBOX_DECISION_READY: u8 = 1;
const MAILBOX_ACTION_READY: u8 = 2;
const MAILBOX_TERMINAL_READY: u8 = 3;
const MAILBOX_DONE: u8 = 4;

const FNV1A64_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV1A64_PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncRolloutConfigV1 {
    pub deck_ids: SessionDeckIdsV1,
    pub learner_seat: PlayerSeatV1,
    pub environment_seed: u64,
    pub opponent_policy_seed: u64,
    pub learner_policy_seed: u64,
    pub max_physical_decisions: u64,
    pub max_policy_steps: u64,
    pub worker_count: usize,
    pub first_episode_id: u64,
    pub episode_count: u64,
    /// `Instant` sampling once per non-empty ready snapshot. Disable when
    /// measuring the absolute mailbox ceiling.
    pub measure_broker_service_time: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AsyncRolloutTerminalV1 {
    pub episode_id: u64,
    pub terminal_outcome: TerminalOutcomeV1,
    pub terminal_classification: TerminalClassificationV1,
    pub terminal_code: TerminalSafeCodeV2,
    pub winner: Option<PlayerSeatV1>,
    pub terminal_reward: [i32; 2],
    pub policy_step_count: u64,
    pub physical_decision_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AsyncRolloutEpisodeV1 {
    pub terminal: AsyncRolloutTerminalV1,
    pub learner_action_count: u64,
    pub learner_trace_hash: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AsyncRolloutMetricsV1 {
    /// Full successful API latency: validation, allocation, thread lifecycle,
    /// rollout service, joins, canonicalization, and aggregate accounting.
    pub total_elapsed_ns: u64,
    pub ready_snapshot_count: u64,
    pub ready_width_sum: u64,
    pub max_ready_width: u32,
    pub learner_action_count: u64,
    pub action_epoch_publications: u64,
    pub terminal_notifications: u64,
    pub broker_service_ns: u64,
}

impl AsyncRolloutMetricsV1 {
    pub fn mean_ready_width(self) -> f64 {
        if self.ready_snapshot_count == 0 {
            0.0
        } else {
            self.ready_width_sum as f64 / self.ready_snapshot_count as f64
        }
    }

    pub fn mean_broker_service_ns(self) -> f64 {
        if self.ready_snapshot_count == 0 {
            0.0
        } else {
            self.broker_service_ns as f64 / self.ready_snapshot_count as f64
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncRolloutResultV1 {
    /// Strictly ordered by episode id.
    pub episodes: Vec<AsyncRolloutEpisodeV1>,
    pub policy_step_count: u64,
    pub physical_decision_count: u64,
    pub metrics: AsyncRolloutMetricsV1,
}

impl AsyncRolloutResultV1 {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncRolloutWorkerPhaseV1 {
    Reset,
    LearnerActionBinding,
    LearnerStep,
    OpponentStep,
    Panic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncRolloutErrorV1 {
    InvalidWorkerCount {
        requested: usize,
    },
    EmptyEpisodeRange,
    EpisodeRangeOverflow,
    EpisodeCountExceedsAddressSpace {
        requested: u64,
    },
    ResultAllocationFailed {
        requested: u64,
    },
    WorkerSpawnFailed {
        lane_id: usize,
    },
    WorkerFailed {
        lane_id: usize,
        episode_id: u64,
        phase: AsyncRolloutWorkerPhaseV1,
    },
    ActionBindingMismatch {
        lane_id: usize,
        episode_id: u64,
    },
    BrokerProtocolViolation,
    WorkerPanicked {
        lane_id: usize,
    },
}

impl fmt::Display for AsyncRolloutErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWorkerCount { requested } => write!(
                formatter,
                "worker_count {requested} is outside 1..={ASYNC_ROLLOUT_MAX_LANES_V1}"
            ),
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
            Self::WorkerSpawnFailed { lane_id } => {
                write!(
                    formatter,
                    "failed to spawn asynchronous rollout lane {lane_id}"
                )
            }
            Self::WorkerFailed {
                lane_id,
                episode_id,
                phase,
            } => write!(
                formatter,
                "asynchronous rollout lane {lane_id} failed in {phase:?} for episode {episode_id}"
            ),
            Self::ActionBindingMismatch {
                lane_id,
                episode_id,
            } => write!(
                formatter,
                "learner action binding for lane {lane_id} does not match episode {episode_id}"
            ),
            Self::BrokerProtocolViolation => {
                write!(formatter, "asynchronous rollout broker protocol violation")
            }
            Self::WorkerPanicked { lane_id } => {
                write!(formatter, "asynchronous rollout lane {lane_id} panicked")
            }
        }
    }
}

impl std::error::Error for AsyncRolloutErrorV1 {}

#[derive(Debug, Clone, Copy)]
enum WorkerMessageV1 {
    Decision(FastActorDecisionV1),
    Terminal {
        terminal: AsyncRolloutTerminalV1,
        learner_action_count: u64,
        learner_trace_hash: u64,
    },
}

#[repr(align(64))]
struct ReadyBitmapV1(AtomicU32);

#[repr(align(64))]
struct EpisodeCounterV1(AtomicU64);

#[repr(align(64))]
struct WorkerControlV1 {
    started_workers: AtomicUsize,
    start: AtomicBool,
    cancel: AtomicBool,
}

#[derive(Debug, Clone, Copy)]
struct ActionEnvelopeV1 {
    episode_id: u64,
    revision: u64,
    physical_decision_id: u64,
    legal_action_count: u32,
    selected_index: u32,
}

/// The alignment prevents state traffic for adjacent lanes from sharing a
/// cache line. `state` transfers exclusive ownership of the two payload cells:
/// the worker owns them in state 0, the broker owns `message` in states 1/3,
/// and the worker may read `action` only after the broker's release to state 2.
#[repr(align(64))]
struct LaneMailboxV1 {
    state: AtomicU8,
    current_episode_id: AtomicU64,
    failure_episode_id: AtomicU64,
    failure_phase: AtomicU8,
    message: UnsafeCell<MaybeUninit<WorkerMessageV1>>,
    action: UnsafeCell<MaybeUninit<ActionEnvelopeV1>>,
}

// SAFETY: all accesses to the UnsafeCell payloads are sequenced by `state` as
// described above. There is exactly one worker writer/reader and one broker
// writer/reader for each direction.
unsafe impl Sync for LaneMailboxV1 {}

impl LaneMailboxV1 {
    fn new() -> Self {
        Self {
            state: AtomicU8::new(MAILBOX_WORKER_OWNED),
            current_episode_id: AtomicU64::new(u64::MAX),
            failure_episode_id: AtomicU64::new(u64::MAX),
            failure_phase: AtomicU8::new(0),
            message: UnsafeCell::new(MaybeUninit::uninit()),
            action: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    fn worker_publish_message(&self, message: WorkerMessageV1, state: u8) {
        debug_assert!(matches!(
            state,
            MAILBOX_DECISION_READY | MAILBOX_TERMINAL_READY
        ));
        // SAFETY: only the worker writes `message` while it owns the mailbox.
        unsafe { (*self.message.get()).write(message) };
        self.state.store(state, Ordering::Release);
    }

    fn broker_read_message(&self) -> WorkerMessageV1 {
        // SAFETY: an acquire load observed a message-ready state before this
        // call, and the message is Copy and remains initialized until ack.
        unsafe { (*self.message.get()).assume_init_read() }
    }

    fn broker_write_action(&self, action: ActionEnvelopeV1) {
        // SAFETY: the worker is waiting in DECISION_READY and cannot read the
        // action until the broker publishes ACTION_READY.
        unsafe { (*self.action.get()).write(action) };
    }

    fn worker_read_action(&self) -> ActionEnvelopeV1 {
        // SAFETY: an acquire load observed ACTION_READY before this call. The
        // broker does not touch this action again before worker ownership.
        unsafe { (*self.action.get()).assume_init_read() }
    }
}

struct SharedRolloutV1 {
    lanes: [LaneMailboxV1; ASYNC_ROLLOUT_MAX_LANES_V1],
    // Keep the contended ready word away from the read-mostly cancellation
    // controls and the terminal-only episode allocator.
    ready_bitmap: ReadyBitmapV1,
    next_episode_id: EpisodeCounterV1,
    end_episode_id: u64,
    control: WorkerControlV1,
    #[cfg(test)]
    test_instrumentation: Arc<TestRunInstrumentationV1>,
}

impl SharedRolloutV1 {
    fn new(first_episode_id: u64, end_episode_id: u64) -> Self {
        Self {
            lanes: std::array::from_fn(|_| LaneMailboxV1::new()),
            ready_bitmap: ReadyBitmapV1(AtomicU32::new(0)),
            next_episode_id: EpisodeCounterV1(AtomicU64::new(first_episode_id)),
            end_episode_id,
            control: WorkerControlV1 {
                started_workers: AtomicUsize::new(0),
                start: AtomicBool::new(false),
                cancel: AtomicBool::new(false),
            },
            #[cfg(test)]
            test_instrumentation: current_test_run_instrumentation_v1(),
        }
    }

    fn claim_episode(&self) -> Option<u64> {
        let mut current = self.next_episode_id.0.load(Ordering::Relaxed);
        loop {
            if current >= self.end_episode_id {
                return None;
            }
            match self.next_episode_id.0.compare_exchange_weak(
                current,
                current + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Some(current),
                Err(observed) => current = observed,
            }
        }
    }

    fn notify_broker(&self, lane_id: usize) {
        self.ready_bitmap
            .0
            .fetch_or(1u32 << lane_id, Ordering::Release);
    }

    fn notify_failure(&self, lane_id: usize, episode_id: u64, phase: AsyncRolloutWorkerPhaseV1) {
        let mailbox = &self.lanes[lane_id];
        // Failure publication never touches either UnsafeCell. This remains
        // race-free even if an unwind begins while the broker owns `message`.
        mailbox
            .failure_episode_id
            .store(episode_id, Ordering::Relaxed);
        mailbox
            .failure_phase
            .store(worker_phase_code(phase), Ordering::Release);
        self.ready_bitmap
            .0
            .fetch_or(1u32 << (lane_id + 16), Ordering::Release);
    }
}

#[derive(Debug, Clone, Copy)]
struct WorkerFailureV1 {
    episode_id: u64,
    phase: AsyncRolloutWorkerPhaseV1,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct TestRunInstrumentationV1 {
    active_workers: AtomicUsize,
}

#[cfg(test)]
std::thread_local! {
    static ACTIVE_TEST_RUN_INSTRUMENTATION_V1:
        std::cell::RefCell<Option<Arc<TestRunInstrumentationV1>>> = const {
            std::cell::RefCell::new(None)
        };
}

#[cfg(test)]
struct AsyncRolloutTestGuardV1 {
    instrumentation: Arc<TestRunInstrumentationV1>,
    thread_bound: std::marker::PhantomData<std::rc::Rc<()>>,
}

#[cfg(test)]
impl AsyncRolloutTestGuardV1 {
    fn active_workers(&self) -> usize {
        self.instrumentation.active_workers.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
fn acquire_async_rollout_test_guard_v1() -> AsyncRolloutTestGuardV1 {
    let instrumentation = Arc::new(TestRunInstrumentationV1::default());
    ACTIVE_TEST_RUN_INSTRUMENTATION_V1.with(|active| {
        assert!(
            active.replace(Some(Arc::clone(&instrumentation))).is_none(),
            "nested async-rollout-v1 test instrumentation is unsupported"
        );
    });
    AsyncRolloutTestGuardV1 {
        instrumentation,
        thread_bound: std::marker::PhantomData,
    }
}

#[cfg(test)]
fn current_test_run_instrumentation_v1() -> Arc<TestRunInstrumentationV1> {
    ACTIVE_TEST_RUN_INSTRUMENTATION_V1
        .with(|active| active.borrow().clone())
        .unwrap_or_else(|| Arc::new(TestRunInstrumentationV1::default()))
}

#[cfg(test)]
impl Drop for AsyncRolloutTestGuardV1 {
    fn drop(&mut self) {
        ACTIVE_TEST_RUN_INSTRUMENTATION_V1.with(|active| {
            let installed = active.replace(None);
            debug_assert!(installed
                .as_ref()
                .is_some_and(|installed| Arc::ptr_eq(installed, &self.instrumentation)));
        });
    }
}

#[cfg(test)]
struct TestActiveWorkerGuardV1 {
    instrumentation: Arc<TestRunInstrumentationV1>,
}

#[cfg(test)]
impl TestActiveWorkerGuardV1 {
    fn enter(instrumentation: Arc<TestRunInstrumentationV1>) -> Self {
        instrumentation
            .active_workers
            .fetch_add(1, Ordering::SeqCst);
        Self { instrumentation }
    }
}

#[cfg(test)]
impl Drop for TestActiveWorkerGuardV1 {
    fn drop(&mut self) {
        self.instrumentation
            .active_workers
            .fetch_sub(1, Ordering::SeqCst);
    }
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

fn validate_action_binding(decision: FastActorDecisionV1, action: ActionEnvelopeV1) -> bool {
    action.episode_id == decision.episode_id
        && action.revision == decision.step
        && action.physical_decision_id == decision.physical_decision_id
        && action.legal_action_count == decision.legal_action_count
        && action.selected_index < decision.legal_action_count
}

fn wait_for_action(
    shared: &SharedRolloutV1,
    lane_id: usize,
    decision: FastActorDecisionV1,
) -> Result<Option<ActionEnvelopeV1>, WorkerFailureV1> {
    let mailbox = &shared.lanes[lane_id];
    loop {
        if shared.control.cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        match mailbox.state.load(Ordering::Acquire) {
            MAILBOX_ACTION_READY => {
                let action = mailbox.worker_read_action();
                if !validate_action_binding(decision, action) {
                    return Err(WorkerFailureV1 {
                        episode_id: decision.episode_id,
                        phase: AsyncRolloutWorkerPhaseV1::LearnerActionBinding,
                    });
                }
                mailbox.state.store(MAILBOX_WORKER_OWNED, Ordering::Release);
                return Ok(Some(action));
            }
            MAILBOX_DECISION_READY => std::hint::spin_loop(),
            _ => {
                return Err(WorkerFailureV1 {
                    episode_id: decision.episode_id,
                    phase: AsyncRolloutWorkerPhaseV1::LearnerActionBinding,
                });
            }
        }
    }
}

fn wait_for_terminal_ack(shared: &SharedRolloutV1, lane_id: usize) -> bool {
    let mailbox = &shared.lanes[lane_id];
    loop {
        if shared.control.cancel.load(Ordering::Relaxed) {
            return false;
        }
        match mailbox.state.load(Ordering::Acquire) {
            MAILBOX_TERMINAL_READY => std::hint::spin_loop(),
            MAILBOX_WORKER_OWNED => return true,
            _ => return false,
        }
    }
}

fn worker_loop(
    shared: &SharedRolloutV1,
    config: &AsyncRolloutConfigV1,
    lane_id: usize,
) -> Result<(), WorkerFailureV1> {
    shared
        .control
        .started_workers
        .fetch_add(1, Ordering::Release);
    while !shared.control.start.load(Ordering::Acquire) {
        if shared.control.cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        std::hint::spin_loop();
    }

    while !shared.control.cancel.load(Ordering::Relaxed) {
        let Some(episode_id) = shared.claim_episode() else {
            shared.lanes[lane_id]
                .current_episode_id
                .store(u64::MAX, Ordering::Relaxed);
            shared.lanes[lane_id]
                .state
                .store(MAILBOX_DONE, Ordering::Release);
            shared.notify_broker(lane_id);
            return Ok(());
        };
        shared.lanes[lane_id]
            .current_episode_id
            .store(episode_id, Ordering::Relaxed);
        let mut session = FastActorSessionV1::reset_with_decks_and_limits(
            episode_id,
            derive_env_seed(config.environment_seed, episode_id),
            config.max_physical_decisions,
            config.max_policy_steps,
            config.deck_ids.clone(),
        )
        .map_err(|_| WorkerFailureV1 {
            episode_id,
            phase: AsyncRolloutWorkerPhaseV1::Reset,
        })?;
        let mut opponent_policy =
            SplitMix64::seed(derive_policy_seed(config.opponent_policy_seed, episode_id));
        let mut learner_action_count = 0u64;
        let mut learner_trace_hash = hash_bytes(FNV1A64_OFFSET, &episode_id.to_le_bytes());
        let mut response = session.current_response();

        loop {
            if shared.control.cancel.load(Ordering::Relaxed) {
                return Ok(());
            }
            match response {
                FastActorResponseV1::Terminal(terminal) => {
                    shared.lanes[lane_id].worker_publish_message(
                        WorkerMessageV1::Terminal {
                            terminal: compact_terminal(&terminal),
                            learner_action_count,
                            learner_trace_hash,
                        },
                        MAILBOX_TERMINAL_READY,
                    );
                    shared.notify_broker(lane_id);
                    if !wait_for_terminal_ack(shared, lane_id) {
                        return Ok(());
                    }
                    break;
                }
                FastActorResponseV1::Decision(decision)
                    if decision.acting_player == config.learner_seat =>
                {
                    shared.lanes[lane_id].worker_publish_message(
                        WorkerMessageV1::Decision(decision),
                        MAILBOX_DECISION_READY,
                    );
                    shared.notify_broker(lane_id);
                    let Some(action) = wait_for_action(shared, lane_id, decision)? else {
                        return Ok(());
                    };
                    response = session
                        .step(action.episode_id, action.revision, action.selected_index)
                        .map_err(|_| WorkerFailureV1 {
                            episode_id,
                            phase: AsyncRolloutWorkerPhaseV1::LearnerStep,
                        })?;
                    learner_action_count += 1;
                    learner_trace_hash =
                        record_trace(learner_trace_hash, decision, action.selected_index);
                }
                FastActorResponseV1::Decision(decision) => {
                    let selected_index =
                        uniform_index(&mut opponent_policy, decision.legal_action_count);
                    response = session
                        .step(decision.episode_id, decision.step, selected_index)
                        .map_err(|_| WorkerFailureV1 {
                            episode_id,
                            phase: AsyncRolloutWorkerPhaseV1::OpponentStep,
                        })?;
                }
            }
        }
    }
    Ok(())
}

fn worker_entry(shared: Arc<SharedRolloutV1>, config: AsyncRolloutConfigV1, lane_id: usize) {
    #[cfg(test)]
    let _active_worker = TestActiveWorkerGuardV1::enter(Arc::clone(&shared.test_instrumentation));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        worker_loop(&shared, &config, lane_id)
    }));
    if shared.control.cancel.load(Ordering::Relaxed) {
        return;
    }
    let failure = match result {
        Ok(Ok(())) => return,
        Ok(Err(failure)) => failure,
        Err(_) => WorkerFailureV1 {
            episode_id: shared.lanes[lane_id]
                .current_episode_id
                .load(Ordering::Relaxed),
            phase: AsyncRolloutWorkerPhaseV1::Panic,
        },
    };
    shared.notify_failure(lane_id, failure.episode_id, failure.phase);
}

#[derive(Debug, Clone, Copy)]
struct BrokerEpisodeV1 {
    active: bool,
    episode_id: u64,
    learner_policy: SplitMix64,
}

impl BrokerEpisodeV1 {
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
    ) -> Result<(), AsyncRolloutErrorV1> {
        if !self.active {
            self.active = true;
            self.episode_id = episode_id;
            self.learner_policy =
                SplitMix64::seed(derive_policy_seed(learner_policy_seed, episode_id));
        }
        if self.episode_id != episode_id {
            return Err(AsyncRolloutErrorV1::BrokerProtocolViolation);
        }
        Ok(())
    }

    fn finish(
        &mut self,
        learner_policy_seed: u64,
        terminal: AsyncRolloutTerminalV1,
        learner_action_count: u64,
        learner_trace_hash: u64,
    ) -> Result<AsyncRolloutEpisodeV1, AsyncRolloutErrorV1> {
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

fn record_trace(mut trace_hash: u64, decision: FastActorDecisionV1, selected_index: u32) -> u64 {
    trace_hash = hash_bytes(trace_hash, &decision.step.to_le_bytes());
    trace_hash = hash_bytes(trace_hash, &decision.physical_decision_id.to_le_bytes());
    trace_hash = hash_bytes(trace_hash, &decision.substep_index.to_le_bytes());
    trace_hash = hash_bytes(trace_hash, &decision.substep_count.to_le_bytes());
    trace_hash = hash_bytes(
        trace_hash,
        &[match decision.decision_kind {
            FastActorDecisionKindV1::Surface => 0,
            FastActorDecisionKindV1::AttackerInclusion => 1,
            FastActorDecisionKindV1::BlockerInclusion => 2,
        }],
    );
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

#[derive(Debug, Clone, Copy)]
struct BrokerRequestV1 {
    lane_id: usize,
    decision: FastActorDecisionV1,
    action: ActionEnvelopeV1,
}

fn publish_action_snapshot(
    shared: &SharedRolloutV1,
    requests: &[BrokerRequestV1],
) -> Result<(), AsyncRolloutErrorV1> {
    // Validate every binding before writing or publishing any action. A
    // rejected snapshot therefore leaves every mailbox in DECISION_READY and
    // every FastActorSession unchanged.
    for request in requests {
        if request.lane_id >= ASYNC_ROLLOUT_MAX_LANES_V1
            || shared.lanes[request.lane_id].state.load(Ordering::Acquire) != MAILBOX_DECISION_READY
            || !validate_action_binding(request.decision, request.action)
        {
            return Err(AsyncRolloutErrorV1::ActionBindingMismatch {
                lane_id: request.lane_id,
                episode_id: request.decision.episode_id,
            });
        }
    }
    for request in requests {
        shared.lanes[request.lane_id].broker_write_action(request.action);
    }
    for request in requests {
        shared.lanes[request.lane_id]
            .state
            .store(MAILBOX_ACTION_READY, Ordering::Release);
    }
    Ok(())
}

fn sort_requests_by_episode(requests: &mut [BrokerRequestV1]) {
    // The fixed upper bound is 16, so insertion sort avoids allocation and is
    // efficient for the already-nearly-ordered common case.
    for index in 1..requests.len() {
        let mut cursor = index;
        while cursor > 0
            && requests[cursor - 1].decision.episode_id > requests[cursor].decision.episode_id
        {
            requests.swap(cursor - 1, cursor);
            cursor -= 1;
        }
    }
}

struct WorkerJoinGuardV1 {
    shared: Arc<SharedRolloutV1>,
    handles: [Option<JoinHandle<()>>; ASYNC_ROLLOUT_MAX_LANES_V1],
    armed: bool,
}

impl WorkerJoinGuardV1 {
    fn new(shared: Arc<SharedRolloutV1>) -> Self {
        Self {
            shared,
            handles: std::array::from_fn(|_| None),
            armed: true,
        }
    }

    fn install(&mut self, lane_id: usize, handle: JoinHandle<()>) {
        debug_assert!(self.handles[lane_id].is_none());
        self.handles[lane_id] = Some(handle);
    }

    fn join_every_worker(&mut self) -> Result<(), AsyncRolloutErrorV1> {
        let mut first_panicked_lane = None;
        for (lane_id, handle) in self.handles.iter_mut().enumerate() {
            if let Some(handle) = handle.take() {
                if handle.join().is_err() && first_panicked_lane.is_none() {
                    first_panicked_lane = Some(lane_id);
                }
            }
        }
        self.armed = false;
        match first_panicked_lane {
            Some(lane_id) => Err(AsyncRolloutErrorV1::WorkerPanicked { lane_id }),
            None => Ok(()),
        }
    }
}

impl Drop for WorkerJoinGuardV1 {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.shared.control.cancel.store(true, Ordering::Release);
        self.shared.control.start.store(true, Ordering::Release);
        // Drop cannot report a panic, but it must still wait for every worker;
        // never let dropping an earlier JoinHandle detach later lanes.
        let _ = self.join_every_worker();
    }
}

fn read_worker_failure(
    shared: &SharedRolloutV1,
    failure_bits: u32,
) -> Result<AsyncRolloutErrorV1, AsyncRolloutErrorV1> {
    let lane_id = failure_bits.trailing_zeros() as usize;
    let mailbox = &shared.lanes[lane_id];
    let phase = worker_phase_from_code(mailbox.failure_phase.load(Ordering::Acquire))
        .ok_or(AsyncRolloutErrorV1::BrokerProtocolViolation)?;
    Ok(AsyncRolloutErrorV1::WorkerFailed {
        lane_id,
        episode_id: mailbox.failure_episode_id.load(Ordering::Relaxed),
        phase,
    })
}

/// Run a finite seeded-uniform rollout schedule through the asynchronous
/// request/response path.
pub fn run_seeded_uniform_async_rollout_v1(
    config: AsyncRolloutConfigV1,
) -> Result<AsyncRolloutResultV1, AsyncRolloutErrorV1> {
    let api_started = Instant::now();
    if !(1..=ASYNC_ROLLOUT_MAX_LANES_V1).contains(&config.worker_count) {
        return Err(AsyncRolloutErrorV1::InvalidWorkerCount {
            requested: config.worker_count,
        });
    }
    if config.episode_count == 0 {
        return Err(AsyncRolloutErrorV1::EmptyEpisodeRange);
    }
    let end_episode_id = config
        .first_episode_id
        .checked_add(config.episode_count)
        .ok_or(AsyncRolloutErrorV1::EpisodeRangeOverflow)?;
    let episode_count_usize = usize::try_from(config.episode_count).map_err(|_| {
        AsyncRolloutErrorV1::EpisodeCountExceedsAddressSpace {
            requested: config.episode_count,
        }
    })?;
    let mut episodes = Vec::new();
    episodes
        .try_reserve_exact(episode_count_usize)
        .map_err(|_| AsyncRolloutErrorV1::ResultAllocationFailed {
            requested: config.episode_count,
        })?;
    let shared = Arc::new(SharedRolloutV1::new(
        config.first_episode_id,
        end_episode_id,
    ));
    // This guard exists before the first spawn and owns each handle immediately
    // after creation. Every subsequent `?`, return, or unwind cancels, starts
    // any not-yet-released workers, and joins all installed handles.
    let mut worker_guard = WorkerJoinGuardV1::new(Arc::clone(&shared));
    for lane_id in 0..config.worker_count {
        let worker_shared = Arc::clone(&shared);
        let worker_config = config.clone();
        let handle = thread::Builder::new()
            .name(format!("mtg-async-rollout-v1-{lane_id}"))
            .spawn(move || worker_entry(worker_shared, worker_config, lane_id));
        match handle {
            Ok(handle) => worker_guard.install(lane_id, handle),
            Err(_) => return Err(AsyncRolloutErrorV1::WorkerSpawnFailed { lane_id }),
        }
    }
    while shared.control.started_workers.load(Ordering::Acquire) != config.worker_count {
        std::hint::spin_loop();
    }

    let mut broker_episodes: [BrokerEpisodeV1; ASYNC_ROLLOUT_MAX_LANES_V1] =
        std::array::from_fn(|_| BrokerEpisodeV1::empty());
    let mut metrics = AsyncRolloutMetricsV1::default();
    let mut done_workers = 0usize;
    let active_mask = (1u32 << config.worker_count) - 1;
    shared.control.start.store(true, Ordering::Release);

    let broker_result = (|| -> Result<(), AsyncRolloutErrorV1> {
        while done_workers < config.worker_count {
            let notification = shared.ready_bitmap.0.swap(0, Ordering::Acquire);
            if notification == 0 {
                std::hint::spin_loop();
                continue;
            }
            let failure_bits = (notification >> 16) & active_mask;
            if failure_bits != 0 {
                return Err(read_worker_failure(&shared, failure_bits)?);
            }
            let ready = notification & active_mask;
            if ready == 0 {
                continue;
            }
            let mut requests: [MaybeUninit<BrokerRequestV1>; ASYNC_ROLLOUT_MAX_LANES_V1] =
                std::array::from_fn(|_| MaybeUninit::uninit());
            let mut request_count = 0usize;

            let mut remaining = ready;
            while remaining != 0 {
                let lane_id = remaining.trailing_zeros() as usize;
                remaining &= remaining - 1;
                let mailbox = &shared.lanes[lane_id];
                match mailbox.state.load(Ordering::Acquire) {
                    MAILBOX_DECISION_READY => {
                        let WorkerMessageV1::Decision(decision) = mailbox.broker_read_message()
                        else {
                            return Err(AsyncRolloutErrorV1::BrokerProtocolViolation);
                        };
                        broker_episodes[lane_id]
                            .bind(config.learner_policy_seed, decision.episode_id)?;
                        let selected_index = uniform_index(
                            &mut broker_episodes[lane_id].learner_policy,
                            decision.legal_action_count,
                        );
                        requests[request_count].write(BrokerRequestV1 {
                            lane_id,
                            decision,
                            action: ActionEnvelopeV1 {
                                episode_id: decision.episode_id,
                                revision: decision.step,
                                physical_decision_id: decision.physical_decision_id,
                                legal_action_count: decision.legal_action_count,
                                selected_index,
                            },
                        });
                        request_count += 1;
                    }
                    MAILBOX_TERMINAL_READY => {
                        let WorkerMessageV1::Terminal {
                            terminal,
                            learner_action_count,
                            learner_trace_hash,
                        } = mailbox.broker_read_message()
                        else {
                            return Err(AsyncRolloutErrorV1::BrokerProtocolViolation);
                        };
                        if episodes.len() == episode_count_usize {
                            return Err(AsyncRolloutErrorV1::BrokerProtocolViolation);
                        }
                        episodes.push(broker_episodes[lane_id].finish(
                            config.learner_policy_seed,
                            terminal,
                            learner_action_count,
                            learner_trace_hash,
                        )?);
                        metrics.terminal_notifications += 1;
                        mailbox.state.store(MAILBOX_WORKER_OWNED, Ordering::Release);
                    }
                    MAILBOX_DONE => done_workers += 1,
                    _ => return Err(AsyncRolloutErrorV1::BrokerProtocolViolation),
                }
            }

            if request_count > 0 {
                let service_started = config.measure_broker_service_time.then(Instant::now);
                // SAFETY: the first request_count elements were initialized in
                // the loop above and BrokerRequestV1 is Copy.
                let requests = unsafe {
                    std::slice::from_raw_parts_mut(
                        requests.as_mut_ptr().cast::<BrokerRequestV1>(),
                        request_count,
                    )
                };
                sort_requests_by_episode(requests);
                publish_action_snapshot(&shared, requests)?;
                let width = u32::try_from(request_count)
                    .expect("request_count is bounded by the fixed 16 lanes");
                metrics.ready_snapshot_count += 1;
                metrics.ready_width_sum += u64::from(width);
                metrics.max_ready_width = metrics.max_ready_width.max(width);
                metrics.learner_action_count += u64::from(width);
                metrics.action_epoch_publications += u64::from(width);
                if let Some(service_started) = service_started {
                    metrics.broker_service_ns = metrics.broker_service_ns.saturating_add(
                        u64::try_from(service_started.elapsed().as_nanos()).unwrap_or(u64::MAX),
                    );
                }
            }
        }
        Ok(())
    })();

    if broker_result.is_err() {
        shared.control.cancel.store(true, Ordering::Release);
    }
    let join_result = worker_guard.join_every_worker();
    broker_result?;
    join_result?;

    episodes.sort_unstable_by_key(|episode| episode.terminal.episode_id);
    if episodes.len() != episode_count_usize
        || episodes.iter().enumerate().any(|(index, episode)| {
            episode.terminal.episode_id != config.first_episode_id + index as u64
        })
    {
        return Err(AsyncRolloutErrorV1::BrokerProtocolViolation);
    }
    let policy_step_count = episodes.iter().fold(0u64, |total, episode| {
        total.saturating_add(episode.terminal.policy_step_count)
    });
    let physical_decision_count = episodes.iter().fold(0u64, |total, episode| {
        total.saturating_add(episode.terminal.physical_decision_count)
    });
    metrics.total_elapsed_ns = u64::try_from(api_started.elapsed().as_nanos()).unwrap_or(u64::MAX);
    Ok(AsyncRolloutResultV1 {
        episodes,
        policy_step_count,
        physical_decision_count,
        metrics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(worker_count: usize) -> AsyncRolloutConfigV1 {
        AsyncRolloutConfigV1 {
            deck_ids: ["Rally".to_string(), "Rally".to_string()],
            learner_seat: PlayerSeatV1::P0,
            environment_seed: 0xE070_D1A6_0000_0001,
            opponent_policy_seed: 0x0AA0_D1A6_0000_0001,
            learner_policy_seed: 0x1EA2_D1A6_0000_0001,
            max_physical_decisions: 4_096,
            max_policy_steps: 524_288,
            worker_count,
            first_episode_id: 50_000,
            episode_count: 4,
            measure_broker_service_time: false,
        }
    }

    #[test]
    fn asynchronous_n1_and_n16_are_exact_episode_invariant_and_natural() {
        let n1 = run_seeded_uniform_async_rollout_v1(config(1)).unwrap();
        let n16 = run_seeded_uniform_async_rollout_v1(config(16)).unwrap();
        assert!(n1.all_natural());
        assert_eq!(n1.episodes, n16.episodes);
        assert_eq!(n1.policy_step_count, n16.policy_step_count);
        assert_eq!(n1.physical_decision_count, n16.physical_decision_count);
        assert_eq!(
            n1.metrics.learner_action_count,
            n16.metrics.learner_action_count
        );
    }

    #[test]
    fn rejected_snapshot_publishes_no_action_epoch() {
        let shared = SharedRolloutV1::new(1, 3);
        let decision_a = FastActorDecisionV1 {
            episode_id: 1,
            step: 7,
            environment_revision: 7,
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
        shared.lanes[0].worker_publish_message(
            WorkerMessageV1::Decision(decision_a),
            MAILBOX_DECISION_READY,
        );
        shared.lanes[1].worker_publish_message(
            WorkerMessageV1::Decision(decision_b),
            MAILBOX_DECISION_READY,
        );
        let requests = [
            BrokerRequestV1 {
                lane_id: 0,
                decision: decision_a,
                action: ActionEnvelopeV1 {
                    episode_id: 1,
                    revision: 7,
                    physical_decision_id: 6,
                    legal_action_count: 2,
                    selected_index: 1,
                },
            },
            BrokerRequestV1 {
                lane_id: 1,
                decision: decision_b,
                action: ActionEnvelopeV1 {
                    episode_id: 2,
                    revision: 8,
                    physical_decision_id: 6,
                    legal_action_count: 2,
                    selected_index: 1,
                },
            },
        ];
        assert!(matches!(
            publish_action_snapshot(&shared, &requests),
            Err(AsyncRolloutErrorV1::ActionBindingMismatch {
                lane_id: 1,
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
    fn impossible_result_capacity_fails_before_spawning_workers() {
        let test_state = acquire_async_rollout_test_guard_v1();
        assert_eq!(test_state.active_workers(), 0);
        let mut huge = config(1);
        huge.first_episode_id = 0;
        huge.episode_count = u64::MAX;
        assert!(matches!(
            run_seeded_uniform_async_rollout_v1(huge),
            Err(AsyncRolloutErrorV1::ResultAllocationFailed {
                requested: u64::MAX
            }) | Err(AsyncRolloutErrorV1::EpisodeCountExceedsAddressSpace {
                requested: u64::MAX
            })
        ));
        assert_eq!(test_state.active_workers(), 0);
    }

    #[test]
    fn unwind_guard_cancels_starts_and_joins_spawned_worker() {
        let test_state = acquire_async_rollout_test_guard_v1();
        let shared = Arc::new(SharedRolloutV1::new(70_000, 70_001));
        let worker_state = Arc::clone(&test_state.instrumentation);
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe({
            let shared = Arc::clone(&shared);
            move || {
                let mut guard = WorkerJoinGuardV1::new(Arc::clone(&shared));
                let worker_shared = Arc::clone(&shared);
                let worker_config = config(1);
                guard.install(
                    0,
                    thread::spawn(move || worker_entry(worker_shared, worker_config, 0)),
                );
                while worker_state.active_workers.load(Ordering::SeqCst) == 0 {
                    std::hint::spin_loop();
                }
                panic!("exercise asynchronous rollout unwind cleanup");
            }
        }));
        assert!(unwind.is_err());
        assert_eq!(test_state.active_workers(), 0);
        assert!(shared.control.cancel.load(Ordering::Acquire));
        assert!(shared.control.start.load(Ordering::Acquire));
    }

    #[test]
    fn unscoped_parallel_v1_run_cannot_contaminate_owned_worker_count() {
        let test_state = acquire_async_rollout_test_guard_v1();
        let shared = Arc::new(SharedRolloutV1::new(70_000, 70_001));
        let mut guard = WorkerJoinGuardV1::new(Arc::clone(&shared));
        let worker_shared = Arc::clone(&shared);
        let worker_config = config(1);
        guard.install(
            0,
            thread::spawn(move || worker_entry(worker_shared, worker_config, 0)),
        );
        while test_state.active_workers() == 0 {
            std::hint::spin_loop();
        }

        let unrelated = thread::spawn(|| run_seeded_uniform_async_rollout_v1(config(1)));
        assert!(unrelated.join().unwrap().is_ok());
        assert_eq!(test_state.active_workers(), 1);

        shared.control.cancel.store(true, Ordering::Release);
        shared.control.start.store(true, Ordering::Release);
        guard.join_every_worker().unwrap();
        assert_eq!(test_state.active_workers(), 0);
    }

    #[test]
    fn join_guard_reports_first_panic_only_after_joining_every_handle() {
        let shared = Arc::new(SharedRolloutV1::new(1, 2));
        let completed = Arc::new(AtomicBool::new(false));
        let mut guard = WorkerJoinGuardV1::new(shared);
        guard.install(0, thread::spawn(|| panic!("first worker panic")));
        let completed_worker = Arc::clone(&completed);
        guard.install(
            1,
            thread::spawn(move || completed_worker.store(true, Ordering::Release)),
        );
        assert!(matches!(
            guard.join_every_worker(),
            Err(AsyncRolloutErrorV1::WorkerPanicked { lane_id: 0 })
        ));
        assert!(completed.load(Ordering::Acquire));
    }
}
