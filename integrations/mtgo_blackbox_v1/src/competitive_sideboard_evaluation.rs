use crate::{
    validate_snapshot_artifact_source_v1, validate_source_profile_v1,
    validate_visible_competitive_lifecycle_snapshot_v1,
    validate_visible_competitive_sideboard_snapshot_v1,
    CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    CheckedUntrustedMtgoCompetitiveNavigationSourceV1,
    CheckedUntrustedMtgoCompetitiveSideboardSnapshotV1, MtgoCompetitiveEventKindV1,
    MtgoContractErrorV1, MtgoVisibleCompetitiveLifecycleSnapshotV1,
    MtgoVisibleCompetitiveSideboardSnapshotV1, ValidatedMtgoCompetitiveDeckManifestV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

pub const MTGO_COMPETITIVE_SIDEBOARD_EVALUATION_SCHEMA_V1: u32 = 1;

const SIDEBOARD_CORPUS_DOMAIN_V1: &[u8] = b"mtgo-competitive-sideboard-corpus-v1";
const SIDEBOARD_PREDICTION_DOMAIN_V1: &[u8] = b"mtgo-competitive-sideboard-prediction-v1";
const SIDEBOARD_EVALUATION_DOMAIN_V1: &[u8] = b"mtgo-competitive-sideboard-evaluation-v1";
const SIDEBOARD_EVALUATION_RATIFICATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-sideboard-evaluation-ratification-v1";
const SIDEBOARD_EVALUATION_ADMISSION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-sideboard-evaluation-admission-v1";
pub(crate) const RATIFIED_COMPETITIVE_SIDEBOARD_EVALUATION_COMMITMENT_V1: Option<&str> = None;

struct SideboardIdentitySetV1<'a> {
    profile_commitment_sha256: &'a str,
    approved_account_alias_sha256: &'a str,
    deck_list_sha256: &'a str,
    deck_manifest_commitment_sha256: &'a str,
    deck_format_sha256: &'a str,
    policy_deployment_commitment_sha256: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveSideboardEvaluationSliceV1 {
    LeagueNoChanges,
    LeagueChangedConfiguration,
    ChallengeNoChanges,
    ChallengeChangedConfiguration,
}

const REQUIRED_SIDEBOARD_SLICES_V1: [MtgoCompetitiveSideboardEvaluationSliceV1; 4] = [
    MtgoCompetitiveSideboardEvaluationSliceV1::LeagueNoChanges,
    MtgoCompetitiveSideboardEvaluationSliceV1::LeagueChangedConfiguration,
    MtgoCompetitiveSideboardEvaluationSliceV1::ChallengeNoChanges,
    MtgoCompetitiveSideboardEvaluationSliceV1::ChallengeChangedConfiguration,
];

pub fn canonical_competitive_sideboard_evaluation_slices_v1(
) -> &'static [MtgoCompetitiveSideboardEvaluationSliceV1] {
    &REQUIRED_SIDEBOARD_SLICES_V1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveSideboardEvaluationSpecV1 {
    pub schema_version: u32,
    pub evaluation_id: String,
    pub profile_commitment_sha256: String,
    pub corpus_manifest_sha256: String,
    pub annotation_protocol_sha256: String,
    pub evaluator_binary_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub minimum_unique_cases_per_slice: u32,
    pub minimum_prediction_coverage_bps: u16,
    pub minimum_exact_accuracy_bps: u16,
    pub required_slices: Vec<MtgoCompetitiveSideboardEvaluationSliceV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveSideboardCorpusCaseV1 {
    pub case_id: String,
    pub slice: MtgoCompetitiveSideboardEvaluationSliceV1,
    pub source_manifest_sha256: String,
    pub source_canonical_bgra8_sha256: String,
    pub source_profile_binding_sha256: String,
    pub expected_lifecycle_snapshot_commitment_sha256: String,
    pub expected_sideboard_snapshot_commitment_sha256: String,
    pub annotator_alias_sha256: String,
    pub annotation_receipt_sha256: String,
    pub annotated_at_unix_millis: u64,
    pub unobscured_frame_visually_confirmed: bool,
    pub event_and_game_identity_visually_confirmed: bool,
    pub all_visible_card_rows_annotated: bool,
    pub zone_and_empty_drop_regions_annotated: bool,
    pub submit_control_visually_confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveSideboardCorpusManifestV1 {
    pub schema_version: u32,
    pub corpus_id: String,
    pub profile_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub annotation_protocol_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub cases: Vec<MtgoCompetitiveSideboardCorpusCaseV1>,
}

/// Commitment-only review of exact sideboard frames. It cannot classify,
/// submit, enter, spend, or input.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoCompetitiveSideboardCorpusManifestV1;
/// fn cannot_act(value: &CheckedUntrustedMtgoCompetitiveSideboardCorpusManifestV1) {
///     let _ = value.submit_sideboard();
///     let _ = value.input_command();
/// }
/// ```
pub struct CheckedUntrustedMtgoCompetitiveSideboardCorpusManifestV1 {
    manifest: MtgoCompetitiveSideboardCorpusManifestV1,
    manifest_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveSideboardCorpusManifestV1 {
    pub fn manifest_sha256_v1(&self) -> &str {
        &self.manifest_sha256
    }

    pub fn case_count_v1(&self) -> usize {
        self.manifest.cases.len()
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveSideboardPredictionV1 {
    pub schema_version: u32,
    pub profile_commitment_sha256: String,
    pub source_manifest_sha256: String,
    pub source_canonical_bgra8_sha256: String,
    pub source_profile_binding_sha256: String,
    pub classifier_request_sha256: String,
    pub classifier_response_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub lifecycle: MtgoVisibleCompetitiveLifecycleSnapshotV1,
    pub sideboard: MtgoVisibleCompetitiveSideboardSnapshotV1,
}

pub struct CheckedUntrustedMtgoCompetitiveSideboardPredictionV1 {
    record: MtgoCompetitiveSideboardPredictionV1,
    sideboard: CheckedUntrustedMtgoCompetitiveSideboardSnapshotV1,
    prediction_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveSideboardPredictionV1 {
    pub fn prediction_commitment_sha256_v1(&self) -> &str {
        &self.prediction_commitment_sha256
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

pub struct MtgoCompetitiveSideboardEvaluationCaseV1<'a> {
    pub case_id: String,
    pub source: &'a CheckedUntrustedMtgoCompetitiveNavigationSourceV1,
    pub expected_lifecycle: MtgoVisibleCompetitiveLifecycleSnapshotV1,
    pub expected_sideboard: MtgoVisibleCompetitiveSideboardSnapshotV1,
    pub prediction: Option<&'a CheckedUntrustedMtgoCompetitiveSideboardPredictionV1>,
}

pub struct CheckedUntrustedMtgoCompetitiveSideboardEvaluationV1 {
    profile_commitment_sha256: String,
    approved_account_alias_sha256: String,
    deck_list_sha256: String,
    deck_manifest_commitment_sha256: String,
    deck_format_sha256: String,
    policy_deployment_commitment_sha256: String,
    corpus_manifest_sha256: String,
    evaluation_commitment_sha256: String,
    unique_case_count: u32,
    prediction_count: u32,
    exact_prediction_count: u32,
    prediction_coverage_bps: u16,
    exact_accuracy_bps: u16,
    minimum_observed_cases_per_slice: u32,
    minimum_prediction_coverage_bps_per_slice: u16,
    minimum_exact_accuracy_bps_per_slice: u16,
    missing_slices: Vec<MtgoCompetitiveSideboardEvaluationSliceV1>,
    passes_declared_gate: bool,
}

impl CheckedUntrustedMtgoCompetitiveSideboardEvaluationV1 {
    pub fn evaluation_commitment_sha256_v1(&self) -> &str {
        &self.evaluation_commitment_sha256
    }

    pub fn unique_case_count_v1(&self) -> u32 {
        self.unique_case_count
    }

    pub fn prediction_count_v1(&self) -> u32 {
        self.prediction_count
    }

    pub fn exact_prediction_count_v1(&self) -> u32 {
        self.exact_prediction_count
    }

    pub fn prediction_coverage_bps_v1(&self) -> u16 {
        self.prediction_coverage_bps
    }

    pub fn exact_accuracy_bps_v1(&self) -> u16 {
        self.exact_accuracy_bps
    }

    pub fn minimum_observed_cases_per_slice_v1(&self) -> u32 {
        self.minimum_observed_cases_per_slice
    }

    pub fn minimum_prediction_coverage_bps_per_slice_v1(&self) -> u16 {
        self.minimum_prediction_coverage_bps_per_slice
    }

    pub fn minimum_exact_accuracy_bps_per_slice_v1(&self) -> u16 {
        self.minimum_exact_accuracy_bps_per_slice
    }

    pub fn missing_slices_v1(&self) -> &[MtgoCompetitiveSideboardEvaluationSliceV1] {
        &self.missing_slices
    }

    pub fn passes_declared_gate_v1(&self) -> bool {
        self.passes_declared_gate
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoReviewedCompetitiveSideboardEvaluationRatificationCandidateV1 {
    pub profile_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub corpus_manifest_sha256: String,
    pub evaluation_commitment_sha256: String,
    pub ratification_commitment_sha256: String,
}

pub struct AdmittedMtgoCompetitiveSideboardEvaluationV1 {
    commitments: MtgoReviewedCompetitiveSideboardEvaluationRatificationCandidateV1,
    admission_commitment_sha256: String,
}

impl AdmittedMtgoCompetitiveSideboardEvaluationV1 {
    pub fn commitments_v1(
        &self,
    ) -> MtgoReviewedCompetitiveSideboardEvaluationRatificationCandidateV1 {
        self.commitments.clone()
    }

    pub fn admission_commitment_sha256_v1(&self) -> &str {
        &self.admission_commitment_sha256
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

pub fn check_untrusted_competitive_sideboard_corpus_manifest_v1(
    profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    policy_deployment_commitment_sha256: &str,
    manifest: MtgoCompetitiveSideboardCorpusManifestV1,
) -> Result<CheckedUntrustedMtgoCompetitiveSideboardCorpusManifestV1, MtgoContractErrorV1> {
    if manifest.schema_version != MTGO_COMPETITIVE_SIDEBOARD_EVALUATION_SCHEMA_V1 {
        return Err(error_v1(
            "sideboard_corpus_schema",
            "expected schema version 1",
        ));
    }
    validate_identifier_v1(&manifest.corpus_id, "sideboard_corpus_id")?;
    validate_identity_set_v1(
        profile,
        deck,
        policy_deployment_commitment_sha256,
        SideboardIdentitySetV1 {
            profile_commitment_sha256: &manifest.profile_commitment_sha256,
            approved_account_alias_sha256: &manifest.approved_account_alias_sha256,
            deck_list_sha256: &manifest.deck_list_sha256,
            deck_manifest_commitment_sha256: &manifest.deck_manifest_commitment_sha256,
            deck_format_sha256: &manifest.deck_format_sha256,
            policy_deployment_commitment_sha256: &manifest.policy_deployment_commitment_sha256,
        },
    )?;
    validate_sha256_v1(
        &manifest.annotation_protocol_sha256,
        "sideboard_corpus_annotation",
    )?;
    if manifest.cases.len() < REQUIRED_SIDEBOARD_SLICES_V1.len() || manifest.cases.len() > 100_000 {
        return Err(error_v1(
            "sideboard_corpus_case_count",
            "corpus must contain between four and 100000 reviewed cases",
        ));
    }
    let mut prior_case_id: Option<&str> = None;
    let mut source_manifests = HashSet::new();
    let mut source_frames = HashSet::new();
    for case in &manifest.cases {
        validate_identifier_v1(&case.case_id, "sideboard_corpus_case_id")?;
        if prior_case_id.is_some_and(|prior| prior >= case.case_id.as_str()) {
            return Err(error_v1(
                "sideboard_corpus_case_order",
                "case IDs must be unique and strictly increasing",
            ));
        }
        prior_case_id = Some(&case.case_id);
        for (digest, code) in [
            (
                &case.source_manifest_sha256,
                "sideboard_corpus_source_manifest",
            ),
            (
                &case.source_canonical_bgra8_sha256,
                "sideboard_corpus_source_frame",
            ),
            (
                &case.source_profile_binding_sha256,
                "sideboard_corpus_source_profile",
            ),
            (
                &case.expected_lifecycle_snapshot_commitment_sha256,
                "sideboard_corpus_lifecycle",
            ),
            (
                &case.expected_sideboard_snapshot_commitment_sha256,
                "sideboard_corpus_sideboard",
            ),
            (&case.annotator_alias_sha256, "sideboard_corpus_annotator"),
            (&case.annotation_receipt_sha256, "sideboard_corpus_receipt"),
        ] {
            validate_sha256_v1(digest, code)?;
        }
        if !source_manifests.insert(case.source_manifest_sha256.as_str())
            || !source_frames.insert(case.source_canonical_bgra8_sha256.as_str())
        {
            return Err(error_v1(
                "sideboard_corpus_duplicate_source",
                "each reviewed case must bind distinct manifest and frame commitments",
            ));
        }
        if case.annotated_at_unix_millis == 0
            || !case.unobscured_frame_visually_confirmed
            || !case.event_and_game_identity_visually_confirmed
            || !case.all_visible_card_rows_annotated
            || !case.zone_and_empty_drop_regions_annotated
            || !case.submit_control_visually_confirmed
        {
            return Err(error_v1(
                "sideboard_corpus_visual_review",
                "every sideboard case requires the complete visual review receipt",
            ));
        }
    }
    let bytes = serde_json::to_vec(&manifest)
        .map_err(|error| error_v1("sideboard_corpus_serialization", error.to_string()))?;
    let manifest_sha256 = commitment_v1(SIDEBOARD_CORPUS_DOMAIN_V1, &[bytes.as_slice()]);
    Ok(CheckedUntrustedMtgoCompetitiveSideboardCorpusManifestV1 {
        manifest,
        manifest_sha256,
    })
}

pub fn check_untrusted_competitive_sideboard_prediction_v1(
    profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    source: &CheckedUntrustedMtgoCompetitiveNavigationSourceV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    policy_deployment_commitment_sha256: &str,
    prediction: MtgoCompetitiveSideboardPredictionV1,
) -> Result<CheckedUntrustedMtgoCompetitiveSideboardPredictionV1, MtgoContractErrorV1> {
    if prediction.schema_version != MTGO_COMPETITIVE_SIDEBOARD_EVALUATION_SCHEMA_V1 {
        return Err(error_v1(
            "sideboard_prediction_schema",
            "expected schema version 1",
        ));
    }
    validate_source_profile_v1(profile, source)?;
    validate_identity_set_v1(
        profile,
        deck,
        policy_deployment_commitment_sha256,
        SideboardIdentitySetV1 {
            profile_commitment_sha256: &prediction.profile_commitment_sha256,
            approved_account_alias_sha256: profile.approved_account_alias_sha256(),
            deck_list_sha256: &prediction.deck_list_sha256,
            deck_manifest_commitment_sha256: &prediction.deck_manifest_commitment_sha256,
            deck_format_sha256: &prediction.deck_format_sha256,
            policy_deployment_commitment_sha256: &prediction.policy_deployment_commitment_sha256,
        },
    )?;
    for (digest, code) in [
        (
            &prediction.source_manifest_sha256,
            "sideboard_prediction_manifest",
        ),
        (
            &prediction.source_canonical_bgra8_sha256,
            "sideboard_prediction_frame",
        ),
        (
            &prediction.source_profile_binding_sha256,
            "sideboard_prediction_profile",
        ),
        (
            &prediction.classifier_request_sha256,
            "sideboard_prediction_request",
        ),
        (
            &prediction.classifier_response_sha256,
            "sideboard_prediction_response",
        ),
    ] {
        validate_sha256_v1(digest, code)?;
    }
    if prediction.source_manifest_sha256 != source.manifest_sha256()
        || prediction.source_canonical_bgra8_sha256 != source.canonical_bgra8_sha256()
        || prediction.source_profile_binding_sha256 != source.source_profile_binding_sha256()
        || prediction.classifier_request_sha256 == prediction.classifier_response_sha256
    {
        return Err(error_v1(
            "sideboard_prediction_source",
            "prediction must bind the exact source and distinct classifier exchange",
        ));
    }
    let lifecycle =
        validate_visible_competitive_lifecycle_snapshot_v1(prediction.lifecycle.clone())?;
    validate_snapshot_artifact_source_v1(&lifecycle, source)?;
    let sideboard = validate_visible_competitive_sideboard_snapshot_v1(
        lifecycle,
        deck,
        prediction.sideboard.clone(),
    )?;
    let bytes = serde_json::to_vec(&prediction)
        .map_err(|error| error_v1("sideboard_prediction_serialization", error.to_string()))?;
    let prediction_commitment_sha256 =
        commitment_v1(SIDEBOARD_PREDICTION_DOMAIN_V1, &[bytes.as_slice()]);
    Ok(CheckedUntrustedMtgoCompetitiveSideboardPredictionV1 {
        record: prediction,
        sideboard,
        prediction_commitment_sha256,
    })
}

pub fn evaluate_untrusted_competitive_sideboard_profile_v1(
    profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    corpus: &CheckedUntrustedMtgoCompetitiveSideboardCorpusManifestV1,
    spec: MtgoCompetitiveSideboardEvaluationSpecV1,
    cases: Vec<MtgoCompetitiveSideboardEvaluationCaseV1<'_>>,
) -> Result<CheckedUntrustedMtgoCompetitiveSideboardEvaluationV1, MtgoContractErrorV1> {
    validate_evaluation_spec_v1(&spec)?;
    validate_identity_set_v1(
        profile,
        deck,
        &corpus.manifest.policy_deployment_commitment_sha256,
        SideboardIdentitySetV1 {
            profile_commitment_sha256: &spec.profile_commitment_sha256,
            approved_account_alias_sha256: &corpus.manifest.approved_account_alias_sha256,
            deck_list_sha256: &spec.deck_list_sha256,
            deck_manifest_commitment_sha256: &spec.deck_manifest_commitment_sha256,
            deck_format_sha256: &spec.deck_format_sha256,
            policy_deployment_commitment_sha256: &spec.policy_deployment_commitment_sha256,
        },
    )?;
    if corpus.manifest.profile_commitment_sha256 != profile.profile_commitment_sha256()
        || corpus.manifest.approved_account_alias_sha256 != profile.approved_account_alias_sha256()
        || spec.corpus_manifest_sha256 != corpus.manifest_sha256
        || spec.annotation_protocol_sha256 != corpus.manifest.annotation_protocol_sha256
        || spec.deck_list_sha256 != corpus.manifest.deck_list_sha256
        || spec.deck_manifest_commitment_sha256 != corpus.manifest.deck_manifest_commitment_sha256
        || spec.deck_format_sha256 != corpus.manifest.deck_format_sha256
        || spec.policy_deployment_commitment_sha256
            != corpus.manifest.policy_deployment_commitment_sha256
    {
        return Err(error_v1(
            "sideboard_evaluation_identity",
            "evaluation must bind the exact profile, account, corpus, annotation, deck, format, and policy",
        ));
    }
    if cases.is_empty() || cases.len() > 100_000 || cases.len() != corpus.manifest.cases.len() {
        return Err(error_v1(
            "sideboard_evaluation_case_count",
            format!(
                "evaluation has {} cases while the reviewed corpus has {}",
                cases.len(),
                corpus.manifest.cases.len()
            ),
        ));
    }

    let spec_bytes = serde_json::to_vec(&spec)
        .map_err(|error| error_v1("sideboard_evaluation_spec_serialization", error.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(SIDEBOARD_EVALUATION_DOMAIN_V1);
    hash_part_v1(&mut hasher, &spec_bytes);

    let mut previous_case_id: Option<&str> = None;
    let mut source_manifests = HashSet::new();
    let mut source_frames = HashSet::new();
    let mut slice_counts = HashMap::new();
    let mut slice_prediction_counts = HashMap::new();
    let mut slice_exact_counts = HashMap::new();
    let mut prediction_count = 0_u32;
    let mut exact_prediction_count = 0_u32;

    for (case, reviewed) in cases.iter().zip(&corpus.manifest.cases) {
        validate_identifier_v1(&case.case_id, "sideboard_evaluation_case_id")?;
        if previous_case_id.is_some_and(|previous| previous >= case.case_id.as_str()) {
            return Err(error_v1(
                "sideboard_evaluation_case_order",
                "case IDs must be unique and strictly increasing",
            ));
        }
        previous_case_id = Some(&case.case_id);
        if !source_manifests.insert(case.source.manifest_sha256())
            || !source_frames.insert(case.source.canonical_bgra8_sha256())
        {
            return Err(error_v1(
                "sideboard_evaluation_duplicate_source",
                "each case must use a distinct reviewed source and frame",
            ));
        }
        validate_source_profile_v1(profile, case.source)?;
        let lifecycle =
            validate_visible_competitive_lifecycle_snapshot_v1(case.expected_lifecycle.clone())?;
        validate_snapshot_artifact_source_v1(&lifecycle, case.source)?;
        let expected = validate_visible_competitive_sideboard_snapshot_v1(
            lifecycle,
            deck,
            case.expected_sideboard.clone(),
        )?;
        let slice = sideboard_slice_v1(expected.event_kind(), expected.configuration_v1(), deck);
        if reviewed.case_id != case.case_id
            || reviewed.slice != slice
            || reviewed.source_manifest_sha256 != case.source.manifest_sha256()
            || reviewed.source_canonical_bgra8_sha256 != case.source.canonical_bgra8_sha256()
            || reviewed.source_profile_binding_sha256 != case.source.source_profile_binding_sha256()
            || reviewed.expected_lifecycle_snapshot_commitment_sha256
                != expected.lifecycle_snapshot_commitment_sha256()
            || reviewed.expected_sideboard_snapshot_commitment_sha256
                != expected.snapshot_commitment_sha256()
        {
            return Err(error_v1(
                "sideboard_evaluation_corpus_case_binding",
                "evaluated case does not exactly match its reviewed corpus record",
            ));
        }
        *slice_counts.entry(slice).or_insert(0_u32) += 1;

        let (prediction_commitment, exact) = if let Some(prediction) = case.prediction {
            if prediction.record.profile_commitment_sha256 != profile.profile_commitment_sha256()
                || prediction.record.source_manifest_sha256 != case.source.manifest_sha256()
                || prediction.record.source_canonical_bgra8_sha256
                    != case.source.canonical_bgra8_sha256()
                || prediction.record.source_profile_binding_sha256
                    != case.source.source_profile_binding_sha256()
                || prediction.record.deck_manifest_commitment_sha256
                    != deck.manifest_commitment_sha256()
                || prediction.record.policy_deployment_commitment_sha256
                    != spec.policy_deployment_commitment_sha256
                || prediction.sideboard.frame_id() != expected.frame_id()
                || prediction.sideboard.frame_sequence() != expected.frame_sequence()
                || prediction.sideboard.frame_sha256() != expected.frame_sha256()
            {
                return Err(error_v1(
                    "sideboard_evaluation_prediction_source",
                    "prediction must refer to the exact annotated source, deck, policy, and frame",
                ));
            }
            prediction_count += 1;
            *slice_prediction_counts.entry(slice).or_insert(0_u32) += 1;
            let exact = prediction.sideboard.snapshot_commitment_sha256()
                == expected.snapshot_commitment_sha256();
            exact_prediction_count += u32::from(exact);
            *slice_exact_counts.entry(slice).or_insert(0_u32) += u32::from(exact);
            (prediction.prediction_commitment_sha256_v1(), exact)
        } else {
            ("abstained", false)
        };

        for part in [
            case.case_id.as_bytes(),
            case.source.manifest_sha256().as_bytes(),
            case.source.canonical_bgra8_sha256().as_bytes(),
            expected.lifecycle_snapshot_commitment_sha256().as_bytes(),
            expected.snapshot_commitment_sha256().as_bytes(),
            prediction_commitment.as_bytes(),
            &[u8::from(exact)],
        ] {
            hash_part_v1(&mut hasher, part);
        }
    }

    let unique_case_count = u32::try_from(cases.len()).map_err(|_| {
        error_v1(
            "sideboard_evaluation_case_count",
            "case count does not fit u32",
        )
    })?;
    let prediction_coverage_bps = ratio_bps_v1(prediction_count, unique_case_count);
    let exact_accuracy_bps = ratio_bps_v1(exact_prediction_count, prediction_count);
    let missing_slices = REQUIRED_SIDEBOARD_SLICES_V1
        .iter()
        .copied()
        .filter(|slice| !slice_counts.contains_key(slice))
        .collect::<Vec<_>>();
    let minimum_observed_cases_per_slice = REQUIRED_SIDEBOARD_SLICES_V1
        .iter()
        .map(|slice| slice_counts.get(slice).copied().unwrap_or(0))
        .min()
        .unwrap_or(0);
    let minimum_prediction_coverage_bps_per_slice = REQUIRED_SIDEBOARD_SLICES_V1
        .iter()
        .map(|slice| {
            ratio_bps_v1(
                slice_prediction_counts.get(slice).copied().unwrap_or(0),
                slice_counts.get(slice).copied().unwrap_or(0),
            )
        })
        .min()
        .unwrap_or(0);
    let minimum_exact_accuracy_bps_per_slice = REQUIRED_SIDEBOARD_SLICES_V1
        .iter()
        .map(|slice| {
            ratio_bps_v1(
                slice_exact_counts.get(slice).copied().unwrap_or(0),
                slice_prediction_counts.get(slice).copied().unwrap_or(0),
            )
        })
        .min()
        .unwrap_or(0);
    let passes_declared_gate = missing_slices.is_empty()
        && minimum_observed_cases_per_slice >= spec.minimum_unique_cases_per_slice
        && prediction_coverage_bps >= spec.minimum_prediction_coverage_bps
        && exact_accuracy_bps >= spec.minimum_exact_accuracy_bps
        && minimum_prediction_coverage_bps_per_slice >= spec.minimum_prediction_coverage_bps
        && minimum_exact_accuracy_bps_per_slice >= spec.minimum_exact_accuracy_bps;

    Ok(CheckedUntrustedMtgoCompetitiveSideboardEvaluationV1 {
        profile_commitment_sha256: spec.profile_commitment_sha256,
        approved_account_alias_sha256: corpus.manifest.approved_account_alias_sha256.clone(),
        deck_list_sha256: spec.deck_list_sha256,
        deck_manifest_commitment_sha256: spec.deck_manifest_commitment_sha256,
        deck_format_sha256: spec.deck_format_sha256,
        policy_deployment_commitment_sha256: spec.policy_deployment_commitment_sha256,
        corpus_manifest_sha256: spec.corpus_manifest_sha256,
        evaluation_commitment_sha256: format!("{:x}", hasher.finalize()),
        unique_case_count,
        prediction_count,
        exact_prediction_count,
        prediction_coverage_bps,
        exact_accuracy_bps,
        minimum_observed_cases_per_slice,
        minimum_prediction_coverage_bps_per_slice,
        minimum_exact_accuracy_bps_per_slice,
        missing_slices,
        passes_declared_gate,
    })
}

pub fn review_competitive_sideboard_evaluation_ratification_candidate_v1(
    evaluation: &CheckedUntrustedMtgoCompetitiveSideboardEvaluationV1,
) -> Result<MtgoReviewedCompetitiveSideboardEvaluationRatificationCandidateV1, MtgoContractErrorV1>
{
    if !evaluation.passes_declared_gate {
        return Err(error_v1(
            "sideboard_evaluation_declared_gate",
            evaluation.evaluation_commitment_sha256.clone(),
        ));
    }
    let ratification_commitment_sha256 = commitment_v1(
        SIDEBOARD_EVALUATION_RATIFICATION_DOMAIN_V1,
        &[
            evaluation.profile_commitment_sha256.as_bytes(),
            evaluation.approved_account_alias_sha256.as_bytes(),
            evaluation.deck_list_sha256.as_bytes(),
            evaluation.deck_manifest_commitment_sha256.as_bytes(),
            evaluation.deck_format_sha256.as_bytes(),
            evaluation.policy_deployment_commitment_sha256.as_bytes(),
            evaluation.corpus_manifest_sha256.as_bytes(),
            evaluation.evaluation_commitment_sha256.as_bytes(),
            b"league_challenge_changed_unchanged_sideboard_exact_visible_snapshot_v1",
            b"no_live_classification_no_event_entry_no_spending_no_coordinates_no_input",
        ],
    );
    Ok(
        MtgoReviewedCompetitiveSideboardEvaluationRatificationCandidateV1 {
            profile_commitment_sha256: evaluation.profile_commitment_sha256.clone(),
            approved_account_alias_sha256: evaluation.approved_account_alias_sha256.clone(),
            deck_list_sha256: evaluation.deck_list_sha256.clone(),
            deck_manifest_commitment_sha256: evaluation.deck_manifest_commitment_sha256.clone(),
            deck_format_sha256: evaluation.deck_format_sha256.clone(),
            policy_deployment_commitment_sha256: evaluation
                .policy_deployment_commitment_sha256
                .clone(),
            corpus_manifest_sha256: evaluation.corpus_manifest_sha256.clone(),
            evaluation_commitment_sha256: evaluation.evaluation_commitment_sha256.clone(),
            ratification_commitment_sha256,
        },
    )
}

pub fn admit_ratified_competitive_sideboard_evaluation_v1(
    evaluation: CheckedUntrustedMtgoCompetitiveSideboardEvaluationV1,
    candidate: MtgoReviewedCompetitiveSideboardEvaluationRatificationCandidateV1,
) -> Result<AdmittedMtgoCompetitiveSideboardEvaluationV1, MtgoContractErrorV1> {
    admit_competitive_sideboard_evaluation_against_ratification_v1(
        evaluation,
        candidate,
        RATIFIED_COMPETITIVE_SIDEBOARD_EVALUATION_COMMITMENT_V1,
    )
}

fn admit_competitive_sideboard_evaluation_against_ratification_v1(
    evaluation: CheckedUntrustedMtgoCompetitiveSideboardEvaluationV1,
    candidate: MtgoReviewedCompetitiveSideboardEvaluationRatificationCandidateV1,
    ratified_commitment: Option<&str>,
) -> Result<AdmittedMtgoCompetitiveSideboardEvaluationV1, MtgoContractErrorV1> {
    let expected = review_competitive_sideboard_evaluation_ratification_candidate_v1(&evaluation)?;
    if candidate != expected {
        return Err(error_v1(
            "sideboard_evaluation_ratification_candidate",
            "candidate does not exactly match the checked evaluation",
        ));
    }
    let ratified = ratified_commitment.ok_or_else(|| {
        error_v1(
            "sideboard_evaluation_not_ratified",
            "production contains no ratified League and Challenge sideboard evaluation",
        )
    })?;
    validate_sha256_v1(ratified, "sideboard_evaluation_ratification")?;
    if candidate.ratification_commitment_sha256 != ratified {
        return Err(error_v1(
            "sideboard_evaluation_not_ratified",
            "candidate does not match the production ratification",
        ));
    }
    let admission_commitment_sha256 = commitment_v1(
        SIDEBOARD_EVALUATION_ADMISSION_DOMAIN_V1,
        &[
            candidate.ratification_commitment_sha256.as_bytes(),
            candidate.evaluation_commitment_sha256.as_bytes(),
            candidate.profile_commitment_sha256.as_bytes(),
            candidate.approved_account_alias_sha256.as_bytes(),
            candidate.deck_manifest_commitment_sha256.as_bytes(),
            candidate.policy_deployment_commitment_sha256.as_bytes(),
            b"reviewed_sideboard_evaluation_only_no_runtime_pixels_no_input",
        ],
    );
    Ok(AdmittedMtgoCompetitiveSideboardEvaluationV1 {
        commitments: candidate,
        admission_commitment_sha256,
    })
}

fn validate_evaluation_spec_v1(
    spec: &MtgoCompetitiveSideboardEvaluationSpecV1,
) -> Result<(), MtgoContractErrorV1> {
    if spec.schema_version != MTGO_COMPETITIVE_SIDEBOARD_EVALUATION_SCHEMA_V1 {
        return Err(error_v1(
            "sideboard_evaluation_schema",
            "expected schema version 1",
        ));
    }
    validate_identifier_v1(&spec.evaluation_id, "sideboard_evaluation_id")?;
    for (digest, code) in [
        (
            &spec.profile_commitment_sha256,
            "sideboard_evaluation_profile",
        ),
        (&spec.corpus_manifest_sha256, "sideboard_evaluation_corpus"),
        (
            &spec.annotation_protocol_sha256,
            "sideboard_evaluation_annotation",
        ),
        (&spec.evaluator_binary_sha256, "sideboard_evaluation_binary"),
        (&spec.deck_list_sha256, "sideboard_evaluation_deck"),
        (
            &spec.deck_manifest_commitment_sha256,
            "sideboard_evaluation_manifest",
        ),
        (&spec.deck_format_sha256, "sideboard_evaluation_format"),
        (
            &spec.policy_deployment_commitment_sha256,
            "sideboard_evaluation_policy",
        ),
    ] {
        validate_sha256_v1(digest, code)?;
    }
    if spec.minimum_unique_cases_per_slice == 0 || spec.minimum_unique_cases_per_slice > 25_000 {
        return Err(error_v1(
            "sideboard_evaluation_minimum_cases",
            spec.minimum_unique_cases_per_slice.to_string(),
        ));
    }
    if !(9_500..=10_000).contains(&spec.minimum_prediction_coverage_bps) {
        return Err(error_v1(
            "sideboard_evaluation_coverage_threshold",
            spec.minimum_prediction_coverage_bps.to_string(),
        ));
    }
    if !(9_500..=10_000).contains(&spec.minimum_exact_accuracy_bps) {
        return Err(error_v1(
            "sideboard_evaluation_accuracy_threshold",
            spec.minimum_exact_accuracy_bps.to_string(),
        ));
    }
    if spec.required_slices != REQUIRED_SIDEBOARD_SLICES_V1 {
        return Err(error_v1(
            "sideboard_evaluation_required_slices",
            "all four League and Challenge changed and unchanged slices are required in canonical order",
        ));
    }
    Ok(())
}

fn validate_identity_set_v1(
    profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    expected_policy_deployment_commitment_sha256: &str,
    identity: SideboardIdentitySetV1<'_>,
) -> Result<(), MtgoContractErrorV1> {
    validate_sha256_v1(
        expected_policy_deployment_commitment_sha256,
        "sideboard_policy_deployment",
    )?;
    if identity.profile_commitment_sha256 != profile.profile_commitment_sha256()
        || identity.approved_account_alias_sha256 != profile.approved_account_alias_sha256()
        || identity.deck_list_sha256 != deck.deck_list_sha256()
        || identity.deck_manifest_commitment_sha256 != deck.manifest_commitment_sha256()
        || identity.deck_format_sha256 != deck.format_sha256()
        || identity.policy_deployment_commitment_sha256
            != expected_policy_deployment_commitment_sha256
    {
        return Err(error_v1(
            "sideboard_evaluation_identity",
            "profile, approved account, deck, format, or policy commitment differs",
        ));
    }
    Ok(())
}

fn sideboard_slice_v1(
    event_kind: MtgoCompetitiveEventKindV1,
    configuration: &crate::MtgoCompetitiveDeckConfigurationV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
) -> MtgoCompetitiveSideboardEvaluationSliceV1 {
    match (event_kind, configuration == deck.configuration_v1()) {
        (MtgoCompetitiveEventKindV1::League, true) => {
            MtgoCompetitiveSideboardEvaluationSliceV1::LeagueNoChanges
        }
        (MtgoCompetitiveEventKindV1::League, false) => {
            MtgoCompetitiveSideboardEvaluationSliceV1::LeagueChangedConfiguration
        }
        (MtgoCompetitiveEventKindV1::Challenge, true) => {
            MtgoCompetitiveSideboardEvaluationSliceV1::ChallengeNoChanges
        }
        (MtgoCompetitiveEventKindV1::Challenge, false) => {
            MtgoCompetitiveSideboardEvaluationSliceV1::ChallengeChangedConfiguration
        }
    }
}

fn ratio_bps_v1(numerator: u32, denominator: u32) -> u16 {
    if denominator == 0 {
        return 0;
    }
    let ratio = u64::from(numerator) * 10_000 / u64::from(denominator);
    u16::try_from(ratio).unwrap_or(10_000)
}

fn validate_identifier_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
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

fn validate_sha256_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1(
            code,
            "value must be 64 lowercase hexadecimal characters",
        ));
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

fn hash_part_v1(hasher: &mut Sha256, part: &[u8]) {
    hasher.update((part.len() as u64).to_be_bytes());
    hasher.update(part);
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        canonical_competitive_lifecycle_slices_v1,
        check_untrusted_competitive_navigation_runtime_profile_v1,
        checked_untrusted_competitive_navigation_source_for_test_v1,
        validate_competitive_deck_manifest_v1, MtgoCompetitiveDeckCardCountV1,
        MtgoCompetitiveDeckConfigurationV1, MtgoCompetitiveDeckManifestV1,
        MtgoCompetitiveDeckPartitionV1, MtgoCompetitiveLifecyclePhaseV1,
        MtgoCompetitiveNavigationRuntimeProfileV1, MtgoLifecycleVisibleFactKindV1,
        MtgoLifecycleVisibleFactV1, MtgoRectPxV1, MtgoSizePxV1,
        MtgoVisibleCompetitiveSideboardCardV1, MtgoVisibleCompetitiveSideboardZoneV1,
        MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1, MTGO_COMPETITIVE_NAVIGATION_EVALUATION_SCHEMA_V1,
        MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
    };
    use mtg_kernel::card_def::card_id_by_name;

    fn digest(byte: char) -> String {
        byte.to_string().repeat(64)
    }

    fn card(name: &str, count: u16) -> MtgoCompetitiveDeckCardCountV1 {
        MtgoCompetitiveDeckCardCountV1 {
            card_db_id: card_id_by_name(name).unwrap(),
            card_name: name.to_owned(),
            count,
        }
    }

    fn sorted(
        mut cards: Vec<MtgoCompetitiveDeckCardCountV1>,
    ) -> Vec<MtgoCompetitiveDeckCardCountV1> {
        cards.sort_by_key(|card| card.card_db_id);
        cards
    }

    fn starting_configuration() -> MtgoCompetitiveDeckConfigurationV1 {
        MtgoCompetitiveDeckConfigurationV1 {
            mainboard: sorted(vec![card("Mountain", 3), card("Lightning Bolt", 2)]),
            sideboard: sorted(vec![card("Searing Blaze", 2)]),
        }
    }

    fn changed_configuration() -> MtgoCompetitiveDeckConfigurationV1 {
        MtgoCompetitiveDeckConfigurationV1 {
            mainboard: sorted(vec![
                card("Mountain", 3),
                card("Lightning Bolt", 1),
                card("Searing Blaze", 1),
            ]),
            sideboard: sorted(vec![card("Lightning Bolt", 1), card("Searing Blaze", 1)]),
        }
    }

    fn profile() -> CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1 {
        check_untrusted_competitive_navigation_runtime_profile_v1(
            MtgoCompetitiveNavigationRuntimeProfileV1 {
                schema_version: MTGO_COMPETITIVE_NAVIGATION_EVALUATION_SCHEMA_V1,
                profile_id: "sideboard-evaluation-test-v1".to_owned(),
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

    fn deck() -> ValidatedMtgoCompetitiveDeckManifestV1 {
        validate_competitive_deck_manifest_v1(MtgoCompetitiveDeckManifestV1 {
            schema_version: MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
            deck_list_sha256: digest('0'),
            format_sha256: digest('1'),
            starting_mainboard_count: 5,
            starting_sideboard_count: 2,
            configuration: starting_configuration(),
        })
        .unwrap()
    }

    fn lifecycle(
        source: &CheckedUntrustedMtgoCompetitiveNavigationSourceV1,
        event_kind: MtgoCompetitiveEventKindV1,
        frame_id: u64,
    ) -> MtgoVisibleCompetitiveLifecycleSnapshotV1 {
        let facts = [
            MtgoLifecycleVisibleFactKindV1::SideboardSurfaceVisible,
            MtgoLifecycleVisibleFactKindV1::SideboardTimerVisible,
            MtgoLifecycleVisibleFactKindV1::SideboardConfigurationVisible,
            MtgoLifecycleVisibleFactKindV1::SideboardSubmitControlEnabled,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, kind)| MtgoLifecycleVisibleFactV1 {
            kind,
            rect_client_px: MtgoRectPxV1 {
                x: 20 + u32::try_from(index).unwrap() * 30,
                y: 20,
                width: 24,
                height: 24,
            },
            content_sha256: format!("{:064x}", index + 20),
            confidence_bps: 10_000,
        })
        .collect();
        MtgoVisibleCompetitiveLifecycleSnapshotV1 {
            schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
            snapshot_id: format!("sideboard-evaluation-lifecycle-{frame_id}"),
            event_kind,
            phase: MtgoCompetitiveLifecyclePhaseV1::Sideboarding,
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
            match_identity_sha256: Some(digest('e')),
            game_number: Some(1),
            entry_terms: None,
            visible_state_complete: true,
            facts,
        }
    }

    fn visible_cards(
        configuration: &MtgoCompetitiveDeckConfigurationV1,
    ) -> Vec<MtgoVisibleCompetitiveSideboardCardV1> {
        configuration
            .mainboard
            .iter()
            .map(|card| (MtgoCompetitiveDeckPartitionV1::Mainboard, card))
            .chain(
                configuration
                    .sideboard
                    .iter()
                    .map(|card| (MtgoCompetitiveDeckPartitionV1::Sideboard, card)),
            )
            .enumerate()
            .map(
                |(index, (partition, card))| MtgoVisibleCompetitiveSideboardCardV1 {
                    partition,
                    card_db_id: card.card_db_id,
                    card_name: card.card_name.clone(),
                    count: card.count,
                    rect_client_px: MtgoRectPxV1 {
                        x: 20 + u32::try_from(index).unwrap() * 100,
                        y: match partition {
                            MtgoCompetitiveDeckPartitionV1::Mainboard => 150,
                            MtgoCompetitiveDeckPartitionV1::Sideboard => 550,
                        },
                        width: 80,
                        height: 100,
                    },
                    content_sha256: format!("{:064x}", index + 30),
                    confidence_bps: 10_000,
                },
            )
            .collect()
    }

    fn sideboard(
        lifecycle: &MtgoVisibleCompetitiveLifecycleSnapshotV1,
        deck: &ValidatedMtgoCompetitiveDeckManifestV1,
        configuration: &MtgoCompetitiveDeckConfigurationV1,
    ) -> MtgoVisibleCompetitiveSideboardSnapshotV1 {
        let checked_lifecycle =
            validate_visible_competitive_lifecycle_snapshot_v1(lifecycle.clone()).unwrap();
        MtgoVisibleCompetitiveSideboardSnapshotV1 {
            schema_version: MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
            snapshot_id: format!("sideboard-evaluation-{}", lifecycle.frame_id),
            event_kind: lifecycle.event_kind,
            event_identity_sha256: lifecycle.event_identity_sha256.clone().unwrap(),
            match_identity_sha256: lifecycle.match_identity_sha256.clone().unwrap(),
            game_number: 1,
            frame_id: lifecycle.frame_id,
            frame_sequence: lifecycle.frame_sequence,
            frame_sha256: lifecycle.frame_sha256.clone(),
            lifecycle_snapshot_commitment_sha256: checked_lifecycle
                .snapshot_commitment_sha256()
                .to_owned(),
            deck_manifest_commitment_sha256: deck.manifest_commitment_sha256().to_owned(),
            policy_deployment_commitment_sha256: digest('7'),
            visible_configuration_complete: true,
            mainboard_zone: MtgoVisibleCompetitiveSideboardZoneV1 {
                rect_client_px: MtgoRectPxV1 {
                    x: 0,
                    y: 120,
                    width: 1_200,
                    height: 300,
                },
                content_sha256: digest('8'),
                empty_drop_rect_client_px: MtgoRectPxV1 {
                    x: 1_000,
                    y: 160,
                    width: 100,
                    height: 100,
                },
                empty_drop_content_sha256: digest('9'),
                confidence_bps: 10_000,
            },
            sideboard_zone: MtgoVisibleCompetitiveSideboardZoneV1 {
                rect_client_px: MtgoRectPxV1 {
                    x: 0,
                    y: 500,
                    width: 1_200,
                    height: 300,
                },
                content_sha256: digest('a'),
                empty_drop_rect_client_px: MtgoRectPxV1 {
                    x: 1_000,
                    y: 540,
                    width: 100,
                    height: 100,
                },
                empty_drop_content_sha256: digest('b'),
                confidence_bps: 10_000,
            },
            cards: visible_cards(configuration),
        }
    }

    struct Fixture {
        profile: CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
        deck: ValidatedMtgoCompetitiveDeckManifestV1,
        sources: Vec<CheckedUntrustedMtgoCompetitiveNavigationSourceV1>,
        lifecycles: Vec<MtgoVisibleCompetitiveLifecycleSnapshotV1>,
        sideboards: Vec<MtgoVisibleCompetitiveSideboardSnapshotV1>,
        predictions: Vec<CheckedUntrustedMtgoCompetitiveSideboardPredictionV1>,
        corpus: CheckedUntrustedMtgoCompetitiveSideboardCorpusManifestV1,
        spec: MtgoCompetitiveSideboardEvaluationSpecV1,
    }

    fn fixture() -> Fixture {
        let profile = profile();
        let deck = deck();
        let sources = (1_u8..=4)
            .map(|value| {
                checked_untrusted_competitive_navigation_source_for_test_v1(&profile, value)
            })
            .collect::<Vec<_>>();
        let kinds = [
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
            MtgoCompetitiveEventKindV1::Challenge,
        ];
        let configurations = [
            starting_configuration(),
            changed_configuration(),
            starting_configuration(),
            changed_configuration(),
        ];
        let lifecycles = sources
            .iter()
            .enumerate()
            .map(|(index, source)| lifecycle(source, kinds[index], index as u64 + 1))
            .collect::<Vec<_>>();
        let sideboards = lifecycles
            .iter()
            .enumerate()
            .map(|(index, lifecycle)| sideboard(lifecycle, &deck, &configurations[index]))
            .collect::<Vec<_>>();
        let mut corpus_cases = Vec::new();
        for index in 0..4 {
            let checked_lifecycle =
                validate_visible_competitive_lifecycle_snapshot_v1(lifecycles[index].clone())
                    .unwrap();
            let checked_sideboard = validate_visible_competitive_sideboard_snapshot_v1(
                checked_lifecycle,
                &deck,
                sideboards[index].clone(),
            )
            .unwrap();
            corpus_cases.push(MtgoCompetitiveSideboardCorpusCaseV1 {
                case_id: format!("case-{:02}", index + 1),
                slice: REQUIRED_SIDEBOARD_SLICES_V1[index],
                source_manifest_sha256: sources[index].manifest_sha256().to_owned(),
                source_canonical_bgra8_sha256: sources[index].canonical_bgra8_sha256().to_owned(),
                source_profile_binding_sha256: sources[index]
                    .source_profile_binding_sha256()
                    .to_owned(),
                expected_lifecycle_snapshot_commitment_sha256: checked_sideboard
                    .lifecycle_snapshot_commitment_sha256()
                    .to_owned(),
                expected_sideboard_snapshot_commitment_sha256: checked_sideboard
                    .snapshot_commitment_sha256()
                    .to_owned(),
                annotator_alias_sha256: digest('c'),
                annotation_receipt_sha256: format!("{:064x}", index + 50),
                annotated_at_unix_millis: 1_786_400_000_000 + index as u64,
                unobscured_frame_visually_confirmed: true,
                event_and_game_identity_visually_confirmed: true,
                all_visible_card_rows_annotated: true,
                zone_and_empty_drop_regions_annotated: true,
                submit_control_visually_confirmed: true,
            });
        }
        let raw_corpus = MtgoCompetitiveSideboardCorpusManifestV1 {
            schema_version: MTGO_COMPETITIVE_SIDEBOARD_EVALUATION_SCHEMA_V1,
            corpus_id: "sideboard-corpus-test-v1".to_owned(),
            profile_commitment_sha256: profile.profile_commitment_sha256().to_owned(),
            approved_account_alias_sha256: profile.approved_account_alias_sha256().to_owned(),
            annotation_protocol_sha256: digest('d'),
            deck_list_sha256: deck.deck_list_sha256().to_owned(),
            deck_manifest_commitment_sha256: deck.manifest_commitment_sha256().to_owned(),
            deck_format_sha256: deck.format_sha256().to_owned(),
            policy_deployment_commitment_sha256: digest('7'),
            cases: corpus_cases,
        };
        let corpus = check_untrusted_competitive_sideboard_corpus_manifest_v1(
            &profile,
            &deck,
            &digest('7'),
            raw_corpus,
        )
        .unwrap();
        let predictions = (0..4)
            .map(|index| {
                check_untrusted_competitive_sideboard_prediction_v1(
                    &profile,
                    &sources[index],
                    &deck,
                    &digest('7'),
                    MtgoCompetitiveSideboardPredictionV1 {
                        schema_version: MTGO_COMPETITIVE_SIDEBOARD_EVALUATION_SCHEMA_V1,
                        profile_commitment_sha256: profile.profile_commitment_sha256().to_owned(),
                        source_manifest_sha256: sources[index].manifest_sha256().to_owned(),
                        source_canonical_bgra8_sha256: sources[index]
                            .canonical_bgra8_sha256()
                            .to_owned(),
                        source_profile_binding_sha256: sources[index]
                            .source_profile_binding_sha256()
                            .to_owned(),
                        classifier_request_sha256: format!("{:064x}", index + 100),
                        classifier_response_sha256: format!("{:064x}", index + 110),
                        deck_list_sha256: deck.deck_list_sha256().to_owned(),
                        deck_manifest_commitment_sha256: deck
                            .manifest_commitment_sha256()
                            .to_owned(),
                        deck_format_sha256: deck.format_sha256().to_owned(),
                        policy_deployment_commitment_sha256: digest('7'),
                        lifecycle: lifecycles[index].clone(),
                        sideboard: sideboards[index].clone(),
                    },
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let spec = MtgoCompetitiveSideboardEvaluationSpecV1 {
            schema_version: MTGO_COMPETITIVE_SIDEBOARD_EVALUATION_SCHEMA_V1,
            evaluation_id: "sideboard-evaluation-test-v1".to_owned(),
            profile_commitment_sha256: profile.profile_commitment_sha256().to_owned(),
            corpus_manifest_sha256: corpus.manifest_sha256_v1().to_owned(),
            annotation_protocol_sha256: digest('d'),
            evaluator_binary_sha256: digest('e'),
            deck_list_sha256: deck.deck_list_sha256().to_owned(),
            deck_manifest_commitment_sha256: deck.manifest_commitment_sha256().to_owned(),
            deck_format_sha256: deck.format_sha256().to_owned(),
            policy_deployment_commitment_sha256: digest('7'),
            minimum_unique_cases_per_slice: 1,
            minimum_prediction_coverage_bps: 10_000,
            minimum_exact_accuracy_bps: 10_000,
            required_slices: REQUIRED_SIDEBOARD_SLICES_V1.to_vec(),
        };
        Fixture {
            profile,
            deck,
            sources,
            lifecycles,
            sideboards,
            predictions,
            corpus,
            spec,
        }
    }

    fn cases<'a>(fixture: &'a Fixture) -> Vec<MtgoCompetitiveSideboardEvaluationCaseV1<'a>> {
        (0..4)
            .map(|index| MtgoCompetitiveSideboardEvaluationCaseV1 {
                case_id: format!("case-{:02}", index + 1),
                source: &fixture.sources[index],
                expected_lifecycle: fixture.lifecycles[index].clone(),
                expected_sideboard: fixture.sideboards[index].clone(),
                prediction: Some(&fixture.predictions[index]),
            })
            .collect()
    }

    #[test]
    fn four_slice_exact_evaluation_passes_but_production_admission_is_empty() {
        let fixture = fixture();
        let evaluation = evaluate_untrusted_competitive_sideboard_profile_v1(
            &fixture.profile,
            &fixture.deck,
            &fixture.corpus,
            fixture.spec.clone(),
            cases(&fixture),
        )
        .unwrap();
        assert_eq!(evaluation.unique_case_count_v1(), 4);
        assert_eq!(evaluation.prediction_count_v1(), 4);
        assert_eq!(evaluation.exact_prediction_count_v1(), 4);
        assert_eq!(evaluation.prediction_coverage_bps_v1(), 10_000);
        assert_eq!(evaluation.exact_accuracy_bps_v1(), 10_000);
        assert_eq!(evaluation.minimum_observed_cases_per_slice_v1(), 1);
        assert_eq!(
            evaluation.minimum_prediction_coverage_bps_per_slice_v1(),
            10_000
        );
        assert_eq!(evaluation.minimum_exact_accuracy_bps_per_slice_v1(), 10_000);
        assert!(evaluation.missing_slices_v1().is_empty());
        assert!(evaluation.passes_declared_gate_v1());
        assert!(!evaluation.safe_for_live_classification_v1());
        assert!(!evaluation.safe_for_input_v1());
        let candidate =
            review_competitive_sideboard_evaluation_ratification_candidate_v1(&evaluation).unwrap();
        let error = admit_ratified_competitive_sideboard_evaluation_v1(evaluation, candidate)
            .err()
            .unwrap();
        assert_eq!(error.code(), "sideboard_evaluation_not_ratified");
    }

    #[test]
    fn internal_exact_ratification_is_opaque_and_non_authorizing() {
        let fixture = fixture();
        let evaluation = evaluate_untrusted_competitive_sideboard_profile_v1(
            &fixture.profile,
            &fixture.deck,
            &fixture.corpus,
            fixture.spec.clone(),
            cases(&fixture),
        )
        .unwrap();
        let candidate =
            review_competitive_sideboard_evaluation_ratification_candidate_v1(&evaluation).unwrap();
        let ratified = candidate.ratification_commitment_sha256.clone();
        let admitted = admit_competitive_sideboard_evaluation_against_ratification_v1(
            evaluation,
            candidate,
            Some(&ratified),
        )
        .unwrap();
        assert_eq!(
            admitted.commitments_v1().ratification_commitment_sha256,
            ratified
        );
        assert_eq!(admitted.admission_commitment_sha256_v1().len(), 64);
        assert!(!admitted.safe_for_live_classification_v1());
        assert!(!admitted.safe_for_input_v1());
    }

    #[test]
    fn abstention_fails_overall_and_per_slice_coverage() {
        let fixture = fixture();
        let mut evaluation_cases = cases(&fixture);
        evaluation_cases[3].prediction = None;
        let evaluation = evaluate_untrusted_competitive_sideboard_profile_v1(
            &fixture.profile,
            &fixture.deck,
            &fixture.corpus,
            fixture.spec.clone(),
            evaluation_cases,
        )
        .unwrap();
        assert_eq!(evaluation.prediction_coverage_bps_v1(), 7_500);
        assert_eq!(evaluation.minimum_prediction_coverage_bps_per_slice_v1(), 0);
        assert!(!evaluation.passes_declared_gate_v1());
        assert_eq!(
            review_competitive_sideboard_evaluation_ratification_candidate_v1(&evaluation)
                .err()
                .unwrap()
                .code(),
            "sideboard_evaluation_declared_gate"
        );
    }

    #[test]
    fn structurally_valid_wrong_configuration_fails_exact_accuracy() {
        let mut fixture = fixture();
        let wrong_sideboard = sideboard(
            &fixture.lifecycles[0],
            &fixture.deck,
            &changed_configuration(),
        );
        fixture.predictions[0] = check_untrusted_competitive_sideboard_prediction_v1(
            &fixture.profile,
            &fixture.sources[0],
            &fixture.deck,
            &digest('7'),
            MtgoCompetitiveSideboardPredictionV1 {
                schema_version: MTGO_COMPETITIVE_SIDEBOARD_EVALUATION_SCHEMA_V1,
                profile_commitment_sha256: fixture.profile.profile_commitment_sha256().to_owned(),
                source_manifest_sha256: fixture.sources[0].manifest_sha256().to_owned(),
                source_canonical_bgra8_sha256: fixture.sources[0]
                    .canonical_bgra8_sha256()
                    .to_owned(),
                source_profile_binding_sha256: fixture.sources[0]
                    .source_profile_binding_sha256()
                    .to_owned(),
                classifier_request_sha256: digest('a'),
                classifier_response_sha256: digest('b'),
                deck_list_sha256: fixture.deck.deck_list_sha256().to_owned(),
                deck_manifest_commitment_sha256: fixture
                    .deck
                    .manifest_commitment_sha256()
                    .to_owned(),
                deck_format_sha256: fixture.deck.format_sha256().to_owned(),
                policy_deployment_commitment_sha256: digest('7'),
                lifecycle: fixture.lifecycles[0].clone(),
                sideboard: wrong_sideboard,
            },
        )
        .unwrap();
        let evaluation = evaluate_untrusted_competitive_sideboard_profile_v1(
            &fixture.profile,
            &fixture.deck,
            &fixture.corpus,
            fixture.spec.clone(),
            cases(&fixture),
        )
        .unwrap();
        assert_eq!(evaluation.exact_prediction_count_v1(), 3);
        assert_eq!(evaluation.exact_accuracy_bps_v1(), 7_500);
        assert_eq!(evaluation.minimum_exact_accuracy_bps_per_slice_v1(), 0);
        assert!(!evaluation.passes_declared_gate_v1());
    }

    #[test]
    fn source_and_review_substitutions_fail_closed() {
        let fixture = fixture();
        let mut evaluation_cases = cases(&fixture);
        evaluation_cases[0].prediction = Some(&fixture.predictions[1]);
        assert_eq!(
            evaluate_untrusted_competitive_sideboard_profile_v1(
                &fixture.profile,
                &fixture.deck,
                &fixture.corpus,
                fixture.spec.clone(),
                evaluation_cases,
            )
            .err()
            .unwrap()
            .code(),
            "sideboard_evaluation_prediction_source"
        );

        let mut wrong_cases = cases(&fixture);
        wrong_cases[0].case_id = "case-00".to_owned();
        assert_eq!(
            evaluate_untrusted_competitive_sideboard_profile_v1(
                &fixture.profile,
                &fixture.deck,
                &fixture.corpus,
                fixture.spec.clone(),
                wrong_cases,
            )
            .err()
            .unwrap()
            .code(),
            "sideboard_evaluation_corpus_case_binding"
        );
    }

    #[test]
    fn spec_rejects_missing_slice_and_weak_thresholds() {
        let fixture = fixture();
        let mut missing = fixture.spec.clone();
        missing.required_slices.pop();
        assert_eq!(
            evaluate_untrusted_competitive_sideboard_profile_v1(
                &fixture.profile,
                &fixture.deck,
                &fixture.corpus,
                missing,
                cases(&fixture),
            )
            .err()
            .unwrap()
            .code(),
            "sideboard_evaluation_required_slices"
        );

        let mut weak = fixture.spec.clone();
        weak.minimum_exact_accuracy_bps = 9_499;
        assert_eq!(
            evaluate_untrusted_competitive_sideboard_profile_v1(
                &fixture.profile,
                &fixture.deck,
                &fixture.corpus,
                weak,
                cases(&fixture),
            )
            .err()
            .unwrap()
            .code(),
            "sideboard_evaluation_accuracy_threshold"
        );
    }

    #[test]
    fn corpus_requires_ordered_distinct_sources_and_complete_review() {
        let fixture = fixture();
        let mut raw = fixture.corpus.manifest.clone();
        raw.cases[0].unobscured_frame_visually_confirmed = false;
        assert_eq!(
            check_untrusted_competitive_sideboard_corpus_manifest_v1(
                &fixture.profile,
                &fixture.deck,
                &digest('7'),
                raw,
            )
            .err()
            .unwrap()
            .code(),
            "sideboard_corpus_visual_review"
        );

        let mut raw = fixture.corpus.manifest.clone();
        raw.cases.swap(0, 1);
        assert_eq!(
            check_untrusted_competitive_sideboard_corpus_manifest_v1(
                &fixture.profile,
                &fixture.deck,
                &digest('7'),
                raw,
            )
            .err()
            .unwrap()
            .code(),
            "sideboard_corpus_case_order"
        );
    }

    #[test]
    fn serialized_contracts_reject_unknown_fields() {
        let fixture = fixture();
        let mut value = serde_json::to_value(&fixture.spec).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("process_memory".to_owned(), serde_json::json!(true));
        assert!(serde_json::from_value::<MtgoCompetitiveSideboardEvaluationSpecV1>(value).is_err());
    }
}
