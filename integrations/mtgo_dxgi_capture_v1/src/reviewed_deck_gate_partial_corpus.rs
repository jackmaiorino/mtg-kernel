use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const MTGO_REVIEWED_DECK_GATE_PARTIAL_CORPUS_QUALIFICATION_SCHEMA_V1: u32 = 1;
pub const MTGO_REVIEWED_DECK_GATE_PARTIAL_CORPUS_SHA256_V1: &str =
    "0b3f21b49f78f607ca9fe34918dae66993124eaabf82263d68d990fab45360a1";

const BUILT_IN_RECEIPT_BYTES_V1: &[u8] =
    include_bytes!("../assets/reviewed_modern_deck_gate_partial_corpus_v1.json");
const EXPECTED_SCOPE_V1: &str = "reviewed_modern_league_challenge_deck_gate_partial_corpus_v1";
const EXPECTED_REVIEW_METHOD_V1: &str = "visible_pixel_review_plus_exact_external_frame_rehash_v1";
const EXPECTED_PRODUCT_VERSION_V1: &str = "3.4.158.4691";
const EXPECTED_PIXEL_FORMAT_V1: &str = "bgra8_unorm_top_down_tightly_packed_v1";
const EXPECTED_DECK_LABEL_V1: &str = "mtgo-kernel-modern-basics-v1";
const AWAITING_STATE_V1: &str = "awaiting_compatible_deck_selection";
const SELECTED_STATE_V1: &str = "compatible_deck_selected";
const OPEN_REVIEW_STATE_V1: &str = "open_entry_review_available";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoReviewedDeckGatePartialCorpusQualificationV1 {
    pub schema_version: u32,
    pub purpose: String,
    pub receipt_sha256: String,
    pub reviewed_source_count: usize,
    pub partial_profile_count: usize,
    pub league_safe_state_count: usize,
    pub challenge_safe_state_count: usize,
    pub open_entry_review_state_present: bool,
    pub passes_three_state_gate: bool,
    pub safe_for_offline_partial_classifier_reference_generation: bool,
    pub safe_for_live_classification: bool,
    pub grants_semantic_evidence: bool,
    pub grants_policy_scoring: bool,
    pub grants_input: bool,
    pub grants_open_entry_review: bool,
    pub grants_event_entry: bool,
    pub grants_spending: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptV1 {
    schema_version: u32,
    scope: String,
    review_id: String,
    reviewed_at_utc: String,
    review_method: String,
    client_identity: ClientIdentityV1,
    region_geometry: RegionGeometryV1,
    reference_hashes: ReferenceHashesV1,
    reviewed_sources: Vec<ReviewedSourceV1>,
    partial_reference_profiles: Vec<PartialProfileV1>,
    coverage: Vec<CoverageV1>,
    passes_three_state_gate: bool,
    review_confirmations: ReviewConfirmationsV1,
    authority: AuthorityV1,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClientIdentityV1 {
    product_version: String,
    executable_sha256: String,
    signer_thumbprint: String,
    dpi: u32,
    client_size_px: SizeV1,
    canonical_pixel_format: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SizeV1 {
    width: u32,
    height: u32,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RectV1 {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegionGeometryV1 {
    event_label_search_rect_client_px: RectV1,
    deck_status_search_rect_client_px: RectV1,
    deck_label_region_rect_client_px: RectV1,
    select_deck_control_rect_client_px: RectV1,
    open_entry_review_status_rect_client_px: RectV1,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceHashesV1 {
    selected_deck_label_region_sha256: String,
    enabled_select_deck_control_region_sha256: String,
    open_entry_review_unavailable_region_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewedSourceV1 {
    source_id: String,
    event_kind: String,
    state: String,
    source_manifest_sha256: String,
    source_png_sha256: String,
    canonical_bgra8_sha256: String,
    pending_visual_review_capture: bool,
    safe_for_semantic_evidence: bool,
    safe_for_policy_scoring: bool,
    safe_for_input: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialProfileV1 {
    profile_id: String,
    event_kind: String,
    event_display_label_sha256: String,
    expected_visible_event_label: String,
    deck_display_label_sha256: String,
    expected_visible_deck_label: String,
    state: String,
    selected_deck_label_reference_sha256s: Vec<String>,
    select_deck_control_reference_sha256s: Vec<String>,
    open_entry_review_unavailable_reference_sha256s: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CoverageV1 {
    event_kind: String,
    reviewed_states: Vec<String>,
    missing_states: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewConfirmationsV1 {
    all_sources_player_visible_composed_pixels_only: bool,
    all_sources_unobscured_and_cursor_absent: bool,
    event_kind_and_visible_label_confirmed: bool,
    missing_deck_prompt_confirmed_for_awaiting_sources: bool,
    exact_selected_deck_confirmed_for_selected_sources: bool,
    entry_choices_remained_unselected: bool,
    open_entry_review_absent_from_every_source: bool,
    formal_selected_listing_case_claimed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorityV1 {
    safe_for_partial_classifier_reference_generation: bool,
    grants_live_classification: bool,
    grants_semantic_evidence: bool,
    grants_policy_scoring: bool,
    grants_input: bool,
    grants_open_entry_review: bool,
    grants_event_entry: bool,
    grants_spending: bool,
}

pub fn check_built_in_reviewed_deck_gate_partial_corpus_v1(
) -> Result<MtgoReviewedDeckGatePartialCorpusQualificationV1, String> {
    let actual_sha256 = sha256_hex_v1(BUILT_IN_RECEIPT_BYTES_V1);
    if actual_sha256 != MTGO_REVIEWED_DECK_GATE_PARTIAL_CORPUS_SHA256_V1 {
        return Err("built-in partial deck-gate receipt hash differs".to_owned());
    }
    check_untrusted_reviewed_deck_gate_partial_corpus_v1(BUILT_IN_RECEIPT_BYTES_V1)
}

/// Structurally checks an untrusted, non-authorizing partial corpus receipt.
///
/// Success permits only offline reference generation. It is not a live-frame
/// admission, semantic classifier, scorer, input authority, or entry authority.
pub fn check_untrusted_reviewed_deck_gate_partial_corpus_v1(
    receipt_bytes: &[u8],
) -> Result<MtgoReviewedDeckGatePartialCorpusQualificationV1, String> {
    let receipt: ReceiptV1 = serde_json::from_slice(receipt_bytes)
        .map_err(|error| format!("parse partial deck-gate receipt: {error}"))?;
    validate_receipt_identity_v1(&receipt)?;
    validate_geometry_v1(&receipt)?;
    validate_sources_v1(&receipt)?;
    let profile_states = validate_profiles_v1(&receipt)?;
    validate_coverage_v1(&receipt, &profile_states)?;
    validate_review_and_authority_v1(&receipt)?;

    Ok(MtgoReviewedDeckGatePartialCorpusQualificationV1 {
        schema_version: MTGO_REVIEWED_DECK_GATE_PARTIAL_CORPUS_QUALIFICATION_SCHEMA_V1,
        purpose: "checked_untrusted_non_authorizing_cross_mode_deck_gate_partial_corpus_v1"
            .to_owned(),
        receipt_sha256: sha256_hex_v1(receipt_bytes),
        reviewed_source_count: receipt.reviewed_sources.len(),
        partial_profile_count: receipt.partial_reference_profiles.len(),
        league_safe_state_count: profile_states.get("league").map_or(0, BTreeSet::len),
        challenge_safe_state_count: profile_states.get("challenge").map_or(0, BTreeSet::len),
        open_entry_review_state_present: false,
        passes_three_state_gate: false,
        safe_for_offline_partial_classifier_reference_generation: true,
        safe_for_live_classification: false,
        grants_semantic_evidence: false,
        grants_policy_scoring: false,
        grants_input: false,
        grants_open_entry_review: false,
        grants_event_entry: false,
        grants_spending: false,
    })
}

fn validate_receipt_identity_v1(receipt: &ReceiptV1) -> Result<(), String> {
    if receipt.schema_version != 1
        || receipt.scope != EXPECTED_SCOPE_V1
        || receipt.review_id.is_empty()
        || !valid_utc_second_v1(&receipt.reviewed_at_utc)
        || receipt.review_method != EXPECTED_REVIEW_METHOD_V1
        || receipt.client_identity.product_version != EXPECTED_PRODUCT_VERSION_V1
        || !is_lower_sha256_v1(&receipt.client_identity.executable_sha256)
        || !is_lower_hex_v1(&receipt.client_identity.signer_thumbprint, 40)
        || receipt.client_identity.dpi != 120
        || receipt.client_identity.client_size_px
            != (SizeV1 {
                width: 1_550,
                height: 925,
            })
        || receipt.client_identity.canonical_pixel_format != EXPECTED_PIXEL_FORMAT_V1
    {
        return Err("partial deck-gate receipt identity differs".to_owned());
    }
    Ok(())
}

fn validate_geometry_v1(receipt: &ReceiptV1) -> Result<(), String> {
    let geometry = &receipt.region_geometry;
    let size = &receipt.client_identity.client_size_px;
    let expected = [
        (
            &geometry.event_label_search_rect_client_px,
            RectV1 {
                x: 500,
                y: 110,
                width: 415,
                height: 100,
            },
        ),
        (
            &geometry.deck_status_search_rect_client_px,
            RectV1 {
                x: 590,
                y: 420,
                width: 320,
                height: 100,
            },
        ),
        (
            &geometry.deck_label_region_rect_client_px,
            RectV1 {
                x: 670,
                y: 435,
                width: 210,
                height: 75,
            },
        ),
        (
            &geometry.select_deck_control_rect_client_px,
            RectV1 {
                x: 510,
                y: 425,
                width: 75,
                height: 90,
            },
        ),
        (
            &geometry.open_entry_review_status_rect_client_px,
            RectV1 {
                x: 800,
                y: 850,
                width: 90,
                height: 50,
            },
        ),
    ];
    if expected
        .iter()
        .any(|(actual, expected)| !rect_is_valid_v1(actual, size) || *actual != expected)
        || !rect_contains_v1(
            &geometry.deck_status_search_rect_client_px,
            &geometry.deck_label_region_rect_client_px,
        )
    {
        return Err("partial deck-gate geometry differs".to_owned());
    }
    let disjoint = [
        &geometry.event_label_search_rect_client_px,
        &geometry.deck_status_search_rect_client_px,
        &geometry.select_deck_control_rect_client_px,
        &geometry.open_entry_review_status_rect_client_px,
    ];
    for first in 0..disjoint.len() {
        for second in (first + 1)..disjoint.len() {
            if rects_overlap_v1(disjoint[first], disjoint[second]) {
                return Err("partial deck-gate semantic regions overlap".to_owned());
            }
        }
    }
    for digest in [
        &receipt.reference_hashes.selected_deck_label_region_sha256,
        &receipt
            .reference_hashes
            .enabled_select_deck_control_region_sha256,
        &receipt
            .reference_hashes
            .open_entry_review_unavailable_region_sha256,
    ] {
        if !is_lower_sha256_v1(digest) {
            return Err("partial deck-gate reference hash is invalid".to_owned());
        }
    }
    Ok(())
}

fn validate_sources_v1(receipt: &ReceiptV1) -> Result<(), String> {
    if receipt.reviewed_sources.len() != 5 {
        return Err("partial deck-gate receipt requires five source records".to_owned());
    }
    let mut source_ids = BTreeSet::new();
    let mut manifests = BTreeSet::new();
    let mut pngs = BTreeSet::new();
    let mut pixels = BTreeSet::new();
    let mut slices = BTreeMap::<(&str, &str), usize>::new();
    for source in &receipt.reviewed_sources {
        if !source_ids.insert(source.source_id.as_str())
            || !manifests.insert(source.source_manifest_sha256.as_str())
            || !pngs.insert(source.source_png_sha256.as_str())
            || !pixels.insert(source.canonical_bgra8_sha256.as_str())
            || !matches!(source.event_kind.as_str(), "league" | "challenge")
            || !matches!(source.state.as_str(), AWAITING_STATE_V1 | SELECTED_STATE_V1)
            || !is_lower_sha256_v1(&source.source_manifest_sha256)
            || !is_lower_sha256_v1(&source.source_png_sha256)
            || !is_lower_sha256_v1(&source.canonical_bgra8_sha256)
            || !source.pending_visual_review_capture
            || source.safe_for_semantic_evidence
            || source.safe_for_policy_scoring
            || source.safe_for_input
        {
            return Err("partial deck-gate source record is invalid or authoritative".to_owned());
        }
        *slices
            .entry((source.event_kind.as_str(), source.state.as_str()))
            .or_default() += 1;
    }
    let expected = BTreeMap::from([
        (("challenge", AWAITING_STATE_V1), 1usize),
        (("challenge", SELECTED_STATE_V1), 1usize),
        (("league", AWAITING_STATE_V1), 1usize),
        (("league", SELECTED_STATE_V1), 2usize),
    ]);
    if slices != expected {
        return Err("partial deck-gate reviewed source coverage differs".to_owned());
    }
    Ok(())
}

fn validate_profiles_v1<'a>(
    receipt: &'a ReceiptV1,
) -> Result<BTreeMap<&'a str, BTreeSet<&'a str>>, String> {
    if receipt.partial_reference_profiles.len() != 4 {
        return Err("partial deck-gate receipt requires four reference profiles".to_owned());
    }
    let source_ids = receipt
        .reviewed_sources
        .iter()
        .map(|source| source.source_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut previous_id: Option<&str> = None;
    let mut profile_ids = BTreeSet::new();
    let mut states = BTreeMap::<&str, BTreeSet<&str>>::new();
    for profile in &receipt.partial_reference_profiles {
        let expected_event_label = match profile.event_kind.as_str() {
            "league" => "Modern League",
            "challenge" => "Modern Challenge 96",
            _ => return Err("partial deck-gate profile event kind is invalid".to_owned()),
        };
        if profile.profile_id.is_empty()
            || !profile_ids.insert(profile.profile_id.as_str())
            || !source_ids.contains(profile.profile_id.as_str())
            || previous_id.is_some_and(|previous| previous >= profile.profile_id.as_str())
            || profile.expected_visible_event_label != expected_event_label
            || profile.event_display_label_sha256 != sha256_hex_v1(expected_event_label.as_bytes())
            || profile.expected_visible_deck_label != EXPECTED_DECK_LABEL_V1
            || profile.deck_display_label_sha256 != sha256_hex_v1(EXPECTED_DECK_LABEL_V1.as_bytes())
            || !matches!(
                profile.state.as_str(),
                AWAITING_STATE_V1 | SELECTED_STATE_V1
            )
            || profile.select_deck_control_reference_sha256s
                != [receipt
                    .reference_hashes
                    .enabled_select_deck_control_region_sha256
                    .clone()]
            || profile.open_entry_review_unavailable_reference_sha256s
                != [receipt
                    .reference_hashes
                    .open_entry_review_unavailable_region_sha256
                    .clone()]
        {
            return Err("partial deck-gate reference profile differs".to_owned());
        }
        let source = receipt
            .reviewed_sources
            .iter()
            .find(|source| source.source_id == profile.profile_id)
            .ok_or_else(|| "partial profile source is absent".to_owned())?;
        if source.event_kind != profile.event_kind || source.state != profile.state {
            return Err("partial profile source mode or state differs".to_owned());
        }
        match profile.state.as_str() {
            AWAITING_STATE_V1 if profile.selected_deck_label_reference_sha256s.is_empty() => {}
            SELECTED_STATE_V1
                if profile.selected_deck_label_reference_sha256s
                    == [receipt
                        .reference_hashes
                        .selected_deck_label_region_sha256
                        .clone()] => {}
            _ => return Err("partial profile selected-deck reference differs".to_owned()),
        }
        previous_id = Some(profile.profile_id.as_str());
        states
            .entry(profile.event_kind.as_str())
            .or_default()
            .insert(profile.state.as_str());
    }
    let expected_states = BTreeSet::from([AWAITING_STATE_V1, SELECTED_STATE_V1]);
    if states.get("league") != Some(&expected_states)
        || states.get("challenge") != Some(&expected_states)
        || states.len() != 2
    {
        return Err("partial deck-gate profile mode coverage differs".to_owned());
    }
    Ok(states)
}

fn validate_coverage_v1(
    receipt: &ReceiptV1,
    profile_states: &BTreeMap<&str, BTreeSet<&str>>,
) -> Result<(), String> {
    if receipt.coverage.len() != 2 {
        return Err("partial deck-gate coverage requires two modes".to_owned());
    }
    let mut seen = BTreeSet::new();
    for coverage in &receipt.coverage {
        if !seen.insert(coverage.event_kind.as_str())
            || !matches!(coverage.event_kind.as_str(), "league" | "challenge")
            || coverage
                .reviewed_states
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
                != *profile_states
                    .get(coverage.event_kind.as_str())
                    .ok_or_else(|| "partial coverage mode has no profiles".to_owned())?
            || coverage.missing_states != [OPEN_REVIEW_STATE_V1]
        {
            return Err("partial deck-gate declared coverage differs".to_owned());
        }
    }
    Ok(())
}

fn validate_review_and_authority_v1(receipt: &ReceiptV1) -> Result<(), String> {
    let review = &receipt.review_confirmations;
    let authority = &receipt.authority;
    if !review.all_sources_player_visible_composed_pixels_only
        || !review.all_sources_unobscured_and_cursor_absent
        || !review.event_kind_and_visible_label_confirmed
        || !review.missing_deck_prompt_confirmed_for_awaiting_sources
        || !review.exact_selected_deck_confirmed_for_selected_sources
        || !review.entry_choices_remained_unselected
        || !review.open_entry_review_absent_from_every_source
        || review.formal_selected_listing_case_claimed
        || receipt.passes_three_state_gate
        || !authority.safe_for_partial_classifier_reference_generation
        || authority.grants_live_classification
        || authority.grants_semantic_evidence
        || authority.grants_policy_scoring
        || authority.grants_input
        || authority.grants_open_entry_review
        || authority.grants_event_entry
        || authority.grants_spending
    {
        return Err("partial deck-gate review or authority boundary differs".to_owned());
    }
    Ok(())
}

fn sha256_hex_v1(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_lower_sha256_v1(value: &str) -> bool {
    is_lower_hex_v1(value, 64)
}

fn is_lower_hex_v1(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn rect_is_valid_v1(rect: &RectV1, size: &SizeV1) -> bool {
    rect.width > 0
        && rect.height > 0
        && rect
            .x
            .checked_add(rect.width)
            .is_some_and(|right| right <= size.width)
        && rect
            .y
            .checked_add(rect.height)
            .is_some_and(|bottom| bottom <= size.height)
}

fn rect_contains_v1(outer: &RectV1, inner: &RectV1) -> bool {
    let Some(inner_right) = inner.x.checked_add(inner.width) else {
        return false;
    };
    let Some(inner_bottom) = inner.y.checked_add(inner.height) else {
        return false;
    };
    let Some(outer_right) = outer.x.checked_add(outer.width) else {
        return false;
    };
    let Some(outer_bottom) = outer.y.checked_add(outer.height) else {
        return false;
    };
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner_right <= outer_right
        && inner_bottom <= outer_bottom
}

fn rects_overlap_v1(first: &RectV1, second: &RectV1) -> bool {
    first.x < second.x.saturating_add(second.width)
        && second.x < first.x.saturating_add(first.width)
        && first.y < second.y.saturating_add(second.height)
        && second.y < first.y.saturating_add(first.height)
}

fn valid_utc_second_v1(value: &str) -> bool {
    if value.len() != 20
        || value.as_bytes().get(4) != Some(&b'-')
        || value.as_bytes().get(7) != Some(&b'-')
        || value.as_bytes().get(10) != Some(&b'T')
        || value.as_bytes().get(13) != Some(&b':')
        || value.as_bytes().get(16) != Some(&b':')
        || value.as_bytes().get(19) != Some(&b'Z')
    {
        return false;
    }
    let digits = |range: std::ops::Range<usize>| {
        value
            .as_bytes()
            .get(range.clone())
            .filter(|bytes| bytes.iter().all(u8::is_ascii_digit))
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .and_then(|text| text.parse::<u32>().ok())
    };
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        digits(0..4),
        digits(5..7),
        digits(8..10),
        digits(11..13),
        digits(14..16),
        digits(17..19),
    ) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=9_999).contains(&year)
        && (1..=max_day).contains(&day)
        && hour <= 23
        && minute <= 59
        && second <= 59
}
