use crate::{
    AdmittedMtgoCompetitivePregameProfileV1, MtgoCompetitivePregamePlayDrawV1, MtgoContractErrorV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_EVALUATION_SCHEMA_V1: u32 = 1;
pub const MTGO_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_CANONICAL_STATE_COUNT_V1: usize = 8;

const PUBLIC_CONTEXT_EVALUATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-pregame-public-context-evaluation-v1";
const PUBLIC_CONTEXT_ADMISSION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-pregame-public-context-admission-v1";
pub(crate) const RATIFIED_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_EVALUATION_COMMITMENT_V1: Option<
    &str,
> = None;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitivePregamePublicContextLabelV1 {
    pub game_number: u8,
    pub play_draw: MtgoCompetitivePregamePlayDrawV1,
    pub acting_player_games_won: u8,
    pub opponent_games_won: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitivePregamePublicContextEvaluationSpecV1 {
    pub schema_version: u32,
    pub evaluation_id: String,
    pub pregame_evaluation_commitment_sha256: String,
    pub pregame_profile_admission_commitment_sha256: String,
    pub corpus_manifest_sha256: String,
    pub annotation_protocol_sha256: String,
    pub evaluator_binary_sha256: String,
    pub minimum_unique_cases_per_canonical_state: u32,
    pub minimum_prediction_coverage_bps_per_canonical_state: u16,
    pub minimum_exact_accuracy_bps_per_canonical_state: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitivePregamePublicContextEvaluationCaseV1 {
    pub case_id: String,
    pub source_manifest_sha256: String,
    pub source_canonical_bgra8_sha256: String,
    pub source_frame_sequence: u64,
    pub acting_player_duel_capture_confirmed: bool,
    pub public_context_unobscured_confirmed: bool,
    pub play_draw_and_match_score_reviewed: bool,
    pub pregame_classification_commitment_sha256: String,
    pub visible_interaction_commitment_sha256: String,
    pub expected: MtgoCompetitivePregamePublicContextLabelV1,
    pub prediction: Option<MtgoCompetitivePregamePublicContextLabelV1>,
    pub expected_public_context_commitment_sha256: String,
    pub predicted_public_context_commitment_sha256: Option<String>,
}

pub struct CheckedUntrustedMtgoCompetitivePregamePublicContextEvaluationV1 {
    pregame_evaluation_commitment_sha256: String,
    pregame_profile_admission_commitment_sha256: String,
    evaluation_commitment_sha256: String,
    unique_case_count: u32,
    prediction_count: u32,
    exact_prediction_count: u32,
    passes_declared_gate: bool,
}

impl CheckedUntrustedMtgoCompetitivePregamePublicContextEvaluationV1 {
    pub fn pregame_evaluation_commitment_sha256(&self) -> &str {
        &self.pregame_evaluation_commitment_sha256
    }

    pub fn pregame_profile_admission_commitment_sha256(&self) -> &str {
        &self.pregame_profile_admission_commitment_sha256
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

    pub fn passes_declared_gate(&self) -> bool {
        self.passes_declared_gate
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoCompetitivePregamePublicContextProfileScopeV1 {
    ActingPlayerDuelPlayDrawAndBestOfThreeScore,
}

pub struct AdmittedMtgoCompetitivePregamePublicContextProfileV1 {
    pregame_evaluation_commitment_sha256: String,
    pregame_profile_admission_commitment_sha256: String,
    evaluation_commitment_sha256: String,
    admission_commitment_sha256: String,
}

impl AdmittedMtgoCompetitivePregamePublicContextProfileV1 {
    pub fn scope(&self) -> MtgoCompetitivePregamePublicContextProfileScopeV1 {
        MtgoCompetitivePregamePublicContextProfileScopeV1::ActingPlayerDuelPlayDrawAndBestOfThreeScore
    }

    pub fn pregame_evaluation_commitment_sha256(&self) -> &str {
        &self.pregame_evaluation_commitment_sha256
    }

    pub fn pregame_profile_admission_commitment_sha256(&self) -> &str {
        &self.pregame_profile_admission_commitment_sha256
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

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

pub fn canonical_competitive_pregame_public_context_states_v1(
) -> Vec<MtgoCompetitivePregamePublicContextLabelV1> {
    let mut states =
        Vec::with_capacity(MTGO_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_CANONICAL_STATE_COUNT_V1);
    for play_draw in [
        MtgoCompetitivePregamePlayDrawV1::OnPlay,
        MtgoCompetitivePregamePlayDrawV1::OnDraw,
    ] {
        states.push(MtgoCompetitivePregamePublicContextLabelV1 {
            game_number: 1,
            play_draw,
            acting_player_games_won: 0,
            opponent_games_won: 0,
        });
        states.push(MtgoCompetitivePregamePublicContextLabelV1 {
            game_number: 2,
            play_draw,
            acting_player_games_won: 1,
            opponent_games_won: 0,
        });
        states.push(MtgoCompetitivePregamePublicContextLabelV1 {
            game_number: 2,
            play_draw,
            acting_player_games_won: 0,
            opponent_games_won: 1,
        });
        states.push(MtgoCompetitivePregamePublicContextLabelV1 {
            game_number: 3,
            play_draw,
            acting_player_games_won: 1,
            opponent_games_won: 1,
        });
    }
    states
}

pub fn evaluate_untrusted_competitive_pregame_public_context_profile_v1(
    pregame_profile: &AdmittedMtgoCompetitivePregameProfileV1,
    spec: MtgoCompetitivePregamePublicContextEvaluationSpecV1,
    cases: Vec<MtgoCompetitivePregamePublicContextEvaluationCaseV1>,
) -> Result<CheckedUntrustedMtgoCompetitivePregamePublicContextEvaluationV1, MtgoContractErrorV1> {
    validate_spec_v1(&spec)?;
    if spec.pregame_evaluation_commitment_sha256 != pregame_profile.evaluation_commitment_sha256()
        || spec.pregame_profile_admission_commitment_sha256
            != pregame_profile.admission_commitment_sha256()
    {
        return Err(error_v1(
            "competitive_pregame_public_context_profile_mismatch",
            "evaluation does not bind the exact admitted pregame profile",
        ));
    }
    if cases.is_empty() || cases.len() > 100_000 {
        return Err(error_v1(
            "competitive_pregame_public_context_case_count_invalid",
            cases.len().to_string(),
        ));
    }

    let canonical_states = canonical_competitive_pregame_public_context_states_v1();
    let mut case_counts = vec![0_u32; canonical_states.len()];
    let mut prediction_counts = vec![0_u32; canonical_states.len()];
    let mut exact_counts = vec![0_u32; canonical_states.len()];
    let mut prior_case_id: Option<&str> = None;
    let mut source_manifests = HashSet::new();
    let mut source_frames = HashSet::new();
    let mut prediction_count = 0_u32;
    let mut exact_prediction_count = 0_u32;

    for case in &cases {
        validate_case_source_v1(case)?;
        if prior_case_id.is_some_and(|prior| prior >= case.case_id.as_str()) {
            return Err(error_v1(
                "competitive_pregame_public_context_case_order_invalid",
                "case IDs must be unique and strictly increasing",
            ));
        }
        prior_case_id = Some(&case.case_id);
        if !source_manifests.insert(case.source_manifest_sha256.as_str())
            || !source_frames.insert(case.source_canonical_bgra8_sha256.as_str())
        {
            return Err(error_v1(
                "competitive_pregame_public_context_duplicate_source",
                &case.case_id,
            ));
        }
        let state_index = canonical_states
            .iter()
            .position(|state| state == &case.expected)
            .ok_or_else(|| {
                error_v1(
                    "competitive_pregame_public_context_expected_state_invalid",
                    &case.case_id,
                )
            })?;
        case_counts[state_index] += 1;
        if case.prediction.is_some() != case.predicted_public_context_commitment_sha256.is_some() {
            return Err(error_v1(
                "competitive_pregame_public_context_prediction_incomplete",
                &case.case_id,
            ));
        }
        if let (Some(prediction), Some(predicted_commitment)) = (
            case.prediction,
            case.predicted_public_context_commitment_sha256.as_deref(),
        ) {
            if !canonical_states.contains(&prediction) {
                return Err(error_v1(
                    "competitive_pregame_public_context_prediction_invalid",
                    &case.case_id,
                ));
            }
            prediction_counts[state_index] += 1;
            prediction_count += 1;
            if prediction == case.expected
                && predicted_commitment == case.expected_public_context_commitment_sha256
            {
                exact_counts[state_index] += 1;
                exact_prediction_count += 1;
            }
        }
    }

    let passes_declared_gate = canonical_states.iter().enumerate().all(|(index, _)| {
        let cases = case_counts[index];
        let predictions = prediction_counts[index];
        let exact = exact_counts[index];
        cases >= spec.minimum_unique_cases_per_canonical_state
            && ratio_bps_v1(predictions, cases)
                >= spec.minimum_prediction_coverage_bps_per_canonical_state
            && ratio_bps_v1(exact, predictions)
                >= spec.minimum_exact_accuracy_bps_per_canonical_state
    });
    if !passes_declared_gate {
        return Err(error_v1(
            "competitive_pregame_public_context_evaluation_gate_failed",
            "one or more canonical states missed the declared source, coverage, or accuracy floor",
        ));
    }

    let spec_json = serde_json::to_vec(&spec).map_err(|error| {
        error_v1(
            "competitive_pregame_public_context_evaluation_serialization_failed",
            error.to_string(),
        )
    })?;
    let cases_json = serde_json::to_vec(&cases).map_err(|error| {
        error_v1(
            "competitive_pregame_public_context_evaluation_serialization_failed",
            error.to_string(),
        )
    })?;
    let evaluation_commitment_sha256 = commitment_v1(
        PUBLIC_CONTEXT_EVALUATION_DOMAIN_V1,
        &[
            &spec_json,
            &cases_json,
            &(cases.len() as u64).to_be_bytes(),
            &u64::from(prediction_count).to_be_bytes(),
            &u64::from(exact_prediction_count).to_be_bytes(),
            b"all_eight_best_of_three_play_draw_states_exact_per_state_gate",
        ],
    );
    Ok(
        CheckedUntrustedMtgoCompetitivePregamePublicContextEvaluationV1 {
            pregame_evaluation_commitment_sha256: spec.pregame_evaluation_commitment_sha256,
            pregame_profile_admission_commitment_sha256: spec
                .pregame_profile_admission_commitment_sha256,
            evaluation_commitment_sha256,
            unique_case_count: u32::try_from(cases.len()).map_err(|_| {
                error_v1(
                    "competitive_pregame_public_context_case_count_invalid",
                    "case count overflow",
                )
            })?,
            prediction_count,
            exact_prediction_count,
            passes_declared_gate,
        },
    )
}

pub fn admit_ratified_competitive_pregame_public_context_profile_v1(
    checked: CheckedUntrustedMtgoCompetitivePregamePublicContextEvaluationV1,
) -> Result<AdmittedMtgoCompetitivePregamePublicContextProfileV1, MtgoContractErrorV1> {
    admit_against_ratification_v1(
        checked,
        RATIFIED_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_EVALUATION_COMMITMENT_V1,
    )
}

fn admit_against_ratification_v1(
    checked: CheckedUntrustedMtgoCompetitivePregamePublicContextEvaluationV1,
    ratified: Option<&str>,
) -> Result<AdmittedMtgoCompetitivePregamePublicContextProfileV1, MtgoContractErrorV1> {
    if !checked.passes_declared_gate {
        return Err(error_v1(
            "competitive_pregame_public_context_evaluation_not_passing",
            "evaluation did not pass its declared gate",
        ));
    }
    let ratified = ratified.ok_or_else(|| {
        error_v1(
            "competitive_pregame_public_context_evaluation_not_ratified",
            "production evaluation root is empty",
        )
    })?;
    validate_lower_sha256_v1(ratified)?;
    if ratified != checked.evaluation_commitment_sha256 {
        return Err(error_v1(
            "competitive_pregame_public_context_evaluation_ratification_mismatch",
            "evaluated corpus differs from the production root",
        ));
    }
    let admission_commitment_sha256 = commitment_v1(
        PUBLIC_CONTEXT_ADMISSION_DOMAIN_V1,
        &[
            checked.evaluation_commitment_sha256.as_bytes(),
            checked.pregame_evaluation_commitment_sha256.as_bytes(),
            checked
                .pregame_profile_admission_commitment_sha256
                .as_bytes(),
            b"identity_only_no_pixels_model_input_or_live_authority",
        ],
    );
    Ok(AdmittedMtgoCompetitivePregamePublicContextProfileV1 {
        pregame_evaluation_commitment_sha256: checked.pregame_evaluation_commitment_sha256,
        pregame_profile_admission_commitment_sha256: checked
            .pregame_profile_admission_commitment_sha256,
        evaluation_commitment_sha256: checked.evaluation_commitment_sha256,
        admission_commitment_sha256,
    })
}

fn validate_spec_v1(
    spec: &MtgoCompetitivePregamePublicContextEvaluationSpecV1,
) -> Result<(), MtgoContractErrorV1> {
    if spec.schema_version != MTGO_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_EVALUATION_SCHEMA_V1
        || !valid_identifier_v1(&spec.evaluation_id)
        || spec.minimum_unique_cases_per_canonical_state == 0
        || spec.minimum_prediction_coverage_bps_per_canonical_state != 10_000
        || spec.minimum_exact_accuracy_bps_per_canonical_state != 10_000
    {
        return Err(error_v1(
            "competitive_pregame_public_context_evaluation_spec_invalid",
            "spec requires a safe ID, positive per-state sources, and exact coverage and accuracy",
        ));
    }
    for digest in [
        spec.pregame_evaluation_commitment_sha256.as_str(),
        spec.pregame_profile_admission_commitment_sha256.as_str(),
        spec.corpus_manifest_sha256.as_str(),
        spec.annotation_protocol_sha256.as_str(),
        spec.evaluator_binary_sha256.as_str(),
    ] {
        validate_lower_sha256_v1(digest)?;
    }
    Ok(())
}

fn validate_case_source_v1(
    case: &MtgoCompetitivePregamePublicContextEvaluationCaseV1,
) -> Result<(), MtgoContractErrorV1> {
    if !valid_identifier_v1(&case.case_id)
        || case.source_frame_sequence == 0
        || !case.acting_player_duel_capture_confirmed
        || !case.public_context_unobscured_confirmed
        || !case.play_draw_and_match_score_reviewed
    {
        return Err(error_v1(
            "competitive_pregame_public_context_source_review_invalid",
            &case.case_id,
        ));
    }
    for digest in [
        case.source_manifest_sha256.as_str(),
        case.source_canonical_bgra8_sha256.as_str(),
        case.pregame_classification_commitment_sha256.as_str(),
        case.visible_interaction_commitment_sha256.as_str(),
        case.expected_public_context_commitment_sha256.as_str(),
    ] {
        validate_lower_sha256_v1(digest)?;
    }
    if let Some(digest) = &case.predicted_public_context_commitment_sha256 {
        validate_lower_sha256_v1(digest)?;
    }
    Ok(())
}

fn ratio_bps_v1(numerator: u32, denominator: u32) -> u16 {
    if denominator == 0 {
        return 0;
    }
    u16::try_from((u64::from(numerator) * 10_000) / u64::from(denominator)).unwrap_or(0)
}

fn valid_identifier_v1(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn validate_lower_sha256_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(error_v1(
            "competitive_pregame_public_context_evaluation_digest_invalid",
            value,
        ));
    }
    Ok(())
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        competitive_pregame_profile_admitted_for_test_v1,
        duel_perception_profile_admitted_for_test_v1,
        duel_perception_runtime_profile_payload_for_test_v1,
    };

    fn profile_v1() -> AdmittedMtgoCompetitivePregameProfileV1 {
        let payload = duel_perception_runtime_profile_payload_for_test_v1();
        let duel = duel_perception_profile_admitted_for_test_v1(
            crate::check_untrusted_duel_perception_runtime_profile_v1(payload).unwrap(),
        );
        competitive_pregame_profile_admitted_for_test_v1(&duel)
    }

    fn spec_v1(
        profile: &AdmittedMtgoCompetitivePregameProfileV1,
    ) -> MtgoCompetitivePregamePublicContextEvaluationSpecV1 {
        MtgoCompetitivePregamePublicContextEvaluationSpecV1 {
            schema_version: MTGO_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_EVALUATION_SCHEMA_V1,
            evaluation_id: "synthetic-public-context-v1".to_owned(),
            pregame_evaluation_commitment_sha256: profile.evaluation_commitment_sha256().to_owned(),
            pregame_profile_admission_commitment_sha256: profile
                .admission_commitment_sha256()
                .to_owned(),
            corpus_manifest_sha256: "a".repeat(64),
            annotation_protocol_sha256: "b".repeat(64),
            evaluator_binary_sha256: "c".repeat(64),
            minimum_unique_cases_per_canonical_state: 1,
            minimum_prediction_coverage_bps_per_canonical_state: 10_000,
            minimum_exact_accuracy_bps_per_canonical_state: 10_000,
        }
    }

    fn cases_v1() -> Vec<MtgoCompetitivePregamePublicContextEvaluationCaseV1> {
        canonical_competitive_pregame_public_context_states_v1()
            .into_iter()
            .enumerate()
            .map(|(index, label)| {
                let context = format!("{:064x}", index + 20);
                MtgoCompetitivePregamePublicContextEvaluationCaseV1 {
                    case_id: format!("case-{index:02}"),
                    source_manifest_sha256: format!("{:064x}", index + 1),
                    source_canonical_bgra8_sha256: format!("{:064x}", index + 10),
                    source_frame_sequence: index as u64 + 1,
                    acting_player_duel_capture_confirmed: true,
                    public_context_unobscured_confirmed: true,
                    play_draw_and_match_score_reviewed: true,
                    pregame_classification_commitment_sha256: format!("{:064x}", index + 30),
                    visible_interaction_commitment_sha256: format!("{:064x}", index + 40),
                    expected: label,
                    prediction: Some(label),
                    expected_public_context_commitment_sha256: context.clone(),
                    predicted_public_context_commitment_sha256: Some(context),
                }
            })
            .collect()
    }

    #[test]
    fn canonical_surface_has_all_eight_best_of_three_play_draw_states() {
        let states = canonical_competitive_pregame_public_context_states_v1();
        assert_eq!(states.len(), 8);
        assert_eq!(states.iter().copied().collect::<HashSet<_>>().len(), 8);
    }

    #[test]
    fn exact_corpus_passes_but_production_admission_is_empty() {
        let profile = profile_v1();
        let checked = evaluate_untrusted_competitive_pregame_public_context_profile_v1(
            &profile,
            spec_v1(&profile),
            cases_v1(),
        )
        .unwrap();
        assert_eq!(checked.unique_case_count(), 8);
        assert_eq!(checked.prediction_count(), 8);
        assert_eq!(checked.exact_prediction_count(), 8);
        assert!(checked.passes_declared_gate());
        assert!(!checked.safe_for_live_classification_v1());
        assert!(!checked.safe_for_model_scoring_v1());
        assert!(admit_ratified_competitive_pregame_public_context_profile_v1(checked).is_err());
    }

    #[test]
    fn missing_abstained_wrong_or_unreviewed_state_fails_closed() {
        let profile = profile_v1();
        let spec = spec_v1(&profile);

        let mut missing = cases_v1();
        missing.pop();
        assert!(
            evaluate_untrusted_competitive_pregame_public_context_profile_v1(
                &profile,
                spec.clone(),
                missing,
            )
            .is_err()
        );

        let mut abstained = cases_v1();
        abstained[0].prediction = None;
        abstained[0].predicted_public_context_commitment_sha256 = None;
        assert!(
            evaluate_untrusted_competitive_pregame_public_context_profile_v1(
                &profile,
                spec.clone(),
                abstained,
            )
            .is_err()
        );

        let mut wrong = cases_v1();
        wrong[0].prediction = Some(wrong[1].expected);
        assert!(
            evaluate_untrusted_competitive_pregame_public_context_profile_v1(
                &profile,
                spec.clone(),
                wrong,
            )
            .is_err()
        );

        let mut unreviewed = cases_v1();
        unreviewed[0].play_draw_and_match_score_reviewed = false;
        assert!(
            evaluate_untrusted_competitive_pregame_public_context_profile_v1(
                &profile, spec, unreviewed,
            )
            .is_err()
        );
    }

    #[test]
    fn spec_or_profile_substitution_fails_closed() {
        let profile = profile_v1();

        let mut weak_coverage = spec_v1(&profile);
        weak_coverage.minimum_prediction_coverage_bps_per_canonical_state = 9_999;
        assert!(
            evaluate_untrusted_competitive_pregame_public_context_profile_v1(
                &profile,
                weak_coverage,
                cases_v1(),
            )
            .is_err()
        );

        let mut substituted_profile = spec_v1(&profile);
        substituted_profile.pregame_profile_admission_commitment_sha256 = "d".repeat(64);
        assert!(
            evaluate_untrusted_competitive_pregame_public_context_profile_v1(
                &profile,
                substituted_profile,
                cases_v1(),
            )
            .is_err()
        );
    }

    #[test]
    fn duplicate_reordered_or_noncanonical_sources_fail_closed() {
        let profile = profile_v1();
        let spec = spec_v1(&profile);

        let mut duplicate = cases_v1();
        duplicate[1].source_manifest_sha256 = duplicate[0].source_manifest_sha256.clone();
        assert!(
            evaluate_untrusted_competitive_pregame_public_context_profile_v1(
                &profile,
                spec.clone(),
                duplicate,
            )
            .is_err()
        );

        let mut reordered = cases_v1();
        reordered.swap(0, 1);
        assert!(
            evaluate_untrusted_competitive_pregame_public_context_profile_v1(
                &profile,
                spec.clone(),
                reordered,
            )
            .is_err()
        );

        let mut noncanonical = cases_v1();
        noncanonical[0].expected.game_number = 4;
        assert!(
            evaluate_untrusted_competitive_pregame_public_context_profile_v1(
                &profile,
                spec,
                noncanonical,
            )
            .is_err()
        );
    }

    #[test]
    fn prediction_label_and_commitment_are_one_complete_identity() {
        let profile = profile_v1();
        let spec = spec_v1(&profile);

        let mut missing_commitment = cases_v1();
        missing_commitment[0].predicted_public_context_commitment_sha256 = None;
        assert!(
            evaluate_untrusted_competitive_pregame_public_context_profile_v1(
                &profile,
                spec.clone(),
                missing_commitment,
            )
            .is_err()
        );

        let mut wrong_commitment = cases_v1();
        wrong_commitment[0].predicted_public_context_commitment_sha256 = Some("f".repeat(64));
        assert!(
            evaluate_untrusted_competitive_pregame_public_context_profile_v1(
                &profile,
                spec,
                wrong_commitment,
            )
            .is_err()
        );
    }

    #[test]
    fn internal_exact_ratification_is_identity_only() {
        let profile = profile_v1();
        let checked = evaluate_untrusted_competitive_pregame_public_context_profile_v1(
            &profile,
            spec_v1(&profile),
            cases_v1(),
        )
        .unwrap();
        let ratified = checked.evaluation_commitment_sha256().to_owned();
        let admitted = admit_against_ratification_v1(checked, Some(&ratified)).unwrap();
        assert_eq!(
            admitted.scope(),
            MtgoCompetitivePregamePublicContextProfileScopeV1::ActingPlayerDuelPlayDrawAndBestOfThreeScore
        );
        assert!(!admitted.safe_for_live_classification_v1());
        assert!(!admitted.safe_for_model_scoring_v1());
        assert!(!admitted.safe_for_input_v1());
    }
}
