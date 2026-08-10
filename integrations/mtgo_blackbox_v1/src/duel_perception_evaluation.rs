use crate::{
    validate_observed_decision_v1, CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1,
    MtgoContractErrorV1, MtgoObservedDecisionV1,
};
use mtg_kernel::rl::ActionSemanticV1;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_DUEL_PERCEPTION_EVALUATION_SCHEMA_V1: u32 = 1;

const DUEL_PERCEPTION_EVALUATION_DOMAIN_V1: &[u8] = b"mtgo-duel-perception-evaluation-v1";
const DUEL_PERCEPTION_PROFILE_ADMISSION_DOMAIN_V1: &[u8] =
    b"mtgo-duel-perception-profile-admission-v1";
const RATIFIED_DUEL_PERCEPTION_EVALUATION_COMMITMENT_V1: Option<&str> = None;

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

/// One expected semantic decision and the optional perception result for the
/// same visible acting-player duel frame.
///
/// The source hashes and expected record are still untrusted corpus inputs.
/// Evaluation recomputes structural validity and semantic equality, but only a
/// separately reviewed production commitment may ratify the result.
pub struct MtgoDuelPerceptionEvaluationCaseV1<'a> {
    pub case_id: String,
    pub source_manifest_sha256: String,
    pub source_canonical_bgra8_sha256: String,
    pub source_frame_sequence: u64,
    pub expected: MtgoObservedDecisionV1,
    pub prediction: Option<&'a CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1>,
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
    perception_profile_commitment_sha256: String,
    evaluation_commitment_sha256: String,
    admission_commitment_sha256: String,
}

impl AdmittedMtgoDuelPerceptionProfileV1 {
    pub fn scope(&self) -> MtgoDuelPerceptionProfileScopeV1 {
        MtgoDuelPerceptionProfileScopeV1::ActingPlayerDuelSemanticPerception
    }

    pub fn perception_profile_commitment_sha256(&self) -> &str {
        &self.perception_profile_commitment_sha256
    }

    pub fn evaluation_commitment_sha256(&self) -> &str {
        &self.evaluation_commitment_sha256
    }

    pub fn admission_commitment_sha256(&self) -> &str {
        &self.admission_commitment_sha256
    }

    pub fn safe_for_model_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

pub fn evaluate_untrusted_duel_perception_profile_v1(
    spec: MtgoDuelPerceptionEvaluationSpecV1,
    cases: Vec<MtgoDuelPerceptionEvaluationCaseV1<'_>>,
) -> Result<CheckedUntrustedMtgoDuelPerceptionEvaluationV1, MtgoContractErrorV1> {
    validate_spec_v1(&spec)?;
    if cases.is_empty() || cases.len() > 100_000 {
        return Err(error_v1(
            "duel_perception_case_count_invalid",
            cases.len().to_string(),
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

    for case in &cases {
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
        if !source_manifests.insert(case.source_manifest_sha256.as_str()) {
            return Err(error_v1(
                "duel_perception_duplicate_source",
                case.source_manifest_sha256.clone(),
            ));
        }

        validate_expected_source_v1(case)?;
        let expected = validate_observed_decision_v1(case.expected.clone())?;
        for action in expected.legal_actions() {
            observed_action_families.insert(action_family_v1(action));
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

/// Admits only the exact corpus evaluation pinned by the private production
/// trust root. The production root is deliberately empty until a real,
/// reviewed acting-player duel corpus and evaluator run exist.
pub fn admit_ratified_duel_perception_profile_v1(
    evaluation: CheckedUntrustedMtgoDuelPerceptionEvaluationV1,
) -> Result<AdmittedMtgoDuelPerceptionProfileV1, MtgoContractErrorV1> {
    admit_duel_perception_profile_against_ratification_v1(
        evaluation,
        RATIFIED_DUEL_PERCEPTION_EVALUATION_COMMITMENT_V1,
    )
}

fn admit_duel_perception_profile_against_ratification_v1(
    evaluation: CheckedUntrustedMtgoDuelPerceptionEvaluationV1,
    ratified_evaluation_commitment: Option<&str>,
) -> Result<AdmittedMtgoDuelPerceptionProfileV1, MtgoContractErrorV1> {
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
        perception_profile_commitment_sha256: evaluation.perception_profile_commitment_sha256,
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
    if spec.required_action_families.is_empty() {
        return Err(error_v1(
            "duel_perception_required_families_empty",
            "at least one action family is required",
        ));
    }
    let mut seen = HashSet::new();
    for family in &spec.required_action_families {
        if !seen.insert(*family) {
            return Err(error_v1(
                "duel_perception_required_family_duplicate",
                format!("{family:?}"),
            ));
        }
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

fn action_family_v1(action: &ActionSemanticV1) -> MtgoDuelActionFamilyV1 {
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

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        checked_untrusted_dxgi_artifact_for_test_v1,
        complete_acting_player_duel_audit_record_for_test_v1,
        dxgi_observed_decision_record_for_test_v1, local_metadata_commitment_v1,
        payload_leaf_inventory_v1, validate_dxgi_bound_observation_reconstruction_audit_v1,
        MtgoDxgiCaptureRoleV2,
    };

    const PROFILE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn spec_v1() -> MtgoDuelPerceptionEvaluationSpecV1 {
        MtgoDuelPerceptionEvaluationSpecV1 {
            schema_version: MTGO_DUEL_PERCEPTION_EVALUATION_SCHEMA_V1,
            evaluation_id: "duel-perception-evaluation-test-v1".to_owned(),
            perception_profile_commitment_sha256: PROFILE.to_owned(),
            corpus_manifest_sha256: "b".repeat(64),
            annotation_protocol_sha256: "c".repeat(64),
            evaluator_binary_sha256: "d".repeat(64),
            minimum_unique_cases: 1,
            minimum_prediction_coverage_bps: 10_000,
            required_action_families: vec![
                MtgoDuelActionFamilyV1::PriorityPass,
                MtgoDuelActionFamilyV1::PlayLand,
            ],
        }
    }

    fn fixture_v1() -> (
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
            PROFILE,
            expected.clone(),
        )
        .unwrap();
        (prediction, expected)
    }

    fn case_v1<'a>(
        prediction: &'a CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1,
        expected: MtgoObservedDecisionV1,
    ) -> MtgoDuelPerceptionEvaluationCaseV1<'a> {
        MtgoDuelPerceptionEvaluationCaseV1 {
            case_id: "case-0001".to_owned(),
            source_manifest_sha256: prediction.source_manifest_sha256().to_owned(),
            source_canonical_bgra8_sha256: prediction.source_canonical_bgra8_sha256().to_owned(),
            source_frame_sequence: prediction.frame_sequence(),
            expected,
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
    fn exact_semantics_and_required_families_pass_without_runtime_authority() {
        let (prediction, expected) = fixture_v1();
        let evaluation = evaluate_untrusted_duel_perception_profile_v1(
            spec_v1(),
            vec![case_v1(&prediction, expected)],
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
    fn semantic_mismatch_and_abstention_fail_declared_gate() {
        let (prediction, mut expected) = fixture_v1();
        expected.payload.object_bindings[0].adapter_object_id = "manual-label:hand:0".to_owned();
        refresh_expected_payload_v1(&mut expected);
        let mismatch = evaluate_untrusted_duel_perception_profile_v1(
            spec_v1(),
            vec![case_v1(&prediction, expected)],
        )
        .unwrap();
        assert_eq!(mismatch.exact_observation_count(), 1);
        assert_eq!(mismatch.exact_legal_action_count(), 1);
        assert_eq!(mismatch.exact_object_binding_count(), 0);
        assert_eq!(mismatch.exact_payload_count(), 0);
        assert!(!mismatch.passes_declared_gate());

        let (prediction, expected) = fixture_v1();
        let mut abstained_case = case_v1(&prediction, expected);
        abstained_case.prediction = None;
        let abstained =
            evaluate_untrusted_duel_perception_profile_v1(spec_v1(), vec![abstained_case]).unwrap();
        assert_eq!(abstained.prediction_count(), 0);
        assert_eq!(abstained.prediction_coverage_bps(), 0);
        assert!(!abstained.passes_declared_gate());
    }

    #[test]
    fn profile_source_and_coverage_substitution_fail_closed() {
        let (prediction, expected) = fixture_v1();
        let mut wrong_profile_spec = spec_v1();
        wrong_profile_spec.perception_profile_commitment_sha256 = "e".repeat(64);
        assert_eq!(
            evaluate_untrusted_duel_perception_profile_v1(
                wrong_profile_spec,
                vec![case_v1(&prediction, expected.clone())],
            )
            .err()
            .unwrap()
            .code(),
            "duel_perception_prediction_source_mismatch"
        );

        let mut wrong_source_case = case_v1(&prediction, expected.clone());
        wrong_source_case.source_canonical_bgra8_sha256 = "f".repeat(64);
        assert_eq!(
            evaluate_untrusted_duel_perception_profile_v1(spec_v1(), vec![wrong_source_case],)
                .err()
                .unwrap()
                .code(),
            "duel_perception_expected_source_mismatch"
        );

        let mut missing_family_spec = spec_v1();
        missing_family_spec
            .required_action_families
            .push(MtgoDuelActionFamilyV1::CastOrPlotSpell);
        let missing = evaluate_untrusted_duel_perception_profile_v1(
            missing_family_spec,
            vec![case_v1(&prediction, expected)],
        )
        .unwrap();
        assert_eq!(
            missing.missing_action_families(),
            &[MtgoDuelActionFamilyV1::CastOrPlotSpell]
        );
        assert!(!missing.passes_declared_gate());
    }

    #[test]
    fn production_ratification_is_empty_and_private_test_path_is_profile_only() {
        let (prediction, expected) = fixture_v1();
        let evaluation = evaluate_untrusted_duel_perception_profile_v1(
            spec_v1(),
            vec![case_v1(&prediction, expected)],
        )
        .unwrap();
        let commitment = evaluation.evaluation_commitment_sha256().to_owned();
        assert_eq!(
            admit_ratified_duel_perception_profile_v1(evaluation)
                .err()
                .unwrap()
                .code(),
            "duel_perception_profile_not_ratified"
        );

        let (prediction, expected) = fixture_v1();
        let evaluation = evaluate_untrusted_duel_perception_profile_v1(
            spec_v1(),
            vec![case_v1(&prediction, expected)],
        )
        .unwrap();
        assert_eq!(evaluation.evaluation_commitment_sha256(), commitment);
        let admitted =
            admit_duel_perception_profile_against_ratification_v1(evaluation, Some(&commitment))
                .unwrap();
        assert_eq!(
            admitted.scope(),
            MtgoDuelPerceptionProfileScopeV1::ActingPlayerDuelSemanticPerception
        );
        assert_eq!(admitted.perception_profile_commitment_sha256(), PROFILE);
        assert_eq!(admitted.evaluation_commitment_sha256(), commitment);
        assert_eq!(admitted.admission_commitment_sha256().len(), 64);
        assert!(!admitted.safe_for_model_scoring());
        assert!(!admitted.safe_for_input());
    }
}
