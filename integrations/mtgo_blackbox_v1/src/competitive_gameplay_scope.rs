use crate::{
    check_untrusted_profile_bound_action_postcondition_pixels_v1,
    check_untrusted_profile_bound_postcondition_before_input_pixels_v1,
    inspect_untrusted_profile_bound_action_postcondition_candidate_pixels_v1,
    validate_authorization_for_mode_v1, CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    CheckedUntrustedMtgoProfileBoundActionPostconditionPlanV1,
    CheckedUntrustedMtgoProfileBoundActionPostconditionV1, MtgoAuthorizationScopeV1,
    MtgoCompetitiveEventKindV1, MtgoCompetitiveLifecyclePhaseV1, MtgoContractErrorV1,
    MtgoProfileBoundActionPostconditionPlanCommitmentsV1,
    MtgoProfileBoundPostconditionAfterFrameMetadataV1,
    MtgoProfileBoundPostconditionBeforeInputFrameV1,
    MtgoProfileBoundPostconditionCandidateStatusV1, MtgoRuntimeModeV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MTGO_COMPETITIVE_MATCH_GAMEPLAY_AUTHORIZATION_SCHEMA_V1: u32 = 1;
const COMPETITIVE_GAMEPLAY_SCOPE_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-gameplay-action-scope-v1";
const COMPETITIVE_MODE_AUTHORIZATION_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-mode-authorization-v1";
const COMPETITIVE_GAMEPLAY_POSTCONDITION_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-gameplay-postcondition-v1";
const COMPETITIVE_GAMEPLAY_BEFORE_INPUT_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-gameplay-before-input-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveMatchGameplayAuthorizationV1 {
    pub schema_version: u32,
    pub account_alias_sha256: String,
    pub written_permission_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub match_identity_sha256: String,
    pub game_number: u8,
    pub entry_authorization_sha256: String,
    pub owner_launch_authorization_sha256: String,
    pub exact_match_gameplay_authorized: bool,
    pub valid_through_frame_sequence: u64,
}

/// One exact model action and visible postcondition plan scoped to one visible
/// competitive match, game, account, written permission, entry authorization,
/// and owner launch authorization.
///
/// This result remains checked-untrusted because the offline lifecycle and
/// gameplay interpretations are not opaque in-process capture proofs. It has
/// no coordinate or input conversion.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoCompetitiveGameplayActionPlanV1;
/// fn cannot_act(value: &CheckedUntrustedMtgoCompetitiveGameplayActionPlanV1) {
///     let _ = value.input_command();
///     let _ = value.target_point_client_px();
/// }
/// ```
pub struct CheckedUntrustedMtgoCompetitiveGameplayActionPlanV1 {
    plan: CheckedUntrustedMtgoProfileBoundActionPostconditionPlanV1,
    _lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    event_kind: MtgoCompetitiveEventKindV1,
    game_number: u8,
    gameplay_authorization_valid_through_frame_sequence: u64,
    mode_authorization_commitment_sha256: String,
    authorization_commitment_sha256: String,
    competitive_scope_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveGameplayActionPlanV1 {
    pub fn event_kind(&self) -> MtgoCompetitiveEventKindV1 {
        self.event_kind
    }

    pub fn game_number(&self) -> u8 {
        self.game_number
    }

    pub fn gameplay_authorization_valid_through_frame_sequence(&self) -> u64 {
        self.gameplay_authorization_valid_through_frame_sequence
    }

    pub fn postcondition_plan_commitment_sha256(&self) -> &str {
        self.plan.plan_commitment_sha256()
    }

    pub fn authorization_commitment_sha256(&self) -> &str {
        &self.authorization_commitment_sha256
    }

    pub fn mode_authorization_commitment_sha256(&self) -> &str {
        &self.mode_authorization_commitment_sha256
    }

    pub fn competitive_scope_commitment_sha256(&self) -> &str {
        &self.competitive_scope_commitment_sha256
    }

    pub fn postcondition_plan_commitments_v1(
        &self,
    ) -> MtgoProfileBoundActionPostconditionPlanCommitmentsV1 {
        self.plan.commitments_v1()
    }

    pub fn safe_for_live_input(&self) -> bool {
        false
    }

    pub fn permits_event_entry(&self) -> bool {
        false
    }
}

/// One exact competitive action whose complete calibrated pixel regions
/// changed on a newer frame. This remains checked-untrusted until an opaque
/// capture path supplies the bytes and metadata.
pub struct CheckedUntrustedMtgoCompetitiveGameplayPostconditionV1 {
    _postcondition: CheckedUntrustedMtgoProfileBoundActionPostconditionV1,
    event_kind: MtgoCompetitiveEventKindV1,
    event_identity_sha256: String,
    match_identity_sha256: String,
    game_number: u8,
    player_visible_decision: crate::MtgoPlayerVisibleConfirmedDuelDecisionV1,
    decision_commitment_sha256: String,
    selection_commitment_sha256: String,
    deployment_commitment_sha256: String,
    source_frame_id: u64,
    source_frame_sequence: u64,
    source_frame_sha256: String,
    after_frame_id: u64,
    after_frame_sequence: u64,
    confirmation_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveGameplayPostconditionV1 {
    pub fn event_kind(&self) -> MtgoCompetitiveEventKindV1 {
        self.event_kind
    }

    pub fn game_number(&self) -> u8 {
        self.game_number
    }

    pub fn event_identity_sha256_v1(&self) -> &str {
        &self.event_identity_sha256
    }

    pub fn match_identity_sha256_v1(&self) -> &str {
        &self.match_identity_sha256
    }

    pub fn player_visible_decision_v1(&self) -> &crate::MtgoPlayerVisibleConfirmedDuelDecisionV1 {
        &self.player_visible_decision
    }

    pub fn decision_commitment_sha256_v1(&self) -> &str {
        &self.decision_commitment_sha256
    }

    pub fn selection_commitment_sha256_v1(&self) -> &str {
        &self.selection_commitment_sha256
    }

    pub fn deployment_commitment_sha256_v1(&self) -> &str {
        &self.deployment_commitment_sha256
    }

    pub fn source_frame_id_v1(&self) -> u64 {
        self.source_frame_id
    }

    pub fn source_frame_sequence_v1(&self) -> u64 {
        self.source_frame_sequence
    }

    pub fn source_frame_sha256_v1(&self) -> &str {
        &self.source_frame_sha256
    }

    pub fn after_frame_id(&self) -> u64 {
        self.after_frame_id
    }

    pub fn after_frame_sequence(&self) -> u64 {
        self.after_frame_sequence
    }

    pub fn confirmation_commitment_sha256(&self) -> &str {
        &self.confirmation_commitment_sha256
    }

    pub fn safe_for_additional_input(&self) -> bool {
        false
    }

    pub fn permits_event_entry(&self) -> bool {
        false
    }
}

pub struct CheckedUntrustedMtgoCompetitiveGameplayBeforeInputV1 {
    frame_id: u64,
    frame_sequence: u64,
    canonical_bgra8_sha256: String,
    verification_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveGameplayBeforeInputV1 {
    pub fn frame_id(&self) -> u64 {
        self.frame_id
    }

    pub fn frame_sequence(&self) -> u64 {
        self.frame_sequence
    }

    pub fn canonical_bgra8_sha256(&self) -> &str {
        &self.canonical_bgra8_sha256
    }

    pub fn verification_commitment_sha256(&self) -> &str {
        &self.verification_commitment_sha256
    }

    pub fn safe_for_input(&self) -> bool {
        false
    }
}

pub fn bind_profile_bound_action_plan_to_competitive_match_v1(
    plan: CheckedUntrustedMtgoProfileBoundActionPostconditionPlanV1,
    lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    mode_authorization: &MtgoAuthorizationScopeV1,
    gameplay_authorization: &MtgoCompetitiveMatchGameplayAuthorizationV1,
) -> Result<CheckedUntrustedMtgoCompetitiveGameplayActionPlanV1, MtgoContractErrorV1> {
    let mode_authorization_commitment_sha256 =
        competitive_mode_authorization_commitment_v1(mode_authorization, lifecycle.event_kind())?;
    if lifecycle.phase() != MtgoCompetitiveLifecyclePhaseV1::MatchInProgress {
        return Err(error_v1(
            "competitive_gameplay_lifecycle_phase",
            "model gameplay actions require the visible match-in-progress phase",
        ));
    }
    let plan_size = plan.source_client_size_px_v1();
    let lifecycle_bounds = lifecycle.client_bounds_v1();
    if lifecycle.frame_id_v1() != plan.source_frame_id_v1()
        || lifecycle.frame_sequence() != plan.source_frame_sequence_v1()
        || lifecycle.frame_sha256_v1() != plan.source_frame_sha256_v1()
        || lifecycle_bounds.x != 0
        || lifecycle_bounds.y != 0
        || lifecycle_bounds.width != plan_size.width
        || lifecycle_bounds.height != plan_size.height
    {
        return Err(error_v1(
            "competitive_gameplay_source_frame_mismatch",
            "lifecycle and model action plans must interpret the exact same source frame",
        ));
    }
    validate_gameplay_authorization_v1(&lifecycle, mode_authorization, gameplay_authorization)?;

    let authorization_commitment_sha256 =
        competitive_match_gameplay_authorization_commitment_v1(gameplay_authorization)?;
    let mode_bytes = serde_json::to_vec(mode_authorization)
        .map_err(|error| error_v1("competitive_gameplay_mode_serialization", error.to_string()))?;
    let competitive_scope_commitment_sha256 = commitment_v1(
        COMPETITIVE_GAMEPLAY_SCOPE_COMMITMENT_DOMAIN_V1,
        &[
            plan.plan_commitment_sha256().as_bytes(),
            lifecycle.snapshot_commitment_sha256().as_bytes(),
            mode_bytes.as_slice(),
            mode_authorization_commitment_sha256.as_bytes(),
            authorization_commitment_sha256.as_bytes(),
            b"checked_untrusted_no_input_or_event_entry",
        ],
    );
    Ok(CheckedUntrustedMtgoCompetitiveGameplayActionPlanV1 {
        event_kind: lifecycle.event_kind(),
        game_number: lifecycle
            .game_number_v1()
            .expect("validated match phase has a game"),
        gameplay_authorization_valid_through_frame_sequence: gameplay_authorization
            .valid_through_frame_sequence,
        plan,
        _lifecycle: lifecycle,
        mode_authorization_commitment_sha256,
        authorization_commitment_sha256,
        competitive_scope_commitment_sha256,
    })
}

/// Commits one exact authorization document to one competitive runtime mode.
/// The complete scope is included, so enabling or disabling any other mode
/// changes this commitment even when the selected mode remains authorized.
pub fn competitive_mode_authorization_commitment_v1(
    scope: &MtgoAuthorizationScopeV1,
    event_kind: MtgoCompetitiveEventKindV1,
) -> Result<String, MtgoContractErrorV1> {
    let runtime_mode = match event_kind {
        MtgoCompetitiveEventKindV1::League => MtgoRuntimeModeV1::LeagueInput,
        MtgoCompetitiveEventKindV1::Challenge => MtgoRuntimeModeV1::ChallengeInput,
    };
    validate_authorization_for_mode_v1(scope, runtime_mode)?;
    let scope_bytes = serde_json::to_vec(scope)
        .map_err(|error| error_v1("competitive_gameplay_mode_serialization", error.to_string()))?;
    let event_kind_bytes = serde_json::to_vec(&event_kind).map_err(|error| {
        error_v1(
            "competitive_gameplay_event_kind_serialization",
            error.to_string(),
        )
    })?;
    Ok(commitment_v1(
        COMPETITIVE_MODE_AUTHORIZATION_COMMITMENT_DOMAIN_V1,
        &[scope_bytes.as_slice(), event_kind_bytes.as_slice()],
    ))
}

pub fn competitive_match_gameplay_authorization_commitment_v1(
    authorization: &MtgoCompetitiveMatchGameplayAuthorizationV1,
) -> Result<String, MtgoContractErrorV1> {
    let authorization_bytes = serde_json::to_vec(authorization).map_err(|error| {
        error_v1(
            "competitive_gameplay_authorization_serialization",
            error.to_string(),
        )
    })?;
    Ok(commitment_v1(
        b"mtgo-competitive-match-gameplay-authorization-v1",
        &[authorization_bytes.as_slice()],
    ))
}

pub fn check_untrusted_competitive_gameplay_postcondition_pixels_v1(
    competitive: CheckedUntrustedMtgoCompetitiveGameplayActionPlanV1,
    metadata: MtgoProfileBoundPostconditionAfterFrameMetadataV1,
    canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoCompetitiveGameplayPostconditionV1, MtgoContractErrorV1> {
    let event_kind = competitive.event_kind;
    let game_number = competitive.game_number;
    let event_identity_sha256 = competitive
        ._lifecycle
        .event_identity_sha256_v1()
        .expect("validated match phase has an event identity")
        .to_owned();
    let match_identity_sha256 = competitive
        ._lifecycle
        .match_identity_sha256_v1()
        .expect("validated match phase has a match identity")
        .to_owned();
    let plan_commitments = competitive.plan.commitments_v1();
    let player_visible_decision = competitive.plan.player_visible_confirmed_decision_v1()?;
    let mode_authorization_commitment_sha256 =
        competitive.mode_authorization_commitment_sha256.clone();
    let authorization_commitment_sha256 = competitive.authorization_commitment_sha256.clone();
    let competitive_scope_commitment_sha256 =
        competitive.competitive_scope_commitment_sha256.clone();
    let postcondition = check_untrusted_profile_bound_action_postcondition_pixels_v1(
        competitive.plan,
        metadata,
        canonical_bgra8,
    )?;
    let after_frame_id = postcondition.after_frame_id();
    let after_frame_sequence = postcondition.after_frame_sequence();
    let event_kind_bytes = serde_json::to_vec(&event_kind).map_err(|error| {
        error_v1(
            "competitive_gameplay_postcondition_event_serialization",
            error.to_string(),
        )
    })?;
    let confirmation_commitment_sha256 = commitment_v1(
        COMPETITIVE_GAMEPLAY_POSTCONDITION_COMMITMENT_DOMAIN_V1,
        &[
            mode_authorization_commitment_sha256.as_bytes(),
            authorization_commitment_sha256.as_bytes(),
            competitive_scope_commitment_sha256.as_bytes(),
            postcondition.confirmation_commitment_sha256().as_bytes(),
            event_kind_bytes.as_slice(),
            &[game_number],
            &after_frame_id.to_le_bytes(),
            &after_frame_sequence.to_le_bytes(),
            b"checked_untrusted_no_additional_input_or_event_entry",
        ],
    );
    Ok(CheckedUntrustedMtgoCompetitiveGameplayPostconditionV1 {
        _postcondition: postcondition,
        event_kind,
        event_identity_sha256,
        match_identity_sha256,
        game_number,
        player_visible_decision,
        decision_commitment_sha256: plan_commitments.decision_commitment_sha256,
        selection_commitment_sha256: plan_commitments.selection_commitment_sha256,
        deployment_commitment_sha256: plan_commitments.deployment_commitment_sha256,
        source_frame_id: plan_commitments.frame_id,
        source_frame_sequence: plan_commitments.frame_sequence,
        source_frame_sha256: plan_commitments.source_frame_sha256,
        after_frame_id,
        after_frame_sequence,
        confirmation_commitment_sha256,
    })
}

/// Inspects one newer competitive frame without consuming the exact match
/// plan. Only a still-incomplete visible change is retryable. The result has no
/// input, event-entry, or next-action authority.
pub fn inspect_untrusted_competitive_gameplay_postcondition_candidate_pixels_v1(
    competitive: &CheckedUntrustedMtgoCompetitiveGameplayActionPlanV1,
    metadata: MtgoProfileBoundPostconditionAfterFrameMetadataV1,
    canonical_bgra8: &[u8],
) -> Result<MtgoProfileBoundPostconditionCandidateStatusV1, MtgoContractErrorV1> {
    inspect_untrusted_profile_bound_action_postcondition_candidate_pixels_v1(
        &competitive.plan,
        metadata,
        canonical_bgra8,
    )
}

pub fn check_untrusted_competitive_gameplay_before_input_pixels_v1(
    competitive: &CheckedUntrustedMtgoCompetitiveGameplayActionPlanV1,
    before: MtgoProfileBoundPostconditionBeforeInputFrameV1,
    canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoCompetitiveGameplayBeforeInputV1, MtgoContractErrorV1> {
    let checked = check_untrusted_profile_bound_postcondition_before_input_pixels_v1(
        &competitive.plan,
        before,
        canonical_bgra8,
    )?;
    let event_kind_bytes = serde_json::to_vec(&competitive.event_kind).map_err(|error| {
        error_v1(
            "competitive_gameplay_before_input_event_serialization",
            error.to_string(),
        )
    })?;
    let verification_commitment_sha256 = commitment_v1(
        COMPETITIVE_GAMEPLAY_BEFORE_INPUT_COMMITMENT_DOMAIN_V1,
        &[
            competitive.mode_authorization_commitment_sha256.as_bytes(),
            competitive.authorization_commitment_sha256.as_bytes(),
            competitive.competitive_scope_commitment_sha256.as_bytes(),
            checked.verification_commitment_sha256().as_bytes(),
            event_kind_bytes.as_slice(),
            &[competitive.game_number],
            b"checked_untrusted_before_input_no_input_or_event_entry_authority",
        ],
    );
    Ok(CheckedUntrustedMtgoCompetitiveGameplayBeforeInputV1 {
        frame_id: checked.frame_id(),
        frame_sequence: checked.frame_sequence(),
        canonical_bgra8_sha256: checked.canonical_bgra8_sha256().to_owned(),
        verification_commitment_sha256,
    })
}

fn validate_gameplay_authorization_v1(
    lifecycle: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    mode: &MtgoAuthorizationScopeV1,
    authorization: &MtgoCompetitiveMatchGameplayAuthorizationV1,
) -> Result<(), MtgoContractErrorV1> {
    if authorization.schema_version != MTGO_COMPETITIVE_MATCH_GAMEPLAY_AUTHORIZATION_SCHEMA_V1 {
        return Err(error_v1(
            "competitive_gameplay_authorization_schema",
            authorization.schema_version.to_string(),
        ));
    }
    if !authorization.exact_match_gameplay_authorized
        || authorization.valid_through_frame_sequence < lifecycle.frame_sequence()
    {
        return Err(error_v1(
            "competitive_gameplay_authorization_inactive",
            "exact match gameplay authorization must be active for the source frame",
        ));
    }
    for (value, code) in [
        (
            authorization.account_alias_sha256.as_str(),
            "competitive_gameplay_account_hash",
        ),
        (
            authorization.written_permission_sha256.as_str(),
            "competitive_gameplay_permission_hash",
        ),
        (
            authorization.event_identity_sha256.as_str(),
            "competitive_gameplay_event_hash",
        ),
        (
            authorization.match_identity_sha256.as_str(),
            "competitive_gameplay_match_hash",
        ),
        (
            authorization.entry_authorization_sha256.as_str(),
            "competitive_gameplay_entry_hash",
        ),
        (
            authorization.owner_launch_authorization_sha256.as_str(),
            "competitive_gameplay_owner_launch_hash",
        ),
    ] {
        require_sha256_v1(value, code)?;
    }
    if authorization.entry_authorization_sha256 == authorization.written_permission_sha256
        || authorization.owner_launch_authorization_sha256
            == authorization.written_permission_sha256
        || authorization.owner_launch_authorization_sha256
            == authorization.entry_authorization_sha256
    {
        return Err(error_v1(
            "competitive_gameplay_authorization_record_reuse",
            "written permission, exact entry, and owner launch must be separate records",
        ));
    }
    if authorization.account_alias_sha256 != mode.account_alias_sha256
        || authorization.written_permission_sha256 != mode.written_permission_sha256
        || authorization.event_kind != lifecycle.event_kind()
        || lifecycle.event_identity_sha256_v1()
            != Some(authorization.event_identity_sha256.as_str())
        || lifecycle.match_identity_sha256_v1()
            != Some(authorization.match_identity_sha256.as_str())
        || lifecycle.game_number_v1() != Some(authorization.game_number)
    {
        return Err(error_v1(
            "competitive_gameplay_authorization_binding",
            "authorization must bind the exact account, permission, event, match, and game",
        ));
    }
    Ok(())
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

fn require_sha256_v1(value: &str, code: &'static str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1(code, value));
    }
    Ok(())
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
pub(crate) use tests::{
    competitive_gameplay_postcondition_for_match_memory_test_v1,
    later_competitive_gameplay_postcondition_for_match_memory_test_v1,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        prepare_profile_bound_action_postcondition_plan_v1,
        profile_bound_resolved_action_control_for_test_v1,
        validate_visible_competitive_lifecycle_snapshot_v1,
        validate_visible_object_action_calibration_trace_v1, MtgoDuelVisiblePostconditionKindV1,
        MtgoLifecycleVisibleFactKindV1, MtgoLifecycleVisibleFactV1,
        MtgoProfileBoundPostconditionCalibrationV1, MtgoProfileBoundPostconditionRegionCandidateV1,
        MtgoProfileBoundPostconditionRegionSetV1, MtgoRectPxV1,
        MtgoVisibleCompetitiveLifecycleSnapshotV1, MtgoVisibleObjectActionCalibrationTraceV1,
        MTGO_AUTHORIZATION_SCHEMA_V1, MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
        MTGO_PROFILE_BOUND_POSTCONDITION_REGION_SET_SCHEMA_V1,
    };

    fn digest_v1(value: char) -> String {
        value.to_string().repeat(64)
    }

    fn plan_v1() -> CheckedUntrustedMtgoProfileBoundActionPostconditionPlanV1 {
        let resolution = profile_bound_resolved_action_control_for_test_v1();
        let region_set = MtgoProfileBoundPostconditionRegionSetV1 {
            schema_version: MTGO_PROFILE_BOUND_POSTCONDITION_REGION_SET_SCHEMA_V1,
            profile_bound_resolution_commitment_sha256: resolution
                .profile_bound_resolution_commitment_sha256()
                .to_owned(),
            decision_commitment_sha256: resolution.decision_commitment_sha256_v1().to_owned(),
            frame_id: resolution.frame_id(),
            frame_sequence: resolution.frame_sequence(),
            candidate_set_complete: true,
            regions: vec![
                region_v1(MtgoDuelVisiblePostconditionKindV1::PromptChanged, 60),
                region_v1(MtgoDuelVisiblePostconditionKindV1::PlayerCountsChanged, 70),
                region_v1(MtgoDuelVisiblePostconditionKindV1::BattlefieldChanged, 80),
                region_v1(MtgoDuelVisiblePostconditionKindV1::HandChanged, 90),
                region_v1(
                    MtgoDuelVisiblePostconditionKindV1::VisibleGameLogChanged,
                    100,
                ),
            ],
        };
        let record: MtgoVisibleObjectActionCalibrationTraceV1 = serde_json::from_str(include_str!(
            "../fixtures/solitaire_play_land_transition_v1.json"
        ))
        .unwrap();
        let calibration: MtgoProfileBoundPostconditionCalibrationV1 =
            validate_visible_object_action_calibration_trace_v1(record)
                .unwrap()
                .into();
        prepare_profile_bound_action_postcondition_plan_v1(resolution, calibration, region_set)
            .unwrap()
    }

    fn region_v1(
        kind: MtgoDuelVisiblePostconditionKindV1,
        frame_region_evidence_id: u64,
    ) -> MtgoProfileBoundPostconditionRegionCandidateV1 {
        MtgoProfileBoundPostconditionRegionCandidateV1 {
            kind,
            frame_region_evidence_id,
        }
    }

    fn lifecycle_v1(
        plan: &CheckedUntrustedMtgoProfileBoundActionPostconditionPlanV1,
        sequence_delta: u64,
        event_kind: MtgoCompetitiveEventKindV1,
    ) -> CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1 {
        let size = plan.source_client_size_px_v1();
        let facts = [
            MtgoLifecycleVisibleFactKindV1::MatchSurfaceVisible,
            MtgoLifecycleVisibleFactKindV1::LocalClockVisible,
            MtgoLifecycleVisibleFactKindV1::OpponentClockVisible,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, kind)| MtgoLifecycleVisibleFactV1 {
            kind,
            rect_client_px: MtgoRectPxV1 {
                x: 10 + index as u32 * 30,
                y: 10,
                width: 20,
                height: 20,
            },
            content_sha256: digest_v1('5'),
            confidence_bps: 10_000,
        })
        .collect();
        validate_visible_competitive_lifecycle_snapshot_v1(
            MtgoVisibleCompetitiveLifecycleSnapshotV1 {
                schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
                snapshot_id: "league-match-game-1-source".to_owned(),
                event_kind,
                phase: MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
                frame_id: plan.source_frame_id_v1(),
                frame_sequence: plan.source_frame_sequence_v1() + sequence_delta,
                frame_sha256: plan.source_frame_sha256_v1().to_owned(),
                client_bounds: MtgoRectPxV1 {
                    x: 0,
                    y: 0,
                    width: size.width,
                    height: size.height,
                },
                event_identity_sha256: Some(digest_v1('6')),
                match_identity_sha256: Some(digest_v1('7')),
                game_number: Some(1),
                entry_terms: None,
                visible_state_complete: true,
                facts,
            },
        )
        .unwrap()
    }

    fn mode_v1() -> MtgoAuthorizationScopeV1 {
        MtgoAuthorizationScopeV1 {
            schema_version: MTGO_AUTHORIZATION_SCHEMA_V1,
            account_alias_sha256: digest_v1('1'),
            written_permission_sha256: digest_v1('2'),
            visible_channels_only: true,
            league_input: true,
            challenge_input: true,
            ..MtgoAuthorizationScopeV1::default()
        }
    }

    fn authorization_v1(
        frame_sequence: u64,
        event_kind: MtgoCompetitiveEventKindV1,
    ) -> MtgoCompetitiveMatchGameplayAuthorizationV1 {
        MtgoCompetitiveMatchGameplayAuthorizationV1 {
            schema_version: MTGO_COMPETITIVE_MATCH_GAMEPLAY_AUTHORIZATION_SCHEMA_V1,
            account_alias_sha256: digest_v1('1'),
            written_permission_sha256: digest_v1('2'),
            event_kind,
            event_identity_sha256: digest_v1('6'),
            match_identity_sha256: digest_v1('7'),
            game_number: 1,
            entry_authorization_sha256: digest_v1('8'),
            owner_launch_authorization_sha256: digest_v1('9'),
            exact_match_gameplay_authorized: true,
            valid_through_frame_sequence: frame_sequence + 100,
        }
    }

    #[cfg(test)]
    pub(crate) fn competitive_gameplay_postcondition_for_match_memory_test_v1(
    ) -> CheckedUntrustedMtgoCompetitiveGameplayPostconditionV1 {
        let plan = plan_v1();
        let sequence = plan.source_frame_sequence_v1();
        let lifecycle = lifecycle_v1(&plan, 0, MtgoCompetitiveEventKindV1::League);
        let competitive = bind_profile_bound_action_plan_to_competitive_match_v1(
            plan,
            lifecycle,
            &mode_v1(),
            &authorization_v1(sequence, MtgoCompetitiveEventKindV1::League),
        )
        .unwrap();
        let plan_commitments = competitive.plan.commitments_v1();
        let size = plan_commitments.source_client_size_px.clone();
        let metadata = MtgoProfileBoundPostconditionAfterFrameMetadataV1 {
            schema_version: crate::MTGO_PROFILE_BOUND_POSTCONDITION_AFTER_FRAME_SCHEMA_V1,
            plan_commitment_sha256: competitive.plan.plan_commitment_sha256().to_owned(),
            frame_id: competitive.plan.source_frame_id_v1() + 1,
            frame_sequence: competitive.plan.source_frame_sequence_v1() + 1,
            manifest_sha256: "a".repeat(64),
            output_identity_sha256: plan_commitments.source_output_identity_sha256,
            perception_profile_admission_commitment_sha256: plan_commitments
                .perception_profile_admission_commitment_sha256,
            client_size_px: size.clone(),
            capture_role: crate::MtgoDxgiCaptureRoleV2::ActingPlayerDuel,
        };
        let pixels =
            vec![
                31_u8;
                usize::try_from(size.width).unwrap() * usize::try_from(size.height).unwrap() * 4
            ];
        check_untrusted_competitive_gameplay_postcondition_pixels_v1(competitive, metadata, &pixels)
            .unwrap()
    }

    #[cfg(test)]
    pub(crate) fn later_competitive_gameplay_postcondition_for_match_memory_test_v1(
    ) -> CheckedUntrustedMtgoCompetitiveGameplayPostconditionV1 {
        let mut confirmed = competitive_gameplay_postcondition_for_match_memory_test_v1();
        confirmed.source_frame_id += 2;
        confirmed.source_frame_sequence += 2;
        confirmed.source_frame_sha256 = "b".repeat(64);
        confirmed.after_frame_id += 2;
        confirmed.after_frame_sequence += 2;
        confirmed.decision_commitment_sha256 = "c".repeat(64);
        confirmed.selection_commitment_sha256 = "d".repeat(64);
        confirmed.confirmation_commitment_sha256 = "e".repeat(64);
        confirmed
    }

    #[test]
    fn exact_visible_match_account_permission_and_owner_launch_bind_without_input() {
        for event_kind in [
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ] {
            let plan = plan_v1();
            let lifecycle = lifecycle_v1(&plan, 0, event_kind);
            let authorization = authorization_v1(plan.source_frame_sequence_v1(), event_kind);
            let scoped = bind_profile_bound_action_plan_to_competitive_match_v1(
                plan,
                lifecycle,
                &mode_v1(),
                &authorization,
            )
            .unwrap();
            assert_eq!(scoped.event_kind(), event_kind);
            assert_eq!(scoped.game_number(), 1);
            assert!(matches!(
                scoped
                    .plan
                    .player_visible_confirmed_decision_v1()
                    .unwrap()
                    .selected_action,
                crate::MtgoPlayerVisibleDuelActionV1::PlayLand { .. }
            ));
            assert_eq!(scoped.authorization_commitment_sha256().len(), 64);
            assert_eq!(scoped.mode_authorization_commitment_sha256().len(), 64);
            assert_eq!(scoped.competitive_scope_commitment_sha256().len(), 64);
            assert!(!scoped.safe_for_live_input());
            assert!(!scoped.permits_event_entry());
        }
    }

    #[test]
    fn source_frame_and_exact_match_authorization_substitutions_reject() {
        let plan = plan_v1();
        let sequence = plan.source_frame_sequence_v1();
        let snapshot = lifecycle_v1(&plan, 1, MtgoCompetitiveEventKindV1::League);
        assert_eq!(
            bind_profile_bound_action_plan_to_competitive_match_v1(
                plan,
                snapshot,
                &mode_v1(),
                &authorization_v1(sequence, MtgoCompetitiveEventKindV1::League),
            )
            .err()
            .unwrap()
            .code(),
            "competitive_gameplay_source_frame_mismatch"
        );

        for mutation in 0..6 {
            let plan = plan_v1();
            let lifecycle = lifecycle_v1(&plan, 0, MtgoCompetitiveEventKindV1::League);
            let mut authorization = authorization_v1(
                plan.source_frame_sequence_v1(),
                MtgoCompetitiveEventKindV1::League,
            );
            match mutation {
                0 => authorization.account_alias_sha256 = digest_v1('a'),
                1 => authorization.event_identity_sha256 = digest_v1('a'),
                2 => authorization.match_identity_sha256 = digest_v1('a'),
                3 => authorization.game_number = 2,
                4 => authorization.exact_match_gameplay_authorized = false,
                5 => authorization.valid_through_frame_sequence = 0,
                _ => unreachable!(),
            }
            let code = bind_profile_bound_action_plan_to_competitive_match_v1(
                plan,
                lifecycle,
                &mode_v1(),
                &authorization,
            )
            .err()
            .unwrap()
            .code();
            if mutation >= 4 {
                assert_eq!(code, "competitive_gameplay_authorization_inactive");
            } else {
                assert_eq!(code, "competitive_gameplay_authorization_binding");
            }
        }
    }

    #[test]
    fn league_and_challenge_scope_remain_distinct() {
        let plan = plan_v1();
        let lifecycle = lifecycle_v1(&plan, 0, MtgoCompetitiveEventKindV1::League);
        let authorization = authorization_v1(
            plan.source_frame_sequence_v1(),
            MtgoCompetitiveEventKindV1::League,
        );
        let mut challenge_only = mode_v1();
        challenge_only.league_input = false;
        assert_eq!(
            bind_profile_bound_action_plan_to_competitive_match_v1(
                plan,
                lifecycle,
                &challenge_only,
                &authorization,
            )
            .err()
            .unwrap()
            .code(),
            "mode_not_authorized"
        );

        let both = mode_v1();
        let league =
            competitive_mode_authorization_commitment_v1(&both, MtgoCompetitiveEventKindV1::League)
                .unwrap();
        let challenge = competitive_mode_authorization_commitment_v1(
            &both,
            MtgoCompetitiveEventKindV1::Challenge,
        )
        .unwrap();
        assert_ne!(league, challenge);

        let mut league_only = both;
        league_only.challenge_input = false;
        assert_ne!(
            league,
            competitive_mode_authorization_commitment_v1(
                &league_only,
                MtgoCompetitiveEventKindV1::League,
            )
            .unwrap()
        );
    }

    #[test]
    fn broad_written_permission_cannot_stand_in_for_entry_or_owner_launch() {
        for mutation in 0..2 {
            let plan = plan_v1();
            let lifecycle = lifecycle_v1(&plan, 0, MtgoCompetitiveEventKindV1::League);
            let mut authorization = authorization_v1(
                plan.source_frame_sequence_v1(),
                MtgoCompetitiveEventKindV1::League,
            );
            if mutation == 0 {
                authorization.entry_authorization_sha256 =
                    authorization.written_permission_sha256.clone();
            } else {
                authorization.owner_launch_authorization_sha256 =
                    authorization.written_permission_sha256.clone();
            }
            assert_eq!(
                bind_profile_bound_action_plan_to_competitive_match_v1(
                    plan,
                    lifecycle,
                    &mode_v1(),
                    &authorization,
                )
                .err()
                .unwrap()
                .code(),
                "competitive_gameplay_authorization_record_reuse"
            );
        }
    }

    #[test]
    fn competitive_postcondition_recomputes_private_regions_from_exact_pixels() {
        let confirmed = competitive_gameplay_postcondition_for_match_memory_test_v1();
        assert_eq!(confirmed.event_kind(), MtgoCompetitiveEventKindV1::League);
        assert_eq!(confirmed.game_number(), 1);
        assert_eq!(confirmed.after_frame_id(), 2);
        assert_eq!(confirmed.after_frame_sequence(), 2);
        assert_eq!(confirmed.confirmation_commitment_sha256().len(), 64);
        assert!(!confirmed.safe_for_additional_input());
        assert!(!confirmed.permits_event_entry());
    }
}
