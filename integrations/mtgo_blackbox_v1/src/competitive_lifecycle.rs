use crate::{
    validate_authorization_for_mode_v1, MtgoAuthorizationScopeV1, MtgoContractErrorV1,
    MtgoRectPxV1, MtgoRuntimeModeV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1: u32 = 1;
const MIN_LIFECYCLE_CONFIDENCE_BPS_V1: u16 = 9_500;
const SNAPSHOT_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-competitive-lifecycle-snapshot-v1";
const ENTRY_AUTHORIZATION_DOMAIN_V1: &[u8] = b"mtgo-competitive-entry-authorization-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveEventKindV1 {
    League,
    Challenge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveLifecyclePhaseV1 {
    EventBrowser,
    EntryReview,
    EnteredWaitingForPairing,
    PairingReady,
    MatchInProgress,
    Sideboarding,
    MatchComplete,
    EventComplete,
    Reconnect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoLifecycleVisibleFactKindV1 {
    EventBrowserVisible,
    EntryReviewVisible,
    EntryTermsVisible,
    EnteredEventVisible,
    PairingVisible,
    PairingAcceptControlEnabled,
    MatchSurfaceVisible,
    LocalClockVisible,
    OpponentClockVisible,
    SideboardSurfaceVisible,
    SideboardTimerVisible,
    SideboardConfigurationVisible,
    SideboardNoChangesConfirmed,
    SideboardSubmitControlEnabled,
    MatchResultVisible,
    MatchContinueControlEnabled,
    EventResultVisible,
    EventCloseControlEnabled,
    ReconnectVisible,
    ReconnectResumeControlEnabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoLifecycleVisibleFactV1 {
    pub kind: MtgoLifecycleVisibleFactKindV1,
    pub rect_client_px: MtgoRectPxV1,
    pub content_sha256: String,
    pub confidence_bps: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveEntryResourceV1 {
    NoCost,
    ExistingEventToken,
    ExistingPlayPoints,
    ExistingEventTickets,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveEntryTermsV1 {
    pub terms_sha256: String,
    pub resource: MtgoCompetitiveEntryResourceV1,
    pub amount: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleCompetitiveLifecycleSnapshotV1 {
    pub schema_version: u32,
    pub snapshot_id: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub phase: MtgoCompetitiveLifecyclePhaseV1,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub frame_sha256: String,
    pub client_bounds: MtgoRectPxV1,
    pub event_identity_sha256: Option<String>,
    pub match_identity_sha256: Option<String>,
    pub game_number: Option<u8>,
    pub entry_terms: Option<MtgoCompetitiveEntryTermsV1>,
    pub visible_state_complete: bool,
    pub facts: Vec<MtgoLifecycleVisibleFactV1>,
}

/// A structurally checked, still untrusted interpretation of visible pixels.
/// It carries no coordinates, input method, process handle, or live authority.
pub struct CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1 {
    snapshot: MtgoVisibleCompetitiveLifecycleSnapshotV1,
    snapshot_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1 {
    pub fn event_kind(&self) -> MtgoCompetitiveEventKindV1 {
        self.snapshot.event_kind
    }

    pub fn phase(&self) -> MtgoCompetitiveLifecyclePhaseV1 {
        self.snapshot.phase
    }

    pub fn frame_sequence(&self) -> u64 {
        self.snapshot.frame_sequence
    }

    pub fn snapshot_commitment_sha256(&self) -> &str {
        &self.snapshot_commitment_sha256
    }

    pub fn frame_id_v1(&self) -> u64 {
        self.snapshot.frame_id
    }

    pub fn frame_sha256_v1(&self) -> &str {
        &self.snapshot.frame_sha256
    }

    pub fn client_bounds_v1(&self) -> &MtgoRectPxV1 {
        &self.snapshot.client_bounds
    }

    pub fn event_identity_sha256_v1(&self) -> Option<&str> {
        self.snapshot.event_identity_sha256.as_deref()
    }

    pub fn match_identity_sha256_v1(&self) -> Option<&str> {
        self.snapshot.match_identity_sha256.as_deref()
    }

    pub fn game_number_v1(&self) -> Option<u8> {
        self.snapshot.game_number
    }

    /// Returns the exact visible entry terms only while the checked snapshot
    /// is in the entry-review phase. The terms remain coordinate-free,
    /// non-authorizing data.
    pub fn entry_terms_v1(&self) -> Option<&MtgoCompetitiveEntryTermsV1> {
        self.snapshot.entry_terms.as_ref()
    }

    /// Returns the visible regions and their content commitments so an opaque
    /// capture owner can recompute them against the retained source pixels.
    /// The checked snapshot itself remains non-authorizing.
    pub fn visible_facts_v1(&self) -> &[MtgoLifecycleVisibleFactV1] {
        &self.snapshot.facts
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveEntryAuthorizationV1 {
    pub schema_version: u32,
    pub account_alias_sha256: String,
    pub written_permission_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub entry_terms: MtgoCompetitiveEntryTermsV1,
    pub exact_entry_authorized: bool,
    pub existing_account_resources_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveLifecycleActionV1 {
    OpenEntryReview,
    CancelEntry,
    ConfirmEntry,
    AcceptPairing,
    SubmitSideboard,
    ContinueAfterMatch,
    ResumeMatch,
    CloseCompletedEvent,
}

/// Coordinate-free lifecycle intent. It is not an input instruction or proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoOfflineCompetitiveLifecycleIntentV1 {
    pub schema_version: u32,
    pub source_snapshot_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub action: MtgoCompetitiveLifecycleActionV1,
    pub entry_authorization_sha256: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoObservedCompetitiveLifecycleAdvanceV1 {
    PairingPosted,
    GameEndedForSideboarding,
    MatchEnded,
    EventEnded,
    ConnectionInterrupted,
}

/// A checked before/after lifecycle relationship. It remains observation-only.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoCompetitiveLifecycleTransitionV1;
/// fn cannot_extract_input(value: &CheckedUntrustedMtgoCompetitiveLifecycleTransitionV1) {
///     let _ = value.input_command();
/// }
/// ```
pub struct CheckedUntrustedMtgoCompetitiveLifecycleTransitionV1 {
    source_snapshot_commitment_sha256: String,
    next_snapshot_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveLifecycleTransitionV1 {
    pub fn source_snapshot_commitment_sha256(&self) -> &str {
        &self.source_snapshot_commitment_sha256
    }

    pub fn next_snapshot_commitment_sha256(&self) -> &str {
        &self.next_snapshot_commitment_sha256
    }

    pub fn safe_for_live_input(&self) -> bool {
        false
    }
}

pub fn validate_visible_competitive_lifecycle_snapshot_v1(
    snapshot: MtgoVisibleCompetitiveLifecycleSnapshotV1,
) -> Result<CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1, MtgoContractErrorV1> {
    if snapshot.schema_version != MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1 {
        return Err(error(
            "lifecycle_schema",
            "the lifecycle schema must be exactly version 1",
        ));
    }
    validate_identifier("snapshot_id", &snapshot.snapshot_id)?;
    if snapshot.frame_id == 0 || snapshot.frame_sequence == 0 {
        return Err(error(
            "lifecycle_frame",
            "frame identity and sequence must be nonzero",
        ));
    }
    validate_sha256("frame_sha256", &snapshot.frame_sha256)?;
    if snapshot.client_bounds.x != 0
        || snapshot.client_bounds.y != 0
        || snapshot.client_bounds.width == 0
        || snapshot.client_bounds.height == 0
    {
        return Err(error(
            "lifecycle_client_bounds",
            "client bounds must be a nonempty window-local rectangle rooted at zero",
        ));
    }
    if !snapshot.visible_state_complete {
        return Err(error(
            "lifecycle_state_incomplete",
            "the visible lifecycle state must be declared complete",
        ));
    }
    validate_optional_sha256("event_identity_sha256", &snapshot.event_identity_sha256)?;
    validate_optional_sha256("match_identity_sha256", &snapshot.match_identity_sha256)?;
    validate_entry_terms(snapshot.entry_terms.as_ref())?;
    validate_phase_fields(&snapshot)?;
    validate_visible_facts(&snapshot)?;

    let commitment = commitment(
        SNAPSHOT_COMMITMENT_DOMAIN_V1,
        &serde_json::to_vec(&snapshot)
            .map_err(|value| error("lifecycle_serialization", value.to_string()))?,
    );
    Ok(CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1 {
        snapshot,
        snapshot_commitment_sha256: commitment,
    })
}

pub fn make_offline_competitive_lifecycle_intent_v1(
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    action: MtgoCompetitiveLifecycleActionV1,
    mode_authorization: &MtgoAuthorizationScopeV1,
    entry_authorization: Option<&MtgoCompetitiveEntryAuthorizationV1>,
) -> Result<MtgoOfflineCompetitiveLifecycleIntentV1, MtgoContractErrorV1> {
    validate_authorization_for_mode_v1(mode_authorization, runtime_mode(source.event_kind()))?;
    validate_action_source_phase(source.phase(), action)?;
    if action == MtgoCompetitiveLifecycleActionV1::SubmitSideboard
        && !source
            .visible_facts_v1()
            .iter()
            .any(|fact| fact.kind == MtgoLifecycleVisibleFactKindV1::SideboardNoChangesConfirmed)
    {
        return Err(error(
            "sideboard_changes_unplanned",
            "generic sideboard submission is limited to an explicitly visible no-change state",
        ));
    }
    let entry_authorization_sha256 = if action == MtgoCompetitiveLifecycleActionV1::ConfirmEntry {
        Some(validate_entry_authorization(
            source,
            mode_authorization,
            entry_authorization.ok_or_else(|| {
                error(
                    "entry_authorization_missing",
                    "entry confirmation requires an exact separately reviewed authorization",
                )
            })?,
        )?)
    } else {
        if entry_authorization.is_some() {
            return Err(error(
                "entry_authorization_unexpected",
                "entry authorization may only accompany exact entry confirmation",
            ));
        }
        None
    };

    Ok(MtgoOfflineCompetitiveLifecycleIntentV1 {
        schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
        source_snapshot_commitment_sha256: source.snapshot_commitment_sha256.clone(),
        event_kind: source.event_kind(),
        action,
        entry_authorization_sha256,
    })
}

pub fn validate_competitive_lifecycle_action_transition_v1(
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    intent: &MtgoOfflineCompetitiveLifecycleIntentV1,
    mode_authorization: &MtgoAuthorizationScopeV1,
    entry_authorization: Option<&MtgoCompetitiveEntryAuthorizationV1>,
    next: MtgoVisibleCompetitiveLifecycleSnapshotV1,
) -> Result<CheckedUntrustedMtgoCompetitiveLifecycleTransitionV1, MtgoContractErrorV1> {
    let next = validate_visible_competitive_lifecycle_snapshot_v1(next)?;
    validate_checked_competitive_lifecycle_action_transition_v1(
        source,
        intent,
        mode_authorization,
        entry_authorization,
        &next,
    )
}

/// Validates an action transition when both lifecycle snapshots already came
/// through the strict structural checker. This avoids serializing or
/// reconstructing classifier output and still grants no input authority.
pub fn validate_checked_competitive_lifecycle_action_transition_v1(
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    intent: &MtgoOfflineCompetitiveLifecycleIntentV1,
    mode_authorization: &MtgoAuthorizationScopeV1,
    entry_authorization: Option<&MtgoCompetitiveEntryAuthorizationV1>,
    next: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
) -> Result<CheckedUntrustedMtgoCompetitiveLifecycleTransitionV1, MtgoContractErrorV1> {
    let expected_intent = make_offline_competitive_lifecycle_intent_v1(
        source,
        intent.action,
        mode_authorization,
        entry_authorization,
    )?;
    if intent != &expected_intent {
        return Err(error(
            "lifecycle_intent_binding",
            "the lifecycle intent must match the exact revalidated authorization and source",
        ));
    }
    validate_common_transition(source, next)?;
    let phase_allowed = match intent.action {
        MtgoCompetitiveLifecycleActionV1::OpenEntryReview => {
            next.phase() == MtgoCompetitiveLifecyclePhaseV1::EntryReview
        }
        MtgoCompetitiveLifecycleActionV1::CancelEntry => {
            next.phase() == MtgoCompetitiveLifecyclePhaseV1::EventBrowser
        }
        MtgoCompetitiveLifecycleActionV1::ConfirmEntry => {
            next.phase() == MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing
        }
        MtgoCompetitiveLifecycleActionV1::AcceptPairing
        | MtgoCompetitiveLifecycleActionV1::SubmitSideboard
        | MtgoCompetitiveLifecycleActionV1::ResumeMatch => {
            next.phase() == MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
        }
        MtgoCompetitiveLifecycleActionV1::ContinueAfterMatch => matches!(
            next.phase(),
            MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing
                | MtgoCompetitiveLifecyclePhaseV1::EventComplete
        ),
        MtgoCompetitiveLifecycleActionV1::CloseCompletedEvent => {
            next.phase() == MtgoCompetitiveLifecyclePhaseV1::EventBrowser
        }
    };
    if !phase_allowed {
        return Err(error(
            "lifecycle_action_transition",
            "the visible next phase is not a postcondition of the requested action",
        ));
    }
    validate_identity_continuity(source, next, intent.action)?;
    Ok(transition(source, next))
}

pub fn validate_observed_competitive_lifecycle_advance_v1(
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    observed: MtgoObservedCompetitiveLifecycleAdvanceV1,
    next: MtgoVisibleCompetitiveLifecycleSnapshotV1,
) -> Result<CheckedUntrustedMtgoCompetitiveLifecycleTransitionV1, MtgoContractErrorV1> {
    let next = validate_visible_competitive_lifecycle_snapshot_v1(next)?;
    validate_checked_observed_competitive_lifecycle_advance_v1(source, observed, &next)
}

/// Validates a server- or client-observed lifecycle advance when both visible
/// snapshots have already passed the strict structural checker. This is an
/// observation-only transition and grants no input authority.
pub fn validate_checked_observed_competitive_lifecycle_advance_v1(
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    observed: MtgoObservedCompetitiveLifecycleAdvanceV1,
    next: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
) -> Result<CheckedUntrustedMtgoCompetitiveLifecycleTransitionV1, MtgoContractErrorV1> {
    validate_common_transition(source, next)?;
    let allowed = match observed {
        MtgoObservedCompetitiveLifecycleAdvanceV1::PairingPosted => {
            source.phase() == MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing
                && next.phase() == MtgoCompetitiveLifecyclePhaseV1::PairingReady
        }
        MtgoObservedCompetitiveLifecycleAdvanceV1::GameEndedForSideboarding => {
            source.phase() == MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
                && next.phase() == MtgoCompetitiveLifecyclePhaseV1::Sideboarding
        }
        MtgoObservedCompetitiveLifecycleAdvanceV1::MatchEnded => {
            matches!(
                source.phase(),
                MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
                    | MtgoCompetitiveLifecyclePhaseV1::Sideboarding
            ) && next.phase() == MtgoCompetitiveLifecyclePhaseV1::MatchComplete
        }
        MtgoObservedCompetitiveLifecycleAdvanceV1::EventEnded => {
            matches!(
                source.phase(),
                MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing
                    | MtgoCompetitiveLifecyclePhaseV1::MatchComplete
            ) && next.phase() == MtgoCompetitiveLifecyclePhaseV1::EventComplete
        }
        MtgoObservedCompetitiveLifecycleAdvanceV1::ConnectionInterrupted => {
            matches!(
                source.phase(),
                MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
                    | MtgoCompetitiveLifecyclePhaseV1::Sideboarding
            ) && next.phase() == MtgoCompetitiveLifecyclePhaseV1::Reconnect
        }
    };
    if !allowed {
        return Err(error(
            "lifecycle_observed_transition",
            "the observed client event does not allow the visible phase transition",
        ));
    }
    validate_observed_identity_continuity(source, next, observed)?;
    Ok(transition(source, next))
}

fn validate_visible_facts(
    snapshot: &MtgoVisibleCompetitiveLifecycleSnapshotV1,
) -> Result<(), MtgoContractErrorV1> {
    if snapshot.facts.is_empty() || snapshot.facts.len() > 32 {
        return Err(error(
            "lifecycle_fact_count",
            "a complete lifecycle snapshot must contain between 1 and 32 visible facts",
        ));
    }
    let mut kinds = HashSet::new();
    for fact in &snapshot.facts {
        if !kinds.insert(fact.kind) {
            return Err(error(
                "lifecycle_fact_duplicate",
                "each visible lifecycle fact kind must occur exactly once",
            ));
        }
        validate_sha256("lifecycle_fact.content_sha256", &fact.content_sha256)?;
        if fact.confidence_bps < MIN_LIFECYCLE_CONFIDENCE_BPS_V1 || fact.confidence_bps > 10_000 {
            return Err(error(
                "lifecycle_fact_confidence",
                "every visible lifecycle fact must meet the declared confidence floor",
            ));
        }
        validate_rect_inside(&fact.rect_client_px, &snapshot.client_bounds)?;
    }
    for required in required_facts(snapshot.phase) {
        if !kinds.contains(required) {
            return Err(error(
                "lifecycle_required_fact",
                "the current phase is missing a required visible fact",
            ));
        }
    }
    Ok(())
}

fn required_facts(
    phase: MtgoCompetitiveLifecyclePhaseV1,
) -> &'static [MtgoLifecycleVisibleFactKindV1] {
    use MtgoLifecycleVisibleFactKindV1::*;
    match phase {
        MtgoCompetitiveLifecyclePhaseV1::EventBrowser => &[EventBrowserVisible],
        MtgoCompetitiveLifecyclePhaseV1::EntryReview => &[EntryReviewVisible, EntryTermsVisible],
        MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing => &[EnteredEventVisible],
        MtgoCompetitiveLifecyclePhaseV1::PairingReady => {
            &[PairingVisible, PairingAcceptControlEnabled]
        }
        MtgoCompetitiveLifecyclePhaseV1::MatchInProgress => {
            &[MatchSurfaceVisible, LocalClockVisible, OpponentClockVisible]
        }
        MtgoCompetitiveLifecyclePhaseV1::Sideboarding => &[
            SideboardSurfaceVisible,
            SideboardTimerVisible,
            SideboardConfigurationVisible,
            SideboardSubmitControlEnabled,
        ],
        MtgoCompetitiveLifecyclePhaseV1::MatchComplete => {
            &[MatchResultVisible, MatchContinueControlEnabled]
        }
        MtgoCompetitiveLifecyclePhaseV1::EventComplete => {
            &[EventResultVisible, EventCloseControlEnabled]
        }
        MtgoCompetitiveLifecyclePhaseV1::Reconnect => {
            &[ReconnectVisible, ReconnectResumeControlEnabled]
        }
    }
}

fn validate_phase_fields(
    snapshot: &MtgoVisibleCompetitiveLifecycleSnapshotV1,
) -> Result<(), MtgoContractErrorV1> {
    let event_required = snapshot.phase != MtgoCompetitiveLifecyclePhaseV1::EventBrowser;
    if event_required != snapshot.event_identity_sha256.is_some() {
        return Err(error(
            "lifecycle_event_identity",
            "exactly the browser phase omits an event identity",
        ));
    }
    let match_required = matches!(
        snapshot.phase,
        MtgoCompetitiveLifecyclePhaseV1::PairingReady
            | MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
            | MtgoCompetitiveLifecyclePhaseV1::Sideboarding
            | MtgoCompetitiveLifecyclePhaseV1::MatchComplete
            | MtgoCompetitiveLifecyclePhaseV1::Reconnect
    );
    if match_required != snapshot.match_identity_sha256.is_some() {
        return Err(error(
            "lifecycle_match_identity",
            "match-related phases require one exact visible match identity",
        ));
    }
    let game_required = matches!(
        snapshot.phase,
        MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
            | MtgoCompetitiveLifecyclePhaseV1::Sideboarding
            | MtgoCompetitiveLifecyclePhaseV1::Reconnect
    );
    if game_required != snapshot.game_number.is_some()
        || snapshot
            .game_number
            .is_some_and(|value| !(1..=3).contains(&value))
    {
        return Err(error(
            "lifecycle_game_number",
            "gameplay, sideboard, and reconnect phases require game number 1 through 3",
        ));
    }
    if (snapshot.phase == MtgoCompetitiveLifecyclePhaseV1::EntryReview)
        != snapshot.entry_terms.is_some()
    {
        return Err(error(
            "lifecycle_entry_terms",
            "entry terms must appear only in the exact entry-review phase",
        ));
    }
    Ok(())
}

fn validate_entry_authorization(
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    mode: &MtgoAuthorizationScopeV1,
    entry: &MtgoCompetitiveEntryAuthorizationV1,
) -> Result<String, MtgoContractErrorV1> {
    if entry.schema_version != MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1
        || !entry.exact_entry_authorized
        || !entry.existing_account_resources_only
    {
        return Err(error(
            "entry_authorization",
            "entry must be exact, explicit, and limited to existing account resources",
        ));
    }
    if entry.account_alias_sha256 != mode.account_alias_sha256
        || entry.written_permission_sha256 != mode.written_permission_sha256
        || entry.event_kind != source.event_kind()
        || Some(&entry.event_identity_sha256) != source.snapshot.event_identity_sha256.as_ref()
        || Some(&entry.entry_terms) != source.snapshot.entry_terms.as_ref()
    {
        return Err(error(
            "entry_authorization_binding",
            "entry authorization must bind the account, permission, event, and visible terms",
        ));
    }
    validate_sha256("entry.event_identity_sha256", &entry.event_identity_sha256)?;
    validate_entry_terms(Some(&entry.entry_terms))?;
    let bytes = serde_json::to_vec(entry)
        .map_err(|value| error("entry_authorization_serialization", value.to_string()))?;
    Ok(commitment(ENTRY_AUTHORIZATION_DOMAIN_V1, &bytes))
}

fn validate_entry_terms(
    terms: Option<&MtgoCompetitiveEntryTermsV1>,
) -> Result<(), MtgoContractErrorV1> {
    let Some(terms) = terms else {
        return Ok(());
    };
    validate_sha256("entry_terms.terms_sha256", &terms.terms_sha256)?;
    match terms.resource {
        MtgoCompetitiveEntryResourceV1::NoCost if terms.amount == 0 => Ok(()),
        MtgoCompetitiveEntryResourceV1::NoCost => Err(error(
            "entry_amount",
            "a no-cost entry must declare zero resource units",
        )),
        _ if terms.amount > 0 => Ok(()),
        _ => Err(error(
            "entry_amount",
            "an existing-resource entry must declare a positive exact amount",
        )),
    }
}

fn validate_action_source_phase(
    phase: MtgoCompetitiveLifecyclePhaseV1,
    action: MtgoCompetitiveLifecycleActionV1,
) -> Result<(), MtgoContractErrorV1> {
    let allowed = matches!(
        (phase, action),
        (
            MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
            MtgoCompetitiveLifecycleActionV1::OpenEntryReview
        ) | (
            MtgoCompetitiveLifecyclePhaseV1::EntryReview,
            MtgoCompetitiveLifecycleActionV1::CancelEntry
                | MtgoCompetitiveLifecycleActionV1::ConfirmEntry
        ) | (
            MtgoCompetitiveLifecyclePhaseV1::PairingReady,
            MtgoCompetitiveLifecycleActionV1::AcceptPairing
        ) | (
            MtgoCompetitiveLifecyclePhaseV1::Sideboarding,
            MtgoCompetitiveLifecycleActionV1::SubmitSideboard
        ) | (
            MtgoCompetitiveLifecyclePhaseV1::MatchComplete,
            MtgoCompetitiveLifecycleActionV1::ContinueAfterMatch
        ) | (
            MtgoCompetitiveLifecyclePhaseV1::Reconnect,
            MtgoCompetitiveLifecycleActionV1::ResumeMatch
        ) | (
            MtgoCompetitiveLifecyclePhaseV1::EventComplete,
            MtgoCompetitiveLifecycleActionV1::CloseCompletedEvent
        )
    );
    if allowed {
        Ok(())
    } else {
        Err(error(
            "lifecycle_action_source",
            "the requested lifecycle action is not available from the visible source phase",
        ))
    }
}

fn validate_common_transition(
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    next: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
) -> Result<(), MtgoContractErrorV1> {
    if next.event_kind() != source.event_kind()
        || next.snapshot.frame_id == source.snapshot.frame_id
        || next.frame_sequence() <= source.frame_sequence()
        || next.snapshot.frame_sha256 == source.snapshot.frame_sha256
        || next.snapshot.client_bounds != source.snapshot.client_bounds
    {
        return Err(error(
            "lifecycle_transition_freshness",
            "the next state must use the same event/layout and a changed strictly newer frame",
        ));
    }
    Ok(())
}

fn validate_identity_continuity(
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    next: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    action: MtgoCompetitiveLifecycleActionV1,
) -> Result<(), MtgoContractErrorV1> {
    let event_must_match = !matches!(
        action,
        MtgoCompetitiveLifecycleActionV1::OpenEntryReview
            | MtgoCompetitiveLifecycleActionV1::CancelEntry
            | MtgoCompetitiveLifecycleActionV1::CloseCompletedEvent
    );
    if event_must_match
        && source.snapshot.event_identity_sha256 != next.snapshot.event_identity_sha256
    {
        return Err(error(
            "lifecycle_event_continuity",
            "the event identity changed across one lifecycle action",
        ));
    }
    if matches!(
        action,
        MtgoCompetitiveLifecycleActionV1::AcceptPairing
            | MtgoCompetitiveLifecycleActionV1::SubmitSideboard
            | MtgoCompetitiveLifecycleActionV1::ResumeMatch
    ) && source.snapshot.match_identity_sha256 != next.snapshot.match_identity_sha256
    {
        return Err(error(
            "lifecycle_match_continuity",
            "the match identity changed across one match action",
        ));
    }
    Ok(())
}

fn validate_observed_identity_continuity(
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    next: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    observed: MtgoObservedCompetitiveLifecycleAdvanceV1,
) -> Result<(), MtgoContractErrorV1> {
    if source.snapshot.event_identity_sha256 != next.snapshot.event_identity_sha256 {
        return Err(error(
            "lifecycle_event_continuity",
            "the event identity changed across one observed client transition",
        ));
    }
    let match_must_match = !matches!(
        observed,
        MtgoObservedCompetitiveLifecycleAdvanceV1::PairingPosted
            | MtgoObservedCompetitiveLifecycleAdvanceV1::EventEnded
    );
    if match_must_match
        && source.snapshot.match_identity_sha256 != next.snapshot.match_identity_sha256
    {
        return Err(error(
            "lifecycle_match_continuity",
            "the match identity changed across one observed match transition",
        ));
    }
    Ok(())
}

fn transition(
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    next: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
) -> CheckedUntrustedMtgoCompetitiveLifecycleTransitionV1 {
    CheckedUntrustedMtgoCompetitiveLifecycleTransitionV1 {
        source_snapshot_commitment_sha256: source.snapshot_commitment_sha256.clone(),
        next_snapshot_commitment_sha256: next.snapshot_commitment_sha256.clone(),
    }
}

fn runtime_mode(event: MtgoCompetitiveEventKindV1) -> MtgoRuntimeModeV1 {
    match event {
        MtgoCompetitiveEventKindV1::League => MtgoRuntimeModeV1::LeagueInput,
        MtgoCompetitiveEventKindV1::Challenge => MtgoRuntimeModeV1::ChallengeInput,
    }
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
            "lifecycle_fact_bounds",
            "every visible fact rectangle must be nonempty and inside the client",
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

fn validate_optional_sha256(
    field: &'static str,
    value: &Option<String>,
) -> Result<(), MtgoContractErrorV1> {
    if let Some(value) = value {
        validate_sha256(field, value)?;
    }
    Ok(())
}

fn validate_sha256(field: &'static str, value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error(field, "expected a lowercase SHA-256 digest"));
    }
    Ok(())
}

fn commitment(domain: &[u8], value: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update((value.len() as u64).to_be_bytes());
    hash.update(value);
    format!("{:x}", hash.finalize())
}

fn error(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}
