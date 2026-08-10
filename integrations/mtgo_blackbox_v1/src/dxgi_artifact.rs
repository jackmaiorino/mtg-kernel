use crate::{
    decode_preview_png_to_canonical_bgra8_v1, preview_output_identity_commitment_v1,
    MtgoContractErrorV1, MtgoPregameActionSemanticV1, MtgoRectPxV1, MtgoSignedRectDesktopPxV1,
    MtgoSizePxV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const DXGI_ARTIFACT_SCHEMA_V1: &str = "mtgo-dxgi-visible-frame-candidate/v1";
const DXGI_ARTIFACT_KIND_V1: &str = "mtgo_untrusted_dxgi_visible_frame_candidate_v1";
const DXGI_ARTIFACT_SCHEMA_V2: &str = "mtgo-dxgi-visible-frame-candidate/v2";
const DXGI_ARTIFACT_KIND_V2: &str = "mtgo_untrusted_dxgi_visible_frame_candidate_v2";
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
const DXGI_OFFLINE_CALIBRATION_ADMISSION_DOMAIN_V1: &[u8] =
    b"mtgo-dxgi-offline-calibration-admission-v1";
const DXGI_OFFLINE_CALIBRATION_SCOPE_V1: &[u8] =
    b"offline-acting-player-solitaire-calibration-only-v1";
const DXGI_ACTING_PLAYER_SOLITAIRE_ROLE_V1: &[u8] = b"acting_player_solitaire";
const RATIFIED_DXGI_OFFLINE_CALIBRATION_ADMISSION_COMMITMENT_V1: Option<&str> =
    Some("9b0aef61a6fc31ee6050d9e381fba4c3e1a6b887c62319c8b3e1e13e9523e291");
const DXGI_KEEP_TRANSITION_SCHEMA_V1: u32 = 1;
const DXGI_KEEP_TRANSITION_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-dxgi-keep-transition-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoDxgiCaptureRoleV2 {
    Navigation,
    ActingPlayerSolitaire,
    ActingPlayerDuel,
    Spectator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DxgiArtifactModeV2 {
    MainClient,
    SolitaireGame(String),
    DuelGame(String),
    SpectatorGame(String),
}

impl DxgiArtifactModeV2 {
    fn role(&self) -> MtgoDxgiCaptureRoleV2 {
        match self {
            Self::MainClient => MtgoDxgiCaptureRoleV2::Navigation,
            Self::SolitaireGame(_) => MtgoDxgiCaptureRoleV2::ActingPlayerSolitaire,
            Self::DuelGame(_) => MtgoDxgiCaptureRoleV2::ActingPlayerDuel,
            Self::SpectatorGame(_) => MtgoDxgiCaptureRoleV2::Spectator,
        }
    }

    fn game_format(&self) -> Option<&str> {
        match self {
            Self::MainClient => None,
            Self::SolitaireGame(format) | Self::DuelGame(format) | Self::SpectatorGame(format) => {
                Some(format)
            }
        }
    }
}

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
    window_mode: Option<String>,
    capture_role: Option<String>,
    expected_game_format: Option<String>,
    title_rule_version: Option<String>,
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
    executable_sha256: String,
    signer_thumbprint: String,
    signer_subject_sha256: String,
    dpi: u32,
    client_size_px: MtgoSizePxV1,
    captured_at_unix_millis: u64,
    capture_role: MtgoDxgiCaptureRoleV2,
    game_format: Option<String>,
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

    pub fn executable_sha256(&self) -> &str {
        &self.executable_sha256
    }

    pub fn signer_thumbprint(&self) -> &str {
        &self.signer_thumbprint
    }

    pub fn signer_subject_sha256(&self) -> &str {
        &self.signer_subject_sha256
    }

    pub fn dpi(&self) -> u32 {
        self.dpi
    }

    pub fn client_size_px(&self) -> &MtgoSizePxV1 {
        &self.client_size_px
    }

    pub fn captured_at_unix_millis(&self) -> u64 {
        self.captured_at_unix_millis
    }

    pub fn capture_role(&self) -> MtgoDxgiCaptureRoleV2 {
        self.capture_role
    }

    pub fn game_format(&self) -> Option<&str> {
        self.game_format.as_deref()
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoDxgiOfflineCalibrationScopeV1 {
    ActingPlayerSolitaireOnlyV1,
}

/// One exact, manually inspected DXGI artifact admitted for offline calibration.
///
/// The source commitment is pinned privately in this crate. Runtime manifests,
/// caller booleans, and caller-selected review records cannot ratify another
/// artifact. The retained pixels are available only to crate-internal, separately
/// reviewed offline calibration code. This type grants no live-frame, semantic-
/// evidence, policy-scoring, action, or input authority.
///
/// It intentionally implements neither `Debug` nor `Clone` and is not serializable.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::AdmittedMtgoDxgiOfflineCalibrationFrameV1;
/// fn pixel_escape(value: &AdmittedMtgoDxgiOfflineCalibrationFrameV1) {
///     let _ = value.canonical_pixels_for_offline_calibration_v1();
/// }
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::AdmittedMtgoDxgiOfflineCalibrationFrameV1;
/// fn requires_debug<T: core::fmt::Debug>() {}
/// requires_debug::<AdmittedMtgoDxgiOfflineCalibrationFrameV1>();
/// ```
pub struct AdmittedMtgoDxgiOfflineCalibrationFrameV1 {
    checked: CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    admission_commitment_sha256: String,
    canonical_bgra8: Box<[u8]>,
}

impl AdmittedMtgoDxgiOfflineCalibrationFrameV1 {
    pub fn scope(&self) -> MtgoDxgiOfflineCalibrationScopeV1 {
        MtgoDxgiOfflineCalibrationScopeV1::ActingPlayerSolitaireOnlyV1
    }

    pub fn admission_commitment_sha256(&self) -> &str {
        &self.admission_commitment_sha256
    }

    pub fn manifest_sha256(&self) -> &str {
        self.checked.manifest_sha256()
    }

    pub fn canonical_bgra8_sha256(&self) -> &str {
        self.checked.canonical_bgra8_sha256()
    }

    pub fn preview_png_sha256(&self) -> &str {
        self.checked.preview_png_sha256()
    }

    pub fn output_identity_sha256(&self) -> &str {
        self.checked.output_identity_sha256()
    }

    pub fn client_size_px(&self) -> &MtgoSizePxV1 {
        self.checked.client_size_px()
    }

    pub fn captured_at_unix_millis(&self) -> u64 {
        self.checked.captured_at_unix_millis()
    }

    pub fn capture_role(&self) -> MtgoDxgiCaptureRoleV2 {
        self.checked.capture_role()
    }

    pub fn safe_for_live_ocr(&self) -> bool {
        false
    }

    pub fn safe_for_semantic_evidence(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }

    #[allow(dead_code)]
    pub(crate) fn canonical_pixels_for_offline_calibration_v1(&self) -> &[u8] {
        &self.canonical_bgra8
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDxgiArtifactReferenceV1 {
    pub manifest_sha256: String,
    pub canonical_bgra8_sha256: String,
    pub preview_png_sha256: String,
    pub output_identity_sha256: String,
    pub client_size_px: MtgoSizePxV1,
    pub captured_at_unix_millis: u64,
    pub capture_role: MtgoDxgiCaptureRoleV2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoDxgiKeepVisibleChangeV1 {
    PromptChanged,
    PlayerCountsChanged,
    VisibleGameLogChanged,
    PhaseBarChanged,
    HandChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDxgiKeepChangedRegionV1 {
    pub change: MtgoDxgiKeepVisibleChangeV1,
    pub rect_client_px: MtgoRectPxV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoDxgiKeepTransitionV1 {
    pub schema_version: u32,
    pub trace_id: String,
    pub before_frame: MtgoDxgiArtifactReferenceV1,
    pub action: MtgoPregameActionSemanticV1,
    pub action_control_before: MtgoRectPxV1,
    pub after_frame: MtgoDxgiArtifactReferenceV1,
    pub visible_postconditions: Vec<MtgoDxgiKeepChangedRegionV1>,
}

#[derive(Serialize)]
struct MtgoDxgiKeepRegionMetricV1 {
    change: MtgoDxgiKeepVisibleChangeV1,
    rect_client_px: MtgoRectPxV1,
    before_bgra8_sha256: String,
    after_bgra8_sha256: String,
    changed_pixels: u64,
    total_pixels: u64,
}

/// A byte-checked Keep calibration transition between two role-correct DXGI artifacts.
///
/// The visible region names remain manual labels. Success proves exact artifact
/// identity, byte changes in every required region, ordering, geometry, and a
/// strictly newer after-frame. It does not prove semantic recognition and grants
/// no live-frame, scoring, action, coordinate, or input authority.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoDxgiKeepTransitionV1;
/// fn pixel_escape(value: &CheckedUntrustedMtgoDxgiKeepTransitionV1) {
///     let _ = value.canonical_pixels_v1();
/// }
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoDxgiKeepTransitionV1;
/// fn coordinate_escape(value: &CheckedUntrustedMtgoDxgiKeepTransitionV1) {
///     let _ = value.action_control_before();
/// }
/// ```
pub struct CheckedUntrustedMtgoDxgiKeepTransitionV1 {
    record: MtgoDxgiKeepTransitionV1,
    transition_commitment_sha256: String,
}

impl CheckedUntrustedMtgoDxgiKeepTransitionV1 {
    pub fn action(&self) -> MtgoPregameActionSemanticV1 {
        MtgoPregameActionSemanticV1::KeepOpeningHand
    }

    pub fn before_manifest_sha256(&self) -> &str {
        &self.record.before_frame.manifest_sha256
    }

    pub fn after_manifest_sha256(&self) -> &str {
        &self.record.after_frame.manifest_sha256
    }

    pub fn changed_region_count(&self) -> usize {
        self.record.visible_postconditions.len()
    }

    pub fn transition_commitment_sha256(&self) -> &str {
        &self.transition_commitment_sha256
    }

    pub fn safe_for_semantic_evidence(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring(&self) -> bool {
        false
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

pub fn check_untrusted_dxgi_keep_transition_v1(
    record: MtgoDxgiKeepTransitionV1,
    before_checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    before_canonical_bgra8: &[u8],
    after_checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    after_canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoDxgiKeepTransitionV1, MtgoContractErrorV1> {
    if record.schema_version != DXGI_KEEP_TRANSITION_SCHEMA_V1 {
        return Err(error_v1(
            "dxgi_keep_transition_schema",
            "DXGI Keep transition schema must be version 1",
        ));
    }
    if record.trace_id.is_empty()
        || record.trace_id.len() > 128
        || !record
            .trace_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(error_v1(
            "dxgi_keep_transition_trace_id",
            "trace ID must be 1 to 128 safe identifier characters",
        ));
    }
    if !matches!(record.action, MtgoPregameActionSemanticV1::KeepOpeningHand) {
        return Err(error_v1(
            "dxgi_keep_transition_action",
            "v1 records only KeepOpeningHand",
        ));
    }
    validate_dxgi_artifact_reference_v1(&record.before_frame, before_checked)?;
    validate_dxgi_artifact_reference_v1(&record.after_frame, after_checked)?;
    if before_checked.capture_role() != MtgoDxgiCaptureRoleV2::ActingPlayerSolitaire
        || after_checked.capture_role() != MtgoDxgiCaptureRoleV2::ActingPlayerSolitaire
    {
        return Err(error_v1(
            "dxgi_keep_transition_role",
            "both artifacts must have the acting-player Solitaire role",
        ));
    }
    if before_checked.client_size_px() != after_checked.client_size_px()
        || before_checked.output_identity_sha256() != after_checked.output_identity_sha256()
    {
        return Err(error_v1(
            "dxgi_keep_transition_layout",
            "before and after artifacts must share client and output identity",
        ));
    }
    if after_checked.captured_at_unix_millis() <= before_checked.captured_at_unix_millis()
        || after_checked.manifest_sha256() == before_checked.manifest_sha256()
        || after_checked.canonical_bgra8_sha256() == before_checked.canonical_bgra8_sha256()
    {
        return Err(error_v1(
            "dxgi_keep_transition_order",
            "after artifact must be strictly newer and byte-distinct",
        ));
    }

    validate_dxgi_raw_pixels_v1(before_checked, before_canonical_bgra8)?;
    validate_dxgi_raw_pixels_v1(after_checked, after_canonical_bgra8)?;

    let expected_changes = [
        MtgoDxgiKeepVisibleChangeV1::PromptChanged,
        MtgoDxgiKeepVisibleChangeV1::PlayerCountsChanged,
        MtgoDxgiKeepVisibleChangeV1::VisibleGameLogChanged,
        MtgoDxgiKeepVisibleChangeV1::PhaseBarChanged,
        MtgoDxgiKeepVisibleChangeV1::HandChanged,
    ];
    let actual_changes: Vec<_> = record
        .visible_postconditions
        .iter()
        .map(|region| region.change)
        .collect();
    if actual_changes.as_slice() != expected_changes {
        return Err(error_v1(
            "dxgi_keep_transition_postconditions",
            "Keep requires the canonical prompt, player-counts, game-log, phase-bar, and hand changes",
        ));
    }

    let size = before_checked.client_size_px();
    validate_client_region_v1(&record.action_control_before, size)?;
    let prompt_rect = &record.visible_postconditions[0].rect_client_px;
    if !client_rect_contains_v1(prompt_rect, &record.action_control_before) {
        return Err(error_v1(
            "dxgi_keep_transition_action_control",
            "Keep control must be contained in the declared prompt region",
        ));
    }
    let action_changed = count_changed_pixels_v1(
        before_canonical_bgra8,
        after_canonical_bgra8,
        size,
        &record.action_control_before,
    )?;
    let action_total = rect_pixel_count_v1(&record.action_control_before);
    if action_changed < minimum_changed_pixels_v1(action_total, 10) {
        return Err(error_v1(
            "dxgi_keep_transition_action_control",
            "Keep control region did not change enough after the click",
        ));
    }

    let mut metrics = Vec::with_capacity(record.visible_postconditions.len());
    for (index, region) in record.visible_postconditions.iter().enumerate() {
        validate_client_region_v1(&region.rect_client_px, size)?;
        for previous in &record.visible_postconditions[..index] {
            if client_rects_overlap_v1(&previous.rect_client_px, &region.rect_client_px) {
                return Err(error_v1(
                    "dxgi_keep_transition_region_overlap",
                    "visible postcondition regions must not overlap",
                ));
            }
        }
        let changed_pixels = count_changed_pixels_v1(
            before_canonical_bgra8,
            after_canonical_bgra8,
            size,
            &region.rect_client_px,
        )?;
        let total_pixels = rect_pixel_count_v1(&region.rect_client_px);
        if changed_pixels < minimum_changed_pixels_v1(total_pixels, 1) {
            return Err(error_v1(
                "dxgi_keep_transition_region_unchanged",
                "each required visible postcondition must change by at least one percent",
            ));
        }
        metrics.push(MtgoDxgiKeepRegionMetricV1 {
            change: region.change,
            rect_client_px: region.rect_client_px.clone(),
            before_bgra8_sha256: hash_bgra_region_v1(
                before_canonical_bgra8,
                size,
                &region.rect_client_px,
            )?,
            after_bgra8_sha256: hash_bgra_region_v1(
                after_canonical_bgra8,
                size,
                &region.rect_client_px,
            )?,
            changed_pixels,
            total_pixels,
        });
    }

    let record_bytes = serde_json::to_vec(&record)
        .map_err(|error| error_v1("dxgi_keep_transition_serialization", error.to_string()))?;
    let metrics_bytes = serde_json::to_vec(&metrics)
        .map_err(|error| error_v1("dxgi_keep_transition_serialization", error.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(DXGI_KEEP_TRANSITION_COMMITMENT_DOMAIN_V1);
    for part in [record_bytes.as_slice(), metrics_bytes.as_slice()] {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }

    Ok(CheckedUntrustedMtgoDxgiKeepTransitionV1 {
        record,
        transition_commitment_sha256: format!("{:x}", hasher.finalize()),
    })
}

fn validate_dxgi_artifact_reference_v1(
    reference: &MtgoDxgiArtifactReferenceV1,
    checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
) -> Result<(), MtgoContractErrorV1> {
    if reference.manifest_sha256 != checked.manifest_sha256()
        || reference.canonical_bgra8_sha256 != checked.canonical_bgra8_sha256()
        || reference.preview_png_sha256 != checked.preview_png_sha256()
        || reference.output_identity_sha256 != checked.output_identity_sha256()
        || &reference.client_size_px != checked.client_size_px()
        || reference.captured_at_unix_millis != checked.captured_at_unix_millis()
        || reference.capture_role != checked.capture_role()
    {
        return Err(error_v1(
            "dxgi_keep_transition_artifact_reference",
            "transition artifact reference does not match the checked artifact",
        ));
    }
    Ok(())
}

fn validate_dxgi_raw_pixels_v1(
    checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    canonical_bgra8: &[u8],
) -> Result<(), MtgoContractErrorV1> {
    let expected_len = usize::try_from(checked.client_size_px().width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .and_then(|stride| {
            usize::try_from(checked.client_size_px().height)
                .ok()
                .and_then(|height| stride.checked_mul(height))
        })
        .filter(|length| *length <= MAX_CANONICAL_BYTES_V1)
        .ok_or_else(|| {
            error_v1(
                "dxgi_keep_transition_pixel_size",
                "canonical pixel size overflow",
            )
        })?;
    if canonical_bgra8.len() != expected_len
        || sha256_v1(canonical_bgra8) != checked.canonical_bgra8_sha256()
    {
        return Err(error_v1(
            "dxgi_keep_transition_pixel_identity",
            "canonical pixels do not match the checked artifact",
        ));
    }
    Ok(())
}

fn validate_client_region_v1(
    rect: &MtgoRectPxV1,
    size: &MtgoSizePxV1,
) -> Result<(), MtgoContractErrorV1> {
    let right = rect.x.checked_add(rect.width);
    let bottom = rect.y.checked_add(rect.height);
    if rect.width == 0
        || rect.height == 0
        || right.is_none_or(|right| right > size.width)
        || bottom.is_none_or(|bottom| bottom > size.height)
    {
        return Err(error_v1(
            "dxgi_keep_transition_region_geometry",
            "visible region must be nonempty and inside the client",
        ));
    }
    Ok(())
}

fn client_rect_contains_v1(outer: &MtgoRectPxV1, inner: &MtgoRectPxV1) -> bool {
    let Some(outer_right) = outer.x.checked_add(outer.width) else {
        return false;
    };
    let Some(outer_bottom) = outer.y.checked_add(outer.height) else {
        return false;
    };
    let Some(inner_right) = inner.x.checked_add(inner.width) else {
        return false;
    };
    let Some(inner_bottom) = inner.y.checked_add(inner.height) else {
        return false;
    };
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner_right <= outer_right
        && inner_bottom <= outer_bottom
}

fn client_rects_overlap_v1(left: &MtgoRectPxV1, right: &MtgoRectPxV1) -> bool {
    let Some(left_right) = left.x.checked_add(left.width) else {
        return true;
    };
    let Some(left_bottom) = left.y.checked_add(left.height) else {
        return true;
    };
    let Some(right_right) = right.x.checked_add(right.width) else {
        return true;
    };
    let Some(right_bottom) = right.y.checked_add(right.height) else {
        return true;
    };
    left.x < right_right && right.x < left_right && left.y < right_bottom && right.y < left_bottom
}

fn rect_pixel_count_v1(rect: &MtgoRectPxV1) -> u64 {
    u64::from(rect.width) * u64::from(rect.height)
}

fn minimum_changed_pixels_v1(total_pixels: u64, percentage: u64) -> u64 {
    total_pixels
        .saturating_mul(percentage)
        .div_ceil(100)
        .max(32)
        .min(total_pixels)
}

fn count_changed_pixels_v1(
    before: &[u8],
    after: &[u8],
    size: &MtgoSizePxV1,
    rect: &MtgoRectPxV1,
) -> Result<u64, MtgoContractErrorV1> {
    let mut changed = 0_u64;
    for y in rect.y..rect.y + rect.height {
        for x in rect.x..rect.x + rect.width {
            let offset = pixel_offset_v1(size, x, y)?;
            if before[offset..offset + 4] != after[offset..offset + 4] {
                changed += 1;
            }
        }
    }
    Ok(changed)
}

pub(crate) fn hash_bgra_region_v1(
    pixels: &[u8],
    size: &MtgoSizePxV1,
    rect: &MtgoRectPxV1,
) -> Result<String, MtgoContractErrorV1> {
    let mut hasher = Sha256::new();
    hasher.update(b"mtgo-dxgi-bgra-region-v1");
    hasher.update(rect.x.to_be_bytes());
    hasher.update(rect.y.to_be_bytes());
    hasher.update(rect.width.to_be_bytes());
    hasher.update(rect.height.to_be_bytes());
    for y in rect.y..rect.y + rect.height {
        let start = pixel_offset_v1(size, rect.x, y)?;
        let row_bytes = usize::try_from(rect.width)
            .ok()
            .and_then(|width| width.checked_mul(4))
            .ok_or_else(|| {
                error_v1(
                    "dxgi_keep_transition_region_geometry",
                    "visible region row size overflow",
                )
            })?;
        hasher.update(&pixels[start..start + row_bytes]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn pixel_offset_v1(size: &MtgoSizePxV1, x: u32, y: u32) -> Result<usize, MtgoContractErrorV1> {
    usize::try_from(y)
        .ok()
        .and_then(|y| {
            usize::try_from(size.width)
                .ok()
                .and_then(|width| y.checked_mul(width))
        })
        .and_then(|row| usize::try_from(x).ok().and_then(|x| row.checked_add(x)))
        .and_then(|pixel| pixel.checked_mul(4))
        .ok_or_else(|| {
            error_v1(
                "dxgi_keep_transition_region_geometry",
                "visible region pixel offset overflow",
            )
        })
}

/// Admits only the one exact DXGI artifact pinned by source review.
///
/// The admitted scope is offline acting-player Solitaire calibration. It is not
/// a live-frame attestation and cannot feed a decision, scorer, or actuator.
pub fn admit_ratified_dxgi_offline_calibration_artifact_v1(
    manifest_bytes: &[u8],
    canonical_bgra8: Box<[u8]>,
    preview_png_bytes: &[u8],
) -> Result<AdmittedMtgoDxgiOfflineCalibrationFrameV1, MtgoContractErrorV1> {
    let checked = check_untrusted_dxgi_capture_artifact_v1(
        manifest_bytes,
        &canonical_bgra8,
        preview_png_bytes,
    )?;
    if checked.capture_role() != MtgoDxgiCaptureRoleV2::ActingPlayerSolitaire {
        return Err(error_v1(
            "dxgi_offline_calibration_role",
            "only the acting-player Solitaire role can enter this calibration scope",
        ));
    }
    let ratified = RATIFIED_DXGI_OFFLINE_CALIBRATION_ADMISSION_COMMITMENT_V1.ok_or_else(|| {
        error_v1(
            "dxgi_offline_calibration_not_ratified",
            "production contains no ratified DXGI offline-calibration commitment",
        )
    })?;
    validate_lower_hex_v1("dxgi_offline_calibration_ratification", ratified, 64)?;
    let admission_commitment_sha256 = dxgi_offline_calibration_admission_commitment_v1(&checked);
    if admission_commitment_sha256 != ratified {
        return Err(error_v1(
            "dxgi_offline_calibration_not_ratified",
            "DXGI artifact does not match the ratified offline-calibration commitment",
        ));
    }

    Ok(AdmittedMtgoDxgiOfflineCalibrationFrameV1 {
        checked,
        admission_commitment_sha256,
        canonical_bgra8,
    })
}

fn dxgi_offline_calibration_admission_commitment_v1(
    checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
) -> String {
    let width = checked.client_size_px().width.to_string();
    let height = checked.client_size_px().height.to_string();
    let captured_at = checked.captured_at_unix_millis().to_string();
    let mut hasher = Sha256::new();
    hasher.update(DXGI_OFFLINE_CALIBRATION_ADMISSION_DOMAIN_V1);
    for part in [
        DXGI_OFFLINE_CALIBRATION_SCOPE_V1,
        checked.manifest_sha256().as_bytes(),
        checked.canonical_bgra8_sha256().as_bytes(),
        checked.preview_png_sha256().as_bytes(),
        checked.output_identity_sha256().as_bytes(),
        width.as_bytes(),
        height.as_bytes(),
        captured_at.as_bytes(),
        DXGI_ACTING_PLAYER_SOLITAIRE_ROLE_V1,
    ] {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
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

    let mode = validate_header_v1(&manifest)?;
    if manifest.pre != manifest.post {
        return Err(error_v1(
            "dxgi_artifact_snapshot_drift",
            "pre and post window snapshots must be identical",
        ));
    }
    validate_window_snapshot_v1(&manifest.pre, &mode)?;
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
    let game_format = mode.game_format().map(str::to_owned);
    Ok(CheckedUntrustedMtgoDxgiCaptureArtifactV1 {
        manifest_sha256: sha256_v1(manifest_bytes),
        canonical_bgra8_sha256: sha256_v1(canonical_bgra8),
        preview_png_sha256: sha256_v1(preview_png_bytes),
        output_identity_sha256,
        executable_sha256: manifest.pre.executable_sha256,
        signer_thumbprint: manifest.pre.signer_thumbprint,
        signer_subject_sha256: manifest.pre.signer_subject_sha256,
        dpi: manifest.pre.dpi,
        client_size_px: client_size,
        captured_at_unix_millis: manifest.captured_at_unix_millis,
        capture_role: mode.role(),
        game_format,
    })
}

fn validate_header_v1(
    manifest: &MtgoDxgiCaptureArtifactManifestV1,
) -> Result<DxgiArtifactModeV2, MtgoContractErrorV1> {
    if manifest.status != DXGI_ARTIFACT_STATUS_V1
        || manifest.capture_backend != DXGI_CAPTURE_BACKEND_V1
    {
        return Err(error_v1(
            "dxgi_artifact_header",
            "status and backend must identify a checked-untrusted DXGI artifact",
        ));
    }
    let mode = match manifest.schema.as_str() {
        DXGI_ARTIFACT_SCHEMA_V1 => {
            if manifest.artifact_kind != DXGI_ARTIFACT_KIND_V1
                || manifest.window_mode.is_some()
                || manifest.capture_role.is_some()
                || manifest.expected_game_format.is_some()
                || manifest.title_rule_version.is_some()
            {
                return Err(error_v1(
                    "dxgi_artifact_header",
                    "v1 is the legacy main-client artifact and cannot declare v2 role fields",
                ));
            }
            DxgiArtifactModeV2::MainClient
        }
        DXGI_ARTIFACT_SCHEMA_V2 => {
            if manifest.artifact_kind != DXGI_ARTIFACT_KIND_V2
                || manifest.title_rule_version.as_deref() != Some("mtgo_visible_title_rule_v2")
            {
                return Err(error_v1(
                    "dxgi_artifact_header",
                    "v2 kind and title-rule version must be exact",
                ));
            }
            match (
                manifest.window_mode.as_deref(),
                manifest.capture_role.as_deref(),
                manifest.expected_game_format.as_deref(),
            ) {
                (Some("main_client"), Some("navigation"), Some("")) => {
                    DxgiArtifactModeV2::MainClient
                }
                (Some("solitaire_game"), Some("acting_player_solitaire"), Some(format)) => {
                    validate_game_format_v2(format)?;
                    DxgiArtifactModeV2::SolitaireGame(format.to_owned())
                }
                (Some("duel_game"), Some("acting_player_duel"), Some(format)) => {
                    validate_game_format_v2(format)?;
                    DxgiArtifactModeV2::DuelGame(format.to_owned())
                }
                (Some("spectator_game"), Some("spectator"), Some(format)) => {
                    validate_game_format_v2(format)?;
                    DxgiArtifactModeV2::SpectatorGame(format.to_owned())
                }
                _ => {
                    return Err(error_v1(
                        "dxgi_artifact_role",
                        "window mode, capture role, and expected format must be an exact permitted tuple",
                    ));
                }
            }
        }
        _ => {
            return Err(error_v1(
                "dxgi_artifact_header",
                "schema must be the legacy v1 or role-explicit v2 artifact",
            ));
        }
    };
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
    Ok(mode)
}

fn validate_window_snapshot_v1(
    snapshot: &DxgiWindowSnapshotV1,
    mode: &DxgiArtifactModeV2,
) -> Result<(), MtgoContractErrorV1> {
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
    validate_visible_title_v2(mode, &snapshot.title)?;
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

fn validate_visible_title_v2(
    mode: &DxgiArtifactModeV2,
    title: &str,
) -> Result<(), MtgoContractErrorV1> {
    if title.is_empty() || title.len() > 1_024 || title.chars().any(char::is_control) {
        return Err(error_v1(
            "dxgi_artifact_window_title",
            "window title is empty, too long, or contains control characters",
        ));
    }
    match mode {
        DxgiArtifactModeV2::MainClient => {
            if !title.contains("Magic: The Gathering Online") {
                return Err(error_v1(
                    "dxgi_artifact_window_title",
                    "main-client title must identify Magic: The Gathering Online",
                ));
            }
        }
        DxgiArtifactModeV2::SolitaireGame(format) => {
            let prefix = format!("(Solitaire): {format}: Vs. ");
            let participant = title.strip_prefix(&prefix).ok_or_else(|| {
                error_v1(
                    "dxgi_artifact_window_title",
                    "Solitaire title does not match the exact format prefix",
                )
            })?;
            validate_participant_title_v2(participant, false)?;
        }
        DxgiArtifactModeV2::DuelGame(format) => {
            let prefix = format!("(1-on-1): {format}: Vs. ");
            let opponent = title.strip_prefix(&prefix).ok_or_else(|| {
                error_v1(
                    "dxgi_artifact_window_title",
                    "acting-player duel title does not match the exact format prefix",
                )
            })?;
            validate_participant_title_v2(opponent, false)?;
        }
        DxgiArtifactModeV2::SpectatorGame(format) => {
            let prefix = format!("(1-on-1): {format}: Vs. ");
            let participants = title.strip_prefix(&prefix).ok_or_else(|| {
                error_v1(
                    "dxgi_artifact_window_title",
                    "spectator title does not match the exact format prefix",
                )
            })?;
            validate_participant_title_v2(participants, true)?;
        }
    }
    Ok(())
}

fn validate_game_format_v2(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b' ' | b'-'))
    {
        return Err(error_v1(
            "dxgi_artifact_game_format",
            "expected game format must be a safe visible label",
        ));
    }
    Ok(())
}

fn validate_participant_title_v2(
    value: &str,
    require_comma: bool,
) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty() || value.len() > 512 {
        return Err(error_v1(
            "dxgi_artifact_window_title",
            "participant title text is empty or too long",
        ));
    }
    let participant_text = if let Some((participants, identity)) = value.split_once(" Match #") {
        let (match_id, game_id) = identity.split_once(" - Game #").ok_or_else(|| {
            error_v1(
                "dxgi_artifact_window_title",
                "visible match title suffix is malformed",
            )
        })?;
        if match_id.is_empty()
            || game_id.is_empty()
            || !match_id.bytes().all(|byte| byte.is_ascii_digit())
            || !game_id.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(error_v1(
                "dxgi_artifact_window_title",
                "visible match and game IDs must be decimal integers",
            ));
        }
        participants
    } else {
        if value.contains('#') {
            return Err(error_v1(
                "dxgi_artifact_window_title",
                "visible match title suffix is malformed",
            ));
        }
        value
    };
    if participant_text.trim() != participant_text || participant_text.is_empty() {
        return Err(error_v1(
            "dxgi_artifact_window_title",
            "participant title text has invalid surrounding whitespace",
        ));
    }
    if require_comma {
        let (left, right) = participant_text.split_once(',').ok_or_else(|| {
            error_v1(
                "dxgi_artifact_window_title",
                "spectator title must visibly identify two participants",
            )
        })?;
        if left.trim().is_empty() || right.trim().is_empty() || right.contains(',') {
            return Err(error_v1(
                "dxgi_artifact_window_title",
                "spectator title must contain exactly two visible participants",
            ));
        }
    } else if participant_text.contains(',') {
        return Err(error_v1(
            "dxgi_artifact_window_title",
            "acting-player title must identify one visible participant",
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

#[cfg(test)]
pub(crate) fn checked_untrusted_dxgi_artifact_for_test_v1(
    role: MtgoDxgiCaptureRoleV2,
) -> CheckedUntrustedMtgoDxgiCaptureArtifactV1 {
    let game_format = match role {
        MtgoDxgiCaptureRoleV2::Navigation => None,
        MtgoDxgiCaptureRoleV2::ActingPlayerSolitaire | MtgoDxgiCaptureRoleV2::ActingPlayerDuel => {
            Some("Freeform".to_owned())
        }
        MtgoDxgiCaptureRoleV2::Spectator => Some("Standard".to_owned()),
    };
    CheckedUntrustedMtgoDxgiCaptureArtifactV1 {
        manifest_sha256: "1".repeat(64),
        canonical_bgra8_sha256: "2".repeat(64),
        preview_png_sha256: "3".repeat(64),
        output_identity_sha256: "4".repeat(64),
        executable_sha256: "a".repeat(64),
        signer_thumbprint: "b".repeat(40),
        signer_subject_sha256: "c".repeat(64),
        dpi: 120,
        client_size_px: MtgoSizePxV1 {
            width: 1_550,
            height: 925,
        },
        captured_at_unix_millis: 1_786_338_000_000,
        capture_role: role,
        game_format,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_checked_v1(
        manifest_byte: char,
        png_byte: char,
        output_byte: char,
        pixels: &[u8],
        captured_at_unix_millis: u64,
        role: MtgoDxgiCaptureRoleV2,
    ) -> CheckedUntrustedMtgoDxgiCaptureArtifactV1 {
        let game_format = match role {
            MtgoDxgiCaptureRoleV2::Navigation => None,
            MtgoDxgiCaptureRoleV2::ActingPlayerSolitaire
            | MtgoDxgiCaptureRoleV2::ActingPlayerDuel => Some("Freeform".to_owned()),
            MtgoDxgiCaptureRoleV2::Spectator => Some("Standard".to_owned()),
        };
        CheckedUntrustedMtgoDxgiCaptureArtifactV1 {
            manifest_sha256: manifest_byte.to_string().repeat(64),
            canonical_bgra8_sha256: sha256_v1(pixels),
            preview_png_sha256: png_byte.to_string().repeat(64),
            output_identity_sha256: output_byte.to_string().repeat(64),
            executable_sha256: "a".repeat(64),
            signer_thumbprint: "b".repeat(40),
            signer_subject_sha256: "c".repeat(64),
            dpi: 120,
            client_size_px: MtgoSizePxV1 {
                width: 64,
                height: 64,
            },
            captured_at_unix_millis,
            capture_role: role,
            game_format,
        }
    }

    fn reference_v1(
        checked: &CheckedUntrustedMtgoDxgiCaptureArtifactV1,
    ) -> MtgoDxgiArtifactReferenceV1 {
        MtgoDxgiArtifactReferenceV1 {
            manifest_sha256: checked.manifest_sha256().to_owned(),
            canonical_bgra8_sha256: checked.canonical_bgra8_sha256().to_owned(),
            preview_png_sha256: checked.preview_png_sha256().to_owned(),
            output_identity_sha256: checked.output_identity_sha256().to_owned(),
            client_size_px: checked.client_size_px().clone(),
            captured_at_unix_millis: checked.captured_at_unix_millis(),
            capture_role: checked.capture_role(),
        }
    }

    fn paint_rect_v1(pixels: &mut [u8], rect: &MtgoRectPxV1, value: u8) {
        for y in rect.y..rect.y + rect.height {
            for x in rect.x..rect.x + rect.width {
                let offset = (usize::try_from(y).unwrap() * 64 + usize::try_from(x).unwrap()) * 4;
                pixels[offset..offset + 4].copy_from_slice(&[value, value, value, 255]);
            }
        }
    }

    fn synthetic_keep_v1() -> (
        MtgoDxgiKeepTransitionV1,
        CheckedUntrustedMtgoDxgiCaptureArtifactV1,
        Vec<u8>,
        CheckedUntrustedMtgoDxgiCaptureArtifactV1,
        Vec<u8>,
    ) {
        let mut before = vec![0_u8; 64 * 64 * 4];
        for pixel in before.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
        let mut after = before.clone();
        let regions = [
            (
                MtgoDxgiKeepVisibleChangeV1::PromptChanged,
                MtgoRectPxV1 {
                    x: 0,
                    y: 0,
                    width: 16,
                    height: 16,
                },
            ),
            (
                MtgoDxgiKeepVisibleChangeV1::PlayerCountsChanged,
                MtgoRectPxV1 {
                    x: 16,
                    y: 0,
                    width: 8,
                    height: 8,
                },
            ),
            (
                MtgoDxgiKeepVisibleChangeV1::VisibleGameLogChanged,
                MtgoRectPxV1 {
                    x: 24,
                    y: 0,
                    width: 16,
                    height: 16,
                },
            ),
            (
                MtgoDxgiKeepVisibleChangeV1::PhaseBarChanged,
                MtgoRectPxV1 {
                    x: 0,
                    y: 16,
                    width: 32,
                    height: 8,
                },
            ),
            (
                MtgoDxgiKeepVisibleChangeV1::HandChanged,
                MtgoRectPxV1 {
                    x: 0,
                    y: 24,
                    width: 32,
                    height: 24,
                },
            ),
        ];
        for (index, (_, rect)) in regions.iter().enumerate() {
            paint_rect_v1(&mut after, rect, u8::try_from(index + 1).unwrap());
        }
        let before_checked = synthetic_checked_v1(
            'a',
            'b',
            'c',
            &before,
            1_786_338_000_000,
            MtgoDxgiCaptureRoleV2::ActingPlayerSolitaire,
        );
        let after_checked = synthetic_checked_v1(
            'd',
            'e',
            'c',
            &after,
            1_786_338_001_000,
            MtgoDxgiCaptureRoleV2::ActingPlayerSolitaire,
        );
        let record = MtgoDxgiKeepTransitionV1 {
            schema_version: 1,
            trace_id: "synthetic-dxgi-keep-v1".to_owned(),
            before_frame: reference_v1(&before_checked),
            action: MtgoPregameActionSemanticV1::KeepOpeningHand,
            action_control_before: MtgoRectPxV1 {
                x: 4,
                y: 4,
                width: 8,
                height: 8,
            },
            after_frame: reference_v1(&after_checked),
            visible_postconditions: regions
                .into_iter()
                .map(|(change, rect_client_px)| MtgoDxgiKeepChangedRegionV1 {
                    change,
                    rect_client_px,
                })
                .collect(),
        };
        (record, before_checked, before, after_checked, after)
    }

    fn transition_error_code_v1(
        result: Result<CheckedUntrustedMtgoDxgiKeepTransitionV1, MtgoContractErrorV1>,
    ) -> &'static str {
        match result {
            Ok(_) => panic!("transition unexpectedly validated"),
            Err(error) => error.code(),
        }
    }

    #[test]
    fn ratified_offline_calibration_commitment_is_stable() {
        let checked = CheckedUntrustedMtgoDxgiCaptureArtifactV1 {
            manifest_sha256: "af62f6392454c74a81ada7ea9bc0f0111164bd95b84d114f03e7d416fc27aee3"
                .to_owned(),
            canonical_bgra8_sha256:
                "72005a726e339c1c803ca7c4604d3b182629e65a792de235ae36e10bfa68162d".to_owned(),
            preview_png_sha256: "16bdb4dd6a0fb2e724adcd7962486fa730ce939da9fd75a9731bc7f703877b34"
                .to_owned(),
            output_identity_sha256:
                "89c86876d12827c79ef4d746b9cd88c8decaf3e4fae33b41f6ab57c94bf222a6".to_owned(),
            executable_sha256: "a672755dad7fe8cd08c7986216d0d0fb2c4dbafe669ad3d2aff2bfa2c21b9c69"
                .to_owned(),
            signer_thumbprint: "e9d9e2b989f90555b04c506fddf889c7aba7ac30".to_owned(),
            signer_subject_sha256:
                "89e095d976048cdd8da11e2ff312231867f79e521fa3b5aa6415d2aa59b79cfc".to_owned(),
            dpi: 120,
            client_size_px: MtgoSizePxV1 {
                width: 1550,
                height: 925,
            },
            captured_at_unix_millis: 1_786_337_620_374,
            capture_role: MtgoDxgiCaptureRoleV2::ActingPlayerSolitaire,
            game_format: Some("Freeform".to_owned()),
        };
        assert_eq!(
            dxgi_offline_calibration_admission_commitment_v1(&checked),
            RATIFIED_DXGI_OFFLINE_CALIBRATION_ADMISSION_COMMITMENT_V1.unwrap()
        );
    }

    #[test]
    fn byte_checked_keep_transition_requires_all_visible_changes() {
        let (record, before_checked, before, after_checked, after) = synthetic_keep_v1();
        let checked = check_untrusted_dxgi_keep_transition_v1(
            record,
            &before_checked,
            &before,
            &after_checked,
            &after,
        )
        .unwrap();
        assert!(matches!(
            checked.action(),
            MtgoPregameActionSemanticV1::KeepOpeningHand
        ));
        assert_eq!(checked.changed_region_count(), 5);
        assert_eq!(checked.transition_commitment_sha256().len(), 64);
        assert!(!checked.safe_for_semantic_evidence());
        assert!(!checked.safe_for_policy_scoring());
        assert!(!checked.safe_for_input());
    }

    #[test]
    fn keep_transition_rejects_missing_geometry_and_pixel_identity() {
        let (mut record, before_checked, before, after_checked, after) = synthetic_keep_v1();
        record.visible_postconditions.pop();
        assert_eq!(
            transition_error_code_v1(check_untrusted_dxgi_keep_transition_v1(
                record,
                &before_checked,
                &before,
                &after_checked,
                &after,
            )),
            "dxgi_keep_transition_postconditions"
        );

        let (mut record, before_checked, before, after_checked, after) = synthetic_keep_v1();
        record.action_control_before.x = 48;
        assert_eq!(
            transition_error_code_v1(check_untrusted_dxgi_keep_transition_v1(
                record,
                &before_checked,
                &before,
                &after_checked,
                &after,
            )),
            "dxgi_keep_transition_action_control"
        );

        let (record, before_checked, mut before, after_checked, after) = synthetic_keep_v1();
        before[0] ^= 1;
        assert_eq!(
            transition_error_code_v1(check_untrusted_dxgi_keep_transition_v1(
                record,
                &before_checked,
                &before,
                &after_checked,
                &after,
            )),
            "dxgi_keep_transition_pixel_identity"
        );
    }

    #[test]
    fn keep_transition_rejects_unchanged_required_region_and_wrong_role() {
        let (mut record, before_checked, before, _, mut after) = synthetic_keep_v1();
        let phase = record.visible_postconditions[3].rect_client_px.clone();
        paint_rect_v1(&mut after, &phase, 0);
        let after_checked = synthetic_checked_v1(
            'd',
            'e',
            'c',
            &after,
            1_786_338_001_000,
            MtgoDxgiCaptureRoleV2::ActingPlayerSolitaire,
        );
        record.after_frame = reference_v1(&after_checked);
        assert_eq!(
            transition_error_code_v1(check_untrusted_dxgi_keep_transition_v1(
                record,
                &before_checked,
                &before,
                &after_checked,
                &after,
            )),
            "dxgi_keep_transition_region_unchanged"
        );

        let (mut record, before_checked, before, _, after) = synthetic_keep_v1();
        let after_checked = synthetic_checked_v1(
            'd',
            'e',
            'c',
            &after,
            1_786_338_001_000,
            MtgoDxgiCaptureRoleV2::Spectator,
        );
        record.after_frame = reference_v1(&after_checked);
        assert_eq!(
            transition_error_code_v1(check_untrusted_dxgi_keep_transition_v1(
                record,
                &before_checked,
                &before,
                &after_checked,
                &after,
            )),
            "dxgi_keep_transition_role"
        );
    }
}
