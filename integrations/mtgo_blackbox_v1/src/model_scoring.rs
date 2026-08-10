use crate::{
    make_offline_intent_v1, MtgoContractErrorV1, MtgoOfflineActionIntentV1,
    ValidatedMtgoObservedDecisionV1,
};
use mtg_kernel::rl::{ActionSemanticV1, ObservationV5};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MTGO_EXTERNAL_MODEL_SCORING_SCHEMA_V1: u32 = 1;

const DEPLOYMENT_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-model-deployment-v1";
const OBSERVATION_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-scoring-observation-v1";
const ORDERED_ACTIONS_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-scoring-ordered-actions-v1";
const REQUEST_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-external-scoring-request-v1";
const SELECTION_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-external-model-selection-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoNativeCheckpointIdentityV1 {
    pub run_sha256: String,
    pub checkpoint_manifest_sha256: String,
    pub checkpoint_payload_sha256: String,
    pub train_state_sha256: String,
    pub model_parameter_sha256: String,
    pub generation_index: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoExpectedModelDeploymentV1 {
    pub schema_version: u32,
    pub deployment_id: String,
    pub checkpoint: MtgoNativeCheckpointIdentityV1,
    pub scorer_contract_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoExternalScoringRequestV1 {
    pub schema_version: u32,
    pub decision_commitment_sha256: String,
    pub observation_sha256: String,
    pub ordered_actions_sha256: String,
    pub action_count: u32,
    pub deployment_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoExternalModelScoreResponseV1 {
    pub schema_version: u32,
    pub request_commitment_sha256: String,
    pub logits_f32_bits: Vec<u32>,
    pub value_f32_bits: u32,
}

/// In-process bridge implemented later by the exact kernel checkpoint scorer.
///
/// The scorer receives immutable exact kernel types and a request commitment.
/// It receives no pixels, process handles, coordinates, authorization, or input
/// capability.
pub trait MtgoExternalObservationScorerV1 {
    fn score_observation_v1(
        &mut self,
        request: &MtgoExternalScoringRequestV1,
        observation: &ObservationV5,
        ordered_legal_actions: &[ActionSemanticV1],
    ) -> Result<MtgoExternalModelScoreResponseV1, MtgoContractErrorV1>;
}

/// One structurally checked score response and deterministic action selection.
///
/// This value is not trusted model authority. The configured deployment and
/// scorer implementation still need an external trust root. It cannot be cloned,
/// serialized, debug-formatted, converted to live input, or used without the
/// exact validated source decision.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoModelSelectionV1;
/// fn cannot_extract_input(selection: &CheckedUntrustedMtgoModelSelectionV1) {
///     let _ = selection.input_command();
/// }
/// ```
pub struct CheckedUntrustedMtgoModelSelectionV1 {
    request: MtgoExternalScoringRequestV1,
    response: MtgoExternalModelScoreResponseV1,
    selected_index: usize,
    selected_semantic: ActionSemanticV1,
    selection_commitment_sha256: String,
}

impl CheckedUntrustedMtgoModelSelectionV1 {
    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn selected_logit_f32_bits(&self) -> u32 {
        self.response.logits_f32_bits[self.selected_index]
    }

    pub fn value_f32_bits(&self) -> u32 {
        self.response.value_f32_bits
    }

    pub fn decision_commitment_sha256(&self) -> &str {
        &self.request.decision_commitment_sha256
    }

    pub fn request_commitment_sha256(&self) -> &str {
        &self.response.request_commitment_sha256
    }

    pub fn selection_commitment_sha256(&self) -> &str {
        &self.selection_commitment_sha256
    }

    pub fn safe_for_live_input(&self) -> bool {
        false
    }

    pub(crate) fn selected_semantic(&self) -> &ActionSemanticV1 {
        &self.selected_semantic
    }
}

pub fn build_external_scoring_request_v1(
    decision: &ValidatedMtgoObservedDecisionV1,
    deployment: &MtgoExpectedModelDeploymentV1,
) -> Result<MtgoExternalScoringRequestV1, MtgoContractErrorV1> {
    let deployment_commitment_sha256 = model_deployment_commitment_v1(deployment)?;
    let action_count = u32::try_from(decision.legal_actions().len()).map_err(|_| {
        MtgoContractErrorV1::new(
            "external_scoring_action_count_overflow",
            decision.legal_actions().len().to_string(),
        )
    })?;
    Ok(MtgoExternalScoringRequestV1 {
        schema_version: MTGO_EXTERNAL_MODEL_SCORING_SCHEMA_V1,
        decision_commitment_sha256: decision.decision_commitment_sha256().to_owned(),
        observation_sha256: canonical_commitment_v1(
            OBSERVATION_COMMITMENT_DOMAIN_V1,
            decision.observation(),
            "external_scoring_observation_serialization_failed",
        )?,
        ordered_actions_sha256: canonical_commitment_v1(
            ORDERED_ACTIONS_COMMITMENT_DOMAIN_V1,
            decision.legal_actions(),
            "external_scoring_actions_serialization_failed",
        )?,
        action_count,
        deployment_commitment_sha256,
    })
}

pub fn scoring_request_commitment_v1(
    request: &MtgoExternalScoringRequestV1,
) -> Result<String, MtgoContractErrorV1> {
    validate_scoring_request_shape_v1(request)?;
    canonical_commitment_v1(
        REQUEST_COMMITMENT_DOMAIN_V1,
        request,
        "external_scoring_request_serialization_failed",
    )
}

pub fn score_and_select_external_model_v1<S: MtgoExternalObservationScorerV1>(
    decision: &ValidatedMtgoObservedDecisionV1,
    deployment: &MtgoExpectedModelDeploymentV1,
    scorer: &mut S,
) -> Result<CheckedUntrustedMtgoModelSelectionV1, MtgoContractErrorV1> {
    let request = build_external_scoring_request_v1(decision, deployment)?;
    let response =
        scorer.score_observation_v1(&request, decision.observation(), decision.legal_actions())?;
    validate_external_model_score_response_v1(decision, deployment, response)
}

pub fn validate_external_model_score_response_v1(
    decision: &ValidatedMtgoObservedDecisionV1,
    deployment: &MtgoExpectedModelDeploymentV1,
    response: MtgoExternalModelScoreResponseV1,
) -> Result<CheckedUntrustedMtgoModelSelectionV1, MtgoContractErrorV1> {
    let request = build_external_scoring_request_v1(decision, deployment)?;
    let request_commitment_sha256 = scoring_request_commitment_v1(&request)?;
    if response.schema_version != MTGO_EXTERNAL_MODEL_SCORING_SCHEMA_V1 {
        return Err(MtgoContractErrorV1::new(
            "external_scoring_response_schema_mismatch",
            response.schema_version.to_string(),
        ));
    }
    require_sha256_v1(
        &response.request_commitment_sha256,
        "external_scoring_response_request_hash_invalid",
    )?;
    if response.request_commitment_sha256 != request_commitment_sha256 {
        return Err(MtgoContractErrorV1::new(
            "external_scoring_response_request_mismatch",
            "score response does not bind the exact scoring request",
        ));
    }
    if response.logits_f32_bits.len() != decision.legal_actions().len()
        || response.logits_f32_bits.is_empty()
    {
        return Err(MtgoContractErrorV1::new(
            "external_scoring_logit_count_mismatch",
            format!(
                "expected={},actual={}",
                decision.legal_actions().len(),
                response.logits_f32_bits.len()
            ),
        ));
    }
    let logits = response
        .logits_f32_bits
        .iter()
        .map(|bits| f32::from_bits(*bits))
        .collect::<Vec<_>>();
    if logits.iter().any(|value| !value.is_finite()) {
        return Err(MtgoContractErrorV1::new(
            "external_scoring_logit_nonfinite",
            "every policy logit must be finite",
        ));
    }
    if !f32::from_bits(response.value_f32_bits).is_finite() {
        return Err(MtgoContractErrorV1::new(
            "external_scoring_value_nonfinite",
            "the value output must be finite",
        ));
    }

    let mut selected_index = 0;
    for index in 1..logits.len() {
        if logits[index].total_cmp(&logits[selected_index]).is_gt() {
            selected_index = index;
        }
    }
    let selected_semantic = decision.legal_actions()[selected_index].clone();
    #[derive(Serialize)]
    struct SelectionCommitmentRecordV1<'a> {
        request: &'a MtgoExternalScoringRequestV1,
        response: &'a MtgoExternalModelScoreResponseV1,
        selected_index: usize,
        selected_semantic: &'a ActionSemanticV1,
    }
    let selection_commitment_sha256 = canonical_commitment_v1(
        SELECTION_COMMITMENT_DOMAIN_V1,
        &SelectionCommitmentRecordV1 {
            request: &request,
            response: &response,
            selected_index,
            selected_semantic: &selected_semantic,
        },
        "external_scoring_selection_serialization_failed",
    )?;
    Ok(CheckedUntrustedMtgoModelSelectionV1 {
        request,
        response,
        selected_index,
        selected_semantic,
        selection_commitment_sha256,
    })
}

pub fn make_scored_offline_intent_v1(
    decision: &ValidatedMtgoObservedDecisionV1,
    selection: &CheckedUntrustedMtgoModelSelectionV1,
) -> Result<MtgoOfflineActionIntentV1, MtgoContractErrorV1> {
    if selection.request.decision_commitment_sha256 != decision.decision_commitment_sha256()
        || decision.legal_actions().get(selection.selected_index)
            != Some(&selection.selected_semantic)
    {
        return Err(MtgoContractErrorV1::new(
            "external_scoring_selection_decision_mismatch",
            "the selection does not belong to this validated decision",
        ));
    }
    make_offline_intent_v1(decision, selection.selected_index)
}

/// Validates and commits the exact model deployment identity shared by
/// gameplay and pregame scoring adapters.
pub fn model_deployment_commitment_v1(
    deployment: &MtgoExpectedModelDeploymentV1,
) -> Result<String, MtgoContractErrorV1> {
    if deployment.schema_version != MTGO_EXTERNAL_MODEL_SCORING_SCHEMA_V1 {
        return Err(MtgoContractErrorV1::new(
            "external_scoring_deployment_schema_mismatch",
            deployment.schema_version.to_string(),
        ));
    }
    validate_safe_identifier_v1(
        &deployment.deployment_id,
        "external_scoring_deployment_id_invalid",
    )?;
    for digest in [
        &deployment.checkpoint.run_sha256,
        &deployment.checkpoint.checkpoint_manifest_sha256,
        &deployment.checkpoint.checkpoint_payload_sha256,
        &deployment.checkpoint.train_state_sha256,
        &deployment.checkpoint.model_parameter_sha256,
        &deployment.scorer_contract_sha256,
    ] {
        require_sha256_v1(digest, "external_scoring_deployment_hash_invalid")?;
    }
    canonical_commitment_v1(
        DEPLOYMENT_COMMITMENT_DOMAIN_V1,
        deployment,
        "external_scoring_deployment_serialization_failed",
    )
}

fn validate_scoring_request_shape_v1(
    request: &MtgoExternalScoringRequestV1,
) -> Result<(), MtgoContractErrorV1> {
    if request.schema_version != MTGO_EXTERNAL_MODEL_SCORING_SCHEMA_V1 {
        return Err(MtgoContractErrorV1::new(
            "external_scoring_request_schema_mismatch",
            request.schema_version.to_string(),
        ));
    }
    if request.action_count == 0 || request.action_count > 64 {
        return Err(MtgoContractErrorV1::new(
            "external_scoring_request_action_count_invalid",
            request.action_count.to_string(),
        ));
    }
    for digest in [
        &request.decision_commitment_sha256,
        &request.observation_sha256,
        &request.ordered_actions_sha256,
        &request.deployment_commitment_sha256,
    ] {
        require_sha256_v1(digest, "external_scoring_request_hash_invalid")?;
    }
    Ok(())
}

fn canonical_commitment_v1<T: Serialize + ?Sized>(
    domain: &[u8],
    value: &T,
    code: &'static str,
) -> Result<String, MtgoContractErrorV1> {
    let encoded = serde_json::to_vec(value)
        .map_err(|error| MtgoContractErrorV1::new(code, error.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update((encoded.len() as u64).to_le_bytes());
    hasher.update(encoded);
    Ok(format!("{:x}", hasher.finalize()))
}

fn require_sha256_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(MtgoContractErrorV1::new(code, value));
    }
    Ok(())
}

fn validate_safe_identifier_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(MtgoContractErrorV1::new(code, value));
    }
    Ok(())
}
