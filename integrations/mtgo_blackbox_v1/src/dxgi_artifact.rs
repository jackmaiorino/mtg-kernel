use crate::{
    decode_preview_png_to_canonical_bgra8_v1, preview_output_identity_commitment_v1,
    MtgoContractErrorV1, MtgoSignedRectDesktopPxV1, MtgoSizePxV1,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const DXGI_ARTIFACT_SCHEMA_V1: &str = "mtgo-dxgi-visible-frame-candidate/v1";
const DXGI_ARTIFACT_KIND_V1: &str = "mtgo_untrusted_dxgi_visible_frame_candidate_v1";
const DXGI_ARTIFACT_STATUS_V1: &str = "checked_untrusted_not_admitted";
const DXGI_CAPTURE_BACKEND_V1: &str = "dxgi_desktop_duplication_v1";
const CANONICAL_PIXELS_FILE_V1: &str = "frame.bgra";
const PREVIEW_PNG_FILE_V1: &str = "frame.png";
const MANIFEST_FILE_V1: &str = "manifest.json";
const DXGI_FORMAT_B8G8R8A8_UNORM_V1: i32 = 87;
const DXGI_MODE_ROTATION_IDENTITY_V1: i32 = 1;
const DXGI_COLOR_SPACE_SDR_SRGB_V1: i32 = 0;
const MAX_CLIENT_DIMENSION_V1: u32 = 16_384;
const MAX_OUTPUT_DIMENSION_V1: u32 = 32_768;
const MAX_CANONICAL_BYTES_V1: usize = 512 * 1_048_576;
const MAX_MANIFEST_BYTES_V1: usize = 1_048_576;
const MAX_PREVIEW_PNG_BYTES_V1: usize = 512 * 1_048_576;
const MIN_CAPTURE_UNIX_MILLIS_V1: u64 = 1_577_836_800_000;
const MAX_CAPTURE_UNIX_MILLIS_V1: u64 = 4_102_444_800_000;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct DxgiRectV1 {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct DxgiWindowSnapshotV1 {
    hwnd: u64,
    process_id: u32,
    mtgo_process_count: u32,
    process_start_filetime_100ns: u64,
    process_image: String,
    executable_sha256: String,
    authenticode_valid: bool,
    signer_thumbprint: String,
    signer_subject: String,
    signer_subject_sha256: String,
    title: String,
    dpi: u32,
    client_rect_desktop_px: DxgiRectV1,
    extended_frame_rect_desktop_px: DxgiRectV1,
    foreground: bool,
    visible: bool,
    minimized: bool,
    cloaked: bool,
    hung: bool,
    display_affinity: u32,
    desktop_composition_enabled: bool,
    cursor_showing: bool,
    cursor_x: i32,
    cursor_y: i32,
    cursor_inside_client: bool,
    occlusion_target_found: bool,
    occluding_windows_above: u32,
    z_order_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct DxgiOutputIdentityV1 {
    adapter_index: u32,
    output_index: u32,
    adapter_luid_low: u32,
    adapter_luid_high: i32,
    device_name: String,
    bounds_desktop_px: DxgiRectV1,
    rotation: i32,
    color_space: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct DxgiFrameMetadataV1 {
    last_present_time_qpc: i64,
    last_mouse_update_time_qpc: i64,
    accumulated_frames: u32,
    protected_content_masked_out: bool,
    pointer_visible: bool,
    pointer_x: i32,
    pointer_y: i32,
    source_texture_width: u32,
    source_texture_height: u32,
    source_texture_format: i32,
    canonical_width: u32,
    canonical_height: u32,
    canonical_stride: u32,
    canonical_byte_length: usize,
    canonical_bgra8_sha256: String,
    preview_png_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct DxgiSafetyFlagsV1 {
    safe_for_semantic_evidence: bool,
    safe_for_ocr: bool,
    safe_for_policy_scoring: bool,
    safe_for_input: bool,
    authenticode_verified_in_probe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct DxgiFilesV1 {
    canonical_pixels: String,
    preview_png: String,
    manifest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoDxgiCaptureArtifactManifestV1 {
    schema: String,
    artifact_kind: String,
    status: String,
    capture_backend: String,
    captured_at_unix_millis: u64,
    safety: DxgiSafetyFlagsV1,
    pre: DxgiWindowSnapshotV1,
    post: DxgiWindowSnapshotV1,
    output: DxgiOutputIdentityV1,
    frame: DxgiFrameMetadataV1,
    files: DxgiFilesV1,
}

/// A complete probe artifact whose producer claims and bytes are structurally checked.
///
/// This is not a trusted capture attestation. The type retains no pixels and grants
/// no OCR, semantic-evidence, policy-scoring, action, or input authority. It also
/// intentionally implements neither `Debug` nor `Clone` and is not serializable.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoDxgiCaptureArtifactV1;
/// fn pixel_escape(value: &CheckedUntrustedMtgoDxgiCaptureArtifactV1) {
///     let _ = value.canonical_pixels_for_ocr_v1();
/// }
/// ```
pub struct CheckedUntrustedMtgoDxgiCaptureArtifactV1 {
    manifest_sha256: String,
    canonical_bgra8_sha256: String,
    preview_png_sha256: String,
    output_identity_sha256: String,
    client_size_px: MtgoSizePxV1,
    captured_at_unix_millis: u64,
}

impl CheckedUntrustedMtgoDxgiCaptureArtifactV1 {
    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }

    pub fn canonical_bgra8_sha256(&self) -> &str {
        &self.canonical_bgra8_sha256
    }

    pub fn preview_png_sha256(&self) -> &str {
        &self.preview_png_sha256
    }

    pub fn output_identity_sha256(&self) -> &str {
        &self.output_identity_sha256
    }

    pub fn client_size_px(&self) -> &MtgoSizePxV1 {
        &self.client_size_px
    }

    pub fn captured_at_unix_millis(&self) -> u64 {
        self.captured_at_unix_millis
    }

    pub fn safe_for_semantic_evidence(&self) -> bool {
        false
    }

    pub fn safe_for_ocr(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

/// Structurally checks one complete output from `mtgo_dxgi_capture_v1`.
///
/// Success proves only that the supplied manifest is internally consistent with
/// the supplied raw BGRA8 and PNG bytes. Caller-controlled live assertions remain
/// untrusted, and no runtime authority is created.
pub fn check_untrusted_dxgi_capture_artifact_v1(
    manifest_bytes: &[u8],
    canonical_bgra8: &[u8],
    preview_png_bytes: &[u8],
) -> Result<CheckedUntrustedMtgoDxgiCaptureArtifactV1, MtgoContractErrorV1> {
    if manifest_bytes.is_empty() || manifest_bytes.len() > MAX_MANIFEST_BYTES_V1 {
        return Err(error_v1(
            "dxgi_artifact_manifest_size",
            "manifest must be between 1 byte and 1 MiB",
        ));
    }
    if preview_png_bytes.is_empty() || preview_png_bytes.len() > MAX_PREVIEW_PNG_BYTES_V1 {
        return Err(error_v1(
            "dxgi_artifact_png_size",
            "preview PNG must be between 1 byte and 512 MiB",
        ));
    }
    let manifest: MtgoDxgiCaptureArtifactManifestV1 = serde_json::from_slice(manifest_bytes)
        .map_err(|error| error_v1("dxgi_artifact_manifest", error.to_string()))?;

    validate_header_v1(&manifest)?;
    if manifest.pre != manifest.post {
        return Err(error_v1(
            "dxgi_artifact_snapshot_drift",
            "pre and post window snapshots must be identical",
        ));
    }
    validate_window_snapshot_v1(&manifest.pre)?;
    let output_rect = validate_output_v1(&manifest.output, &manifest.pre)?;
    let client_size = validate_frame_v1(
        &manifest.frame,
        &manifest.pre.client_rect_desktop_px,
        &output_rect,
        canonical_bgra8,
        preview_png_bytes,
    )?;

    if manifest.files.canonical_pixels != CANONICAL_PIXELS_FILE_V1
        || manifest.files.preview_png != PREVIEW_PNG_FILE_V1
        || manifest.files.manifest != MANIFEST_FILE_V1
    {
        return Err(error_v1(
            "dxgi_artifact_files",
            "artifact file names must be frame.bgra, frame.png, and manifest.json",
        ));
    }

    let output_identity_sha256 =
        preview_output_identity_commitment_v1(&manifest.output.device_name, &output_rect)?;
    Ok(CheckedUntrustedMtgoDxgiCaptureArtifactV1 {
        manifest_sha256: sha256_v1(manifest_bytes),
        canonical_bgra8_sha256: sha256_v1(canonical_bgra8),
        preview_png_sha256: sha256_v1(preview_png_bytes),
        output_identity_sha256,
        client_size_px: client_size,
        captured_at_unix_millis: manifest.captured_at_unix_millis,
    })
}

fn validate_header_v1(
    manifest: &MtgoDxgiCaptureArtifactManifestV1,
) -> Result<(), MtgoContractErrorV1> {
    if manifest.schema != DXGI_ARTIFACT_SCHEMA_V1
        || manifest.artifact_kind != DXGI_ARTIFACT_KIND_V1
        || manifest.status != DXGI_ARTIFACT_STATUS_V1
        || manifest.capture_backend != DXGI_CAPTURE_BACKEND_V1
    {
        return Err(error_v1(
            "dxgi_artifact_header",
            "schema, kind, status, and backend must identify the checked-untrusted DXGI v1 artifact",
        ));
    }
    if manifest.captured_at_unix_millis < MIN_CAPTURE_UNIX_MILLIS_V1
        || manifest.captured_at_unix_millis > MAX_CAPTURE_UNIX_MILLIS_V1
    {
        return Err(error_v1(
            "dxgi_artifact_capture_time",
            "capture time must be a plausible Unix millisecond timestamp",
        ));
    }
    if manifest.safety.safe_for_semantic_evidence
        || manifest.safety.safe_for_ocr
        || manifest.safety.safe_for_policy_scoring
        || manifest.safety.safe_for_input
        || !manifest.safety.authenticode_verified_in_probe
    {
        return Err(error_v1(
            "dxgi_artifact_safety_flags",
            "all runtime-use flags must be false and probe Authenticode verification must be true",
        ));
    }
    Ok(())
}

fn validate_window_snapshot_v1(snapshot: &DxgiWindowSnapshotV1) -> Result<(), MtgoContractErrorV1> {
    if snapshot.hwnd == 0
        || snapshot.process_id == 0
        || snapshot.mtgo_process_count != 1
        || snapshot.process_start_filetime_100ns == 0
    {
        return Err(error_v1(
            "dxgi_artifact_process_identity",
            "window, process, start time, and unique MTGO process identity are required",
        ));
    }
    let normalized_image = snapshot
        .process_image
        .replace('/', "\\")
        .to_ascii_lowercase();
    if snapshot.process_image.is_empty()
        || snapshot.process_image.len() > 32_768
        || snapshot.process_image.chars().any(char::is_control)
        || !looks_like_absolute_windows_path_v1(&snapshot.process_image)
        || !normalized_image.ends_with("\\mtgo.exe")
    {
        return Err(error_v1(
            "dxgi_artifact_process_image",
            "process image must be a visible absolute path ending in MTGO.exe",
        ));
    }
    validate_lower_hex_v1(
        "dxgi_artifact_executable_sha256",
        &snapshot.executable_sha256,
        64,
    )?;
    validate_lower_hex_v1(
        "dxgi_artifact_signer_thumbprint",
        &snapshot.signer_thumbprint,
        40,
    )?;
    validate_lower_hex_v1(
        "dxgi_artifact_signer_subject_sha256",
        &snapshot.signer_subject_sha256,
        64,
    )?;
    if !snapshot.authenticode_valid
        || snapshot.signer_subject.is_empty()
        || snapshot.signer_subject.len() > 1_024
        || snapshot.signer_subject.chars().any(char::is_control)
        || sha256_v1(snapshot.signer_subject.as_bytes()) != snapshot.signer_subject_sha256
    {
        return Err(error_v1(
            "dxgi_artifact_signature",
            "valid Authenticode and an exact signer subject commitment are required",
        ));
    }
    if snapshot.title.is_empty()
        || snapshot.title.len() > 1_024
        || snapshot.title.chars().any(char::is_control)
        || !snapshot.title.contains("Magic: The Gathering Online")
    {
        return Err(error_v1(
            "dxgi_artifact_window_title",
            "visible window title must identify Magic: The Gathering Online",
        ));
    }
    if !(96..=480).contains(&snapshot.dpi) {
        return Err(error_v1(
            "dxgi_artifact_dpi",
            "window DPI must be between 96 and 480",
        ));
    }
    let client = validate_rect_v1(
        "dxgi_artifact_client_rect",
        &snapshot.client_rect_desktop_px,
        MAX_CLIENT_DIMENSION_V1,
    )?;
    let extended = validate_rect_v1(
        "dxgi_artifact_extended_frame_rect",
        &snapshot.extended_frame_rect_desktop_px,
        MAX_OUTPUT_DIMENSION_V1,
    )?;
    if !rect_contains_v1(&extended, &client) {
        return Err(error_v1(
            "dxgi_artifact_window_geometry",
            "extended frame must contain the client rectangle",
        ));
    }
    if !snapshot.foreground
        || !snapshot.visible
        || snapshot.minimized
        || snapshot.cloaked
        || snapshot.hung
        || snapshot.display_affinity != 0
        || !snapshot.desktop_composition_enabled
        || snapshot.cursor_inside_client
        || !snapshot.occlusion_target_found
        || snapshot.occluding_windows_above != 0
    {
        return Err(error_v1(
            "dxgi_artifact_visible_window_safety",
            "foreground, visible, responsive, composed, unobscured client state is required",
        ));
    }
    if snapshot.cursor_inside_client
        || (snapshot.cursor_showing
            && rect_contains_point_v1(&client, snapshot.cursor_x, snapshot.cursor_y))
    {
        return Err(error_v1(
            "dxgi_artifact_cursor",
            "visible cursor must remain outside the client rectangle",
        ));
    }
    validate_lower_hex_v1("dxgi_artifact_z_order_sha256", &snapshot.z_order_sha256, 64)?;
    Ok(())
}

fn validate_output_v1(
    output: &DxgiOutputIdentityV1,
    snapshot: &DxgiWindowSnapshotV1,
) -> Result<MtgoSignedRectDesktopPxV1, MtgoContractErrorV1> {
    let output_rect = validate_rect_v1(
        "dxgi_artifact_output_bounds",
        &output.bounds_desktop_px,
        MAX_OUTPUT_DIMENSION_V1,
    )?;
    let client_rect = validate_rect_v1(
        "dxgi_artifact_client_rect",
        &snapshot.client_rect_desktop_px,
        MAX_CLIENT_DIMENSION_V1,
    )?;
    if output.device_name.is_empty()
        || output.device_name.len() > 256
        || output.device_name.chars().any(char::is_control)
        || !looks_like_display_device_v1(&output.device_name)
        || !rect_contains_v1(&output_rect, &client_rect)
        || output.rotation != DXGI_MODE_ROTATION_IDENTITY_V1
        || output.color_space != DXGI_COLOR_SPACE_SDR_SRGB_V1
    {
        return Err(error_v1(
            "dxgi_artifact_output_identity",
            "one named identity-rotation SDR output must wholly contain the client",
        ));
    }
    Ok(output_rect)
}

fn validate_frame_v1(
    frame: &DxgiFrameMetadataV1,
    client_ltrb: &DxgiRectV1,
    output_rect: &MtgoSignedRectDesktopPxV1,
    canonical_bgra8: &[u8],
    preview_png_bytes: &[u8],
) -> Result<MtgoSizePxV1, MtgoContractErrorV1> {
    if frame.last_present_time_qpc <= 0
        || frame.last_mouse_update_time_qpc < 0
        || frame.accumulated_frames == 0
        || frame.protected_content_masked_out
    {
        return Err(error_v1(
            "dxgi_artifact_desktop_present",
            "a current desktop presentation without protected-content masking is required",
        ));
    }
    let client_rect = validate_rect_v1(
        "dxgi_artifact_client_rect",
        client_ltrb,
        MAX_CLIENT_DIMENSION_V1,
    )?;
    if frame.pointer_visible
        && rect_contains_point_v1(&client_rect, frame.pointer_x, frame.pointer_y)
    {
        return Err(error_v1(
            "dxgi_artifact_pointer",
            "DXGI pointer must remain outside the client crop",
        ));
    }
    if frame.source_texture_width != output_rect.width
        || frame.source_texture_height != output_rect.height
        || frame.source_texture_format != DXGI_FORMAT_B8G8R8A8_UNORM_V1
        || frame.canonical_width != client_rect.width
        || frame.canonical_height != client_rect.height
    {
        return Err(error_v1(
            "dxgi_artifact_frame_geometry",
            "source and canonical texture geometry must match the checked output and client",
        ));
    }
    let expected_stride = frame
        .canonical_width
        .checked_mul(4)
        .ok_or_else(|| error_v1("dxgi_artifact_frame_size", "canonical stride overflow"))?;
    let expected_len = usize::try_from(expected_stride)
        .ok()
        .and_then(|stride| {
            usize::try_from(frame.canonical_height)
                .ok()
                .and_then(|height| stride.checked_mul(height))
        })
        .filter(|length| *length <= MAX_CANONICAL_BYTES_V1)
        .ok_or_else(|| error_v1("dxgi_artifact_frame_size", "canonical byte length overflow"))?;
    if frame.canonical_stride != expected_stride
        || frame.canonical_byte_length != expected_len
        || canonical_bgra8.len() != expected_len
    {
        return Err(error_v1(
            "dxgi_artifact_frame_size",
            "canonical stride and byte length must be exact tightly packed BGRA8",
        ));
    }
    validate_lower_hex_v1(
        "dxgi_artifact_canonical_sha256",
        &frame.canonical_bgra8_sha256,
        64,
    )?;
    validate_lower_hex_v1("dxgi_artifact_png_sha256", &frame.preview_png_sha256, 64)?;
    if sha256_v1(canonical_bgra8) != frame.canonical_bgra8_sha256
        || sha256_v1(preview_png_bytes) != frame.preview_png_sha256
    {
        return Err(error_v1(
            "dxgi_artifact_file_hash",
            "raw BGRA8 or PNG hash does not match the supplied bytes",
        ));
    }
    let size = MtgoSizePxV1 {
        width: frame.canonical_width,
        height: frame.canonical_height,
    };
    let decoded = decode_preview_png_to_canonical_bgra8_v1(preview_png_bytes, &size, expected_len)?;
    if decoded != canonical_bgra8 {
        return Err(error_v1(
            "dxgi_artifact_png_pixel_mismatch",
            "PNG must decode to the exact canonical BGRA8 artifact",
        ));
    }
    Ok(size)
}

fn validate_rect_v1(
    code: &'static str,
    rect: &DxgiRectV1,
    max_dimension: u32,
) -> Result<MtgoSignedRectDesktopPxV1, MtgoContractErrorV1> {
    let width = i64::from(rect.right) - i64::from(rect.left);
    let height = i64::from(rect.bottom) - i64::from(rect.top);
    let width = u32::try_from(width)
        .ok()
        .filter(|width| *width > 0 && *width <= max_dimension)
        .ok_or_else(|| error_v1(code, "rectangle width is invalid"))?;
    let height = u32::try_from(height)
        .ok()
        .filter(|height| *height > 0 && *height <= max_dimension)
        .ok_or_else(|| error_v1(code, "rectangle height is invalid"))?;
    Ok(MtgoSignedRectDesktopPxV1 {
        left: rect.left,
        top: rect.top,
        width,
        height,
    })
}

fn rect_contains_v1(outer: &MtgoSignedRectDesktopPxV1, inner: &MtgoSignedRectDesktopPxV1) -> bool {
    let outer_right = i64::from(outer.left) + i64::from(outer.width);
    let outer_bottom = i64::from(outer.top) + i64::from(outer.height);
    let inner_right = i64::from(inner.left) + i64::from(inner.width);
    let inner_bottom = i64::from(inner.top) + i64::from(inner.height);
    i64::from(inner.left) >= i64::from(outer.left)
        && i64::from(inner.top) >= i64::from(outer.top)
        && inner_right <= outer_right
        && inner_bottom <= outer_bottom
}

fn rect_contains_point_v1(rect: &MtgoSignedRectDesktopPxV1, x: i32, y: i32) -> bool {
    let right = i64::from(rect.left) + i64::from(rect.width);
    let bottom = i64::from(rect.top) + i64::from(rect.height);
    i64::from(x) >= i64::from(rect.left)
        && i64::from(y) >= i64::from(rect.top)
        && i64::from(x) < right
        && i64::from(y) < bottom
}

fn validate_lower_hex_v1(
    code: &'static str,
    value: &str,
    expected_len: usize,
) -> Result<(), MtgoContractErrorV1> {
    if value.len() != expected_len
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1(
            code,
            "value must be fixed-length lowercase hexadecimal",
        ));
    }
    Ok(())
}

fn looks_like_absolute_windows_path_v1(value: &str) -> bool {
    let bytes = value.as_bytes();
    (bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/'))
        || value.starts_with("\\\\")
}

fn looks_like_display_device_v1(value: &str) -> bool {
    value
        .to_ascii_uppercase()
        .strip_prefix("\\\\.\\DISPLAY")
        .is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn sha256_v1(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}
