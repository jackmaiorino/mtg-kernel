use crate::actuator::{
    advance_competitive_direct_visible_gameplay_session_v1,
    advance_competitive_event_monitor_in_runtime_v1, advance_competitive_event_runtime_observed_v1,
    advance_competitive_player_visible_gameplay_session_v1,
    attach_competitive_event_monitor_to_runtime_v1,
    bind_competitive_duel_gesture_sequence_session_v1,
    bind_competitive_event_native_sideboard_request_v1,
    bind_competitive_event_pregame_native_request_v1,
    bind_competitive_event_pregame_native_request_with_completed_history_v2,
    bind_competitive_event_runtime_to_match_launch_identity_v1,
    checkout_competitive_event_gameplay_session_v1,
    competitive_gesture_game_session_action_authorities_v1,
    confirm_pending_competitive_event_lifecycle_control_v1,
    confirm_pending_competitive_player_visible_gameplay_primitive_v1,
    execute_prepared_competitive_event_lifecycle_control_v1,
    execute_prepared_competitive_player_visible_gameplay_primitive_v1,
    measure_competitive_event_runtime_sideboard_v1, next_competitive_event_driver_directive_v1,
    prepare_competitive_event_runtime_lifecycle_control_v1,
    ratify_competitive_event_match_launch_with_visible_identity_attended_v1,
    release_confirmed_competitive_player_visible_gameplay_primitive_v1,
    release_confirmed_direct_visible_input_pending_v1,
    return_competitive_event_gameplay_session_v1, MtgoCompetitiveEventDriverDirectiveV1,
    MtgoCompetitiveEventDriverStepV1, MtgoCompetitiveEventGameplayLeaseCommitmentsV1,
    MtgoCompetitiveEventMatchLaunchBindingCommitmentsV1, MtgoCompetitiveEventRuntimeCommitmentsV1,
    MtgoCompetitiveGestureGameSessionCommitmentsV1,
    MtgoPendingCompetitiveEventLifecycleControlCommitmentsV1,
    MtgoPreparedCompetitiveEventLifecycleControlCommitmentsV1,
    OpaqueMtgoCompetitiveEventGameplayLeaseV1, OpaqueMtgoCompetitiveEventMatchLaunchBindingV1,
    OpaqueMtgoCompetitiveEventRuntimeV1, OpaqueMtgoCompetitiveGestureGameSessionV1,
    OpaqueMtgoCompetitiveNativePregameRequestV1, OpaqueMtgoCompetitiveNativeSideboardRequestV1,
    OpaqueMtgoPendingCompetitiveEventLifecycleControlV1,
    OpaqueMtgoPendingCompetitivePlayerVisibleGameplayPrimitiveV1,
    OpaqueMtgoPreparedCompetitiveEventLifecycleControlV1,
    OpaqueMtgoSessionBoundCompetitiveDuelGestureV1, RatifiedMtgoCompetitiveMatchLaunchV1,
};
use crate::competitive_auxiliary_action_resolution::{
    resolve_checked_untrusted_competitive_native_pregame_selection_v1,
    resolve_checked_untrusted_competitive_native_sideboard_selection_v1,
    CheckedUntrustedMtgoCompetitivePregameSemanticResolutionV1,
    CheckedUntrustedMtgoCompetitiveSideboardSemanticResolutionV1,
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
use crate::competitive_visible_match_memory::MtgoCompetitiveExternalPublicHistoryConsumerV1;
use crate::competitive_visible_match_memory::OpaqueMtgoCompetitiveCompletedMatchHistoryV1;
use crate::probe::{
    advance_opaque_player_visible_duel_gesture_target_v1,
    begin_competitive_match_visible_game_log_action_baseline_v1,
    begin_competitive_visible_game_log_baseline_v1, begin_evaluated_competitive_event_monitor_v1,
    begin_opaque_competitive_duel_gesture_sequence_from_pinned_runtime_v1,
    bind_competitive_match_visible_game_log_lease_v1,
    bind_opaque_player_visible_duel_gesture_intent_v1,
    bind_opaque_player_visible_duel_source_gesture_target_v1,
    capture_admitted_mtgo_duel_visible_frame_v1, confirm_attested_direct_visible_dispatch_v1,
    corroborate_competitive_match_visible_game_log_action_v1,
    execute_attested_direct_visible_selection_v1, frame_id_from_capture_commitment_v1,
    perceive_admitted_duel_frame_v1,
    prepare_attested_direct_visible_competitive_before_dispatch_v1,
    prepare_opaque_competitive_duel_action_plan_v1,
    prepare_opaque_player_visible_duel_gesture_pointer_v1,
    prepare_opaque_player_visible_gameplay_before_input_v1,
    rebind_opaque_player_visible_duel_gesture_target_v1,
    refresh_competitive_match_visible_game_log_v1, resolve_opaque_profile_bound_duel_control_v1,
    score_and_select_opaque_admitted_duel_perception_with_loaded_deployment_v1,
    score_select_and_resolve_opaque_player_visible_duel_perception_with_ongoing_history_v1,
    AdmittedMtgoPlayerVisibleDuelGestureTargetProtocolV1,
    AdmittedMtgoPlayerVisibleGameplayPostconditionProtocolV1,
    MtgoAttestedDirectVisibleBeforeDispatchRegionSetV1, MtgoDuelPerceptionFrameIdentityV1,
    MtgoDxgiCaptureRequestV3, MtgoPrivatePlayerVisibleGameplayBeforeInputContextV1,
    OpaqueMtgoAdmittedDuelPerceptionV1, OpaqueMtgoAttestedDirectVisibleCompetitiveBeforeDispatchV1,
    OpaqueMtgoClassifiedCompetitiveEventRecordV1, OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    OpaqueMtgoClassifiedCompetitivePregameModelContextV1, OpaqueMtgoCompetitiveLaunchIdentityV1,
    OpaqueMtgoCompetitiveMatchVisibleGameLogLeaseV1,
    OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    OpaqueMtgoCompetitiveVisibleGameLogBaselineV1,
    OpaqueMtgoPendingAttestedDirectVisibleDispatchV1, OpaqueMtgoPlayerVisibleDuelGestureIntentV1,
    OpaqueMtgoPlayerVisibleDuelGestureTargetBindingV1,
    OpaqueMtgoPlayerVisibleDuelResolvedControlV1,
    OpaqueMtgoPreparedPlayerVisibleDuelGesturePointerV1,
    OpaqueMtgoPreparedPlayerVisibleGameplayBeforeInputV1,
    OpaqueMtgoProfileBoundDuelResolvedControlV1,
    OpaqueMtgoRefreshedAttestedDirectVisibleSelectionV1,
    OpaqueMtgoVerifiedDirectVisibleDispatchRuntimeV1,
};
use mtgo_blackbox_v1::{
    append_checked_untrusted_competitive_player_visible_game_history_from_direct_visible_postcondition_v1,
    append_checked_untrusted_competitive_player_visible_game_history_from_player_visible_postcondition_v1,
    begin_checked_untrusted_competitive_player_visible_game_history_from_direct_visible_postcondition_v1,
    begin_checked_untrusted_competitive_player_visible_game_history_from_player_visible_postcondition_v1,
    validate_competitive_player_visible_game_history_for_session_v1,
    validate_native_checkpoint_competitive_capabilities_v1,
    CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
    CheckedUntrustedMtgoPlayerVisibleDuelGesturePlanV1,
    CheckedUntrustedMtgoPlayerVisibleGameLogActionBaselineV1, MtgoCompetitiveEventKindV1,
    MtgoCompetitiveLifecycleActionV1, MtgoCompetitiveLifecyclePhaseV1, MtgoDuelGestureStageV1,
    MtgoNativeCheckpointCompetitiveCapabilitiesV1, MtgoObservedCompetitiveLifecycleAdvanceV1,
    MtgoPlayerVisibleConfirmedDuelDecisionV1, MtgoPlayerVisibleDuelActionV1,
    MtgoPlayerVisibleDuelGesturePrimitiveV1, MtgoPlayerVisibleDuelScorerV1,
    MtgoProfileBoundPostconditionCalibrationV1, MtgoProfileBoundPostconditionRegionSetV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const COMPETITIVE_POST_ENTRY_OPERATOR_DOMAIN_V1: &[u8] = b"mtgo-competitive-post-entry-operator-v1";
const COMPETITIVE_POST_ENTRY_OPERATOR_ADVANCE_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-post-entry-operator-advance-v1";
const COMPETITIVE_OPERATOR_PREGAME_RESOLUTION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-operator-pregame-resolution-v1";
const COMPETITIVE_OPERATOR_SIDEBOARD_RESOLUTION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-operator-sideboard-resolution-v1";
const COMPETITIVE_OPERATOR_PLAYER_VISIBLE_PRIMITIVE_CONFIRMATION_CHAIN_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-operator-player-visible-primitive-confirmation-chain-v1";
const COMPETITIVE_OPERATOR_DIRECT_VISIBLE_BEFORE_DISPATCH_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-operator-direct-visible-before-dispatch-v1";

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
    pub visible_game_log_baseline_commitment_sha256: Option<String>,
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
    visible_game_log_baseline: Option<OpaqueMtgoCompetitiveVisibleGameLogBaselineV1>,
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

    pub fn visible_game_log_baseline_present_v1(&self) -> bool {
        self.visible_game_log_baseline.is_some()
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
    visible_game_log_baseline: Option<OpaqueMtgoCompetitiveVisibleGameLogBaselineV1>,
    prior_operator: MtgoCompetitivePostEntryOperatorCommitmentsV1,
}

pub struct OpaqueMtgoPendingCompetitiveOperatorLifecycleV1 {
    resources: MtgoCompetitiveOperatorResourcesPartsV1,
    resource_commitments: MtgoCompetitiveOperatorResourceCommitmentsV1,
    pending: OpaqueMtgoPendingCompetitiveEventLifecycleControlV1,
    visible_game_log_baseline: Option<OpaqueMtgoCompetitiveVisibleGameLogBaselineV1>,
    prior_operator: MtgoCompetitivePostEntryOperatorCommitmentsV1,
}

/// Move-only bridge retaining the operator and the exact pre-pairing Game Log
/// baseline while one visible duel identity undergoes attended review.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorMatchLaunchBindingV1;
/// let _forged = OpaqueMtgoCompetitiveOperatorMatchLaunchBindingV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorMatchLaunchBindingV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveOperatorMatchLaunchBindingV1>();
/// ```
pub struct OpaqueMtgoCompetitiveOperatorMatchLaunchBindingV1 {
    resources: MtgoCompetitiveOperatorResourcesPartsV1,
    resource_commitments: MtgoCompetitiveOperatorResourceCommitmentsV1,
    binding: OpaqueMtgoCompetitiveEventMatchLaunchBindingV1,
    visible_game_log_baseline: OpaqueMtgoCompetitiveVisibleGameLogBaselineV1,
    binding_commitments: MtgoCompetitiveEventMatchLaunchBindingCommitmentsV1,
    prior_operator: MtgoCompetitivePostEntryOperatorCommitmentsV1,
}

impl OpaqueMtgoCompetitiveOperatorMatchLaunchBindingV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveEventMatchLaunchBindingCommitmentsV1 {
        self.binding_commitments.clone()
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

/// Move-only attended launch result that retains the exact visible identity
/// and its pre-pairing Game Log baseline for the later direct visible-log
/// lease binder.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorAttendedMatchLaunchV1;
/// let _forged = OpaqueMtgoCompetitiveOperatorAttendedMatchLaunchV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorAttendedMatchLaunchV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveOperatorAttendedMatchLaunchV1>();
/// ```
pub struct OpaqueMtgoCompetitiveOperatorAttendedMatchLaunchV1 {
    resources: MtgoCompetitiveOperatorResourcesPartsV1,
    resource_commitments: MtgoCompetitiveOperatorResourceCommitmentsV1,
    runtime: OpaqueMtgoCompetitiveEventRuntimeV1,
    match_launch: RatifiedMtgoCompetitiveMatchLaunchV1,
    visible_identity: OpaqueMtgoCompetitiveLaunchIdentityV1,
    visible_game_log_baseline: OpaqueMtgoCompetitiveVisibleGameLogBaselineV1,
    prior_operator: MtgoCompetitivePostEntryOperatorCommitmentsV1,
}

impl OpaqueMtgoCompetitiveOperatorAttendedMatchLaunchV1 {
    pub fn visible_game_log_baseline_commitment_sha256_v1(&self) -> &str {
        self.visible_game_log_baseline
            .baseline_commitment_sha256_v1()
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

pub struct OpaqueMtgoCompetitiveOperatorGameplayLeaseV1 {
    resources: MtgoCompetitiveOperatorResourcesPartsV1,
    resource_commitments: MtgoCompetitiveOperatorResourceCommitmentsV1,
    lease: OpaqueMtgoCompetitiveEventGameplayLeaseV1,
    prior_operator: MtgoCompetitivePostEntryOperatorCommitmentsV1,
}

/// Move-only ownership join for one direct client action selected solely from
/// the player-visible model boundary. It withholds the exact event lease, game
/// session, launch identity, public Game Log, prior confirmed history, and
/// checked-untrusted direct source until an opaque live-source producer and
/// dispatch executor are both present.
///
/// This is deliberately not an input capability. It exposes only a composite
/// commitment and the already sanitized selected player-visible action.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorDirectVisibleBeforeDispatchV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveOperatorDirectVisibleBeforeDispatchV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorDirectVisibleBeforeDispatchV1;
/// fn cannot_act(value: OpaqueMtgoCompetitiveOperatorDirectVisibleBeforeDispatchV1) {
///     let _ = value.dispatch();
///     let _ = value.client_action();
///     let _ = value.process_handle();
/// }
/// ```
pub struct OpaqueMtgoCompetitiveOperatorDirectVisibleBeforeDispatchV1 {
    _lease: OpaqueMtgoCompetitiveOperatorGameplayLeaseV1,
    _session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    _visible_identity: OpaqueMtgoCompetitiveLaunchIdentityV1,
    _visible_game_log: OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    _confirmed_history: Option<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1>,
    _direct: OpaqueMtgoAttestedDirectVisibleCompetitiveBeforeDispatchV1,
    selected_action: MtgoPlayerVisibleDuelActionV1,
    operator_binding_commitment_sha256: String,
}

impl OpaqueMtgoCompetitiveOperatorDirectVisibleBeforeDispatchV1 {
    pub fn selected_action_v1(&self) -> &MtgoPlayerVisibleDuelActionV1 {
        &self.selected_action
    }

    pub fn operator_binding_commitment_sha256_v1(&self) -> &str {
        &self.operator_binding_commitment_sha256
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

/// Operator ownership after the sealed client producer submitted one exact
/// selected visible action. The process-wide input gate remains closed until
/// a newer player-visible frame confirms the declared transition.
pub struct OpaqueMtgoCompetitiveOperatorDirectVisiblePendingV1 {
    lease: OpaqueMtgoCompetitiveOperatorGameplayLeaseV1,
    session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    visible_identity: OpaqueMtgoCompetitiveLaunchIdentityV1,
    visible_game_log: OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    confirmed_history: Option<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1>,
    pending: OpaqueMtgoPendingAttestedDirectVisibleDispatchV1,
    selected_action: MtgoPlayerVisibleDuelActionV1,
}

impl OpaqueMtgoCompetitiveOperatorDirectVisiblePendingV1 {
    pub fn selected_action_v1(&self) -> &MtgoPlayerVisibleDuelActionV1 {
        &self.selected_action
    }

    pub fn safe_for_next_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Move-only player-visible gameplay selection for one exact League or
/// Challenge game. It retains the event lease, exact-game gesture session,
/// match-bound public Game Log, optional prior confirmed decisions, and the
/// privately resolved current control. The scorer receives only the
/// player-visible state, actions, and public-history callbacks.
///
/// This value deliberately has no input conversion. A later binding must
/// consume it into the separately reviewed player-visible gesture actuator and
/// must return the lease and history only after a newer visible postcondition.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplaySelectionV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplaySelectionV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplaySelectionV1;
/// fn cannot_act(value: OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplaySelectionV1) {
///     let _ = value.input_command();
///     let _ = value.observation();
///     let _ = value.selected_semantic();
/// }
/// ```
pub struct OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplaySelectionV1 {
    _lease: OpaqueMtgoCompetitiveOperatorGameplayLeaseV1,
    _session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    _visible_identity: OpaqueMtgoCompetitiveLaunchIdentityV1,
    _visible_game_log: OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    confirmed_history: Option<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1>,
    _control: OpaqueMtgoPlayerVisibleDuelResolvedControlV1,
    selected_action: MtgoPlayerVisibleDuelActionV1,
}

impl OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplaySelectionV1 {
    pub fn selected_action_v1(&self) -> &MtgoPlayerVisibleDuelActionV1 {
        &self.selected_action
    }

    pub fn prior_confirmed_action_count_v1(&self) -> usize {
        self.confirmed_history
            .as_ref()
            .map(CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1::decision_count_v1)
            .unwrap_or(0)
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

/// Move-only operator ownership after the selected player-visible action is
/// joined to a complete coordinate-free player-visible gesture. It retains
/// the exact event lease, game session, Game Log snapshot, confirmed history,
/// and private current-frame control. No coordinates or input method cross
/// this boundary.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayGestureV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayGestureV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayGestureV1;
/// fn cannot_act(value: OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayGestureV1) {
///     let _ = value.input_command();
///     let _ = value.coordinates();
///     let _ = value.event_session();
/// }
/// ```
pub struct OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayGestureV1 {
    _lease: OpaqueMtgoCompetitiveOperatorGameplayLeaseV1,
    _session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    _visible_identity: OpaqueMtgoCompetitiveLaunchIdentityV1,
    _visible_game_log: OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    confirmed_history: Option<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1>,
    gesture: OpaqueMtgoPlayerVisibleDuelGestureIntentV1,
    selected_action: MtgoPlayerVisibleDuelActionV1,
}

impl OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayGestureV1 {
    pub fn selected_action_v1(&self) -> &MtgoPlayerVisibleDuelActionV1 {
        &self.selected_action
    }

    pub fn primitives_v1(&self) -> &[MtgoPlayerVisibleDuelGesturePrimitiveV1] {
        self.gesture.primitives_v1()
    }

    pub fn prior_confirmed_action_count_v1(&self) -> usize {
        self.confirmed_history
            .as_ref()
            .map(CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1::decision_count_v1)
            .unwrap_or(0)
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

/// Move-only operator ownership after the current visible gesture primitive
/// is bound to exact retained-frame pixels by the separately reviewed target
/// classifier. Target rectangles, desktop points, pixels, and input methods
/// remain private.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayTargetV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayTargetV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayTargetV1;
/// fn cannot_act(value: OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayTargetV1) {
///     let _ = value.input_command();
///     let _ = value.rect_client_px();
///     let _ = value.points_desktop_px();
/// }
/// ```
pub struct OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayTargetV1 {
    _lease: OpaqueMtgoCompetitiveOperatorGameplayLeaseV1,
    _session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    _visible_identity: OpaqueMtgoCompetitiveLaunchIdentityV1,
    _visible_game_log: OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    confirmed_history: Option<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1>,
    target: OpaqueMtgoPlayerVisibleDuelGestureTargetBindingV1,
    selected_action: MtgoPlayerVisibleDuelActionV1,
    confirmed_prior_primitive_count: u16,
    prior_primitive_confirmation_chain_sha256: Option<String>,
}

impl OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayTargetV1 {
    pub fn selected_action_v1(&self) -> &MtgoPlayerVisibleDuelActionV1 {
        &self.selected_action
    }

    pub fn primitive_index_v1(&self) -> u16 {
        self.target.primitive_index_v1()
    }

    pub fn primitive_v1(&self) -> &MtgoPlayerVisibleDuelGesturePrimitiveV1 {
        self.target.primitive_v1()
    }

    pub fn prior_confirmed_action_count_v1(&self) -> usize {
        self.confirmed_history
            .as_ref()
            .map(CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1::decision_count_v1)
            .unwrap_or(0)
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

/// Move-only pre-input holder created only after refreshing the exact bound
/// visible Game Log, capturing and classifying a newer exact duel frame, and
/// rebinding the same selected gesture primitive to that frame's pixels. The
/// optional action baseline is created only for Game Log action families that
/// the visible parser can corroborate exactly. This type still has no input
/// conversion.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayFreshTargetV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayFreshTargetV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayFreshTargetV1;
/// fn cannot_act(value: OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayFreshTargetV1) {
///     let _ = value.input_command();
///     let _ = value.rect_client_px();
///     let _ = value.points_desktop_px();
///     let _ = value.game_log_baseline();
/// }
/// ```
pub struct OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayFreshTargetV1 {
    _lease: OpaqueMtgoCompetitiveOperatorGameplayLeaseV1,
    _session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    _visible_identity: OpaqueMtgoCompetitiveLaunchIdentityV1,
    _visible_game_log: OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    confirmed_history: Option<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1>,
    _action_baseline: Option<CheckedUntrustedMtgoPlayerVisibleGameLogActionBaselineV1>,
    _confirmed_decision: MtgoPlayerVisibleConfirmedDuelDecisionV1,
    target: OpaqueMtgoPlayerVisibleDuelGestureTargetBindingV1,
    selected_action: MtgoPlayerVisibleDuelActionV1,
    confirmed_prior_primitive_count: u16,
    prior_primitive_confirmation_chain_sha256: Option<String>,
}

impl OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayFreshTargetV1 {
    pub fn selected_action_v1(&self) -> &MtgoPlayerVisibleDuelActionV1 {
        &self.selected_action
    }

    pub fn primitive_index_v1(&self) -> u16 {
        self.target.primitive_index_v1()
    }

    pub fn primitive_v1(&self) -> &MtgoPlayerVisibleDuelGesturePrimitiveV1 {
        self.target.primitive_v1()
    }

    pub fn prior_confirmed_action_count_v1(&self) -> usize {
        self.confirmed_history
            .as_ref()
            .map(CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1::decision_count_v1)
            .unwrap_or(0)
    }

    pub fn visible_game_log_corroboration_available_v1(&self) -> bool {
        self._action_baseline.is_some()
    }

    pub fn is_final_primitive_v1(&self) -> bool {
        self.target.is_final_primitive_v1()
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

/// Move-only, non-actuating operator holder after the exact reviewed target
/// regions have been converted privately to current desktop points. It retains
/// the complete player-visible decision, Game Log, session, and target-runtime
/// lineages but exposes no point, rectangle, pixel, process, or input method.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayPreparedPointerV1;
/// fn cannot_act(value: OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayPreparedPointerV1) {
///     let _ = value.points_desktop_px();
///     let _ = value.process_handle();
///     let _ = value.input_command();
/// }
/// ```
pub struct OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayPreparedPointerV1 {
    _lease: OpaqueMtgoCompetitiveOperatorGameplayLeaseV1,
    _session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    _visible_identity: OpaqueMtgoCompetitiveLaunchIdentityV1,
    _visible_game_log: OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    confirmed_history: Option<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1>,
    _action_baseline: Option<CheckedUntrustedMtgoPlayerVisibleGameLogActionBaselineV1>,
    _confirmed_decision: MtgoPlayerVisibleConfirmedDuelDecisionV1,
    _pointer: OpaqueMtgoPreparedPlayerVisibleDuelGesturePointerV1,
    selected_action: MtgoPlayerVisibleDuelActionV1,
    primitive: MtgoPlayerVisibleDuelGesturePrimitiveV1,
    primitive_index: u16,
    is_final_primitive: bool,
    confirmed_prior_primitive_count: u16,
    prior_primitive_confirmation_chain_sha256: Option<String>,
}

impl OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayPreparedPointerV1 {
    pub fn selected_action_v1(&self) -> &MtgoPlayerVisibleDuelActionV1 {
        &self.selected_action
    }

    pub fn primitive_index_v1(&self) -> u16 {
        self.primitive_index
    }

    pub fn primitive_v1(&self) -> &MtgoPlayerVisibleDuelGesturePrimitiveV1 {
        &self.primitive
    }

    pub fn is_final_primitive_v1(&self) -> bool {
        self.is_final_primitive
    }

    pub fn prior_confirmed_action_count_v1(&self) -> usize {
        self.confirmed_history
            .as_ref()
            .map(CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1::decision_count_v1)
            .unwrap_or(0)
    }

    pub fn visible_game_log_corroboration_available_v1(&self) -> bool {
        self._action_baseline.is_some()
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

/// Operator ownership after the exact retained before-input pixels have been
/// classified into a complete action-specific visible region plan. The plan,
/// pixels, rectangles, points, process identity, Game Log baseline, and event
/// session remain sealed. This type still cannot emit input.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayBeforeInputV1;
/// fn cannot_act(value: OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayBeforeInputV1) {
///     let _ = value.regions();
///     let _ = value.points_desktop_px();
///     let _ = value.input_command();
/// }
/// ```
pub struct OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayBeforeInputV1 {
    _lease: OpaqueMtgoCompetitiveOperatorGameplayLeaseV1,
    _session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    _visible_identity: OpaqueMtgoCompetitiveLaunchIdentityV1,
    _visible_game_log: OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    confirmed_history: Option<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1>,
    _action_baseline: Option<CheckedUntrustedMtgoPlayerVisibleGameLogActionBaselineV1>,
    _confirmed_decision: MtgoPlayerVisibleConfirmedDuelDecisionV1,
    _before_input: OpaqueMtgoPreparedPlayerVisibleGameplayBeforeInputV1,
    selected_action: MtgoPlayerVisibleDuelActionV1,
    primitive: MtgoPlayerVisibleDuelGesturePrimitiveV1,
    primitive_index: u16,
    is_final_primitive: bool,
    confirmed_prior_primitive_count: u16,
    prior_primitive_confirmation_chain_sha256: Option<String>,
}

impl OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayBeforeInputV1 {
    pub fn selected_action_v1(&self) -> &MtgoPlayerVisibleDuelActionV1 {
        &self.selected_action
    }

    pub fn primitive_v1(&self) -> &MtgoPlayerVisibleDuelGesturePrimitiveV1 {
        &self.primitive
    }

    pub fn primitive_index_v1(&self) -> u16 {
        self.primitive_index
    }

    pub fn is_final_primitive_v1(&self) -> bool {
        self.is_final_primitive
    }

    pub fn prior_confirmed_action_count_v1(&self) -> usize {
        self.confirmed_history
            .as_ref()
            .map(CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1::decision_count_v1)
            .unwrap_or(0)
    }

    pub fn before_input_commitment_sha256_v1(&self) -> &str {
        self._before_input.before_input_commitment_sha256_v1()
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

/// Operator ownership after one player-visible primitive was emitted. The
/// shared process gate is locked and this type can only proceed through a
/// strictly newer visible recapture and postcondition confirmation.
pub struct OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayPendingV1 {
    lease: OpaqueMtgoCompetitiveOperatorGameplayLeaseV1,
    session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    visible_identity: OpaqueMtgoCompetitiveLaunchIdentityV1,
    visible_game_log: OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    confirmed_history: Option<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1>,
    action_baseline: Option<CheckedUntrustedMtgoPlayerVisibleGameLogActionBaselineV1>,
    pending: OpaqueMtgoPendingCompetitivePlayerVisibleGameplayPrimitiveV1,
    selected_action: MtgoPlayerVisibleDuelActionV1,
    primitive_index: u16,
    is_final_primitive: bool,
    confirmed_prior_primitive_count: u16,
    prior_primitive_confirmation_chain_sha256: Option<String>,
}

impl OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayPendingV1 {
    pub fn selected_action_v1(&self) -> &MtgoPlayerVisibleDuelActionV1 {
        &self.selected_action
    }

    pub fn primitive_index_v1(&self) -> u16 {
        self.primitive_index
    }

    pub fn is_final_primitive_v1(&self) -> bool {
        self.is_final_primitive
    }

    pub fn safe_for_next_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Final action confirmation returned only after the exact input receipt was
/// joined to a newer visible transition. It retains the updated exact-game
/// session and player-visible decision history for the next model decision.
pub struct OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayConfirmedV1 {
    lease: OpaqueMtgoCompetitiveOperatorGameplayLeaseV1,
    session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    visible_identity: OpaqueMtgoCompetitiveLaunchIdentityV1,
    visible_game_log: OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    confirmed_history: CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
    confirmation_commitment_sha256: String,
}

impl OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayConfirmedV1 {
    pub fn confirmed_action_count_v1(&self) -> usize {
        self.confirmed_history.decision_count_v1()
    }

    pub fn confirmation_commitment_sha256_v1(&self) -> &str {
        &self.confirmation_commitment_sha256
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

/// Scores the next player-visible decision after one final action confirmation.
/// The caller supplies a newer admitted perception from the same exact game.
pub fn select_next_competitive_post_entry_operator_player_visible_gameplay_action_v1<S>(
    confirmed: OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayConfirmedV1,
    perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    scorer: &mut S,
) -> Result<OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplaySelectionV1, String>
where
    S: MtgoPlayerVisibleDuelScorerV1 + MtgoCompetitiveExternalPublicHistoryConsumerV1<Output = ()>,
{
    let OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayConfirmedV1 {
        lease,
        session,
        visible_identity,
        visible_game_log,
        confirmed_history,
        confirmation_commitment_sha256: _,
    } = confirmed;
    select_competitive_post_entry_operator_player_visible_gameplay_action_v1(
        lease,
        session,
        visible_identity,
        visible_game_log,
        Some(confirmed_history),
        perception,
        scorer,
    )
}

/// Returns the gameplay lease after a confirmed action when the visible
/// lifecycle indicates the game has ended. This does not enter another event
/// or spend resources.
pub fn return_confirmed_competitive_post_entry_operator_player_visible_gameplay_v1(
    confirmed: OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayConfirmedV1,
) -> Result<OpaqueMtgoCompetitivePostEntryOperatorV1, String> {
    let OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayConfirmedV1 {
        lease,
        session,
        visible_identity: _,
        visible_game_log: _,
        confirmed_history: _,
        confirmation_commitment_sha256: _,
    } = confirmed;
    return_competitive_post_entry_operator_gameplay_v1(lease, session)
}

pub enum MtgoCompetitiveOperatorPlayerVisibleGameplayConfirmationV1 {
    ContinueGesture(Box<OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayTargetV1>),
    ActionConfirmed(Box<OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayConfirmedV1>),
}

/// Move-only ownership of the gameplay lease and attended game session after
/// the exact loaded checkpoint selected one current player-visible legal
/// action. Only the sanitized player-visible action is exposed so the reviewed
/// UI adapter can build its coordinate-free gesture stages.
// Dormant until the kernel exposes a player-visible-only scorer.
#[allow(dead_code)]
pub(crate) struct OpaqueMtgoCompetitiveOperatorGameplaySelectionV1 {
    lease: OpaqueMtgoCompetitiveOperatorGameplayLeaseV1,
    session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    control: OpaqueMtgoProfileBoundDuelResolvedControlV1,
    player_visible_selected_action: MtgoPlayerVisibleDuelActionV1,
}

#[allow(dead_code)]
impl OpaqueMtgoCompetitiveOperatorGameplaySelectionV1 {
    pub(crate) fn player_visible_selected_action_v1(&self) -> &MtgoPlayerVisibleDuelActionV1 {
        &self.player_visible_selected_action
    }

    pub(crate) fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub(crate) fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub(crate) fn permits_spending_v1(&self) -> bool {
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

/// Move-only real operator pregame request that additionally retains the
/// exact visible launch identity and pre-pairing Game Log baseline. It exposes
/// only the same seated-player-visible model input as the ordinary request.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorAttendedNativePregameRequestV1;
/// let _forged = OpaqueMtgoCompetitiveOperatorAttendedNativePregameRequestV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveOperatorAttendedNativePregameRequestV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveOperatorAttendedNativePregameRequestV1>();
/// ```
pub struct OpaqueMtgoCompetitiveOperatorAttendedNativePregameRequestV1 {
    request: OpaqueMtgoCompetitiveOperatorNativePregameRequestV1,
    visible_identity: OpaqueMtgoCompetitiveLaunchIdentityV1,
    visible_game_log: OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1,
    visible_game_log_lease_commitment_sha256: String,
}

enum OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1 {
    Lease(Box<OpaqueMtgoCompetitiveMatchVisibleGameLogLeaseV1>),
    Snapshot(Box<OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1>),
}

impl OpaqueMtgoCompetitiveOperatorAttendedNativePregameRequestV1 {
    pub fn model_input_v1(&self) -> &crate::MtgoCompetitiveNativePregameModelInputV1 {
        self.request.model_input_v1()
    }

    pub fn model_input_commitment_sha256_v1(&self) -> &str {
        self.request.model_input_commitment_sha256_v1()
    }

    pub fn deployment_commitment_sha256_v1(&self) -> &str {
        self.request.deployment_commitment_sha256_v1()
    }

    pub fn visible_game_log_lease_commitment_sha256_v1(&self) -> &str {
        &self.visible_game_log_lease_commitment_sha256
    }

    pub fn visible_launch_identity_commitment_sha256_v1(&self) -> String {
        self.visible_identity
            .commitments_v1()
            .launch_identity_commitment_sha256
    }

    pub fn visible_game_log_snapshot_present_v1(&self) -> bool {
        matches!(
            self.visible_game_log,
            OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Snapshot(_)
        )
    }

    pub fn visible_game_log_snapshot_commitment_sha256_v1(&self) -> Option<&str> {
        match &self.visible_game_log {
            OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Lease(_) => None,
            OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Snapshot(snapshot) => {
                Some(snapshot.snapshot_commitment_sha256_v1())
            }
        }
    }

    pub fn visible_game_log_event_count_v1(&self) -> Option<usize> {
        match &self.visible_game_log {
            OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Lease(_) => None,
            OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Snapshot(snapshot) => {
                Some(snapshot.event_count_v1())
            }
        }
    }

    pub fn visible_game_log_event_v1(
        &self,
        index: usize,
    ) -> Option<mtgo_blackbox_v1::MtgoVisibleGameLogSemanticEventViewV1<'_>> {
        match &self.visible_game_log {
            OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Lease(_) => None,
            OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Snapshot(snapshot) => {
                snapshot.event_v1(index)
            }
        }
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_session_recovery_v1(&self) -> bool {
        false
    }
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
    resources: MtgoCompetitiveOperatorResourcesPartsV1,
    resource_commitments: MtgoCompetitiveOperatorResourceCommitmentsV1,
    scored_request: OpaqueMtgoScoredCompetitiveNativePregameRequestV1,
    prior_operator: MtgoCompetitivePostEntryOperatorCommitmentsV1,
}

/// Offline checked scorer result that retains the exact attended visible-log
/// source without exposing its transport. The scorer receives only the
/// ordinary player-visible pregame input and cannot recover this source.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoScoredCompetitiveOperatorAttendedNativePregameV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoScoredCompetitiveOperatorAttendedNativePregameV1>();
/// ```
pub struct OpaqueMtgoScoredCompetitiveOperatorAttendedNativePregameV1 {
    scored: OpaqueMtgoScoredCompetitiveOperatorNativePregameV1,
    visible_identity: OpaqueMtgoCompetitiveLaunchIdentityV1,
    visible_game_log: OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1,
    visible_game_log_lease_commitment_sha256: String,
}

/// Move-only proof that the full post-entry ownership chain survived one
/// checked-untrusted pregame score and exact visible semantic resolution.
/// It still cannot recover the event session or reach live input. A future
/// kernel-owned opaque response must replace the checked scorer result before
/// an operator resume path can exist.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoResolvedCompetitiveOperatorNativePregameV1;
/// fn cannot_resume(value: OpaqueMtgoResolvedCompetitiveOperatorNativePregameV1) {
///     let _ = value.into_operator();
///     let _ = value.input_command();
/// }
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoResolvedCompetitiveOperatorNativePregameV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoResolvedCompetitiveOperatorNativePregameV1>();
/// ```
pub struct OpaqueMtgoResolvedCompetitiveOperatorNativePregameV1 {
    _resources: MtgoCompetitiveOperatorResourcesPartsV1,
    _resource_commitments: MtgoCompetitiveOperatorResourceCommitmentsV1,
    _scored_request: OpaqueMtgoScoredCompetitiveNativePregameRequestV1,
    resolution: CheckedUntrustedMtgoCompetitivePregameSemanticResolutionV1,
    operator_resolution_commitment_sha256: String,
    _prior_operator: MtgoCompetitivePostEntryOperatorCommitmentsV1,
}

/// Move-only offline proof that the exact attended visible-log source survives
/// pregame scoring and semantic resolution. It grants no event-session or
/// input authority and exposes only the conservative visible event projection.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoResolvedCompetitiveOperatorAttendedNativePregameV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoResolvedCompetitiveOperatorAttendedNativePregameV1>();
/// ```
pub struct OpaqueMtgoResolvedCompetitiveOperatorAttendedNativePregameV1 {
    resolved: OpaqueMtgoResolvedCompetitiveOperatorNativePregameV1,
    visible_identity: OpaqueMtgoCompetitiveLaunchIdentityV1,
    visible_game_log: OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1,
    visible_game_log_lease_commitment_sha256: String,
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
    resources: MtgoCompetitiveOperatorResourcesDuringSideboardV1,
    resource_commitments: MtgoCompetitiveOperatorResourceCommitmentsV1,
    scored_request: OpaqueMtgoScoredCompetitiveNativeSideboardRequestV1,
    prior_operator: MtgoCompetitivePostEntryOperatorCommitmentsV1,
}

/// Move-only proof that the full post-entry ownership chain survived one
/// checked-untrusted sideboard score and exact manifest-backed semantic
/// resolution. Adapter-local card IDs remain private and no drag, Submit Deck,
/// event-session recovery, or input path is exposed.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoResolvedCompetitiveOperatorNativeSideboardV1;
/// fn cannot_resume(value: OpaqueMtgoResolvedCompetitiveOperatorNativeSideboardV1) {
///     let _ = value.into_operator();
///     let _ = value.submit_sideboard();
/// }
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoResolvedCompetitiveOperatorNativeSideboardV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoResolvedCompetitiveOperatorNativeSideboardV1>();
/// ```
pub struct OpaqueMtgoResolvedCompetitiveOperatorNativeSideboardV1 {
    _resources: MtgoCompetitiveOperatorResourcesDuringSideboardV1,
    _resource_commitments: MtgoCompetitiveOperatorResourceCommitmentsV1,
    _scored_request: OpaqueMtgoScoredCompetitiveNativeSideboardRequestV1,
    resolution: CheckedUntrustedMtgoCompetitiveSideboardSemanticResolutionV1,
    operator_resolution_commitment_sha256: String,
    _prior_operator: MtgoCompetitivePostEntryOperatorCommitmentsV1,
}

impl OpaqueMtgoResolvedCompetitiveOperatorNativePregameV1 {
    pub fn selected_action_v1(&self) -> &crate::MtgoCompetitivePregameSelectedActionV1 {
        self.resolution.selected_action_v1()
    }

    pub fn expected_postcondition_v1(
        &self,
    ) -> &crate::MtgoCompetitivePregameExpectedPostconditionV1 {
        self.resolution.expected_postcondition_v1()
    }

    pub fn semantic_resolution_commitment_sha256_v1(&self) -> &str {
        self.resolution.semantic_resolution_commitment_sha256_v1()
    }

    pub fn operator_resolution_commitment_sha256_v1(&self) -> &str {
        &self.operator_resolution_commitment_sha256
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_session_recovery_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

impl OpaqueMtgoScoredCompetitiveOperatorAttendedNativePregameV1 {
    pub fn selected_action_v1(&self) -> &crate::MtgoCompetitiveNativePregameActionV1 {
        self.scored.scored_request.selected_action_v1()
    }

    pub fn visible_game_log_snapshot_present_v1(&self) -> bool {
        matches!(
            self.visible_game_log,
            OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Snapshot(_)
        )
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_session_recovery_v1(&self) -> bool {
        false
    }
}

impl OpaqueMtgoResolvedCompetitiveOperatorAttendedNativePregameV1 {
    pub fn selected_action_v1(&self) -> &crate::MtgoCompetitivePregameSelectedActionV1 {
        self.resolved.selected_action_v1()
    }

    pub fn expected_postcondition_v1(
        &self,
    ) -> &crate::MtgoCompetitivePregameExpectedPostconditionV1 {
        self.resolved.expected_postcondition_v1()
    }

    pub fn operator_resolution_commitment_sha256_v1(&self) -> &str {
        self.resolved.operator_resolution_commitment_sha256_v1()
    }

    pub fn visible_game_log_lease_commitment_sha256_v1(&self) -> &str {
        &self.visible_game_log_lease_commitment_sha256
    }

    pub fn visible_game_log_snapshot_commitment_sha256_v1(&self) -> Option<&str> {
        match &self.visible_game_log {
            OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Lease(_) => None,
            OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Snapshot(snapshot) => {
                Some(snapshot.snapshot_commitment_sha256_v1())
            }
        }
    }

    pub fn visible_game_log_event_count_v1(&self) -> Option<usize> {
        match &self.visible_game_log {
            OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Lease(_) => None,
            OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Snapshot(snapshot) => {
                Some(snapshot.event_count_v1())
            }
        }
    }

    pub fn visible_game_log_event_v1(
        &self,
        index: usize,
    ) -> Option<mtgo_blackbox_v1::MtgoVisibleGameLogSemanticEventViewV1<'_>> {
        match &self.visible_game_log {
            OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Lease(_) => None,
            OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Snapshot(snapshot) => {
                snapshot.event_v1(index)
            }
        }
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_session_recovery_v1(&self) -> bool {
        false
    }
}

impl OpaqueMtgoResolvedCompetitiveOperatorNativeSideboardV1 {
    pub fn visible_target_configuration_v1(
        &self,
    ) -> &crate::MtgoCompetitiveNativeSideboardConfigurationV1 {
        self.resolution.visible_target_configuration_v1()
    }

    pub fn no_changes_selected_v1(&self) -> bool {
        self.resolution.no_changes_selected_v1()
    }

    pub fn adapter_target_configuration_commitment_sha256_v1(&self) -> &str {
        self.resolution
            .adapter_target_configuration_commitment_sha256_v1()
    }

    pub fn semantic_resolution_commitment_sha256_v1(&self) -> &str {
        self.resolution.semantic_resolution_commitment_sha256_v1()
    }

    pub fn operator_resolution_commitment_sha256_v1(&self) -> &str {
        &self.operator_resolution_commitment_sha256
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

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
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
        None,
        COMPETITIVE_POST_ENTRY_OPERATOR_DOMAIN_V1,
    )?;
    Ok(OpaqueMtgoCompetitivePostEntryOperatorV1 {
        resources: resources.into_parts_v1(),
        resource_commitments,
        runtime,
        visible_game_log_baseline: None,
        commitments,
    })
}

/// Withholds the operator while binding the exact current visible duel
/// identity. The pre-pairing Game Log baseline must already have been captured
/// before Accept Pairing was sent and is consumed into this bridge.
pub fn bind_competitive_post_entry_operator_match_launch_identity_v1(
    operator: OpaqueMtgoCompetitivePostEntryOperatorV1,
    perception: &OpaqueMtgoAdmittedDuelPerceptionV1,
    event_display_label: String,
    event_label_rect_client_px: mtgo_blackbox_v1::MtgoRectPxV1,
) -> Result<OpaqueMtgoCompetitiveOperatorMatchLaunchBindingV1, String> {
    let directive = next_competitive_post_entry_operator_directive_v1(&operator)?;
    let (route_match_identity_sha256, route_game_number) = match directive.route {
        MtgoCompetitivePostEntryOperatorRouteV1::ResolvePregameWithNativeModel {
            match_identity_sha256,
            game_number,
            ..
        } => (match_identity_sha256, game_number),
        _ => {
            return Err(
                "competitive post-entry operator is not at a match launch identity".to_owned(),
            )
        }
    };
    if route_game_number != 1 {
        return Err(
            "competitive post-entry operator v1 supports an initial match launch only".to_owned(),
        );
    }
    let OpaqueMtgoCompetitivePostEntryOperatorV1 {
        resources,
        resource_commitments,
        runtime,
        visible_game_log_baseline,
        commitments,
    } = operator;
    let visible_game_log_baseline = visible_game_log_baseline
        .ok_or("competitive operator match launch lacks its pre-pairing Game Log baseline")?;
    if commitments
        .visible_game_log_baseline_commitment_sha256
        .as_deref()
        != Some(visible_game_log_baseline.baseline_commitment_sha256_v1())
    {
        return Err(
            "competitive operator match launch changed its visible Game Log baseline".to_owned(),
        );
    }
    let binding = bind_competitive_event_runtime_to_match_launch_identity_v1(
        runtime,
        perception,
        &resources.duel_lifecycle_profile,
        event_display_label,
        event_label_rect_client_px,
    )?;
    let binding_commitments = binding.commitments_v1();
    if binding_commitments.match_identity_sha256 != route_match_identity_sha256
        || binding_commitments.game_number != route_game_number
        || binding_commitments.event_kind != commitments.event_kind
        || binding_commitments.event_runtime_commitment_sha256
            != commitments.event_runtime_commitment_sha256
        || resource_commitments.resource_bundle_commitment_sha256
            != commitments.resource_bundle_commitment_sha256
    {
        return Err(
            "competitive operator match launch changed event, match, game, runtime, or resources"
                .to_owned(),
        );
    }
    Ok(OpaqueMtgoCompetitiveOperatorMatchLaunchBindingV1 {
        resources,
        resource_commitments,
        binding,
        visible_game_log_baseline,
        binding_commitments,
        prior_operator: commitments,
    })
}

/// Performs the existing interactive exact-game owner review while retaining
/// the visible identity and Game Log baseline. This sends no MTGO input.
pub fn ratify_competitive_post_entry_operator_match_launch_attended_v1(
    value: OpaqueMtgoCompetitiveOperatorMatchLaunchBindingV1,
    visible_account_alias: &str,
) -> Result<OpaqueMtgoCompetitiveOperatorAttendedMatchLaunchV1, String> {
    let OpaqueMtgoCompetitiveOperatorMatchLaunchBindingV1 {
        resources,
        resource_commitments,
        binding,
        visible_game_log_baseline,
        binding_commitments,
        prior_operator,
    } = value;
    let (runtime, match_launch, visible_identity) =
        ratify_competitive_event_match_launch_with_visible_identity_attended_v1(
            binding,
            visible_account_alias,
        )?;
    if binding_commitments.event_runtime_commitment_sha256
        != prior_operator.event_runtime_commitment_sha256
        || runtime.commitments_v1().runtime_commitment_sha256
            != prior_operator.event_runtime_commitment_sha256
        || resource_commitments.resource_bundle_commitment_sha256
            != prior_operator.resource_bundle_commitment_sha256
    {
        return Err(
            "competitive attended match launch changed operator runtime or resources".to_owned(),
        );
    }
    Ok(OpaqueMtgoCompetitiveOperatorAttendedMatchLaunchV1 {
        resources,
        resource_commitments,
        runtime,
        match_launch,
        visible_identity,
        visible_game_log_baseline,
        prior_operator,
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
    if game_number != 1 {
        return Err(
            "later-game competitive pregame requires the history-preserving v2 checkout".to_owned(),
        );
    }
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
        visible_game_log_baseline: _,
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

/// Game-two and game-three counterpart to the game-one checkout. The complete
/// exact earlier-game visible history moves into the native pregame request and
/// can be consumed only through its sanitized visitor.
pub fn checkout_competitive_post_entry_operator_native_pregame_with_completed_history_v2(
    operator: OpaqueMtgoCompetitivePostEntryOperatorV1,
    match_launch: RatifiedMtgoCompetitiveMatchLaunchV1,
    context: OpaqueMtgoClassifiedCompetitivePregameModelContextV1,
    completed_match_history: OpaqueMtgoCompetitiveCompletedMatchHistoryV1,
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
    if !(2..=3).contains(&game_number) {
        return Err("history-preserving pregame checkout requires game two or three".to_owned());
    }
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
        visible_game_log_baseline: _,
        commitments,
    } = operator;
    let request = bind_competitive_event_pregame_native_request_with_completed_history_v2(
        runtime,
        match_launch,
        context,
        &resources.deck_manifest,
        completed_match_history,
    )?;
    if request.completed_game_count_v1()
        != usize::from(
            game_number
                .checked_sub(1)
                .ok_or("native pregame route game number underflow")?,
        )
    {
        return Err("native pregame request lost the complete earlier-game history".to_owned());
    }
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

/// Consumes one attended launch into the existing exact pregame request while
/// preserving the visible launch identity and pre-pairing Game Log baseline
/// inside the returned opaque request.
pub fn checkout_competitive_post_entry_operator_attended_native_pregame_v1(
    value: OpaqueMtgoCompetitiveOperatorAttendedMatchLaunchV1,
    context: OpaqueMtgoClassifiedCompetitivePregameModelContextV1,
    acting_player_alias: &str,
    visible_game_log_capture_request: MtgoDxgiCaptureRequestV3,
) -> Result<OpaqueMtgoCompetitiveOperatorAttendedNativePregameRequestV1, String> {
    let OpaqueMtgoCompetitiveOperatorAttendedMatchLaunchV1 {
        resources,
        resource_commitments,
        runtime,
        match_launch,
        visible_identity,
        visible_game_log_baseline,
        prior_operator,
    } = value;
    let operator = OpaqueMtgoCompetitivePostEntryOperatorV1 {
        resources,
        resource_commitments,
        runtime,
        visible_game_log_baseline: None,
        commitments: prior_operator,
    };
    let visible_game_log_lease = bind_competitive_match_visible_game_log_lease_v1(
        visible_game_log_baseline,
        &visible_identity,
        acting_player_alias,
        visible_game_log_capture_request,
    )?;
    let visible_game_log_lease_commitment_sha256 = visible_game_log_lease
        .lease_commitment_sha256_v1()
        .to_owned();
    let request = checkout_competitive_post_entry_operator_native_pregame_v1(
        operator,
        match_launch,
        context,
    )?;
    Ok(
        OpaqueMtgoCompetitiveOperatorAttendedNativePregameRequestV1 {
            request,
            visible_identity,
            visible_game_log: OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Lease(Box::new(
                visible_game_log_lease,
            )),
            visible_game_log_lease_commitment_sha256,
        },
    )
}

/// Refreshes the exact match-scoped persisted Game Log while retaining the
/// complete attended pregame request. Only the conservative seated-player
/// semantic projection is exposed. Raw bytes, paths, markup, source IDs, and
/// non-rendered client metadata remain private. Repeated calls consume the
/// prior snapshot and reread the same bound source between exact duel frames.
pub fn refresh_competitive_post_entry_operator_attended_pregame_visible_game_log_v1(
    value: OpaqueMtgoCompetitiveOperatorAttendedNativePregameRequestV1,
    request: MtgoDxgiCaptureRequestV3,
) -> Result<OpaqueMtgoCompetitiveOperatorAttendedNativePregameRequestV1, String> {
    let OpaqueMtgoCompetitiveOperatorAttendedNativePregameRequestV1 {
        request: pregame_request,
        visible_identity,
        visible_game_log,
        visible_game_log_lease_commitment_sha256,
    } = value;
    let lease = match visible_game_log {
        OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Lease(lease) => *lease,
        OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Snapshot(snapshot) => {
            (*snapshot).into_match_lease_v1()
        }
    };
    if lease.lease_commitment_sha256_v1() != visible_game_log_lease_commitment_sha256 {
        return Err(
            "competitive operator visible Game Log lease commitment changed before refresh"
                .to_owned(),
        );
    }
    let snapshot =
        refresh_competitive_match_visible_game_log_v1(lease, &visible_identity, request)?;
    Ok(
        OpaqueMtgoCompetitiveOperatorAttendedNativePregameRequestV1 {
            request: pregame_request,
            visible_identity,
            visible_game_log: OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Snapshot(
                Box::new(snapshot),
            ),
            visible_game_log_lease_commitment_sha256,
        },
    )
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
        resources,
        resource_commitments,
        scored_request,
        prior_operator,
    })
}

/// Exercises the complete attended ownership chain through an offline scorer.
/// Only the visible-only pregame request is scored; the bound Game Log source
/// stays sealed in the returned move-only holder.
pub fn score_checked_untrusted_competitive_operator_attended_native_pregame_v1<
    S: MtgoCompetitiveNativePregameScorerV1,
>(
    value: OpaqueMtgoCompetitiveOperatorAttendedNativePregameRequestV1,
    scorer: &mut S,
) -> Result<OpaqueMtgoScoredCompetitiveOperatorAttendedNativePregameV1, String> {
    let OpaqueMtgoCompetitiveOperatorAttendedNativePregameRequestV1 {
        request,
        visible_identity,
        visible_game_log,
        visible_game_log_lease_commitment_sha256,
    } = value;
    let scored = score_checked_untrusted_competitive_operator_native_pregame_v1(request, scorer)?;
    Ok(OpaqueMtgoScoredCompetitiveOperatorAttendedNativePregameV1 {
        scored,
        visible_identity,
        visible_game_log,
        visible_game_log_lease_commitment_sha256,
    })
}

/// Carries every retained post-entry resource through exact coordinate-free
/// pregame semantic resolution. This is an offline ownership proof only.
pub fn resolve_checked_untrusted_competitive_operator_native_pregame_v1(
    value: OpaqueMtgoScoredCompetitiveOperatorNativePregameV1,
) -> Result<OpaqueMtgoResolvedCompetitiveOperatorNativePregameV1, String> {
    let OpaqueMtgoScoredCompetitiveOperatorNativePregameV1 {
        resources,
        resource_commitments,
        scored_request,
        prior_operator,
    } = value;
    let resolution = resolve_checked_untrusted_competitive_native_pregame_selection_v1(
        scored_request.source_request_v1().model_input_v1(),
        scored_request.checked_selection_v1(),
    )?;
    if resolution.model_input_commitment_sha256_v1()
        != scored_request
            .source_request_v1()
            .model_input_commitment_sha256_v1()
        || resource_commitments.resource_bundle_commitment_sha256
            != prior_operator.resource_bundle_commitment_sha256
    {
        return Err(
            "competitive operator pregame semantic resolution lost exact lineage".to_owned(),
        );
    }
    let operator_resolution_commitment_sha256 = operator_auxiliary_resolution_commitment_v1(
        COMPETITIVE_OPERATOR_PREGAME_RESOLUTION_DOMAIN_V1,
        &resource_commitments.resource_bundle_commitment_sha256,
        &prior_operator.operator_commitment_sha256,
        resolution.semantic_resolution_commitment_sha256_v1(),
        b"checked_untrusted_pregame_resolution_no_session_recovery_no_input",
    )?;
    Ok(OpaqueMtgoResolvedCompetitiveOperatorNativePregameV1 {
        _resources: resources,
        _resource_commitments: resource_commitments,
        _scored_request: scored_request,
        resolution,
        operator_resolution_commitment_sha256,
        _prior_operator: prior_operator,
    })
}

/// Carries the sealed attended visible-log source through exact offline
/// semantic resolution. This proves ownership continuity only and creates no
/// live resume or input path.
pub fn resolve_checked_untrusted_competitive_operator_attended_native_pregame_v1(
    value: OpaqueMtgoScoredCompetitiveOperatorAttendedNativePregameV1,
) -> Result<OpaqueMtgoResolvedCompetitiveOperatorAttendedNativePregameV1, String> {
    let OpaqueMtgoScoredCompetitiveOperatorAttendedNativePregameV1 {
        scored,
        visible_identity,
        visible_game_log,
        visible_game_log_lease_commitment_sha256,
    } = value;
    let resolved = resolve_checked_untrusted_competitive_operator_native_pregame_v1(scored)?;
    Ok(
        OpaqueMtgoResolvedCompetitiveOperatorAttendedNativePregameV1 {
            resolved,
            visible_identity,
            visible_game_log,
            visible_game_log_lease_commitment_sha256,
        },
    )
}

/// Refreshes the exact sealed visible Game Log after offline pregame semantic
/// resolution without changing the selected action or creating a live resume
/// path. The source remains bound to the same attended launch identity.
pub fn refresh_resolved_competitive_operator_attended_pregame_visible_game_log_v1(
    value: OpaqueMtgoResolvedCompetitiveOperatorAttendedNativePregameV1,
    request: MtgoDxgiCaptureRequestV3,
) -> Result<OpaqueMtgoResolvedCompetitiveOperatorAttendedNativePregameV1, String> {
    let OpaqueMtgoResolvedCompetitiveOperatorAttendedNativePregameV1 {
        resolved,
        visible_identity,
        visible_game_log,
        visible_game_log_lease_commitment_sha256,
    } = value;
    let lease = match visible_game_log {
        OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Lease(lease) => *lease,
        OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Snapshot(snapshot) => {
            (*snapshot).into_match_lease_v1()
        }
    };
    if lease.lease_commitment_sha256_v1() != visible_game_log_lease_commitment_sha256 {
        return Err(
            "resolved competitive operator visible Game Log lease changed before refresh"
                .to_owned(),
        );
    }
    let snapshot =
        refresh_competitive_match_visible_game_log_v1(lease, &visible_identity, request)?;
    Ok(
        OpaqueMtgoResolvedCompetitiveOperatorAttendedNativePregameV1 {
            resolved,
            visible_identity,
            visible_game_log: OpaqueMtgoCompetitiveOperatorVisibleGameLogStateV1::Snapshot(
                Box::new(snapshot),
            ),
            visible_game_log_lease_commitment_sha256,
        },
    )
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
    completed_history: OpaqueMtgoCompetitiveCompletedMatchHistoryV1,
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
    let outcome_lineage = completed_history.latest_lineage_v1()?;
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
        visible_game_log_baseline: _,
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
    let request =
        bind_competitive_event_native_sideboard_request_v1(measurement, completed_history)?;
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
        resources,
        resource_commitments,
        scored_request,
        prior_operator,
    })
}

/// Carries every retained post-entry resource through exact manifest-backed
/// sideboard semantic resolution. This remains unable to recover the event
/// session, move a card, or submit the deck.
pub fn resolve_checked_untrusted_competitive_operator_native_sideboard_v1(
    value: OpaqueMtgoScoredCompetitiveOperatorNativeSideboardV1,
) -> Result<OpaqueMtgoResolvedCompetitiveOperatorNativeSideboardV1, String> {
    let OpaqueMtgoScoredCompetitiveOperatorNativeSideboardV1 {
        resources,
        resource_commitments,
        scored_request,
        prior_operator,
    } = value;
    let source_request = scored_request.source_request_v1();
    let resolution = resolve_checked_untrusted_competitive_native_sideboard_selection_v1(
        source_request.model_input_v1(),
        scored_request.checked_selection_v1(),
        source_request.source_manifest_v1(),
        source_request.source_snapshot_commitment_sha256_v1(),
        &prior_operator.policy_deployment_commitment_sha256,
    )?;
    if resolution.model_input_commitment_sha256_v1()
        != source_request.model_input_commitment_sha256_v1()
        || resource_commitments.resource_bundle_commitment_sha256
            != prior_operator.resource_bundle_commitment_sha256
    {
        return Err(
            "competitive operator sideboard semantic resolution lost exact lineage".to_owned(),
        );
    }
    let operator_resolution_commitment_sha256 = operator_auxiliary_resolution_commitment_v1(
        COMPETITIVE_OPERATOR_SIDEBOARD_RESOLUTION_DOMAIN_V1,
        &resource_commitments.resource_bundle_commitment_sha256,
        &prior_operator.operator_commitment_sha256,
        resolution.semantic_resolution_commitment_sha256_v1(),
        b"checked_untrusted_sideboard_resolution_no_session_recovery_no_drag_no_submit",
    )?;
    Ok(OpaqueMtgoResolvedCompetitiveOperatorNativeSideboardV1 {
        _resources: resources,
        _resource_commitments: resource_commitments,
        _scored_request: scored_request,
        resolution,
        operator_resolution_commitment_sha256,
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
        operator.visible_game_log_baseline,
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
    let visible_game_log_baseline = match action {
        MtgoCompetitiveLifecycleActionV1::AcceptPairing => {
            if operator.visible_game_log_baseline.is_some() {
                return Err(
                    "competitive operator already owns a pre-pairing Game Log baseline".to_owned(),
                );
            }
            Some(begin_competitive_visible_game_log_baseline_v1(
                &operator.runtime,
            )?)
        }
        _ => operator.visible_game_log_baseline,
    };
    let prepared =
        prepare_competitive_event_runtime_lifecycle_control_v1(operator.runtime, action)?;
    let prepared_commitments = prepared.commitments_v1();
    validate_prepared_operator_lifecycle_v1(&operator.commitments, action, &prepared_commitments)?;
    Ok(OpaqueMtgoPreparedCompetitiveOperatorLifecycleV1 {
        resources: operator.resources,
        resource_commitments: operator.resource_commitments,
        prepared,
        visible_game_log_baseline,
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
        visible_game_log_baseline: prepared.visible_game_log_baseline,
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
        pending.visible_game_log_baseline,
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
        operator.visible_game_log_baseline,
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

/// Scores and resolves one current competitive duel decision through only the
/// player-visible model contract while retaining the exact ongoing public
/// history and event-session ownership. The caller must refresh the bound Game
/// Log before acquiring `perception`; the scoring bridge enforces that the
/// perception is strictly newer than that refresh.
///
/// The optional confirmed history is absent before the first model action in
/// the game. On later actions it must describe the same event, match, game,
/// deployment, and a strictly older confirmed frame. No live input occurs.
pub fn select_competitive_post_entry_operator_player_visible_gameplay_action_v1<S>(
    lease: OpaqueMtgoCompetitiveOperatorGameplayLeaseV1,
    session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    visible_identity: OpaqueMtgoCompetitiveLaunchIdentityV1,
    visible_game_log: OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    confirmed_history: Option<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1>,
    perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    scorer: &mut S,
) -> Result<OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplaySelectionV1, String>
where
    S: MtgoPlayerVisibleDuelScorerV1 + MtgoCompetitiveExternalPublicHistoryConsumerV1<Output = ()>,
{
    let lease_commitments = lease.lease.commitments_v1();
    let session_commitments = session.commitments_v1();
    let perception_commitments = perception.commitments_v1();
    let deployment_commitment_sha256 = lease
        .resources
        .checkpoint_deployment
        .deployment_commitment_sha256()
        .to_owned();
    validate_operator_gameplay_action_source_v1(
        &lease.resource_commitments,
        &lease_commitments,
        &session_commitments,
        &perception_commitments,
        &deployment_commitment_sha256,
    )?;
    let launch_commitments = visible_identity.commitments_v1();
    if launch_commitments.event_kind != lease_commitments.event_kind
        || launch_commitments.game_number != lease_commitments.game_number
        || visible_identity.event_identity_sha256_v1() != lease_commitments.event_identity_sha256
        || visible_identity.match_identity_sha256_v1() != lease_commitments.match_identity_sha256
    {
        return Err(
            "player-visible launch identity changed the exact event, match, or game".to_owned(),
        );
    }
    if visible_game_log.event_kind_v1() != lease_commitments.event_kind
        || visible_game_log.event_identity_sha256_v1() != lease_commitments.event_identity_sha256
        || visible_game_log.match_identity_sha256_v1() != lease_commitments.match_identity_sha256
        || visible_game_log.game_number_v1() != lease_commitments.game_number
    {
        return Err(
            "player-visible gameplay history changed the exact event, match, or game".to_owned(),
        );
    }
    match &confirmed_history {
        Some(history) => validate_competitive_player_visible_game_history_for_session_v1(
            history,
            &deployment_commitment_sha256,
            session_commitments.confirmed_action_count,
            session_commitments.last_confirmed_frame_sequence,
            perception_commitments.frame_sequence,
        )
        .map_err(|error| format!("validate exact-game confirmed decision history: {error}"))?,
        None if session_commitments.confirmed_action_count == 0 => {}
        None => {
            return Err(
                "player-visible gameplay session has confirmed actions but no decision history"
                    .to_owned(),
            )
        }
    }
    let control =
        score_select_and_resolve_opaque_player_visible_duel_perception_with_ongoing_history_v1(
            &visible_game_log,
            confirmed_history.as_ref(),
            perception,
            &lease.resources.duel_perception_profile,
            &deployment_commitment_sha256,
            scorer,
        )?;
    let selected_action = control.selected_action_v1().clone();
    Ok(
        OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplaySelectionV1 {
            _lease: lease,
            _session: session,
            _visible_identity: visible_identity,
            _visible_game_log: visible_game_log,
            confirmed_history,
            _control: control,
            selected_action,
        },
    )
}

/// Captures and joins one source-attested direct selection to exact competitive
/// operator ownership. The next logical frame sequence is derived from the
/// owned game session and attended launch. Both authorization records also
/// come from that opaque session. The caller cannot assign freshness, swap
/// authorization, or attach an unverified Game Log action baseline. No input
/// occurs.
#[allow(clippy::too_many_arguments)]
pub fn bind_competitive_post_entry_operator_direct_visible_before_dispatch_v1(
    lease: OpaqueMtgoCompetitiveOperatorGameplayLeaseV1,
    session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    visible_identity: OpaqueMtgoCompetitiveLaunchIdentityV1,
    visible_game_log: OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    confirmed_history: Option<CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1>,
    refreshed: OpaqueMtgoRefreshedAttestedDirectVisibleSelectionV1,
    region_set: MtgoAttestedDirectVisibleBeforeDispatchRegionSetV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoCompetitiveOperatorDirectVisibleBeforeDispatchV1, String> {
    let lease_commitments = lease.lease.commitments_v1();
    let session_commitments = session.commitments_v1();
    let launch_commitments = visible_identity.commitments_v1();
    let corroborating_frame_sequence = next_direct_visible_frame_sequence_v1(
        session_commitments.valid_from_frame_sequence,
        session_commitments.valid_through_frame_sequence,
        session_commitments.last_confirmed_frame_sequence,
        launch_commitments.frame_sequence,
    )?;
    let (mode_authorization, gameplay_authorization) =
        competitive_gesture_game_session_action_authorities_v1(&session);
    let direct = prepare_attested_direct_visible_competitive_before_dispatch_v1(
        refreshed,
        &lease.resources.duel_perception_profile,
        &lease.resources.duel_perception_runtime,
        corroborating_frame_sequence,
        region_set,
        &mode_authorization,
        &gameplay_authorization,
        timeout_ms,
    )?;
    let source_attestation_binding_commitment_sha256 =
        direct.binding_commitment_sha256_v1().to_owned();
    let direct_commitments = direct.checked_v1().dispatch_commitments_v1();
    validate_operator_direct_visible_before_dispatch_v1(
        &lease.resource_commitments,
        &lease_commitments,
        &session_commitments,
        &launch_commitments,
        visible_identity.event_identity_sha256_v1(),
        visible_identity.match_identity_sha256_v1(),
        &visible_game_log,
        confirmed_history.as_ref(),
        &direct_commitments,
    )?;
    let selected_action = direct.selected_action_v1().clone();
    let selected_action_json = serde_json::to_vec(&selected_action)
        .map_err(|error| format!("serialize direct visible operator selection: {error}"))?;
    let prior_history_commitment = confirmed_history
        .as_ref()
        .map(
            CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1::history_commitment_sha256_v1,
        )
        .unwrap_or("none");
    let operator_binding_commitment_sha256 = hash_parts_v1(
        COMPETITIVE_OPERATOR_DIRECT_VISIBLE_BEFORE_DISPATCH_DOMAIN_V1,
        &[
            lease_commitments
                .gameplay_lease_commitment_sha256
                .as_bytes(),
            session_commitments.session_commitment_sha256.as_bytes(),
            launch_commitments
                .launch_identity_commitment_sha256
                .as_bytes(),
            visible_game_log.snapshot_commitment_sha256_v1().as_bytes(),
            prior_history_commitment.as_bytes(),
            direct_commitments
                .direct_competitive_scope_commitment_sha256_v1()
                .as_bytes(),
            direct_commitments
                .before_dispatch_commitment_sha256_v1()
                .as_bytes(),
            direct_commitments
                .decision_commitment_sha256_v1()
                .as_bytes(),
            direct_commitments
                .selection_commitment_sha256_v1()
                .as_bytes(),
            direct_commitments.refresh_commitment_sha256_v1().as_bytes(),
            direct_commitments
                .exact_producer_result_sha256_v1()
                .as_bytes(),
            direct_commitments.broker_binary_sha256_v1().as_bytes(),
            direct_commitments.producer_binary_sha256_v1().as_bytes(),
            source_attestation_binding_commitment_sha256.as_bytes(),
            &selected_action_json,
            direct_commitments
                .selected_index_v1()
                .to_be_bytes()
                .as_slice(),
            direct_commitments
                .source_frame_id_v1()
                .to_be_bytes()
                .as_slice(),
            direct_commitments
                .source_frame_sequence_v1()
                .to_be_bytes()
                .as_slice(),
            direct_commitments
                .source_captured_at_unix_millis_v1()
                .to_be_bytes()
                .as_slice(),
            b"operator_ownership_withheld_pending_opaque_live_source_and_dispatch",
        ],
    );
    Ok(OpaqueMtgoCompetitiveOperatorDirectVisibleBeforeDispatchV1 {
        _lease: lease,
        _session: session,
        _visible_identity: visible_identity,
        _visible_game_log: visible_game_log,
        _confirmed_history: confirmed_history,
        _direct: direct,
        selected_action,
        operator_binding_commitment_sha256,
    })
}

/// Attempts the single sealed direct-client action only through the separately
/// pinned dispatch runtime. The production dispatch ratification root is
/// empty, so this currently returns before reserving the input gate or invoking
/// the broker. Event entry and spending are never part of this function.
pub fn execute_competitive_post_entry_operator_direct_visible_action_v1(
    value: OpaqueMtgoCompetitiveOperatorDirectVisibleBeforeDispatchV1,
    runtime: &OpaqueMtgoVerifiedDirectVisibleDispatchRuntimeV1,
    reviewed_dispatch_runtime_commitment_sha256: &str,
    broker_timeout_ms: u32,
) -> Result<OpaqueMtgoCompetitiveOperatorDirectVisiblePendingV1, String> {
    let OpaqueMtgoCompetitiveOperatorDirectVisibleBeforeDispatchV1 {
        _lease: lease,
        _session: session,
        _visible_identity: visible_identity,
        _visible_game_log: visible_game_log,
        _confirmed_history: confirmed_history,
        _direct: direct,
        selected_action,
        operator_binding_commitment_sha256: _,
    } = value;
    let pending = execute_attested_direct_visible_selection_v1(
        direct,
        runtime,
        reviewed_dispatch_runtime_commitment_sha256,
        broker_timeout_ms,
    )?;
    Ok(OpaqueMtgoCompetitiveOperatorDirectVisiblePendingV1 {
        lease,
        session,
        visible_identity,
        visible_game_log,
        confirmed_history,
        pending,
        selected_action,
    })
}

/// Refreshes the visible Game Log and captures a strictly newer composed duel
/// frame. Only after the fixed player-visible regions change does it advance
/// the exact-game session, append sanitized visible history, and reopen the
/// shared input gate.
pub fn confirm_competitive_post_entry_operator_direct_visible_action_v1(
    value: OpaqueMtgoCompetitiveOperatorDirectVisiblePendingV1,
    visible_game_log_capture_request: MtgoDxgiCaptureRequestV3,
    capture_timeout_ms: u32,
) -> Result<OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayConfirmedV1, String> {
    let OpaqueMtgoCompetitiveOperatorDirectVisiblePendingV1 {
        lease,
        session,
        visible_identity,
        visible_game_log,
        confirmed_history,
        pending,
        selected_action: _,
    } = value;
    let receipt = pending.dispatch_receipt_commitment_sha256_v1().to_owned();
    let visible_game_log = refresh_competitive_match_visible_game_log_v1(
        visible_game_log.into_match_lease_v1(),
        &visible_identity,
        visible_game_log_capture_request,
    )?;
    validate_operator_visible_game_log_lineage_v1(&lease, &visible_identity, &visible_game_log)?;
    let confirmed = confirm_attested_direct_visible_dispatch_v1(
        pending,
        &lease.resources.duel_perception_profile,
        capture_timeout_ms,
        visible_game_log.latest_capture_unix_millis_v1(),
    )?;
    let confirmation_commitment_sha256 = confirmed.confirmation_commitment_sha256_v1().to_owned();
    let session = advance_competitive_direct_visible_gameplay_session_v1(session, &confirmed)?;
    let history_id = player_visible_history_id_v1(
        visible_identity.match_identity_sha256_v1(),
        visible_identity.commitments_v1().game_number,
    )?;
    let confirmed_history = match confirmed_history {
        Some(history) => {
            append_checked_untrusted_competitive_player_visible_game_history_from_direct_visible_postcondition_v1(
                history,
                confirmed,
            )
            .map_err(|error| format!("append direct-visible gameplay history: {error}"))?
        }
        None => {
            begin_checked_untrusted_competitive_player_visible_game_history_from_direct_visible_postcondition_v1(
                &history_id,
                confirmed,
            )
            .map_err(|error| format!("begin direct-visible gameplay history: {error}"))?
        }
    };
    release_confirmed_direct_visible_input_pending_v1(&receipt)?;
    Ok(
        OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayConfirmedV1 {
            lease,
            session,
            visible_identity,
            visible_game_log,
            confirmed_history,
            confirmation_commitment_sha256,
        },
    )
}

fn next_direct_visible_frame_sequence_v1(
    valid_from_frame_sequence: u64,
    valid_through_frame_sequence: u64,
    last_confirmed_frame_sequence: u64,
    launch_frame_sequence: u64,
) -> Result<u64, String> {
    if valid_from_frame_sequence == 0
        || valid_from_frame_sequence > valid_through_frame_sequence
        || last_confirmed_frame_sequence >= valid_through_frame_sequence
    {
        return Err("direct visible gameplay has no valid next frame lifetime".to_owned());
    }
    let predecessor = last_confirmed_frame_sequence
        .max(launch_frame_sequence)
        .max(valid_from_frame_sequence - 1)
        .max(1);
    let next = predecessor
        .checked_add(1)
        .ok_or("direct visible next frame sequence overflow")?;
    if next > valid_through_frame_sequence {
        return Err("direct visible next frame exceeds the game session lifetime".to_owned());
    }
    Ok(next)
}

/// Joins the exact operator-owned player-visible selection to a validated
/// coordinate-free gesture plan for the same selected visible action. This is
/// an offline ownership transition only. Fresh target pixels, a fresh pre-input
/// Game Log refresh, authorization, input, and a newer postcondition are all
/// still absent.
pub fn bind_competitive_post_entry_operator_player_visible_gameplay_gesture_v1(
    selection: OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplaySelectionV1,
    gesture: CheckedUntrustedMtgoPlayerVisibleDuelGesturePlanV1,
) -> Result<OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayGestureV1, String> {
    let OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplaySelectionV1 {
        _lease: lease,
        _session: session,
        _visible_identity: visible_identity,
        _visible_game_log: visible_game_log,
        confirmed_history,
        _control: control,
        selected_action,
    } = selection;
    let gesture = bind_opaque_player_visible_duel_gesture_intent_v1(control, gesture)?;
    Ok(
        OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayGestureV1 {
            _lease: lease,
            _session: session,
            _visible_identity: visible_identity,
            _visible_game_log: visible_game_log,
            confirmed_history,
            gesture,
            selected_action,
        },
    )
}

/// Runs the reviewed target classifier against the retained source frame for
/// primitive zero and keeps the resulting pixel-bound target sealed inside the
/// exact operator ownership chain. This remains non-actuating.
pub fn bind_competitive_post_entry_operator_player_visible_gameplay_source_target_v1(
    value: OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayGestureV1,
    protocol: &AdmittedMtgoPlayerVisibleDuelGestureTargetProtocolV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayTargetV1, String> {
    let OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayGestureV1 {
        _lease: lease,
        _session: session,
        _visible_identity: visible_identity,
        _visible_game_log: visible_game_log,
        confirmed_history,
        gesture,
        selected_action,
    } = value;
    let target = bind_opaque_player_visible_duel_source_gesture_target_v1(
        gesture,
        &lease.resources.duel_gesture_profile,
        &lease.resources.duel_gesture_runtime,
        protocol,
        0,
        timeout_ms,
    )?;
    Ok(OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayTargetV1 {
        _lease: lease,
        _session: session,
        _visible_identity: visible_identity,
        _visible_game_log: visible_game_log,
        confirmed_history,
        target,
        selected_action,
        confirmed_prior_primitive_count: 0,
        prior_primitive_confirmation_chain_sha256: None,
    })
}

/// Rebinds the next gesture primitive to one exact newer visible perception.
/// It observes only the continuation target transition and creates no input.
pub fn advance_competitive_post_entry_operator_player_visible_gameplay_target_v1(
    value: OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayTargetV1,
    current_perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    protocol: &AdmittedMtgoPlayerVisibleDuelGestureTargetProtocolV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayTargetV1, String> {
    let OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayTargetV1 {
        _lease: lease,
        _session: session,
        _visible_identity: visible_identity,
        _visible_game_log: visible_game_log,
        confirmed_history,
        target,
        selected_action,
        confirmed_prior_primitive_count,
        prior_primitive_confirmation_chain_sha256,
    } = value;
    let lease_commitments = lease.lease.commitments_v1();
    let session_commitments = session.commitments_v1();
    let current_commitments = current_perception.commitments_v1();
    let deployment_commitment_sha256 = lease
        .resources
        .checkpoint_deployment
        .deployment_commitment_sha256();
    validate_operator_gameplay_action_source_v1(
        &lease.resource_commitments,
        &lease_commitments,
        &session_commitments,
        &current_commitments,
        deployment_commitment_sha256,
    )?;
    let current_lifecycle = current_perception
        .competitive_lifecycle_v1()
        .ok_or("player-visible gesture continuation lacks competitive lifecycle")?;
    if current_lifecycle.event_kind() != lease_commitments.event_kind
        || current_lifecycle.event_identity_sha256_v1()
            != Some(lease_commitments.event_identity_sha256.as_str())
        || current_lifecycle.match_identity_sha256_v1()
            != Some(lease_commitments.match_identity_sha256.as_str())
        || current_lifecycle.game_number_v1() != Some(lease_commitments.game_number)
    {
        return Err(
            "player-visible gesture continuation changed the exact event, match, or game"
                .to_owned(),
        );
    }
    let target = advance_opaque_player_visible_duel_gesture_target_v1(
        target,
        current_perception,
        &lease.resources.duel_gesture_profile,
        &lease.resources.duel_gesture_runtime,
        protocol,
        timeout_ms,
    )?;
    Ok(OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayTargetV1 {
        _lease: lease,
        _session: session,
        _visible_identity: visible_identity,
        _visible_game_log: visible_game_log,
        confirmed_history,
        target,
        selected_action,
        confirmed_prior_primitive_count,
        prior_primitive_confirmation_chain_sha256,
    })
}

/// Refreshes the exact match-scoped visible Game Log, captures and classifies
/// one newer duel frame, revalidates the event/session/history lineage, and
/// rebinds the same current primitive to the newer pixels. The order is fixed
/// so the returned target and optional visible-log action baseline are both
/// newer than the history snapshot used for scoring. No input occurs.
#[allow(clippy::too_many_arguments)]
pub fn refresh_competitive_post_entry_operator_player_visible_gameplay_target_v1(
    value: OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayTargetV1,
    visible_game_log_capture_request: MtgoDxgiCaptureRequestV3,
    fresh_frame_identity: MtgoDuelPerceptionFrameIdentityV1,
    capture_timeout_ms: u32,
    perception_timeout_ms: u32,
    target_timeout_ms: u32,
    protocol: &AdmittedMtgoPlayerVisibleDuelGestureTargetProtocolV1,
) -> Result<OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayFreshTargetV1, String> {
    let OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayTargetV1 {
        _lease: lease,
        _session: session,
        _visible_identity: visible_identity,
        _visible_game_log: visible_game_log,
        confirmed_history,
        target,
        selected_action,
        confirmed_prior_primitive_count,
        prior_primitive_confirmation_chain_sha256,
    } = value;
    let visible_game_log = refresh_competitive_match_visible_game_log_v1(
        visible_game_log.into_match_lease_v1(),
        &visible_identity,
        visible_game_log_capture_request,
    )?;
    let fresh_frame = capture_admitted_mtgo_duel_visible_frame_v1(
        &lease.resources.duel_perception_profile,
        capture_timeout_ms,
    )?;
    let fresh_perception = perceive_admitted_duel_frame_v1(
        fresh_frame,
        &lease.resources.duel_perception_profile,
        &lease.resources.duel_perception_runtime,
        fresh_frame_identity,
        perception_timeout_ms,
    )?;
    let lease_commitments = lease.lease.commitments_v1();
    let session_commitments = session.commitments_v1();
    let fresh_commitments = fresh_perception.commitments_v1();
    let deployment_commitment_sha256 = lease
        .resources
        .checkpoint_deployment
        .deployment_commitment_sha256();
    validate_operator_gameplay_action_source_v1(
        &lease.resource_commitments,
        &lease_commitments,
        &session_commitments,
        &fresh_commitments,
        deployment_commitment_sha256,
    )?;
    if fresh_commitments
        .source_frame
        .source_capture
        .captured_at_unix_millis
        <= visible_game_log.latest_capture_unix_millis_v1()
    {
        return Err(
            "fresh player-visible gameplay target is not newer than the Game Log refresh"
                .to_owned(),
        );
    }
    let fresh_lifecycle = fresh_perception
        .competitive_lifecycle_v1()
        .ok_or("fresh player-visible gameplay target lacks competitive lifecycle")?;
    if fresh_lifecycle.event_kind() != lease_commitments.event_kind
        || fresh_lifecycle.event_identity_sha256_v1()
            != Some(lease_commitments.event_identity_sha256.as_str())
        || fresh_lifecycle.match_identity_sha256_v1()
            != Some(lease_commitments.match_identity_sha256.as_str())
        || fresh_lifecycle.game_number_v1() != Some(lease_commitments.game_number)
    {
        return Err(
            "fresh player-visible gameplay target changed the exact event, match, or game"
                .to_owned(),
        );
    }
    match &confirmed_history {
        Some(history) => validate_competitive_player_visible_game_history_for_session_v1(
            history,
            deployment_commitment_sha256,
            session_commitments.confirmed_action_count,
            session_commitments.last_confirmed_frame_sequence,
            fresh_commitments.frame_sequence,
        )
        .map_err(|error| {
            format!("validate fresh exact-game confirmed decision history: {error}")
        })?,
        None if session_commitments.confirmed_action_count == 0 => {}
        None => return Err(
            "fresh player-visible gameplay session has confirmed actions but no decision history"
                .to_owned(),
        ),
    }
    let confirmed_decision =
        fresh_perception.player_visible_confirmed_decision_for_action_v1(&selected_action)?;
    let action_baseline = if target.is_final_primitive_v1()
        && matches!(
            selected_action,
            MtgoPlayerVisibleDuelActionV1::PlayLand { .. }
                | MtgoPlayerVisibleDuelActionV1::CastSpell { .. }
                | MtgoPlayerVisibleDuelActionV1::Discard { .. }
                | MtgoPlayerVisibleDuelActionV1::DeclareAttackers { .. }
        ) {
        Some(begin_competitive_match_visible_game_log_action_baseline_v1(
            &visible_game_log,
            &confirmed_decision,
        )?)
    } else {
        None
    };
    let target = rebind_opaque_player_visible_duel_gesture_target_v1(
        target,
        fresh_perception,
        &lease.resources.duel_gesture_profile,
        &lease.resources.duel_gesture_runtime,
        protocol,
        target_timeout_ms,
    )?;
    Ok(
        OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayFreshTargetV1 {
            _lease: lease,
            _session: session,
            _visible_identity: visible_identity,
            _visible_game_log: visible_game_log,
            confirmed_history,
            _action_baseline: action_baseline,
            _confirmed_decision: confirmed_decision,
            target,
            selected_action,
            confirmed_prior_primitive_count,
            prior_primitive_confirmation_chain_sha256,
        },
    )
}

/// Privately derives current desktop points from the exact refreshed visible
/// target regions while retaining the complete operator ownership chain. This
/// creates no input command and does not reserve or modify the shared input
/// gate.
pub fn prepare_competitive_post_entry_operator_player_visible_gameplay_pointer_v1(
    value: OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayFreshTargetV1,
) -> Result<OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayPreparedPointerV1, String> {
    let OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayFreshTargetV1 {
        _lease: lease,
        _session: session,
        _visible_identity: visible_identity,
        _visible_game_log: visible_game_log,
        confirmed_history,
        _action_baseline: action_baseline,
        _confirmed_decision: confirmed_decision,
        target,
        selected_action,
        confirmed_prior_primitive_count,
        prior_primitive_confirmation_chain_sha256,
    } = value;
    let primitive = target.primitive_v1().clone();
    let primitive_index = target.primitive_index_v1();
    let is_final_primitive = target.is_final_primitive_v1();
    let pointer = prepare_opaque_player_visible_duel_gesture_pointer_v1(target)?;
    Ok(
        OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayPreparedPointerV1 {
            _lease: lease,
            _session: session,
            _visible_identity: visible_identity,
            _visible_game_log: visible_game_log,
            confirmed_history,
            _action_baseline: action_baseline,
            _confirmed_decision: confirmed_decision,
            _pointer: pointer,
            selected_action,
            primitive,
            primitive_index,
            is_final_primitive,
            confirmed_prior_primitive_count,
            prior_primitive_confirmation_chain_sha256,
        },
    )
}

/// Classifies and fixes the complete player-visible postcondition region set
/// against the same privately retained source pixels as the current pointer.
/// The protocol trust root is separately reviewed and currently empty. This
/// performs no capture, input, event entry, spending, or session advancement.
pub fn prepare_competitive_post_entry_operator_player_visible_gameplay_before_input_v1(
    value: OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayPreparedPointerV1,
    protocol: &AdmittedMtgoPlayerVisibleGameplayPostconditionProtocolV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayBeforeInputV1, String> {
    let OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayPreparedPointerV1 {
        _lease: lease,
        _session: session,
        _visible_identity: visible_identity,
        _visible_game_log: visible_game_log,
        confirmed_history,
        _action_baseline: action_baseline,
        _confirmed_decision: confirmed_decision,
        _pointer: pointer,
        selected_action,
        primitive,
        primitive_index,
        is_final_primitive,
        confirmed_prior_primitive_count,
        prior_primitive_confirmation_chain_sha256,
    } = value;
    let launch = visible_identity.commitments_v1();
    let session_commitments = session.commitments_v1();
    let expected_game_log_baseline_commitment_sha256 = action_baseline
        .as_ref()
        .map(|baseline| baseline.baseline_commitment_sha256_v1().to_owned());
    let context = MtgoPrivatePlayerVisibleGameplayBeforeInputContextV1 {
        event_kind: launch.event_kind,
        event_identity_sha256: visible_identity.event_identity_sha256_v1().to_owned(),
        match_identity_sha256: visible_identity.match_identity_sha256_v1().to_owned(),
        game_number: launch.game_number,
        deployment_commitment_sha256: lease
            .resources
            .checkpoint_deployment
            .deployment_commitment_sha256()
            .to_owned(),
        confirmed_prior_primitive_count,
        prior_primitive_confirmation_chain_sha256: prior_primitive_confirmation_chain_sha256
            .clone(),
        expected_game_log_baseline_commitment_sha256,
    };
    if session_commitments.event_kind != launch.event_kind
        || session_commitments.game_number != launch.game_number
    {
        return Err(
            "player-visible before-input context changed its exact event or game".to_owned(),
        );
    }
    let before_input = prepare_opaque_player_visible_gameplay_before_input_v1(
        pointer,
        context,
        &lease.resources.duel_gesture_profile,
        &lease.resources.duel_gesture_runtime,
        protocol,
        timeout_ms,
    )?;
    Ok(
        OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayBeforeInputV1 {
            _lease: lease,
            _session: session,
            _visible_identity: visible_identity,
            _visible_game_log: visible_game_log,
            confirmed_history,
            _action_baseline: action_baseline,
            _confirmed_decision: confirmed_decision,
            _before_input: before_input,
            selected_action,
            primitive,
            primitive_index,
            is_final_primitive,
            confirmed_prior_primitive_count,
            prior_primitive_confirmation_chain_sha256,
        },
    )
}

/// Crosses the only player-visible live-input seam. The exact event lease and
/// attended game session are borrowed for authorization, then retained in a
/// move-only pending value while the process-wide input gate is locked.
pub fn execute_competitive_post_entry_operator_player_visible_gameplay_primitive_v1(
    value: OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayBeforeInputV1,
) -> Result<OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayPendingV1, String> {
    let OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayBeforeInputV1 {
        _lease: lease,
        _session: session,
        _visible_identity: visible_identity,
        _visible_game_log: visible_game_log,
        confirmed_history,
        _action_baseline: action_baseline,
        _confirmed_decision: _,
        _before_input: before_input,
        selected_action,
        primitive: _,
        primitive_index,
        is_final_primitive,
        confirmed_prior_primitive_count,
        prior_primitive_confirmation_chain_sha256,
    } = value;
    if confirmed_prior_primitive_count != primitive_index {
        return Err(
            "player-visible gesture input lacks one confirmed prior primitive per stage".to_owned(),
        );
    }
    let pending = execute_prepared_competitive_player_visible_gameplay_primitive_v1(
        before_input,
        &lease.lease,
        &session,
    )?;
    Ok(
        OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayPendingV1 {
            lease,
            session,
            visible_identity,
            visible_game_log,
            confirmed_history,
            action_baseline,
            pending,
            selected_action,
            primitive_index,
            is_final_primitive,
            confirmed_prior_primitive_count,
            prior_primitive_confirmation_chain_sha256,
        },
    )
}

/// Joins one pending primitive to an internally captured newer visible frame
/// and an internally refreshed visible Game Log snapshot. Intermediate
/// primitives return the next target with a confirmation chain; final
/// primitives advance session and decision history. The input gate remains
/// closed until the entire transition succeeds.
pub fn confirm_competitive_post_entry_operator_player_visible_gameplay_primitive_v1(
    value: OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayPendingV1,
    visible_game_log_capture_request: MtgoDxgiCaptureRequestV3,
    capture_timeout_ms: u32,
    perception_timeout_ms: u32,
    target_protocol: &AdmittedMtgoPlayerVisibleDuelGestureTargetProtocolV1,
    target_timeout_ms: u32,
) -> Result<MtgoCompetitiveOperatorPlayerVisibleGameplayConfirmationV1, String> {
    let OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayPendingV1 {
        lease,
        session,
        visible_identity,
        visible_game_log,
        confirmed_history,
        action_baseline,
        pending,
        selected_action,
        primitive_index,
        is_final_primitive,
        confirmed_prior_primitive_count,
        prior_primitive_confirmation_chain_sha256,
    } = value;
    let pending_commitments = pending.commitments_v1();
    let visible_game_log = refresh_competitive_match_visible_game_log_v1(
        visible_game_log.into_match_lease_v1(),
        &visible_identity,
        visible_game_log_capture_request,
    )?;
    validate_operator_visible_game_log_lineage_v1(&lease, &visible_identity, &visible_game_log)?;
    if visible_game_log.latest_capture_unix_millis_v1()
        <= pending_commitments.input_sent_at_unix_millis
    {
        return Err(
            "after-input visible Game Log snapshot is not newer than the input receipt".to_owned(),
        );
    }
    let corroboration = match action_baseline {
        Some(baseline) => Some(corroborate_competitive_match_visible_game_log_action_v1(
            baseline,
            &visible_game_log,
        )?),
        None => None,
    };
    let after_frame = capture_admitted_mtgo_duel_visible_frame_v1(
        &lease.resources.duel_perception_profile,
        capture_timeout_ms,
    )?;
    let after_frame_commitments = after_frame.commitments_v1();
    if after_frame_commitments
        .source_capture
        .captured_at_unix_millis
        <= pending_commitments.input_sent_at_unix_millis
        || after_frame_commitments
            .source_capture
            .captured_at_unix_millis
            <= visible_game_log.latest_capture_unix_millis_v1()
    {
        return Err(
            "after-input duel frame is not newer than the input receipt and Game Log refresh"
                .to_owned(),
        );
    }
    let after_frame_id = frame_id_from_capture_commitment_v1(
        &after_frame_commitments
            .source_capture
            .capture_commitment_sha256,
        pending_commitments.before_frame_id,
    )?;
    let after_frame_sequence = pending_commitments
        .before_frame_sequence
        .checked_add(1)
        .ok_or("after-input duel frame sequence overflow")?;
    let after = confirm_pending_competitive_player_visible_gameplay_primitive_v1(
        pending,
        after_frame,
        MtgoDuelPerceptionFrameIdentityV1 {
            frame_id: after_frame_id,
            frame_sequence: after_frame_sequence,
        },
        corroboration,
    )?;
    let mut session_slot = Some(session);
    let advanced_session = if is_final_primitive {
        Some(advance_competitive_player_visible_gameplay_session_v1(
            session_slot
                .take()
                .expect("player-visible session is present before final confirmation"),
            &after,
        )?)
    } else {
        None
    };
    let (checked, binding, confirmation_commitment, input_receipt, _, _) = after.into_parts_v1();
    if is_final_primitive {
        let session = advanced_session.expect("final primitive produced an advanced session");
        let history_id = player_visible_history_id_v1(
            visible_identity.match_identity_sha256_v1(),
            visible_identity.commitments_v1().game_number,
        )?;
        let confirmed_history = match confirmed_history {
            Some(history) => {
                append_checked_untrusted_competitive_player_visible_game_history_from_player_visible_postcondition_v1(
                    history,
                    checked,
                )
                .map_err(|error| format!("append player-visible gameplay history: {error}"))?
            }
            None => {
                begin_checked_untrusted_competitive_player_visible_game_history_from_player_visible_postcondition_v1(
                    &history_id,
                    checked,
                )
                .map_err(|error| format!("begin player-visible gameplay history: {error}"))?
            }
        };
        release_confirmed_competitive_player_visible_gameplay_primitive_v1(&input_receipt)?;
        return Ok(
            MtgoCompetitiveOperatorPlayerVisibleGameplayConfirmationV1::ActionConfirmed(Box::new(
                OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayConfirmedV1 {
                    lease,
                    session,
                    visible_identity,
                    visible_game_log,
                    confirmed_history,
                    confirmation_commitment_sha256: confirmation_commitment,
                },
            )),
        );
    }
    let session = session_slot.expect("non-final primitive retains the unchanged session");
    let next_count = confirmed_prior_primitive_count
        .checked_add(1)
        .ok_or("player-visible confirmed primitive count overflow")?;
    let chain = hash_parts_v1(
        COMPETITIVE_OPERATOR_PLAYER_VISIBLE_PRIMITIVE_CONFIRMATION_CHAIN_DOMAIN_V1,
        &[
            prior_primitive_confirmation_chain_sha256
                .as_deref()
                .unwrap_or("none")
                .as_bytes(),
            confirmation_commitment.as_bytes(),
            input_receipt.as_bytes(),
            primitive_index.to_be_bytes().as_slice(),
            next_count.to_be_bytes().as_slice(),
            b"one_nonfinal_visible_primitive_confirmed_before_next_target",
        ],
    );
    let continuation_frame = capture_admitted_mtgo_duel_visible_frame_v1(
        &lease.resources.duel_perception_profile,
        capture_timeout_ms,
    )?;
    let continuation_commitments = continuation_frame.commitments_v1();
    if continuation_commitments
        .source_capture
        .captured_at_unix_millis
        <= after_frame_commitments
            .source_capture
            .captured_at_unix_millis
        || continuation_commitments
            .source_capture
            .capture_commitment_sha256
            == after_frame_commitments
                .source_capture
                .capture_commitment_sha256
    {
        return Err(
            "player-visible continuation frame is not strictly newer than the confirmed postcondition"
                .to_owned(),
        );
    }
    let continuation_frame_id = frame_id_from_capture_commitment_v1(
        &continuation_commitments
            .source_capture
            .capture_commitment_sha256,
        after_frame_id,
    )?;
    let continuation_frame_sequence = after_frame_sequence
        .checked_add(1)
        .ok_or("player-visible continuation frame sequence overflow")?;
    let continuation_perception = perceive_admitted_duel_frame_v1(
        continuation_frame,
        &lease.resources.duel_perception_profile,
        &lease.resources.duel_perception_runtime,
        MtgoDuelPerceptionFrameIdentityV1 {
            frame_id: continuation_frame_id,
            frame_sequence: continuation_frame_sequence,
        },
        perception_timeout_ms,
    )?;
    let lease_commitments = lease.lease.commitments_v1();
    let session_commitments = session.commitments_v1();
    let continuation_perception_commitments = continuation_perception.commitments_v1();
    validate_operator_gameplay_action_source_v1(
        &lease.resource_commitments,
        &lease_commitments,
        &session_commitments,
        &continuation_perception_commitments,
        lease
            .resources
            .checkpoint_deployment
            .deployment_commitment_sha256(),
    )?;
    let continuation_lifecycle = continuation_perception
        .competitive_lifecycle_v1()
        .ok_or("player-visible gesture continuation lacks competitive lifecycle")?;
    if continuation_lifecycle.event_kind() != lease_commitments.event_kind
        || continuation_lifecycle.event_identity_sha256_v1()
            != Some(lease_commitments.event_identity_sha256.as_str())
        || continuation_lifecycle.match_identity_sha256_v1()
            != Some(lease_commitments.match_identity_sha256.as_str())
        || continuation_lifecycle.game_number_v1() != Some(lease_commitments.game_number)
    {
        return Err(
            "player-visible gesture continuation changed the exact event, match, or game"
                .to_owned(),
        );
    }
    let target = advance_opaque_player_visible_duel_gesture_target_v1(
        binding,
        continuation_perception,
        &lease.resources.duel_gesture_profile,
        &lease.resources.duel_gesture_runtime,
        target_protocol,
        target_timeout_ms,
    )?;
    if target.primitive_index_v1() != next_count {
        return Err("player-visible gesture continuation skipped a confirmed primitive".to_owned());
    }
    release_confirmed_competitive_player_visible_gameplay_primitive_v1(&input_receipt)?;
    Ok(
        MtgoCompetitiveOperatorPlayerVisibleGameplayConfirmationV1::ContinueGesture(Box::new(
            OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayTargetV1 {
                _lease: lease,
                _session: session,
                _visible_identity: visible_identity,
                _visible_game_log: visible_game_log,
                confirmed_history,
                target,
                selected_action,
                confirmed_prior_primitive_count: next_count,
                prior_primitive_confirmation_chain_sha256: Some(chain),
            },
        )),
    )
}

fn validate_operator_visible_game_log_lineage_v1(
    lease: &OpaqueMtgoCompetitiveOperatorGameplayLeaseV1,
    visible_identity: &OpaqueMtgoCompetitiveLaunchIdentityV1,
    visible_game_log: &OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
) -> Result<(), String> {
    let lease_commitments = lease.lease.commitments_v1();
    let launch = visible_identity.commitments_v1();
    if launch.event_kind != lease_commitments.event_kind
        || launch.game_number != lease_commitments.game_number
        || visible_identity.event_identity_sha256_v1() != lease_commitments.event_identity_sha256
        || visible_identity.match_identity_sha256_v1() != lease_commitments.match_identity_sha256
        || visible_game_log.event_kind_v1() != lease_commitments.event_kind
        || visible_game_log.game_number_v1() != lease_commitments.game_number
        || visible_game_log.event_identity_sha256_v1() != lease_commitments.event_identity_sha256
        || visible_game_log.match_identity_sha256_v1() != lease_commitments.match_identity_sha256
    {
        return Err(
            "after-input visible Game Log changed the exact event, match, or game".to_owned(),
        );
    }
    Ok(())
}

fn player_visible_history_id_v1(
    match_identity_sha256: &str,
    game_number: u8,
) -> Result<String, String> {
    require_sha256_v1(match_identity_sha256, "match_identity_sha256")?;
    Ok(format!(
        "mtgo-visible-game-{}-{game_number}",
        &match_identity_sha256[..16]
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
        None,
        lease.prior_operator,
    )
}

/// Scores one current player-visible duel decision through the exact loaded
/// checkpoint and resolves its selected legal semantic to the exact visible
/// control. The returned move-only selection retains the gameplay lease and
/// attended game session and exposes only the sanitized player-visible action.
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
#[allow(dead_code)]
pub(crate) fn select_competitive_post_entry_operator_gameplay_action_v1(
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
    let player_visible_input = perception
        .player_visible_duel_decision_input_v1()
        .map_err(|error| format!("project player-visible duel decision: {error}"))?;
    let selection = score_and_select_opaque_admitted_duel_perception_with_loaded_deployment_v1(
        perception,
        &lease.resources.duel_perception_profile,
        &lease.resources.checkpoint_deployment,
    )?;
    let selected_index = selection.commitments_v1().selected_index;
    let player_visible_selected_action = player_visible_input
        .ordered_legal_actions
        .get(selected_index)
        .cloned()
        .ok_or("duel model selected outside the player-visible legal-action vector")?;
    let control = resolve_opaque_profile_bound_duel_control_v1(selection)?;
    Ok(OpaqueMtgoCompetitiveOperatorGameplaySelectionV1 {
        lease,
        session,
        control,
        player_visible_selected_action,
    })
}

/// Binds the reviewed UI gesture for an already model-selected action to the
/// exact League or Challenge session and profile-pinned source target runtime.
/// The returned ordinary session-bound action still requires one distinct
/// fresh visible perception before any primitive can be prepared.
#[allow(dead_code)]
pub(crate) fn bind_competitive_post_entry_operator_gameplay_action_v1(
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
        player_visible_selected_action: _,
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
    visible_game_log_baseline: Option<OpaqueMtgoCompetitiveVisibleGameLogBaselineV1>,
    prior: MtgoCompetitivePostEntryOperatorCommitmentsV1,
) -> Result<OpaqueMtgoCompetitivePostEntryOperatorV1, String> {
    if resource_commitments.resource_bundle_commitment_sha256
        != prior.resource_bundle_commitment_sha256
    {
        return Err("competitive post-entry operator resources changed while advancing".to_owned());
    }
    let visible_game_log_baseline_commitment_sha256 = visible_game_log_baseline
        .as_ref()
        .map(OpaqueMtgoCompetitiveVisibleGameLogBaselineV1::baseline_commitment_sha256_v1);
    validate_visible_game_log_baseline_transition_v1(
        prior.current_phase,
        prior.visible_game_log_baseline_commitment_sha256.as_deref(),
        visible_game_log_baseline_commitment_sha256,
    )?;
    let accepted_transition_count = prior
        .accepted_transition_count
        .checked_add(1)
        .ok_or("competitive post-entry operator transition count overflow")?;
    let runtime_commitments = runtime.commitments_v1();
    let commitments = post_entry_operator_commitments_v1(
        &resource_commitments,
        &runtime_commitments,
        accepted_transition_count,
        visible_game_log_baseline_commitment_sha256,
        Some(prior.operator_commitment_sha256.as_str()),
        COMPETITIVE_POST_ENTRY_OPERATOR_ADVANCE_DOMAIN_V1,
    )?;
    Ok(OpaqueMtgoCompetitivePostEntryOperatorV1 {
        resources,
        resource_commitments,
        runtime,
        visible_game_log_baseline,
        commitments,
    })
}

fn validate_visible_game_log_baseline_transition_v1(
    prior_phase: MtgoCompetitiveLifecyclePhaseV1,
    prior_baseline_commitment_sha256: Option<&str>,
    next_baseline_commitment_sha256: Option<&str>,
) -> Result<(), String> {
    if let Some(commitment) = prior_baseline_commitment_sha256 {
        require_sha256_v1(commitment, "prior visible Game Log baseline commitment")?;
    }
    if let Some(commitment) = next_baseline_commitment_sha256 {
        require_sha256_v1(commitment, "next visible Game Log baseline commitment")?;
    }
    match (
        prior_baseline_commitment_sha256,
        next_baseline_commitment_sha256,
    ) {
        (None, None) => Ok(()),
        (None, Some(_)) if prior_phase == MtgoCompetitiveLifecyclePhaseV1::PairingReady => Ok(()),
        (Some(prior), Some(next)) if prior == next => Ok(()),
        (None, Some(_)) => {
            Err("visible Game Log baseline may begin only at the Pairing Ready boundary".to_owned())
        }
        (Some(_), None) => {
            Err("visible Game Log baseline was dropped while advancing the operator".to_owned())
        }
        (Some(_), Some(_)) => {
            Err("visible Game Log baseline changed while advancing the operator".to_owned())
        }
    }
}

#[cfg(test)]
fn next_visible_game_log_baseline_present_after_lifecycle_action_v1(
    current_phase: MtgoCompetitiveLifecyclePhaseV1,
    baseline_present: bool,
    action: MtgoCompetitiveLifecycleActionV1,
) -> Result<bool, String> {
    match action {
        MtgoCompetitiveLifecycleActionV1::AcceptPairing => {
            if current_phase != MtgoCompetitiveLifecyclePhaseV1::PairingReady {
                return Err(
                    "pre-pairing Game Log baseline requires the Pairing Ready boundary".to_owned(),
                );
            }
            if baseline_present {
                return Err(
                    "competitive operator already owns a pre-pairing Game Log baseline".to_owned(),
                );
            }
            Ok(true)
        }
        _ => Ok(baseline_present),
    }
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
    visible_game_log_baseline_commitment_sha256: Option<&str>,
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
    if let Some(baseline) = visible_game_log_baseline_commitment_sha256 {
        require_sha256_v1(baseline, "visible Game Log baseline commitment")?;
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
            visible_game_log_baseline_commitment_sha256
                .unwrap_or("")
                .as_bytes(),
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
        visible_game_log_baseline_commitment_sha256: visible_game_log_baseline_commitment_sha256
            .map(str::to_owned),
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
            && checkpoint.native_player_visible_duel_action_interface_present
            && model.native_checkpoint_player_visible_only_duel_action_interface_present
            && model.current_duel_scorer_kernel_bookkeeping_withheld
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

#[allow(dead_code)]
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

#[allow(clippy::too_many_arguments)]
fn validate_operator_direct_visible_before_dispatch_v1(
    resources: &MtgoCompetitiveOperatorResourceCommitmentsV1,
    lease: &MtgoCompetitiveEventGameplayLeaseCommitmentsV1,
    session: &MtgoCompetitiveGestureGameSessionCommitmentsV1,
    launch: &crate::MtgoOpaqueCompetitiveLaunchIdentityCommitmentsV1,
    launch_event_identity_sha256: &str,
    launch_match_identity_sha256: &str,
    visible_game_log: &OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    confirmed_history: Option<&CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1>,
    direct: &mtgo_blackbox_v1::MtgoDirectVisibleGameplayDispatchCommitmentsV1<'_>,
) -> Result<(), String> {
    let session_policy = session
        .policy_deployment_commitment_sha256
        .as_deref()
        .ok_or("direct gameplay requires a session bound to the selected policy deployment")?;
    validate_operator_direct_visible_join_identity_v1(&OperatorDirectVisibleJoinIdentityV1 {
        resource_deployment_commitment_sha256: resources
            .policy_deployment_commitment_sha256
            .clone(),
        lease_event_kind: lease.event_kind,
        lease_event_identity_sha256: lease.event_identity_sha256.clone(),
        lease_match_identity_sha256: lease.match_identity_sha256.clone(),
        lease_game_number: lease.game_number,
        lease_deployment_commitment_sha256: lease.policy_deployment_commitment_sha256.clone(),
        session_event_kind: session.event_kind,
        session_game_number: session.game_number,
        session_deployment_commitment_sha256: session_policy.to_owned(),
        session_mode_authorization_commitment_sha256: session
            .mode_authorization_commitment_sha256
            .clone(),
        session_gameplay_authorization_commitment_sha256: session
            .match_gameplay_authorization_commitment_sha256
            .clone(),
        session_valid_from_frame_sequence: session.valid_from_frame_sequence,
        session_valid_through_frame_sequence: session.valid_through_frame_sequence,
        session_last_confirmed_frame_sequence: session.last_confirmed_frame_sequence,
        launch_event_kind: launch.event_kind,
        launch_event_identity_sha256: launch_event_identity_sha256.to_owned(),
        launch_match_identity_sha256: launch_match_identity_sha256.to_owned(),
        launch_game_number: launch.game_number,
        launch_frame_sequence: launch.frame_sequence,
        launch_captured_at_unix_millis: launch.captured_at_unix_millis,
        game_log_event_kind: visible_game_log.event_kind_v1(),
        game_log_event_identity_sha256: visible_game_log.event_identity_sha256_v1().to_owned(),
        game_log_match_identity_sha256: visible_game_log.match_identity_sha256_v1().to_owned(),
        game_log_game_number: visible_game_log.game_number_v1(),
        game_log_captured_at_unix_millis: visible_game_log.latest_capture_unix_millis_v1(),
        direct_event_kind: direct.event_kind_v1(),
        direct_event_identity_sha256: direct.event_identity_sha256_v1().to_owned(),
        direct_match_identity_sha256: direct.match_identity_sha256_v1().to_owned(),
        direct_game_number: direct.game_number_v1(),
        direct_deployment_commitment_sha256: direct.deployment_commitment_sha256_v1().to_owned(),
        direct_mode_authorization_commitment_sha256: direct
            .mode_authorization_commitment_sha256_v1()
            .to_owned(),
        direct_gameplay_authorization_commitment_sha256: direct
            .gameplay_authorization_commitment_sha256_v1()
            .to_owned(),
        direct_source_frame_sequence: direct.source_frame_sequence_v1(),
        direct_source_captured_at_unix_millis: direct.source_captured_at_unix_millis_v1(),
    })?;
    match confirmed_history {
        Some(history) => validate_competitive_player_visible_game_history_for_session_v1(
            history,
            &resources.policy_deployment_commitment_sha256,
            session.confirmed_action_count,
            session.last_confirmed_frame_sequence,
            direct.source_frame_sequence_v1(),
        )
        .map_err(|error| format!("validate direct visible confirmed history: {error}"))?,
        None if session.confirmed_action_count == 0 => {}
        None => {
            return Err(
                "direct visible gameplay session has confirmed actions but no decision history"
                    .to_owned(),
            )
        }
    }
    Ok(())
}

struct OperatorDirectVisibleJoinIdentityV1 {
    resource_deployment_commitment_sha256: String,
    lease_event_kind: MtgoCompetitiveEventKindV1,
    lease_event_identity_sha256: String,
    lease_match_identity_sha256: String,
    lease_game_number: u8,
    lease_deployment_commitment_sha256: String,
    session_event_kind: MtgoCompetitiveEventKindV1,
    session_game_number: u8,
    session_deployment_commitment_sha256: String,
    session_mode_authorization_commitment_sha256: String,
    session_gameplay_authorization_commitment_sha256: String,
    session_valid_from_frame_sequence: u64,
    session_valid_through_frame_sequence: u64,
    session_last_confirmed_frame_sequence: u64,
    launch_event_kind: MtgoCompetitiveEventKindV1,
    launch_event_identity_sha256: String,
    launch_match_identity_sha256: String,
    launch_game_number: u8,
    launch_frame_sequence: u64,
    launch_captured_at_unix_millis: u128,
    game_log_event_kind: MtgoCompetitiveEventKindV1,
    game_log_event_identity_sha256: String,
    game_log_match_identity_sha256: String,
    game_log_game_number: u8,
    game_log_captured_at_unix_millis: u128,
    direct_event_kind: MtgoCompetitiveEventKindV1,
    direct_event_identity_sha256: String,
    direct_match_identity_sha256: String,
    direct_game_number: u8,
    direct_deployment_commitment_sha256: String,
    direct_mode_authorization_commitment_sha256: String,
    direct_gameplay_authorization_commitment_sha256: String,
    direct_source_frame_sequence: u64,
    direct_source_captured_at_unix_millis: u128,
}

fn validate_operator_direct_visible_join_identity_v1(
    value: &OperatorDirectVisibleJoinIdentityV1,
) -> Result<(), String> {
    if value.lease_event_kind != value.session_event_kind
        || value.lease_event_kind != value.launch_event_kind
        || value.lease_event_kind != value.game_log_event_kind
        || value.lease_event_kind != value.direct_event_kind
        || value.lease_game_number != value.session_game_number
        || value.lease_game_number != value.launch_game_number
        || value.lease_game_number != value.game_log_game_number
        || value.lease_game_number != value.direct_game_number
        || value.lease_event_identity_sha256 != value.launch_event_identity_sha256
        || value.lease_event_identity_sha256 != value.game_log_event_identity_sha256
        || value.lease_event_identity_sha256 != value.direct_event_identity_sha256
        || value.lease_match_identity_sha256 != value.launch_match_identity_sha256
        || value.lease_match_identity_sha256 != value.game_log_match_identity_sha256
        || value.lease_match_identity_sha256 != value.direct_match_identity_sha256
        || value.lease_deployment_commitment_sha256 != value.resource_deployment_commitment_sha256
        || value.session_deployment_commitment_sha256 != value.resource_deployment_commitment_sha256
        || value.direct_deployment_commitment_sha256 != value.resource_deployment_commitment_sha256
        || value.session_mode_authorization_commitment_sha256
            != value.direct_mode_authorization_commitment_sha256
        || value.session_gameplay_authorization_commitment_sha256
            != value.direct_gameplay_authorization_commitment_sha256
        || value.launch_frame_sequence > value.direct_source_frame_sequence
        || value.launch_captured_at_unix_millis > value.direct_source_captured_at_unix_millis
        || value.game_log_captured_at_unix_millis > value.direct_source_captured_at_unix_millis
        || value.direct_source_frame_sequence < value.session_valid_from_frame_sequence
        || value.direct_source_frame_sequence <= value.session_last_confirmed_frame_sequence
        || value.direct_source_frame_sequence > value.session_valid_through_frame_sequence
    {
        return Err(
            "direct visible gameplay changed the operator event, match, game, deployment, authorization, source frame, or Game Log lineage"
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

fn operator_auxiliary_resolution_commitment_v1(
    domain: &[u8],
    resource_bundle_commitment_sha256: &str,
    prior_operator_commitment_sha256: &str,
    semantic_resolution_commitment_sha256: &str,
    scope: &[u8],
) -> Result<String, String> {
    for (value, label) in [
        (
            resource_bundle_commitment_sha256,
            "auxiliary resolution resource bundle",
        ),
        (
            prior_operator_commitment_sha256,
            "auxiliary resolution prior operator",
        ),
        (
            semantic_resolution_commitment_sha256,
            "auxiliary semantic resolution",
        ),
    ] {
        require_sha256_v1(value, label)?;
    }
    Ok(hash_parts_v1(
        domain,
        &[
            resource_bundle_commitment_sha256.as_bytes(),
            prior_operator_commitment_sha256.as_bytes(),
            semantic_resolution_commitment_sha256.as_bytes(),
            scope,
        ],
    ))
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
            visible_game_log_baseline_commitment_sha256: None,
            prior_operator_commitment_sha256: Some(digest('9')),
            operator_commitment_sha256: digest('a'),
        }
    }

    #[test]
    fn accept_pairing_is_the_only_initial_visible_game_log_baseline_boundary() {
        assert!(
            next_visible_game_log_baseline_present_after_lifecycle_action_v1(
                MtgoCompetitiveLifecyclePhaseV1::PairingReady,
                false,
                MtgoCompetitiveLifecycleActionV1::AcceptPairing,
            )
            .unwrap()
        );
        assert!(
            next_visible_game_log_baseline_present_after_lifecycle_action_v1(
                MtgoCompetitiveLifecyclePhaseV1::PairingReady,
                true,
                MtgoCompetitiveLifecycleActionV1::AcceptPairing,
            )
            .is_err()
        );
        assert!(
            next_visible_game_log_baseline_present_after_lifecycle_action_v1(
                MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
                false,
                MtgoCompetitiveLifecycleActionV1::AcceptPairing,
            )
            .is_err()
        );
        assert!(
            next_visible_game_log_baseline_present_after_lifecycle_action_v1(
                MtgoCompetitiveLifecyclePhaseV1::MatchComplete,
                true,
                MtgoCompetitiveLifecycleActionV1::ContinueAfterMatch,
            )
            .unwrap()
        );

        assert!(validate_visible_game_log_baseline_transition_v1(
            MtgoCompetitiveLifecyclePhaseV1::PairingReady,
            None,
            Some(&digest('1')),
        )
        .is_ok());
        assert!(validate_visible_game_log_baseline_transition_v1(
            MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
            Some(&digest('1')),
            Some(&digest('1')),
        )
        .is_ok());
        assert!(validate_visible_game_log_baseline_transition_v1(
            MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
            Some(&digest('1')),
            None,
        )
        .is_err());
        assert!(validate_visible_game_log_baseline_transition_v1(
            MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
            Some(&digest('1')),
            Some(&digest('2')),
        )
        .is_err());
        assert!(validate_visible_game_log_baseline_transition_v1(
            MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
            None,
            Some(&digest('1')),
        )
        .is_err());
    }

    fn checkpoint_capabilities_v1(
        native_duel_action_interface_present: bool,
    ) -> MtgoNativeCheckpointCompetitiveCapabilitiesV1 {
        let mut value = MtgoNativeCheckpointCompetitiveCapabilitiesV1 {
            schema_version:
                mtgo_blackbox_v1::MTGO_NATIVE_CHECKPOINT_COMPETITIVE_CAPABILITIES_SCHEMA_V1,
            deployment_commitment_sha256: digest('8'),
            native_duel_action_interface_present,
            native_player_visible_duel_action_interface_present: false,
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

    fn direct_visible_join_v1() -> OperatorDirectVisibleJoinIdentityV1 {
        OperatorDirectVisibleJoinIdentityV1 {
            resource_deployment_commitment_sha256: digest('8'),
            lease_event_kind: MtgoCompetitiveEventKindV1::League,
            lease_event_identity_sha256: digest('d'),
            lease_match_identity_sha256: digest('e'),
            lease_game_number: 1,
            lease_deployment_commitment_sha256: digest('8'),
            session_event_kind: MtgoCompetitiveEventKindV1::League,
            session_game_number: 1,
            session_deployment_commitment_sha256: digest('8'),
            session_mode_authorization_commitment_sha256: digest('2'),
            session_gameplay_authorization_commitment_sha256: digest('6'),
            session_valid_from_frame_sequence: 10,
            session_valid_through_frame_sequence: 300,
            session_last_confirmed_frame_sequence: 199,
            launch_event_kind: MtgoCompetitiveEventKindV1::League,
            launch_event_identity_sha256: digest('d'),
            launch_match_identity_sha256: digest('e'),
            launch_game_number: 1,
            launch_frame_sequence: 100,
            launch_captured_at_unix_millis: 1_000,
            game_log_event_kind: MtgoCompetitiveEventKindV1::League,
            game_log_event_identity_sha256: digest('d'),
            game_log_match_identity_sha256: digest('e'),
            game_log_game_number: 1,
            game_log_captured_at_unix_millis: 1_900,
            direct_event_kind: MtgoCompetitiveEventKindV1::League,
            direct_event_identity_sha256: digest('d'),
            direct_match_identity_sha256: digest('e'),
            direct_game_number: 1,
            direct_deployment_commitment_sha256: digest('8'),
            direct_mode_authorization_commitment_sha256: digest('2'),
            direct_gameplay_authorization_commitment_sha256: digest('6'),
            direct_source_frame_sequence: 202,
            direct_source_captured_at_unix_millis: 2_000,
        }
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
    fn auxiliary_operator_resolution_commitment_binds_resource_operator_and_semantic_lineage() {
        let baseline = operator_auxiliary_resolution_commitment_v1(
            COMPETITIVE_OPERATOR_PREGAME_RESOLUTION_DOMAIN_V1,
            &digest('1'),
            &digest('2'),
            &digest('3'),
            b"pregame",
        )
        .unwrap();
        for changed in [
            operator_auxiliary_resolution_commitment_v1(
                COMPETITIVE_OPERATOR_PREGAME_RESOLUTION_DOMAIN_V1,
                &digest('4'),
                &digest('2'),
                &digest('3'),
                b"pregame",
            )
            .unwrap(),
            operator_auxiliary_resolution_commitment_v1(
                COMPETITIVE_OPERATOR_PREGAME_RESOLUTION_DOMAIN_V1,
                &digest('1'),
                &digest('4'),
                &digest('3'),
                b"pregame",
            )
            .unwrap(),
            operator_auxiliary_resolution_commitment_v1(
                COMPETITIVE_OPERATOR_PREGAME_RESOLUTION_DOMAIN_V1,
                &digest('1'),
                &digest('2'),
                &digest('4'),
                b"pregame",
            )
            .unwrap(),
            operator_auxiliary_resolution_commitment_v1(
                COMPETITIVE_OPERATOR_SIDEBOARD_RESOLUTION_DOMAIN_V1,
                &digest('1'),
                &digest('2'),
                &digest('3'),
                b"sideboard",
            )
            .unwrap(),
        ] {
            assert_ne!(baseline, changed);
        }
        assert!(operator_auxiliary_resolution_commitment_v1(
            COMPETITIVE_OPERATOR_PREGAME_RESOLUTION_DOMAIN_V1,
            "not-a-digest",
            &digest('2'),
            &digest('3'),
            b"pregame",
        )
        .is_err());
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
    fn gameplay_route_requires_checkpoint_and_player_visible_only_interface() {
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
                native_model_path_present: false,
                ..
            }
        ));

        let model = check_competitive_model_decision_readiness_v1();
        assert!(model.native_checkpoint_duel_action_interface_present);
        assert!(!model.native_checkpoint_player_visible_only_duel_action_interface_present);
        assert!(!model.current_duel_scorer_kernel_bookkeeping_withheld);

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

    #[test]
    fn direct_visible_operator_join_accepts_both_modes_and_exact_lineage() {
        validate_operator_direct_visible_join_identity_v1(&direct_visible_join_v1()).unwrap();
        let mut challenge = direct_visible_join_v1();
        challenge.lease_event_kind = MtgoCompetitiveEventKindV1::Challenge;
        challenge.session_event_kind = MtgoCompetitiveEventKindV1::Challenge;
        challenge.launch_event_kind = MtgoCompetitiveEventKindV1::Challenge;
        challenge.game_log_event_kind = MtgoCompetitiveEventKindV1::Challenge;
        challenge.direct_event_kind = MtgoCompetitiveEventKindV1::Challenge;
        validate_operator_direct_visible_join_identity_v1(&challenge).unwrap();
    }

    #[test]
    fn direct_visible_next_frame_is_derived_from_owned_session_and_launch() {
        assert_eq!(
            next_direct_visible_frame_sequence_v1(10, 300, 199, 100).unwrap(),
            200
        );
        assert_eq!(
            next_direct_visible_frame_sequence_v1(10, 300, 9, 150).unwrap(),
            151
        );
        assert_eq!(
            next_direct_visible_frame_sequence_v1(10, 300, 9, 5).unwrap(),
            10
        );
        assert_eq!(
            next_direct_visible_frame_sequence_v1(1, 2, 0, 0).unwrap(),
            2
        );
    }

    #[test]
    fn direct_visible_next_frame_rejects_invalid_or_exhausted_lifetime() {
        assert!(next_direct_visible_frame_sequence_v1(0, 300, 0, 0).is_err());
        assert!(next_direct_visible_frame_sequence_v1(20, 10, 9, 9).is_err());
        assert!(next_direct_visible_frame_sequence_v1(1, 1, 0, 0).is_err());
        assert!(next_direct_visible_frame_sequence_v1(10, 20, 20, 10).is_err());
        assert!(next_direct_visible_frame_sequence_v1(10, 20, 19, 20).is_err());
        assert!(next_direct_visible_frame_sequence_v1(1, u64::MAX, 0, u64::MAX).is_err());
    }

    #[test]
    fn direct_visible_operator_join_rejects_crossed_authority_and_lineage() {
        let mutations: [fn(&mut OperatorDirectVisibleJoinIdentityV1); 10] = [
            |value| value.direct_event_kind = MtgoCompetitiveEventKindV1::Challenge,
            |value| value.direct_event_identity_sha256 = digest('f'),
            |value| value.direct_match_identity_sha256 = digest('f'),
            |value| value.direct_game_number = 2,
            |value| value.direct_deployment_commitment_sha256 = digest('f'),
            |value| value.direct_mode_authorization_commitment_sha256 = digest('f'),
            |value| value.direct_gameplay_authorization_commitment_sha256 = digest('f'),
            |value| {
                value.direct_source_frame_sequence = value.session_last_confirmed_frame_sequence
            },
            |value| {
                value.direct_source_frame_sequence = value.session_valid_through_frame_sequence + 1
            },
            |value| {
                value.game_log_captured_at_unix_millis =
                    value.direct_source_captured_at_unix_millis + 1
            },
        ];
        for mutate in mutations {
            let mut crossed = direct_visible_join_v1();
            mutate(&mut crossed);
            assert!(validate_operator_direct_visible_join_identity_v1(&crossed).is_err());
        }
    }
}
