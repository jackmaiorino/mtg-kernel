use super::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, TreeScope_Subtree};

pub const MTGO_VISIBLE_ACCESSIBILITY_PROBE_SCHEMA_V1: u32 = 1;
pub const MTGO_VISIBLE_ACCESSIBILITY_PIXEL_CORROBORATION_SCHEMA_V1: u32 = 1;
pub const MTGO_VISIBLE_ACCESSIBILITY_CATALOG_SCHEMA_V1: u32 = 1;
pub const MTGO_VISIBLE_ACCESSIBILITY_PIXEL_CATALOG_SCHEMA_V1: u32 = 1;
pub const MTGO_VISIBLE_ACCESSIBILITY_CATALOG_REVIEW_SCHEMA_V1: u32 = 1;
pub const MTGO_VISIBLE_ACCESSIBILITY_CATALOG_CASE_EVALUATION_SCHEMA_V1: u32 = 1;
pub const MTGO_VISIBLE_ACCESSIBILITY_CATALOG_REVIEW_ARTIFACT_SCHEMA_V1: u32 = 1;
pub const MTGO_VISIBLE_ACCESSIBILITY_CATALOG_CORPUS_EVALUATION_SCHEMA_V1: u32 = 1;

const MAX_VISIBLE_ACCESSIBILITY_QUERIES_V1: usize = 64;
const MAX_VISIBLE_ACCESSIBILITY_ELEMENTS_V1: i32 = 4_096;
const MAX_MATCHES_PER_QUERY_V1: usize = 64;
const MAX_VISIBLE_ACCESSIBILITY_CORPUS_CASES_V1: usize = 64;
const MAX_VISIBLE_ACCESSIBILITY_REVIEW_IMAGE_DIMENSION_V1: u32 = 8_192;
const MAX_VISIBLE_ACCESSIBILITY_REVIEW_DECODED_IMAGE_BYTES_V1: usize = 64 * 1_024 * 1_024;
const MAX_VISIBLE_ACCESSIBILITY_REVIEW_TOTAL_CROP_PNG_BYTES_V1: usize = 128 * 1_024 * 1_024;
const MAX_VISIBLE_ACCESSIBILITY_CAPTURE_BRACKET_MILLIS_V1: u128 = 5_000;
const VISIBLE_ACCESSIBILITY_REPORT_DOMAIN_V1: &[u8] = b"mtgo-visible-accessibility-report-v1";
const VISIBLE_ACCESSIBILITY_PIXEL_MATCH_SET_DOMAIN_V1: &[u8] =
    b"mtgo-visible-accessibility-pixel-match-set-v1";
const VISIBLE_ACCESSIBILITY_PIXEL_REPORT_DOMAIN_V1: &[u8] =
    b"mtgo-visible-accessibility-pixel-report-v1";
const VISIBLE_ACCESSIBILITY_CATALOG_DOMAIN_V1: &[u8] =
    b"mtgo-visible-accessibility-known-label-catalog-v1";
const VISIBLE_ACCESSIBILITY_CATALOG_REPORT_DOMAIN_V1: &[u8] =
    b"mtgo-visible-accessibility-known-label-catalog-report-v1";
const VISIBLE_ACCESSIBILITY_PIXEL_CATALOG_REPORT_DOMAIN_V1: &[u8] =
    b"mtgo-visible-accessibility-known-label-pixel-catalog-report-v1";
const VISIBLE_ACCESSIBILITY_CATALOG_REVIEW_DOMAIN_V1: &[u8] =
    b"mtgo-visible-accessibility-known-label-review-v1";
const VISIBLE_ACCESSIBILITY_CATALOG_CASE_EVALUATION_DOMAIN_V1: &[u8] =
    b"mtgo-visible-accessibility-known-label-case-evaluation-v1";
const VISIBLE_ACCESSIBILITY_CATALOG_REVIEW_ARTIFACT_DOMAIN_V1: &[u8] =
    b"mtgo-visible-accessibility-known-label-review-artifact-v1";
const VISIBLE_ACCESSIBILITY_CATALOG_CORPUS_EVALUATION_DOMAIN_V1: &[u8] =
    b"mtgo-visible-accessibility-known-label-corpus-evaluation-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoVisibleAccessibilityCatalogSliceV1 {
    Pregame,
    Gameplay,
    Bottoming,
    Sideboard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityCatalogEntrySummaryV1 {
    pub query_id: String,
    pub slice: MtgoVisibleAccessibilityCatalogSliceV1,
    pub expected_visible_text_sha256: String,
    pub exact_visible_match_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityCatalogProbeSummaryV1 {
    pub schema_version: u32,
    pub catalog_commitment_sha256: String,
    pub source_probe_commitment_sha256: String,
    pub report_commitment_sha256: String,
    pub catalog_entry_count: u32,
    pub matched_catalog_entry_count: u32,
    pub total_exact_visible_match_count: u32,
    pub entries: Vec<MtgoVisibleAccessibilityCatalogEntrySummaryV1>,
    pub raw_visible_text_exposed: bool,
    pub caller_selected_text_queries_enabled: bool,
    pub unmatched_visible_text_retained: bool,
    pub requires_same_frame_pixel_corroboration: bool,
    pub safe_for_semantic_evidence: bool,
    pub safe_for_policy_scoring: bool,
    pub safe_for_input: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityPixelCatalogEntrySummaryV1 {
    pub query_id: String,
    pub slice: MtgoVisibleAccessibilityCatalogSliceV1,
    pub expected_visible_text_sha256: String,
    pub exact_visible_match_count: u32,
    pub visible_pixel_match_set_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityPixelCatalogProbeSummaryV1 {
    pub schema_version: u32,
    pub catalog_commitment_sha256: String,
    pub source_pixel_report_commitment_sha256: String,
    pub before_frame_sha256: String,
    pub after_frame_sha256: String,
    pub entries: Vec<MtgoVisibleAccessibilityPixelCatalogEntrySummaryV1>,
    pub total_pixel_corroborated_match_count: u32,
    pub has_pixel_corroborated_match: bool,
    pub matched_regions_pixel_stable_across_bracket: bool,
    pub raw_visible_text_exposed: bool,
    pub caller_selected_text_queries_enabled: bool,
    pub unmatched_visible_text_retained: bool,
    pub private_match_rectangles_exposed: bool,
    pub safe_for_semantic_evidence: bool,
    pub safe_for_policy_scoring: bool,
    pub safe_for_input: bool,
    pub report_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityCatalogReviewEntryV1 {
    pub query_id: String,
    pub slice: MtgoVisibleAccessibilityCatalogSliceV1,
    pub expected_visible_text_sha256: String,
    pub reviewed_exact_visible_match_count: u32,
    pub reviewed_visible_pixel_match_set_commitment_sha256: String,
    pub every_matched_region_visibly_contains_exact_catalog_label: bool,
    pub visible_absence_reviewed_when_match_count_is_zero: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityCatalogReviewV1 {
    pub schema_version: u32,
    pub catalog_commitment_sha256: String,
    pub source_pixel_report_commitment_sha256: String,
    pub before_frame_sha256: String,
    pub after_frame_sha256: String,
    pub reviewer_alias_sha256: String,
    pub client_only_and_unobscured_confirmed: bool,
    pub exact_frame_pair_identity_confirmed: bool,
    pub entries: Vec<MtgoVisibleAccessibilityCatalogReviewEntryV1>,
    pub review_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityCatalogCaseEvaluationSummaryV1 {
    pub schema_version: u32,
    pub catalog_commitment_sha256: String,
    pub source_pixel_report_commitment_sha256: String,
    pub review_commitment_sha256: String,
    pub evaluated_entry_count: u32,
    pub positive_entry_count: u32,
    pub total_exact_visible_match_count: u32,
    pub covered_slices: Vec<MtgoVisibleAccessibilityCatalogSliceV1>,
    pub exact_review_agreement: bool,
    pub ratification_candidate_commitment_sha256: String,
    pub production_evaluation_ratified: bool,
    pub safe_for_semantic_evidence: bool,
    pub safe_for_policy_scoring: bool,
    pub safe_for_input: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityCatalogCorpusEntrySummaryV1 {
    pub query_id: String,
    pub slice: MtgoVisibleAccessibilityCatalogSliceV1,
    pub expected_visible_text_sha256: String,
    pub reviewed_positive_case_count: u32,
    pub reviewed_zero_case_count: u32,
    pub total_reviewed_exact_visible_match_count: u32,
    pub presence_and_absence_coverage_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityCatalogCorpusEvaluationSummaryV1 {
    pub schema_version: u32,
    pub catalog_commitment_sha256: String,
    pub reviewed_case_count: u32,
    pub distinct_visible_frame_pair_count: u32,
    pub entries: Vec<MtgoVisibleAccessibilityCatalogCorpusEntrySummaryV1>,
    pub every_catalog_entry_has_reviewed_presence_and_absence: bool,
    pub exact_frame_pairs_are_distinct_across_cases: bool,
    pub corpus_candidate_commitment_sha256: String,
    pub production_evaluation_ratified: bool,
    pub safe_for_semantic_evidence: bool,
    pub safe_for_policy_scoring: bool,
    pub safe_for_input: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityCatalogReviewArtifactReceiptV1 {
    pub schema_version: u32,
    pub artifact_commitment_sha256: String,
    pub before_visible_client_png_sha256: String,
    pub after_visible_client_png_sha256: String,
    pub catalog_entry_count: u32,
    pub exact_visible_match_crop_count: u32,
    pub contains_only_player_visible_pixels_and_opaque_commitments: bool,
    pub human_review_complete: bool,
    pub safe_for_semantic_evidence: bool,
    pub safe_for_policy_scoring: bool,
    pub safe_for_input: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityCatalogCompletedReviewReceiptV1 {
    pub schema_version: u32,
    pub catalog_commitment_sha256: String,
    pub source_pixel_report_commitment_sha256: String,
    pub review_commitment_sha256: String,
    pub reviewed_entry_count: u32,
    pub completed_review_written: bool,
    pub production_evaluation_ratified: bool,
    pub safe_for_semantic_evidence: bool,
    pub safe_for_policy_scoring: bool,
    pub safe_for_input: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrivateVisibleAccessibilityCatalogReviewCropV1 {
    file: String,
    png_sha256: String,
    region_bgra8_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrivateVisibleAccessibilityCatalogReviewArtifactEntryV1 {
    query_id: String,
    slice: MtgoVisibleAccessibilityCatalogSliceV1,
    expected_visible_text_sha256: String,
    exact_visible_match_count: u32,
    match_crops: Vec<PrivateVisibleAccessibilityCatalogReviewCropV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrivateVisibleAccessibilityCatalogReviewArtifactManifestV1 {
    schema_version: u32,
    artifact_kind: String,
    catalog_commitment_sha256: String,
    source_pixel_report_commitment_sha256: String,
    before_frame_sha256: String,
    after_frame_sha256: String,
    before_visible_client_png_file: String,
    before_visible_client_png_sha256: String,
    after_visible_client_png_file: String,
    after_visible_client_png_sha256: String,
    review_template_file: String,
    review_template_sha256: String,
    entries: Vec<PrivateVisibleAccessibilityCatalogReviewArtifactEntryV1>,
    exact_visible_match_crop_count: u32,
    raw_or_unmatched_visible_text_exposed: bool,
    pixel_coordinates_exposed: bool,
    accessibility_metadata_exposed: bool,
    process_or_window_metadata_exposed: bool,
    human_review_complete: bool,
    safe_for_semantic_evidence: bool,
    safe_for_policy_scoring: bool,
    safe_for_input: bool,
    artifact_commitment_sha256: String,
}

struct PrivateVisibleAccessibilityCatalogReviewArtifactFilesV1 {
    manifest: PrivateVisibleAccessibilityCatalogReviewArtifactManifestV1,
    manifest_bytes: Vec<u8>,
    review_template_bytes: Vec<u8>,
    before_visible_client_png: Vec<u8>,
    after_visible_client_png: Vec<u8>,
    crop_files: Vec<(String, Vec<u8>)>,
}

struct PrivateLoadedVisibleAccessibilityCatalogReviewArtifactV1 {
    _files: PrivateVisibleAccessibilityCatalogReviewArtifactFilesV1,
    catalog_summary: MtgoVisibleAccessibilityPixelCatalogProbeSummaryV1,
}

enum PrivateVisibleAccessibilityCatalogCaseSourceV1 {
    Live {
        _source: Box<OpaqueMtgoVisibleAccessibilityPixelCorroborationV1>,
    },
    ReviewedArtifact {
        _artifact: Box<PrivateLoadedVisibleAccessibilityCatalogReviewArtifactV1>,
    },
}

/// Move-only evaluation of one exact fixed-catalog pixel bracket. It retains
/// the opaque source so application code cannot replace a real captured case
/// with copied report hashes. The review is caller-authored and the result is
/// therefore only a non-authorizing ratification candidate.
pub struct CheckedUntrustedMtgoVisibleAccessibilityCatalogCaseEvaluationV1 {
    _source: PrivateVisibleAccessibilityCatalogCaseSourceV1,
    _catalog_summary: MtgoVisibleAccessibilityPixelCatalogProbeSummaryV1,
    _review: MtgoVisibleAccessibilityCatalogReviewV1,
    summary: MtgoVisibleAccessibilityCatalogCaseEvaluationSummaryV1,
}

impl CheckedUntrustedMtgoVisibleAccessibilityCatalogCaseEvaluationV1 {
    pub fn summary_v1(&self) -> MtgoVisibleAccessibilityCatalogCaseEvaluationSummaryV1 {
        self.summary.clone()
    }

    pub fn ratification_candidate_commitment_sha256_v1(&self) -> &str {
        &self.summary.ratification_candidate_commitment_sha256
    }

    pub fn production_evaluation_ratified_v1(&self) -> bool {
        self.summary.production_evaluation_ratified
    }

    pub fn safe_for_semantic_evidence_v1(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

#[derive(Clone)]
struct PrivateVisibleAccessibilityCatalogCorpusCaseV1 {
    catalog_commitment_sha256: String,
    before_frame_sha256: String,
    after_frame_sha256: String,
    case_candidate_commitment_sha256: String,
    entries: Vec<MtgoVisibleAccessibilityPixelCatalogEntrySummaryV1>,
}

/// Move-only aggregate of separately reviewed fixed-catalog cases. The
/// aggregate requires both a positive and a zero-match case for every fixed
/// label and rejects repeated visible frame pairs. It remains a candidate only;
/// no production ratification root or semantic conversion exists.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoVisibleAccessibilityCatalogCorpusEvaluationV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<CheckedUntrustedMtgoVisibleAccessibilityCatalogCorpusEvaluationV1>();
/// ```
pub struct CheckedUntrustedMtgoVisibleAccessibilityCatalogCorpusEvaluationV1 {
    _cases: Vec<CheckedUntrustedMtgoVisibleAccessibilityCatalogCaseEvaluationV1>,
    summary: MtgoVisibleAccessibilityCatalogCorpusEvaluationSummaryV1,
}

impl CheckedUntrustedMtgoVisibleAccessibilityCatalogCorpusEvaluationV1 {
    pub fn summary_v1(&self) -> MtgoVisibleAccessibilityCatalogCorpusEvaluationSummaryV1 {
        self.summary.clone()
    }

    pub fn corpus_candidate_commitment_sha256_v1(&self) -> &str {
        &self.summary.corpus_candidate_commitment_sha256
    }

    pub fn production_evaluation_ratified_v1(&self) -> bool {
        false
    }

    pub fn safe_for_semantic_evidence_v1(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PrivateVisibleAccessibilityCatalogEntryV1 {
    query_id: &'static str,
    slice: MtgoVisibleAccessibilityCatalogSliceV1,
    expected_visible_text: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityExactTextQueryV1 {
    pub query_id: String,
    pub expected_visible_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityQueryResultV1 {
    pub query_id: String,
    pub expected_visible_text_sha256: String,
    pub exact_visible_match_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityProbeSummaryV1 {
    pub schema_version: u32,
    pub query_results: Vec<MtgoVisibleAccessibilityQueryResultV1>,
    pub raw_visible_text_exposed: bool,
    pub requires_same_frame_pixel_corroboration: bool,
    pub safe_for_semantic_evidence: bool,
    pub safe_for_policy_scoring: bool,
    pub safe_for_input: bool,
    pub report_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityPixelQueryResultV1 {
    pub query_id: String,
    pub expected_visible_text_sha256: String,
    pub exact_visible_match_count: u32,
    pub visible_pixel_match_set_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityPixelCorroborationSummaryV1 {
    pub schema_version: u32,
    pub before_frame_sha256: String,
    pub after_frame_sha256: String,
    pub accessibility_report_commitment_sha256: String,
    pub query_results: Vec<MtgoVisibleAccessibilityPixelQueryResultV1>,
    pub total_pixel_corroborated_match_count: u32,
    pub has_pixel_corroborated_match: bool,
    pub matched_regions_pixel_stable_across_bracket: bool,
    pub raw_visible_text_exposed: bool,
    pub private_match_rectangles_exposed: bool,
    pub safe_for_semantic_evidence: bool,
    pub safe_for_policy_scoring: bool,
    pub safe_for_input: bool,
    pub report_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PrivateVisibleAccessibilityMatchV1 {
    query_index: usize,
    control_type_id: i32,
    rect_desktop_px: SignedRectV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PrivateVisibleAccessibilityPixelMatchV1 {
    query_index: usize,
    control_type_id: i32,
    rect_client_px: MtgoRectPxV1,
    region_bgra8_sha256: String,
}

/// Read-only exact-text matches from Windows UI Automation. Raw names and
/// rectangles remain private. The result has no screenshot, semantic,
/// scoring, control-pattern, focus, or input authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoVisibleAccessibilityProbeV1;
/// let _forged = OpaqueMtgoVisibleAccessibilityProbeV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoVisibleAccessibilityProbeV1;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoVisibleAccessibilityProbeV1>();
/// ```
pub struct OpaqueMtgoVisibleAccessibilityProbeV1 {
    _queries: Vec<MtgoVisibleAccessibilityExactTextQueryV1>,
    _private_matches: Vec<PrivateVisibleAccessibilityMatchV1>,
    _window_snapshot: WindowSnapshotV1,
    summary: MtgoVisibleAccessibilityProbeSummaryV1,
}

impl OpaqueMtgoVisibleAccessibilityProbeV1 {
    pub fn summary_v1(&self) -> MtgoVisibleAccessibilityProbeSummaryV1 {
        self.summary.clone()
    }

    pub fn grants_capture_authority_v1(&self) -> bool {
        false
    }

    pub fn safe_for_semantic_evidence_v1(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

/// A move-only exact-region pixel corroboration of visible UI Automation
/// matches. The query text, private rectangles, pixels, and both source frames
/// remain private. The result is still diagnostic because it has no reviewed
/// label-accuracy profile and grants no semantic, scoring, or input authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoVisibleAccessibilityPixelCorroborationV1;
/// let _forged = OpaqueMtgoVisibleAccessibilityPixelCorroborationV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoVisibleAccessibilityPixelCorroborationV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoVisibleAccessibilityPixelCorroborationV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoVisibleAccessibilityPixelCorroborationV1;
/// fn pixels(value: &OpaqueMtgoVisibleAccessibilityPixelCorroborationV1) {
///     let _ = value.canonical_bgra8();
///     let _ = value.match_rectangles();
/// }
/// ```
pub struct OpaqueMtgoVisibleAccessibilityPixelCorroborationV1 {
    _before_frame: OpaqueMtgoDxgiFrameCandidateV3,
    _accessibility_probe: OpaqueMtgoVisibleAccessibilityProbeV1,
    _after_frame: OpaqueMtgoDxgiFrameCandidateV3,
    _private_pixel_matches: Vec<PrivateVisibleAccessibilityPixelMatchV1>,
    summary: MtgoVisibleAccessibilityPixelCorroborationSummaryV1,
}

impl OpaqueMtgoVisibleAccessibilityPixelCorroborationV1 {
    pub fn summary_v1(&self) -> MtgoVisibleAccessibilityPixelCorroborationSummaryV1 {
        self.summary.clone()
    }

    pub fn safe_for_semantic_evidence_v1(&self) -> bool {
        false
    }

    pub fn safe_for_policy_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

/// Probes only accessibility elements that Windows reports as on-screen,
/// process-bound, nonempty, and fully contained in the visible MTGO client.
/// `CurrentName` is read only after all of those checks pass. Exact matches
/// remain diagnostic until a future single-frame composed-pixel binder
/// corroborates the same private rectangles. The separately reviewed static
/// entry classifier may consume only its fixed exact-label subset and exports
/// only one bounded deck-gate state, never arbitrary names or control state.
pub fn probe_mtgo_visible_accessibility_exact_text_v1(
    window_request: MtgoDxgiCaptureRequestV3,
    queries: Vec<MtgoVisibleAccessibilityExactTextQueryV1>,
) -> Result<OpaqueMtgoVisibleAccessibilityProbeV1, String> {
    validate_capture_request_v3(&window_request)?;
    validate_queries_v1(&queries)?;
    let _dpi_guard = enter_per_monitor_v2()?;

    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return Err("visible accessibility probe found no foreground window".to_owned());
    }
    let pre = snapshot_window(hwnd, &window_request)?;
    require_admitted_window(&pre, None)?;

    let _com = ComApartmentGuardV1::initialize_v1()?;
    let automation: IUIAutomation = unsafe {
        CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| format!("create UI Automation client: {error}"))?
    };
    let root = unsafe {
        automation
            .ElementFromHandle(hwnd)
            .map_err(|error| format!("bind UI Automation to foreground MTGO window: {error}"))?
    };
    let root_process_id = unsafe {
        root.CurrentProcessId()
            .map_err(|error| format!("read UI Automation root process: {error}"))?
    };
    if u32::try_from(root_process_id).ok() != Some(pre.process_id) {
        return Err("UI Automation root is not owned by the admitted MTGO process".to_owned());
    }
    let condition = unsafe {
        automation
            .CreateTrueCondition()
            .map_err(|error| format!("create UI Automation traversal condition: {error}"))?
    };
    let elements = unsafe {
        root.FindAll(TreeScope_Subtree, &condition)
            .map_err(|error| format!("enumerate UI Automation subtree: {error}"))?
    };
    let element_count = unsafe {
        elements
            .Length()
            .map_err(|error| format!("read UI Automation element count: {error}"))?
    };
    if !(0..=MAX_VISIBLE_ACCESSIBILITY_ELEMENTS_V1).contains(&element_count) {
        return Err("UI Automation subtree is outside the bounded element count".to_owned());
    }

    let mut private_matches = Vec::new();
    for index in 0..element_count {
        let element = unsafe {
            elements
                .GetElement(index)
                .map_err(|error| format!("read UI Automation element {index}: {error}"))?
        };
        let element_process_id = unsafe {
            element
                .CurrentProcessId()
                .map_err(|error| format!("read UI Automation element process {index}: {error}"))?
        };
        if u32::try_from(element_process_id).ok() != Some(pre.process_id) {
            continue;
        }
        let is_offscreen = unsafe {
            element
                .CurrentIsOffscreen()
                .map_err(|error| format!("read UI Automation visibility {index}: {error}"))?
        };
        if is_offscreen.as_bool() {
            continue;
        }
        let rect = unsafe {
            element
                .CurrentBoundingRectangle()
                .map_err(|error| format!("read UI Automation bounds {index}: {error}"))?
        };
        let rect = SignedRectV1 {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        };
        if rect.width().is_err()
            || rect.height().is_err()
            || !pre.client_rect_desktop_px.contains(rect)
        {
            continue;
        }
        // Keep this read after process, off-screen, and client containment
        // checks. The probe never reads names from hidden or off-client UIA.
        let name = unsafe {
            element
                .CurrentName()
                .map_err(|error| format!("read visible UI Automation name {index}: {error}"))?
                .to_string()
        };
        if name.is_empty() {
            continue;
        }
        for (query_index, query) in queries.iter().enumerate() {
            if name == query.expected_visible_text {
                let width = rect.width().map_err(str::to_owned)?;
                let height = rect.height().map_err(str::to_owned)?;
                let center = POINT {
                    x: rect
                        .left
                        .checked_add(i32::try_from(width / 2).map_err(|_| {
                            "visible UI Automation horizontal center overflow".to_owned()
                        })?)
                        .ok_or("visible UI Automation horizontal center overflow")?,
                    y: rect
                        .top
                        .checked_add(i32::try_from(height / 2).map_err(|_| {
                            "visible UI Automation vertical center overflow".to_owned()
                        })?)
                        .ok_or("visible UI Automation vertical center overflow")?,
                };
                let hit = unsafe {
                    automation.ElementFromPoint(center).map_err(|error| {
                        format!("hit-test visible UI Automation element {index}: {error}")
                    })?
                };
                if !unsafe {
                    automation
                        .CompareElements(&element, &hit)
                        .map_err(|error| {
                            format!("compare hit-tested UI Automation element {index}: {error}")
                        })?
                        .as_bool()
                } {
                    continue;
                }
                let control_type_id = unsafe {
                    element
                        .CurrentControlType()
                        .map_err(|error| {
                            format!("read visible UI Automation control type {index}: {error}")
                        })?
                        .0
                };
                private_matches.push(PrivateVisibleAccessibilityMatchV1 {
                    query_index,
                    control_type_id,
                    rect_desktop_px: rect,
                });
            }
        }
    }

    let post = snapshot_window(hwnd, &window_request)?;
    require_admitted_window(&post, None)?;
    if pre != post {
        return Err(
            "MTGO window, process, visibility, cursor, geometry, or z-order changed during accessibility probe"
                .to_owned(),
        );
    }
    private_matches.sort_by_key(|matched| {
        (
            matched.query_index,
            matched.rect_desktop_px.top,
            matched.rect_desktop_px.left,
            matched.rect_desktop_px.bottom,
            matched.rect_desktop_px.right,
            matched.control_type_id,
        )
    });

    let query_results = build_query_results_v1(&queries, &private_matches)?;
    let mut summary = MtgoVisibleAccessibilityProbeSummaryV1 {
        schema_version: MTGO_VISIBLE_ACCESSIBILITY_PROBE_SCHEMA_V1,
        query_results,
        raw_visible_text_exposed: false,
        requires_same_frame_pixel_corroboration: true,
        safe_for_semantic_evidence: false,
        safe_for_policy_scoring: false,
        safe_for_input: false,
        report_commitment_sha256: String::new(),
    };
    summary.report_commitment_sha256 = summary_commitment_v1(&summary)?;
    Ok(OpaqueMtgoVisibleAccessibilityProbeV1 {
        _queries: queries,
        _private_matches: private_matches,
        _window_snapshot: pre,
        summary,
    })
}

/// Captures one admitted composed-desktop frame, performs the read-only UIA
/// query, then captures a second admitted frame. Every private UIA match region
/// must contain identical canonical BGRA8 bytes in both exact frames. Window,
/// process, geometry, cursor, z-order, and output identity must remain stable
/// throughout the bracket.
pub fn probe_mtgo_visible_accessibility_exact_text_with_pixel_corroboration_v1(
    window_request: MtgoDxgiCaptureRequestV3,
    queries: Vec<MtgoVisibleAccessibilityExactTextQueryV1>,
) -> Result<OpaqueMtgoVisibleAccessibilityPixelCorroborationV1, String> {
    validate_capture_request_v3(&window_request)?;
    validate_queries_v1(&queries)?;
    let before_frame = capture_mtgo_dxgi_frame_candidate_v3(window_request.clone())?;
    let accessibility_probe =
        probe_mtgo_visible_accessibility_exact_text_v1(window_request.clone(), queries)?;
    let after_frame = capture_mtgo_dxgi_frame_candidate_v3(window_request)?;
    bind_visible_accessibility_pixel_corroboration_v1(
        before_frame,
        accessibility_probe,
        after_frame,
    )
}

fn bind_visible_accessibility_pixel_corroboration_v1(
    before_frame: OpaqueMtgoDxgiFrameCandidateV3,
    accessibility_probe: OpaqueMtgoVisibleAccessibilityProbeV1,
    after_frame: OpaqueMtgoDxgiFrameCandidateV3,
) -> Result<OpaqueMtgoVisibleAccessibilityPixelCorroborationV1, String> {
    let before = &before_frame.manifest;
    let after = &after_frame.manifest;
    if before.pre != before.post
        || after.pre != after.post
        || before.pre != accessibility_probe._window_snapshot
        || after.pre != accessibility_probe._window_snapshot
        || before.output != after.output
        || before.window_mode != after.window_mode
        || before.capture_role != after.capture_role
        || before.expected_game_format != after.expected_game_format
        || before.title_rule_version != after.title_rule_version
        || before.frame.canonical_width != after.frame.canonical_width
        || before.frame.canonical_height != after.frame.canonical_height
        || before.frame.canonical_stride != after.frame.canonical_stride
        || before.frame.source_texture_width != after.frame.source_texture_width
        || before.frame.source_texture_height != after.frame.source_texture_height
        || before.frame.source_texture_format != after.frame.source_texture_format
        || before.frame.pointer_visible != after.frame.pointer_visible
        || before.frame.pointer_x != after.frame.pointer_x
        || before.frame.pointer_y != after.frame.pointer_y
        || before.captured_at_unix_millis > after.captured_at_unix_millis
        || after.captured_at_unix_millis - before.captured_at_unix_millis
            > MAX_VISIBLE_ACCESSIBILITY_CAPTURE_BRACKET_MILLIS_V1
    {
        return Err(
            "MTGO capture bracket changed process, window, geometry, cursor, z-order, output, or role identity"
                .to_owned(),
        );
    }
    if accessibility_probe.summary.report_commitment_sha256
        != summary_commitment_v1(&accessibility_probe.summary)?
    {
        return Err("accessibility report does not bind the exact capture bracket".to_owned());
    }
    for (frame, label) in [(&before_frame, "before"), (&after_frame, "after")] {
        let expected_length = usize::try_from(frame.manifest.frame.canonical_width)
            .ok()
            .and_then(|width| {
                usize::try_from(frame.manifest.frame.canonical_height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| format!("{label} corroboration frame geometry overflows"))?;
        if frame.canonical_bgra8.len() != expected_length
            || frame.manifest.frame.canonical_stride
                != frame
                    .manifest
                    .frame
                    .canonical_width
                    .checked_mul(4)
                    .ok_or_else(|| format!("{label} corroboration frame stride overflows"))?
            || frame.manifest.frame.canonical_bgra8_sha256 != sha256_hex_v1(&frame.canonical_bgra8)
            || frame.capture_commitment_sha256
                != capture_commitment_v3(
                    &frame.manifest,
                    &frame.canonical_bgra8,
                    &frame.preview_png,
                )?
        {
            return Err(format!(
                "{label} corroboration frame bytes or commitment are invalid"
            ));
        }
    }

    let size = MtgoSizePxV1 {
        width: before.frame.canonical_width,
        height: before.frame.canonical_height,
    };
    if before.pre.client_rect_desktop_px.width()? != size.width
        || before.pre.client_rect_desktop_px.height()? != size.height
    {
        return Err("capture bracket client bounds do not match canonical pixels".to_owned());
    }
    let private_pixel_matches = corroborate_private_match_regions_v1(
        &accessibility_probe._private_matches,
        before.pre.client_rect_desktop_px,
        &before_frame.canonical_bgra8,
        &after_frame.canonical_bgra8,
        &size,
    )?;
    let query_results =
        build_pixel_query_results_v1(&accessibility_probe._queries, &private_pixel_matches)?;
    let total_pixel_corroborated_match_count =
        query_results.iter().try_fold(0_u32, |total, result| {
            total
                .checked_add(result.exact_visible_match_count)
                .ok_or("visible accessibility total pixel match count overflow")
        })?;
    let expected_probe_results = build_query_results_v1(
        &accessibility_probe._queries,
        &accessibility_probe._private_matches,
    )?;
    for (plain, pixel) in expected_probe_results.iter().zip(query_results.iter()) {
        if plain.query_id != pixel.query_id
            || plain.expected_visible_text_sha256 != pixel.expected_visible_text_sha256
            || plain.exact_visible_match_count != pixel.exact_visible_match_count
        {
            return Err("pixel corroboration changed the accessibility match inventory".to_owned());
        }
    }

    let mut summary = MtgoVisibleAccessibilityPixelCorroborationSummaryV1 {
        schema_version: MTGO_VISIBLE_ACCESSIBILITY_PIXEL_CORROBORATION_SCHEMA_V1,
        before_frame_sha256: before.frame.canonical_bgra8_sha256.clone(),
        after_frame_sha256: after.frame.canonical_bgra8_sha256.clone(),
        accessibility_report_commitment_sha256: accessibility_probe
            .summary
            .report_commitment_sha256
            .clone(),
        query_results,
        total_pixel_corroborated_match_count,
        has_pixel_corroborated_match: total_pixel_corroborated_match_count != 0,
        matched_regions_pixel_stable_across_bracket: true,
        raw_visible_text_exposed: false,
        private_match_rectangles_exposed: false,
        safe_for_semantic_evidence: false,
        safe_for_policy_scoring: false,
        safe_for_input: false,
        report_commitment_sha256: String::new(),
    };
    summary.report_commitment_sha256 = pixel_summary_commitment_v1(&summary)?;
    Ok(OpaqueMtgoVisibleAccessibilityPixelCorroborationV1 {
        _before_frame: before_frame,
        _accessibility_probe: accessibility_probe,
        _after_frame: after_frame,
        _private_pixel_matches: private_pixel_matches,
        summary,
    })
}

pub fn run_visible_accessibility_probe_cli_v1(
) -> Result<MtgoVisibleAccessibilityProbeSummaryV1, String> {
    let (window_request, queries) = parse_cli_v1()?;
    Ok(probe_mtgo_visible_accessibility_exact_text_v1(window_request, queries)?.summary_v1())
}

pub fn run_visible_accessibility_pixel_corroboration_cli_v1(
) -> Result<MtgoVisibleAccessibilityPixelCorroborationSummaryV1, String> {
    let (window_request, queries) = parse_cli_v1()?;
    Ok(
        probe_mtgo_visible_accessibility_exact_text_with_pixel_corroboration_v1(
            window_request,
            queries,
        )?
        .summary_v1(),
    )
}

/// Probes only a compile-time catalog of exact labels previously observed in
/// retained player-visible MTGO frames. The function does not return or persist
/// arbitrary accessibility text: it matches only the compile-time exact label
/// catalog. Raw catalog strings and rectangles remain private; the summary
/// contains hashes, counts, control types, and commitments only. Matches remain
/// diagnostic pending same-frame pixels and a reviewed semantic profile.
pub fn probe_mtgo_visible_accessibility_known_label_catalog_v1(
    window_request: MtgoDxgiCaptureRequestV3,
) -> Result<MtgoVisibleAccessibilityCatalogProbeSummaryV1, String> {
    let catalog = known_label_catalog_v1();
    validate_known_label_catalog_v1(&catalog)?;
    let catalog_commitment_sha256 = known_label_catalog_commitment_v1(&catalog)?;
    let queries = catalog
        .iter()
        .map(|entry| MtgoVisibleAccessibilityExactTextQueryV1 {
            query_id: entry.query_id.to_owned(),
            expected_visible_text: entry.expected_visible_text.to_owned(),
        })
        .collect::<Vec<_>>();
    let source =
        probe_mtgo_visible_accessibility_exact_text_v1(window_request, queries)?.summary_v1();
    let entries = catalog
        .iter()
        .zip(&source.query_results)
        .map(
            |(entry, result)| MtgoVisibleAccessibilityCatalogEntrySummaryV1 {
                query_id: entry.query_id.to_owned(),
                slice: entry.slice,
                expected_visible_text_sha256: result.expected_visible_text_sha256.clone(),
                exact_visible_match_count: result.exact_visible_match_count,
            },
        )
        .collect::<Vec<_>>();
    let matched_catalog_entry_count = u32::try_from(
        entries
            .iter()
            .filter(|entry| entry.exact_visible_match_count > 0)
            .count(),
    )
    .map_err(|_| "visible accessibility matched catalog count overflow")?;
    let total_exact_visible_match_count = entries.iter().try_fold(0_u32, |total, entry| {
        total
            .checked_add(entry.exact_visible_match_count)
            .ok_or("visible accessibility total catalog match count overflow")
    })?;
    let catalog_entry_count =
        u32::try_from(entries.len()).map_err(|_| "visible accessibility catalog count overflow")?;
    let report_commitment_sha256 = known_label_catalog_report_commitment_v1(
        &catalog_commitment_sha256,
        &source.report_commitment_sha256,
        catalog_entry_count,
        matched_catalog_entry_count,
        total_exact_visible_match_count,
        &entries,
    )?;
    Ok(MtgoVisibleAccessibilityCatalogProbeSummaryV1 {
        schema_version: MTGO_VISIBLE_ACCESSIBILITY_CATALOG_SCHEMA_V1,
        catalog_commitment_sha256,
        source_probe_commitment_sha256: source.report_commitment_sha256,
        report_commitment_sha256,
        catalog_entry_count,
        matched_catalog_entry_count,
        total_exact_visible_match_count,
        entries,
        raw_visible_text_exposed: false,
        caller_selected_text_queries_enabled: false,
        unmatched_visible_text_retained: false,
        requires_same_frame_pixel_corroboration: true,
        safe_for_semantic_evidence: false,
        safe_for_policy_scoring: false,
        safe_for_input: false,
    })
}

pub fn run_visible_accessibility_known_label_catalog_cli_v1(
) -> Result<MtgoVisibleAccessibilityCatalogProbeSummaryV1, String> {
    let window_request = parse_catalog_cli_v1()?;
    probe_mtgo_visible_accessibility_known_label_catalog_v1(window_request)
}

/// Runs the fixed known-label catalog through the composed-pixel capture
/// bracket. Raw labels, unmatched UIA names, rectangles, and pixels remain
/// private. The result is diagnostic and cannot become semantic evidence,
/// policy input, or client input without a separately reviewed profile.
pub fn probe_mtgo_visible_accessibility_known_label_catalog_with_pixel_corroboration_v1(
    window_request: MtgoDxgiCaptureRequestV3,
) -> Result<MtgoVisibleAccessibilityPixelCatalogProbeSummaryV1, String> {
    let catalog = known_label_catalog_v1();
    validate_known_label_catalog_v1(&catalog)?;
    let catalog_commitment_sha256 = known_label_catalog_commitment_v1(&catalog)?;
    let queries = known_label_queries_v1(&catalog);
    let source = probe_mtgo_visible_accessibility_exact_text_with_pixel_corroboration_v1(
        window_request,
        queries,
    )?
    .summary_v1();
    build_known_label_pixel_catalog_summary_v1(&catalog, catalog_commitment_sha256, source)
}

pub fn probe_mtgo_visible_accessibility_known_label_catalog_evaluation_source_v1(
    window_request: MtgoDxgiCaptureRequestV3,
) -> Result<OpaqueMtgoVisibleAccessibilityPixelCorroborationV1, String> {
    let catalog = known_label_catalog_v1();
    validate_known_label_catalog_v1(&catalog)?;
    probe_mtgo_visible_accessibility_exact_text_with_pixel_corroboration_v1(
        window_request,
        known_label_queries_v1(&catalog),
    )
}

/// Writes one human-reviewable, non-authorizing artifact from the opaque fixed
/// catalog bracket. The artifact contains the before and after composed visible
/// MTGO client PNGs and tightly cropped visible pixels for exact fixed-label
/// matches. It omits
/// raw discovered text, coordinates, UIA metadata, process/window metadata,
/// and all input capability. The source is consumed so the artifact cannot be
/// detached from the exact private match inventory.
pub fn persist_mtgo_visible_accessibility_catalog_review_artifact_v1(
    source: OpaqueMtgoVisibleAccessibilityPixelCorroborationV1,
    output: &Path,
) -> Result<MtgoVisibleAccessibilityCatalogReviewArtifactReceiptV1, String> {
    let output = validate_visible_accessibility_review_output_v1(output)?;
    let files = build_visible_accessibility_catalog_review_artifact_v1(&source)?;
    persist_visible_accessibility_catalog_review_artifact_v1(&output, &files)?;
    Ok(MtgoVisibleAccessibilityCatalogReviewArtifactReceiptV1 {
        schema_version: MTGO_VISIBLE_ACCESSIBILITY_CATALOG_REVIEW_ARTIFACT_SCHEMA_V1,
        artifact_commitment_sha256: files.manifest.artifact_commitment_sha256.clone(),
        before_visible_client_png_sha256: files.manifest.before_visible_client_png_sha256.clone(),
        after_visible_client_png_sha256: files.manifest.after_visible_client_png_sha256.clone(),
        catalog_entry_count: u32::try_from(files.manifest.entries.len())
            .map_err(|_| "visible accessibility review catalog count overflow")?,
        exact_visible_match_crop_count: files.manifest.exact_visible_match_crop_count,
        contains_only_player_visible_pixels_and_opaque_commitments: true,
        human_review_complete: false,
        safe_for_semantic_evidence: false,
        safe_for_policy_scoring: false,
        safe_for_input: false,
    })
}

fn build_visible_accessibility_catalog_review_artifact_v1(
    source: &OpaqueMtgoVisibleAccessibilityPixelCorroborationV1,
) -> Result<PrivateVisibleAccessibilityCatalogReviewArtifactFilesV1, String> {
    let catalog = known_label_catalog_v1();
    validate_known_label_catalog_v1(&catalog)?;
    if source._accessibility_probe._queries != known_label_queries_v1(&catalog) {
        return Err("review artifact source did not use the exact fixed catalog".to_owned());
    }
    let source_summary = source.summary_v1();
    if source_summary.report_commitment_sha256 != pixel_summary_commitment_v1(&source_summary)? {
        return Err("review artifact source report commitment changed".to_owned());
    }
    let catalog_commitment_sha256 = known_label_catalog_commitment_v1(&catalog)?;
    let catalog_summary = build_known_label_pixel_catalog_summary_v1(
        &catalog,
        catalog_commitment_sha256.clone(),
        source_summary,
    )?;

    let frame = &source._before_frame;
    let after_frame = &source._after_frame;
    for (candidate, label) in [(frame, "before"), (after_frame, "after")] {
        if candidate.manifest.frame.canonical_bgra8_sha256
            != sha256_hex_v1(&candidate.canonical_bgra8)
            || candidate.manifest.frame.preview_png_sha256 != sha256_hex_v1(&candidate.preview_png)
        {
            return Err(format!(
                "review artifact {label} visible client bytes changed"
            ));
        }
    }
    let frame_size = MtgoSizePxV1 {
        width: frame.manifest.frame.canonical_width,
        height: frame.manifest.frame.canonical_height,
    };
    let review_template = review_template_for_catalog_summary_v1(&catalog_summary)?;
    let review_template_bytes = serde_json::to_vec_pretty(&review_template)
        .map_err(|error| format!("serialize accessibility review template: {error}"))?;
    let review_template_sha256 = sha256_hex_v1(&review_template_bytes);

    let mut crop_files = Vec::new();
    let mut entries = Vec::new();
    for (query_index, entry) in catalog_summary.entries.iter().enumerate() {
        let matches = source
            ._private_pixel_matches
            .iter()
            .filter(|matched| matched.query_index == query_index)
            .collect::<Vec<_>>();
        if matches.len() != usize::try_from(entry.exact_visible_match_count).unwrap_or(usize::MAX) {
            return Err("review artifact private match inventory changed".to_owned());
        }
        let mut match_crops = Vec::new();
        for (match_index, matched) in matches.into_iter().enumerate() {
            let region_hash = mtgo_blackbox_v1::visible_frame_region_content_sha256_v1(
                &frame.canonical_bgra8,
                &frame_size,
                &matched.rect_client_px,
            )
            .map_err(|error| format!("hash accessibility review crop: {error}"))?;
            if region_hash != matched.region_bgra8_sha256 {
                return Err("review artifact matched pixels changed".to_owned());
            }
            let crop_pixels = crop_visible_bgra8_v1(
                &frame.canonical_bgra8,
                &frame_size,
                &matched.rect_client_px,
            )?;
            let crop_png = encode_visible_accessibility_png_v1(
                &crop_pixels,
                matched.rect_client_px.width,
                matched.rect_client_px.height,
            )?;
            let file = format!("match-{query_index:02}-{match_index:02}.png");
            let png_sha256 = sha256_hex_v1(&crop_png);
            crop_files.push((file.clone(), crop_png));
            match_crops.push(PrivateVisibleAccessibilityCatalogReviewCropV1 {
                file,
                png_sha256,
                region_bgra8_sha256: matched.region_bgra8_sha256.clone(),
            });
        }
        entries.push(PrivateVisibleAccessibilityCatalogReviewArtifactEntryV1 {
            query_id: entry.query_id.clone(),
            slice: entry.slice,
            expected_visible_text_sha256: entry.expected_visible_text_sha256.clone(),
            exact_visible_match_count: entry.exact_visible_match_count,
            match_crops,
        });
    }
    let exact_visible_match_crop_count = entries.iter().try_fold(0_u32, |total, entry| {
        total
            .checked_add(
                u32::try_from(entry.match_crops.len())
                    .map_err(|_| "review artifact crop count overflow")?,
            )
            .ok_or("review artifact total crop count overflow")
    })?;
    if exact_visible_match_crop_count != catalog_summary.total_pixel_corroborated_match_count {
        return Err("review artifact crop total changed".to_owned());
    }
    let mut manifest = PrivateVisibleAccessibilityCatalogReviewArtifactManifestV1 {
        schema_version: MTGO_VISIBLE_ACCESSIBILITY_CATALOG_REVIEW_ARTIFACT_SCHEMA_V1,
        artifact_kind: "mtgo_visible_accessibility_catalog_review_artifact_v1".to_owned(),
        catalog_commitment_sha256,
        source_pixel_report_commitment_sha256: catalog_summary
            .source_pixel_report_commitment_sha256,
        before_frame_sha256: catalog_summary.before_frame_sha256,
        after_frame_sha256: catalog_summary.after_frame_sha256,
        before_visible_client_png_file: "before-visible-client.png".to_owned(),
        before_visible_client_png_sha256: sha256_hex_v1(&frame.preview_png),
        after_visible_client_png_file: "after-visible-client.png".to_owned(),
        after_visible_client_png_sha256: sha256_hex_v1(&after_frame.preview_png),
        review_template_file: "review-template.json".to_owned(),
        review_template_sha256,
        entries,
        exact_visible_match_crop_count,
        raw_or_unmatched_visible_text_exposed: false,
        pixel_coordinates_exposed: false,
        accessibility_metadata_exposed: false,
        process_or_window_metadata_exposed: false,
        human_review_complete: false,
        safe_for_semantic_evidence: false,
        safe_for_policy_scoring: false,
        safe_for_input: false,
        artifact_commitment_sha256: String::new(),
    };
    manifest.artifact_commitment_sha256 =
        visible_accessibility_catalog_review_artifact_commitment_v1(
            &manifest,
            &review_template_bytes,
            &frame.preview_png,
            &after_frame.preview_png,
            &crop_files,
        )?;
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("serialize accessibility review artifact: {error}"))?;
    Ok(PrivateVisibleAccessibilityCatalogReviewArtifactFilesV1 {
        manifest,
        manifest_bytes,
        review_template_bytes,
        before_visible_client_png: frame.preview_png.clone(),
        after_visible_client_png: after_frame.preview_png.clone(),
        crop_files,
    })
}

fn review_template_for_catalog_summary_v1(
    catalog_summary: &MtgoVisibleAccessibilityPixelCatalogProbeSummaryV1,
) -> Result<MtgoVisibleAccessibilityCatalogReviewV1, String> {
    if catalog_summary.report_commitment_sha256
        != known_label_pixel_catalog_report_commitment_v1(catalog_summary)?
    {
        return Err("accessibility review template source commitment changed".to_owned());
    }
    Ok(MtgoVisibleAccessibilityCatalogReviewV1 {
        schema_version: MTGO_VISIBLE_ACCESSIBILITY_CATALOG_REVIEW_SCHEMA_V1,
        catalog_commitment_sha256: catalog_summary.catalog_commitment_sha256.clone(),
        source_pixel_report_commitment_sha256: catalog_summary
            .source_pixel_report_commitment_sha256
            .clone(),
        before_frame_sha256: catalog_summary.before_frame_sha256.clone(),
        after_frame_sha256: catalog_summary.after_frame_sha256.clone(),
        reviewer_alias_sha256: String::new(),
        client_only_and_unobscured_confirmed: false,
        exact_frame_pair_identity_confirmed: false,
        entries: catalog_summary
            .entries
            .iter()
            .map(|entry| MtgoVisibleAccessibilityCatalogReviewEntryV1 {
                query_id: entry.query_id.clone(),
                slice: entry.slice,
                expected_visible_text_sha256: entry.expected_visible_text_sha256.clone(),
                reviewed_exact_visible_match_count: entry.exact_visible_match_count,
                reviewed_visible_pixel_match_set_commitment_sha256: entry
                    .visible_pixel_match_set_commitment_sha256
                    .clone(),
                every_matched_region_visibly_contains_exact_catalog_label: false,
                visible_absence_reviewed_when_match_count_is_zero: false,
            })
            .collect(),
        review_commitment_sha256: String::new(),
    })
}

fn visible_accessibility_catalog_review_artifact_commitment_v1(
    manifest: &PrivateVisibleAccessibilityCatalogReviewArtifactManifestV1,
    review_template_bytes: &[u8],
    before_visible_client_png: &[u8],
    after_visible_client_png: &[u8],
    crop_files: &[(String, Vec<u8>)],
) -> Result<String, String> {
    let mut unsigned = manifest.clone();
    unsigned.artifact_commitment_sha256.clear();
    let manifest_bytes = serde_json::to_vec(&unsigned)
        .map_err(|error| format!("serialize unsigned accessibility review artifact: {error}"))?;
    let mut hasher = Sha256::new();
    hasher.update(
        (VISIBLE_ACCESSIBILITY_CATALOG_REVIEW_ARTIFACT_DOMAIN_V1.len() as u64).to_be_bytes(),
    );
    hasher.update(VISIBLE_ACCESSIBILITY_CATALOG_REVIEW_ARTIFACT_DOMAIN_V1);
    for part in [
        manifest_bytes.as_slice(),
        review_template_bytes,
        before_visible_client_png,
        after_visible_client_png,
    ] {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    for (file, bytes) in crop_files {
        hasher.update((file.len() as u64).to_be_bytes());
        hasher.update(file.as_bytes());
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn crop_visible_bgra8_v1(
    pixels: &[u8],
    size: &MtgoSizePxV1,
    rect: &MtgoRectPxV1,
) -> Result<Vec<u8>, String> {
    mtgo_blackbox_v1::visible_frame_region_content_sha256_v1(pixels, size, rect)
        .map_err(|error| format!("validate accessibility review crop: {error}"))?;
    let source_row_bytes = usize::try_from(size.width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or("review crop source row overflow")?;
    let crop_row_bytes = usize::try_from(rect.width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or("review crop row overflow")?;
    let mut output = Vec::with_capacity(
        crop_row_bytes
            .checked_mul(usize::try_from(rect.height).map_err(|_| "review crop height overflow")?)
            .ok_or("review crop byte length overflow")?,
    );
    for row in rect.y..rect.y + rect.height {
        let start = usize::try_from(row)
            .ok()
            .and_then(|row| row.checked_mul(source_row_bytes))
            .and_then(|offset| {
                usize::try_from(rect.x)
                    .ok()
                    .and_then(|x| x.checked_mul(4))
                    .and_then(|x| offset.checked_add(x))
            })
            .ok_or("review crop offset overflow")?;
        let end = start
            .checked_add(crop_row_bytes)
            .ok_or("review crop end overflow")?;
        output.extend_from_slice(&pixels[start..end]);
    }
    Ok(output)
}

fn encode_visible_accessibility_png_v1(
    pixels: &[u8],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    let expected = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("accessibility review PNG geometry overflow")?;
    if pixels.len() != expected || width == 0 || height == 0 {
        return Err("accessibility review PNG bytes do not match geometry".to_owned());
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
            .map_err(|error| format!("write accessibility review PNG header: {error}"))?;
        writer
            .write_image_data(&rgba)
            .map_err(|error| format!("write accessibility review PNG pixels: {error}"))?;
    }
    Ok(encoded)
}

fn validate_visible_accessibility_review_output_v1(requested: &Path) -> Result<PathBuf, String> {
    if !requested.is_absolute() || requested.exists() {
        return Err("review artifact output must be a new absolute directory".to_owned());
    }
    let parent = requested
        .parent()
        .ok_or("review artifact output has no parent")?
        .canonicalize()
        .map_err(|error| format!("canonicalize review artifact parent: {error}"))?;
    let name = requested
        .file_name()
        .ok_or("review artifact output has no name")?;
    let resolved = parent.join(name);
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("could not derive repository root")?
        .canonicalize()
        .map_err(|error| format!("canonicalize repository root: {error}"))?;
    if resolved.starts_with(repository) {
        return Err("review artifact may not be written inside the repository".to_owned());
    }
    Ok(resolved)
}

fn persist_visible_accessibility_catalog_review_artifact_v1(
    output: &Path,
    files: &PrivateVisibleAccessibilityCatalogReviewArtifactFilesV1,
) -> Result<(), String> {
    let parent = output
        .parent()
        .ok_or("review artifact output has no parent")?;
    let partial = parent.join(format!(
        ".mtgo-visible-accessibility-review-partial-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before epoch: {error}"))?
            .as_nanos()
    ));
    if partial.exists() {
        return Err("review artifact partial path already exists".to_owned());
    }
    fs::create_dir(&partial)
        .map_err(|error| format!("create review artifact partial directory: {error}"))?;
    let result = (|| {
        fs::write(partial.join("manifest.json"), &files.manifest_bytes)
            .map_err(|error| format!("write review artifact manifest: {error}"))?;
        fs::write(
            partial.join("review-template.json"),
            &files.review_template_bytes,
        )
        .map_err(|error| format!("write review template: {error}"))?;
        fs::write(
            partial.join("before-visible-client.png"),
            &files.before_visible_client_png,
        )
        .map_err(|error| format!("write before visible client PNG: {error}"))?;
        fs::write(
            partial.join("after-visible-client.png"),
            &files.after_visible_client_png,
        )
        .map_err(|error| format!("write after visible client PNG: {error}"))?;
        for (file, bytes) in &files.crop_files {
            fs::write(partial.join(file), bytes)
                .map_err(|error| format!("write visible match crop: {error}"))?;
        }
        fs::rename(&partial, output)
            .map_err(|error| format!("commit review artifact directory: {error}"))?;
        Ok(())
    })();
    if result.is_err() && partial.exists() {
        let _ = fs::remove_dir_all(&partial);
    }
    result
}

fn load_visible_accessibility_catalog_review_artifact_v1(
    artifact_directory: &Path,
) -> Result<PrivateLoadedVisibleAccessibilityCatalogReviewArtifactV1, String> {
    if !artifact_directory.is_absolute()
        || fs::symlink_metadata(artifact_directory)
            .map_err(|error| format!("inspect accessibility review artifact directory: {error}"))?
            .file_type()
            .is_symlink()
    {
        return Err(
            "accessibility review artifact must be an absolute non-symlink directory".to_owned(),
        );
    }
    let directory = artifact_directory
        .canonicalize()
        .map_err(|error| format!("canonicalize accessibility review artifact: {error}"))?;
    if !directory.is_dir() {
        return Err("accessibility review artifact is not a directory".to_owned());
    }
    let manifest_bytes = read_bounded_regular_file_v1(
        &directory.join("manifest.json"),
        512 * 1_024,
        "accessibility review artifact manifest",
    )?;
    let manifest: PrivateVisibleAccessibilityCatalogReviewArtifactManifestV1 =
        serde_json::from_slice(&manifest_bytes)
            .map_err(|error| format!("parse accessibility review artifact manifest: {error}"))?;
    let canonical_manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("canonicalize accessibility review artifact manifest: {error}"))?;
    if manifest_bytes != canonical_manifest_bytes {
        return Err("accessibility review artifact manifest is not canonical".to_owned());
    }
    validate_visible_accessibility_review_artifact_manifest_v1(&manifest)?;

    let review_template_bytes = read_bounded_regular_file_v1(
        &directory.join(&manifest.review_template_file),
        256 * 1_024,
        "accessibility review template",
    )?;
    let before_visible_client_png = read_bounded_regular_file_v1(
        &directory.join(&manifest.before_visible_client_png_file),
        64 * 1_024 * 1_024,
        "before visible client PNG",
    )?;
    let after_visible_client_png = read_bounded_regular_file_v1(
        &directory.join(&manifest.after_visible_client_png_file),
        64 * 1_024 * 1_024,
        "after visible client PNG",
    )?;
    if sha256_hex_v1(&before_visible_client_png) != manifest.before_visible_client_png_sha256
        || sha256_hex_v1(&after_visible_client_png) != manifest.after_visible_client_png_sha256
    {
        return Err("visible client PNG hash changed".to_owned());
    }
    let (before_width, before_height, before_bgra8) =
        decode_visible_accessibility_png_to_bgra8_v1(&before_visible_client_png)?;
    let (after_width, after_height, after_bgra8) =
        decode_visible_accessibility_png_to_bgra8_v1(&after_visible_client_png)?;
    if (before_width, before_height) != (after_width, after_height)
        || sha256_hex_v1(&before_bgra8) != manifest.before_frame_sha256
        || sha256_hex_v1(&after_bgra8) != manifest.after_frame_sha256
    {
        return Err("visible client PNG pixels do not bind the declared frame pair".to_owned());
    }

    let mut crop_files = Vec::new();
    let mut total_crop_png_bytes = 0_usize;
    let mut query_results = Vec::new();
    let mut expected_files = HashSet::from([
        "manifest.json".to_owned(),
        manifest.review_template_file.clone(),
        manifest.before_visible_client_png_file.clone(),
        manifest.after_visible_client_png_file.clone(),
    ]);
    for (query_index, entry) in manifest.entries.iter().enumerate() {
        let mut region_hashes = Vec::new();
        for (match_index, crop) in entry.match_crops.iter().enumerate() {
            let expected_file = format!("match-{query_index:02}-{match_index:02}.png");
            if crop.file != expected_file || !expected_files.insert(crop.file.clone()) {
                return Err(
                    "accessibility review crop filename changed or is duplicated".to_owned(),
                );
            }
            let bytes = read_bounded_regular_file_v1(
                &directory.join(&crop.file),
                16 * 1_024 * 1_024,
                "visible accessibility match crop",
            )?;
            total_crop_png_bytes = total_crop_png_bytes
                .checked_add(bytes.len())
                .ok_or("visible accessibility match crop byte total overflow")?;
            if total_crop_png_bytes > MAX_VISIBLE_ACCESSIBILITY_REVIEW_TOTAL_CROP_PNG_BYTES_V1 {
                return Err(
                    "visible accessibility match crop files exceed the bounded byte total"
                        .to_owned(),
                );
            }
            let (crop_width, crop_height, bgra8) =
                decode_visible_accessibility_png_to_bgra8_v1(&bytes)?;
            if sha256_hex_v1(&bytes) != crop.png_sha256
                || !visible_crop_matches_region_commitment_at_same_position_v1(
                    &before_bgra8,
                    &after_bgra8,
                    before_width,
                    before_height,
                    &bgra8,
                    crop_width,
                    crop_height,
                    &crop.region_bgra8_sha256,
                )?
            {
                return Err(
                    "visible accessibility crop pixels changed or are not in the frame pair"
                        .to_owned(),
                );
            }
            region_hashes.push(crop.region_bgra8_sha256.as_str());
            crop_files.push((crop.file.clone(), bytes));
        }
        region_hashes.sort_unstable();
        let match_bytes = serde_json::to_vec(&region_hashes)
            .map_err(|error| format!("serialize loaded visible match set: {error}"))?;
        query_results.push(MtgoVisibleAccessibilityPixelQueryResultV1 {
            query_id: entry.query_id.clone(),
            expected_visible_text_sha256: entry.expected_visible_text_sha256.clone(),
            exact_visible_match_count: entry.exact_visible_match_count,
            visible_pixel_match_set_commitment_sha256: commitment_v1(
                VISIBLE_ACCESSIBILITY_PIXEL_MATCH_SET_DOMAIN_V1,
                &[entry.expected_visible_text_sha256.as_bytes(), &match_bytes],
            ),
        });
    }
    let actual_files = fs::read_dir(&directory)
        .map_err(|error| format!("enumerate accessibility review artifact: {error}"))?
        .map(|entry| {
            entry
                .map_err(|error| format!("read accessibility review artifact entry: {error}"))
                .and_then(|entry| {
                    entry.file_name().into_string().map_err(|_| {
                        "accessibility review artifact filename is not Unicode".to_owned()
                    })
                })
        })
        .collect::<Result<HashSet<_>, _>>()?;
    if actual_files != expected_files {
        return Err(
            "accessibility review artifact contains missing or unexpected files".to_owned(),
        );
    }

    let catalog_summary =
        reconstruct_loaded_visible_accessibility_catalog_summary_v1(&manifest, query_results)?;
    let expected_review_template = review_template_for_catalog_summary_v1(&catalog_summary)?;
    let expected_review_template_bytes = serde_json::to_vec_pretty(&expected_review_template)
        .map_err(|error| format!("serialize expected accessibility review template: {error}"))?;
    if review_template_bytes != expected_review_template_bytes
        || sha256_hex_v1(&review_template_bytes) != manifest.review_template_sha256
    {
        return Err("accessibility review template changed or is already approved".to_owned());
    }
    let files = PrivateVisibleAccessibilityCatalogReviewArtifactFilesV1 {
        manifest,
        manifest_bytes,
        review_template_bytes,
        before_visible_client_png,
        after_visible_client_png,
        crop_files,
    };
    if files.manifest.artifact_commitment_sha256
        != visible_accessibility_catalog_review_artifact_commitment_v1(
            &files.manifest,
            &files.review_template_bytes,
            &files.before_visible_client_png,
            &files.after_visible_client_png,
            &files.crop_files,
        )?
    {
        return Err("accessibility review artifact commitment changed".to_owned());
    }
    Ok(PrivateLoadedVisibleAccessibilityCatalogReviewArtifactV1 {
        _files: files,
        catalog_summary,
    })
}

fn validate_visible_accessibility_review_artifact_manifest_v1(
    manifest: &PrivateVisibleAccessibilityCatalogReviewArtifactManifestV1,
) -> Result<(), String> {
    let catalog = known_label_catalog_v1();
    if manifest.schema_version != MTGO_VISIBLE_ACCESSIBILITY_CATALOG_REVIEW_ARTIFACT_SCHEMA_V1
        || manifest.artifact_kind != "mtgo_visible_accessibility_catalog_review_artifact_v1"
        || manifest.catalog_commitment_sha256 != known_label_catalog_commitment_v1(&catalog)?
        || manifest.before_visible_client_png_file != "before-visible-client.png"
        || manifest.after_visible_client_png_file != "after-visible-client.png"
        || manifest.review_template_file != "review-template.json"
        || manifest.entries.len() != catalog.len()
        || manifest.raw_or_unmatched_visible_text_exposed
        || manifest.pixel_coordinates_exposed
        || manifest.accessibility_metadata_exposed
        || manifest.process_or_window_metadata_exposed
        || manifest.human_review_complete
        || manifest.safe_for_semantic_evidence
        || manifest.safe_for_policy_scoring
        || manifest.safe_for_input
    {
        return Err(
            "accessibility review artifact manifest is invalid or too authoritative".to_owned(),
        );
    }
    for digest in [
        &manifest.source_pixel_report_commitment_sha256,
        &manifest.before_frame_sha256,
        &manifest.after_frame_sha256,
        &manifest.before_visible_client_png_sha256,
        &manifest.after_visible_client_png_sha256,
        &manifest.review_template_sha256,
        &manifest.artifact_commitment_sha256,
    ] {
        validate_sha256_text_v1(digest)?;
    }
    let mut total = 0_u32;
    for (expected, entry) in catalog.iter().zip(&manifest.entries) {
        if entry.query_id != expected.query_id
            || entry.slice != expected.slice
            || entry.expected_visible_text_sha256
                != sha256_hex_v1(expected.expected_visible_text.as_bytes())
            || usize::try_from(entry.exact_visible_match_count).ok()
                != Some(entry.match_crops.len())
            || entry.match_crops.len() > MAX_MATCHES_PER_QUERY_V1
        {
            return Err("accessibility review artifact catalog entry changed".to_owned());
        }
        total = total
            .checked_add(entry.exact_visible_match_count)
            .ok_or("accessibility review artifact crop total overflow")?;
        for crop in &entry.match_crops {
            validate_sha256_text_v1(&crop.png_sha256)?;
            validate_sha256_text_v1(&crop.region_bgra8_sha256)?;
        }
    }
    if total != manifest.exact_visible_match_crop_count {
        return Err("accessibility review artifact crop total changed".to_owned());
    }
    Ok(())
}

fn reconstruct_loaded_visible_accessibility_catalog_summary_v1(
    manifest: &PrivateVisibleAccessibilityCatalogReviewArtifactManifestV1,
    query_results: Vec<MtgoVisibleAccessibilityPixelQueryResultV1>,
) -> Result<MtgoVisibleAccessibilityPixelCatalogProbeSummaryV1, String> {
    let mut plain = MtgoVisibleAccessibilityProbeSummaryV1 {
        schema_version: MTGO_VISIBLE_ACCESSIBILITY_PROBE_SCHEMA_V1,
        query_results: query_results
            .iter()
            .map(|entry| MtgoVisibleAccessibilityQueryResultV1 {
                query_id: entry.query_id.clone(),
                expected_visible_text_sha256: entry.expected_visible_text_sha256.clone(),
                exact_visible_match_count: entry.exact_visible_match_count,
            })
            .collect(),
        raw_visible_text_exposed: false,
        requires_same_frame_pixel_corroboration: true,
        safe_for_semantic_evidence: false,
        safe_for_policy_scoring: false,
        safe_for_input: false,
        report_commitment_sha256: String::new(),
    };
    plain.report_commitment_sha256 = summary_commitment_v1(&plain)?;
    let total = query_results.iter().try_fold(0_u32, |total, entry| {
        total
            .checked_add(entry.exact_visible_match_count)
            .ok_or("loaded accessibility match count overflow")
    })?;
    let mut pixel = MtgoVisibleAccessibilityPixelCorroborationSummaryV1 {
        schema_version: MTGO_VISIBLE_ACCESSIBILITY_PIXEL_CORROBORATION_SCHEMA_V1,
        before_frame_sha256: manifest.before_frame_sha256.clone(),
        after_frame_sha256: manifest.after_frame_sha256.clone(),
        accessibility_report_commitment_sha256: plain.report_commitment_sha256,
        query_results,
        total_pixel_corroborated_match_count: total,
        has_pixel_corroborated_match: total != 0,
        matched_regions_pixel_stable_across_bracket: true,
        raw_visible_text_exposed: false,
        private_match_rectangles_exposed: false,
        safe_for_semantic_evidence: false,
        safe_for_policy_scoring: false,
        safe_for_input: false,
        report_commitment_sha256: String::new(),
    };
    pixel.report_commitment_sha256 = pixel_summary_commitment_v1(&pixel)?;
    if pixel.report_commitment_sha256 != manifest.source_pixel_report_commitment_sha256 {
        return Err("loaded accessibility source report commitment changed".to_owned());
    }
    let catalog = known_label_catalog_v1();
    build_known_label_pixel_catalog_summary_v1(
        &catalog,
        manifest.catalog_commitment_sha256.clone(),
        pixel,
    )
}

fn read_bounded_regular_file_v1(
    path: &Path,
    maximum_bytes: u64,
    label: &str,
) -> Result<Vec<u8>, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("inspect {label}: {error}"))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > maximum_bytes
    {
        return Err(format!(
            "{label} must be one bounded non-symlink regular file"
        ));
    }
    let bytes = fs::read(path).map_err(|error| format!("read {label}: {error}"))?;
    if u64::try_from(bytes.len()).ok() != Some(metadata.len()) {
        return Err(format!("{label} changed while being read"));
    }
    Ok(bytes)
}

fn decode_visible_accessibility_png_to_bgra8_v1(
    bytes: &[u8],
) -> Result<(u32, u32, Vec<u8>), String> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder
        .read_info()
        .map_err(|error| format!("decode accessibility review PNG header: {error}"))?;
    let info = reader.info();
    if info.width == 0
        || info.height == 0
        || info.width > MAX_VISIBLE_ACCESSIBILITY_REVIEW_IMAGE_DIMENSION_V1
        || info.height > MAX_VISIBLE_ACCESSIBILITY_REVIEW_IMAGE_DIMENSION_V1
        || info.interlaced
        || info.animation_control.is_some()
        || info.bit_depth != png::BitDepth::Eight
        || info.color_type != png::ColorType::Rgba
    {
        return Err(
            "accessibility review PNG must be one bounded non-interlaced opaque RGBA8 image"
                .to_owned(),
        );
    }
    let width = info.width;
    let height = info.height;
    let source_length = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("accessibility review PNG geometry overflow")?;
    if source_length > MAX_VISIBLE_ACCESSIBILITY_REVIEW_DECODED_IMAGE_BYTES_V1 {
        return Err("accessibility review PNG decoded byte length exceeds its bound".to_owned());
    }
    let mut rgba = vec![0_u8; source_length];
    let frame = reader
        .next_frame(&mut rgba)
        .map_err(|error| format!("decode accessibility review PNG pixels: {error}"))?;
    if frame.width != width
        || frame.height != height
        || frame.color_type != png::ColorType::Rgba
        || frame.bit_depth != png::BitDepth::Eight
        || frame.buffer_size() != source_length
    {
        return Err("decoded accessibility review PNG differs from its header".to_owned());
    }
    let mut bgra = Vec::with_capacity(source_length);
    for pixel in rgba.chunks_exact(4) {
        if pixel[3] != 255 {
            return Err("accessibility review PNG must be fully opaque".to_owned());
        }
        bgra.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
    }
    Ok((width, height, bgra))
}

#[allow(clippy::too_many_arguments)]
fn visible_crop_exists_at_same_position_v1(
    before: &[u8],
    after: &[u8],
    frame_width: u32,
    frame_height: u32,
    crop: &[u8],
    crop_width: u32,
    crop_height: u32,
) -> Result<bool, String> {
    visible_crop_matches_at_same_position_v1(
        before,
        after,
        frame_width,
        frame_height,
        crop,
        crop_width,
        crop_height,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn visible_crop_matches_region_commitment_at_same_position_v1(
    before: &[u8],
    after: &[u8],
    frame_width: u32,
    frame_height: u32,
    crop: &[u8],
    crop_width: u32,
    crop_height: u32,
    expected_region_commitment_sha256: &str,
) -> Result<bool, String> {
    visible_crop_matches_at_same_position_v1(
        before,
        after,
        frame_width,
        frame_height,
        crop,
        crop_width,
        crop_height,
        Some(expected_region_commitment_sha256),
    )
}

#[allow(clippy::too_many_arguments)]
fn visible_crop_matches_at_same_position_v1(
    before: &[u8],
    after: &[u8],
    frame_width: u32,
    frame_height: u32,
    crop: &[u8],
    crop_width: u32,
    crop_height: u32,
    expected_region_commitment_sha256: Option<&str>,
) -> Result<bool, String> {
    if crop_width == 0 || crop_height == 0 || crop_width > frame_width || crop_height > frame_height
    {
        return Ok(false);
    }
    let frame_row_bytes = usize::try_from(frame_width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or("visible crop frame row overflow")?;
    let crop_row_bytes = usize::try_from(crop_width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or("visible crop row overflow")?;
    let frame_length = frame_row_bytes
        .checked_mul(usize::try_from(frame_height).map_err(|_| "visible crop frame overflow")?)
        .ok_or("visible crop frame length overflow")?;
    let crop_length = crop_row_bytes
        .checked_mul(usize::try_from(crop_height).map_err(|_| "visible crop height overflow")?)
        .ok_or("visible crop length overflow")?;
    if before.len() != frame_length || after.len() != frame_length || crop.len() != crop_length {
        return Err("visible crop bytes do not match their decoded geometry".to_owned());
    }
    for y in 0..=frame_height - crop_height {
        for x in 0..=frame_width - crop_width {
            let matches = (0..crop_height).all(|row| {
                let frame_start = usize::try_from(y + row).unwrap() * frame_row_bytes
                    + usize::try_from(x).unwrap() * 4;
                let crop_start = usize::try_from(row).unwrap() * crop_row_bytes;
                let frame_end = frame_start + crop_row_bytes;
                let crop_end = crop_start + crop_row_bytes;
                before[frame_start..frame_end] == crop[crop_start..crop_end]
                    && after[frame_start..frame_end] == crop[crop_start..crop_end]
            });
            if matches {
                if let Some(expected) = expected_region_commitment_sha256 {
                    let observed = mtgo_blackbox_v1::visible_frame_region_content_sha256_v1(
                        before,
                        &MtgoSizePxV1 {
                            width: frame_width,
                            height: frame_height,
                        },
                        &MtgoRectPxV1 {
                            x,
                            y,
                            width: crop_width,
                            height: crop_height,
                        },
                    )
                    .map_err(|error| {
                        format!("recompute persisted accessibility crop commitment: {error}")
                    })?;
                    if observed == expected {
                        return Ok(true);
                    }
                } else {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

pub fn run_visible_accessibility_known_label_catalog_pixel_corroboration_cli_v1(
) -> Result<MtgoVisibleAccessibilityPixelCatalogProbeSummaryV1, String> {
    let window_request = parse_catalog_cli_v1()?;
    probe_mtgo_visible_accessibility_known_label_catalog_with_pixel_corroboration_v1(window_request)
}

pub fn run_visible_accessibility_known_label_catalog_review_artifact_cli_v1(
) -> Result<MtgoVisibleAccessibilityCatalogReviewArtifactReceiptV1, String> {
    let (window_request, output) = parse_catalog_review_artifact_cli_v1()?;
    let source =
        probe_mtgo_visible_accessibility_known_label_catalog_evaluation_source_v1(window_request)?;
    persist_mtgo_visible_accessibility_catalog_review_artifact_v1(source, &output)
}

fn known_label_catalog_v1() -> Vec<PrivateVisibleAccessibilityCatalogEntryV1> {
    vec![
        PrivateVisibleAccessibilityCatalogEntryV1 {
            query_id: "bottoming.cancel",
            slice: MtgoVisibleAccessibilityCatalogSliceV1::Bottoming,
            expected_visible_text: "Cancel",
        },
        PrivateVisibleAccessibilityCatalogEntryV1 {
            query_id: "gameplay.combat",
            slice: MtgoVisibleAccessibilityCatalogSliceV1::Gameplay,
            expected_visible_text: "Combat",
        },
        PrivateVisibleAccessibilityCatalogEntryV1 {
            query_id: "pregame.keep",
            slice: MtgoVisibleAccessibilityCatalogSliceV1::Pregame,
            expected_visible_text: "Keep",
        },
        PrivateVisibleAccessibilityCatalogEntryV1 {
            query_id: "pregame.mulligan",
            slice: MtgoVisibleAccessibilityCatalogSliceV1::Pregame,
            expected_visible_text: "Mulligan",
        },
        PrivateVisibleAccessibilityCatalogEntryV1 {
            query_id: "sideboard.submit_deck",
            slice: MtgoVisibleAccessibilityCatalogSliceV1::Sideboard,
            expected_visible_text: "Submit Deck",
        },
    ]
}

fn validate_known_label_catalog_v1(
    catalog: &[PrivateVisibleAccessibilityCatalogEntryV1],
) -> Result<(), String> {
    validate_queries_v1(&known_label_queries_v1(catalog))
}

fn known_label_queries_v1(
    catalog: &[PrivateVisibleAccessibilityCatalogEntryV1],
) -> Vec<MtgoVisibleAccessibilityExactTextQueryV1> {
    catalog
        .iter()
        .map(|entry| MtgoVisibleAccessibilityExactTextQueryV1 {
            query_id: entry.query_id.to_owned(),
            expected_visible_text: entry.expected_visible_text.to_owned(),
        })
        .collect()
}

fn known_label_catalog_commitment_v1(
    catalog: &[PrivateVisibleAccessibilityCatalogEntryV1],
) -> Result<String, String> {
    let bytes = serde_json::to_vec(catalog)
        .map_err(|error| format!("serialize visible accessibility catalog: {error}"))?;
    Ok(commitment_v1(
        VISIBLE_ACCESSIBILITY_CATALOG_DOMAIN_V1,
        &[&bytes],
    ))
}

#[allow(clippy::too_many_arguments)]
fn known_label_catalog_report_commitment_v1(
    catalog_commitment_sha256: &str,
    source_probe_commitment_sha256: &str,
    catalog_entry_count: u32,
    matched_catalog_entry_count: u32,
    total_exact_visible_match_count: u32,
    entries: &[MtgoVisibleAccessibilityCatalogEntrySummaryV1],
) -> Result<String, String> {
    let counts = [
        catalog_entry_count,
        matched_catalog_entry_count,
        total_exact_visible_match_count,
    ];
    let count_bytes = counts
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    let entry_bytes = serde_json::to_vec(entries)
        .map_err(|error| format!("serialize visible accessibility catalog report: {error}"))?;
    Ok(commitment_v1(
        VISIBLE_ACCESSIBILITY_CATALOG_REPORT_DOMAIN_V1,
        &[
            &MTGO_VISIBLE_ACCESSIBILITY_CATALOG_SCHEMA_V1.to_le_bytes(),
            catalog_commitment_sha256.as_bytes(),
            source_probe_commitment_sha256.as_bytes(),
            &count_bytes,
            &entry_bytes,
            b"raw_visible_text_exposed=false",
            b"caller_selected_text_queries_enabled=false",
            b"unmatched_visible_text_retained=false",
            b"requires_same_frame_pixel_corroboration=true",
            b"safe_for_semantic_evidence=false",
            b"safe_for_policy_scoring=false",
            b"safe_for_input=false",
        ],
    ))
}

fn build_known_label_pixel_catalog_summary_v1(
    catalog: &[PrivateVisibleAccessibilityCatalogEntryV1],
    catalog_commitment_sha256: String,
    source: MtgoVisibleAccessibilityPixelCorroborationSummaryV1,
) -> Result<MtgoVisibleAccessibilityPixelCatalogProbeSummaryV1, String> {
    if source.report_commitment_sha256 != pixel_summary_commitment_v1(&source)?
        || source.query_results.len() != catalog.len()
        || !source.matched_regions_pixel_stable_across_bracket
        || source.raw_visible_text_exposed
        || source.private_match_rectangles_exposed
        || source.safe_for_semantic_evidence
        || source.safe_for_policy_scoring
        || source.safe_for_input
    {
        return Err(
            "pixel-corroborated accessibility source is invalid or too authoritative".into(),
        );
    }
    let entries = catalog
        .iter()
        .zip(&source.query_results)
        .map(|(entry, result)| {
            if entry.query_id != result.query_id
                || sha256_hex_v1(entry.expected_visible_text.as_bytes())
                    != result.expected_visible_text_sha256
            {
                return Err("pixel-corroborated catalog query identity changed".to_owned());
            }
            Ok(MtgoVisibleAccessibilityPixelCatalogEntrySummaryV1 {
                query_id: entry.query_id.to_owned(),
                slice: entry.slice,
                expected_visible_text_sha256: result.expected_visible_text_sha256.clone(),
                exact_visible_match_count: result.exact_visible_match_count,
                visible_pixel_match_set_commitment_sha256: result
                    .visible_pixel_match_set_commitment_sha256
                    .clone(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let total = entries.iter().try_fold(0_u32, |count, entry| {
        count
            .checked_add(entry.exact_visible_match_count)
            .ok_or("pixel-corroborated catalog match count overflow")
    })?;
    if total != source.total_pixel_corroborated_match_count
        || source.has_pixel_corroborated_match != (total != 0)
    {
        return Err("pixel-corroborated catalog match totals changed".to_owned());
    }
    let mut summary = MtgoVisibleAccessibilityPixelCatalogProbeSummaryV1 {
        schema_version: MTGO_VISIBLE_ACCESSIBILITY_PIXEL_CATALOG_SCHEMA_V1,
        catalog_commitment_sha256,
        source_pixel_report_commitment_sha256: source.report_commitment_sha256,
        before_frame_sha256: source.before_frame_sha256,
        after_frame_sha256: source.after_frame_sha256,
        entries,
        total_pixel_corroborated_match_count: total,
        has_pixel_corroborated_match: total != 0,
        matched_regions_pixel_stable_across_bracket: true,
        raw_visible_text_exposed: false,
        caller_selected_text_queries_enabled: false,
        unmatched_visible_text_retained: false,
        private_match_rectangles_exposed: false,
        safe_for_semantic_evidence: false,
        safe_for_policy_scoring: false,
        safe_for_input: false,
        report_commitment_sha256: String::new(),
    };
    summary.report_commitment_sha256 = known_label_pixel_catalog_report_commitment_v1(&summary)?;
    Ok(summary)
}

fn known_label_pixel_catalog_report_commitment_v1(
    summary: &MtgoVisibleAccessibilityPixelCatalogProbeSummaryV1,
) -> Result<String, String> {
    let mut record = summary.clone();
    record.report_commitment_sha256.clear();
    let bytes = serde_json::to_vec(&record)
        .map_err(|error| format!("serialize visible accessibility pixel catalog: {error}"))?;
    Ok(commitment_v1(
        VISIBLE_ACCESSIBILITY_PIXEL_CATALOG_REPORT_DOMAIN_V1,
        &[&bytes],
    ))
}

pub fn mtgo_visible_accessibility_catalog_review_commitment_v1(
    review: &MtgoVisibleAccessibilityCatalogReviewV1,
) -> Result<String, String> {
    let mut record = review.clone();
    record.review_commitment_sha256.clear();
    let bytes = serde_json::to_vec(&record)
        .map_err(|error| format!("serialize visible accessibility catalog review: {error}"))?;
    Ok(commitment_v1(
        VISIBLE_ACCESSIBILITY_CATALOG_REVIEW_DOMAIN_V1,
        &[&bytes],
    ))
}

pub fn evaluate_untrusted_visible_accessibility_catalog_case_v1(
    source: OpaqueMtgoVisibleAccessibilityPixelCorroborationV1,
    review: MtgoVisibleAccessibilityCatalogReviewV1,
) -> Result<CheckedUntrustedMtgoVisibleAccessibilityCatalogCaseEvaluationV1, String> {
    let catalog = known_label_catalog_v1();
    validate_known_label_catalog_v1(&catalog)?;
    if source._accessibility_probe._queries != known_label_queries_v1(&catalog) {
        return Err(
            "accessibility evaluation source did not use the exact fixed catalog".to_owned(),
        );
    }
    let source_summary = source.summary_v1();
    if source_summary.report_commitment_sha256 != pixel_summary_commitment_v1(&source_summary)? {
        return Err("accessibility evaluation source report commitment changed".to_owned());
    }
    let catalog_commitment = known_label_catalog_commitment_v1(&catalog)?;
    let catalog_summary = build_known_label_pixel_catalog_summary_v1(
        &catalog,
        catalog_commitment.clone(),
        source_summary,
    )?;
    let summary =
        evaluate_visible_accessibility_catalog_case_summary_v1(&catalog_summary, &review)?;

    Ok(
        CheckedUntrustedMtgoVisibleAccessibilityCatalogCaseEvaluationV1 {
            _source: PrivateVisibleAccessibilityCatalogCaseSourceV1::Live {
                _source: Box::new(source),
            },
            _catalog_summary: catalog_summary,
            _review: review,
            summary,
        },
    )
}

pub fn load_checked_untrusted_visible_accessibility_catalog_case_from_review_artifact_v1(
    artifact_directory: &Path,
    completed_review_path: &Path,
) -> Result<CheckedUntrustedMtgoVisibleAccessibilityCatalogCaseEvaluationV1, String> {
    if !completed_review_path.is_absolute() {
        return Err("completed accessibility catalog review path must be absolute".to_owned());
    }
    let loaded = load_visible_accessibility_catalog_review_artifact_v1(artifact_directory)?;
    let completed_review_bytes = read_bounded_regular_file_v1(
        completed_review_path,
        256 * 1_024,
        "completed accessibility catalog review",
    )?;
    let review: MtgoVisibleAccessibilityCatalogReviewV1 =
        serde_json::from_slice(&completed_review_bytes)
            .map_err(|error| format!("parse completed accessibility catalog review: {error}"))?;
    validate_visible_accessibility_catalog_review_v1(&loaded.catalog_summary, &review)?;
    let summary =
        evaluate_visible_accessibility_catalog_case_summary_v1(&loaded.catalog_summary, &review)?;
    Ok(
        CheckedUntrustedMtgoVisibleAccessibilityCatalogCaseEvaluationV1 {
            _catalog_summary: loaded.catalog_summary.clone(),
            _source: PrivateVisibleAccessibilityCatalogCaseSourceV1::ReviewedArtifact {
                _artifact: Box::new(loaded),
            },
            _review: review,
            summary,
        },
    )
}

pub fn finalize_visible_accessibility_catalog_review_artifact_v1(
    artifact_directory: &Path,
    edited_review_path: &Path,
    reviewer_alias_path: &Path,
    output_path: &Path,
) -> Result<MtgoVisibleAccessibilityCatalogCompletedReviewReceiptV1, String> {
    for (path, label) in [
        (edited_review_path, "edited accessibility catalog review"),
        (reviewer_alias_path, "reviewer alias"),
        (output_path, "completed accessibility catalog review output"),
    ] {
        if !path.is_absolute() {
            return Err(format!("{label} path must be absolute"));
        }
    }
    if output_path.exists() {
        return Err("completed accessibility catalog review output must be new".to_owned());
    }

    let loaded = load_visible_accessibility_catalog_review_artifact_v1(artifact_directory)?;
    let artifact_directory = artifact_directory
        .canonicalize()
        .map_err(|error| format!("canonicalize accessibility review artifact: {error}"))?;
    let output_parent = output_path
        .parent()
        .ok_or("completed accessibility catalog review output has no parent")?
        .canonicalize()
        .map_err(|error| format!("canonicalize completed review output parent: {error}"))?;
    if output_parent.starts_with(&artifact_directory) {
        return Err(
            "completed accessibility catalog review may not modify the source artifact directory"
                .to_owned(),
        );
    }
    let edited_review_bytes = read_bounded_regular_file_v1(
        edited_review_path,
        256 * 1_024,
        "edited accessibility catalog review",
    )?;
    let mut review: MtgoVisibleAccessibilityCatalogReviewV1 =
        serde_json::from_slice(&edited_review_bytes)
            .map_err(|error| format!("parse edited accessibility catalog review: {error}"))?;
    let reviewer_alias_bytes =
        read_bounded_regular_file_v1(reviewer_alias_path, 256, "reviewer alias")?;
    let reviewer_alias_with_optional_line_ending =
        std::str::from_utf8(&reviewer_alias_bytes).map_err(|_| "reviewer alias must be UTF-8")?;
    let reviewer_alias = reviewer_alias_with_optional_line_ending
        .strip_suffix("\r\n")
        .or_else(|| reviewer_alias_with_optional_line_ending.strip_suffix('\n'))
        .unwrap_or(reviewer_alias_with_optional_line_ending);
    if reviewer_alias.trim() != reviewer_alias
        || reviewer_alias.is_empty()
        || reviewer_alias.chars().any(char::is_control)
    {
        return Err(
            "reviewer alias must be nonempty bounded printable UTF-8 without surrounding whitespace"
                .to_owned(),
        );
    }
    review.reviewer_alias_sha256 = sha256_hex_v1(reviewer_alias.as_bytes());
    review.review_commitment_sha256.clear();
    review.review_commitment_sha256 =
        mtgo_visible_accessibility_catalog_review_commitment_v1(&review)?;
    validate_visible_accessibility_catalog_review_v1(&loaded.catalog_summary, &review)?;
    let canonical = serde_json::to_vec_pretty(&review)
        .map_err(|error| format!("serialize completed accessibility catalog review: {error}"))?;
    write_new_regular_file_v1(
        output_path,
        &canonical,
        "completed accessibility catalog review",
    )?;
    Ok(MtgoVisibleAccessibilityCatalogCompletedReviewReceiptV1 {
        schema_version: MTGO_VISIBLE_ACCESSIBILITY_CATALOG_REVIEW_SCHEMA_V1,
        catalog_commitment_sha256: review.catalog_commitment_sha256,
        source_pixel_report_commitment_sha256: review.source_pixel_report_commitment_sha256,
        review_commitment_sha256: review.review_commitment_sha256,
        reviewed_entry_count: u32::try_from(review.entries.len())
            .map_err(|_| "completed accessibility review entry count overflow")?,
        completed_review_written: true,
        production_evaluation_ratified: false,
        safe_for_semantic_evidence: false,
        safe_for_policy_scoring: false,
        safe_for_input: false,
    })
}

fn write_new_regular_file_v1(path: &Path, bytes: &[u8], label: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{label} has no parent"))?;
    let metadata =
        fs::symlink_metadata(parent).map_err(|error| format!("inspect {label} parent: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!("{label} parent must be a non-symlink directory"));
    }
    let partial = parent.join(format!(
        ".mtgo-visible-accessibility-completed-review-partial-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before epoch: {error}"))?
            .as_nanos()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial)
        .map_err(|error| format!("create {label}: {error}"))?;
    let result = (|| -> Result<(), String> {
        file.write_all(bytes)
            .map_err(|error| format!("write {label}: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("sync {label}: {error}"))?;
        drop(file);
        fs::rename(&partial, path).map_err(|error| format!("commit {label}: {error}"))
    })();
    if result.is_err() && partial.exists() {
        let _ = fs::remove_file(&partial);
    }
    result
}

pub fn run_visible_accessibility_catalog_review_finalization_cli_v1(
) -> Result<MtgoVisibleAccessibilityCatalogCompletedReviewReceiptV1, String> {
    let (artifact, edited_review, reviewer_alias, output) =
        parse_catalog_review_finalization_arguments_v1(std::env::args_os().skip(1))?;
    finalize_visible_accessibility_catalog_review_artifact_v1(
        &artifact,
        &edited_review,
        &reviewer_alias,
        &output,
    )
}

pub fn evaluate_untrusted_visible_accessibility_catalog_corpus_v1(
    cases: Vec<CheckedUntrustedMtgoVisibleAccessibilityCatalogCaseEvaluationV1>,
) -> Result<CheckedUntrustedMtgoVisibleAccessibilityCatalogCorpusEvaluationV1, String> {
    if cases.is_empty() || cases.len() > MAX_VISIBLE_ACCESSIBILITY_CORPUS_CASES_V1 {
        return Err("accessibility catalog corpus must contain between 1 and 64 cases".to_owned());
    }
    let private_cases = cases
        .iter()
        .map(private_catalog_corpus_case_v1)
        .collect::<Result<Vec<_>, _>>()?;
    let summary = evaluate_visible_accessibility_catalog_corpus_summary_v1(&private_cases)?;
    Ok(
        CheckedUntrustedMtgoVisibleAccessibilityCatalogCorpusEvaluationV1 {
            _cases: cases,
            summary,
        },
    )
}

pub fn run_visible_accessibility_catalog_corpus_evaluation_cli_v1(
) -> Result<MtgoVisibleAccessibilityCatalogCorpusEvaluationSummaryV1, String> {
    let sources = parse_catalog_corpus_arguments_v1(std::env::args_os().skip(1))?;
    let cases = sources
        .into_iter()
        .map(|(artifact_directory, completed_review_path)| {
            load_checked_untrusted_visible_accessibility_catalog_case_from_review_artifact_v1(
                &artifact_directory,
                &completed_review_path,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(evaluate_untrusted_visible_accessibility_catalog_corpus_v1(cases)?.summary_v1())
}

fn private_catalog_corpus_case_v1(
    case: &CheckedUntrustedMtgoVisibleAccessibilityCatalogCaseEvaluationV1,
) -> Result<PrivateVisibleAccessibilityCatalogCorpusCaseV1, String> {
    let catalog_summary = &case._catalog_summary;
    let expected =
        evaluate_visible_accessibility_catalog_case_summary_v1(catalog_summary, &case._review)?;
    if expected != case.summary {
        return Err("accessibility catalog corpus case summary changed".to_owned());
    }
    Ok(PrivateVisibleAccessibilityCatalogCorpusCaseV1 {
        catalog_commitment_sha256: expected.catalog_commitment_sha256,
        before_frame_sha256: catalog_summary.before_frame_sha256.clone(),
        after_frame_sha256: catalog_summary.after_frame_sha256.clone(),
        case_candidate_commitment_sha256: expected.ratification_candidate_commitment_sha256,
        entries: catalog_summary.entries.clone(),
    })
}

fn evaluate_visible_accessibility_catalog_corpus_summary_v1(
    cases: &[PrivateVisibleAccessibilityCatalogCorpusCaseV1],
) -> Result<MtgoVisibleAccessibilityCatalogCorpusEvaluationSummaryV1, String> {
    if cases.is_empty() || cases.len() > MAX_VISIBLE_ACCESSIBILITY_CORPUS_CASES_V1 {
        return Err("accessibility catalog corpus must contain between 1 and 64 cases".to_owned());
    }
    let catalog = known_label_catalog_v1();
    let catalog_commitment_sha256 = known_label_catalog_commitment_v1(&catalog)?;
    let mut frame_pairs = HashSet::new();
    for case in cases {
        if case.catalog_commitment_sha256 != catalog_commitment_sha256
            || case.entries.len() != catalog.len()
            || !frame_pairs.insert((
                case.before_frame_sha256.as_str(),
                case.after_frame_sha256.as_str(),
            ))
        {
            return Err(
                "catalog corpus requires one exact catalog and distinct visible frame pairs"
                    .to_owned(),
            );
        }
    }
    let entries = catalog
        .iter()
        .enumerate()
        .map(|(index, expected)| {
            let observed = cases
                .iter()
                .map(|case| &case.entries[index])
                .collect::<Vec<_>>();
            if observed.iter().any(|entry| {
                entry.query_id != expected.query_id
                    || entry.slice != expected.slice
                    || entry.expected_visible_text_sha256
                        != sha256_hex_v1(expected.expected_visible_text.as_bytes())
            }) {
                return Err("catalog corpus entry identity changed".to_owned());
            }
            let reviewed_positive_case_count = u32::try_from(
                observed
                    .iter()
                    .filter(|entry| entry.exact_visible_match_count != 0)
                    .count(),
            )
            .map_err(|_| "catalog corpus positive case count overflow")?;
            let reviewed_zero_case_count = u32::try_from(
                observed
                    .iter()
                    .filter(|entry| entry.exact_visible_match_count == 0)
                    .count(),
            )
            .map_err(|_| "catalog corpus zero case count overflow")?;
            let total_reviewed_exact_visible_match_count =
                observed.iter().try_fold(0_u32, |total, entry| {
                    total
                        .checked_add(entry.exact_visible_match_count)
                        .ok_or("catalog corpus match count overflow")
                })?;
            Ok(MtgoVisibleAccessibilityCatalogCorpusEntrySummaryV1 {
                query_id: expected.query_id.to_owned(),
                slice: expected.slice,
                expected_visible_text_sha256: sha256_hex_v1(
                    expected.expected_visible_text.as_bytes(),
                ),
                reviewed_positive_case_count,
                reviewed_zero_case_count,
                total_reviewed_exact_visible_match_count,
                presence_and_absence_coverage_complete: reviewed_positive_case_count != 0
                    && reviewed_zero_case_count != 0,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if entries
        .iter()
        .any(|entry| !entry.presence_and_absence_coverage_complete)
    {
        return Err(
            "catalog corpus requires reviewed presence and absence for every fixed label"
                .to_owned(),
        );
    }
    let mut case_commitments = cases
        .iter()
        .map(|case| case.case_candidate_commitment_sha256.as_str())
        .collect::<Vec<_>>();
    case_commitments.sort_unstable();
    let case_bytes = serde_json::to_vec(&case_commitments)
        .map_err(|error| format!("serialize catalog corpus case commitments: {error}"))?;
    let entry_bytes = serde_json::to_vec(&entries)
        .map_err(|error| format!("serialize catalog corpus entries: {error}"))?;
    let reviewed_case_count =
        u32::try_from(cases.len()).map_err(|_| "catalog corpus case count overflow")?;
    let corpus_candidate_commitment_sha256 = commitment_v1(
        VISIBLE_ACCESSIBILITY_CATALOG_CORPUS_EVALUATION_DOMAIN_V1,
        &[
            &MTGO_VISIBLE_ACCESSIBILITY_CATALOG_CORPUS_EVALUATION_SCHEMA_V1.to_le_bytes(),
            catalog_commitment_sha256.as_bytes(),
            &reviewed_case_count.to_le_bytes(),
            &case_bytes,
            &entry_bytes,
            b"every_catalog_entry_has_reviewed_presence_and_absence=true",
            b"exact_frame_pairs_are_distinct_across_cases=true",
            b"production_evaluation_ratified=false",
            b"safe_for_semantic_evidence=false",
            b"safe_for_policy_scoring=false",
            b"safe_for_input=false",
        ],
    );
    Ok(MtgoVisibleAccessibilityCatalogCorpusEvaluationSummaryV1 {
        schema_version: MTGO_VISIBLE_ACCESSIBILITY_CATALOG_CORPUS_EVALUATION_SCHEMA_V1,
        catalog_commitment_sha256,
        reviewed_case_count,
        distinct_visible_frame_pair_count: reviewed_case_count,
        entries,
        every_catalog_entry_has_reviewed_presence_and_absence: true,
        exact_frame_pairs_are_distinct_across_cases: true,
        corpus_candidate_commitment_sha256,
        production_evaluation_ratified: false,
        safe_for_semantic_evidence: false,
        safe_for_policy_scoring: false,
        safe_for_input: false,
    })
}

fn evaluate_visible_accessibility_catalog_case_summary_v1(
    catalog_summary: &MtgoVisibleAccessibilityPixelCatalogProbeSummaryV1,
    review: &MtgoVisibleAccessibilityCatalogReviewV1,
) -> Result<MtgoVisibleAccessibilityCatalogCaseEvaluationSummaryV1, String> {
    validate_visible_accessibility_catalog_review_v1(catalog_summary, review)?;

    let positive_entry_count = u32::try_from(
        catalog_summary
            .entries
            .iter()
            .filter(|entry| entry.exact_visible_match_count != 0)
            .count(),
    )
    .map_err(|_| "accessibility evaluation positive entry count overflow")?;
    let covered_slices = covered_catalog_slices_v1(&catalog_summary.entries);
    let ratification_candidate_commitment_sha256 = commitment_v1(
        VISIBLE_ACCESSIBILITY_CATALOG_CASE_EVALUATION_DOMAIN_V1,
        &[
            catalog_summary.report_commitment_sha256.as_bytes(),
            review.review_commitment_sha256.as_bytes(),
            b"exact_review_agreement=true",
            b"safe_for_semantic_evidence=false",
            b"safe_for_policy_scoring=false",
            b"safe_for_input=false",
        ],
    );
    Ok(MtgoVisibleAccessibilityCatalogCaseEvaluationSummaryV1 {
        schema_version: MTGO_VISIBLE_ACCESSIBILITY_CATALOG_CASE_EVALUATION_SCHEMA_V1,
        catalog_commitment_sha256: catalog_summary.catalog_commitment_sha256.clone(),
        source_pixel_report_commitment_sha256: catalog_summary
            .source_pixel_report_commitment_sha256
            .clone(),
        review_commitment_sha256: review.review_commitment_sha256.clone(),
        evaluated_entry_count: u32::try_from(catalog_summary.entries.len())
            .map_err(|_| "accessibility evaluation entry count overflow")?,
        positive_entry_count,
        total_exact_visible_match_count: catalog_summary.total_pixel_corroborated_match_count,
        covered_slices,
        exact_review_agreement: true,
        ratification_candidate_commitment_sha256,
        // A single reviewed case is deliberately incapable of ratifying the
        // catalog. The corpus contract has no production ratification root.
        production_evaluation_ratified: false,
        safe_for_semantic_evidence: false,
        safe_for_policy_scoring: false,
        safe_for_input: false,
    })
}

fn validate_visible_accessibility_catalog_review_v1(
    source: &MtgoVisibleAccessibilityPixelCatalogProbeSummaryV1,
    review: &MtgoVisibleAccessibilityCatalogReviewV1,
) -> Result<(), String> {
    if review.schema_version != MTGO_VISIBLE_ACCESSIBILITY_CATALOG_REVIEW_SCHEMA_V1
        || review.catalog_commitment_sha256 != source.catalog_commitment_sha256
        || review.source_pixel_report_commitment_sha256
            != source.source_pixel_report_commitment_sha256
        || review.before_frame_sha256 != source.before_frame_sha256
        || review.after_frame_sha256 != source.after_frame_sha256
        || !review.client_only_and_unobscured_confirmed
        || !review.exact_frame_pair_identity_confirmed
        || review.entries.len() != source.entries.len()
    {
        return Err("accessibility catalog review does not bind the exact source case".to_owned());
    }
    validate_sha256_text_v1(&review.reviewer_alias_sha256)?;
    for (expected, observed) in source.entries.iter().zip(&review.entries) {
        if observed.query_id != expected.query_id
            || observed.slice != expected.slice
            || observed.expected_visible_text_sha256 != expected.expected_visible_text_sha256
            || observed.reviewed_exact_visible_match_count != expected.exact_visible_match_count
            || observed.reviewed_visible_pixel_match_set_commitment_sha256
                != expected.visible_pixel_match_set_commitment_sha256
        {
            return Err(
                "accessibility catalog review entry does not match the exact source".into(),
            );
        }
        if expected.exact_visible_match_count == 0 {
            if observed.every_matched_region_visibly_contains_exact_catalog_label
                || !observed.visible_absence_reviewed_when_match_count_is_zero
            {
                return Err(
                    "zero-match catalog entry requires an explicit visible-absence review"
                        .to_owned(),
                );
            }
        } else if !observed.every_matched_region_visibly_contains_exact_catalog_label
            || observed.visible_absence_reviewed_when_match_count_is_zero
        {
            return Err(
                "positive catalog entry requires exact visible-label review for every match"
                    .to_owned(),
            );
        }
    }
    validate_sha256_text_v1(&review.review_commitment_sha256)?;
    if review.review_commitment_sha256
        != mtgo_visible_accessibility_catalog_review_commitment_v1(review)?
    {
        return Err("accessibility catalog review commitment changed".to_owned());
    }
    Ok(())
}

fn covered_catalog_slices_v1(
    entries: &[MtgoVisibleAccessibilityPixelCatalogEntrySummaryV1],
) -> Vec<MtgoVisibleAccessibilityCatalogSliceV1> {
    let mut covered = Vec::new();
    for slice in [
        MtgoVisibleAccessibilityCatalogSliceV1::Pregame,
        MtgoVisibleAccessibilityCatalogSliceV1::Gameplay,
        MtgoVisibleAccessibilityCatalogSliceV1::Bottoming,
        MtgoVisibleAccessibilityCatalogSliceV1::Sideboard,
    ] {
        if entries
            .iter()
            .any(|entry| entry.slice == slice && entry.exact_visible_match_count != 0)
        {
            covered.push(slice);
        }
    }
    covered
}

fn validate_sha256_text_v1(value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("accessibility catalog review digest must be lowercase SHA-256".to_owned());
    }
    Ok(())
}

fn validate_queries_v1(queries: &[MtgoVisibleAccessibilityExactTextQueryV1]) -> Result<(), String> {
    if queries.is_empty() || queries.len() > MAX_VISIBLE_ACCESSIBILITY_QUERIES_V1 {
        return Err("visible accessibility query count must be between 1 and 64".to_owned());
    }
    let mut prior_id: Option<&str> = None;
    let mut expected_texts = HashSet::new();
    for query in queries {
        if !valid_safe_identifier_v1(&query.query_id)
            || query.expected_visible_text.is_empty()
            || query.expected_visible_text.len() > 256
            || query.expected_visible_text.trim() != query.expected_visible_text
            || query.expected_visible_text.chars().any(char::is_control)
            || prior_id.is_some_and(|prior| prior >= query.query_id.as_str())
            || !expected_texts.insert(query.expected_visible_text.as_str())
        {
            return Err(
                "visible accessibility queries require canonical unique IDs and bounded exact text"
                    .to_owned(),
            );
        }
        prior_id = Some(&query.query_id);
    }
    Ok(())
}

fn build_query_results_v1(
    queries: &[MtgoVisibleAccessibilityExactTextQueryV1],
    private_matches: &[PrivateVisibleAccessibilityMatchV1],
) -> Result<Vec<MtgoVisibleAccessibilityQueryResultV1>, String> {
    queries
        .iter()
        .enumerate()
        .map(|(query_index, query)| {
            let matched = private_matches
                .iter()
                .filter(|matched| matched.query_index == query_index)
                .collect::<Vec<_>>();
            if matched.len() > MAX_MATCHES_PER_QUERY_V1 {
                return Err(
                    "visible accessibility query produced too many exact matches".to_owned(),
                );
            }
            let expected_visible_text_sha256 =
                sha256_hex_v1(query.expected_visible_text.as_bytes());
            Ok(MtgoVisibleAccessibilityQueryResultV1 {
                query_id: query.query_id.clone(),
                expected_visible_text_sha256: expected_visible_text_sha256.clone(),
                exact_visible_match_count: u32::try_from(matched.len())
                    .map_err(|_| "visible accessibility match count overflow")?,
            })
        })
        .collect()
}

fn corroborate_private_match_regions_v1(
    private_matches: &[PrivateVisibleAccessibilityMatchV1],
    client_rect_desktop_px: SignedRectV1,
    before_bgra8: &[u8],
    after_bgra8: &[u8],
    size: &MtgoSizePxV1,
) -> Result<Vec<PrivateVisibleAccessibilityPixelMatchV1>, String> {
    private_matches
        .iter()
        .map(|matched| {
            let crop = matched
                .rect_desktop_px
                .crop_box_within(client_rect_desktop_px)
                .map_err(|error| format!("convert visible accessibility bounds: {error}"))?;
            let rect_client_px = MtgoRectPxV1 {
                x: crop.left,
                y: crop.top,
                width: crop.width,
                height: crop.height,
            };
            let before_hash = mtgo_blackbox_v1::visible_frame_region_content_sha256_v1(
                before_bgra8,
                size,
                &rect_client_px,
            )
            .map_err(|error| format!("hash before accessibility match region: {error}"))?;
            let after_hash = mtgo_blackbox_v1::visible_frame_region_content_sha256_v1(
                after_bgra8,
                size,
                &rect_client_px,
            )
            .map_err(|error| format!("hash after accessibility match region: {error}"))?;
            if before_hash != after_hash {
                return Err(
                    "visible accessibility match pixels changed across the capture bracket"
                        .to_owned(),
                );
            }
            Ok(PrivateVisibleAccessibilityPixelMatchV1 {
                query_index: matched.query_index,
                control_type_id: matched.control_type_id,
                rect_client_px,
                region_bgra8_sha256: before_hash,
            })
        })
        .collect()
}

fn build_pixel_query_results_v1(
    queries: &[MtgoVisibleAccessibilityExactTextQueryV1],
    private_matches: &[PrivateVisibleAccessibilityPixelMatchV1],
) -> Result<Vec<MtgoVisibleAccessibilityPixelQueryResultV1>, String> {
    queries
        .iter()
        .enumerate()
        .map(|(query_index, query)| {
            let matched = private_matches
                .iter()
                .filter(|matched| matched.query_index == query_index)
                .collect::<Vec<_>>();
            if matched.len() > MAX_MATCHES_PER_QUERY_V1 {
                return Err(
                    "visible accessibility query produced too many pixel matches".to_owned(),
                );
            }
            let expected_visible_text_sha256 =
                sha256_hex_v1(query.expected_visible_text.as_bytes());
            let mut visible_region_hashes = matched
                .iter()
                .map(|matched| matched.region_bgra8_sha256.as_str())
                .collect::<Vec<_>>();
            visible_region_hashes.sort_unstable();
            let match_bytes = serde_json::to_vec(&visible_region_hashes)
                .map_err(|error| format!("serialize visible pixel match set: {error}"))?;
            Ok(MtgoVisibleAccessibilityPixelQueryResultV1 {
                query_id: query.query_id.clone(),
                expected_visible_text_sha256: expected_visible_text_sha256.clone(),
                exact_visible_match_count: u32::try_from(matched.len())
                    .map_err(|_| "visible accessibility pixel match count overflow")?,
                visible_pixel_match_set_commitment_sha256: commitment_v1(
                    VISIBLE_ACCESSIBILITY_PIXEL_MATCH_SET_DOMAIN_V1,
                    &[expected_visible_text_sha256.as_bytes(), &match_bytes],
                ),
            })
        })
        .collect()
}

fn summary_commitment_v1(
    summary: &MtgoVisibleAccessibilityProbeSummaryV1,
) -> Result<String, String> {
    let mut unsigned = summary.clone();
    unsigned.report_commitment_sha256.clear();
    let bytes = serde_json::to_vec(&unsigned)
        .map_err(|error| format!("serialize visible accessibility summary: {error}"))?;
    Ok(commitment_v1(
        VISIBLE_ACCESSIBILITY_REPORT_DOMAIN_V1,
        &[&bytes],
    ))
}

fn pixel_summary_commitment_v1(
    summary: &MtgoVisibleAccessibilityPixelCorroborationSummaryV1,
) -> Result<String, String> {
    let mut unsigned = summary.clone();
    unsigned.report_commitment_sha256.clear();
    let bytes = serde_json::to_vec(&unsigned)
        .map_err(|error| format!("serialize accessibility pixel summary: {error}"))?;
    Ok(commitment_v1(
        VISIBLE_ACCESSIBILITY_PIXEL_REPORT_DOMAIN_V1,
        &[&bytes],
    ))
}

fn parse_cli_v1() -> Result<
    (
        MtgoDxgiCaptureRequestV3,
        Vec<MtgoVisibleAccessibilityExactTextQueryV1>,
    ),
    String,
> {
    let mut args = std::env::args().skip(1);
    let mut expected_executable_sha256 = None;
    let mut expected_signer_thumbprint = None;
    let mut expected_signer_subject_sha256 = None;
    let mut window_mode = CaptureWindowModeV2::MainClient;
    let mut expected_game_format = None;
    let mut expected_title_contains = None;
    let mut queries = Vec::new();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--query" => {
                let query_id = args.next().ok_or("--query requires an ID and exact text")?;
                let expected_visible_text =
                    args.next().ok_or("--query requires an ID and exact text")?;
                queries.push(MtgoVisibleAccessibilityExactTextQueryV1 {
                    query_id,
                    expected_visible_text,
                });
            }
            "--expected-exe-sha256" => {
                expected_executable_sha256 = Some(
                    args.next()
                        .ok_or("missing value for --expected-exe-sha256")?,
                )
            }
            "--expected-signer-thumbprint" => {
                expected_signer_thumbprint = Some(
                    args.next()
                        .ok_or("missing value for --expected-signer-thumbprint")?,
                )
            }
            "--expected-signer-subject-sha256" => {
                expected_signer_subject_sha256 = Some(
                    args.next()
                        .ok_or("missing value for --expected-signer-subject-sha256")?,
                )
            }
            "--window-mode" => {
                let value = args.next().ok_or("missing value for --window-mode")?;
                window_mode = match value.as_str() {
                    "main_client" => CaptureWindowModeV2::MainClient,
                    "solitaire_game" => CaptureWindowModeV2::SolitaireGame,
                    "duel_game" => CaptureWindowModeV2::DuelGame,
                    "spectator_game" => CaptureWindowModeV2::SpectatorGame,
                    _ => {
                        return Err(
                            "window mode must be main_client, solitaire_game, duel_game, or spectator_game"
                                .to_owned(),
                        )
                    }
                };
            }
            "--expected-game-format" => {
                expected_game_format = Some(
                    args.next()
                        .ok_or("missing value for --expected-game-format")?,
                )
            }
            "--expected-title-contains" => {
                expected_title_contains = Some(
                    args.next()
                        .ok_or("missing value for --expected-title-contains")?,
                )
            }
            _ => return Err(format!("unknown argument {argument}")),
        }
    }
    Ok((
        MtgoDxgiCaptureRequestV3 {
            expected_executable_sha256: expected_executable_sha256
                .ok_or("--expected-exe-sha256 is required")?,
            expected_signer_thumbprint: expected_signer_thumbprint
                .ok_or("--expected-signer-thumbprint is required")?,
            expected_signer_subject_sha256: expected_signer_subject_sha256
                .ok_or("--expected-signer-subject-sha256 is required")?,
            window_mode,
            expected_game_format,
            expected_title_contains,
            timeout_ms: 1_500,
        },
        queries,
    ))
}

fn parse_catalog_cli_v1() -> Result<MtgoDxgiCaptureRequestV3, String> {
    let (request, output) = parse_catalog_arguments_v1(std::env::args().skip(1), false)?;
    if output.is_some() {
        return Err("catalog probe does not accept an output directory".to_owned());
    }
    Ok(request)
}

fn parse_catalog_review_artifact_cli_v1() -> Result<(MtgoDxgiCaptureRequestV3, PathBuf), String> {
    let (request, output) = parse_catalog_arguments_v1(std::env::args().skip(1), true)?;
    Ok((
        request,
        output.ok_or("--output is required for a review artifact")?,
    ))
}

fn parse_catalog_corpus_arguments_v1<I>(args: I) -> Result<Vec<(PathBuf, PathBuf)>, String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter();
    let mut cases = Vec::new();
    while let Some(argument) = args.next() {
        if argument != "--case" {
            return Err("corpus evaluation accepts only repeated --case arguments".to_owned());
        }
        let artifact_directory = args
            .next()
            .ok_or("--case requires an artifact directory and completed review path")?;
        let completed_review_path = args
            .next()
            .ok_or("--case requires an artifact directory and completed review path")?;
        cases.push((
            PathBuf::from(artifact_directory),
            PathBuf::from(completed_review_path),
        ));
        if cases.len() > MAX_VISIBLE_ACCESSIBILITY_CORPUS_CASES_V1 {
            return Err("catalog corpus accepts at most 64 reviewed cases".to_owned());
        }
    }
    if cases.is_empty() {
        return Err("catalog corpus requires at least one --case pair".to_owned());
    }
    Ok(cases)
}

fn parse_catalog_review_finalization_arguments_v1<I>(
    args: I,
) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf), String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter();
    let mut artifact = None;
    let mut edited_review = None;
    let mut reviewer_alias = None;
    let mut output = None;
    while let Some(argument) = args.next() {
        let destination = match argument.to_str() {
            Some("--artifact") => &mut artifact,
            Some("--edited-review") => &mut edited_review,
            Some("--reviewer-alias") => &mut reviewer_alias,
            Some("--output") => &mut output,
            _ => {
                return Err(
                    "review finalization accepts only --artifact, --edited-review, --reviewer-alias, and --output"
                        .to_owned(),
                )
            }
        };
        if destination.is_some() {
            return Err("review finalization arguments may appear only once".to_owned());
        }
        *destination = Some(PathBuf::from(
            args.next()
                .ok_or("review finalization argument is missing its path")?,
        ));
    }
    Ok((
        artifact.ok_or("--artifact is required")?,
        edited_review.ok_or("--edited-review is required")?,
        reviewer_alias.ok_or("--reviewer-alias is required")?,
        output.ok_or("--output is required")?,
    ))
}

fn parse_catalog_arguments_v1<I>(
    args: I,
    permit_output: bool,
) -> Result<(MtgoDxgiCaptureRequestV3, Option<PathBuf>), String>
where
    I: IntoIterator<Item = String>,
{
    let mut expected_executable_sha256 = None;
    let mut expected_signer_thumbprint = None;
    let mut expected_signer_subject_sha256 = None;
    let mut window_mode = CaptureWindowModeV2::DuelGame;
    let mut expected_game_format = None;
    let mut expected_title_contains = None;
    let mut output = None;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--expected-exe-sha256" => {
                expected_executable_sha256 = Some(
                    args.next()
                        .ok_or("missing value for --expected-exe-sha256")?,
                )
            }
            "--expected-signer-thumbprint" => {
                expected_signer_thumbprint = Some(
                    args.next()
                        .ok_or("missing value for --expected-signer-thumbprint")?,
                )
            }
            "--expected-signer-subject-sha256" => {
                expected_signer_subject_sha256 = Some(
                    args.next()
                        .ok_or("missing value for --expected-signer-subject-sha256")?,
                )
            }
            "--window-mode" => {
                let value = args.next().ok_or("missing value for --window-mode")?;
                window_mode = match value.as_str() {
                    "solitaire_game" => CaptureWindowModeV2::SolitaireGame,
                    "duel_game" => CaptureWindowModeV2::DuelGame,
                    _ => {
                        return Err(
                            "catalog window mode must be solitaire_game or duel_game".to_owned()
                        )
                    }
                };
            }
            "--expected-game-format" => {
                expected_game_format = Some(
                    args.next()
                        .ok_or("missing value for --expected-game-format")?,
                )
            }
            "--expected-title-contains" => {
                expected_title_contains = Some(
                    args.next()
                        .ok_or("missing value for --expected-title-contains")?,
                )
            }
            "--output" if permit_output => {
                if output.is_some() {
                    return Err("--output may be supplied exactly once".to_owned());
                }
                output = Some(PathBuf::from(
                    args.next().ok_or("missing value for --output")?,
                ));
            }
            _ => return Err(format!("unknown catalog argument {argument}")),
        }
    }
    Ok((
        MtgoDxgiCaptureRequestV3 {
            expected_executable_sha256: expected_executable_sha256
                .ok_or("--expected-exe-sha256 is required")?,
            expected_signer_thumbprint: expected_signer_thumbprint
                .ok_or("--expected-signer-thumbprint is required")?,
            expected_signer_subject_sha256: expected_signer_subject_sha256
                .ok_or("--expected-signer-subject-sha256 is required")?,
            window_mode,
            expected_game_format,
            expected_title_contains,
            timeout_ms: 1_500,
        },
        output,
    ))
}

fn valid_safe_identifier_v1(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update((domain.len() as u64).to_be_bytes());
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

struct ComApartmentGuardV1;

impl ComApartmentGuardV1 {
    fn initialize_v1() -> Result<Self, String> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|error| format!("initialize UI Automation COM apartment: {error}"))?;
        }
        Ok(Self)
    }
}

impl Drop for ComApartmentGuardV1 {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queries_v1() -> Vec<MtgoVisibleAccessibilityExactTextQueryV1> {
        vec![
            MtgoVisibleAccessibilityExactTextQueryV1 {
                query_id: "challenge-card".to_owned(),
                expected_visible_text: "Premodern Challenge 32".to_owned(),
            },
            MtgoVisibleAccessibilityExactTextQueryV1 {
                query_id: "league-card".to_owned(),
                expected_visible_text: "Premodern League".to_owned(),
            },
        ]
    }

    fn reviewed_pixel_catalog_source_v1() -> MtgoVisibleAccessibilityPixelCatalogProbeSummaryV1 {
        let catalog = known_label_catalog_v1();
        let query_results = catalog
            .iter()
            .enumerate()
            .map(
                |(index, entry)| MtgoVisibleAccessibilityPixelQueryResultV1 {
                    query_id: entry.query_id.to_owned(),
                    expected_visible_text_sha256: sha256_hex_v1(
                        entry.expected_visible_text.as_bytes(),
                    ),
                    exact_visible_match_count: u32::from(index == 0),
                    visible_pixel_match_set_commitment_sha256: format!("{index:064x}"),
                },
            )
            .collect();
        let mut source = MtgoVisibleAccessibilityPixelCorroborationSummaryV1 {
            schema_version: MTGO_VISIBLE_ACCESSIBILITY_PIXEL_CORROBORATION_SCHEMA_V1,
            before_frame_sha256: "d".repeat(64),
            after_frame_sha256: "e".repeat(64),
            accessibility_report_commitment_sha256: "f".repeat(64),
            query_results,
            total_pixel_corroborated_match_count: 1,
            has_pixel_corroborated_match: true,
            matched_regions_pixel_stable_across_bracket: true,
            raw_visible_text_exposed: false,
            private_match_rectangles_exposed: false,
            safe_for_semantic_evidence: false,
            safe_for_policy_scoring: false,
            safe_for_input: false,
            report_commitment_sha256: String::new(),
        };
        source.report_commitment_sha256 = pixel_summary_commitment_v1(&source).unwrap();
        build_known_label_pixel_catalog_summary_v1(
            &catalog,
            known_label_catalog_commitment_v1(&catalog).unwrap(),
            source,
        )
        .unwrap()
    }

    fn private_corpus_case_v1(
        seed: u8,
        positive_query_indices: &[usize],
    ) -> PrivateVisibleAccessibilityCatalogCorpusCaseV1 {
        let catalog = known_label_catalog_v1();
        PrivateVisibleAccessibilityCatalogCorpusCaseV1 {
            catalog_commitment_sha256: known_label_catalog_commitment_v1(&catalog).unwrap(),
            before_frame_sha256: format!("{seed:064x}"),
            after_frame_sha256: format!("{:064x}", seed + 64),
            case_candidate_commitment_sha256: format!("{:064x}", seed + 128),
            entries: catalog
                .iter()
                .enumerate()
                .map(
                    |(index, entry)| MtgoVisibleAccessibilityPixelCatalogEntrySummaryV1 {
                        query_id: entry.query_id.to_owned(),
                        slice: entry.slice,
                        expected_visible_text_sha256: sha256_hex_v1(
                            entry.expected_visible_text.as_bytes(),
                        ),
                        exact_visible_match_count: u32::from(
                            positive_query_indices.contains(&index),
                        ),
                        visible_pixel_match_set_commitment_sha256: format!(
                            "{:064x}",
                            usize::from(seed) * 16 + index
                        ),
                    },
                )
                .collect(),
        }
    }

    fn exact_catalog_review_v1(
        source: &MtgoVisibleAccessibilityPixelCatalogProbeSummaryV1,
    ) -> MtgoVisibleAccessibilityCatalogReviewV1 {
        let entries = source
            .entries
            .iter()
            .map(|entry| MtgoVisibleAccessibilityCatalogReviewEntryV1 {
                query_id: entry.query_id.clone(),
                slice: entry.slice,
                expected_visible_text_sha256: entry.expected_visible_text_sha256.clone(),
                reviewed_exact_visible_match_count: entry.exact_visible_match_count,
                reviewed_visible_pixel_match_set_commitment_sha256: entry
                    .visible_pixel_match_set_commitment_sha256
                    .clone(),
                every_matched_region_visibly_contains_exact_catalog_label: entry
                    .exact_visible_match_count
                    != 0,
                visible_absence_reviewed_when_match_count_is_zero: entry.exact_visible_match_count
                    == 0,
            })
            .collect();
        let mut review = MtgoVisibleAccessibilityCatalogReviewV1 {
            schema_version: MTGO_VISIBLE_ACCESSIBILITY_CATALOG_REVIEW_SCHEMA_V1,
            catalog_commitment_sha256: source.catalog_commitment_sha256.clone(),
            source_pixel_report_commitment_sha256: source
                .source_pixel_report_commitment_sha256
                .clone(),
            before_frame_sha256: source.before_frame_sha256.clone(),
            after_frame_sha256: source.after_frame_sha256.clone(),
            reviewer_alias_sha256: "9".repeat(64),
            client_only_and_unobscured_confirmed: true,
            exact_frame_pair_identity_confirmed: true,
            entries,
            review_commitment_sha256: String::new(),
        };
        review.review_commitment_sha256 =
            mtgo_visible_accessibility_catalog_review_commitment_v1(&review).unwrap();
        review
    }

    fn synthetic_review_artifact_files_v1(
    ) -> PrivateVisibleAccessibilityCatalogReviewArtifactFilesV1 {
        let before_pixels = vec![10, 20, 30, 255, 40, 50, 60, 255];
        let after_pixels = vec![10, 20, 30, 255, 41, 51, 61, 255];
        let crop_pixels = before_pixels[..4].to_vec();
        let crop_region_hash = mtgo_blackbox_v1::visible_frame_region_content_sha256_v1(
            &before_pixels,
            &MtgoSizePxV1 {
                width: 2,
                height: 1,
            },
            &MtgoRectPxV1 {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
        )
        .unwrap();
        let before_png = encode_visible_accessibility_png_v1(&before_pixels, 2, 1).unwrap();
        let after_png = encode_visible_accessibility_png_v1(&after_pixels, 2, 1).unwrap();
        let crop_png = encode_visible_accessibility_png_v1(&crop_pixels, 1, 1).unwrap();
        let crop_file = "match-00-00.png".to_owned();
        let crop_files = vec![(crop_file.clone(), crop_png.clone())];
        let catalog = known_label_catalog_v1();
        let query_results = catalog
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let expected_visible_text_sha256 =
                    sha256_hex_v1(entry.expected_visible_text.as_bytes());
                let region_hashes = if index == 0 {
                    vec![crop_region_hash.clone()]
                } else {
                    Vec::new()
                };
                let match_bytes = serde_json::to_vec(&region_hashes).unwrap();
                MtgoVisibleAccessibilityPixelQueryResultV1 {
                    query_id: entry.query_id.to_owned(),
                    expected_visible_text_sha256: expected_visible_text_sha256.clone(),
                    exact_visible_match_count: u32::from(index == 0),
                    visible_pixel_match_set_commitment_sha256: commitment_v1(
                        VISIBLE_ACCESSIBILITY_PIXEL_MATCH_SET_DOMAIN_V1,
                        &[expected_visible_text_sha256.as_bytes(), &match_bytes],
                    ),
                }
            })
            .collect::<Vec<_>>();
        let mut plain = MtgoVisibleAccessibilityProbeSummaryV1 {
            schema_version: MTGO_VISIBLE_ACCESSIBILITY_PROBE_SCHEMA_V1,
            query_results: query_results
                .iter()
                .map(|entry| MtgoVisibleAccessibilityQueryResultV1 {
                    query_id: entry.query_id.clone(),
                    expected_visible_text_sha256: entry.expected_visible_text_sha256.clone(),
                    exact_visible_match_count: entry.exact_visible_match_count,
                })
                .collect(),
            raw_visible_text_exposed: false,
            requires_same_frame_pixel_corroboration: true,
            safe_for_semantic_evidence: false,
            safe_for_policy_scoring: false,
            safe_for_input: false,
            report_commitment_sha256: String::new(),
        };
        plain.report_commitment_sha256 = summary_commitment_v1(&plain).unwrap();
        let mut pixel = MtgoVisibleAccessibilityPixelCorroborationSummaryV1 {
            schema_version: MTGO_VISIBLE_ACCESSIBILITY_PIXEL_CORROBORATION_SCHEMA_V1,
            before_frame_sha256: sha256_hex_v1(&before_pixels),
            after_frame_sha256: sha256_hex_v1(&after_pixels),
            accessibility_report_commitment_sha256: plain.report_commitment_sha256,
            query_results,
            total_pixel_corroborated_match_count: 1,
            has_pixel_corroborated_match: true,
            matched_regions_pixel_stable_across_bracket: true,
            raw_visible_text_exposed: false,
            private_match_rectangles_exposed: false,
            safe_for_semantic_evidence: false,
            safe_for_policy_scoring: false,
            safe_for_input: false,
            report_commitment_sha256: String::new(),
        };
        pixel.report_commitment_sha256 = pixel_summary_commitment_v1(&pixel).unwrap();
        let catalog_summary = build_known_label_pixel_catalog_summary_v1(
            &catalog,
            known_label_catalog_commitment_v1(&catalog).unwrap(),
            pixel,
        )
        .unwrap();
        let review_template = review_template_for_catalog_summary_v1(&catalog_summary).unwrap();
        let review_template_bytes = serde_json::to_vec_pretty(&review_template).unwrap();
        let mut manifest = PrivateVisibleAccessibilityCatalogReviewArtifactManifestV1 {
            schema_version: MTGO_VISIBLE_ACCESSIBILITY_CATALOG_REVIEW_ARTIFACT_SCHEMA_V1,
            artifact_kind: "mtgo_visible_accessibility_catalog_review_artifact_v1".to_owned(),
            catalog_commitment_sha256: catalog_summary.catalog_commitment_sha256,
            source_pixel_report_commitment_sha256: catalog_summary
                .source_pixel_report_commitment_sha256,
            before_frame_sha256: sha256_hex_v1(&before_pixels),
            after_frame_sha256: sha256_hex_v1(&after_pixels),
            before_visible_client_png_file: "before-visible-client.png".to_owned(),
            before_visible_client_png_sha256: sha256_hex_v1(&before_png),
            after_visible_client_png_file: "after-visible-client.png".to_owned(),
            after_visible_client_png_sha256: sha256_hex_v1(&after_png),
            review_template_file: "review-template.json".to_owned(),
            review_template_sha256: sha256_hex_v1(&review_template_bytes),
            entries: catalog_summary
                .entries
                .into_iter()
                .enumerate()
                .map(
                    |(index, entry)| PrivateVisibleAccessibilityCatalogReviewArtifactEntryV1 {
                        query_id: entry.query_id,
                        slice: entry.slice,
                        expected_visible_text_sha256: entry.expected_visible_text_sha256,
                        exact_visible_match_count: entry.exact_visible_match_count,
                        match_crops: if index == 0 {
                            vec![PrivateVisibleAccessibilityCatalogReviewCropV1 {
                                file: crop_file.clone(),
                                png_sha256: sha256_hex_v1(&crop_png),
                                region_bgra8_sha256: crop_region_hash.clone(),
                            }]
                        } else {
                            Vec::new()
                        },
                    },
                )
                .collect(),
            exact_visible_match_crop_count: 1,
            raw_or_unmatched_visible_text_exposed: false,
            pixel_coordinates_exposed: false,
            accessibility_metadata_exposed: false,
            process_or_window_metadata_exposed: false,
            human_review_complete: false,
            safe_for_semantic_evidence: false,
            safe_for_policy_scoring: false,
            safe_for_input: false,
            artifact_commitment_sha256: String::new(),
        };
        manifest.artifact_commitment_sha256 =
            visible_accessibility_catalog_review_artifact_commitment_v1(
                &manifest,
                &review_template_bytes,
                &before_png,
                &after_png,
                &crop_files,
            )
            .unwrap();
        let manifest_bytes = serde_json::to_vec_pretty(&manifest).unwrap();
        PrivateVisibleAccessibilityCatalogReviewArtifactFilesV1 {
            manifest,
            manifest_bytes,
            review_template_bytes,
            before_visible_client_png: before_png,
            after_visible_client_png: after_png,
            crop_files,
        }
    }

    fn unique_temp_review_output_v1(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mtgo-visible-accessibility-review-test-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn exact_queries_are_canonical_and_private_results_are_commitment_only() {
        let queries = queries_v1();
        validate_queries_v1(&queries).unwrap();
        let matches = vec![
            PrivateVisibleAccessibilityMatchV1 {
                query_index: 0,
                control_type_id: 50_007,
                rect_desktop_px: SignedRectV1 {
                    left: -100,
                    top: 20,
                    right: -20,
                    bottom: 60,
                },
            },
            PrivateVisibleAccessibilityMatchV1 {
                query_index: 1,
                control_type_id: 50_020,
                rect_desktop_px: SignedRectV1 {
                    left: 10,
                    top: 20,
                    right: 90,
                    bottom: 60,
                },
            },
        ];
        let results = build_query_results_v1(&queries, &matches).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].exact_visible_match_count, 1);
        let json = serde_json::to_string(&results).unwrap();
        assert!(!json.contains("Premodern"));
        assert!(!json.contains("left"));
        assert!(!json.contains("top"));
        assert!(!json.contains("control_type"));
    }

    #[test]
    fn duplicate_unsorted_or_unsafe_queries_fail_closed() {
        let mut duplicate = queries_v1();
        duplicate[1].expected_visible_text = duplicate[0].expected_visible_text.clone();
        assert!(validate_queries_v1(&duplicate).is_err());

        let mut unsorted = queries_v1();
        unsorted.swap(0, 1);
        assert!(validate_queries_v1(&unsorted).is_err());

        let mut unsafe_text = queries_v1();
        unsafe_text[0].expected_visible_text.push('\n');
        assert!(validate_queries_v1(&unsafe_text).is_err());
    }

    #[test]
    fn known_label_catalog_is_canonical_and_hash_only_at_the_public_boundary() {
        let catalog = known_label_catalog_v1();
        validate_known_label_catalog_v1(&catalog).unwrap();
        assert_eq!(catalog.len(), 5);
        assert_eq!(catalog[0].query_id, "bottoming.cancel");
        assert_eq!(catalog[4].query_id, "sideboard.submit_deck");
        let commitment = known_label_catalog_commitment_v1(&catalog).unwrap();
        assert_eq!(commitment.len(), 64);

        let entries = catalog
            .iter()
            .map(|entry| MtgoVisibleAccessibilityCatalogEntrySummaryV1 {
                query_id: entry.query_id.to_owned(),
                slice: entry.slice,
                expected_visible_text_sha256: sha256_hex_v1(entry.expected_visible_text.as_bytes()),
                exact_visible_match_count: 0,
            })
            .collect::<Vec<_>>();
        let summary = MtgoVisibleAccessibilityCatalogProbeSummaryV1 {
            schema_version: MTGO_VISIBLE_ACCESSIBILITY_CATALOG_SCHEMA_V1,
            catalog_commitment_sha256: commitment,
            source_probe_commitment_sha256: "b".repeat(64),
            report_commitment_sha256: "d".repeat(64),
            catalog_entry_count: 5,
            matched_catalog_entry_count: 0,
            total_exact_visible_match_count: 0,
            entries,
            raw_visible_text_exposed: false,
            caller_selected_text_queries_enabled: false,
            unmatched_visible_text_retained: false,
            requires_same_frame_pixel_corroboration: true,
            safe_for_semantic_evidence: false,
            safe_for_policy_scoring: false,
            safe_for_input: false,
        };
        let json = serde_json::to_string(&summary).unwrap();
        for raw_label in ["Cancel", "Combat", "Keep", "Mulligan", "Submit Deck"] {
            assert!(!json.contains(raw_label));
        }
        assert!(json.contains("bottoming.cancel"));
        assert!(json.contains("sideboard.submit_deck"));
    }

    #[test]
    fn known_label_pixel_catalog_is_hash_only_and_rejects_source_substitution() {
        let catalog = known_label_catalog_v1();
        let catalog_commitment = known_label_catalog_commitment_v1(&catalog).unwrap();
        let query_results = catalog
            .iter()
            .map(|entry| MtgoVisibleAccessibilityPixelQueryResultV1 {
                query_id: entry.query_id.to_owned(),
                expected_visible_text_sha256: sha256_hex_v1(entry.expected_visible_text.as_bytes()),
                exact_visible_match_count: 0,
                visible_pixel_match_set_commitment_sha256: "a".repeat(64),
            })
            .collect();
        let mut source = MtgoVisibleAccessibilityPixelCorroborationSummaryV1 {
            schema_version: MTGO_VISIBLE_ACCESSIBILITY_PIXEL_CORROBORATION_SCHEMA_V1,
            before_frame_sha256: "d".repeat(64),
            after_frame_sha256: "e".repeat(64),
            accessibility_report_commitment_sha256: "f".repeat(64),
            query_results,
            total_pixel_corroborated_match_count: 0,
            has_pixel_corroborated_match: false,
            matched_regions_pixel_stable_across_bracket: true,
            raw_visible_text_exposed: false,
            private_match_rectangles_exposed: false,
            safe_for_semantic_evidence: false,
            safe_for_policy_scoring: false,
            safe_for_input: false,
            report_commitment_sha256: String::new(),
        };
        source.report_commitment_sha256 = pixel_summary_commitment_v1(&source).unwrap();
        let summary = build_known_label_pixel_catalog_summary_v1(
            &catalog,
            catalog_commitment.clone(),
            source.clone(),
        )
        .unwrap();
        assert_eq!(
            summary.report_commitment_sha256,
            known_label_pixel_catalog_report_commitment_v1(&summary).unwrap()
        );
        let json = serde_json::to_string(&summary).unwrap();
        for raw_label in ["Cancel", "Combat", "Keep", "Mulligan", "Submit Deck"] {
            assert!(!json.contains(raw_label));
        }
        assert!(!summary.safe_for_semantic_evidence);
        assert!(!summary.safe_for_policy_scoring);
        assert!(!summary.safe_for_input);

        source.query_results[0].query_id = "crossed.query".to_owned();
        source.report_commitment_sha256 = pixel_summary_commitment_v1(&source).unwrap();
        assert!(
            build_known_label_pixel_catalog_summary_v1(&catalog, catalog_commitment, source)
                .is_err()
        );
    }

    #[test]
    fn catalog_review_is_exact_case_bound_and_fail_closed() {
        let source = reviewed_pixel_catalog_source_v1();
        let review = exact_catalog_review_v1(&source);
        validate_visible_accessibility_catalog_review_v1(&source, &review).unwrap();
        let evaluation =
            evaluate_visible_accessibility_catalog_case_summary_v1(&source, &review).unwrap();
        assert!(evaluation.exact_review_agreement);
        assert_eq!(evaluation.evaluated_entry_count, 5);
        assert_eq!(evaluation.positive_entry_count, 1);
        assert_eq!(evaluation.total_exact_visible_match_count, 1);
        assert!(!evaluation.production_evaluation_ratified);
        assert!(!evaluation.safe_for_semantic_evidence);
        assert!(!evaluation.safe_for_policy_scoring);
        assert!(!evaluation.safe_for_input);
        assert_eq!(
            covered_catalog_slices_v1(&source.entries),
            vec![MtgoVisibleAccessibilityCatalogSliceV1::Bottoming]
        );

        let mut mutations = Vec::new();
        let mut crossed_frame = review.clone();
        crossed_frame.before_frame_sha256 = "8".repeat(64);
        mutations.push(crossed_frame);
        let mut changed_count = review.clone();
        changed_count.entries[0].reviewed_exact_visible_match_count = 2;
        mutations.push(changed_count);
        let mut missing_positive_review = review.clone();
        missing_positive_review.entries[0]
            .every_matched_region_visibly_contains_exact_catalog_label = false;
        mutations.push(missing_positive_review);
        let mut false_zero_claim = review.clone();
        false_zero_claim.entries[1].every_matched_region_visibly_contains_exact_catalog_label =
            true;
        mutations.push(false_zero_claim);
        let mut missing_zero_review = review.clone();
        missing_zero_review.entries[1].visible_absence_reviewed_when_match_count_is_zero = false;
        mutations.push(missing_zero_review);
        let mut forged_commitment = review.clone();
        forged_commitment.review_commitment_sha256 = "7".repeat(64);
        mutations.push(forged_commitment);

        for mutation in mutations {
            assert!(validate_visible_accessibility_catalog_review_v1(&source, &mutation).is_err());
        }
    }

    #[test]
    fn corpus_evaluation_requires_reviewed_presence_and_absence_for_every_label() {
        let cases = vec![
            private_corpus_case_v1(1, &[0, 1, 2]),
            private_corpus_case_v1(2, &[3, 4]),
        ];
        let summary = evaluate_visible_accessibility_catalog_corpus_summary_v1(&cases).unwrap();
        assert_eq!(summary.reviewed_case_count, 2);
        assert_eq!(summary.distinct_visible_frame_pair_count, 2);
        assert_eq!(summary.entries.len(), 5);
        assert!(summary
            .entries
            .iter()
            .all(|entry| entry.reviewed_positive_case_count == 1
                && entry.reviewed_zero_case_count == 1
                && entry.presence_and_absence_coverage_complete));
        assert!(summary.every_catalog_entry_has_reviewed_presence_and_absence);
        assert!(summary.exact_frame_pairs_are_distinct_across_cases);
        assert!(!summary.production_evaluation_ratified);
        assert!(!summary.safe_for_semantic_evidence);
        assert!(!summary.safe_for_policy_scoring);
        assert!(!summary.safe_for_input);

        let json = serde_json::to_string(&summary).unwrap();
        for raw_label in ["Cancel", "Combat", "Keep", "Mulligan", "Submit Deck"] {
            assert!(!json.contains(raw_label));
        }
        for forbidden in ["frame_sha256", "process", "window", "control_type", "rect"] {
            assert!(!json.contains(forbidden));
        }
    }

    #[test]
    fn corpus_evaluation_rejects_missing_coverage_duplicate_frames_and_identity_drift() {
        let all_positive = private_corpus_case_v1(1, &[0, 1, 2, 3, 4]);
        assert!(
            evaluate_visible_accessibility_catalog_corpus_summary_v1(std::slice::from_ref(
                &all_positive
            ))
            .is_err()
        );

        let all_zero = private_corpus_case_v1(2, &[]);
        let mut duplicate_pair = all_zero.clone();
        duplicate_pair.before_frame_sha256 = all_positive.before_frame_sha256.clone();
        duplicate_pair.after_frame_sha256 = all_positive.after_frame_sha256.clone();
        assert!(evaluate_visible_accessibility_catalog_corpus_summary_v1(&[
            all_positive.clone(),
            duplicate_pair,
        ])
        .is_err());

        let mut wrong_catalog = all_zero.clone();
        wrong_catalog.catalog_commitment_sha256 = "f".repeat(64);
        assert!(evaluate_visible_accessibility_catalog_corpus_summary_v1(&[
            all_positive.clone(),
            wrong_catalog,
        ])
        .is_err());

        let mut wrong_entry = all_zero;
        wrong_entry.entries[2].query_id = "changed.query".to_owned();
        assert!(evaluate_visible_accessibility_catalog_corpus_summary_v1(&[
            all_positive,
            wrong_entry,
        ])
        .is_err());
    }

    #[test]
    fn corpus_candidate_commitment_is_order_independent_but_case_bound() {
        let first = private_corpus_case_v1(1, &[0, 1, 2]);
        let second = private_corpus_case_v1(2, &[3, 4]);
        let expected = evaluate_visible_accessibility_catalog_corpus_summary_v1(&[
            first.clone(),
            second.clone(),
        ])
        .unwrap();
        let reordered =
            evaluate_visible_accessibility_catalog_corpus_summary_v1(&[second.clone(), first])
                .unwrap();
        assert_eq!(
            expected.corpus_candidate_commitment_sha256,
            reordered.corpus_candidate_commitment_sha256
        );

        let mut changed = second;
        changed.case_candidate_commitment_sha256 = "f".repeat(64);
        let changed = evaluate_visible_accessibility_catalog_corpus_summary_v1(&[
            private_corpus_case_v1(1, &[0, 1, 2]),
            changed,
        ])
        .unwrap();
        assert_ne!(
            expected.corpus_candidate_commitment_sha256,
            changed.corpus_candidate_commitment_sha256
        );
    }

    #[test]
    fn plain_match_summary_exposes_counts_but_no_metadata_commitment() {
        let queries = queries_v1();
        let baseline = vec![PrivateVisibleAccessibilityMatchV1 {
            query_index: 0,
            control_type_id: 50_007,
            rect_desktop_px: SignedRectV1 {
                left: 10,
                top: 20,
                right: 90,
                bottom: 60,
            },
        }];
        let expected = build_query_results_v1(&queries, &baseline).unwrap();
        assert_eq!(expected[0].exact_visible_match_count, 1);
        let json = serde_json::to_string(&expected).unwrap();
        assert!(!json.contains("commitment"));
        assert!(!json.contains("control_type"));
        assert!(!json.contains("rect"));
    }

    #[test]
    fn negative_origin_pixel_corroboration_binds_exact_stable_regions() {
        let queries = queries_v1();
        let matches = vec![PrivateVisibleAccessibilityMatchV1 {
            query_index: 1,
            control_type_id: 50_020,
            rect_desktop_px: SignedRectV1 {
                left: -98,
                top: -48,
                right: -96,
                bottom: -46,
            },
        }];
        let mut before = vec![0_u8; 4 * 4 * 4];
        for (index, byte) in before.iter_mut().enumerate() {
            *byte = u8::try_from(index).unwrap();
        }
        let after = before.clone();
        let private = corroborate_private_match_regions_v1(
            &matches,
            SignedRectV1 {
                left: -100,
                top: -50,
                right: -96,
                bottom: -46,
            },
            &before,
            &after,
            &MtgoSizePxV1 {
                width: 4,
                height: 4,
            },
        )
        .unwrap();
        assert_eq!(
            private[0].rect_client_px,
            MtgoRectPxV1 {
                x: 2,
                y: 2,
                width: 2,
                height: 2,
            }
        );
        let results = build_pixel_query_results_v1(&queries, &private).unwrap();
        assert_eq!(results[0].exact_visible_match_count, 0);
        assert_eq!(results[1].exact_visible_match_count, 1);
        let json = serde_json::to_string(&results).unwrap();
        assert!(!json.contains("Premodern"));
        assert!(!json.contains("rect_client_px"));
    }

    #[test]
    fn changed_match_pixels_or_invalid_bounds_reject_but_outside_change_is_allowed() {
        let matched = PrivateVisibleAccessibilityMatchV1 {
            query_index: 0,
            control_type_id: 50_007,
            rect_desktop_px: SignedRectV1 {
                left: 10,
                top: 10,
                right: 12,
                bottom: 12,
            },
        };
        let size = MtgoSizePxV1 {
            width: 4,
            height: 4,
        };
        let client = SignedRectV1 {
            left: 10,
            top: 10,
            right: 14,
            bottom: 14,
        };
        let before = vec![7_u8; 4 * 4 * 4];
        let mut outside_changed = before.clone();
        outside_changed[(3 * 4 + 3) * 4] = 8;
        assert!(corroborate_private_match_regions_v1(
            std::slice::from_ref(&matched),
            client,
            &before,
            &outside_changed,
            &size,
        )
        .is_ok());

        let mut inside_changed = before.clone();
        inside_changed[0] = 8;
        assert!(corroborate_private_match_regions_v1(
            std::slice::from_ref(&matched),
            client,
            &before,
            &inside_changed,
            &size,
        )
        .is_err());

        let mut off_client = matched;
        off_client.rect_desktop_px.left = 9;
        assert!(corroborate_private_match_regions_v1(
            &[off_client],
            client,
            &before,
            &before,
            &size,
        )
        .is_err());
    }

    #[test]
    fn pixel_match_commitment_binds_only_visible_pixels_and_fixed_query_text() {
        let queries = queries_v1();
        let baseline = vec![PrivateVisibleAccessibilityPixelMatchV1 {
            query_index: 0,
            control_type_id: 50_007,
            rect_client_px: MtgoRectPxV1 {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
            },
            region_bgra8_sha256: "a".repeat(64),
        }];
        let expected = build_pixel_query_results_v1(&queries, &baseline).unwrap();
        let mut changed_pixels = baseline.clone();
        changed_pixels[0].region_bgra8_sha256 = "b".repeat(64);
        assert_ne!(
            expected[0].visible_pixel_match_set_commitment_sha256,
            build_pixel_query_results_v1(&queries, &changed_pixels).unwrap()[0]
                .visible_pixel_match_set_commitment_sha256
        );
        for metadata_only in [
            {
                let mut value = baseline.clone();
                value[0].rect_client_px.x += 1;
                value
            },
            {
                let mut value = baseline.clone();
                value[0].control_type_id += 1;
                value
            },
        ] {
            assert_eq!(
                expected[0].visible_pixel_match_set_commitment_sha256,
                build_pixel_query_results_v1(&queries, &metadata_only).unwrap()[0]
                    .visible_pixel_match_set_commitment_sha256
            );
        }
        let mut changed_queries = queries;
        changed_queries[0].expected_visible_text.push('!');
        assert_ne!(
            expected[0].visible_pixel_match_set_commitment_sha256,
            build_pixel_query_results_v1(&changed_queries, &baseline).unwrap()[0]
                .visible_pixel_match_set_commitment_sha256
        );
    }

    #[test]
    fn review_artifact_contains_only_visible_pngs_and_coordinate_free_commitments() {
        let files = synthetic_review_artifact_files_v1();
        assert_eq!(
            files.manifest.artifact_commitment_sha256,
            visible_accessibility_catalog_review_artifact_commitment_v1(
                &files.manifest,
                &files.review_template_bytes,
                &files.before_visible_client_png,
                &files.after_visible_client_png,
                &files.crop_files,
            )
            .unwrap()
        );
        let manifest = String::from_utf8(files.manifest_bytes.clone()).unwrap();
        for forbidden in [
            "\"process_id\"",
            "\"window_handle\"",
            "\"client_rect\"",
            "\"rect_client_px\"",
            "\"control_type_id\"",
            "\"dpi\"",
            "\"expected_visible_text\"",
            "\"Keep\"",
        ] {
            assert!(!manifest.contains(forbidden), "leaked {forbidden}");
        }
        assert!(manifest.contains("\"process_or_window_metadata_exposed\": false"));
        assert!(manifest.contains("\"pixel_coordinates_exposed\": false"));
        let review = String::from_utf8(files.review_template_bytes.clone()).unwrap();
        assert!(review.contains("\"client_only_and_unobscured_confirmed\": false"));
        assert!(review.contains("\"exact_frame_pair_identity_confirmed\": false"));
        assert!(review.contains("\"review_commitment_sha256\": \"\""));

        let mut changed_after = files.after_visible_client_png.clone();
        changed_after[0] ^= 1;
        assert_ne!(
            files.manifest.artifact_commitment_sha256,
            visible_accessibility_catalog_review_artifact_commitment_v1(
                &files.manifest,
                &files.review_template_bytes,
                &files.before_visible_client_png,
                &changed_after,
                &files.crop_files,
            )
            .unwrap()
        );
    }

    #[test]
    fn review_artifact_crop_is_exact_and_png_encoded() {
        let pixels = (0_u8..64).collect::<Vec<_>>();
        let crop = crop_visible_bgra8_v1(
            &pixels,
            &MtgoSizePxV1 {
                width: 4,
                height: 4,
            },
            &MtgoRectPxV1 {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            },
        )
        .unwrap();
        assert_eq!(crop, [&pixels[20..28], &pixels[36..44]].concat());
        let encoded = encode_visible_accessibility_png_v1(&crop, 2, 2).unwrap();
        let decoder = png::Decoder::new(encoded.as_slice());
        let mut reader = decoder.read_info().unwrap();
        let mut decoded = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut decoded).unwrap();
        assert_eq!((info.width, info.height), (2, 2));
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(info.bit_depth, png::BitDepth::Eight);
        assert_eq!(
            &decoded[..info.buffer_size()],
            &[
                crop[2], crop[1], crop[0], crop[3], crop[6], crop[5], crop[4], crop[7], crop[10],
                crop[9], crop[8], crop[11], crop[14], crop[13], crop[12], crop[15],
            ]
        );
        assert!(encode_visible_accessibility_png_v1(&crop[..15], 2, 2).is_err());
    }

    #[test]
    fn review_artifact_output_is_new_outside_repository_and_committed_atomically() {
        let files = synthetic_review_artifact_files_v1();
        let parent = unique_temp_review_output_v1("success-parent");
        fs::create_dir(&parent).unwrap();
        let output = parent.join("artifact");
        let output = validate_visible_accessibility_review_output_v1(&output).unwrap();
        persist_visible_accessibility_catalog_review_artifact_v1(&output, &files).unwrap();
        for file in [
            "manifest.json",
            "review-template.json",
            "before-visible-client.png",
            "after-visible-client.png",
            "match-00-00.png",
        ] {
            assert!(output.join(file).is_file(), "missing {file}");
        }
        assert!(validate_visible_accessibility_review_output_v1(&output).is_err());
        fs::remove_dir_all(&parent).unwrap();

        assert!(validate_visible_accessibility_review_output_v1(Path::new("relative")).is_err());
        let repository_output = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("forbidden-review-output-{}", std::process::id()));
        assert!(validate_visible_accessibility_review_output_v1(&repository_output).is_err());
    }

    #[test]
    fn failed_review_artifact_write_removes_partial_pixels() {
        let mut files = synthetic_review_artifact_files_v1();
        files.crop_files[0].0 = "missing-parent/match.png".to_owned();
        let parent = unique_temp_review_output_v1("failure-parent");
        fs::create_dir(&parent).unwrap();
        let output = parent.join("artifact");
        assert!(persist_visible_accessibility_catalog_review_artifact_v1(&output, &files).is_err());
        assert!(!output.exists());
        assert_eq!(fs::read_dir(&parent).unwrap().count(), 0);
        fs::remove_dir(&parent).unwrap();
    }

    #[test]
    fn persisted_review_artifact_reloads_only_with_exact_completed_review() {
        let files = synthetic_review_artifact_files_v1();
        let parent = unique_temp_review_output_v1("loader-success-parent");
        fs::create_dir(&parent).unwrap();
        let artifact = parent.join("artifact");
        persist_visible_accessibility_catalog_review_artifact_v1(&artifact, &files).unwrap();
        let checked = load_visible_accessibility_catalog_review_artifact_v1(&artifact).unwrap();
        let review = exact_catalog_review_v1(&checked.catalog_summary);
        let review_path = parent.join("completed-review.json");
        fs::write(&review_path, serde_json::to_vec_pretty(&review).unwrap()).unwrap();
        let evaluated =
            load_checked_untrusted_visible_accessibility_catalog_case_from_review_artifact_v1(
                &artifact,
                &review_path,
            )
            .unwrap();
        assert!(evaluated.summary_v1().exact_review_agreement);
        assert_eq!(evaluated.summary_v1().positive_entry_count, 1);
        assert!(!evaluated.production_evaluation_ratified_v1());
        assert!(!evaluated.safe_for_semantic_evidence_v1());
        assert!(!evaluated.safe_for_policy_scoring_v1());
        assert!(!evaluated.safe_for_input_v1());
        fs::remove_dir_all(&parent).unwrap();
    }

    #[test]
    fn persisted_review_artifact_loader_rejects_tamper_extra_files_and_review_drift() {
        let files = synthetic_review_artifact_files_v1();
        let parent = unique_temp_review_output_v1("loader-reject-parent");
        fs::create_dir(&parent).unwrap();
        let artifact = parent.join("artifact");
        persist_visible_accessibility_catalog_review_artifact_v1(&artifact, &files).unwrap();
        let checked = load_visible_accessibility_catalog_review_artifact_v1(&artifact).unwrap();
        let review = exact_catalog_review_v1(&checked.catalog_summary);
        let review_path = parent.join("completed-review.json");
        fs::write(&review_path, serde_json::to_vec_pretty(&review).unwrap()).unwrap();

        let crop_path = artifact.join("match-00-00.png");
        let original_crop = fs::read(&crop_path).unwrap();
        let mut changed_crop = original_crop.clone();
        let last = changed_crop.len() - 1;
        changed_crop[last] ^= 1;
        fs::write(&crop_path, changed_crop).unwrap();
        assert!(load_visible_accessibility_catalog_review_artifact_v1(&artifact).is_err());
        fs::write(&crop_path, original_crop).unwrap();

        fs::write(artifact.join("unexpected.txt"), b"visible but undeclared").unwrap();
        assert!(load_visible_accessibility_catalog_review_artifact_v1(&artifact).is_err());
        fs::remove_file(artifact.join("unexpected.txt")).unwrap();

        let mut changed_review = review;
        changed_review.entries[0].reviewed_exact_visible_match_count = 2;
        changed_review.review_commitment_sha256 =
            mtgo_visible_accessibility_catalog_review_commitment_v1(&changed_review).unwrap();
        fs::write(
            &review_path,
            serde_json::to_vec_pretty(&changed_review).unwrap(),
        )
        .unwrap();
        assert!(
            load_checked_untrusted_visible_accessibility_catalog_case_from_review_artifact_v1(
                &artifact,
                &review_path,
            )
            .is_err()
        );
        fs::remove_dir_all(&parent).unwrap();
    }

    #[test]
    fn persisted_review_crop_must_exist_at_one_stable_visible_frame_position() {
        let before = vec![1, 2, 3, 255, 4, 5, 6, 255];
        let same_position_after = vec![1, 2, 3, 255, 7, 8, 9, 255];
        let moved_after = vec![7, 8, 9, 255, 1, 2, 3, 255];
        let crop = vec![1, 2, 3, 255];
        assert!(visible_crop_exists_at_same_position_v1(
            &before,
            &same_position_after,
            2,
            1,
            &crop,
            1,
            1
        )
        .unwrap());
        let region_commitment = mtgo_blackbox_v1::visible_frame_region_content_sha256_v1(
            &before,
            &MtgoSizePxV1 {
                width: 2,
                height: 1,
            },
            &MtgoRectPxV1 {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
        )
        .unwrap();
        assert!(visible_crop_matches_region_commitment_at_same_position_v1(
            &before,
            &same_position_after,
            2,
            1,
            &crop,
            1,
            1,
            &region_commitment,
        )
        .unwrap());
        assert!(!visible_crop_matches_region_commitment_at_same_position_v1(
            &before,
            &same_position_after,
            2,
            1,
            &crop,
            1,
            1,
            &"0".repeat(64),
        )
        .unwrap());
        assert!(
            !visible_crop_exists_at_same_position_v1(&before, &moved_after, 2, 1, &crop, 1, 1)
                .unwrap()
        );
        assert!(!visible_crop_exists_at_same_position_v1(
            &before,
            &same_position_after,
            2,
            1,
            &[10, 11, 12, 255],
            1,
            1
        )
        .unwrap());
    }

    #[test]
    fn catalog_corpus_cli_accepts_only_bounded_complete_case_pairs() {
        let parsed = parse_catalog_corpus_arguments_v1([
            OsString::from("--case"),
            OsString::from(r"C:\review-artifact"),
            OsString::from(r"C:\review.json"),
        ])
        .unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].0, PathBuf::from(r"C:\review-artifact"));
        assert_eq!(parsed[0].1, PathBuf::from(r"C:\review.json"));
        assert!(parse_catalog_corpus_arguments_v1(Vec::<OsString>::new()).is_err());
        assert!(parse_catalog_corpus_arguments_v1([OsString::from("--case")]).is_err());
        assert!(parse_catalog_corpus_arguments_v1([
            OsString::from("--artifact"),
            OsString::from(r"C:\review-artifact"),
            OsString::from(r"C:\review.json"),
        ])
        .is_err());
    }

    #[test]
    fn review_finalizer_hashes_alias_commits_review_and_never_ratifies() {
        let files = synthetic_review_artifact_files_v1();
        let parent = unique_temp_review_output_v1("finalizer-success-parent");
        fs::create_dir(&parent).unwrap();
        let artifact = parent.join("artifact");
        persist_visible_accessibility_catalog_review_artifact_v1(&artifact, &files).unwrap();
        let loaded = load_visible_accessibility_catalog_review_artifact_v1(&artifact).unwrap();
        let mut edited_review = exact_catalog_review_v1(&loaded.catalog_summary);
        edited_review.reviewer_alias_sha256.clear();
        edited_review.review_commitment_sha256.clear();
        let edited_review_path = parent.join("edited-review.json");
        let reviewer_alias_path = parent.join("reviewer-alias.txt");
        let output = parent.join("completed-review.json");
        fs::write(
            &edited_review_path,
            serde_json::to_vec_pretty(&edited_review).unwrap(),
        )
        .unwrap();
        fs::write(&reviewer_alias_path, b"reviewer-one\n").unwrap();

        let receipt = finalize_visible_accessibility_catalog_review_artifact_v1(
            &artifact,
            &edited_review_path,
            &reviewer_alias_path,
            &output,
        )
        .unwrap();
        assert!(receipt.completed_review_written);
        assert!(!receipt.production_evaluation_ratified);
        assert!(!receipt.safe_for_semantic_evidence);
        assert!(!receipt.safe_for_policy_scoring);
        assert!(!receipt.safe_for_input);
        let completed: MtgoVisibleAccessibilityCatalogReviewV1 =
            serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
        assert_eq!(
            completed.reviewer_alias_sha256,
            sha256_hex_v1(b"reviewer-one")
        );
        assert_eq!(
            completed.review_commitment_sha256,
            mtgo_visible_accessibility_catalog_review_commitment_v1(&completed).unwrap()
        );
        assert!(
            load_checked_untrusted_visible_accessibility_catalog_case_from_review_artifact_v1(
                &artifact, &output
            )
            .is_ok()
        );
        fs::remove_dir_all(&parent).unwrap();
    }

    #[test]
    fn review_finalizer_rejects_incomplete_review_alias_and_source_mutation() {
        let files = synthetic_review_artifact_files_v1();
        let parent = unique_temp_review_output_v1("finalizer-reject-parent");
        fs::create_dir(&parent).unwrap();
        let artifact = parent.join("artifact");
        persist_visible_accessibility_catalog_review_artifact_v1(&artifact, &files).unwrap();
        let loaded = load_visible_accessibility_catalog_review_artifact_v1(&artifact).unwrap();
        let mut edited_review = exact_catalog_review_v1(&loaded.catalog_summary);
        edited_review.reviewer_alias_sha256.clear();
        edited_review.review_commitment_sha256.clear();
        edited_review.client_only_and_unobscured_confirmed = false;
        let edited_review_path = parent.join("edited-review.json");
        let reviewer_alias_path = parent.join("reviewer-alias.txt");
        let output = parent.join("completed-review.json");
        fs::write(
            &edited_review_path,
            serde_json::to_vec_pretty(&edited_review).unwrap(),
        )
        .unwrap();
        fs::write(&reviewer_alias_path, b"reviewer-one").unwrap();
        assert!(finalize_visible_accessibility_catalog_review_artifact_v1(
            &artifact,
            &edited_review_path,
            &reviewer_alias_path,
            &output
        )
        .is_err());
        assert!(!output.exists());

        edited_review.client_only_and_unobscured_confirmed = true;
        fs::write(
            &edited_review_path,
            serde_json::to_vec_pretty(&edited_review).unwrap(),
        )
        .unwrap();
        fs::write(&reviewer_alias_path, b"reviewer-one\n\n").unwrap();
        assert!(finalize_visible_accessibility_catalog_review_artifact_v1(
            &artifact,
            &edited_review_path,
            &reviewer_alias_path,
            &output
        )
        .is_err());
        assert!(!output.exists());

        fs::write(&reviewer_alias_path, b"reviewer-one").unwrap();
        let inside_artifact = artifact.join("completed-review.json");
        assert!(finalize_visible_accessibility_catalog_review_artifact_v1(
            &artifact,
            &edited_review_path,
            &reviewer_alias_path,
            &inside_artifact
        )
        .is_err());
        assert!(!inside_artifact.exists());
        fs::remove_dir_all(&parent).unwrap();
    }

    #[test]
    fn review_finalization_cli_requires_each_exact_path_once() {
        let parsed = parse_catalog_review_finalization_arguments_v1([
            OsString::from("--artifact"),
            OsString::from(r"C:\artifact"),
            OsString::from("--edited-review"),
            OsString::from(r"C:\edited.json"),
            OsString::from("--reviewer-alias"),
            OsString::from(r"C:\alias.txt"),
            OsString::from("--output"),
            OsString::from(r"C:\completed.json"),
        ])
        .unwrap();
        assert_eq!(parsed.0, PathBuf::from(r"C:\artifact"));
        assert_eq!(parsed.3, PathBuf::from(r"C:\completed.json"));
        assert!(parse_catalog_review_finalization_arguments_v1(Vec::<OsString>::new()).is_err());
        assert!(parse_catalog_review_finalization_arguments_v1([
            OsString::from("--artifact"),
            OsString::from(r"C:\artifact"),
            OsString::from("--artifact"),
            OsString::from(r"C:\other"),
        ])
        .is_err());
    }
}
