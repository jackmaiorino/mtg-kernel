use crate::{
    validate_visible_competitive_lifecycle_snapshot_v1, AdmittedMtgoDuelPerceptionProfileV1,
    MtgoCompetitiveEventKindV1, MtgoCompetitiveLifecyclePhaseV1, MtgoContractErrorV1,
    MtgoVisibleCompetitiveLifecycleSnapshotV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_COMPETITIVE_DUEL_LIFECYCLE_EVALUATION_SCHEMA_V1: u32 = 1;

const COMPETITIVE_DUEL_LIFECYCLE_EVALUATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-duel-lifecycle-evaluation-v1";
const COMPETITIVE_DUEL_LIFECYCLE_ADMISSION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-duel-lifecycle-admission-v1";
const RATIFIED_COMPETITIVE_DUEL_LIFECYCLE_EVALUATION_COMMITMENT_V1: Option<&str> = None;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveDuelLifecycleEvaluationSpecV1 {
    pub schema_version: u32,
    pub evaluation_id: String,
    pub duel_perception_profile_commitment_sha256: String,
    pub duel_perception_profile_admission_commitment_sha256: String,
    pub corpus_manifest_sha256: String,
    pub annotation_protocol_sha256: String,
    pub evaluator_binary_sha256: String,
    pub minimum_unique_cases_per_mode: u32,
    pub minimum_prediction_coverage_bps: u16,
    pub required_event_kinds: Vec<MtgoCompetitiveEventKindV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveDuelLifecycleEvaluationCaseV1 {
    pub case_id: String,
    pub source_manifest_sha256: String,
    pub source_canonical_bgra8_sha256: String,
    pub source_frame_sequence: u64,
    pub expected: MtgoVisibleCompetitiveLifecycleSnapshotV1,
    pub prediction: Option<MtgoVisibleCompetitiveLifecycleSnapshotV1>,
}

/// Recomputed League and Challenge Match-in-Progress lifecycle measurements.
/// This checked value has no capture, launch, event-entry, scoring, or input
/// authority. Production admission requires a separate compile-pinned root.
pub struct CheckedUntrustedMtgoCompetitiveDuelLifecycleEvaluationV1 {
    duel_perception_profile_commitment_sha256: String,
    duel_perception_profile_admission_commitment_sha256: String,
    evaluation_commitment_sha256: String,
    unique_case_count: u32,
    league_case_count: u32,
    challenge_case_count: u32,
    prediction_count: u32,
    exact_snapshot_count: u32,
    prediction_coverage_bps: u16,
    league_prediction_coverage_bps: u16,
    challenge_prediction_coverage_bps: u16,
    passes_declared_gate: bool,
}

impl CheckedUntrustedMtgoCompetitiveDuelLifecycleEvaluationV1 {
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

    pub fn league_case_count(&self) -> u32 {
        self.league_case_count
    }

    pub fn challenge_case_count(&self) -> u32 {
        self.challenge_case_count
    }

    pub fn prediction_count(&self) -> u32 {
        self.prediction_count
    }

    pub fn exact_snapshot_count(&self) -> u32 {
        self.exact_snapshot_count
    }

    pub fn prediction_coverage_bps(&self) -> u16 {
        self.prediction_coverage_bps
    }

    pub fn league_prediction_coverage_bps(&self) -> u16 {
        self.league_prediction_coverage_bps
    }

    pub fn challenge_prediction_coverage_bps(&self) -> u16 {
        self.challenge_prediction_coverage_bps
    }

    pub fn passes_declared_gate(&self) -> bool {
        self.passes_declared_gate
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoCompetitiveDuelLifecycleProfileScopeV1 {
    LeagueAndChallengeMatchInProgressLifecycle,
}

/// A separately ratified accuracy gate for the competitive lifecycle emitted
/// by the exact admitted duel perception profile. It grants no input or event
/// entry authority and contains no pixels or lifecycle labels.
pub struct AdmittedMtgoCompetitiveDuelLifecycleProfileV1 {
    duel_perception_profile_commitment_sha256: String,
    duel_perception_profile_admission_commitment_sha256: String,
    evaluation_commitment_sha256: String,
    admission_commitment_sha256: String,
}

impl AdmittedMtgoCompetitiveDuelLifecycleProfileV1 {
    pub fn scope(&self) -> MtgoCompetitiveDuelLifecycleProfileScopeV1 {
        MtgoCompetitiveDuelLifecycleProfileScopeV1::LeagueAndChallengeMatchInProgressLifecycle
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

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

pub fn evaluate_untrusted_competitive_duel_lifecycle_v1(
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    spec: MtgoCompetitiveDuelLifecycleEvaluationSpecV1,
    cases: Vec<MtgoCompetitiveDuelLifecycleEvaluationCaseV1>,
) -> Result<CheckedUntrustedMtgoCompetitiveDuelLifecycleEvaluationV1, MtgoContractErrorV1> {
    validate_spec_v1(&spec)?;
    if spec.duel_perception_profile_commitment_sha256
        != profile.perception_profile_commitment_sha256()
        || spec.duel_perception_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
    {
        return Err(error_v1(
            "competitive_duel_lifecycle_profile_mismatch",
            "evaluation spec does not bind the exact admitted duel perception profile",
        ));
    }
    if cases.is_empty() || cases.len() > 100_000 {
        return Err(error_v1(
            "competitive_duel_lifecycle_case_count_invalid",
            cases.len().to_string(),
        ));
    }

    let encoded_spec = serde_json::to_vec(&spec).map_err(|error| {
        error_v1(
            "competitive_duel_lifecycle_spec_serialization_failed",
            error.to_string(),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(COMPETITIVE_DUEL_LIFECYCLE_EVALUATION_DOMAIN_V1);
    hash_part_v1(&mut hasher, &encoded_spec);

    let mut previous_case_id: Option<&str> = None;
    let mut source_manifests = HashSet::new();
    let mut source_frames = HashSet::new();
    let mut mode_case_counts = [0_u32; 2];
    let mut mode_prediction_counts = [0_u32; 2];
    let mut prediction_count = 0_u32;
    let mut exact_snapshot_count = 0_u32;

    for case in &cases {
        validate_safe_identifier_v1(&case.case_id)?;
        if previous_case_id.is_some_and(|previous| previous >= case.case_id.as_str()) {
            return Err(error_v1(
                "competitive_duel_lifecycle_case_order_invalid",
                "case IDs must be unique and strictly increasing",
            ));
        }
        previous_case_id = Some(&case.case_id);
        validate_lower_hex_sha256_v1(&case.source_manifest_sha256)?;
        validate_lower_hex_sha256_v1(&case.source_canonical_bgra8_sha256)?;
        if case.source_frame_sequence == 0 {
            return Err(error_v1(
                "competitive_duel_lifecycle_source_sequence_invalid",
                &case.case_id,
            ));
        }
        if !source_manifests.insert(case.source_manifest_sha256.as_str()) {
            return Err(error_v1(
                "competitive_duel_lifecycle_duplicate_source",
                &case.source_manifest_sha256,
            ));
        }
        if !source_frames.insert(case.source_canonical_bgra8_sha256.as_str()) {
            return Err(error_v1(
                "competitive_duel_lifecycle_duplicate_frame",
                &case.source_canonical_bgra8_sha256,
            ));
        }

        let expected = validate_visible_competitive_lifecycle_snapshot_v1(case.expected.clone())
            .map_err(|error| {
                error_v1(
                    "competitive_duel_lifecycle_expected_invalid",
                    error.to_string(),
                )
            })?;
        validate_case_source_v1(case, &expected)?;
        let mode_index = event_kind_index_v1(expected.event_kind());
        mode_case_counts[mode_index] += 1;

        let (prediction_commitment, exact) = if let Some(prediction) = &case.prediction {
            let checked_prediction = validate_visible_competitive_lifecycle_snapshot_v1(
                prediction.clone(),
            )
            .map_err(|error| {
                error_v1(
                    "competitive_duel_lifecycle_prediction_invalid",
                    error.to_string(),
                )
            })?;
            validate_prediction_source_v1(case, &checked_prediction)?;
            prediction_count += 1;
            mode_prediction_counts[mode_index] += 1;
            let exact = lifecycle_semantics_equal_v1(&case.expected, prediction);
            exact_snapshot_count += u32::from(exact);
            (
                checked_prediction.snapshot_commitment_sha256().to_owned(),
                exact,
            )
        } else {
            ("abstained".to_owned(), false)
        };

        for part in [
            case.case_id.as_bytes(),
            case.source_manifest_sha256.as_bytes(),
            case.source_canonical_bgra8_sha256.as_bytes(),
            &case.source_frame_sequence.to_be_bytes(),
            expected.snapshot_commitment_sha256().as_bytes(),
            prediction_commitment.as_bytes(),
            &[u8::from(exact)],
        ] {
            hash_part_v1(&mut hasher, part);
        }
    }

    let unique_case_count = u32::try_from(cases.len()).map_err(|_| {
        error_v1(
            "competitive_duel_lifecycle_case_count_invalid",
            "case count does not fit u32",
        )
    })?;
    let prediction_coverage_bps = ratio_bps_v1(prediction_count, unique_case_count);
    let league_prediction_coverage_bps =
        ratio_bps_v1(mode_prediction_counts[0], mode_case_counts[0]);
    let challenge_prediction_coverage_bps =
        ratio_bps_v1(mode_prediction_counts[1], mode_case_counts[1]);
    let passes_declared_gate = mode_case_counts
        .iter()
        .all(|count| *count >= spec.minimum_unique_cases_per_mode)
        && prediction_coverage_bps >= spec.minimum_prediction_coverage_bps
        && league_prediction_coverage_bps >= spec.minimum_prediction_coverage_bps
        && challenge_prediction_coverage_bps >= spec.minimum_prediction_coverage_bps
        && prediction_count != 0
        && exact_snapshot_count == prediction_count;

    Ok(CheckedUntrustedMtgoCompetitiveDuelLifecycleEvaluationV1 {
        duel_perception_profile_commitment_sha256: spec.duel_perception_profile_commitment_sha256,
        duel_perception_profile_admission_commitment_sha256: spec
            .duel_perception_profile_admission_commitment_sha256,
        evaluation_commitment_sha256: format!("{:x}", hasher.finalize()),
        unique_case_count,
        league_case_count: mode_case_counts[0],
        challenge_case_count: mode_case_counts[1],
        prediction_count,
        exact_snapshot_count,
        prediction_coverage_bps,
        league_prediction_coverage_bps,
        challenge_prediction_coverage_bps,
        passes_declared_gate,
    })
}

pub fn admit_ratified_competitive_duel_lifecycle_profile_v1(
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    evaluation: CheckedUntrustedMtgoCompetitiveDuelLifecycleEvaluationV1,
) -> Result<AdmittedMtgoCompetitiveDuelLifecycleProfileV1, MtgoContractErrorV1> {
    admit_competitive_duel_lifecycle_profile_against_ratification_v1(
        profile,
        evaluation,
        RATIFIED_COMPETITIVE_DUEL_LIFECYCLE_EVALUATION_COMMITMENT_V1,
    )
}

fn admit_competitive_duel_lifecycle_profile_against_ratification_v1(
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    evaluation: CheckedUntrustedMtgoCompetitiveDuelLifecycleEvaluationV1,
    ratified_evaluation_commitment: Option<&str>,
) -> Result<AdmittedMtgoCompetitiveDuelLifecycleProfileV1, MtgoContractErrorV1> {
    if evaluation.duel_perception_profile_commitment_sha256
        != profile.perception_profile_commitment_sha256()
        || evaluation.duel_perception_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
    {
        return Err(error_v1(
            "competitive_duel_lifecycle_profile_mismatch",
            "evaluation and admitted duel perception profile differ",
        ));
    }
    if !evaluation.passes_declared_gate {
        return Err(error_v1(
            "competitive_duel_lifecycle_declared_gate_failed",
            &evaluation.evaluation_commitment_sha256,
        ));
    }
    let ratified = ratified_evaluation_commitment.ok_or_else(|| {
        error_v1(
            "competitive_duel_lifecycle_profile_not_ratified",
            "production contains no ratified League and Challenge duel lifecycle evaluation",
        )
    })?;
    validate_lower_hex_sha256_v1(ratified)?;
    if evaluation.evaluation_commitment_sha256 != ratified {
        return Err(error_v1(
            "competitive_duel_lifecycle_profile_not_ratified",
            "evaluation does not match the ratified production commitment",
        ));
    }

    let admission_commitment_sha256 = commitment_v1(
        COMPETITIVE_DUEL_LIFECYCLE_ADMISSION_DOMAIN_V1,
        &[
            evaluation
                .duel_perception_profile_commitment_sha256
                .as_bytes(),
            evaluation
                .duel_perception_profile_admission_commitment_sha256
                .as_bytes(),
            evaluation.evaluation_commitment_sha256.as_bytes(),
            b"league_and_challenge_match_in_progress_lifecycle_accuracy_only_no_input_or_entry",
        ],
    );
    Ok(AdmittedMtgoCompetitiveDuelLifecycleProfileV1 {
        duel_perception_profile_commitment_sha256: evaluation
            .duel_perception_profile_commitment_sha256,
        duel_perception_profile_admission_commitment_sha256: evaluation
            .duel_perception_profile_admission_commitment_sha256,
        evaluation_commitment_sha256: evaluation.evaluation_commitment_sha256,
        admission_commitment_sha256,
    })
}

#[cfg(test)]
pub(crate) fn competitive_duel_lifecycle_profile_admitted_for_test_v1(
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
) -> AdmittedMtgoCompetitiveDuelLifecycleProfileV1 {
    let evaluation_commitment_sha256 = "d".repeat(64);
    let admission_commitment_sha256 = commitment_v1(
        COMPETITIVE_DUEL_LIFECYCLE_ADMISSION_DOMAIN_V1,
        &[
            profile.perception_profile_commitment_sha256().as_bytes(),
            profile.admission_commitment_sha256().as_bytes(),
            evaluation_commitment_sha256.as_bytes(),
            b"league_and_challenge_match_in_progress_lifecycle_accuracy_only_no_input_or_entry",
        ],
    );
    AdmittedMtgoCompetitiveDuelLifecycleProfileV1 {
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

fn validate_spec_v1(
    spec: &MtgoCompetitiveDuelLifecycleEvaluationSpecV1,
) -> Result<(), MtgoContractErrorV1> {
    if spec.schema_version != MTGO_COMPETITIVE_DUEL_LIFECYCLE_EVALUATION_SCHEMA_V1 {
        return Err(error_v1(
            "competitive_duel_lifecycle_schema_mismatch",
            spec.schema_version.to_string(),
        ));
    }
    validate_safe_identifier_v1(&spec.evaluation_id)?;
    for value in [
        spec.duel_perception_profile_commitment_sha256.as_str(),
        spec.duel_perception_profile_admission_commitment_sha256
            .as_str(),
        spec.corpus_manifest_sha256.as_str(),
        spec.annotation_protocol_sha256.as_str(),
        spec.evaluator_binary_sha256.as_str(),
    ] {
        validate_lower_hex_sha256_v1(value)?;
    }
    if spec.minimum_unique_cases_per_mode == 0
        || spec.minimum_unique_cases_per_mode > 50_000
        || !(1..=10_000).contains(&spec.minimum_prediction_coverage_bps)
    {
        return Err(error_v1(
            "competitive_duel_lifecycle_gate_invalid",
            "per-mode case and prediction coverage requirements are invalid",
        ));
    }
    if spec.required_event_kinds
        != [
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ]
    {
        return Err(error_v1(
            "competitive_duel_lifecycle_modes_invalid",
            "required modes must be exactly League then Challenge",
        ));
    }
    Ok(())
}

fn validate_case_source_v1(
    case: &MtgoCompetitiveDuelLifecycleEvaluationCaseV1,
    expected: &crate::CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
) -> Result<(), MtgoContractErrorV1> {
    if expected.phase() != MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
        || expected.frame_sha256_v1() != case.source_canonical_bgra8_sha256
        || expected.frame_sequence() != case.source_frame_sequence
    {
        return Err(error_v1(
            "competitive_duel_lifecycle_expected_source_mismatch",
            &case.case_id,
        ));
    }
    Ok(())
}

fn validate_prediction_source_v1(
    case: &MtgoCompetitiveDuelLifecycleEvaluationCaseV1,
    prediction: &crate::CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
) -> Result<(), MtgoContractErrorV1> {
    if prediction.phase() != MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
        || prediction.frame_sha256_v1() != case.source_canonical_bgra8_sha256
        || prediction.frame_sequence() != case.source_frame_sequence
        || prediction.event_kind() != case.expected.event_kind
    {
        return Err(error_v1(
            "competitive_duel_lifecycle_prediction_source_mismatch",
            &case.case_id,
        ));
    }
    Ok(())
}

fn lifecycle_semantics_equal_v1(
    expected: &MtgoVisibleCompetitiveLifecycleSnapshotV1,
    prediction: &MtgoVisibleCompetitiveLifecycleSnapshotV1,
) -> bool {
    expected.schema_version == prediction.schema_version
        && expected.event_kind == prediction.event_kind
        && expected.phase == prediction.phase
        && expected.frame_id == prediction.frame_id
        && expected.frame_sequence == prediction.frame_sequence
        && expected.frame_sha256 == prediction.frame_sha256
        && expected.client_bounds == prediction.client_bounds
        && expected.event_identity_sha256 == prediction.event_identity_sha256
        && expected.match_identity_sha256 == prediction.match_identity_sha256
        && expected.game_number == prediction.game_number
        && expected.entry_terms == prediction.entry_terms
        && expected.visible_state_complete == prediction.visible_state_complete
        && expected.facts == prediction.facts
}

fn event_kind_index_v1(kind: MtgoCompetitiveEventKindV1) -> usize {
    match kind {
        MtgoCompetitiveEventKindV1::League => 0,
        MtgoCompetitiveEventKindV1::Challenge => 1,
    }
}

fn ratio_bps_v1(numerator: u32, denominator: u32) -> u16 {
    if denominator == 0 {
        return 0;
    }
    u16::try_from((u64::from(numerator) * 10_000) / u64::from(denominator)).unwrap_or(10_000)
}

fn validate_safe_identifier_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(error_v1(
            "competitive_duel_lifecycle_identifier_invalid",
            value,
        ));
    }
    Ok(())
}

fn validate_lower_hex_sha256_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1("competitive_duel_lifecycle_sha256_invalid", value));
    }
    Ok(())
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
        MtgoLifecycleVisibleFactKindV1, MtgoLifecycleVisibleFactV1, MtgoRectPxV1,
    };

    fn snapshot_v1(
        kind: MtgoCompetitiveEventKindV1,
        frame_id: u64,
        frame_sequence: u64,
        frame_sha256: String,
        snapshot_id: &str,
    ) -> MtgoVisibleCompetitiveLifecycleSnapshotV1 {
        let fact = |kind, x| MtgoLifecycleVisibleFactV1 {
            kind,
            rect_client_px: MtgoRectPxV1 {
                x,
                y: 0,
                width: 1,
                height: 1,
            },
            content_sha256: format!("{:064x}", x + 1),
            confidence_bps: 10_000,
        };
        MtgoVisibleCompetitiveLifecycleSnapshotV1 {
            schema_version: 1,
            snapshot_id: snapshot_id.to_owned(),
            event_kind: kind,
            phase: MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
            frame_id,
            frame_sequence,
            frame_sha256,
            client_bounds: MtgoRectPxV1 {
                x: 0,
                y: 0,
                width: 3,
                height: 1,
            },
            event_identity_sha256: Some("a".repeat(64)),
            match_identity_sha256: Some("b".repeat(64)),
            game_number: Some(1),
            entry_terms: None,
            visible_state_complete: true,
            facts: vec![
                fact(MtgoLifecycleVisibleFactKindV1::MatchSurfaceVisible, 0),
                fact(MtgoLifecycleVisibleFactKindV1::LocalClockVisible, 1),
                fact(MtgoLifecycleVisibleFactKindV1::OpponentClockVisible, 2),
            ],
        }
    }

    fn spec_v1(
        profile: &AdmittedMtgoDuelPerceptionProfileV1,
    ) -> MtgoCompetitiveDuelLifecycleEvaluationSpecV1 {
        MtgoCompetitiveDuelLifecycleEvaluationSpecV1 {
            schema_version: MTGO_COMPETITIVE_DUEL_LIFECYCLE_EVALUATION_SCHEMA_V1,
            evaluation_id: "competitive-duel-lifecycle-test-v1".to_owned(),
            duel_perception_profile_commitment_sha256: profile
                .perception_profile_commitment_sha256()
                .to_owned(),
            duel_perception_profile_admission_commitment_sha256: profile
                .admission_commitment_sha256()
                .to_owned(),
            corpus_manifest_sha256: "1".repeat(64),
            annotation_protocol_sha256: "2".repeat(64),
            evaluator_binary_sha256: "3".repeat(64),
            minimum_unique_cases_per_mode: 1,
            minimum_prediction_coverage_bps: 10_000,
            required_event_kinds: vec![
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEventKindV1::Challenge,
            ],
        }
    }

    fn cases_v1() -> Vec<MtgoCompetitiveDuelLifecycleEvaluationCaseV1> {
        [
            (MtgoCompetitiveEventKindV1::League, 10_u64, '4'),
            (MtgoCompetitiveEventKindV1::Challenge, 11_u64, '5'),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (kind, sequence, digit))| {
            let frame_sha256 = digit.to_string().repeat(64);
            let expected = snapshot_v1(
                kind,
                20 + index as u64,
                sequence,
                frame_sha256.clone(),
                &format!("expected-{index}"),
            );
            let mut prediction = expected.clone();
            prediction.snapshot_id = format!("prediction-{index}");
            MtgoCompetitiveDuelLifecycleEvaluationCaseV1 {
                case_id: format!("case-{index}"),
                source_manifest_sha256: format!("{:064x}", index + 6),
                source_canonical_bgra8_sha256: frame_sha256,
                source_frame_sequence: sequence,
                expected,
                prediction: Some(prediction),
            }
        })
        .collect()
    }

    #[test]
    fn exact_league_and_challenge_predictions_pass_but_production_is_empty() {
        let profile = duel_perception_profile_admitted_for_test_v1(
            duel_perception_runtime_profile_for_test_v1(),
        );
        let evaluation = evaluate_untrusted_competitive_duel_lifecycle_v1(
            &profile,
            spec_v1(&profile),
            cases_v1(),
        )
        .unwrap();
        assert!(evaluation.passes_declared_gate());
        assert_eq!(evaluation.unique_case_count(), 2);
        assert_eq!(evaluation.league_case_count(), 1);
        assert_eq!(evaluation.challenge_case_count(), 1);
        assert_eq!(evaluation.prediction_count(), 2);
        assert_eq!(evaluation.exact_snapshot_count(), 2);
        assert_eq!(evaluation.prediction_coverage_bps(), 10_000);
        assert_eq!(evaluation.league_prediction_coverage_bps(), 10_000);
        assert_eq!(evaluation.challenge_prediction_coverage_bps(), 10_000);
        assert!(!evaluation.safe_for_live_input_v1());
        assert!(!evaluation.permits_event_entry_v1());
        assert_eq!(
            admit_ratified_competitive_duel_lifecycle_profile_v1(&profile, evaluation)
                .err()
                .unwrap()
                .code(),
            "competitive_duel_lifecycle_profile_not_ratified"
        );

        let admitted = competitive_duel_lifecycle_profile_admitted_for_test_v1(&profile);
        assert_eq!(
            admitted.scope(),
            MtgoCompetitiveDuelLifecycleProfileScopeV1::LeagueAndChallengeMatchInProgressLifecycle
        );
        assert_eq!(
            admitted.duel_perception_profile_commitment_sha256(),
            profile.perception_profile_commitment_sha256()
        );
        assert!(!admitted.safe_for_live_input_v1());
        assert!(!admitted.permits_event_entry_v1());
    }

    #[test]
    fn missing_mode_abstention_semantic_drift_and_duplicate_frame_fail() {
        let profile = duel_perception_profile_admitted_for_test_v1(
            duel_perception_runtime_profile_for_test_v1(),
        );
        let mut cases = cases_v1();
        cases.pop();
        let missing_mode =
            evaluate_untrusted_competitive_duel_lifecycle_v1(&profile, spec_v1(&profile), cases)
                .unwrap();
        assert!(!missing_mode.passes_declared_gate());

        let mut cases = cases_v1();
        cases[0].prediction = None;
        let abstained =
            evaluate_untrusted_competitive_duel_lifecycle_v1(&profile, spec_v1(&profile), cases)
                .unwrap();
        assert!(!abstained.passes_declared_gate());

        let mut cases = cases_v1();
        cases[0].prediction.as_mut().unwrap().match_identity_sha256 = Some("c".repeat(64));
        let drifted =
            evaluate_untrusted_competitive_duel_lifecycle_v1(&profile, spec_v1(&profile), cases)
                .unwrap();
        assert!(!drifted.passes_declared_gate());

        let mut cases = cases_v1();
        let duplicate_frame_sha256 = cases[0].source_canonical_bgra8_sha256.clone();
        cases[1].source_canonical_bgra8_sha256 = duplicate_frame_sha256.clone();
        cases[1].expected.frame_sha256 = duplicate_frame_sha256.clone();
        cases[1].prediction.as_mut().unwrap().frame_sha256 = duplicate_frame_sha256;
        assert_eq!(
            evaluate_untrusted_competitive_duel_lifecycle_v1(&profile, spec_v1(&profile), cases)
                .err()
                .unwrap()
                .code(),
            "competitive_duel_lifecycle_duplicate_frame"
        );
    }
}
