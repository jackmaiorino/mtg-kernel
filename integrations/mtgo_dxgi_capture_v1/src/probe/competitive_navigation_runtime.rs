use super::{
    bind_opaque_navigation_frame_to_competitive_entry_review_identity_with_classification_v1,
    competitive_entry_window_continuity_commitment_for_frame_v1,
    mtgo_process_continuity_commitment_for_frame_v1, serialize_manifest_v2, sha256_hex_v1,
    MtgoAdmittedCompetitiveNavigationFrameCommitmentsV1,
    OpaqueMtgoAdmittedCompetitiveNavigationFrameV1, OpaqueMtgoCompetitiveEntryReviewIdentityV1,
};
use mtgo_blackbox_v1::{
    check_untrusted_competitive_navigation_prediction_v1,
    check_untrusted_competitive_navigation_source_v1,
    validate_visible_competitive_lifecycle_snapshot_v1, AdmittedMtgoCompetitiveNavigationProfileV1,
    CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    CheckedUntrustedMtgoCompetitiveNavigationPredictionV1,
    CheckedUntrustedMtgoCompetitiveNavigationSourceV1, MtgoCompetitiveEventKindV1,
    MtgoCompetitiveLifecyclePhaseV1, MtgoCompetitiveNavigationPredictionV1,
    MtgoCompetitiveNavigationProfileScopeV1, MtgoRectPxV1, MtgoSizePxV1,
    MtgoVisibleCompetitiveLifecycleSnapshotV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const NAVIGATION_CLASSIFIER_RUNTIME_IDENTITY_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-navigation-classifier-runtime-identity-v1";
const NAVIGATION_CLASSIFIER_REQUEST_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-navigation-classifier-request-v1";
const NAVIGATION_CLASSIFIER_RESULT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-navigation-classifier-result-v1";
const NAVIGATION_CLASSIFIER_PROTOCOL_MAGIC_V1: &[u8] = b"MTGO_VISIBLE_COMPETITIVE_NAVIGATION_V1\0";
const MAX_RUNTIME_ARTIFACT_BYTES_V1: u64 = 512 * 1024 * 1024;
const MAX_CLASSIFIER_ASSETS_MANIFEST_BYTES_V1: u64 = 16 * 1024 * 1024;
const MAX_CLASSIFIER_REQUEST_HEADER_BYTES_V1: usize = 1024 * 1024;
const MAX_CLASSIFIER_RESPONSE_BYTES_V1: usize = 2 * 1024 * 1024;
const MAX_CLASSIFIER_STDERR_BYTES_V1: usize = 64 * 1024;

/// Caller-assigned local identity for one navigation frame. It is committed
/// into the classifier exchange and grants no input or event-entry authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveNavigationFrameIdentityV1 {
    pub frame_id: u64,
    pub frame_sequence: u64,
}

/// Strict response schema implemented by the exact reviewed navigation
/// classifier executable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveNavigationClassifierProcessResponseV1 {
    pub schema_version: u32,
    pub request_commitment_sha256: String,
    pub lifecycle: MtgoVisibleCompetitiveLifecycleSnapshotV1,
}

/// Canonical JSON header followed by tightly packed BGRA8 bytes in the private
/// classifier stdin protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveNavigationClassifierRequestHeaderV1 {
    pub schema_version: u32,
    pub protocol: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub captured_at_unix_millis: u128,
    pub canonical_width: u32,
    pub canonical_height: u32,
    pub canonical_stride: u32,
    pub canonical_byte_length: u64,
    pub canonical_bgra8_sha256: String,
    pub source_manifest_sha256: String,
    pub source_capture_commitment_sha256: String,
    pub source_profile_binding_sha256: String,
    pub source_frame_profile_binding_sha256: String,
    pub navigation_profile_commitment_sha256: String,
    pub navigation_profile_admission_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub runtime_identity_commitment_sha256: String,
    pub classifier_binary_sha256: String,
    pub classifier_assets_manifest_sha256: String,
}

/// Structurally checked request metadata. This retains no pixels and does not
/// prove that the request came from an opaque DXGI frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedUntrustedMtgoCompetitiveNavigationClassifierRequestV1 {
    header: MtgoCompetitiveNavigationClassifierRequestHeaderV1,
    request_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveNavigationClassifierRequestV1 {
    pub fn header_v1(&self) -> &MtgoCompetitiveNavigationClassifierRequestHeaderV1 {
        &self.header
    }

    pub fn request_commitment_sha256_v1(&self) -> &str {
        &self.request_commitment_sha256
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

/// Copyable commitments for one exact reviewed classifier runtime. It contains
/// no paths, pixels, lifecycle coordinates, event-entry method, or input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoVerifiedCompetitiveNavigationClassifierRuntimeCommitmentsV1 {
    pub navigation_profile_commitment_sha256: String,
    pub navigation_profile_admission_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub classifier_binary_sha256: String,
    pub classifier_assets_manifest_sha256: String,
    pub runtime_identity_commitment_sha256: String,
}

/// Exact on-disk classifier artifacts matched to one admitted 18-slice
/// navigation profile. Paths and process-launch details remain private.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1;
/// let _forged = OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1>();
/// ```
pub struct OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1 {
    executable_path: PathBuf,
    classifier_assets_manifest_path: PathBuf,
    classifier_assets_manifest_bytes: Box<[u8]>,
    commitments: MtgoVerifiedCompetitiveNavigationClassifierRuntimeCommitmentsV1,
}

impl OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1 {
    pub fn commitments_v1(
        &self,
    ) -> MtgoVerifiedCompetitiveNavigationClassifierRuntimeCommitmentsV1 {
        self.commitments.clone()
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

/// Copyable telemetry for one exact opaque frame and classifier exchange. It
/// contains no visible-fact rectangles, pixels, paths, or input method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1 {
    pub source_frame: MtgoAdmittedCompetitiveNavigationFrameCommitmentsV1,
    pub runtime_identity_commitment_sha256: String,
    pub request_commitment_sha256: String,
    pub classifier_response_sha256: String,
    pub lifecycle_snapshot_commitment_sha256: String,
    pub prediction_commitment_sha256: String,
    pub classification_result_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub phase: MtgoCompetitiveLifecyclePhaseV1,
    pub frame_id: u64,
    pub frame_sequence: u64,
}

/// One admitted approved-account navigation frame retained through the exact
/// reviewed lifecycle classifier and visible-region rehashing.
///
/// The type is move-only. It exposes commitments and lifecycle labels only.
/// It has no pixel, rectangle, process, Join, spending, or input accessor.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoClassifiedCompetitiveNavigationFrameV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoClassifiedCompetitiveNavigationFrameV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoClassifiedCompetitiveNavigationFrameV1;
/// fn cannot_control(value: &OpaqueMtgoClassifiedCompetitiveNavigationFrameV1) {
///     let _ = value.canonical_bgra8();
///     let _ = value.visible_fact_rectangles();
///     let _ = value.join_control();
///     let _ = value.input_command();
/// }
/// ```
pub struct OpaqueMtgoClassifiedCompetitiveNavigationFrameV1 {
    pub(super) _source_frame: OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    pub(super) _lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    _source: CheckedUntrustedMtgoCompetitiveNavigationSourceV1,
    _prediction: CheckedUntrustedMtgoCompetitiveNavigationPredictionV1,
    commitments: MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1,
}

pub(super) struct OpaqueMtgoRetainedCompetitiveNavigationClassificationV1 {
    _source: CheckedUntrustedMtgoCompetitiveNavigationSourceV1,
    _prediction: CheckedUntrustedMtgoCompetitiveNavigationPredictionV1,
    commitments: MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1,
}

impl OpaqueMtgoRetainedCompetitiveNavigationClassificationV1 {
    pub(super) fn commitments_v1(&self) -> MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1 {
        self.commitments.clone()
    }
}

impl OpaqueMtgoClassifiedCompetitiveNavigationFrameV1 {
    pub fn commitments_v1(&self) -> MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.commitments.event_kind
    }

    pub fn phase_v1(&self) -> MtgoCompetitiveLifecyclePhaseV1 {
        self.commitments.phase
    }

    pub fn safe_for_lifecycle_classification_v1(&self) -> bool {
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

    pub(crate) fn lifecycle_snapshot_v1(
        &self,
    ) -> &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1 {
        &self._lifecycle
    }

    pub(crate) fn process_continuity_commitment_sha256_v1(&self) -> String {
        mtgo_process_continuity_commitment_for_frame_v1(&self._source_frame.source_frame)
    }

    pub(crate) fn window_continuity_commitment_sha256_v1(&self) -> Result<String, String> {
        competitive_entry_window_continuity_commitment_for_frame_v1(
            &self._source_frame.source_frame,
        )
    }

    pub(super) fn into_event_listing_parts_v1(
        self,
    ) -> (
        OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
        CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
        OpaqueMtgoRetainedCompetitiveNavigationClassificationV1,
    ) {
        let Self {
            _source_frame,
            _lifecycle,
            _source,
            _prediction,
            commitments,
        } = self;
        let retained = OpaqueMtgoRetainedCompetitiveNavigationClassificationV1 {
            _source,
            _prediction,
            commitments,
        };
        (_source_frame, _lifecycle, retained)
    }
}

/// Consumes one exact classifier result and carries its complete profile,
/// approved-account, runtime, request, response, and pixel lineage into the
/// existing non-actionable entry-review identity. A caller still supplies the
/// human-reviewed event label and its visible region. This function cannot
/// create an event-entry intent, spending authority, coordinates, or input.
pub fn bind_classified_navigation_frame_to_competitive_entry_review_identity_v1(
    classified: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    event_display_label: String,
    event_label_rect_client_px: MtgoRectPxV1,
) -> Result<OpaqueMtgoCompetitiveEntryReviewIdentityV1, String> {
    let OpaqueMtgoClassifiedCompetitiveNavigationFrameV1 {
        _source_frame: admitted_source,
        _lifecycle: lifecycle,
        _source,
        _prediction,
        commitments,
    } = classified;
    if commitments.phase != MtgoCompetitiveLifecyclePhaseV1::EntryReview
        || lifecycle.phase() != MtgoCompetitiveLifecyclePhaseV1::EntryReview
        || commitments.event_kind != lifecycle.event_kind()
        || commitments.frame_id != lifecycle.frame_id_v1()
        || commitments.frame_sequence != lifecycle.frame_sequence()
        || commitments.lifecycle_snapshot_commitment_sha256
            != lifecycle.snapshot_commitment_sha256()
    {
        return Err(
            "classified navigation source is not the exact retained entry-review lifecycle"
                .to_owned(),
        );
    }
    let admitted_commitments = admitted_source.commitments_v1();
    if commitments.source_frame != admitted_commitments {
        return Err("classified navigation source-frame lineage changed".to_owned());
    }
    let OpaqueMtgoAdmittedCompetitiveNavigationFrameV1 {
        source_frame,
        profile_commitment_sha256,
        profile_admission_commitment_sha256,
        approved_account_alias_sha256,
        frame_profile_binding_sha256,
    } = admitted_source;
    if profile_commitment_sha256 != commitments.source_frame.profile_commitment_sha256
        || profile_admission_commitment_sha256
            != commitments.source_frame.profile_admission_commitment_sha256
        || approved_account_alias_sha256 != commitments.source_frame.approved_account_alias_sha256
        || frame_profile_binding_sha256 != commitments.source_frame.frame_profile_binding_sha256
    {
        return Err("classified navigation approved-account profile lineage changed".to_owned());
    }
    let retained_classification = OpaqueMtgoRetainedCompetitiveNavigationClassificationV1 {
        _source,
        _prediction,
        commitments,
    };
    bind_opaque_navigation_frame_to_competitive_entry_review_identity_with_classification_v1(
        source_frame,
        lifecycle,
        event_display_label,
        event_label_rect_client_px,
        Some(retained_classification),
    )
}

/// Checks canonical request framing plus its exact assets and pixel
/// commitments. Success is structural only because caller-supplied bytes do
/// not prove DXGI ownership.
pub fn check_untrusted_competitive_navigation_classifier_request_v1(
    canonical_header_json: &[u8],
    classifier_assets_manifest: &[u8],
    canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoCompetitiveNavigationClassifierRequestV1, String> {
    if canonical_header_json.is_empty()
        || canonical_header_json.len() > MAX_CLASSIFIER_REQUEST_HEADER_BYTES_V1
        || classifier_assets_manifest.is_empty()
        || classifier_assets_manifest.len() as u64 > MAX_CLASSIFIER_ASSETS_MANIFEST_BYTES_V1
    {
        return Err("navigation classifier request header or assets are outside bounds".to_owned());
    }
    let header: MtgoCompetitiveNavigationClassifierRequestHeaderV1 =
        serde_json::from_slice(canonical_header_json)
            .map_err(|error| format!("parse navigation classifier request header: {error}"))?;
    let reserialized = serde_json::to_vec(&header)
        .map_err(|error| format!("serialize navigation classifier request header: {error}"))?;
    if reserialized != canonical_header_json {
        return Err("navigation classifier request header is not canonical JSON".to_owned());
    }
    if header.schema_version != 1
        || header.protocol != "mtgo_visible_competitive_navigation_v1"
        || header.frame_id == 0
        || header.frame_sequence == 0
        || header.captured_at_unix_millis == 0
        || header.canonical_width == 0
        || header.canonical_height == 0
        || header.canonical_width > 16_384
        || header.canonical_height > 16_384
    {
        return Err("navigation classifier request identity is invalid".to_owned());
    }
    let expected_stride = header
        .canonical_width
        .checked_mul(4)
        .ok_or("navigation classifier stride overflow")?;
    let expected_length = u64::from(expected_stride)
        .checked_mul(u64::from(header.canonical_height))
        .ok_or("navigation classifier byte length overflow")?;
    if header.canonical_stride != expected_stride
        || header.canonical_byte_length != expected_length
        || usize::try_from(expected_length).ok() != Some(canonical_bgra8.len())
        || header.canonical_bgra8_sha256 != sha256_hex_v1(canonical_bgra8)
        || header.classifier_assets_manifest_sha256 != sha256_hex_v1(classifier_assets_manifest)
    {
        return Err(
            "navigation classifier pixels or assets do not match the request header".to_owned(),
        );
    }
    for value in [
        header.canonical_bgra8_sha256.as_str(),
        header.source_manifest_sha256.as_str(),
        header.source_capture_commitment_sha256.as_str(),
        header.source_profile_binding_sha256.as_str(),
        header.source_frame_profile_binding_sha256.as_str(),
        header.navigation_profile_commitment_sha256.as_str(),
        header
            .navigation_profile_admission_commitment_sha256
            .as_str(),
        header.approved_account_alias_sha256.as_str(),
        header.runtime_identity_commitment_sha256.as_str(),
        header.classifier_binary_sha256.as_str(),
        header.classifier_assets_manifest_sha256.as_str(),
    ] {
        if !looks_like_lower_sha256_v1(value) {
            return Err("navigation classifier request contains an invalid commitment".to_owned());
        }
    }
    let request_commitment_sha256 = commitment_v1(
        NAVIGATION_CLASSIFIER_REQUEST_DOMAIN_V1,
        &[
            canonical_header_json,
            classifier_assets_manifest,
            canonical_bgra8,
        ],
    );
    Ok(
        CheckedUntrustedMtgoCompetitiveNavigationClassifierRequestV1 {
            header,
            request_commitment_sha256,
        },
    )
}

/// Verifies the exact classifier executable and asset manifest selected by one
/// separately admitted 18-slice lifecycle profile.
pub fn verify_competitive_navigation_classifier_runtime_v1(
    profile: &AdmittedMtgoCompetitiveNavigationProfileV1,
    classifier_binary_path: &Path,
    classifier_assets_manifest_path: &Path,
) -> Result<OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1, String> {
    if profile.scope()
        != MtgoCompetitiveNavigationProfileScopeV1::LeagueAndChallengeLifecycleClassification
    {
        return Err("navigation profile does not cover all 18 lifecycle slices".to_owned());
    }
    let checked = profile.checked_runtime_profile();
    let executable_path = verify_runtime_artifact_v1(
        classifier_binary_path,
        checked.classifier_binary_sha256(),
        "navigation classifier binary",
    )?;
    if executable_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("exe"))
        != Some(true)
    {
        return Err("navigation classifier binary must be an exact .exe artifact".to_owned());
    }
    let classifier_assets_manifest_path = verify_runtime_artifact_v1(
        classifier_assets_manifest_path,
        checked.classifier_assets_manifest_sha256(),
        "navigation classifier assets manifest",
    )?;
    let classifier_assets_manifest_bytes = read_bounded_file_bytes_v1(
        &classifier_assets_manifest_path,
        MAX_CLASSIFIER_ASSETS_MANIFEST_BYTES_V1,
        "navigation classifier assets manifest",
    )?;
    if sha256_hex_v1(&classifier_assets_manifest_bytes)
        != checked.classifier_assets_manifest_sha256()
    {
        return Err("navigation classifier assets manifest changed during verification".to_owned());
    }
    let runtime_identity_commitment_sha256 = commitment_v1(
        NAVIGATION_CLASSIFIER_RUNTIME_IDENTITY_DOMAIN_V1,
        &[
            profile.profile_commitment_sha256().as_bytes(),
            profile.admission_commitment_sha256().as_bytes(),
            checked.approved_account_alias_sha256().as_bytes(),
            checked.classifier_binary_sha256().as_bytes(),
            checked.classifier_assets_manifest_sha256().as_bytes(),
            b"eighteen_slice_lifecycle_classification_only_no_entry_no_spending_no_input",
        ],
    );
    Ok(OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1 {
        executable_path,
        classifier_assets_manifest_path,
        classifier_assets_manifest_bytes: classifier_assets_manifest_bytes.into_boxed_slice(),
        commitments: MtgoVerifiedCompetitiveNavigationClassifierRuntimeCommitmentsV1 {
            navigation_profile_commitment_sha256: profile.profile_commitment_sha256().to_owned(),
            navigation_profile_admission_commitment_sha256: profile
                .admission_commitment_sha256()
                .to_owned(),
            approved_account_alias_sha256: checked.approved_account_alias_sha256().to_owned(),
            classifier_binary_sha256: checked.classifier_binary_sha256().to_owned(),
            classifier_assets_manifest_sha256: checked
                .classifier_assets_manifest_sha256()
                .to_owned(),
            runtime_identity_commitment_sha256,
        },
    })
}

/// Runs the exact profile-pinned classifier over one opaque approved-account
/// frame. Every response fact is rehashed against the retained pixels before
/// the opaque classified result is constructed.
pub fn classify_admitted_mtgo_competitive_navigation_frame_v1(
    source_frame: OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    profile: &AdmittedMtgoCompetitiveNavigationProfileV1,
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    identity: MtgoCompetitiveNavigationFrameIdentityV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoClassifiedCompetitiveNavigationFrameV1, String> {
    if identity.frame_id == 0 || identity.frame_sequence == 0 {
        return Err("navigation classifier frame identity must be nonzero".to_owned());
    }
    if !(100..=60_000).contains(&timeout_ms) {
        return Err("navigation classifier timeout must be between 100 and 60000 ms".to_owned());
    }
    if profile.scope()
        != MtgoCompetitiveNavigationProfileScopeV1::LeagueAndChallengeLifecycleClassification
    {
        return Err("navigation profile does not cover all 18 lifecycle slices".to_owned());
    }
    let frame_commitments = source_frame.commitments_v1();
    let checked_profile = profile.checked_runtime_profile();
    if frame_commitments.profile_commitment_sha256 != profile.profile_commitment_sha256()
        || frame_commitments.profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
        || frame_commitments.approved_account_alias_sha256
            != checked_profile.approved_account_alias_sha256()
        || runtime.commitments.navigation_profile_commitment_sha256
            != profile.profile_commitment_sha256()
        || runtime
            .commitments
            .navigation_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
        || runtime.commitments.approved_account_alias_sha256
            != checked_profile.approved_account_alias_sha256()
    {
        return Err("navigation frame, runtime, and admitted profile differ".to_owned());
    }
    verify_runtime_identity_now_v1(runtime)?;

    let raw_source = &source_frame.source_frame;
    let width = raw_source.manifest.frame.canonical_width;
    let height = raw_source.manifest.frame.canonical_height;
    let stride = width
        .checked_mul(4)
        .ok_or("navigation classifier canonical stride overflow")?;
    let byte_length = u64::try_from(raw_source.canonical_bgra8.len())
        .map_err(|_| "navigation classifier canonical byte length overflow")?;
    if raw_source.manifest.frame.canonical_stride != stride
        || raw_source.manifest.frame.canonical_byte_length != raw_source.canonical_bgra8.len()
        || raw_source.manifest.frame.canonical_bgra8_sha256
            != sha256_hex_v1(&raw_source.canonical_bgra8)
    {
        return Err("opaque navigation pixels no longer match capture metadata".to_owned());
    }
    let manifest_bytes = serialize_manifest_v2(&raw_source.manifest)
        .map_err(|error| format!("serialize opaque navigation source manifest: {error}"))?;
    let checked_source = check_untrusted_competitive_navigation_source_v1(
        checked_profile,
        &manifest_bytes,
        &raw_source.canonical_bgra8,
        &raw_source.preview_png,
    )
    .map_err(|error| format!("check opaque navigation source: {error}"))?;
    let header = MtgoCompetitiveNavigationClassifierRequestHeaderV1 {
        schema_version: 1,
        protocol: "mtgo_visible_competitive_navigation_v1".to_owned(),
        frame_id: identity.frame_id,
        frame_sequence: identity.frame_sequence,
        captured_at_unix_millis: raw_source.manifest.captured_at_unix_millis,
        canonical_width: width,
        canonical_height: height,
        canonical_stride: stride,
        canonical_byte_length: byte_length,
        canonical_bgra8_sha256: checked_source.canonical_bgra8_sha256().to_owned(),
        source_manifest_sha256: checked_source.manifest_sha256().to_owned(),
        source_capture_commitment_sha256: raw_source.capture_commitment_sha256.clone(),
        source_profile_binding_sha256: checked_source.source_profile_binding_sha256().to_owned(),
        source_frame_profile_binding_sha256: source_frame.frame_profile_binding_sha256.clone(),
        navigation_profile_commitment_sha256: profile.profile_commitment_sha256().to_owned(),
        navigation_profile_admission_commitment_sha256: profile
            .admission_commitment_sha256()
            .to_owned(),
        approved_account_alias_sha256: checked_profile.approved_account_alias_sha256().to_owned(),
        runtime_identity_commitment_sha256: runtime
            .commitments
            .runtime_identity_commitment_sha256
            .clone(),
        classifier_binary_sha256: checked_profile.classifier_binary_sha256().to_owned(),
        classifier_assets_manifest_sha256: checked_profile
            .classifier_assets_manifest_sha256()
            .to_owned(),
    };
    let header_json = serde_json::to_vec(&header)
        .map_err(|error| format!("serialize navigation classifier request: {error}"))?;
    let checked_request = check_untrusted_competitive_navigation_classifier_request_v1(
        &header_json,
        &runtime.classifier_assets_manifest_bytes,
        &raw_source.canonical_bgra8,
    )?;
    let request_commitment_sha256 = checked_request.request_commitment_sha256_v1().to_owned();
    let response_bytes = invoke_verified_navigation_classifier_process_v1(
        runtime,
        &header_json,
        &runtime.classifier_assets_manifest_bytes,
        &raw_source.canonical_bgra8,
        Duration::from_millis(u64::from(timeout_ms)),
    )?;
    verify_runtime_identity_now_v1(runtime)?;
    let response = parse_classifier_response_v1(&response_bytes, &request_commitment_sha256)?;
    if response.lifecycle.frame_id != identity.frame_id
        || response.lifecycle.frame_sequence != identity.frame_sequence
    {
        return Err("navigation classifier response changed the frame identity".to_owned());
    }
    let lifecycle = validate_visible_competitive_lifecycle_snapshot_v1(response.lifecycle.clone())
        .map_err(|error| format!("validate navigation lifecycle response: {error}"))?;
    rehash_lifecycle_visible_facts_v1(
        &response.lifecycle,
        &raw_source.canonical_bgra8,
        &MtgoSizePxV1 { width, height },
    )?;
    let classifier_response_sha256 = sha256_hex_v1(&response_bytes);
    let prediction = check_untrusted_competitive_navigation_prediction_v1(
        checked_profile,
        &checked_source,
        MtgoCompetitiveNavigationPredictionV1 {
            schema_version: 1,
            profile_commitment_sha256: profile.profile_commitment_sha256().to_owned(),
            source_manifest_sha256: checked_source.manifest_sha256().to_owned(),
            source_canonical_bgra8_sha256: checked_source.canonical_bgra8_sha256().to_owned(),
            classifier_request_sha256: request_commitment_sha256.clone(),
            classifier_response_sha256: classifier_response_sha256.clone(),
            lifecycle: response.lifecycle,
        },
    )
    .map_err(|error| format!("bind navigation classifier prediction: {error}"))?;
    let lifecycle_snapshot_commitment_sha256 = lifecycle.snapshot_commitment_sha256().to_owned();
    let prediction_commitment_sha256 = prediction.prediction_commitment_sha256().to_owned();
    let classification_result_commitment_sha256 = commitment_v1(
        NAVIGATION_CLASSIFIER_RESULT_DOMAIN_V1,
        &[
            source_frame.frame_profile_binding_sha256.as_bytes(),
            runtime
                .commitments
                .runtime_identity_commitment_sha256
                .as_bytes(),
            request_commitment_sha256.as_bytes(),
            classifier_response_sha256.as_bytes(),
            lifecycle_snapshot_commitment_sha256.as_bytes(),
            prediction_commitment_sha256.as_bytes(),
            b"opaque_eighteen_slice_lifecycle_classification_no_entry_no_spending_no_input",
        ],
    );
    let commitments = MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1 {
        source_frame: frame_commitments,
        runtime_identity_commitment_sha256: runtime
            .commitments
            .runtime_identity_commitment_sha256
            .clone(),
        request_commitment_sha256,
        classifier_response_sha256,
        lifecycle_snapshot_commitment_sha256,
        prediction_commitment_sha256,
        classification_result_commitment_sha256,
        event_kind: lifecycle.event_kind(),
        phase: lifecycle.phase(),
        frame_id: identity.frame_id,
        frame_sequence: identity.frame_sequence,
    };
    Ok(OpaqueMtgoClassifiedCompetitiveNavigationFrameV1 {
        _source_frame: source_frame,
        _lifecycle: lifecycle,
        _source: checked_source,
        _prediction: prediction,
        commitments,
    })
}

fn parse_classifier_response_v1(
    response_bytes: &[u8],
    request_commitment_sha256: &str,
) -> Result<MtgoCompetitiveNavigationClassifierProcessResponseV1, String> {
    let response: MtgoCompetitiveNavigationClassifierProcessResponseV1 =
        serde_json::from_slice(response_bytes).map_err(|error| {
            format!("navigation classifier response is not one strict JSON value: {error}")
        })?;
    let canonical = serde_json::to_vec(&response)
        .map_err(|error| format!("serialize navigation classifier response: {error}"))?;
    if canonical != response_bytes {
        return Err("navigation classifier response is not canonical JSON".to_owned());
    }
    if response.schema_version != 1
        || response.request_commitment_sha256 != request_commitment_sha256
    {
        return Err("navigation classifier response does not bind the exact request".to_owned());
    }
    Ok(response)
}

fn rehash_lifecycle_visible_facts_v1(
    lifecycle: &MtgoVisibleCompetitiveLifecycleSnapshotV1,
    canonical_bgra8: &[u8],
    size: &MtgoSizePxV1,
) -> Result<(), String> {
    for fact in &lifecycle.facts {
        let actual = mtgo_blackbox_v1::visible_frame_region_content_sha256_v1(
            canonical_bgra8,
            size,
            &fact.rect_client_px,
        )
        .map_err(|error| format!("rehash navigation lifecycle fact pixels: {error}"))?;
        if actual != fact.content_sha256 {
            return Err(
                "navigation classifier lifecycle fact does not match retained source pixels"
                    .to_owned(),
            );
        }
    }
    Ok(())
}

fn verify_runtime_artifact_v1(
    path: &Path,
    expected_sha256: &str,
    label: &str,
) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!("{label} path must be absolute"));
    }
    let link_metadata =
        fs::symlink_metadata(path).map_err(|error| format!("inspect {label} metadata: {error}"))?;
    if link_metadata.file_type().is_symlink() || !link_metadata.is_file() {
        return Err(format!("{label} must be a regular non-symlink file"));
    }
    let canonical = fs::canonicalize(path).map_err(|error| format!("resolve {label}: {error}"))?;
    let actual_sha256 = hash_bounded_file_v1(&canonical, label)?;
    if actual_sha256 != expected_sha256 {
        return Err(format!("{label} hash differs from the admitted profile"));
    }
    Ok(canonical)
}

pub(super) fn verify_runtime_identity_now_v1(
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
) -> Result<(), String> {
    for (path, expected, label) in [
        (
            runtime.executable_path.as_path(),
            runtime.commitments.classifier_binary_sha256.as_str(),
            "navigation classifier binary",
        ),
        (
            runtime.classifier_assets_manifest_path.as_path(),
            runtime
                .commitments
                .classifier_assets_manifest_sha256
                .as_str(),
            "navigation classifier assets manifest",
        ),
    ] {
        let canonical = verify_runtime_artifact_v1(path, expected, label)?;
        if canonical != path {
            return Err(format!("{label} canonical path changed after verification"));
        }
    }
    Ok(())
}

fn hash_bounded_file_v1(path: &Path, label: &str) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| format!("open {label}: {error}"))?;
    let length = file
        .metadata()
        .map_err(|error| format!("inspect {label}: {error}"))?
        .len();
    if length == 0 || length > MAX_RUNTIME_ARTIFACT_BYTES_V1 {
        return Err(format!("{label} length is outside the supported range"));
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("read {label}: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn read_bounded_file_bytes_v1(
    path: &Path,
    maximum_bytes: u64,
    label: &str,
) -> Result<Vec<u8>, String> {
    let mut file = File::open(path).map_err(|error| format!("open {label}: {error}"))?;
    let length = file
        .metadata()
        .map_err(|error| format!("inspect {label}: {error}"))?
        .len();
    if length == 0 || length > maximum_bytes || length > usize::MAX as u64 {
        return Err(format!("{label} length is outside the supported range"));
    }
    let mut bytes = Vec::with_capacity(length as usize);
    file.read_to_end(&mut bytes)
        .map_err(|error| format!("read {label}: {error}"))?;
    if bytes.len() as u64 != length {
        return Err(format!("{label} changed length while being read"));
    }
    Ok(bytes)
}

fn invoke_verified_navigation_classifier_process_v1(
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    header_json: &[u8],
    classifier_assets_manifest: &[u8],
    canonical_bgra8: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    invoke_verified_classifier_process_v1(
        runtime,
        "--mtgo-visible-competitive-navigation-v1",
        NAVIGATION_CLASSIFIER_PROTOCOL_MAGIC_V1,
        header_json,
        classifier_assets_manifest,
        canonical_bgra8,
        timeout,
    )
}

pub(super) fn invoke_verified_competitive_event_record_classifier_process_v1(
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    header_json: &[u8],
    classifier_assets_manifest: &[u8],
    canonical_bgra8: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    invoke_verified_classifier_process_v1(
        runtime,
        "--mtgo-visible-competitive-event-record-v1",
        b"MTGO_VISIBLE_COMPETITIVE_EVENT_RECORD_V1\0",
        header_json,
        classifier_assets_manifest,
        canonical_bgra8,
        timeout,
    )
}

pub(super) fn invoke_verified_competitive_event_listing_classifier_process_v1(
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    header_json: &[u8],
    classifier_assets_manifest: &[u8],
    canonical_bgra8: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    invoke_verified_classifier_process_v1(
        runtime,
        "--mtgo-visible-competitive-event-listing-v1",
        b"MTGO_VISIBLE_COMPETITIVE_EVENT_LISTING_V1\0",
        header_json,
        classifier_assets_manifest,
        canonical_bgra8,
        timeout,
    )
}

pub(super) fn invoke_verified_competitive_sideboard_classifier_process_v1(
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    header_json: &[u8],
    classifier_assets_manifest: &[u8],
    canonical_bgra8: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    invoke_verified_classifier_process_v1(
        runtime,
        "--mtgo-visible-competitive-sideboard-v1",
        b"MTGO_VISIBLE_COMPETITIVE_SIDEBOARD_V1\0",
        header_json,
        classifier_assets_manifest,
        canonical_bgra8,
        timeout,
    )
}

pub(super) fn competitive_navigation_classifier_assets_manifest_bytes_v1(
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
) -> &[u8] {
    &runtime.classifier_assets_manifest_bytes
}

fn invoke_verified_classifier_process_v1(
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    mode_argument: &str,
    protocol_magic: &[u8],
    header_json: &[u8],
    classifier_assets_manifest: &[u8],
    canonical_bgra8: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    let mut child = Command::new(&runtime.executable_path)
        .arg(mode_argument)
        .current_dir(
            runtime
                .executable_path
                .parent()
                .ok_or("navigation classifier binary has no parent directory")?,
        )
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("start verified navigation classifier runtime: {error}"))?;
    let mut stdin = match child.stdin.take() {
        Some(value) => value,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("verified navigation classifier runtime has no stdin".to_owned());
        }
    };
    let mut stdout = match child.stdout.take() {
        Some(value) => value,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("verified navigation classifier runtime has no stdout".to_owned());
        }
    };
    let mut stderr = match child.stderr.take() {
        Some(value) => value,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("verified navigation classifier runtime has no stderr".to_owned());
        }
    };
    let started = Instant::now();
    let (status, output, output_truncated, stderr_digest, stderr_truncated) =
        thread::scope(|scope| {
            let writer = scope.spawn(|| -> Result<(), String> {
                stdin
                    .write_all(protocol_magic)
                    .and_then(|_| {
                        stdin.write_all(
                            &u64::try_from(header_json.len())
                                .map_err(|_| {
                                    std::io::Error::new(
                                        std::io::ErrorKind::InvalidInput,
                                        "navigation classifier header is too large",
                                    )
                                })?
                                .to_be_bytes(),
                        )
                    })
                    .and_then(|_| {
                        stdin.write_all(
                            &u64::try_from(classifier_assets_manifest.len())
                                .map_err(|_| {
                                    std::io::Error::new(
                                        std::io::ErrorKind::InvalidInput,
                                        "navigation classifier assets manifest is too large",
                                    )
                                })?
                                .to_be_bytes(),
                        )
                    })
                    .and_then(|_| stdin.write_all(header_json))
                    .and_then(|_| stdin.write_all(classifier_assets_manifest))
                    .and_then(|_| stdin.write_all(canonical_bgra8))
                    .map_err(|error| format!("write navigation classifier request: {error}"))?;
                drop(stdin);
                Ok(())
            });
            let stdout_reader = scope
                .spawn(|| read_bounded_and_drain_v1(&mut stdout, MAX_CLASSIFIER_RESPONSE_BYTES_V1));
            let stderr_reader = scope
                .spawn(|| read_bounded_and_drain_v1(&mut stderr, MAX_CLASSIFIER_STDERR_BYTES_V1));
            let status = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) => {}
                    Err(error) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(format!("poll navigation classifier runtime: {error}"));
                    }
                }
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("verified navigation classifier runtime timed out".to_owned());
                }
                thread::sleep(Duration::from_millis(5));
            };
            writer
                .join()
                .map_err(|_| "navigation classifier request writer panicked".to_owned())??;
            let (output, output_truncated) = stdout_reader
                .join()
                .map_err(|_| "navigation classifier stdout reader panicked".to_owned())??;
            let (stderr_bytes, stderr_truncated) = stderr_reader
                .join()
                .map_err(|_| "navigation classifier stderr reader panicked".to_owned())??;
            Ok::<_, String>((
                status,
                output,
                output_truncated,
                sha256_hex_v1(&stderr_bytes),
                stderr_truncated,
            ))
        })?;
    validate_process_result_v1(
        status,
        output,
        output_truncated,
        &stderr_digest,
        stderr_truncated,
    )
}

fn read_bounded_and_drain_v1(
    reader: &mut impl Read,
    maximum_retained: usize,
) -> Result<(Vec<u8>, bool), String> {
    let mut retained = Vec::new();
    let mut truncated = false;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("read navigation classifier process pipe: {error}"))?;
        if read == 0 {
            break;
        }
        let remaining = maximum_retained.saturating_sub(retained.len());
        let keep = remaining.min(read);
        retained.extend_from_slice(&buffer[..keep]);
        truncated |= keep != read;
    }
    Ok((retained, truncated))
}

fn validate_process_result_v1(
    status: ExitStatus,
    output: Vec<u8>,
    output_truncated: bool,
    stderr_digest: &str,
    stderr_truncated: bool,
) -> Result<Vec<u8>, String> {
    if !status.success() {
        return Err(format!(
            "verified navigation classifier runtime failed: status={status},stderr_sha256={stderr_digest},stderr_truncated={stderr_truncated}"
        ));
    }
    if output_truncated || output.is_empty() {
        return Err("verified navigation classifier response is empty or exceeds 2 MiB".to_owned());
    }
    Ok(output)
}

fn looks_like_lower_sha256_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtgo_blackbox_v1::{
        MtgoLifecycleVisibleFactKindV1, MtgoLifecycleVisibleFactV1, MtgoRectPxV1,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    fn request_header_v1(
        pixels: &[u8],
        assets: &[u8],
    ) -> MtgoCompetitiveNavigationClassifierRequestHeaderV1 {
        MtgoCompetitiveNavigationClassifierRequestHeaderV1 {
            schema_version: 1,
            protocol: "mtgo_visible_competitive_navigation_v1".to_owned(),
            frame_id: 7,
            frame_sequence: 9,
            captured_at_unix_millis: 11,
            canonical_width: 2,
            canonical_height: 2,
            canonical_stride: 8,
            canonical_byte_length: 16,
            canonical_bgra8_sha256: sha256_hex_v1(pixels),
            source_manifest_sha256: "a".repeat(64),
            source_capture_commitment_sha256: "b".repeat(64),
            source_profile_binding_sha256: "c".repeat(64),
            source_frame_profile_binding_sha256: "d".repeat(64),
            navigation_profile_commitment_sha256: "e".repeat(64),
            navigation_profile_admission_commitment_sha256: "f".repeat(64),
            approved_account_alias_sha256: "1".repeat(64),
            runtime_identity_commitment_sha256: "2".repeat(64),
            classifier_binary_sha256: "3".repeat(64),
            classifier_assets_manifest_sha256: sha256_hex_v1(assets),
        }
    }

    fn lifecycle_v1(request: &str) -> MtgoCompetitiveNavigationClassifierProcessResponseV1 {
        MtgoCompetitiveNavigationClassifierProcessResponseV1 {
            schema_version: 1,
            request_commitment_sha256: request.to_owned(),
            lifecycle: MtgoVisibleCompetitiveLifecycleSnapshotV1 {
                schema_version: 1,
                snapshot_id: "snapshot-1".to_owned(),
                event_kind: MtgoCompetitiveEventKindV1::League,
                phase: MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
                frame_id: 7,
                frame_sequence: 9,
                frame_sha256: "5".repeat(64),
                client_bounds: MtgoRectPxV1 {
                    x: 0,
                    y: 0,
                    width: 2,
                    height: 2,
                },
                event_identity_sha256: None,
                match_identity_sha256: None,
                game_number: None,
                entry_terms: None,
                visible_state_complete: true,
                facts: vec![MtgoLifecycleVisibleFactV1 {
                    kind: MtgoLifecycleVisibleFactKindV1::EventBrowserVisible,
                    rect_client_px: MtgoRectPxV1 {
                        x: 0,
                        y: 0,
                        width: 2,
                        height: 2,
                    },
                    content_sha256: "6".repeat(64),
                    confidence_bps: 10_000,
                }],
            },
        }
    }

    #[test]
    fn request_checker_binds_canonical_header_and_exact_pixels() {
        let pixels = [7_u8; 16];
        let assets = b"exact-assets-manifest";
        let header = request_header_v1(&pixels, assets);
        let json = serde_json::to_vec(&header).unwrap();
        let checked =
            check_untrusted_competitive_navigation_classifier_request_v1(&json, assets, &pixels)
                .unwrap();
        assert_eq!(checked.header_v1(), &header);
        assert_eq!(checked.request_commitment_sha256_v1().len(), 64);
        assert!(!checked.permits_event_entry_v1());
        assert!(!checked.permits_spending_v1());
        assert!(!checked.safe_for_input_v1());

        let mut changed = pixels;
        changed[3] ^= 1;
        assert!(
            check_untrusted_competitive_navigation_classifier_request_v1(&json, assets, &changed)
                .is_err()
        );
        assert!(
            check_untrusted_competitive_navigation_classifier_request_v1(
                &json,
                b"changed-assets-manifest",
                &pixels,
            )
            .is_err()
        );
        assert!(
            check_untrusted_competitive_navigation_classifier_request_v1(&json, b"", &pixels)
                .is_err()
        );
    }

    #[test]
    fn request_checker_rejects_noncanonical_json_and_geometry_drift() {
        let pixels = [9_u8; 16];
        let assets = b"exact-assets-manifest";
        let mut header = request_header_v1(&pixels, assets);
        let mut json = serde_json::to_vec(&header).unwrap();
        json.push(b'\n');
        assert!(
            check_untrusted_competitive_navigation_classifier_request_v1(&json, assets, &pixels)
                .is_err()
        );

        header.canonical_stride = 12;
        let json = serde_json::to_vec(&header).unwrap();
        assert!(
            check_untrusted_competitive_navigation_classifier_request_v1(&json, assets, &pixels)
                .is_err()
        );
    }

    #[test]
    fn response_parser_requires_canonical_exact_request_echo() {
        let request = "a".repeat(64);
        let response = lifecycle_v1(&request);
        let json = serde_json::to_vec(&response).unwrap();
        assert_eq!(
            parse_classifier_response_v1(&json, &request).unwrap(),
            response
        );
        assert!(parse_classifier_response_v1(&json, &"b".repeat(64)).is_err());
        let mut noncanonical = json;
        noncanonical.push(b' ');
        assert!(parse_classifier_response_v1(&noncanonical, &request).is_err());
    }

    #[test]
    fn visible_fact_rehash_rejects_classifier_pixel_mismatch() {
        let pixels = [3_u8; 16];
        let mut response = lifecycle_v1(&"a".repeat(64));
        response.lifecycle.frame_sha256 = sha256_hex_v1(&pixels);
        response.lifecycle.facts[0].content_sha256 =
            mtgo_blackbox_v1::visible_frame_region_content_sha256_v1(
                &pixels,
                &MtgoSizePxV1 {
                    width: 2,
                    height: 2,
                },
                &response.lifecycle.facts[0].rect_client_px,
            )
            .unwrap();
        rehash_lifecycle_visible_facts_v1(
            &response.lifecycle,
            &pixels,
            &MtgoSizePxV1 {
                width: 2,
                height: 2,
            },
        )
        .unwrap();
        response.lifecycle.facts[0].content_sha256 = "0".repeat(64);
        assert!(rehash_lifecycle_visible_facts_v1(
            &response.lifecycle,
            &pixels,
            &MtgoSizePxV1 {
                width: 2,
                height: 2,
            },
        )
        .is_err());
    }

    #[test]
    fn runtime_artifact_verification_requires_absolute_regular_exact_file() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "mtgo-navigation-runtime-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let file = directory.join("classifier.exe");
        fs::write(&file, b"exact-classifier").unwrap();
        let expected = sha256_hex_v1(b"exact-classifier");
        assert_eq!(
            verify_runtime_artifact_v1(&file, &expected, "test classifier").unwrap(),
            fs::canonicalize(&file).unwrap()
        );
        assert!(verify_runtime_artifact_v1(&file, &"0".repeat(64), "test classifier").is_err());
        assert!(verify_runtime_artifact_v1(
            Path::new("classifier.exe"),
            &expected,
            "test classifier"
        )
        .is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn verified_runtime_recheck_rejects_asset_mutation() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "mtgo-navigation-runtime-recheck-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let executable = directory.join("classifier.exe");
        let assets = directory.join("assets.json");
        fs::write(&executable, b"exact-classifier").unwrap();
        fs::write(&assets, b"exact-assets").unwrap();
        let executable = fs::canonicalize(executable).unwrap();
        let assets = fs::canonicalize(assets).unwrap();
        let runtime = OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1 {
            executable_path: executable,
            classifier_assets_manifest_path: assets.clone(),
            classifier_assets_manifest_bytes: b"exact-assets".to_vec().into_boxed_slice(),
            commitments: MtgoVerifiedCompetitiveNavigationClassifierRuntimeCommitmentsV1 {
                navigation_profile_commitment_sha256: "1".repeat(64),
                navigation_profile_admission_commitment_sha256: "2".repeat(64),
                approved_account_alias_sha256: "3".repeat(64),
                classifier_binary_sha256: sha256_hex_v1(b"exact-classifier"),
                classifier_assets_manifest_sha256: sha256_hex_v1(b"exact-assets"),
                runtime_identity_commitment_sha256: "4".repeat(64),
            },
        };
        verify_runtime_identity_now_v1(&runtime).unwrap();
        assert!(!runtime.permits_event_entry_v1());
        assert!(!runtime.permits_spending_v1());
        assert!(!runtime.safe_for_input_v1());

        fs::write(&assets, b"mutated-assets").unwrap();
        assert!(verify_runtime_identity_now_v1(&runtime).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn result_commitment_changes_with_each_runtime_boundary() {
        let baseline = commitment_v1(
            NAVIGATION_CLASSIFIER_RESULT_DOMAIN_V1,
            &[b"frame", b"runtime", b"request", b"response", b"lifecycle"],
        );
        for changed in [
            [
                b"changed".as_slice(),
                b"runtime",
                b"request",
                b"response",
                b"lifecycle",
            ],
            [
                b"frame".as_slice(),
                b"changed",
                b"request",
                b"response",
                b"lifecycle",
            ],
            [
                b"frame".as_slice(),
                b"runtime",
                b"changed",
                b"response",
                b"lifecycle",
            ],
            [
                b"frame".as_slice(),
                b"runtime",
                b"request",
                b"changed",
                b"lifecycle",
            ],
            [
                b"frame".as_slice(),
                b"runtime",
                b"request",
                b"response",
                b"changed",
            ],
        ] {
            assert_ne!(
                baseline,
                commitment_v1(NAVIGATION_CLASSIFIER_RESULT_DOMAIN_V1, &changed)
            );
        }
    }
}
