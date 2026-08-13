use crate::{
    build_player_visible_duel_decision_input_v1, validate_observed_decision_v1,
    CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1,
    CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1, MtgoContractErrorV1,
    MtgoObservedDecisionV1, MtgoPlayerRelativeRoleV1, MtgoPlayerVisibleDuelActionV1,
    MtgoPlayerVisibleDuelDecisionInputV1, MtgoSizePxV1,
};
use mtg_kernel::rl::ActionSemanticV1;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_DUEL_PERCEPTION_EVALUATION_SCHEMA_V1: u32 = 1;
pub const MTGO_PLAYER_VISIBLE_DUEL_PERCEPTION_EVALUATION_SCHEMA_V2: u32 = 2;

const DUEL_PERCEPTION_EVALUATION_DOMAIN_V1: &[u8] = b"mtgo-duel-perception-evaluation-v1";
const PLAYER_VISIBLE_DUEL_PERCEPTION_EVALUATION_DOMAIN_V2: &[u8] =
    b"mtgo-player-visible-duel-perception-evaluation-v2";
const DUEL_PERCEPTION_PROFILE_ADMISSION_DOMAIN_V1: &[u8] =
    b"mtgo-duel-perception-profile-admission-v1";
const PLAYER_VISIBLE_DUEL_PERCEPTION_PROFILE_ADMISSION_DOMAIN_V2: &[u8] =
    b"mtgo-player-visible-duel-perception-profile-admission-v2";
const RATIFIED_DUEL_PERCEPTION_EVALUATION_COMMITMENT_V1: Option<&str> = None;
pub(crate) const RATIFIED_PLAYER_VISIBLE_DUEL_PERCEPTION_EVALUATION_COMMITMENT_V2: Option<&str> =
    None;
const CANONICAL_PIXEL_FORMAT_V1: &str = "bgra8_unorm_top_down_tightly_packed_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoDuelActionFamilyV1 {
    PriorityPass,
    PlayLand,
    CastOrPlotSpell,
    ManaAbility,
    NonManaAbility,
    TargetChoice,
    CostOrModeChoice,
    EffectChoice,
    Discard,
    CombatChoice,
    TriggerOrdering,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDuelPerceptionRuntimeProfileV1 {
    pub schema_version: u32,
    pub profile_id: String,
    pub executable_sha256: String,
    pub signer_thumbprint: String,
    pub signer_subject_sha256: String,
    pub dpi: u32,
    pub client_size_px: MtgoSizePxV1,
    pub output_identity_sha256: String,
    pub game_format: String,
    pub canonical_pixel_format: String,
    pub perception_pipeline_binary_sha256: String,
    pub classifier_assets_manifest_sha256: String,
    pub card_database_profile_sha256: String,
    pub supported_action_families: Vec<MtgoDuelActionFamilyV1>,
}

/// Structurally checked runtime facts and classifier identities for one duel
/// perception profile. The caller still supplies this payload, so the wrapper
/// is not an admission or a claim of measured accuracy.
pub struct CheckedUntrustedMtgoDuelPerceptionRuntimeProfileV1 {
    payload: MtgoDuelPerceptionRuntimeProfileV1,
    profile_commitment_sha256: String,
}

impl CheckedUntrustedMtgoDuelPerceptionRuntimeProfileV1 {
    pub fn profile_commitment_sha256(&self) -> &str {
        &self.profile_commitment_sha256
    }

    pub fn executable_sha256(&self) -> &str {
        &self.payload.executable_sha256
    }

    pub fn signer_thumbprint(&self) -> &str {
        &self.payload.signer_thumbprint
    }

    pub fn signer_subject_sha256(&self) -> &str {
        &self.payload.signer_subject_sha256
    }

    pub fn dpi(&self) -> u32 {
        self.payload.dpi
    }

    pub fn client_size_px(&self) -> &MtgoSizePxV1 {
        &self.payload.client_size_px
    }

    pub fn output_identity_sha256(&self) -> &str {
        &self.payload.output_identity_sha256
    }

    pub fn game_format(&self) -> &str {
        &self.payload.game_format
    }

    pub fn perception_pipeline_binary_sha256(&self) -> &str {
        &self.payload.perception_pipeline_binary_sha256
    }

    pub fn classifier_assets_manifest_sha256(&self) -> &str {
        &self.payload.classifier_assets_manifest_sha256
    }

    pub fn card_database_profile_sha256(&self) -> &str {
        &self.payload.card_database_profile_sha256
    }

    pub fn supported_action_families(&self) -> &[MtgoDuelActionFamilyV1] {
        &self.payload.supported_action_families
    }

    pub fn safe_for_model_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDuelPerceptionEvaluationSpecV1 {
    pub schema_version: u32,
    pub evaluation_id: String,
    pub perception_profile_commitment_sha256: String,
    pub corpus_manifest_sha256: String,
    pub annotation_protocol_sha256: String,
    pub evaluator_binary_sha256: String,
    pub minimum_unique_cases: u32,
    pub minimum_prediction_coverage_bps: u16,
    pub required_action_families: Vec<MtgoDuelActionFamilyV1>,
}

/// Legacy full-kernel reconstruction comparison for one acting-player frame.
///
/// This is retained for offline adapter diagnostics only. Its expected value
/// can represent kernel bookkeeping, so v1 is permanently ineligible for the
/// competitive perception ratification path.
pub struct MtgoDuelPerceptionEvaluationCaseV1<'a> {
    pub case_id: String,
    pub source_manifest_sha256: String,
    pub source_canonical_bgra8_sha256: String,
    pub source_frame_sequence: u64,
    pub expected: MtgoObservedDecisionV1,
    pub prediction: Option<&'a CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleDuelPerceptionEvaluationSpecV2 {
    pub schema_version: u32,
    pub evaluation_id: String,
    pub perception_profile_commitment_sha256: String,
    pub corpus_manifest_sha256: String,
    pub annotation_protocol_sha256: String,
    pub evaluator_binary_sha256: String,
    pub minimum_unique_cases: u32,
    pub minimum_prediction_coverage_bps: u16,
    pub required_action_families: Vec<MtgoDuelActionFamilyV1>,
}

/// One player-visible annotation and optional prediction for an exact corpus
/// frame. The expected value's type cannot represent `ObservationV5`, kernel
/// object references, card database identifiers, capture lineage, or any
/// hidden opponent zone.
pub struct MtgoPlayerVisibleDuelPerceptionEvaluationCaseV2<'a> {
    pub case_id: String,
    pub source_manifest_sha256: String,
    pub source_canonical_bgra8_sha256: String,
    pub prediction: Option<&'a CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1>,
}

/// Recomputed accuracy over the exact player-visible model payload. It has no
/// observations, pixels, source metadata, model-scoring authority, or input
/// conversion.
///
/// This type intentionally has no `Debug`, `Clone`, or serde implementation.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoPlayerVisibleDuelPerceptionEvaluationV2;
/// fn cannot_read_predictions(
///     value: &CheckedUntrustedMtgoPlayerVisibleDuelPerceptionEvaluationV2,
/// ) {
///     let _ = value.observation();
///     let _ = value.legal_actions();
///     let _ = value.pixels();
/// }
/// ```
pub struct CheckedUntrustedMtgoPlayerVisibleDuelPerceptionEvaluationV2 {
    perception_profile_commitment_sha256: String,
    evaluation_commitment_sha256: String,
    unique_case_count: u32,
    prediction_count: u32,
    exact_visible_state_count: u32,
    exact_visible_legal_action_count: u32,
    exact_visible_decision_input_count: u32,
    prediction_coverage_bps: u16,
    missing_action_families: Vec<MtgoDuelActionFamilyV1>,
    passes_declared_gate: bool,
}

impl CheckedUntrustedMtgoPlayerVisibleDuelPerceptionEvaluationV2 {
    pub fn perception_profile_commitment_sha256(&self) -> &str {
        &self.perception_profile_commitment_sha256
    }

    pub fn evaluation_commitment_sha256(&self) -> &str {
        &self.evaluation_commitment_sha256
    }

    pub fn unique_case_count(&self) -> u32 {
        self.unique_case_count
    }

    pub fn prediction_count(&self) -> u32 {
        self.prediction_count
    }

    pub fn exact_visible_state_count(&self) -> u32 {
        self.exact_visible_state_count
    }

    pub fn exact_visible_legal_action_count(&self) -> u32 {
        self.exact_visible_legal_action_count
    }

    pub fn exact_visible_decision_input_count(&self) -> u32 {
        self.exact_visible_decision_input_count
    }

    pub fn prediction_coverage_bps(&self) -> u16 {
        self.prediction_coverage_bps
    }

    pub fn missing_action_families(&self) -> &[MtgoDuelActionFamilyV1] {
        &self.missing_action_families
    }

    pub fn passes_declared_gate(&self) -> bool {
        self.passes_declared_gate
    }

    pub fn safe_for_model_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

/// Recomputed corpus result without runtime authority.
///
/// This type intentionally has no `Debug`, `Clone`, or serde implementation.
/// It exposes measurements and commitments only, not observations or actions.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoDuelPerceptionEvaluationV1;
/// fn cannot_score(value: &CheckedUntrustedMtgoDuelPerceptionEvaluationV1) {
///     let _ = value.observation();
///     let _ = value.legal_actions();
/// }
/// ```
pub struct CheckedUntrustedMtgoDuelPerceptionEvaluationV1 {
    perception_profile_commitment_sha256: String,
    evaluation_commitment_sha256: String,
    unique_case_count: u32,
    prediction_count: u32,
    exact_observation_count: u32,
    exact_legal_action_count: u32,
    exact_object_binding_count: u32,
    exact_payload_count: u32,
    prediction_coverage_bps: u16,
    missing_action_families: Vec<MtgoDuelActionFamilyV1>,
    passes_declared_gate: bool,
}

impl CheckedUntrustedMtgoDuelPerceptionEvaluationV1 {
    pub fn perception_profile_commitment_sha256(&self) -> &str {
        &self.perception_profile_commitment_sha256
    }

    pub fn evaluation_commitment_sha256(&self) -> &str {
        &self.evaluation_commitment_sha256
    }

    pub fn unique_case_count(&self) -> u32 {
        self.unique_case_count
    }

    pub fn prediction_count(&self) -> u32 {
        self.prediction_count
    }

    pub fn exact_observation_count(&self) -> u32 {
        self.exact_observation_count
    }

    pub fn exact_legal_action_count(&self) -> u32 {
        self.exact_legal_action_count
    }

    pub fn exact_object_binding_count(&self) -> u32 {
        self.exact_object_binding_count
    }

    pub fn exact_payload_count(&self) -> u32 {
        self.exact_payload_count
    }

    pub fn prediction_coverage_bps(&self) -> u16 {
        self.prediction_coverage_bps
    }

    pub fn missing_action_families(&self) -> &[MtgoDuelActionFamilyV1] {
        &self.missing_action_families
    }

    pub fn passes_declared_gate(&self) -> bool {
        self.passes_declared_gate
    }

    pub fn safe_for_model_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoDuelPerceptionProfileScopeV1 {
    ActingPlayerDuelSemanticPerception,
}

/// A profile pinned by a separately reviewed production commitment.
///
/// Production currently contains no such commitment, so callers cannot obtain
/// this type. Even when ratified, this value grants profile identity only. It
/// is not capture attestation, a scorable decision, or input authority.
pub struct AdmittedMtgoDuelPerceptionProfileV1 {
    profile: CheckedUntrustedMtgoDuelPerceptionRuntimeProfileV1,
    evaluation_commitment_sha256: String,
    admission_commitment_sha256: String,
}

impl AdmittedMtgoDuelPerceptionProfileV1 {
    pub fn scope(&self) -> MtgoDuelPerceptionProfileScopeV1 {
        MtgoDuelPerceptionProfileScopeV1::ActingPlayerDuelSemanticPerception
    }

    pub fn perception_profile_commitment_sha256(&self) -> &str {
        self.profile.profile_commitment_sha256()
    }

    pub fn checked_runtime_profile_v1(
        &self,
    ) -> &CheckedUntrustedMtgoDuelPerceptionRuntimeProfileV1 {
        &self.profile
    }

    pub fn evaluation_commitment_sha256(&self) -> &str {
        &self.evaluation_commitment_sha256
    }

    pub fn admission_commitment_sha256(&self) -> &str {
        &self.admission_commitment_sha256
    }

    pub fn executable_sha256(&self) -> &str {
        self.profile.executable_sha256()
    }

    pub fn signer_thumbprint(&self) -> &str {
        self.profile.signer_thumbprint()
    }

    pub fn signer_subject_sha256(&self) -> &str {
        self.profile.signer_subject_sha256()
    }

    pub fn dpi(&self) -> u32 {
        self.profile.dpi()
    }

    pub fn client_size_px(&self) -> &MtgoSizePxV1 {
        self.profile.client_size_px()
    }

    pub fn output_identity_sha256(&self) -> &str {
        self.profile.output_identity_sha256()
    }

    pub fn game_format(&self) -> &str {
        self.profile.game_format()
    }

    pub fn perception_pipeline_binary_sha256(&self) -> &str {
        self.profile.perception_pipeline_binary_sha256()
    }

    pub fn classifier_assets_manifest_sha256(&self) -> &str {
        self.profile.classifier_assets_manifest_sha256()
    }

    pub fn card_database_profile_sha256(&self) -> &str {
        self.profile.card_database_profile_sha256()
    }

    pub fn supported_action_families(&self) -> &[MtgoDuelActionFamilyV1] {
        self.profile.supported_action_families()
    }

    pub fn safe_for_model_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

pub fn check_untrusted_duel_perception_runtime_profile_v1(
    payload: MtgoDuelPerceptionRuntimeProfileV1,
) -> Result<CheckedUntrustedMtgoDuelPerceptionRuntimeProfileV1, MtgoContractErrorV1> {
    if payload.schema_version != MTGO_DUEL_PERCEPTION_EVALUATION_SCHEMA_V1 {
        return Err(error_v1(
            "duel_perception_runtime_profile_schema_mismatch",
            payload.schema_version.to_string(),
        ));
    }
    validate_safe_identifier_v1(
        &payload.profile_id,
        "duel_perception_runtime_profile_id_invalid",
    )?;
    for (field, value) in [
        (
            "duel_perception_executable",
            payload.executable_sha256.as_str(),
        ),
        (
            "duel_perception_signer_subject",
            payload.signer_subject_sha256.as_str(),
        ),
        (
            "duel_perception_output_identity",
            payload.output_identity_sha256.as_str(),
        ),
        (
            "duel_perception_pipeline_binary",
            payload.perception_pipeline_binary_sha256.as_str(),
        ),
        (
            "duel_perception_classifier_assets",
            payload.classifier_assets_manifest_sha256.as_str(),
        ),
        (
            "duel_perception_card_database",
            payload.card_database_profile_sha256.as_str(),
        ),
    ] {
        validate_lower_hex_sha256_v1(field, value)?;
    }
    validate_lower_hex_v1(
        "duel_perception_signer_thumbprint",
        &payload.signer_thumbprint,
        40,
    )?;
    if !(96..=480).contains(&payload.dpi)
        || payload.client_size_px.width == 0
        || payload.client_size_px.height == 0
        || payload.client_size_px.width > 16_384
        || payload.client_size_px.height > 16_384
    {
        return Err(error_v1(
            "duel_perception_runtime_geometry_invalid",
            format!(
                "dpi={},width={},height={}",
                payload.dpi, payload.client_size_px.width, payload.client_size_px.height
            ),
        ));
    }
    validate_game_format_v1(&payload.game_format)?;
    if payload.canonical_pixel_format != CANONICAL_PIXEL_FORMAT_V1 {
        return Err(error_v1(
            "duel_perception_pixel_format_invalid",
            payload.canonical_pixel_format.clone(),
        ));
    }
    validate_action_families_v1(&payload.supported_action_families)?;

    let encoded = serde_json::to_vec(&payload).map_err(|error| {
        error_v1(
            "duel_perception_runtime_profile_serialization_failed",
            error.to_string(),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(b"mtgo-duel-perception-runtime-profile-v1");
    hash_part_v1(&mut hasher, &encoded);
    Ok(CheckedUntrustedMtgoDuelPerceptionRuntimeProfileV1 {
        payload,
        profile_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

#[cfg(test)]
pub(crate) fn duel_perception_runtime_profile_payload_for_test_v1(
) -> MtgoDuelPerceptionRuntimeProfileV1 {
    MtgoDuelPerceptionRuntimeProfileV1 {
        schema_version: MTGO_DUEL_PERCEPTION_EVALUATION_SCHEMA_V1,
        profile_id: "acting-player-duel-perception-test-v1".to_owned(),
        executable_sha256: "a".repeat(64),
        signer_thumbprint: "b".repeat(40),
        signer_subject_sha256: "c".repeat(64),
        dpi: 120,
        client_size_px: MtgoSizePxV1 {
            width: 1_550,
            height: 925,
        },
        output_identity_sha256: "4".repeat(64),
        game_format: "Freeform".to_owned(),
        canonical_pixel_format: CANONICAL_PIXEL_FORMAT_V1.to_owned(),
        perception_pipeline_binary_sha256: "5".repeat(64),
        classifier_assets_manifest_sha256: "6".repeat(64),
        card_database_profile_sha256: "7".repeat(64),
        supported_action_families: vec![
            MtgoDuelActionFamilyV1::PriorityPass,
            MtgoDuelActionFamilyV1::PlayLand,
        ],
    }
}

#[cfg(test)]
pub(crate) fn duel_perception_runtime_profile_for_test_v1(
) -> CheckedUntrustedMtgoDuelPerceptionRuntimeProfileV1 {
    check_untrusted_duel_perception_runtime_profile_v1(
        duel_perception_runtime_profile_payload_for_test_v1(),
    )
    .expect("test duel perception profile is valid")
}

#[cfg(test)]
pub(crate) fn duel_perception_profile_admitted_for_test_v1(
    profile: CheckedUntrustedMtgoDuelPerceptionRuntimeProfileV1,
) -> AdmittedMtgoDuelPerceptionProfileV1 {
    let evaluation_commitment_sha256 = "e".repeat(64);
    let mut hasher = Sha256::new();
    hasher.update(DUEL_PERCEPTION_PROFILE_ADMISSION_DOMAIN_V1);
    for part in [
        profile.profile_commitment_sha256().as_bytes(),
        evaluation_commitment_sha256.as_bytes(),
        b"acting_player_duel_semantic_perception",
        b"profile_identity_only_no_scoring_or_input",
    ] {
        hash_part_v1(&mut hasher, part);
    }
    AdmittedMtgoDuelPerceptionProfileV1 {
        profile,
        evaluation_commitment_sha256,
        admission_commitment_sha256: format!("{:x}", hasher.finalize()),
    }
}

pub fn evaluate_untrusted_duel_perception_profile_v1(
    profile: &CheckedUntrustedMtgoDuelPerceptionRuntimeProfileV1,
    corpus: &CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1,
    spec: MtgoDuelPerceptionEvaluationSpecV1,
    cases: Vec<MtgoDuelPerceptionEvaluationCaseV1<'_>>,
) -> Result<CheckedUntrustedMtgoDuelPerceptionEvaluationV1, MtgoContractErrorV1> {
    validate_spec_v1(&spec)?;
    if spec.perception_profile_commitment_sha256 != profile.profile_commitment_sha256()
        || spec.required_action_families != profile.supported_action_families()
    {
        return Err(error_v1(
            "duel_perception_evaluation_profile_mismatch",
            "evaluation spec must bind the exact runtime profile and supported action families",
        ));
    }
    if spec.corpus_manifest_sha256 != corpus.canonical_manifest_sha256()
        || profile.game_format() != corpus.manifest_v1().game_format
    {
        return Err(error_v1(
            "duel_perception_evaluation_corpus_mismatch",
            "evaluation spec and runtime profile must bind the exact checked duel corpus",
        ));
    }
    if cases.is_empty() || cases.len() > 100_000 {
        return Err(error_v1(
            "duel_perception_case_count_invalid",
            cases.len().to_string(),
        ));
    }
    if cases.len() != corpus.sample_count() {
        return Err(error_v1(
            "duel_perception_evaluation_corpus_coverage",
            "evaluation must contain exactly one case for every checked corpus sample",
        ));
    }

    let encoded_spec = serde_json::to_vec(&spec).map_err(|error| {
        error_v1(
            "duel_perception_spec_serialization_failed",
            error.to_string(),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(DUEL_PERCEPTION_EVALUATION_DOMAIN_V1);
    hash_part_v1(&mut hasher, &encoded_spec);

    let mut previous_case_id: Option<&str> = None;
    let mut source_manifests = HashSet::new();
    let mut observed_action_families = HashSet::new();
    let mut prediction_count = 0_u32;
    let mut exact_observation_count = 0_u32;
    let mut exact_legal_action_count = 0_u32;
    let mut exact_object_binding_count = 0_u32;
    let mut exact_payload_count = 0_u32;

    for (corpus_sample, case) in corpus.manifest_v1().samples.iter().zip(&cases) {
        validate_safe_identifier_v1(&case.case_id, "duel_perception_case_id_invalid")?;
        if previous_case_id.is_some_and(|previous| previous >= case.case_id.as_str()) {
            return Err(error_v1(
                "duel_perception_case_order_invalid",
                "case IDs must be unique and strictly increasing",
            ));
        }
        previous_case_id = Some(&case.case_id);
        validate_lower_hex_sha256_v1(
            "duel_perception_source_manifest",
            &case.source_manifest_sha256,
        )?;
        validate_lower_hex_sha256_v1(
            "duel_perception_source_frame",
            &case.source_canonical_bgra8_sha256,
        )?;
        if case.source_frame_sequence == 0 {
            return Err(error_v1(
                "duel_perception_source_sequence_invalid",
                case.case_id.clone(),
            ));
        }
        if case.case_id != corpus_sample.sample_id
            || case.source_manifest_sha256 != corpus_sample.source_manifest_sha256
            || case.source_canonical_bgra8_sha256 != corpus_sample.source_canonical_bgra8_sha256
        {
            return Err(error_v1(
                "duel_perception_evaluation_corpus_source_mismatch",
                case.case_id.clone(),
            ));
        }
        if !source_manifests.insert(case.source_manifest_sha256.as_str()) {
            return Err(error_v1(
                "duel_perception_duplicate_source",
                case.source_manifest_sha256.clone(),
            ));
        }

        validate_expected_source_v1(case)?;
        let expected = validate_observed_decision_v1(case.expected.clone())?;
        for action in expected.legal_actions() {
            observed_action_families.insert(duel_action_family_v1(action));
        }

        let mut observation_exact = false;
        let mut legal_actions_exact = false;
        let mut object_bindings_exact = false;
        let mut payload_exact = false;
        let prediction_commitment = if let Some(prediction) = case.prediction {
            validate_prediction_source_v1(&spec, case, prediction)?;
            prediction_count += 1;
            let predicted = prediction.validated_decision_v1();
            observation_exact = predicted.observation() == expected.observation();
            legal_actions_exact = predicted.legal_actions() == expected.legal_actions();
            object_bindings_exact = predicted.object_bindings() == expected.object_bindings();
            payload_exact = observation_exact && legal_actions_exact && object_bindings_exact;
            exact_observation_count += u32::from(observation_exact);
            exact_legal_action_count += u32::from(legal_actions_exact);
            exact_object_binding_count += u32::from(object_bindings_exact);
            exact_payload_count += u32::from(payload_exact);
            prediction.candidate_commitment_sha256()
        } else {
            "abstained"
        };

        for part in [
            case.case_id.as_bytes(),
            case.source_manifest_sha256.as_bytes(),
            case.source_canonical_bgra8_sha256.as_bytes(),
            &case.source_frame_sequence.to_le_bytes(),
            expected.decision_commitment_sha256().as_bytes(),
            prediction_commitment.as_bytes(),
            &[
                u8::from(observation_exact),
                u8::from(legal_actions_exact),
                u8::from(object_bindings_exact),
                u8::from(payload_exact),
            ],
        ] {
            hash_part_v1(&mut hasher, part);
        }
    }

    let unique_case_count = u32::try_from(cases.len()).map_err(|_| {
        error_v1(
            "duel_perception_case_count_invalid",
            "case count does not fit u32",
        )
    })?;
    let prediction_coverage_bps = ratio_bps_v1(prediction_count, unique_case_count);
    let missing_action_families = spec
        .required_action_families
        .iter()
        .copied()
        .filter(|family| !observed_action_families.contains(family))
        .collect::<Vec<_>>();
    let passes_declared_gate = unique_case_count >= spec.minimum_unique_cases
        && prediction_coverage_bps >= spec.minimum_prediction_coverage_bps
        && prediction_count != 0
        && exact_observation_count == prediction_count
        && exact_legal_action_count == prediction_count
        && exact_object_binding_count == prediction_count
        && exact_payload_count == prediction_count
        && missing_action_families.is_empty();

    Ok(CheckedUntrustedMtgoDuelPerceptionEvaluationV1 {
        perception_profile_commitment_sha256: spec.perception_profile_commitment_sha256,
        evaluation_commitment_sha256: format!("{:x}", hasher.finalize()),
        unique_case_count,
        prediction_count,
        exact_observation_count,
        exact_legal_action_count,
        exact_object_binding_count,
        exact_payload_count,
        prediction_coverage_bps,
        missing_action_families,
        passes_declared_gate,
    })
}

pub fn evaluate_untrusted_player_visible_duel_perception_profile_v2(
    profile: &CheckedUntrustedMtgoDuelPerceptionRuntimeProfileV1,
    corpus: &CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1,
    annotations: &crate::CheckedUntrustedMtgoPlayerVisibleDuelAnnotationSetV1,
    spec: MtgoPlayerVisibleDuelPerceptionEvaluationSpecV2,
    cases: Vec<MtgoPlayerVisibleDuelPerceptionEvaluationCaseV2<'_>>,
) -> Result<CheckedUntrustedMtgoPlayerVisibleDuelPerceptionEvaluationV2, MtgoContractErrorV1> {
    validate_player_visible_spec_v2(&spec)?;
    if spec.perception_profile_commitment_sha256 != profile.profile_commitment_sha256()
        || spec.required_action_families != profile.supported_action_families()
    {
        return Err(error_v1(
            "player_visible_duel_perception_evaluation_profile_mismatch",
            "evaluation spec must bind the exact runtime profile and supported action families",
        ));
    }
    if spec.corpus_manifest_sha256 != corpus.canonical_manifest_sha256()
        || profile.game_format() != corpus.manifest_v1().game_format
        || annotations.corpus_manifest_sha256() != corpus.canonical_manifest_sha256()
        || annotations.corpus_commitment_sha256() != corpus.corpus_commitment_sha256()
        || spec.annotation_protocol_sha256 != annotations.annotation_protocol_sha256()
    {
        return Err(error_v1(
            "player_visible_duel_perception_evaluation_corpus_mismatch",
            "evaluation spec and runtime profile must bind the exact checked duel corpus",
        ));
    }
    if cases.is_empty() || cases.len() > 100_000 {
        return Err(error_v1(
            "player_visible_duel_perception_case_count_invalid",
            cases.len().to_string(),
        ));
    }
    if cases.len() != corpus.sample_count() {
        return Err(error_v1(
            "player_visible_duel_perception_evaluation_corpus_coverage",
            "evaluation must contain exactly one case for every checked corpus sample",
        ));
    }

    let encoded_spec = serde_json::to_vec(&spec).map_err(|error| {
        error_v1(
            "player_visible_duel_perception_spec_serialization_failed",
            error.to_string(),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(PLAYER_VISIBLE_DUEL_PERCEPTION_EVALUATION_DOMAIN_V2);
    hash_part_v1(&mut hasher, &encoded_spec);
    hash_part_v1(
        &mut hasher,
        annotations.annotation_set_commitment_sha256().as_bytes(),
    );

    let mut previous_case_id: Option<&str> = None;
    let mut source_manifests = HashSet::new();
    let mut observed_action_families = HashSet::new();
    let mut prediction_count = 0_u32;
    let mut exact_visible_state_count = 0_u32;
    let mut exact_visible_legal_action_count = 0_u32;
    let mut exact_visible_decision_input_count = 0_u32;

    for ((corpus_sample, annotation), case) in corpus
        .manifest_v1()
        .samples
        .iter()
        .zip(&annotations.manifest_v1().entries)
        .zip(&cases)
    {
        validate_safe_identifier_v1(
            &case.case_id,
            "player_visible_duel_perception_case_id_invalid",
        )?;
        if previous_case_id.is_some_and(|previous| previous >= case.case_id.as_str()) {
            return Err(error_v1(
                "player_visible_duel_perception_case_order_invalid",
                "case IDs must be unique and strictly increasing",
            ));
        }
        previous_case_id = Some(&case.case_id);
        validate_lower_hex_sha256_v1(
            "player_visible_duel_perception_source_manifest",
            &case.source_manifest_sha256,
        )?;
        validate_lower_hex_sha256_v1(
            "player_visible_duel_perception_source_frame",
            &case.source_canonical_bgra8_sha256,
        )?;
        if case.case_id != corpus_sample.sample_id
            || case.case_id != annotation.case_id
            || case.source_manifest_sha256 != corpus_sample.source_manifest_sha256
            || case.source_manifest_sha256 != annotation.source_manifest_sha256
            || case.source_canonical_bgra8_sha256 != corpus_sample.source_canonical_bgra8_sha256
            || case.source_canonical_bgra8_sha256 != annotation.source_canonical_bgra8_sha256
        {
            return Err(error_v1(
                "player_visible_duel_perception_evaluation_corpus_source_mismatch",
                case.case_id.clone(),
            ));
        }
        if !source_manifests.insert(case.source_manifest_sha256.as_str()) {
            return Err(error_v1(
                "player_visible_duel_perception_duplicate_source",
                case.case_id.clone(),
            ));
        }
        validate_player_visible_expected_v2(&annotation.expected)?;
        for action in &annotation.expected.ordered_legal_actions {
            observed_action_families.insert(player_visible_duel_action_family_v1(action));
        }

        let mut visible_state_exact = false;
        let mut visible_legal_actions_exact = false;
        let mut visible_decision_input_exact = false;
        let prediction_commitment = if let Some(prediction) = case.prediction {
            validate_player_visible_prediction_source_v2(&spec, case, prediction)?;
            prediction_count += 1;
            let predicted =
                build_player_visible_duel_decision_input_v1(prediction.validated_decision_v1())?;
            visible_state_exact = predicted.current_state == annotation.expected.current_state;
            visible_legal_actions_exact =
                predicted.ordered_legal_actions == annotation.expected.ordered_legal_actions;
            visible_decision_input_exact = visible_state_exact && visible_legal_actions_exact;
            exact_visible_state_count += u32::from(visible_state_exact);
            exact_visible_legal_action_count += u32::from(visible_legal_actions_exact);
            exact_visible_decision_input_count += u32::from(visible_decision_input_exact);
            prediction.candidate_commitment_sha256()
        } else {
            "abstained"
        };
        let expected_commitment = annotation.expected.commitment_sha256_v1()?;
        for part in [
            case.case_id.as_bytes(),
            case.source_manifest_sha256.as_bytes(),
            case.source_canonical_bgra8_sha256.as_bytes(),
            expected_commitment.as_bytes(),
            prediction_commitment.as_bytes(),
            &[
                u8::from(visible_state_exact),
                u8::from(visible_legal_actions_exact),
                u8::from(visible_decision_input_exact),
            ],
        ] {
            hash_part_v1(&mut hasher, part);
        }
    }

    let unique_case_count = u32::try_from(cases.len()).map_err(|_| {
        error_v1(
            "player_visible_duel_perception_case_count_invalid",
            "case count does not fit u32",
        )
    })?;
    let prediction_coverage_bps = ratio_bps_v1(prediction_count, unique_case_count);
    let missing_action_families = spec
        .required_action_families
        .iter()
        .copied()
        .filter(|family| !observed_action_families.contains(family))
        .collect::<Vec<_>>();
    let passes_declared_gate = unique_case_count >= spec.minimum_unique_cases
        && prediction_coverage_bps >= spec.minimum_prediction_coverage_bps
        && prediction_count != 0
        && exact_visible_state_count == prediction_count
        && exact_visible_legal_action_count == prediction_count
        && exact_visible_decision_input_count == prediction_count
        && missing_action_families.is_empty();

    Ok(
        CheckedUntrustedMtgoPlayerVisibleDuelPerceptionEvaluationV2 {
            perception_profile_commitment_sha256: spec.perception_profile_commitment_sha256,
            evaluation_commitment_sha256: format!("{:x}", hasher.finalize()),
            unique_case_count,
            prediction_count,
            exact_visible_state_count,
            exact_visible_legal_action_count,
            exact_visible_decision_input_count,
            prediction_coverage_bps,
            missing_action_families,
            passes_declared_gate,
        },
    )
}

/// Legacy v1 admission is permanently empty because its annotation can carry
/// full kernel bookkeeping. Use the player-visible v2 path for any future
/// competitive perception ratification.
pub fn admit_ratified_duel_perception_profile_v1(
    profile: CheckedUntrustedMtgoDuelPerceptionRuntimeProfileV1,
    evaluation: CheckedUntrustedMtgoDuelPerceptionEvaluationV1,
) -> Result<AdmittedMtgoDuelPerceptionProfileV1, MtgoContractErrorV1> {
    admit_duel_perception_profile_against_ratification_v1(
        profile,
        evaluation,
        RATIFIED_DUEL_PERCEPTION_EVALUATION_COMMITMENT_V1,
    )
}

/// Admits only the exact player-visible corpus evaluation pinned by the
/// private production trust root. The root is deliberately empty until a real,
/// reviewed acting-player duel corpus and evaluator run exist.
pub fn admit_ratified_player_visible_duel_perception_profile_v2(
    profile: CheckedUntrustedMtgoDuelPerceptionRuntimeProfileV1,
    evaluation: CheckedUntrustedMtgoPlayerVisibleDuelPerceptionEvaluationV2,
) -> Result<AdmittedMtgoDuelPerceptionProfileV1, MtgoContractErrorV1> {
    admit_player_visible_duel_perception_profile_against_ratification_v2(
        profile,
        evaluation,
        RATIFIED_PLAYER_VISIBLE_DUEL_PERCEPTION_EVALUATION_COMMITMENT_V2,
    )
}

fn admit_player_visible_duel_perception_profile_against_ratification_v2(
    profile: CheckedUntrustedMtgoDuelPerceptionRuntimeProfileV1,
    evaluation: CheckedUntrustedMtgoPlayerVisibleDuelPerceptionEvaluationV2,
    ratified_evaluation_commitment: Option<&str>,
) -> Result<AdmittedMtgoDuelPerceptionProfileV1, MtgoContractErrorV1> {
    if evaluation.perception_profile_commitment_sha256 != profile.profile_commitment_sha256() {
        return Err(error_v1(
            "player_visible_duel_perception_evaluation_profile_mismatch",
            "evaluation and runtime profile commitments differ",
        ));
    }
    if !evaluation.passes_declared_gate {
        return Err(error_v1(
            "player_visible_duel_perception_declared_gate_failed",
            evaluation.evaluation_commitment_sha256,
        ));
    }
    let ratified = ratified_evaluation_commitment.ok_or_else(|| {
        error_v1(
            "player_visible_duel_perception_profile_not_ratified",
            "production contains no ratified player-visible duel perception evaluation",
        )
    })?;
    validate_lower_hex_sha256_v1("player_visible_duel_perception_ratification", ratified)?;
    if evaluation.evaluation_commitment_sha256 != ratified {
        return Err(error_v1(
            "player_visible_duel_perception_profile_not_ratified",
            "evaluation does not match the ratified player-visible production commitment",
        ));
    }

    let mut hasher = Sha256::new();
    hasher.update(PLAYER_VISIBLE_DUEL_PERCEPTION_PROFILE_ADMISSION_DOMAIN_V2);
    hash_part_v1(
        &mut hasher,
        evaluation.perception_profile_commitment_sha256.as_bytes(),
    );
    hash_part_v1(
        &mut hasher,
        evaluation.evaluation_commitment_sha256.as_bytes(),
    );
    hash_part_v1(
        &mut hasher,
        b"player_visible_payload_only_profile_identity_no_scoring_or_input",
    );
    Ok(AdmittedMtgoDuelPerceptionProfileV1 {
        profile,
        evaluation_commitment_sha256: evaluation.evaluation_commitment_sha256,
        admission_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

fn admit_duel_perception_profile_against_ratification_v1(
    profile: CheckedUntrustedMtgoDuelPerceptionRuntimeProfileV1,
    evaluation: CheckedUntrustedMtgoDuelPerceptionEvaluationV1,
    ratified_evaluation_commitment: Option<&str>,
) -> Result<AdmittedMtgoDuelPerceptionProfileV1, MtgoContractErrorV1> {
    if evaluation.perception_profile_commitment_sha256 != profile.profile_commitment_sha256() {
        return Err(error_v1(
            "duel_perception_evaluation_profile_mismatch",
            "evaluation and runtime profile commitments differ",
        ));
    }
    if !evaluation.passes_declared_gate {
        return Err(error_v1(
            "duel_perception_declared_gate_failed",
            evaluation.evaluation_commitment_sha256,
        ));
    }
    let ratified = ratified_evaluation_commitment.ok_or_else(|| {
        error_v1(
            "duel_perception_profile_not_ratified",
            "production contains no ratified acting-player duel perception evaluation",
        )
    })?;
    validate_lower_hex_sha256_v1("duel_perception_ratification", ratified)?;
    if evaluation.evaluation_commitment_sha256 != ratified {
        return Err(error_v1(
            "duel_perception_profile_not_ratified",
            "evaluation does not match the ratified production commitment",
        ));
    }

    let mut hasher = Sha256::new();
    hasher.update(DUEL_PERCEPTION_PROFILE_ADMISSION_DOMAIN_V1);
    hash_part_v1(
        &mut hasher,
        evaluation.perception_profile_commitment_sha256.as_bytes(),
    );
    hash_part_v1(
        &mut hasher,
        evaluation.evaluation_commitment_sha256.as_bytes(),
    );
    hash_part_v1(&mut hasher, b"profile_identity_only_no_scoring_or_input");
    Ok(AdmittedMtgoDuelPerceptionProfileV1 {
        profile,
        evaluation_commitment_sha256: evaluation.evaluation_commitment_sha256,
        admission_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

fn validate_spec_v1(spec: &MtgoDuelPerceptionEvaluationSpecV1) -> Result<(), MtgoContractErrorV1> {
    if spec.schema_version != MTGO_DUEL_PERCEPTION_EVALUATION_SCHEMA_V1 {
        return Err(error_v1(
            "duel_perception_evaluation_schema_mismatch",
            spec.schema_version.to_string(),
        ));
    }
    validate_safe_identifier_v1(&spec.evaluation_id, "duel_perception_evaluation_id_invalid")?;
    for (field, value) in [
        (
            "duel_perception_profile_commitment",
            spec.perception_profile_commitment_sha256.as_str(),
        ),
        (
            "duel_perception_corpus_manifest",
            spec.corpus_manifest_sha256.as_str(),
        ),
        (
            "duel_perception_annotation_protocol",
            spec.annotation_protocol_sha256.as_str(),
        ),
        (
            "duel_perception_evaluator_binary",
            spec.evaluator_binary_sha256.as_str(),
        ),
    ] {
        validate_lower_hex_sha256_v1(field, value)?;
    }
    if spec.minimum_unique_cases == 0 || spec.minimum_unique_cases > 100_000 {
        return Err(error_v1(
            "duel_perception_minimum_cases_invalid",
            spec.minimum_unique_cases.to_string(),
        ));
    }
    if !(9_500..=10_000).contains(&spec.minimum_prediction_coverage_bps) {
        return Err(error_v1(
            "duel_perception_coverage_threshold_invalid",
            spec.minimum_prediction_coverage_bps.to_string(),
        ));
    }
    validate_action_families_v1(&spec.required_action_families)
}

fn validate_player_visible_spec_v2(
    spec: &MtgoPlayerVisibleDuelPerceptionEvaluationSpecV2,
) -> Result<(), MtgoContractErrorV1> {
    if spec.schema_version != MTGO_PLAYER_VISIBLE_DUEL_PERCEPTION_EVALUATION_SCHEMA_V2 {
        return Err(error_v1(
            "player_visible_duel_perception_evaluation_schema_mismatch",
            spec.schema_version.to_string(),
        ));
    }
    validate_safe_identifier_v1(
        &spec.evaluation_id,
        "player_visible_duel_perception_evaluation_id_invalid",
    )?;
    for (field, value) in [
        (
            "player_visible_duel_perception_profile_commitment",
            spec.perception_profile_commitment_sha256.as_str(),
        ),
        (
            "player_visible_duel_perception_corpus_manifest",
            spec.corpus_manifest_sha256.as_str(),
        ),
        (
            "player_visible_duel_perception_annotation_protocol",
            spec.annotation_protocol_sha256.as_str(),
        ),
        (
            "player_visible_duel_perception_evaluator_binary",
            spec.evaluator_binary_sha256.as_str(),
        ),
    ] {
        validate_lower_hex_sha256_v1(field, value)?;
    }
    if spec.minimum_unique_cases == 0 || spec.minimum_unique_cases > 100_000 {
        return Err(error_v1(
            "player_visible_duel_perception_minimum_cases_invalid",
            spec.minimum_unique_cases.to_string(),
        ));
    }
    if !(9_500..=10_000).contains(&spec.minimum_prediction_coverage_bps) {
        return Err(error_v1(
            "player_visible_duel_perception_coverage_threshold_invalid",
            spec.minimum_prediction_coverage_bps.to_string(),
        ));
    }
    validate_action_families_v1(&spec.required_action_families)
}

pub(crate) fn validate_player_visible_expected_v2(
    expected: &MtgoPlayerVisibleDuelDecisionInputV1,
) -> Result<(), MtgoContractErrorV1> {
    if expected.current_state.acting_player != MtgoPlayerRelativeRoleV1::SeatedPlayer
        || expected.ordered_legal_actions.is_empty()
        || expected.ordered_legal_actions.len() > 64
    {
        return Err(error_v1(
            "player_visible_duel_perception_expected_invalid",
            "expected input must be a seated-player decision with 1 to 64 visible actions",
        ));
    }
    if expected.ordered_legal_actions.iter().any(|action| {
        player_visible_action_actor_v1(action) != MtgoPlayerRelativeRoleV1::SeatedPlayer
    }) {
        return Err(error_v1(
            "player_visible_duel_perception_expected_actor_mismatch",
            "every expected visible action must belong to the seated player",
        ));
    }
    let _ = expected.commitment_sha256_v1()?;
    Ok(())
}

fn validate_player_visible_prediction_source_v2(
    spec: &MtgoPlayerVisibleDuelPerceptionEvaluationSpecV2,
    case: &MtgoPlayerVisibleDuelPerceptionEvaluationCaseV2<'_>,
    prediction: &CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1,
) -> Result<(), MtgoContractErrorV1> {
    if prediction.perception_profile_commitment_sha256()
        != spec.perception_profile_commitment_sha256
        || prediction.source_manifest_sha256() != case.source_manifest_sha256
        || prediction.source_canonical_bgra8_sha256() != case.source_canonical_bgra8_sha256
    {
        return Err(error_v1(
            "player_visible_duel_perception_prediction_source_mismatch",
            case.case_id.clone(),
        ));
    }
    Ok(())
}

fn player_visible_action_actor_v1(
    action: &MtgoPlayerVisibleDuelActionV1,
) -> MtgoPlayerRelativeRoleV1 {
    match action {
        MtgoPlayerVisibleDuelActionV1::Pass { actor }
        | MtgoPlayerVisibleDuelActionV1::PlayLand { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::CastSpell { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ActivateManaAbility { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ActivateAbility { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::PlotSpell { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseTarget { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseCostTarget { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseCastMode { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseKicker { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseSpellMode { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectOption { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectTarget { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::FinishEffectSelection { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectColor { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectNumber { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectBoolean { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::FinishTargetSelection { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseOptionalCostUse { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseOptionalCostWhich { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseSpellCopyPayment { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseSpellCopyRetarget { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseMadnessCast { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::Discard { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::DeclareAttackers { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::DeclareBlockersForAttacker { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseAttackerInclusion { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseBlockerInclusion { actor, .. }
        | MtgoPlayerVisibleDuelActionV1::OrderTriggers { actor, .. } => *actor,
    }
}

fn player_visible_duel_action_family_v1(
    action: &MtgoPlayerVisibleDuelActionV1,
) -> MtgoDuelActionFamilyV1 {
    match action {
        MtgoPlayerVisibleDuelActionV1::Pass { .. } => MtgoDuelActionFamilyV1::PriorityPass,
        MtgoPlayerVisibleDuelActionV1::PlayLand { .. } => MtgoDuelActionFamilyV1::PlayLand,
        MtgoPlayerVisibleDuelActionV1::CastSpell { .. }
        | MtgoPlayerVisibleDuelActionV1::PlotSpell { .. } => {
            MtgoDuelActionFamilyV1::CastOrPlotSpell
        }
        MtgoPlayerVisibleDuelActionV1::ActivateManaAbility { .. } => {
            MtgoDuelActionFamilyV1::ManaAbility
        }
        MtgoPlayerVisibleDuelActionV1::ActivateAbility { .. } => {
            MtgoDuelActionFamilyV1::NonManaAbility
        }
        MtgoPlayerVisibleDuelActionV1::ChooseTarget { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseCostTarget { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectTarget { .. }
        | MtgoPlayerVisibleDuelActionV1::FinishTargetSelection { .. } => {
            MtgoDuelActionFamilyV1::TargetChoice
        }
        MtgoPlayerVisibleDuelActionV1::ChooseCastMode { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseKicker { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseSpellMode { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseOptionalCostUse { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseOptionalCostWhich { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseSpellCopyPayment { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseSpellCopyRetarget { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseMadnessCast { .. } => {
            MtgoDuelActionFamilyV1::CostOrModeChoice
        }
        MtgoPlayerVisibleDuelActionV1::ChooseEffectOption { .. }
        | MtgoPlayerVisibleDuelActionV1::FinishEffectSelection { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectColor { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectNumber { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseEffectBoolean { .. } => {
            MtgoDuelActionFamilyV1::EffectChoice
        }
        MtgoPlayerVisibleDuelActionV1::Discard { .. } => MtgoDuelActionFamilyV1::Discard,
        MtgoPlayerVisibleDuelActionV1::DeclareAttackers { .. }
        | MtgoPlayerVisibleDuelActionV1::DeclareBlockersForAttacker { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseAttackerInclusion { .. }
        | MtgoPlayerVisibleDuelActionV1::ChooseBlockerInclusion { .. } => {
            MtgoDuelActionFamilyV1::CombatChoice
        }
        MtgoPlayerVisibleDuelActionV1::OrderTriggers { .. } => {
            MtgoDuelActionFamilyV1::TriggerOrdering
        }
    }
}

fn validate_action_families_v1(
    families: &[MtgoDuelActionFamilyV1],
) -> Result<(), MtgoContractErrorV1> {
    if families.is_empty() {
        return Err(error_v1(
            "duel_perception_required_families_empty",
            "at least one action family is required",
        ));
    }
    let mut seen = HashSet::new();
    for family in families {
        if !seen.insert(*family) {
            return Err(error_v1(
                "duel_perception_required_family_duplicate",
                format!("{family:?}"),
            ));
        }
    }
    Ok(())
}

fn validate_game_format_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 64
        || value.trim() != value
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b' ' | b'-' | b'_'))
    {
        return Err(error_v1(
            "duel_perception_game_format_invalid",
            value.to_owned(),
        ));
    }
    Ok(())
}

fn validate_expected_source_v1(
    case: &MtgoDuelPerceptionEvaluationCaseV1<'_>,
) -> Result<(), MtgoContractErrorV1> {
    if case.expected.frames.len() != 1 {
        return Err(error_v1(
            "duel_perception_expected_frame_count",
            case.case_id.clone(),
        ));
    }
    let frame = &case.expected.frames[0];
    if frame.frame_id != case.expected.frame_id
        || frame.sequence != case.source_frame_sequence
        || frame.sha256 != case.source_canonical_bgra8_sha256
    {
        return Err(error_v1(
            "duel_perception_expected_source_mismatch",
            case.case_id.clone(),
        ));
    }
    Ok(())
}

fn validate_prediction_source_v1(
    spec: &MtgoDuelPerceptionEvaluationSpecV1,
    case: &MtgoDuelPerceptionEvaluationCaseV1<'_>,
    prediction: &CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1,
) -> Result<(), MtgoContractErrorV1> {
    if prediction.perception_profile_commitment_sha256()
        != spec.perception_profile_commitment_sha256
        || prediction.source_manifest_sha256() != case.source_manifest_sha256
        || prediction.source_canonical_bgra8_sha256() != case.source_canonical_bgra8_sha256
        || prediction.frame_sequence() != case.source_frame_sequence
    {
        return Err(error_v1(
            "duel_perception_prediction_source_mismatch",
            case.case_id.clone(),
        ));
    }
    Ok(())
}

pub fn duel_action_family_v1(action: &ActionSemanticV1) -> MtgoDuelActionFamilyV1 {
    match action {
        ActionSemanticV1::Pass { .. } => MtgoDuelActionFamilyV1::PriorityPass,
        ActionSemanticV1::PlayLand { .. } => MtgoDuelActionFamilyV1::PlayLand,
        ActionSemanticV1::CastSpell { .. } | ActionSemanticV1::PlotSpell { .. } => {
            MtgoDuelActionFamilyV1::CastOrPlotSpell
        }
        ActionSemanticV1::ActivateManaAbility { .. } => MtgoDuelActionFamilyV1::ManaAbility,
        ActionSemanticV1::ActivateAbility { .. } => MtgoDuelActionFamilyV1::NonManaAbility,
        ActionSemanticV1::ChooseTarget { .. }
        | ActionSemanticV1::ChooseCostTarget { .. }
        | ActionSemanticV1::ChooseEffectTarget { .. }
        | ActionSemanticV1::FinishTargetSelection { .. } => MtgoDuelActionFamilyV1::TargetChoice,
        ActionSemanticV1::ChooseCastMode { .. }
        | ActionSemanticV1::ChooseKicker { .. }
        | ActionSemanticV1::ChooseSpellMode { .. }
        | ActionSemanticV1::ChooseOptionalCostUse { .. }
        | ActionSemanticV1::ChooseOptionalCostWhich { .. }
        | ActionSemanticV1::ChooseSpellCopyPayment { .. }
        | ActionSemanticV1::ChooseSpellCopyRetarget { .. }
        | ActionSemanticV1::ChooseMadnessCast { .. } => MtgoDuelActionFamilyV1::CostOrModeChoice,
        ActionSemanticV1::ChooseEffectOption { .. }
        | ActionSemanticV1::FinishEffectSelection { .. }
        | ActionSemanticV1::ChooseEffectColor { .. }
        | ActionSemanticV1::ChooseEffectNumber { .. }
        | ActionSemanticV1::ChooseEffectBoolean { .. } => MtgoDuelActionFamilyV1::EffectChoice,
        ActionSemanticV1::Discard { .. } => MtgoDuelActionFamilyV1::Discard,
        ActionSemanticV1::DeclareAttackers { .. }
        | ActionSemanticV1::DeclareBlockersForAttacker { .. }
        | ActionSemanticV1::ChooseAttackerInclusion { .. }
        | ActionSemanticV1::ChooseBlockerInclusion { .. } => MtgoDuelActionFamilyV1::CombatChoice,
        ActionSemanticV1::OrderTriggers { .. } => MtgoDuelActionFamilyV1::TriggerOrdering,
        ActionSemanticV1::Ambiguous { .. } => MtgoDuelActionFamilyV1::EffectChoice,
    }
}

fn ratio_bps_v1(numerator: u32, denominator: u32) -> u16 {
    if denominator == 0 {
        return 0;
    }
    u16::try_from((u64::from(numerator) * 10_000) / u64::from(denominator))
        .expect("basis-point ratio is at most 10,000")
}

fn hash_part_v1(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn validate_safe_identifier_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(error_v1(code, value.to_owned()));
    }
    Ok(())
}

fn validate_lower_hex_sha256_v1(
    field: &'static str,
    value: &str,
) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1(
            "duel_perception_digest_invalid",
            format!("{field} must be lowercase SHA-256 hex"),
        ));
    }
    Ok(())
}

fn validate_lower_hex_v1(
    field: &'static str,
    value: &str,
    length: usize,
) -> Result<(), MtgoContractErrorV1> {
    if value.len() != length
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1(
            "duel_perception_digest_invalid",
            format!("{field} must be {length} lowercase hex characters"),
        ));
    }
    Ok(())
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        build_checked_untrusted_acting_player_duel_calibration_corpus_v1,
        checked_untrusted_dxgi_artifact_for_test_v1,
        checked_untrusted_player_visible_duel_annotation_set_for_test_v1,
        complete_acting_player_duel_audit_record_for_test_v1,
        dxgi_observed_decision_record_for_test_v1, local_metadata_commitment_v1,
        payload_leaf_inventory_v1, validate_dxgi_bound_observation_reconstruction_audit_v1,
        MtgoDxgiCaptureRoleV2,
    };

    fn spec_v1(
        profile: &CheckedUntrustedMtgoDuelPerceptionRuntimeProfileV1,
        corpus: &CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1,
    ) -> MtgoDuelPerceptionEvaluationSpecV1 {
        MtgoDuelPerceptionEvaluationSpecV1 {
            schema_version: MTGO_DUEL_PERCEPTION_EVALUATION_SCHEMA_V1,
            evaluation_id: "duel-perception-evaluation-test-v1".to_owned(),
            perception_profile_commitment_sha256: profile.profile_commitment_sha256().to_owned(),
            corpus_manifest_sha256: corpus.canonical_manifest_sha256().to_owned(),
            annotation_protocol_sha256: "c".repeat(64),
            evaluator_binary_sha256: "d".repeat(64),
            minimum_unique_cases: 1,
            minimum_prediction_coverage_bps: 10_000,
            required_action_families: profile.supported_action_families().to_vec(),
        }
    }

    fn player_visible_spec_v2(
        profile: &CheckedUntrustedMtgoDuelPerceptionRuntimeProfileV1,
        corpus: &CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1,
    ) -> MtgoPlayerVisibleDuelPerceptionEvaluationSpecV2 {
        MtgoPlayerVisibleDuelPerceptionEvaluationSpecV2 {
            schema_version: MTGO_PLAYER_VISIBLE_DUEL_PERCEPTION_EVALUATION_SCHEMA_V2,
            evaluation_id: "player-visible-duel-perception-evaluation-test-v2".to_owned(),
            perception_profile_commitment_sha256: profile.profile_commitment_sha256().to_owned(),
            corpus_manifest_sha256: corpus.canonical_manifest_sha256().to_owned(),
            annotation_protocol_sha256:
                crate::mtgo_player_visible_duel_annotation_protocol_sha256_v1(),
            evaluator_binary_sha256: "d".repeat(64),
            minimum_unique_cases: 1,
            minimum_prediction_coverage_bps: 10_000,
            required_action_families: profile.supported_action_families().to_vec(),
        }
    }

    fn fixture_v1(
        profile: &CheckedUntrustedMtgoDuelPerceptionRuntimeProfileV1,
    ) -> (
        CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1,
        CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1,
        MtgoObservedDecisionV1,
    ) {
        let source =
            checked_untrusted_dxgi_artifact_for_test_v1(MtgoDxgiCaptureRoleV2::ActingPlayerDuel);
        let audit = validate_dxgi_bound_observation_reconstruction_audit_v1(
            &source,
            complete_acting_player_duel_audit_record_for_test_v1(&source),
        )
        .unwrap();
        let expected =
            dxgi_observed_decision_record_for_test_v1(&source, audit.source_frame_sequence());
        let prediction = crate::check_untrusted_dxgi_observed_decision_candidate_v1(
            &source,
            &audit,
            profile,
            expected.clone(),
        )
        .unwrap();
        let corpus = build_checked_untrusted_acting_player_duel_calibration_corpus_v1(
            "duel-perception-test-corpus-v1",
            &[&source],
        )
        .unwrap();
        (corpus, prediction, expected)
    }

    fn case_v1<'a>(
        corpus: &CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1,
        prediction: &'a CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1,
        expected: MtgoObservedDecisionV1,
    ) -> MtgoDuelPerceptionEvaluationCaseV1<'a> {
        MtgoDuelPerceptionEvaluationCaseV1 {
            case_id: corpus.manifest_v1().samples[0].sample_id.clone(),
            source_manifest_sha256: prediction.source_manifest_sha256().to_owned(),
            source_canonical_bgra8_sha256: prediction.source_canonical_bgra8_sha256().to_owned(),
            source_frame_sequence: prediction.frame_sequence(),
            expected,
            prediction: Some(prediction),
        }
    }

    fn player_visible_case_v2<'a>(
        corpus: &CheckedUntrustedMtgoActingPlayerDuelCalibrationCorpusV1,
        prediction: &'a CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1,
    ) -> MtgoPlayerVisibleDuelPerceptionEvaluationCaseV2<'a> {
        MtgoPlayerVisibleDuelPerceptionEvaluationCaseV2 {
            case_id: corpus.manifest_v1().samples[0].sample_id.clone(),
            source_manifest_sha256: prediction.source_manifest_sha256().to_owned(),
            source_canonical_bgra8_sha256: prediction.source_canonical_bgra8_sha256().to_owned(),
            prediction: Some(prediction),
        }
    }

    fn refresh_expected_payload_v1(record: &mut MtgoObservedDecisionV1) {
        let leaves = payload_leaf_inventory_v1(&record.payload).unwrap();
        for provenance in &mut record.provenance {
            provenance.value_sha256 = leaves
                .iter()
                .find(|leaf| leaf.json_pointer == provenance.json_pointer)
                .unwrap()
                .value_sha256
                .clone();
        }
        record.local_metadata_sha256 = local_metadata_commitment_v1(&record.payload).unwrap();
    }

    #[test]
    fn runtime_profile_binds_identity_geometry_format_and_assets() {
        let profile = duel_perception_runtime_profile_for_test_v1();
        assert_eq!(profile.executable_sha256(), "a".repeat(64));
        assert_eq!(profile.signer_thumbprint(), "b".repeat(40));
        assert_eq!(profile.signer_subject_sha256(), "c".repeat(64));
        assert_eq!(profile.dpi(), 120);
        assert_eq!(profile.client_size_px().width, 1_550);
        assert_eq!(profile.output_identity_sha256(), "4".repeat(64));
        assert_eq!(profile.game_format(), "Freeform");
        assert_eq!(profile.profile_commitment_sha256().len(), 64);
        assert!(!profile.safe_for_model_scoring());
        assert!(!profile.safe_for_input());

        let mut changed_assets = duel_perception_runtime_profile_payload_for_test_v1();
        changed_assets.classifier_assets_manifest_sha256 = "8".repeat(64);
        let changed = check_untrusted_duel_perception_runtime_profile_v1(changed_assets).unwrap();
        assert_ne!(
            profile.profile_commitment_sha256(),
            changed.profile_commitment_sha256()
        );
    }

    #[test]
    fn malformed_runtime_profile_fails_closed() {
        let mut mutations = Vec::new();
        let mut value = duel_perception_runtime_profile_payload_for_test_v1();
        value.signer_thumbprint = "B".repeat(40);
        mutations.push(value);
        let mut value = duel_perception_runtime_profile_payload_for_test_v1();
        value.dpi = 0;
        mutations.push(value);
        let mut value = duel_perception_runtime_profile_payload_for_test_v1();
        value.client_size_px.width = 0;
        mutations.push(value);
        let mut value = duel_perception_runtime_profile_payload_for_test_v1();
        value.game_format = " Freeform".to_owned();
        mutations.push(value);
        let mut value = duel_perception_runtime_profile_payload_for_test_v1();
        value.canonical_pixel_format = "rgba8".to_owned();
        mutations.push(value);
        let mut value = duel_perception_runtime_profile_payload_for_test_v1();
        value
            .supported_action_families
            .push(MtgoDuelActionFamilyV1::PriorityPass);
        mutations.push(value);

        for mutation in mutations {
            assert!(check_untrusted_duel_perception_runtime_profile_v1(mutation).is_err());
        }
    }

    #[test]
    fn exact_semantics_and_required_families_pass_without_runtime_authority() {
        let profile = duel_perception_runtime_profile_for_test_v1();
        let (corpus, prediction, expected) = fixture_v1(&profile);
        let evaluation = evaluate_untrusted_duel_perception_profile_v1(
            &profile,
            &corpus,
            spec_v1(&profile, &corpus),
            vec![case_v1(&corpus, &prediction, expected)],
        )
        .unwrap();

        assert_eq!(evaluation.unique_case_count(), 1);
        assert_eq!(evaluation.prediction_count(), 1);
        assert_eq!(evaluation.exact_observation_count(), 1);
        assert_eq!(evaluation.exact_legal_action_count(), 1);
        assert_eq!(evaluation.exact_object_binding_count(), 1);
        assert_eq!(evaluation.exact_payload_count(), 1);
        assert_eq!(evaluation.prediction_coverage_bps(), 10_000);
        assert!(evaluation.missing_action_families().is_empty());
        assert!(evaluation.passes_declared_gate());
        assert!(!evaluation.safe_for_model_scoring());
        assert!(!evaluation.safe_for_input());
    }

    #[test]
    fn exact_player_visible_payload_passes_without_runtime_authority() {
        let profile = duel_perception_runtime_profile_for_test_v1();
        let (corpus, prediction, _) = fixture_v1(&profile);
        let expected =
            build_player_visible_duel_decision_input_v1(prediction.validated_decision_v1())
                .unwrap();
        let annotations =
            checked_untrusted_player_visible_duel_annotation_set_for_test_v1(&corpus, expected);
        let evaluation = evaluate_untrusted_player_visible_duel_perception_profile_v2(
            &profile,
            &corpus,
            &annotations,
            player_visible_spec_v2(&profile, &corpus),
            vec![player_visible_case_v2(&corpus, &prediction)],
        )
        .unwrap();

        assert_eq!(evaluation.unique_case_count(), 1);
        assert_eq!(evaluation.prediction_count(), 1);
        assert_eq!(evaluation.exact_visible_state_count(), 1);
        assert_eq!(evaluation.exact_visible_legal_action_count(), 1);
        assert_eq!(evaluation.exact_visible_decision_input_count(), 1);
        assert_eq!(evaluation.prediction_coverage_bps(), 10_000);
        assert!(evaluation.missing_action_families().is_empty());
        assert!(evaluation.passes_declared_gate());
        assert!(!evaluation.safe_for_model_scoring());
        assert!(!evaluation.safe_for_input());
    }

    #[test]
    fn player_visible_mismatch_fails_gate_without_kernel_annotation() {
        let profile = duel_perception_runtime_profile_for_test_v1();
        let (corpus, prediction, _) = fixture_v1(&profile);
        let mut expected =
            build_player_visible_duel_decision_input_v1(prediction.validated_decision_v1())
                .unwrap();
        expected.current_state.life_totals[0] += 1;
        let annotations =
            checked_untrusted_player_visible_duel_annotation_set_for_test_v1(&corpus, expected);
        let evaluation = evaluate_untrusted_player_visible_duel_perception_profile_v2(
            &profile,
            &corpus,
            &annotations,
            player_visible_spec_v2(&profile, &corpus),
            vec![player_visible_case_v2(&corpus, &prediction)],
        )
        .unwrap();

        assert_eq!(evaluation.exact_visible_state_count(), 0);
        assert_eq!(evaluation.exact_visible_legal_action_count(), 1);
        assert_eq!(evaluation.exact_visible_decision_input_count(), 0);
        assert!(!evaluation.passes_declared_gate());
    }

    #[test]
    fn player_visible_annotation_serialization_excludes_kernel_and_capture_metadata() {
        let profile = duel_perception_runtime_profile_for_test_v1();
        let (_, prediction, _) = fixture_v1(&profile);
        let expected =
            build_player_visible_duel_decision_input_v1(prediction.validated_decision_v1())
                .unwrap();
        let serialized = serde_json::to_string(&expected).unwrap();
        for forbidden in [
            "arena_id",
            "card_db_id",
            "zone_change_count",
            "adapter_object_id",
            "engine_context",
            "surface_context",
            "policy_surface_context",
            "frame_id",
            "decision_commitment",
            "source_manifest",
        ] {
            assert!(!serialized.contains(forbidden), "found {forbidden}");
        }
    }

    #[test]
    fn player_visible_corpus_source_substitution_fails_closed() {
        let profile = duel_perception_runtime_profile_for_test_v1();
        let (corpus, prediction, _) = fixture_v1(&profile);
        let expected =
            build_player_visible_duel_decision_input_v1(prediction.validated_decision_v1())
                .unwrap();
        let annotations =
            checked_untrusted_player_visible_duel_annotation_set_for_test_v1(&corpus, expected);
        let mut case = player_visible_case_v2(&corpus, &prediction);
        case.source_manifest_sha256 = "f".repeat(64);
        assert_eq!(
            evaluate_untrusted_player_visible_duel_perception_profile_v2(
                &profile,
                &corpus,
                &annotations,
                player_visible_spec_v2(&profile, &corpus),
                vec![case],
            )
            .err()
            .unwrap()
            .code(),
            "player_visible_duel_perception_evaluation_corpus_source_mismatch"
        );
    }

    #[test]
    fn player_visible_evaluation_binds_the_checked_annotation_commitment() {
        let profile = duel_perception_runtime_profile_for_test_v1();
        let (corpus, prediction, _) = fixture_v1(&profile);
        let expected =
            build_player_visible_duel_decision_input_v1(prediction.validated_decision_v1())
                .unwrap();
        let annotations = checked_untrusted_player_visible_duel_annotation_set_for_test_v1(
            &corpus,
            expected.clone(),
        );
        let first = evaluate_untrusted_player_visible_duel_perception_profile_v2(
            &profile,
            &corpus,
            &annotations,
            player_visible_spec_v2(&profile, &corpus),
            vec![player_visible_case_v2(&corpus, &prediction)],
        )
        .unwrap();

        let mut changed_expected = expected;
        changed_expected.current_state.life_totals[0] += 1;
        let changed_annotations = checked_untrusted_player_visible_duel_annotation_set_for_test_v1(
            &corpus,
            changed_expected,
        );
        let second = evaluate_untrusted_player_visible_duel_perception_profile_v2(
            &profile,
            &corpus,
            &changed_annotations,
            player_visible_spec_v2(&profile, &corpus),
            vec![player_visible_case_v2(&corpus, &prediction)],
        )
        .unwrap();

        assert_ne!(
            first.evaluation_commitment_sha256(),
            second.evaluation_commitment_sha256()
        );
        assert!(first.passes_declared_gate());
        assert!(!second.passes_declared_gate());
    }

    #[test]
    fn semantic_mismatch_and_abstention_fail_declared_gate() {
        let profile = duel_perception_runtime_profile_for_test_v1();
        let (corpus, prediction, mut expected) = fixture_v1(&profile);
        expected.payload.object_bindings[0].adapter_object_id = "manual-label:hand:0".to_owned();
        refresh_expected_payload_v1(&mut expected);
        let mismatch = evaluate_untrusted_duel_perception_profile_v1(
            &profile,
            &corpus,
            spec_v1(&profile, &corpus),
            vec![case_v1(&corpus, &prediction, expected)],
        )
        .unwrap();
        assert_eq!(mismatch.exact_observation_count(), 1);
        assert_eq!(mismatch.exact_legal_action_count(), 1);
        assert_eq!(mismatch.exact_object_binding_count(), 0);
        assert_eq!(mismatch.exact_payload_count(), 0);
        assert!(!mismatch.passes_declared_gate());

        let (corpus, prediction, expected) = fixture_v1(&profile);
        let mut abstained_case = case_v1(&corpus, &prediction, expected);
        abstained_case.prediction = None;
        let abstained = evaluate_untrusted_duel_perception_profile_v1(
            &profile,
            &corpus,
            spec_v1(&profile, &corpus),
            vec![abstained_case],
        )
        .unwrap();
        assert_eq!(abstained.prediction_count(), 0);
        assert_eq!(abstained.prediction_coverage_bps(), 0);
        assert!(!abstained.passes_declared_gate());
    }

    #[test]
    fn profile_source_and_coverage_substitution_fail_closed() {
        let profile = duel_perception_runtime_profile_for_test_v1();
        let (corpus, prediction, expected) = fixture_v1(&profile);
        let mut wrong_profile_spec = spec_v1(&profile, &corpus);
        wrong_profile_spec.perception_profile_commitment_sha256 = "e".repeat(64);
        assert_eq!(
            evaluate_untrusted_duel_perception_profile_v1(
                &profile,
                &corpus,
                wrong_profile_spec,
                vec![case_v1(&corpus, &prediction, expected.clone())],
            )
            .err()
            .unwrap()
            .code(),
            "duel_perception_evaluation_profile_mismatch"
        );

        let mut wrong_corpus_spec = spec_v1(&profile, &corpus);
        wrong_corpus_spec.corpus_manifest_sha256 = "a".repeat(64);
        assert_eq!(
            evaluate_untrusted_duel_perception_profile_v1(
                &profile,
                &corpus,
                wrong_corpus_spec,
                vec![case_v1(&corpus, &prediction, expected.clone())],
            )
            .err()
            .unwrap()
            .code(),
            "duel_perception_evaluation_corpus_mismatch"
        );

        assert_eq!(
            evaluate_untrusted_duel_perception_profile_v1(
                &profile,
                &corpus,
                spec_v1(&profile, &corpus),
                vec![
                    case_v1(&corpus, &prediction, expected.clone()),
                    case_v1(&corpus, &prediction, expected.clone()),
                ],
            )
            .err()
            .unwrap()
            .code(),
            "duel_perception_evaluation_corpus_coverage"
        );

        let mut wrong_source_case = case_v1(&corpus, &prediction, expected.clone());
        wrong_source_case.source_canonical_bgra8_sha256 = "f".repeat(64);
        assert_eq!(
            evaluate_untrusted_duel_perception_profile_v1(
                &profile,
                &corpus,
                spec_v1(&profile, &corpus),
                vec![wrong_source_case],
            )
            .err()
            .unwrap()
            .code(),
            "duel_perception_evaluation_corpus_source_mismatch"
        );

        let mut missing_family_spec = spec_v1(&profile, &corpus);
        missing_family_spec
            .required_action_families
            .push(MtgoDuelActionFamilyV1::CastOrPlotSpell);
        assert_eq!(
            evaluate_untrusted_duel_perception_profile_v1(
                &profile,
                &corpus,
                missing_family_spec,
                vec![case_v1(&corpus, &prediction, expected)],
            )
            .err()
            .unwrap()
            .code(),
            "duel_perception_evaluation_profile_mismatch"
        );
    }

    #[test]
    fn production_ratification_is_empty_and_private_test_path_is_profile_only() {
        let profile = duel_perception_runtime_profile_for_test_v1();
        let (corpus, prediction, expected) = fixture_v1(&profile);
        let evaluation = evaluate_untrusted_duel_perception_profile_v1(
            &profile,
            &corpus,
            spec_v1(&profile, &corpus),
            vec![case_v1(&corpus, &prediction, expected)],
        )
        .unwrap();
        let commitment = evaluation.evaluation_commitment_sha256().to_owned();
        assert_eq!(
            admit_ratified_duel_perception_profile_v1(profile, evaluation)
                .err()
                .unwrap()
                .code(),
            "duel_perception_profile_not_ratified"
        );

        let profile = duel_perception_runtime_profile_for_test_v1();
        let (corpus, prediction, expected) = fixture_v1(&profile);
        let evaluation = evaluate_untrusted_duel_perception_profile_v1(
            &profile,
            &corpus,
            spec_v1(&profile, &corpus),
            vec![case_v1(&corpus, &prediction, expected)],
        )
        .unwrap();
        assert_eq!(evaluation.evaluation_commitment_sha256(), commitment);
        let admitted = admit_duel_perception_profile_against_ratification_v1(
            profile,
            evaluation,
            Some(&commitment),
        )
        .unwrap();
        assert_eq!(
            admitted.scope(),
            MtgoDuelPerceptionProfileScopeV1::ActingPlayerDuelSemanticPerception
        );
        assert_eq!(admitted.perception_profile_commitment_sha256().len(), 64);
        assert_eq!(admitted.evaluation_commitment_sha256(), commitment);
        assert_eq!(admitted.admission_commitment_sha256().len(), 64);
        assert!(!admitted.safe_for_model_scoring());
        assert!(!admitted.safe_for_input());
    }

    #[test]
    fn only_player_visible_v2_has_a_future_competitive_ratification_path() {
        let profile = duel_perception_runtime_profile_for_test_v1();
        let (corpus, prediction, _) = fixture_v1(&profile);
        let expected =
            build_player_visible_duel_decision_input_v1(prediction.validated_decision_v1())
                .unwrap();
        let annotations =
            checked_untrusted_player_visible_duel_annotation_set_for_test_v1(&corpus, expected);
        let evaluation = evaluate_untrusted_player_visible_duel_perception_profile_v2(
            &profile,
            &corpus,
            &annotations,
            player_visible_spec_v2(&profile, &corpus),
            vec![player_visible_case_v2(&corpus, &prediction)],
        )
        .unwrap();
        let commitment = evaluation.evaluation_commitment_sha256().to_owned();
        assert_eq!(
            admit_ratified_player_visible_duel_perception_profile_v2(profile, evaluation)
                .err()
                .unwrap()
                .code(),
            "player_visible_duel_perception_profile_not_ratified"
        );

        let profile = duel_perception_runtime_profile_for_test_v1();
        let (corpus, prediction, _) = fixture_v1(&profile);
        let expected =
            build_player_visible_duel_decision_input_v1(prediction.validated_decision_v1())
                .unwrap();
        let annotations =
            checked_untrusted_player_visible_duel_annotation_set_for_test_v1(&corpus, expected);
        let evaluation = evaluate_untrusted_player_visible_duel_perception_profile_v2(
            &profile,
            &corpus,
            &annotations,
            player_visible_spec_v2(&profile, &corpus),
            vec![player_visible_case_v2(&corpus, &prediction)],
        )
        .unwrap();
        let admitted = admit_player_visible_duel_perception_profile_against_ratification_v2(
            profile,
            evaluation,
            Some(&commitment),
        )
        .unwrap();
        assert_eq!(admitted.evaluation_commitment_sha256(), commitment);
        assert!(!admitted.safe_for_model_scoring());
        assert!(!admitted.safe_for_input());
    }
}
