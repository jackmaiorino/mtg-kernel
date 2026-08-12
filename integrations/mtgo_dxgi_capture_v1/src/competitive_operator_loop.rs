use crate::actuator::{
    advance_competitive_event_monitor_in_runtime_v1, advance_competitive_event_runtime_observed_v1,
    attach_competitive_event_monitor_to_runtime_v1,
    bind_competitive_duel_gesture_sequence_session_v1,
    bind_competitive_event_native_sideboard_request_v1,
    bind_competitive_event_pregame_native_request_v1,
    checkout_competitive_event_gameplay_session_v1,
    competitive_gesture_game_session_action_authorities_v1,
    confirm_pending_competitive_event_lifecycle_control_v1,
    execute_prepared_competitive_event_lifecycle_control_v1,
    measure_competitive_event_runtime_sideboard_v1, next_competitive_event_driver_directive_v1,
    prepare_competitive_event_runtime_lifecycle_control_v1,
    return_competitive_event_gameplay_session_v1, MtgoCompetitiveEventDriverDirectiveV1,
    MtgoCompetitiveEventDriverStepV1, MtgoCompetitiveEventGameplayLeaseCommitmentsV1,
    MtgoCompetitiveEventRuntimeCommitmentsV1, MtgoCompetitiveGestureGameSessionCommitmentsV1,
    MtgoPendingCompetitiveEventLifecycleControlCommitmentsV1,
    MtgoPreparedCompetitiveEventLifecycleControlCommitmentsV1,
    OpaqueMtgoCompetitiveEventGameplayLeaseV1, OpaqueMtgoCompetitiveEventRuntimeV1,
    OpaqueMtgoCompetitiveGestureGameSessionV1, OpaqueMtgoCompetitiveNativePregameRequestV1,
    OpaqueMtgoCompetitiveNativeSideboardRequestV1,
    OpaqueMtgoPendingCompetitiveEventLifecycleControlV1,
    OpaqueMtgoPreparedCompetitiveEventLifecycleControlV1,
    OpaqueMtgoSessionBoundCompetitiveDuelGestureV1, RatifiedMtgoCompetitiveMatchLaunchV1,
};
use crate::competitive_auxiliary_model_scoring::{
    score_checked_untrusted_competitive_native_pregame_request_v1,
    score_checked_untrusted_competitive_native_sideboard_request_v1,
    MtgoCompetitiveNativePregameScorerV1, MtgoCompetitiveNativeSideboardScorerV1,
    OpaqueMtgoScoredCompetitiveNativePregameRequestV1,
    OpaqueMtgoScoredCompetitiveNativeSideboardRequestV1,
};
use crate::competitive_model_decision_readiness::check_competitive_model_decision_readiness_v1;
use crate::competitive_operator_bootstrap::{
    MtgoCompetitiveOperatorResourceCommitmentsV1,
    MtgoCompetitiveOperatorResourcesDuringSideboardV1, MtgoCompetitiveOperatorResourcesPartsV1,
    OpaqueMtgoCompetitiveOperatorResourcesV1,
};
use crate::competitive_visible_match_memory::OpaqueMtgoCompetitiveVisibleGameOutcomeV1;
use crate::probe::{
    begin_evaluated_competitive_event_monitor_v1,
    begin_opaque_competitive_duel_gesture_sequence_from_pinned_runtime_v1,
    prepare_opaque_competitive_duel_action_plan_v1, resolve_opaque_profile_bound_duel_control_v1,
    score_and_select_opaque_admitted_duel_perception_with_loaded_deployment_v1,
    OpaqueMtgoAdmittedDuelPerceptionV1, OpaqueMtgoClassifiedCompetitiveEventRecordV1,
    OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    OpaqueMtgoClassifiedCompetitivePregameModelContextV1,
    OpaqueMtgoProfileBoundDuelResolvedControlV1,
};
use mtgo_blackbox_v1::{
    validate_native_checkpoint_competitive_capabilities_v1, ActionSemanticV1,
    MtgoCompetitiveEventKindV1, MtgoCompetitiveLifecycleActionV1, MtgoCompetitiveLifecyclePhaseV1,
    MtgoDuelGestureStageV1, MtgoNativeCheckpointCompetitiveCapabilitiesV1,
    MtgoObservedCompetitiveLifecycleAdvanceV1, MtgoProfileBoundPostconditionCalibrationV1,
    MtgoProfileBoundPostconditionRegionSetV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const COMPETITIVE_POST_ENTRY_OPERATOR_DOMAIN_V1: &[u8] = b"mtgo-competitive-post-entry-operator-v1";
const COMPETITIVE_POST_ENTRY_OPERATOR_ADVANCE_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-post-entry-operator-advance-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitivePostEntryOperatorCommitmentsV1 {
    pub resource_bundle_commitment_sha256: String,
    pub event_runtime_commitment_sha256: String,
    pub navigation_profile_commitment_sha256: String,
    pub navigation_profile_admission_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub checkpoint_competitive_capabilities_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub current_phase: MtgoCompetitiveLifecyclePhaseV1,
    pub current_frame_sequence: u64,
    pub accepted_transition_count: u64,
    pub prior_operator_commitment_sha256: Option<String>,
    pub operator_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitivePostEntryOperatorRouteV1 {
    ObserveLifecycle {
        allowed_advances: Vec<MtgoObservedCompetitiveLifecycleAdvanceV1>,
    },
    LifecycleControl {
        action: MtgoCompetitiveLifecycleActionV1,
    },
    ResolvePregameWithNativeModel {
        match_identity_sha256: String,
        game_number: u8,
        native_model_path_present: bool,
    },
    LaunchGameplay {
        match_identity_sha256: String,
        game_number: u8,
        native_model_path_present: bool,
    },
    ResolveSideboardWithNativeModel {
        match_identity_sha256: String,
        game_number: u8,
        native_model_path_present: bool,
        changed_sideboard_resources_present: bool,
    },
    BeginTerminalEventRecordMonitor,
    AdvanceTerminalEventRecordMonitor {
        prior_observation_count: u64,
    },
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitivePostEntryOperatorDirectiveV1 {
    pub source_operator_commitment_sha256: String,
    pub source_event_runtime_commitment_sha256: String,
    pub resource_bundle_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub current_phase: MtgoCompetitiveLifecyclePhaseV1,
    pub current_frame_sequence: u64,
    pub route: MtgoCompetitivePostEntryOperatorRouteV1,
    pub safe_for_live_input: bool,
    pub permits_event_entry: bool,
    pub permits_spending: bool,
}

/// One move-only post-entry coordinator that retains the exact semantic and
/// runtime resources together with the already-entered event runtime.
///
/// It begins only after the existing exact entry confirmation. It cannot
/// create another entry, authorize spending, manufacture a model decision, or
/// expose pixels, coordinates, process handles, or input primitives.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitivePostEntryOperatorV1;
/// let _forged = OpaqueMtgoCompetitivePostEntryOperatorV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitivePostEntryOperatorV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitivePostEntryOperatorV1>();
/// ```
pub struct OpaqueMtgoCompetitivePostEntryOperatorV1 {
    resources: MtgoCompetitiveOperatorResourcesPartsV1,
    resource_commitments: MtgoCompetitiveOperatorResourceCommitmentsV1,
    runtime: OpaqueMtgoCompetitiveEventRuntimeV1,
    commitments: MtgoCompetitivePostEntryOperatorCommitmentsV1,
}

impl OpaqueMtgoCompetitivePostEntryOperatorV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitivePostEntryOperatorCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn navigation_profile_v1(
        &self,
    ) -> &mtgo_blackbox_v1::AdmittedMtgoCompetitiveNavigationProfileV1 {
        &self.resources.navigation_profile
    }

    pub fn navigation_runtime_v1(
        &self,
    ) -> &crate::OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1 {
        &self.resources.navigation_runtime
    }

    pub fn deck_manifest_v1(&self) -> &mtgo_blackbox_v1::ValidatedMtgoCompetitiveDeckManifestV1 {
        &self.resources.deck_manifest
    }

    pub fn duel_perception_profile_v1(
        &self,
    ) -> &mtgo_blackbox_v1::AdmittedMtgoDuelPerceptionProfileV1 {
        &self.resources.duel_perception_profile
    }

    pub fn duel_perception_runtime_v1(&self) -> &crate::OpaqueMtgoVerifiedDuelPerceptionRuntimeV1 {
        &self.resources.duel_perception_runtime
    }

    pub fn duel_gesture_profile_v1(&self) -> &mtgo_blackbox_v1::AdmittedMtgoDuelGestureProfileV1 {
        &self.resources.duel_gesture_profile
    }

    pub fn duel_gesture_runtime_v1(&self) -> &crate::OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1 {
        &self.resources.duel_gesture_runtime
    }

    pub fn checkpoint_deployment_v1(
        &self,
    ) -> &mtgo_blackbox_v1::LoadedMtgoNativeCheckpointDeploymentV1 {
        &self.resources.checkpoint_deployment
    }

    pub fn changed_sideboard_resources_present_v1(&self) -> bool {
        self.resources.changed_sideboard_evaluation.is_some()
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

pub struct OpaqueMtgoPreparedCompetitiveOperatorLifecycleV1 {
    resources: MtgoCompetitiveOperatorResourcesPartsV1,
    resource_commitments: MtgoCompetitiveOperatorResourceCommitmentsV1,
    prepared: OpaqueMtgoPreparedCompetitiveEventLifecycleControlV1,
    prior_operator: MtgoCompetitivePostEntryOperatorCommitmentsV1,
}

pub struct OpaqueMtgoPendingCompetitiveOperatorLifecycleV1 {
    resources: MtgoCompetitiveOperatorResourcesPartsV1,
    resource_commitments: MtgoCompetitiveOperatorResourceCommitmentsV1,
    pending: OpaqueMtgoPendingCompetitiveEventLifecycleControlV1,
    prior_operator: MtgoCompetitivePostEntryOperatorCommitmentsV1,
}

pub struct OpaqueMtgoCompetitiveOperatorGameplayLeaseV1 {
    resources: MtgoCompetitiveOperatorResourcesPartsV1,
    resource_commitments: MtgoCompetitiveOperatorResourceCommitmentsV1,
    lease: OpaqueMtgoCompetitiveEventGameplayLeaseV1,
    prior_operator: MtgoCompetitivePostEntryOperatorCommitmentsV1,
}

/// Move-only ownership of the gameplay lease and attended game session after
/// the exact loaded checkpoint selected one current player-visible legal
/// action. Only that semantic is exposed so the reviewed UI adapter can build
/// its coordinate-free gesture stages.
pub struct OpaqueMtgoCompetitiveOperatorGameplaySelectionV1 {
    lease: OpaqueMtgoCompetitiveOperatorGameplayLeaseV1,
    session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    control: OpaqueMtgoProfileBoundDuelResolvedControlV1,
}

impl OpaqueMtgoCompetitiveOperatorGameplaySelectionV1 {
    pub fn selected_semantic_v1(&self) -> &ActionSemanticV1 {
        self.control.selected_semantic_for_operator_v1()
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

/// Move-only ownership of the complete post-entry operator while one exact
/// player-visible pregame request is outside the operator for native scoring.
/// The paid-event runtime remains inside `request` and cannot be recovered
/// through this type.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorNativePregameRequestV1;
/// fn cannot_act(value: OpaqueMtgoCompetitiveOperatorNativePregameRequestV1) {
///     let _ = value.into_event_runtime();
///     let _ = value.input_command();
/// }
/// ```
pub struct OpaqueMtgoCompetitiveOperatorNativePregameRequestV1 {
    resources: MtgoCompetitiveOperatorResourcesPartsV1,
    resource_commitments: MtgoCompetitiveOperatorResourceCommitmentsV1,
    request: OpaqueMtgoCompetitiveNativePregameRequestV1,
    prior_operator: MtgoCompetitivePostEntryOperatorCommitmentsV1,
}

impl OpaqueMtgoCompetitiveOperatorNativePregameRequestV1 {
    pub fn model_input_v1(&self) -> &crate::MtgoCompetitiveNativePregameModelInputV1 {
        self.request.model_input_v1()
    }

    pub fn model_input_commitment_sha256_v1(&self) -> &str {
        self.request.model_input_commitment_sha256_v1()
    }

    pub fn deployment_commitment_sha256_v1(&self) -> &str {
        &self.prior_operator.policy_deployment_commitment_sha256
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_session_recovery_v1(&self) -> bool {
        false
    }
}

/// Offline checked scorer result that still owns every resource and the exact
/// source request. It deliberately has no path back to the event operator. A
/// future return seam must accept a kernel-owned opaque response, not this
/// checked-untrusted scorer result.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoScoredCompetitiveOperatorNativePregameV1;
/// fn cannot_resume(value: OpaqueMtgoScoredCompetitiveOperatorNativePregameV1) {
///     let _ = value.into_operator();
///     let _ = value.input_command();
/// }
/// ```
pub struct OpaqueMtgoScoredCompetitiveOperatorNativePregameV1 {
    _resources: MtgoCompetitiveOperatorResourcesPartsV1,
    _resource_commitments: MtgoCompetitiveOperatorResourceCommitmentsV1,
    scored_request: OpaqueMtgoScoredCompetitiveNativePregameRequestV1,
    _prior_operator: MtgoCompetitivePostEntryOperatorCommitmentsV1,
}

/// Move-only ownership of every post-entry resource while the non-cloneable
/// deck manifest and event runtime are held inside one exact visible-only
/// sideboard request.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorNativeSideboardRequestV1;
/// fn cannot_act(value: OpaqueMtgoCompetitiveOperatorNativeSideboardRequestV1) {
///     let _ = value.into_event_runtime();
///     let _ = value.submit_sideboard();
/// }
/// ```
pub struct OpaqueMtgoCompetitiveOperatorNativeSideboardRequestV1 {
    resources: MtgoCompetitiveOperatorResourcesDuringSideboardV1,
    resource_commitments: MtgoCompetitiveOperatorResourceCommitmentsV1,
    request: OpaqueMtgoCompetitiveNativeSideboardRequestV1,
    prior_operator: MtgoCompetitivePostEntryOperatorCommitmentsV1,
}

impl OpaqueMtgoCompetitiveOperatorNativeSideboardRequestV1 {
    pub fn model_input_v1(&self) -> &crate::MtgoCompetitiveNativeSideboardModelInputV1 {
        self.request.model_input_v1()
    }

    pub fn model_input_commitment_sha256_v1(&self) -> &str {
        self.request.model_input_commitment_sha256_v1()
    }

    pub fn deployment_commitment_sha256_v1(&self) -> &str {
        &self.prior_operator.policy_deployment_commitment_sha256
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_session_recovery_v1(&self) -> bool {
        false
    }

    pub fn permits_sideboard_submission_v1(&self) -> bool {
        false
    }
}

/// Offline checked sideboard result retaining the exact request and every
/// other operator resource. No event, drag, submission, or operator recovery
/// path is exposed.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoScoredCompetitiveOperatorNativeSideboardV1;
/// fn cannot_resume(value: OpaqueMtgoScoredCompetitiveOperatorNativeSideboardV1) {
///     let _ = value.into_operator();
///     let _ = value.submit_sideboard();
/// }
/// ```
pub struct OpaqueMtgoScoredCompetitiveOperatorNativeSideboardV1 {
    _resources: MtgoCompetitiveOperatorResourcesDuringSideboardV1,
    _resource_commitments: MtgoCompetitiveOperatorResourceCommitmentsV1,
    scored_request: OpaqueMtgoScoredCompetitiveNativeSideboardRequestV1,
    _prior_operator: MtgoCompetitivePostEntryOperatorCommitmentsV1,
}

impl OpaqueMtgoScoredCompetitiveOperatorNativeSideboardV1 {
    pub fn selection_v1(&self) -> &crate::MtgoCompetitiveNativeSideboardModelSelectionV1 {
        self.scored_request.selection_v1()
    }

    pub fn checked_selection_commitment_sha256_v1(&self) -> &str {
        self.scored_request.checked_selection_commitment_sha256_v1()
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_session_recovery_v1(&self) -> bool {
        false
    }

    pub fn permits_sideboard_submission_v1(&self) -> bool {
        false
    }
}

impl OpaqueMtgoScoredCompetitiveOperatorNativePregameV1 {
    pub fn selected_index_v1(&self) -> usize {
        self.scored_request.selected_index_v1()
    }

    pub fn selected_action_v1(&self) -> &crate::MtgoCompetitiveNativePregameActionV1 {
        self.scored_request.selected_action_v1()
    }

    pub fn selection_commitment_sha256_v1(&self) -> &str {
        self.scored_request.selection_commitment_sha256_v1()
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_session_recovery_v1(&self) -> bool {
        false
    }
}

impl OpaqueMtgoCompetitiveOperatorGameplayLeaseV1 {
    pub fn duel_perception_profile_v1(
        &self,
    ) -> &mtgo_blackbox_v1::AdmittedMtgoDuelPerceptionProfileV1 {
        &self.resources.duel_perception_profile
    }

    pub fn duel_perception_runtime_v1(&self) -> &crate::OpaqueMtgoVerifiedDuelPerceptionRuntimeV1 {
        &self.resources.duel_perception_runtime
    }

    pub fn duel_gesture_profile_v1(&self) -> &mtgo_blackbox_v1::AdmittedMtgoDuelGestureProfileV1 {
        &self.resources.duel_gesture_profile
    }

    pub fn duel_gesture_runtime_v1(&self) -> &crate::OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1 {
        &self.resources.duel_gesture_runtime
    }

    pub fn checkpoint_deployment_v1(
        &self,
    ) -> &mtgo_blackbox_v1::LoadedMtgoNativeCheckpointDeploymentV1 {
        &self.resources.checkpoint_deployment
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

pub fn begin_competitive_post_entry_operator_v1(
    resources: OpaqueMtgoCompetitiveOperatorResourcesV1,
    runtime: OpaqueMtgoCompetitiveEventRuntimeV1,
) -> Result<OpaqueMtgoCompetitivePostEntryOperatorV1, String> {
    let resource_commitments = resources.commitments_v1();
    let runtime_commitments = runtime.commitments_v1();
    let commitments = post_entry_operator_commitments_v1(
        &resource_commitments,
        &runtime_commitments,
        0,
        None,
        COMPETITIVE_POST_ENTRY_OPERATOR_DOMAIN_V1,
    )?;
    Ok(OpaqueMtgoCompetitivePostEntryOperatorV1 {
        resources: resources.into_parts_v1(),
        resource_commitments,
        runtime,
        commitments,
    })
}

pub fn next_competitive_post_entry_operator_directive_v1(
    operator: &OpaqueMtgoCompetitivePostEntryOperatorV1,
) -> Result<MtgoCompetitivePostEntryOperatorDirectiveV1, String> {
    let driver = next_competitive_event_driver_directive_v1(&operator.runtime)?;
    directive_from_driver_v1(
        &operator.commitments,
        operator
            .resources
            .checkpoint_deployment
            .competitive_capabilities_v1(),
        operator.resources.changed_sideboard_evaluation.is_some(),
        &driver,
    )
}

/// Checks out the exact post-entry runtime into one player-visible native
/// pregame request while retaining every operator resource. This performs no
/// scoring or input. It is valid even while the static readiness report says
/// the native pregame head is missing, so an offline scorer can exercise the
/// complete ownership path without falsely advertising a live model path.
pub fn checkout_competitive_post_entry_operator_native_pregame_v1(
    operator: OpaqueMtgoCompetitivePostEntryOperatorV1,
    match_launch: RatifiedMtgoCompetitiveMatchLaunchV1,
    context: OpaqueMtgoClassifiedCompetitivePregameModelContextV1,
) -> Result<OpaqueMtgoCompetitiveOperatorNativePregameRequestV1, String> {
    let directive = next_competitive_post_entry_operator_directive_v1(&operator)?;
    let (match_identity_sha256, game_number) = match directive.route {
        MtgoCompetitivePostEntryOperatorRouteV1::ResolvePregameWithNativeModel {
            match_identity_sha256,
            game_number,
            ..
        } => (match_identity_sha256, game_number),
        _ => {
            return Err(
                "competitive post-entry operator is not at a native pregame request".to_owned(),
            )
        }
    };
    let launch = match_launch.gameplay_authorization_record_v2();
    validate_operator_native_pregame_checkout_v1(&OperatorNativePregameCheckoutIdentityV1 {
        route_match_identity_sha256: match_identity_sha256,
        route_game_number: game_number,
        launch_match_identity_sha256: launch.match_identity_sha256.clone(),
        launch_game_number: launch.game_number,
        launch_event_kind: launch.event_kind,
        operator_event_kind: operator.commitments.event_kind,
        resource_bundle_commitment_sha256: operator
            .resource_commitments
            .resource_bundle_commitment_sha256
            .clone(),
        operator_resource_bundle_commitment_sha256: operator
            .commitments
            .resource_bundle_commitment_sha256
            .clone(),
    })?;
    let OpaqueMtgoCompetitivePostEntryOperatorV1 {
        resources,
        resource_commitments,
        runtime,
        commitments,
    } = operator;
    let request = bind_competitive_event_pregame_native_request_v1(
        runtime,
        match_launch,
        context,
        &resources.deck_manifest,
    )?;
    require_sha256_v1(
        request.model_input_commitment_sha256_v1(),
        "competitive operator pregame request",
    )?;
    Ok(OpaqueMtgoCompetitiveOperatorNativePregameRequestV1 {
        resources,
        resource_commitments,
        request,
        prior_operator: commitments,
    })
}

/// Runs the full post-entry ownership path through a checked-untrusted offline
/// scorer. All live resources remain consumed and inaccessible afterward.
pub fn score_checked_untrusted_competitive_operator_native_pregame_v1<
    S: MtgoCompetitiveNativePregameScorerV1,
>(
    value: OpaqueMtgoCompetitiveOperatorNativePregameRequestV1,
    scorer: &mut S,
) -> Result<OpaqueMtgoScoredCompetitiveOperatorNativePregameV1, String> {
    validate_operator_native_pregame_scoring_v1(&OperatorNativePregameScoringIdentityV1 {
        resource_bundle_commitment_sha256: value
            .resource_commitments
            .resource_bundle_commitment_sha256
            .clone(),
        prior_resource_bundle_commitment_sha256: value
            .prior_operator
            .resource_bundle_commitment_sha256
            .clone(),
        request_deployment_commitment_sha256: value.deployment_commitment_sha256_v1().to_owned(),
        loaded_checkpoint_deployment_commitment_sha256: value
            .resources
            .checkpoint_deployment
            .deployment_commitment_sha256()
            .to_owned(),
    })?;
    let deployment_commitment_sha256 = value.deployment_commitment_sha256_v1().to_owned();
    let OpaqueMtgoCompetitiveOperatorNativePregameRequestV1 {
        resources,
        resource_commitments,
        request,
        prior_operator,
    } = value;
    let scored_request = score_checked_untrusted_competitive_native_pregame_request_v1(
        request,
        &deployment_commitment_sha256,
        scorer,
    )?;
    Ok(OpaqueMtgoScoredCompetitiveOperatorNativePregameV1 {
        _resources: resources,
        _resource_commitments: resource_commitments,
        scored_request,
        _prior_operator: prior_operator,
    })
}

/// Consumes the exact Sideboarding operator state into one player-visible
/// native sideboard request. The manifest moves into the request because it is
/// intentionally non-cloneable; all other original resources remain in the
/// returned opaque holder. The admitted four-slice parser evaluation is
/// consumed here, but changed-sideboard drag authorization is deliberately not
/// required before the model chooses a target. This parses retained visible
/// pixels but performs no capture, scoring, drag, submission, or other input.
pub fn checkout_competitive_post_entry_operator_native_sideboard_v1(
    operator: OpaqueMtgoCompetitivePostEntryOperatorV1,
    outcome: OpaqueMtgoCompetitiveVisibleGameOutcomeV1,
    classifier_timeout_ms: u32,
) -> Result<OpaqueMtgoCompetitiveOperatorNativeSideboardRequestV1, String> {
    let directive = next_competitive_post_entry_operator_directive_v1(&operator)?;
    let (route_match_identity_sha256, route_game_number, changed_resources_present) =
        match directive.route {
            MtgoCompetitivePostEntryOperatorRouteV1::ResolveSideboardWithNativeModel {
                match_identity_sha256,
                game_number,
                changed_sideboard_resources_present,
                ..
            } => (
                match_identity_sha256,
                game_number,
                changed_sideboard_resources_present,
            ),
            _ => {
                return Err(
                    "competitive post-entry operator is not at a native sideboard request"
                        .to_owned(),
                )
            }
        };
    let outcome_lineage = outcome.lineage_v1();
    validate_operator_native_sideboard_checkout_v1(&OperatorNativeSideboardCheckoutIdentityV1 {
        route_match_identity_sha256,
        route_game_number,
        route_changed_resources_present: changed_resources_present,
        outcome_match_identity_sha256: outcome_lineage.match_identity_sha256.to_owned(),
        outcome_game_number: outcome_lineage.game_number,
        outcome_event_kind: outcome_lineage.event_kind,
        operator_event_kind: operator.commitments.event_kind,
        resource_bundle_commitment_sha256: operator
            .resource_commitments
            .resource_bundle_commitment_sha256
            .clone(),
        operator_resource_bundle_commitment_sha256: operator
            .commitments
            .resource_bundle_commitment_sha256
            .clone(),
        resource_sideboard_evaluation_ratification_commitment_sha256: operator
            .resource_commitments
            .changed_sideboard_evaluation_ratification_commitment_sha256
            .clone(),
        resource_sideboard_evaluation_admission_commitment_sha256: operator
            .resource_commitments
            .changed_sideboard_evaluation_admission_commitment_sha256
            .clone(),
        operator_deck_manifest_commitment_sha256: operator
            .commitments
            .deck_manifest_commitment_sha256
            .clone(),
        operator_policy_deployment_commitment_sha256: operator
            .commitments
            .policy_deployment_commitment_sha256
            .clone(),
    })?;
    let OpaqueMtgoCompetitivePostEntryOperatorV1 {
        resources,
        resource_commitments,
        runtime,
        commitments,
    } = operator;
    let (deck_manifest, sideboard_evaluation, resources) =
        resources.into_sideboard_model_parts_v1()?;
    let measurement = measure_competitive_event_runtime_sideboard_v1(
        runtime,
        deck_manifest,
        sideboard_evaluation,
        &resources.navigation_runtime,
        classifier_timeout_ms,
    )?;
    let request = bind_competitive_event_native_sideboard_request_v1(measurement, outcome)?;
    require_sha256_v1(
        request.model_input_commitment_sha256_v1(),
        "competitive operator sideboard request",
    )?;
    Ok(OpaqueMtgoCompetitiveOperatorNativeSideboardRequestV1 {
        resources,
        resource_commitments,
        request,
        prior_operator: commitments,
    })
}

/// Runs the exact post-entry sideboard ownership path through a
/// checked-untrusted offline scorer. The result remains unable to recover the
/// event runtime or submit either a changed or unchanged deck.
pub fn score_checked_untrusted_competitive_operator_native_sideboard_v1<
    S: MtgoCompetitiveNativeSideboardScorerV1,
>(
    value: OpaqueMtgoCompetitiveOperatorNativeSideboardRequestV1,
    scorer: &mut S,
) -> Result<OpaqueMtgoScoredCompetitiveOperatorNativeSideboardV1, String> {
    validate_operator_native_sideboard_scoring_v1(&OperatorNativeSideboardScoringIdentityV1 {
        resource_bundle_commitment_sha256: value
            .resource_commitments
            .resource_bundle_commitment_sha256
            .clone(),
        prior_resource_bundle_commitment_sha256: value
            .prior_operator
            .resource_bundle_commitment_sha256
            .clone(),
        request_deployment_commitment_sha256: value.deployment_commitment_sha256_v1().to_owned(),
        loaded_checkpoint_deployment_commitment_sha256: value
            .resources
            .checkpoint_deployment
            .deployment_commitment_sha256()
            .to_owned(),
    })?;
    let deployment_commitment_sha256 = value.deployment_commitment_sha256_v1().to_owned();
    let OpaqueMtgoCompetitiveOperatorNativeSideboardRequestV1 {
        resources,
        resource_commitments,
        request,
        prior_operator,
    } = value;
    let scored_request = score_checked_untrusted_competitive_native_sideboard_request_v1(
        request,
        &deployment_commitment_sha256,
        scorer,
    )?;
    Ok(OpaqueMtgoScoredCompetitiveOperatorNativeSideboardV1 {
        _resources: resources,
        _resource_commitments: resource_commitments,
        scored_request,
        _prior_operator: prior_operator,
    })
}

pub fn advance_competitive_post_entry_operator_observed_v1(
    operator: OpaqueMtgoCompetitivePostEntryOperatorV1,
    observed: MtgoObservedCompetitiveLifecycleAdvanceV1,
    next: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
) -> Result<OpaqueMtgoCompetitivePostEntryOperatorV1, String> {
    let directive = next_competitive_post_entry_operator_directive_v1(&operator)?;
    match &directive.route {
        MtgoCompetitivePostEntryOperatorRouteV1::ObserveLifecycle { allowed_advances }
            if allowed_advances.contains(&observed) => {}
        _ => {
            return Err(
                "competitive post-entry operator does not permit that observed advance now"
                    .to_owned(),
            )
        }
    }
    let runtime = advance_competitive_event_runtime_observed_v1(operator.runtime, observed, next)?;
    advance_operator_v1(
        operator.resources,
        operator.resource_commitments,
        runtime,
        operator.commitments,
    )
}

pub fn prepare_competitive_post_entry_operator_lifecycle_v1(
    operator: OpaqueMtgoCompetitivePostEntryOperatorV1,
) -> Result<OpaqueMtgoPreparedCompetitiveOperatorLifecycleV1, String> {
    let directive = next_competitive_post_entry_operator_directive_v1(&operator)?;
    let action = match directive.route {
        MtgoCompetitivePostEntryOperatorRouteV1::LifecycleControl { action } => action,
        _ => {
            return Err(
                "competitive post-entry operator is not at a lifecycle-control step".to_owned(),
            )
        }
    };
    let prepared =
        prepare_competitive_event_runtime_lifecycle_control_v1(operator.runtime, action)?;
    let prepared_commitments = prepared.commitments_v1();
    validate_prepared_operator_lifecycle_v1(&operator.commitments, action, &prepared_commitments)?;
    Ok(OpaqueMtgoPreparedCompetitiveOperatorLifecycleV1 {
        resources: operator.resources,
        resource_commitments: operator.resource_commitments,
        prepared,
        prior_operator: operator.commitments,
    })
}

pub fn execute_prepared_competitive_post_entry_operator_lifecycle_v1(
    prepared: OpaqueMtgoPreparedCompetitiveOperatorLifecycleV1,
) -> Result<OpaqueMtgoPendingCompetitiveOperatorLifecycleV1, String> {
    let pending = execute_prepared_competitive_event_lifecycle_control_v1(prepared.prepared)?;
    let pending_commitments = pending.commitments_v1();
    validate_pending_operator_lifecycle_v1(&prepared.prior_operator, &pending_commitments)?;
    Ok(OpaqueMtgoPendingCompetitiveOperatorLifecycleV1 {
        resources: prepared.resources,
        resource_commitments: prepared.resource_commitments,
        pending,
        prior_operator: prepared.prior_operator,
    })
}

pub fn confirm_pending_competitive_post_entry_operator_lifecycle_v1(
    pending: OpaqueMtgoPendingCompetitiveOperatorLifecycleV1,
    after: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
) -> Result<OpaqueMtgoCompetitivePostEntryOperatorV1, String> {
    let runtime = confirm_pending_competitive_event_lifecycle_control_v1(pending.pending, after)?;
    advance_operator_v1(
        pending.resources,
        pending.resource_commitments,
        runtime,
        pending.prior_operator,
    )
}

pub fn observe_competitive_post_entry_operator_event_record_v1(
    operator: OpaqueMtgoCompetitivePostEntryOperatorV1,
    record: OpaqueMtgoClassifiedCompetitiveEventRecordV1,
) -> Result<OpaqueMtgoCompetitivePostEntryOperatorV1, String> {
    let directive = next_competitive_post_entry_operator_directive_v1(&operator)?;
    let runtime = match directive.route {
        MtgoCompetitivePostEntryOperatorRouteV1::BeginTerminalEventRecordMonitor => {
            let monitor = begin_evaluated_competitive_event_monitor_v1(
                record,
                &operator.resources.event_record_evaluation,
            )?;
            attach_competitive_event_monitor_to_runtime_v1(operator.runtime, monitor)?
        }
        MtgoCompetitivePostEntryOperatorRouteV1::AdvanceTerminalEventRecordMonitor { .. } => {
            advance_competitive_event_monitor_in_runtime_v1(operator.runtime, record)?
        }
        _ => {
            return Err("competitive post-entry operator is not at an event-record step".to_owned())
        }
    };
    advance_operator_v1(
        operator.resources,
        operator.resource_commitments,
        runtime,
        operator.commitments,
    )
}

pub fn checkout_competitive_post_entry_operator_gameplay_v1(
    operator: OpaqueMtgoCompetitivePostEntryOperatorV1,
    session: OpaqueMtgoCompetitiveGestureGameSessionV1,
) -> Result<
    (
        OpaqueMtgoCompetitiveOperatorGameplayLeaseV1,
        OpaqueMtgoCompetitiveGestureGameSessionV1,
    ),
    String,
> {
    let directive = next_competitive_post_entry_operator_directive_v1(&operator)?;
    match directive.route {
        MtgoCompetitivePostEntryOperatorRouteV1::LaunchGameplay {
            native_model_path_present: true,
            ..
        } => {}
        _ => {
            return Err(
                "competitive post-entry operator is not at a model-ready gameplay launch"
                    .to_owned(),
            )
        }
    }
    validate_operator_gameplay_session_resources_v1(
        &operator.resource_commitments,
        &session.commitments_v1(),
    )?;
    let (lease, session) =
        checkout_competitive_event_gameplay_session_v1(operator.runtime, session)?;
    validate_operator_gameplay_lease_v1(&operator.commitments, &lease.commitments_v1())?;
    Ok((
        OpaqueMtgoCompetitiveOperatorGameplayLeaseV1 {
            resources: operator.resources,
            resource_commitments: operator.resource_commitments,
            lease,
            prior_operator: operator.commitments,
        },
        session,
    ))
}

pub fn return_competitive_post_entry_operator_gameplay_v1(
    lease: OpaqueMtgoCompetitiveOperatorGameplayLeaseV1,
    session: OpaqueMtgoCompetitiveGestureGameSessionV1,
) -> Result<OpaqueMtgoCompetitivePostEntryOperatorV1, String> {
    let runtime = return_competitive_event_gameplay_session_v1(lease.lease, session)?;
    advance_operator_v1(
        lease.resources,
        lease.resource_commitments,
        runtime,
        lease.prior_operator,
    )
}

/// Scores one current player-visible duel decision through the exact loaded
/// checkpoint and resolves its selected legal semantic to the exact visible
/// control. The returned move-only selection retains the gameplay lease and
/// attended game session and exposes only the selected semantic.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorGameplaySelectionV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveOperatorGameplaySelectionV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorGameplaySelectionV1;
/// fn cannot_act(value: OpaqueMtgoCompetitiveOperatorGameplaySelectionV1) {
///     let _ = value.input_command();
/// }
/// ```
pub fn select_competitive_post_entry_operator_gameplay_action_v1(
    lease: OpaqueMtgoCompetitiveOperatorGameplayLeaseV1,
    session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    perception: OpaqueMtgoAdmittedDuelPerceptionV1,
) -> Result<OpaqueMtgoCompetitiveOperatorGameplaySelectionV1, String> {
    let lease_commitments = lease.lease.commitments_v1();
    let session_commitments = session.commitments_v1();
    validate_operator_gameplay_action_source_v1(
        &lease.resource_commitments,
        &lease_commitments,
        &session_commitments,
        &perception.commitments_v1(),
        lease
            .resources
            .checkpoint_deployment
            .deployment_commitment_sha256(),
    )?;
    let selection = score_and_select_opaque_admitted_duel_perception_with_loaded_deployment_v1(
        perception,
        &lease.resources.duel_perception_profile,
        &lease.resources.checkpoint_deployment,
    )?;
    let control = resolve_opaque_profile_bound_duel_control_v1(selection)?;
    Ok(OpaqueMtgoCompetitiveOperatorGameplaySelectionV1 {
        lease,
        session,
        control,
    })
}

/// Binds the reviewed UI gesture for an already model-selected action to the
/// exact League or Challenge session and profile-pinned source target runtime.
/// The returned ordinary session-bound action still requires one distinct
/// fresh visible perception before any primitive can be prepared.
pub fn bind_competitive_post_entry_operator_gameplay_action_v1(
    selected: OpaqueMtgoCompetitiveOperatorGameplaySelectionV1,
    gesture_stages: Vec<MtgoDuelGestureStageV1>,
    postcondition_calibration: MtgoProfileBoundPostconditionCalibrationV1,
    postcondition_region_set: MtgoProfileBoundPostconditionRegionSetV1,
    gesture_target_timeout_ms: u32,
) -> Result<
    (
        OpaqueMtgoCompetitiveOperatorGameplayLeaseV1,
        OpaqueMtgoSessionBoundCompetitiveDuelGestureV1,
    ),
    String,
> {
    let OpaqueMtgoCompetitiveOperatorGameplaySelectionV1 {
        lease,
        session,
        control,
    } = selected;
    let (scope, gameplay_authorization) =
        competitive_gesture_game_session_action_authorities_v1(&session);
    let gesture_plan = control.gesture_plan_for_operator_v1(gesture_stages);
    let plan = prepare_opaque_competitive_duel_action_plan_v1(
        control,
        gesture_plan,
        postcondition_calibration,
        postcondition_region_set,
        &scope,
        &gameplay_authorization,
    )?;
    let sequence = begin_opaque_competitive_duel_gesture_sequence_from_pinned_runtime_v1(
        plan,
        &lease.resources.duel_gesture_profile,
        &lease.resources.duel_gesture_runtime,
        gesture_target_timeout_ms,
    )?;
    let bound = bind_competitive_duel_gesture_sequence_session_v1(sequence, session)?;
    Ok((lease, bound))
}

fn advance_operator_v1(
    resources: MtgoCompetitiveOperatorResourcesPartsV1,
    resource_commitments: MtgoCompetitiveOperatorResourceCommitmentsV1,
    runtime: OpaqueMtgoCompetitiveEventRuntimeV1,
    prior: MtgoCompetitivePostEntryOperatorCommitmentsV1,
) -> Result<OpaqueMtgoCompetitivePostEntryOperatorV1, String> {
    if resource_commitments.resource_bundle_commitment_sha256
        != prior.resource_bundle_commitment_sha256
    {
        return Err("competitive post-entry operator resources changed while advancing".to_owned());
    }
    let accepted_transition_count = prior
        .accepted_transition_count
        .checked_add(1)
        .ok_or("competitive post-entry operator transition count overflow")?;
    let runtime_commitments = runtime.commitments_v1();
    let commitments = post_entry_operator_commitments_v1(
        &resource_commitments,
        &runtime_commitments,
        accepted_transition_count,
        Some(prior.operator_commitment_sha256.as_str()),
        COMPETITIVE_POST_ENTRY_OPERATOR_ADVANCE_DOMAIN_V1,
    )?;
    Ok(OpaqueMtgoCompetitivePostEntryOperatorV1 {
        resources,
        resource_commitments,
        runtime,
        commitments,
    })
}

#[derive(Clone)]
struct PostEntryOperatorJoinIdentityV1 {
    resource_bundle_commitment_sha256: String,
    resource_navigation_profile_commitment_sha256: String,
    resource_navigation_profile_admission_commitment_sha256: String,
    resource_approved_account_alias_sha256: String,
    resource_deck_list_sha256: String,
    resource_deck_manifest_commitment_sha256: String,
    resource_deck_format_sha256: String,
    resource_policy_deployment_commitment_sha256: String,
    resource_checkpoint_competitive_capabilities_commitment_sha256: String,
    runtime_commitment_sha256: String,
    runtime_navigation_profile_commitment_sha256: String,
    runtime_navigation_profile_admission_commitment_sha256: String,
    runtime_approved_account_alias_sha256: String,
    runtime_deck_list_sha256: String,
    runtime_deck_manifest_commitment_sha256: String,
    runtime_deck_format_sha256: String,
    runtime_policy_deployment_commitment_sha256: String,
    event_kind: MtgoCompetitiveEventKindV1,
    current_phase: MtgoCompetitiveLifecyclePhaseV1,
    current_frame_sequence: u64,
}

#[derive(Clone)]
struct OperatorNativePregameCheckoutIdentityV1 {
    route_match_identity_sha256: String,
    route_game_number: u8,
    launch_match_identity_sha256: String,
    launch_game_number: u8,
    launch_event_kind: MtgoCompetitiveEventKindV1,
    operator_event_kind: MtgoCompetitiveEventKindV1,
    resource_bundle_commitment_sha256: String,
    operator_resource_bundle_commitment_sha256: String,
}

fn validate_operator_native_pregame_checkout_v1(
    value: &OperatorNativePregameCheckoutIdentityV1,
) -> Result<(), String> {
    for (digest, label) in [
        (&value.route_match_identity_sha256, "pregame route match"),
        (&value.launch_match_identity_sha256, "pregame launch match"),
        (
            &value.resource_bundle_commitment_sha256,
            "pregame resource bundle",
        ),
        (
            &value.operator_resource_bundle_commitment_sha256,
            "pregame operator resource bundle",
        ),
    ] {
        require_sha256_v1(digest, label)?;
    }
    if value.route_game_number == 0 || value.launch_game_number == 0 {
        return Err("competitive operator pregame checkout requires a nonzero game".to_owned());
    }
    if value.route_match_identity_sha256 != value.launch_match_identity_sha256
        || value.route_game_number != value.launch_game_number
        || value.launch_event_kind != value.operator_event_kind
        || value.resource_bundle_commitment_sha256
            != value.operator_resource_bundle_commitment_sha256
    {
        return Err(
            "competitive operator pregame checkout changed the exact event, match, game, or resources"
                .to_owned(),
        );
    }
    Ok(())
}

#[derive(Clone)]
struct OperatorNativePregameScoringIdentityV1 {
    resource_bundle_commitment_sha256: String,
    prior_resource_bundle_commitment_sha256: String,
    request_deployment_commitment_sha256: String,
    loaded_checkpoint_deployment_commitment_sha256: String,
}

fn validate_operator_native_pregame_scoring_v1(
    value: &OperatorNativePregameScoringIdentityV1,
) -> Result<(), String> {
    for (digest, label) in [
        (
            &value.resource_bundle_commitment_sha256,
            "pregame scoring resource bundle",
        ),
        (
            &value.prior_resource_bundle_commitment_sha256,
            "pregame scoring prior resource bundle",
        ),
        (
            &value.request_deployment_commitment_sha256,
            "pregame scoring request deployment",
        ),
        (
            &value.loaded_checkpoint_deployment_commitment_sha256,
            "pregame scoring loaded checkpoint deployment",
        ),
    ] {
        require_sha256_v1(digest, label)?;
    }
    if value.resource_bundle_commitment_sha256 != value.prior_resource_bundle_commitment_sha256
        || value.request_deployment_commitment_sha256
            != value.loaded_checkpoint_deployment_commitment_sha256
    {
        return Err(
            "competitive operator pregame scoring changed resources or deployment".to_owned(),
        );
    }
    Ok(())
}

#[derive(Clone)]
struct OperatorNativeSideboardCheckoutIdentityV1 {
    route_match_identity_sha256: String,
    route_game_number: u8,
    route_changed_resources_present: bool,
    outcome_match_identity_sha256: String,
    outcome_game_number: u8,
    outcome_event_kind: MtgoCompetitiveEventKindV1,
    operator_event_kind: MtgoCompetitiveEventKindV1,
    resource_bundle_commitment_sha256: String,
    operator_resource_bundle_commitment_sha256: String,
    resource_sideboard_evaluation_ratification_commitment_sha256: Option<String>,
    resource_sideboard_evaluation_admission_commitment_sha256: Option<String>,
    operator_deck_manifest_commitment_sha256: String,
    operator_policy_deployment_commitment_sha256: String,
}

fn validate_operator_native_sideboard_checkout_v1(
    value: &OperatorNativeSideboardCheckoutIdentityV1,
) -> Result<(), String> {
    for (digest, label) in [
        (&value.route_match_identity_sha256, "sideboard route match"),
        (
            &value.outcome_match_identity_sha256,
            "sideboard outcome match",
        ),
        (
            &value.resource_bundle_commitment_sha256,
            "sideboard resource bundle",
        ),
        (
            &value.operator_resource_bundle_commitment_sha256,
            "sideboard operator resource bundle",
        ),
        (
            &value.operator_deck_manifest_commitment_sha256,
            "sideboard operator deck manifest",
        ),
        (
            &value.operator_policy_deployment_commitment_sha256,
            "sideboard operator policy",
        ),
    ] {
        require_sha256_v1(digest, label)?;
    }
    let resource_evaluation = value
        .resource_sideboard_evaluation_ratification_commitment_sha256
        .as_deref()
        .ok_or("competitive operator sideboard checkout requires retained sideboard evaluation")?;
    let resource_admission = value
        .resource_sideboard_evaluation_admission_commitment_sha256
        .as_deref()
        .ok_or("competitive operator sideboard checkout requires retained sideboard admission")?;
    require_sha256_v1(resource_evaluation, "sideboard resource evaluation")?;
    require_sha256_v1(
        resource_admission,
        "sideboard resource evaluation admission",
    )?;
    if value.route_game_number == 0 || value.outcome_game_number == 0 {
        return Err("competitive operator sideboard checkout requires a nonzero game".to_owned());
    }
    if !value.route_changed_resources_present
        || value.route_match_identity_sha256 != value.outcome_match_identity_sha256
        || value.route_game_number != value.outcome_game_number
        || value.outcome_event_kind != value.operator_event_kind
        || value.resource_bundle_commitment_sha256
            != value.operator_resource_bundle_commitment_sha256
    {
        return Err(
            "competitive operator sideboard checkout changed the exact event, outcome, resources, deck, or deployment"
                .to_owned(),
        );
    }
    Ok(())
}

#[derive(Clone)]
struct OperatorNativeSideboardScoringIdentityV1 {
    resource_bundle_commitment_sha256: String,
    prior_resource_bundle_commitment_sha256: String,
    request_deployment_commitment_sha256: String,
    loaded_checkpoint_deployment_commitment_sha256: String,
}

fn validate_operator_native_sideboard_scoring_v1(
    value: &OperatorNativeSideboardScoringIdentityV1,
) -> Result<(), String> {
    for (digest, label) in [
        (
            &value.resource_bundle_commitment_sha256,
            "sideboard scoring resource bundle",
        ),
        (
            &value.prior_resource_bundle_commitment_sha256,
            "sideboard scoring prior resource bundle",
        ),
        (
            &value.request_deployment_commitment_sha256,
            "sideboard scoring request deployment",
        ),
        (
            &value.loaded_checkpoint_deployment_commitment_sha256,
            "sideboard scoring loaded checkpoint deployment",
        ),
    ] {
        require_sha256_v1(digest, label)?;
    }
    if value.resource_bundle_commitment_sha256 != value.prior_resource_bundle_commitment_sha256
        || value.request_deployment_commitment_sha256
            != value.loaded_checkpoint_deployment_commitment_sha256
    {
        return Err(
            "competitive operator sideboard scoring changed resources or deployment".to_owned(),
        );
    }
    Ok(())
}

fn post_entry_operator_commitments_v1(
    resources: &MtgoCompetitiveOperatorResourceCommitmentsV1,
    runtime: &MtgoCompetitiveEventRuntimeCommitmentsV1,
    accepted_transition_count: u64,
    prior_operator_commitment_sha256: Option<&str>,
    domain: &[u8],
) -> Result<MtgoCompetitivePostEntryOperatorCommitmentsV1, String> {
    let identity = PostEntryOperatorJoinIdentityV1 {
        resource_bundle_commitment_sha256: resources.resource_bundle_commitment_sha256.clone(),
        resource_navigation_profile_commitment_sha256: resources
            .navigation_profile_commitment_sha256
            .clone(),
        resource_navigation_profile_admission_commitment_sha256: resources
            .navigation_profile_admission_commitment_sha256
            .clone(),
        resource_approved_account_alias_sha256: resources.approved_account_alias_sha256.clone(),
        resource_deck_list_sha256: resources.deck_list_sha256.clone(),
        resource_deck_manifest_commitment_sha256: resources.deck_manifest_commitment_sha256.clone(),
        resource_deck_format_sha256: resources.deck_format_sha256.clone(),
        resource_policy_deployment_commitment_sha256: resources
            .policy_deployment_commitment_sha256
            .clone(),
        resource_checkpoint_competitive_capabilities_commitment_sha256: resources
            .checkpoint_competitive_capabilities_commitment_sha256
            .clone(),
        runtime_commitment_sha256: runtime.runtime_commitment_sha256.clone(),
        runtime_navigation_profile_commitment_sha256: runtime
            .navigation_profile_commitment_sha256
            .clone(),
        runtime_navigation_profile_admission_commitment_sha256: runtime
            .navigation_profile_admission_commitment_sha256
            .clone(),
        runtime_approved_account_alias_sha256: runtime.approved_account_alias_sha256.clone(),
        runtime_deck_list_sha256: runtime.deck_list_sha256.clone(),
        runtime_deck_manifest_commitment_sha256: runtime.deck_manifest_sha256.clone(),
        runtime_deck_format_sha256: runtime.deck_format_sha256.clone(),
        runtime_policy_deployment_commitment_sha256: runtime
            .policy_deployment_commitment_sha256
            .clone(),
        event_kind: runtime.event_kind,
        current_phase: runtime.current_phase,
        current_frame_sequence: runtime.current_frame_sequence,
    };
    validate_post_entry_operator_join_v1(&identity)?;
    if let Some(prior) = prior_operator_commitment_sha256 {
        require_sha256_v1(prior, "prior operator commitment")?;
    }
    let operator_commitment_sha256 = hash_parts_v1(
        domain,
        &[
            prior_operator_commitment_sha256.unwrap_or("").as_bytes(),
            identity.resource_bundle_commitment_sha256.as_bytes(),
            identity.runtime_commitment_sha256.as_bytes(),
            identity.resource_deck_manifest_commitment_sha256.as_bytes(),
            identity
                .resource_checkpoint_competitive_capabilities_commitment_sha256
                .as_bytes(),
            competitive_event_kind_tag_v1(identity.event_kind),
            competitive_phase_tag_v1(identity.current_phase),
            identity.current_frame_sequence.to_be_bytes().as_slice(),
            accepted_transition_count.to_be_bytes().as_slice(),
            b"exact_resources_and_entered_event_runtime_move_only_no_new_entry_no_spending",
        ],
    );
    Ok(MtgoCompetitivePostEntryOperatorCommitmentsV1 {
        resource_bundle_commitment_sha256: identity.resource_bundle_commitment_sha256,
        event_runtime_commitment_sha256: identity.runtime_commitment_sha256,
        navigation_profile_commitment_sha256: identity
            .resource_navigation_profile_commitment_sha256,
        navigation_profile_admission_commitment_sha256: identity
            .resource_navigation_profile_admission_commitment_sha256,
        approved_account_alias_sha256: identity.resource_approved_account_alias_sha256,
        deck_list_sha256: identity.resource_deck_list_sha256,
        deck_manifest_commitment_sha256: identity.resource_deck_manifest_commitment_sha256,
        deck_format_sha256: identity.resource_deck_format_sha256,
        policy_deployment_commitment_sha256: identity.resource_policy_deployment_commitment_sha256,
        checkpoint_competitive_capabilities_commitment_sha256: identity
            .resource_checkpoint_competitive_capabilities_commitment_sha256,
        event_kind: identity.event_kind,
        current_phase: identity.current_phase,
        current_frame_sequence: identity.current_frame_sequence,
        accepted_transition_count,
        prior_operator_commitment_sha256: prior_operator_commitment_sha256.map(str::to_owned),
        operator_commitment_sha256,
    })
}

fn validate_post_entry_operator_join_v1(
    value: &PostEntryOperatorJoinIdentityV1,
) -> Result<(), String> {
    for (digest, label) in [
        (&value.resource_bundle_commitment_sha256, "resource bundle"),
        (
            &value.resource_navigation_profile_commitment_sha256,
            "resource navigation profile",
        ),
        (
            &value.resource_navigation_profile_admission_commitment_sha256,
            "resource navigation admission",
        ),
        (
            &value.resource_approved_account_alias_sha256,
            "resource account",
        ),
        (&value.resource_deck_list_sha256, "resource deck list"),
        (
            &value.resource_deck_manifest_commitment_sha256,
            "resource deck manifest",
        ),
        (&value.resource_deck_format_sha256, "resource deck format"),
        (
            &value.resource_policy_deployment_commitment_sha256,
            "resource policy",
        ),
        (
            &value.resource_checkpoint_competitive_capabilities_commitment_sha256,
            "resource checkpoint competitive capabilities",
        ),
        (&value.runtime_commitment_sha256, "event runtime"),
    ] {
        require_sha256_v1(digest, label)?;
    }
    if value.current_frame_sequence == 0 {
        return Err("competitive post-entry operator requires a nonzero current frame".to_owned());
    }
    if value.resource_navigation_profile_commitment_sha256
        != value.runtime_navigation_profile_commitment_sha256
        || value.resource_navigation_profile_admission_commitment_sha256
            != value.runtime_navigation_profile_admission_commitment_sha256
        || value.resource_approved_account_alias_sha256
            != value.runtime_approved_account_alias_sha256
        || value.resource_deck_list_sha256 != value.runtime_deck_list_sha256
        || value.resource_deck_manifest_commitment_sha256
            != value.runtime_deck_manifest_commitment_sha256
        || value.resource_deck_format_sha256 != value.runtime_deck_format_sha256
        || value.resource_policy_deployment_commitment_sha256
            != value.runtime_policy_deployment_commitment_sha256
    {
        return Err(
            "competitive post-entry operator resources and event runtime are crossed".to_owned(),
        );
    }
    Ok(())
}

fn directive_from_driver_v1(
    operator: &MtgoCompetitivePostEntryOperatorCommitmentsV1,
    checkpoint: &MtgoNativeCheckpointCompetitiveCapabilitiesV1,
    changed_sideboard_resources_present: bool,
    driver: &MtgoCompetitiveEventDriverDirectiveV1,
) -> Result<MtgoCompetitivePostEntryOperatorDirectiveV1, String> {
    if driver.source_runtime_commitment_sha256 != operator.event_runtime_commitment_sha256
        || driver.event_kind != operator.event_kind
        || driver.current_phase != operator.current_phase
        || driver.current_frame_sequence != operator.current_frame_sequence
    {
        return Err(
            "competitive post-entry driver changed the operator runtime lineage".to_owned(),
        );
    }
    validate_native_checkpoint_competitive_capabilities_v1(checkpoint)
        .map_err(|error| format!("competitive post-entry checkpoint capabilities: {error}"))?;
    if checkpoint.deployment_commitment_sha256 != operator.policy_deployment_commitment_sha256
        || checkpoint.capabilities_commitment_sha256
            != operator.checkpoint_competitive_capabilities_commitment_sha256
    {
        return Err(
            "competitive post-entry operator checkpoint capabilities changed lineage".to_owned(),
        );
    }
    let model = check_competitive_model_decision_readiness_v1();
    let route = route_from_driver_step_v1(
        &driver.step,
        driver.allowed_observed_advances_v1(),
        checkpoint.pregame_head_ready_v1() && model.public_model_owned_pregame_action_path_present,
        checkpoint.native_duel_action_interface_present
            && model.public_model_owned_duel_action_path_present,
        checkpoint.sideboard_head_ready_v1()
            && model.public_model_owned_changed_sideboard_path_present
            && model.public_model_owned_unchanged_sideboard_path_present,
        changed_sideboard_resources_present,
    );
    Ok(MtgoCompetitivePostEntryOperatorDirectiveV1 {
        source_operator_commitment_sha256: operator.operator_commitment_sha256.clone(),
        source_event_runtime_commitment_sha256: operator.event_runtime_commitment_sha256.clone(),
        resource_bundle_commitment_sha256: operator.resource_bundle_commitment_sha256.clone(),
        event_kind: operator.event_kind,
        current_phase: operator.current_phase,
        current_frame_sequence: operator.current_frame_sequence,
        route,
        safe_for_live_input: false,
        permits_event_entry: false,
        permits_spending: false,
    })
}

fn route_from_driver_step_v1(
    step: &MtgoCompetitiveEventDriverStepV1,
    allowed_observed_advances: &[MtgoObservedCompetitiveLifecycleAdvanceV1],
    pregame_model_path_present: bool,
    duel_model_path_present: bool,
    sideboard_model_path_present: bool,
    changed_sideboard_resources_present: bool,
) -> MtgoCompetitivePostEntryOperatorRouteV1 {
    match step {
        MtgoCompetitiveEventDriverStepV1::AwaitPairingOrEventEnd
        | MtgoCompetitiveEventDriverStepV1::AwaitGameOutcome { .. } => {
            MtgoCompetitivePostEntryOperatorRouteV1::ObserveLifecycle {
                allowed_advances: allowed_observed_advances.to_vec(),
            }
        }
        MtgoCompetitiveEventDriverStepV1::AcceptPairing
        | MtgoCompetitiveEventDriverStepV1::ContinueAfterMatch
        | MtgoCompetitiveEventDriverStepV1::ResumeMatch { .. }
        | MtgoCompetitiveEventDriverStepV1::CloseCompletedEvent => {
            MtgoCompetitivePostEntryOperatorRouteV1::LifecycleControl {
                action: match step {
                    MtgoCompetitiveEventDriverStepV1::AcceptPairing => {
                        MtgoCompetitiveLifecycleActionV1::AcceptPairing
                    }
                    MtgoCompetitiveEventDriverStepV1::ContinueAfterMatch => {
                        MtgoCompetitiveLifecycleActionV1::ContinueAfterMatch
                    }
                    MtgoCompetitiveEventDriverStepV1::ResumeMatch { .. } => {
                        MtgoCompetitiveLifecycleActionV1::ResumeMatch
                    }
                    MtgoCompetitiveEventDriverStepV1::CloseCompletedEvent => {
                        MtgoCompetitiveLifecycleActionV1::CloseCompletedEvent
                    }
                    _ => unreachable!(),
                },
            }
        }
        MtgoCompetitiveEventDriverStepV1::ResolvePregame {
            match_identity_sha256,
            game_number,
        } => MtgoCompetitivePostEntryOperatorRouteV1::ResolvePregameWithNativeModel {
            match_identity_sha256: match_identity_sha256.clone(),
            game_number: *game_number,
            native_model_path_present: pregame_model_path_present,
        },
        MtgoCompetitiveEventDriverStepV1::LaunchGameplay {
            match_identity_sha256,
            game_number,
        } => MtgoCompetitivePostEntryOperatorRouteV1::LaunchGameplay {
            match_identity_sha256: match_identity_sha256.clone(),
            game_number: *game_number,
            native_model_path_present: duel_model_path_present,
        },
        MtgoCompetitiveEventDriverStepV1::ResolveSideboard {
            match_identity_sha256,
            game_number,
        } => MtgoCompetitivePostEntryOperatorRouteV1::ResolveSideboardWithNativeModel {
            match_identity_sha256: match_identity_sha256.clone(),
            game_number: *game_number,
            native_model_path_present: sideboard_model_path_present,
            changed_sideboard_resources_present,
        },
        MtgoCompetitiveEventDriverStepV1::BeginTerminalEventRecordMonitor => {
            MtgoCompetitivePostEntryOperatorRouteV1::BeginTerminalEventRecordMonitor
        }
        MtgoCompetitiveEventDriverStepV1::AdvanceTerminalEventRecordMonitor {
            prior_observation_count,
        } => MtgoCompetitivePostEntryOperatorRouteV1::AdvanceTerminalEventRecordMonitor {
            prior_observation_count: *prior_observation_count,
        },
        MtgoCompetitiveEventDriverStepV1::Complete => {
            MtgoCompetitivePostEntryOperatorRouteV1::Complete
        }
    }
}

fn validate_prepared_operator_lifecycle_v1(
    operator: &MtgoCompetitivePostEntryOperatorCommitmentsV1,
    expected_action: MtgoCompetitiveLifecycleActionV1,
    prepared: &MtgoPreparedCompetitiveEventLifecycleControlCommitmentsV1,
) -> Result<(), String> {
    if prepared.prior_event_runtime_commitment_sha256 != operator.event_runtime_commitment_sha256
        || prepared.event_kind != operator.event_kind
        || prepared.action != expected_action
        || prepared.source_frame_sequence != operator.current_frame_sequence
    {
        return Err(
            "prepared lifecycle control changed the post-entry operator lineage".to_owned(),
        );
    }
    Ok(())
}

fn validate_pending_operator_lifecycle_v1(
    operator: &MtgoCompetitivePostEntryOperatorCommitmentsV1,
    pending: &MtgoPendingCompetitiveEventLifecycleControlCommitmentsV1,
) -> Result<(), String> {
    if pending.prior_event_runtime_commitment_sha256 != operator.event_runtime_commitment_sha256
        || pending.event_kind != operator.event_kind
    {
        return Err("pending lifecycle input changed the post-entry operator lineage".to_owned());
    }
    Ok(())
}

fn validate_operator_gameplay_lease_v1(
    operator: &MtgoCompetitivePostEntryOperatorCommitmentsV1,
    lease: &MtgoCompetitiveEventGameplayLeaseCommitmentsV1,
) -> Result<(), String> {
    if lease.event_runtime_commitment_sha256 != operator.event_runtime_commitment_sha256
        || lease.event_kind != operator.event_kind
        || lease.deck_manifest_sha256 != operator.deck_manifest_commitment_sha256
        || lease.deck_format_sha256 != operator.deck_format_sha256
        || lease.policy_deployment_commitment_sha256 != operator.policy_deployment_commitment_sha256
    {
        return Err("gameplay checkout changed the post-entry operator lineage".to_owned());
    }
    Ok(())
}

fn validate_operator_gameplay_session_resources_v1(
    resources: &MtgoCompetitiveOperatorResourceCommitmentsV1,
    session: &MtgoCompetitiveGestureGameSessionCommitmentsV1,
) -> Result<(), String> {
    if session.gesture_evaluation_commitment_sha256
        != resources.duel_gesture_evaluation_commitment_sha256
        || session.gesture_profile_admission_commitment_sha256
            != resources.duel_gesture_profile_admission_commitment_sha256
    {
        return Err(
            "gameplay session changed the post-entry operator gesture resources".to_owned(),
        );
    }
    Ok(())
}

fn validate_operator_gameplay_action_source_v1(
    resources: &MtgoCompetitiveOperatorResourceCommitmentsV1,
    lease: &MtgoCompetitiveEventGameplayLeaseCommitmentsV1,
    session: &MtgoCompetitiveGestureGameSessionCommitmentsV1,
    perception: &crate::MtgoAdmittedDuelPerceptionCommitmentsV1,
    loaded_deployment_commitment_sha256: &str,
) -> Result<(), String> {
    let session_policy = session
        .policy_deployment_commitment_sha256
        .as_deref()
        .ok_or("gameplay action requires a session bound to the selected policy deployment")?;
    if resources.policy_deployment_commitment_sha256 != loaded_deployment_commitment_sha256
        || lease.policy_deployment_commitment_sha256
            != resources.policy_deployment_commitment_sha256
        || session_policy != resources.policy_deployment_commitment_sha256
        || perception
            .source_frame
            .perception_profile_admission_commitment_sha256
            != resources.duel_perception_profile_admission_commitment_sha256
        || session.gesture_evaluation_commitment_sha256
            != resources.duel_gesture_evaluation_commitment_sha256
        || session.gesture_profile_admission_commitment_sha256
            != resources.duel_gesture_profile_admission_commitment_sha256
        || lease.event_kind != session.event_kind
        || lease.game_number != session.game_number
        || perception
            .competitive_lifecycle_snapshot_commitment_sha256
            .is_none()
        || perception.frame_sequence < session.valid_from_frame_sequence
        || perception.frame_sequence <= session.last_confirmed_frame_sequence
        || perception.frame_sequence > session.valid_through_frame_sequence
    {
        return Err(
            "gameplay action changed the operator deployment, visible perception, gesture profile, event, game, or frame lifetime"
                .to_owned(),
        );
    }
    Ok(())
}

fn require_sha256_v1(value: &str, label: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("{label} must be a lowercase SHA-256 digest"));
    }
    Ok(())
}

fn hash_parts_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

fn competitive_event_kind_tag_v1(value: MtgoCompetitiveEventKindV1) -> &'static [u8] {
    match value {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    }
}

fn competitive_phase_tag_v1(value: MtgoCompetitiveLifecyclePhaseV1) -> &'static [u8] {
    match value {
        MtgoCompetitiveLifecyclePhaseV1::EventBrowser => b"event_browser",
        MtgoCompetitiveLifecyclePhaseV1::EntryReview => b"entry_review",
        MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing => b"entered_waiting_for_pairing",
        MtgoCompetitiveLifecyclePhaseV1::PairingReady => b"pairing_ready",
        MtgoCompetitiveLifecyclePhaseV1::MatchInProgress => b"match_in_progress",
        MtgoCompetitiveLifecyclePhaseV1::Sideboarding => b"sideboarding",
        MtgoCompetitiveLifecyclePhaseV1::MatchComplete => b"match_complete",
        MtgoCompetitiveLifecyclePhaseV1::EventComplete => b"event_complete",
        MtgoCompetitiveLifecyclePhaseV1::Reconnect => b"reconnect",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(value: char) -> String {
        value.to_string().repeat(64)
    }

    fn join_v1() -> PostEntryOperatorJoinIdentityV1 {
        PostEntryOperatorJoinIdentityV1 {
            resource_bundle_commitment_sha256: digest('0'),
            resource_navigation_profile_commitment_sha256: digest('1'),
            resource_navigation_profile_admission_commitment_sha256: digest('2'),
            resource_approved_account_alias_sha256: digest('3'),
            resource_deck_list_sha256: digest('4'),
            resource_deck_manifest_commitment_sha256: digest('5'),
            resource_deck_format_sha256: digest('6'),
            resource_policy_deployment_commitment_sha256: digest('7'),
            resource_checkpoint_competitive_capabilities_commitment_sha256: digest('a'),
            runtime_commitment_sha256: digest('8'),
            runtime_navigation_profile_commitment_sha256: digest('1'),
            runtime_navigation_profile_admission_commitment_sha256: digest('2'),
            runtime_approved_account_alias_sha256: digest('3'),
            runtime_deck_list_sha256: digest('4'),
            runtime_deck_manifest_commitment_sha256: digest('5'),
            runtime_deck_format_sha256: digest('6'),
            runtime_policy_deployment_commitment_sha256: digest('7'),
            event_kind: MtgoCompetitiveEventKindV1::League,
            current_phase: MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing,
            current_frame_sequence: 10,
        }
    }

    fn operator_commitments_v1() -> MtgoCompetitivePostEntryOperatorCommitmentsV1 {
        MtgoCompetitivePostEntryOperatorCommitmentsV1 {
            resource_bundle_commitment_sha256: digest('0'),
            event_runtime_commitment_sha256: digest('1'),
            navigation_profile_commitment_sha256: digest('2'),
            navigation_profile_admission_commitment_sha256: digest('3'),
            approved_account_alias_sha256: digest('4'),
            deck_list_sha256: digest('5'),
            deck_manifest_commitment_sha256: digest('6'),
            deck_format_sha256: digest('7'),
            policy_deployment_commitment_sha256: digest('8'),
            checkpoint_competitive_capabilities_commitment_sha256: digest('b'),
            event_kind: MtgoCompetitiveEventKindV1::League,
            current_phase: MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
            current_frame_sequence: 10,
            accepted_transition_count: 1,
            prior_operator_commitment_sha256: Some(digest('9')),
            operator_commitment_sha256: digest('a'),
        }
    }

    fn checkpoint_capabilities_v1(
        native_duel_action_interface_present: bool,
    ) -> MtgoNativeCheckpointCompetitiveCapabilitiesV1 {
        let mut value = MtgoNativeCheckpointCompetitiveCapabilitiesV1 {
            schema_version:
                mtgo_blackbox_v1::MTGO_NATIVE_CHECKPOINT_COMPETITIVE_CAPABILITIES_SCHEMA_V1,
            deployment_commitment_sha256: digest('8'),
            native_duel_action_interface_present,
            native_pregame_interface_present: false,
            terminal_outcome_trained_pregame_head_present: false,
            native_sideboard_interface_present: false,
            terminal_outcome_trained_sideboard_head_present: false,
            native_changed_sideboard_action_present: false,
            native_unchanged_sideboard_action_present: false,
            capabilities_commitment_sha256: String::new(),
        };
        value.capabilities_commitment_sha256 =
            mtgo_blackbox_v1::native_checkpoint_competitive_capabilities_commitment_v1(&value)
                .unwrap();
        value
    }

    fn gameplay_lease_v1() -> MtgoCompetitiveEventGameplayLeaseCommitmentsV1 {
        MtgoCompetitiveEventGameplayLeaseCommitmentsV1 {
            event_runtime_commitment_sha256: digest('1'),
            initial_game_session_commitment_sha256: digest('b'),
            gameplay_lease_commitment_sha256: digest('c'),
            event_kind: MtgoCompetitiveEventKindV1::League,
            event_identity_sha256: digest('d'),
            match_identity_sha256: digest('e'),
            entry_ratification_commitment_sha256: digest('f'),
            selected_deck_label_sha256: digest('0'),
            selected_deck_region_sha256: digest('1'),
            deck_manifest_sha256: digest('6'),
            deck_format_sha256: digest('7'),
            policy_deployment_commitment_sha256: digest('8'),
            game_number: 1,
            checkout_frame_sequence: 10,
            initial_confirmed_action_count: 0,
        }
    }

    fn resource_commitments_v1() -> MtgoCompetitiveOperatorResourceCommitmentsV1 {
        MtgoCompetitiveOperatorResourceCommitmentsV1 {
            navigation_profile_commitment_sha256: digest('0'),
            navigation_profile_admission_commitment_sha256: digest('1'),
            approved_account_alias_sha256: digest('2'),
            navigation_runtime_identity_commitment_sha256: digest('3'),
            event_listing_evaluation_ratification_commitment_sha256: digest('4'),
            event_listing_evaluation_admission_commitment_sha256: digest('5'),
            event_record_evaluation_ratification_commitment_sha256: digest('6'),
            event_record_evaluation_admission_commitment_sha256: digest('7'),
            deck_list_sha256: digest('8'),
            deck_manifest_commitment_sha256: digest('9'),
            deck_format_sha256: digest('a'),
            policy_deployment_commitment_sha256: digest('b'),
            checkpoint_competitive_capabilities_commitment_sha256: digest('c'),
            duel_perception_profile_commitment_sha256: digest('c'),
            duel_perception_profile_admission_commitment_sha256: digest('d'),
            duel_perception_runtime_identity_commitment_sha256: digest('e'),
            duel_lifecycle_evaluation_commitment_sha256: digest('f'),
            duel_lifecycle_admission_commitment_sha256: digest('0'),
            duel_gesture_evaluation_commitment_sha256: digest('1'),
            duel_gesture_profile_admission_commitment_sha256: digest('2'),
            duel_gesture_runtime_identity_commitment_sha256: digest('3'),
            changed_sideboard_evaluation_ratification_commitment_sha256: None,
            changed_sideboard_evaluation_admission_commitment_sha256: None,
            resource_bundle_commitment_sha256: digest('4'),
        }
    }

    fn gameplay_session_v1() -> MtgoCompetitiveGestureGameSessionCommitmentsV1 {
        MtgoCompetitiveGestureGameSessionCommitmentsV1 {
            session_commitment_sha256: digest('0'),
            general_gesture_permission_commitment_sha256: digest('1'),
            mode_authorization_commitment_sha256: digest('2'),
            correspondence_sha256: digest('3'),
            permission_review_commitment_sha256: digest('4'),
            pass_match_launch_commitment_sha256: digest('5'),
            match_gameplay_authorization_commitment_sha256: digest('6'),
            gesture_match_launch_commitment_sha256: digest('7'),
            gesture_evaluation_commitment_sha256: digest('1'),
            gesture_profile_admission_commitment_sha256: digest('2'),
            entry_ratification_commitment_sha256: None,
            selected_deck_label_sha256: None,
            selected_deck_region_sha256: None,
            deck_manifest_sha256: None,
            deck_format_sha256: None,
            policy_deployment_commitment_sha256: None,
            event_kind: MtgoCompetitiveEventKindV1::League,
            game_number: 1,
            valid_from_frame_sequence: 10,
            valid_through_frame_sequence: 20,
            last_confirmed_frame_sequence: 9,
            confirmed_action_count: 0,
        }
    }

    fn gameplay_action_perception_v1() -> crate::MtgoAdmittedDuelPerceptionCommitmentsV1 {
        crate::MtgoAdmittedDuelPerceptionCommitmentsV1 {
            source_frame: crate::MtgoAdmittedDuelVisibleFrameCommitmentsV1 {
                perception_profile_commitment_sha256: digest('c'),
                perception_profile_admission_commitment_sha256: digest('d'),
                frame_profile_binding_sha256: digest('e'),
                source_capture: crate::MtgoDxgiFrameCommitmentsV3 {
                    capture_commitment_sha256: digest('f'),
                    canonical_bgra8_sha256: digest('0'),
                    preview_png_sha256: digest('1'),
                    canonical_width: 1_550,
                    canonical_height: 925,
                    client_rect_desktop_px: crate::SignedRectV1 {
                        left: 0,
                        top: 0,
                        right: 1_550,
                        bottom: 925,
                    },
                    captured_at_unix_millis: 1,
                },
            },
            runtime_identity_commitment_sha256: digest('2'),
            request_commitment_sha256: digest('3'),
            decision_commitment_sha256: digest('4'),
            competitive_lifecycle_snapshot_commitment_sha256: Some(digest('5')),
            perception_result_commitment_sha256: digest('6'),
            frame_id: 11,
            frame_sequence: 11,
        }
    }

    fn gameplay_action_parts_v1() -> (
        MtgoCompetitiveOperatorResourceCommitmentsV1,
        MtgoCompetitiveEventGameplayLeaseCommitmentsV1,
        MtgoCompetitiveGestureGameSessionCommitmentsV1,
        crate::MtgoAdmittedDuelPerceptionCommitmentsV1,
    ) {
        let mut resources = resource_commitments_v1();
        resources.policy_deployment_commitment_sha256 = digest('8');
        let lease = gameplay_lease_v1();
        let mut session = gameplay_session_v1();
        session.policy_deployment_commitment_sha256 = Some(digest('8'));
        (resources, lease, session, gameplay_action_perception_v1())
    }

    fn native_pregame_checkout_identity_v1() -> OperatorNativePregameCheckoutIdentityV1 {
        OperatorNativePregameCheckoutIdentityV1 {
            route_match_identity_sha256: digest('1'),
            route_game_number: 2,
            launch_match_identity_sha256: digest('1'),
            launch_game_number: 2,
            launch_event_kind: MtgoCompetitiveEventKindV1::League,
            operator_event_kind: MtgoCompetitiveEventKindV1::League,
            resource_bundle_commitment_sha256: digest('2'),
            operator_resource_bundle_commitment_sha256: digest('2'),
        }
    }

    fn native_pregame_scoring_identity_v1() -> OperatorNativePregameScoringIdentityV1 {
        OperatorNativePregameScoringIdentityV1 {
            resource_bundle_commitment_sha256: digest('1'),
            prior_resource_bundle_commitment_sha256: digest('1'),
            request_deployment_commitment_sha256: digest('2'),
            loaded_checkpoint_deployment_commitment_sha256: digest('2'),
        }
    }

    fn native_sideboard_checkout_identity_v1() -> OperatorNativeSideboardCheckoutIdentityV1 {
        OperatorNativeSideboardCheckoutIdentityV1 {
            route_match_identity_sha256: digest('1'),
            route_game_number: 1,
            route_changed_resources_present: true,
            outcome_match_identity_sha256: digest('1'),
            outcome_game_number: 1,
            outcome_event_kind: MtgoCompetitiveEventKindV1::League,
            operator_event_kind: MtgoCompetitiveEventKindV1::League,
            resource_bundle_commitment_sha256: digest('2'),
            operator_resource_bundle_commitment_sha256: digest('2'),
            resource_sideboard_evaluation_ratification_commitment_sha256: Some(digest('3')),
            resource_sideboard_evaluation_admission_commitment_sha256: Some(digest('4')),
            operator_deck_manifest_commitment_sha256: digest('5'),
            operator_policy_deployment_commitment_sha256: digest('6'),
        }
    }

    fn native_sideboard_scoring_identity_v1() -> OperatorNativeSideboardScoringIdentityV1 {
        OperatorNativeSideboardScoringIdentityV1 {
            resource_bundle_commitment_sha256: digest('1'),
            prior_resource_bundle_commitment_sha256: digest('1'),
            request_deployment_commitment_sha256: digest('2'),
            loaded_checkpoint_deployment_commitment_sha256: digest('2'),
        }
    }

    #[test]
    fn exact_post_entry_join_accepts_both_modes_and_rejects_crossed_resources() {
        let mut league = join_v1();
        validate_post_entry_operator_join_v1(&league).unwrap();
        league.event_kind = MtgoCompetitiveEventKindV1::Challenge;
        validate_post_entry_operator_join_v1(&league).unwrap();

        for mutate in [
            |value: &mut PostEntryOperatorJoinIdentityV1| {
                value.runtime_navigation_profile_commitment_sha256 = digest('9')
            },
            |value: &mut PostEntryOperatorJoinIdentityV1| {
                value.runtime_navigation_profile_admission_commitment_sha256 = digest('9')
            },
            |value: &mut PostEntryOperatorJoinIdentityV1| {
                value.runtime_approved_account_alias_sha256 = digest('9')
            },
            |value: &mut PostEntryOperatorJoinIdentityV1| {
                value.runtime_deck_list_sha256 = digest('9')
            },
            |value: &mut PostEntryOperatorJoinIdentityV1| {
                value.runtime_deck_manifest_commitment_sha256 = digest('9')
            },
            |value: &mut PostEntryOperatorJoinIdentityV1| {
                value.runtime_deck_format_sha256 = digest('9')
            },
            |value: &mut PostEntryOperatorJoinIdentityV1| {
                value.runtime_policy_deployment_commitment_sha256 = digest('9')
            },
        ] {
            let mut crossed = join_v1();
            mutate(&mut crossed);
            assert!(validate_post_entry_operator_join_v1(&crossed).is_err());
        }
    }

    #[test]
    fn native_pregame_checkout_accepts_both_modes_and_rejects_crossed_lineage() {
        let mut exact = native_pregame_checkout_identity_v1();
        validate_operator_native_pregame_checkout_v1(&exact).unwrap();
        exact.launch_event_kind = MtgoCompetitiveEventKindV1::Challenge;
        exact.operator_event_kind = MtgoCompetitiveEventKindV1::Challenge;
        validate_operator_native_pregame_checkout_v1(&exact).unwrap();

        for mutate in [
            |value: &mut OperatorNativePregameCheckoutIdentityV1| {
                value.launch_match_identity_sha256 = digest('3')
            },
            |value: &mut OperatorNativePregameCheckoutIdentityV1| value.launch_game_number = 3,
            |value: &mut OperatorNativePregameCheckoutIdentityV1| {
                value.launch_event_kind = MtgoCompetitiveEventKindV1::Challenge
            },
            |value: &mut OperatorNativePregameCheckoutIdentityV1| {
                value.operator_resource_bundle_commitment_sha256 = digest('3')
            },
            |value: &mut OperatorNativePregameCheckoutIdentityV1| value.route_game_number = 0,
        ] {
            let mut crossed = native_pregame_checkout_identity_v1();
            mutate(&mut crossed);
            assert!(validate_operator_native_pregame_checkout_v1(&crossed).is_err());
        }
    }

    #[test]
    fn native_pregame_scoring_requires_exact_resources_and_loaded_deployment() {
        validate_operator_native_pregame_scoring_v1(&native_pregame_scoring_identity_v1()).unwrap();

        let mut crossed_resources = native_pregame_scoring_identity_v1();
        crossed_resources.prior_resource_bundle_commitment_sha256 = digest('3');
        assert!(validate_operator_native_pregame_scoring_v1(&crossed_resources).is_err());

        let mut crossed_deployment = native_pregame_scoring_identity_v1();
        crossed_deployment.loaded_checkpoint_deployment_commitment_sha256 = digest('3');
        assert!(validate_operator_native_pregame_scoring_v1(&crossed_deployment).is_err());

        let mut malformed = native_pregame_scoring_identity_v1();
        malformed.request_deployment_commitment_sha256 = "not-a-digest".to_owned();
        assert!(validate_operator_native_pregame_scoring_v1(&malformed).is_err());
    }

    #[test]
    fn native_sideboard_checkout_accepts_both_modes_and_rejects_crossed_lineage() {
        let mut exact = native_sideboard_checkout_identity_v1();
        validate_operator_native_sideboard_checkout_v1(&exact).unwrap();
        exact.outcome_event_kind = MtgoCompetitiveEventKindV1::Challenge;
        exact.operator_event_kind = MtgoCompetitiveEventKindV1::Challenge;
        validate_operator_native_sideboard_checkout_v1(&exact).unwrap();

        for mutate in [
            |value: &mut OperatorNativeSideboardCheckoutIdentityV1| {
                value.route_changed_resources_present = false
            },
            |value: &mut OperatorNativeSideboardCheckoutIdentityV1| {
                value.outcome_match_identity_sha256 = digest('7')
            },
            |value: &mut OperatorNativeSideboardCheckoutIdentityV1| value.outcome_game_number = 2,
            |value: &mut OperatorNativeSideboardCheckoutIdentityV1| {
                value.operator_event_kind = MtgoCompetitiveEventKindV1::Challenge
            },
            |value: &mut OperatorNativeSideboardCheckoutIdentityV1| {
                value.operator_resource_bundle_commitment_sha256 = digest('7')
            },
            |value: &mut OperatorNativeSideboardCheckoutIdentityV1| {
                value.resource_sideboard_evaluation_ratification_commitment_sha256 = None
            },
            |value: &mut OperatorNativeSideboardCheckoutIdentityV1| {
                value.resource_sideboard_evaluation_admission_commitment_sha256 = None
            },
            |value: &mut OperatorNativeSideboardCheckoutIdentityV1| {
                value.operator_deck_manifest_commitment_sha256 = "not-a-digest".to_owned()
            },
            |value: &mut OperatorNativeSideboardCheckoutIdentityV1| {
                value.operator_policy_deployment_commitment_sha256 = "not-a-digest".to_owned()
            },
        ] {
            let mut crossed = native_sideboard_checkout_identity_v1();
            mutate(&mut crossed);
            assert!(validate_operator_native_sideboard_checkout_v1(&crossed).is_err());
        }
    }

    #[test]
    fn native_sideboard_scoring_requires_exact_resources_and_loaded_deployment() {
        validate_operator_native_sideboard_scoring_v1(&native_sideboard_scoring_identity_v1())
            .unwrap();

        let mut crossed_resources = native_sideboard_scoring_identity_v1();
        crossed_resources.prior_resource_bundle_commitment_sha256 = digest('3');
        assert!(validate_operator_native_sideboard_scoring_v1(&crossed_resources).is_err());

        let mut crossed_deployment = native_sideboard_scoring_identity_v1();
        crossed_deployment.loaded_checkpoint_deployment_commitment_sha256 = digest('3');
        assert!(validate_operator_native_sideboard_scoring_v1(&crossed_deployment).is_err());
    }

    #[test]
    fn every_event_driver_branch_has_one_operator_route() {
        let match_id = digest('a');
        let cases = [
            MtgoCompetitiveEventDriverStepV1::AwaitPairingOrEventEnd,
            MtgoCompetitiveEventDriverStepV1::AcceptPairing,
            MtgoCompetitiveEventDriverStepV1::ResolvePregame {
                match_identity_sha256: match_id.clone(),
                game_number: 1,
            },
            MtgoCompetitiveEventDriverStepV1::LaunchGameplay {
                match_identity_sha256: match_id.clone(),
                game_number: 1,
            },
            MtgoCompetitiveEventDriverStepV1::AwaitGameOutcome {
                match_identity_sha256: match_id.clone(),
                game_number: 1,
            },
            MtgoCompetitiveEventDriverStepV1::ResolveSideboard {
                match_identity_sha256: match_id.clone(),
                game_number: 1,
            },
            MtgoCompetitiveEventDriverStepV1::ContinueAfterMatch,
            MtgoCompetitiveEventDriverStepV1::ResumeMatch {
                match_identity_sha256: match_id,
                game_number: 2,
            },
            MtgoCompetitiveEventDriverStepV1::BeginTerminalEventRecordMonitor,
            MtgoCompetitiveEventDriverStepV1::AdvanceTerminalEventRecordMonitor {
                prior_observation_count: 1,
            },
            MtgoCompetitiveEventDriverStepV1::CloseCompletedEvent,
            MtgoCompetitiveEventDriverStepV1::Complete,
        ];
        let mut routes = Vec::new();
        for step in &cases {
            routes.push(route_from_driver_step_v1(
                step,
                &[MtgoObservedCompetitiveLifecycleAdvanceV1::PairingPosted],
                false,
                true,
                false,
                true,
            ));
        }
        assert_eq!(routes.len(), cases.len());
        assert!(matches!(
            &routes[2],
            MtgoCompetitivePostEntryOperatorRouteV1::ResolvePregameWithNativeModel {
                native_model_path_present: false,
                ..
            }
        ));
        assert!(matches!(
            &routes[3],
            MtgoCompetitivePostEntryOperatorRouteV1::LaunchGameplay {
                native_model_path_present: true,
                ..
            }
        ));
        assert!(matches!(
            &routes[5],
            MtgoCompetitivePostEntryOperatorRouteV1::ResolveSideboardWithNativeModel {
                native_model_path_present: false,
                changed_sideboard_resources_present: true,
                ..
            }
        ));
    }

    #[test]
    fn gameplay_route_requires_the_exact_loaded_checkpoint_capability() {
        let unavailable_capabilities = checkpoint_capabilities_v1(false);
        let mut unavailable_operator = operator_commitments_v1();
        unavailable_operator.checkpoint_competitive_capabilities_commitment_sha256 =
            unavailable_capabilities
                .capabilities_commitment_sha256
                .clone();
        let driver = MtgoCompetitiveEventDriverDirectiveV1 {
            source_runtime_commitment_sha256: unavailable_operator
                .event_runtime_commitment_sha256
                .clone(),
            event_kind: unavailable_operator.event_kind,
            current_phase: unavailable_operator.current_phase,
            current_frame_sequence: unavailable_operator.current_frame_sequence,
            step: MtgoCompetitiveEventDriverStepV1::LaunchGameplay {
                match_identity_sha256: digest('c'),
                game_number: 1,
            },
        };

        let unavailable = directive_from_driver_v1(
            &unavailable_operator,
            &unavailable_capabilities,
            false,
            &driver,
        )
        .unwrap();
        assert!(matches!(
            unavailable.route,
            MtgoCompetitivePostEntryOperatorRouteV1::LaunchGameplay {
                native_model_path_present: false,
                ..
            }
        ));

        let available_capabilities = checkpoint_capabilities_v1(true);
        let mut available_operator = operator_commitments_v1();
        available_operator.checkpoint_competitive_capabilities_commitment_sha256 =
            available_capabilities
                .capabilities_commitment_sha256
                .clone();
        let available =
            directive_from_driver_v1(&available_operator, &available_capabilities, false, &driver)
                .unwrap();
        assert!(matches!(
            available.route,
            MtgoCompetitivePostEntryOperatorRouteV1::LaunchGameplay {
                native_model_path_present: true,
                ..
            }
        ));

        assert!(directive_from_driver_v1(
            &unavailable_operator,
            &available_capabilities,
            false,
            &driver
        )
        .is_err());
    }

    #[test]
    fn gameplay_lease_rejoins_only_the_exact_operator_lineage() {
        let operator = operator_commitments_v1();
        validate_operator_gameplay_lease_v1(&operator, &gameplay_lease_v1()).unwrap();
        for mutate in [
            |value: &mut MtgoCompetitiveEventGameplayLeaseCommitmentsV1| {
                value.event_runtime_commitment_sha256 = digest('9')
            },
            |value: &mut MtgoCompetitiveEventGameplayLeaseCommitmentsV1| {
                value.event_kind = MtgoCompetitiveEventKindV1::Challenge
            },
            |value: &mut MtgoCompetitiveEventGameplayLeaseCommitmentsV1| {
                value.deck_manifest_sha256 = digest('9')
            },
            |value: &mut MtgoCompetitiveEventGameplayLeaseCommitmentsV1| {
                value.deck_format_sha256 = digest('9')
            },
            |value: &mut MtgoCompetitiveEventGameplayLeaseCommitmentsV1| {
                value.policy_deployment_commitment_sha256 = digest('9')
            },
        ] {
            let mut crossed = gameplay_lease_v1();
            mutate(&mut crossed);
            assert!(validate_operator_gameplay_lease_v1(&operator, &crossed).is_err());
        }
    }

    #[test]
    fn gameplay_checkout_requires_the_operator_gesture_profile() {
        let resources = resource_commitments_v1();
        validate_operator_gameplay_session_resources_v1(&resources, &gameplay_session_v1())
            .unwrap();

        let mut wrong_evaluation = gameplay_session_v1();
        wrong_evaluation.gesture_evaluation_commitment_sha256 = digest('9');
        assert!(
            validate_operator_gameplay_session_resources_v1(&resources, &wrong_evaluation).is_err()
        );

        let mut wrong_admission = gameplay_session_v1();
        wrong_admission.gesture_profile_admission_commitment_sha256 = digest('9');
        assert!(
            validate_operator_gameplay_session_resources_v1(&resources, &wrong_admission).is_err()
        );
    }

    #[test]
    fn gameplay_action_composition_rejects_crossed_deployment_profile_and_frame_lifetime() {
        let (resources, lease, session, perception) = gameplay_action_parts_v1();
        validate_operator_gameplay_action_source_v1(
            &resources,
            &lease,
            &session,
            &perception,
            &digest('8'),
        )
        .unwrap();

        let mut crossed_deployment = session.clone();
        crossed_deployment.policy_deployment_commitment_sha256 = Some(digest('9'));
        assert!(validate_operator_gameplay_action_source_v1(
            &resources,
            &lease,
            &crossed_deployment,
            &perception,
            &digest('8'),
        )
        .is_err());

        let mut crossed_profile = perception.clone();
        crossed_profile
            .source_frame
            .perception_profile_admission_commitment_sha256 = digest('9');
        assert!(validate_operator_gameplay_action_source_v1(
            &resources,
            &lease,
            &session,
            &crossed_profile,
            &digest('8'),
        )
        .is_err());

        let mut stale = perception.clone();
        stale.frame_sequence = session.last_confirmed_frame_sequence;
        assert!(validate_operator_gameplay_action_source_v1(
            &resources,
            &lease,
            &session,
            &stale,
            &digest('8'),
        )
        .is_err());

        let mut missing_lifecycle = perception;
        missing_lifecycle.competitive_lifecycle_snapshot_commitment_sha256 = None;
        assert!(validate_operator_gameplay_action_source_v1(
            &resources,
            &lease,
            &session,
            &missing_lifecycle,
            &digest('8'),
        )
        .is_err());
    }
}
