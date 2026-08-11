use super::{capture_commitment_v3, OpaqueMtgoDxgiFrameCandidateV3};
use crate::sha256_hex_v1;
use mtgo_blackbox_v1::{
    visible_frame_region_content_sha256_v1, CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    MtgoCompetitiveEntryResourceV1, MtgoCompetitiveEventKindV1, MtgoCompetitiveLifecyclePhaseV1,
    MtgoLifecycleVisibleFactKindV1, MtgoRectPxV1, MtgoSizePxV1,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

const OPAQUE_COMPETITIVE_ENTRY_REVIEW_IDENTITY_DOMAIN_V1: &[u8] =
    b"mtgo-opaque-competitive-entry-review-identity-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1 {
    pub source_capture_commitment_sha256: String,
    pub source_lifecycle_snapshot_commitment_sha256: String,
    pub source_window_title_sha256: String,
    pub event_label_region_sha256: String,
    pub source_identity_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub resource: MtgoCompetitiveEntryResourceV1,
    pub amount: u32,
}

/// Exact visible entry-review identity bound to one retained composed-desktop
/// navigation frame. The label remains a human-reviewed interpretation of its
/// committed pixel region. This value is move-only and has no input, entry, or
/// spending conversion.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveEntryReviewIdentityV1;
/// let _forged = OpaqueMtgoCompetitiveEntryReviewIdentityV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveEntryReviewIdentityV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveEntryReviewIdentityV1>();
/// ```
pub struct OpaqueMtgoCompetitiveEntryReviewIdentityV1 {
    _source_frame: OpaqueMtgoDxgiFrameCandidateV3,
    lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    event_display_label: String,
    commitments: MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1,
}

impl OpaqueMtgoCompetitiveEntryReviewIdentityV1 {
    pub fn commitments_v1(&self) -> MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub(crate) fn lifecycle_v1(&self) -> &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1 {
        &self.lifecycle
    }

    pub(crate) fn event_display_label_v1(&self) -> &str {
        &self.event_display_label
    }
}

/// Binds one checked entry-review interpretation to the exact retained pixels
/// of one composed-desktop main-client frame. Every lifecycle fact is rehashed
/// from those pixels. The output remains checked-untrusted because this does
/// not establish classifier accuracy or eliminate the transient-occluder race.
pub fn bind_opaque_navigation_frame_to_competitive_entry_review_identity_v1(
    source_frame: OpaqueMtgoDxgiFrameCandidateV3,
    lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    event_display_label: String,
    event_label_rect_client_px: MtgoRectPxV1,
) -> Result<OpaqueMtgoCompetitiveEntryReviewIdentityV1, String> {
    let recomputed_capture_commitment = capture_commitment_v3(
        &source_frame.manifest,
        &source_frame.canonical_bgra8,
        &source_frame.preview_png,
    )?;
    if recomputed_capture_commitment != source_frame.capture_commitment_sha256 {
        return Err("competitive entry source capture commitment changed".to_owned());
    }
    let manifest = &source_frame.manifest;
    if manifest.window_mode != "main_client"
        || manifest.capture_role != "navigation"
        || !manifest.expected_game_format.is_empty()
        || manifest.pre.title != manifest.post.title
        || manifest.safety.safe_for_semantic_evidence
        || manifest.safety.safe_for_ocr
        || manifest.safety.safe_for_policy_scoring
        || manifest.safety.safe_for_input
        || !manifest.safety.authenticode_verified_in_probe
    {
        return Err(
            "competitive entry review requires one exact non-actionable main-client navigation capture"
                .to_owned(),
        );
    }
    let source_capture = source_frame.commitments_v3();
    let commitments = bind_competitive_entry_review_source_parts_v1(
        &source_capture.capture_commitment_sha256,
        &source_capture.canonical_bgra8_sha256,
        source_capture.canonical_width,
        source_capture.canonical_height,
        &source_frame.canonical_bgra8,
        &manifest.pre.title,
        "main_client",
        "navigation",
        &lifecycle,
        &event_display_label,
        &event_label_rect_client_px,
    )?;
    Ok(OpaqueMtgoCompetitiveEntryReviewIdentityV1 {
        _source_frame: source_frame,
        lifecycle,
        event_display_label,
        commitments,
    })
}

#[allow(clippy::too_many_arguments)]
fn bind_competitive_entry_review_source_parts_v1(
    source_capture_commitment_sha256: &str,
    source_frame_sha256: &str,
    source_width: u32,
    source_height: u32,
    source_pixels: &[u8],
    source_window_title: &str,
    source_window_mode: &str,
    source_capture_role: &str,
    lifecycle: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    event_display_label: &str,
    event_label_rect_client_px: &MtgoRectPxV1,
) -> Result<MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1, String> {
    if source_window_mode != "main_client" || source_capture_role != "navigation" {
        return Err("competitive entry identity requires the navigation capture role".to_owned());
    }
    if lifecycle.phase() != MtgoCompetitiveLifecyclePhaseV1::EntryReview {
        return Err("competitive entry identity requires entry-review pixels".to_owned());
    }
    let client_bounds = lifecycle.client_bounds_v1();
    if lifecycle.frame_sha256_v1() != source_frame_sha256
        || client_bounds.x != 0
        || client_bounds.y != 0
        || client_bounds.width != source_width
        || client_bounds.height != source_height
    {
        return Err(
            "competitive entry lifecycle does not describe the exact opaque navigation frame"
                .to_owned(),
        );
    }
    let source_size = MtgoSizePxV1 {
        width: source_width,
        height: source_height,
    };
    for fact in lifecycle.visible_facts_v1() {
        let actual = visible_frame_region_content_sha256_v1(
            source_pixels,
            &source_size,
            &fact.rect_client_px,
        )
        .map_err(|error| format!("rehash competitive entry lifecycle fact pixels: {error}"))?;
        if actual != fact.content_sha256 {
            return Err(
                "competitive entry lifecycle fact does not match the retained source pixels"
                    .to_owned(),
            );
        }
    }
    validate_entry_display_label_v1(event_display_label, lifecycle.event_kind())?;
    if event_label_rect_client_px.width < 8 || event_label_rect_client_px.height < 8 {
        return Err("competitive entry label region is too small for human review".to_owned());
    }
    let review_surface = lifecycle
        .visible_facts_v1()
        .iter()
        .find(|fact| fact.kind == MtgoLifecycleVisibleFactKindV1::EntryReviewVisible)
        .map(|fact| &fact.rect_client_px)
        .ok_or("competitive entry lifecycle is missing its visible review surface")?;
    if !rect_contains_rect_v1(review_surface, event_label_rect_client_px)? {
        return Err(
            "competitive entry label region is outside the visible entry-review surface".to_owned(),
        );
    }
    let event_label_region_sha256 = visible_frame_region_content_sha256_v1(
        source_pixels,
        &source_size,
        event_label_rect_client_px,
    )
    .map_err(|error| format!("hash competitive entry label pixels: {error}"))?;
    let event_identity_sha256 = lifecycle
        .event_identity_sha256_v1()
        .ok_or("competitive entry lifecycle is missing its event identity")?;
    let entry_terms = lifecycle
        .entry_terms_v1()
        .ok_or("competitive entry lifecycle is missing its exact visible terms")?;
    let event_kind_json = canonical_json_v1(&lifecycle.event_kind(), "entry mode")?;
    let entry_terms_json = canonical_json_v1(entry_terms, "entry terms")?;
    let event_label_rect_json =
        canonical_json_v1(event_label_rect_client_px, "entry label region")?;
    let source_window_title_sha256 = sha256_hex_v1(source_window_title.as_bytes());
    let source_identity_commitment_sha256 = commitment_v1(
        OPAQUE_COMPETITIVE_ENTRY_REVIEW_IDENTITY_DOMAIN_V1,
        &[
            source_capture_commitment_sha256.as_bytes(),
            source_frame_sha256.as_bytes(),
            lifecycle.snapshot_commitment_sha256().as_bytes(),
            source_window_title_sha256.as_bytes(),
            event_display_label.as_bytes(),
            &event_label_rect_json,
            event_label_region_sha256.as_bytes(),
            &event_kind_json,
            event_identity_sha256.as_bytes(),
            &entry_terms_json,
            lifecycle.frame_id_v1().to_be_bytes().as_slice(),
            lifecycle.frame_sequence().to_be_bytes().as_slice(),
            b"opaque_composed_navigation_pixels_owner_review_only_no_entry_no_spending_no_input",
        ],
    );
    Ok(MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1 {
        source_capture_commitment_sha256: source_capture_commitment_sha256.to_owned(),
        source_lifecycle_snapshot_commitment_sha256: lifecycle
            .snapshot_commitment_sha256()
            .to_owned(),
        source_window_title_sha256,
        event_label_region_sha256,
        source_identity_commitment_sha256,
        event_kind: lifecycle.event_kind(),
        frame_id: lifecycle.frame_id_v1(),
        frame_sequence: lifecycle.frame_sequence(),
        resource: entry_terms.resource,
        amount: entry_terms.amount,
    })
}

fn validate_entry_display_label_v1(
    value: &str,
    event_kind: MtgoCompetitiveEventKindV1,
) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 160
        || value.trim() != value
        || !value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
    {
        return Err(
            "competitive entry label must be nonempty, trimmed, bounded ASCII display text"
                .to_owned(),
        );
    }
    let mode_word = match event_kind {
        MtgoCompetitiveEventKindV1::League => "league",
        MtgoCompetitiveEventKindV1::Challenge => "challenge",
    };
    if !value.to_ascii_lowercase().contains(mode_word) {
        return Err("competitive entry label does not identify the selected mode".to_owned());
    }
    Ok(())
}

fn rect_contains_rect_v1(outer: &MtgoRectPxV1, inner: &MtgoRectPxV1) -> Result<bool, String> {
    if inner.width == 0 || inner.height == 0 {
        return Ok(false);
    }
    let outer_right = outer
        .x
        .checked_add(outer.width)
        .ok_or("competitive entry outer rectangle overflow")?;
    let outer_bottom = outer
        .y
        .checked_add(outer.height)
        .ok_or("competitive entry outer rectangle overflow")?;
    let inner_right = inner
        .x
        .checked_add(inner.width)
        .ok_or("competitive entry inner rectangle overflow")?;
    let inner_bottom = inner
        .y
        .checked_add(inner.height)
        .ok_or("competitive entry inner rectangle overflow")?;
    Ok(inner.x >= outer.x
        && inner.y >= outer.y
        && inner_right <= outer_right
        && inner_bottom <= outer_bottom)
}

fn canonical_json_v1<T: Serialize>(value: &T, field: &str) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|error| format!("serialize competitive {field}: {error}"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use mtgo_blackbox_v1::{
        validate_visible_competitive_lifecycle_snapshot_v1, MtgoCompetitiveEntryTermsV1,
        MtgoLifecycleVisibleFactV1, MtgoVisibleCompetitiveLifecycleSnapshotV1,
        MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
    };

    fn source_v1(
        pixels: &[u8],
        event_kind: MtgoCompetitiveEventKindV1,
        resource: MtgoCompetitiveEntryResourceV1,
        amount: u32,
    ) -> CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1 {
        let size = MtgoSizePxV1 {
            width: 100,
            height: 100,
        };
        let review_rect = MtgoRectPxV1 {
            x: 5,
            y: 5,
            width: 80,
            height: 40,
        };
        let terms_rect = MtgoRectPxV1 {
            x: 10,
            y: 50,
            width: 70,
            height: 30,
        };
        validate_visible_competitive_lifecycle_snapshot_v1(
            MtgoVisibleCompetitiveLifecycleSnapshotV1 {
                schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
                snapshot_id: "opaque-entry-review-source-test-v1".to_owned(),
                event_kind,
                phase: MtgoCompetitiveLifecyclePhaseV1::EntryReview,
                frame_id: 7,
                frame_sequence: 11,
                frame_sha256: sha256_hex_v1(pixels),
                client_bounds: MtgoRectPxV1 {
                    x: 0,
                    y: 0,
                    width: 100,
                    height: 100,
                },
                event_identity_sha256: Some("2".repeat(64)),
                match_identity_sha256: None,
                game_number: None,
                entry_terms: Some(MtgoCompetitiveEntryTermsV1 {
                    terms_sha256: "3".repeat(64),
                    resource,
                    amount,
                }),
                visible_state_complete: true,
                facts: vec![
                    MtgoLifecycleVisibleFactV1 {
                        kind: MtgoLifecycleVisibleFactKindV1::EntryReviewVisible,
                        rect_client_px: review_rect.clone(),
                        content_sha256: visible_frame_region_content_sha256_v1(
                            pixels,
                            &size,
                            &review_rect,
                        )
                        .unwrap(),
                        confidence_bps: 10_000,
                    },
                    MtgoLifecycleVisibleFactV1 {
                        kind: MtgoLifecycleVisibleFactKindV1::EntryTermsVisible,
                        rect_client_px: terms_rect.clone(),
                        content_sha256: visible_frame_region_content_sha256_v1(
                            pixels,
                            &size,
                            &terms_rect,
                        )
                        .unwrap(),
                        confidence_bps: 10_000,
                    },
                ],
            },
        )
        .unwrap()
    }

    #[test]
    fn exact_navigation_pixels_bind_both_entry_modes_without_authority() {
        let pixels = vec![17_u8; 100 * 100 * 4];
        for (event_kind, label) in [
            (MtgoCompetitiveEventKindV1::League, "Modern League"),
            (MtgoCompetitiveEventKindV1::Challenge, "Modern Challenge"),
        ] {
            let source = source_v1(
                &pixels,
                event_kind,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            );
            let commitments = bind_competitive_entry_review_source_parts_v1(
                &"1".repeat(64),
                &sha256_hex_v1(&pixels),
                100,
                100,
                &pixels,
                "Magic: The Gathering Online",
                "main_client",
                "navigation",
                &source,
                label,
                &MtgoRectPxV1 {
                    x: 10,
                    y: 10,
                    width: 50,
                    height: 20,
                },
            )
            .unwrap();
            assert_eq!(commitments.event_kind, event_kind);
            assert_eq!(
                commitments.resource,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints
            );
            assert_eq!(commitments.amount, 100);
            assert_eq!(commitments.frame_id, 7);
            assert_eq!(commitments.frame_sequence, 11);
        }
    }

    #[test]
    fn role_frame_fact_label_and_terms_drift_fail_closed() {
        let mut pixels = vec![19_u8; 100 * 100 * 4];
        let source = source_v1(
            &pixels,
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEntryResourceV1::ExistingEventTickets,
            10,
        );
        let bind = |pixels: &[u8], role: &str, label: &str, rect: MtgoRectPxV1| {
            bind_competitive_entry_review_source_parts_v1(
                &"1".repeat(64),
                source.frame_sha256_v1(),
                100,
                100,
                pixels,
                "Magic: The Gathering Online",
                "main_client",
                role,
                &source,
                label,
                &rect,
            )
        };
        let valid_rect = MtgoRectPxV1 {
            x: 10,
            y: 10,
            width: 50,
            height: 20,
        };
        assert!(bind(
            &pixels,
            "acting_player_duel",
            "Modern League",
            valid_rect.clone()
        )
        .is_err());
        assert!(bind(
            &pixels,
            "navigation",
            "Modern Challenge",
            valid_rect.clone()
        )
        .is_err());
        assert!(bind(
            &pixels,
            "navigation",
            "Modern League",
            MtgoRectPxV1 {
                x: 90,
                y: 90,
                width: 10,
                height: 10,
            },
        )
        .is_err());
        pixels[5 * 100 * 4 + 5 * 4] ^= 1;
        assert!(bind(&pixels, "navigation", "Modern League", valid_rect).is_err());
    }
}
