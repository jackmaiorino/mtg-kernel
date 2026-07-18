//! Deterministic asynchronous rollout with worker-side typed encoding and
//! synchronous broker-side batch scoring.
//!
//! This module is deliberately additive to [`crate::async_rollout_v2`]. The
//! legacy seeded-uniform scheduler keeps its sampler, mailbox, digest, and
//! public behavior unchanged. Here workers advance fixed-stride logical lanes
//! to global quiescent rounds, retain all consume authority and operational
//! action-object rows, and send only active typed model prefixes to the broker.
//! The broker stable-sorts a complete round, scores every deterministic chunk,
//! validates every output, samples with a versioned per-episode seed schedule,
//! and only then replies to any worker.
//!
//! A cooperative deadline bounds scheduler waits. As with async rollout v2,
//! Rust cannot safely kill a worker stuck inside a non-returning engine call or
//! a scorer that never returns; hard enforcement requires process isolation.

use crate::async_rollout::{AsyncRolloutEpisodeV1, AsyncRolloutTerminalV1};
use crate::async_rollout_v2::{
    AsyncRolloutConfigV2, ASYNC_ROLLOUT_MAX_SESSIONS_PER_WORKER_V2, ASYNC_ROLLOUT_MAX_WORKERS_V2,
};
use crate::fast_sampler::{
    FastCategoricalError, FastCategoricalScratch, FAST_CATEGORICAL_MAX_ACTIONS,
};
use crate::flat_policy_v1::{
    FlatCompletedDungeonV1, FlatContextElementKindV1, FlatContextKindV1, FlatContextPathElementV1,
    FlatDecisionBindingV1, FlatDecisionEncoderV1, FlatDecisionErrorV1, FlatDecisionV1,
    FlatEffectSubtypeChangeKindV1, FlatEffectSubtypeChangeV1, FlatGlobalsV1,
    FlatObjectAbilityUseV1, FlatObjectCoreV1, FlatObjectGoadV1, FlatObjectGroupV1,
    FlatObjectSourceKindV1, FlatObjectSubtypeV1, FlatPendingEffectChoiceV1,
    FlatPolicyContractDigestsV1, FlatRelationV1, FlatRelativePlayerV1, FlatScorerActionCoreV1,
    FlatScorerActionRefV1, FlatScoringDecisionViewV1, FlatScoringOwnedBuffersV1,
    FLAT_ACTION_REF_INTERNAL_TO_PROJECTION_V1, FLAT_ACTION_REF_PROJECTION_ROLE_MAPPING_VERSION_V1,
    FLAT_POLICY_CONTEXT_SUBROLE_MAPPING_VERSION_V1, FLAT_POLICY_CONTRACT_DIGESTS_V1,
    FLAT_POLICY_ENUM_MAPPING_VERSION_V1, FLAT_POLICY_FEATURE_INVENTORY_VERSION_V1,
    FLAT_POLICY_OBJECT_GROUP_MAPPING_VERSION_V1, FLAT_POLICY_RELATION_ROLE_MAPPING_VERSION_V1,
    FLAT_POLICY_TYPED_LAYOUT_VERSION_V1, FLAT_SCORER_ACTION_REF_VERSION_V1,
    FLAT_SCORER_PACKET_VERSION_V1, FLAT_SCORER_VISIBLE_MANIFEST_V1,
    FLAT_SCORER_VISIBLE_MANIFEST_VERSION_V1,
};
use crate::rl::{
    derive_env_seed, derive_policy_seed, PlayerSeatV1, TerminalClassificationV1, TerminalSafeCodeV2,
};
use crate::rl_session::{
    FastActorDecisionKindV1, FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1,
    RlSessionTerminalV1, FLAT_ACTION_CANDIDATE_COMMITMENT_VERSION_V1,
    FLAT_ACTION_CARD_TOKEN_MAPPING_VERSION_V1, FLAT_ACTION_DECISION_SLICE_VERSION_V1,
    FLAT_ACTION_REF_ROLE_MAPPING_VERSION_V1,
};
use crate::state::SplitMix64;
use std::fmt;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};
use std::time::Instant;

pub const ASYNC_FLAT_SCORED_ROLLOUT_VERSION_V1: u32 = 1;
pub const ASYNC_FLAT_SCORED_SAMPLER_VERSION_V1: u32 = 1;
pub const ASYNC_FLAT_SCORED_SPLITMIX_GAMMA_V1: u64 = 0x9E37_79B9_7F4A_7C15;
pub const ASYNC_FLAT_SCORED_SAMPLER_ID_V1: &str =
    "derive-policy-seed-plus-decision-ordinal-splitmix-gamma-fast-categorical-v1";

const FNV1A64_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV1A64_PRIME: u64 = 0x0000_0100_0000_01b3;

#[cfg(test)]
static TEST_DELAY_WORKER_ID_V1: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(usize::MAX);
#[cfg(test)]
static TEST_DELAY_WORKER_MS_V1: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(test)]
static TEST_DELAY_FINAL_REDUCTION_MS_V1: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
#[cfg(test)]
static TEST_ENTERED_FINAL_REDUCTION_V1: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
static TEST_EXIT_AFTER_ROUND_WORKER_ID_V1: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(usize::MAX);
#[cfg(test)]
static TEST_CONSUMED_ACTION_COUNT_V1: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
#[cfg(test)]
static TEST_TERMINAL_ACK_COUNT_V1: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
#[cfg(test)]
static TEST_CAPTURE_ACTION_EVENTS_V1: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
static TEST_ACTION_EVENTS_V1: std::sync::Mutex<Vec<TestScoredActionEventV1>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct TestScoredActionEventV1 {
    expected: FastActorDecisionV1,
    learner_ordinal: u64,
    safe_packet_payload: String,
    scorer_value_bits: u32,
    selected_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlatScorerContractV1 {
    pub scorer_packet_version: u32,
    pub scorer_action_ref_version: u32,
    pub scorer_visible_manifest_version: u32,
    pub scorer_visible_manifest: &'static str,
    pub typed_layout_version: u32,
    pub feature_inventory_version: u32,
    pub enum_mapping_version: u32,
    pub object_group_mapping_version: u32,
    pub relation_role_mapping_version: u32,
    pub context_subrole_mapping_version: u32,
    pub action_ref_projection_role_mapping_version: u32,
    pub action_slice_version: u32,
    pub action_ref_role_mapping_version: u32,
    pub card_token_mapping_version: u32,
    pub candidate_commitment_version: u32,
    pub card_db_hash: u64,
    pub contract_digests: FlatPolicyContractDigestsV1,
}

pub struct FlatScoringBatchViewV1<'a> {
    contract: FlatScorerContractV1,
    decisions: &'a [RoundDecisionV1],
    action_offsets: &'a [usize],
}

impl<'a> FlatScoringBatchViewV1<'a> {
    pub fn contract(&self) -> FlatScorerContractV1 {
        self.contract
    }

    pub fn decision_count(&self) -> usize {
        self.decisions.len()
    }

    pub fn decision(&self, index: usize) -> Option<FlatScoringDecisionViewV1<'_>> {
        self.decisions
            .get(index)
            .map(|decision| decision.packet.scorer_view())
    }

    /// Prefix offsets into the caller-owned flattened logit output. The slice
    /// has `decision_count + 1` entries, starts at zero, and ends at the exact
    /// active action count for this batch.
    pub fn action_offsets(&self) -> &[usize] {
        self.action_offsets
    }

    pub fn total_action_count(&self) -> usize {
        self.action_offsets.last().copied().unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlatBatchScorerErrorV1 {
    pub code: u32,
}

impl FlatBatchScorerErrorV1 {
    pub const fn new(code: u32) -> Self {
        Self { code }
    }
}

impl fmt::Display for FlatBatchScorerErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "flat batch scorer rejected input with code {}",
            self.code
        )
    }
}

impl std::error::Error for FlatBatchScorerErrorV1 {}

/// Synchronous scorer boundary. The caller pre-fills both output slices with
/// NaNs and rejects an error, an untouched element, or any non-finite result.
/// Logits are flattened by the immutable broker-supplied action offsets; one
/// value is required per decision.
pub trait FlatBatchScorerV1 {
    fn score_batch_v1(
        &mut self,
        batch: &FlatScoringBatchViewV1<'_>,
        action_logits: &mut [f32],
        values: &mut [f32],
    ) -> Result<(), FlatBatchScorerErrorV1>;
}

/// Borrowed, fully validated scorer output for one selected learner action.
/// The observer must copy only the fields it elects to stage; none of these
/// buffers are transferred out of the broker.
pub(crate) struct FlatScoredSelectedEventV1<'a> {
    pub(crate) expected: FastActorDecisionV1,
    pub(crate) binding: FlatDecisionBindingV1,
    pub(crate) learner_ordinal: u64,
    pub(crate) action_seed: u64,
    pub(crate) selected_index: u32,
    pub(crate) raw_action_logits: &'a [f32],
    pub(crate) predicted_value_bits: u32,
    pub(crate) decision: FlatScoringDecisionViewV1<'a>,
}

/// Compact terminal notification paired with the selected-action stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FlatScoredTerminalEventV1 {
    pub(crate) terminal: AsyncRolloutTerminalV1,
    pub(crate) learner_action_count: u64,
    pub(crate) learner_trace_hash: u64,
}

/// Crate-private staging boundary for a future native learner. Implementations
/// may only mutate inert owned staging: neither observation callback nor
/// `finish_v1` may publish through shared state, I/O, or another side effect.
/// The core can still reject after `finish_v1`, so that method must only return
/// an inert owned value whose publication waits for this run function to return
/// it successfully.
pub(crate) trait FlatScoredTrajectoryObserverV1: Sized {
    type Error;
    type Output;

    /// Compile-time capability used to preserve the original public no-op hot
    /// path. Real observers retain complete-round prevalidation semantics.
    const OBSERVES_TRAJECTORY: bool = true;

    fn observe_selected_v1(
        &mut self,
        event: FlatScoredSelectedEventV1<'_>,
    ) -> Result<(), Self::Error>;

    fn observe_terminal_v1(&mut self, event: FlatScoredTerminalEventV1) -> Result<(), Self::Error>;

    /// Converts private staging into an inert canonical owned value only after
    /// the rollout has joined workers and validated the aggregate result. The
    /// core retains authority to discard that value at its final deadline gate.
    fn finish_v1(self) -> Result<Self::Output, Self::Error>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlatScoredObserverPhaseV1 {
    Selected,
    Terminal,
    Finish,
}

#[derive(Debug)]
pub(crate) enum AsyncFlatScoredObservedRunErrorV1<E> {
    Rollout(AsyncFlatScoredRolloutErrorV1),
    ObserverFailed {
        phase: FlatScoredObserverPhaseV1,
        error: E,
    },
    ObserverPanicked {
        phase: FlatScoredObserverPhaseV1,
    },
}

impl<E> From<AsyncFlatScoredRolloutErrorV1> for AsyncFlatScoredObservedRunErrorV1<E> {
    fn from(error: AsyncFlatScoredRolloutErrorV1) -> Self {
        Self::Rollout(error)
    }
}

enum FlatScoredObserverInterruptionV1<E> {
    Failed {
        phase: FlatScoredObserverPhaseV1,
        error: E,
    },
    Panicked {
        phase: FlatScoredObserverPhaseV1,
    },
}

impl<E> FlatScoredObserverInterruptionV1<E> {
    fn into_run_error(self) -> AsyncFlatScoredObservedRunErrorV1<E> {
        match self {
            Self::Failed { phase, error } => {
                AsyncFlatScoredObservedRunErrorV1::ObserverFailed { phase, error }
            }
            Self::Panicked { phase } => {
                AsyncFlatScoredObservedRunErrorV1::ObserverPanicked { phase }
            }
        }
    }
}

fn observe_selected_v1<O: FlatScoredTrajectoryObserverV1>(
    observer: &mut O,
    event: FlatScoredSelectedEventV1<'_>,
) -> Result<(), FlatScoredObserverInterruptionV1<O::Error>> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        observer.observe_selected_v1(event)
    })) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(FlatScoredObserverInterruptionV1::Failed {
            phase: FlatScoredObserverPhaseV1::Selected,
            error,
        }),
        Err(_) => Err(FlatScoredObserverInterruptionV1::Panicked {
            phase: FlatScoredObserverPhaseV1::Selected,
        }),
    }
}

fn observe_terminal_v1<O: FlatScoredTrajectoryObserverV1>(
    observer: &mut O,
    event: FlatScoredTerminalEventV1,
) -> Result<(), FlatScoredObserverInterruptionV1<O::Error>> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        observer.observe_terminal_v1(event)
    })) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(FlatScoredObserverInterruptionV1::Failed {
            phase: FlatScoredObserverPhaseV1::Terminal,
            error,
        }),
        Err(_) => Err(FlatScoredObserverInterruptionV1::Panicked {
            phase: FlatScoredObserverPhaseV1::Terminal,
        }),
    }
}

struct NoopFlatScoredTrajectoryObserverV1;

impl FlatScoredTrajectoryObserverV1 for NoopFlatScoredTrajectoryObserverV1 {
    type Error = std::convert::Infallible;
    type Output = ();

    const OBSERVES_TRAJECTORY: bool = false;

    fn observe_selected_v1(
        &mut self,
        event: FlatScoredSelectedEventV1<'_>,
    ) -> Result<(), Self::Error> {
        let FlatScoredSelectedEventV1 {
            expected,
            binding,
            learner_ordinal,
            action_seed,
            selected_index,
            raw_action_logits,
            predicted_value_bits,
            decision,
        } = event;
        let _ = (
            expected,
            binding,
            learner_ordinal,
            action_seed,
            selected_index,
            raw_action_logits,
            predicted_value_bits,
            decision,
        );
        Ok(())
    }

    fn observe_terminal_v1(
        &mut self,
        _event: FlatScoredTerminalEventV1,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn finish_v1(self) -> Result<Self::Output, Self::Error> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AsyncFlatScoredRolloutMetricsV1 {
    pub total_elapsed_ns: u64,
    pub complete_round_count: u64,
    pub scorer_batch_count: u64,
    pub scored_decision_count: u64,
    pub scored_action_logit_count: u64,
    pub sampled_action_count: u64,
    pub terminal_notification_count: u64,
    pub batch_width_sum: u64,
    pub max_batch_width: u32,
    pub full_target_batch_count: u64,
    pub short_batch_count: u64,
    pub broker_service_ns: u64,
    /// Schedule/topology binding over complete rounds, stable decision order,
    /// sampler seeds, and selected indices. It is not an authenticity digest.
    pub batch_membership_digest: [u8; 32],
}

impl AsyncFlatScoredRolloutMetricsV1 {
    pub fn mean_batch_width(self) -> f64 {
        if self.scorer_batch_count == 0 {
            0.0
        } else {
            self.batch_width_sum as f64 / self.scorer_batch_count as f64
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncFlatScoredRolloutResultV1 {
    pub episodes: Vec<AsyncRolloutEpisodeV1>,
    pub policy_step_count: u64,
    pub physical_decision_count: u64,
    pub metrics: AsyncFlatScoredRolloutMetricsV1,
}

impl AsyncFlatScoredRolloutResultV1 {
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
pub enum AsyncFlatScoredWorkerPhaseV1 {
    Reset,
    Encode,
    LearnerActionBinding,
    LearnerConsume,
    OpponentStep,
    Protocol,
    Panic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncFlatScoredRolloutErrorV1 {
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
        phase: AsyncFlatScoredWorkerPhaseV1,
    },
    ScorerFailed {
        batch_index: u64,
        code: u32,
    },
    ScorerPanicked {
        batch_index: u64,
    },
    ScorerOutputNonFinite {
        batch_index: u64,
        output_index: usize,
        is_value: bool,
        bits: u32,
    },
    SamplingFailed {
        logical_lane_id: usize,
        episode_id: u64,
        decision_ordinal: u64,
        error: FastCategoricalError,
    },
    SchedulerDeadlineExceeded,
    BrokerProtocolViolation,
    WorkerPanicked {
        worker_id: usize,
    },
}

impl fmt::Display for AsyncFlatScoredRolloutErrorV1 {
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
            Self::InvalidSchedulerTimeout => write!(formatter, "scheduler timeout is invalid"),
            Self::EmptyEpisodeRange => write!(formatter, "episode_count must be positive"),
            Self::EpisodeRangeOverflow => write!(formatter, "episode id range overflows u64"),
            Self::EpisodeCountExceedsAddressSpace { requested } => write!(
                formatter,
                "episode_count {requested} cannot be represented by this process"
            ),
            Self::ResultAllocationFailed { requested } => write!(
                formatter,
                "could not reserve result storage for {requested} episodes"
            ),
            Self::WorkerSpawnFailed { worker_id } => {
                write!(formatter, "failed to spawn scored rollout worker {worker_id}")
            }
            Self::WorkerFailed {
                worker_id,
                logical_lane_id,
                episode_id,
                phase,
            } => write!(
                formatter,
                "scored rollout worker {worker_id} failed on lane {logical_lane_id} in {phase:?} for episode {episode_id}"
            ),
            Self::ScorerFailed { batch_index, code } => write!(
                formatter,
                "flat scorer batch {batch_index} failed with code {code}"
            ),
            Self::ScorerPanicked { batch_index } => {
                write!(formatter, "flat scorer batch {batch_index} panicked")
            }
            Self::ScorerOutputNonFinite {
                batch_index,
                output_index,
                is_value,
                bits,
            } => write!(
                formatter,
                "flat scorer batch {batch_index} left non-finite {} output {output_index} (bits=0x{bits:08x})",
                if *is_value { "value" } else { "logit" }
            ),
            Self::SamplingFailed {
                logical_lane_id,
                episode_id,
                decision_ordinal,
                error,
            } => write!(
                formatter,
                "sampling failed on lane {logical_lane_id}, episode {episode_id}, learner decision {decision_ordinal}: {error}"
            ),
            Self::SchedulerDeadlineExceeded => {
                write!(formatter, "cooperative scored rollout deadline exceeded")
            }
            Self::BrokerProtocolViolation => write!(formatter, "scored rollout protocol violation"),
            Self::WorkerPanicked { worker_id } => {
                write!(formatter, "scored rollout worker {worker_id} panicked")
            }
        }
    }
}

impl std::error::Error for AsyncFlatScoredRolloutErrorV1 {}

#[derive(Default)]
struct OwnedFlatScoringDecisionV1 {
    decision: FlatDecisionV1,
    objects: Vec<FlatObjectCoreV1>,
    relations: Vec<FlatRelationV1>,
    object_subtypes: Vec<FlatObjectSubtypeV1>,
    ability_uses: Vec<FlatObjectAbilityUseV1>,
    goads: Vec<FlatObjectGoadV1>,
    completed_dungeons: Vec<FlatCompletedDungeonV1>,
    effect_subtype_changes: Vec<FlatEffectSubtypeChangeV1>,
    context_path_elements: Vec<FlatContextPathElementV1>,
    actions: Vec<FlatScorerActionCoreV1>,
    scorer_action_refs: Vec<FlatScorerActionRefV1>,
}

impl OwnedFlatScoringDecisionV1 {
    fn scorer_contract(&self) -> FlatScorerContractV1 {
        let binding = self.decision.binding;
        let action = binding.action_binding;
        FlatScorerContractV1 {
            scorer_packet_version: FLAT_SCORER_PACKET_VERSION_V1,
            scorer_action_ref_version: FLAT_SCORER_ACTION_REF_VERSION_V1,
            scorer_visible_manifest_version: FLAT_SCORER_VISIBLE_MANIFEST_VERSION_V1,
            scorer_visible_manifest: FLAT_SCORER_VISIBLE_MANIFEST_V1,
            typed_layout_version: binding.typed_layout_version,
            feature_inventory_version: binding.feature_inventory_version,
            enum_mapping_version: binding.enum_mapping_version,
            object_group_mapping_version: binding.object_group_mapping_version,
            relation_role_mapping_version: binding.relation_role_mapping_version,
            context_subrole_mapping_version: binding.context_subrole_mapping_version,
            action_ref_projection_role_mapping_version: binding
                .action_ref_projection_role_mapping_version,
            action_slice_version: action.slice_version,
            action_ref_role_mapping_version: action.ref_role_mapping_version,
            card_token_mapping_version: action.card_token_mapping_version,
            candidate_commitment_version: action.candidate_commitment_version,
            card_db_hash: action.card_db_hash,
            contract_digests: binding.contract_digests,
        }
    }

    fn scorer_view(&self) -> FlatScoringDecisionViewV1<'_> {
        FlatScoringDecisionViewV1::new(
            &self.decision.globals,
            active_prefix(&self.objects, self.decision.active_object_count),
            active_prefix(&self.relations, self.decision.active_relation_count),
            active_prefix(
                &self.object_subtypes,
                self.decision.active_object_subtype_count,
            ),
            active_prefix(&self.ability_uses, self.decision.active_ability_use_count),
            active_prefix(&self.goads, self.decision.active_goad_count),
            active_prefix(
                &self.completed_dungeons,
                self.decision.active_completed_dungeon_count,
            ),
            active_prefix(
                &self.effect_subtype_changes,
                self.decision.active_effect_subtype_change_count,
            ),
            active_prefix(
                &self.context_path_elements,
                self.decision.active_context_path_element_count,
            ),
            active_prefix(&self.actions, self.decision.active_action_count),
            active_prefix(
                &self.scorer_action_refs,
                self.decision.active_action_ref_count,
            ),
        )
    }
}

struct ValidatedOwnedFlatScoringDecisionV1(OwnedFlatScoringDecisionV1);

impl ValidatedOwnedFlatScoringDecisionV1 {
    fn decision(&self) -> &FlatDecisionV1 {
        &self.0.decision
    }

    fn scorer_contract(&self) -> FlatScorerContractV1 {
        self.0.scorer_contract()
    }

    fn scorer_view(&self) -> FlatScoringDecisionViewV1<'_> {
        self.0.scorer_view()
    }

    fn into_inner(self) -> OwnedFlatScoringDecisionV1 {
        self.0
    }
}

fn active_prefix<T>(buffer: &[T], count: u32) -> &[T] {
    let end = usize::try_from(count).expect("u32 active count must fit usize");
    debug_assert!(end <= buffer.len());
    &buffer[..end]
}

#[cfg(test)]
fn test_safe_packet_payload(decision: FlatScoringDecisionViewV1<'_>) -> String {
    format!(
        "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
        decision.globals(),
        decision.objects(),
        decision.relations(),
        decision.object_subtypes(),
        decision.ability_uses(),
        decision.goads(),
        decision.completed_dungeons(),
        decision.effect_subtype_changes(),
        decision.context_path_elements(),
        decision.actions(),
        decision.action_refs(),
    )
}

#[cfg(test)]
fn test_content_sensitive_logits(
    decision: FlatScoringDecisionViewV1<'_>,
    output: &mut [f32],
) -> String {
    assert_eq!(output.len(), decision.actions().len());
    let payload = test_safe_packet_payload(decision);
    let base = hash_bytes(FNV1A64_OFFSET, payload.as_bytes());
    for (action_index, logit) in output.iter_mut().enumerate() {
        let action_index = u64::try_from(action_index).expect("test action index fits u64");
        let mixed = hash_bytes(base, &action_index.to_le_bytes());
        let q8 = i32::try_from(mixed & 0x7ff).expect("masked test logit fits i32") - 1024;
        *logit = q8 as f32 / 256.0;
    }
    payload
}

#[cfg(test)]
fn test_content_sensitive_value(payload: &str) -> f32 {
    let payload_hash = hash_bytes(FNV1A64_OFFSET, payload.as_bytes());
    let value_q8 =
        i32::try_from((payload_hash >> 11) & 0x7ff).expect("masked test value fits i32") - 1024;
    value_q8 as f32 / 256.0
}

struct RoundDecisionV1 {
    worker_id: usize,
    logical_lane_id: usize,
    expected: FastActorDecisionV1,
    packet: ValidatedOwnedFlatScoringDecisionV1,
}

#[derive(Debug, Clone, Copy)]
struct RoundTerminalV1 {
    worker_id: usize,
    logical_lane_id: usize,
    terminal: AsyncRolloutTerminalV1,
    learner_action_count: u64,
    learner_trace_hash: u64,
}

struct WorkerRoundV1 {
    worker_id: usize,
    decisions: Vec<RoundDecisionV1>,
    terminals: Vec<RoundTerminalV1>,
    reply: WorkerReplyV1,
}

impl WorkerRoundV1 {
    fn with_capacity(worker_id: usize, capacity: usize) -> Self {
        Self {
            worker_id,
            decisions: Vec::with_capacity(capacity),
            terminals: Vec::with_capacity(capacity),
            reply: WorkerReplyV1 {
                actions: Vec::with_capacity(capacity),
                terminal_acks: Vec::with_capacity(capacity),
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct WorkerFailureV1 {
    worker_id: usize,
    logical_lane_id: usize,
    episode_id: u64,
    phase: AsyncFlatScoredWorkerPhaseV1,
}

enum WorkerMessageV1 {
    Round(WorkerRoundV1),
    Done { worker_id: usize },
    Failed(WorkerFailureV1),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FiniteScorerValueV1(u32);

impl FiniteScorerValueV1 {
    fn new(value: f32) -> Option<Self> {
        value.is_finite().then_some(Self(value.to_bits()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BoundScoredActionV1 {
    binding: crate::flat_policy_v1::FlatDecisionBindingV1,
    learner_ordinal: u64,
    selected_index: u32,
    predicted_value: FiniteScorerValueV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ValidatedRoundSampleV1 {
    learner_ordinal: u64,
    action_seed: u64,
    selected_index: u32,
    predicted_value: FiniteScorerValueV1,
}

struct ActionReplyV1 {
    logical_lane_id: usize,
    scored: BoundScoredActionV1,
    packet: ValidatedOwnedFlatScoringDecisionV1,
}

#[derive(Default)]
struct WorkerReplyV1 {
    actions: Vec<ActionReplyV1>,
    terminal_acks: Vec<usize>,
}

enum WorkerControlV1 {
    Continue {
        release_epoch: u64,
        round: WorkerRoundV1,
    },
    Cancel,
}

#[derive(Debug, Clone, Copy)]
struct WaitingDecisionV1 {
    expected: FastActorDecisionV1,
    binding: crate::flat_policy_v1::FlatDecisionBindingV1,
}

struct LocalLaneV1 {
    worker_id: usize,
    logical_lane_id: usize,
    next_episode_id: Option<u64>,
    episode_id: u64,
    session: Option<FastActorSessionV1>,
    response: Option<FastActorResponseV1>,
    encoder: FlatDecisionEncoderV1,
    packet: Option<OwnedFlatScoringDecisionV1>,
    waiting_decision: Option<WaitingDecisionV1>,
    waiting_terminal: bool,
    opponent_policy: SplitMix64,
    learner_action_count: u64,
    learner_trace_hash: u64,
}

impl LocalLaneV1 {
    fn vacant(
        worker_id: usize,
        logical_lane_id: usize,
        first_episode_id: u64,
        end_episode_id: u64,
    ) -> Self {
        let next_episode_id = first_episode_id
            .checked_add(logical_lane_id as u64)
            .filter(|episode_id| *episode_id < end_episode_id);
        Self {
            worker_id,
            logical_lane_id,
            next_episode_id,
            episode_id: u64::MAX,
            session: None,
            response: None,
            encoder: FlatDecisionEncoderV1::default(),
            packet: Some(OwnedFlatScoringDecisionV1::default()),
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

    fn failure(&self, phase: AsyncFlatScoredWorkerPhaseV1) -> WorkerFailureV1 {
        WorkerFailureV1 {
            worker_id: self.worker_id,
            logical_lane_id: self.logical_lane_id,
            episode_id: self.episode_id,
            phase,
        }
    }

    fn apply_reply(&mut self, reply: &mut WorkerReplyV1) -> Result<(), WorkerFailureV1> {
        if let Some(waiting) = self.waiting_decision {
            let index = reply
                .actions
                .iter()
                .position(|action| action.logical_lane_id == self.logical_lane_id)
                .ok_or_else(|| self.failure(AsyncFlatScoredWorkerPhaseV1::Protocol))?;
            let action = reply.actions.swap_remove(index);
            if action.scored.binding != waiting.binding
                || action.packet.decision().binding != waiting.binding
                || action.scored.learner_ordinal != self.learner_action_count
                || action.scored.selected_index >= waiting.expected.legal_action_count
            {
                return Err(self.failure(AsyncFlatScoredWorkerPhaseV1::LearnerActionBinding));
            }
            let missing_session = self.failure(AsyncFlatScoredWorkerPhaseV1::Protocol);
            let response = self
                .session
                .as_mut()
                .ok_or(missing_session)?
                .consume_current_flat_action_slice_v1(
                    action.scored.binding.action_binding,
                    action.scored.selected_index,
                )
                .map_err(|_| self.failure(AsyncFlatScoredWorkerPhaseV1::LearnerConsume))?;
            #[cfg(test)]
            if TEST_CAPTURE_ACTION_EVENTS_V1.load(std::sync::atomic::Ordering::SeqCst) {
                TEST_ACTION_EVENTS_V1
                    .lock()
                    .expect("test action-event sink mutex poisoned")
                    .push(TestScoredActionEventV1 {
                        expected: waiting.expected,
                        learner_ordinal: action.scored.learner_ordinal,
                        safe_packet_payload: test_safe_packet_payload(action.packet.scorer_view()),
                        scorer_value_bits: action.scored.predicted_value.0,
                        selected_index: action.scored.selected_index,
                    });
            }
            self.packet = Some(action.packet.into_inner());
            self.response = Some(response);
            self.waiting_decision = None;
            #[cfg(test)]
            TEST_CONSUMED_ACTION_COUNT_V1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.learner_action_count = self
                .learner_action_count
                .checked_add(1)
                .ok_or_else(|| self.failure(AsyncFlatScoredWorkerPhaseV1::Protocol))?;
            self.learner_trace_hash = record_trace(
                self.learner_trace_hash,
                waiting.expected,
                action.scored.selected_index,
            );
        } else if reply
            .actions
            .iter()
            .any(|action| action.logical_lane_id == self.logical_lane_id)
        {
            return Err(self.failure(AsyncFlatScoredWorkerPhaseV1::Protocol));
        }

        if self.waiting_terminal {
            let index = reply
                .terminal_acks
                .iter()
                .position(|lane| *lane == self.logical_lane_id)
                .ok_or_else(|| self.failure(AsyncFlatScoredWorkerPhaseV1::Protocol))?;
            reply.terminal_acks.swap_remove(index);
            #[cfg(test)]
            TEST_TERMINAL_ACK_COUNT_V1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.session = None;
            self.response = None;
            self.waiting_terminal = false;
            self.episode_id = u64::MAX;
        } else if reply.terminal_acks.contains(&self.logical_lane_id) {
            return Err(self.failure(AsyncFlatScoredWorkerPhaseV1::Protocol));
        }
        Ok(())
    }

    fn fill(
        &mut self,
        config: &AsyncRolloutConfigV2,
        end_episode_id: u64,
        logical_lane_count: usize,
    ) -> Result<(), WorkerFailureV1> {
        if self.is_active() {
            return Ok(());
        }
        let Some(episode_id) = self.next_episode_id else {
            return Ok(());
        };
        self.next_episode_id = episode_id
            .checked_add(logical_lane_count as u64)
            .filter(|next| *next < end_episode_id);
        let session = FastActorSessionV1::reset_with_decks_and_limits(
            episode_id,
            derive_env_seed(config.environment_seed, episode_id),
            config.max_physical_decisions,
            config.max_policy_steps,
            config.deck_ids.clone(),
        )
        .map_err(|_| {
            self.episode_id = episode_id;
            self.failure(AsyncFlatScoredWorkerPhaseV1::Reset)
        })?;
        self.response = Some(session.current_response());
        self.session = Some(session);
        self.episode_id = episode_id;
        self.waiting_decision = None;
        self.waiting_terminal = false;
        self.opponent_policy =
            SplitMix64::seed(derive_policy_seed(config.opponent_policy_seed, episode_id));
        self.learner_action_count = 0;
        self.learner_trace_hash = hash_bytes(FNV1A64_OFFSET, &episode_id.to_le_bytes());
        Ok(())
    }

    fn advance_to_event(
        &mut self,
        config: &AsyncRolloutConfigV2,
        deadline: Instant,
        cancel: &AtomicBool,
        decisions: &mut Vec<RoundDecisionV1>,
        terminals: &mut Vec<RoundTerminalV1>,
    ) -> Result<(), WorkerFailureV1> {
        if self.session.is_none() {
            return Ok(());
        }
        loop {
            if cancel.load(Ordering::Acquire) || Instant::now() >= deadline {
                return Ok(());
            }
            let response = self
                .response
                .take()
                .ok_or_else(|| self.failure(AsyncFlatScoredWorkerPhaseV1::Protocol))?;
            match response {
                FastActorResponseV1::Terminal(terminal) => {
                    terminals.push(RoundTerminalV1 {
                        worker_id: self.worker_id,
                        logical_lane_id: self.logical_lane_id,
                        terminal: compact_terminal(&terminal),
                        learner_action_count: self.learner_action_count,
                        learner_trace_hash: self.learner_trace_hash,
                    });
                    self.waiting_terminal = true;
                    return Ok(());
                }
                FastActorResponseV1::Decision(expected)
                    if expected.acting_player == config.learner_seat =>
                {
                    if expected.legal_action_count == 0
                        || usize::try_from(expected.legal_action_count)
                            .map_or(true, |width| width > FAST_CATEGORICAL_MAX_ACTIONS)
                    {
                        return Err(self.failure(AsyncFlatScoredWorkerPhaseV1::Encode));
                    }
                    let packet = self
                        .packet
                        .take()
                        .ok_or_else(|| self.failure(AsyncFlatScoredWorkerPhaseV1::Protocol))?;
                    let packet = encode_packet(
                        self.session
                            .as_ref()
                            .ok_or_else(|| self.failure(AsyncFlatScoredWorkerPhaseV1::Protocol))?,
                        expected,
                        &mut self.encoder,
                        packet,
                    )
                    .map_err(|_| self.failure(AsyncFlatScoredWorkerPhaseV1::Encode))?;
                    let binding = packet.decision().binding;
                    self.waiting_decision = Some(WaitingDecisionV1 { expected, binding });
                    decisions.push(RoundDecisionV1 {
                        worker_id: self.worker_id,
                        logical_lane_id: self.logical_lane_id,
                        expected,
                        packet,
                    });
                    return Ok(());
                }
                FastActorResponseV1::Decision(decision) => {
                    let selected_index =
                        uniform_index(&mut self.opponent_policy, decision.legal_action_count);
                    let missing_session = self.failure(AsyncFlatScoredWorkerPhaseV1::Protocol);
                    let response = self
                        .session
                        .as_mut()
                        .ok_or(missing_session)?
                        .step(decision.episode_id, decision.step, selected_index)
                        .map_err(|_| self.failure(AsyncFlatScoredWorkerPhaseV1::OpponentStep))?;
                    self.response = Some(response);
                }
            }
        }
    }
}

fn encode_packet(
    session: &FastActorSessionV1,
    expected: FastActorDecisionV1,
    encoder: &mut FlatDecisionEncoderV1,
    mut packet: OwnedFlatScoringDecisionV1,
) -> Result<ValidatedOwnedFlatScoringDecisionV1, FlatDecisionErrorV1> {
    let decision = session.encode_current_flat_scoring_decision_owned_v1(
        expected,
        encoder,
        &mut FlatScoringOwnedBuffersV1 {
            objects: &mut packet.objects,
            relations: &mut packet.relations,
            object_subtypes: &mut packet.object_subtypes,
            ability_uses: &mut packet.ability_uses,
            goads: &mut packet.goads,
            completed_dungeons: &mut packet.completed_dungeons,
            effect_subtype_changes: &mut packet.effect_subtype_changes,
            context_path_elements: &mut packet.context_path_elements,
            actions: &mut packet.actions,
            action_refs: &mut packet.scorer_action_refs,
        },
    )?;
    packet.decision = decision;
    validate_packet(&packet).map_err(|_| FlatDecisionErrorV1::InvalidReference)?;
    Ok(ValidatedOwnedFlatScoringDecisionV1(packet))
}

fn expected_scorer_contract(card_db_hash: u64) -> FlatScorerContractV1 {
    FlatScorerContractV1 {
        scorer_packet_version: FLAT_SCORER_PACKET_VERSION_V1,
        scorer_action_ref_version: FLAT_SCORER_ACTION_REF_VERSION_V1,
        scorer_visible_manifest_version: FLAT_SCORER_VISIBLE_MANIFEST_VERSION_V1,
        scorer_visible_manifest: FLAT_SCORER_VISIBLE_MANIFEST_V1,
        typed_layout_version: FLAT_POLICY_TYPED_LAYOUT_VERSION_V1,
        feature_inventory_version: FLAT_POLICY_FEATURE_INVENTORY_VERSION_V1,
        enum_mapping_version: FLAT_POLICY_ENUM_MAPPING_VERSION_V1,
        object_group_mapping_version: FLAT_POLICY_OBJECT_GROUP_MAPPING_VERSION_V1,
        relation_role_mapping_version: FLAT_POLICY_RELATION_ROLE_MAPPING_VERSION_V1,
        context_subrole_mapping_version: FLAT_POLICY_CONTEXT_SUBROLE_MAPPING_VERSION_V1,
        action_ref_projection_role_mapping_version:
            FLAT_ACTION_REF_PROJECTION_ROLE_MAPPING_VERSION_V1,
        action_slice_version: FLAT_ACTION_DECISION_SLICE_VERSION_V1,
        action_ref_role_mapping_version: FLAT_ACTION_REF_ROLE_MAPPING_VERSION_V1,
        card_token_mapping_version: FLAT_ACTION_CARD_TOKEN_MAPPING_VERSION_V1,
        candidate_commitment_version: FLAT_ACTION_CANDIDATE_COMMITMENT_VERSION_V1,
        card_db_hash,
        contract_digests: FLAT_POLICY_CONTRACT_DIGESTS_V1,
    }
}

fn checked_row_range(start: u32, count: u32, len: usize) -> Result<(usize, usize), ()> {
    let start = usize::try_from(start).map_err(|_| ())?;
    let count = usize::try_from(count).map_err(|_| ())?;
    let end = start.checked_add(count).ok_or(())?;
    if end > len {
        return Err(());
    }
    Ok((start, end))
}

fn validate_completed_dungeons(
    globals: &FlatGlobalsV1,
    rows: &[FlatCompletedDungeonV1],
) -> Result<(), ()> {
    let mut cursor = 0usize;
    for (player_index, expected_player) in [
        FlatRelativePlayerV1::SelfPlayer,
        FlatRelativePlayerV1::Opponent,
    ]
    .into_iter()
    .enumerate()
    {
        let player = globals.players[player_index];
        let (start, end) = checked_row_range(
            player.completed_dungeon_start,
            player.completed_dungeon_count,
            rows.len(),
        )?;
        if start != cursor
            || rows[start..end].iter().enumerate().any(|(order, row)| {
                row.player != expected_player || u32::try_from(order).ok() != Some(row.order)
            })
        {
            return Err(());
        }
        cursor = end;
    }
    (cursor == rows.len()).then_some(()).ok_or(())
}

fn validate_pending_effect_context_segment(
    rows: &[FlatContextPathElementV1],
    start: u32,
    count: u32,
    expected_start: usize,
    kind: FlatContextElementKindV1,
) -> Result<usize, ()> {
    let (start, end) = checked_row_range(start, count, rows.len())?;
    if start != expected_start
        || rows[start..end].iter().enumerate().any(|(order, row)| {
            row.context != FlatContextKindV1::PendingEffect
                || row.context_order != 0
                || row.kind != kind
                || u32::try_from(order).ok() != Some(row.order)
                || (kind == FlatContextElementKindV1::LegalColor && row.value > 5)
        })
    {
        return Err(());
    }
    Ok(end)
}

fn validate_pending_effect_context(
    globals: &FlatGlobalsV1,
    rows: &[FlatContextPathElementV1],
) -> Result<(), ()> {
    let Some(choice) = globals
        .engine
        .pending_effect
        .and_then(|pending| pending.choice)
    else {
        return rows.is_empty().then_some(()).ok_or(());
    };
    let (player, path_start, path_count, color_range) = match choice {
        FlatPendingEffectChoiceV1::Options {
            player,
            path_start,
            path_count,
            ..
        }
        | FlatPendingEffectChoiceV1::Targets {
            player,
            path_start,
            path_count,
            ..
        }
        | FlatPendingEffectChoiceV1::Number {
            player,
            path_start,
            path_count,
            ..
        }
        | FlatPendingEffectChoiceV1::Boolean {
            player,
            path_start,
            path_count,
            ..
        } => (player, path_start, path_count, None),
        FlatPendingEffectChoiceV1::Color {
            player,
            path_start,
            path_count,
            legal_color_start,
            legal_color_count,
        } => (
            player,
            path_start,
            path_count,
            Some((legal_color_start, legal_color_count)),
        ),
    };
    if player == FlatRelativePlayerV1::None {
        return Err(());
    }
    let path_end = validate_pending_effect_context_segment(
        rows,
        path_start,
        path_count,
        0,
        FlatContextElementKindV1::StructuralPath,
    )?;
    let final_end = if let Some((color_start, color_count)) = color_range {
        validate_pending_effect_context_segment(
            rows,
            color_start,
            color_count,
            path_end,
            FlatContextElementKindV1::LegalColor,
        )?
    } else {
        path_end
    };
    (final_end == rows.len()).then_some(()).ok_or(())
}

fn validate_effect_subtype_changes(
    objects: &[FlatObjectCoreV1],
    rows: &[FlatEffectSubtypeChangeV1],
) -> Result<(), ()> {
    let mut effect_count = 0usize;
    for object in objects
        .iter()
        .filter(|object| object.group == FlatObjectGroupV1::ContinuousEffect)
    {
        if object.source_kind != FlatObjectSourceKindV1::Effect
            || usize::try_from(object.visible_ordinal).ok() != Some(effect_count)
        {
            return Err(());
        }
        effect_count = effect_count.checked_add(1).ok_or(())?;
    }

    let mut prior_effect = None;
    let mut next_add_order = 0u32;
    let mut next_remove_order = 0u32;
    let mut saw_remove = false;
    for row in rows {
        if usize::try_from(row.effect_order).map_err(|_| ())? >= effect_count {
            return Err(());
        }
        if prior_effect != Some(row.effect_order) {
            if prior_effect.is_some_and(|prior| row.effect_order <= prior) {
                return Err(());
            }
            prior_effect = Some(row.effect_order);
            next_add_order = 0;
            next_remove_order = 0;
            saw_remove = false;
        }
        match row.kind {
            FlatEffectSubtypeChangeKindV1::Add => {
                if saw_remove || row.order != next_add_order {
                    return Err(());
                }
                next_add_order = next_add_order.checked_add(1).ok_or(())?;
            }
            FlatEffectSubtypeChangeKindV1::Remove => {
                if row.order != next_remove_order {
                    return Err(());
                }
                saw_remove = true;
                next_remove_order = next_remove_order.checked_add(1).ok_or(())?;
            }
        }
    }
    Ok(())
}

fn validate_packet(packet: &OwnedFlatScoringDecisionV1) -> Result<(), ()> {
    let decision = packet.decision;
    let action_binding = decision.binding.action_binding;
    let active_object_count = usize::try_from(decision.active_object_count).map_err(|_| ())?;
    let active_relation_count = usize::try_from(decision.active_relation_count).map_err(|_| ())?;
    let active_object_subtype_count =
        usize::try_from(decision.active_object_subtype_count).map_err(|_| ())?;
    let active_ability_use_count =
        usize::try_from(decision.active_ability_use_count).map_err(|_| ())?;
    let active_goad_count = usize::try_from(decision.active_goad_count).map_err(|_| ())?;
    let active_completed_dungeon_count =
        usize::try_from(decision.active_completed_dungeon_count).map_err(|_| ())?;
    let active_effect_subtype_change_count =
        usize::try_from(decision.active_effect_subtype_change_count).map_err(|_| ())?;
    let active_context_path_element_count =
        usize::try_from(decision.active_context_path_element_count).map_err(|_| ())?;
    let active_action_count = usize::try_from(decision.active_action_count).map_err(|_| ())?;
    let active_action_ref_count =
        usize::try_from(decision.active_action_ref_count).map_err(|_| ())?;
    if packet.scorer_contract() != expected_scorer_contract(action_binding.card_db_hash)
        || active_object_count > packet.objects.len()
        || active_relation_count > packet.relations.len()
        || active_object_subtype_count > packet.object_subtypes.len()
        || active_ability_use_count > packet.ability_uses.len()
        || active_goad_count > packet.goads.len()
        || active_completed_dungeon_count > packet.completed_dungeons.len()
        || active_effect_subtype_change_count > packet.effect_subtype_changes.len()
        || active_context_path_element_count > packet.context_path_elements.len()
        || active_action_count > packet.actions.len()
        || active_action_ref_count > packet.scorer_action_refs.len()
        || action_binding.legal_action_count != decision.active_action_count
        || active_action_count == 0
        || active_action_count > FAST_CATEGORICAL_MAX_ACTIONS
    {
        return Err(());
    }
    let objects = &packet.objects[..active_object_count];
    let relations = &packet.relations[..active_relation_count];
    let object_subtypes = &packet.object_subtypes[..active_object_subtype_count];
    let ability_uses = &packet.ability_uses[..active_ability_use_count];
    let goads = &packet.goads[..active_goad_count];
    let completed_dungeons = &packet.completed_dungeons[..active_completed_dungeon_count];
    let effect_subtype_changes =
        &packet.effect_subtype_changes[..active_effect_subtype_change_count];
    let context_path_elements = &packet.context_path_elements[..active_context_path_element_count];
    let actions = &packet.actions[..active_action_count];
    let scorer_action_refs = &packet.scorer_action_refs[..active_action_ref_count];
    validate_completed_dungeons(&decision.globals, completed_dungeons)?;
    validate_pending_effect_context(&decision.globals, context_path_elements)?;
    validate_effect_subtype_changes(objects, effect_subtype_changes)?;
    let object_count = u32::try_from(objects.len()).map_err(|_| ())?;
    if relations.iter().any(|row| {
        row.source_object.is_some_and(|index| index >= object_count)
            || row.target_object.is_some_and(|index| index >= object_count)
    }) || object_subtypes
        .iter()
        .any(|row| row.object_index >= object_count)
        || ability_uses
            .iter()
            .any(|row| row.object_index >= object_count)
        || goads.iter().any(|row| row.object_index >= object_count)
    {
        return Err(());
    }
    let mut subtype_cursor = 0usize;
    let mut ability_cursor = 0usize;
    let mut goad_cursor = 0usize;
    for (index, object) in objects.iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| ())?;
        let (subtype_start, subtype_end) = checked_row_range(
            object.subtype_start,
            object.subtype_count,
            object_subtypes.len(),
        )?;
        let (ability_start, ability_end) = checked_row_range(
            object.ability_use_start,
            object.ability_use_count,
            ability_uses.len(),
        )?;
        let (goad_start, goad_end) =
            checked_row_range(object.goad_start, object.goad_count, goads.len())?;
        if (object.subtype_count != 0 && subtype_start != subtype_cursor)
            || (object.ability_use_count != 0 && ability_start != ability_cursor)
            || (object.goad_count != 0 && goad_start != goad_cursor)
            || object_subtypes[subtype_start..subtype_end]
                .iter()
                .enumerate()
                .any(|(order, row)| {
                    row.object_index != index || u32::try_from(order).ok() != Some(row.order)
                })
            || ability_uses[ability_start..ability_end]
                .iter()
                .enumerate()
                .any(|(order, row)| {
                    row.object_index != index || u32::try_from(order).ok() != Some(row.order)
                })
            || goads[goad_start..goad_end]
                .iter()
                .enumerate()
                .any(|(order, row)| {
                    row.object_index != index || u32::try_from(order).ok() != Some(row.order)
                })
        {
            return Err(());
        }
        if object.subtype_count != 0 {
            subtype_cursor = subtype_end;
        }
        if object.ability_use_count != 0 {
            ability_cursor = ability_end;
        }
        if object.goad_count != 0 {
            goad_cursor = goad_end;
        }
    }
    if subtype_cursor != object_subtypes.len()
        || ability_cursor != ability_uses.len()
        || goad_cursor != goads.len()
    {
        return Err(());
    }
    let mut ref_cursor = 0usize;
    for (action_index, action) in actions.iter().enumerate() {
        let start = usize::try_from(action.ref_start).map_err(|_| ())?;
        let end = start.checked_add(usize::from(action.ref_len)).ok_or(())?;
        if start != ref_cursor
            || end > scorer_action_refs.len()
            || scorer_action_refs[start..end].iter().any(|reference| {
                usize::try_from(reference.action_index).ok() != Some(action_index)
                    || !FLAT_ACTION_REF_INTERNAL_TO_PROJECTION_V1
                        .contains(&reference.projection_role_id)
                    || usize::try_from(reference.model_object_index)
                        .ok()
                        .and_then(|index| objects.get(index))
                        .is_none_or(|object| object.card_token != u32::from(reference.card_token))
            })
        {
            return Err(());
        }
        ref_cursor = end;
    }
    if ref_cursor != scorer_action_refs.len() {
        return Err(());
    }
    Ok(())
}

#[derive(Clone)]
struct WorkerRuntimeV1 {
    end_episode_id: u64,
    logical_lane_count: usize,
    deadline: Instant,
    cancel: Arc<AtomicBool>,
    released_epoch: Arc<AtomicU64>,
}

fn worker_loop(
    worker_id: usize,
    config: &AsyncRolloutConfigV2,
    runtime: &WorkerRuntimeV1,
    message_tx: &SyncSender<WorkerMessageV1>,
    control_rx: &Receiver<WorkerControlV1>,
) -> Result<(), WorkerFailureV1> {
    #[cfg(test)]
    if TEST_DELAY_WORKER_ID_V1.load(std::sync::atomic::Ordering::SeqCst) == worker_id {
        thread::sleep(std::time::Duration::from_millis(
            TEST_DELAY_WORKER_MS_V1.load(std::sync::atomic::Ordering::SeqCst),
        ));
    }
    let first_lane = worker_id
        .checked_mul(config.sessions_per_worker)
        .ok_or(WorkerFailureV1 {
            worker_id,
            logical_lane_id: usize::MAX,
            episode_id: u64::MAX,
            phase: AsyncFlatScoredWorkerPhaseV1::Protocol,
        })?;
    let mut lanes: Vec<LocalLaneV1> = (0..config.sessions_per_worker)
        .map(|slot| {
            LocalLaneV1::vacant(
                worker_id,
                first_lane + slot,
                config.first_episode_id,
                runtime.end_episode_id,
            )
        })
        .collect();
    let mut round = WorkerRoundV1::with_capacity(worker_id, config.sessions_per_worker);
    loop {
        if runtime.cancel.load(Ordering::Acquire) || Instant::now() >= runtime.deadline {
            return Ok(());
        }
        for lane in &mut lanes {
            lane.apply_reply(&mut round.reply)?;
        }
        if !round.reply.actions.is_empty() || !round.reply.terminal_acks.is_empty() {
            return Err(WorkerFailureV1 {
                worker_id,
                logical_lane_id: first_lane,
                episode_id: u64::MAX,
                phase: AsyncFlatScoredWorkerPhaseV1::Protocol,
            });
        }
        if !round.decisions.is_empty() || !round.terminals.is_empty() {
            return Err(WorkerFailureV1 {
                worker_id,
                logical_lane_id: first_lane,
                episode_id: u64::MAX,
                phase: AsyncFlatScoredWorkerPhaseV1::Protocol,
            });
        }

        for lane in &mut lanes {
            lane.fill(config, runtime.end_episode_id, runtime.logical_lane_count)?;
            lane.advance_to_event(
                config,
                runtime.deadline,
                &runtime.cancel,
                &mut round.decisions,
                &mut round.terminals,
            )?;
            if runtime.cancel.load(Ordering::Acquire) || Instant::now() >= runtime.deadline {
                return Ok(());
            }
        }
        let any_active = lanes.iter().any(LocalLaneV1::is_active);
        let has_future = lanes.iter().any(LocalLaneV1::has_future_episode);
        if !any_active && !has_future {
            message_tx
                .send(WorkerMessageV1::Done { worker_id })
                .map_err(|_| WorkerFailureV1 {
                    worker_id,
                    logical_lane_id: first_lane,
                    episode_id: u64::MAX,
                    phase: AsyncFlatScoredWorkerPhaseV1::Protocol,
                })?;
            return Ok(());
        }
        if round.decisions.is_empty() && round.terminals.is_empty() {
            return Err(WorkerFailureV1 {
                worker_id,
                logical_lane_id: first_lane,
                episode_id: u64::MAX,
                phase: AsyncFlatScoredWorkerPhaseV1::Protocol,
            });
        }
        message_tx
            .send(WorkerMessageV1::Round(round))
            .map_err(|_| WorkerFailureV1 {
                worker_id,
                logical_lane_id: first_lane,
                episode_id: u64::MAX,
                phase: AsyncFlatScoredWorkerPhaseV1::Protocol,
            })?;
        #[cfg(test)]
        if TEST_EXIT_AFTER_ROUND_WORKER_ID_V1.load(std::sync::atomic::Ordering::SeqCst) == worker_id
        {
            return Ok(());
        }
        let remaining = runtime.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(());
        }
        match control_rx.recv_timeout(remaining) {
            Ok(WorkerControlV1::Continue {
                release_epoch,
                round: returned_round,
            }) => {
                while runtime.released_epoch.load(Ordering::Acquire) < release_epoch {
                    if runtime.cancel.load(Ordering::Acquire) || Instant::now() >= runtime.deadline
                    {
                        return Ok(());
                    }
                    thread::yield_now();
                }
                if returned_round.worker_id != worker_id {
                    return Err(WorkerFailureV1 {
                        worker_id,
                        logical_lane_id: first_lane,
                        episode_id: u64::MAX,
                        phase: AsyncFlatScoredWorkerPhaseV1::Protocol,
                    });
                }
                round = returned_round;
            }
            Ok(WorkerControlV1::Cancel) | Err(RecvTimeoutError::Disconnected) => return Ok(()),
            Err(RecvTimeoutError::Timeout) => return Ok(()),
        }
    }
}

fn worker_entry(
    worker_id: usize,
    config: AsyncRolloutConfigV2,
    runtime: WorkerRuntimeV1,
    message_tx: SyncSender<WorkerMessageV1>,
    control_rx: Receiver<WorkerControlV1>,
) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        worker_loop(worker_id, &config, &runtime, &message_tx, &control_rx)
    }));
    match result {
        Ok(Ok(())) => {}
        Ok(Err(failure)) => {
            let _ = message_tx.send(WorkerMessageV1::Failed(failure));
        }
        Err(_) => {
            let _ = message_tx.send(WorkerMessageV1::Failed(WorkerFailureV1 {
                worker_id,
                logical_lane_id: worker_id.saturating_mul(config.sessions_per_worker),
                episode_id: u64::MAX,
                phase: AsyncFlatScoredWorkerPhaseV1::Panic,
            }));
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BrokerEpisodeV1 {
    active: bool,
    episode_id: u64,
    learner_decision_ordinal: u64,
}

impl BrokerEpisodeV1 {
    const fn empty() -> Self {
        Self {
            active: false,
            episode_id: 0,
            learner_decision_ordinal: 0,
        }
    }

    fn bind(&mut self, episode_id: u64) -> Result<(), AsyncFlatScoredRolloutErrorV1> {
        if !self.active {
            self.active = true;
            self.episode_id = episode_id;
            self.learner_decision_ordinal = 0;
        }
        if self.episode_id != episode_id {
            return Err(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation);
        }
        Ok(())
    }

    fn sample_seed(
        &mut self,
        learner_policy_seed: u64,
        episode_id: u64,
    ) -> Result<(u64, u64), AsyncFlatScoredRolloutErrorV1> {
        self.bind(episode_id)?;
        let ordinal = self.learner_decision_ordinal;
        let seed =
            derive_async_flat_scored_action_seed_v1(learner_policy_seed, episode_id, ordinal);
        Ok((ordinal, seed))
    }

    fn commit_sample(&mut self) -> Result<(), AsyncFlatScoredRolloutErrorV1> {
        self.learner_decision_ordinal = self
            .learner_decision_ordinal
            .checked_add(1)
            .ok_or(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)?;
        Ok(())
    }

    fn finish(
        &mut self,
        terminal: AsyncRolloutTerminalV1,
        learner_action_count: u64,
        learner_trace_hash: u64,
    ) -> Result<AsyncRolloutEpisodeV1, AsyncFlatScoredRolloutErrorV1> {
        self.bind(terminal.episode_id)?;
        if learner_action_count != self.learner_decision_ordinal {
            return Err(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation);
        }
        let episode = AsyncRolloutEpisodeV1 {
            terminal,
            learner_action_count,
            learner_trace_hash,
        };
        *self = Self::empty();
        Ok(episode)
    }
}

/// Derives the scored learner action seed independently of worker, lane,
/// round, batch, chunk, and timing. `decision_ordinal` is zero based and is
/// consumed even when the legal width is one.
pub fn derive_async_flat_scored_action_seed_v1(
    learner_policy_seed: u64,
    episode_id: u64,
    decision_ordinal: u64,
) -> u64 {
    derive_policy_seed(learner_policy_seed, episode_id)
        .wrapping_add(decision_ordinal.wrapping_mul(ASYNC_FLAT_SCORED_SPLITMIX_GAMMA_V1))
}

#[derive(Clone)]
struct MembershipDigestV1 {
    states: [u64; 4],
}

impl MembershipDigestV1 {
    fn new() -> Self {
        let mut digest = Self {
            states: [
                FNV1A64_OFFSET,
                FNV1A64_OFFSET ^ 0x9e37_79b9_7f4a_7c15,
                FNV1A64_OFFSET ^ 0xd1b5_4a32_d192_ed03,
                FNV1A64_OFFSET ^ 0x94d0_49bb_1331_11eb,
            ],
        };
        digest.update(b"mtg-kernel/async-flat-scored-rollout-v1/membership/v1");
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

fn sample_validated_round_decision_v1(
    broker_episodes: &mut [BrokerEpisodeV1],
    sampler: &mut FastCategoricalScratch,
    learner_policy_seed: u64,
    decision: &RoundDecisionV1,
    action_logits: &[f32],
    predicted_value: f32,
) -> Result<ValidatedRoundSampleV1, AsyncFlatScoredRolloutErrorV1> {
    let broker_episode = broker_episodes
        .get_mut(decision.logical_lane_id)
        .ok_or(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)?;
    let (learner_ordinal, action_seed) =
        broker_episode.sample_seed(learner_policy_seed, decision.expected.episode_id)?;
    let selected_index = sampler
        .sample(action_logits, action_seed)
        .map_err(|error| AsyncFlatScoredRolloutErrorV1::SamplingFailed {
            logical_lane_id: decision.logical_lane_id,
            episode_id: decision.expected.episode_id,
            decision_ordinal: learner_ordinal,
            error,
        })?;
    broker_episode.commit_sample()?;
    Ok(ValidatedRoundSampleV1 {
        learner_ordinal,
        action_seed,
        selected_index: u32::try_from(selected_index)
            .map_err(|_| AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)?,
        predicted_value: FiniteScorerValueV1::new(predicted_value)
            .ok_or(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)?,
    })
}

fn record_validated_round_sample_v1(
    digest: &mut MembershipDigestV1,
    decision: &RoundDecisionV1,
    sample: ValidatedRoundSampleV1,
) {
    digest.update((decision.logical_lane_id as u64).to_le_bytes());
    digest.update(decision.expected.episode_id.to_le_bytes());
    digest.update(decision.expected.step.to_le_bytes());
    digest.update(sample.learner_ordinal.to_le_bytes());
    digest.update(sample.action_seed.to_le_bytes());
    digest.update(sample.selected_index.to_le_bytes());
}

fn stage_validated_action_reply_v1(
    worker_rounds: &mut [Option<WorkerRoundV1>],
    decision: RoundDecisionV1,
    sample: ValidatedRoundSampleV1,
) -> Result<(), AsyncFlatScoredRolloutErrorV1> {
    let binding = decision.packet.decision().binding;
    worker_rounds
        .get_mut(decision.worker_id)
        .and_then(Option::as_mut)
        .ok_or(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)?
        .reply
        .actions
        .push(ActionReplyV1 {
            logical_lane_id: decision.logical_lane_id,
            scored: BoundScoredActionV1 {
                binding,
                learner_ordinal: sample.learner_ordinal,
                selected_index: sample.selected_index,
                predicted_value: sample.predicted_value,
            },
            packet: decision.packet,
        });
    Ok(())
}

fn ensure_before_deadline_v1(deadline: Instant) -> Result<(), AsyncFlatScoredRolloutErrorV1> {
    if Instant::now() >= deadline {
        Err(AsyncFlatScoredRolloutErrorV1::SchedulerDeadlineExceeded)
    } else {
        Ok(())
    }
}

fn protocol_or_deadline_error_v1(deadline: Instant) -> AsyncFlatScoredRolloutErrorV1 {
    if Instant::now() >= deadline {
        AsyncFlatScoredRolloutErrorV1::SchedulerDeadlineExceeded
    } else {
        AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation
    }
}

/// Runs the finite multi-session scored rollout. The scorer remains on the
/// calling thread; worker threads own every game session, private action
/// cache, consume binding, and operational action-object buffer.
pub fn run_async_flat_scored_rollout_v1(
    config: AsyncRolloutConfigV2,
    scorer: &mut impl FlatBatchScorerV1,
) -> Result<AsyncFlatScoredRolloutResultV1, AsyncFlatScoredRolloutErrorV1> {
    match run_async_flat_scored_rollout_observed_v1(
        config,
        scorer,
        NoopFlatScoredTrajectoryObserverV1,
    ) {
        Ok((result, ())) => Ok(result),
        Err(AsyncFlatScoredObservedRunErrorV1::Rollout(error)) => Err(error),
        Err(AsyncFlatScoredObservedRunErrorV1::ObserverFailed { phase, error }) => {
            let _ = phase;
            match error {}
        }
        Err(AsyncFlatScoredObservedRunErrorV1::ObserverPanicked { phase }) => {
            let _ = phase;
            Err(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)
        }
    }
}

pub(crate) fn run_async_flat_scored_rollout_observed_v1<O: FlatScoredTrajectoryObserverV1>(
    config: AsyncRolloutConfigV2,
    scorer: &mut impl FlatBatchScorerV1,
    mut observer: O,
) -> Result<(AsyncFlatScoredRolloutResultV1, O::Output), AsyncFlatScoredObservedRunErrorV1<O::Error>>
{
    let api_started = Instant::now();
    if !(1..=ASYNC_ROLLOUT_MAX_WORKERS_V2).contains(&config.worker_count) {
        return Err(AsyncFlatScoredRolloutErrorV1::InvalidWorkerCount {
            requested: config.worker_count,
        }
        .into());
    }
    if !(1..=ASYNC_ROLLOUT_MAX_SESSIONS_PER_WORKER_V2).contains(&config.sessions_per_worker) {
        return Err(AsyncFlatScoredRolloutErrorV1::InvalidSessionsPerWorker {
            requested: config.sessions_per_worker,
        }
        .into());
    }
    let logical_lane_count = config
        .worker_count
        .checked_mul(config.sessions_per_worker)
        .ok_or(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)?;
    if !(1..=logical_lane_count).contains(&config.broker_batch_target) {
        return Err(AsyncFlatScoredRolloutErrorV1::InvalidBrokerBatchTarget {
            requested: config.broker_batch_target,
            logical_lanes: logical_lane_count,
        }
        .into());
    }
    if config.scheduler_timeout.is_zero() {
        return Err(AsyncFlatScoredRolloutErrorV1::InvalidSchedulerTimeout.into());
    }
    let deadline = api_started
        .checked_add(config.scheduler_timeout)
        .ok_or(AsyncFlatScoredRolloutErrorV1::InvalidSchedulerTimeout)?;
    if config.episode_count == 0 {
        return Err(AsyncFlatScoredRolloutErrorV1::EmptyEpisodeRange.into());
    }
    let end_episode_id = config
        .first_episode_id
        .checked_add(config.episode_count)
        .ok_or(AsyncFlatScoredRolloutErrorV1::EpisodeRangeOverflow)?;
    let episode_count_usize = usize::try_from(config.episode_count).map_err(|_| {
        AsyncFlatScoredRolloutErrorV1::EpisodeCountExceedsAddressSpace {
            requested: config.episode_count,
        }
    })?;
    let mut episodes = Vec::new();
    episodes
        .try_reserve_exact(episode_count_usize)
        .map_err(|_| AsyncFlatScoredRolloutErrorV1::ResultAllocationFailed {
            requested: config.episode_count,
        })?;

    let (message_tx, message_rx) = mpsc::sync_channel(config.worker_count);
    let cancel = Arc::new(AtomicBool::new(false));
    let released_epoch = Arc::new(AtomicU64::new(0));
    let worker_runtime = WorkerRuntimeV1 {
        end_episode_id,
        logical_lane_count,
        deadline,
        cancel: Arc::clone(&cancel),
        released_epoch: Arc::clone(&released_epoch),
    };
    let mut control_txs = Vec::with_capacity(config.worker_count);
    let mut handles: Vec<Option<JoinHandle<()>>> = Vec::with_capacity(config.worker_count);
    for worker_id in 0..config.worker_count {
        let (control_tx, control_rx) = mpsc::channel();
        let worker_message_tx = message_tx.clone();
        let worker_runtime = worker_runtime.clone();
        let worker_config = config.clone();
        let spawn = thread::Builder::new()
            .name(format!("mtg-async-flat-scored-v1-{worker_id}"))
            .spawn(move || {
                worker_entry(
                    worker_id,
                    worker_config,
                    worker_runtime,
                    worker_message_tx,
                    control_rx,
                )
            });
        match spawn {
            Ok(handle) => {
                control_txs.push(control_tx);
                handles.push(Some(handle));
            }
            Err(_) => {
                cancel.store(true, Ordering::Release);
                for control in &control_txs {
                    let _ = control.send(WorkerControlV1::Cancel);
                }
                let _ = join_every_worker(&mut handles);
                return Err(AsyncFlatScoredRolloutErrorV1::WorkerSpawnFailed { worker_id }.into());
            }
        }
    }
    drop(message_tx);

    let mut active_workers = vec![true; config.worker_count];
    let mut broker_episodes = vec![BrokerEpisodeV1::empty(); logical_lane_count];
    let mut metrics = AsyncFlatScoredRolloutMetricsV1::default();
    let mut digest = MembershipDigestV1::new();
    digest.update(config.first_episode_id.to_le_bytes());
    digest.update(config.episode_count.to_le_bytes());
    digest.update((logical_lane_count as u64).to_le_bytes());
    digest.update((config.broker_batch_target as u64).to_le_bytes());
    digest.update(config.learner_policy_seed.to_le_bytes());
    let mut sampler = FastCategoricalScratch::default();
    let mut round_logits = Vec::<f32>::new();
    let mut round_values = Vec::<f32>::new();
    let mut round_action_offsets = Vec::<usize>::new();
    let mut chunk_action_offsets = Vec::<usize>::new();
    let mut seen = vec![false; config.worker_count];
    let mut done_this_round = vec![false; config.worker_count];
    let mut round_decisions = Vec::with_capacity(logical_lane_count);
    let mut round_terminals = Vec::with_capacity(logical_lane_count);
    let mut round_samples = O::OBSERVES_TRAJECTORY.then(|| Vec::with_capacity(logical_lane_count));
    let mut worker_rounds: Vec<Option<WorkerRoundV1>> =
        (0..config.worker_count).map(|_| None).collect();
    let mut observer_interruption = None;

    let broker_result = (|| -> Result<(), AsyncFlatScoredRolloutErrorV1> {
        while active_workers.iter().any(|active| *active) {
            if Instant::now() >= deadline {
                return Err(AsyncFlatScoredRolloutErrorV1::SchedulerDeadlineExceeded);
            }
            let round_index = metrics.complete_round_count;
            checked_add(&mut metrics.complete_round_count, 1)?;
            seen.fill(false);
            done_this_round.fill(false);
            round_decisions.clear();
            round_terminals.clear();
            debug_assert!(worker_rounds.iter().all(Option::is_none));
            let active_count = active_workers.iter().filter(|active| **active).count();
            for _ in 0..active_count {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(AsyncFlatScoredRolloutErrorV1::SchedulerDeadlineExceeded);
                }
                let message = match message_rx.recv_timeout(remaining) {
                    Ok(message) => message,
                    Err(RecvTimeoutError::Timeout) => {
                        return Err(AsyncFlatScoredRolloutErrorV1::SchedulerDeadlineExceeded);
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        return Err(if Instant::now() >= deadline {
                            AsyncFlatScoredRolloutErrorV1::SchedulerDeadlineExceeded
                        } else {
                            AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation
                        });
                    }
                };
                let worker_id = match &message {
                    WorkerMessageV1::Round(round) => round.worker_id,
                    WorkerMessageV1::Done { worker_id } => *worker_id,
                    WorkerMessageV1::Failed(failure) => failure.worker_id,
                };
                if worker_id >= config.worker_count || !active_workers[worker_id] || seen[worker_id]
                {
                    return Err(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation);
                }
                seen[worker_id] = true;
                match message {
                    WorkerMessageV1::Failed(failure) => {
                        return Err(AsyncFlatScoredRolloutErrorV1::WorkerFailed {
                            worker_id: failure.worker_id,
                            logical_lane_id: failure.logical_lane_id,
                            episode_id: failure.episode_id,
                            phase: failure.phase,
                        });
                    }
                    WorkerMessageV1::Done { worker_id } => {
                        done_this_round[worker_id] = true;
                    }
                    WorkerMessageV1::Round(mut round) => {
                        if (round.decisions.is_empty() && round.terminals.is_empty())
                            || !round.reply.actions.is_empty()
                            || !round.reply.terminal_acks.is_empty()
                            || worker_rounds[worker_id].is_some()
                        {
                            return Err(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation);
                        }
                        if round.decisions.iter().any(|decision| {
                            decision.worker_id != worker_id
                                || decision.logical_lane_id >= logical_lane_count
                                || decision.logical_lane_id / config.sessions_per_worker
                                    != worker_id
                        }) || round.terminals.iter().any(|terminal| {
                            terminal.worker_id != worker_id
                                || terminal.logical_lane_id >= logical_lane_count
                                || terminal.logical_lane_id / config.sessions_per_worker
                                    != worker_id
                        }) {
                            return Err(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation);
                        }
                        round_decisions.append(&mut round.decisions);
                        round_terminals.append(&mut round.terminals);
                        worker_rounds[worker_id] = Some(round);
                    }
                }
            }
            if active_workers
                .iter()
                .enumerate()
                .any(|(worker_id, active)| *active && !seen[worker_id])
            {
                return Err(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation);
            }
            let service_started = config.measure_broker_service_time.then(Instant::now);

            round_decisions.sort_unstable_by_key(stable_decision_key);
            round_terminals.sort_unstable_by_key(|terminal| {
                (terminal.terminal.episode_id, terminal.logical_lane_id)
            });
            let round_contract = round_decisions
                .first()
                .map(|decision| decision.packet.scorer_contract());
            for decision in &round_decisions {
                if decision.packet.scorer_contract() != round_contract.expect("nonempty round")
                    || !expected_matches_binding(decision.expected, *decision.packet.decision())
                {
                    return Err(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation);
                }
            }

            round_action_offsets.clear();
            round_action_offsets.push(0);
            for decision in &round_decisions {
                let next = round_action_offsets
                    .last()
                    .copied()
                    .ok_or(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)?
                    .checked_add(
                        usize::try_from(decision.packet.decision().active_action_count)
                            .map_err(|_| AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)?,
                    )
                    .ok_or(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)?;
                round_action_offsets.push(next);
            }
            let total_actions = round_action_offsets.last().copied().unwrap_or(0);
            round_logits.clear();
            round_logits.resize(total_actions, f32::from_bits(0x7fc0_dead));
            round_values.clear();
            round_values.resize(round_decisions.len(), f32::from_bits(0x7fc0_dead));

            if let Some(contract) = round_contract {
                for (chunk_index, chunk_start) in (0..round_decisions.len())
                    .step_by(config.broker_batch_target)
                    .enumerate()
                {
                    if Instant::now() >= deadline {
                        return Err(AsyncFlatScoredRolloutErrorV1::SchedulerDeadlineExceeded);
                    }
                    let chunk_end =
                        (chunk_start + config.broker_batch_target).min(round_decisions.len());
                    let logit_start = round_action_offsets[chunk_start];
                    let logit_end = round_action_offsets[chunk_end];
                    chunk_action_offsets.clear();
                    chunk_action_offsets.extend(
                        round_action_offsets[chunk_start..=chunk_end]
                            .iter()
                            .map(|offset| offset - logit_start),
                    );
                    let batch = FlatScoringBatchViewV1 {
                        contract,
                        decisions: &round_decisions[chunk_start..chunk_end],
                        action_offsets: &chunk_action_offsets,
                    };
                    let batch_index = metrics.scorer_batch_count;
                    let score_result =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            scorer.score_batch_v1(
                                &batch,
                                &mut round_logits[logit_start..logit_end],
                                &mut round_values[chunk_start..chunk_end],
                            )
                        }));
                    match score_result {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => {
                            return Err(AsyncFlatScoredRolloutErrorV1::ScorerFailed {
                                batch_index,
                                code: error.code,
                            });
                        }
                        Err(_) => {
                            return Err(AsyncFlatScoredRolloutErrorV1::ScorerPanicked {
                                batch_index,
                            });
                        }
                    }
                    for (index, value) in round_logits[logit_start..logit_end].iter().enumerate() {
                        if !value.is_finite() {
                            return Err(AsyncFlatScoredRolloutErrorV1::ScorerOutputNonFinite {
                                batch_index,
                                output_index: index,
                                is_value: false,
                                bits: value.to_bits(),
                            });
                        }
                    }
                    for (index, value) in round_values[chunk_start..chunk_end].iter().enumerate() {
                        if !value.is_finite() {
                            return Err(AsyncFlatScoredRolloutErrorV1::ScorerOutputNonFinite {
                                batch_index,
                                output_index: index,
                                is_value: true,
                                bits: value.to_bits(),
                            });
                        }
                    }
                    let width = chunk_end - chunk_start;
                    checked_add(&mut metrics.scorer_batch_count, 1)?;
                    checked_add(
                        &mut metrics.scored_decision_count,
                        u64::try_from(width)
                            .map_err(|_| AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)?,
                    )?;
                    checked_add(
                        &mut metrics.scored_action_logit_count,
                        u64::try_from(logit_end - logit_start)
                            .map_err(|_| AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)?,
                    )?;
                    checked_add(
                        &mut metrics.batch_width_sum,
                        u64::try_from(width)
                            .map_err(|_| AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)?,
                    )?;
                    metrics.max_batch_width = metrics.max_batch_width.max(
                        u32::try_from(width)
                            .map_err(|_| AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)?,
                    );
                    if width == config.broker_batch_target {
                        checked_add(&mut metrics.full_target_batch_count, 1)?;
                    } else {
                        checked_add(&mut metrics.short_batch_count, 1)?;
                    }
                    digest.update([0x42]);
                    digest.update((chunk_index as u64).to_le_bytes());
                    digest.update((width as u64).to_le_bytes());
                }
            }
            if Instant::now() >= deadline {
                return Err(AsyncFlatScoredRolloutErrorV1::SchedulerDeadlineExceeded);
            }

            digest.update([0x52]);
            digest.update(round_index.to_le_bytes());
            digest.update((round_decisions.len() as u64).to_le_bytes());
            if O::OBSERVES_TRAJECTORY {
                let samples = round_samples
                    .as_mut()
                    .ok_or(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)?;
                samples.clear();
                for (decision_index, decision) in round_decisions.iter().enumerate() {
                    let logit_start = round_action_offsets[decision_index];
                    let logit_end = round_action_offsets[decision_index + 1];
                    let sample = sample_validated_round_decision_v1(
                        &mut broker_episodes,
                        &mut sampler,
                        config.learner_policy_seed,
                        decision,
                        &round_logits[logit_start..logit_end],
                        round_values[decision_index],
                    )?;
                    record_validated_round_sample_v1(&mut digest, decision, sample);
                    samples.push(sample);
                }
                if samples.len() != round_decisions.len() {
                    return Err(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation);
                }
                for (decision_index, (decision, sample)) in
                    round_decisions.drain(..).zip(samples.drain(..)).enumerate()
                {
                    let logit_start = round_action_offsets[decision_index];
                    let logit_end = round_action_offsets[decision_index + 1];
                    if let Err(interruption) = observe_selected_v1(
                        &mut observer,
                        FlatScoredSelectedEventV1 {
                            expected: decision.expected,
                            binding: decision.packet.decision().binding,
                            learner_ordinal: sample.learner_ordinal,
                            action_seed: sample.action_seed,
                            selected_index: sample.selected_index,
                            raw_action_logits: &round_logits[logit_start..logit_end],
                            predicted_value_bits: sample.predicted_value.0,
                            decision: decision.packet.scorer_view(),
                        },
                    ) {
                        observer_interruption = Some(interruption);
                        return Err(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation);
                    }
                    ensure_before_deadline_v1(deadline)?;
                    stage_validated_action_reply_v1(&mut worker_rounds, decision, sample)?;
                    checked_add(&mut metrics.sampled_action_count, 1)?;
                }
            } else {
                if round_samples.is_some() {
                    return Err(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation);
                }
                for (decision_index, decision) in round_decisions.drain(..).enumerate() {
                    let logit_start = round_action_offsets[decision_index];
                    let logit_end = round_action_offsets[decision_index + 1];
                    let sample = sample_validated_round_decision_v1(
                        &mut broker_episodes,
                        &mut sampler,
                        config.learner_policy_seed,
                        &decision,
                        &round_logits[logit_start..logit_end],
                        round_values[decision_index],
                    )?;
                    record_validated_round_sample_v1(&mut digest, &decision, sample);
                    stage_validated_action_reply_v1(&mut worker_rounds, decision, sample)?;
                    checked_add(&mut metrics.sampled_action_count, 1)?;
                }
            }

            digest.update((round_terminals.len() as u64).to_le_bytes());
            for terminal in round_terminals.drain(..) {
                if episodes.len() == episode_count_usize {
                    return Err(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation);
                }
                digest.update((terminal.logical_lane_id as u64).to_le_bytes());
                digest.update(terminal.terminal.episode_id.to_le_bytes());
                let episode = broker_episodes[terminal.logical_lane_id].finish(
                    terminal.terminal,
                    terminal.learner_action_count,
                    terminal.learner_trace_hash,
                )?;
                if O::OBSERVES_TRAJECTORY {
                    if let Err(interruption) = observe_terminal_v1(
                        &mut observer,
                        FlatScoredTerminalEventV1 {
                            terminal: episode.terminal,
                            learner_action_count: episode.learner_action_count,
                            learner_trace_hash: episode.learner_trace_hash,
                        },
                    ) {
                        observer_interruption = Some(interruption);
                        return Err(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation);
                    }
                    ensure_before_deadline_v1(deadline)?;
                }
                episodes.push(episode);
                worker_rounds[terminal.worker_id]
                    .as_mut()
                    .ok_or(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)?
                    .reply
                    .terminal_acks
                    .push(terminal.logical_lane_id);
                checked_add(&mut metrics.terminal_notification_count, 1)?;
            }

            // No worker receives an action until every scorer chunk, output,
            // sampler call, terminal binding, and reply has validated. Workers
            // then wait on one shared release epoch, so a later send failure
            // cannot let an earlier recipient consume a partial round.
            let release_epoch = round_index
                .checked_add(1)
                .ok_or(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)?;
            for worker_id in 0..config.worker_count {
                if !active_workers[worker_id] || done_this_round[worker_id] {
                    continue;
                }
                let round = worker_rounds[worker_id]
                    .take()
                    .ok_or(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)?;
                if round.reply.actions.is_empty() && round.reply.terminal_acks.is_empty() {
                    return Err(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation);
                }
                ensure_before_deadline_v1(deadline)?;
                control_txs[worker_id]
                    .send(WorkerControlV1::Continue {
                        release_epoch,
                        round,
                    })
                    .map_err(|_| protocol_or_deadline_error_v1(deadline))?;
                ensure_before_deadline_v1(deadline)?;
            }
            ensure_before_deadline_v1(deadline)?;
            released_epoch.store(release_epoch, Ordering::Release);
            for worker_id in 0..config.worker_count {
                if done_this_round[worker_id] {
                    active_workers[worker_id] = false;
                }
            }
            if let Some(service_started) = service_started {
                metrics.broker_service_ns = metrics.broker_service_ns.saturating_add(
                    u64::try_from(service_started.elapsed().as_nanos()).unwrap_or(u64::MAX),
                );
            }
        }
        Ok(())
    })();

    // Disconnect the request side before joining. Every worker has at most one
    // outstanding request, so this also releases any sender if the broker
    // stopped draining on an error or deadline.
    drop(message_rx);
    if broker_result.is_err() {
        cancel.store(true, Ordering::Release);
        for control in &control_txs {
            let _ = control.send(WorkerControlV1::Cancel);
        }
    }
    let join_result = join_every_worker(&mut handles);
    if let Some(interruption) = observer_interruption {
        return Err(interruption.into_run_error());
    }
    broker_result.map_err(AsyncFlatScoredObservedRunErrorV1::Rollout)?;
    join_result.map_err(AsyncFlatScoredObservedRunErrorV1::Rollout)?;
    if Instant::now() >= deadline {
        return Err(AsyncFlatScoredRolloutErrorV1::SchedulerDeadlineExceeded.into());
    }
    #[cfg(test)]
    {
        TEST_ENTERED_FINAL_REDUCTION_V1.store(true, std::sync::atomic::Ordering::SeqCst);
        thread::sleep(std::time::Duration::from_millis(
            TEST_DELAY_FINAL_REDUCTION_MS_V1.load(std::sync::atomic::Ordering::SeqCst),
        ));
    }

    episodes.sort_unstable_by_key(|episode| episode.terminal.episode_id);
    if episodes.len() != episode_count_usize
        || episodes.iter().enumerate().any(|(index, episode)| {
            episode.terminal.episode_id != config.first_episode_id + index as u64
        })
    {
        return Err(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation.into());
    }
    let policy_step_count =
        checked_episode_sum(&episodes, |episode| episode.terminal.policy_step_count)?;
    let physical_decision_count = checked_episode_sum(&episodes, |episode| {
        episode.terminal.physical_decision_count
    })?;
    let episode_learner_actions =
        checked_episode_sum(&episodes, |episode| episode.learner_action_count)?;
    if metrics.scored_decision_count != metrics.sampled_action_count
        || metrics.scored_decision_count != episode_learner_actions
        || metrics.batch_width_sum != metrics.scored_decision_count
        || metrics.terminal_notification_count != config.episode_count
        || metrics.max_batch_width as usize > config.broker_batch_target
        || metrics.scorer_batch_count
            != metrics
                .full_target_batch_count
                .checked_add(metrics.short_batch_count)
                .ok_or(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)?
    {
        return Err(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation.into());
    }
    metrics.batch_membership_digest = digest.finalize();
    if Instant::now() >= deadline {
        return Err(AsyncFlatScoredRolloutErrorV1::SchedulerDeadlineExceeded.into());
    }
    let observed_output =
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| observer.finish_v1())) {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                return Err(AsyncFlatScoredObservedRunErrorV1::ObserverFailed {
                    phase: FlatScoredObserverPhaseV1::Finish,
                    error,
                });
            }
            Err(_) => {
                return Err(AsyncFlatScoredObservedRunErrorV1::ObserverPanicked {
                    phase: FlatScoredObserverPhaseV1::Finish,
                });
            }
        };
    if Instant::now() >= deadline {
        return Err(AsyncFlatScoredRolloutErrorV1::SchedulerDeadlineExceeded.into());
    }
    metrics.total_elapsed_ns = u64::try_from(api_started.elapsed().as_nanos()).unwrap_or(u64::MAX);
    let result = AsyncFlatScoredRolloutResultV1 {
        episodes,
        policy_step_count,
        physical_decision_count,
        metrics,
    };
    Ok((result, observed_output))
}

fn join_every_worker(
    handles: &mut [Option<JoinHandle<()>>],
) -> Result<(), AsyncFlatScoredRolloutErrorV1> {
    let mut first_panicked = None;
    for (worker_id, handle) in handles.iter_mut().enumerate() {
        if handle.take().is_some_and(|handle| handle.join().is_err()) && first_panicked.is_none() {
            first_panicked = Some(worker_id);
        }
    }
    match first_panicked {
        Some(worker_id) => Err(AsyncFlatScoredRolloutErrorV1::WorkerPanicked { worker_id }),
        None => Ok(()),
    }
}

fn stable_decision_key(
    decision: &RoundDecisionV1,
) -> (u64, u64, u64, u64, u32, u32, u8, u8, u32, usize) {
    let expected = decision.expected;
    (
        expected.episode_id,
        expected.step,
        expected.environment_revision,
        expected.physical_decision_id,
        expected.substep_index,
        expected.substep_count,
        player_seat_code(expected.acting_player),
        decision_kind_code(expected.decision_kind),
        expected.legal_action_count,
        decision.logical_lane_id,
    )
}

fn expected_matches_binding(expected: FastActorDecisionV1, decision: FlatDecisionV1) -> bool {
    let binding = decision.binding.action_binding;
    binding.episode_id == expected.episode_id
        && binding.environment_revision == expected.environment_revision
        && binding.bound_policy_step_count == expected.step
        && binding.physical_decision_id == expected.physical_decision_id
        && binding.substep_index == expected.substep_index
        && binding.substep_count == expected.substep_count
        && binding.legal_action_count == expected.legal_action_count
        && binding.acting_player == player_seat_code(expected.acting_player)
        && binding.decision_kind == decision_kind_code(expected.decision_kind)
}

fn checked_add(target: &mut u64, value: u64) -> Result<(), AsyncFlatScoredRolloutErrorV1> {
    *target = target
        .checked_add(value)
        .ok_or(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)?;
    Ok(())
}

fn checked_episode_sum(
    episodes: &[AsyncRolloutEpisodeV1],
    value: impl Fn(&AsyncRolloutEpisodeV1) -> u64,
) -> Result<u64, AsyncFlatScoredRolloutErrorV1> {
    episodes.iter().try_fold(0u64, |total, episode| {
        total
            .checked_add(value(episode))
            .ok_or(AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation)
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

fn player_seat_code(seat: PlayerSeatV1) -> u8 {
    match seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fast_sampler::splitmix64_first;
    use sha2::{Digest, Sha256};
    use std::collections::BTreeSet;
    use std::sync::Mutex;
    use std::time::Duration;

    static TEST_LOCK: Mutex<()> = Mutex::new(());
    const TEST_LEARNER_POLICY_SEED: u64 = 83_501;

    #[derive(Default)]
    struct ZeroScorer {
        calls: u64,
        decisions: u64,
        logits: u64,
    }

    impl FlatBatchScorerV1 for ZeroScorer {
        fn score_batch_v1(
            &mut self,
            batch: &FlatScoringBatchViewV1<'_>,
            action_logits: &mut [f32],
            values: &mut [f32],
        ) -> Result<(), FlatBatchScorerErrorV1> {
            assert_eq!(values.len(), batch.decision_count());
            assert_eq!(action_logits.len(), batch.total_action_count());
            assert_eq!(batch.action_offsets().first(), Some(&0));
            assert_eq!(batch.action_offsets().last(), Some(&action_logits.len()));
            for decision_index in 0..batch.decision_count() {
                let decision = batch.decision(decision_index).unwrap();
                let start = batch.action_offsets()[decision_index];
                let end = batch.action_offsets()[decision_index + 1];
                assert_eq!(end - start, decision.actions().len());
                for reference in decision.action_refs() {
                    let object = &decision.objects()[reference.model_object_index as usize];
                    assert_eq!(object.card_token, u32::from(reference.card_token));
                }
            }
            action_logits.fill(0.0);
            values.fill(0.0);
            self.calls += 1;
            self.decisions += batch.decision_count() as u64;
            self.logits += action_logits.len() as u64;
            Ok(())
        }
    }

    #[derive(Default)]
    struct ContentSensitiveScorer {
        distinct_payloads: BTreeSet<String>,
        saw_nonuniform_multi_action: bool,
    }

    impl FlatBatchScorerV1 for ContentSensitiveScorer {
        fn score_batch_v1(
            &mut self,
            batch: &FlatScoringBatchViewV1<'_>,
            action_logits: &mut [f32],
            values: &mut [f32],
        ) -> Result<(), FlatBatchScorerErrorV1> {
            assert_eq!(values.len(), batch.decision_count());
            assert_eq!(action_logits.len(), batch.total_action_count());
            for (decision_index, value) in values.iter_mut().enumerate() {
                let decision = batch.decision(decision_index).unwrap();
                let start = batch.action_offsets()[decision_index];
                let end = batch.action_offsets()[decision_index + 1];
                assert_eq!(end - start, decision.actions().len());
                let payload =
                    test_content_sensitive_logits(decision, &mut action_logits[start..end]);
                *value = test_content_sensitive_value(&payload);
                self.saw_nonuniform_multi_action |= action_logits[start..end]
                    .first()
                    .is_some_and(|first| action_logits[start + 1..end].iter().any(|x| x != first));
                self.distinct_payloads.insert(payload);
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct ValueSwappingScorer {
        inner: ContentSensitiveScorer,
        swapped: bool,
    }

    impl FlatBatchScorerV1 for ValueSwappingScorer {
        fn score_batch_v1(
            &mut self,
            batch: &FlatScoringBatchViewV1<'_>,
            action_logits: &mut [f32],
            values: &mut [f32],
        ) -> Result<(), FlatBatchScorerErrorV1> {
            self.inner.score_batch_v1(batch, action_logits, values)?;
            if !self.swapped {
                'pairs: for left in 0..values.len() {
                    for right in left + 1..values.len() {
                        if values[left].to_bits() != values[right].to_bits() {
                            values.swap(left, right);
                            self.swapped = true;
                            break 'pairs;
                        }
                    }
                }
            }
            Ok(())
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TestObservedSelectedEventV1 {
        expected: FastActorDecisionV1,
        binding: FlatDecisionBindingV1,
        learner_ordinal: u64,
        action_seed: u64,
        selected_index: u32,
        raw_action_logit_bits: Vec<u32>,
        predicted_value_bits: u32,
        safe_packet_payload: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum TestObservedTrajectoryEventV1 {
        Selected(Box<TestObservedSelectedEventV1>),
        Terminal(FlatScoredTerminalEventV1),
    }

    impl TestObservedTrajectoryEventV1 {
        fn canonical_key(&self) -> (u64, u8, u64) {
            match self {
                Self::Selected(event) => (event.expected.episode_id, 0, event.learner_ordinal),
                Self::Terminal(event) => (event.terminal.episode_id, 1, event.learner_action_count),
            }
        }
    }

    #[derive(Default)]
    struct CanonicalTestTrajectoryObserverV1 {
        staged: Vec<TestObservedTrajectoryEventV1>,
    }

    impl FlatScoredTrajectoryObserverV1 for CanonicalTestTrajectoryObserverV1 {
        type Error = std::convert::Infallible;
        type Output = Vec<TestObservedTrajectoryEventV1>;

        fn observe_selected_v1(
            &mut self,
            event: FlatScoredSelectedEventV1<'_>,
        ) -> Result<(), Self::Error> {
            assert!(expected_matches_binding(
                event.expected,
                FlatDecisionV1 {
                    binding: event.binding,
                    ..FlatDecisionV1::default()
                }
            ));
            assert_eq!(
                event.action_seed,
                derive_async_flat_scored_action_seed_v1(
                    TEST_LEARNER_POLICY_SEED,
                    event.expected.episode_id,
                    event.learner_ordinal,
                )
            );
            assert!(event.selected_index < event.expected.legal_action_count);
            assert_eq!(
                event.raw_action_logits.len(),
                usize::try_from(event.expected.legal_action_count).unwrap()
            );
            let mut expected_logits = vec![0.0; event.raw_action_logits.len()];
            let safe_packet_payload =
                test_content_sensitive_logits(event.decision, &mut expected_logits);
            let raw_action_logit_bits = event
                .raw_action_logits
                .iter()
                .copied()
                .map(f32::to_bits)
                .collect::<Vec<_>>();
            assert_eq!(
                raw_action_logit_bits,
                expected_logits
                    .iter()
                    .copied()
                    .map(f32::to_bits)
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                event.predicted_value_bits,
                test_content_sensitive_value(&safe_packet_payload).to_bits()
            );
            self.staged
                .push(TestObservedTrajectoryEventV1::Selected(Box::new(
                    TestObservedSelectedEventV1 {
                        expected: event.expected,
                        binding: event.binding,
                        learner_ordinal: event.learner_ordinal,
                        action_seed: event.action_seed,
                        selected_index: event.selected_index,
                        raw_action_logit_bits,
                        predicted_value_bits: event.predicted_value_bits,
                        safe_packet_payload,
                    },
                )));
            Ok(())
        }

        fn observe_terminal_v1(
            &mut self,
            event: FlatScoredTerminalEventV1,
        ) -> Result<(), Self::Error> {
            self.staged
                .push(TestObservedTrajectoryEventV1::Terminal(event));
            Ok(())
        }

        fn finish_v1(mut self) -> Result<Self::Output, Self::Error> {
            self.staged
                .sort_unstable_by_key(TestObservedTrajectoryEventV1::canonical_key);
            Ok(self.staged)
        }
    }

    struct FailingFirstSelectedObserverV1 {
        callback_count: Arc<AtomicU64>,
        finished: Arc<AtomicBool>,
    }

    impl FlatScoredTrajectoryObserverV1 for FailingFirstSelectedObserverV1 {
        type Error = u32;
        type Output = ();

        fn observe_selected_v1(
            &mut self,
            _event: FlatScoredSelectedEventV1<'_>,
        ) -> Result<(), Self::Error> {
            self.callback_count.fetch_add(1, Ordering::SeqCst);
            Err(91)
        }

        fn observe_terminal_v1(
            &mut self,
            _event: FlatScoredTerminalEventV1,
        ) -> Result<(), Self::Error> {
            panic!("terminal must not precede the first selected event")
        }

        fn finish_v1(self) -> Result<Self::Output, Self::Error> {
            self.finished.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    struct PanickingFirstSelectedObserverV1 {
        callback_count: Arc<AtomicU64>,
        finished: Arc<AtomicBool>,
    }

    impl FlatScoredTrajectoryObserverV1 for PanickingFirstSelectedObserverV1 {
        type Error = std::convert::Infallible;
        type Output = ();

        fn observe_selected_v1(
            &mut self,
            _event: FlatScoredSelectedEventV1<'_>,
        ) -> Result<(), Self::Error> {
            self.callback_count.fetch_add(1, Ordering::SeqCst);
            panic!("intentional trajectory observer panic")
        }

        fn observe_terminal_v1(
            &mut self,
            _event: FlatScoredTerminalEventV1,
        ) -> Result<(), Self::Error> {
            panic!("terminal must not precede the first selected event")
        }

        fn finish_v1(self) -> Result<Self::Output, Self::Error> {
            self.finished.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    struct SlowFirstSelectedObserverV1 {
        callback_count: Arc<AtomicU64>,
        finished: Arc<AtomicBool>,
        delay: Duration,
    }

    impl FlatScoredTrajectoryObserverV1 for SlowFirstSelectedObserverV1 {
        type Error = std::convert::Infallible;
        type Output = ();

        fn observe_selected_v1(
            &mut self,
            _event: FlatScoredSelectedEventV1<'_>,
        ) -> Result<(), Self::Error> {
            self.callback_count.fetch_add(1, Ordering::SeqCst);
            thread::sleep(self.delay);
            Ok(())
        }

        fn observe_terminal_v1(
            &mut self,
            _event: FlatScoredTerminalEventV1,
        ) -> Result<(), Self::Error> {
            panic!("terminal must not precede the first selected event")
        }

        fn finish_v1(self) -> Result<Self::Output, Self::Error> {
            self.finished.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    struct ErroringTerminalObserverV1;

    impl FlatScoredTrajectoryObserverV1 for ErroringTerminalObserverV1 {
        type Error = u32;
        type Output = ();

        fn observe_selected_v1(
            &mut self,
            _event: FlatScoredSelectedEventV1<'_>,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        fn observe_terminal_v1(
            &mut self,
            _event: FlatScoredTerminalEventV1,
        ) -> Result<(), Self::Error> {
            Err(92)
        }

        fn finish_v1(self) -> Result<Self::Output, Self::Error> {
            Ok(())
        }
    }

    struct PanickingTerminalObserverV1;

    impl FlatScoredTrajectoryObserverV1 for PanickingTerminalObserverV1 {
        type Error = std::convert::Infallible;
        type Output = ();

        fn observe_selected_v1(
            &mut self,
            _event: FlatScoredSelectedEventV1<'_>,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        fn observe_terminal_v1(
            &mut self,
            _event: FlatScoredTerminalEventV1,
        ) -> Result<(), Self::Error> {
            panic!("intentional terminal observer panic")
        }

        fn finish_v1(self) -> Result<Self::Output, Self::Error> {
            Ok(())
        }
    }

    struct ErroringFinishObserverV1;

    impl FlatScoredTrajectoryObserverV1 for ErroringFinishObserverV1 {
        type Error = u32;
        type Output = ();

        fn observe_selected_v1(
            &mut self,
            _event: FlatScoredSelectedEventV1<'_>,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        fn observe_terminal_v1(
            &mut self,
            _event: FlatScoredTerminalEventV1,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        fn finish_v1(self) -> Result<Self::Output, Self::Error> {
            Err(93)
        }
    }

    struct PanickingFinishObserverV1;

    impl FlatScoredTrajectoryObserverV1 for PanickingFinishObserverV1 {
        type Error = std::convert::Infallible;
        type Output = ();

        fn observe_selected_v1(
            &mut self,
            _event: FlatScoredSelectedEventV1<'_>,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        fn observe_terminal_v1(
            &mut self,
            _event: FlatScoredTerminalEventV1,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        fn finish_v1(self) -> Result<Self::Output, Self::Error> {
            panic!("intentional trajectory observer finish panic")
        }
    }

    fn capture_async_action_events<T>(
        run: impl FnOnce() -> T,
    ) -> (T, Vec<TestScoredActionEventV1>) {
        TEST_ACTION_EVENTS_V1
            .lock()
            .expect("test action-event sink mutex poisoned")
            .clear();
        TEST_CAPTURE_ACTION_EVENTS_V1.store(true, std::sync::atomic::Ordering::SeqCst);
        let result = run();
        TEST_CAPTURE_ACTION_EVENTS_V1.store(false, std::sync::atomic::Ordering::SeqCst);
        let mut events = std::mem::take(
            &mut *TEST_ACTION_EVENTS_V1
                .lock()
                .expect("test action-event sink mutex poisoned"),
        );
        events.sort_unstable_by_key(|event| (event.expected.episode_id, event.learner_ordinal));
        (result, events)
    }

    fn first_action_event_mismatch(
        expected: &[TestScoredActionEventV1],
        actual: &[TestScoredActionEventV1],
    ) -> Option<usize> {
        expected
            .iter()
            .zip(actual)
            .position(|(expected, actual)| expected != actual)
            .or_else(|| (expected.len() != actual.len()).then(|| expected.len().min(actual.len())))
    }

    struct SynchronousReferenceV1 {
        episodes: Vec<AsyncRolloutEpisodeV1>,
        events: Vec<TestScoredActionEventV1>,
        policy_step_count: u64,
        physical_decision_count: u64,
        scored_action_logit_count: u64,
    }

    fn synchronous_content_sensitive_reference(
        config: &AsyncRolloutConfigV2,
    ) -> SynchronousReferenceV1 {
        let end_episode_id = config
            .first_episode_id
            .checked_add(config.episode_count)
            .unwrap();
        let mut episodes = Vec::new();
        let mut events = Vec::new();
        let mut encoder = FlatDecisionEncoderV1::default();
        let mut packet = OwnedFlatScoringDecisionV1::default();
        let mut sampler = FastCategoricalScratch::default();
        let mut logits = Vec::new();
        let mut scored_action_logit_count = 0u64;

        for episode_id in config.first_episode_id..end_episode_id {
            let mut session = FastActorSessionV1::reset_with_decks_and_limits(
                episode_id,
                derive_env_seed(config.environment_seed, episode_id),
                config.max_physical_decisions,
                config.max_policy_steps,
                config.deck_ids.clone(),
            )
            .unwrap();
            let mut response = session.current_response();
            let mut opponent_policy =
                SplitMix64::seed(derive_policy_seed(config.opponent_policy_seed, episode_id));
            let mut learner_ordinal = 0u64;
            let mut learner_trace_hash = hash_bytes(FNV1A64_OFFSET, &episode_id.to_le_bytes());

            loop {
                match response {
                    FastActorResponseV1::Terminal(terminal) => {
                        episodes.push(AsyncRolloutEpisodeV1 {
                            terminal: compact_terminal(&terminal),
                            learner_action_count: learner_ordinal,
                            learner_trace_hash,
                        });
                        break;
                    }
                    FastActorResponseV1::Decision(expected)
                        if expected.acting_player == config.learner_seat =>
                    {
                        let validated_packet =
                            encode_packet(&session, expected, &mut encoder, packet).unwrap();
                        let width = usize::try_from(expected.legal_action_count).unwrap();
                        logits.resize(width, 0.0);
                        let payload = test_content_sensitive_logits(
                            validated_packet.scorer_view(),
                            &mut logits,
                        );
                        let action_seed = derive_async_flat_scored_action_seed_v1(
                            config.learner_policy_seed,
                            episode_id,
                            learner_ordinal,
                        );
                        let selected_index =
                            u32::try_from(sampler.sample(&logits, action_seed).unwrap()).unwrap();
                        events.push(TestScoredActionEventV1 {
                            expected,
                            learner_ordinal,
                            scorer_value_bits: test_content_sensitive_value(&payload).to_bits(),
                            safe_packet_payload: payload,
                            selected_index,
                        });
                        learner_trace_hash =
                            record_trace(learner_trace_hash, expected, selected_index);
                        learner_ordinal = learner_ordinal.checked_add(1).unwrap();
                        scored_action_logit_count = scored_action_logit_count
                            .checked_add(u64::from(expected.legal_action_count))
                            .unwrap();
                        response = session
                            .consume_current_flat_action_slice_v1(
                                validated_packet.decision().binding.action_binding,
                                selected_index,
                            )
                            .unwrap();
                        packet = validated_packet.into_inner();
                    }
                    FastActorResponseV1::Decision(decision) => {
                        let selected_index =
                            uniform_index(&mut opponent_policy, decision.legal_action_count);
                        response = session
                            .step(decision.episode_id, decision.step, selected_index)
                            .unwrap();
                    }
                }
            }
        }
        let policy_step_count = episodes
            .iter()
            .map(|episode| episode.terminal.policy_step_count)
            .sum();
        let physical_decision_count = episodes
            .iter()
            .map(|episode| episode.terminal.physical_decision_count)
            .sum();
        SynchronousReferenceV1 {
            episodes,
            events,
            policy_step_count,
            physical_decision_count,
            scored_action_logit_count,
        }
    }

    fn config(
        worker_count: usize,
        sessions_per_worker: usize,
        broker_batch_target: usize,
        episode_count: u64,
    ) -> AsyncRolloutConfigV2 {
        AsyncRolloutConfigV2 {
            deck_ids: ["Rally".to_string(), "Rally".to_string()],
            learner_seat: PlayerSeatV1::P0,
            environment_seed: 81_501,
            opponent_policy_seed: 82_501,
            learner_policy_seed: TEST_LEARNER_POLICY_SEED,
            max_physical_decisions: 5_000,
            max_policy_steps: 640_000,
            worker_count,
            sessions_per_worker,
            broker_batch_target,
            first_episode_id: 0,
            episode_count,
            scheduler_timeout: Duration::from_secs(60),
            measure_broker_service_time: false,
        }
    }

    fn minimal_valid_packet() -> OwnedFlatScoringDecisionV1 {
        let card_db_hash = 0x5ca1_ab1e_u64;
        let contract = expected_scorer_contract(card_db_hash);
        let action_binding = crate::rl_session::FlatActionDecisionBindingV1 {
            slice_version: contract.action_slice_version,
            ref_role_mapping_version: contract.action_ref_role_mapping_version,
            card_token_mapping_version: contract.card_token_mapping_version,
            candidate_commitment_version: contract.candidate_commitment_version,
            card_db_hash,
            legal_action_count: 1,
            ..Default::default()
        };
        OwnedFlatScoringDecisionV1 {
            decision: FlatDecisionV1 {
                binding: crate::flat_policy_v1::FlatDecisionBindingV1 {
                    action_binding,
                    typed_layout_version: contract.typed_layout_version,
                    feature_inventory_version: contract.feature_inventory_version,
                    enum_mapping_version: contract.enum_mapping_version,
                    object_group_mapping_version: contract.object_group_mapping_version,
                    relation_role_mapping_version: contract.relation_role_mapping_version,
                    context_subrole_mapping_version: contract.context_subrole_mapping_version,
                    action_ref_projection_role_mapping_version: contract
                        .action_ref_projection_role_mapping_version,
                    contract_digests: contract.contract_digests,
                },
                active_object_count: 1,
                active_action_count: 1,
                active_action_ref_count: 1,
                ..Default::default()
            },
            objects: vec![FlatObjectCoreV1 {
                card_token: 1,
                ..Default::default()
            }],
            actions: vec![FlatScorerActionCoreV1 {
                ref_start: 0,
                ref_len: 1,
                ..Default::default()
            }],
            scorer_action_refs: vec![FlatScorerActionRefV1 {
                action_index: 0,
                projection_role_id: 0,
                card_token: 1,
                model_object_index: 0,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn scorer_manifest_and_nested_packet_validation_are_fail_closed() {
        assert_eq!(
            FLAT_SCORER_VISIBLE_MANIFEST_V1
                .split(',')
                .collect::<Vec<_>>(),
            [
                "globals",
                "objects",
                "relations",
                "object_subtypes",
                "ability_uses",
                "goads",
                "completed_dungeons",
                "effect_subtype_changes",
                "context_path_elements",
                "actions",
                "action_refs",
            ]
        );
        let packet = minimal_valid_packet();
        assert_eq!(
            packet.scorer_contract().scorer_visible_manifest,
            FLAT_SCORER_VISIBLE_MANIFEST_V1
        );
        validate_packet(&packet).unwrap();

        let mut bad_role = minimal_valid_packet();
        bad_role.scorer_action_refs[0].projection_role_id = 7;
        assert!(validate_packet(&bad_role).is_err());
        bad_role.scorer_action_refs[0].projection_role_id = u8::MAX;
        assert!(validate_packet(&bad_role).is_err());

        let mut child = minimal_valid_packet();
        child.objects[0].subtype_count = 1;
        child.object_subtypes.push(FlatObjectSubtypeV1 {
            object_index: 0,
            order: 0,
            subtype_id: 7,
        });
        child.decision.active_object_subtype_count = 1;
        validate_packet(&child).unwrap();
        child.object_subtypes[0].order = 1;
        assert!(validate_packet(&child).is_err());
        child.object_subtypes[0].order = 0;
        child.objects[0].subtype_count = 0;
        assert!(validate_packet(&child).is_err());

        let mut dungeons = minimal_valid_packet();
        dungeons.decision.globals.players[0].completed_dungeon_count = 1;
        dungeons.decision.globals.players[1].completed_dungeon_start = 1;
        dungeons.decision.globals.players[1].completed_dungeon_count = 1;
        dungeons.completed_dungeons = vec![
            FlatCompletedDungeonV1 {
                player: FlatRelativePlayerV1::SelfPlayer,
                order: 0,
                dungeon_id: 1,
            },
            FlatCompletedDungeonV1 {
                player: FlatRelativePlayerV1::Opponent,
                order: 0,
                dungeon_id: 2,
            },
        ];
        dungeons.decision.active_completed_dungeon_count = 2;
        validate_packet(&dungeons).unwrap();
        dungeons.completed_dungeons[1].player = FlatRelativePlayerV1::SelfPlayer;
        assert!(validate_packet(&dungeons).is_err());
        dungeons.completed_dungeons[1].player = FlatRelativePlayerV1::Opponent;
        dungeons.decision.globals.players[1].completed_dungeon_start = 0;
        assert!(validate_packet(&dungeons).is_err());

        let mut context = minimal_valid_packet();
        context.decision.globals.engine.pending_effect =
            Some(crate::flat_policy_v1::FlatPendingEffectGlobalsV1 {
                choice: Some(FlatPendingEffectChoiceV1::Color {
                    player: FlatRelativePlayerV1::SelfPlayer,
                    path_start: 0,
                    path_count: 1,
                    legal_color_start: 1,
                    legal_color_count: 2,
                }),
                ..Default::default()
            });
        context.context_path_elements = vec![
            FlatContextPathElementV1 {
                context: FlatContextKindV1::PendingEffect,
                context_order: 0,
                kind: FlatContextElementKindV1::StructuralPath,
                order: 0,
                value: 9,
            },
            FlatContextPathElementV1 {
                context: FlatContextKindV1::PendingEffect,
                context_order: 0,
                kind: FlatContextElementKindV1::LegalColor,
                order: 0,
                value: 1,
            },
            FlatContextPathElementV1 {
                context: FlatContextKindV1::PendingEffect,
                context_order: 0,
                kind: FlatContextElementKindV1::LegalColor,
                order: 1,
                value: 5,
            },
        ];
        context.decision.active_context_path_element_count = 3;
        validate_packet(&context).unwrap();
        context.context_path_elements[2].value = 6;
        assert!(validate_packet(&context).is_err());
        context.context_path_elements[2].value = 5;
        context.context_path_elements[2].order = 0;
        assert!(validate_packet(&context).is_err());

        let mut effects = minimal_valid_packet();
        effects.objects.push(FlatObjectCoreV1 {
            group: FlatObjectGroupV1::ContinuousEffect,
            source_kind: FlatObjectSourceKindV1::Effect,
            visible_ordinal: 0,
            ..Default::default()
        });
        effects.decision.active_object_count = 2;
        effects.effect_subtype_changes = vec![
            FlatEffectSubtypeChangeV1 {
                effect_order: 0,
                kind: FlatEffectSubtypeChangeKindV1::Add,
                order: 0,
                subtype_id: 3,
            },
            FlatEffectSubtypeChangeV1 {
                effect_order: 0,
                kind: FlatEffectSubtypeChangeKindV1::Remove,
                order: 0,
                subtype_id: 4,
            },
        ];
        effects.decision.active_effect_subtype_change_count = 2;
        validate_packet(&effects).unwrap();
        effects.effect_subtype_changes.swap(0, 1);
        assert!(validate_packet(&effects).is_err());
        effects.effect_subtype_changes.swap(0, 1);
        effects.objects[1].visible_ordinal = 1;
        assert!(validate_packet(&effects).is_err());
    }

    #[test]
    fn scored_action_seed_and_sampler_vectors_are_exact() {
        let vectors = [
            (73_501, 0, 0, 0xa96f_fdca_56cf_c747, 0x5af1_3aee_af71_1a0e),
            (73_501, 0, 1, 0x47a7_7783_d61a_435c, 0xfd95_5c18_0fe0_f81b),
            (73_501, 0, 2, 0xe5de_f13d_5564_bf71, 0xb947_44e7_cf64_3f46),
            (
                73_501,
                u64::MAX,
                3,
                0xd60b_95d7_e5b0_f552,
                0x35db_b796_86e4_8b08,
            ),
            (
                73_501,
                7,
                u64::MAX,
                0x1082_4916_bfdf_0a81,
                0x14e1_6127_1043_3f2e,
            ),
            (
                u64::MAX,
                u64::MAX,
                u64::MAX,
                0xb88c_39e6_8c06_6acc,
                0xa6ca_9e95_ff29_277e,
            ),
        ];
        for (base, episode, ordinal, expected_seed, expected_draw) in vectors {
            let seed = derive_async_flat_scored_action_seed_v1(base, episode, ordinal);
            assert_eq!(seed, expected_seed);
            assert_eq!(splitmix64_first(seed), expected_draw);
        }
        assert_eq!(derive_policy_seed(73_501, 0), 0xa96f_fdca_56cf_c747);
        assert_eq!(derive_policy_seed(73_501, u64::MAX), 0xfb65_28ab_67d1_8113);
        assert_eq!(derive_policy_seed(73_501, 7), 0xaeb9_c2d0_3f29_8696);
        assert_eq!(
            derive_policy_seed(u64::MAX, u64::MAX),
            0x56c3_b3a0_0b50_e6e1
        );

        let mut sampler = FastCategoricalScratch::default();
        assert_eq!(
            sampler
                .sample(
                    &[0.0],
                    derive_async_flat_scored_action_seed_v1(73_501, 0, 0)
                )
                .unwrap(),
            0
        );
        assert_eq!(
            sampler
                .sample(
                    &[0.0, 0.0],
                    derive_async_flat_scored_action_seed_v1(73_501, 0, 1),
                )
                .unwrap(),
            1
        );
        assert_eq!(
            sampler
                .sample(
                    &[0.0, 1.0, 2.0],
                    derive_async_flat_scored_action_seed_v1(73_501, 0, 2),
                )
                .unwrap(),
            2
        );
        let pattern = |width: usize| {
            (0..width)
                .map(|index| -(((index * 37) % 4097) as f32 / 256.0))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            sampler
                .sample(
                    &pattern(13),
                    derive_async_flat_scored_action_seed_v1(73_501, u64::MAX, 3),
                )
                .unwrap(),
            1
        );
        assert_eq!(
            sampler
                .sample(
                    &pattern(64),
                    derive_async_flat_scored_action_seed_v1(73_501, 7, u64::MAX),
                )
                .unwrap(),
            0
        );
        assert_eq!(
            sampler
                .sample(
                    &[-1.0, 1.0],
                    derive_async_flat_scored_action_seed_v1(u64::MAX, u64::MAX, u64::MAX,),
                )
                .unwrap(),
            1
        );
    }

    #[test]
    fn scored_rollout_is_natural_exact_and_repeatable() {
        let _lock = TEST_LOCK.lock().unwrap();
        let mut first_scorer = ZeroScorer::default();
        let first =
            run_async_flat_scored_rollout_v1(config(2, 2, 4, 16), &mut first_scorer).unwrap();
        assert!(first.all_natural());
        assert_eq!(first.episodes.len(), 16);
        assert_eq!(
            first.metrics.scored_decision_count,
            first.metrics.sampled_action_count
        );
        assert_eq!(first_scorer.decisions, first.metrics.scored_decision_count);
        assert_eq!(first_scorer.logits, first.metrics.scored_action_logit_count);
        assert_eq!(first_scorer.calls, first.metrics.scorer_batch_count);
        assert_ne!(first.metrics.batch_membership_digest, [0; 32]);

        let mut second_scorer = ZeroScorer::default();
        let second =
            run_async_flat_scored_rollout_v1(config(2, 2, 4, 16), &mut second_scorer).unwrap();
        assert_eq!(second.episodes, first.episodes);
        assert_eq!(second.policy_step_count, first.policy_step_count);
        assert_eq!(
            second.physical_decision_count,
            first.physical_decision_count
        );
        assert_eq!(
            second.metrics.batch_membership_digest,
            first.metrics.batch_membership_digest
        );
        assert_eq!(
            (
                second.metrics.complete_round_count,
                second.metrics.scorer_batch_count,
                second.metrics.scored_decision_count,
                second.metrics.scored_action_logit_count,
                second.metrics.max_batch_width,
            ),
            (
                first.metrics.complete_round_count,
                first.metrics.scorer_batch_count,
                first.metrics.scored_decision_count,
                first.metrics.scored_action_logit_count,
                first.metrics.max_batch_width,
            )
        );
    }

    #[test]
    fn public_noop_path_matches_private_noop_core() {
        let _lock = TEST_LOCK.lock().unwrap();
        let shaped = config(2, 2, 3, 8);
        let mut public_scorer = ZeroScorer::default();
        let public = run_async_flat_scored_rollout_v1(shaped.clone(), &mut public_scorer).unwrap();
        let mut observed_scorer = ZeroScorer::default();
        let (observed, output) = run_async_flat_scored_rollout_observed_v1(
            shaped,
            &mut observed_scorer,
            NoopFlatScoredTrajectoryObserverV1,
        )
        .unwrap();

        assert_eq!(output, ());
        assert_eq!(observed.episodes, public.episodes);
        assert_eq!(observed.policy_step_count, public.policy_step_count);
        assert_eq!(
            observed.physical_decision_count,
            public.physical_decision_count
        );
        let mut public_metrics = public.metrics;
        let mut observed_metrics = observed.metrics;
        public_metrics.total_elapsed_ns = 0;
        observed_metrics.total_elapsed_ns = 0;
        assert_eq!(observed_metrics, public_metrics);
        assert_eq!(
            (
                observed_scorer.calls,
                observed_scorer.decisions,
                observed_scorer.logits
            ),
            (
                public_scorer.calls,
                public_scorer.decisions,
                public_scorer.logits
            )
        );
    }

    #[test]
    fn logical_transcript_is_schedule_invariant() {
        let _lock = TEST_LOCK.lock().unwrap();
        let mut base = config(1, 1, 1, 9);
        base.first_episode_id = 37;
        let reference = synchronous_content_sensitive_reference(&base);
        assert!(reference
            .episodes
            .iter()
            .all(|episode| episode.terminal.terminal_classification
                == TerminalClassificationV1::Natural));
        assert!(
            reference
                .events
                .iter()
                .map(|event| event.scorer_value_bits)
                .collect::<BTreeSet<_>>()
                .len()
                > 1,
            "schedule oracle must exercise distinct scorer value bits"
        );
        let shapes = [(1, 1, 1), (1, 4, 3), (4, 1, 3), (4, 2, 5)];
        for (workers, sessions, target) in shapes {
            let mut shaped = base.clone();
            shaped.worker_count = workers;
            shaped.sessions_per_worker = sessions;
            shaped.broker_batch_target = target;
            let mut scorer = ContentSensitiveScorer::default();
            let (result, events) = capture_async_action_events(|| {
                run_async_flat_scored_rollout_v1(shaped, &mut scorer).unwrap()
            });
            assert!(result.all_natural());
            assert!(scorer.saw_nonuniform_multi_action);
            assert!(scorer.distinct_payloads.len() > 1);
            assert_eq!(result.episodes, reference.episodes);
            assert_eq!(
                first_action_event_mismatch(&reference.events, &events),
                None
            );
            assert_eq!(result.policy_step_count, reference.policy_step_count);
            assert_eq!(
                result.physical_decision_count,
                reference.physical_decision_count
            );
            assert_eq!(
                result.metrics.scored_decision_count,
                u64::try_from(reference.events.len()).unwrap()
            );
            assert_eq!(
                result.metrics.scored_action_logit_count,
                reference.scored_action_logit_count
            );
        }
    }

    #[test]
    fn observed_selected_and_terminal_stream_is_canonical_and_schedule_invariant() {
        let _lock = TEST_LOCK.lock().unwrap();
        let mut base = config(1, 1, 1, 9);
        base.first_episode_id = 37;
        let shapes = [(1, 1, 1), (1, 4, 3), (4, 1, 3), (4, 2, 5)];
        let mut reference_stream = None;

        for (workers, sessions, target) in shapes {
            let mut shaped = base.clone();
            shaped.worker_count = workers;
            shaped.sessions_per_worker = sessions;
            shaped.broker_batch_target = target;
            let mut scorer = ContentSensitiveScorer::default();
            let (result, stream) = run_async_flat_scored_rollout_observed_v1(
                shaped,
                &mut scorer,
                CanonicalTestTrajectoryObserverV1::default(),
            )
            .unwrap();

            assert!(result.all_natural());
            assert_eq!(
                stream
                    .iter()
                    .filter(|event| matches!(event, TestObservedTrajectoryEventV1::Selected(_)))
                    .count(),
                usize::try_from(result.metrics.sampled_action_count).unwrap()
            );
            let observed_terminals = stream
                .iter()
                .filter_map(|event| match event {
                    TestObservedTrajectoryEventV1::Terminal(event) => Some(*event),
                    TestObservedTrajectoryEventV1::Selected(_) => None,
                })
                .collect::<Vec<_>>();
            let expected_terminals = result
                .episodes
                .iter()
                .map(|episode| FlatScoredTerminalEventV1 {
                    terminal: episode.terminal,
                    learner_action_count: episode.learner_action_count,
                    learner_trace_hash: episode.learner_trace_hash,
                })
                .collect::<Vec<_>>();
            assert_eq!(observed_terminals, expected_terminals);
            assert!(stream
                .windows(2)
                .all(|events| events[0].canonical_key() < events[1].canonical_key()));
            match &reference_stream {
                Some(reference) => assert_eq!(&stream, reference),
                None => reference_stream = Some(stream),
            }
        }
    }

    #[test]
    fn synchronous_reference_oracle_rejects_packet_misassociation() {
        let _lock = TEST_LOCK.lock().unwrap();
        let mut config = config(1, 1, 1, 9);
        config.first_episode_id = 37;
        let reference = synchronous_content_sensitive_reference(&config);
        let mut misassociated = reference.events.clone();
        let first_payload = misassociated
            .first()
            .expect("reference must contain scored events")
            .safe_packet_payload
            .clone();
        let source_index = misassociated
            .iter()
            .position(|event| event.safe_packet_payload != first_payload)
            .expect("reference must contain distinct scored packet payloads");
        misassociated[0].safe_packet_payload =
            misassociated[source_index].safe_packet_payload.clone();

        assert_eq!(
            first_action_event_mismatch(&reference.events, &misassociated),
            Some(0)
        );
    }

    #[test]
    fn synchronous_reference_oracle_rejects_value_misassociation() {
        let _lock = TEST_LOCK.lock().unwrap();
        let mut config = config(1, 1, 1, 9);
        config.first_episode_id = 37;
        let reference = synchronous_content_sensitive_reference(&config);
        let mut misassociated = reference.events.clone();
        let first_value_bits = misassociated
            .first()
            .expect("reference must contain scored events")
            .scorer_value_bits;
        let source_index = misassociated
            .iter()
            .position(|event| event.scorer_value_bits != first_value_bits)
            .expect("reference must contain distinct scorer values");
        let source_value_bits = misassociated[source_index].scorer_value_bits;
        misassociated[0].scorer_value_bits = source_value_bits;
        misassociated[source_index].scorer_value_bits = first_value_bits;

        assert_eq!(
            first_action_event_mismatch(&reference.events, &misassociated),
            Some(0)
        );
    }

    #[test]
    fn asynchronous_oracle_rejects_scorer_value_row_swap() {
        let _lock = TEST_LOCK.lock().unwrap();
        let mut config = config(1, 4, 4, 9);
        config.first_episode_id = 37;
        let reference = synchronous_content_sensitive_reference(&config);
        let mut scorer = ValueSwappingScorer::default();
        let (result, events) = capture_async_action_events(|| {
            run_async_flat_scored_rollout_v1(config, &mut scorer).unwrap()
        });

        assert!(result.all_natural());
        assert!(scorer.swapped);
        assert_eq!(result.episodes, reference.episodes);
        assert!(first_action_event_mismatch(&reference.events, &events).is_some());
    }

    struct FailingScorer {
        calls: u64,
        fail_on: u64,
    }

    impl FlatBatchScorerV1 for FailingScorer {
        fn score_batch_v1(
            &mut self,
            _batch: &FlatScoringBatchViewV1<'_>,
            logits: &mut [f32],
            values: &mut [f32],
        ) -> Result<(), FlatBatchScorerErrorV1> {
            self.calls += 1;
            logits.fill(0.0);
            values.fill(0.0);
            if self.calls == self.fail_on {
                Err(FlatBatchScorerErrorV1::new(77))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn later_chunk_scorer_failure_cancels_without_hanging() {
        let _lock = TEST_LOCK.lock().unwrap();
        let mut scorer = FailingScorer {
            calls: 0,
            fail_on: 2,
        };
        let error = run_async_flat_scored_rollout_v1(config(1, 4, 1, 4), &mut scorer).unwrap_err();
        assert_eq!(
            error,
            AsyncFlatScoredRolloutErrorV1::ScorerFailed {
                batch_index: 1,
                code: 77,
            }
        );
    }

    #[test]
    fn first_selected_observer_error_releases_no_multi_lane_action_or_output() {
        let _lock = TEST_LOCK.lock().unwrap();
        TEST_CONSUMED_ACTION_COUNT_V1.store(0, Ordering::SeqCst);
        let callback_count = Arc::new(AtomicU64::new(0));
        let finished = Arc::new(AtomicBool::new(false));
        let observer = FailingFirstSelectedObserverV1 {
            callback_count: Arc::clone(&callback_count),
            finished: Arc::clone(&finished),
        };
        let mut scorer = ZeroScorer::default();

        let error =
            run_async_flat_scored_rollout_observed_v1(config(1, 4, 4, 4), &mut scorer, observer)
                .unwrap_err();

        assert!(matches!(
            error,
            AsyncFlatScoredObservedRunErrorV1::ObserverFailed {
                phase: FlatScoredObserverPhaseV1::Selected,
                error: 91,
            }
        ));
        assert_eq!(scorer.calls, 1);
        assert_eq!(scorer.decisions, 4);
        assert_eq!(callback_count.load(Ordering::SeqCst), 1);
        assert!(!finished.load(Ordering::SeqCst));
        assert_eq!(TEST_CONSUMED_ACTION_COUNT_V1.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn first_selected_observer_panic_releases_no_multi_lane_action_or_output() {
        let _lock = TEST_LOCK.lock().unwrap();
        TEST_CONSUMED_ACTION_COUNT_V1.store(0, Ordering::SeqCst);
        let callback_count = Arc::new(AtomicU64::new(0));
        let finished = Arc::new(AtomicBool::new(false));
        let observer = PanickingFirstSelectedObserverV1 {
            callback_count: Arc::clone(&callback_count),
            finished: Arc::clone(&finished),
        };
        let mut scorer = ZeroScorer::default();

        let error =
            run_async_flat_scored_rollout_observed_v1(config(1, 4, 4, 4), &mut scorer, observer)
                .unwrap_err();

        assert!(matches!(
            error,
            AsyncFlatScoredObservedRunErrorV1::ObserverPanicked {
                phase: FlatScoredObserverPhaseV1::Selected,
            }
        ));
        assert_eq!(scorer.calls, 1);
        assert_eq!(scorer.decisions, 4);
        assert_eq!(callback_count.load(Ordering::SeqCst), 1);
        assert!(!finished.load(Ordering::SeqCst));
        assert_eq!(TEST_CONSUMED_ACTION_COUNT_V1.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn slow_first_selected_observer_deadline_releases_no_action_or_output() {
        let _lock = TEST_LOCK.lock().unwrap();
        TEST_CONSUMED_ACTION_COUNT_V1.store(0, Ordering::SeqCst);
        let callback_count = Arc::new(AtomicU64::new(0));
        let finished = Arc::new(AtomicBool::new(false));
        let observer = SlowFirstSelectedObserverV1 {
            callback_count: Arc::clone(&callback_count),
            finished: Arc::clone(&finished),
            delay: Duration::from_millis(1_100),
        };
        let mut scorer = ZeroScorer::default();
        let mut shaped = config(1, 4, 4, 4);
        shaped.scheduler_timeout = Duration::from_secs(1);

        let error =
            run_async_flat_scored_rollout_observed_v1(shaped, &mut scorer, observer).unwrap_err();

        assert!(matches!(
            error,
            AsyncFlatScoredObservedRunErrorV1::Rollout(
                AsyncFlatScoredRolloutErrorV1::SchedulerDeadlineExceeded
            )
        ));
        assert_eq!(scorer.calls, 1);
        assert_eq!(scorer.decisions, 4);
        assert_eq!(callback_count.load(Ordering::SeqCst), 1);
        assert!(!finished.load(Ordering::SeqCst));
        assert_eq!(TEST_CONSUMED_ACTION_COUNT_V1.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn terminal_observer_error_maps_phase_and_releases_no_terminal_ack_or_output() {
        let _lock = TEST_LOCK.lock().unwrap();
        TEST_CONSUMED_ACTION_COUNT_V1.store(0, Ordering::SeqCst);
        TEST_TERMINAL_ACK_COUNT_V1.store(0, Ordering::SeqCst);
        let mut scorer = ZeroScorer::default();

        let error = run_async_flat_scored_rollout_observed_v1(
            config(1, 1, 1, 1),
            &mut scorer,
            ErroringTerminalObserverV1,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            AsyncFlatScoredObservedRunErrorV1::ObserverFailed {
                phase: FlatScoredObserverPhaseV1::Terminal,
                error: 92,
            }
        ));
        assert!(TEST_CONSUMED_ACTION_COUNT_V1.load(Ordering::SeqCst) > 0);
        assert_eq!(TEST_TERMINAL_ACK_COUNT_V1.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn terminal_observer_panic_maps_phase_and_releases_no_terminal_ack_or_output() {
        let _lock = TEST_LOCK.lock().unwrap();
        TEST_CONSUMED_ACTION_COUNT_V1.store(0, Ordering::SeqCst);
        TEST_TERMINAL_ACK_COUNT_V1.store(0, Ordering::SeqCst);
        let mut scorer = ZeroScorer::default();

        let error = run_async_flat_scored_rollout_observed_v1(
            config(1, 1, 1, 1),
            &mut scorer,
            PanickingTerminalObserverV1,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            AsyncFlatScoredObservedRunErrorV1::ObserverPanicked {
                phase: FlatScoredObserverPhaseV1::Terminal,
            }
        ));
        assert!(TEST_CONSUMED_ACTION_COUNT_V1.load(Ordering::SeqCst) > 0);
        assert_eq!(TEST_TERMINAL_ACK_COUNT_V1.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn finish_observer_error_maps_phase_and_returns_no_result_or_output() {
        let _lock = TEST_LOCK.lock().unwrap();
        TEST_TERMINAL_ACK_COUNT_V1.store(0, Ordering::SeqCst);
        let mut scorer = ZeroScorer::default();

        let error = run_async_flat_scored_rollout_observed_v1(
            config(1, 1, 1, 1),
            &mut scorer,
            ErroringFinishObserverV1,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            AsyncFlatScoredObservedRunErrorV1::ObserverFailed {
                phase: FlatScoredObserverPhaseV1::Finish,
                error: 93,
            }
        ));
        assert_eq!(TEST_TERMINAL_ACK_COUNT_V1.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn finish_observer_panic_maps_phase_and_returns_no_result_or_output() {
        let _lock = TEST_LOCK.lock().unwrap();
        TEST_TERMINAL_ACK_COUNT_V1.store(0, Ordering::SeqCst);
        let mut scorer = ZeroScorer::default();

        let error = run_async_flat_scored_rollout_observed_v1(
            config(1, 1, 1, 1),
            &mut scorer,
            PanickingFinishObserverV1,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            AsyncFlatScoredObservedRunErrorV1::ObserverPanicked {
                phase: FlatScoredObserverPhaseV1::Finish,
            }
        ));
        assert_eq!(TEST_TERMINAL_ACK_COUNT_V1.load(Ordering::SeqCst), 1);
    }

    struct UnwrittenScorer;

    impl FlatBatchScorerV1 for UnwrittenScorer {
        fn score_batch_v1(
            &mut self,
            _batch: &FlatScoringBatchViewV1<'_>,
            _logits: &mut [f32],
            _values: &mut [f32],
        ) -> Result<(), FlatBatchScorerErrorV1> {
            Ok(())
        }
    }

    #[test]
    fn unwritten_scorer_output_fails_closed() {
        let _lock = TEST_LOCK.lock().unwrap();
        let error =
            run_async_flat_scored_rollout_v1(config(1, 1, 1, 1), &mut UnwrittenScorer).unwrap_err();
        assert!(matches!(
            error,
            AsyncFlatScoredRolloutErrorV1::ScorerOutputNonFinite {
                batch_index: 0,
                is_value: false,
                ..
            }
        ));
    }

    struct NonFiniteValueScorer {
        value_bits: u32,
    }

    impl FlatBatchScorerV1 for NonFiniteValueScorer {
        fn score_batch_v1(
            &mut self,
            _batch: &FlatScoringBatchViewV1<'_>,
            logits: &mut [f32],
            values: &mut [f32],
        ) -> Result<(), FlatBatchScorerErrorV1> {
            logits.fill(0.0);
            values.fill(0.0);
            values[0] = f32::from_bits(self.value_bits);
            Ok(())
        }
    }

    #[test]
    fn non_finite_scorer_value_fails_closed_with_exact_bits() {
        let _lock = TEST_LOCK.lock().unwrap();
        for value_bits in [0x7fc0_1234, f32::INFINITY.to_bits()] {
            let error = run_async_flat_scored_rollout_v1(
                config(1, 1, 1, 1),
                &mut NonFiniteValueScorer { value_bits },
            )
            .unwrap_err();
            assert_eq!(
                error,
                AsyncFlatScoredRolloutErrorV1::ScorerOutputNonFinite {
                    batch_index: 0,
                    output_index: 0,
                    is_value: true,
                    bits: value_bits,
                }
            );
        }
    }

    #[test]
    fn finite_scorer_value_wrapper_rejects_every_non_finite_class() {
        let _lock = TEST_LOCK.lock().unwrap();
        assert_eq!(
            FiniteScorerValueV1::new(-0.0),
            Some(FiniteScorerValueV1(0x8000_0000))
        );
        for value in [f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
            assert_eq!(FiniteScorerValueV1::new(value), None);
        }
    }

    #[test]
    fn apply_reply_rejects_corrupted_learner_ordinal_before_consume() {
        let _lock = TEST_LOCK.lock().unwrap();
        TEST_CONSUMED_ACTION_COUNT_V1.store(0, Ordering::SeqCst);
        let shaped = config(1, 1, 1, 1);
        let end_episode_id = shaped.first_episode_id + shaped.episode_count;
        let mut lane = LocalLaneV1::vacant(0, 0, shaped.first_episode_id, end_episode_id);
        lane.fill(&shaped, end_episode_id, 1).unwrap();
        let mut decisions = Vec::new();
        let mut terminals = Vec::new();
        lane.advance_to_event(
            &shaped,
            Instant::now() + Duration::from_secs(30),
            &AtomicBool::new(false),
            &mut decisions,
            &mut terminals,
        )
        .unwrap();
        assert!(terminals.is_empty());
        let decision = decisions.pop().expect("lane must reach a learner decision");
        assert!(decisions.is_empty());
        let binding = decision.packet.decision().binding;
        let mut reply = WorkerReplyV1 {
            actions: vec![ActionReplyV1 {
                logical_lane_id: 0,
                scored: BoundScoredActionV1 {
                    binding,
                    learner_ordinal: 1,
                    selected_index: 0,
                    predicted_value: FiniteScorerValueV1::new(0.0).unwrap(),
                },
                packet: decision.packet,
            }],
            terminal_acks: Vec::new(),
        };

        let error = lane.apply_reply(&mut reply).unwrap_err();
        assert_eq!(
            error.phase,
            AsyncFlatScoredWorkerPhaseV1::LearnerActionBinding
        );
        assert_eq!(lane.learner_action_count, 0);
        assert_eq!(TEST_CONSUMED_ACTION_COUNT_V1.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn partial_round_deadline_disconnects_senders_and_joins() {
        let _lock = TEST_LOCK.lock().unwrap();
        TEST_DELAY_WORKER_ID_V1.store(1, std::sync::atomic::Ordering::SeqCst);
        TEST_DELAY_WORKER_MS_V1.store(100, std::sync::atomic::Ordering::SeqCst);
        let mut delayed = config(2, 1, 2, 2);
        delayed.scheduler_timeout = Duration::from_millis(20);
        let started = Instant::now();
        let error =
            run_async_flat_scored_rollout_v1(delayed, &mut ZeroScorer::default()).unwrap_err();
        TEST_DELAY_WORKER_ID_V1.store(usize::MAX, std::sync::atomic::Ordering::SeqCst);
        TEST_DELAY_WORKER_MS_V1.store(0, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            error,
            AsyncFlatScoredRolloutErrorV1::SchedulerDeadlineExceeded
        );
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn worker_disconnect_at_deadline_has_stable_deadline_error() {
        let _lock = TEST_LOCK.lock().unwrap();
        TEST_DELAY_WORKER_ID_V1.store(0, std::sync::atomic::Ordering::SeqCst);
        TEST_DELAY_WORKER_MS_V1.store(50, std::sync::atomic::Ordering::SeqCst);
        let mut delayed = config(1, 1, 1, 1);
        delayed.scheduler_timeout = Duration::from_millis(10);
        let error =
            run_async_flat_scored_rollout_v1(delayed, &mut ZeroScorer::default()).unwrap_err();
        TEST_DELAY_WORKER_ID_V1.store(usize::MAX, std::sync::atomic::Ordering::SeqCst);
        TEST_DELAY_WORKER_MS_V1.store(0, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            error,
            AsyncFlatScoredRolloutErrorV1::SchedulerDeadlineExceeded
        );
    }

    #[test]
    fn final_reduction_cannot_return_success_after_public_deadline() {
        let _lock = TEST_LOCK.lock().unwrap();
        TEST_ENTERED_FINAL_REDUCTION_V1.store(false, std::sync::atomic::Ordering::SeqCst);
        TEST_DELAY_FINAL_REDUCTION_MS_V1.store(1_100, std::sync::atomic::Ordering::SeqCst);
        let mut delayed = config(1, 1, 1, 1);
        delayed.scheduler_timeout = Duration::from_secs(1);
        let error =
            run_async_flat_scored_rollout_v1(delayed, &mut ZeroScorer::default()).unwrap_err();
        TEST_DELAY_FINAL_REDUCTION_MS_V1.store(0, std::sync::atomic::Ordering::SeqCst);
        assert!(TEST_ENTERED_FINAL_REDUCTION_V1.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(
            error,
            AsyncFlatScoredRolloutErrorV1::SchedulerDeadlineExceeded
        );
    }

    #[test]
    fn late_reply_send_failure_releases_no_partial_round() {
        let _lock = TEST_LOCK.lock().unwrap();
        TEST_EXIT_AFTER_ROUND_WORKER_ID_V1.store(1, std::sync::atomic::Ordering::SeqCst);
        TEST_CONSUMED_ACTION_COUNT_V1.store(0, std::sync::atomic::Ordering::SeqCst);
        let error =
            run_async_flat_scored_rollout_v1(config(2, 1, 2, 2), &mut ZeroScorer::default())
                .unwrap_err();
        TEST_EXIT_AFTER_ROUND_WORKER_ID_V1.store(usize::MAX, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            error,
            AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation
        );
        assert_eq!(
            TEST_CONSUMED_ACTION_COUNT_V1.load(std::sync::atomic::Ordering::SeqCst),
            0
        );
    }

    #[derive(Default)]
    struct PacketGoldenScorer {
        counts: Vec<[usize; 11]>,
        payloads: Vec<String>,
    }

    impl FlatBatchScorerV1 for PacketGoldenScorer {
        fn score_batch_v1(
            &mut self,
            batch: &FlatScoringBatchViewV1<'_>,
            logits: &mut [f32],
            values: &mut [f32],
        ) -> Result<(), FlatBatchScorerErrorV1> {
            for decision_index in 0..batch.decision_count() {
                if self.payloads.len() == 5 {
                    break;
                }
                let decision = batch.decision(decision_index).unwrap();
                self.counts.push([
                    decision.objects().len(),
                    decision.relations().len(),
                    decision.object_subtypes().len(),
                    decision.ability_uses().len(),
                    decision.goads().len(),
                    decision.completed_dungeons().len(),
                    decision.effect_subtype_changes().len(),
                    decision.context_path_elements().len(),
                    decision.actions().len(),
                    decision.action_refs().len(),
                    batch.action_offsets()[decision_index + 1]
                        - batch.action_offsets()[decision_index],
                ]);
                self.payloads.push(format!(
                    "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
                    batch.contract(),
                    decision.globals(),
                    decision.objects(),
                    decision.relations(),
                    decision.object_subtypes(),
                    decision.ability_uses(),
                    decision.goads(),
                    decision.completed_dungeons(),
                    decision.effect_subtype_changes(),
                    decision.context_path_elements(),
                    decision.actions(),
                    decision.action_refs(),
                ));
            }
            logits.fill(0.0);
            values.fill(0.0);
            Ok(())
        }
    }

    #[test]
    fn first_five_scorer_packets_have_exact_safe_golden() {
        let _lock = TEST_LOCK.lock().unwrap();
        let mut scorer = PacketGoldenScorer::default();
        let result = run_async_flat_scored_rollout_v1(config(1, 1, 1, 1), &mut scorer).unwrap();
        assert!(result.all_natural());
        assert_eq!(scorer.payloads.len(), 5);
        let mut hash = Sha256::new();
        for payload in &scorer.payloads {
            hash.update((payload.len() as u64).to_le_bytes());
            hash.update(payload.as_bytes());
        }
        let digest = format!("{:x}", hash.finalize());
        assert_eq!(
            scorer.counts,
            [
                [10, 0, 3, 0, 0, 0, 0, 0, 2, 1, 2],
                [10, 0, 3, 0, 0, 0, 0, 0, 2, 1, 2],
                [12, 8, 3, 0, 0, 0, 0, 0, 8, 8, 8],
                [14, 4, 5, 1, 0, 0, 0, 0, 2, 2, 2],
                [15, 1, 5, 0, 0, 0, 0, 0, 2, 1, 2],
            ]
        );
        assert_eq!(
            digest,
            "e1aa12d2736f4c52fed56fc89defb9f16bcda852631b2eee8c2e8bb707943981"
        );
    }
}
