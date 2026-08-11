use super::{
    sha256_hex_v1, MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1,
    OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    OpaqueMtgoRetainedCompetitiveNavigationClassificationV1,
};
use mtgo_blackbox_v1::{
    validate_visible_competitive_event_listing_selection_v1,
    visible_frame_region_content_sha256_v1, CheckedUntrustedMtgoCompetitiveEventListingSelectionV1,
    CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1, MtgoCompetitiveEventKindV1,
    MtgoCompetitiveEventListingTargetV1, MtgoSizePxV1,
    MtgoVisibleCompetitiveEventListingSelectionV1, ValidatedMtgoCompetitiveDeckManifestV1,
};
use sha2::{Digest, Sha256};

const SOURCE_BOUND_EVENT_LISTING_DOMAIN_V1: &[u8] =
    b"mtgo-source-bound-competitive-event-listing-v1";

/// Coordinate-free telemetry for one selected Event Browser listing whose
/// declared regions were rehashed from an opaque classified navigation frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoSourceBoundCompetitiveEventListingCommitmentsV1 {
    pub source_navigation: MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1,
    pub target_commitment_sha256: String,
    pub selection_commitment_sha256: String,
    pub source_binding_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
}

/// One move-only Event Browser selection retained with its exact opaque
/// composed-desktop source and navigation-classifier lineage.
///
/// This type exposes commitments only. It has no pixel, rectangle, click,
/// entry-confirmation, spending, or input accessor.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoSourceBoundCompetitiveEventListingV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoSourceBoundCompetitiveEventListingV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoSourceBoundCompetitiveEventListingV1;
/// fn cannot_control(value: &OpaqueMtgoSourceBoundCompetitiveEventListingV1) {
///     let _ = value.canonical_bgra8();
///     let _ = value.control_rect_client_px();
///     let _ = value.click();
///     let _ = value.confirm_entry();
/// }
/// ```
pub struct OpaqueMtgoSourceBoundCompetitiveEventListingV1 {
    _source_frame: OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    _navigation_classification: OpaqueMtgoRetainedCompetitiveNavigationClassificationV1,
    selection: CheckedUntrustedMtgoCompetitiveEventListingSelectionV1,
    commitments: MtgoSourceBoundCompetitiveEventListingCommitmentsV1,
}

impl OpaqueMtgoSourceBoundCompetitiveEventListingV1 {
    pub fn commitments_v1(&self) -> MtgoSourceBoundCompetitiveEventListingCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.selection.event_kind_v1()
    }

    pub fn event_identity_sha256_v1(&self) -> &str {
        self.selection.event_identity_sha256_v1()
    }

    pub fn safe_for_event_listing_perception_v1(&self) -> bool {
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

/// Recomputes the lifecycle frame, every lifecycle fact, and both selected
/// listing regions from caller-supplied pixels before applying the blackbox
/// semantic contract. Success remains checked-untrusted because this function
/// cannot prove that the bytes came from the desktop capture backend.
pub fn check_untrusted_competitive_event_listing_pixels_v1(
    lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    target: MtgoCompetitiveEventListingTargetV1,
    raw: MtgoVisibleCompetitiveEventListingSelectionV1,
    expected_approved_account_alias_sha256: &str,
    canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoCompetitiveEventListingSelectionV1, String> {
    let bounds = lifecycle.client_bounds_v1();
    let size = MtgoSizePxV1 {
        width: bounds.width,
        height: bounds.height,
    };
    let expected_length = usize::try_from(
        u64::from(size.width)
            .checked_mul(u64::from(size.height))
            .and_then(|value| value.checked_mul(4))
            .ok_or("event-listing pixel length overflow")?,
    )
    .map_err(|_| "event-listing pixel length does not fit this process")?;
    if bounds.x != 0
        || bounds.y != 0
        || canonical_bgra8.len() != expected_length
        || sha256_hex_v1(canonical_bgra8) != lifecycle.frame_sha256_v1()
    {
        return Err(
            "event-listing pixels do not match the exact lifecycle frame and geometry".to_owned(),
        );
    }
    for fact in lifecycle.visible_facts_v1() {
        let actual =
            visible_frame_region_content_sha256_v1(canonical_bgra8, &size, &fact.rect_client_px)
                .map_err(|error| format!("rehash event-listing lifecycle fact: {error}"))?;
        if actual != fact.content_sha256 {
            return Err(
                "event-listing lifecycle fact does not match the supplied pixels".to_owned(),
            );
        }
    }
    let actual_label = visible_frame_region_content_sha256_v1(
        canonical_bgra8,
        &size,
        &raw.event_label_rect_client_px,
    )
    .map_err(|error| format!("rehash selected event label: {error}"))?;
    let actual_control = visible_frame_region_content_sha256_v1(
        canonical_bgra8,
        &size,
        &raw.open_entry_review_control_rect_client_px,
    )
    .map_err(|error| format!("rehash selected event control: {error}"))?;
    if actual_label != raw.event_label_region_sha256
        || actual_control != raw.open_entry_review_control_region_sha256
    {
        return Err(
            "selected event label or enabled control does not match the supplied pixels".to_owned(),
        );
    }
    let checked =
        validate_visible_competitive_event_listing_selection_v1(lifecycle, deck, target, raw)
            .map_err(|error| format!("validate selected competitive event listing: {error}"))?;
    if checked.approved_account_alias_sha256_v1() != expected_approved_account_alias_sha256 {
        return Err("selected event listing does not bind the approved account".to_owned());
    }
    Ok(checked)
}

/// Consumes one exact opaque navigation-classifier result and retains the
/// selected listing only after its visible regions are rehashed from the same
/// private composed-desktop pixels. This is a perception boundary only.
pub fn bind_classified_navigation_frame_to_competitive_event_listing_v1(
    classified: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    target: MtgoCompetitiveEventListingTargetV1,
    raw: MtgoVisibleCompetitiveEventListingSelectionV1,
) -> Result<OpaqueMtgoSourceBoundCompetitiveEventListingV1, String> {
    let source_navigation = classified.commitments_v1();
    let (source_frame, lifecycle, navigation_classification) =
        classified.into_event_listing_parts_v1();
    let source_frame_commitments = source_frame.commitments_v1();
    if source_navigation.source_frame != source_frame_commitments {
        return Err("selected event listing source-frame lineage changed".to_owned());
    }
    let selection = check_untrusted_competitive_event_listing_pixels_v1(
        lifecycle,
        deck,
        target,
        raw,
        &source_frame.approved_account_alias_sha256,
        &source_frame.source_frame.canonical_bgra8,
    )?;
    let target_commitment_sha256 = selection.target_commitment_sha256_v1().to_owned();
    let selection_commitment_sha256 = selection.selection_commitment_sha256_v1().to_owned();
    let event_kind = selection.event_kind_v1();
    let event_identity_sha256 = selection.event_identity_sha256_v1().to_owned();
    let deck_list_sha256 = selection.deck_list_sha256_v1().to_owned();
    let deck_manifest_commitment_sha256 = selection.deck_manifest_commitment_sha256_v1().to_owned();
    let deck_format_sha256 = selection.deck_format_sha256_v1().to_owned();
    let policy_deployment_commitment_sha256 = selection
        .policy_deployment_commitment_sha256_v1()
        .to_owned();
    let event_kind_json = serde_json::to_vec(&event_kind)
        .map_err(|error| format!("serialize selected event kind: {error}"))?;
    let source_binding_commitment_sha256 = commitment_v1(
        SOURCE_BOUND_EVENT_LISTING_DOMAIN_V1,
        &[
            source_navigation
                .source_frame
                .profile_commitment_sha256
                .as_bytes(),
            source_navigation
                .source_frame
                .profile_admission_commitment_sha256
                .as_bytes(),
            source_navigation
                .source_frame
                .approved_account_alias_sha256
                .as_bytes(),
            source_navigation
                .source_frame
                .frame_profile_binding_sha256
                .as_bytes(),
            source_navigation
                .classification_result_commitment_sha256
                .as_bytes(),
            target_commitment_sha256.as_bytes(),
            selection_commitment_sha256.as_bytes(),
            deck_list_sha256.as_bytes(),
            deck_manifest_commitment_sha256.as_bytes(),
            deck_format_sha256.as_bytes(),
            policy_deployment_commitment_sha256.as_bytes(),
            event_kind_json.as_slice(),
            b"opaque_current_pixels_no_open_review_no_entry_no_spending_no_input",
        ],
    );
    Ok(OpaqueMtgoSourceBoundCompetitiveEventListingV1 {
        _source_frame: source_frame,
        _navigation_classification: navigation_classification,
        selection,
        commitments: MtgoSourceBoundCompetitiveEventListingCommitmentsV1 {
            source_navigation,
            target_commitment_sha256,
            selection_commitment_sha256,
            source_binding_commitment_sha256,
            event_kind,
            event_identity_sha256,
            deck_list_sha256,
            deck_manifest_commitment_sha256,
            deck_format_sha256,
            policy_deployment_commitment_sha256,
        },
    })
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtgo_blackbox_v1::{
        competitive_event_listing_target_commitment_v1, validate_competitive_deck_manifest_v1,
        validate_visible_competitive_lifecycle_snapshot_v1, MtgoCompetitiveDeckCardCountV1,
        MtgoCompetitiveDeckConfigurationV1, MtgoCompetitiveDeckManifestV1,
        MtgoCompetitiveLifecyclePhaseV1, MtgoLifecycleVisibleFactKindV1,
        MtgoLifecycleVisibleFactV1, MtgoRectPxV1, MtgoVisibleCompetitiveLifecycleSnapshotV1,
        MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1, MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
        MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
    };

    fn digest(byte: char) -> String {
        byte.to_string().repeat(64)
    }

    fn pixels_v1() -> Vec<u8> {
        (0..32 * 16 * 4)
            .map(|index| ((index * 29 + 17) % 251) as u8)
            .collect()
    }

    fn rect_hash_v1(pixels: &[u8], rect: &MtgoRectPxV1) -> String {
        visible_frame_region_content_sha256_v1(
            pixels,
            &MtgoSizePxV1 {
                width: 32,
                height: 16,
            },
            rect,
        )
        .unwrap()
    }

    fn deck_v1() -> ValidatedMtgoCompetitiveDeckManifestV1 {
        validate_competitive_deck_manifest_v1(MtgoCompetitiveDeckManifestV1 {
            schema_version: MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
            deck_list_sha256: digest('1'),
            format_sha256: digest('2'),
            starting_mainboard_count: 5,
            starting_sideboard_count: 2,
            configuration: MtgoCompetitiveDeckConfigurationV1 {
                mainboard: vec![
                    MtgoCompetitiveDeckCardCountV1 {
                        card_db_id: 66,
                        card_name: "Lightning Bolt".to_owned(),
                        count: 2,
                    },
                    MtgoCompetitiveDeckCardCountV1 {
                        card_db_id: 76,
                        card_name: "Mountain".to_owned(),
                        count: 3,
                    },
                ],
                sideboard: vec![MtgoCompetitiveDeckCardCountV1 {
                    card_db_id: 101,
                    card_name: "Searing Blaze".to_owned(),
                    count: 2,
                }],
            },
        })
        .unwrap()
    }

    struct FixtureV1 {
        lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
        deck: ValidatedMtgoCompetitiveDeckManifestV1,
        target: MtgoCompetitiveEventListingTargetV1,
        raw: MtgoVisibleCompetitiveEventListingSelectionV1,
        pixels: Vec<u8>,
    }

    fn fixture_v1() -> FixtureV1 {
        fixture_with_browser_fact_v1(None)
    }

    fn fixture_with_browser_fact_v1(browser_fact_sha256: Option<String>) -> FixtureV1 {
        let pixels = pixels_v1();
        let client_bounds = MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: 32,
            height: 16,
        };
        let browser_rect = MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: 4,
            height: 4,
        };
        let label_rect = MtgoRectPxV1 {
            x: 5,
            y: 4,
            width: 8,
            height: 4,
        };
        let control_rect = MtgoRectPxV1 {
            x: 20,
            y: 4,
            width: 8,
            height: 4,
        };
        let lifecycle = validate_visible_competitive_lifecycle_snapshot_v1(
            MtgoVisibleCompetitiveLifecycleSnapshotV1 {
                schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
                snapshot_id: "source-bound-event-browser-v1".to_owned(),
                event_kind: MtgoCompetitiveEventKindV1::League,
                phase: MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
                frame_id: 7,
                frame_sequence: 11,
                frame_sha256: sha256_hex_v1(&pixels),
                client_bounds: client_bounds.clone(),
                event_identity_sha256: None,
                match_identity_sha256: None,
                game_number: None,
                entry_terms: None,
                visible_state_complete: true,
                facts: vec![MtgoLifecycleVisibleFactV1 {
                    kind: MtgoLifecycleVisibleFactKindV1::EventBrowserVisible,
                    rect_client_px: browser_rect.clone(),
                    content_sha256: browser_fact_sha256
                        .unwrap_or_else(|| rect_hash_v1(&pixels, &browser_rect)),
                    confidence_bps: 10_000,
                }],
            },
        )
        .unwrap();
        let deck = deck_v1();
        let target = MtgoCompetitiveEventListingTargetV1 {
            schema_version: MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1,
            target_id: "source-bound-modern-league-v1".to_owned(),
            event_kind: MtgoCompetitiveEventKindV1::League,
            approved_account_alias_sha256: digest('a'),
            event_identity_sha256: digest('3'),
            event_display_label_sha256: digest('4'),
            deck_list_sha256: deck.deck_list_sha256().to_owned(),
            deck_manifest_commitment_sha256: deck.manifest_commitment_sha256().to_owned(),
            deck_format_sha256: deck.format_sha256().to_owned(),
            policy_deployment_commitment_sha256: digest('5'),
        };
        let target_commitment_sha256 =
            competitive_event_listing_target_commitment_v1(&target, &deck).unwrap();
        let raw = MtgoVisibleCompetitiveEventListingSelectionV1 {
            schema_version: MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1,
            selection_id: "source-bound-event-listing-v1".to_owned(),
            target_commitment_sha256,
            event_kind: MtgoCompetitiveEventKindV1::League,
            event_identity_sha256: target.event_identity_sha256.clone(),
            event_display_label_sha256: target.event_display_label_sha256.clone(),
            source_lifecycle_snapshot_commitment_sha256: lifecycle
                .snapshot_commitment_sha256()
                .to_owned(),
            frame_id: lifecycle.frame_id_v1(),
            frame_sequence: lifecycle.frame_sequence(),
            frame_sha256: lifecycle.frame_sha256_v1().to_owned(),
            client_bounds,
            event_label_rect_client_px: label_rect.clone(),
            event_label_region_sha256: rect_hash_v1(&pixels, &label_rect),
            open_entry_review_control_rect_client_px: control_rect.clone(),
            open_entry_review_control_region_sha256: rect_hash_v1(&pixels, &control_rect),
            open_entry_review_control_enabled: true,
            confidence_bps: 10_000,
        };
        FixtureV1 {
            lifecycle,
            deck,
            target,
            raw,
            pixels,
        }
    }

    #[test]
    fn exact_current_pixels_create_only_a_checked_untrusted_selection() {
        let fixture = fixture_v1();
        let checked = check_untrusted_competitive_event_listing_pixels_v1(
            fixture.lifecycle,
            &fixture.deck,
            fixture.target,
            fixture.raw,
            &digest('a'),
            &fixture.pixels,
        )
        .unwrap();
        assert_eq!(checked.event_kind_v1(), MtgoCompetitiveEventKindV1::League);
        assert!(!checked.safe_for_input_v1());
        assert!(!checked.permits_event_entry_v1());
        assert!(!checked.permits_spending_v1());
    }

    #[test]
    fn full_frame_lifecycle_fact_and_selected_regions_are_rehashed() {
        let mut full_frame = fixture_v1();
        full_frame.pixels[100] ^= 0xff;
        assert!(check_untrusted_competitive_event_listing_pixels_v1(
            full_frame.lifecycle,
            &full_frame.deck,
            full_frame.target,
            full_frame.raw,
            &digest('a'),
            &full_frame.pixels,
        )
        .err()
        .unwrap()
        .contains("exact lifecycle frame"));

        let lifecycle_fact = fixture_with_browser_fact_v1(Some(digest('d')));
        assert!(check_untrusted_competitive_event_listing_pixels_v1(
            lifecycle_fact.lifecycle,
            &lifecycle_fact.deck,
            lifecycle_fact.target,
            lifecycle_fact.raw,
            &digest('a'),
            &lifecycle_fact.pixels,
        )
        .err()
        .unwrap()
        .contains("lifecycle fact"));

        let mut label = fixture_v1();
        label.raw.event_label_region_sha256 = digest('f');
        assert!(check_untrusted_competitive_event_listing_pixels_v1(
            label.lifecycle,
            &label.deck,
            label.target,
            label.raw,
            &digest('a'),
            &label.pixels,
        )
        .err()
        .unwrap()
        .contains("selected event label"));

        let mut control = fixture_v1();
        control.raw.open_entry_review_control_region_sha256 = digest('e');
        assert!(check_untrusted_competitive_event_listing_pixels_v1(
            control.lifecycle,
            &control.deck,
            control.target,
            control.raw,
            &digest('a'),
            &control.pixels,
        )
        .err()
        .unwrap()
        .contains("enabled control"));
    }

    #[test]
    fn exact_approved_account_is_required() {
        let fixture = fixture_v1();
        assert!(check_untrusted_competitive_event_listing_pixels_v1(
            fixture.lifecycle,
            &fixture.deck,
            fixture.target,
            fixture.raw,
            &digest('b'),
            &fixture.pixels,
        )
        .err()
        .unwrap()
        .contains("approved account"));
    }
}
