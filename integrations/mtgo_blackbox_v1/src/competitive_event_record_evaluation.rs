use crate::{
    validate_snapshot_artifact_source_v1, validate_source_profile_v1,
    validate_visible_competitive_event_record_v1,
    validate_visible_competitive_lifecycle_snapshot_v1,
    CheckedUntrustedMtgoCompetitiveEventRecordV1,
    CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    CheckedUntrustedMtgoCompetitiveNavigationSourceV1, MtgoCompetitiveEventKindV1,
    MtgoCompetitiveEventProgressV1, MtgoCompetitiveEventVisibleStatusV1,
    MtgoCompetitiveLifecyclePhaseV1, MtgoContractErrorV1, MtgoVisibleCompetitiveEventRecordV1,
    MtgoVisibleCompetitiveLifecycleSnapshotV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

pub const MTGO_COMPETITIVE_EVENT_RECORD_EVALUATION_SCHEMA_V1: u32 = 1;

const EVENT_RECORD_PREDICTION_DOMAIN_V1: &[u8] = b"mtgo-competitive-event-record-prediction-v1";
const EVENT_RECORD_EVALUATION_DOMAIN_V1: &[u8] = b"mtgo-competitive-event-record-evaluation-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveEventRecordSliceV1 {
    LeagueWaitingForPairing,
    LeaguePairingReady,
    LeagueBetweenMatches,
    LeagueEventComplete,
    ChallengeWaitingForPairing,
    ChallengePairingReady,
    ChallengeBetweenMatches,
    ChallengeEventComplete,
}

const REQUIRED_EVENT_RECORD_SLICES_V1: [MtgoCompetitiveEventRecordSliceV1; 8] = [
    MtgoCompetitiveEventRecordSliceV1::LeagueWaitingForPairing,
    MtgoCompetitiveEventRecordSliceV1::LeaguePairingReady,
    MtgoCompetitiveEventRecordSliceV1::LeagueBetweenMatches,
    MtgoCompetitiveEventRecordSliceV1::LeagueEventComplete,
    MtgoCompetitiveEventRecordSliceV1::ChallengeWaitingForPairing,
    MtgoCompetitiveEventRecordSliceV1::ChallengePairingReady,
    MtgoCompetitiveEventRecordSliceV1::ChallengeBetweenMatches,
    MtgoCompetitiveEventRecordSliceV1::ChallengeEventComplete,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveEventRecordFieldV1 {
    LifecycleSnapshot,
    EventIdentity,
    EventKind,
    Status,
    MatchRecord,
    ScheduleProgress,
    ChallengeMatchPoints,
    ChallengeStanding,
    Completion,
}

const REQUIRED_EVENT_RECORD_FIELDS_V1: [MtgoCompetitiveEventRecordFieldV1; 9] = [
    MtgoCompetitiveEventRecordFieldV1::LifecycleSnapshot,
    MtgoCompetitiveEventRecordFieldV1::EventIdentity,
    MtgoCompetitiveEventRecordFieldV1::EventKind,
    MtgoCompetitiveEventRecordFieldV1::Status,
    MtgoCompetitiveEventRecordFieldV1::MatchRecord,
    MtgoCompetitiveEventRecordFieldV1::ScheduleProgress,
    MtgoCompetitiveEventRecordFieldV1::ChallengeMatchPoints,
    MtgoCompetitiveEventRecordFieldV1::ChallengeStanding,
    MtgoCompetitiveEventRecordFieldV1::Completion,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveEventRecordEvaluationSpecV1 {
    pub schema_version: u32,
    pub evaluation_id: String,
    pub navigation_profile_commitment_sha256: String,
    pub corpus_manifest_sha256: String,
    pub annotation_protocol_sha256: String,
    pub evaluator_binary_sha256: String,
    pub minimum_unique_cases_per_slice: u32,
    pub minimum_prediction_coverage_bps: u16,
    pub minimum_field_accuracy_bps: u16,
    pub required_slices: Vec<MtgoCompetitiveEventRecordSliceV1>,
    pub required_fields: Vec<MtgoCompetitiveEventRecordFieldV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveEventRecordCorpusCaseV1 {
    pub case_id: String,
    pub slice: MtgoCompetitiveEventRecordSliceV1,
    pub source_manifest_sha256: String,
    pub source_canonical_bgra8_sha256: String,
    pub source_profile_binding_sha256: String,
    pub expected_lifecycle_snapshot_commitment_sha256: String,
    pub expected_event_record_commitment_sha256: String,
    pub annotator_alias_sha256: String,
    pub annotation_receipt_sha256: String,
    pub annotated_at_unix_millis: u64,
    pub approved_account_identity_visually_confirmed: bool,
    pub unobscured_frame_visually_confirmed: bool,
    pub event_identity_visually_confirmed: bool,
    pub event_kind_visually_confirmed: bool,
    pub status_visually_confirmed: bool,
    pub match_record_visually_confirmed: bool,
    pub schedule_progress_visually_confirmed: bool,
    pub challenge_match_points_visually_confirmed: bool,
    pub challenge_standing_visually_confirmed: bool,
    pub completion_visually_confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveEventRecordCorpusManifestV1 {
    pub schema_version: u32,
    pub corpus_id: String,
    pub navigation_profile_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub annotation_protocol_sha256: String,
    pub cases: Vec<MtgoCompetitiveEventRecordCorpusCaseV1>,
}

/// A structurally checked review manifest for event-summary interpretation.
/// It contains commitments only and grants no classification or action scope.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoCompetitiveEventRecordCorpusManifestV1;
/// fn cannot_act(value: &CheckedUntrustedMtgoCompetitiveEventRecordCorpusManifestV1) {
///     let _ = value.input_command();
///     let _ = value.enter_event();
/// }
/// ```
pub struct CheckedUntrustedMtgoCompetitiveEventRecordCorpusManifestV1 {
    manifest: MtgoCompetitiveEventRecordCorpusManifestV1,
    manifest_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveEventRecordCorpusManifestV1 {
    pub fn manifest_sha256_v1(&self) -> &str {
        &self.manifest_sha256
    }

    pub fn case_count_v1(&self) -> usize {
        self.manifest.cases.len()
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveEventRecordPredictionV1 {
    pub schema_version: u32,
    pub navigation_profile_commitment_sha256: String,
    pub source_manifest_sha256: String,
    pub source_canonical_bgra8_sha256: String,
    pub source_profile_binding_sha256: String,
    pub classifier_request_sha256: String,
    pub classifier_response_sha256: String,
    pub lifecycle: MtgoVisibleCompetitiveLifecycleSnapshotV1,
    pub record: MtgoVisibleCompetitiveEventRecordV1,
}

/// One checked-untrusted classifier declaration bound to an exact reviewed
/// navigation source. The classifier exchange hashes remain caller supplied;
/// the value therefore has no runtime or action authority.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoCompetitiveEventRecordPredictionV1;
/// fn cannot_act(value: &CheckedUntrustedMtgoCompetitiveEventRecordPredictionV1) {
///     let _ = value.input_command();
///     let _ = value.enter_event();
/// }
/// ```
pub struct CheckedUntrustedMtgoCompetitiveEventRecordPredictionV1 {
    payload: MtgoCompetitiveEventRecordPredictionV1,
    lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    record: CheckedUntrustedMtgoCompetitiveEventRecordV1,
    prediction_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveEventRecordPredictionV1 {
    pub fn prediction_commitment_sha256_v1(&self) -> &str {
        &self.prediction_commitment_sha256
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

pub struct MtgoCompetitiveEventRecordEvaluationCaseV1<'a> {
    pub case_id: String,
    pub source: &'a CheckedUntrustedMtgoCompetitiveNavigationSourceV1,
    pub expected_lifecycle: MtgoVisibleCompetitiveLifecycleSnapshotV1,
    pub expected_record: MtgoVisibleCompetitiveEventRecordV1,
    pub prediction: Option<&'a CheckedUntrustedMtgoCompetitiveEventRecordPredictionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveEventRecordFieldAccuracyV1 {
    pub field: MtgoCompetitiveEventRecordFieldV1,
    pub evaluated_predictions: u32,
    pub exact_predictions: u32,
    pub accuracy_bps: u16,
}

/// Recomputed event-record coverage and per-field accuracy. It intentionally
/// has no `Debug`, `Clone`, serde, ratification, or action conversion.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoCompetitiveEventRecordEvaluationV1;
/// fn cannot_act(value: &CheckedUntrustedMtgoCompetitiveEventRecordEvaluationV1) {
///     let _ = value.input_command();
///     let _ = value.enter_event();
/// }
/// ```
pub struct CheckedUntrustedMtgoCompetitiveEventRecordEvaluationV1 {
    navigation_profile_commitment_sha256: String,
    evaluation_commitment_sha256: String,
    unique_case_count: u32,
    prediction_count: u32,
    exact_semantic_prediction_count: u32,
    prediction_coverage_bps: u16,
    minimum_prediction_coverage_bps_per_slice: u16,
    minimum_observed_cases_per_slice: u32,
    field_accuracy: Vec<MtgoCompetitiveEventRecordFieldAccuracyV1>,
    minimum_field_accuracy_bps: u16,
    missing_slices: Vec<MtgoCompetitiveEventRecordSliceV1>,
    passes_declared_gate: bool,
}

impl CheckedUntrustedMtgoCompetitiveEventRecordEvaluationV1 {
    pub fn navigation_profile_commitment_sha256_v1(&self) -> &str {
        &self.navigation_profile_commitment_sha256
    }

    pub fn evaluation_commitment_sha256_v1(&self) -> &str {
        &self.evaluation_commitment_sha256
    }

    pub fn unique_case_count_v1(&self) -> u32 {
        self.unique_case_count
    }

    pub fn prediction_count_v1(&self) -> u32 {
        self.prediction_count
    }

    pub fn exact_semantic_prediction_count_v1(&self) -> u32 {
        self.exact_semantic_prediction_count
    }

    pub fn prediction_coverage_bps_v1(&self) -> u16 {
        self.prediction_coverage_bps
    }

    pub fn minimum_prediction_coverage_bps_per_slice_v1(&self) -> u16 {
        self.minimum_prediction_coverage_bps_per_slice
    }

    pub fn minimum_observed_cases_per_slice_v1(&self) -> u32 {
        self.minimum_observed_cases_per_slice
    }

    pub fn field_accuracy_v1(&self) -> &[MtgoCompetitiveEventRecordFieldAccuracyV1] {
        &self.field_accuracy
    }

    pub fn minimum_field_accuracy_bps_v1(&self) -> u16 {
        self.minimum_field_accuracy_bps
    }

    pub fn missing_slices_v1(&self) -> &[MtgoCompetitiveEventRecordSliceV1] {
        &self.missing_slices
    }

    pub fn passes_declared_gate_v1(&self) -> bool {
        self.passes_declared_gate
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

pub fn check_untrusted_competitive_event_record_corpus_manifest_v1(
    profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    manifest: MtgoCompetitiveEventRecordCorpusManifestV1,
) -> Result<CheckedUntrustedMtgoCompetitiveEventRecordCorpusManifestV1, MtgoContractErrorV1> {
    if manifest.schema_version != MTGO_COMPETITIVE_EVENT_RECORD_EVALUATION_SCHEMA_V1 {
        return Err(error(
            "event_record_corpus_schema",
            "schema must be version 1",
        ));
    }
    validate_identifier(&manifest.corpus_id, "event_record_corpus_id")?;
    validate_sha256(
        &manifest.navigation_profile_commitment_sha256,
        "event_record_corpus_profile",
    )?;
    validate_sha256(
        &manifest.approved_account_alias_sha256,
        "event_record_corpus_account",
    )?;
    validate_sha256(
        &manifest.annotation_protocol_sha256,
        "event_record_annotation_protocol",
    )?;
    if manifest.navigation_profile_commitment_sha256 != profile.profile_commitment_sha256()
        || manifest.approved_account_alias_sha256 != profile.approved_account_alias_sha256()
    {
        return Err(error(
            "event_record_corpus_profile",
            "corpus must bind the exact navigation profile and approved account",
        ));
    }
    if manifest.cases.is_empty() || manifest.cases.len() > 100_000 {
        return Err(error(
            "event_record_corpus_case_count",
            "corpus must contain between 1 and 100000 cases",
        ));
    }

    let mut previous_case_id: Option<&str> = None;
    let mut source_manifests = HashSet::new();
    let mut source_frames = HashSet::new();
    for case in &manifest.cases {
        validate_identifier(&case.case_id, "event_record_corpus_case_id")?;
        if previous_case_id.is_some_and(|previous| previous >= case.case_id.as_str()) {
            return Err(error(
                "event_record_corpus_case_order",
                "case IDs must be unique and strictly increasing",
            ));
        }
        previous_case_id = Some(&case.case_id);
        for (value, code) in [
            (&case.source_manifest_sha256, "event_record_source_manifest"),
            (
                &case.source_canonical_bgra8_sha256,
                "event_record_source_pixels",
            ),
            (
                &case.source_profile_binding_sha256,
                "event_record_source_profile",
            ),
            (
                &case.expected_lifecycle_snapshot_commitment_sha256,
                "event_record_expected_lifecycle",
            ),
            (
                &case.expected_event_record_commitment_sha256,
                "event_record_expected_record",
            ),
            (&case.annotator_alias_sha256, "event_record_annotator"),
            (
                &case.annotation_receipt_sha256,
                "event_record_annotation_receipt",
            ),
        ] {
            validate_sha256(value, code)?;
        }
        if !source_manifests.insert(case.source_manifest_sha256.as_str())
            || !source_frames.insert(case.source_canonical_bgra8_sha256.as_str())
        {
            return Err(error(
                "event_record_corpus_duplicate_source",
                "source manifests and frame pixels must each be unique",
            ));
        }
        validate_annotation_review(case)?;
    }

    let bytes = serde_json::to_vec(&manifest)
        .map_err(|value| error("event_record_corpus_serialization", value.to_string()))?;
    Ok(CheckedUntrustedMtgoCompetitiveEventRecordCorpusManifestV1 {
        manifest,
        manifest_sha256: format!("{:x}", Sha256::digest(bytes)),
    })
}

pub fn check_untrusted_competitive_event_record_prediction_v1(
    profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    source: &CheckedUntrustedMtgoCompetitiveNavigationSourceV1,
    payload: MtgoCompetitiveEventRecordPredictionV1,
) -> Result<CheckedUntrustedMtgoCompetitiveEventRecordPredictionV1, MtgoContractErrorV1> {
    if payload.schema_version != MTGO_COMPETITIVE_EVENT_RECORD_EVALUATION_SCHEMA_V1 {
        return Err(error(
            "event_record_prediction_schema",
            "schema must be version 1",
        ));
    }
    validate_source_profile_v1(profile, source)?;
    if payload.navigation_profile_commitment_sha256 != profile.profile_commitment_sha256()
        || payload.source_manifest_sha256 != source.manifest_sha256()
        || payload.source_canonical_bgra8_sha256 != source.canonical_bgra8_sha256()
        || payload.source_profile_binding_sha256 != source.source_profile_binding_sha256()
    {
        return Err(error(
            "event_record_prediction_source",
            "prediction must bind the exact profile and navigation source",
        ));
    }
    validate_sha256(
        &payload.classifier_request_sha256,
        "event_record_classifier_request",
    )?;
    validate_sha256(
        &payload.classifier_response_sha256,
        "event_record_classifier_response",
    )?;
    let lifecycle = validate_visible_competitive_lifecycle_snapshot_v1(payload.lifecycle.clone())?;
    validate_snapshot_artifact_source_v1(&lifecycle, source)?;
    let record = validate_visible_competitive_event_record_v1(
        &lifecycle,
        profile.approved_account_alias_sha256(),
        payload.record.clone(),
    )?;
    event_record_slice(
        lifecycle.event_kind(),
        lifecycle.phase(),
        record.status_v1(),
    )?;
    let bytes = serde_json::to_vec(&payload)
        .map_err(|value| error("event_record_prediction_serialization", value.to_string()))?;
    Ok(CheckedUntrustedMtgoCompetitiveEventRecordPredictionV1 {
        payload,
        lifecycle,
        record,
        prediction_commitment_sha256: commitment(EVENT_RECORD_PREDICTION_DOMAIN_V1, &[&bytes]),
    })
}

pub fn evaluate_untrusted_competitive_event_record_profile_v1(
    profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    corpus: &CheckedUntrustedMtgoCompetitiveEventRecordCorpusManifestV1,
    spec: MtgoCompetitiveEventRecordEvaluationSpecV1,
    cases: Vec<MtgoCompetitiveEventRecordEvaluationCaseV1<'_>>,
) -> Result<CheckedUntrustedMtgoCompetitiveEventRecordEvaluationV1, MtgoContractErrorV1> {
    validate_evaluation_spec(&spec)?;
    if spec.navigation_profile_commitment_sha256 != profile.profile_commitment_sha256()
        || corpus.manifest.navigation_profile_commitment_sha256
            != profile.profile_commitment_sha256()
        || corpus.manifest.approved_account_alias_sha256 != profile.approved_account_alias_sha256()
        || spec.corpus_manifest_sha256 != corpus.manifest_sha256
        || spec.annotation_protocol_sha256 != corpus.manifest.annotation_protocol_sha256
    {
        return Err(error(
            "event_record_evaluation_profile",
            "evaluation must bind the exact profile, account, corpus, and annotation protocol",
        ));
    }
    if cases.is_empty() || cases.len() > 100_000 || cases.len() != corpus.manifest.cases.len() {
        return Err(error(
            "event_record_evaluation_case_count",
            "evaluated cases must exactly cover the reviewed corpus",
        ));
    }

    let spec_bytes = serde_json::to_vec(&spec)
        .map_err(|value| error("event_record_spec_serialization", value.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(EVENT_RECORD_EVALUATION_DOMAIN_V1);
    hash_part(&mut hasher, &spec_bytes);

    let mut previous_case_id: Option<&str> = None;
    let mut source_manifests = HashSet::new();
    let mut source_frames = HashSet::new();
    let mut slice_counts = HashMap::new();
    let mut slice_prediction_counts = HashMap::new();
    let mut field_counts: HashMap<MtgoCompetitiveEventRecordFieldV1, (u32, u32)> = HashMap::new();
    let mut prediction_count = 0_u32;
    let mut exact_semantic_prediction_count = 0_u32;

    for (case, reviewed) in cases.iter().zip(&corpus.manifest.cases) {
        validate_identifier(&case.case_id, "event_record_case_id")?;
        if previous_case_id.is_some_and(|previous| previous >= case.case_id.as_str()) {
            return Err(error(
                "event_record_case_order",
                "case IDs must be unique and strictly increasing",
            ));
        }
        previous_case_id = Some(&case.case_id);
        if !source_manifests.insert(case.source.manifest_sha256())
            || !source_frames.insert(case.source.canonical_bgra8_sha256())
        {
            return Err(error(
                "event_record_duplicate_source",
                "evaluated source manifests and frames must be unique",
            ));
        }
        let (expected_lifecycle, expected_record, slice) = validate_case_record(
            profile,
            case.source,
            case.expected_lifecycle.clone(),
            case.expected_record.clone(),
        )?;
        if reviewed.case_id != case.case_id
            || reviewed.slice != slice
            || reviewed.source_manifest_sha256 != case.source.manifest_sha256()
            || reviewed.source_canonical_bgra8_sha256 != case.source.canonical_bgra8_sha256()
            || reviewed.source_profile_binding_sha256 != case.source.source_profile_binding_sha256()
            || reviewed.expected_lifecycle_snapshot_commitment_sha256
                != expected_lifecycle.snapshot_commitment_sha256()
            || reviewed.expected_event_record_commitment_sha256
                != expected_record.record_commitment_sha256_v1()
        {
            return Err(error(
                "event_record_corpus_case_binding",
                "evaluated case must exactly match its reviewed corpus record",
            ));
        }
        *slice_counts.entry(slice).or_insert(0_u32) += 1;

        let (prediction_commitment, field_results) = if let Some(prediction) = case.prediction {
            if prediction.payload.navigation_profile_commitment_sha256
                != profile.profile_commitment_sha256()
                || prediction.payload.source_manifest_sha256 != case.source.manifest_sha256()
                || prediction.payload.source_canonical_bgra8_sha256
                    != case.source.canonical_bgra8_sha256()
                || prediction.payload.source_profile_binding_sha256
                    != case.source.source_profile_binding_sha256()
            {
                return Err(error(
                    "event_record_prediction_source",
                    "prediction must refer to the exact reviewed case source",
                ));
            }
            prediction_count += 1;
            *slice_prediction_counts.entry(slice).or_insert(0_u32) += 1;
            let results = compare_fields(
                &expected_lifecycle,
                &expected_record,
                &prediction.lifecycle,
                &prediction.record,
            );
            for (field, exact) in &results {
                let counts = field_counts.entry(*field).or_insert((0, 0));
                counts.0 += 1;
                counts.1 += u32::from(*exact);
            }
            exact_semantic_prediction_count += u32::from(results.iter().all(|(_, exact)| *exact));
            (prediction.prediction_commitment_sha256.as_str(), results)
        } else {
            ("abstained", Vec::new())
        };

        hash_part(&mut hasher, case.case_id.as_bytes());
        hash_part(&mut hasher, case.source.manifest_sha256().as_bytes());
        hash_part(&mut hasher, case.source.canonical_bgra8_sha256().as_bytes());
        hash_part(
            &mut hasher,
            expected_lifecycle.snapshot_commitment_sha256().as_bytes(),
        );
        hash_part(
            &mut hasher,
            expected_record.record_commitment_sha256_v1().as_bytes(),
        );
        hash_part(&mut hasher, prediction_commitment.as_bytes());
        for (field, exact) in field_results {
            hash_part(&mut hasher, &[field as u8, u8::from(exact)]);
        }
    }

    let unique_case_count = u32::try_from(cases.len())
        .map_err(|_| error("event_record_case_count", "case count does not fit u32"))?;
    let prediction_coverage_bps = ratio_bps(prediction_count, unique_case_count);
    let missing_slices = REQUIRED_EVENT_RECORD_SLICES_V1
        .iter()
        .copied()
        .filter(|slice| !slice_counts.contains_key(slice))
        .collect::<Vec<_>>();
    let minimum_observed_cases_per_slice = REQUIRED_EVENT_RECORD_SLICES_V1
        .iter()
        .map(|slice| slice_counts.get(slice).copied().unwrap_or(0))
        .min()
        .unwrap_or(0);
    let minimum_prediction_coverage_bps_per_slice = REQUIRED_EVENT_RECORD_SLICES_V1
        .iter()
        .map(|slice| {
            ratio_bps(
                slice_prediction_counts.get(slice).copied().unwrap_or(0),
                slice_counts.get(slice).copied().unwrap_or(0),
            )
        })
        .min()
        .unwrap_or(0);
    let field_accuracy = REQUIRED_EVENT_RECORD_FIELDS_V1
        .iter()
        .copied()
        .map(|field| {
            let (evaluated_predictions, exact_predictions) =
                field_counts.get(&field).copied().unwrap_or((0, 0));
            MtgoCompetitiveEventRecordFieldAccuracyV1 {
                field,
                evaluated_predictions,
                exact_predictions,
                accuracy_bps: ratio_bps(exact_predictions, evaluated_predictions),
            }
        })
        .collect::<Vec<_>>();
    let minimum_field_accuracy_bps = field_accuracy
        .iter()
        .map(|metric| metric.accuracy_bps)
        .min()
        .unwrap_or(0);
    let passes_declared_gate = missing_slices.is_empty()
        && minimum_observed_cases_per_slice >= spec.minimum_unique_cases_per_slice
        && prediction_coverage_bps >= spec.minimum_prediction_coverage_bps
        && minimum_prediction_coverage_bps_per_slice >= spec.minimum_prediction_coverage_bps
        && minimum_field_accuracy_bps >= spec.minimum_field_accuracy_bps;

    Ok(CheckedUntrustedMtgoCompetitiveEventRecordEvaluationV1 {
        navigation_profile_commitment_sha256: spec.navigation_profile_commitment_sha256,
        evaluation_commitment_sha256: format!("{:x}", hasher.finalize()),
        unique_case_count,
        prediction_count,
        exact_semantic_prediction_count,
        prediction_coverage_bps,
        minimum_prediction_coverage_bps_per_slice,
        minimum_observed_cases_per_slice,
        field_accuracy,
        minimum_field_accuracy_bps,
        missing_slices,
        passes_declared_gate,
    })
}

fn validate_case_record(
    profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    source: &CheckedUntrustedMtgoCompetitiveNavigationSourceV1,
    lifecycle: MtgoVisibleCompetitiveLifecycleSnapshotV1,
    record: MtgoVisibleCompetitiveEventRecordV1,
) -> Result<
    (
        CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
        CheckedUntrustedMtgoCompetitiveEventRecordV1,
        MtgoCompetitiveEventRecordSliceV1,
    ),
    MtgoContractErrorV1,
> {
    validate_source_profile_v1(profile, source)?;
    let lifecycle = validate_visible_competitive_lifecycle_snapshot_v1(lifecycle)?;
    validate_snapshot_artifact_source_v1(&lifecycle, source)?;
    let record = validate_visible_competitive_event_record_v1(
        &lifecycle,
        profile.approved_account_alias_sha256(),
        record,
    )?;
    let slice = event_record_slice(
        lifecycle.event_kind(),
        lifecycle.phase(),
        record.status_v1(),
    )?;
    Ok((lifecycle, record, slice))
}

fn event_record_slice(
    kind: MtgoCompetitiveEventKindV1,
    phase: MtgoCompetitiveLifecyclePhaseV1,
    status: MtgoCompetitiveEventVisibleStatusV1,
) -> Result<MtgoCompetitiveEventRecordSliceV1, MtgoContractErrorV1> {
    use MtgoCompetitiveEventRecordSliceV1::*;
    match (kind, phase, status) {
        (
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing,
            MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing,
        ) => Ok(LeagueWaitingForPairing),
        (
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveLifecyclePhaseV1::PairingReady,
            MtgoCompetitiveEventVisibleStatusV1::PairingReady,
        ) => Ok(LeaguePairingReady),
        (
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveLifecyclePhaseV1::MatchComplete,
            MtgoCompetitiveEventVisibleStatusV1::BetweenMatches,
        ) => Ok(LeagueBetweenMatches),
        (
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveLifecyclePhaseV1::EventComplete,
            MtgoCompetitiveEventVisibleStatusV1::EventComplete,
        ) => Ok(LeagueEventComplete),
        (
            MtgoCompetitiveEventKindV1::Challenge,
            MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing,
            MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing,
        ) => Ok(ChallengeWaitingForPairing),
        (
            MtgoCompetitiveEventKindV1::Challenge,
            MtgoCompetitiveLifecyclePhaseV1::PairingReady,
            MtgoCompetitiveEventVisibleStatusV1::PairingReady,
        ) => Ok(ChallengePairingReady),
        (
            MtgoCompetitiveEventKindV1::Challenge,
            MtgoCompetitiveLifecyclePhaseV1::MatchComplete,
            MtgoCompetitiveEventVisibleStatusV1::BetweenMatches,
        ) => Ok(ChallengeBetweenMatches),
        (
            MtgoCompetitiveEventKindV1::Challenge,
            MtgoCompetitiveLifecyclePhaseV1::EventComplete,
            MtgoCompetitiveEventVisibleStatusV1::EventComplete,
        ) => Ok(ChallengeEventComplete),
        _ => Err(error(
            "event_record_slice",
            "event-record evaluation accepts only the eight canonical League and Challenge summary slices",
        )),
    }
}

fn compare_fields(
    expected_lifecycle: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    expected: &CheckedUntrustedMtgoCompetitiveEventRecordV1,
    prediction_lifecycle: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    prediction: &CheckedUntrustedMtgoCompetitiveEventRecordV1,
) -> Vec<(MtgoCompetitiveEventRecordFieldV1, bool)> {
    use MtgoCompetitiveEventRecordFieldV1::*;
    let mut results = vec![
        (
            LifecycleSnapshot,
            expected_lifecycle.snapshot_commitment_sha256()
                == prediction_lifecycle.snapshot_commitment_sha256(),
        ),
        (
            EventIdentity,
            expected.event_identity_sha256_v1() == prediction.event_identity_sha256_v1(),
        ),
        (
            EventKind,
            expected.event_kind_v1() == prediction.event_kind_v1(),
        ),
        (Status, expected.status_v1() == prediction.status_v1()),
        (
            MatchRecord,
            match_record(expected.progress_v1()) == match_record(prediction.progress_v1()),
        ),
        (
            ScheduleProgress,
            schedule_progress(expected.progress_v1())
                == schedule_progress(prediction.progress_v1()),
        ),
    ];
    if expected.event_kind_v1() == MtgoCompetitiveEventKindV1::Challenge {
        results.push((
            ChallengeMatchPoints,
            challenge_match_points(expected.progress_v1())
                == challenge_match_points(prediction.progress_v1()),
        ));
        results.push((
            ChallengeStanding,
            challenge_standing(expected.progress_v1())
                == challenge_standing(prediction.progress_v1()),
        ));
    }
    results.push((
        Completion,
        expected.completion_v1() == prediction.completion_v1(),
    ));
    results
}

fn match_record(progress: &MtgoCompetitiveEventProgressV1) -> &crate::MtgoCompetitiveMatchRecordV1 {
    match progress {
        MtgoCompetitiveEventProgressV1::League { match_record, .. }
        | MtgoCompetitiveEventProgressV1::Challenge { match_record, .. } => match_record,
    }
}

fn schedule_progress(progress: &MtgoCompetitiveEventProgressV1) -> (u8, u16, Option<u16>) {
    match progress {
        MtgoCompetitiveEventProgressV1::League { matches_total, .. } => (0, 0, *matches_total),
        MtgoCompetitiveEventProgressV1::Challenge {
            rounds_completed,
            rounds_total,
            ..
        } => (1, *rounds_completed, Some(*rounds_total)),
    }
}

fn challenge_match_points(progress: &MtgoCompetitiveEventProgressV1) -> Option<u16> {
    match progress {
        MtgoCompetitiveEventProgressV1::Challenge { match_points, .. } => Some(*match_points),
        MtgoCompetitiveEventProgressV1::League { .. } => None,
    }
}

fn challenge_standing(progress: &MtgoCompetitiveEventProgressV1) -> Option<(u32, u32)> {
    match progress {
        MtgoCompetitiveEventProgressV1::Challenge {
            standing_rank,
            field_size,
            ..
        } => standing_rank.zip(*field_size),
        MtgoCompetitiveEventProgressV1::League { .. } => None,
    }
}

fn validate_annotation_review(
    case: &MtgoCompetitiveEventRecordCorpusCaseV1,
) -> Result<(), MtgoContractErrorV1> {
    let common = case.annotated_at_unix_millis != 0
        && case.approved_account_identity_visually_confirmed
        && case.unobscured_frame_visually_confirmed
        && case.event_identity_visually_confirmed
        && case.event_kind_visually_confirmed
        && case.status_visually_confirmed
        && case.match_record_visually_confirmed
        && case.schedule_progress_visually_confirmed
        && case.completion_visually_confirmed;
    let challenge = matches!(
        case.slice,
        MtgoCompetitiveEventRecordSliceV1::ChallengeWaitingForPairing
            | MtgoCompetitiveEventRecordSliceV1::ChallengePairingReady
            | MtgoCompetitiveEventRecordSliceV1::ChallengeBetweenMatches
            | MtgoCompetitiveEventRecordSliceV1::ChallengeEventComplete
    );
    if !common
        || case.challenge_match_points_visually_confirmed != challenge
        || case.challenge_standing_visually_confirmed != challenge
    {
        return Err(error(
            "event_record_corpus_review",
            "every applicable event-record field must be visually confirmed",
        ));
    }
    Ok(())
}

fn validate_evaluation_spec(
    spec: &MtgoCompetitiveEventRecordEvaluationSpecV1,
) -> Result<(), MtgoContractErrorV1> {
    if spec.schema_version != MTGO_COMPETITIVE_EVENT_RECORD_EVALUATION_SCHEMA_V1 {
        return Err(error(
            "event_record_evaluation_schema",
            "schema must be version 1",
        ));
    }
    validate_identifier(&spec.evaluation_id, "event_record_evaluation_id")?;
    for (value, code) in [
        (
            &spec.navigation_profile_commitment_sha256,
            "event_record_evaluation_profile",
        ),
        (
            &spec.corpus_manifest_sha256,
            "event_record_evaluation_corpus",
        ),
        (
            &spec.annotation_protocol_sha256,
            "event_record_evaluation_annotation",
        ),
        (
            &spec.evaluator_binary_sha256,
            "event_record_evaluation_binary",
        ),
    ] {
        validate_sha256(value, code)?;
    }
    if spec.minimum_unique_cases_per_slice == 0 || spec.minimum_unique_cases_per_slice > 12_500 {
        return Err(error(
            "event_record_evaluation_minimum_cases",
            "minimum cases per slice must be between 1 and 12500",
        ));
    }
    if !(9_500..=10_000).contains(&spec.minimum_prediction_coverage_bps)
        || spec.minimum_field_accuracy_bps != 10_000
    {
        return Err(error(
            "event_record_evaluation_threshold",
            "coverage must be between 9500 and 10000 basis points and every non-abstained semantic field must be exact",
        ));
    }
    if spec.required_slices != REQUIRED_EVENT_RECORD_SLICES_V1
        || spec.required_fields != REQUIRED_EVENT_RECORD_FIELDS_V1
    {
        return Err(error(
            "event_record_evaluation_required_dimensions",
            "all eight slices and all nine semantic fields are required in canonical order",
        ));
    }
    Ok(())
}

fn ratio_bps(numerator: u32, denominator: u32) -> u16 {
    if denominator == 0 {
        return 0;
    }
    u16::try_from(u64::from(numerator) * 10_000 / u64::from(denominator)).unwrap_or(10_000)
}

fn validate_identifier(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(error(code, "identifier syntax is invalid"));
    }
    Ok(())
}

fn validate_sha256(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error(code, "expected lowercase SHA-256 hex"));
    }
    Ok(())
}

fn commitment(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hash_part(&mut hasher, part);
    }
    format!("{:x}", hasher.finalize())
}

fn hash_part(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn error(code: &'static str, message: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        canonical_competitive_lifecycle_slices_v1,
        check_untrusted_competitive_navigation_runtime_profile_v1,
        checked_untrusted_competitive_navigation_source_for_test_v1,
        MtgoCompetitiveEventCompletionV1, MtgoCompetitiveEventRecordVisibleFactKindV1,
        MtgoCompetitiveEventRecordVisibleFactV1, MtgoCompetitiveMatchRecordV1,
        MtgoCompetitiveNavigationRuntimeProfileV1, MtgoLifecycleVisibleFactKindV1,
        MtgoLifecycleVisibleFactV1, MtgoRectPxV1, MtgoSizePxV1,
        MTGO_COMPETITIVE_NAVIGATION_EVALUATION_SCHEMA_V1,
    };

    fn digest(byte: char) -> String {
        byte.to_string().repeat(64)
    }

    fn profile() -> CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1 {
        check_untrusted_competitive_navigation_runtime_profile_v1(
            MtgoCompetitiveNavigationRuntimeProfileV1 {
                schema_version: MTGO_COMPETITIVE_NAVIGATION_EVALUATION_SCHEMA_V1,
                profile_id: "event-record-evaluation-test-v1".to_owned(),
                executable_sha256: digest('a'),
                signer_thumbprint: "b".repeat(40),
                signer_subject_sha256: digest('c'),
                window_title_sha256: digest('9'),
                approved_account_alias_sha256: digest('2'),
                account_identity_rect_client_px: MtgoRectPxV1 {
                    x: 24,
                    y: 24,
                    width: 160,
                    height: 32,
                },
                account_identity_region_sha256: digest('3'),
                dpi: 120,
                client_size_px: MtgoSizePxV1 {
                    width: 1_550,
                    height: 925,
                },
                output_identity_sha256: digest('4'),
                canonical_pixel_format: "bgra8_unorm_top_down_tightly_packed_v1".to_owned(),
                classifier_binary_sha256: digest('5'),
                classifier_assets_manifest_sha256: digest('6'),
                supported_slices: canonical_competitive_lifecycle_slices_v1().to_vec(),
            },
        )
        .unwrap()
    }

    fn lifecycle(
        source: &CheckedUntrustedMtgoCompetitiveNavigationSourceV1,
        kind: MtgoCompetitiveEventKindV1,
        phase: MtgoCompetitiveLifecyclePhaseV1,
        frame_id: u64,
    ) -> MtgoVisibleCompetitiveLifecycleSnapshotV1 {
        let fact_kinds: &[MtgoLifecycleVisibleFactKindV1] = match phase {
            MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing => {
                &[MtgoLifecycleVisibleFactKindV1::EnteredEventVisible]
            }
            MtgoCompetitiveLifecyclePhaseV1::PairingReady => &[
                MtgoLifecycleVisibleFactKindV1::PairingVisible,
                MtgoLifecycleVisibleFactKindV1::PairingAcceptControlEnabled,
            ],
            MtgoCompetitiveLifecyclePhaseV1::MatchComplete => &[
                MtgoLifecycleVisibleFactKindV1::MatchResultVisible,
                MtgoLifecycleVisibleFactKindV1::MatchContinueControlEnabled,
            ],
            MtgoCompetitiveLifecyclePhaseV1::EventComplete => &[
                MtgoLifecycleVisibleFactKindV1::EventResultVisible,
                MtgoLifecycleVisibleFactKindV1::EventCloseControlEnabled,
            ],
            _ => unreachable!(),
        };
        let match_related = matches!(
            phase,
            MtgoCompetitiveLifecyclePhaseV1::PairingReady
                | MtgoCompetitiveLifecyclePhaseV1::MatchComplete
        );
        MtgoVisibleCompetitiveLifecycleSnapshotV1 {
            schema_version: 1,
            snapshot_id: format!("event-record-eval-source-{frame_id}"),
            event_kind: kind,
            phase,
            frame_id,
            frame_sequence: frame_id,
            frame_sha256: source.canonical_bgra8_sha256().to_owned(),
            client_bounds: MtgoRectPxV1 {
                x: 0,
                y: 0,
                width: 1_550,
                height: 925,
            },
            event_identity_sha256: Some(digest('f')),
            match_identity_sha256: match_related.then(|| digest('e')),
            game_number: None,
            entry_terms: None,
            visible_state_complete: true,
            facts: fact_kinds
                .iter()
                .enumerate()
                .map(|(index, kind)| MtgoLifecycleVisibleFactV1 {
                    kind: *kind,
                    rect_client_px: MtgoRectPxV1 {
                        x: 80 + u32::try_from(index).unwrap() * 20,
                        y: 80 + u32::try_from(index).unwrap() * 20,
                        width: 600,
                        height: 400,
                    },
                    content_sha256: format!("{:064x}", index + 13),
                    confidence_bps: 10_000,
                })
                .collect(),
        }
    }

    fn visible_fact(
        kind: MtgoCompetitiveEventRecordVisibleFactKindV1,
        index: u32,
    ) -> MtgoCompetitiveEventRecordVisibleFactV1 {
        MtgoCompetitiveEventRecordVisibleFactV1 {
            kind,
            rect_client_px: MtgoRectPxV1 {
                x: 100 + index * 220,
                y: 500,
                width: 180,
                height: 40,
            },
            content_sha256: format!("{:064x}", index + 1),
            confidence_bps: 10_000,
        }
    }

    fn record(
        profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
        lifecycle: &MtgoVisibleCompetitiveLifecycleSnapshotV1,
        index: usize,
    ) -> MtgoVisibleCompetitiveEventRecordV1 {
        let checked_lifecycle =
            validate_visible_competitive_lifecycle_snapshot_v1(lifecycle.clone()).unwrap();
        let status = match lifecycle.phase {
            MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing => {
                MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing
            }
            MtgoCompetitiveLifecyclePhaseV1::PairingReady => {
                MtgoCompetitiveEventVisibleStatusV1::PairingReady
            }
            MtgoCompetitiveLifecyclePhaseV1::MatchComplete => {
                MtgoCompetitiveEventVisibleStatusV1::BetweenMatches
            }
            MtgoCompetitiveLifecyclePhaseV1::EventComplete => {
                MtgoCompetitiveEventVisibleStatusV1::EventComplete
            }
            _ => unreachable!(),
        };
        let completed = match lifecycle.phase {
            MtgoCompetitiveLifecyclePhaseV1::MatchComplete => 1,
            MtgoCompetitiveLifecyclePhaseV1::EventComplete => {
                if lifecycle.event_kind == MtgoCompetitiveEventKindV1::League {
                    5
                } else {
                    8
                }
            }
            _ => 0,
        };
        let match_record = MtgoCompetitiveMatchRecordV1 {
            wins: completed,
            losses: 0,
            draws: 0,
            matches_completed: completed,
        };
        let progress = match lifecycle.event_kind {
            MtgoCompetitiveEventKindV1::League => MtgoCompetitiveEventProgressV1::League {
                match_record,
                matches_total: Some(5),
            },
            MtgoCompetitiveEventKindV1::Challenge => MtgoCompetitiveEventProgressV1::Challenge {
                match_record,
                rounds_completed: completed,
                rounds_total: 8,
                match_points: completed * 3,
                standing_rank: Some(if completed == 0 { 128 } else { 12 }),
                field_size: Some(128),
            },
        };
        let completion = (status == MtgoCompetitiveEventVisibleStatusV1::EventComplete)
            .then_some(MtgoCompetitiveEventCompletionV1::Completed);
        let mut facts = vec![
            visible_fact(
                MtgoCompetitiveEventRecordVisibleFactKindV1::EventStatusVisible,
                0,
            ),
            visible_fact(
                MtgoCompetitiveEventRecordVisibleFactKindV1::EventProgressVisible,
                1,
            ),
        ];
        if lifecycle.event_kind == MtgoCompetitiveEventKindV1::Challenge {
            facts.push(visible_fact(
                MtgoCompetitiveEventRecordVisibleFactKindV1::EventStandingVisible,
                2,
            ));
        }
        if completion.is_some() {
            facts.push(visible_fact(
                MtgoCompetitiveEventRecordVisibleFactKindV1::EventResultVisible,
                3,
            ));
        }
        MtgoVisibleCompetitiveEventRecordV1 {
            schema_version: 1,
            record_id: format!("event-record-eval-{index:02}"),
            source_lifecycle_snapshot_commitment_sha256: checked_lifecycle
                .snapshot_commitment_sha256()
                .to_owned(),
            approved_account_alias_sha256: profile.approved_account_alias_sha256().to_owned(),
            event_kind: lifecycle.event_kind,
            lifecycle_phase: lifecycle.phase,
            frame_id: lifecycle.frame_id,
            frame_sequence: lifecycle.frame_sequence,
            frame_sha256: lifecycle.frame_sha256.clone(),
            client_bounds: lifecycle.client_bounds.clone(),
            event_identity_sha256: lifecycle.event_identity_sha256.clone().unwrap(),
            status,
            progress,
            completion,
            visible_record_complete: true,
            facts,
        }
    }

    fn run_evaluation(
        changed_match_points: bool,
        abstain_index: Option<usize>,
        changed_lifecycle: bool,
    ) -> CheckedUntrustedMtgoCompetitiveEventRecordEvaluationV1 {
        let profile = profile();
        let sources = (1_u8..=8)
            .map(|value| {
                checked_untrusted_competitive_navigation_source_for_test_v1(&profile, value)
            })
            .collect::<Vec<_>>();
        let dimensions = [
            (
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing,
            ),
            (
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveLifecyclePhaseV1::PairingReady,
            ),
            (
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveLifecyclePhaseV1::MatchComplete,
            ),
            (
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveLifecyclePhaseV1::EventComplete,
            ),
            (
                MtgoCompetitiveEventKindV1::Challenge,
                MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing,
            ),
            (
                MtgoCompetitiveEventKindV1::Challenge,
                MtgoCompetitiveLifecyclePhaseV1::PairingReady,
            ),
            (
                MtgoCompetitiveEventKindV1::Challenge,
                MtgoCompetitiveLifecyclePhaseV1::MatchComplete,
            ),
            (
                MtgoCompetitiveEventKindV1::Challenge,
                MtgoCompetitiveLifecyclePhaseV1::EventComplete,
            ),
        ];
        let lifecycles = dimensions
            .iter()
            .enumerate()
            .map(|(index, (kind, phase))| {
                lifecycle(&sources[index], *kind, *phase, (index + 1) as u64)
            })
            .collect::<Vec<_>>();
        let expected_records = lifecycles
            .iter()
            .enumerate()
            .map(|(index, value)| record(&profile, value, index))
            .collect::<Vec<_>>();
        let predictions = lifecycles
            .iter()
            .enumerate()
            .map(|(index, value)| {
                if abstain_index == Some(index) {
                    return None;
                }
                let mut predicted_lifecycle = value.clone();
                if changed_lifecycle && index == 1 {
                    predicted_lifecycle.snapshot_id =
                        "event-record-eval-alternate-lifecycle".to_owned();
                }
                let mut predicted_record = if changed_lifecycle && index == 1 {
                    record(&profile, &predicted_lifecycle, index)
                } else {
                    expected_records[index].clone()
                };
                if changed_match_points && index == 6 {
                    if let MtgoCompetitiveEventProgressV1::Challenge { match_points, .. } =
                        &mut predicted_record.progress
                    {
                        *match_points += 1;
                    }
                }
                Some(
                    check_untrusted_competitive_event_record_prediction_v1(
                        &profile,
                        &sources[index],
                        MtgoCompetitiveEventRecordPredictionV1 {
                            schema_version: 1,
                            navigation_profile_commitment_sha256: profile
                                .profile_commitment_sha256()
                                .to_owned(),
                            source_manifest_sha256: sources[index].manifest_sha256().to_owned(),
                            source_canonical_bgra8_sha256: sources[index]
                                .canonical_bgra8_sha256()
                                .to_owned(),
                            source_profile_binding_sha256: sources[index]
                                .source_profile_binding_sha256()
                                .to_owned(),
                            classifier_request_sha256: format!("{:064x}", index + 20),
                            classifier_response_sha256: format!("{:064x}", index + 40),
                            lifecycle: predicted_lifecycle,
                            record: predicted_record,
                        },
                    )
                    .unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let cases = (0..8)
            .map(|index| MtgoCompetitiveEventRecordEvaluationCaseV1 {
                case_id: format!("event-record-case-{index:02}"),
                source: &sources[index],
                expected_lifecycle: lifecycles[index].clone(),
                expected_record: expected_records[index].clone(),
                prediction: predictions[index].as_ref(),
            })
            .collect::<Vec<_>>();
        let corpus_cases = cases
            .iter()
            .map(|case| {
                let checked_lifecycle = validate_visible_competitive_lifecycle_snapshot_v1(
                    case.expected_lifecycle.clone(),
                )
                .unwrap();
                let checked_record = validate_visible_competitive_event_record_v1(
                    &checked_lifecycle,
                    profile.approved_account_alias_sha256(),
                    case.expected_record.clone(),
                )
                .unwrap();
                let slice = event_record_slice(
                    checked_lifecycle.event_kind(),
                    checked_lifecycle.phase(),
                    checked_record.status_v1(),
                )
                .unwrap();
                let challenge =
                    checked_lifecycle.event_kind() == MtgoCompetitiveEventKindV1::Challenge;
                MtgoCompetitiveEventRecordCorpusCaseV1 {
                    case_id: case.case_id.clone(),
                    slice,
                    source_manifest_sha256: case.source.manifest_sha256().to_owned(),
                    source_canonical_bgra8_sha256: case.source.canonical_bgra8_sha256().to_owned(),
                    source_profile_binding_sha256: case
                        .source
                        .source_profile_binding_sha256()
                        .to_owned(),
                    expected_lifecycle_snapshot_commitment_sha256: checked_lifecycle
                        .snapshot_commitment_sha256()
                        .to_owned(),
                    expected_event_record_commitment_sha256: checked_record
                        .record_commitment_sha256_v1()
                        .to_owned(),
                    annotator_alias_sha256: digest('7'),
                    annotation_receipt_sha256: commitment(
                        b"event-record-test-annotation-v1",
                        &[case.case_id.as_bytes()],
                    ),
                    annotated_at_unix_millis: 1_786_350_000_000,
                    approved_account_identity_visually_confirmed: true,
                    unobscured_frame_visually_confirmed: true,
                    event_identity_visually_confirmed: true,
                    event_kind_visually_confirmed: true,
                    status_visually_confirmed: true,
                    match_record_visually_confirmed: true,
                    schedule_progress_visually_confirmed: true,
                    challenge_match_points_visually_confirmed: challenge,
                    challenge_standing_visually_confirmed: challenge,
                    completion_visually_confirmed: true,
                }
            })
            .collect::<Vec<_>>();
        let corpus = check_untrusted_competitive_event_record_corpus_manifest_v1(
            &profile,
            MtgoCompetitiveEventRecordCorpusManifestV1 {
                schema_version: 1,
                corpus_id: "event-record-evaluation-corpus-v1".to_owned(),
                navigation_profile_commitment_sha256: profile
                    .profile_commitment_sha256()
                    .to_owned(),
                approved_account_alias_sha256: profile.approved_account_alias_sha256().to_owned(),
                annotation_protocol_sha256: digest('8'),
                cases: corpus_cases,
            },
        )
        .unwrap();
        evaluate_untrusted_competitive_event_record_profile_v1(
            &profile,
            &corpus,
            MtgoCompetitiveEventRecordEvaluationSpecV1 {
                schema_version: 1,
                evaluation_id: "event-record-evaluation-v1".to_owned(),
                navigation_profile_commitment_sha256: profile
                    .profile_commitment_sha256()
                    .to_owned(),
                corpus_manifest_sha256: corpus.manifest_sha256_v1().to_owned(),
                annotation_protocol_sha256: digest('8'),
                evaluator_binary_sha256: digest('9'),
                minimum_unique_cases_per_slice: 1,
                minimum_prediction_coverage_bps: 10_000,
                minimum_field_accuracy_bps: 10_000,
                required_slices: REQUIRED_EVENT_RECORD_SLICES_V1.to_vec(),
                required_fields: REQUIRED_EVENT_RECORD_FIELDS_V1.to_vec(),
            },
            cases,
        )
        .unwrap()
    }

    #[test]
    fn exact_eight_slice_evaluation_passes_without_runtime_authority() {
        let evaluation = run_evaluation(false, None, false);
        assert_eq!(evaluation.unique_case_count_v1(), 8);
        assert_eq!(evaluation.prediction_count_v1(), 8);
        assert_eq!(evaluation.exact_semantic_prediction_count_v1(), 8);
        assert_eq!(evaluation.prediction_coverage_bps_v1(), 10_000);
        assert_eq!(evaluation.minimum_field_accuracy_bps_v1(), 10_000);
        assert_eq!(evaluation.field_accuracy_v1().len(), 9);
        assert!(evaluation.missing_slices_v1().is_empty());
        assert!(evaluation.passes_declared_gate_v1());
        assert!(!evaluation.safe_for_live_classification_v1());
        assert!(!evaluation.permits_event_entry_v1());
        assert!(!evaluation.permits_spending_v1());
        assert!(!evaluation.safe_for_input_v1());
    }

    #[test]
    fn one_wrong_challenge_points_value_fails_the_per_field_gate() {
        let evaluation = run_evaluation(true, None, false);
        let points = evaluation
            .field_accuracy_v1()
            .iter()
            .find(|metric| metric.field == MtgoCompetitiveEventRecordFieldV1::ChallengeMatchPoints)
            .unwrap();
        assert_eq!(points.evaluated_predictions, 4);
        assert_eq!(points.exact_predictions, 3);
        assert_eq!(points.accuracy_bps, 7_500);
        assert_eq!(evaluation.exact_semantic_prediction_count_v1(), 7);
        assert!(!evaluation.passes_declared_gate_v1());
    }

    #[test]
    fn one_slice_abstention_cannot_hide_in_aggregate_coverage() {
        let evaluation = run_evaluation(false, Some(0), false);
        assert_eq!(evaluation.prediction_count_v1(), 7);
        assert_eq!(evaluation.minimum_prediction_coverage_bps_per_slice_v1(), 0);
        assert!(!evaluation.passes_declared_gate_v1());
    }

    #[test]
    fn alternate_lifecycle_commitment_fails_its_own_exact_field() {
        let evaluation = run_evaluation(false, None, true);
        let lifecycle = evaluation
            .field_accuracy_v1()
            .iter()
            .find(|metric| metric.field == MtgoCompetitiveEventRecordFieldV1::LifecycleSnapshot)
            .unwrap();
        assert_eq!(lifecycle.evaluated_predictions, 8);
        assert_eq!(lifecycle.exact_predictions, 7);
        assert!(!evaluation.passes_declared_gate_v1());
    }

    #[test]
    fn challenge_review_requires_points_and_standing_confirmation() {
        let profile = profile();
        let source = checked_untrusted_competitive_navigation_source_for_test_v1(&profile, 1);
        let lifecycle = lifecycle(
            &source,
            MtgoCompetitiveEventKindV1::Challenge,
            MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing,
            1,
        );
        let record = record(&profile, &lifecycle, 0);
        let checked_lifecycle =
            validate_visible_competitive_lifecycle_snapshot_v1(lifecycle).unwrap();
        let checked_record = validate_visible_competitive_event_record_v1(
            &checked_lifecycle,
            profile.approved_account_alias_sha256(),
            record,
        )
        .unwrap();
        let case = MtgoCompetitiveEventRecordCorpusCaseV1 {
            case_id: "challenge-review-case".to_owned(),
            slice: MtgoCompetitiveEventRecordSliceV1::ChallengeWaitingForPairing,
            source_manifest_sha256: source.manifest_sha256().to_owned(),
            source_canonical_bgra8_sha256: source.canonical_bgra8_sha256().to_owned(),
            source_profile_binding_sha256: source.source_profile_binding_sha256().to_owned(),
            expected_lifecycle_snapshot_commitment_sha256: checked_lifecycle
                .snapshot_commitment_sha256()
                .to_owned(),
            expected_event_record_commitment_sha256: checked_record
                .record_commitment_sha256_v1()
                .to_owned(),
            annotator_alias_sha256: digest('7'),
            annotation_receipt_sha256: digest('8'),
            annotated_at_unix_millis: 1,
            approved_account_identity_visually_confirmed: true,
            unobscured_frame_visually_confirmed: true,
            event_identity_visually_confirmed: true,
            event_kind_visually_confirmed: true,
            status_visually_confirmed: true,
            match_record_visually_confirmed: true,
            schedule_progress_visually_confirmed: true,
            challenge_match_points_visually_confirmed: false,
            challenge_standing_visually_confirmed: true,
            completion_visually_confirmed: true,
        };
        let error = check_untrusted_competitive_event_record_corpus_manifest_v1(
            &profile,
            MtgoCompetitiveEventRecordCorpusManifestV1 {
                schema_version: 1,
                corpus_id: "challenge-review-corpus".to_owned(),
                navigation_profile_commitment_sha256: profile
                    .profile_commitment_sha256()
                    .to_owned(),
                approved_account_alias_sha256: profile.approved_account_alias_sha256().to_owned(),
                annotation_protocol_sha256: digest('9'),
                cases: vec![case],
            },
        )
        .err()
        .unwrap();
        assert_eq!(error.code(), "event_record_corpus_review");
    }
}
