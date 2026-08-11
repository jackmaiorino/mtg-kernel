use crate::{AdmittedMtgoDuelPerceptionProfileV1, MtgoContractErrorV1};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_COMPETITIVE_PREGAME_EVALUATION_SCHEMA_V1: u32 = 1;
pub const MTGO_COMPETITIVE_PREGAME_CANONICAL_STATE_COUNT_V1: usize = 44;

const COMPETITIVE_PREGAME_EVALUATION_DOMAIN_V1: &[u8] = b"mtgo-competitive-pregame-evaluation-v1";
const COMPETITIVE_PREGAME_ADMISSION_DOMAIN_V1: &[u8] = b"mtgo-competitive-pregame-admission-v1";
pub(crate) const RATIFIED_COMPETITIVE_PREGAME_EVALUATION_COMMITMENT_V1: Option<&str> = None;

/// Mode-independent visible pregame semantics. League or Challenge identity
/// is attached later by the exact paid-event runtime, not inferred from this
/// duel-window classifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoCompetitivePregameStageLabelV1 {
    MulliganChoice {
        prospective_keep_size: u8,
    },
    LondonBottoming {
        required_bottom_count: u8,
        selected_bottom_count: u8,
    },
    GameplayReady,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitivePregameEvaluationSpecV1 {
    pub schema_version: u32,
    pub evaluation_id: String,
    pub duel_perception_profile_commitment_sha256: String,
    pub duel_perception_profile_admission_commitment_sha256: String,
    pub corpus_manifest_sha256: String,
    pub annotation_protocol_sha256: String,
    pub evaluator_binary_sha256: String,
    pub minimum_unique_cases_per_canonical_state: u32,
    pub minimum_prediction_coverage_bps_per_canonical_state: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitivePregameEvaluationCaseV1 {
    pub case_id: String,
    pub source_manifest_sha256: String,
    pub source_canonical_bgra8_sha256: String,
    pub source_frame_sequence: u64,
    pub acting_player_duel_capture_confirmed: bool,
    pub prompt_and_hand_unobscured_confirmed: bool,
    pub expected: MtgoCompetitivePregameStageLabelV1,
    pub prediction: Option<MtgoCompetitivePregameStageLabelV1>,
}

/// Recomputed evaluation over the complete 44-state visible pregame surface:
/// keep sizes zero through seven, every legal London-bottoming count pair,
/// and GameplayReady. It has no capture, scoring, event, or input authority.
pub struct CheckedUntrustedMtgoCompetitivePregameEvaluationV1 {
    duel_perception_profile_commitment_sha256: String,
    duel_perception_profile_admission_commitment_sha256: String,
    evaluation_commitment_sha256: String,
    unique_case_count: u32,
    prediction_count: u32,
    exact_prediction_count: u32,
    covered_canonical_state_count: u8,
    passes_declared_gate: bool,
}

impl CheckedUntrustedMtgoCompetitivePregameEvaluationV1 {
    pub fn duel_perception_profile_commitment_sha256(&self) -> &str {
        &self.duel_perception_profile_commitment_sha256
    }

    pub fn duel_perception_profile_admission_commitment_sha256(&self) -> &str {
        &self.duel_perception_profile_admission_commitment_sha256
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

    pub fn exact_prediction_count(&self) -> u32 {
        self.exact_prediction_count
    }

    pub fn covered_canonical_state_count(&self) -> u8 {
        self.covered_canonical_state_count
    }

    pub fn passes_declared_gate(&self) -> bool {
        self.passes_declared_gate
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoCompetitivePregameProfileScopeV1 {
    ActingPlayerDuelPregameStageSemantics,
}

/// Separately ratified accuracy profile for mode-independent acting-player
/// duel pregame semantics. It contains no pixels, classifier process, card
/// identities, event identity, coordinates, scoring, or input method.
pub struct AdmittedMtgoCompetitivePregameProfileV1 {
    duel_perception_profile_commitment_sha256: String,
    duel_perception_profile_admission_commitment_sha256: String,
    evaluation_commitment_sha256: String,
    admission_commitment_sha256: String,
}

impl AdmittedMtgoCompetitivePregameProfileV1 {
    pub fn scope(&self) -> MtgoCompetitivePregameProfileScopeV1 {
        MtgoCompetitivePregameProfileScopeV1::ActingPlayerDuelPregameStageSemantics
    }

    pub fn duel_perception_profile_commitment_sha256(&self) -> &str {
        &self.duel_perception_profile_commitment_sha256
    }

    pub fn duel_perception_profile_admission_commitment_sha256(&self) -> &str {
        &self.duel_perception_profile_admission_commitment_sha256
    }

    pub fn evaluation_commitment_sha256(&self) -> &str {
        &self.evaluation_commitment_sha256
    }

    pub fn admission_commitment_sha256(&self) -> &str {
        &self.admission_commitment_sha256
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

pub fn evaluate_untrusted_competitive_pregame_profile_v1(
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    spec: MtgoCompetitivePregameEvaluationSpecV1,
    cases: Vec<MtgoCompetitivePregameEvaluationCaseV1>,
) -> Result<CheckedUntrustedMtgoCompetitivePregameEvaluationV1, MtgoContractErrorV1> {
    validate_spec_v1(&spec)?;
    if spec.duel_perception_profile_commitment_sha256
        != profile.perception_profile_commitment_sha256()
        || spec.duel_perception_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
    {
        return Err(error_v1(
            "competitive_pregame_profile_mismatch",
            "evaluation spec does not bind the exact admitted duel perception profile",
        ));
    }
    if cases.is_empty() || cases.len() > 100_000 {
        return Err(error_v1(
            "competitive_pregame_case_count_invalid",
            cases.len().to_string(),
        ));
    }

    let encoded_spec = serde_json::to_vec(&spec).map_err(|error| {
        error_v1(
            "competitive_pregame_spec_serialization_failed",
            error.to_string(),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(COMPETITIVE_PREGAME_EVALUATION_DOMAIN_V1);
    hash_part_v1(&mut hasher, &encoded_spec);

    let canonical_states = canonical_pregame_states_v1();
    let mut case_counts = vec![0_u32; canonical_states.len()];
    let mut prediction_counts = vec![0_u32; canonical_states.len()];
    let mut previous_case_id: Option<&str> = None;
    let mut source_manifests = HashSet::new();
    let mut source_frames = HashSet::new();
    let mut prediction_count = 0_u32;
    let mut exact_prediction_count = 0_u32;

    for case in &cases {
        validate_safe_identifier_v1(&case.case_id)?;
        if previous_case_id.is_some_and(|previous| previous >= case.case_id.as_str()) {
            return Err(error_v1(
                "competitive_pregame_case_order_invalid",
                "case IDs must be unique and strictly increasing",
            ));
        }
        previous_case_id = Some(&case.case_id);
        validate_lower_hex_sha256_v1(&case.source_manifest_sha256)?;
        validate_lower_hex_sha256_v1(&case.source_canonical_bgra8_sha256)?;
        if case.source_frame_sequence == 0
            || !case.acting_player_duel_capture_confirmed
            || !case.prompt_and_hand_unobscured_confirmed
        {
            return Err(error_v1(
                "competitive_pregame_source_review_invalid",
                &case.case_id,
            ));
        }
        if !source_manifests.insert(case.source_manifest_sha256.as_str())
            || !source_frames.insert(case.source_canonical_bgra8_sha256.as_str())
        {
            return Err(error_v1(
                "competitive_pregame_duplicate_source",
                &case.case_id,
            ));
        }
        validate_stage_v1(case.expected)?;
        let state_index = canonical_states
            .iter()
            .position(|state| *state == case.expected)
            .ok_or_else(|| error_v1("competitive_pregame_expected_state_invalid", &case.case_id))?;
        case_counts[state_index] += 1;

        let (prediction_json, exact) = if let Some(prediction) = case.prediction {
            validate_stage_v1(prediction)?;
            prediction_count += 1;
            prediction_counts[state_index] += 1;
            let exact = prediction == case.expected;
            exact_prediction_count += u32::from(exact);
            (
                serde_json::to_vec(&prediction).map_err(|error| {
                    error_v1(
                        "competitive_pregame_prediction_serialization_failed",
                        error.to_string(),
                    )
                })?,
                exact,
            )
        } else {
            (b"abstained".to_vec(), false)
        };
        let expected_json = serde_json::to_vec(&case.expected).map_err(|error| {
            error_v1(
                "competitive_pregame_expected_serialization_failed",
                error.to_string(),
            )
        })?;
        for part in [
            case.case_id.as_bytes(),
            case.source_manifest_sha256.as_bytes(),
            case.source_canonical_bgra8_sha256.as_bytes(),
            &case.source_frame_sequence.to_be_bytes(),
            &expected_json,
            &prediction_json,
            &[u8::from(exact)],
        ] {
            hash_part_v1(&mut hasher, part);
        }
    }

    let covered_canonical_state_count = u8::try_from(
        case_counts
            .iter()
            .filter(|count| **count >= spec.minimum_unique_cases_per_canonical_state)
            .count(),
    )
    .map_err(|_| {
        error_v1(
            "competitive_pregame_state_count_invalid",
            "covered state count does not fit u8",
        )
    })?;
    let all_state_gates_pass =
        case_counts
            .iter()
            .zip(&prediction_counts)
            .all(|(cases, predictions)| {
                *cases >= spec.minimum_unique_cases_per_canonical_state
                    && ratio_bps_v1(*predictions, *cases)
                        >= spec.minimum_prediction_coverage_bps_per_canonical_state
            });
    let passes_declared_gate =
        all_state_gates_pass && prediction_count != 0 && exact_prediction_count == prediction_count;

    Ok(CheckedUntrustedMtgoCompetitivePregameEvaluationV1 {
        duel_perception_profile_commitment_sha256: spec.duel_perception_profile_commitment_sha256,
        duel_perception_profile_admission_commitment_sha256: spec
            .duel_perception_profile_admission_commitment_sha256,
        evaluation_commitment_sha256: format!("{:x}", hasher.finalize()),
        unique_case_count: u32::try_from(cases.len()).map_err(|_| {
            error_v1(
                "competitive_pregame_case_count_invalid",
                "case count does not fit u32",
            )
        })?,
        prediction_count,
        exact_prediction_count,
        covered_canonical_state_count,
        passes_declared_gate,
    })
}

pub fn admit_ratified_competitive_pregame_profile_v1(
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    evaluation: CheckedUntrustedMtgoCompetitivePregameEvaluationV1,
) -> Result<AdmittedMtgoCompetitivePregameProfileV1, MtgoContractErrorV1> {
    admit_competitive_pregame_profile_against_ratification_v1(
        profile,
        evaluation,
        RATIFIED_COMPETITIVE_PREGAME_EVALUATION_COMMITMENT_V1,
    )
}

fn admit_competitive_pregame_profile_against_ratification_v1(
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    evaluation: CheckedUntrustedMtgoCompetitivePregameEvaluationV1,
    ratified_evaluation_commitment: Option<&str>,
) -> Result<AdmittedMtgoCompetitivePregameProfileV1, MtgoContractErrorV1> {
    if evaluation.duel_perception_profile_commitment_sha256
        != profile.perception_profile_commitment_sha256()
        || evaluation.duel_perception_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
    {
        return Err(error_v1(
            "competitive_pregame_profile_mismatch",
            "evaluation and admitted duel perception profile differ",
        ));
    }
    if !evaluation.passes_declared_gate {
        return Err(error_v1(
            "competitive_pregame_declared_gate_failed",
            &evaluation.evaluation_commitment_sha256,
        ));
    }
    let ratified = ratified_evaluation_commitment.ok_or_else(|| {
        error_v1(
            "competitive_pregame_profile_not_ratified",
            "production contains no ratified complete-state pregame evaluation",
        )
    })?;
    validate_lower_hex_sha256_v1(ratified)?;
    if ratified != evaluation.evaluation_commitment_sha256 {
        return Err(error_v1(
            "competitive_pregame_profile_not_ratified",
            "evaluation does not match the production root",
        ));
    }
    let admission_commitment_sha256 = commitment_v1(
        COMPETITIVE_PREGAME_ADMISSION_DOMAIN_V1,
        &[
            evaluation
                .duel_perception_profile_commitment_sha256
                .as_bytes(),
            evaluation
                .duel_perception_profile_admission_commitment_sha256
                .as_bytes(),
            evaluation.evaluation_commitment_sha256.as_bytes(),
            b"complete_acting_player_duel_pregame_semantics_no_mode_inference_no_input",
        ],
    );
    Ok(AdmittedMtgoCompetitivePregameProfileV1 {
        duel_perception_profile_commitment_sha256: evaluation
            .duel_perception_profile_commitment_sha256,
        duel_perception_profile_admission_commitment_sha256: evaluation
            .duel_perception_profile_admission_commitment_sha256,
        evaluation_commitment_sha256: evaluation.evaluation_commitment_sha256,
        admission_commitment_sha256,
    })
}

#[cfg(test)]
pub(crate) fn competitive_pregame_profile_admitted_for_test_v1(
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
) -> AdmittedMtgoCompetitivePregameProfileV1 {
    let evaluation_commitment_sha256 = "c".repeat(64);
    let admission_commitment_sha256 = commitment_v1(
        COMPETITIVE_PREGAME_ADMISSION_DOMAIN_V1,
        &[
            profile.perception_profile_commitment_sha256().as_bytes(),
            profile.admission_commitment_sha256().as_bytes(),
            evaluation_commitment_sha256.as_bytes(),
            b"complete_acting_player_duel_pregame_semantics_no_mode_inference_no_input",
        ],
    );
    AdmittedMtgoCompetitivePregameProfileV1 {
        duel_perception_profile_commitment_sha256: profile
            .perception_profile_commitment_sha256()
            .to_owned(),
        duel_perception_profile_admission_commitment_sha256: profile
            .admission_commitment_sha256()
            .to_owned(),
        evaluation_commitment_sha256,
        admission_commitment_sha256,
    }
}

pub fn canonical_competitive_pregame_states_v1() -> Vec<MtgoCompetitivePregameStageLabelV1> {
    canonical_pregame_states_v1()
}

fn canonical_pregame_states_v1() -> Vec<MtgoCompetitivePregameStageLabelV1> {
    let mut states = Vec::with_capacity(MTGO_COMPETITIVE_PREGAME_CANONICAL_STATE_COUNT_V1);
    for prospective_keep_size in 0..=7 {
        states.push(MtgoCompetitivePregameStageLabelV1::MulliganChoice {
            prospective_keep_size,
        });
    }
    for required_bottom_count in 1..=7 {
        for selected_bottom_count in 0..=required_bottom_count {
            states.push(MtgoCompetitivePregameStageLabelV1::LondonBottoming {
                required_bottom_count,
                selected_bottom_count,
            });
        }
    }
    states.push(MtgoCompetitivePregameStageLabelV1::GameplayReady);
    debug_assert_eq!(
        states.len(),
        MTGO_COMPETITIVE_PREGAME_CANONICAL_STATE_COUNT_V1
    );
    states
}

fn validate_stage_v1(stage: MtgoCompetitivePregameStageLabelV1) -> Result<(), MtgoContractErrorV1> {
    match stage {
        MtgoCompetitivePregameStageLabelV1::MulliganChoice {
            prospective_keep_size,
        } if prospective_keep_size <= 7 => Ok(()),
        MtgoCompetitivePregameStageLabelV1::LondonBottoming {
            required_bottom_count,
            selected_bottom_count,
        } if (1..=7).contains(&required_bottom_count)
            && selected_bottom_count <= required_bottom_count =>
        {
            Ok(())
        }
        MtgoCompetitivePregameStageLabelV1::GameplayReady => Ok(()),
        _ => Err(error_v1(
            "competitive_pregame_stage_invalid",
            "stage is outside the London mulligan state surface",
        )),
    }
}

fn validate_spec_v1(
    spec: &MtgoCompetitivePregameEvaluationSpecV1,
) -> Result<(), MtgoContractErrorV1> {
    if spec.schema_version != MTGO_COMPETITIVE_PREGAME_EVALUATION_SCHEMA_V1 {
        return Err(error_v1(
            "competitive_pregame_schema_mismatch",
            spec.schema_version.to_string(),
        ));
    }
    validate_safe_identifier_v1(&spec.evaluation_id)?;
    for digest in [
        spec.duel_perception_profile_commitment_sha256.as_str(),
        spec.duel_perception_profile_admission_commitment_sha256
            .as_str(),
        spec.corpus_manifest_sha256.as_str(),
        spec.annotation_protocol_sha256.as_str(),
        spec.evaluator_binary_sha256.as_str(),
    ] {
        validate_lower_hex_sha256_v1(digest)?;
    }
    if spec.minimum_unique_cases_per_canonical_state == 0
        || spec.minimum_unique_cases_per_canonical_state > 1_000
        || !(1..=10_000).contains(&spec.minimum_prediction_coverage_bps_per_canonical_state)
    {
        return Err(error_v1(
            "competitive_pregame_gate_invalid",
            "per-state case or coverage gate is invalid",
        ));
    }
    Ok(())
}

fn validate_safe_identifier_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(error_v1("competitive_pregame_identifier_invalid", value));
    }
    Ok(())
}

fn validate_lower_hex_sha256_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1("competitive_pregame_sha256_invalid", value));
    }
    Ok(())
}

fn ratio_bps_v1(numerator: u32, denominator: u32) -> u16 {
    if denominator == 0 {
        return 0;
    }
    u16::try_from((u64::from(numerator) * 10_000) / u64::from(denominator)).unwrap_or(10_000)
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hash_part_v1(&mut hasher, part);
    }
    format!("{:x}", hasher.finalize())
}

fn hash_part_v1(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        duel_perception_profile_admitted_for_test_v1, duel_perception_runtime_profile_for_test_v1,
    };

    fn profile_v1() -> AdmittedMtgoDuelPerceptionProfileV1 {
        duel_perception_profile_admitted_for_test_v1(duel_perception_runtime_profile_for_test_v1())
    }

    fn spec_v1(
        profile: &AdmittedMtgoDuelPerceptionProfileV1,
    ) -> MtgoCompetitivePregameEvaluationSpecV1 {
        MtgoCompetitivePregameEvaluationSpecV1 {
            schema_version: MTGO_COMPETITIVE_PREGAME_EVALUATION_SCHEMA_V1,
            evaluation_id: "competitive-pregame-evaluation-v1".to_owned(),
            duel_perception_profile_commitment_sha256: profile
                .perception_profile_commitment_sha256()
                .to_owned(),
            duel_perception_profile_admission_commitment_sha256: profile
                .admission_commitment_sha256()
                .to_owned(),
            corpus_manifest_sha256: "1".repeat(64),
            annotation_protocol_sha256: "2".repeat(64),
            evaluator_binary_sha256: "3".repeat(64),
            minimum_unique_cases_per_canonical_state: 1,
            minimum_prediction_coverage_bps_per_canonical_state: 10_000,
        }
    }

    fn cases_v1() -> Vec<MtgoCompetitivePregameEvaluationCaseV1> {
        canonical_pregame_states_v1()
            .into_iter()
            .enumerate()
            .map(|(index, state)| MtgoCompetitivePregameEvaluationCaseV1 {
                case_id: format!("case-{index:03}"),
                source_manifest_sha256: format!("{:064x}", index + 1),
                source_canonical_bgra8_sha256: format!("{:064x}", index + 101),
                source_frame_sequence: u64::try_from(index + 1).unwrap(),
                acting_player_duel_capture_confirmed: true,
                prompt_and_hand_unobscured_confirmed: true,
                expected: state,
                prediction: Some(state),
            })
            .collect()
    }

    #[test]
    fn canonical_surface_contains_all_44_states_once() {
        let states = canonical_pregame_states_v1();
        assert_eq!(
            states.len(),
            MTGO_COMPETITIVE_PREGAME_CANONICAL_STATE_COUNT_V1
        );
        assert_eq!(states.iter().copied().collect::<HashSet<_>>().len(), 44);
        assert!(
            states.contains(&MtgoCompetitivePregameStageLabelV1::MulliganChoice {
                prospective_keep_size: 0
            })
        );
        assert!(
            states.contains(&MtgoCompetitivePregameStageLabelV1::LondonBottoming {
                required_bottom_count: 7,
                selected_bottom_count: 7
            })
        );
        assert!(states.contains(&MtgoCompetitivePregameStageLabelV1::GameplayReady));
    }

    #[test]
    fn complete_exact_corpus_passes_but_production_admission_is_empty() {
        let profile = profile_v1();
        let evaluation = evaluate_untrusted_competitive_pregame_profile_v1(
            &profile,
            spec_v1(&profile),
            cases_v1(),
        )
        .unwrap();
        assert_eq!(evaluation.unique_case_count(), 44);
        assert_eq!(evaluation.prediction_count(), 44);
        assert_eq!(evaluation.exact_prediction_count(), 44);
        assert_eq!(evaluation.covered_canonical_state_count(), 44);
        assert!(evaluation.passes_declared_gate());
        assert!(!evaluation.safe_for_live_classification_v1());
        assert!(!evaluation.safe_for_live_input_v1());
        assert!(admit_ratified_competitive_pregame_profile_v1(&profile, evaluation).is_err());
    }

    #[test]
    fn exact_internal_ratification_admits_identity_only() {
        let profile = profile_v1();
        let evaluation = evaluate_untrusted_competitive_pregame_profile_v1(
            &profile,
            spec_v1(&profile),
            cases_v1(),
        )
        .unwrap();
        let ratification = evaluation.evaluation_commitment_sha256().to_owned();
        let admitted = admit_competitive_pregame_profile_against_ratification_v1(
            &profile,
            evaluation,
            Some(&ratification),
        )
        .unwrap();
        assert_eq!(
            admitted.scope(),
            MtgoCompetitivePregameProfileScopeV1::ActingPlayerDuelPregameStageSemantics
        );
        assert!(!admitted.safe_for_live_classification_v1());
        assert!(!admitted.safe_for_live_input_v1());
        assert!(!admitted.permits_event_entry_v1());
    }

    #[test]
    fn missing_abstained_wrong_or_unreviewed_states_fail_closed() {
        let profile = profile_v1();

        let mut missing = cases_v1();
        missing.pop();
        let evaluation =
            evaluate_untrusted_competitive_pregame_profile_v1(&profile, spec_v1(&profile), missing)
                .unwrap();
        assert!(!evaluation.passes_declared_gate());

        let mut abstained = cases_v1();
        abstained[0].prediction = None;
        let evaluation = evaluate_untrusted_competitive_pregame_profile_v1(
            &profile,
            spec_v1(&profile),
            abstained,
        )
        .unwrap();
        assert!(!evaluation.passes_declared_gate());

        let mut wrong = cases_v1();
        wrong[0].prediction = Some(MtgoCompetitivePregameStageLabelV1::GameplayReady);
        let evaluation =
            evaluate_untrusted_competitive_pregame_profile_v1(&profile, spec_v1(&profile), wrong)
                .unwrap();
        assert!(!evaluation.passes_declared_gate());

        let mut unreviewed = cases_v1();
        unreviewed[0].prompt_and_hand_unobscured_confirmed = false;
        assert!(evaluate_untrusted_competitive_pregame_profile_v1(
            &profile,
            spec_v1(&profile),
            unreviewed,
        )
        .is_err());
    }

    #[test]
    fn source_and_profile_substitution_are_rejected() {
        let profile = profile_v1();
        let mut duplicate = cases_v1();
        duplicate[1].source_manifest_sha256 = duplicate[0].source_manifest_sha256.clone();
        assert!(evaluate_untrusted_competitive_pregame_profile_v1(
            &profile,
            spec_v1(&profile),
            duplicate,
        )
        .is_err());

        let mut crossed = spec_v1(&profile);
        crossed.duel_perception_profile_admission_commitment_sha256 = "9".repeat(64);
        assert!(
            evaluate_untrusted_competitive_pregame_profile_v1(&profile, crossed, cases_v1(),)
                .is_err()
        );
    }
}
