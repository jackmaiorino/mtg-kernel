//! Pure in-memory checkpoint-to-inference bridge for the native model.
//!
//! This module adds no artifact schema, codec, path, publication, seed, or
//! evaluator contract. It consumes authorities that were validated elsewhere,
//! independently rechecks the bindings needed for inference, validates the
//! complete train-state payload, and retains only a private immutable model and
//! digest facts. Optimizer moments are validated during load and then dropped.

use crate::async_flat_scored_rollout_v2::{
    expected_scorer_contract, FlatBatchScorerErrorV2, FlatBatchScorerV2, FlatScoringBatchViewV2,
};
use crate::flat_policy_v2::FlatScoringDecisionViewV2;
use crate::native_flat_tensorizer_v2::{
    NativeFlatDecisionTensorV2, NativeFlatTensorErrorV2, NativeFlatTensorizerV2,
};
use crate::native_policy_value_net_v1::{
    NativeEncodedDecisionSchemaV1, NativeEncodedDecisionViewV1, NativePolicyValueErrorV1,
    NativePolicyValueModelConfigV1, NativePolicyValueNetV1, NativePolicyValueNetWideV1,
    FEATURE_CONTRACT_DIGEST_V1, FEATURE_ENCODING_DIGEST_V1, MODEL_ARCHITECTURE_VERSION_V1,
    MODEL_CONFIG_FINGERPRINT_V1, PARAMETER_COUNT_V1, W_MODEL_ARCHITECTURE_VERSION_V1,
    W_MODEL_CONFIG_FINGERPRINT_V1, W_PARAMETER_COUNT_V1,
};
use crate::native_train_state_payload_v1::{
    decode_native_train_state_payload_verified_v1,
    decode_native_train_state_payload_verified_wide_v1, NativeTrainStatePayloadDigestFieldV1,
    NativeTrainStatePayloadDigestsV1, NativeTrainStatePayloadErrorV1,
    NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1, NATIVE_TRAIN_STATE_PAYLOAD_ENCODING_V1,
    NATIVE_TRAIN_STATE_PAYLOAD_SCHEMA_V1, NATIVE_TRAIN_STATE_PAYLOAD_SECTIONS_V1,
    W_NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1, W_NATIVE_TRAIN_STATE_PAYLOAD_SECTIONS_V1,
};
use crate::native_training_store_checkpoint_v3::CheckpointManifestV3;
use crate::native_training_store_digest_v1::{
    lower_hex_raw32_v1, parse_lower_hex_raw32_v1, sha256_v1,
};
use crate::native_training_store_run_v2::ValidatedTrainRunV2;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};

// These codes intentionally match the equivalent native-trainer scorer
// classes. They are scorer-protocol diagnostics, not persisted Store fields.
pub const NATIVE_CHECKPOINT_SCORER_CONTRACT_CODE_V1: u32 = 1;
pub const NATIVE_CHECKPOINT_SCORER_OUTPUT_SHAPE_CODE_V1: u32 = 2;
pub const NATIVE_CHECKPOINT_SCORER_MISSING_DECISION_CODE_V1: u32 = 3;
pub const NATIVE_CHECKPOINT_SCORER_DECISION_CODE_V1: u32 = 4;
pub const NATIVE_CHECKPOINT_SCORER_MODEL_CODE_V1: u32 = 5;

/// Stable failure classes for the pure checkpoint-to-inference boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeCheckpointInferenceErrorKindV1 {
    AuthorityBinding,
    PayloadExactLength,
    PayloadDigestMismatch,
    PayloadInvalid,
    ModelInvalid,
    DecisionInvalid,
    ScoringInvalid,
}

impl NativeCheckpointInferenceErrorKindV1 {
    pub const fn code(self) -> &'static str {
        match self {
            Self::AuthorityBinding => "native_checkpoint_inference_v1_authority_binding",
            Self::PayloadExactLength => "native_checkpoint_inference_v1_payload_exact_length",
            Self::PayloadDigestMismatch => "native_checkpoint_inference_v1_payload_digest_mismatch",
            Self::PayloadInvalid => "native_checkpoint_inference_v1_payload_invalid",
            Self::ModelInvalid => "native_checkpoint_inference_v1_model_invalid",
            Self::DecisionInvalid => "native_checkpoint_inference_v1_decision_invalid",
            Self::ScoringInvalid => "native_checkpoint_inference_v1_scoring_invalid",
        }
    }
}

/// Path-free error value that exposes no payload or model contents.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeCheckpointInferenceErrorV1 {
    kind: NativeCheckpointInferenceErrorKindV1,
}

impl NativeCheckpointInferenceErrorV1 {
    const fn new(kind: NativeCheckpointInferenceErrorKindV1) -> Self {
        Self { kind }
    }

    pub const fn kind(self) -> NativeCheckpointInferenceErrorKindV1 {
        self.kind
    }

    pub const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl Display for NativeCheckpointInferenceErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for NativeCheckpointInferenceErrorV1 {}

type Result<T> = std::result::Result<T, NativeCheckpointInferenceErrorV1>;

/// Immutable result of scoring one existing V2 flat decision view.
///
/// Fields remain private so a caller cannot confuse partially filled output
/// buffers with a successful model result.
#[derive(Clone, PartialEq)]
pub struct NativeCheckpointInferenceOutputV1 {
    action_logits: Vec<f32>,
    value: f32,
}

impl Debug for NativeCheckpointInferenceOutputV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeCheckpointInferenceOutputV1")
            .field("action_logit_count", &self.action_logits.len())
            .finish_non_exhaustive()
    }
}

// The library runner is the intended first production consumer. Keeping these
// accessors crate-private prevents an external raw-score API in this slice.
#[allow(dead_code)]
impl NativeCheckpointInferenceOutputV1 {
    pub(crate) fn action_logits(&self) -> &[f32] {
        &self.action_logits
    }

    pub(crate) fn value(&self) -> f32 {
        self.value
    }
}

/// Move-only native inference model loaded from one exact checkpoint payload.
///
/// The model, optimizer state, and all construction fields are private. The
/// type deliberately implements neither `Clone` nor serialization:
///
/// ```compile_fail
/// use mtg_kernel::native_checkpoint_inference_v1::NativeCheckpointInferenceV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<NativeCheckpointInferenceV1>();
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_checkpoint_inference_v1::NativeCheckpointInferenceV1;
/// use serde::Serialize;
/// fn require_serialize<T: Serialize>() {}
/// require_serialize::<NativeCheckpointInferenceV1>();
/// ```
///
/// There is no model accessor or mutable escape hatch:
///
/// ```compile_fail
/// use mtg_kernel::native_checkpoint_inference_v1::NativeCheckpointInferenceV1;
/// fn mutable_model(handle: &mut NativeCheckpointInferenceV1) {
///     let _ = handle.model_v1();
/// }
/// ```
///
/// Optimizer state has no accessor either:
///
/// ```compile_fail
/// use mtg_kernel::native_checkpoint_inference_v1::NativeCheckpointInferenceV1;
/// fn optimizer_state(handle: &NativeCheckpointInferenceV1) {
///     let _ = handle.first_moments_v1();
/// }
/// ```
pub struct NativeCheckpointInferenceV1 {
    model: NativePolicyValueNetV1,
    run_sha256: [u8; 32],
    checkpoint_manifest_sha256: [u8; 32],
    checkpoint_payload_sha256: [u8; 32],
    train_state_sha256: [u8; 32],
    model_parameter_sha256: [u8; 32],
    generation_index: u64,
}

impl Debug for NativeCheckpointInferenceV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeCheckpointInferenceV1")
            .field("run_sha256", &lower_hex_raw32_v1(self.run_sha256))
            .field(
                "checkpoint_manifest_sha256",
                &lower_hex_raw32_v1(self.checkpoint_manifest_sha256),
            )
            .field(
                "checkpoint_payload_sha256",
                &lower_hex_raw32_v1(self.checkpoint_payload_sha256),
            )
            .field(
                "train_state_sha256",
                &lower_hex_raw32_v1(self.train_state_sha256),
            )
            .field(
                "model_parameter_sha256",
                &lower_hex_raw32_v1(self.model_parameter_sha256),
            )
            .field("generation_index", &self.generation_index)
            .finish_non_exhaustive()
    }
}

impl NativeCheckpointInferenceV1 {
    pub const fn run_sha256(&self) -> [u8; 32] {
        self.run_sha256
    }

    pub const fn checkpoint_manifest_sha256(&self) -> [u8; 32] {
        self.checkpoint_manifest_sha256
    }

    pub const fn checkpoint_payload_sha256(&self) -> [u8; 32] {
        self.checkpoint_payload_sha256
    }

    pub const fn train_state_sha256(&self) -> [u8; 32] {
        self.train_state_sha256
    }

    pub const fn model_parameter_sha256(&self) -> [u8; 32] {
        self.model_parameter_sha256
    }

    pub const fn generation_index(&self) -> u64 {
        self.generation_index
    }

    /// The typed native net behind this handle, for the model-guided
    /// (test-time search) selector's narrow search-capable seam
    /// (`LEAD_TEST_TIME_SEARCH_DESIGN_SKETCH_V2.md` Section 5, S0: "a
    /// narrow search-capable model interface that retains the typed native
    /// net (the `dyn` scorer keeps flat scoring)").
    ///
    /// `pub(crate)` and immutable, so this does not widen the handle's
    /// external surface: `NativePolicyValueNetV1` is itself `pub(crate)`,
    /// and a shared reference cannot mutate parameters, replace a
    /// snapshot, or reach optimizer state. The searcher needs the typed
    /// net because `forward_search_deterministic_v1` (the MXCSR-gated,
    /// kernel-tanh variant) exists only there; `score_decision_v1`, the
    /// flat path, cannot express it.
    pub(crate) const fn search_model_v1(&self) -> &NativePolicyValueNetV1 {
        &self.model
    }

    /// Scores one already-typed V2 decision using only immutable handle state.
    /// Tensorization uses fresh local scratch, so a failed decision cannot
    /// poison this handle or any independently loaded handle.
    pub fn score_decision_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
    ) -> Result<NativeCheckpointInferenceOutputV1> {
        let mut tensorizer = NativeFlatTensorizerV2::new();
        let mut tensor = NativeFlatDecisionTensorV2::default();
        self.score_decision_with_scratch_v1(decision, &mut tensorizer, &mut tensor)
    }

    /// Search-scoped sibling of [`Self::score_decision_v1`]: identical
    /// tensorization and output validation, but routes through
    /// [`NativePolicyValueNetV1::forward_search_deterministic_v1`] instead
    /// of [`NativePolicyValueNetV1::forward_v1`], so the net's activation
    /// uses the kernel-owned, libm-free
    /// [`crate::deterministic_math_v1::tanh_f32_v1`] instead of
    /// `f32::tanh()`. `CLAUDE-MODEL-GUIDED-SEARCHER-DESIGN-V1.md` Section
    /// 1.5, Option S
    /// (`docs/audits/model_guided_forward_determinism_audit_v1.md`):
    /// nothing on the production, checkpoint-runner, or training path calls
    /// this method today. It exists so the audit's own determinism probe
    /// can exercise the search-scoped forward variant through the same
    /// checkpoint-inference boundary the production path uses
    /// (`native_forward_determinism_probe_v1.rs`); the future
    /// model-guided-searcher leaf-evaluation wiring (design items 5/6) is
    /// the intended production consumer, not built by this change.
    pub fn score_decision_search_deterministic_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
    ) -> Result<NativeCheckpointInferenceOutputV1> {
        let mut tensorizer = NativeFlatTensorizerV2::new();
        let mut tensor = NativeFlatDecisionTensorV2::default();
        self.score_decision_with_scratch_search_deterministic_v1(
            decision,
            &mut tensorizer,
            &mut tensor,
        )
    }

    /// Creates a reusable fail-closed adapter for the existing V2 rollout
    /// scorer contract. The adapter borrows this immutable checkpoint handle;
    /// no model or optimizer state is copied or exposed.
    pub fn batch_scorer_v1(&self) -> NativeCheckpointBatchScorerV1<'_> {
        NativeCheckpointBatchScorerV1::new_v1(self)
    }

    fn score_decision_with_scratch_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
        tensorizer: &mut NativeFlatTensorizerV2,
        tensor: &mut NativeFlatDecisionTensorV2,
    ) -> Result<NativeCheckpointInferenceOutputV1> {
        tensorizer
            .fill(decision, tensor)
            .map_err(map_tensor_error_v1)?;
        let output = self
            .model
            .forward_v1(encoded_decision_view_v1(tensor))
            .map_err(map_scoring_error_v1)?;
        if output.logits.len() != decision.actions().len()
            || output.logits.is_empty()
            || output.logits.iter().any(|value| !value.is_finite())
            || !output.value.is_finite()
        {
            return Err(NativeCheckpointInferenceErrorV1::new(
                NativeCheckpointInferenceErrorKindV1::ScoringInvalid,
            ));
        }
        Ok(NativeCheckpointInferenceOutputV1 {
            action_logits: output.logits,
            value: output.value,
        })
    }

    /// Search-scoped sibling of [`Self::score_decision_with_scratch_v1`]:
    /// identical tensorization, scratch reuse, and output validation, but
    /// calls `forward_search_deterministic_v1` instead of `forward_v1`. See
    /// [`Self::score_decision_search_deterministic_v1`] for the full
    /// scope note.
    fn score_decision_with_scratch_search_deterministic_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
        tensorizer: &mut NativeFlatTensorizerV2,
        tensor: &mut NativeFlatDecisionTensorV2,
    ) -> Result<NativeCheckpointInferenceOutputV1> {
        tensorizer
            .fill(decision, tensor)
            .map_err(map_tensor_error_v1)?;
        let output = self
            .model
            .forward_search_deterministic_v1(encoded_decision_view_v1(tensor))
            .map_err(map_scoring_error_v1)?;
        if output.logits.len() != decision.actions().len()
            || output.logits.is_empty()
            || output.logits.iter().any(|value| !value.is_finite())
            || !output.value.is_finite()
        {
            return Err(NativeCheckpointInferenceErrorV1::new(
                NativeCheckpointInferenceErrorKindV1::ScoringInvalid,
            ));
        }
        Ok(NativeCheckpointInferenceOutputV1 {
            action_logits: output.logits,
            value: output.value,
        })
    }
}

/// Reusable V2 rollout scorer borrowing one immutable checkpoint model.
///
/// The adapter validates the complete batch shape and scorer contract before
/// inference. It stages every result privately and changes caller output
/// slices only after the whole batch succeeds. Any failure permanently poisons
/// that adapter instance with its first stable code; construct a fresh adapter
/// from the unchanged inference handle to retry a corrected workload.
///
/// Its fields are private and it is deliberately neither cloneable nor
/// serializable:
///
/// ```compile_fail
/// use mtg_kernel::native_checkpoint_inference_v1::NativeCheckpointBatchScorerV1;
/// let _ = NativeCheckpointBatchScorerV1 {};
/// ```
///
/// ```compile_fail
/// use mtg_kernel::native_checkpoint_inference_v1::NativeCheckpointBatchScorerV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<NativeCheckpointBatchScorerV1<'static>>();
/// ```
pub struct NativeCheckpointBatchScorerV1<'a> {
    inference: &'a NativeCheckpointInferenceV1,
    tensorizer: NativeFlatTensorizerV2,
    tensor: NativeFlatDecisionTensorV2,
    candidate_logits: Vec<f32>,
    candidate_values: Vec<f32>,
    first_failure_code: Option<u32>,
}

impl Debug for NativeCheckpointBatchScorerV1<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeCheckpointBatchScorerV1")
            .field("generation_index", &self.inference.generation_index)
            .field("first_failure_code", &self.first_failure_code)
            .finish_non_exhaustive()
    }
}

impl<'a> NativeCheckpointBatchScorerV1<'a> {
    fn new_v1(inference: &'a NativeCheckpointInferenceV1) -> Self {
        Self {
            inference,
            tensorizer: NativeFlatTensorizerV2::new(),
            tensor: NativeFlatDecisionTensorV2::default(),
            candidate_logits: Vec::new(),
            candidate_values: Vec::new(),
            first_failure_code: None,
        }
    }

    pub const fn first_failure_code(&self) -> Option<u32> {
        self.first_failure_code
    }

    fn score_batch_checked_v1(
        &mut self,
        batch: &FlatScoringBatchViewV2<'_>,
        action_logits: &mut [f32],
        values: &mut [f32],
    ) -> std::result::Result<(), u32> {
        let contract = batch.contract();
        if contract != expected_scorer_contract(contract.card_db_hash) {
            return Err(NATIVE_CHECKPOINT_SCORER_CONTRACT_CODE_V1);
        }

        let decision_count = batch.decision_count();
        let action_offsets = batch.action_offsets();
        if decision_count == 0
            || values.len() != decision_count
            || action_logits.is_empty()
            || action_logits.len() != batch.total_action_count()
            || action_offsets.len() != decision_count + 1
            || action_offsets.first().copied() != Some(0)
            || action_offsets.last().copied() != Some(action_logits.len())
        {
            return Err(NATIVE_CHECKPOINT_SCORER_OUTPUT_SHAPE_CODE_V1);
        }

        self.candidate_logits.clear();
        self.candidate_values.clear();
        self.candidate_logits
            .try_reserve_exact(action_logits.len())
            .map_err(|_| NATIVE_CHECKPOINT_SCORER_OUTPUT_SHAPE_CODE_V1)?;
        self.candidate_values
            .try_reserve_exact(values.len())
            .map_err(|_| NATIVE_CHECKPOINT_SCORER_OUTPUT_SHAPE_CODE_V1)?;

        for decision_index in 0..decision_count {
            let decision = batch
                .decision(decision_index)
                .ok_or(NATIVE_CHECKPOINT_SCORER_MISSING_DECISION_CODE_V1)?;
            let begin = action_offsets[decision_index];
            let end = action_offsets[decision_index + 1];
            if end <= begin || end > action_logits.len() || end - begin != decision.actions().len()
            {
                return Err(NATIVE_CHECKPOINT_SCORER_OUTPUT_SHAPE_CODE_V1);
            }
            let output = self
                .inference
                .score_decision_with_scratch_v1(decision, &mut self.tensorizer, &mut self.tensor)
                .map_err(batch_scorer_code_v1)?;
            if output.action_logits.len() != end - begin
                || output.action_logits.iter().any(|value| !value.is_finite())
                || !output.value.is_finite()
            {
                return Err(NATIVE_CHECKPOINT_SCORER_OUTPUT_SHAPE_CODE_V1);
            }
            self.candidate_logits
                .extend_from_slice(&output.action_logits);
            self.candidate_values.push(output.value);
        }

        if self.candidate_logits.len() != action_logits.len()
            || self.candidate_values.len() != values.len()
        {
            return Err(NATIVE_CHECKPOINT_SCORER_OUTPUT_SHAPE_CODE_V1);
        }
        action_logits.copy_from_slice(&self.candidate_logits);
        values.copy_from_slice(&self.candidate_values);
        Ok(())
    }
}

impl FlatBatchScorerV2 for NativeCheckpointBatchScorerV1<'_> {
    fn score_batch_v2(
        &mut self,
        batch: &FlatScoringBatchViewV2<'_>,
        action_logits: &mut [f32],
        values: &mut [f32],
    ) -> std::result::Result<(), FlatBatchScorerErrorV2> {
        if let Some(code) = self.first_failure_code {
            return Err(FlatBatchScorerErrorV2::new(code));
        }
        match self.score_batch_checked_v1(batch, action_logits, values) {
            Ok(()) => Ok(()),
            Err(code) => {
                self.first_failure_code = Some(code);
                Err(FlatBatchScorerErrorV2::new(code))
            }
        }
    }
}

fn batch_scorer_code_v1(error: NativeCheckpointInferenceErrorV1) -> u32 {
    match error.kind() {
        NativeCheckpointInferenceErrorKindV1::DecisionInvalid => {
            NATIVE_CHECKPOINT_SCORER_DECISION_CODE_V1
        }
        NativeCheckpointInferenceErrorKindV1::ScoringInvalid => {
            NATIVE_CHECKPOINT_SCORER_MODEL_CODE_V1
        }
        NativeCheckpointInferenceErrorKindV1::AuthorityBinding
        | NativeCheckpointInferenceErrorKindV1::PayloadExactLength
        | NativeCheckpointInferenceErrorKindV1::PayloadDigestMismatch
        | NativeCheckpointInferenceErrorKindV1::PayloadInvalid
        | NativeCheckpointInferenceErrorKindV1::ModelInvalid => {
            NATIVE_CHECKPOINT_SCORER_MODEL_CODE_V1
        }
    }
}

/// Loads a fresh immutable model from already-validated run and checkpoint
/// authorities plus the exact payload bytes named by that checkpoint.
///
/// Construction repeats the inference-critical authority bindings, verifies
/// every raw and semantic payload digest, validates the complete train state,
/// loads parameters transactionally into a new native model, and re-attests
/// the model's named-parameter digest before anything is returned.
pub fn load_native_checkpoint_inference_v1(
    run: &ValidatedTrainRunV2,
    checkpoint: &CheckpointManifestV3,
    payload: &[u8],
) -> Result<NativeCheckpointInferenceV1> {
    if payload.len() != NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1 {
        return Err(NativeCheckpointInferenceErrorV1::new(
            NativeCheckpointInferenceErrorKindV1::PayloadExactLength,
        ));
    }
    let run_sha256 = validate_authority_bindings_v1(run, checkpoint)?;
    let expected = expected_payload_digests_v1(checkpoint)?;
    let anchor =
        u32::try_from(checkpoint.train_state().scorer_bias_anchor_f32_bits()).map_err(|_| {
            NativeCheckpointInferenceErrorV1::new(
                NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
            )
        })?;
    let decoded = decode_native_train_state_payload_verified_v1(
        payload,
        checkpoint.train_state().adam_step(),
        anchor,
        &expected,
    )
    .map_err(map_payload_error_v1)?;
    if decoded.snapshot.adam_step != checkpoint.generation_index()
        || decoded.snapshot.scorer_bias_anchor_bits != anchor
        || decoded.digests.payload_sha256 != checkpoint.checkpoint_payload_sha256()
        || decoded.digests.model_parameter_sha256 != checkpoint.model_parameter_sha256()
        || decoded.digests.native_state_sha256 != checkpoint.train_state_sha256()
    {
        return Err(NativeCheckpointInferenceErrorV1::new(
            NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
        ));
    }

    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .map_err(map_model_error_v1)?;
    model
        .replace_parameter_snapshot_v1(&decoded.snapshot.parameters)
        .map_err(map_model_error_v1)?;
    let model_parameter_sha256 = checkpoint.model_parameter_sha256();
    if model.parameter_count_v1() != PARAMETER_COUNT_V1
        || model.parameter_manifest_sha256_v1() != lower_hex_raw32_v1(model_parameter_sha256)
    {
        return Err(NativeCheckpointInferenceErrorV1::new(
            NativeCheckpointInferenceErrorKindV1::ModelInvalid,
        ));
    }

    Ok(NativeCheckpointInferenceV1 {
        model,
        run_sha256,
        checkpoint_manifest_sha256: checkpoint.checkpoint_manifest_sha256(),
        checkpoint_payload_sha256: checkpoint.checkpoint_payload_sha256(),
        train_state_sha256: checkpoint.train_state_sha256(),
        model_parameter_sha256,
        generation_index: checkpoint.generation_index(),
    })
}

fn validate_authority_bindings_v1(
    run: &ValidatedTrainRunV2,
    checkpoint: &CheckpointManifestV3,
) -> Result<[u8; 32]> {
    let record = run.record();
    let model_contract = &record.contracts.model;
    let tensorizer_contract = &record.contracts.tensorizer;
    let snapshot = &record.model_snapshot;
    let state = checkpoint.train_state();
    let progress = checkpoint.progress();
    let segment_updates = checkpoint.checkpoint_segment_updates();
    let generation = checkpoint.generation_index();
    let expected_segment = generation.checked_div(segment_updates).ok_or_else(|| {
        NativeCheckpointInferenceErrorV1::new(
            NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
        )
    })?;
    let run_sha256 = parse_lower_hex_raw32_v1(run.run_sha256()).map_err(|_| {
        NativeCheckpointInferenceErrorV1::new(
            NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
        )
    })?;

    if sha256_v1(run.canonical_bytes()) != run_sha256
        || sha256_v1(checkpoint.canonical_bytes()) != checkpoint.checkpoint_manifest_sha256()
        || checkpoint.run_sha256() != run.run_sha256()
        || checkpoint.identity_bundle_sha256() != run.identity_bundle_sha256()
        || record.contracts.identity_bundle_sha256 != run.identity_bundle_sha256()
        || checkpoint.batch_episodes() != run.batch_episodes()
        || segment_updates == 0
        || segment_updates != run.checkpoint_segment_updates()
        || !generation.is_multiple_of(segment_updates)
        || checkpoint.segment_ordinal() != expected_segment
        || generation > run.requested_successful_updates()
        || state.adam_step() != generation
        || progress.successful_update_count() != generation
        || progress.batch_episodes() != checkpoint.batch_episodes()
        || progress.checkpoint_segment_updates() != segment_updates
        || state.scorer_bias_anchor_f32_bits() != snapshot.scorer_bias_anchor_f32_bits
        || state.parameter_layout_sha256 != model_contract.parameter_layout_sha256
        || state.parameter_layout_sha256 != snapshot.parameter_layout_sha256
        || state.parameter_tensor_count != model_contract.parameter_tensor_count
        || state.parameter_tensor_count != snapshot.parameter_tensor_count
        || state.parameter_element_count != model_contract.parameter_element_count
        || state.parameter_element_count != snapshot.parameter_element_count
        || model_contract.architecture_identity != MODEL_ARCHITECTURE_VERSION_V1
        || model_contract.config_fingerprint != MODEL_CONFIG_FINGERPRINT_V1
        || usize::try_from(model_contract.parameter_element_count).ok() != Some(PARAMETER_COUNT_V1)
        || tensorizer_contract.feature_contract_digest != FEATURE_CONTRACT_DIGEST_V1
        || tensorizer_contract.feature_encoding_digest != FEATURE_ENCODING_DIGEST_V1
        || checkpoint.checkpoint_payload_sha256()
            != parse_lower_hex_raw32_v1(&checkpoint.payload().sha256).map_err(|_| {
                NativeCheckpointInferenceErrorV1::new(
                    NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
                )
            })?
        || checkpoint.model_parameter_sha256()
            != parse_lower_hex_raw32_v1(state.model_parameter_sha256()).map_err(|_| {
                NativeCheckpointInferenceErrorV1::new(
                    NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
                )
            })?
        || checkpoint.train_state_sha256()
            != parse_lower_hex_raw32_v1(state.state_sha256()).map_err(|_| {
                NativeCheckpointInferenceErrorV1::new(
                    NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
                )
            })?
    {
        return Err(NativeCheckpointInferenceErrorV1::new(
            NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
        ));
    }
    validate_payload_layout_v1(checkpoint)?;
    // Design directive slice 3 (closing slice 2's STOP finding, the third
    // independent copy of the generation-0-implies-common-snapshot
    // invariant, this time in the checkpoint-INFERENCE layer that gates
    // evaluation/the checkpoint runner/ladder-opponent resolution): a
    // continual-init record (`record.contracts.opponent_ladder_initialization`
    // present) pins generation 0 against the record's OWN self-contained
    // `derived_model_parameter_sha256` field instead of the common
    // snapshot's `named_parameter_stream_sha256`, mirroring
    // `decode_genesis_checkpoint_manifest_v2_v3_self_contained`
    // (`native_training_store_checkpoint_v3`) exactly. A record with no
    // init section reproduces today's check byte-for-byte.
    let expected_genesis_model_parameter_sha256 =
        match &record.contracts.opponent_ladder_initialization {
            Some(init) => &init.derived_model_parameter_sha256,
            None => &snapshot.named_parameter_stream_sha256,
        };
    if generation == 0
        && checkpoint.model_parameter_sha256()
            != parse_lower_hex_raw32_v1(expected_genesis_model_parameter_sha256).map_err(|_| {
                NativeCheckpointInferenceErrorV1::new(
                    NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
                )
            })?
    {
        return Err(NativeCheckpointInferenceErrorV1::new(
            NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
        ));
    }
    Ok(run_sha256)
}

fn validate_payload_layout_v1(checkpoint: &CheckpointManifestV3) -> Result<()> {
    let payload = checkpoint.payload();
    if payload.schema != NATIVE_TRAIN_STATE_PAYLOAD_SCHEMA_V1
        || payload.encoding != NATIVE_TRAIN_STATE_PAYLOAD_ENCODING_V1
        || usize::try_from(payload.byte_count).ok()
            != Some(NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1)
    {
        return Err(NativeCheckpointInferenceErrorV1::new(
            NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
        ));
    }
    for (declared, expected) in payload
        .sections
        .iter()
        .zip(NATIVE_TRAIN_STATE_PAYLOAD_SECTIONS_V1)
    {
        if declared.name != expected.name
            || usize::try_from(declared.offset_bytes).ok() != Some(expected.offset_bytes)
            || usize::try_from(declared.byte_count).ok() != Some(expected.byte_count)
        {
            return Err(NativeCheckpointInferenceErrorV1::new(
                NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
            ));
        }
    }
    Ok(())
}

fn expected_payload_digests_v1(
    checkpoint: &CheckpointManifestV3,
) -> Result<NativeTrainStatePayloadDigestsV1> {
    let sections = &checkpoint.payload().sections;
    Ok(NativeTrainStatePayloadDigestsV1 {
        payload_sha256: checkpoint.checkpoint_payload_sha256(),
        parameters_sha256: parse_authority_digest_v1(&sections[0].sha256)?,
        first_moments_sha256: parse_authority_digest_v1(&sections[1].sha256)?,
        second_moments_sha256: parse_authority_digest_v1(&sections[2].sha256)?,
        model_parameter_sha256: checkpoint.model_parameter_sha256(),
        native_state_sha256: checkpoint.train_state_sha256(),
    })
}

fn parse_authority_digest_v1(value: &str) -> Result<[u8; 32]> {
    parse_lower_hex_raw32_v1(value).map_err(|_| {
        NativeCheckpointInferenceErrorV1::new(
            NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
        )
    })
}

/// Visibility bump only (was module-private): `model-guided-searcher-v1`
/// item 6a (`CLAUDE-MODEL-GUIDED-SEARCHER-DESIGN-V1.md` Section 1.4's
/// "third dispatch shape", Section 5.3 item 6) reuses this exact,
/// unmodified conversion from its own leaf evaluator
/// (`model_guided_search_core_v1::ModelGuidedSearchRealForwardValueEvaluatorV1`)
/// to build a `NativeEncodedDecisionViewV1` from a `NativeFlatDecisionTensorV2`
/// it tensorizes itself, without going through the full checkpoint-manifest
/// loading this module's own `NativeCheckpointInferenceV1` requires (the
/// search seam constructs its model in-memory via
/// `NativePolicyValueNetV1::runner_fixed_v1`, matching this crate's own
/// item-5 precedent). Zero behavior change: the function body is byte-for-
/// byte unchanged, and every existing caller and test in this module is
/// unaffected.
pub(crate) fn encoded_decision_view_v1(
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

fn map_payload_error_v1(error: NativeTrainStatePayloadErrorV1) -> NativeCheckpointInferenceErrorV1 {
    let kind = match error {
        NativeTrainStatePayloadErrorV1::ExactLength { .. } => {
            NativeCheckpointInferenceErrorKindV1::PayloadExactLength
        }
        NativeTrainStatePayloadErrorV1::DigestMismatch(
            NativeTrainStatePayloadDigestFieldV1::Payload
            | NativeTrainStatePayloadDigestFieldV1::Parameters
            | NativeTrainStatePayloadDigestFieldV1::FirstMoments
            | NativeTrainStatePayloadDigestFieldV1::SecondMoments
            | NativeTrainStatePayloadDigestFieldV1::ModelParameters
            | NativeTrainStatePayloadDigestFieldV1::NativeState,
        ) => NativeCheckpointInferenceErrorKindV1::PayloadDigestMismatch,
        NativeTrainStatePayloadErrorV1::LayoutInvariant(_)
        | NativeTrainStatePayloadErrorV1::TrainState(_) => {
            NativeCheckpointInferenceErrorKindV1::PayloadInvalid
        }
    };
    NativeCheckpointInferenceErrorV1::new(kind)
}

fn map_model_error_v1(_error: NativePolicyValueErrorV1) -> NativeCheckpointInferenceErrorV1 {
    NativeCheckpointInferenceErrorV1::new(NativeCheckpointInferenceErrorKindV1::ModelInvalid)
}

fn map_tensor_error_v1(_error: NativeFlatTensorErrorV2) -> NativeCheckpointInferenceErrorV1 {
    NativeCheckpointInferenceErrorV1::new(NativeCheckpointInferenceErrorKindV1::DecisionInvalid)
}

fn map_scoring_error_v1(error: NativePolicyValueErrorV1) -> NativeCheckpointInferenceErrorV1 {
    let kind = match error {
        NativePolicyValueErrorV1::NonFiniteOutput { .. }
        | NativePolicyValueErrorV1::ParameterInvariant(_) => {
            NativeCheckpointInferenceErrorKindV1::ScoringInvalid
        }
        _ => NativeCheckpointInferenceErrorKindV1::DecisionInvalid,
    };
    NativeCheckpointInferenceErrorV1::new(kind)
}

// =============================================================================
// Capacity-experiment wide-net (kernel-policy-value-net-8w128) inference
// sibling (CAPACITY-EXPERIMENT-CONTRACT-DRAFT.md Section 3). EVALUATION ONLY:
// reuses the already-landed, golden-tested wide forward
// (`NativePolicyValueNetWideV1::forward_wide_v1`) and the wide payload codec;
// adds no training/backward capability. Purely additive: none of the frozen
// types or functions above this marker are edited.
// =============================================================================

/// Wide-net sibling of [`NativeCheckpointInferenceV1`].
pub struct NativeCheckpointInferenceWideV1 {
    model: NativePolicyValueNetWideV1,
    run_sha256: [u8; 32],
    checkpoint_manifest_sha256: [u8; 32],
    checkpoint_payload_sha256: [u8; 32],
    train_state_sha256: [u8; 32],
    model_parameter_sha256: [u8; 32],
    generation_index: u64,
}

impl Debug for NativeCheckpointInferenceWideV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeCheckpointInferenceWideV1")
            .field("run_sha256", &lower_hex_raw32_v1(self.run_sha256))
            .field(
                "checkpoint_manifest_sha256",
                &lower_hex_raw32_v1(self.checkpoint_manifest_sha256),
            )
            .field(
                "checkpoint_payload_sha256",
                &lower_hex_raw32_v1(self.checkpoint_payload_sha256),
            )
            .field(
                "train_state_sha256",
                &lower_hex_raw32_v1(self.train_state_sha256),
            )
            .field(
                "model_parameter_sha256",
                &lower_hex_raw32_v1(self.model_parameter_sha256),
            )
            .field("generation_index", &self.generation_index)
            .finish_non_exhaustive()
    }
}

impl NativeCheckpointInferenceWideV1 {
    pub const fn run_sha256(&self) -> [u8; 32] {
        self.run_sha256
    }

    pub const fn checkpoint_manifest_sha256(&self) -> [u8; 32] {
        self.checkpoint_manifest_sha256
    }

    pub const fn checkpoint_payload_sha256(&self) -> [u8; 32] {
        self.checkpoint_payload_sha256
    }

    pub const fn train_state_sha256(&self) -> [u8; 32] {
        self.train_state_sha256
    }

    pub const fn model_parameter_sha256(&self) -> [u8; 32] {
        self.model_parameter_sha256
    }

    pub const fn generation_index(&self) -> u64 {
        self.generation_index
    }

    /// Wide-net sibling of [`NativeCheckpointInferenceV1::score_decision_v1`].
    pub fn score_decision_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
    ) -> Result<NativeCheckpointInferenceOutputV1> {
        let mut tensorizer = NativeFlatTensorizerV2::new();
        let mut tensor = NativeFlatDecisionTensorV2::default();
        self.score_decision_with_scratch_wide_v1(decision, &mut tensorizer, &mut tensor)
    }

    /// Wide-net sibling of [`NativeCheckpointInferenceV1::batch_scorer_v1`].
    pub fn batch_scorer_v1(&self) -> NativeCheckpointBatchScorerWideV1<'_> {
        NativeCheckpointBatchScorerWideV1::new_v1(self)
    }

    fn score_decision_with_scratch_wide_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
        tensorizer: &mut NativeFlatTensorizerV2,
        tensor: &mut NativeFlatDecisionTensorV2,
    ) -> Result<NativeCheckpointInferenceOutputV1> {
        tensorizer
            .fill(decision, tensor)
            .map_err(map_tensor_error_v1)?;
        let output = self
            .model
            .forward_wide_v1(encoded_decision_view_v1(tensor))
            .map_err(map_scoring_error_v1)?;
        if output.logits.len() != decision.actions().len()
            || output.logits.is_empty()
            || output.logits.iter().any(|value| !value.is_finite())
            || !output.value.is_finite()
        {
            return Err(NativeCheckpointInferenceErrorV1::new(
                NativeCheckpointInferenceErrorKindV1::ScoringInvalid,
            ));
        }
        Ok(NativeCheckpointInferenceOutputV1 {
            action_logits: output.logits,
            value: output.value,
        })
    }
}

/// Wide-net sibling of [`NativeCheckpointBatchScorerV1`].
pub struct NativeCheckpointBatchScorerWideV1<'a> {
    inference: &'a NativeCheckpointInferenceWideV1,
    tensorizer: NativeFlatTensorizerV2,
    tensor: NativeFlatDecisionTensorV2,
    candidate_logits: Vec<f32>,
    candidate_values: Vec<f32>,
    first_failure_code: Option<u32>,
}

impl Debug for NativeCheckpointBatchScorerWideV1<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeCheckpointBatchScorerWideV1")
            .field("generation_index", &self.inference.generation_index)
            .field("first_failure_code", &self.first_failure_code)
            .finish_non_exhaustive()
    }
}

impl<'a> NativeCheckpointBatchScorerWideV1<'a> {
    fn new_v1(inference: &'a NativeCheckpointInferenceWideV1) -> Self {
        Self {
            inference,
            tensorizer: NativeFlatTensorizerV2::new(),
            tensor: NativeFlatDecisionTensorV2::default(),
            candidate_logits: Vec::new(),
            candidate_values: Vec::new(),
            first_failure_code: None,
        }
    }

    pub const fn first_failure_code(&self) -> Option<u32> {
        self.first_failure_code
    }

    fn score_batch_checked_v1(
        &mut self,
        batch: &FlatScoringBatchViewV2<'_>,
        action_logits: &mut [f32],
        values: &mut [f32],
    ) -> std::result::Result<(), u32> {
        let contract = batch.contract();
        if contract != expected_scorer_contract(contract.card_db_hash) {
            return Err(NATIVE_CHECKPOINT_SCORER_CONTRACT_CODE_V1);
        }

        let decision_count = batch.decision_count();
        let action_offsets = batch.action_offsets();
        if decision_count == 0
            || values.len() != decision_count
            || action_logits.is_empty()
            || action_logits.len() != batch.total_action_count()
            || action_offsets.len() != decision_count + 1
            || action_offsets.first().copied() != Some(0)
            || action_offsets.last().copied() != Some(action_logits.len())
        {
            return Err(NATIVE_CHECKPOINT_SCORER_OUTPUT_SHAPE_CODE_V1);
        }

        self.candidate_logits.clear();
        self.candidate_values.clear();
        self.candidate_logits
            .try_reserve_exact(action_logits.len())
            .map_err(|_| NATIVE_CHECKPOINT_SCORER_OUTPUT_SHAPE_CODE_V1)?;
        self.candidate_values
            .try_reserve_exact(values.len())
            .map_err(|_| NATIVE_CHECKPOINT_SCORER_OUTPUT_SHAPE_CODE_V1)?;

        for decision_index in 0..decision_count {
            let decision = batch
                .decision(decision_index)
                .ok_or(NATIVE_CHECKPOINT_SCORER_MISSING_DECISION_CODE_V1)?;
            let begin = action_offsets[decision_index];
            let end = action_offsets[decision_index + 1];
            if end <= begin || end > action_logits.len() || end - begin != decision.actions().len()
            {
                return Err(NATIVE_CHECKPOINT_SCORER_OUTPUT_SHAPE_CODE_V1);
            }
            let output = self
                .inference
                .score_decision_with_scratch_wide_v1(
                    decision,
                    &mut self.tensorizer,
                    &mut self.tensor,
                )
                .map_err(batch_scorer_code_v1)?;
            if output.action_logits.len() != end - begin
                || output.action_logits.iter().any(|value| !value.is_finite())
                || !output.value.is_finite()
            {
                return Err(NATIVE_CHECKPOINT_SCORER_OUTPUT_SHAPE_CODE_V1);
            }
            self.candidate_logits
                .extend_from_slice(&output.action_logits);
            self.candidate_values.push(output.value);
        }

        if self.candidate_logits.len() != action_logits.len()
            || self.candidate_values.len() != values.len()
        {
            return Err(NATIVE_CHECKPOINT_SCORER_OUTPUT_SHAPE_CODE_V1);
        }
        action_logits.copy_from_slice(&self.candidate_logits);
        values.copy_from_slice(&self.candidate_values);
        Ok(())
    }
}

impl FlatBatchScorerV2 for NativeCheckpointBatchScorerWideV1<'_> {
    fn score_batch_v2(
        &mut self,
        batch: &FlatScoringBatchViewV2<'_>,
        action_logits: &mut [f32],
        values: &mut [f32],
    ) -> std::result::Result<(), FlatBatchScorerErrorV2> {
        if let Some(code) = self.first_failure_code {
            return Err(FlatBatchScorerErrorV2::new(code));
        }
        match self.score_batch_checked_v1(batch, action_logits, values) {
            Ok(()) => Ok(()),
            Err(code) => {
                self.first_failure_code = Some(code);
                Err(FlatBatchScorerErrorV2::new(code))
            }
        }
    }
}

/// Wide-net sibling of [`load_native_checkpoint_inference_v1`]. EVALUATION
/// ONLY; see the module-level marker above.
pub fn load_native_checkpoint_inference_wide_v1(
    run: &ValidatedTrainRunV2,
    checkpoint: &CheckpointManifestV3,
    payload: &[u8],
) -> Result<NativeCheckpointInferenceWideV1> {
    if payload.len() != W_NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1 {
        return Err(NativeCheckpointInferenceErrorV1::new(
            NativeCheckpointInferenceErrorKindV1::PayloadExactLength,
        ));
    }
    let run_sha256 = validate_authority_bindings_wide_v1(run, checkpoint)?;
    let expected = expected_payload_digests_v1(checkpoint)?;
    let anchor =
        u32::try_from(checkpoint.train_state().scorer_bias_anchor_f32_bits()).map_err(|_| {
            NativeCheckpointInferenceErrorV1::new(
                NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
            )
        })?;
    let decoded = decode_native_train_state_payload_verified_wide_v1(
        payload,
        checkpoint.train_state().adam_step(),
        anchor,
        &expected,
    )
    .map_err(map_payload_error_v1)?;
    if decoded.snapshot.adam_step != checkpoint.generation_index()
        || decoded.snapshot.scorer_bias_anchor_bits != anchor
        || decoded.digests.payload_sha256 != checkpoint.checkpoint_payload_sha256()
        || decoded.digests.model_parameter_sha256 != checkpoint.model_parameter_sha256()
        || decoded.digests.native_state_sha256 != checkpoint.train_state_sha256()
    {
        return Err(NativeCheckpointInferenceErrorV1::new(
            NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
        ));
    }

    let mut model = NativePolicyValueNetWideV1::runner_fixed_wide_v1(
        NativePolicyValueModelConfigV1::contract_wide_v1(),
    )
    .map_err(map_model_error_v1)?;
    model
        .replace_parameter_snapshot_wide_v1(&decoded.snapshot.parameters)
        .map_err(map_model_error_v1)?;
    let model_parameter_sha256 = checkpoint.model_parameter_sha256();
    if model.parameter_count_wide_v1() != W_PARAMETER_COUNT_V1
        || model.parameter_manifest_sha256_wide_v1() != lower_hex_raw32_v1(model_parameter_sha256)
    {
        return Err(NativeCheckpointInferenceErrorV1::new(
            NativeCheckpointInferenceErrorKindV1::ModelInvalid,
        ));
    }

    Ok(NativeCheckpointInferenceWideV1 {
        model,
        run_sha256,
        checkpoint_manifest_sha256: checkpoint.checkpoint_manifest_sha256(),
        checkpoint_payload_sha256: checkpoint.checkpoint_payload_sha256(),
        train_state_sha256: checkpoint.train_state_sha256(),
        model_parameter_sha256,
        generation_index: checkpoint.generation_index(),
    })
}

/// Wide-net sibling of [`validate_authority_bindings_v1`]. Fails closed if
/// `run`'s record does not carry `contracts.wide_model_experiment_v1`: this
/// path is never a silent substitute for the frozen loader.
fn validate_authority_bindings_wide_v1(
    run: &ValidatedTrainRunV2,
    checkpoint: &CheckpointManifestV3,
) -> Result<[u8; 32]> {
    let record = run.record();
    if record.contracts.wide_model_experiment_v1.is_none() {
        return Err(NativeCheckpointInferenceErrorV1::new(
            NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
        ));
    }
    let model_contract = &record.contracts.model;
    let tensorizer_contract = &record.contracts.tensorizer;
    let snapshot = &record.model_snapshot;
    let state = checkpoint.train_state();
    let progress = checkpoint.progress();
    let segment_updates = checkpoint.checkpoint_segment_updates();
    let generation = checkpoint.generation_index();
    let expected_segment = generation.checked_div(segment_updates).ok_or_else(|| {
        NativeCheckpointInferenceErrorV1::new(
            NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
        )
    })?;
    let run_sha256 = parse_lower_hex_raw32_v1(run.run_sha256()).map_err(|_| {
        NativeCheckpointInferenceErrorV1::new(
            NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
        )
    })?;

    if sha256_v1(run.canonical_bytes()) != run_sha256
        || sha256_v1(checkpoint.canonical_bytes()) != checkpoint.checkpoint_manifest_sha256()
        || checkpoint.run_sha256() != run.run_sha256()
        || checkpoint.identity_bundle_sha256() != run.identity_bundle_sha256()
        || record.contracts.identity_bundle_sha256 != run.identity_bundle_sha256()
        || checkpoint.batch_episodes() != run.batch_episodes()
        || segment_updates == 0
        || segment_updates != run.checkpoint_segment_updates()
        || !generation.is_multiple_of(segment_updates)
        || checkpoint.segment_ordinal() != expected_segment
        || generation > run.requested_successful_updates()
        || state.adam_step() != generation
        || progress.successful_update_count() != generation
        || progress.batch_episodes() != checkpoint.batch_episodes()
        || progress.checkpoint_segment_updates() != segment_updates
        || state.scorer_bias_anchor_f32_bits() != snapshot.scorer_bias_anchor_f32_bits
        || state.parameter_layout_sha256 != model_contract.parameter_layout_sha256
        || state.parameter_layout_sha256 != snapshot.parameter_layout_sha256
        || state.parameter_tensor_count != model_contract.parameter_tensor_count
        || state.parameter_tensor_count != snapshot.parameter_tensor_count
        || state.parameter_element_count != model_contract.parameter_element_count
        || state.parameter_element_count != snapshot.parameter_element_count
        || model_contract.architecture_identity != W_MODEL_ARCHITECTURE_VERSION_V1
        || model_contract.config_fingerprint != W_MODEL_CONFIG_FINGERPRINT_V1
        || usize::try_from(model_contract.parameter_element_count).ok()
            != Some(W_PARAMETER_COUNT_V1)
        || tensorizer_contract.feature_contract_digest != FEATURE_CONTRACT_DIGEST_V1
        || tensorizer_contract.feature_encoding_digest != FEATURE_ENCODING_DIGEST_V1
        || checkpoint.checkpoint_payload_sha256()
            != parse_lower_hex_raw32_v1(&checkpoint.payload().sha256).map_err(|_| {
                NativeCheckpointInferenceErrorV1::new(
                    NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
                )
            })?
        || checkpoint.model_parameter_sha256()
            != parse_lower_hex_raw32_v1(state.model_parameter_sha256()).map_err(|_| {
                NativeCheckpointInferenceErrorV1::new(
                    NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
                )
            })?
        || checkpoint.train_state_sha256()
            != parse_lower_hex_raw32_v1(state.state_sha256()).map_err(|_| {
                NativeCheckpointInferenceErrorV1::new(
                    NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
                )
            })?
    {
        return Err(NativeCheckpointInferenceErrorV1::new(
            NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
        ));
    }
    validate_payload_layout_wide_v1(checkpoint)?;
    // The wide protocol trains fresh-init only (contract Section 4): a
    // continual-init ladder section is not expected for this experiment, but
    // if one were ever present the binding is identical in spirit to the
    // frozen path's -- pin against the record's own derived digest when
    // present, the common wide snapshot's named-parameter stream otherwise.
    let expected_genesis_model_parameter_sha256 =
        match &record.contracts.opponent_ladder_initialization {
            Some(init) => &init.derived_model_parameter_sha256,
            None => &snapshot.named_parameter_stream_sha256,
        };
    if generation == 0
        && checkpoint.model_parameter_sha256()
            != parse_lower_hex_raw32_v1(expected_genesis_model_parameter_sha256).map_err(|_| {
                NativeCheckpointInferenceErrorV1::new(
                    NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
                )
            })?
    {
        return Err(NativeCheckpointInferenceErrorV1::new(
            NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
        ));
    }
    Ok(run_sha256)
}

/// Wide-net sibling of [`validate_payload_layout_v1`].
fn validate_payload_layout_wide_v1(checkpoint: &CheckpointManifestV3) -> Result<()> {
    let payload = checkpoint.payload();
    if payload.schema != NATIVE_TRAIN_STATE_PAYLOAD_SCHEMA_V1
        || payload.encoding != NATIVE_TRAIN_STATE_PAYLOAD_ENCODING_V1
        || usize::try_from(payload.byte_count).ok()
            != Some(W_NATIVE_TRAIN_STATE_PAYLOAD_BYTE_COUNT_V1)
    {
        return Err(NativeCheckpointInferenceErrorV1::new(
            NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
        ));
    }
    for (declared, expected) in payload
        .sections
        .iter()
        .zip(W_NATIVE_TRAIN_STATE_PAYLOAD_SECTIONS_V1)
    {
        if declared.name != expected.name
            || usize::try_from(declared.offset_bytes).ok() != Some(expected.offset_bytes)
            || usize::try_from(declared.byte_count).ok() != Some(expected.byte_count)
        {
            return Err(NativeCheckpointInferenceErrorV1::new(
                NativeCheckpointInferenceErrorKindV1::AuthorityBinding,
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod checkpoint_reliance_probe_v1;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_flat_scored_rollout_v2::run_async_flat_scored_rollout_v2;
    use crate::async_rollout_v2::AsyncRolloutConfigV2;
    use crate::common_model_snapshot_v1::common_model_snapshot_paths_v1;
    use crate::flat_policy_v2::{
        FlatGlobalsV2, FlatRelativePlayerV2, FlatScorerActionCoreV2, FlatScoringDecisionViewV2,
    };
    use crate::native_policy_train_step_v1::native_train_state_parameter_layout_v1;
    use crate::native_training_executor_v1::{
        NativeTrainingExecutionConfigV1, NativeTrainingExecutorV1, NativeTrainingNumericalBackendV1,
    };
    use crate::native_training_store_checkpoint_v3::{
        build_genesis_checkpoint_manifest_v3, build_trained_checkpoint_manifest_v3,
        decode_genesis_checkpoint_manifest_v3, decode_trained_checkpoint_manifest_v3,
    };
    use crate::native_training_store_run_v2::{decode_train_run_v2, test_fixture_bytes_v2};
    use crate::native_training_store_update_group_v1::{
        begin_update_evidence_chain_v1, build_update_group_v1,
    };
    use crate::rl::PlayerSeatV1;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::OnceLock;
    use std::time::Duration;

    struct FixtureV1 {
        run_bytes: Vec<u8>,
        payload: Vec<u8>,
        manifest: Vec<u8>,
    }

    static FIXTURE_V1: OnceLock<FixtureV1> = OnceLock::new();
    static TEMP_ORDINAL_V1: AtomicU64 = AtomicU64::new(0);

    fn execution_config_v1(run: &ValidatedTrainRunV2) -> NativeTrainingExecutionConfigV1 {
        NativeTrainingExecutionConfigV1 {
            run_base_seed: run.record().schedule.base_seed,
            batch_episodes: run.batch_episodes(),
            deck_ids: ["Rally".to_owned(), "Rally".to_owned()],
            max_physical_decisions: run.record().limits.max_physical_decisions,
            max_policy_steps: run.record().limits.max_policy_steps,
            worker_count: usize::try_from(run.record().topology.worker_count).unwrap(),
            sessions_per_worker: usize::try_from(run.record().topology.sessions_per_worker)
                .unwrap(),
            broker_batch_target: usize::try_from(run.record().topology.broker_batch_target)
                .unwrap(),
            scheduler_timeout: Duration::from_secs(30),
            measure_broker_service_time: false,
            value_coefficient_bits: 0.5_f32.to_bits(),
            learning_rate_bits: 0.001_f32.to_bits(),
            numerical_backend: NativeTrainingNumericalBackendV1::Sequential,
            backward_worker_limit: 1,
            searcher_active_v1: false,
        }
    }

    fn fresh_executor_v1(run: &ValidatedTrainRunV2) -> NativeTrainingExecutorV1 {
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config_v1(run),
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap()
    }

    fn fixture_v1() -> &'static FixtureV1 {
        FIXTURE_V1.get_or_init(|| {
            let run_bytes = test_fixture_bytes_v2();
            let run = decode_train_run_v2(&run_bytes).unwrap();
            let executor = fresh_executor_v1(&run);
            let checkpoint = executor.checkpoint_candidate_v1().unwrap();
            let payload = checkpoint.payload().to_vec();
            let authority = build_genesis_checkpoint_manifest_v3(&run, &payload).unwrap();
            FixtureV1 {
                run_bytes,
                payload,
                manifest: authority.canonical_bytes().to_vec(),
            }
        })
    }

    fn authorities_v1() -> (ValidatedTrainRunV2, CheckpointManifestV3) {
        let fixture = fixture_v1();
        let run = decode_train_run_v2(&fixture.run_bytes).unwrap();
        let checkpoint =
            decode_genesis_checkpoint_manifest_v3(&fixture.manifest, &fixture.payload, &run)
                .unwrap();
        (run, checkpoint)
    }

    fn decision_parts_v1() -> (FlatGlobalsV2, [FlatScorerActionCoreV2; 1]) {
        (
            FlatGlobalsV2 {
                acting_player: FlatRelativePlayerV2::SelfPlayer,
                ..FlatGlobalsV2::default()
            },
            [FlatScorerActionCoreV2::default()],
        )
    }

    fn decision_view_v1<'a>(
        globals: &'a FlatGlobalsV2,
        actions: &'a [FlatScorerActionCoreV2],
    ) -> FlatScoringDecisionViewV2<'a> {
        FlatScoringDecisionViewV2::new(
            globals,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            actions,
            &[],
        )
    }

    fn direct_checkpoint_model_v1(
        checkpoint: &CheckpointManifestV3,
        payload: &[u8],
    ) -> NativePolicyValueNetV1 {
        let expected = expected_payload_digests_v1(checkpoint).unwrap();
        let decoded = decode_native_train_state_payload_verified_v1(
            payload,
            checkpoint.train_state().adam_step(),
            u32::try_from(checkpoint.train_state().scorer_bias_anchor_f32_bits()).unwrap(),
            &expected,
        )
        .unwrap();
        let mut model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        model
            .replace_parameter_snapshot_v1(&decoded.snapshot.parameters)
            .unwrap();
        model
    }

    fn tensor_offset_v1(name: &str) -> usize {
        let mut offset = 0usize;
        for (candidate, shape) in native_train_state_parameter_layout_v1() {
            if candidate == name {
                return offset;
            }
            offset += shape.iter().product::<usize>() * 4;
        }
        panic!("unknown parameter tensor")
    }

    fn assert_payload_rejected_v1(payload: &[u8]) {
        let (run, checkpoint) = authorities_v1();
        assert_eq!(
            load_native_checkpoint_inference_v1(&run, &checkpoint, payload)
                .unwrap_err()
                .kind(),
            NativeCheckpointInferenceErrorKindV1::PayloadDigestMismatch
        );
    }

    fn checkpoint_rollout_config_v1(run: &ValidatedTrainRunV2) -> AsyncRolloutConfigV2 {
        AsyncRolloutConfigV2 {
            deck_ids: run.record().environment().deck_ids().clone(),
            learner_seat: PlayerSeatV1::P0,
            environment_seed: 71_901,
            opponent_policy_seed: 72_901,
            learner_policy_seed: 73_901,
            max_physical_decisions: run.record().limits().max_physical_decisions(),
            max_policy_steps: run.record().limits().max_policy_steps(),
            worker_count: 1,
            sessions_per_worker: 1,
            broker_batch_target: 1,
            first_episode_id: 0,
            episode_count: 1,
            scheduler_timeout: Duration::from_secs(60),
            measure_broker_service_time: false,
            starting_player: None,
        }
    }

    struct ComparingCheckpointScorerV1<'a> {
        inference: &'a NativeCheckpointInferenceV1,
        scorer: NativeCheckpointBatchScorerV1<'a>,
        call_count: u64,
        checked_transactional_failure: bool,
    }

    impl<'a> ComparingCheckpointScorerV1<'a> {
        fn new_v1(inference: &'a NativeCheckpointInferenceV1) -> Self {
            Self {
                inference,
                scorer: inference.batch_scorer_v1(),
                call_count: 0,
                checked_transactional_failure: false,
            }
        }
    }

    impl FlatBatchScorerV2 for ComparingCheckpointScorerV1<'_> {
        fn score_batch_v2(
            &mut self,
            batch: &FlatScoringBatchViewV2<'_>,
            action_logits: &mut [f32],
            values: &mut [f32],
        ) -> std::result::Result<(), FlatBatchScorerErrorV2> {
            if !self.checked_transactional_failure {
                let mut rejected = self.inference.batch_scorer_v1();
                let mut wrong_logits = vec![123.25_f32; action_logits.len() + 1];
                let mut wrong_values = vec![-456.5_f32; values.len()];
                let before_wrong_logits = wrong_logits.clone();
                let before_wrong_values = wrong_values.clone();
                let error = rejected
                    .score_batch_v2(batch, &mut wrong_logits, &mut wrong_values)
                    .unwrap_err();
                assert_eq!(error.code, NATIVE_CHECKPOINT_SCORER_OUTPUT_SHAPE_CODE_V1);
                assert_eq!(rejected.first_failure_code(), Some(error.code));
                assert_eq!(wrong_logits, before_wrong_logits);
                assert_eq!(wrong_values, before_wrong_values);

                let mut retry_logits = vec![789.75_f32; action_logits.len()];
                let mut retry_values = vec![-987.25_f32; values.len()];
                let before_retry_logits = retry_logits.clone();
                let before_retry_values = retry_values.clone();
                let retry_error = rejected
                    .score_batch_v2(batch, &mut retry_logits, &mut retry_values)
                    .unwrap_err();
                assert_eq!(retry_error, error);
                assert_eq!(retry_logits, before_retry_logits);
                assert_eq!(retry_values, before_retry_values);
                self.checked_transactional_failure = true;
            }

            self.scorer.score_batch_v2(batch, action_logits, values)?;
            for (decision_index, actual_value) in values.iter().enumerate() {
                let decision = batch.decision(decision_index).unwrap();
                let expected = self.inference.score_decision_v1(decision).unwrap();
                let begin = batch.action_offsets()[decision_index];
                let end = batch.action_offsets()[decision_index + 1];
                assert_eq!(
                    action_logits[begin..end]
                        .iter()
                        .map(|value| value.to_bits())
                        .collect::<Vec<_>>(),
                    expected
                        .action_logits()
                        .iter()
                        .map(|value| value.to_bits())
                        .collect::<Vec<_>>()
                );
                assert_eq!(actual_value.to_bits(), expected.value().to_bits());
            }
            self.call_count += 1;
            Ok(())
        }
    }

    #[test]
    fn loaded_model_scores_exactly_like_independent_checkpoint_decode_and_genesis_authority() {
        let fixture = fixture_v1();
        let (run, checkpoint) = authorities_v1();
        let handle =
            load_native_checkpoint_inference_v1(&run, &checkpoint, &fixture.payload).unwrap();
        let direct_model = direct_checkpoint_model_v1(&checkpoint, &fixture.payload);
        let (globals, actions) = decision_parts_v1();
        let actual = handle
            .score_decision_v1(decision_view_v1(&globals, &actions))
            .unwrap();
        let mut tensorizer = NativeFlatTensorizerV2::new();
        let mut tensor = NativeFlatDecisionTensorV2::default();
        tensorizer
            .fill(decision_view_v1(&globals, &actions), &mut tensor)
            .unwrap();
        let expected = direct_model
            .forward_v1(encoded_decision_view_v1(&tensor))
            .unwrap();

        assert_eq!(
            actual
                .action_logits()
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            expected
                .logits
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>()
        );
        assert_eq!(actual.value().to_bits(), expected.value.to_bits());
        assert_eq!(handle.generation_index(), 0);
        assert_eq!(
            handle.run_sha256(),
            parse_lower_hex_raw32_v1(run.run_sha256()).unwrap()
        );
        assert_eq!(
            handle.model_parameter_sha256(),
            parse_lower_hex_raw32_v1(&run.record().model_snapshot.named_parameter_stream_sha256)
                .unwrap()
        );
        assert_eq!(
            handle.model_parameter_sha256(),
            parse_lower_hex_raw32_v1(&direct_model.parameter_manifest_sha256_v1()).unwrap()
        );
        assert_eq!(
            handle.checkpoint_manifest_sha256(),
            checkpoint.checkpoint_manifest_sha256()
        );
        assert_eq!(handle.run_sha256(), sha256_v1(run.canonical_bytes()));
        assert_eq!(
            handle.checkpoint_manifest_sha256(),
            sha256_v1(checkpoint.canonical_bytes())
        );
        assert_eq!(
            handle.checkpoint_payload_sha256(),
            checkpoint.checkpoint_payload_sha256()
        );
        validate_payload_layout_v1(&checkpoint).unwrap();
        assert_eq!(handle.train_state_sha256(), checkpoint.train_state_sha256());
    }

    // ------------------------------------------------------------------
    // Capacity-experiment wide-net inference
    // (CAPACITY-EXPERIMENT-CONTRACT-DRAFT.md Section 3). EVALUATION ONLY.
    // ------------------------------------------------------------------

    fn wide_zero_moment_payload_v1() -> Vec<u8> {
        use crate::native_policy_train_step_v1::NativePolicyValueTrainSnapshotV1;
        use crate::native_policy_value_net_v1::NativeNamedParameterV1;
        use crate::native_train_state_payload_v1::encode_native_train_state_payload_wide_v1;
        let (manifest_path, payload_path) =
            crate::common_model_snapshot_v1::wide_model_snapshot_paths_v1();
        let (model, _record) = crate::common_model_snapshot_v1::build_wide_model_candidate_v1(
            &manifest_path,
            &payload_path,
        )
        .expect("real wide snapshot must load");
        let parameters = model.parameter_snapshot_wide_v1();
        let zero_moments: Vec<_> = parameters
            .iter()
            .map(|parameter| NativeNamedParameterV1 {
                name: parameter.name,
                shape: parameter.shape.clone(),
                values: vec![0.0; parameter.values.len()],
            })
            .collect();
        let scorer_bias_anchor_bits = parameters
            .iter()
            .find(|parameter| parameter.name == "scorer.2.bias")
            .expect("scorer.2.bias tensor present")
            .values[0]
            .to_bits();
        let snapshot = NativePolicyValueTrainSnapshotV1 {
            adam_step: 0,
            scorer_bias_anchor_bits,
            parameters,
            first_moments: zero_moments.clone(),
            second_moments: zero_moments,
        };
        encode_native_train_state_payload_wide_v1(&snapshot)
            .unwrap()
            .bytes
    }

    fn wide_authorities_v1() -> (ValidatedTrainRunV2, CheckpointManifestV3, Vec<u8>) {
        use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_wide_v2;
        let run_bytes = test_fixture_bytes_with_schedule_and_base_seed_wide_v2(
            NativeTrainingNumericalBackendV1::Sequential,
            2,
            4,
            4,
            2,
            4,
            8,
            32_768,
            65_536,
            71501,
        );
        let run = decode_train_run_v2(&run_bytes).unwrap();
        let payload = wide_zero_moment_payload_v1();
        let manifest = build_genesis_checkpoint_manifest_v3(&run, &payload).unwrap();
        let checkpoint =
            decode_genesis_checkpoint_manifest_v3(manifest.canonical_bytes(), &payload, &run)
                .unwrap();
        (run, checkpoint, payload)
    }

    /// Wide-net sibling of
    /// `loaded_model_scores_exactly_like_independent_checkpoint_decode_and_genesis_authority`:
    /// loads a real wide genesis checkpoint through the inference layer and
    /// scores one synthetic decision, plus fail-closed both directions (the
    /// frozen loader rejects the wide-length payload and vice versa).
    #[test]
    fn wide_loaded_model_scores_a_synthetic_decision_and_fails_closed_both_directions() {
        let (run, checkpoint, payload) = wide_authorities_v1();
        let handle = load_native_checkpoint_inference_wide_v1(&run, &checkpoint, &payload)
            .expect("wide checkpoint must load through the inference layer");
        assert_eq!(handle.generation_index(), 0);
        assert_eq!(
            handle.run_sha256(),
            parse_lower_hex_raw32_v1(run.run_sha256()).unwrap()
        );

        let (globals, actions) = decision_parts_v1();
        let output = handle
            .score_decision_v1(decision_view_v1(&globals, &actions))
            .expect("wide model must score a synthetic decision");
        assert_eq!(output.action_logits().len(), 1);
        assert!(output.action_logits()[0].is_finite());
        assert!(output.value().is_finite());

        // Same result through the batch-scorer adapter's own private
        // scratch path (score_decision_with_scratch_wide_v1), proving the
        // adapter and the direct scorer agree.
        let scorer = handle.batch_scorer_v1();
        assert_eq!(scorer.first_failure_code(), None);
        drop(scorer);

        // Fail-closed direction 1: the frozen loader rejects the wide-length
        // payload against a real frozen checkpoint outright.
        let (frozen_run, frozen_checkpoint) = authorities_v1();
        assert_eq!(
            load_native_checkpoint_inference_v1(&frozen_run, &frozen_checkpoint, &payload)
                .unwrap_err()
                .kind(),
            NativeCheckpointInferenceErrorKindV1::PayloadExactLength
        );

        // Fail-closed direction 2: the wide loader rejects a real
        // frozen-length payload against the wide checkpoint outright.
        let frozen_payload = fixture_v1().payload.clone();
        assert_eq!(
            load_native_checkpoint_inference_wide_v1(&run, &checkpoint, &frozen_payload)
                .unwrap_err()
                .kind(),
            NativeCheckpointInferenceErrorKindV1::PayloadExactLength
        );
    }

    #[test]
    fn real_k2_s4_trained_checkpoint_loads_bit_exact_inference_model() {
        let fixture = fixture_v1();
        let run = decode_train_run_v2(&fixture.run_bytes).unwrap();
        assert_eq!(run.batch_episodes(), 2);
        assert_eq!(run.checkpoint_segment_updates(), 4);
        let genesis =
            decode_genesis_checkpoint_manifest_v3(&fixture.manifest, &fixture.payload, &run)
                .unwrap();
        let mut context = begin_update_evidence_chain_v1(&run, &genesis).unwrap();
        let mut executor = fresh_executor_v1(&run);
        let update_count = usize::try_from(run.checkpoint_segment_updates()).unwrap();
        let mut final_candidate = None;
        for update_ordinal in 0..update_count {
            let prepared = executor.prepare_update_v2().unwrap();
            let built = build_update_group_v1(&run, context, &prepared).unwrap();
            final_candidate = Some(prepared.checkpoint_candidate().clone());
            context = built.into_parts().1;
            drop(prepared);
            if update_ordinal + 1 < update_count {
                executor.run_update_v2().unwrap();
            }
        }
        let final_candidate = final_candidate.unwrap();
        let trained_built =
            build_trained_checkpoint_manifest_v3(&run, &context, &final_candidate).unwrap();
        let trained_manifest = trained_built.canonical_bytes().to_vec();
        let trained_payload = final_candidate.payload().to_vec();
        let trained = decode_trained_checkpoint_manifest_v3(
            &trained_manifest,
            &trained_payload,
            &run,
            &context,
        )
        .unwrap();
        let handle = load_native_checkpoint_inference_v1(&run, &trained, &trained_payload).unwrap();
        let direct_model = direct_checkpoint_model_v1(&trained, &trained_payload);
        let (globals, actions) = decision_parts_v1();
        let actual = handle
            .score_decision_v1(decision_view_v1(&globals, &actions))
            .unwrap();
        let mut tensorizer = NativeFlatTensorizerV2::new();
        let mut tensor = NativeFlatDecisionTensorV2::default();
        tensorizer
            .fill(decision_view_v1(&globals, &actions), &mut tensor)
            .unwrap();
        let expected = direct_model
            .forward_v1(encoded_decision_view_v1(&tensor))
            .unwrap();

        assert_eq!(
            actual
                .action_logits()
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            expected
                .logits
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>()
        );
        assert_eq!(actual.value().to_bits(), expected.value.to_bits());
        assert_eq!(handle.generation_index(), 4);
        assert_eq!(handle.generation_index(), trained.generation_index());
        assert_eq!(
            handle.model_parameter_sha256(),
            trained.model_parameter_sha256()
        );
        assert_eq!(
            handle.model_parameter_sha256(),
            final_candidate.digests().model_parameter_sha256
        );
        assert_eq!(
            handle.train_state_sha256(),
            final_candidate.digests().native_state_sha256
        );
        assert_eq!(
            handle.checkpoint_payload_sha256(),
            final_candidate.digests().payload_sha256
        );
        assert_eq!(
            handle.checkpoint_manifest_sha256(),
            sha256_v1(&trained_manifest)
        );
        assert_ne!(
            handle.model_parameter_sha256(),
            parse_lower_hex_raw32_v1(&run.record().model_snapshot.named_parameter_stream_sha256)
                .unwrap()
        );

        let mut scorer = ComparingCheckpointScorerV1::new_v1(&handle);
        let rollout =
            run_async_flat_scored_rollout_v2(checkpoint_rollout_config_v1(&run), &mut scorer)
                .unwrap();
        assert_eq!(rollout.episodes.len(), 1);
        assert!(rollout.all_natural());
        assert!(scorer.call_count > 1);
        assert!(scorer.checked_transactional_failure);
        assert_eq!(scorer.scorer.first_failure_code(), None);
    }

    #[test]
    fn exact_length_payload_parameter_anchor_and_nonfinite_corruption_fail_closed() {
        let fixture = fixture_v1();
        let (run, checkpoint) = authorities_v1();
        assert_eq!(
            load_native_checkpoint_inference_v1(
                &run,
                &checkpoint,
                &fixture.payload[..fixture.payload.len() - 1],
            )
            .unwrap_err()
            .kind(),
            NativeCheckpointInferenceErrorKindV1::PayloadExactLength
        );

        let mut payload_corruption = fixture.payload.clone();
        let last = payload_corruption.len() - 1;
        payload_corruption[last] ^= 0x80;
        assert_payload_rejected_v1(&payload_corruption);

        let mut parameter_corruption = fixture.payload.clone();
        parameter_corruption[tensor_offset_v1("object_encoder.0.weight")] ^= 1;
        assert_payload_rejected_v1(&parameter_corruption);

        let mut anchor_corruption = fixture.payload.clone();
        anchor_corruption[tensor_offset_v1("scorer.2.bias")] ^= 1;
        assert_payload_rejected_v1(&anchor_corruption);

        let mut nonfinite_corruption = fixture.payload.clone();
        let offset = tensor_offset_v1("object_encoder.0.weight");
        nonfinite_corruption[offset..offset + 4].copy_from_slice(&f32::NAN.to_bits().to_le_bytes());
        assert_payload_rejected_v1(&nonfinite_corruption);
    }

    #[test]
    fn independently_loaded_handles_remain_read_only_and_failure_isolated() {
        let fixture = fixture_v1();
        let (run, checkpoint) = authorities_v1();
        let first =
            load_native_checkpoint_inference_v1(&run, &checkpoint, &fixture.payload).unwrap();
        let second =
            load_native_checkpoint_inference_v1(&run, &checkpoint, &fixture.payload).unwrap();
        let (globals, actions) = decision_parts_v1();
        let first_output = first
            .score_decision_v1(decision_view_v1(&globals, &actions))
            .unwrap();
        drop(first);
        let second_output = second
            .score_decision_v1(decision_view_v1(&globals, &actions))
            .unwrap();
        assert_eq!(first_output, second_output);

        let invalid_globals = FlatGlobalsV2 {
            acting_player: FlatRelativePlayerV2::None,
            ..FlatGlobalsV2::default()
        };
        assert_eq!(
            second
                .score_decision_v1(decision_view_v1(&invalid_globals, &actions))
                .unwrap_err()
                .kind(),
            NativeCheckpointInferenceErrorKindV1::DecisionInvalid
        );
        assert_eq!(
            second
                .score_decision_v1(decision_view_v1(&globals, &actions))
                .unwrap(),
            second_output
        );
    }

    #[test]
    fn load_has_no_filesystem_surface_or_effects() {
        let production_source = include_str!("native_checkpoint_inference_v1.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        for forbidden in ["std::fs", "std::path", "PathBuf", "File::", "OpenOptions"] {
            assert!(!production_source.contains(forbidden));
        }

        let root = std::env::temp_dir().join(format!(
            "mtg-kernel-native-checkpoint-inference-v1-{}-{}",
            std::process::id(),
            TEMP_ORDINAL_V1.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("sentinel"), b"unchanged").unwrap();
        let before = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        let fixture = fixture_v1();
        let (run, checkpoint) = authorities_v1();
        let handle =
            load_native_checkpoint_inference_v1(&run, &checkpoint, &fixture.payload).unwrap();
        let after = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(before, after);
        assert_eq!(fs::read(root.join("sentinel")).unwrap(), b"unchanged");
        drop(handle);
        fs::remove_dir_all(root).unwrap();
    }

    // Design directive slice 3: closes the STOP finding
    // `native_science_loop_v1.rs`'s
    // `ladder_init_science_loop_trains_the_store_end_to_end` test reported --
    // `validate_authority_bindings_v1`'s generation-0-implies-common-snapshot
    // check is a THIRD independent copy of the invariant slice 2 already
    // taught the walk/publish decode dispatch
    // (`decode_genesis_checkpoint_manifest_dispatch_v2_v3`,
    // `native_training_store_checkpoint_v3.rs`) to branch on. This is the
    // checkpoint-INFERENCE layer's own copy: it gates evaluation, the
    // checkpoint runner, and ladder-opponent-engine resolution.

    struct LadderInitFixtureV1 {
        run_bytes: Vec<u8>,
        reference_manifest: CheckpointManifestV3,
        reference_payload: Vec<u8>,
    }

    static LADDER_INIT_FIXTURE_V1: OnceLock<LadderInitFixtureV1> = OnceLock::new();

    const REAL_LADDER_INIT_REFERENCE_STORE_V1: &str =
        r"D:\mtg-kernel-s1-mirror-20260724\dev1\run-0\store";
    const REAL_LADDER_INIT_REFERENCE_GENERATION_V1: u64 = 32;
    const REAL_LADDER_PILOT_POOL_JSON_V1: &str =
        r"D:\mtg-kernel-ladder-pilot-20260725\pool\pool.json";

    /// GenesisInitializationV2 fixture (mirrors the equivalent fixture in
    /// `native_training_store_checkpoint_v3.rs` and `native_science_loop_v1.rs`
    /// -- a real ladder-init run record referencing the real S1 gen-32
    /// checkpoint, D:\mtg-kernel-s1-mirror-20260724\dev1\run-0\store,
    /// read-only source). Each module keeps its own copy so its tests stay
    /// independently runnable; the base seed here is unique to this module.
    fn ladder_init_fixture_v1() -> &'static LadderInitFixtureV1 {
        LADDER_INIT_FIXTURE_V1.get_or_init(|| {
            use crate::native_ladder_pool_resolution_v1::{
                ladder_init_as_checkpoint_ref_v1, resolve_ladder_checkpoint_authority_v1,
                stage_ladder_checkpoint_initialization_v1,
            };
            use crate::native_training_store_run_v2::{
                test_fixture_bytes_with_schedule_and_base_seed_ladder_init_v2,
                OpponentLadderPoolContractV1,
            };

            let pool_bytes = fs::read(REAL_LADDER_PILOT_POOL_JSON_V1)
                .expect("real ladder pilot pool.json must be readable");
            let pool: OpponentLadderPoolContractV1 = serde_json::from_slice(&pool_bytes)
                .expect("pool.json must decode as OpponentLadderPoolContractV1");

            // Design directive slice 2: stages the complete six-field init
            // section (five-field digest-pin plus
            // `derived_model_parameter_sha256`) from the real reference
            // checkpoint on disk.
            let init = stage_ladder_checkpoint_initialization_v1(
                std::path::Path::new(REAL_LADDER_INIT_REFERENCE_STORE_V1),
                REAL_LADDER_INIT_REFERENCE_GENERATION_V1,
            )
            .expect("real S1 mirror gen-32 checkpoint must stage to a valid init section");

            let run_bytes = test_fixture_bytes_with_schedule_and_base_seed_ladder_init_v2(
                NativeTrainingNumericalBackendV1::Sequential,
                2,
                4,
                4,
                2,
                4,
                8,
                32_768,
                65_536,
                555_090,
                pool,
                init,
            );
            let run = decode_train_run_v2(&run_bytes).expect("ladder-init run record");

            // Resolved from the RECORD's own init section (not the staging
            // call above) through the same conversion + chain-proven loader
            // a real caller uses.
            let checkpoint_ref = ladder_init_as_checkpoint_ref_v1(
                run.record()
                    .contracts
                    .opponent_ladder_initialization
                    .as_ref()
                    .expect("fixture record must carry the init section"),
            );
            let authority = resolve_ladder_checkpoint_authority_v1(
                std::path::Path::new(REAL_LADDER_INIT_REFERENCE_STORE_V1),
                &checkpoint_ref,
            )
            .expect("real S1 mirror checkpoint must resolve through the chain-proven loader");
            let (reference_manifest, reference_payload) = authority.into_checkpoint_and_payload();

            LadderInitFixtureV1 {
                run_bytes,
                reference_manifest,
                reference_payload,
            }
        })
    }

    /// THE FIX, positive proof: a continual-init genesis (generation 0,
    /// weights inherited from the real S1 gen-32 checkpoint) now loads
    /// through the inference layer. Before this slice,
    /// `validate_authority_bindings_v1` unconditionally required
    /// `checkpoint.model_parameter_sha256() ==
    /// snapshot.named_parameter_stream_sha256` at generation 0 -- never true
    /// for an inherited genesis -- so this call rejected with
    /// `AuthorityBinding` for every continual-init record, regardless of
    /// target. It now pins against the record's own
    /// `derived_model_parameter_sha256` instead when the init section is
    /// present.
    #[test]
    fn ladder_init_genesis_loads_through_the_inference_layer_against_its_own_derived_digest() {
        use crate::native_training_store_checkpoint_v3::{
            build_genesis_checkpoint_manifest_v2_v3, derive_genesis_weights_only_payload_v2_v3,
        };

        // Machine-local sealed evidence; skips on hosted runners, strict on the science host.
        if !crate::native_test_support_local_evidence_v1::require_local_evidence_v1(
            std::path::Path::new(REAL_LADDER_INIT_REFERENCE_STORE_V1),
        ) {
            return;
        }
        if !crate::native_test_support_local_evidence_v1::require_local_evidence_v1(
            std::path::Path::new(REAL_LADDER_PILOT_POOL_JSON_V1),
        ) {
            return;
        }

        let fixture = ladder_init_fixture_v1();
        let run = decode_train_run_v2(&fixture.run_bytes).expect("ladder-init run record");

        let payload = derive_genesis_weights_only_payload_v2_v3(&fixture.reference_payload)
            .expect("weights-only payload derivation must succeed");
        let checkpoint =
            build_genesis_checkpoint_manifest_v2_v3(&run, &fixture.reference_manifest, &payload)
                .expect("GenesisInitializationV2 authoring must succeed for a matching reference");

        let handle = load_native_checkpoint_inference_v1(&run, &checkpoint, &payload)
            .expect("a continual-init genesis must load through the inference layer");
        assert_eq!(handle.generation_index(), 0);
        assert_eq!(
            handle.model_parameter_sha256(),
            checkpoint.model_parameter_sha256()
        );
    }

    /// Fail-closed (same discipline as
    /// `decode_genesis_checkpoint_manifest_v2_v3_self_contained_rejects_wrong_derived_digest`
    /// in `native_training_store_checkpoint_v3.rs`): a continual-init record
    /// whose OWN claimed `derived_model_parameter_sha256` does not match the
    /// actual gen-0 payload must reject at the inference layer too, not
    /// just at genesis decode. Authors a genuinely valid V2 genesis for a
    /// run whose declared digest is wrong (authoring succeeds -- the
    /// reference-based path ignores that field entirely, it only pins the
    /// reference's own weights), then proves the inference layer's
    /// independent recheck, which trusts nothing but the record's own
    /// claim, catches the mismatch.
    #[test]
    fn ladder_init_genesis_inference_rejects_a_run_with_a_wrong_derived_digest() {
        use crate::native_ladder_pool_resolution_v1::stage_ladder_checkpoint_initialization_v1;
        use crate::native_training_store_checkpoint_v3::{
            build_genesis_checkpoint_manifest_v2_v3, derive_genesis_weights_only_payload_v2_v3,
        };
        use crate::native_training_store_run_v2::{
            test_fixture_bytes_with_schedule_and_base_seed_ladder_init_v2,
            OpponentLadderPoolContractV1,
        };

        // Machine-local sealed evidence; skips on hosted runners, strict on the science host.
        if !crate::native_test_support_local_evidence_v1::require_local_evidence_v1(
            std::path::Path::new(REAL_LADDER_INIT_REFERENCE_STORE_V1),
        ) {
            return;
        }
        if !crate::native_test_support_local_evidence_v1::require_local_evidence_v1(
            std::path::Path::new(REAL_LADDER_PILOT_POOL_JSON_V1),
        ) {
            return;
        }

        let fixture = ladder_init_fixture_v1();

        let mut wrong_init = stage_ladder_checkpoint_initialization_v1(
            std::path::Path::new(REAL_LADDER_INIT_REFERENCE_STORE_V1),
            REAL_LADDER_INIT_REFERENCE_GENERATION_V1,
        )
        .expect("real S1 mirror gen-32 checkpoint must stage to a valid init section");
        assert_ne!(wrong_init.derived_model_parameter_sha256, "0".repeat(64));
        wrong_init.derived_model_parameter_sha256 = "0".repeat(64);

        let pool_bytes = fs::read(REAL_LADDER_PILOT_POOL_JSON_V1)
            .expect("real ladder pilot pool.json must be readable");
        let pool: OpponentLadderPoolContractV1 = serde_json::from_slice(&pool_bytes)
            .expect("pool.json must decode as OpponentLadderPoolContractV1");
        let wrong_run_bytes = test_fixture_bytes_with_schedule_and_base_seed_ladder_init_v2(
            NativeTrainingNumericalBackendV1::Sequential,
            2,
            4,
            4,
            2,
            4,
            8,
            32_768,
            65_536,
            555_091,
            pool,
            wrong_init,
        );
        let wrong_run = decode_train_run_v2(&wrong_run_bytes).expect("ladder-init run record");

        let payload = derive_genesis_weights_only_payload_v2_v3(&fixture.reference_payload)
            .expect("weights-only payload derivation must succeed");
        let checkpoint = build_genesis_checkpoint_manifest_v2_v3(
            &wrong_run,
            &fixture.reference_manifest,
            &payload,
        )
        .expect("reference-based authoring ignores the record's own claimed digest");

        assert_eq!(
            load_native_checkpoint_inference_v1(&wrong_run, &checkpoint, &payload)
                .unwrap_err()
                .kind(),
            NativeCheckpointInferenceErrorKindV1::AuthorityBinding
        );
    }
}
