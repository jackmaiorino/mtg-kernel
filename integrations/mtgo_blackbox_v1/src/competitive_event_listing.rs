use crate::{
    competitive_mode_authorization_commitment_v1, make_offline_competitive_lifecycle_intent_v1,
    validate_checked_competitive_lifecycle_action_transition_v1,
    validate_visible_competitive_lifecycle_snapshot_v1,
    CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    CheckedUntrustedMtgoCompetitiveLifecycleTransitionV1, MtgoAuthorizationScopeV1,
    MtgoCompetitiveEventKindV1, MtgoCompetitiveLifecycleActionV1, MtgoCompetitiveLifecyclePhaseV1,
    MtgoContractErrorV1, MtgoOfflineCompetitiveLifecycleIntentV1, MtgoRectPxV1,
    MtgoVisibleCompetitiveLifecycleSnapshotV1, ValidatedMtgoCompetitiveDeckManifestV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1: u32 = 1;
const MIN_EVENT_LISTING_CONFIDENCE_BPS_V1: u16 = 9_500;
const EVENT_LISTING_TARGET_DOMAIN_V1: &[u8] = b"mtgo-competitive-event-listing-target-v1";
const EVENT_LISTING_SELECTION_DOMAIN_V1: &[u8] =
    b"mtgo-visible-competitive-event-listing-selection-v1";
const EVENT_LISTING_OPEN_INTENT_DOMAIN_V1: &[u8] = b"mtgo-competitive-event-listing-open-intent-v1";
const EVENT_LISTING_ARRIVAL_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-listing-entry-review-arrival-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveEventListingTargetV1 {
    pub schema_version: u32,
    pub target_id: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub approved_account_alias_sha256: String,
    pub event_identity_sha256: String,
    pub event_display_label_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoVisibleCompetitiveEventListingSelectionV1 {
    pub schema_version: u32,
    pub selection_id: String,
    pub target_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub event_display_label_sha256: String,
    pub source_lifecycle_snapshot_commitment_sha256: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub frame_sha256: String,
    pub client_bounds: MtgoRectPxV1,
    pub event_label_rect_client_px: MtgoRectPxV1,
    pub event_label_region_sha256: String,
    pub open_entry_review_control_rect_client_px: MtgoRectPxV1,
    pub open_entry_review_control_region_sha256: String,
    pub open_entry_review_control_enabled: bool,
    pub confidence_bps: u16,
}

/// Exact, coordinate-private interpretation of one visible competitive event
/// listing. It is structurally checked but remains untrusted and non-actionable.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoCompetitiveEventListingSelectionV1;
/// fn cannot_open(value: &CheckedUntrustedMtgoCompetitiveEventListingSelectionV1) {
///     let _ = value.open_entry_review();
///     let _ = value.control_rect_client_px();
/// }
/// ```
pub struct CheckedUntrustedMtgoCompetitiveEventListingSelectionV1 {
    lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    target: MtgoCompetitiveEventListingTargetV1,
    _raw: MtgoVisibleCompetitiveEventListingSelectionV1,
    target_commitment_sha256: String,
    selection_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveEventListingSelectionV1 {
    pub(crate) fn raw_for_evaluation_v1(&self) -> &MtgoVisibleCompetitiveEventListingSelectionV1 {
        &self._raw
    }

    pub fn target_commitment_sha256_v1(&self) -> &str {
        &self.target_commitment_sha256
    }

    pub fn selection_commitment_sha256_v1(&self) -> &str {
        &self.selection_commitment_sha256
    }

    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.target.event_kind
    }

    pub fn event_identity_sha256_v1(&self) -> &str {
        &self.target.event_identity_sha256
    }

    pub fn approved_account_alias_sha256_v1(&self) -> &str {
        &self.target.approved_account_alias_sha256
    }

    pub fn deck_list_sha256_v1(&self) -> &str {
        &self.target.deck_list_sha256
    }

    pub fn deck_manifest_commitment_sha256_v1(&self) -> &str {
        &self.target.deck_manifest_commitment_sha256
    }

    pub fn deck_format_sha256_v1(&self) -> &str {
        &self.target.deck_format_sha256
    }

    pub fn policy_deployment_commitment_sha256_v1(&self) -> &str {
        &self.target.policy_deployment_commitment_sha256
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

/// Coordinate-free request to open the exact visible listing's Entry Review.
/// It owns no input primitive or authority to confirm or spend.
pub struct CheckedUntrustedMtgoCompetitiveEventListingOpenIntentV1 {
    selection: CheckedUntrustedMtgoCompetitiveEventListingSelectionV1,
    lifecycle_intent: MtgoOfflineCompetitiveLifecycleIntentV1,
    mode_authorization_commitment_sha256: String,
    open_intent_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveEventListingOpenIntentV1 {
    pub fn selection_commitment_sha256_v1(&self) -> &str {
        self.selection.selection_commitment_sha256_v1()
    }

    pub fn open_intent_commitment_sha256_v1(&self) -> &str {
        &self.open_intent_commitment_sha256
    }

    pub fn mode_authorization_commitment_sha256_v1(&self) -> &str {
        &self.mode_authorization_commitment_sha256
    }

    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.selection.event_kind_v1()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_entry_confirmation_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Exact visible arrival at the selected listing's Entry Review. This proves
/// only the before/after relationship. It cannot confirm entry or spend.
pub struct CheckedUntrustedMtgoCompetitiveEntryReviewArrivalV1 {
    _intent: CheckedUntrustedMtgoCompetitiveEventListingOpenIntentV1,
    _next: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    _transition: CheckedUntrustedMtgoCompetitiveLifecycleTransitionV1,
    event_kind: MtgoCompetitiveEventKindV1,
    event_identity_sha256: String,
    deck_list_sha256: String,
    deck_manifest_commitment_sha256: String,
    deck_format_sha256: String,
    policy_deployment_commitment_sha256: String,
    next_lifecycle_snapshot_commitment_sha256: String,
    arrival_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveEntryReviewArrivalV1 {
    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.event_kind
    }

    pub fn event_identity_sha256_v1(&self) -> &str {
        &self.event_identity_sha256
    }

    pub fn deck_list_sha256_v1(&self) -> &str {
        &self.deck_list_sha256
    }

    pub fn deck_manifest_commitment_sha256_v1(&self) -> &str {
        &self.deck_manifest_commitment_sha256
    }

    pub fn deck_format_sha256_v1(&self) -> &str {
        &self.deck_format_sha256
    }

    pub fn policy_deployment_commitment_sha256_v1(&self) -> &str {
        &self.policy_deployment_commitment_sha256
    }

    pub fn next_lifecycle_snapshot_commitment_sha256_v1(&self) -> &str {
        &self.next_lifecycle_snapshot_commitment_sha256
    }

    pub fn arrival_commitment_sha256_v1(&self) -> &str {
        &self.arrival_commitment_sha256
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_entry_confirmation_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

pub fn competitive_event_listing_target_commitment_v1(
    target: &MtgoCompetitiveEventListingTargetV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
) -> Result<String, MtgoContractErrorV1> {
    validate_target_v1(target, deck)?;
    let target_bytes = canonical_json_v1(target, "event listing target")?;
    Ok(commitment_v1(
        EVENT_LISTING_TARGET_DOMAIN_V1,
        &[target_bytes.as_slice()],
    ))
}

pub fn validate_visible_competitive_event_listing_selection_v1(
    lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    target: MtgoCompetitiveEventListingTargetV1,
    raw: MtgoVisibleCompetitiveEventListingSelectionV1,
) -> Result<CheckedUntrustedMtgoCompetitiveEventListingSelectionV1, MtgoContractErrorV1> {
    let target_commitment_sha256 = competitive_event_listing_target_commitment_v1(&target, deck)?;
    if raw.schema_version != MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1 {
        return Err(error_v1(
            "event_listing_schema",
            "expected event-listing schema version 1",
        ));
    }
    validate_identifier_v1(&raw.selection_id, "event_listing_selection_id")?;
    for (value, code) in [
        (&raw.target_commitment_sha256, "event_listing_target"),
        (&raw.event_identity_sha256, "event_listing_event"),
        (&raw.event_display_label_sha256, "event_listing_label"),
        (
            &raw.source_lifecycle_snapshot_commitment_sha256,
            "event_listing_lifecycle",
        ),
        (&raw.frame_sha256, "event_listing_frame"),
        (&raw.event_label_region_sha256, "event_listing_label_region"),
        (
            &raw.open_entry_review_control_region_sha256,
            "event_listing_control_region",
        ),
    ] {
        validate_sha256_v1(value, code)?;
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
            "event_listing_source",
            "selection must bind the exact Event Browser lifecycle frame",
        ));
    }
    if raw.target_commitment_sha256 != target_commitment_sha256
        || raw.event_kind != target.event_kind
        || raw.event_identity_sha256 != target.event_identity_sha256
        || raw.event_display_label_sha256 != target.event_display_label_sha256
    {
        return Err(error_v1(
            "event_listing_target_binding",
            "visible selection must match the exact declared event target",
        ));
    }
    if !raw.open_entry_review_control_enabled
        || raw.confidence_bps < MIN_EVENT_LISTING_CONFIDENCE_BPS_V1
        || raw.confidence_bps > 10_000
    {
        return Err(error_v1(
            "event_listing_readiness",
            "event label and enabled Entry Review control must be complete and high confidence",
        ));
    }
    validate_rect_inside_v1(&raw.event_label_rect_client_px, &raw.client_bounds)?;
    validate_rect_inside_v1(
        &raw.open_entry_review_control_rect_client_px,
        &raw.client_bounds,
    )?;
    if rects_overlap_v1(
        &raw.event_label_rect_client_px,
        &raw.open_entry_review_control_rect_client_px,
    )? {
        return Err(error_v1(
            "event_listing_region_overlap",
            "event label and enabled control evidence must not overlap",
        ));
    }
    let raw_bytes = canonical_json_v1(&raw, "event listing selection")?;
    let selection_commitment_sha256 = commitment_v1(
        EVENT_LISTING_SELECTION_DOMAIN_V1,
        &[
            target_commitment_sha256.as_bytes(),
            lifecycle.snapshot_commitment_sha256().as_bytes(),
            raw_bytes.as_slice(),
            b"checked_untrusted_visible_listing_no_input_no_confirmation_no_spending",
        ],
    );
    Ok(CheckedUntrustedMtgoCompetitiveEventListingSelectionV1 {
        lifecycle,
        target,
        _raw: raw,
        target_commitment_sha256,
        selection_commitment_sha256,
    })
}

pub fn make_offline_competitive_event_listing_open_intent_v1(
    selection: CheckedUntrustedMtgoCompetitiveEventListingSelectionV1,
    authorization: &MtgoAuthorizationScopeV1,
) -> Result<CheckedUntrustedMtgoCompetitiveEventListingOpenIntentV1, MtgoContractErrorV1> {
    let mode_authorization_commitment_sha256 =
        competitive_mode_authorization_commitment_v1(authorization, selection.target.event_kind)?;
    if authorization.account_alias_sha256 != selection.target.approved_account_alias_sha256 {
        return Err(error_v1(
            "event_listing_account",
            "selected listing must bind the exact approved account",
        ));
    }
    validate_exact_mode_scope_v1(authorization, selection.target.event_kind)?;
    let lifecycle_intent = make_offline_competitive_lifecycle_intent_v1(
        &selection.lifecycle,
        MtgoCompetitiveLifecycleActionV1::OpenEntryReview,
        authorization,
        None,
    )?;
    let open_intent_commitment_sha256 = commitment_v1(
        EVENT_LISTING_OPEN_INTENT_DOMAIN_V1,
        &[
            selection.selection_commitment_sha256.as_bytes(),
            mode_authorization_commitment_sha256.as_bytes(),
            selection.target.deck_list_sha256.as_bytes(),
            selection.target.deck_manifest_commitment_sha256.as_bytes(),
            selection.target.deck_format_sha256.as_bytes(),
            selection
                .target
                .policy_deployment_commitment_sha256
                .as_bytes(),
            b"open_exact_visible_entry_review_only_no_confirmation_no_spending_no_input_primitive",
        ],
    );
    Ok(CheckedUntrustedMtgoCompetitiveEventListingOpenIntentV1 {
        selection,
        lifecycle_intent,
        mode_authorization_commitment_sha256,
        open_intent_commitment_sha256,
    })
}

pub fn confirm_competitive_event_listing_opened_v1(
    intent: CheckedUntrustedMtgoCompetitiveEventListingOpenIntentV1,
    authorization: &MtgoAuthorizationScopeV1,
    next: MtgoVisibleCompetitiveLifecycleSnapshotV1,
) -> Result<CheckedUntrustedMtgoCompetitiveEntryReviewArrivalV1, MtgoContractErrorV1> {
    let next = validate_visible_competitive_lifecycle_snapshot_v1(next)?;
    confirm_checked_competitive_event_listing_opened_v1(intent, authorization, next)
}

pub fn confirm_checked_competitive_event_listing_opened_v1(
    intent: CheckedUntrustedMtgoCompetitiveEventListingOpenIntentV1,
    authorization: &MtgoAuthorizationScopeV1,
    next: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
) -> Result<CheckedUntrustedMtgoCompetitiveEntryReviewArrivalV1, MtgoContractErrorV1> {
    let expected_mode_commitment = competitive_mode_authorization_commitment_v1(
        authorization,
        intent.selection.target.event_kind,
    )?;
    if expected_mode_commitment != intent.mode_authorization_commitment_sha256
        || authorization.account_alias_sha256
            != intent.selection.target.approved_account_alias_sha256
    {
        return Err(error_v1(
            "event_listing_arrival_authorization",
            "arrival must retain the exact selected account and one-mode authorization",
        ));
    }
    validate_exact_mode_scope_v1(authorization, intent.selection.target.event_kind)?;
    let transition = validate_checked_competitive_lifecycle_action_transition_v1(
        &intent.selection.lifecycle,
        &intent.lifecycle_intent,
        authorization,
        None,
        &next,
    )?;
    if next.phase() != MtgoCompetitiveLifecyclePhaseV1::EntryReview
        || next.event_kind() != intent.selection.target.event_kind
        || next.event_identity_sha256_v1()
            != Some(intent.selection.target.event_identity_sha256.as_str())
    {
        return Err(error_v1(
            "event_listing_arrival_identity",
            "Entry Review must visibly identify the exact selected event",
        ));
    }
    let next_lifecycle_snapshot_commitment_sha256 = next.snapshot_commitment_sha256().to_owned();
    let arrival_commitment_sha256 = commitment_v1(
        EVENT_LISTING_ARRIVAL_DOMAIN_V1,
        &[
            intent.open_intent_commitment_sha256.as_bytes(),
            transition.source_snapshot_commitment_sha256().as_bytes(),
            transition.next_snapshot_commitment_sha256().as_bytes(),
            intent.selection.target.event_identity_sha256.as_bytes(),
            intent.selection.target.deck_list_sha256.as_bytes(),
            intent
                .selection
                .target
                .deck_manifest_commitment_sha256
                .as_bytes(),
            intent.selection.target.deck_format_sha256.as_bytes(),
            intent
                .selection
                .target
                .policy_deployment_commitment_sha256
                .as_bytes(),
            b"exact_selected_entry_review_visible_no_confirmation_no_spending_no_input",
        ],
    );
    Ok(CheckedUntrustedMtgoCompetitiveEntryReviewArrivalV1 {
        event_kind: intent.selection.target.event_kind,
        event_identity_sha256: intent.selection.target.event_identity_sha256.clone(),
        deck_list_sha256: intent.selection.target.deck_list_sha256.clone(),
        deck_manifest_commitment_sha256: intent
            .selection
            .target
            .deck_manifest_commitment_sha256
            .clone(),
        deck_format_sha256: intent.selection.target.deck_format_sha256.clone(),
        policy_deployment_commitment_sha256: intent
            .selection
            .target
            .policy_deployment_commitment_sha256
            .clone(),
        next_lifecycle_snapshot_commitment_sha256,
        arrival_commitment_sha256,
        _intent: intent,
        _next: next,
        _transition: transition,
    })
}

fn validate_target_v1(
    target: &MtgoCompetitiveEventListingTargetV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
) -> Result<(), MtgoContractErrorV1> {
    if target.schema_version != MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1 {
        return Err(error_v1(
            "event_listing_target_schema",
            "expected event-listing target schema version 1",
        ));
    }
    validate_identifier_v1(&target.target_id, "event_listing_target_id")?;
    for (value, code) in [
        (
            &target.approved_account_alias_sha256,
            "event_listing_account",
        ),
        (&target.event_identity_sha256, "event_listing_event"),
        (&target.event_display_label_sha256, "event_listing_label"),
        (&target.deck_list_sha256, "event_listing_deck"),
        (
            &target.deck_manifest_commitment_sha256,
            "event_listing_manifest",
        ),
        (&target.deck_format_sha256, "event_listing_format"),
        (
            &target.policy_deployment_commitment_sha256,
            "event_listing_policy",
        ),
    ] {
        validate_sha256_v1(value, code)?;
    }
    if target.deck_list_sha256 != deck.deck_list_sha256()
        || target.deck_manifest_commitment_sha256 != deck.manifest_commitment_sha256()
        || target.deck_format_sha256 != deck.format_sha256()
        || target.policy_deployment_commitment_sha256 == target.deck_list_sha256
        || target.policy_deployment_commitment_sha256 == target.deck_manifest_commitment_sha256
        || target.policy_deployment_commitment_sha256 == target.deck_format_sha256
    {
        return Err(error_v1(
            "event_listing_deployment_identity",
            "event target must bind the exact distinct deck, format, and policy identities",
        ));
    }
    Ok(())
}

fn validate_exact_mode_scope_v1(
    authorization: &MtgoAuthorizationScopeV1,
    event_kind: MtgoCompetitiveEventKindV1,
) -> Result<(), MtgoContractErrorV1> {
    let exact = !authorization.shadow_observation
        && !authorization.private_match_input
        && !authorization.open_play_input
        && !authorization.other_prize_event_input
        && match event_kind {
            MtgoCompetitiveEventKindV1::League => {
                authorization.league_input && !authorization.challenge_input
            }
            MtgoCompetitiveEventKindV1::Challenge => {
                authorization.challenge_input && !authorization.league_input
            }
        };
    if !exact {
        return Err(error_v1(
            "event_listing_mode_scope",
            "opening Entry Review requires exactly one League or Challenge mode",
        ));
    }
    Ok(())
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
            "event_listing_region_bounds",
            "event-listing evidence region must be nonempty and inside the client",
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
        .ok_or_else(|| error_v1("event_listing_region_overflow", "first rectangle overflow"))?;
    let first_bottom = first
        .y
        .checked_add(first.height)
        .ok_or_else(|| error_v1("event_listing_region_overflow", "first rectangle overflow"))?;
    let second_right = second
        .x
        .checked_add(second.width)
        .ok_or_else(|| error_v1("event_listing_region_overflow", "second rectangle overflow"))?;
    let second_bottom = second
        .y
        .checked_add(second.height)
        .ok_or_else(|| error_v1("event_listing_region_overflow", "second rectangle overflow"))?;
    Ok(first.x < second_right
        && second.x < first_right
        && first.y < second_bottom
        && second.y < first_bottom)
}

fn validate_identifier_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(error_v1(code, value.to_owned()));
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

fn canonical_json_v1<T: Serialize>(
    value: &T,
    label: &'static str,
) -> Result<Vec<u8>, MtgoContractErrorV1> {
    serde_json::to_vec(value)
        .map_err(|error| error_v1("event_listing_serialization", format!("{label}: {error}")))
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
