use crate::{
    competitive_event_listing_target_commitment_v1,
    CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1, MtgoCompetitiveEventKindV1,
    MtgoCompetitiveEventListingTargetV1, MtgoCompetitiveLifecyclePhaseV1, MtgoContractErrorV1,
    MtgoRectPxV1, ValidatedMtgoCompetitiveDeckManifestV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MTGO_COMPETITIVE_DECK_GATE_SCHEMA_V1: u32 = 1;
const MIN_COMPETITIVE_DECK_GATE_CONFIDENCE_BPS_V1: u16 = 9_500;
const COMPETITIVE_DECK_GATE_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-visible-competitive-deck-gate-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveDeckGateStateV1 {
    AwaitingCompatibleDeckSelection,
    CompatibleDeckSelected,
}

/// One selected League or Challenge listing before Open Entry Review.
///
/// The live client can show entry choices while still withholding the Open
/// Entry Review control behind a compatible-deck selection. This record keeps
/// that intermediate state distinct from a listing that is actually ready to
/// open. Every field is derived only from the rendered Event Browser surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleCompetitiveDeckGateV1 {
    pub schema_version: u32,
    pub observation_id: String,
    pub target_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub event_display_label_sha256: String,
    pub source_lifecycle_snapshot_commitment_sha256: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub frame_sha256: String,
    pub client_bounds: MtgoRectPxV1,
    pub state: MtgoCompetitiveDeckGateStateV1,
    pub event_label_rect_client_px: MtgoRectPxV1,
    pub event_label_region_sha256: String,
    pub select_deck_control_rect_client_px: MtgoRectPxV1,
    pub select_deck_control_region_sha256: String,
    pub select_deck_control_enabled: bool,
    pub missing_deck_prompt_rect_client_px: Option<MtgoRectPxV1>,
    pub missing_deck_prompt_region_sha256: Option<String>,
    pub selected_deck_label_sha256: Option<String>,
    pub selected_deck_rect_client_px: Option<MtgoRectPxV1>,
    pub selected_deck_region_sha256: Option<String>,
    pub open_entry_review_control_rect_client_px: Option<MtgoRectPxV1>,
    pub open_entry_review_control_region_sha256: Option<String>,
    pub open_entry_review_control_enabled: bool,
    pub confidence_bps: u16,
}

/// Coordinate-private, checked interpretation of the visible pre-entry deck
/// gate. It deliberately has no deck-chooser, entry-review, input, or spending
/// conversion.
pub struct CheckedUntrustedMtgoCompetitiveDeckGateV1 {
    _lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    _target: MtgoCompetitiveEventListingTargetV1,
    _raw: MtgoVisibleCompetitiveDeckGateV1,
    target_commitment_sha256: String,
    observation_commitment_sha256: String,
    state: MtgoCompetitiveDeckGateStateV1,
}

impl CheckedUntrustedMtgoCompetitiveDeckGateV1 {
    pub fn target_commitment_sha256_v1(&self) -> &str {
        &self.target_commitment_sha256
    }

    pub fn observation_commitment_sha256_v1(&self) -> &str {
        &self.observation_commitment_sha256
    }

    pub fn state_v1(&self) -> MtgoCompetitiveDeckGateStateV1 {
        self.state
    }

    pub fn ready_for_open_entry_review_v1(&self) -> bool {
        self.state == MtgoCompetitiveDeckGateStateV1::CompatibleDeckSelected
    }

    pub fn permits_open_deck_chooser_v1(&self) -> bool {
        false
    }

    pub fn permits_open_entry_review_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

pub fn validate_visible_competitive_deck_gate_v1(
    lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    target: MtgoCompetitiveEventListingTargetV1,
    raw: MtgoVisibleCompetitiveDeckGateV1,
) -> Result<CheckedUntrustedMtgoCompetitiveDeckGateV1, MtgoContractErrorV1> {
    if raw.schema_version != MTGO_COMPETITIVE_DECK_GATE_SCHEMA_V1 {
        return Err(error_v1(
            "competitive_deck_gate_schema",
            "expected competitive deck-gate schema version 1",
        ));
    }
    validate_identifier_v1(&raw.observation_id)?;
    let target_commitment_sha256 = competitive_event_listing_target_commitment_v1(&target, deck)?;
    for (value, code) in [
        (
            &raw.target_commitment_sha256,
            "competitive_deck_gate_target",
        ),
        (&raw.event_identity_sha256, "competitive_deck_gate_event"),
        (
            &raw.event_display_label_sha256,
            "competitive_deck_gate_event_label",
        ),
        (
            &raw.source_lifecycle_snapshot_commitment_sha256,
            "competitive_deck_gate_lifecycle",
        ),
        (&raw.frame_sha256, "competitive_deck_gate_frame"),
        (
            &raw.event_label_region_sha256,
            "competitive_deck_gate_event_region",
        ),
        (
            &raw.select_deck_control_region_sha256,
            "competitive_deck_gate_select_control_region",
        ),
    ] {
        validate_sha256_v1(value, code)?;
    }
    for (value, code) in [
        (
            raw.missing_deck_prompt_region_sha256.as_deref(),
            "competitive_deck_gate_prompt_region",
        ),
        (
            raw.selected_deck_label_sha256.as_deref(),
            "competitive_deck_gate_selected_deck_label",
        ),
        (
            raw.selected_deck_region_sha256.as_deref(),
            "competitive_deck_gate_selected_deck_region",
        ),
        (
            raw.open_entry_review_control_region_sha256.as_deref(),
            "competitive_deck_gate_open_review_region",
        ),
    ] {
        if let Some(value) = value {
            validate_sha256_v1(value, code)?;
        }
    }
    if lifecycle.phase() != MtgoCompetitiveLifecyclePhaseV1::EventBrowser
        || lifecycle.event_kind() != target.event_kind
        || lifecycle.frame_id_v1() != raw.frame_id
        || lifecycle.frame_sequence() != raw.frame_sequence
        || lifecycle.frame_sha256_v1() != raw.frame_sha256
        || lifecycle.client_bounds_v1() != &raw.client_bounds
        || lifecycle.snapshot_commitment_sha256() != raw.source_lifecycle_snapshot_commitment_sha256
    {
        return Err(error_v1(
            "competitive_deck_gate_source",
            "deck gate must bind the exact Event Browser lifecycle frame",
        ));
    }
    if raw.target_commitment_sha256 != target_commitment_sha256
        || raw.event_kind != target.event_kind
        || raw.event_identity_sha256 != target.event_identity_sha256
        || raw.event_display_label_sha256 != target.event_display_label_sha256
    {
        return Err(error_v1(
            "competitive_deck_gate_target_binding",
            "visible deck gate must match the exact event and deck target",
        ));
    }
    if !raw.select_deck_control_enabled
        || raw.confidence_bps < MIN_COMPETITIVE_DECK_GATE_CONFIDENCE_BPS_V1
        || raw.confidence_bps > 10_000
    {
        return Err(error_v1(
            "competitive_deck_gate_readiness",
            "event label and enabled deck control must be complete and high confidence",
        ));
    }

    let mut evidence_rects = vec![
        &raw.event_label_rect_client_px,
        &raw.select_deck_control_rect_client_px,
    ];
    match raw.state {
        MtgoCompetitiveDeckGateStateV1::AwaitingCompatibleDeckSelection => {
            if raw.missing_deck_prompt_rect_client_px.is_none()
                || raw.missing_deck_prompt_region_sha256.is_none()
                || raw.selected_deck_label_sha256.is_some()
                || raw.selected_deck_rect_client_px.is_some()
                || raw.selected_deck_region_sha256.is_some()
                || raw.open_entry_review_control_rect_client_px.is_some()
                || raw.open_entry_review_control_region_sha256.is_some()
                || raw.open_entry_review_control_enabled
            {
                return Err(error_v1(
                    "competitive_deck_gate_awaiting_state",
                    "awaiting-deck state requires the visible missing-deck prompt and no selected deck or Open Entry Review control",
                ));
            }
            evidence_rects.push(
                raw.missing_deck_prompt_rect_client_px
                    .as_ref()
                    .expect("checked above"),
            );
        }
        MtgoCompetitiveDeckGateStateV1::CompatibleDeckSelected => {
            if raw.missing_deck_prompt_rect_client_px.is_some()
                || raw.missing_deck_prompt_region_sha256.is_some()
                || raw.selected_deck_label_sha256.is_none()
                || raw.selected_deck_rect_client_px.is_none()
                || raw.selected_deck_region_sha256.is_none()
                || raw.open_entry_review_control_rect_client_px.is_none()
                || raw.open_entry_review_control_region_sha256.is_none()
                || !raw.open_entry_review_control_enabled
            {
                return Err(error_v1(
                    "competitive_deck_gate_ready_state",
                    "ready state requires one visible selected deck and one enabled Open Entry Review control",
                ));
            }
            evidence_rects.push(
                raw.selected_deck_rect_client_px
                    .as_ref()
                    .expect("checked above"),
            );
            evidence_rects.push(
                raw.open_entry_review_control_rect_client_px
                    .as_ref()
                    .expect("checked above"),
            );
        }
    }
    for rect in &evidence_rects {
        validate_rect_inside_v1(rect, &raw.client_bounds)?;
    }
    for first in 0..evidence_rects.len() {
        for second in (first + 1)..evidence_rects.len() {
            if rects_overlap_v1(evidence_rects[first], evidence_rects[second])? {
                return Err(error_v1(
                    "competitive_deck_gate_region_overlap",
                    "deck-gate evidence regions must not overlap",
                ));
            }
        }
    }

    let raw_bytes = serde_json::to_vec(&raw).map_err(|error| {
        error_v1(
            "competitive_deck_gate_serialization",
            format!("serialize deck gate: {error}"),
        )
    })?;
    let observation_commitment_sha256 = commitment_v1(
        COMPETITIVE_DECK_GATE_COMMITMENT_DOMAIN_V1,
        &[
            target_commitment_sha256.as_bytes(),
            lifecycle.snapshot_commitment_sha256().as_bytes(),
            raw_bytes.as_slice(),
            b"checked_untrusted_visible_pre_entry_deck_gate_no_input_no_entry_no_spending",
        ],
    );
    let state = raw.state;
    Ok(CheckedUntrustedMtgoCompetitiveDeckGateV1 {
        _lifecycle: lifecycle,
        _target: target,
        _raw: raw,
        target_commitment_sha256,
        observation_commitment_sha256,
        state,
    })
}

fn validate_rect_inside_v1(
    rect: &MtgoRectPxV1,
    bounds: &MtgoRectPxV1,
) -> Result<(), MtgoContractErrorV1> {
    let right = rect.x.checked_add(rect.width);
    let bottom = rect.y.checked_add(rect.height);
    let bounds_right = bounds.x.checked_add(bounds.width);
    let bounds_bottom = bounds.y.checked_add(bounds.height);
    if rect.width == 0
        || rect.height == 0
        || right.is_none()
        || bottom.is_none()
        || bounds_right.is_none()
        || bounds_bottom.is_none()
        || rect.x < bounds.x
        || rect.y < bounds.y
        || right > bounds_right
        || bottom > bounds_bottom
    {
        return Err(error_v1(
            "competitive_deck_gate_region_bounds",
            "deck-gate evidence region must be nonempty and inside the client",
        ));
    }
    Ok(())
}

fn rects_overlap_v1(
    first: &MtgoRectPxV1,
    second: &MtgoRectPxV1,
) -> Result<bool, MtgoContractErrorV1> {
    let first_right = first
        .x
        .checked_add(first.width)
        .ok_or_else(|| error_v1("competitive_deck_gate_region_overflow", "first rectangle"))?;
    let first_bottom = first
        .y
        .checked_add(first.height)
        .ok_or_else(|| error_v1("competitive_deck_gate_region_overflow", "first rectangle"))?;
    let second_right = second
        .x
        .checked_add(second.width)
        .ok_or_else(|| error_v1("competitive_deck_gate_region_overflow", "second rectangle"))?;
    let second_bottom = second
        .y
        .checked_add(second.height)
        .ok_or_else(|| error_v1("competitive_deck_gate_region_overflow", "second rectangle"))?;
    Ok(first.x < second_right
        && second.x < first_right
        && first.y < second_bottom
        && second.y < first_bottom)
}

fn validate_identifier_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(error_v1(
            "competitive_deck_gate_observation_id",
            "observation ID is not a safe bounded identifier",
        ));
    }
    Ok(())
}

fn validate_sha256_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1(
            code,
            "value must be 64 lowercase hexadecimal characters",
        ));
    }
    Ok(())
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        validate_competitive_deck_manifest_v1, validate_visible_competitive_lifecycle_snapshot_v1,
        MtgoCompetitiveDeckCardCountV1, MtgoCompetitiveDeckConfigurationV1,
        MtgoCompetitiveDeckManifestV1, MtgoLifecycleVisibleFactKindV1, MtgoLifecycleVisibleFactV1,
        MtgoVisibleCompetitiveLifecycleSnapshotV1, MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
        MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
    };
    use mtg_kernel::card_def::card_id_by_name;

    fn digest(byte: char) -> String {
        byte.to_string().repeat(64)
    }

    fn deck_v1() -> ValidatedMtgoCompetitiveDeckManifestV1 {
        let mut mainboard = vec![
            MtgoCompetitiveDeckCardCountV1 {
                card_db_id: card_id_by_name("Island").unwrap(),
                card_name: "Island".to_owned(),
                count: 30,
            },
            MtgoCompetitiveDeckCardCountV1 {
                card_db_id: card_id_by_name("Forest").unwrap(),
                card_name: "Forest".to_owned(),
                count: 30,
            },
        ];
        mainboard.sort_by_key(|card| card.card_db_id);
        validate_competitive_deck_manifest_v1(MtgoCompetitiveDeckManifestV1 {
            schema_version: MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
            deck_list_sha256: digest('0'),
            format_sha256: digest('1'),
            starting_mainboard_count: 60,
            starting_sideboard_count: 0,
            configuration: MtgoCompetitiveDeckConfigurationV1 {
                mainboard,
                sideboard: Vec::new(),
            },
        })
        .unwrap()
    }

    fn target_v1(
        deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    ) -> MtgoCompetitiveEventListingTargetV1 {
        MtgoCompetitiveEventListingTargetV1 {
            schema_version: 1,
            target_id: "modern-league-live-v1".to_owned(),
            event_kind: MtgoCompetitiveEventKindV1::League,
            approved_account_alias_sha256: digest('2'),
            event_identity_sha256: digest('3'),
            event_display_label_sha256: digest('4'),
            deck_list_sha256: deck.deck_list_sha256().to_owned(),
            deck_manifest_commitment_sha256: deck.manifest_commitment_sha256().to_owned(),
            deck_format_sha256: deck.format_sha256().to_owned(),
            policy_deployment_commitment_sha256: digest('5'),
        }
    }

    fn lifecycle_v1() -> CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1 {
        validate_visible_competitive_lifecycle_snapshot_v1(
            MtgoVisibleCompetitiveLifecycleSnapshotV1 {
                schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
                snapshot_id: "competitive-deck-gate-lifecycle-v1".to_owned(),
                event_kind: MtgoCompetitiveEventKindV1::League,
                phase: MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
                frame_id: 10,
                frame_sequence: 20,
                frame_sha256: digest('a'),
                client_bounds: MtgoRectPxV1 {
                    x: 0,
                    y: 0,
                    width: 1_550,
                    height: 925,
                },
                event_identity_sha256: None,
                match_identity_sha256: None,
                game_number: None,
                entry_terms: None,
                visible_state_complete: true,
                facts: vec![MtgoLifecycleVisibleFactV1 {
                    kind: MtgoLifecycleVisibleFactKindV1::EventBrowserVisible,
                    rect_client_px: MtgoRectPxV1 {
                        x: 20,
                        y: 20,
                        width: 100,
                        height: 30,
                    },
                    content_sha256: digest('b'),
                    confidence_bps: 10_000,
                }],
            },
        )
        .unwrap()
    }

    fn awaiting_v1(
        lifecycle: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
        deck: &ValidatedMtgoCompetitiveDeckManifestV1,
        target: &MtgoCompetitiveEventListingTargetV1,
    ) -> MtgoVisibleCompetitiveDeckGateV1 {
        MtgoVisibleCompetitiveDeckGateV1 {
            schema_version: MTGO_COMPETITIVE_DECK_GATE_SCHEMA_V1,
            observation_id: "competitive-deck-gate-awaiting-v1".to_owned(),
            target_commitment_sha256: competitive_event_listing_target_commitment_v1(target, deck)
                .unwrap(),
            event_kind: target.event_kind,
            event_identity_sha256: target.event_identity_sha256.clone(),
            event_display_label_sha256: target.event_display_label_sha256.clone(),
            source_lifecycle_snapshot_commitment_sha256: lifecycle
                .snapshot_commitment_sha256()
                .to_owned(),
            frame_id: lifecycle.frame_id_v1(),
            frame_sequence: lifecycle.frame_sequence(),
            frame_sha256: lifecycle.frame_sha256_v1().to_owned(),
            client_bounds: lifecycle.client_bounds_v1().clone(),
            state: MtgoCompetitiveDeckGateStateV1::AwaitingCompatibleDeckSelection,
            event_label_rect_client_px: MtgoRectPxV1 {
                x: 200,
                y: 100,
                width: 200,
                height: 50,
            },
            event_label_region_sha256: digest('6'),
            select_deck_control_rect_client_px: MtgoRectPxV1 {
                x: 500,
                y: 100,
                width: 50,
                height: 50,
            },
            select_deck_control_region_sha256: digest('7'),
            select_deck_control_enabled: true,
            missing_deck_prompt_rect_client_px: Some(MtgoRectPxV1 {
                x: 600,
                y: 100,
                width: 200,
                height: 50,
            }),
            missing_deck_prompt_region_sha256: Some(digest('8')),
            selected_deck_label_sha256: None,
            selected_deck_rect_client_px: None,
            selected_deck_region_sha256: None,
            open_entry_review_control_rect_client_px: None,
            open_entry_review_control_region_sha256: None,
            open_entry_review_control_enabled: false,
            confidence_bps: 10_000,
        }
    }

    #[test]
    fn awaiting_and_ready_states_are_distinct_and_non_authorizing() {
        let deck = deck_v1();
        let target = target_v1(&deck);
        let lifecycle = lifecycle_v1();
        let awaiting = validate_visible_competitive_deck_gate_v1(
            lifecycle,
            &deck,
            target,
            awaiting_v1(&lifecycle_v1(), &deck, &target_v1(&deck)),
        )
        .unwrap();
        assert_eq!(
            awaiting.state_v1(),
            MtgoCompetitiveDeckGateStateV1::AwaitingCompatibleDeckSelection
        );
        assert!(!awaiting.ready_for_open_entry_review_v1());
        assert!(!awaiting.permits_open_deck_chooser_v1());
        assert!(!awaiting.permits_open_entry_review_v1());
        assert!(!awaiting.permits_event_entry_v1());
        assert!(!awaiting.permits_spending_v1());
        assert!(!awaiting.safe_for_input_v1());

        let deck = deck_v1();
        let target = target_v1(&deck);
        let lifecycle = lifecycle_v1();
        let mut ready = awaiting_v1(&lifecycle, &deck, &target);
        ready.observation_id = "competitive-deck-gate-ready-v1".to_owned();
        ready.state = MtgoCompetitiveDeckGateStateV1::CompatibleDeckSelected;
        ready.missing_deck_prompt_rect_client_px = None;
        ready.missing_deck_prompt_region_sha256 = None;
        ready.selected_deck_label_sha256 = Some(digest('9'));
        ready.selected_deck_rect_client_px = Some(MtgoRectPxV1 {
            x: 600,
            y: 100,
            width: 200,
            height: 50,
        });
        ready.selected_deck_region_sha256 = Some(digest('c'));
        ready.open_entry_review_control_rect_client_px = Some(MtgoRectPxV1 {
            x: 900,
            y: 100,
            width: 200,
            height: 50,
        });
        ready.open_entry_review_control_region_sha256 = Some(digest('d'));
        ready.open_entry_review_control_enabled = true;
        let ready =
            validate_visible_competitive_deck_gate_v1(lifecycle, &deck, target, ready).unwrap();
        assert!(ready.ready_for_open_entry_review_v1());
        assert!(!ready.permits_open_entry_review_v1());
        assert!(!ready.safe_for_input_v1());
        assert_ne!(
            awaiting.observation_commitment_sha256_v1(),
            ready.observation_commitment_sha256_v1()
        );
    }

    #[test]
    fn contradictory_deck_gate_states_fail_closed() {
        let deck = deck_v1();
        let target = target_v1(&deck);
        let lifecycle = lifecycle_v1();
        let mut raw = awaiting_v1(&lifecycle, &deck, &target);
        raw.open_entry_review_control_enabled = true;
        assert!(validate_visible_competitive_deck_gate_v1(lifecycle, &deck, target, raw).is_err());

        let deck = deck_v1();
        let target = target_v1(&deck);
        let lifecycle = lifecycle_v1();
        let mut raw = awaiting_v1(&lifecycle, &deck, &target);
        raw.state = MtgoCompetitiveDeckGateStateV1::CompatibleDeckSelected;
        assert!(validate_visible_competitive_deck_gate_v1(lifecycle, &deck, target, raw).is_err());
    }

    #[test]
    fn crossed_identity_and_overlapping_or_out_of_bounds_regions_fail() {
        let deck = deck_v1();
        let target = target_v1(&deck);
        let lifecycle = lifecycle_v1();
        let mut crossed = awaiting_v1(&lifecycle, &deck, &target);
        crossed.event_identity_sha256 = digest('e');
        assert!(
            validate_visible_competitive_deck_gate_v1(lifecycle, &deck, target, crossed).is_err()
        );

        let deck = deck_v1();
        let target = target_v1(&deck);
        let lifecycle = lifecycle_v1();
        let mut overlapping = awaiting_v1(&lifecycle, &deck, &target);
        overlapping.missing_deck_prompt_rect_client_px = Some(MtgoRectPxV1 {
            x: 520,
            y: 100,
            width: 200,
            height: 50,
        });
        assert!(
            validate_visible_competitive_deck_gate_v1(lifecycle, &deck, target, overlapping)
                .is_err()
        );

        let deck = deck_v1();
        let target = target_v1(&deck);
        let lifecycle = lifecycle_v1();
        let mut outside = awaiting_v1(&lifecycle, &deck, &target);
        outside.select_deck_control_rect_client_px.x = 1_520;
        assert!(
            validate_visible_competitive_deck_gate_v1(lifecycle, &deck, target, outside).is_err()
        );
    }

    #[test]
    fn deck_gate_wire_rejects_unknown_fields() {
        let deck = deck_v1();
        let target = target_v1(&deck);
        let lifecycle = lifecycle_v1();
        let mut value = serde_json::to_value(awaiting_v1(&lifecycle, &deck, &target)).unwrap();
        value["process_memory"] = serde_json::json!(true);
        assert!(serde_json::from_value::<MtgoVisibleCompetitiveDeckGateV1>(value).is_err());
    }
}
