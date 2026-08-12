#[cfg(not(target_os = "windows"))]
compile_error!("mtgo-dxgi-capture-v1 is Windows-only");

mod bottoming_model;
mod competitive_entry_runtime;
mod competitive_event_listing_runtime;
mod competitive_event_record_runtime;
mod competitive_lifecycle_control_runtime;
mod competitive_navigation_profile_frame;
mod competitive_navigation_runtime;
mod competitive_pregame_runtime;
mod competitive_sideboard_runtime;
mod duel_perception_runtime;
mod duel_profile_frame;
mod live_frame;
mod player_visible_duel_gesture_target_wire;
mod pregame_heuristic;
mod visible_accessibility;
mod visible_game_log;
pub use bottoming_model::*;
pub use competitive_entry_runtime::*;
pub use competitive_event_listing_runtime::*;
pub use competitive_event_record_runtime::*;
pub use competitive_lifecycle_control_runtime::*;
pub use competitive_navigation_profile_frame::*;
pub use competitive_navigation_runtime::*;
pub use competitive_pregame_runtime::*;
pub use competitive_sideboard_runtime::*;
pub use duel_perception_runtime::*;
pub use duel_profile_frame::*;
pub use live_frame::*;
pub use pregame_heuristic::*;
pub use visible_accessibility::*;
pub use visible_game_log::*;

use crate::{
    copy_tightly_packed_bgra8_v1, sha256_hex_v1, validate_visible_mtgo_title_v2,
    CaptureWindowModeV2, SignedRectV1,
};
use mtgo_blackbox_v1::{
    check_untrusted_dxgi_capture_artifact_v1,
    classify_untrusted_offline_bottom_six_reflow_candidate_v2,
    classify_untrusted_offline_bottom_six_state_candidate_v3,
    classify_untrusted_offline_bottom_six_visible_card_identities_v3,
    classify_untrusted_offline_first_main_candidate_v2,
    classify_untrusted_offline_first_main_visible_card_identities_v1,
    classify_untrusted_offline_mulligan_ladder_candidate_v2,
    classify_untrusted_offline_mulligan_visible_card_identities_v1, model_deployment_commitment_v1,
    CheckedUntrustedMtgoOfflineBottomSixReflowCandidateV1,
    CheckedUntrustedMtgoOfflineBottomSixStateCandidateV3,
    CheckedUntrustedMtgoOfflineFirstMainCandidateV2,
    CheckedUntrustedMtgoOfflineFirstMainVisibleCardIdentityCandidateV1,
    CheckedUntrustedMtgoOfflineMulliganLadderCandidateV1,
    CheckedUntrustedMtgoOfflineMulliganVisibleCardIdentityCandidateV1,
    CheckedUntrustedMtgoOfflineVisibleCardIdentityCandidateV3,
    CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1, MtgoExpectedModelDeploymentV1,
    MtgoOfflineBottomSixInitialClassificationV1, MtgoOfflineBottomSixReflowClassificationV1,
    MtgoOfflineBottomSixStateClassificationV3, MtgoOfflineFirstMainClassificationV1,
    MtgoOfflineMulliganLadderClassificationV1, MtgoOfflineVisibleCardIdentityClassificationV1,
    MtgoOfflineVisibleCardIdentityV1, MtgoPregameActionSemanticV1, MtgoRectPxV1, MtgoSizePxV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::c_void;
use std::fs::{self, File};
use std::io::Read;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows::core::{Interface, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_NO_MORE_FILES, FILETIME, HANDLE, HMODULE, HWND, POINT, RECT,
};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_BOX,
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
    D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dwm::{
    DwmFlush, DwmGetWindowAttribute, DwmIsCompositionEnabled, DWMWA_CLOAKED,
    DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709, DXGI_FORMAT_B8G8R8A8_UNORM,
    DXGI_MODE_ROTATION_IDENTITY, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput1, IDXGIOutput6,
    IDXGIOutputDuplication, IDXGIResource, DXGI_ERROR_NOT_FOUND, DXGI_OUTDUPL_FRAME_INFO,
};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::Security::Cryptography::{
    CertCloseStore, CertFindCertificateInStore, CertFreeCertificateContext,
    CertGetCertificateContextProperty, CertNameToStrW, CryptMsgClose, CryptMsgGetParam,
    CryptQueryObject, CERT_CONTEXT, CERT_FIND_SUBJECT_CERT, CERT_HASH_PROP_ID, CERT_INFO,
    CERT_NAME_STR_REVERSE_FLAG, CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED,
    CERT_QUERY_FORMAT_FLAG_BINARY, CERT_QUERY_OBJECT_FILE, CERT_STRING_TYPE, CERT_X500_NAME_STR,
    CMSG_SIGNER_INFO, CMSG_SIGNER_INFO_PARAM, HCERTSTORE, PKCS_7_ASN_ENCODING, X509_ASN_ENCODING,
};
use windows::Win32::Security::WinTrust::{
    WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0,
    WINTRUST_FILE_INFO, WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE, WTD_REVOKE_NONE,
    WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UICONTEXT_EXECUTE, WTD_UI_NONE,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, GetDpiForWindow, GetThreadDpiAwarenessContext,
    SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetClientRect, GetCursorInfo, GetForegroundWindow, GetTopWindow, GetWindow,
    GetWindowDisplayAffinity, GetWindowTextW, GetWindowThreadProcessId, IsHungAppWindow, IsIconic,
    IsWindow, IsWindowVisible, CURSORINFO, CURSOR_SHOWING, GA_ROOT, GW_HWNDNEXT, WDA_NONE,
};

type ProbeResult<T> = Result<T, String>;

pub const MTGO_PREGAME_EXTERNAL_SCORING_SCHEMA_V3: u32 = 3;
pub const MTGO_PREGAME_CARD_AWARE_SCORING_SCHEMA_V4: u32 = 4;

const PREGAME_SCORING_REQUEST_DOMAIN_V3: &[u8] = b"mtgo-pregame-scoring-request-v3";
const PREGAME_MODEL_SELECTION_DOMAIN_V3: &[u8] = b"mtgo-pregame-model-selection-v3";
const PREGAME_CARD_AWARE_SCORING_REQUEST_DOMAIN_V4: &[u8] =
    b"mtgo-pregame-card-aware-scoring-request-v4";
const PREGAME_CARD_AWARE_MODEL_SELECTION_DOMAIN_V4: &[u8] =
    b"mtgo-pregame-card-aware-model-selection-v4";
const PREGAME_ACTION_PLAN_DOMAIN_V3: &[u8] = b"mtgo-pregame-action-plan-v3";
const PREGAME_MULLIGAN_CONFIRMATION_DOMAIN_V3: &[u8] = b"mtgo-pregame-mulligan-confirmation-v3";
const PREGAME_KEEP_FIRST_MAIN_CONFIRMATION_DOMAIN_V3: &[u8] =
    b"mtgo-pregame-keep-first-main-confirmation-v3";
const PREGAME_KEEP_BOTTOM_SIX_CONFIRMATION_DOMAIN_V3: &[u8] =
    b"mtgo-pregame-keep-bottom-six-confirmation-v3";
const PREGAME_CONTROL_PROFILE_ID_V3: &str =
    "freeform-solitaire-pregame-controls-1550x925-20260810-v3";
const PREGAME_MULLIGAN_LADDER_PROFILE_COMMITMENT_V3: &str =
    "82f85cdc4a46008686c5336721c8794ce00479ce06910a8b032f72ac3343326f";
const PREGAME_BOTTOM_SIX_INITIAL_PROFILE_COMMITMENT_V3: &str =
    "d7536f59f258be66975b3e87a6435ddf509eb67a2ebc01a5bfd5800568afecb4";

#[derive(Debug)]
struct CliV1 {
    output_directory: PathBuf,
    request: MtgoDxgiCaptureRequestV3,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoDxgiCaptureRequestV3 {
    pub expected_executable_sha256: String,
    pub expected_signer_thumbprint: String,
    pub expected_signer_subject_sha256: String,
    pub window_mode: CaptureWindowModeV2,
    pub expected_game_format: Option<String>,
    pub expected_title_contains: Option<String>,
    pub timeout_ms: u32,
}

/// Copyable telemetry only. Possessing this value does not prove capture.
/// Downstream admission must accept `OpaqueMtgoDxgiFrameCandidateV3` itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoDxgiFrameCommitmentsV3 {
    pub capture_commitment_sha256: String,
    pub canonical_bgra8_sha256: String,
    pub preview_png_sha256: String,
    pub canonical_width: u32,
    pub canonical_height: u32,
    pub client_rect_desktop_px: SignedRectV1,
    pub captured_at_unix_millis: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowSnapshotV1 {
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
    client_rect_desktop_px: SignedRectV1,
    extended_frame_rect_desktop_px: SignedRectV1,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputIdentityV1 {
    adapter_index: u32,
    output_index: u32,
    adapter_luid_low: u32,
    adapter_luid_high: i32,
    device_name: String,
    bounds_desktop_px: SignedRectV1,
    rotation: i32,
    color_space: i32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameMetadataV1 {
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptureManifestV2 {
    schema: String,
    artifact_kind: String,
    status: String,
    capture_backend: String,
    window_mode: String,
    capture_role: String,
    expected_game_format: String,
    title_rule_version: String,
    captured_at_unix_millis: u128,
    safety: SafetyFlagsV1,
    pre: WindowSnapshotV1,
    post: WindowSnapshotV1,
    output: OutputIdentityV1,
    frame: FrameMetadataV1,
    files: FilesV1,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SafetyFlagsV1 {
    safe_for_semantic_evidence: bool,
    safe_for_ocr: bool,
    safe_for_policy_scoring: bool,
    safe_for_input: bool,
    authenticode_verified_in_probe: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FilesV1 {
    canonical_pixels: String,
    preview_png: String,
    manifest: String,
}

struct CapturedFrameV1 {
    pixels: Vec<u8>,
    preview_png: Vec<u8>,
    metadata: FrameMetadataV1,
    output: OutputIdentityV1,
}

/// An in-process proof that the DXGI capture routine completed its checks.
///
/// The type intentionally has no public fields, `Debug`, `Clone`, serde traits,
/// raw-pixel accessor, evidence conversion, policy conversion, or input
/// conversion. Its existence does not resolve the transient-occluder race and
/// therefore does not authorize OCR, semantic evidence, policy, or input.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoDxgiFrameCandidateV3;
/// let _forged = OpaqueMtgoDxgiFrameCandidateV3 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoDxgiFrameCandidateV3;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoDxgiFrameCandidateV3>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoDxgiFrameCandidateV3;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoDxgiFrameCandidateV3>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoDxgiFrameCandidateV3;
/// fn require_serialize<T: serde::Serialize>() {}
/// require_serialize::<OpaqueMtgoDxgiFrameCandidateV3>();
/// ```
pub struct OpaqueMtgoDxgiFrameCandidateV3 {
    capture_commitment_sha256: String,
    canonical_bgra8: Vec<u8>,
    preview_png: Vec<u8>,
    manifest: CaptureManifestV2,
}

impl OpaqueMtgoDxgiFrameCandidateV3 {
    pub fn commitments_v3(&self) -> MtgoDxgiFrameCommitmentsV3 {
        MtgoDxgiFrameCommitmentsV3 {
            capture_commitment_sha256: self.capture_commitment_sha256.clone(),
            canonical_bgra8_sha256: self.manifest.frame.canonical_bgra8_sha256.clone(),
            preview_png_sha256: self.manifest.frame.preview_png_sha256.clone(),
            canonical_width: self.manifest.frame.canonical_width,
            canonical_height: self.manifest.frame.canonical_height,
            client_rect_desktop_px: self.manifest.pre.client_rect_desktop_px,
            captured_at_unix_millis: self.manifest.captured_at_unix_millis,
        }
    }
}

/// A direct in-process exact-template measurement over an opaque DXGI frame.
///
/// Only `measure_mtgo_dxgi_mulligan_ladder_candidate_v3` can construct this
/// type in safe production code. The source frame and pixels remain retained
/// and private. The measurement is checked-untrusted and creates no live-frame,
/// semantic-evidence, observation, policy, or input authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoDxgiMulliganMeasurementV3;
/// let _forged = OpaqueMtgoDxgiMulliganMeasurementV3 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoDxgiMulliganMeasurementV3;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoDxgiMulliganMeasurementV3>();
/// ```
pub struct OpaqueMtgoDxgiMulliganMeasurementV3 {
    source_frame: OpaqueMtgoDxgiFrameCandidateV3,
    measurement: CheckedUntrustedMtgoOfflineMulliganLadderCandidateV1,
}

impl OpaqueMtgoDxgiMulliganMeasurementV3 {
    pub fn source_capture_commitments_v3(&self) -> MtgoDxgiFrameCommitmentsV3 {
        self.source_frame.commitments_v3()
    }

    pub fn classification_v3(&self) -> MtgoOfflineMulliganLadderClassificationV1 {
        self.measurement.classification()
    }

    pub fn prospective_keep_size_v3(&self) -> Option<u8> {
        self.measurement.prospective_keep_size()
    }

    pub fn ordered_actions_v3(&self) -> &[MtgoPregameActionSemanticV1] {
        self.measurement.ordered_actions()
    }

    pub fn profile_set_commitment_sha256_v3(&self) -> &str {
        self.measurement.profile_set_commitment_sha256()
    }

    pub fn measurement_commitment_sha256_v3(&self) -> &str {
        self.measurement.candidate_commitment_sha256()
    }

    pub fn safe_for_semantic_evidence_v3(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5_v3(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring_v3(&self) -> bool {
        false
    }

    pub fn safe_for_input_v3(&self) -> bool {
        false
    }
}

pub fn measure_mtgo_dxgi_mulligan_ladder_candidate_v3(
    source_frame: OpaqueMtgoDxgiFrameCandidateV3,
) -> Result<OpaqueMtgoDxgiMulliganMeasurementV3, String> {
    let manifest_bytes = serialize_manifest_v2(&source_frame.manifest)?;
    let measurement = measure_mulligan_ladder_parts_v3(
        &manifest_bytes,
        &source_frame.canonical_bgra8,
        &source_frame.preview_png,
    )?;
    Ok(OpaqueMtgoDxgiMulliganMeasurementV3 {
        source_frame,
        measurement,
    })
}

/// A direct in-process complete-hand identity candidate over one opaque London
/// mulligan measurement and one checked-untrusted deck template profile. The
/// source frame, pixels, and template bytes remain private. The result is
/// suitable only for the checked-untrusted external scoring experiment and
/// grants no semantic evidence, observation, trusted policy, or input
/// authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoDxgiMulliganVisibleHandMeasurementV3;
/// let _forged = OpaqueMtgoDxgiMulliganVisibleHandMeasurementV3 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoDxgiMulliganVisibleHandMeasurementV3;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoDxgiMulliganVisibleHandMeasurementV3>();
/// ```
pub struct OpaqueMtgoDxgiMulliganVisibleHandMeasurementV3 {
    source: OpaqueMtgoDxgiMulliganMeasurementV3,
    profile: CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1,
    measurement: CheckedUntrustedMtgoOfflineMulliganVisibleCardIdentityCandidateV1,
}

impl OpaqueMtgoDxgiMulliganVisibleHandMeasurementV3 {
    pub fn source_capture_commitments_v3(&self) -> MtgoDxgiFrameCommitmentsV3 {
        self.source.source_frame.commitments_v3()
    }

    pub fn classification_v3(&self) -> MtgoOfflineVisibleCardIdentityClassificationV1 {
        self.measurement.classification()
    }

    pub fn prospective_keep_size_v3(&self) -> Option<u8> {
        self.measurement.prospective_keep_size()
    }

    pub fn ordered_actions_v3(&self) -> &[MtgoPregameActionSemanticV1] {
        self.source.ordered_actions_v3()
    }

    pub fn identities_v3(&self) -> &[MtgoOfflineVisibleCardIdentityV1] {
        self.measurement.identities()
    }

    pub fn mulligan_profile_set_commitment_sha256_v3(&self) -> &str {
        self.source.profile_set_commitment_sha256_v3()
    }

    pub fn mulligan_measurement_commitment_sha256_v3(&self) -> &str {
        self.source.measurement_commitment_sha256_v3()
    }

    pub fn visible_card_profile_commitment_sha256_v3(&self) -> &str {
        self.profile.profile_commitment_sha256()
    }

    pub fn visible_identity_measurement_commitment_sha256_v3(&self) -> &str {
        self.measurement.candidate_commitment_sha256()
    }

    pub fn safe_for_semantic_evidence_v3(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5_v3(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring_v3(&self) -> bool {
        false
    }

    pub fn safe_for_input_v3(&self) -> bool {
        false
    }
}

pub fn measure_mtgo_dxgi_mulligan_visible_hand_candidate_v3(
    source: OpaqueMtgoDxgiMulliganMeasurementV3,
    profile: CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1,
) -> Result<OpaqueMtgoDxgiMulliganVisibleHandMeasurementV3, String> {
    let manifest_bytes = serialize_manifest_v2(&source.source_frame.manifest)?;
    let checked = check_untrusted_dxgi_capture_artifact_v1(
        &manifest_bytes,
        &source.source_frame.canonical_bgra8,
        &source.source_frame.preview_png,
    )
    .map_err(|error| format!("check opaque mulligan visible-hand capture: {error}"))?;
    let measurement = classify_untrusted_offline_mulligan_visible_card_identities_v1(
        &checked,
        &source.source_frame.canonical_bgra8,
        &profile,
    )
    .map_err(|error| format!("classify opaque mulligan visible hand: {error}"))?;
    if measurement.source_ladder_commitment_sha256()
        != source.measurement.candidate_commitment_sha256()
    {
        return Err(
            "mulligan visible-hand measurement does not bind the source ladder measurement"
                .to_owned(),
        );
    }
    Ok(OpaqueMtgoDxgiMulliganVisibleHandMeasurementV3 {
        source,
        profile,
        measurement,
    })
}

/// A direct in-process measurement of the exact visible bottom-six prompt
/// before any card has been selected. The opaque source frame and pixels remain
/// private. This recognizes only required bottom count six at selected count
/// zero and grants no observation, policy, or input authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoDxgiBottomSixInitialMeasurementV3;
/// let _forged = OpaqueMtgoDxgiBottomSixInitialMeasurementV3 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoDxgiBottomSixInitialMeasurementV3;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoDxgiBottomSixInitialMeasurementV3>();
/// ```
pub struct OpaqueMtgoDxgiBottomSixInitialMeasurementV3 {
    source_frame: OpaqueMtgoDxgiFrameCandidateV3,
    measurement: CheckedUntrustedMtgoOfflineBottomSixStateCandidateV3,
}

impl OpaqueMtgoDxgiBottomSixInitialMeasurementV3 {
    pub fn source_capture_commitments_v3(&self) -> MtgoDxgiFrameCommitmentsV3 {
        self.source_frame.commitments_v3()
    }

    pub fn classification_v3(&self) -> MtgoOfflineBottomSixInitialClassificationV1 {
        if self.measurement.classification() == MtgoOfflineBottomSixStateClassificationV3::Match
            && self.measurement.selected_count() == Some(0)
        {
            MtgoOfflineBottomSixInitialClassificationV1::Match
        } else {
            MtgoOfflineBottomSixInitialClassificationV1::NoMatch
        }
    }

    pub fn required_bottom_count_v3(&self) -> Option<u8> {
        (self.classification_v3() == MtgoOfflineBottomSixInitialClassificationV1::Match)
            .then_some(6)
    }

    pub fn selected_count_v3(&self) -> Option<u8> {
        (self.classification_v3() == MtgoOfflineBottomSixInitialClassificationV1::Match)
            .then_some(0)
    }

    pub fn profile_commitment_sha256_v3(&self) -> &str {
        self.measurement.profile_commitment_sha256()
    }

    pub fn measurement_commitment_sha256_v3(&self) -> &str {
        self.measurement.candidate_commitment_sha256()
    }

    pub fn safe_for_semantic_evidence_v3(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5_v3(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring_v3(&self) -> bool {
        false
    }

    pub fn safe_for_input_v3(&self) -> bool {
        false
    }
}

pub fn measure_mtgo_dxgi_bottom_six_initial_candidate_v3(
    source_frame: OpaqueMtgoDxgiFrameCandidateV3,
) -> Result<OpaqueMtgoDxgiBottomSixInitialMeasurementV3, String> {
    let manifest_bytes = serialize_manifest_v2(&source_frame.manifest)?;
    let checked = check_untrusted_dxgi_capture_artifact_v1(
        &manifest_bytes,
        &source_frame.canonical_bgra8,
        &source_frame.preview_png,
    )
    .map_err(|error| format!("check opaque bottom-six capture: {error}"))?;
    let measurement = classify_untrusted_offline_bottom_six_state_candidate_v3(
        &checked,
        &source_frame.canonical_bgra8,
    )
    .map_err(|error| format!("classify opaque bottom-six capture: {error}"))?;
    Ok(OpaqueMtgoDxgiBottomSixInitialMeasurementV3 {
        source_frame,
        measurement,
    })
}

/// A direct in-process structural measurement of any reviewed bottom-six
/// selection stage from zero through six selected cards. The opaque source
/// frame and pixels remain private. A Match exposes only the visible hand
/// count, selected count, Done visibility, action cardinality, and immutable
/// commitments. It does not identify cards or grant observation, scoring, or
/// input authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoDxgiBottomSixStateMeasurementV3;
/// let _forged = OpaqueMtgoDxgiBottomSixStateMeasurementV3 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoDxgiBottomSixStateMeasurementV3;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoDxgiBottomSixStateMeasurementV3>();
/// ```
pub struct OpaqueMtgoDxgiBottomSixStateMeasurementV3 {
    source_frame: OpaqueMtgoDxgiFrameCandidateV3,
    measurement: CheckedUntrustedMtgoOfflineBottomSixStateCandidateV3,
}

impl OpaqueMtgoDxgiBottomSixStateMeasurementV3 {
    pub fn source_capture_commitments_v3(&self) -> MtgoDxgiFrameCommitmentsV3 {
        self.source_frame.commitments_v3()
    }

    pub fn classification_v3(&self) -> MtgoOfflineBottomSixStateClassificationV3 {
        self.measurement.classification()
    }

    pub fn required_bottom_count_v3(&self) -> Option<u8> {
        self.measurement.required_bottom_count()
    }

    pub fn visible_hand_count_v3(&self) -> Option<u8> {
        self.measurement.visible_hand_count()
    }

    pub fn selected_count_v3(&self) -> Option<u8> {
        self.measurement.selected_count()
    }

    pub fn done_visible_v3(&self) -> Option<bool> {
        self.measurement.done_visible()
    }

    pub fn legal_action_count_v3(&self) -> Option<u8> {
        self.measurement.legal_action_count()
    }

    pub fn profile_commitment_sha256_v3(&self) -> &str {
        self.measurement.profile_commitment_sha256()
    }

    pub fn measurement_commitment_sha256_v3(&self) -> &str {
        self.measurement.candidate_commitment_sha256()
    }

    pub fn safe_for_semantic_evidence_v3(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5_v3(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring_v3(&self) -> bool {
        false
    }

    pub fn safe_for_input_v3(&self) -> bool {
        false
    }
}

pub fn measure_mtgo_dxgi_bottom_six_state_candidate_v3(
    source_frame: OpaqueMtgoDxgiFrameCandidateV3,
) -> Result<OpaqueMtgoDxgiBottomSixStateMeasurementV3, String> {
    let manifest_bytes = serialize_manifest_v2(&source_frame.manifest)?;
    let checked = check_untrusted_dxgi_capture_artifact_v1(
        &manifest_bytes,
        &source_frame.canonical_bgra8,
        &source_frame.preview_png,
    )
    .map_err(|error| format!("check opaque bottom-six state capture: {error}"))?;
    let measurement = classify_untrusted_offline_bottom_six_state_candidate_v3(
        &checked,
        &source_frame.canonical_bgra8,
    )
    .map_err(|error| format!("classify opaque bottom-six state capture: {error}"))?;
    Ok(OpaqueMtgoDxgiBottomSixStateMeasurementV3 {
        source_frame,
        measurement,
    })
}

/// A direct in-process complete-hand identity candidate over one opaque
/// bottom-six state and one checked-untrusted deck template profile. The
/// profile labels remain caller supplied and unratified. Both source pixels
/// and template pixels stay private, and the result grants no semantic,
/// observation, scoring, coordinate, or input authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoDxgiBottomSixVisibleCardIdentityMeasurementV3;
/// let _forged = OpaqueMtgoDxgiBottomSixVisibleCardIdentityMeasurementV3 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoDxgiBottomSixVisibleCardIdentityMeasurementV3;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoDxgiBottomSixVisibleCardIdentityMeasurementV3>();
/// ```
pub struct OpaqueMtgoDxgiBottomSixVisibleCardIdentityMeasurementV3 {
    source: OpaqueMtgoDxgiBottomSixStateMeasurementV3,
    profile: CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1,
    measurement: CheckedUntrustedMtgoOfflineVisibleCardIdentityCandidateV3,
}

impl OpaqueMtgoDxgiBottomSixVisibleCardIdentityMeasurementV3 {
    pub fn source_capture_commitments_v3(&self) -> MtgoDxgiFrameCommitmentsV3 {
        self.source.source_frame.commitments_v3()
    }

    pub fn classification_v3(&self) -> MtgoOfflineVisibleCardIdentityClassificationV1 {
        self.measurement.classification()
    }

    pub fn visible_hand_count_v3(&self) -> Option<u8> {
        self.measurement.visible_hand_count()
    }

    pub fn matched_identity_count_v3(&self) -> u8 {
        self.measurement.matched_identity_count()
    }

    pub fn identities_v3(&self) -> &[MtgoOfflineVisibleCardIdentityV1] {
        self.measurement.identities()
    }

    pub fn profile_commitment_sha256_v3(&self) -> &str {
        self.profile.profile_commitment_sha256()
    }

    pub fn measurement_commitment_sha256_v3(&self) -> &str {
        self.measurement.candidate_commitment_sha256()
    }

    pub fn safe_for_semantic_evidence_v3(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5_v3(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring_v3(&self) -> bool {
        false
    }

    pub fn safe_for_input_v3(&self) -> bool {
        false
    }
}

pub fn measure_mtgo_dxgi_bottom_six_visible_card_identities_candidate_v3(
    source: OpaqueMtgoDxgiBottomSixStateMeasurementV3,
    profile: CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1,
) -> Result<OpaqueMtgoDxgiBottomSixVisibleCardIdentityMeasurementV3, String> {
    let manifest_bytes = serialize_manifest_v2(&source.source_frame.manifest)?;
    let checked = check_untrusted_dxgi_capture_artifact_v1(
        &manifest_bytes,
        &source.source_frame.canonical_bgra8,
        &source.source_frame.preview_png,
    )
    .map_err(|error| format!("check opaque visible-card identity capture: {error}"))?;
    let measurement = classify_untrusted_offline_bottom_six_visible_card_identities_v3(
        &checked,
        &source.source_frame.canonical_bgra8,
        &profile,
    )
    .map_err(|error| format!("classify opaque visible-card identities: {error}"))?;
    Ok(OpaqueMtgoDxgiBottomSixVisibleCardIdentityMeasurementV3 {
        source,
        profile,
        measurement,
    })
}

/// A direct in-process visual reflow measurement over two opaque consecutive
/// bottom-six stages. A Match identifies the unique visible ordinal that
/// disappeared while retaining both source frames, all pixels, and card-region
/// geometry privately. It does not identify the card name or grant observation,
/// scoring, or input authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoDxgiBottomSixReflowMeasurementV3;
/// let _forged = OpaqueMtgoDxgiBottomSixReflowMeasurementV3 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoDxgiBottomSixReflowMeasurementV3;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoDxgiBottomSixReflowMeasurementV3>();
/// ```
pub struct OpaqueMtgoDxgiBottomSixReflowMeasurementV3 {
    before: OpaqueMtgoDxgiBottomSixStateMeasurementV3,
    after: OpaqueMtgoDxgiBottomSixStateMeasurementV3,
    measurement: CheckedUntrustedMtgoOfflineBottomSixReflowCandidateV1,
}

impl OpaqueMtgoDxgiBottomSixReflowMeasurementV3 {
    pub fn before_source_capture_commitments_v3(&self) -> MtgoDxgiFrameCommitmentsV3 {
        self.before.source_frame.commitments_v3()
    }

    pub fn after_source_capture_commitments_v3(&self) -> MtgoDxgiFrameCommitmentsV3 {
        self.after.source_frame.commitments_v3()
    }

    pub fn classification_v3(&self) -> MtgoOfflineBottomSixReflowClassificationV1 {
        self.measurement.classification()
    }

    pub fn before_selected_count_v3(&self) -> Option<u8> {
        self.measurement.before_selected_count()
    }

    pub fn after_selected_count_v3(&self) -> Option<u8> {
        self.measurement.after_selected_count()
    }

    pub fn removed_before_ordinal_v3(&self) -> Option<u8> {
        self.measurement.removed_before_ordinal()
    }

    pub fn matched_pair_mean_absolute_difference_milli_v3(&self) -> &[u32] {
        self.measurement
            .matched_pair_mean_absolute_difference_milli()
    }

    pub fn passing_deletion_candidate_count_v3(&self) -> u8 {
        self.measurement.passing_deletion_candidate_count()
    }

    pub fn measurement_commitment_sha256_v3(&self) -> &str {
        self.measurement.candidate_commitment_sha256()
    }

    pub fn safe_for_semantic_evidence_v3(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5_v3(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring_v3(&self) -> bool {
        false
    }

    pub fn safe_for_input_v3(&self) -> bool {
        false
    }
}

pub fn measure_mtgo_dxgi_bottom_six_reflow_candidate_v3(
    before: OpaqueMtgoDxgiBottomSixStateMeasurementV3,
    after: OpaqueMtgoDxgiBottomSixStateMeasurementV3,
) -> Result<OpaqueMtgoDxgiBottomSixReflowMeasurementV3, String> {
    let before_manifest = serialize_manifest_v2(&before.source_frame.manifest)?;
    let after_manifest = serialize_manifest_v2(&after.source_frame.manifest)?;
    let before_checked = check_untrusted_dxgi_capture_artifact_v1(
        &before_manifest,
        &before.source_frame.canonical_bgra8,
        &before.source_frame.preview_png,
    )
    .map_err(|error| format!("check opaque before-reflow capture: {error}"))?;
    let after_checked = check_untrusted_dxgi_capture_artifact_v1(
        &after_manifest,
        &after.source_frame.canonical_bgra8,
        &after.source_frame.preview_png,
    )
    .map_err(|error| format!("check opaque after-reflow capture: {error}"))?;
    let measurement = classify_untrusted_offline_bottom_six_reflow_candidate_v2(
        &before_checked,
        &before.source_frame.canonical_bgra8,
        &after_checked,
        &after.source_frame.canonical_bgra8,
    )
    .map_err(|error| format!("classify opaque bottom-six reflow: {error}"))?;
    Ok(OpaqueMtgoDxgiBottomSixReflowMeasurementV3 {
        before,
        after,
        measurement,
    })
}

/// A direct in-process exact-template measurement of the visible Turn 1
/// first-main state. The source frame and pixels remain private, and the result
/// grants no observation, policy, or input authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoDxgiFirstMainMeasurementV3;
/// let _forged = OpaqueMtgoDxgiFirstMainMeasurementV3 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoDxgiFirstMainMeasurementV3;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoDxgiFirstMainMeasurementV3>();
/// ```
pub struct OpaqueMtgoDxgiFirstMainMeasurementV3 {
    source_frame: OpaqueMtgoDxgiFrameCandidateV3,
    measurement: CheckedUntrustedMtgoOfflineFirstMainCandidateV2,
}

impl OpaqueMtgoDxgiFirstMainMeasurementV3 {
    pub fn source_capture_commitments_v3(&self) -> MtgoDxgiFrameCommitmentsV3 {
        self.source_frame.commitments_v3()
    }

    pub fn classification_v3(&self) -> MtgoOfflineFirstMainClassificationV1 {
        self.measurement.classification()
    }

    pub fn profile_commitment_sha256_v3(&self) -> &str {
        self.measurement.profile_commitment_sha256()
    }

    pub fn measurement_commitment_sha256_v3(&self) -> &str {
        self.measurement.candidate_commitment_sha256()
    }

    pub fn safe_for_semantic_evidence_v3(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5_v3(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring_v3(&self) -> bool {
        false
    }

    pub fn safe_for_input_v3(&self) -> bool {
        false
    }
}

pub fn measure_mtgo_dxgi_first_main_candidate_v3(
    source_frame: OpaqueMtgoDxgiFrameCandidateV3,
) -> Result<OpaqueMtgoDxgiFirstMainMeasurementV3, String> {
    let manifest_bytes = serialize_manifest_v2(&source_frame.manifest)?;
    let checked = check_untrusted_dxgi_capture_artifact_v1(
        &manifest_bytes,
        &source_frame.canonical_bgra8,
        &source_frame.preview_png,
    )
    .map_err(|error| format!("check opaque first-main capture: {error}"))?;
    let measurement =
        classify_untrusted_offline_first_main_candidate_v2(&checked, &source_frame.canonical_bgra8)
            .map_err(|error| format!("classify opaque first-main capture: {error}"))?;
    Ok(OpaqueMtgoDxgiFirstMainMeasurementV3 {
        source_frame,
        measurement,
    })
}

/// A complete eight-card identity candidate over one opaque Turn 1 first-main
/// measurement and one checked-untrusted template profile. The source frame,
/// pixels, coordinates, and template bytes remain private. The result grants no
/// semantic evidence, observation, model-scoring, or input authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoDxgiFirstMainVisibleHandMeasurementV1;
/// let _forged = OpaqueMtgoDxgiFirstMainVisibleHandMeasurementV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoDxgiFirstMainVisibleHandMeasurementV1;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoDxgiFirstMainVisibleHandMeasurementV1>();
/// ```
pub struct OpaqueMtgoDxgiFirstMainVisibleHandMeasurementV1 {
    source: OpaqueMtgoDxgiFirstMainMeasurementV3,
    profile: CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1,
    measurement: CheckedUntrustedMtgoOfflineFirstMainVisibleCardIdentityCandidateV1,
}

impl OpaqueMtgoDxgiFirstMainVisibleHandMeasurementV1 {
    pub fn source_capture_commitments_v1(&self) -> MtgoDxgiFrameCommitmentsV3 {
        self.source.source_frame.commitments_v3()
    }

    pub fn classification_v1(&self) -> MtgoOfflineVisibleCardIdentityClassificationV1 {
        self.measurement.classification()
    }

    pub fn visible_hand_count_v1(&self) -> Option<u8> {
        self.measurement.visible_hand_count()
    }

    pub fn matched_identity_count_v1(&self) -> u8 {
        self.measurement.matched_identity_count()
    }

    pub fn identities_v1(&self) -> &[MtgoOfflineVisibleCardIdentityV1] {
        self.measurement.identities()
    }

    pub fn first_main_measurement_commitment_sha256_v1(&self) -> &str {
        self.source.measurement_commitment_sha256_v3()
    }

    pub fn visible_card_profile_commitment_sha256_v1(&self) -> &str {
        self.profile.profile_commitment_sha256()
    }

    pub fn visible_identity_measurement_commitment_sha256_v1(&self) -> &str {
        self.measurement.candidate_commitment_sha256()
    }

    pub fn safe_for_semantic_evidence_v1(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5_v1(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

pub fn measure_mtgo_dxgi_first_main_visible_hand_candidate_v1(
    source: OpaqueMtgoDxgiFirstMainMeasurementV3,
    profile: CheckedUntrustedMtgoOfflineVisibleCardTemplateProfileV1,
) -> Result<OpaqueMtgoDxgiFirstMainVisibleHandMeasurementV1, String> {
    let manifest_bytes = serialize_manifest_v2(&source.source_frame.manifest)?;
    let checked = check_untrusted_dxgi_capture_artifact_v1(
        &manifest_bytes,
        &source.source_frame.canonical_bgra8,
        &source.source_frame.preview_png,
    )
    .map_err(|error| format!("check opaque first-main visible-hand capture: {error}"))?;
    let measurement = classify_untrusted_offline_first_main_visible_card_identities_v1(
        &checked,
        &source.source_frame.canonical_bgra8,
        &source.measurement,
        &profile,
    )
    .map_err(|error| format!("classify opaque first-main visible hand: {error}"))?;
    Ok(OpaqueMtgoDxgiFirstMainVisibleHandMeasurementV1 {
        source,
        profile,
        measurement,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPregameScoringRequestV3 {
    pub schema_version: u32,
    pub source_capture_commitment_sha256: String,
    pub measurement_commitment_sha256: String,
    pub profile_set_commitment_sha256: String,
    pub prospective_keep_size: u8,
    pub ordered_actions: Vec<MtgoPregameActionSemanticV1>,
    pub deployment_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPregameScoreResponseV3 {
    pub schema_version: u32,
    pub request_commitment_sha256: String,
    pub logits_f32_bits: Vec<u32>,
    pub value_f32_bits: u32,
}

/// External scorers receive only exact visible pregame semantics and model
/// identity commitments. They receive no pixels, coordinates, process handles,
/// authorization, or input capability.
pub trait MtgoExternalPregameScorerV3 {
    fn score_pregame_v3(
        &mut self,
        request: &MtgoPregameScoringRequestV3,
    ) -> Result<MtgoPregameScoreResponseV3, String>;
}

/// One request-bound, deterministic model selection over an opaque measured
/// pregame frame. This is not trusted model authority and cannot be converted
/// to input.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPregameModelSelectionV3;
/// let _forged = OpaqueMtgoPregameModelSelectionV3 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPregameModelSelectionV3;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoPregameModelSelectionV3>();
/// ```
pub struct OpaqueMtgoPregameModelSelectionV3 {
    measurement: OpaqueMtgoDxgiMulliganMeasurementV3,
    request: MtgoPregameScoringRequestV3,
    response: MtgoPregameScoreResponseV3,
    selected_index: usize,
    selected_semantic: MtgoPregameActionSemanticV1,
    selection_commitment_sha256: String,
}

impl OpaqueMtgoPregameModelSelectionV3 {
    pub fn selected_index_v3(&self) -> usize {
        self.selected_index
    }

    pub fn selected_semantic_v3(&self) -> &MtgoPregameActionSemanticV1 {
        &self.selected_semantic
    }

    pub fn selected_logit_f32_bits_v3(&self) -> u32 {
        self.response.logits_f32_bits[self.selected_index]
    }

    pub fn value_f32_bits_v3(&self) -> u32 {
        self.response.value_f32_bits
    }

    pub fn request_commitment_sha256_v3(&self) -> &str {
        &self.response.request_commitment_sha256
    }

    pub fn deployment_commitment_sha256_v3(&self) -> &str {
        &self.request.deployment_commitment_sha256
    }

    pub fn measurement_commitment_sha256_v3(&self) -> &str {
        self.measurement.measurement_commitment_sha256_v3()
    }

    pub fn selection_commitment_sha256_v3(&self) -> &str {
        &self.selection_commitment_sha256
    }

    pub fn safe_for_semantic_evidence_v3(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5_v3(&self) -> bool {
        false
    }

    pub fn safe_for_live_input_v3(&self) -> bool {
        false
    }
}

pub fn build_pregame_scoring_request_v3(
    measurement: &OpaqueMtgoDxgiMulliganMeasurementV3,
    deployment: &MtgoExpectedModelDeploymentV1,
) -> Result<MtgoPregameScoringRequestV3, String> {
    let capture = measurement.source_capture_commitments_v3();
    build_pregame_scoring_request_from_parts_v3(
        measurement.classification_v3(),
        measurement.prospective_keep_size_v3(),
        measurement.ordered_actions_v3(),
        &capture.capture_commitment_sha256,
        measurement.measurement_commitment_sha256_v3(),
        measurement.profile_set_commitment_sha256_v3(),
        deployment,
    )
}

pub fn pregame_scoring_request_commitment_v3(
    request: &MtgoPregameScoringRequestV3,
) -> Result<String, String> {
    validate_pregame_scoring_request_v3(request)?;
    canonical_json_commitment_v3(PREGAME_SCORING_REQUEST_DOMAIN_V3, request)
}

pub fn score_and_select_pregame_model_v3<S: MtgoExternalPregameScorerV3>(
    measurement: OpaqueMtgoDxgiMulliganMeasurementV3,
    deployment: &MtgoExpectedModelDeploymentV1,
    scorer: &mut S,
) -> Result<OpaqueMtgoPregameModelSelectionV3, String> {
    let request = build_pregame_scoring_request_v3(&measurement, deployment)?;
    let response = scorer.score_pregame_v3(&request)?;
    validate_pregame_score_response_v3(measurement, deployment, response)
}

pub fn validate_pregame_score_response_v3(
    measurement: OpaqueMtgoDxgiMulliganMeasurementV3,
    deployment: &MtgoExpectedModelDeploymentV1,
    response: MtgoPregameScoreResponseV3,
) -> Result<OpaqueMtgoPregameModelSelectionV3, String> {
    let request = build_pregame_scoring_request_v3(&measurement, deployment)?;
    let (selected_index, selected_semantic, selection_commitment_sha256) =
        validate_pregame_score_response_parts_v3(&request, &response)?;
    Ok(OpaqueMtgoPregameModelSelectionV3 {
        measurement,
        request,
        response,
        selected_index,
        selected_semantic,
        selection_commitment_sha256,
    })
}

#[allow(clippy::too_many_arguments)]
fn build_pregame_scoring_request_from_parts_v3(
    classification: MtgoOfflineMulliganLadderClassificationV1,
    prospective_keep_size: Option<u8>,
    ordered_actions: &[MtgoPregameActionSemanticV1],
    source_capture_commitment_sha256: &str,
    measurement_commitment_sha256: &str,
    profile_set_commitment_sha256: &str,
    deployment: &MtgoExpectedModelDeploymentV1,
) -> Result<MtgoPregameScoringRequestV3, String> {
    if classification != MtgoOfflineMulliganLadderClassificationV1::Match {
        return Err("pregame scoring requires exactly one measured prompt match".to_owned());
    }
    let prospective_keep_size = prospective_keep_size
        .filter(|value| (1..=7).contains(value))
        .ok_or("pregame scoring requires a prospective keep size from one through seven")?;
    let expected_actions = [
        MtgoPregameActionSemanticV1::Mulligan {
            next_hand_size: prospective_keep_size - 1,
        },
        MtgoPregameActionSemanticV1::KeepOpeningHand,
    ];
    if ordered_actions != expected_actions {
        return Err(
            "pregame scoring requires the exact ordered Mulligan and Keep actions".to_owned(),
        );
    }
    for digest in [
        source_capture_commitment_sha256,
        measurement_commitment_sha256,
        profile_set_commitment_sha256,
    ] {
        require_lower_sha256_v3(digest, "pregame scoring source commitment")?;
    }
    let deployment_commitment_sha256 = model_deployment_commitment_v1(deployment)
        .map_err(|error| format!("pregame model deployment: {error}"))?;
    let request = MtgoPregameScoringRequestV3 {
        schema_version: MTGO_PREGAME_EXTERNAL_SCORING_SCHEMA_V3,
        source_capture_commitment_sha256: source_capture_commitment_sha256.to_owned(),
        measurement_commitment_sha256: measurement_commitment_sha256.to_owned(),
        profile_set_commitment_sha256: profile_set_commitment_sha256.to_owned(),
        prospective_keep_size,
        ordered_actions: ordered_actions.to_vec(),
        deployment_commitment_sha256,
    };
    validate_pregame_scoring_request_v3(&request)?;
    Ok(request)
}

fn validate_pregame_scoring_request_v3(
    request: &MtgoPregameScoringRequestV3,
) -> Result<(), String> {
    if request.schema_version != MTGO_PREGAME_EXTERNAL_SCORING_SCHEMA_V3
        || !(1..=7).contains(&request.prospective_keep_size)
    {
        return Err("pregame scoring request schema or keep size is invalid".to_owned());
    }
    let expected_actions = [
        MtgoPregameActionSemanticV1::Mulligan {
            next_hand_size: request.prospective_keep_size - 1,
        },
        MtgoPregameActionSemanticV1::KeepOpeningHand,
    ];
    if request.ordered_actions != expected_actions {
        return Err("pregame scoring request actions are not the canonical ordered set".to_owned());
    }
    for digest in [
        &request.source_capture_commitment_sha256,
        &request.measurement_commitment_sha256,
        &request.profile_set_commitment_sha256,
        &request.deployment_commitment_sha256,
    ] {
        require_lower_sha256_v3(digest, "pregame scoring request commitment")?;
    }
    Ok(())
}

fn validate_pregame_score_response_parts_v3(
    request: &MtgoPregameScoringRequestV3,
    response: &MtgoPregameScoreResponseV3,
) -> Result<(usize, MtgoPregameActionSemanticV1, String), String> {
    let request_commitment_sha256 = pregame_scoring_request_commitment_v3(request)?;
    if response.schema_version != MTGO_PREGAME_EXTERNAL_SCORING_SCHEMA_V3
        || response.request_commitment_sha256 != request_commitment_sha256
    {
        return Err("pregame score response does not bind the exact request".to_owned());
    }
    require_lower_sha256_v3(
        &response.request_commitment_sha256,
        "pregame score response request commitment",
    )?;
    if response.logits_f32_bits.len() != request.ordered_actions.len()
        || response.logits_f32_bits.is_empty()
    {
        return Err("pregame score response logit count is invalid".to_owned());
    }
    let logits = response
        .logits_f32_bits
        .iter()
        .map(|bits| f32::from_bits(*bits))
        .collect::<Vec<_>>();
    if logits.iter().any(|value| !value.is_finite())
        || !f32::from_bits(response.value_f32_bits).is_finite()
    {
        return Err("pregame score response must contain only finite values".to_owned());
    }
    let mut selected_index = 0;
    for index in 1..logits.len() {
        if logits[index].total_cmp(&logits[selected_index]).is_gt() {
            selected_index = index;
        }
    }
    let selected_semantic = request.ordered_actions[selected_index].clone();
    #[derive(Serialize)]
    struct SelectionRecordV3<'a> {
        request: &'a MtgoPregameScoringRequestV3,
        response: &'a MtgoPregameScoreResponseV3,
        selected_index: usize,
        selected_semantic: &'a MtgoPregameActionSemanticV1,
    }
    let selection_commitment_sha256 = canonical_json_commitment_v3(
        PREGAME_MODEL_SELECTION_DOMAIN_V3,
        &SelectionRecordV3 {
            request,
            response,
            selected_index,
            selected_semantic: &selected_semantic,
        },
    )?;
    Ok((
        selected_index,
        selected_semantic,
        selection_commitment_sha256,
    ))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCardAwarePregameScoringRequestV4 {
    pub schema_version: u32,
    pub source_capture_commitment_sha256: String,
    pub mulligan_measurement_commitment_sha256: String,
    pub visible_identity_measurement_commitment_sha256: String,
    pub mulligan_profile_set_commitment_sha256: String,
    pub visible_card_profile_commitment_sha256: String,
    pub prospective_keep_size: u8,
    pub ordered_visible_card_names: Vec<String>,
    pub ordered_actions: Vec<MtgoPregameActionSemanticV1>,
    pub deployment_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCardAwarePregameScoreResponseV4 {
    pub schema_version: u32,
    pub request_commitment_sha256: String,
    pub logits_f32_bits: Vec<u32>,
    pub value_f32_bits: u32,
}

/// External scorers receive the exact ordered visible card labels, prospective
/// keep size, legal pregame actions, and model identity commitments. They
/// receive no pixels, template bytes, coordinates, process handles,
/// authorization, or input capability. Card labels remain checked-untrusted.
pub trait MtgoExternalCardAwarePregameScorerV4 {
    fn score_card_aware_pregame_v4(
        &mut self,
        request: &MtgoCardAwarePregameScoringRequestV4,
    ) -> Result<MtgoCardAwarePregameScoreResponseV4, String>;
}

/// One request-bound deterministic selection from the card-aware external
/// scorer. It cannot be converted to the v3 action plan or to live input.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCardAwarePregameModelSelectionV4;
/// let _forged = OpaqueMtgoCardAwarePregameModelSelectionV4 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCardAwarePregameModelSelectionV4;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoCardAwarePregameModelSelectionV4>();
/// ```
pub struct OpaqueMtgoCardAwarePregameModelSelectionV4 {
    measurement: OpaqueMtgoDxgiMulliganVisibleHandMeasurementV3,
    request: MtgoCardAwarePregameScoringRequestV4,
    response: MtgoCardAwarePregameScoreResponseV4,
    selected_index: usize,
    selected_semantic: MtgoPregameActionSemanticV1,
    selection_commitment_sha256: String,
}

impl OpaqueMtgoCardAwarePregameModelSelectionV4 {
    pub fn selected_index_v4(&self) -> usize {
        self.selected_index
    }

    pub fn selected_semantic_v4(&self) -> &MtgoPregameActionSemanticV1 {
        &self.selected_semantic
    }

    pub fn selected_logit_f32_bits_v4(&self) -> u32 {
        self.response.logits_f32_bits[self.selected_index]
    }

    pub fn value_f32_bits_v4(&self) -> u32 {
        self.response.value_f32_bits
    }

    pub fn request_commitment_sha256_v4(&self) -> &str {
        &self.response.request_commitment_sha256
    }

    pub fn deployment_commitment_sha256_v4(&self) -> &str {
        &self.request.deployment_commitment_sha256
    }

    pub fn mulligan_measurement_commitment_sha256_v4(&self) -> &str {
        self.measurement.mulligan_measurement_commitment_sha256_v3()
    }

    pub fn visible_identity_measurement_commitment_sha256_v4(&self) -> &str {
        self.measurement
            .visible_identity_measurement_commitment_sha256_v3()
    }

    pub fn selection_commitment_sha256_v4(&self) -> &str {
        &self.selection_commitment_sha256
    }

    pub fn safe_for_semantic_evidence_v4(&self) -> bool {
        false
    }

    pub fn safe_for_observation_v5_v4(&self) -> bool {
        false
    }

    pub fn safe_for_live_input_v4(&self) -> bool {
        false
    }
}

pub fn build_card_aware_pregame_scoring_request_v4(
    measurement: &OpaqueMtgoDxgiMulliganVisibleHandMeasurementV3,
    deployment: &MtgoExpectedModelDeploymentV1,
) -> Result<MtgoCardAwarePregameScoringRequestV4, String> {
    if measurement.classification_v3() != MtgoOfflineVisibleCardIdentityClassificationV1::Match {
        return Err("card-aware pregame scoring requires seven matched identities".to_owned());
    }
    let identities = measurement.identities_v3();
    if identities.len() != 7
        || identities
            .iter()
            .enumerate()
            .any(|(ordinal, identity)| usize::from(identity.ordinal()) != ordinal)
    {
        return Err(
            "card-aware pregame scoring requires exact ordered ordinals zero through six"
                .to_owned(),
        );
    }
    let ordered_visible_card_names = identities
        .iter()
        .map(|identity| identity.visible_card_name().to_owned())
        .collect::<Vec<_>>();
    let capture = measurement.source_capture_commitments_v3();
    build_card_aware_pregame_scoring_request_from_names_v4(
        measurement.prospective_keep_size_v3(),
        &ordered_visible_card_names,
        measurement.ordered_actions_v3(),
        &capture.capture_commitment_sha256,
        measurement.mulligan_measurement_commitment_sha256_v3(),
        measurement.visible_identity_measurement_commitment_sha256_v3(),
        measurement.mulligan_profile_set_commitment_sha256_v3(),
        measurement.visible_card_profile_commitment_sha256_v3(),
        deployment,
    )
}

pub fn card_aware_pregame_scoring_request_commitment_v4(
    request: &MtgoCardAwarePregameScoringRequestV4,
) -> Result<String, String> {
    validate_card_aware_pregame_scoring_request_v4(request)?;
    canonical_json_commitment_v3(PREGAME_CARD_AWARE_SCORING_REQUEST_DOMAIN_V4, request)
}

pub fn score_and_select_card_aware_pregame_model_v4<S: MtgoExternalCardAwarePregameScorerV4>(
    measurement: OpaqueMtgoDxgiMulliganVisibleHandMeasurementV3,
    deployment: &MtgoExpectedModelDeploymentV1,
    scorer: &mut S,
) -> Result<OpaqueMtgoCardAwarePregameModelSelectionV4, String> {
    let request = build_card_aware_pregame_scoring_request_v4(&measurement, deployment)?;
    let response = scorer.score_card_aware_pregame_v4(&request)?;
    validate_card_aware_pregame_score_response_v4(measurement, deployment, response)
}

pub fn validate_card_aware_pregame_score_response_v4(
    measurement: OpaqueMtgoDxgiMulliganVisibleHandMeasurementV3,
    deployment: &MtgoExpectedModelDeploymentV1,
    response: MtgoCardAwarePregameScoreResponseV4,
) -> Result<OpaqueMtgoCardAwarePregameModelSelectionV4, String> {
    let request = build_card_aware_pregame_scoring_request_v4(&measurement, deployment)?;
    let (selected_index, selected_semantic, selection_commitment_sha256) =
        validate_card_aware_pregame_score_response_parts_v4(&request, &response)?;
    Ok(OpaqueMtgoCardAwarePregameModelSelectionV4 {
        measurement,
        request,
        response,
        selected_index,
        selected_semantic,
        selection_commitment_sha256,
    })
}

#[allow(clippy::too_many_arguments)]
fn build_card_aware_pregame_scoring_request_from_names_v4(
    prospective_keep_size: Option<u8>,
    ordered_visible_card_names: &[String],
    ordered_actions: &[MtgoPregameActionSemanticV1],
    source_capture_commitment_sha256: &str,
    mulligan_measurement_commitment_sha256: &str,
    visible_identity_measurement_commitment_sha256: &str,
    mulligan_profile_set_commitment_sha256: &str,
    visible_card_profile_commitment_sha256: &str,
    deployment: &MtgoExpectedModelDeploymentV1,
) -> Result<MtgoCardAwarePregameScoringRequestV4, String> {
    let prospective_keep_size = prospective_keep_size
        .filter(|value| (1..=7).contains(value))
        .ok_or("card-aware pregame scoring requires a keep size from one through seven")?;
    let expected_actions = [
        MtgoPregameActionSemanticV1::Mulligan {
            next_hand_size: prospective_keep_size - 1,
        },
        MtgoPregameActionSemanticV1::KeepOpeningHand,
    ];
    if ordered_actions != expected_actions {
        return Err(
            "card-aware pregame scoring requires canonical Mulligan and Keep actions".to_owned(),
        );
    }
    for digest in [
        source_capture_commitment_sha256,
        mulligan_measurement_commitment_sha256,
        visible_identity_measurement_commitment_sha256,
        mulligan_profile_set_commitment_sha256,
        visible_card_profile_commitment_sha256,
    ] {
        require_lower_sha256_v3(digest, "card-aware pregame scoring source commitment")?;
    }
    let deployment_commitment_sha256 = model_deployment_commitment_v1(deployment)
        .map_err(|error| format!("card-aware pregame model deployment: {error}"))?;
    let request = MtgoCardAwarePregameScoringRequestV4 {
        schema_version: MTGO_PREGAME_CARD_AWARE_SCORING_SCHEMA_V4,
        source_capture_commitment_sha256: source_capture_commitment_sha256.to_owned(),
        mulligan_measurement_commitment_sha256: mulligan_measurement_commitment_sha256.to_owned(),
        visible_identity_measurement_commitment_sha256:
            visible_identity_measurement_commitment_sha256.to_owned(),
        mulligan_profile_set_commitment_sha256: mulligan_profile_set_commitment_sha256.to_owned(),
        visible_card_profile_commitment_sha256: visible_card_profile_commitment_sha256.to_owned(),
        prospective_keep_size,
        ordered_visible_card_names: ordered_visible_card_names.to_vec(),
        ordered_actions: ordered_actions.to_vec(),
        deployment_commitment_sha256,
    };
    validate_card_aware_pregame_scoring_request_v4(&request)?;
    Ok(request)
}

fn validate_card_aware_pregame_scoring_request_v4(
    request: &MtgoCardAwarePregameScoringRequestV4,
) -> Result<(), String> {
    if request.schema_version != MTGO_PREGAME_CARD_AWARE_SCORING_SCHEMA_V4
        || !(1..=7).contains(&request.prospective_keep_size)
    {
        return Err("card-aware pregame request schema or keep size is invalid".to_owned());
    }
    if request.ordered_visible_card_names.len() != 7
        || request.ordered_visible_card_names.iter().any(|name| {
            name.is_empty()
                || name.len() > 256
                || name.trim() != name
                || name.chars().any(char::is_control)
        })
    {
        return Err(
            "card-aware pregame request requires seven bounded visible card names".to_owned(),
        );
    }
    let expected_actions = [
        MtgoPregameActionSemanticV1::Mulligan {
            next_hand_size: request.prospective_keep_size - 1,
        },
        MtgoPregameActionSemanticV1::KeepOpeningHand,
    ];
    if request.ordered_actions != expected_actions {
        return Err("card-aware pregame request actions are not canonical".to_owned());
    }
    for digest in [
        &request.source_capture_commitment_sha256,
        &request.mulligan_measurement_commitment_sha256,
        &request.visible_identity_measurement_commitment_sha256,
        &request.mulligan_profile_set_commitment_sha256,
        &request.visible_card_profile_commitment_sha256,
        &request.deployment_commitment_sha256,
    ] {
        require_lower_sha256_v3(digest, "card-aware pregame request commitment")?;
    }
    Ok(())
}

fn validate_card_aware_pregame_score_response_parts_v4(
    request: &MtgoCardAwarePregameScoringRequestV4,
    response: &MtgoCardAwarePregameScoreResponseV4,
) -> Result<(usize, MtgoPregameActionSemanticV1, String), String> {
    let request_commitment_sha256 = card_aware_pregame_scoring_request_commitment_v4(request)?;
    if response.schema_version != MTGO_PREGAME_CARD_AWARE_SCORING_SCHEMA_V4
        || response.request_commitment_sha256 != request_commitment_sha256
    {
        return Err("card-aware pregame response does not bind the exact request".to_owned());
    }
    require_lower_sha256_v3(
        &response.request_commitment_sha256,
        "card-aware pregame response request commitment",
    )?;
    if response.logits_f32_bits.len() != request.ordered_actions.len()
        || response.logits_f32_bits.is_empty()
    {
        return Err("card-aware pregame response logit count is invalid".to_owned());
    }
    let logits = response
        .logits_f32_bits
        .iter()
        .map(|bits| f32::from_bits(*bits))
        .collect::<Vec<_>>();
    if logits.iter().any(|value| !value.is_finite())
        || !f32::from_bits(response.value_f32_bits).is_finite()
    {
        return Err("card-aware pregame response must contain finite values".to_owned());
    }
    let mut selected_index = 0;
    for index in 1..logits.len() {
        if logits[index].total_cmp(&logits[selected_index]).is_gt() {
            selected_index = index;
        }
    }
    let selected_semantic = request.ordered_actions[selected_index].clone();
    #[derive(Serialize)]
    struct SelectionRecordV4<'a> {
        request: &'a MtgoCardAwarePregameScoringRequestV4,
        response: &'a MtgoCardAwarePregameScoreResponseV4,
        selected_index: usize,
        selected_semantic: &'a MtgoPregameActionSemanticV1,
    }
    let selection_commitment_sha256 = canonical_json_commitment_v3(
        PREGAME_CARD_AWARE_MODEL_SELECTION_DOMAIN_V4,
        &SelectionRecordV4 {
            request,
            response,
            selected_index,
            selected_semantic: &selected_semantic,
        },
    )?;
    Ok((
        selected_index,
        selected_semantic,
        selection_commitment_sha256,
    ))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "postcondition_kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum MtgoPlannedPregamePostconditionV3 {
    NextMulliganPrompt { prospective_keep_size: u8 },
    LondonBottoming { required_bottom_count: u8 },
    GameplayFirstMain,
}

#[derive(Clone, Copy, Serialize)]
struct ClientPointV3 {
    x: u32,
    y: u32,
}

struct PregameActionPlanPartsV3 {
    control_profile_commitment_sha256: String,
    #[allow(dead_code)]
    control_id: &'static str,
    #[allow(dead_code)]
    control_rect_client_px: MtgoRectPxV1,
    #[allow(dead_code)]
    target_point_client_px: ClientPointV3,
    observed_control_region_sha256: String,
    source_transition_identity_sha256: String,
    planned_postcondition: MtgoPlannedPregamePostconditionV3,
    action_plan_commitment_sha256: String,
}

/// A coordinate-private plan for one scorer-selected pregame
/// control. The plan binds the exact source capture, measured prompt, scorer
/// selection, fixed client layout, observed control pixels, and required
/// visible postcondition. It cannot perform or authorize input by itself.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPregameActionPlanV3;
/// let _forged = OpaqueMtgoPregameActionPlanV3 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPregameActionPlanV3;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoPregameActionPlanV3>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPregameActionPlanV3;
/// fn coordinate_escape(value: &OpaqueMtgoPregameActionPlanV3) {
///     let _ = value.target_point_client_px_v3();
/// }
/// ```
pub struct OpaqueMtgoPregameActionPlanV3 {
    selection: PregameActionPlanSelectionV3,
    parts: PregameActionPlanPartsV3,
}

enum PregameActionPlanSelectionV3 {
    PromptOnly(Box<OpaqueMtgoPregameModelSelectionV3>),
    CardAware(Box<OpaqueMtgoCardAwarePregameModelSelectionV4>),
}

impl PregameActionPlanSelectionV3 {
    fn measurement_v3(&self) -> &OpaqueMtgoDxgiMulliganMeasurementV3 {
        match self {
            Self::PromptOnly(selection) => &selection.measurement,
            Self::CardAware(selection) => &selection.measurement.source,
        }
    }

    fn selected_semantic_v3(&self) -> &MtgoPregameActionSemanticV1 {
        match self {
            Self::PromptOnly(selection) => selection.selected_semantic_v3(),
            Self::CardAware(selection) => selection.selected_semantic_v4(),
        }
    }

    fn selection_commitment_sha256_v3(&self) -> &str {
        match self {
            Self::PromptOnly(selection) => selection.selection_commitment_sha256_v3(),
            Self::CardAware(selection) => selection.selection_commitment_sha256_v4(),
        }
    }
}

pub(crate) struct PreparedPregameActuationV3 {
    pub hwnd: u64,
    pub process_id: u32,
    pub process_start_filetime_100ns: u64,
    pub dpi: u32,
    pub client_rect_desktop_px: SignedRectV1,
    pub target_x_desktop_px: i32,
    pub target_y_desktop_px: i32,
    pub park_x_desktop_px: i32,
    pub park_y_desktop_px: i32,
    pub current_capture_commitment_sha256: String,
    pub current_captured_at_unix_millis: u128,
    pub action_plan_commitment_sha256: String,
    pub selected_semantic: MtgoPregameActionSemanticV1,
    pub planned_postcondition: MtgoPlannedPregamePostconditionV3,
}

impl OpaqueMtgoPregameActionPlanV3 {
    pub fn selected_semantic_v3(&self) -> &MtgoPregameActionSemanticV1 {
        self.selection.selected_semantic_v3()
    }

    pub fn planned_postcondition_v3(&self) -> &MtgoPlannedPregamePostconditionV3 {
        &self.parts.planned_postcondition
    }

    pub fn control_profile_commitment_sha256_v3(&self) -> &str {
        &self.parts.control_profile_commitment_sha256
    }

    pub fn observed_control_region_sha256_v3(&self) -> &str {
        &self.parts.observed_control_region_sha256
    }

    pub fn source_capture_commitment_sha256_v3(&self) -> &str {
        &self
            .selection
            .measurement_v3()
            .source_frame
            .capture_commitment_sha256
    }

    pub fn selection_commitment_sha256_v3(&self) -> &str {
        self.selection.selection_commitment_sha256_v3()
    }

    pub fn action_plan_commitment_sha256_v3(&self) -> &str {
        &self.parts.action_plan_commitment_sha256
    }

    pub fn safe_for_live_input_v3(&self) -> bool {
        false
    }

    pub fn safe_for_purchase_v3(&self) -> bool {
        false
    }

    pub fn safe_for_queue_entry_v3(&self) -> bool {
        false
    }
}

/// A visible next-prompt confirmation for one planned Mulligan transition.
/// This type proves request binding and exact prompt progression only. It does
/// not prove that a particular input caused the transition and cannot enable a
/// later input.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoConfirmedMulliganTransitionV3;
/// let _forged = OpaqueMtgoConfirmedMulliganTransitionV3 {};
/// ```
pub struct OpaqueMtgoConfirmedMulliganTransitionV3 {
    plan: OpaqueMtgoPregameActionPlanV3,
    after: OpaqueMtgoDxgiMulliganMeasurementV3,
    resulting_prospective_keep_size: u8,
    confirmation_commitment_sha256: String,
}

impl OpaqueMtgoConfirmedMulliganTransitionV3 {
    pub fn selected_semantic_v3(&self) -> &MtgoPregameActionSemanticV1 {
        self.plan.selected_semantic_v3()
    }

    pub fn resulting_prospective_keep_size_v3(&self) -> u8 {
        self.resulting_prospective_keep_size
    }

    pub fn source_action_plan_commitment_sha256_v3(&self) -> &str {
        self.plan.action_plan_commitment_sha256_v3()
    }

    pub fn resulting_measurement_commitment_sha256_v3(&self) -> &str {
        self.after.measurement_commitment_sha256_v3()
    }

    pub fn confirmation_commitment_sha256_v3(&self) -> &str {
        &self.confirmation_commitment_sha256
    }

    pub fn safe_for_live_input_v3(&self) -> bool {
        false
    }
}

/// An exact visible confirmation that a planned one-card Keep reached the
/// reviewed bottom-six, zero-selected prompt. It cannot confirm other bottom
/// counts and cannot enable input by itself.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoConfirmedKeepToBottomSixTransitionV3;
/// let _forged = OpaqueMtgoConfirmedKeepToBottomSixTransitionV3 {};
/// ```
pub struct OpaqueMtgoConfirmedKeepToBottomSixTransitionV3 {
    plan: OpaqueMtgoPregameActionPlanV3,
    after: OpaqueMtgoDxgiBottomSixInitialMeasurementV3,
    confirmation_commitment_sha256: String,
}

impl OpaqueMtgoConfirmedKeepToBottomSixTransitionV3 {
    pub fn selected_semantic_v3(&self) -> &MtgoPregameActionSemanticV1 {
        self.plan.selected_semantic_v3()
    }

    pub fn required_bottom_count_v3(&self) -> u8 {
        6
    }

    pub fn selected_count_v3(&self) -> u8 {
        0
    }

    pub fn source_action_plan_commitment_sha256_v3(&self) -> &str {
        self.plan.action_plan_commitment_sha256_v3()
    }

    pub fn resulting_measurement_commitment_sha256_v3(&self) -> &str {
        self.after.measurement_commitment_sha256_v3()
    }

    pub fn confirmation_commitment_sha256_v3(&self) -> &str {
        &self.confirmation_commitment_sha256
    }

    pub fn safe_for_live_input_v3(&self) -> bool {
        false
    }
}

/// An exact visible Turn 1 first-main confirmation for a planned seven-card
/// Keep. It does not prove that a particular input caused the transition and
/// cannot enable a later input.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoConfirmedKeepToFirstMainTransitionV3;
/// let _forged = OpaqueMtgoConfirmedKeepToFirstMainTransitionV3 {};
/// ```
pub struct OpaqueMtgoConfirmedKeepToFirstMainTransitionV3 {
    plan: OpaqueMtgoPregameActionPlanV3,
    after: OpaqueMtgoDxgiFirstMainMeasurementV3,
    confirmation_commitment_sha256: String,
}

impl OpaqueMtgoConfirmedKeepToFirstMainTransitionV3 {
    pub fn selected_semantic_v3(&self) -> &MtgoPregameActionSemanticV1 {
        self.plan.selected_semantic_v3()
    }

    pub fn source_action_plan_commitment_sha256_v3(&self) -> &str {
        self.plan.action_plan_commitment_sha256_v3()
    }

    pub fn resulting_measurement_commitment_sha256_v3(&self) -> &str {
        self.after.measurement_commitment_sha256_v3()
    }

    pub fn confirmation_commitment_sha256_v3(&self) -> &str {
        &self.confirmation_commitment_sha256
    }

    pub fn safe_for_live_input_v3(&self) -> bool {
        false
    }
}

pub fn build_pregame_action_plan_v3(
    selection: OpaqueMtgoPregameModelSelectionV3,
) -> Result<OpaqueMtgoPregameActionPlanV3, String> {
    let measurement = &selection.measurement;
    let capture = measurement.source_capture_commitments_v3();
    let source_transition_identity_sha256 =
        pregame_transition_identity_commitment_v3(&measurement.source_frame.manifest)?;
    let parts = build_pregame_action_plan_parts_v3(
        selection.selected_semantic_v3(),
        measurement.prospective_keep_size_v3(),
        measurement.profile_set_commitment_sha256_v3(),
        selection.selection_commitment_sha256_v3(),
        measurement.measurement_commitment_sha256_v3(),
        &capture,
        &source_transition_identity_sha256,
        &measurement.source_frame.canonical_bgra8,
    )?;
    Ok(OpaqueMtgoPregameActionPlanV3 {
        selection: PregameActionPlanSelectionV3::PromptOnly(Box::new(selection)),
        parts,
    })
}

/// Builds the same coordinate-private pregame plan from a card-aware v4
/// selection. The v4 selection commitment already binds all seven visible
/// labels and the exact scorer response. This function adds no input authority.
pub fn build_card_aware_pregame_action_plan_v4(
    selection: OpaqueMtgoCardAwarePregameModelSelectionV4,
) -> Result<OpaqueMtgoPregameActionPlanV3, String> {
    let measurement = &selection.measurement;
    let source = &measurement.source;
    let capture = source.source_capture_commitments_v3();
    let source_transition_identity_sha256 =
        pregame_transition_identity_commitment_v3(&source.source_frame.manifest)?;
    let parts = build_pregame_action_plan_parts_v3(
        selection.selected_semantic_v4(),
        measurement.prospective_keep_size_v3(),
        measurement.mulligan_profile_set_commitment_sha256_v3(),
        selection.selection_commitment_sha256_v4(),
        measurement.mulligan_measurement_commitment_sha256_v3(),
        &capture,
        &source_transition_identity_sha256,
        &source.source_frame.canonical_bgra8,
    )?;
    Ok(OpaqueMtgoPregameActionPlanV3 {
        selection: PregameActionPlanSelectionV3::CardAware(Box::new(selection)),
        parts,
    })
}

pub(crate) fn prepare_pregame_actuation_v3(
    plan: &OpaqueMtgoPregameActionPlanV3,
    account_alias: &str,
) -> Result<PreparedPregameActuationV3, String> {
    if account_alias.is_empty()
        || account_alias.len() > 64
        || account_alias.chars().any(char::is_control)
    {
        return Err("the visible account alias is invalid".to_owned());
    }
    let source = &plan.selection.measurement_v3().source_frame;
    let expected_title = format!("(Solitaire): Freeform: Vs. {account_alias}");
    if source.manifest.pre.title != expected_title
        || source.manifest.post.title != expected_title
        || source.manifest.window_mode != "solitaire_game"
        || source.manifest.capture_role != "acting_player_solitaire"
        || source.manifest.expected_game_format != "Freeform"
    {
        return Err(
            "the source plan is not bound to the exact visible authorized Solitaire account"
                .to_owned(),
        );
    }

    let request = MtgoDxgiCaptureRequestV3 {
        expected_executable_sha256: source.manifest.pre.executable_sha256.clone(),
        expected_signer_thumbprint: source.manifest.pre.signer_thumbprint.clone(),
        expected_signer_subject_sha256: source.manifest.pre.signer_subject_sha256.clone(),
        window_mode: CaptureWindowModeV2::SolitaireGame,
        expected_game_format: Some("Freeform".to_owned()),
        expected_title_contains: Some(expected_title),
        timeout_ms: 1_500,
    };
    let current_frame = capture_mtgo_dxgi_frame_candidate_v3(request)?;
    let current_identity = pregame_transition_identity_commitment_v3(&current_frame.manifest)?;
    let current_measurement = measure_mulligan_ladder_parts_v3(
        &serialize_manifest_v2(&current_frame.manifest)?,
        &current_frame.canonical_bgra8,
        &current_frame.preview_png,
    )?;
    let source_measurement = &plan.selection.measurement_v3().measurement;
    if current_measurement.classification() != MtgoOfflineMulliganLadderClassificationV1::Match
        || current_measurement.prospective_keep_size() != source_measurement.prospective_keep_size()
        || current_measurement.ordered_actions() != source_measurement.ordered_actions()
        || current_measurement.profile_set_commitment_sha256()
            != source_measurement.profile_set_commitment_sha256()
        || current_identity != plan.parts.source_transition_identity_sha256
        || current_frame.manifest.captured_at_unix_millis <= source.manifest.captured_at_unix_millis
    {
        return Err(
            "the immediate visible prompt, legal actions, identity, or capture order changed"
                .to_owned(),
        );
    }
    if let PregameActionPlanSelectionV3::CardAware(selection) = &plan.selection {
        let source_card_measurement = &selection.measurement.measurement;
        let checked_current = check_untrusted_dxgi_capture_artifact_v1(
            &serialize_manifest_v2(&current_frame.manifest)?,
            &current_frame.canonical_bgra8,
            &current_frame.preview_png,
        )
        .map_err(|error| format!("check immediate card-aware pregame capture: {error}"))?;
        let current_card_measurement =
            classify_untrusted_offline_mulligan_visible_card_identities_v1(
                &checked_current,
                &current_frame.canonical_bgra8,
                &selection.measurement.profile,
            )
            .map_err(|error| format!("classify immediate card-aware pregame hand: {error}"))?;
        if current_card_measurement.classification()
            != MtgoOfflineVisibleCardIdentityClassificationV1::Match
            || current_card_measurement.prospective_keep_size()
                != source_card_measurement.prospective_keep_size()
            || current_card_measurement.profile_commitment_sha256()
                != source_card_measurement.profile_commitment_sha256()
            || current_card_measurement.identities() != source_card_measurement.identities()
        {
            return Err(
                "the immediate visible card identities changed after card-aware scoring".to_owned(),
            );
        }
    }

    let current_control_region_sha256 = hash_bgra_region_for_plan_v3(
        &current_frame.canonical_bgra8,
        &MtgoSizePxV1 {
            width: current_frame.manifest.frame.canonical_width,
            height: current_frame.manifest.frame.canonical_height,
        },
        &plan.parts.control_rect_client_px,
    )?;
    if current_control_region_sha256 != plan.parts.observed_control_region_sha256 {
        return Err("the selected control pixels changed before input".to_owned());
    }

    let client_rect = current_frame.manifest.pre.client_rect_desktop_px;
    let target_x_desktop_px = client_rect
        .left
        .checked_add(
            i32::try_from(plan.parts.target_point_client_px.x)
                .map_err(|_| "pregame target x does not fit the desktop")?,
        )
        .ok_or("pregame target x overflow")?;
    let target_y_desktop_px = client_rect
        .top
        .checked_add(
            i32::try_from(plan.parts.target_point_client_px.y)
                .map_err(|_| "pregame target y does not fit the desktop")?,
        )
        .ok_or("pregame target y overflow")?;
    if !client_rect.contains_point(target_x_desktop_px, target_y_desktop_px) {
        return Err("pregame target point is outside the current client".to_owned());
    }
    let (park_x_desktop_px, park_y_desktop_px) = choose_cursor_park_point_v3(
        &client_rect,
        &current_frame.manifest.output.bounds_desktop_px,
    )?;

    Ok(PreparedPregameActuationV3 {
        hwnd: current_frame.manifest.pre.hwnd,
        process_id: current_frame.manifest.pre.process_id,
        process_start_filetime_100ns: current_frame.manifest.pre.process_start_filetime_100ns,
        dpi: current_frame.manifest.pre.dpi,
        client_rect_desktop_px: client_rect,
        target_x_desktop_px,
        target_y_desktop_px,
        park_x_desktop_px,
        park_y_desktop_px,
        current_capture_commitment_sha256: current_frame.capture_commitment_sha256,
        current_captured_at_unix_millis: current_frame.manifest.captured_at_unix_millis,
        action_plan_commitment_sha256: plan.parts.action_plan_commitment_sha256.clone(),
        selected_semantic: plan.selection.selected_semantic_v3().clone(),
        planned_postcondition: plan.parts.planned_postcondition.clone(),
    })
}

pub(crate) fn choose_cursor_park_point_v3(
    client: &SignedRectV1,
    output: &SignedRectV1,
) -> Result<(i32, i32), String> {
    let right = output
        .right
        .checked_sub(2)
        .ok_or("capture output has no cursor parking width")?;
    let bottom = output
        .bottom
        .checked_sub(2)
        .ok_or("capture output has no cursor parking height")?;
    let left = output
        .left
        .checked_add(1)
        .ok_or("capture output cursor parking x overflow")?;
    let top = output
        .top
        .checked_add(1)
        .ok_or("capture output cursor parking y overflow")?;
    for (x, y) in [(left, top), (right, top), (left, bottom), (right, bottom)] {
        if output.contains_point(x, y) && !client.contains_point(x, y) {
            return Ok((x, y));
        }
    }
    Err("the current output has no cursor parking point outside the client".to_owned())
}

pub fn confirm_pregame_mulligan_transition_v3(
    plan: OpaqueMtgoPregameActionPlanV3,
    after: OpaqueMtgoDxgiMulliganMeasurementV3,
) -> Result<OpaqueMtgoConfirmedMulliganTransitionV3, String> {
    let source_capture = plan
        .selection
        .measurement_v3()
        .source_capture_commitments_v3();
    let after_capture = after.source_capture_commitments_v3();
    let after_transition_identity_sha256 =
        pregame_transition_identity_commitment_v3(&after.source_frame.manifest)?;
    let confirmation_commitment_sha256 = validate_mulligan_postcondition_parts_v3(
        plan.action_plan_commitment_sha256_v3(),
        &plan.parts.planned_postcondition,
        &source_capture,
        &plan.parts.source_transition_identity_sha256,
        after.classification_v3(),
        after.prospective_keep_size_v3(),
        after.profile_set_commitment_sha256_v3(),
        after.measurement_commitment_sha256_v3(),
        &after_capture,
        &after_transition_identity_sha256,
    )?;
    let resulting_prospective_keep_size = after
        .prospective_keep_size_v3()
        .ok_or("confirmed next prompt did not retain its prospective keep size")?;
    Ok(OpaqueMtgoConfirmedMulliganTransitionV3 {
        plan,
        after,
        resulting_prospective_keep_size,
        confirmation_commitment_sha256,
    })
}

pub fn confirm_pregame_keep_to_bottom_six_transition_v3(
    plan: OpaqueMtgoPregameActionPlanV3,
    after: OpaqueMtgoDxgiBottomSixInitialMeasurementV3,
) -> Result<OpaqueMtgoConfirmedKeepToBottomSixTransitionV3, String> {
    let source_capture = plan
        .selection
        .measurement_v3()
        .source_capture_commitments_v3();
    let after_capture = after.source_capture_commitments_v3();
    let after_transition_identity_sha256 =
        pregame_transition_identity_commitment_v3(&after.source_frame.manifest)?;
    let confirmation_commitment_sha256 = validate_keep_bottom_six_postcondition_parts_v3(
        plan.action_plan_commitment_sha256_v3(),
        &plan.parts.planned_postcondition,
        &source_capture,
        &plan.parts.source_transition_identity_sha256,
        after.classification_v3(),
        after.required_bottom_count_v3(),
        after.selected_count_v3(),
        after.profile_commitment_sha256_v3(),
        after.measurement_commitment_sha256_v3(),
        &after_capture,
        &after_transition_identity_sha256,
    )?;
    Ok(OpaqueMtgoConfirmedKeepToBottomSixTransitionV3 {
        plan,
        after,
        confirmation_commitment_sha256,
    })
}

pub fn confirm_pregame_keep_to_first_main_transition_v3(
    plan: OpaqueMtgoPregameActionPlanV3,
    after: OpaqueMtgoDxgiFirstMainMeasurementV3,
) -> Result<OpaqueMtgoConfirmedKeepToFirstMainTransitionV3, String> {
    let source_capture = plan
        .selection
        .measurement_v3()
        .source_capture_commitments_v3();
    let after_capture = after.source_capture_commitments_v3();
    let after_transition_identity_sha256 =
        pregame_transition_identity_commitment_v3(&after.source_frame.manifest)?;
    let confirmation_commitment_sha256 = validate_keep_first_main_postcondition_parts_v3(
        plan.action_plan_commitment_sha256_v3(),
        &plan.parts.planned_postcondition,
        &source_capture,
        &plan.parts.source_transition_identity_sha256,
        after.classification_v3(),
        after.profile_commitment_sha256_v3(),
        after.measurement_commitment_sha256_v3(),
        &after_capture,
        &after_transition_identity_sha256,
    )?;
    Ok(OpaqueMtgoConfirmedKeepToFirstMainTransitionV3 {
        plan,
        after,
        confirmation_commitment_sha256,
    })
}

#[allow(clippy::too_many_arguments)]
fn build_pregame_action_plan_parts_v3(
    selected_semantic: &MtgoPregameActionSemanticV1,
    prospective_keep_size: Option<u8>,
    ladder_profile_commitment_sha256: &str,
    selection_commitment_sha256: &str,
    measurement_commitment_sha256: &str,
    source_capture: &MtgoDxgiFrameCommitmentsV3,
    source_transition_identity_sha256: &str,
    canonical_bgra8: &[u8],
) -> Result<PregameActionPlanPartsV3, String> {
    let prospective_keep_size = prospective_keep_size
        .filter(|value| (1..=7).contains(value))
        .ok_or("pregame action plan requires an exact prospective keep size")?;
    if ladder_profile_commitment_sha256 != PREGAME_MULLIGAN_LADDER_PROFILE_COMMITMENT_V3 {
        return Err("pregame action plan requires the reviewed ladder profile".to_owned());
    }
    for digest in [
        selection_commitment_sha256,
        measurement_commitment_sha256,
        &source_capture.capture_commitment_sha256,
        &source_capture.canonical_bgra8_sha256,
        source_transition_identity_sha256,
    ] {
        require_lower_sha256_v3(digest, "pregame action plan commitment")?;
    }
    let client_size_px = MtgoSizePxV1 {
        width: source_capture.canonical_width,
        height: source_capture.canonical_height,
    };
    if client_size_px.width != 1_550 || client_size_px.height != 925 {
        return Err("pregame action plan requires the reviewed 1550 by 925 client".to_owned());
    }
    let expected_len = usize::try_from(client_size_px.width)
        .ok()
        .and_then(|width| {
            usize::try_from(client_size_px.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("pregame action plan pixel length overflow")?;
    if canonical_bgra8.len() != expected_len
        || format!("{:x}", Sha256::digest(canonical_bgra8)) != source_capture.canonical_bgra8_sha256
    {
        return Err("pregame action plan pixels do not bind the source capture".to_owned());
    }

    let (control_id, control_rect_client_px, planned_postcondition) = match selected_semantic {
        MtgoPregameActionSemanticV1::Mulligan { next_hand_size }
            if next_hand_size.checked_add(1) == Some(prospective_keep_size)
                && *next_hand_size >= 1 =>
        {
            (
                "mulligan",
                MtgoRectPxV1 {
                    x: 29,
                    y: 153,
                    width: 82,
                    height: 33,
                },
                MtgoPlannedPregamePostconditionV3::NextMulliganPrompt {
                    prospective_keep_size: *next_hand_size,
                },
            )
        }
        MtgoPregameActionSemanticV1::Mulligan { next_hand_size: 0 }
            if prospective_keep_size == 1 =>
        {
            return Err(
                "zero-card Mulligan is not plannable until its visible prompt is calibrated"
                    .to_owned(),
            );
        }
        MtgoPregameActionSemanticV1::KeepOpeningHand => {
            let postcondition = if prospective_keep_size == 7 {
                MtgoPlannedPregamePostconditionV3::GameplayFirstMain
            } else {
                MtgoPlannedPregamePostconditionV3::LondonBottoming {
                    required_bottom_count: 7 - prospective_keep_size,
                }
            };
            (
                "keep",
                MtgoRectPxV1 {
                    x: 116,
                    y: 153,
                    width: 55,
                    height: 33,
                },
                postcondition,
            )
        }
        _ => {
            return Err(
                "selected pregame semantic does not match the measured prospective keep size"
                    .to_owned(),
            );
        }
    };
    let target_point_client_px = ClientPointV3 {
        x: control_rect_client_px.x + control_rect_client_px.width / 2,
        y: control_rect_client_px.y + control_rect_client_px.height / 2,
    };
    let observed_control_region_sha256 =
        hash_bgra_region_for_plan_v3(canonical_bgra8, &client_size_px, &control_rect_client_px)?;
    let control_profile_commitment_sha256 = pregame_control_profile_commitment_v3()?;

    #[derive(Serialize)]
    struct ActionPlanRecordV3<'a> {
        schema_version: u32,
        control_profile_commitment_sha256: &'a str,
        ladder_profile_commitment_sha256: &'a str,
        selection_commitment_sha256: &'a str,
        measurement_commitment_sha256: &'a str,
        source_capture_commitment_sha256: &'a str,
        source_transition_identity_sha256: &'a str,
        prospective_keep_size: u8,
        selected_semantic: &'a MtgoPregameActionSemanticV1,
        control_id: &'a str,
        control_rect_client_px: &'a MtgoRectPxV1,
        target_point_client_px: ClientPointV3,
        observed_control_region_sha256: &'a str,
        planned_postcondition: &'a MtgoPlannedPregamePostconditionV3,
    }
    let action_plan_commitment_sha256 = canonical_json_commitment_v3(
        PREGAME_ACTION_PLAN_DOMAIN_V3,
        &ActionPlanRecordV3 {
            schema_version: MTGO_PREGAME_EXTERNAL_SCORING_SCHEMA_V3,
            control_profile_commitment_sha256: &control_profile_commitment_sha256,
            ladder_profile_commitment_sha256,
            selection_commitment_sha256,
            measurement_commitment_sha256,
            source_capture_commitment_sha256: &source_capture.capture_commitment_sha256,
            source_transition_identity_sha256,
            prospective_keep_size,
            selected_semantic,
            control_id,
            control_rect_client_px: &control_rect_client_px,
            target_point_client_px,
            observed_control_region_sha256: &observed_control_region_sha256,
            planned_postcondition: &planned_postcondition,
        },
    )?;
    Ok(PregameActionPlanPartsV3 {
        control_profile_commitment_sha256,
        control_id,
        control_rect_client_px,
        target_point_client_px,
        observed_control_region_sha256,
        source_transition_identity_sha256: source_transition_identity_sha256.to_owned(),
        planned_postcondition,
        action_plan_commitment_sha256,
    })
}

#[allow(clippy::too_many_arguments)]
fn validate_mulligan_postcondition_parts_v3(
    action_plan_commitment_sha256: &str,
    planned_postcondition: &MtgoPlannedPregamePostconditionV3,
    source_capture: &MtgoDxgiFrameCommitmentsV3,
    source_transition_identity_sha256: &str,
    after_classification: MtgoOfflineMulliganLadderClassificationV1,
    after_prospective_keep_size: Option<u8>,
    after_profile_commitment_sha256: &str,
    after_measurement_commitment_sha256: &str,
    after_capture: &MtgoDxgiFrameCommitmentsV3,
    after_transition_identity_sha256: &str,
) -> Result<String, String> {
    let MtgoPlannedPregamePostconditionV3::NextMulliganPrompt {
        prospective_keep_size,
    } = planned_postcondition
    else {
        return Err(
            "Keep postconditions require a dedicated bottoming or first-main classifier".to_owned(),
        );
    };
    if after_classification != MtgoOfflineMulliganLadderClassificationV1::Match
        || after_prospective_keep_size != Some(*prospective_keep_size)
        || after_profile_commitment_sha256 != PREGAME_MULLIGAN_LADDER_PROFILE_COMMITMENT_V3
    {
        return Err("the visible next prompt does not match the planned Mulligan".to_owned());
    }
    for digest in [
        action_plan_commitment_sha256,
        source_transition_identity_sha256,
        after_profile_commitment_sha256,
        after_measurement_commitment_sha256,
        &source_capture.capture_commitment_sha256,
        &source_capture.canonical_bgra8_sha256,
        &after_capture.capture_commitment_sha256,
        &after_capture.canonical_bgra8_sha256,
        after_transition_identity_sha256,
    ] {
        require_lower_sha256_v3(digest, "pregame Mulligan confirmation commitment")?;
    }
    validate_pregame_capture_progression_v3(
        source_capture,
        source_transition_identity_sha256,
        after_capture,
        after_transition_identity_sha256,
    )?;

    #[derive(Serialize)]
    struct ConfirmationRecordV3<'a> {
        action_plan_commitment_sha256: &'a str,
        source_capture_commitment_sha256: &'a str,
        source_transition_identity_sha256: &'a str,
        planned_postcondition: &'a MtgoPlannedPregamePostconditionV3,
        after_capture_commitment_sha256: &'a str,
        after_measurement_commitment_sha256: &'a str,
        after_transition_identity_sha256: &'a str,
    }
    canonical_json_commitment_v3(
        PREGAME_MULLIGAN_CONFIRMATION_DOMAIN_V3,
        &ConfirmationRecordV3 {
            action_plan_commitment_sha256,
            source_capture_commitment_sha256: &source_capture.capture_commitment_sha256,
            source_transition_identity_sha256,
            planned_postcondition,
            after_capture_commitment_sha256: &after_capture.capture_commitment_sha256,
            after_measurement_commitment_sha256,
            after_transition_identity_sha256,
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn validate_keep_bottom_six_postcondition_parts_v3(
    action_plan_commitment_sha256: &str,
    planned_postcondition: &MtgoPlannedPregamePostconditionV3,
    source_capture: &MtgoDxgiFrameCommitmentsV3,
    source_transition_identity_sha256: &str,
    after_classification: MtgoOfflineBottomSixInitialClassificationV1,
    after_required_bottom_count: Option<u8>,
    after_selected_count: Option<u8>,
    after_profile_commitment_sha256: &str,
    after_measurement_commitment_sha256: &str,
    after_capture: &MtgoDxgiFrameCommitmentsV3,
    after_transition_identity_sha256: &str,
) -> Result<String, String> {
    if planned_postcondition
        != &(MtgoPlannedPregamePostconditionV3::LondonBottoming {
            required_bottom_count: 6,
        })
    {
        return Err("only a one-card Keep can use the current bottom-six confirmation".to_owned());
    }
    if after_classification != MtgoOfflineBottomSixInitialClassificationV1::Match
        || after_required_bottom_count != Some(6)
        || after_selected_count != Some(0)
        || after_profile_commitment_sha256 != PREGAME_BOTTOM_SIX_INITIAL_PROFILE_COMMITMENT_V3
    {
        return Err(
            "the visible result is not the exact reviewed bottom-six initial state".to_owned(),
        );
    }
    for digest in [
        action_plan_commitment_sha256,
        source_transition_identity_sha256,
        after_profile_commitment_sha256,
        after_measurement_commitment_sha256,
        &source_capture.capture_commitment_sha256,
        &source_capture.canonical_bgra8_sha256,
        &after_capture.capture_commitment_sha256,
        &after_capture.canonical_bgra8_sha256,
        after_transition_identity_sha256,
    ] {
        require_lower_sha256_v3(digest, "pregame bottom-six confirmation commitment")?;
    }
    validate_pregame_capture_progression_v3(
        source_capture,
        source_transition_identity_sha256,
        after_capture,
        after_transition_identity_sha256,
    )?;

    #[derive(Serialize)]
    struct ConfirmationRecordV3<'a> {
        action_plan_commitment_sha256: &'a str,
        source_capture_commitment_sha256: &'a str,
        source_transition_identity_sha256: &'a str,
        planned_postcondition: &'a MtgoPlannedPregamePostconditionV3,
        after_capture_commitment_sha256: &'a str,
        after_profile_commitment_sha256: &'a str,
        after_measurement_commitment_sha256: &'a str,
        after_transition_identity_sha256: &'a str,
    }
    canonical_json_commitment_v3(
        PREGAME_KEEP_BOTTOM_SIX_CONFIRMATION_DOMAIN_V3,
        &ConfirmationRecordV3 {
            action_plan_commitment_sha256,
            source_capture_commitment_sha256: &source_capture.capture_commitment_sha256,
            source_transition_identity_sha256,
            planned_postcondition,
            after_capture_commitment_sha256: &after_capture.capture_commitment_sha256,
            after_profile_commitment_sha256,
            after_measurement_commitment_sha256,
            after_transition_identity_sha256,
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn validate_keep_first_main_postcondition_parts_v3(
    action_plan_commitment_sha256: &str,
    planned_postcondition: &MtgoPlannedPregamePostconditionV3,
    source_capture: &MtgoDxgiFrameCommitmentsV3,
    source_transition_identity_sha256: &str,
    after_classification: MtgoOfflineFirstMainClassificationV1,
    after_profile_commitment_sha256: &str,
    after_measurement_commitment_sha256: &str,
    after_capture: &MtgoDxgiFrameCommitmentsV3,
    after_transition_identity_sha256: &str,
) -> Result<String, String> {
    if planned_postcondition != &MtgoPlannedPregamePostconditionV3::GameplayFirstMain {
        return Err(
            "only a seven-card Keep can use the current first-main confirmation".to_owned(),
        );
    }
    if after_classification != MtgoOfflineFirstMainClassificationV1::Match {
        return Err("the visible result is not the exact reviewed Turn 1 first main".to_owned());
    }
    for digest in [
        action_plan_commitment_sha256,
        source_transition_identity_sha256,
        after_profile_commitment_sha256,
        after_measurement_commitment_sha256,
        &source_capture.capture_commitment_sha256,
        &source_capture.canonical_bgra8_sha256,
        &after_capture.capture_commitment_sha256,
        &after_capture.canonical_bgra8_sha256,
        after_transition_identity_sha256,
    ] {
        require_lower_sha256_v3(digest, "pregame Keep confirmation commitment")?;
    }
    validate_pregame_capture_progression_v3(
        source_capture,
        source_transition_identity_sha256,
        after_capture,
        after_transition_identity_sha256,
    )?;

    #[derive(Serialize)]
    struct ConfirmationRecordV3<'a> {
        action_plan_commitment_sha256: &'a str,
        source_capture_commitment_sha256: &'a str,
        source_transition_identity_sha256: &'a str,
        planned_postcondition: &'a MtgoPlannedPregamePostconditionV3,
        after_capture_commitment_sha256: &'a str,
        after_profile_commitment_sha256: &'a str,
        after_measurement_commitment_sha256: &'a str,
        after_transition_identity_sha256: &'a str,
    }
    canonical_json_commitment_v3(
        PREGAME_KEEP_FIRST_MAIN_CONFIRMATION_DOMAIN_V3,
        &ConfirmationRecordV3 {
            action_plan_commitment_sha256,
            source_capture_commitment_sha256: &source_capture.capture_commitment_sha256,
            source_transition_identity_sha256,
            planned_postcondition,
            after_capture_commitment_sha256: &after_capture.capture_commitment_sha256,
            after_profile_commitment_sha256,
            after_measurement_commitment_sha256,
            after_transition_identity_sha256,
        },
    )
}

fn validate_pregame_capture_progression_v3(
    source_capture: &MtgoDxgiFrameCommitmentsV3,
    source_transition_identity_sha256: &str,
    after_capture: &MtgoDxgiFrameCommitmentsV3,
    after_transition_identity_sha256: &str,
) -> Result<(), String> {
    if source_transition_identity_sha256 != after_transition_identity_sha256
        || source_capture.canonical_width != after_capture.canonical_width
        || source_capture.canonical_height != after_capture.canonical_height
        || source_capture.client_rect_desktop_px != after_capture.client_rect_desktop_px
    {
        return Err("the MTGO process, match window, or capture layout changed".to_owned());
    }
    if after_capture.captured_at_unix_millis <= source_capture.captured_at_unix_millis
        || after_capture.capture_commitment_sha256 == source_capture.capture_commitment_sha256
        || after_capture.canonical_bgra8_sha256 == source_capture.canonical_bgra8_sha256
    {
        return Err("the postcondition must use a strictly newer changed frame".to_owned());
    }
    Ok(())
}

fn pregame_control_profile_commitment_v3() -> Result<String, String> {
    #[derive(Serialize)]
    struct ControlRecordV3<'a> {
        control_id: &'a str,
        rect_client_px: MtgoRectPxV1,
    }
    #[derive(Serialize)]
    struct ControlProfileV3<'a> {
        profile_id: &'a str,
        ladder_profile_commitment_sha256: &'a str,
        client_size_px: MtgoSizePxV1,
        controls: [ControlRecordV3<'a>; 2],
    }
    canonical_json_commitment_v3(
        b"mtgo-pregame-control-profile-v3",
        &ControlProfileV3 {
            profile_id: PREGAME_CONTROL_PROFILE_ID_V3,
            ladder_profile_commitment_sha256: PREGAME_MULLIGAN_LADDER_PROFILE_COMMITMENT_V3,
            client_size_px: MtgoSizePxV1 {
                width: 1_550,
                height: 925,
            },
            controls: [
                ControlRecordV3 {
                    control_id: "mulligan",
                    rect_client_px: MtgoRectPxV1 {
                        x: 29,
                        y: 153,
                        width: 82,
                        height: 33,
                    },
                },
                ControlRecordV3 {
                    control_id: "keep",
                    rect_client_px: MtgoRectPxV1 {
                        x: 116,
                        y: 153,
                        width: 55,
                        height: 33,
                    },
                },
            ],
        },
    )
}

fn hash_bgra_region_for_plan_v3(
    pixels: &[u8],
    size: &MtgoSizePxV1,
    rect: &MtgoRectPxV1,
) -> Result<String, String> {
    let right = rect
        .x
        .checked_add(rect.width)
        .ok_or("pregame action control rectangle overflow")?;
    let bottom = rect
        .y
        .checked_add(rect.height)
        .ok_or("pregame action control rectangle overflow")?;
    if rect.width == 0 || rect.height == 0 || right > size.width || bottom > size.height {
        return Err("pregame action control rectangle is outside the client".to_owned());
    }
    let mut hasher = Sha256::new();
    hasher.update(b"mtgo-pregame-control-bgra-region-v3");
    for value in [rect.x, rect.y, rect.width, rect.height] {
        hasher.update(value.to_be_bytes());
    }
    let row_bytes = usize::try_from(rect.width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or("pregame action control row size overflow")?;
    for y in rect.y..bottom {
        let start = usize::try_from(y)
            .ok()
            .and_then(|y| {
                usize::try_from(size.width)
                    .ok()
                    .and_then(|width| y.checked_mul(width))
            })
            .and_then(|row| {
                usize::try_from(rect.x)
                    .ok()
                    .and_then(|x| row.checked_add(x))
            })
            .and_then(|pixel| pixel.checked_mul(4))
            .ok_or("pregame action control pixel offset overflow")?;
        let end = start
            .checked_add(row_bytes)
            .ok_or("pregame action control row end overflow")?;
        let row = pixels
            .get(start..end)
            .ok_or("pregame action control pixels are incomplete")?;
        hasher.update(row);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn pregame_transition_identity_commitment_v3(
    manifest: &CaptureManifestV2,
) -> Result<String, String> {
    #[derive(Serialize)]
    struct TransitionIdentityV3<'a> {
        schema: &'a str,
        capture_backend: &'a str,
        window_mode: &'a str,
        capture_role: &'a str,
        expected_game_format: &'a str,
        title_rule_version: &'a str,
        hwnd: u64,
        process_id: u32,
        process_start_filetime_100ns: u64,
        process_image: &'a str,
        executable_sha256: &'a str,
        signer_thumbprint: &'a str,
        signer_subject_sha256: &'a str,
        title: &'a str,
        dpi: u32,
        client_rect_desktop_px: SignedRectV1,
        extended_frame_rect_desktop_px: SignedRectV1,
        output: &'a OutputIdentityV1,
    }
    canonical_json_commitment_v3(
        b"mtgo-pregame-transition-identity-v3",
        &TransitionIdentityV3 {
            schema: &manifest.schema,
            capture_backend: &manifest.capture_backend,
            window_mode: &manifest.window_mode,
            capture_role: &manifest.capture_role,
            expected_game_format: &manifest.expected_game_format,
            title_rule_version: &manifest.title_rule_version,
            hwnd: manifest.pre.hwnd,
            process_id: manifest.pre.process_id,
            process_start_filetime_100ns: manifest.pre.process_start_filetime_100ns,
            process_image: &manifest.pre.process_image,
            executable_sha256: &manifest.pre.executable_sha256,
            signer_thumbprint: &manifest.pre.signer_thumbprint,
            signer_subject_sha256: &manifest.pre.signer_subject_sha256,
            title: &manifest.pre.title,
            dpi: manifest.pre.dpi,
            client_rect_desktop_px: manifest.pre.client_rect_desktop_px,
            extended_frame_rect_desktop_px: manifest.pre.extended_frame_rect_desktop_px,
            output: &manifest.output,
        },
    )
}

fn measure_mulligan_ladder_parts_v3(
    manifest_bytes: &[u8],
    canonical_bgra8: &[u8],
    preview_png: &[u8],
) -> Result<CheckedUntrustedMtgoOfflineMulliganLadderCandidateV1, String> {
    let checked =
        check_untrusted_dxgi_capture_artifact_v1(manifest_bytes, canonical_bgra8, preview_png)
            .map_err(|error| format!("check in-process DXGI frame: {error}"))?;
    classify_untrusted_offline_mulligan_ladder_candidate_v2(&checked, canonical_bgra8)
        .map_err(|error| format!("measure London mulligan ladder: {error}"))
}

struct ProcessHandleV1(windows::Win32::Foundation::HANDLE);

struct CryptQueryGuardV1 {
    store: HCERTSTORE,
    message: *const c_void,
}

impl Drop for CryptQueryGuardV1 {
    fn drop(&mut self) {
        unsafe {
            let _ = CryptMsgClose(Some(self.message));
            let _ = CertCloseStore(Some(self.store), 0);
        }
    }
}

struct CertificateGuardV1(*const CERT_CONTEXT);

impl Drop for CertificateGuardV1 {
    fn drop(&mut self) {
        unsafe {
            let _ = CertFreeCertificateContext(Some(self.0));
        }
    }
}

struct AuthenticodeIdentityV1 {
    thumbprint: String,
    subject: String,
    subject_sha256: String,
}

impl Drop for ProcessHandleV1 {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct DpiContextGuardV1(windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT);

impl Drop for DpiContextGuardV1 {
    fn drop(&mut self) {
        unsafe {
            let _ = SetThreadDpiAwarenessContext(self.0);
        }
    }
}

struct DuplicationFrameGuardV1<'a>(&'a IDXGIOutputDuplication);

impl Drop for DuplicationFrameGuardV1<'_> {
    fn drop(&mut self) {
        unsafe {
            let _ = self.0.ReleaseFrame();
        }
    }
}

struct MappedTextureGuardV1<'a> {
    context: &'a ID3D11DeviceContext,
    texture: &'a ID3D11Texture2D,
}

impl Drop for MappedTextureGuardV1<'_> {
    fn drop(&mut self) {
        unsafe {
            self.context.Unmap(self.texture, 0);
        }
    }
}

pub fn run_cli_v3() -> Result<PathBuf, String> {
    let cli = parse_cli()?;
    let output_directory = validate_output_destination(&cli.output_directory)?;
    let frame = capture_mtgo_dxgi_frame_candidate_v3(cli.request)?;
    persist_atomically(
        &output_directory,
        &frame.canonical_bgra8,
        &frame.preview_png,
        &frame.manifest,
    )?;
    Ok(output_directory)
}

pub fn capture_mtgo_dxgi_frame_candidate_v3(
    request: MtgoDxgiCaptureRequestV3,
) -> Result<OpaqueMtgoDxgiFrameCandidateV3, String> {
    validate_capture_request_v3(&request)?;
    let _dpi_guard = enter_per_monitor_v2()?;

    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return Err("foreground window is null".to_owned());
    }
    let pre = snapshot_window(hwnd, &request)?;
    require_admitted_window(&pre)?;

    unsafe { DwmFlush().map_err(|error| format!("DwmFlush before capture: {error}"))? };
    let captured = capture_dxgi_frame(pre.client_rect_desktop_px, request.timeout_ms)?;
    let post = snapshot_window(hwnd, &request)?;
    require_admitted_window(&post)?;
    if pre != post {
        return Err(
            "window, process, focus, geometry, cursor, or z-order changed during capture"
                .to_owned(),
        );
    }
    if captured.metadata.pointer_visible
        && pre
            .client_rect_desktop_px
            .contains_point(captured.metadata.pointer_x, captured.metadata.pointer_y)
    {
        return Err("DXGI pointer position intersects the client crop".to_owned());
    }

    let manifest = CaptureManifestV2 {
        schema: "mtgo-dxgi-visible-frame-candidate/v2".to_owned(),
        artifact_kind: "mtgo_untrusted_dxgi_visible_frame_candidate_v2".to_owned(),
        status: "checked_untrusted_not_admitted".to_owned(),
        capture_backend: "dxgi_desktop_duplication_v1".to_owned(),
        window_mode: request.window_mode.manifest_name().to_owned(),
        capture_role: request.window_mode.capture_role().to_owned(),
        expected_game_format: request.expected_game_format.clone().unwrap_or_default(),
        title_rule_version: "mtgo_visible_title_rule_v2".to_owned(),
        captured_at_unix_millis: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before epoch: {error}"))?
            .as_millis(),
        safety: SafetyFlagsV1 {
            safe_for_semantic_evidence: false,
            safe_for_ocr: false,
            safe_for_policy_scoring: false,
            safe_for_input: false,
            authenticode_verified_in_probe: true,
        },
        pre,
        post,
        output: captured.output,
        frame: captured.metadata,
        files: FilesV1 {
            canonical_pixels: "frame.bgra".to_owned(),
            preview_png: "frame.png".to_owned(),
            manifest: "manifest.json".to_owned(),
        },
    };
    let capture_commitment_sha256 =
        capture_commitment_v3(&manifest, &captured.pixels, &captured.preview_png)?;
    Ok(OpaqueMtgoDxgiFrameCandidateV3 {
        capture_commitment_sha256,
        canonical_bgra8: captured.pixels,
        preview_png: captured.preview_png,
        manifest,
    })
}

/// Loads one already persisted DXGI capture into the same opaque in-process
/// frame type used by the live probe. This exists only for deterministic,
/// no-input rehearsal. It byte-checks the manifest, canonical BGRA frame, and
/// PNG before constructing the opaque value and never focuses or controls MTGO.
pub fn load_checked_untrusted_mtgo_dxgi_frame_candidate_from_artifact_v1(
    artifact_directory: &Path,
) -> Result<OpaqueMtgoDxgiFrameCandidateV3, String> {
    if !artifact_directory.is_absolute() || !artifact_directory.is_dir() {
        return Err(
            "the DXGI rehearsal artifact must be an existing absolute directory".to_owned(),
        );
    }
    let manifest_bytes = fs::read(artifact_directory.join("manifest.json"))
        .map_err(|error| format!("read rehearsal manifest.json: {error}"))?;
    let canonical_bgra8 = fs::read(artifact_directory.join("frame.bgra"))
        .map_err(|error| format!("read rehearsal frame.bgra: {error}"))?;
    let preview_png = fs::read(artifact_directory.join("frame.png"))
        .map_err(|error| format!("read rehearsal frame.png: {error}"))?;
    check_untrusted_dxgi_capture_artifact_v1(&manifest_bytes, &canonical_bgra8, &preview_png)
        .map_err(|error| format!("check rehearsal DXGI artifact: {error}"))?;
    let manifest: CaptureManifestV2 = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("parse rehearsal DXGI manifest: {error}"))?;
    let capture_commitment_sha256 =
        capture_commitment_from_parts_v3(&manifest_bytes, &canonical_bgra8, &preview_png)?;
    Ok(OpaqueMtgoDxgiFrameCandidateV3 {
        capture_commitment_sha256,
        canonical_bgra8,
        preview_png,
        manifest,
    })
}

fn parse_cli() -> ProbeResult<CliV1> {
    let mut args = std::env::args().skip(1);
    let mut output_directory = None;
    let mut expected_executable_sha256 = None;
    let mut expected_signer_thumbprint = None;
    let mut expected_signer_subject_sha256 = None;
    let mut window_mode = CaptureWindowModeV2::MainClient;
    let mut expected_game_format = None;
    let mut expected_title_contains = None;
    let mut timeout_ms = 1_500u32;
    while let Some(argument) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {argument}"))?;
        match argument.as_str() {
            "--output" => output_directory = Some(PathBuf::from(value)),
            "--expected-exe-sha256" => expected_executable_sha256 = Some(value),
            "--expected-signer-thumbprint" => expected_signer_thumbprint = Some(value),
            "--expected-signer-subject-sha256" => expected_signer_subject_sha256 = Some(value),
            "--window-mode" => window_mode = match value.as_str() {
                "main_client" => CaptureWindowModeV2::MainClient,
                "solitaire_game" => CaptureWindowModeV2::SolitaireGame,
                "duel_game" => CaptureWindowModeV2::DuelGame,
                "spectator_game" => CaptureWindowModeV2::SpectatorGame,
                _ => return Err(
                    "window mode must be main_client, solitaire_game, duel_game, or spectator_game"
                        .to_owned(),
                ),
            },
            "--expected-game-format" => expected_game_format = Some(value),
            "--expected-title-contains" => expected_title_contains = Some(value),
            "--timeout-ms" => {
                timeout_ms = value
                    .parse::<u32>()
                    .map_err(|_| "timeout must be an integer".to_owned())?;
                if !(100..=10_000).contains(&timeout_ms) {
                    return Err("timeout must be between 100 and 10000 milliseconds".to_owned());
                }
            }
            _ => return Err(format!("unknown argument {argument}")),
        }
    }
    Ok(CliV1 {
        output_directory: output_directory.ok_or("--output is required")?,
        request: MtgoDxgiCaptureRequestV3 {
            expected_executable_sha256: expected_executable_sha256
                .ok_or("--expected-exe-sha256 is required")?,
            expected_signer_thumbprint: expected_signer_thumbprint
                .ok_or("--expected-signer-thumbprint is required")?,
            expected_signer_subject_sha256: expected_signer_subject_sha256
                .ok_or("--expected-signer-subject-sha256 is required")?,
            window_mode,
            expected_game_format,
            expected_title_contains,
            timeout_ms,
        },
    })
}

fn validate_capture_request_v3(request: &MtgoDxgiCaptureRequestV3) -> ProbeResult<()> {
    validate_sha256(&request.expected_executable_sha256)?;
    validate_sha1(&request.expected_signer_thumbprint)?;
    validate_sha256(&request.expected_signer_subject_sha256)?;
    if request.expected_game_format.as_ref().is_some_and(|value| {
        value.is_empty()
            || value.len() > 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b' ' | b'-'))
    }) {
        return Err("expected game format is not a safe visible label".to_owned());
    }
    if request
        .expected_title_contains
        .as_ref()
        .is_some_and(|value| {
            value.is_empty() || value.len() > 512 || value.chars().any(char::is_control)
        })
    {
        return Err("expected title substring is not a safe visible label".to_owned());
    }
    if !(100..=10_000).contains(&request.timeout_ms) {
        return Err("timeout must be between 100 and 10000 milliseconds".to_owned());
    }
    match request.window_mode {
        CaptureWindowModeV2::MainClient if request.expected_game_format.is_some() => {
            Err("main-client mode cannot declare an expected game format".to_owned())
        }
        CaptureWindowModeV2::SolitaireGame
        | CaptureWindowModeV2::DuelGame
        | CaptureWindowModeV2::SpectatorGame
            if request.expected_game_format.is_none() =>
        {
            Err("game window mode requires an expected game format".to_owned())
        }
        _ => Ok(()),
    }
}

fn enter_per_monitor_v2() -> ProbeResult<DpiContextGuardV1> {
    unsafe {
        let previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        if previous.0.is_null() {
            return Err("SetThreadDpiAwarenessContext failed".to_owned());
        }
        if !AreDpiAwarenessContextsEqual(
            GetThreadDpiAwarenessContext(),
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        )
        .as_bool()
        {
            let _ = SetThreadDpiAwarenessContext(previous);
            return Err("capture thread is not Per-Monitor V2 aware".to_owned());
        }
        Ok(DpiContextGuardV1(previous))
    }
}

fn snapshot_window(
    hwnd: HWND,
    request: &MtgoDxgiCaptureRequestV3,
) -> ProbeResult<WindowSnapshotV1> {
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() || GetAncestor(hwnd, GA_ROOT) != hwnd {
            return Err("foreground target is not a root window".to_owned());
        }
        let mut process_id = 0u32;
        if GetWindowThreadProcessId(hwnd, Some(&mut process_id)) == 0 || process_id == 0 {
            return Err("could not resolve foreground process".to_owned());
        }
        let handle = ProcessHandleV1(
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id)
                .map_err(|error| format!("OpenProcess: {error}"))?,
        );
        let mtgo_process_count = count_mtgo_processes(process_id)?;
        let process_image = query_process_image(handle.0)?;
        if !Path::new(&process_image)
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("MTGO.exe"))
        {
            return Err("foreground process image is not MTGO.exe".to_owned());
        }
        let process_image_path = Path::new(&process_image);
        let executable_sha256 = sha256_file(process_image_path)?;
        if executable_sha256 != request.expected_executable_sha256 {
            return Err(
                "foreground MTGO executable hash does not match the expected hash".to_owned(),
            );
        }
        let signer = verify_authenticode(process_image_path)?;
        if signer.thumbprint != request.expected_signer_thumbprint
            || signer.subject_sha256 != request.expected_signer_subject_sha256
        {
            return Err("MTGO signer identity does not match the pinned certificate".to_owned());
        }
        let title = window_title(hwnd);
        validate_visible_mtgo_title_v2(
            request.window_mode,
            request.expected_game_format.as_deref(),
            &title,
        )
        .map_err(str::to_owned)?;
        if request
            .expected_title_contains
            .as_ref()
            .is_some_and(|expected| expected.is_empty() || !title.contains(expected))
        {
            return Err(
                "foreground title does not match the configured MTGO title rule".to_owned(),
            );
        }
        let client_rect = client_rect_desktop(hwnd)?;
        let extended_rect = dwm_rect(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS)?;
        if !extended_rect.contains(client_rect) {
            return Err("DWM frame bounds do not contain the client crop".to_owned());
        }
        let cloaked = dwm_u32(hwnd, DWMWA_CLOAKED)? != 0;
        let mut display_affinity = u32::MAX;
        GetWindowDisplayAffinity(hwnd, &mut display_affinity)
            .map_err(|error| format!("GetWindowDisplayAffinity: {error}"))?;
        let composition = DwmIsCompositionEnabled()
            .map_err(|error| format!("DwmIsCompositionEnabled: {error}"))?
            .as_bool();
        let cursor = cursor_state(client_rect)?;
        let occlusion = audit_occlusion(hwnd, client_rect)?;
        Ok(WindowSnapshotV1 {
            hwnd: hwnd.0 as usize as u64,
            process_id,
            mtgo_process_count,
            process_start_filetime_100ns: process_start_filetime(handle.0)?,
            process_image,
            executable_sha256,
            authenticode_valid: true,
            signer_thumbprint: signer.thumbprint,
            signer_subject: signer.subject,
            signer_subject_sha256: signer.subject_sha256,
            title,
            dpi: GetDpiForWindow(hwnd),
            client_rect_desktop_px: client_rect,
            extended_frame_rect_desktop_px: extended_rect,
            foreground: GetForegroundWindow() == hwnd,
            visible: IsWindowVisible(hwnd).as_bool(),
            minimized: IsIconic(hwnd).as_bool(),
            cloaked,
            hung: IsHungAppWindow(hwnd).as_bool(),
            display_affinity,
            desktop_composition_enabled: composition,
            cursor_showing: cursor.0,
            cursor_x: cursor.1,
            cursor_y: cursor.2,
            cursor_inside_client: cursor.3,
            occlusion_target_found: occlusion.0,
            occluding_windows_above: occlusion.1,
            z_order_sha256: occlusion.2,
        })
    }
}

fn require_admitted_window(snapshot: &WindowSnapshotV1) -> ProbeResult<()> {
    if !snapshot.foreground
        || !snapshot.visible
        || snapshot.minimized
        || snapshot.cloaked
        || snapshot.hung
        || !snapshot.authenticode_valid
        || snapshot.mtgo_process_count != 1
        || snapshot.display_affinity != WDA_NONE.0
        || !snapshot.desktop_composition_enabled
        || snapshot.dpi == 0
        || snapshot.cursor_inside_client
        || !snapshot.occlusion_target_found
        || snapshot.occluding_windows_above != 0
    {
        return Err("window admission checks did not all pass".to_owned());
    }
    Ok(())
}

fn capture_dxgi_frame(client_rect: SignedRectV1, timeout_ms: u32) -> ProbeResult<CapturedFrameV1> {
    unsafe {
        let factory: IDXGIFactory1 =
            CreateDXGIFactory1().map_err(|error| format!("CreateDXGIFactory1: {error}"))?;
        let mut adapter_index = 0u32;
        loop {
            let adapter = match factory.EnumAdapters1(adapter_index) {
                Ok(value) => value,
                Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(error) => return Err(format!("EnumAdapters1: {error}")),
            };
            let mut output_index = 0u32;
            loop {
                let output = match adapter.EnumOutputs(output_index) {
                    Ok(value) => value,
                    Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                    Err(error) => return Err(format!("EnumOutputs: {error}")),
                };
                let desc = output
                    .GetDesc()
                    .map_err(|error| format!("IDXGIOutput::GetDesc: {error}"))?;
                let output_rect = rect(desc.DesktopCoordinates);
                if output_rect.contains(client_rect) {
                    return capture_from_output(
                        adapter,
                        output
                            .cast()
                            .map_err(|error| format!("IDXGIOutput1: {error}"))?,
                        output
                            .cast()
                            .map_err(|error| format!("IDXGIOutput6: {error}"))?,
                        adapter_index,
                        output_index,
                        client_rect,
                        output_rect,
                        timeout_ms,
                    );
                }
                output_index += 1;
            }
            adapter_index += 1;
        }
    }
    Err("client rectangle is not wholly contained in one DXGI output".to_owned())
}

#[allow(clippy::too_many_arguments)]
unsafe fn capture_from_output(
    adapter: IDXGIAdapter1,
    output1: IDXGIOutput1,
    output6: IDXGIOutput6,
    adapter_index: u32,
    output_index: u32,
    client_rect: SignedRectV1,
    output_rect: SignedRectV1,
    timeout_ms: u32,
) -> ProbeResult<CapturedFrameV1> {
    let output_desc = output1
        .GetDesc()
        .map_err(|error| format!("output GetDesc: {error}"))?;
    let output_desc1 = output6
        .GetDesc1()
        .map_err(|error| format!("output GetDesc1: {error}"))?;
    if !output_desc.AttachedToDesktop.as_bool()
        || output_desc.Rotation != DXGI_MODE_ROTATION_IDENTITY
        || output_desc1.ColorSpace != DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709
    {
        return Err("output is detached, rotated, HDR, or not canonical SDR sRGB".to_owned());
    }

    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;
    D3D11CreateDevice(
        &adapter,
        D3D_DRIVER_TYPE_UNKNOWN,
        HMODULE::default(),
        D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        Some(&[D3D_FEATURE_LEVEL_11_0]),
        D3D11_SDK_VERSION,
        Some(&mut device),
        None,
        Some(&mut context),
    )
    .map_err(|error| format!("D3D11CreateDevice: {error}"))?;
    let device = device.ok_or("D3D11CreateDevice returned no device")?;
    let context = context.ok_or("D3D11CreateDevice returned no context")?;
    let duplication = output1
        .DuplicateOutput(&device)
        .map_err(|error| format!("DuplicateOutput: {error}"))?;
    let duplication_desc = duplication.GetDesc();
    if duplication_desc.ModeDesc.Format != DXGI_FORMAT_B8G8R8A8_UNORM
        || duplication_desc.Rotation != DXGI_MODE_ROTATION_IDENTITY
    {
        return Err("desktop duplication format or rotation is not canonical BGRA8".to_owned());
    }

    let deadline = Instant::now()
        .checked_add(Duration::from_millis(timeout_ms as u64))
        .ok_or("capture deadline overflow")?;
    let (info, desktop_resource) = loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("timed out waiting for a real desktop presentation".to_owned());
        }
        let wait_ms = u32::try_from(remaining.as_millis().max(1))
            .unwrap_or(u32::MAX)
            .min(timeout_ms);
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut desktop_resource: Option<IDXGIResource> = None;
        duplication
            .AcquireNextFrame(wait_ms, &mut info, &mut desktop_resource)
            .map_err(|error| format!("AcquireNextFrame: {error}"))?;
        if info.ProtectedContentMaskedOut.as_bool() {
            let _ = duplication.ReleaseFrame();
            return Err("DXGI frame masks protected content".to_owned());
        }
        if info.LastPresentTime != 0 && info.AccumulatedFrames != 0 {
            break (info, desktop_resource);
        }
        duplication
            .ReleaseFrame()
            .map_err(|error| format!("ReleaseFrame after pointer-only update: {error}"))?;
    };
    let _frame_guard = DuplicationFrameGuardV1(&duplication);
    let desktop_texture: ID3D11Texture2D = desktop_resource
        .ok_or("AcquireNextFrame returned no desktop resource")?
        .cast()
        .map_err(|error| format!("desktop resource is not Texture2D: {error}"))?;
    let mut source_desc = D3D11_TEXTURE2D_DESC::default();
    desktop_texture.GetDesc(&mut source_desc);
    if source_desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM
        || source_desc.Width != output_rect.width().map_err(str::to_owned)?
        || source_desc.Height != output_rect.height().map_err(str::to_owned)?
    {
        return Err("DXGI source texture does not match the selected output".to_owned());
    }
    let crop = client_rect
        .crop_box_within(output_rect)
        .map_err(str::to_owned)?;
    let staging_desc = D3D11_TEXTURE2D_DESC {
        Width: crop.width,
        Height: crop.height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };
    let mut staging: Option<ID3D11Texture2D> = None;
    device
        .CreateTexture2D(&staging_desc, None, Some(&mut staging))
        .map_err(|error| format!("CreateTexture2D staging: {error}"))?;
    let staging = staging.ok_or("CreateTexture2D returned no staging texture")?;
    let source_box = D3D11_BOX {
        left: crop.left,
        top: crop.top,
        front: 0,
        right: crop.left + crop.width,
        bottom: crop.top + crop.height,
        back: 1,
    };
    context.CopySubresourceRegion(&staging, 0, 0, 0, 0, &desktop_texture, 0, Some(&source_box));
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    context
        .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
        .map_err(|error| format!("Map staging texture: {error}"))?;
    let _mapped_guard = MappedTextureGuardV1 {
        context: &context,
        texture: &staging,
    };
    let mapped_len = usize::try_from(mapped.RowPitch)
        .ok()
        .and_then(|row| row.checked_mul(crop.height as usize))
        .ok_or("mapped texture byte length overflow")?;
    let mapped_bytes = std::slice::from_raw_parts(mapped.pData.cast::<u8>(), mapped_len);
    let pixels = copy_tightly_packed_bgra8_v1(
        mapped_bytes,
        mapped.RowPitch as usize,
        crop.width,
        crop.height,
    )
    .map_err(str::to_owned)?;
    let preview_png = encode_preview_png(&pixels, crop.width, crop.height)?;

    let adapter_desc = adapter
        .GetDesc1()
        .map_err(|error| format!("adapter GetDesc1: {error}"))?;
    let output = OutputIdentityV1 {
        adapter_index,
        output_index,
        adapter_luid_low: adapter_desc.AdapterLuid.LowPart,
        adapter_luid_high: adapter_desc.AdapterLuid.HighPart,
        device_name: utf16_nul(&output_desc.DeviceName),
        bounds_desktop_px: output_rect,
        rotation: output_desc.Rotation.0,
        color_space: output_desc1.ColorSpace.0,
    };
    let metadata = FrameMetadataV1 {
        last_present_time_qpc: info.LastPresentTime,
        last_mouse_update_time_qpc: info.LastMouseUpdateTime,
        accumulated_frames: info.AccumulatedFrames,
        protected_content_masked_out: info.ProtectedContentMaskedOut.as_bool(),
        pointer_visible: info.PointerPosition.Visible.as_bool(),
        pointer_x: info.PointerPosition.Position.x,
        pointer_y: info.PointerPosition.Position.y,
        source_texture_width: source_desc.Width,
        source_texture_height: source_desc.Height,
        source_texture_format: source_desc.Format.0,
        canonical_width: crop.width,
        canonical_height: crop.height,
        canonical_stride: crop.width.checked_mul(4).ok_or("stride overflow")?,
        canonical_byte_length: pixels.len(),
        canonical_bgra8_sha256: sha256_hex_v1(&pixels),
        preview_png_sha256: sha256_hex_v1(&preview_png),
    };
    Ok(CapturedFrameV1 {
        pixels,
        preview_png,
        metadata,
        output,
    })
}

fn client_rect_desktop(hwnd: HWND) -> ProbeResult<SignedRectV1> {
    unsafe {
        let mut client = RECT::default();
        GetClientRect(hwnd, &mut client).map_err(|error| format!("GetClientRect: {error}"))?;
        let mut upper_left = POINT {
            x: client.left,
            y: client.top,
        };
        let mut lower_right = POINT {
            x: client.right,
            y: client.bottom,
        };
        if !ClientToScreen(hwnd, &mut upper_left).as_bool()
            || !ClientToScreen(hwnd, &mut lower_right).as_bool()
        {
            return Err("ClientToScreen failed".to_owned());
        }
        let value = SignedRectV1 {
            left: upper_left.x,
            top: upper_left.y,
            right: lower_right.x,
            bottom: lower_right.y,
        };
        value.width().map_err(str::to_owned)?;
        value.height().map_err(str::to_owned)?;
        Ok(value)
    }
}

fn dwm_rect(
    hwnd: HWND,
    attribute: windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE,
) -> ProbeResult<SignedRectV1> {
    unsafe {
        let mut value = RECT::default();
        DwmGetWindowAttribute(
            hwnd,
            attribute,
            (&mut value as *mut RECT).cast::<c_void>(),
            std::mem::size_of::<RECT>() as u32,
        )
        .map_err(|error| format!("DwmGetWindowAttribute rect: {error}"))?;
        let value = rect(value);
        value.width().map_err(str::to_owned)?;
        value.height().map_err(str::to_owned)?;
        Ok(value)
    }
}

fn dwm_u32(
    hwnd: HWND,
    attribute: windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE,
) -> ProbeResult<u32> {
    unsafe {
        let mut value = 0u32;
        DwmGetWindowAttribute(
            hwnd,
            attribute,
            (&mut value as *mut u32).cast::<c_void>(),
            std::mem::size_of::<u32>() as u32,
        )
        .map_err(|error| format!("DwmGetWindowAttribute u32: {error}"))?;
        Ok(value)
    }
}

fn cursor_state(client: SignedRectV1) -> ProbeResult<(bool, i32, i32, bool)> {
    unsafe {
        let mut cursor = CURSORINFO {
            cbSize: std::mem::size_of::<CURSORINFO>() as u32,
            ..Default::default()
        };
        GetCursorInfo(&mut cursor).map_err(|error| format!("GetCursorInfo: {error}"))?;
        let showing = cursor.flags.0 & CURSOR_SHOWING.0 != 0;
        Ok((
            showing,
            cursor.ptScreenPos.x,
            cursor.ptScreenPos.y,
            showing && client.contains_point(cursor.ptScreenPos.x, cursor.ptScreenPos.y),
        ))
    }
}

fn audit_occlusion(hwnd: HWND, client: SignedRectV1) -> ProbeResult<(bool, u32, String)> {
    unsafe {
        let mut current = GetTopWindow(None).map_err(|error| format!("GetTopWindow: {error}"))?;
        let mut lines = Vec::new();
        let mut intersections = 0u32;
        for _ in 0..4_096 {
            if current == hwnd {
                lines.push(format!("target:{}", current.0 as usize));
                return Ok((
                    true,
                    intersections,
                    sha256_hex_v1(lines.join("\n").as_bytes()),
                ));
            }
            if IsWindowVisible(current).as_bool() && dwm_u32(current, DWMWA_CLOAKED)? == 0 {
                let bounds = dwm_rect(current, DWMWA_EXTENDED_FRAME_BOUNDS)?;
                let intersects = bounds.intersects(client);
                if intersects {
                    intersections = intersections
                        .checked_add(1)
                        .ok_or("occlusion count overflow")?;
                }
                lines.push(format!(
                    "{}:{}:{}:{}:{}:{}",
                    current.0 as usize,
                    bounds.left,
                    bounds.top,
                    bounds.right,
                    bounds.bottom,
                    intersects
                ));
            }
            current = GetWindow(current, GW_HWNDNEXT)
                .map_err(|error| format!("GetWindow(GW_HWNDNEXT) before target: {error}"))?;
        }
    }
    Err("target was not found within the top-level z-order audit bound".to_owned())
}

fn query_process_image(handle: windows::Win32::Foundation::HANDLE) -> ProbeResult<String> {
    unsafe {
        let mut buffer = vec![0u16; 32_768];
        let mut length = buffer.len() as u32;
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
        .map_err(|error| format!("QueryFullProcessImageNameW: {error}"))?;
        buffer.truncate(length as usize);
        String::from_utf16(&buffer).map_err(|error| format!("process path is not UTF-16: {error}"))
    }
}

fn count_mtgo_processes(target_process_id: u32) -> ProbeResult<u32> {
    let snapshot = ProcessHandleV1(
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
            .map_err(|error| format!("CreateToolhelp32Snapshot: {error}"))?,
    );
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    unsafe { Process32FirstW(snapshot.0, &mut entry) }
        .map_err(|error| format!("Process32FirstW: {error}"))?;
    let mut count = 0u32;
    let mut target_found = false;
    loop {
        if utf16_nul(&entry.szExeFile).eq_ignore_ascii_case("MTGO.exe") {
            count = count.checked_add(1).ok_or("MTGO process count overflow")?;
            target_found |= entry.th32ProcessID == target_process_id;
        }
        match unsafe { Process32NextW(snapshot.0, &mut entry) } {
            Ok(()) => {}
            Err(error) if error.code() == ERROR_NO_MORE_FILES.to_hresult() => break,
            Err(error) => return Err(format!("Process32NextW: {error}")),
        }
    }
    if !target_found {
        return Err("foreground MTGO process was absent from the process snapshot".to_owned());
    }
    Ok(count)
}

fn process_start_filetime(handle: windows::Win32::Foundation::HANDLE) -> ProbeResult<u64> {
    unsafe {
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user)
            .map_err(|error| format!("GetProcessTimes: {error}"))?;
        Ok(((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64)
    }
}

fn window_title(hwnd: HWND) -> String {
    unsafe {
        let mut buffer = vec![0u16; 2_048];
        let length = GetWindowTextW(hwnd, &mut buffer).max(0) as usize;
        buffer.truncate(length);
        String::from_utf16_lossy(&buffer)
    }
}

fn sha256_file(path: &Path) -> ProbeResult<String> {
    let mut file = File::open(path).map_err(|error| format!("open executable: {error}"))?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 1_048_576];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("read executable: {error}"))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn verify_authenticode(path: &Path) -> ProbeResult<AuthenticodeIdentityV1> {
    let mut wide_path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide_path.as_mut_ptr()),
        hFile: HANDLE::default(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut trust_data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 {
            pFile: &mut file_info,
        },
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL,
        dwUIContext: WTD_UICONTEXT_EXECUTE,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let status = unsafe {
        WinVerifyTrust(
            HWND::default(),
            &mut action,
            (&mut trust_data as *mut WINTRUST_DATA).cast::<c_void>(),
        )
    };
    trust_data.dwStateAction = WTD_STATEACTION_CLOSE;
    let close_status = unsafe {
        WinVerifyTrust(
            HWND::default(),
            &mut action,
            (&mut trust_data as *mut WINTRUST_DATA).cast::<c_void>(),
        )
    };
    if status != 0 {
        return Err(format!(
            "WinVerifyTrust rejected the MTGO executable with status {status:#010x}"
        ));
    }
    if close_status != 0 {
        return Err(format!(
            "WinVerifyTrust state close failed with status {close_status:#010x}"
        ));
    }
    signer_identity_from_file(&wide_path)
}

fn signer_identity_from_file(wide_path: &[u16]) -> ProbeResult<AuthenticodeIdentityV1> {
    let mut store = HCERTSTORE::default();
    let mut message: *mut c_void = std::ptr::null_mut();
    unsafe {
        CryptQueryObject(
            CERT_QUERY_OBJECT_FILE,
            wide_path.as_ptr().cast::<c_void>(),
            CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED,
            CERT_QUERY_FORMAT_FLAG_BINARY,
            0,
            None,
            None,
            None,
            Some(&mut store),
            Some(&mut message),
            None,
        )
        .map_err(|error| format!("CryptQueryObject signer extraction: {error}"))?;
    }
    if store.0.is_null() || message.is_null() {
        return Err("CryptQueryObject returned no signer store or message".to_owned());
    }
    let _query_guard = CryptQueryGuardV1 {
        store,
        message: message.cast_const(),
    };

    let mut signer_bytes = 0u32;
    unsafe {
        CryptMsgGetParam(
            message.cast_const(),
            CMSG_SIGNER_INFO_PARAM,
            0,
            None,
            &mut signer_bytes,
        )
        .map_err(|error| format!("CryptMsgGetParam signer size: {error}"))?;
    }
    let word_size = std::mem::size_of::<usize>();
    let words = (signer_bytes as usize)
        .checked_add(word_size - 1)
        .ok_or("signer allocation overflow")?
        / word_size;
    let mut signer_storage = vec![0usize; words];
    unsafe {
        CryptMsgGetParam(
            message.cast_const(),
            CMSG_SIGNER_INFO_PARAM,
            0,
            Some(signer_storage.as_mut_ptr().cast::<c_void>()),
            &mut signer_bytes,
        )
        .map_err(|error| format!("CryptMsgGetParam signer data: {error}"))?;
    }
    let signer = unsafe { &*signer_storage.as_ptr().cast::<CMSG_SIGNER_INFO>() };
    let cert_info = CERT_INFO {
        Issuer: signer.Issuer,
        SerialNumber: signer.SerialNumber,
        ..Default::default()
    };
    let encoding = X509_ASN_ENCODING | PKCS_7_ASN_ENCODING;
    let certificate = unsafe {
        CertFindCertificateInStore(
            store,
            encoding,
            0,
            CERT_FIND_SUBJECT_CERT,
            Some((&cert_info as *const CERT_INFO).cast::<c_void>()),
            None,
        )
    };
    if certificate.is_null() {
        return Err("embedded Authenticode signer certificate was not found".to_owned());
    }
    let _certificate_guard = CertificateGuardV1(certificate);

    let mut thumbprint_bytes = 0u32;
    unsafe {
        CertGetCertificateContextProperty(
            certificate,
            CERT_HASH_PROP_ID,
            None,
            &mut thumbprint_bytes,
        )
        .map_err(|error| format!("certificate thumbprint size: {error}"))?;
    }
    let mut thumbprint = vec![0u8; thumbprint_bytes as usize];
    unsafe {
        CertGetCertificateContextProperty(
            certificate,
            CERT_HASH_PROP_ID,
            Some(thumbprint.as_mut_ptr().cast::<c_void>()),
            &mut thumbprint_bytes,
        )
        .map_err(|error| format!("certificate thumbprint: {error}"))?;
    }
    thumbprint.truncate(thumbprint_bytes as usize);
    let thumbprint = thumbprint
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    let cert_info = unsafe { &*(*certificate).pCertInfo };
    let string_type = CERT_STRING_TYPE(CERT_X500_NAME_STR.0 | CERT_NAME_STR_REVERSE_FLAG);
    let subject_len = unsafe { CertNameToStrW(encoding, &cert_info.Subject, string_type, None) };
    if subject_len <= 1 {
        return Err("certificate subject is empty".to_owned());
    }
    let mut subject_buffer = vec![0u16; subject_len as usize];
    let written = unsafe {
        CertNameToStrW(
            encoding,
            &cert_info.Subject,
            string_type,
            Some(&mut subject_buffer),
        )
    };
    if written != subject_len {
        return Err("certificate subject length changed during extraction".to_owned());
    }
    subject_buffer.truncate((written - 1) as usize);
    let subject = String::from_utf16(&subject_buffer)
        .map_err(|error| format!("certificate subject is not UTF-16: {error}"))?;
    let subject_sha256 = sha256_hex_v1(subject.as_bytes());
    Ok(AuthenticodeIdentityV1 {
        thumbprint,
        subject,
        subject_sha256,
    })
}

fn validate_output_destination(requested: &Path) -> ProbeResult<PathBuf> {
    if !requested.is_absolute() || requested.exists() {
        return Err("output must be a new absolute directory".to_owned());
    }
    let parent = requested
        .parent()
        .ok_or("output directory has no parent")?
        .canonicalize()
        .map_err(|error| format!("canonicalize output parent: {error}"))?;
    let name = requested
        .file_name()
        .ok_or("output directory has no name")?;
    let resolved = parent.join(name);
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("could not derive repository root")?
        .canonicalize()
        .map_err(|error| format!("canonicalize repository root: {error}"))?;
    if resolved.starts_with(repository) {
        return Err("capture output may not be written anywhere inside the repository".to_owned());
    }
    Ok(resolved)
}

fn persist_atomically(
    output: &Path,
    pixels: &[u8],
    preview_png: &[u8],
    manifest: &CaptureManifestV2,
) -> ProbeResult<()> {
    let parent = output.parent().ok_or("output has no parent")?;
    let partial = parent.join(format!(
        ".mtgo-dxgi-partial-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before epoch: {error}"))?
            .as_nanos()
    ));
    if partial.exists() {
        return Err("partial output path already exists".to_owned());
    }
    fs::create_dir(&partial).map_err(|error| format!("create partial directory: {error}"))?;
    let result = (|| {
        fs::write(partial.join("frame.bgra"), pixels)
            .map_err(|error| format!("write canonical pixels: {error}"))?;
        fs::write(partial.join("frame.png"), preview_png)
            .map_err(|error| format!("write preview PNG: {error}"))?;
        let manifest_bytes = serialize_manifest_v2(manifest)?;
        fs::write(partial.join("manifest.json"), manifest_bytes)
            .map_err(|error| format!("write manifest: {error}"))?;
        fs::rename(&partial, output)
            .map_err(|error| format!("commit output directory: {error}"))?;
        Ok(())
    })();
    if result.is_err() && partial.exists() {
        let _ = fs::remove_dir_all(&partial);
    }
    result
}

fn capture_commitment_v3(
    manifest: &CaptureManifestV2,
    canonical_bgra8: &[u8],
    preview_png: &[u8],
) -> ProbeResult<String> {
    let manifest_bytes = serialize_manifest_v2(manifest)?;
    capture_commitment_from_parts_v3(&manifest_bytes, canonical_bgra8, preview_png)
}

fn serialize_manifest_v2(manifest: &CaptureManifestV2) -> ProbeResult<Vec<u8>> {
    serde_json::to_vec_pretty(manifest).map_err(|error| format!("serialize manifest: {error}"))
}

fn capture_commitment_from_parts_v3(
    manifest_bytes: &[u8],
    canonical_bgra8: &[u8],
    preview_png: &[u8],
) -> ProbeResult<String> {
    let mut hash = Sha256::new();
    hash.update(b"mtgo-dxgi-frame-candidate-commitment-v3\0");
    for part in [manifest_bytes, canonical_bgra8, preview_png] {
        let length = u64::try_from(part.len()).map_err(|_| "commitment part is too large")?;
        hash.update(length.to_be_bytes());
        hash.update(part);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn canonical_json_commitment_v3<T: Serialize + ?Sized>(
    domain: &[u8],
    value: &T,
) -> ProbeResult<String> {
    let encoded = serde_json::to_vec(value)
        .map_err(|error| format!("serialize canonical commitment: {error}"))?;
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update((encoded.len() as u64).to_be_bytes());
    hash.update(encoded);
    Ok(format!("{:x}", hash.finalize()))
}

fn require_lower_sha256_v3(value: &str, label: &str) -> ProbeResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("{label} must be lowercase SHA-256"));
    }
    Ok(())
}

fn encode_preview_png(pixels: &[u8], width: u32, height: u32) -> ProbeResult<Vec<u8>> {
    let expected = usize::try_from(width)
        .ok()
        .and_then(|value| value.checked_mul(height as usize))
        .and_then(|value| value.checked_mul(4))
        .ok_or("PNG geometry overflow")?;
    if pixels.len() != expected {
        return Err("canonical BGRA length does not match PNG geometry".to_owned());
    }
    let mut rgba = Vec::with_capacity(pixels.len());
    for pixel in pixels.chunks_exact(4) {
        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| format!("write PNG header: {error}"))?;
        writer
            .write_image_data(&rgba)
            .map_err(|error| format!("write PNG pixels: {error}"))?;
    }
    Ok(encoded)
}

fn validate_sha256(value: &str) -> ProbeResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("expected executable hash must be lowercase SHA-256".to_owned());
    }
    Ok(())
}

fn validate_sha1(value: &str) -> ProbeResult<()> {
    if value.len() != 40
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("expected signer thumbprint must be lowercase SHA-1".to_owned());
    }
    Ok(())
}

fn rect(value: RECT) -> SignedRectV1 {
    SignedRectV1 {
        left: value.left,
        top: value.top,
        right: value.right,
        bottom: value.bottom,
    }
}

fn utf16_nul(value: &[u16]) -> String {
    let length = value
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..length])
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtgo_blackbox_v1::{MtgoNativeCheckpointIdentityV1, MTGO_EXTERNAL_MODEL_SCORING_SCHEMA_V1};
    use serde_json::json;

    fn request(mode: CaptureWindowModeV2) -> MtgoDxgiCaptureRequestV3 {
        MtgoDxgiCaptureRequestV3 {
            expected_executable_sha256: "a".repeat(64),
            expected_signer_thumbprint: "b".repeat(40),
            expected_signer_subject_sha256: "c".repeat(64),
            window_mode: mode,
            expected_game_format: None,
            expected_title_contains: None,
            timeout_ms: 1_500,
        }
    }

    #[test]
    fn request_validation_binds_mode_identity_and_timeout() {
        assert!(validate_capture_request_v3(&request(CaptureWindowModeV2::MainClient)).is_ok());

        let mut malformed = request(CaptureWindowModeV2::MainClient);
        malformed.expected_executable_sha256 = "A".repeat(64);
        assert!(validate_capture_request_v3(&malformed).is_err());

        let mut wrong_role = request(CaptureWindowModeV2::MainClient);
        wrong_role.expected_game_format = Some("Freeform".to_owned());
        assert!(validate_capture_request_v3(&wrong_role).is_err());

        let mut missing_format = request(CaptureWindowModeV2::SolitaireGame);
        missing_format.expected_game_format = None;
        assert!(validate_capture_request_v3(&missing_format).is_err());

        let mut valid_game = request(CaptureWindowModeV2::SolitaireGame);
        valid_game.expected_game_format = Some("Freeform".to_owned());
        assert!(validate_capture_request_v3(&valid_game).is_ok());

        let mut valid_duel = request(CaptureWindowModeV2::DuelGame);
        valid_duel.expected_game_format = Some("Freeform".to_owned());
        assert!(validate_capture_request_v3(&valid_duel).is_ok());

        let missing_duel_format = request(CaptureWindowModeV2::DuelGame);
        assert!(validate_capture_request_v3(&missing_duel_format).is_err());

        let mut bad_timeout = request(CaptureWindowModeV2::MainClient);
        bad_timeout.timeout_ms = 99;
        assert!(validate_capture_request_v3(&bad_timeout).is_err());

        let mut bad_title = request(CaptureWindowModeV2::MainClient);
        bad_title.expected_title_contains = Some("\n".to_owned());
        assert!(validate_capture_request_v3(&bad_title).is_err());

        let mut bad_format = request(CaptureWindowModeV2::SolitaireGame);
        bad_format.expected_game_format = Some("Freeform:".to_owned());
        assert!(validate_capture_request_v3(&bad_format).is_err());
    }

    #[test]
    fn commitment_is_domain_separated_and_binds_order_and_every_byte() {
        let baseline = capture_commitment_from_parts_v3(b"manifest", b"pixels", b"png").unwrap();
        assert_eq!(baseline.len(), 64);
        assert!(baseline
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
        assert_eq!(
            baseline,
            capture_commitment_from_parts_v3(b"manifest", b"pixels", b"png").unwrap()
        );
        assert_ne!(
            baseline,
            capture_commitment_from_parts_v3(b"manifest!", b"pixels", b"png").unwrap()
        );
        assert_ne!(
            baseline,
            capture_commitment_from_parts_v3(b"manifest", b"pixels!", b"png").unwrap()
        );
        assert_ne!(
            baseline,
            capture_commitment_from_parts_v3(b"manifest", b"pixels", b"png!").unwrap()
        );
        assert_ne!(
            baseline,
            capture_commitment_from_parts_v3(b"pixels", b"manifest", b"png").unwrap()
        );
    }

    #[test]
    fn pregame_scoring_contract_binds_measurement_actions_model_and_finite_scores() {
        let deployment = deployment_v3();
        let actions = [
            MtgoPregameActionSemanticV1::Mulligan { next_hand_size: 6 },
            MtgoPregameActionSemanticV1::KeepOpeningHand,
        ];
        let request = build_pregame_scoring_request_from_parts_v3(
            MtgoOfflineMulliganLadderClassificationV1::Match,
            Some(7),
            &actions,
            &"a".repeat(64),
            &"b".repeat(64),
            &"c".repeat(64),
            &deployment,
        )
        .unwrap();
        let commitment = pregame_scoring_request_commitment_v3(&request).unwrap();
        let response = MtgoPregameScoreResponseV3 {
            schema_version: MTGO_PREGAME_EXTERNAL_SCORING_SCHEMA_V3,
            request_commitment_sha256: commitment.clone(),
            logits_f32_bits: vec![0.25_f32.to_bits(), 0.75_f32.to_bits()],
            value_f32_bits: (-0.5_f32).to_bits(),
        };
        let (selected_index, selected_semantic, selection_commitment) =
            validate_pregame_score_response_parts_v3(&request, &response).unwrap();
        assert_eq!(selected_index, 1);
        assert_eq!(
            selected_semantic,
            MtgoPregameActionSemanticV1::KeepOpeningHand
        );
        assert_eq!(selection_commitment.len(), 64);

        let tied = MtgoPregameScoreResponseV3 {
            logits_f32_bits: vec![1.0_f32.to_bits(), 1.0_f32.to_bits()],
            ..response.clone()
        };
        assert_eq!(
            validate_pregame_score_response_parts_v3(&request, &tied)
                .unwrap()
                .0,
            0
        );

        let stale = MtgoPregameScoreResponseV3 {
            request_commitment_sha256: "d".repeat(64),
            ..response.clone()
        };
        assert!(validate_pregame_score_response_parts_v3(&request, &stale).is_err());
        let nonfinite = MtgoPregameScoreResponseV3 {
            logits_f32_bits: vec![f32::NAN.to_bits(), 0.0_f32.to_bits()],
            ..response.clone()
        };
        assert!(validate_pregame_score_response_parts_v3(&request, &nonfinite).is_err());
        let wrong_count = MtgoPregameScoreResponseV3 {
            logits_f32_bits: vec![0.0_f32.to_bits()],
            ..response
        };
        assert!(validate_pregame_score_response_parts_v3(&request, &wrong_count).is_err());

        assert!(build_pregame_scoring_request_from_parts_v3(
            MtgoOfflineMulliganLadderClassificationV1::NoMatch,
            None,
            &[],
            &"a".repeat(64),
            &"b".repeat(64),
            &"c".repeat(64),
            &deployment,
        )
        .is_err());
        assert!(build_pregame_scoring_request_from_parts_v3(
            MtgoOfflineMulliganLadderClassificationV1::Match,
            Some(7),
            &[actions[1].clone(), actions[0].clone()],
            &"a".repeat(64),
            &"b".repeat(64),
            &"c".repeat(64),
            &deployment,
        )
        .is_err());
    }

    #[test]
    fn card_aware_pregame_scoring_binds_all_seven_cards_and_response() {
        let deployment = deployment_v3();
        let actions = [
            MtgoPregameActionSemanticV1::Mulligan { next_hand_size: 5 },
            MtgoPregameActionSemanticV1::KeepOpeningHand,
        ];
        let names = [
            "Plains", "Island", "Plains", "Island", "Plains", "Island", "Plains",
        ]
        .map(str::to_owned);
        let request = build_card_aware_pregame_scoring_request_from_names_v4(
            Some(6),
            &names,
            &actions,
            &"a".repeat(64),
            &"b".repeat(64),
            &"c".repeat(64),
            &"d".repeat(64),
            &"e".repeat(64),
            &deployment,
        )
        .unwrap();
        assert_eq!(request.ordered_visible_card_names, names);
        let commitment = card_aware_pregame_scoring_request_commitment_v4(&request).unwrap();
        let mut reordered = request.clone();
        reordered.ordered_visible_card_names.swap(0, 1);
        assert_ne!(
            commitment,
            card_aware_pregame_scoring_request_commitment_v4(&reordered).unwrap()
        );

        let response = MtgoCardAwarePregameScoreResponseV4 {
            schema_version: MTGO_PREGAME_CARD_AWARE_SCORING_SCHEMA_V4,
            request_commitment_sha256: commitment,
            logits_f32_bits: vec![0.75_f32.to_bits(), 0.25_f32.to_bits()],
            value_f32_bits: 0.5_f32.to_bits(),
        };
        let (selected_index, selected_semantic, selection_commitment) =
            validate_card_aware_pregame_score_response_parts_v4(&request, &response).unwrap();
        assert_eq!(selected_index, 0);
        assert_eq!(
            selected_semantic,
            MtgoPregameActionSemanticV1::Mulligan { next_hand_size: 5 }
        );
        assert_eq!(selection_commitment.len(), 64);

        let stale = MtgoCardAwarePregameScoreResponseV4 {
            request_commitment_sha256: "f".repeat(64),
            ..response.clone()
        };
        assert!(validate_card_aware_pregame_score_response_parts_v4(&request, &stale).is_err());
        let nonfinite = MtgoCardAwarePregameScoreResponseV4 {
            logits_f32_bits: vec![f32::INFINITY.to_bits(), 0.0_f32.to_bits()],
            ..response
        };
        assert!(validate_card_aware_pregame_score_response_parts_v4(&request, &nonfinite).is_err());

        assert!(build_card_aware_pregame_scoring_request_from_names_v4(
            Some(6),
            &names[..6],
            &actions,
            &"a".repeat(64),
            &"b".repeat(64),
            &"c".repeat(64),
            &"d".repeat(64),
            &"e".repeat(64),
            &deployment,
        )
        .is_err());
    }

    #[test]
    fn pregame_action_plan_binds_exact_control_pixels_and_required_transition() {
        let pixels = vec![0_u8; 1_550 * 925 * 4];
        let source = capture_commitments_for_pixels_v3(&pixels, 'a', 100);
        let mulligan = build_pregame_action_plan_parts_v3(
            &MtgoPregameActionSemanticV1::Mulligan { next_hand_size: 6 },
            Some(7),
            PREGAME_MULLIGAN_LADDER_PROFILE_COMMITMENT_V3,
            &"b".repeat(64),
            &"c".repeat(64),
            &source,
            &"d".repeat(64),
            &pixels,
        )
        .unwrap();
        assert_eq!(mulligan.control_id, "mulligan");
        assert_eq!(mulligan.control_rect_client_px.x, 29);
        assert_eq!(mulligan.target_point_client_px.x, 70);
        assert_eq!(mulligan.target_point_client_px.y, 169);
        assert_eq!(mulligan.observed_control_region_sha256.len(), 64);
        assert_eq!(
            mulligan.planned_postcondition,
            MtgoPlannedPregamePostconditionV3::NextMulliganPrompt {
                prospective_keep_size: 6
            }
        );
        assert_eq!(mulligan.action_plan_commitment_sha256.len(), 64);

        let mut animated_pixels = pixels.clone();
        let first_control_pixel = (153 * 1_550 + 29) * 4;
        animated_pixels[first_control_pixel] = 1;
        let animated_source = capture_commitments_for_pixels_v3(&animated_pixels, 'e', 100);
        let animated = build_pregame_action_plan_parts_v3(
            &MtgoPregameActionSemanticV1::Mulligan { next_hand_size: 6 },
            Some(7),
            PREGAME_MULLIGAN_LADDER_PROFILE_COMMITMENT_V3,
            &"b".repeat(64),
            &"c".repeat(64),
            &animated_source,
            &"d".repeat(64),
            &animated_pixels,
        )
        .unwrap();
        assert_ne!(
            animated.observed_control_region_sha256,
            mulligan.observed_control_region_sha256
        );
        assert_ne!(
            animated.action_plan_commitment_sha256,
            mulligan.action_plan_commitment_sha256
        );

        let keep_seven = build_pregame_action_plan_parts_v3(
            &MtgoPregameActionSemanticV1::KeepOpeningHand,
            Some(7),
            PREGAME_MULLIGAN_LADDER_PROFILE_COMMITMENT_V3,
            &"b".repeat(64),
            &"c".repeat(64),
            &source,
            &"d".repeat(64),
            &pixels,
        )
        .unwrap();
        assert_eq!(keep_seven.control_id, "keep");
        assert_eq!(
            keep_seven.planned_postcondition,
            MtgoPlannedPregamePostconditionV3::GameplayFirstMain
        );

        let keep_four = build_pregame_action_plan_parts_v3(
            &MtgoPregameActionSemanticV1::KeepOpeningHand,
            Some(4),
            PREGAME_MULLIGAN_LADDER_PROFILE_COMMITMENT_V3,
            &"b".repeat(64),
            &"c".repeat(64),
            &source,
            &"d".repeat(64),
            &pixels,
        )
        .unwrap();
        assert_eq!(
            keep_four.planned_postcondition,
            MtgoPlannedPregamePostconditionV3::LondonBottoming {
                required_bottom_count: 3
            }
        );

        assert!(build_pregame_action_plan_parts_v3(
            &MtgoPregameActionSemanticV1::Mulligan { next_hand_size: 0 },
            Some(1),
            PREGAME_MULLIGAN_LADDER_PROFILE_COMMITMENT_V3,
            &"b".repeat(64),
            &"c".repeat(64),
            &source,
            &"d".repeat(64),
            &pixels,
        )
        .is_err());

        let mut changed_pixels = pixels.clone();
        changed_pixels[0] = 1;
        assert!(build_pregame_action_plan_parts_v3(
            &MtgoPregameActionSemanticV1::KeepOpeningHand,
            Some(7),
            PREGAME_MULLIGAN_LADDER_PROFILE_COMMITMENT_V3,
            &"b".repeat(64),
            &"c".repeat(64),
            &source,
            &"d".repeat(64),
            &changed_pixels,
        )
        .is_err());
    }

    #[test]
    fn mulligan_confirmation_requires_exact_new_prompt_and_stable_identity() {
        let pixels = vec![0_u8; 1_550 * 925 * 4];
        let source = capture_commitments_for_pixels_v3(&pixels, 'a', 100);
        let mut after = source.clone();
        after.capture_commitment_sha256 = "e".repeat(64);
        after.canonical_bgra8_sha256 = "f".repeat(64);
        after.captured_at_unix_millis = 101;
        let expected = MtgoPlannedPregamePostconditionV3::NextMulliganPrompt {
            prospective_keep_size: 6,
        };
        let confirmed = validate_mulligan_postcondition_parts_v3(
            &"1".repeat(64),
            &expected,
            &source,
            &"2".repeat(64),
            MtgoOfflineMulliganLadderClassificationV1::Match,
            Some(6),
            PREGAME_MULLIGAN_LADDER_PROFILE_COMMITMENT_V3,
            &"3".repeat(64),
            &after,
            &"2".repeat(64),
        )
        .unwrap();
        assert_eq!(confirmed.len(), 64);

        let mut stale = after.clone();
        stale.captured_at_unix_millis = 100;
        assert!(validate_mulligan_postcondition_parts_v3(
            &"1".repeat(64),
            &expected,
            &source,
            &"2".repeat(64),
            MtgoOfflineMulliganLadderClassificationV1::Match,
            Some(6),
            PREGAME_MULLIGAN_LADDER_PROFILE_COMMITMENT_V3,
            &"3".repeat(64),
            &stale,
            &"2".repeat(64),
        )
        .is_err());
        assert!(validate_mulligan_postcondition_parts_v3(
            &"1".repeat(64),
            &expected,
            &source,
            &"2".repeat(64),
            MtgoOfflineMulliganLadderClassificationV1::Match,
            Some(5),
            PREGAME_MULLIGAN_LADDER_PROFILE_COMMITMENT_V3,
            &"3".repeat(64),
            &after,
            &"2".repeat(64),
        )
        .is_err());
        assert!(validate_mulligan_postcondition_parts_v3(
            &"1".repeat(64),
            &expected,
            &source,
            &"2".repeat(64),
            MtgoOfflineMulliganLadderClassificationV1::Match,
            Some(6),
            PREGAME_MULLIGAN_LADDER_PROFILE_COMMITMENT_V3,
            &"3".repeat(64),
            &after,
            &"4".repeat(64),
        )
        .is_err());

        assert!(validate_mulligan_postcondition_parts_v3(
            &"1".repeat(64),
            &MtgoPlannedPregamePostconditionV3::GameplayFirstMain,
            &source,
            &"2".repeat(64),
            MtgoOfflineMulliganLadderClassificationV1::NoMatch,
            None,
            PREGAME_MULLIGAN_LADDER_PROFILE_COMMITMENT_V3,
            &"3".repeat(64),
            &after,
            &"2".repeat(64),
        )
        .is_err());
    }

    #[test]
    fn keep_confirmation_requires_exact_new_turn_one_first_main() {
        let pixels = vec![0_u8; 1_550 * 925 * 4];
        let source = capture_commitments_for_pixels_v3(&pixels, 'a', 100);
        let mut after = source.clone();
        after.capture_commitment_sha256 = "e".repeat(64);
        after.canonical_bgra8_sha256 = "f".repeat(64);
        after.captured_at_unix_millis = 101;
        let first_main = MtgoPlannedPregamePostconditionV3::GameplayFirstMain;
        let confirmed = validate_keep_first_main_postcondition_parts_v3(
            &"1".repeat(64),
            &first_main,
            &source,
            &"2".repeat(64),
            MtgoOfflineFirstMainClassificationV1::Match,
            &"3".repeat(64),
            &"4".repeat(64),
            &after,
            &"2".repeat(64),
        )
        .unwrap();
        assert_eq!(confirmed.len(), 64);

        assert!(validate_keep_first_main_postcondition_parts_v3(
            &"1".repeat(64),
            &first_main,
            &source,
            &"2".repeat(64),
            MtgoOfflineFirstMainClassificationV1::NoMatch,
            &"3".repeat(64),
            &"4".repeat(64),
            &after,
            &"2".repeat(64),
        )
        .is_err());
        assert!(validate_keep_first_main_postcondition_parts_v3(
            &"1".repeat(64),
            &MtgoPlannedPregamePostconditionV3::LondonBottoming {
                required_bottom_count: 1,
            },
            &source,
            &"2".repeat(64),
            MtgoOfflineFirstMainClassificationV1::Match,
            &"3".repeat(64),
            &"4".repeat(64),
            &after,
            &"2".repeat(64),
        )
        .is_err());

        let mut stale = after.clone();
        stale.captured_at_unix_millis = 100;
        assert!(validate_keep_first_main_postcondition_parts_v3(
            &"1".repeat(64),
            &first_main,
            &source,
            &"2".repeat(64),
            MtgoOfflineFirstMainClassificationV1::Match,
            &"3".repeat(64),
            &"4".repeat(64),
            &stale,
            &"2".repeat(64),
        )
        .is_err());
    }

    #[test]
    fn one_card_keep_confirmation_requires_exact_new_bottom_six_initial_state() {
        let pixels = vec![0_u8; 1_550 * 925 * 4];
        let source = capture_commitments_for_pixels_v3(&pixels, 'a', 100);
        let mut after = source.clone();
        after.capture_commitment_sha256 = "e".repeat(64);
        after.canonical_bgra8_sha256 = "f".repeat(64);
        after.captured_at_unix_millis = 101;
        let bottom_six = MtgoPlannedPregamePostconditionV3::LondonBottoming {
            required_bottom_count: 6,
        };
        let confirmed = validate_keep_bottom_six_postcondition_parts_v3(
            &"1".repeat(64),
            &bottom_six,
            &source,
            &"2".repeat(64),
            MtgoOfflineBottomSixInitialClassificationV1::Match,
            Some(6),
            Some(0),
            PREGAME_BOTTOM_SIX_INITIAL_PROFILE_COMMITMENT_V3,
            &"3".repeat(64),
            &after,
            &"2".repeat(64),
        )
        .unwrap();
        assert_eq!(confirmed.len(), 64);

        for (required, selected) in [(Some(5), Some(0)), (Some(6), Some(1))] {
            assert!(validate_keep_bottom_six_postcondition_parts_v3(
                &"1".repeat(64),
                &bottom_six,
                &source,
                &"2".repeat(64),
                MtgoOfflineBottomSixInitialClassificationV1::Match,
                required,
                selected,
                PREGAME_BOTTOM_SIX_INITIAL_PROFILE_COMMITMENT_V3,
                &"3".repeat(64),
                &after,
                &"2".repeat(64),
            )
            .is_err());
        }
        assert!(validate_keep_bottom_six_postcondition_parts_v3(
            &"1".repeat(64),
            &MtgoPlannedPregamePostconditionV3::LondonBottoming {
                required_bottom_count: 5,
            },
            &source,
            &"2".repeat(64),
            MtgoOfflineBottomSixInitialClassificationV1::Match,
            Some(6),
            Some(0),
            PREGAME_BOTTOM_SIX_INITIAL_PROFILE_COMMITMENT_V3,
            &"3".repeat(64),
            &after,
            &"2".repeat(64),
        )
        .is_err());
        assert!(validate_keep_bottom_six_postcondition_parts_v3(
            &"1".repeat(64),
            &bottom_six,
            &source,
            &"2".repeat(64),
            MtgoOfflineBottomSixInitialClassificationV1::NoMatch,
            None,
            None,
            PREGAME_BOTTOM_SIX_INITIAL_PROFILE_COMMITMENT_V3,
            &"3".repeat(64),
            &after,
            &"2".repeat(64),
        )
        .is_err());

        let mut stale = after.clone();
        stale.captured_at_unix_millis = 100;
        assert!(validate_keep_bottom_six_postcondition_parts_v3(
            &"1".repeat(64),
            &bottom_six,
            &source,
            &"2".repeat(64),
            MtgoOfflineBottomSixInitialClassificationV1::Match,
            Some(6),
            Some(0),
            PREGAME_BOTTOM_SIX_INITIAL_PROFILE_COMMITMENT_V3,
            &"3".repeat(64),
            &stale,
            &"2".repeat(64),
        )
        .is_err());
    }

    #[test]
    fn in_process_parts_join_rechecks_capture_and_returns_non_actionable_no_match() {
        let (manifest, mut pixels, png) = synthetic_no_match_artifact_v3();
        let measured = measure_mulligan_ladder_parts_v3(&manifest, &pixels, &png).unwrap();
        assert_eq!(
            measured.classification(),
            MtgoOfflineMulliganLadderClassificationV1::NoMatch
        );
        assert_eq!(measured.prospective_keep_size(), None);
        assert!(measured.ordered_actions().is_empty());
        assert!(!measured.safe_for_live_frame());
        assert!(!measured.safe_for_semantic_evidence());
        assert!(!measured.safe_for_observation_v5());
        assert!(!measured.safe_for_policy_scoring());
        assert!(!measured.safe_for_input());

        pixels[0] ^= 1;
        assert!(measure_mulligan_ladder_parts_v3(&manifest, &pixels, &png).is_err());
    }

    fn synthetic_no_match_artifact_v3() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let width = 1_550_u32;
        let height = 925_u32;
        let mut pixels = vec![0_u8; width as usize * height as usize * 4];
        for pixel in pixels.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
        let png = encode_preview_png(&pixels, width, height).unwrap();
        let canonical_sha256 = sha256_hex_v1(&pixels);
        let png_sha256 = sha256_hex_v1(&png);
        let signer_subject = "CN=Daybreak Game Company LLC";
        let signer_subject_sha256 = sha256_hex_v1(signer_subject.as_bytes());
        let snapshot = json!({
            "hwnd": 1,
            "process_id": 2,
            "mtgo_process_count": 1,
            "process_start_filetime_100ns": 3,
            "process_image": "C:\\MTGO.exe",
            "executable_sha256": "a".repeat(64),
            "authenticode_valid": true,
            "signer_thumbprint": "b".repeat(40),
            "signer_subject": signer_subject,
            "signer_subject_sha256": signer_subject_sha256,
            "title": "(Solitaire): Freeform: Vs. test-player",
            "dpi": 120,
            "client_rect_desktop_px": {
                "left": 0,
                "top": 0,
                "right": width,
                "bottom": height
            },
            "extended_frame_rect_desktop_px": {
                "left": 0,
                "top": 0,
                "right": width,
                "bottom": height
            },
            "foreground": true,
            "visible": true,
            "minimized": false,
            "cloaked": false,
            "hung": false,
            "display_affinity": 0,
            "desktop_composition_enabled": true,
            "cursor_showing": true,
            "cursor_x": 2_500,
            "cursor_y": 1_400,
            "cursor_inside_client": false,
            "occlusion_target_found": true,
            "occluding_windows_above": 0,
            "z_order_sha256": "d".repeat(64)
        });
        let manifest = json!({
            "schema": "mtgo-dxgi-visible-frame-candidate/v2",
            "artifact_kind": "mtgo_untrusted_dxgi_visible_frame_candidate_v2",
            "status": "checked_untrusted_not_admitted",
            "capture_backend": "dxgi_desktop_duplication_v1",
            "window_mode": "solitaire_game",
            "capture_role": "acting_player_solitaire",
            "expected_game_format": "Freeform",
            "title_rule_version": "mtgo_visible_title_rule_v2",
            "captured_at_unix_millis": 1_786_338_302_650_u64,
            "safety": {
                "safe_for_semantic_evidence": false,
                "safe_for_ocr": false,
                "safe_for_policy_scoring": false,
                "safe_for_input": false,
                "authenticode_verified_in_probe": true
            },
            "pre": snapshot.clone(),
            "post": snapshot,
            "output": {
                "adapter_index": 0,
                "output_index": 0,
                "adapter_luid_low": 59_989,
                "adapter_luid_high": 0,
                "device_name": "\\\\.\\DISPLAY2",
                "bounds_desktop_px": {
                    "left": 0,
                    "top": 0,
                    "right": 2_560,
                    "bottom": 1_440
                },
                "rotation": 1,
                "color_space": 0
            },
            "frame": {
                "last_present_time_qpc": 1,
                "last_mouse_update_time_qpc": 0,
                "accumulated_frames": 1,
                "protected_content_masked_out": false,
                "pointer_visible": false,
                "pointer_x": 0,
                "pointer_y": 0,
                "source_texture_width": 2_560,
                "source_texture_height": 1_440,
                "source_texture_format": 87,
                "canonical_width": width,
                "canonical_height": height,
                "canonical_stride": width * 4,
                "canonical_byte_length": pixels.len(),
                "canonical_bgra8_sha256": canonical_sha256,
                "preview_png_sha256": png_sha256
            },
            "files": {
                "canonical_pixels": "frame.bgra",
                "preview_png": "frame.png",
                "manifest": "manifest.json"
            }
        });
        (serde_json::to_vec_pretty(&manifest).unwrap(), pixels, png)
    }

    fn deployment_v3() -> MtgoExpectedModelDeploymentV1 {
        MtgoExpectedModelDeploymentV1 {
            schema_version: MTGO_EXTERNAL_MODEL_SCORING_SCHEMA_V1,
            deployment_id: "mtgo-pregame-test-v3".to_owned(),
            checkpoint: MtgoNativeCheckpointIdentityV1 {
                run_sha256: "1".repeat(64),
                checkpoint_manifest_sha256: "2".repeat(64),
                checkpoint_payload_sha256: "3".repeat(64),
                train_state_sha256: "4".repeat(64),
                model_parameter_sha256: "5".repeat(64),
                generation_index: 7,
            },
            scorer_contract_sha256: "6".repeat(64),
        }
    }

    fn capture_commitments_for_pixels_v3(
        pixels: &[u8],
        capture_hash_character: char,
        captured_at_unix_millis: u128,
    ) -> MtgoDxgiFrameCommitmentsV3 {
        MtgoDxgiFrameCommitmentsV3 {
            capture_commitment_sha256: std::iter::repeat_n(capture_hash_character, 64).collect(),
            canonical_bgra8_sha256: format!("{:x}", Sha256::digest(pixels)),
            preview_png_sha256: "9".repeat(64),
            canonical_width: 1_550,
            canonical_height: 925,
            client_rect_desktop_px: SignedRectV1 {
                left: 0,
                top: 0,
                right: 1_550,
                bottom: 925,
            },
            captured_at_unix_millis,
        }
    }
}
