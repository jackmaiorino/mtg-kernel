//! Standalone eight-slot population-opponent runtime primitive.
//!
//! This module deliberately does not wire into the trainer, rollout, science
//! loop, or Store. It owns only the versioned slot identity, integer-weight
//! validation, deterministic episode slot selection, and dispatch from one
//! selected slot to one immutable checkpoint-inference handle. The existing
//! K4 ladder module remains unchanged.

use crate::flat_policy_v2::FlatScoringDecisionViewV2;
use crate::native_checkpoint_inference_v1::NativeCheckpointInferenceV1;
use crate::native_ladder_opponent_v1::softmax_sample_temperature_one_v1;
use crate::native_trainer_schedule_v2::derive_native_trainer_opponent_pool_choice_seed_v2;
use core::fmt;
#[cfg(test)]
use std::sync::Mutex;

/// Versioned identity for the standalone eight-policy population primitive.
pub(crate) const POPULATION_OPPONENT_IDENTITY_V1: &str = "mtg-kernel-native-population-opponent-v1";

/// The population runtime has exactly eight policy slots.
pub(crate) const POPULATION_OPPONENT_SLOT_COUNT_V1: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PopulationOpponentErrorV1 {
    InvalidWeightTotal,
    WeightOverflow,
    SeedDerivation,
    InvalidSlot,
    Inference,
    Softmax,
}

impl fmt::Display for PopulationOpponentErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidWeightTotal => "population weight total is invalid",
            Self::WeightOverflow => "population weight total overflows u64",
            Self::SeedDerivation => "population slot seed derivation failed",
            Self::InvalidSlot => "population slot is invalid",
            Self::Inference => "population checkpoint inference rejected the decision",
            Self::Softmax => "population temperature-one softmax selection failed",
        })
    }
}

impl std::error::Error for PopulationOpponentErrorV1 {}

/// A validated integer weight vector and its cumulative thresholds.
///
/// `declared_total` is checked against the exact sum of `weights`. Runtime
/// selection performs one documented modulo draw, `seed % total`, with no
/// rejection sampling. Zero weights are allowed for inactive slots; the total
/// must remain nonzero, so every selectable slot necessarily has positive
/// weight.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PopulationWeightVectorV1 {
    weights: [u64; POPULATION_OPPONENT_SLOT_COUNT_V1],
    total: u64,
    cumulative: [u64; POPULATION_OPPONENT_SLOT_COUNT_V1],
}

impl PopulationWeightVectorV1 {
    pub(crate) fn new_v1(
        weights: [u64; POPULATION_OPPONENT_SLOT_COUNT_V1],
        declared_total: u64,
    ) -> Result<Self, PopulationOpponentErrorV1> {
        if declared_total == 0 {
            return Err(PopulationOpponentErrorV1::InvalidWeightTotal);
        }
        let mut cumulative = [0_u64; POPULATION_OPPONENT_SLOT_COUNT_V1];
        let mut running = 0_u64;
        for (index, &weight) in weights.iter().enumerate() {
            running = running
                .checked_add(weight)
                .ok_or(PopulationOpponentErrorV1::WeightOverflow)?;
            cumulative[index] = running;
        }
        if running != declared_total {
            return Err(PopulationOpponentErrorV1::InvalidWeightTotal);
        }
        Ok(Self {
            weights,
            total: declared_total,
            cumulative,
        })
    }

    pub(crate) const fn weights_v1(&self) -> &[u64; POPULATION_OPPONENT_SLOT_COUNT_V1] {
        &self.weights
    }

    pub(crate) const fn total_v1(&self) -> u64 {
        self.total
    }

    fn select_draw_v1(&self, draw: u64) -> PopulationSlotV1 {
        let reduced = draw % self.total;
        for (index, &threshold) in self.cumulative.iter().enumerate() {
            if reduced < threshold {
                return PopulationSlotV1::from_index_v1(index).expect("validated slot index");
            }
        }
        unreachable!("validated cumulative thresholds cover the modulo domain")
    }
}

/// Stable ordinal for one of the eight population slots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PopulationSlotV1(u8);

impl PopulationSlotV1 {
    pub(crate) const fn from_index_v1(index: usize) -> Option<Self> {
        if index < POPULATION_OPPONENT_SLOT_COUNT_V1 {
            Some(Self(index as u8))
        } else {
            None
        }
    }

    pub(crate) const fn index_v1(self) -> usize {
        self.0 as usize
    }
}

/// Selects one population slot for an episode using the existing
/// `train-opponent-pool-choice` seed derivation and exactly one modulo draw.
pub(crate) fn population_slot_for_episode_v1(
    base_seed: u64,
    episode_index: u64,
    weights: &PopulationWeightVectorV1,
) -> Result<PopulationSlotV1, PopulationOpponentErrorV1> {
    let seed = derive_native_trainer_opponent_pool_choice_seed_v2(base_seed, episode_index)
        .map_err(|_| PopulationOpponentErrorV1::SeedDerivation)?;
    Ok(weights.select_draw_v1(seed))
}

fn grouped_population_slot_for_episode_v1(
    base_seed: u64,
    episode_index: u64,
    selection_group_size: u64,
    weights: &PopulationWeightVectorV1,
) -> Result<PopulationSlotV1, PopulationOpponentErrorV1> {
    let selection_index = episode_index
        .checked_div(selection_group_size)
        .ok_or(PopulationOpponentErrorV1::InvalidSlot)?;
    population_slot_for_episode_v1(base_seed, selection_index, weights)
}

/// Runtime bridge for one validated eight-slot population snapshot.
///
/// The array is the only model-handle field and contains exactly eight
/// immutable `NativeCheckpointInferenceV1` values. Handles are selected by
/// slot ordinal and are never mutated by this type.
pub(crate) struct PopulationOpponentEngineV1 {
    weights: PopulationWeightVectorV1,
    handles: [NativeCheckpointInferenceV1; POPULATION_OPPONENT_SLOT_COUNT_V1],
    selection_group_size: u64,
    #[cfg(test)]
    selected_episode_slots: Mutex<Vec<(u64, u64, usize)>>,
}

impl fmt::Debug for PopulationOpponentEngineV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PopulationOpponentEngineV1")
            .field("identity", &POPULATION_OPPONENT_IDENTITY_V1)
            .field("total_weight", &self.weights.total_v1())
            .field("slot_count", &POPULATION_OPPONENT_SLOT_COUNT_V1)
            .field("selection_group_size", &self.selection_group_size)
            .finish_non_exhaustive()
    }
}

impl PopulationOpponentEngineV1 {
    pub(crate) fn new_v1(
        weights: PopulationWeightVectorV1,
        handles: [NativeCheckpointInferenceV1; POPULATION_OPPONENT_SLOT_COUNT_V1],
    ) -> Self {
        Self {
            weights,
            handles,
            selection_group_size: 1,
            #[cfg(test)]
            selected_episode_slots: Mutex::new(Vec::new()),
        }
    }

    /// Evaluation-only constructor that selects one component for each
    /// complete seat-swapped pair. Training continues to use one selection
    /// per episode through [`Self::new_v1`].
    #[cfg(test)]
    pub(crate) fn new_pairwise_eval_v1(
        weights: PopulationWeightVectorV1,
        handles: [NativeCheckpointInferenceV1; POPULATION_OPPONENT_SLOT_COUNT_V1],
    ) -> Self {
        Self {
            weights,
            handles,
            selection_group_size: 2,
            selected_episode_slots: Mutex::new(Vec::new()),
        }
    }

    pub(crate) const fn weights_v1(&self) -> &PopulationWeightVectorV1 {
        &self.weights
    }

    pub(crate) fn slot_for_episode_v1(
        &self,
        base_seed: u64,
        episode_index: u64,
    ) -> Result<PopulationSlotV1, PopulationOpponentErrorV1> {
        let slot = grouped_population_slot_for_episode_v1(
            base_seed,
            episode_index,
            self.selection_group_size,
            &self.weights,
        )?;
        #[cfg(test)]
        self.selected_episode_slots
            .lock()
            .expect("population selection trace mutex poisoned")
            .push((base_seed, episode_index, slot.index_v1()));
        Ok(slot)
    }

    #[cfg(test)]
    pub(crate) fn selected_episode_slots_for_test_v1(&self) -> Vec<(u64, u64, usize)> {
        self.selected_episode_slots
            .lock()
            .expect("population selection trace mutex poisoned")
            .clone()
    }

    /// The immutable `(run_sha256, checkpoint_manifest_sha256)` identity
    /// installed at `slot`. Read-only: it performs no inference and mutates
    /// nothing, so callers may use it purely to record which opponent
    /// checkpoint a training episode's `slot_for_episode_v1` result names.
    pub(crate) fn checkpoint_identity_for_slot_v1(
        &self,
        slot: PopulationSlotV1,
    ) -> ([u8; 32], [u8; 32]) {
        let handle = &self.handles[slot.index_v1()];
        (handle.run_sha256(), handle.checkpoint_manifest_sha256())
    }

    /// Scores one decision with the selected immutable checkpoint and reuses
    /// the K4 ladder's temperature-one softmax sampler unchanged.
    pub(crate) fn select_policy_action_v1(
        &self,
        slot: PopulationSlotV1,
        decision: FlatScoringDecisionViewV2<'_>,
        policy_substep_seed: u64,
    ) -> Result<u32, PopulationOpponentErrorV1> {
        let handle = self
            .handles
            .get(slot.index_v1())
            .ok_or(PopulationOpponentErrorV1::InvalidSlot)?;
        let output = handle
            .score_decision_v1(decision)
            .map_err(|_| PopulationOpponentErrorV1::Inference)?;
        softmax_sample_temperature_one_v1(output.action_logits(), policy_substep_seed)
            .map_err(|_| PopulationOpponentErrorV1::Softmax)
    }
}

/// Constructs independently loaded real checkpoint handles for focused
/// rollout integration tests. This is CPU-only and never launches gameplay or
/// training compute.
#[cfg(test)]
pub(crate) fn checkpoint_inference_handles_for_test_v1<const N: usize>(
) -> [NativeCheckpointInferenceV1; N] {
    use crate::native_checkpoint_inference_v1::load_native_checkpoint_inference_v1;
    use crate::native_training_executor_v1::{
        NativeTrainingExecutionConfigV1, NativeTrainingExecutorV1, NativeTrainingNumericalBackendV1,
    };
    use crate::native_training_store_checkpoint_v3::build_genesis_checkpoint_manifest_v3;
    use crate::native_training_store_run_v2::{decode_train_run_v2, test_fixture_bytes_v2};
    use std::time::Duration;

    let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
    let config = NativeTrainingExecutionConfigV1 {
        run_base_seed: run.record().schedule.base_seed,
        batch_episodes: run.batch_episodes(),
        deck_ids: ["Rally".to_owned(), "Rally".to_owned()],
        max_physical_decisions: run.record().limits.max_physical_decisions,
        max_policy_steps: run.record().limits.max_policy_steps,
        worker_count: usize::try_from(run.record().topology.worker_count).unwrap(),
        sessions_per_worker: usize::try_from(run.record().topology.sessions_per_worker).unwrap(),
        broker_batch_target: usize::try_from(run.record().topology.broker_batch_target).unwrap(),
        scheduler_timeout: Duration::from_secs(30),
        measure_broker_service_time: false,
        value_coefficient_bits: 0.5_f32.to_bits(),
        learning_rate_bits: 0.001_f32.to_bits(),
        numerical_backend: NativeTrainingNumericalBackendV1::Sequential,
        backward_worker_limit: 1,
    };
    let (snapshot_manifest, snapshot_payload) =
        crate::common_model_snapshot_v1::common_model_snapshot_paths_v1();
    let executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
        config,
        &snapshot_manifest,
        &snapshot_payload,
    )
    .unwrap();
    let payload = executor
        .checkpoint_candidate_v1()
        .unwrap()
        .payload()
        .to_vec();
    let checkpoint = build_genesis_checkpoint_manifest_v3(&run, &payload).unwrap();
    std::array::from_fn(|_| {
        load_native_checkpoint_inference_v1(&run, &checkpoint, &payload).unwrap()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn equal_weights_v1() -> PopulationWeightVectorV1 {
        PopulationWeightVectorV1::new_v1([25; POPULATION_OPPONENT_SLOT_COUNT_V1], 200).unwrap()
    }

    #[test]
    fn identity_and_slot_count_are_versioned() {
        assert_eq!(
            POPULATION_OPPONENT_IDENTITY_V1,
            "mtg-kernel-native-population-opponent-v1"
        );
        assert_eq!(POPULATION_OPPONENT_SLOT_COUNT_V1, 8);
    }

    #[test]
    fn weights_validate_and_preserve_integer_thresholds() {
        let weights = PopulationWeightVectorV1::new_v1([1, 2, 3, 4, 5, 6, 7, 8], 36).unwrap();
        assert_eq!(weights.weights_v1(), &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(weights.total_v1(), 36);
        assert_eq!(weights.select_draw_v1(0).index_v1(), 0);
        assert_eq!(weights.select_draw_v1(1).index_v1(), 1);
        assert_eq!(weights.select_draw_v1(35).index_v1(), 7);
    }

    #[test]
    fn malformed_totals_and_overflow_fail_closed() {
        assert_eq!(
            PopulationWeightVectorV1::new_v1([1; POPULATION_OPPONENT_SLOT_COUNT_V1], 0),
            Err(PopulationOpponentErrorV1::InvalidWeightTotal)
        );
        assert_eq!(
            PopulationWeightVectorV1::new_v1([1; POPULATION_OPPONENT_SLOT_COUNT_V1], 7),
            Err(PopulationOpponentErrorV1::InvalidWeightTotal)
        );
        assert_eq!(
            PopulationWeightVectorV1::new_v1(
                [u64::MAX; POPULATION_OPPONENT_SLOT_COUNT_V1],
                u64::MAX
            ),
            Err(PopulationOpponentErrorV1::WeightOverflow)
        );
    }

    #[test]
    fn zero_weight_inactive_slots_are_never_selected() {
        let weights = PopulationWeightVectorV1::new_v1([1, 1, 1, 1, 1, 1, 0, 0], 6).unwrap();
        for draw in 0..10_000 {
            assert!(weights.select_draw_v1(draw).index_v1() < 6);
        }
        for active_slot in 0..6 {
            assert_eq!(
                weights.select_draw_v1(active_slot as u64).index_v1(),
                active_slot
            );
        }
    }

    #[test]
    fn modulo_draw_wraps_deterministically_at_weight_boundaries() {
        let weights = equal_weights_v1();
        assert_eq!(weights.select_draw_v1(0).index_v1(), 0);
        assert_eq!(weights.select_draw_v1(24).index_v1(), 0);
        assert_eq!(weights.select_draw_v1(25).index_v1(), 1);
        assert_eq!(weights.select_draw_v1(49).index_v1(), 1);
        assert_eq!(weights.select_draw_v1(50).index_v1(), 2);
        assert_eq!(weights.select_draw_v1(199).index_v1(), 7);
        assert_eq!(weights.select_draw_v1(200).index_v1(), 0);
    }

    #[test]
    fn episode_mapping_is_deterministic_and_uses_existing_seed_namespace() {
        let weights = equal_weights_v1();
        let first = population_slot_for_episode_v1(71_501, 17, &weights).unwrap();
        let second = population_slot_for_episode_v1(71_501, 17, &weights).unwrap();
        assert_eq!(first, second);
        assert!(population_slot_for_episode_v1(1_u64 << 63, 0, &weights).is_err());
        assert!(population_slot_for_episode_v1(0, 1_u64 << 63, &weights).is_err());
    }

    #[test]
    fn pairwise_selection_maps_both_legs_to_one_component() {
        let weights = equal_weights_v1();
        for pair_index in 0..1_024_u64 {
            let expected = population_slot_for_episode_v1(71_502, pair_index, &weights).unwrap();
            for leg in 0..2_u64 {
                let actual = grouped_population_slot_for_episode_v1(
                    71_502,
                    pair_index * 2 + leg,
                    2,
                    &weights,
                )
                .unwrap();
                assert_eq!(actual, expected);
            }
        }
        assert_eq!(
            grouped_population_slot_for_episode_v1(71_502, 0, 0, &weights),
            Err(PopulationOpponentErrorV1::InvalidSlot)
        );
    }

    #[test]
    fn every_slot_dispatch_ordinal_is_in_range() {
        for index in 0..POPULATION_OPPONENT_SLOT_COUNT_V1 {
            let slot = PopulationSlotV1::from_index_v1(index).unwrap();
            assert_eq!(slot.index_v1(), index);
        }
        assert!(PopulationSlotV1::from_index_v1(POPULATION_OPPONENT_SLOT_COUNT_V1).is_none());
    }
}
