use super::{
    MtgoAdmittedCompetitiveNavigationFrameCommitmentsV1,
    OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
};
use mtgo_blackbox_v1::{
    make_offline_competitive_lifecycle_intent_v1,
    validate_checked_competitive_lifecycle_action_transition_v1,
    visible_frame_region_content_sha256_v1, CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    CheckedUntrustedMtgoCompetitiveLifecycleTransitionV1, MtgoAuthorizationScopeV1,
    MtgoCompetitiveEventKindV1, MtgoCompetitiveLifecycleActionV1, MtgoCompetitiveLifecyclePhaseV1,
    MtgoLifecycleVisibleFactKindV1, MtgoLifecycleVisibleFactV1, MtgoRectPxV1, MtgoSizePxV1,
};
use sha2::{Digest, Sha256};

const COMPETITIVE_LIFECYCLE_CONTROL_BINDING_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-lifecycle-control-binding-v1";
const COMPETITIVE_LIFECYCLE_CONTROL_TRANSITION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-lifecycle-control-transition-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoOpaqueCompetitiveLifecycleControlCommitmentsV1 {
    pub source_frame: MtgoAdmittedCompetitiveNavigationFrameCommitmentsV1,
    pub approved_account_alias_sha256: String,
    pub runtime_identity_commitment_sha256: String,
    pub classification_result_commitment_sha256: String,
    pub lifecycle_snapshot_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub phase: MtgoCompetitiveLifecyclePhaseV1,
    pub action: MtgoCompetitiveLifecycleActionV1,
    pub event_identity_sha256: String,
    pub match_identity_sha256: Option<String>,
    pub game_number: Option<u8>,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub control_region_sha256: String,
    pub control_confidence_bps: u16,
    pub control_binding_commitment_sha256: String,
}

/// One exact enabled lifecycle control retained with its classifier-backed
/// composed-desktop source frame. The type is move-only and exposes no pixels,
/// coordinates, process handle, pointer target, input command, or authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveLifecycleControlV1;
/// let _forged = OpaqueMtgoCompetitiveLifecycleControlV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveLifecycleControlV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveLifecycleControlV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveLifecycleControlV1;
/// fn cannot_control(value: &OpaqueMtgoCompetitiveLifecycleControlV1) {
///     let _ = value.rect_client_px();
///     let _ = value.input_command();
/// }
/// ```
pub struct OpaqueMtgoCompetitiveLifecycleControlV1 {
    pub(crate) _source: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    pub(crate) _control_rect_client_px: MtgoRectPxV1,
    pub(crate) commitments: MtgoOpaqueCompetitiveLifecycleControlCommitmentsV1,
}

impl OpaqueMtgoCompetitiveLifecycleControlV1 {
    pub fn commitments_v1(&self) -> MtgoOpaqueCompetitiveLifecycleControlCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn action_v1(&self) -> MtgoCompetitiveLifecycleActionV1 {
        self.commitments.action
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveLifecycleControlTransitionCommitmentsV1 {
    pub control_binding_commitment_sha256: String,
    pub before_capture_commitment_sha256: String,
    pub before_classification_result_commitment_sha256: String,
    pub before_lifecycle_snapshot_commitment_sha256: String,
    pub after_capture_commitment_sha256: String,
    pub after_classification_result_commitment_sha256: String,
    pub after_lifecycle_snapshot_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub action: MtgoCompetitiveLifecycleActionV1,
    pub event_identity_sha256: String,
    pub match_identity_sha256: Option<String>,
    pub before_frame_id: u64,
    pub before_frame_sequence: u64,
    pub after_frame_id: u64,
    pub after_frame_sequence: u64,
    pub after_captured_at_unix_millis: u128,
    pub transition_commitment_sha256: String,
}

pub(crate) struct OpaqueMtgoConfirmedCompetitiveLifecycleControlPostconditionV1 {
    _before: OpaqueMtgoCompetitiveLifecycleControlV1,
    _after: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    _transition: CheckedUntrustedMtgoCompetitiveLifecycleTransitionV1,
    commitments: MtgoCompetitiveLifecycleControlTransitionCommitmentsV1,
}

impl OpaqueMtgoConfirmedCompetitiveLifecycleControlPostconditionV1 {
    pub(crate) fn commitments_v1(&self) -> MtgoCompetitiveLifecycleControlTransitionCommitmentsV1 {
        self.commitments.clone()
    }

    pub(crate) fn into_after_frame_v1(self) -> OpaqueMtgoClassifiedCompetitiveNavigationFrameV1 {
        self._after
    }
}

/// Consumes a complete 18-slice lifecycle classification and retains the
/// exact enabled control region for one non-entry lifecycle action. The region
/// is rehashed against the same opaque frame before the move-only result is
/// constructed. This is a control-detection boundary only and cannot input.
pub fn bind_classified_navigation_frame_to_lifecycle_control_v1(
    source: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    action: MtgoCompetitiveLifecycleActionV1,
) -> Result<OpaqueMtgoCompetitiveLifecycleControlV1, String> {
    let fact = select_lifecycle_control_fact_v1(&source._lifecycle, action)?;
    let fact_kind = fact.kind;
    let control_rect_client_px = fact.rect_client_px.clone();
    let control_region_sha256 = fact.content_sha256.clone();
    let control_confidence_bps = fact.confidence_bps;
    let lifecycle_snapshot_commitment_sha256 =
        source._lifecycle.snapshot_commitment_sha256().to_owned();
    let event_identity_sha256 = source
        ._lifecycle
        .event_identity_sha256_v1()
        .ok_or("lifecycle control source is missing its event identity")?
        .to_owned();
    let match_identity_sha256 = source
        ._lifecycle
        .match_identity_sha256_v1()
        .map(str::to_owned);
    let game_number = source._lifecycle.game_number_v1();
    let classified = source.commitments_v1();
    let raw_source = &source._source_frame.source_frame;
    let actual_region_sha256 = visible_frame_region_content_sha256_v1(
        &raw_source.canonical_bgra8,
        &MtgoSizePxV1 {
            width: raw_source.manifest.frame.canonical_width,
            height: raw_source.manifest.frame.canonical_height,
        },
        &control_rect_client_px,
    )
    .map_err(|error| format!("rehash lifecycle control pixels: {error}"))?;
    if actual_region_sha256 != control_region_sha256 {
        return Err("lifecycle control region differs from the retained source pixels".to_owned());
    }

    let source_frame = source._source_frame.commitments_v1();
    let control_binding_commitment_sha256 = commitment_v1(&[
        source_frame.frame_profile_binding_sha256.as_bytes(),
        classified.runtime_identity_commitment_sha256.as_bytes(),
        classified
            .classification_result_commitment_sha256
            .as_bytes(),
        lifecycle_snapshot_commitment_sha256.as_bytes(),
        event_tag_v1(classified.event_kind),
        phase_tag_v1(classified.phase),
        action_tag_v1(action),
        fact_tag_v1(fact_kind),
        event_identity_sha256.as_bytes(),
        match_identity_sha256.as_deref().unwrap_or("").as_bytes(),
        game_number
            .map_or([0_u8, 0_u8], |value| [1_u8, value])
            .as_slice(),
        classified.frame_id.to_be_bytes().as_slice(),
        classified.frame_sequence.to_be_bytes().as_slice(),
        control_rect_client_px.x.to_be_bytes().as_slice(),
        control_rect_client_px.y.to_be_bytes().as_slice(),
        control_rect_client_px.width.to_be_bytes().as_slice(),
        control_rect_client_px.height.to_be_bytes().as_slice(),
        control_region_sha256.as_bytes(),
        control_confidence_bps.to_be_bytes().as_slice(),
        b"exact_enabled_control_detection_only_no_coordinates_no_input",
    ]);
    let commitments = MtgoOpaqueCompetitiveLifecycleControlCommitmentsV1 {
        source_frame,
        approved_account_alias_sha256: classified.source_frame.approved_account_alias_sha256,
        runtime_identity_commitment_sha256: classified.runtime_identity_commitment_sha256,
        classification_result_commitment_sha256: classified.classification_result_commitment_sha256,
        lifecycle_snapshot_commitment_sha256,
        event_kind: classified.event_kind,
        phase: classified.phase,
        action,
        event_identity_sha256,
        match_identity_sha256,
        game_number,
        frame_id: classified.frame_id,
        frame_sequence: classified.frame_sequence,
        control_region_sha256,
        control_confidence_bps,
        control_binding_commitment_sha256,
    };
    Ok(OpaqueMtgoCompetitiveLifecycleControlV1 {
        _source: source,
        _control_rect_client_px: control_rect_client_px,
        commitments,
    })
}

pub(crate) fn confirm_opaque_competitive_lifecycle_control_postcondition_v1(
    before: OpaqueMtgoCompetitiveLifecycleControlV1,
    after: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    authorization: &MtgoAuthorizationScopeV1,
    input_sent_at_unix_millis: u128,
) -> Result<OpaqueMtgoConfirmedCompetitiveLifecycleControlPostconditionV1, String> {
    let before_commitments = before.commitments_v1();
    let after_commitments = after.commitments_v1();
    if before_commitments.source_frame.profile_commitment_sha256
        != after_commitments.source_frame.profile_commitment_sha256
        || before_commitments
            .source_frame
            .profile_admission_commitment_sha256
            != after_commitments
                .source_frame
                .profile_admission_commitment_sha256
        || before_commitments.approved_account_alias_sha256
            != after_commitments.source_frame.approved_account_alias_sha256
        || before_commitments.runtime_identity_commitment_sha256
            != after_commitments.runtime_identity_commitment_sha256
        || before_commitments.event_kind != after_commitments.event_kind
        || after_commitments.frame_id == before_commitments.frame_id
        || after_commitments.frame_sequence <= before_commitments.frame_sequence
    {
        return Err(
            "lifecycle control postcondition changed profile, account, runtime, mode, or frame order"
                .to_owned(),
        );
    }
    let before_raw = &before._source._source_frame.source_frame;
    let after_raw = &after._source_frame.source_frame;
    if before_raw.manifest.pre.process_id != after_raw.manifest.pre.process_id
        || before_raw.manifest.pre.process_start_filetime_100ns
            != after_raw.manifest.pre.process_start_filetime_100ns
        || before_raw.manifest.pre.hwnd != after_raw.manifest.pre.hwnd
        || before_raw.manifest.pre.dpi != after_raw.manifest.pre.dpi
        || before_raw.manifest.pre.client_rect_desktop_px
            != after_raw.manifest.pre.client_rect_desktop_px
        || before_raw.manifest.output.device_name != after_raw.manifest.output.device_name
        || before_raw.manifest.output.bounds_desktop_px
            != after_raw.manifest.output.bounds_desktop_px
        || before_raw.manifest.captured_at_unix_millis > input_sent_at_unix_millis
        || after_raw.manifest.captured_at_unix_millis <= input_sent_at_unix_millis
    {
        return Err(
            "lifecycle control postcondition changed the client incarnation or capture timeline"
                .to_owned(),
        );
    }
    let intent = make_offline_competitive_lifecycle_intent_v1(
        &before._source._lifecycle,
        before_commitments.action,
        authorization,
        None,
    )
    .map_err(|error| format!("build lifecycle control intent: {error}"))?;
    let transition = validate_checked_competitive_lifecycle_action_transition_v1(
        &before._source._lifecycle,
        &intent,
        authorization,
        None,
        &after._lifecycle,
    )
    .map_err(|error| format!("validate lifecycle control postcondition: {error}"))?;
    let after_capture_commitment_sha256 = after_commitments
        .source_frame
        .source_capture
        .capture_commitment_sha256
        .clone();
    let after_lifecycle_snapshot_commitment_sha256 = after_commitments
        .lifecycle_snapshot_commitment_sha256
        .clone();
    let transition_commitment_sha256 = commitment_with_domain_v1(
        COMPETITIVE_LIFECYCLE_CONTROL_TRANSITION_DOMAIN_V1,
        &[
            before_commitments
                .control_binding_commitment_sha256
                .as_bytes(),
            before_commitments
                .source_frame
                .source_capture
                .capture_commitment_sha256
                .as_bytes(),
            before_commitments
                .classification_result_commitment_sha256
                .as_bytes(),
            before_commitments
                .lifecycle_snapshot_commitment_sha256
                .as_bytes(),
            after_capture_commitment_sha256.as_bytes(),
            after_commitments
                .classification_result_commitment_sha256
                .as_bytes(),
            after_lifecycle_snapshot_commitment_sha256.as_bytes(),
            transition.source_snapshot_commitment_sha256().as_bytes(),
            transition.next_snapshot_commitment_sha256().as_bytes(),
            input_sent_at_unix_millis.to_be_bytes().as_slice(),
            after_raw
                .manifest
                .captured_at_unix_millis
                .to_be_bytes()
                .as_slice(),
            b"strictly_newer_exact_action_postcondition_no_input_authority",
        ],
    );
    let commitments = MtgoCompetitiveLifecycleControlTransitionCommitmentsV1 {
        control_binding_commitment_sha256: before_commitments.control_binding_commitment_sha256,
        before_capture_commitment_sha256: before_commitments
            .source_frame
            .source_capture
            .capture_commitment_sha256,
        before_classification_result_commitment_sha256: before_commitments
            .classification_result_commitment_sha256,
        before_lifecycle_snapshot_commitment_sha256: before_commitments
            .lifecycle_snapshot_commitment_sha256,
        after_capture_commitment_sha256,
        after_classification_result_commitment_sha256: after_commitments
            .classification_result_commitment_sha256,
        after_lifecycle_snapshot_commitment_sha256,
        event_kind: before_commitments.event_kind,
        action: before_commitments.action,
        event_identity_sha256: before_commitments.event_identity_sha256,
        match_identity_sha256: before_commitments.match_identity_sha256,
        before_frame_id: before_commitments.frame_id,
        before_frame_sequence: before_commitments.frame_sequence,
        after_frame_id: after_commitments.frame_id,
        after_frame_sequence: after_commitments.frame_sequence,
        after_captured_at_unix_millis: after_raw.manifest.captured_at_unix_millis,
        transition_commitment_sha256,
    };
    Ok(
        OpaqueMtgoConfirmedCompetitiveLifecycleControlPostconditionV1 {
            _before: before,
            _after: after,
            _transition: transition,
            commitments,
        },
    )
}

fn select_lifecycle_control_fact_v1(
    lifecycle: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    action: MtgoCompetitiveLifecycleActionV1,
) -> Result<&MtgoLifecycleVisibleFactV1, String> {
    let expected_kind = match (lifecycle.phase(), action) {
        (
            MtgoCompetitiveLifecyclePhaseV1::PairingReady,
            MtgoCompetitiveLifecycleActionV1::AcceptPairing,
        ) => MtgoLifecycleVisibleFactKindV1::PairingAcceptControlEnabled,
        (
            MtgoCompetitiveLifecyclePhaseV1::Sideboarding,
            MtgoCompetitiveLifecycleActionV1::SubmitSideboard,
        ) => MtgoLifecycleVisibleFactKindV1::SideboardSubmitControlEnabled,
        (
            MtgoCompetitiveLifecyclePhaseV1::MatchComplete,
            MtgoCompetitiveLifecycleActionV1::ContinueAfterMatch,
        ) => MtgoLifecycleVisibleFactKindV1::MatchContinueControlEnabled,
        (
            MtgoCompetitiveLifecyclePhaseV1::Reconnect,
            MtgoCompetitiveLifecycleActionV1::ResumeMatch,
        ) => MtgoLifecycleVisibleFactKindV1::ReconnectResumeControlEnabled,
        (
            MtgoCompetitiveLifecyclePhaseV1::EventComplete,
            MtgoCompetitiveLifecycleActionV1::CloseCompletedEvent,
        ) => MtgoLifecycleVisibleFactKindV1::EventCloseControlEnabled,
        _ => {
            return Err(
                "the requested non-entry lifecycle action is unavailable in the source phase"
                    .to_owned(),
            )
        }
    };
    if action == MtgoCompetitiveLifecycleActionV1::SubmitSideboard
        && !lifecycle
            .visible_facts_v1()
            .iter()
            .any(|fact| fact.kind == MtgoLifecycleVisibleFactKindV1::SideboardNoChangesConfirmed)
    {
        return Err(
            "generic sideboard control binding requires an explicitly visible no-change state"
                .to_owned(),
        );
    }
    lifecycle
        .visible_facts_v1()
        .iter()
        .find(|fact| fact.kind == expected_kind)
        .ok_or_else(|| "the source is missing the exact enabled lifecycle control".to_owned())
}

fn event_tag_v1(event: MtgoCompetitiveEventKindV1) -> &'static [u8] {
    match event {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    }
}

fn phase_tag_v1(phase: MtgoCompetitiveLifecyclePhaseV1) -> &'static [u8] {
    match phase {
        MtgoCompetitiveLifecyclePhaseV1::EventBrowser => b"event_browser",
        MtgoCompetitiveLifecyclePhaseV1::EntryReview => b"entry_review",
        MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing => b"entered_waiting",
        MtgoCompetitiveLifecyclePhaseV1::PairingReady => b"pairing_ready",
        MtgoCompetitiveLifecyclePhaseV1::MatchInProgress => b"match_in_progress",
        MtgoCompetitiveLifecyclePhaseV1::Sideboarding => b"sideboarding",
        MtgoCompetitiveLifecyclePhaseV1::MatchComplete => b"match_complete",
        MtgoCompetitiveLifecyclePhaseV1::EventComplete => b"event_complete",
        MtgoCompetitiveLifecyclePhaseV1::Reconnect => b"reconnect",
    }
}

fn action_tag_v1(action: MtgoCompetitiveLifecycleActionV1) -> &'static [u8] {
    match action {
        MtgoCompetitiveLifecycleActionV1::OpenEntryReview => b"open_entry_review",
        MtgoCompetitiveLifecycleActionV1::CancelEntry => b"cancel_entry",
        MtgoCompetitiveLifecycleActionV1::ConfirmEntry => b"confirm_entry",
        MtgoCompetitiveLifecycleActionV1::AcceptPairing => b"accept_pairing",
        MtgoCompetitiveLifecycleActionV1::SubmitSideboard => b"submit_sideboard",
        MtgoCompetitiveLifecycleActionV1::ContinueAfterMatch => b"continue_after_match",
        MtgoCompetitiveLifecycleActionV1::ResumeMatch => b"resume_match",
        MtgoCompetitiveLifecycleActionV1::CloseCompletedEvent => b"close_completed_event",
    }
}

fn fact_tag_v1(kind: MtgoLifecycleVisibleFactKindV1) -> &'static [u8] {
    match kind {
        MtgoLifecycleVisibleFactKindV1::PairingAcceptControlEnabled => b"pairing_accept",
        MtgoLifecycleVisibleFactKindV1::SideboardSubmitControlEnabled => b"sideboard_submit",
        MtgoLifecycleVisibleFactKindV1::MatchContinueControlEnabled => b"match_continue",
        MtgoLifecycleVisibleFactKindV1::EventCloseControlEnabled => b"event_close",
        MtgoLifecycleVisibleFactKindV1::ReconnectResumeControlEnabled => b"reconnect_resume",
        _ => b"not_a_lifecycle_control",
    }
}

fn commitment_v1(parts: &[&[u8]]) -> String {
    commitment_with_domain_v1(COMPETITIVE_LIFECYCLE_CONTROL_BINDING_DOMAIN_V1, parts)
}

fn commitment_with_domain_v1(domain: &[u8], parts: &[&[u8]]) -> String {
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
        validate_visible_competitive_lifecycle_snapshot_v1,
        MtgoVisibleCompetitiveLifecycleSnapshotV1,
    };

    fn lifecycle_v1(
        phase: MtgoCompetitiveLifecyclePhaseV1,
    ) -> CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1 {
        let kinds: &[MtgoLifecycleVisibleFactKindV1] = match phase {
            MtgoCompetitiveLifecyclePhaseV1::PairingReady => &[
                MtgoLifecycleVisibleFactKindV1::PairingVisible,
                MtgoLifecycleVisibleFactKindV1::PairingAcceptControlEnabled,
            ],
            MtgoCompetitiveLifecyclePhaseV1::Sideboarding => &[
                MtgoLifecycleVisibleFactKindV1::SideboardSurfaceVisible,
                MtgoLifecycleVisibleFactKindV1::SideboardTimerVisible,
                MtgoLifecycleVisibleFactKindV1::SideboardConfigurationVisible,
                MtgoLifecycleVisibleFactKindV1::SideboardNoChangesConfirmed,
                MtgoLifecycleVisibleFactKindV1::SideboardSubmitControlEnabled,
            ],
            MtgoCompetitiveLifecyclePhaseV1::MatchComplete => &[
                MtgoLifecycleVisibleFactKindV1::MatchResultVisible,
                MtgoLifecycleVisibleFactKindV1::MatchContinueControlEnabled,
            ],
            MtgoCompetitiveLifecyclePhaseV1::EventComplete => &[
                MtgoLifecycleVisibleFactKindV1::EventResultVisible,
                MtgoLifecycleVisibleFactKindV1::EventCloseControlEnabled,
            ],
            MtgoCompetitiveLifecyclePhaseV1::Reconnect => &[
                MtgoLifecycleVisibleFactKindV1::ReconnectVisible,
                MtgoLifecycleVisibleFactKindV1::ReconnectResumeControlEnabled,
            ],
            _ => &[MtgoLifecycleVisibleFactKindV1::EnteredEventVisible],
        };
        let match_phase = matches!(
            phase,
            MtgoCompetitiveLifecyclePhaseV1::PairingReady
                | MtgoCompetitiveLifecyclePhaseV1::Sideboarding
                | MtgoCompetitiveLifecyclePhaseV1::MatchComplete
                | MtgoCompetitiveLifecyclePhaseV1::Reconnect
        );
        let game_phase = matches!(
            phase,
            MtgoCompetitiveLifecyclePhaseV1::Sideboarding
                | MtgoCompetitiveLifecyclePhaseV1::Reconnect
        );
        validate_visible_competitive_lifecycle_snapshot_v1(
            MtgoVisibleCompetitiveLifecycleSnapshotV1 {
                schema_version: 1,
                snapshot_id: format!("lifecycle-control-{phase:?}").to_ascii_lowercase(),
                event_kind: MtgoCompetitiveEventKindV1::League,
                phase,
                frame_id: 7,
                frame_sequence: 9,
                frame_sha256: "1".repeat(64),
                client_bounds: MtgoRectPxV1 {
                    x: 0,
                    y: 0,
                    width: 1_550,
                    height: 925,
                },
                event_identity_sha256: Some("2".repeat(64)),
                match_identity_sha256: match_phase.then(|| "3".repeat(64)),
                game_number: game_phase.then_some(2),
                entry_terms: None,
                visible_state_complete: true,
                facts: kinds
                    .iter()
                    .enumerate()
                    .map(|(index, kind)| MtgoLifecycleVisibleFactV1 {
                        kind: *kind,
                        rect_client_px: MtgoRectPxV1 {
                            x: 100 + u32::try_from(index).unwrap() * 100,
                            y: 100,
                            width: 80,
                            height: 40,
                        },
                        content_sha256: format!("{:064x}", index + 20),
                        confidence_bps: 10_000,
                    })
                    .collect(),
            },
        )
        .unwrap()
    }

    #[test]
    fn every_non_entry_lifecycle_action_selects_only_its_enabled_control() {
        let cases = [
            (
                MtgoCompetitiveLifecyclePhaseV1::PairingReady,
                MtgoCompetitiveLifecycleActionV1::AcceptPairing,
                MtgoLifecycleVisibleFactKindV1::PairingAcceptControlEnabled,
            ),
            (
                MtgoCompetitiveLifecyclePhaseV1::Sideboarding,
                MtgoCompetitiveLifecycleActionV1::SubmitSideboard,
                MtgoLifecycleVisibleFactKindV1::SideboardSubmitControlEnabled,
            ),
            (
                MtgoCompetitiveLifecyclePhaseV1::MatchComplete,
                MtgoCompetitiveLifecycleActionV1::ContinueAfterMatch,
                MtgoLifecycleVisibleFactKindV1::MatchContinueControlEnabled,
            ),
            (
                MtgoCompetitiveLifecyclePhaseV1::Reconnect,
                MtgoCompetitiveLifecycleActionV1::ResumeMatch,
                MtgoLifecycleVisibleFactKindV1::ReconnectResumeControlEnabled,
            ),
            (
                MtgoCompetitiveLifecyclePhaseV1::EventComplete,
                MtgoCompetitiveLifecycleActionV1::CloseCompletedEvent,
                MtgoLifecycleVisibleFactKindV1::EventCloseControlEnabled,
            ),
        ];
        for (phase, action, expected_kind) in cases {
            let lifecycle = lifecycle_v1(phase);
            let selected = select_lifecycle_control_fact_v1(&lifecycle, action).unwrap();
            assert_eq!(selected.kind, expected_kind);
        }
    }

    #[test]
    fn entry_actions_and_cross_phase_controls_fail_closed() {
        let pairing = lifecycle_v1(MtgoCompetitiveLifecyclePhaseV1::PairingReady);
        for action in [
            MtgoCompetitiveLifecycleActionV1::OpenEntryReview,
            MtgoCompetitiveLifecycleActionV1::CancelEntry,
            MtgoCompetitiveLifecycleActionV1::ConfirmEntry,
            MtgoCompetitiveLifecycleActionV1::SubmitSideboard,
            MtgoCompetitiveLifecycleActionV1::ContinueAfterMatch,
            MtgoCompetitiveLifecycleActionV1::ResumeMatch,
            MtgoCompetitiveLifecycleActionV1::CloseCompletedEvent,
        ] {
            assert!(select_lifecycle_control_fact_v1(&pairing, action).is_err());
        }
    }
}
