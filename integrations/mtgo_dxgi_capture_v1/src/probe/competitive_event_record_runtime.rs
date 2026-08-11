use super::{sha256_hex_v1, OpaqueMtgoClassifiedCompetitiveNavigationFrameV1};
use mtgo_blackbox_v1::{
    validate_visible_competitive_event_record_v1, visible_frame_region_content_sha256_v1,
    CheckedUntrustedMtgoCompetitiveEventRecordV1, MtgoCompetitiveEventCompletionV1,
    MtgoCompetitiveEventKindV1, MtgoCompetitiveEventProgressV1,
    MtgoCompetitiveEventRecordVisibleFactV1, MtgoCompetitiveEventVisibleStatusV1, MtgoSizePxV1,
    MtgoVisibleCompetitiveEventRecordV1,
};
use sha2::{Digest, Sha256};

const SOURCE_BOUND_COMPETITIVE_EVENT_RECORD_DOMAIN_V1: &[u8] =
    b"mtgo-source-bound-competitive-event-record-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoSourceBoundCompetitiveEventRecordCommitmentsV1 {
    pub source_capture_commitment_sha256: String,
    pub source_frame_profile_binding_sha256: String,
    pub source_classification_result_commitment_sha256: String,
    pub source_lifecycle_snapshot_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub event_identity_sha256: String,
    pub event_record_commitment_sha256: String,
    pub source_binding_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub status: MtgoCompetitiveEventVisibleStatusV1,
}

/// One checked event record whose complete visible fact set has been rehashed
/// against the exact retained classifier frame. The frame and coordinates stay
/// private. The value exposes coordinate-free progress only and grants no
/// input, event entry, spending, or gameplay authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoSourceBoundCompetitiveEventRecordV1;
/// let _forged = OpaqueMtgoSourceBoundCompetitiveEventRecordV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoSourceBoundCompetitiveEventRecordV1;
/// fn cannot_extract(value: &OpaqueMtgoSourceBoundCompetitiveEventRecordV1) {
///     let _ = value.canonical_bgra8();
///     let _ = value.visible_fact_rectangles();
///     let _ = value.input_command();
/// }
/// ```
pub struct OpaqueMtgoSourceBoundCompetitiveEventRecordV1 {
    _source_frame: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    _record: CheckedUntrustedMtgoCompetitiveEventRecordV1,
    commitments: MtgoSourceBoundCompetitiveEventRecordCommitmentsV1,
}

impl OpaqueMtgoSourceBoundCompetitiveEventRecordV1 {
    pub fn commitments_v1(&self) -> MtgoSourceBoundCompetitiveEventRecordCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.commitments.event_kind
    }

    pub fn status_v1(&self) -> MtgoCompetitiveEventVisibleStatusV1 {
        self.commitments.status
    }

    pub fn progress_v1(&self) -> &MtgoCompetitiveEventProgressV1 {
        self._record.progress_v1()
    }

    pub fn completion_v1(&self) -> Option<MtgoCompetitiveEventCompletionV1> {
        self._record.completion_v1()
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

    pub fn permits_gameplay_v1(&self) -> bool {
        false
    }
}

/// Consumes one exact classified main-client frame and binds a visible League
/// or Challenge record only after rehashing every declared record fact against
/// that frame's retained composed-desktop pixels.
pub fn bind_classified_navigation_frame_to_visible_event_record_v1(
    source: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    record: MtgoVisibleCompetitiveEventRecordV1,
) -> Result<OpaqueMtgoSourceBoundCompetitiveEventRecordV1, String> {
    let source_commitments = source.commitments_v1();
    let approved_account_alias_sha256 = source_commitments
        .source_frame
        .approved_account_alias_sha256
        .clone();
    let checked = validate_visible_competitive_event_record_v1(
        &source._lifecycle,
        &approved_account_alias_sha256,
        record,
    )
    .map_err(|error| format!("validate visible competitive event record: {error}"))?;

    let retained = &source._source_frame.source_frame;
    let size = MtgoSizePxV1 {
        width: retained.manifest.frame.canonical_width,
        height: retained.manifest.frame.canonical_height,
    };
    validate_event_record_visible_fact_pixels_v1(
        &retained.canonical_bgra8,
        &size,
        checked.visible_facts_v1(),
    )?;
    if retained.capture_commitment_sha256
        != source_commitments
            .source_frame
            .source_capture
            .capture_commitment_sha256
        || sha256_hex_v1(&retained.canonical_bgra8)
            != source_commitments
                .source_frame
                .source_capture
                .canonical_bgra8_sha256
    {
        return Err("event-record source capture changed before pixel binding".to_owned());
    }

    let event_record_commitment_sha256 = checked.record_commitment_sha256_v1().to_owned();
    let event_identity_sha256 = checked.event_identity_sha256_v1().to_owned();
    let source_binding_commitment_sha256 = source_bound_event_record_commitment_v1(
        &source_commitments
            .source_frame
            .source_capture
            .capture_commitment_sha256,
        &source_commitments.source_frame.frame_profile_binding_sha256,
        &source_commitments.classification_result_commitment_sha256,
        checked.source_lifecycle_snapshot_commitment_sha256_v1(),
        checked.approved_account_alias_sha256_v1(),
        &event_identity_sha256,
        &event_record_commitment_sha256,
    );
    Ok(OpaqueMtgoSourceBoundCompetitiveEventRecordV1 {
        _source_frame: source,
        commitments: MtgoSourceBoundCompetitiveEventRecordCommitmentsV1 {
            source_capture_commitment_sha256: source_commitments
                .source_frame
                .source_capture
                .capture_commitment_sha256,
            source_frame_profile_binding_sha256: source_commitments
                .source_frame
                .frame_profile_binding_sha256,
            source_classification_result_commitment_sha256: source_commitments
                .classification_result_commitment_sha256,
            source_lifecycle_snapshot_commitment_sha256: checked
                .source_lifecycle_snapshot_commitment_sha256_v1()
                .to_owned(),
            approved_account_alias_sha256,
            event_identity_sha256,
            event_record_commitment_sha256,
            source_binding_commitment_sha256,
            event_kind: checked.event_kind_v1(),
            status: checked.status_v1(),
        },
        _record: checked,
    })
}

fn validate_event_record_visible_fact_pixels_v1(
    canonical_bgra8: &[u8],
    size: &MtgoSizePxV1,
    facts: &[MtgoCompetitiveEventRecordVisibleFactV1],
) -> Result<(), String> {
    let expected_len = u64::from(size.width)
        .checked_mul(u64::from(size.height))
        .and_then(|value| value.checked_mul(4))
        .ok_or("event-record source pixel length overflow")?;
    if u64::try_from(canonical_bgra8.len()).ok() != Some(expected_len) {
        return Err("event-record source pixels do not match the declared geometry".to_owned());
    }
    for fact in facts {
        let actual =
            visible_frame_region_content_sha256_v1(canonical_bgra8, size, &fact.rect_client_px)
                .map_err(|error| format!("rehash visible event-record fact: {error}"))?;
        if actual != fact.content_sha256 {
            return Err(
                "visible event-record fact does not match the retained source pixels".to_owned(),
            );
        }
    }
    Ok(())
}

fn source_bound_event_record_commitment_v1(
    source_capture_commitment_sha256: &str,
    source_frame_profile_binding_sha256: &str,
    source_classification_result_commitment_sha256: &str,
    source_lifecycle_snapshot_commitment_sha256: &str,
    approved_account_alias_sha256: &str,
    event_identity_sha256: &str,
    event_record_commitment_sha256: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(SOURCE_BOUND_COMPETITIVE_EVENT_RECORD_DOMAIN_V1);
    for part in [
        source_capture_commitment_sha256,
        source_frame_profile_binding_sha256,
        source_classification_result_commitment_sha256,
        source_lifecycle_snapshot_commitment_sha256,
        approved_account_alias_sha256,
        event_identity_sha256,
        event_record_commitment_sha256,
    ] {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    hasher.update(
        b"exact_same_frame_event_record_pixels_rehashed_no_input_no_entry_no_spending_no_gameplay",
    );
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtgo_blackbox_v1::{
        visible_frame_region_content_sha256_v1, MtgoCompetitiveEventRecordVisibleFactKindV1,
        MtgoRectPxV1,
    };

    #[test]
    fn event_record_fact_pixels_are_rehashed_exactly() {
        let size = MtgoSizePxV1 {
            width: 4,
            height: 2,
        };
        let mut pixels = vec![0_u8; 32];
        for (index, byte) in pixels.iter_mut().enumerate() {
            *byte = index as u8;
        }
        let rect = MtgoRectPxV1 {
            x: 1,
            y: 0,
            width: 2,
            height: 2,
        };
        let fact = MtgoCompetitiveEventRecordVisibleFactV1 {
            kind: MtgoCompetitiveEventRecordVisibleFactKindV1::EventProgressVisible,
            content_sha256: visible_frame_region_content_sha256_v1(&pixels, &size, &rect).unwrap(),
            rect_client_px: rect,
            confidence_bps: 10_000,
        };
        validate_event_record_visible_fact_pixels_v1(&pixels, &size, std::slice::from_ref(&fact))
            .unwrap();
        pixels[4] ^= 0xff;
        assert!(validate_event_record_visible_fact_pixels_v1(&pixels, &size, &[fact]).is_err());
    }

    #[test]
    fn event_record_source_binding_commitment_binds_every_lineage_digest() {
        let baseline = source_bound_event_record_commitment_v1(
            &"1".repeat(64),
            &"2".repeat(64),
            &"3".repeat(64),
            &"4".repeat(64),
            &"5".repeat(64),
            &"6".repeat(64),
            &"7".repeat(64),
        );
        let changed = source_bound_event_record_commitment_v1(
            &"1".repeat(64),
            &"2".repeat(64),
            &"3".repeat(64),
            &"4".repeat(64),
            &"5".repeat(64),
            &"6".repeat(64),
            &"8".repeat(64),
        );
        assert_eq!(baseline.len(), 64);
        assert_ne!(baseline, changed);
    }
}
