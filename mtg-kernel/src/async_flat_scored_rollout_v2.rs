//! Explicit Flat Policy V2 scored-rollout boundary.
//!
//! The deterministic scheduler is shared with V1 through a private generic
//! family contract. This module supplies only V2-owned packets, bindings,
//! scorer views, callbacks, and public result types. A V1 packet or binding is
//! therefore unrepresentable on this path, while V1 remains independently
//! runnable through [`crate::async_flat_scored_rollout_v1`].

use crate::async_flat_scored_rollout_v1::{
    run_async_flat_scored_rollout_core, AsyncFlatScoredObservedRunErrorV1,
    AsyncFlatScoredRolloutErrorV1, AsyncFlatScoredRolloutMetricsV1, AsyncFlatScoredRolloutResultV1,
    AsyncFlatScoredWorkerPhaseV1, FlatBatchScorerCore, FlatBatchScorerErrorV1,
    FlatScoredExecutionScheduleV1, FlatScoredFamilyCore, FlatScoredObserverPhaseV1,
    FlatScoredSelectedEventCore, FlatScoredTerminalEventV1, FlatScoredTrajectoryObserverCore,
    RoundDecisionCore, ASYNC_FLAT_SCORED_SAMPLER_ID_V1, ASYNC_FLAT_SCORED_SAMPLER_VERSION_V1,
    ASYNC_FLAT_SCORED_SPLITMIX_GAMMA_V1,
};
use crate::async_rollout::{AsyncRolloutEpisodeV1, AsyncRolloutTerminalV1};
use crate::async_rollout_v2::{
    AsyncRolloutConfigV2, ASYNC_ROLLOUT_MAX_SESSIONS_PER_WORKER_V2, ASYNC_ROLLOUT_MAX_WORKERS_V2,
};
use crate::fast_sampler::{FastCategoricalError, FAST_CATEGORICAL_MAX_ACTIONS};
use crate::flat_policy_v2::{
    FlatCompletedDungeonV2, FlatContextPathElementV2, FlatDecisionBindingV2, FlatDecisionEncoderV2,
    FlatDecisionV2, FlatEffectSubtypeChangeV2, FlatObjectAbilityUseV2, FlatObjectCoreV2,
    FlatObjectGoadV2, FlatObjectSubtypeV2, FlatPolicyContractDigestsV2, FlatRelationPayloadV2,
    FlatRelationV2, FlatScorerActionCoreV2, FlatScorerActionRefV2, FlatScoringDecisionViewV2,
    FlatScoringOwnedBuffersV2, FLAT_ACTION_REF_INTERNAL_TO_PROJECTION_V2,
    FLAT_ACTION_REF_PROJECTION_ROLE_MAPPING_VERSION_V2,
    FLAT_POLICY_CONTEXT_SUBROLE_MAPPING_VERSION_V2, FLAT_POLICY_CONTRACT_DIGESTS_V2,
    FLAT_POLICY_ENUM_MAPPING_VERSION_V2, FLAT_POLICY_FEATURE_INVENTORY_VERSION_V2,
    FLAT_POLICY_OBJECT_GROUP_MAPPING_VERSION_V2, FLAT_POLICY_RELATION_ROLE_MAPPING_VERSION_V2,
    FLAT_POLICY_TYPED_LAYOUT_VERSION_V2, FLAT_SCORER_ACTION_REF_VERSION_V2,
    FLAT_SCORER_PACKET_VERSION_V2, FLAT_SCORER_VISIBLE_MANIFEST_V2,
    FLAT_SCORER_VISIBLE_MANIFEST_VERSION_V2,
};
use crate::rl::{TerminalClassificationV1, TerminalSafeCodeV2};
use crate::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1,
    FLAT_ACTION_CANDIDATE_COMMITMENT_VERSION_V2, FLAT_ACTION_CARD_TOKEN_MAPPING_VERSION_V2,
    FLAT_ACTION_DECISION_SLICE_VERSION_V2, FLAT_ACTION_REF_ROLE_MAPPING_VERSION_V2,
};
use std::fmt;

#[cfg(test)]
use crate::rl_session::FlatActionDecisionBindingV2;

pub const ASYNC_FLAT_SCORED_ROLLOUT_VERSION_V2: u32 = 2;
/// V2 changes the packet family, not the sampler or seed derivation.
pub const ASYNC_FLAT_SCORED_SAMPLER_VERSION_V2: u32 = ASYNC_FLAT_SCORED_SAMPLER_VERSION_V1;
pub const ASYNC_FLAT_SCORED_SPLITMIX_GAMMA_V2: u64 = ASYNC_FLAT_SCORED_SPLITMIX_GAMMA_V1;
pub const ASYNC_FLAT_SCORED_SAMPLER_ID_V2: &str = ASYNC_FLAT_SCORED_SAMPLER_ID_V1;
pub const ASYNC_FLAT_SCORED_MEMBERSHIP_DIGEST_IDENTITY_V1: &str =
    "mtg-kernel-async-flat-scored-four-lane-membership-digest-v1";
pub const ASYNC_FLAT_SCORED_MEMBERSHIP_DIGEST_DOMAIN_V2: &[u8] =
    b"mtg-kernel/async-flat-scored-rollout-v2/membership/v2";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlatScorerContractV2 {
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
    pub contract_digests: FlatPolicyContractDigestsV2,
}

pub struct FlatScoringBatchViewV2<'a> {
    contract: FlatScorerContractV2,
    decisions: &'a [RoundDecisionCore<FlatScoredFamilyV2>],
    action_offsets: &'a [usize],
}

impl<'a> FlatScoringBatchViewV2<'a> {
    pub fn contract(&self) -> FlatScorerContractV2 {
        self.contract
    }

    pub fn decision_count(&self) -> usize {
        self.decisions.len()
    }

    pub fn decision(&self, index: usize) -> Option<FlatScoringDecisionViewV2<'_>> {
        self.decisions
            .get(index)
            .map(|decision| FlatScoredFamilyV2::packet_view(&decision.packet))
    }

    /// Crate-private scorer/trajectory association key. Public scorers need
    /// only the typed decision view; the native trainer additionally binds an
    /// owned tensor to the exact packet that produced it.
    pub(crate) fn binding(&self, index: usize) -> Option<FlatDecisionBindingV2> {
        self.decisions
            .get(index)
            .map(|decision| FlatScoredFamilyV2::packet_binding(&decision.packet))
    }

    /// Crate-private owned clone of the validated packet, so a scorer can
    /// tensorize on worker threads that outlive this borrowed view. The clone
    /// carries exactly the validated content `decision` exposes.
    pub(crate) fn cloned_validated_packet(
        &self,
        index: usize,
    ) -> Option<ValidatedOwnedFlatScoringDecisionV2> {
        self.decisions
            .get(index)
            .map(|decision| decision.packet.clone())
    }

    pub fn action_offsets(&self) -> &[usize] {
        self.action_offsets
    }

    pub fn total_action_count(&self) -> usize {
        self.action_offsets.last().copied().unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlatBatchScorerErrorV2 {
    pub code: u32,
}

impl FlatBatchScorerErrorV2 {
    pub const fn new(code: u32) -> Self {
        Self { code }
    }
}

impl fmt::Display for FlatBatchScorerErrorV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "flat V2 batch scorer rejected input with code {}",
            self.code
        )
    }
}

impl std::error::Error for FlatBatchScorerErrorV2 {}

pub trait FlatBatchScorerV2 {
    fn score_batch_v2(
        &mut self,
        batch: &FlatScoringBatchViewV2<'_>,
        action_logits: &mut [f32],
        values: &mut [f32],
    ) -> Result<(), FlatBatchScorerErrorV2>;
}

pub(crate) struct FlatScoredSelectedEventV2<'a> {
    pub(crate) expected: FastActorDecisionV1,
    pub(crate) binding: FlatDecisionBindingV2,
    pub(crate) learner_ordinal: u64,
    pub(crate) action_seed: u64,
    pub(crate) selected_index: u32,
    pub(crate) raw_action_logits: &'a [f32],
    pub(crate) predicted_value_bits: u32,
    pub(crate) decision: FlatScoringDecisionViewV2<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FlatScoredTerminalEventV2 {
    pub(crate) terminal: AsyncRolloutTerminalV1,
    pub(crate) learner_action_count: u64,
    pub(crate) learner_trace_hash: u64,
    pub(crate) native_full_trajectory_receipt:
        Option<crate::native_full_episode_trajectory_v1::NativeFullEpisodeTrajectoryReceiptV1>,
}

pub(crate) trait FlatScoredTrajectoryObserverV2: Sized {
    type Error;
    type Output;

    const OBSERVES_TRAJECTORY: bool = true;

    fn observe_selected_v2(
        &mut self,
        event: FlatScoredSelectedEventV2<'_>,
    ) -> Result<(), Self::Error>;

    fn observe_terminal_v2(&mut self, event: FlatScoredTerminalEventV2) -> Result<(), Self::Error>;

    fn finish_v2(self) -> Result<Self::Output, Self::Error>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlatScoredObserverPhaseV2 {
    Selected,
    Terminal,
    Finish,
}

#[derive(Debug)]
pub(crate) enum AsyncFlatScoredObservedRunErrorV2<E> {
    Rollout(AsyncFlatScoredRolloutErrorV2),
    ObserverFailed {
        phase: FlatScoredObserverPhaseV2,
        error: E,
    },
    ObserverPanicked {
        phase: FlatScoredObserverPhaseV2,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AsyncFlatScoredRolloutMetricsV2 {
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
    pub batch_membership_digest: [u8; 32],
}

impl AsyncFlatScoredRolloutMetricsV2 {
    pub fn mean_batch_width(self) -> f64 {
        if self.scorer_batch_count == 0 {
            0.0
        } else {
            self.batch_width_sum as f64 / self.scorer_batch_count as f64
        }
    }
}

impl From<AsyncFlatScoredRolloutMetricsV1> for AsyncFlatScoredRolloutMetricsV2 {
    fn from(metrics: AsyncFlatScoredRolloutMetricsV1) -> Self {
        Self {
            total_elapsed_ns: metrics.total_elapsed_ns,
            complete_round_count: metrics.complete_round_count,
            scorer_batch_count: metrics.scorer_batch_count,
            scored_decision_count: metrics.scored_decision_count,
            scored_action_logit_count: metrics.scored_action_logit_count,
            sampled_action_count: metrics.sampled_action_count,
            terminal_notification_count: metrics.terminal_notification_count,
            batch_width_sum: metrics.batch_width_sum,
            max_batch_width: metrics.max_batch_width,
            full_target_batch_count: metrics.full_target_batch_count,
            short_batch_count: metrics.short_batch_count,
            broker_service_ns: metrics.broker_service_ns,
            batch_membership_digest: metrics.batch_membership_digest,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncFlatScoredRolloutResultV2 {
    pub episodes: Vec<AsyncRolloutEpisodeV1>,
    pub policy_step_count: u64,
    pub physical_decision_count: u64,
    pub metrics: AsyncFlatScoredRolloutMetricsV2,
}

impl AsyncFlatScoredRolloutResultV2 {
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

impl From<AsyncFlatScoredRolloutResultV1> for AsyncFlatScoredRolloutResultV2 {
    fn from(result: AsyncFlatScoredRolloutResultV1) -> Self {
        Self {
            episodes: result.episodes,
            policy_step_count: result.policy_step_count,
            physical_decision_count: result.physical_decision_count,
            metrics: result.metrics.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncFlatScoredWorkerPhaseV2 {
    Reset,
    Encode,
    LearnerActionBinding,
    LearnerConsume,
    OpponentStep,
    Protocol,
    Panic,
}

impl From<AsyncFlatScoredWorkerPhaseV1> for AsyncFlatScoredWorkerPhaseV2 {
    fn from(phase: AsyncFlatScoredWorkerPhaseV1) -> Self {
        match phase {
            AsyncFlatScoredWorkerPhaseV1::Reset => Self::Reset,
            AsyncFlatScoredWorkerPhaseV1::Encode => Self::Encode,
            AsyncFlatScoredWorkerPhaseV1::LearnerActionBinding => Self::LearnerActionBinding,
            AsyncFlatScoredWorkerPhaseV1::LearnerConsume => Self::LearnerConsume,
            AsyncFlatScoredWorkerPhaseV1::OpponentStep => Self::OpponentStep,
            AsyncFlatScoredWorkerPhaseV1::Protocol => Self::Protocol,
            AsyncFlatScoredWorkerPhaseV1::Panic => Self::Panic,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncFlatScoredRolloutErrorV2 {
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
        phase: AsyncFlatScoredWorkerPhaseV2,
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

impl From<AsyncFlatScoredRolloutErrorV1> for AsyncFlatScoredRolloutErrorV2 {
    fn from(error: AsyncFlatScoredRolloutErrorV1) -> Self {
        match error {
            AsyncFlatScoredRolloutErrorV1::InvalidWorkerCount { requested } => {
                Self::InvalidWorkerCount { requested }
            }
            AsyncFlatScoredRolloutErrorV1::InvalidSessionsPerWorker { requested } => {
                Self::InvalidSessionsPerWorker { requested }
            }
            AsyncFlatScoredRolloutErrorV1::InvalidBrokerBatchTarget {
                requested,
                logical_lanes,
            } => Self::InvalidBrokerBatchTarget {
                requested,
                logical_lanes,
            },
            AsyncFlatScoredRolloutErrorV1::InvalidSchedulerTimeout => Self::InvalidSchedulerTimeout,
            AsyncFlatScoredRolloutErrorV1::EmptyEpisodeRange => Self::EmptyEpisodeRange,
            AsyncFlatScoredRolloutErrorV1::EpisodeRangeOverflow => Self::EpisodeRangeOverflow,
            AsyncFlatScoredRolloutErrorV1::EpisodeCountExceedsAddressSpace { requested } => {
                Self::EpisodeCountExceedsAddressSpace { requested }
            }
            AsyncFlatScoredRolloutErrorV1::ResultAllocationFailed { requested } => {
                Self::ResultAllocationFailed { requested }
            }
            AsyncFlatScoredRolloutErrorV1::WorkerSpawnFailed { worker_id } => {
                Self::WorkerSpawnFailed { worker_id }
            }
            AsyncFlatScoredRolloutErrorV1::WorkerFailed {
                worker_id,
                logical_lane_id,
                episode_id,
                phase,
            } => Self::WorkerFailed {
                worker_id,
                logical_lane_id,
                episode_id,
                phase: phase.into(),
            },
            AsyncFlatScoredRolloutErrorV1::ScorerFailed { batch_index, code } => {
                Self::ScorerFailed { batch_index, code }
            }
            AsyncFlatScoredRolloutErrorV1::ScorerPanicked { batch_index } => {
                Self::ScorerPanicked { batch_index }
            }
            AsyncFlatScoredRolloutErrorV1::ScorerOutputNonFinite {
                batch_index,
                output_index,
                is_value,
                bits,
            } => Self::ScorerOutputNonFinite {
                batch_index,
                output_index,
                is_value,
                bits,
            },
            AsyncFlatScoredRolloutErrorV1::SamplingFailed {
                logical_lane_id,
                episode_id,
                decision_ordinal,
                error,
            } => Self::SamplingFailed {
                logical_lane_id,
                episode_id,
                decision_ordinal,
                error,
            },
            AsyncFlatScoredRolloutErrorV1::SchedulerDeadlineExceeded => {
                Self::SchedulerDeadlineExceeded
            }
            AsyncFlatScoredRolloutErrorV1::BrokerProtocolViolation => Self::BrokerProtocolViolation,
            AsyncFlatScoredRolloutErrorV1::WorkerPanicked { worker_id } => {
                Self::WorkerPanicked { worker_id }
            }
        }
    }
}

impl fmt::Display for AsyncFlatScoredRolloutErrorV2 {
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
                write!(formatter, "failed to spawn scored V2 rollout worker {worker_id}")
            }
            Self::WorkerFailed {
                worker_id,
                logical_lane_id,
                episode_id,
                phase,
            } => write!(
                formatter,
                "scored V2 rollout worker {worker_id} failed on lane {logical_lane_id} in {phase:?} for episode {episode_id}"
            ),
            Self::ScorerFailed { batch_index, code } => {
                write!(formatter, "flat V2 scorer batch {batch_index} failed with code {code}")
            }
            Self::ScorerPanicked { batch_index } => {
                write!(formatter, "flat V2 scorer batch {batch_index} panicked")
            }
            Self::ScorerOutputNonFinite {
                batch_index,
                output_index,
                is_value,
                bits,
            } => write!(
                formatter,
                "flat V2 scorer batch {batch_index} left non-finite {} output {output_index} (bits=0x{bits:08x})",
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
                write!(formatter, "cooperative scored V2 rollout deadline exceeded")
            }
            Self::BrokerProtocolViolation => write!(formatter, "scored V2 rollout protocol violation"),
            Self::WorkerPanicked { worker_id } => {
                write!(formatter, "scored V2 rollout worker {worker_id} panicked")
            }
        }
    }
}

impl std::error::Error for AsyncFlatScoredRolloutErrorV2 {}

#[derive(Default, Clone)]
pub(crate) struct OwnedFlatScoringDecisionV2 {
    decision: FlatDecisionV2,
    objects: Vec<FlatObjectCoreV2>,
    relations: Vec<FlatRelationV2>,
    object_subtypes: Vec<FlatObjectSubtypeV2>,
    ability_uses: Vec<FlatObjectAbilityUseV2>,
    goads: Vec<FlatObjectGoadV2>,
    completed_dungeons: Vec<FlatCompletedDungeonV2>,
    effect_subtype_changes: Vec<FlatEffectSubtypeChangeV2>,
    context_path_elements: Vec<FlatContextPathElementV2>,
    actions: Vec<FlatScorerActionCoreV2>,
    scorer_action_refs: Vec<FlatScorerActionRefV2>,
}

impl OwnedFlatScoringDecisionV2 {
    fn scorer_contract(&self) -> FlatScorerContractV2 {
        let binding = self.decision.binding;
        let action = binding.action_binding;
        FlatScorerContractV2 {
            scorer_packet_version: FLAT_SCORER_PACKET_VERSION_V2,
            scorer_action_ref_version: FLAT_SCORER_ACTION_REF_VERSION_V2,
            scorer_visible_manifest_version: FLAT_SCORER_VISIBLE_MANIFEST_VERSION_V2,
            scorer_visible_manifest: FLAT_SCORER_VISIBLE_MANIFEST_V2,
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

    fn scorer_view(&self) -> FlatScoringDecisionViewV2<'_> {
        FlatScoringDecisionViewV2::new(
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

#[derive(Clone)]
pub(crate) struct ValidatedOwnedFlatScoringDecisionV2(OwnedFlatScoringDecisionV2);

impl ValidatedOwnedFlatScoringDecisionV2 {
    /// Crate-private typed view over the validated owned content, for
    /// worker-side tensorization away from the broker's borrowed batch.
    pub(crate) fn scorer_view_v1(&self) -> FlatScoringDecisionViewV2<'_> {
        self.0.scorer_view()
    }
}

fn active_prefix<T>(buffer: &[T], count: u32) -> &[T] {
    let end = usize::try_from(count).expect("u32 active count must fit usize");
    debug_assert!(end <= buffer.len());
    &buffer[..end]
}

pub(crate) fn expected_scorer_contract(card_db_hash: u64) -> FlatScorerContractV2 {
    FlatScorerContractV2 {
        scorer_packet_version: FLAT_SCORER_PACKET_VERSION_V2,
        scorer_action_ref_version: FLAT_SCORER_ACTION_REF_VERSION_V2,
        scorer_visible_manifest_version: FLAT_SCORER_VISIBLE_MANIFEST_VERSION_V2,
        scorer_visible_manifest: FLAT_SCORER_VISIBLE_MANIFEST_V2,
        typed_layout_version: FLAT_POLICY_TYPED_LAYOUT_VERSION_V2,
        feature_inventory_version: FLAT_POLICY_FEATURE_INVENTORY_VERSION_V2,
        enum_mapping_version: FLAT_POLICY_ENUM_MAPPING_VERSION_V2,
        object_group_mapping_version: FLAT_POLICY_OBJECT_GROUP_MAPPING_VERSION_V2,
        relation_role_mapping_version: FLAT_POLICY_RELATION_ROLE_MAPPING_VERSION_V2,
        context_subrole_mapping_version: FLAT_POLICY_CONTEXT_SUBROLE_MAPPING_VERSION_V2,
        action_ref_projection_role_mapping_version:
            FLAT_ACTION_REF_PROJECTION_ROLE_MAPPING_VERSION_V2,
        action_slice_version: FLAT_ACTION_DECISION_SLICE_VERSION_V2,
        action_ref_role_mapping_version: FLAT_ACTION_REF_ROLE_MAPPING_VERSION_V2,
        card_token_mapping_version: FLAT_ACTION_CARD_TOKEN_MAPPING_VERSION_V2,
        candidate_commitment_version: FLAT_ACTION_CANDIDATE_COMMITMENT_VERSION_V2,
        card_db_hash,
        contract_digests: FLAT_POLICY_CONTRACT_DIGESTS_V2,
    }
}

fn validate_packet(packet: &OwnedFlatScoringDecisionV2) -> Result<(), ()> {
    let decision = packet.decision;
    let action_binding = decision.binding.action_binding;
    let object_count = usize::try_from(decision.active_object_count).map_err(|_| ())?;
    let relation_count = usize::try_from(decision.active_relation_count).map_err(|_| ())?;
    let subtype_count = usize::try_from(decision.active_object_subtype_count).map_err(|_| ())?;
    let ability_count = usize::try_from(decision.active_ability_use_count).map_err(|_| ())?;
    let goad_count = usize::try_from(decision.active_goad_count).map_err(|_| ())?;
    let dungeon_count = usize::try_from(decision.active_completed_dungeon_count).map_err(|_| ())?;
    let effect_subtype_count =
        usize::try_from(decision.active_effect_subtype_change_count).map_err(|_| ())?;
    let context_count =
        usize::try_from(decision.active_context_path_element_count).map_err(|_| ())?;
    let action_count = usize::try_from(decision.active_action_count).map_err(|_| ())?;
    let action_ref_count = usize::try_from(decision.active_action_ref_count).map_err(|_| ())?;
    if packet.scorer_contract() != expected_scorer_contract(action_binding.card_db_hash)
        || object_count > packet.objects.len()
        || relation_count > packet.relations.len()
        || subtype_count > packet.object_subtypes.len()
        || ability_count > packet.ability_uses.len()
        || goad_count > packet.goads.len()
        || dungeon_count > packet.completed_dungeons.len()
        || effect_subtype_count > packet.effect_subtype_changes.len()
        || context_count > packet.context_path_elements.len()
        || action_count > packet.actions.len()
        || action_ref_count > packet.scorer_action_refs.len()
        || action_binding.legal_action_count != decision.active_action_count
        || action_count == 0
        || action_count > FAST_CATEGORICAL_MAX_ACTIONS
    {
        return Err(());
    }

    let objects = &packet.objects[..object_count];
    let relations = &packet.relations[..relation_count];
    let object_subtypes = &packet.object_subtypes[..subtype_count];
    let ability_uses = &packet.ability_uses[..ability_count];
    let goads = &packet.goads[..goad_count];
    let actions = &packet.actions[..action_count];
    let action_refs = &packet.scorer_action_refs[..action_ref_count];
    let object_count_u32 = u32::try_from(object_count).map_err(|_| ())?;
    if relations.iter().any(|row| {
        row.source_object
            .is_some_and(|index| index >= object_count_u32)
            || row
                .target_object
                .is_some_and(|index| index >= object_count_u32)
    }) || object_subtypes
        .iter()
        .any(|row| row.object_index >= object_count_u32)
        || ability_uses
            .iter()
            .any(|row| row.object_index >= object_count_u32)
        || goads.iter().any(|row| row.object_index >= object_count_u32)
    {
        return Err(());
    }

    let mut blocked_orders = relations
        .iter()
        .filter_map(|row| match row.payload {
            FlatRelationPayloadV2::CombatAttacker {
                blocked_order: Some(order),
            } => Some(order),
            _ => None,
        })
        .collect::<Vec<_>>();
    blocked_orders.sort_unstable();
    if blocked_orders
        .iter()
        .enumerate()
        .any(|(expected, actual)| u32::try_from(expected).ok() != Some(*actual))
    {
        return Err(());
    }

    let mut ref_cursor = 0usize;
    for (action_index, action) in actions.iter().enumerate() {
        let start = usize::try_from(action.ref_start).map_err(|_| ())?;
        let end = start.checked_add(usize::from(action.ref_len)).ok_or(())?;
        if start != ref_cursor
            || end > action_refs.len()
            || action_refs[start..end].iter().any(|reference| {
                usize::try_from(reference.action_index).ok() != Some(action_index)
                    || !FLAT_ACTION_REF_INTERNAL_TO_PROJECTION_V2
                        .contains(&reference.projection_role_id)
                    || reference.card_token == 0
                    || reference.card_token > 65_536
                    || usize::try_from(reference.model_object_index)
                        .ok()
                        .and_then(|index| objects.get(index))
                        .is_none_or(|object| object.card_token != reference.card_token)
            })
        {
            return Err(());
        }
        ref_cursor = end;
    }
    (ref_cursor == action_refs.len()).then_some(()).ok_or(())
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FlatScoredFamilyV2;

impl FlatScoredFamilyCore for FlatScoredFamilyV2 {
    type Encoder = FlatDecisionEncoderV2;
    type OwnedPacket = OwnedFlatScoringDecisionV2;
    type ValidatedPacket = ValidatedOwnedFlatScoringDecisionV2;
    type Binding = FlatDecisionBindingV2;
    type Contract = FlatScorerContractV2;
    type Decision = FlatDecisionV2;
    type DecisionView<'a> = FlatScoringDecisionViewV2<'a>;

    const WORKER_NAME: &'static str = "mtg-async-flat-scored-v2";
    const MEMBERSHIP_DIGEST_DOMAIN: &'static [u8] = ASYNC_FLAT_SCORED_MEMBERSHIP_DIGEST_DOMAIN_V2;

    fn reset_session(
        config: &AsyncRolloutConfigV2,
        episode_id: u64,
        environment_seed: u64,
    ) -> Result<FastActorSessionV1, ()> {
        FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
            episode_id,
            environment_seed,
            config.max_physical_decisions,
            config.max_policy_steps,
            config.deck_ids.clone(),
        )
        .map_err(|_| ())
    }

    fn encode_packet(
        session: &FastActorSessionV1,
        expected: FastActorDecisionV1,
        encoder: &mut Self::Encoder,
        mut packet: Self::OwnedPacket,
    ) -> Result<Self::ValidatedPacket, ()> {
        let decision = session
            .encode_current_flat_scoring_decision_owned_v2(
                expected,
                encoder,
                &mut FlatScoringOwnedBuffersV2 {
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
            )
            .map_err(|_| ())?;
        packet.decision = decision;
        validate_packet(&packet)?;
        Ok(ValidatedOwnedFlatScoringDecisionV2(packet))
    }

    fn packet_contract(packet: &Self::ValidatedPacket) -> Self::Contract {
        packet.0.scorer_contract()
    }

    fn packet_binding(packet: &Self::ValidatedPacket) -> Self::Binding {
        packet.0.decision.binding
    }

    fn packet_decision(packet: &Self::ValidatedPacket) -> Self::Decision {
        packet.0.decision
    }

    fn packet_view(packet: &Self::ValidatedPacket) -> Self::DecisionView<'_> {
        packet.0.scorer_view()
    }

    fn packet_action_count(packet: &Self::ValidatedPacket) -> u32 {
        packet.0.decision.active_action_count
    }

    fn into_owned_packet(packet: Self::ValidatedPacket) -> Self::OwnedPacket {
        packet.0
    }

    fn expected_matches_binding(expected: FastActorDecisionV1, decision: Self::Decision) -> bool {
        let binding = decision.binding.action_binding;
        binding.episode_id == expected.episode_id
            && binding.environment_revision == expected.environment_revision
            && binding.bound_policy_step_count == expected.step
            && binding.physical_decision_id == expected.physical_decision_id
            && binding.bound_physical_decision_count == expected.physical_decision_id
            && binding.substep_index == expected.substep_index
            && binding.substep_count == expected.substep_count
            && binding.acting_player == player_seat_code(expected.acting_player)
            && binding.decision_kind == decision_kind_code(expected.decision_kind)
            && binding.legal_action_count == expected.legal_action_count
            && decision.active_action_count == expected.legal_action_count
    }

    #[cfg(test)]
    fn test_safe_packet_payload(packet: &Self::ValidatedPacket) -> String {
        let view = packet.0.scorer_view();
        format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            view.globals(),
            view.objects(),
            view.relations(),
            view.object_subtypes(),
            view.ability_uses(),
            view.goads(),
            view.completed_dungeons(),
            view.effect_subtype_changes(),
            view.context_path_elements(),
            view.actions(),
            view.action_refs(),
        )
    }

    fn consume(
        session: &mut FastActorSessionV1,
        binding: Self::Binding,
        selected_index: u32,
    ) -> Result<FastActorResponseV1, ()> {
        session
            .consume_current_flat_action_slice_v2(binding.action_binding, selected_index)
            .map_err(|_| ())
    }

    fn native_full_trajectory_commitment(binding: Self::Binding) -> Result<[u8; 16], ()> {
        Ok(binding.action_binding.candidate_order_commitment)
    }

    fn native_full_trajectory_opponent_commitment(
        session: &FastActorSessionV1,
        expected: FastActorDecisionV1,
    ) -> Result<[u8; 16], ()> {
        session
            .native_full_trajectory_current_binding_v2(expected)
            .map(|binding| binding.candidate_order_commitment)
            .map_err(|_| ())
    }
}

fn player_seat_code(seat: crate::rl::PlayerSeatV1) -> u8 {
    match seat {
        crate::rl::PlayerSeatV1::P0 => 0,
        crate::rl::PlayerSeatV1::P1 => 1,
    }
}

fn decision_kind_code(kind: crate::rl_session::FastActorDecisionKindV1) -> u8 {
    match kind {
        crate::rl_session::FastActorDecisionKindV1::Surface => 0,
        crate::rl_session::FastActorDecisionKindV1::AttackerInclusion => 1,
        crate::rl_session::FastActorDecisionKindV1::BlockerInclusion => 2,
    }
}

struct FlatBatchScorerAdapterV2<'a, S: FlatBatchScorerV2>(&'a mut S);

impl<S: FlatBatchScorerV2> FlatBatchScorerCore<FlatScoredFamilyV2>
    for FlatBatchScorerAdapterV2<'_, S>
{
    fn score_batch_core(
        &mut self,
        contract: FlatScorerContractV2,
        decisions: &[RoundDecisionCore<FlatScoredFamilyV2>],
        action_offsets: &[usize],
        action_logits: &mut [f32],
        values: &mut [f32],
    ) -> Result<(), FlatBatchScorerErrorV1> {
        self.0
            .score_batch_v2(
                &FlatScoringBatchViewV2 {
                    contract,
                    decisions,
                    action_offsets,
                },
                action_logits,
                values,
            )
            .map_err(|error| FlatBatchScorerErrorV1::new(error.code))
    }
}

struct FlatScoredTrajectoryObserverAdapterV2<O: FlatScoredTrajectoryObserverV2>(O);

impl<O: FlatScoredTrajectoryObserverV2> FlatScoredTrajectoryObserverCore<FlatScoredFamilyV2>
    for FlatScoredTrajectoryObserverAdapterV2<O>
{
    type Error = O::Error;
    type Output = O::Output;

    const OBSERVES_TRAJECTORY: bool = O::OBSERVES_TRAJECTORY;

    fn observe_selected_core(
        &mut self,
        event: FlatScoredSelectedEventCore<'_, FlatScoredFamilyV2>,
    ) -> Result<(), Self::Error> {
        self.0.observe_selected_v2(FlatScoredSelectedEventV2 {
            expected: event.expected,
            binding: event.binding,
            learner_ordinal: event.learner_ordinal,
            action_seed: event.action_seed,
            selected_index: event.selected_index,
            raw_action_logits: event.raw_action_logits,
            predicted_value_bits: event.predicted_value_bits,
            decision: event.decision,
        })
    }

    fn observe_terminal_core(
        &mut self,
        event: FlatScoredTerminalEventV1,
    ) -> Result<(), Self::Error> {
        self.0.observe_terminal_v2(FlatScoredTerminalEventV2 {
            terminal: event.terminal,
            learner_action_count: event.learner_action_count,
            learner_trace_hash: event.learner_trace_hash,
            native_full_trajectory_receipt: event.native_full_trajectory_receipt,
        })
    }

    fn finish_core(self) -> Result<Self::Output, Self::Error> {
        self.0.finish_v2()
    }
}

struct NoopFlatScoredTrajectoryObserverV2;

impl FlatScoredTrajectoryObserverV2 for NoopFlatScoredTrajectoryObserverV2 {
    type Error = std::convert::Infallible;
    type Output = ();

    const OBSERVES_TRAJECTORY: bool = false;

    fn observe_selected_v2(
        &mut self,
        _event: FlatScoredSelectedEventV2<'_>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn observe_terminal_v2(
        &mut self,
        _event: FlatScoredTerminalEventV2,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn finish_v2(self) -> Result<Self::Output, Self::Error> {
        Ok(())
    }
}

fn observer_phase_v2(phase: FlatScoredObserverPhaseV1) -> FlatScoredObserverPhaseV2 {
    match phase {
        FlatScoredObserverPhaseV1::Selected => FlatScoredObserverPhaseV2::Selected,
        FlatScoredObserverPhaseV1::Terminal => FlatScoredObserverPhaseV2::Terminal,
        FlatScoredObserverPhaseV1::Finish => FlatScoredObserverPhaseV2::Finish,
    }
}

pub fn run_async_flat_scored_rollout_v2(
    config: AsyncRolloutConfigV2,
    scorer: &mut impl FlatBatchScorerV2,
) -> Result<AsyncFlatScoredRolloutResultV2, AsyncFlatScoredRolloutErrorV2> {
    match run_async_flat_scored_rollout_observed_v2(
        config,
        scorer,
        NoopFlatScoredTrajectoryObserverV2,
    ) {
        Ok((result, ())) => Ok(result),
        Err(AsyncFlatScoredObservedRunErrorV2::Rollout(error)) => Err(error),
        Err(AsyncFlatScoredObservedRunErrorV2::ObserverFailed { phase, error }) => {
            let _ = phase;
            match error {}
        }
        Err(AsyncFlatScoredObservedRunErrorV2::ObserverPanicked { phase }) => {
            let _ = phase;
            Err(AsyncFlatScoredRolloutErrorV2::BrokerProtocolViolation)
        }
    }
}

pub(crate) fn run_async_flat_scored_rollout_observed_v2<O: FlatScoredTrajectoryObserverV2>(
    config: AsyncRolloutConfigV2,
    scorer: &mut impl FlatBatchScorerV2,
    observer: O,
) -> Result<(AsyncFlatScoredRolloutResultV2, O::Output), AsyncFlatScoredObservedRunErrorV2<O::Error>>
{
    run_async_flat_scored_rollout_observed_with_schedule_v2(
        config,
        FlatScoredExecutionScheduleV1::Legacy,
        scorer,
        observer,
    )
}

/// Internal native-trainer execution path. It deliberately accepts no public
/// seed/config schema: the caller supplies the one frozen trainer base seed,
/// while public V2 continues to use its legacy fixed-seat three-seed schedule.
#[allow(dead_code)]
pub(crate) fn run_async_flat_scored_rollout_native_observed_v2<
    O: FlatScoredTrajectoryObserverV2,
>(
    config: AsyncRolloutConfigV2,
    base_seed: u64,
    scorer: &mut impl FlatBatchScorerV2,
    observer: O,
) -> Result<(AsyncFlatScoredRolloutResultV2, O::Output), AsyncFlatScoredObservedRunErrorV2<O::Error>>
{
    run_async_flat_scored_rollout_observed_with_schedule_v2(
        config,
        FlatScoredExecutionScheduleV1::NativeTrainerV1 { base_seed },
        scorer,
        observer,
    )
}

fn run_async_flat_scored_rollout_observed_with_schedule_v2<O: FlatScoredTrajectoryObserverV2>(
    config: AsyncRolloutConfigV2,
    execution_schedule: FlatScoredExecutionScheduleV1,
    scorer: &mut impl FlatBatchScorerV2,
    observer: O,
) -> Result<(AsyncFlatScoredRolloutResultV2, O::Output), AsyncFlatScoredObservedRunErrorV2<O::Error>>
{
    let mut scorer = FlatBatchScorerAdapterV2(scorer);
    let observer = FlatScoredTrajectoryObserverAdapterV2(observer);
    match run_async_flat_scored_rollout_core::<FlatScoredFamilyV2, _, _>(
        config,
        execution_schedule,
        &mut scorer,
        observer,
    ) {
        Ok((result, output)) => Ok((result.into(), output)),
        Err(AsyncFlatScoredObservedRunErrorV1::Rollout(error)) => {
            Err(AsyncFlatScoredObservedRunErrorV2::Rollout(error.into()))
        }
        Err(AsyncFlatScoredObservedRunErrorV1::ObserverFailed { phase, error }) => {
            Err(AsyncFlatScoredObservedRunErrorV2::ObserverFailed {
                phase: observer_phase_v2(phase),
                error,
            })
        }
        Err(AsyncFlatScoredObservedRunErrorV1::ObserverPanicked { phase }) => {
            Err(AsyncFlatScoredObservedRunErrorV2::ObserverPanicked {
                phase: observer_phase_v2(phase),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_flat_scored_rollout_v1::{
        run_async_flat_scored_rollout_observed_v1, FlatBatchScorerErrorV1, FlatBatchScorerV1,
        FlatScoredSelectedEventV1, FlatScoredTerminalEventV1, FlatScoredTrajectoryObserverV1,
        FlatScoringBatchViewV1,
    };
    use crate::native_trainer_schedule_v1::derive_native_trainer_learner_action_seed_v1;
    use crate::private_physical_trajectory_core::FlatPhysicalLearnerSeatRuleCore;
    use crate::private_physical_trajectory_v2::NativeFlatPhysicalTrajectoryObserverV2;
    use crate::rl::PlayerSeatV1;
    use sha2::{Digest, Sha256};
    use std::any::TypeId;
    use std::convert::Infallible;
    use std::time::Duration;

    fn minimal_valid_packet() -> OwnedFlatScoringDecisionV2 {
        let mut packet = OwnedFlatScoringDecisionV2::default();
        packet.decision.active_action_count = 1;
        packet.decision.binding.action_binding = FlatActionDecisionBindingV2 {
            slice_version: FLAT_ACTION_DECISION_SLICE_VERSION_V2,
            ref_role_mapping_version: FLAT_ACTION_REF_ROLE_MAPPING_VERSION_V2,
            card_token_mapping_version: FLAT_ACTION_CARD_TOKEN_MAPPING_VERSION_V2,
            candidate_commitment_version: FLAT_ACTION_CANDIDATE_COMMITMENT_VERSION_V2,
            legal_action_count: 1,
            ..FlatActionDecisionBindingV2::default()
        };
        packet.decision.binding.typed_layout_version = FLAT_POLICY_TYPED_LAYOUT_VERSION_V2;
        packet.decision.binding.feature_inventory_version =
            FLAT_POLICY_FEATURE_INVENTORY_VERSION_V2;
        packet.decision.binding.enum_mapping_version = FLAT_POLICY_ENUM_MAPPING_VERSION_V2;
        packet.decision.binding.object_group_mapping_version =
            FLAT_POLICY_OBJECT_GROUP_MAPPING_VERSION_V2;
        packet.decision.binding.relation_role_mapping_version =
            FLAT_POLICY_RELATION_ROLE_MAPPING_VERSION_V2;
        packet.decision.binding.context_subrole_mapping_version =
            FLAT_POLICY_CONTEXT_SUBROLE_MAPPING_VERSION_V2;
        packet
            .decision
            .binding
            .action_ref_projection_role_mapping_version =
            FLAT_ACTION_REF_PROJECTION_ROLE_MAPPING_VERSION_V2;
        packet.decision.binding.contract_digests = FLAT_POLICY_CONTRACT_DIGESTS_V2;
        packet.actions.push(FlatScorerActionCoreV2::default());
        packet
    }

    fn packet_with_single_action_ref_token(card_token: u32) -> OwnedFlatScoringDecisionV2 {
        let mut packet = minimal_valid_packet();
        packet.objects.push(FlatObjectCoreV2 {
            card_token,
            ..FlatObjectCoreV2::default()
        });
        packet.scorer_action_refs.push(FlatScorerActionRefV2 {
            action_index: 0,
            projection_role_id: crate::flat_policy_v2::flat_action_ref_projection_role_id_v2(
                crate::rl_session::FlatActionRefRoleV1::Source,
            ),
            card_token,
            model_object_index: 0,
            ..FlatScorerActionRefV2::default()
        });
        packet.actions[0].ref_len = 1;
        packet.decision.active_object_count = 1;
        packet.decision.active_action_ref_count = 1;
        packet
    }

    fn config(episode_count: u64) -> AsyncRolloutConfigV2 {
        AsyncRolloutConfigV2 {
            deck_ids: ["Rally".to_string(), "Rally".to_string()],
            learner_seat: PlayerSeatV1::P0,
            environment_seed: 91_501,
            opponent_policy_seed: 92_501,
            learner_policy_seed: 93_501,
            max_physical_decisions: 5_000,
            max_policy_steps: 640_000,
            worker_count: 1,
            sessions_per_worker: 1,
            broker_batch_target: 1,
            first_episode_id: 0,
            episode_count,
            scheduler_timeout: Duration::from_secs(60),
            measure_broker_service_time: false,
        }
    }

    #[test]
    fn blocked_orders_must_be_unique_and_contiguous() {
        let mut packet = minimal_valid_packet();
        assert!(validate_packet(&packet).is_ok());

        packet.relations = vec![
            FlatRelationV2 {
                payload: FlatRelationPayloadV2::CombatAttacker {
                    blocked_order: Some(0),
                },
                ..FlatRelationV2::default()
            },
            FlatRelationV2 {
                payload: FlatRelationPayloadV2::CombatAttacker {
                    blocked_order: Some(0),
                },
                ..FlatRelationV2::default()
            },
        ];
        packet.decision.active_relation_count = 2;
        assert!(validate_packet(&packet).is_err());
        if let FlatRelationPayloadV2::CombatAttacker { blocked_order } =
            &mut packet.relations[1].payload
        {
            *blocked_order = Some(1);
        }
        assert!(validate_packet(&packet).is_ok());
    }

    #[test]
    fn packet_validation_rejects_padding_and_above_max_card_tokens() {
        assert!(validate_packet(&packet_with_single_action_ref_token(1)).is_ok());
        assert!(validate_packet(&packet_with_single_action_ref_token(65_536)).is_ok());
        assert!(validate_packet(&packet_with_single_action_ref_token(0)).is_err());
        assert!(validate_packet(&packet_with_single_action_ref_token(65_537)).is_err());
    }

    #[test]
    fn stale_and_cross_family_contract_versions_fail_closed() {
        let contract = expected_scorer_contract(7);
        assert_eq!(
            contract.scorer_packet_version,
            FLAT_SCORER_PACKET_VERSION_V2
        );
        assert_eq!(
            contract.action_slice_version,
            FLAT_ACTION_DECISION_SLICE_VERSION_V2
        );
        assert_ne!(contract.scorer_packet_version, 1);
        assert_ne!(contract.action_slice_version, 1);
        assert_ne!(
            TypeId::of::<FlatActionDecisionBindingV2>(),
            TypeId::of::<crate::rl_session::FlatActionDecisionBindingV1>()
        );

        let mut packet = minimal_valid_packet();
        assert!(validate_packet(&packet).is_ok());
        packet.decision.binding.typed_layout_version = 1;
        assert!(validate_packet(&packet).is_err());

        let mut packet = minimal_valid_packet();
        packet.decision.binding.action_binding.slice_version = 1;
        assert!(validate_packet(&packet).is_err());

        let mut packet = minimal_valid_packet();
        packet.decision.binding.contract_digests = FlatPolicyContractDigestsV2::default();
        assert!(validate_packet(&packet).is_err());
    }

    #[test]
    fn poisoned_owned_buffer_tails_are_dropped_before_reuse() {
        let shaped = config(1);
        let session = FlatScoredFamilyV2::reset_session(
            &shaped,
            0,
            crate::rl::derive_env_seed(shaped.environment_seed, 0),
        )
        .unwrap();
        let FastActorResponseV1::Decision(expected) = session.current_response() else {
            panic!("expected live V2 decision");
        };
        let mut encoder = FlatDecisionEncoderV2::default();
        let first = FlatScoredFamilyV2::encode_packet(
            &session,
            expected,
            &mut encoder,
            OwnedFlatScoringDecisionV2::default(),
        )
        .unwrap();
        let mut poisoned = FlatScoredFamilyV2::into_owned_packet(first);
        poisoned.objects.push(FlatObjectCoreV2 {
            card_token: u32::MAX,
            ..FlatObjectCoreV2::default()
        });
        poisoned.relations.push(FlatRelationV2 {
            payload: FlatRelationPayloadV2::CombatAttacker {
                blocked_order: Some(u32::MAX),
            },
            ..FlatRelationV2::default()
        });
        poisoned.scorer_action_refs.push(FlatScorerActionRefV2 {
            card_token: u32::MAX,
            ..FlatScorerActionRefV2::default()
        });

        let second =
            FlatScoredFamilyV2::encode_packet(&session, expected, &mut encoder, poisoned).unwrap();
        assert!(validate_packet(&second.0).is_ok());
        assert!(second
            .0
            .objects
            .iter()
            .all(|row| row.card_token != u32::MAX));
        assert!(second.0.relations.iter().all(|row| row.payload
            != (FlatRelationPayloadV2::CombatAttacker {
                blocked_order: Some(u32::MAX),
            })));
        assert!(second
            .0
            .scorer_action_refs
            .iter()
            .all(|row| row.card_token != u32::MAX));
    }

    #[derive(Default)]
    struct ContractCheckingScorerV2 {
        calls: u64,
        saw_decision: bool,
    }

    impl FlatBatchScorerV2 for ContractCheckingScorerV2 {
        fn score_batch_v2(
            &mut self,
            batch: &FlatScoringBatchViewV2<'_>,
            action_logits: &mut [f32],
            values: &mut [f32],
        ) -> Result<(), FlatBatchScorerErrorV2> {
            let contract = batch.contract();
            assert_eq!(contract, expected_scorer_contract(contract.card_db_hash));
            assert_eq!(values.len(), batch.decision_count());
            assert_eq!(action_logits.len(), batch.total_action_count());
            for index in 0..batch.decision_count() {
                let decision = batch.decision(index).unwrap();
                assert_eq!(
                    decision.actions().len(),
                    batch.action_offsets()[index + 1] - batch.action_offsets()[index]
                );
                self.saw_decision = true;
            }
            action_logits.fill(0.0);
            values.fill(0.0);
            self.calls += 1;
            Ok(())
        }
    }

    #[derive(Default)]
    struct UniformScorerV1;

    impl FlatBatchScorerV1 for UniformScorerV1 {
        fn score_batch_v1(
            &mut self,
            batch: &FlatScoringBatchViewV1<'_>,
            action_logits: &mut [f32],
            values: &mut [f32],
        ) -> Result<(), FlatBatchScorerErrorV1> {
            assert_eq!(action_logits.len(), batch.total_action_count());
            assert_eq!(values.len(), batch.decision_count());
            action_logits.fill(0.0);
            values.fill(0.0);
            Ok(())
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct UniformTrajectoryRow {
        episode_id: u64,
        step: u64,
        physical_decision_id: u64,
        substep_index: u32,
        learner_ordinal: u64,
        action_seed: u64,
        selected_index: u32,
    }

    impl UniformTrajectoryRow {
        fn from_selected(
            expected: FastActorDecisionV1,
            learner_ordinal: u64,
            action_seed: u64,
            selected_index: u32,
        ) -> Self {
            Self {
                episode_id: expected.episode_id,
                step: expected.step,
                physical_decision_id: expected.physical_decision_id,
                substep_index: expected.substep_index,
                learner_ordinal,
                action_seed,
                selected_index,
            }
        }
    }

    #[derive(Default)]
    struct UniformTrajectoryObserverV1(Vec<UniformTrajectoryRow>);

    impl FlatScoredTrajectoryObserverV1 for UniformTrajectoryObserverV1 {
        type Error = Infallible;
        type Output = Vec<UniformTrajectoryRow>;

        fn observe_selected_v1(
            &mut self,
            event: FlatScoredSelectedEventV1<'_>,
        ) -> Result<(), Self::Error> {
            self.0.push(UniformTrajectoryRow::from_selected(
                event.expected,
                event.learner_ordinal,
                event.action_seed,
                event.selected_index,
            ));
            Ok(())
        }

        fn observe_terminal_v1(
            &mut self,
            _event: FlatScoredTerminalEventV1,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        fn finish_v1(self) -> Result<Self::Output, Self::Error> {
            Ok(self.0)
        }
    }

    #[derive(Default)]
    struct UniformTrajectoryObserverV2(Vec<UniformTrajectoryRow>);

    impl FlatScoredTrajectoryObserverV2 for UniformTrajectoryObserverV2 {
        type Error = Infallible;
        type Output = Vec<UniformTrajectoryRow>;

        fn observe_selected_v2(
            &mut self,
            event: FlatScoredSelectedEventV2<'_>,
        ) -> Result<(), Self::Error> {
            self.0.push(UniformTrajectoryRow::from_selected(
                event.expected,
                event.learner_ordinal,
                event.action_seed,
                event.selected_index,
            ));
            Ok(())
        }

        fn observe_terminal_v2(
            &mut self,
            _event: FlatScoredTerminalEventV2,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        fn finish_v2(self) -> Result<Self::Output, Self::Error> {
            Ok(self.0)
        }
    }

    fn canonical_uniform_trajectory_fold(rows: &[UniformTrajectoryRow]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"mtg-kernel-test-uniform-trajectory-v1\0");
        hasher.update((rows.len() as u64).to_le_bytes());
        for row in rows {
            hasher.update(row.episode_id.to_le_bytes());
            hasher.update(row.step.to_le_bytes());
            hasher.update(row.physical_decision_id.to_le_bytes());
            hasher.update(row.substep_index.to_le_bytes());
            hasher.update(row.learner_ordinal.to_le_bytes());
            hasher.update(row.action_seed.to_le_bytes());
            hasher.update(row.selected_index.to_le_bytes());
        }
        hasher.finalize().into()
    }

    #[test]
    fn actual_v2_rollout_delivers_only_v2_contract_batches() {
        let mut scorer = ContractCheckingScorerV2::default();
        let result = run_async_flat_scored_rollout_v2(config(1), &mut scorer).unwrap();
        assert_eq!(result.episodes.len(), 1);
        assert!(result.all_natural());
        assert!(scorer.calls > 0);
        assert!(scorer.saw_decision);
        assert_eq!(
            result.metrics.scored_decision_count,
            result.metrics.sampled_action_count
        );
    }

    #[test]
    fn v1_and_v2_uniform_logits_preserve_bounded_trajectory_semantics() {
        let mut shaped = config(4);
        shaped.worker_count = 2;
        shaped.sessions_per_worker = 2;
        shaped.broker_batch_target = 3;
        let mut v1_scorer = UniformScorerV1;
        let mut v2_scorer = ContractCheckingScorerV2::default();

        let (v1, v1_rows) = run_async_flat_scored_rollout_observed_v1(
            shaped.clone(),
            &mut v1_scorer,
            UniformTrajectoryObserverV1::default(),
        )
        .unwrap();
        let (v2, v2_rows) = run_async_flat_scored_rollout_observed_v2(
            shaped,
            &mut v2_scorer,
            UniformTrajectoryObserverV2::default(),
        )
        .unwrap();
        assert!(v1.all_natural());
        assert!(v2.all_natural());
        assert!(!v1_rows.is_empty());
        assert_eq!(v2_rows, v1_rows);
        assert_eq!(
            canonical_uniform_trajectory_fold(&v2_rows),
            canonical_uniform_trajectory_fold(&v1_rows)
        );
        // Episode records bind terminal summaries, learner selected-action
        // traces, and per-episode action counts. Exact observer-row equality
        // independently binds every learner seed and selected action.
        assert_eq!(v2.episodes, v1.episodes);
        assert_eq!(v2.policy_step_count, v1.policy_step_count);
        assert_eq!(v2.physical_decision_count, v1.physical_decision_count);
        assert_ne!(v1.metrics.batch_membership_digest, [0; 32]);
        assert_ne!(v2.metrics.batch_membership_digest, [0; 32]);
        // The production membership folds deliberately use distinct V1/V2
        // domains, so equal semantic trajectories must remain version-separated.
        assert_ne!(
            v2.metrics.batch_membership_digest,
            v1.metrics.batch_membership_digest
        );
        assert_eq!(
            v2.metrics.scored_decision_count,
            v1.metrics.scored_decision_count
        );
        assert_eq!(
            v2.metrics.sampled_action_count,
            v1.metrics.sampled_action_count
        );
        assert_eq!(
            v2.metrics.terminal_notification_count,
            v1.metrics.terminal_notification_count
        );
    }

    #[test]
    fn native_schedule_processes_episode_parity_and_group_widths_topology_free() {
        const BASE_SEED: u64 = 71_501;
        let shapes = [(1, 1, 1), (1, 2, 2)];
        let mut reference = None;
        let mut exercised_width_invariance = false;
        for (workers, sessions, target) in shapes {
            let mut shaped = config(2);
            shaped.worker_count = workers;
            shaped.sessions_per_worker = sessions;
            shaped.broker_batch_target = target;
            let observer = NativeFlatPhysicalTrajectoryObserverV2::new(
                shaped.first_episode_id,
                shaped.episode_count,
            )
            .unwrap();
            let mut scorer = ContractCheckingScorerV2::default();
            let (result, batch) = run_async_flat_scored_rollout_native_observed_v2(
                shaped,
                BASE_SEED,
                &mut scorer,
                observer,
            )
            .unwrap();
            assert!(result.all_natural());
            assert_eq!(
                batch.learner_seat_rule,
                FlatPhysicalLearnerSeatRuleCore::EpisodeParity
            );
            assert_eq!(batch.episodes.len(), 2);
            assert_eq!(batch.episodes[0].learner_seat, PlayerSeatV1::P0);
            assert_eq!(batch.episodes[1].learner_seat, PlayerSeatV1::P1);

            for episode in &batch.episodes {
                let mut preceding_policy_width = 0u64;
                for (group_index, group) in episode.groups.iter().enumerate() {
                    let group_ordinal = u64::try_from(group_index).unwrap();
                    for substep in &group.substeps {
                        assert_eq!(
                            substep.action_seed,
                            derive_native_trainer_learner_action_seed_v1(
                                BASE_SEED,
                                episode.episode_id,
                                group_ordinal,
                                substep.expected.substep_index,
                            )
                            .unwrap()
                        );
                    }
                    if preceding_policy_width != group_ordinal {
                        let first = group.substeps.first().unwrap();
                        let wrong_width_coupled_seed =
                            derive_native_trainer_learner_action_seed_v1(
                                BASE_SEED,
                                episode.episode_id,
                                preceding_policy_width,
                                0,
                            )
                            .unwrap();
                        assert_ne!(first.action_seed, wrong_width_coupled_seed);
                        exercised_width_invariance = true;
                    }
                    preceding_policy_width = preceding_policy_width
                        .checked_add(u64::from(group.substep_count))
                        .unwrap();
                }
            }
            match &reference {
                Some(expected) => assert_eq!(&batch, expected),
                None => reference = Some(batch),
            }
        }
        assert!(
            exercised_width_invariance,
            "real Rally processing must include a later learner group after a multi-substep group"
        );
    }

    #[test]
    fn native_schedule_discards_a_policy_cap_terminal_without_observer_output() {
        let mut shaped = config(1);
        shaped.max_policy_steps = 1;
        let observer = NativeFlatPhysicalTrajectoryObserverV2::new(
            shaped.first_episode_id,
            shaped.episode_count,
        )
        .unwrap();
        let mut scorer = ContractCheckingScorerV2::default();
        let error =
            run_async_flat_scored_rollout_native_observed_v2(shaped, 71_501, &mut scorer, observer)
                .unwrap_err();
        assert!(matches!(
            error,
            AsyncFlatScoredObservedRunErrorV2::Rollout(
                AsyncFlatScoredRolloutErrorV2::WorkerFailed {
                    phase: AsyncFlatScoredWorkerPhaseV2::Protocol,
                    ..
                }
            )
        ));
    }
}
