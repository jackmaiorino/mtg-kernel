//! Real in-memory native trainer integration.
//!
//! The scorer tensorizes each production V2 learner decision exactly once,
//! evaluates the live native model, and atomically stages the owned thirteen
//! tensors beside the exact packet binding and output bits.  The trajectory
//! observer consumes that association before grouping, so training never
//! reconstructs from copied raw scorer tables.  A complete two-episode
//! alternating-seat rollout and grouped Adam step are prepared on private
//! candidates; the live trainer changes only after every cross-check passes.
//!
//! This module deliberately owns no persisted schema, checkpoint writer, CLI,
//! sampler identity, seed identity, schedule identity, loss identity, or gauge
//! identity.  Those frozen contracts are consumed unchanged.

use crate::async_flat_scored_rollout_v2::{
    expected_scorer_contract, run_async_flat_scored_rollout_native_observed_v2,
    AsyncFlatScoredObservedRunErrorV2, AsyncFlatScoredRolloutErrorV2,
    AsyncFlatScoredRolloutMetricsV2, FlatBatchScorerErrorV2, FlatBatchScorerV2,
    FlatScoredObserverPhaseV2, FlatScoredSelectedEventV2, FlatScoredTerminalEventV2,
    FlatScoredTrajectoryObserverV2, FlatScoringBatchViewV2,
};
use crate::async_rollout_v2::AsyncRolloutConfigV2;
use crate::flat_policy_v2::FlatDecisionBindingV2;
use crate::native_flat_tensorizer_v2::{
    NativeFlatDecisionTensorV2, NativeFlatTensorErrorV2, NativeFlatTensorizerV2,
    NATIVE_FLAT_ACTION_FEATURE_DIM_V2,
};
use crate::native_full_episode_trajectory_v1::NativeFullEpisodeTrajectoryReceiptV1;
use crate::native_policy_train_step_v1::{
    NativePolicyPhysicalDecisionV1, NativePolicySubstepV1, NativePolicyTrainErrorV1,
    NativePolicyValueTrainStateV1, NativeScorerBiasGaugeRecordV1,
};
use crate::native_policy_value_net_v1::{
    NativeEncodedDecisionSchemaV1, NativeEncodedDecisionViewV1, NativeNamedParameterV1,
    NativePolicyValueErrorV1, NativePolicyValueNetV1,
};
use crate::native_trainer_schedule_v1::{
    native_trainer_episode_schedule_v1, NativeTrainerScheduleErrorV1,
};
use crate::private_physical_trajectory_core::{
    FlatGroupedTrajectoryBatchCore, FlatPhysicalLearnerSeatRuleCore,
    FlatPhysicalTrajectoryObserverCore, FlatPhysicalUpdateStagingCore, FlatSelectedSampleCore,
    FlatTerminalSampleCore,
};
use crate::private_physical_trajectory_v2::{
    selected_binding_matches, FlatPhysicalTrajectoryErrorV2,
};
use crate::rl::{PlayerSeatV1, TerminalOutcomeV1};
use crate::rl_session::{SessionDeckHashesV1, SessionDeckIdsV1};
use crate::runtime_decks::runtime_deck_by_id;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

const NATIVE_POLICY_SCORER_CONTRACT_CODE_V1: u32 = 1;
const NATIVE_POLICY_SCORER_OUTPUT_SHAPE_CODE_V1: u32 = 2;
const NATIVE_POLICY_SCORER_DECISION_CODE_V1: u32 = 3;
const NATIVE_POLICY_SCORER_TENSOR_CODE_V1: u32 = 4;
const NATIVE_POLICY_SCORER_MODEL_CODE_V1: u32 = 5;
const NATIVE_POLICY_SCORER_ASSOCIATION_CODE_V1: u32 = 6;
const NATIVE_POLICY_SCORER_COUNTER_CODE_V1: u32 = 7;
const NATIVE_TRAINER_EPISODES_PER_UPDATE_V1: u64 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativePolicyAssociationErrorV1 {
    BorrowConflict,
    AllocationFailed,
    ProducerPoisoned,
    MissingScoredDecision,
    BindingMismatch,
    LogitCountMismatch,
    LogitBitsMismatch,
    ValueBitsMismatch,
    SelectedIndexOutOfRange,
    TensorActionShapeMismatch,
    ResidualScoredDecisions,
}

#[derive(Clone, Debug, PartialEq)]
struct NativeScoredDecisionAssociationV1 {
    binding: FlatDecisionBindingV2,
    tensor: NativeFlatDecisionTensorV2,
    logit_bits: Vec<u32>,
    value_bits: u32,
}

#[derive(Debug, Default)]
struct NativePolicyAssociationStateV1 {
    queue: VecDeque<NativeScoredDecisionAssociationV1>,
    poisoned: Option<NativePolicyAssociationErrorV1>,
    #[cfg(test)]
    pending_test_mutation: Option<NativePolicyAssociationTestMutationV1>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativePolicyAssociationTestMutationV1 {
    Binding,
    SelectedLogit,
    Value,
}

#[derive(Clone, Debug)]
struct NativePolicyAssociationProducerV1 {
    shared: Rc<RefCell<NativePolicyAssociationStateV1>>,
}

#[derive(Clone, Debug)]
struct NativePolicyAssociationConsumerV1 {
    shared: Rc<RefCell<NativePolicyAssociationStateV1>>,
}

fn native_policy_association_channel_v1() -> (
    NativePolicyAssociationProducerV1,
    NativePolicyAssociationConsumerV1,
) {
    let shared = Rc::new(RefCell::new(NativePolicyAssociationStateV1::default()));
    (
        NativePolicyAssociationProducerV1 {
            shared: Rc::clone(&shared),
        },
        NativePolicyAssociationConsumerV1 { shared },
    )
}

impl NativePolicyAssociationProducerV1 {
    fn stage_chunk_v1(
        &self,
        chunk: Vec<NativeScoredDecisionAssociationV1>,
    ) -> Result<(), NativePolicyAssociationErrorV1> {
        let mut shared = self
            .shared
            .try_borrow_mut()
            .map_err(|_| NativePolicyAssociationErrorV1::BorrowConflict)?;
        if shared.poisoned.is_some() {
            return Err(NativePolicyAssociationErrorV1::ProducerPoisoned);
        }

        // Reserve the complete chunk before the first queue mutation. Once
        // this succeeds, VecDeque::extend cannot expose a partial accepted
        // chunk through an allocation failure.
        shared
            .queue
            .try_reserve(chunk.len())
            .map_err(|_| NativePolicyAssociationErrorV1::AllocationFailed)?;
        shared.queue.extend(chunk);
        Ok(())
    }

    #[cfg(test)]
    fn set_test_mutation_v1(
        &self,
        mutation: NativePolicyAssociationTestMutationV1,
    ) -> Result<(), NativePolicyAssociationErrorV1> {
        let mut shared = self
            .shared
            .try_borrow_mut()
            .map_err(|_| NativePolicyAssociationErrorV1::BorrowConflict)?;
        shared.pending_test_mutation = Some(mutation);
        Ok(())
    }
}

impl NativePolicyAssociationConsumerV1 {
    fn pop_verified_v1(
        &self,
        event: &FlatScoredSelectedEventV2<'_>,
    ) -> Result<NativeFlatDecisionTensorV2, NativePolicyAssociationErrorV1> {
        let mut shared = self
            .shared
            .try_borrow_mut()
            .map_err(|_| NativePolicyAssociationErrorV1::BorrowConflict)?;
        if let Some(error) = shared.poisoned {
            return Err(error);
        }
        let staged = match shared.queue.pop_front() {
            Some(staged) => staged,
            None => {
                shared.poisoned = Some(NativePolicyAssociationErrorV1::MissingScoredDecision);
                return Err(NativePolicyAssociationErrorV1::MissingScoredDecision);
            }
        };
        #[cfg(test)]
        let mut staged = staged;

        let selected_index = match usize::try_from(event.selected_index) {
            Ok(index) if index < event.raw_action_logits.len() => index,
            _ => {
                shared.poisoned = Some(NativePolicyAssociationErrorV1::SelectedIndexOutOfRange);
                return Err(NativePolicyAssociationErrorV1::SelectedIndexOutOfRange);
            }
        };
        #[cfg(test)]
        // The corruption hook is private to this module's rollback tests and
        // fires only after an entire scorer chunk has been accepted. Mutating
        // the sampled row here lets the test distinguish the selected-row
        // association check from the full-vector check.
        if let Some(mutation) = shared.pending_test_mutation.take() {
            match mutation {
                NativePolicyAssociationTestMutationV1::Binding => {
                    staged.binding.action_binding.episode_id ^= 1;
                }
                NativePolicyAssociationTestMutationV1::SelectedLogit => {
                    let bits = match staged.logit_bits.get_mut(selected_index) {
                        Some(bits) => bits,
                        None => {
                            shared.poisoned =
                                Some(NativePolicyAssociationErrorV1::LogitCountMismatch);
                            return Err(NativePolicyAssociationErrorV1::LogitCountMismatch);
                        }
                    };
                    *bits ^= 1;
                }
                NativePolicyAssociationTestMutationV1::Value => {
                    staged.value_bits ^= 1;
                }
            }
        }
        let tensor_action_count = staged
            .tensor
            .action_features
            .len()
            .checked_div(NATIVE_FLAT_ACTION_FEATURE_DIM_V2)
            .filter(|_| {
                staged
                    .tensor
                    .action_features
                    .len()
                    .is_multiple_of(NATIVE_FLAT_ACTION_FEATURE_DIM_V2)
            });
        let error = if staged.binding != event.binding {
            Some(NativePolicyAssociationErrorV1::BindingMismatch)
        } else if staged.logit_bits.len() != event.raw_action_logits.len() {
            Some(NativePolicyAssociationErrorV1::LogitCountMismatch)
        } else if tensor_action_count != Some(staged.logit_bits.len()) {
            Some(NativePolicyAssociationErrorV1::TensorActionShapeMismatch)
        } else if staged.logit_bits[selected_index]
            != event.raw_action_logits[selected_index].to_bits()
            || staged
                .logit_bits
                .iter()
                .copied()
                .zip(event.raw_action_logits.iter().map(|value| value.to_bits()))
                .any(|(expected, actual)| expected != actual)
        {
            Some(NativePolicyAssociationErrorV1::LogitBitsMismatch)
        } else if staged.value_bits != event.predicted_value_bits {
            Some(NativePolicyAssociationErrorV1::ValueBitsMismatch)
        } else {
            None
        };
        if let Some(error) = error {
            shared.poisoned = Some(error);
            return Err(error);
        }
        Ok(staged.tensor)
    }

    fn finish_v1(&self) -> Result<(), NativePolicyAssociationErrorV1> {
        let mut shared = self
            .shared
            .try_borrow_mut()
            .map_err(|_| NativePolicyAssociationErrorV1::BorrowConflict)?;
        if let Some(error) = shared.poisoned {
            return Err(error);
        }
        if !shared.queue.is_empty() {
            shared.poisoned = Some(NativePolicyAssociationErrorV1::ResidualScoredDecisions);
            return Err(NativePolicyAssociationErrorV1::ResidualScoredDecisions);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NativePolicyScorerFailureV1 {
    Contract,
    OutputShape,
    MissingDecision,
    Tensor(NativeFlatTensorErrorV2),
    Model(NativePolicyValueErrorV1),
    Association(NativePolicyAssociationErrorV1),
    CounterOverflow,
}

impl NativePolicyScorerFailureV1 {
    const fn code_v1(&self) -> u32 {
        match self {
            Self::Contract => NATIVE_POLICY_SCORER_CONTRACT_CODE_V1,
            Self::OutputShape => NATIVE_POLICY_SCORER_OUTPUT_SHAPE_CODE_V1,
            Self::MissingDecision => NATIVE_POLICY_SCORER_DECISION_CODE_V1,
            Self::Tensor(_) => NATIVE_POLICY_SCORER_TENSOR_CODE_V1,
            Self::Model(_) => NATIVE_POLICY_SCORER_MODEL_CODE_V1,
            Self::Association(_) => NATIVE_POLICY_SCORER_ASSOCIATION_CODE_V1,
            Self::CounterOverflow => NATIVE_POLICY_SCORER_COUNTER_CODE_V1,
        }
    }
}

/// Production V2 scorer backed by the exact native thirteen-tensor encoder and
/// native policy/value network.  It owns no model state; one update borrows the
/// current immutable model until rollout completes.
struct NativePolicyBatchScorerV2<'a> {
    model: &'a NativePolicyValueNetV1,
    tensorizer: NativeFlatTensorizerV2,
    associations: NativePolicyAssociationProducerV1,
    last_failure: Option<NativePolicyScorerFailureV1>,
    accepted_batch_count: u64,
    accepted_decision_count: u64,
}

impl<'a> NativePolicyBatchScorerV2<'a> {
    fn new_v1(
        model: &'a NativePolicyValueNetV1,
        associations: NativePolicyAssociationProducerV1,
    ) -> Self {
        Self {
            model,
            tensorizer: NativeFlatTensorizerV2::new(),
            associations,
            last_failure: None,
            accepted_batch_count: 0,
            accepted_decision_count: 0,
        }
    }

    fn score_chunk_v1(
        &mut self,
        batch: &FlatScoringBatchViewV2<'_>,
        action_logits: &mut [f32],
        values: &mut [f32],
    ) -> Result<(), NativePolicyScorerFailureV1> {
        let contract = batch.contract();
        if contract != expected_scorer_contract(contract.card_db_hash) {
            return Err(NativePolicyScorerFailureV1::Contract);
        }
        if batch.decision_count() == 0
            || values.len() != batch.decision_count()
            || action_logits.len() != batch.total_action_count()
            || action_logits.is_empty()
            || batch.action_offsets().len() != batch.decision_count() + 1
        {
            return Err(NativePolicyScorerFailureV1::OutputShape);
        }
        let next_batch_count = self
            .accepted_batch_count
            .checked_add(1)
            .ok_or(NativePolicyScorerFailureV1::CounterOverflow)?;
        let next_decision_count = self
            .accepted_decision_count
            .checked_add(
                u64::try_from(batch.decision_count())
                    .map_err(|_| NativePolicyScorerFailureV1::CounterOverflow)?,
            )
            .ok_or(NativePolicyScorerFailureV1::CounterOverflow)?;

        let mut candidate_logits = Vec::new();
        candidate_logits
            .try_reserve_exact(action_logits.len())
            .map_err(|_| NativePolicyScorerFailureV1::OutputShape)?;
        let mut candidate_values = Vec::new();
        candidate_values
            .try_reserve_exact(values.len())
            .map_err(|_| NativePolicyScorerFailureV1::OutputShape)?;
        let mut candidate_associations = Vec::new();
        candidate_associations
            .try_reserve_exact(batch.decision_count())
            .map_err(|_| NativePolicyScorerFailureV1::OutputShape)?;

        for decision_index in 0..batch.decision_count() {
            let decision = batch
                .decision(decision_index)
                .ok_or(NativePolicyScorerFailureV1::MissingDecision)?;
            let binding = batch
                .binding(decision_index)
                .ok_or(NativePolicyScorerFailureV1::MissingDecision)?;
            let begin = batch.action_offsets()[decision_index];
            let end = batch.action_offsets()[decision_index + 1];
            if end < begin || end > action_logits.len() || end - begin != decision.actions().len() {
                return Err(NativePolicyScorerFailureV1::OutputShape);
            }

            let mut tensor = NativeFlatDecisionTensorV2::default();
            self.tensorizer
                .fill(decision, &mut tensor)
                .map_err(NativePolicyScorerFailureV1::Tensor)?;
            let output = self
                .model
                .forward_v1(native_encoded_decision_view_v1(&tensor))
                .map_err(NativePolicyScorerFailureV1::Model)?;
            if output.logits.len() != end - begin || !output.value.is_finite() {
                return Err(NativePolicyScorerFailureV1::OutputShape);
            }
            let logit_bits = output
                .logits
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>();
            candidate_logits.extend_from_slice(&output.logits);
            candidate_values.push(output.value);
            candidate_associations.push(NativeScoredDecisionAssociationV1 {
                binding,
                tensor,
                logit_bits,
                value_bits: output.value.to_bits(),
            });
        }
        if candidate_logits.len() != action_logits.len() || candidate_values.len() != values.len() {
            return Err(NativePolicyScorerFailureV1::OutputShape);
        }

        // Queue publication is the only fallible operation after all model
        // outputs exist. It reserves and commits the whole chunk before the
        // infallible caller-slice copies and scorer counters become visible.
        self.associations
            .stage_chunk_v1(candidate_associations)
            .map_err(NativePolicyScorerFailureV1::Association)?;
        action_logits.copy_from_slice(&candidate_logits);
        values.copy_from_slice(&candidate_values);
        self.accepted_batch_count = next_batch_count;
        self.accepted_decision_count = next_decision_count;
        Ok(())
    }
}

impl FlatBatchScorerV2 for NativePolicyBatchScorerV2<'_> {
    fn score_batch_v2(
        &mut self,
        batch: &FlatScoringBatchViewV2<'_>,
        action_logits: &mut [f32],
        values: &mut [f32],
    ) -> Result<(), FlatBatchScorerErrorV2> {
        match self.score_chunk_v1(batch, action_logits, values) {
            Ok(()) => Ok(()),
            Err(error) => {
                let code = error.code_v1();
                if self.last_failure.is_none() {
                    self.last_failure = Some(error);
                }
                Err(FlatBatchScorerErrorV2::new(code))
            }
        }
    }
}

fn native_encoded_decision_view_v1(
    tensor: &NativeFlatDecisionTensorV2,
) -> NativeEncodedDecisionViewV1<'_> {
    NativeEncodedDecisionViewV1::from_slices_unvalidated(
        NativeEncodedDecisionSchemaV1::contract_v1(),
        &tensor.state,
        &tensor.object_features,
        &tensor.object_card_ids,
        &tensor.object_groups,
        &tensor.object_node_ids,
        &tensor.edge_features,
        &tensor.edge_source_indices,
        &tensor.edge_target_indices,
        &tensor.action_features,
        &tensor.action_ref_features,
        &tensor.action_ref_card_ids,
        &tensor.action_ref_action_indices,
        &tensor.action_ref_node_indices,
    )
}

type NativePolicyGroupedTrajectoryV1 =
    FlatGroupedTrajectoryBatchCore<FlatDecisionBindingV2, NativeFlatDecisionTensorV2>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativePolicyTrajectoryErrorV1 {
    Association(NativePolicyAssociationErrorV1),
    Grouping(FlatPhysicalTrajectoryErrorV2),
    FullTrajectoryReceiptInvariant(&'static str),
}

#[derive(Debug)]
struct NativePolicyObservedTrajectoryV1 {
    grouped: NativePolicyGroupedTrajectoryV1,
    full_trajectory_receipts: Vec<NativeFullEpisodeTrajectoryReceiptV1>,
}

#[derive(Debug)]
struct NativePolicyTrajectoryObserverV1 {
    core: FlatPhysicalTrajectoryObserverCore<FlatDecisionBindingV2, NativeFlatDecisionTensorV2>,
    associations: NativePolicyAssociationConsumerV1,
    base_seed: u64,
    expected_deck_hashes: SessionDeckHashesV1,
    full_trajectory_receipts: Vec<NativeFullEpisodeTrajectoryReceiptV1>,
}

impl NativePolicyTrajectoryObserverV1 {
    fn new_v1(
        first_episode_id: u64,
        episode_count: u64,
        base_seed: u64,
        expected_deck_hashes: SessionDeckHashesV1,
        associations: NativePolicyAssociationConsumerV1,
    ) -> Result<Self, NativePolicyTrajectoryErrorV1> {
        let core =
            FlatPhysicalTrajectoryObserverCore::new_episode_parity(first_episode_id, episode_count)
                .map_err(|error| {
                    NativePolicyTrajectoryErrorV1::Grouping(FlatPhysicalTrajectoryErrorV2::from(
                        error,
                    ))
                })?;
        let receipt_capacity = usize::try_from(episode_count).map_err(|_| {
            NativePolicyTrajectoryErrorV1::FullTrajectoryReceiptInvariant(
                "episode count does not fit receipt storage",
            )
        })?;
        Ok(Self {
            core,
            associations,
            base_seed,
            expected_deck_hashes,
            full_trajectory_receipts: Vec::with_capacity(receipt_capacity),
        })
    }
}

impl FlatScoredTrajectoryObserverV2 for NativePolicyTrajectoryObserverV1 {
    type Error = NativePolicyTrajectoryErrorV1;
    type Output = NativePolicyObservedTrajectoryV1;

    fn observe_selected_v2(
        &mut self,
        event: FlatScoredSelectedEventV2<'_>,
    ) -> Result<(), Self::Error> {
        let binding_matches = selected_binding_matches(&event);
        let tensor = self
            .associations
            .pop_verified_v1(&event)
            .map_err(NativePolicyTrajectoryErrorV1::Association)?;
        let scorer_action_count = tensor
            .action_features
            .len()
            .checked_div(NATIVE_FLAT_ACTION_FEATURE_DIM_V2)
            .ok_or(NativePolicyTrajectoryErrorV1::Association(
                NativePolicyAssociationErrorV1::TensorActionShapeMismatch,
            ))?;
        self.core
            .observe_selected(
                FlatSelectedSampleCore {
                    expected: event.expected,
                    binding: event.binding,
                    binding_matches,
                    learner_ordinal: event.learner_ordinal,
                    action_seed: event.action_seed,
                    selected_index: event.selected_index,
                    raw_action_logits: event.raw_action_logits,
                    scorer_action_count,
                    predicted_value_bits: event.predicted_value_bits,
                },
                || tensor,
            )
            .map_err(|error| {
                NativePolicyTrajectoryErrorV1::Grouping(FlatPhysicalTrajectoryErrorV2::from(error))
            })
    }

    fn observe_terminal_v2(&mut self, event: FlatScoredTerminalEventV2) -> Result<(), Self::Error> {
        let receipt = event.native_full_trajectory_receipt.ok_or(
            NativePolicyTrajectoryErrorV1::FullTrajectoryReceiptInvariant(
                "native terminal is missing its full trajectory receipt",
            ),
        )?;
        let expected_schedule =
            native_trainer_episode_schedule_v1(self.base_seed, event.terminal.episode_id).map_err(
                |_| {
                    NativePolicyTrajectoryErrorV1::FullTrajectoryReceiptInvariant(
                        "native terminal schedule provenance cannot be reconstructed",
                    )
                },
            )?;
        if receipt.episode_index != event.terminal.episode_id
            || receipt.environment_seed != expected_schedule.environment_seed
            || receipt.learner_seat != expected_schedule.learner_seat
            || receipt.deck_hashes != self.expected_deck_hashes
            || receipt.policy_step_count != event.terminal.policy_step_count
            || receipt.physical_decision_count != event.terminal.physical_decision_count
            || receipt.learner_policy_step_count != event.learner_action_count
            || self
                .full_trajectory_receipts
                .iter()
                .any(|prior| prior.episode_index == receipt.episode_index)
        {
            return Err(
                NativePolicyTrajectoryErrorV1::FullTrajectoryReceiptInvariant(
                    "native terminal trajectory receipt does not match its terminal",
                ),
            );
        }
        self.core
            .observe_terminal(FlatTerminalSampleCore {
                terminal: event.terminal,
                learner_action_count: event.learner_action_count,
                learner_trace_hash: event.learner_trace_hash,
            })
            .map_err(|error| {
                NativePolicyTrajectoryErrorV1::Grouping(FlatPhysicalTrajectoryErrorV2::from(error))
            })?;
        self.full_trajectory_receipts.push(receipt);
        Ok(())
    }

    fn finish_v2(self) -> Result<Self::Output, Self::Error> {
        let Self {
            core,
            associations,
            base_seed: _,
            expected_deck_hashes: _,
            full_trajectory_receipts,
        } = self;
        associations
            .finish_v1()
            .map_err(NativePolicyTrajectoryErrorV1::Association)?;
        let grouped = core.finish().map_err(|error| {
            NativePolicyTrajectoryErrorV1::Grouping(FlatPhysicalTrajectoryErrorV2::from(error))
        })?;
        validate_full_trajectory_receipts_v1(&grouped, &full_trajectory_receipts)?;
        Ok(NativePolicyObservedTrajectoryV1 {
            grouped,
            full_trajectory_receipts,
        })
    }
}

fn validate_full_trajectory_receipts_v1(
    grouped: &NativePolicyGroupedTrajectoryV1,
    receipts: &[NativeFullEpisodeTrajectoryReceiptV1],
) -> Result<(), NativePolicyTrajectoryErrorV1> {
    if receipts.len() != grouped.episodes.len() {
        return Err(
            NativePolicyTrajectoryErrorV1::FullTrajectoryReceiptInvariant(
                "trajectory receipt count does not match grouped episodes",
            ),
        );
    }
    for episode in &grouped.episodes {
        let mut matches = receipts
            .iter()
            .filter(|receipt| receipt.episode_index == episode.episode_id);
        let receipt = matches.next().ok_or(
            NativePolicyTrajectoryErrorV1::FullTrajectoryReceiptInvariant(
                "grouped episode has no trajectory receipt",
            ),
        )?;
        if matches.next().is_some()
            || receipt.learner_seat != episode.learner_seat
            || receipt.policy_step_count != episode.terminal.policy_step_count
            || receipt.physical_decision_count != episode.terminal.physical_decision_count
            || receipt.learner_policy_step_count != episode.learner_policy_step_count
            || receipt.opponent_policy_step_count != episode.opponent_policy_step_count
            || receipt.learner_physical_decision_count != episode.learner_physical_decision_count
            || receipt.opponent_physical_decision_count != episode.opponent_physical_decision_count
        {
            return Err(
                NativePolicyTrajectoryErrorV1::FullTrajectoryReceiptInvariant(
                    "trajectory receipt counts do not match grouped episode",
                ),
            );
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeTrainerUpdateConfigV1 {
    pub(crate) deck_ids: SessionDeckIdsV1,
    pub(crate) max_physical_decisions: u64,
    pub(crate) max_policy_steps: u64,
    pub(crate) worker_count: usize,
    pub(crate) sessions_per_worker: usize,
    pub(crate) broker_batch_target: usize,
    pub(crate) scheduler_timeout: Duration,
    pub(crate) measure_broker_service_time: bool,
    pub(crate) value_coefficient_bits: u32,
    pub(crate) learning_rate_bits: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeTrainerProgressV1 {
    pub(crate) next_episode_index: u64,
    pub(crate) successful_update_count: u64,
    pub(crate) completed_episode_count: u64,
    pub(crate) learner_physical_decision_count: u64,
    pub(crate) learner_policy_step_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeTrainerEpisodeEvidenceV1 {
    pub(crate) episode_index: u64,
    pub(crate) learner_seat: PlayerSeatV1,
    pub(crate) learner_return: i8,
    pub(crate) learner_group_count: u64,
    pub(crate) learner_policy_step_count: u64,
    pub(crate) learner_trace_hash: u64,
    pub(crate) terminal_outcome: TerminalOutcomeV1,
    /// Full both-actor accepted-action commitment. The legacy learner-only
    /// trace remains diagnostic and is not a persisted trajectory identity.
    pub(crate) full_trajectory_receipt: NativeFullEpisodeTrajectoryReceiptV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeTrainerSelectedOutputEvidenceV1 {
    pub(crate) group_index: usize,
    pub(crate) substep_index: usize,
    pub(crate) selected_action_index: usize,
    pub(crate) selected_logit_bits: u32,
    pub(crate) value_bits: u32,
    pub(crate) selected_log_probability_bits: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeTrainerPhysicalTermEvidenceV1 {
    pub(crate) joint_log_probability_bits: u32,
    pub(crate) value_bits: u32,
    pub(crate) terminal_return: i8,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeTrainerUpdateEvidenceV1 {
    pub(crate) first_episode_index: u64,
    pub(crate) episode_count: u64,
    pub(crate) episodes: [NativeTrainerEpisodeEvidenceV1; 2],
    pub(crate) learner_group_count: u64,
    pub(crate) learner_policy_step_count: u64,
    pub(crate) scorer_accepted_batch_count: u64,
    pub(crate) scorer_accepted_decision_count: u64,
    pub(crate) rollout_metrics: AsyncFlatScoredRolloutMetricsV2,
    pub(crate) model_digest_before: String,
    pub(crate) model_digest_after: String,
    pub(crate) changed_non_gauge_parameter_count: usize,
    pub(crate) policy_sum_bits: u32,
    pub(crate) value_sum_bits: u32,
    pub(crate) loss_bits: u32,
    pub(crate) adam_step_before: u64,
    pub(crate) adam_step_after: u64,
    pub(crate) selected_outputs: Vec<NativeTrainerSelectedOutputEvidenceV1>,
    pub(crate) physical_terms: Vec<NativeTrainerPhysicalTermEvidenceV1>,
    pub(crate) scorer_bias_gauge: NativeScorerBiasGaugeRecordV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NativeTrainerErrorV1 {
    Schedule(NativeTrainerScheduleErrorV1),
    InvalidUpdateConfig(&'static str),
    ObserverConstruction(NativePolicyTrajectoryErrorV1),
    Scorer(NativePolicyScorerFailureV1),
    Rollout(AsyncFlatScoredRolloutErrorV2),
    ObserverFailed {
        phase: FlatScoredObserverPhaseV2,
        error: NativePolicyTrajectoryErrorV1,
    },
    ObserverPanicked {
        phase: FlatScoredObserverPhaseV2,
    },
    GroupingInvariant(&'static str),
    TerminalReturnRange {
        episode_index: u64,
        value: i32,
    },
    Train(NativePolicyTrainErrorV1),
    RecomputedOutputMismatch {
        field: &'static str,
        group_index: usize,
        substep_index: usize,
    },
    CounterOverflow,
}

#[derive(Clone, Debug)]
pub(crate) struct NativeTrainerStateV1 {
    base_seed: u64,
    train_state: NativePolicyValueTrainStateV1,
    progress: NativeTrainerProgressV1,
    #[cfg(test)]
    pending_test_association_mutation: Option<NativePolicyAssociationTestMutationV1>,
    #[cfg(test)]
    pending_test_train_non_selected_logit_mutation: bool,
}

impl NativeTrainerStateV1 {
    pub(crate) fn new_v1(
        base_seed: u64,
        train_state: NativePolicyValueTrainStateV1,
    ) -> Result<Self, NativeTrainerErrorV1> {
        native_trainer_episode_schedule_v1(base_seed, 0).map_err(NativeTrainerErrorV1::Schedule)?;
        Ok(Self {
            base_seed,
            train_state,
            progress: NativeTrainerProgressV1 {
                next_episode_index: 0,
                successful_update_count: 0,
                completed_episode_count: 0,
                learner_physical_decision_count: 0,
                learner_policy_step_count: 0,
            },
            #[cfg(test)]
            pending_test_association_mutation: None,
            #[cfg(test)]
            pending_test_train_non_selected_logit_mutation: false,
        })
    }

    pub(crate) fn progress_v1(&self) -> NativeTrainerProgressV1 {
        self.progress
    }

    pub(crate) fn train_state_v1(&self) -> &NativePolicyValueTrainStateV1 {
        &self.train_state
    }

    pub(crate) fn run_two_episode_update_v1(
        &mut self,
        config: &NativeTrainerUpdateConfigV1,
    ) -> Result<NativeTrainerUpdateEvidenceV1, NativeTrainerErrorV1> {
        self.run_two_episode_update_inner_v1(config)
    }

    fn run_two_episode_update_inner_v1(
        &mut self,
        config: &NativeTrainerUpdateConfigV1,
    ) -> Result<NativeTrainerUpdateEvidenceV1, NativeTrainerErrorV1> {
        #[cfg(test)]
        let test_mutation = self.pending_test_association_mutation.take();
        #[cfg(test)]
        let test_train_non_selected_logit_mutation =
            std::mem::take(&mut self.pending_test_train_non_selected_logit_mutation);
        validate_update_config_v1(config)?;
        let expected_deck_hashes = [
            runtime_deck_by_id(&config.deck_ids[0])
                .ok_or(NativeTrainerErrorV1::InvalidUpdateConfig("deck_ids"))?
                .runtime_deck_hash,
            runtime_deck_by_id(&config.deck_ids[1])
                .ok_or(NativeTrainerErrorV1::InvalidUpdateConfig("deck_ids"))?
                .runtime_deck_hash,
        ];
        if self.progress.next_episode_index & 1 != 0 {
            return Err(NativeTrainerErrorV1::GroupingInvariant(
                "next episode must begin an even/odd parity pair",
            ));
        }
        let first_episode_index = self.progress.next_episode_index;
        let end_episode_index = first_episode_index
            .checked_add(NATIVE_TRAINER_EPISODES_PER_UPDATE_V1)
            .ok_or(NativeTrainerErrorV1::CounterOverflow)?;
        native_trainer_episode_schedule_v1(self.base_seed, first_episode_index)
            .map_err(NativeTrainerErrorV1::Schedule)?;
        native_trainer_episode_schedule_v1(self.base_seed, end_episode_index - 1)
            .map_err(NativeTrainerErrorV1::Schedule)?;

        let rollout_config = AsyncRolloutConfigV2 {
            deck_ids: config.deck_ids.clone(),
            learner_seat: PlayerSeatV1::P0,
            environment_seed: self.base_seed,
            opponent_policy_seed: self.base_seed,
            learner_policy_seed: self.base_seed,
            max_physical_decisions: config.max_physical_decisions,
            max_policy_steps: config.max_policy_steps,
            worker_count: config.worker_count,
            sessions_per_worker: config.sessions_per_worker,
            broker_batch_target: config.broker_batch_target,
            first_episode_id: first_episode_index,
            episode_count: NATIVE_TRAINER_EPISODES_PER_UPDATE_V1,
            scheduler_timeout: config.scheduler_timeout,
            measure_broker_service_time: config.measure_broker_service_time,
        };
        let (producer, consumer) = native_policy_association_channel_v1();
        #[cfg(test)]
        if let Some(mutation) = test_mutation {
            producer.set_test_mutation_v1(mutation).map_err(|error| {
                NativeTrainerErrorV1::ObserverConstruction(
                    NativePolicyTrajectoryErrorV1::Association(error),
                )
            })?;
        }
        let observer = NativePolicyTrajectoryObserverV1::new_v1(
            first_episode_index,
            NATIVE_TRAINER_EPISODES_PER_UPDATE_V1,
            self.base_seed,
            expected_deck_hashes,
            consumer,
        )
        .map_err(NativeTrainerErrorV1::ObserverConstruction)?;
        let mut scorer = NativePolicyBatchScorerV2::new_v1(self.train_state.model_v1(), producer);
        let rollout_result = run_async_flat_scored_rollout_native_observed_v2(
            rollout_config,
            self.base_seed,
            &mut scorer,
            observer,
        );
        let scorer_accepted_batch_count = scorer.accepted_batch_count;
        let scorer_accepted_decision_count = scorer.accepted_decision_count;
        let scorer_failure = scorer.last_failure.clone();
        drop(scorer);
        let (rollout, observed_trajectory) = match rollout_result {
            Ok(output) => output,
            Err(AsyncFlatScoredObservedRunErrorV2::Rollout(
                error @ AsyncFlatScoredRolloutErrorV2::ScorerFailed { .. },
            )) => {
                return Err(match scorer_failure {
                    Some(failure) => NativeTrainerErrorV1::Scorer(failure),
                    None => NativeTrainerErrorV1::Rollout(error),
                });
            }
            Err(AsyncFlatScoredObservedRunErrorV2::Rollout(error)) => {
                return Err(NativeTrainerErrorV1::Rollout(error));
            }
            Err(AsyncFlatScoredObservedRunErrorV2::ObserverFailed { phase, error }) => {
                return Err(NativeTrainerErrorV1::ObserverFailed { phase, error });
            }
            Err(AsyncFlatScoredObservedRunErrorV2::ObserverPanicked { phase }) => {
                return Err(NativeTrainerErrorV1::ObserverPanicked { phase });
            }
        };
        let NativePolicyObservedTrajectoryV1 {
            grouped,
            full_trajectory_receipts,
        } = observed_trajectory;
        validate_grouped_batch_v1(&grouped, first_episode_index)?;
        if !rollout.all_natural() || rollout.episodes.len() != 2 {
            return Err(NativeTrainerErrorV1::GroupingInvariant(
                "rollout must contain exactly two natural episodes",
            ));
        }
        #[cfg(test)]
        let mut grouped = grouped;
        #[cfg(test)]
        if test_train_non_selected_logit_mutation {
            mutate_grouped_non_selected_logit_for_test_v1(&mut grouped)?;
        }

        let model_digest_before = self.train_state.model_v1().parameter_manifest_sha256_v1();
        let parameters_before = self.train_state.model_v1().parameter_snapshot_v1();
        let adam_step_before = self.train_state.adam_step_v1();
        let mut candidate_train_state = self.train_state.clone();
        let (train_result, episode_evidence, learner_group_count) = train_grouped_candidate_v1(
            &mut candidate_train_state,
            &grouped,
            &full_trajectory_receipts,
            f32::from_bits(config.value_coefficient_bits),
            f32::from_bits(config.learning_rate_bits),
        )?;
        let parameters_after = candidate_train_state.model_v1().parameter_snapshot_v1();
        let model_digest_after = candidate_train_state
            .model_v1()
            .parameter_manifest_sha256_v1();
        let changed_non_gauge_parameter_count =
            changed_non_gauge_parameters_v1(&parameters_before, &parameters_after)?;

        let next_progress = NativeTrainerProgressV1 {
            next_episode_index: end_episode_index,
            successful_update_count: self
                .progress
                .successful_update_count
                .checked_add(1)
                .ok_or(NativeTrainerErrorV1::CounterOverflow)?,
            completed_episode_count: self
                .progress
                .completed_episode_count
                .checked_add(NATIVE_TRAINER_EPISODES_PER_UPDATE_V1)
                .ok_or(NativeTrainerErrorV1::CounterOverflow)?,
            learner_physical_decision_count: self
                .progress
                .learner_physical_decision_count
                .checked_add(learner_group_count)
                .ok_or(NativeTrainerErrorV1::CounterOverflow)?,
            learner_policy_step_count: self
                .progress
                .learner_policy_step_count
                .checked_add(grouped.learner_policy_step_count)
                .ok_or(NativeTrainerErrorV1::CounterOverflow)?,
        };
        let expected_adam_step = adam_step_before
            .checked_add(1)
            .ok_or(NativeTrainerErrorV1::CounterOverflow)?;
        if train_result.adam_step != expected_adam_step {
            return Err(NativeTrainerErrorV1::GroupingInvariant(
                "one grouped batch must advance Adam exactly once",
            ));
        }
        let selected_outputs = train_result
            .selected_outputs
            .iter()
            .map(|output| NativeTrainerSelectedOutputEvidenceV1 {
                group_index: output.group_index,
                substep_index: output.substep_index,
                selected_action_index: output.selected_action_index,
                selected_logit_bits: output.selected_logit.to_bits(),
                value_bits: output.value.to_bits(),
                selected_log_probability_bits: output.selected_log_probability.to_bits(),
            })
            .collect();
        let physical_terms = train_result
            .physical_terms
            .iter()
            .map(|term| NativeTrainerPhysicalTermEvidenceV1 {
                joint_log_probability_bits: term.joint_log_probability.to_bits(),
                value_bits: term.value.to_bits(),
                terminal_return: term.terminal_return,
            })
            .collect();
        let evidence = NativeTrainerUpdateEvidenceV1 {
            first_episode_index,
            episode_count: NATIVE_TRAINER_EPISODES_PER_UPDATE_V1,
            episodes: episode_evidence,
            learner_group_count,
            learner_policy_step_count: grouped.learner_policy_step_count,
            scorer_accepted_batch_count,
            scorer_accepted_decision_count,
            rollout_metrics: rollout.metrics,
            model_digest_before,
            model_digest_after,
            changed_non_gauge_parameter_count,
            policy_sum_bits: train_result.policy_sum.to_bits(),
            value_sum_bits: train_result.value_sum.to_bits(),
            loss_bits: train_result.loss.to_bits(),
            adam_step_before,
            adam_step_after: train_result.adam_step,
            selected_outputs,
            physical_terms,
            scorer_bias_gauge: train_result.scorer_bias_gauge,
        };

        // The only live-state commit in the update path. Every rollout,
        // association, grouping, recomputation, train, parameter, optimizer,
        // evidence, and counter check above completed on owned candidates.
        self.train_state = candidate_train_state;
        self.progress = next_progress;
        Ok(evidence)
    }

    #[cfg(test)]
    fn run_two_episode_update_with_mutation_v1(
        &mut self,
        config: &NativeTrainerUpdateConfigV1,
        mutation: NativePolicyAssociationTestMutationV1,
    ) -> Result<NativeTrainerUpdateEvidenceV1, NativeTrainerErrorV1> {
        assert!(self.pending_test_association_mutation.is_none());
        self.pending_test_association_mutation = Some(mutation);
        self.run_two_episode_update_v1(config)
    }

    #[cfg(test)]
    fn run_two_episode_update_with_train_non_selected_logit_mutation_v1(
        &mut self,
        config: &NativeTrainerUpdateConfigV1,
    ) -> Result<NativeTrainerUpdateEvidenceV1, NativeTrainerErrorV1> {
        assert!(!self.pending_test_train_non_selected_logit_mutation);
        self.pending_test_train_non_selected_logit_mutation = true;
        self.run_two_episode_update_v1(config)
    }
}

#[cfg(test)]
fn mutate_grouped_non_selected_logit_for_test_v1(
    grouped: &mut NativePolicyGroupedTrajectoryV1,
) -> Result<(), NativeTrainerErrorV1> {
    let mut group_index = 0usize;
    for episode in &mut grouped.episodes {
        for group in &mut episode.groups {
            for (substep_index, substep) in group.substeps.iter_mut().enumerate() {
                let selected_action_index =
                    usize::try_from(substep.selected_index).map_err(|_| {
                        NativeTrainerErrorV1::RecomputedOutputMismatch {
                            field: "selected_action_index",
                            group_index,
                            substep_index,
                        }
                    })?;
                if substep.raw_action_logit_bits.len() > 1 {
                    let action_index = if selected_action_index == 0 { 1 } else { 0 };
                    substep.raw_action_logit_bits[action_index] ^= 1;
                    return Ok(());
                }
            }
            group_index = group_index
                .checked_add(1)
                .ok_or(NativeTrainerErrorV1::CounterOverflow)?;
        }
    }
    Err(NativeTrainerErrorV1::GroupingInvariant(
        "test requires a non-selected action row",
    ))
}

fn validate_update_config_v1(
    config: &NativeTrainerUpdateConfigV1,
) -> Result<(), NativeTrainerErrorV1> {
    if config.deck_ids.iter().any(String::is_empty) {
        return Err(NativeTrainerErrorV1::InvalidUpdateConfig("deck_ids"));
    }
    if config.max_physical_decisions == 0 {
        return Err(NativeTrainerErrorV1::InvalidUpdateConfig(
            "max_physical_decisions",
        ));
    }
    if config.max_policy_steps == 0 {
        return Err(NativeTrainerErrorV1::InvalidUpdateConfig(
            "max_policy_steps",
        ));
    }
    if config.scheduler_timeout.is_zero() {
        return Err(NativeTrainerErrorV1::InvalidUpdateConfig(
            "scheduler_timeout",
        ));
    }
    let value_coefficient = f32::from_bits(config.value_coefficient_bits);
    if !value_coefficient.is_finite() || value_coefficient <= 0.0 {
        return Err(NativeTrainerErrorV1::InvalidUpdateConfig(
            "value_coefficient",
        ));
    }
    let learning_rate = f32::from_bits(config.learning_rate_bits);
    if !learning_rate.is_finite() || learning_rate <= 0.0 {
        return Err(NativeTrainerErrorV1::InvalidUpdateConfig("learning_rate"));
    }
    Ok(())
}

fn validate_grouped_batch_v1(
    grouped: &NativePolicyGroupedTrajectoryV1,
    first_episode_index: u64,
) -> Result<(), NativeTrainerErrorV1> {
    if grouped.learner_seat_rule != FlatPhysicalLearnerSeatRuleCore::EpisodeParity
        || grouped.first_episode_id != first_episode_index
        || grouped.episode_count != NATIVE_TRAINER_EPISODES_PER_UPDATE_V1
        || grouped.episodes.len() != 2
    {
        return Err(NativeTrainerErrorV1::GroupingInvariant(
            "alternating-seat batch envelope",
        ));
    }
    for (offset, episode) in grouped.episodes.iter().enumerate() {
        let expected_episode = first_episode_index
            .checked_add(u64::try_from(offset).map_err(|_| NativeTrainerErrorV1::CounterOverflow)?)
            .ok_or(NativeTrainerErrorV1::CounterOverflow)?;
        let expected_seat = if offset == 0 {
            PlayerSeatV1::P0
        } else {
            PlayerSeatV1::P1
        };
        if episode.episode_id != expected_episode || episode.learner_seat != expected_seat {
            return Err(NativeTrainerErrorV1::GroupingInvariant(
                "alternating-seat episode order",
            ));
        }
    }
    match grouped.update_staging {
        FlatPhysicalUpdateStagingCore::Ready {
            learner_group_count,
        } if learner_group_count > 0
            && learner_group_count == grouped.learner_physical_decision_count =>
        {
            Ok(())
        }
        _ => Err(NativeTrainerErrorV1::GroupingInvariant(
            "nonzero canonical learner groups",
        )),
    }
}

fn train_grouped_candidate_v1(
    candidate: &mut NativePolicyValueTrainStateV1,
    grouped: &NativePolicyGroupedTrajectoryV1,
    full_trajectory_receipts: &[NativeFullEpisodeTrajectoryReceiptV1],
    value_coefficient: f32,
    learning_rate: f32,
) -> Result<
    (
        crate::native_policy_train_step_v1::NativePolicyTrainStepResultV1,
        [NativeTrainerEpisodeEvidenceV1; 2],
        u64,
    ),
    NativeTrainerErrorV1,
> {
    let mut source_groups = Vec::new();
    let mut terminal_returns = Vec::new();
    let mut episode_evidence = Vec::with_capacity(2);
    for episode in &grouped.episodes {
        let mut matching_receipts = full_trajectory_receipts
            .iter()
            .filter(|receipt| receipt.episode_index == episode.episode_id);
        let full_trajectory_receipt =
            matching_receipts
                .next()
                .copied()
                .ok_or(NativeTrainerErrorV1::GroupingInvariant(
                    "episode evidence is missing its full trajectory receipt",
                ))?;
        if matching_receipts.next().is_some() {
            return Err(NativeTrainerErrorV1::GroupingInvariant(
                "episode evidence has duplicate full trajectory receipts",
            ));
        }
        let terminal_return = i8::try_from(episode.learner_return).map_err(|_| {
            NativeTrainerErrorV1::TerminalReturnRange {
                episode_index: episode.episode_id,
                value: episode.learner_return,
            }
        })?;
        if !matches!(terminal_return, -1..=1) {
            return Err(NativeTrainerErrorV1::TerminalReturnRange {
                episode_index: episode.episode_id,
                value: episode.learner_return,
            });
        }
        episode_evidence.push(NativeTrainerEpisodeEvidenceV1 {
            episode_index: episode.episode_id,
            learner_seat: episode.learner_seat,
            learner_return: terminal_return,
            learner_group_count: u64::try_from(episode.groups.len())
                .map_err(|_| NativeTrainerErrorV1::CounterOverflow)?,
            learner_policy_step_count: episode.learner_policy_step_count,
            learner_trace_hash: episode.learner_trace_hash,
            terminal_outcome: episode.terminal.terminal_outcome,
            full_trajectory_receipt,
        });
        for group in &episode.groups {
            source_groups.push(group);
            terminal_returns.push(terminal_return);
        }
    }
    let learner_group_count =
        u64::try_from(source_groups.len()).map_err(|_| NativeTrainerErrorV1::CounterOverflow)?;
    if learner_group_count == 0 || learner_group_count != grouped.learner_physical_decision_count {
        return Err(NativeTrainerErrorV1::GroupingInvariant(
            "group count does not match grouped staging",
        ));
    }

    let borrowed_substeps = source_groups
        .iter()
        .enumerate()
        .map(|(group_index, group)| {
            group
                .substeps
                .iter()
                .enumerate()
                .map(|(substep_index, substep)| {
                    Ok(NativePolicySubstepV1 {
                        encoded: native_encoded_decision_view_v1(&substep.scoring_inputs),
                        selected_action_index: usize::try_from(substep.selected_index).map_err(
                            |_| NativeTrainerErrorV1::RecomputedOutputMismatch {
                                field: "selected_action_index",
                                group_index,
                                substep_index,
                            },
                        )?,
                        expected_raw_action_logit_bits: &substep.raw_action_logit_bits,
                        expected_value_bits: substep.predicted_value_bits,
                    })
                })
                .collect::<Result<Vec<_>, NativeTrainerErrorV1>>()
        })
        .collect::<Result<Vec<_>, NativeTrainerErrorV1>>()?;
    let borrowed_groups = borrowed_substeps
        .iter()
        .zip(&terminal_returns)
        .map(
            |(substeps, terminal_return)| NativePolicyPhysicalDecisionV1 {
                substeps,
                terminal_return: *terminal_return,
            },
        )
        .collect::<Vec<_>>();
    let result = candidate
        .train_step_v1(&borrowed_groups, value_coefficient, learning_rate)
        .map_err(NativeTrainerErrorV1::Train)?;
    verify_recomputed_outputs_v1(&source_groups, &terminal_returns, &result)?;
    let episodes: [NativeTrainerEpisodeEvidenceV1; 2] = episode_evidence
        .try_into()
        .map_err(|_| NativeTrainerErrorV1::GroupingInvariant("episode evidence count"))?;
    Ok((result, episodes, learner_group_count))
}

fn verify_recomputed_outputs_v1(
    source_groups: &[&crate::private_physical_trajectory_core::FlatPhysicalDecisionSampleCore<
        FlatDecisionBindingV2,
        NativeFlatDecisionTensorV2,
    >],
    terminal_returns: &[i8],
    result: &crate::native_policy_train_step_v1::NativePolicyTrainStepResultV1,
) -> Result<(), NativeTrainerErrorV1> {
    let expected_substep_count = source_groups
        .iter()
        .try_fold(0usize, |sum, group| sum.checked_add(group.substeps.len()))
        .ok_or(NativeTrainerErrorV1::CounterOverflow)?;
    if terminal_returns.len() != source_groups.len()
        || result.selected_outputs.len() != expected_substep_count
        || result.physical_terms.len() != source_groups.len()
    {
        return Err(NativeTrainerErrorV1::GroupingInvariant(
            "train result cardinality",
        ));
    }
    let mut output_index = 0usize;
    for (group_index, group) in source_groups.iter().enumerate() {
        for (substep_index, substep) in group.substeps.iter().enumerate() {
            let output = &result.selected_outputs[output_index];
            let selected = usize::try_from(substep.selected_index).map_err(|_| {
                NativeTrainerErrorV1::RecomputedOutputMismatch {
                    field: "selected_action_index",
                    group_index,
                    substep_index,
                }
            })?;
            let expected_logit_bits = substep.raw_action_logit_bits.get(selected).copied().ok_or(
                NativeTrainerErrorV1::RecomputedOutputMismatch {
                    field: "selected_logit",
                    group_index,
                    substep_index,
                },
            )?;
            let mismatch = if output.group_index != group_index
                || output.substep_index != substep_index
                || output.selected_action_index != selected
            {
                Some("selected_action_index")
            } else if output.selected_logit.to_bits() != expected_logit_bits {
                Some("selected_logit")
            } else if output.value.to_bits() != substep.predicted_value_bits {
                Some("value")
            } else if output.selected_log_probability.to_bits()
                != substep.selected_log_probability_bits
            {
                Some("selected_log_probability")
            } else {
                None
            };
            if let Some(field) = mismatch {
                return Err(NativeTrainerErrorV1::RecomputedOutputMismatch {
                    field,
                    group_index,
                    substep_index,
                });
            }
            output_index += 1;
        }
        let term = &result.physical_terms[group_index];
        if term.joint_log_probability.to_bits() != group.joint_selected_log_probability_bits {
            return Err(NativeTrainerErrorV1::RecomputedOutputMismatch {
                field: "joint_log_probability",
                group_index,
                substep_index: 0,
            });
        }
        if term.value.to_bits() != group.value_bits {
            return Err(NativeTrainerErrorV1::RecomputedOutputMismatch {
                field: "first_value",
                group_index,
                substep_index: 0,
            });
        }
        if term.terminal_return != terminal_returns[group_index] {
            return Err(NativeTrainerErrorV1::RecomputedOutputMismatch {
                field: "terminal_return",
                group_index,
                substep_index: 0,
            });
        }
    }
    Ok(())
}

fn changed_non_gauge_parameters_v1(
    before: &[NativeNamedParameterV1],
    after: &[NativeNamedParameterV1],
) -> Result<usize, NativeTrainerErrorV1> {
    if before.len() != after.len() {
        return Err(NativeTrainerErrorV1::GroupingInvariant(
            "parameter manifest length",
        ));
    }
    let mut changed = 0usize;
    for (before, after) in before.iter().zip(after) {
        if before.name != after.name
            || before.shape != after.shape
            || before.values.len() != after.values.len()
            || !after.values.iter().all(|value| value.is_finite())
        {
            return Err(NativeTrainerErrorV1::GroupingInvariant(
                "candidate parameter manifest",
            ));
        }
        let differs = before
            .values
            .iter()
            .zip(&after.values)
            .any(|(left, right)| left.to_bits() != right.to_bits());
        if before.name == "scorer.2.bias" {
            if differs {
                return Err(NativeTrainerErrorV1::GroupingInvariant(
                    "scorer bias gauge anchor",
                ));
            }
        } else if differs {
            changed = changed
                .checked_add(1)
                .ok_or(NativeTrainerErrorV1::CounterOverflow)?;
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_flat_scored_rollout_v1::ASYNC_FLAT_SCORED_TEST_LOCK_V1 as TEST_LOCK;
    use crate::native_policy_train_step_v1::NativePolicyValueTrainStateV1;
    use crate::native_policy_value_net_v1::{
        NativePolicyValueModelConfigV1, NativePolicyValueNetV1,
    };

    fn burn_update_config_v1(
        worker_count: usize,
        sessions_per_worker: usize,
        broker_batch_target: usize,
    ) -> NativeTrainerUpdateConfigV1 {
        NativeTrainerUpdateConfigV1 {
            deck_ids: ["Burn".to_owned(), "Burn".to_owned()],
            max_physical_decisions: 5_000,
            max_policy_steps: 640_000,
            worker_count,
            sessions_per_worker,
            broker_batch_target,
            scheduler_timeout: Duration::from_secs(600),
            measure_broker_service_time: false,
            value_coefficient_bits: 0.5f32.to_bits(),
            learning_rate_bits: 0.001f32.to_bits(),
        }
    }

    fn trainer_v1() -> NativeTrainerStateV1 {
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let train_state = NativePolicyValueTrainStateV1::new_v1(model).unwrap();
        NativeTrainerStateV1::new_v1(71_501, train_state).unwrap()
    }

    fn exact_state_snapshot_v1(
        trainer: &NativeTrainerStateV1,
    ) -> (
        NativeTrainerProgressV1,
        Vec<NativeNamedParameterV1>,
        Vec<NativeNamedParameterV1>,
        Vec<NativeNamedParameterV1>,
    ) {
        (
            trainer.progress_v1(),
            trainer.train_state_v1().model_v1().parameter_snapshot_v1(),
            trainer.train_state_v1().first_moment_snapshot_v1(),
            trainer.train_state_v1().second_moment_snapshot_v1(),
        )
    }

    fn gauge_value_bits_v1(parameters: &[NativeNamedParameterV1]) -> u32 {
        parameters
            .iter()
            .find(|parameter| parameter.name == "scorer.2.bias")
            .unwrap()
            .values[0]
            .to_bits()
    }

    #[test]
    fn real_burn_pair_updates_once_and_is_topology_invariant() {
        let _lock = TEST_LOCK.lock().unwrap();
        let initial = trainer_v1();
        let initial_parameters = initial.train_state_v1().model_v1().parameter_snapshot_v1();
        let initial_bias_bits = gauge_value_bits_v1(&initial_parameters);
        let mut narrow = initial.clone();
        let mut wide = initial;

        let narrow_evidence = narrow
            .run_two_episode_update_v1(&burn_update_config_v1(1, 1, 1))
            .unwrap();
        let wide_evidence = wide
            .run_two_episode_update_v1(&burn_update_config_v1(2, 2, 3))
            .unwrap();

        for evidence in [&narrow_evidence, &wide_evidence] {
            assert_eq!(evidence.first_episode_index, 0);
            assert_eq!(evidence.episode_count, 2);
            assert_eq!(evidence.episodes[0].learner_seat, PlayerSeatV1::P0);
            assert_eq!(evidence.episodes[1].learner_seat, PlayerSeatV1::P1);
            assert!(evidence
                .episodes
                .iter()
                .all(|episode| episode.learner_group_count > 0));
            for episode in &evidence.episodes {
                let receipt = episode.full_trajectory_receipt;
                let expected_schedule =
                    native_trainer_episode_schedule_v1(71_501, episode.episode_index).unwrap();
                assert_eq!(receipt.episode_index, episode.episode_index);
                assert_eq!(receipt.environment_seed, expected_schedule.environment_seed);
                assert_eq!(receipt.learner_seat, episode.learner_seat);
                assert_eq!(receipt.deck_hashes, [0x5fdb_7b92_986b_6fc1; 2]);
                assert_ne!(receipt.trajectory_sha256, [0; 32]);
                assert_eq!(
                    receipt.learner_policy_step_count,
                    episode.learner_policy_step_count
                );
                assert_eq!(
                    receipt.learner_physical_decision_count,
                    episode.learner_group_count
                );
                assert!(receipt.opponent_policy_step_count > 0);
                assert!(receipt.opponent_physical_decision_count > 0);
                assert_eq!(
                    receipt.policy_step_count,
                    receipt.learner_policy_step_count + receipt.opponent_policy_step_count
                );
                assert_eq!(
                    receipt.physical_decision_count,
                    receipt.learner_physical_decision_count
                        + receipt.opponent_physical_decision_count
                );
            }
            assert!(evidence.learner_group_count > 0);
            assert!(evidence.learner_policy_step_count > 0);
            assert!(evidence.scorer_accepted_batch_count > 0);
            assert_eq!(evidence.adam_step_before, 0);
            assert_eq!(evidence.adam_step_after, 1);
            assert!(evidence.changed_non_gauge_parameter_count > 0);
            assert_ne!(evidence.model_digest_before, evidence.model_digest_after);
            assert_eq!(
                evidence.scorer_bias_gauge.parameter_before_bits,
                initial_bias_bits
            );
            assert_eq!(
                evidence.scorer_bias_gauge.parameter_after_bits,
                initial_bias_bits
            );
            assert_eq!(evidence.scorer_bias_gauge.canonical_gradient.to_bits(), 0);
            assert_eq!(
                evidence.scorer_accepted_decision_count,
                evidence.learner_policy_step_count
            );
            assert_eq!(
                u64::try_from(evidence.selected_outputs.len()).unwrap(),
                evidence.learner_policy_step_count
            );
            assert!(evidence
                .selected_outputs
                .iter()
                .any(|output| output.substep_index > 0));
            assert_eq!(
                evidence.rollout_metrics.scored_decision_count,
                evidence.scorer_accepted_decision_count
            );
        }

        assert_eq!(narrow.progress_v1().successful_update_count, 1);
        assert_eq!(narrow.progress_v1().completed_episode_count, 2);
        assert_eq!(narrow.progress_v1().next_episode_index, 2);
        assert_eq!(narrow.train_state_v1().adam_step_v1(), 1);
        let narrow_parameters = narrow.train_state_v1().model_v1().parameter_snapshot_v1();
        assert!(narrow_parameters
            .iter()
            .flat_map(|parameter| &parameter.values)
            .all(|value| value.is_finite()));
        assert_eq!(gauge_value_bits_v1(&narrow_parameters), initial_bias_bits);
        let scorer_first = narrow
            .train_state_v1()
            .first_moment_snapshot_v1()
            .into_iter()
            .find(|parameter| parameter.name == "scorer.2.bias")
            .unwrap();
        let scorer_second = narrow
            .train_state_v1()
            .second_moment_snapshot_v1()
            .into_iter()
            .find(|parameter| parameter.name == "scorer.2.bias")
            .unwrap();
        assert!(scorer_first.values.iter().all(|value| value.to_bits() == 0));
        assert!(scorer_second
            .values
            .iter()
            .all(|value| value.to_bits() == 0));

        assert_eq!(narrow_evidence.episodes, wide_evidence.episodes);
        assert_eq!(
            narrow_evidence.learner_group_count,
            wide_evidence.learner_group_count
        );
        assert_eq!(
            narrow_evidence.learner_policy_step_count,
            wide_evidence.learner_policy_step_count
        );
        assert_eq!(
            narrow_evidence.selected_outputs,
            wide_evidence.selected_outputs
        );
        assert_eq!(narrow_evidence.physical_terms, wide_evidence.physical_terms);
        assert_eq!(
            narrow_evidence.policy_sum_bits,
            wide_evidence.policy_sum_bits
        );
        assert_eq!(narrow_evidence.value_sum_bits, wide_evidence.value_sum_bits);
        assert_eq!(narrow_evidence.loss_bits, wide_evidence.loss_bits);
        assert_eq!(
            narrow.train_state_v1().model_v1().parameter_snapshot_v1(),
            wide.train_state_v1().model_v1().parameter_snapshot_v1()
        );
        assert_eq!(
            narrow.train_state_v1().first_moment_snapshot_v1(),
            wide.train_state_v1().first_moment_snapshot_v1()
        );
        assert_eq!(
            narrow.train_state_v1().second_moment_snapshot_v1(),
            wide.train_state_v1().second_moment_snapshot_v1()
        );

        let second_evidence = narrow
            .run_two_episode_update_v1(&burn_update_config_v1(1, 1, 1))
            .unwrap();
        assert_eq!(second_evidence.first_episode_index, 2);
        assert_eq!(
            second_evidence
                .episodes
                .map(|episode| episode.episode_index),
            [2, 3]
        );
        assert_eq!(second_evidence.episodes[0].learner_seat, PlayerSeatV1::P0);
        assert_eq!(second_evidence.episodes[1].learner_seat, PlayerSeatV1::P1);
        assert_eq!(second_evidence.adam_step_before, 1);
        assert_eq!(second_evidence.adam_step_after, 2);
        assert_eq!(narrow.progress_v1().successful_update_count, 2);
        assert_eq!(narrow.progress_v1().completed_episode_count, 4);
        assert_eq!(narrow.progress_v1().next_episode_index, 4);
        assert_eq!(narrow.train_state_v1().adam_step_v1(), 2);
    }

    #[test]
    fn association_mutations_leave_model_optimizer_and_counters_exact() {
        let _lock = TEST_LOCK.lock().unwrap();
        let mut trainer = trainer_v1();
        let config = burn_update_config_v1(1, 1, 1);
        let before = exact_state_snapshot_v1(&trainer);
        for (mutation, expected) in [
            (
                NativePolicyAssociationTestMutationV1::Binding,
                NativePolicyAssociationErrorV1::BindingMismatch,
            ),
            (
                NativePolicyAssociationTestMutationV1::SelectedLogit,
                NativePolicyAssociationErrorV1::LogitBitsMismatch,
            ),
            (
                NativePolicyAssociationTestMutationV1::Value,
                NativePolicyAssociationErrorV1::ValueBitsMismatch,
            ),
        ] {
            let error = trainer
                .run_two_episode_update_with_mutation_v1(&config, mutation)
                .unwrap_err();
            assert!(matches!(
                error,
                NativeTrainerErrorV1::ObserverFailed {
                    phase: FlatScoredObserverPhaseV2::Selected,
                    error: NativePolicyTrajectoryErrorV1::Association(actual),
                } if actual == expected
            ));
            assert_eq!(exact_state_snapshot_v1(&trainer), before);
        }
    }

    #[test]
    fn non_selected_logit_mutation_reaches_full_vector_gate_transactionally() {
        let _lock = TEST_LOCK.lock().unwrap();
        let mut trainer = trainer_v1();
        let config = burn_update_config_v1(1, 1, 1);
        let before = exact_state_snapshot_v1(&trainer);
        let error = trainer
            .run_two_episode_update_with_train_non_selected_logit_mutation_v1(&config)
            .unwrap_err();
        assert!(matches!(
            error,
            NativeTrainerErrorV1::Train(
                NativePolicyTrainErrorV1::RecomputedLogitBitsMismatch {
                    action_index,
                    selected_action_index,
                    expected_bits,
                    actual_bits,
                    ..
                }
            ) if action_index != selected_action_index && expected_bits != actual_bits
        ));
        assert_eq!(exact_state_snapshot_v1(&trainer), before);
    }
}
