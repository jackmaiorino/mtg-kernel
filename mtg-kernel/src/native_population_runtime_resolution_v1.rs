//! Resolve one validated population refresh into immutable runtime handles.
//!
//! Slot roots are positional and carry no authority by themselves. Each root
//! is reopened through its own RunV2 record and complete Store walk, then the
//! named generation and every digest recorded by the refresh manifest are
//! checked before checkpoint inference is constructed.

use crate::native_checkpoint_inference_v1::load_native_checkpoint_inference_v1;
use crate::native_population_opponent_v1::{
    PopulationOpponentEngineV1, PopulationWeightVectorV1,
    POPULATION_OPPONENT_SLOT_COUNT_V1,
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

pub(crate) fn resolve_population_opponent_v1(
    manifest: &PopulationRefreshManifestV1,
    slot_store_roots: &[PathBuf],
) -> Result<PopulationOpponentEngineV1> {
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
            PopulationRuntimeResolutionErrorV1::new(
                PopulationRuntimeResolutionErrorKindV1::RunRead,
            )
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
        let boundary = load_native_training_boundary_v2(
            &root,
            &run,
            slot.source_generation_v1(),
        )
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
            && lower_hex_raw32_v1(checkpoint.checkpoint_payload_sha256())
                == slot.state_sha256_v1()
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

    let handles = handles.try_into().map_err(|_| {
        PopulationRuntimeResolutionErrorV1::new(
            PopulationRuntimeResolutionErrorKindV1::SlotCount,
        )
    })?;
    let weights = std::array::from_fn(|index| manifest.slots_v1()[index].weight_units_v1());
    let total = weights.iter().try_fold(0_u64, |sum, weight| sum.checked_add(*weight)).ok_or_else(
        || {
            PopulationRuntimeResolutionErrorV1::new(
                PopulationRuntimeResolutionErrorKindV1::WeightInvalid,
            )
        },
    )?;
    let weights = PopulationWeightVectorV1::new_v1(weights, total).map_err(|_| {
        PopulationRuntimeResolutionErrorV1::new(
            PopulationRuntimeResolutionErrorKindV1::WeightInvalid,
        )
    })?;
    Ok(PopulationOpponentEngineV1::new_v1(weights, handles))
}
