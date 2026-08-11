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

const MAX_VISIBLE_ACCESSIBILITY_QUERIES_V1: usize = 64;
const MAX_VISIBLE_ACCESSIBILITY_ELEMENTS_V1: i32 = 4_096;
const MAX_MATCHES_PER_QUERY_V1: usize = 64;
const VISIBLE_ACCESSIBILITY_WINDOW_DOMAIN_V1: &[u8] = b"mtgo-visible-accessibility-window-v1";
const VISIBLE_ACCESSIBILITY_MATCH_SET_DOMAIN_V1: &[u8] = b"mtgo-visible-accessibility-match-set-v1";
const VISIBLE_ACCESSIBILITY_REPORT_DOMAIN_V1: &[u8] = b"mtgo-visible-accessibility-report-v1";

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PrivateVisibleAccessibilityMatchV1 {
    query_index: usize,
    control_type_id: i32,
    rect_desktop_px: SignedRectV1,
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
        summary,
    })
}

pub fn run_visible_accessibility_probe_cli_v1(
) -> Result<MtgoVisibleAccessibilityProbeSummaryV1, String> {
    let (window_request, queries) = parse_cli_v1()?;
    Ok(probe_mtgo_visible_accessibility_exact_text_v1(window_request, queries)?.summary_v1())
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
}
