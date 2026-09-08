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

pub const MTGO_COMPETITIVE_ENTRY_ACCESSIBILITY_SCHEMA_V2: u32 = 2;

const ENTRY_ACCESSIBILITY_CLASSIFICATION_DOMAIN_V2: &[u8] =
    b"mtgo-competitive-entry-visible-accessibility-classification-v2";
const ENTRY_ACCESSIBILITY_LEAGUE_PROFILE_ID_V2: &str =
    "mtgo-event-browser-visible-labels-league-3.4.158.4691-1550x925-20260814-v2";
const ENTRY_ACCESSIBILITY_CHALLENGE_PROFILE_ID_V2: &str =
    "mtgo-event-browser-visible-labels-challenge-3.4.158.4691-1550x925-20260814-v2";
const ENTRY_ACCESSIBILITY_REVIEWED_LEAGUE_MISSING_FRAME_BGRA_SHA256_V1: &str =
    "80d67beea4c0aa25a34b22cef6aafad15cc169952c43a57c03abd4a86f9bc61c";
const ENTRY_ACCESSIBILITY_REVIEWED_LEAGUE_SELECTED_FRAME_BGRA_SHA256_V1: &str =
    "3b6537e51d5489a2f08e441275335a1797bffe1a1bce7f794d1fe416cbd1b712";
const ENTRY_ACCESSIBILITY_REVIEWED_LEAGUE_LIVE_REPORT_SHA256_V1: &str =
    "ac9406307b9e97b4a950a64ca8476ab855f53e0c1b99b6bcd04e5f0b4375c070";
const ENTRY_ACCESSIBILITY_REVIEWED_CHALLENGE_SELECTED_FRAME_BGRA_SHA256_V1: &str =
    "7b5e2926d8fa14590d1041be6571e5ab95ec2ad6e29c1ccf8c9717f965b5945b";
const ENTRY_ACCESSIBILITY_REVIEWED_CHALLENGE_LIVE_REPORT_SHA256_V1: &str =
    "380be9fc32b4c4e01d61104eddba093e8457d0111165efef71a7d9f24671f40f";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveEntryAccessibilityCountsV2 {
    pub change_deck: u32,
    pub choose_entry_option: u32,
    pub entry_event_tickets: u32,
    pub entry_play_points: u32,
    pub event_label: u32,
    pub open_entry_review: u32,
    pub please_select_a_deck: u32,
    pub selected_deck: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveEntryAccessibilityCommitmentsV2 {
    pub schema_version: u32,
    pub profile_id: String,
    pub target_commitment_sha256: String,
    pub source_report_commitment_sha256: String,
    pub classification_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub state: MtgoCompetitiveDeckGateStateV1,
    pub counts: MtgoCompetitiveEntryAccessibilityCountsV2,
}

/// A current, visible-only Event Browser deck-gate classification. The source
/// retains the exact UI Automation probe whose matches were process-bound,
/// on-screen, fully client-contained, center hit-tested, and bracketed by
/// unchanged foreground/window/occlusion snapshots. It exposes no discovered
/// text, rectangles, UIA element, control pattern, pixels, or input method.
pub struct OpaqueMtgoCompetitiveEntryAccessibilityClassificationV2 {
    _source: OpaqueMtgoVisibleAccessibilityProbeV1,
    commitments: MtgoCompetitiveEntryAccessibilityCommitmentsV2,
}

impl OpaqueMtgoCompetitiveEntryAccessibilityClassificationV2 {
    pub fn commitments_v2(&self) -> MtgoCompetitiveEntryAccessibilityCommitmentsV2 {
        self.commitments.clone()
    }

    pub fn state_v2(&self) -> MtgoCompetitiveDeckGateStateV1 {
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

pub fn classify_live_competitive_entry_accessibility_v2(
    window_request: MtgoDxgiCaptureRequestV3,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    target: &MtgoCompetitiveEventListingTargetV1,
    expected_visible_event_label: &str,
    expected_visible_deck_label: &str,
) -> Result<OpaqueMtgoCompetitiveEntryAccessibilityClassificationV2, String> {
    validate_expected_visible_label_v1(expected_visible_event_label, "event")?;
    validate_expected_visible_label_v1(expected_visible_deck_label, "deck")?;
    if sha256_hex_v1(expected_visible_event_label.as_bytes()) != target.event_display_label_sha256
        || sha256_hex_v1(expected_visible_deck_label.as_bytes()) != target.deck_display_label_sha256
    {
        return Err("visible entry labels do not match the exact event target".to_owned());
    }
    let target_commitment_sha256 = competitive_event_listing_target_commitment_v1(target, deck)
        .map_err(|error| format!("validate visible entry target: {error}"))?;
    let queries = entry_accessibility_queries_v2(
        target.event_kind,
        expected_visible_event_label,
        expected_visible_deck_label,
    );
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
    let counts = MtgoCompetitiveEntryAccessibilityCountsV2 {
        change_deck: count("change_deck")?,
        choose_entry_option: count("choose_entry_option")?,
        entry_event_tickets: count("entry_event_tickets")?,
        entry_play_points: count("entry_play_points")?,
        event_label: count("event_label")?,
        open_entry_review: count("open_entry_review")?,
        please_select_a_deck: count("please_select_a_deck")?,
        selected_deck: count("selected_deck")?,
    };
    let state = classify_entry_accessibility_counts_v2(target.event_kind, &counts)?;
    let counts_json = serde_json::to_vec(&counts)
        .map_err(|error| format!("serialize visible entry counts: {error}"))?;
    let state_json = serde_json::to_vec(&state)
        .map_err(|error| format!("serialize visible entry state: {error}"))?;
    let profile_commitment = entry_accessibility_profile_commitment_v2(target.event_kind);
    let classification_commitment_sha256 = commitment_v1(
        ENTRY_ACCESSIBILITY_CLASSIFICATION_DOMAIN_V2,
        &[
            profile_commitment.as_bytes(),
            target_commitment_sha256.as_bytes(),
            summary.report_commitment_sha256.as_bytes(),
            counts_json.as_slice(),
            state_json.as_slice(),
            b"visible_uia_exact_center_hit_tested_no_control_pattern_no_input",
        ],
    );
    Ok(OpaqueMtgoCompetitiveEntryAccessibilityClassificationV2 {
        _source: source,
        commitments: MtgoCompetitiveEntryAccessibilityCommitmentsV2 {
            schema_version: MTGO_COMPETITIVE_ENTRY_ACCESSIBILITY_SCHEMA_V2,
            profile_id: entry_accessibility_profile_id_v2(target.event_kind).to_owned(),
            target_commitment_sha256,
            source_report_commitment_sha256: summary.report_commitment_sha256,
            classification_commitment_sha256,
            event_kind: target.event_kind,
            state,
            counts,
        },
    })
}

fn entry_accessibility_queries_v2(
    event_kind: MtgoCompetitiveEventKindV1,
    expected_visible_event_label: &str,
    expected_visible_deck_label: &str,
) -> Vec<MtgoVisibleAccessibilityExactTextQueryV1> {
    let (play_points_label, event_tickets_label) = entry_terms_visible_labels_v2(event_kind);
    [
        ("change_deck", "Change Deck"),
        ("choose_entry_option", "Choose Entry Option:"),
        ("entry_event_tickets", event_tickets_label),
        ("entry_play_points", play_points_label),
        ("event_label", expected_visible_event_label),
        ("open_entry_review", "Open Entry Review"),
        ("please_select_a_deck", "Please Select a Deck"),
        ("selected_deck", expected_visible_deck_label),
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

fn entry_terms_visible_labels_v2(
    event_kind: MtgoCompetitiveEventKindV1,
) -> (&'static str, &'static str) {
    match event_kind {
        MtgoCompetitiveEventKindV1::League => ("100 Play Points", "10 Event Tickets"),
        MtgoCompetitiveEventKindV1::Challenge => ("300 Play Points", "30 Event Tickets"),
    }
}

fn classify_entry_accessibility_counts_v2(
    event_kind: MtgoCompetitiveEventKindV1,
    counts: &MtgoCompetitiveEntryAccessibilityCountsV2,
) -> Result<MtgoCompetitiveDeckGateStateV1, String> {
    let expected_event_label_count = match event_kind {
        MtgoCompetitiveEventKindV1::League => 2,
        MtgoCompetitiveEventKindV1::Challenge => 3,
    };
    if counts.event_label != expected_event_label_count
        || counts.choose_entry_option > 1
        || counts.entry_play_points > 1
        || counts.entry_event_tickets > 1
        || counts.open_entry_review > 1
        || counts.please_select_a_deck > 1
        || counts.selected_deck > 1
        || counts.change_deck > 1
    {
        return Err("visible entry label counts are outside the reviewed profile".to_owned());
    }
    let exact_entry_surface = counts.change_deck == 1
        && counts.choose_entry_option == 1
        && counts.entry_play_points == 1
        && counts.entry_event_tickets == 1;
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

fn entry_accessibility_profile_id_v2(event_kind: MtgoCompetitiveEventKindV1) -> &'static str {
    match event_kind {
        MtgoCompetitiveEventKindV1::League => ENTRY_ACCESSIBILITY_LEAGUE_PROFILE_ID_V2,
        MtgoCompetitiveEventKindV1::Challenge => ENTRY_ACCESSIBILITY_CHALLENGE_PROFILE_ID_V2,
    }
}

fn entry_accessibility_profile_commitment_v2(event_kind: MtgoCompetitiveEventKindV1) -> String {
    let (event_kind_label, play_points_label, event_tickets_label, reviewed_sources): (
        &[u8],
        &str,
        &str,
        &[&[u8]],
    ) = match event_kind {
        MtgoCompetitiveEventKindV1::League => (
            b"league",
            "100 Play Points",
            "10 Event Tickets",
            &[
                ENTRY_ACCESSIBILITY_REVIEWED_LEAGUE_MISSING_FRAME_BGRA_SHA256_V1.as_bytes(),
                ENTRY_ACCESSIBILITY_REVIEWED_LEAGUE_SELECTED_FRAME_BGRA_SHA256_V1.as_bytes(),
                ENTRY_ACCESSIBILITY_REVIEWED_LEAGUE_LIVE_REPORT_SHA256_V1.as_bytes(),
            ],
        ),
        MtgoCompetitiveEventKindV1::Challenge => (
            b"challenge",
            "300 Play Points",
            "30 Event Tickets",
            &[
                ENTRY_ACCESSIBILITY_REVIEWED_CHALLENGE_SELECTED_FRAME_BGRA_SHA256_V1.as_bytes(),
                ENTRY_ACCESSIBILITY_REVIEWED_CHALLENGE_LIVE_REPORT_SHA256_V1.as_bytes(),
            ],
        ),
    };
    let mut parts: Vec<&[u8]> = vec![
        entry_accessibility_profile_id_v2(event_kind).as_bytes(),
        event_kind_label,
        play_points_label.as_bytes(),
        event_tickets_label.as_bytes(),
    ];
    parts.extend_from_slice(reviewed_sources);
    parts.push(b"exact_visible_labels_manually_reviewed_in_player_visible_main_client");
    commitment_v1(
        b"mtgo-competitive-entry-visible-accessibility-reviewed-profile-v2",
        &parts,
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

    fn query_text_v1<'a>(
        queries: &'a [MtgoVisibleAccessibilityExactTextQueryV1],
        id: &str,
    ) -> &'a str {
        queries
            .iter()
            .find(|query| query.query_id == id)
            .unwrap()
            .expected_visible_text
            .as_str()
    }

    fn selected_counts_v2(
        event_kind: MtgoCompetitiveEventKindV1,
    ) -> MtgoCompetitiveEntryAccessibilityCountsV2 {
        MtgoCompetitiveEntryAccessibilityCountsV2 {
            change_deck: 1,
            choose_entry_option: 1,
            entry_event_tickets: 1,
            entry_play_points: 1,
            event_label: match event_kind {
                MtgoCompetitiveEventKindV1::League => 2,
                MtgoCompetitiveEventKindV1::Challenge => 3,
            },
            open_entry_review: 0,
            please_select_a_deck: 0,
            selected_deck: 1,
        }
    }

    #[test]
    fn exact_three_states_classify_and_ambiguous_counts_reject() {
        let mut awaiting = selected_counts_v2(MtgoCompetitiveEventKindV1::League);
        awaiting.please_select_a_deck = 1;
        awaiting.selected_deck = 0;
        assert_eq!(
            classify_entry_accessibility_counts_v2(MtgoCompetitiveEventKindV1::League, &awaiting)
                .unwrap(),
            MtgoCompetitiveDeckGateStateV1::AwaitingCompatibleDeckSelection
        );

        let selected = selected_counts_v2(MtgoCompetitiveEventKindV1::League);
        assert_eq!(
            classify_entry_accessibility_counts_v2(MtgoCompetitiveEventKindV1::League, &selected)
                .unwrap(),
            MtgoCompetitiveDeckGateStateV1::CompatibleDeckSelected
        );

        let mut review = selected.clone();
        review.open_entry_review = 1;
        assert_eq!(
            classify_entry_accessibility_counts_v2(MtgoCompetitiveEventKindV1::League, &review)
                .unwrap(),
            MtgoCompetitiveDeckGateStateV1::OpenEntryReviewAvailable
        );

        let mut ambiguous = selected;
        ambiguous.please_select_a_deck = 1;
        assert!(classify_entry_accessibility_counts_v2(
            MtgoCompetitiveEventKindV1::League,
            &ambiguous
        )
        .is_err());
    }

    #[test]
    fn league_and_challenge_queries_bind_exact_visible_terms() {
        let league = entry_accessibility_queries_v2(
            MtgoCompetitiveEventKindV1::League,
            "Modern League",
            "deck",
        );
        let challenge = entry_accessibility_queries_v2(
            MtgoCompetitiveEventKindV1::Challenge,
            "Modern Challenge 64",
            "deck",
        );
        assert_eq!(
            query_text_v1(&league, "entry_play_points"),
            "100 Play Points"
        );
        assert_eq!(
            query_text_v1(&league, "entry_event_tickets"),
            "10 Event Tickets"
        );
        assert_eq!(
            query_text_v1(&challenge, "entry_play_points"),
            "300 Play Points"
        );
        assert_eq!(
            query_text_v1(&challenge, "entry_event_tickets"),
            "30 Event Tickets"
        );
        assert!(league
            .windows(2)
            .all(|pair| pair[0].query_id < pair[1].query_id));
        assert!(challenge
            .windows(2)
            .all(|pair| pair[0].query_id < pair[1].query_id));
    }

    #[test]
    fn challenge_selected_counts_classify_only_under_challenge_profile() {
        let selected = selected_counts_v2(MtgoCompetitiveEventKindV1::Challenge);
        assert_eq!(
            classify_entry_accessibility_counts_v2(
                MtgoCompetitiveEventKindV1::Challenge,
                &selected
            )
            .unwrap(),
            MtgoCompetitiveDeckGateStateV1::CompatibleDeckSelected
        );
        assert!(classify_entry_accessibility_counts_v2(
            MtgoCompetitiveEventKindV1::League,
            &selected
        )
        .is_err());
    }

    #[test]
    fn profile_commitment_binds_event_specific_visible_sources() {
        let league = entry_accessibility_profile_commitment_v2(MtgoCompetitiveEventKindV1::League);
        let challenge =
            entry_accessibility_profile_commitment_v2(MtgoCompetitiveEventKindV1::Challenge);
        assert_eq!(league.len(), 64);
        assert_eq!(challenge.len(), 64);
        assert_ne!(league, challenge);
        assert_ne!(league, "0".repeat(64));
        assert_ne!(challenge, "0".repeat(64));
    }
}
