use crate::{
    check_untrusted_dxgi_capture_artifact_v1, validate_visible_competitive_lifecycle_snapshot_v1,
    visible_frame_region_content_sha256_v1, CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    CheckedUntrustedMtgoDxgiCaptureArtifactV1, MtgoCompetitiveEventKindV1,
    MtgoCompetitiveLifecyclePhaseV1, MtgoContractErrorV1, MtgoDxgiCaptureRoleV2, MtgoRectPxV1,
    MtgoSizePxV1, MtgoVisibleCompetitiveLifecycleSnapshotV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

pub const MTGO_COMPETITIVE_NAVIGATION_EVALUATION_SCHEMA_V1: u32 = 1;

const CANONICAL_PIXEL_FORMAT_V1: &str = "bgra8_unorm_top_down_tightly_packed_v1";
const NAVIGATION_PROFILE_DOMAIN_V1: &[u8] = b"mtgo-competitive-navigation-profile-v1";
const NAVIGATION_EVALUATION_DOMAIN_V1: &[u8] = b"mtgo-competitive-navigation-evaluation-v1";
const NAVIGATION_PREDICTION_DOMAIN_V1: &[u8] = b"mtgo-competitive-navigation-prediction-v1";
const NAVIGATION_PROFILE_ADMISSION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-navigation-profile-admission-v1";
const NAVIGATION_SOURCE_PROFILE_BINDING_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-navigation-source-profile-binding-v1";
const RATIFIED_COMPETITIVE_NAVIGATION_EVALUATION_COMMITMENT_V1: Option<&str> = None;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveNavigationSliceV1 {
    LeagueEventBrowser,
    LeagueEntryReview,
    ChallengeEventBrowser,
    ChallengeEntryReview,
}

const REQUIRED_NAVIGATION_SLICES_V1: [MtgoCompetitiveNavigationSliceV1; 4] = [
    MtgoCompetitiveNavigationSliceV1::LeagueEventBrowser,
    MtgoCompetitiveNavigationSliceV1::LeagueEntryReview,
    MtgoCompetitiveNavigationSliceV1::ChallengeEventBrowser,
    MtgoCompetitiveNavigationSliceV1::ChallengeEntryReview,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveNavigationRuntimeProfileV1 {
    pub schema_version: u32,
    pub profile_id: String,
    pub executable_sha256: String,
    pub signer_thumbprint: String,
    pub signer_subject_sha256: String,
    pub window_title_sha256: String,
    pub approved_account_alias_sha256: String,
    pub account_identity_rect_client_px: MtgoRectPxV1,
    pub account_identity_region_sha256: String,
    pub dpi: u32,
    pub client_size_px: MtgoSizePxV1,
    pub output_identity_sha256: String,
    pub canonical_pixel_format: String,
    pub classifier_binary_sha256: String,
    pub classifier_assets_manifest_sha256: String,
    pub supported_slices: Vec<MtgoCompetitiveNavigationSliceV1>,
}

/// Structurally checked runtime and classifier identity. The profile remains
/// caller supplied and grants no capture, classification, entry, spending, or
/// input authority.
pub struct CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1 {
    payload: MtgoCompetitiveNavigationRuntimeProfileV1,
    profile_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1 {
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

    pub fn window_title_sha256(&self) -> &str {
        &self.payload.window_title_sha256
    }

    pub fn approved_account_alias_sha256(&self) -> &str {
        &self.payload.approved_account_alias_sha256
    }

    pub fn account_identity_rect_client_px(&self) -> &MtgoRectPxV1 {
        &self.payload.account_identity_rect_client_px
    }

    pub fn account_identity_region_sha256(&self) -> &str {
        &self.payload.account_identity_region_sha256
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

    pub fn classifier_binary_sha256(&self) -> &str {
        &self.payload.classifier_binary_sha256
    }

    pub fn classifier_assets_manifest_sha256(&self) -> &str {
        &self.payload.classifier_assets_manifest_sha256
    }

    pub fn supported_slices(&self) -> &[MtgoCompetitiveNavigationSliceV1] {
        &self.payload.supported_slices
    }

    pub fn safe_for_live_classification(&self) -> bool {
        false
    }

    pub fn permits_event_entry(&self) -> bool {
        false
    }

    pub fn permits_spending(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

/// One structurally checked main-client artifact whose visible account-identity
/// region was rehashed from the supplied canonical pixels against the exact
/// navigation profile. The canonical pixels are not retained or exposed.
pub struct CheckedUntrustedMtgoCompetitiveNavigationSourceV1 {
    artifact: CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    account_identity_region_sha256: String,
    source_profile_binding_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveNavigationSourceV1 {
    pub fn manifest_sha256(&self) -> &str {
        self.artifact.manifest_sha256()
    }

    pub fn canonical_bgra8_sha256(&self) -> &str {
        self.artifact.canonical_bgra8_sha256()
    }

    pub fn source_profile_binding_sha256(&self) -> &str {
        &self.source_profile_binding_sha256
    }

    pub fn safe_for_live_classification(&self) -> bool {
        false
    }

    pub fn permits_event_entry(&self) -> bool {
        false
    }

    pub fn permits_spending(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveNavigationEvaluationSpecV1 {
    pub schema_version: u32,
    pub evaluation_id: String,
    pub profile_commitment_sha256: String,
    pub corpus_manifest_sha256: String,
    pub annotation_protocol_sha256: String,
    pub evaluator_binary_sha256: String,
    pub minimum_unique_cases_per_slice: u32,
    pub minimum_prediction_coverage_bps: u16,
    pub required_slices: Vec<MtgoCompetitiveNavigationSliceV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveNavigationPredictionV1 {
    pub schema_version: u32,
    pub profile_commitment_sha256: String,
    pub source_manifest_sha256: String,
    pub source_canonical_bgra8_sha256: String,
    pub classifier_request_sha256: String,
    pub classifier_response_sha256: String,
    pub lifecycle: MtgoVisibleCompetitiveLifecycleSnapshotV1,
}

/// Structurally binds a declared classifier exchange to one exact profile and
/// navigation source. It remains caller supplied and cannot prove that the
/// pinned classifier executable produced the response.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoCompetitiveNavigationPredictionV1;
/// fn cannot_enter(value: &CheckedUntrustedMtgoCompetitiveNavigationPredictionV1) {
///     let _ = value.join_control();
///     let _ = value.input_command();
/// }
/// ```
pub struct CheckedUntrustedMtgoCompetitiveNavigationPredictionV1 {
    record: MtgoCompetitiveNavigationPredictionV1,
    lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    prediction_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveNavigationPredictionV1 {
    pub fn profile_commitment_sha256(&self) -> &str {
        &self.record.profile_commitment_sha256
    }

    pub fn source_manifest_sha256(&self) -> &str {
        &self.record.source_manifest_sha256
    }

    pub fn source_canonical_bgra8_sha256(&self) -> &str {
        &self.record.source_canonical_bgra8_sha256
    }

    pub fn prediction_commitment_sha256(&self) -> &str {
        &self.prediction_commitment_sha256
    }

    pub fn permits_event_entry(&self) -> bool {
        false
    }

    pub fn permits_spending(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

/// One annotated navigation capture and the optional classifier prediction for
/// the same frame. Inputs remain untrusted until a separately reviewed corpus
/// result is pinned in production source.
pub struct MtgoCompetitiveNavigationEvaluationCaseV1<'a> {
    pub case_id: String,
    pub source: &'a CheckedUntrustedMtgoCompetitiveNavigationSourceV1,
    pub expected: MtgoVisibleCompetitiveLifecycleSnapshotV1,
    pub prediction: Option<&'a CheckedUntrustedMtgoCompetitiveNavigationPredictionV1>,
}

/// Recomputed corpus measurements without runtime authority.
///
/// This value intentionally has no `Debug`, `Clone`, or serde implementation.
/// It exposes counts and commitments only.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoCompetitiveNavigationEvaluationV1;
/// fn cannot_enter(value: &CheckedUntrustedMtgoCompetitiveNavigationEvaluationV1) {
///     let _ = value.join_control();
///     let _ = value.input_command();
/// }
/// ```
pub struct CheckedUntrustedMtgoCompetitiveNavigationEvaluationV1 {
    profile_commitment_sha256: String,
    evaluation_commitment_sha256: String,
    unique_case_count: u32,
    prediction_count: u32,
    exact_prediction_count: u32,
    prediction_coverage_bps: u16,
    minimum_prediction_coverage_bps_per_slice: u16,
    minimum_observed_cases_per_slice: u32,
    missing_slices: Vec<MtgoCompetitiveNavigationSliceV1>,
    passes_declared_gate: bool,
}

impl CheckedUntrustedMtgoCompetitiveNavigationEvaluationV1 {
    pub fn profile_commitment_sha256(&self) -> &str {
        &self.profile_commitment_sha256
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

    pub fn prediction_coverage_bps(&self) -> u16 {
        self.prediction_coverage_bps
    }

    pub fn minimum_prediction_coverage_bps_per_slice(&self) -> u16 {
        self.minimum_prediction_coverage_bps_per_slice
    }

    pub fn minimum_observed_cases_per_slice(&self) -> u32 {
        self.minimum_observed_cases_per_slice
    }

    pub fn missing_slices(&self) -> &[MtgoCompetitiveNavigationSliceV1] {
        &self.missing_slices
    }

    pub fn passes_declared_gate(&self) -> bool {
        self.passes_declared_gate
    }

    pub fn permits_event_entry(&self) -> bool {
        false
    }

    pub fn permits_spending(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoCompetitiveNavigationProfileScopeV1 {
    LeagueAndChallengeBrowserAndEntryReviewClassification,
}

/// A separately ratified profile identity for classifying League and Challenge
/// browser and entry-review frames. It remains insufficient for event entry,
/// resource spending, coordinates, or input.
///
/// Production currently has no ratified evaluation commitment, so callers
/// cannot construct this value.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::AdmittedMtgoCompetitiveNavigationProfileV1;
/// fn cannot_enter(value: &AdmittedMtgoCompetitiveNavigationProfileV1) {
///     let _ = value.join_control();
///     let _ = value.spend();
///     let _ = value.input_command();
/// }
/// ```
pub struct AdmittedMtgoCompetitiveNavigationProfileV1 {
    profile: CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    evaluation_commitment_sha256: String,
    admission_commitment_sha256: String,
}

impl AdmittedMtgoCompetitiveNavigationProfileV1 {
    pub fn scope(&self) -> MtgoCompetitiveNavigationProfileScopeV1 {
        MtgoCompetitiveNavigationProfileScopeV1::LeagueAndChallengeBrowserAndEntryReviewClassification
    }

    pub fn profile_commitment_sha256(&self) -> &str {
        self.profile.profile_commitment_sha256()
    }

    pub fn evaluation_commitment_sha256(&self) -> &str {
        &self.evaluation_commitment_sha256
    }

    pub fn admission_commitment_sha256(&self) -> &str {
        &self.admission_commitment_sha256
    }

    pub fn checked_runtime_profile(
        &self,
    ) -> &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1 {
        &self.profile
    }

    pub fn safe_for_live_classification(&self) -> bool {
        false
    }

    pub fn permits_event_entry(&self) -> bool {
        false
    }

    pub fn permits_spending(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

pub fn check_untrusted_competitive_navigation_runtime_profile_v1(
    payload: MtgoCompetitiveNavigationRuntimeProfileV1,
) -> Result<CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1, MtgoContractErrorV1> {
    if payload.schema_version != MTGO_COMPETITIVE_NAVIGATION_EVALUATION_SCHEMA_V1 {
        return Err(error_v1(
            "competitive_navigation_profile_schema",
            payload.schema_version.to_string(),
        ));
    }
    validate_identifier_v1(&payload.profile_id, "competitive_navigation_profile_id")?;
    for (field, value) in [
        ("executable", payload.executable_sha256.as_str()),
        ("signer_subject", payload.signer_subject_sha256.as_str()),
        ("window_title", payload.window_title_sha256.as_str()),
        (
            "approved_account_alias",
            payload.approved_account_alias_sha256.as_str(),
        ),
        (
            "account_identity_region",
            payload.account_identity_region_sha256.as_str(),
        ),
        ("output_identity", payload.output_identity_sha256.as_str()),
        (
            "classifier_binary",
            payload.classifier_binary_sha256.as_str(),
        ),
        (
            "classifier_assets_manifest",
            payload.classifier_assets_manifest_sha256.as_str(),
        ),
    ] {
        validate_sha256_v1(field, value)?;
    }
    validate_lower_hex_v1("signer_thumbprint", &payload.signer_thumbprint, 40)?;
    if !(96..=480).contains(&payload.dpi)
        || payload.client_size_px.width == 0
        || payload.client_size_px.height == 0
        || payload.client_size_px.width > 16_384
        || payload.client_size_px.height > 16_384
    {
        return Err(error_v1(
            "competitive_navigation_profile_geometry",
            "DPI and client size must be bounded and nonzero",
        ));
    }
    let account_right = payload
        .account_identity_rect_client_px
        .x
        .checked_add(payload.account_identity_rect_client_px.width)
        .ok_or_else(|| {
            error_v1(
                "competitive_navigation_account_identity_geometry",
                "account identity region right edge overflow",
            )
        })?;
    let account_bottom = payload
        .account_identity_rect_client_px
        .y
        .checked_add(payload.account_identity_rect_client_px.height)
        .ok_or_else(|| {
            error_v1(
                "competitive_navigation_account_identity_geometry",
                "account identity region bottom edge overflow",
            )
        })?;
    if payload.account_identity_rect_client_px.width < 8
        || payload.account_identity_rect_client_px.height < 8
        || account_right > payload.client_size_px.width
        || account_bottom > payload.client_size_px.height
    {
        return Err(error_v1(
            "competitive_navigation_account_identity_geometry",
            "reviewed visible account identity region must be nontrivial and inside the client",
        ));
    }
    if payload.canonical_pixel_format != CANONICAL_PIXEL_FORMAT_V1 {
        return Err(error_v1(
            "competitive_navigation_profile_pixel_format",
            payload.canonical_pixel_format.clone(),
        ));
    }
    validate_required_slices_v1(&payload.supported_slices)?;

    let bytes = serde_json::to_vec(&payload).map_err(|error| {
        error_v1(
            "competitive_navigation_profile_serialization",
            error.to_string(),
        )
    })?;
    let commitment = commitment_v1(NAVIGATION_PROFILE_DOMAIN_V1, &[&bytes]);
    Ok(CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1 {
        payload,
        profile_commitment_sha256: commitment,
    })
}

/// Checks a complete offline DXGI artifact against one exact navigation
/// profile and recomputes the reviewed account-identity region from the
/// supplied canonical pixels. Success remains non-authorizing.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoCompetitiveNavigationSourceV1;
/// fn cannot_extract_pixels(value: &CheckedUntrustedMtgoCompetitiveNavigationSourceV1) {
///     let _ = value.canonical_pixels_for_ocr_v1();
/// }
/// ```
pub fn check_untrusted_competitive_navigation_source_v1(
    profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    manifest_bytes: &[u8],
    canonical_bgra8: &[u8],
    preview_png_bytes: &[u8],
) -> Result<CheckedUntrustedMtgoCompetitiveNavigationSourceV1, MtgoContractErrorV1> {
    let artifact = check_untrusted_dxgi_capture_artifact_v1(
        manifest_bytes,
        canonical_bgra8,
        preview_png_bytes,
    )?;
    bind_checked_competitive_navigation_source_v1(profile, artifact, canonical_bgra8)
}

fn bind_checked_competitive_navigation_source_v1(
    profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    artifact: CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoCompetitiveNavigationSourceV1, MtgoContractErrorV1> {
    validate_source_artifact_profile_v1(profile, &artifact)?;
    let account_identity_region_sha256 = visible_frame_region_content_sha256_v1(
        canonical_bgra8,
        profile.client_size_px(),
        profile.account_identity_rect_client_px(),
    )?;
    if account_identity_region_sha256 != profile.account_identity_region_sha256() {
        return Err(error_v1(
            "competitive_navigation_source_account_identity",
            "source pixels do not match the reviewed approved-account identity region",
        ));
    }
    let source_profile_binding_sha256 = commitment_v1(
        NAVIGATION_SOURCE_PROFILE_BINDING_DOMAIN_V1,
        &[
            profile.profile_commitment_sha256().as_bytes(),
            artifact.manifest_sha256().as_bytes(),
            artifact.canonical_bgra8_sha256().as_bytes(),
            profile.approved_account_alias_sha256().as_bytes(),
            account_identity_region_sha256.as_bytes(),
            b"checked_untrusted_offline_source_no_pixels_no_classification_no_entry_no_spending_no_input",
        ],
    );
    Ok(CheckedUntrustedMtgoCompetitiveNavigationSourceV1 {
        artifact,
        account_identity_region_sha256,
        source_profile_binding_sha256,
    })
}

pub fn check_untrusted_competitive_navigation_prediction_v1(
    profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    source: &CheckedUntrustedMtgoCompetitiveNavigationSourceV1,
    record: MtgoCompetitiveNavigationPredictionV1,
) -> Result<CheckedUntrustedMtgoCompetitiveNavigationPredictionV1, MtgoContractErrorV1> {
    if record.schema_version != MTGO_COMPETITIVE_NAVIGATION_EVALUATION_SCHEMA_V1 {
        return Err(error_v1(
            "competitive_navigation_prediction_schema",
            record.schema_version.to_string(),
        ));
    }
    validate_source_profile_v1(profile, source)?;
    if record.profile_commitment_sha256 != profile.profile_commitment_sha256()
        || record.source_manifest_sha256 != source.manifest_sha256()
        || record.source_canonical_bgra8_sha256 != source.canonical_bgra8_sha256()
    {
        return Err(error_v1(
            "competitive_navigation_prediction_source",
            "prediction must bind the exact runtime profile and navigation source",
        ));
    }
    validate_sha256_v1("classifier_request", &record.classifier_request_sha256)?;
    validate_sha256_v1("classifier_response", &record.classifier_response_sha256)?;
    let lifecycle = validate_visible_competitive_lifecycle_snapshot_v1(record.lifecycle.clone())?;
    validate_snapshot_source_v1(&lifecycle, source)?;
    let bytes = serde_json::to_vec(&record).map_err(|error| {
        error_v1(
            "competitive_navigation_prediction_serialization",
            error.to_string(),
        )
    })?;
    Ok(CheckedUntrustedMtgoCompetitiveNavigationPredictionV1 {
        record,
        lifecycle,
        prediction_commitment_sha256: commitment_v1(NAVIGATION_PREDICTION_DOMAIN_V1, &[&bytes]),
    })
}

pub fn evaluate_untrusted_competitive_navigation_profile_v1(
    profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    spec: MtgoCompetitiveNavigationEvaluationSpecV1,
    cases: Vec<MtgoCompetitiveNavigationEvaluationCaseV1<'_>>,
) -> Result<CheckedUntrustedMtgoCompetitiveNavigationEvaluationV1, MtgoContractErrorV1> {
    validate_evaluation_spec_v1(&spec)?;
    if spec.profile_commitment_sha256 != profile.profile_commitment_sha256()
        || spec.required_slices != profile.supported_slices()
    {
        return Err(error_v1(
            "competitive_navigation_evaluation_profile",
            "evaluation must bind the exact runtime profile and required slices",
        ));
    }
    if cases.is_empty() || cases.len() > 100_000 {
        return Err(error_v1(
            "competitive_navigation_case_count",
            cases.len().to_string(),
        ));
    }

    let spec_bytes = serde_json::to_vec(&spec).map_err(|error| {
        error_v1(
            "competitive_navigation_spec_serialization",
            error.to_string(),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(NAVIGATION_EVALUATION_DOMAIN_V1);
    hash_part_v1(&mut hasher, &spec_bytes);

    let mut previous_case_id: Option<&str> = None;
    let mut source_manifests = HashSet::new();
    let mut source_frames = HashSet::new();
    let mut slice_counts = HashMap::new();
    let mut slice_prediction_counts = HashMap::new();
    let mut prediction_count = 0_u32;
    let mut exact_prediction_count = 0_u32;

    for case in &cases {
        validate_identifier_v1(&case.case_id, "competitive_navigation_case_id")?;
        if previous_case_id.is_some_and(|previous| previous >= case.case_id.as_str()) {
            return Err(error_v1(
                "competitive_navigation_case_order",
                "case IDs must be unique and strictly increasing",
            ));
        }
        previous_case_id = Some(&case.case_id);
        if !source_manifests.insert(case.source.manifest_sha256()) {
            return Err(error_v1(
                "competitive_navigation_duplicate_source",
                case.source.manifest_sha256().to_owned(),
            ));
        }
        if !source_frames.insert(case.source.canonical_bgra8_sha256()) {
            return Err(error_v1(
                "competitive_navigation_duplicate_frame",
                case.source.canonical_bgra8_sha256().to_owned(),
            ));
        }
        validate_source_profile_v1(profile, case.source)?;
        let expected = validate_visible_competitive_lifecycle_snapshot_v1(case.expected.clone())?;
        validate_snapshot_source_v1(&expected, case.source)?;
        let slice = navigation_slice_v1(expected.event_kind(), expected.phase())?;
        *slice_counts.entry(slice).or_insert(0_u32) += 1;

        let (prediction_commitment, exact) = if let Some(prediction) = case.prediction {
            if prediction.profile_commitment_sha256() != profile.profile_commitment_sha256()
                || prediction.source_manifest_sha256() != case.source.manifest_sha256()
                || prediction.source_canonical_bgra8_sha256()
                    != case.source.canonical_bgra8_sha256()
                || prediction.lifecycle.frame_id_v1() != expected.frame_id_v1()
                || prediction.lifecycle.frame_sequence() != expected.frame_sequence()
            {
                return Err(error_v1(
                    "competitive_navigation_prediction_source",
                    "prediction must refer to the exact annotated source frame",
                ));
            }
            prediction_count += 1;
            *slice_prediction_counts.entry(slice).or_insert(0_u32) += 1;
            let exact = prediction.lifecycle.snapshot_commitment_sha256()
                == expected.snapshot_commitment_sha256();
            exact_prediction_count += u32::from(exact);
            (prediction.prediction_commitment_sha256(), exact)
        } else {
            ("abstained", false)
        };

        for part in [
            case.case_id.as_bytes(),
            case.source.manifest_sha256().as_bytes(),
            case.source.canonical_bgra8_sha256().as_bytes(),
            expected.snapshot_commitment_sha256().as_bytes(),
            prediction_commitment.as_bytes(),
            &[u8::from(exact)],
        ] {
            hash_part_v1(&mut hasher, part);
        }
    }

    let unique_case_count = u32::try_from(cases.len()).map_err(|_| {
        error_v1(
            "competitive_navigation_case_count",
            "case count does not fit u32",
        )
    })?;
    let prediction_coverage_bps = ratio_bps_v1(prediction_count, unique_case_count);
    let missing_slices = REQUIRED_NAVIGATION_SLICES_V1
        .iter()
        .copied()
        .filter(|slice| !slice_counts.contains_key(slice))
        .collect::<Vec<_>>();
    let minimum_observed_cases_per_slice = REQUIRED_NAVIGATION_SLICES_V1
        .iter()
        .map(|slice| slice_counts.get(slice).copied().unwrap_or(0))
        .min()
        .unwrap_or(0);
    let minimum_prediction_coverage_bps_per_slice = REQUIRED_NAVIGATION_SLICES_V1
        .iter()
        .map(|slice| {
            ratio_bps_v1(
                slice_prediction_counts.get(slice).copied().unwrap_or(0),
                slice_counts.get(slice).copied().unwrap_or(0),
            )
        })
        .min()
        .unwrap_or(0);
    let passes_declared_gate = missing_slices.is_empty()
        && minimum_observed_cases_per_slice >= spec.minimum_unique_cases_per_slice
        && prediction_coverage_bps >= spec.minimum_prediction_coverage_bps
        && minimum_prediction_coverage_bps_per_slice >= spec.minimum_prediction_coverage_bps
        && prediction_count != 0
        && exact_prediction_count == prediction_count;

    Ok(CheckedUntrustedMtgoCompetitiveNavigationEvaluationV1 {
        profile_commitment_sha256: spec.profile_commitment_sha256,
        evaluation_commitment_sha256: format!("{:x}", hasher.finalize()),
        unique_case_count,
        prediction_count,
        exact_prediction_count,
        prediction_coverage_bps,
        minimum_prediction_coverage_bps_per_slice,
        minimum_observed_cases_per_slice,
        missing_slices,
        passes_declared_gate,
    })
}

/// Admits only the exact four-slice evaluation pinned in the private production
/// trust root. The root remains empty until a real reviewed navigation corpus
/// and classifier run exist.
pub fn admit_ratified_competitive_navigation_profile_v1(
    profile: CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    evaluation: CheckedUntrustedMtgoCompetitiveNavigationEvaluationV1,
) -> Result<AdmittedMtgoCompetitiveNavigationProfileV1, MtgoContractErrorV1> {
    admit_competitive_navigation_profile_against_ratification_v1(
        profile,
        evaluation,
        RATIFIED_COMPETITIVE_NAVIGATION_EVALUATION_COMMITMENT_V1,
    )
}

fn admit_competitive_navigation_profile_against_ratification_v1(
    profile: CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    evaluation: CheckedUntrustedMtgoCompetitiveNavigationEvaluationV1,
    ratified_evaluation_commitment: Option<&str>,
) -> Result<AdmittedMtgoCompetitiveNavigationProfileV1, MtgoContractErrorV1> {
    if evaluation.profile_commitment_sha256 != profile.profile_commitment_sha256() {
        return Err(error_v1(
            "competitive_navigation_evaluation_profile",
            "evaluation and runtime profile commitments differ",
        ));
    }
    if !evaluation.passes_declared_gate {
        return Err(error_v1(
            "competitive_navigation_declared_gate",
            evaluation.evaluation_commitment_sha256,
        ));
    }
    let ratified = ratified_evaluation_commitment.ok_or_else(|| {
        error_v1(
            "competitive_navigation_profile_not_ratified",
            "production contains no ratified League and Challenge navigation evaluation",
        )
    })?;
    validate_sha256_v1("competitive_navigation_ratification", ratified)?;
    if evaluation.evaluation_commitment_sha256 != ratified {
        return Err(error_v1(
            "competitive_navigation_profile_not_ratified",
            "evaluation does not match the production ratification",
        ));
    }
    let admission_commitment_sha256 = commitment_v1(
        NAVIGATION_PROFILE_ADMISSION_DOMAIN_V1,
        &[
            evaluation.profile_commitment_sha256.as_bytes(),
            evaluation.evaluation_commitment_sha256.as_bytes(),
            b"league_and_challenge_browser_and_entry_review_classification_only",
            b"no_event_entry_no_spending_no_coordinates_no_input",
        ],
    );
    Ok(AdmittedMtgoCompetitiveNavigationProfileV1 {
        profile,
        evaluation_commitment_sha256: evaluation.evaluation_commitment_sha256,
        admission_commitment_sha256,
    })
}

fn validate_evaluation_spec_v1(
    spec: &MtgoCompetitiveNavigationEvaluationSpecV1,
) -> Result<(), MtgoContractErrorV1> {
    if spec.schema_version != MTGO_COMPETITIVE_NAVIGATION_EVALUATION_SCHEMA_V1 {
        return Err(error_v1(
            "competitive_navigation_evaluation_schema",
            spec.schema_version.to_string(),
        ));
    }
    validate_identifier_v1(&spec.evaluation_id, "competitive_navigation_evaluation_id")?;
    for (field, value) in [
        ("profile", spec.profile_commitment_sha256.as_str()),
        ("corpus_manifest", spec.corpus_manifest_sha256.as_str()),
        (
            "annotation_protocol",
            spec.annotation_protocol_sha256.as_str(),
        ),
        ("evaluator_binary", spec.evaluator_binary_sha256.as_str()),
    ] {
        validate_sha256_v1(field, value)?;
    }
    if spec.minimum_unique_cases_per_slice == 0 || spec.minimum_unique_cases_per_slice > 25_000 {
        return Err(error_v1(
            "competitive_navigation_minimum_cases",
            spec.minimum_unique_cases_per_slice.to_string(),
        ));
    }
    if !(9_500..=10_000).contains(&spec.minimum_prediction_coverage_bps) {
        return Err(error_v1(
            "competitive_navigation_coverage_threshold",
            spec.minimum_prediction_coverage_bps.to_string(),
        ));
    }
    validate_required_slices_v1(&spec.required_slices)
}

fn validate_required_slices_v1(
    slices: &[MtgoCompetitiveNavigationSliceV1],
) -> Result<(), MtgoContractErrorV1> {
    if slices != REQUIRED_NAVIGATION_SLICES_V1 {
        return Err(error_v1(
            "competitive_navigation_required_slices",
            "profile and evaluation must cover League and Challenge browser and entry review in canonical order",
        ));
    }
    Ok(())
}

fn validate_source_profile_v1(
    profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    source: &CheckedUntrustedMtgoCompetitiveNavigationSourceV1,
) -> Result<(), MtgoContractErrorV1> {
    validate_source_artifact_profile_v1(profile, &source.artifact)?;
    let expected_binding = commitment_v1(
        NAVIGATION_SOURCE_PROFILE_BINDING_DOMAIN_V1,
        &[
            profile.profile_commitment_sha256().as_bytes(),
            source.artifact.manifest_sha256().as_bytes(),
            source.artifact.canonical_bgra8_sha256().as_bytes(),
            profile.approved_account_alias_sha256().as_bytes(),
            source.account_identity_region_sha256.as_bytes(),
            b"checked_untrusted_offline_source_no_pixels_no_classification_no_entry_no_spending_no_input",
        ],
    );
    if source.account_identity_region_sha256 != profile.account_identity_region_sha256()
        || source.source_profile_binding_sha256 != expected_binding
    {
        return Err(error_v1(
            "competitive_navigation_source_profile",
            "source must retain the exact profile and approved-account pixel binding",
        ));
    }
    Ok(())
}

fn validate_source_artifact_profile_v1(
    profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    source: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
) -> Result<(), MtgoContractErrorV1> {
    if source.capture_role() != MtgoDxgiCaptureRoleV2::Navigation
        || source.game_format().is_some()
        || source.executable_sha256() != profile.executable_sha256()
        || source.signer_thumbprint() != profile.signer_thumbprint()
        || source.signer_subject_sha256() != profile.signer_subject_sha256()
        || source.window_title_sha256() != profile.window_title_sha256()
        || source.dpi() != profile.dpi()
        || source.client_size_px() != profile.client_size_px()
        || source.output_identity_sha256() != profile.output_identity_sha256()
    {
        return Err(error_v1(
            "competitive_navigation_source_profile",
            "source must be an exact profile-matching main-client navigation artifact",
        ));
    }
    Ok(())
}

fn validate_snapshot_source_v1(
    snapshot: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    source: &CheckedUntrustedMtgoCompetitiveNavigationSourceV1,
) -> Result<(), MtgoContractErrorV1> {
    let bounds = snapshot.client_bounds_v1();
    if snapshot.frame_sha256_v1() != source.canonical_bgra8_sha256()
        || bounds.x != 0
        || bounds.y != 0
        || bounds.width != source.artifact.client_size_px().width
        || bounds.height != source.artifact.client_size_px().height
    {
        return Err(error_v1(
            "competitive_navigation_snapshot_source",
            "lifecycle snapshot must bind the exact navigation artifact pixels and geometry",
        ));
    }
    navigation_slice_v1(snapshot.event_kind(), snapshot.phase())?;
    Ok(())
}

fn navigation_slice_v1(
    event_kind: MtgoCompetitiveEventKindV1,
    phase: MtgoCompetitiveLifecyclePhaseV1,
) -> Result<MtgoCompetitiveNavigationSliceV1, MtgoContractErrorV1> {
    match (event_kind, phase) {
        (MtgoCompetitiveEventKindV1::League, MtgoCompetitiveLifecyclePhaseV1::EventBrowser) => {
            Ok(MtgoCompetitiveNavigationSliceV1::LeagueEventBrowser)
        }
        (MtgoCompetitiveEventKindV1::League, MtgoCompetitiveLifecyclePhaseV1::EntryReview) => {
            Ok(MtgoCompetitiveNavigationSliceV1::LeagueEntryReview)
        }
        (MtgoCompetitiveEventKindV1::Challenge, MtgoCompetitiveLifecyclePhaseV1::EventBrowser) => {
            Ok(MtgoCompetitiveNavigationSliceV1::ChallengeEventBrowser)
        }
        (MtgoCompetitiveEventKindV1::Challenge, MtgoCompetitiveLifecyclePhaseV1::EntryReview) => {
            Ok(MtgoCompetitiveNavigationSliceV1::ChallengeEntryReview)
        }
        _ => Err(error_v1(
            "competitive_navigation_slice",
            "navigation evaluation accepts only League and Challenge browser and entry-review phases",
        )),
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

fn validate_sha256_v1(field: &'static str, value: &str) -> Result<(), MtgoContractErrorV1> {
    validate_lower_hex_v1(field, value, 64)
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
            "competitive_navigation_digest",
            format!("{field} must be {length} lowercase hexadecimal characters"),
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
        checked_untrusted_dxgi_navigation_artifact_for_test_v1, MtgoCompetitiveEntryResourceV1,
        MtgoCompetitiveEntryTermsV1, MtgoLifecycleVisibleFactKindV1, MtgoLifecycleVisibleFactV1,
    };

    fn profile_payload_v1() -> MtgoCompetitiveNavigationRuntimeProfileV1 {
        MtgoCompetitiveNavigationRuntimeProfileV1 {
            schema_version: MTGO_COMPETITIVE_NAVIGATION_EVALUATION_SCHEMA_V1,
            profile_id: "competitive-navigation-test-v1".to_owned(),
            executable_sha256: "a".repeat(64),
            signer_thumbprint: "b".repeat(40),
            signer_subject_sha256: "c".repeat(64),
            window_title_sha256: "9".repeat(64),
            approved_account_alias_sha256: "2".repeat(64),
            account_identity_rect_client_px: MtgoRectPxV1 {
                x: 24,
                y: 24,
                width: 160,
                height: 32,
            },
            account_identity_region_sha256: "3".repeat(64),
            dpi: 120,
            client_size_px: MtgoSizePxV1 {
                width: 1_550,
                height: 925,
            },
            output_identity_sha256: "4".repeat(64),
            canonical_pixel_format: CANONICAL_PIXEL_FORMAT_V1.to_owned(),
            classifier_binary_sha256: "5".repeat(64),
            classifier_assets_manifest_sha256: "6".repeat(64),
            supported_slices: REQUIRED_NAVIGATION_SLICES_V1.to_vec(),
        }
    }

    fn profile_v1() -> CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1 {
        check_untrusted_competitive_navigation_runtime_profile_v1(profile_payload_v1()).unwrap()
    }

    fn source_from_artifact_v1(
        profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
        artifact: CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    ) -> CheckedUntrustedMtgoCompetitiveNavigationSourceV1 {
        let account_identity_region_sha256 = profile.account_identity_region_sha256().to_owned();
        let source_profile_binding_sha256 = commitment_v1(
            NAVIGATION_SOURCE_PROFILE_BINDING_DOMAIN_V1,
            &[
                profile.profile_commitment_sha256().as_bytes(),
                artifact.manifest_sha256().as_bytes(),
                artifact.canonical_bgra8_sha256().as_bytes(),
                profile.approved_account_alias_sha256().as_bytes(),
                account_identity_region_sha256.as_bytes(),
                b"checked_untrusted_offline_source_no_pixels_no_classification_no_entry_no_spending_no_input",
            ],
        );
        CheckedUntrustedMtgoCompetitiveNavigationSourceV1 {
            artifact,
            account_identity_region_sha256,
            source_profile_binding_sha256,
        }
    }

    fn source_v1(
        profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
        discriminator: u8,
    ) -> CheckedUntrustedMtgoCompetitiveNavigationSourceV1 {
        source_from_artifact_v1(
            profile,
            checked_untrusted_dxgi_navigation_artifact_for_test_v1(discriminator),
        )
    }

    fn spec_v1(
        profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
    ) -> MtgoCompetitiveNavigationEvaluationSpecV1 {
        MtgoCompetitiveNavigationEvaluationSpecV1 {
            schema_version: MTGO_COMPETITIVE_NAVIGATION_EVALUATION_SCHEMA_V1,
            evaluation_id: "competitive-navigation-evaluation-test-v1".to_owned(),
            profile_commitment_sha256: profile.profile_commitment_sha256().to_owned(),
            corpus_manifest_sha256: "7".repeat(64),
            annotation_protocol_sha256: "8".repeat(64),
            evaluator_binary_sha256: "9".repeat(64),
            minimum_unique_cases_per_slice: 1,
            minimum_prediction_coverage_bps: 10_000,
            required_slices: REQUIRED_NAVIGATION_SLICES_V1.to_vec(),
        }
    }

    fn snapshot_v1(
        event_kind: MtgoCompetitiveEventKindV1,
        phase: MtgoCompetitiveLifecyclePhaseV1,
        frame_id: u64,
        frame_sha256: String,
    ) -> MtgoVisibleCompetitiveLifecycleSnapshotV1 {
        let entry_review = phase == MtgoCompetitiveLifecyclePhaseV1::EntryReview;
        let mut facts = vec![MtgoLifecycleVisibleFactV1 {
            kind: if entry_review {
                MtgoLifecycleVisibleFactKindV1::EntryReviewVisible
            } else {
                MtgoLifecycleVisibleFactKindV1::EventBrowserVisible
            },
            rect_client_px: MtgoRectPxV1 {
                x: 100,
                y: 100,
                width: 600,
                height: 500,
            },
            content_sha256: "d".repeat(64),
            confidence_bps: 10_000,
        }];
        if entry_review {
            facts.push(MtgoLifecycleVisibleFactV1 {
                kind: MtgoLifecycleVisibleFactKindV1::EntryTermsVisible,
                rect_client_px: MtgoRectPxV1 {
                    x: 300,
                    y: 450,
                    width: 240,
                    height: 80,
                },
                content_sha256: "e".repeat(64),
                confidence_bps: 10_000,
            });
        }
        MtgoVisibleCompetitiveLifecycleSnapshotV1 {
            schema_version: 1,
            snapshot_id: format!("navigation-case-{frame_id}"),
            event_kind,
            phase,
            frame_id,
            frame_sequence: frame_id,
            frame_sha256,
            client_bounds: MtgoRectPxV1 {
                x: 0,
                y: 0,
                width: 1_550,
                height: 925,
            },
            event_identity_sha256: entry_review.then(|| "f".repeat(64)),
            match_identity_sha256: None,
            game_number: None,
            entry_terms: entry_review.then(|| MtgoCompetitiveEntryTermsV1 {
                terms_sha256: "1".repeat(64),
                resource: MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                amount: 100,
            }),
            visible_state_complete: true,
            facts,
        }
    }

    fn prediction_v1(
        profile: &CheckedUntrustedMtgoCompetitiveNavigationRuntimeProfileV1,
        source: &CheckedUntrustedMtgoCompetitiveNavigationSourceV1,
        lifecycle: MtgoVisibleCompetitiveLifecycleSnapshotV1,
        discriminator: u8,
    ) -> CheckedUntrustedMtgoCompetitiveNavigationPredictionV1 {
        check_untrusted_competitive_navigation_prediction_v1(
            profile,
            source,
            MtgoCompetitiveNavigationPredictionV1 {
                schema_version: MTGO_COMPETITIVE_NAVIGATION_EVALUATION_SCHEMA_V1,
                profile_commitment_sha256: profile.profile_commitment_sha256().to_owned(),
                source_manifest_sha256: source.manifest_sha256().to_owned(),
                source_canonical_bgra8_sha256: source.canonical_bgra8_sha256().to_owned(),
                classifier_request_sha256: format!("{:064x}", discriminator),
                classifier_response_sha256: format!("{:064x}", discriminator.saturating_add(16)),
                lifecycle,
            },
        )
        .unwrap()
    }

    #[test]
    fn four_slice_exact_evaluation_passes_but_production_admission_is_empty() {
        let profile = profile_v1();
        let sources = (1_u8..=4)
            .map(|discriminator| source_v1(&profile, discriminator))
            .collect::<Vec<_>>();
        let kinds = [
            (
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
            ),
            (
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveLifecyclePhaseV1::EntryReview,
            ),
            (
                MtgoCompetitiveEventKindV1::Challenge,
                MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
            ),
            (
                MtgoCompetitiveEventKindV1::Challenge,
                MtgoCompetitiveLifecyclePhaseV1::EntryReview,
            ),
        ];
        let expected = sources
            .iter()
            .zip(kinds)
            .enumerate()
            .map(|(index, (source, (event, phase)))| {
                snapshot_v1(
                    event,
                    phase,
                    u64::try_from(index + 1).unwrap(),
                    source.canonical_bgra8_sha256().to_owned(),
                )
            })
            .collect::<Vec<_>>();
        let predictions = sources
            .iter()
            .zip(expected.iter().cloned())
            .enumerate()
            .map(|(index, (source, expected))| {
                prediction_v1(&profile, source, expected, u8::try_from(index + 1).unwrap())
            })
            .collect::<Vec<_>>();
        let cases = sources
            .iter()
            .zip(expected.iter())
            .zip(predictions.iter())
            .enumerate()
            .map(|(index, ((source, expected), prediction))| {
                MtgoCompetitiveNavigationEvaluationCaseV1 {
                    case_id: format!("case-{}", index + 1),
                    source,
                    expected: expected.clone(),
                    prediction: Some(prediction),
                }
            })
            .collect::<Vec<_>>();
        let evaluation = evaluate_untrusted_competitive_navigation_profile_v1(
            &profile,
            spec_v1(&profile),
            cases,
        )
        .unwrap();
        assert!(evaluation.passes_declared_gate());
        assert_eq!(evaluation.unique_case_count(), 4);
        assert_eq!(evaluation.exact_prediction_count(), 4);
        assert_eq!(evaluation.minimum_observed_cases_per_slice(), 1);
        assert_eq!(
            evaluation.minimum_prediction_coverage_bps_per_slice(),
            10_000
        );
        assert!(evaluation.missing_slices().is_empty());
        assert!(!evaluation.permits_event_entry());
        assert!(!evaluation.permits_spending());
        assert!(!evaluation.safe_for_input());

        let error = admit_ratified_competitive_navigation_profile_v1(profile, evaluation)
            .err()
            .unwrap();
        assert_eq!(error.code(), "competitive_navigation_profile_not_ratified");
    }

    #[test]
    fn private_exact_ratification_grants_profile_identity_only() {
        let profile = profile_v1();
        let sources = (1_u8..=4)
            .map(|discriminator| source_v1(&profile, discriminator))
            .collect::<Vec<_>>();
        let kinds = [
            (
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
            ),
            (
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveLifecyclePhaseV1::EntryReview,
            ),
            (
                MtgoCompetitiveEventKindV1::Challenge,
                MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
            ),
            (
                MtgoCompetitiveEventKindV1::Challenge,
                MtgoCompetitiveLifecyclePhaseV1::EntryReview,
            ),
        ];
        let expected = sources
            .iter()
            .zip(kinds)
            .enumerate()
            .map(|(index, (source, (event, phase)))| {
                snapshot_v1(
                    event,
                    phase,
                    u64::try_from(index + 1).unwrap(),
                    source.canonical_bgra8_sha256().to_owned(),
                )
            })
            .collect::<Vec<_>>();
        let predictions = sources
            .iter()
            .zip(expected.iter().cloned())
            .enumerate()
            .map(|(index, (source, expected))| {
                prediction_v1(&profile, source, expected, u8::try_from(index + 1).unwrap())
            })
            .collect::<Vec<_>>();
        let cases = sources
            .iter()
            .zip(expected.iter())
            .zip(predictions.iter())
            .enumerate()
            .map(|(index, ((source, expected), prediction))| {
                MtgoCompetitiveNavigationEvaluationCaseV1 {
                    case_id: format!("case-{}", index + 1),
                    source,
                    expected: expected.clone(),
                    prediction: Some(prediction),
                }
            })
            .collect::<Vec<_>>();
        let evaluation = evaluate_untrusted_competitive_navigation_profile_v1(
            &profile,
            spec_v1(&profile),
            cases,
        )
        .unwrap();
        let ratification = evaluation.evaluation_commitment_sha256().to_owned();
        let admitted = admit_competitive_navigation_profile_against_ratification_v1(
            profile,
            evaluation,
            Some(&ratification),
        )
        .unwrap();
        assert_eq!(
            admitted.scope(),
            MtgoCompetitiveNavigationProfileScopeV1::LeagueAndChallengeBrowserAndEntryReviewClassification
        );
        assert_eq!(admitted.evaluation_commitment_sha256(), ratification);
        assert_eq!(admitted.admission_commitment_sha256().len(), 64);
        assert!(!admitted.safe_for_live_classification());
        assert!(!admitted.permits_event_entry());
        assert!(!admitted.permits_spending());
        assert!(!admitted.safe_for_input());
    }

    #[test]
    fn missing_slice_and_abstention_fail_the_declared_gate() {
        let profile = profile_v1();
        let source = source_v1(&profile, 1);
        let expected = snapshot_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
            1,
            source.canonical_bgra8_sha256().to_owned(),
        );
        let evaluation = evaluate_untrusted_competitive_navigation_profile_v1(
            &profile,
            spec_v1(&profile),
            vec![MtgoCompetitiveNavigationEvaluationCaseV1 {
                case_id: "case-1".to_owned(),
                source: &source,
                expected,
                prediction: None,
            }],
        )
        .unwrap();
        assert!(!evaluation.passes_declared_gate());
        assert_eq!(evaluation.prediction_coverage_bps(), 0);
        assert_eq!(evaluation.missing_slices().len(), 3);
    }

    #[test]
    fn aggregate_coverage_cannot_hide_a_zero_coverage_required_slice() {
        let profile = profile_v1();
        let sources = (1_u8..=23)
            .map(|discriminator| source_v1(&profile, discriminator))
            .collect::<Vec<_>>();
        let expected = sources
            .iter()
            .enumerate()
            .map(|(index, source)| {
                let (event, phase) = match index {
                    0 => (
                        MtgoCompetitiveEventKindV1::League,
                        MtgoCompetitiveLifecyclePhaseV1::EntryReview,
                    ),
                    1 => (
                        MtgoCompetitiveEventKindV1::Challenge,
                        MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
                    ),
                    2 => (
                        MtgoCompetitiveEventKindV1::Challenge,
                        MtgoCompetitiveLifecyclePhaseV1::EntryReview,
                    ),
                    _ => (
                        MtgoCompetitiveEventKindV1::League,
                        MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
                    ),
                };
                snapshot_v1(
                    event,
                    phase,
                    u64::try_from(index + 1).unwrap(),
                    source.canonical_bgra8_sha256().to_owned(),
                )
            })
            .collect::<Vec<_>>();
        let predictions = sources
            .iter()
            .zip(expected.iter().cloned())
            .enumerate()
            .map(|(index, (source, expected))| {
                prediction_v1(&profile, source, expected, u8::try_from(index + 1).unwrap())
            })
            .collect::<Vec<_>>();
        let cases = sources
            .iter()
            .zip(expected.iter())
            .zip(predictions.iter())
            .enumerate()
            .map(|(index, ((source, expected), prediction))| {
                MtgoCompetitiveNavigationEvaluationCaseV1 {
                    case_id: format!("case-{:02}", index + 1),
                    source,
                    expected: expected.clone(),
                    prediction: (index != 2).then_some(prediction),
                }
            })
            .collect::<Vec<_>>();
        let mut spec = spec_v1(&profile);
        spec.minimum_prediction_coverage_bps = 9_500;
        let evaluation =
            evaluate_untrusted_competitive_navigation_profile_v1(&profile, spec, cases).unwrap();
        assert!(evaluation.prediction_coverage_bps() >= 9_500);
        assert_eq!(evaluation.minimum_prediction_coverage_bps_per_slice(), 0);
        assert!(!evaluation.passes_declared_gate());
    }

    #[test]
    fn declared_classifier_exchange_is_bound_to_exact_profile_and_source() {
        let profile = profile_v1();
        let source = source_v1(&profile, 1);
        let lifecycle = snapshot_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
            1,
            source.canonical_bgra8_sha256().to_owned(),
        );
        let mut record = MtgoCompetitiveNavigationPredictionV1 {
            schema_version: MTGO_COMPETITIVE_NAVIGATION_EVALUATION_SCHEMA_V1,
            profile_commitment_sha256: profile.profile_commitment_sha256().to_owned(),
            source_manifest_sha256: source.manifest_sha256().to_owned(),
            source_canonical_bgra8_sha256: source.canonical_bgra8_sha256().to_owned(),
            classifier_request_sha256: "5".repeat(64),
            classifier_response_sha256: "6".repeat(64),
            lifecycle,
        };
        record.source_manifest_sha256 = "0".repeat(64);
        assert!(
            check_untrusted_competitive_navigation_prediction_v1(&profile, &source, record)
                .is_err()
        );
    }

    #[test]
    fn wrong_role_identity_and_source_frame_fail_closed() {
        let profile = profile_v1();
        let wrong_role = source_from_artifact_v1(
            &profile,
            crate::checked_untrusted_dxgi_artifact_for_test_v1(
                MtgoDxgiCaptureRoleV2::ActingPlayerDuel,
            ),
        );
        let expected = snapshot_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
            1,
            wrong_role.canonical_bgra8_sha256().to_owned(),
        );
        assert!(evaluate_untrusted_competitive_navigation_profile_v1(
            &profile,
            spec_v1(&profile),
            vec![MtgoCompetitiveNavigationEvaluationCaseV1 {
                case_id: "case-1".to_owned(),
                source: &wrong_role,
                expected,
                prediction: None,
            }],
        )
        .is_err());

        let source = source_v1(&profile, 1);
        let wrong_frame = snapshot_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
            1,
            "0".repeat(64),
        );
        assert!(evaluate_untrusted_competitive_navigation_profile_v1(
            &profile,
            spec_v1(&profile),
            vec![MtgoCompetitiveNavigationEvaluationCaseV1 {
                case_id: "case-1".to_owned(),
                source: &source,
                expected: wrong_frame,
                prediction: None,
            }],
        )
        .is_err());
    }

    #[test]
    fn main_window_title_drift_fails_closed() {
        let mut payload = profile_payload_v1();
        payload.window_title_sha256 = "0".repeat(64);
        let wrong_account_profile =
            check_untrusted_competitive_navigation_runtime_profile_v1(payload).unwrap();
        let source = source_v1(&wrong_account_profile, 1);
        let expected = snapshot_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
            1,
            source.canonical_bgra8_sha256().to_owned(),
        );
        let error = evaluate_untrusted_competitive_navigation_profile_v1(
            &wrong_account_profile,
            spec_v1(&wrong_account_profile),
            vec![MtgoCompetitiveNavigationEvaluationCaseV1 {
                case_id: "wrong-main-window-title".to_owned(),
                source: &source,
                expected,
                prediction: None,
            }],
        )
        .err()
        .unwrap();
        assert_eq!(error.code(), "competitive_navigation_source_profile");
    }

    #[test]
    fn approved_account_identity_region_must_be_bounded_and_committed() {
        let mut payload = profile_payload_v1();
        payload.account_identity_rect_client_px.x = 1_549;
        let error = check_untrusted_competitive_navigation_runtime_profile_v1(payload)
            .err()
            .unwrap();
        assert_eq!(
            error.code(),
            "competitive_navigation_account_identity_geometry"
        );
    }

    #[test]
    fn offline_navigation_source_rehashes_the_approved_account_region() {
        let mut payload = profile_payload_v1();
        let pixel_count = usize::try_from(payload.client_size_px.width)
            .unwrap()
            .checked_mul(usize::try_from(payload.client_size_px.height).unwrap())
            .unwrap();
        let mut pixels = vec![0_u8; pixel_count.checked_mul(4).unwrap()];
        payload.account_identity_region_sha256 = visible_frame_region_content_sha256_v1(
            &pixels,
            &payload.client_size_px,
            &payload.account_identity_rect_client_px,
        )
        .unwrap();
        let profile = check_untrusted_competitive_navigation_runtime_profile_v1(payload).unwrap();
        let source = bind_checked_competitive_navigation_source_v1(
            &profile,
            checked_untrusted_dxgi_navigation_artifact_for_test_v1(1),
            &pixels,
        )
        .unwrap();
        assert_eq!(source.source_profile_binding_sha256().len(), 64);
        assert!(!source.safe_for_live_classification());
        assert!(!source.permits_event_entry());
        assert!(!source.permits_spending());
        assert!(!source.safe_for_input());

        let changed_pixel = (24_usize * 1_550 + 24) * 4;
        pixels[changed_pixel] = 1;
        let error = bind_checked_competitive_navigation_source_v1(
            &profile,
            checked_untrusted_dxgi_navigation_artifact_for_test_v1(1),
            &pixels,
        )
        .err()
        .unwrap();
        assert_eq!(
            error.code(),
            "competitive_navigation_source_account_identity"
        );
    }

    #[test]
    fn profile_requires_all_four_slices_in_canonical_order() {
        let mut payload = profile_payload_v1();
        payload.supported_slices.pop();
        assert!(check_untrusted_competitive_navigation_runtime_profile_v1(payload).is_err());
    }
}
