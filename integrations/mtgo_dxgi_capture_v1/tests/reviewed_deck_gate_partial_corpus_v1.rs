use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

const RECEIPT_JSON: &str =
    include_str!("../assets/reviewed_modern_deck_gate_partial_corpus_v1.json");

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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SizeV1 {
    width: u32,
    height: u32,
}

#[derive(Debug, Deserialize)]
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

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn rect_is_valid(rect: &RectV1, size: &SizeV1) -> bool {
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

#[test]
fn reviewed_partial_corpus_covers_two_states_per_mode_and_fails_closed() {
    let receipt: ReceiptV1 = serde_json::from_str(RECEIPT_JSON).unwrap();
    assert_eq!(receipt.schema_version, 1);
    assert_eq!(
        receipt.scope,
        "reviewed_modern_league_challenge_deck_gate_partial_corpus_v1"
    );
    assert!(!receipt.review_id.is_empty());
    assert!(receipt.reviewed_at_utc.ends_with('Z'));
    assert_eq!(
        receipt.review_method,
        "visible_pixel_review_plus_exact_external_frame_rehash_v1"
    );
    assert_eq!(receipt.client_identity.product_version, "3.4.158.4691");
    assert!(is_lower_sha256(&receipt.client_identity.executable_sha256));
    assert_eq!(receipt.client_identity.signer_thumbprint.len(), 40);
    assert_eq!(receipt.client_identity.dpi, 120);
    assert_eq!(receipt.client_identity.client_size_px.width, 1_550);
    assert_eq!(receipt.client_identity.client_size_px.height, 925);
    assert_eq!(
        receipt.client_identity.canonical_pixel_format,
        "bgra8_unorm_top_down_tightly_packed_v1"
    );

    for rect in [
        &receipt.region_geometry.event_label_search_rect_client_px,
        &receipt.region_geometry.deck_status_search_rect_client_px,
        &receipt.region_geometry.deck_label_region_rect_client_px,
        &receipt.region_geometry.select_deck_control_rect_client_px,
        &receipt
            .region_geometry
            .open_entry_review_status_rect_client_px,
    ] {
        assert!(rect_is_valid(rect, &receipt.client_identity.client_size_px));
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
        assert!(is_lower_sha256(digest));
    }

    assert_eq!(receipt.reviewed_sources.len(), 5);
    let mut source_ids = BTreeSet::new();
    let mut source_manifests = BTreeSet::new();
    let mut source_pngs = BTreeSet::new();
    let mut source_pixels = BTreeSet::new();
    for source in &receipt.reviewed_sources {
        assert!(source_ids.insert(source.source_id.as_str()));
        assert!(source_manifests.insert(source.source_manifest_sha256.as_str()));
        assert!(source_pngs.insert(source.source_png_sha256.as_str()));
        assert!(source_pixels.insert(source.canonical_bgra8_sha256.as_str()));
        assert!(matches!(source.event_kind.as_str(), "league" | "challenge"));
        assert!(matches!(
            source.state.as_str(),
            "awaiting_compatible_deck_selection" | "compatible_deck_selected"
        ));
        assert!(is_lower_sha256(&source.source_manifest_sha256));
        assert!(is_lower_sha256(&source.source_png_sha256));
        assert!(is_lower_sha256(&source.canonical_bgra8_sha256));
        assert!(source.pending_visual_review_capture);
        assert!(!source.safe_for_semantic_evidence);
        assert!(!source.safe_for_policy_scoring);
        assert!(!source.safe_for_input);
    }

    let expected_states = BTreeSet::from([
        "awaiting_compatible_deck_selection",
        "compatible_deck_selected",
    ]);
    let mut coverage_by_kind = BTreeMap::new();
    for coverage in &receipt.coverage {
        assert!(coverage_by_kind
            .insert(coverage.event_kind.as_str(), coverage)
            .is_none());
        assert_eq!(
            coverage
                .reviewed_states
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            expected_states
        );
        assert_eq!(coverage.missing_states, ["open_entry_review_available"]);
    }
    assert_eq!(coverage_by_kind.len(), 2);
    assert!(coverage_by_kind.contains_key("league"));
    assert!(coverage_by_kind.contains_key("challenge"));

    assert_eq!(receipt.partial_reference_profiles.len(), 4);
    let mut profile_states = BTreeMap::<&str, BTreeSet<&str>>::new();
    for profile in &receipt.partial_reference_profiles {
        assert!(!profile.profile_id.is_empty());
        assert!(matches!(
            profile.event_kind.as_str(),
            "league" | "challenge"
        ));
        assert!(is_lower_sha256(&profile.event_display_label_sha256));
        assert!(!profile.expected_visible_event_label.is_empty());
        assert!(is_lower_sha256(&profile.deck_display_label_sha256));
        assert_eq!(
            profile.expected_visible_deck_label,
            "mtgo-kernel-modern-basics-v1"
        );
        assert_ne!(profile.state, "open_entry_review_available");
        if profile.state == "compatible_deck_selected" {
            assert_eq!(
                profile.selected_deck_label_reference_sha256s,
                [receipt
                    .reference_hashes
                    .selected_deck_label_region_sha256
                    .clone()]
            );
        } else {
            assert!(profile.selected_deck_label_reference_sha256s.is_empty());
        }
        assert_eq!(
            profile.select_deck_control_reference_sha256s,
            [receipt
                .reference_hashes
                .enabled_select_deck_control_region_sha256
                .clone()]
        );
        assert_eq!(
            profile.open_entry_review_unavailable_reference_sha256s,
            [receipt
                .reference_hashes
                .open_entry_review_unavailable_region_sha256
                .clone()]
        );
        profile_states
            .entry(profile.event_kind.as_str())
            .or_default()
            .insert(profile.state.as_str());
    }
    assert_eq!(profile_states.get("league"), Some(&expected_states));
    assert_eq!(profile_states.get("challenge"), Some(&expected_states));

    let confirmations = &receipt.review_confirmations;
    assert!(confirmations.all_sources_player_visible_composed_pixels_only);
    assert!(confirmations.all_sources_unobscured_and_cursor_absent);
    assert!(confirmations.event_kind_and_visible_label_confirmed);
    assert!(confirmations.missing_deck_prompt_confirmed_for_awaiting_sources);
    assert!(confirmations.exact_selected_deck_confirmed_for_selected_sources);
    assert!(confirmations.entry_choices_remained_unselected);
    assert!(confirmations.open_entry_review_absent_from_every_source);
    assert!(!confirmations.formal_selected_listing_case_claimed);

    assert!(!receipt.passes_three_state_gate);
    assert!(
        receipt
            .authority
            .safe_for_partial_classifier_reference_generation
    );
    assert!(!receipt.authority.grants_live_classification);
    assert!(!receipt.authority.grants_semantic_evidence);
    assert!(!receipt.authority.grants_policy_scoring);
    assert!(!receipt.authority.grants_input);
    assert!(!receipt.authority.grants_open_entry_review);
    assert!(!receipt.authority.grants_event_entry);
    assert!(!receipt.authority.grants_spending);
}
