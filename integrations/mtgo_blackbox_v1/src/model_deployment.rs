use crate::{
    model_deployment_commitment_v1, MtgoContractErrorV1, MtgoExpectedModelDeploymentV1,
    MtgoNativeCheckpointObservationScorerV1,
};
use mtg_kernel::native_checkpoint_inference_v1::{
    load_native_checkpoint_inference_v1, NativeCheckpointInferenceV1,
};
use mtg_kernel::native_training_store_resume_v2::load_native_training_boundary_v2;
use mtg_kernel::native_training_store_root_v2::ValidatedNativeTrainingStoreRootV2;
use mtg_kernel::native_training_store_run_v2::decode_train_run_v2;
use std::fmt::{Debug, Formatter};
use std::fs;
use std::path::Path;

const MAX_RUN_JSON_BYTES_V1: u64 = 1_048_576;

/// One exact native checkpoint deployment loaded through the validated Store
/// walk and rebound to an independently supplied MTGO deployment identity.
///
/// This value is move-only and has no input, capture, account, event-entry, or
/// match authority. Its only operational surface is creation of the existing
/// observation scorer.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::LoadedMtgoNativeCheckpointDeploymentV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<LoadedMtgoNativeCheckpointDeploymentV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::LoadedMtgoNativeCheckpointDeploymentV1;
/// use serde::Serialize;
/// fn require_serialize<T: Serialize>() {}
/// require_serialize::<LoadedMtgoNativeCheckpointDeploymentV1>();
/// ```
pub struct LoadedMtgoNativeCheckpointDeploymentV1 {
    expected: MtgoExpectedModelDeploymentV1,
    deployment_commitment_sha256: String,
    inference: NativeCheckpointInferenceV1,
}

impl Debug for LoadedMtgoNativeCheckpointDeploymentV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LoadedMtgoNativeCheckpointDeploymentV1")
            .field("deployment_id", &self.expected.deployment_id)
            .field(
                "generation_index",
                &self.expected.checkpoint.generation_index,
            )
            .field(
                "deployment_commitment_sha256",
                &self.deployment_commitment_sha256,
            )
            .finish_non_exhaustive()
    }
}

impl LoadedMtgoNativeCheckpointDeploymentV1 {
    pub fn deployment_id(&self) -> &str {
        &self.expected.deployment_id
    }

    pub fn generation_index(&self) -> u64 {
        self.expected.checkpoint.generation_index
    }

    pub fn deployment_commitment_sha256(&self) -> &str {
        &self.deployment_commitment_sha256
    }

    pub fn scorer_v1(
        &self,
    ) -> Result<MtgoNativeCheckpointObservationScorerV1<'_>, MtgoContractErrorV1> {
        MtgoNativeCheckpointObservationScorerV1::new_v1(&self.inference, &self.expected)
    }

    pub fn safe_for_live_input(&self) -> bool {
        false
    }

    pub fn permits_match_entry(&self) -> bool {
        false
    }
}

/// Loads one exact checkpoint from a complete native Store root.
///
/// The Store loader validates `run.json` and the complete Store through its
/// latest pointer, then rewalks every boundary through the requested
/// generation before returning its checkpoint manifest and complete payload.
/// The inference handle is constructed only after that walk. The final scorer
/// constructor then compares all six checkpoint identity fields and the
/// scorer-contract digest with the independently supplied deployment record.
pub fn load_mtgo_native_checkpoint_deployment_v1(
    store_root: impl AsRef<Path>,
    expected: MtgoExpectedModelDeploymentV1,
) -> Result<LoadedMtgoNativeCheckpointDeploymentV1, MtgoContractErrorV1> {
    let deployment_commitment_sha256 = model_deployment_commitment_v1(&expected)?;
    let store_root = store_root.as_ref();
    let run_path = store_root.join("run.json");
    let run_metadata = fs::metadata(&run_path).map_err(|_| {
        error_v1(
            "mtgo_model_deployment_run_metadata_failed",
            "the selected Store run.json could not be inspected",
        )
    })?;
    if !run_metadata.is_file() || run_metadata.len() > MAX_RUN_JSON_BYTES_V1 {
        return Err(error_v1(
            "mtgo_model_deployment_run_size_invalid",
            "the selected Store run.json is absent or exceeds the fixed byte cap",
        ));
    }
    let run_bytes = fs::read(&run_path).map_err(|_| {
        error_v1(
            "mtgo_model_deployment_run_read_failed",
            "the selected Store run.json could not be read",
        )
    })?;
    let run = decode_train_run_v2(&run_bytes).map_err(|_| {
        error_v1(
            "mtgo_model_deployment_run_invalid",
            "the selected Store run.json failed the native contract",
        )
    })?;
    let root = ValidatedNativeTrainingStoreRootV2::open_v2(store_root).map_err(|_| {
        error_v1(
            "mtgo_model_deployment_store_invalid",
            "the selected native Store root failed validation",
        )
    })?;
    let boundary =
        load_native_training_boundary_v2(&root, &run, expected.checkpoint.generation_index)
            .map_err(|_| {
                error_v1(
                    "mtgo_model_deployment_boundary_invalid",
                    "the selected checkpoint boundary failed the complete Store walk",
                )
            })?;
    let inference =
        load_native_checkpoint_inference_v1(&run, boundary.checkpoint(), boundary.payload())
            .map_err(|_| {
                error_v1(
                    "mtgo_model_deployment_inference_invalid",
                    "the selected checkpoint could not construct the native inference handle",
                )
            })?;
    MtgoNativeCheckpointObservationScorerV1::new_v1(&inference, &expected)?;

    Ok(LoadedMtgoNativeCheckpointDeploymentV1 {
        expected,
        deployment_commitment_sha256,
        inference,
    })
}

fn error_v1(code: &'static str, detail: &'static str) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provisional_deployment_v1() -> MtgoExpectedModelDeploymentV1 {
        serde_json::from_str(include_str!(
            "../fixtures/provisional_promoted2_mtgo_deployment_20260810_v1.json"
        ))
        .expect("checked-in provisional deployment must parse")
    }

    #[test]
    fn provisional_promoted2_identity_is_exact_and_non_actuating() {
        let deployment = provisional_deployment_v1();
        assert_eq!(
            deployment.deployment_id,
            "promoted2-generation384-provisional-mtgo-wiring"
        );
        assert_eq!(deployment.checkpoint.generation_index, 384);
        assert_eq!(
            deployment.checkpoint.run_sha256,
            "2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae"
        );
        assert_eq!(
            deployment.checkpoint.model_parameter_sha256,
            "db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d"
        );
        assert_eq!(
            model_deployment_commitment_v1(&deployment).unwrap(),
            "1a3288a47a96989b4f4eddd22b0fa2f8db6b083420fd6329e385a7f50641849e"
        );
    }

    #[test]
    fn malformed_expected_identity_fails_before_store_access() {
        let mut deployment = provisional_deployment_v1();
        deployment.checkpoint.run_sha256 = "not-a-digest".to_owned();
        let error =
            load_mtgo_native_checkpoint_deployment_v1("this-path-must-not-be-read", deployment)
                .expect_err("invalid expected identity must fail");
        assert_eq!(error.code(), "external_scoring_deployment_hash_invalid");
    }
}
