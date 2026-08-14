use super::{
    probe_mtgo_visible_accessibility_exact_text_v1, sha256_hex_v1, MtgoDxgiCaptureRequestV3,
    MtgoVisibleAccessibilityExactTextQueryV1, OpaqueMtgoVisibleAccessibilityProbeV1,
};
use mtgo_blackbox_v1::{
    competitive_event_listing_target_commitment_v1, MtgoCompetitiveDeckGateStateV1,
    MtgoCompetitiveEventKindV1, MtgoCompetitiveEventListingTargetV1,
    ValidatedMtgoCompetitiveDeckManifestV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MTGO_COMPETITIVE_ENTRY_ACCESSIBILITY_SCHEMA_V1: u32 = 1;

const ENTRY_ACCESSIBILITY_CLASSIFICATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-entry-visible-accessibility-classification-v1";
const ENTRY_ACCESSIBILITY_PROFILE_ID_V1: &str =
    "mtgo-event-browser-visible-labels-3.4.158.4691-1550x925-20260814-v1";
const ENTRY_ACCESSIBILITY_REVIEWED_MISSING_FRAME_BGRA_SHA256_V1: &str =
    "80d67beea4c0aa25a34b22cef6aafad15cc169952c43a57c03abd4a86f9bc61c";
const ENTRY_ACCESSIBILITY_REVIEWED_SELECTED_FRAME_BGRA_SHA256_V1: &str =
    "3b6537e51d5489a2f08e441275335a1797bffe1a1bce7f794d1fe416cbd1b712";
const ENTRY_ACCESSIBILITY_REVIEWED_LIVE_REPORT_SHA256_V1: &str =
    "ac9406307b9e97b4a950a64ca8476ab855f53e0c1b99b6bcd04e5f0b4375c070";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveEntryAccessibilityCountsV1 {
    pub change_deck: u32,
    pub choose_entry_option: u32,
    pub event_label: u32,
    pub one_hundred_play_points: u32,
    pub open_entry_review: u32,
    pub please_select_a_deck: u32,
    pub selected_deck: u32,
    pub ten_event_tickets: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveEntryAccessibilityCommitmentsV1 {
    pub schema_version: u32,
    pub profile_id: String,
    pub target_commitment_sha256: String,
    pub source_report_commitment_sha256: String,
    pub classification_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub state: MtgoCompetitiveDeckGateStateV1,
    pub counts: MtgoCompetitiveEntryAccessibilityCountsV1,
}

/// A current, visible-only Event Browser deck-gate classification. The source
/// retains the exact UI Automation probe whose matches were process-bound,
/// on-screen, fully client-contained, center hit-tested, and bracketed by
/// unchanged foreground/window/occlusion snapshots. It exposes no discovered
/// text, rectangles, UIA element, control pattern, pixels, or input method.
pub struct OpaqueMtgoCompetitiveEntryAccessibilityClassificationV1 {
    _source: OpaqueMtgoVisibleAccessibilityProbeV1,
    commitments: MtgoCompetitiveEntryAccessibilityCommitmentsV1,
}

impl OpaqueMtgoCompetitiveEntryAccessibilityClassificationV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveEntryAccessibilityCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn state_v1(&self) -> MtgoCompetitiveDeckGateStateV1 {
        self.commitments.state
    }

    pub fn safe_for_pre_entry_deck_gate_classification_v1(&self) -> bool {
        true
    }

    pub fn safe_for_policy_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

pub fn classify_live_competitive_entry_accessibility_v1(
    window_request: MtgoDxgiCaptureRequestV3,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    target: &MtgoCompetitiveEventListingTargetV1,
    expected_visible_event_label: &str,
    expected_visible_deck_label: &str,
) -> Result<OpaqueMtgoCompetitiveEntryAccessibilityClassificationV1, String> {
    validate_expected_visible_label_v1(expected_visible_event_label, "event")?;
    validate_expected_visible_label_v1(expected_visible_deck_label, "deck")?;
    if sha256_hex_v1(expected_visible_event_label.as_bytes()) != target.event_display_label_sha256
        || sha256_hex_v1(expected_visible_deck_label.as_bytes()) != target.deck_display_label_sha256
    {
        return Err("visible entry labels do not match the exact event target".to_owned());
    }
    let target_commitment_sha256 = competitive_event_listing_target_commitment_v1(target, deck)
        .map_err(|error| format!("validate visible entry target: {error}"))?;
    let queries =
        entry_accessibility_queries_v1(expected_visible_event_label, expected_visible_deck_label);
    let source = probe_mtgo_visible_accessibility_exact_text_v1(window_request, queries)?;
    let summary = source.summary_v1();
    let count = |query_id: &str| -> Result<u32, String> {
        summary
            .query_results
            .iter()
            .find(|result| result.query_id == query_id)
            .map(|result| result.exact_visible_match_count)
            .ok_or_else(|| format!("visible entry query {query_id} is missing"))
    };
    let counts = MtgoCompetitiveEntryAccessibilityCountsV1 {
        change_deck: count("change_deck")?,
        choose_entry_option: count("choose_entry_option")?,
        event_label: count("event_label")?,
        one_hundred_play_points: count("one_hundred_play_points")?,
        open_entry_review: count("open_entry_review")?,
        please_select_a_deck: count("please_select_a_deck")?,
        selected_deck: count("selected_deck")?,
        ten_event_tickets: count("ten_event_tickets")?,
    };
    let state = classify_entry_accessibility_counts_v1(&counts)?;
    let counts_json = serde_json::to_vec(&counts)
        .map_err(|error| format!("serialize visible entry counts: {error}"))?;
    let state_json = serde_json::to_vec(&state)
        .map_err(|error| format!("serialize visible entry state: {error}"))?;
    let profile_commitment = entry_accessibility_profile_commitment_v1();
    let classification_commitment_sha256 = commitment_v1(
        ENTRY_ACCESSIBILITY_CLASSIFICATION_DOMAIN_V1,
        &[
            profile_commitment.as_bytes(),
            target_commitment_sha256.as_bytes(),
            summary.report_commitment_sha256.as_bytes(),
            counts_json.as_slice(),
            state_json.as_slice(),
            b"visible_uia_exact_center_hit_tested_no_control_pattern_no_input",
        ],
    );
    Ok(OpaqueMtgoCompetitiveEntryAccessibilityClassificationV1 {
        _source: source,
        commitments: MtgoCompetitiveEntryAccessibilityCommitmentsV1 {
            schema_version: MTGO_COMPETITIVE_ENTRY_ACCESSIBILITY_SCHEMA_V1,
            profile_id: ENTRY_ACCESSIBILITY_PROFILE_ID_V1.to_owned(),
            target_commitment_sha256,
            source_report_commitment_sha256: summary.report_commitment_sha256,
            classification_commitment_sha256,
            event_kind: target.event_kind,
            state,
            counts,
        },
    })
}

fn entry_accessibility_queries_v1(
    expected_visible_event_label: &str,
    expected_visible_deck_label: &str,
) -> Vec<MtgoVisibleAccessibilityExactTextQueryV1> {
    [
        ("change_deck", "Change Deck"),
        ("choose_entry_option", "Choose Entry Option:"),
        ("event_label", expected_visible_event_label),
        ("one_hundred_play_points", "100 Play Points"),
        ("open_entry_review", "Open Entry Review"),
        ("please_select_a_deck", "Please Select a Deck"),
        ("selected_deck", expected_visible_deck_label),
        ("ten_event_tickets", "10 Event Tickets"),
    ]
    .into_iter()
    .map(
        |(query_id, expected_visible_text)| MtgoVisibleAccessibilityExactTextQueryV1 {
            query_id: query_id.to_owned(),
            expected_visible_text: expected_visible_text.to_owned(),
        },
    )
    .collect()
}

fn classify_entry_accessibility_counts_v1(
    counts: &MtgoCompetitiveEntryAccessibilityCountsV1,
) -> Result<MtgoCompetitiveDeckGateStateV1, String> {
    if counts.event_label != 2
        || counts.choose_entry_option > 1
        || counts.one_hundred_play_points > 1
        || counts.ten_event_tickets > 1
        || counts.open_entry_review > 1
        || counts.please_select_a_deck > 1
        || counts.selected_deck > 1
        || counts.change_deck > 1
    {
        return Err("visible entry label counts are outside the reviewed profile".to_owned());
    }
    let exact_entry_surface = counts.change_deck == 1
        && counts.choose_entry_option == 1
        && counts.one_hundred_play_points == 1
        && counts.ten_event_tickets == 1;
    if exact_entry_surface
        && counts.please_select_a_deck == 1
        && counts.selected_deck == 0
        && counts.open_entry_review == 0
    {
        return Ok(MtgoCompetitiveDeckGateStateV1::AwaitingCompatibleDeckSelection);
    }
    let exact_selected_surface =
        exact_entry_surface && counts.please_select_a_deck == 0 && counts.selected_deck == 1;
    if exact_selected_surface && counts.open_entry_review == 0 {
        return Ok(MtgoCompetitiveDeckGateStateV1::CompatibleDeckSelected);
    }
    if exact_selected_surface && counts.open_entry_review == 1 {
        return Ok(MtgoCompetitiveDeckGateStateV1::OpenEntryReviewAvailable);
    }
    Err("visible entry labels do not form exactly one deck-gate state".to_owned())
}

fn validate_expected_visible_label_v1(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 256
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(format!("expected visible {label} label is not canonical"));
    }
    Ok(())
}

fn entry_accessibility_profile_commitment_v1() -> String {
    commitment_v1(
        b"mtgo-competitive-entry-visible-accessibility-reviewed-profile-v1",
        &[
            ENTRY_ACCESSIBILITY_PROFILE_ID_V1.as_bytes(),
            ENTRY_ACCESSIBILITY_REVIEWED_MISSING_FRAME_BGRA_SHA256_V1.as_bytes(),
            ENTRY_ACCESSIBILITY_REVIEWED_SELECTED_FRAME_BGRA_SHA256_V1.as_bytes(),
            ENTRY_ACCESSIBILITY_REVIEWED_LIVE_REPORT_SHA256_V1.as_bytes(),
            b"exact_visible_labels_manually_reviewed_in_player_visible_main_client",
        ],
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    fn selected_counts_v1() -> MtgoCompetitiveEntryAccessibilityCountsV1 {
        MtgoCompetitiveEntryAccessibilityCountsV1 {
            change_deck: 1,
            choose_entry_option: 1,
            event_label: 2,
            one_hundred_play_points: 1,
            open_entry_review: 0,
            please_select_a_deck: 0,
            selected_deck: 1,
            ten_event_tickets: 1,
        }
    }

    #[test]
    fn exact_three_states_classify_and_ambiguous_counts_reject() {
        let mut awaiting = selected_counts_v1();
        awaiting.please_select_a_deck = 1;
        awaiting.selected_deck = 0;
        assert_eq!(
            classify_entry_accessibility_counts_v1(&awaiting).unwrap(),
            MtgoCompetitiveDeckGateStateV1::AwaitingCompatibleDeckSelection
        );

        let selected = selected_counts_v1();
        assert_eq!(
            classify_entry_accessibility_counts_v1(&selected).unwrap(),
            MtgoCompetitiveDeckGateStateV1::CompatibleDeckSelected
        );

        let mut review = selected.clone();
        review.open_entry_review = 1;
        assert_eq!(
            classify_entry_accessibility_counts_v1(&review).unwrap(),
            MtgoCompetitiveDeckGateStateV1::OpenEntryReviewAvailable
        );

        let mut ambiguous = selected;
        ambiguous.please_select_a_deck = 1;
        assert!(classify_entry_accessibility_counts_v1(&ambiguous).is_err());
    }

    #[test]
    fn profile_commitment_binds_reviewed_visible_sources() {
        let commitment = entry_accessibility_profile_commitment_v1();
        assert_eq!(commitment.len(), 64);
        assert_ne!(commitment, "0".repeat(64));
    }
}
