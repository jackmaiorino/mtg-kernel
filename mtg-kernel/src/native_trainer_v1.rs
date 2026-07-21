//! Real in-memory native trainer integration.
//!
//! The scorer tensorizes each production V2 learner decision exactly once,
//! evaluates the live native model, and atomically stages both its canonical
//! encoded tensor and private packed forward tape beside the exact packet
//! binding and output bits. Before packed activations may reach backward,
//! training independently reevaluates the retained tensor under its immutable
//! parameter snapshot and requires full output-bit identity. A complete
//! configurable even batch of alternating-seat episodes and one grouped Adam
//! step are prepared on private candidates; the live trainer changes only
//! after every cross-check passes.
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
use crate::async_rollout_v2::{
    AsyncRolloutConfigV2, ASYNC_ROLLOUT_MAX_SESSIONS_PER_WORKER_V2, ASYNC_ROLLOUT_MAX_WORKERS_V2,
};
use crate::common_model_snapshot_v1::{
    load_common_model_snapshot_v1, CommonModelSnapshotErrorV1, CommonModelSnapshotRecordV1,
};
use crate::flat_policy_v2::FlatDecisionBindingV2;
use crate::native_flat_tensorizer_v2::{
    NativeFlatDecisionTensorV2, NativeFlatTensorErrorV2, NativeFlatTensorizerV2,
};
use crate::native_full_episode_trajectory_v1::NativeFullEpisodeTrajectoryReceiptV1;
#[cfg(test)]
use crate::native_policy_train_step_v1::{
    packed_actual_recompute_call_count_for_test_v1, FIXED_BACKWARD_PARTITION_COUNT_V1,
};
use crate::native_policy_train_step_v1::{
    NativePolicyForwardInputV1, NativePolicyPackedForwardBuilderV1,
    NativePolicyPackedForwardTapeV1, NativePolicyPhysicalDecisionV1, NativePolicySubstepV1,
    NativePolicyTrainErrorV1, NativePolicyTrainStepResultV1, NativePolicyValueTrainStateV1,
    NativeScorerBiasGaugeRecordV1, NativeTrainingNumericalBackendV1,
};
use crate::native_policy_value_net_v1::{
    NativeEncodedDecisionSchemaV1, NativeEncodedDecisionViewV1, NativeNamedParameterV1,
    NativePolicyValueErrorV1, NativePolicyValueModelConfigV1, NativePolicyValueNetV1,
};
use crate::native_trainer_schedule_v1::{
    native_trainer_episode_schedule_v1, NativeTrainerScheduleErrorV1,
};
use crate::native_training_phase_diagnostic_v1::{
    NativeTrainingPhaseProfileV1, NativeTrainingPhaseRecorderV1, NativeTrainingPhaseV1,
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
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const NATIVE_POLICY_SCORER_CONTRACT_CODE_V1: u32 = 1;
const NATIVE_POLICY_SCORER_OUTPUT_SHAPE_CODE_V1: u32 = 2;
const NATIVE_POLICY_SCORER_DECISION_CODE_V1: u32 = 3;
const NATIVE_POLICY_SCORER_TENSOR_CODE_V1: u32 = 4;
const NATIVE_POLICY_SCORER_MODEL_CODE_V1: u32 = 5;
const NATIVE_POLICY_SCORER_ASSOCIATION_CODE_V1: u32 = 6;
const NATIVE_POLICY_SCORER_COUNTER_CODE_V1: u32 = 7;
pub(crate) const NATIVE_TRAINER_CONTRACT_IDENTITY_V2: &str =
    "mtg-kernel-native-even-batch-trainer-v2";
pub(crate) const NATIVE_TRAINER_MIN_BATCH_EPISODES_V2: u64 = 2;
pub(crate) const NATIVE_TRAINER_MAX_BATCH_EPISODES_V2: u64 = 10_000;
const NATIVE_TRAINER_U63_MAX_V2: u64 = (1_u64 << 63) - 1;

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
    ResidualScoredDecisions,
}

#[derive(Debug)]
struct NativePolicyScoredTrainingInputV1 {
    tensor: NativeFlatDecisionTensorV2,
    tape: NativePolicyPackedForwardTapeV1,
}

#[cfg(test)]
impl NativePolicyScoredTrainingInputV1 {
    fn corrupt_canonical_tensor_for_test_v1(&mut self) -> Result<(), ()> {
        if self.tensor.action_features.is_empty() {
            return Err(());
        }
        for value in &mut self.tensor.action_features {
            *value += 1.0;
            if !value.is_finite() {
                return Err(());
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
struct NativeScoredDecisionAssociationV1 {
    binding: FlatDecisionBindingV2,
    training_input: NativePolicyScoredTrainingInputV1,
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
    ModelGeneration,
    CanonicalTensor,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativePolicyTrainRevalidationTestMutationV1 {
    ExpectedLogitCount { episode_offset: usize },
    Logit { episode_offset: usize },
    Value { episode_offset: usize },
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
    ) -> Result<NativePolicyScoredTrainingInputV1, NativePolicyAssociationErrorV1> {
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
                    staged
                        .training_input
                        .tape
                        .corrupt_logit_for_test_v1(selected_index)
                        .map_err(|_| NativePolicyAssociationErrorV1::LogitCountMismatch)?;
                }
                NativePolicyAssociationTestMutationV1::Value => {
                    staged.training_input.tape.corrupt_value_for_test_v1();
                }
                NativePolicyAssociationTestMutationV1::ModelGeneration => {
                    staged
                        .training_input
                        .tape
                        .corrupt_model_generation_for_test_v1();
                }
                NativePolicyAssociationTestMutationV1::CanonicalTensor => {
                    staged
                        .training_input
                        .corrupt_canonical_tensor_for_test_v1()
                        .map_err(|_| NativePolicyAssociationErrorV1::LogitCountMismatch)?;
                }
            }
        }
        let tape_logits = staged.training_input.tape.logits_v1();
        let error = if staged.binding != event.binding {
            Some(NativePolicyAssociationErrorV1::BindingMismatch)
        } else if tape_logits.len() != event.raw_action_logits.len() {
            Some(NativePolicyAssociationErrorV1::LogitCountMismatch)
        } else if tape_logits[selected_index].to_bits()
            != event.raw_action_logits[selected_index].to_bits()
            || tape_logits
                .iter()
                .zip(event.raw_action_logits)
                .any(|(expected, actual)| expected.to_bits() != actual.to_bits())
        {
            Some(NativePolicyAssociationErrorV1::LogitBitsMismatch)
        } else if staged.training_input.tape.value_v1().to_bits() != event.predicted_value_bits {
            Some(NativePolicyAssociationErrorV1::ValueBitsMismatch)
        } else {
            None
        };
        if let Some(error) = error {
            shared.poisoned = Some(error);
            return Err(error);
        }
        Ok(staged.training_input)
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
    PackedForward(NativePolicyTrainErrorV1),
    ForwardWorker,
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
            Self::PackedForward(_) => NATIVE_POLICY_SCORER_MODEL_CODE_V1,
            Self::ForwardWorker => NATIVE_POLICY_SCORER_MODEL_CODE_V1,
            Self::Association(_) => NATIVE_POLICY_SCORER_ASSOCIATION_CODE_V1,
            Self::CounterOverflow => NATIVE_POLICY_SCORER_COUNTER_CODE_V1,
        }
    }
}

struct NativePolicyForwardTaskV1 {
    ordinal: usize,
    tensor: NativeFlatDecisionTensorV2,
    #[cfg(test)]
    force_panic: bool,
}

struct NativePolicyForwardResultV1 {
    ordinal: usize,
    tensor: NativeFlatDecisionTensorV2,
    tape: Option<NativePolicyPackedForwardTapeV1>,
    error: Option<NativePolicyTrainErrorV1>,
    panicked: bool,
}

/// Per-update bounded workers for independent CPU forwards. Tensorization and
/// result publication remain on the broker thread; workers see only owned
/// tensors and one immutable parameter snapshot. Results are reassembled by
/// input ordinal before any caller-visible slice or association is changed.
struct NativePolicyForwardPoolV1 {
    task_sender: Option<mpsc::SyncSender<NativePolicyForwardTaskV1>>,
    result_receiver: mpsc::Receiver<NativePolicyForwardResultV1>,
    workers: Vec<JoinHandle<()>>,
}

impl NativePolicyForwardPoolV1 {
    fn try_new_v1(
        builder: Arc<NativePolicyPackedForwardBuilderV1>,
        worker_count: usize,
    ) -> Option<Self> {
        if worker_count < 2 {
            return None;
        }
        let (task_sender, task_receiver) =
            mpsc::sync_channel::<NativePolicyForwardTaskV1>(worker_count);
        let task_receiver = Arc::new(Mutex::new(task_receiver));
        let (result_sender, result_receiver) = mpsc::channel();
        let mut workers = Vec::<JoinHandle<()>>::with_capacity(worker_count);
        for worker_index in 0..worker_count {
            let worker_builder = Arc::clone(&builder);
            let worker_tasks = Arc::clone(&task_receiver);
            let worker_results = result_sender.clone();
            let handle = match thread::Builder::new()
                .name(format!("native-policy-forward-{worker_index}"))
                .spawn(move || loop {
                    let task = {
                        let receiver = match worker_tasks.lock() {
                            Ok(receiver) => receiver,
                            Err(_) => break,
                        };
                        match receiver.recv() {
                            Ok(task) => task,
                            Err(_) => break,
                        }
                    };
                    let ordinal = task.ordinal;
                    let completed = catch_unwind(AssertUnwindSafe(|| {
                        #[cfg(test)]
                        if task.force_panic {
                            panic!("injected native policy forward worker panic");
                        }
                        worker_builder.forward_v1(native_encoded_decision_view_v1(&task.tensor))
                    }));
                    let (tape, error, panicked) = match completed {
                        Ok(Ok(tape)) => (Some(tape), None, false),
                        Ok(Err(error)) => (None, Some(error), false),
                        Err(_) => (None, None, true),
                    };
                    if worker_results
                        .send(NativePolicyForwardResultV1 {
                            ordinal,
                            tensor: task.tensor,
                            tape,
                            error,
                            panicked,
                        })
                        .is_err()
                    {
                        break;
                    }
                }) {
                Ok(handle) => handle,
                Err(_) => {
                    drop(task_sender);
                    drop(result_sender);
                    for worker in workers {
                        let _ = worker.join();
                    }
                    return None;
                }
            };
            workers.push(handle);
        }
        drop(result_sender);
        Some(Self {
            task_sender: Some(task_sender),
            result_receiver,
            workers,
        })
    }

    fn submit_v1(&self, task: NativePolicyForwardTaskV1) -> Result<(), ()> {
        self.task_sender
            .as_ref()
            .ok_or(())?
            .send(task)
            .map_err(|_| ())
    }

    fn receive_v1(&self) -> Result<NativePolicyForwardResultV1, ()> {
        self.result_receiver.recv().map_err(|_| ())
    }
}

impl Drop for NativePolicyForwardPoolV1 {
    fn drop(&mut self) {
        drop(self.task_sender.take());
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

/// Production V2 scorer backed by the exact native thirteen-tensor encoder and
/// native policy/value network. It owns one immutable parameter snapshot for
/// the update so every scored decision can retain the exact backward tape.
struct NativePolicyBatchScorerV2 {
    forward_builder: Arc<NativePolicyPackedForwardBuilderV1>,
    forward_pool: Option<NativePolicyForwardPoolV1>,
    tensorizer: NativeFlatTensorizerV2,
    associations: NativePolicyAssociationProducerV1,
    last_failure: Option<NativePolicyScorerFailureV1>,
    accepted_batch_count: u64,
    accepted_decision_count: u64,
    #[cfg(test)]
    force_next_parallel_worker_panic: bool,
}

impl NativePolicyBatchScorerV2 {
    fn new_v1(
        model: &NativePolicyValueNetV1,
        associations: NativePolicyAssociationProducerV1,
        forward_worker_limit: usize,
    ) -> Result<Self, NativePolicyTrainErrorV1> {
        let forward_builder = Arc::new(NativePolicyPackedForwardBuilderV1::from_model_v1(model)?);
        let available_workers = thread::available_parallelism()
            .map(|parallelism| parallelism.get())
            .unwrap_or(1);
        let forward_pool = NativePolicyForwardPoolV1::try_new_v1(
            Arc::clone(&forward_builder),
            forward_worker_limit.min(available_workers),
        );
        Ok(Self {
            forward_builder,
            forward_pool,
            tensorizer: NativeFlatTensorizerV2::new(),
            associations,
            last_failure: None,
            accepted_batch_count: 0,
            accepted_decision_count: 0,
            #[cfg(test)]
            force_next_parallel_worker_panic: false,
        })
    }

    fn score_decisions_scalar_v1(
        &mut self,
        batch: &FlatScoringBatchViewV2<'_>,
        action_logit_count: usize,
        candidate_logits: &mut Vec<f32>,
        candidate_values: &mut Vec<f32>,
        candidate_associations: &mut Vec<NativeScoredDecisionAssociationV1>,
    ) -> Result<(), NativePolicyScorerFailureV1> {
        for decision_index in 0..batch.decision_count() {
            let decision = batch
                .decision(decision_index)
                .ok_or(NativePolicyScorerFailureV1::MissingDecision)?;
            let binding = batch
                .binding(decision_index)
                .ok_or(NativePolicyScorerFailureV1::MissingDecision)?;
            let begin = batch.action_offsets()[decision_index];
            let end = batch.action_offsets()[decision_index + 1];
            if end < begin || end > action_logit_count || end - begin != decision.actions().len() {
                return Err(NativePolicyScorerFailureV1::OutputShape);
            }

            let mut tensor = NativeFlatDecisionTensorV2::default();
            self.tensorizer
                .fill(decision, &mut tensor)
                .map_err(NativePolicyScorerFailureV1::Tensor)?;
            let tape = self
                .forward_builder
                .forward_v1(native_encoded_decision_view_v1(&tensor))
                .map_err(NativePolicyScorerFailureV1::PackedForward)?;
            if tape.logits_v1().len() != end - begin || !tape.value_v1().is_finite() {
                return Err(NativePolicyScorerFailureV1::OutputShape);
            }
            candidate_logits.extend_from_slice(tape.logits_v1());
            candidate_values.push(tape.value_v1());
            candidate_associations.push(NativeScoredDecisionAssociationV1 {
                binding,
                training_input: NativePolicyScoredTrainingInputV1 { tensor, tape },
            });
        }
        Ok(())
    }

    fn score_decisions_parallel_v1(
        &mut self,
        batch: &FlatScoringBatchViewV2<'_>,
        action_logit_count: usize,
        candidate_logits: &mut Vec<f32>,
        candidate_values: &mut Vec<f32>,
        candidate_associations: &mut Vec<NativeScoredDecisionAssociationV1>,
    ) -> Result<(), NativePolicyScorerFailureV1> {
        let pool = self
            .forward_pool
            .as_ref()
            .ok_or(NativePolicyScorerFailureV1::ForwardWorker)?;
        #[cfg(test)]
        let force_worker_panic = std::mem::take(&mut self.force_next_parallel_worker_panic);
        let mut bindings = Vec::new();
        bindings
            .try_reserve_exact(batch.decision_count())
            .map_err(|_| NativePolicyScorerFailureV1::OutputShape)?;
        let mut expected_logit_counts = Vec::new();
        expected_logit_counts
            .try_reserve_exact(batch.decision_count())
            .map_err(|_| NativePolicyScorerFailureV1::OutputShape)?;
        let mut submitted = 0usize;
        let mut synchronous_failure = None;

        for decision_index in 0..batch.decision_count() {
            #[cfg(test)]
            // Pair the injected ordinal-zero worker panic with a later
            // broker-thread failure. The failure witness then proves that
            // parallel collection retains the scalar path's first-ordinal
            // precedence instead of returning the failure observed first in
            // wall-clock order.
            if force_worker_panic && decision_index == 1 {
                synchronous_failure = Some(NativePolicyScorerFailureV1::OutputShape);
                break;
            }
            let decision = match batch.decision(decision_index) {
                Some(decision) => decision,
                None => {
                    synchronous_failure = Some(NativePolicyScorerFailureV1::MissingDecision);
                    break;
                }
            };
            let binding = match batch.binding(decision_index) {
                Some(binding) => binding,
                None => {
                    synchronous_failure = Some(NativePolicyScorerFailureV1::MissingDecision);
                    break;
                }
            };
            let begin = batch.action_offsets()[decision_index];
            let end = batch.action_offsets()[decision_index + 1];
            if end < begin || end > action_logit_count || end - begin != decision.actions().len() {
                synchronous_failure = Some(NativePolicyScorerFailureV1::OutputShape);
                break;
            }
            let mut tensor = NativeFlatDecisionTensorV2::default();
            if let Err(error) = self.tensorizer.fill(decision, &mut tensor) {
                synchronous_failure = Some(NativePolicyScorerFailureV1::Tensor(error));
                break;
            }
            let task = NativePolicyForwardTaskV1 {
                ordinal: decision_index,
                tensor,
                #[cfg(test)]
                force_panic: force_worker_panic && decision_index == 0,
            };
            if pool.submit_v1(task).is_err() {
                synchronous_failure = Some(NativePolicyScorerFailureV1::ForwardWorker);
                break;
            }
            bindings.push(binding);
            expected_logit_counts.push(end - begin);
            submitted += 1;
        }

        let mut result_slots = (0..submitted)
            .map(|_| None)
            .collect::<Vec<Option<NativePolicyForwardResultV1>>>();
        let mut pool_protocol_failed = false;
        for _ in 0..submitted {
            match pool.receive_v1() {
                Ok(result)
                    if result.ordinal < submitted && result_slots[result.ordinal].is_none() =>
                {
                    let ordinal = result.ordinal;
                    result_slots[ordinal] = Some(result);
                }
                Ok(_) => pool_protocol_failed = true,
                Err(()) => {
                    pool_protocol_failed = true;
                    break;
                }
            }
        }

        for ((binding, expected_logit_count), slot) in bindings
            .into_iter()
            .zip(expected_logit_counts)
            .zip(result_slots)
        {
            let (tensor, tape) = match slot {
                Some(result) if result.panicked => {
                    return Err(NativePolicyScorerFailureV1::ForwardWorker);
                }
                Some(mut result) => {
                    let tape = match (result.tape.take(), result.error.take()) {
                        (Some(tape), None) => tape,
                        (None, Some(error)) => {
                            return Err(NativePolicyScorerFailureV1::PackedForward(error));
                        }
                        _ => return Err(NativePolicyScorerFailureV1::ForwardWorker),
                    };
                    (result.tensor, tape)
                }
                None => return Err(NativePolicyScorerFailureV1::ForwardWorker),
            };
            if tape.logits_v1().len() != expected_logit_count || !tape.value_v1().is_finite() {
                return Err(NativePolicyScorerFailureV1::OutputShape);
            }
            candidate_logits.extend_from_slice(tape.logits_v1());
            candidate_values.push(tape.value_v1());
            candidate_associations.push(NativeScoredDecisionAssociationV1 {
                binding,
                training_input: NativePolicyScoredTrainingInputV1 { tensor, tape },
            });
        }
        if pool_protocol_failed {
            return Err(NativePolicyScorerFailureV1::ForwardWorker);
        }
        if let Some(error) = synchronous_failure {
            return Err(error);
        }
        if submitted != batch.decision_count() {
            return Err(NativePolicyScorerFailureV1::ForwardWorker);
        }
        Ok(())
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

        if self.forward_pool.is_some() && batch.decision_count() > 1 {
            self.score_decisions_parallel_v1(
                batch,
                action_logits.len(),
                &mut candidate_logits,
                &mut candidate_values,
                &mut candidate_associations,
            )?;
        } else {
            self.score_decisions_scalar_v1(
                batch,
                action_logits.len(),
                &mut candidate_logits,
                &mut candidate_values,
                &mut candidate_associations,
            )?;
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

impl FlatBatchScorerV2 for NativePolicyBatchScorerV2 {
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
    FlatGroupedTrajectoryBatchCore<FlatDecisionBindingV2, NativePolicyScoredTrainingInputV1>;

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
    core: FlatPhysicalTrajectoryObserverCore<
        FlatDecisionBindingV2,
        NativePolicyScoredTrainingInputV1,
    >,
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
        let training_input = self
            .associations
            .pop_verified_v1(&event)
            .map_err(NativePolicyTrajectoryErrorV1::Association)?;
        let scorer_action_count = training_input.tape.logits_v1().len();
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
                || training_input,
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
pub(crate) struct NativeTrainerUpdateConfigV2 {
    pub(crate) deck_ids: SessionDeckIdsV1,
    pub(crate) batch_episodes: u64,
    pub(crate) max_physical_decisions: u64,
    pub(crate) max_policy_steps: u64,
    pub(crate) worker_count: usize,
    pub(crate) sessions_per_worker: usize,
    pub(crate) broker_batch_target: usize,
    pub(crate) scheduler_timeout: Duration,
    pub(crate) measure_broker_service_time: bool,
    pub(crate) value_coefficient_bits: u32,
    pub(crate) learning_rate_bits: u32,
    pub(crate) numerical_backend: NativeTrainingNumericalBackendV1,
    pub(crate) backward_worker_limit: usize,
}

#[derive(Clone, Copy)]
struct NativeTrainerGroupedTrainConfigV1 {
    value_coefficient: f32,
    learning_rate: f32,
    recompute_worker_limit: usize,
    numerical_backend: NativeTrainingNumericalBackendV1,
    backward_worker_limit: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeTrainerProgressV2 {
    pub(crate) next_episode_index: u64,
    pub(crate) successful_update_count: u64,
    pub(crate) completed_episode_count: u64,
    pub(crate) learner_physical_decision_count: u64,
    pub(crate) learner_policy_step_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeTrainerEpisodeEvidenceV1 {
    pub episode_index: u64,
    pub learner_seat: PlayerSeatV1,
    pub learner_return: i8,
    pub learner_group_count: u64,
    pub learner_policy_step_count: u64,
    pub learner_trace_hash: u64,
    pub terminal_outcome: TerminalOutcomeV1,
    /// Full both-actor accepted-action commitment. The legacy learner-only
    /// trace remains diagnostic and is not a persisted trajectory identity.
    pub full_trajectory_receipt: NativeFullEpisodeTrajectoryReceiptV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeTrainerSelectedOutputEvidenceV1 {
    pub group_index: usize,
    pub substep_index: usize,
    pub selected_action_index: usize,
    pub selected_logit_bits: u32,
    pub value_bits: u32,
    pub selected_log_probability_bits: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeTrainerPhysicalTermEvidenceV1 {
    pub joint_log_probability_bits: u32,
    pub value_bits: u32,
    pub terminal_return: i8,
    pub substep_count: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativeTrainerUpdateEvidenceV2 {
    pub trainer_contract_identity: &'static str,
    /// End-to-end successful-update wall time, including rollout, inference,
    /// grouping, training, evidence construction, and pre-commit validation.
    pub update_elapsed_ns: u64,
    pub first_episode_index: u64,
    pub episode_count: u64,
    pub physical_decision_count: u64,
    pub policy_step_count: u64,
    pub worker_count: usize,
    pub sessions_per_worker: usize,
    pub logical_actor_count: usize,
    pub broker_batch_target: usize,
    pub episodes: Vec<NativeTrainerEpisodeEvidenceV1>,
    pub learner_group_count: u64,
    pub learner_policy_step_count: u64,
    pub scorer_accepted_batch_count: u64,
    pub scorer_accepted_decision_count: u64,
    pub rollout_metrics: AsyncFlatScoredRolloutMetricsV2,
    pub model_digest_before: String,
    pub model_digest_after: String,
    pub changed_non_gauge_parameter_count: usize,
    pub policy_sum_bits: u32,
    pub value_sum_bits: u32,
    pub loss_bits: u32,
    pub adam_step_before: u64,
    pub adam_step_after: u64,
    pub selected_outputs: Vec<NativeTrainerSelectedOutputEvidenceV1>,
    pub physical_terms: Vec<NativeTrainerPhysicalTermEvidenceV1>,
    pub scorer_bias_gauge: NativeScorerBiasGaugeRecordV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NativeTrainerErrorV1 {
    Schedule(NativeTrainerScheduleErrorV1),
    InvalidUpdateConfig(&'static str),
    ResumeInvariant(&'static str),
    ProgressOutsideU63 {
        field: &'static str,
        value: u64,
    },
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NativeTrainerBootstrapErrorV1 {
    PlaceholderModel(NativePolicyValueErrorV1),
    OptimizerBootstrap(NativePolicyTrainErrorV1),
    Trainer(NativeTrainerErrorV1),
    Snapshot(CommonModelSnapshotErrorV1),
    RunSeedMatchesSnapshotAuthority {
        run_base_seed: u64,
        snapshot_base_seed: u64,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct NativeTrainerStateV2 {
    base_seed: u64,
    batch_episodes: u64,
    train_state: NativePolicyValueTrainStateV1,
    progress: NativeTrainerProgressV2,
    #[cfg(test)]
    pending_test_association_mutation: Option<NativePolicyAssociationTestMutationV1>,
    #[cfg(test)]
    pending_test_train_non_selected_logit_mutation: bool,
    #[cfg(test)]
    pending_test_train_revalidation_mutation: Option<NativePolicyTrainRevalidationTestMutationV1>,
    #[cfg(test)]
    pending_test_forward_worker_panic: bool,
    #[cfg(test)]
    pending_test_physical_substep_count_mutation: bool,
}

impl NativeTrainerStateV2 {
    /// Builds a new trainer around the validated Python-authoritative common
    /// model snapshot. The runner-fixed model and zeroed optimizer are only a
    /// function-local loader target: no trainer is returned unless snapshot
    /// validation, private candidate replacement, optimizer bootstrap, and
    /// run-seed provenance separation all succeed.
    pub(crate) fn from_common_model_snapshot_v2(
        run_base_seed: u64,
        batch_episodes: u64,
        manifest_path: &Path,
        payload_path: &Path,
    ) -> Result<(Self, CommonModelSnapshotRecordV1), NativeTrainerBootstrapErrorV1> {
        let placeholder_model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .map_err(NativeTrainerBootstrapErrorV1::PlaceholderModel)?;
        let placeholder_train_state = NativePolicyValueTrainStateV1::new_v1(placeholder_model)
            .map_err(NativeTrainerBootstrapErrorV1::OptimizerBootstrap)?;
        let mut candidate = Self::new_v2(run_base_seed, batch_episodes, placeholder_train_state)
            .map_err(NativeTrainerBootstrapErrorV1::Trainer)?;
        let record =
            load_common_model_snapshot_v1(manifest_path, payload_path, &mut candidate.train_state)
                .map_err(NativeTrainerBootstrapErrorV1::Snapshot)?;
        if run_base_seed == record.base_seed {
            return Err(
                NativeTrainerBootstrapErrorV1::RunSeedMatchesSnapshotAuthority {
                    run_base_seed,
                    snapshot_base_seed: record.base_seed,
                },
            );
        }
        Ok((candidate, record))
    }

    pub(crate) fn new_v2(
        base_seed: u64,
        batch_episodes: u64,
        train_state: NativePolicyValueTrainStateV1,
    ) -> Result<Self, NativeTrainerErrorV1> {
        let progress = NativeTrainerProgressV2 {
            next_episode_index: 0,
            successful_update_count: 0,
            completed_episode_count: 0,
            learner_physical_decision_count: 0,
            learner_policy_step_count: 0,
        };
        validate_resumed_parts_v2(base_seed, batch_episodes, &train_state, progress)?;
        Ok(Self {
            base_seed,
            batch_episodes,
            train_state,
            progress,
            #[cfg(test)]
            pending_test_association_mutation: None,
            #[cfg(test)]
            pending_test_train_non_selected_logit_mutation: false,
            #[cfg(test)]
            pending_test_train_revalidation_mutation: None,
            #[cfg(test)]
            pending_test_forward_worker_panic: false,
            #[cfg(test)]
            pending_test_physical_substep_count_mutation: false,
        })
    }

    /// Reconstructs a trainer only after validating the persisted batch binding,
    /// complete decoded train state, progress arithmetic, and next full schedule.
    /// The caller retains the borrowed candidate unchanged on every rejection.
    pub(crate) fn from_resumed_parts_v2(
        base_seed: u64,
        batch_episodes: u64,
        train_state: &NativePolicyValueTrainStateV1,
        progress: NativeTrainerProgressV2,
    ) -> Result<Self, NativeTrainerErrorV1> {
        validate_resumed_parts_v2(base_seed, batch_episodes, train_state, progress)?;
        Ok(Self {
            base_seed,
            batch_episodes,
            // The only ownership acquisition in the resume path. Every
            // fallible validation above has already completed.
            train_state: train_state.clone(),
            progress,
            #[cfg(test)]
            pending_test_association_mutation: None,
            #[cfg(test)]
            pending_test_train_non_selected_logit_mutation: false,
            #[cfg(test)]
            pending_test_train_revalidation_mutation: None,
            #[cfg(test)]
            pending_test_forward_worker_panic: false,
            #[cfg(test)]
            pending_test_physical_substep_count_mutation: false,
        })
    }

    pub(crate) fn base_seed_v2(&self) -> u64 {
        self.base_seed
    }

    pub(crate) fn progress_v2(&self) -> NativeTrainerProgressV2 {
        self.progress
    }

    pub(crate) fn train_state_v1(&self) -> &NativePolicyValueTrainStateV1 {
        &self.train_state
    }

    #[cfg(test)]
    pub(crate) fn mutate_optimizer_moment_for_preclone_test_v2(&mut self) {
        self.train_state
            .mutate_optimizer_moment_for_preclone_test_v2();
    }

    #[cfg(test)]
    pub(crate) fn mutate_model_parameter_for_preclone_test_v2(&mut self) {
        self.train_state
            .mutate_model_parameter_for_preclone_test_v2();
    }

    #[cfg(test)]
    pub(crate) fn mutate_progress_for_preclone_test_v2(&mut self) {
        self.progress.learner_policy_step_count = self
            .progress
            .learner_policy_step_count
            .checked_add(1)
            .expect("test-only progress mutation must remain representable");
        assert!(validate_resumed_parts_v2(
            self.base_seed,
            self.batch_episodes,
            &self.train_state,
            self.progress,
        )
        .is_ok());
    }

    #[cfg(test)]
    pub(crate) fn mutate_scorer_anchor_for_preclone_test_v2(&mut self) {
        self.train_state.mutate_scorer_anchor_for_preclone_test_v2();
    }

    pub(crate) fn run_even_batch_update_v2(
        &mut self,
        config: &NativeTrainerUpdateConfigV2,
    ) -> Result<NativeTrainerUpdateEvidenceV2, NativeTrainerErrorV1> {
        let mut phase_recorder = NativeTrainingPhaseRecorderV1::disabled_v1();
        self.run_even_batch_update_inner_v2(config, &mut phase_recorder)
    }

    pub(crate) fn run_even_batch_update_profiled_v2(
        &mut self,
        config: &NativeTrainerUpdateConfigV2,
    ) -> Result<(NativeTrainerUpdateEvidenceV2, NativeTrainingPhaseProfileV1), NativeTrainerErrorV1>
    {
        let mut profile = NativeTrainingPhaseProfileV1::default();
        let evidence = {
            let mut phase_recorder = NativeTrainingPhaseRecorderV1::enabled_v1(&mut profile);
            self.run_even_batch_update_inner_v2(config, &mut phase_recorder)?
        };
        Ok((evidence, profile))
    }

    fn run_even_batch_update_inner_v2(
        &mut self,
        config: &NativeTrainerUpdateConfigV2,
        phase_recorder: &mut NativeTrainingPhaseRecorderV1<'_>,
    ) -> Result<NativeTrainerUpdateEvidenceV2, NativeTrainerErrorV1> {
        let update_started = Instant::now();
        let setup_timer = phase_recorder.start_v1(NativeTrainingPhaseV1::SetupValidation);
        #[cfg(test)]
        let test_mutation = self.pending_test_association_mutation.take();
        #[cfg(test)]
        let test_train_non_selected_logit_mutation =
            std::mem::take(&mut self.pending_test_train_non_selected_logit_mutation);
        #[cfg(test)]
        let test_train_revalidation_mutation = self.pending_test_train_revalidation_mutation.take();
        #[cfg(test)]
        let test_forward_worker_panic = std::mem::take(&mut self.pending_test_forward_worker_panic);
        #[cfg(test)]
        let test_physical_substep_count_mutation =
            std::mem::take(&mut self.pending_test_physical_substep_count_mutation);
        validate_update_config_v2(config)?;
        if config.batch_episodes != self.batch_episodes {
            return Err(NativeTrainerErrorV1::InvalidUpdateConfig("batch_episodes"));
        }
        let expected_deck_hashes = [
            runtime_deck_by_id(&config.deck_ids[0])
                .ok_or(NativeTrainerErrorV1::InvalidUpdateConfig("deck_ids"))?
                .runtime_deck_hash,
            runtime_deck_by_id(&config.deck_ids[1])
                .ok_or(NativeTrainerErrorV1::InvalidUpdateConfig("deck_ids"))?
                .runtime_deck_hash,
        ];
        let logical_actor_count = config
            .worker_count
            .checked_mul(config.sessions_per_worker)
            .ok_or(NativeTrainerErrorV1::CounterOverflow)?;
        if self.progress.next_episode_index & 1 != 0 {
            return Err(NativeTrainerErrorV1::GroupingInvariant(
                "next episode must begin an even/odd parity pair",
            ));
        }
        let first_episode_index = self.progress.next_episode_index;
        let end_episode_index = first_episode_index
            .checked_add(config.batch_episodes)
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
            episode_count: config.batch_episodes,
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
            config.batch_episodes,
            self.base_seed,
            expected_deck_hashes,
            consumer,
        )
        .map_err(NativeTrainerErrorV1::ObserverConstruction)?;
        let mut scorer = NativePolicyBatchScorerV2::new_v1(
            self.train_state.model_v1(),
            producer,
            config.broker_batch_target.min(logical_actor_count),
        )
        .map_err(NativeTrainerErrorV1::Train)?;
        #[cfg(test)]
        if test_forward_worker_panic {
            scorer.force_next_parallel_worker_panic = true;
        }
        phase_recorder.finish_v1(setup_timer);
        let rollout_timer = phase_recorder.start_v1(NativeTrainingPhaseV1::Rollout);
        let rollout_result = run_async_flat_scored_rollout_native_observed_v2(
            rollout_config,
            self.base_seed,
            &mut scorer,
            observer,
        );
        phase_recorder.finish_v1(rollout_timer);
        let scorer_accepted_batch_count = scorer.accepted_batch_count;
        let scorer_accepted_decision_count = scorer.accepted_decision_count;
        #[cfg(test)]
        let scorer_forward_call_count = scorer.forward_builder.forward_call_count_for_test_v1();
        let scorer_failure = scorer.last_failure.clone();
        if phase_recorder.is_enabled_v1() {
            let cleanup_timer = phase_recorder.start_v1(NativeTrainingPhaseV1::CleanupDrop);
            drop(scorer);
            phase_recorder.finish_v1(cleanup_timer);
        } else {
            drop(scorer);
        }
        let grouping_timer =
            phase_recorder.start_v1(NativeTrainingPhaseV1::GroupingMaterialization);
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
        validate_scorer_rollout_counters_v2(
            scorer_accepted_batch_count,
            scorer_accepted_decision_count,
            &rollout.metrics,
        )?;
        #[cfg(test)]
        assert_eq!(
            scorer_forward_call_count, scorer_accepted_decision_count,
            "the shared scorer builder must run exactly once per accepted decision"
        );
        let NativePolicyObservedTrajectoryV1 {
            grouped,
            full_trajectory_receipts,
        } = observed_trajectory;
        validate_grouped_batch_v2(&grouped, first_episode_index, config.batch_episodes)?;
        let expected_episode_count = usize::try_from(config.batch_episodes)
            .map_err(|_| NativeTrainerErrorV1::CounterOverflow)?;
        if !rollout.all_natural() || rollout.episodes.len() != expected_episode_count {
            return Err(NativeTrainerErrorV1::GroupingInvariant(
                "rollout must contain exactly the configured natural episodes",
            ));
        }
        #[cfg(test)]
        let mut grouped = grouped;
        #[cfg(test)]
        if test_train_non_selected_logit_mutation {
            mutate_grouped_non_selected_logit_for_test_v1(&mut grouped)?;
        }
        #[cfg(test)]
        if let Some(mutation) = test_train_revalidation_mutation {
            mutate_grouped_train_revalidation_for_test_v1(&mut grouped, mutation)?;
        }

        let model_digest_before = self.train_state.model_v1().parameter_manifest_sha256_v1();
        let parameters_before = self.train_state.model_v1().parameter_snapshot_v1();
        let adam_step_before = self.train_state.adam_step_v1();
        let mut candidate_train_state = self.train_state.clone();
        phase_recorder.finish_v1(grouping_timer);
        let (train_result, episode_evidence, learner_group_count) = train_grouped_candidate_v1(
            &mut candidate_train_state,
            &grouped,
            &full_trajectory_receipts,
            NativeTrainerGroupedTrainConfigV1 {
                value_coefficient: f32::from_bits(config.value_coefficient_bits),
                learning_rate: f32::from_bits(config.learning_rate_bits),
                recompute_worker_limit: config.worker_count,
                numerical_backend: config.numerical_backend,
                backward_worker_limit: config.backward_worker_limit,
            },
            #[cfg(test)]
            test_physical_substep_count_mutation,
            phase_recorder,
        )?;
        let finalization_timer =
            phase_recorder.start_v1(NativeTrainingPhaseV1::FinalizationCloning);
        let parameters_after = candidate_train_state.model_v1().parameter_snapshot_v1();
        let model_digest_after = candidate_train_state
            .model_v1()
            .parameter_manifest_sha256_v1();
        let changed_non_gauge_parameter_count =
            changed_non_gauge_parameters_v1(&parameters_before, &parameters_after)?;

        let next_progress = progress_after_successful_update_v2(
            self.progress,
            self.batch_episodes,
            learner_group_count,
            grouped.learner_policy_step_count,
        )?;
        if next_progress.next_episode_index != end_episode_index {
            return Err(NativeTrainerErrorV1::GroupingInvariant(
                "progress helper must advance the configured batch exactly once",
            ));
        }
        let expected_adam_step = adam_step_before
            .checked_add(1)
            .ok_or(NativeTrainerErrorV1::CounterOverflow)?;
        if train_result.adam_step != expected_adam_step {
            return Err(NativeTrainerErrorV1::GroupingInvariant(
                "one grouped batch must advance Adam exactly once",
            ));
        }
        phase_recorder.finish_v1(finalization_timer);

        let NativePolicyTrainStepResultV1 {
            policy_sum,
            value_sum,
            loss,
            adam_step,
            selected_outputs: source_selected_outputs,
            physical_terms: source_physical_terms,
            gradients,
            scorer_bias_gauge,
        } = train_result;
        let evidence_timer = phase_recorder.start_v1(NativeTrainingPhaseV1::EvidenceConstruction);
        let selected_outputs = source_selected_outputs
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
        let physical_terms = source_physical_terms
            .iter()
            .map(|term| NativeTrainerPhysicalTermEvidenceV1 {
                joint_log_probability_bits: term.joint_log_probability.to_bits(),
                value_bits: term.value.to_bits(),
                terminal_return: term.terminal_return,
                substep_count: term.substep_count,
            })
            .collect();
        let mut evidence = NativeTrainerUpdateEvidenceV2 {
            trainer_contract_identity: NATIVE_TRAINER_CONTRACT_IDENTITY_V2,
            update_elapsed_ns: 0,
            first_episode_index,
            episode_count: config.batch_episodes,
            physical_decision_count: rollout.physical_decision_count,
            policy_step_count: rollout.policy_step_count,
            worker_count: config.worker_count,
            sessions_per_worker: config.sessions_per_worker,
            logical_actor_count,
            broker_batch_target: config.broker_batch_target,
            episodes: episode_evidence,
            learner_group_count,
            learner_policy_step_count: grouped.learner_policy_step_count,
            scorer_accepted_batch_count,
            scorer_accepted_decision_count,
            rollout_metrics: rollout.metrics,
            model_digest_before,
            model_digest_after,
            changed_non_gauge_parameter_count,
            policy_sum_bits: policy_sum.to_bits(),
            value_sum_bits: value_sum.to_bits(),
            loss_bits: loss.to_bits(),
            adam_step_before,
            adam_step_after: adam_step,
            selected_outputs,
            physical_terms,
            scorer_bias_gauge,
        };
        phase_recorder.finish_v1(evidence_timer);

        if phase_recorder.is_enabled_v1() {
            let cleanup_timer = phase_recorder.start_v1(NativeTrainingPhaseV1::CleanupDrop);
            drop(source_selected_outputs);
            drop(source_physical_terms);
            drop(gradients);
            drop(parameters_before);
            drop(parameters_after);
            drop(grouped);
            drop(full_trajectory_receipts);
            drop(rollout);
            phase_recorder.finish_v1(cleanup_timer);
        }

        // The only live-state commit in the update path. Every rollout,
        // association, grouping, recomputation, train, parameter, optimizer,
        // evidence, and counter check above completed on owned candidates.
        let commit_timer = phase_recorder.start_v1(NativeTrainingPhaseV1::FinalizationCloning);
        self.train_state = candidate_train_state;
        self.progress = next_progress;
        phase_recorder.finish_v1(commit_timer);
        evidence.update_elapsed_ns =
            u64::try_from(update_started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        phase_recorder.finish_update_v1(evidence.update_elapsed_ns);
        Ok(evidence)
    }

    #[cfg(test)]
    fn run_even_batch_update_with_mutation_v2(
        &mut self,
        config: &NativeTrainerUpdateConfigV2,
        mutation: NativePolicyAssociationTestMutationV1,
    ) -> Result<NativeTrainerUpdateEvidenceV2, NativeTrainerErrorV1> {
        assert!(self.pending_test_association_mutation.is_none());
        self.pending_test_association_mutation = Some(mutation);
        self.run_even_batch_update_v2(config)
    }

    #[cfg(test)]
    fn run_even_batch_update_with_train_non_selected_logit_mutation_v2(
        &mut self,
        config: &NativeTrainerUpdateConfigV2,
    ) -> Result<NativeTrainerUpdateEvidenceV2, NativeTrainerErrorV1> {
        assert!(!self.pending_test_train_non_selected_logit_mutation);
        self.pending_test_train_non_selected_logit_mutation = true;
        self.run_even_batch_update_v2(config)
    }

    #[cfg(test)]
    fn run_even_batch_update_with_train_revalidation_mutation_v2(
        &mut self,
        config: &NativeTrainerUpdateConfigV2,
        mutation: NativePolicyTrainRevalidationTestMutationV1,
    ) -> Result<NativeTrainerUpdateEvidenceV2, NativeTrainerErrorV1> {
        assert!(self.pending_test_train_revalidation_mutation.is_none());
        self.pending_test_train_revalidation_mutation = Some(mutation);
        self.run_even_batch_update_v2(config)
    }

    #[cfg(test)]
    fn run_even_batch_update_with_forward_worker_panic_v2(
        &mut self,
        config: &NativeTrainerUpdateConfigV2,
    ) -> Result<NativeTrainerUpdateEvidenceV2, NativeTrainerErrorV1> {
        assert!(!self.pending_test_forward_worker_panic);
        self.pending_test_forward_worker_panic = true;
        self.run_even_batch_update_v2(config)
    }

    #[cfg(test)]
    fn run_even_batch_update_with_physical_substep_count_mutation_v2(
        &mut self,
        config: &NativeTrainerUpdateConfigV2,
    ) -> Result<NativeTrainerUpdateEvidenceV2, NativeTrainerErrorV1> {
        assert!(!self.pending_test_physical_substep_count_mutation);
        self.pending_test_physical_substep_count_mutation = true;
        self.run_even_batch_update_v2(config)
    }
}

fn validate_progress_u63_v2(progress: NativeTrainerProgressV2) -> Result<(), NativeTrainerErrorV1> {
    for (field, value) in [
        ("next_episode_index", progress.next_episode_index),
        ("successful_update_count", progress.successful_update_count),
        ("completed_episode_count", progress.completed_episode_count),
        (
            "learner_physical_decision_count",
            progress.learner_physical_decision_count,
        ),
        (
            "learner_policy_step_count",
            progress.learner_policy_step_count,
        ),
    ] {
        if value > NATIVE_TRAINER_U63_MAX_V2 {
            return Err(NativeTrainerErrorV1::ProgressOutsideU63 { field, value });
        }
    }
    Ok(())
}

fn progress_after_successful_update_v2(
    progress: NativeTrainerProgressV2,
    batch_episodes: u64,
    learner_physical_decision_count: u64,
    learner_policy_step_count: u64,
) -> Result<NativeTrainerProgressV2, NativeTrainerErrorV1> {
    let next_progress = NativeTrainerProgressV2 {
        next_episode_index: progress
            .next_episode_index
            .checked_add(batch_episodes)
            .ok_or(NativeTrainerErrorV1::CounterOverflow)?,
        successful_update_count: progress
            .successful_update_count
            .checked_add(1)
            .ok_or(NativeTrainerErrorV1::CounterOverflow)?,
        completed_episode_count: progress
            .completed_episode_count
            .checked_add(batch_episodes)
            .ok_or(NativeTrainerErrorV1::CounterOverflow)?,
        learner_physical_decision_count: progress
            .learner_physical_decision_count
            .checked_add(learner_physical_decision_count)
            .ok_or(NativeTrainerErrorV1::CounterOverflow)?,
        learner_policy_step_count: progress
            .learner_policy_step_count
            .checked_add(learner_policy_step_count)
            .ok_or(NativeTrainerErrorV1::CounterOverflow)?,
    };
    validate_progress_u63_v2(next_progress)?;
    Ok(next_progress)
}

pub(crate) fn validate_resumed_parts_v2(
    base_seed: u64,
    batch_episodes: u64,
    train_state: &NativePolicyValueTrainStateV1,
    progress: NativeTrainerProgressV2,
) -> Result<(), NativeTrainerErrorV1> {
    validate_batch_episodes_v2(batch_episodes)?;
    train_state
        .validate_state_v1()
        .map_err(NativeTrainerErrorV1::Train)?;
    validate_progress_u63_v2(progress)?;

    if progress.next_episode_index & 1 != 0 {
        return Err(NativeTrainerErrorV1::ResumeInvariant(
            "next episode must begin an even/odd parity pair",
        ));
    }
    if progress.next_episode_index != progress.completed_episode_count {
        return Err(NativeTrainerErrorV1::ResumeInvariant(
            "next episode must equal completed episode count",
        ));
    }
    let expected_completed_episode_count = progress
        .successful_update_count
        .checked_mul(batch_episodes)
        .ok_or(NativeTrainerErrorV1::CounterOverflow)?;
    if progress.completed_episode_count != expected_completed_episode_count {
        return Err(NativeTrainerErrorV1::ResumeInvariant(
            "completed episode count must equal successful updates times persisted batch episodes",
        ));
    }
    if train_state.adam_step_v1() != progress.successful_update_count {
        return Err(NativeTrainerErrorV1::ResumeInvariant(
            "Adam step must equal successful update count",
        ));
    }

    let final_episode_index = progress
        .next_episode_index
        .checked_add(batch_episodes - 1)
        .ok_or(NativeTrainerErrorV1::CounterOverflow)?;
    native_trainer_episode_schedule_v1(base_seed, progress.next_episode_index)
        .map_err(NativeTrainerErrorV1::Schedule)?;
    native_trainer_episode_schedule_v1(base_seed, final_episode_index)
        .map_err(NativeTrainerErrorV1::Schedule)?;
    Ok(())
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

#[cfg(test)]
fn mutate_grouped_train_revalidation_for_test_v1(
    grouped: &mut NativePolicyGroupedTrajectoryV1,
    mutation: NativePolicyTrainRevalidationTestMutationV1,
) -> Result<(), NativeTrainerErrorV1> {
    let episode_offset = match mutation {
        NativePolicyTrainRevalidationTestMutationV1::ExpectedLogitCount { episode_offset }
        | NativePolicyTrainRevalidationTestMutationV1::Logit { episode_offset }
        | NativePolicyTrainRevalidationTestMutationV1::Value { episode_offset } => episode_offset,
    };
    let episode =
        grouped
            .episodes
            .get_mut(episode_offset)
            .ok_or(NativeTrainerErrorV1::GroupingInvariant(
                "test mutation episode offset is out of range",
            ))?;
    if matches!(
        mutation,
        NativePolicyTrainRevalidationTestMutationV1::Logit { .. }
    ) {
        for group in &mut episode.groups {
            for substep in &mut group.substeps {
                let selected = usize::try_from(substep.selected_index).map_err(|_| {
                    NativeTrainerErrorV1::GroupingInvariant(
                        "test mutation selected index is out of range",
                    )
                })?;
                if substep.raw_action_logit_bits.len() > 1 {
                    let action_index = if selected == 0 { 1 } else { 0 };
                    substep.raw_action_logit_bits[action_index] ^= 1;
                    return Ok(());
                }
            }
        }
        return Err(NativeTrainerErrorV1::GroupingInvariant(
            "test requires a non-selected action row in the requested episode",
        ));
    }
    for group in &mut episode.groups {
        if let Some(substep) = group.substeps.first_mut() {
            match mutation {
                NativePolicyTrainRevalidationTestMutationV1::ExpectedLogitCount { .. } => {
                    substep.raw_action_logit_bits.pop().ok_or(
                        NativeTrainerErrorV1::GroupingInvariant(
                            "test requires a nonempty expected-logit vector",
                        ),
                    )?;
                }
                NativePolicyTrainRevalidationTestMutationV1::Logit { .. } => {
                    unreachable!("logit mutation returns before the first-substep mutation path")
                }
                NativePolicyTrainRevalidationTestMutationV1::Value { .. } => {
                    substep.predicted_value_bits ^= 1;
                }
            }
            return Ok(());
        }
    }
    Err(NativeTrainerErrorV1::GroupingInvariant(
        "test requires a learner substep",
    ))
}

fn validate_batch_episodes_v2(batch_episodes: u64) -> Result<(), NativeTrainerErrorV1> {
    if !(NATIVE_TRAINER_MIN_BATCH_EPISODES_V2..=NATIVE_TRAINER_MAX_BATCH_EPISODES_V2)
        .contains(&batch_episodes)
        || batch_episodes & 1 != 0
    {
        return Err(NativeTrainerErrorV1::InvalidUpdateConfig("batch_episodes"));
    }
    Ok(())
}

pub(crate) fn validate_update_config_v2(
    config: &NativeTrainerUpdateConfigV2,
) -> Result<(), NativeTrainerErrorV1> {
    validate_batch_episodes_v2(config.batch_episodes)?;
    if config.deck_ids.iter().any(String::is_empty) {
        return Err(NativeTrainerErrorV1::InvalidUpdateConfig("deck_ids"));
    }
    if !(1..=ASYNC_ROLLOUT_MAX_WORKERS_V2).contains(&config.worker_count) {
        return Err(NativeTrainerErrorV1::InvalidUpdateConfig("worker_count"));
    }
    if !(1..=ASYNC_ROLLOUT_MAX_SESSIONS_PER_WORKER_V2).contains(&config.sessions_per_worker) {
        return Err(NativeTrainerErrorV1::InvalidUpdateConfig(
            "sessions_per_worker",
        ));
    }
    let logical_actor_count = config
        .worker_count
        .checked_mul(config.sessions_per_worker)
        .ok_or(NativeTrainerErrorV1::CounterOverflow)?;
    if !(1..=logical_actor_count).contains(&config.broker_batch_target) {
        return Err(NativeTrainerErrorV1::InvalidUpdateConfig(
            "broker_batch_target",
        ));
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
    if !config
        .numerical_backend
        .accepts_backward_worker_limit_v1(config.backward_worker_limit)
    {
        return Err(NativeTrainerErrorV1::InvalidUpdateConfig(
            "backward_worker_limit",
        ));
    }
    Ok(())
}

fn validate_scorer_rollout_counters_v2(
    scorer_accepted_batch_count: u64,
    scorer_accepted_decision_count: u64,
    rollout_metrics: &AsyncFlatScoredRolloutMetricsV2,
) -> Result<(), NativeTrainerErrorV1> {
    if scorer_accepted_batch_count != rollout_metrics.scorer_batch_count
        || scorer_accepted_decision_count != rollout_metrics.scored_decision_count
        || scorer_accepted_decision_count != rollout_metrics.sampled_action_count
        || scorer_accepted_decision_count != rollout_metrics.batch_width_sum
    {
        return Err(NativeTrainerErrorV1::GroupingInvariant(
            "scorer accepted counters must exactly match rollout counters",
        ));
    }
    Ok(())
}

fn validate_grouped_batch_v2(
    grouped: &NativePolicyGroupedTrajectoryV1,
    first_episode_index: u64,
    batch_episodes: u64,
) -> Result<(), NativeTrainerErrorV1> {
    let expected_episode_count =
        usize::try_from(batch_episodes).map_err(|_| NativeTrainerErrorV1::CounterOverflow)?;
    if grouped.learner_seat_rule != FlatPhysicalLearnerSeatRuleCore::EpisodeParity
        || grouped.first_episode_id != first_episode_index
        || grouped.episode_count != batch_episodes
        || grouped.episodes.len() != expected_episode_count
    {
        return Err(NativeTrainerErrorV1::GroupingInvariant(
            "alternating-seat batch envelope",
        ));
    }
    for (offset, episode) in grouped.episodes.iter().enumerate() {
        let expected_episode = first_episode_index
            .checked_add(u64::try_from(offset).map_err(|_| NativeTrainerErrorV1::CounterOverflow)?)
            .ok_or(NativeTrainerErrorV1::CounterOverflow)?;
        let expected_seat = if expected_episode & 1 == 0 {
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
    execution: NativeTrainerGroupedTrainConfigV1,
    #[cfg(test)] test_physical_substep_count_mutation: bool,
    phase_recorder: &mut NativeTrainingPhaseRecorderV1<'_>,
) -> Result<
    (
        NativePolicyTrainStepResultV1,
        Vec<NativeTrainerEpisodeEvidenceV1>,
        u64,
    ),
    NativeTrainerErrorV1,
> {
    let grouping_timer = phase_recorder.start_v1(NativeTrainingPhaseV1::GroupingMaterialization);
    let mut source_groups = Vec::new();
    let mut terminal_returns = Vec::new();
    let episode_capacity = usize::try_from(grouped.episode_count)
        .map_err(|_| NativeTrainerErrorV1::CounterOverflow)?;
    if full_trajectory_receipts.len() != episode_capacity {
        return Err(NativeTrainerErrorV1::GroupingInvariant(
            "full trajectory receipt count",
        ));
    }
    let mut episode_evidence = Vec::with_capacity(episode_capacity);
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
                        forward: NativePolicyForwardInputV1::Packed {
                            encoded: Box::new(native_encoded_decision_view_v1(
                                &substep.scoring_inputs.tensor,
                            )),
                            tape: &substep.scoring_inputs.tape,
                        },
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
    phase_recorder.finish_v1(grouping_timer);
    let result = match execution.numerical_backend {
        NativeTrainingNumericalBackendV1::Sequential => candidate
            .train_step_with_recompute_workers_profiled_v1(
                &borrowed_groups,
                execution.value_coefficient,
                execution.learning_rate,
                execution.recompute_worker_limit,
                phase_recorder,
            ),
        NativeTrainingNumericalBackendV1::FixedFourPartitions => candidate
            .train_step_with_fixed_partition_parallel_backward_profiled_v1(
                &borrowed_groups,
                execution.value_coefficient,
                execution.learning_rate,
                execution.recompute_worker_limit,
                execution.backward_worker_limit,
                phase_recorder,
            ),
        // The device-resident CUDA path is wired at the executor level, not
        // through the CPU candidate; reaching this arm means the executor
        // dispatched a CUDA-configured update onto the CPU train path.
        NativeTrainingNumericalBackendV1::CudaBurnDense => {
            return Err(NativeTrainerErrorV1::InvalidUpdateConfig(
                "cuda-burn-dense-backend-not-wired-into-the-cpu-train-path",
            ));
        }
    }
    .map_err(NativeTrainerErrorV1::Train)?;
    #[cfg(test)]
    let mut result = result;
    #[cfg(test)]
    if test_physical_substep_count_mutation {
        let term =
            result
                .physical_terms
                .first_mut()
                .ok_or(NativeTrainerErrorV1::GroupingInvariant(
                    "test requires one physical loss term",
                ))?;
        term.substep_count ^= 1;
    }
    let finalization_timer = phase_recorder.start_v1(NativeTrainingPhaseV1::FinalizationCloning);
    verify_recomputed_outputs_v1(&source_groups, &terminal_returns, &result)?;
    if episode_evidence.len() != episode_capacity {
        return Err(NativeTrainerErrorV1::GroupingInvariant(
            "episode evidence count",
        ));
    }
    phase_recorder.finish_v1(finalization_timer);
    if phase_recorder.is_enabled_v1() {
        let cleanup_timer = phase_recorder.start_v1(NativeTrainingPhaseV1::CleanupDrop);
        drop(borrowed_groups);
        drop(borrowed_substeps);
        drop(source_groups);
        drop(terminal_returns);
        phase_recorder.finish_v1(cleanup_timer);
    }
    Ok((result, episode_evidence, learner_group_count))
}

fn verify_recomputed_outputs_v1(
    source_groups: &[&crate::private_physical_trajectory_core::FlatPhysicalDecisionSampleCore<
        FlatDecisionBindingV2,
        NativePolicyScoredTrainingInputV1,
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
    let mut selected_output_group_counts = vec![0_u32; source_groups.len()];
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
            selected_output_group_counts[output.group_index] = selected_output_group_counts
                [output.group_index]
                .checked_add(1)
                .ok_or(NativeTrainerErrorV1::CounterOverflow)?;
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
        verify_physical_term_substep_count_v1(
            group_index,
            group.substeps.len(),
            selected_output_group_counts[group_index],
            term.substep_count,
        )?;
    }
    Ok(())
}

fn verify_physical_term_substep_count_v1(
    group_index: usize,
    direct_group_substep_count: usize,
    selected_output_substep_count: u32,
    recorded_substep_count: u32,
) -> Result<(), NativeTrainerErrorV1> {
    let direct_group_substep_count = u32::try_from(direct_group_substep_count)
        .map_err(|_| NativeTrainerErrorV1::CounterOverflow)?;
    if direct_group_substep_count == 0
        || direct_group_substep_count != selected_output_substep_count
        || direct_group_substep_count != recorded_substep_count
    {
        return Err(NativeTrainerErrorV1::RecomputedOutputMismatch {
            field: "substep_count",
            group_index,
            substep_index: 0,
        });
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
    use crate::async_flat_scored_rollout_v1::acquire_async_flat_scored_test_lock_v1;
    use crate::common_model_snapshot_v1::{
        common_model_snapshot_paths_v1, BASE_SEED_V1 as SNAPSHOT_AUTHORITY_BASE_SEED_V1,
        MODEL_INIT_SEED_V1 as SNAPSHOT_MODEL_INIT_SEED_V1, SNAPSHOT_IDENTITY_V1,
    };
    use crate::native_policy_train_step_v1::NativePolicyValueTrainStateV1;
    use crate::native_policy_value_net_v1::{
        NativePolicyValueModelConfigV1, NativePolicyValueNetV1,
    };
    use crate::native_train_state_payload_v1::{
        decode_native_train_state_payload_verified_v1, encode_native_train_state_payload_v1,
    };
    use crate::native_trainer_schedule_v1::derive_native_trainer_model_init_seed_v1;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SNAPSHOT_CORRUPTION_ORDINAL_V1: AtomicU64 = AtomicU64::new(0);

    struct CorruptedSnapshotPayloadV1 {
        path: PathBuf,
    }

    impl CorruptedSnapshotPayloadV1 {
        fn new_v1() -> Self {
            let ordinal = SNAPSHOT_CORRUPTION_ORDINAL_V1.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "mtg-kernel-native-trainer-corrupt-snapshot-v1-{}-{ordinal}.f32le",
                std::process::id()
            ));
            let mut payload =
                include_bytes!("../../data/common_model_snapshot_v1/parameters.f32le").to_vec();
            payload[0] ^= 1;
            fs::write(&path, payload).expect("write isolated corrupted snapshot payload");
            Self { path }
        }
    }

    impl Drop for CorruptedSnapshotPayloadV1 {
        fn drop(&mut self) {
            fs::remove_file(&self.path).expect("remove isolated corrupted snapshot payload");
        }
    }

    fn burn_pair_config_v2(
        worker_count: usize,
        sessions_per_worker: usize,
        broker_batch_target: usize,
    ) -> NativeTrainerUpdateConfigV2 {
        NativeTrainerUpdateConfigV2 {
            deck_ids: ["Burn".to_owned(), "Burn".to_owned()],
            batch_episodes: 2,
            max_physical_decisions: 5_000,
            max_policy_steps: 640_000,
            worker_count,
            sessions_per_worker,
            broker_batch_target,
            scheduler_timeout: Duration::from_secs(600),
            measure_broker_service_time: false,
            value_coefficient_bits: 0.5f32.to_bits(),
            learning_rate_bits: 0.001f32.to_bits(),
            numerical_backend: NativeTrainingNumericalBackendV1::Sequential,
            backward_worker_limit: 1,
        }
    }

    fn burn_even_batch_config_v2(
        batch_episodes: u64,
        worker_count: usize,
        sessions_per_worker: usize,
        broker_batch_target: usize,
    ) -> NativeTrainerUpdateConfigV2 {
        let mut config =
            burn_pair_config_v2(worker_count, sessions_per_worker, broker_batch_target);
        config.batch_episodes = batch_episodes;
        config
    }

    fn trainer_v2(batch_episodes: u64) -> NativeTrainerStateV2 {
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let train_state = NativePolicyValueTrainStateV1::new_v1(model).unwrap();
        NativeTrainerStateV2::new_v2(71_501, batch_episodes, train_state).unwrap()
    }

    fn exact_state_snapshot_v1(
        trainer: &NativeTrainerStateV2,
    ) -> (
        NativeTrainerProgressV2,
        u64,
        u32,
        Vec<NativeNamedParameterV1>,
        Vec<NativeNamedParameterV1>,
        Vec<NativeNamedParameterV1>,
    ) {
        (
            trainer.progress_v2(),
            trainer.train_state_v1().adam_step_v1(),
            trainer.train_state_v1().scorer_bias_anchor_f32_bits_v1(),
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
    fn physical_term_substep_count_requires_direct_positive_histogram_match() {
        assert_eq!(verify_physical_term_substep_count_v1(7, 2, 2, 2), Ok(()));
        for (direct, selected, recorded) in [(0, 0, 0), (2, 1, 2), (2, 2, 1)] {
            assert_eq!(
                verify_physical_term_substep_count_v1(7, direct, selected, recorded),
                Err(NativeTrainerErrorV1::RecomputedOutputMismatch {
                    field: "substep_count",
                    group_index: 7,
                    substep_index: 0,
                })
            );
        }
        #[cfg(target_pointer_width = "64")]
        assert_eq!(
            verify_physical_term_substep_count_v1(7, u32::MAX as usize + 1, 1, 1),
            Err(NativeTrainerErrorV1::CounterOverflow)
        );
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct BurnPairNumericalWitnessV1<'a> {
        train_state_sha256: [u8; 32],
        model_digest_after: &'a str,
        policy_sum_bits: u32,
        value_sum_bits: u32,
        loss_bits: u32,
    }

    fn recorded_burn_pair_numerical_witness_v1(
    ) -> (&'static str, BurnPairNumericalWitnessV1<'static>) {
        // The trainer intentionally uses the target's f32 transcendental
        // implementations. Their last-bit differences are immaterial to the
        // declared numerical tolerances but become visible in an exact
        // optimizer-state digest. These witnesses are scoped to the repository-
        // pinned Rust toolchain and named target tuple. Keep each tuple exact
        // and fail closed instead of silently applying one target's witness to
        // another.
        #[cfg(all(target_arch = "x86_64", target_os = "windows", target_env = "msvc"))]
        {
            (
                "x86_64-pc-windows-msvc",
                BurnPairNumericalWitnessV1 {
                    train_state_sha256: [
                        250, 165, 172, 135, 179, 143, 5, 205, 138, 114, 252, 103, 138, 241, 177,
                        197, 117, 96, 251, 190, 79, 49, 165, 11, 15, 249, 71, 182, 127, 49, 170,
                        141,
                    ],
                    model_digest_after:
                        "5dcf4eff6f0bce4d5c38f9d3eeb84f0a33afd9db67a8969dfc4360b9df35d443",
                    policy_sum_bits: 1_111_603_742,
                    value_sum_bits: 1_121_934_211,
                    loss_bits: 1_064_195_456,
                },
            )
        }
        #[cfg(all(target_arch = "x86_64", target_os = "linux", target_env = "gnu"))]
        {
            (
                "x86_64-unknown-linux-gnu",
                BurnPairNumericalWitnessV1 {
                    train_state_sha256: [
                        123, 200, 0, 83, 51, 3, 54, 216, 47, 5, 112, 187, 4, 74, 137, 69, 67, 101,
                        49, 78, 192, 135, 162, 81, 61, 143, 123, 166, 225, 191, 172, 17,
                    ],
                    model_digest_after:
                        "40eafa2be6624d0126e5aaf704441034f6186799c4235f7b7c513b7d3628f06d",
                    policy_sum_bits: 1_111_603_742,
                    value_sum_bits: 1_121_934_212,
                    loss_bits: 1_064_195_457,
                },
            )
        }
        #[cfg(not(any(
            all(target_arch = "x86_64", target_os = "windows", target_env = "msvc"),
            all(target_arch = "x86_64", target_os = "linux", target_env = "gnu")
        )))]
        panic!("no reviewed exact Burn-pair numerical witness for this Rust target");
    }

    fn without_observed_timing_v2(
        mut evidence: NativeTrainerUpdateEvidenceV2,
    ) -> NativeTrainerUpdateEvidenceV2 {
        evidence.update_elapsed_ns = 0;
        evidence.rollout_metrics.total_elapsed_ns = 0;
        evidence.rollout_metrics.broker_service_ns = 0;
        evidence
    }

    #[test]
    fn scorer_acceptance_counters_must_match_rollout_counters() {
        let expected_error = NativeTrainerErrorV1::GroupingInvariant(
            "scorer accepted counters must exactly match rollout counters",
        );
        let valid = AsyncFlatScoredRolloutMetricsV2 {
            scorer_batch_count: 2,
            scored_decision_count: 3,
            sampled_action_count: 3,
            batch_width_sum: 3,
            ..AsyncFlatScoredRolloutMetricsV2::default()
        };
        assert_eq!(validate_scorer_rollout_counters_v2(2, 3, &valid), Ok(()));

        let mut wrong_batch_count = valid;
        wrong_batch_count.scorer_batch_count = 1;
        assert_eq!(
            validate_scorer_rollout_counters_v2(2, 3, &wrong_batch_count),
            Err(expected_error.clone())
        );

        let mut wrong_scored_count = valid;
        wrong_scored_count.scored_decision_count = 2;
        assert_eq!(
            validate_scorer_rollout_counters_v2(2, 3, &wrong_scored_count),
            Err(expected_error.clone())
        );

        let mut wrong_sampled_count = valid;
        wrong_sampled_count.sampled_action_count = 2;
        assert_eq!(
            validate_scorer_rollout_counters_v2(2, 3, &wrong_sampled_count),
            Err(expected_error.clone())
        );

        let mut wrong_width_sum = valid;
        wrong_width_sum.batch_width_sum = 2;
        assert_eq!(
            validate_scorer_rollout_counters_v2(2, 3, &wrong_width_sum),
            Err(expected_error)
        );
    }

    #[test]
    fn phase_profile_is_ordered_accounted_and_training_semantics_neutral() {
        let _lock = acquire_async_flat_scored_test_lock_v1();
        let initial = trainer_v2(2);
        let mut ordinary = initial.clone();
        let mut profiled = initial;
        let config = burn_pair_config_v2(1, 1, 1);

        let ordinary_evidence = ordinary.run_even_batch_update_v2(&config).unwrap();
        let (profiled_evidence, profile) =
            profiled.run_even_batch_update_profiled_v2(&config).unwrap();

        assert_eq!(
            without_observed_timing_v2(ordinary_evidence),
            without_observed_timing_v2(profiled_evidence.clone())
        );
        assert_eq!(
            exact_state_snapshot_v1(&ordinary),
            exact_state_snapshot_v1(&profiled)
        );
        assert_eq!(
            profile.update_elapsed_ns_v1(),
            profiled_evidence.update_elapsed_ns
        );
        assert!(profile.update_elapsed_ns_v1() > 0);
        assert!(profile.accounted_elapsed_ns_v1() <= profile.update_elapsed_ns_v1());
        for phase in NativeTrainingPhaseV1::ALL {
            assert!(
                profile.phase_record_count_v1(phase) > 0,
                "missing diagnostic phase {}",
                phase.label_v1()
            );
        }

        let timeline = profile
            .records_v1()
            .iter()
            .map(|record| record.phase)
            .collect::<Vec<_>>();
        let required_order = [
            NativeTrainingPhaseV1::SetupValidation,
            NativeTrainingPhaseV1::Rollout,
            NativeTrainingPhaseV1::GroupingMaterialization,
            NativeTrainingPhaseV1::ForwardLoss,
            NativeTrainingPhaseV1::BackwardGauge,
            NativeTrainingPhaseV1::AdamMath,
            NativeTrainingPhaseV1::FinalizationCloning,
            NativeTrainingPhaseV1::EvidenceConstruction,
        ];
        let mut cursor = 0usize;
        for required in required_order {
            let relative = timeline[cursor..]
                .iter()
                .position(|phase| *phase == required)
                .unwrap_or_else(|| panic!("phase {} is out of order", required.label_v1()));
            cursor += relative + 1;
        }
    }

    #[test]
    fn common_snapshot_bootstrap_keeps_authority_seed_separate_and_trains_rally_pair() {
        const RUN_BASE_SEED_V1: u64 = 71_501;

        let _lock = acquire_async_flat_scored_test_lock_v1();
        let (manifest_path, payload_path) = common_model_snapshot_paths_v1();
        let (mut trainer, record) = NativeTrainerStateV2::from_common_model_snapshot_v2(
            RUN_BASE_SEED_V1,
            2,
            &manifest_path,
            &payload_path,
        )
        .unwrap();

        assert_eq!(trainer.base_seed, RUN_BASE_SEED_V1);
        assert_eq!(trainer.batch_episodes, 2);
        assert_eq!(record.identity, SNAPSHOT_IDENTITY_V1);
        assert_eq!(record.base_seed, SNAPSHOT_AUTHORITY_BASE_SEED_V1);
        assert_eq!(record.model_init_seed, SNAPSHOT_MODEL_INIT_SEED_V1);
        assert_eq!(
            record.model_init_seed,
            derive_native_trainer_model_init_seed_v1(record.base_seed).unwrap()
        );
        assert_ne!(trainer.base_seed, record.base_seed);
        assert_ne!(
            derive_native_trainer_model_init_seed_v1(trainer.base_seed).unwrap(),
            record.model_init_seed
        );
        assert_eq!(record.adam_step_initial, 0);
        assert_eq!(trainer.train_state_v1().adam_step_v1(), 0);
        assert!(record.snapshot_load_completed_before_trial_start);
        assert!(!record.snapshot_load_timed);
        assert!(!record.rust_seeded_initializer_reproduced);
        assert_eq!(
            record.loaded_named_parameter_stream_sha256,
            record.named_parameter_stream_sha256
        );

        let progress_before = trainer.progress_v2();
        let parameters_before = trainer.train_state_v1().model_v1().parameter_snapshot_v1();
        let first_moments_before = trainer.train_state_v1().first_moment_snapshot_v1();
        let second_moments_before = trainer.train_state_v1().second_moment_snapshot_v1();
        let model_digest_before = trainer
            .train_state_v1()
            .model_v1()
            .parameter_manifest_sha256_v1();
        assert_eq!(progress_before.next_episode_index, 0);
        assert_eq!(progress_before.successful_update_count, 0);
        assert!(first_moments_before
            .iter()
            .chain(&second_moments_before)
            .flat_map(|parameter| &parameter.values)
            .all(|value| value.to_bits() == 0));

        let mut config = burn_even_batch_config_v2(2, 1, 1, 1);
        config.deck_ids = ["Rally".to_owned(), "Rally".to_owned()];
        let evidence = trainer.run_even_batch_update_v2(&config).unwrap();

        assert_eq!(evidence.first_episode_index, 0);
        assert_eq!(evidence.episode_count, 2);
        assert_eq!(evidence.adam_step_before, 0);
        assert_eq!(evidence.adam_step_after, 1);
        assert_eq!(evidence.model_digest_before, model_digest_before);
        assert_ne!(evidence.model_digest_after, evidence.model_digest_before);
        assert!(evidence.changed_non_gauge_parameter_count > 0);
        for episode in &evidence.episodes {
            let schedule =
                native_trainer_episode_schedule_v1(RUN_BASE_SEED_V1, episode.episode_index)
                    .unwrap();
            assert_eq!(episode.learner_seat, schedule.learner_seat);
            assert_eq!(
                episode.full_trajectory_receipt.environment_seed,
                schedule.environment_seed
            );
        }

        let progress_after = trainer.progress_v2();
        assert_eq!(progress_after.next_episode_index, 2);
        assert_eq!(progress_after.successful_update_count, 1);
        assert_eq!(progress_after.completed_episode_count, 2);
        assert_eq!(
            progress_after.learner_physical_decision_count,
            evidence.learner_group_count
        );
        assert_eq!(
            progress_after.learner_policy_step_count,
            evidence.learner_policy_step_count
        );
        assert_eq!(trainer.train_state_v1().adam_step_v1(), 1);
        assert_eq!(
            trainer
                .train_state_v1()
                .model_v1()
                .parameter_manifest_sha256_v1(),
            evidence.model_digest_after
        );
        assert_ne!(
            trainer.train_state_v1().model_v1().parameter_snapshot_v1(),
            parameters_before
        );
        assert_ne!(
            trainer.train_state_v1().first_moment_snapshot_v1(),
            first_moments_before
        );
        assert_ne!(
            trainer.train_state_v1().second_moment_snapshot_v1(),
            second_moments_before
        );
    }

    #[test]
    fn common_snapshot_bootstrap_rejects_corruption_and_seed_collision_without_live_drift() {
        const RUN_BASE_SEED_V1: u64 = 71_501;

        let _lock = acquire_async_flat_scored_test_lock_v1();
        let (manifest_path, payload_path) = common_model_snapshot_paths_v1();
        let (trainer, _) = NativeTrainerStateV2::from_common_model_snapshot_v2(
            RUN_BASE_SEED_V1,
            2,
            &manifest_path,
            &payload_path,
        )
        .unwrap();
        let before = exact_state_snapshot_v1(&trainer);
        let corrupted = CorruptedSnapshotPayloadV1::new_v1();
        let error = NativeTrainerStateV2::from_common_model_snapshot_v2(
            RUN_BASE_SEED_V1,
            2,
            &manifest_path,
            &corrupted.path,
        )
        .unwrap_err();
        assert!(matches!(error, NativeTrainerBootstrapErrorV1::Snapshot(_)));
        assert_eq!(exact_state_snapshot_v1(&trainer), before);

        let error = NativeTrainerStateV2::from_common_model_snapshot_v2(
            SNAPSHOT_AUTHORITY_BASE_SEED_V1,
            2,
            &manifest_path,
            &payload_path,
        )
        .unwrap_err();
        assert_eq!(
            error,
            NativeTrainerBootstrapErrorV1::RunSeedMatchesSnapshotAuthority {
                run_base_seed: SNAPSHOT_AUTHORITY_BASE_SEED_V1,
                snapshot_base_seed: SNAPSHOT_AUTHORITY_BASE_SEED_V1,
            }
        );
        assert_eq!(exact_state_snapshot_v1(&trainer), before);
    }

    #[test]
    fn real_burn_pair_updates_once_and_is_topology_invariant() {
        let _lock = acquire_async_flat_scored_test_lock_v1();
        let initial = trainer_v2(2);
        let initial_parameters = initial.train_state_v1().model_v1().parameter_snapshot_v1();
        let initial_bias_bits = gauge_value_bits_v1(&initial_parameters);
        let mut narrow = initial.clone();
        let mut wide = initial;

        let narrow_recompute_count_before = packed_actual_recompute_call_count_for_test_v1();
        let narrow_evidence = narrow
            .run_even_batch_update_v2(&burn_pair_config_v2(1, 1, 1))
            .unwrap();
        let narrow_recompute_count_after = packed_actual_recompute_call_count_for_test_v1();
        let wide_recompute_count_before = narrow_recompute_count_after;
        let wide_evidence = wide
            .run_even_batch_update_v2(&burn_pair_config_v2(2, 2, 3))
            .unwrap();
        let wide_recompute_count_after = packed_actual_recompute_call_count_for_test_v1();

        for (before, after, evidence) in [
            (
                narrow_recompute_count_before,
                narrow_recompute_count_after,
                &narrow_evidence,
            ),
            (
                wide_recompute_count_before,
                wide_recompute_count_after,
                &wide_evidence,
            ),
        ] {
            assert_eq!(
                after - before,
                evidence.scorer_accepted_decision_count,
                "training must independently recompute each accepted decision exactly once"
            );
        }

        let narrow_state_sha256 = narrow.train_state_v1().state_sha256_v1().unwrap();
        let wide_state_sha256 = wide.train_state_v1().state_sha256_v1().unwrap();
        assert_eq!(
            narrow_state_sha256, wide_state_sha256,
            "the exact K=2 train state must be scheduler-topology invariant"
        );
        assert_eq!(
            narrow_evidence.model_digest_after, wide_evidence.model_digest_after,
            "the exact K=2 model must be scheduler-topology invariant"
        );
        assert_eq!(
            (
                narrow_evidence.policy_sum_bits,
                narrow_evidence.value_sum_bits,
                narrow_evidence.loss_bits,
            ),
            (
                wide_evidence.policy_sum_bits,
                wide_evidence.value_sum_bits,
                wide_evidence.loss_bits,
            ),
            "the exact K=2 loss tuple must be scheduler-topology invariant"
        );

        // The Windows witness remains frozen to the exact reviewed PR #44
        // two-episode behavior. The independently repeated Linux witness pins
        // the same test program on its GNU target tuple; it is deliberately not
        // a cross-OS PR #44 bit-parity claim. Timing and scheduler topology are
        // excluded. Runtime and trajectory facts below stay target-independent;
        // platform libm last bits flow into the exact numerical/Adam digest.
        let (reviewed_target, expected_numerical) = recorded_burn_pair_numerical_witness_v1();
        let actual_numerical = BurnPairNumericalWitnessV1 {
            train_state_sha256: narrow_state_sha256,
            model_digest_after: narrow_evidence.model_digest_after.as_str(),
            policy_sum_bits: narrow_evidence.policy_sum_bits,
            value_sum_bits: narrow_evidence.value_sum_bits,
            loss_bits: narrow_evidence.loss_bits,
        };
        assert_eq!(
            actual_numerical, expected_numerical,
            "exact K=2 numerical witness drifted on {reviewed_target}"
        );
        assert_eq!(narrow_evidence.learner_group_count, 112);
        assert_eq!(narrow_evidence.learner_policy_step_count, 113);
        assert_eq!(narrow_evidence.scorer_accepted_batch_count, 113);
        assert_eq!(narrow_evidence.scorer_accepted_decision_count, 113);
        assert_eq!(
            narrow_evidence.model_digest_before,
            "cc8205d35f68b9d961a4115b7029b2c394f9ee9a981887284e46410b5a90991c"
        );
        assert_eq!(narrow_evidence.changed_non_gauge_parameter_count, 32);
        assert_eq!(
            narrow_evidence.episodes[0]
                .full_trajectory_receipt
                .trajectory_sha256,
            [
                218, 58, 252, 127, 21, 185, 50, 121, 19, 64, 114, 39, 237, 157, 11, 206, 2, 100,
                249, 37, 3, 248, 145, 82, 102, 176, 154, 247, 122, 191, 134, 7,
            ]
        );
        assert_eq!(
            narrow_evidence.episodes[1]
                .full_trajectory_receipt
                .trajectory_sha256,
            [
                0, 225, 21, 221, 53, 228, 140, 20, 20, 160, 25, 212, 244, 84, 87, 177, 246, 163,
                191, 9, 245, 195, 100, 216, 166, 134, 107, 212, 163, 200, 224, 119,
            ]
        );

        for evidence in [&narrow_evidence, &wide_evidence] {
            // The update timer starts before validation and is captured only
            // after the live model/progress commit. Rollout is a strict inner
            // span, so every successful update must record both a nonzero
            // duration and at least the rollout's own elapsed duration.
            assert!(evidence.update_elapsed_ns > 0);
            assert!(evidence.update_elapsed_ns >= evidence.rollout_metrics.total_elapsed_ns);
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
            assert!(evidence
                .physical_terms
                .iter()
                .all(|term| term.substep_count > 0));
            assert_eq!(
                evidence
                    .physical_terms
                    .iter()
                    .map(|term| u64::from(term.substep_count))
                    .sum::<u64>(),
                evidence.learner_policy_step_count
            );
            let mut selected_output_group_counts = vec![0_u32; evidence.physical_terms.len()];
            for output in &evidence.selected_outputs {
                selected_output_group_counts[output.group_index] += 1;
            }
            assert!(evidence
                .physical_terms
                .iter()
                .zip(selected_output_group_counts)
                .all(|(term, selected_count)| term.substep_count == selected_count));
            assert_eq!(
                evidence.rollout_metrics.scored_decision_count,
                evidence.scorer_accepted_decision_count
            );
        }

        assert_eq!(narrow.progress_v2().successful_update_count, 1);
        assert_eq!(narrow.progress_v2().completed_episode_count, 2);
        assert_eq!(narrow.progress_v2().next_episode_index, 2);
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
            .run_even_batch_update_v2(&burn_pair_config_v2(1, 1, 1))
            .unwrap();
        assert_eq!(second_evidence.first_episode_index, 2);
        assert_eq!(
            second_evidence
                .episodes
                .iter()
                .map(|episode| episode.episode_index)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert_eq!(second_evidence.episodes[0].learner_seat, PlayerSeatV1::P0);
        assert_eq!(second_evidence.episodes[1].learner_seat, PlayerSeatV1::P1);
        assert_eq!(second_evidence.adam_step_before, 1);
        assert_eq!(second_evidence.adam_step_after, 2);
        assert_eq!(narrow.progress_v2().successful_update_count, 2);
        assert_eq!(narrow.progress_v2().completed_episode_count, 4);
        assert_eq!(narrow.progress_v2().next_episode_index, 4);
        assert_eq!(narrow.train_state_v1().adam_step_v1(), 2);
    }

    #[test]
    fn even_batch_v2_accepts_python_range_and_rejects_non_even_cardinality() {
        assert_eq!(
            NATIVE_TRAINER_CONTRACT_IDENTITY_V2,
            "mtg-kernel-native-even-batch-trainer-v2"
        );
        assert_eq!(NATIVE_TRAINER_MAX_BATCH_EPISODES_V2, 10_000);
        for batch_episodes in [0, 1, 3, 10_001, NATIVE_TRAINER_MAX_BATCH_EPISODES_V2 + 2] {
            let config = burn_even_batch_config_v2(batch_episodes, 1, 1, 1);
            assert_eq!(
                validate_update_config_v2(&config),
                Err(NativeTrainerErrorV1::InvalidUpdateConfig("batch_episodes"))
            );
        }
        for batch_episodes in [2, 4, 16, NATIVE_TRAINER_MAX_BATCH_EPISODES_V2] {
            validate_update_config_v2(&burn_even_batch_config_v2(batch_episodes, 1, 1, 1)).unwrap();
        }

        let mut trainer = trainer_v2(4);
        let before = exact_state_snapshot_v1(&trainer);
        assert_eq!(trainer.batch_episodes, 4);
        assert_eq!(
            trainer.run_even_batch_update_v2(&burn_even_batch_config_v2(2, 1, 1, 1)),
            Err(NativeTrainerErrorV1::InvalidUpdateConfig("batch_episodes"))
        );
        assert_eq!(trainer.batch_episodes, 4);
        assert_eq!(exact_state_snapshot_v1(&trainer), before);
    }

    #[test]
    fn numerical_backend_and_backward_worker_topology_are_validated_explicitly() {
        assert_ne!(
            NativeTrainingNumericalBackendV1::Sequential.identity_v1(),
            NativeTrainingNumericalBackendV1::FixedFourPartitions.identity_v1()
        );

        let mut config = burn_pair_config_v2(1, 1, 1);
        config.backward_worker_limit = 2;
        assert_eq!(
            validate_update_config_v2(&config),
            Err(NativeTrainerErrorV1::InvalidUpdateConfig(
                "backward_worker_limit"
            ))
        );

        config.numerical_backend = NativeTrainingNumericalBackendV1::FixedFourPartitions;
        for worker_limit in 1..=FIXED_BACKWARD_PARTITION_COUNT_V1 {
            config.backward_worker_limit = worker_limit;
            validate_update_config_v2(&config).unwrap();
        }
        for worker_limit in [0, FIXED_BACKWARD_PARTITION_COUNT_V1 + 1] {
            config.backward_worker_limit = worker_limit;
            assert_eq!(
                validate_update_config_v2(&config),
                Err(NativeTrainerErrorV1::InvalidUpdateConfig(
                    "backward_worker_limit"
                ))
            );
        }
    }

    #[test]
    fn fixed_four_backend_runs_a_real_update_and_is_worker_topology_invariant() {
        let _lock = acquire_async_flat_scored_test_lock_v1();
        let initial = trainer_v2(2);
        let mut single_worker = initial.clone();
        let mut four_workers = initial;
        let mut config = burn_pair_config_v2(1, 1, 1);
        config.numerical_backend = NativeTrainingNumericalBackendV1::FixedFourPartitions;
        config.backward_worker_limit = 1;
        let single_evidence = single_worker.run_even_batch_update_v2(&config).unwrap();
        config.backward_worker_limit = FIXED_BACKWARD_PARTITION_COUNT_V1;
        let four_evidence = four_workers.run_even_batch_update_v2(&config).unwrap();

        assert_eq!(
            without_observed_timing_v2(single_evidence),
            without_observed_timing_v2(four_evidence)
        );
        assert_eq!(
            exact_state_snapshot_v1(&single_worker),
            exact_state_snapshot_v1(&four_workers)
        );
    }

    #[test]
    fn real_burn_even_batches_update_once_and_are_topology_invariant() {
        let _lock = acquire_async_flat_scored_test_lock_v1();
        for batch_episodes in [4, 16] {
            let initial = trainer_v2(batch_episodes);
            let mut narrow = initial.clone();
            let mut wide = initial;
            let narrow_evidence = narrow
                .run_even_batch_update_v2(&burn_even_batch_config_v2(batch_episodes, 1, 1, 1))
                .unwrap();
            let wide_evidence = wide
                .run_even_batch_update_v2(&burn_even_batch_config_v2(batch_episodes, 4, 4, 16))
                .unwrap();

            assert_eq!(narrow_evidence.episode_count, batch_episodes);
            assert_eq!(
                narrow_evidence.trainer_contract_identity,
                NATIVE_TRAINER_CONTRACT_IDENTITY_V2
            );
            assert_eq!(narrow_evidence.worker_count, 1);
            assert_eq!(narrow_evidence.sessions_per_worker, 1);
            assert_eq!(narrow_evidence.logical_actor_count, 1);
            assert_eq!(narrow_evidence.broker_batch_target, 1);
            assert_eq!(wide_evidence.worker_count, 4);
            assert_eq!(wide_evidence.sessions_per_worker, 4);
            assert_eq!(wide_evidence.logical_actor_count, 16);
            assert_eq!(wide_evidence.broker_batch_target, 16);
            assert_eq!(
                narrow_evidence.episodes.len(),
                usize::try_from(batch_episodes).unwrap()
            );
            for (offset, episode) in narrow_evidence.episodes.iter().enumerate() {
                let expected_index = u64::try_from(offset).unwrap();
                assert_eq!(episode.episode_index, expected_index);
                assert_eq!(
                    episode.learner_seat,
                    if expected_index & 1 == 0 {
                        PlayerSeatV1::P0
                    } else {
                        PlayerSeatV1::P1
                    }
                );
                assert_eq!(
                    episode.full_trajectory_receipt.environment_seed,
                    native_trainer_episode_schedule_v1(71_501, expected_index)
                        .unwrap()
                        .environment_seed
                );
            }
            for pair in narrow_evidence.episodes.chunks_exact(2) {
                assert_eq!(
                    pair[0].full_trajectory_receipt.environment_seed,
                    pair[1].full_trajectory_receipt.environment_seed
                );
            }
            assert_eq!(narrow_evidence.adam_step_before, 0);
            assert_eq!(narrow_evidence.adam_step_after, 1);
            assert_eq!(narrow.progress_v2().successful_update_count, 1);
            assert_eq!(narrow.progress_v2().completed_episode_count, batch_episodes);
            assert_eq!(narrow.progress_v2().next_episode_index, batch_episodes);
            assert_eq!(narrow.train_state_v1().adam_step_v1(), 1);

            assert_eq!(narrow_evidence.episodes, wide_evidence.episodes);
            assert_eq!(
                narrow_evidence.scorer_accepted_decision_count,
                wide_evidence.scorer_accepted_decision_count
            );
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
                narrow_evidence.model_digest_before,
                wide_evidence.model_digest_before
            );
            assert_eq!(
                narrow_evidence.model_digest_after,
                wide_evidence.model_digest_after
            );
            assert_eq!(
                exact_state_snapshot_v1(&narrow),
                exact_state_snapshot_v1(&wide)
            );
        }
    }

    #[test]
    fn even_batch_v2_resume_binds_persisted_k_and_continues_exactly() {
        let _lock = acquire_async_flat_scored_test_lock_v1();
        let config = burn_even_batch_config_v2(4, 2, 2, 4);
        let mut uninterrupted = trainer_v2(4);
        uninterrupted.run_even_batch_update_v2(&config).unwrap();

        let persisted_progress = uninterrupted.progress_v2();
        let persisted_snapshot = uninterrupted.train_state_v1().snapshot_v1().unwrap();
        let encoded = encode_native_train_state_payload_v1(&persisted_snapshot).unwrap();
        let decoded = decode_native_train_state_payload_verified_v1(
            &encoded.bytes,
            persisted_snapshot.adam_step,
            persisted_snapshot.scorer_bias_anchor_bits,
            &encoded.digests,
        )
        .unwrap();
        assert_eq!(decoded.snapshot, persisted_snapshot);
        assert_eq!(decoded.digests, encoded.digests);

        // Reconstruct from only the frozen model contract plus decoded payload
        // state. This deliberately does not clone the live trainer model.
        let mut template =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        template
            .replace_parameter_snapshot_v1(&decoded.snapshot.parameters)
            .unwrap();
        let persisted_train_state =
            NativePolicyValueTrainStateV1::from_snapshot_v1(template, &decoded.snapshot).unwrap();
        let persisted_state_sha = persisted_train_state.state_sha256_v1().unwrap();
        let mut resumed = NativeTrainerStateV2::from_resumed_parts_v2(
            uninterrupted.base_seed_v2(),
            4,
            &persisted_train_state,
            persisted_progress,
        )
        .unwrap();
        assert_eq!(resumed.base_seed_v2(), 71_501);
        assert_eq!(resumed.batch_episodes, 4);
        assert_eq!(
            persisted_train_state.state_sha256_v1().unwrap(),
            persisted_state_sha
        );
        assert_eq!(
            resumed.train_state_v1().state_sha256_v1().unwrap(),
            persisted_state_sha
        );

        let uninterrupted_evidence = uninterrupted.run_even_batch_update_v2(&config).unwrap();
        let resumed_evidence = resumed.run_even_batch_update_v2(&config).unwrap();
        assert_eq!(
            without_observed_timing_v2(uninterrupted_evidence),
            without_observed_timing_v2(resumed_evidence)
        );
        assert_eq!(
            exact_state_snapshot_v1(&uninterrupted),
            exact_state_snapshot_v1(&resumed)
        );
        assert_eq!(resumed.progress_v2().successful_update_count, 2);
        assert_eq!(resumed.progress_v2().completed_episode_count, 8);
        assert_eq!(resumed.progress_v2().next_episode_index, 8);

        let source_before = persisted_train_state.state_sha256_v1().unwrap();
        let error = NativeTrainerStateV2::from_resumed_parts_v2(
            71_501,
            2,
            &persisted_train_state,
            persisted_progress,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            NativeTrainerErrorV1::ResumeInvariant(
                "completed episode count must equal successful updates times persisted batch episodes"
            )
        ));
        assert_eq!(
            persisted_train_state.state_sha256_v1().unwrap(),
            source_before
        );
    }

    #[test]
    fn even_batch_v2_resume_progress_corruption_is_transactional() {
        let _lock = acquire_async_flat_scored_test_lock_v1();
        let train_state = trainer_v2(4).train_state_v1().clone();
        let source_sha = train_state.state_sha256_v1().unwrap();
        let valid = NativeTrainerProgressV2 {
            next_episode_index: 0,
            successful_update_count: 0,
            completed_episode_count: 0,
            learner_physical_decision_count: 0,
            learner_policy_step_count: 0,
        };

        for (progress, expected) in [
            (
                NativeTrainerProgressV2 {
                    next_episode_index: 1,
                    ..valid
                },
                NativeTrainerErrorV1::ResumeInvariant(
                    "next episode must begin an even/odd parity pair",
                ),
            ),
            (
                NativeTrainerProgressV2 {
                    next_episode_index: 4,
                    completed_episode_count: 2,
                    successful_update_count: 1,
                    ..valid
                },
                NativeTrainerErrorV1::ResumeInvariant(
                    "next episode must equal completed episode count",
                ),
            ),
            (
                NativeTrainerProgressV2 {
                    next_episode_index: 4,
                    completed_episode_count: 4,
                    successful_update_count: 2,
                    ..valid
                },
                NativeTrainerErrorV1::ResumeInvariant(
                    "completed episode count must equal successful updates times persisted batch episodes",
                ),
            ),
            (
                NativeTrainerProgressV2 {
                    next_episode_index: 4,
                    completed_episode_count: 4,
                    successful_update_count: 1,
                    ..valid
                },
                NativeTrainerErrorV1::ResumeInvariant(
                    "Adam step must equal successful update count",
                ),
            ),
            (
                NativeTrainerProgressV2 {
                    learner_policy_step_count: NATIVE_TRAINER_U63_MAX_V2 + 1,
                    ..valid
                },
                NativeTrainerErrorV1::ProgressOutsideU63 {
                    field: "learner_policy_step_count",
                    value: NATIVE_TRAINER_U63_MAX_V2 + 1,
                },
            ),
        ] {
            let error = NativeTrainerStateV2::from_resumed_parts_v2(
                71_501,
                4,
                &train_state,
                progress,
            )
            .unwrap_err();
            assert_eq!(error, expected);
            assert_eq!(train_state.state_sha256_v1().unwrap(), source_sha);
        }
    }

    #[test]
    fn association_mutations_leave_model_optimizer_and_counters_exact() {
        let _lock = acquire_async_flat_scored_test_lock_v1();
        let mut trainer = trainer_v2(2);
        let config = burn_pair_config_v2(1, 1, 1);
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
                .run_even_batch_update_with_mutation_v2(&config, mutation)
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

        let error = trainer
            .run_even_batch_update_with_mutation_v2(
                &config,
                NativePolicyAssociationTestMutationV1::ModelGeneration,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            NativeTrainerErrorV1::Train(
                NativePolicyTrainErrorV1::PackedForwardModelGenerationMismatch { .. }
            )
        ));
        assert_eq!(exact_state_snapshot_v1(&trainer), before);

        let error = trainer
            .run_even_batch_update_with_mutation_v2(
                &config,
                NativePolicyAssociationTestMutationV1::CanonicalTensor,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            NativeTrainerErrorV1::Train(
                NativePolicyTrainErrorV1::RecomputedLogitBitsMismatch {
                    expected_bits,
                    actual_bits,
                    ..
                }
            ) if expected_bits != actual_bits
        ));
        assert_eq!(exact_state_snapshot_v1(&trainer), before);
    }

    #[test]
    fn parallel_scorer_preserves_failure_order_and_is_transactional_and_retryable() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NativePolicyPackedForwardBuilderV1>();
        assert_send_sync::<NativePolicyPackedForwardTapeV1>();

        let _lock = acquire_async_flat_scored_test_lock_v1();
        let initial = trainer_v2(2);
        let before = exact_state_snapshot_v1(&initial);
        let mut faulted = initial.clone();
        let mut reference = initial;
        let config = burn_even_batch_config_v2(2, 2, 2, 3);
        assert_eq!(
            faulted
                .run_even_batch_update_with_forward_worker_panic_v2(&config)
                .unwrap_err(),
            NativeTrainerErrorV1::Scorer(NativePolicyScorerFailureV1::ForwardWorker)
        );
        assert_eq!(exact_state_snapshot_v1(&faulted), before);

        let faulted_evidence = faulted.run_even_batch_update_v2(&config).unwrap();
        let reference_evidence = reference.run_even_batch_update_v2(&config).unwrap();
        assert_eq!(
            without_observed_timing_v2(faulted_evidence),
            without_observed_timing_v2(reference_evidence)
        );
        assert_eq!(
            exact_state_snapshot_v1(&faulted),
            exact_state_snapshot_v1(&reference)
        );
    }

    #[test]
    fn non_selected_logit_mutation_reaches_full_vector_gate_transactionally() {
        let _lock = acquire_async_flat_scored_test_lock_v1();
        let mut trainer = trainer_v2(2);
        let config = burn_pair_config_v2(1, 1, 1);
        let before = exact_state_snapshot_v1(&trainer);
        let error = trainer
            .run_even_batch_update_with_train_non_selected_logit_mutation_v2(&config)
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

    #[test]
    fn physical_substep_count_corruption_is_transactional_before_live_commit() {
        let _lock = acquire_async_flat_scored_test_lock_v1();
        let mut trainer = trainer_v2(2);
        let config = burn_pair_config_v2(1, 1, 1);
        let before = exact_state_snapshot_v1(&trainer);
        assert_eq!(
            trainer
                .run_even_batch_update_with_physical_substep_count_mutation_v2(&config)
                .unwrap_err(),
            NativeTrainerErrorV1::RecomputedOutputMismatch {
                field: "substep_count",
                group_index: 0,
                substep_index: 0,
            }
        );
        assert_eq!(exact_state_snapshot_v1(&trainer), before);
    }

    #[test]
    fn even_batch_v2_recomputed_logit_corruption_is_transactional_at_each_batch_region() {
        let _lock = acquire_async_flat_scored_test_lock_v1();
        let config = burn_even_batch_config_v2(4, 2, 2, 4);
        for episode_offset in [0, 2, 3] {
            let mut trainer = trainer_v2(4);
            let before = exact_state_snapshot_v1(&trainer);
            let error = trainer
                .run_even_batch_update_with_train_revalidation_mutation_v2(
                    &config,
                    NativePolicyTrainRevalidationTestMutationV1::Logit { episode_offset },
                )
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

    #[test]
    fn even_batch_v2_recomputed_value_corruption_is_transactional_at_each_batch_region() {
        let _lock = acquire_async_flat_scored_test_lock_v1();
        let config = burn_even_batch_config_v2(4, 2, 2, 4);
        for episode_offset in [0, 2, 3] {
            let mut trainer = trainer_v2(4);
            let before = exact_state_snapshot_v1(&trainer);
            let error = trainer
                .run_even_batch_update_with_train_revalidation_mutation_v2(
                    &config,
                    NativePolicyTrainRevalidationTestMutationV1::Value { episode_offset },
                )
                .unwrap_err();
            assert!(matches!(
                error,
                NativeTrainerErrorV1::Train(
                    NativePolicyTrainErrorV1::RecomputedValueBitsMismatch {
                        expected_bits,
                        actual_bits,
                        ..
                    }
                ) if expected_bits != actual_bits
            ));
            assert_eq!(exact_state_snapshot_v1(&trainer), before);
        }
    }

    #[test]
    fn even_batch_v2_expected_logit_count_corruption_is_transactional_at_each_batch_region() {
        let _lock = acquire_async_flat_scored_test_lock_v1();
        let config = burn_even_batch_config_v2(4, 2, 2, 4);
        for episode_offset in [0, 2, 3] {
            let mut trainer = trainer_v2(4);
            let before = exact_state_snapshot_v1(&trainer);
            let error = trainer
                .run_even_batch_update_with_train_revalidation_mutation_v2(
                    &config,
                    NativePolicyTrainRevalidationTestMutationV1::ExpectedLogitCount {
                        episode_offset,
                    },
                )
                .unwrap_err();
            assert!(matches!(
                error,
                NativeTrainerErrorV1::Train(
                    NativePolicyTrainErrorV1::ExpectedLogitCountMismatch {
                        expected,
                        actual,
                        ..
                    }
                ) if expected < actual
            ));
            assert_eq!(exact_state_snapshot_v1(&trainer), before);
        }
    }
}
