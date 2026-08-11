use crate::{
    validate_snapshot_artifact_source_v1, validate_source_profile_v1,
    validate_visible_competitive_event_listing_selection_v1,
    validate_visible_competitive_lifecycle_snapshot_v1,
    CheckedUntrustedMtgoCompetitiveEventListingSelectionV1,
    CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    CheckedUntrustedMtgoCompetitiveNavigationSourceV1, MtgoCompetitiveEventKindV1,
    MtgoCompetitiveEventListingTargetV1, MtgoContractErrorV1,
    MtgoVisibleCompetitiveEventListingSelectionV1, MtgoVisibleCompetitiveLifecycleSnapshotV1,
    ValidatedMtgoCompetitiveDeckManifestV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

pub const MTGO_COMPETITIVE_EVENT_LISTING_EVALUATION_SCHEMA_V1: u32 = 1;

const EVENT_LISTING_CORPUS_DOMAIN_V1: &[u8] = b"mtgo-competitive-event-listing-corpus-v1";
const EVENT_LISTING_PREDICTION_DOMAIN_V1: &[u8] = b"mtgo-competitive-event-listing-prediction-v1";
const EVENT_LISTING_EVALUATION_DOMAIN_V1: &[u8] = b"mtgo-competitive-event-listing-evaluation-v1";
const EVENT_LISTING_EVALUATION_RATIFICATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-listing-evaluation-ratification-v1";
const EVENT_LISTING_EVALUATION_ADMISSION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-listing-evaluation-admission-v1";
pub(crate) const RATIFIED_COMPETITIVE_EVENT_LISTING_EVALUATION_COMMITMENT_V1: Option<&str> = None;

struct EventListingIdentitySetV1<'a> {
    profile_commitment_sha256: &'a str,
    approved_account_alias_sha256: &'a str,
    deck_list_sha256: &'a str,
    deck_manifest_commitment_sha256: &'a str,
    deck_format_sha256: &'a str,
    policy_deployment_commitment_sha256: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveEventListingEvaluationSliceV1 {
    LeagueSelectedListing,
    ChallengeSelectedListing,
}

const REQUIRED_EVENT_LISTING_SLICES_V1: [MtgoCompetitiveEventListingEvaluationSliceV1; 2] = [
    MtgoCompetitiveEventListingEvaluationSliceV1::LeagueSelectedListing,
    MtgoCompetitiveEventListingEvaluationSliceV1::ChallengeSelectedListing,
];

pub fn canonical_competitive_event_listing_evaluation_slices_v1(
) -> &'static [MtgoCompetitiveEventListingEvaluationSliceV1] {
    &REQUIRED_EVENT_LISTING_SLICES_V1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveEventListingEvaluationSpecV1 {
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
    pub minimum_exact_selection_accuracy_bps: u16,
    pub required_slices: Vec<MtgoCompetitiveEventListingEvaluationSliceV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveEventListingCorpusCaseV1 {
    pub case_id: String,
    pub slice: MtgoCompetitiveEventListingEvaluationSliceV1,
    pub source_manifest_sha256: String,
    pub source_canonical_bgra8_sha256: String,
    pub source_profile_binding_sha256: String,
    pub expected_lifecycle_snapshot_commitment_sha256: String,
    pub expected_target_commitment_sha256: String,
    pub expected_selection_commitment_sha256: String,
    pub annotator_alias_sha256: String,
    pub annotation_receipt_sha256: String,
    pub annotated_at_unix_millis: u64,
    pub unobscured_frame_visually_confirmed: bool,
    pub approved_account_identity_confirmed: bool,
    pub event_kind_identity_and_label_visually_confirmed: bool,
    pub label_region_visually_confirmed: bool,
    pub enabled_open_entry_review_control_visually_confirmed: bool,
    pub deck_format_and_policy_target_confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveEventListingCorpusManifestV1 {
    pub schema_version: u32,
    pub corpus_id: String,
    pub profile_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub annotation_protocol_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub cases: Vec<MtgoCompetitiveEventListingCorpusCaseV1>,
}

/// Commitment-only review of selected Event Browser listings. It cannot
/// classify, open Entry Review, enter an event, spend, or input.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoCompetitiveEventListingCorpusManifestV1;
/// fn cannot_act(value: &CheckedUntrustedMtgoCompetitiveEventListingCorpusManifestV1) {
///     let _ = value.open_entry_review();
///     let _ = value.input_command();
/// }
/// ```
pub struct CheckedUntrustedMtgoCompetitiveEventListingCorpusManifestV1 {
    manifest: MtgoCompetitiveEventListingCorpusManifestV1,
    manifest_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveEventListingCorpusManifestV1 {
    pub fn manifest_sha256_v1(&self) -> &str {
        &self.manifest_sha256
    }

    pub fn case_count_v1(&self) -> usize {
        self.manifest.cases.len()
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn permits_open_entry_review_v1(&self) -> bool {
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
pub struct MtgoCompetitiveEventListingPredictionV1 {
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
    pub target: MtgoCompetitiveEventListingTargetV1,
    pub selection: MtgoVisibleCompetitiveEventListingSelectionV1,
}

/// One checked-untrusted listing-parser declaration bound to an exact source.
/// The classifier exchange hashes remain caller supplied, so this value has no
/// runtime or action authority.
pub struct CheckedUntrustedMtgoCompetitiveEventListingPredictionV1 {
    record: MtgoCompetitiveEventListingPredictionV1,
    lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    selection: CheckedUntrustedMtgoCompetitiveEventListingSelectionV1,
    prediction_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveEventListingPredictionV1 {
    pub fn prediction_commitment_sha256_v1(&self) -> &str {
        &self.prediction_commitment_sha256
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn permits_open_entry_review_v1(&self) -> bool {
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

pub struct MtgoCompetitiveEventListingEvaluationCaseV1<'a> {
    pub case_id: String,
    pub source: &'a CheckedUntrustedMtgoCompetitiveNavigationSourceV1,
    pub expected_lifecycle: MtgoVisibleCompetitiveLifecycleSnapshotV1,
    pub expected_target: MtgoCompetitiveEventListingTargetV1,
    pub expected_selection: MtgoVisibleCompetitiveEventListingSelectionV1,
    pub prediction: Option<&'a CheckedUntrustedMtgoCompetitiveEventListingPredictionV1>,
}

/// Recomputed League and Challenge listing coverage and exact selection
/// accuracy. It has no serde, ratification, or action conversion.
pub struct CheckedUntrustedMtgoCompetitiveEventListingEvaluationV1 {
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
    exact_selection_count: u32,
    prediction_coverage_bps: u16,
    exact_selection_accuracy_bps: u16,
    minimum_observed_cases_per_slice: u32,
    minimum_prediction_coverage_bps_per_slice: u16,
    minimum_exact_selection_accuracy_bps_per_slice: u16,
    missing_slices: Vec<MtgoCompetitiveEventListingEvaluationSliceV1>,
    passes_declared_gate: bool,
}

impl CheckedUntrustedMtgoCompetitiveEventListingEvaluationV1 {
    pub fn evaluation_commitment_sha256_v1(&self) -> &str {
        &self.evaluation_commitment_sha256
    }

    pub fn unique_case_count_v1(&self) -> u32 {
        self.unique_case_count
    }

    pub fn prediction_count_v1(&self) -> u32 {
        self.prediction_count
    }

    pub fn exact_selection_count_v1(&self) -> u32 {
        self.exact_selection_count
    }

    pub fn prediction_coverage_bps_v1(&self) -> u16 {
        self.prediction_coverage_bps
    }

    pub fn exact_selection_accuracy_bps_v1(&self) -> u16 {
        self.exact_selection_accuracy_bps
    }

    pub fn minimum_observed_cases_per_slice_v1(&self) -> u32 {
        self.minimum_observed_cases_per_slice
    }

    pub fn minimum_prediction_coverage_bps_per_slice_v1(&self) -> u16 {
        self.minimum_prediction_coverage_bps_per_slice
    }

    pub fn minimum_exact_selection_accuracy_bps_per_slice_v1(&self) -> u16 {
        self.minimum_exact_selection_accuracy_bps_per_slice
    }

    pub fn missing_slices_v1(&self) -> &[MtgoCompetitiveEventListingEvaluationSliceV1] {
        &self.missing_slices
    }

    pub fn passes_declared_gate_v1(&self) -> bool {
        self.passes_declared_gate
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn permits_open_entry_review_v1(&self) -> bool {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoReviewedCompetitiveEventListingEvaluationRatificationCandidateV1 {
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

pub struct AdmittedMtgoCompetitiveEventListingEvaluationV1 {
    commitments: MtgoReviewedCompetitiveEventListingEvaluationRatificationCandidateV1,
    admission_commitment_sha256: String,
}

impl AdmittedMtgoCompetitiveEventListingEvaluationV1 {
    pub fn commitments_v1(
        &self,
    ) -> MtgoReviewedCompetitiveEventListingEvaluationRatificationCandidateV1 {
        self.commitments.clone()
    }

    pub fn admission_commitment_sha256_v1(&self) -> &str {
        &self.admission_commitment_sha256
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn permits_open_entry_review_v1(&self) -> bool {
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

pub fn check_untrusted_competitive_event_listing_corpus_manifest_v1(
    profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    policy_deployment_commitment_sha256: &str,
    manifest: MtgoCompetitiveEventListingCorpusManifestV1,
) -> Result<CheckedUntrustedMtgoCompetitiveEventListingCorpusManifestV1, MtgoContractErrorV1> {
    if manifest.schema_version != MTGO_COMPETITIVE_EVENT_LISTING_EVALUATION_SCHEMA_V1 {
        return Err(error_v1(
            "event_listing_corpus_schema",
            "expected schema version 1",
        ));
    }
    validate_identifier_v1(&manifest.corpus_id, "event_listing_corpus_id")?;
    validate_identity_set_v1(
        profile,
        deck,
        policy_deployment_commitment_sha256,
        EventListingIdentitySetV1 {
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
        "event_listing_corpus_annotation",
    )?;
    if manifest.cases.len() < REQUIRED_EVENT_LISTING_SLICES_V1.len()
        || manifest.cases.len() > 100_000
    {
        return Err(error_v1(
            "event_listing_corpus_case_count",
            "corpus must contain between two and 100000 reviewed cases",
        ));
    }
    let mut previous_case_id: Option<&str> = None;
    let mut source_manifests = HashSet::new();
    let mut source_frames = HashSet::new();
    for case in &manifest.cases {
        validate_identifier_v1(&case.case_id, "event_listing_corpus_case_id")?;
        if previous_case_id.is_some_and(|previous| previous >= case.case_id.as_str()) {
            return Err(error_v1(
                "event_listing_corpus_case_order",
                "case IDs must be unique and strictly increasing",
            ));
        }
        previous_case_id = Some(&case.case_id);
        for (digest, code) in [
            (
                &case.source_manifest_sha256,
                "event_listing_corpus_source_manifest",
            ),
            (
                &case.source_canonical_bgra8_sha256,
                "event_listing_corpus_source_frame",
            ),
            (
                &case.source_profile_binding_sha256,
                "event_listing_corpus_source_profile",
            ),
            (
                &case.expected_lifecycle_snapshot_commitment_sha256,
                "event_listing_corpus_lifecycle",
            ),
            (
                &case.expected_target_commitment_sha256,
                "event_listing_corpus_target",
            ),
            (
                &case.expected_selection_commitment_sha256,
                "event_listing_corpus_selection",
            ),
            (
                &case.annotator_alias_sha256,
                "event_listing_corpus_annotator",
            ),
            (
                &case.annotation_receipt_sha256,
                "event_listing_corpus_receipt",
            ),
        ] {
            validate_sha256_v1(digest, code)?;
        }
        if !source_manifests.insert(case.source_manifest_sha256.as_str())
            || !source_frames.insert(case.source_canonical_bgra8_sha256.as_str())
        {
            return Err(error_v1(
                "event_listing_corpus_duplicate_source",
                "each reviewed case must bind distinct manifest and frame commitments",
            ));
        }
        if case.annotated_at_unix_millis == 0
            || !case.unobscured_frame_visually_confirmed
            || !case.approved_account_identity_confirmed
            || !case.event_kind_identity_and_label_visually_confirmed
            || !case.label_region_visually_confirmed
            || !case.enabled_open_entry_review_control_visually_confirmed
            || !case.deck_format_and_policy_target_confirmed
        {
            return Err(error_v1(
                "event_listing_corpus_visual_review",
                "every listing case requires the complete visual and target review receipt",
            ));
        }
    }
    let bytes = serde_json::to_vec(&manifest)
        .map_err(|error| error_v1("event_listing_corpus_serialization", error.to_string()))?;
    let manifest_sha256 = commitment_v1(EVENT_LISTING_CORPUS_DOMAIN_V1, &[bytes.as_slice()]);
    Ok(
        CheckedUntrustedMtgoCompetitiveEventListingCorpusManifestV1 {
            manifest,
            manifest_sha256,
        },
    )
}

pub fn check_untrusted_competitive_event_listing_prediction_v1(
    profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    source: &CheckedUntrustedMtgoCompetitiveNavigationSourceV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    expected_policy_deployment_commitment_sha256: &str,
    prediction: MtgoCompetitiveEventListingPredictionV1,
) -> Result<CheckedUntrustedMtgoCompetitiveEventListingPredictionV1, MtgoContractErrorV1> {
    if prediction.schema_version != MTGO_COMPETITIVE_EVENT_LISTING_EVALUATION_SCHEMA_V1 {
        return Err(error_v1(
            "event_listing_prediction_schema",
            "expected schema version 1",
        ));
    }
    validate_source_profile_v1(profile, source)?;
    validate_identity_set_v1(
        profile,
        deck,
        expected_policy_deployment_commitment_sha256,
        EventListingIdentitySetV1 {
            profile_commitment_sha256: &prediction.profile_commitment_sha256,
            approved_account_alias_sha256: &prediction.target.approved_account_alias_sha256,
            deck_list_sha256: &prediction.deck_list_sha256,
            deck_manifest_commitment_sha256: &prediction.deck_manifest_commitment_sha256,
            deck_format_sha256: &prediction.deck_format_sha256,
            policy_deployment_commitment_sha256: &prediction.policy_deployment_commitment_sha256,
        },
    )?;
    for (digest, code) in [
        (
            &prediction.source_manifest_sha256,
            "event_listing_prediction_manifest",
        ),
        (
            &prediction.source_canonical_bgra8_sha256,
            "event_listing_prediction_frame",
        ),
        (
            &prediction.source_profile_binding_sha256,
            "event_listing_prediction_profile",
        ),
        (
            &prediction.classifier_request_sha256,
            "event_listing_prediction_request",
        ),
        (
            &prediction.classifier_response_sha256,
            "event_listing_prediction_response",
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
            "event_listing_prediction_source",
            "prediction must bind the exact source and distinct classifier exchange",
        ));
    }
    if prediction.target.deck_list_sha256 != prediction.deck_list_sha256
        || prediction.target.deck_manifest_commitment_sha256
            != prediction.deck_manifest_commitment_sha256
        || prediction.target.deck_format_sha256 != prediction.deck_format_sha256
        || prediction.target.policy_deployment_commitment_sha256
            != prediction.policy_deployment_commitment_sha256
    {
        return Err(error_v1(
            "event_listing_prediction_identity",
            "prediction target must bind the exact deck, format, and policy",
        ));
    }
    let lifecycle =
        validate_visible_competitive_lifecycle_snapshot_v1(prediction.lifecycle.clone())?;
    validate_snapshot_artifact_source_v1(&lifecycle, source)?;
    let selection = validate_visible_competitive_event_listing_selection_v1(
        lifecycle,
        deck,
        prediction.target.clone(),
        prediction.selection.clone(),
    )?;
    let lifecycle =
        validate_visible_competitive_lifecycle_snapshot_v1(prediction.lifecycle.clone())?;
    let bytes = serde_json::to_vec(&prediction)
        .map_err(|error| error_v1("event_listing_prediction_serialization", error.to_string()))?;
    let prediction_commitment_sha256 =
        commitment_v1(EVENT_LISTING_PREDICTION_DOMAIN_V1, &[bytes.as_slice()]);
    Ok(CheckedUntrustedMtgoCompetitiveEventListingPredictionV1 {
        record: prediction,
        lifecycle,
        selection,
        prediction_commitment_sha256,
    })
}

pub fn evaluate_untrusted_competitive_event_listing_profile_v1(
    profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    corpus: &CheckedUntrustedMtgoCompetitiveEventListingCorpusManifestV1,
    spec: MtgoCompetitiveEventListingEvaluationSpecV1,
    cases: Vec<MtgoCompetitiveEventListingEvaluationCaseV1<'_>>,
) -> Result<CheckedUntrustedMtgoCompetitiveEventListingEvaluationV1, MtgoContractErrorV1> {
    validate_evaluation_spec_v1(&spec)?;
    validate_identity_set_v1(
        profile,
        deck,
        &corpus.manifest.policy_deployment_commitment_sha256,
        EventListingIdentitySetV1 {
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
            "event_listing_evaluation_identity",
            "evaluation must bind the exact profile, account, corpus, annotation, deck, format, and policy",
        ));
    }
    if cases.is_empty() || cases.len() > 100_000 || cases.len() != corpus.manifest.cases.len() {
        return Err(error_v1(
            "event_listing_evaluation_case_count",
            "evaluated cases must exactly cover the reviewed corpus",
        ));
    }

    let spec_bytes = serde_json::to_vec(&spec).map_err(|error| {
        error_v1(
            "event_listing_evaluation_spec_serialization",
            error.to_string(),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(EVENT_LISTING_EVALUATION_DOMAIN_V1);
    hash_part_v1(&mut hasher, &spec_bytes);

    let mut previous_case_id: Option<&str> = None;
    let mut source_manifests = HashSet::new();
    let mut source_frames = HashSet::new();
    let mut slice_counts = HashMap::new();
    let mut slice_prediction_counts = HashMap::new();
    let mut slice_exact_counts = HashMap::new();
    let mut prediction_count = 0_u32;
    let mut exact_selection_count = 0_u32;

    for (case, reviewed) in cases.iter().zip(&corpus.manifest.cases) {
        validate_identifier_v1(&case.case_id, "event_listing_evaluation_case_id")?;
        if previous_case_id.is_some_and(|previous| previous >= case.case_id.as_str()) {
            return Err(error_v1(
                "event_listing_evaluation_case_order",
                "case IDs must be unique and strictly increasing",
            ));
        }
        previous_case_id = Some(&case.case_id);
        if !source_manifests.insert(case.source.manifest_sha256())
            || !source_frames.insert(case.source.canonical_bgra8_sha256())
        {
            return Err(error_v1(
                "event_listing_evaluation_duplicate_source",
                "each case must use a distinct reviewed source and frame",
            ));
        }
        validate_source_profile_v1(profile, case.source)?;
        let lifecycle =
            validate_visible_competitive_lifecycle_snapshot_v1(case.expected_lifecycle.clone())?;
        validate_snapshot_artifact_source_v1(&lifecycle, case.source)?;
        let expected = validate_visible_competitive_event_listing_selection_v1(
            lifecycle,
            deck,
            case.expected_target.clone(),
            case.expected_selection.clone(),
        )?;
        let slice = event_listing_slice_v1(expected.event_kind_v1());
        if expected.approved_account_alias_sha256_v1() != profile.approved_account_alias_sha256()
            || expected.deck_list_sha256_v1() != deck.deck_list_sha256()
            || expected.deck_manifest_commitment_sha256_v1() != deck.manifest_commitment_sha256()
            || expected.deck_format_sha256_v1() != deck.format_sha256()
            || expected.policy_deployment_commitment_sha256_v1()
                != spec.policy_deployment_commitment_sha256
        {
            return Err(error_v1(
                "event_listing_evaluation_expected_identity",
                "expected listing must bind the evaluated account, deck, format, and policy",
            ));
        }
        if reviewed.case_id != case.case_id
            || reviewed.slice != slice
            || reviewed.source_manifest_sha256 != case.source.manifest_sha256()
            || reviewed.source_canonical_bgra8_sha256 != case.source.canonical_bgra8_sha256()
            || reviewed.source_profile_binding_sha256 != case.source.source_profile_binding_sha256()
            || reviewed.expected_lifecycle_snapshot_commitment_sha256
                != case
                    .expected_selection
                    .source_lifecycle_snapshot_commitment_sha256
            || reviewed.expected_target_commitment_sha256 != expected.target_commitment_sha256_v1()
            || reviewed.expected_selection_commitment_sha256
                != expected.selection_commitment_sha256_v1()
        {
            return Err(error_v1(
                "event_listing_evaluation_corpus_case_binding",
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
                || prediction.lifecycle.frame_id_v1() != case.expected_selection.frame_id
                || prediction.lifecycle.frame_sequence() != case.expected_selection.frame_sequence
                || prediction.lifecycle.frame_sha256_v1() != case.expected_selection.frame_sha256
            {
                return Err(error_v1(
                    "event_listing_evaluation_prediction_source",
                    "prediction must refer to the exact annotated source, deck, policy, and frame",
                ));
            }
            prediction_count += 1;
            *slice_prediction_counts.entry(slice).or_insert(0_u32) += 1;
            let exact = listing_semantics_equal_v1(&expected, &prediction.selection);
            exact_selection_count += u32::from(exact);
            *slice_exact_counts.entry(slice).or_insert(0_u32) += u32::from(exact);
            (prediction.prediction_commitment_sha256_v1(), exact)
        } else {
            ("abstained", false)
        };

        for part in [
            case.case_id.as_bytes(),
            case.source.manifest_sha256().as_bytes(),
            case.source.canonical_bgra8_sha256().as_bytes(),
            expected.target_commitment_sha256_v1().as_bytes(),
            expected.selection_commitment_sha256_v1().as_bytes(),
            prediction_commitment.as_bytes(),
            &[u8::from(exact)],
        ] {
            hash_part_v1(&mut hasher, part);
        }
    }

    let unique_case_count = u32::try_from(cases.len()).map_err(|_| {
        error_v1(
            "event_listing_evaluation_case_count",
            "case count does not fit u32",
        )
    })?;
    let prediction_coverage_bps = ratio_bps_v1(prediction_count, unique_case_count);
    let exact_selection_accuracy_bps = ratio_bps_v1(exact_selection_count, prediction_count);
    let missing_slices = REQUIRED_EVENT_LISTING_SLICES_V1
        .iter()
        .copied()
        .filter(|slice| !slice_counts.contains_key(slice))
        .collect::<Vec<_>>();
    let minimum_observed_cases_per_slice = REQUIRED_EVENT_LISTING_SLICES_V1
        .iter()
        .map(|slice| slice_counts.get(slice).copied().unwrap_or(0))
        .min()
        .unwrap_or(0);
    let minimum_prediction_coverage_bps_per_slice = REQUIRED_EVENT_LISTING_SLICES_V1
        .iter()
        .map(|slice| {
            ratio_bps_v1(
                slice_prediction_counts.get(slice).copied().unwrap_or(0),
                slice_counts.get(slice).copied().unwrap_or(0),
            )
        })
        .min()
        .unwrap_or(0);
    let minimum_exact_selection_accuracy_bps_per_slice = REQUIRED_EVENT_LISTING_SLICES_V1
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
        && exact_selection_accuracy_bps >= spec.minimum_exact_selection_accuracy_bps
        && minimum_prediction_coverage_bps_per_slice >= spec.minimum_prediction_coverage_bps
        && minimum_exact_selection_accuracy_bps_per_slice
            >= spec.minimum_exact_selection_accuracy_bps;

    Ok(CheckedUntrustedMtgoCompetitiveEventListingEvaluationV1 {
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
        exact_selection_count,
        prediction_coverage_bps,
        exact_selection_accuracy_bps,
        minimum_observed_cases_per_slice,
        minimum_prediction_coverage_bps_per_slice,
        minimum_exact_selection_accuracy_bps_per_slice,
        missing_slices,
        passes_declared_gate,
    })
}

pub fn review_competitive_event_listing_evaluation_ratification_candidate_v1(
    evaluation: &CheckedUntrustedMtgoCompetitiveEventListingEvaluationV1,
) -> Result<MtgoReviewedCompetitiveEventListingEvaluationRatificationCandidateV1, MtgoContractErrorV1>
{
    if !evaluation.passes_declared_gate {
        return Err(error_v1(
            "event_listing_evaluation_declared_gate",
            evaluation.evaluation_commitment_sha256.clone(),
        ));
    }
    let ratification_commitment_sha256 = commitment_v1(
        EVENT_LISTING_EVALUATION_RATIFICATION_DOMAIN_V1,
        &[
            evaluation.profile_commitment_sha256.as_bytes(),
            evaluation.approved_account_alias_sha256.as_bytes(),
            evaluation.deck_list_sha256.as_bytes(),
            evaluation.deck_manifest_commitment_sha256.as_bytes(),
            evaluation.deck_format_sha256.as_bytes(),
            evaluation.policy_deployment_commitment_sha256.as_bytes(),
            evaluation.corpus_manifest_sha256.as_bytes(),
            evaluation.evaluation_commitment_sha256.as_bytes(),
            b"league_challenge_selected_event_listing_exact_semantics_v1",
            b"no_live_classification_no_open_review_no_event_entry_no_spending_no_coordinates_no_input",
        ],
    );
    Ok(
        MtgoReviewedCompetitiveEventListingEvaluationRatificationCandidateV1 {
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

pub fn admit_ratified_competitive_event_listing_evaluation_v1(
    evaluation: CheckedUntrustedMtgoCompetitiveEventListingEvaluationV1,
    candidate: MtgoReviewedCompetitiveEventListingEvaluationRatificationCandidateV1,
) -> Result<AdmittedMtgoCompetitiveEventListingEvaluationV1, MtgoContractErrorV1> {
    admit_competitive_event_listing_evaluation_against_ratification_v1(
        evaluation,
        candidate,
        RATIFIED_COMPETITIVE_EVENT_LISTING_EVALUATION_COMMITMENT_V1,
    )
}

fn admit_competitive_event_listing_evaluation_against_ratification_v1(
    evaluation: CheckedUntrustedMtgoCompetitiveEventListingEvaluationV1,
    candidate: MtgoReviewedCompetitiveEventListingEvaluationRatificationCandidateV1,
    ratified_commitment: Option<&str>,
) -> Result<AdmittedMtgoCompetitiveEventListingEvaluationV1, MtgoContractErrorV1> {
    let expected =
        review_competitive_event_listing_evaluation_ratification_candidate_v1(&evaluation)?;
    if candidate != expected {
        return Err(error_v1(
            "event_listing_evaluation_ratification_candidate",
            "candidate does not exactly match the checked evaluation",
        ));
    }
    let ratified = ratified_commitment.ok_or_else(|| {
        error_v1(
            "event_listing_evaluation_not_ratified",
            "production contains no ratified League and Challenge listing evaluation",
        )
    })?;
    validate_sha256_v1(ratified, "event_listing_evaluation_ratification")?;
    if candidate.ratification_commitment_sha256 != ratified {
        return Err(error_v1(
            "event_listing_evaluation_not_ratified",
            "candidate does not match the production ratification",
        ));
    }
    let admission_commitment_sha256 = commitment_v1(
        EVENT_LISTING_EVALUATION_ADMISSION_DOMAIN_V1,
        &[
            candidate.ratification_commitment_sha256.as_bytes(),
            candidate.evaluation_commitment_sha256.as_bytes(),
            candidate.profile_commitment_sha256.as_bytes(),
            candidate.approved_account_alias_sha256.as_bytes(),
            candidate.deck_manifest_commitment_sha256.as_bytes(),
            candidate.policy_deployment_commitment_sha256.as_bytes(),
            b"reviewed_event_listing_evaluation_only_no_runtime_pixels_no_input",
        ],
    );
    Ok(AdmittedMtgoCompetitiveEventListingEvaluationV1 {
        commitments: candidate,
        admission_commitment_sha256,
    })
}

fn validate_evaluation_spec_v1(
    spec: &MtgoCompetitiveEventListingEvaluationSpecV1,
) -> Result<(), MtgoContractErrorV1> {
    if spec.schema_version != MTGO_COMPETITIVE_EVENT_LISTING_EVALUATION_SCHEMA_V1 {
        return Err(error_v1(
            "event_listing_evaluation_schema",
            "expected schema version 1",
        ));
    }
    validate_identifier_v1(&spec.evaluation_id, "event_listing_evaluation_id")?;
    for (digest, code) in [
        (
            &spec.profile_commitment_sha256,
            "event_listing_evaluation_profile",
        ),
        (
            &spec.corpus_manifest_sha256,
            "event_listing_evaluation_corpus",
        ),
        (
            &spec.annotation_protocol_sha256,
            "event_listing_evaluation_annotation",
        ),
        (
            &spec.evaluator_binary_sha256,
            "event_listing_evaluation_binary",
        ),
        (&spec.deck_list_sha256, "event_listing_evaluation_deck"),
        (
            &spec.deck_manifest_commitment_sha256,
            "event_listing_evaluation_manifest",
        ),
        (&spec.deck_format_sha256, "event_listing_evaluation_format"),
        (
            &spec.policy_deployment_commitment_sha256,
            "event_listing_evaluation_policy",
        ),
    ] {
        validate_sha256_v1(digest, code)?;
    }
    if spec.minimum_unique_cases_per_slice == 0 || spec.minimum_unique_cases_per_slice > 50_000 {
        return Err(error_v1(
            "event_listing_evaluation_minimum_cases",
            spec.minimum_unique_cases_per_slice.to_string(),
        ));
    }
    if !(9_500..=10_000).contains(&spec.minimum_prediction_coverage_bps) {
        return Err(error_v1(
            "event_listing_evaluation_coverage_threshold",
            spec.minimum_prediction_coverage_bps.to_string(),
        ));
    }
    if spec.minimum_exact_selection_accuracy_bps != 10_000 {
        return Err(error_v1(
            "event_listing_evaluation_accuracy_threshold",
            "selected event semantics and localization must be exact",
        ));
    }
    if spec.required_slices != REQUIRED_EVENT_LISTING_SLICES_V1 {
        return Err(error_v1(
            "event_listing_evaluation_required_slices",
            "League and Challenge selected listings are required in canonical order",
        ));
    }
    Ok(())
}

fn validate_identity_set_v1(
    profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    expected_policy_deployment_commitment_sha256: &str,
    identity: EventListingIdentitySetV1<'_>,
) -> Result<(), MtgoContractErrorV1> {
    validate_sha256_v1(
        expected_policy_deployment_commitment_sha256,
        "event_listing_evaluation_policy",
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
            "event_listing_evaluation_identity",
            "profile, approved account, deck, format, or policy commitment differs",
        ));
    }
    Ok(())
}

fn event_listing_slice_v1(
    event_kind: MtgoCompetitiveEventKindV1,
) -> MtgoCompetitiveEventListingEvaluationSliceV1 {
    match event_kind {
        MtgoCompetitiveEventKindV1::League => {
            MtgoCompetitiveEventListingEvaluationSliceV1::LeagueSelectedListing
        }
        MtgoCompetitiveEventKindV1::Challenge => {
            MtgoCompetitiveEventListingEvaluationSliceV1::ChallengeSelectedListing
        }
    }
}

fn listing_semantics_equal_v1(
    expected: &CheckedUntrustedMtgoCompetitiveEventListingSelectionV1,
    predicted: &CheckedUntrustedMtgoCompetitiveEventListingSelectionV1,
) -> bool {
    let expected_raw = expected.raw_for_evaluation_v1();
    let predicted_raw = predicted.raw_for_evaluation_v1();
    expected.target_commitment_sha256_v1() == predicted.target_commitment_sha256_v1()
        && expected_raw.schema_version == predicted_raw.schema_version
        && expected_raw.target_commitment_sha256 == predicted_raw.target_commitment_sha256
        && expected_raw.event_kind == predicted_raw.event_kind
        && expected_raw.event_identity_sha256 == predicted_raw.event_identity_sha256
        && expected_raw.event_display_label_sha256 == predicted_raw.event_display_label_sha256
        && expected_raw.source_lifecycle_snapshot_commitment_sha256
            == predicted_raw.source_lifecycle_snapshot_commitment_sha256
        && expected_raw.frame_id == predicted_raw.frame_id
        && expected_raw.frame_sequence == predicted_raw.frame_sequence
        && expected_raw.frame_sha256 == predicted_raw.frame_sha256
        && expected_raw.client_bounds == predicted_raw.client_bounds
        && expected_raw.event_label_rect_client_px == predicted_raw.event_label_rect_client_px
        && expected_raw.event_label_region_sha256 == predicted_raw.event_label_region_sha256
        && expected_raw.open_entry_review_control_rect_client_px
            == predicted_raw.open_entry_review_control_rect_client_px
        && expected_raw.open_entry_review_control_region_sha256
            == predicted_raw.open_entry_review_control_region_sha256
        && expected_raw.open_entry_review_control_enabled
            == predicted_raw.open_entry_review_control_enabled
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
        competitive_event_listing_target_commitment_v1, validate_competitive_deck_manifest_v1,
        MtgoCompetitiveDeckCardCountV1, MtgoCompetitiveDeckConfigurationV1,
        MtgoCompetitiveDeckManifestV1, MtgoCompetitiveLifecyclePhaseV1,
        MtgoCompetitiveNavigationRuntimeProfileV1, MtgoLifecycleVisibleFactKindV1,
        MtgoLifecycleVisibleFactV1, MtgoRectPxV1, MtgoSizePxV1,
        MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1, MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
        MTGO_COMPETITIVE_NAVIGATION_EVALUATION_SCHEMA_V1, MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
    };
    use mtg_kernel::card_def::card_id_by_name;

    fn digest(byte: char) -> String {
        byte.to_string().repeat(64)
    }

    fn profile() -> CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1 {
        check_untrusted_competitive_navigation_runtime_profile_v1(
            MtgoCompetitiveNavigationRuntimeProfileV1 {
                schema_version: MTGO_COMPETITIVE_NAVIGATION_EVALUATION_SCHEMA_V1,
                profile_id: "event-listing-evaluation-test-v1".to_owned(),
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
        let mut mainboard = vec![
            MtgoCompetitiveDeckCardCountV1 {
                card_db_id: card_id_by_name("Lightning Bolt").unwrap(),
                card_name: "Lightning Bolt".to_owned(),
                count: 2,
            },
            MtgoCompetitiveDeckCardCountV1 {
                card_db_id: card_id_by_name("Mountain").unwrap(),
                card_name: "Mountain".to_owned(),
                count: 3,
            },
        ];
        mainboard.sort_by_key(|card| card.card_db_id);
        validate_competitive_deck_manifest_v1(MtgoCompetitiveDeckManifestV1 {
            schema_version: MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
            deck_list_sha256: digest('0'),
            format_sha256: digest('1'),
            starting_mainboard_count: 5,
            starting_sideboard_count: 2,
            configuration: MtgoCompetitiveDeckConfigurationV1 {
                mainboard,
                sideboard: vec![MtgoCompetitiveDeckCardCountV1 {
                    card_db_id: card_id_by_name("Searing Blaze").unwrap(),
                    card_name: "Searing Blaze".to_owned(),
                    count: 2,
                }],
            },
        })
        .unwrap()
    }

    fn lifecycle(
        source: &CheckedUntrustedMtgoCompetitiveNavigationSourceV1,
        event_kind: MtgoCompetitiveEventKindV1,
        frame_id: u64,
    ) -> MtgoVisibleCompetitiveLifecycleSnapshotV1 {
        MtgoVisibleCompetitiveLifecycleSnapshotV1 {
            schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
            snapshot_id: format!("event-listing-evaluation-lifecycle-{frame_id}"),
            event_kind,
            phase: MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
            frame_id,
            frame_sequence: frame_id,
            frame_sha256: source.canonical_bgra8_sha256().to_owned(),
            client_bounds: MtgoRectPxV1 {
                x: 0,
                y: 0,
                width: 1_550,
                height: 925,
            },
            event_identity_sha256: None,
            match_identity_sha256: None,
            game_number: None,
            entry_terms: None,
            visible_state_complete: true,
            facts: vec![MtgoLifecycleVisibleFactV1 {
                kind: MtgoLifecycleVisibleFactKindV1::EventBrowserVisible,
                rect_client_px: MtgoRectPxV1 {
                    x: 30,
                    y: 80,
                    width: 200,
                    height: 80,
                },
                content_sha256: format!("{:064x}", frame_id + 20),
                confidence_bps: 10_000,
            }],
        }
    }

    fn target(
        profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
        deck: &ValidatedMtgoCompetitiveDeckManifestV1,
        event_kind: MtgoCompetitiveEventKindV1,
        index: usize,
    ) -> MtgoCompetitiveEventListingTargetV1 {
        MtgoCompetitiveEventListingTargetV1 {
            schema_version: MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1,
            target_id: format!("event-listing-target-{:02}", index + 1),
            event_kind,
            approved_account_alias_sha256: profile.approved_account_alias_sha256().to_owned(),
            event_identity_sha256: format!("{:064x}", index + 100),
            event_display_label_sha256: format!("{:064x}", index + 110),
            deck_list_sha256: deck.deck_list_sha256().to_owned(),
            deck_manifest_commitment_sha256: deck.manifest_commitment_sha256().to_owned(),
            deck_format_sha256: deck.format_sha256().to_owned(),
            policy_deployment_commitment_sha256: digest('7'),
        }
    }

    fn selection(
        lifecycle: &MtgoVisibleCompetitiveLifecycleSnapshotV1,
        target: &MtgoCompetitiveEventListingTargetV1,
        deck: &ValidatedMtgoCompetitiveDeckManifestV1,
        index: usize,
    ) -> MtgoVisibleCompetitiveEventListingSelectionV1 {
        let checked_lifecycle =
            validate_visible_competitive_lifecycle_snapshot_v1(lifecycle.clone()).unwrap();
        MtgoVisibleCompetitiveEventListingSelectionV1 {
            schema_version: MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1,
            selection_id: format!("event-listing-selection-{:02}", index + 1),
            target_commitment_sha256: competitive_event_listing_target_commitment_v1(target, deck)
                .unwrap(),
            event_kind: target.event_kind,
            event_identity_sha256: target.event_identity_sha256.clone(),
            event_display_label_sha256: target.event_display_label_sha256.clone(),
            source_lifecycle_snapshot_commitment_sha256: checked_lifecycle
                .snapshot_commitment_sha256()
                .to_owned(),
            frame_id: lifecycle.frame_id,
            frame_sequence: lifecycle.frame_sequence,
            frame_sha256: lifecycle.frame_sha256.clone(),
            client_bounds: lifecycle.client_bounds.clone(),
            event_label_rect_client_px: MtgoRectPxV1 {
                x: 300,
                y: 200,
                width: 400,
                height: 80,
            },
            event_label_region_sha256: format!("{:064x}", index + 120),
            open_entry_review_control_rect_client_px: MtgoRectPxV1 {
                x: 900,
                y: 700,
                width: 220,
                height: 60,
            },
            open_entry_review_control_region_sha256: format!("{:064x}", index + 130),
            open_entry_review_control_enabled: true,
            confidence_bps: 10_000,
        }
    }

    fn prediction(
        profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
        source: &CheckedUntrustedMtgoCompetitiveNavigationSourceV1,
        deck: &ValidatedMtgoCompetitiveDeckManifestV1,
        lifecycle: MtgoVisibleCompetitiveLifecycleSnapshotV1,
        target: MtgoCompetitiveEventListingTargetV1,
        selection: MtgoVisibleCompetitiveEventListingSelectionV1,
        index: usize,
    ) -> CheckedUntrustedMtgoCompetitiveEventListingPredictionV1 {
        check_untrusted_competitive_event_listing_prediction_v1(
            profile,
            source,
            deck,
            &digest('7'),
            MtgoCompetitiveEventListingPredictionV1 {
                schema_version: MTGO_COMPETITIVE_EVENT_LISTING_EVALUATION_SCHEMA_V1,
                profile_commitment_sha256: profile.profile_commitment_sha256().to_owned(),
                source_manifest_sha256: source.manifest_sha256().to_owned(),
                source_canonical_bgra8_sha256: source.canonical_bgra8_sha256().to_owned(),
                source_profile_binding_sha256: source.source_profile_binding_sha256().to_owned(),
                classifier_request_sha256: format!("{:064x}", index + 200),
                classifier_response_sha256: format!("{:064x}", index + 210),
                deck_list_sha256: deck.deck_list_sha256().to_owned(),
                deck_manifest_commitment_sha256: deck.manifest_commitment_sha256().to_owned(),
                deck_format_sha256: deck.format_sha256().to_owned(),
                policy_deployment_commitment_sha256: digest('7'),
                lifecycle,
                target,
                selection,
            },
        )
        .unwrap()
    }

    struct Fixture {
        profile: CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
        deck: ValidatedMtgoCompetitiveDeckManifestV1,
        sources: Vec<CheckedUntrustedMtgoCompetitiveNavigationSourceV1>,
        lifecycles: Vec<MtgoVisibleCompetitiveLifecycleSnapshotV1>,
        targets: Vec<MtgoCompetitiveEventListingTargetV1>,
        selections: Vec<MtgoVisibleCompetitiveEventListingSelectionV1>,
        predictions: Vec<CheckedUntrustedMtgoCompetitiveEventListingPredictionV1>,
        corpus: CheckedUntrustedMtgoCompetitiveEventListingCorpusManifestV1,
        spec: MtgoCompetitiveEventListingEvaluationSpecV1,
    }

    fn fixture_with_kinds(kinds: [MtgoCompetitiveEventKindV1; 2]) -> Fixture {
        let profile = profile();
        let deck = deck();
        let sources = (1_u8..=2)
            .map(|value| {
                checked_untrusted_competitive_navigation_source_for_test_v1(&profile, value)
            })
            .collect::<Vec<_>>();
        let lifecycles = sources
            .iter()
            .enumerate()
            .map(|(index, source)| lifecycle(source, kinds[index], index as u64 + 1))
            .collect::<Vec<_>>();
        let targets = kinds
            .iter()
            .copied()
            .enumerate()
            .map(|(index, kind)| target(&profile, &deck, kind, index))
            .collect::<Vec<_>>();
        let selections = lifecycles
            .iter()
            .zip(&targets)
            .enumerate()
            .map(|(index, (lifecycle, target))| selection(lifecycle, target, &deck, index))
            .collect::<Vec<_>>();
        let predictions = (0..2)
            .map(|index| {
                prediction(
                    &profile,
                    &sources[index],
                    &deck,
                    lifecycles[index].clone(),
                    targets[index].clone(),
                    selections[index].clone(),
                    index,
                )
            })
            .collect::<Vec<_>>();
        let corpus_cases = (0..2)
            .map(|index| {
                let checked_lifecycle =
                    validate_visible_competitive_lifecycle_snapshot_v1(lifecycles[index].clone())
                        .unwrap();
                let checked_selection = validate_visible_competitive_event_listing_selection_v1(
                    checked_lifecycle,
                    &deck,
                    targets[index].clone(),
                    selections[index].clone(),
                )
                .unwrap();
                MtgoCompetitiveEventListingCorpusCaseV1 {
                    case_id: format!("case-{:02}", index + 1),
                    slice: event_listing_slice_v1(kinds[index]),
                    source_manifest_sha256: sources[index].manifest_sha256().to_owned(),
                    source_canonical_bgra8_sha256: sources[index]
                        .canonical_bgra8_sha256()
                        .to_owned(),
                    source_profile_binding_sha256: sources[index]
                        .source_profile_binding_sha256()
                        .to_owned(),
                    expected_lifecycle_snapshot_commitment_sha256: selections[index]
                        .source_lifecycle_snapshot_commitment_sha256
                        .clone(),
                    expected_target_commitment_sha256: checked_selection
                        .target_commitment_sha256_v1()
                        .to_owned(),
                    expected_selection_commitment_sha256: checked_selection
                        .selection_commitment_sha256_v1()
                        .to_owned(),
                    annotator_alias_sha256: digest('8'),
                    annotation_receipt_sha256: format!("{:064x}", index + 300),
                    annotated_at_unix_millis: 1_786_400_000_000 + index as u64,
                    unobscured_frame_visually_confirmed: true,
                    approved_account_identity_confirmed: true,
                    event_kind_identity_and_label_visually_confirmed: true,
                    label_region_visually_confirmed: true,
                    enabled_open_entry_review_control_visually_confirmed: true,
                    deck_format_and_policy_target_confirmed: true,
                }
            })
            .collect::<Vec<_>>();
        let corpus = check_untrusted_competitive_event_listing_corpus_manifest_v1(
            &profile,
            &deck,
            &digest('7'),
            MtgoCompetitiveEventListingCorpusManifestV1 {
                schema_version: MTGO_COMPETITIVE_EVENT_LISTING_EVALUATION_SCHEMA_V1,
                corpus_id: "event-listing-corpus-test-v1".to_owned(),
                profile_commitment_sha256: profile.profile_commitment_sha256().to_owned(),
                approved_account_alias_sha256: profile.approved_account_alias_sha256().to_owned(),
                annotation_protocol_sha256: digest('9'),
                deck_list_sha256: deck.deck_list_sha256().to_owned(),
                deck_manifest_commitment_sha256: deck.manifest_commitment_sha256().to_owned(),
                deck_format_sha256: deck.format_sha256().to_owned(),
                policy_deployment_commitment_sha256: digest('7'),
                cases: corpus_cases,
            },
        )
        .unwrap();
        let spec = MtgoCompetitiveEventListingEvaluationSpecV1 {
            schema_version: MTGO_COMPETITIVE_EVENT_LISTING_EVALUATION_SCHEMA_V1,
            evaluation_id: "event-listing-evaluation-test-v1".to_owned(),
            profile_commitment_sha256: profile.profile_commitment_sha256().to_owned(),
            corpus_manifest_sha256: corpus.manifest_sha256_v1().to_owned(),
            annotation_protocol_sha256: digest('9'),
            evaluator_binary_sha256: digest('a'),
            deck_list_sha256: deck.deck_list_sha256().to_owned(),
            deck_manifest_commitment_sha256: deck.manifest_commitment_sha256().to_owned(),
            deck_format_sha256: deck.format_sha256().to_owned(),
            policy_deployment_commitment_sha256: digest('7'),
            minimum_unique_cases_per_slice: 1,
            minimum_prediction_coverage_bps: 10_000,
            minimum_exact_selection_accuracy_bps: 10_000,
            required_slices: REQUIRED_EVENT_LISTING_SLICES_V1.to_vec(),
        };
        Fixture {
            profile,
            deck,
            sources,
            lifecycles,
            targets,
            selections,
            predictions,
            corpus,
            spec,
        }
    }

    fn fixture() -> Fixture {
        fixture_with_kinds([
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ])
    }

    fn cases<'a>(fixture: &'a Fixture) -> Vec<MtgoCompetitiveEventListingEvaluationCaseV1<'a>> {
        (0..2)
            .map(|index| MtgoCompetitiveEventListingEvaluationCaseV1 {
                case_id: format!("case-{:02}", index + 1),
                source: &fixture.sources[index],
                expected_lifecycle: fixture.lifecycles[index].clone(),
                expected_target: fixture.targets[index].clone(),
                expected_selection: fixture.selections[index].clone(),
                prediction: Some(&fixture.predictions[index]),
            })
            .collect()
    }

    #[test]
    fn exact_league_and_challenge_evaluation_passes_but_production_root_is_empty() {
        let fixture = fixture();
        let evaluation = evaluate_untrusted_competitive_event_listing_profile_v1(
            &fixture.profile,
            &fixture.deck,
            &fixture.corpus,
            fixture.spec.clone(),
            cases(&fixture),
        )
        .unwrap();
        assert_eq!(evaluation.unique_case_count_v1(), 2);
        assert_eq!(evaluation.prediction_count_v1(), 2);
        assert_eq!(evaluation.exact_selection_count_v1(), 2);
        assert_eq!(evaluation.prediction_coverage_bps_v1(), 10_000);
        assert_eq!(evaluation.exact_selection_accuracy_bps_v1(), 10_000);
        assert_eq!(evaluation.minimum_observed_cases_per_slice_v1(), 1);
        assert_eq!(
            evaluation.minimum_prediction_coverage_bps_per_slice_v1(),
            10_000
        );
        assert_eq!(
            evaluation.minimum_exact_selection_accuracy_bps_per_slice_v1(),
            10_000
        );
        assert!(evaluation.passes_declared_gate_v1());
        let candidate =
            review_competitive_event_listing_evaluation_ratification_candidate_v1(&evaluation)
                .unwrap();
        let error = admit_ratified_competitive_event_listing_evaluation_v1(evaluation, candidate)
            .err()
            .unwrap();
        assert_eq!(error.code(), "event_listing_evaluation_not_ratified");
    }

    #[test]
    fn exact_private_ratification_still_grants_no_action_authority() {
        let fixture = fixture();
        let evaluation = evaluate_untrusted_competitive_event_listing_profile_v1(
            &fixture.profile,
            &fixture.deck,
            &fixture.corpus,
            fixture.spec.clone(),
            cases(&fixture),
        )
        .unwrap();
        let candidate =
            review_competitive_event_listing_evaluation_ratification_candidate_v1(&evaluation)
                .unwrap();
        let ratified = candidate.ratification_commitment_sha256.clone();
        let admitted = admit_competitive_event_listing_evaluation_against_ratification_v1(
            evaluation,
            candidate,
            Some(&ratified),
        )
        .unwrap();
        assert!(!admitted.safe_for_live_classification_v1());
        assert!(!admitted.permits_open_entry_review_v1());
        assert!(!admitted.permits_event_entry_v1());
        assert!(!admitted.permits_spending_v1());
        assert!(!admitted.safe_for_input_v1());
    }

    #[test]
    fn abstention_fails_per_mode_coverage() {
        let fixture = fixture();
        let mut evaluation_cases = cases(&fixture);
        evaluation_cases[1].prediction = None;
        let evaluation = evaluate_untrusted_competitive_event_listing_profile_v1(
            &fixture.profile,
            &fixture.deck,
            &fixture.corpus,
            fixture.spec.clone(),
            evaluation_cases,
        )
        .unwrap();
        assert_eq!(evaluation.prediction_coverage_bps_v1(), 5_000);
        assert_eq!(evaluation.minimum_prediction_coverage_bps_per_slice_v1(), 0);
        assert!(!evaluation.passes_declared_gate_v1());
    }

    #[test]
    fn wrong_but_valid_event_selection_fails_exact_accuracy() {
        let fixture = fixture();
        let mut wrong_target = fixture.targets[0].clone();
        wrong_target.target_id = "wrong-valid-league-target-v1".to_owned();
        wrong_target.event_identity_sha256 = digest('e');
        wrong_target.event_display_label_sha256 = digest('f');
        let mut wrong_selection = fixture.selections[0].clone();
        wrong_selection.selection_id = "wrong-valid-league-selection-v1".to_owned();
        wrong_selection.target_commitment_sha256 =
            competitive_event_listing_target_commitment_v1(&wrong_target, &fixture.deck).unwrap();
        wrong_selection.event_identity_sha256 = wrong_target.event_identity_sha256.clone();
        wrong_selection.event_display_label_sha256 =
            wrong_target.event_display_label_sha256.clone();
        let wrong_prediction = prediction(
            &fixture.profile,
            &fixture.sources[0],
            &fixture.deck,
            fixture.lifecycles[0].clone(),
            wrong_target,
            wrong_selection,
            20,
        );
        let mut evaluation_cases = cases(&fixture);
        evaluation_cases[0].prediction = Some(&wrong_prediction);
        let evaluation = evaluate_untrusted_competitive_event_listing_profile_v1(
            &fixture.profile,
            &fixture.deck,
            &fixture.corpus,
            fixture.spec.clone(),
            evaluation_cases,
        )
        .unwrap();
        assert_eq!(evaluation.exact_selection_count_v1(), 1);
        assert_eq!(evaluation.exact_selection_accuracy_bps_v1(), 5_000);
        assert_eq!(
            evaluation.minimum_exact_selection_accuracy_bps_per_slice_v1(),
            0
        );
        assert!(!evaluation.passes_declared_gate_v1());
    }

    #[test]
    fn trace_id_and_valid_confidence_do_not_change_selection_semantics() {
        let fixture = fixture();
        let mut equivalent_selection = fixture.selections[0].clone();
        equivalent_selection.selection_id = "different-parser-trace-id-v1".to_owned();
        equivalent_selection.confidence_bps = 9_500;
        let equivalent_prediction = prediction(
            &fixture.profile,
            &fixture.sources[0],
            &fixture.deck,
            fixture.lifecycles[0].clone(),
            fixture.targets[0].clone(),
            equivalent_selection,
            30,
        );
        let mut evaluation_cases = cases(&fixture);
        evaluation_cases[0].prediction = Some(&equivalent_prediction);
        let evaluation = evaluate_untrusted_competitive_event_listing_profile_v1(
            &fixture.profile,
            &fixture.deck,
            &fixture.corpus,
            fixture.spec.clone(),
            evaluation_cases,
        )
        .unwrap();
        assert_eq!(evaluation.exact_selection_count_v1(), 2);
        assert_eq!(evaluation.exact_selection_accuracy_bps_v1(), 10_000);
        assert!(evaluation.passes_declared_gate_v1());
    }

    #[test]
    fn prediction_source_substitution_is_rejected() {
        let fixture = fixture();
        let mut evaluation_cases = cases(&fixture);
        evaluation_cases[0].prediction = Some(&fixture.predictions[1]);
        let error = evaluate_untrusted_competitive_event_listing_profile_v1(
            &fixture.profile,
            &fixture.deck,
            &fixture.corpus,
            fixture.spec.clone(),
            evaluation_cases,
        )
        .err()
        .unwrap();
        assert_eq!(error.code(), "event_listing_evaluation_prediction_source");
    }

    #[test]
    fn incomplete_visual_review_is_rejected() {
        let fixture = fixture();
        let mut manifest = fixture.corpus.manifest.clone();
        manifest.cases[0].label_region_visually_confirmed = false;
        let error = check_untrusted_competitive_event_listing_corpus_manifest_v1(
            &fixture.profile,
            &fixture.deck,
            &digest('7'),
            manifest,
        )
        .err()
        .unwrap();
        assert_eq!(error.code(), "event_listing_corpus_visual_review");
    }

    #[test]
    fn weak_accuracy_threshold_is_rejected() {
        let fixture = fixture();
        let mut spec = fixture.spec.clone();
        spec.minimum_exact_selection_accuracy_bps = 9_999;
        let error = evaluate_untrusted_competitive_event_listing_profile_v1(
            &fixture.profile,
            &fixture.deck,
            &fixture.corpus,
            spec,
            cases(&fixture),
        )
        .err()
        .unwrap();
        assert_eq!(error.code(), "event_listing_evaluation_accuracy_threshold");
    }

    #[test]
    fn missing_challenge_slice_fails_the_declared_gate() {
        let fixture = fixture_with_kinds([
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::League,
        ]);
        let evaluation = evaluate_untrusted_competitive_event_listing_profile_v1(
            &fixture.profile,
            &fixture.deck,
            &fixture.corpus,
            fixture.spec.clone(),
            cases(&fixture),
        )
        .unwrap();
        assert_eq!(
            evaluation.missing_slices_v1(),
            &[MtgoCompetitiveEventListingEvaluationSliceV1::ChallengeSelectedListing]
        );
        assert!(!evaluation.passes_declared_gate_v1());
    }

    #[test]
    fn serde_rejects_unknown_evaluation_fields() {
        let fixture = fixture();
        let mut value = serde_json::to_value(&fixture.spec).unwrap();
        value["fabricated_live_authority"] = serde_json::json!(true);
        assert!(
            serde_json::from_value::<MtgoCompetitiveEventListingEvaluationSpecV1>(value).is_err()
        );
    }
}
