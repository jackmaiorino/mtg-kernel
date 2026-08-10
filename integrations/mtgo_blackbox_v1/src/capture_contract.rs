use crate::{MtgoContractErrorV1, MtgoRectPxV1};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::Cursor;

pub const MTGO_CALIBRATION_PROFILE_SCHEMA_V1: u32 = 1;
pub const MTGO_CALIBRATION_REVIEW_SCHEMA_V1: u32 = 1;
pub const MTGO_REAL_VISIBLE_FRAME_SCHEMA_V1: u32 = 1;

const PROFILE_HASH_DOMAIN_V1: &[u8] = b"mtgo-calibration-profile-v1";
const REVIEW_HASH_DOMAIN_V1: &[u8] = b"mtgo-calibration-review-v1";
const REAL_FRAME_HASH_DOMAIN_V1: &[u8] = b"mtgo-real-visible-frame-v1";
const PREVIEW_OUTPUT_IDENTITY_DOMAIN_V1: &[u8] = b"mtgo-preview-output-identity-v1";
const REVIEWED_PREVIEW_ADMISSION_DOMAIN_V1: &[u8] = b"mtgo-reviewed-preview-admission-v1";
const REVIEWED_PREVIEW_SCOPE_DOMAIN_V1: &[u8] = b"offline-calibration-preview-only-v1";

// This is the production trust root. It must remain `None` until one exact preview
// commitment is added by a separately reviewed source commit after manual review.
const RATIFIED_REVIEWED_PREVIEW_ADMISSION_COMMITMENT_V1: Option<&str> = None;
const MAX_CLIENT_DIMENSION_V1: u32 = 16_384;
const MAX_CANONICAL_FRAME_BYTES_V1: usize = 512 * 1_048_576;
const MAX_CAPTURE_VALIDATION_LAG_SECONDS_V1: i64 = 10;
const MAX_CAPTURE_FUTURE_SKEW_SECONDS_V1: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PreviewUtcInstantV1 {
    epoch_seconds: i64,
    fractional_100ns: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCanonicalPixelFormatV1 {
    Bgra8UnormTopDownTightlyPackedV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCaptureBackendV1 {
    DxgiDesktopDuplicationV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoSizePxV1 {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoSignedRectDesktopPxV1 {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCalibrationAnchorV1 {
    pub anchor_id: String,
    pub rect_client_px: MtgoRectPxV1,
    pub reference_bgra8_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCalibrationProfilePayloadV1 {
    pub schema_version: u32,
    pub profile_id: String,
    pub source_preview_manifest_sha256: String,
    pub source_preview_frame_sha256: String,
    pub source_preview_canonical_bgra8_sha256: String,
    pub product_version: String,
    pub file_version: String,
    pub executable_sha256: String,
    pub signer_thumbprint: String,
    pub signer_subject_sha256: String,
    pub window_title_sha256: String,
    pub dpi: u32,
    pub client_size_px: MtgoSizePxV1,
    pub output_identity_sha256: String,
    pub output_device_name_sha256: String,
    pub output_bounds_desktop_px: MtgoSignedRectDesktopPxV1,
    pub canonical_pixel_format: MtgoCanonicalPixelFormatV1,
    pub anchors: Vec<MtgoCalibrationAnchorV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCalibrationReviewV1 {
    pub schema_version: u32,
    pub profile_sha256: String,
    pub source_preview_manifest_sha256: String,
    pub source_preview_frame_sha256: String,
    pub reviewer_alias_sha256: String,
    pub reviewed_at_utc: String,
    pub client_only_confirmed: bool,
    pub unobscured_confirmed: bool,
    pub cursor_absent_confirmed: bool,
    pub identity_confirmed: bool,
    pub anchors_confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Structurally checked review data. This is not a trusted human-review attestation.
pub struct CheckedUntrustedMtgoCalibrationProfileV1 {
    payload: MtgoCalibrationProfilePayloadV1,
    profile_sha256: String,
    review_sha256: String,
    reviewed_at_epoch_seconds: i64,
}

impl CheckedUntrustedMtgoCalibrationProfileV1 {
    pub fn payload(&self) -> &MtgoCalibrationProfilePayloadV1 {
        &self.payload
    }

    pub fn profile_sha256(&self) -> &str {
        &self.profile_sha256
    }

    pub fn review_sha256(&self) -> &str {
        &self.review_sha256
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoReviewedPreviewScopeV1 {
    OfflineCalibrationPreviewOnlyV1,
}

/// One exact preview admitted by the private production ratification commitment.
///
/// This type is intentionally opaque and has no raw-pixel accessor. It grants only
/// offline calibration-preview identity. It grants no OCR, semantic-evidence,
/// policy-scoring, live-frame, action, or input authority.
///
/// It also intentionally implements neither `Debug` nor `Clone` and cannot be
/// serialized or deserialized.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::AdmittedMtgoReviewedPreviewV1;
/// fn requires_debug<T: core::fmt::Debug>() {}
/// requires_debug::<AdmittedMtgoReviewedPreviewV1>();
/// ```
pub struct AdmittedMtgoReviewedPreviewV1 {
    checked: CheckedUntrustedMtgoCalibrationProfileV1,
    admission_commitment_sha256: String,
    // Retain the exact immutable reviewed pixels without exposing them. A later
    // separately reviewed calibration consumer may add a crate-private access path.
    _canonical_bgra8: Box<[u8]>,
}

impl AdmittedMtgoReviewedPreviewV1 {
    pub fn scope(&self) -> MtgoReviewedPreviewScopeV1 {
        MtgoReviewedPreviewScopeV1::OfflineCalibrationPreviewOnlyV1
    }

    pub fn profile_sha256(&self) -> &str {
        self.checked.profile_sha256()
    }

    pub fn review_sha256(&self) -> &str {
        self.checked.review_sha256()
    }

    pub fn source_preview_manifest_sha256(&self) -> &str {
        &self.checked.payload.source_preview_manifest_sha256
    }

    pub fn source_preview_frame_sha256(&self) -> &str {
        &self.checked.payload.source_preview_frame_sha256
    }

    pub fn source_preview_canonical_bgra8_sha256(&self) -> &str {
        &self.checked.payload.source_preview_canonical_bgra8_sha256
    }

    pub fn admission_commitment_sha256(&self) -> &str {
        &self.admission_commitment_sha256
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoRealVisibleFrameCandidateV1 {
    pub schema_version: u32,
    pub frame_id: u64,
    pub sequence: u64,
    pub backend: MtgoCaptureBackendV1,
    pub profile_sha256: String,
    pub profile_review_sha256: String,
    pub product_version: String,
    pub file_version: String,
    pub executable_sha256: String,
    pub signer_thumbprint: String,
    pub signer_subject_sha256: String,
    pub window_title_sha256: String,
    pub dpi: u32,
    pub client_rect_desktop_px: MtgoSignedRectDesktopPxV1,
    pub client_size_px: MtgoSizePxV1,
    pub output_identity_sha256: String,
    pub output_device_name_sha256: String,
    pub output_bounds_desktop_px: MtgoSignedRectDesktopPxV1,
    pub canonical_pixel_format: MtgoCanonicalPixelFormatV1,
    pub canonical_client_pixels_sha256: String,
    pub captured_at_utc: String,
    pub last_present_time_qpc: u64,
    pub accumulated_frames: u32,
    pub capture_time_window_identity_confirmed: bool,
    pub capture_time_geometry_confirmed: bool,
    pub foreground_confirmed: bool,
    pub visible_confirmed: bool,
    pub uncloaked_confirmed: bool,
    pub unminimized_confirmed: bool,
    pub output_contained_confirmed: bool,
    pub stable_pre_post_confirmed: bool,
    pub occlusion_free_confirmed: bool,
    pub cursor_absent_confirmed: bool,
    pub desktop_present_confirmed: bool,
    pub protected_content_absent_confirmed: bool,
}

#[derive(Debug, PartialEq, Eq)]
/// Structurally checked producer claims. This type grants no pixel, OCR, evidence, or input access.
pub struct CheckedUntrustedMtgoRealVisibleFrameV1 {
    candidate: MtgoRealVisibleFrameCandidateV1,
    frame_commitment_sha256: String,
}

impl CheckedUntrustedMtgoRealVisibleFrameV1 {
    pub fn frame_id(&self) -> u64 {
        self.candidate.frame_id
    }

    pub fn sequence(&self) -> u64 {
        self.candidate.sequence
    }

    pub fn profile_sha256(&self) -> &str {
        &self.candidate.profile_sha256
    }

    pub fn canonical_pixels_sha256(&self) -> &str {
        &self.candidate.canonical_client_pixels_sha256
    }

    pub fn frame_commitment_sha256(&self) -> &str {
        &self.frame_commitment_sha256
    }
}

pub fn calibration_profile_commitment_v1(
    payload: &MtgoCalibrationProfilePayloadV1,
) -> Result<String, MtgoContractErrorV1> {
    canonical_struct_commitment_v1(PROFILE_HASH_DOMAIN_V1, payload, "profile_serialization")
}

pub fn calibration_review_commitment_v1(
    review: &MtgoCalibrationReviewV1,
) -> Result<String, MtgoContractErrorV1> {
    canonical_struct_commitment_v1(REVIEW_HASH_DOMAIN_V1, review, "review_serialization")
}

pub fn preview_output_identity_commitment_v1(
    device_name: &str,
    bounds: &MtgoSignedRectDesktopPxV1,
) -> Result<String, MtgoContractErrorV1> {
    if device_name.is_empty()
        || device_name.len() > 256
        || device_name.chars().any(char::is_control)
    {
        return Err(capture_error_v1(
            "preview_output_device_name",
            "output device name must be 1 to 256 visible characters",
        ));
    }
    validate_signed_rect_v1("preview_output_bounds", bounds)?;
    let mut hasher = Sha256::new();
    hasher.update(PREVIEW_OUTPUT_IDENTITY_DOMAIN_V1);
    hasher.update((device_name.len() as u64).to_be_bytes());
    hasher.update(device_name.as_bytes());
    hasher.update(bounds.left.to_be_bytes());
    hasher.update(bounds.top.to_be_bytes());
    hasher.update(bounds.width.to_be_bytes());
    hasher.update(bounds.height.to_be_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn reviewed_preview_admission_commitment_v1(
    profile_sha256: &str,
    review_sha256: &str,
    manifest_sha256: &str,
    frame_png_sha256: &str,
    canonical_bgra8_sha256: &str,
) -> Result<String, MtgoContractErrorV1> {
    for (field, value) in [
        ("admission_profile_sha256", profile_sha256),
        ("admission_review_sha256", review_sha256),
        ("admission_manifest_sha256", manifest_sha256),
        ("admission_frame_png_sha256", frame_png_sha256),
        ("admission_canonical_bgra8_sha256", canonical_bgra8_sha256),
    ] {
        validate_lower_sha256_v1(field, value)?;
    }

    let mut hasher = Sha256::new();
    hasher.update(REVIEWED_PREVIEW_ADMISSION_DOMAIN_V1);
    for part in [
        REVIEWED_PREVIEW_SCOPE_DOMAIN_V1,
        profile_sha256.as_bytes(),
        review_sha256.as_bytes(),
        manifest_sha256.as_bytes(),
        frame_png_sha256.as_bytes(),
        canonical_bgra8_sha256.as_bytes(),
    ] {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Admits only the exact preview pinned by the private production ratification.
///
/// Production ratification is currently absent, so this function always fails
/// closed with `reviewed_preview_not_ratified`. A future source review may replace
/// the private `None` with one exact commitment. No runtime review record, caller
/// boolean, or caller-selected trust key can create authority.
pub fn admit_ratified_reviewed_preview_v1(
    checked: CheckedUntrustedMtgoCalibrationProfileV1,
    source_preview_manifest_bytes: &[u8],
    source_preview_frame_png_bytes: &[u8],
    source_preview_canonical_bgra8: Box<[u8]>,
) -> Result<AdmittedMtgoReviewedPreviewV1, MtgoContractErrorV1> {
    admit_reviewed_preview_against_ratification_v1(
        checked,
        source_preview_manifest_bytes,
        source_preview_frame_png_bytes,
        source_preview_canonical_bgra8,
        RATIFIED_REVIEWED_PREVIEW_ADMISSION_COMMITMENT_V1,
    )
}

fn admit_reviewed_preview_against_ratification_v1(
    checked: CheckedUntrustedMtgoCalibrationProfileV1,
    source_preview_manifest_bytes: &[u8],
    source_preview_frame_png_bytes: &[u8],
    source_preview_canonical_bgra8: Box<[u8]>,
    ratified_admission_commitment: Option<&str>,
) -> Result<AdmittedMtgoReviewedPreviewV1, MtgoContractErrorV1> {
    let ratified_admission_commitment = ratified_admission_commitment.ok_or_else(|| {
        capture_error_v1(
            "reviewed_preview_not_ratified",
            "production contains no ratified reviewed-preview commitment",
        )
    })?;
    validate_lower_sha256_v1(
        "ratified_reviewed_preview_admission_commitment",
        ratified_admission_commitment,
    )?;

    let canonical_profile_sha256 = calibration_profile_commitment_v1(&checked.payload)?;
    if canonical_profile_sha256 != checked.profile_sha256 {
        return Err(capture_error_v1(
            "reviewed_preview_profile_commitment",
            "checked profile no longer matches its canonical commitment",
        ));
    }

    let actual_manifest_sha256 = sha256_bytes_v1(source_preview_manifest_bytes);
    let actual_frame_png_sha256 = sha256_bytes_v1(source_preview_frame_png_bytes);
    let actual_canonical_bgra8_sha256 = sha256_bytes_v1(&source_preview_canonical_bgra8);
    for (code, detail, actual, expected) in [
        (
            "reviewed_preview_manifest_hash",
            "reviewed preview manifest bytes differ from the checked profile",
            actual_manifest_sha256.as_str(),
            checked.payload.source_preview_manifest_sha256.as_str(),
        ),
        (
            "reviewed_preview_frame_png_hash",
            "reviewed preview PNG bytes differ from the checked profile",
            actual_frame_png_sha256.as_str(),
            checked.payload.source_preview_frame_sha256.as_str(),
        ),
        (
            "reviewed_preview_canonical_bgra8_hash",
            "reviewed preview canonical pixels differ from the checked profile",
            actual_canonical_bgra8_sha256.as_str(),
            checked
                .payload
                .source_preview_canonical_bgra8_sha256
                .as_str(),
        ),
    ] {
        if actual != expected {
            return Err(capture_error_v1(code, detail));
        }
    }

    let expected_len = canonical_byte_len_v1(&checked.payload.client_size_px)?;
    if source_preview_canonical_bgra8.len() != expected_len {
        return Err(capture_error_v1(
            "reviewed_preview_canonical_bgra8_length",
            "reviewed preview canonical pixels do not match the checked client size",
        ));
    }
    let decoded_bgra8 = decode_preview_png_to_canonical_bgra8_v1(
        source_preview_frame_png_bytes,
        &checked.payload.client_size_px,
        expected_len,
    )?;
    if decoded_bgra8.as_slice() != source_preview_canonical_bgra8.as_ref() {
        return Err(capture_error_v1(
            "reviewed_preview_png_pixel_mismatch",
            "reviewed preview PNG is not the source of the supplied canonical pixels",
        ));
    }

    let admission_commitment_sha256 = reviewed_preview_admission_commitment_v1(
        &checked.profile_sha256,
        &checked.review_sha256,
        &actual_manifest_sha256,
        &actual_frame_png_sha256,
        &actual_canonical_bgra8_sha256,
    )?;
    if admission_commitment_sha256 != ratified_admission_commitment {
        return Err(capture_error_v1(
            "reviewed_preview_not_ratified",
            "reviewed preview does not match the ratified production commitment",
        ));
    }

    Ok(AdmittedMtgoReviewedPreviewV1 {
        checked,
        admission_commitment_sha256,
        _canonical_bgra8: source_preview_canonical_bgra8,
    })
}

#[cfg(test)]
fn admit_reviewed_preview_for_test_v1(
    checked: CheckedUntrustedMtgoCalibrationProfileV1,
    source_preview_manifest_bytes: &[u8],
    source_preview_frame_png_bytes: &[u8],
    source_preview_canonical_bgra8: Box<[u8]>,
    ratified_admission_commitment: &str,
) -> Result<AdmittedMtgoReviewedPreviewV1, MtgoContractErrorV1> {
    admit_reviewed_preview_against_ratification_v1(
        checked,
        source_preview_manifest_bytes,
        source_preview_frame_png_bytes,
        source_preview_canonical_bgra8,
        Some(ratified_admission_commitment),
    )
}

pub fn check_untrusted_calibration_profile_v1(
    payload: MtgoCalibrationProfilePayloadV1,
    review: MtgoCalibrationReviewV1,
    source_preview_manifest_bytes: &[u8],
    source_preview_frame_bytes: &[u8],
    source_preview_canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoCalibrationProfileV1, MtgoContractErrorV1> {
    if payload.schema_version != MTGO_CALIBRATION_PROFILE_SCHEMA_V1 {
        return Err(capture_error_v1(
            "profile_schema",
            "profile schema must be version 1",
        ));
    }
    validate_identifier_v1("profile_id", &payload.profile_id, 128)?;
    validate_identifier_v1("product_version", &payload.product_version, 64)?;
    validate_identifier_v1("file_version", &payload.file_version, 64)?;
    if payload.product_version != payload.file_version {
        return Err(capture_error_v1(
            "profile_version",
            "product and file versions must match exactly",
        ));
    }
    for (field, value) in [
        (
            "source_preview_manifest_sha256",
            &payload.source_preview_manifest_sha256,
        ),
        (
            "source_preview_frame_sha256",
            &payload.source_preview_frame_sha256,
        ),
        (
            "source_preview_canonical_bgra8_sha256",
            &payload.source_preview_canonical_bgra8_sha256,
        ),
        ("executable_sha256", &payload.executable_sha256),
        ("signer_subject_sha256", &payload.signer_subject_sha256),
        ("window_title_sha256", &payload.window_title_sha256),
        ("output_identity_sha256", &payload.output_identity_sha256),
        (
            "output_device_name_sha256",
            &payload.output_device_name_sha256,
        ),
    ] {
        validate_lower_sha256_v1(field, value)?;
    }
    validate_lower_hex_v1("signer_thumbprint", &payload.signer_thumbprint, 40)?;
    if !(96..=480).contains(&payload.dpi) {
        return Err(capture_error_v1(
            "profile_dpi",
            "DPI must be between 96 and 480",
        ));
    }
    validate_size_v1("profile_client_size", &payload.client_size_px)?;
    validate_signed_rect_v1("profile_output_bounds", &payload.output_bounds_desktop_px)?;
    validate_anchors_v1(&payload)?;

    let preview_captured_at = validate_source_preview_v1(
        &payload,
        source_preview_manifest_bytes,
        source_preview_frame_bytes,
        source_preview_canonical_bgra8,
    )?;

    let profile_sha256 = calibration_profile_commitment_v1(&payload)?;
    if review.schema_version != MTGO_CALIBRATION_REVIEW_SCHEMA_V1 {
        return Err(capture_error_v1(
            "review_schema",
            "review schema must be version 1",
        ));
    }
    validate_lower_sha256_v1("review_profile_sha256", &review.profile_sha256)?;
    validate_lower_sha256_v1("reviewer_alias_sha256", &review.reviewer_alias_sha256)?;
    let reviewed_at_epoch_seconds =
        utc_epoch_seconds_v1("reviewed_at_utc", &review.reviewed_at_utc)?;
    let reviewed_at = PreviewUtcInstantV1 {
        epoch_seconds: reviewed_at_epoch_seconds,
        fractional_100ns: 0,
    };
    if preview_captured_at > reviewed_at {
        return Err(capture_error_v1(
            "profile_preview_after_review",
            "preview capture must not occur after its review",
        ));
    }
    if review.profile_sha256 != profile_sha256 {
        return Err(capture_error_v1(
            "review_profile_binding",
            "review does not bind the canonical profile commitment",
        ));
    }
    if review.source_preview_manifest_sha256 != payload.source_preview_manifest_sha256
        || review.source_preview_frame_sha256 != payload.source_preview_frame_sha256
    {
        return Err(capture_error_v1(
            "review_preview_binding",
            "review and profile must bind the same preview manifest and frame",
        ));
    }
    for (field, value) in [
        (
            "review_source_preview_manifest_sha256",
            &review.source_preview_manifest_sha256,
        ),
        (
            "review_source_preview_frame_sha256",
            &review.source_preview_frame_sha256,
        ),
    ] {
        validate_lower_sha256_v1(field, value)?;
    }
    if !review.client_only_confirmed
        || !review.unobscured_confirmed
        || !review.cursor_absent_confirmed
        || !review.identity_confirmed
        || !review.anchors_confirmed
    {
        return Err(capture_error_v1(
            "review_incomplete",
            "all visible-only calibration review assertions must be true",
        ));
    }
    let review_sha256 = calibration_review_commitment_v1(&review)?;
    Ok(CheckedUntrustedMtgoCalibrationProfileV1 {
        payload,
        profile_sha256,
        review_sha256,
        reviewed_at_epoch_seconds,
    })
}

pub fn check_untrusted_real_visible_frame_v1(
    profile: &CheckedUntrustedMtgoCalibrationProfileV1,
    candidate: MtgoRealVisibleFrameCandidateV1,
    canonical_bgra8: Box<[u8]>,
    validation_now_utc: &str,
) -> Result<CheckedUntrustedMtgoRealVisibleFrameV1, MtgoContractErrorV1> {
    if candidate.schema_version != MTGO_REAL_VISIBLE_FRAME_SCHEMA_V1 {
        return Err(capture_error_v1(
            "real_frame_schema",
            "real frame schema must be version 1",
        ));
    }
    if candidate.frame_id == 0 || candidate.sequence == 0 {
        return Err(capture_error_v1(
            "real_frame_identity",
            "frame ID and sequence must be nonzero",
        ));
    }
    let expected = profile.payload();
    if candidate.profile_sha256 != profile.profile_sha256()
        || candidate.profile_review_sha256 != profile.review_sha256()
    {
        return Err(capture_error_v1(
            "real_frame_profile_binding",
            "candidate does not bind the validated profile and review",
        ));
    }
    for (field, actual, expected_value) in [
        (
            "product_version",
            candidate.product_version.as_str(),
            expected.product_version.as_str(),
        ),
        (
            "file_version",
            candidate.file_version.as_str(),
            expected.file_version.as_str(),
        ),
        (
            "executable_sha256",
            candidate.executable_sha256.as_str(),
            expected.executable_sha256.as_str(),
        ),
        (
            "signer_thumbprint",
            candidate.signer_thumbprint.as_str(),
            expected.signer_thumbprint.as_str(),
        ),
        (
            "signer_subject_sha256",
            candidate.signer_subject_sha256.as_str(),
            expected.signer_subject_sha256.as_str(),
        ),
        (
            "window_title_sha256",
            candidate.window_title_sha256.as_str(),
            expected.window_title_sha256.as_str(),
        ),
        (
            "output_identity_sha256",
            candidate.output_identity_sha256.as_str(),
            expected.output_identity_sha256.as_str(),
        ),
        (
            "output_device_name_sha256",
            candidate.output_device_name_sha256.as_str(),
            expected.output_device_name_sha256.as_str(),
        ),
    ] {
        if actual != expected_value {
            return Err(capture_error_v1(
                "real_frame_identity_drift",
                format!("{field} differs from the reviewed profile"),
            ));
        }
    }
    if candidate.dpi != expected.dpi
        || candidate.client_size_px != expected.client_size_px
        || candidate.output_bounds_desktop_px != expected.output_bounds_desktop_px
        || candidate.canonical_pixel_format != expected.canonical_pixel_format
    {
        return Err(capture_error_v1(
            "real_frame_layout_drift",
            "DPI, client size, output bounds, or pixel format differs from the profile",
        ));
    }
    let captured_at_epoch_seconds =
        utc_epoch_seconds_v1("captured_at_utc", &candidate.captured_at_utc)?;
    let validation_now_epoch_seconds =
        utc_epoch_seconds_v1("validation_now_utc", validation_now_utc)?;
    if captured_at_epoch_seconds < profile.reviewed_at_epoch_seconds {
        return Err(capture_error_v1(
            "real_frame_before_review",
            "capture time must not precede the calibration review",
        ));
    }
    if captured_at_epoch_seconds > validation_now_epoch_seconds + MAX_CAPTURE_FUTURE_SKEW_SECONDS_V1
        || validation_now_epoch_seconds - captured_at_epoch_seconds
            > MAX_CAPTURE_VALIDATION_LAG_SECONDS_V1
    {
        return Err(capture_error_v1(
            "real_frame_freshness",
            "capture must be validated immediately against a trusted current UTC time",
        ));
    }
    validate_signed_rect_v1("candidate_client_rect", &candidate.client_rect_desktop_px)?;
    validate_signed_rect_v1(
        "candidate_output_bounds",
        &candidate.output_bounds_desktop_px,
    )?;
    if candidate.client_rect_desktop_px.width != candidate.client_size_px.width
        || candidate.client_rect_desktop_px.height != candidate.client_size_px.height
        || !signed_rect_contains_v1(
            &candidate.output_bounds_desktop_px,
            &candidate.client_rect_desktop_px,
        )
    {
        return Err(capture_error_v1(
            "real_frame_output_containment",
            "client rectangle must match the profile size and be inside the calibrated output",
        ));
    }
    if candidate.last_present_time_qpc == 0 || candidate.accumulated_frames == 0 {
        return Err(capture_error_v1(
            "real_frame_desktop_present",
            "desktop duplication must report a current presented frame",
        ));
    }
    if !candidate.capture_time_window_identity_confirmed
        || !candidate.capture_time_geometry_confirmed
        || !candidate.foreground_confirmed
        || !candidate.visible_confirmed
        || !candidate.uncloaked_confirmed
        || !candidate.unminimized_confirmed
        || !candidate.output_contained_confirmed
        || !candidate.stable_pre_post_confirmed
        || !candidate.occlusion_free_confirmed
        || !candidate.cursor_absent_confirmed
        || !candidate.desktop_present_confirmed
        || !candidate.protected_content_absent_confirmed
    {
        return Err(capture_error_v1(
            "real_frame_safety_assertion",
            "every capture-time visible-desktop safety assertion must be true",
        ));
    }
    validate_lower_sha256_v1(
        "canonical_client_pixels_sha256",
        &candidate.canonical_client_pixels_sha256,
    )?;
    let expected_byte_len = canonical_byte_len_v1(&candidate.client_size_px)?;
    if canonical_bgra8.len() != expected_byte_len {
        return Err(capture_error_v1(
            "real_frame_pixel_length",
            format!(
                "expected {expected_byte_len} canonical bytes but received {}",
                canonical_bgra8.len()
            ),
        ));
    }
    let actual_pixels_sha256 = sha256_bytes_v1(&canonical_bgra8);
    if actual_pixels_sha256 != candidate.canonical_client_pixels_sha256 {
        return Err(capture_error_v1(
            "real_frame_pixel_hash",
            "canonical client pixel hash does not match the supplied bytes",
        ));
    }
    validate_live_anchors_v1(expected, &canonical_bgra8)?;

    let frame_commitment_sha256 = canonical_struct_commitment_v1(
        REAL_FRAME_HASH_DOMAIN_V1,
        &candidate,
        "real_frame_serialization",
    )?;
    Ok(CheckedUntrustedMtgoRealVisibleFrameV1 {
        candidate,
        frame_commitment_sha256,
    })
}

fn validate_anchors_v1(
    payload: &MtgoCalibrationProfilePayloadV1,
) -> Result<(), MtgoContractErrorV1> {
    if payload.anchors.len() < 4 || payload.anchors.len() > 32 {
        return Err(capture_error_v1(
            "profile_anchor_count",
            "a profile must contain between 4 and 32 anchors",
        ));
    }
    let mut ids = HashSet::new();
    let mut previous_id: Option<&str> = None;
    let mut quadrants = [false; 4];
    let mut anchored_area = 0_u64;
    for anchor in &payload.anchors {
        validate_identifier_v1("anchor_id", &anchor.anchor_id, 64)?;
        if !ids.insert(anchor.anchor_id.as_str()) {
            return Err(capture_error_v1(
                "profile_anchor_duplicate",
                "anchor IDs must be unique",
            ));
        }
        if previous_id.is_some_and(|previous| previous >= anchor.anchor_id.as_str()) {
            return Err(capture_error_v1(
                "profile_anchor_order",
                "anchors must be strictly ordered by anchor ID",
            ));
        }
        previous_id = Some(anchor.anchor_id.as_str());
        validate_lower_sha256_v1("reference_bgra8_sha256", &anchor.reference_bgra8_sha256)?;
        validate_local_rect_v1(&anchor.rect_client_px, &payload.client_size_px)?;
        if anchor.rect_client_px.width < 4 || anchor.rect_client_px.height < 4 {
            return Err(capture_error_v1(
                "profile_anchor_size",
                "every anchor must cover at least 4 by 4 pixels",
            ));
        }
        anchored_area = anchored_area
            .checked_add(
                u64::from(anchor.rect_client_px.width) * u64::from(anchor.rect_client_px.height),
            )
            .ok_or_else(|| capture_error_v1("profile_anchor_area", "anchor area overflow"))?;
        let center_x =
            u64::from(anchor.rect_client_px.x) + u64::from(anchor.rect_client_px.width) / 2;
        let center_y =
            u64::from(anchor.rect_client_px.y) + u64::from(anchor.rect_client_px.height) / 2;
        let right_half = center_x >= u64::from(payload.client_size_px.width) / 2;
        let bottom_half = center_y >= u64::from(payload.client_size_px.height) / 2;
        quadrants[usize::from(right_half) + 2 * usize::from(bottom_half)] = true;
    }
    for left_index in 0..payload.anchors.len() {
        for right_index in (left_index + 1)..payload.anchors.len() {
            if local_rects_intersect_v1(
                &payload.anchors[left_index].rect_client_px,
                &payload.anchors[right_index].rect_client_px,
            ) {
                return Err(capture_error_v1(
                    "profile_anchor_overlap",
                    "calibration anchors must not overlap",
                ));
            }
        }
    }
    if anchored_area < 64 || quadrants.iter().any(|present| !present) {
        return Err(capture_error_v1(
            "profile_anchor_coverage",
            "anchors must cover at least 64 pixels and all four client quadrants",
        ));
    }
    Ok(())
}

fn validate_source_preview_v1(
    payload: &MtgoCalibrationProfilePayloadV1,
    manifest_bytes: &[u8],
    frame_bytes: &[u8],
    canonical_bgra8: &[u8],
) -> Result<PreviewUtcInstantV1, MtgoContractErrorV1> {
    if sha256_bytes_v1(manifest_bytes) != payload.source_preview_manifest_sha256 {
        return Err(capture_error_v1(
            "profile_preview_manifest_hash",
            "profile preview-manifest hash does not match the supplied bytes",
        ));
    }
    if sha256_bytes_v1(frame_bytes) != payload.source_preview_frame_sha256 {
        return Err(capture_error_v1(
            "profile_preview_frame_hash",
            "profile preview-frame hash does not match the supplied bytes",
        ));
    }
    let expected_len = canonical_byte_len_v1(&payload.client_size_px)?;
    if canonical_bgra8.len() != expected_len {
        return Err(capture_error_v1(
            "profile_preview_canonical_length",
            "decoded preview pixels do not match the calibrated client size",
        ));
    }
    let decoded_bgra8 = decode_preview_png_to_canonical_bgra8_v1(
        frame_bytes,
        &payload.client_size_px,
        expected_len,
    )?;
    if decoded_bgra8.as_slice() != canonical_bgra8 {
        return Err(capture_error_v1(
            "profile_preview_png_pixel_mismatch",
            "decoded PNG pixels do not exactly match the supplied canonical BGRA8 pixels",
        ));
    }
    if sha256_bytes_v1(canonical_bgra8) != payload.source_preview_canonical_bgra8_sha256 {
        return Err(capture_error_v1(
            "profile_preview_canonical_hash",
            "profile canonical preview hash does not match the decoded pixels",
        ));
    }
    validate_live_anchors_v1(payload, canonical_bgra8)?;

    let manifest: Value = serde_json::from_slice(manifest_bytes)
        .map_err(|error| capture_error_v1("profile_preview_manifest_json", error.to_string()))?;
    require_json_u64_v1(&manifest, &["schema_version"], 1)?;
    require_json_string_v1(
        &manifest,
        &["artifact_kind"],
        "mtgo_visible_desktop_calibration_preview_v1",
    )?;
    require_json_string_v1(&manifest, &["status"], "pending_visual_review")?;
    let captured_at = preview_utc_instant_v1(
        "profile_preview_captured_at_utc",
        json_string_v1(&manifest, &["captured_at_utc"])?,
    )?;
    require_json_string_v1(
        &manifest,
        &["capture_backend"],
        "system_drawing_copy_from_composed_screen_v1",
    )?;
    require_json_string_v1(
        &manifest,
        &["pixel_source"],
        "visible_desktop_client_crop_only",
    )?;
    if !json_value_v1(&manifest, &["profile_id"])?.is_null() {
        return Err(capture_error_v1(
            "profile_preview_manifest_value",
            "preview manifest profile_id must be null before review",
        ));
    }
    for field in [
        "safe_for_semantic_evidence",
        "safe_for_ocr",
        "safe_for_policy_scoring",
        "safe_for_input",
    ] {
        require_json_bool_v1(&manifest, &[field], false)?;
    }

    require_json_string_v1(
        &manifest,
        &["expected_identity", "product_version"],
        &payload.product_version,
    )?;
    require_json_hex_case_insensitive_v1(
        &manifest,
        &["expected_identity", "executable_sha256"],
        &payload.executable_sha256,
    )?;
    require_json_hex_case_insensitive_v1(
        &manifest,
        &["expected_identity", "signer_thumbprint"],
        &payload.signer_thumbprint,
    )?;
    let expected_signer_subject =
        json_string_v1(&manifest, &["expected_identity", "signer_subject"])?;
    require_utf8_sha256_v1(
        "profile_preview_expected_signer_subject",
        expected_signer_subject,
        &payload.signer_subject_sha256,
    )?;
    require_json_u64_v1(
        &manifest,
        &["expected_identity", "dpi"],
        u64::from(payload.dpi),
    )?;
    let expected_window_title = json_string_v1(&manifest, &["expected_identity", "window_title"])?;
    require_utf8_sha256_v1(
        "profile_preview_expected_window_title",
        expected_window_title,
        &payload.window_title_sha256,
    )?;

    let process_id = json_u64_v1(&manifest, &["observed_identity", "process_id"])?;
    if process_id == 0 {
        return Err(capture_error_v1(
            "profile_preview_process_id",
            "preview process ID must be positive",
        ));
    }
    let process_started_at = preview_utc_instant_v1(
        "profile_preview_process_start_utc",
        json_string_v1(&manifest, &["observed_identity", "process_start_utc"])?,
    )?;
    if process_started_at > captured_at {
        return Err(capture_error_v1(
            "profile_preview_process_chronology",
            "preview process start must not follow preview capture",
        ));
    }
    require_json_string_v1(
        &manifest,
        &["observed_identity", "product_version"],
        &payload.product_version,
    )?;
    require_json_string_v1(
        &manifest,
        &["observed_identity", "file_version"],
        &payload.file_version,
    )?;
    require_json_hex_case_insensitive_v1(
        &manifest,
        &["observed_identity", "executable_sha256"],
        &payload.executable_sha256,
    )?;
    require_json_hex_case_insensitive_v1(
        &manifest,
        &["observed_identity", "signer_thumbprint"],
        &payload.signer_thumbprint,
    )?;
    let signer_subject = json_string_v1(&manifest, &["observed_identity", "signer_subject"])?;
    require_utf8_sha256_v1(
        "profile_preview_signer_subject",
        signer_subject,
        &payload.signer_subject_sha256,
    )?;
    if signer_subject != expected_signer_subject {
        return Err(capture_error_v1(
            "profile_preview_identity_disagreement",
            "expected and observed signer subjects must match exactly",
        ));
    }

    let window_title = json_string_v1(&manifest, &["window", "title"])?;
    require_utf8_sha256_v1(
        "profile_preview_window_title",
        window_title,
        &payload.window_title_sha256,
    )?;
    if window_title != expected_window_title {
        return Err(capture_error_v1(
            "profile_preview_identity_disagreement",
            "expected and observed window titles must match exactly",
        ));
    }
    require_json_bool_v1(&manifest, &["window", "foreground"], true)?;
    require_json_bool_v1(&manifest, &["window", "visible"], true)?;
    require_json_bool_v1(&manifest, &["window", "minimized"], false)?;
    require_json_bool_v1(&manifest, &["window", "cloaked"], false)?;
    require_json_u64_v1(&manifest, &["window", "display_affinity"], 0)?;
    require_json_bool_v1(&manifest, &["window", "desktop_composition_enabled"], true)?;
    require_json_u64_v1(&manifest, &["window", "dpi"], u64::from(payload.dpi))?;
    let client_bounds = json_manifest_rect_v1(&manifest, &["window", "client_bounds_desktop_px"])?;
    if client_bounds.width != payload.client_size_px.width
        || client_bounds.height != payload.client_size_px.height
    {
        return Err(capture_error_v1(
            "profile_preview_client_size",
            "preview client bounds must match the calibrated client size",
        ));
    }
    let extended_frame_bounds =
        json_manifest_rect_v1(&manifest, &["window", "extended_frame_bounds_desktop_px"])?;
    if !signed_rect_contains_v1(&extended_frame_bounds, &client_bounds) {
        return Err(capture_error_v1(
            "profile_preview_extended_frame",
            "extended frame bounds must contain the client bounds",
        ));
    }

    let output_device_name = json_string_v1(&manifest, &["monitor", "device_name"])?;
    if output_device_name.is_empty()
        || sha256_bytes_v1(output_device_name.as_bytes()) != payload.output_device_name_sha256
    {
        return Err(capture_error_v1(
            "profile_preview_output_device",
            "preview monitor device name does not match the profile digest",
        ));
    }
    let _is_primary = json_bool_v1(&manifest, &["monitor", "is_primary"])?;
    let output_bounds = json_manifest_rect_v1(&manifest, &["monitor", "bounds_desktop_px"])?;
    if output_bounds != payload.output_bounds_desktop_px {
        return Err(capture_error_v1(
            "profile_preview_output_bounds",
            "preview monitor bounds do not match the profile",
        ));
    }
    if preview_output_identity_commitment_v1(output_device_name, &output_bounds)?
        != payload.output_identity_sha256
    {
        return Err(capture_error_v1(
            "profile_preview_output_identity",
            "preview monitor identity does not match the profile commitment",
        ));
    }
    if !signed_rect_contains_v1(&output_bounds, &client_bounds) {
        return Err(capture_error_v1(
            "profile_preview_output_containment",
            "preview client bounds must be contained by the calibrated monitor",
        ));
    }
    let work_area = json_manifest_rect_v1(&manifest, &["monitor", "work_area_desktop_px"])?;
    if !signed_rect_contains_v1(&output_bounds, &work_area) {
        return Err(capture_error_v1(
            "profile_preview_work_area",
            "preview monitor work area must be contained by its bounds",
        ));
    }

    require_json_bool_v1(&manifest, &["occlusion_audit", "target_found"], true)?;
    let _windows_examined = json_u64_v1(
        &manifest,
        &["occlusion_audit", "windows_examined_above_target"],
    )?;
    require_json_u64_v1(
        &manifest,
        &["occlusion_audit", "intersecting_windows_above_target"],
        0,
    )?;
    let _cursor_showing = json_bool_v1(&manifest, &["occlusion_audit", "cursor_showing"])?;
    require_json_bool_v1(
        &manifest,
        &["occlusion_audit", "cursor_inside_client"],
        false,
    )?;

    require_json_string_v1(&manifest, &["frame", "file"], "frame.png")?;
    require_json_u64_v1(
        &manifest,
        &["frame", "width"],
        u64::from(payload.client_size_px.width),
    )?;
    require_json_u64_v1(
        &manifest,
        &["frame", "height"],
        u64::from(payload.client_size_px.height),
    )?;
    require_json_string_v1(&manifest, &["frame", "format"], "png")?;
    require_json_hex_case_insensitive_v1(
        &manifest,
        &["frame", "sha256"],
        &payload.source_preview_frame_sha256,
    )?;
    let review_requirements = json_value_v1(&manifest, &["review_requirements"])?
        .as_array()
        .ok_or_else(|| {
            capture_error_v1(
                "profile_preview_manifest_type",
                "preview manifest review_requirements must be an array",
            )
        })?;
    if review_requirements.is_empty()
        || review_requirements.iter().any(|requirement| {
            requirement
                .as_str()
                .is_none_or(|text| text.trim().is_empty())
        })
    {
        return Err(capture_error_v1(
            "profile_preview_review_requirements",
            "preview review requirements must contain nonempty strings",
        ));
    }
    Ok(captured_at)
}

pub(crate) fn decode_preview_png_to_canonical_bgra8_v1(
    frame_bytes: &[u8],
    expected_size: &MtgoSizePxV1,
    expected_len: usize,
) -> Result<Vec<u8>, MtgoContractErrorV1> {
    let mut decoder = png::Decoder::new(Cursor::new(frame_bytes));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder
        .read_info()
        .map_err(|error| capture_error_v1("profile_preview_png_decode", error.to_string()))?;
    let (width, height, color_type, bit_depth, interlaced, animated) = {
        let info = reader.info();
        (
            info.width,
            info.height,
            info.color_type,
            info.bit_depth,
            info.interlaced,
            info.animation_control.is_some(),
        )
    };
    if width != expected_size.width || height != expected_size.height {
        return Err(capture_error_v1(
            "profile_preview_png_dimensions",
            "PNG dimensions do not match the calibrated client size",
        ));
    }
    if interlaced || animated || bit_depth != png::BitDepth::Eight {
        return Err(capture_error_v1(
            "profile_preview_png_format",
            "preview PNG must be one non-interlaced 8-bit RGB or RGBA image",
        ));
    }
    let channels = match color_type {
        png::ColorType::Rgb => 3_usize,
        png::ColorType::Rgba => 4_usize,
        _ => {
            return Err(capture_error_v1(
                "profile_preview_png_format",
                "preview PNG must use RGB or RGBA pixels",
            ));
        }
    };
    let source_len = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(channels))
        .ok_or_else(|| {
            capture_error_v1(
                "profile_preview_png_dimensions",
                "PNG decoded size overflow",
            )
        })?;
    let mut source = vec![0_u8; source_len];
    let frame_info = reader
        .next_frame(&mut source)
        .map_err(|error| capture_error_v1("profile_preview_png_decode", error.to_string()))?;
    if frame_info.width != width
        || frame_info.height != height
        || frame_info.color_type != color_type
        || frame_info.bit_depth != bit_depth
        || frame_info.buffer_size() != source_len
    {
        return Err(capture_error_v1(
            "profile_preview_png_format",
            "decoded PNG frame does not match its admitted header",
        ));
    }

    let mut canonical = Vec::with_capacity(expected_len);
    match color_type {
        png::ColorType::Rgb => {
            for pixel in source.chunks_exact(3) {
                canonical.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
            }
        }
        png::ColorType::Rgba => {
            for pixel in source.chunks_exact(4) {
                if pixel[3] != 255 {
                    return Err(capture_error_v1(
                        "profile_preview_png_alpha",
                        "composed-desktop preview PNG pixels must be fully opaque",
                    ));
                }
                canonical.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
            }
        }
        _ => unreachable!("color type was admitted above"),
    }
    if canonical.len() != expected_len {
        return Err(capture_error_v1(
            "profile_preview_png_dimensions",
            "decoded PNG byte length does not match the calibrated client size",
        ));
    }
    Ok(canonical)
}

fn json_value_v1<'a>(value: &'a Value, path: &[&str]) -> Result<&'a Value, MtgoContractErrorV1> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment).ok_or_else(|| {
            capture_error_v1(
                "profile_preview_manifest_field",
                format!("missing preview manifest field {}", path.join(".")),
            )
        })?;
    }
    Ok(current)
}

fn json_string_v1<'a>(value: &'a Value, path: &[&str]) -> Result<&'a str, MtgoContractErrorV1> {
    json_value_v1(value, path)?.as_str().ok_or_else(|| {
        capture_error_v1(
            "profile_preview_manifest_type",
            format!("preview manifest field {} must be a string", path.join(".")),
        )
    })
}

fn json_u64_v1(value: &Value, path: &[&str]) -> Result<u64, MtgoContractErrorV1> {
    json_value_v1(value, path)?.as_u64().ok_or_else(|| {
        capture_error_v1(
            "profile_preview_manifest_type",
            format!(
                "preview manifest field {} must be an unsigned integer",
                path.join(".")
            ),
        )
    })
}

fn json_i64_v1(value: &Value, path: &[&str]) -> Result<i64, MtgoContractErrorV1> {
    json_value_v1(value, path)?.as_i64().ok_or_else(|| {
        capture_error_v1(
            "profile_preview_manifest_type",
            format!(
                "preview manifest field {} must be a signed integer",
                path.join(".")
            ),
        )
    })
}

fn json_bool_v1(value: &Value, path: &[&str]) -> Result<bool, MtgoContractErrorV1> {
    json_value_v1(value, path)?.as_bool().ok_or_else(|| {
        capture_error_v1(
            "profile_preview_manifest_type",
            format!(
                "preview manifest field {} must be a boolean",
                path.join(".")
            ),
        )
    })
}

fn require_utf8_sha256_v1(
    field: &'static str,
    value: &str,
    expected_sha256: &str,
) -> Result<(), MtgoContractErrorV1> {
    if sha256_bytes_v1(value.as_bytes()) != expected_sha256 {
        return Err(capture_error_v1(
            field,
            "preview manifest text does not match the profile digest",
        ));
    }
    Ok(())
}

fn json_manifest_rect_v1(
    value: &Value,
    path: &[&str],
) -> Result<MtgoSignedRectDesktopPxV1, MtgoContractErrorV1> {
    let field_path = |field: &'static str| {
        let mut result = path.to_vec();
        result.push(field);
        result
    };
    let left = i32::try_from(json_i64_v1(value, &field_path("left"))?).map_err(|_| {
        capture_error_v1(
            "profile_preview_manifest_rect",
            format!("preview rectangle {}.left exceeds i32", path.join(".")),
        )
    })?;
    let top = i32::try_from(json_i64_v1(value, &field_path("top"))?).map_err(|_| {
        capture_error_v1(
            "profile_preview_manifest_rect",
            format!("preview rectangle {}.top exceeds i32", path.join(".")),
        )
    })?;
    let right = json_i64_v1(value, &field_path("right"))?;
    let bottom = json_i64_v1(value, &field_path("bottom"))?;
    let width = u32::try_from(json_u64_v1(value, &field_path("width"))?).map_err(|_| {
        capture_error_v1(
            "profile_preview_manifest_rect",
            format!("preview rectangle {}.width exceeds u32", path.join(".")),
        )
    })?;
    let height = u32::try_from(json_u64_v1(value, &field_path("height"))?).map_err(|_| {
        capture_error_v1(
            "profile_preview_manifest_rect",
            format!("preview rectangle {}.height exceeds u32", path.join(".")),
        )
    })?;
    let rect = MtgoSignedRectDesktopPxV1 {
        left,
        top,
        width,
        height,
    };
    validate_signed_rect_v1("profile_preview_manifest_rect", &rect)?;
    let computed_right = i64::from(left) + i64::from(width);
    let computed_bottom = i64::from(top) + i64::from(height);
    if right != computed_right || bottom != computed_bottom {
        return Err(capture_error_v1(
            "profile_preview_manifest_rect",
            format!(
                "preview rectangle {} has inconsistent edges and dimensions",
                path.join(".")
            ),
        ));
    }
    Ok(rect)
}

fn require_json_string_v1(
    value: &Value,
    path: &[&str],
    expected: &str,
) -> Result<(), MtgoContractErrorV1> {
    if json_string_v1(value, path)? != expected {
        return Err(capture_error_v1(
            "profile_preview_manifest_value",
            format!(
                "preview manifest field {} has an unexpected value",
                path.join(".")
            ),
        ));
    }
    Ok(())
}

fn require_json_hex_case_insensitive_v1(
    value: &Value,
    path: &[&str],
    expected_lowercase: &str,
) -> Result<(), MtgoContractErrorV1> {
    let actual = json_string_v1(value, path)?;
    if !actual.eq_ignore_ascii_case(expected_lowercase) {
        return Err(capture_error_v1(
            "profile_preview_manifest_value",
            format!(
                "preview manifest field {} has an unexpected digest",
                path.join(".")
            ),
        ));
    }
    Ok(())
}

fn require_json_u64_v1(
    value: &Value,
    path: &[&str],
    expected: u64,
) -> Result<(), MtgoContractErrorV1> {
    if json_u64_v1(value, path)? != expected {
        return Err(capture_error_v1(
            "profile_preview_manifest_value",
            format!(
                "preview manifest field {} has an unexpected integer",
                path.join(".")
            ),
        ));
    }
    Ok(())
}

fn require_json_bool_v1(
    value: &Value,
    path: &[&str],
    expected: bool,
) -> Result<(), MtgoContractErrorV1> {
    if json_bool_v1(value, path)? != expected {
        return Err(capture_error_v1(
            "profile_preview_manifest_value",
            format!(
                "preview manifest field {} has an unexpected boolean",
                path.join(".")
            ),
        ));
    }
    Ok(())
}

fn validate_live_anchors_v1(
    profile: &MtgoCalibrationProfilePayloadV1,
    pixels: &[u8],
) -> Result<(), MtgoContractErrorV1> {
    let row_stride = usize::try_from(profile.client_size_px.width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or_else(|| capture_error_v1("anchor_stride", "client row stride overflow"))?;
    for anchor in &profile.anchors {
        let x = usize::try_from(anchor.rect_client_px.x)
            .map_err(|_| capture_error_v1("anchor_crop", "anchor x does not fit usize"))?;
        let y = usize::try_from(anchor.rect_client_px.y)
            .map_err(|_| capture_error_v1("anchor_crop", "anchor y does not fit usize"))?;
        let width = usize::try_from(anchor.rect_client_px.width)
            .map_err(|_| capture_error_v1("anchor_crop", "anchor width does not fit usize"))?;
        let height = usize::try_from(anchor.rect_client_px.height)
            .map_err(|_| capture_error_v1("anchor_crop", "anchor height does not fit usize"))?;
        let row_bytes = width
            .checked_mul(4)
            .ok_or_else(|| capture_error_v1("anchor_crop", "anchor row length overflow"))?;
        let x_bytes = x
            .checked_mul(4)
            .ok_or_else(|| capture_error_v1("anchor_crop", "anchor x offset overflow"))?;
        let mut hasher = Sha256::new();
        for row in y..(y + height) {
            let start = row
                .checked_mul(row_stride)
                .and_then(|offset| offset.checked_add(x_bytes))
                .ok_or_else(|| capture_error_v1("anchor_crop", "anchor crop offset overflow"))?;
            let end = start
                .checked_add(row_bytes)
                .ok_or_else(|| capture_error_v1("anchor_crop", "anchor crop end overflow"))?;
            let slice = pixels.get(start..end).ok_or_else(|| {
                capture_error_v1("anchor_crop", "anchor crop is outside the frame")
            })?;
            hasher.update(slice);
        }
        let actual = format!("{:x}", hasher.finalize());
        if actual != anchor.reference_bgra8_sha256 {
            return Err(capture_error_v1(
                "real_frame_anchor_mismatch",
                format!(
                    "anchor {} does not match the reviewed pixels",
                    anchor.anchor_id
                ),
            ));
        }
    }
    Ok(())
}

fn validate_size_v1(field: &'static str, size: &MtgoSizePxV1) -> Result<(), MtgoContractErrorV1> {
    if size.width == 0
        || size.height == 0
        || size.width > MAX_CLIENT_DIMENSION_V1
        || size.height > MAX_CLIENT_DIMENSION_V1
    {
        return Err(capture_error_v1(
            field,
            format!("dimensions must be between 1 and {MAX_CLIENT_DIMENSION_V1}"),
        ));
    }
    canonical_byte_len_v1(size)?;
    Ok(())
}

fn canonical_byte_len_v1(size: &MtgoSizePxV1) -> Result<usize, MtgoContractErrorV1> {
    let bytes = usize::try_from(size.width)
        .ok()
        .and_then(|width| {
            usize::try_from(size.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| capture_error_v1("canonical_frame_size", "canonical frame size overflow"))?;
    if bytes > MAX_CANONICAL_FRAME_BYTES_V1 {
        return Err(capture_error_v1(
            "canonical_frame_size",
            "canonical frame exceeds the v1 byte limit",
        ));
    }
    Ok(bytes)
}

fn validate_local_rect_v1(
    rect: &MtgoRectPxV1,
    size: &MtgoSizePxV1,
) -> Result<(), MtgoContractErrorV1> {
    if rect.width == 0 || rect.height == 0 {
        return Err(capture_error_v1(
            "profile_anchor_rect",
            "anchor rectangle must be nonempty",
        ));
    }
    let right = u64::from(rect.x) + u64::from(rect.width);
    let bottom = u64::from(rect.y) + u64::from(rect.height);
    if right > u64::from(size.width) || bottom > u64::from(size.height) {
        return Err(capture_error_v1(
            "profile_anchor_rect",
            "anchor rectangle must be inside the client image",
        ));
    }
    Ok(())
}

fn local_rects_intersect_v1(first: &MtgoRectPxV1, second: &MtgoRectPxV1) -> bool {
    let first_right = u64::from(first.x) + u64::from(first.width);
    let first_bottom = u64::from(first.y) + u64::from(first.height);
    let second_right = u64::from(second.x) + u64::from(second.width);
    let second_bottom = u64::from(second.y) + u64::from(second.height);
    u64::from(first.x) < second_right
        && first_right > u64::from(second.x)
        && u64::from(first.y) < second_bottom
        && first_bottom > u64::from(second.y)
}

fn validate_signed_rect_v1(
    field: &'static str,
    rect: &MtgoSignedRectDesktopPxV1,
) -> Result<(), MtgoContractErrorV1> {
    if rect.width == 0 || rect.height == 0 {
        return Err(capture_error_v1(
            field,
            "desktop rectangle must be nonempty",
        ));
    }
    let right = i64::from(rect.left) + i64::from(rect.width);
    let bottom = i64::from(rect.top) + i64::from(rect.height);
    if right > i64::from(i32::MAX) || bottom > i64::from(i32::MAX) {
        return Err(capture_error_v1(
            field,
            "desktop rectangle endpoint exceeds signed coordinates",
        ));
    }
    Ok(())
}

fn signed_rect_contains_v1(
    outer: &MtgoSignedRectDesktopPxV1,
    inner: &MtgoSignedRectDesktopPxV1,
) -> bool {
    let outer_right = i64::from(outer.left) + i64::from(outer.width);
    let outer_bottom = i64::from(outer.top) + i64::from(outer.height);
    let inner_right = i64::from(inner.left) + i64::from(inner.width);
    let inner_bottom = i64::from(inner.top) + i64::from(inner.height);
    i64::from(inner.left) >= i64::from(outer.left)
        && i64::from(inner.top) >= i64::from(outer.top)
        && inner_right <= outer_right
        && inner_bottom <= outer_bottom
}

fn validate_identifier_v1(
    field: &'static str,
    value: &str,
    max_len: usize,
) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > max_len
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(capture_error_v1(
            field,
            "value must be a bounded ASCII identifier",
        ));
    }
    Ok(())
}

fn validate_lower_sha256_v1(field: &'static str, value: &str) -> Result<(), MtgoContractErrorV1> {
    validate_lower_hex_v1(field, value, 64)
}

fn validate_lower_hex_v1(
    field: &'static str,
    value: &str,
    expected_len: usize,
) -> Result<(), MtgoContractErrorV1> {
    if value.len() != expected_len
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(capture_error_v1(
            field,
            format!("value must be exactly {expected_len} lowercase hexadecimal characters"),
        ));
    }
    Ok(())
}

pub(crate) fn preview_utc_instant_v1(
    field: &'static str,
    value: &str,
) -> Result<PreviewUtcInstantV1, MtgoContractErrorV1> {
    let bytes = value.as_bytes();
    if bytes.len() != 28
        || bytes[19] != b'.'
        || bytes[27] != b'Z'
        || !bytes[20..27].iter().all(u8::is_ascii_digit)
    {
        return Err(capture_error_v1(
            field,
            "timestamp must use PowerShell round-trip UTC precision: YYYY-MM-DDTHH:MM:SS.fffffffZ",
        ));
    }
    let whole_seconds = format!("{}Z", &value[..19]);
    let epoch_seconds = utc_epoch_seconds_v1(field, &whole_seconds)?;
    let fractional_100ns = bytes[20..27].iter().fold(0_u32, |fraction, byte| {
        fraction * 10 + u32::from(byte - b'0')
    });
    Ok(PreviewUtcInstantV1 {
        epoch_seconds,
        fractional_100ns,
    })
}

fn utc_epoch_seconds_v1(field: &'static str, value: &str) -> Result<i64, MtgoContractErrorV1> {
    let bytes = value.as_bytes();
    let separators = [
        (4, b'-'),
        (7, b'-'),
        (10, b'T'),
        (13, b':'),
        (16, b':'),
        (19, b'Z'),
    ];
    if bytes.len() != 20
        || separators
            .iter()
            .any(|(index, byte)| bytes[*index] != *byte)
        || bytes.iter().enumerate().any(|(index, byte)| {
            !separators
                .iter()
                .any(|(separator_index, _)| *separator_index == index)
                && !byte.is_ascii_digit()
        })
    {
        return Err(capture_error_v1(
            field,
            "timestamp must be UTC at whole-second precision: YYYY-MM-DDTHH:MM:SSZ",
        ));
    }
    let parse_decimal = |start: usize, len: usize| -> u32 {
        bytes[start..(start + len)]
            .iter()
            .fold(0_u32, |value, byte| value * 10 + u32::from(byte - b'0'))
    };
    let year = parse_decimal(0, 4);
    let month = parse_decimal(5, 2);
    let day = parse_decimal(8, 2);
    let hour = parse_decimal(11, 2);
    let minute = parse_decimal(14, 2);
    let second = parse_decimal(17, 2);
    let leap_year =
        year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year => 29,
        2 => 28,
        _ => 0,
    };
    if year < 2020
        || max_day == 0
        || day == 0
        || day > max_day
        || hour > 23
        || minute > 59
        || second > 59
    {
        return Err(capture_error_v1(
            field,
            "timestamp contains an invalid UTC calendar value",
        ));
    }
    let year_adjusted = i64::from(year) - i64::from(month <= 2);
    let era = year_adjusted.div_euclid(400);
    let year_of_era = year_adjusted - era * 400;
    let month_prime = i64::from(month) + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days_since_epoch = era * 146_097 + day_of_era - 719_468;
    Ok(days_since_epoch * 86_400
        + i64::from(hour) * 3_600
        + i64::from(minute) * 60
        + i64::from(second))
}

fn canonical_struct_commitment_v1<T: Serialize>(
    domain: &[u8],
    value: &T,
    error_code: &'static str,
) -> Result<String, MtgoContractErrorV1> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| capture_error_v1(error_code, error.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn sha256_bytes_v1(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn capture_error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod reviewed_preview_admission_tests_v1 {
    use super::*;

    fn private_ratified_fixture_v1() -> (
        CheckedUntrustedMtgoCalibrationProfileV1,
        Vec<u8>,
        Vec<u8>,
        Box<[u8]>,
        String,
    ) {
        let canonical_bgra8 = vec![30, 20, 10, 255].into_boxed_slice();
        let mut frame_png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut frame_png, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[10, 20, 30, 255]).unwrap();
        }
        let manifest = b"private-reviewed-preview-fixture-v1".to_vec();
        let payload = MtgoCalibrationProfilePayloadV1 {
            schema_version: MTGO_CALIBRATION_PROFILE_SCHEMA_V1,
            profile_id: "private-reviewed-preview-fixture-v1".to_owned(),
            source_preview_manifest_sha256: sha256_bytes_v1(&manifest),
            source_preview_frame_sha256: sha256_bytes_v1(&frame_png),
            source_preview_canonical_bgra8_sha256: sha256_bytes_v1(&canonical_bgra8),
            product_version: "test".to_owned(),
            file_version: "test".to_owned(),
            executable_sha256: "1".repeat(64),
            signer_thumbprint: "2".repeat(40),
            signer_subject_sha256: "3".repeat(64),
            window_title_sha256: "4".repeat(64),
            dpi: 96,
            client_size_px: MtgoSizePxV1 {
                width: 1,
                height: 1,
            },
            output_identity_sha256: "5".repeat(64),
            output_device_name_sha256: "6".repeat(64),
            output_bounds_desktop_px: MtgoSignedRectDesktopPxV1 {
                left: 0,
                top: 0,
                width: 1,
                height: 1,
            },
            canonical_pixel_format: MtgoCanonicalPixelFormatV1::Bgra8UnormTopDownTightlyPackedV1,
            anchors: Vec::new(),
        };
        let profile_sha256 = calibration_profile_commitment_v1(&payload).unwrap();
        let review_sha256 = sha256_bytes_v1(b"private-reviewed-review-fixture-v1");
        let admission_commitment = reviewed_preview_admission_commitment_v1(
            &profile_sha256,
            &review_sha256,
            &payload.source_preview_manifest_sha256,
            &payload.source_preview_frame_sha256,
            &payload.source_preview_canonical_bgra8_sha256,
        )
        .unwrap();
        let checked = CheckedUntrustedMtgoCalibrationProfileV1 {
            payload,
            profile_sha256,
            review_sha256,
            reviewed_at_epoch_seconds: 0,
        };
        (
            checked,
            manifest,
            frame_png,
            canonical_bgra8,
            admission_commitment,
        )
    }

    #[test]
    fn private_test_ratification_admits_only_preview_scope() {
        let (checked, manifest, frame_png, canonical_bgra8, ratification) =
            private_ratified_fixture_v1();
        let admitted = admit_reviewed_preview_for_test_v1(
            checked,
            &manifest,
            &frame_png,
            canonical_bgra8,
            &ratification,
        )
        .unwrap();
        assert_eq!(
            admitted.scope(),
            MtgoReviewedPreviewScopeV1::OfflineCalibrationPreviewOnlyV1
        );
        assert_eq!(admitted.admission_commitment_sha256(), ratification);
    }
}
