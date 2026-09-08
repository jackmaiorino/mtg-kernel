use crate::{
    CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1, MtgoCompetitiveEventKindV1,
    MtgoCompetitiveLifecyclePhaseV1, MtgoContractErrorV1, MtgoRectPxV1,
    MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_COMPETITIVE_EVENT_RECORD_SCHEMA_V1: u32 = 1;
const MIN_EVENT_RECORD_CONFIDENCE_BPS_V1: u16 = 9_500;
const EVENT_RECORD_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-visible-competitive-event-record-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveEventVisibleStatusV1 {
    WaitingForPairing,
    PairingReady,
    BetweenMatches,
    EventComplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveEventCompletionV1 {
    Completed,
    Dropped,
    Eliminated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveMatchRecordV1 {
    pub wins: u16,
    pub losses: u16,
    pub draws: u16,
    pub matches_completed: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoCompetitiveEventProgressV1 {
    League {
        match_record: MtgoCompetitiveMatchRecordV1,
        matches_total: Option<u16>,
    },
    Challenge {
        match_record: MtgoCompetitiveMatchRecordV1,
        rounds_completed: u16,
        rounds_total: u16,
        match_points: u16,
        standing_rank: Option<u32>,
        field_size: Option<u32>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveEventRecordVisibleFactKindV1 {
    EventStatusVisible,
    EventProgressVisible,
    EventStandingVisible,
    EventResultVisible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveEventRecordVisibleFactV1 {
    pub kind: MtgoCompetitiveEventRecordVisibleFactKindV1,
    pub rect_client_px: MtgoRectPxV1,
    pub content_sha256: String,
    pub confidence_bps: u16,
}

/// A coordinate-bearing interpretation of one visible event summary. This is
/// observation data only. Validation binds it to an exact checked lifecycle
/// snapshot and approved account digest, but does not prove the labels match
/// the retained pixels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleCompetitiveEventRecordV1 {
    pub schema_version: u32,
    pub record_id: String,
    pub source_lifecycle_snapshot_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub lifecycle_phase: MtgoCompetitiveLifecyclePhaseV1,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub frame_sha256: String,
    pub client_bounds: MtgoRectPxV1,
    pub event_identity_sha256: String,
    pub status: MtgoCompetitiveEventVisibleStatusV1,
    pub progress: MtgoCompetitiveEventProgressV1,
    pub completion: Option<MtgoCompetitiveEventCompletionV1>,
    pub visible_record_complete: bool,
    pub facts: Vec<MtgoCompetitiveEventRecordVisibleFactV1>,
}

/// A structurally checked, still untrusted event record. It exposes only
/// coordinate-free progress and pixel commitments for a later opaque source
/// rehash. It has no capture, input, entry, spending, or gameplay conversion.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoCompetitiveEventRecordV1;
/// let _forged = CheckedUntrustedMtgoCompetitiveEventRecordV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoCompetitiveEventRecordV1;
/// fn cannot_act(value: &CheckedUntrustedMtgoCompetitiveEventRecordV1) {
///     let _ = value.input_command();
///     let _ = value.enter_event();
///     let _ = value.spend_resources();
/// }
/// ```
pub struct CheckedUntrustedMtgoCompetitiveEventRecordV1 {
    record: MtgoVisibleCompetitiveEventRecordV1,
    record_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveEventRecordV1 {
    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.record.event_kind
    }

    pub fn status_v1(&self) -> MtgoCompetitiveEventVisibleStatusV1 {
        self.record.status
    }

    pub fn progress_v1(&self) -> &MtgoCompetitiveEventProgressV1 {
        &self.record.progress
    }

    pub fn completion_v1(&self) -> Option<MtgoCompetitiveEventCompletionV1> {
        self.record.completion
    }

    pub fn source_lifecycle_snapshot_commitment_sha256_v1(&self) -> &str {
        &self.record.source_lifecycle_snapshot_commitment_sha256
    }

    pub fn approved_account_alias_sha256_v1(&self) -> &str {
        &self.record.approved_account_alias_sha256
    }

    pub fn event_identity_sha256_v1(&self) -> &str {
        &self.record.event_identity_sha256
    }

    pub fn record_commitment_sha256_v1(&self) -> &str {
        &self.record_commitment_sha256
    }

    /// The rectangles and hashes remain non-authorizing. A later opaque frame
    /// owner must recompute every hash before using any parsed progress value.
    pub fn visible_facts_v1(&self) -> &[MtgoCompetitiveEventRecordVisibleFactV1] {
        &self.record.facts
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

pub fn validate_visible_competitive_event_record_v1(
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    approved_account_alias_sha256: &str,
    record: MtgoVisibleCompetitiveEventRecordV1,
) -> Result<CheckedUntrustedMtgoCompetitiveEventRecordV1, MtgoContractErrorV1> {
    if record.schema_version != MTGO_COMPETITIVE_EVENT_RECORD_SCHEMA_V1
        || record.schema_version != MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1
    {
        return Err(error(
            "event_record_schema",
            "the visible event record schema must be exactly version 1",
        ));
    }
    validate_identifier("event_record_id", &record.record_id)?;
    validate_sha256(
        "event_record_source_lifecycle",
        &record.source_lifecycle_snapshot_commitment_sha256,
    )?;
    validate_sha256(
        "event_record_account_alias",
        &record.approved_account_alias_sha256,
    )?;
    validate_sha256("event_record_frame", &record.frame_sha256)?;
    validate_sha256("event_record_event_identity", &record.event_identity_sha256)?;
    if !record.visible_record_complete {
        return Err(error(
            "event_record_incomplete",
            "the visible event record must be declared complete",
        ));
    }
    validate_source_binding(source, approved_account_alias_sha256, &record)?;
    validate_status(record.lifecycle_phase, record.status)?;
    validate_progress(record.event_kind, &record.progress)?;
    validate_completion(record.status, record.completion, &record.progress)?;
    validate_visible_facts(&record)?;

    let bytes = serde_json::to_vec(&record)
        .map_err(|value| error("event_record_serialization", value.to_string()))?;
    let record_commitment_sha256 = commitment(EVENT_RECORD_COMMITMENT_DOMAIN_V1, &bytes);
    Ok(CheckedUntrustedMtgoCompetitiveEventRecordV1 {
        record,
        record_commitment_sha256,
    })
}

fn validate_source_binding(
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    approved_account_alias_sha256: &str,
    record: &MtgoVisibleCompetitiveEventRecordV1,
) -> Result<(), MtgoContractErrorV1> {
    validate_sha256(
        "approved_account_alias_sha256",
        approved_account_alias_sha256,
    )?;
    if record.approved_account_alias_sha256 != approved_account_alias_sha256
        || record.source_lifecycle_snapshot_commitment_sha256 != source.snapshot_commitment_sha256()
        || record.event_kind != source.event_kind()
        || record.lifecycle_phase != source.phase()
        || record.frame_id != source.frame_id_v1()
        || record.frame_sequence != source.frame_sequence()
        || record.frame_sha256 != source.frame_sha256_v1()
        || &record.client_bounds != source.client_bounds_v1()
        || Some(record.event_identity_sha256.as_str()) != source.event_identity_sha256_v1()
    {
        return Err(error(
            "event_record_source_binding",
            "the event record must bind the exact lifecycle frame, event, and approved account",
        ));
    }
    Ok(())
}

fn validate_status(
    phase: MtgoCompetitiveLifecyclePhaseV1,
    status: MtgoCompetitiveEventVisibleStatusV1,
) -> Result<(), MtgoContractErrorV1> {
    let valid = matches!(
        (phase, status),
        (
            MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing,
            MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing
        ) | (
            MtgoCompetitiveLifecyclePhaseV1::PairingReady,
            MtgoCompetitiveEventVisibleStatusV1::PairingReady
        ) | (
            MtgoCompetitiveLifecyclePhaseV1::MatchComplete,
            MtgoCompetitiveEventVisibleStatusV1::BetweenMatches
        ) | (
            MtgoCompetitiveLifecyclePhaseV1::EventComplete,
            MtgoCompetitiveEventVisibleStatusV1::EventComplete
        )
    );
    if !valid {
        return Err(error(
            "event_record_status",
            "the visible event status does not match an event-summary lifecycle phase",
        ));
    }
    Ok(())
}

fn validate_progress(
    event_kind: MtgoCompetitiveEventKindV1,
    progress: &MtgoCompetitiveEventProgressV1,
) -> Result<(), MtgoContractErrorV1> {
    match (event_kind, progress) {
        (
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventProgressV1::League {
                match_record,
                matches_total,
            },
        ) => {
            validate_match_record(match_record)?;
            if matches_total.is_some_and(|total| {
                total == 0 || total > 128 || total < match_record.matches_completed
            }) {
                return Err(error(
                    "event_record_league_total",
                    "a visible League match total must contain the completed record",
                ));
            }
        }
        (
            MtgoCompetitiveEventKindV1::Challenge,
            MtgoCompetitiveEventProgressV1::Challenge {
                match_record,
                rounds_completed,
                rounds_total,
                match_points: _,
                standing_rank,
                field_size,
            },
        ) => {
            validate_match_record(match_record)?;
            if *rounds_total == 0
                || *rounds_total > 64
                || rounds_completed > rounds_total
                || match_record.matches_completed > *rounds_completed
                || match_record.matches_completed > *rounds_total
            {
                return Err(error(
                    "event_record_challenge_rounds",
                    "visible Challenge round progress is internally inconsistent",
                ));
            }
            match (standing_rank, field_size) {
                (Some(rank), Some(field)) if *rank > 0 && *field > 0 && rank <= field => {}
                (None, None) => {}
                _ => {
                    return Err(error(
                        "event_record_challenge_standing",
                        "visible Challenge rank and field size must be absent together or form a valid standing",
                    ));
                }
            }
        }
        _ => {
            return Err(error(
                "event_record_mode",
                "League and Challenge records cannot exchange progress schemas",
            ));
        }
    }
    Ok(())
}

fn validate_match_record(record: &MtgoCompetitiveMatchRecordV1) -> Result<(), MtgoContractErrorV1> {
    let completed = record
        .wins
        .checked_add(record.losses)
        .and_then(|value| value.checked_add(record.draws))
        .ok_or_else(|| {
            error(
                "event_record_match_record",
                "visible match record arithmetic overflowed",
            )
        })?;
    if completed != record.matches_completed || completed > 128 {
        return Err(error(
            "event_record_match_record",
            "wins, losses, and draws must sum to the visible completed-match count",
        ));
    }
    Ok(())
}

fn validate_completion(
    status: MtgoCompetitiveEventVisibleStatusV1,
    completion: Option<MtgoCompetitiveEventCompletionV1>,
    progress: &MtgoCompetitiveEventProgressV1,
) -> Result<(), MtgoContractErrorV1> {
    if (status == MtgoCompetitiveEventVisibleStatusV1::EventComplete) != completion.is_some() {
        return Err(error(
            "event_record_completion",
            "a completion reason must appear exactly in the visible completed-event state",
        ));
    }
    if completion == Some(MtgoCompetitiveEventCompletionV1::Completed) {
        let reached_declared_end = match progress {
            MtgoCompetitiveEventProgressV1::League {
                match_record,
                matches_total,
            } => matches_total.is_none_or(|total| match_record.matches_completed == total),
            MtgoCompetitiveEventProgressV1::Challenge {
                rounds_completed,
                rounds_total,
                ..
            } => rounds_completed == rounds_total,
        };
        if !reached_declared_end {
            return Err(error(
                "event_record_completion_progress",
                "a normally completed event must reach its visible declared total",
            ));
        }
    }
    Ok(())
}

fn validate_visible_facts(
    record: &MtgoVisibleCompetitiveEventRecordV1,
) -> Result<(), MtgoContractErrorV1> {
    if record.facts.len() < 2 || record.facts.len() > 4 {
        return Err(error(
            "event_record_fact_count",
            "a complete event record must contain between two and four visible facts",
        ));
    }
    let mut kinds = HashSet::new();
    for fact in &record.facts {
        if !kinds.insert(fact.kind) {
            return Err(error(
                "event_record_fact_duplicate",
                "each event-record visible fact kind must occur at most once",
            ));
        }
        validate_sha256("event_record_fact.content_sha256", &fact.content_sha256)?;
        if fact.confidence_bps < MIN_EVENT_RECORD_CONFIDENCE_BPS_V1 || fact.confidence_bps > 10_000
        {
            return Err(error(
                "event_record_fact_confidence",
                "every event-record fact must meet the confidence floor",
            ));
        }
        validate_rect_inside(&fact.rect_client_px, &record.client_bounds)?;
    }
    use MtgoCompetitiveEventRecordVisibleFactKindV1::*;
    if !kinds.contains(&EventStatusVisible) || !kinds.contains(&EventProgressVisible) {
        return Err(error(
            "event_record_required_fact",
            "event status and progress must both be visibly sourced",
        ));
    }
    let ranked = matches!(
        &record.progress,
        MtgoCompetitiveEventProgressV1::Challenge {
            standing_rank: Some(_),
            field_size: Some(_),
            ..
        }
    );
    if ranked != kinds.contains(&EventStandingVisible) {
        return Err(error(
            "event_record_standing_fact",
            "a visible standing region must appear exactly when rank and field size are present",
        ));
    }
    let complete = record.status == MtgoCompetitiveEventVisibleStatusV1::EventComplete;
    if complete != kinds.contains(&EventResultVisible) {
        return Err(error(
            "event_record_result_fact",
            "a visible result region must appear exactly for a completed event",
        ));
    }
    Ok(())
}

fn validate_rect_inside(
    rect: &MtgoRectPxV1,
    bounds: &MtgoRectPxV1,
) -> Result<(), MtgoContractErrorV1> {
    let right = rect.x.checked_add(rect.width);
    let bottom = rect.y.checked_add(rect.height);
    if rect.width == 0
        || rect.height == 0
        || right.is_none()
        || bottom.is_none()
        || right.unwrap() > bounds.width
        || bottom.unwrap() > bounds.height
    {
        return Err(error(
            "event_record_fact_bounds",
            "every event-record fact must be nonempty and inside the client",
        ));
    }
    Ok(())
}

fn validate_identifier(field: &'static str, value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(error(field, "identifier syntax is invalid"));
    }
    Ok(())
}

fn validate_sha256(field: &'static str, value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error(field, "expected lowercase SHA-256 hex"));
    }
    Ok(())
}

fn commitment(domain: &[u8], payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update((payload.len() as u64).to_be_bytes());
    hasher.update(payload);
    format!("{:x}", hasher.finalize())
}

fn error(code: &'static str, message: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, message)
}
