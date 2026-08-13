use super::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
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

const MAX_VISIBLE_ACCESSIBILITY_QUERIES_V1: usize = 64;
const MAX_VISIBLE_ACCESSIBILITY_ELEMENTS_V1: i32 = 4_096;
const MAX_MATCHES_PER_QUERY_V1: usize = 64;
const MAX_VISIBLE_ACCESSIBILITY_CAPTURE_BRACKET_MILLIS_V1: u128 = 5_000;
const VISIBLE_ACCESSIBILITY_WINDOW_DOMAIN_V1: &[u8] = b"mtgo-visible-accessibility-window-v1";
const VISIBLE_ACCESSIBILITY_MATCH_SET_DOMAIN_V1: &[u8] = b"mtgo-visible-accessibility-match-set-v1";
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
    pub observed_control_type_ids: Vec<i32>,
    pub private_match_set_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityCatalogProbeSummaryV1 {
    pub schema_version: u32,
    pub catalog_commitment_sha256: String,
    pub source_probe_commitment_sha256: String,
    pub source_window_identity_commitment_sha256: String,
    pub report_commitment_sha256: String,
    pub eligible_visible_element_count: u32,
    pub visible_named_element_count: u32,
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
    pub observed_control_type_ids: Vec<i32>,
    pub private_pixel_match_set_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityPixelCatalogProbeSummaryV1 {
    pub schema_version: u32,
    pub catalog_commitment_sha256: String,
    pub source_pixel_report_commitment_sha256: String,
    pub source_window_identity_commitment_sha256: String,
    pub before_capture_commitment_sha256: String,
    pub after_capture_commitment_sha256: String,
    pub before_frame_sha256: String,
    pub after_frame_sha256: String,
    pub entries: Vec<MtgoVisibleAccessibilityPixelCatalogEntrySummaryV1>,
    pub total_pixel_corroborated_match_count: u32,
    pub has_pixel_corroborated_match: bool,
    pub capture_bracket_identity_confirmed: bool,
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
    pub reviewed_control_type_ids: Vec<i32>,
    pub reviewed_private_pixel_match_set_commitment_sha256: String,
    pub every_matched_region_visibly_contains_exact_catalog_label: bool,
    pub visible_absence_reviewed_when_match_count_is_zero: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityCatalogReviewV1 {
    pub schema_version: u32,
    pub catalog_commitment_sha256: String,
    pub source_pixel_report_commitment_sha256: String,
    pub source_window_identity_commitment_sha256: String,
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
    pub source_window_identity_commitment_sha256: String,
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

/// Move-only evaluation of one exact fixed-catalog pixel bracket. It retains
/// the opaque source so application code cannot replace a real captured case
/// with copied report hashes. The review is caller-authored and the result is
/// therefore only a non-authorizing ratification candidate.
pub struct CheckedUntrustedMtgoVisibleAccessibilityCatalogCaseEvaluationV1 {
    _source: OpaqueMtgoVisibleAccessibilityPixelCorroborationV1,
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
    pub observed_control_type_ids: Vec<i32>,
    pub private_match_set_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityProbeSummaryV1 {
    pub schema_version: u32,
    pub source_window_identity_commitment_sha256: String,
    pub process_id: u32,
    pub window_handle: u64,
    pub client_rect_desktop_sha256: String,
    pub dpi: u32,
    pub eligible_visible_element_count: u32,
    pub visible_named_element_count: u32,
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
    pub observed_control_type_ids: Vec<i32>,
    pub private_pixel_match_set_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleAccessibilityPixelCorroborationSummaryV1 {
    pub schema_version: u32,
    pub before_capture_commitment_sha256: String,
    pub after_capture_commitment_sha256: String,
    pub before_frame_sha256: String,
    pub after_frame_sha256: String,
    pub accessibility_report_commitment_sha256: String,
    pub source_window_identity_commitment_sha256: String,
    pub query_results: Vec<MtgoVisibleAccessibilityPixelQueryResultV1>,
    pub total_pixel_corroborated_match_count: u32,
    pub has_pixel_corroborated_match: bool,
    pub capture_bracket_identity_confirmed: bool,
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
/// corroborates the same private rectangles.
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
    require_admitted_window(&pre)?;

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

    let mut eligible_visible_element_count = 0_u32;
    let mut visible_named_element_count = 0_u32;
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
        eligible_visible_element_count = eligible_visible_element_count
            .checked_add(1)
            .ok_or("visible accessibility eligible element count overflow")?;

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
        visible_named_element_count = visible_named_element_count
            .checked_add(1)
            .ok_or("visible accessibility named element count overflow")?;
        for (query_index, query) in queries.iter().enumerate() {
            if name == query.expected_visible_text {
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
    require_admitted_window(&post)?;
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

    let source_window_identity_commitment_sha256 = commitment_v1(
        VISIBLE_ACCESSIBILITY_WINDOW_DOMAIN_V1,
        &[&serde_json::to_vec(&pre)
            .map_err(|error| format!("serialize accessibility window identity: {error}"))?],
    );
    let client_rect_desktop_sha256 = sha256_hex_v1(
        &serde_json::to_vec(&pre.client_rect_desktop_px)
            .map_err(|error| format!("serialize accessibility client bounds: {error}"))?,
    );
    let query_results = build_query_results_v1(&queries, &private_matches)?;
    let mut summary = MtgoVisibleAccessibilityProbeSummaryV1 {
        schema_version: MTGO_VISIBLE_ACCESSIBILITY_PROBE_SCHEMA_V1,
        source_window_identity_commitment_sha256,
        process_id: pre.process_id,
        window_handle: pre.hwnd,
        client_rect_desktop_sha256,
        dpi: pre.dpi,
        eligible_visible_element_count,
        visible_named_element_count,
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
    if accessibility_probe.summary.process_id != before.pre.process_id
        || accessibility_probe.summary.window_handle != before.pre.hwnd
        || accessibility_probe.summary.dpi != before.pre.dpi
        || accessibility_probe.summary.client_rect_desktop_sha256
            != sha256_hex_v1(
                &serde_json::to_vec(&before.pre.client_rect_desktop_px).map_err(|error| {
                    format!("serialize corroborated accessibility client bounds: {error}")
                })?,
            )
        || accessibility_probe.summary.report_commitment_sha256
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
            || plain.observed_control_type_ids != pixel.observed_control_type_ids
        {
            return Err("pixel corroboration changed the accessibility match inventory".to_owned());
        }
    }

    let mut summary = MtgoVisibleAccessibilityPixelCorroborationSummaryV1 {
        schema_version: MTGO_VISIBLE_ACCESSIBILITY_PIXEL_CORROBORATION_SCHEMA_V1,
        before_capture_commitment_sha256: before_frame.capture_commitment_sha256.clone(),
        after_capture_commitment_sha256: after_frame.capture_commitment_sha256.clone(),
        before_frame_sha256: before.frame.canonical_bgra8_sha256.clone(),
        after_frame_sha256: after.frame.canonical_bgra8_sha256.clone(),
        accessibility_report_commitment_sha256: accessibility_probe
            .summary
            .report_commitment_sha256
            .clone(),
        source_window_identity_commitment_sha256: accessibility_probe
            .summary
            .source_window_identity_commitment_sha256
            .clone(),
        query_results,
        total_pixel_corroborated_match_count,
        has_pixel_corroborated_match: total_pixel_corroborated_match_count != 0,
        capture_bracket_identity_confirmed: true,
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
                observed_control_type_ids: result.observed_control_type_ids.clone(),
                private_match_set_commitment_sha256: result
                    .private_match_set_commitment_sha256
                    .clone(),
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
        &source.source_window_identity_commitment_sha256,
        source.eligible_visible_element_count,
        source.visible_named_element_count,
        catalog_entry_count,
        matched_catalog_entry_count,
        total_exact_visible_match_count,
        &entries,
    )?;
    Ok(MtgoVisibleAccessibilityCatalogProbeSummaryV1 {
        schema_version: MTGO_VISIBLE_ACCESSIBILITY_CATALOG_SCHEMA_V1,
        catalog_commitment_sha256,
        source_probe_commitment_sha256: source.report_commitment_sha256,
        source_window_identity_commitment_sha256: source.source_window_identity_commitment_sha256,
        report_commitment_sha256,
        eligible_visible_element_count: source.eligible_visible_element_count,
        visible_named_element_count: source.visible_named_element_count,
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

pub fn run_visible_accessibility_known_label_catalog_pixel_corroboration_cli_v1(
) -> Result<MtgoVisibleAccessibilityPixelCatalogProbeSummaryV1, String> {
    let window_request = parse_catalog_cli_v1()?;
    probe_mtgo_visible_accessibility_known_label_catalog_with_pixel_corroboration_v1(window_request)
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
    source_window_identity_commitment_sha256: &str,
    eligible_visible_element_count: u32,
    visible_named_element_count: u32,
    catalog_entry_count: u32,
    matched_catalog_entry_count: u32,
    total_exact_visible_match_count: u32,
    entries: &[MtgoVisibleAccessibilityCatalogEntrySummaryV1],
) -> Result<String, String> {
    let counts = [
        eligible_visible_element_count,
        visible_named_element_count,
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
            source_window_identity_commitment_sha256.as_bytes(),
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
        || !source.capture_bracket_identity_confirmed
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
                observed_control_type_ids: result.observed_control_type_ids.clone(),
                private_pixel_match_set_commitment_sha256: result
                    .private_pixel_match_set_commitment_sha256
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
        source_window_identity_commitment_sha256: source.source_window_identity_commitment_sha256,
        before_capture_commitment_sha256: source.before_capture_commitment_sha256,
        after_capture_commitment_sha256: source.after_capture_commitment_sha256,
        before_frame_sha256: source.before_frame_sha256,
        after_frame_sha256: source.after_frame_sha256,
        entries,
        total_pixel_corroborated_match_count: total,
        has_pixel_corroborated_match: total != 0,
        capture_bracket_identity_confirmed: true,
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
            _source: source,
            _review: review,
            summary,
        },
    )
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
        source_window_identity_commitment_sha256: catalog_summary
            .source_window_identity_commitment_sha256
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
        // catalog. A future corpus evaluator must own any production root.
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
        || review.source_window_identity_commitment_sha256
            != source.source_window_identity_commitment_sha256
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
            || observed.reviewed_control_type_ids != expected.observed_control_type_ids
            || observed.reviewed_private_pixel_match_set_commitment_sha256
                != expected.private_pixel_match_set_commitment_sha256
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
            let matched_json = serde_json::to_vec(&matched)
                .map_err(|error| format!("serialize private accessibility match set: {error}"))?;
            let expected_visible_text_sha256 =
                sha256_hex_v1(query.expected_visible_text.as_bytes());
            let mut observed_control_type_ids = matched
                .iter()
                .map(|matched| matched.control_type_id)
                .collect::<Vec<_>>();
            observed_control_type_ids.sort_unstable();
            observed_control_type_ids.dedup();
            Ok(MtgoVisibleAccessibilityQueryResultV1 {
                query_id: query.query_id.clone(),
                expected_visible_text_sha256: expected_visible_text_sha256.clone(),
                exact_visible_match_count: u32::try_from(matched.len())
                    .map_err(|_| "visible accessibility match count overflow")?,
                observed_control_type_ids,
                private_match_set_commitment_sha256: commitment_v1(
                    VISIBLE_ACCESSIBILITY_MATCH_SET_DOMAIN_V1,
                    &[expected_visible_text_sha256.as_bytes(), &matched_json],
                ),
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
            let mut observed_control_type_ids = matched
                .iter()
                .map(|matched| matched.control_type_id)
                .collect::<Vec<_>>();
            observed_control_type_ids.sort_unstable();
            observed_control_type_ids.dedup();
            let expected_visible_text_sha256 =
                sha256_hex_v1(query.expected_visible_text.as_bytes());
            let match_bytes = serde_json::to_vec(&matched)
                .map_err(|error| format!("serialize private pixel match set: {error}"))?;
            Ok(MtgoVisibleAccessibilityPixelQueryResultV1 {
                query_id: query.query_id.clone(),
                expected_visible_text_sha256: expected_visible_text_sha256.clone(),
                exact_visible_match_count: u32::try_from(matched.len())
                    .map_err(|_| "visible accessibility pixel match count overflow")?,
                observed_control_type_ids,
                private_pixel_match_set_commitment_sha256: commitment_v1(
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
    let mut args = std::env::args().skip(1);
    let mut expected_executable_sha256 = None;
    let mut expected_signer_thumbprint = None;
    let mut expected_signer_subject_sha256 = None;
    let mut window_mode = CaptureWindowModeV2::DuelGame;
    let mut expected_game_format = None;
    let mut expected_title_contains = None;
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
            _ => return Err(format!("unknown catalog argument {argument}")),
        }
    }
    Ok(MtgoDxgiCaptureRequestV3 {
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
    })
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
                    observed_control_type_ids: if index == 0 { vec![50_000] } else { Vec::new() },
                    private_pixel_match_set_commitment_sha256: format!("{index:064x}"),
                },
            )
            .collect();
        let mut source = MtgoVisibleAccessibilityPixelCorroborationSummaryV1 {
            schema_version: MTGO_VISIBLE_ACCESSIBILITY_PIXEL_CORROBORATION_SCHEMA_V1,
            before_capture_commitment_sha256: "b".repeat(64),
            after_capture_commitment_sha256: "c".repeat(64),
            before_frame_sha256: "d".repeat(64),
            after_frame_sha256: "e".repeat(64),
            accessibility_report_commitment_sha256: "f".repeat(64),
            source_window_identity_commitment_sha256: "1".repeat(64),
            query_results,
            total_pixel_corroborated_match_count: 1,
            has_pixel_corroborated_match: true,
            capture_bracket_identity_confirmed: true,
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
                reviewed_control_type_ids: entry.observed_control_type_ids.clone(),
                reviewed_private_pixel_match_set_commitment_sha256: entry
                    .private_pixel_match_set_commitment_sha256
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
            source_window_identity_commitment_sha256: source
                .source_window_identity_commitment_sha256
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
        assert_eq!(results[0].observed_control_type_ids, vec![50_007]);
        let json = serde_json::to_string(&results).unwrap();
        assert!(!json.contains("Premodern"));
        assert!(!json.contains("left"));
        assert!(!json.contains("top"));
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
                observed_control_type_ids: Vec::new(),
                private_match_set_commitment_sha256: "a".repeat(64),
            })
            .collect::<Vec<_>>();
        let summary = MtgoVisibleAccessibilityCatalogProbeSummaryV1 {
            schema_version: MTGO_VISIBLE_ACCESSIBILITY_CATALOG_SCHEMA_V1,
            catalog_commitment_sha256: commitment,
            source_probe_commitment_sha256: "b".repeat(64),
            source_window_identity_commitment_sha256: "c".repeat(64),
            report_commitment_sha256: "d".repeat(64),
            eligible_visible_element_count: 0,
            visible_named_element_count: 0,
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
                observed_control_type_ids: Vec::new(),
                private_pixel_match_set_commitment_sha256: "a".repeat(64),
            })
            .collect();
        let mut source = MtgoVisibleAccessibilityPixelCorroborationSummaryV1 {
            schema_version: MTGO_VISIBLE_ACCESSIBILITY_PIXEL_CORROBORATION_SCHEMA_V1,
            before_capture_commitment_sha256: "b".repeat(64),
            after_capture_commitment_sha256: "c".repeat(64),
            before_frame_sha256: "d".repeat(64),
            after_frame_sha256: "e".repeat(64),
            accessibility_report_commitment_sha256: "f".repeat(64),
            source_window_identity_commitment_sha256: "1".repeat(64),
            query_results,
            total_pixel_corroborated_match_count: 0,
            has_pixel_corroborated_match: false,
            capture_bracket_identity_confirmed: true,
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
        let mut changed_type = review.clone();
        changed_type.entries[0].reviewed_control_type_ids[0] += 1;
        mutations.push(changed_type);
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
    fn match_set_commitment_changes_with_private_bounds_or_control_type() {
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

        let mut moved = baseline.clone();
        moved[0].rect_desktop_px.left += 1;
        assert_ne!(
            expected[0].private_match_set_commitment_sha256,
            build_query_results_v1(&queries, &moved).unwrap()[0]
                .private_match_set_commitment_sha256
        );

        let mut changed_type = baseline;
        changed_type[0].control_type_id += 1;
        assert_ne!(
            expected[0].private_match_set_commitment_sha256,
            build_query_results_v1(&queries, &changed_type).unwrap()[0]
                .private_match_set_commitment_sha256
        );
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
    fn pixel_match_commitment_binds_pixels_bounds_control_and_query_text() {
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
        for changed in [
            {
                let mut value = baseline.clone();
                value[0].region_bgra8_sha256 = "b".repeat(64);
                value
            },
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
            assert_ne!(
                expected[0].private_pixel_match_set_commitment_sha256,
                build_pixel_query_results_v1(&queries, &changed).unwrap()[0]
                    .private_pixel_match_set_commitment_sha256
            );
        }
        let mut changed_queries = queries;
        changed_queries[0].expected_visible_text.push('!');
        assert_ne!(
            expected[0].private_pixel_match_set_commitment_sha256,
            build_pixel_query_results_v1(&changed_queries, &baseline).unwrap()[0]
                .private_pixel_match_set_commitment_sha256
        );
    }
}
