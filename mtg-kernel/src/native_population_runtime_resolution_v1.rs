//! Resolve one validated population refresh into immutable runtime handles.
//!
//! Slot roots are positional and carry no authority by themselves. Each root
//! is reopened through its own RunV2 record and complete Store walk, then the
//! named generation and every digest recorded by the refresh manifest are
//! checked before checkpoint inference is constructed.

use crate::native_checkpoint_inference_v1::{
    load_native_checkpoint_inference_v1, NativeCheckpointInferenceV1,
};
use crate::native_population_opponent_v1::{
    PopulationOpponentEngineV1, PopulationWeightVectorV1, POPULATION_OPPONENT_SLOT_COUNT_V1,
};
use crate::native_population_refresh_manifest_v1::PopulationRefreshManifestV1;
use crate::native_training_store_digest_v1::lower_hex_raw32_v1;
use crate::native_training_store_resume_v2::load_native_training_boundary_v2;
use crate::native_training_store_root_v2::ValidatedNativeTrainingStoreRootV2;
use crate::native_training_store_run_v2::decode_train_run_v2;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PopulationRuntimeResolutionErrorKindV1 {
    SlotCount,
    RunRead,
    RunInvalid,
    RootInvalid,
    BoundaryInvalid,
    AuthorityMismatch,
    InferenceInvalid,
    WeightInvalid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PopulationRuntimeResolutionErrorV1 {
    kind: PopulationRuntimeResolutionErrorKindV1,
}

impl PopulationRuntimeResolutionErrorV1 {
    const fn new(kind: PopulationRuntimeResolutionErrorKindV1) -> Self {
        Self { kind }
    }

    pub(crate) const fn kind_v1(self) -> PopulationRuntimeResolutionErrorKindV1 {
        self.kind
    }
}

impl Display for PopulationRuntimeResolutionErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}", self.kind)
    }
}

impl Error for PopulationRuntimeResolutionErrorV1 {}

type Result<T> = std::result::Result<T, PopulationRuntimeResolutionErrorV1>;

const POPULATION_RESPONSE_TARGET_SLOT_COUNT_V1: usize = 6;

pub(crate) fn resolve_population_opponent_v1(
    manifest: &PopulationRefreshManifestV1,
    slot_store_roots: &[PathBuf],
) -> Result<PopulationOpponentEngineV1> {
    let handles = resolve_population_handles_v1(manifest, slot_store_roots)?;
    let weights = population_manifest_weight_vector_v1(manifest)?;
    Ok(PopulationOpponentEngineV1::new_v1(weights, handles))
}

/// Resolves the frozen response-exploiter target through all eight manifest
/// authorities while restricting runtime selection to slots 0 through 5.
///
/// The six retained weights are the manifest's original integer units. Slots
/// 6 and 7 are assigned exact zero weight, and the declared total is the
/// checked sum of the retained units, so no proportional rounding occurs.
pub(crate) fn resolve_population_response_target_v1(
    manifest: &PopulationRefreshManifestV1,
    slot_store_roots: &[PathBuf],
) -> Result<PopulationOpponentEngineV1> {
    let handles = resolve_population_handles_v1(manifest, slot_store_roots)?;
    let weights = population_response_target_weight_vector_v1(manifest)?;
    Ok(PopulationOpponentEngineV1::new_v1(weights, handles))
}

/// Evaluation-only sibling of [`resolve_population_response_target_v1`].
/// It authenticates the same eight Store authorities and uses the same six
/// retained integer weights, but keeps one selected component fixed across
/// both legs of each seat-swapped pair.
#[cfg(test)]
pub(crate) fn resolve_population_response_target_pairwise_v1(
    manifest: &PopulationRefreshManifestV1,
    slot_store_roots: &[PathBuf],
) -> Result<PopulationOpponentEngineV1> {
    let handles = resolve_population_handles_v1(manifest, slot_store_roots)?;
    let weights = population_response_target_weight_vector_v1(manifest)?;
    Ok(PopulationOpponentEngineV1::new_pairwise_eval_v1(
        weights, handles,
    ))
}

fn resolve_population_handles_v1(
    manifest: &PopulationRefreshManifestV1,
    slot_store_roots: &[PathBuf],
) -> Result<[NativeCheckpointInferenceV1; POPULATION_OPPONENT_SLOT_COUNT_V1]> {
    if slot_store_roots.len() != POPULATION_OPPONENT_SLOT_COUNT_V1
        || manifest.slots_v1().len() != POPULATION_OPPONENT_SLOT_COUNT_V1
    {
        return Err(PopulationRuntimeResolutionErrorV1::new(
            PopulationRuntimeResolutionErrorKindV1::SlotCount,
        ));
    }

    let mut handles = Vec::with_capacity(POPULATION_OPPONENT_SLOT_COUNT_V1);
    for (slot, store_root) in manifest.slots_v1().iter().zip(slot_store_roots) {
        let run_bytes = fs::read(store_root.join("run.json")).map_err(|_| {
            PopulationRuntimeResolutionErrorV1::new(PopulationRuntimeResolutionErrorKindV1::RunRead)
        })?;
        let run = decode_train_run_v2(&run_bytes).map_err(|_| {
            PopulationRuntimeResolutionErrorV1::new(
                PopulationRuntimeResolutionErrorKindV1::RunInvalid,
            )
        })?;
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(store_root).map_err(|_| {
            PopulationRuntimeResolutionErrorV1::new(
                PopulationRuntimeResolutionErrorKindV1::RootInvalid,
            )
        })?;
        let boundary = load_native_training_boundary_v2(&root, &run, slot.source_generation_v1())
            .map_err(|_| {
            PopulationRuntimeResolutionErrorV1::new(
                PopulationRuntimeResolutionErrorKindV1::BoundaryInvalid,
            )
        })?;
        let checkpoint = boundary.checkpoint();
        let matches_authority = run.run_sha256() == slot.source_run_sha256_v1()
            && run.record().schedule.base_seed == slot.source_base_seed_v1()
            && checkpoint.generation_index() == slot.source_generation_v1()
            && lower_hex_raw32_v1(checkpoint.checkpoint_manifest_sha256())
                == slot.checkpoint_sha256_v1()
            && lower_hex_raw32_v1(boundary.boundary().checkpoint_sidecar_sha256())
                == slot.sidecar_sha256_v1()
            && lower_hex_raw32_v1(checkpoint.checkpoint_payload_sha256()) == slot.state_sha256_v1()
            && lower_hex_raw32_v1(checkpoint.model_parameter_sha256())
                == slot.model_parameter_sha256_v1();
        if !matches_authority {
            return Err(PopulationRuntimeResolutionErrorV1::new(
                PopulationRuntimeResolutionErrorKindV1::AuthorityMismatch,
            ));
        }
        handles.push(
            load_native_checkpoint_inference_v1(&run, checkpoint, boundary.payload()).map_err(
                |_| {
                    PopulationRuntimeResolutionErrorV1::new(
                        PopulationRuntimeResolutionErrorKindV1::InferenceInvalid,
                    )
                },
            )?,
        );
    }

    handles.try_into().map_err(|_| {
        PopulationRuntimeResolutionErrorV1::new(PopulationRuntimeResolutionErrorKindV1::SlotCount)
    })
}

fn population_manifest_weight_vector_v1(
    manifest: &PopulationRefreshManifestV1,
) -> Result<PopulationWeightVectorV1> {
    population_weight_vector_for_prefix_v1(manifest, POPULATION_OPPONENT_SLOT_COUNT_V1)
}

fn population_response_target_weight_vector_v1(
    manifest: &PopulationRefreshManifestV1,
) -> Result<PopulationWeightVectorV1> {
    population_weight_vector_for_prefix_v1(manifest, POPULATION_RESPONSE_TARGET_SLOT_COUNT_V1)
}

fn population_weight_vector_for_prefix_v1(
    manifest: &PopulationRefreshManifestV1,
    retained_slot_count: usize,
) -> Result<PopulationWeightVectorV1> {
    let weights = std::array::from_fn(|index| {
        if index < retained_slot_count {
            manifest.slots_v1()[index].weight_units_v1()
        } else {
            0
        }
    });
    let total = weights
        .iter()
        .try_fold(0_u64, |sum, weight| sum.checked_add(*weight))
        .ok_or_else(|| {
            PopulationRuntimeResolutionErrorV1::new(
                PopulationRuntimeResolutionErrorKindV1::WeightInvalid,
            )
        })?;
    let weights = PopulationWeightVectorV1::new_v1(weights, total).map_err(|_| {
        PopulationRuntimeResolutionErrorV1::new(
            PopulationRuntimeResolutionErrorKindV1::WeightInvalid,
        )
    })?;
    Ok(weights)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_population_opponent_v1::population_slot_for_episode_v1;
    use crate::native_population_refresh_manifest_v1::{
        build_population_refresh_manifest_v1, PopulationRefreshSlotV1,
    };

    const TEST_WEIGHTS_V1: [u64; POPULATION_OPPONENT_SLOT_COUNT_V1] = [
        125_407, 115_542, 127_252, 127_098, 128_077, 127_916, 122_718, 125_990,
    ];

    fn digest_v1(value: usize) -> String {
        format!("{value:064x}")
    }

    fn manifest_v1() -> PopulationRefreshManifestV1 {
        let roles = [
            "anchor-0",
            "anchor-1",
            "historical-0",
            "historical-1",
            "current-0",
            "current-1",
            "exploiter-0",
            "exploiter-1",
        ];
        let slots = (0..POPULATION_OPPONENT_SLOT_COUNT_V1)
            .map(|index| {
                let (base_seed, generation, run, checkpoint, sidecar, state, model) = match index {
                    0 => (
                        920_012,
                        384,
                        "2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae"
                            .to_owned(),
                        "4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8"
                            .to_owned(),
                        "7511c0377edd4e8d918fa5843f89a0270a8264e5466c329f6b4ef18bbf9e76bb"
                            .to_owned(),
                        "a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99"
                            .to_owned(),
                        "db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d"
                            .to_owned(),
                    ),
                    1 => (
                        920_005,
                        512,
                        "8bc06b6cf2e26df8002b5cece2784e0cd165cdd6bbd199a835e06c17e8d5de5c"
                            .to_owned(),
                        "03f0e226f884f51bf7128f70bec189bd6ac2c8f231ced8886f2cb7d3e936cc90"
                            .to_owned(),
                        "c56a8ba1361ab172c669307084c4522ee06ac79e39b7cf4a306f11effe36b031"
                            .to_owned(),
                        "2904dd7b899c21234c64925440277dbfa8d6f552d8f620b153bc8d16c44f523a"
                            .to_owned(),
                        "0635d2defb8facd700ede34789434956fc4a2fd3b5058cc2df5dd820398b4c22"
                            .to_owned(),
                    ),
                    2 | 3 => (
                        970_003,
                        if index == 2 { 256 } else { 128 },
                        digest_v1(10 + index),
                        digest_v1(20 + index),
                        digest_v1(30 + index),
                        digest_v1(40 + index),
                        digest_v1(50 + index),
                    ),
                    4 | 5 => (
                        970_001 + (index - 4) as u64,
                        512,
                        digest_v1(10 + index),
                        digest_v1(20 + index),
                        digest_v1(30 + index),
                        digest_v1(40 + index),
                        digest_v1(50 + index),
                    ),
                    _ => (
                        980_000 + index as u64,
                        256,
                        digest_v1(10 + index),
                        digest_v1(20 + index),
                        digest_v1(30 + index),
                        digest_v1(40 + index),
                        digest_v1(50 + index),
                    ),
                };
                PopulationRefreshSlotV1::new_v1(
                    index as u64,
                    roles[index],
                    "policy",
                    base_seed,
                    run,
                    generation,
                    512,
                    checkpoint,
                    sidecar,
                    state,
                    model,
                    TEST_WEIGHTS_V1[index],
                )
            })
            .collect();
        build_population_refresh_manifest_v1(0, None, None, slots).unwrap()
    }

    #[test]
    fn response_target_preserves_six_integer_weights_and_exact_total() {
        let weights = population_response_target_weight_vector_v1(&manifest_v1()).unwrap();
        assert_eq!(
            weights.weights_v1(),
            &[125_407, 115_542, 127_252, 127_098, 128_077, 127_916, 0, 0]
        );
        assert_eq!(weights.total_v1(), 751_292);
    }

    #[test]
    fn response_target_never_selects_excluded_slots() {
        let weights = population_response_target_weight_vector_v1(&manifest_v1()).unwrap();
        let mut selected = [false; POPULATION_RESPONSE_TARGET_SLOT_COUNT_V1];
        for episode_index in 0..10_000 {
            let slot = population_slot_for_episode_v1(71_501, episode_index, &weights).unwrap();
            assert!(slot.index_v1() < POPULATION_RESPONSE_TARGET_SLOT_COUNT_V1);
            selected[slot.index_v1()] = true;
        }
        assert!(selected.into_iter().all(|value| value));
    }

    #[test]
    fn response_target_rejects_bad_root_count_before_resolution() {
        let roots = std::array::from_fn::<_, 7, _>(|_| PathBuf::from("unused"));
        assert_eq!(
            resolve_population_response_target_v1(&manifest_v1(), &roots)
                .unwrap_err()
                .kind_v1(),
            PopulationRuntimeResolutionErrorKindV1::SlotCount
        );
        assert_eq!(
            resolve_population_response_target_pairwise_v1(&manifest_v1(), &roots)
                .unwrap_err()
                .kind_v1(),
            PopulationRuntimeResolutionErrorKindV1::SlotCount
        );
    }

    #[test]
    fn ordinary_resolver_weight_path_is_unchanged() {
        let manifest = manifest_v1();
        let weights = population_manifest_weight_vector_v1(&manifest).unwrap();
        assert_eq!(weights.weights_v1(), &TEST_WEIGHTS_V1);
        assert_eq!(weights.total_v1(), 1_000_000);

        let roots = std::array::from_fn::<_, 7, _>(|_| PathBuf::from("unused"));
        assert_eq!(
            resolve_population_opponent_v1(&manifest, &roots)
                .unwrap_err()
                .kind_v1(),
            PopulationRuntimeResolutionErrorKindV1::SlotCount
        );
    }
}
