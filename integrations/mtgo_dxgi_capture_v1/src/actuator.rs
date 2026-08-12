use crate::competitive_pregame_policy::AdmittedMtgoCompetitivePregameHeuristicV1;
use crate::competitive_visible_match_memory::OpaqueMtgoCompetitiveVisibleGameOutcomeV1;
use crate::probe::{
    advance_evaluated_competitive_event_monitor_v1,
    advance_prepared_competitive_duel_gesture_sequence_from_pinned_runtime_v1,
    bind_opaque_duel_perception_to_competitive_launch_identity_v1,
    capture_admitted_mtgo_competitive_navigation_frame_v1,
    classify_admitted_mtgo_competitive_navigation_frame_v1,
    classify_checked_untrusted_competitive_sideboard_v1,
    complete_opaque_player_visible_gameplay_after_input_v1,
    confirm_opaque_competitive_duel_gesture_postcondition_v1,
    confirm_opaque_competitive_duel_pass_postcondition_v1,
    confirm_opaque_competitive_entry_postcondition_v1,
    confirm_opaque_competitive_event_listing_opened_v1,
    confirm_opaque_competitive_lifecycle_control_postcondition_v1,
    confirm_pregame_keep_to_bottom_six_transition_v3,
    confirm_pregame_keep_to_first_main_transition_v3, confirm_pregame_mulligan_transition_v3,
    make_opaque_player_visible_gameplay_input_receipt_v1,
    prepare_opaque_competitive_duel_gesture_continuation_stage_from_pinned_runtime_v1,
    prepare_opaque_competitive_duel_gesture_source_stage_from_pinned_runtime_v1,
    prepare_opaque_competitive_event_listing_open_source_v1, prepare_pregame_actuation_v3,
    resolve_competitive_entry_pointer_target_v1,
    resolve_competitive_sideboard_drag_pointer_target_v1,
    validate_classifier_backed_competitive_entry_frame_transition_v1,
    validate_classifier_backed_competitive_entry_immediate_recapture_v1,
    MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1,
    MtgoClassifiedCompetitivePregameFrameCommitmentsV1,
    MtgoClassifiedCompetitiveSideboardCommitmentsV1, MtgoCompetitiveEntryControlDryRunPartsV1,
    MtgoCompetitiveEntryFrameTransitionCommitmentsV1, MtgoCompetitiveEntryFrameTransitionViewV1,
    MtgoCompetitiveEntryImmediateRecaptureCommitmentsV1, MtgoCompetitiveEntryPointerTargetV1,
    MtgoCompetitiveEntryVisibleConfirmationCommitmentsV1,
    MtgoCompetitiveEventListingOpenVisibleConfirmationCommitmentsV1,
    MtgoCompetitiveEventMonitorCommitmentsV1,
    MtgoCompetitiveLifecycleControlTransitionCommitmentsV1,
    MtgoCompetitiveNavigationFrameIdentityV1, MtgoCompetitivePregamePointerTargetV1,
    MtgoCompetitiveSideboardDragPointerTargetV1,
    MtgoOpaqueCompetitiveDuelGestureConfirmationCommitmentsV1,
    MtgoOpaqueCompetitiveDuelGestureSequenceCommitmentsV1,
    MtgoOpaqueCompetitiveDuelGestureSourcePreparationCommitmentsV1,
    MtgoOpaqueCompetitiveDuelPassConfirmationCommitmentsV1,
    MtgoOpaqueCompetitiveDuelPassPreparationCommitmentsV1,
    MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1,
    MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1,
    MtgoOpaqueCompetitiveLaunchIdentityCommitmentsV1,
    MtgoOpaquePinnedCompetitiveDuelGestureContinuationCommitmentsV1,
    MtgoPlannedCompetitiveSideboardCommitmentsV1, MtgoPlannedPregamePostconditionV3,
    OpaqueMtgoAdmittedDuelPerceptionV1, OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    OpaqueMtgoClassifiedCompetitivePregameFrameV1,
    OpaqueMtgoClassifiedCompetitivePregameModelContextV1,
    OpaqueMtgoClassifiedCompetitiveSideboardV1, OpaqueMtgoCompetitiveDuelGestureSequenceV1,
    OpaqueMtgoCompetitiveEntryControlDryRunV1, OpaqueMtgoCompetitiveEntryReviewIdentityV1,
    OpaqueMtgoCompetitiveEventMonitorV1, OpaqueMtgoCompetitiveLaunchIdentityV1,
    OpaqueMtgoCompetitiveLifecycleControlV1, OpaqueMtgoCompetitivePregamePublicContextWitnessV1,
    OpaqueMtgoConfirmedCompetitiveDuelGestureV1, OpaqueMtgoConfirmedCompetitiveDuelPassV1,
    OpaqueMtgoConfirmedCompetitiveEntryPostconditionV1,
    OpaqueMtgoConfirmedCompetitiveEventListingOpenV1,
    OpaqueMtgoConfirmedCompetitiveLifecycleControlPostconditionV1,
    OpaqueMtgoConfirmedKeepToBottomSixTransitionV3, OpaqueMtgoConfirmedKeepToFirstMainTransitionV3,
    OpaqueMtgoConfirmedMulliganTransitionV3, OpaqueMtgoDxgiBottomSixInitialMeasurementV3,
    OpaqueMtgoDxgiFirstMainMeasurementV3, OpaqueMtgoDxgiMulliganMeasurementV3,
    OpaqueMtgoEvaluatedCompetitiveEventListingV1,
    OpaqueMtgoPinnedCompetitiveDuelGestureContinuationV1, OpaqueMtgoPlannedCompetitiveSideboardV1,
    OpaqueMtgoPlayerVisibleGameplayAfterInputV1, OpaqueMtgoPregameActionPlanV3,
    OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1 as ProbeOpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1,
    OpaqueMtgoPreparedCompetitiveDuelPassV1, OpaqueMtgoPreparedCompetitiveEventListingOpenSourceV1,
    OpaqueMtgoPreparedPlayerVisibleDuelGesturePointerV1,
    OpaqueMtgoPreparedPlayerVisibleGameplayBeforeInputV1,
    OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1, PreparedPregameActuationV3,
};
use crate::{
    competitive_native_sideboard_configuration_commitment_v1,
    competitive_native_sideboard_model_input_commitment_v1,
    validate_competitive_native_sideboard_model_input_v1,
    visible_native_sideboard_configuration_v1, MtgoCompetitiveNativeSideboardModelInputV1,
    MtgoCompetitivePlayerRelativeGameWinnerV1,
};
use mtgo_blackbox_v1::{
    append_checked_untrusted_competitive_player_visible_game_history_v1,
    begin_checked_untrusted_competitive_player_visible_game_history_v1,
    canonical_duel_gesture_action_families_v1,
    competitive_match_gameplay_authorization_commitment_v1,
    competitive_mode_authorization_commitment_v1, confirm_competitive_sideboard_target_visible_v1,
    make_offline_competitive_lifecycle_intent_v1, validate_authorization_for_mode_v1,
    validate_checked_observed_competitive_lifecycle_advance_v1,
    AdmittedMtgoCompetitiveDuelLifecycleProfileV1, AdmittedMtgoCompetitiveEventListingEvaluationV1,
    AdmittedMtgoCompetitiveNavigationProfileV1, AdmittedMtgoCompetitiveSideboardEvaluationV1,
    AdmittedMtgoDuelGestureProfileV1, AdmittedMtgoDuelPerceptionProfileV1,
    CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
    CheckedUntrustedMtgoCompetitiveSideboardReadyV1,
    CheckedUntrustedMtgoPlayerVisibleGameLogActionCorroborationV1, MtgoAuthorizationScopeV1,
    MtgoCompetitiveDeckConfigurationV1, MtgoCompetitiveDeckPartitionV1,
    MtgoCompetitiveEntryAuthorizationV1, MtgoCompetitiveEntryResourceV1,
    MtgoCompetitiveEntryTermsV1, MtgoCompetitiveEventKindV1, MtgoCompetitiveLifecycleActionV1,
    MtgoCompetitiveLifecyclePhaseV1, MtgoCompetitiveMatchGameplayAuthorizationV1,
    MtgoCompetitivePregameStageLabelV1, MtgoCompetitivePregameVisibleCardV1,
    MtgoCompetitivePregameVisibleControlSemanticV1, MtgoCompetitivePregameVisibleControlV1,
    MtgoCompetitiveSideboardTransferDirectionV1, MtgoCompetitiveSideboardTransferV1,
    MtgoDuelActionFamilyV1, MtgoDuelGesturePrimitiveV1, MtgoDuelPrimaryActivationV1,
    MtgoObservedCompetitiveLifecycleAdvanceV1, MtgoPlayerVisibleDuelGesturePrimitiveV1,
    MtgoPregameActionSemanticV1, MtgoRuntimeModeV1, MtgoVisibleCompetitiveSideboardCardV1,
    MtgoVisibleCompetitiveSideboardZoneV1, ValidatedMtgoCompetitiveDeckManifestV1,
    MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1, MTGO_COMPETITIVE_MATCH_GAMEPLAY_AUTHORIZATION_SCHEMA_V1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::c_void;
use std::io::{self, IsTerminal, Write};
use std::mem::size_of;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, GetDpiForWindow, GetThreadDpiAwarenessContext,
    SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEINPUT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetClientRect, GetForegroundWindow, GetWindowThreadProcessId, IsHungAppWindow,
    IsIconic, IsWindow, IsWindowVisible, SetCursorPos, WindowFromPoint, GA_ROOT,
};

const PREGAME_INPUT_RECEIPT_DOMAIN_V3: &[u8] = b"mtgo-private-pregame-input-receipt-v3";
const COMPETITIVE_DUEL_PASS_INPUT_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-duel-priority-pass-input-receipt-v1";
const COMPETITIVE_DUEL_PASS_TRANSITION_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-duel-priority-pass-transition-receipt-v1";
const COMPETITIVE_DUEL_GESTURE_INPUT_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-duel-gesture-input-receipt-v1";
const COMPETITIVE_PLAYER_VISIBLE_GAMEPLAY_AUTHORITY_BINDING_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-player-visible-gameplay-authority-binding-v1";
const COMPETITIVE_PLAYER_VISIBLE_GAMEPLAY_SESSION_ADVANCE_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-player-visible-gameplay-session-advance-v1";
const COMPETITIVE_DUEL_GESTURE_TRANSITION_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-duel-gesture-transition-receipt-v1";
const COMPETITIVE_DUEL_GESTURE_CONTINUATION_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-duel-gesture-continuation-receipt-v1";
const PRIVATE_MATCH_AUTHORIZATION_DOMAIN_V3: &[u8] = b"mtgo-private-match-authorization-v3";
const RATIFIED_PRIVATE_MATCH_AUTHORIZATION_COMMITMENT_V3: Option<&str> = None;
const COMPETITIVE_DUEL_PASS_AUTHORIZATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-duel-priority-pass-authorization-v1";
const COMPETITIVE_DUEL_PASS_AUTHORIZATION_FROM_REVIEW_DOMAIN_V2: &[u8] =
    b"mtgo-competitive-duel-priority-pass-authorization-from-review-v2";
const COMPETITIVE_DUEL_PASS_AUTHORIZATION_BINDING_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-duel-priority-pass-authorization-binding-v1";
const RATIFIED_COMPETITIVE_DUEL_PASS_AUTHORIZATION_COMMITMENT_V1: Option<&str> = None;
const RATIFIED_COMPETITIVE_DUEL_PASS_AUTHORIZATION_FROM_REVIEW_COMMITMENT_V2: Option<&str> = None;
const COMPETITIVE_DUEL_GESTURE_AUTHORIZATION_FROM_REVIEW_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-duel-gesture-authorization-from-review-v1";
const RATIFIED_COMPETITIVE_DUEL_GESTURE_AUTHORIZATION_FROM_REVIEW_COMMITMENT_V1: Option<&str> =
    None;
const COMPETITIVE_ENTRY_AUTHORIZATION_RATIFICATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-entry-authorization-ratification-v1";
const COMPETITIVE_SELECTED_LISTING_ENTRY_REVIEW_BINDING_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-selected-listing-entry-review-binding-v1";
const COMPETITIVE_SELECTED_LISTING_ENTRY_AUTHORIZATION_RATIFICATION_DOMAIN_V2: &[u8] =
    b"mtgo-competitive-selected-listing-entry-authorization-ratification-v2";
const RATIFIED_COMPETITIVE_SELECTED_LISTING_ENTRY_AUTHORIZATION_COMMITMENT_V2: Option<&str> = None;
const COMPETITIVE_MATCH_LAUNCH_AUTHORIZATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-match-launch-authorization-v1";
#[cfg(test)]
const RATIFIED_COMPETITIVE_MATCH_LAUNCH_AUTHORIZATION_COMMITMENT_V1: Option<&str> = None;
const COMPETITIVE_GAME_SESSION_INITIAL_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-game-session-initial-v1";
const COMPETITIVE_GAME_SESSION_ADVANCE_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-game-session-advance-v1";
const ATTENDED_COMPETITIVE_MATCH_LAUNCH_REQUEST_DOMAIN_V4: &[u8] =
    b"mtgo-attended-competitive-match-launch-request-v4";
const ATTENDED_COMPETITIVE_MATCH_LAUNCH_RECEIPT_DOMAIN_V4: &[u8] =
    b"mtgo-attended-competitive-match-launch-receipt-v4";
const ATTENDED_COMPETITIVE_GESTURE_MATCH_LAUNCH_UPGRADE_DOMAIN_V1: &[u8] =
    b"mtgo-attended-competitive-gesture-match-launch-upgrade-v1";
const COMPETITIVE_GESTURE_GAME_SESSION_INITIAL_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-gesture-game-session-initial-v1";
const COMPETITIVE_GESTURE_GAME_SESSION_ADVANCE_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-gesture-game-session-advance-v1";
const COMPETITIVE_GESTURE_SESSION_SEQUENCE_BINDING_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-gesture-session-sequence-binding-v1";
const COMPETITIVE_GESTURE_SESSION_SOURCE_PREPARATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-gesture-session-source-preparation-v1";
const COMPETITIVE_GESTURE_SESSION_CONTINUATION_PREPARATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-gesture-session-continuation-preparation-v1";
const ATTENDED_COMPETITIVE_ENTRY_REVIEW_REQUEST_DOMAIN_V1: &[u8] =
    b"mtgo-attended-competitive-entry-review-request-v1";
const ATTENDED_COMPETITIVE_ENTRY_REVIEW_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-attended-competitive-entry-review-receipt-v1";
const CLASSIFIER_BOUND_COMPETITIVE_ENTRY_REVIEW_DOMAIN_V3: &[u8] =
    b"mtgo-classifier-bound-competitive-entry-review-v3";
const CONTROL_BOUND_COMPETITIVE_ENTRY_REVIEW_DOMAIN_V4: &[u8] =
    b"mtgo-control-bound-competitive-entry-review-v4";
const ATTENDED_COMPETITIVE_DECK_REVIEW_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-attended-competitive-deck-review-receipt-v1";
const COMPETITIVE_ENTRY_POSTCONDITION_DRY_RUN_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-entry-postcondition-dry-run-v1";
const COMPETITIVE_ENTRY_PREPARATION_DOMAIN_V1: &[u8] = b"mtgo-competitive-entry-preparation-v1";
const COMPETITIVE_ENTRY_INPUT_RECEIPT_DOMAIN_V1: &[u8] = b"mtgo-competitive-entry-input-receipt-v1";
const COMPETITIVE_ENTRY_CONFIRMATION_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-entry-confirmation-receipt-v1";
const COMPETITIVE_LIFECYCLE_AUTHORIZATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-lifecycle-authorization-v1";
const COMPETITIVE_LIFECYCLE_ACTION_SET_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-lifecycle-action-set-v1";
const RATIFIED_COMPETITIVE_LIFECYCLE_AUTHORIZATION_COMMITMENT_V1: Option<&str> = None;
const COMPETITIVE_OPEN_ENTRY_REVIEW_AUTHORIZATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-open-entry-review-authorization-v1";
const COMPETITIVE_OPEN_ENTRY_REVIEW_SCOPE_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-open-entry-review-scope-v1";
const RATIFIED_COMPETITIVE_OPEN_ENTRY_REVIEW_AUTHORIZATION_COMMITMENT_V1: Option<&str> = None;
const COMPETITIVE_OPEN_ENTRY_REVIEW_PREPARATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-open-entry-review-preparation-v1";
const COMPETITIVE_OPEN_ENTRY_REVIEW_INPUT_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-open-entry-review-input-receipt-v1";
const COMPETITIVE_OPEN_ENTRY_REVIEW_CONFIRMATION_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-open-entry-review-confirmation-receipt-v1";
const COMPETITIVE_SIDEBOARD_AUTOMATION_AUTHORIZATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-sideboard-automation-authorization-v1";
const COMPETITIVE_SIDEBOARD_AUTOMATION_SCOPE_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-sideboard-automation-scope-v1";
const RATIFIED_COMPETITIVE_SIDEBOARD_AUTOMATION_COMMITMENT_V1: Option<&str> = None;
const COMPETITIVE_LIFECYCLE_PREPARATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-lifecycle-preparation-v1";
const COMPETITIVE_LIFECYCLE_INPUT_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-lifecycle-input-receipt-v1";
const COMPETITIVE_LIFECYCLE_CONFIRMATION_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-lifecycle-confirmation-receipt-v1";
const COMPETITIVE_EVENT_RUNTIME_INITIAL_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-runtime-initial-v1";
const COMPETITIVE_EVENT_RUNTIME_ADVANCE_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-runtime-advance-v1";
const COMPETITIVE_EVENT_RUNTIME_MONITOR_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-runtime-monitor-v1";
const COMPETITIVE_EVENT_GAMEPLAY_LEASE_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-gameplay-lease-v1";
const COMPETITIVE_PREGAME_OBSERVATION_DOMAIN_V1: &[u8] = b"mtgo-competitive-pregame-observation-v1";
const COMPETITIVE_EVENT_PREGAME_SESSION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-pregame-session-v1";
const COMPETITIVE_EVENT_PREGAME_ADVANCE_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-pregame-advance-v1";
const COMPETITIVE_EVENT_PREGAME_BOTTOM_HISTORY_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-pregame-bottom-history-v1";
const COMPETITIVE_EVENT_PREGAME_PUBLIC_CONTEXT_STATE_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-pregame-public-context-state-v1";
const COMPETITIVE_EVENT_PREGAME_ACTION_PLAN_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-pregame-action-plan-v1";
const COMPETITIVE_NATIVE_PREGAME_MODEL_INPUT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-native-pregame-model-input-v1";
const COMPETITIVE_NATIVE_PREGAME_REQUEST_BINDING_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-native-pregame-request-binding-v1";
const COMPETITIVE_EVENT_PREGAME_FRESH_PREPARATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-pregame-fresh-preparation-v1";
const COMPETITIVE_EVENT_PREGAME_POSTCONDITION_DRY_RUN_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-pregame-postcondition-dry-run-v1";
const COMPETITIVE_EVENT_PREGAME_AUTHORIZATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-pregame-authorization-v1";
const RATIFIED_COMPETITIVE_EVENT_PREGAME_AUTHORIZATION_COMMITMENT_V1: Option<&str> = None;
const COMPETITIVE_EVENT_PREGAME_INPUT_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-pregame-input-receipt-v1";
const COMPETITIVE_EVENT_PREGAME_CONFIRMATION_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-pregame-confirmation-receipt-v1";
const COMPETITIVE_EVENT_PREGAME_COMPLETION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-pregame-completion-v1";
const COMPETITIVE_EVENT_MATCH_LAUNCH_BINDING_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-match-launch-binding-v1";
const COMPETITIVE_EVENT_SIDEBOARD_MEASUREMENT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-sideboard-measurement-v1";
const COMPETITIVE_NATIVE_SIDEBOARD_REQUEST_BINDING_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-native-sideboard-request-binding-v1";
const COMPETITIVE_EVENT_SIDEBOARD_SEQUENCE_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-sideboard-sequence-v1";
const COMPETITIVE_EVENT_SIDEBOARD_TRANSFER_PREPARATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-sideboard-transfer-preparation-v1";
const COMPETITIVE_EVENT_SIDEBOARD_FRESH_DRAG_PREPARATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-sideboard-fresh-drag-preparation-v1";
const COMPETITIVE_EVENT_SIDEBOARD_DRAG_INPUT_RECEIPT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-sideboard-drag-input-receipt-v1";
const COMPETITIVE_EVENT_SIDEBOARD_DRAG_CONFIRMED_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-sideboard-drag-confirmed-v1";
const COMPETITIVE_EVENT_SIDEBOARD_TRANSFER_CONFIRMATION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-sideboard-transfer-confirmation-v1";
const COMPETITIVE_EVENT_SIDEBOARD_READY_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-sideboard-ready-v1";
const COMPETITIVE_GESTURE_GAME_SESSION_EVENT_DECK_BIND_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-gesture-game-session-event-deck-bind-v1";
const COMPETITIVE_EVENT_GAMEPLAY_RETURN_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-gameplay-return-v1";
const ATTENDED_COMPETITIVE_MATCH_MAX_FRAME_ADVANCE_V4: u64 = 512;

const MTGO_ATTENDED_COMPETITIVE_MATCH_LAUNCH_REQUEST_SCHEMA_V4: u32 = 4;
const MTGO_ATTENDED_COMPETITIVE_ENTRY_REVIEW_SCHEMA_V1: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoPregameInputGateStatusV3 {
    Idle,
    Preparing,
    AwaitingVisiblePostcondition,
    Halted,
}

pub type MtgoInputGateStatusV3 = MtgoPregameInputGateStatusV3;

pub const MTGO_COMPETITIVE_AUTHORIZATION_READINESS_SCHEMA_V1: u32 = 1;

/// Non-authorizing visibility into the compile-pinned competitive permission
/// roots. The booleans reveal presence only, never commitment values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveAuthorizationRatificationReadinessV1 {
    pub schema_version: u32,
    pub reviewed_priority_pass_authorization_present: bool,
    pub reviewed_all_family_gesture_authorization_present: bool,
    pub selected_listing_entry_authorization_present: bool,
    pub lifecycle_authorization_present: bool,
    pub open_entry_review_authorization_present: bool,
    pub changed_sideboard_automation_authorization_present: bool,
    pub reviewed_competitive_pregame_authorization_present: bool,
}

impl MtgoCompetitiveAuthorizationRatificationReadinessV1 {
    pub fn unchanged_sideboard_event_path_present_v1(&self) -> bool {
        self.reviewed_all_family_gesture_authorization_present
            && self.selected_listing_entry_authorization_present
            && self.lifecycle_authorization_present
            && self.open_entry_review_authorization_present
            && self.reviewed_competitive_pregame_authorization_present
    }

    pub fn changed_sideboard_event_path_present_v1(&self) -> bool {
        self.unchanged_sideboard_event_path_present_v1()
            && self.changed_sideboard_automation_authorization_present
    }

    pub fn grants_live_authority_v1(&self) -> bool {
        false
    }
}

pub fn competitive_authorization_ratification_readiness_v1(
) -> MtgoCompetitiveAuthorizationRatificationReadinessV1 {
    MtgoCompetitiveAuthorizationRatificationReadinessV1 {
        schema_version: MTGO_COMPETITIVE_AUTHORIZATION_READINESS_SCHEMA_V1,
        reviewed_priority_pass_authorization_present:
            RATIFIED_COMPETITIVE_DUEL_PASS_AUTHORIZATION_FROM_REVIEW_COMMITMENT_V2.is_some(),
        reviewed_all_family_gesture_authorization_present:
            RATIFIED_COMPETITIVE_DUEL_GESTURE_AUTHORIZATION_FROM_REVIEW_COMMITMENT_V1.is_some(),
        selected_listing_entry_authorization_present:
            RATIFIED_COMPETITIVE_SELECTED_LISTING_ENTRY_AUTHORIZATION_COMMITMENT_V2.is_some(),
        lifecycle_authorization_present: RATIFIED_COMPETITIVE_LIFECYCLE_AUTHORIZATION_COMMITMENT_V1
            .is_some(),
        open_entry_review_authorization_present:
            RATIFIED_COMPETITIVE_OPEN_ENTRY_REVIEW_AUTHORIZATION_COMMITMENT_V1.is_some(),
        changed_sideboard_automation_authorization_present:
            RATIFIED_COMPETITIVE_SIDEBOARD_AUTOMATION_COMMITMENT_V1.is_some(),
        reviewed_competitive_pregame_authorization_present:
            RATIFIED_COMPETITIVE_EVENT_PREGAME_AUTHORIZATION_COMMITMENT_V1.is_some(),
    }
}

enum PregameInputGateStateV3 {
    Idle,
    Preparing,
    AwaitingVisiblePostcondition { receipt_sha256: String },
    Halted,
}

static PREGAME_INPUT_GATE_V3: OnceLock<Mutex<PregameInputGateStateV3>> = OnceLock::new();

/// A separately ratified exact-account and exact-correspondence authorization
/// for visible-only private-match input. The production ratification list is
/// empty until the exact Daybreak correspondence artifact is reviewed and
/// committed.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::RatifiedMtgoPrivateMatchAuthorizationV3;
/// let _forged = RatifiedMtgoPrivateMatchAuthorizationV3 {};
/// ```
pub struct RatifiedMtgoPrivateMatchAuthorizationV3 {
    scope: MtgoAuthorizationScopeV1,
    visible_account_alias: String,
    authorization_commitment_sha256: String,
}

impl RatifiedMtgoPrivateMatchAuthorizationV3 {
    pub fn account_alias_sha256_v3(&self) -> &str {
        &self.scope.account_alias_sha256
    }

    pub fn written_permission_sha256_v3(&self) -> &str {
        &self.scope.written_permission_sha256
    }

    pub fn authorization_commitment_sha256_v3(&self) -> &str {
        &self.authorization_commitment_sha256
    }
}

/// One separately ratified exact-account authorization for priority Pass in
/// exactly one of League or Challenge gameplay. Production ratification is
/// empty until the correspondence bytes and visible account identity are
/// reviewed and their exact commitment is compiled into the actuator.
///
/// This value grants no event-entry or purchase authority. It is not itself an
/// input command and has no coordinate conversion.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::RatifiedMtgoCompetitiveDuelPassAuthorizationV1;
/// let _forged = RatifiedMtgoCompetitiveDuelPassAuthorizationV1 {};
/// ```
pub struct RatifiedMtgoCompetitiveDuelPassAuthorizationV1 {
    scope: MtgoAuthorizationScopeV1,
    _permission_correspondence: Option<CheckedUntrustedMtgoAuthorizationCorrespondenceV1>,
    #[allow(dead_code)]
    visible_account_alias: String,
    event_kind: MtgoCompetitiveEventKindV1,
    mode_authorization_commitment_sha256: String,
    authorization_commitment_sha256: String,
    permission_review_commitment_sha256: Option<String>,
}

impl RatifiedMtgoCompetitiveDuelPassAuthorizationV1 {
    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.event_kind
    }

    pub fn account_alias_sha256_v1(&self) -> &str {
        &self.scope.account_alias_sha256
    }

    pub fn written_permission_sha256_v1(&self) -> &str {
        &self.scope.written_permission_sha256
    }

    pub fn mode_authorization_commitment_sha256_v1(&self) -> &str {
        &self.mode_authorization_commitment_sha256
    }

    pub fn authorization_commitment_sha256_v1(&self) -> &str {
        &self.authorization_commitment_sha256
    }

    pub fn permission_review_commitment_sha256_v2(&self) -> Option<&str> {
        self.permission_review_commitment_sha256.as_deref()
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoReviewedCompetitivePassRatificationCandidateV2 {
    pub permission_review_commitment_sha256: String,
    pub account_alias_sha256: String,
    pub correspondence_sha256: String,
    pub mode_authorization_commitment_sha256: String,
    pub ratification_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
}

impl MtgoReviewedCompetitivePassRatificationCandidateV2 {
    pub fn safe_for_live_input_v2(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v2(&self) -> bool {
        false
    }

    pub fn permits_spending_v2(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoReviewedCompetitivePregameRatificationCandidateV1 {
    pub permission_review_commitment_sha256: String,
    pub account_alias_sha256: String,
    pub correspondence_sha256: String,
    pub mode_authorization_commitment_sha256: String,
    pub heuristic_profile_commitment_sha256: String,
    pub heuristic_algorithm_commitment_sha256: String,
    pub heuristic_review_commitment_sha256: String,
    pub heuristic_admission_commitment_sha256: String,
    pub ratification_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
}

impl MtgoReviewedCompetitivePregameRatificationCandidateV1 {
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

/// Separately ratified exact-correspondence authority for the four competitive
/// pregame action families under one exact League or Challenge mode. It is
/// bound to one reviewed heuristic identity and grants no event-entry or
/// spending authority. The production root is empty.
pub struct RatifiedMtgoCompetitivePregameAuthorizationV1 {
    scope: MtgoAuthorizationScopeV1,
    _permission_correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    visible_account_alias: String,
    commitments: MtgoReviewedCompetitivePregameRatificationCandidateV1,
}

impl RatifiedMtgoCompetitivePregameAuthorizationV1 {
    pub fn commitments_v1(&self) -> MtgoReviewedCompetitivePregameRatificationCandidateV1 {
        self.commitments.clone()
    }

    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.commitments.event_kind
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

/// One separately ratified exact-account permission for the complete reviewed
/// duel gesture profile in exactly one League or Challenge mode. The profile
/// identity is retained, but this value is not an input command and grants no
/// event-entry or spending authority. The production ratification root is
/// empty.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::RatifiedMtgoCompetitiveDuelGestureAuthorizationV1;
/// let _forged = RatifiedMtgoCompetitiveDuelGestureAuthorizationV1 {};
/// ```
pub struct RatifiedMtgoCompetitiveDuelGestureAuthorizationV1 {
    scope: MtgoAuthorizationScopeV1,
    _permission_correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    _gesture_profile: AdmittedMtgoDuelGestureProfileV1,
    #[allow(dead_code)]
    visible_account_alias: String,
    event_kind: MtgoCompetitiveEventKindV1,
    mode_authorization_commitment_sha256: String,
    permission_review_commitment_sha256: String,
    gesture_evaluation_commitment_sha256: String,
    gesture_profile_admission_commitment_sha256: String,
    authorization_commitment_sha256: String,
}

impl RatifiedMtgoCompetitiveDuelGestureAuthorizationV1 {
    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.event_kind
    }

    pub fn account_alias_sha256_v1(&self) -> &str {
        &self.scope.account_alias_sha256
    }

    pub fn written_permission_sha256_v1(&self) -> &str {
        &self.scope.written_permission_sha256
    }

    pub fn mode_authorization_commitment_sha256_v1(&self) -> &str {
        &self.mode_authorization_commitment_sha256
    }

    pub fn permission_review_commitment_sha256_v1(&self) -> &str {
        &self.permission_review_commitment_sha256
    }

    pub fn gesture_evaluation_commitment_sha256_v1(&self) -> &str {
        &self.gesture_evaluation_commitment_sha256
    }

    pub fn gesture_profile_admission_commitment_sha256_v1(&self) -> &str {
        &self.gesture_profile_admission_commitment_sha256
    }

    pub fn authorization_commitment_sha256_v1(&self) -> &str {
        &self.authorization_commitment_sha256
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoReviewedCompetitiveGestureRatificationCandidateV1 {
    pub permission_review_commitment_sha256: String,
    pub account_alias_sha256: String,
    pub correspondence_sha256: String,
    pub mode_authorization_commitment_sha256: String,
    pub gesture_evaluation_commitment_sha256: String,
    pub gesture_profile_admission_commitment_sha256: String,
    pub gesture_target_runtime_binary_sha256: String,
    pub gesture_target_assets_manifest_sha256: String,
    pub supported_action_families: Vec<MtgoDuelActionFamilyV1>,
    pub ratification_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
}

impl MtgoReviewedCompetitiveGestureRatificationCandidateV1 {
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

#[derive(Clone, Copy)]
struct CompetitiveDuelGestureProfileFactsV1<'a> {
    evaluation_commitment_sha256: &'a str,
    admission_commitment_sha256: &'a str,
    runtime_binary_sha256: &'a str,
    assets_manifest_sha256: &'a str,
    supported_action_families: &'a [MtgoDuelActionFamilyV1],
}

/// Copyable review telemetry for one exact League or Challenge entry and its
/// exact existing-account resource terms. This is a proposed compile-time
/// ratification value only. It cannot enter the event, spend resources, or
/// send input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoReviewedCompetitiveEntryRatificationCandidateV1 {
    pub permission_review_commitment_sha256: String,
    pub account_alias_sha256: String,
    pub correspondence_sha256: String,
    pub mode_authorization_commitment_sha256: String,
    pub control_bound_review_commitment_sha256: String,
    pub owner_review_receipt_sha256: String,
    pub entry_authorization_sha256: String,
    pub source_identity_commitment_sha256: String,
    pub source_capture_commitment_sha256: String,
    pub source_navigation_classification_result_commitment_sha256: String,
    pub visible_control_region_sha256: String,
    pub selected_deck_label_sha256: String,
    pub selected_deck_region_sha256: String,
    pub deck_manifest_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub deck_review_receipt_sha256: String,
    pub event_identity_sha256: String,
    pub entry_terms_sha256: String,
    pub ratification_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub resource: MtgoCompetitiveEntryResourceV1,
    pub amount: u32,
}

impl MtgoReviewedCompetitiveEntryRatificationCandidateV1 {
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

/// Separately compile-ratified authority for one exact owner-reviewed entry.
/// It owns the checked correspondence and the complete control-bound review,
/// but it exposes no coordinates or input method. A future actuator must also
/// require an immediate recapture and the exact visible postcondition gate.
///
/// The production ratification root is empty.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::RatifiedMtgoCompetitiveEntryAuthorizationV1;
/// let _forged = RatifiedMtgoCompetitiveEntryAuthorizationV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::RatifiedMtgoCompetitiveEntryAuthorizationV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<RatifiedMtgoCompetitiveEntryAuthorizationV1>();
/// ```
pub struct RatifiedMtgoCompetitiveEntryAuthorizationV1 {
    _open_entry_review_authorization: RatifiedMtgoCompetitiveOpenEntryReviewAuthorizationV1,
    _open_entry_review_visible_confirmation: OpaqueMtgoConfirmedCompetitiveEventListingOpenV1,
    _review: CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4,
    #[allow(dead_code)]
    visible_account_alias: String,
    selected_listing_binding_commitment_sha256: String,
    commitments: MtgoReviewedCompetitiveEntryRatificationCandidateV1,
}

impl RatifiedMtgoCompetitiveEntryAuthorizationV1 {
    pub fn commitments_v1(&self) -> MtgoReviewedCompetitiveEntryRatificationCandidateV1 {
        self.commitments.clone()
    }

    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.commitments.event_kind
    }

    pub fn resource_v1(&self) -> MtgoCompetitiveEntryResourceV1 {
        self.commitments.resource
    }

    pub fn amount_v1(&self) -> u32 {
        self.commitments.amount
    }

    pub fn selected_listing_binding_commitment_sha256_v1(&self) -> &str {
        &self.selected_listing_binding_commitment_sha256
    }

    pub fn authorizes_exact_reviewed_entry_v1(&self) -> bool {
        true
    }

    pub fn authorizes_exact_reviewed_resource_terms_v1(&self) -> bool {
        true
    }

    pub fn permits_exact_reviewed_spending_v1(&self) -> bool {
        self.commitments.resource != MtgoCompetitiveEntryResourceV1::NoCost
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoReviewedCompetitiveLifecycleRatificationCandidateV1 {
    pub correspondence_sha256: String,
    pub permission_review_commitment_sha256: String,
    pub mode_authorization_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub lifecycle_profile_commitment_sha256: String,
    pub lifecycle_profile_admission_commitment_sha256: String,
    pub allowed_actions_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub ratification_commitment_sha256: String,
}

/// Exact reviewed correspondence and evaluated lifecycle-profile authority for
/// one League or Challenge mode. Production construction is disabled until the
/// complete candidate commitment is intentionally pinned.
pub struct RatifiedMtgoCompetitiveLifecycleAuthorizationV1 {
    _permission_correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    scope: MtgoAuthorizationScopeV1,
    commitments: MtgoReviewedCompetitiveLifecycleRatificationCandidateV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoReviewedCompetitiveSideboardAutomationRatificationCandidateV1 {
    pub lifecycle_authorization_commitment_sha256: String,
    pub permission_review_commitment_sha256: String,
    pub mode_authorization_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub navigation_profile_commitment_sha256: String,
    pub navigation_profile_admission_commitment_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub sideboard_evaluation_ratification_commitment_sha256: String,
    pub sideboard_evaluation_admission_commitment_sha256: String,
    pub automation_scope_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub ratification_commitment_sha256: String,
}

/// Separately reviewed authority for the exact visible sideboard parser,
/// one-card drag protocol, and changed-sideboard Submit Deck bridge. The
/// production root is empty, so this type cannot currently be constructed by
/// application code.
pub struct RatifiedMtgoCompetitiveSideboardAutomationAuthorizationV1 {
    commitments: MtgoReviewedCompetitiveSideboardAutomationRatificationCandidateV1,
}

impl RatifiedMtgoCompetitiveSideboardAutomationAuthorizationV1 {
    pub fn commitments_v1(
        &self,
    ) -> MtgoReviewedCompetitiveSideboardAutomationRatificationCandidateV1 {
        self.commitments.clone()
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }
}

impl RatifiedMtgoCompetitiveLifecycleAuthorizationV1 {
    pub fn commitments_v1(&self) -> MtgoReviewedCompetitiveLifecycleRatificationCandidateV1 {
        self.commitments.clone()
    }

    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.commitments.event_kind
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoReviewedCompetitiveOpenEntryReviewRatificationCandidateV1 {
    pub correspondence_sha256: String,
    pub permission_review_commitment_sha256: String,
    pub mode_authorization_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub navigation_profile_commitment_sha256: String,
    pub navigation_profile_admission_commitment_sha256: String,
    pub listing_evaluation_ratification_commitment_sha256: String,
    pub listing_evaluation_admission_commitment_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub open_review_scope_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub ratification_commitment_sha256: String,
}

/// Exact reviewed permission to open one evaluated League or Challenge
/// listing's Entry Review. It grants neither entry confirmation nor spending.
/// Production construction is disabled until the complete candidate is pinned.
pub struct RatifiedMtgoCompetitiveOpenEntryReviewAuthorizationV1 {
    _permission_correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    scope: MtgoAuthorizationScopeV1,
    commitments: MtgoReviewedCompetitiveOpenEntryReviewRatificationCandidateV1,
}

impl RatifiedMtgoCompetitiveOpenEntryReviewAuthorizationV1 {
    pub fn commitments_v1(&self) -> MtgoReviewedCompetitiveOpenEntryReviewRatificationCandidateV1 {
        self.commitments.clone()
    }

    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.commitments.event_kind
    }

    pub fn permits_entry_confirmation_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoPreparedCompetitiveOpenEntryReviewCommitmentsV1 {
    pub authorization_ratification_commitment_sha256: String,
    pub evaluated_listing_binding_commitment_sha256: String,
    pub source_preparation_commitment_sha256: String,
    pub preparation_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub source_frame_id: u64,
    pub source_frame_sequence: u64,
    pub source_captured_at_unix_millis: u128,
}

/// Move-only, fresh selected-listing input preparation. The control point and
/// opaque pixels remain private. This type cannot confirm entry or spend.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPreparedCompetitiveOpenEntryReviewV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoPreparedCompetitiveOpenEntryReviewV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPreparedCompetitiveOpenEntryReviewV1;
/// fn cannot_enter(value: &OpaqueMtgoPreparedCompetitiveOpenEntryReviewV1) {
///     let _ = value.control_rect_client_px();
///     let _ = value.confirm_entry();
///     let _ = value.spend();
/// }
/// ```
pub struct OpaqueMtgoPreparedCompetitiveOpenEntryReviewV1 {
    authorization: RatifiedMtgoCompetitiveOpenEntryReviewAuthorizationV1,
    source: OpaqueMtgoPreparedCompetitiveEventListingOpenSourceV1,
    pointer_target: MtgoCompetitiveEntryPointerTargetV1,
    commitments: MtgoPreparedCompetitiveOpenEntryReviewCommitmentsV1,
}

impl OpaqueMtgoPreparedCompetitiveOpenEntryReviewV1 {
    pub fn commitments_v1(&self) -> MtgoPreparedCompetitiveOpenEntryReviewCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn permits_entry_confirmation_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveOpenEntryReviewInputReceiptCommitmentsV1 {
    pub preparation_commitment_sha256: String,
    pub authorization_ratification_commitment_sha256: String,
    pub input_receipt_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub source_frame_id: u64,
    pub source_frame_sequence: u64,
    pub input_sent_at_unix_millis: u128,
    pub cursor_parked_outside_client: bool,
}

pub struct OpaqueMtgoPendingCompetitiveOpenEntryReviewV1 {
    prepared: OpaqueMtgoPreparedCompetitiveOpenEntryReviewV1,
    commitments: MtgoCompetitiveOpenEntryReviewInputReceiptCommitmentsV1,
}

impl OpaqueMtgoPendingCompetitiveOpenEntryReviewV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveOpenEntryReviewInputReceiptCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_next_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoConfirmedCompetitiveOpenEntryReviewCommitmentsV1 {
    pub input_receipt_sha256: String,
    pub preparation_commitment_sha256: String,
    pub authorization_ratification_commitment_sha256: String,
    pub visible_confirmation: MtgoCompetitiveEventListingOpenVisibleConfirmationCommitmentsV1,
    pub confirmation_receipt_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub after_frame_id: u64,
    pub after_frame_sequence: u64,
    pub after_captured_at_unix_millis: u128,
}

/// Exact visible arrival at Entry Review. It remains non-convertible to the
/// separately governed paid-entry confirmation path.
pub struct OpaqueMtgoConfirmedCompetitiveOpenEntryReviewV1 {
    _authorization: RatifiedMtgoCompetitiveOpenEntryReviewAuthorizationV1,
    _visible_confirmation: OpaqueMtgoConfirmedCompetitiveEventListingOpenV1,
    commitments: MtgoConfirmedCompetitiveOpenEntryReviewCommitmentsV1,
}

impl OpaqueMtgoConfirmedCompetitiveOpenEntryReviewV1 {
    pub fn commitments_v1(&self) -> MtgoConfirmedCompetitiveOpenEntryReviewCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn visibly_arrived_at_entry_review_v1(&self) -> bool {
        true
    }

    pub fn permits_entry_confirmation_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub fn safe_for_next_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoSelectedListingBoundCompetitiveEntryReviewCommitmentsV1 {
    pub open_entry_review_authorization_ratification_commitment_sha256: String,
    pub open_entry_review_confirmation_receipt_sha256: String,
    pub open_entry_review_visible_confirmation_commitment_sha256: String,
    pub control_bound_entry_review_commitment_sha256: String,
    pub legacy_entry_ratification_candidate_commitment_sha256: String,
    pub correspondence_sha256: String,
    pub permission_review_commitment_sha256: String,
    pub mode_authorization_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub navigation_profile_commitment_sha256: String,
    pub navigation_profile_admission_commitment_sha256: String,
    pub listing_evaluation_admission_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub entry_terms_sha256: String,
    pub resource: MtgoCompetitiveEntryResourceV1,
    pub amount: u32,
    pub open_arrival_frame_id: u64,
    pub open_arrival_frame_sequence: u64,
    pub open_arrival_captured_at_unix_millis: u128,
    pub entry_review_frame_id: u64,
    pub entry_review_frame_sequence: u64,
    pub entry_review_captured_at_unix_millis: u128,
    pub entry_review_source_capture_commitment_sha256: String,
    pub entry_review_window_continuity_commitment_sha256: String,
    pub binding_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoReviewedSelectedListingCompetitiveEntryRatificationCandidateV2 {
    pub selected_listing_binding_commitment_sha256: String,
    pub entry_review_candidate: MtgoReviewedCompetitiveEntryRatificationCandidateV1,
    pub ratification_commitment_sha256: String,
}

impl MtgoReviewedSelectedListingCompetitiveEntryRatificationCandidateV2 {
    pub fn safe_for_live_input_v2(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v2(&self) -> bool {
        false
    }

    pub fn permits_spending_v2(&self) -> bool {
        false
    }
}

/// One fresh paid Entry Review bound to the exact selected listing whose Open
/// Entry Review click was visibly confirmed. It remains checked-untrusted and
/// grants no entry, spending, or input authority. Production ratification is a
/// separate empty-root step.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoSelectedListingBoundCompetitiveEntryReviewV1;
/// let _forged = CheckedUntrustedMtgoSelectedListingBoundCompetitiveEntryReviewV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoSelectedListingBoundCompetitiveEntryReviewV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<CheckedUntrustedMtgoSelectedListingBoundCompetitiveEntryReviewV1>();
/// ```
pub struct CheckedUntrustedMtgoSelectedListingBoundCompetitiveEntryReviewV1 {
    open_entry_review: OpaqueMtgoConfirmedCompetitiveOpenEntryReviewV1,
    review: CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4,
    visible_account_alias: String,
    commitments: MtgoSelectedListingBoundCompetitiveEntryReviewCommitmentsV1,
    ratification_candidate: MtgoReviewedSelectedListingCompetitiveEntryRatificationCandidateV2,
}

impl CheckedUntrustedMtgoSelectedListingBoundCompetitiveEntryReviewV1 {
    pub fn commitments_v1(&self) -> MtgoSelectedListingBoundCompetitiveEntryReviewCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn ratification_candidate_v2(
        &self,
    ) -> MtgoReviewedSelectedListingCompetitiveEntryRatificationCandidateV2 {
        self.ratification_candidate.clone()
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoPreparedCompetitiveLifecycleControlCommitmentsV1 {
    pub lifecycle_authorization_commitment_sha256: String,
    pub control_binding_commitment_sha256: String,
    pub preparation_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub action: MtgoCompetitiveLifecycleActionV1,
    pub event_identity_sha256: String,
    pub match_identity_sha256: Option<String>,
    pub game_number: Option<u8>,
    pub source_frame_id: u64,
    pub source_frame_sequence: u64,
    pub source_captured_at_unix_millis: u128,
}

pub struct OpaqueMtgoPreparedCompetitiveLifecycleControlV1 {
    authorization: RatifiedMtgoCompetitiveLifecycleAuthorizationV1,
    control: OpaqueMtgoCompetitiveLifecycleControlV1,
    pointer_target: MtgoCompetitiveEntryPointerTargetV1,
    commitments: MtgoPreparedCompetitiveLifecycleControlCommitmentsV1,
}

impl OpaqueMtgoPreparedCompetitiveLifecycleControlV1 {
    pub fn commitments_v1(&self) -> MtgoPreparedCompetitiveLifecycleControlCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveLifecycleInputReceiptCommitmentsV1 {
    pub preparation_commitment_sha256: String,
    pub lifecycle_authorization_commitment_sha256: String,
    pub input_receipt_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub action: MtgoCompetitiveLifecycleActionV1,
    pub source_frame_id: u64,
    pub source_frame_sequence: u64,
    pub input_sent_at_unix_millis: u128,
    pub cursor_parked_outside_client: bool,
}

pub struct OpaqueMtgoPendingCompetitiveLifecycleControlV1 {
    prepared: OpaqueMtgoPreparedCompetitiveLifecycleControlV1,
    commitments: MtgoCompetitiveLifecycleInputReceiptCommitmentsV1,
}

impl OpaqueMtgoPendingCompetitiveLifecycleControlV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveLifecycleInputReceiptCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_next_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoConfirmedCompetitiveLifecycleControlCommitmentsV1 {
    pub input_receipt_sha256: String,
    pub preparation_commitment_sha256: String,
    pub lifecycle_authorization_commitment_sha256: String,
    pub visible_transition: MtgoCompetitiveLifecycleControlTransitionCommitmentsV1,
    pub confirmation_receipt_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub action: MtgoCompetitiveLifecycleActionV1,
    pub after_frame_id: u64,
    pub after_frame_sequence: u64,
    pub after_captured_at_unix_millis: u128,
}

pub struct OpaqueMtgoConfirmedCompetitiveLifecycleControlV1 {
    _authorization: RatifiedMtgoCompetitiveLifecycleAuthorizationV1,
    _visible_postcondition: OpaqueMtgoConfirmedCompetitiveLifecycleControlPostconditionV1,
    commitments: MtgoConfirmedCompetitiveLifecycleControlCommitmentsV1,
}

impl OpaqueMtgoConfirmedCompetitiveLifecycleControlV1 {
    pub fn commitments_v1(&self) -> MtgoConfirmedCompetitiveLifecycleControlCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn visibly_confirmed_v1(&self) -> bool {
        true
    }

    pub fn safe_for_next_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveEventRuntimeCommitmentsV1 {
    pub runtime_commitment_sha256: String,
    pub entry_confirmation_receipt_sha256: String,
    pub entry_ratification_commitment_sha256: String,
    pub entry_authorization_sha256: String,
    pub correspondence_sha256: String,
    pub permission_review_commitment_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_sha256: String,
    pub deck_format_sha256: String,
    pub player_known_current_deck_configuration_commitment_sha256: String,
    pub selected_deck_label_sha256: String,
    pub selected_deck_region_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub lifecycle_authorization_commitment_sha256: String,
    pub mode_authorization_commitment_sha256: String,
    pub navigation_profile_commitment_sha256: String,
    pub navigation_profile_admission_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub bound_event_identity_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub current_phase: MtgoCompetitiveLifecyclePhaseV1,
    pub current_lifecycle_snapshot_commitment_sha256: String,
    pub current_match_identity_sha256: Option<String>,
    pub current_game_number: Option<u8>,
    pub current_frame_id: u64,
    pub current_frame_sequence: u64,
    pub lifecycle_transition_count: u64,
    pub confirmed_lifecycle_action_count: u64,
    pub observed_lifecycle_advance_count: u64,
    pub pregame_session_count: u64,
    pub last_completed_pregame: Option<MtgoCompletedCompetitivePregameCommitmentsV1>,
    pub gameplay_lease_count: u64,
    pub last_returned_gameplay_frame_sequence: Option<u64>,
    pub event_monitor_chain_commitment_sha256: Option<String>,
    pub event_monitor_observation_count: u64,
    pub terminal_event_record_confirmed: bool,
    pub closed_to_event_browser: bool,
}

struct MtgoCompetitivePlayerKnownDeckStateV1 {
    submitted: crate::MtgoCompetitiveNativeSideboardConfigurationV1,
    current: crate::MtgoCompetitiveNativeSideboardConfigurationV1,
}

impl MtgoCompetitivePlayerKnownDeckStateV1 {
    fn from_manifest_v1(manifest: &ValidatedMtgoCompetitiveDeckManifestV1) -> Result<Self, String> {
        let submitted = visible_native_sideboard_configuration_v1(manifest.configuration_v1())?;
        Ok(Self {
            current: submitted.clone(),
            submitted,
        })
    }

    fn current_commitment_v1(&self) -> Result<String, String> {
        competitive_native_sideboard_configuration_commitment_v1(&self.current)
    }

    fn replace_current_v1(
        &mut self,
        current: crate::MtgoCompetitiveNativeSideboardConfigurationV1,
    ) -> Result<(), String> {
        crate::competitive_native_sideboard::validate_native_sideboard_configuration_v1(&current)?;
        self.current = current;
        Ok(())
    }

    fn reset_for_next_match_v1(&mut self) {
        self.current = self.submitted.clone();
    }
}

/// The next coordinator operation implied by one exact classified event
/// runtime. This is routing information only. It cannot authorize input,
/// event entry, spending, or a sideboard selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MtgoCompetitiveEventDriverStepV1 {
    AwaitPairingOrEventEnd,
    AcceptPairing,
    ResolvePregame {
        match_identity_sha256: String,
        game_number: u8,
    },
    LaunchGameplay {
        match_identity_sha256: String,
        game_number: u8,
    },
    AwaitGameOutcome {
        match_identity_sha256: String,
        game_number: u8,
    },
    ResolveSideboard {
        match_identity_sha256: String,
        game_number: u8,
    },
    ContinueAfterMatch,
    ResumeMatch {
        match_identity_sha256: String,
        game_number: u8,
    },
    BeginTerminalEventRecordMonitor,
    AdvanceTerminalEventRecordMonitor {
        prior_observation_count: u64,
    },
    CloseCompletedEvent,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveEventDriverDirectiveV1 {
    pub source_runtime_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub current_phase: MtgoCompetitiveLifecyclePhaseV1,
    pub current_frame_sequence: u64,
    pub step: MtgoCompetitiveEventDriverStepV1,
}

impl MtgoCompetitiveEventDriverDirectiveV1 {
    pub fn lifecycle_action_v1(&self) -> Option<MtgoCompetitiveLifecycleActionV1> {
        match &self.step {
            MtgoCompetitiveEventDriverStepV1::AcceptPairing => {
                Some(MtgoCompetitiveLifecycleActionV1::AcceptPairing)
            }
            MtgoCompetitiveEventDriverStepV1::ContinueAfterMatch => {
                Some(MtgoCompetitiveLifecycleActionV1::ContinueAfterMatch)
            }
            MtgoCompetitiveEventDriverStepV1::ResumeMatch { .. } => {
                Some(MtgoCompetitiveLifecycleActionV1::ResumeMatch)
            }
            MtgoCompetitiveEventDriverStepV1::CloseCompletedEvent => {
                Some(MtgoCompetitiveLifecycleActionV1::CloseCompletedEvent)
            }
            _ => None,
        }
    }

    pub fn allowed_observed_advances_v1(
        &self,
    ) -> &'static [MtgoObservedCompetitiveLifecycleAdvanceV1] {
        match &self.step {
            MtgoCompetitiveEventDriverStepV1::AwaitPairingOrEventEnd => &[
                MtgoObservedCompetitiveLifecycleAdvanceV1::PairingPosted,
                MtgoObservedCompetitiveLifecycleAdvanceV1::EventEnded,
            ],
            MtgoCompetitiveEventDriverStepV1::AwaitGameOutcome { .. } => &[
                MtgoObservedCompetitiveLifecycleAdvanceV1::GameEndedForSideboarding,
                MtgoObservedCompetitiveLifecycleAdvanceV1::MatchEnded,
                MtgoObservedCompetitiveLifecycleAdvanceV1::ConnectionInterrupted,
            ],
            _ => &[],
        }
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

/// Move-only coordinator for one exact, visibly confirmed League or Challenge
/// entry. It owns the spent entry authorization, the reusable exact-mode
/// lifecycle authorization, the current opaque classifier-backed frame, and an
/// optional monotonic event-record monitor. It cannot create another entry or
/// expose pixels, coordinates, process handles, or input primitives.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveEventRuntimeV1;
/// let _forged = OpaqueMtgoCompetitiveEventRuntimeV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveEventRuntimeV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveEventRuntimeV1>();
/// ```
pub struct OpaqueMtgoCompetitiveEventRuntimeV1 {
    _spent_entry_authorization: RatifiedMtgoCompetitiveEntryAuthorizationV1,
    lifecycle_authorization: RatifiedMtgoCompetitiveLifecycleAuthorizationV1,
    current_frame: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    player_known_deck_state: MtgoCompetitivePlayerKnownDeckStateV1,
    event_monitor: Option<OpaqueMtgoCompetitiveEventMonitorV1>,
    commitments: MtgoCompetitiveEventRuntimeCommitmentsV1,
}

impl OpaqueMtgoCompetitiveEventRuntimeV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveEventRuntimeCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.commitments.event_kind
    }

    pub fn current_phase_v1(&self) -> MtgoCompetitiveLifecyclePhaseV1 {
        self.commitments.current_phase
    }

    pub fn terminal_event_record_confirmed_v1(&self) -> bool {
        self.commitments.terminal_event_record_confirmed
    }

    pub fn closed_v1(&self) -> bool {
        self.commitments.closed_to_event_browser
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_additional_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_additional_spending_v1(&self) -> bool {
        false
    }

    pub(crate) fn current_frame_for_visible_game_log_v1(
        &self,
    ) -> &OpaqueMtgoClassifiedCompetitiveNavigationFrameV1 {
        &self.current_frame
    }
}

/// Derives the only next high-level driver branch from the current opaque
/// event runtime. Input-producing branches still have to pass their existing
/// ratification, immediate-recapture, shared-gate, and visible-postcondition
/// boundaries. The directive itself carries no authority.
pub fn next_competitive_event_driver_directive_v1(
    runtime: &OpaqueMtgoCompetitiveEventRuntimeV1,
) -> Result<MtgoCompetitiveEventDriverDirectiveV1, String> {
    validate_player_known_deck_state_against_runtime_v1(
        &runtime.commitments,
        &runtime.player_known_deck_state,
    )?;
    competitive_event_driver_directive_from_state_v1(&runtime.commitments)
}

#[cfg(test)]
fn competitive_event_driver_directive_from_commitments_v1(
    runtime: &MtgoCompetitiveEventRuntimeCommitmentsV1,
) -> Result<MtgoCompetitiveEventDriverDirectiveV1, String> {
    competitive_event_driver_directive_from_state_v1(runtime)
}

fn competitive_event_driver_directive_from_state_v1(
    runtime: &MtgoCompetitiveEventRuntimeCommitmentsV1,
) -> Result<MtgoCompetitiveEventDriverDirectiveV1, String> {
    if runtime.gameplay_lease_count == 0 && runtime.last_returned_gameplay_frame_sequence.is_some()
        || runtime.gameplay_lease_count > 0
            && runtime.last_returned_gameplay_frame_sequence.is_none()
    {
        return Err(
            "competitive event runtime has inconsistent gameplay checkout and return state"
                .to_owned(),
        );
    }
    if runtime.pregame_session_count == 0 && runtime.last_completed_pregame.is_some()
        || runtime.pregame_session_count > 0 && runtime.last_completed_pregame.is_none()
    {
        return Err(
            "competitive event runtime has inconsistent pregame session and completion state"
                .to_owned(),
        );
    }
    let event_monitor_present = runtime.event_monitor_chain_commitment_sha256.is_some();
    if event_monitor_present != (runtime.event_monitor_observation_count > 0)
        || runtime.terminal_event_record_confirmed && !event_monitor_present
    {
        return Err("competitive event runtime has inconsistent event monitor state".to_owned());
    }
    if runtime.terminal_event_record_confirmed
        && runtime.current_phase != MtgoCompetitiveLifecyclePhaseV1::EventComplete
        && !(runtime.closed_to_event_browser
            && runtime.current_phase == MtgoCompetitiveLifecyclePhaseV1::EventBrowser)
    {
        return Err(
            "competitive event runtime has a terminal record outside event completion".to_owned(),
        );
    }
    if runtime.closed_to_event_browser {
        if runtime.current_phase != MtgoCompetitiveLifecyclePhaseV1::EventBrowser
            || !runtime.terminal_event_record_confirmed
        {
            return Err(
                "closed competitive event runtime lacks its terminal record or browser return"
                    .to_owned(),
            );
        }
        return Ok(competitive_event_driver_directive_v1(
            runtime,
            MtgoCompetitiveEventDriverStepV1::Complete,
        ));
    }

    let step = match runtime.current_phase {
        MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing => {
            MtgoCompetitiveEventDriverStepV1::AwaitPairingOrEventEnd
        }
        MtgoCompetitiveLifecyclePhaseV1::PairingReady => {
            require_competitive_event_runtime_match_identity_v1(runtime)?;
            if runtime.current_game_number.is_some() {
                return Err(
                    "competitive event driver found a game number before gameplay".to_owned(),
                );
            }
            MtgoCompetitiveEventDriverStepV1::AcceptPairing
        }
        MtgoCompetitiveLifecyclePhaseV1::MatchInProgress => {
            let (match_identity_sha256, game_number) =
                require_competitive_event_runtime_match_and_game_v1(runtime)?;
            if runtime
                .last_returned_gameplay_frame_sequence
                .is_some_and(|returned| returned >= runtime.current_frame_sequence)
            {
                MtgoCompetitiveEventDriverStepV1::AwaitGameOutcome {
                    match_identity_sha256,
                    game_number,
                }
            } else if !runtime
                .last_completed_pregame
                .as_ref()
                .is_some_and(|completed| {
                    completed.match_identity_sha256 == match_identity_sha256
                        && completed.game_number == game_number
                })
            {
                MtgoCompetitiveEventDriverStepV1::ResolvePregame {
                    match_identity_sha256,
                    game_number,
                }
            } else {
                MtgoCompetitiveEventDriverStepV1::LaunchGameplay {
                    match_identity_sha256,
                    game_number,
                }
            }
        }
        MtgoCompetitiveLifecyclePhaseV1::Sideboarding => {
            let (match_identity_sha256, game_number) =
                require_competitive_event_runtime_match_and_game_v1(runtime)?;
            MtgoCompetitiveEventDriverStepV1::ResolveSideboard {
                match_identity_sha256,
                game_number,
            }
        }
        MtgoCompetitiveLifecyclePhaseV1::MatchComplete => {
            require_competitive_event_runtime_match_identity_v1(runtime)?;
            if runtime.current_game_number.is_some() {
                return Err(
                    "competitive event driver found a game number after match completion"
                        .to_owned(),
                );
            }
            MtgoCompetitiveEventDriverStepV1::ContinueAfterMatch
        }
        MtgoCompetitiveLifecyclePhaseV1::EventComplete => {
            if runtime.terminal_event_record_confirmed {
                MtgoCompetitiveEventDriverStepV1::CloseCompletedEvent
            } else if event_monitor_present {
                MtgoCompetitiveEventDriverStepV1::AdvanceTerminalEventRecordMonitor {
                    prior_observation_count: runtime.event_monitor_observation_count,
                }
            } else {
                MtgoCompetitiveEventDriverStepV1::BeginTerminalEventRecordMonitor
            }
        }
        MtgoCompetitiveLifecyclePhaseV1::Reconnect => {
            let (match_identity_sha256, game_number) =
                require_competitive_event_runtime_match_and_game_v1(runtime)?;
            MtgoCompetitiveEventDriverStepV1::ResumeMatch {
                match_identity_sha256,
                game_number,
            }
        }
        MtgoCompetitiveLifecyclePhaseV1::EventBrowser
        | MtgoCompetitiveLifecyclePhaseV1::EntryReview => {
            return Err(
                "an open competitive event runtime cannot route from a pre-entry phase".to_owned(),
            );
        }
    };
    Ok(competitive_event_driver_directive_v1(runtime, step))
}

fn require_competitive_event_runtime_match_identity_v1(
    runtime: &MtgoCompetitiveEventRuntimeCommitmentsV1,
) -> Result<String, String> {
    runtime
        .current_match_identity_sha256
        .clone()
        .ok_or_else(|| {
            "competitive event driver requires the current exact match identity".to_owned()
        })
}

fn require_competitive_event_runtime_match_and_game_v1(
    runtime: &MtgoCompetitiveEventRuntimeCommitmentsV1,
) -> Result<(String, u8), String> {
    let match_identity_sha256 = require_competitive_event_runtime_match_identity_v1(runtime)?;
    let game_number = runtime
        .current_game_number
        .ok_or("competitive event driver requires the current exact game number")?;
    Ok((match_identity_sha256, game_number))
}

fn competitive_event_driver_directive_v1(
    runtime: &MtgoCompetitiveEventRuntimeCommitmentsV1,
    step: MtgoCompetitiveEventDriverStepV1,
) -> MtgoCompetitiveEventDriverDirectiveV1 {
    MtgoCompetitiveEventDriverDirectiveV1 {
        source_runtime_commitment_sha256: runtime.runtime_commitment_sha256.clone(),
        event_kind: runtime.event_kind,
        current_phase: runtime.current_phase,
        current_frame_sequence: runtime.current_frame_sequence,
        step,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoMeasuredCompetitiveEventSideboardCommitmentsV1 {
    pub prior_event_runtime_commitment_sha256: String,
    pub sideboard_evaluation_ratification_commitment_sha256: String,
    pub sideboard_evaluation_admission_commitment_sha256: String,
    pub sideboard_classification: MtgoClassifiedCompetitiveSideboardCommitmentsV1,
    pub measurement_binding_commitment_sha256: String,
}

/// The event coordinator withheld while its exact current sideboard frame is
/// measured. The raw local configuration stays private; model selection sees
/// only the player-visible name-and-count projection through the native request.
/// No pixels, rectangles, input, or submit operation is exposed.
pub struct OpaqueMtgoMeasuredCompetitiveEventSideboardV1 {
    _spent_entry_authorization: RatifiedMtgoCompetitiveEntryAuthorizationV1,
    _lifecycle_authorization: RatifiedMtgoCompetitiveLifecycleAuthorizationV1,
    _sideboard_evaluation: AdmittedMtgoCompetitiveSideboardEvaluationV1,
    classified: OpaqueMtgoClassifiedCompetitiveSideboardV1,
    _player_known_deck_state: MtgoCompetitivePlayerKnownDeckStateV1,
    _manifest: ValidatedMtgoCompetitiveDeckManifestV1,
    _event_monitor: Option<OpaqueMtgoCompetitiveEventMonitorV1>,
    _prior: MtgoCompetitiveEventRuntimeCommitmentsV1,
    commitments: MtgoMeasuredCompetitiveEventSideboardCommitmentsV1,
}

impl OpaqueMtgoMeasuredCompetitiveEventSideboardV1 {
    pub fn commitments_v1(&self) -> MtgoMeasuredCompetitiveEventSideboardCommitmentsV1 {
        self.commitments.clone()
    }

    pub(crate) fn configuration_v1(&self) -> &MtgoCompetitiveDeckConfigurationV1 {
        self.classified.configuration_v1()
    }

    pub fn source_snapshot_commitment_sha256_v1(&self) -> &str {
        self.classified.source_snapshot_commitment_sha256_v1()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_sideboard_submission_v1(&self) -> bool {
        false
    }
}

/// Move-only exact sideboard request joining the visible configuration to the
/// prior game's visible winner. Only the semantic model input is public. The
/// event, match, source-memory, capture, classifier, authorization, and
/// deployment bindings stay private.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveNativeSideboardRequestV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveNativeSideboardRequestV1>();
/// ```
pub struct OpaqueMtgoCompetitiveNativeSideboardRequestV1 {
    _measurement: OpaqueMtgoMeasuredCompetitiveEventSideboardV1,
    _outcome: OpaqueMtgoCompetitiveVisibleGameOutcomeV1,
    model_input: MtgoCompetitiveNativeSideboardModelInputV1,
    model_input_commitment_sha256: String,
    _request_binding_commitment_sha256: String,
}

impl OpaqueMtgoCompetitiveNativeSideboardRequestV1 {
    pub fn model_input_v1(&self) -> &MtgoCompetitiveNativeSideboardModelInputV1 {
        &self.model_input
    }

    pub fn model_input_commitment_sha256_v1(&self) -> &str {
        &self.model_input_commitment_sha256
    }

    pub(crate) fn source_manifest_v1(&self) -> &ValidatedMtgoCompetitiveDeckManifestV1 {
        &self._measurement._manifest
    }

    pub(crate) fn source_snapshot_commitment_sha256_v1(&self) -> &str {
        self._measurement.source_snapshot_commitment_sha256_v1()
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoPlannedCompetitiveEventSideboardCommitmentsV1 {
    pub prior_event_runtime_commitment_sha256: String,
    pub sideboard_automation_ratification_commitment_sha256: String,
    pub measurement_binding_commitment_sha256: String,
    pub sideboard_plan: MtgoPlannedCompetitiveSideboardCommitmentsV1,
    pub event_plan_binding_commitment_sha256: String,
}

/// The event coordinator withheld with one exact coordinate-free model plan.
/// Public transfer semantics contain only visible names, counts, directions,
/// and sequence positions. Pixels, rectangles, local kernel card identifiers,
/// process handles, input primitives, and sideboard submission remain private
/// or absent.
pub struct OpaqueMtgoPlannedCompetitiveEventSideboardV1 {
    _spent_entry_authorization: RatifiedMtgoCompetitiveEntryAuthorizationV1,
    lifecycle_authorization: RatifiedMtgoCompetitiveLifecycleAuthorizationV1,
    sideboard_authorization: RatifiedMtgoCompetitiveSideboardAutomationAuthorizationV1,
    planned: OpaqueMtgoPlannedCompetitiveSideboardV1,
    player_known_deck_state: MtgoCompetitivePlayerKnownDeckStateV1,
    manifest: ValidatedMtgoCompetitiveDeckManifestV1,
    event_monitor: Option<OpaqueMtgoCompetitiveEventMonitorV1>,
    prior: MtgoCompetitiveEventRuntimeCommitmentsV1,
    commitments: MtgoPlannedCompetitiveEventSideboardCommitmentsV1,
}

impl OpaqueMtgoPlannedCompetitiveEventSideboardV1 {
    pub fn commitments_v1(&self) -> MtgoPlannedCompetitiveEventSideboardCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_sideboard_submission_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoAtomicCompetitiveSideboardTransferV1 {
    pub step_index: u16,
    pub card_name: String,
    pub direction: MtgoCompetitiveSideboardTransferDirectionV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveEventSideboardSequenceCommitmentsV1 {
    pub prior_event_runtime_commitment_sha256: String,
    pub event_plan_binding_commitment_sha256: String,
    pub plan_commitment_sha256: String,
    pub sequence_chain_commitment_sha256: String,
    pub current_sideboard_snapshot_commitment_sha256: String,
    pub current_frame_sequence: u64,
    pub total_transfer_steps: u16,
    pub confirmed_transfer_steps: u16,
    pub next_transfer: MtgoAtomicCompetitiveSideboardTransferV1,
}

/// A deterministic one-card-at-a-time sideboard sequence. All transfers from
/// sideboard to mainboard are ordered first so every intermediate state keeps
/// the mainboard at or above its legal minimum and the sideboard at or below
/// its starting capacity.
pub struct OpaqueMtgoCompetitiveEventSideboardSequenceV1 {
    _spent_entry_authorization: RatifiedMtgoCompetitiveEntryAuthorizationV1,
    lifecycle_authorization: RatifiedMtgoCompetitiveLifecycleAuthorizationV1,
    sideboard_authorization: RatifiedMtgoCompetitiveSideboardAutomationAuthorizationV1,
    current_frame: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    manifest: ValidatedMtgoCompetitiveDeckManifestV1,
    plan: mtgo_blackbox_v1::CheckedUntrustedMtgoCompetitiveSideboardPlanV1,
    player_known_deck_state: MtgoCompetitivePlayerKnownDeckStateV1,
    current_configuration: MtgoCompetitiveDeckConfigurationV1,
    current_visible_cards: Vec<MtgoVisibleCompetitiveSideboardCardV1>,
    current_mainboard_zone: MtgoVisibleCompetitiveSideboardZoneV1,
    current_sideboard_zone: MtgoVisibleCompetitiveSideboardZoneV1,
    atomic_transfers: Vec<MtgoAtomicCompetitiveSideboardTransferV1>,
    event_monitor: Option<OpaqueMtgoCompetitiveEventMonitorV1>,
    prior: MtgoCompetitiveEventRuntimeCommitmentsV1,
    commitments: MtgoCompetitiveEventSideboardSequenceCommitmentsV1,
}

impl OpaqueMtgoCompetitiveEventSideboardSequenceV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveEventSideboardSequenceCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn next_transfer_v1(&self) -> MtgoAtomicCompetitiveSideboardTransferV1 {
        self.commitments.next_transfer.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_sideboard_submission_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoPreparedCompetitiveEventSideboardTransferCommitmentsV1 {
    pub sequence_chain_commitment_sha256: String,
    pub plan_commitment_sha256: String,
    pub source_sideboard_snapshot_commitment_sha256: String,
    pub transfer: MtgoAtomicCompetitiveSideboardTransferV1,
    pub source_card_region_sha256: String,
    pub destination_empty_drop_region_sha256: String,
    pub source_frame_sequence: u64,
    pub preparation_commitment_sha256: String,
}

/// One pixel-bound drag proposal for the next semantic transfer. The source
/// card and empty destination rectangles stay private. A separate immediate
/// recapture must reconstruct this proposal under the sideboard-automation
/// ratification before the one-drag executor can accept it.
pub struct OpaqueMtgoPreparedCompetitiveEventSideboardTransferV1 {
    sequence: OpaqueMtgoCompetitiveEventSideboardSequenceV1,
    _source_card: MtgoVisibleCompetitiveSideboardCardV1,
    _destination_zone: MtgoVisibleCompetitiveSideboardZoneV1,
    commitments: MtgoPreparedCompetitiveEventSideboardTransferCommitmentsV1,
}

impl OpaqueMtgoPreparedCompetitiveEventSideboardTransferV1 {
    pub fn commitments_v1(&self) -> MtgoPreparedCompetitiveEventSideboardTransferCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoFreshPreparedCompetitiveEventSideboardDragCommitmentsV1 {
    pub sideboard_automation_ratification_commitment_sha256: String,
    pub sequence_chain_commitment_sha256: String,
    pub plan_commitment_sha256: String,
    pub transfer: MtgoAtomicCompetitiveSideboardTransferV1,
    pub immediate_navigation_classification_commitment_sha256: String,
    pub immediate_sideboard_classification_commitment_sha256: String,
    pub immediate_sideboard_snapshot_commitment_sha256: String,
    pub immediate_source_card_region_sha256: String,
    pub immediate_destination_drop_region_sha256: String,
    pub immediate_frame_id: u64,
    pub immediate_frame_sequence: u64,
    pub immediate_captured_at_unix_millis: u128,
    pub fresh_drag_preparation_commitment_sha256: String,
}

/// One immediate recapture of the exact unchanged sideboard configuration,
/// with private source and destination drag points resolved from that frame.
/// Production construction remains impossible while the sideboard automation
/// ratification root is empty.
pub struct OpaqueMtgoFreshPreparedCompetitiveEventSideboardDragV1 {
    prepared: OpaqueMtgoPreparedCompetitiveEventSideboardTransferV1,
    pointer_target: MtgoCompetitiveSideboardDragPointerTargetV1,
    commitments: MtgoFreshPreparedCompetitiveEventSideboardDragCommitmentsV1,
}

impl OpaqueMtgoFreshPreparedCompetitiveEventSideboardDragV1 {
    pub fn commitments_v1(&self) -> MtgoFreshPreparedCompetitiveEventSideboardDragCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveEventSideboardDragInputReceiptCommitmentsV1 {
    pub fresh_drag_preparation_commitment_sha256: String,
    pub sideboard_automation_ratification_commitment_sha256: String,
    pub sequence_chain_commitment_sha256: String,
    pub transfer: MtgoAtomicCompetitiveSideboardTransferV1,
    pub input_receipt_sha256: String,
    pub source_frame_sequence: u64,
    pub input_sent_at_unix_millis: u128,
    pub emitted_mouse_record_count: u8,
    pub cursor_parked_outside_client: bool,
}

pub struct OpaqueMtgoPendingCompetitiveEventSideboardDragV1 {
    prepared: OpaqueMtgoFreshPreparedCompetitiveEventSideboardDragV1,
    commitments: MtgoCompetitiveEventSideboardDragInputReceiptCommitmentsV1,
}

impl OpaqueMtgoPendingCompetitiveEventSideboardDragV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveEventSideboardDragInputReceiptCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_next_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoConfirmedCompetitiveEventSideboardDragCommitmentsV1 {
    pub input_receipt_sha256: String,
    pub fresh_drag_preparation_commitment_sha256: String,
    pub prior_sequence_chain_commitment_sha256: String,
    pub resulting_sequence_or_ready_commitment_sha256: String,
    pub transfer: MtgoAtomicCompetitiveSideboardTransferV1,
    pub after_frame_sequence: u64,
    pub confirmation_commitment_sha256: String,
}

pub struct OpaqueMtgoConfirmedCompetitiveEventSideboardDragV1 {
    advance: MtgoCompetitiveEventSideboardTransferAdvanceV1,
    commitments: MtgoConfirmedCompetitiveEventSideboardDragCommitmentsV1,
}

impl OpaqueMtgoConfirmedCompetitiveEventSideboardDragV1 {
    pub fn commitments_v1(&self) -> MtgoConfirmedCompetitiveEventSideboardDragCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn into_advance_v1(self) -> MtgoCompetitiveEventSideboardTransferAdvanceV1 {
        self.advance
    }

    pub fn safe_for_next_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoReadyCompetitiveEventSideboardCommitmentsV1 {
    pub prior_event_runtime_commitment_sha256: String,
    pub event_plan_binding_commitment_sha256: String,
    pub plan_commitment_sha256: String,
    pub ready_commitment_sha256: String,
    pub event_ready_binding_commitment_sha256: String,
    pub final_sideboard_snapshot_commitment_sha256: String,
    pub final_frame_id: u64,
    pub final_frame_sequence: u64,
    pub confirmed_transfer_steps: u16,
}

/// Exact target configuration observed after every individual semantic drag.
/// It still cannot submit the deck or emit input.
pub struct OpaqueMtgoReadyCompetitiveEventSideboardV1 {
    _spent_entry_authorization: RatifiedMtgoCompetitiveEntryAuthorizationV1,
    lifecycle_authorization: RatifiedMtgoCompetitiveLifecycleAuthorizationV1,
    sideboard_authorization: RatifiedMtgoCompetitiveSideboardAutomationAuthorizationV1,
    current_frame: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    manifest: ValidatedMtgoCompetitiveDeckManifestV1,
    _ready: CheckedUntrustedMtgoCompetitiveSideboardReadyV1,
    player_known_deck_state: MtgoCompetitivePlayerKnownDeckStateV1,
    event_monitor: Option<OpaqueMtgoCompetitiveEventMonitorV1>,
    effective_prior: MtgoCompetitiveEventRuntimeCommitmentsV1,
    commitments: MtgoReadyCompetitiveEventSideboardCommitmentsV1,
}

impl OpaqueMtgoReadyCompetitiveEventSideboardV1 {
    pub fn commitments_v1(&self) -> MtgoReadyCompetitiveEventSideboardCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_sideboard_submission_v1(&self) -> bool {
        false
    }
}

pub enum MtgoCompetitiveEventSideboardTransferAdvanceV1 {
    AwaitingNext(Box<OpaqueMtgoCompetitiveEventSideboardSequenceV1>),
    ReadyToSubmit(Box<OpaqueMtgoReadyCompetitiveEventSideboardV1>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoPreparedCompetitiveEventLifecycleControlCommitmentsV1 {
    pub prior_event_runtime_commitment_sha256: String,
    pub lifecycle_preparation_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub action: MtgoCompetitiveLifecycleActionV1,
    pub source_frame_sequence: u64,
}

pub struct OpaqueMtgoPreparedCompetitiveEventLifecycleControlV1 {
    _spent_entry_authorization: RatifiedMtgoCompetitiveEntryAuthorizationV1,
    prepared: OpaqueMtgoPreparedCompetitiveLifecycleControlV1,
    player_known_deck_state: MtgoCompetitivePlayerKnownDeckStateV1,
    event_monitor: Option<OpaqueMtgoCompetitiveEventMonitorV1>,
    prior: MtgoCompetitiveEventRuntimeCommitmentsV1,
    commitments: MtgoPreparedCompetitiveEventLifecycleControlCommitmentsV1,
}

impl OpaqueMtgoPreparedCompetitiveEventLifecycleControlV1 {
    pub fn commitments_v1(&self) -> MtgoPreparedCompetitiveEventLifecycleControlCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoPendingCompetitiveEventLifecycleControlCommitmentsV1 {
    pub prior_event_runtime_commitment_sha256: String,
    pub lifecycle_input_receipt_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub action: MtgoCompetitiveLifecycleActionV1,
    pub input_sent_at_unix_millis: u128,
}

pub struct OpaqueMtgoPendingCompetitiveEventLifecycleControlV1 {
    _spent_entry_authorization: RatifiedMtgoCompetitiveEntryAuthorizationV1,
    pending: OpaqueMtgoPendingCompetitiveLifecycleControlV1,
    player_known_deck_state: MtgoCompetitivePlayerKnownDeckStateV1,
    event_monitor: Option<OpaqueMtgoCompetitiveEventMonitorV1>,
    prior: MtgoCompetitiveEventRuntimeCommitmentsV1,
    commitments: MtgoPendingCompetitiveEventLifecycleControlCommitmentsV1,
}

impl OpaqueMtgoPendingCompetitiveEventLifecycleControlV1 {
    pub fn commitments_v1(&self) -> MtgoPendingCompetitiveEventLifecycleControlCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_next_input_v1(&self) -> bool {
        false
    }
}

/// The visible pregame state of one exact League or Challenge game. This is
/// deliberately separate from `ObservationV5`: the native checkpoint has no
/// Keep, Mulligan, or London-bottoming action representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoCompetitivePregameStageV1 {
    MulliganChoice {
        prospective_keep_size: u8,
    },
    LondonBottoming {
        required_bottom_count: u8,
        selected_bottom_count: u8,
    },
    GameplayReady,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitivePregameObservationCommitmentsV1 {
    pub observation_commitment_sha256: String,
    pub source_capture_commitment_sha256: String,
    pub duel_perception_profile_commitment_sha256: String,
    pub duel_perception_profile_admission_commitment_sha256: String,
    pub classifier_runtime_commitment_sha256: String,
    pub pregame_evaluation_commitment_sha256: String,
    pub pregame_profile_admission_commitment_sha256: String,
    pub pregame_classification_commitment_sha256: String,
    pub visible_interaction_commitment_sha256: String,
    pub process_continuity_commitment_sha256: String,
    pub window_continuity_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub entry_authorization_sha256: String,
    pub event_identity_sha256: String,
    pub match_identity_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub stage: MtgoCompetitivePregameStageV1,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub captured_at_unix_millis: u128,
}

/// One evaluated visible pregame observation. No public constructor exists in
/// this tranche because the competitive duel-window pregame profile and its
/// classifier evaluation have not yet been captured and ratified. A future
/// producer must live behind that opaque in-process capture path.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitivePregameObservationV1;
/// let _forged = OpaqueMtgoCompetitivePregameObservationV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitivePregameObservationV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitivePregameObservationV1>();
/// ```
pub struct OpaqueMtgoCompetitivePregameObservationV1 {
    _classified_source: Option<OpaqueMtgoCompetitivePregameClassifiedSourceV1>,
    commitments: MtgoCompetitivePregameObservationCommitmentsV1,
}

enum OpaqueMtgoCompetitivePregameClassifiedSourceV1 {
    Frame(Box<OpaqueMtgoClassifiedCompetitivePregameFrameV1>),
}

impl OpaqueMtgoCompetitivePregameClassifiedSourceV1 {
    fn response_v1(&self) -> &mtgo_blackbox_v1::MtgoCompetitivePregameClassifierResponseV1 {
        match self {
            Self::Frame(value) => value.response_v1(),
        }
    }
}

impl OpaqueMtgoCompetitivePregameObservationV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitivePregameObservationCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn stage_v1(&self) -> MtgoCompetitivePregameStageV1 {
        self.commitments.stage
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveEventPregameSessionCommitmentsV1 {
    pub session_commitment_sha256: String,
    pub prior_session_commitment_sha256: Option<String>,
    pub event_runtime_commitment_sha256: String,
    pub match_launch_authorization_commitment_sha256: String,
    pub match_gameplay_authorization_commitment_sha256: String,
    pub mode_authorization_commitment_sha256: String,
    pub entry_ratification_commitment_sha256: String,
    pub entry_authorization_sha256: String,
    pub deck_manifest_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub event_identity_sha256: String,
    pub match_identity_sha256: String,
    pub process_continuity_commitment_sha256: String,
    pub window_continuity_commitment_sha256: String,
    pub duel_perception_profile_commitment_sha256: String,
    pub duel_perception_profile_admission_commitment_sha256: String,
    pub pregame_evaluation_commitment_sha256: String,
    pub pregame_profile_admission_commitment_sha256: String,
    pub initial_observation_commitment_sha256: String,
    pub current_observation_commitment_sha256: String,
    pub confirmed_bottom_history_commitment_sha256: String,
    pub current_model_context_binding_commitment_sha256: Option<String>,
    pub player_visible_public_context_commitment_sha256: Option<String>,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub current_stage: MtgoCompetitivePregameStageV1,
    pub checkout_event_frame_sequence: u64,
    pub initial_frame_sequence: u64,
    pub current_frame_sequence: u64,
    pub visible_transition_count: u64,
}

/// Move-only owner of the paid event runtime and exact attended match launch
/// while visible Mulligan and London-bottoming states are resolved. It exposes
/// no target, coordinate, scoring, or input conversion. The event coordinator
/// cannot advance until this session reaches an evaluated GameplayReady state.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveEventPregameSessionV1;
/// let _forged = OpaqueMtgoCompetitiveEventPregameSessionV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveEventPregameSessionV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveEventPregameSessionV1>();
/// ```
pub struct OpaqueMtgoCompetitiveEventPregameSessionV1 {
    runtime: OpaqueMtgoCompetitiveEventRuntimeV1,
    match_launch: RatifiedMtgoCompetitiveMatchLaunchV1,
    current_observation: OpaqueMtgoCompetitivePregameObservationV1,
    ordered_confirmed_bottom_slots: Vec<u8>,
    current_model_context: Option<OpaqueMtgoCompetitivePregamePublicContextWitnessV1>,
    player_visible_public_context: Option<MtgoCompetitivePregamePlayerVisiblePublicContextStateV1>,
    commitments: MtgoCompetitiveEventPregameSessionCommitmentsV1,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct MtgoCompetitivePregamePlayerVisiblePublicContextStateV1 {
    game_number: u8,
    play_draw: mtgo_blackbox_v1::MtgoCompetitivePregamePlayDrawV1,
    acting_player_games_won: u8,
    opponent_games_won: u8,
}

impl From<&crate::probe::MtgoClassifiedCompetitivePregameModelContextCommitmentsV1>
    for MtgoCompetitivePregamePlayerVisiblePublicContextStateV1
{
    fn from(
        value: &crate::probe::MtgoClassifiedCompetitivePregameModelContextCommitmentsV1,
    ) -> Self {
        Self {
            game_number: value.game_number,
            play_draw: value.play_draw,
            acting_player_games_won: value.acting_player_games_won,
            opponent_games_won: value.opponent_games_won,
        }
    }
}

impl OpaqueMtgoCompetitiveEventPregameSessionV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveEventPregameSessionCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn current_stage_v1(&self) -> MtgoCompetitivePregameStageV1 {
        self.commitments.current_stage
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoCompetitivePregameSelectedActionV1 {
    KeepOpeningHand,
    Mulligan {
        next_hand_size: u8,
    },
    SelectForBottom {
        card_slot: u8,
        visible_card_name: String,
    },
    SubmitBottoming,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "postcondition", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoCompetitivePregameExpectedPostconditionV1 {
    MulliganChoice {
        prospective_keep_size: u8,
    },
    LondonBottoming {
        required_bottom_count: u8,
        selected_bottom_count: u8,
        newly_selected_card_slot: Option<u8>,
    },
    GameplayReady,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitivePregameActionPlanCommitmentsV1 {
    pub action_plan_commitment_sha256: String,
    pub pregame_session_commitment_sha256: String,
    pub current_observation_commitment_sha256: String,
    pub pregame_classification_commitment_sha256: String,
    pub visible_interaction_commitment_sha256: String,
    pub heuristic_profile_commitment_sha256: String,
    pub heuristic_algorithm_commitment_sha256: String,
    pub heuristic_review_commitment_sha256: String,
    pub heuristic_admission_commitment_sha256: String,
    pub selected_control_visible_content_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub match_identity_sha256: String,
    pub game_number: u8,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub selected_action: MtgoCompetitivePregameSelectedActionV1,
    pub expected_postcondition: MtgoCompetitivePregameExpectedPostconditionV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveNativePregameCardV1 {
    pub card_slot: u8,
    pub visible_card_name: String,
    pub selected_for_bottom: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoCompetitiveNativePregameActionV1 {
    KeepOpeningHand,
    Mulligan { next_hand_size: u8 },
    SelectForBottom { card_slot: u8 },
    SubmitBottoming,
}

/// Exact checkpoint-facing game information for one visible competitive
/// pregame decision. Every field is information available to the seated
/// player through the client. Event, authorization, capture, classifier, and
/// checkpoint-lineage commitments remain in the opaque adapter envelope and
/// are deliberately absent here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveNativePregameModelInputV1 {
    pub game_number: u8,
    pub play_draw: mtgo_blackbox_v1::MtgoCompetitivePregamePlayDrawV1,
    pub acting_player_games_won: u8,
    pub opponent_games_won: u8,
    pub player_known_deck_configuration: crate::MtgoCompetitiveNativeSideboardConfigurationV1,
    pub stage: MtgoCompetitivePregameStageV1,
    pub prospective_keep_size: Option<u8>,
    pub required_bottom_count: u8,
    pub selected_bottom_count: u8,
    pub ordered_visible_cards: Vec<MtgoCompetitiveNativePregameCardV1>,
    pub ordered_confirmed_bottom_slots: Vec<u8>,
    pub ordered_actions: Vec<MtgoCompetitiveNativePregameActionV1>,
}

/// Move-only source-bound request for a future native pregame scorer. It owns
/// the paid-event pregame session and same-frame public model context. The
/// request can be inspected for scorer integration, but only a future opaque
/// checkpoint response may recover the session for a live action plan.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveNativePregameRequestV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveNativePregameRequestV1>();
/// ```
pub struct OpaqueMtgoCompetitiveNativePregameRequestV1 {
    _session: OpaqueMtgoCompetitiveEventPregameSessionV1,
    model_input: MtgoCompetitiveNativePregameModelInputV1,
    model_input_commitment_sha256: String,
    _request_binding_commitment_sha256: String,
}

impl OpaqueMtgoCompetitiveNativePregameRequestV1 {
    pub fn model_input_v1(&self) -> &MtgoCompetitiveNativePregameModelInputV1 {
        &self.model_input
    }

    pub fn model_input_commitment_sha256_v1(&self) -> &str {
        &self.model_input_commitment_sha256
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
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

/// One deck-specific deterministic pregame selection over the exact retained
/// classified frame. The selected control rectangle remains private. This
/// plan has no input method and cannot advance the event session.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitivePregameActionPlanV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitivePregameActionPlanV1>();
/// ```
pub struct OpaqueMtgoCompetitivePregameActionPlanV1 {
    _session: OpaqueMtgoCompetitiveEventPregameSessionV1,
    _heuristic: AdmittedMtgoCompetitivePregameHeuristicV1,
    _selected_control: MtgoCompetitivePregameVisibleControlV1,
    commitments: MtgoCompetitivePregameActionPlanCommitmentsV1,
}

impl OpaqueMtgoCompetitivePregameActionPlanV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitivePregameActionPlanCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn selected_action_v1(&self) -> &MtgoCompetitivePregameSelectedActionV1 {
        &self.commitments.selected_action
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoPreparedCompetitivePregameActionCommitmentsV1 {
    pub preparation_commitment_sha256: String,
    pub action_plan_commitment_sha256: String,
    pub pregame_session_commitment_sha256: String,
    pub fresh_classification_commitment_sha256: String,
    pub fresh_visible_interaction_commitment_sha256: String,
    pub selected_control_visible_content_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub match_identity_sha256: String,
    pub game_number: u8,
    pub planned_frame_id: u64,
    pub planned_frame_sequence: u64,
    pub fresh_frame_id: u64,
    pub fresh_frame_sequence: u64,
    pub fresh_captured_at_unix_millis: u128,
    pub selected_action: MtgoCompetitivePregameSelectedActionV1,
    pub expected_postcondition: MtgoCompetitivePregameExpectedPostconditionV1,
}

/// One action plan rechecked against the immediate next classified frame.
/// The exact current target is retained privately, but this value deliberately
/// does not implement the process input-target trait and cannot emit input.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPreparedCompetitivePregameActionV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoPreparedCompetitivePregameActionV1>();
/// ```
pub struct OpaqueMtgoPreparedCompetitivePregameActionV1 {
    _plan: OpaqueMtgoCompetitivePregameActionPlanV1,
    _fresh_classified_frame: OpaqueMtgoClassifiedCompetitivePregameFrameV1,
    _pointer_target: MtgoCompetitivePregamePointerTargetV1,
    commitments: MtgoPreparedCompetitivePregameActionCommitmentsV1,
}

impl OpaqueMtgoPreparedCompetitivePregameActionV1 {
    pub fn commitments_v1(&self) -> MtgoPreparedCompetitivePregameActionCommitmentsV1 {
        self.commitments.clone()
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCheckedCompetitivePregamePostconditionCommitmentsV1 {
    pub dry_run_commitment_sha256: String,
    pub preparation_commitment_sha256: String,
    pub action_plan_commitment_sha256: String,
    pub after_classification_commitment_sha256: String,
    pub after_visible_interaction_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub match_identity_sha256: String,
    pub game_number: u8,
    pub before_frame_sequence: u64,
    pub after_frame_id: u64,
    pub after_frame_sequence: u64,
    pub after_captured_at_unix_millis: u128,
    pub selected_action: MtgoCompetitivePregameSelectedActionV1,
    pub observed_postcondition: MtgoCompetitivePregameExpectedPostconditionV1,
}

/// Structurally checks the next visible classified state against a prepared
/// action's declared postcondition. It has no input receipt, claims no action
/// causality, and cannot release the shared input gate.
pub struct CheckedUntrustedMtgoCompetitivePregamePostconditionV1 {
    _prepared: OpaqueMtgoPreparedCompetitivePregameActionV1,
    _after_classified_frame: OpaqueMtgoClassifiedCompetitivePregameFrameV1,
    commitments: MtgoCheckedCompetitivePregamePostconditionCommitmentsV1,
}

impl CheckedUntrustedMtgoCompetitivePregamePostconditionV1 {
    pub fn commitments_v1(&self) -> MtgoCheckedCompetitivePregamePostconditionCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn claims_action_causality_v1(&self) -> bool {
        false
    }

    pub fn safe_for_next_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitivePregameInputReceiptCommitmentsV1 {
    pub input_receipt_sha256: String,
    pub preparation_commitment_sha256: String,
    pub action_plan_commitment_sha256: String,
    pub authorization_ratification_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub match_identity_sha256: String,
    pub game_number: u8,
    pub before_frame_id: u64,
    pub before_frame_sequence: u64,
    pub input_sent_at_unix_millis: u128,
    pub cursor_parked_outside_client: bool,
    pub selected_action: MtgoCompetitivePregameSelectedActionV1,
}

/// One exact competitive pregame click waiting for its action-specific visible
/// postcondition. Dropping it cannot release the process-wide input gate.
pub struct OpaqueMtgoPendingCompetitivePregameInputV1 {
    prepared: OpaqueMtgoPreparedCompetitivePregameActionV1,
    authorization: RatifiedMtgoCompetitivePregameAuthorizationV1,
    commitments: MtgoCompetitivePregameInputReceiptCommitmentsV1,
}

impl OpaqueMtgoPendingCompetitivePregameInputV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitivePregameInputReceiptCommitmentsV1 {
        self.commitments.clone()
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoConfirmedCompetitivePregameActionCommitmentsV1 {
    pub confirmation_receipt_sha256: String,
    pub input_receipt_sha256: String,
    pub preparation_commitment_sha256: String,
    pub authorization_ratification_commitment_sha256: String,
    pub postcondition_commitment_sha256: String,
    pub advanced_pregame_session_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub match_identity_sha256: String,
    pub game_number: u8,
    pub after_frame_id: u64,
    pub after_frame_sequence: u64,
    pub after_captured_at_unix_millis: u128,
    pub selected_action: MtgoCompetitivePregameSelectedActionV1,
    pub observed_postcondition: MtgoCompetitivePregameExpectedPostconditionV1,
}

/// One receipt-bound competitive pregame transition whose shared input gate
/// was released only after the exact visible postcondition. The advanced
/// move-only event pregame session is returned explicitly for the next step.
pub struct OpaqueMtgoConfirmedCompetitivePregameActionV1 {
    _authorization: RatifiedMtgoCompetitivePregameAuthorizationV1,
    session: OpaqueMtgoCompetitiveEventPregameSessionV1,
    commitments: MtgoConfirmedCompetitivePregameActionCommitmentsV1,
}

impl OpaqueMtgoConfirmedCompetitivePregameActionV1 {
    pub fn commitments_v1(&self) -> MtgoConfirmedCompetitivePregameActionCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn into_pregame_session_v1(self) -> OpaqueMtgoCompetitiveEventPregameSessionV1 {
        self.session
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompletedCompetitivePregameCommitmentsV1 {
    pub completion_receipt_sha256: String,
    pub pregame_session_commitment_sha256: String,
    pub final_observation_commitment_sha256: String,
    pub match_identity_sha256: String,
    pub game_number: u8,
    pub completion_frame_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveEventGameplayLeaseCommitmentsV1 {
    pub event_runtime_commitment_sha256: String,
    pub initial_game_session_commitment_sha256: String,
    pub gameplay_lease_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub match_identity_sha256: String,
    pub entry_ratification_commitment_sha256: String,
    pub selected_deck_label_sha256: String,
    pub selected_deck_region_sha256: String,
    pub deck_manifest_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub game_number: u8,
    pub checkout_frame_sequence: u64,
    pub initial_confirmed_action_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveEventMatchLaunchBindingCommitmentsV1 {
    pub event_runtime_commitment_sha256: String,
    pub source_launch_identity_commitment_sha256: String,
    pub match_launch_binding_commitment_sha256: String,
    pub entry_ratification_commitment_sha256: String,
    pub entry_authorization_sha256: String,
    pub selected_deck_label_sha256: String,
    pub selected_deck_region_sha256: String,
    pub deck_manifest_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub event_identity_sha256: String,
    pub match_identity_sha256: String,
    pub process_continuity_commitment_sha256: String,
    pub event_runtime_lifecycle_snapshot_commitment_sha256: String,
    pub launch_lifecycle_snapshot_commitment_sha256: String,
    pub launch_lifecycle_evaluation_commitment_sha256: String,
    pub launch_lifecycle_profile_admission_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub event_runtime_frame_id: u64,
    pub event_runtime_frame_sequence: u64,
    pub launch_frame_id: u64,
    pub launch_frame_sequence: u64,
    pub event_runtime_captured_at_unix_millis: u128,
    pub launch_captured_at_unix_millis: u128,
}

/// Move-only bridge from one exact paid event runtime to the visible identity
/// of its current match and game. It withholds the event runtime while the
/// owner reviews the attended launch, and it grants no input, entry, or
/// spending authority by itself.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveEventMatchLaunchBindingV1;
/// let _forged = OpaqueMtgoCompetitiveEventMatchLaunchBindingV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveEventMatchLaunchBindingV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveEventMatchLaunchBindingV1>();
/// ```
pub struct OpaqueMtgoCompetitiveEventMatchLaunchBindingV1 {
    runtime: OpaqueMtgoCompetitiveEventRuntimeV1,
    visible_identity: OpaqueMtgoCompetitiveLaunchIdentityV1,
    commitments: MtgoCompetitiveEventMatchLaunchBindingCommitmentsV1,
}

impl OpaqueMtgoCompetitiveEventMatchLaunchBindingV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveEventMatchLaunchBindingCommitmentsV1 {
        self.commitments.clone()
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

/// Move-only token withholding the event coordinator while the existing exact
/// game session is used. The event runtime cannot advance until this token is
/// reunited with that same session lineage.
pub struct OpaqueMtgoCompetitiveEventGameplayLeaseV1 {
    runtime: OpaqueMtgoCompetitiveEventRuntimeV1,
    commitments: MtgoCompetitiveEventGameplayLeaseCommitmentsV1,
}

impl OpaqueMtgoCompetitiveEventGameplayLeaseV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveEventGameplayLeaseCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoPreparedCompetitiveEntryCommitmentsV1 {
    pub entry_ratification_commitment_sha256: String,
    pub immediate_recapture: MtgoCompetitiveEntryImmediateRecaptureCommitmentsV1,
    pub preparation_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub resource: MtgoCompetitiveEntryResourceV1,
    pub amount: u32,
    pub immediate_frame_id: u64,
    pub immediate_frame_sequence: u64,
}

/// One exact separately ratified entry whose visible Entry Review state was
/// reacquired and reclassified immediately before the exact entry input
/// boundary. The complete authorization, coordinate-private control, and fresh
/// opaque frame remain retained. Only `execute_prepared_competitive_entry_v1`
/// may consume it, and that production-disabled path permits exactly one click
/// before withholding all later input pending visible confirmation.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPreparedCompetitiveEntryV1;
/// let _forged = OpaqueMtgoPreparedCompetitiveEntryV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPreparedCompetitiveEntryV1;
/// fn cannot_control(value: &OpaqueMtgoPreparedCompetitiveEntryV1) {
///     let _ = value.target_point_client_px();
///     let _ = value.send_input();
///     let _ = value.enter_event();
/// }
/// ```
pub struct OpaqueMtgoPreparedCompetitiveEntryV1 {
    _authorization: RatifiedMtgoCompetitiveEntryAuthorizationV1,
    _immediate_frame: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    pointer_target: MtgoCompetitiveEntryPointerTargetV1,
    commitments: MtgoPreparedCompetitiveEntryCommitmentsV1,
}

impl OpaqueMtgoPreparedCompetitiveEntryV1 {
    pub fn commitments_v1(&self) -> MtgoPreparedCompetitiveEntryCommitmentsV1 {
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveEntryInputReceiptCommitmentsV1 {
    pub preparation_commitment_sha256: String,
    pub entry_ratification_commitment_sha256: String,
    pub input_receipt_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub resource: MtgoCompetitiveEntryResourceV1,
    pub amount: u32,
    pub immediate_frame_id: u64,
    pub immediate_frame_sequence: u64,
    pub input_sent_at_unix_millis: u128,
    pub cursor_parked_outside_client: bool,
}

/// Proof that one exact-entry click was emitted and the shared process gate is
/// withholding all later input pending a visible entered-waiting result.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPendingCompetitiveEntryV1;
/// let _forged = OpaqueMtgoPendingCompetitiveEntryV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPendingCompetitiveEntryV1;
/// fn cannot_repeat(value: &OpaqueMtgoPendingCompetitiveEntryV1) {
///     let _ = value.send_input();
///     let _ = value.enter_another_event();
///     let _ = value.spend_again();
/// }
/// ```
pub struct OpaqueMtgoPendingCompetitiveEntryV1 {
    prepared: OpaqueMtgoPreparedCompetitiveEntryV1,
    commitments: MtgoCompetitiveEntryInputReceiptCommitmentsV1,
}

impl OpaqueMtgoPendingCompetitiveEntryV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveEntryInputReceiptCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_next_input_v1(&self) -> bool {
        false
    }

    pub fn permits_additional_spending_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoConfirmedCompetitiveEntryCommitmentsV1 {
    pub input_receipt_sha256: String,
    pub preparation_commitment_sha256: String,
    pub entry_ratification_commitment_sha256: String,
    pub visible_transition_commitment_sha256: String,
    pub visible_confirmation_commitment_sha256: String,
    pub confirmation_receipt_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub resource: MtgoCompetitiveEntryResourceV1,
    pub amount: u32,
    pub after_frame_id: u64,
    pub after_frame_sequence: u64,
    pub after_captured_at_unix_millis: u128,
    pub postcondition_candidate_count: u32,
}

/// One exact entry click whose strictly newer visible result is the matching
/// entered-and-waiting-for-pairing state. It has no authority for gameplay,
/// another entry, another resource spend, or another input.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoConfirmedCompetitiveEntryV1;
/// let _forged = OpaqueMtgoConfirmedCompetitiveEntryV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoConfirmedCompetitiveEntryV1;
/// fn cannot_continue(value: &OpaqueMtgoConfirmedCompetitiveEntryV1) {
///     let _ = value.send_input();
///     let _ = value.enter_another_event();
///     let _ = value.start_gameplay();
/// }
/// ```
pub struct OpaqueMtgoConfirmedCompetitiveEntryV1 {
    _authorization: RatifiedMtgoCompetitiveEntryAuthorizationV1,
    _visible_confirmation: OpaqueMtgoConfirmedCompetitiveEntryPostconditionV1,
    commitments: MtgoConfirmedCompetitiveEntryCommitmentsV1,
}

impl OpaqueMtgoConfirmedCompetitiveEntryV1 {
    pub fn commitments_v1(&self) -> MtgoConfirmedCompetitiveEntryCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn entry_visibly_confirmed_v1(&self) -> bool {
        true
    }

    pub fn safe_for_gameplay_input_v1(&self) -> bool {
        false
    }

    pub fn permits_additional_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_additional_spending_v1(&self) -> bool {
        false
    }
}

/// The exact coordinate-free facts shown to the account owner before they
/// review one visible League or Challenge entry. This request cannot enter the
/// event or spend the declared resources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoAttendedCompetitiveEntryReviewRequestV1 {
    schema_version: u32,
    event_kind: MtgoCompetitiveEventKindV1,
    event_display_label: String,
    source_lifecycle_snapshot_commitment_sha256: String,
    frame_id: u64,
    frame_sequence: u64,
    event_identity_sha256: String,
    entry_terms: MtgoCompetitiveEntryTermsV1,
    account_alias_sha256: String,
    correspondence_sha256: String,
    permission_review_commitment_sha256: String,
    mode_authorization_commitment_sha256: String,
}

struct MtgoAttendedCompetitiveEntryReviewPartsV1 {
    entry_authorization: MtgoCompetitiveEntryAuthorizationV1,
    commitments: MtgoAttendedCompetitiveEntryReviewCommitmentsV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoAttendedCompetitiveEntryReviewCommitmentsV1 {
    pub source_lifecycle_snapshot_commitment_sha256: String,
    pub permission_review_commitment_sha256: String,
    pub mode_authorization_commitment_sha256: String,
    pub request_commitment_sha256: String,
    pub entry_authorization_sha256: String,
    pub owner_review_receipt_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub resource: MtgoCompetitiveEntryResourceV1,
    pub amount: u32,
}

/// One owner-attended review of the exact event and entry terms reported by a
/// checked visible lifecycle snapshot. The snapshot remains checked-untrusted,
/// so this value deliberately grants no event-entry, spending, or input
/// authority. A future live entry seam must additionally bind an opaque,
/// admitted navigation capture to these same facts.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoAttendedCompetitiveEntryReviewV1;
/// let _forged = CheckedUntrustedMtgoAttendedCompetitiveEntryReviewV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoAttendedCompetitiveEntryReviewV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<CheckedUntrustedMtgoAttendedCompetitiveEntryReviewV1>();
/// ```
pub struct CheckedUntrustedMtgoAttendedCompetitiveEntryReviewV1 {
    _source: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    entry_authorization: MtgoCompetitiveEntryAuthorizationV1,
    commitments: MtgoAttendedCompetitiveEntryReviewCommitmentsV1,
}

impl CheckedUntrustedMtgoAttendedCompetitiveEntryReviewV1 {
    pub fn commitments_v1(&self) -> MtgoAttendedCompetitiveEntryReviewCommitmentsV1 {
        self.commitments.clone()
    }

    /// Returns coordinate-free data for validating a future offline lifecycle
    /// intent. The record alone is not authority to enter an event.
    pub fn entry_authorization_record_v1(&self) -> MtgoCompetitiveEntryAuthorizationV1 {
        self.entry_authorization.clone()
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
pub struct MtgoSourceBoundCompetitiveEntryReviewCommitmentsV2 {
    pub source_identity_commitment_sha256: String,
    pub source_capture_commitment_sha256: String,
    pub source_navigation_classification_result_commitment_sha256: Option<String>,
    pub attended_review: MtgoAttendedCompetitiveEntryReviewCommitmentsV1,
}

/// One attended entry review whose lifecycle interpretation has already been
/// bound to a retained opaque composed-desktop navigation frame. It remains
/// checked-untrusted and non-actionable because source binding does not prove
/// classifier accuracy or resolve the transient-occluder race.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2;
/// let _forged = CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2;
/// fn require_clone<T: Clone>() {}
/// require_clone::<CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2>();
/// ```
pub struct CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2 {
    _source_identity: OpaqueMtgoCompetitiveEntryReviewIdentityV1,
    entry_authorization: MtgoCompetitiveEntryAuthorizationV1,
    commitments: MtgoSourceBoundCompetitiveEntryReviewCommitmentsV2,
}

impl CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2 {
    pub fn commitments_v2(&self) -> MtgoSourceBoundCompetitiveEntryReviewCommitmentsV2 {
        self.commitments.clone()
    }

    pub fn entry_authorization_record_v2(&self) -> MtgoCompetitiveEntryAuthorizationV1 {
        self.entry_authorization.clone()
    }

    pub fn safe_for_live_input_v2(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v2(&self) -> bool {
        false
    }

    pub fn permits_spending_v2(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3 {
    pub source_navigation_classification_result_commitment_sha256: String,
    pub classifier_bound_review_commitment_sha256: String,
    pub source_bound_review: MtgoSourceBoundCompetitiveEntryReviewCommitmentsV2,
}

/// One attended entry review whose visible frame was interpreted by the exact
/// profile-pinned classifier retained inside the source identity. A stricter
/// control-bound v4 review adds the required visible Confirm Entry control.
///
/// It remains checked-untrusted and deliberately grants no Join, spending, or
/// input authority. The account owner must separately authorize any eventual
/// entry action and exact resource spend.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3;
/// let _forged = CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3;
/// fn require_clone<T: Clone>() {}
/// require_clone::<CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3>();
/// ```
pub struct CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3 {
    _source_bound_review: CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2,
    entry_authorization: MtgoCompetitiveEntryAuthorizationV1,
    commitments: MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3,
}

impl CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3 {
    pub fn commitments_v3(&self) -> MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3 {
        self.commitments.clone()
    }

    pub fn entry_authorization_record_v3(&self) -> MtgoCompetitiveEntryAuthorizationV1 {
        self.entry_authorization.clone()
    }

    pub fn safe_for_live_input_v3(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v3(&self) -> bool {
        false
    }

    pub fn permits_spending_v3(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoControlBoundCompetitiveEntryReviewCommitmentsV4 {
    pub control_bound_review_commitment_sha256: String,
    pub deck_review_receipt_sha256: String,
    pub classifier_bound_review: MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3,
    pub entry_control_dry_run: MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1,
}

/// One owner-attended entry review bound to a coordinate-private, visibly
/// enabled Confirm Entry control on the exact classifier-backed source frame.
/// This is the most specific dry-run type and the only review type a future
/// event-entry actuator may accept.
///
/// It still grants no Join, spending, or input authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4;
/// let _forged = CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4;
/// fn require_clone<T: Clone>() {}
/// require_clone::<CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4>();
/// ```
pub struct CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4 {
    _classifier_bound_review: CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3,
    _visible_control_label: String,
    _control_rect_client_px: mtgo_blackbox_v1::MtgoRectPxV1,
    _selected_deck_label: String,
    _selected_deck_rect_client_px: mtgo_blackbox_v1::MtgoRectPxV1,
    entry_authorization: MtgoCompetitiveEntryAuthorizationV1,
    commitments: MtgoControlBoundCompetitiveEntryReviewCommitmentsV4,
}

impl CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4 {
    pub fn commitments_v4(&self) -> MtgoControlBoundCompetitiveEntryReviewCommitmentsV4 {
        self.commitments.clone()
    }

    pub fn entry_authorization_record_v4(&self) -> MtgoCompetitiveEntryAuthorizationV1 {
        self.entry_authorization.clone()
    }

    pub fn safe_for_live_input_v4(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v4(&self) -> bool {
        false
    }

    pub fn permits_spending_v4(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveEntryPostconditionDryRunCommitmentsV1 {
    pub control_bound_review_commitment_sha256: String,
    pub entry_authorization_sha256: String,
    pub frame_transition: MtgoCompetitiveEntryFrameTransitionCommitmentsV1,
    pub postcondition_dry_run_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub resource: MtgoCompetitiveEntryResourceV1,
    pub amount: u32,
}

/// One exact classifier-backed entry-review frame paired with a strictly newer
/// entered-waiting frame from the same account, client process, window,
/// geometry, output, profile, classifier runtime, mode, and event identity.
///
/// This is calibration evidence only. No input occurred between the retained
/// frames, so the pair does not claim that a Join action caused the transition.
/// It exposes no pixels or coordinates and grants no entry, spending, or input
/// authority.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoCompetitiveEntryPostconditionDryRunV1;
/// let _forged = CheckedUntrustedMtgoCompetitiveEntryPostconditionDryRunV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoCompetitiveEntryPostconditionDryRunV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<CheckedUntrustedMtgoCompetitiveEntryPostconditionDryRunV1>();
/// ```
pub struct CheckedUntrustedMtgoCompetitiveEntryPostconditionDryRunV1 {
    _source_review: CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4,
    _entered_waiting_frame: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    commitments: MtgoCompetitiveEntryPostconditionDryRunCommitmentsV1,
}

impl CheckedUntrustedMtgoCompetitiveEntryPostconditionDryRunV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveEntryPostconditionDryRunCommitmentsV1 {
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

    pub fn claims_action_causality_v1(&self) -> bool {
        false
    }
}

/// A separately ratified owner launch for one exact competitive event, match,
/// game, entry record, and gameplay-authorization lifetime. Its production
/// trust root is independent from general Daybreak mode permission.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::RatifiedMtgoCompetitiveMatchLaunchV1;
/// let _forged = RatifiedMtgoCompetitiveMatchLaunchV1 {};
/// ```
pub struct RatifiedMtgoCompetitiveMatchLaunchV1 {
    #[allow(dead_code)]
    authorization: MtgoCompetitiveMatchGameplayAuthorizationV1,
    mode_authorization_commitment_sha256: String,
    gameplay_authorization_commitment_sha256: String,
    launch_authorization_commitment_sha256: String,
    valid_from_frame_sequence: u64,
}

impl RatifiedMtgoCompetitiveMatchLaunchV1 {
    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.authorization.event_kind
    }

    pub fn game_number_v1(&self) -> u8 {
        self.authorization.game_number
    }

    pub fn mode_authorization_commitment_sha256_v1(&self) -> &str {
        &self.mode_authorization_commitment_sha256
    }

    pub fn gameplay_authorization_commitment_sha256_v1(&self) -> &str {
        &self.gameplay_authorization_commitment_sha256
    }

    pub fn launch_authorization_commitment_sha256_v1(&self) -> &str {
        &self.launch_authorization_commitment_sha256
    }

    pub fn valid_from_frame_sequence_v2(&self) -> u64 {
        self.valid_from_frame_sequence
    }

    pub fn valid_through_frame_sequence_v1(&self) -> u64 {
        self.authorization.valid_through_frame_sequence
    }

    pub fn owner_launch_authorization_sha256_v2(&self) -> &str {
        &self.authorization.owner_launch_authorization_sha256
    }

    /// Returns the coordinate-free authorization record needed when building
    /// the exact visible match plan. The record alone grants no input authority.
    pub fn gameplay_authorization_record_v2(&self) -> MtgoCompetitiveMatchGameplayAuthorizationV1 {
        self.authorization.clone()
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

/// An attended extension of one exact priority-Pass match launch to the
/// complete reviewed eleven-family gesture profile. It owns both independent
/// ratifications and the original exact-game owner launch. It grants no event
/// entry or spending authority and has no input conversion.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::RatifiedMtgoCompetitiveGestureMatchLaunchV1;
/// let _forged = RatifiedMtgoCompetitiveGestureMatchLaunchV1 {};
/// ```
pub struct RatifiedMtgoCompetitiveGestureMatchLaunchV1 {
    gesture_authorization: RatifiedMtgoCompetitiveDuelGestureAuthorizationV1,
    pass_match_launch: RatifiedMtgoCompetitiveMatchLaunchV1,
    gesture_match_launch_commitment_sha256: String,
}

impl RatifiedMtgoCompetitiveGestureMatchLaunchV1 {
    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.pass_match_launch.authorization.event_kind
    }

    pub fn game_number_v1(&self) -> u8 {
        self.pass_match_launch.authorization.game_number
    }

    pub fn mode_authorization_commitment_sha256_v1(&self) -> &str {
        &self
            .gesture_authorization
            .mode_authorization_commitment_sha256
    }

    pub fn general_gesture_permission_commitment_sha256_v1(&self) -> &str {
        &self.gesture_authorization.authorization_commitment_sha256
    }

    pub fn pass_match_launch_commitment_sha256_v1(&self) -> &str {
        &self
            .pass_match_launch
            .launch_authorization_commitment_sha256
    }

    pub fn gesture_match_launch_commitment_sha256_v1(&self) -> &str {
        &self.gesture_match_launch_commitment_sha256
    }

    pub fn gesture_evaluation_commitment_sha256_v1(&self) -> &str {
        &self
            .gesture_authorization
            .gesture_evaluation_commitment_sha256
    }

    pub fn gesture_profile_admission_commitment_sha256_v1(&self) -> &str {
        &self
            .gesture_authorization
            .gesture_profile_admission_commitment_sha256
    }

    pub fn valid_from_frame_sequence_v1(&self) -> u64 {
        self.pass_match_launch.valid_from_frame_sequence
    }

    pub fn valid_through_frame_sequence_v1(&self) -> u64 {
        self.pass_match_launch
            .authorization
            .valid_through_frame_sequence
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveGestureGameSessionCommitmentsV1 {
    pub session_commitment_sha256: String,
    pub general_gesture_permission_commitment_sha256: String,
    pub mode_authorization_commitment_sha256: String,
    pub correspondence_sha256: String,
    pub permission_review_commitment_sha256: String,
    pub pass_match_launch_commitment_sha256: String,
    pub match_gameplay_authorization_commitment_sha256: String,
    pub gesture_match_launch_commitment_sha256: String,
    pub gesture_evaluation_commitment_sha256: String,
    pub gesture_profile_admission_commitment_sha256: String,
    pub entry_ratification_commitment_sha256: Option<String>,
    pub selected_deck_label_sha256: Option<String>,
    pub selected_deck_region_sha256: Option<String>,
    pub deck_manifest_sha256: Option<String>,
    pub deck_format_sha256: Option<String>,
    pub policy_deployment_commitment_sha256: Option<String>,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub valid_from_frame_sequence: u64,
    pub valid_through_frame_sequence: u64,
    pub last_confirmed_frame_sequence: u64,
    pub confirmed_action_count: u64,
}

/// Move-only all-family session identity for one exact already-entered League
/// or Challenge game. This tranche creates no preparation, execution, or
/// advancement method. A future actuator must consume the session and return
/// it only after each exact visible postcondition.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveGestureGameSessionV1;
/// let _forged = OpaqueMtgoCompetitiveGestureGameSessionV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveGestureGameSessionV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveGestureGameSessionV1>();
/// ```
pub struct OpaqueMtgoCompetitiveGestureGameSessionV1 {
    launch: RatifiedMtgoCompetitiveGestureMatchLaunchV1,
    session_commitment_sha256: String,
    last_confirmed_frame_sequence: u64,
    confirmed_action_count: u64,
    entry_ratification_commitment_sha256: Option<String>,
    selected_deck_label_sha256: Option<String>,
    selected_deck_region_sha256: Option<String>,
    deck_manifest_sha256: Option<String>,
    deck_format_sha256: Option<String>,
    policy_deployment_commitment_sha256: Option<String>,
}

impl OpaqueMtgoCompetitiveGestureGameSessionV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveGestureGameSessionCommitmentsV1 {
        MtgoCompetitiveGestureGameSessionCommitmentsV1 {
            session_commitment_sha256: self.session_commitment_sha256.clone(),
            general_gesture_permission_commitment_sha256: self
                .launch
                .gesture_authorization
                .authorization_commitment_sha256
                .clone(),
            mode_authorization_commitment_sha256: self
                .launch
                .gesture_authorization
                .mode_authorization_commitment_sha256
                .clone(),
            correspondence_sha256: self
                .launch
                .gesture_authorization
                .scope
                .written_permission_sha256
                .clone(),
            permission_review_commitment_sha256: self
                .launch
                .gesture_authorization
                .permission_review_commitment_sha256
                .clone(),
            pass_match_launch_commitment_sha256: self
                .launch
                .pass_match_launch
                .launch_authorization_commitment_sha256
                .clone(),
            match_gameplay_authorization_commitment_sha256: self
                .launch
                .pass_match_launch
                .gameplay_authorization_commitment_sha256
                .clone(),
            gesture_match_launch_commitment_sha256: self
                .launch
                .gesture_match_launch_commitment_sha256
                .clone(),
            gesture_evaluation_commitment_sha256: self
                .launch
                .gesture_authorization
                .gesture_evaluation_commitment_sha256
                .clone(),
            gesture_profile_admission_commitment_sha256: self
                .launch
                .gesture_authorization
                .gesture_profile_admission_commitment_sha256
                .clone(),
            entry_ratification_commitment_sha256: self.entry_ratification_commitment_sha256.clone(),
            selected_deck_label_sha256: self.selected_deck_label_sha256.clone(),
            selected_deck_region_sha256: self.selected_deck_region_sha256.clone(),
            deck_manifest_sha256: self.deck_manifest_sha256.clone(),
            deck_format_sha256: self.deck_format_sha256.clone(),
            policy_deployment_commitment_sha256: self.policy_deployment_commitment_sha256.clone(),
            event_kind: self.launch.pass_match_launch.authorization.event_kind,
            game_number: self.launch.pass_match_launch.authorization.game_number,
            valid_from_frame_sequence: self.launch.pass_match_launch.valid_from_frame_sequence,
            valid_through_frame_sequence: self
                .launch
                .pass_match_launch
                .authorization
                .valid_through_frame_sequence,
            last_confirmed_frame_sequence: self.last_confirmed_frame_sequence,
            confirmed_action_count: self.confirmed_action_count,
        }
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

#[allow(dead_code)]
pub(crate) fn competitive_gesture_game_session_action_authorities_v1(
    session: &OpaqueMtgoCompetitiveGestureGameSessionV1,
) -> (
    MtgoAuthorizationScopeV1,
    MtgoCompetitiveMatchGameplayAuthorizationV1,
) {
    (
        session.launch.gesture_authorization.scope.clone(),
        session.launch.pass_match_launch.authorization.clone(),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoSessionBoundCompetitiveDuelGestureCommitmentsV1 {
    pub binding_commitment_sha256: String,
    pub game_session_commitment_sha256: String,
    pub gesture_match_launch_commitment_sha256: String,
    pub gesture_evaluation_commitment_sha256: String,
    pub gesture_profile_admission_commitment_sha256: String,
    pub entry_ratification_commitment_sha256: String,
    pub selected_deck_label_sha256: String,
    pub selected_deck_region_sha256: String,
    pub deck_manifest_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub competitive_action_plan_commitment_sha256: String,
    pub gesture_plan_commitment_sha256: String,
    pub gesture_sequence_commitment_sha256: String,
    pub selected_action_family: MtgoDuelActionFamilyV1,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub source_frame_sequence: u64,
    pub gesture_stage_count: u16,
}

/// One source-stage gesture sequence joined to the move-only all-family game
/// session for the exact same mode, game, gameplay record, and frame lifetime.
/// It keeps the session and private target points opaque. It cannot prepare or
/// execute input.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoSessionBoundCompetitiveDuelGestureV1;
/// fn cannot_act(value: OpaqueMtgoSessionBoundCompetitiveDuelGestureV1) {
///     let _ = value.target_points_desktop_px();
///     let _ = value.send_input();
/// }
/// ```
pub struct OpaqueMtgoSessionBoundCompetitiveDuelGestureV1 {
    _session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    _sequence: OpaqueMtgoCompetitiveDuelGestureSequenceV1,
    commitments: MtgoSessionBoundCompetitiveDuelGestureCommitmentsV1,
}

impl OpaqueMtgoSessionBoundCompetitiveDuelGestureV1 {
    pub fn commitments_v1(&self) -> MtgoSessionBoundCompetitiveDuelGestureCommitmentsV1 {
        self.commitments.clone()
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoPreparedCompetitiveDuelGestureSourceStageCommitmentsV1 {
    pub preparation_binding_commitment_sha256: String,
    pub session_sequence_binding_commitment_sha256: String,
    pub game_session_commitment_sha256: String,
    pub gesture_match_launch_commitment_sha256: String,
    pub source_sequence_commitment_sha256: String,
    pub prepared_sequence_commitment_sha256: String,
    pub competitive_action_plan_commitment_sha256: String,
    pub gesture_plan_commitment_sha256: String,
    pub fresh_stage_binding_commitment_sha256: String,
    pub fresh_capture_commitment_sha256: String,
    pub fresh_perception_result_commitment_sha256: String,
    pub gesture_target_runtime_identity_commitment_sha256: String,
    pub gesture_target_request_commitment_sha256: String,
    pub before_input_postcondition_verification_commitment_sha256: String,
    pub primitive_commitment_sha256: String,
    pub selected_action_family: MtgoDuelActionFamilyV1,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub stage_index: u16,
    pub gesture_stage_count: u16,
    pub target_count: u16,
    pub fresh_frame_id: u64,
    pub fresh_frame_sequence: u64,
    pub fresh_captured_at_unix_millis: u128,
}

/// One session-bound source primitive rechecked against a distinct next-frame
/// opaque perception and the response from the exact profile-pinned gesture
/// target runtime. This wrapper has no input method and exposes no target
/// points.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1;
/// fn cannot_act(value: OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1) {
///     let _ = value.target_points_desktop_px();
///     let _ = value.send_input();
/// }
/// ```
pub struct OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1 {
    _session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    _prepared: ProbeOpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1,
    commitments: MtgoPreparedCompetitiveDuelGestureSourceStageCommitmentsV1,
}

impl OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1 {
    pub fn commitments_v1(&self) -> MtgoPreparedCompetitiveDuelGestureSourceStageCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn target_runtime_attested_v1(&self) -> bool {
        true
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoPendingCompetitiveDuelGesturePrimitiveCommitmentsV1 {
    pub preparation_binding_commitment_sha256: String,
    pub gesture_target_runtime_identity_commitment_sha256: String,
    pub gesture_target_request_commitment_sha256: String,
    pub before_input_postcondition_verification_commitment_sha256: String,
    pub input_receipt_sha256: String,
    pub selected_action_family: MtgoDuelActionFamilyV1,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub stage_index: u16,
    pub gesture_stage_count: u16,
    pub before_frame_id: u64,
    pub before_frame_sequence: u64,
    pub input_sent_at_unix_millis: u128,
    pub emitted_mouse_record_count: u8,
    pub cursor_parked_outside_client: bool,
}

/// Exactly one emitted primitive for one competitive gesture stage. The
/// process gate remains locked until the declared visible postcondition is
/// confirmed. This type exposes no coordinates, window handle, or input API.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPendingCompetitiveDuelGesturePrimitiveV1;
/// let _forged = OpaqueMtgoPendingCompetitiveDuelGesturePrimitiveV1 {};
/// ```
pub struct OpaqueMtgoPendingCompetitiveDuelGesturePrimitiveV1 {
    prepared: OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1,
    commitments: MtgoPendingCompetitiveDuelGesturePrimitiveCommitmentsV1,
}

impl OpaqueMtgoPendingCompetitiveDuelGesturePrimitiveV1 {
    pub fn commitments_v1(&self) -> MtgoPendingCompetitiveDuelGesturePrimitiveCommitmentsV1 {
        self.commitments.clone()
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoPendingCompetitivePlayerVisibleGameplayPrimitiveCommitmentsV1 {
    pub before_input_commitment_sha256: String,
    pub actuator_authority_binding_sha256: String,
    pub input_receipt_sha256: String,
    pub selected_action_family: MtgoDuelActionFamilyV1,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub primitive_index: u16,
    pub primitive_is_final: bool,
    pub before_frame_id: u64,
    pub before_frame_sequence: u64,
    pub input_sent_at_unix_millis: u128,
    pub emitted_mouse_record_count: u8,
    pub cursor_parked_outside_client: bool,
}

/// Exactly one emitted primitive from the player-visible-only gameplay path.
/// The process gate remains locked until a newer exact visible postcondition
/// is confirmed. This value exposes no coordinates, pixels, process identity,
/// client-native object identity, or input API.
pub struct OpaqueMtgoPendingCompetitivePlayerVisibleGameplayPrimitiveV1 {
    before: OpaqueMtgoPreparedPlayerVisibleGameplayBeforeInputV1,
    input_receipt: crate::probe::OpaqueMtgoPlayerVisibleGameplayInputReceiptV1,
    commitments: MtgoPendingCompetitivePlayerVisibleGameplayPrimitiveCommitmentsV1,
}

impl OpaqueMtgoPendingCompetitivePlayerVisibleGameplayPrimitiveV1 {
    pub fn commitments_v1(
        &self,
    ) -> MtgoPendingCompetitivePlayerVisibleGameplayPrimitiveCommitmentsV1 {
        self.commitments.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoConfirmedCompetitiveDuelGestureContinuationCommitmentsV1 {
    pub input_receipt_sha256: String,
    pub preparation_binding_commitment_sha256: String,
    pub prepared_sequence_commitment_sha256: String,
    pub prior_sequence_commitment_sha256: String,
    pub advanced_sequence_commitment_sha256: String,
    pub visible_transition_commitment_sha256: String,
    pub continuation_receipt_sha256: String,
    pub gesture_target_runtime_identity_commitment_sha256: String,
    pub next_stage_target_request_commitment_sha256: String,
    pub unchanged_game_session_commitment_sha256: String,
    pub selected_action_family: MtgoDuelActionFamilyV1,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub completed_stage_index: u16,
    pub next_stage_index: u16,
    pub gesture_stage_count: u16,
    pub before_frame_id: u64,
    pub before_frame_sequence: u64,
    pub after_frame_id: u64,
    pub after_frame_sequence: u64,
    pub after_captured_at_unix_millis: u128,
    pub next_stage_target_count: u16,
}

/// One non-final primitive joined to a strictly newer, runtime-pinned visible
/// continuation. The game session is retained but not advanced because the
/// selected semantic is still incomplete. No next primitive can be emitted
/// from this value.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoConfirmedCompetitiveDuelGestureContinuationV1;
/// fn cannot_act(value: OpaqueMtgoConfirmedCompetitiveDuelGestureContinuationV1) {
///     let _ = value.send_input();
/// }
/// ```
pub struct OpaqueMtgoConfirmedCompetitiveDuelGestureContinuationV1 {
    _continuation: OpaqueMtgoPinnedCompetitiveDuelGestureContinuationV1,
    _session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    commitments: MtgoConfirmedCompetitiveDuelGestureContinuationCommitmentsV1,
}

impl OpaqueMtgoConfirmedCompetitiveDuelGestureContinuationV1 {
    pub fn commitments_v1(&self) -> MtgoConfirmedCompetitiveDuelGestureContinuationCommitmentsV1 {
        self.commitments.clone()
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoConfirmedCompetitiveDuelGesturePrimitiveCommitmentsV1 {
    pub input_receipt_sha256: String,
    pub visible_postcondition_commitment_sha256: String,
    pub transition_receipt_sha256: String,
    pub prior_game_session_commitment_sha256: String,
    pub advanced_game_session_commitment_sha256: String,
    pub selected_action_family: MtgoDuelActionFamilyV1,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub after_frame_id: u64,
    pub after_frame_sequence: u64,
    pub postcondition_candidate_count: u32,
    pub confirmed_action_count: u64,
}

/// One final gesture stage whose exact newer visible postcondition was
/// confirmed. Consuming this value is the only way to recover the all-family
/// game session for another action.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoConfirmedCompetitiveDuelGesturePrimitiveV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoConfirmedCompetitiveDuelGesturePrimitiveV1>();
/// ```
pub struct OpaqueMtgoConfirmedCompetitiveDuelGesturePrimitiveV1 {
    confirmation: OpaqueMtgoConfirmedCompetitiveDuelGestureV1,
    session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    commitments: MtgoConfirmedCompetitiveDuelGesturePrimitiveCommitmentsV1,
}

impl OpaqueMtgoConfirmedCompetitiveDuelGesturePrimitiveV1 {
    pub fn commitments_v1(&self) -> MtgoConfirmedCompetitiveDuelGesturePrimitiveCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn into_game_session_v1(self) -> OpaqueMtgoCompetitiveGestureGameSessionV1 {
        self.session
    }

    /// Starts the semantic decision-history ledger from this first exact
    /// input and confirmed visible postcondition while returning the advanced
    /// exact-game session. Public Game Log events are joined separately.
    pub fn into_initial_player_visible_history_v1(
        self,
        history_id: &str,
    ) -> Result<
        (
            OpaqueMtgoCompetitiveGestureGameSessionV1,
            CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
        ),
        String,
    > {
        let history = begin_checked_untrusted_competitive_player_visible_game_history_v1(
            history_id,
            self.confirmation.into_checked_postcondition_v1(),
        )
        .map_err(|error| format!("begin competitive player-visible history: {error}"))?;
        Ok((self.session, history))
    }

    /// Appends this exact confirmed action to the existing semantic history
    /// ledger and returns the advanced exact-game session.
    pub fn into_appended_player_visible_history_v1(
        self,
        history: CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
    ) -> Result<
        (
            OpaqueMtgoCompetitiveGestureGameSessionV1,
            CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
        ),
        String,
    > {
        let history = append_checked_untrusted_competitive_player_visible_game_history_v1(
            history,
            self.confirmation.into_checked_postcondition_v1(),
        )
        .map_err(|error| format!("append competitive player-visible history: {error}"))?;
        Ok((self.session, history))
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoCompetitiveGameSessionCommitmentsV1 {
    pub session_commitment_sha256: String,
    pub general_permission_commitment_sha256: String,
    pub match_launch_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub valid_from_frame_sequence: u64,
    pub valid_through_frame_sequence: u64,
    pub last_confirmed_frame_sequence: u64,
    pub confirmed_action_count: u64,
}

/// Move-only authority for sequential visible actions in one exact already
/// entered League or Challenge game. It owns both independent ratifications.
/// The session is returned to the caller only after an emitted action has a
/// newer visible postcondition and the shared input gate is released.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveGameSessionV1;
/// let _forged = OpaqueMtgoCompetitiveGameSessionV1 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoCompetitiveGameSessionV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoCompetitiveGameSessionV1>();
/// ```
pub struct OpaqueMtgoCompetitiveGameSessionV1 {
    authorization: RatifiedMtgoCompetitiveDuelPassAuthorizationV1,
    match_launch: RatifiedMtgoCompetitiveMatchLaunchV1,
    session_commitment_sha256: String,
    last_confirmed_frame_sequence: u64,
    confirmed_action_count: u64,
}

impl OpaqueMtgoCompetitiveGameSessionV1 {
    pub fn commitments_v1(&self) -> MtgoCompetitiveGameSessionCommitmentsV1 {
        MtgoCompetitiveGameSessionCommitmentsV1 {
            session_commitment_sha256: self.session_commitment_sha256.clone(),
            general_permission_commitment_sha256: self
                .authorization
                .authorization_commitment_sha256
                .clone(),
            match_launch_commitment_sha256: self
                .match_launch
                .launch_authorization_commitment_sha256
                .clone(),
            event_kind: self.match_launch.authorization.event_kind,
            game_number: self.match_launch.authorization.game_number,
            valid_from_frame_sequence: self.match_launch.valid_from_frame_sequence,
            valid_through_frame_sequence: self
                .match_launch
                .authorization
                .valid_through_frame_sequence,
            last_confirmed_frame_sequence: self.last_confirmed_frame_sequence,
            confirmed_action_count: self.confirmed_action_count,
        }
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

/// Coordinate-free facts shown to the account owner before authorizing
/// gameplay in one exact already-entered League or Challenge game. Event entry
/// and resource spending are deliberately outside this request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoAttendedCompetitiveMatchLaunchRequestV4 {
    pub schema_version: u32,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_display_label: String,
    pub opponent_display_name: String,
    pub visible_match_id: String,
    pub visible_game_id: String,
    pub event_identity_sha256: String,
    pub match_identity_sha256: String,
    pub game_number: u8,
    pub entry_authorization_sha256: String,
    pub observed_frame_sequence: u64,
    pub source_capture_commitment_sha256: String,
    pub source_perception_result_commitment_sha256: String,
    pub source_lifecycle_snapshot_commitment_sha256: String,
    pub source_window_title_sha256: String,
    pub source_event_label_region_sha256: String,
    pub source_launch_identity_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoAuthorizationBoundCompetitiveDuelPassCommitmentsV1 {
    pub preparation_commitment_sha256: String,
    pub mode_authorization_commitment_sha256: String,
    pub before_input_postcondition_verification_commitment_sha256: String,
    pub ratified_authorization_commitment_sha256: String,
    pub ratified_match_launch_commitment_sha256: String,
    pub competitive_match_gameplay_authorization_commitment_sha256: String,
    pub competitive_game_session_commitment_sha256: String,
    pub authorization_binding_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub immediate_frame_id: u64,
    pub immediate_frame_sequence: u64,
}

/// One prepared priority Pass joined to its separately ratified exact League
/// or Challenge authority. It still cannot send input or enter an event.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoAuthorizationBoundCompetitiveDuelPassV1;
/// fn cannot_act(value: &OpaqueMtgoAuthorizationBoundCompetitiveDuelPassV1) {
///     let _ = value.target_point_client_px();
///     let _ = value.send_input();
///     let _ = value.enter_event();
/// }
/// ```
pub struct OpaqueMtgoAuthorizationBoundCompetitiveDuelPassV1 {
    _prepared: OpaqueMtgoPreparedCompetitiveDuelPassV1,
    _session: OpaqueMtgoCompetitiveGameSessionV1,
    commitments: MtgoAuthorizationBoundCompetitiveDuelPassCommitmentsV1,
}

impl OpaqueMtgoAuthorizationBoundCompetitiveDuelPassV1 {
    pub fn commitments_v1(&self) -> MtgoAuthorizationBoundCompetitiveDuelPassCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

/// Proof that exactly one authorization-bound competitive Pass click was
/// emitted and the shared process gate is waiting for its exact visible
/// postcondition. It exposes no pixels, HWND, or coordinates.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPendingCompetitiveDuelPassV1;
/// let _forged = OpaqueMtgoPendingCompetitiveDuelPassV1 {};
/// ```
pub struct OpaqueMtgoPendingCompetitiveDuelPassV1 {
    prepared: OpaqueMtgoPreparedCompetitiveDuelPassV1,
    session: OpaqueMtgoCompetitiveGameSessionV1,
    authorization_binding_commitments: MtgoAuthorizationBoundCompetitiveDuelPassCommitmentsV1,
    input_receipt_sha256: String,
    input_sent_at_unix_millis: u128,
    cursor_parked_outside_client: bool,
}

impl OpaqueMtgoPendingCompetitiveDuelPassV1 {
    pub fn authorization_binding_commitment_sha256_v1(&self) -> &str {
        &self
            .authorization_binding_commitments
            .authorization_binding_commitment_sha256
    }

    pub fn input_receipt_sha256_v1(&self) -> &str {
        &self.input_receipt_sha256
    }

    pub fn input_sent_at_unix_millis_v1(&self) -> u128 {
        self.input_sent_at_unix_millis
    }

    pub fn cursor_parked_outside_client_v1(&self) -> bool {
        self.cursor_parked_outside_client
    }

    pub fn event_kind_v1(&self) -> MtgoCompetitiveEventKindV1 {
        self.authorization_binding_commitments.event_kind
    }

    pub fn game_number_v1(&self) -> u8 {
        self.authorization_binding_commitments.game_number
    }

    pub fn safe_for_next_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoConfirmedCompetitiveDuelPassCommitmentsV2 {
    pub input_receipt_sha256: String,
    pub visible_postcondition_commitment_sha256: String,
    pub transition_receipt_sha256: String,
    pub prior_game_session_commitment_sha256: String,
    pub advanced_game_session_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub game_number: u8,
    pub after_frame_id: u64,
    pub after_frame_sequence: u64,
    pub postcondition_candidate_count: u32,
    pub confirmed_action_count: u64,
}

/// One emitted priority Pass with its exact newer visible postcondition
/// confirmed. The shared input gate has been released. The only way to recover
/// the move-only game session for a later action is to consume this value.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoConfirmedCompetitiveDuelPassTransitionV2;
/// let _forged = OpaqueMtgoConfirmedCompetitiveDuelPassTransitionV2 {};
/// ```
pub struct OpaqueMtgoConfirmedCompetitiveDuelPassTransitionV2 {
    confirmation: OpaqueMtgoConfirmedCompetitiveDuelPassV1,
    session: OpaqueMtgoCompetitiveGameSessionV1,
    commitments: MtgoConfirmedCompetitiveDuelPassCommitmentsV2,
}

impl OpaqueMtgoConfirmedCompetitiveDuelPassTransitionV2 {
    pub fn commitments_v2(&self) -> MtgoConfirmedCompetitiveDuelPassCommitmentsV2 {
        self.commitments.clone()
    }

    pub fn into_game_session_v1(self) -> OpaqueMtgoCompetitiveGameSessionV1 {
        self.session
    }

    pub fn into_initial_player_visible_history_v1(
        self,
        history_id: &str,
    ) -> Result<
        (
            OpaqueMtgoCompetitiveGameSessionV1,
            CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
        ),
        String,
    > {
        let history = begin_checked_untrusted_competitive_player_visible_game_history_v1(
            history_id,
            self.confirmation.into_checked_postcondition_v1(),
        )
        .map_err(|error| format!("begin competitive player-visible history: {error}"))?;
        Ok((self.session, history))
    }

    pub fn into_appended_player_visible_history_v1(
        self,
        history: CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
    ) -> Result<
        (
            OpaqueMtgoCompetitiveGameSessionV1,
            CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1,
        ),
        String,
    > {
        let history = append_checked_untrusted_competitive_player_visible_game_history_v1(
            history,
            self.confirmation.into_checked_postcondition_v1(),
        )
        .map_err(|error| format!("append competitive player-visible history: {error}"))?;
        Ok((self.session, history))
    }

    pub fn safe_for_next_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

/// Proof that one authorized private-match pregame click was emitted and that
/// the process-wide gate is waiting for its exact visible postcondition.
///
/// This value exposes no pixels, window handle, or coordinates. Dropping it
/// does not release the gate. A failed confirmation halts later input for the
/// life of the process.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPendingPregameInputV3;
/// let _forged = OpaqueMtgoPendingPregameInputV3 {};
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoPendingPregameInputV3;
/// fn require_debug<T: std::fmt::Debug>() {}
/// require_debug::<OpaqueMtgoPendingPregameInputV3>();
/// ```
pub struct OpaqueMtgoPendingPregameInputV3 {
    plan: OpaqueMtgoPregameActionPlanV3,
    input_receipt_sha256: String,
    before_input_capture_commitment_sha256: String,
    input_sent_at_unix_millis: u128,
    cursor_parked_outside_client: bool,
}

impl OpaqueMtgoPendingPregameInputV3 {
    pub fn selected_semantic_v3(&self) -> &MtgoPregameActionSemanticV1 {
        self.plan.selected_semantic_v3()
    }

    pub fn planned_postcondition_v3(&self) -> &MtgoPlannedPregamePostconditionV3 {
        self.plan.planned_postcondition_v3()
    }

    pub fn action_plan_commitment_sha256_v3(&self) -> &str {
        self.plan.action_plan_commitment_sha256_v3()
    }

    pub fn before_input_capture_commitment_sha256_v3(&self) -> &str {
        &self.before_input_capture_commitment_sha256
    }

    pub fn input_receipt_sha256_v3(&self) -> &str {
        &self.input_receipt_sha256
    }

    pub fn input_sent_at_unix_millis_v3(&self) -> u128 {
        self.input_sent_at_unix_millis
    }

    pub fn cursor_parked_outside_client_v3(&self) -> bool {
        self.cursor_parked_outside_client
    }

    pub fn safe_for_next_input_v3(&self) -> bool {
        false
    }

    pub fn safe_for_purchase_v3(&self) -> bool {
        false
    }

    pub fn safe_for_queue_entry_v3(&self) -> bool {
        false
    }
}

pub fn pregame_input_gate_status_v3() -> Result<MtgoPregameInputGateStatusV3, String> {
    mtgo_input_gate_status_v3()
}

pub fn mtgo_input_gate_status_v3() -> Result<MtgoInputGateStatusV3, String> {
    let gate = input_gate_v3()
        .lock()
        .map_err(|_| "the process-wide input gate is poisoned".to_owned())?;
    Ok(match &*gate {
        PregameInputGateStateV3::Idle => MtgoPregameInputGateStatusV3::Idle,
        PregameInputGateStateV3::Preparing => MtgoPregameInputGateStatusV3::Preparing,
        PregameInputGateStateV3::AwaitingVisiblePostcondition { .. } => {
            MtgoPregameInputGateStatusV3::AwaitingVisiblePostcondition
        }
        PregameInputGateStateV3::Halted => MtgoPregameInputGateStatusV3::Halted,
    })
}

pub fn execute_authorized_private_match_pregame_action_v3(
    plan: OpaqueMtgoPregameActionPlanV3,
    authorization: RatifiedMtgoPrivateMatchAuthorizationV3,
) -> Result<OpaqueMtgoPendingPregameInputV3, String> {
    reserve_input_gate_v3()?;

    let prepared = match prepare_pregame_actuation_v3(&plan, &authorization.visible_account_alias) {
        Ok(prepared) => prepared,
        Err(error) => {
            release_unattempted_reservation_v3()?;
            return Err(error);
        }
    };
    let input_sent_at_unix_millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(error) => {
            release_unattempted_reservation_v3()?;
            return Err(format!("system clock is before epoch: {error}"));
        }
    };
    if validate_preinput_capture_freshness_v3(
        prepared.current_captured_at_unix_millis,
        input_sent_at_unix_millis,
    )
    .is_err()
    {
        release_unattempted_reservation_v3()?;
        return Err("the immediate pre-input capture is stale or from the future".to_owned());
    }
    halt_before_input_attempt_v3()?;
    let cursor_parked_outside_client = send_exactly_one_left_click_v3(&prepared)?;
    let input_receipt_sha256 = input_receipt_commitment_v3(
        &prepared,
        &authorization.scope,
        input_sent_at_unix_millis,
        cursor_parked_outside_client,
    );
    set_pending_v3(&input_receipt_sha256)?;

    Ok(OpaqueMtgoPendingPregameInputV3 {
        plan,
        input_receipt_sha256,
        before_input_capture_commitment_sha256: prepared.current_capture_commitment_sha256,
        input_sent_at_unix_millis,
        cursor_parked_outside_client,
    })
}

pub fn ratify_private_match_authorization_v3(
    scope: MtgoAuthorizationScopeV1,
    visible_account_alias: String,
) -> Result<RatifiedMtgoPrivateMatchAuthorizationV3, String> {
    ratify_private_match_authorization_with_commitment_v3(
        scope,
        visible_account_alias,
        RATIFIED_PRIVATE_MATCH_AUTHORIZATION_COMMITMENT_V3,
    )
}

pub fn ratify_competitive_duel_pass_authorization_v1(
    scope: MtgoAuthorizationScopeV1,
    visible_account_alias: String,
    event_kind: MtgoCompetitiveEventKindV1,
) -> Result<RatifiedMtgoCompetitiveDuelPassAuthorizationV1, String> {
    ratify_competitive_duel_pass_authorization_with_commitment_v1(
        scope,
        visible_account_alias,
        event_kind,
        RATIFIED_COMPETITIVE_DUEL_PASS_AUTHORIZATION_COMMITMENT_V1,
    )
}

/// Preferred production path for general League or Challenge priority-Pass
/// permission. It consumes a structurally checked review of the exact private
/// correspondence bytes, derives a one-mode scope from that review, and then
/// requires a separately compile-pinned commitment to the review and scope.
/// The production commitment remains empty until the exact reply is imported
/// and reviewed. This function grants no event-entry or spending authority.
pub fn ratify_competitive_duel_pass_authorization_from_correspondence_v2(
    correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    visible_account_alias: String,
    event_kind: MtgoCompetitiveEventKindV1,
) -> Result<RatifiedMtgoCompetitiveDuelPassAuthorizationV1, String> {
    ratify_competitive_duel_pass_authorization_from_correspondence_with_commitment_v2(
        correspondence,
        visible_account_alias,
        event_kind,
        RATIFIED_COMPETITIVE_DUEL_PASS_AUTHORIZATION_FROM_REVIEW_COMMITMENT_V2,
    )
}

/// Computes the exact one-mode production ratification candidate from a
/// structurally checked correspondence review without creating authority.
/// The caller must independently review the private message and the source
/// diff before placing this value in a production trust root.
pub fn review_competitive_duel_pass_ratification_candidate_from_correspondence_v2(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    visible_account_alias: &str,
    event_kind: MtgoCompetitiveEventKindV1,
) -> Result<MtgoReviewedCompetitivePassRatificationCandidateV2, String> {
    let permission_review_commitment_sha256 = correspondence.review_commitment_sha256().to_owned();
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(event_kind)
        .map_err(|error| format!("derive reviewed competitive mode scope: {error}"))?;
    let mode_authorization_commitment_sha256 =
        validate_competitive_duel_pass_authorization_v1(&scope, visible_account_alias, event_kind)?;
    let ratification_commitment_sha256 =
        competitive_duel_pass_authorization_from_review_commitment_v2(
            &scope,
            visible_account_alias,
            event_kind,
            &mode_authorization_commitment_sha256,
            &permission_review_commitment_sha256,
        );
    Ok(MtgoReviewedCompetitivePassRatificationCandidateV2 {
        permission_review_commitment_sha256,
        account_alias_sha256: scope.account_alias_sha256,
        correspondence_sha256: scope.written_permission_sha256,
        mode_authorization_commitment_sha256,
        ratification_commitment_sha256,
        event_kind,
    })
}

/// Computes the non-authorizing exact-account, one-mode pregame permission
/// candidate from the reviewed Daybreak correspondence and the separately
/// admitted deck-bound heuristic. The production root remains empty.
pub fn review_competitive_pregame_ratification_candidate_from_correspondence_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    visible_account_alias: &str,
    event_kind: MtgoCompetitiveEventKindV1,
    heuristic: &AdmittedMtgoCompetitivePregameHeuristicV1,
) -> Result<MtgoReviewedCompetitivePregameRatificationCandidateV1, String> {
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(event_kind)
        .map_err(|error| format!("derive reviewed competitive pregame scope: {error}"))?;
    let mode_authorization_commitment_sha256 = validate_exact_competitive_mode_authorization_v1(
        &scope,
        visible_account_alias,
        event_kind,
        "competitive pregame ratification",
    )?;
    let heuristic_commitments = heuristic.commitments_v1();
    let permission_review_commitment_sha256 = correspondence.review_commitment_sha256().to_owned();
    let ratification_commitment_sha256 = competitive_pregame_authorization_commitment_v1(
        &scope,
        visible_account_alias,
        event_kind,
        &mode_authorization_commitment_sha256,
        &permission_review_commitment_sha256,
        (
            &heuristic_commitments.heuristic_profile_commitment_sha256,
            &heuristic_commitments.heuristic_algorithm_commitment_sha256,
            &heuristic_commitments.review_commitment_sha256,
            &heuristic_commitments.admission_commitment_sha256,
        ),
    );
    Ok(MtgoReviewedCompetitivePregameRatificationCandidateV1 {
        permission_review_commitment_sha256,
        account_alias_sha256: scope.account_alias_sha256,
        correspondence_sha256: scope.written_permission_sha256,
        mode_authorization_commitment_sha256,
        heuristic_profile_commitment_sha256: heuristic_commitments
            .heuristic_profile_commitment_sha256,
        heuristic_algorithm_commitment_sha256: heuristic_commitments
            .heuristic_algorithm_commitment_sha256,
        heuristic_review_commitment_sha256: heuristic_commitments.review_commitment_sha256,
        heuristic_admission_commitment_sha256: heuristic_commitments.admission_commitment_sha256,
        ratification_commitment_sha256,
        event_kind,
    })
}

pub fn ratify_competitive_pregame_authorization_from_correspondence_v1(
    correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    visible_account_alias: String,
    event_kind: MtgoCompetitiveEventKindV1,
    heuristic: &AdmittedMtgoCompetitivePregameHeuristicV1,
) -> Result<RatifiedMtgoCompetitivePregameAuthorizationV1, String> {
    ratify_competitive_pregame_authorization_from_correspondence_with_commitment_v1(
        correspondence,
        visible_account_alias,
        event_kind,
        heuristic,
        RATIFIED_COMPETITIVE_EVENT_PREGAME_AUTHORIZATION_COMMITMENT_V1,
    )
}

/// Computes the non-authorizing production-ratification candidate for the
/// complete reviewed duel gesture profile in one exact League or Challenge
/// mode. Both the private Daybreak reply and the admitted gesture evaluation
/// are retained by their opaque wrappers. No input, entry, or spending
/// authority is created.
pub fn review_competitive_duel_gesture_ratification_candidate_from_correspondence_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    visible_account_alias: &str,
    event_kind: MtgoCompetitiveEventKindV1,
    gesture_profile: &AdmittedMtgoDuelGestureProfileV1,
) -> Result<MtgoReviewedCompetitiveGestureRatificationCandidateV1, String> {
    let facts = CompetitiveDuelGestureProfileFactsV1 {
        evaluation_commitment_sha256: gesture_profile.evaluation_commitment_sha256(),
        admission_commitment_sha256: gesture_profile.admission_commitment_sha256(),
        runtime_binary_sha256: gesture_profile.gesture_target_runtime_binary_sha256(),
        assets_manifest_sha256: gesture_profile.gesture_target_assets_manifest_sha256(),
        supported_action_families: gesture_profile.supported_action_families(),
    };
    competitive_duel_gesture_ratification_candidate_from_parts_v1(
        correspondence,
        visible_account_alias,
        event_kind,
        &facts,
    )
}

/// Production constructor for one exact-account, exact-mode, reviewed gesture
/// permission. The compile-pinned root is empty until the correspondence and
/// measured all-family gesture corpus have both been reviewed. The returned
/// value remains a permission identity only and cannot send input.
pub fn ratify_competitive_duel_gesture_authorization_from_correspondence_v1(
    correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    visible_account_alias: String,
    event_kind: MtgoCompetitiveEventKindV1,
    gesture_profile: AdmittedMtgoDuelGestureProfileV1,
) -> Result<RatifiedMtgoCompetitiveDuelGestureAuthorizationV1, String> {
    let candidate = review_competitive_duel_gesture_ratification_candidate_from_correspondence_v1(
        &correspondence,
        &visible_account_alias,
        event_kind,
        &gesture_profile,
    )?;
    if RATIFIED_COMPETITIVE_DUEL_GESTURE_AUTHORIZATION_FROM_REVIEW_COMMITMENT_V1
        != Some(candidate.ratification_commitment_sha256.as_str())
    {
        return Err(
            "the exact reviewed competitive gesture permission is not ratified in this build"
                .to_owned(),
        );
    }
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(event_kind)
        .map_err(|error| format!("derive reviewed competitive gesture mode scope: {error}"))?;
    Ok(RatifiedMtgoCompetitiveDuelGestureAuthorizationV1 {
        scope,
        _permission_correspondence: correspondence,
        _gesture_profile: gesture_profile,
        visible_account_alias,
        event_kind,
        mode_authorization_commitment_sha256: candidate.mode_authorization_commitment_sha256,
        permission_review_commitment_sha256: candidate.permission_review_commitment_sha256,
        gesture_evaluation_commitment_sha256: candidate.gesture_evaluation_commitment_sha256,
        gesture_profile_admission_commitment_sha256: candidate
            .gesture_profile_admission_commitment_sha256,
        authorization_commitment_sha256: candidate.ratification_commitment_sha256,
    })
}

/// Computes the exact production ratification candidate for one attended,
/// classifier-backed, visibly enabled League or Challenge entry review. The
/// output is non-authorizing telemetry. The private correspondence and the
/// opaque coordinate-bearing review remain retained by their callers.
pub fn review_competitive_entry_ratification_candidate_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    review: &CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4,
    visible_account_alias: &str,
) -> Result<MtgoReviewedCompetitiveEntryRatificationCandidateV1, String> {
    let source_identity = &review
        ._classifier_bound_review
        ._source_bound_review
        ._source_identity;
    competitive_entry_ratification_candidate_from_parts_v1(
        correspondence,
        visible_account_alias,
        source_identity.lifecycle_v1(),
        &source_identity.commitments_v1(),
        &review.entry_authorization,
        &review.commitments,
    )
}

/// Consumes the visibly confirmed selected-listing arrival and one strictly
/// newer, control-bound paid Entry Review. Success proves only that the two
/// reviewed states preserve the same account, mode, client lineage, event,
/// deck, and policy commitments. It grants no entry or spending authority.
pub fn bind_confirmed_competitive_open_entry_review_to_entry_review_v1(
    open_entry_review: OpaqueMtgoConfirmedCompetitiveOpenEntryReviewV1,
    review: CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4,
    visible_account_alias: String,
) -> Result<CheckedUntrustedMtgoSelectedListingBoundCompetitiveEntryReviewV1, String> {
    let open_authorization = open_entry_review._authorization.commitments_v1();
    let open_confirmation = open_entry_review.commitments_v1();
    let open_visible = open_entry_review._visible_confirmation.commitments_v1();
    let (open_after_navigation, open_after_window_continuity_commitment_sha256) = open_entry_review
        ._visible_confirmation
        .after_source_lineage_v1()?;
    let source_identity = &review
        ._classifier_bound_review
        ._source_bound_review
        ._source_identity;
    let entry_review_lineage = source_identity.source_lineage_v1()?;
    let entry_review_candidate = review_competitive_entry_ratification_candidate_v1(
        &open_entry_review._authorization._permission_correspondence,
        &review,
        &visible_account_alias,
    )?;
    let commitments = bind_selected_listing_to_competitive_entry_review_from_parts_v1(
        &open_authorization,
        &open_confirmation,
        &open_visible,
        &open_after_navigation,
        &open_after_window_continuity_commitment_sha256,
        &entry_review_lineage,
        &entry_review_candidate,
    )?;
    let ratification_candidate =
        selected_listing_competitive_entry_ratification_candidate_from_parts_v2(
            &commitments,
            &entry_review_candidate,
        )?;
    Ok(
        CheckedUntrustedMtgoSelectedListingBoundCompetitiveEntryReviewV1 {
            open_entry_review,
            review,
            visible_account_alias,
            commitments,
            ratification_candidate,
        },
    )
}

/// Returns non-authorizing telemetry for the exact selected-listing-bound paid
/// entry candidate. This is the only candidate accepted by the v2 production
/// ratifier.
pub fn review_competitive_entry_ratification_candidate_from_selected_listing_v2(
    review: &CheckedUntrustedMtgoSelectedListingBoundCompetitiveEntryReviewV1,
) -> MtgoReviewedSelectedListingCompetitiveEntryRatificationCandidateV2 {
    review.ratification_candidate.clone()
}

/// Production path for one exact selected-listing-bound League or Challenge
/// entry. Its compile-time root is empty, so current builds always reject. Even
/// a future ratified value must still pass immediate recapture and visible
/// postcondition gates before one paid-entry click can occur.
pub fn ratify_competitive_entry_authorization_from_selected_listing_v2(
    review: CheckedUntrustedMtgoSelectedListingBoundCompetitiveEntryReviewV1,
) -> Result<RatifiedMtgoCompetitiveEntryAuthorizationV1, String> {
    let candidate = review.ratification_candidate.clone();
    if RATIFIED_COMPETITIVE_SELECTED_LISTING_ENTRY_AUTHORIZATION_COMMITMENT_V2
        != Some(candidate.ratification_commitment_sha256.as_str())
    {
        return Err(
            "the exact selected-listing-bound competitive entry is not ratified in this build"
                .to_owned(),
        );
    }
    let CheckedUntrustedMtgoSelectedListingBoundCompetitiveEntryReviewV1 {
        open_entry_review,
        review,
        visible_account_alias,
        commitments,
        ratification_candidate: _,
    } = review;
    let OpaqueMtgoConfirmedCompetitiveOpenEntryReviewV1 {
        _authorization: open_entry_review_authorization,
        _visible_confirmation: open_entry_review_visible_confirmation,
        commitments: _,
    } = open_entry_review;
    let mut entry_commitments = candidate.entry_review_candidate;
    entry_commitments.ratification_commitment_sha256 = candidate.ratification_commitment_sha256;
    Ok(RatifiedMtgoCompetitiveEntryAuthorizationV1 {
        _open_entry_review_authorization: open_entry_review_authorization,
        _open_entry_review_visible_confirmation: open_entry_review_visible_confirmation,
        _review: review,
        visible_account_alias,
        selected_listing_binding_commitment_sha256: commitments.binding_commitment_sha256,
        commitments: entry_commitments,
    })
}

/// Computes the complete non-authorizing production candidate for all five
/// non-entry lifecycle controls in one exact League or Challenge mode.
pub fn review_competitive_lifecycle_ratification_candidate_from_correspondence_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    profile: &AdmittedMtgoCompetitiveNavigationProfileV1,
    visible_account_alias: &str,
    event_kind: MtgoCompetitiveEventKindV1,
) -> Result<MtgoReviewedCompetitiveLifecycleRatificationCandidateV1, String> {
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(event_kind)
        .map_err(|error| format!("competitive lifecycle correspondence scope: {error}"))?;
    let mode_authorization_commitment_sha256 = validate_exact_competitive_mode_authorization_v1(
        &scope,
        visible_account_alias,
        event_kind,
        "competitive lifecycle ratification",
    )?;
    let runtime = profile.checked_runtime_profile();
    if runtime.approved_account_alias_sha256() != scope.account_alias_sha256 {
        return Err(
            "competitive lifecycle profile and correspondence approve different accounts"
                .to_owned(),
        );
    }
    let allowed_actions_commitment_sha256 = competitive_lifecycle_allowed_actions_commitment_v1();
    let ratification_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_LIFECYCLE_AUTHORIZATION_DOMAIN_V1,
        &[
            correspondence.correspondence_sha256().as_bytes(),
            correspondence.review_commitment_sha256().as_bytes(),
            mode_authorization_commitment_sha256.as_bytes(),
            scope.account_alias_sha256.as_bytes(),
            profile.profile_commitment_sha256().as_bytes(),
            profile.admission_commitment_sha256().as_bytes(),
            allowed_actions_commitment_sha256.as_bytes(),
            competitive_event_kind_tag_v1(event_kind),
            b"all_five_non_entry_lifecycle_controls_one_click_each_exact_postcondition",
        ],
    );
    Ok(MtgoReviewedCompetitiveLifecycleRatificationCandidateV1 {
        correspondence_sha256: correspondence.correspondence_sha256().to_owned(),
        permission_review_commitment_sha256: correspondence.review_commitment_sha256().to_owned(),
        mode_authorization_commitment_sha256,
        approved_account_alias_sha256: scope.account_alias_sha256,
        lifecycle_profile_commitment_sha256: profile.profile_commitment_sha256().to_owned(),
        lifecycle_profile_admission_commitment_sha256: profile
            .admission_commitment_sha256()
            .to_owned(),
        allowed_actions_commitment_sha256,
        event_kind,
        ratification_commitment_sha256,
    })
}

pub fn ratify_competitive_lifecycle_authorization_from_correspondence_v1(
    correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    profile: &AdmittedMtgoCompetitiveNavigationProfileV1,
    visible_account_alias: String,
    event_kind: MtgoCompetitiveEventKindV1,
) -> Result<RatifiedMtgoCompetitiveLifecycleAuthorizationV1, String> {
    ratify_competitive_lifecycle_authorization_with_commitment_v1(
        correspondence,
        profile,
        visible_account_alias,
        event_kind,
        RATIFIED_COMPETITIVE_LIFECYCLE_AUTHORIZATION_COMMITMENT_V1,
    )
}

/// Computes the exact non-authorizing candidate for opening a selected League
/// or Challenge listing to Entry Review. The candidate binds the reviewed
/// correspondence, approved account, navigation profile, listing evaluation,
/// deck, format, and deployment policy. Paid entry remains out of scope.
#[allow(clippy::too_many_arguments)]
pub fn review_competitive_open_entry_review_ratification_candidate_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    profile: &AdmittedMtgoCompetitiveNavigationProfileV1,
    listing_evaluation: &AdmittedMtgoCompetitiveEventListingEvaluationV1,
    manifest: &ValidatedMtgoCompetitiveDeckManifestV1,
    policy_deployment_commitment_sha256: &str,
    visible_account_alias: &str,
    event_kind: MtgoCompetitiveEventKindV1,
) -> Result<MtgoReviewedCompetitiveOpenEntryReviewRatificationCandidateV1, String> {
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(event_kind)
        .map_err(|error| format!("Open Entry Review correspondence scope: {error}"))?;
    let mode_authorization_commitment_sha256 = validate_exact_competitive_mode_authorization_v1(
        &scope,
        visible_account_alias,
        event_kind,
        "Open Entry Review ratification",
    )?;
    let runtime = profile.checked_runtime_profile();
    let evaluated = listing_evaluation.commitments_v1();
    let listing_evaluation_admission_commitment_sha256 = listing_evaluation
        .admission_commitment_sha256_v1()
        .to_owned();
    for commitment in [
        correspondence.correspondence_sha256(),
        correspondence.review_commitment_sha256(),
        mode_authorization_commitment_sha256.as_str(),
        scope.account_alias_sha256.as_str(),
        profile.profile_commitment_sha256(),
        profile.admission_commitment_sha256(),
        evaluated.ratification_commitment_sha256.as_str(),
        listing_evaluation_admission_commitment_sha256.as_str(),
        manifest.deck_list_sha256(),
        manifest.manifest_commitment_sha256(),
        manifest.format_sha256(),
        policy_deployment_commitment_sha256,
    ] {
        if !is_sha256_v2(commitment) {
            return Err("Open Entry Review ratification contains an invalid commitment".to_owned());
        }
    }
    if runtime.approved_account_alias_sha256() != scope.account_alias_sha256
        || evaluated.profile_commitment_sha256 != profile.profile_commitment_sha256()
        || evaluated.approved_account_alias_sha256 != scope.account_alias_sha256
        || evaluated.deck_list_sha256 != manifest.deck_list_sha256()
        || evaluated.deck_manifest_commitment_sha256 != manifest.manifest_commitment_sha256()
        || evaluated.deck_format_sha256 != manifest.format_sha256()
        || evaluated.policy_deployment_commitment_sha256 != policy_deployment_commitment_sha256
    {
        return Err(
            "Open Entry Review profile, evaluation, account, deck, format, or policy identity differs"
                .to_owned(),
        );
    }
    let open_review_scope_commitment_sha256 = competitive_open_entry_review_scope_commitment_v1();
    let ratification_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_OPEN_ENTRY_REVIEW_AUTHORIZATION_DOMAIN_V1,
        &[
            correspondence.correspondence_sha256().as_bytes(),
            correspondence.review_commitment_sha256().as_bytes(),
            mode_authorization_commitment_sha256.as_bytes(),
            scope.account_alias_sha256.as_bytes(),
            profile.profile_commitment_sha256().as_bytes(),
            profile.admission_commitment_sha256().as_bytes(),
            evaluated.ratification_commitment_sha256.as_bytes(),
            listing_evaluation_admission_commitment_sha256.as_bytes(),
            manifest.deck_list_sha256().as_bytes(),
            manifest.manifest_commitment_sha256().as_bytes(),
            manifest.format_sha256().as_bytes(),
            policy_deployment_commitment_sha256.as_bytes(),
            open_review_scope_commitment_sha256.as_bytes(),
            competitive_event_kind_tag_v1(event_kind),
            b"open_review_only_separately_governed_from_paid_entry",
        ],
    );
    Ok(
        MtgoReviewedCompetitiveOpenEntryReviewRatificationCandidateV1 {
            correspondence_sha256: correspondence.correspondence_sha256().to_owned(),
            permission_review_commitment_sha256: correspondence
                .review_commitment_sha256()
                .to_owned(),
            mode_authorization_commitment_sha256,
            approved_account_alias_sha256: scope.account_alias_sha256,
            navigation_profile_commitment_sha256: profile.profile_commitment_sha256().to_owned(),
            navigation_profile_admission_commitment_sha256: profile
                .admission_commitment_sha256()
                .to_owned(),
            listing_evaluation_ratification_commitment_sha256: evaluated
                .ratification_commitment_sha256,
            listing_evaluation_admission_commitment_sha256,
            deck_list_sha256: manifest.deck_list_sha256().to_owned(),
            deck_manifest_commitment_sha256: manifest.manifest_commitment_sha256().to_owned(),
            deck_format_sha256: manifest.format_sha256().to_owned(),
            policy_deployment_commitment_sha256: policy_deployment_commitment_sha256.to_owned(),
            open_review_scope_commitment_sha256,
            event_kind,
            ratification_commitment_sha256,
        },
    )
}

/// Production constructor for the exact Open Entry Review-only permission.
/// The root remains empty until the exact written League and Challenge
/// approval, approved account, profile, evaluation, deck, and policy are
/// imported and reviewed. The returned type, when reachable, still cannot
/// confirm entry.
#[allow(clippy::too_many_arguments)]
pub fn ratify_competitive_open_entry_review_authorization_v1(
    correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    profile: &AdmittedMtgoCompetitiveNavigationProfileV1,
    listing_evaluation: &AdmittedMtgoCompetitiveEventListingEvaluationV1,
    manifest: &ValidatedMtgoCompetitiveDeckManifestV1,
    policy_deployment_commitment_sha256: &str,
    visible_account_alias: String,
    event_kind: MtgoCompetitiveEventKindV1,
) -> Result<RatifiedMtgoCompetitiveOpenEntryReviewAuthorizationV1, String> {
    let candidate = review_competitive_open_entry_review_ratification_candidate_v1(
        &correspondence,
        profile,
        listing_evaluation,
        manifest,
        policy_deployment_commitment_sha256,
        &visible_account_alias,
        event_kind,
    )?;
    let expected = RATIFIED_COMPETITIVE_OPEN_ENTRY_REVIEW_AUTHORIZATION_COMMITMENT_V1
        .ok_or("the exact Open Entry Review permission is not ratified in this build")?;
    if candidate.ratification_commitment_sha256 != expected {
        return Err(
            "Open Entry Review candidate differs from the production ratification root".to_owned(),
        );
    }
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(event_kind)
        .map_err(|error| format!("derive ratified Open Entry Review mode scope: {error}"))?;
    Ok(RatifiedMtgoCompetitiveOpenEntryReviewAuthorizationV1 {
        _permission_correspondence: correspondence,
        scope,
        commitments: candidate,
    })
}

/// Computes the exact non-authorizing production candidate for visible
/// sideboard parsing, one-card drag, per-transfer visible confirmation, and
/// changed-sideboard Submit Deck in one already ratified mode.
pub fn review_competitive_sideboard_automation_ratification_candidate_v1(
    lifecycle_authorization: &RatifiedMtgoCompetitiveLifecycleAuthorizationV1,
    manifest: &ValidatedMtgoCompetitiveDeckManifestV1,
    policy_deployment_commitment_sha256: &str,
    sideboard_evaluation: &AdmittedMtgoCompetitiveSideboardEvaluationV1,
) -> Result<MtgoReviewedCompetitiveSideboardAutomationRatificationCandidateV1, String> {
    let lifecycle = lifecycle_authorization.commitments_v1();
    let evaluation = sideboard_evaluation.commitments_v1();
    for commitment in [
        lifecycle.ratification_commitment_sha256.as_str(),
        lifecycle.permission_review_commitment_sha256.as_str(),
        lifecycle.mode_authorization_commitment_sha256.as_str(),
        lifecycle.approved_account_alias_sha256.as_str(),
        lifecycle.lifecycle_profile_commitment_sha256.as_str(),
        lifecycle
            .lifecycle_profile_admission_commitment_sha256
            .as_str(),
        manifest.deck_list_sha256(),
        manifest.manifest_commitment_sha256(),
        manifest.format_sha256(),
        policy_deployment_commitment_sha256,
        evaluation.ratification_commitment_sha256.as_str(),
        sideboard_evaluation.admission_commitment_sha256_v1(),
    ] {
        if !is_sha256_v2(commitment) {
            return Err(
                "sideboard automation ratification contains an invalid commitment".to_owned(),
            );
        }
    }
    if manifest.deck_list_sha256() == manifest.format_sha256()
        || manifest.deck_list_sha256() == policy_deployment_commitment_sha256
        || manifest.manifest_commitment_sha256() == policy_deployment_commitment_sha256
        || manifest.format_sha256() == policy_deployment_commitment_sha256
    {
        return Err("sideboard deck, format, and policy identities are crossed".to_owned());
    }
    if evaluation.profile_commitment_sha256 != lifecycle.lifecycle_profile_commitment_sha256
        || evaluation.approved_account_alias_sha256 != lifecycle.approved_account_alias_sha256
        || evaluation.deck_list_sha256 != manifest.deck_list_sha256()
        || evaluation.deck_manifest_commitment_sha256 != manifest.manifest_commitment_sha256()
        || evaluation.deck_format_sha256 != manifest.format_sha256()
        || evaluation.policy_deployment_commitment_sha256 != policy_deployment_commitment_sha256
    {
        return Err(
            "sideboard evaluation differs from the exact lifecycle profile, account, deck, format, or policy"
                .to_owned(),
        );
    }
    let automation_scope_commitment_sha256 = competitive_sideboard_automation_scope_v1();
    let ratification_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_SIDEBOARD_AUTOMATION_AUTHORIZATION_DOMAIN_V1,
        &[
            lifecycle.ratification_commitment_sha256.as_bytes(),
            lifecycle.permission_review_commitment_sha256.as_bytes(),
            lifecycle.mode_authorization_commitment_sha256.as_bytes(),
            lifecycle.approved_account_alias_sha256.as_bytes(),
            lifecycle.lifecycle_profile_commitment_sha256.as_bytes(),
            lifecycle
                .lifecycle_profile_admission_commitment_sha256
                .as_bytes(),
            manifest.deck_list_sha256().as_bytes(),
            manifest.manifest_commitment_sha256().as_bytes(),
            manifest.format_sha256().as_bytes(),
            policy_deployment_commitment_sha256.as_bytes(),
            evaluation.ratification_commitment_sha256.as_bytes(),
            sideboard_evaluation
                .admission_commitment_sha256_v1()
                .as_bytes(),
            automation_scope_commitment_sha256.as_bytes(),
            competitive_event_kind_tag_v1(lifecycle.event_kind),
            b"exact_visible_sideboard_parser_one_card_drag_confirmation_and_changed_submit",
        ],
    );
    Ok(
        MtgoReviewedCompetitiveSideboardAutomationRatificationCandidateV1 {
            lifecycle_authorization_commitment_sha256: lifecycle.ratification_commitment_sha256,
            permission_review_commitment_sha256: lifecycle.permission_review_commitment_sha256,
            mode_authorization_commitment_sha256: lifecycle.mode_authorization_commitment_sha256,
            approved_account_alias_sha256: lifecycle.approved_account_alias_sha256,
            navigation_profile_commitment_sha256: lifecycle.lifecycle_profile_commitment_sha256,
            navigation_profile_admission_commitment_sha256: lifecycle
                .lifecycle_profile_admission_commitment_sha256,
            deck_list_sha256: manifest.deck_list_sha256().to_owned(),
            deck_manifest_commitment_sha256: manifest.manifest_commitment_sha256().to_owned(),
            deck_format_sha256: manifest.format_sha256().to_owned(),
            policy_deployment_commitment_sha256: policy_deployment_commitment_sha256.to_owned(),
            sideboard_evaluation_ratification_commitment_sha256: evaluation
                .ratification_commitment_sha256,
            sideboard_evaluation_admission_commitment_sha256: sideboard_evaluation
                .admission_commitment_sha256_v1()
                .to_owned(),
            automation_scope_commitment_sha256,
            event_kind: lifecycle.event_kind,
            ratification_commitment_sha256,
        },
    )
}

pub fn ratify_competitive_sideboard_automation_v1(
    lifecycle_authorization: &RatifiedMtgoCompetitiveLifecycleAuthorizationV1,
    manifest: &ValidatedMtgoCompetitiveDeckManifestV1,
    policy_deployment_commitment_sha256: &str,
    sideboard_evaluation: &AdmittedMtgoCompetitiveSideboardEvaluationV1,
) -> Result<RatifiedMtgoCompetitiveSideboardAutomationAuthorizationV1, String> {
    let candidate = review_competitive_sideboard_automation_ratification_candidate_v1(
        lifecycle_authorization,
        manifest,
        policy_deployment_commitment_sha256,
        sideboard_evaluation,
    )?;
    let expected = RATIFIED_COMPETITIVE_SIDEBOARD_AUTOMATION_COMMITMENT_V1
        .ok_or("the exact competitive sideboard automation is not ratified in this build")?;
    if candidate.ratification_commitment_sha256 != expected {
        return Err(
            "competitive sideboard candidate differs from the production ratification root"
                .to_owned(),
        );
    }
    Ok(RatifiedMtgoCompetitiveSideboardAutomationAuthorizationV1 {
        commitments: candidate,
    })
}

/// Consumes one freshly captured evaluated listing and the separately ratified
/// Open Entry Review-only permission. The source capture must still be fresh at
/// execution time, and no paid-entry control is reachable from this value.
pub fn prepare_ratified_competitive_open_entry_review_v1(
    authorization: RatifiedMtgoCompetitiveOpenEntryReviewAuthorizationV1,
    evaluated_listing: OpaqueMtgoEvaluatedCompetitiveEventListingV1,
) -> Result<OpaqueMtgoPreparedCompetitiveOpenEntryReviewV1, String> {
    let authorization_commitments = authorization.commitments_v1();
    let (source, pointer_target) = prepare_opaque_competitive_event_listing_open_source_v1(
        evaluated_listing,
        &authorization.scope,
    )?;
    let source_commitments = source.commitments_v1();
    let listing = &source_commitments
        .evaluated_listing
        .classified_listing
        .source_listing;
    if source_commitments.event_kind != authorization_commitments.event_kind
        || source_commitments.mode_authorization_commitment_sha256
            != authorization_commitments.mode_authorization_commitment_sha256
        || listing
            .source_navigation
            .source_frame
            .approved_account_alias_sha256
            != authorization_commitments.approved_account_alias_sha256
        || listing
            .source_navigation
            .source_frame
            .profile_commitment_sha256
            != authorization_commitments.navigation_profile_commitment_sha256
        || listing
            .source_navigation
            .source_frame
            .profile_admission_commitment_sha256
            != authorization_commitments.navigation_profile_admission_commitment_sha256
        || source_commitments
            .evaluated_listing
            .evaluation_ratification_commitment_sha256
            != authorization_commitments.listing_evaluation_ratification_commitment_sha256
        || source_commitments
            .evaluated_listing
            .evaluation_admission_commitment_sha256
            != authorization_commitments.listing_evaluation_admission_commitment_sha256
        || listing.deck_list_sha256 != authorization_commitments.deck_list_sha256
        || listing.deck_manifest_commitment_sha256
            != authorization_commitments.deck_manifest_commitment_sha256
        || listing.deck_format_sha256 != authorization_commitments.deck_format_sha256
        || listing.policy_deployment_commitment_sha256
            != authorization_commitments.policy_deployment_commitment_sha256
        || authorization_commitments.open_review_scope_commitment_sha256
            != competitive_open_entry_review_scope_commitment_v1()
    {
        return Err(
            "evaluated listing differs from the exact ratified Open Entry Review account, mode, profile, evaluation, deck, format, or policy"
                .to_owned(),
        );
    }
    let preparation_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_OPEN_ENTRY_REVIEW_PREPARATION_DOMAIN_V1,
        &[
            authorization_commitments
                .ratification_commitment_sha256
                .as_bytes(),
            source_commitments
                .evaluated_listing
                .evaluated_binding_commitment_sha256
                .as_bytes(),
            source_commitments
                .source_preparation_commitment_sha256
                .as_bytes(),
            source_commitments.open_intent_commitment_sha256.as_bytes(),
            competitive_event_kind_tag_v1(source_commitments.event_kind),
            source_commitments.event_identity_sha256.as_bytes(),
            source_commitments.source_frame_id.to_be_bytes().as_slice(),
            source_commitments
                .source_frame_sequence
                .to_be_bytes()
                .as_slice(),
            source_commitments
                .source_captured_at_unix_millis
                .to_be_bytes()
                .as_slice(),
            b"fresh_exact_open_review_control_one_click_shared_gate_pending_exact_visible_arrival",
        ],
    );
    let commitments = MtgoPreparedCompetitiveOpenEntryReviewCommitmentsV1 {
        authorization_ratification_commitment_sha256: authorization_commitments
            .ratification_commitment_sha256,
        evaluated_listing_binding_commitment_sha256: source_commitments
            .evaluated_listing
            .evaluated_binding_commitment_sha256,
        source_preparation_commitment_sha256: source_commitments
            .source_preparation_commitment_sha256,
        preparation_commitment_sha256,
        event_kind: source_commitments.event_kind,
        event_identity_sha256: source_commitments.event_identity_sha256,
        source_frame_id: source_commitments.source_frame_id,
        source_frame_sequence: source_commitments.source_frame_sequence,
        source_captured_at_unix_millis: source_commitments.source_captured_at_unix_millis,
    };
    Ok(OpaqueMtgoPreparedCompetitiveOpenEntryReviewV1 {
        authorization,
        source,
        pointer_target,
        commitments,
    })
}

/// Sends exactly one left click through the process-wide input gate. The gate
/// remains pending until a newer exact Entry Review frame is confirmed.
pub fn execute_prepared_competitive_open_entry_review_v1(
    prepared: OpaqueMtgoPreparedCompetitiveOpenEntryReviewV1,
) -> Result<OpaqueMtgoPendingCompetitiveOpenEntryReviewV1, String> {
    reserve_input_gate_v3()?;
    let input_sent_at_unix_millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(error) => {
            release_unattempted_reservation_v3()?;
            return Err(format!("system clock is before epoch: {error}"));
        }
    };
    let prepared_commitments = prepared.commitments_v1();
    if validate_preinput_capture_freshness_v3(
        prepared_commitments.source_captured_at_unix_millis,
        input_sent_at_unix_millis,
    )
    .is_err()
    {
        release_unattempted_reservation_v3()?;
        return Err("the selected Open Entry Review capture is stale or future-dated".to_owned());
    }
    halt_before_input_attempt_v3()?;
    let cursor_parked_outside_client = send_exactly_one_left_click_v3(&prepared)?;
    let input_receipt_sha256 = hash_parts_v2(
        COMPETITIVE_OPEN_ENTRY_REVIEW_INPUT_RECEIPT_DOMAIN_V1,
        &[
            prepared_commitments
                .preparation_commitment_sha256
                .as_bytes(),
            prepared_commitments
                .authorization_ratification_commitment_sha256
                .as_bytes(),
            prepared_commitments
                .evaluated_listing_binding_commitment_sha256
                .as_bytes(),
            competitive_event_kind_tag_v1(prepared_commitments.event_kind),
            prepared_commitments.event_identity_sha256.as_bytes(),
            prepared_commitments
                .source_frame_id
                .to_be_bytes()
                .as_slice(),
            prepared_commitments
                .source_frame_sequence
                .to_be_bytes()
                .as_slice(),
            input_sent_at_unix_millis.to_be_bytes().as_slice(),
            &[u8::from(cursor_parked_outside_client)],
            b"exactly_one_open_review_left_click_shared_gate_pending_visible_entry_review",
        ],
    );
    set_pending_v3(&input_receipt_sha256)?;
    let commitments = MtgoCompetitiveOpenEntryReviewInputReceiptCommitmentsV1 {
        preparation_commitment_sha256: prepared_commitments.preparation_commitment_sha256,
        authorization_ratification_commitment_sha256: prepared_commitments
            .authorization_ratification_commitment_sha256,
        input_receipt_sha256,
        event_kind: prepared_commitments.event_kind,
        event_identity_sha256: prepared_commitments.event_identity_sha256,
        source_frame_id: prepared_commitments.source_frame_id,
        source_frame_sequence: prepared_commitments.source_frame_sequence,
        input_sent_at_unix_millis,
        cursor_parked_outside_client,
    };
    Ok(OpaqueMtgoPendingCompetitiveOpenEntryReviewV1 {
        prepared,
        commitments,
    })
}

/// Releases the shared gate only after a strictly newer classified frame
/// visibly reaches Entry Review for the exact selected event. A mismatch halts
/// the gate and does not grant event-entry confirmation or spending authority.
pub fn confirm_pending_competitive_open_entry_review_v1(
    pending: OpaqueMtgoPendingCompetitiveOpenEntryReviewV1,
    after: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
) -> Result<OpaqueMtgoConfirmedCompetitiveOpenEntryReviewV1, String> {
    require_matching_pending_v3(&pending.commitments.input_receipt_sha256)?;
    let OpaqueMtgoPendingCompetitiveOpenEntryReviewV1 {
        prepared,
        commitments: input,
    } = pending;
    let OpaqueMtgoPreparedCompetitiveOpenEntryReviewV1 {
        authorization,
        source,
        pointer_target: _,
        commitments: prepared_commitments,
    } = prepared;
    let visible_confirmation = match confirm_opaque_competitive_event_listing_opened_v1(
        source,
        after,
        &authorization.scope,
        input.input_sent_at_unix_millis,
    ) {
        Ok(value) => value,
        Err(error) => {
            halt_gate_v3()?;
            return Err(format!(
                "Open Entry Review postcondition failed and the input gate is halted: {error}"
            ));
        }
    };
    let visible = visible_confirmation.commitments_v1();
    if visible.source_preparation_commitment_sha256
        != prepared_commitments.source_preparation_commitment_sha256
        || visible.event_kind != prepared_commitments.event_kind
        || visible.event_identity_sha256 != prepared_commitments.event_identity_sha256
        || visible.after_frame_sequence <= prepared_commitments.source_frame_sequence
        || visible.after_captured_at_unix_millis <= input.input_sent_at_unix_millis
    {
        halt_gate_v3()?;
        return Err(
            "Open Entry Review confirmation changed the exact selected-listing lineage and halted the input gate"
                .to_owned(),
        );
    }
    let confirmation_receipt_sha256 = hash_parts_v2(
        COMPETITIVE_OPEN_ENTRY_REVIEW_CONFIRMATION_RECEIPT_DOMAIN_V1,
        &[
            input.input_receipt_sha256.as_bytes(),
            prepared_commitments
                .preparation_commitment_sha256
                .as_bytes(),
            prepared_commitments
                .authorization_ratification_commitment_sha256
                .as_bytes(),
            visible.visible_confirmation_commitment_sha256.as_bytes(),
            visible.arrival_commitment_sha256.as_bytes(),
            competitive_event_kind_tag_v1(visible.event_kind),
            visible.event_identity_sha256.as_bytes(),
            visible.after_frame_id.to_be_bytes().as_slice(),
            visible.after_frame_sequence.to_be_bytes().as_slice(),
            visible
                .after_captured_at_unix_millis
                .to_be_bytes()
                .as_slice(),
            b"exact_entry_review_visibly_confirmed_shared_gate_released_no_entry_no_spending",
        ],
    );
    release_confirmed_pending_v3(&input.input_receipt_sha256)?;
    Ok(OpaqueMtgoConfirmedCompetitiveOpenEntryReviewV1 {
        _authorization: authorization,
        _visible_confirmation: visible_confirmation,
        commitments: MtgoConfirmedCompetitiveOpenEntryReviewCommitmentsV1 {
            input_receipt_sha256: input.input_receipt_sha256,
            preparation_commitment_sha256: prepared_commitments.preparation_commitment_sha256,
            authorization_ratification_commitment_sha256: prepared_commitments
                .authorization_ratification_commitment_sha256,
            visible_confirmation: visible.clone(),
            confirmation_receipt_sha256,
            event_kind: visible.event_kind,
            event_identity_sha256: visible.event_identity_sha256,
            after_frame_id: visible.after_frame_id,
            after_frame_sequence: visible.after_frame_sequence,
            after_captured_at_unix_millis: visible.after_captured_at_unix_millis,
        },
    })
}

fn validate_competitive_lifecycle_sideboard_submit_provenance_v1(
    control: &OpaqueMtgoCompetitiveLifecycleControlV1,
) -> Result<(), String> {
    let retained_ready_commitment_sha256 = control
        ._changed_sideboard_ready
        .as_ref()
        .map(|ready| ready.ready_commitment_sha256());
    let claimed_ready_commitment_sha256 = control
        .commitments
        .changed_sideboard_ready_commitment_sha256
        .as_deref();
    validate_competitive_lifecycle_sideboard_submit_provenance_parts_v1(
        control.commitments.action,
        retained_ready_commitment_sha256,
        claimed_ready_commitment_sha256,
    )
}

fn validate_competitive_lifecycle_sideboard_submit_provenance_parts_v1(
    action: MtgoCompetitiveLifecycleActionV1,
    retained_ready_commitment_sha256: Option<&str>,
    claimed_ready_commitment_sha256: Option<&str>,
) -> Result<(), String> {
    if retained_ready_commitment_sha256 != claimed_ready_commitment_sha256 {
        return Err(
            "competitive lifecycle control changed its retained sideboard-ready provenance"
                .to_owned(),
        );
    }
    if action == MtgoCompetitiveLifecycleActionV1::SubmitSideboard {
        if retained_ready_commitment_sha256.is_none() {
            return Err(
                "generic competitive lifecycle preparation cannot submit a sideboard without exact model-selected target provenance"
                    .to_owned(),
            );
        }
    } else if retained_ready_commitment_sha256.is_some() {
        return Err(
            "changed-sideboard target provenance cannot authorize a non-sideboard lifecycle action"
                .to_owned(),
        );
    }
    Ok(())
}

pub fn prepare_ratified_competitive_lifecycle_control_v1(
    authorization: RatifiedMtgoCompetitiveLifecycleAuthorizationV1,
    control: OpaqueMtgoCompetitiveLifecycleControlV1,
) -> Result<OpaqueMtgoPreparedCompetitiveLifecycleControlV1, String> {
    validate_competitive_lifecycle_sideboard_submit_provenance_v1(&control)?;
    let control_commitments = control.commitments_v1();
    let authorization_commitments = authorization.commitments_v1();
    if control_commitments.event_kind != authorization_commitments.event_kind
        || control_commitments.approved_account_alias_sha256
            != authorization_commitments.approved_account_alias_sha256
        || control_commitments.source_frame.profile_commitment_sha256
            != authorization_commitments.lifecycle_profile_commitment_sha256
        || control_commitments
            .source_frame
            .profile_admission_commitment_sha256
            != authorization_commitments.lifecycle_profile_admission_commitment_sha256
        || authorization_commitments.allowed_actions_commitment_sha256
            != competitive_lifecycle_allowed_actions_commitment_v1()
        || !is_non_entry_lifecycle_action_v1(control_commitments.action)
    {
        return Err(
            "competitive lifecycle control differs from its exact ratified mode, account, profile, or action set"
                .to_owned(),
        );
    }
    let pointer_target = resolve_competitive_entry_pointer_target_v1(
        &control._source,
        &control._control_rect_client_px,
    )?;
    let source_captured_at_unix_millis = control_commitments
        .source_frame
        .source_capture
        .captured_at_unix_millis;
    let preparation_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_LIFECYCLE_PREPARATION_DOMAIN_V1,
        &[
            authorization_commitments
                .ratification_commitment_sha256
                .as_bytes(),
            control_commitments
                .control_binding_commitment_sha256
                .as_bytes(),
            control_commitments
                .source_frame
                .source_capture
                .capture_commitment_sha256
                .as_bytes(),
            control_commitments
                .classification_result_commitment_sha256
                .as_bytes(),
            competitive_event_kind_tag_v1(control_commitments.event_kind),
            competitive_lifecycle_action_tag_v1(control_commitments.action),
            control_commitments.event_identity_sha256.as_bytes(),
            control_commitments
                .match_identity_sha256
                .as_deref()
                .unwrap_or("")
                .as_bytes(),
            control_commitments
                .game_number
                .map_or([0_u8, 0_u8], |value| [1_u8, value])
                .as_slice(),
            control_commitments.frame_id.to_be_bytes().as_slice(),
            control_commitments.frame_sequence.to_be_bytes().as_slice(),
            source_captured_at_unix_millis.to_be_bytes().as_slice(),
            b"fresh_exact_enabled_control_one_click_pending_visible_postcondition",
        ],
    );
    let commitments = MtgoPreparedCompetitiveLifecycleControlCommitmentsV1 {
        lifecycle_authorization_commitment_sha256: authorization_commitments
            .ratification_commitment_sha256,
        control_binding_commitment_sha256: control_commitments.control_binding_commitment_sha256,
        preparation_commitment_sha256,
        event_kind: control_commitments.event_kind,
        action: control_commitments.action,
        event_identity_sha256: control_commitments.event_identity_sha256,
        match_identity_sha256: control_commitments.match_identity_sha256,
        game_number: control_commitments.game_number,
        source_frame_id: control_commitments.frame_id,
        source_frame_sequence: control_commitments.frame_sequence,
        source_captured_at_unix_millis,
    };
    Ok(OpaqueMtgoPreparedCompetitiveLifecycleControlV1 {
        authorization,
        control,
        pointer_target,
        commitments,
    })
}

pub fn execute_prepared_competitive_lifecycle_control_v1(
    prepared: OpaqueMtgoPreparedCompetitiveLifecycleControlV1,
) -> Result<OpaqueMtgoPendingCompetitiveLifecycleControlV1, String> {
    reserve_input_gate_v3()?;
    let input_sent_at_unix_millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(error) => {
            release_unattempted_reservation_v3()?;
            return Err(format!("system clock is before epoch: {error}"));
        }
    };
    let prepared_commitments = prepared.commitments_v1();
    if validate_preinput_capture_freshness_v3(
        prepared_commitments.source_captured_at_unix_millis,
        input_sent_at_unix_millis,
    )
    .is_err()
    {
        release_unattempted_reservation_v3()?;
        return Err(
            "the competitive lifecycle control capture is stale or future-dated".to_owned(),
        );
    }
    halt_before_input_attempt_v3()?;
    let cursor_parked_outside_client = send_exactly_one_left_click_v3(&prepared)?;
    let input_receipt_sha256 = hash_parts_v2(
        COMPETITIVE_LIFECYCLE_INPUT_RECEIPT_DOMAIN_V1,
        &[
            prepared_commitments
                .preparation_commitment_sha256
                .as_bytes(),
            prepared_commitments
                .lifecycle_authorization_commitment_sha256
                .as_bytes(),
            competitive_event_kind_tag_v1(prepared_commitments.event_kind),
            competitive_lifecycle_action_tag_v1(prepared_commitments.action),
            prepared_commitments
                .source_frame_id
                .to_be_bytes()
                .as_slice(),
            prepared_commitments
                .source_frame_sequence
                .to_be_bytes()
                .as_slice(),
            input_sent_at_unix_millis.to_be_bytes().as_slice(),
            &[u8::from(cursor_parked_outside_client)],
            b"exactly_one_left_click_shared_gate_pending_visible_postcondition",
        ],
    );
    set_pending_v3(&input_receipt_sha256)?;
    let commitments = MtgoCompetitiveLifecycleInputReceiptCommitmentsV1 {
        preparation_commitment_sha256: prepared_commitments.preparation_commitment_sha256,
        lifecycle_authorization_commitment_sha256: prepared_commitments
            .lifecycle_authorization_commitment_sha256,
        input_receipt_sha256,
        event_kind: prepared_commitments.event_kind,
        action: prepared_commitments.action,
        source_frame_id: prepared_commitments.source_frame_id,
        source_frame_sequence: prepared_commitments.source_frame_sequence,
        input_sent_at_unix_millis,
        cursor_parked_outside_client,
    };
    Ok(OpaqueMtgoPendingCompetitiveLifecycleControlV1 {
        prepared,
        commitments,
    })
}

pub fn confirm_pending_competitive_lifecycle_control_v1(
    pending: OpaqueMtgoPendingCompetitiveLifecycleControlV1,
    after: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
) -> Result<OpaqueMtgoConfirmedCompetitiveLifecycleControlV1, String> {
    require_matching_pending_v3(&pending.commitments.input_receipt_sha256)?;
    let OpaqueMtgoPendingCompetitiveLifecycleControlV1 {
        prepared,
        commitments: input,
    } = pending;
    let OpaqueMtgoPreparedCompetitiveLifecycleControlV1 {
        authorization,
        control,
        pointer_target: _,
        commitments: prepared_commitments,
    } = prepared;
    let visible_postcondition = match confirm_opaque_competitive_lifecycle_control_postcondition_v1(
        control,
        after,
        &authorization.scope,
        input.input_sent_at_unix_millis,
    ) {
        Ok(value) => value,
        Err(error) => {
            halt_gate_v3()?;
            return Err(format!(
                "competitive lifecycle postcondition failed and the input gate is halted: {error}"
            ));
        }
    };
    let visible = visible_postcondition.commitments_v1();
    if visible.control_binding_commitment_sha256
        != prepared_commitments.control_binding_commitment_sha256
        || visible.event_kind != prepared_commitments.event_kind
        || visible.action != prepared_commitments.action
        || visible.event_identity_sha256 != prepared_commitments.event_identity_sha256
        || visible.match_identity_sha256 != prepared_commitments.match_identity_sha256
        || visible.before_frame_id != prepared_commitments.source_frame_id
        || visible.before_frame_sequence != prepared_commitments.source_frame_sequence
        || visible.after_frame_sequence <= prepared_commitments.source_frame_sequence
        || visible.after_captured_at_unix_millis <= input.input_sent_at_unix_millis
    {
        halt_gate_v3()?;
        return Err(
            "competitive lifecycle confirmation changed the exact action lineage and halted the input gate"
                .to_owned(),
        );
    }
    let confirmation_receipt_sha256 = hash_parts_v2(
        COMPETITIVE_LIFECYCLE_CONFIRMATION_RECEIPT_DOMAIN_V1,
        &[
            input.input_receipt_sha256.as_bytes(),
            prepared_commitments
                .preparation_commitment_sha256
                .as_bytes(),
            prepared_commitments
                .lifecycle_authorization_commitment_sha256
                .as_bytes(),
            visible.transition_commitment_sha256.as_bytes(),
            competitive_event_kind_tag_v1(visible.event_kind),
            competitive_lifecycle_action_tag_v1(visible.action),
            visible.after_frame_id.to_be_bytes().as_slice(),
            visible.after_frame_sequence.to_be_bytes().as_slice(),
            visible
                .after_captured_at_unix_millis
                .to_be_bytes()
                .as_slice(),
            b"exact_action_visibly_confirmed_shared_gate_released",
        ],
    );
    release_confirmed_pending_v3(&input.input_receipt_sha256)?;
    Ok(OpaqueMtgoConfirmedCompetitiveLifecycleControlV1 {
        _authorization: authorization,
        _visible_postcondition: visible_postcondition,
        commitments: MtgoConfirmedCompetitiveLifecycleControlCommitmentsV1 {
            input_receipt_sha256: input.input_receipt_sha256,
            preparation_commitment_sha256: prepared_commitments.preparation_commitment_sha256,
            lifecycle_authorization_commitment_sha256: prepared_commitments
                .lifecycle_authorization_commitment_sha256,
            visible_transition: visible.clone(),
            confirmation_receipt_sha256,
            event_kind: visible.event_kind,
            action: visible.action,
            after_frame_id: visible.after_frame_id,
            after_frame_sequence: visible.after_frame_sequence,
            after_captured_at_unix_millis: visible.after_captured_at_unix_millis,
        },
    })
}

/// Starts the single-event coordinator only after an exact entry click has a
/// strictly newer entered-waiting visible postcondition. The separately
/// ratified lifecycle authority must name the same account, mode, evaluated
/// profile, and admission. Production construction remains disabled while the
/// entry and lifecycle ratification roots are empty.
pub fn begin_competitive_event_runtime_after_entry_v1(
    confirmed_entry: OpaqueMtgoConfirmedCompetitiveEntryV1,
    lifecycle_authorization: RatifiedMtgoCompetitiveLifecycleAuthorizationV1,
    deck_manifest: &ValidatedMtgoCompetitiveDeckManifestV1,
) -> Result<OpaqueMtgoCompetitiveEventRuntimeV1, String> {
    let OpaqueMtgoConfirmedCompetitiveEntryV1 {
        _authorization: spent_entry_authorization,
        _visible_confirmation: visible_confirmation,
        commitments: entry,
    } = confirmed_entry;
    let visible = visible_confirmation.commitments_v1();
    let current_frame = visible_confirmation.into_after_frame_v1();
    let current = current_frame.commitments_v1();
    let lifecycle = current_frame.lifecycle_snapshot_v1();
    let entry_ratification = spent_entry_authorization.commitments_v1();
    let entry_deck_list_sha256 = spent_entry_authorization
        ._open_entry_review_authorization
        .commitments
        .deck_list_sha256
        .clone();
    let entry_deck_manifest_commitment_sha256 = spent_entry_authorization
        ._open_entry_review_authorization
        .commitments
        .deck_manifest_commitment_sha256
        .clone();
    let lifecycle_ratification = lifecycle_authorization.commitments_v1();
    validate_competitive_event_authorization_lineage_v1(
        &entry_ratification,
        &lifecycle_ratification,
    )?;
    if current.phase != MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing
        || lifecycle.phase() != MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing
        || entry.event_kind != lifecycle_ratification.event_kind
        || entry.event_kind != current.event_kind
        || entry_ratification.event_kind != entry.event_kind
        || entry.entry_ratification_commitment_sha256
            != entry_ratification.ratification_commitment_sha256
        || entry_ratification.deck_manifest_sha256 != entry_deck_manifest_commitment_sha256
        || entry_ratification.deck_manifest_sha256 != deck_manifest.manifest_commitment_sha256()
        || entry_deck_list_sha256 != deck_manifest.deck_list_sha256()
        || entry_ratification.deck_format_sha256 != deck_manifest.format_sha256()
        || visible
            .frame_transition
            .after_lifecycle_snapshot_commitment_sha256
            != current.lifecycle_snapshot_commitment_sha256
        || visible.frame_transition.after_frame_id != current.frame_id
        || visible.frame_transition.after_frame_sequence != current.frame_sequence
        || visible.frame_transition.event_identity_sha256
            != entry_ratification.event_identity_sha256
        || lifecycle.event_identity_sha256_v1()
            != Some(entry_ratification.event_identity_sha256.as_str())
        || lifecycle_ratification.lifecycle_profile_commitment_sha256
            != current.source_frame.profile_commitment_sha256
        || lifecycle_ratification.lifecycle_profile_admission_commitment_sha256
            != current.source_frame.profile_admission_commitment_sha256
        || lifecycle_ratification.approved_account_alias_sha256
            != current.source_frame.approved_account_alias_sha256
    {
        return Err(
            "competitive event runtime entry, account, mode, profile, or visible postcondition changed"
                .to_owned(),
        );
    }
    let player_known_deck_state =
        MtgoCompetitivePlayerKnownDeckStateV1::from_manifest_v1(deck_manifest)?;
    let player_known_current_deck_configuration_commitment_sha256 =
        player_known_deck_state.current_commitment_v1()?;
    let mut commitments = MtgoCompetitiveEventRuntimeCommitmentsV1 {
        runtime_commitment_sha256: String::new(),
        entry_confirmation_receipt_sha256: entry.confirmation_receipt_sha256,
        entry_ratification_commitment_sha256: entry_ratification.ratification_commitment_sha256,
        entry_authorization_sha256: entry_ratification.entry_authorization_sha256,
        correspondence_sha256: entry_ratification.correspondence_sha256,
        permission_review_commitment_sha256: entry_ratification.permission_review_commitment_sha256,
        deck_list_sha256: entry_deck_list_sha256,
        deck_manifest_sha256: entry_ratification.deck_manifest_sha256,
        deck_format_sha256: entry_ratification.deck_format_sha256,
        player_known_current_deck_configuration_commitment_sha256,
        selected_deck_label_sha256: entry_ratification.selected_deck_label_sha256,
        selected_deck_region_sha256: entry_ratification.selected_deck_region_sha256,
        policy_deployment_commitment_sha256: entry_ratification.policy_deployment_commitment_sha256,
        lifecycle_authorization_commitment_sha256: lifecycle_ratification
            .ratification_commitment_sha256,
        mode_authorization_commitment_sha256: lifecycle_ratification
            .mode_authorization_commitment_sha256,
        navigation_profile_commitment_sha256: lifecycle_ratification
            .lifecycle_profile_commitment_sha256,
        navigation_profile_admission_commitment_sha256: lifecycle_ratification
            .lifecycle_profile_admission_commitment_sha256,
        approved_account_alias_sha256: lifecycle_ratification.approved_account_alias_sha256,
        bound_event_identity_sha256: entry_ratification.event_identity_sha256,
        event_kind: entry.event_kind,
        current_phase: current.phase,
        current_lifecycle_snapshot_commitment_sha256: current.lifecycle_snapshot_commitment_sha256,
        current_match_identity_sha256: lifecycle.match_identity_sha256_v1().map(str::to_owned),
        current_game_number: lifecycle.game_number_v1(),
        current_frame_id: current.frame_id,
        current_frame_sequence: current.frame_sequence,
        lifecycle_transition_count: 0,
        confirmed_lifecycle_action_count: 0,
        observed_lifecycle_advance_count: 0,
        pregame_session_count: 0,
        last_completed_pregame: None,
        gameplay_lease_count: 0,
        last_returned_gameplay_frame_sequence: None,
        event_monitor_chain_commitment_sha256: None,
        event_monitor_observation_count: 0,
        terminal_event_record_confirmed: false,
        closed_to_event_browser: false,
    };
    commitments.runtime_commitment_sha256 = competitive_event_runtime_commitment_v1(
        COMPETITIVE_EVENT_RUNTIME_INITIAL_DOMAIN_V1,
        None,
        &commitments,
        b"entered_waiting_after_exactly_one_confirmed_entry",
    );
    Ok(OpaqueMtgoCompetitiveEventRuntimeV1 {
        _spent_entry_authorization: spent_entry_authorization,
        lifecycle_authorization,
        current_frame,
        player_known_deck_state,
        event_monitor: None,
        commitments,
    })
}

/// Consumes the event coordinator while its exact current Sideboarding frame
/// is parsed against the entry-bound deck and policy. The parser must already
/// have passed the four-slice League/Challenge sideboard evaluation. No drag or
/// Submit Deck authorization is required until a model selects a changed target.
/// This measurement neither releases input authority nor permits submission.
pub fn measure_competitive_event_runtime_sideboard_v1(
    runtime: OpaqueMtgoCompetitiveEventRuntimeV1,
    manifest: ValidatedMtgoCompetitiveDeckManifestV1,
    sideboard_evaluation: AdmittedMtgoCompetitiveSideboardEvaluationV1,
    classifier_runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoMeasuredCompetitiveEventSideboardV1, String> {
    if runtime.commitments.closed_to_event_browser
        || runtime.commitments.current_phase != MtgoCompetitiveLifecyclePhaseV1::Sideboarding
        || manifest.deck_list_sha256() != runtime.commitments.deck_list_sha256
        || manifest.format_sha256() != runtime.commitments.deck_format_sha256
    {
        return Err(
            "sideboard measurement requires the current exact between-game event, deck list, and format"
                .to_owned(),
        );
    }
    let OpaqueMtgoCompetitiveEventRuntimeV1 {
        _spent_entry_authorization,
        lifecycle_authorization,
        current_frame,
        player_known_deck_state,
        event_monitor,
        commitments: prior,
    } = runtime;
    let sideboard_evaluation_commitments = sideboard_evaluation.commitments_v1();
    if sideboard_evaluation_commitments.profile_commitment_sha256
        != prior.navigation_profile_commitment_sha256
        || sideboard_evaluation_commitments.approved_account_alias_sha256
            != prior.approved_account_alias_sha256
        || sideboard_evaluation_commitments.deck_list_sha256 != prior.deck_list_sha256
        || sideboard_evaluation_commitments.deck_manifest_commitment_sha256
            != manifest.manifest_commitment_sha256()
        || sideboard_evaluation_commitments.deck_format_sha256 != prior.deck_format_sha256
        || sideboard_evaluation_commitments.policy_deployment_commitment_sha256
            != prior.policy_deployment_commitment_sha256
    {
        return Err(
            "sideboard evaluation differs from the exact event, account, deck, or policy"
                .to_owned(),
        );
    }
    let classified = classify_checked_untrusted_competitive_sideboard_v1(
        current_frame,
        &manifest,
        prior.policy_deployment_commitment_sha256.clone(),
        classifier_runtime,
        timeout_ms,
    )?;
    let sideboard = classified.commitments_v1();
    if sideboard.navigation_profile_commitment_sha256 != prior.navigation_profile_commitment_sha256
        || sideboard.navigation_profile_admission_commitment_sha256
            != prior.navigation_profile_admission_commitment_sha256
        || sideboard.approved_account_alias_sha256 != prior.approved_account_alias_sha256
        || sideboard.source_navigation_classification_result_commitment_sha256
            != classified
                .source_frame
                .commitments_v1()
                .classification_result_commitment_sha256
        || sideboard.source_lifecycle_snapshot_commitment_sha256
            != prior.current_lifecycle_snapshot_commitment_sha256
        || sideboard.deck_list_sha256 != prior.deck_list_sha256
        || sideboard.deck_format_sha256 != prior.deck_format_sha256
        || sideboard.policy_deployment_commitment_sha256
            != prior.policy_deployment_commitment_sha256
        || sideboard.event_kind != prior.event_kind
        || sideboard.event_identity_sha256 != prior.bound_event_identity_sha256
        || prior.current_match_identity_sha256.as_deref()
            != Some(sideboard.match_identity_sha256.as_str())
        || prior.current_game_number != Some(sideboard.game_number)
        || sideboard.frame_id != prior.current_frame_id
        || sideboard.frame_sequence != prior.current_frame_sequence
    {
        return Err("sideboard measurement changed the exact event runtime lineage".to_owned());
    }
    if visible_native_sideboard_configuration_v1(classified.configuration_v1())?
        != player_known_deck_state.current
        || player_known_deck_state.current_commitment_v1()?
            != prior.player_known_current_deck_configuration_commitment_sha256
    {
        return Err(
            "sideboard measurement changed the retained player-known current deck".to_owned(),
        );
    }
    let measurement_binding_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_EVENT_SIDEBOARD_MEASUREMENT_DOMAIN_V1,
        &[
            prior.runtime_commitment_sha256.as_bytes(),
            sideboard_evaluation_commitments
                .ratification_commitment_sha256
                .as_bytes(),
            sideboard_evaluation
                .admission_commitment_sha256_v1()
                .as_bytes(),
            sideboard.classification_result_commitment_sha256.as_bytes(),
            sideboard.sideboard_snapshot_commitment_sha256.as_bytes(),
            sideboard.deck_list_sha256.as_bytes(),
            sideboard.deck_manifest_commitment_sha256.as_bytes(),
            sideboard.deck_format_sha256.as_bytes(),
            sideboard.policy_deployment_commitment_sha256.as_bytes(),
            sideboard.frame_sequence.to_be_bytes().as_slice(),
            b"event_runtime_withheld_during_checked_untrusted_sideboard_measurement_no_input_no_submit",
        ],
    );
    let commitments = MtgoMeasuredCompetitiveEventSideboardCommitmentsV1 {
        prior_event_runtime_commitment_sha256: prior.runtime_commitment_sha256.clone(),
        sideboard_evaluation_ratification_commitment_sha256: sideboard_evaluation_commitments
            .ratification_commitment_sha256,
        sideboard_evaluation_admission_commitment_sha256: sideboard_evaluation
            .admission_commitment_sha256_v1()
            .to_owned(),
        sideboard_classification: sideboard,
        measurement_binding_commitment_sha256,
    };
    Ok(OpaqueMtgoMeasuredCompetitiveEventSideboardV1 {
        _spent_entry_authorization,
        _lifecycle_authorization: lifecycle_authorization,
        _sideboard_evaluation: sideboard_evaluation,
        classified,
        _player_known_deck_state: player_known_deck_state,
        _manifest: manifest,
        _event_monitor: event_monitor,
        _prior: prior,
        commitments,
    })
}

/// Joins the exact visible Sideboarding configuration with one exact
/// prior-game visible outcome. The model-facing payload contains only game
/// information. This does not call a checkpoint, select a target, move a
/// card, or submit the sideboard.
pub fn bind_competitive_event_native_sideboard_request_v1(
    measurement: OpaqueMtgoMeasuredCompetitiveEventSideboardV1,
    outcome: OpaqueMtgoCompetitiveVisibleGameOutcomeV1,
) -> Result<OpaqueMtgoCompetitiveNativeSideboardRequestV1, String> {
    let sideboard = &measurement.commitments.sideboard_classification;
    let outcome_lineage = outcome.lineage_v1();
    if sideboard.event_kind != outcome_lineage.event_kind
        || sideboard.event_identity_sha256 != outcome_lineage.event_identity_sha256
        || sideboard.match_identity_sha256 != outcome_lineage.match_identity_sha256
        || sideboard.game_number != outcome_lineage.game_number
        || measurement._prior.current_phase != MtgoCompetitiveLifecyclePhaseV1::Sideboarding
        || measurement._prior.current_game_number != Some(outcome_lineage.game_number)
    {
        return Err(
            "native sideboard request changed the exact prior-game event or match lineage"
                .to_owned(),
        );
    }
    let next_game_number = outcome_lineage
        .game_number
        .checked_add(1)
        .filter(|game| *game <= 3)
        .ok_or("native sideboard request has no next best-of-three game")?;
    let (acting_player_games_won, opponent_games_won) = native_sideboard_score_from_prior_game_v1(
        outcome_lineage.game_number,
        outcome_lineage.winner,
    )?;
    let model_input = MtgoCompetitiveNativeSideboardModelInputV1 {
        next_game_number,
        acting_player_games_won,
        opponent_games_won,
        current_configuration: visible_native_sideboard_configuration_v1(
            measurement.configuration_v1(),
        )?,
    };
    validate_competitive_native_sideboard_model_input_v1(&model_input)?;
    let model_input_commitment_sha256 =
        competitive_native_sideboard_model_input_commitment_v1(&model_input)?;
    let request_binding_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_NATIVE_SIDEBOARD_REQUEST_BINDING_DOMAIN_V1,
        &[
            model_input_commitment_sha256.as_bytes(),
            measurement
                .commitments
                .measurement_binding_commitment_sha256
                .as_bytes(),
            measurement
                .commitments
                .prior_event_runtime_commitment_sha256
                .as_bytes(),
            measurement
                .commitments
                .sideboard_evaluation_ratification_commitment_sha256
                .as_bytes(),
            measurement
                .commitments
                .sideboard_evaluation_admission_commitment_sha256
                .as_bytes(),
            sideboard.sideboard_snapshot_commitment_sha256.as_bytes(),
            sideboard.deck_manifest_commitment_sha256.as_bytes(),
            sideboard.policy_deployment_commitment_sha256.as_bytes(),
            outcome_lineage.source_memory_commitment_sha256.as_bytes(),
            outcome_lineage.outcome_commitment_sha256.as_bytes(),
            b"opaque_adapter_lineage_not_model_game_information_no_selection_no_input_no_submit",
        ],
    );
    Ok(OpaqueMtgoCompetitiveNativeSideboardRequestV1 {
        _measurement: measurement,
        _outcome: outcome,
        model_input,
        model_input_commitment_sha256,
        _request_binding_commitment_sha256: request_binding_commitment_sha256,
    })
}

fn native_sideboard_score_from_prior_game_v1(
    prior_game_number: u8,
    winner: MtgoCompetitivePlayerRelativeGameWinnerV1,
) -> Result<(u8, u8), String> {
    match (prior_game_number, winner) {
        (1, MtgoCompetitivePlayerRelativeGameWinnerV1::ActingPlayer) => Ok((1, 0)),
        (1, MtgoCompetitivePlayerRelativeGameWinnerV1::Opponent) => Ok((0, 1)),
        (2, _) => Ok((1, 1)),
        _ => Err("native sideboard request visible score is invalid".to_owned()),
    }
}

pub fn begin_competitive_event_sideboard_transfer_sequence_v1(
    planned_event: OpaqueMtgoPlannedCompetitiveEventSideboardV1,
) -> Result<OpaqueMtgoCompetitiveEventSideboardSequenceV1, String> {
    let event_commitments = planned_event.commitments.clone();
    let OpaqueMtgoPlannedCompetitiveEventSideboardV1 {
        _spent_entry_authorization,
        lifecycle_authorization,
        sideboard_authorization,
        planned,
        player_known_deck_state,
        manifest,
        event_monitor,
        prior,
        commitments: _,
    } = planned_event;
    if event_commitments.sideboard_automation_ratification_commitment_sha256
        != sideboard_authorization
            .commitments
            .ratification_commitment_sha256
    {
        return Err("sideboard plan lost its exact automation authorization".to_owned());
    }
    let OpaqueMtgoPlannedCompetitiveSideboardV1 {
        source_frame,
        plan,
        visible_cards,
        mainboard_zone,
        sideboard_zone,
        commitments: plan_commitments,
    } = planned;
    if plan_commitments != event_commitments.sideboard_plan
        || plan_commitments.no_changes_selected
        || plan_commitments.transfer_count == 0
    {
        return Err(
            "changed-sideboard transfer sequencing requires the exact nonempty event plan"
                .to_owned(),
        );
    }
    let atomic_transfers = expand_atomic_sideboard_transfers_v1(plan.transfers_v1())?;
    let total_transfer_steps = u16::try_from(atomic_transfers.len())
        .map_err(|_| "atomic sideboard transfer count overflow")?;
    if total_transfer_steps == 0 {
        return Err("changed-sideboard transfer sequence is empty".to_owned());
    }
    let next_transfer = atomic_transfers[0].clone();
    let sequence_chain_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_EVENT_SIDEBOARD_SEQUENCE_DOMAIN_V1,
        &[
            prior.runtime_commitment_sha256.as_bytes(),
            event_commitments
                .event_plan_binding_commitment_sha256
                .as_bytes(),
            event_commitments
                .sideboard_automation_ratification_commitment_sha256
                .as_bytes(),
            plan_commitments.plan_commitment_sha256.as_bytes(),
            plan_commitments
                .source_snapshot_commitment_sha256
                .as_bytes(),
            total_transfer_steps.to_be_bytes().as_slice(),
            b"sideboard_to_mainboard_first_one_card_per_step_visible_confirmation_required_no_input",
        ],
    );
    let commitments = MtgoCompetitiveEventSideboardSequenceCommitmentsV1 {
        prior_event_runtime_commitment_sha256: prior.runtime_commitment_sha256.clone(),
        event_plan_binding_commitment_sha256: event_commitments
            .event_plan_binding_commitment_sha256,
        plan_commitment_sha256: plan_commitments.plan_commitment_sha256,
        sequence_chain_commitment_sha256,
        current_sideboard_snapshot_commitment_sha256: plan_commitments
            .source_snapshot_commitment_sha256,
        current_frame_sequence: plan_commitments.source_frame_sequence,
        total_transfer_steps,
        confirmed_transfer_steps: 0,
        next_transfer,
    };
    let current_configuration = plan.source_configuration_v1().clone();
    Ok(OpaqueMtgoCompetitiveEventSideboardSequenceV1 {
        _spent_entry_authorization,
        lifecycle_authorization,
        sideboard_authorization,
        current_frame: source_frame,
        manifest,
        plan,
        player_known_deck_state,
        current_configuration,
        current_visible_cards: visible_cards,
        current_mainboard_zone: mainboard_zone,
        current_sideboard_zone: sideboard_zone,
        atomic_transfers,
        event_monitor,
        prior,
        commitments,
    })
}

pub fn prepare_competitive_event_sideboard_transfer_drag_v1(
    sequence: OpaqueMtgoCompetitiveEventSideboardSequenceV1,
) -> Result<OpaqueMtgoPreparedCompetitiveEventSideboardTransferV1, String> {
    let transfer = sequence.commitments.next_transfer.clone();
    if usize::from(transfer.step_index) >= sequence.atomic_transfers.len()
        || sequence.atomic_transfers[usize::from(transfer.step_index)] != transfer
        || transfer.step_index != sequence.commitments.confirmed_transfer_steps
    {
        return Err("sideboard sequence next-transfer index changed".to_owned());
    }
    let source_partition = match transfer.direction {
        MtgoCompetitiveSideboardTransferDirectionV1::SideboardToMainboard => {
            MtgoCompetitiveDeckPartitionV1::Sideboard
        }
        MtgoCompetitiveSideboardTransferDirectionV1::MainboardToSideboard => {
            MtgoCompetitiveDeckPartitionV1::Mainboard
        }
    };
    let source_card = sequence
        .current_visible_cards
        .iter()
        .find(|card| {
            card.partition == source_partition
                && card.card_name == transfer.card_name
                && card.count > 0
        })
        .cloned()
        .ok_or("next sideboard transfer source card is not visibly present")?;
    let destination_zone = match transfer.direction {
        MtgoCompetitiveSideboardTransferDirectionV1::SideboardToMainboard => {
            sequence.current_mainboard_zone.clone()
        }
        MtgoCompetitiveSideboardTransferDirectionV1::MainboardToSideboard => {
            sequence.current_sideboard_zone.clone()
        }
    };
    let preparation_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_EVENT_SIDEBOARD_TRANSFER_PREPARATION_DOMAIN_V1,
        &[
            sequence
                .commitments
                .sequence_chain_commitment_sha256
                .as_bytes(),
            sequence.commitments.plan_commitment_sha256.as_bytes(),
            sequence
                .commitments
                .current_sideboard_snapshot_commitment_sha256
                .as_bytes(),
            transfer.step_index.to_be_bytes().as_slice(),
            transfer.card_name.as_bytes(),
            competitive_sideboard_transfer_direction_tag_v1(transfer.direction),
            source_card.rect_client_px.x.to_be_bytes().as_slice(),
            source_card.rect_client_px.y.to_be_bytes().as_slice(),
            source_card.rect_client_px.width.to_be_bytes().as_slice(),
            source_card.rect_client_px.height.to_be_bytes().as_slice(),
            source_card.content_sha256.as_bytes(),
            destination_zone
                .empty_drop_rect_client_px
                .x
                .to_be_bytes()
                .as_slice(),
            destination_zone
                .empty_drop_rect_client_px
                .y
                .to_be_bytes()
                .as_slice(),
            destination_zone
                .empty_drop_rect_client_px
                .width
                .to_be_bytes()
                .as_slice(),
            destination_zone
                .empty_drop_rect_client_px
                .height
                .to_be_bytes()
                .as_slice(),
            destination_zone.empty_drop_content_sha256.as_bytes(),
            sequence
                .commitments
                .current_frame_sequence
                .to_be_bytes()
                .as_slice(),
            b"official_mtgo_drag_between_visible_zones_preparation_only_no_input",
        ],
    );
    let commitments = MtgoPreparedCompetitiveEventSideboardTransferCommitmentsV1 {
        sequence_chain_commitment_sha256: sequence
            .commitments
            .sequence_chain_commitment_sha256
            .clone(),
        plan_commitment_sha256: sequence.commitments.plan_commitment_sha256.clone(),
        source_sideboard_snapshot_commitment_sha256: sequence
            .commitments
            .current_sideboard_snapshot_commitment_sha256
            .clone(),
        transfer,
        source_card_region_sha256: source_card.content_sha256.clone(),
        destination_empty_drop_region_sha256: destination_zone.empty_drop_content_sha256.clone(),
        source_frame_sequence: sequence.commitments.current_frame_sequence,
        preparation_commitment_sha256,
    };
    Ok(OpaqueMtgoPreparedCompetitiveEventSideboardTransferV1 {
        sequence,
        _source_card: source_card,
        _destination_zone: destination_zone,
        commitments,
    })
}

/// Performs the mandatory immediate main-client recapture before one
/// sideboard drag. The fresh navigation and sideboard classifiers must show
/// the exact unchanged configuration and next semantic transfer. Only then
/// are private source and destination points resolved.
pub fn prepare_fresh_competitive_event_sideboard_transfer_drag_v1(
    prepared: OpaqueMtgoPreparedCompetitiveEventSideboardTransferV1,
    profile: &AdmittedMtgoCompetitiveNavigationProfileV1,
    classifier_runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    immediate_identity: MtgoCompetitiveNavigationFrameIdentityV1,
    capture_timeout_ms: u32,
    classifier_timeout_ms: u32,
    sideboard_classifier_timeout_ms: u32,
) -> Result<OpaqueMtgoFreshPreparedCompetitiveEventSideboardDragV1, String> {
    let OpaqueMtgoPreparedCompetitiveEventSideboardTransferV1 {
        mut sequence,
        _source_card: _,
        _destination_zone: _,
        commitments: prior_preparation,
    } = prepared;
    let expected_immediate_sequence = sequence
        .commitments
        .current_frame_sequence
        .checked_add(1)
        .ok_or("sideboard immediate frame sequence overflow")?;
    if immediate_identity.frame_sequence != expected_immediate_sequence
        || immediate_identity.frame_id == 0
    {
        return Err(
            "sideboard immediate recapture must use the exact next nonzero frame identity"
                .to_owned(),
        );
    }
    let profile_runtime = profile.checked_runtime_profile();
    let sideboard_authorization = sequence.sideboard_authorization.commitments_v1();
    if profile.profile_commitment_sha256() != sequence.prior.navigation_profile_commitment_sha256
        || profile.admission_commitment_sha256()
            != sequence
                .prior
                .navigation_profile_admission_commitment_sha256
        || profile_runtime.approved_account_alias_sha256()
            != sequence.prior.approved_account_alias_sha256
        || sideboard_authorization.ratification_commitment_sha256
            != sequence
                .sideboard_authorization
                .commitments
                .ratification_commitment_sha256
        || prior_preparation.sequence_chain_commitment_sha256
            != sequence.commitments.sequence_chain_commitment_sha256
        || prior_preparation.plan_commitment_sha256 != sequence.commitments.plan_commitment_sha256
        || prior_preparation.transfer != sequence.commitments.next_transfer
    {
        return Err(
            "sideboard immediate recapture changed its profile, authority, plan, or transfer lineage"
                .to_owned(),
        );
    }
    let prior_frame = sequence.current_frame.commitments_v1();
    let immediate_capture =
        capture_admitted_mtgo_competitive_navigation_frame_v1(profile, capture_timeout_ms)?;
    let immediate_navigation = classify_admitted_mtgo_competitive_navigation_frame_v1(
        immediate_capture,
        profile,
        classifier_runtime,
        immediate_identity,
        classifier_timeout_ms,
    )?;
    require_same_competitive_navigation_lineage_v1(&sequence.current_frame, &immediate_navigation)?;
    let immediate = classify_checked_untrusted_competitive_sideboard_v1(
        immediate_navigation,
        &sequence.manifest,
        sequence.prior.policy_deployment_commitment_sha256.clone(),
        classifier_runtime,
        sideboard_classifier_timeout_ms,
    )?;
    let immediate_commitments = immediate.commitments_v1();
    if immediate.configuration_v1() != &sequence.current_configuration
        || immediate_commitments.event_kind != sequence.prior.event_kind
        || immediate_commitments.event_identity_sha256 != sequence.prior.bound_event_identity_sha256
        || sequence.prior.current_match_identity_sha256.as_deref()
            != Some(immediate_commitments.match_identity_sha256.as_str())
        || sequence.prior.current_game_number != Some(immediate_commitments.game_number)
        || immediate_commitments.frame_sequence != expected_immediate_sequence
        || immediate_commitments.frame_id == prior_frame.frame_id
        || immediate_commitments.captured_at_unix_millis
            <= prior_frame
                .source_frame
                .source_capture
                .captured_at_unix_millis
        || immediate_commitments.deck_list_sha256 != sequence.prior.deck_list_sha256
        || immediate_commitments.deck_format_sha256 != sequence.prior.deck_format_sha256
        || immediate_commitments.policy_deployment_commitment_sha256
            != sequence.prior.policy_deployment_commitment_sha256
    {
        return Err(
            "sideboard immediate recapture is not the exact unchanged current configuration"
                .to_owned(),
        );
    }
    let OpaqueMtgoClassifiedCompetitiveSideboardV1 {
        source_frame,
        sideboard: _,
        visible_cards,
        mainboard_zone,
        sideboard_zone,
        commitments: _,
    } = immediate;
    sequence.current_frame = source_frame;
    sequence.current_visible_cards = visible_cards;
    sequence.current_mainboard_zone = mainboard_zone;
    sequence.current_sideboard_zone = sideboard_zone;
    sequence.commitments.sequence_chain_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_EVENT_SIDEBOARD_FRESH_DRAG_PREPARATION_DOMAIN_V1,
        &[
            sequence
                .commitments
                .sequence_chain_commitment_sha256
                .as_bytes(),
            sideboard_authorization
                .ratification_commitment_sha256
                .as_bytes(),
            prior_preparation.preparation_commitment_sha256.as_bytes(),
            immediate_commitments
                .classification_result_commitment_sha256
                .as_bytes(),
            immediate_commitments
                .sideboard_snapshot_commitment_sha256
                .as_bytes(),
            immediate_commitments
                .frame_sequence
                .to_be_bytes()
                .as_slice(),
            prior_preparation
                .transfer
                .step_index
                .to_be_bytes()
                .as_slice(),
            b"exact_next_frame_same_configuration_before_one_sideboard_drag",
        ],
    );
    sequence
        .commitments
        .current_sideboard_snapshot_commitment_sha256 = immediate_commitments
        .sideboard_snapshot_commitment_sha256
        .clone();
    sequence.commitments.current_frame_sequence = immediate_commitments.frame_sequence;
    let refreshed = prepare_competitive_event_sideboard_transfer_drag_v1(sequence)?;
    if refreshed.commitments.transfer != prior_preparation.transfer {
        return Err("sideboard immediate recapture changed the next transfer".to_owned());
    }
    let pointer_target = resolve_competitive_sideboard_drag_pointer_target_v1(
        &refreshed.sequence.current_frame,
        &refreshed._source_card.rect_client_px,
        &refreshed._destination_zone.empty_drop_rect_client_px,
    )?;
    let fresh_drag_preparation_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_EVENT_SIDEBOARD_FRESH_DRAG_PREPARATION_DOMAIN_V1,
        &[
            sideboard_authorization
                .ratification_commitment_sha256
                .as_bytes(),
            refreshed
                .commitments
                .preparation_commitment_sha256
                .as_bytes(),
            refreshed
                .commitments
                .sequence_chain_commitment_sha256
                .as_bytes(),
            immediate_commitments
                .classification_result_commitment_sha256
                .as_bytes(),
            immediate_commitments
                .sideboard_snapshot_commitment_sha256
                .as_bytes(),
            refreshed.commitments.source_card_region_sha256.as_bytes(),
            refreshed
                .commitments
                .destination_empty_drop_region_sha256
                .as_bytes(),
            immediate_commitments.frame_id.to_be_bytes().as_slice(),
            immediate_commitments
                .frame_sequence
                .to_be_bytes()
                .as_slice(),
            immediate_commitments
                .captured_at_unix_millis
                .to_be_bytes()
                .as_slice(),
            b"private_fresh_drag_points_resolved_no_input_yet",
        ],
    );
    let commitments = MtgoFreshPreparedCompetitiveEventSideboardDragCommitmentsV1 {
        sideboard_automation_ratification_commitment_sha256: sideboard_authorization
            .ratification_commitment_sha256,
        sequence_chain_commitment_sha256: refreshed
            .commitments
            .sequence_chain_commitment_sha256
            .clone(),
        plan_commitment_sha256: refreshed.commitments.plan_commitment_sha256.clone(),
        transfer: refreshed.commitments.transfer.clone(),
        immediate_navigation_classification_commitment_sha256: immediate_commitments
            .source_navigation_classification_result_commitment_sha256,
        immediate_sideboard_classification_commitment_sha256: immediate_commitments
            .classification_result_commitment_sha256,
        immediate_sideboard_snapshot_commitment_sha256: immediate_commitments
            .sideboard_snapshot_commitment_sha256,
        immediate_source_card_region_sha256: refreshed
            .commitments
            .source_card_region_sha256
            .clone(),
        immediate_destination_drop_region_sha256: refreshed
            .commitments
            .destination_empty_drop_region_sha256
            .clone(),
        immediate_frame_id: immediate_commitments.frame_id,
        immediate_frame_sequence: immediate_commitments.frame_sequence,
        immediate_captured_at_unix_millis: immediate_commitments.captured_at_unix_millis,
        fresh_drag_preparation_commitment_sha256,
    };
    Ok(OpaqueMtgoFreshPreparedCompetitiveEventSideboardDragV1 {
        prepared: refreshed,
        pointer_target,
        commitments,
    })
}

pub fn execute_fresh_competitive_event_sideboard_drag_v1(
    prepared: OpaqueMtgoFreshPreparedCompetitiveEventSideboardDragV1,
) -> Result<OpaqueMtgoPendingCompetitiveEventSideboardDragV1, String> {
    reserve_input_gate_v3()?;
    let input_sent_at_unix_millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(error) => {
            release_unattempted_reservation_v3()?;
            return Err(format!("system clock is before epoch: {error}"));
        }
    };
    if validate_preinput_capture_freshness_v3(
        prepared.commitments.immediate_captured_at_unix_millis,
        input_sent_at_unix_millis,
    )
    .is_err()
    {
        release_unattempted_reservation_v3()?;
        return Err("the immediate sideboard drag capture is stale or future-dated".to_owned());
    }
    let inner = prepared.prepared.commitments_v1();
    let retained_authorization = prepared
        .prepared
        .sequence
        .sideboard_authorization
        .commitments_v1();
    if inner.sequence_chain_commitment_sha256
        != prepared.commitments.sequence_chain_commitment_sha256
        || inner.plan_commitment_sha256 != prepared.commitments.plan_commitment_sha256
        || inner.transfer != prepared.commitments.transfer
        || inner.source_card_region_sha256
            != prepared.commitments.immediate_source_card_region_sha256
        || inner.destination_empty_drop_region_sha256
            != prepared
                .commitments
                .immediate_destination_drop_region_sha256
        || inner.source_frame_sequence != prepared.commitments.immediate_frame_sequence
        || retained_authorization.ratification_commitment_sha256
            != prepared
                .commitments
                .sideboard_automation_ratification_commitment_sha256
    {
        release_unattempted_reservation_v3()?;
        return Err("fresh sideboard drag preparation changed before input".to_owned());
    }
    halt_before_input_attempt_v3()?;
    let (emitted_mouse_record_count, cursor_parked_outside_client) =
        send_exactly_one_sideboard_drag_v1(&prepared.pointer_target)?;
    let input_receipt_sha256 = hash_parts_v2(
        COMPETITIVE_EVENT_SIDEBOARD_DRAG_INPUT_RECEIPT_DOMAIN_V1,
        &[
            prepared
                .commitments
                .fresh_drag_preparation_commitment_sha256
                .as_bytes(),
            prepared
                .commitments
                .sideboard_automation_ratification_commitment_sha256
                .as_bytes(),
            prepared
                .commitments
                .sequence_chain_commitment_sha256
                .as_bytes(),
            prepared.commitments.plan_commitment_sha256.as_bytes(),
            prepared
                .commitments
                .transfer
                .step_index
                .to_be_bytes()
                .as_slice(),
            prepared.commitments.transfer.card_name.as_bytes(),
            competitive_sideboard_transfer_direction_tag_v1(
                prepared.commitments.transfer.direction,
            ),
            prepared
                .commitments
                .immediate_frame_sequence
                .to_be_bytes()
                .as_slice(),
            input_sent_at_unix_millis.to_be_bytes().as_slice(),
            &[emitted_mouse_record_count],
            &[u8::from(cursor_parked_outside_client)],
            b"exactly_one_visible_zone_drag_shared_gate_pending_exact_inventory_postcondition",
        ],
    );
    set_pending_v3(&input_receipt_sha256)?;
    let commitments = MtgoCompetitiveEventSideboardDragInputReceiptCommitmentsV1 {
        fresh_drag_preparation_commitment_sha256: prepared
            .commitments
            .fresh_drag_preparation_commitment_sha256
            .clone(),
        sideboard_automation_ratification_commitment_sha256: prepared
            .commitments
            .sideboard_automation_ratification_commitment_sha256
            .clone(),
        sequence_chain_commitment_sha256: prepared
            .commitments
            .sequence_chain_commitment_sha256
            .clone(),
        transfer: prepared.commitments.transfer.clone(),
        input_receipt_sha256,
        source_frame_sequence: prepared.commitments.immediate_frame_sequence,
        input_sent_at_unix_millis,
        emitted_mouse_record_count,
        cursor_parked_outside_client,
    };
    Ok(OpaqueMtgoPendingCompetitiveEventSideboardDragV1 {
        prepared,
        commitments,
    })
}

/// Confirms exactly one semantic transfer from a strictly newer, independently
/// parsed visible frame. This does not claim that an input caused the change.
pub fn confirm_competitive_event_sideboard_transfer_visible_v1(
    prepared: OpaqueMtgoPreparedCompetitiveEventSideboardTransferV1,
    next_frame: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    classifier_runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    timeout_ms: u32,
) -> Result<MtgoCompetitiveEventSideboardTransferAdvanceV1, String> {
    let OpaqueMtgoPreparedCompetitiveEventSideboardTransferV1 {
        mut sequence,
        _source_card: _,
        _destination_zone: _,
        commitments: preparation,
    } = prepared;
    if preparation.sequence_chain_commitment_sha256
        != sequence.commitments.sequence_chain_commitment_sha256
        || preparation.plan_commitment_sha256 != sequence.commitments.plan_commitment_sha256
        || preparation.source_sideboard_snapshot_commitment_sha256
            != sequence
                .commitments
                .current_sideboard_snapshot_commitment_sha256
        || preparation.transfer != sequence.commitments.next_transfer
        || preparation.source_frame_sequence != sequence.commitments.current_frame_sequence
    {
        return Err("prepared sideboard transfer changed its exact sequence lineage".to_owned());
    }
    require_same_competitive_navigation_lineage_v1(&sequence.current_frame, &next_frame)?;
    let classified = classify_checked_untrusted_competitive_sideboard_v1(
        next_frame,
        &sequence.manifest,
        sequence.prior.policy_deployment_commitment_sha256.clone(),
        classifier_runtime,
        timeout_ms,
    )?;
    let next = classified.commitments_v1();
    let current_frame = sequence.current_frame.commitments_v1();
    if next.navigation_profile_commitment_sha256
        != sequence.prior.navigation_profile_commitment_sha256
        || next.navigation_profile_admission_commitment_sha256
            != sequence
                .prior
                .navigation_profile_admission_commitment_sha256
        || next.approved_account_alias_sha256 != sequence.prior.approved_account_alias_sha256
        || next.deck_list_sha256 != sequence.prior.deck_list_sha256
        || next.deck_format_sha256 != sequence.prior.deck_format_sha256
        || next.policy_deployment_commitment_sha256
            != sequence.prior.policy_deployment_commitment_sha256
        || next.event_kind != sequence.prior.event_kind
        || next.event_identity_sha256 != sequence.prior.bound_event_identity_sha256
        || sequence.prior.current_match_identity_sha256.as_deref()
            != Some(next.match_identity_sha256.as_str())
        || sequence.prior.current_game_number != Some(next.game_number)
        || next.frame_sequence <= sequence.commitments.current_frame_sequence
        || next.source_capture_commitment_sha256
            == current_frame
                .source_frame
                .source_capture
                .capture_commitment_sha256
        || next.sideboard_snapshot_commitment_sha256
            == sequence
                .commitments
                .current_sideboard_snapshot_commitment_sha256
    {
        return Err("next sideboard frame changed identity or did not advance".to_owned());
    }
    let expected_configuration =
        apply_atomic_sideboard_transfer_v1(&sequence.current_configuration, &preparation.transfer)?;
    if classified.configuration_v1() != &expected_configuration {
        return Err(
            "next visible sideboard configuration is not exactly one declared transfer".to_owned(),
        );
    }
    let next_step_count = sequence
        .commitments
        .confirmed_transfer_steps
        .checked_add(1)
        .ok_or("confirmed sideboard transfer count overflow")?;
    let confirmation_receipt_sha256 = hash_parts_v2(
        COMPETITIVE_EVENT_SIDEBOARD_TRANSFER_CONFIRMATION_DOMAIN_V1,
        &[
            preparation.preparation_commitment_sha256.as_bytes(),
            sequence
                .commitments
                .sequence_chain_commitment_sha256
                .as_bytes(),
            next.classification_result_commitment_sha256.as_bytes(),
            next.sideboard_snapshot_commitment_sha256.as_bytes(),
            next.frame_sequence.to_be_bytes().as_slice(),
            next_step_count.to_be_bytes().as_slice(),
            b"exactly_one_newer_visible_sideboard_transfer_no_causality_no_input",
        ],
    );
    let OpaqueMtgoClassifiedCompetitiveSideboardV1 {
        source_frame,
        sideboard,
        visible_cards,
        mainboard_zone,
        sideboard_zone,
        commitments: _,
    } = classified;
    if next_step_count < sequence.commitments.total_transfer_steps {
        let next_transfer = sequence.atomic_transfers[usize::from(next_step_count)].clone();
        let sequence_chain_commitment_sha256 = hash_parts_v2(
            COMPETITIVE_EVENT_SIDEBOARD_SEQUENCE_DOMAIN_V1,
            &[
                sequence
                    .commitments
                    .sequence_chain_commitment_sha256
                    .as_bytes(),
                confirmation_receipt_sha256.as_bytes(),
                next.sideboard_snapshot_commitment_sha256.as_bytes(),
                next.frame_sequence.to_be_bytes().as_slice(),
                next_step_count.to_be_bytes().as_slice(),
                b"awaiting_next_exact_visible_sideboard_transfer_no_input",
            ],
        );
        sequence.current_frame = source_frame;
        sequence.current_configuration = expected_configuration;
        sequence.current_visible_cards = visible_cards;
        sequence.current_mainboard_zone = mainboard_zone;
        sequence.current_sideboard_zone = sideboard_zone;
        sequence.commitments.sequence_chain_commitment_sha256 = sequence_chain_commitment_sha256;
        sequence
            .commitments
            .current_sideboard_snapshot_commitment_sha256 =
            next.sideboard_snapshot_commitment_sha256;
        sequence.commitments.current_frame_sequence = next.frame_sequence;
        sequence.commitments.confirmed_transfer_steps = next_step_count;
        sequence.commitments.next_transfer = next_transfer;
        return Ok(
            MtgoCompetitiveEventSideboardTransferAdvanceV1::AwaitingNext(Box::new(sequence)),
        );
    }
    if next_step_count != sequence.commitments.total_transfer_steps
        || expected_configuration != *sequence.plan.target_configuration_v1()
    {
        return Err("sideboard sequence reached an invalid terminal transfer count".to_owned());
    }
    let visible_target_configuration =
        visible_native_sideboard_configuration_v1(&expected_configuration)?;
    sequence
        .player_known_deck_state
        .replace_current_v1(visible_target_configuration)?;
    effective_current_deck_commitment_v1(&mut sequence.prior, &sequence.player_known_deck_state)?;
    let ready = confirm_competitive_sideboard_target_visible_v1(sequence.plan, sideboard)
        .map_err(|error| format!("confirm final visible sideboard target: {error}"))?;
    if ready.after_frame_id() != next.frame_id
        || ready.after_frame_sequence() != next.frame_sequence
        || ready.after_snapshot_commitment_sha256() != next.sideboard_snapshot_commitment_sha256
        || ready.event_identity_sha256() != sequence.prior.bound_event_identity_sha256
        || sequence.prior.current_match_identity_sha256.as_deref()
            != Some(ready.match_identity_sha256())
        || ready.policy_deployment_commitment_sha256()
            != sequence.prior.policy_deployment_commitment_sha256
    {
        return Err("final sideboard ready state changed the event lineage".to_owned());
    }
    let mut effective_prior = sequence.prior;
    effective_prior.current_lifecycle_snapshot_commitment_sha256 =
        next.source_lifecycle_snapshot_commitment_sha256;
    effective_prior.current_frame_id = next.frame_id;
    effective_prior.current_frame_sequence = next.frame_sequence;
    effective_prior.runtime_commitment_sha256 = competitive_event_runtime_commitment_v1(
        COMPETITIVE_EVENT_SIDEBOARD_READY_DOMAIN_V1,
        Some(effective_prior.runtime_commitment_sha256.as_str()),
        &effective_prior,
        confirmation_receipt_sha256.as_bytes(),
    );
    let event_ready_binding_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_EVENT_SIDEBOARD_READY_DOMAIN_V1,
        &[
            sequence
                .commitments
                .event_plan_binding_commitment_sha256
                .as_bytes(),
            sequence.commitments.plan_commitment_sha256.as_bytes(),
            ready.ready_commitment_sha256().as_bytes(),
            confirmation_receipt_sha256.as_bytes(),
            effective_prior.runtime_commitment_sha256.as_bytes(),
            next.frame_sequence.to_be_bytes().as_slice(),
            next_step_count.to_be_bytes().as_slice(),
            b"all_model_selected_sideboard_transfers_visibly_confirmed_no_submit_no_input",
        ],
    );
    let commitments = MtgoReadyCompetitiveEventSideboardCommitmentsV1 {
        prior_event_runtime_commitment_sha256: sequence
            .commitments
            .prior_event_runtime_commitment_sha256,
        event_plan_binding_commitment_sha256: sequence
            .commitments
            .event_plan_binding_commitment_sha256,
        plan_commitment_sha256: sequence.commitments.plan_commitment_sha256,
        ready_commitment_sha256: ready.ready_commitment_sha256().to_owned(),
        event_ready_binding_commitment_sha256,
        final_sideboard_snapshot_commitment_sha256: next.sideboard_snapshot_commitment_sha256,
        final_frame_id: next.frame_id,
        final_frame_sequence: next.frame_sequence,
        confirmed_transfer_steps: next_step_count,
    };
    Ok(
        MtgoCompetitiveEventSideboardTransferAdvanceV1::ReadyToSubmit(Box::new(
            OpaqueMtgoReadyCompetitiveEventSideboardV1 {
                _spent_entry_authorization: sequence._spent_entry_authorization,
                lifecycle_authorization: sequence.lifecycle_authorization,
                sideboard_authorization: sequence.sideboard_authorization,
                current_frame: source_frame,
                manifest: sequence.manifest,
                _ready: ready,
                player_known_deck_state: sequence.player_known_deck_state,
                event_monitor: sequence.event_monitor,
                effective_prior,
                commitments,
            },
        )),
    )
}

pub fn confirm_pending_competitive_event_sideboard_drag_v1(
    pending: OpaqueMtgoPendingCompetitiveEventSideboardDragV1,
    next_frame: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    classifier_runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoConfirmedCompetitiveEventSideboardDragV1, String> {
    require_matching_pending_v3(&pending.commitments.input_receipt_sha256)?;
    let after = next_frame.commitments_v1();
    if after.frame_sequence <= pending.commitments.source_frame_sequence
        || after.source_frame.source_capture.captured_at_unix_millis
            <= pending.commitments.input_sent_at_unix_millis
    {
        halt_gate_v3()?;
        return Err(
            "sideboard drag postcondition is not a strictly newer post-input frame; input gate halted"
                .to_owned(),
        );
    }
    let OpaqueMtgoPendingCompetitiveEventSideboardDragV1 {
        prepared,
        commitments: input,
    } = pending;
    let fresh = prepared.commitments.clone();
    let advance = match confirm_competitive_event_sideboard_transfer_visible_v1(
        prepared.prepared,
        next_frame,
        classifier_runtime,
        timeout_ms,
    ) {
        Ok(value) => value,
        Err(error) => {
            halt_gate_v3()?;
            return Err(format!(
                "sideboard drag visible postcondition failed and the input gate is halted: {error}"
            ));
        }
    };
    let (resulting_sequence_or_ready_commitment_sha256, after_frame_sequence) = match &advance {
        MtgoCompetitiveEventSideboardTransferAdvanceV1::AwaitingNext(sequence) => (
            sequence
                .commitments
                .sequence_chain_commitment_sha256
                .clone(),
            sequence.commitments.current_frame_sequence,
        ),
        MtgoCompetitiveEventSideboardTransferAdvanceV1::ReadyToSubmit(ready) => (
            ready
                .commitments
                .event_ready_binding_commitment_sha256
                .clone(),
            ready.commitments.final_frame_sequence,
        ),
    };
    if input.fresh_drag_preparation_commitment_sha256
        != fresh.fresh_drag_preparation_commitment_sha256
        || input.sideboard_automation_ratification_commitment_sha256
            != fresh.sideboard_automation_ratification_commitment_sha256
        || input.sequence_chain_commitment_sha256 != fresh.sequence_chain_commitment_sha256
        || input.transfer != fresh.transfer
        || after_frame_sequence != after.frame_sequence
    {
        halt_gate_v3()?;
        return Err(
            "confirmed sideboard drag changed its exact preparation or visible result lineage; input gate halted"
                .to_owned(),
        );
    }
    let confirmation_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_EVENT_SIDEBOARD_DRAG_CONFIRMED_DOMAIN_V1,
        &[
            input.input_receipt_sha256.as_bytes(),
            fresh.fresh_drag_preparation_commitment_sha256.as_bytes(),
            fresh
                .sideboard_automation_ratification_commitment_sha256
                .as_bytes(),
            fresh.sequence_chain_commitment_sha256.as_bytes(),
            resulting_sequence_or_ready_commitment_sha256.as_bytes(),
            input.transfer.step_index.to_be_bytes().as_slice(),
            after_frame_sequence.to_be_bytes().as_slice(),
            b"one_drag_exact_newer_inventory_transition_confirmed_shared_gate_released",
        ],
    );
    release_confirmed_pending_v3(&input.input_receipt_sha256)?;
    Ok(OpaqueMtgoConfirmedCompetitiveEventSideboardDragV1 {
        advance,
        commitments: MtgoConfirmedCompetitiveEventSideboardDragCommitmentsV1 {
            input_receipt_sha256: input.input_receipt_sha256,
            fresh_drag_preparation_commitment_sha256: fresh
                .fresh_drag_preparation_commitment_sha256,
            prior_sequence_chain_commitment_sha256: fresh.sequence_chain_commitment_sha256,
            resulting_sequence_or_ready_commitment_sha256,
            transfer: input.transfer,
            after_frame_sequence,
            confirmation_commitment_sha256,
        },
    })
}

/// Converts the exact visibly confirmed changed-sideboard target into the
/// existing one-click lifecycle preparation for Submit Deck. The proof is
/// retained inside the control and revalidated again after the visible
/// transition into the next game.
pub fn prepare_ready_competitive_event_sideboard_submit_v1(
    ready_event: OpaqueMtgoReadyCompetitiveEventSideboardV1,
) -> Result<OpaqueMtgoPreparedCompetitiveEventLifecycleControlV1, String> {
    let OpaqueMtgoReadyCompetitiveEventSideboardV1 {
        _spent_entry_authorization,
        lifecycle_authorization,
        sideboard_authorization,
        current_frame,
        manifest,
        _ready,
        player_known_deck_state,
        event_monitor,
        effective_prior: prior,
        commitments: ready_commitments,
    } = ready_event;
    let frame = current_frame.commitments_v1();
    let sideboard_authorization_commitments = sideboard_authorization.commitments_v1();
    if sideboard_authorization_commitments.lifecycle_authorization_commitment_sha256
        != lifecycle_authorization
            .commitments
            .ratification_commitment_sha256
        || sideboard_authorization_commitments.permission_review_commitment_sha256
            != prior.permission_review_commitment_sha256
        || sideboard_authorization_commitments.mode_authorization_commitment_sha256
            != prior.mode_authorization_commitment_sha256
        || sideboard_authorization_commitments.approved_account_alias_sha256
            != prior.approved_account_alias_sha256
        || sideboard_authorization_commitments.navigation_profile_commitment_sha256
            != prior.navigation_profile_commitment_sha256
        || sideboard_authorization_commitments.navigation_profile_admission_commitment_sha256
            != prior.navigation_profile_admission_commitment_sha256
        || sideboard_authorization_commitments.deck_list_sha256 != prior.deck_list_sha256
        || sideboard_authorization_commitments.deck_manifest_commitment_sha256
            != manifest.manifest_commitment_sha256()
        || sideboard_authorization_commitments.deck_format_sha256 != prior.deck_format_sha256
        || sideboard_authorization_commitments.policy_deployment_commitment_sha256
            != prior.policy_deployment_commitment_sha256
        || sideboard_authorization_commitments.automation_scope_commitment_sha256
            != competitive_sideboard_automation_scope_v1()
        || sideboard_authorization_commitments.event_kind != prior.event_kind
        || prior.current_phase != MtgoCompetitiveLifecyclePhaseV1::Sideboarding
        || prior.current_lifecycle_snapshot_commitment_sha256
            != frame.lifecycle_snapshot_commitment_sha256
        || prior.current_frame_id != ready_commitments.final_frame_id
        || prior.current_frame_sequence != ready_commitments.final_frame_sequence
        || frame.frame_id != ready_commitments.final_frame_id
        || frame.frame_sequence != ready_commitments.final_frame_sequence
        || _ready.ready_commitment_sha256() != ready_commitments.ready_commitment_sha256
        || _ready.after_snapshot_commitment_sha256()
            != ready_commitments.final_sideboard_snapshot_commitment_sha256
        || _ready.event_kind() != prior.event_kind
        || _ready.event_identity_sha256() != prior.bound_event_identity_sha256
        || prior.current_match_identity_sha256.as_deref() != Some(_ready.match_identity_sha256())
        || prior.current_game_number != Some(_ready.game_number())
        || manifest.deck_list_sha256() != prior.deck_list_sha256
        || manifest.format_sha256() != prior.deck_format_sha256
        || _ready.deck_manifest_commitment_sha256() != manifest.manifest_commitment_sha256()
        || _ready.policy_deployment_commitment_sha256() != prior.policy_deployment_commitment_sha256
    {
        return Err(
            "changed-sideboard Submit Deck proof differs from the exact event runtime".to_owned(),
        );
    }
    let control =
        crate::probe::bind_classified_navigation_frame_to_confirmed_sideboard_submit_control_v1(
            current_frame,
            _ready,
            &lifecycle_authorization.scope,
        )?;
    let prepared =
        prepare_ratified_competitive_lifecycle_control_v1(lifecycle_authorization, control)?;
    let lifecycle = prepared.commitments_v1();
    if lifecycle.event_kind != prior.event_kind
        || lifecycle.action != MtgoCompetitiveLifecycleActionV1::SubmitSideboard
        || lifecycle.event_identity_sha256 != prior.bound_event_identity_sha256
        || lifecycle.match_identity_sha256 != prior.current_match_identity_sha256
        || lifecycle.game_number != prior.current_game_number
        || lifecycle.source_frame_sequence != prior.current_frame_sequence
    {
        return Err(
            "prepared changed-sideboard Submit Deck control changed the exact event lineage"
                .to_owned(),
        );
    }
    let commitments = MtgoPreparedCompetitiveEventLifecycleControlCommitmentsV1 {
        prior_event_runtime_commitment_sha256: prior.runtime_commitment_sha256.clone(),
        lifecycle_preparation_commitment_sha256: lifecycle.preparation_commitment_sha256,
        event_kind: lifecycle.event_kind,
        action: lifecycle.action,
        source_frame_sequence: lifecycle.source_frame_sequence,
    };
    Ok(OpaqueMtgoPreparedCompetitiveEventLifecycleControlV1 {
        _spent_entry_authorization,
        prepared,
        player_known_deck_state,
        event_monitor,
        prior,
        commitments,
    })
}

/// Consumes the event runtime and prepares one exact enabled lifecycle control
/// from its current retained frame. Entry actions are impossible here. Closing
/// an event also requires a terminal event-record monitor already attached.
pub fn prepare_competitive_event_runtime_lifecycle_control_v1(
    runtime: OpaqueMtgoCompetitiveEventRuntimeV1,
    action: MtgoCompetitiveLifecycleActionV1,
) -> Result<OpaqueMtgoPreparedCompetitiveEventLifecycleControlV1, String> {
    if runtime.commitments.closed_to_event_browser {
        return Err("a closed competitive event runtime cannot prepare another control".to_owned());
    }
    if !is_non_entry_lifecycle_action_v1(action) {
        return Err("competitive event runtime excludes all event-entry actions".to_owned());
    }
    if action == MtgoCompetitiveLifecycleActionV1::SubmitSideboard {
        return Err(
            "competitive event runtime cannot submit an unchanged sideboard without an opaque native model decision"
                .to_owned(),
        );
    }
    if action == MtgoCompetitiveLifecycleActionV1::CloseCompletedEvent
        && !runtime.commitments.terminal_event_record_confirmed
    {
        return Err(
            "closing a competitive event requires its terminal visible event record".to_owned(),
        );
    }
    let OpaqueMtgoCompetitiveEventRuntimeV1 {
        _spent_entry_authorization,
        lifecycle_authorization,
        current_frame,
        player_known_deck_state,
        event_monitor,
        commitments: prior,
    } = runtime;
    let control = crate::probe::bind_classified_navigation_frame_to_lifecycle_control_v1(
        current_frame,
        action,
    )?;
    let prepared =
        prepare_ratified_competitive_lifecycle_control_v1(lifecycle_authorization, control)?;
    let lifecycle = prepared.commitments_v1();
    if lifecycle.event_kind != prior.event_kind
        || lifecycle.action != action
        || lifecycle.event_identity_sha256 != prior.bound_event_identity_sha256
        || lifecycle.source_frame_sequence != prior.current_frame_sequence
    {
        return Err(
            "prepared lifecycle control differs from the exact event runtime state".to_owned(),
        );
    }
    let commitments = MtgoPreparedCompetitiveEventLifecycleControlCommitmentsV1 {
        prior_event_runtime_commitment_sha256: prior.runtime_commitment_sha256.clone(),
        lifecycle_preparation_commitment_sha256: lifecycle.preparation_commitment_sha256,
        event_kind: lifecycle.event_kind,
        action,
        source_frame_sequence: lifecycle.source_frame_sequence,
    };
    Ok(OpaqueMtgoPreparedCompetitiveEventLifecycleControlV1 {
        _spent_entry_authorization,
        prepared,
        player_known_deck_state,
        event_monitor,
        prior,
        commitments,
    })
}

pub fn execute_prepared_competitive_event_lifecycle_control_v1(
    prepared: OpaqueMtgoPreparedCompetitiveEventLifecycleControlV1,
) -> Result<OpaqueMtgoPendingCompetitiveEventLifecycleControlV1, String> {
    let OpaqueMtgoPreparedCompetitiveEventLifecycleControlV1 {
        _spent_entry_authorization,
        prepared,
        player_known_deck_state,
        event_monitor,
        prior,
        commitments: prepared_commitments,
    } = prepared;
    let pending = execute_prepared_competitive_lifecycle_control_v1(prepared)?;
    let input = pending.commitments_v1();
    if input.preparation_commitment_sha256
        != prepared_commitments.lifecycle_preparation_commitment_sha256
        || input.event_kind != prepared_commitments.event_kind
        || input.action != prepared_commitments.action
    {
        halt_gate_v3()?;
        return Err(
            "event lifecycle input receipt differs from its exact preparation and halted the gate"
                .to_owned(),
        );
    }
    let commitments = MtgoPendingCompetitiveEventLifecycleControlCommitmentsV1 {
        prior_event_runtime_commitment_sha256: prior.runtime_commitment_sha256.clone(),
        lifecycle_input_receipt_sha256: input.input_receipt_sha256,
        event_kind: input.event_kind,
        action: input.action,
        input_sent_at_unix_millis: input.input_sent_at_unix_millis,
    };
    Ok(OpaqueMtgoPendingCompetitiveEventLifecycleControlV1 {
        _spent_entry_authorization,
        pending,
        player_known_deck_state,
        event_monitor,
        prior,
        commitments,
    })
}

pub fn confirm_pending_competitive_event_lifecycle_control_v1(
    pending: OpaqueMtgoPendingCompetitiveEventLifecycleControlV1,
    after: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
) -> Result<OpaqueMtgoCompetitiveEventRuntimeV1, String> {
    let OpaqueMtgoPendingCompetitiveEventLifecycleControlV1 {
        _spent_entry_authorization,
        pending,
        mut player_known_deck_state,
        event_monitor,
        mut prior,
        commitments: event_input,
    } = pending;
    let confirmed = confirm_pending_competitive_lifecycle_control_v1(pending, after)?;
    let OpaqueMtgoConfirmedCompetitiveLifecycleControlV1 {
        _authorization: lifecycle_authorization,
        _visible_postcondition: visible_postcondition,
        commitments: confirmed,
    } = confirmed;
    let visible = visible_postcondition.commitments_v1();
    let current_frame = visible_postcondition.into_after_frame_v1();
    let current = current_frame.commitments_v1();
    if event_input.prior_event_runtime_commitment_sha256 != prior.runtime_commitment_sha256
        || event_input.lifecycle_input_receipt_sha256 != confirmed.input_receipt_sha256
        || confirmed.event_kind != prior.event_kind
        || confirmed.action != event_input.action
        || visible.before_lifecycle_snapshot_commitment_sha256
            != prior.current_lifecycle_snapshot_commitment_sha256
        || visible.after_lifecycle_snapshot_commitment_sha256
            != current.lifecycle_snapshot_commitment_sha256
        || visible.event_identity_sha256 != prior.bound_event_identity_sha256
    {
        halt_gate_v3()?;
        return Err(
            "confirmed event lifecycle control changed the runtime lineage and halted the gate"
                .to_owned(),
        );
    }
    if confirmed.action == MtgoCompetitiveLifecycleActionV1::ContinueAfterMatch {
        player_known_deck_state.reset_for_next_match_v1();
        effective_current_deck_commitment_v1(&mut prior, &player_known_deck_state)?;
    }
    let commitments = advance_competitive_event_runtime_commitments_v1(
        &prior,
        &current_frame,
        Some((
            confirmed.action,
            confirmed.confirmation_receipt_sha256.as_str(),
        )),
        None,
        event_monitor.as_ref().map(|value| value.commitments_v1()),
    )?;
    Ok(OpaqueMtgoCompetitiveEventRuntimeV1 {
        _spent_entry_authorization,
        lifecycle_authorization,
        current_frame,
        player_known_deck_state,
        event_monitor,
        commitments,
    })
}

/// Advances the coordinator for one exact server- or client-observed state
/// change without emitting input. Both frames came through the same admitted
/// classifier runtime, and the black-box lifecycle state machine revalidates
/// the transition over the already checked snapshots.
pub fn advance_competitive_event_runtime_observed_v1(
    runtime: OpaqueMtgoCompetitiveEventRuntimeV1,
    observed: MtgoObservedCompetitiveLifecycleAdvanceV1,
    next: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
) -> Result<OpaqueMtgoCompetitiveEventRuntimeV1, String> {
    if runtime.commitments.closed_to_event_browser {
        return Err("a closed competitive event runtime cannot advance".to_owned());
    }
    require_same_competitive_navigation_lineage_v1(&runtime.current_frame, &next)?;
    let transition = validate_checked_observed_competitive_lifecycle_advance_v1(
        runtime.current_frame.lifecycle_snapshot_v1(),
        observed,
        next.lifecycle_snapshot_v1(),
    )
    .map_err(|error| format!("validate observed competitive lifecycle advance: {error}"))?;
    if transition.source_snapshot_commitment_sha256()
        != runtime
            .commitments
            .current_lifecycle_snapshot_commitment_sha256
        || transition.next_snapshot_commitment_sha256()
            != next.commitments_v1().lifecycle_snapshot_commitment_sha256
    {
        return Err(
            "observed lifecycle transition changed the runtime snapshot lineage".to_owned(),
        );
    }
    let observed_receipt_sha256 = hash_parts_v2(
        COMPETITIVE_EVENT_RUNTIME_ADVANCE_DOMAIN_V1,
        &[
            runtime.commitments.runtime_commitment_sha256.as_bytes(),
            transition.source_snapshot_commitment_sha256().as_bytes(),
            transition.next_snapshot_commitment_sha256().as_bytes(),
            observed_lifecycle_advance_tag_v1(observed),
            b"same_classifier_lineage_observation_only",
        ],
    );
    let commitments = advance_competitive_event_runtime_commitments_v1(
        &runtime.commitments,
        &next,
        None,
        Some((observed, observed_receipt_sha256.as_str())),
        runtime
            .event_monitor
            .as_ref()
            .map(|value| value.commitments_v1()),
    )?;
    Ok(OpaqueMtgoCompetitiveEventRuntimeV1 {
        _spent_entry_authorization: runtime._spent_entry_authorization,
        lifecycle_authorization: runtime.lifecycle_authorization,
        current_frame: next,
        player_known_deck_state: runtime.player_known_deck_state,
        event_monitor: runtime.event_monitor,
        commitments,
    })
}

pub fn attach_competitive_event_monitor_to_runtime_v1(
    mut runtime: OpaqueMtgoCompetitiveEventRuntimeV1,
    monitor: OpaqueMtgoCompetitiveEventMonitorV1,
) -> Result<OpaqueMtgoCompetitiveEventRuntimeV1, String> {
    if runtime.event_monitor.is_some() {
        return Err("competitive event runtime already owns an event monitor".to_owned());
    }
    let monitor_commitments = monitor.commitments_v1();
    validate_event_monitor_against_runtime_v1(&runtime, &monitor_commitments)?;
    runtime.event_monitor = Some(monitor);
    apply_event_monitor_to_runtime_commitments_v1(
        &mut runtime.commitments,
        &monitor_commitments,
        b"attached_exact_event_monitor",
    );
    Ok(runtime)
}

pub fn advance_competitive_event_monitor_in_runtime_v1(
    mut runtime: OpaqueMtgoCompetitiveEventRuntimeV1,
    next: crate::probe::OpaqueMtgoClassifiedCompetitiveEventRecordV1,
) -> Result<OpaqueMtgoCompetitiveEventRuntimeV1, String> {
    let monitor = runtime
        .event_monitor
        .take()
        .ok_or("competitive event runtime has no attached event monitor")?;
    let monitor = advance_evaluated_competitive_event_monitor_v1(monitor, next)?;
    let monitor_commitments = monitor.commitments_v1();
    validate_event_monitor_against_runtime_v1(&runtime, &monitor_commitments)?;
    runtime.event_monitor = Some(monitor);
    apply_event_monitor_to_runtime_commitments_v1(
        &mut runtime.commitments,
        &monitor_commitments,
        b"advanced_exact_event_monitor",
    );
    Ok(runtime)
}

/// Consumes one current main-client event runtime and binds it to a strictly
/// newer duel-window perception whose exact classifier response already
/// contains the same-frame lifecycle interpretation. Event, match, game,
/// paid-entry, and account lineage remain exact across the intentional
/// cross-window handoff. The returned bridge is still non-authorizing and
/// exposes no input primitive.
pub fn bind_competitive_event_runtime_to_match_launch_identity_v1(
    runtime: OpaqueMtgoCompetitiveEventRuntimeV1,
    perception: &OpaqueMtgoAdmittedDuelPerceptionV1,
    lifecycle_profile: &AdmittedMtgoCompetitiveDuelLifecycleProfileV1,
    event_display_label: String,
    event_label_rect_client_px: mtgo_blackbox_v1::MtgoRectPxV1,
) -> Result<OpaqueMtgoCompetitiveEventMatchLaunchBindingV1, String> {
    let visible_identity = bind_opaque_duel_perception_to_competitive_launch_identity_v1(
        perception,
        lifecycle_profile,
        event_display_label,
        event_label_rect_client_px,
        runtime.commitments.entry_authorization_sha256.clone(),
    )?;
    let current_process_continuity_commitment_sha256 = runtime
        .current_frame
        .process_continuity_commitment_sha256_v1();
    let current_frame = runtime.current_frame.commitments_v1();
    let source = visible_identity.commitments_v1();
    let commitments = competitive_event_match_launch_binding_commitments_v1(
        &runtime.commitments,
        &current_process_continuity_commitment_sha256,
        current_frame
            .source_frame
            .source_capture
            .captured_at_unix_millis,
        &source,
        visible_identity.event_identity_sha256_v1(),
        visible_identity.match_identity_sha256_v1(),
        visible_identity.entry_authorization_sha256_v1(),
    )?;
    Ok(OpaqueMtgoCompetitiveEventMatchLaunchBindingV1 {
        runtime,
        visible_identity,
        commitments,
    })
}

/// Withholds one exact paid-event runtime and its attended exact-game launch
/// while the visible pregame is resolved. The observation has no public
/// production constructor until a competitive pregame corpus, profile, and
/// classifier evaluation are separately admitted.
pub fn checkout_competitive_event_pregame_session_v1(
    runtime: OpaqueMtgoCompetitiveEventRuntimeV1,
    match_launch: RatifiedMtgoCompetitiveMatchLaunchV1,
    initial_observation: OpaqueMtgoCompetitivePregameObservationV1,
) -> Result<OpaqueMtgoCompetitiveEventPregameSessionV1, String> {
    let runtime_process_continuity_commitment_sha256 = runtime
        .current_frame
        .process_continuity_commitment_sha256_v1();
    let runtime_window_continuity_commitment_sha256 = runtime
        .current_frame
        .window_continuity_commitment_sha256_v1()?;
    let runtime_captured_at_unix_millis = runtime
        .current_frame
        .commitments_v1()
        .source_frame
        .source_capture
        .captured_at_unix_millis;
    let commitments = competitive_event_pregame_session_commitments_from_parts_v1(
        &runtime.commitments,
        &runtime_process_continuity_commitment_sha256,
        &runtime_window_continuity_commitment_sha256,
        runtime_captured_at_unix_millis,
        &match_launch,
        &initial_observation.commitments,
    )?;
    Ok(OpaqueMtgoCompetitiveEventPregameSessionV1 {
        runtime,
        match_launch,
        current_observation: initial_observation,
        ordered_confirmed_bottom_slots: Vec::new(),
        current_model_context: None,
        player_visible_public_context: None,
        commitments,
    })
}

/// Converts one exact immediate classifier-backed duel capture into the
/// initial pregame observation for the current paid League or Challenge game.
/// The mode-independent classifier cannot choose the event scope. That scope
/// comes only from the already-entered event runtime and its exact attended
/// match launch.
pub fn checkout_competitive_event_pregame_session_from_classified_frame_v2(
    runtime: OpaqueMtgoCompetitiveEventRuntimeV1,
    match_launch: RatifiedMtgoCompetitiveMatchLaunchV1,
    classified_frame: OpaqueMtgoClassifiedCompetitivePregameFrameV1,
) -> Result<OpaqueMtgoCompetitiveEventPregameSessionV1, String> {
    let runtime_frame = runtime.current_frame.commitments_v1();
    if runtime.commitments.current_frame_id != runtime_frame.frame_id
        || runtime.commitments.current_frame_sequence != runtime_frame.frame_sequence
        || runtime.commitments.current_phase != MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
    {
        return Err(
            "competitive pregame source requires the exact current match-in-progress frame"
                .to_owned(),
        );
    }
    classified_frame.require_immediate_successor_v1(
        runtime_frame.frame_id,
        runtime_frame.frame_sequence,
        &runtime_frame
            .source_frame
            .source_capture
            .capture_commitment_sha256,
        runtime_frame
            .source_frame
            .source_capture
            .captured_at_unix_millis,
    )?;
    let process_continuity_commitment_sha256 =
        classified_frame.process_continuity_commitment_sha256_v1();
    let window_continuity_commitment_sha256 =
        classified_frame.window_continuity_commitment_sha256_v1()?;
    if process_continuity_commitment_sha256
        != runtime
            .current_frame
            .process_continuity_commitment_sha256_v1()
        || window_continuity_commitment_sha256
            != runtime
                .current_frame
                .window_continuity_commitment_sha256_v1()?
    {
        return Err(
            "competitive pregame classifier frame changed the event process or window".to_owned(),
        );
    }

    let classified = classified_frame.commitments_v1();
    let observation_commitments = competitive_pregame_observation_from_classified_view_v2(
        &runtime.commitments,
        CompetitivePregameClassifiedViewV2::from_commitments_v2(
            &classified,
            &process_continuity_commitment_sha256,
            &window_continuity_commitment_sha256,
        ),
    )?;
    let observation = OpaqueMtgoCompetitivePregameObservationV1 {
        _classified_source: Some(OpaqueMtgoCompetitivePregameClassifiedSourceV1::Frame(
            Box::new(classified_frame),
        )),
        commitments: observation_commitments,
    };
    checkout_competitive_event_pregame_session_v1(runtime, match_launch, observation)
}

/// Converts the same-frame visible hand, controls, play-or-draw fact, and
/// public match score into one exact checkpoint-facing request while retaining
/// the paid-event session. This is the adapter half of the native pregame
/// bridge. It cannot score, select, prepare input, enter an event, or spend.
pub fn bind_competitive_event_pregame_native_request_v1(
    runtime: OpaqueMtgoCompetitiveEventRuntimeV1,
    match_launch: RatifiedMtgoCompetitiveMatchLaunchV1,
    context: OpaqueMtgoClassifiedCompetitivePregameModelContextV1,
    deck_manifest: &ValidatedMtgoCompetitiveDeckManifestV1,
) -> Result<OpaqueMtgoCompetitiveNativePregameRequestV1, String> {
    validate_player_known_deck_state_against_runtime_v1(
        &runtime.commitments,
        &runtime.player_known_deck_state,
    )?;
    let runtime_frame = runtime.current_frame.commitments_v1();
    if runtime.commitments.current_frame_id != runtime_frame.frame_id
        || runtime.commitments.current_frame_sequence != runtime_frame.frame_sequence
        || runtime.commitments.current_phase != MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
        || deck_manifest.manifest_commitment_sha256() != runtime.commitments.deck_manifest_sha256
        || deck_manifest.deck_list_sha256() != runtime.commitments.deck_list_sha256
        || deck_manifest.format_sha256() != runtime.commitments.deck_format_sha256
    {
        return Err(
            "native pregame request requires the exact current match-in-progress frame".to_owned(),
        );
    }
    let context_commitments = context.commitments_v1();
    context.require_immediate_successor_v1(
        runtime_frame.frame_id,
        runtime_frame.frame_sequence,
        &runtime_frame
            .source_frame
            .source_capture
            .capture_commitment_sha256,
        runtime_frame
            .source_frame
            .source_capture
            .captured_at_unix_millis,
    )?;
    let process_continuity_commitment_sha256 = context.process_continuity_commitment_sha256_v1();
    let window_continuity_commitment_sha256 = context.window_continuity_commitment_sha256_v1()?;
    if process_continuity_commitment_sha256
        != runtime
            .current_frame
            .process_continuity_commitment_sha256_v1()
        || window_continuity_commitment_sha256
            != runtime
                .current_frame
                .window_continuity_commitment_sha256_v1()?
        || context_commitments.game_number != runtime.commitments.current_game_number.unwrap_or(0)
    {
        return Err(
            "native pregame context changed the exact event process, window, or game".to_owned(),
        );
    }

    let observation_commitments = competitive_pregame_observation_from_classified_view_v2(
        &runtime.commitments,
        CompetitivePregameClassifiedViewV2::from_commitments_v2(
            &context_commitments.source,
            &process_continuity_commitment_sha256,
            &window_continuity_commitment_sha256,
        ),
    )?;
    let response = context.response_v1();
    let model_input = competitive_native_pregame_model_input_from_parts_v1(
        &runtime.commitments,
        &context_commitments,
        response,
        &runtime.player_known_deck_state.current,
        &[],
    )?;
    let (classified_frame, public_context_witness) =
        context.into_classified_frame_and_public_context_witness_v1();
    let observation = OpaqueMtgoCompetitivePregameObservationV1 {
        _classified_source: Some(OpaqueMtgoCompetitivePregameClassifiedSourceV1::Frame(
            Box::new(classified_frame),
        )),
        commitments: observation_commitments,
    };
    let mut session =
        checkout_competitive_event_pregame_session_v1(runtime, match_launch, observation)?;
    session
        .commitments
        .current_model_context_binding_commitment_sha256 = Some(
        context_commitments
            .model_context_binding_commitment_sha256
            .clone(),
    );
    session.current_model_context = Some(public_context_witness);
    let player_visible_public_context =
        MtgoCompetitivePregamePlayerVisiblePublicContextStateV1::from(&context_commitments);
    session
        .commitments
        .player_visible_public_context_commitment_sha256 = Some(
        competitive_pregame_player_visible_public_context_state_commitment_v1(
            player_visible_public_context,
        ),
    );
    session.player_visible_public_context = Some(player_visible_public_context);
    session.commitments.session_commitment_sha256 =
        competitive_event_pregame_session_commitment_v1(
            COMPETITIVE_EVENT_PREGAME_SESSION_DOMAIN_V1,
            None,
            &session.commitments,
        )?;
    validate_competitive_event_pregame_session_integrity_v1(&session)?;
    if session.commitments.game_number != model_input.game_number
        || session.commitments.game_number != context_commitments.game_number
    {
        return Err("native pregame request changed the exact event-session lineage".to_owned());
    }
    finish_competitive_event_pregame_native_request_v1(session, model_input, &context_commitments)
}

/// Rebuilds the next visible-only model request from an advanced pregame
/// session. The session must retain the exact same-frame public context and
/// any ordered bottom slots confirmed by prior visible postconditions.
pub fn bind_competitive_event_pregame_native_request_from_session_v1(
    session: OpaqueMtgoCompetitiveEventPregameSessionV1,
) -> Result<OpaqueMtgoCompetitiveNativePregameRequestV1, String> {
    validate_competitive_event_pregame_session_integrity_v1(&session)?;
    let context_commitments = session
        .current_model_context
        .as_ref()
        .ok_or("native pregame session has no same-frame public model context")?
        .commitments_v1()
        .clone();
    let response = session
        .current_observation
        ._classified_source
        .as_ref()
        .ok_or("native pregame session has no retained visible classification")?
        .response_v1();
    let model_input = competitive_native_pregame_model_input_from_parts_v1(
        &session.runtime.commitments,
        &context_commitments,
        response,
        &session.runtime.player_known_deck_state.current,
        &session.ordered_confirmed_bottom_slots,
    )?;
    finish_competitive_event_pregame_native_request_v1(session, model_input, &context_commitments)
}

fn finish_competitive_event_pregame_native_request_v1(
    session: OpaqueMtgoCompetitiveEventPregameSessionV1,
    model_input: MtgoCompetitiveNativePregameModelInputV1,
    context_commitments: &crate::probe::MtgoClassifiedCompetitivePregameModelContextCommitmentsV1,
) -> Result<OpaqueMtgoCompetitiveNativePregameRequestV1, String> {
    validate_competitive_event_pregame_session_integrity_v1(&session)?;
    validate_competitive_native_pregame_model_input_v1(&model_input)?;
    let model_input_commitment_sha256 =
        competitive_native_pregame_model_input_commitment_v1(&model_input)?;
    let request_binding_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_NATIVE_PREGAME_REQUEST_BINDING_DOMAIN_V1,
        &[
            model_input_commitment_sha256.as_bytes(),
            session.commitments.session_commitment_sha256.as_bytes(),
            session
                .commitments
                .event_runtime_commitment_sha256
                .as_bytes(),
            session.commitments.deck_manifest_sha256.as_bytes(),
            session
                .commitments
                .policy_deployment_commitment_sha256
                .as_bytes(),
            context_commitments
                .source
                .classification_commitment_sha256
                .as_bytes(),
            context_commitments
                .public_context_commitment_sha256
                .as_bytes(),
            context_commitments
                .model_context_binding_commitment_sha256
                .as_bytes(),
            b"opaque_adapter_lineage_not_model_game_information_no_selection_no_input",
        ],
    );
    Ok(OpaqueMtgoCompetitiveNativePregameRequestV1 {
        _session: session,
        model_input,
        model_input_commitment_sha256,
        _request_binding_commitment_sha256: request_binding_commitment_sha256,
    })
}

fn competitive_native_pregame_model_input_from_parts_v1(
    runtime: &MtgoCompetitiveEventRuntimeCommitmentsV1,
    context: &crate::probe::MtgoClassifiedCompetitivePregameModelContextCommitmentsV1,
    response: &mtgo_blackbox_v1::MtgoCompetitivePregameClassifierResponseV1,
    player_known_deck_configuration: &crate::MtgoCompetitiveNativeSideboardConfigurationV1,
    ordered_confirmed_bottom_slots: &[u8],
) -> Result<MtgoCompetitiveNativePregameModelInputV1, String> {
    runtime
        .current_match_identity_sha256
        .as_deref()
        .ok_or("native pregame request requires one exact current match")?;
    let game_number = runtime
        .current_game_number
        .ok_or("native pregame request requires one exact current game")?;
    if competitive_native_sideboard_configuration_commitment_v1(player_known_deck_configuration)?
        != runtime.player_known_current_deck_configuration_commitment_sha256
    {
        return Err(
            "native pregame request changed the exact player-known current deck configuration"
                .to_owned(),
        );
    }
    if context.game_number != game_number
        || context.source.stage != response.stage
        || context.source.visible_interaction_commitment_sha256
            != response.visible_interaction_commitment_sha256
        || response.visible_cards.len() != 7
        || matches!(
            response.stage,
            MtgoCompetitivePregameStageLabelV1::GameplayReady
        )
    {
        return Err(
            "native pregame request context, interaction, stage, or hand is incomplete".to_owned(),
        );
    }

    let mut selected_slots = [false; 7];
    for control in &response.visible_controls {
        if let MtgoCompetitivePregameVisibleControlSemanticV1::SelectForBottom {
            card_slot,
            selected,
        } = &control.semantic
        {
            let slot = usize::from(*card_slot);
            if slot >= selected_slots.len() {
                return Err("native pregame selected card slot is out of range".to_owned());
            }
            selected_slots[slot] = *selected;
        }
    }
    if !is_sha256_v2(&context.source.classification_commitment_sha256)
        || !is_sha256_v2(&context.public_context_commitment_sha256)
        || !is_sha256_v2(&context.model_context_binding_commitment_sha256)
        || !is_sha256_v2(&runtime.runtime_commitment_sha256)
    {
        return Err("native pregame source commitments are malformed".to_owned());
    }
    let ordered_visible_cards = response
        .visible_cards
        .iter()
        .enumerate()
        .map(|(index, card)| {
            if usize::from(card.card_slot) != index {
                return Err("native pregame visible card order changed".to_owned());
            }
            Ok(MtgoCompetitiveNativePregameCardV1 {
                card_slot: card.card_slot,
                visible_card_name: card.visible_card_name.clone(),
                selected_for_bottom: selected_slots[index],
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let ordered_actions = response
        .visible_controls
        .iter()
        .filter_map(|control| match &control.semantic {
            MtgoCompetitivePregameVisibleControlSemanticV1::KeepOpeningHand => {
                Some(Ok(MtgoCompetitiveNativePregameActionV1::KeepOpeningHand))
            }
            MtgoCompetitivePregameVisibleControlSemanticV1::Mulligan { next_hand_size } => {
                Some(Ok(MtgoCompetitiveNativePregameActionV1::Mulligan {
                    next_hand_size: *next_hand_size,
                }))
            }
            MtgoCompetitivePregameVisibleControlSemanticV1::SelectForBottom {
                card_slot,
                selected: false,
            } => Some(
                ordered_visible_cards
                    .get(usize::from(*card_slot))
                    .filter(|card| card.card_slot == *card_slot)
                    .map(|_| MtgoCompetitiveNativePregameActionV1::SelectForBottom {
                        card_slot: *card_slot,
                    })
                    .ok_or_else(|| {
                        "native pregame legal bottom action does not resolve to the visible hand"
                            .to_owned()
                    }),
            ),
            MtgoCompetitivePregameVisibleControlSemanticV1::SelectForBottom {
                selected: true,
                ..
            } => None,
            MtgoCompetitivePregameVisibleControlSemanticV1::SubmitBottoming => {
                Some(Ok(MtgoCompetitiveNativePregameActionV1::SubmitBottoming))
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    if ordered_actions.is_empty() {
        return Err("native pregame request has no legal action".to_owned());
    }

    let (stage, prospective_keep_size, required_bottom_count, selected_bottom_count) =
        match response.stage {
            MtgoCompetitivePregameStageLabelV1::MulliganChoice {
                prospective_keep_size,
            } => (
                MtgoCompetitivePregameStageV1::MulliganChoice {
                    prospective_keep_size,
                },
                Some(prospective_keep_size),
                0,
                0,
            ),
            MtgoCompetitivePregameStageLabelV1::LondonBottoming {
                required_bottom_count,
                selected_bottom_count,
            } => (
                MtgoCompetitivePregameStageV1::LondonBottoming {
                    required_bottom_count,
                    selected_bottom_count,
                },
                None,
                required_bottom_count,
                selected_bottom_count,
            ),
            MtgoCompetitivePregameStageLabelV1::GameplayReady => unreachable!(),
        };
    if usize::from(selected_bottom_count) != ordered_confirmed_bottom_slots.len() {
        return Err(
            "native pregame request changed the ordered confirmed bottom history".to_owned(),
        );
    }
    for card_slot in ordered_confirmed_bottom_slots {
        if !ordered_visible_cards
            .get(usize::from(*card_slot))
            .is_some_and(|card| card.card_slot == *card_slot && card.selected_for_bottom)
        {
            return Err(
                "native pregame request bottom history differs from the visible selected cards"
                    .to_owned(),
            );
        }
    }
    let model_input = MtgoCompetitiveNativePregameModelInputV1 {
        game_number,
        play_draw: context.play_draw,
        acting_player_games_won: context.acting_player_games_won,
        opponent_games_won: context.opponent_games_won,
        player_known_deck_configuration: player_known_deck_configuration.clone(),
        stage,
        prospective_keep_size,
        required_bottom_count,
        selected_bottom_count,
        ordered_visible_cards,
        ordered_confirmed_bottom_slots: ordered_confirmed_bottom_slots.to_vec(),
        ordered_actions,
    };
    validate_competitive_native_pregame_model_input_v1(&model_input)?;
    Ok(model_input)
}

#[cfg(test)]
fn competitive_native_pregame_model_input_from_checked_parts_for_tests_v1(
    runtime: &MtgoCompetitiveEventRuntimeCommitmentsV1,
    context: &crate::probe::MtgoClassifiedCompetitivePregameModelContextCommitmentsV1,
    response: &mtgo_blackbox_v1::MtgoCompetitivePregameClassifierResponseV1,
    deck_manifest: &ValidatedMtgoCompetitiveDeckManifestV1,
) -> Result<MtgoCompetitiveNativePregameModelInputV1, String> {
    let player_known_deck_configuration =
        visible_native_sideboard_configuration_v1(deck_manifest.configuration_v1())?;
    competitive_native_pregame_model_input_from_parts_v1(
        runtime,
        context,
        response,
        &player_known_deck_configuration,
        &[],
    )
}

pub fn competitive_native_pregame_model_input_commitment_v1(
    model_input: &MtgoCompetitiveNativePregameModelInputV1,
) -> Result<String, String> {
    let canonical = serde_json::to_vec(model_input)
        .map_err(|error| format!("serialize native pregame model input: {error}"))?;
    Ok(hash_parts_v2(
        COMPETITIVE_NATIVE_PREGAME_MODEL_INPUT_DOMAIN_V1,
        &[
            &canonical,
            b"player_visible_game_information_only_no_model_selection_no_input",
        ],
    ))
}

/// Rechecks the complete coordinate-free model request. This validates only
/// structural integrity. It does not admit a
/// classifier, score a checkpoint, select an action, or authorize input.
pub fn validate_competitive_native_pregame_model_input_v1(
    model_input: &MtgoCompetitiveNativePregameModelInputV1,
) -> Result<(), String> {
    let score_valid = match model_input.game_number {
        1 => model_input.acting_player_games_won == 0 && model_input.opponent_games_won == 0,
        2 => {
            model_input
                .acting_player_games_won
                .checked_add(model_input.opponent_games_won)
                == Some(1)
                && model_input.acting_player_games_won <= 1
                && model_input.opponent_games_won <= 1
        }
        3 => model_input.acting_player_games_won == 1 && model_input.opponent_games_won == 1,
        _ => false,
    };
    if !score_valid {
        return Err("native pregame request scope or completeness is invalid".to_owned());
    }
    crate::competitive_native_sideboard::validate_native_sideboard_configuration_v1(
        &model_input.player_known_deck_configuration,
    )?;
    if model_input.ordered_visible_cards.len() != 7
        || model_input.ordered_actions.is_empty()
        || model_input.ordered_confirmed_bottom_slots.len()
            != usize::from(model_input.selected_bottom_count)
    {
        return Err(
            "native pregame request hand, action, or bottom history is incomplete".to_owned(),
        );
    }
    let mut confirmed_bottom_slots = std::collections::HashSet::new();
    for card_slot in &model_input.ordered_confirmed_bottom_slots {
        if usize::from(*card_slot) >= model_input.ordered_visible_cards.len()
            || !confirmed_bottom_slots.insert(*card_slot)
        {
            return Err("native pregame request bottom history is invalid".to_owned());
        }
    }
    for (index, card) in model_input.ordered_visible_cards.iter().enumerate() {
        if usize::from(card.card_slot) != index
            || card.visible_card_name.trim().is_empty()
            || card.visible_card_name.len() > 256
            || card.visible_card_name.chars().any(char::is_control)
            || card.selected_for_bottom
                != model_input
                    .ordered_confirmed_bottom_slots
                    .contains(&card.card_slot)
        {
            return Err("native pregame request visible-card identity is invalid".to_owned());
        }
    }
    let mut action_slots = std::collections::HashSet::new();
    for action in &model_input.ordered_actions {
        if let MtgoCompetitiveNativePregameActionV1::SelectForBottom { card_slot } = action {
            if !action_slots.insert(*card_slot)
                || !model_input
                    .ordered_visible_cards
                    .get(usize::from(*card_slot))
                    .is_some_and(|card| card.card_slot == *card_slot && !card.selected_for_bottom)
            {
                return Err("native pregame request bottom action is invalid".to_owned());
            }
        }
    }
    match model_input.stage {
        MtgoCompetitivePregameStageV1::MulliganChoice {
            prospective_keep_size,
        } => {
            if model_input.prospective_keep_size != Some(prospective_keep_size)
                || prospective_keep_size > 7
                || model_input.required_bottom_count != 0
                || model_input.selected_bottom_count != 0
                || !model_input.ordered_confirmed_bottom_slots.is_empty()
                || model_input.ordered_actions.first()
                    != Some(&MtgoCompetitiveNativePregameActionV1::KeepOpeningHand)
            {
                return Err("native pregame mulligan request is inconsistent".to_owned());
            }
            let expected_len = if prospective_keep_size == 0 { 1 } else { 2 };
            if model_input.ordered_actions.len() != expected_len
                || prospective_keep_size > 0
                    && model_input.ordered_actions.get(1)
                        != Some(&MtgoCompetitiveNativePregameActionV1::Mulligan {
                            next_hand_size: prospective_keep_size - 1,
                        })
            {
                return Err("native pregame mulligan action set is incomplete".to_owned());
            }
        }
        MtgoCompetitivePregameStageV1::LondonBottoming {
            required_bottom_count,
            selected_bottom_count,
        } => {
            if model_input.prospective_keep_size.is_some()
                || required_bottom_count != model_input.required_bottom_count
                || selected_bottom_count != model_input.selected_bottom_count
                || !(1..=7).contains(&required_bottom_count)
                || selected_bottom_count > required_bottom_count
            {
                return Err("native pregame London request is inconsistent".to_owned());
            }
            let expected_select_count = usize::from(7 - selected_bottom_count);
            let submit_required = selected_bottom_count == required_bottom_count;
            let mut expected_actions = model_input
                .ordered_visible_cards
                .iter()
                .filter(|card| !card.selected_for_bottom)
                .map(
                    |card| MtgoCompetitiveNativePregameActionV1::SelectForBottom {
                        card_slot: card.card_slot,
                    },
                )
                .collect::<Vec<_>>();
            if submit_required {
                expected_actions.push(MtgoCompetitiveNativePregameActionV1::SubmitBottoming);
            }
            if action_slots.len() != expected_select_count
                || model_input.ordered_actions != expected_actions
            {
                return Err("native pregame London action set is incomplete".to_owned());
            }
        }
        MtgoCompetitivePregameStageV1::GameplayReady => {
            return Err("GameplayReady is not a pregame model decision".to_owned())
        }
    }
    Ok(())
}

/// Selects one Keep, Mulligan, London card, or Submit action from the exact
/// classifier-backed event session using the separately admitted deck-bound
/// non-model pregame policy. The result retains the session and private
/// control target but cannot prepare or emit input.
pub fn plan_competitive_event_pregame_action_v1(
    session: OpaqueMtgoCompetitiveEventPregameSessionV1,
    heuristic: AdmittedMtgoCompetitivePregameHeuristicV1,
) -> Result<OpaqueMtgoCompetitivePregameActionPlanV1, String> {
    validate_competitive_event_pregame_session_integrity_v1(&session)?;
    let heuristic_commitments = heuristic.commitments_v1();
    if heuristic_commitments.deck_manifest_commitment_sha256
        != session.commitments.deck_manifest_sha256
        || heuristic_commitments.deck_format_sha256 != session.commitments.deck_format_sha256
        || heuristic_commitments.gameplay_policy_deployment_commitment_sha256
            != session.commitments.policy_deployment_commitment_sha256
    {
        return Err(
            "competitive pregame heuristic differs from the event deck, format, or gameplay policy"
                .to_owned(),
        );
    }
    let current = session.current_observation.commitments_v1();
    let classified = session
        .current_observation
        ._classified_source
        .as_ref()
        .ok_or("competitive pregame action planning requires a retained classified frame")?;
    let response = classified.response_v1();
    let response_stage = match response.stage {
        MtgoCompetitivePregameStageLabelV1::MulliganChoice {
            prospective_keep_size,
        } => MtgoCompetitivePregameStageV1::MulliganChoice {
            prospective_keep_size,
        },
        MtgoCompetitivePregameStageLabelV1::LondonBottoming {
            required_bottom_count,
            selected_bottom_count,
        } => MtgoCompetitivePregameStageV1::LondonBottoming {
            required_bottom_count,
            selected_bottom_count,
        },
        MtgoCompetitivePregameStageLabelV1::GameplayReady => {
            MtgoCompetitivePregameStageV1::GameplayReady
        }
    };
    if response_stage != current.stage
        || response.visible_interaction_commitment_sha256
            != current.visible_interaction_commitment_sha256
    {
        return Err(
            "competitive pregame retained response differs from the event observation".to_owned(),
        );
    }
    let selected_semantic = heuristic.select_visible_control_v1(
        response.stage,
        &response.visible_cards,
        &response.visible_controls,
    )?;
    let mut matching_controls = response
        .visible_controls
        .iter()
        .filter(|control| control.semantic == selected_semantic);
    let selected_control = matching_controls
        .next()
        .cloned()
        .ok_or("competitive pregame selected control is absent")?;
    if matching_controls.next().is_some() {
        return Err("competitive pregame selected control is ambiguous".to_owned());
    }
    let selected_action = match selected_semantic {
        MtgoCompetitivePregameVisibleControlSemanticV1::KeepOpeningHand => {
            MtgoCompetitivePregameSelectedActionV1::KeepOpeningHand
        }
        MtgoCompetitivePregameVisibleControlSemanticV1::Mulligan { next_hand_size } => {
            MtgoCompetitivePregameSelectedActionV1::Mulligan { next_hand_size }
        }
        MtgoCompetitivePregameVisibleControlSemanticV1::SelectForBottom {
            card_slot,
            selected: false,
        } => {
            let card = response
                .visible_cards
                .get(usize::from(card_slot))
                .filter(|card| card.card_slot == card_slot)
                .ok_or("competitive pregame selected card does not resolve")?;
            MtgoCompetitivePregameSelectedActionV1::SelectForBottom {
                card_slot,
                visible_card_name: card.visible_card_name.clone(),
            }
        }
        MtgoCompetitivePregameVisibleControlSemanticV1::SubmitBottoming => {
            MtgoCompetitivePregameSelectedActionV1::SubmitBottoming
        }
        MtgoCompetitivePregameVisibleControlSemanticV1::SelectForBottom {
            selected: true, ..
        } => {
            return Err(
                "competitive pregame heuristic selected an already selected card".to_owned(),
            )
        }
    };
    let expected_postcondition =
        competitive_pregame_expected_postcondition_v1(response.stage, &selected_action)?;
    let selected_action_json = serde_json::to_vec(&selected_action)
        .map_err(|error| format!("serialize competitive pregame selected action: {error}"))?;
    let expected_postcondition_json = serde_json::to_vec(&expected_postcondition)
        .map_err(|error| format!("serialize competitive pregame postcondition: {error}"))?;
    let selected_control_json = serde_json::to_vec(&selected_control)
        .map_err(|error| format!("serialize competitive pregame selected control: {error}"))?;
    let action_plan_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_EVENT_PREGAME_ACTION_PLAN_DOMAIN_V1,
        &[
            session.commitments.session_commitment_sha256.as_bytes(),
            current.observation_commitment_sha256.as_bytes(),
            current.pregame_classification_commitment_sha256.as_bytes(),
            current.visible_interaction_commitment_sha256.as_bytes(),
            heuristic_commitments
                .heuristic_profile_commitment_sha256
                .as_bytes(),
            heuristic_commitments
                .heuristic_algorithm_commitment_sha256
                .as_bytes(),
            heuristic_commitments.review_commitment_sha256.as_bytes(),
            heuristic_commitments.admission_commitment_sha256.as_bytes(),
            &selected_action_json,
            &expected_postcondition_json,
            &selected_control_json,
            current.frame_id.to_be_bytes().as_slice(),
            current.frame_sequence.to_be_bytes().as_slice(),
            b"deck_bound_deterministic_visible_pregame_selection_no_input",
        ],
    );
    let commitments = MtgoCompetitivePregameActionPlanCommitmentsV1 {
        action_plan_commitment_sha256,
        pregame_session_commitment_sha256: session.commitments.session_commitment_sha256.clone(),
        current_observation_commitment_sha256: current.observation_commitment_sha256,
        pregame_classification_commitment_sha256: current.pregame_classification_commitment_sha256,
        visible_interaction_commitment_sha256: current.visible_interaction_commitment_sha256,
        heuristic_profile_commitment_sha256: heuristic_commitments
            .heuristic_profile_commitment_sha256,
        heuristic_algorithm_commitment_sha256: heuristic_commitments
            .heuristic_algorithm_commitment_sha256,
        heuristic_review_commitment_sha256: heuristic_commitments.review_commitment_sha256,
        heuristic_admission_commitment_sha256: heuristic_commitments.admission_commitment_sha256,
        selected_control_visible_content_sha256: selected_control.visible_content_sha256.clone(),
        event_kind: session.commitments.event_kind,
        match_identity_sha256: session.commitments.match_identity_sha256.clone(),
        game_number: session.commitments.game_number,
        frame_id: current.frame_id,
        frame_sequence: current.frame_sequence,
        selected_action,
        expected_postcondition,
    };
    Ok(OpaqueMtgoCompetitivePregameActionPlanV1 {
        _session: session,
        _heuristic: heuristic,
        _selected_control: selected_control,
        commitments,
    })
}

/// Rechecks a selected competitive pregame action against the immediate next
/// classified capture. The current cards must retain their ordered visible
/// labels, and the exact selected control must retain its semantic, rectangle,
/// enabled state, and pixel commitment. Coordinates remain private and no
/// input trait is implemented for the returned value.
pub fn prepare_fresh_competitive_event_pregame_action_v1(
    plan: OpaqueMtgoCompetitivePregameActionPlanV1,
    fresh: OpaqueMtgoClassifiedCompetitivePregameFrameV1,
) -> Result<OpaqueMtgoPreparedCompetitivePregameActionV1, String> {
    validate_competitive_event_pregame_session_integrity_v1(&plan._session)?;
    let planned = plan.commitments_v1();
    let prior = plan._session.current_observation.commitments_v1();
    if planned.pregame_session_commitment_sha256
        != plan._session.commitments.session_commitment_sha256
        || planned.current_observation_commitment_sha256 != prior.observation_commitment_sha256
        || planned.pregame_classification_commitment_sha256
            != prior.pregame_classification_commitment_sha256
        || planned.visible_interaction_commitment_sha256
            != prior.visible_interaction_commitment_sha256
        || planned.frame_id != prior.frame_id
        || planned.frame_sequence != prior.frame_sequence
    {
        return Err("competitive pregame action plan lineage changed".to_owned());
    }
    fresh.require_immediate_successor_v1(
        prior.frame_id,
        prior.frame_sequence,
        &prior.source_capture_commitment_sha256,
        prior.captured_at_unix_millis,
    )?;
    let fresh_process = fresh.process_continuity_commitment_sha256_v1();
    let fresh_window = fresh.window_continuity_commitment_sha256_v1()?;
    if fresh_process != prior.process_continuity_commitment_sha256
        || fresh_window != prior.window_continuity_commitment_sha256
    {
        return Err(
            "competitive pregame fresh preparation changed the event process or window".to_owned(),
        );
    }
    let fresh_commitments = fresh.commitments_v1();
    if fresh_commitments
        .source_frame
        .perception_profile_commitment_sha256
        != prior.duel_perception_profile_commitment_sha256
        || fresh_commitments
            .source_frame
            .perception_profile_admission_commitment_sha256
            != prior.duel_perception_profile_admission_commitment_sha256
        || fresh_commitments.classifier_runtime_identity_commitment_sha256
            != prior.classifier_runtime_commitment_sha256
        || fresh_commitments.pregame_evaluation_commitment_sha256
            != prior.pregame_evaluation_commitment_sha256
        || fresh_commitments.pregame_profile_admission_commitment_sha256
            != prior.pregame_profile_admission_commitment_sha256
        || fresh_commitments.frame_sequence
            > plan
                ._session
                .match_launch
                .authorization
                .valid_through_frame_sequence
    {
        return Err(
            "competitive pregame fresh preparation changed profile, classifier, authorization, or lifetime"
                .to_owned(),
        );
    }
    let prior_response = plan
        ._session
        .current_observation
        ._classified_source
        .as_ref()
        .ok_or("competitive pregame action plan lost its classified source")?
        .response_v1();
    let fresh_response = fresh.response_v1();
    if fresh_response.stage != prior_response.stage
        || !competitive_pregame_visible_card_labels_equal_v1(
            &prior_response.visible_cards,
            &fresh_response.visible_cards,
        )
    {
        return Err(
            "competitive pregame stage or visible card labels changed before preparation"
                .to_owned(),
        );
    }
    let mut selected_controls = fresh_response
        .visible_controls
        .iter()
        .filter(|control| control.semantic == plan._selected_control.semantic);
    let fresh_selected_control = selected_controls
        .next()
        .cloned()
        .ok_or("competitive pregame selected control disappeared before preparation")?;
    if selected_controls.next().is_some() || fresh_selected_control != plan._selected_control {
        return Err(
            "competitive pregame selected control semantic, rectangle, state, or pixels changed before preparation"
                .to_owned(),
        );
    }
    let pointer_target =
        fresh.resolve_visible_control_pointer_target_v1(&fresh_selected_control)?;
    let selected_control_json = serde_json::to_vec(&fresh_selected_control)
        .map_err(|error| format!("serialize fresh competitive pregame control: {error}"))?;
    let selected_action_json = serde_json::to_vec(&planned.selected_action)
        .map_err(|error| format!("serialize fresh competitive pregame action: {error}"))?;
    let expected_postcondition_json = serde_json::to_vec(&planned.expected_postcondition)
        .map_err(|error| format!("serialize fresh competitive pregame postcondition: {error}"))?;
    let preparation_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_EVENT_PREGAME_FRESH_PREPARATION_DOMAIN_V1,
        &[
            planned.action_plan_commitment_sha256.as_bytes(),
            planned.pregame_session_commitment_sha256.as_bytes(),
            fresh_commitments
                .classification_commitment_sha256
                .as_bytes(),
            fresh_commitments
                .visible_interaction_commitment_sha256
                .as_bytes(),
            fresh_commitments
                .source_frame
                .source_capture
                .capture_commitment_sha256
                .as_bytes(),
            &selected_control_json,
            &selected_action_json,
            &expected_postcondition_json,
            fresh_commitments.frame_id.to_be_bytes().as_slice(),
            fresh_commitments.frame_sequence.to_be_bytes().as_slice(),
            fresh_commitments
                .captured_at_unix_millis
                .to_be_bytes()
                .as_slice(),
            b"immediate_reclassified_same_cards_exact_control_private_target_no_input",
        ],
    );
    let commitments = MtgoPreparedCompetitivePregameActionCommitmentsV1 {
        preparation_commitment_sha256,
        action_plan_commitment_sha256: planned.action_plan_commitment_sha256,
        pregame_session_commitment_sha256: planned.pregame_session_commitment_sha256,
        fresh_classification_commitment_sha256: fresh_commitments.classification_commitment_sha256,
        fresh_visible_interaction_commitment_sha256: fresh_commitments
            .visible_interaction_commitment_sha256,
        selected_control_visible_content_sha256: fresh_selected_control.visible_content_sha256,
        event_kind: planned.event_kind,
        match_identity_sha256: planned.match_identity_sha256,
        game_number: planned.game_number,
        planned_frame_id: planned.frame_id,
        planned_frame_sequence: planned.frame_sequence,
        fresh_frame_id: fresh_commitments.frame_id,
        fresh_frame_sequence: fresh_commitments.frame_sequence,
        fresh_captured_at_unix_millis: fresh_commitments.captured_at_unix_millis,
        selected_action: planned.selected_action,
        expected_postcondition: planned.expected_postcondition,
    };
    Ok(OpaqueMtgoPreparedCompetitivePregameActionV1 {
        _plan: plan,
        _fresh_classified_frame: fresh,
        _pointer_target: pointer_target,
        commitments,
    })
}

/// Exercises the exact postcondition contract without an input receipt. This
/// is useful for offline corpus and wiring tests only. It cannot claim that the
/// planned action caused the next visible state or release the input gate.
pub fn check_competitive_event_pregame_postcondition_dry_run_v1(
    prepared: OpaqueMtgoPreparedCompetitivePregameActionV1,
    after: OpaqueMtgoClassifiedCompetitivePregameFrameV1,
) -> Result<CheckedUntrustedMtgoCompetitivePregamePostconditionV1, String> {
    let before = prepared._fresh_classified_frame.commitments_v1();
    after.require_immediate_successor_v1(
        before.frame_id,
        before.frame_sequence,
        &before.source_frame.source_capture.capture_commitment_sha256,
        before.captured_at_unix_millis,
    )?;
    let prior = prepared._plan._session.current_observation.commitments_v1();
    let after_process = after.process_continuity_commitment_sha256_v1();
    let after_window = after.window_continuity_commitment_sha256_v1()?;
    let after_commitments = after.commitments_v1();
    if after_process != prior.process_continuity_commitment_sha256
        || after_window != prior.window_continuity_commitment_sha256
        || after_commitments
            .source_frame
            .perception_profile_commitment_sha256
            != prior.duel_perception_profile_commitment_sha256
        || after_commitments
            .source_frame
            .perception_profile_admission_commitment_sha256
            != prior.duel_perception_profile_admission_commitment_sha256
        || after_commitments.classifier_runtime_identity_commitment_sha256
            != prior.classifier_runtime_commitment_sha256
        || after_commitments.pregame_evaluation_commitment_sha256
            != prior.pregame_evaluation_commitment_sha256
        || after_commitments.pregame_profile_admission_commitment_sha256
            != prior.pregame_profile_admission_commitment_sha256
        || after_commitments.frame_sequence
            > prepared
                ._plan
                ._session
                .match_launch
                .authorization
                .valid_through_frame_sequence
    {
        return Err(
            "competitive pregame dry-run postcondition changed profile, process, window, or authorization"
                .to_owned(),
        );
    }
    validate_competitive_pregame_expected_postcondition_v1(
        &prepared.commitments.expected_postcondition,
        &prepared.commitments.selected_action,
        prepared._fresh_classified_frame.response_v1(),
        after.response_v1(),
    )?;
    let selected_action_json = serde_json::to_vec(&prepared.commitments.selected_action)
        .map_err(|error| format!("serialize dry-run competitive pregame action: {error}"))?;
    let observed_postcondition = prepared.commitments.expected_postcondition.clone();
    let observed_postcondition_json = serde_json::to_vec(&observed_postcondition)
        .map_err(|error| format!("serialize dry-run competitive pregame postcondition: {error}"))?;
    let dry_run_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_EVENT_PREGAME_POSTCONDITION_DRY_RUN_DOMAIN_V1,
        &[
            prepared
                .commitments
                .preparation_commitment_sha256
                .as_bytes(),
            prepared
                .commitments
                .action_plan_commitment_sha256
                .as_bytes(),
            after_commitments
                .classification_commitment_sha256
                .as_bytes(),
            after_commitments
                .visible_interaction_commitment_sha256
                .as_bytes(),
            &selected_action_json,
            &observed_postcondition_json,
            after_commitments.frame_id.to_be_bytes().as_slice(),
            after_commitments.frame_sequence.to_be_bytes().as_slice(),
            after_commitments
                .captured_at_unix_millis
                .to_be_bytes()
                .as_slice(),
            b"structural_visible_postcondition_only_no_input_receipt_no_causality_no_gate_release",
        ],
    );
    let commitments = MtgoCheckedCompetitivePregamePostconditionCommitmentsV1 {
        dry_run_commitment_sha256,
        preparation_commitment_sha256: prepared.commitments.preparation_commitment_sha256.clone(),
        action_plan_commitment_sha256: prepared.commitments.action_plan_commitment_sha256.clone(),
        after_classification_commitment_sha256: after_commitments.classification_commitment_sha256,
        after_visible_interaction_commitment_sha256: after_commitments
            .visible_interaction_commitment_sha256,
        event_kind: prepared.commitments.event_kind,
        match_identity_sha256: prepared.commitments.match_identity_sha256.clone(),
        game_number: prepared.commitments.game_number,
        before_frame_sequence: prepared.commitments.fresh_frame_sequence,
        after_frame_id: after_commitments.frame_id,
        after_frame_sequence: after_commitments.frame_sequence,
        after_captured_at_unix_millis: after_commitments.captured_at_unix_millis,
        selected_action: prepared.commitments.selected_action.clone(),
        observed_postcondition,
    };
    Ok(CheckedUntrustedMtgoCompetitivePregamePostconditionV1 {
        _prepared: prepared,
        _after_classified_frame: after,
        commitments,
    })
}

/// Retains the legacy non-model pregame executor for internal validation only.
/// It is deliberately not exported: League or Challenge pregame input must
/// remain unavailable until a future opaque native model decision owns the
/// selected action and its checkpoint provenance.
#[allow(dead_code)]
pub(crate) fn execute_prepared_competitive_pregame_action_v1(
    prepared: OpaqueMtgoPreparedCompetitivePregameActionV1,
    authorization: RatifiedMtgoCompetitivePregameAuthorizationV1,
) -> Result<OpaqueMtgoPendingCompetitivePregameInputV1, String> {
    validate_competitive_pregame_authorization_for_prepared_v1(&authorization, &prepared)?;
    reserve_input_gate_v3()?;
    let input_sent_at_unix_millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(error) => {
            release_unattempted_reservation_v3()?;
            return Err(format!("system clock is before epoch: {error}"));
        }
    };
    if validate_preinput_capture_freshness_v3(
        prepared.commitments.fresh_captured_at_unix_millis,
        input_sent_at_unix_millis,
    )
    .is_err()
    {
        release_unattempted_reservation_v3()?;
        return Err("the competitive pregame preparation is stale or future-dated".to_owned());
    }
    halt_before_input_attempt_v3()?;
    let cursor_parked_outside_client = send_exactly_one_left_click_v3(&prepared._pointer_target)?;
    let selected_action_json = serde_json::to_vec(&prepared.commitments.selected_action)
        .map_err(|error| format!("serialize competitive pregame input action: {error}"))?;
    let input_receipt_sha256 = hash_parts_v2(
        COMPETITIVE_EVENT_PREGAME_INPUT_RECEIPT_DOMAIN_V1,
        &[
            prepared
                .commitments
                .preparation_commitment_sha256
                .as_bytes(),
            prepared
                .commitments
                .action_plan_commitment_sha256
                .as_bytes(),
            authorization
                .commitments
                .ratification_commitment_sha256
                .as_bytes(),
            competitive_event_kind_tag_v1(prepared.commitments.event_kind),
            prepared.commitments.match_identity_sha256.as_bytes(),
            &[prepared.commitments.game_number],
            &selected_action_json,
            prepared.commitments.fresh_frame_id.to_be_bytes().as_slice(),
            prepared
                .commitments
                .fresh_frame_sequence
                .to_be_bytes()
                .as_slice(),
            input_sent_at_unix_millis.to_be_bytes().as_slice(),
            &[u8::from(cursor_parked_outside_client)],
            b"exactly_one_competitive_pregame_left_click_pending_exact_visible_postcondition",
        ],
    );
    set_pending_v3(&input_receipt_sha256)?;
    let commitments = MtgoCompetitivePregameInputReceiptCommitmentsV1 {
        input_receipt_sha256,
        preparation_commitment_sha256: prepared.commitments.preparation_commitment_sha256.clone(),
        action_plan_commitment_sha256: prepared.commitments.action_plan_commitment_sha256.clone(),
        authorization_ratification_commitment_sha256: authorization
            .commitments
            .ratification_commitment_sha256
            .clone(),
        event_kind: prepared.commitments.event_kind,
        match_identity_sha256: prepared.commitments.match_identity_sha256.clone(),
        game_number: prepared.commitments.game_number,
        before_frame_id: prepared.commitments.fresh_frame_id,
        before_frame_sequence: prepared.commitments.fresh_frame_sequence,
        input_sent_at_unix_millis,
        cursor_parked_outside_client,
        selected_action: prepared.commitments.selected_action.clone(),
    };
    Ok(OpaqueMtgoPendingCompetitivePregameInputV1 {
        prepared,
        authorization,
        commitments,
    })
}

/// Releases the shared gate and returns the advanced event pregame session
/// only when the immediate post-input classified frame satisfies the exact
/// action-specific visible transition.
pub fn confirm_pending_competitive_pregame_action_v1(
    pending: OpaqueMtgoPendingCompetitivePregameInputV1,
    after: OpaqueMtgoClassifiedCompetitivePregameModelContextV1,
) -> Result<OpaqueMtgoConfirmedCompetitivePregameActionV1, String> {
    require_matching_pending_v3(&pending.commitments.input_receipt_sha256)?;
    let input = pending.commitments.clone();
    let (after, after_model_context) = after.into_classified_frame_and_public_context_witness_v1();
    let after_player_visible_public_context =
        MtgoCompetitivePregamePlayerVisiblePublicContextStateV1::from(
            after_model_context.commitments_v1(),
        );
    let checked =
        match check_competitive_event_pregame_postcondition_dry_run_v1(pending.prepared, after) {
            Ok(value) => value,
            Err(error) => {
                halt_gate_v3()?;
                return Err(format!(
                "competitive pregame postcondition failed and the input gate is halted: {error}"
            ));
            }
        };
    let postcondition = checked.commitments_v1();
    if postcondition.after_captured_at_unix_millis <= input.input_sent_at_unix_millis
        || postcondition.before_frame_sequence != input.before_frame_sequence
        || postcondition.preparation_commitment_sha256 != input.preparation_commitment_sha256
        || postcondition.action_plan_commitment_sha256 != input.action_plan_commitment_sha256
        || postcondition.event_kind != input.event_kind
        || postcondition.match_identity_sha256 != input.match_identity_sha256
        || postcondition.game_number != input.game_number
        || postcondition.selected_action != input.selected_action
    {
        halt_gate_v3()?;
        return Err(
            "competitive pregame postcondition changed input lineage and the gate is halted"
                .to_owned(),
        );
    }
    let CheckedUntrustedMtgoCompetitivePregamePostconditionV1 {
        _prepared: prepared,
        _after_classified_frame: after,
        commitments: _,
    } = checked;
    let OpaqueMtgoPreparedCompetitivePregameActionV1 {
        _plan: plan,
        _fresh_classified_frame: _,
        _pointer_target: _,
        commitments: _,
    } = prepared;
    let OpaqueMtgoCompetitivePregameActionPlanV1 {
        _session: session,
        _heuristic: _,
        _selected_control: _,
        commitments: _,
    } = plan;
    if session.player_visible_public_context != Some(after_player_visible_public_context) {
        halt_gate_v3()?;
        return Err(
            "competitive pregame confirmation changed the player-visible public context and the input gate is halted"
                .to_owned(),
        );
    }
    let after_process = after.process_continuity_commitment_sha256_v1();
    let after_window = match after.window_continuity_commitment_sha256_v1() {
        Ok(value) => value,
        Err(error) => {
            halt_gate_v3()?;
            return Err(format!(
                "competitive pregame confirmation window failed and the input gate is halted: {error}"
            ));
        }
    };
    let after_classified = after.commitments_v1();
    let next_commitments = match competitive_pregame_observation_from_classified_view_v2(
        &session.runtime.commitments,
        CompetitivePregameClassifiedViewV2::from_commitments_v2(
            &after_classified,
            &after_process,
            &after_window,
        ),
    ) {
        Ok(value) => value,
        Err(error) => {
            halt_gate_v3()?;
            return Err(format!(
                "competitive pregame confirmation observation failed and the input gate is halted: {error}"
            ));
        }
    };
    let next = OpaqueMtgoCompetitivePregameObservationV1 {
        _classified_source: Some(OpaqueMtgoCompetitivePregameClassifiedSourceV1::Frame(
            Box::new(after),
        )),
        commitments: next_commitments,
    };
    let confirmed_bottom_slot = match &input.selected_action {
        MtgoCompetitivePregameSelectedActionV1::SelectForBottom { card_slot, .. } => {
            Some(*card_slot)
        }
        _ => None,
    };
    let mut session = match advance_competitive_event_pregame_observed_with_confirmed_bottom_v1(
        session,
        next,
        confirmed_bottom_slot,
    ) {
        Ok(value) => value,
        Err(error) => {
            halt_gate_v3()?;
            return Err(format!(
                "competitive pregame session advance failed and the input gate is halted: {error}"
            ));
        }
    };
    session
        .commitments
        .current_model_context_binding_commitment_sha256 = Some(
        after_model_context
            .commitments_v1()
            .model_context_binding_commitment_sha256
            .clone(),
    );
    session
        .commitments
        .player_visible_public_context_commitment_sha256 = Some(
        competitive_pregame_player_visible_public_context_state_commitment_v1(
            after_player_visible_public_context,
        ),
    );
    session.player_visible_public_context = Some(after_player_visible_public_context);
    session.current_model_context = Some(after_model_context);
    let prior = session
        .commitments
        .prior_session_commitment_sha256
        .clone()
        .ok_or("advanced competitive pregame session lost its prior chain")?;
    session.commitments.session_commitment_sha256 =
        competitive_event_pregame_session_commitment_v1(
            COMPETITIVE_EVENT_PREGAME_ADVANCE_DOMAIN_V1,
            Some(prior.as_str()),
            &session.commitments,
        )?;
    if let Err(error) = validate_competitive_event_pregame_session_integrity_v1(&session) {
        halt_gate_v3()?;
        return Err(format!(
            "competitive pregame retained public context failed and the input gate is halted: {error}"
        ));
    }
    let confirmation_receipt_sha256 = hash_parts_v2(
        COMPETITIVE_EVENT_PREGAME_CONFIRMATION_RECEIPT_DOMAIN_V1,
        &[
            input.input_receipt_sha256.as_bytes(),
            input.preparation_commitment_sha256.as_bytes(),
            input
                .authorization_ratification_commitment_sha256
                .as_bytes(),
            postcondition.dry_run_commitment_sha256.as_bytes(),
            session.commitments.session_commitment_sha256.as_bytes(),
            competitive_event_kind_tag_v1(input.event_kind),
            input.match_identity_sha256.as_bytes(),
            &[input.game_number],
            postcondition.after_frame_id.to_be_bytes().as_slice(),
            postcondition.after_frame_sequence.to_be_bytes().as_slice(),
            postcondition
                .after_captured_at_unix_millis
                .to_be_bytes()
                .as_slice(),
            b"receipt_bound_exact_visible_pregame_transition_shared_gate_released",
        ],
    );
    release_confirmed_pending_v3(&input.input_receipt_sha256)?;
    let commitments = MtgoConfirmedCompetitivePregameActionCommitmentsV1 {
        confirmation_receipt_sha256,
        input_receipt_sha256: input.input_receipt_sha256,
        preparation_commitment_sha256: input.preparation_commitment_sha256,
        authorization_ratification_commitment_sha256: input
            .authorization_ratification_commitment_sha256,
        postcondition_commitment_sha256: postcondition.dry_run_commitment_sha256,
        advanced_pregame_session_commitment_sha256: session
            .commitments
            .session_commitment_sha256
            .clone(),
        event_kind: input.event_kind,
        match_identity_sha256: input.match_identity_sha256,
        game_number: input.game_number,
        after_frame_id: postcondition.after_frame_id,
        after_frame_sequence: postcondition.after_frame_sequence,
        after_captured_at_unix_millis: postcondition.after_captured_at_unix_millis,
        selected_action: input.selected_action,
        observed_postcondition: postcondition.observed_postcondition,
    };
    Ok(OpaqueMtgoConfirmedCompetitivePregameActionV1 {
        _authorization: pending.authorization,
        session,
        commitments,
    })
}

fn competitive_pregame_visible_card_labels_equal_v1(
    left: &[MtgoCompetitivePregameVisibleCardV1],
    right: &[MtgoCompetitivePregameVisibleCardV1],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.card_slot == right.card_slot && left.visible_card_name == right.visible_card_name
        })
}

pub(crate) fn competitive_pregame_expected_postcondition_v1(
    stage: MtgoCompetitivePregameStageLabelV1,
    selected_action: &MtgoCompetitivePregameSelectedActionV1,
) -> Result<MtgoCompetitivePregameExpectedPostconditionV1, String> {
    match (stage, selected_action) {
        (
            MtgoCompetitivePregameStageLabelV1::MulliganChoice {
                prospective_keep_size: 7,
            },
            MtgoCompetitivePregameSelectedActionV1::KeepOpeningHand,
        ) => Ok(MtgoCompetitivePregameExpectedPostconditionV1::GameplayReady),
        (
            MtgoCompetitivePregameStageLabelV1::MulliganChoice {
                prospective_keep_size,
            },
            MtgoCompetitivePregameSelectedActionV1::KeepOpeningHand,
        ) if prospective_keep_size < 7 => Ok(
            MtgoCompetitivePregameExpectedPostconditionV1::LondonBottoming {
                required_bottom_count: 7 - prospective_keep_size,
                selected_bottom_count: 0,
                newly_selected_card_slot: None,
            },
        ),
        (
            MtgoCompetitivePregameStageLabelV1::MulliganChoice {
                prospective_keep_size,
            },
            MtgoCompetitivePregameSelectedActionV1::Mulligan { next_hand_size },
        ) if prospective_keep_size > 0 && *next_hand_size == prospective_keep_size - 1 => Ok(
            MtgoCompetitivePregameExpectedPostconditionV1::MulliganChoice {
                prospective_keep_size: *next_hand_size,
            },
        ),
        (
            MtgoCompetitivePregameStageLabelV1::LondonBottoming {
                required_bottom_count,
                selected_bottom_count,
            },
            MtgoCompetitivePregameSelectedActionV1::SelectForBottom { card_slot, .. },
        ) if selected_bottom_count < required_bottom_count => Ok(
            MtgoCompetitivePregameExpectedPostconditionV1::LondonBottoming {
                required_bottom_count,
                selected_bottom_count: selected_bottom_count + 1,
                newly_selected_card_slot: Some(*card_slot),
            },
        ),
        (
            MtgoCompetitivePregameStageLabelV1::LondonBottoming {
                required_bottom_count,
                selected_bottom_count,
            },
            MtgoCompetitivePregameSelectedActionV1::SubmitBottoming,
        ) if selected_bottom_count == required_bottom_count => {
            Ok(MtgoCompetitivePregameExpectedPostconditionV1::GameplayReady)
        }
        _ => Err("competitive pregame action is invalid for the classified stage".to_owned()),
    }
}

fn validate_competitive_pregame_expected_postcondition_v1(
    expected: &MtgoCompetitivePregameExpectedPostconditionV1,
    selected_action: &MtgoCompetitivePregameSelectedActionV1,
    before: &mtgo_blackbox_v1::MtgoCompetitivePregameClassifierResponseV1,
    after: &mtgo_blackbox_v1::MtgoCompetitivePregameClassifierResponseV1,
) -> Result<(), String> {
    let stage_matches = match (expected, after.stage) {
        (
            MtgoCompetitivePregameExpectedPostconditionV1::MulliganChoice {
                prospective_keep_size: expected_size,
            },
            MtgoCompetitivePregameStageLabelV1::MulliganChoice {
                prospective_keep_size: observed_size,
            },
        ) => expected_size == &observed_size,
        (
            MtgoCompetitivePregameExpectedPostconditionV1::LondonBottoming {
                required_bottom_count: expected_required,
                selected_bottom_count: expected_selected,
                ..
            },
            MtgoCompetitivePregameStageLabelV1::LondonBottoming {
                required_bottom_count: observed_required,
                selected_bottom_count: observed_selected,
            },
        ) => expected_required == &observed_required && expected_selected == &observed_selected,
        (
            MtgoCompetitivePregameExpectedPostconditionV1::GameplayReady,
            MtgoCompetitivePregameStageLabelV1::GameplayReady,
        ) => true,
        _ => false,
    };
    if !stage_matches {
        return Err("competitive pregame visible postcondition stage does not match".to_owned());
    }

    match (expected, selected_action) {
        (
            MtgoCompetitivePregameExpectedPostconditionV1::LondonBottoming {
                newly_selected_card_slot,
                ..
            },
            MtgoCompetitivePregameSelectedActionV1::KeepOpeningHand,
        ) => {
            if newly_selected_card_slot.is_some()
                || !competitive_pregame_visible_card_labels_equal_v1(
                    &before.visible_cards,
                    &after.visible_cards,
                )
            {
                return Err(
                    "competitive pregame Keep changed card labels or invented a selected card"
                        .to_owned(),
                );
            }
        }
        (
            MtgoCompetitivePregameExpectedPostconditionV1::LondonBottoming {
                newly_selected_card_slot: Some(expected_slot),
                ..
            },
            MtgoCompetitivePregameSelectedActionV1::SelectForBottom { card_slot, .. },
        ) if expected_slot == card_slot => {
            if !competitive_pregame_visible_card_labels_equal_v1(
                &before.visible_cards,
                &after.visible_cards,
            ) {
                return Err(
                    "competitive pregame bottom selection changed visible card labels".to_owned(),
                );
            }
            let before_control = before.visible_controls.iter().find(|control| {
                control.semantic
                    == MtgoCompetitivePregameVisibleControlSemanticV1::SelectForBottom {
                        card_slot: *card_slot,
                        selected: false,
                    }
            });
            let after_control = after.visible_controls.iter().find(|control| {
                control.semantic
                    == MtgoCompetitivePregameVisibleControlSemanticV1::SelectForBottom {
                        card_slot: *card_slot,
                        selected: true,
                    }
            });
            let (before_control, after_control) = before_control
                .zip(after_control)
                .ok_or("competitive pregame selected card did not visibly toggle")?;
            if before_control.rect_client_px != after_control.rect_client_px
                || before_control.visible_content_sha256 == after_control.visible_content_sha256
            {
                return Err(
                    "competitive pregame selected card lacks an exact same-rect pixel change"
                        .to_owned(),
                );
            }
            for other_slot in 0_u8..7 {
                if other_slot == *card_slot {
                    continue;
                }
                let before_other =
                    before
                        .visible_controls
                        .iter()
                        .find_map(|control| match &control.semantic {
                            MtgoCompetitivePregameVisibleControlSemanticV1::SelectForBottom {
                                card_slot,
                                selected,
                            } if *card_slot == other_slot => {
                                Some((selected, &control.rect_client_px))
                            }
                            _ => None,
                        });
                let after_other = after
                    .visible_controls
                    .iter()
                    .find_map(|control| match &control.semantic {
                        MtgoCompetitivePregameVisibleControlSemanticV1::SelectForBottom {
                            card_slot,
                            selected,
                        } if *card_slot == other_slot => Some((selected, &control.rect_client_px)),
                        _ => None,
                    });
                if before_other != after_other {
                    return Err(
                        "competitive pregame bottom selection changed another card state or rectangle"
                            .to_owned(),
                    );
                }
            }
        }
        (MtgoCompetitivePregameExpectedPostconditionV1::GameplayReady, _)
        | (MtgoCompetitivePregameExpectedPostconditionV1::MulliganChoice { .. }, _) => {}
        _ => {
            return Err(
                "competitive pregame selected action does not match its postcondition".to_owned(),
            )
        }
    }
    Ok(())
}

/// Advances one exact paid-event pregame session with the immediate next
/// classifier-backed duel capture. This remains observation-only. A later
/// actuator must bind one sent input and its visible postcondition before it
/// can use the transition autonomously.
pub fn advance_competitive_event_pregame_from_classified_frame_v2(
    session: OpaqueMtgoCompetitiveEventPregameSessionV1,
    classified_frame: OpaqueMtgoClassifiedCompetitivePregameFrameV1,
) -> Result<OpaqueMtgoCompetitiveEventPregameSessionV1, String> {
    validate_competitive_event_pregame_session_integrity_v1(&session)?;
    if session.current_observation._classified_source.is_none() {
        return Err(
            "competitive pregame session did not originate from a retained classified frame"
                .to_owned(),
        );
    }
    let current = session.current_observation.commitments_v1();
    classified_frame.require_immediate_successor_v1(
        current.frame_id,
        current.frame_sequence,
        &current.source_capture_commitment_sha256,
        current.captured_at_unix_millis,
    )?;
    let process_continuity_commitment_sha256 =
        classified_frame.process_continuity_commitment_sha256_v1();
    let window_continuity_commitment_sha256 =
        classified_frame.window_continuity_commitment_sha256_v1()?;
    if process_continuity_commitment_sha256 != current.process_continuity_commitment_sha256
        || window_continuity_commitment_sha256 != current.window_continuity_commitment_sha256
    {
        return Err(
            "competitive pregame classifier successor changed the event process or window"
                .to_owned(),
        );
    }

    let classified = classified_frame.commitments_v1();
    let next_commitments = competitive_pregame_observation_from_classified_view_v2(
        &session.runtime.commitments,
        CompetitivePregameClassifiedViewV2::from_commitments_v2(
            &classified,
            &process_continuity_commitment_sha256,
            &window_continuity_commitment_sha256,
        ),
    )?;
    let next = OpaqueMtgoCompetitivePregameObservationV1 {
        _classified_source: Some(OpaqueMtgoCompetitivePregameClassifiedSourceV1::Frame(
            Box::new(classified_frame),
        )),
        commitments: next_commitments,
    };
    advance_competitive_event_pregame_observed_v1(session, next)
}

/// Advances only the evaluated visible pregame state. It does not claim that
/// an input caused the transition and does not expose or reopen an input gate.
/// A future actuator must join its own one-input receipt to each adjacent
/// transition before using this observation-only seam autonomously.
pub fn advance_competitive_event_pregame_observed_v1(
    session: OpaqueMtgoCompetitiveEventPregameSessionV1,
    next: OpaqueMtgoCompetitivePregameObservationV1,
) -> Result<OpaqueMtgoCompetitiveEventPregameSessionV1, String> {
    advance_competitive_event_pregame_observed_with_confirmed_bottom_v1(session, next, None)
}

fn advance_competitive_event_pregame_observed_with_confirmed_bottom_v1(
    mut session: OpaqueMtgoCompetitiveEventPregameSessionV1,
    next: OpaqueMtgoCompetitivePregameObservationV1,
    confirmed_bottom_slot: Option<u8>,
) -> Result<OpaqueMtgoCompetitiveEventPregameSessionV1, String> {
    validate_competitive_event_pregame_session_integrity_v1(&session)?;
    validate_competitive_pregame_observation_v1(&next.commitments)?;
    let current = &session.current_observation.commitments;
    let next_commitments = &next.commitments;
    if current.source_capture_commitment_sha256 == next_commitments.source_capture_commitment_sha256
        || current.duel_perception_profile_commitment_sha256
            != next_commitments.duel_perception_profile_commitment_sha256
        || current.duel_perception_profile_admission_commitment_sha256
            != next_commitments.duel_perception_profile_admission_commitment_sha256
        || current.classifier_runtime_commitment_sha256
            != next_commitments.classifier_runtime_commitment_sha256
        || current.pregame_evaluation_commitment_sha256
            != next_commitments.pregame_evaluation_commitment_sha256
        || current.pregame_profile_admission_commitment_sha256
            != next_commitments.pregame_profile_admission_commitment_sha256
        || current.process_continuity_commitment_sha256
            != next_commitments.process_continuity_commitment_sha256
        || current.window_continuity_commitment_sha256
            != next_commitments.window_continuity_commitment_sha256
        || current.approved_account_alias_sha256 != next_commitments.approved_account_alias_sha256
        || current.entry_authorization_sha256 != next_commitments.entry_authorization_sha256
        || current.event_identity_sha256 != next_commitments.event_identity_sha256
        || current.match_identity_sha256 != next_commitments.match_identity_sha256
        || current.event_kind != next_commitments.event_kind
        || current.game_number != next_commitments.game_number
        || current.frame_id == next_commitments.frame_id
        || next_commitments.frame_sequence <= current.frame_sequence
        || next_commitments.captured_at_unix_millis < current.captured_at_unix_millis
        || next_commitments.frame_sequence
            > session
                .match_launch
                .authorization
                .valid_through_frame_sequence
    {
        return Err(
            "competitive pregame observation changed exact account, event, match, game, profile, runtime, process, window, authorization, or frame order"
                .to_owned(),
        );
    }
    validate_competitive_pregame_stage_transition_v1(current.stage, next_commitments.stage)?;
    session.ordered_confirmed_bottom_slots = next_competitive_pregame_bottom_history_v1(
        current.stage,
        next_commitments.stage,
        &session.ordered_confirmed_bottom_slots,
        confirmed_bottom_slot,
    )?;
    let prior_session_commitment_sha256 = session.commitments.session_commitment_sha256.clone();
    session.commitments.current_observation_commitment_sha256 =
        next_commitments.observation_commitment_sha256.clone();
    session.current_model_context = None;
    session
        .commitments
        .current_model_context_binding_commitment_sha256 = None;
    session
        .commitments
        .confirmed_bottom_history_commitment_sha256 =
        competitive_pregame_confirmed_bottom_history_commitment_v1(
            &session.ordered_confirmed_bottom_slots,
        );
    session.commitments.current_stage = next_commitments.stage;
    session.commitments.current_frame_sequence = next_commitments.frame_sequence;
    session.commitments.visible_transition_count = session
        .commitments
        .visible_transition_count
        .checked_add(1)
        .ok_or("competitive pregame visible transition count overflow")?;
    session.commitments.prior_session_commitment_sha256 =
        Some(prior_session_commitment_sha256.clone());
    session.commitments.session_commitment_sha256 =
        competitive_event_pregame_session_commitment_v1(
            COMPETITIVE_EVENT_PREGAME_ADVANCE_DOMAIN_V1,
            Some(prior_session_commitment_sha256.as_str()),
            &session.commitments,
        )?;
    session.current_observation = next;
    Ok(session)
}

/// Releases the event coordinator and exact attended match launch only after
/// the opaque evaluated pregame chain reaches GameplayReady. No action or
/// input authority is created here.
pub fn complete_competitive_event_pregame_session_v1(
    session: OpaqueMtgoCompetitiveEventPregameSessionV1,
) -> Result<
    (
        OpaqueMtgoCompetitiveEventRuntimeV1,
        RatifiedMtgoCompetitiveMatchLaunchV1,
    ),
    String,
> {
    validate_competitive_event_pregame_session_integrity_v1(&session)?;
    if session.commitments.current_stage != MtgoCompetitivePregameStageV1::GameplayReady {
        return Err(
            "competitive event pregame session cannot release before GameplayReady".to_owned(),
        );
    }
    let completion_receipt_sha256 = hash_parts_v2(
        COMPETITIVE_EVENT_PREGAME_COMPLETION_DOMAIN_V1,
        &[
            session.commitments.session_commitment_sha256.as_bytes(),
            session
                .commitments
                .current_observation_commitment_sha256
                .as_bytes(),
            session
                .commitments
                .event_runtime_commitment_sha256
                .as_bytes(),
            session
                .commitments
                .match_launch_authorization_commitment_sha256
                .as_bytes(),
            session.commitments.event_identity_sha256.as_bytes(),
            session.commitments.match_identity_sha256.as_bytes(),
            &[session.commitments.game_number],
            session
                .commitments
                .current_frame_sequence
                .to_be_bytes()
                .as_slice(),
            b"evaluated_visible_gameplay_ready_event_runtime_and_exact_launch_released",
        ],
    );
    let completed = MtgoCompletedCompetitivePregameCommitmentsV1 {
        completion_receipt_sha256: completion_receipt_sha256.clone(),
        pregame_session_commitment_sha256: session.commitments.session_commitment_sha256.clone(),
        final_observation_commitment_sha256: session
            .commitments
            .current_observation_commitment_sha256
            .clone(),
        match_identity_sha256: session.commitments.match_identity_sha256.clone(),
        game_number: session.commitments.game_number,
        completion_frame_sequence: session.commitments.current_frame_sequence,
    };
    let mut runtime = session.runtime;
    let prior_runtime_commitment_sha256 = runtime.commitments.runtime_commitment_sha256.clone();
    runtime.commitments.pregame_session_count = runtime
        .commitments
        .pregame_session_count
        .checked_add(1)
        .ok_or("competitive event pregame session count overflow")?;
    runtime.commitments.last_completed_pregame = Some(completed);
    runtime.commitments.runtime_commitment_sha256 = competitive_event_runtime_commitment_v1(
        COMPETITIVE_EVENT_PREGAME_COMPLETION_DOMAIN_V1,
        Some(prior_runtime_commitment_sha256.as_str()),
        &runtime.commitments,
        completion_receipt_sha256.as_bytes(),
    );
    Ok((runtime, session.match_launch))
}

/// Withholds the event coordinator while the existing all-family exact-game
/// session is used. The lease and the returned game session must later be
/// reunited before any lifecycle or result transition can continue.
pub fn checkout_competitive_event_gameplay_session_v1(
    runtime: OpaqueMtgoCompetitiveEventRuntimeV1,
    mut session: OpaqueMtgoCompetitiveGestureGameSessionV1,
) -> Result<
    (
        OpaqueMtgoCompetitiveEventGameplayLeaseV1,
        OpaqueMtgoCompetitiveGestureGameSessionV1,
    ),
    String,
> {
    validate_game_session_against_event_runtime_v1(&runtime, &session)?;
    let unbound = session.commitments_v1();
    let deck_bound_session_commitment_sha256 =
        competitive_gesture_game_session_event_deck_binding_commitment_v1(
            &unbound,
            &runtime.commitments,
        )?;
    session.entry_ratification_commitment_sha256 = Some(
        runtime
            .commitments
            .entry_ratification_commitment_sha256
            .clone(),
    );
    session.selected_deck_label_sha256 =
        Some(runtime.commitments.selected_deck_label_sha256.clone());
    session.selected_deck_region_sha256 =
        Some(runtime.commitments.selected_deck_region_sha256.clone());
    session.deck_manifest_sha256 = Some(runtime.commitments.deck_manifest_sha256.clone());
    session.deck_format_sha256 = Some(runtime.commitments.deck_format_sha256.clone());
    session.policy_deployment_commitment_sha256 = Some(
        runtime
            .commitments
            .policy_deployment_commitment_sha256
            .clone(),
    );
    session.session_commitment_sha256 = deck_bound_session_commitment_sha256;
    let game = session.commitments_v1();
    let match_identity_sha256 = runtime
        .commitments
        .current_match_identity_sha256
        .clone()
        .ok_or("gameplay checkout requires an exact current match identity")?;
    let pregame_completion_frame_sequence = runtime
        .commitments
        .last_completed_pregame
        .as_ref()
        .ok_or("gameplay checkout requires the exact completed pregame")?
        .completion_frame_sequence;
    let gameplay_lease_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_EVENT_GAMEPLAY_LEASE_DOMAIN_V1,
        &[
            runtime.commitments.runtime_commitment_sha256.as_bytes(),
            game.session_commitment_sha256.as_bytes(),
            runtime.commitments.bound_event_identity_sha256.as_bytes(),
            match_identity_sha256.as_bytes(),
            runtime
                .commitments
                .entry_ratification_commitment_sha256
                .as_bytes(),
            runtime.commitments.selected_deck_label_sha256.as_bytes(),
            runtime.commitments.selected_deck_region_sha256.as_bytes(),
            runtime.commitments.deck_manifest_sha256.as_bytes(),
            runtime.commitments.deck_format_sha256.as_bytes(),
            runtime
                .commitments
                .policy_deployment_commitment_sha256
                .as_bytes(),
            game.game_number.to_be_bytes().as_slice(),
            pregame_completion_frame_sequence.to_be_bytes().as_slice(),
            game.confirmed_action_count.to_be_bytes().as_slice(),
            b"move_only_gameplay_lease_event_runtime_withheld",
        ],
    );
    let commitments = MtgoCompetitiveEventGameplayLeaseCommitmentsV1 {
        event_runtime_commitment_sha256: runtime.commitments.runtime_commitment_sha256.clone(),
        initial_game_session_commitment_sha256: game.session_commitment_sha256,
        gameplay_lease_commitment_sha256,
        event_kind: runtime.commitments.event_kind,
        event_identity_sha256: runtime.commitments.bound_event_identity_sha256.clone(),
        match_identity_sha256,
        entry_ratification_commitment_sha256: runtime
            .commitments
            .entry_ratification_commitment_sha256
            .clone(),
        selected_deck_label_sha256: runtime.commitments.selected_deck_label_sha256.clone(),
        selected_deck_region_sha256: runtime.commitments.selected_deck_region_sha256.clone(),
        deck_manifest_sha256: runtime.commitments.deck_manifest_sha256.clone(),
        deck_format_sha256: runtime.commitments.deck_format_sha256.clone(),
        policy_deployment_commitment_sha256: runtime
            .commitments
            .policy_deployment_commitment_sha256
            .clone(),
        game_number: game.game_number,
        checkout_frame_sequence: pregame_completion_frame_sequence,
        initial_confirmed_action_count: game.confirmed_action_count,
    };
    Ok((
        OpaqueMtgoCompetitiveEventGameplayLeaseV1 {
            runtime,
            commitments,
        },
        session,
    ))
}

pub fn return_competitive_event_gameplay_session_v1(
    lease: OpaqueMtgoCompetitiveEventGameplayLeaseV1,
    session: OpaqueMtgoCompetitiveGestureGameSessionV1,
) -> Result<OpaqueMtgoCompetitiveEventRuntimeV1, String> {
    validate_game_session_against_event_runtime_v1(&lease.runtime, &session)?;
    let game = session.commitments_v1();
    if lease.commitments.event_runtime_commitment_sha256
        != lease.runtime.commitments.runtime_commitment_sha256
        || lease.commitments.event_kind != game.event_kind
        || lease.commitments.game_number != game.game_number
        || game.entry_ratification_commitment_sha256.as_deref()
            != Some(
                lease
                    .commitments
                    .entry_ratification_commitment_sha256
                    .as_str(),
            )
        || game.selected_deck_label_sha256.as_deref()
            != Some(lease.commitments.selected_deck_label_sha256.as_str())
        || game.selected_deck_region_sha256.as_deref()
            != Some(lease.commitments.selected_deck_region_sha256.as_str())
        || game.deck_manifest_sha256.as_deref()
            != Some(lease.commitments.deck_manifest_sha256.as_str())
        || game.deck_format_sha256.as_deref() != Some(lease.commitments.deck_format_sha256.as_str())
        || game.policy_deployment_commitment_sha256.as_deref()
            != Some(
                lease
                    .commitments
                    .policy_deployment_commitment_sha256
                    .as_str(),
            )
        || game.confirmed_action_count < lease.commitments.initial_confirmed_action_count
        || game.last_confirmed_frame_sequence < lease.commitments.checkout_frame_sequence
        || game.last_confirmed_frame_sequence > game.valid_through_frame_sequence
        || lease
            .runtime
            .commitments
            .last_returned_gameplay_frame_sequence
            .is_some_and(|prior| game.last_confirmed_frame_sequence < prior)
    {
        return Err("returned gameplay session differs from its exact event lease".to_owned());
    }
    let return_receipt_sha256 = hash_parts_v2(
        COMPETITIVE_EVENT_GAMEPLAY_RETURN_DOMAIN_V1,
        &[
            lease
                .commitments
                .gameplay_lease_commitment_sha256
                .as_bytes(),
            lease
                .commitments
                .initial_game_session_commitment_sha256
                .as_bytes(),
            game.session_commitment_sha256.as_bytes(),
            lease
                .commitments
                .entry_ratification_commitment_sha256
                .as_bytes(),
            lease.commitments.selected_deck_label_sha256.as_bytes(),
            lease.commitments.selected_deck_region_sha256.as_bytes(),
            lease.commitments.deck_manifest_sha256.as_bytes(),
            lease.commitments.deck_format_sha256.as_bytes(),
            lease
                .commitments
                .policy_deployment_commitment_sha256
                .as_bytes(),
            game.last_confirmed_frame_sequence.to_be_bytes().as_slice(),
            game.confirmed_action_count.to_be_bytes().as_slice(),
            b"same_exact_game_session_returned_event_runtime_released",
        ],
    );
    let mut runtime = lease.runtime;
    runtime.commitments.gameplay_lease_count = runtime
        .commitments
        .gameplay_lease_count
        .checked_add(1)
        .ok_or("competitive event gameplay lease count overflow")?;
    runtime.commitments.last_returned_gameplay_frame_sequence =
        Some(game.last_confirmed_frame_sequence);
    runtime.commitments.runtime_commitment_sha256 = competitive_event_runtime_commitment_v1(
        COMPETITIVE_EVENT_GAMEPLAY_RETURN_DOMAIN_V1,
        Some(lease.commitments.event_runtime_commitment_sha256.as_str()),
        &runtime.commitments,
        return_receipt_sha256.as_bytes(),
    );
    Ok(runtime)
}

/// Reacquires and reclassifies the exact owner-reviewed League or Challenge
/// entry immediately before its exact actuation boundary. This consumes the
/// separately ratified authorization and retains both it and the fresh opaque
/// frame. The result exposes no coordinates or general input capability and is
/// accepted only by the one-click, visible-confirmation-locked entry executor.
///
/// Current production builds cannot reach this function successfully because
/// both the navigation-profile and exact-entry ratification roots are empty.
pub fn prepare_ratified_competitive_entry_v1(
    authorization: RatifiedMtgoCompetitiveEntryAuthorizationV1,
    profile: &AdmittedMtgoCompetitiveNavigationProfileV1,
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    immediate_identity: MtgoCompetitiveNavigationFrameIdentityV1,
    capture_timeout_ms: u32,
    classifier_timeout_ms: u32,
) -> Result<OpaqueMtgoPreparedCompetitiveEntryV1, String> {
    let immediate_frame =
        capture_admitted_mtgo_competitive_navigation_frame_v1(profile, capture_timeout_ms)?;
    let immediate = classify_admitted_mtgo_competitive_navigation_frame_v1(
        immediate_frame,
        profile,
        runtime,
        immediate_identity,
        classifier_timeout_ms,
    )?;
    let recapture = {
        let review = &authorization._review;
        let source = &review
            ._classifier_bound_review
            ._source_bound_review
            ._source_identity;
        validate_classifier_backed_competitive_entry_immediate_recapture_v1(
            source,
            &review._control_rect_client_px,
            &review._selected_deck_rect_client_px,
            &review.commitments.entry_control_dry_run,
            &immediate,
        )?
    };
    let pointer_target = resolve_competitive_entry_pointer_target_v1(
        &immediate,
        &authorization._review._control_rect_client_px,
    )?;
    let commitments =
        competitive_entry_preparation_from_commitments_v1(&authorization.commitments, &recapture)?;
    Ok(OpaqueMtgoPreparedCompetitiveEntryV1 {
        _authorization: authorization,
        _immediate_frame: immediate,
        pointer_target,
        commitments,
    })
}

/// Emits exactly one left click for the exact freshly prepared entry and then
/// locks the shared process gate until its visible postcondition is confirmed.
/// Production builds cannot construct the prerequisite ratified entry or
/// admitted navigation profile because both trust roots remain empty.
pub fn execute_prepared_competitive_entry_v1(
    prepared: OpaqueMtgoPreparedCompetitiveEntryV1,
) -> Result<OpaqueMtgoPendingCompetitiveEntryV1, String> {
    reserve_input_gate_v3()?;
    let input_sent_at_unix_millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(error) => {
            release_unattempted_reservation_v3()?;
            return Err(format!("system clock is before epoch: {error}"));
        }
    };
    let prepared_commitments = prepared.commitments_v1();
    if validate_preinput_capture_freshness_v3(
        prepared_commitments
            .immediate_recapture
            .immediate_captured_at_unix_millis,
        input_sent_at_unix_millis,
    )
    .is_err()
    {
        release_unattempted_reservation_v3()?;
        return Err(
            "the immediate competitive entry capture is stale or from the future".to_owned(),
        );
    }
    halt_before_input_attempt_v3()?;
    let cursor_parked_outside_client = send_exactly_one_left_click_v3(&prepared)?;
    let input_receipt_sha256 = competitive_entry_input_receipt_v1(
        &prepared,
        input_sent_at_unix_millis,
        cursor_parked_outside_client,
    );
    set_pending_v3(&input_receipt_sha256)?;
    Ok(OpaqueMtgoPendingCompetitiveEntryV1 {
        prepared,
        commitments: MtgoCompetitiveEntryInputReceiptCommitmentsV1 {
            preparation_commitment_sha256: prepared_commitments.preparation_commitment_sha256,
            entry_ratification_commitment_sha256: prepared_commitments
                .entry_ratification_commitment_sha256,
            input_receipt_sha256,
            event_kind: prepared_commitments.event_kind,
            resource: prepared_commitments.resource,
            amount: prepared_commitments.amount,
            immediate_frame_id: prepared_commitments.immediate_frame_id,
            immediate_frame_sequence: prepared_commitments.immediate_frame_sequence,
            input_sent_at_unix_millis,
            cursor_parked_outside_client,
        },
    })
}

pub fn confirm_pending_competitive_entry_v1(
    pending: OpaqueMtgoPendingCompetitiveEntryV1,
    profile: &AdmittedMtgoCompetitiveNavigationProfileV1,
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoConfirmedCompetitiveEntryV1, String> {
    require_matching_pending_v3(&pending.commitments.input_receipt_sha256)?;
    let OpaqueMtgoPendingCompetitiveEntryV1 {
        prepared,
        commitments: input,
    } = pending;
    let OpaqueMtgoPreparedCompetitiveEntryV1 {
        _authorization: authorization,
        _immediate_frame: immediate_frame,
        pointer_target: _,
        commitments: prepared_commitments,
    } = prepared;
    let source = &authorization
        ._review
        ._classifier_bound_review
        ._source_bound_review
        ._source_identity;
    let source_identity_commitment_sha256 =
        source.commitments_v1().source_identity_commitment_sha256;
    let event_label_rect_client_px = source.event_label_rect_client_px_v1().clone();
    let control_rect_client_px = authorization._review._control_rect_client_px.clone();
    let visible_confirmation = match confirm_opaque_competitive_entry_postcondition_v1(
        immediate_frame,
        source_identity_commitment_sha256,
        event_label_rect_client_px,
        control_rect_client_px,
        prepared_commitments
            .immediate_recapture
            .event_label_region_sha256
            .clone(),
        prepared_commitments
            .immediate_recapture
            .visible_control_region_sha256
            .clone(),
        prepared_commitments.preparation_commitment_sha256.clone(),
        profile,
        runtime,
        input.input_sent_at_unix_millis,
        timeout_ms,
    ) {
        Ok(confirmation) => confirmation,
        Err(error) => {
            halt_gate_v3()?;
            return Err(format!(
                "competitive entry postcondition failed and the input gate is halted: {error}"
            ));
        }
    };
    let visible = visible_confirmation.commitments_v1();
    if visible.frame_transition.event_kind != prepared_commitments.event_kind
        || visible.frame_transition.event_identity_sha256
            != prepared_commitments
                .immediate_recapture
                .event_identity_sha256
        || visible.frame_transition.before_frame_id != prepared_commitments.immediate_frame_id
        || visible.frame_transition.before_frame_sequence
            != prepared_commitments.immediate_frame_sequence
        || visible.frame_transition.after_frame_sequence
            <= prepared_commitments.immediate_frame_sequence
        || visible.after_captured_at_unix_millis <= input.input_sent_at_unix_millis
    {
        halt_gate_v3()?;
        return Err(
            "competitive entry confirmation changed the exact event or frame and halted the input gate"
                .to_owned(),
        );
    }
    let confirmation_receipt_sha256 =
        competitive_entry_confirmation_receipt_v1(&input, &prepared_commitments, &visible);
    release_confirmed_pending_v3(&input.input_receipt_sha256)?;
    Ok(OpaqueMtgoConfirmedCompetitiveEntryV1 {
        _authorization: authorization,
        _visible_confirmation: visible_confirmation,
        commitments: MtgoConfirmedCompetitiveEntryCommitmentsV1 {
            input_receipt_sha256: input.input_receipt_sha256,
            preparation_commitment_sha256: prepared_commitments.preparation_commitment_sha256,
            entry_ratification_commitment_sha256: prepared_commitments
                .entry_ratification_commitment_sha256,
            visible_transition_commitment_sha256: visible
                .frame_transition
                .transition_commitment_sha256,
            visible_confirmation_commitment_sha256: visible.confirmation_commitment_sha256,
            confirmation_receipt_sha256,
            event_kind: prepared_commitments.event_kind,
            resource: prepared_commitments.resource,
            amount: prepared_commitments.amount,
            after_frame_id: visible.frame_transition.after_frame_id,
            after_frame_sequence: visible.frame_transition.after_frame_sequence,
            after_captured_at_unix_millis: visible.after_captured_at_unix_millis,
            postcondition_candidate_count: visible.postcondition_candidate_count,
        },
    })
}

/// Performs a terminal-attended review of one exact visible League or
/// Challenge entry and its exact existing-account resource terms. This is a
/// non-authorizing review boundary. It neither enters the event nor enables a
/// later input path.
pub fn review_competitive_entry_attended_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    source: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    visible_account_alias: &str,
    event_display_label: String,
) -> Result<CheckedUntrustedMtgoAttendedCompetitiveEntryReviewV1, String> {
    let request = build_attended_competitive_entry_review_request_v1(
        correspondence,
        &source,
        visible_account_alias,
        &event_display_label,
    )?;
    let (challenge_nonce, issued_at_unix_millis, supplied_phrase) =
        prompt_attended_competitive_entry_review_v1(
            &request,
            visible_account_alias,
            "checked visible lifecycle facts",
        )?;
    review_competitive_entry_from_attended_confirmation_v1(
        correspondence,
        source,
        visible_account_alias,
        event_display_label,
        challenge_nonce,
        issued_at_unix_millis,
        &supplied_phrase,
    )
}

/// Source-bound manual entry-review path. It consumes an identity that has already
/// rehashed every lifecycle fact against one retained opaque composed-desktop
/// navigation frame. The result remains non-actionable and cannot click Join
/// or spend resources.
pub fn review_competitive_entry_attended_v2(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    source_identity: OpaqueMtgoCompetitiveEntryReviewIdentityV1,
    visible_account_alias: &str,
) -> Result<CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2, String> {
    let source_commitments = source_identity.commitments_v1();
    let source_description = if source_commitments
        .source_navigation_classification_result_commitment_sha256
        .is_some()
    {
        "exact profile-pinned classifier over opaque composed-desktop navigation pixels"
    } else {
        "opaque composed-desktop navigation pixels"
    };
    review_competitive_entry_attended_v2_with_source_description(
        correspondence,
        source_identity,
        visible_account_alias,
        source_description,
    )
}

fn review_competitive_entry_attended_v2_with_source_description(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    source_identity: OpaqueMtgoCompetitiveEntryReviewIdentityV1,
    visible_account_alias: &str,
    source_description: &str,
) -> Result<CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2, String> {
    let source_commitments = source_identity.commitments_v1();
    let request = build_attended_competitive_entry_review_request_v1(
        correspondence,
        source_identity.lifecycle_v1(),
        visible_account_alias,
        source_identity.event_display_label_v1(),
    )?;
    if request.source_lifecycle_snapshot_commitment_sha256
        != source_commitments.source_lifecycle_snapshot_commitment_sha256
        || request.event_kind != source_commitments.event_kind
        || request.frame_id != source_commitments.frame_id
        || request.frame_sequence != source_commitments.frame_sequence
        || request.entry_terms.resource != source_commitments.resource
        || request.entry_terms.amount != source_commitments.amount
    {
        return Err(
            "source-bound entry review identity changed before owner confirmation".to_owned(),
        );
    }
    let (challenge_nonce, issued_at_unix_millis, supplied_phrase) =
        prompt_attended_competitive_entry_review_v1(
            &request,
            visible_account_alias,
            source_description,
        )?;
    let parts = make_attended_competitive_entry_review_parts_v1(
        correspondence,
        source_identity.lifecycle_v1(),
        visible_account_alias,
        source_identity.event_display_label_v1(),
        challenge_nonce,
        issued_at_unix_millis,
        &supplied_phrase,
    )?;
    Ok(CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2 {
        _source_identity: source_identity,
        entry_authorization: parts.entry_authorization,
        commitments: MtgoSourceBoundCompetitiveEntryReviewCommitmentsV2 {
            source_identity_commitment_sha256: source_commitments.source_identity_commitment_sha256,
            source_capture_commitment_sha256: source_commitments.source_capture_commitment_sha256,
            source_navigation_classification_result_commitment_sha256: source_commitments
                .source_navigation_classification_result_commitment_sha256,
            attended_review: parts.commitments,
        },
    })
}

/// Preferred classifier-bound entry-review path. The consumed source must
/// retain the exact profile-pinned classifier result that produced its visible
/// lifecycle interpretation. Success still cannot click Join, spend resources,
/// or enable input.
pub fn review_competitive_entry_attended_v3(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    source_identity: OpaqueMtgoCompetitiveEntryReviewIdentityV1,
    visible_account_alias: &str,
) -> Result<CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3, String> {
    review_competitive_entry_attended_v3_with_source_description(
        correspondence,
        source_identity,
        visible_account_alias,
        "exact profile-pinned classifier over opaque composed-desktop navigation pixels",
    )
}

fn review_competitive_entry_attended_v3_with_source_description(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    source_identity: OpaqueMtgoCompetitiveEntryReviewIdentityV1,
    visible_account_alias: &str,
    source_description: &str,
) -> Result<CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3, String> {
    let source_commitments = source_identity.commitments_v1();
    let classifier_commitment =
        require_classifier_bound_competitive_entry_source_v3(&source_commitments)?.to_owned();
    let source_bound_review = review_competitive_entry_attended_v2_with_source_description(
        correspondence,
        source_identity,
        visible_account_alias,
        source_description,
    )?;
    let source_bound_commitments = source_bound_review.commitments_v2();
    if source_bound_commitments
        .source_navigation_classification_result_commitment_sha256
        .as_deref()
        != Some(classifier_commitment.as_str())
    {
        return Err(
            "classifier-bound entry review lineage changed during owner confirmation".to_owned(),
        );
    }
    let classifier_bound_review_commitment_sha256 =
        classifier_bound_competitive_entry_review_commitment_v3(
            &source_bound_commitments,
            &classifier_commitment,
        );
    let entry_authorization = source_bound_review.entry_authorization_record_v2();
    Ok(
        CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3 {
            _source_bound_review: source_bound_review,
            entry_authorization,
            commitments: MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3 {
                source_navigation_classification_result_commitment_sha256: classifier_commitment,
                classifier_bound_review_commitment_sha256,
                source_bound_review: source_bound_commitments,
            },
        },
    )
}

/// Preferred owner-attended competitive entry dry run. It consumes a visible,
/// explicitly enabled Confirm Entry control bound to the same exact opaque
/// classifier-backed frame as the event and terms review. Success records the
/// review but cannot click Join, spend resources, or enable input.
pub fn review_competitive_entry_attended_v4(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    dry_run: OpaqueMtgoCompetitiveEntryControlDryRunV1,
    visible_account_alias: &str,
) -> Result<CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4, String> {
    let MtgoCompetitiveEntryControlDryRunPartsV1 {
        source_identity,
        visible_control_label,
        control_rect_client_px,
        selected_deck_label,
        selected_deck_rect_client_px,
        commitments: entry_control_dry_run,
    } = dry_run.into_parts_v1();
    let source_description = format!(
        "exact profile-pinned classifier plus human-reviewed visibly enabled control {visible_control_label:?} and selected deck {selected_deck_label:?} over opaque composed-desktop navigation pixels"
    );
    let classifier_bound_review = review_competitive_entry_attended_v3_with_source_description(
        correspondence,
        source_identity,
        visible_account_alias,
        &source_description,
    )?;
    let classifier_bound_commitments = classifier_bound_review.commitments_v3();
    let deck_review_receipt_sha256 = prompt_attended_competitive_deck_selection_review_v1(
        visible_account_alias,
        &selected_deck_label,
        &entry_control_dry_run,
        &classifier_bound_commitments,
    )?;
    let control_bound_review_commitment_sha256 =
        bind_control_bound_competitive_entry_review_commitment_v4(
            &classifier_bound_commitments,
            &entry_control_dry_run,
            &deck_review_receipt_sha256,
        )?;
    let entry_authorization = classifier_bound_review.entry_authorization_record_v3();
    Ok(CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4 {
        _classifier_bound_review: classifier_bound_review,
        _visible_control_label: visible_control_label,
        _control_rect_client_px: control_rect_client_px,
        _selected_deck_label: selected_deck_label,
        _selected_deck_rect_client_px: selected_deck_rect_client_px,
        entry_authorization,
        commitments: MtgoControlBoundCompetitiveEntryReviewCommitmentsV4 {
            control_bound_review_commitment_sha256,
            deck_review_receipt_sha256,
            classifier_bound_review: classifier_bound_commitments,
            entry_control_dry_run,
        },
    })
}

/// Pairs the exact owner-attended, control-bound entry review with one strictly
/// newer classifier-backed entered-waiting frame. This records the visible
/// postcondition shape only. It does not create or execute ConfirmEntry.
pub fn bind_competitive_entry_postcondition_dry_run_v1(
    source_review: CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4,
    entered_waiting_frame: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
) -> Result<CheckedUntrustedMtgoCompetitiveEntryPostconditionDryRunV1, String> {
    let frame_transition = validate_classifier_backed_competitive_entry_frame_transition_v1(
        &source_review
            ._classifier_bound_review
            ._source_bound_review
            ._source_identity,
        &entered_waiting_frame,
    )?;
    let entry_event_identity_sha256 = source_review
        .entry_authorization
        .event_identity_sha256
        .clone();
    let review = source_review.commitments_v4();
    let source_bound = &review.classifier_bound_review.source_bound_review;
    let attended = &source_bound.attended_review;
    if review
        .entry_control_dry_run
        .source_identity_commitment_sha256
        != frame_transition.source_identity_commitment_sha256
        || review
            .entry_control_dry_run
            .source_capture_commitment_sha256
            != frame_transition.before_capture_commitment_sha256
        || review
            .entry_control_dry_run
            .source_navigation_classification_result_commitment_sha256
            != frame_transition.before_classification_result_commitment_sha256
        || review
            .entry_control_dry_run
            .source_lifecycle_snapshot_commitment_sha256
            != frame_transition.before_lifecycle_snapshot_commitment_sha256
        || review
            .classifier_bound_review
            .source_navigation_classification_result_commitment_sha256
            != frame_transition.before_classification_result_commitment_sha256
        || source_bound.source_identity_commitment_sha256
            != frame_transition.source_identity_commitment_sha256
        || source_bound.source_capture_commitment_sha256
            != frame_transition.before_capture_commitment_sha256
        || attended.source_lifecycle_snapshot_commitment_sha256
            != frame_transition.before_lifecycle_snapshot_commitment_sha256
        || attended.event_kind != frame_transition.event_kind
        || attended.frame_id != frame_transition.before_frame_id
        || attended.frame_sequence != frame_transition.before_frame_sequence
        || entry_event_identity_sha256 != frame_transition.event_identity_sha256
        || attended.resource != review.entry_control_dry_run.resource
        || attended.amount != review.entry_control_dry_run.amount
    {
        return Err(
            "competitive entry postcondition dry run changed the exact review, control, classifier, frame, event, or terms"
                .to_owned(),
        );
    }
    for digest in [
        review.control_bound_review_commitment_sha256.as_str(),
        attended.entry_authorization_sha256.as_str(),
        frame_transition.transition_commitment_sha256.as_str(),
    ] {
        if !is_sha256_v2(digest) {
            return Err(
                "competitive entry postcondition dry run contains an invalid commitment".to_owned(),
            );
        }
    }
    let event_kind: &[u8] = match attended.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let resource: &[u8] = match attended.resource {
        MtgoCompetitiveEntryResourceV1::NoCost => b"no_cost",
        MtgoCompetitiveEntryResourceV1::ExistingPlayPoints => b"existing_play_points",
        MtgoCompetitiveEntryResourceV1::ExistingEventTickets => b"existing_event_tickets",
        MtgoCompetitiveEntryResourceV1::ExistingEventToken => b"existing_event_token",
    };
    let postcondition_dry_run_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_ENTRY_POSTCONDITION_DRY_RUN_DOMAIN_V1,
        &[
            review.control_bound_review_commitment_sha256.as_bytes(),
            attended.entry_authorization_sha256.as_bytes(),
            frame_transition.transition_commitment_sha256.as_bytes(),
            frame_transition
                .before_classification_result_commitment_sha256
                .as_bytes(),
            frame_transition
                .after_classification_result_commitment_sha256
                .as_bytes(),
            event_kind,
            entry_event_identity_sha256.as_bytes(),
            resource,
            attended.amount.to_be_bytes().as_slice(),
            b"visible_postcondition_calibration_pair_no_causality_no_join_no_spending_no_input",
        ],
    );
    let commitments = MtgoCompetitiveEntryPostconditionDryRunCommitmentsV1 {
        control_bound_review_commitment_sha256: review.control_bound_review_commitment_sha256,
        entry_authorization_sha256: attended.entry_authorization_sha256.clone(),
        frame_transition,
        postcondition_dry_run_commitment_sha256,
        event_kind: attended.event_kind,
        resource: attended.resource,
        amount: attended.amount,
    };
    Ok(CheckedUntrustedMtgoCompetitiveEntryPostconditionDryRunV1 {
        _source_review: source_review,
        _entered_waiting_frame: entered_waiting_frame,
        commitments,
    })
}

fn require_classifier_bound_competitive_entry_source_v3(
    source: &MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1,
) -> Result<&str, String> {
    let classifier_commitment = source
        .source_navigation_classification_result_commitment_sha256
        .as_deref()
        .ok_or(
            "classifier-bound entry review requires an exact retained navigation classifier result",
        )?;
    if !is_sha256_v2(classifier_commitment) {
        return Err(
            "classifier-bound entry review contains an invalid classifier commitment".to_owned(),
        );
    }
    Ok(classifier_commitment)
}

fn classifier_bound_competitive_entry_review_commitment_v3(
    source_bound: &MtgoSourceBoundCompetitiveEntryReviewCommitmentsV2,
    classifier_commitment_sha256: &str,
) -> String {
    let event_kind: &[u8] = match source_bound.attended_review.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let resource: &[u8] = match source_bound.attended_review.resource {
        MtgoCompetitiveEntryResourceV1::NoCost => b"no_cost",
        MtgoCompetitiveEntryResourceV1::ExistingPlayPoints => b"existing_play_points",
        MtgoCompetitiveEntryResourceV1::ExistingEventTickets => b"existing_event_tickets",
        MtgoCompetitiveEntryResourceV1::ExistingEventToken => b"existing_event_token",
    };
    hash_parts_v2(
        CLASSIFIER_BOUND_COMPETITIVE_ENTRY_REVIEW_DOMAIN_V3,
        &[
            classifier_commitment_sha256.as_bytes(),
            source_bound.source_identity_commitment_sha256.as_bytes(),
            source_bound.source_capture_commitment_sha256.as_bytes(),
            source_bound
                .attended_review
                .source_lifecycle_snapshot_commitment_sha256
                .as_bytes(),
            source_bound
                .attended_review
                .permission_review_commitment_sha256
                .as_bytes(),
            source_bound
                .attended_review
                .mode_authorization_commitment_sha256
                .as_bytes(),
            source_bound
                .attended_review
                .request_commitment_sha256
                .as_bytes(),
            source_bound
                .attended_review
                .entry_authorization_sha256
                .as_bytes(),
            source_bound
                .attended_review
                .owner_review_receipt_sha256
                .as_bytes(),
            event_kind,
            source_bound
                .attended_review
                .frame_id
                .to_be_bytes()
                .as_slice(),
            source_bound
                .attended_review
                .frame_sequence
                .to_be_bytes()
                .as_slice(),
            resource,
            source_bound.attended_review.amount.to_be_bytes().as_slice(),
            b"classifier_bound_owner_review_no_entry_no_spending_no_input",
        ],
    )
}

fn bind_control_bound_competitive_entry_review_commitment_v4(
    classifier_bound: &MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3,
    dry_run: &MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1,
    deck_review_receipt_sha256: &str,
) -> Result<String, String> {
    let source_bound = &classifier_bound.source_bound_review;
    let attended = &source_bound.attended_review;
    if !dry_run.visibly_enabled_confirmed
        || classifier_bound.source_navigation_classification_result_commitment_sha256
            != dry_run.source_navigation_classification_result_commitment_sha256
        || source_bound
            .source_navigation_classification_result_commitment_sha256
            .as_deref()
            != Some(
                dry_run
                    .source_navigation_classification_result_commitment_sha256
                    .as_str(),
            )
        || source_bound.source_identity_commitment_sha256
            != dry_run.source_identity_commitment_sha256
        || source_bound.source_capture_commitment_sha256 != dry_run.source_capture_commitment_sha256
        || attended.source_lifecycle_snapshot_commitment_sha256
            != dry_run.source_lifecycle_snapshot_commitment_sha256
        || attended.event_kind != dry_run.event_kind
        || attended.frame_id != dry_run.frame_id
        || attended.frame_sequence != dry_run.frame_sequence
        || attended.resource != dry_run.resource
        || attended.amount != dry_run.amount
    {
        return Err(
            "control-bound competitive entry review does not match its exact classifier, frame, lifecycle, event, and terms"
                .to_owned(),
        );
    }
    for commitment in [
        classifier_bound
            .classifier_bound_review_commitment_sha256
            .as_str(),
        dry_run.dry_run_commitment_sha256.as_str(),
        dry_run.visible_control_label_sha256.as_str(),
        dry_run.visible_control_region_sha256.as_str(),
        dry_run.selected_deck_label_sha256.as_str(),
        dry_run.selected_deck_region_sha256.as_str(),
        dry_run.deck_manifest_sha256.as_str(),
        dry_run.deck_format_sha256.as_str(),
        dry_run.policy_deployment_commitment_sha256.as_str(),
        deck_review_receipt_sha256,
    ] {
        if !is_sha256_v2(commitment) {
            return Err(
                "control-bound competitive entry review contains an invalid commitment".to_owned(),
            );
        }
    }
    Ok(hash_parts_v2(
        CONTROL_BOUND_COMPETITIVE_ENTRY_REVIEW_DOMAIN_V4,
        &[
            classifier_bound
                .classifier_bound_review_commitment_sha256
                .as_bytes(),
            dry_run.dry_run_commitment_sha256.as_bytes(),
            dry_run.source_identity_commitment_sha256.as_bytes(),
            dry_run.source_capture_commitment_sha256.as_bytes(),
            dry_run
                .source_lifecycle_snapshot_commitment_sha256
                .as_bytes(),
            dry_run
                .source_navigation_classification_result_commitment_sha256
                .as_bytes(),
            dry_run.visible_control_label_sha256.as_bytes(),
            dry_run.visible_control_region_sha256.as_bytes(),
            dry_run.selected_deck_label_sha256.as_bytes(),
            dry_run.selected_deck_region_sha256.as_bytes(),
            dry_run.deck_manifest_sha256.as_bytes(),
            dry_run.deck_format_sha256.as_bytes(),
            dry_run.policy_deployment_commitment_sha256.as_bytes(),
            deck_review_receipt_sha256.as_bytes(),
            &[u8::from(dry_run.visibly_enabled_confirmed)],
            attended.owner_review_receipt_sha256.as_bytes(),
            attended.entry_authorization_sha256.as_bytes(),
            b"owner_attended_control_and_selected_deck_bound_entry_dry_run_no_join_no_spending_no_input",
        ],
    ))
}

fn prompt_attended_competitive_entry_review_v1(
    request: &MtgoAttendedCompetitiveEntryReviewRequestV1,
    visible_account_alias: &str,
    source_description: &str,
) -> Result<([u8; 8], u128, String), String> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    if !stdin.is_terminal() || !stdout.is_terminal() {
        return Err(
            "attended competitive entry review requires an interactive terminal".to_owned(),
        );
    }
    let mut challenge_nonce = [0_u8; 8];
    unsafe {
        BCryptGenRandom(None, &mut challenge_nonce, BCRYPT_USE_SYSTEM_PREFERRED_RNG)
            .ok()
            .map_err(|error| format!("generate attended entry review challenge: {error}"))?;
    }
    let issued_at_unix_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before epoch: {error}"))?
        .as_millis();
    let expected_phrase = attended_competitive_entry_review_confirmation_phrase_v1(
        request.event_kind,
        request.entry_terms.resource,
        request.entry_terms.amount,
        &challenge_nonce,
    );
    let event_prefix = &request.event_identity_sha256[..12];
    let terms_prefix = &request.entry_terms.terms_sha256[..12];
    writeln!(stdout, "MTGO attended competitive entry review")
        .map_err(|error| format!("write attended entry review prompt: {error}"))?;
    writeln!(stdout, "Account: {visible_account_alias}")
        .map_err(|error| format!("write attended entry review account: {error}"))?;
    writeln!(stdout, "Visible source: {source_description}")
        .map_err(|error| format!("write attended entry review source: {error}"))?;
    writeln!(
        stdout,
        "Mode: {}; event: {}; resource: {}; exact amount: {}; event id: {event_prefix}; terms id: {terms_prefix}",
        competitive_event_kind_label_v4(request.event_kind),
        request.event_display_label,
        competitive_entry_resource_label_v1(request.entry_terms.resource),
        request.entry_terms.amount,
    )
    .map_err(|error| format!("write attended entry review terms: {error}"))?;
    writeln!(
        stdout,
        "This records your review only. It does not enter the event, spend resources, or enable input."
    )
    .map_err(|error| format!("write attended entry review scope: {error}"))?;
    writeln!(stdout, "Type exactly: {expected_phrase}")
        .map_err(|error| format!("write attended entry review challenge: {error}"))?;
    stdout
        .flush()
        .map_err(|error| format!("flush attended entry review prompt: {error}"))?;
    let mut supplied_phrase = String::new();
    stdin
        .read_line(&mut supplied_phrase)
        .map_err(|error| format!("read attended entry review confirmation: {error}"))?;
    Ok((
        challenge_nonce,
        issued_at_unix_millis,
        supplied_phrase.trim_end_matches(['\r', '\n']).to_owned(),
    ))
}

fn prompt_attended_competitive_deck_selection_review_v1(
    visible_account_alias: &str,
    selected_deck_label: &str,
    dry_run: &MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1,
    classifier_bound: &MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3,
) -> Result<String, String> {
    validate_attended_launch_display_label_v4(selected_deck_label, 96, "selected deck label")?;
    let selected_deck_label_sha256 =
        format!("{:x}", Sha256::digest(selected_deck_label.as_bytes()));
    if selected_deck_label_sha256 != dry_run.selected_deck_label_sha256 {
        return Err("attended selected-deck label changed from its visible dry run".to_owned());
    }
    for value in [
        dry_run.dry_run_commitment_sha256.as_str(),
        dry_run.selected_deck_label_sha256.as_str(),
        dry_run.selected_deck_region_sha256.as_str(),
        dry_run.deck_manifest_sha256.as_str(),
        dry_run.deck_format_sha256.as_str(),
        dry_run.policy_deployment_commitment_sha256.as_str(),
        classifier_bound
            .classifier_bound_review_commitment_sha256
            .as_str(),
    ] {
        if !is_sha256_v2(value) {
            return Err("attended selected-deck review contains an invalid commitment".to_owned());
        }
    }
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    if !stdin.is_terminal() || !stdout.is_terminal() {
        return Err("attended selected-deck review requires an interactive terminal".to_owned());
    }
    let mut challenge_nonce = [0_u8; 8];
    unsafe {
        BCryptGenRandom(None, &mut challenge_nonce, BCRYPT_USE_SYSTEM_PREFERRED_RNG)
            .ok()
            .map_err(|error| format!("generate attended selected-deck challenge: {error}"))?;
    }
    let issued_at_unix_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before epoch: {error}"))?
        .as_millis();
    let expected_phrase =
        attended_competitive_deck_selection_confirmation_phrase_v1(&challenge_nonce);
    writeln!(stdout, "MTGO attended selected-deck review")
        .map_err(|error| format!("write attended selected-deck prompt: {error}"))?;
    writeln!(stdout, "Account: {visible_account_alias}")
        .map_err(|error| format!("write attended selected-deck account: {error}"))?;
    writeln!(stdout, "Visible selected deck: {selected_deck_label}")
        .map_err(|error| format!("write attended selected-deck label: {error}"))?;
    writeln!(
        stdout,
        "Deck manifest: {}; format: {}; policy deployment: {}; visible region: {}",
        &dry_run.deck_manifest_sha256[..12],
        &dry_run.deck_format_sha256[..12],
        &dry_run.policy_deployment_commitment_sha256[..12],
        &dry_run.selected_deck_region_sha256[..12],
    )
    .map_err(|error| format!("write attended selected-deck commitments: {error}"))?;
    writeln!(
        stdout,
        "This records the exact deck and policy deployment selected for the reviewed entry. It does not enter the event, spend resources, or enable input."
    )
    .map_err(|error| format!("write attended selected-deck scope: {error}"))?;
    writeln!(stdout, "Type exactly: {expected_phrase}")
        .map_err(|error| format!("write attended selected-deck challenge: {error}"))?;
    stdout
        .flush()
        .map_err(|error| format!("flush attended selected-deck prompt: {error}"))?;
    let mut supplied_phrase = String::new();
    stdin
        .read_line(&mut supplied_phrase)
        .map_err(|error| format!("read attended selected-deck confirmation: {error}"))?;
    let supplied_phrase = supplied_phrase.trim_end_matches(['\r', '\n']);
    if supplied_phrase != expected_phrase {
        return Err("attended selected-deck confirmation phrase did not match".to_owned());
    }
    let account_alias_sha256 = format!("{:x}", Sha256::digest(visible_account_alias.as_bytes()));
    Ok(hash_parts_v2(
        ATTENDED_COMPETITIVE_DECK_REVIEW_RECEIPT_DOMAIN_V1,
        &[
            classifier_bound
                .classifier_bound_review_commitment_sha256
                .as_bytes(),
            dry_run.dry_run_commitment_sha256.as_bytes(),
            account_alias_sha256.as_bytes(),
            selected_deck_label.as_bytes(),
            dry_run.selected_deck_label_sha256.as_bytes(),
            dry_run.selected_deck_region_sha256.as_bytes(),
            dry_run.deck_manifest_sha256.as_bytes(),
            dry_run.deck_format_sha256.as_bytes(),
            dry_run.policy_deployment_commitment_sha256.as_bytes(),
            &challenge_nonce,
            issued_at_unix_millis.to_be_bytes().as_slice(),
            supplied_phrase.as_bytes(),
            b"owner_confirmed_exact_visible_selected_deck_no_entry_no_spending_no_input",
        ],
    ))
}

#[cfg(test)]
fn ratify_competitive_match_launch_v1(
    scope: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
    authorization: MtgoCompetitiveMatchGameplayAuthorizationV1,
) -> Result<RatifiedMtgoCompetitiveMatchLaunchV1, String> {
    ratify_competitive_match_launch_with_commitment_v1(
        scope,
        visible_account_alias,
        authorization,
        RATIFIED_COMPETITIVE_MATCH_LAUNCH_AUTHORIZATION_COMMITMENT_V1,
    )
}

/// Requires a match identity already bound to the exact paid event runtime, a
/// real interactive terminal, and an exact owner-entered challenge before
/// creating one move-only League or Challenge game launch. Redirected
/// stdin/stdout is rejected. The same event runtime is returned only after a
/// successful review so it can be checked out into the resulting game session.
pub fn ratify_competitive_event_match_launch_attended_v5(
    binding: OpaqueMtgoCompetitiveEventMatchLaunchBindingV1,
    visible_account_alias: &str,
) -> Result<
    (
        OpaqueMtgoCompetitiveEventRuntimeV1,
        RatifiedMtgoCompetitiveMatchLaunchV1,
    ),
    String,
> {
    let (runtime, launch, _visible_identity) =
        ratify_competitive_event_match_launch_with_visible_identity_attended_v1(
            binding,
            visible_account_alias,
        )?;
    Ok((runtime, launch))
}

/// Operator-only attended review variant that also returns the exact visible
/// launch identity. The identity is required to bind the seated-player Game
/// Log and grants no input, event-entry, or spending authority.
pub(crate) fn ratify_competitive_event_match_launch_with_visible_identity_attended_v1(
    binding: OpaqueMtgoCompetitiveEventMatchLaunchBindingV1,
    visible_account_alias: &str,
) -> Result<
    (
        OpaqueMtgoCompetitiveEventRuntimeV1,
        RatifiedMtgoCompetitiveMatchLaunchV1,
        OpaqueMtgoCompetitiveLaunchIdentityV1,
    ),
    String,
> {
    let current_process_continuity_commitment_sha256 = binding
        .runtime
        .current_frame
        .process_continuity_commitment_sha256_v1();
    let current_frame = binding.runtime.current_frame.commitments_v1();
    let source = binding.visible_identity.commitments_v1();
    let recomputed = competitive_event_match_launch_binding_commitments_v1(
        &binding.runtime.commitments,
        &current_process_continuity_commitment_sha256,
        current_frame
            .source_frame
            .source_capture
            .captured_at_unix_millis,
        &source,
        binding.visible_identity.event_identity_sha256_v1(),
        binding.visible_identity.match_identity_sha256_v1(),
        binding.visible_identity.entry_authorization_sha256_v1(),
    )?;
    if recomputed != binding.commitments {
        return Err("competitive event match launch binding changed".to_owned());
    }
    let OpaqueMtgoCompetitiveEventMatchLaunchBindingV1 {
        runtime,
        visible_identity,
        commitments: _,
    } = binding;
    let scope = &runtime.lifecycle_authorization.scope;
    let source = visible_identity.commitments_v1();
    let request = MtgoAttendedCompetitiveMatchLaunchRequestV4 {
        schema_version: MTGO_ATTENDED_COMPETITIVE_MATCH_LAUNCH_REQUEST_SCHEMA_V4,
        event_kind: source.event_kind,
        event_display_label: visible_identity.event_display_label_v1().to_owned(),
        opponent_display_name: visible_identity.opponent_display_name_v1().to_owned(),
        visible_match_id: visible_identity.visible_match_id_v1().to_owned(),
        visible_game_id: visible_identity.visible_game_id_v1().to_owned(),
        event_identity_sha256: visible_identity.event_identity_sha256_v1().to_owned(),
        match_identity_sha256: visible_identity.match_identity_sha256_v1().to_owned(),
        game_number: source.game_number,
        entry_authorization_sha256: visible_identity.entry_authorization_sha256_v1().to_owned(),
        observed_frame_sequence: source.frame_sequence,
        source_capture_commitment_sha256: source.source_capture_commitment_sha256,
        source_perception_result_commitment_sha256: source.perception_result_commitment_sha256,
        source_lifecycle_snapshot_commitment_sha256: source.lifecycle_snapshot_commitment_sha256,
        source_window_title_sha256: source.window_title_sha256,
        source_event_label_region_sha256: source.event_label_region_sha256,
        source_launch_identity_commitment_sha256: source.launch_identity_commitment_sha256,
    };
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    if !stdin.is_terminal() || !stdout.is_terminal() {
        return Err(
            "attended competitive match launch requires an interactive terminal".to_owned(),
        );
    }
    let mut challenge_nonce = [0_u8; 8];
    unsafe {
        BCryptGenRandom(None, &mut challenge_nonce, BCRYPT_USE_SYSTEM_PREFERRED_RNG)
            .ok()
            .map_err(|error| format!("generate attended launch challenge: {error}"))?;
    }
    let issued_at_unix_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before epoch: {error}"))?
        .as_millis();
    validate_attended_competitive_match_launch_request_v4(scope, visible_account_alias, &request)?;
    let expected_phrase = attended_competitive_match_launch_confirmation_phrase_v4(
        request.event_kind,
        request.game_number,
        &challenge_nonce,
    );
    let mode_label = competitive_event_kind_label_v4(request.event_kind);
    let event_prefix = &request.event_identity_sha256[..12];
    let match_prefix = &request.match_identity_sha256[..12];
    writeln!(stdout, "MTGO attended competitive match launch")
        .map_err(|error| format!("write attended launch prompt: {error}"))?;
    writeln!(stdout, "Account: {visible_account_alias}")
        .map_err(|error| format!("write attended launch account: {error}"))?;
    writeln!(
        stdout,
        "Mode: {mode_label}; event: {}; opponent: {}; game: {}; visible match id: {}; visible game id: {}; event id: {event_prefix}; match id: {match_prefix}",
        request.event_display_label,
        request.opponent_display_name,
        request.game_number,
        request.visible_match_id,
        request.visible_game_id
    )
    .map_err(|error| format!("write attended launch identity: {error}"))?;
    writeln!(
        stdout,
        "This authorizes visible priority-Pass gameplay only for this exact already-entered game. It cannot enter an event or spend resources."
    )
    .map_err(|error| format!("write attended launch scope: {error}"))?;
    writeln!(stdout, "Type exactly: {expected_phrase}")
        .map_err(|error| format!("write attended launch challenge: {error}"))?;
    stdout
        .flush()
        .map_err(|error| format!("flush attended launch prompt: {error}"))?;

    let mut supplied_phrase = String::new();
    stdin
        .read_line(&mut supplied_phrase)
        .map_err(|error| format!("read attended launch confirmation: {error}"))?;
    let supplied_phrase = supplied_phrase.trim_end_matches(['\r', '\n']);
    let launch = ratify_competitive_match_launch_from_attended_confirmation_v4(
        scope,
        visible_account_alias,
        request,
        challenge_nonce,
        issued_at_unix_millis,
        supplied_phrase,
    )?;
    Ok((runtime, launch, visible_identity))
}

/// Extends one separately attended priority-Pass launch to the exact admitted
/// eleven-family gesture profile. The owner must confirm the broader scope in
/// an interactive terminal. This consumes both authorities and creates no
/// input, event-entry, or spending capability.
pub fn ratify_competitive_gesture_match_launch_attended_v1(
    gesture_authorization: RatifiedMtgoCompetitiveDuelGestureAuthorizationV1,
    pass_match_launch: RatifiedMtgoCompetitiveMatchLaunchV1,
) -> Result<RatifiedMtgoCompetitiveGestureMatchLaunchV1, String> {
    let candidate = validate_competitive_gesture_match_launch_authorities_v1(
        &gesture_authorization,
        &pass_match_launch,
    )?;
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    if !stdin.is_terminal() || !stdout.is_terminal() {
        return Err(
            "attended competitive gesture match launch requires an interactive terminal".to_owned(),
        );
    }
    let mut challenge_nonce = [0_u8; 8];
    unsafe {
        BCryptGenRandom(None, &mut challenge_nonce, BCRYPT_USE_SYSTEM_PREFERRED_RNG)
            .ok()
            .map_err(|error| format!("generate attended gesture launch challenge: {error}"))?;
    }
    let issued_at_unix_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before epoch: {error}"))?
        .as_millis();
    let expected_phrase = attended_competitive_gesture_match_launch_confirmation_phrase_v1(
        candidate.event_kind,
        pass_match_launch.authorization.game_number,
        &challenge_nonce,
    );
    writeln!(stdout, "MTGO attended all-family match launch extension")
        .map_err(|error| format!("write attended gesture launch prompt: {error}"))?;
    writeln!(
        stdout,
        "Account: {}",
        gesture_authorization.visible_account_alias
    )
    .map_err(|error| format!("write attended gesture launch account: {error}"))?;
    writeln!(
        stdout,
        "Mode: {}; game: {}; exact match launch: {}; gesture evaluation: {}; gesture profile: {}",
        competitive_event_kind_label_v4(candidate.event_kind),
        pass_match_launch.authorization.game_number,
        &pass_match_launch.launch_authorization_commitment_sha256[..12],
        &candidate.gesture_evaluation_commitment_sha256[..12],
        &candidate.gesture_profile_admission_commitment_sha256[..12]
    )
    .map_err(|error| format!("write attended gesture launch identity: {error}"))?;
    writeln!(
        stdout,
        "This extends the exact already-entered game from priority Pass to the reviewed eleven-family gesture profile. It cannot enter an event or spend resources."
    )
    .map_err(|error| format!("write attended gesture launch scope: {error}"))?;
    writeln!(stdout, "Type exactly: {expected_phrase}")
        .map_err(|error| format!("write attended gesture launch challenge: {error}"))?;
    stdout
        .flush()
        .map_err(|error| format!("flush attended gesture launch prompt: {error}"))?;
    let mut supplied_phrase = String::new();
    stdin
        .read_line(&mut supplied_phrase)
        .map_err(|error| format!("read attended gesture launch confirmation: {error}"))?;
    ratify_competitive_gesture_match_launch_from_confirmation_v1(
        gesture_authorization,
        pass_match_launch,
        candidate,
        challenge_nonce,
        issued_at_unix_millis,
        supplied_phrase.trim_end_matches(['\r', '\n']),
    )
}

pub fn begin_competitive_game_session_v1(
    authorization: RatifiedMtgoCompetitiveDuelPassAuthorizationV1,
    match_launch: RatifiedMtgoCompetitiveMatchLaunchV1,
) -> Result<OpaqueMtgoCompetitiveGameSessionV1, String> {
    validate_competitive_game_session_authorities_v1(&authorization, &match_launch)?;
    let session_commitment_sha256 =
        initial_competitive_game_session_commitment_v1(&authorization, &match_launch);
    Ok(OpaqueMtgoCompetitiveGameSessionV1 {
        authorization,
        last_confirmed_frame_sequence: match_launch
            .valid_from_frame_sequence
            .checked_sub(1)
            .ok_or("competitive game session has an invalid starting frame")?,
        match_launch,
        session_commitment_sha256,
        confirmed_action_count: 0,
    })
}

/// Begins a move-only all-family session for one exact attended League or
/// Challenge game. The session is a lineage boundary only. It cannot prepare
/// or execute a gesture in this tranche.
pub fn begin_competitive_gesture_game_session_v1(
    launch: RatifiedMtgoCompetitiveGestureMatchLaunchV1,
) -> Result<OpaqueMtgoCompetitiveGestureGameSessionV1, String> {
    let candidate = validate_competitive_gesture_match_launch_authorities_v1(
        &launch.gesture_authorization,
        &launch.pass_match_launch,
    )?;
    if !is_sha256_v2(&launch.gesture_match_launch_commitment_sha256) {
        return Err("competitive gesture match launch commitment is invalid".to_owned());
    }
    let valid_from_frame_sequence = launch.pass_match_launch.valid_from_frame_sequence;
    let last_confirmed_frame_sequence = valid_from_frame_sequence
        .checked_sub(1)
        .ok_or("competitive gesture game session has an invalid starting frame")?;
    let session_commitment_sha256 = initial_competitive_gesture_game_session_commitment_v1(
        &candidate,
        &launch.pass_match_launch,
        &launch.gesture_match_launch_commitment_sha256,
    );
    Ok(OpaqueMtgoCompetitiveGestureGameSessionV1 {
        launch,
        session_commitment_sha256,
        last_confirmed_frame_sequence,
        confirmed_action_count: 0,
        entry_ratification_commitment_sha256: None,
        selected_deck_label_sha256: None,
        selected_deck_region_sha256: None,
        deck_manifest_sha256: None,
        deck_format_sha256: None,
        policy_deployment_commitment_sha256: None,
    })
}

/// Joins the source stage of one exact gesture sequence to the matching
/// all-family game session. This consumes both move-only lineages and creates
/// no preparation, executor, or coordinate access.
pub fn bind_competitive_duel_gesture_sequence_session_v1(
    sequence: OpaqueMtgoCompetitiveDuelGestureSequenceV1,
    session: OpaqueMtgoCompetitiveGestureGameSessionV1,
) -> Result<OpaqueMtgoSessionBoundCompetitiveDuelGestureV1, String> {
    let sequence_commitments = sequence.commitments_v1();
    let session_commitments = session.commitments_v1();
    let commitments = competitive_duel_gesture_sequence_session_binding_from_parts_v1(
        &sequence_commitments,
        &session_commitments,
    )?;
    Ok(OpaqueMtgoSessionBoundCompetitiveDuelGestureV1 {
        _session: session,
        _sequence: sequence,
        commitments,
    })
}

/// Rechecks the source primitive of one exact session-bound gesture on a
/// distinct next-frame perception. Only the exact profile-pinned gesture
/// target runtime may provide the complete target set. The request and runtime
/// identities are retained in the result, which cannot send input and exposes
/// no target coordinates.
pub fn prepare_session_bound_competitive_duel_gesture_source_stage_v1(
    bound: OpaqueMtgoSessionBoundCompetitiveDuelGestureV1,
    fresh_perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    runtime: &OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1, String> {
    let OpaqueMtgoSessionBoundCompetitiveDuelGestureV1 {
        _session: session,
        _sequence: sequence,
        commitments: bound_commitments,
    } = bound;
    let session_commitments = session.commitments_v1();
    let profile = &session.launch.gesture_authorization._gesture_profile;
    if bound_commitments.gesture_evaluation_commitment_sha256
        != profile.evaluation_commitment_sha256()
        || bound_commitments.gesture_profile_admission_commitment_sha256
            != profile.admission_commitment_sha256()
    {
        return Err("session-bound gesture differs from its retained profile".to_owned());
    }
    let prepared = prepare_opaque_competitive_duel_gesture_source_stage_from_pinned_runtime_v1(
        sequence,
        fresh_perception,
        profile,
        runtime,
        timeout_ms,
    )?;
    let commitments = competitive_duel_gesture_source_preparation_from_parts_v1(
        &bound_commitments,
        &session_commitments,
        &prepared.commitments,
    )?;
    Ok(OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1 {
        _session: session,
        _prepared: prepared,
        commitments,
    })
}

/// Emits exactly one primitive from an immediately rechecked gesture stage.
/// The process-wide gate is locked before the input attempt and remains locked
/// until either the final postcondition or the exact next visible stage is
/// confirmed.
pub fn execute_prepared_competitive_duel_gesture_primitive_v1(
    prepared: OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1,
) -> Result<OpaqueMtgoPendingCompetitiveDuelGesturePrimitiveV1, String> {
    let prepared_commitments = prepared.commitments_v1();
    if prepared_commitments.stage_index >= prepared_commitments.gesture_stage_count
        || prepared_commitments.target_count == 0
        || prepared_commitments.target_count > 2
        || !canonical_duel_gesture_action_families_v1()
            .contains(&prepared_commitments.selected_action_family)
    {
        return Err(
            "one-primitive executor requires one declared prepared gesture stage".to_owned(),
        );
    }
    reserve_input_gate_v3()?;
    let input_attempt_started_at_unix_millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(error) => {
            release_unattempted_reservation_v3()?;
            return Err(format!("system clock is before epoch: {error}"));
        }
    };
    if validate_preinput_capture_freshness_v3(
        prepared_commitments.fresh_captured_at_unix_millis,
        input_attempt_started_at_unix_millis,
    )
    .is_err()
    {
        release_unattempted_reservation_v3()?;
        return Err("the immediate competitive gesture capture is stale or future".to_owned());
    }
    halt_before_input_attempt_v3()?;
    let (emitted_mouse_record_count, cursor_parked_outside_client) =
        send_exactly_one_gesture_primitive_v1(&prepared._prepared)?;
    let input_sent_at_unix_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock moved before epoch after gesture input: {error}"))?
        .as_millis();
    if input_sent_at_unix_millis < input_attempt_started_at_unix_millis {
        return Err("system clock moved backwards during competitive gesture input".to_owned());
    }
    let input_receipt_sha256 = competitive_duel_gesture_input_receipt_v1(
        &prepared_commitments,
        &prepared._prepared.primitive,
        input_sent_at_unix_millis,
        emitted_mouse_record_count,
        cursor_parked_outside_client,
    )?;
    set_pending_v3(&input_receipt_sha256)?;
    Ok(OpaqueMtgoPendingCompetitiveDuelGesturePrimitiveV1 {
        prepared,
        commitments: MtgoPendingCompetitiveDuelGesturePrimitiveCommitmentsV1 {
            preparation_binding_commitment_sha256: prepared_commitments
                .preparation_binding_commitment_sha256,
            gesture_target_runtime_identity_commitment_sha256: prepared_commitments
                .gesture_target_runtime_identity_commitment_sha256,
            gesture_target_request_commitment_sha256: prepared_commitments
                .gesture_target_request_commitment_sha256,
            before_input_postcondition_verification_commitment_sha256: prepared_commitments
                .before_input_postcondition_verification_commitment_sha256,
            input_receipt_sha256,
            selected_action_family: prepared_commitments.selected_action_family,
            event_kind: prepared_commitments.event_kind,
            game_number: prepared_commitments.game_number,
            stage_index: prepared_commitments.stage_index,
            gesture_stage_count: prepared_commitments.gesture_stage_count,
            before_frame_id: prepared_commitments.fresh_frame_id,
            before_frame_sequence: prepared_commitments.fresh_frame_sequence,
            input_sent_at_unix_millis,
            emitted_mouse_record_count,
            cursor_parked_outside_client,
        },
    })
}

/// Emits one exact gesture primitive derived only from the player-visible model
/// boundary. Authority is borrowed from the exact withheld event lease and
/// attended game session. The process-wide gate remains locked until a newer
/// visible postcondition is confirmed.
pub(crate) fn execute_prepared_competitive_player_visible_gameplay_primitive_v1(
    before: OpaqueMtgoPreparedPlayerVisibleGameplayBeforeInputV1,
    lease: &OpaqueMtgoCompetitiveEventGameplayLeaseV1,
    session: &OpaqueMtgoCompetitiveGestureGameSessionV1,
) -> Result<OpaqueMtgoPendingCompetitivePlayerVisibleGameplayPrimitiveV1, String> {
    let authority_binding =
        competitive_player_visible_gameplay_authority_binding_v1(&before, lease, session)?;
    let pointer = &before.pointer;
    let before_commitment = before.before_input_commitment_sha256_v1().to_owned();
    let action_family = before.checked.action_family_v1();
    let primitive_index = pointer.commitments.primitive_index;
    let primitive_is_final = pointer.is_final_primitive_v1();
    let before_frame_id = pointer.commitments.frame_id;
    let before_frame_sequence = pointer.commitments.frame_sequence;
    reserve_input_gate_v3()?;
    let input_attempt_started_at_unix_millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(error) => {
            release_unattempted_reservation_v3()?;
            return Err(format!("system clock is before epoch: {error}"));
        }
    };
    if validate_preinput_capture_freshness_v3(
        pointer.commitments.captured_at_unix_millis,
        input_attempt_started_at_unix_millis,
    )
    .is_err()
    {
        release_unattempted_reservation_v3()?;
        return Err("the immediate player-visible gameplay capture is stale or future".to_owned());
    }
    halt_before_input_attempt_v3()?;
    let (emitted_mouse_record_count, cursor_parked_outside_client) =
        send_exactly_one_player_visible_gameplay_primitive_v1(pointer)?;
    let input_sent_at_unix_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock moved before epoch after gameplay input: {error}"))?
        .as_millis();
    if input_sent_at_unix_millis < input_attempt_started_at_unix_millis {
        return Err("system clock moved backwards during player-visible gameplay input".to_owned());
    }
    let input_receipt = make_opaque_player_visible_gameplay_input_receipt_v1(
        &before,
        &authority_binding,
        input_sent_at_unix_millis,
        emitted_mouse_record_count,
        cursor_parked_outside_client,
    )?;
    let input_receipt_sha256 = input_receipt
        .input_receipt_commitment_sha256_v1()
        .to_owned();
    set_pending_v3(&input_receipt_sha256)?;
    Ok(
        OpaqueMtgoPendingCompetitivePlayerVisibleGameplayPrimitiveV1 {
            before,
            input_receipt,
            commitments: MtgoPendingCompetitivePlayerVisibleGameplayPrimitiveCommitmentsV1 {
                before_input_commitment_sha256: before_commitment,
                actuator_authority_binding_sha256: authority_binding,
                input_receipt_sha256,
                selected_action_family: action_family,
                event_kind: lease.commitments.event_kind,
                game_number: lease.commitments.game_number,
                primitive_index,
                primitive_is_final,
                before_frame_id,
                before_frame_sequence,
                input_sent_at_unix_millis,
                emitted_mouse_record_count,
                cursor_parked_outside_client,
            },
        },
    )
}

pub(crate) fn confirm_pending_competitive_player_visible_gameplay_primitive_v1(
    pending: OpaqueMtgoPendingCompetitivePlayerVisibleGameplayPrimitiveV1,
    after_frame: crate::probe::OpaqueMtgoAdmittedDuelVisibleFrameV1,
    after_identity: crate::probe::MtgoDuelPerceptionFrameIdentityV1,
    game_log_corroboration: Option<CheckedUntrustedMtgoPlayerVisibleGameLogActionCorroborationV1>,
) -> Result<OpaqueMtgoPlayerVisibleGameplayAfterInputV1, String> {
    require_matching_pending_v3(&pending.commitments.input_receipt_sha256)?;
    let OpaqueMtgoPendingCompetitivePlayerVisibleGameplayPrimitiveV1 {
        before,
        input_receipt,
        commitments: _,
    } = pending;
    let after = match complete_opaque_player_visible_gameplay_after_input_v1(
        before,
        input_receipt,
        after_frame,
        after_identity,
        game_log_corroboration,
    ) {
        Ok(after) => after,
        Err(error) => {
            halt_gate_v3()?;
            return Err(format!(
                "player-visible gameplay postcondition failed and the input gate is halted: {error}"
            ));
        }
    };
    Ok(after)
}

pub(crate) fn release_confirmed_competitive_player_visible_gameplay_primitive_v1(
    input_receipt_sha256: &str,
) -> Result<(), String> {
    release_confirmed_pending_v3(input_receipt_sha256)
}

/// Confirms one non-final primitive by joining its pending input receipt to a
/// strictly newer visible stage discovered by the exact pinned target runtime.
/// The process gate is reopened only after every lineage and time check passes.
/// The returned value still cannot emit the next primitive.
pub fn confirm_pending_competitive_duel_gesture_continuation_v1(
    pending: OpaqueMtgoPendingCompetitiveDuelGesturePrimitiveV1,
    current_perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    runtime: &OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoConfirmedCompetitiveDuelGestureContinuationV1, String> {
    require_matching_pending_v3(&pending.commitments.input_receipt_sha256)?;
    let OpaqueMtgoPendingCompetitiveDuelGesturePrimitiveV1 {
        prepared,
        commitments: pending_commitments,
    } = pending;
    if pending_commitments
        .stage_index
        .checked_add(1)
        .ok_or("pending gesture continuation stage index overflow")?
        >= pending_commitments.gesture_stage_count
    {
        halt_gate_v3()?;
        return Err(
            "final gesture primitive requires complete postcondition confirmation; input gate halted"
                .to_owned(),
        );
    }
    let OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1 {
        _session: session,
        _prepared: probe_prepared,
        commitments: prepared_commitments,
    } = prepared;
    let continuation =
        match advance_prepared_competitive_duel_gesture_sequence_from_pinned_runtime_v1(
            probe_prepared,
            current_perception,
            &session.launch.gesture_authorization._gesture_profile,
            runtime,
            timeout_ms,
        ) {
            Ok(continuation) => continuation,
            Err(error) => {
                halt_gate_v3()?;
                return Err(format!(
                    "competitive gesture continuation failed and the input gate is halted: {error}"
                ));
            }
        };
    let visible = continuation.commitments_v1();
    let continuation_receipt_sha256 = match competitive_duel_gesture_continuation_receipt_v1(
        &pending_commitments,
        &prepared_commitments,
        &visible,
    ) {
        Ok(receipt) => receipt,
        Err(error) => {
            halt_gate_v3()?;
            return Err(format!(
                "competitive gesture continuation receipt failed and the input gate is halted: {error}"
            ));
        }
    };
    release_confirmed_pending_v3(&pending_commitments.input_receipt_sha256)?;
    Ok(OpaqueMtgoConfirmedCompetitiveDuelGestureContinuationV1 {
        _continuation: continuation,
        _session: session,
        commitments: MtgoConfirmedCompetitiveDuelGestureContinuationCommitmentsV1 {
            input_receipt_sha256: pending_commitments.input_receipt_sha256,
            preparation_binding_commitment_sha256: prepared_commitments
                .preparation_binding_commitment_sha256,
            prepared_sequence_commitment_sha256: prepared_commitments
                .prepared_sequence_commitment_sha256,
            prior_sequence_commitment_sha256: visible.prior_sequence_commitment_sha256,
            advanced_sequence_commitment_sha256: visible.advanced_sequence_commitment_sha256,
            visible_transition_commitment_sha256: visible.visible_transition_commitment_sha256,
            continuation_receipt_sha256,
            gesture_target_runtime_identity_commitment_sha256: visible
                .gesture_target_runtime_identity_commitment_sha256,
            next_stage_target_request_commitment_sha256: visible
                .gesture_target_request_commitment_sha256,
            unchanged_game_session_commitment_sha256: prepared_commitments
                .game_session_commitment_sha256,
            selected_action_family: visible.selected_action_family,
            event_kind: visible.event_kind,
            game_number: visible.game_number,
            completed_stage_index: pending_commitments.stage_index,
            next_stage_index: visible.stage_index,
            gesture_stage_count: visible.gesture_stage_count,
            before_frame_id: pending_commitments.before_frame_id,
            before_frame_sequence: pending_commitments.before_frame_sequence,
            after_frame_id: visible.frame_id,
            after_frame_sequence: visible.frame_sequence,
            after_captured_at_unix_millis: visible.captured_at_unix_millis,
            next_stage_target_count: visible.target_count,
        },
    })
}

/// Consumes one confirmed intermediate stage and requires one additional
/// admitted perception before preparing its next primitive. The exact target
/// set is reacquired from the pinned runtime on that fresh frame. No input is
/// emitted by this function.
pub fn prepare_confirmed_competitive_duel_gesture_continuation_stage_v1(
    confirmed: OpaqueMtgoConfirmedCompetitiveDuelGestureContinuationV1,
    fresh_perception: OpaqueMtgoAdmittedDuelPerceptionV1,
    runtime: &OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1, String> {
    let OpaqueMtgoConfirmedCompetitiveDuelGestureContinuationV1 {
        _continuation: continuation,
        _session: session,
        commitments: confirmed_commitments,
    } = confirmed;
    let session_commitments = session.commitments_v1();
    let prepared =
        prepare_opaque_competitive_duel_gesture_continuation_stage_from_pinned_runtime_v1(
            continuation,
            fresh_perception,
            &session.launch.gesture_authorization._gesture_profile,
            runtime,
            timeout_ms,
        )?;
    let commitments = competitive_duel_gesture_continuation_preparation_from_parts_v1(
        &confirmed_commitments,
        &session_commitments,
        &prepared.commitments,
    )?;
    Ok(OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1 {
        _session: session,
        _prepared: prepared,
        commitments,
    })
}

pub fn confirm_pending_competitive_duel_gesture_primitive_v1(
    pending: OpaqueMtgoPendingCompetitiveDuelGesturePrimitiveV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoConfirmedCompetitiveDuelGesturePrimitiveV1, String> {
    require_matching_pending_v3(&pending.commitments.input_receipt_sha256)?;
    if pending.commitments.stage_index.checked_add(1)
        != Some(pending.commitments.gesture_stage_count)
    {
        halt_gate_v3()?;
        return Err(
            "non-final gesture primitive requires visible continuation confirmation; input gate halted"
                .to_owned(),
        );
    }
    let OpaqueMtgoPendingCompetitiveDuelGesturePrimitiveV1 {
        prepared,
        commitments: pending_commitments,
    } = pending;
    let OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1 {
        _session: session,
        _prepared: probe_prepared,
        commitments: prepared_commitments,
    } = prepared;
    let confirmation = match confirm_opaque_competitive_duel_gesture_postcondition_v1(
        probe_prepared,
        profile,
        pending_commitments.input_sent_at_unix_millis,
        timeout_ms,
    ) {
        Ok(confirmation) => confirmation,
        Err(error) => {
            halt_gate_v3()?;
            return Err(format!(
                "competitive gesture postcondition failed and the input gate is halted: {error}"
            ));
        }
    };
    let visible = confirmation.commitments_v1();
    let transition_receipt_sha256 = competitive_duel_gesture_transition_receipt_v1(
        &pending_commitments,
        &prepared_commitments,
        &visible,
    )?;
    let prior_game_session_commitment_sha256 = session.session_commitment_sha256.clone();
    let session = match advance_competitive_gesture_game_session_v1(
        session,
        &visible,
        &transition_receipt_sha256,
    ) {
        Ok(session) => session,
        Err(error) => {
            halt_gate_v3()?;
            return Err(format!(
                "competitive gesture game session advance failed and the input gate is halted: {error}"
            ));
        }
    };
    let advanced = session.commitments_v1();
    release_confirmed_pending_v3(&pending_commitments.input_receipt_sha256)?;
    Ok(OpaqueMtgoConfirmedCompetitiveDuelGesturePrimitiveV1 {
        confirmation,
        session,
        commitments: MtgoConfirmedCompetitiveDuelGesturePrimitiveCommitmentsV1 {
            input_receipt_sha256: pending_commitments.input_receipt_sha256,
            visible_postcondition_commitment_sha256: visible.opaque_confirmation_commitment_sha256,
            transition_receipt_sha256,
            prior_game_session_commitment_sha256,
            advanced_game_session_commitment_sha256: advanced.session_commitment_sha256,
            selected_action_family: visible.selected_action_family,
            event_kind: visible.event_kind,
            game_number: visible.game_number,
            after_frame_id: visible.after_frame_id,
            after_frame_sequence: visible.after_frame_sequence,
            postcondition_candidate_count: visible.postcondition_candidate_count,
            confirmed_action_count: advanced.confirmed_action_count,
        },
    })
}

pub fn bind_prepared_competitive_duel_pass_session_v2(
    prepared: OpaqueMtgoPreparedCompetitiveDuelPassV1,
    session: OpaqueMtgoCompetitiveGameSessionV1,
) -> Result<OpaqueMtgoAuthorizationBoundCompetitiveDuelPassV1, String> {
    let prepared_commitments = prepared.commitments_v1();
    let commitments = competitive_duel_pass_authorization_binding_commitments_v1(
        &prepared_commitments,
        &session,
    )?;
    Ok(OpaqueMtgoAuthorizationBoundCompetitiveDuelPassV1 {
        _prepared: prepared,
        _session: session,
        commitments,
    })
}

pub fn execute_authorized_competitive_duel_pass_v1(
    bound: OpaqueMtgoAuthorizationBoundCompetitiveDuelPassV1,
) -> Result<OpaqueMtgoPendingCompetitiveDuelPassV1, String> {
    reserve_input_gate_v3()?;
    let OpaqueMtgoAuthorizationBoundCompetitiveDuelPassV1 {
        _prepared: prepared,
        _session: session,
        commitments: authorization_binding_commitments,
    } = bound;
    let input_sent_at_unix_millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(error) => {
            release_unattempted_reservation_v3()?;
            return Err(format!("system clock is before epoch: {error}"));
        }
    };
    if validate_preinput_capture_freshness_v3(
        prepared.commitments_v1().immediate_captured_at_unix_millis,
        input_sent_at_unix_millis,
    )
    .is_err()
    {
        release_unattempted_reservation_v3()?;
        return Err(
            "the immediate competitive Pass capture is stale or from the future".to_owned(),
        );
    }
    halt_before_input_attempt_v3()?;
    let cursor_parked_outside_client = send_exactly_one_left_click_v3(&prepared)?;
    let input_receipt_sha256 = competitive_duel_pass_input_receipt_v1(
        &prepared,
        &authorization_binding_commitments,
        input_sent_at_unix_millis,
        cursor_parked_outside_client,
    );
    set_pending_v3(&input_receipt_sha256)?;
    Ok(OpaqueMtgoPendingCompetitiveDuelPassV1 {
        prepared,
        session,
        authorization_binding_commitments,
        input_receipt_sha256,
        input_sent_at_unix_millis,
        cursor_parked_outside_client,
    })
}

pub fn confirm_pending_competitive_duel_pass_v2(
    pending: OpaqueMtgoPendingCompetitiveDuelPassV1,
    profile: &AdmittedMtgoDuelPerceptionProfileV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoConfirmedCompetitiveDuelPassTransitionV2, String> {
    require_matching_pending_v3(&pending.input_receipt_sha256)?;
    let OpaqueMtgoPendingCompetitiveDuelPassV1 {
        prepared,
        session,
        authorization_binding_commitments,
        input_receipt_sha256,
        input_sent_at_unix_millis: _,
        cursor_parked_outside_client: _,
    } = pending;
    let authorization_binding_commitment_sha256 =
        authorization_binding_commitments.authorization_binding_commitment_sha256;
    let confirmation = match confirm_opaque_competitive_duel_pass_postcondition_v1(
        prepared, profile, timeout_ms,
    ) {
        Ok(confirmation) => confirmation,
        Err(error) => {
            halt_gate_v3()?;
            return Err(format!(
                "competitive Pass postcondition failed and the input gate is halted: {error}"
            ));
        }
    };
    let visible = confirmation.commitments_v1();
    let transition_receipt_sha256 = competitive_duel_pass_transition_receipt_v1(
        &input_receipt_sha256,
        &authorization_binding_commitment_sha256,
        &visible,
    );
    let prior_game_session_commitment_sha256 = session.session_commitment_sha256.clone();
    let session =
        match advance_competitive_game_session_v1(session, &visible, &transition_receipt_sha256) {
            Ok(session) => session,
            Err(error) => {
                halt_gate_v3()?;
                return Err(format!(
                    "competitive game session advance failed and the input gate is halted: {error}"
                ));
            }
        };
    let advanced = session.commitments_v1();
    release_confirmed_pending_v3(&input_receipt_sha256)?;
    Ok(OpaqueMtgoConfirmedCompetitiveDuelPassTransitionV2 {
        confirmation,
        session,
        commitments: MtgoConfirmedCompetitiveDuelPassCommitmentsV2 {
            input_receipt_sha256,
            visible_postcondition_commitment_sha256: visible.opaque_confirmation_commitment_sha256,
            transition_receipt_sha256,
            prior_game_session_commitment_sha256,
            advanced_game_session_commitment_sha256: advanced.session_commitment_sha256,
            event_kind: visible.event_kind,
            game_number: visible.game_number,
            after_frame_id: visible.after_frame_id,
            after_frame_sequence: visible.after_frame_sequence,
            postcondition_candidate_count: visible.postcondition_candidate_count,
            confirmed_action_count: advanced.confirmed_action_count,
        },
    })
}

fn ratify_competitive_duel_pass_authorization_with_commitment_v1(
    scope: MtgoAuthorizationScopeV1,
    visible_account_alias: String,
    event_kind: MtgoCompetitiveEventKindV1,
    ratified_commitment_sha256: Option<&str>,
) -> Result<RatifiedMtgoCompetitiveDuelPassAuthorizationV1, String> {
    let mode_authorization_commitment_sha256 = validate_competitive_duel_pass_authorization_v1(
        &scope,
        &visible_account_alias,
        event_kind,
    )?;
    let authorization_commitment_sha256 = competitive_duel_pass_authorization_commitment_v1(
        &scope,
        &visible_account_alias,
        event_kind,
        &mode_authorization_commitment_sha256,
    );
    if ratified_commitment_sha256 != Some(authorization_commitment_sha256.as_str()) {
        return Err(
            "the exact competitive Pass permission correspondence is not ratified in this build"
                .to_owned(),
        );
    }
    Ok(RatifiedMtgoCompetitiveDuelPassAuthorizationV1 {
        scope,
        _permission_correspondence: None,
        visible_account_alias,
        event_kind,
        mode_authorization_commitment_sha256,
        authorization_commitment_sha256,
        permission_review_commitment_sha256: None,
    })
}

fn ratify_competitive_duel_pass_authorization_from_correspondence_with_commitment_v2(
    correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    visible_account_alias: String,
    event_kind: MtgoCompetitiveEventKindV1,
    ratified_commitment_sha256: Option<&str>,
) -> Result<RatifiedMtgoCompetitiveDuelPassAuthorizationV1, String> {
    let candidate = review_competitive_duel_pass_ratification_candidate_from_correspondence_v2(
        &correspondence,
        &visible_account_alias,
        event_kind,
    )?;
    if ratified_commitment_sha256 != Some(candidate.ratification_commitment_sha256.as_str()) {
        return Err(
            "the exact reviewed competitive Pass permission is not ratified in this build"
                .to_owned(),
        );
    }
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(event_kind)
        .map_err(|error| format!("derive reviewed competitive mode scope: {error}"))?;
    Ok(RatifiedMtgoCompetitiveDuelPassAuthorizationV1 {
        scope,
        _permission_correspondence: Some(correspondence),
        visible_account_alias,
        event_kind,
        mode_authorization_commitment_sha256: candidate.mode_authorization_commitment_sha256,
        authorization_commitment_sha256: candidate.ratification_commitment_sha256,
        permission_review_commitment_sha256: Some(candidate.permission_review_commitment_sha256),
    })
}

fn ratify_competitive_pregame_authorization_from_correspondence_with_commitment_v1(
    correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    visible_account_alias: String,
    event_kind: MtgoCompetitiveEventKindV1,
    heuristic: &AdmittedMtgoCompetitivePregameHeuristicV1,
    ratified_commitment_sha256: Option<&str>,
) -> Result<RatifiedMtgoCompetitivePregameAuthorizationV1, String> {
    let candidate = review_competitive_pregame_ratification_candidate_from_correspondence_v1(
        &correspondence,
        &visible_account_alias,
        event_kind,
        heuristic,
    )?;
    if ratified_commitment_sha256 != Some(candidate.ratification_commitment_sha256.as_str()) {
        return Err(
            "the exact reviewed competitive pregame permission is not ratified in this build"
                .to_owned(),
        );
    }
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(event_kind)
        .map_err(|error| format!("derive reviewed competitive pregame scope: {error}"))?;
    Ok(RatifiedMtgoCompetitivePregameAuthorizationV1 {
        scope,
        _permission_correspondence: correspondence,
        visible_account_alias,
        commitments: candidate,
    })
}

fn build_attended_competitive_entry_review_request_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    visible_account_alias: &str,
    event_display_label: &str,
) -> Result<MtgoAttendedCompetitiveEntryReviewRequestV1, String> {
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(source.event_kind())
        .map_err(|error| format!("derive reviewed competitive entry scope: {error}"))?;
    let mode_authorization_commitment_sha256 = validate_competitive_duel_pass_authorization_v1(
        &scope,
        visible_account_alias,
        source.event_kind(),
    )?;
    let request = MtgoAttendedCompetitiveEntryReviewRequestV1 {
        schema_version: MTGO_ATTENDED_COMPETITIVE_ENTRY_REVIEW_SCHEMA_V1,
        event_kind: source.event_kind(),
        event_display_label: event_display_label.to_owned(),
        source_lifecycle_snapshot_commitment_sha256: source.snapshot_commitment_sha256().to_owned(),
        frame_id: source.frame_id_v1(),
        frame_sequence: source.frame_sequence(),
        event_identity_sha256: source
            .event_identity_sha256_v1()
            .ok_or("entry-review snapshot has no exact event identity")?
            .to_owned(),
        entry_terms: source
            .entry_terms_v1()
            .ok_or("entry-review snapshot has no exact visible entry terms")?
            .clone(),
        account_alias_sha256: scope.account_alias_sha256.clone(),
        correspondence_sha256: scope.written_permission_sha256.clone(),
        permission_review_commitment_sha256: correspondence.review_commitment_sha256().to_owned(),
        mode_authorization_commitment_sha256,
    };
    validate_attended_competitive_entry_review_request_v1(
        correspondence,
        source,
        visible_account_alias,
        &request,
    )?;
    Ok(request)
}

fn review_competitive_entry_from_attended_confirmation_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    source: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    visible_account_alias: &str,
    event_display_label: String,
    challenge_nonce: [u8; 8],
    issued_at_unix_millis: u128,
    supplied_phrase: &str,
) -> Result<CheckedUntrustedMtgoAttendedCompetitiveEntryReviewV1, String> {
    let parts = make_attended_competitive_entry_review_parts_v1(
        correspondence,
        &source,
        visible_account_alias,
        &event_display_label,
        challenge_nonce,
        issued_at_unix_millis,
        supplied_phrase,
    )?;
    Ok(CheckedUntrustedMtgoAttendedCompetitiveEntryReviewV1 {
        _source: source,
        entry_authorization: parts.entry_authorization,
        commitments: parts.commitments,
    })
}

fn make_attended_competitive_entry_review_parts_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    visible_account_alias: &str,
    event_display_label: &str,
    challenge_nonce: [u8; 8],
    issued_at_unix_millis: u128,
    supplied_phrase: &str,
) -> Result<MtgoAttendedCompetitiveEntryReviewPartsV1, String> {
    let request = build_attended_competitive_entry_review_request_v1(
        correspondence,
        source,
        visible_account_alias,
        event_display_label,
    )?;
    if issued_at_unix_millis == 0 || challenge_nonce.iter().all(|value| *value == 0) {
        return Err("attended competitive entry review challenge is invalid".to_owned());
    }
    let expected_phrase = attended_competitive_entry_review_confirmation_phrase_v1(
        request.event_kind,
        request.entry_terms.resource,
        request.entry_terms.amount,
        &challenge_nonce,
    );
    if supplied_phrase != expected_phrase {
        return Err("attended competitive entry review challenge did not match".to_owned());
    }
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(request.event_kind)
        .map_err(|error| format!("derive reviewed competitive entry scope: {error}"))?;
    let entry_authorization = MtgoCompetitiveEntryAuthorizationV1 {
        schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
        account_alias_sha256: request.account_alias_sha256.clone(),
        written_permission_sha256: request.correspondence_sha256.clone(),
        event_kind: request.event_kind,
        event_identity_sha256: request.event_identity_sha256.clone(),
        entry_terms: request.entry_terms.clone(),
        exact_entry_authorized: true,
        existing_account_resources_only: true,
    };
    let intent = make_offline_competitive_lifecycle_intent_v1(
        source,
        MtgoCompetitiveLifecycleActionV1::ConfirmEntry,
        &scope,
        Some(&entry_authorization),
    )
    .map_err(|error| format!("validate exact offline competitive entry review: {error}"))?;
    let entry_authorization_sha256 = intent
        .entry_authorization_sha256
        .ok_or("validated competitive entry intent omitted its authorization commitment")?;
    let request_json = serde_json::to_vec(&request)
        .map_err(|error| format!("serialize attended competitive entry review: {error}"))?;
    let request_commitment_sha256 = hash_parts_v2(
        ATTENDED_COMPETITIVE_ENTRY_REVIEW_REQUEST_DOMAIN_V1,
        &[
            request_json.as_slice(),
            visible_account_alias.as_bytes(),
            b"owner_readable_label_checked_visible_terms_review_only",
        ],
    );
    let owner_review_receipt_sha256 = hash_parts_v2(
        ATTENDED_COMPETITIVE_ENTRY_REVIEW_RECEIPT_DOMAIN_V1,
        &[
            request_commitment_sha256.as_bytes(),
            entry_authorization_sha256.as_bytes(),
            challenge_nonce.as_slice(),
            issued_at_unix_millis.to_be_bytes().as_slice(),
            supplied_phrase.as_bytes(),
            b"interactive_terminal_owner_review_no_entry_no_spending_no_input",
        ],
    );
    let commitments = MtgoAttendedCompetitiveEntryReviewCommitmentsV1 {
        source_lifecycle_snapshot_commitment_sha256: request
            .source_lifecycle_snapshot_commitment_sha256,
        permission_review_commitment_sha256: request.permission_review_commitment_sha256,
        mode_authorization_commitment_sha256: request.mode_authorization_commitment_sha256,
        request_commitment_sha256,
        entry_authorization_sha256,
        owner_review_receipt_sha256,
        event_kind: request.event_kind,
        frame_id: request.frame_id,
        frame_sequence: request.frame_sequence,
        resource: request.entry_terms.resource,
        amount: request.entry_terms.amount,
    };
    Ok(MtgoAttendedCompetitiveEntryReviewPartsV1 {
        entry_authorization,
        commitments,
    })
}

fn validate_attended_competitive_entry_review_request_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    visible_account_alias: &str,
    request: &MtgoAttendedCompetitiveEntryReviewRequestV1,
) -> Result<(), String> {
    if request.schema_version != MTGO_ATTENDED_COMPETITIVE_ENTRY_REVIEW_SCHEMA_V1
        || source.phase() != MtgoCompetitiveLifecyclePhaseV1::EntryReview
    {
        return Err(
            "attended competitive entry review requires the exact entry-review phase".to_owned(),
        );
    }
    validate_attended_launch_display_label_v4(
        &request.event_display_label,
        160,
        "entry event display label",
    )?;
    let expected_mode_word = match request.event_kind {
        MtgoCompetitiveEventKindV1::League => "league",
        MtgoCompetitiveEventKindV1::Challenge => "challenge",
    };
    if !request
        .event_display_label
        .to_ascii_lowercase()
        .contains(expected_mode_word)
    {
        return Err("attended entry event label does not identify the selected mode".to_owned());
    }
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(request.event_kind)
        .map_err(|error| format!("derive reviewed competitive entry scope: {error}"))?;
    let expected_mode_commitment = validate_competitive_duel_pass_authorization_v1(
        &scope,
        visible_account_alias,
        request.event_kind,
    )?;
    let source_event_identity = source
        .event_identity_sha256_v1()
        .ok_or("entry-review snapshot has no exact event identity")?;
    let source_entry_terms = source
        .entry_terms_v1()
        .ok_or("entry-review snapshot has no exact visible entry terms")?;
    if request.event_kind != source.event_kind()
        || request.source_lifecycle_snapshot_commitment_sha256
            != source.snapshot_commitment_sha256()
        || request.frame_id != source.frame_id_v1()
        || request.frame_sequence != source.frame_sequence()
        || request.event_identity_sha256 != source_event_identity
        || &request.entry_terms != source_entry_terms
        || request.account_alias_sha256 != scope.account_alias_sha256
        || request.correspondence_sha256 != correspondence.correspondence_sha256()
        || request.permission_review_commitment_sha256 != correspondence.review_commitment_sha256()
        || request.mode_authorization_commitment_sha256 != expected_mode_commitment
    {
        return Err(
            "attended competitive entry review does not match the exact permission, mode, frame, event, and visible terms"
                .to_owned(),
        );
    }
    for value in [
        request.source_lifecycle_snapshot_commitment_sha256.as_str(),
        request.event_identity_sha256.as_str(),
        request.entry_terms.terms_sha256.as_str(),
        request.account_alias_sha256.as_str(),
        request.correspondence_sha256.as_str(),
        request.permission_review_commitment_sha256.as_str(),
        request.mode_authorization_commitment_sha256.as_str(),
    ] {
        if !is_sha256_v2(value) {
            return Err(
                "attended competitive entry review contains an invalid commitment".to_owned(),
            );
        }
    }
    Ok(())
}

fn attended_competitive_entry_review_confirmation_phrase_v1(
    event_kind: MtgoCompetitiveEventKindV1,
    resource: MtgoCompetitiveEntryResourceV1,
    amount: u32,
    challenge_nonce: &[u8; 8],
) -> String {
    let nonce = challenge_nonce
        .iter()
        .map(|value| format!("{value:02X}"))
        .collect::<String>();
    format!(
        "AUTHORIZE MTGO {} ENTRY USING {amount} {} {nonce}",
        competitive_event_kind_label_v4(event_kind).to_ascii_uppercase(),
        competitive_entry_resource_label_v1(resource).to_ascii_uppercase(),
    )
}

fn attended_competitive_deck_selection_confirmation_phrase_v1(challenge_nonce: &[u8; 8]) -> String {
    let nonce = challenge_nonce
        .iter()
        .map(|value| format!("{value:02X}"))
        .collect::<String>();
    format!("CONFIRM MTGO SELECTED DECK {nonce}")
}

fn competitive_entry_resource_label_v1(resource: MtgoCompetitiveEntryResourceV1) -> &'static str {
    match resource {
        MtgoCompetitiveEntryResourceV1::NoCost => "no cost",
        MtgoCompetitiveEntryResourceV1::ExistingEventToken => "existing event token",
        MtgoCompetitiveEntryResourceV1::ExistingPlayPoints => "existing play points",
        MtgoCompetitiveEntryResourceV1::ExistingEventTickets => "existing event tickets",
    }
}

fn ratify_competitive_match_launch_from_attended_confirmation_v4(
    scope: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
    request: MtgoAttendedCompetitiveMatchLaunchRequestV4,
    challenge_nonce: [u8; 8],
    issued_at_unix_millis: u128,
    supplied_phrase: &str,
) -> Result<RatifiedMtgoCompetitiveMatchLaunchV1, String> {
    let mode_authorization_commitment_sha256 =
        validate_attended_competitive_match_launch_request_v4(
            scope,
            visible_account_alias,
            &request,
        )?;
    let expected_phrase = attended_competitive_match_launch_confirmation_phrase_v4(
        request.event_kind,
        request.game_number,
        &challenge_nonce,
    );
    if supplied_phrase != expected_phrase {
        return Err("attended competitive match launch challenge did not match".to_owned());
    }
    let request_json = serde_json::to_vec(&request)
        .map_err(|error| format!("serialize attended match launch request: {error}"))?;
    let request_commitment_sha256 = hash_parts_v2(
        ATTENDED_COMPETITIVE_MATCH_LAUNCH_REQUEST_DOMAIN_V4,
        &[
            scope.account_alias_sha256.as_bytes(),
            scope.written_permission_sha256.as_bytes(),
            visible_account_alias.as_bytes(),
            mode_authorization_commitment_sha256.as_bytes(),
            request_json.as_slice(),
        ],
    );
    let owner_launch_authorization_sha256 = hash_parts_v2(
        ATTENDED_COMPETITIVE_MATCH_LAUNCH_RECEIPT_DOMAIN_V4,
        &[
            request_commitment_sha256.as_bytes(),
            challenge_nonce.as_slice(),
            issued_at_unix_millis.to_be_bytes().as_slice(),
            supplied_phrase.as_bytes(),
            b"interactive_terminal_owner_confirmation_priority_pass_only",
        ],
    );
    let valid_through_frame_sequence = request
        .observed_frame_sequence
        .checked_add(ATTENDED_COMPETITIVE_MATCH_MAX_FRAME_ADVANCE_V4)
        .ok_or("attended competitive match frame lifetime overflow")?;
    let authorization = MtgoCompetitiveMatchGameplayAuthorizationV1 {
        schema_version: MTGO_COMPETITIVE_MATCH_GAMEPLAY_AUTHORIZATION_SCHEMA_V1,
        account_alias_sha256: scope.account_alias_sha256.clone(),
        written_permission_sha256: scope.written_permission_sha256.clone(),
        event_kind: request.event_kind,
        event_identity_sha256: request.event_identity_sha256,
        match_identity_sha256: request.match_identity_sha256,
        game_number: request.game_number,
        entry_authorization_sha256: request.entry_authorization_sha256,
        owner_launch_authorization_sha256,
        exact_match_gameplay_authorized: true,
        valid_through_frame_sequence,
    };
    let gameplay_authorization_commitment_sha256 =
        competitive_match_gameplay_authorization_commitment_v1(&authorization)
            .map_err(|error| format!("competitive match authorization commitment: {error}"))?;
    let launch_authorization_commitment_sha256 = competitive_match_launch_commitment_v1(
        visible_account_alias,
        &mode_authorization_commitment_sha256,
        &gameplay_authorization_commitment_sha256,
        &authorization,
    );
    let mut ratified = ratify_competitive_match_launch_with_commitment_v1(
        scope,
        visible_account_alias,
        authorization,
        Some(&launch_authorization_commitment_sha256),
    )?;
    ratified.valid_from_frame_sequence = request.observed_frame_sequence;
    Ok(ratified)
}

fn validate_competitive_gesture_match_launch_authorities_v1(
    gesture_authorization: &RatifiedMtgoCompetitiveDuelGestureAuthorizationV1,
    pass_match_launch: &RatifiedMtgoCompetitiveMatchLaunchV1,
) -> Result<MtgoReviewedCompetitiveGestureRatificationCandidateV1, String> {
    let candidate = review_competitive_duel_gesture_ratification_candidate_from_correspondence_v1(
        &gesture_authorization._permission_correspondence,
        &gesture_authorization.visible_account_alias,
        gesture_authorization.event_kind,
        &gesture_authorization._gesture_profile,
    )?;
    if candidate.ratification_commitment_sha256
        != gesture_authorization.authorization_commitment_sha256
        || candidate.mode_authorization_commitment_sha256
            != gesture_authorization.mode_authorization_commitment_sha256
        || candidate.permission_review_commitment_sha256
            != gesture_authorization.permission_review_commitment_sha256
        || candidate.gesture_evaluation_commitment_sha256
            != gesture_authorization.gesture_evaluation_commitment_sha256
        || candidate.gesture_profile_admission_commitment_sha256
            != gesture_authorization.gesture_profile_admission_commitment_sha256
    {
        return Err("competitive gesture authorization commitment changed".to_owned());
    }
    validate_competitive_match_launch_record_v1(
        &gesture_authorization.scope,
        &pass_match_launch.authorization,
    )?;
    validate_competitive_gesture_match_launch_facts_v1(
        &candidate,
        pass_match_launch,
        &gesture_authorization.visible_account_alias,
    )?;
    Ok(candidate)
}

fn validate_competitive_gesture_match_launch_facts_v1(
    candidate: &MtgoReviewedCompetitiveGestureRatificationCandidateV1,
    pass_match_launch: &RatifiedMtgoCompetitiveMatchLaunchV1,
    visible_account_alias: &str,
) -> Result<(), String> {
    if candidate.supported_action_families != canonical_duel_gesture_action_families_v1()
        || candidate.event_kind != pass_match_launch.authorization.event_kind
        || candidate.mode_authorization_commitment_sha256
            != pass_match_launch.mode_authorization_commitment_sha256
        || candidate.account_alias_sha256 != pass_match_launch.authorization.account_alias_sha256
        || candidate.correspondence_sha256
            != pass_match_launch.authorization.written_permission_sha256
        || format!("{:x}", Sha256::digest(visible_account_alias.as_bytes()))
            != candidate.account_alias_sha256
        || pass_match_launch.valid_from_frame_sequence == 0
        || pass_match_launch.valid_from_frame_sequence
            > pass_match_launch.authorization.valid_through_frame_sequence
    {
        return Err(
            "competitive gesture permission and priority-Pass launch describe different exact games"
                .to_owned(),
        );
    }
    for commitment in [
        candidate.permission_review_commitment_sha256.as_str(),
        candidate.mode_authorization_commitment_sha256.as_str(),
        candidate.gesture_evaluation_commitment_sha256.as_str(),
        candidate
            .gesture_profile_admission_commitment_sha256
            .as_str(),
        candidate.gesture_target_runtime_binary_sha256.as_str(),
        candidate.gesture_target_assets_manifest_sha256.as_str(),
        candidate.ratification_commitment_sha256.as_str(),
        pass_match_launch
            .gameplay_authorization_commitment_sha256
            .as_str(),
        pass_match_launch
            .launch_authorization_commitment_sha256
            .as_str(),
    ] {
        if !is_sha256_v2(commitment) {
            return Err(
                "competitive gesture match launch contains an invalid commitment".to_owned(),
            );
        }
    }
    let expected_gameplay =
        competitive_match_gameplay_authorization_commitment_v1(&pass_match_launch.authorization)
            .map_err(|error| format!("competitive gesture match commitment: {error}"))?;
    let expected_pass_launch = competitive_match_launch_commitment_v1(
        visible_account_alias,
        &candidate.mode_authorization_commitment_sha256,
        &expected_gameplay,
        &pass_match_launch.authorization,
    );
    if expected_gameplay != pass_match_launch.gameplay_authorization_commitment_sha256
        || expected_pass_launch != pass_match_launch.launch_authorization_commitment_sha256
    {
        return Err("priority-Pass launch commitment changed before gesture extension".to_owned());
    }
    Ok(())
}

fn ratify_competitive_gesture_match_launch_from_confirmation_v1(
    gesture_authorization: RatifiedMtgoCompetitiveDuelGestureAuthorizationV1,
    pass_match_launch: RatifiedMtgoCompetitiveMatchLaunchV1,
    candidate: MtgoReviewedCompetitiveGestureRatificationCandidateV1,
    challenge_nonce: [u8; 8],
    issued_at_unix_millis: u128,
    supplied_phrase: &str,
) -> Result<RatifiedMtgoCompetitiveGestureMatchLaunchV1, String> {
    let expected_candidate = validate_competitive_gesture_match_launch_authorities_v1(
        &gesture_authorization,
        &pass_match_launch,
    )?;
    if candidate != expected_candidate {
        return Err("competitive gesture match launch candidate changed".to_owned());
    }
    let gesture_match_launch_commitment_sha256 =
        competitive_gesture_match_launch_commitment_from_facts_v1(
            &candidate,
            &pass_match_launch,
            &gesture_authorization.visible_account_alias,
            &challenge_nonce,
            issued_at_unix_millis,
            supplied_phrase,
        )?;
    Ok(RatifiedMtgoCompetitiveGestureMatchLaunchV1 {
        gesture_authorization,
        pass_match_launch,
        gesture_match_launch_commitment_sha256,
    })
}

fn competitive_gesture_match_launch_commitment_from_facts_v1(
    candidate: &MtgoReviewedCompetitiveGestureRatificationCandidateV1,
    pass_match_launch: &RatifiedMtgoCompetitiveMatchLaunchV1,
    visible_account_alias: &str,
    challenge_nonce: &[u8; 8],
    issued_at_unix_millis: u128,
    supplied_phrase: &str,
) -> Result<String, String> {
    validate_competitive_gesture_match_launch_facts_v1(
        candidate,
        pass_match_launch,
        visible_account_alias,
    )?;
    if issued_at_unix_millis == 0 {
        return Err("attended gesture match launch has an invalid issue time".to_owned());
    }
    let expected_phrase = attended_competitive_gesture_match_launch_confirmation_phrase_v1(
        candidate.event_kind,
        pass_match_launch.authorization.game_number,
        challenge_nonce,
    );
    if supplied_phrase != expected_phrase {
        return Err("attended competitive gesture match launch challenge did not match".to_owned());
    }
    let event_kind: &[u8] = match candidate.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let family_json = serde_json::to_vec(&candidate.supported_action_families)
        .map_err(|error| format!("serialize attended gesture launch families: {error}"))?;
    Ok(hash_parts_v2(
        ATTENDED_COMPETITIVE_GESTURE_MATCH_LAUNCH_UPGRADE_DOMAIN_V1,
        &[
            candidate.ratification_commitment_sha256.as_bytes(),
            candidate.permission_review_commitment_sha256.as_bytes(),
            candidate.mode_authorization_commitment_sha256.as_bytes(),
            candidate.gesture_evaluation_commitment_sha256.as_bytes(),
            candidate
                .gesture_profile_admission_commitment_sha256
                .as_bytes(),
            candidate.gesture_target_runtime_binary_sha256.as_bytes(),
            candidate
                .gesture_target_assets_manifest_sha256
                .as_bytes(),
            pass_match_launch
                .launch_authorization_commitment_sha256
                .as_bytes(),
            pass_match_launch
                .gameplay_authorization_commitment_sha256
                .as_bytes(),
            pass_match_launch
                .authorization
                .owner_launch_authorization_sha256
                .as_bytes(),
            pass_match_launch
                .authorization
                .event_identity_sha256
                .as_bytes(),
            pass_match_launch
                .authorization
                .match_identity_sha256
                .as_bytes(),
            pass_match_launch
                .authorization
                .entry_authorization_sha256
                .as_bytes(),
            event_kind,
            &[pass_match_launch.authorization.game_number],
            pass_match_launch
                .valid_from_frame_sequence
                .to_be_bytes()
                .as_slice(),
            pass_match_launch
                .authorization
                .valid_through_frame_sequence
                .to_be_bytes()
                .as_slice(),
            challenge_nonce.as_slice(),
            issued_at_unix_millis.to_be_bytes().as_slice(),
            supplied_phrase.as_bytes(),
            family_json.as_slice(),
            b"owner_extends_exact_pass_launch_to_complete_reviewed_eleven_family_profile_no_entry_or_spending",
        ],
    ))
}

fn validate_attended_competitive_match_launch_request_v4(
    scope: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
    request: &MtgoAttendedCompetitiveMatchLaunchRequestV4,
) -> Result<String, String> {
    if request.schema_version != MTGO_ATTENDED_COMPETITIVE_MATCH_LAUNCH_REQUEST_SCHEMA_V4
        || !(1..=3).contains(&request.game_number)
        || request.observed_frame_sequence == 0
    {
        return Err("attended competitive match launch request is invalid".to_owned());
    }
    validate_attended_launch_display_label_v4(visible_account_alias, 64, "account alias")?;
    validate_attended_launch_display_label_v4(
        &request.event_display_label,
        160,
        "event display label",
    )?;
    validate_attended_launch_display_label_v4(
        &request.opponent_display_name,
        64,
        "opponent display name",
    )?;
    if request
        .opponent_display_name
        .eq_ignore_ascii_case(visible_account_alias)
    {
        return Err("attended opponent display name must differ from the account alias".to_owned());
    }
    for (value, field) in [
        (request.visible_match_id.as_str(), "visible match ID"),
        (request.visible_game_id.as_str(), "visible game ID"),
    ] {
        if value.is_empty() || value.len() > 32 || !value.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(format!(
                "attended {field} must be a bounded decimal integer"
            ));
        }
    }
    let mode_authorization_commitment_sha256 = validate_competitive_duel_pass_authorization_v1(
        scope,
        visible_account_alias,
        request.event_kind,
    )?;
    for value in [
        request.event_identity_sha256.as_str(),
        request.match_identity_sha256.as_str(),
        request.entry_authorization_sha256.as_str(),
        request.source_capture_commitment_sha256.as_str(),
        request.source_perception_result_commitment_sha256.as_str(),
        request.source_lifecycle_snapshot_commitment_sha256.as_str(),
        request.source_window_title_sha256.as_str(),
        request.source_event_label_region_sha256.as_str(),
        request.source_launch_identity_commitment_sha256.as_str(),
    ] {
        if !is_sha256_v2(value) {
            return Err(
                "attended competitive match launch contains an invalid commitment".to_owned(),
            );
        }
    }
    if request.event_identity_sha256 == request.match_identity_sha256
        || request.event_identity_sha256 == request.entry_authorization_sha256
        || request.match_identity_sha256 == request.entry_authorization_sha256
        || request.entry_authorization_sha256 == scope.written_permission_sha256
    {
        return Err(
            "attended event, match, entry, and permission records must be distinct".to_owned(),
        );
    }
    request
        .observed_frame_sequence
        .checked_add(ATTENDED_COMPETITIVE_MATCH_MAX_FRAME_ADVANCE_V4)
        .ok_or("attended competitive match frame lifetime overflow")?;
    Ok(mode_authorization_commitment_sha256)
}

fn attended_competitive_match_launch_confirmation_phrase_v4(
    event_kind: MtgoCompetitiveEventKindV1,
    game_number: u8,
    challenge_nonce: &[u8; 8],
) -> String {
    let nonce = challenge_nonce
        .iter()
        .map(|value| format!("{value:02X}"))
        .collect::<String>();
    format!(
        "AUTHORIZE MTGO {} GAME {game_number} {nonce}",
        competitive_event_kind_label_v4(event_kind).to_ascii_uppercase()
    )
}

fn attended_competitive_gesture_match_launch_confirmation_phrase_v1(
    event_kind: MtgoCompetitiveEventKindV1,
    game_number: u8,
    challenge_nonce: &[u8; 8],
) -> String {
    let nonce = challenge_nonce
        .iter()
        .map(|value| format!("{value:02X}"))
        .collect::<String>();
    format!(
        "AUTHORIZE MTGO {} GAME {game_number} ALL ELEVEN GESTURES {nonce}",
        competitive_event_kind_label_v4(event_kind).to_ascii_uppercase()
    )
}

fn competitive_event_kind_label_v4(event_kind: MtgoCompetitiveEventKindV1) -> &'static str {
    match event_kind {
        MtgoCompetitiveEventKindV1::League => "League",
        MtgoCompetitiveEventKindV1::Challenge => "Challenge",
    }
}

fn validate_attended_launch_display_label_v4(
    value: &str,
    max_bytes: usize,
    field: &str,
) -> Result<(), String> {
    if value.is_empty()
        || value.len() > max_bytes
        || value.trim() != value
        || !value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
    {
        return Err(format!(
            "attended {field} must be nonempty, trimmed, bounded ASCII display text"
        ));
    }
    Ok(())
}

fn expand_atomic_sideboard_transfers_v1(
    transfers: &[MtgoCompetitiveSideboardTransferV1],
) -> Result<Vec<MtgoAtomicCompetitiveSideboardTransferV1>, String> {
    let mut atomic = Vec::new();
    for direction in [
        MtgoCompetitiveSideboardTransferDirectionV1::SideboardToMainboard,
        MtgoCompetitiveSideboardTransferDirectionV1::MainboardToSideboard,
    ] {
        for transfer in transfers
            .iter()
            .filter(|transfer| transfer.direction == direction)
        {
            for _ in 0..transfer.count {
                let step_index = u16::try_from(atomic.len())
                    .map_err(|_| "atomic sideboard transfer count overflow")?;
                atomic.push(MtgoAtomicCompetitiveSideboardTransferV1 {
                    step_index,
                    card_name: transfer.card_name.clone(),
                    direction,
                });
            }
        }
    }
    Ok(atomic)
}

fn apply_atomic_sideboard_transfer_v1(
    current: &MtgoCompetitiveDeckConfigurationV1,
    transfer: &MtgoAtomicCompetitiveSideboardTransferV1,
) -> Result<MtgoCompetitiveDeckConfigurationV1, String> {
    let mut matching_card_db_ids = current
        .mainboard
        .iter()
        .chain(&current.sideboard)
        .filter(|card| card.card_name == transfer.card_name)
        .map(|card| card.card_db_id);
    let card_db_id = matching_card_db_ids
        .next()
        .ok_or("sideboard transfer visible card name is absent from the current configuration")?;
    if matching_card_db_ids.any(|candidate| candidate != card_db_id) {
        return Err(
            "sideboard transfer visible card name resolved to conflicting local identities"
                .to_owned(),
        );
    }
    let mut next = current.clone();
    match transfer.direction {
        MtgoCompetitiveSideboardTransferDirectionV1::SideboardToMainboard => {
            remove_one_sideboard_card_v1(&mut next.sideboard, card_db_id, &transfer.card_name)?;
            add_one_sideboard_card_v1(&mut next.mainboard, card_db_id, &transfer.card_name)?;
        }
        MtgoCompetitiveSideboardTransferDirectionV1::MainboardToSideboard => {
            remove_one_sideboard_card_v1(&mut next.mainboard, card_db_id, &transfer.card_name)?;
            add_one_sideboard_card_v1(&mut next.sideboard, card_db_id, &transfer.card_name)?;
        }
    }
    Ok(next)
}

fn remove_one_sideboard_card_v1(
    partition: &mut Vec<mtgo_blackbox_v1::MtgoCompetitiveDeckCardCountV1>,
    card_db_id: u16,
    card_name: &str,
) -> Result<(), String> {
    let index = partition
        .iter()
        .position(|card| card.card_db_id == card_db_id && card.card_name == card_name)
        .ok_or("sideboard transfer source card is absent from the current configuration")?;
    if partition[index].count == 0 {
        return Err("sideboard transfer source card has zero copies".to_owned());
    }
    partition[index].count -= 1;
    if partition[index].count == 0 {
        partition.remove(index);
    }
    Ok(())
}

fn add_one_sideboard_card_v1(
    partition: &mut Vec<mtgo_blackbox_v1::MtgoCompetitiveDeckCardCountV1>,
    card_db_id: u16,
    card_name: &str,
) -> Result<(), String> {
    match partition.binary_search_by_key(&card_db_id, |card| card.card_db_id) {
        Ok(index) => {
            if partition[index].card_name != card_name {
                return Err("sideboard destination card identity changed".to_owned());
            }
            partition[index].count = partition[index]
                .count
                .checked_add(1)
                .ok_or("sideboard destination card count overflow")?;
        }
        Err(index) => partition.insert(
            index,
            mtgo_blackbox_v1::MtgoCompetitiveDeckCardCountV1 {
                card_db_id,
                card_name: card_name.to_owned(),
                count: 1,
            },
        ),
    }
    Ok(())
}

fn competitive_sideboard_transfer_direction_tag_v1(
    direction: MtgoCompetitiveSideboardTransferDirectionV1,
) -> &'static [u8] {
    match direction {
        MtgoCompetitiveSideboardTransferDirectionV1::MainboardToSideboard => {
            b"mainboard_to_sideboard"
        }
        MtgoCompetitiveSideboardTransferDirectionV1::SideboardToMainboard => {
            b"sideboard_to_mainboard"
        }
    }
}

fn hash_parts_v2(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        update_hash_part_v3(&mut hasher, part);
    }
    format!("{:x}", hasher.finalize())
}

fn is_sha256_v2(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn competitive_duel_gesture_ratification_candidate_from_parts_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    visible_account_alias: &str,
    event_kind: MtgoCompetitiveEventKindV1,
    gesture_profile: &CompetitiveDuelGestureProfileFactsV1<'_>,
) -> Result<MtgoReviewedCompetitiveGestureRatificationCandidateV1, String> {
    let permission_review_commitment_sha256 = correspondence.review_commitment_sha256().to_owned();
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(event_kind)
        .map_err(|error| format!("derive reviewed competitive gesture mode scope: {error}"))?;
    let mode_authorization_commitment_sha256 = validate_exact_competitive_mode_authorization_v1(
        &scope,
        visible_account_alias,
        event_kind,
        "competitive gesture permission",
    )?;
    let canonical_families = canonical_duel_gesture_action_families_v1();
    if gesture_profile.supported_action_families != canonical_families.as_slice() {
        return Err(
            "competitive gesture permission requires all eleven action families in canonical order"
                .to_owned(),
        );
    }
    let profile_hashes = [
        gesture_profile.evaluation_commitment_sha256,
        gesture_profile.admission_commitment_sha256,
        gesture_profile.runtime_binary_sha256,
        gesture_profile.assets_manifest_sha256,
    ];
    if profile_hashes.iter().any(|value| !is_sha256_v2(value))
        || profile_hashes
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != profile_hashes.len()
    {
        return Err(
            "competitive gesture profile commitments must be valid and pairwise distinct"
                .to_owned(),
        );
    }
    let family_json = serde_json::to_vec(&canonical_families)
        .map_err(|error| format!("serialize competitive gesture families: {error}"))?;
    let event_kind_bytes: &[u8] = match event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let ratification_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_DUEL_GESTURE_AUTHORIZATION_FROM_REVIEW_DOMAIN_V1,
        &[
            permission_review_commitment_sha256.as_bytes(),
            scope.account_alias_sha256.as_bytes(),
            scope.written_permission_sha256.as_bytes(),
            visible_account_alias.as_bytes(),
            event_kind_bytes,
            mode_authorization_commitment_sha256.as_bytes(),
            gesture_profile.evaluation_commitment_sha256.as_bytes(),
            gesture_profile.admission_commitment_sha256.as_bytes(),
            gesture_profile.runtime_binary_sha256.as_bytes(),
            gesture_profile.assets_manifest_sha256.as_bytes(),
            &family_json,
            b"complete_reviewed_eleven_family_profile_permission_identity_only_no_input_entry_or_spending",
        ],
    );
    Ok(MtgoReviewedCompetitiveGestureRatificationCandidateV1 {
        permission_review_commitment_sha256,
        account_alias_sha256: scope.account_alias_sha256,
        correspondence_sha256: scope.written_permission_sha256,
        mode_authorization_commitment_sha256,
        gesture_evaluation_commitment_sha256: gesture_profile
            .evaluation_commitment_sha256
            .to_owned(),
        gesture_profile_admission_commitment_sha256: gesture_profile
            .admission_commitment_sha256
            .to_owned(),
        gesture_target_runtime_binary_sha256: gesture_profile.runtime_binary_sha256.to_owned(),
        gesture_target_assets_manifest_sha256: gesture_profile.assets_manifest_sha256.to_owned(),
        supported_action_families: canonical_families,
        ratification_commitment_sha256,
        event_kind,
    })
}

#[allow(clippy::too_many_arguments)]
fn bind_selected_listing_to_competitive_entry_review_from_parts_v1(
    open_authorization: &MtgoReviewedCompetitiveOpenEntryReviewRatificationCandidateV1,
    open_confirmation: &MtgoConfirmedCompetitiveOpenEntryReviewCommitmentsV1,
    open_visible: &MtgoCompetitiveEventListingOpenVisibleConfirmationCommitmentsV1,
    open_after_navigation: &MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1,
    open_after_window_continuity_commitment_sha256: &str,
    entry_review_lineage: &MtgoCompetitiveEntryFrameTransitionViewV1,
    entry_review_candidate: &MtgoReviewedCompetitiveEntryRatificationCandidateV1,
) -> Result<MtgoSelectedListingBoundCompetitiveEntryReviewCommitmentsV1, String> {
    let open_after_capture = &open_after_navigation.source_frame.source_capture;
    if open_confirmation.authorization_ratification_commitment_sha256
        != open_authorization.ratification_commitment_sha256
        || open_confirmation.visible_confirmation != *open_visible
        || open_confirmation.event_kind != open_authorization.event_kind
        || open_confirmation.event_kind != open_visible.event_kind
        || open_confirmation.event_identity_sha256 != open_visible.event_identity_sha256
        || open_confirmation.after_frame_id != open_visible.after_frame_id
        || open_confirmation.after_frame_sequence != open_visible.after_frame_sequence
        || open_confirmation.after_captured_at_unix_millis
            != open_visible.after_captured_at_unix_millis
        || open_visible.after_capture_commitment_sha256
            != open_after_capture.capture_commitment_sha256
        || open_visible.after_classification_result_commitment_sha256
            != open_after_navigation.classification_result_commitment_sha256
        || open_visible.after_lifecycle_snapshot_commitment_sha256
            != open_after_navigation.lifecycle_snapshot_commitment_sha256
        || open_visible.after_frame_id != open_after_navigation.frame_id
        || open_visible.after_frame_sequence != open_after_navigation.frame_sequence
        || open_visible.after_captured_at_unix_millis != open_after_capture.captured_at_unix_millis
        || open_after_navigation.phase != MtgoCompetitiveLifecyclePhaseV1::EntryReview
        || open_after_navigation.event_kind != open_authorization.event_kind
    {
        return Err(
            "selected-listing entry bridge changed the confirmed Open Entry Review arrival"
                .to_owned(),
        );
    }
    if open_after_navigation.source_frame.profile_commitment_sha256
        != open_authorization.navigation_profile_commitment_sha256
        || open_after_navigation
            .source_frame
            .profile_admission_commitment_sha256
            != open_authorization.navigation_profile_admission_commitment_sha256
        || open_after_navigation
            .source_frame
            .approved_account_alias_sha256
            != open_authorization.approved_account_alias_sha256
        || entry_review_lineage.navigation_profile_commitment_sha256
            != open_authorization.navigation_profile_commitment_sha256
        || entry_review_lineage.navigation_profile_admission_commitment_sha256
            != open_authorization.navigation_profile_admission_commitment_sha256
        || entry_review_lineage.approved_account_alias_sha256
            != open_authorization.approved_account_alias_sha256
        || entry_review_lineage.runtime_identity_commitment_sha256
            != open_after_navigation.runtime_identity_commitment_sha256
        || entry_review_lineage.window_continuity_commitment_sha256
            != open_after_window_continuity_commitment_sha256
    {
        return Err(
            "selected-listing entry bridge changed account, profile, runtime, process, window, geometry, or output"
                .to_owned(),
        );
    }
    if entry_review_lineage.phase != MtgoCompetitiveLifecyclePhaseV1::EntryReview
        || entry_review_lineage.event_kind != open_authorization.event_kind
        || entry_review_lineage.event_identity_sha256 != open_visible.event_identity_sha256
        || entry_review_lineage.frame_id == open_visible.after_frame_id
        || entry_review_lineage.frame_sequence <= open_visible.after_frame_sequence
        || entry_review_lineage.captured_at_unix_millis
            <= open_visible.after_captured_at_unix_millis
        || entry_review_lineage.capture_commitment_sha256
            == open_visible.after_capture_commitment_sha256
    {
        return Err(
            "selected-listing entry bridge requires a strictly newer visible Entry Review for the same exact event"
                .to_owned(),
        );
    }
    if entry_review_candidate.correspondence_sha256 != open_authorization.correspondence_sha256
        || entry_review_candidate.permission_review_commitment_sha256
            != open_authorization.permission_review_commitment_sha256
        || entry_review_candidate.mode_authorization_commitment_sha256
            != open_authorization.mode_authorization_commitment_sha256
        || entry_review_candidate.account_alias_sha256
            != open_authorization.approved_account_alias_sha256
        || entry_review_candidate.event_kind != open_authorization.event_kind
        || entry_review_candidate.event_identity_sha256 != open_visible.event_identity_sha256
        || entry_review_candidate.deck_manifest_sha256
            != open_authorization.deck_manifest_commitment_sha256
        || entry_review_candidate.deck_format_sha256 != open_authorization.deck_format_sha256
        || entry_review_candidate.policy_deployment_commitment_sha256
            != open_authorization.policy_deployment_commitment_sha256
        || entry_review_candidate.source_capture_commitment_sha256
            != entry_review_lineage.capture_commitment_sha256
    {
        return Err(
            "selected-listing entry bridge changed correspondence, account, mode, event, deck, policy, or source capture"
                .to_owned(),
        );
    }
    for digest in [
        open_authorization.ratification_commitment_sha256.as_str(),
        open_authorization.correspondence_sha256.as_str(),
        open_authorization
            .permission_review_commitment_sha256
            .as_str(),
        open_authorization
            .mode_authorization_commitment_sha256
            .as_str(),
        open_authorization.approved_account_alias_sha256.as_str(),
        open_authorization
            .navigation_profile_commitment_sha256
            .as_str(),
        open_authorization
            .navigation_profile_admission_commitment_sha256
            .as_str(),
        open_authorization
            .listing_evaluation_admission_commitment_sha256
            .as_str(),
        open_authorization.deck_list_sha256.as_str(),
        open_authorization.deck_manifest_commitment_sha256.as_str(),
        open_authorization.deck_format_sha256.as_str(),
        open_authorization
            .policy_deployment_commitment_sha256
            .as_str(),
        open_confirmation.confirmation_receipt_sha256.as_str(),
        open_visible.visible_confirmation_commitment_sha256.as_str(),
        entry_review_candidate
            .control_bound_review_commitment_sha256
            .as_str(),
        entry_review_candidate
            .ratification_commitment_sha256
            .as_str(),
        entry_review_candidate.entry_terms_sha256.as_str(),
        entry_review_lineage.capture_commitment_sha256.as_str(),
        open_after_window_continuity_commitment_sha256,
    ] {
        if !is_sha256_v2(digest) {
            return Err("selected-listing entry bridge contains an invalid commitment".to_owned());
        }
    }
    let event_kind = competitive_event_kind_tag_v1(open_authorization.event_kind);
    let resource: &[u8] = match entry_review_candidate.resource {
        MtgoCompetitiveEntryResourceV1::NoCost => b"no_cost",
        MtgoCompetitiveEntryResourceV1::ExistingEventToken => b"existing_event_token",
        MtgoCompetitiveEntryResourceV1::ExistingPlayPoints => b"existing_play_points",
        MtgoCompetitiveEntryResourceV1::ExistingEventTickets => b"existing_event_tickets",
    };
    let binding_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_SELECTED_LISTING_ENTRY_REVIEW_BINDING_DOMAIN_V1,
        &[
            open_authorization.ratification_commitment_sha256.as_bytes(),
            open_confirmation.confirmation_receipt_sha256.as_bytes(),
            open_visible.visible_confirmation_commitment_sha256.as_bytes(),
            entry_review_candidate
                .control_bound_review_commitment_sha256
                .as_bytes(),
            entry_review_candidate
                .ratification_commitment_sha256
                .as_bytes(),
            open_authorization.correspondence_sha256.as_bytes(),
            open_authorization
                .permission_review_commitment_sha256
                .as_bytes(),
            open_authorization
                .mode_authorization_commitment_sha256
                .as_bytes(),
            open_authorization.approved_account_alias_sha256.as_bytes(),
            open_authorization
                .navigation_profile_commitment_sha256
                .as_bytes(),
            open_authorization
                .navigation_profile_admission_commitment_sha256
                .as_bytes(),
            open_authorization
                .listing_evaluation_admission_commitment_sha256
                .as_bytes(),
            event_kind,
            open_visible.event_identity_sha256.as_bytes(),
            open_authorization.deck_list_sha256.as_bytes(),
            open_authorization
                .deck_manifest_commitment_sha256
                .as_bytes(),
            open_authorization.deck_format_sha256.as_bytes(),
            open_authorization
                .policy_deployment_commitment_sha256
                .as_bytes(),
            entry_review_candidate.entry_terms_sha256.as_bytes(),
            resource,
            entry_review_candidate.amount.to_be_bytes().as_slice(),
            open_visible.after_frame_id.to_be_bytes().as_slice(),
            open_visible.after_frame_sequence.to_be_bytes().as_slice(),
            open_visible
                .after_captured_at_unix_millis
                .to_be_bytes()
                .as_slice(),
            entry_review_lineage.frame_id.to_be_bytes().as_slice(),
            entry_review_lineage.frame_sequence.to_be_bytes().as_slice(),
            entry_review_lineage
                .captured_at_unix_millis
                .to_be_bytes()
                .as_slice(),
            entry_review_lineage.capture_commitment_sha256.as_bytes(),
            open_after_window_continuity_commitment_sha256.as_bytes(),
            b"selected_listing_visible_arrival_to_fresh_exact_paid_entry_review_no_entry_no_spending_no_input",
        ],
    );
    Ok(
        MtgoSelectedListingBoundCompetitiveEntryReviewCommitmentsV1 {
            open_entry_review_authorization_ratification_commitment_sha256: open_authorization
                .ratification_commitment_sha256
                .clone(),
            open_entry_review_confirmation_receipt_sha256: open_confirmation
                .confirmation_receipt_sha256
                .clone(),
            open_entry_review_visible_confirmation_commitment_sha256: open_visible
                .visible_confirmation_commitment_sha256
                .clone(),
            control_bound_entry_review_commitment_sha256: entry_review_candidate
                .control_bound_review_commitment_sha256
                .clone(),
            legacy_entry_ratification_candidate_commitment_sha256: entry_review_candidate
                .ratification_commitment_sha256
                .clone(),
            correspondence_sha256: open_authorization.correspondence_sha256.clone(),
            permission_review_commitment_sha256: open_authorization
                .permission_review_commitment_sha256
                .clone(),
            mode_authorization_commitment_sha256: open_authorization
                .mode_authorization_commitment_sha256
                .clone(),
            approved_account_alias_sha256: open_authorization.approved_account_alias_sha256.clone(),
            navigation_profile_commitment_sha256: open_authorization
                .navigation_profile_commitment_sha256
                .clone(),
            navigation_profile_admission_commitment_sha256: open_authorization
                .navigation_profile_admission_commitment_sha256
                .clone(),
            listing_evaluation_admission_commitment_sha256: open_authorization
                .listing_evaluation_admission_commitment_sha256
                .clone(),
            event_kind: open_authorization.event_kind,
            event_identity_sha256: open_visible.event_identity_sha256.clone(),
            deck_list_sha256: open_authorization.deck_list_sha256.clone(),
            deck_manifest_commitment_sha256: open_authorization
                .deck_manifest_commitment_sha256
                .clone(),
            deck_format_sha256: open_authorization.deck_format_sha256.clone(),
            policy_deployment_commitment_sha256: open_authorization
                .policy_deployment_commitment_sha256
                .clone(),
            entry_terms_sha256: entry_review_candidate.entry_terms_sha256.clone(),
            resource: entry_review_candidate.resource,
            amount: entry_review_candidate.amount,
            open_arrival_frame_id: open_visible.after_frame_id,
            open_arrival_frame_sequence: open_visible.after_frame_sequence,
            open_arrival_captured_at_unix_millis: open_visible.after_captured_at_unix_millis,
            entry_review_frame_id: entry_review_lineage.frame_id,
            entry_review_frame_sequence: entry_review_lineage.frame_sequence,
            entry_review_captured_at_unix_millis: entry_review_lineage.captured_at_unix_millis,
            entry_review_source_capture_commitment_sha256: entry_review_lineage
                .capture_commitment_sha256
                .clone(),
            entry_review_window_continuity_commitment_sha256:
                open_after_window_continuity_commitment_sha256.to_owned(),
            binding_commitment_sha256,
        },
    )
}

fn selected_listing_competitive_entry_ratification_candidate_from_parts_v2(
    binding: &MtgoSelectedListingBoundCompetitiveEntryReviewCommitmentsV1,
    entry_review_candidate: &MtgoReviewedCompetitiveEntryRatificationCandidateV1,
) -> Result<MtgoReviewedSelectedListingCompetitiveEntryRatificationCandidateV2, String> {
    if binding.legacy_entry_ratification_candidate_commitment_sha256
        != entry_review_candidate.ratification_commitment_sha256
        || binding.control_bound_entry_review_commitment_sha256
            != entry_review_candidate.control_bound_review_commitment_sha256
        || binding.correspondence_sha256 != entry_review_candidate.correspondence_sha256
        || binding.permission_review_commitment_sha256
            != entry_review_candidate.permission_review_commitment_sha256
        || binding.mode_authorization_commitment_sha256
            != entry_review_candidate.mode_authorization_commitment_sha256
        || binding.approved_account_alias_sha256 != entry_review_candidate.account_alias_sha256
        || binding.event_kind != entry_review_candidate.event_kind
        || binding.event_identity_sha256 != entry_review_candidate.event_identity_sha256
        || binding.deck_manifest_commitment_sha256 != entry_review_candidate.deck_manifest_sha256
        || binding.deck_format_sha256 != entry_review_candidate.deck_format_sha256
        || binding.policy_deployment_commitment_sha256
            != entry_review_candidate.policy_deployment_commitment_sha256
        || binding.entry_terms_sha256 != entry_review_candidate.entry_terms_sha256
        || binding.resource != entry_review_candidate.resource
        || binding.amount != entry_review_candidate.amount
    {
        return Err(
            "selected-listing entry ratification changed the bound paid Entry Review".to_owned(),
        );
    }
    if !is_sha256_v2(&binding.binding_commitment_sha256)
        || !is_sha256_v2(&entry_review_candidate.ratification_commitment_sha256)
    {
        return Err(
            "selected-listing entry ratification contains an invalid commitment".to_owned(),
        );
    }
    let event_kind = competitive_event_kind_tag_v1(binding.event_kind);
    let resource: &[u8] = match binding.resource {
        MtgoCompetitiveEntryResourceV1::NoCost => b"no_cost",
        MtgoCompetitiveEntryResourceV1::ExistingEventToken => b"existing_event_token",
        MtgoCompetitiveEntryResourceV1::ExistingPlayPoints => b"existing_play_points",
        MtgoCompetitiveEntryResourceV1::ExistingEventTickets => b"existing_event_tickets",
    };
    let ratification_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_SELECTED_LISTING_ENTRY_AUTHORIZATION_RATIFICATION_DOMAIN_V2,
        &[
            binding.binding_commitment_sha256.as_bytes(),
            entry_review_candidate
                .ratification_commitment_sha256
                .as_bytes(),
            binding.open_entry_review_authorization_ratification_commitment_sha256.as_bytes(),
            binding.open_entry_review_confirmation_receipt_sha256.as_bytes(),
            binding.approved_account_alias_sha256.as_bytes(),
            event_kind,
            binding.event_identity_sha256.as_bytes(),
            binding.deck_list_sha256.as_bytes(),
            binding.deck_manifest_commitment_sha256.as_bytes(),
            binding.deck_format_sha256.as_bytes(),
            binding.policy_deployment_commitment_sha256.as_bytes(),
            binding.entry_terms_sha256.as_bytes(),
            resource,
            binding.amount.to_be_bytes().as_slice(),
            b"exact_selected_listing_bound_owner_reviewed_entry_requires_fresh_recapture_and_visible_postcondition",
        ],
    );
    Ok(
        MtgoReviewedSelectedListingCompetitiveEntryRatificationCandidateV2 {
            selected_listing_binding_commitment_sha256: binding.binding_commitment_sha256.clone(),
            entry_review_candidate: entry_review_candidate.clone(),
            ratification_commitment_sha256,
        },
    )
}

fn competitive_entry_ratification_candidate_from_parts_v1(
    correspondence: &CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    visible_account_alias: &str,
    lifecycle: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    source_identity: &MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1,
    entry_authorization: &MtgoCompetitiveEntryAuthorizationV1,
    review: &MtgoControlBoundCompetitiveEntryReviewCommitmentsV4,
) -> Result<MtgoReviewedCompetitiveEntryRatificationCandidateV1, String> {
    let classifier = &review.classifier_bound_review;
    let source_bound = &classifier.source_bound_review;
    let attended = &source_bound.attended_review;
    let dry_run = &review.entry_control_dry_run;
    let retained_classifier_commitment =
        require_classifier_bound_competitive_entry_source_v3(source_identity)?;
    if classifier.source_navigation_classification_result_commitment_sha256
        != retained_classifier_commitment
        || source_bound.source_identity_commitment_sha256
            != source_identity.source_identity_commitment_sha256
        || source_bound.source_capture_commitment_sha256
            != source_identity.source_capture_commitment_sha256
        || source_bound
            .source_navigation_classification_result_commitment_sha256
            .as_deref()
            != Some(retained_classifier_commitment)
        || source_bound
            .attended_review
            .source_lifecycle_snapshot_commitment_sha256
            != source_identity.source_lifecycle_snapshot_commitment_sha256
        || source_identity.source_lifecycle_snapshot_commitment_sha256
            != lifecycle.snapshot_commitment_sha256()
        || source_identity.event_kind != lifecycle.event_kind()
        || source_identity.frame_id != lifecycle.frame_id_v1()
        || source_identity.frame_sequence != lifecycle.frame_sequence()
    {
        return Err("competitive entry ratification changed the exact source lineage".to_owned());
    }
    let expected_classifier_bound = classifier_bound_competitive_entry_review_commitment_v3(
        source_bound,
        retained_classifier_commitment,
    );
    if classifier.classifier_bound_review_commitment_sha256 != expected_classifier_bound {
        return Err(
            "competitive entry ratification changed the classifier-bound review".to_owned(),
        );
    }
    let expected_control_bound = bind_control_bound_competitive_entry_review_commitment_v4(
        classifier,
        dry_run,
        &review.deck_review_receipt_sha256,
    )?;
    if review.control_bound_review_commitment_sha256 != expected_control_bound {
        return Err("competitive entry ratification changed the control-bound review".to_owned());
    }

    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(attended.event_kind)
        .map_err(|error| format!("derive reviewed competitive entry mode scope: {error}"))?;
    let mode_authorization_commitment_sha256 = validate_competitive_duel_pass_authorization_v1(
        &scope,
        visible_account_alias,
        attended.event_kind,
    )?;
    if attended.permission_review_commitment_sha256 != correspondence.review_commitment_sha256()
        || attended.mode_authorization_commitment_sha256 != mode_authorization_commitment_sha256
        || attended.event_kind != lifecycle.event_kind()
        || attended.frame_id != lifecycle.frame_id_v1()
        || attended.frame_sequence != lifecycle.frame_sequence()
    {
        return Err(
            "competitive entry ratification changed the correspondence, account, mode, or frame"
                .to_owned(),
        );
    }
    let lifecycle_intent = make_offline_competitive_lifecycle_intent_v1(
        lifecycle,
        MtgoCompetitiveLifecycleActionV1::ConfirmEntry,
        &scope,
        Some(entry_authorization),
    )
    .map_err(|error| format!("validate reviewed competitive entry authorization: {error}"))?;
    let entry_authorization_sha256 = lifecycle_intent
        .entry_authorization_sha256
        .ok_or("competitive entry ratification has no exact entry authorization")?;
    let event_identity_sha256 = lifecycle
        .event_identity_sha256_v1()
        .ok_or("competitive entry ratification has no exact event identity")?
        .to_owned();
    let entry_terms = lifecycle
        .entry_terms_v1()
        .ok_or("competitive entry ratification has no exact visible entry terms")?;
    if attended.entry_authorization_sha256 != entry_authorization_sha256
        || attended.resource != entry_terms.resource
        || attended.amount != entry_terms.amount
        || source_identity.resource != entry_terms.resource
        || source_identity.amount != entry_terms.amount
        || dry_run.event_kind != attended.event_kind
        || dry_run.frame_id != attended.frame_id
        || dry_run.frame_sequence != attended.frame_sequence
        || dry_run.resource != entry_terms.resource
        || dry_run.amount != entry_terms.amount
        || !dry_run.visibly_enabled_confirmed
    {
        return Err(
            "competitive entry ratification changed the visible event, terms, or enabled control"
                .to_owned(),
        );
    }
    for digest in [
        correspondence.review_commitment_sha256(),
        scope.account_alias_sha256.as_str(),
        scope.written_permission_sha256.as_str(),
        mode_authorization_commitment_sha256.as_str(),
        review.control_bound_review_commitment_sha256.as_str(),
        review.deck_review_receipt_sha256.as_str(),
        classifier
            .classifier_bound_review_commitment_sha256
            .as_str(),
        attended.owner_review_receipt_sha256.as_str(),
        entry_authorization_sha256.as_str(),
        source_identity.source_identity_commitment_sha256.as_str(),
        source_identity.source_capture_commitment_sha256.as_str(),
        retained_classifier_commitment,
        source_identity
            .source_lifecycle_snapshot_commitment_sha256
            .as_str(),
        dry_run.dry_run_commitment_sha256.as_str(),
        dry_run.visible_control_label_sha256.as_str(),
        dry_run.visible_control_region_sha256.as_str(),
        dry_run.selected_deck_label_sha256.as_str(),
        dry_run.selected_deck_region_sha256.as_str(),
        dry_run.deck_manifest_sha256.as_str(),
        dry_run.deck_format_sha256.as_str(),
        dry_run.policy_deployment_commitment_sha256.as_str(),
        event_identity_sha256.as_str(),
        entry_terms.terms_sha256.as_str(),
    ] {
        if !is_sha256_v2(digest) {
            return Err("competitive entry ratification contains an invalid commitment".to_owned());
        }
    }
    let event_kind: &[u8] = match attended.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let resource: &[u8] = match entry_terms.resource {
        MtgoCompetitiveEntryResourceV1::NoCost => b"no_cost",
        MtgoCompetitiveEntryResourceV1::ExistingEventToken => b"existing_event_token",
        MtgoCompetitiveEntryResourceV1::ExistingPlayPoints => b"existing_play_points",
        MtgoCompetitiveEntryResourceV1::ExistingEventTickets => b"existing_event_tickets",
    };
    let ratification_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_ENTRY_AUTHORIZATION_RATIFICATION_DOMAIN_V1,
        &[
            correspondence.review_commitment_sha256().as_bytes(),
            scope.account_alias_sha256.as_bytes(),
            scope.written_permission_sha256.as_bytes(),
            mode_authorization_commitment_sha256.as_bytes(),
            review.control_bound_review_commitment_sha256.as_bytes(),
            review.deck_review_receipt_sha256.as_bytes(),
            classifier
                .classifier_bound_review_commitment_sha256
                .as_bytes(),
            attended.owner_review_receipt_sha256.as_bytes(),
            entry_authorization_sha256.as_bytes(),
            source_identity.source_identity_commitment_sha256.as_bytes(),
            source_identity.source_capture_commitment_sha256.as_bytes(),
            retained_classifier_commitment.as_bytes(),
            source_identity
                .source_lifecycle_snapshot_commitment_sha256
                .as_bytes(),
            dry_run.dry_run_commitment_sha256.as_bytes(),
            dry_run.visible_control_label_sha256.as_bytes(),
            dry_run.visible_control_region_sha256.as_bytes(),
            dry_run.selected_deck_label_sha256.as_bytes(),
            dry_run.selected_deck_region_sha256.as_bytes(),
            dry_run.deck_manifest_sha256.as_bytes(),
            dry_run.deck_format_sha256.as_bytes(),
            dry_run.policy_deployment_commitment_sha256.as_bytes(),
            event_kind,
            event_identity_sha256.as_bytes(),
            entry_terms.terms_sha256.as_bytes(),
            resource,
            entry_terms.amount.to_be_bytes().as_slice(),
            b"exact_owner_reviewed_existing_account_resource_and_selected_deck_entry_requires_fresh_recapture_and_visible_postcondition",
        ],
    );
    Ok(MtgoReviewedCompetitiveEntryRatificationCandidateV1 {
        permission_review_commitment_sha256: correspondence.review_commitment_sha256().to_owned(),
        account_alias_sha256: scope.account_alias_sha256,
        correspondence_sha256: scope.written_permission_sha256,
        mode_authorization_commitment_sha256,
        control_bound_review_commitment_sha256: review
            .control_bound_review_commitment_sha256
            .clone(),
        owner_review_receipt_sha256: attended.owner_review_receipt_sha256.clone(),
        entry_authorization_sha256,
        source_identity_commitment_sha256: source_identity
            .source_identity_commitment_sha256
            .clone(),
        source_capture_commitment_sha256: source_identity.source_capture_commitment_sha256.clone(),
        source_navigation_classification_result_commitment_sha256: retained_classifier_commitment
            .to_owned(),
        visible_control_region_sha256: dry_run.visible_control_region_sha256.clone(),
        selected_deck_label_sha256: dry_run.selected_deck_label_sha256.clone(),
        selected_deck_region_sha256: dry_run.selected_deck_region_sha256.clone(),
        deck_manifest_sha256: dry_run.deck_manifest_sha256.clone(),
        deck_format_sha256: dry_run.deck_format_sha256.clone(),
        policy_deployment_commitment_sha256: dry_run.policy_deployment_commitment_sha256.clone(),
        deck_review_receipt_sha256: review.deck_review_receipt_sha256.clone(),
        event_identity_sha256,
        entry_terms_sha256: entry_terms.terms_sha256.clone(),
        ratification_commitment_sha256,
        event_kind: attended.event_kind,
        resource: entry_terms.resource,
        amount: entry_terms.amount,
    })
}

fn competitive_entry_preparation_from_commitments_v1(
    authorization: &MtgoReviewedCompetitiveEntryRatificationCandidateV1,
    recapture: &MtgoCompetitiveEntryImmediateRecaptureCommitmentsV1,
) -> Result<MtgoPreparedCompetitiveEntryCommitmentsV1, String> {
    if authorization.account_alias_sha256 != recapture.approved_account_alias_sha256
        || authorization.source_identity_commitment_sha256
            != recapture.source_identity_commitment_sha256
        || authorization.source_capture_commitment_sha256
            != recapture.source_capture_commitment_sha256
        || authorization.source_navigation_classification_result_commitment_sha256
            != recapture.source_classification_result_commitment_sha256
        || authorization.visible_control_region_sha256 != recapture.visible_control_region_sha256
        || authorization.selected_deck_label_sha256 != recapture.selected_deck_label_sha256
        || authorization.selected_deck_region_sha256 != recapture.selected_deck_region_sha256
        || authorization.deck_manifest_sha256 != recapture.deck_manifest_sha256
        || authorization.deck_format_sha256 != recapture.deck_format_sha256
        || authorization.policy_deployment_commitment_sha256
            != recapture.policy_deployment_commitment_sha256
        || authorization.event_identity_sha256 != recapture.event_identity_sha256
        || authorization.entry_terms_sha256 != recapture.entry_terms_sha256
        || authorization.event_kind != recapture.event_kind
        || authorization.resource != recapture.resource
        || authorization.amount != recapture.amount
    {
        return Err(
            "competitive entry preparation changed the ratified account, source, control, selected deck, event, or terms"
                .to_owned(),
        );
    }
    if recapture.source_frame_id == 0
        || recapture.source_frame_sequence == 0
        || recapture.source_captured_at_unix_millis == 0
        || recapture.immediate_frame_id == 0
        || recapture.immediate_frame_sequence <= recapture.source_frame_sequence
        || recapture.immediate_captured_at_unix_millis <= recapture.source_captured_at_unix_millis
    {
        return Err(
            "competitive entry preparation does not retain a strictly newer frame identity"
                .to_owned(),
        );
    }
    for digest in [
        authorization.ratification_commitment_sha256.as_str(),
        recapture.navigation_profile_commitment_sha256.as_str(),
        recapture
            .navigation_profile_admission_commitment_sha256
            .as_str(),
        recapture.approved_account_alias_sha256.as_str(),
        recapture.runtime_identity_commitment_sha256.as_str(),
        recapture.window_continuity_commitment_sha256.as_str(),
        recapture.source_identity_commitment_sha256.as_str(),
        recapture.source_capture_commitment_sha256.as_str(),
        recapture
            .source_classification_result_commitment_sha256
            .as_str(),
        recapture
            .source_lifecycle_snapshot_commitment_sha256
            .as_str(),
        recapture.immediate_capture_commitment_sha256.as_str(),
        recapture
            .immediate_classification_result_commitment_sha256
            .as_str(),
        recapture
            .immediate_lifecycle_snapshot_commitment_sha256
            .as_str(),
        recapture.event_label_region_sha256.as_str(),
        recapture.visible_control_label_sha256.as_str(),
        recapture.visible_control_region_sha256.as_str(),
        recapture.selected_deck_label_sha256.as_str(),
        recapture.selected_deck_region_sha256.as_str(),
        recapture.deck_manifest_sha256.as_str(),
        recapture.deck_format_sha256.as_str(),
        recapture.policy_deployment_commitment_sha256.as_str(),
        recapture.event_identity_sha256.as_str(),
        recapture.entry_terms_sha256.as_str(),
        recapture.recapture_commitment_sha256.as_str(),
    ] {
        if !is_sha256_v2(digest) {
            return Err("competitive entry preparation contains an invalid commitment".to_owned());
        }
    }
    let event_kind: &[u8] = match authorization.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let resource: &[u8] = match authorization.resource {
        MtgoCompetitiveEntryResourceV1::NoCost => b"no_cost",
        MtgoCompetitiveEntryResourceV1::ExistingEventToken => b"existing_event_token",
        MtgoCompetitiveEntryResourceV1::ExistingPlayPoints => b"existing_play_points",
        MtgoCompetitiveEntryResourceV1::ExistingEventTickets => b"existing_event_tickets",
    };
    let preparation_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_ENTRY_PREPARATION_DOMAIN_V1,
        &[
            authorization.ratification_commitment_sha256.as_bytes(),
            recapture.recapture_commitment_sha256.as_bytes(),
            recapture.navigation_profile_commitment_sha256.as_bytes(),
            recapture
                .navigation_profile_admission_commitment_sha256
                .as_bytes(),
            recapture.approved_account_alias_sha256.as_bytes(),
            recapture.runtime_identity_commitment_sha256.as_bytes(),
            recapture.window_continuity_commitment_sha256.as_bytes(),
            recapture.source_identity_commitment_sha256.as_bytes(),
            recapture.source_capture_commitment_sha256.as_bytes(),
            recapture
                .source_classification_result_commitment_sha256
                .as_bytes(),
            recapture
                .source_lifecycle_snapshot_commitment_sha256
                .as_bytes(),
            recapture.immediate_capture_commitment_sha256.as_bytes(),
            recapture
                .immediate_classification_result_commitment_sha256
                .as_bytes(),
            recapture
                .immediate_lifecycle_snapshot_commitment_sha256
                .as_bytes(),
            recapture.event_label_region_sha256.as_bytes(),
            recapture.visible_control_label_sha256.as_bytes(),
            recapture.visible_control_region_sha256.as_bytes(),
            recapture.selected_deck_label_sha256.as_bytes(),
            recapture.selected_deck_region_sha256.as_bytes(),
            recapture.deck_manifest_sha256.as_bytes(),
            recapture.deck_format_sha256.as_bytes(),
            recapture.policy_deployment_commitment_sha256.as_bytes(),
            event_kind,
            recapture.event_identity_sha256.as_bytes(),
            recapture.entry_terms_sha256.as_bytes(),
            resource,
            authorization.amount.to_be_bytes().as_slice(),
            recapture.source_frame_id.to_be_bytes().as_slice(),
            recapture.source_frame_sequence.to_be_bytes().as_slice(),
            recapture
                .source_captured_at_unix_millis
                .to_be_bytes()
                .as_slice(),
            recapture.immediate_frame_id.to_be_bytes().as_slice(),
            recapture.immediate_frame_sequence.to_be_bytes().as_slice(),
            recapture
                .immediate_captured_at_unix_millis
                .to_be_bytes()
                .as_slice(),
            b"ratified_exact_entry_fresh_visible_review_retained_no_input_no_join_no_spending",
        ],
    );
    Ok(MtgoPreparedCompetitiveEntryCommitmentsV1 {
        entry_ratification_commitment_sha256: authorization.ratification_commitment_sha256.clone(),
        immediate_recapture: recapture.clone(),
        preparation_commitment_sha256,
        event_kind: authorization.event_kind,
        resource: authorization.resource,
        amount: authorization.amount,
        immediate_frame_id: recapture.immediate_frame_id,
        immediate_frame_sequence: recapture.immediate_frame_sequence,
    })
}

fn ratify_competitive_match_launch_with_commitment_v1(
    scope: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
    authorization: MtgoCompetitiveMatchGameplayAuthorizationV1,
    ratified_commitment_sha256: Option<&str>,
) -> Result<RatifiedMtgoCompetitiveMatchLaunchV1, String> {
    let mode_authorization_commitment_sha256 = validate_competitive_duel_pass_authorization_v1(
        scope,
        visible_account_alias,
        authorization.event_kind,
    )?;
    validate_competitive_match_launch_record_v1(scope, &authorization)?;
    let gameplay_authorization_commitment_sha256 =
        competitive_match_gameplay_authorization_commitment_v1(&authorization)
            .map_err(|error| format!("competitive match authorization commitment: {error}"))?;
    let launch_authorization_commitment_sha256 = competitive_match_launch_commitment_v1(
        visible_account_alias,
        &mode_authorization_commitment_sha256,
        &gameplay_authorization_commitment_sha256,
        &authorization,
    );
    if ratified_commitment_sha256 != Some(launch_authorization_commitment_sha256.as_str()) {
        return Err(
            "the exact competitive match owner launch is not ratified in this build".to_owned(),
        );
    }
    Ok(RatifiedMtgoCompetitiveMatchLaunchV1 {
        authorization,
        mode_authorization_commitment_sha256,
        gameplay_authorization_commitment_sha256,
        launch_authorization_commitment_sha256,
        valid_from_frame_sequence: 1,
    })
}

fn ratify_private_match_authorization_with_commitment_v3(
    scope: MtgoAuthorizationScopeV1,
    visible_account_alias: String,
    ratified_commitment_sha256: Option<&str>,
) -> Result<RatifiedMtgoPrivateMatchAuthorizationV3, String> {
    validate_private_match_authorization_v3(&scope, &visible_account_alias)?;
    if scope.shadow_observation
        || scope.open_play_input
        || scope.league_input
        || scope.challenge_input
        || scope.other_prize_event_input
    {
        return Err(
            "the private-match actuator requires a scope with every other runtime mode disabled"
                .to_owned(),
        );
    }
    let authorization_commitment_sha256 =
        private_match_authorization_commitment_v3(&scope, &visible_account_alias);
    if ratified_commitment_sha256 != Some(authorization_commitment_sha256.as_str()) {
        return Err(
            "the exact private-match permission correspondence is not ratified in this build"
                .to_owned(),
        );
    }
    Ok(RatifiedMtgoPrivateMatchAuthorizationV3 {
        scope,
        visible_account_alias,
        authorization_commitment_sha256,
    })
}

pub fn confirm_pending_pregame_mulligan_v3(
    pending: OpaqueMtgoPendingPregameInputV3,
    after: OpaqueMtgoDxgiMulliganMeasurementV3,
) -> Result<OpaqueMtgoConfirmedMulliganTransitionV3, String> {
    require_matching_pending_v3(&pending.input_receipt_sha256)?;
    match confirm_pregame_mulligan_transition_v3(pending.plan, after) {
        Ok(confirmed) => {
            release_confirmed_pending_v3(&pending.input_receipt_sha256)?;
            Ok(confirmed)
        }
        Err(error) => {
            halt_gate_v3()?;
            Err(format!(
                "pregame Mulligan postcondition failed and the input gate is halted: {error}"
            ))
        }
    }
}

pub fn confirm_pending_pregame_keep_to_bottom_six_v3(
    pending: OpaqueMtgoPendingPregameInputV3,
    after: OpaqueMtgoDxgiBottomSixInitialMeasurementV3,
) -> Result<OpaqueMtgoConfirmedKeepToBottomSixTransitionV3, String> {
    require_matching_pending_v3(&pending.input_receipt_sha256)?;
    match confirm_pregame_keep_to_bottom_six_transition_v3(pending.plan, after) {
        Ok(confirmed) => {
            release_confirmed_pending_v3(&pending.input_receipt_sha256)?;
            Ok(confirmed)
        }
        Err(error) => {
            halt_gate_v3()?;
            Err(format!(
                "pregame Keep-to-bottom-six postcondition failed and the input gate is halted: {error}"
            ))
        }
    }
}

pub fn confirm_pending_pregame_keep_to_first_main_v3(
    pending: OpaqueMtgoPendingPregameInputV3,
    after: OpaqueMtgoDxgiFirstMainMeasurementV3,
) -> Result<OpaqueMtgoConfirmedKeepToFirstMainTransitionV3, String> {
    require_matching_pending_v3(&pending.input_receipt_sha256)?;
    match confirm_pregame_keep_to_first_main_transition_v3(pending.plan, after) {
        Ok(confirmed) => {
            release_confirmed_pending_v3(&pending.input_receipt_sha256)?;
            Ok(confirmed)
        }
        Err(error) => {
            halt_gate_v3()?;
            Err(format!(
                "pregame Keep postcondition failed and the input gate is halted: {error}"
            ))
        }
    }
}

fn ratify_competitive_lifecycle_authorization_with_commitment_v1(
    correspondence: CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
    profile: &AdmittedMtgoCompetitiveNavigationProfileV1,
    visible_account_alias: String,
    event_kind: MtgoCompetitiveEventKindV1,
    expected_ratification_commitment_sha256: Option<&str>,
) -> Result<RatifiedMtgoCompetitiveLifecycleAuthorizationV1, String> {
    let candidate = review_competitive_lifecycle_ratification_candidate_from_correspondence_v1(
        &correspondence,
        profile,
        &visible_account_alias,
        event_kind,
    )?;
    let expected = expected_ratification_commitment_sha256
        .ok_or("the exact competitive lifecycle authorization is not ratified in this build")?;
    if candidate.ratification_commitment_sha256 != expected {
        return Err(
            "competitive lifecycle candidate differs from the production ratification root"
                .to_owned(),
        );
    }
    let scope = correspondence
        .checked_untrusted_scope_for_mode_v1(event_kind)
        .map_err(|error| format!("competitive lifecycle ratified scope: {error}"))?;
    Ok(RatifiedMtgoCompetitiveLifecycleAuthorizationV1 {
        _permission_correspondence: correspondence,
        scope,
        commitments: candidate,
    })
}

fn competitive_lifecycle_allowed_actions_commitment_v1() -> String {
    hash_parts_v2(
        COMPETITIVE_LIFECYCLE_ACTION_SET_DOMAIN_V1,
        &[
            competitive_lifecycle_action_tag_v1(MtgoCompetitiveLifecycleActionV1::AcceptPairing),
            competitive_lifecycle_action_tag_v1(MtgoCompetitiveLifecycleActionV1::SubmitSideboard),
            competitive_lifecycle_action_tag_v1(
                MtgoCompetitiveLifecycleActionV1::ContinueAfterMatch,
            ),
            competitive_lifecycle_action_tag_v1(MtgoCompetitiveLifecycleActionV1::ResumeMatch),
            competitive_lifecycle_action_tag_v1(
                MtgoCompetitiveLifecycleActionV1::CloseCompletedEvent,
            ),
            b"canonical_order_exactly_five_no_entry_actions",
        ],
    )
}

fn competitive_open_entry_review_scope_commitment_v1() -> String {
    hash_parts_v2(
        COMPETITIVE_OPEN_ENTRY_REVIEW_SCOPE_DOMAIN_V1,
        &[
            b"one_fresh_evaluated_selected_listing",
            b"one_left_click_on_enabled_open_entry_review_control",
            b"strictly_newer_exact_entry_review_visible_confirmation",
            b"no_event_entry_confirmation_no_spending_no_hidden_channels_no_second_input",
        ],
    )
}

fn competitive_sideboard_automation_scope_v1() -> String {
    hash_parts_v2(
        COMPETITIVE_SIDEBOARD_AUTOMATION_SCOPE_DOMAIN_V1,
        &[
            b"mtgo_visible_competitive_sideboard_v1",
            b"official_mtgo_drag_between_visible_zones_one_card_per_input",
            b"strictly_newer_exact_inventory_confirmation_after_each_drag",
            b"changed_sideboard_submit_only_after_exact_target_ready",
            b"no_double_click_no_keyboard_no_hidden_channels_no_event_entry_no_spending",
        ],
    )
}

fn is_non_entry_lifecycle_action_v1(action: MtgoCompetitiveLifecycleActionV1) -> bool {
    matches!(
        action,
        MtgoCompetitiveLifecycleActionV1::AcceptPairing
            | MtgoCompetitiveLifecycleActionV1::SubmitSideboard
            | MtgoCompetitiveLifecycleActionV1::ContinueAfterMatch
            | MtgoCompetitiveLifecycleActionV1::ResumeMatch
            | MtgoCompetitiveLifecycleActionV1::CloseCompletedEvent
    )
}

fn require_same_competitive_navigation_lineage_v1(
    current: &OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    next: &OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
) -> Result<(), String> {
    let current = current.commitments_v1();
    let next = next.commitments_v1();
    if current.source_frame.profile_commitment_sha256 != next.source_frame.profile_commitment_sha256
        || current.source_frame.profile_admission_commitment_sha256
            != next.source_frame.profile_admission_commitment_sha256
        || current.source_frame.approved_account_alias_sha256
            != next.source_frame.approved_account_alias_sha256
        || current.runtime_identity_commitment_sha256 != next.runtime_identity_commitment_sha256
        || current.event_kind != next.event_kind
        || next.frame_id == current.frame_id
        || next.frame_sequence <= current.frame_sequence
    {
        return Err(
            "competitive event navigation advance changed profile, account, runtime, mode, or frame order"
                .to_owned(),
        );
    }
    Ok(())
}

fn effective_current_deck_commitment_v1(
    runtime: &mut MtgoCompetitiveEventRuntimeCommitmentsV1,
    state: &MtgoCompetitivePlayerKnownDeckStateV1,
) -> Result<(), String> {
    runtime.player_known_current_deck_configuration_commitment_sha256 =
        state.current_commitment_v1()?;
    Ok(())
}

fn validate_player_known_deck_state_against_runtime_v1(
    runtime: &MtgoCompetitiveEventRuntimeCommitmentsV1,
    state: &MtgoCompetitivePlayerKnownDeckStateV1,
) -> Result<(), String> {
    if state.current_commitment_v1()?
        != runtime.player_known_current_deck_configuration_commitment_sha256
    {
        return Err(
            "competitive event runtime changed its player-known current deck state".to_owned(),
        );
    }
    Ok(())
}

fn advance_competitive_event_runtime_commitments_v1(
    prior: &MtgoCompetitiveEventRuntimeCommitmentsV1,
    current_frame: &OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    confirmed_action: Option<(MtgoCompetitiveLifecycleActionV1, &str)>,
    observed_advance: Option<(MtgoObservedCompetitiveLifecycleAdvanceV1, &str)>,
    monitor: Option<MtgoCompetitiveEventMonitorCommitmentsV1>,
) -> Result<MtgoCompetitiveEventRuntimeCommitmentsV1, String> {
    if confirmed_action.is_some() == observed_advance.is_some() {
        return Err(
            "competitive event runtime advance requires exactly one action or observation receipt"
                .to_owned(),
        );
    }
    let current = current_frame.commitments_v1();
    let lifecycle = current_frame.lifecycle_snapshot_v1();
    if current.event_kind != prior.event_kind
        || current.source_frame.profile_commitment_sha256
            != prior.navigation_profile_commitment_sha256
        || current.source_frame.profile_admission_commitment_sha256
            != prior.navigation_profile_admission_commitment_sha256
        || current.source_frame.approved_account_alias_sha256 != prior.approved_account_alias_sha256
        || (current.phase != MtgoCompetitiveLifecyclePhaseV1::EventBrowser
            && lifecycle.event_identity_sha256_v1()
                != Some(prior.bound_event_identity_sha256.as_str()))
        || validate_competitive_event_next_frame_order_v1(prior, current.frame_sequence).is_err()
    {
        return Err(
            "competitive event runtime next frame changed its exact event lineage".to_owned(),
        );
    }
    let mut next = prior.clone();
    next.current_phase = current.phase;
    next.current_lifecycle_snapshot_commitment_sha256 =
        current.lifecycle_snapshot_commitment_sha256;
    next.current_match_identity_sha256 = lifecycle.match_identity_sha256_v1().map(str::to_owned);
    next.current_game_number = lifecycle.game_number_v1();
    next.current_frame_id = current.frame_id;
    next.current_frame_sequence = current.frame_sequence;
    next.lifecycle_transition_count = prior
        .lifecycle_transition_count
        .checked_add(1)
        .ok_or("competitive event lifecycle transition count overflow")?;
    let (transition_receipt, transition_label): (&str, &[u8]) =
        if let Some((action, receipt)) = confirmed_action {
            next.confirmed_lifecycle_action_count = prior
                .confirmed_lifecycle_action_count
                .checked_add(1)
                .ok_or("competitive event lifecycle action count overflow")?;
            if action == MtgoCompetitiveLifecycleActionV1::CloseCompletedEvent {
                if !prior.terminal_event_record_confirmed
                    || current.phase != MtgoCompetitiveLifecyclePhaseV1::EventBrowser
                {
                    return Err(
                        "event closure requires the terminal record and visible Event Browser"
                            .to_owned(),
                    );
                }
                next.closed_to_event_browser = true;
            }
            (receipt, competitive_lifecycle_action_tag_v1(action))
        } else if let Some((observed, receipt)) = observed_advance {
            next.observed_lifecycle_advance_count = prior
                .observed_lifecycle_advance_count
                .checked_add(1)
                .ok_or("competitive event observed advance count overflow")?;
            (receipt, observed_lifecycle_advance_tag_v1(observed))
        } else {
            unreachable!()
        };
    if let Some(monitor) = monitor {
        next.event_monitor_chain_commitment_sha256 = Some(monitor.monitor_chain_commitment_sha256);
        next.event_monitor_observation_count = monitor.observation_count;
        next.terminal_event_record_confirmed = monitor.terminal;
    }
    next.runtime_commitment_sha256 = competitive_event_runtime_commitment_v1(
        COMPETITIVE_EVENT_RUNTIME_ADVANCE_DOMAIN_V1,
        Some(prior.runtime_commitment_sha256.as_str()),
        &next,
        &[transition_label, transition_receipt.as_bytes()].concat(),
    );
    Ok(next)
}

fn validate_event_monitor_against_runtime_v1(
    runtime: &OpaqueMtgoCompetitiveEventRuntimeV1,
    monitor: &MtgoCompetitiveEventMonitorCommitmentsV1,
) -> Result<(), String> {
    let commitments = &runtime.commitments;
    if monitor.navigation_profile_commitment_sha256
        != commitments.navigation_profile_commitment_sha256
        || monitor.navigation_profile_admission_commitment_sha256
            != commitments.navigation_profile_admission_commitment_sha256
        || monitor.approved_account_alias_sha256 != commitments.approved_account_alias_sha256
        || monitor.event_identity_sha256 != commitments.bound_event_identity_sha256
        || monitor.event_kind != commitments.event_kind
        || monitor.process_continuity_commitment_sha256
            != runtime
                .current_frame
                .process_continuity_commitment_sha256_v1()
        || monitor.last_frame_sequence < commitments.current_frame_sequence
    {
        return Err(
            "competitive event monitor differs from the exact runtime event, client incarnation, or is stale"
                .to_owned(),
        );
    }
    Ok(())
}

fn validate_competitive_event_next_frame_order_v1(
    runtime: &MtgoCompetitiveEventRuntimeCommitmentsV1,
    next_frame_sequence: u64,
) -> Result<(), String> {
    if next_frame_sequence <= runtime.current_frame_sequence
        || runtime
            .last_completed_pregame
            .as_ref()
            .is_some_and(|pregame| next_frame_sequence <= pregame.completion_frame_sequence)
        || runtime
            .last_returned_gameplay_frame_sequence
            .is_some_and(|gameplay| next_frame_sequence <= gameplay)
    {
        return Err(
            "competitive event next frame is not newer than its lifecycle, pregame, and returned-gameplay floors"
                .to_owned(),
        );
    }
    Ok(())
}

fn apply_event_monitor_to_runtime_commitments_v1(
    runtime: &mut MtgoCompetitiveEventRuntimeCommitmentsV1,
    monitor: &MtgoCompetitiveEventMonitorCommitmentsV1,
    label: &[u8],
) {
    let prior_runtime_commitment_sha256 = runtime.runtime_commitment_sha256.clone();
    runtime.event_monitor_chain_commitment_sha256 =
        Some(monitor.monitor_chain_commitment_sha256.clone());
    runtime.event_monitor_observation_count = monitor.observation_count;
    runtime.terminal_event_record_confirmed = monitor.terminal;
    runtime.runtime_commitment_sha256 = competitive_event_runtime_commitment_v1(
        COMPETITIVE_EVENT_RUNTIME_MONITOR_DOMAIN_V1,
        Some(prior_runtime_commitment_sha256.as_str()),
        runtime,
        &[label, monitor.monitor_chain_commitment_sha256.as_bytes()].concat(),
    );
}

#[derive(Clone, Copy)]
struct CompetitivePregameClassifiedViewV2<'a> {
    source_capture_commitment_sha256: &'a str,
    duel_perception_profile_commitment_sha256: &'a str,
    duel_perception_profile_admission_commitment_sha256: &'a str,
    classifier_runtime_commitment_sha256: &'a str,
    pregame_evaluation_commitment_sha256: &'a str,
    pregame_profile_admission_commitment_sha256: &'a str,
    pregame_classification_commitment_sha256: &'a str,
    visible_interaction_commitment_sha256: &'a str,
    process_continuity_commitment_sha256: &'a str,
    window_continuity_commitment_sha256: &'a str,
    stage: MtgoCompetitivePregameStageLabelV1,
    frame_id: u64,
    frame_sequence: u64,
    captured_at_unix_millis: u128,
}

impl<'a> CompetitivePregameClassifiedViewV2<'a> {
    fn from_commitments_v2(
        value: &'a MtgoClassifiedCompetitivePregameFrameCommitmentsV1,
        process_continuity_commitment_sha256: &'a str,
        window_continuity_commitment_sha256: &'a str,
    ) -> Self {
        Self {
            source_capture_commitment_sha256: &value
                .source_frame
                .source_capture
                .capture_commitment_sha256,
            duel_perception_profile_commitment_sha256: &value
                .source_frame
                .perception_profile_commitment_sha256,
            duel_perception_profile_admission_commitment_sha256: &value
                .source_frame
                .perception_profile_admission_commitment_sha256,
            classifier_runtime_commitment_sha256: &value
                .classifier_runtime_identity_commitment_sha256,
            pregame_evaluation_commitment_sha256: &value.pregame_evaluation_commitment_sha256,
            pregame_profile_admission_commitment_sha256: &value
                .pregame_profile_admission_commitment_sha256,
            pregame_classification_commitment_sha256: &value.classification_commitment_sha256,
            visible_interaction_commitment_sha256: &value.visible_interaction_commitment_sha256,
            process_continuity_commitment_sha256,
            window_continuity_commitment_sha256,
            stage: value.stage,
            frame_id: value.frame_id,
            frame_sequence: value.frame_sequence,
            captured_at_unix_millis: value.captured_at_unix_millis,
        }
    }
}

fn competitive_pregame_observation_from_classified_view_v2(
    runtime: &MtgoCompetitiveEventRuntimeCommitmentsV1,
    source: CompetitivePregameClassifiedViewV2<'_>,
) -> Result<MtgoCompetitivePregameObservationCommitmentsV1, String> {
    if runtime.current_phase != MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
        || runtime.closed_to_event_browser
        || runtime.terminal_event_record_confirmed
    {
        return Err(
            "competitive pregame classification requires an open match-in-progress event runtime"
                .to_owned(),
        );
    }
    let (match_identity_sha256, game_number) =
        require_competitive_event_runtime_match_and_game_v1(runtime)?;
    let stage = match source.stage {
        MtgoCompetitivePregameStageLabelV1::MulliganChoice {
            prospective_keep_size,
        } => MtgoCompetitivePregameStageV1::MulliganChoice {
            prospective_keep_size,
        },
        MtgoCompetitivePregameStageLabelV1::LondonBottoming {
            required_bottom_count,
            selected_bottom_count,
        } => MtgoCompetitivePregameStageV1::LondonBottoming {
            required_bottom_count,
            selected_bottom_count,
        },
        MtgoCompetitivePregameStageLabelV1::GameplayReady => {
            MtgoCompetitivePregameStageV1::GameplayReady
        }
    };
    let mut observation = MtgoCompetitivePregameObservationCommitmentsV1 {
        observation_commitment_sha256: String::new(),
        source_capture_commitment_sha256: source.source_capture_commitment_sha256.to_owned(),
        duel_perception_profile_commitment_sha256: source
            .duel_perception_profile_commitment_sha256
            .to_owned(),
        duel_perception_profile_admission_commitment_sha256: source
            .duel_perception_profile_admission_commitment_sha256
            .to_owned(),
        classifier_runtime_commitment_sha256: source
            .classifier_runtime_commitment_sha256
            .to_owned(),
        pregame_evaluation_commitment_sha256: source
            .pregame_evaluation_commitment_sha256
            .to_owned(),
        pregame_profile_admission_commitment_sha256: source
            .pregame_profile_admission_commitment_sha256
            .to_owned(),
        pregame_classification_commitment_sha256: source
            .pregame_classification_commitment_sha256
            .to_owned(),
        visible_interaction_commitment_sha256: source
            .visible_interaction_commitment_sha256
            .to_owned(),
        process_continuity_commitment_sha256: source
            .process_continuity_commitment_sha256
            .to_owned(),
        window_continuity_commitment_sha256: source.window_continuity_commitment_sha256.to_owned(),
        approved_account_alias_sha256: runtime.approved_account_alias_sha256.clone(),
        entry_authorization_sha256: runtime.entry_authorization_sha256.clone(),
        event_identity_sha256: runtime.bound_event_identity_sha256.clone(),
        match_identity_sha256,
        event_kind: runtime.event_kind,
        game_number,
        stage,
        frame_id: source.frame_id,
        frame_sequence: source.frame_sequence,
        captured_at_unix_millis: source.captured_at_unix_millis,
    };
    observation.observation_commitment_sha256 =
        competitive_pregame_observation_commitment_v1(&observation)?;
    validate_competitive_pregame_observation_v1(&observation)?;
    Ok(observation)
}

fn competitive_pregame_observation_commitment_v1(
    value: &MtgoCompetitivePregameObservationCommitmentsV1,
) -> Result<String, String> {
    let stage_json = serde_json::to_vec(&value.stage)
        .map_err(|error| format!("serialize competitive pregame stage: {error}"))?;
    Ok(hash_parts_v2(
        COMPETITIVE_PREGAME_OBSERVATION_DOMAIN_V1,
        &[
            value.source_capture_commitment_sha256.as_bytes(),
            value.duel_perception_profile_commitment_sha256.as_bytes(),
            value
                .duel_perception_profile_admission_commitment_sha256
                .as_bytes(),
            value.classifier_runtime_commitment_sha256.as_bytes(),
            value.pregame_evaluation_commitment_sha256.as_bytes(),
            value.pregame_profile_admission_commitment_sha256.as_bytes(),
            value.pregame_classification_commitment_sha256.as_bytes(),
            value.visible_interaction_commitment_sha256.as_bytes(),
            value.process_continuity_commitment_sha256.as_bytes(),
            value.window_continuity_commitment_sha256.as_bytes(),
            value.approved_account_alias_sha256.as_bytes(),
            value.entry_authorization_sha256.as_bytes(),
            value.event_identity_sha256.as_bytes(),
            value.match_identity_sha256.as_bytes(),
            competitive_event_kind_tag_v1(value.event_kind),
            &[value.game_number],
            &stage_json,
            value.frame_id.to_be_bytes().as_slice(),
            value.frame_sequence.to_be_bytes().as_slice(),
            value.captured_at_unix_millis.to_be_bytes().as_slice(),
            b"evaluated_visible_pregame_no_hidden_channels_no_input_no_spending",
        ],
    ))
}

fn validate_competitive_pregame_observation_v1(
    value: &MtgoCompetitivePregameObservationCommitmentsV1,
) -> Result<(), String> {
    for digest in [
        value.observation_commitment_sha256.as_str(),
        value.source_capture_commitment_sha256.as_str(),
        value.duel_perception_profile_commitment_sha256.as_str(),
        value
            .duel_perception_profile_admission_commitment_sha256
            .as_str(),
        value.classifier_runtime_commitment_sha256.as_str(),
        value.pregame_evaluation_commitment_sha256.as_str(),
        value.pregame_profile_admission_commitment_sha256.as_str(),
        value.pregame_classification_commitment_sha256.as_str(),
        value.visible_interaction_commitment_sha256.as_str(),
        value.process_continuity_commitment_sha256.as_str(),
        value.window_continuity_commitment_sha256.as_str(),
        value.approved_account_alias_sha256.as_str(),
        value.entry_authorization_sha256.as_str(),
        value.event_identity_sha256.as_str(),
        value.match_identity_sha256.as_str(),
    ] {
        if !is_sha256_v2(digest) {
            return Err(
                "competitive pregame observation contains an invalid commitment".to_owned(),
            );
        }
    }
    if value.game_number == 0
        || value.game_number > 3
        || value.frame_id == 0
        || value.frame_sequence == 0
        || value.captured_at_unix_millis == 0
    {
        return Err(
            "competitive pregame observation has an invalid game or frame identity".to_owned(),
        );
    }
    match value.stage {
        MtgoCompetitivePregameStageV1::MulliganChoice {
            prospective_keep_size,
        } if prospective_keep_size <= 7 => {}
        MtgoCompetitivePregameStageV1::LondonBottoming {
            required_bottom_count,
            selected_bottom_count,
        } if (1..=7).contains(&required_bottom_count)
            && selected_bottom_count <= required_bottom_count => {}
        MtgoCompetitivePregameStageV1::GameplayReady => {}
        _ => {
            return Err("competitive pregame observation has an impossible stage".to_owned());
        }
    }
    let expected = competitive_pregame_observation_commitment_v1(value)?;
    if expected != value.observation_commitment_sha256 {
        return Err("competitive pregame observation commitment changed".to_owned());
    }
    Ok(())
}

fn validate_competitive_pregame_stage_transition_v1(
    current: MtgoCompetitivePregameStageV1,
    next: MtgoCompetitivePregameStageV1,
) -> Result<(), String> {
    let allowed = match (current, next) {
        (
            MtgoCompetitivePregameStageV1::MulliganChoice {
                prospective_keep_size: current,
            },
            MtgoCompetitivePregameStageV1::MulliganChoice {
                prospective_keep_size: next,
            },
        ) => current > 0 && next.checked_add(1) == Some(current),
        (
            MtgoCompetitivePregameStageV1::MulliganChoice {
                prospective_keep_size: 7,
            },
            MtgoCompetitivePregameStageV1::GameplayReady,
        ) => true,
        (
            MtgoCompetitivePregameStageV1::MulliganChoice {
                prospective_keep_size,
            },
            MtgoCompetitivePregameStageV1::LondonBottoming {
                required_bottom_count,
                selected_bottom_count: 0,
            },
        ) => prospective_keep_size < 7 && required_bottom_count == 7 - prospective_keep_size,
        (
            MtgoCompetitivePregameStageV1::LondonBottoming {
                required_bottom_count: current_required,
                selected_bottom_count: current_selected,
            },
            MtgoCompetitivePregameStageV1::LondonBottoming {
                required_bottom_count: next_required,
                selected_bottom_count: next_selected,
            },
        ) => {
            current_required == next_required
                && current_selected < current_required
                && next_selected == current_selected + 1
        }
        (
            MtgoCompetitivePregameStageV1::LondonBottoming {
                required_bottom_count,
                selected_bottom_count,
            },
            MtgoCompetitivePregameStageV1::GameplayReady,
        ) => selected_bottom_count == required_bottom_count,
        _ => false,
    };
    if !allowed {
        return Err(
            "competitive pregame observation skipped or reversed a visible stage".to_owned(),
        );
    }
    Ok(())
}

fn next_competitive_pregame_bottom_history_v1(
    current: MtgoCompetitivePregameStageV1,
    next: MtgoCompetitivePregameStageV1,
    prior: &[u8],
    confirmed_bottom_slot: Option<u8>,
) -> Result<Vec<u8>, String> {
    let current_selected = match current {
        MtgoCompetitivePregameStageV1::LondonBottoming {
            selected_bottom_count,
            ..
        } => selected_bottom_count,
        _ => 0,
    };
    let next_selected = match next {
        MtgoCompetitivePregameStageV1::LondonBottoming {
            selected_bottom_count,
            ..
        } => selected_bottom_count,
        _ => current_selected,
    };
    if prior.len() != usize::from(current_selected) {
        return Err("competitive pregame prior bottom history is incomplete".to_owned());
    }
    if next_selected == current_selected {
        if confirmed_bottom_slot.is_some() {
            return Err(
                "competitive pregame confirmed bottom slot lacks a visible count increase"
                    .to_owned(),
            );
        }
        return Ok(prior.to_vec());
    }
    let card_slot = confirmed_bottom_slot
        .ok_or("competitive pregame observed bottom selection lacks confirmed action history")?;
    if next_selected != current_selected.saturating_add(1)
        || card_slot >= 7
        || prior.contains(&card_slot)
    {
        return Err(
            "competitive pregame confirmed bottom slot differs from the visible transition"
                .to_owned(),
        );
    }
    let mut next_history = prior.to_vec();
    next_history.push(card_slot);
    Ok(next_history)
}

fn competitive_event_pregame_session_commitments_from_parts_v1(
    runtime: &MtgoCompetitiveEventRuntimeCommitmentsV1,
    runtime_process_continuity_commitment_sha256: &str,
    runtime_window_continuity_commitment_sha256: &str,
    runtime_captured_at_unix_millis: u128,
    match_launch: &RatifiedMtgoCompetitiveMatchLaunchV1,
    source: &MtgoCompetitivePregameObservationCommitmentsV1,
) -> Result<MtgoCompetitiveEventPregameSessionCommitmentsV1, String> {
    validate_competitive_pregame_observation_v1(source)?;
    let authorization = &match_launch.authorization;
    let (match_identity_sha256, game_number) =
        require_competitive_event_runtime_match_and_game_v1(runtime)?;
    if runtime.closed_to_event_browser
        || runtime.terminal_event_record_confirmed
        || runtime.current_phase != MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
        || runtime.pregame_session_count == 0 && runtime.last_completed_pregame.is_some()
        || runtime.pregame_session_count > 0 && runtime.last_completed_pregame.is_none()
        || runtime
            .last_completed_pregame
            .as_ref()
            .is_some_and(|completed| {
                completed.match_identity_sha256 == match_identity_sha256
                    && completed.game_number == game_number
            })
        || runtime
            .last_returned_gameplay_frame_sequence
            .is_some_and(|returned| returned >= runtime.current_frame_sequence)
        || authorization.event_kind != runtime.event_kind
        || authorization.account_alias_sha256 != runtime.approved_account_alias_sha256
        || authorization.written_permission_sha256 != runtime.correspondence_sha256
        || authorization.event_identity_sha256 != runtime.bound_event_identity_sha256
        || authorization.match_identity_sha256 != match_identity_sha256
        || authorization.game_number != game_number
        || authorization.entry_authorization_sha256 != runtime.entry_authorization_sha256
        || !authorization.exact_match_gameplay_authorized
        || match_launch.mode_authorization_commitment_sha256
            != runtime.mode_authorization_commitment_sha256
        || source.event_kind != runtime.event_kind
        || source.game_number != game_number
        || source.approved_account_alias_sha256 != runtime.approved_account_alias_sha256
        || source.entry_authorization_sha256 != runtime.entry_authorization_sha256
        || source.event_identity_sha256 != runtime.bound_event_identity_sha256
        || source.match_identity_sha256 != match_identity_sha256
        || source.process_continuity_commitment_sha256
            != runtime_process_continuity_commitment_sha256
        || source.window_continuity_commitment_sha256 != runtime_window_continuity_commitment_sha256
        || source.frame_sequence <= runtime.current_frame_sequence
        || source.frame_sequence < match_launch.valid_from_frame_sequence
        || source.frame_sequence > authorization.valid_through_frame_sequence
        || source.captured_at_unix_millis < runtime_captured_at_unix_millis
    {
        return Err(
            "competitive pregame checkout differs from the exact paid event, account, match, game, deck launch, process, or frame lifetime"
                .to_owned(),
        );
    }
    let mut commitments = MtgoCompetitiveEventPregameSessionCommitmentsV1 {
        session_commitment_sha256: String::new(),
        prior_session_commitment_sha256: None,
        event_runtime_commitment_sha256: runtime.runtime_commitment_sha256.clone(),
        match_launch_authorization_commitment_sha256: match_launch
            .launch_authorization_commitment_sha256
            .clone(),
        match_gameplay_authorization_commitment_sha256: match_launch
            .gameplay_authorization_commitment_sha256
            .clone(),
        mode_authorization_commitment_sha256: match_launch
            .mode_authorization_commitment_sha256
            .clone(),
        entry_ratification_commitment_sha256: runtime.entry_ratification_commitment_sha256.clone(),
        entry_authorization_sha256: runtime.entry_authorization_sha256.clone(),
        deck_manifest_sha256: runtime.deck_manifest_sha256.clone(),
        deck_format_sha256: runtime.deck_format_sha256.clone(),
        policy_deployment_commitment_sha256: runtime.policy_deployment_commitment_sha256.clone(),
        approved_account_alias_sha256: runtime.approved_account_alias_sha256.clone(),
        event_identity_sha256: runtime.bound_event_identity_sha256.clone(),
        match_identity_sha256,
        process_continuity_commitment_sha256: source.process_continuity_commitment_sha256.clone(),
        window_continuity_commitment_sha256: source.window_continuity_commitment_sha256.clone(),
        duel_perception_profile_commitment_sha256: source
            .duel_perception_profile_commitment_sha256
            .clone(),
        duel_perception_profile_admission_commitment_sha256: source
            .duel_perception_profile_admission_commitment_sha256
            .clone(),
        pregame_evaluation_commitment_sha256: source.pregame_evaluation_commitment_sha256.clone(),
        pregame_profile_admission_commitment_sha256: source
            .pregame_profile_admission_commitment_sha256
            .clone(),
        initial_observation_commitment_sha256: source.observation_commitment_sha256.clone(),
        current_observation_commitment_sha256: source.observation_commitment_sha256.clone(),
        confirmed_bottom_history_commitment_sha256:
            competitive_pregame_confirmed_bottom_history_commitment_v1(&[]),
        current_model_context_binding_commitment_sha256: None,
        player_visible_public_context_commitment_sha256: None,
        event_kind: runtime.event_kind,
        game_number,
        current_stage: source.stage,
        checkout_event_frame_sequence: runtime.current_frame_sequence,
        initial_frame_sequence: source.frame_sequence,
        current_frame_sequence: source.frame_sequence,
        visible_transition_count: 0,
    };
    commitments.session_commitment_sha256 = competitive_event_pregame_session_commitment_v1(
        COMPETITIVE_EVENT_PREGAME_SESSION_DOMAIN_V1,
        None,
        &commitments,
    )?;
    Ok(commitments)
}

fn competitive_event_pregame_session_commitment_v1(
    domain: &[u8],
    prior_session_commitment_sha256: Option<&str>,
    value: &MtgoCompetitiveEventPregameSessionCommitmentsV1,
) -> Result<String, String> {
    let stage_json = serde_json::to_vec(&value.current_stage)
        .map_err(|error| format!("serialize current competitive pregame stage: {error}"))?;
    Ok(hash_parts_v2(
        domain,
        &[
            prior_session_commitment_sha256.unwrap_or("").as_bytes(),
            value
                .prior_session_commitment_sha256
                .as_deref()
                .unwrap_or("")
                .as_bytes(),
            value.event_runtime_commitment_sha256.as_bytes(),
            value
                .match_launch_authorization_commitment_sha256
                .as_bytes(),
            value
                .match_gameplay_authorization_commitment_sha256
                .as_bytes(),
            value.mode_authorization_commitment_sha256.as_bytes(),
            value.entry_ratification_commitment_sha256.as_bytes(),
            value.entry_authorization_sha256.as_bytes(),
            value.deck_manifest_sha256.as_bytes(),
            value.deck_format_sha256.as_bytes(),
            value.policy_deployment_commitment_sha256.as_bytes(),
            value.approved_account_alias_sha256.as_bytes(),
            value.event_identity_sha256.as_bytes(),
            value.match_identity_sha256.as_bytes(),
            value.process_continuity_commitment_sha256.as_bytes(),
            value.window_continuity_commitment_sha256.as_bytes(),
            value.duel_perception_profile_commitment_sha256.as_bytes(),
            value
                .duel_perception_profile_admission_commitment_sha256
                .as_bytes(),
            value.pregame_evaluation_commitment_sha256.as_bytes(),
            value.pregame_profile_admission_commitment_sha256.as_bytes(),
            value.initial_observation_commitment_sha256.as_bytes(),
            value.current_observation_commitment_sha256.as_bytes(),
            value.confirmed_bottom_history_commitment_sha256.as_bytes(),
            value
                .current_model_context_binding_commitment_sha256
                .as_deref()
                .unwrap_or("")
                .as_bytes(),
            value
                .player_visible_public_context_commitment_sha256
                .as_deref()
                .unwrap_or("")
                .as_bytes(),
            competitive_event_kind_tag_v1(value.event_kind),
            &[value.game_number],
            &stage_json,
            value.checkout_event_frame_sequence.to_be_bytes().as_slice(),
            value.initial_frame_sequence.to_be_bytes().as_slice(),
            value.current_frame_sequence.to_be_bytes().as_slice(),
            value.visible_transition_count.to_be_bytes().as_slice(),
            b"move_only_exact_event_pregame_no_input_no_event_entry_no_spending",
        ],
    ))
}

fn competitive_pregame_confirmed_bottom_history_commitment_v1(
    ordered_confirmed_bottom_slots: &[u8],
) -> String {
    hash_parts_v2(
        COMPETITIVE_EVENT_PREGAME_BOTTOM_HISTORY_DOMAIN_V1,
        &[
            ordered_confirmed_bottom_slots,
            b"ordered_player_visible_confirmed_london_bottom_slots",
        ],
    )
}

fn competitive_pregame_player_visible_public_context_state_commitment_v1(
    value: MtgoCompetitivePregamePlayerVisiblePublicContextStateV1,
) -> String {
    let play_draw = match value.play_draw {
        mtgo_blackbox_v1::MtgoCompetitivePregamePlayDrawV1::OnPlay => b"on_play".as_slice(),
        mtgo_blackbox_v1::MtgoCompetitivePregamePlayDrawV1::OnDraw => b"on_draw".as_slice(),
    };
    hash_parts_v2(
        COMPETITIVE_EVENT_PREGAME_PUBLIC_CONTEXT_STATE_DOMAIN_V1,
        &[
            &[value.game_number],
            play_draw,
            &[value.acting_player_games_won],
            &[value.opponent_games_won],
            b"player_visible_public_pregame_context_only",
        ],
    )
}

fn validate_competitive_pregame_confirmed_bottom_history_v1(
    stage: MtgoCompetitivePregameStageV1,
    ordered_confirmed_bottom_slots: &[u8],
) -> Result<(), String> {
    let mut unique = std::collections::HashSet::new();
    if ordered_confirmed_bottom_slots
        .iter()
        .any(|slot| *slot >= 7 || !unique.insert(*slot))
    {
        return Err("competitive pregame confirmed bottom history is invalid".to_owned());
    }
    match stage {
        MtgoCompetitivePregameStageV1::MulliganChoice { .. }
            if ordered_confirmed_bottom_slots.is_empty() => {}
        MtgoCompetitivePregameStageV1::LondonBottoming {
            selected_bottom_count,
            ..
        } if ordered_confirmed_bottom_slots.len() == usize::from(selected_bottom_count) => {}
        MtgoCompetitivePregameStageV1::GameplayReady => {}
        _ => {
            return Err(
                "competitive pregame confirmed bottom history differs from the visible stage"
                    .to_owned(),
            )
        }
    }
    Ok(())
}

fn validate_competitive_event_pregame_session_integrity_v1(
    session: &OpaqueMtgoCompetitiveEventPregameSessionV1,
) -> Result<(), String> {
    validate_competitive_pregame_observation_v1(&session.current_observation.commitments)?;
    let source = &session.current_observation.commitments;
    let value = &session.commitments;
    validate_competitive_pregame_confirmed_bottom_history_v1(
        value.current_stage,
        &session.ordered_confirmed_bottom_slots,
    )?;
    let retained_model_context_binding = session.current_model_context.as_ref().map(|context| {
        context
            .commitments_v1()
            .model_context_binding_commitment_sha256
            .as_str()
    });
    let retained_model_context_matches = session
        .current_model_context
        .as_ref()
        .map(OpaqueMtgoCompetitivePregamePublicContextWitnessV1::commitments_v1)
        .is_none_or(|context| {
            context.source.classification_commitment_sha256
                == source.pregame_classification_commitment_sha256
                && context.source.visible_interaction_commitment_sha256
                    == source.visible_interaction_commitment_sha256
                && context
                    .source
                    .source_frame
                    .source_capture
                    .capture_commitment_sha256
                    == source.source_capture_commitment_sha256
                && context.source.frame_id == source.frame_id
                && context.source.frame_sequence == source.frame_sequence
                && context.source.captured_at_unix_millis == source.captured_at_unix_millis
                && context.source.stage
                    == match source.stage {
                        MtgoCompetitivePregameStageV1::MulliganChoice {
                            prospective_keep_size,
                        } => MtgoCompetitivePregameStageLabelV1::MulliganChoice {
                            prospective_keep_size,
                        },
                        MtgoCompetitivePregameStageV1::LondonBottoming {
                            required_bottom_count,
                            selected_bottom_count,
                        } => MtgoCompetitivePregameStageLabelV1::LondonBottoming {
                            required_bottom_count,
                            selected_bottom_count,
                        },
                        MtgoCompetitivePregameStageV1::GameplayReady => {
                            MtgoCompetitivePregameStageLabelV1::GameplayReady
                        }
                    }
                && context.game_number == value.game_number
                && [
                    context.public_context_evaluation_commitment_sha256.as_str(),
                    context
                        .public_context_profile_admission_commitment_sha256
                        .as_str(),
                    context.public_context_request_commitment_sha256.as_str(),
                    context.public_context_result_commitment_sha256.as_str(),
                    context.public_context_commitment_sha256.as_str(),
                    context.model_context_binding_commitment_sha256.as_str(),
                ]
                .into_iter()
                .all(is_sha256_v2)
        });
    let retained_player_visible_public_context_commitment = session
        .player_visible_public_context
        .map(competitive_pregame_player_visible_public_context_state_commitment_v1);
    let retained_public_context_pair_valid = match (
        session.current_model_context.as_ref(),
        session.player_visible_public_context,
    ) {
        (Some(context), Some(visible)) => {
            visible
                == MtgoCompetitivePregamePlayerVisiblePublicContextStateV1::from(
                    context.commitments_v1(),
                )
        }
        (None, None) => true,
        _ => false,
    };
    if value.current_observation_commitment_sha256 != source.observation_commitment_sha256
        || value.current_stage != source.stage
        || value.current_frame_sequence != source.frame_sequence
        || value.process_continuity_commitment_sha256 != source.process_continuity_commitment_sha256
        || value.window_continuity_commitment_sha256 != source.window_continuity_commitment_sha256
        || value.duel_perception_profile_commitment_sha256
            != source.duel_perception_profile_commitment_sha256
        || value.duel_perception_profile_admission_commitment_sha256
            != source.duel_perception_profile_admission_commitment_sha256
        || value.pregame_evaluation_commitment_sha256 != source.pregame_evaluation_commitment_sha256
        || value.pregame_profile_admission_commitment_sha256
            != source.pregame_profile_admission_commitment_sha256
        || value.approved_account_alias_sha256 != source.approved_account_alias_sha256
        || value.entry_authorization_sha256 != source.entry_authorization_sha256
        || value.event_identity_sha256 != source.event_identity_sha256
        || value.match_identity_sha256 != source.match_identity_sha256
        || value.event_kind != source.event_kind
        || value.game_number != source.game_number
        || value.event_runtime_commitment_sha256
            != session.runtime.commitments.runtime_commitment_sha256
        || value.entry_ratification_commitment_sha256
            != session
                .runtime
                .commitments
                .entry_ratification_commitment_sha256
        || value.entry_authorization_sha256
            != session.runtime.commitments.entry_authorization_sha256
        || value.deck_manifest_sha256 != session.runtime.commitments.deck_manifest_sha256
        || value.deck_format_sha256 != session.runtime.commitments.deck_format_sha256
        || value.policy_deployment_commitment_sha256
            != session
                .runtime
                .commitments
                .policy_deployment_commitment_sha256
        || value.approved_account_alias_sha256
            != session.runtime.commitments.approved_account_alias_sha256
        || value.event_identity_sha256 != session.runtime.commitments.bound_event_identity_sha256
        || session
            .runtime
            .commitments
            .current_match_identity_sha256
            .as_deref()
            != Some(value.match_identity_sha256.as_str())
        || session.runtime.commitments.current_game_number != Some(value.game_number)
        || session.runtime.commitments.event_kind != value.event_kind
        || value.match_launch_authorization_commitment_sha256
            != session.match_launch.launch_authorization_commitment_sha256
        || value.match_gameplay_authorization_commitment_sha256
            != session
                .match_launch
                .gameplay_authorization_commitment_sha256
        || value.mode_authorization_commitment_sha256
            != session.match_launch.mode_authorization_commitment_sha256
        || value.confirmed_bottom_history_commitment_sha256
            != competitive_pregame_confirmed_bottom_history_commitment_v1(
                &session.ordered_confirmed_bottom_slots,
            )
        || value
            .current_model_context_binding_commitment_sha256
            .as_deref()
            != retained_model_context_binding
        || !retained_model_context_matches
        || value.player_visible_public_context_commitment_sha256
            != retained_player_visible_public_context_commitment
        || !retained_public_context_pair_valid
    {
        return Err("competitive pregame session lineage changed".to_owned());
    }
    let (domain, prior) = if value.visible_transition_count == 0 {
        if value.prior_session_commitment_sha256.is_some() {
            return Err("initial competitive pregame session has a prior chain".to_owned());
        }
        (COMPETITIVE_EVENT_PREGAME_SESSION_DOMAIN_V1, None)
    } else {
        let prior = value
            .prior_session_commitment_sha256
            .as_deref()
            .ok_or("advanced competitive pregame session is missing its prior chain")?;
        if !is_sha256_v2(prior) {
            return Err("competitive pregame prior session commitment is invalid".to_owned());
        }
        (COMPETITIVE_EVENT_PREGAME_ADVANCE_DOMAIN_V1, Some(prior))
    };
    let expected = competitive_event_pregame_session_commitment_v1(domain, prior, value)?;
    if expected != value.session_commitment_sha256 {
        return Err("competitive pregame session commitment changed".to_owned());
    }
    Ok(())
}

fn competitive_gesture_game_session_event_deck_binding_commitment_v1(
    session: &MtgoCompetitiveGestureGameSessionCommitmentsV1,
    runtime: &MtgoCompetitiveEventRuntimeCommitmentsV1,
) -> Result<String, String> {
    if session.entry_ratification_commitment_sha256.is_some()
        || session.selected_deck_label_sha256.is_some()
        || session.selected_deck_region_sha256.is_some()
        || session.deck_manifest_sha256.is_some()
        || session.deck_format_sha256.is_some()
        || session.policy_deployment_commitment_sha256.is_some()
    {
        return Err(
            "competitive gameplay checkout requires an unbound exact-game session".to_owned(),
        );
    }
    for digest in [
        session.session_commitment_sha256.as_str(),
        runtime.runtime_commitment_sha256.as_str(),
        runtime.entry_ratification_commitment_sha256.as_str(),
        runtime.selected_deck_label_sha256.as_str(),
        runtime.selected_deck_region_sha256.as_str(),
        runtime.deck_manifest_sha256.as_str(),
        runtime.deck_format_sha256.as_str(),
        runtime.policy_deployment_commitment_sha256.as_str(),
    ] {
        if !is_sha256_v2(digest) {
            return Err(
                "competitive gameplay deck binding contains an invalid commitment".to_owned(),
            );
        }
    }
    if runtime.deck_manifest_sha256 == runtime.deck_format_sha256
        || runtime.deck_manifest_sha256 == runtime.policy_deployment_commitment_sha256
        || runtime.deck_format_sha256 == runtime.policy_deployment_commitment_sha256
    {
        return Err(
            "competitive gameplay deck, format, and policy commitments must remain distinct"
                .to_owned(),
        );
    }
    Ok(hash_parts_v2(
        COMPETITIVE_GESTURE_GAME_SESSION_EVENT_DECK_BIND_DOMAIN_V1,
        &[
            session.session_commitment_sha256.as_bytes(),
            runtime.runtime_commitment_sha256.as_bytes(),
            runtime.entry_ratification_commitment_sha256.as_bytes(),
            runtime.selected_deck_label_sha256.as_bytes(),
            runtime.selected_deck_region_sha256.as_bytes(),
            runtime.deck_manifest_sha256.as_bytes(),
            runtime.deck_format_sha256.as_bytes(),
            runtime.policy_deployment_commitment_sha256.as_bytes(),
            b"exact_event_entry_selected_deck_bound_to_all_family_game_session",
        ],
    ))
}

fn competitive_event_match_launch_binding_commitments_v1(
    runtime: &MtgoCompetitiveEventRuntimeCommitmentsV1,
    current_process_continuity_commitment_sha256: &str,
    current_captured_at_unix_millis: u128,
    source: &MtgoOpaqueCompetitiveLaunchIdentityCommitmentsV1,
    event_identity_sha256: &str,
    match_identity_sha256: &str,
    entry_authorization_sha256: &str,
) -> Result<MtgoCompetitiveEventMatchLaunchBindingCommitmentsV1, String> {
    if runtime.closed_to_event_browser
        || runtime.terminal_event_record_confirmed
        || runtime.current_phase != MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
        || source.event_kind != runtime.event_kind
        || runtime.current_game_number != Some(source.game_number)
        || source.frame_id == 0
        || source.frame_sequence <= runtime.current_frame_sequence
        || current_captured_at_unix_millis == 0
        || source.captured_at_unix_millis < current_captured_at_unix_millis
        || source.process_continuity_commitment_sha256
            != current_process_continuity_commitment_sha256
        || event_identity_sha256 != runtime.bound_event_identity_sha256
        || runtime.current_match_identity_sha256.as_deref() != Some(match_identity_sha256)
        || entry_authorization_sha256 != runtime.entry_authorization_sha256
    {
        return Err(
            "competitive duel launch differs from the current exact paid event runtime or is not strictly newer"
                .to_owned(),
        );
    }
    for value in [
        runtime.runtime_commitment_sha256.as_str(),
        runtime.entry_ratification_commitment_sha256.as_str(),
        runtime.entry_authorization_sha256.as_str(),
        runtime.selected_deck_label_sha256.as_str(),
        runtime.selected_deck_region_sha256.as_str(),
        runtime.deck_manifest_sha256.as_str(),
        runtime.deck_format_sha256.as_str(),
        runtime.policy_deployment_commitment_sha256.as_str(),
        runtime.bound_event_identity_sha256.as_str(),
        runtime
            .current_lifecycle_snapshot_commitment_sha256
            .as_str(),
        current_process_continuity_commitment_sha256,
        source.source_capture_commitment_sha256.as_str(),
        source.perception_result_commitment_sha256.as_str(),
        source.lifecycle_snapshot_commitment_sha256.as_str(),
        source.lifecycle_evaluation_commitment_sha256.as_str(),
        source
            .lifecycle_profile_admission_commitment_sha256
            .as_str(),
        source.process_continuity_commitment_sha256.as_str(),
        source.window_continuity_commitment_sha256.as_str(),
        source.window_title_sha256.as_str(),
        source.event_label_region_sha256.as_str(),
        source.launch_identity_commitment_sha256.as_str(),
        event_identity_sha256,
        match_identity_sha256,
        entry_authorization_sha256,
    ] {
        if !is_sha256_v2(value) {
            return Err(
                "competitive event match launch binding contains an invalid commitment".to_owned(),
            );
        }
    }
    let event_kind: &[u8] = match runtime.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let match_launch_binding_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_EVENT_MATCH_LAUNCH_BINDING_DOMAIN_V1,
        &[
            runtime.runtime_commitment_sha256.as_bytes(),
            runtime.entry_ratification_commitment_sha256.as_bytes(),
            runtime.entry_authorization_sha256.as_bytes(),
            runtime.selected_deck_label_sha256.as_bytes(),
            runtime.selected_deck_region_sha256.as_bytes(),
            runtime.deck_manifest_sha256.as_bytes(),
            runtime.deck_format_sha256.as_bytes(),
            runtime.policy_deployment_commitment_sha256.as_bytes(),
            runtime
                .current_lifecycle_snapshot_commitment_sha256
                .as_bytes(),
            runtime.current_frame_id.to_be_bytes().as_slice(),
            runtime.current_frame_sequence.to_be_bytes().as_slice(),
            current_captured_at_unix_millis.to_be_bytes().as_slice(),
            event_identity_sha256.as_bytes(),
            match_identity_sha256.as_bytes(),
            event_kind,
            &[source.game_number],
            source.frame_id.to_be_bytes().as_slice(),
            source.frame_sequence.to_be_bytes().as_slice(),
            source.captured_at_unix_millis.to_be_bytes().as_slice(),
            source.source_capture_commitment_sha256.as_bytes(),
            source.perception_result_commitment_sha256.as_bytes(),
            source.lifecycle_snapshot_commitment_sha256.as_bytes(),
            source.lifecycle_evaluation_commitment_sha256.as_bytes(),
            source
                .lifecycle_profile_admission_commitment_sha256
                .as_bytes(),
            source.process_continuity_commitment_sha256.as_bytes(),
            source.window_continuity_commitment_sha256.as_bytes(),
            source.launch_identity_commitment_sha256.as_bytes(),
            b"paid_main_client_event_runtime_bound_to_newer_same_process_duel_launch_no_input_no_spending",
        ],
    );
    Ok(MtgoCompetitiveEventMatchLaunchBindingCommitmentsV1 {
        event_runtime_commitment_sha256: runtime.runtime_commitment_sha256.clone(),
        source_launch_identity_commitment_sha256: source.launch_identity_commitment_sha256.clone(),
        match_launch_binding_commitment_sha256,
        entry_ratification_commitment_sha256: runtime.entry_ratification_commitment_sha256.clone(),
        entry_authorization_sha256: runtime.entry_authorization_sha256.clone(),
        selected_deck_label_sha256: runtime.selected_deck_label_sha256.clone(),
        selected_deck_region_sha256: runtime.selected_deck_region_sha256.clone(),
        deck_manifest_sha256: runtime.deck_manifest_sha256.clone(),
        deck_format_sha256: runtime.deck_format_sha256.clone(),
        policy_deployment_commitment_sha256: runtime.policy_deployment_commitment_sha256.clone(),
        event_identity_sha256: event_identity_sha256.to_owned(),
        match_identity_sha256: match_identity_sha256.to_owned(),
        process_continuity_commitment_sha256: source.process_continuity_commitment_sha256.clone(),
        event_runtime_lifecycle_snapshot_commitment_sha256: runtime
            .current_lifecycle_snapshot_commitment_sha256
            .clone(),
        launch_lifecycle_snapshot_commitment_sha256: source
            .lifecycle_snapshot_commitment_sha256
            .clone(),
        launch_lifecycle_evaluation_commitment_sha256: source
            .lifecycle_evaluation_commitment_sha256
            .clone(),
        launch_lifecycle_profile_admission_commitment_sha256: source
            .lifecycle_profile_admission_commitment_sha256
            .clone(),
        event_kind: runtime.event_kind,
        game_number: source.game_number,
        event_runtime_frame_id: runtime.current_frame_id,
        event_runtime_frame_sequence: runtime.current_frame_sequence,
        launch_frame_id: source.frame_id,
        launch_frame_sequence: source.frame_sequence,
        event_runtime_captured_at_unix_millis: current_captured_at_unix_millis,
        launch_captured_at_unix_millis: source.captured_at_unix_millis,
    })
}

fn validate_game_session_against_event_runtime_v1(
    runtime: &OpaqueMtgoCompetitiveEventRuntimeV1,
    session: &OpaqueMtgoCompetitiveGestureGameSessionV1,
) -> Result<(), String> {
    let game = session.commitments_v1();
    let gameplay = &session.launch.pass_match_launch.authorization;
    validate_game_session_commitments_against_event_runtime_v1(
        &runtime.commitments,
        &game,
        gameplay,
    )
}

fn validate_competitive_event_authorization_lineage_v1(
    entry: &MtgoReviewedCompetitiveEntryRatificationCandidateV1,
    lifecycle: &MtgoReviewedCompetitiveLifecycleRatificationCandidateV1,
) -> Result<(), String> {
    if lifecycle.event_kind != entry.event_kind
        || lifecycle.approved_account_alias_sha256 != entry.account_alias_sha256
        || lifecycle.correspondence_sha256 != entry.correspondence_sha256
        || lifecycle.permission_review_commitment_sha256
            != entry.permission_review_commitment_sha256
        || lifecycle.mode_authorization_commitment_sha256
            != entry.mode_authorization_commitment_sha256
    {
        return Err(
            "competitive entry and lifecycle authorities do not share one exact reviewed permission lineage"
                .to_owned(),
        );
    }
    Ok(())
}

fn validate_game_session_commitments_against_event_runtime_v1(
    runtime: &MtgoCompetitiveEventRuntimeCommitmentsV1,
    game: &MtgoCompetitiveGestureGameSessionCommitmentsV1,
    gameplay: &MtgoCompetitiveMatchGameplayAuthorizationV1,
) -> Result<(), String> {
    let deck_unbound = game.entry_ratification_commitment_sha256.is_none()
        && game.selected_deck_label_sha256.is_none()
        && game.selected_deck_region_sha256.is_none()
        && game.deck_manifest_sha256.is_none()
        && game.deck_format_sha256.is_none()
        && game.policy_deployment_commitment_sha256.is_none();
    let deck_exact = game.entry_ratification_commitment_sha256.as_deref()
        == Some(runtime.entry_ratification_commitment_sha256.as_str())
        && game.selected_deck_label_sha256.as_deref()
            == Some(runtime.selected_deck_label_sha256.as_str())
        && game.selected_deck_region_sha256.as_deref()
            == Some(runtime.selected_deck_region_sha256.as_str())
        && game.deck_manifest_sha256.as_deref() == Some(runtime.deck_manifest_sha256.as_str())
        && game.deck_format_sha256.as_deref() == Some(runtime.deck_format_sha256.as_str())
        && game.policy_deployment_commitment_sha256.as_deref()
            == Some(runtime.policy_deployment_commitment_sha256.as_str());
    let pregame_exact = runtime
        .last_completed_pregame
        .as_ref()
        .is_some_and(|pregame| {
            runtime.current_match_identity_sha256.as_deref()
                == Some(pregame.match_identity_sha256.as_str())
                && runtime.current_game_number == Some(pregame.game_number)
                && pregame.completion_frame_sequence <= game.valid_through_frame_sequence
        });
    if runtime.closed_to_event_browser
        || runtime.current_phase != MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
        || game.event_kind != runtime.event_kind
        || game.mode_authorization_commitment_sha256 != runtime.mode_authorization_commitment_sha256
        || game.correspondence_sha256 != runtime.correspondence_sha256
        || game.permission_review_commitment_sha256 != runtime.permission_review_commitment_sha256
        || gameplay.account_alias_sha256 != runtime.approved_account_alias_sha256
        || gameplay.entry_authorization_sha256 != runtime.entry_authorization_sha256
        || gameplay.event_identity_sha256 != runtime.bound_event_identity_sha256
        || runtime.current_match_identity_sha256.as_deref()
            != Some(gameplay.match_identity_sha256.as_str())
        || runtime.current_game_number != Some(game.game_number)
        || gameplay.game_number != game.game_number
        || game.valid_from_frame_sequence < runtime.current_frame_sequence
        || game.valid_from_frame_sequence > game.valid_through_frame_sequence
        || !pregame_exact
        || !(deck_unbound || deck_exact)
    {
        return Err(
            "competitive game session differs from the current exact entry, selected deck, event, match, game, account, or frame lifetime"
                .to_owned(),
        );
    }
    Ok(())
}

fn competitive_event_runtime_commitment_v1(
    domain: &[u8],
    prior_runtime_commitment_sha256: Option<&str>,
    value: &MtgoCompetitiveEventRuntimeCommitmentsV1,
    receipt: &[u8],
) -> String {
    hash_parts_v2(
        domain,
        &[
            prior_runtime_commitment_sha256.unwrap_or("").as_bytes(),
            value.entry_confirmation_receipt_sha256.as_bytes(),
            value.entry_ratification_commitment_sha256.as_bytes(),
            value.entry_authorization_sha256.as_bytes(),
            value.correspondence_sha256.as_bytes(),
            value.permission_review_commitment_sha256.as_bytes(),
            value.deck_list_sha256.as_bytes(),
            value.deck_manifest_sha256.as_bytes(),
            value.deck_format_sha256.as_bytes(),
            value
                .player_known_current_deck_configuration_commitment_sha256
                .as_bytes(),
            value.selected_deck_label_sha256.as_bytes(),
            value.selected_deck_region_sha256.as_bytes(),
            value.policy_deployment_commitment_sha256.as_bytes(),
            value.lifecycle_authorization_commitment_sha256.as_bytes(),
            value.mode_authorization_commitment_sha256.as_bytes(),
            value.navigation_profile_commitment_sha256.as_bytes(),
            value
                .navigation_profile_admission_commitment_sha256
                .as_bytes(),
            value.approved_account_alias_sha256.as_bytes(),
            value.bound_event_identity_sha256.as_bytes(),
            competitive_event_kind_tag_v1(value.event_kind),
            competitive_lifecycle_phase_tag_v1(value.current_phase),
            value
                .current_lifecycle_snapshot_commitment_sha256
                .as_bytes(),
            value
                .current_match_identity_sha256
                .as_deref()
                .unwrap_or("")
                .as_bytes(),
            value
                .current_game_number
                .map_or([0_u8, 0_u8], |game| [1_u8, game])
                .as_slice(),
            value.current_frame_id.to_be_bytes().as_slice(),
            value.current_frame_sequence.to_be_bytes().as_slice(),
            value.lifecycle_transition_count.to_be_bytes().as_slice(),
            value
                .confirmed_lifecycle_action_count
                .to_be_bytes()
                .as_slice(),
            value
                .observed_lifecycle_advance_count
                .to_be_bytes()
                .as_slice(),
            value.pregame_session_count.to_be_bytes().as_slice(),
            value
                .last_completed_pregame
                .as_ref()
                .map(|completed| completed.completion_receipt_sha256.as_str())
                .unwrap_or("")
                .as_bytes(),
            value
                .last_completed_pregame
                .as_ref()
                .map(|completed| completed.pregame_session_commitment_sha256.as_str())
                .unwrap_or("")
                .as_bytes(),
            value
                .last_completed_pregame
                .as_ref()
                .map(|completed| completed.final_observation_commitment_sha256.as_str())
                .unwrap_or("")
                .as_bytes(),
            value
                .last_completed_pregame
                .as_ref()
                .map(|completed| completed.match_identity_sha256.as_str())
                .unwrap_or("")
                .as_bytes(),
            value
                .last_completed_pregame
                .as_ref()
                .map_or([0_u8, 0_u8], |completed| [1_u8, completed.game_number])
                .as_slice(),
            value
                .last_completed_pregame
                .as_ref()
                .map_or([0_u8; 9], |completed| {
                    let mut encoded = [0_u8; 9];
                    encoded[0] = 1;
                    encoded[1..]
                        .copy_from_slice(&completed.completion_frame_sequence.to_be_bytes());
                    encoded
                })
                .as_slice(),
            value.gameplay_lease_count.to_be_bytes().as_slice(),
            value
                .last_returned_gameplay_frame_sequence
                .map_or([0_u8; 9], |sequence| {
                    let mut encoded = [0_u8; 9];
                    encoded[0] = 1;
                    encoded[1..].copy_from_slice(&sequence.to_be_bytes());
                    encoded
                })
                .as_slice(),
            value
                .event_monitor_chain_commitment_sha256
                .as_deref()
                .unwrap_or("")
                .as_bytes(),
            value
                .event_monitor_observation_count
                .to_be_bytes()
                .as_slice(),
            &[
                u8::from(value.terminal_event_record_confirmed),
                u8::from(value.closed_to_event_browser),
            ],
            receipt,
            b"one_exact_event_move_only_no_reentry_no_additional_spending",
        ],
    )
}

fn competitive_lifecycle_phase_tag_v1(phase: MtgoCompetitiveLifecyclePhaseV1) -> &'static [u8] {
    match phase {
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

fn observed_lifecycle_advance_tag_v1(
    observed: MtgoObservedCompetitiveLifecycleAdvanceV1,
) -> &'static [u8] {
    match observed {
        MtgoObservedCompetitiveLifecycleAdvanceV1::PairingPosted => b"pairing_posted",
        MtgoObservedCompetitiveLifecycleAdvanceV1::GameEndedForSideboarding => {
            b"game_ended_for_sideboarding"
        }
        MtgoObservedCompetitiveLifecycleAdvanceV1::MatchEnded => b"match_ended",
        MtgoObservedCompetitiveLifecycleAdvanceV1::EventEnded => b"event_ended",
        MtgoObservedCompetitiveLifecycleAdvanceV1::ConnectionInterrupted => {
            b"connection_interrupted"
        }
    }
}

fn competitive_event_kind_tag_v1(event_kind: MtgoCompetitiveEventKindV1) -> &'static [u8] {
    match event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    }
}

fn competitive_lifecycle_action_tag_v1(action: MtgoCompetitiveLifecycleActionV1) -> &'static [u8] {
    match action {
        MtgoCompetitiveLifecycleActionV1::OpenEntryReview => b"open_entry_review",
        MtgoCompetitiveLifecycleActionV1::CancelEntry => b"cancel_entry",
        MtgoCompetitiveLifecycleActionV1::ConfirmEntry => b"confirm_entry",
        MtgoCompetitiveLifecycleActionV1::AcceptPairing => b"accept_pairing",
        MtgoCompetitiveLifecycleActionV1::SubmitSideboard => b"submit_sideboard_no_changes",
        MtgoCompetitiveLifecycleActionV1::ContinueAfterMatch => b"continue_after_match",
        MtgoCompetitiveLifecycleActionV1::ResumeMatch => b"resume_match",
        MtgoCompetitiveLifecycleActionV1::CloseCompletedEvent => b"close_completed_event",
    }
}

fn validate_private_match_authorization_v3(
    authorization: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
) -> Result<(), String> {
    validate_authorization_for_mode_v1(authorization, MtgoRuntimeModeV1::PrivateMatchInput)
        .map_err(|error| format!("private-match authorization rejected: {error}"))?;
    validate_visible_account_alias_v1(authorization, visible_account_alias)?;
    Ok(())
}

fn validate_visible_account_alias_v1(
    authorization: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
) -> Result<(), String> {
    if visible_account_alias.is_empty()
        || visible_account_alias.len() > 64
        || visible_account_alias.chars().any(char::is_control)
        || format!("{:x}", Sha256::digest(visible_account_alias.as_bytes()))
            != authorization.account_alias_sha256
    {
        return Err("the visible account alias does not match the authorized account".to_owned());
    }
    Ok(())
}

fn validate_competitive_duel_pass_authorization_v1(
    authorization: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
    event_kind: MtgoCompetitiveEventKindV1,
) -> Result<String, String> {
    validate_exact_competitive_mode_authorization_v1(
        authorization,
        visible_account_alias,
        event_kind,
        "competitive Pass ratification",
    )
}

fn validate_exact_competitive_mode_authorization_v1(
    authorization: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
    event_kind: MtgoCompetitiveEventKindV1,
    context: &str,
) -> Result<String, String> {
    let runtime_mode = match event_kind {
        MtgoCompetitiveEventKindV1::League => MtgoRuntimeModeV1::LeagueInput,
        MtgoCompetitiveEventKindV1::Challenge => MtgoRuntimeModeV1::ChallengeInput,
    };
    validate_authorization_for_mode_v1(authorization, runtime_mode)
        .map_err(|error| format!("{context} authorization rejected: {error}"))?;
    validate_visible_account_alias_v1(authorization, visible_account_alias)?;

    let selected_mode_is_exact = match event_kind {
        MtgoCompetitiveEventKindV1::League => {
            authorization.league_input && !authorization.challenge_input
        }
        MtgoCompetitiveEventKindV1::Challenge => {
            authorization.challenge_input && !authorization.league_input
        }
    };
    if !selected_mode_is_exact
        || authorization.shadow_observation
        || authorization.private_match_input
        || authorization.open_play_input
        || authorization.other_prize_event_input
    {
        return Err(format!(
            "{context} requires exactly one League or Challenge input mode"
        ));
    }
    competitive_mode_authorization_commitment_v1(authorization, event_kind)
        .map_err(|error| format!("{context} mode commitment rejected: {error}"))
}

fn competitive_duel_pass_authorization_commitment_v1(
    authorization: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
    event_kind: MtgoCompetitiveEventKindV1,
    mode_authorization_commitment_sha256: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(COMPETITIVE_DUEL_PASS_AUTHORIZATION_DOMAIN_V1);
    let schema_version = authorization.schema_version.to_be_bytes();
    let event_kind_bytes: &[u8] = match event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    for part in [
        schema_version.as_slice(),
        authorization.account_alias_sha256.as_bytes(),
        authorization.written_permission_sha256.as_bytes(),
        visible_account_alias.as_bytes(),
        event_kind_bytes,
        mode_authorization_commitment_sha256.as_bytes(),
        b"priority_pass",
        b"one_verified_left_click",
        b"no_event_entry_or_purchase_authority",
    ] {
        update_hash_part_v3(&mut hasher, part);
    }
    format!("{:x}", hasher.finalize())
}

fn competitive_duel_pass_authorization_from_review_commitment_v2(
    authorization: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
    event_kind: MtgoCompetitiveEventKindV1,
    mode_authorization_commitment_sha256: &str,
    permission_review_commitment_sha256: &str,
) -> String {
    let base_commitment_sha256 = competitive_duel_pass_authorization_commitment_v1(
        authorization,
        visible_account_alias,
        event_kind,
        mode_authorization_commitment_sha256,
    );
    hash_parts_v2(
        COMPETITIVE_DUEL_PASS_AUTHORIZATION_FROM_REVIEW_DOMAIN_V2,
        &[
            base_commitment_sha256.as_bytes(),
            permission_review_commitment_sha256.as_bytes(),
            b"exact_correspondence_bytes_and_human_review_required",
        ],
    )
}

fn competitive_pregame_authorization_commitment_v1(
    scope: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
    event_kind: MtgoCompetitiveEventKindV1,
    mode_authorization_commitment_sha256: &str,
    permission_review_commitment_sha256: &str,
    heuristic_commitments: (&str, &str, &str, &str),
) -> String {
    hash_parts_v2(
        COMPETITIVE_EVENT_PREGAME_AUTHORIZATION_DOMAIN_V1,
        &[
            scope.account_alias_sha256.as_bytes(),
            scope.written_permission_sha256.as_bytes(),
            visible_account_alias.as_bytes(),
            competitive_event_kind_tag_v1(event_kind),
            mode_authorization_commitment_sha256.as_bytes(),
            permission_review_commitment_sha256.as_bytes(),
            heuristic_commitments.0.as_bytes(),
            heuristic_commitments.1.as_bytes(),
            heuristic_commitments.2.as_bytes(),
            heuristic_commitments.3.as_bytes(),
            b"keep_mulligan_london_select_submit_one_click_each_no_entry_no_spending",
        ],
    )
}

fn validate_competitive_pregame_authorization_for_prepared_v1(
    authorization: &RatifiedMtgoCompetitivePregameAuthorizationV1,
    prepared: &OpaqueMtgoPreparedCompetitivePregameActionV1,
) -> Result<(), String> {
    let reviewed = &authorization.commitments;
    let mode_authorization_commitment_sha256 = validate_exact_competitive_mode_authorization_v1(
        &authorization.scope,
        &authorization.visible_account_alias,
        reviewed.event_kind,
        "competitive pregame execution",
    )?;
    let expected_ratification = competitive_pregame_authorization_commitment_v1(
        &authorization.scope,
        &authorization.visible_account_alias,
        reviewed.event_kind,
        &mode_authorization_commitment_sha256,
        authorization
            ._permission_correspondence
            .review_commitment_sha256(),
        (
            &reviewed.heuristic_profile_commitment_sha256,
            &reviewed.heuristic_algorithm_commitment_sha256,
            &reviewed.heuristic_review_commitment_sha256,
            &reviewed.heuristic_admission_commitment_sha256,
        ),
    );
    let launch = &prepared._plan._session.match_launch;
    let plan = &prepared._plan.commitments;
    if reviewed.ratification_commitment_sha256 != expected_ratification
        || reviewed.permission_review_commitment_sha256
            != authorization
                ._permission_correspondence
                .review_commitment_sha256()
        || reviewed.account_alias_sha256 != authorization.scope.account_alias_sha256
        || reviewed.correspondence_sha256 != authorization.scope.written_permission_sha256
        || reviewed.mode_authorization_commitment_sha256 != mode_authorization_commitment_sha256
        || reviewed.event_kind != prepared.commitments.event_kind
        || reviewed.event_kind != launch.authorization.event_kind
        || reviewed.account_alias_sha256 != launch.authorization.account_alias_sha256
        || reviewed.correspondence_sha256 != launch.authorization.written_permission_sha256
        || reviewed.mode_authorization_commitment_sha256
            != launch.mode_authorization_commitment_sha256
        || reviewed.heuristic_profile_commitment_sha256 != plan.heuristic_profile_commitment_sha256
        || reviewed.heuristic_algorithm_commitment_sha256
            != plan.heuristic_algorithm_commitment_sha256
        || reviewed.heuristic_review_commitment_sha256 != plan.heuristic_review_commitment_sha256
        || reviewed.heuristic_admission_commitment_sha256
            != plan.heuristic_admission_commitment_sha256
        || prepared.commitments.match_identity_sha256 != launch.authorization.match_identity_sha256
        || prepared.commitments.game_number != launch.authorization.game_number
    {
        return Err(
            "competitive pregame authority differs from the permission, heuristic, event, match, or game"
                .to_owned(),
        );
    }
    Ok(())
}

fn validate_competitive_match_launch_record_v1(
    scope: &MtgoAuthorizationScopeV1,
    authorization: &MtgoCompetitiveMatchGameplayAuthorizationV1,
) -> Result<(), String> {
    if authorization.schema_version != MTGO_COMPETITIVE_MATCH_GAMEPLAY_AUTHORIZATION_SCHEMA_V1
        || !authorization.exact_match_gameplay_authorized
        || !(1..=3).contains(&authorization.game_number)
        || authorization.valid_through_frame_sequence == 0
        || authorization.account_alias_sha256 != scope.account_alias_sha256
        || authorization.written_permission_sha256 != scope.written_permission_sha256
    {
        return Err("the competitive match launch record is inactive or mismatched".to_owned());
    }
    for value in [
        authorization.account_alias_sha256.as_str(),
        authorization.written_permission_sha256.as_str(),
        authorization.event_identity_sha256.as_str(),
        authorization.match_identity_sha256.as_str(),
        authorization.entry_authorization_sha256.as_str(),
        authorization.owner_launch_authorization_sha256.as_str(),
    ] {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("the competitive match launch contains an invalid commitment".to_owned());
        }
    }
    if authorization.event_identity_sha256 == authorization.match_identity_sha256
        || authorization.written_permission_sha256 == authorization.entry_authorization_sha256
        || authorization.written_permission_sha256
            == authorization.owner_launch_authorization_sha256
        || authorization.entry_authorization_sha256
            == authorization.owner_launch_authorization_sha256
    {
        return Err(
            "competitive permission, entry, match, and launch records must be distinct".to_owned(),
        );
    }
    Ok(())
}

fn competitive_match_launch_commitment_v1(
    visible_account_alias: &str,
    mode_authorization_commitment_sha256: &str,
    gameplay_authorization_commitment_sha256: &str,
    authorization: &MtgoCompetitiveMatchGameplayAuthorizationV1,
) -> String {
    let event_kind_bytes: &[u8] = match authorization.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let mut hasher = Sha256::new();
    hasher.update(COMPETITIVE_MATCH_LAUNCH_AUTHORIZATION_DOMAIN_V1);
    for part in [
        visible_account_alias.as_bytes(),
        mode_authorization_commitment_sha256.as_bytes(),
        gameplay_authorization_commitment_sha256.as_bytes(),
        event_kind_bytes,
        authorization.event_identity_sha256.as_bytes(),
        authorization.match_identity_sha256.as_bytes(),
        &[authorization.game_number],
        authorization.entry_authorization_sha256.as_bytes(),
        authorization.owner_launch_authorization_sha256.as_bytes(),
        authorization
            .valid_through_frame_sequence
            .to_be_bytes()
            .as_slice(),
        b"owner_launched_exact_match_priority_pass_only_no_event_entry_authority",
    ] {
        update_hash_part_v3(&mut hasher, part);
    }
    format!("{:x}", hasher.finalize())
}

fn validate_competitive_game_session_authorities_v1(
    authorization: &RatifiedMtgoCompetitiveDuelPassAuthorizationV1,
    match_launch: &RatifiedMtgoCompetitiveMatchLaunchV1,
) -> Result<(), String> {
    if authorization.event_kind != match_launch.authorization.event_kind
        || authorization.mode_authorization_commitment_sha256
            != match_launch.mode_authorization_commitment_sha256
        || authorization.scope.account_alias_sha256
            != match_launch.authorization.account_alias_sha256
        || authorization.scope.written_permission_sha256
            != match_launch.authorization.written_permission_sha256
        || match_launch.valid_from_frame_sequence == 0
        || match_launch.valid_from_frame_sequence
            > match_launch.authorization.valid_through_frame_sequence
    {
        return Err(
            "competitive game session authorities describe different modes or lifetimes".to_owned(),
        );
    }
    validate_competitive_match_launch_record_v1(&authorization.scope, &match_launch.authorization)?;
    let expected_mode = validate_competitive_duel_pass_authorization_v1(
        &authorization.scope,
        &authorization.visible_account_alias,
        authorization.event_kind,
    )?;
    if expected_mode != authorization.mode_authorization_commitment_sha256 {
        return Err("competitive game session mode commitment changed".to_owned());
    }
    let expected_gameplay =
        competitive_match_gameplay_authorization_commitment_v1(&match_launch.authorization)
            .map_err(|error| format!("competitive game session match commitment: {error}"))?;
    let expected_launch = competitive_match_launch_commitment_v1(
        &authorization.visible_account_alias,
        &expected_mode,
        &expected_gameplay,
        &match_launch.authorization,
    );
    if expected_gameplay != match_launch.gameplay_authorization_commitment_sha256
        || expected_launch != match_launch.launch_authorization_commitment_sha256
    {
        return Err("competitive game session match launch commitment changed".to_owned());
    }
    Ok(())
}

fn initial_competitive_game_session_commitment_v1(
    authorization: &RatifiedMtgoCompetitiveDuelPassAuthorizationV1,
    match_launch: &RatifiedMtgoCompetitiveMatchLaunchV1,
) -> String {
    let event_kind_bytes: &[u8] = match authorization.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    hash_parts_v2(
        COMPETITIVE_GAME_SESSION_INITIAL_DOMAIN_V1,
        &[
            authorization.authorization_commitment_sha256.as_bytes(),
            match_launch
                .launch_authorization_commitment_sha256
                .as_bytes(),
            match_launch
                .gameplay_authorization_commitment_sha256
                .as_bytes(),
            authorization
                .mode_authorization_commitment_sha256
                .as_bytes(),
            authorization.scope.account_alias_sha256.as_bytes(),
            authorization.scope.written_permission_sha256.as_bytes(),
            event_kind_bytes,
            &[match_launch.authorization.game_number],
            match_launch
                .valid_from_frame_sequence
                .to_be_bytes()
                .as_slice(),
            match_launch
                .authorization
                .valid_through_frame_sequence
                .to_be_bytes()
                .as_slice(),
            b"move_only_sequential_visible_postconditions_no_entry_or_spending",
        ],
    )
}

fn initial_competitive_gesture_game_session_commitment_v1(
    candidate: &MtgoReviewedCompetitiveGestureRatificationCandidateV1,
    pass_match_launch: &RatifiedMtgoCompetitiveMatchLaunchV1,
    gesture_match_launch_commitment_sha256: &str,
) -> String {
    let event_kind: &[u8] = match candidate.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    hash_parts_v2(
        COMPETITIVE_GESTURE_GAME_SESSION_INITIAL_DOMAIN_V1,
        &[
            candidate.ratification_commitment_sha256.as_bytes(),
            candidate.gesture_evaluation_commitment_sha256.as_bytes(),
            candidate
                .gesture_profile_admission_commitment_sha256
                .as_bytes(),
            pass_match_launch
                .launch_authorization_commitment_sha256
                .as_bytes(),
            pass_match_launch
                .gameplay_authorization_commitment_sha256
                .as_bytes(),
            gesture_match_launch_commitment_sha256.as_bytes(),
            event_kind,
            &[pass_match_launch.authorization.game_number],
            pass_match_launch
                .valid_from_frame_sequence
                .to_be_bytes()
                .as_slice(),
            pass_match_launch
                .authorization
                .valid_through_frame_sequence
                .to_be_bytes()
                .as_slice(),
            b"move_only_all_family_lineage_no_preparation_execution_entry_or_spending",
        ],
    )
}

fn competitive_duel_gesture_sequence_session_binding_from_parts_v1(
    sequence: &MtgoOpaqueCompetitiveDuelGestureSequenceCommitmentsV1,
    session: &MtgoCompetitiveGestureGameSessionCommitmentsV1,
) -> Result<MtgoSessionBoundCompetitiveDuelGestureCommitmentsV1, String> {
    let entry_ratification_commitment_sha256 = session
        .entry_ratification_commitment_sha256
        .as_deref()
        .ok_or("gesture session is not bound to an exact event entry and selected deck")?;
    let selected_deck_label_sha256 = session
        .selected_deck_label_sha256
        .as_deref()
        .ok_or("gesture session is not bound to an exact selected-deck label")?;
    let selected_deck_region_sha256 = session
        .selected_deck_region_sha256
        .as_deref()
        .ok_or("gesture session is not bound to exact selected-deck pixels")?;
    let deck_manifest_sha256 = session
        .deck_manifest_sha256
        .as_deref()
        .ok_or("gesture session is not bound to an exact deck manifest")?;
    let deck_format_sha256 = session
        .deck_format_sha256
        .as_deref()
        .ok_or("gesture session is not bound to an exact deck format")?;
    let policy_deployment_commitment_sha256 = session
        .policy_deployment_commitment_sha256
        .as_deref()
        .ok_or("gesture session is not bound to an exact policy deployment")?;
    for commitment in [
        sequence.competitive_action_plan_commitment_sha256.as_str(),
        sequence.gesture_plan_commitment_sha256.as_str(),
        sequence
            .competitive_mode_authorization_commitment_sha256
            .as_str(),
        sequence
            .competitive_match_gameplay_authorization_commitment_sha256
            .as_str(),
        sequence.policy_deployment_commitment_sha256.as_str(),
        sequence.current_stage_binding_commitment_sha256.as_str(),
        sequence.current_opaque_stage_commitment_sha256.as_str(),
        sequence.sequence_commitment_sha256.as_str(),
        session.session_commitment_sha256.as_str(),
        session
            .general_gesture_permission_commitment_sha256
            .as_str(),
        session.mode_authorization_commitment_sha256.as_str(),
        session.pass_match_launch_commitment_sha256.as_str(),
        session
            .match_gameplay_authorization_commitment_sha256
            .as_str(),
        session.gesture_match_launch_commitment_sha256.as_str(),
        session.gesture_evaluation_commitment_sha256.as_str(),
        session.gesture_profile_admission_commitment_sha256.as_str(),
        entry_ratification_commitment_sha256,
        selected_deck_label_sha256,
        selected_deck_region_sha256,
        deck_manifest_sha256,
        deck_format_sha256,
        policy_deployment_commitment_sha256,
    ] {
        if !is_sha256_v2(commitment) {
            return Err("gesture session binding contains an invalid commitment".to_owned());
        }
    }
    if sequence.last_visible_transition_commitment_sha256.is_some()
        || sequence.current_stage_index != 0
        || sequence.observed_stage_count != 1
        || sequence.gesture_stage_count == 0
        || sequence.gesture_stage_count > 64
        || sequence.current_frame_id == 0
        || sequence.current_frame_sequence == 0
        || !canonical_duel_gesture_action_families_v1().contains(&sequence.selected_action_family)
    {
        return Err(
            "gesture session binding requires one canonical source stage of a complete plan"
                .to_owned(),
        );
    }
    if sequence.event_kind != session.event_kind
        || sequence.game_number != session.game_number
        || sequence.competitive_mode_authorization_commitment_sha256
            != session.mode_authorization_commitment_sha256
        || sequence.competitive_match_gameplay_authorization_commitment_sha256
            != session.match_gameplay_authorization_commitment_sha256
        || sequence.policy_deployment_commitment_sha256 != policy_deployment_commitment_sha256
        || sequence.gameplay_authorization_valid_through_frame_sequence
            != session.valid_through_frame_sequence
        || sequence.current_frame_sequence < session.valid_from_frame_sequence
        || sequence.current_frame_sequence <= session.last_confirmed_frame_sequence
        || sequence.current_frame_sequence > session.valid_through_frame_sequence
    {
        return Err(
            "gesture sequence does not match the exact game session mode, policy deployment, game, or frame lifetime"
                .to_owned(),
        );
    }
    let family_json = serde_json::to_vec(&sequence.selected_action_family)
        .map_err(|error| format!("serialize session-bound gesture family: {error}"))?;
    let event_kind: &[u8] = match sequence.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let binding_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_GESTURE_SESSION_SEQUENCE_BINDING_DOMAIN_V1,
        &[
            session.session_commitment_sha256.as_bytes(),
            session
                .general_gesture_permission_commitment_sha256
                .as_bytes(),
            session.gesture_match_launch_commitment_sha256.as_bytes(),
            session.gesture_evaluation_commitment_sha256.as_bytes(),
            session
                .gesture_profile_admission_commitment_sha256
                .as_bytes(),
            entry_ratification_commitment_sha256.as_bytes(),
            selected_deck_label_sha256.as_bytes(),
            selected_deck_region_sha256.as_bytes(),
            deck_manifest_sha256.as_bytes(),
            deck_format_sha256.as_bytes(),
            policy_deployment_commitment_sha256.as_bytes(),
            sequence
                .competitive_action_plan_commitment_sha256
                .as_bytes(),
            sequence.gesture_plan_commitment_sha256.as_bytes(),
            sequence.current_stage_binding_commitment_sha256.as_bytes(),
            sequence.current_opaque_stage_commitment_sha256.as_bytes(),
            sequence.sequence_commitment_sha256.as_bytes(),
            family_json.as_slice(),
            event_kind,
            &[sequence.game_number],
            sequence.current_frame_sequence.to_be_bytes().as_slice(),
            sequence.gesture_stage_count.to_be_bytes().as_slice(),
            session.confirmed_action_count.to_be_bytes().as_slice(),
            b"one_source_stage_bound_to_exact_all_family_session_no_preparation_execution_or_input",
        ],
    );
    Ok(MtgoSessionBoundCompetitiveDuelGestureCommitmentsV1 {
        binding_commitment_sha256,
        game_session_commitment_sha256: session.session_commitment_sha256.clone(),
        gesture_match_launch_commitment_sha256: session
            .gesture_match_launch_commitment_sha256
            .clone(),
        gesture_evaluation_commitment_sha256: session.gesture_evaluation_commitment_sha256.clone(),
        gesture_profile_admission_commitment_sha256: session
            .gesture_profile_admission_commitment_sha256
            .clone(),
        entry_ratification_commitment_sha256: entry_ratification_commitment_sha256.to_owned(),
        selected_deck_label_sha256: selected_deck_label_sha256.to_owned(),
        selected_deck_region_sha256: selected_deck_region_sha256.to_owned(),
        deck_manifest_sha256: deck_manifest_sha256.to_owned(),
        deck_format_sha256: deck_format_sha256.to_owned(),
        policy_deployment_commitment_sha256: policy_deployment_commitment_sha256.to_owned(),
        competitive_action_plan_commitment_sha256: sequence
            .competitive_action_plan_commitment_sha256
            .clone(),
        gesture_plan_commitment_sha256: sequence.gesture_plan_commitment_sha256.clone(),
        gesture_sequence_commitment_sha256: sequence.sequence_commitment_sha256.clone(),
        selected_action_family: sequence.selected_action_family,
        event_kind: sequence.event_kind,
        game_number: sequence.game_number,
        source_frame_sequence: sequence.current_frame_sequence,
        gesture_stage_count: sequence.gesture_stage_count,
    })
}

fn competitive_duel_gesture_source_preparation_from_parts_v1(
    bound: &MtgoSessionBoundCompetitiveDuelGestureCommitmentsV1,
    session: &MtgoCompetitiveGestureGameSessionCommitmentsV1,
    prepared: &MtgoOpaqueCompetitiveDuelGestureSourcePreparationCommitmentsV1,
) -> Result<MtgoPreparedCompetitiveDuelGestureSourceStageCommitmentsV1, String> {
    for commitment in [
        bound.binding_commitment_sha256.as_str(),
        bound.game_session_commitment_sha256.as_str(),
        bound.gesture_match_launch_commitment_sha256.as_str(),
        bound.gesture_sequence_commitment_sha256.as_str(),
        bound.competitive_action_plan_commitment_sha256.as_str(),
        bound.gesture_plan_commitment_sha256.as_str(),
        session.session_commitment_sha256.as_str(),
        session.mode_authorization_commitment_sha256.as_str(),
        session
            .match_gameplay_authorization_commitment_sha256
            .as_str(),
        prepared.source_sequence_commitment_sha256.as_str(),
        prepared.prepared_sequence_commitment_sha256.as_str(),
        prepared.competitive_action_plan_commitment_sha256.as_str(),
        prepared.gesture_plan_commitment_sha256.as_str(),
        prepared
            .competitive_mode_authorization_commitment_sha256
            .as_str(),
        prepared
            .competitive_match_gameplay_authorization_commitment_sha256
            .as_str(),
        prepared.fresh_stage_binding_commitment_sha256.as_str(),
        prepared.fresh_capture_commitment_sha256.as_str(),
        prepared.fresh_perception_result_commitment_sha256.as_str(),
        prepared
            .gesture_target_runtime_identity_commitment_sha256
            .as_str(),
        prepared.gesture_target_request_commitment_sha256.as_str(),
        prepared
            .before_input_postcondition_verification_commitment_sha256
            .as_str(),
        prepared.primitive_commitment_sha256.as_str(),
        prepared.preparation_commitment_sha256.as_str(),
    ] {
        if !is_sha256_v2(commitment) {
            return Err("gesture source preparation contains an invalid commitment".to_owned());
        }
    }
    let expected_fresh_sequence = bound
        .source_frame_sequence
        .checked_add(1)
        .ok_or("gesture source preparation sequence overflow")?;
    if bound.game_session_commitment_sha256 != session.session_commitment_sha256
        || prepared.source_sequence_commitment_sha256 != bound.gesture_sequence_commitment_sha256
        || prepared.competitive_action_plan_commitment_sha256
            != bound.competitive_action_plan_commitment_sha256
        || prepared.gesture_plan_commitment_sha256 != bound.gesture_plan_commitment_sha256
        || prepared.competitive_mode_authorization_commitment_sha256
            != session.mode_authorization_commitment_sha256
        || prepared.competitive_match_gameplay_authorization_commitment_sha256
            != session.match_gameplay_authorization_commitment_sha256
        || prepared.selected_action_family != bound.selected_action_family
        || prepared.event_kind != bound.event_kind
        || prepared.event_kind != session.event_kind
        || prepared.game_number != bound.game_number
        || prepared.game_number != session.game_number
        || prepared.stage_index != 0
        || prepared.gesture_stage_count != bound.gesture_stage_count
        || prepared.target_count == 0
        || prepared.target_count > 2
        || prepared.fresh_frame_id == 0
        || prepared.fresh_frame_sequence != expected_fresh_sequence
        || prepared.fresh_frame_sequence <= session.last_confirmed_frame_sequence
        || prepared.fresh_frame_sequence < session.valid_from_frame_sequence
        || prepared.fresh_frame_sequence > session.valid_through_frame_sequence
        || prepared.gameplay_authorization_valid_through_frame_sequence
            != session.valid_through_frame_sequence
        || prepared.fresh_captured_at_unix_millis == 0
    {
        return Err(
            "fresh gesture source stage does not match the exact competitive session".to_owned(),
        );
    }
    let family_json = serde_json::to_vec(&prepared.selected_action_family)
        .map_err(|error| format!("serialize prepared session gesture family: {error}"))?;
    let event_kind_json = serde_json::to_vec(&prepared.event_kind)
        .map_err(|error| format!("serialize prepared session event kind: {error}"))?;
    let preparation_binding_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_GESTURE_SESSION_SOURCE_PREPARATION_DOMAIN_V1,
        &[
            bound.binding_commitment_sha256.as_bytes(),
            session.session_commitment_sha256.as_bytes(),
            session.gesture_match_launch_commitment_sha256.as_bytes(),
            prepared.source_sequence_commitment_sha256.as_bytes(),
            prepared.prepared_sequence_commitment_sha256.as_bytes(),
            prepared
                .competitive_action_plan_commitment_sha256
                .as_bytes(),
            prepared.gesture_plan_commitment_sha256.as_bytes(),
            prepared.fresh_stage_binding_commitment_sha256.as_bytes(),
            prepared.fresh_capture_commitment_sha256.as_bytes(),
            prepared
                .fresh_perception_result_commitment_sha256
                .as_bytes(),
            prepared
                .gesture_target_runtime_identity_commitment_sha256
                .as_bytes(),
            prepared.gesture_target_request_commitment_sha256.as_bytes(),
            prepared
                .before_input_postcondition_verification_commitment_sha256
                .as_bytes(),
            prepared.primitive_commitment_sha256.as_bytes(),
            prepared.preparation_commitment_sha256.as_bytes(),
            &family_json,
            &event_kind_json,
            &[prepared.game_number],
            &prepared.stage_index.to_be_bytes(),
            &prepared.gesture_stage_count.to_be_bytes(),
            &prepared.target_count.to_be_bytes(),
            &prepared.fresh_frame_id.to_be_bytes(),
            &prepared.fresh_frame_sequence.to_be_bytes(),
            b"source_primitive_freshly_rechecked_pinned_runtime_targets_no_input",
        ],
    );
    Ok(MtgoPreparedCompetitiveDuelGestureSourceStageCommitmentsV1 {
        preparation_binding_commitment_sha256,
        session_sequence_binding_commitment_sha256: bound.binding_commitment_sha256.clone(),
        game_session_commitment_sha256: session.session_commitment_sha256.clone(),
        gesture_match_launch_commitment_sha256: session
            .gesture_match_launch_commitment_sha256
            .clone(),
        source_sequence_commitment_sha256: prepared.source_sequence_commitment_sha256.clone(),
        prepared_sequence_commitment_sha256: prepared.prepared_sequence_commitment_sha256.clone(),
        competitive_action_plan_commitment_sha256: prepared
            .competitive_action_plan_commitment_sha256
            .clone(),
        gesture_plan_commitment_sha256: prepared.gesture_plan_commitment_sha256.clone(),
        fresh_stage_binding_commitment_sha256: prepared
            .fresh_stage_binding_commitment_sha256
            .clone(),
        fresh_capture_commitment_sha256: prepared.fresh_capture_commitment_sha256.clone(),
        fresh_perception_result_commitment_sha256: prepared
            .fresh_perception_result_commitment_sha256
            .clone(),
        gesture_target_runtime_identity_commitment_sha256: prepared
            .gesture_target_runtime_identity_commitment_sha256
            .clone(),
        gesture_target_request_commitment_sha256: prepared
            .gesture_target_request_commitment_sha256
            .clone(),
        before_input_postcondition_verification_commitment_sha256: prepared
            .before_input_postcondition_verification_commitment_sha256
            .clone(),
        primitive_commitment_sha256: prepared.primitive_commitment_sha256.clone(),
        selected_action_family: prepared.selected_action_family,
        event_kind: prepared.event_kind,
        game_number: prepared.game_number,
        stage_index: prepared.stage_index,
        gesture_stage_count: prepared.gesture_stage_count,
        target_count: prepared.target_count,
        fresh_frame_id: prepared.fresh_frame_id,
        fresh_frame_sequence: prepared.fresh_frame_sequence,
        fresh_captured_at_unix_millis: prepared.fresh_captured_at_unix_millis,
    })
}

fn competitive_duel_gesture_continuation_preparation_from_parts_v1(
    confirmed: &MtgoConfirmedCompetitiveDuelGestureContinuationCommitmentsV1,
    session: &MtgoCompetitiveGestureGameSessionCommitmentsV1,
    prepared: &MtgoOpaqueCompetitiveDuelGestureSourcePreparationCommitmentsV1,
) -> Result<MtgoPreparedCompetitiveDuelGestureSourceStageCommitmentsV1, String> {
    for commitment in [
        confirmed.continuation_receipt_sha256.as_str(),
        confirmed.advanced_sequence_commitment_sha256.as_str(),
        confirmed.unchanged_game_session_commitment_sha256.as_str(),
        session.session_commitment_sha256.as_str(),
        prepared.source_sequence_commitment_sha256.as_str(),
        prepared.prepared_sequence_commitment_sha256.as_str(),
        prepared.competitive_action_plan_commitment_sha256.as_str(),
        prepared.gesture_plan_commitment_sha256.as_str(),
        prepared
            .competitive_mode_authorization_commitment_sha256
            .as_str(),
        prepared
            .competitive_match_gameplay_authorization_commitment_sha256
            .as_str(),
        prepared.fresh_stage_binding_commitment_sha256.as_str(),
        prepared.fresh_capture_commitment_sha256.as_str(),
        prepared.fresh_perception_result_commitment_sha256.as_str(),
        prepared
            .gesture_target_runtime_identity_commitment_sha256
            .as_str(),
        prepared.gesture_target_request_commitment_sha256.as_str(),
        prepared
            .before_input_postcondition_verification_commitment_sha256
            .as_str(),
        prepared.primitive_commitment_sha256.as_str(),
        prepared.preparation_commitment_sha256.as_str(),
    ] {
        if !is_sha256_v2(commitment) {
            return Err(
                "gesture continuation preparation contains an invalid commitment".to_owned(),
            );
        }
    }
    let expected_fresh_sequence = confirmed
        .after_frame_sequence
        .checked_add(1)
        .ok_or("gesture continuation preparation sequence overflow")?;
    if confirmed.unchanged_game_session_commitment_sha256 != session.session_commitment_sha256
        || prepared.source_sequence_commitment_sha256
            != confirmed.advanced_sequence_commitment_sha256
        || prepared.competitive_mode_authorization_commitment_sha256
            != session.mode_authorization_commitment_sha256
        || prepared.competitive_match_gameplay_authorization_commitment_sha256
            != session.match_gameplay_authorization_commitment_sha256
        || prepared.selected_action_family != confirmed.selected_action_family
        || prepared.event_kind != confirmed.event_kind
        || prepared.event_kind != session.event_kind
        || prepared.game_number != confirmed.game_number
        || prepared.game_number != session.game_number
        || prepared.stage_index != confirmed.next_stage_index
        || prepared.gesture_stage_count != confirmed.gesture_stage_count
        || prepared.target_count == 0
        || prepared.target_count > 2
        || prepared.fresh_frame_id == 0
        || prepared.fresh_frame_id == confirmed.after_frame_id
        || prepared.fresh_frame_sequence != expected_fresh_sequence
        || prepared.fresh_frame_sequence <= session.last_confirmed_frame_sequence
        || prepared.fresh_frame_sequence < session.valid_from_frame_sequence
        || prepared.fresh_frame_sequence > session.valid_through_frame_sequence
        || prepared.gameplay_authorization_valid_through_frame_sequence
            != session.valid_through_frame_sequence
        || prepared.fresh_captured_at_unix_millis <= confirmed.after_captured_at_unix_millis
        || prepared.gesture_target_runtime_identity_commitment_sha256
            != confirmed.gesture_target_runtime_identity_commitment_sha256
        || prepared.gesture_target_request_commitment_sha256
            == confirmed.next_stage_target_request_commitment_sha256
    {
        return Err(
            "fresh gesture continuation stage does not match its exact session and prior receipt"
                .to_owned(),
        );
    }
    let family_json = serde_json::to_vec(&prepared.selected_action_family)
        .map_err(|error| format!("serialize prepared continuation gesture family: {error}"))?;
    let event_kind_json = serde_json::to_vec(&prepared.event_kind)
        .map_err(|error| format!("serialize prepared continuation event kind: {error}"))?;
    let preparation_binding_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_GESTURE_SESSION_CONTINUATION_PREPARATION_DOMAIN_V1,
        &[
            confirmed.continuation_receipt_sha256.as_bytes(),
            confirmed.advanced_sequence_commitment_sha256.as_bytes(),
            session.session_commitment_sha256.as_bytes(),
            prepared.source_sequence_commitment_sha256.as_bytes(),
            prepared.prepared_sequence_commitment_sha256.as_bytes(),
            prepared
                .competitive_action_plan_commitment_sha256
                .as_bytes(),
            prepared.gesture_plan_commitment_sha256.as_bytes(),
            prepared.fresh_stage_binding_commitment_sha256.as_bytes(),
            prepared.fresh_capture_commitment_sha256.as_bytes(),
            prepared
                .fresh_perception_result_commitment_sha256
                .as_bytes(),
            prepared
                .gesture_target_runtime_identity_commitment_sha256
                .as_bytes(),
            prepared.gesture_target_request_commitment_sha256.as_bytes(),
            prepared
                .before_input_postcondition_verification_commitment_sha256
                .as_bytes(),
            prepared.primitive_commitment_sha256.as_bytes(),
            prepared.preparation_commitment_sha256.as_bytes(),
            &family_json,
            &event_kind_json,
            &[prepared.game_number],
            &prepared.stage_index.to_be_bytes(),
            &prepared.gesture_stage_count.to_be_bytes(),
            &prepared.target_count.to_be_bytes(),
            &confirmed.after_frame_id.to_be_bytes(),
            &confirmed.after_frame_sequence.to_be_bytes(),
            &prepared.fresh_frame_id.to_be_bytes(),
            &prepared.fresh_frame_sequence.to_be_bytes(),
            b"confirmed_transition_then_distinct_fresh_stage_recheck_no_input",
        ],
    );
    Ok(MtgoPreparedCompetitiveDuelGestureSourceStageCommitmentsV1 {
        preparation_binding_commitment_sha256,
        session_sequence_binding_commitment_sha256: confirmed.continuation_receipt_sha256.clone(),
        game_session_commitment_sha256: session.session_commitment_sha256.clone(),
        gesture_match_launch_commitment_sha256: session
            .gesture_match_launch_commitment_sha256
            .clone(),
        source_sequence_commitment_sha256: prepared.source_sequence_commitment_sha256.clone(),
        prepared_sequence_commitment_sha256: prepared.prepared_sequence_commitment_sha256.clone(),
        competitive_action_plan_commitment_sha256: prepared
            .competitive_action_plan_commitment_sha256
            .clone(),
        gesture_plan_commitment_sha256: prepared.gesture_plan_commitment_sha256.clone(),
        fresh_stage_binding_commitment_sha256: prepared
            .fresh_stage_binding_commitment_sha256
            .clone(),
        fresh_capture_commitment_sha256: prepared.fresh_capture_commitment_sha256.clone(),
        fresh_perception_result_commitment_sha256: prepared
            .fresh_perception_result_commitment_sha256
            .clone(),
        gesture_target_runtime_identity_commitment_sha256: prepared
            .gesture_target_runtime_identity_commitment_sha256
            .clone(),
        gesture_target_request_commitment_sha256: prepared
            .gesture_target_request_commitment_sha256
            .clone(),
        before_input_postcondition_verification_commitment_sha256: prepared
            .before_input_postcondition_verification_commitment_sha256
            .clone(),
        primitive_commitment_sha256: prepared.primitive_commitment_sha256.clone(),
        selected_action_family: prepared.selected_action_family,
        event_kind: prepared.event_kind,
        game_number: prepared.game_number,
        stage_index: prepared.stage_index,
        gesture_stage_count: prepared.gesture_stage_count,
        target_count: prepared.target_count,
        fresh_frame_id: prepared.fresh_frame_id,
        fresh_frame_sequence: prepared.fresh_frame_sequence,
        fresh_captured_at_unix_millis: prepared.fresh_captured_at_unix_millis,
    })
}

fn competitive_duel_gesture_input_receipt_v1(
    prepared: &MtgoPreparedCompetitiveDuelGestureSourceStageCommitmentsV1,
    primitive: &MtgoDuelGesturePrimitiveV1,
    input_sent_at_unix_millis: u128,
    emitted_mouse_record_count: u8,
    cursor_parked_outside_client: bool,
) -> Result<String, String> {
    if prepared.stage_index >= prepared.gesture_stage_count
        || input_sent_at_unix_millis == 0
        || !matches!(emitted_mouse_record_count, 2 | 4)
    {
        return Err("competitive gesture input receipt envelope is invalid".to_owned());
    }
    let primitive_json = serde_json::to_vec(primitive)
        .map_err(|error| format!("serialize emitted gesture primitive: {error}"))?;
    let family_json = serde_json::to_vec(&prepared.selected_action_family)
        .map_err(|error| format!("serialize emitted gesture family: {error}"))?;
    let event_kind_json = serde_json::to_vec(&prepared.event_kind)
        .map_err(|error| format!("serialize emitted gesture event kind: {error}"))?;
    Ok(hash_parts_v2(
        COMPETITIVE_DUEL_GESTURE_INPUT_RECEIPT_DOMAIN_V1,
        &[
            prepared.preparation_binding_commitment_sha256.as_bytes(),
            prepared.game_session_commitment_sha256.as_bytes(),
            prepared.gesture_match_launch_commitment_sha256.as_bytes(),
            prepared
                .gesture_target_runtime_identity_commitment_sha256
                .as_bytes(),
            prepared.gesture_target_request_commitment_sha256.as_bytes(),
            prepared
                .before_input_postcondition_verification_commitment_sha256
                .as_bytes(),
            prepared.primitive_commitment_sha256.as_bytes(),
            &primitive_json,
            &family_json,
            &event_kind_json,
            &[prepared.game_number],
            prepared.stage_index.to_be_bytes().as_slice(),
            prepared.gesture_stage_count.to_be_bytes().as_slice(),
            prepared.fresh_frame_id.to_be_bytes().as_slice(),
            prepared.fresh_frame_sequence.to_be_bytes().as_slice(),
            input_sent_at_unix_millis.to_be_bytes().as_slice(),
            &[emitted_mouse_record_count],
            &[u8::from(cursor_parked_outside_client)],
            b"one_declared_primitive_emitted_pending_exact_visible_transition",
        ],
    ))
}

fn competitive_player_visible_gameplay_authority_binding_v1(
    before: &OpaqueMtgoPreparedPlayerVisibleGameplayBeforeInputV1,
    lease: &OpaqueMtgoCompetitiveEventGameplayLeaseV1,
    session: &OpaqueMtgoCompetitiveGestureGameSessionV1,
) -> Result<String, String> {
    validate_game_session_against_event_runtime_v1(&lease.runtime, session)?;
    let lease_commitments = lease.commitments_v1();
    let session_commitments = session.commitments_v1();
    let (scope, gameplay) = competitive_gesture_game_session_action_authorities_v1(session);
    let mode_commitment =
        competitive_mode_authorization_commitment_v1(&scope, lease_commitments.event_kind)
            .map_err(|error| {
                format!("player-visible gameplay mode authorization rejected: {error}")
            })?;
    let gameplay_commitment = competitive_match_gameplay_authorization_commitment_v1(&gameplay)
        .map_err(|error| format!("player-visible exact-game authorization rejected: {error}"))?;
    let context = &before.authority_context;
    let prior_chain_shape_valid = player_visible_confirmed_primitive_chain_matches_v1(
        context.confirmed_prior_primitive_count,
        before.pointer.commitments.primitive_index,
        context.prior_primitive_confirmation_chain_sha256.as_deref(),
    );
    if !prior_chain_shape_valid
        || !scope.visible_channels_only
        || !gameplay.exact_match_gameplay_authorized
        || lease_commitments.event_kind != context.event_kind
        || lease_commitments.game_number != context.game_number
        || lease_commitments.event_identity_sha256 != context.event_identity_sha256
        || lease_commitments.match_identity_sha256 != context.match_identity_sha256
        || lease_commitments.policy_deployment_commitment_sha256
            != context.deployment_commitment_sha256
        || session_commitments.event_kind != context.event_kind
        || session_commitments.game_number != context.game_number
        || session_commitments.mode_authorization_commitment_sha256 != mode_commitment
        || session_commitments.match_gameplay_authorization_commitment_sha256 != gameplay_commitment
        || gameplay.event_kind != context.event_kind
        || gameplay.event_identity_sha256 != context.event_identity_sha256
        || gameplay.match_identity_sha256 != context.match_identity_sha256
        || gameplay.game_number != context.game_number
        || before.pointer.commitments.frame_sequence
            <= session_commitments.last_confirmed_frame_sequence
        || before.pointer.commitments.frame_sequence < session_commitments.valid_from_frame_sequence
        || before.pointer.commitments.frame_sequence
            > session_commitments.valid_through_frame_sequence
    {
        return Err(
            "player-visible gameplay preparation differs from its exact visible-only event authority"
                .to_owned(),
        );
    }
    let family_json = serde_json::to_vec(&before.checked.action_family_v1())
        .map_err(|error| format!("serialize player-visible authorized action family: {error}"))?;
    Ok(hash_parts_v2(
        COMPETITIVE_PLAYER_VISIBLE_GAMEPLAY_AUTHORITY_BINDING_DOMAIN_V1,
        &[
            before.before_input_commitment_sha256_v1().as_bytes(),
            before
                .pointer
                .commitments
                .preparation_commitment_sha256
                .as_bytes(),
            lease_commitments
                .gameplay_lease_commitment_sha256
                .as_bytes(),
            session_commitments.session_commitment_sha256.as_bytes(),
            mode_commitment.as_bytes(),
            gameplay_commitment.as_bytes(),
            context.event_identity_sha256.as_bytes(),
            context.match_identity_sha256.as_bytes(),
            context.deployment_commitment_sha256.as_bytes(),
            context
                .prior_primitive_confirmation_chain_sha256
                .as_deref()
                .unwrap_or("none")
                .as_bytes(),
            &family_json,
            competitive_event_kind_tag_v1(context.event_kind),
            &[context.game_number],
            context
                .confirmed_prior_primitive_count
                .to_be_bytes()
                .as_slice(),
            before.pointer.commitments.frame_id.to_be_bytes().as_slice(),
            before
                .pointer
                .commitments
                .frame_sequence
                .to_be_bytes()
                .as_slice(),
            b"exact_visible_only_player_visible_action_authority_no_entry_or_spending",
        ],
    ))
}

fn player_visible_confirmed_primitive_chain_matches_v1(
    confirmed_prior_primitive_count: u16,
    primitive_index: u16,
    prior_primitive_confirmation_chain_sha256: Option<&str>,
) -> bool {
    confirmed_prior_primitive_count == primitive_index
        && match (
            confirmed_prior_primitive_count,
            prior_primitive_confirmation_chain_sha256,
        ) {
            (0, None) => true,
            (count, Some(commitment)) if count > 0 => is_sha256_v2(commitment),
            _ => false,
        }
}

fn competitive_duel_gesture_transition_receipt_v1(
    pending: &MtgoPendingCompetitiveDuelGesturePrimitiveCommitmentsV1,
    prepared: &MtgoPreparedCompetitiveDuelGestureSourceStageCommitmentsV1,
    visible: &MtgoOpaqueCompetitiveDuelGestureConfirmationCommitmentsV1,
) -> Result<String, String> {
    if pending.preparation_binding_commitment_sha256
        != prepared.preparation_binding_commitment_sha256
        || pending.gesture_target_runtime_identity_commitment_sha256
            != prepared.gesture_target_runtime_identity_commitment_sha256
        || pending.gesture_target_request_commitment_sha256
            != prepared.gesture_target_request_commitment_sha256
        || pending.before_input_postcondition_verification_commitment_sha256
            != prepared.before_input_postcondition_verification_commitment_sha256
        || pending.before_input_postcondition_verification_commitment_sha256
            != visible.before_input_verification_commitment_sha256
        || pending.selected_action_family != prepared.selected_action_family
        || pending.selected_action_family != visible.selected_action_family
        || pending.event_kind != prepared.event_kind
        || pending.event_kind != visible.event_kind
        || pending.game_number != prepared.game_number
        || pending.game_number != visible.game_number
        || pending.stage_index != prepared.stage_index
        || pending.gesture_stage_count != prepared.gesture_stage_count
        || pending.stage_index.checked_add(1) != Some(pending.gesture_stage_count)
        || pending.before_frame_id != prepared.fresh_frame_id
        || pending.before_frame_sequence != prepared.fresh_frame_sequence
        || visible.after_frame_sequence <= pending.before_frame_sequence
    {
        return Err("gesture transition does not match its exact pending input".to_owned());
    }
    let family_json = serde_json::to_vec(&pending.selected_action_family)
        .map_err(|error| format!("serialize confirmed gesture family: {error}"))?;
    let event_kind_json = serde_json::to_vec(&pending.event_kind)
        .map_err(|error| format!("serialize confirmed gesture mode: {error}"))?;
    Ok(hash_parts_v2(
        COMPETITIVE_DUEL_GESTURE_TRANSITION_RECEIPT_DOMAIN_V1,
        &[
            pending.input_receipt_sha256.as_bytes(),
            prepared.preparation_binding_commitment_sha256.as_bytes(),
            visible.opaque_confirmation_commitment_sha256.as_bytes(),
            visible.checked_postcondition_commitment_sha256.as_bytes(),
            visible.after_capture_commitment_sha256.as_bytes(),
            &family_json,
            &event_kind_json,
            &[pending.game_number],
            pending.stage_index.to_be_bytes().as_slice(),
            pending.gesture_stage_count.to_be_bytes().as_slice(),
            visible.after_frame_id.to_be_bytes().as_slice(),
            visible.after_frame_sequence.to_be_bytes().as_slice(),
            visible
                .postcondition_candidate_count
                .to_be_bytes()
                .as_slice(),
            b"final_gesture_primitive_bound_to_exact_newer_visible_postcondition",
        ],
    ))
}

fn competitive_duel_gesture_continuation_receipt_v1(
    pending: &MtgoPendingCompetitiveDuelGesturePrimitiveCommitmentsV1,
    prepared: &MtgoPreparedCompetitiveDuelGestureSourceStageCommitmentsV1,
    continuation: &MtgoOpaquePinnedCompetitiveDuelGestureContinuationCommitmentsV1,
) -> Result<String, String> {
    let expected_next_stage = pending
        .stage_index
        .checked_add(1)
        .ok_or("gesture continuation stage index overflow")?;
    if expected_next_stage >= pending.gesture_stage_count
        || pending.preparation_binding_commitment_sha256
            != prepared.preparation_binding_commitment_sha256
        || pending.selected_action_family != prepared.selected_action_family
        || pending.selected_action_family != continuation.selected_action_family
        || pending.event_kind != prepared.event_kind
        || pending.event_kind != continuation.event_kind
        || pending.game_number != prepared.game_number
        || pending.game_number != continuation.game_number
        || pending.stage_index != prepared.stage_index
        || pending.gesture_stage_count != prepared.gesture_stage_count
        || pending.gesture_stage_count != continuation.gesture_stage_count
        || prepared.prepared_sequence_commitment_sha256
            != continuation.prior_sequence_commitment_sha256
        || pending.gesture_target_runtime_identity_commitment_sha256
            != prepared.gesture_target_runtime_identity_commitment_sha256
        || pending.gesture_target_runtime_identity_commitment_sha256
            != continuation.gesture_target_runtime_identity_commitment_sha256
        || pending.before_frame_id != prepared.fresh_frame_id
        || pending.before_frame_sequence != prepared.fresh_frame_sequence
        || continuation.stage_index != expected_next_stage
        || continuation.frame_sequence <= pending.before_frame_sequence
        || continuation.frame_id == pending.before_frame_id
        || continuation.captured_at_unix_millis <= pending.input_sent_at_unix_millis
        || continuation.target_count == 0
        || continuation.target_count > 2
    {
        return Err("gesture continuation does not match its exact pending primitive".to_owned());
    }
    for commitment in [
        pending.input_receipt_sha256.as_str(),
        prepared.preparation_binding_commitment_sha256.as_str(),
        prepared.prepared_sequence_commitment_sha256.as_str(),
        continuation.prior_sequence_commitment_sha256.as_str(),
        continuation.advanced_sequence_commitment_sha256.as_str(),
        continuation.visible_transition_commitment_sha256.as_str(),
        continuation
            .gesture_target_runtime_identity_commitment_sha256
            .as_str(),
        continuation
            .gesture_target_request_commitment_sha256
            .as_str(),
        continuation.continuation_commitment_sha256.as_str(),
    ] {
        if !is_sha256_v2(commitment) {
            return Err("gesture continuation contains an invalid commitment".to_owned());
        }
    }
    let family_json = serde_json::to_vec(&pending.selected_action_family)
        .map_err(|error| format!("serialize continuation gesture family: {error}"))?;
    let event_kind_json = serde_json::to_vec(&pending.event_kind)
        .map_err(|error| format!("serialize continuation gesture mode: {error}"))?;
    Ok(hash_parts_v2(
        COMPETITIVE_DUEL_GESTURE_CONTINUATION_RECEIPT_DOMAIN_V1,
        &[
            pending.input_receipt_sha256.as_bytes(),
            prepared.preparation_binding_commitment_sha256.as_bytes(),
            prepared.prepared_sequence_commitment_sha256.as_bytes(),
            continuation.prior_sequence_commitment_sha256.as_bytes(),
            continuation.advanced_sequence_commitment_sha256.as_bytes(),
            continuation.visible_transition_commitment_sha256.as_bytes(),
            continuation.continuation_commitment_sha256.as_bytes(),
            continuation
                .gesture_target_runtime_identity_commitment_sha256
                .as_bytes(),
            continuation
                .gesture_target_request_commitment_sha256
                .as_bytes(),
            &family_json,
            &event_kind_json,
            &[pending.game_number],
            &pending.stage_index.to_be_bytes(),
            &continuation.stage_index.to_be_bytes(),
            &pending.gesture_stage_count.to_be_bytes(),
            &pending.before_frame_id.to_be_bytes(),
            &pending.before_frame_sequence.to_be_bytes(),
            &continuation.frame_id.to_be_bytes(),
            &continuation.frame_sequence.to_be_bytes(),
            &pending.input_sent_at_unix_millis.to_be_bytes(),
            &continuation.captured_at_unix_millis.to_be_bytes(),
            &continuation.target_count.to_be_bytes(),
            b"pending_primitive_joined_to_exact_newer_runtime_pinned_visible_stage_no_next_input",
        ],
    ))
}

fn advance_competitive_gesture_game_session_v1(
    mut session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    visible: &MtgoOpaqueCompetitiveDuelGestureConfirmationCommitmentsV1,
    transition_receipt_sha256: &str,
) -> Result<OpaqueMtgoCompetitiveGestureGameSessionV1, String> {
    if visible.event_kind != session.launch.pass_match_launch.authorization.event_kind
        || visible.game_number != session.launch.pass_match_launch.authorization.game_number
        || !canonical_duel_gesture_action_families_v1().contains(&visible.selected_action_family)
        || visible.after_frame_sequence <= session.last_confirmed_frame_sequence
        || visible.after_frame_sequence
            > session
                .launch
                .pass_match_launch
                .authorization
                .valid_through_frame_sequence
        || !is_sha256_v2(transition_receipt_sha256)
    {
        return Err(
            "confirmed gesture transition is outside the competitive game session lifetime"
                .to_owned(),
        );
    }
    let next_count = session
        .confirmed_action_count
        .checked_add(1)
        .ok_or("competitive gesture game session action count overflow")?;
    let family_json = serde_json::to_vec(&visible.selected_action_family)
        .map_err(|error| format!("serialize advanced gesture family: {error}"))?;
    session.session_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_GESTURE_GAME_SESSION_ADVANCE_DOMAIN_V1,
        &[
            session.session_commitment_sha256.as_bytes(),
            transition_receipt_sha256.as_bytes(),
            visible.opaque_confirmation_commitment_sha256.as_bytes(),
            &family_json,
            visible.after_frame_id.to_be_bytes().as_slice(),
            visible.after_frame_sequence.to_be_bytes().as_slice(),
            next_count.to_be_bytes().as_slice(),
            b"all_family_session_returned_only_after_newer_visible_postcondition",
        ],
    );
    session.last_confirmed_frame_sequence = visible.after_frame_sequence;
    session.confirmed_action_count = next_count;
    Ok(session)
}

pub(crate) fn advance_competitive_player_visible_gameplay_session_v1(
    mut session: OpaqueMtgoCompetitiveGestureGameSessionV1,
    visible: &OpaqueMtgoPlayerVisibleGameplayAfterInputV1,
) -> Result<OpaqueMtgoCompetitiveGestureGameSessionV1, String> {
    if !visible.is_final_primitive_v1() {
        return Err(
            "a non-final player-visible primitive cannot advance the game session".to_owned(),
        );
    }
    let checked = &visible.checked;
    let context = &visible.authority_context;
    if checked.event_kind_v1() != session.launch.pass_match_launch.authorization.event_kind
        || checked.game_number_v1() != session.launch.pass_match_launch.authorization.game_number
        || checked.event_kind_v1() != context.event_kind
        || checked.game_number_v1() != context.game_number
        || checked.after_frame_sequence_v1() <= session.last_confirmed_frame_sequence
        || checked.after_frame_sequence_v1()
            > session
                .launch
                .pass_match_launch
                .authorization
                .valid_through_frame_sequence
        || !is_sha256_v2(visible.confirmation_commitment_sha256_v1())
        || !is_sha256_v2(visible.input_receipt_commitment_sha256_v1())
        || !is_sha256_v2(&visible.actuator_authority_binding_sha256)
    {
        return Err(
            "confirmed player-visible transition is outside the competitive game session lifetime"
                .to_owned(),
        );
    }
    let next_count = session
        .confirmed_action_count
        .checked_add(1)
        .ok_or("competitive player-visible game session action count overflow")?;
    let family_json = serde_json::to_vec(&checked.action_family_v1())
        .map_err(|error| format!("serialize player-visible gameplay family: {error}"))?;
    session.session_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_PLAYER_VISIBLE_GAMEPLAY_SESSION_ADVANCE_DOMAIN_V1,
        &[
            session.session_commitment_sha256.as_bytes(),
            visible.input_receipt_commitment_sha256_v1().as_bytes(),
            visible.confirmation_commitment_sha256_v1().as_bytes(),
            visible.actuator_authority_binding_sha256.as_bytes(),
            &family_json,
            checked.after_frame_id_v1().to_be_bytes().as_slice(),
            checked.after_frame_sequence_v1().to_be_bytes().as_slice(),
            next_count.to_be_bytes().as_slice(),
            b"visible_only_session_returned_after_exact_newer_final_postcondition",
        ],
    );
    session.last_confirmed_frame_sequence = checked.after_frame_sequence_v1();
    session.confirmed_action_count = next_count;
    Ok(session)
}

fn advance_competitive_game_session_v1(
    mut session: OpaqueMtgoCompetitiveGameSessionV1,
    visible: &MtgoOpaqueCompetitiveDuelPassConfirmationCommitmentsV1,
    transition_receipt_sha256: &str,
) -> Result<OpaqueMtgoCompetitiveGameSessionV1, String> {
    if visible.event_kind != session.match_launch.authorization.event_kind
        || visible.game_number != session.match_launch.authorization.game_number
        || visible.after_frame_sequence <= session.last_confirmed_frame_sequence
        || visible.after_frame_sequence
            > session
                .match_launch
                .authorization
                .valid_through_frame_sequence
        || !is_sha256_v2(transition_receipt_sha256)
    {
        return Err(
            "confirmed transition is outside the competitive game session lifetime".to_owned(),
        );
    }
    let next_count = session
        .confirmed_action_count
        .checked_add(1)
        .ok_or("competitive game session action count overflow")?;
    session.session_commitment_sha256 = hash_parts_v2(
        COMPETITIVE_GAME_SESSION_ADVANCE_DOMAIN_V1,
        &[
            session.session_commitment_sha256.as_bytes(),
            transition_receipt_sha256.as_bytes(),
            visible.opaque_confirmation_commitment_sha256.as_bytes(),
            visible.after_frame_id.to_be_bytes().as_slice(),
            visible.after_frame_sequence.to_be_bytes().as_slice(),
            next_count.to_be_bytes().as_slice(),
            b"returned_only_after_newer_visible_postcondition",
        ],
    );
    session.last_confirmed_frame_sequence = visible.after_frame_sequence;
    session.confirmed_action_count = next_count;
    Ok(session)
}

fn competitive_duel_pass_authorization_binding_commitments_v1(
    prepared: &MtgoOpaqueCompetitiveDuelPassPreparationCommitmentsV1,
    session: &OpaqueMtgoCompetitiveGameSessionV1,
) -> Result<MtgoAuthorizationBoundCompetitiveDuelPassCommitmentsV1, String> {
    let authorization = &session.authorization;
    let match_launch = &session.match_launch;
    if prepared.event_kind != authorization.event_kind
        || prepared.competitive_mode_authorization_commitment_sha256
            != authorization.mode_authorization_commitment_sha256
        || prepared.event_kind != match_launch.authorization.event_kind
        || prepared.game_number != match_launch.authorization.game_number
        || prepared.competitive_mode_authorization_commitment_sha256
            != match_launch.mode_authorization_commitment_sha256
        || prepared.competitive_match_gameplay_authorization_commitment_sha256
            != match_launch.gameplay_authorization_commitment_sha256
        || prepared.immediate_frame_sequence < match_launch.valid_from_frame_sequence
        || prepared.immediate_frame_sequence
            > match_launch.authorization.valid_through_frame_sequence
        || prepared.immediate_frame_sequence <= session.last_confirmed_frame_sequence
    {
        return Err(
            "the prepared Pass does not match the ratified competitive mode authority".to_owned(),
        );
    }
    let event_kind_bytes: &[u8] = match prepared.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let mut hasher = Sha256::new();
    hasher.update(COMPETITIVE_DUEL_PASS_AUTHORIZATION_BINDING_DOMAIN_V1);
    for part in [
        prepared.preparation_commitment_sha256.as_bytes(),
        prepared
            .competitive_mode_authorization_commitment_sha256
            .as_bytes(),
        prepared
            .before_input_postcondition_verification_commitment_sha256
            .as_bytes(),
        authorization.authorization_commitment_sha256.as_bytes(),
        match_launch
            .launch_authorization_commitment_sha256
            .as_bytes(),
        session.session_commitment_sha256.as_bytes(),
        session
            .last_confirmed_frame_sequence
            .to_be_bytes()
            .as_slice(),
        session.confirmed_action_count.to_be_bytes().as_slice(),
        prepared
            .competitive_match_gameplay_authorization_commitment_sha256
            .as_bytes(),
        match_launch
            .valid_from_frame_sequence
            .to_be_bytes()
            .as_slice(),
        match_launch
            .authorization
            .valid_through_frame_sequence
            .to_be_bytes()
            .as_slice(),
        event_kind_bytes,
        &[prepared.game_number],
        prepared.immediate_frame_id.to_be_bytes().as_slice(),
        prepared.immediate_frame_sequence.to_be_bytes().as_slice(),
        b"authorization_bound_no_input_or_event_entry_authority",
    ] {
        update_hash_part_v3(&mut hasher, part);
    }
    Ok(MtgoAuthorizationBoundCompetitiveDuelPassCommitmentsV1 {
        preparation_commitment_sha256: prepared.preparation_commitment_sha256.clone(),
        mode_authorization_commitment_sha256: prepared
            .competitive_mode_authorization_commitment_sha256
            .clone(),
        before_input_postcondition_verification_commitment_sha256: prepared
            .before_input_postcondition_verification_commitment_sha256
            .clone(),
        ratified_authorization_commitment_sha256: authorization
            .authorization_commitment_sha256
            .clone(),
        ratified_match_launch_commitment_sha256: match_launch
            .launch_authorization_commitment_sha256
            .clone(),
        competitive_match_gameplay_authorization_commitment_sha256: prepared
            .competitive_match_gameplay_authorization_commitment_sha256
            .clone(),
        competitive_game_session_commitment_sha256: session.session_commitment_sha256.clone(),
        authorization_binding_commitment_sha256: format!("{:x}", hasher.finalize()),
        event_kind: prepared.event_kind,
        game_number: prepared.game_number,
        immediate_frame_id: prepared.immediate_frame_id,
        immediate_frame_sequence: prepared.immediate_frame_sequence,
    })
}

fn competitive_duel_pass_input_receipt_v1(
    prepared: &OpaqueMtgoPreparedCompetitiveDuelPassV1,
    authorization_binding: &MtgoAuthorizationBoundCompetitiveDuelPassCommitmentsV1,
    input_sent_at_unix_millis: u128,
    cursor_parked_outside_client: bool,
) -> String {
    let prepared_commitments = prepared.commitments_v1();
    let mut hasher = Sha256::new();
    hasher.update(COMPETITIVE_DUEL_PASS_INPUT_RECEIPT_DOMAIN_V1);
    for part in [
        authorization_binding
            .authorization_binding_commitment_sha256
            .as_bytes(),
        prepared_commitments
            .preparation_commitment_sha256
            .as_bytes(),
        prepared_commitments
            .immediate_capture_commitment_sha256
            .as_bytes(),
        prepared_commitments
            .before_input_postcondition_verification_commitment_sha256
            .as_bytes(),
        prepared.target_x_desktop_px.to_be_bytes().as_slice(),
        prepared.target_y_desktop_px.to_be_bytes().as_slice(),
        prepared.park_x_desktop_px.to_be_bytes().as_slice(),
        prepared.park_y_desktop_px.to_be_bytes().as_slice(),
        input_sent_at_unix_millis.to_be_bytes().as_slice(),
        &[u8::from(cursor_parked_outside_client)],
        b"exactly_one_priority_pass_left_click",
    ] {
        update_hash_part_v3(&mut hasher, part);
    }
    format!("{:x}", hasher.finalize())
}

fn competitive_entry_input_receipt_v1(
    prepared: &OpaqueMtgoPreparedCompetitiveEntryV1,
    input_sent_at_unix_millis: u128,
    cursor_parked_outside_client: bool,
) -> String {
    let prepared_commitments = prepared.commitments_v1();
    competitive_entry_input_receipt_from_parts_v1(
        &prepared_commitments,
        &prepared.pointer_target,
        input_sent_at_unix_millis,
        cursor_parked_outside_client,
    )
}

fn competitive_entry_input_receipt_from_parts_v1(
    prepared: &MtgoPreparedCompetitiveEntryCommitmentsV1,
    target: &MtgoCompetitiveEntryPointerTargetV1,
    input_sent_at_unix_millis: u128,
    cursor_parked_outside_client: bool,
) -> String {
    hash_parts_v2(
        COMPETITIVE_ENTRY_INPUT_RECEIPT_DOMAIN_V1,
        &[
            prepared.preparation_commitment_sha256.as_bytes(),
            prepared
                .entry_ratification_commitment_sha256
                .as_bytes(),
            prepared
                .immediate_recapture
                .recapture_commitment_sha256
                .as_bytes(),
            prepared
                .immediate_recapture
                .visible_control_region_sha256
                .as_bytes(),
            target.hwnd.to_be_bytes().as_slice(),
            target.process_id.to_be_bytes().as_slice(),
            target
                .process_start_filetime_100ns
                .to_be_bytes()
                .as_slice(),
            target.target_x_desktop_px.to_be_bytes().as_slice(),
            target.target_y_desktop_px.to_be_bytes().as_slice(),
            target.park_x_desktop_px.to_be_bytes().as_slice(),
            target.park_y_desktop_px.to_be_bytes().as_slice(),
            input_sent_at_unix_millis.to_be_bytes().as_slice(),
            &[u8::from(cursor_parked_outside_client)],
            b"exactly_one_exactly_ratified_competitive_entry_left_click_pending_visible_confirmation",
        ],
    )
}

fn competitive_entry_confirmation_receipt_v1(
    input: &MtgoCompetitiveEntryInputReceiptCommitmentsV1,
    prepared: &MtgoPreparedCompetitiveEntryCommitmentsV1,
    visible: &MtgoCompetitiveEntryVisibleConfirmationCommitmentsV1,
) -> String {
    let event_kind: &[u8] = match prepared.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let resource: &[u8] = match prepared.resource {
        MtgoCompetitiveEntryResourceV1::NoCost => b"no_cost",
        MtgoCompetitiveEntryResourceV1::ExistingEventToken => b"existing_event_token",
        MtgoCompetitiveEntryResourceV1::ExistingPlayPoints => b"existing_play_points",
        MtgoCompetitiveEntryResourceV1::ExistingEventTickets => b"existing_event_tickets",
    };
    hash_parts_v2(
        COMPETITIVE_ENTRY_CONFIRMATION_RECEIPT_DOMAIN_V1,
        &[
            input.input_receipt_sha256.as_bytes(),
            prepared.preparation_commitment_sha256.as_bytes(),
            prepared.entry_ratification_commitment_sha256.as_bytes(),
            visible
                .frame_transition
                .transition_commitment_sha256
                .as_bytes(),
            visible.confirmation_commitment_sha256.as_bytes(),
            visible.after_capture_commitment_sha256.as_bytes(),
            visible
                .after_classification_result_commitment_sha256
                .as_bytes(),
            visible
                .after_lifecycle_snapshot_commitment_sha256
                .as_bytes(),
            event_kind,
            prepared
                .immediate_recapture
                .event_identity_sha256
                .as_bytes(),
            prepared.immediate_recapture.entry_terms_sha256.as_bytes(),
            resource,
            prepared.amount.to_be_bytes().as_slice(),
            input.input_sent_at_unix_millis.to_be_bytes().as_slice(),
            visible
                .after_captured_at_unix_millis
                .to_be_bytes()
                .as_slice(),
            visible
                .frame_transition
                .after_frame_id
                .to_be_bytes()
                .as_slice(),
            visible
                .frame_transition
                .after_frame_sequence
                .to_be_bytes()
                .as_slice(),
            visible
                .postcondition_candidate_count
                .to_be_bytes()
                .as_slice(),
            b"one_entry_input_visible_entered_waiting_confirmed_shared_gate_released",
        ],
    )
}

fn competitive_duel_pass_transition_receipt_v1(
    input_receipt_sha256: &str,
    authorization_binding_commitment_sha256: &str,
    visible: &MtgoOpaqueCompetitiveDuelPassConfirmationCommitmentsV1,
) -> String {
    let event_kind_bytes: &[u8] = match visible.event_kind {
        MtgoCompetitiveEventKindV1::League => b"league",
        MtgoCompetitiveEventKindV1::Challenge => b"challenge",
    };
    let mut hasher = Sha256::new();
    hasher.update(COMPETITIVE_DUEL_PASS_TRANSITION_RECEIPT_DOMAIN_V1);
    for part in [
        input_receipt_sha256.as_bytes(),
        authorization_binding_commitment_sha256.as_bytes(),
        visible
            .before_input_verification_commitment_sha256
            .as_bytes(),
        visible.after_capture_commitment_sha256.as_bytes(),
        visible.checked_postcondition_commitment_sha256.as_bytes(),
        visible.opaque_confirmation_commitment_sha256.as_bytes(),
        event_kind_bytes,
        &[visible.game_number],
        visible.after_frame_id.to_be_bytes().as_slice(),
        visible.after_frame_sequence.to_be_bytes().as_slice(),
        visible
            .postcondition_candidate_count
            .to_be_bytes()
            .as_slice(),
        b"visible_postcondition_confirmed_shared_input_gate_released",
    ] {
        update_hash_part_v3(&mut hasher, part);
    }
    format!("{:x}", hasher.finalize())
}

fn private_match_authorization_commitment_v3(
    authorization: &MtgoAuthorizationScopeV1,
    visible_account_alias: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(PRIVATE_MATCH_AUTHORIZATION_DOMAIN_V3);
    let schema_version = authorization.schema_version.to_be_bytes();
    for part in [
        schema_version.as_slice(),
        authorization.account_alias_sha256.as_bytes(),
        authorization.written_permission_sha256.as_bytes(),
        visible_account_alias.as_bytes(),
    ] {
        update_hash_part_v3(&mut hasher, part);
    }
    for flag in [
        authorization.visible_channels_only,
        authorization.shadow_observation,
        authorization.private_match_input,
        authorization.open_play_input,
        authorization.league_input,
        authorization.challenge_input,
        authorization.other_prize_event_input,
    ] {
        update_hash_part_v3(&mut hasher, &[u8::from(flag)]);
    }
    format!("{:x}", hasher.finalize())
}

fn input_gate_v3() -> &'static Mutex<PregameInputGateStateV3> {
    PREGAME_INPUT_GATE_V3.get_or_init(|| Mutex::new(PregameInputGateStateV3::Idle))
}

fn validate_preinput_capture_freshness_v3(
    captured_at_unix_millis: u128,
    input_at_unix_millis: u128,
) -> Result<(), String> {
    if input_at_unix_millis < captured_at_unix_millis
        || input_at_unix_millis - captured_at_unix_millis > 2_000
    {
        return Err("the immediate pre-input capture is stale or from the future".to_owned());
    }
    Ok(())
}

fn reserve_input_gate_v3() -> Result<(), String> {
    let mut gate = input_gate_v3()
        .lock()
        .map_err(|_| "the process-wide input gate is poisoned".to_owned())?;
    match &*gate {
        PregameInputGateStateV3::Idle => {
            *gate = PregameInputGateStateV3::Preparing;
            Ok(())
        }
        PregameInputGateStateV3::Preparing => {
            Err("another MTGO input is already being prepared".to_owned())
        }
        PregameInputGateStateV3::AwaitingVisiblePostcondition { .. } => {
            Err("a visible postcondition is still pending".to_owned())
        }
        PregameInputGateStateV3::Halted => Err("the process-wide input gate is halted".to_owned()),
    }
}

fn release_unattempted_reservation_v3() -> Result<(), String> {
    let mut gate = input_gate_v3()
        .lock()
        .map_err(|_| "the process-wide input gate is poisoned".to_owned())?;
    if !matches!(*gate, PregameInputGateStateV3::Preparing) {
        *gate = PregameInputGateStateV3::Halted;
        return Err("the input reservation changed unexpectedly".to_owned());
    }
    *gate = PregameInputGateStateV3::Idle;
    Ok(())
}

fn halt_before_input_attempt_v3() -> Result<(), String> {
    let mut gate = input_gate_v3()
        .lock()
        .map_err(|_| "the process-wide input gate is poisoned".to_owned())?;
    if !matches!(*gate, PregameInputGateStateV3::Preparing) {
        *gate = PregameInputGateStateV3::Halted;
        return Err("the input reservation changed unexpectedly".to_owned());
    }
    *gate = PregameInputGateStateV3::Halted;
    Ok(())
}

fn set_pending_v3(receipt_sha256: &str) -> Result<(), String> {
    let mut gate = input_gate_v3()
        .lock()
        .map_err(|_| "the process-wide input gate is poisoned".to_owned())?;
    if !matches!(*gate, PregameInputGateStateV3::Halted) {
        *gate = PregameInputGateStateV3::Halted;
        return Err("the process-wide input gate was not armed for an input attempt".to_owned());
    }
    *gate = PregameInputGateStateV3::AwaitingVisiblePostcondition {
        receipt_sha256: receipt_sha256.to_owned(),
    };
    Ok(())
}

fn require_matching_pending_v3(receipt_sha256: &str) -> Result<(), String> {
    let gate = input_gate_v3()
        .lock()
        .map_err(|_| "the process-wide input gate is poisoned".to_owned())?;
    match &*gate {
        PregameInputGateStateV3::AwaitingVisiblePostcondition {
            receipt_sha256: expected,
        } if expected == receipt_sha256 => Ok(()),
        PregameInputGateStateV3::AwaitingVisiblePostcondition { .. } => {
            Err("the pending input receipt does not match the process gate".to_owned())
        }
        _ => Err("the process gate is not awaiting this postcondition".to_owned()),
    }
}

fn release_confirmed_pending_v3(receipt_sha256: &str) -> Result<(), String> {
    let mut gate = input_gate_v3()
        .lock()
        .map_err(|_| "the process-wide input gate is poisoned".to_owned())?;
    match &*gate {
        PregameInputGateStateV3::AwaitingVisiblePostcondition {
            receipt_sha256: expected,
        } if expected == receipt_sha256 => {
            *gate = PregameInputGateStateV3::Idle;
            Ok(())
        }
        _ => {
            *gate = PregameInputGateStateV3::Halted;
            Err("the confirmed input receipt does not match the process gate".to_owned())
        }
    }
}

fn halt_gate_v3() -> Result<(), String> {
    let mut gate = input_gate_v3()
        .lock()
        .map_err(|_| "the process-wide input gate is poisoned".to_owned())?;
    *gate = PregameInputGateStateV3::Halted;
    Ok(())
}

trait VerifiedPointerTargetV3 {
    fn hwnd_v3(&self) -> u64;
    fn process_id_v3(&self) -> u32;
    fn process_start_filetime_100ns_v3(&self) -> u64;
    fn dpi_v3(&self) -> u32;
    fn client_rect_desktop_px_v3(&self) -> &crate::SignedRectV1;
    fn target_x_desktop_px_v3(&self) -> i32;
    fn target_y_desktop_px_v3(&self) -> i32;
    fn park_x_desktop_px_v3(&self) -> i32;
    fn park_y_desktop_px_v3(&self) -> i32;
}

impl VerifiedPointerTargetV3 for PreparedPregameActuationV3 {
    fn hwnd_v3(&self) -> u64 {
        self.hwnd
    }

    fn process_id_v3(&self) -> u32 {
        self.process_id
    }

    fn process_start_filetime_100ns_v3(&self) -> u64 {
        self.process_start_filetime_100ns
    }

    fn dpi_v3(&self) -> u32 {
        self.dpi
    }

    fn client_rect_desktop_px_v3(&self) -> &crate::SignedRectV1 {
        &self.client_rect_desktop_px
    }

    fn target_x_desktop_px_v3(&self) -> i32 {
        self.target_x_desktop_px
    }

    fn target_y_desktop_px_v3(&self) -> i32 {
        self.target_y_desktop_px
    }

    fn park_x_desktop_px_v3(&self) -> i32 {
        self.park_x_desktop_px
    }

    fn park_y_desktop_px_v3(&self) -> i32 {
        self.park_y_desktop_px
    }
}

impl VerifiedPointerTargetV3 for MtgoCompetitivePregamePointerTargetV1 {
    fn hwnd_v3(&self) -> u64 {
        self.hwnd
    }

    fn process_id_v3(&self) -> u32 {
        self.process_id
    }

    fn process_start_filetime_100ns_v3(&self) -> u64 {
        self.process_start_filetime_100ns
    }

    fn dpi_v3(&self) -> u32 {
        self.dpi
    }

    fn client_rect_desktop_px_v3(&self) -> &crate::SignedRectV1 {
        &self.client_rect_desktop_px
    }

    fn target_x_desktop_px_v3(&self) -> i32 {
        self.target_x_desktop_px
    }

    fn target_y_desktop_px_v3(&self) -> i32 {
        self.target_y_desktop_px
    }

    fn park_x_desktop_px_v3(&self) -> i32 {
        self.park_x_desktop_px
    }

    fn park_y_desktop_px_v3(&self) -> i32 {
        self.park_y_desktop_px
    }
}

impl VerifiedPointerTargetV3 for OpaqueMtgoPreparedCompetitiveDuelPassV1 {
    fn hwnd_v3(&self) -> u64 {
        self.hwnd
    }

    fn process_id_v3(&self) -> u32 {
        self.process_id
    }

    fn process_start_filetime_100ns_v3(&self) -> u64 {
        self.process_start_filetime_100ns
    }

    fn dpi_v3(&self) -> u32 {
        self.dpi
    }

    fn client_rect_desktop_px_v3(&self) -> &crate::SignedRectV1 {
        &self.client_rect_desktop_px
    }

    fn target_x_desktop_px_v3(&self) -> i32 {
        self.target_x_desktop_px
    }

    fn target_y_desktop_px_v3(&self) -> i32 {
        self.target_y_desktop_px
    }

    fn park_x_desktop_px_v3(&self) -> i32 {
        self.park_x_desktop_px
    }

    fn park_y_desktop_px_v3(&self) -> i32 {
        self.park_y_desktop_px
    }
}

impl VerifiedPointerTargetV3 for OpaqueMtgoPreparedCompetitiveEntryV1 {
    fn hwnd_v3(&self) -> u64 {
        self.pointer_target.hwnd
    }

    fn process_id_v3(&self) -> u32 {
        self.pointer_target.process_id
    }

    fn process_start_filetime_100ns_v3(&self) -> u64 {
        self.pointer_target.process_start_filetime_100ns
    }

    fn dpi_v3(&self) -> u32 {
        self.pointer_target.dpi
    }

    fn client_rect_desktop_px_v3(&self) -> &crate::SignedRectV1 {
        &self.pointer_target.client_rect_desktop_px
    }

    fn target_x_desktop_px_v3(&self) -> i32 {
        self.pointer_target.target_x_desktop_px
    }

    fn target_y_desktop_px_v3(&self) -> i32 {
        self.pointer_target.target_y_desktop_px
    }

    fn park_x_desktop_px_v3(&self) -> i32 {
        self.pointer_target.park_x_desktop_px
    }

    fn park_y_desktop_px_v3(&self) -> i32 {
        self.pointer_target.park_y_desktop_px
    }
}

impl VerifiedPointerTargetV3 for OpaqueMtgoPreparedCompetitiveOpenEntryReviewV1 {
    fn hwnd_v3(&self) -> u64 {
        self.pointer_target.hwnd
    }

    fn process_id_v3(&self) -> u32 {
        self.pointer_target.process_id
    }

    fn process_start_filetime_100ns_v3(&self) -> u64 {
        self.pointer_target.process_start_filetime_100ns
    }

    fn dpi_v3(&self) -> u32 {
        self.pointer_target.dpi
    }

    fn client_rect_desktop_px_v3(&self) -> &crate::SignedRectV1 {
        &self.pointer_target.client_rect_desktop_px
    }

    fn target_x_desktop_px_v3(&self) -> i32 {
        self.pointer_target.target_x_desktop_px
    }

    fn target_y_desktop_px_v3(&self) -> i32 {
        self.pointer_target.target_y_desktop_px
    }

    fn park_x_desktop_px_v3(&self) -> i32 {
        self.pointer_target.park_x_desktop_px
    }

    fn park_y_desktop_px_v3(&self) -> i32 {
        self.pointer_target.park_y_desktop_px
    }
}

impl VerifiedPointerTargetV3 for OpaqueMtgoPreparedCompetitiveLifecycleControlV1 {
    fn hwnd_v3(&self) -> u64 {
        self.pointer_target.hwnd
    }

    fn process_id_v3(&self) -> u32 {
        self.pointer_target.process_id
    }

    fn process_start_filetime_100ns_v3(&self) -> u64 {
        self.pointer_target.process_start_filetime_100ns
    }

    fn dpi_v3(&self) -> u32 {
        self.pointer_target.dpi
    }

    fn client_rect_desktop_px_v3(&self) -> &crate::SignedRectV1 {
        &self.pointer_target.client_rect_desktop_px
    }

    fn target_x_desktop_px_v3(&self) -> i32 {
        self.pointer_target.target_x_desktop_px
    }

    fn target_y_desktop_px_v3(&self) -> i32 {
        self.pointer_target.target_y_desktop_px
    }

    fn park_x_desktop_px_v3(&self) -> i32 {
        self.pointer_target.park_x_desktop_px
    }

    fn park_y_desktop_px_v3(&self) -> i32 {
        self.pointer_target.park_y_desktop_px
    }
}

struct VerifiedSideboardDragPointTargetV1<'a> {
    pointer: &'a MtgoCompetitiveSideboardDragPointerTargetV1,
    x: i32,
    y: i32,
}

impl VerifiedPointerTargetV3 for VerifiedSideboardDragPointTargetV1<'_> {
    fn hwnd_v3(&self) -> u64 {
        self.pointer.hwnd
    }

    fn process_id_v3(&self) -> u32 {
        self.pointer.process_id
    }

    fn process_start_filetime_100ns_v3(&self) -> u64 {
        self.pointer.process_start_filetime_100ns
    }

    fn dpi_v3(&self) -> u32 {
        self.pointer.dpi
    }

    fn client_rect_desktop_px_v3(&self) -> &crate::SignedRectV1 {
        &self.pointer.client_rect_desktop_px
    }

    fn target_x_desktop_px_v3(&self) -> i32 {
        self.x
    }

    fn target_y_desktop_px_v3(&self) -> i32 {
        self.y
    }

    fn park_x_desktop_px_v3(&self) -> i32 {
        self.pointer.park_x_desktop_px
    }

    fn park_y_desktop_px_v3(&self) -> i32 {
        self.pointer.park_y_desktop_px
    }
}

struct VerifiedGesturePointTargetV1<'a> {
    prepared: &'a ProbeOpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1,
    target_x_desktop_px: i32,
    target_y_desktop_px: i32,
}

struct VerifiedPlayerVisibleGameplayPointTargetV1<'a> {
    prepared: &'a OpaqueMtgoPreparedPlayerVisibleDuelGesturePointerV1,
    target_x_desktop_px: i32,
    target_y_desktop_px: i32,
}

impl VerifiedPointerTargetV3 for VerifiedPlayerVisibleGameplayPointTargetV1<'_> {
    fn hwnd_v3(&self) -> u64 {
        self.prepared.hwnd
    }

    fn process_id_v3(&self) -> u32 {
        self.prepared.process_id
    }

    fn process_start_filetime_100ns_v3(&self) -> u64 {
        self.prepared.process_start_filetime_100ns
    }

    fn dpi_v3(&self) -> u32 {
        self.prepared.dpi
    }

    fn client_rect_desktop_px_v3(&self) -> &crate::SignedRectV1 {
        &self.prepared.client_rect_desktop_px
    }

    fn target_x_desktop_px_v3(&self) -> i32 {
        self.target_x_desktop_px
    }

    fn target_y_desktop_px_v3(&self) -> i32 {
        self.target_y_desktop_px
    }

    fn park_x_desktop_px_v3(&self) -> i32 {
        self.prepared.park_x_desktop_px
    }

    fn park_y_desktop_px_v3(&self) -> i32 {
        self.prepared.park_y_desktop_px
    }
}

impl VerifiedPointerTargetV3 for VerifiedGesturePointTargetV1<'_> {
    fn hwnd_v3(&self) -> u64 {
        self.prepared.hwnd
    }

    fn process_id_v3(&self) -> u32 {
        self.prepared.process_id
    }

    fn process_start_filetime_100ns_v3(&self) -> u64 {
        self.prepared.process_start_filetime_100ns
    }

    fn dpi_v3(&self) -> u32 {
        self.prepared.dpi
    }

    fn client_rect_desktop_px_v3(&self) -> &crate::SignedRectV1 {
        &self.prepared.client_rect_desktop_px
    }

    fn target_x_desktop_px_v3(&self) -> i32 {
        self.target_x_desktop_px
    }

    fn target_y_desktop_px_v3(&self) -> i32 {
        self.target_y_desktop_px
    }

    fn park_x_desktop_px_v3(&self) -> i32 {
        self.prepared.park_x_desktop_px
    }

    fn park_y_desktop_px_v3(&self) -> i32 {
        self.prepared.park_y_desktop_px
    }
}

fn send_exactly_one_gesture_primitive_v1(
    prepared: &ProbeOpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1,
) -> Result<(u8, bool), String> {
    let expected_target_count = match &prepared.primitive {
        MtgoDuelGesturePrimitiveV1::DragPrimaryToCalibratedPlayArea
        | MtgoDuelGesturePrimitiveV1::DragObjectToOrderSlot { .. } => 2,
        _ => 1,
    };
    if prepared.target_points_desktop_px.len() != expected_target_count {
        return Err("gesture primitive target count changed before input".to_owned());
    }
    let previous_context =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if previous_context.is_invalid() {
        return Err("the input thread could not enter Per-Monitor V2 DPI awareness".to_owned());
    }
    let _dpi_guard = ActuatorDpiGuardV3(previous_context);
    if !unsafe {
        AreDpiAwarenessContextsEqual(
            GetThreadDpiAwarenessContext(),
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        )
    }
    .as_bool()
    {
        return Err("the input thread is not Per-Monitor V2 DPI aware".to_owned());
    }

    let first = prepared.target_points_desktop_px[0];
    let first_target = VerifiedGesturePointTargetV1 {
        prepared,
        target_x_desktop_px: first.0,
        target_y_desktop_px: first.1,
    };
    verify_live_target_v3(&first_target, false)?;
    let mut cursor_park_guard = CursorParkGuardV3 {
        x: prepared.park_x_desktop_px,
        y: prepared.park_y_desktop_px,
        parked: false,
    };
    unsafe { SetCursorPos(first.0, first.1) }
        .map_err(|error| format!("move cursor to gesture source: {error}"))?;
    verify_live_target_v3(&first_target, true)?;

    let emitted = match &prepared.primitive {
        MtgoDuelGesturePrimitiveV1::ActivatePrimary { activation }
        | MtgoDuelGesturePrimitiveV1::ActivateSemanticMenuChoice { activation } => {
            send_mouse_activation_v1(*activation)?
        }
        MtgoDuelGesturePrimitiveV1::SelectObject { .. } | MtgoDuelGesturePrimitiveV1::Submit => {
            send_mouse_activation_v1(MtgoDuelPrimaryActivationV1::SingleLeftClick)?
        }
        MtgoDuelGesturePrimitiveV1::DragPrimaryToCalibratedPlayArea
        | MtgoDuelGesturePrimitiveV1::DragObjectToOrderSlot { .. } => {
            let down = [mouse_input_record_v1(MOUSEEVENTF_LEFTDOWN)];
            if unsafe { SendInput(&down, size_of::<INPUT>() as i32) } != 1 {
                release_mouse_buttons_v1();
                return Err("SendInput did not emit the gesture drag press".to_owned());
            }
            let second = prepared.target_points_desktop_px[1];
            let second_target = VerifiedGesturePointTargetV1 {
                prepared,
                target_x_desktop_px: second.0,
                target_y_desktop_px: second.1,
            };
            if let Err(error) = unsafe { SetCursorPos(second.0, second.1) }
                .map_err(|error| format!("move cursor to gesture destination: {error}"))
                .and_then(|_| verify_live_target_v3(&second_target, true))
            {
                release_mouse_buttons_v1();
                return Err(error);
            }
            let up = [mouse_input_record_v1(MOUSEEVENTF_LEFTUP)];
            if unsafe { SendInput(&up, size_of::<INPUT>() as i32) } != 1 {
                release_mouse_buttons_v1();
                return Err("SendInput did not emit the gesture drag release".to_owned());
            }
            2
        }
    };
    Ok((emitted, cursor_park_guard.park_now()))
}

fn send_exactly_one_player_visible_gameplay_primitive_v1(
    prepared: &OpaqueMtgoPreparedPlayerVisibleDuelGesturePointerV1,
) -> Result<(u8, bool), String> {
    let expected_target_count = match &prepared.primitive {
        MtgoPlayerVisibleDuelGesturePrimitiveV1::DragPrimaryToCalibratedPlayArea
        | MtgoPlayerVisibleDuelGesturePrimitiveV1::DragVisibleObjectToOrderSlot { .. } => 2,
        _ => 1,
    };
    if prepared.target_points_desktop_px.len() != expected_target_count {
        return Err(
            "player-visible gesture primitive target count changed before input".to_owned(),
        );
    }
    let previous_context =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if previous_context.is_invalid() {
        return Err("the input thread could not enter Per-Monitor V2 DPI awareness".to_owned());
    }
    let _dpi_guard = ActuatorDpiGuardV3(previous_context);
    if !unsafe {
        AreDpiAwarenessContextsEqual(
            GetThreadDpiAwarenessContext(),
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        )
    }
    .as_bool()
    {
        return Err("the input thread is not Per-Monitor V2 DPI aware".to_owned());
    }

    let first = prepared.target_points_desktop_px[0];
    let first_target = VerifiedPlayerVisibleGameplayPointTargetV1 {
        prepared,
        target_x_desktop_px: first.0,
        target_y_desktop_px: first.1,
    };
    verify_live_target_v3(&first_target, false)?;
    let mut cursor_park_guard = CursorParkGuardV3 {
        x: prepared.park_x_desktop_px,
        y: prepared.park_y_desktop_px,
        parked: false,
    };
    unsafe { SetCursorPos(first.0, first.1) }
        .map_err(|error| format!("move cursor to player-visible gesture source: {error}"))?;
    verify_live_target_v3(&first_target, true)?;

    let emitted = match &prepared.primitive {
        MtgoPlayerVisibleDuelGesturePrimitiveV1::ActivatePrimary { activation }
        | MtgoPlayerVisibleDuelGesturePrimitiveV1::ActivateSemanticMenuChoice { activation } => {
            send_mouse_activation_v1(*activation)?
        }
        MtgoPlayerVisibleDuelGesturePrimitiveV1::SelectVisibleObject { .. }
        | MtgoPlayerVisibleDuelGesturePrimitiveV1::Submit => {
            send_mouse_activation_v1(MtgoDuelPrimaryActivationV1::SingleLeftClick)?
        }
        MtgoPlayerVisibleDuelGesturePrimitiveV1::DragPrimaryToCalibratedPlayArea
        | MtgoPlayerVisibleDuelGesturePrimitiveV1::DragVisibleObjectToOrderSlot { .. } => {
            let down = [mouse_input_record_v1(MOUSEEVENTF_LEFTDOWN)];
            if unsafe { SendInput(&down, size_of::<INPUT>() as i32) } != 1 {
                release_mouse_buttons_v1();
                return Err("SendInput did not emit the player-visible drag press".to_owned());
            }
            let second = prepared.target_points_desktop_px[1];
            let second_target = VerifiedPlayerVisibleGameplayPointTargetV1 {
                prepared,
                target_x_desktop_px: second.0,
                target_y_desktop_px: second.1,
            };
            if let Err(error) = unsafe { SetCursorPos(second.0, second.1) }
                .map_err(|error| format!("move cursor to player-visible drag destination: {error}"))
                .and_then(|_| verify_live_target_v3(&second_target, true))
            {
                release_mouse_buttons_v1();
                return Err(error);
            }
            let up = [mouse_input_record_v1(MOUSEEVENTF_LEFTUP)];
            if unsafe { SendInput(&up, size_of::<INPUT>() as i32) } != 1 {
                release_mouse_buttons_v1();
                return Err("SendInput did not emit the player-visible drag release".to_owned());
            }
            2
        }
    };
    Ok((emitted, cursor_park_guard.park_now()))
}

fn send_exactly_one_sideboard_drag_v1(
    pointer: &MtgoCompetitiveSideboardDragPointerTargetV1,
) -> Result<(u8, bool), String> {
    let previous_context =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if previous_context.is_invalid() {
        return Err("the input thread could not enter Per-Monitor V2 DPI awareness".to_owned());
    }
    let _dpi_guard = ActuatorDpiGuardV3(previous_context);
    if !unsafe {
        AreDpiAwarenessContextsEqual(
            GetThreadDpiAwarenessContext(),
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        )
    }
    .as_bool()
    {
        return Err("the input thread is not Per-Monitor V2 DPI aware".to_owned());
    }
    let source = VerifiedSideboardDragPointTargetV1 {
        pointer,
        x: pointer.source_x_desktop_px,
        y: pointer.source_y_desktop_px,
    };
    let destination = VerifiedSideboardDragPointTargetV1 {
        pointer,
        x: pointer.destination_x_desktop_px,
        y: pointer.destination_y_desktop_px,
    };
    verify_live_target_v3(&source, false)?;
    verify_live_target_v3(&destination, false)?;
    let mut cursor_park_guard = CursorParkGuardV3 {
        x: pointer.park_x_desktop_px,
        y: pointer.park_y_desktop_px,
        parked: false,
    };
    unsafe { SetCursorPos(pointer.source_x_desktop_px, pointer.source_y_desktop_px) }
        .map_err(|error| format!("move cursor to sideboard source card: {error}"))?;
    verify_live_target_v3(&source, true)?;
    let down = [mouse_input_record_v1(MOUSEEVENTF_LEFTDOWN)];
    if unsafe { SendInput(&down, size_of::<INPUT>() as i32) } != 1 {
        release_mouse_buttons_v1();
        return Err("SendInput did not emit the sideboard drag press".to_owned());
    }
    if let Err(error) = unsafe {
        SetCursorPos(
            pointer.destination_x_desktop_px,
            pointer.destination_y_desktop_px,
        )
    }
    .map_err(|error| format!("move cursor to sideboard destination: {error}"))
    .and_then(|_| verify_live_target_v3(&destination, true))
    {
        release_mouse_buttons_v1();
        return Err(error);
    }
    let up = [mouse_input_record_v1(MOUSEEVENTF_LEFTUP)];
    if unsafe { SendInput(&up, size_of::<INPUT>() as i32) } != 1 {
        release_mouse_buttons_v1();
        return Err("SendInput did not emit the sideboard drag release".to_owned());
    }
    Ok((2, cursor_park_guard.park_now()))
}

fn send_mouse_activation_v1(activation: MtgoDuelPrimaryActivationV1) -> Result<u8, String> {
    let records = match activation {
        MtgoDuelPrimaryActivationV1::SingleLeftClick => vec![
            mouse_input_record_v1(MOUSEEVENTF_LEFTDOWN),
            mouse_input_record_v1(MOUSEEVENTF_LEFTUP),
        ],
        MtgoDuelPrimaryActivationV1::DoubleLeftClick => vec![
            mouse_input_record_v1(MOUSEEVENTF_LEFTDOWN),
            mouse_input_record_v1(MOUSEEVENTF_LEFTUP),
            mouse_input_record_v1(MOUSEEVENTF_LEFTDOWN),
            mouse_input_record_v1(MOUSEEVENTF_LEFTUP),
        ],
        MtgoDuelPrimaryActivationV1::RightClick => vec![
            mouse_input_record_v1(MOUSEEVENTF_RIGHTDOWN),
            mouse_input_record_v1(MOUSEEVENTF_RIGHTUP),
        ],
    };
    let expected = u32::try_from(records.len()).map_err(|_| "mouse record count overflow")?;
    let sent = unsafe { SendInput(&records, size_of::<INPUT>() as i32) };
    if sent != expected {
        release_mouse_buttons_v1();
        return Err(format!(
            "SendInput emitted {sent} of {expected} required gesture mouse records"
        ));
    }
    u8::try_from(expected).map_err(|_| "mouse record count exceeds u8".to_owned())
}

fn mouse_input_record_v1(
    flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flags,
                ..Default::default()
            },
        },
    }
}

fn release_mouse_buttons_v1() {
    let releases = [
        mouse_input_record_v1(MOUSEEVENTF_LEFTUP),
        mouse_input_record_v1(MOUSEEVENTF_RIGHTUP),
    ];
    let _ = unsafe { SendInput(&releases, size_of::<INPUT>() as i32) };
}

fn send_exactly_one_left_click_v3<T: VerifiedPointerTargetV3>(
    prepared: &T,
) -> Result<bool, String> {
    let previous_context =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if previous_context.is_invalid() {
        return Err("the input thread could not enter Per-Monitor V2 DPI awareness".to_owned());
    }
    let _dpi_guard = ActuatorDpiGuardV3(previous_context);
    if !unsafe {
        AreDpiAwarenessContextsEqual(
            GetThreadDpiAwarenessContext(),
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        )
    }
    .as_bool()
    {
        return Err("the input thread is not Per-Monitor V2 DPI aware".to_owned());
    }

    verify_live_target_v3(prepared, false)?;
    let mut cursor_park_guard = CursorParkGuardV3 {
        x: prepared.park_x_desktop_px_v3(),
        y: prepared.park_y_desktop_px_v3(),
        parked: false,
    };
    unsafe {
        SetCursorPos(
            prepared.target_x_desktop_px_v3(),
            prepared.target_y_desktop_px_v3(),
        )
    }
    .map_err(|error| format!("move cursor to selected control: {error}"))?;
    verify_live_target_v3(prepared, true)?;

    let inputs = [
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dwFlags: MOUSEEVENTF_LEFTDOWN,
                    ..Default::default()
                },
            },
        },
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dwFlags: MOUSEEVENTF_LEFTUP,
                    ..Default::default()
                },
            },
        },
    ];
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent != 2 {
        if sent == 1 {
            let release = [INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dwFlags: MOUSEEVENTF_LEFTUP,
                        ..Default::default()
                    },
                },
            }];
            let _ = unsafe { SendInput(&release, size_of::<INPUT>() as i32) };
        }
        return Err(format!(
            "SendInput emitted {sent} of 2 required mouse records"
        ));
    }
    Ok(cursor_park_guard.park_now())
}

struct ActuatorDpiGuardV3(windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT);

impl Drop for ActuatorDpiGuardV3 {
    fn drop(&mut self) {
        unsafe {
            let _ = SetThreadDpiAwarenessContext(self.0);
        }
    }
}

struct CursorParkGuardV3 {
    x: i32,
    y: i32,
    parked: bool,
}

impl CursorParkGuardV3 {
    fn park_now(&mut self) -> bool {
        self.parked = unsafe { SetCursorPos(self.x, self.y) }.is_ok();
        self.parked
    }
}

impl Drop for CursorParkGuardV3 {
    fn drop(&mut self) {
        if !self.parked {
            let _ = unsafe { SetCursorPos(self.x, self.y) };
        }
    }
}

fn verify_live_target_v3<T: VerifiedPointerTargetV3>(
    prepared: &T,
    require_point_hit_test: bool,
) -> Result<(), String> {
    let hwnd = HWND(prepared.hwnd_v3() as *mut c_void);
    if !unsafe { IsWindow(Some(hwnd)) }.as_bool()
        || !unsafe { IsWindowVisible(hwnd) }.as_bool()
        || unsafe { IsIconic(hwnd) }.as_bool()
        || unsafe { IsHungAppWindow(hwnd) }.as_bool()
        || unsafe { GetForegroundWindow() } != hwnd
        || unsafe { GetAncestor(hwnd, GA_ROOT) } != hwnd
    {
        return Err("the authorized MTGO window lost foreground admission".to_owned());
    }
    let mut process_id = 0_u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    if process_id != prepared.process_id_v3()
        || unsafe { GetDpiForWindow(hwnd) } != prepared.dpi_v3()
    {
        return Err("the authorized MTGO process or DPI changed before input".to_owned());
    }
    if live_process_start_filetime_v3(prepared.process_id_v3())?
        != prepared.process_start_filetime_100ns_v3()
    {
        return Err("the authorized MTGO process incarnation changed before input".to_owned());
    }

    let mut client = RECT::default();
    unsafe { GetClientRect(hwnd, &mut client) }
        .map_err(|error| format!("read MTGO client bounds before input: {error}"))?;
    let mut origin = POINT {
        x: client.left,
        y: client.top,
    };
    if !unsafe { ClientToScreen(hwnd, &mut origin) }.as_bool() {
        return Err("map MTGO client bounds before input failed".to_owned());
    }
    let right = origin
        .x
        .checked_add(client.right - client.left)
        .ok_or("current MTGO client right overflow")?;
    let bottom = origin
        .y
        .checked_add(client.bottom - client.top)
        .ok_or("current MTGO client bottom overflow")?;
    if prepared.client_rect_desktop_px_v3().left != origin.x
        || prepared.client_rect_desktop_px_v3().top != origin.y
        || prepared.client_rect_desktop_px_v3().right != right
        || prepared.client_rect_desktop_px_v3().bottom != bottom
    {
        return Err("the MTGO client geometry changed before input".to_owned());
    }

    if require_point_hit_test {
        let hit = unsafe {
            WindowFromPoint(POINT {
                x: prepared.target_x_desktop_px_v3(),
                y: prepared.target_y_desktop_px_v3(),
            })
        };
        if hit.0.is_null() || unsafe { GetAncestor(hit, GA_ROOT) } != hwnd {
            return Err("another window owns the selected control point".to_owned());
        }
        let mut hit_process_id = 0_u32;
        unsafe { GetWindowThreadProcessId(hit, Some(&mut hit_process_id)) };
        if hit_process_id != prepared.process_id_v3() {
            return Err("the selected control point is not owned by MTGO".to_owned());
        }
    }
    Ok(())
}

struct ActuatorProcessHandleV3(HANDLE);

impl Drop for ActuatorProcessHandleV3 {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn live_process_start_filetime_v3(process_id: u32) -> Result<u64, String> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
        .map_err(|error| format!("open MTGO process before input: {error}"))?;
    let handle = ActuatorProcessHandleV3(handle);
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe { GetProcessTimes(handle.0, &mut creation, &mut exit, &mut kernel, &mut user) }
        .map_err(|error| format!("read MTGO process start before input: {error}"))?;
    Ok(((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64)
}

fn input_receipt_commitment_v3(
    prepared: &PreparedPregameActuationV3,
    authorization: &MtgoAuthorizationScopeV1,
    input_sent_at_unix_millis: u128,
    cursor_parked_outside_client: bool,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(PREGAME_INPUT_RECEIPT_DOMAIN_V3);
    for part in [
        authorization.account_alias_sha256.as_bytes(),
        authorization.written_permission_sha256.as_bytes(),
        prepared.action_plan_commitment_sha256.as_bytes(),
        prepared.current_capture_commitment_sha256.as_bytes(),
    ] {
        update_hash_part_v3(&mut hasher, part);
    }
    update_hash_part_v3(
        &mut hasher,
        semantic_bytes_v3(&prepared.selected_semantic).as_bytes(),
    );
    update_hash_part_v3(
        &mut hasher,
        postcondition_bytes_v3(&prepared.planned_postcondition).as_bytes(),
    );
    for coordinate in [
        prepared.target_x_desktop_px,
        prepared.target_y_desktop_px,
        prepared.park_x_desktop_px,
        prepared.park_y_desktop_px,
    ] {
        update_hash_part_v3(&mut hasher, &coordinate.to_be_bytes());
    }
    update_hash_part_v3(&mut hasher, &input_sent_at_unix_millis.to_be_bytes());
    update_hash_part_v3(&mut hasher, &[u8::from(cursor_parked_outside_client)]);
    format!("{:x}", hasher.finalize())
}

fn semantic_bytes_v3(semantic: &MtgoPregameActionSemanticV1) -> String {
    match semantic {
        MtgoPregameActionSemanticV1::Mulligan { next_hand_size } => {
            format!("mulligan:{next_hand_size}")
        }
        MtgoPregameActionSemanticV1::KeepOpeningHand => "keep_opening_hand".to_owned(),
    }
}

fn postcondition_bytes_v3(postcondition: &MtgoPlannedPregamePostconditionV3) -> String {
    match postcondition {
        MtgoPlannedPregamePostconditionV3::NextMulliganPrompt {
            prospective_keep_size,
        } => format!("next_mulligan_prompt:{prospective_keep_size}"),
        MtgoPlannedPregamePostconditionV3::LondonBottoming {
            required_bottom_count,
        } => format!("london_bottoming:{required_bottom_count}"),
        MtgoPlannedPregamePostconditionV3::GameplayFirstMain => "gameplay_first_main".to_owned(),
    }
}

fn update_hash_part_v3(hasher: &mut Sha256, part: &[u8]) {
    hasher.update((part.len() as u64).to_be_bytes());
    hasher.update(part);
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtgo_blackbox_v1::{
        check_untrusted_authorization_correspondence_v1, validate_competitive_deck_manifest_v1,
        validate_visible_competitive_lifecycle_snapshot_v1,
        MtgoAuthorizationCorrespondenceReviewV1, MtgoCompetitiveDeckCardCountV1,
        MtgoCompetitiveDeckManifestV1, MtgoLifecycleVisibleFactKindV1, MtgoLifecycleVisibleFactV1,
        MtgoRectPxV1, MtgoVisibleCompetitiveLifecycleSnapshotV1,
        MTGO_AUTHORIZATION_CORRESPONDENCE_REVIEW_SCHEMA_V1, MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
    };

    fn authorized_scope_v3(alias: &str) -> MtgoAuthorizationScopeV1 {
        MtgoAuthorizationScopeV1 {
            account_alias_sha256: format!("{:x}", Sha256::digest(alias.as_bytes())),
            written_permission_sha256: "a".repeat(64),
            private_match_input: true,
            ..MtgoAuthorizationScopeV1::default()
        }
    }

    #[test]
    fn player_visible_compound_gesture_requires_exact_confirmed_prefix_chain() {
        let digest = "a".repeat(64);
        assert!(player_visible_confirmed_primitive_chain_matches_v1(
            0, 0, None
        ));
        assert!(!player_visible_confirmed_primitive_chain_matches_v1(
            0,
            0,
            Some(&digest)
        ));
        assert!(player_visible_confirmed_primitive_chain_matches_v1(
            1,
            1,
            Some(&digest)
        ));
        assert!(!player_visible_confirmed_primitive_chain_matches_v1(
            1,
            2,
            Some(&digest)
        ));
        assert!(!player_visible_confirmed_primitive_chain_matches_v1(
            1,
            1,
            Some("not-a-sha256")
        ));
        assert!(!player_visible_confirmed_primitive_chain_matches_v1(
            1, 1, None
        ));
    }

    fn competitive_scope_v1(
        alias: &str,
        event_kind: MtgoCompetitiveEventKindV1,
    ) -> MtgoAuthorizationScopeV1 {
        let mut scope = MtgoAuthorizationScopeV1 {
            account_alias_sha256: format!("{:x}", Sha256::digest(alias.as_bytes())),
            written_permission_sha256: "b".repeat(64),
            ..MtgoAuthorizationScopeV1::default()
        };
        match event_kind {
            MtgoCompetitiveEventKindV1::League => scope.league_input = true,
            MtgoCompetitiveEventKindV1::Challenge => scope.challenge_input = true,
        }
        scope
    }

    fn checked_competitive_correspondence_v2() -> CheckedUntrustedMtgoAuthorizationCorrespondenceV1
    {
        const BYTES: &[u8] = b"exact private Daybreak correspondence test fixture\r\n";
        let account_alias = "UnbuckledPie";
        check_untrusted_authorization_correspondence_v1(
            MtgoAuthorizationCorrespondenceReviewV1 {
                schema_version: MTGO_AUTHORIZATION_CORRESPONDENCE_REVIEW_SCHEMA_V1,
                review_id: "daybreak-visible-competitive-test-v1".to_owned(),
                correspondence_sha256: format!("{:x}", Sha256::digest(BYTES)),
                approved_account_alias_sha256: format!(
                    "{:x}",
                    Sha256::digest(account_alias.as_bytes())
                ),
                approved_competitive_modes: vec![
                    MtgoCompetitiveEventKindV1::League,
                    MtgoCompetitiveEventKindV1::Challenge,
                ],
                visible_channels_only: true,
                hidden_information_access_prohibited: true,
                hidden_information_reverse_engineering_prohibited: true,
                cheating_or_hacking_prohibited: true,
                account_owner_event_entry_confirmation_required: true,
                account_owner_spending_confirmation_required: true,
                reviewer_alias_sha256: format!("{:x}", Sha256::digest(b"local-owner-reviewer")),
                reviewed_at_utc: "2026-08-10T20:00:00Z".to_owned(),
            },
            BYTES,
            account_alias,
        )
        .unwrap()
    }

    fn checked_competitive_entry_review_snapshot_v1(
        event_kind: MtgoCompetitiveEventKindV1,
        resource: MtgoCompetitiveEntryResourceV1,
        amount: u32,
    ) -> CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1 {
        validate_visible_competitive_lifecycle_snapshot_v1(
            MtgoVisibleCompetitiveLifecycleSnapshotV1 {
                schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
                snapshot_id: "attended-entry-review-test-v1".to_owned(),
                event_kind,
                phase: MtgoCompetitiveLifecyclePhaseV1::EntryReview,
                frame_id: 17,
                frame_sequence: 41,
                frame_sha256: "1".repeat(64),
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
                        rect_client_px: MtgoRectPxV1 {
                            x: 10,
                            y: 10,
                            width: 30,
                            height: 20,
                        },
                        content_sha256: "4".repeat(64),
                        confidence_bps: 10_000,
                    },
                    MtgoLifecycleVisibleFactV1 {
                        kind: MtgoLifecycleVisibleFactKindV1::EntryTermsVisible,
                        rect_client_px: MtgoRectPxV1 {
                            x: 10,
                            y: 40,
                            width: 40,
                            height: 20,
                        },
                        content_sha256: "5".repeat(64),
                        confidence_bps: 10_000,
                    },
                ],
            },
        )
        .unwrap()
    }

    fn entry_source_identity_commitments_v3(
        classifier_commitment_sha256: Option<String>,
    ) -> MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1 {
        MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1 {
            source_capture_commitment_sha256: "1".repeat(64),
            source_lifecycle_snapshot_commitment_sha256: "2".repeat(64),
            source_window_title_sha256: "3".repeat(64),
            event_label_region_sha256: "4".repeat(64),
            source_identity_commitment_sha256: "5".repeat(64),
            source_navigation_classification_result_commitment_sha256: classifier_commitment_sha256,
            event_kind: MtgoCompetitiveEventKindV1::League,
            frame_id: 17,
            frame_sequence: 41,
            resource: MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
            amount: 100,
        }
    }

    fn source_bound_entry_review_commitments_v3(
        classifier_commitment_sha256: String,
    ) -> MtgoSourceBoundCompetitiveEntryReviewCommitmentsV2 {
        MtgoSourceBoundCompetitiveEntryReviewCommitmentsV2 {
            source_identity_commitment_sha256: "5".repeat(64),
            source_capture_commitment_sha256: "1".repeat(64),
            source_navigation_classification_result_commitment_sha256: Some(
                classifier_commitment_sha256,
            ),
            attended_review: MtgoAttendedCompetitiveEntryReviewCommitmentsV1 {
                source_lifecycle_snapshot_commitment_sha256: "2".repeat(64),
                permission_review_commitment_sha256: "6".repeat(64),
                mode_authorization_commitment_sha256: "7".repeat(64),
                request_commitment_sha256: "8".repeat(64),
                entry_authorization_sha256: "9".repeat(64),
                owner_review_receipt_sha256: "a".repeat(64),
                event_kind: MtgoCompetitiveEventKindV1::League,
                frame_id: 17,
                frame_sequence: 41,
                resource: MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                amount: 100,
            },
        }
    }

    fn classifier_bound_entry_review_commitments_v4(
        classifier_commitment_sha256: String,
    ) -> MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3 {
        let source_bound =
            source_bound_entry_review_commitments_v3(classifier_commitment_sha256.clone());
        let classifier_bound_review_commitment_sha256 =
            classifier_bound_competitive_entry_review_commitment_v3(
                &source_bound,
                &classifier_commitment_sha256,
            );
        MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3 {
            source_navigation_classification_result_commitment_sha256: classifier_commitment_sha256,
            classifier_bound_review_commitment_sha256,
            source_bound_review: source_bound,
        }
    }

    fn entry_control_dry_run_commitments_v4(
        classifier_commitment_sha256: String,
    ) -> MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1 {
        MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1 {
            source_identity_commitment_sha256: "5".repeat(64),
            source_capture_commitment_sha256: "1".repeat(64),
            source_lifecycle_snapshot_commitment_sha256: "2".repeat(64),
            source_navigation_classification_result_commitment_sha256: classifier_commitment_sha256,
            visible_control_label_sha256: "c".repeat(64),
            visible_control_region_sha256: "d".repeat(64),
            selected_deck_label_sha256: "f".repeat(64),
            selected_deck_region_sha256: "0".repeat(64),
            deck_manifest_sha256: "1".repeat(64),
            deck_format_sha256: "2".repeat(64),
            policy_deployment_commitment_sha256: "3".repeat(64),
            visibly_enabled_confirmed: true,
            dry_run_commitment_sha256: "e".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::League,
            frame_id: 17,
            frame_sequence: 41,
            resource: MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
            amount: 100,
        }
    }

    fn competitive_entry_ratification_parts_v1(
        event_kind: MtgoCompetitiveEventKindV1,
        resource: MtgoCompetitiveEntryResourceV1,
        amount: u32,
    ) -> (
        CheckedUntrustedMtgoAuthorizationCorrespondenceV1,
        CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
        MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1,
        MtgoCompetitiveEntryAuthorizationV1,
        MtgoControlBoundCompetitiveEntryReviewCommitmentsV4,
    ) {
        let correspondence = checked_competitive_correspondence_v2();
        let source = checked_competitive_entry_review_snapshot_v1(event_kind, resource, amount);
        let reviewed_source =
            checked_competitive_entry_review_snapshot_v1(event_kind, resource, amount);
        let nonce = [0x31u8; 8];
        let phrase = attended_competitive_entry_review_confirmation_phrase_v1(
            event_kind, resource, amount, &nonce,
        );
        let reviewed = review_competitive_entry_from_attended_confirmation_v1(
            &correspondence,
            reviewed_source,
            "UnbuckledPie",
            match event_kind {
                MtgoCompetitiveEventKindV1::League => "Modern League".to_owned(),
                MtgoCompetitiveEventKindV1::Challenge => "Modern Challenge".to_owned(),
            },
            nonce,
            2_100,
            &phrase,
        )
        .unwrap();
        let attended = reviewed.commitments_v1();
        let entry_authorization = reviewed.entry_authorization_record_v1();
        let classifier_commitment = "b".repeat(64);
        let source_identity = MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1 {
            source_capture_commitment_sha256: "1".repeat(64),
            source_lifecycle_snapshot_commitment_sha256: source
                .snapshot_commitment_sha256()
                .to_owned(),
            source_window_title_sha256: "3".repeat(64),
            event_label_region_sha256: "4".repeat(64),
            source_identity_commitment_sha256: "5".repeat(64),
            source_navigation_classification_result_commitment_sha256: Some(
                classifier_commitment.clone(),
            ),
            event_kind,
            frame_id: source.frame_id_v1(),
            frame_sequence: source.frame_sequence(),
            resource,
            amount,
        };
        let source_bound = MtgoSourceBoundCompetitiveEntryReviewCommitmentsV2 {
            source_identity_commitment_sha256: source_identity
                .source_identity_commitment_sha256
                .clone(),
            source_capture_commitment_sha256: source_identity
                .source_capture_commitment_sha256
                .clone(),
            source_navigation_classification_result_commitment_sha256: Some(
                classifier_commitment.clone(),
            ),
            attended_review: attended,
        };
        let classifier_bound = MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3 {
            source_navigation_classification_result_commitment_sha256: classifier_commitment
                .clone(),
            classifier_bound_review_commitment_sha256:
                classifier_bound_competitive_entry_review_commitment_v3(
                    &source_bound,
                    &classifier_commitment,
                ),
            source_bound_review: source_bound,
        };
        let dry_run = MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1 {
            source_identity_commitment_sha256: source_identity
                .source_identity_commitment_sha256
                .clone(),
            source_capture_commitment_sha256: source_identity
                .source_capture_commitment_sha256
                .clone(),
            source_lifecycle_snapshot_commitment_sha256: source_identity
                .source_lifecycle_snapshot_commitment_sha256
                .clone(),
            source_navigation_classification_result_commitment_sha256: classifier_commitment,
            visible_control_label_sha256: "c".repeat(64),
            visible_control_region_sha256: "d".repeat(64),
            selected_deck_label_sha256: "f".repeat(64),
            selected_deck_region_sha256: "0".repeat(64),
            deck_manifest_sha256: "1".repeat(64),
            deck_format_sha256: "2".repeat(64),
            policy_deployment_commitment_sha256: "4".repeat(64),
            visibly_enabled_confirmed: true,
            dry_run_commitment_sha256: "e".repeat(64),
            event_kind,
            frame_id: source.frame_id_v1(),
            frame_sequence: source.frame_sequence(),
            resource,
            amount,
        };
        let deck_review_receipt_sha256 = "3".repeat(64);
        let control_bound_review_commitment_sha256 =
            bind_control_bound_competitive_entry_review_commitment_v4(
                &classifier_bound,
                &dry_run,
                &deck_review_receipt_sha256,
            )
            .unwrap();
        let control_bound = MtgoControlBoundCompetitiveEntryReviewCommitmentsV4 {
            control_bound_review_commitment_sha256,
            deck_review_receipt_sha256,
            classifier_bound_review: classifier_bound,
            entry_control_dry_run: dry_run,
        };
        (
            correspondence,
            source,
            source_identity,
            entry_authorization,
            control_bound,
        )
    }

    fn competitive_entry_immediate_recapture_commitments_v1(
        candidate: &MtgoReviewedCompetitiveEntryRatificationCandidateV1,
    ) -> MtgoCompetitiveEntryImmediateRecaptureCommitmentsV1 {
        MtgoCompetitiveEntryImmediateRecaptureCommitmentsV1 {
            navigation_profile_commitment_sha256: "6".repeat(64),
            navigation_profile_admission_commitment_sha256: "7".repeat(64),
            approved_account_alias_sha256: candidate.account_alias_sha256.clone(),
            runtime_identity_commitment_sha256: "8".repeat(64),
            window_continuity_commitment_sha256: "9".repeat(64),
            source_identity_commitment_sha256: candidate.source_identity_commitment_sha256.clone(),
            source_capture_commitment_sha256: candidate.source_capture_commitment_sha256.clone(),
            source_classification_result_commitment_sha256: candidate
                .source_navigation_classification_result_commitment_sha256
                .clone(),
            source_lifecycle_snapshot_commitment_sha256: "a".repeat(64),
            immediate_capture_commitment_sha256: "b".repeat(64),
            immediate_classification_result_commitment_sha256: "c".repeat(64),
            immediate_lifecycle_snapshot_commitment_sha256: "d".repeat(64),
            event_label_region_sha256: "e".repeat(64),
            visible_control_label_sha256: "f".repeat(64),
            visible_control_region_sha256: candidate.visible_control_region_sha256.clone(),
            selected_deck_label_sha256: candidate.selected_deck_label_sha256.clone(),
            selected_deck_region_sha256: candidate.selected_deck_region_sha256.clone(),
            deck_manifest_sha256: candidate.deck_manifest_sha256.clone(),
            deck_format_sha256: candidate.deck_format_sha256.clone(),
            policy_deployment_commitment_sha256: candidate
                .policy_deployment_commitment_sha256
                .clone(),
            event_identity_sha256: candidate.event_identity_sha256.clone(),
            entry_terms_sha256: candidate.entry_terms_sha256.clone(),
            event_kind: candidate.event_kind,
            resource: candidate.resource,
            amount: candidate.amount,
            source_frame_id: 17,
            source_frame_sequence: 41,
            source_captured_at_unix_millis: 410,
            immediate_frame_id: 18,
            immediate_frame_sequence: 42,
            immediate_captured_at_unix_millis: 420,
            recapture_commitment_sha256: "0".repeat(64),
        }
    }

    fn competitive_event_runtime_commitments_fixture_v1(
        phase: MtgoCompetitiveLifecyclePhaseV1,
    ) -> MtgoCompetitiveEventRuntimeCommitmentsV1 {
        let match_required = matches!(
            phase,
            MtgoCompetitiveLifecyclePhaseV1::PairingReady
                | MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
                | MtgoCompetitiveLifecyclePhaseV1::Sideboarding
                | MtgoCompetitiveLifecyclePhaseV1::MatchComplete
                | MtgoCompetitiveLifecyclePhaseV1::Reconnect
        );
        let game_required = matches!(
            phase,
            MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
                | MtgoCompetitiveLifecyclePhaseV1::Sideboarding
                | MtgoCompetitiveLifecyclePhaseV1::Reconnect
        );
        MtgoCompetitiveEventRuntimeCommitmentsV1 {
            runtime_commitment_sha256: "c".repeat(64),
            entry_confirmation_receipt_sha256: "1".repeat(64),
            entry_ratification_commitment_sha256: "2".repeat(64),
            entry_authorization_sha256: "d".repeat(64),
            correspondence_sha256: "e".repeat(64),
            permission_review_commitment_sha256: "f".repeat(64),
            deck_list_sha256: "a".repeat(64),
            deck_manifest_sha256: "0".repeat(64),
            deck_format_sha256: "1".repeat(64),
            player_known_current_deck_configuration_commitment_sha256: "b".repeat(64),
            selected_deck_label_sha256: "2".repeat(64),
            selected_deck_region_sha256: "3".repeat(64),
            policy_deployment_commitment_sha256: "4".repeat(64),
            lifecycle_authorization_commitment_sha256: "3".repeat(64),
            mode_authorization_commitment_sha256: "4".repeat(64),
            navigation_profile_commitment_sha256: "5".repeat(64),
            navigation_profile_admission_commitment_sha256: "6".repeat(64),
            approved_account_alias_sha256: "7".repeat(64),
            bound_event_identity_sha256: "8".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::League,
            current_phase: phase,
            current_lifecycle_snapshot_commitment_sha256: "9".repeat(64),
            current_match_identity_sha256: match_required.then(|| "a".repeat(64)),
            current_game_number: game_required.then_some(1),
            current_frame_id: 10,
            current_frame_sequence: 20,
            lifecycle_transition_count: 3,
            confirmed_lifecycle_action_count: 1,
            observed_lifecycle_advance_count: 2,
            pregame_session_count: 0,
            last_completed_pregame: None,
            gameplay_lease_count: 0,
            last_returned_gameplay_frame_sequence: None,
            event_monitor_chain_commitment_sha256: None,
            event_monitor_observation_count: 0,
            terminal_event_record_confirmed: false,
            closed_to_event_browser: false,
        }
    }

    fn competitive_native_pregame_deck_manifest_v1() -> ValidatedMtgoCompetitiveDeckManifestV1 {
        let mut configuration = MtgoCompetitiveDeckConfigurationV1 {
            mainboard: vec![
                MtgoCompetitiveDeckCardCountV1 {
                    card_db_id: mtg_kernel::card_def::card_id_by_name("Mountain").unwrap(),
                    card_name: "Mountain".to_owned(),
                    count: 56,
                },
                MtgoCompetitiveDeckCardCountV1 {
                    card_db_id: mtg_kernel::card_def::card_id_by_name("Lightning Bolt").unwrap(),
                    card_name: "Lightning Bolt".to_owned(),
                    count: 4,
                },
            ],
            sideboard: vec![MtgoCompetitiveDeckCardCountV1 {
                card_db_id: mtg_kernel::card_def::card_id_by_name("Searing Blaze").unwrap(),
                card_name: "Searing Blaze".to_owned(),
                count: 15,
            }],
        };
        configuration.mainboard.sort_by_key(|card| card.card_db_id);
        validate_competitive_deck_manifest_v1(MtgoCompetitiveDeckManifestV1 {
            schema_version: MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
            deck_list_sha256: "a".repeat(64),
            format_sha256: "1".repeat(64),
            starting_mainboard_count: 60,
            starting_sideboard_count: 15,
            configuration,
        })
        .unwrap()
    }

    fn competitive_native_pregame_runtime_and_deck_v1() -> (
        MtgoCompetitiveEventRuntimeCommitmentsV1,
        ValidatedMtgoCompetitiveDeckManifestV1,
    ) {
        let deck_manifest = competitive_native_pregame_deck_manifest_v1();
        let mut runtime = competitive_event_runtime_commitments_fixture_v1(
            MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
        );
        runtime.deck_manifest_sha256 = deck_manifest.manifest_commitment_sha256().to_owned();
        runtime.player_known_current_deck_configuration_commitment_sha256 =
            competitive_native_sideboard_configuration_commitment_v1(
                &visible_native_sideboard_configuration_v1(deck_manifest.configuration_v1())
                    .unwrap(),
            )
            .unwrap();
        (runtime, deck_manifest)
    }

    fn competitive_pregame_observation_fixture_v1(
        runtime: &MtgoCompetitiveEventRuntimeCommitmentsV1,
        stage: MtgoCompetitivePregameStageV1,
        frame_sequence: u64,
    ) -> MtgoCompetitivePregameObservationCommitmentsV1 {
        let mut value = MtgoCompetitivePregameObservationCommitmentsV1 {
            observation_commitment_sha256: String::new(),
            source_capture_commitment_sha256: format!("{:064x}", frame_sequence + 100),
            duel_perception_profile_commitment_sha256: "1".repeat(64),
            duel_perception_profile_admission_commitment_sha256: "2".repeat(64),
            classifier_runtime_commitment_sha256: "3".repeat(64),
            pregame_evaluation_commitment_sha256: "4".repeat(64),
            pregame_profile_admission_commitment_sha256: "5".repeat(64),
            pregame_classification_commitment_sha256: format!("{:064x}", frame_sequence + 200),
            visible_interaction_commitment_sha256: format!("{:064x}", frame_sequence + 300),
            process_continuity_commitment_sha256: "6".repeat(64),
            window_continuity_commitment_sha256: "7".repeat(64),
            approved_account_alias_sha256: runtime.approved_account_alias_sha256.clone(),
            entry_authorization_sha256: runtime.entry_authorization_sha256.clone(),
            event_identity_sha256: runtime.bound_event_identity_sha256.clone(),
            match_identity_sha256: runtime.current_match_identity_sha256.clone().unwrap(),
            event_kind: runtime.event_kind,
            game_number: runtime.current_game_number.unwrap(),
            stage,
            frame_id: frame_sequence + 1_000,
            frame_sequence,
            captured_at_unix_millis: u128::from(frame_sequence) * 10,
        };
        value.observation_commitment_sha256 =
            competitive_pregame_observation_commitment_v1(&value).unwrap();
        value
    }

    fn reseal_competitive_pregame_observation_v1(
        value: &mut MtgoCompetitivePregameObservationCommitmentsV1,
    ) {
        value.observation_commitment_sha256 =
            competitive_pregame_observation_commitment_v1(value).unwrap();
    }

    fn ratified_match_launch_for_event_runtime_v1(
        runtime: &MtgoCompetitiveEventRuntimeCommitmentsV1,
        valid_from_frame_sequence: u64,
    ) -> RatifiedMtgoCompetitiveMatchLaunchV1 {
        let authorization = MtgoCompetitiveMatchGameplayAuthorizationV1 {
            schema_version: MTGO_COMPETITIVE_MATCH_GAMEPLAY_AUTHORIZATION_SCHEMA_V1,
            account_alias_sha256: runtime.approved_account_alias_sha256.clone(),
            written_permission_sha256: runtime.correspondence_sha256.clone(),
            event_kind: runtime.event_kind,
            event_identity_sha256: runtime.bound_event_identity_sha256.clone(),
            match_identity_sha256: runtime.current_match_identity_sha256.clone().unwrap(),
            game_number: runtime.current_game_number.unwrap(),
            entry_authorization_sha256: runtime.entry_authorization_sha256.clone(),
            owner_launch_authorization_sha256: "b".repeat(64),
            exact_match_gameplay_authorized: true,
            valid_through_frame_sequence: valid_from_frame_sequence + 512,
        };
        let gameplay_authorization_commitment_sha256 =
            competitive_match_gameplay_authorization_commitment_v1(&authorization).unwrap();
        RatifiedMtgoCompetitiveMatchLaunchV1 {
            authorization,
            mode_authorization_commitment_sha256: runtime
                .mode_authorization_commitment_sha256
                .clone(),
            gameplay_authorization_commitment_sha256,
            launch_authorization_commitment_sha256: "c".repeat(64),
            valid_from_frame_sequence,
        }
    }

    #[allow(clippy::type_complexity)]
    fn selected_listing_entry_bridge_parts_v1(
        event_kind: MtgoCompetitiveEventKindV1,
        resource: MtgoCompetitiveEntryResourceV1,
        amount: u32,
    ) -> (
        MtgoReviewedCompetitiveOpenEntryReviewRatificationCandidateV1,
        MtgoConfirmedCompetitiveOpenEntryReviewCommitmentsV1,
        MtgoCompetitiveEventListingOpenVisibleConfirmationCommitmentsV1,
        MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1,
        String,
        MtgoCompetitiveEntryFrameTransitionViewV1,
        MtgoReviewedCompetitiveEntryRatificationCandidateV1,
    ) {
        let (correspondence, source, source_identity, entry_authorization, review) =
            competitive_entry_ratification_parts_v1(event_kind, resource, amount);
        let entry = competitive_entry_ratification_candidate_from_parts_v1(
            &correspondence,
            "UnbuckledPie",
            &source,
            &source_identity,
            &entry_authorization,
            &review,
        )
        .unwrap();
        let open_authorization = MtgoReviewedCompetitiveOpenEntryReviewRatificationCandidateV1 {
            correspondence_sha256: entry.correspondence_sha256.clone(),
            permission_review_commitment_sha256: entry.permission_review_commitment_sha256.clone(),
            mode_authorization_commitment_sha256: entry
                .mode_authorization_commitment_sha256
                .clone(),
            approved_account_alias_sha256: entry.account_alias_sha256.clone(),
            navigation_profile_commitment_sha256: "6".repeat(64),
            navigation_profile_admission_commitment_sha256: "7".repeat(64),
            listing_evaluation_ratification_commitment_sha256: "8".repeat(64),
            listing_evaluation_admission_commitment_sha256: "9".repeat(64),
            deck_list_sha256: "a".repeat(64),
            deck_manifest_commitment_sha256: entry.deck_manifest_sha256.clone(),
            deck_format_sha256: entry.deck_format_sha256.clone(),
            policy_deployment_commitment_sha256: entry.policy_deployment_commitment_sha256.clone(),
            open_review_scope_commitment_sha256: "b".repeat(64),
            event_kind,
            ratification_commitment_sha256: "c".repeat(64),
        };
        let open_visible = MtgoCompetitiveEventListingOpenVisibleConfirmationCommitmentsV1 {
            source_preparation_commitment_sha256: "d".repeat(64),
            arrival_commitment_sha256: "e".repeat(64),
            after_capture_commitment_sha256: "5".repeat(64),
            after_classification_result_commitment_sha256: "f".repeat(64),
            after_lifecycle_snapshot_commitment_sha256: "0".repeat(64),
            visible_confirmation_commitment_sha256: "1".repeat(64),
            event_kind,
            event_identity_sha256: entry.event_identity_sha256.clone(),
            after_frame_id: 16,
            after_frame_sequence: 40,
            after_captured_at_unix_millis: 400,
        };
        let open_confirmation = MtgoConfirmedCompetitiveOpenEntryReviewCommitmentsV1 {
            input_receipt_sha256: "2".repeat(64),
            preparation_commitment_sha256: "3".repeat(64),
            authorization_ratification_commitment_sha256: open_authorization
                .ratification_commitment_sha256
                .clone(),
            visible_confirmation: open_visible.clone(),
            confirmation_receipt_sha256: "4".repeat(64),
            event_kind,
            event_identity_sha256: entry.event_identity_sha256.clone(),
            after_frame_id: open_visible.after_frame_id,
            after_frame_sequence: open_visible.after_frame_sequence,
            after_captured_at_unix_millis: open_visible.after_captured_at_unix_millis,
        };
        let open_after_navigation = MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1 {
            source_frame: crate::probe::MtgoAdmittedCompetitiveNavigationFrameCommitmentsV1 {
                profile_commitment_sha256: open_authorization
                    .navigation_profile_commitment_sha256
                    .clone(),
                profile_admission_commitment_sha256: open_authorization
                    .navigation_profile_admission_commitment_sha256
                    .clone(),
                approved_account_alias_sha256: open_authorization
                    .approved_account_alias_sha256
                    .clone(),
                frame_profile_binding_sha256: "2".repeat(64),
                source_capture: crate::probe::MtgoDxgiFrameCommitmentsV3 {
                    capture_commitment_sha256: open_visible.after_capture_commitment_sha256.clone(),
                    canonical_bgra8_sha256: "3".repeat(64),
                    preview_png_sha256: "4".repeat(64),
                    canonical_width: 1240,
                    canonical_height: 740,
                    client_rect_desktop_px: crate::SignedRectV1 {
                        left: 100,
                        top: 100,
                        right: 1340,
                        bottom: 840,
                    },
                    captured_at_unix_millis: open_visible.after_captured_at_unix_millis,
                },
            },
            runtime_identity_commitment_sha256: "8".repeat(64),
            request_commitment_sha256: "6".repeat(64),
            classifier_response_sha256: "7".repeat(64),
            lifecycle_snapshot_commitment_sha256: open_visible
                .after_lifecycle_snapshot_commitment_sha256
                .clone(),
            prediction_commitment_sha256: "9".repeat(64),
            classification_result_commitment_sha256: open_visible
                .after_classification_result_commitment_sha256
                .clone(),
            event_kind,
            phase: MtgoCompetitiveLifecyclePhaseV1::EntryReview,
            frame_id: open_visible.after_frame_id,
            frame_sequence: open_visible.after_frame_sequence,
        };
        let window_continuity_commitment_sha256 = "a".repeat(64);
        let entry_review_lineage = MtgoCompetitiveEntryFrameTransitionViewV1 {
            navigation_profile_commitment_sha256: open_authorization
                .navigation_profile_commitment_sha256
                .clone(),
            navigation_profile_admission_commitment_sha256: open_authorization
                .navigation_profile_admission_commitment_sha256
                .clone(),
            approved_account_alias_sha256: open_authorization.approved_account_alias_sha256.clone(),
            runtime_identity_commitment_sha256: open_after_navigation
                .runtime_identity_commitment_sha256
                .clone(),
            window_continuity_commitment_sha256: window_continuity_commitment_sha256.clone(),
            source_identity_commitment_sha256: Some(
                entry.source_identity_commitment_sha256.clone(),
            ),
            capture_commitment_sha256: entry.source_capture_commitment_sha256.clone(),
            canonical_bgra8_sha256: "b".repeat(64),
            classification_result_commitment_sha256: entry
                .source_navigation_classification_result_commitment_sha256
                .clone(),
            lifecycle_snapshot_commitment_sha256: source.snapshot_commitment_sha256().to_owned(),
            event_kind,
            phase: MtgoCompetitiveLifecyclePhaseV1::EntryReview,
            event_identity_sha256: entry.event_identity_sha256.clone(),
            frame_id: source.frame_id_v1(),
            frame_sequence: source.frame_sequence(),
            captured_at_unix_millis: 410,
        };
        (
            open_authorization,
            open_confirmation,
            open_visible,
            open_after_navigation,
            window_continuity_commitment_sha256,
            entry_review_lineage,
            entry,
        )
    }

    fn ratified_competitive_pass_v1(
        event_kind: MtgoCompetitiveEventKindV1,
    ) -> RatifiedMtgoCompetitiveDuelPassAuthorizationV1 {
        let scope = competitive_scope_v1("UnbuckledPie", event_kind);
        let mode_commitment =
            validate_competitive_duel_pass_authorization_v1(&scope, "UnbuckledPie", event_kind)
                .unwrap();
        let expected = competitive_duel_pass_authorization_commitment_v1(
            &scope,
            "UnbuckledPie",
            event_kind,
            &mode_commitment,
        );
        ratify_competitive_duel_pass_authorization_with_commitment_v1(
            scope,
            "UnbuckledPie".to_owned(),
            event_kind,
            Some(&expected),
        )
        .unwrap()
    }

    fn competitive_match_authorization_v1(
        event_kind: MtgoCompetitiveEventKindV1,
        game_number: u8,
    ) -> MtgoCompetitiveMatchGameplayAuthorizationV1 {
        MtgoCompetitiveMatchGameplayAuthorizationV1 {
            schema_version: MTGO_COMPETITIVE_MATCH_GAMEPLAY_AUTHORIZATION_SCHEMA_V1,
            account_alias_sha256: format!("{:x}", Sha256::digest("UnbuckledPie".as_bytes())),
            written_permission_sha256: "b".repeat(64),
            event_kind,
            event_identity_sha256: "c".repeat(64),
            match_identity_sha256: "d".repeat(64),
            game_number,
            entry_authorization_sha256: "e".repeat(64),
            owner_launch_authorization_sha256: "f".repeat(64),
            exact_match_gameplay_authorized: true,
            valid_through_frame_sequence: 100,
        }
    }

    fn attended_match_launch_request_v4(
        event_kind: MtgoCompetitiveEventKindV1,
        game_number: u8,
        observed_frame_sequence: u64,
    ) -> MtgoAttendedCompetitiveMatchLaunchRequestV4 {
        MtgoAttendedCompetitiveMatchLaunchRequestV4 {
            schema_version: MTGO_ATTENDED_COMPETITIVE_MATCH_LAUNCH_REQUEST_SCHEMA_V4,
            event_kind,
            event_display_label: "Modern League".to_owned(),
            opponent_display_name: "VisibleOpponent".to_owned(),
            visible_match_id: "123456".to_owned(),
            visible_game_id: "789012".to_owned(),
            event_identity_sha256: "c".repeat(64),
            match_identity_sha256: "d".repeat(64),
            game_number,
            entry_authorization_sha256: "e".repeat(64),
            observed_frame_sequence,
            source_capture_commitment_sha256: "1".repeat(64),
            source_perception_result_commitment_sha256: "2".repeat(64),
            source_lifecycle_snapshot_commitment_sha256: "3".repeat(64),
            source_window_title_sha256: "4".repeat(64),
            source_event_label_region_sha256: "5".repeat(64),
            source_launch_identity_commitment_sha256: "6".repeat(64),
        }
    }

    fn ratified_match_launch_v1(
        event_kind: MtgoCompetitiveEventKindV1,
        game_number: u8,
    ) -> RatifiedMtgoCompetitiveMatchLaunchV1 {
        let scope = competitive_scope_v1("UnbuckledPie", event_kind);
        let authorization = competitive_match_authorization_v1(event_kind, game_number);
        let mode_commitment =
            validate_competitive_duel_pass_authorization_v1(&scope, "UnbuckledPie", event_kind)
                .unwrap();
        let gameplay_commitment =
            competitive_match_gameplay_authorization_commitment_v1(&authorization).unwrap();
        let expected = competitive_match_launch_commitment_v1(
            "UnbuckledPie",
            &mode_commitment,
            &gameplay_commitment,
            &authorization,
        );
        ratify_competitive_match_launch_with_commitment_v1(
            &scope,
            "UnbuckledPie",
            authorization,
            Some(&expected),
        )
        .unwrap()
    }

    #[test]
    fn private_match_authorization_binds_the_visible_account() {
        let scope = authorized_scope_v3("UnbuckledPie");
        validate_private_match_authorization_v3(&scope, "UnbuckledPie").unwrap();
        assert!(validate_private_match_authorization_v3(&scope, "another-account").is_err());

        let mut no_private_input = scope.clone();
        no_private_input.private_match_input = false;
        assert!(
            validate_private_match_authorization_v3(&no_private_input, "UnbuckledPie").is_err()
        );

        let mut hidden = scope;
        hidden.visible_channels_only = false;
        assert!(validate_private_match_authorization_v3(&hidden, "UnbuckledPie").is_err());
    }

    #[test]
    fn production_permission_ratification_is_empty_and_scope_exact() {
        let scope = authorized_scope_v3("UnbuckledPie");
        assert!(
            ratify_private_match_authorization_v3(scope.clone(), "UnbuckledPie".to_owned())
                .is_err()
        );

        let baseline = private_match_authorization_commitment_v3(&scope, "UnbuckledPie");
        let mut broader = scope.clone();
        broader.league_input = true;
        assert_ne!(
            baseline,
            private_match_authorization_commitment_v3(&broader, "UnbuckledPie")
        );
        assert!(ratify_private_match_authorization_v3(broader, "UnbuckledPie".to_owned()).is_err());

        let expected = private_match_authorization_commitment_v3(&scope, "UnbuckledPie");
        let ratified = ratify_private_match_authorization_with_commitment_v3(
            scope,
            "UnbuckledPie".to_owned(),
            Some(&expected),
        )
        .unwrap();
        assert_eq!(ratified.authorization_commitment_sha256_v3(), expected);
    }

    #[test]
    fn production_competitive_pass_ratification_is_empty_but_exact_modes_can_be_reviewed() {
        for event_kind in [
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ] {
            let scope = competitive_scope_v1("UnbuckledPie", event_kind);
            assert!(ratify_competitive_duel_pass_authorization_v1(
                scope.clone(),
                "UnbuckledPie".to_owned(),
                event_kind,
            )
            .is_err());

            let mode_commitment =
                validate_competitive_duel_pass_authorization_v1(&scope, "UnbuckledPie", event_kind)
                    .unwrap();
            let expected = competitive_duel_pass_authorization_commitment_v1(
                &scope,
                "UnbuckledPie",
                event_kind,
                &mode_commitment,
            );
            let ratified = ratify_competitive_duel_pass_authorization_with_commitment_v1(
                scope,
                "UnbuckledPie".to_owned(),
                event_kind,
                Some(&expected),
            )
            .unwrap();
            assert_eq!(ratified.event_kind_v1(), event_kind);
            assert_eq!(
                ratified.mode_authorization_commitment_sha256_v1(),
                mode_commitment
            );
            assert_eq!(ratified.authorization_commitment_sha256_v1(), expected);
            assert!(!ratified.safe_for_input_v1());
            assert!(!ratified.permits_event_entry_v1());
        }
    }

    #[test]
    fn reviewed_correspondence_ratification_binds_exact_review_and_one_mode() {
        for event_kind in [
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ] {
            let correspondence = checked_competitive_correspondence_v2();
            let review_commitment = correspondence.review_commitment_sha256().to_owned();
            let scope = correspondence
                .checked_untrusted_scope_for_mode_v1(event_kind)
                .unwrap();
            let mode_commitment =
                validate_competitive_duel_pass_authorization_v1(&scope, "UnbuckledPie", event_kind)
                    .unwrap();
            let expected = competitive_duel_pass_authorization_from_review_commitment_v2(
                &scope,
                "UnbuckledPie",
                event_kind,
                &mode_commitment,
                &review_commitment,
            );
            let candidate =
                review_competitive_duel_pass_ratification_candidate_from_correspondence_v2(
                    &correspondence,
                    "UnbuckledPie",
                    event_kind,
                )
                .unwrap();
            assert_eq!(
                candidate.permission_review_commitment_sha256,
                review_commitment
            );
            assert_eq!(
                candidate.mode_authorization_commitment_sha256,
                mode_commitment
            );
            assert_eq!(candidate.ratification_commitment_sha256, expected);
            assert!(!candidate.safe_for_live_input_v2());
            assert!(!candidate.permits_event_entry_v2());
            assert!(!candidate.permits_spending_v2());
            let ratified =
                ratify_competitive_duel_pass_authorization_from_correspondence_with_commitment_v2(
                    correspondence,
                    "UnbuckledPie".to_owned(),
                    event_kind,
                    Some(&expected),
                )
                .unwrap();
            assert_eq!(ratified.event_kind_v1(), event_kind);
            assert_eq!(
                ratified.permission_review_commitment_sha256_v2(),
                Some(review_commitment.as_str())
            );
            assert_eq!(
                ratified.mode_authorization_commitment_sha256_v1(),
                mode_commitment
            );
            assert_ne!(
                ratified.authorization_commitment_sha256_v1(),
                competitive_duel_pass_authorization_commitment_v1(
                    &scope,
                    "UnbuckledPie",
                    event_kind,
                    &mode_commitment,
                )
            );
            assert!(!ratified.permits_event_entry_v1());
            assert!(!ratified.safe_for_input_v1());
        }

        assert!(
            ratify_competitive_duel_pass_authorization_from_correspondence_v2(
                checked_competitive_correspondence_v2(),
                "UnbuckledPie".to_owned(),
                MtgoCompetitiveEventKindV1::League,
            )
            .is_err()
        );
        assert!(
            review_competitive_duel_pass_ratification_candidate_from_correspondence_v2(
                &checked_competitive_correspondence_v2(),
                "DifferentAccount",
                MtgoCompetitiveEventKindV1::League,
            )
            .is_err()
        );
    }

    #[test]
    fn reviewed_gesture_permission_binds_all_families_profile_and_one_mode() {
        let families = canonical_duel_gesture_action_families_v1();
        let facts = CompetitiveDuelGestureProfileFactsV1 {
            evaluation_commitment_sha256: &"1".repeat(64),
            admission_commitment_sha256: &"2".repeat(64),
            runtime_binary_sha256: &"3".repeat(64),
            assets_manifest_sha256: &"4".repeat(64),
            supported_action_families: &families,
        };
        let mut commitments = Vec::new();
        for event_kind in [
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ] {
            let candidate = competitive_duel_gesture_ratification_candidate_from_parts_v1(
                &checked_competitive_correspondence_v2(),
                "UnbuckledPie",
                event_kind,
                &facts,
            )
            .unwrap();
            assert_eq!(candidate.event_kind, event_kind);
            assert_eq!(candidate.supported_action_families, families);
            assert_eq!(
                candidate.gesture_evaluation_commitment_sha256,
                facts.evaluation_commitment_sha256
            );
            assert_eq!(
                candidate.gesture_profile_admission_commitment_sha256,
                facts.admission_commitment_sha256
            );
            assert!(!candidate.safe_for_live_input_v1());
            assert!(!candidate.permits_event_entry_v1());
            assert!(!candidate.permits_spending_v1());
            commitments.push(candidate.ratification_commitment_sha256);
        }
        assert_ne!(commitments[0], commitments[1]);
        assert!(
            RATIFIED_COMPETITIVE_DUEL_GESTURE_AUTHORIZATION_FROM_REVIEW_COMMITMENT_V1.is_none()
        );

        let incomplete = &families[..families.len() - 1];
        let incomplete_facts = CompetitiveDuelGestureProfileFactsV1 {
            supported_action_families: incomplete,
            ..facts
        };
        assert!(
            competitive_duel_gesture_ratification_candidate_from_parts_v1(
                &checked_competitive_correspondence_v2(),
                "UnbuckledPie",
                MtgoCompetitiveEventKindV1::League,
                &incomplete_facts,
            )
            .is_err()
        );
        let duplicate_hash_facts = CompetitiveDuelGestureProfileFactsV1 {
            admission_commitment_sha256: facts.evaluation_commitment_sha256,
            ..facts
        };
        assert!(
            competitive_duel_gesture_ratification_candidate_from_parts_v1(
                &checked_competitive_correspondence_v2(),
                "UnbuckledPie",
                MtgoCompetitiveEventKindV1::League,
                &duplicate_hash_facts,
            )
            .is_err()
        );
        assert!(
            competitive_duel_gesture_ratification_candidate_from_parts_v1(
                &checked_competitive_correspondence_v2(),
                "DifferentAccount",
                MtgoCompetitiveEventKindV1::League,
                &facts,
            )
            .is_err()
        );
    }

    #[test]
    fn attended_gesture_launch_extension_binds_exact_pass_game_and_profile() {
        let families = canonical_duel_gesture_action_families_v1();
        let evaluation = "1".repeat(64);
        let admission = "2".repeat(64);
        let runtime = "3".repeat(64);
        let assets = "4".repeat(64);
        let facts = CompetitiveDuelGestureProfileFactsV1 {
            evaluation_commitment_sha256: &evaluation,
            admission_commitment_sha256: &admission,
            runtime_binary_sha256: &runtime,
            assets_manifest_sha256: &assets,
            supported_action_families: &families,
        };
        let correspondence = checked_competitive_correspondence_v2();
        let candidate = competitive_duel_gesture_ratification_candidate_from_parts_v1(
            &correspondence,
            "UnbuckledPie",
            MtgoCompetitiveEventKindV1::League,
            &facts,
        )
        .unwrap();
        let mut authorization =
            competitive_match_authorization_v1(MtgoCompetitiveEventKindV1::League, 2);
        authorization.account_alias_sha256 = candidate.account_alias_sha256.clone();
        authorization.written_permission_sha256 = candidate.correspondence_sha256.clone();
        let gameplay_authorization_commitment_sha256 =
            competitive_match_gameplay_authorization_commitment_v1(&authorization).unwrap();
        let launch_authorization_commitment_sha256 = competitive_match_launch_commitment_v1(
            "UnbuckledPie",
            &candidate.mode_authorization_commitment_sha256,
            &gameplay_authorization_commitment_sha256,
            &authorization,
        );
        let pass_match_launch = RatifiedMtgoCompetitiveMatchLaunchV1 {
            authorization,
            mode_authorization_commitment_sha256: candidate
                .mode_authorization_commitment_sha256
                .clone(),
            gameplay_authorization_commitment_sha256,
            launch_authorization_commitment_sha256,
            valid_from_frame_sequence: 40,
        };
        let nonce = [0x2au8; 8];
        let phrase = attended_competitive_gesture_match_launch_confirmation_phrase_v1(
            MtgoCompetitiveEventKindV1::League,
            2,
            &nonce,
        );
        assert_eq!(
            phrase,
            "AUTHORIZE MTGO LEAGUE GAME 2 ALL ELEVEN GESTURES 2A2A2A2A2A2A2A2A"
        );
        let extension = competitive_gesture_match_launch_commitment_from_facts_v1(
            &candidate,
            &pass_match_launch,
            "UnbuckledPie",
            &nonce,
            1_777,
            &phrase,
        )
        .unwrap();
        assert_eq!(extension.len(), 64);
        assert!(competitive_gesture_match_launch_commitment_from_facts_v1(
            &candidate,
            &pass_match_launch,
            "UnbuckledPie",
            &nonce,
            1_777,
            "AUTHORIZE SOMETHING ELSE",
        )
        .is_err());
        let wrong_game_phrase = attended_competitive_gesture_match_launch_confirmation_phrase_v1(
            MtgoCompetitiveEventKindV1::League,
            1,
            &nonce,
        );
        assert!(competitive_gesture_match_launch_commitment_from_facts_v1(
            &candidate,
            &pass_match_launch,
            "UnbuckledPie",
            &nonce,
            1_777,
            &wrong_game_phrase,
        )
        .is_err());
        assert!(competitive_gesture_match_launch_commitment_from_facts_v1(
            &candidate,
            &pass_match_launch,
            "UnbuckledPie",
            &nonce,
            0,
            &phrase,
        )
        .is_err());
        let session = initial_competitive_gesture_game_session_commitment_v1(
            &candidate,
            &pass_match_launch,
            &extension,
        );
        assert_eq!(session.len(), 64);
        assert_ne!(session, extension);

        let mut wrong_mode = candidate.clone();
        wrong_mode.event_kind = MtgoCompetitiveEventKindV1::Challenge;
        assert!(validate_competitive_gesture_match_launch_facts_v1(
            &wrong_mode,
            &pass_match_launch,
            "UnbuckledPie"
        )
        .is_err());
        let mut incomplete = candidate.clone();
        incomplete.supported_action_families.pop();
        assert!(validate_competitive_gesture_match_launch_facts_v1(
            &incomplete,
            &pass_match_launch,
            "UnbuckledPie"
        )
        .is_err());
        assert!(validate_competitive_gesture_match_launch_facts_v1(
            &candidate,
            &pass_match_launch,
            "DifferentAccount"
        )
        .is_err());
        let mut invalid_lifetime = pass_match_launch;
        invalid_lifetime.valid_from_frame_sequence = 0;
        assert!(validate_competitive_gesture_match_launch_facts_v1(
            &candidate,
            &invalid_lifetime,
            "UnbuckledPie"
        )
        .is_err());
    }

    #[test]
    fn all_family_session_binds_only_matching_source_gesture_sequence() {
        let session = MtgoCompetitiveGestureGameSessionCommitmentsV1 {
            session_commitment_sha256: "1".repeat(64),
            general_gesture_permission_commitment_sha256: "2".repeat(64),
            mode_authorization_commitment_sha256: "3".repeat(64),
            correspondence_sha256: "c".repeat(64),
            permission_review_commitment_sha256: "d".repeat(64),
            pass_match_launch_commitment_sha256: "4".repeat(64),
            match_gameplay_authorization_commitment_sha256: "5".repeat(64),
            gesture_match_launch_commitment_sha256: "6".repeat(64),
            gesture_evaluation_commitment_sha256: "7".repeat(64),
            gesture_profile_admission_commitment_sha256: "8".repeat(64),
            entry_ratification_commitment_sha256: Some("e".repeat(64)),
            selected_deck_label_sha256: Some("f".repeat(64)),
            selected_deck_region_sha256: Some("0".repeat(64)),
            deck_manifest_sha256: Some("1".repeat(64)),
            deck_format_sha256: Some("2".repeat(64)),
            policy_deployment_commitment_sha256: Some("3".repeat(64)),
            event_kind: MtgoCompetitiveEventKindV1::League,
            game_number: 2,
            valid_from_frame_sequence: 40,
            valid_through_frame_sequence: 100,
            last_confirmed_frame_sequence: 39,
            confirmed_action_count: 0,
        };
        let sequence = MtgoOpaqueCompetitiveDuelGestureSequenceCommitmentsV1 {
            competitive_action_plan_commitment_sha256: "9".repeat(64),
            gesture_plan_commitment_sha256: "a".repeat(64),
            competitive_mode_authorization_commitment_sha256: session
                .mode_authorization_commitment_sha256
                .clone(),
            competitive_match_gameplay_authorization_commitment_sha256: session
                .match_gameplay_authorization_commitment_sha256
                .clone(),
            policy_deployment_commitment_sha256: session
                .policy_deployment_commitment_sha256
                .clone()
                .unwrap(),
            current_stage_binding_commitment_sha256: "b".repeat(64),
            current_opaque_stage_commitment_sha256: "c".repeat(64),
            last_visible_transition_commitment_sha256: None,
            sequence_commitment_sha256: "d".repeat(64),
            selected_action_family: MtgoDuelActionFamilyV1::PlayLand,
            event_kind: MtgoCompetitiveEventKindV1::League,
            game_number: 2,
            gameplay_authorization_valid_through_frame_sequence: 100,
            current_stage_index: 0,
            gesture_stage_count: 1,
            observed_stage_count: 1,
            current_frame_id: 12,
            current_frame_sequence: 40,
        };
        let bound =
            competitive_duel_gesture_sequence_session_binding_from_parts_v1(&sequence, &session)
                .unwrap();
        assert_eq!(
            bound.selected_action_family,
            MtgoDuelActionFamilyV1::PlayLand
        );
        assert_eq!(bound.event_kind, MtgoCompetitiveEventKindV1::League);
        assert_eq!(bound.game_number, 2);
        assert_eq!(bound.source_frame_sequence, 40);
        assert_eq!(bound.gesture_stage_count, 1);
        assert_eq!(bound.binding_commitment_sha256.len(), 64);
        assert_eq!(
            bound.deck_manifest_sha256,
            session.deck_manifest_sha256.as_deref().unwrap()
        );

        let mut unbound = session.clone();
        unbound.entry_ratification_commitment_sha256 = None;
        unbound.selected_deck_label_sha256 = None;
        unbound.selected_deck_region_sha256 = None;
        unbound.deck_manifest_sha256 = None;
        unbound.deck_format_sha256 = None;
        unbound.policy_deployment_commitment_sha256 = None;
        assert!(
            competitive_duel_gesture_sequence_session_binding_from_parts_v1(&sequence, &unbound,)
                .is_err()
        );

        let mut wrong_mode = sequence.clone();
        wrong_mode.competitive_mode_authorization_commitment_sha256 = "e".repeat(64);
        assert!(
            competitive_duel_gesture_sequence_session_binding_from_parts_v1(&wrong_mode, &session)
                .is_err()
        );
        let mut wrong_policy = sequence.clone();
        wrong_policy.policy_deployment_commitment_sha256 = "4".repeat(64);
        assert!(
            competitive_duel_gesture_sequence_session_binding_from_parts_v1(
                &wrong_policy,
                &session,
            )
            .is_err()
        );
        let mut wrong_game = sequence.clone();
        wrong_game.game_number = 1;
        assert!(
            competitive_duel_gesture_sequence_session_binding_from_parts_v1(&wrong_game, &session)
                .is_err()
        );
        let mut stale = sequence.clone();
        stale.current_frame_sequence = 39;
        assert!(
            competitive_duel_gesture_sequence_session_binding_from_parts_v1(&stale, &session)
                .is_err()
        );
        let mut continuation = sequence.clone();
        continuation.current_stage_index = 1;
        continuation.observed_stage_count = 2;
        continuation.last_visible_transition_commitment_sha256 = Some("e".repeat(64));
        assert!(
            competitive_duel_gesture_sequence_session_binding_from_parts_v1(
                &continuation,
                &session
            )
            .is_err()
        );
        let mut wrong_lifetime = sequence;
        wrong_lifetime.gameplay_authorization_valid_through_frame_sequence = 99;
        assert!(
            competitive_duel_gesture_sequence_session_binding_from_parts_v1(
                &wrong_lifetime,
                &session
            )
            .is_err()
        );
    }

    #[test]
    fn session_bound_gesture_preparation_requires_exact_next_frame_and_authority() {
        let session = MtgoCompetitiveGestureGameSessionCommitmentsV1 {
            session_commitment_sha256: "1".repeat(64),
            general_gesture_permission_commitment_sha256: "2".repeat(64),
            mode_authorization_commitment_sha256: "3".repeat(64),
            correspondence_sha256: "c".repeat(64),
            permission_review_commitment_sha256: "d".repeat(64),
            pass_match_launch_commitment_sha256: "4".repeat(64),
            match_gameplay_authorization_commitment_sha256: "5".repeat(64),
            gesture_match_launch_commitment_sha256: "6".repeat(64),
            gesture_evaluation_commitment_sha256: "7".repeat(64),
            gesture_profile_admission_commitment_sha256: "8".repeat(64),
            entry_ratification_commitment_sha256: Some("e".repeat(64)),
            selected_deck_label_sha256: Some("f".repeat(64)),
            selected_deck_region_sha256: Some("0".repeat(64)),
            deck_manifest_sha256: Some("1".repeat(64)),
            deck_format_sha256: Some("2".repeat(64)),
            policy_deployment_commitment_sha256: Some("3".repeat(64)),
            event_kind: MtgoCompetitiveEventKindV1::Challenge,
            game_number: 1,
            valid_from_frame_sequence: 90,
            valid_through_frame_sequence: 120,
            last_confirmed_frame_sequence: 89,
            confirmed_action_count: 0,
        };
        let sequence = MtgoOpaqueCompetitiveDuelGestureSequenceCommitmentsV1 {
            competitive_action_plan_commitment_sha256: "9".repeat(64),
            gesture_plan_commitment_sha256: "a".repeat(64),
            competitive_mode_authorization_commitment_sha256: session
                .mode_authorization_commitment_sha256
                .clone(),
            competitive_match_gameplay_authorization_commitment_sha256: session
                .match_gameplay_authorization_commitment_sha256
                .clone(),
            policy_deployment_commitment_sha256: session
                .policy_deployment_commitment_sha256
                .clone()
                .unwrap(),
            current_stage_binding_commitment_sha256: "b".repeat(64),
            current_opaque_stage_commitment_sha256: "c".repeat(64),
            last_visible_transition_commitment_sha256: None,
            sequence_commitment_sha256: "d".repeat(64),
            selected_action_family: MtgoDuelActionFamilyV1::CastOrPlotSpell,
            event_kind: MtgoCompetitiveEventKindV1::Challenge,
            game_number: 1,
            gameplay_authorization_valid_through_frame_sequence: 120,
            current_stage_index: 0,
            gesture_stage_count: 2,
            observed_stage_count: 1,
            current_frame_id: 90,
            current_frame_sequence: 90,
        };
        let bound =
            competitive_duel_gesture_sequence_session_binding_from_parts_v1(&sequence, &session)
                .unwrap();
        let prepared = MtgoOpaqueCompetitiveDuelGestureSourcePreparationCommitmentsV1 {
            source_sequence_commitment_sha256: sequence.sequence_commitment_sha256.clone(),
            prepared_sequence_commitment_sha256: "6".repeat(64),
            competitive_action_plan_commitment_sha256: sequence
                .competitive_action_plan_commitment_sha256
                .clone(),
            gesture_plan_commitment_sha256: sequence.gesture_plan_commitment_sha256.clone(),
            competitive_mode_authorization_commitment_sha256: session
                .mode_authorization_commitment_sha256
                .clone(),
            competitive_match_gameplay_authorization_commitment_sha256: session
                .match_gameplay_authorization_commitment_sha256
                .clone(),
            fresh_stage_binding_commitment_sha256: "e".repeat(64),
            fresh_capture_commitment_sha256: "f".repeat(64),
            fresh_perception_result_commitment_sha256: "0".repeat(64),
            gesture_target_runtime_identity_commitment_sha256: "3".repeat(64),
            gesture_target_request_commitment_sha256: "4".repeat(64),
            before_input_postcondition_verification_commitment_sha256: "5".repeat(64),
            primitive_commitment_sha256: "1".repeat(64),
            preparation_commitment_sha256: "2".repeat(64),
            selected_action_family: MtgoDuelActionFamilyV1::CastOrPlotSpell,
            event_kind: MtgoCompetitiveEventKindV1::Challenge,
            game_number: 1,
            stage_index: 0,
            gesture_stage_count: 2,
            target_count: 1,
            fresh_frame_id: 91,
            fresh_frame_sequence: 91,
            fresh_captured_at_unix_millis: 1,
            gameplay_authorization_valid_through_frame_sequence: 120,
        };
        let checked =
            competitive_duel_gesture_source_preparation_from_parts_v1(&bound, &session, &prepared)
                .unwrap();
        assert_eq!(checked.fresh_frame_sequence, 91);
        assert_eq!(checked.stage_index, 0);
        assert_eq!(checked.target_count, 1);
        assert_eq!(checked.preparation_binding_commitment_sha256.len(), 64);

        let mut skipped = prepared.clone();
        skipped.fresh_frame_sequence = 92;
        assert!(competitive_duel_gesture_source_preparation_from_parts_v1(
            &bound, &session, &skipped
        )
        .is_err());
        let mut wrong_authority = prepared.clone();
        wrong_authority.competitive_match_gameplay_authorization_commitment_sha256 = "a".repeat(64);
        assert!(competitive_duel_gesture_source_preparation_from_parts_v1(
            &bound,
            &session,
            &wrong_authority
        )
        .is_err());
        let mut continuation = prepared.clone();
        continuation.stage_index = 1;
        assert!(competitive_duel_gesture_source_preparation_from_parts_v1(
            &bound,
            &session,
            &continuation
        )
        .is_err());
        let mut too_many_targets = prepared;
        too_many_targets.target_count = 3;
        assert!(competitive_duel_gesture_source_preparation_from_parts_v1(
            &bound,
            &session,
            &too_many_targets
        )
        .is_err());
    }

    #[test]
    fn one_stage_gesture_receipt_accepts_only_exact_emission_and_transition() {
        let prepared = MtgoPreparedCompetitiveDuelGestureSourceStageCommitmentsV1 {
            preparation_binding_commitment_sha256: "1".repeat(64),
            session_sequence_binding_commitment_sha256: "2".repeat(64),
            game_session_commitment_sha256: "3".repeat(64),
            gesture_match_launch_commitment_sha256: "4".repeat(64),
            source_sequence_commitment_sha256: "5".repeat(64),
            prepared_sequence_commitment_sha256: "0".repeat(64),
            competitive_action_plan_commitment_sha256: "6".repeat(64),
            gesture_plan_commitment_sha256: "7".repeat(64),
            fresh_stage_binding_commitment_sha256: "8".repeat(64),
            fresh_capture_commitment_sha256: "9".repeat(64),
            fresh_perception_result_commitment_sha256: "a".repeat(64),
            gesture_target_runtime_identity_commitment_sha256: "b".repeat(64),
            gesture_target_request_commitment_sha256: "c".repeat(64),
            before_input_postcondition_verification_commitment_sha256: "d".repeat(64),
            primitive_commitment_sha256: "e".repeat(64),
            selected_action_family: MtgoDuelActionFamilyV1::PlayLand,
            event_kind: MtgoCompetitiveEventKindV1::League,
            game_number: 2,
            stage_index: 0,
            gesture_stage_count: 1,
            target_count: 1,
            fresh_frame_id: 41,
            fresh_frame_sequence: 91,
            fresh_captured_at_unix_millis: 1_000,
        };
        let primitive = MtgoDuelGesturePrimitiveV1::ActivatePrimary {
            activation: MtgoDuelPrimaryActivationV1::SingleLeftClick,
        };
        let receipt =
            competitive_duel_gesture_input_receipt_v1(&prepared, &primitive, 1_001, 2, true)
                .unwrap();
        assert_eq!(receipt.len(), 64);
        assert_ne!(
            receipt,
            competitive_duel_gesture_input_receipt_v1(&prepared, &primitive, 1_001, 4, true,)
                .unwrap()
        );
        for invalid_record_count in [0, 1, 3, 5] {
            assert!(competitive_duel_gesture_input_receipt_v1(
                &prepared,
                &primitive,
                1_001,
                invalid_record_count,
                true,
            )
            .is_err());
        }
        let mut continuation = prepared.clone();
        continuation.gesture_stage_count = 2;
        assert!(competitive_duel_gesture_input_receipt_v1(
            &continuation,
            &primitive,
            1_001,
            2,
            true,
        )
        .is_ok());

        let pending = MtgoPendingCompetitiveDuelGesturePrimitiveCommitmentsV1 {
            preparation_binding_commitment_sha256: prepared
                .preparation_binding_commitment_sha256
                .clone(),
            gesture_target_runtime_identity_commitment_sha256: prepared
                .gesture_target_runtime_identity_commitment_sha256
                .clone(),
            gesture_target_request_commitment_sha256: prepared
                .gesture_target_request_commitment_sha256
                .clone(),
            before_input_postcondition_verification_commitment_sha256: prepared
                .before_input_postcondition_verification_commitment_sha256
                .clone(),
            input_receipt_sha256: receipt,
            selected_action_family: prepared.selected_action_family,
            event_kind: prepared.event_kind,
            game_number: prepared.game_number,
            stage_index: prepared.stage_index,
            gesture_stage_count: prepared.gesture_stage_count,
            before_frame_id: prepared.fresh_frame_id,
            before_frame_sequence: prepared.fresh_frame_sequence,
            input_sent_at_unix_millis: 1_001,
            emitted_mouse_record_count: 2,
            cursor_parked_outside_client: true,
        };
        let visible = MtgoOpaqueCompetitiveDuelGestureConfirmationCommitmentsV1 {
            before_input_verification_commitment_sha256: prepared
                .before_input_postcondition_verification_commitment_sha256
                .clone(),
            after_capture_commitment_sha256: "f".repeat(64),
            checked_postcondition_commitment_sha256: "0".repeat(64),
            opaque_confirmation_commitment_sha256: "1".repeat(64),
            selected_action_family: prepared.selected_action_family,
            event_kind: prepared.event_kind,
            game_number: prepared.game_number,
            after_frame_id: 42,
            after_frame_sequence: 92,
            postcondition_candidate_count: 1,
        };
        assert_eq!(
            competitive_duel_gesture_transition_receipt_v1(&pending, &prepared, &visible)
                .unwrap()
                .len(),
            64
        );

        let mut wrong_baseline = visible.clone();
        wrong_baseline.before_input_verification_commitment_sha256 = "2".repeat(64);
        assert!(competitive_duel_gesture_transition_receipt_v1(
            &pending,
            &prepared,
            &wrong_baseline,
        )
        .is_err());
        let mut wrong_family = visible.clone();
        wrong_family.selected_action_family = MtgoDuelActionFamilyV1::PriorityPass;
        assert!(
            competitive_duel_gesture_transition_receipt_v1(&pending, &prepared, &wrong_family,)
                .is_err()
        );
        let mut not_newer = visible;
        not_newer.after_frame_sequence = pending.before_frame_sequence;
        assert!(
            competitive_duel_gesture_transition_receipt_v1(&pending, &prepared, &not_newer,)
                .is_err()
        );
    }

    #[test]
    fn intermediate_gesture_receipt_requires_exact_pending_runtime_and_newer_stage() {
        let prepared = MtgoPreparedCompetitiveDuelGestureSourceStageCommitmentsV1 {
            preparation_binding_commitment_sha256: "1".repeat(64),
            session_sequence_binding_commitment_sha256: "2".repeat(64),
            game_session_commitment_sha256: "3".repeat(64),
            gesture_match_launch_commitment_sha256: "4".repeat(64),
            source_sequence_commitment_sha256: "5".repeat(64),
            prepared_sequence_commitment_sha256: "6".repeat(64),
            competitive_action_plan_commitment_sha256: "7".repeat(64),
            gesture_plan_commitment_sha256: "8".repeat(64),
            fresh_stage_binding_commitment_sha256: "9".repeat(64),
            fresh_capture_commitment_sha256: "a".repeat(64),
            fresh_perception_result_commitment_sha256: "b".repeat(64),
            gesture_target_runtime_identity_commitment_sha256: "c".repeat(64),
            gesture_target_request_commitment_sha256: "d".repeat(64),
            before_input_postcondition_verification_commitment_sha256: "e".repeat(64),
            primitive_commitment_sha256: "f".repeat(64),
            selected_action_family: MtgoDuelActionFamilyV1::CastOrPlotSpell,
            event_kind: MtgoCompetitiveEventKindV1::Challenge,
            game_number: 1,
            stage_index: 0,
            gesture_stage_count: 2,
            target_count: 1,
            fresh_frame_id: 41,
            fresh_frame_sequence: 91,
            fresh_captured_at_unix_millis: 1_000,
        };
        let pending = MtgoPendingCompetitiveDuelGesturePrimitiveCommitmentsV1 {
            preparation_binding_commitment_sha256: prepared
                .preparation_binding_commitment_sha256
                .clone(),
            gesture_target_runtime_identity_commitment_sha256: prepared
                .gesture_target_runtime_identity_commitment_sha256
                .clone(),
            gesture_target_request_commitment_sha256: prepared
                .gesture_target_request_commitment_sha256
                .clone(),
            before_input_postcondition_verification_commitment_sha256: prepared
                .before_input_postcondition_verification_commitment_sha256
                .clone(),
            input_receipt_sha256: "0".repeat(64),
            selected_action_family: prepared.selected_action_family,
            event_kind: prepared.event_kind,
            game_number: prepared.game_number,
            stage_index: prepared.stage_index,
            gesture_stage_count: prepared.gesture_stage_count,
            before_frame_id: prepared.fresh_frame_id,
            before_frame_sequence: prepared.fresh_frame_sequence,
            input_sent_at_unix_millis: 1_001,
            emitted_mouse_record_count: 2,
            cursor_parked_outside_client: true,
        };
        let continuation = MtgoOpaquePinnedCompetitiveDuelGestureContinuationCommitmentsV1 {
            prior_sequence_commitment_sha256: prepared.prepared_sequence_commitment_sha256.clone(),
            advanced_sequence_commitment_sha256: "1".repeat(64),
            visible_transition_commitment_sha256: "2".repeat(64),
            gesture_target_runtime_identity_commitment_sha256: prepared
                .gesture_target_runtime_identity_commitment_sha256
                .clone(),
            gesture_target_request_commitment_sha256: "3".repeat(64),
            continuation_commitment_sha256: "4".repeat(64),
            selected_action_family: prepared.selected_action_family,
            event_kind: prepared.event_kind,
            game_number: prepared.game_number,
            stage_index: 1,
            gesture_stage_count: prepared.gesture_stage_count,
            frame_id: 42,
            frame_sequence: 92,
            captured_at_unix_millis: 1_002,
            target_count: 1,
        };
        assert_eq!(
            competitive_duel_gesture_continuation_receipt_v1(&pending, &prepared, &continuation,)
                .unwrap()
                .len(),
            64
        );

        let mut wrong_prior = continuation.clone();
        wrong_prior.prior_sequence_commitment_sha256 = "5".repeat(64);
        assert!(competitive_duel_gesture_continuation_receipt_v1(
            &pending,
            &prepared,
            &wrong_prior,
        )
        .is_err());
        let mut before_input = continuation.clone();
        before_input.captured_at_unix_millis = 1_001;
        assert!(competitive_duel_gesture_continuation_receipt_v1(
            &pending,
            &prepared,
            &before_input,
        )
        .is_err());
        let mut wrong_runtime = continuation;
        wrong_runtime.gesture_target_runtime_identity_commitment_sha256 = "6".repeat(64);
        assert!(competitive_duel_gesture_continuation_receipt_v1(
            &pending,
            &prepared,
            &wrong_runtime,
        )
        .is_err());
    }

    #[test]
    fn continuation_stage_preparation_requires_a_distinct_exact_next_frame() {
        let session = MtgoCompetitiveGestureGameSessionCommitmentsV1 {
            session_commitment_sha256: "1".repeat(64),
            general_gesture_permission_commitment_sha256: "2".repeat(64),
            mode_authorization_commitment_sha256: "3".repeat(64),
            correspondence_sha256: "c".repeat(64),
            permission_review_commitment_sha256: "d".repeat(64),
            pass_match_launch_commitment_sha256: "4".repeat(64),
            match_gameplay_authorization_commitment_sha256: "5".repeat(64),
            gesture_match_launch_commitment_sha256: "6".repeat(64),
            gesture_evaluation_commitment_sha256: "7".repeat(64),
            gesture_profile_admission_commitment_sha256: "8".repeat(64),
            entry_ratification_commitment_sha256: Some("e".repeat(64)),
            selected_deck_label_sha256: Some("f".repeat(64)),
            selected_deck_region_sha256: Some("0".repeat(64)),
            deck_manifest_sha256: Some("1".repeat(64)),
            deck_format_sha256: Some("2".repeat(64)),
            policy_deployment_commitment_sha256: Some("3".repeat(64)),
            event_kind: MtgoCompetitiveEventKindV1::League,
            game_number: 2,
            valid_from_frame_sequence: 80,
            valid_through_frame_sequence: 120,
            last_confirmed_frame_sequence: 80,
            confirmed_action_count: 0,
        };
        let confirmed = MtgoConfirmedCompetitiveDuelGestureContinuationCommitmentsV1 {
            input_receipt_sha256: "9".repeat(64),
            preparation_binding_commitment_sha256: "a".repeat(64),
            prepared_sequence_commitment_sha256: "b".repeat(64),
            prior_sequence_commitment_sha256: "b".repeat(64),
            advanced_sequence_commitment_sha256: "c".repeat(64),
            visible_transition_commitment_sha256: "d".repeat(64),
            continuation_receipt_sha256: "e".repeat(64),
            gesture_target_runtime_identity_commitment_sha256: "f".repeat(64),
            next_stage_target_request_commitment_sha256: "0".repeat(64),
            unchanged_game_session_commitment_sha256: session.session_commitment_sha256.clone(),
            selected_action_family: MtgoDuelActionFamilyV1::CastOrPlotSpell,
            event_kind: session.event_kind,
            game_number: session.game_number,
            completed_stage_index: 0,
            next_stage_index: 1,
            gesture_stage_count: 2,
            before_frame_id: 41,
            before_frame_sequence: 91,
            after_frame_id: 42,
            after_frame_sequence: 92,
            after_captured_at_unix_millis: 1_001,
            next_stage_target_count: 1,
        };
        let prepared = MtgoOpaqueCompetitiveDuelGestureSourcePreparationCommitmentsV1 {
            source_sequence_commitment_sha256: confirmed
                .advanced_sequence_commitment_sha256
                .clone(),
            prepared_sequence_commitment_sha256: "1".repeat(64),
            competitive_action_plan_commitment_sha256: "2".repeat(64),
            gesture_plan_commitment_sha256: "3".repeat(64),
            competitive_mode_authorization_commitment_sha256: session
                .mode_authorization_commitment_sha256
                .clone(),
            competitive_match_gameplay_authorization_commitment_sha256: session
                .match_gameplay_authorization_commitment_sha256
                .clone(),
            fresh_stage_binding_commitment_sha256: "4".repeat(64),
            fresh_capture_commitment_sha256: "5".repeat(64),
            fresh_perception_result_commitment_sha256: "6".repeat(64),
            gesture_target_runtime_identity_commitment_sha256: confirmed
                .gesture_target_runtime_identity_commitment_sha256
                .clone(),
            gesture_target_request_commitment_sha256: "7".repeat(64),
            before_input_postcondition_verification_commitment_sha256: "8".repeat(64),
            primitive_commitment_sha256: "9".repeat(64),
            preparation_commitment_sha256: "a".repeat(64),
            selected_action_family: confirmed.selected_action_family,
            event_kind: confirmed.event_kind,
            game_number: confirmed.game_number,
            stage_index: confirmed.next_stage_index,
            gesture_stage_count: confirmed.gesture_stage_count,
            target_count: 1,
            fresh_frame_id: 43,
            fresh_frame_sequence: 93,
            fresh_captured_at_unix_millis: 1_002,
            gameplay_authorization_valid_through_frame_sequence: session
                .valid_through_frame_sequence,
        };
        let checked = competitive_duel_gesture_continuation_preparation_from_parts_v1(
            &confirmed, &session, &prepared,
        )
        .unwrap();
        assert_eq!(checked.stage_index, 1);
        assert_eq!(checked.fresh_frame_sequence, 93);
        assert_eq!(checked.preparation_binding_commitment_sha256.len(), 64);

        let mut reused_transition_frame = prepared.clone();
        reused_transition_frame.fresh_frame_id = confirmed.after_frame_id;
        reused_transition_frame.fresh_frame_sequence = confirmed.after_frame_sequence;
        assert!(
            competitive_duel_gesture_continuation_preparation_from_parts_v1(
                &confirmed,
                &session,
                &reused_transition_frame,
            )
            .is_err()
        );
        let mut skipped_frame = prepared.clone();
        skipped_frame.fresh_frame_sequence = 94;
        assert!(
            competitive_duel_gesture_continuation_preparation_from_parts_v1(
                &confirmed,
                &session,
                &skipped_frame,
            )
            .is_err()
        );
        let mut reused_request = prepared;
        reused_request.gesture_target_request_commitment_sha256 = confirmed
            .next_stage_target_request_commitment_sha256
            .clone();
        assert!(
            competitive_duel_gesture_continuation_preparation_from_parts_v1(
                &confirmed,
                &session,
                &reused_request,
            )
            .is_err()
        );
    }

    #[test]
    fn attended_entry_review_binds_both_approved_modes_and_exact_visible_terms() {
        for (event_kind, label, resource, amount) in [
            (
                MtgoCompetitiveEventKindV1::League,
                "Modern League",
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            ),
            (
                MtgoCompetitiveEventKindV1::Challenge,
                "Modern Challenge",
                MtgoCompetitiveEntryResourceV1::ExistingEventTickets,
                25,
            ),
        ] {
            let correspondence = checked_competitive_correspondence_v2();
            let source = checked_competitive_entry_review_snapshot_v1(event_kind, resource, amount);
            let nonce = [0x5au8; 8];
            let phrase = attended_competitive_entry_review_confirmation_phrase_v1(
                event_kind, resource, amount, &nonce,
            );
            let reviewed = review_competitive_entry_from_attended_confirmation_v1(
                &correspondence,
                source,
                "UnbuckledPie",
                label.to_owned(),
                nonce,
                2_000,
                &phrase,
            )
            .unwrap();
            let commitments = reviewed.commitments_v1();
            let entry = reviewed.entry_authorization_record_v1();
            assert_eq!(commitments.event_kind, event_kind);
            assert_eq!(commitments.resource, resource);
            assert_eq!(commitments.amount, amount);
            assert_eq!(commitments.frame_id, 17);
            assert_eq!(commitments.frame_sequence, 41);
            assert_eq!(entry.event_kind, event_kind);
            assert_eq!(entry.entry_terms.resource, resource);
            assert_eq!(entry.entry_terms.amount, amount);
            assert!(entry.exact_entry_authorized);
            assert!(entry.existing_account_resources_only);

            let expected_source =
                checked_competitive_entry_review_snapshot_v1(event_kind, resource, amount);
            let scope = correspondence
                .checked_untrusted_scope_for_mode_v1(event_kind)
                .unwrap();
            let expected_intent = make_offline_competitive_lifecycle_intent_v1(
                &expected_source,
                MtgoCompetitiveLifecycleActionV1::ConfirmEntry,
                &scope,
                Some(&entry),
            )
            .unwrap();
            assert_eq!(
                expected_intent.entry_authorization_sha256.as_deref(),
                Some(commitments.entry_authorization_sha256.as_str())
            );
            assert_eq!(
                commitments.permission_review_commitment_sha256,
                correspondence.review_commitment_sha256()
            );
            assert!(!reviewed.safe_for_live_input_v1());
            assert!(!reviewed.permits_event_entry_v1());
            assert!(!reviewed.permits_spending_v1());
        }
    }

    #[test]
    fn classifier_bound_entry_review_requires_and_binds_exact_classifier_lineage() {
        let missing = entry_source_identity_commitments_v3(None);
        assert!(require_classifier_bound_competitive_entry_source_v3(&missing).is_err());

        let malformed =
            entry_source_identity_commitments_v3(Some("not-a-classifier-digest".to_owned()));
        assert!(require_classifier_bound_competitive_entry_source_v3(&malformed).is_err());

        let classifier = "b".repeat(64);
        let exact = entry_source_identity_commitments_v3(Some(classifier.clone()));
        assert_eq!(
            require_classifier_bound_competitive_entry_source_v3(&exact).unwrap(),
            classifier
        );

        let source_bound = source_bound_entry_review_commitments_v3(classifier.clone());
        let baseline =
            classifier_bound_competitive_entry_review_commitment_v3(&source_bound, &classifier);
        assert_eq!(baseline.len(), 64);

        let different_classifier = "c".repeat(64);
        let changed_classifier =
            source_bound_entry_review_commitments_v3(different_classifier.clone());
        assert_ne!(
            baseline,
            classifier_bound_competitive_entry_review_commitment_v3(
                &changed_classifier,
                &different_classifier,
            )
        );

        let mut changed_terms = source_bound;
        changed_terms.attended_review.amount = 120;
        assert_ne!(
            baseline,
            classifier_bound_competitive_entry_review_commitment_v3(&changed_terms, &classifier,)
        );
    }

    #[test]
    fn control_bound_attended_review_requires_the_exact_enabled_dry_run() {
        let classifier = "b".repeat(64);
        let classifier_bound = classifier_bound_entry_review_commitments_v4(classifier.clone());
        let dry_run = entry_control_dry_run_commitments_v4(classifier);
        let deck_review_receipt_sha256 = "3".repeat(64);
        let baseline = bind_control_bound_competitive_entry_review_commitment_v4(
            &classifier_bound,
            &dry_run,
            &deck_review_receipt_sha256,
        )
        .unwrap();
        assert_eq!(baseline.len(), 64);

        let mut disabled = dry_run.clone();
        disabled.visibly_enabled_confirmed = false;
        assert!(bind_control_bound_competitive_entry_review_commitment_v4(
            &classifier_bound,
            &disabled,
            &deck_review_receipt_sha256,
        )
        .is_err());

        let mut wrong_frame = dry_run.clone();
        wrong_frame.frame_sequence += 1;
        assert!(bind_control_bound_competitive_entry_review_commitment_v4(
            &classifier_bound,
            &wrong_frame,
            &deck_review_receipt_sha256,
        )
        .is_err());

        let mut wrong_classifier = dry_run.clone();
        wrong_classifier.source_navigation_classification_result_commitment_sha256 = "f".repeat(64);
        assert!(bind_control_bound_competitive_entry_review_commitment_v4(
            &classifier_bound,
            &wrong_classifier,
            &deck_review_receipt_sha256,
        )
        .is_err());

        let mut changed_control = dry_run.clone();
        changed_control.visible_control_region_sha256 = "0".repeat(64);
        let changed = bind_control_bound_competitive_entry_review_commitment_v4(
            &classifier_bound,
            &changed_control,
            &deck_review_receipt_sha256,
        )
        .unwrap();
        assert_ne!(baseline, changed);

        let mut changed_deck = changed_control;
        changed_deck.deck_manifest_sha256 = "4".repeat(64);
        let changed = bind_control_bound_competitive_entry_review_commitment_v4(
            &classifier_bound,
            &changed_deck,
            &deck_review_receipt_sha256,
        )
        .unwrap();
        assert_ne!(baseline, changed);

        let changed_receipt = bind_control_bound_competitive_entry_review_commitment_v4(
            &classifier_bound,
            &dry_run,
            &"4".repeat(64),
        )
        .unwrap();
        assert_ne!(baseline, changed_receipt);
    }

    #[test]
    fn competitive_entry_ratification_candidate_binds_exact_mode_terms_and_review() {
        let mut commitments = Vec::new();
        for (event_kind, resource, amount) in [
            (
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            ),
            (
                MtgoCompetitiveEventKindV1::Challenge,
                MtgoCompetitiveEntryResourceV1::ExistingEventTickets,
                25,
            ),
        ] {
            let (correspondence, source, source_identity, entry_authorization, review) =
                competitive_entry_ratification_parts_v1(event_kind, resource, amount);
            let candidate = competitive_entry_ratification_candidate_from_parts_v1(
                &correspondence,
                "UnbuckledPie",
                &source,
                &source_identity,
                &entry_authorization,
                &review,
            )
            .unwrap();
            assert_eq!(candidate.event_kind, event_kind);
            assert_eq!(candidate.resource, resource);
            assert_eq!(candidate.amount, amount);
            assert_eq!(candidate.ratification_commitment_sha256.len(), 64);
            assert_eq!(
                candidate.permission_review_commitment_sha256,
                correspondence.review_commitment_sha256()
            );
            assert!(!candidate.safe_for_live_input_v1());
            assert!(!candidate.permits_event_entry_v1());
            assert!(!candidate.permits_spending_v1());
            commitments.push(candidate.ratification_commitment_sha256);
        }
        assert_ne!(commitments[0], commitments[1]);
        assert!(RATIFIED_COMPETITIVE_SELECTED_LISTING_ENTRY_AUTHORIZATION_COMMITMENT_V2.is_none());
    }

    #[test]
    fn selected_listing_entry_bridge_binds_league_and_challenge_without_authority() {
        let mut ratifications = Vec::new();
        for (event_kind, resource, amount) in [
            (
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            ),
            (
                MtgoCompetitiveEventKindV1::Challenge,
                MtgoCompetitiveEntryResourceV1::ExistingEventTickets,
                25,
            ),
        ] {
            let (open, confirmation, visible, after, window, lineage, entry) =
                selected_listing_entry_bridge_parts_v1(event_kind, resource, amount);
            let binding = bind_selected_listing_to_competitive_entry_review_from_parts_v1(
                &open,
                &confirmation,
                &visible,
                &after,
                &window,
                &lineage,
                &entry,
            )
            .unwrap();
            assert_eq!(binding.event_kind, event_kind);
            assert_eq!(binding.resource, resource);
            assert_eq!(binding.amount, amount);
            assert_eq!(binding.open_arrival_frame_sequence, 40);
            assert_eq!(binding.entry_review_frame_sequence, 41);
            assert_eq!(binding.binding_commitment_sha256.len(), 64);
            let candidate =
                selected_listing_competitive_entry_ratification_candidate_from_parts_v2(
                    &binding, &entry,
                )
                .unwrap();
            assert_eq!(
                candidate.selected_listing_binding_commitment_sha256,
                binding.binding_commitment_sha256
            );
            assert_eq!(candidate.ratification_commitment_sha256.len(), 64);
            assert!(!candidate.safe_for_live_input_v2());
            assert!(!candidate.permits_event_entry_v2());
            assert!(!candidate.permits_spending_v2());
            ratifications.push(candidate.ratification_commitment_sha256);
        }
        assert_ne!(ratifications[0], ratifications[1]);
        assert!(RATIFIED_COMPETITIVE_SELECTED_LISTING_ENTRY_AUTHORIZATION_COMMITMENT_V2.is_none());
    }

    #[test]
    fn selected_listing_entry_bridge_rejects_identity_deck_window_and_order_drift() {
        let (open, confirmation, visible, after, window, lineage, entry) =
            selected_listing_entry_bridge_parts_v1(
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            );
        let check = |candidate_open: &MtgoReviewedCompetitiveOpenEntryReviewRatificationCandidateV1,
                     candidate_visible: &MtgoCompetitiveEventListingOpenVisibleConfirmationCommitmentsV1,
                     candidate_after: &MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1,
                     candidate_window: &str,
                     candidate_lineage: &MtgoCompetitiveEntryFrameTransitionViewV1,
                     candidate_entry: &MtgoReviewedCompetitiveEntryRatificationCandidateV1| {
            let mut candidate_confirmation = confirmation.clone();
            candidate_confirmation.visible_confirmation = candidate_visible.clone();
            candidate_confirmation.event_identity_sha256 =
                candidate_visible.event_identity_sha256.clone();
            candidate_confirmation.after_frame_id = candidate_visible.after_frame_id;
            candidate_confirmation.after_frame_sequence = candidate_visible.after_frame_sequence;
            candidate_confirmation.after_captured_at_unix_millis =
                candidate_visible.after_captured_at_unix_millis;
            bind_selected_listing_to_competitive_entry_review_from_parts_v1(
                candidate_open,
                &candidate_confirmation,
                candidate_visible,
                candidate_after,
                candidate_window,
                candidate_lineage,
                candidate_entry,
            )
        };

        let mut wrong_event = entry.clone();
        wrong_event.event_identity_sha256 = "f".repeat(64);
        assert!(check(&open, &visible, &after, &window, &lineage, &wrong_event).is_err());

        let mut wrong_deck = entry.clone();
        wrong_deck.deck_manifest_sha256 = "f".repeat(64);
        assert!(check(&open, &visible, &after, &window, &lineage, &wrong_deck).is_err());

        let mut wrong_account = lineage.clone();
        wrong_account.approved_account_alias_sha256 = "f".repeat(64);
        assert!(check(&open, &visible, &after, &window, &wrong_account, &entry).is_err());

        assert!(check(&open, &visible, &after, &"f".repeat(64), &lineage, &entry,).is_err());

        let mut stale = lineage.clone();
        stale.frame_sequence = visible.after_frame_sequence;
        assert!(check(&open, &visible, &after, &window, &stale, &entry).is_err());

        let binding = check(&open, &visible, &after, &window, &lineage, &entry).unwrap();
        let mut changed_entry = entry.clone();
        changed_entry.amount += 1;
        assert!(
            selected_listing_competitive_entry_ratification_candidate_from_parts_v2(
                &binding,
                &changed_entry,
            )
            .is_err()
        );
    }

    #[test]
    fn competitive_entry_ratification_rejects_lineage_terms_and_control_drift() {
        let (correspondence, source, source_identity, entry_authorization, review) =
            competitive_entry_ratification_parts_v1(
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            );
        let check =
            |identity: &MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1,
             authorization: &MtgoCompetitiveEntryAuthorizationV1,
             candidate_review: &MtgoControlBoundCompetitiveEntryReviewCommitmentsV4| {
                competitive_entry_ratification_candidate_from_parts_v1(
                    &correspondence,
                    "UnbuckledPie",
                    &source,
                    identity,
                    authorization,
                    candidate_review,
                )
            };

        let mut wrong_account = entry_authorization.clone();
        wrong_account.account_alias_sha256 = "0".repeat(64);
        assert!(check(&source_identity, &wrong_account, &review).is_err());

        let mut wrong_terms = entry_authorization.clone();
        wrong_terms.entry_terms.amount = 120;
        assert!(check(&source_identity, &wrong_terms, &review).is_err());

        let mut wrong_control = review.clone();
        wrong_control
            .entry_control_dry_run
            .visible_control_region_sha256 = "0".repeat(64);
        assert!(check(&source_identity, &entry_authorization, &wrong_control).is_err());

        let mut wrong_deck = review.clone();
        wrong_deck.entry_control_dry_run.deck_manifest_sha256 = "4".repeat(64);
        assert!(check(&source_identity, &entry_authorization, &wrong_deck).is_err());

        let mut wrong_classifier = source_identity.clone();
        wrong_classifier.source_navigation_classification_result_commitment_sha256 =
            Some("0".repeat(64));
        assert!(check(&wrong_classifier, &entry_authorization, &review).is_err());

        assert!(competitive_entry_ratification_candidate_from_parts_v1(
            &correspondence,
            "DifferentAccount",
            &source,
            &source_identity,
            &entry_authorization,
            &review,
        )
        .is_err());
    }

    #[test]
    fn competitive_entry_preparation_binds_fresh_league_and_challenge_recaptures() {
        let mut commitments = Vec::new();
        for (event_kind, resource, amount) in [
            (
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            ),
            (
                MtgoCompetitiveEventKindV1::Challenge,
                MtgoCompetitiveEntryResourceV1::ExistingEventTickets,
                25,
            ),
        ] {
            let (correspondence, source, source_identity, entry_authorization, review) =
                competitive_entry_ratification_parts_v1(event_kind, resource, amount);
            let candidate = competitive_entry_ratification_candidate_from_parts_v1(
                &correspondence,
                "UnbuckledPie",
                &source,
                &source_identity,
                &entry_authorization,
                &review,
            )
            .unwrap();
            let recapture = competitive_entry_immediate_recapture_commitments_v1(&candidate);
            let prepared =
                competitive_entry_preparation_from_commitments_v1(&candidate, &recapture).unwrap();
            assert_eq!(
                prepared.entry_ratification_commitment_sha256,
                candidate.ratification_commitment_sha256
            );
            assert_eq!(prepared.event_kind, event_kind);
            assert_eq!(prepared.resource, resource);
            assert_eq!(prepared.amount, amount);
            assert_eq!(prepared.immediate_frame_id, 18);
            assert_eq!(prepared.immediate_frame_sequence, 42);
            assert_eq!(prepared.preparation_commitment_sha256.len(), 64);
            commitments.push(prepared.preparation_commitment_sha256);
        }
        assert_ne!(commitments[0], commitments[1]);
    }

    #[test]
    fn competitive_entry_preparation_rejects_ratification_and_recapture_drift() {
        let (correspondence, source, source_identity, entry_authorization, review) =
            competitive_entry_ratification_parts_v1(
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            );
        let candidate = competitive_entry_ratification_candidate_from_parts_v1(
            &correspondence,
            "UnbuckledPie",
            &source,
            &source_identity,
            &entry_authorization,
            &review,
        )
        .unwrap();
        let recapture = competitive_entry_immediate_recapture_commitments_v1(&candidate);

        let mut wrong_account = recapture.clone();
        wrong_account.approved_account_alias_sha256 = "1".repeat(64);
        assert!(
            competitive_entry_preparation_from_commitments_v1(&candidate, &wrong_account).is_err()
        );

        let mut wrong_control = recapture.clone();
        wrong_control.visible_control_region_sha256 = "1".repeat(64);
        assert!(
            competitive_entry_preparation_from_commitments_v1(&candidate, &wrong_control).is_err()
        );

        let mut wrong_deck = recapture.clone();
        wrong_deck.selected_deck_region_sha256 = "1".repeat(64);
        assert!(
            competitive_entry_preparation_from_commitments_v1(&candidate, &wrong_deck).is_err()
        );

        let mut wrong_manifest = recapture.clone();
        wrong_manifest.deck_manifest_sha256 = "4".repeat(64);
        assert!(
            competitive_entry_preparation_from_commitments_v1(&candidate, &wrong_manifest).is_err()
        );

        let mut wrong_terms = recapture.clone();
        wrong_terms.amount += 1;
        assert!(
            competitive_entry_preparation_from_commitments_v1(&candidate, &wrong_terms).is_err()
        );

        let mut stale = recapture.clone();
        stale.immediate_frame_sequence = stale.source_frame_sequence;
        assert!(competitive_entry_preparation_from_commitments_v1(&candidate, &stale).is_err());

        let mut invalid_runtime = recapture;
        invalid_runtime.runtime_identity_commitment_sha256 = "not-a-digest".to_owned();
        assert!(
            competitive_entry_preparation_from_commitments_v1(&candidate, &invalid_runtime)
                .is_err()
        );
    }

    #[test]
    fn competitive_entry_input_receipt_binds_preparation_target_time_and_cursor_result() {
        let (correspondence, source, source_identity, entry_authorization, review) =
            competitive_entry_ratification_parts_v1(
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            );
        let candidate = competitive_entry_ratification_candidate_from_parts_v1(
            &correspondence,
            "UnbuckledPie",
            &source,
            &source_identity,
            &entry_authorization,
            &review,
        )
        .unwrap();
        let recapture = competitive_entry_immediate_recapture_commitments_v1(&candidate);
        let prepared =
            competitive_entry_preparation_from_commitments_v1(&candidate, &recapture).unwrap();
        let target = MtgoCompetitiveEntryPointerTargetV1 {
            hwnd: 17,
            process_id: 23,
            process_start_filetime_100ns: 31,
            dpi: 120,
            client_rect_desktop_px: crate::SignedRectV1 {
                left: -100,
                top: 20,
                right: 1140,
                bottom: 760,
            },
            target_x_desktop_px: 900,
            target_y_desktop_px: 600,
            park_x_desktop_px: 1200,
            park_y_desktop_px: 700,
        };
        let baseline = competitive_entry_input_receipt_from_parts_v1(&prepared, &target, 500, true);
        assert_eq!(baseline.len(), 64);

        let mut changed_target = MtgoCompetitiveEntryPointerTargetV1 { ..target.clone() };
        changed_target.target_x_desktop_px += 1;
        assert_ne!(
            baseline,
            competitive_entry_input_receipt_from_parts_v1(&prepared, &changed_target, 500, true)
        );
        assert_ne!(
            baseline,
            competitive_entry_input_receipt_from_parts_v1(&prepared, &changed_target, 501, true)
        );
        assert_ne!(
            baseline,
            competitive_entry_input_receipt_from_parts_v1(&prepared, &target, 500, false)
        );
    }

    #[test]
    fn competitive_entry_confirmation_receipt_binds_visible_result_and_candidate_count() {
        let (correspondence, source, source_identity, entry_authorization, review) =
            competitive_entry_ratification_parts_v1(
                MtgoCompetitiveEventKindV1::Challenge,
                MtgoCompetitiveEntryResourceV1::ExistingEventTickets,
                25,
            );
        let candidate = competitive_entry_ratification_candidate_from_parts_v1(
            &correspondence,
            "UnbuckledPie",
            &source,
            &source_identity,
            &entry_authorization,
            &review,
        )
        .unwrap();
        let recapture = competitive_entry_immediate_recapture_commitments_v1(&candidate);
        let prepared =
            competitive_entry_preparation_from_commitments_v1(&candidate, &recapture).unwrap();
        let input = MtgoCompetitiveEntryInputReceiptCommitmentsV1 {
            preparation_commitment_sha256: prepared.preparation_commitment_sha256.clone(),
            entry_ratification_commitment_sha256: prepared
                .entry_ratification_commitment_sha256
                .clone(),
            input_receipt_sha256: "1".repeat(64),
            event_kind: prepared.event_kind,
            resource: prepared.resource,
            amount: prepared.amount,
            immediate_frame_id: prepared.immediate_frame_id,
            immediate_frame_sequence: prepared.immediate_frame_sequence,
            input_sent_at_unix_millis: 500,
            cursor_parked_outside_client: true,
        };
        let transition = MtgoCompetitiveEntryFrameTransitionCommitmentsV1 {
            navigation_profile_commitment_sha256: recapture.navigation_profile_commitment_sha256,
            navigation_profile_admission_commitment_sha256: recapture
                .navigation_profile_admission_commitment_sha256,
            approved_account_alias_sha256: recapture.approved_account_alias_sha256,
            runtime_identity_commitment_sha256: recapture.runtime_identity_commitment_sha256,
            window_continuity_commitment_sha256: recapture.window_continuity_commitment_sha256,
            source_identity_commitment_sha256: recapture.source_identity_commitment_sha256,
            before_capture_commitment_sha256: recapture.immediate_capture_commitment_sha256,
            before_classification_result_commitment_sha256: recapture
                .immediate_classification_result_commitment_sha256,
            before_lifecycle_snapshot_commitment_sha256: recapture
                .immediate_lifecycle_snapshot_commitment_sha256,
            after_capture_commitment_sha256: "2".repeat(64),
            after_classification_result_commitment_sha256: "3".repeat(64),
            after_lifecycle_snapshot_commitment_sha256: "4".repeat(64),
            event_kind: prepared.event_kind,
            event_identity_sha256: recapture.event_identity_sha256,
            before_frame_id: prepared.immediate_frame_id,
            before_frame_sequence: prepared.immediate_frame_sequence,
            after_frame_id: 19,
            after_frame_sequence: prepared.immediate_frame_sequence + 1,
            transition_commitment_sha256: "5".repeat(64),
        };
        let visible = MtgoCompetitiveEntryVisibleConfirmationCommitmentsV1 {
            frame_transition: transition,
            after_capture_commitment_sha256: "2".repeat(64),
            after_classification_result_commitment_sha256: "3".repeat(64),
            after_lifecycle_snapshot_commitment_sha256: "4".repeat(64),
            after_captured_at_unix_millis: 600,
            postcondition_candidate_count: 2,
            confirmation_commitment_sha256: "6".repeat(64),
        };
        let baseline = competitive_entry_confirmation_receipt_v1(&input, &prepared, &visible);
        assert_eq!(baseline.len(), 64);
        let mut changed_count = visible.clone();
        changed_count.postcondition_candidate_count += 1;
        assert_ne!(
            baseline,
            competitive_entry_confirmation_receipt_v1(&input, &prepared, &changed_count)
        );
        let mut changed_after = visible;
        changed_after.after_capture_commitment_sha256 = "7".repeat(64);
        assert_ne!(
            baseline,
            competitive_entry_confirmation_receipt_v1(&input, &prepared, &changed_after)
        );
    }

    #[test]
    fn attended_entry_review_receipt_changes_with_exact_resource_amount_and_label() {
        fn receipt(resource: MtgoCompetitiveEntryResourceV1, amount: u32, label: &str) -> String {
            let correspondence = checked_competitive_correspondence_v2();
            let source = checked_competitive_entry_review_snapshot_v1(
                MtgoCompetitiveEventKindV1::League,
                resource,
                amount,
            );
            let nonce = [0x41u8; 8];
            let phrase = attended_competitive_entry_review_confirmation_phrase_v1(
                MtgoCompetitiveEventKindV1::League,
                resource,
                amount,
                &nonce,
            );
            review_competitive_entry_from_attended_confirmation_v1(
                &correspondence,
                source,
                "UnbuckledPie",
                label.to_owned(),
                nonce,
                2_001,
                &phrase,
            )
            .unwrap()
            .commitments_v1()
            .owner_review_receipt_sha256
        }

        let baseline = receipt(
            MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
            100,
            "Modern League",
        );
        assert_ne!(
            baseline,
            receipt(
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                120,
                "Modern League"
            )
        );
        assert_ne!(
            baseline,
            receipt(
                MtgoCompetitiveEntryResourceV1::ExistingEventTickets,
                10,
                "Modern League"
            )
        );
        assert_ne!(
            baseline,
            receipt(
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
                "Vintage League"
            )
        );
    }

    #[test]
    fn attended_entry_review_rejects_alias_mode_source_or_challenge_drift() {
        let correspondence = checked_competitive_correspondence_v2();
        let source = checked_competitive_entry_review_snapshot_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
            100,
        );
        assert!(build_attended_competitive_entry_review_request_v1(
            &correspondence,
            &source,
            "DifferentAccount",
            "Modern League",
        )
        .is_err());
        assert!(build_attended_competitive_entry_review_request_v1(
            &correspondence,
            &source,
            "UnbuckledPie",
            "Modern Challenge",
        )
        .is_err());
        assert!(build_attended_competitive_entry_review_request_v1(
            &correspondence,
            &source,
            "UnbuckledPie",
            "Modern\nLeague",
        )
        .is_err());

        let mut request = build_attended_competitive_entry_review_request_v1(
            &correspondence,
            &source,
            "UnbuckledPie",
            "Modern League",
        )
        .unwrap();
        request.entry_terms.amount = 120;
        assert!(validate_attended_competitive_entry_review_request_v1(
            &correspondence,
            &source,
            "UnbuckledPie",
            &request,
        )
        .is_err());

        let nonce = [0x22u8; 8];
        assert!(review_competitive_entry_from_attended_confirmation_v1(
            &correspondence,
            checked_competitive_entry_review_snapshot_v1(
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            ),
            "UnbuckledPie",
            "Modern League".to_owned(),
            nonce,
            2_002,
            "AUTHORIZE SOMETHING ELSE",
        )
        .is_err());
        let phrase = attended_competitive_entry_review_confirmation_phrase_v1(
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
            100,
            &nonce,
        );
        assert!(review_competitive_entry_from_attended_confirmation_v1(
            &correspondence,
            checked_competitive_entry_review_snapshot_v1(
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
                100,
            ),
            "UnbuckledPie",
            "Modern League".to_owned(),
            [0; 8],
            2_002,
            &phrase,
        )
        .is_err());
    }

    #[test]
    fn production_exact_match_launch_ratification_is_independently_empty() {
        let scope = competitive_scope_v1("UnbuckledPie", MtgoCompetitiveEventKindV1::League);
        let authorization =
            competitive_match_authorization_v1(MtgoCompetitiveEventKindV1::League, 2);
        assert!(
            ratify_competitive_match_launch_v1(&scope, "UnbuckledPie", authorization,).is_err()
        );

        let ratified = ratified_match_launch_v1(MtgoCompetitiveEventKindV1::League, 2);
        assert_eq!(ratified.event_kind_v1(), MtgoCompetitiveEventKindV1::League);
        assert_eq!(ratified.game_number_v1(), 2);
        assert_eq!(
            ratified.launch_authorization_commitment_sha256_v1().len(),
            64
        );
        assert!(!ratified.permits_event_entry_v1());
    }

    #[test]
    fn production_sideboard_automation_ratification_is_independently_empty() {
        assert_eq!(
            RATIFIED_COMPETITIVE_SIDEBOARD_AUTOMATION_COMMITMENT_V1,
            None
        );
        let scope = competitive_sideboard_automation_scope_v1();
        assert!(is_sha256_v2(&scope));
        assert_eq!(scope, competitive_sideboard_automation_scope_v1());
        assert_ne!(scope, competitive_lifecycle_allowed_actions_commitment_v1());
    }

    #[test]
    fn attended_match_launch_binds_terminal_challenge_mode_game_and_frame_lifetime() {
        for event_kind in [
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ] {
            let scope = competitive_scope_v1("UnbuckledPie", event_kind);
            let mut request = attended_match_launch_request_v4(event_kind, 2, 40);
            request.event_display_label = match event_kind {
                MtgoCompetitiveEventKindV1::League => "Modern League".to_owned(),
                MtgoCompetitiveEventKindV1::Challenge => "Modern Challenge".to_owned(),
            };
            let nonce = [0xabu8; 8];
            let phrase =
                attended_competitive_match_launch_confirmation_phrase_v4(event_kind, 2, &nonce);
            let ratified = ratify_competitive_match_launch_from_attended_confirmation_v4(
                &scope,
                "UnbuckledPie",
                request.clone(),
                nonce,
                1_777,
                &phrase,
            )
            .unwrap();
            let authorization = ratified.gameplay_authorization_record_v2();
            assert_eq!(ratified.event_kind_v1(), event_kind);
            assert_eq!(ratified.game_number_v1(), 2);
            assert_eq!(ratified.valid_from_frame_sequence_v2(), 40);
            assert_eq!(
                ratified.valid_through_frame_sequence_v1(),
                40 + ATTENDED_COMPETITIVE_MATCH_MAX_FRAME_ADVANCE_V4
            );
            assert_eq!(
                ratified.owner_launch_authorization_sha256_v2(),
                authorization.owner_launch_authorization_sha256
            );
            assert_eq!(
                authorization.account_alias_sha256,
                scope.account_alias_sha256
            );
            assert_eq!(
                authorization.written_permission_sha256,
                scope.written_permission_sha256
            );
            assert!(authorization.exact_match_gameplay_authorized);
            assert!(!ratified.permits_event_entry_v1());

            let mut relabeled_request = request.clone();
            relabeled_request.opponent_display_name = "OtherVisibleOpponent".to_owned();
            let relabeled = ratify_competitive_match_launch_from_attended_confirmation_v4(
                &scope,
                "UnbuckledPie",
                relabeled_request,
                nonce,
                1_777,
                &phrase,
            )
            .unwrap();
            assert_ne!(
                relabeled.owner_launch_authorization_sha256_v2(),
                ratified.owner_launch_authorization_sha256_v2()
            );

            assert!(
                ratify_competitive_match_launch_from_attended_confirmation_v4(
                    &scope,
                    "UnbuckledPie",
                    request,
                    nonce,
                    1_777,
                    "AUTHORIZE SOMETHING ELSE",
                )
                .is_err()
            );
        }
    }

    #[test]
    fn attended_match_launch_cannot_authorize_a_prelaunch_frame() {
        let event_kind = MtgoCompetitiveEventKindV1::League;
        let scope = competitive_scope_v1("UnbuckledPie", event_kind);
        let authorization = ratified_competitive_pass_v1(event_kind);
        let request = attended_match_launch_request_v4(event_kind, 2, 40);
        let nonce = [0x31u8; 8];
        let phrase =
            attended_competitive_match_launch_confirmation_phrase_v4(event_kind, 2, &nonce);
        let match_launch = ratify_competitive_match_launch_from_attended_confirmation_v4(
            &scope,
            "UnbuckledPie",
            request,
            nonce,
            1_888,
            &phrase,
        )
        .unwrap();
        let mut prepared = MtgoOpaqueCompetitiveDuelPassPreparationCommitmentsV1 {
            competitive_action_plan_commitment_sha256: "1".repeat(64),
            competitive_mode_authorization_commitment_sha256: authorization
                .mode_authorization_commitment_sha256_v1()
                .to_owned(),
            competitive_match_gameplay_authorization_commitment_sha256: match_launch
                .gameplay_authorization_commitment_sha256_v1()
                .to_owned(),
            before_input_postcondition_verification_commitment_sha256: "6".repeat(64),
            immediate_capture_commitment_sha256: "2".repeat(64),
            immediate_perception_result_commitment_sha256: "3".repeat(64),
            preparation_commitment_sha256: "4".repeat(64),
            event_kind,
            game_number: 2,
            immediate_frame_id: 11,
            immediate_frame_sequence: 39,
            immediate_captured_at_unix_millis: 13,
        };
        let session = begin_competitive_game_session_v1(authorization, match_launch).unwrap();
        assert!(
            competitive_duel_pass_authorization_binding_commitments_v1(&prepared, &session,)
                .is_err()
        );
        prepared.immediate_frame_sequence = 40;
        assert!(
            competitive_duel_pass_authorization_binding_commitments_v1(&prepared, &session,)
                .is_ok()
        );
        prepared.immediate_frame_sequence =
            40 + ATTENDED_COMPETITIVE_MATCH_MAX_FRAME_ADVANCE_V4 + 1;
        assert!(
            competitive_duel_pass_authorization_binding_commitments_v1(&prepared, &session,)
                .is_err()
        );
    }

    #[test]
    fn attended_match_launch_rejects_broad_or_malformed_requests() {
        let event_kind = MtgoCompetitiveEventKindV1::League;
        let scope = competitive_scope_v1("UnbuckledPie", event_kind);
        let valid = attended_match_launch_request_v4(event_kind, 1, 40);

        let mut broad_scope = scope.clone();
        broad_scope.challenge_input = true;
        assert!(validate_attended_competitive_match_launch_request_v4(
            &broad_scope,
            "UnbuckledPie",
            &valid,
        )
        .is_err());
        assert!(validate_attended_competitive_match_launch_request_v4(
            &scope,
            "DifferentAccount",
            &valid,
        )
        .is_err());
        assert!(validate_attended_launch_display_label_v4(
            "\u{202e}spoof",
            64,
            "opponent display name"
        )
        .is_err());

        for malformed in [
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                schema_version: 1,
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                game_number: 0,
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                observed_frame_sequence: 0,
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                event_identity_sha256: "C".repeat(64),
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                match_identity_sha256: valid.event_identity_sha256.clone(),
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                event_display_label: String::new(),
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                event_display_label: " Modern League".to_owned(),
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                opponent_display_name: "UnbuckledPie".to_owned(),
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                opponent_display_name: "Visible\nOpponent".to_owned(),
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                opponent_display_name: "x".repeat(65),
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                visible_match_id: "match-123".to_owned(),
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                visible_game_id: String::new(),
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                source_launch_identity_commitment_sha256: "A".repeat(64),
                ..valid.clone()
            },
            MtgoAttendedCompetitiveMatchLaunchRequestV4 {
                observed_frame_sequence: u64::MAX,
                ..valid
            },
        ] {
            assert!(validate_attended_competitive_match_launch_request_v4(
                &scope,
                "UnbuckledPie",
                &malformed,
            )
            .is_err());
        }
    }

    #[test]
    fn competitive_pass_ratification_rejects_alias_mode_and_scope_broadening() {
        let league = competitive_scope_v1("UnbuckledPie", MtgoCompetitiveEventKindV1::League);
        assert!(validate_competitive_duel_pass_authorization_v1(
            &league,
            "another-account",
            MtgoCompetitiveEventKindV1::League,
        )
        .is_err());
        assert!(validate_competitive_duel_pass_authorization_v1(
            &league,
            "UnbuckledPie",
            MtgoCompetitiveEventKindV1::Challenge,
        )
        .is_err());

        for mutation in 0..4 {
            let mut broader = league.clone();
            match mutation {
                0 => broader.challenge_input = true,
                1 => broader.private_match_input = true,
                2 => broader.open_play_input = true,
                3 => broader.other_prize_event_input = true,
                _ => unreachable!(),
            }
            assert!(validate_competitive_duel_pass_authorization_v1(
                &broader,
                "UnbuckledPie",
                MtgoCompetitiveEventKindV1::League,
            )
            .is_err());
        }

        let challenge = competitive_scope_v1("UnbuckledPie", MtgoCompetitiveEventKindV1::Challenge);
        let league_mode = validate_competitive_duel_pass_authorization_v1(
            &league,
            "UnbuckledPie",
            MtgoCompetitiveEventKindV1::League,
        )
        .unwrap();
        let challenge_mode = validate_competitive_duel_pass_authorization_v1(
            &challenge,
            "UnbuckledPie",
            MtgoCompetitiveEventKindV1::Challenge,
        )
        .unwrap();
        assert_ne!(league_mode, challenge_mode);
    }

    #[test]
    fn prepared_pass_binds_only_to_the_same_ratified_competitive_mode() {
        let authorization = ratified_competitive_pass_v1(MtgoCompetitiveEventKindV1::League);
        let match_launch = ratified_match_launch_v1(MtgoCompetitiveEventKindV1::League, 2);
        let correct_mode_commitment = authorization
            .mode_authorization_commitment_sha256_v1()
            .to_owned();
        let mut prepared = MtgoOpaqueCompetitiveDuelPassPreparationCommitmentsV1 {
            competitive_action_plan_commitment_sha256: "1".repeat(64),
            competitive_mode_authorization_commitment_sha256: authorization
                .mode_authorization_commitment_sha256_v1()
                .to_owned(),
            competitive_match_gameplay_authorization_commitment_sha256: match_launch
                .gameplay_authorization_commitment_sha256_v1()
                .to_owned(),
            before_input_postcondition_verification_commitment_sha256: "6".repeat(64),
            immediate_capture_commitment_sha256: "2".repeat(64),
            immediate_perception_result_commitment_sha256: "3".repeat(64),
            preparation_commitment_sha256: "4".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::League,
            game_number: 2,
            immediate_frame_id: 11,
            immediate_frame_sequence: 12,
            immediate_captured_at_unix_millis: 13,
        };
        let session = begin_competitive_game_session_v1(authorization, match_launch).unwrap();
        let bound = competitive_duel_pass_authorization_binding_commitments_v1(&prepared, &session)
            .unwrap();
        assert_eq!(bound.event_kind, MtgoCompetitiveEventKindV1::League);
        assert_eq!(bound.game_number, 2);
        assert_eq!(bound.immediate_frame_id, 11);
        assert_eq!(bound.immediate_frame_sequence, 12);
        assert_eq!(bound.authorization_binding_commitment_sha256.len(), 64);

        prepared.event_kind = MtgoCompetitiveEventKindV1::Challenge;
        assert!(
            competitive_duel_pass_authorization_binding_commitments_v1(&prepared, &session,)
                .is_err()
        );
        prepared.event_kind = MtgoCompetitiveEventKindV1::League;
        prepared.competitive_mode_authorization_commitment_sha256 = "5".repeat(64);
        assert!(
            competitive_duel_pass_authorization_binding_commitments_v1(&prepared, &session,)
                .is_err()
        );
        prepared.competitive_mode_authorization_commitment_sha256 = correct_mode_commitment;
        prepared.competitive_match_gameplay_authorization_commitment_sha256 = "8".repeat(64);
        assert!(
            competitive_duel_pass_authorization_binding_commitments_v1(&prepared, &session,)
                .is_err()
        );
    }

    #[test]
    fn competitive_game_session_advances_only_after_a_newer_visible_transition() {
        let authorization = ratified_competitive_pass_v1(MtgoCompetitiveEventKindV1::League);
        let match_launch = ratified_match_launch_v1(MtgoCompetitiveEventKindV1::League, 2);
        let session = begin_competitive_game_session_v1(authorization, match_launch).unwrap();
        let initial = session.commitments_v1();
        assert_eq!(initial.confirmed_action_count, 0);
        assert_eq!(initial.last_confirmed_frame_sequence, 0);
        assert!(!session.safe_for_input_v1());
        assert!(!session.permits_event_entry_v1());

        let visible = MtgoOpaqueCompetitiveDuelPassConfirmationCommitmentsV1 {
            before_input_verification_commitment_sha256: "3".repeat(64),
            after_capture_commitment_sha256: "4".repeat(64),
            checked_postcondition_commitment_sha256: "5".repeat(64),
            opaque_confirmation_commitment_sha256: "6".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::League,
            game_number: 2,
            after_frame_id: 10,
            after_frame_sequence: 12,
            postcondition_candidate_count: 3,
        };
        let session =
            advance_competitive_game_session_v1(session, &visible, &"a".repeat(64)).unwrap();
        let advanced = session.commitments_v1();
        assert_eq!(advanced.confirmed_action_count, 1);
        assert_eq!(advanced.last_confirmed_frame_sequence, 12);
        assert_ne!(
            initial.session_commitment_sha256,
            advanced.session_commitment_sha256
        );

        let mut prepared = MtgoOpaqueCompetitiveDuelPassPreparationCommitmentsV1 {
            competitive_action_plan_commitment_sha256: "1".repeat(64),
            competitive_mode_authorization_commitment_sha256: session
                .authorization
                .mode_authorization_commitment_sha256
                .clone(),
            competitive_match_gameplay_authorization_commitment_sha256: session
                .match_launch
                .gameplay_authorization_commitment_sha256
                .clone(),
            before_input_postcondition_verification_commitment_sha256: "6".repeat(64),
            immediate_capture_commitment_sha256: "2".repeat(64),
            immediate_perception_result_commitment_sha256: "3".repeat(64),
            preparation_commitment_sha256: "4".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::League,
            game_number: 2,
            immediate_frame_id: 11,
            immediate_frame_sequence: 12,
            immediate_captured_at_unix_millis: 13,
        };
        assert!(
            competitive_duel_pass_authorization_binding_commitments_v1(&prepared, &session)
                .is_err()
        );
        prepared.immediate_frame_sequence = 13;
        let next = competitive_duel_pass_authorization_binding_commitments_v1(&prepared, &session)
            .unwrap();
        assert_eq!(
            next.competitive_game_session_commitment_sha256,
            advanced.session_commitment_sha256
        );
    }

    #[test]
    fn competitive_game_session_rejects_mismatched_or_expired_authority() {
        let league = ratified_competitive_pass_v1(MtgoCompetitiveEventKindV1::League);
        let challenge_launch = ratified_match_launch_v1(MtgoCompetitiveEventKindV1::Challenge, 1);
        assert!(begin_competitive_game_session_v1(league, challenge_launch).is_err());

        let league = ratified_competitive_pass_v1(MtgoCompetitiveEventKindV1::League);
        let mut invalid_launch = ratified_match_launch_v1(MtgoCompetitiveEventKindV1::League, 1);
        invalid_launch.valid_from_frame_sequence = 0;
        assert!(begin_competitive_game_session_v1(league, invalid_launch).is_err());

        let league = ratified_competitive_pass_v1(MtgoCompetitiveEventKindV1::League);
        let launch = ratified_match_launch_v1(MtgoCompetitiveEventKindV1::League, 1);
        let session = begin_competitive_game_session_v1(league, launch).unwrap();
        let visible = MtgoOpaqueCompetitiveDuelPassConfirmationCommitmentsV1 {
            before_input_verification_commitment_sha256: "3".repeat(64),
            after_capture_commitment_sha256: "4".repeat(64),
            checked_postcondition_commitment_sha256: "5".repeat(64),
            opaque_confirmation_commitment_sha256: "6".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::League,
            game_number: 1,
            after_frame_id: 101,
            after_frame_sequence: 101,
            postcondition_candidate_count: 1,
        };
        assert!(advance_competitive_game_session_v1(session, &visible, &"a".repeat(64)).is_err());
    }

    #[test]
    fn competitive_transition_receipt_binds_input_authority_visible_result_and_frame() {
        let visible = MtgoOpaqueCompetitiveDuelPassConfirmationCommitmentsV1 {
            before_input_verification_commitment_sha256: "3".repeat(64),
            after_capture_commitment_sha256: "4".repeat(64),
            checked_postcondition_commitment_sha256: "5".repeat(64),
            opaque_confirmation_commitment_sha256: "6".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::League,
            game_number: 1,
            after_frame_id: 10,
            after_frame_sequence: 11,
            postcondition_candidate_count: 3,
        };
        let baseline =
            competitive_duel_pass_transition_receipt_v1(&"1".repeat(64), &"2".repeat(64), &visible);
        assert_eq!(baseline.len(), 64);
        assert_ne!(
            baseline,
            competitive_duel_pass_transition_receipt_v1(&"a".repeat(64), &"2".repeat(64), &visible,)
        );
        let mut changed = visible.clone();
        changed.event_kind = MtgoCompetitiveEventKindV1::Challenge;
        assert_ne!(
            baseline,
            competitive_duel_pass_transition_receipt_v1(&"1".repeat(64), &"2".repeat(64), &changed,)
        );
        changed.event_kind = MtgoCompetitiveEventKindV1::League;
        changed.after_frame_sequence = 12;
        assert_ne!(
            baseline,
            competitive_duel_pass_transition_receipt_v1(&"1".repeat(64), &"2".repeat(64), &changed,)
        );
        changed.after_frame_sequence = 11;
        changed.postcondition_candidate_count = 4;
        assert_ne!(
            baseline,
            competitive_duel_pass_transition_receipt_v1(&"1".repeat(64), &"2".repeat(64), &changed,)
        );
    }

    #[test]
    fn cursor_park_point_must_be_on_output_and_outside_client() {
        let client = crate::SignedRectV1 {
            left: 200,
            top: 100,
            right: 1_750,
            bottom: 1_025,
        };
        let output = crate::SignedRectV1 {
            left: -100,
            top: -50,
            right: 2_460,
            bottom: 1_390,
        };
        let point = crate::probe::choose_cursor_park_point_v3(&client, &output).unwrap();
        assert!(output.contains_point(point.0, point.1));
        assert!(!client.contains_point(point.0, point.1));

        assert!(crate::probe::choose_cursor_park_point_v3(&output, &output).is_err());
    }

    #[test]
    fn freshness_and_process_gate_block_stale_or_concurrent_input() {
        validate_preinput_capture_freshness_v3(1_000, 1_000).unwrap();
        validate_preinput_capture_freshness_v3(1_000, 3_000).unwrap();
        assert!(validate_preinput_capture_freshness_v3(1_000, 3_001).is_err());
        assert!(validate_preinput_capture_freshness_v3(1_001, 1_000).is_err());

        {
            let mut gate = input_gate_v3().lock().unwrap();
            *gate = PregameInputGateStateV3::Idle;
        }
        reserve_input_gate_v3().unwrap();
        assert_eq!(
            pregame_input_gate_status_v3().unwrap(),
            MtgoPregameInputGateStatusV3::Preparing
        );
        assert!(reserve_input_gate_v3().is_err());
        halt_before_input_attempt_v3().unwrap();
        set_pending_v3(&"a".repeat(64)).unwrap();
        assert_eq!(
            pregame_input_gate_status_v3().unwrap(),
            MtgoPregameInputGateStatusV3::AwaitingVisiblePostcondition
        );
        assert!(reserve_input_gate_v3().is_err());
        assert!(require_matching_pending_v3(&"b".repeat(64)).is_err());
        require_matching_pending_v3(&"a".repeat(64)).unwrap();
        release_confirmed_pending_v3(&"a".repeat(64)).unwrap();
        assert_eq!(
            pregame_input_gate_status_v3().unwrap(),
            MtgoPregameInputGateStatusV3::Idle
        );
    }

    #[test]
    fn input_receipt_binds_authorization_plan_frame_action_point_and_time() {
        let authorization = authorized_scope_v3("UnbuckledPie");
        let mut prepared = PreparedPregameActuationV3 {
            hwnd: 10,
            process_id: 20,
            process_start_filetime_100ns: 30,
            dpi: 120,
            client_rect_desktop_px: crate::SignedRectV1 {
                left: 100,
                top: 200,
                right: 1_650,
                bottom: 1_125,
            },
            target_x_desktop_px: 130,
            target_y_desktop_px: 353,
            park_x_desktop_px: 1,
            park_y_desktop_px: 1,
            current_capture_commitment_sha256: "b".repeat(64),
            current_captured_at_unix_millis: 1_000,
            action_plan_commitment_sha256: "c".repeat(64),
            selected_semantic: MtgoPregameActionSemanticV1::Mulligan { next_hand_size: 6 },
            planned_postcondition: MtgoPlannedPregamePostconditionV3::NextMulliganPrompt {
                prospective_keep_size: 6,
            },
        };
        let baseline = input_receipt_commitment_v3(&prepared, &authorization, 1_100, true);

        prepared.target_x_desktop_px += 1;
        assert_ne!(
            baseline,
            input_receipt_commitment_v3(&prepared, &authorization, 1_100, true)
        );
        prepared.target_x_desktop_px -= 1;
        prepared.selected_semantic = MtgoPregameActionSemanticV1::KeepOpeningHand;
        assert_ne!(
            baseline,
            input_receipt_commitment_v3(&prepared, &authorization, 1_100, true)
        );
        prepared.selected_semantic = MtgoPregameActionSemanticV1::Mulligan { next_hand_size: 6 };
        prepared.current_capture_commitment_sha256 = "d".repeat(64);
        assert_ne!(
            baseline,
            input_receipt_commitment_v3(&prepared, &authorization, 1_100, true)
        );
        prepared.current_capture_commitment_sha256 = "b".repeat(64);
        assert_ne!(
            baseline,
            input_receipt_commitment_v3(&prepared, &authorization, 1_101, true)
        );
        assert_ne!(
            baseline,
            input_receipt_commitment_v3(&prepared, &authorization, 1_100, false)
        );

        let mut different_permission = authorization;
        different_permission.written_permission_sha256 = "e".repeat(64);
        assert_ne!(
            baseline,
            input_receipt_commitment_v3(&prepared, &different_permission, 1_100, true)
        );
    }

    #[test]
    fn competitive_lifecycle_input_stays_unratified_and_excludes_entry_actions() {
        assert_eq!(
            RATIFIED_COMPETITIVE_LIFECYCLE_AUTHORIZATION_COMMITMENT_V1,
            None
        );

        let allowed = [
            MtgoCompetitiveLifecycleActionV1::AcceptPairing,
            MtgoCompetitiveLifecycleActionV1::SubmitSideboard,
            MtgoCompetitiveLifecycleActionV1::ContinueAfterMatch,
            MtgoCompetitiveLifecycleActionV1::ResumeMatch,
            MtgoCompetitiveLifecycleActionV1::CloseCompletedEvent,
        ];
        assert!(allowed.into_iter().all(is_non_entry_lifecycle_action_v1));
        assert!(!is_non_entry_lifecycle_action_v1(
            MtgoCompetitiveLifecycleActionV1::OpenEntryReview
        ));
        assert!(!is_non_entry_lifecycle_action_v1(
            MtgoCompetitiveLifecycleActionV1::CancelEntry
        ));
        assert!(!is_non_entry_lifecycle_action_v1(
            MtgoCompetitiveLifecycleActionV1::ConfirmEntry
        ));

        let commitment = competitive_lifecycle_allowed_actions_commitment_v1();
        assert_eq!(commitment.len(), 64);
        assert_eq!(
            commitment,
            competitive_lifecycle_allowed_actions_commitment_v1()
        );
    }

    #[test]
    fn sideboard_submit_requires_exact_retained_target_provenance() {
        let ready = "a".repeat(64);
        let changed = "b".repeat(64);
        assert!(
            validate_competitive_lifecycle_sideboard_submit_provenance_parts_v1(
                MtgoCompetitiveLifecycleActionV1::AcceptPairing,
                None,
                None,
            )
            .is_ok()
        );
        assert!(
            validate_competitive_lifecycle_sideboard_submit_provenance_parts_v1(
                MtgoCompetitiveLifecycleActionV1::SubmitSideboard,
                None,
                None,
            )
            .is_err()
        );
        assert!(
            validate_competitive_lifecycle_sideboard_submit_provenance_parts_v1(
                MtgoCompetitiveLifecycleActionV1::SubmitSideboard,
                Some(&ready),
                Some(&ready),
            )
            .is_ok()
        );
        assert!(
            validate_competitive_lifecycle_sideboard_submit_provenance_parts_v1(
                MtgoCompetitiveLifecycleActionV1::SubmitSideboard,
                Some(&ready),
                Some(&changed),
            )
            .is_err()
        );
        assert!(
            validate_competitive_lifecycle_sideboard_submit_provenance_parts_v1(
                MtgoCompetitiveLifecycleActionV1::ContinueAfterMatch,
                Some(&ready),
                Some(&ready),
            )
            .is_err()
        );
    }

    #[test]
    fn competitive_open_entry_review_stays_separately_unratified_and_non_spending() {
        assert_eq!(
            RATIFIED_COMPETITIVE_OPEN_ENTRY_REVIEW_AUTHORIZATION_COMMITMENT_V1,
            None
        );
        let scope = competitive_open_entry_review_scope_commitment_v1();
        assert_eq!(scope.len(), 64);
        assert_eq!(scope, competitive_open_entry_review_scope_commitment_v1());
        assert_ne!(scope, competitive_lifecycle_allowed_actions_commitment_v1());
    }

    #[test]
    fn competitive_pregame_observation_binds_stage_and_rejects_skips() {
        let runtime = competitive_event_runtime_commitments_fixture_v1(
            MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
        );
        let seven = competitive_pregame_observation_fixture_v1(
            &runtime,
            MtgoCompetitivePregameStageV1::MulliganChoice {
                prospective_keep_size: 7,
            },
            30,
        );
        validate_competitive_pregame_observation_v1(&seven).unwrap();
        let mut changed = seven.clone();
        changed.stage = MtgoCompetitivePregameStageV1::GameplayReady;
        assert!(validate_competitive_pregame_observation_v1(&changed).is_err());

        assert!(validate_competitive_pregame_stage_transition_v1(
            MtgoCompetitivePregameStageV1::MulliganChoice {
                prospective_keep_size: 7,
            },
            MtgoCompetitivePregameStageV1::MulliganChoice {
                prospective_keep_size: 6,
            },
        )
        .is_ok());
        assert!(validate_competitive_pregame_stage_transition_v1(
            MtgoCompetitivePregameStageV1::MulliganChoice {
                prospective_keep_size: 6,
            },
            MtgoCompetitivePregameStageV1::LondonBottoming {
                required_bottom_count: 1,
                selected_bottom_count: 0,
            },
        )
        .is_ok());
        assert!(validate_competitive_pregame_stage_transition_v1(
            MtgoCompetitivePregameStageV1::LondonBottoming {
                required_bottom_count: 1,
                selected_bottom_count: 0,
            },
            MtgoCompetitivePregameStageV1::LondonBottoming {
                required_bottom_count: 1,
                selected_bottom_count: 1,
            },
        )
        .is_ok());
        assert!(validate_competitive_pregame_stage_transition_v1(
            MtgoCompetitivePregameStageV1::LondonBottoming {
                required_bottom_count: 1,
                selected_bottom_count: 1,
            },
            MtgoCompetitivePregameStageV1::GameplayReady,
        )
        .is_ok());
        assert!(validate_competitive_pregame_stage_transition_v1(
            MtgoCompetitivePregameStageV1::MulliganChoice {
                prospective_keep_size: 6,
            },
            MtgoCompetitivePregameStageV1::GameplayReady,
        )
        .is_err());
        assert!(validate_competitive_pregame_stage_transition_v1(
            MtgoCompetitivePregameStageV1::LondonBottoming {
                required_bottom_count: 2,
                selected_bottom_count: 0,
            },
            MtgoCompetitivePregameStageV1::LondonBottoming {
                required_bottom_count: 2,
                selected_bottom_count: 2,
            },
        )
        .is_err());

        let mut impossible = seven;
        impossible.stage = MtgoCompetitivePregameStageV1::LondonBottoming {
            required_bottom_count: 0,
            selected_bottom_count: 0,
        };
        reseal_competitive_pregame_observation_v1(&mut impossible);
        assert!(validate_competitive_pregame_observation_v1(&impossible).is_err());
    }

    #[test]
    fn competitive_pregame_checkout_binds_both_modes_and_exact_lineage() {
        for event_kind in [
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ] {
            let mut runtime = competitive_event_runtime_commitments_fixture_v1(
                MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
            );
            runtime.event_kind = event_kind;
            let launch = ratified_match_launch_for_event_runtime_v1(&runtime, 25);
            let source = competitive_pregame_observation_fixture_v1(
                &runtime,
                MtgoCompetitivePregameStageV1::MulliganChoice {
                    prospective_keep_size: 7,
                },
                30,
            );
            let session = competitive_event_pregame_session_commitments_from_parts_v1(
                &runtime,
                "6".repeat(64).as_str(),
                "7".repeat(64).as_str(),
                250,
                &launch,
                &source,
            )
            .unwrap();
            assert_eq!(session.event_kind, event_kind);
            assert_eq!(session.game_number, 1);
            assert_eq!(session.deck_manifest_sha256, runtime.deck_manifest_sha256);
            assert_eq!(
                session.policy_deployment_commitment_sha256,
                runtime.policy_deployment_commitment_sha256
            );
            assert_eq!(session.current_stage, source.stage);
            assert_eq!(session.visible_transition_count, 0);
            assert!(is_sha256_v2(&session.session_commitment_sha256));

            assert!(competitive_event_pregame_session_commitments_from_parts_v1(
                &runtime,
                "9".repeat(64).as_str(),
                "7".repeat(64).as_str(),
                250,
                &launch,
                &source,
            )
            .is_err());

            assert!(competitive_event_pregame_session_commitments_from_parts_v1(
                &runtime,
                "6".repeat(64).as_str(),
                "9".repeat(64).as_str(),
                250,
                &launch,
                &source,
            )
            .is_err());

            let mut wrong_account = source.clone();
            wrong_account.approved_account_alias_sha256 = "9".repeat(64);
            reseal_competitive_pregame_observation_v1(&mut wrong_account);
            assert!(competitive_event_pregame_session_commitments_from_parts_v1(
                &runtime,
                "6".repeat(64).as_str(),
                "7".repeat(64).as_str(),
                250,
                &launch,
                &wrong_account,
            )
            .is_err());

            let mut wrong_match = source.clone();
            wrong_match.match_identity_sha256 = "9".repeat(64);
            reseal_competitive_pregame_observation_v1(&mut wrong_match);
            assert!(competitive_event_pregame_session_commitments_from_parts_v1(
                &runtime,
                "6".repeat(64).as_str(),
                "7".repeat(64).as_str(),
                250,
                &launch,
                &wrong_match,
            )
            .is_err());

            let mut stale = source.clone();
            stale.frame_sequence = runtime.current_frame_sequence;
            stale.frame_id += 1;
            stale.captured_at_unix_millis = 250;
            reseal_competitive_pregame_observation_v1(&mut stale);
            assert!(competitive_event_pregame_session_commitments_from_parts_v1(
                &runtime,
                "6".repeat(64).as_str(),
                "7".repeat(64).as_str(),
                250,
                &launch,
                &stale,
            )
            .is_err());

            let mut crossed_launch = ratified_match_launch_for_event_runtime_v1(&runtime, 25);
            crossed_launch.authorization.entry_authorization_sha256 = "9".repeat(64);
            assert!(competitive_event_pregame_session_commitments_from_parts_v1(
                &runtime,
                "6".repeat(64).as_str(),
                "7".repeat(64).as_str(),
                250,
                &crossed_launch,
                &source,
            )
            .is_err());

            let mut already_complete = runtime.clone();
            already_complete.pregame_session_count = 1;
            already_complete.last_completed_pregame =
                Some(MtgoCompletedCompetitivePregameCommitmentsV1 {
                    completion_receipt_sha256: "b".repeat(64),
                    pregame_session_commitment_sha256: "c".repeat(64),
                    final_observation_commitment_sha256: "d".repeat(64),
                    match_identity_sha256: runtime.current_match_identity_sha256.clone().unwrap(),
                    game_number: 1,
                    completion_frame_sequence: 29,
                });
            let repeated_launch = ratified_match_launch_for_event_runtime_v1(&already_complete, 25);
            assert!(competitive_event_pregame_session_commitments_from_parts_v1(
                &already_complete,
                "6".repeat(64).as_str(),
                "7".repeat(64).as_str(),
                250,
                &repeated_launch,
                &source,
            )
            .is_err());
        }
    }

    #[test]
    fn classified_pregame_view_inherits_exact_event_scope_and_stage() {
        let source_capture = "0".repeat(64);
        let duel_profile = "1".repeat(64);
        let duel_admission = "2".repeat(64);
        let classifier_runtime = "3".repeat(64);
        let pregame_evaluation = "4".repeat(64);
        let pregame_admission = "5".repeat(64);
        let classification = "6".repeat(64);
        let visible_interaction = "9".repeat(64);
        let process = "7".repeat(64);
        let window = "8".repeat(64);
        for event_kind in [
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ] {
            let mut runtime = competitive_event_runtime_commitments_fixture_v1(
                MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
            );
            runtime.event_kind = event_kind;
            let source = CompetitivePregameClassifiedViewV2 {
                source_capture_commitment_sha256: &source_capture,
                duel_perception_profile_commitment_sha256: &duel_profile,
                duel_perception_profile_admission_commitment_sha256: &duel_admission,
                classifier_runtime_commitment_sha256: &classifier_runtime,
                pregame_evaluation_commitment_sha256: &pregame_evaluation,
                pregame_profile_admission_commitment_sha256: &pregame_admission,
                pregame_classification_commitment_sha256: &classification,
                visible_interaction_commitment_sha256: &visible_interaction,
                process_continuity_commitment_sha256: &process,
                window_continuity_commitment_sha256: &window,
                stage: MtgoCompetitivePregameStageLabelV1::LondonBottoming {
                    required_bottom_count: 2,
                    selected_bottom_count: 1,
                },
                frame_id: 31,
                frame_sequence: 21,
                captured_at_unix_millis: 210,
            };
            let observation =
                competitive_pregame_observation_from_classified_view_v2(&runtime, source).unwrap();
            assert_eq!(observation.event_kind, event_kind);
            assert_eq!(
                observation.visible_interaction_commitment_sha256,
                visible_interaction
            );
            assert_eq!(
                observation.match_identity_sha256,
                runtime.current_match_identity_sha256.clone().unwrap()
            );
            assert_eq!(observation.game_number, 1);
            assert_eq!(
                observation.stage,
                MtgoCompetitivePregameStageV1::LondonBottoming {
                    required_bottom_count: 2,
                    selected_bottom_count: 1,
                }
            );
            assert_eq!(
                observation.classifier_runtime_commitment_sha256,
                classifier_runtime
            );
            assert!(is_sha256_v2(&observation.observation_commitment_sha256));

            let mut impossible = source;
            impossible.stage = MtgoCompetitivePregameStageLabelV1::MulliganChoice {
                prospective_keep_size: 8,
            };
            assert!(
                competitive_pregame_observation_from_classified_view_v2(&runtime, impossible,)
                    .is_err()
            );

            runtime.closed_to_event_browser = true;
            assert!(
                competitive_pregame_observation_from_classified_view_v2(&runtime, source,).is_err()
            );
        }
    }

    #[test]
    fn competitive_event_runtime_commitment_binds_mode_state_monitor_and_counts() {
        let mut state = MtgoCompetitiveEventRuntimeCommitmentsV1 {
            runtime_commitment_sha256: String::new(),
            entry_confirmation_receipt_sha256: "1".repeat(64),
            entry_ratification_commitment_sha256: "2".repeat(64),
            entry_authorization_sha256: "d".repeat(64),
            correspondence_sha256: "e".repeat(64),
            permission_review_commitment_sha256: "f".repeat(64),
            deck_list_sha256: "a".repeat(64),
            deck_manifest_sha256: "0".repeat(64),
            deck_format_sha256: "1".repeat(64),
            player_known_current_deck_configuration_commitment_sha256: "b".repeat(64),
            selected_deck_label_sha256: "2".repeat(64),
            selected_deck_region_sha256: "3".repeat(64),
            policy_deployment_commitment_sha256: "4".repeat(64),
            lifecycle_authorization_commitment_sha256: "3".repeat(64),
            mode_authorization_commitment_sha256: "4".repeat(64),
            navigation_profile_commitment_sha256: "5".repeat(64),
            navigation_profile_admission_commitment_sha256: "6".repeat(64),
            approved_account_alias_sha256: "7".repeat(64),
            bound_event_identity_sha256: "8".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::League,
            current_phase: MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
            current_lifecycle_snapshot_commitment_sha256: "9".repeat(64),
            current_match_identity_sha256: Some("a".repeat(64)),
            current_game_number: Some(1),
            current_frame_id: 10,
            current_frame_sequence: 20,
            lifecycle_transition_count: 3,
            confirmed_lifecycle_action_count: 1,
            observed_lifecycle_advance_count: 2,
            pregame_session_count: 0,
            last_completed_pregame: None,
            gameplay_lease_count: 0,
            last_returned_gameplay_frame_sequence: None,
            event_monitor_chain_commitment_sha256: Some("b".repeat(64)),
            event_monitor_observation_count: 4,
            terminal_event_record_confirmed: false,
            closed_to_event_browser: false,
        };
        let prior = "c".repeat(64);
        let baseline = competitive_event_runtime_commitment_v1(
            COMPETITIVE_EVENT_RUNTIME_ADVANCE_DOMAIN_V1,
            Some(&prior),
            &state,
            b"transition",
        );
        assert_eq!(baseline.len(), 64);
        assert_eq!(
            baseline,
            competitive_event_runtime_commitment_v1(
                COMPETITIVE_EVENT_RUNTIME_ADVANCE_DOMAIN_V1,
                Some(&prior),
                &state,
                b"transition",
            )
        );

        state.event_kind = MtgoCompetitiveEventKindV1::Challenge;
        assert_ne!(
            baseline,
            competitive_event_runtime_commitment_v1(
                COMPETITIVE_EVENT_RUNTIME_ADVANCE_DOMAIN_V1,
                Some(&prior),
                &state,
                b"transition",
            )
        );
        state.event_kind = MtgoCompetitiveEventKindV1::League;
        state.entry_authorization_sha256 = "e".repeat(64);
        assert_ne!(
            baseline,
            competitive_event_runtime_commitment_v1(
                COMPETITIVE_EVENT_RUNTIME_ADVANCE_DOMAIN_V1,
                Some(&prior),
                &state,
                b"transition",
            )
        );
        state.entry_authorization_sha256 = "d".repeat(64);
        state.deck_list_sha256 = "3".repeat(64);
        assert_ne!(
            baseline,
            competitive_event_runtime_commitment_v1(
                COMPETITIVE_EVENT_RUNTIME_ADVANCE_DOMAIN_V1,
                Some(&prior),
                &state,
                b"transition",
            )
        );
        state.deck_list_sha256 = "a".repeat(64);
        state.deck_manifest_sha256 = "3".repeat(64);
        assert_ne!(
            baseline,
            competitive_event_runtime_commitment_v1(
                COMPETITIVE_EVENT_RUNTIME_ADVANCE_DOMAIN_V1,
                Some(&prior),
                &state,
                b"transition",
            )
        );
        state.deck_manifest_sha256 = "0".repeat(64);
        state.player_known_current_deck_configuration_commitment_sha256 = "c".repeat(64);
        assert_ne!(
            baseline,
            competitive_event_runtime_commitment_v1(
                COMPETITIVE_EVENT_RUNTIME_ADVANCE_DOMAIN_V1,
                Some(&prior),
                &state,
                b"transition",
            )
        );
        state.player_known_current_deck_configuration_commitment_sha256 = "b".repeat(64);
        state.pregame_session_count = 1;
        state.last_completed_pregame = Some(MtgoCompletedCompetitivePregameCommitmentsV1 {
            completion_receipt_sha256: "5".repeat(64),
            pregame_session_commitment_sha256: "6".repeat(64),
            final_observation_commitment_sha256: "7".repeat(64),
            match_identity_sha256: "a".repeat(64),
            game_number: 1,
            completion_frame_sequence: 21,
        });
        assert_ne!(
            baseline,
            competitive_event_runtime_commitment_v1(
                COMPETITIVE_EVENT_RUNTIME_ADVANCE_DOMAIN_V1,
                Some(&prior),
                &state,
                b"transition",
            )
        );
        state.pregame_session_count = 0;
        state.last_completed_pregame = None;
        state.gameplay_lease_count = 1;
        assert_ne!(
            baseline,
            competitive_event_runtime_commitment_v1(
                COMPETITIVE_EVENT_RUNTIME_ADVANCE_DOMAIN_V1,
                Some(&prior),
                &state,
                b"transition",
            )
        );
        state.gameplay_lease_count = 0;
        state.last_returned_gameplay_frame_sequence = Some(21);
        assert_ne!(
            baseline,
            competitive_event_runtime_commitment_v1(
                COMPETITIVE_EVENT_RUNTIME_ADVANCE_DOMAIN_V1,
                Some(&prior),
                &state,
                b"transition",
            )
        );
        state.last_returned_gameplay_frame_sequence = None;
        state.terminal_event_record_confirmed = true;
        assert_ne!(
            baseline,
            competitive_event_runtime_commitment_v1(
                COMPETITIVE_EVENT_RUNTIME_ADVANCE_DOMAIN_V1,
                Some(&prior),
                &state,
                b"transition",
            )
        );
    }

    #[test]
    fn competitive_event_driver_routes_every_post_entry_phase_and_rejects_drift() {
        let waiting = competitive_event_driver_directive_from_commitments_v1(
            &competitive_event_runtime_commitments_fixture_v1(
                MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing,
            ),
        )
        .unwrap();
        assert_eq!(
            waiting.step,
            MtgoCompetitiveEventDriverStepV1::AwaitPairingOrEventEnd
        );
        assert_eq!(
            waiting.allowed_observed_advances_v1(),
            &[
                MtgoObservedCompetitiveLifecycleAdvanceV1::PairingPosted,
                MtgoObservedCompetitiveLifecycleAdvanceV1::EventEnded,
            ]
        );
        assert!(!waiting.safe_for_live_input_v1());
        assert!(!waiting.permits_event_entry_v1());
        assert!(!waiting.permits_spending_v1());

        let pairing = competitive_event_driver_directive_from_commitments_v1(
            &competitive_event_runtime_commitments_fixture_v1(
                MtgoCompetitiveLifecyclePhaseV1::PairingReady,
            ),
        )
        .unwrap();
        assert_eq!(
            pairing.lifecycle_action_v1(),
            Some(MtgoCompetitiveLifecycleActionV1::AcceptPairing)
        );

        let mut gameplay = competitive_event_runtime_commitments_fixture_v1(
            MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
        );
        assert!(matches!(
            competitive_event_driver_directive_from_commitments_v1(&gameplay)
                .unwrap()
                .step,
            MtgoCompetitiveEventDriverStepV1::ResolvePregame { game_number: 1, .. }
        ));
        gameplay.pregame_session_count = 1;
        gameplay.last_completed_pregame = Some(MtgoCompletedCompetitivePregameCommitmentsV1 {
            completion_receipt_sha256: "b".repeat(64),
            pregame_session_commitment_sha256: "c".repeat(64),
            final_observation_commitment_sha256: "d".repeat(64),
            match_identity_sha256: gameplay.current_match_identity_sha256.clone().unwrap(),
            game_number: 1,
            completion_frame_sequence: gameplay.current_frame_sequence + 1,
        });
        assert!(matches!(
            competitive_event_driver_directive_from_commitments_v1(&gameplay)
                .unwrap()
                .step,
            MtgoCompetitiveEventDriverStepV1::LaunchGameplay { game_number: 1, .. }
        ));
        gameplay.gameplay_lease_count = 1;
        gameplay.last_returned_gameplay_frame_sequence = Some(gameplay.current_frame_sequence + 2);
        let outcome = competitive_event_driver_directive_from_commitments_v1(&gameplay).unwrap();
        assert!(matches!(
            &outcome.step,
            MtgoCompetitiveEventDriverStepV1::AwaitGameOutcome { game_number: 1, .. }
        ));
        assert_eq!(
            outcome.allowed_observed_advances_v1(),
            &[
                MtgoObservedCompetitiveLifecycleAdvanceV1::GameEndedForSideboarding,
                MtgoObservedCompetitiveLifecycleAdvanceV1::MatchEnded,
                MtgoObservedCompetitiveLifecycleAdvanceV1::ConnectionInterrupted,
            ]
        );
        gameplay.current_frame_sequence += 1;
        assert!(matches!(
            competitive_event_driver_directive_from_commitments_v1(&gameplay)
                .unwrap()
                .step,
            MtgoCompetitiveEventDriverStepV1::AwaitGameOutcome { .. }
        ));

        let sideboard = competitive_event_driver_directive_from_commitments_v1(
            &competitive_event_runtime_commitments_fixture_v1(
                MtgoCompetitiveLifecyclePhaseV1::Sideboarding,
            ),
        )
        .unwrap();
        assert!(matches!(
            sideboard.step,
            MtgoCompetitiveEventDriverStepV1::ResolveSideboard { game_number: 1, .. }
        ));
        let continued = competitive_event_driver_directive_from_commitments_v1(
            &competitive_event_runtime_commitments_fixture_v1(
                MtgoCompetitiveLifecyclePhaseV1::MatchComplete,
            ),
        )
        .unwrap();
        assert_eq!(
            continued.lifecycle_action_v1(),
            Some(MtgoCompetitiveLifecycleActionV1::ContinueAfterMatch)
        );

        let resumed = competitive_event_driver_directive_from_commitments_v1(
            &competitive_event_runtime_commitments_fixture_v1(
                MtgoCompetitiveLifecyclePhaseV1::Reconnect,
            ),
        )
        .unwrap();
        assert_eq!(
            resumed.lifecycle_action_v1(),
            Some(MtgoCompetitiveLifecycleActionV1::ResumeMatch)
        );

        let mut completed = competitive_event_runtime_commitments_fixture_v1(
            MtgoCompetitiveLifecyclePhaseV1::EventComplete,
        );
        assert_eq!(
            competitive_event_driver_directive_from_commitments_v1(&completed)
                .unwrap()
                .step,
            MtgoCompetitiveEventDriverStepV1::BeginTerminalEventRecordMonitor
        );
        completed.event_monitor_chain_commitment_sha256 = Some("f".repeat(64));
        completed.event_monitor_observation_count = 1;
        assert_eq!(
            competitive_event_driver_directive_from_commitments_v1(&completed)
                .unwrap()
                .step,
            MtgoCompetitiveEventDriverStepV1::AdvanceTerminalEventRecordMonitor {
                prior_observation_count: 1,
            }
        );
        completed.terminal_event_record_confirmed = true;
        let close = competitive_event_driver_directive_from_commitments_v1(&completed).unwrap();
        assert_eq!(
            close.lifecycle_action_v1(),
            Some(MtgoCompetitiveLifecycleActionV1::CloseCompletedEvent)
        );

        completed.current_phase = MtgoCompetitiveLifecyclePhaseV1::EventBrowser;
        completed.closed_to_event_browser = true;
        assert_eq!(
            competitive_event_driver_directive_from_commitments_v1(&completed)
                .unwrap()
                .step,
            MtgoCompetitiveEventDriverStepV1::Complete
        );

        let pre_entry = competitive_event_runtime_commitments_fixture_v1(
            MtgoCompetitiveLifecyclePhaseV1::EntryReview,
        );
        assert!(competitive_event_driver_directive_from_commitments_v1(&pre_entry).is_err());

        let mut inconsistent = competitive_event_runtime_commitments_fixture_v1(
            MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
        );
        inconsistent.gameplay_lease_count = 1;
        assert!(competitive_event_driver_directive_from_commitments_v1(&inconsistent).is_err());

        let mut inconsistent_monitor = competitive_event_runtime_commitments_fixture_v1(
            MtgoCompetitiveLifecyclePhaseV1::EventComplete,
        );
        inconsistent_monitor.event_monitor_observation_count = 1;
        assert!(
            competitive_event_driver_directive_from_commitments_v1(&inconsistent_monitor).is_err()
        );
        inconsistent_monitor.event_monitor_chain_commitment_sha256 = Some("f".repeat(64));
        inconsistent_monitor.event_monitor_observation_count = 0;
        assert!(
            competitive_event_driver_directive_from_commitments_v1(&inconsistent_monitor).is_err()
        );

        let mut misplaced_terminal = competitive_event_runtime_commitments_fixture_v1(
            MtgoCompetitiveLifecyclePhaseV1::MatchComplete,
        );
        misplaced_terminal.event_monitor_chain_commitment_sha256 = Some("f".repeat(64));
        misplaced_terminal.event_monitor_observation_count = 1;
        misplaced_terminal.terminal_event_record_confirmed = true;
        assert!(
            competitive_event_driver_directive_from_commitments_v1(&misplaced_terminal).is_err()
        );

        let mut missing_match = competitive_event_runtime_commitments_fixture_v1(
            MtgoCompetitiveLifecyclePhaseV1::Sideboarding,
        );
        missing_match.current_match_identity_sha256 = None;
        assert!(competitive_event_driver_directive_from_commitments_v1(&missing_match).is_err());
    }

    #[test]
    fn competitive_event_match_launch_binding_requires_the_exact_paid_runtime_lineage() {
        for event_kind in [
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ] {
            let runtime = MtgoCompetitiveEventRuntimeCommitmentsV1 {
                runtime_commitment_sha256: "0".repeat(64),
                entry_confirmation_receipt_sha256: "1".repeat(64),
                entry_ratification_commitment_sha256: "2".repeat(64),
                entry_authorization_sha256: "3".repeat(64),
                correspondence_sha256: "4".repeat(64),
                permission_review_commitment_sha256: "5".repeat(64),
                deck_list_sha256: "0".repeat(64),
                deck_manifest_sha256: "6".repeat(64),
                deck_format_sha256: "7".repeat(64),
                player_known_current_deck_configuration_commitment_sha256: "3".repeat(64),
                selected_deck_label_sha256: "8".repeat(64),
                selected_deck_region_sha256: "9".repeat(64),
                policy_deployment_commitment_sha256: "a".repeat(64),
                lifecycle_authorization_commitment_sha256: "b".repeat(64),
                mode_authorization_commitment_sha256: "c".repeat(64),
                navigation_profile_commitment_sha256: "d".repeat(64),
                navigation_profile_admission_commitment_sha256: "e".repeat(64),
                approved_account_alias_sha256: "f".repeat(64),
                bound_event_identity_sha256: "1".repeat(64),
                event_kind,
                current_phase: MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
                current_lifecycle_snapshot_commitment_sha256: "2".repeat(64),
                current_match_identity_sha256: Some("4".repeat(64)),
                current_game_number: Some(2),
                current_frame_id: 40,
                current_frame_sequence: 50,
                lifecycle_transition_count: 3,
                confirmed_lifecycle_action_count: 1,
                observed_lifecycle_advance_count: 2,
                pregame_session_count: 0,
                last_completed_pregame: None,
                gameplay_lease_count: 0,
                last_returned_gameplay_frame_sequence: None,
                event_monitor_chain_commitment_sha256: None,
                event_monitor_observation_count: 0,
                terminal_event_record_confirmed: false,
                closed_to_event_browser: false,
            };
            let process = "5".repeat(64);
            let source = MtgoOpaqueCompetitiveLaunchIdentityCommitmentsV1 {
                source_capture_commitment_sha256: "6".repeat(64),
                perception_result_commitment_sha256: "7".repeat(64),
                lifecycle_snapshot_commitment_sha256: "b".repeat(64),
                lifecycle_evaluation_commitment_sha256: "d".repeat(64),
                lifecycle_profile_admission_commitment_sha256: "e".repeat(64),
                process_continuity_commitment_sha256: process.clone(),
                window_continuity_commitment_sha256: "c".repeat(64),
                window_title_sha256: "8".repeat(64),
                event_label_region_sha256: "9".repeat(64),
                launch_identity_commitment_sha256: "a".repeat(64),
                event_kind,
                game_number: 2,
                frame_id: 41,
                frame_sequence: 51,
                captured_at_unix_millis: 101,
            };
            let bound = competitive_event_match_launch_binding_commitments_v1(
                &runtime,
                &process,
                100,
                &source,
                &runtime.bound_event_identity_sha256,
                runtime.current_match_identity_sha256.as_deref().unwrap(),
                &runtime.entry_authorization_sha256,
            )
            .unwrap();
            assert_eq!(
                bound.entry_ratification_commitment_sha256,
                runtime.entry_ratification_commitment_sha256
            );
            assert_eq!(bound.deck_manifest_sha256, runtime.deck_manifest_sha256);
            assert_eq!(bound.event_kind, event_kind);
            assert_eq!(bound.game_number, 2);
            assert_eq!(bound.event_runtime_frame_sequence, 50);
            assert_eq!(bound.launch_frame_sequence, 51);
            assert_ne!(
                bound.event_runtime_lifecycle_snapshot_commitment_sha256,
                bound.launch_lifecycle_snapshot_commitment_sha256
            );

            let mut drifted = source.clone();
            drifted.frame_sequence = runtime.current_frame_sequence;
            assert!(competitive_event_match_launch_binding_commitments_v1(
                &runtime,
                &process,
                100,
                &drifted,
                &runtime.bound_event_identity_sha256,
                runtime.current_match_identity_sha256.as_deref().unwrap(),
                &runtime.entry_authorization_sha256,
            )
            .is_err());
            drifted = source.clone();
            drifted.captured_at_unix_millis = 99;
            assert!(competitive_event_match_launch_binding_commitments_v1(
                &runtime,
                &process,
                100,
                &drifted,
                &runtime.bound_event_identity_sha256,
                runtime.current_match_identity_sha256.as_deref().unwrap(),
                &runtime.entry_authorization_sha256,
            )
            .is_err());
            drifted = source.clone();
            drifted.process_continuity_commitment_sha256 = "d".repeat(64);
            assert!(competitive_event_match_launch_binding_commitments_v1(
                &runtime,
                &process,
                100,
                &drifted,
                &runtime.bound_event_identity_sha256,
                runtime.current_match_identity_sha256.as_deref().unwrap(),
                &runtime.entry_authorization_sha256,
            )
            .is_err());
            assert!(competitive_event_match_launch_binding_commitments_v1(
                &runtime,
                &process,
                100,
                &source,
                &"b".repeat(64),
                runtime.current_match_identity_sha256.as_deref().unwrap(),
                &runtime.entry_authorization_sha256,
            )
            .is_err());
            assert!(competitive_event_match_launch_binding_commitments_v1(
                &runtime,
                &process,
                100,
                &source,
                &runtime.bound_event_identity_sha256,
                &"c".repeat(64),
                &runtime.entry_authorization_sha256,
            )
            .is_err());
            assert!(competitive_event_match_launch_binding_commitments_v1(
                &runtime,
                &process,
                100,
                &source,
                &runtime.bound_event_identity_sha256,
                runtime.current_match_identity_sha256.as_deref().unwrap(),
                &"d".repeat(64),
            )
            .is_err());
            let mut wrong_phase = runtime.clone();
            wrong_phase.current_phase = MtgoCompetitiveLifecyclePhaseV1::Sideboarding;
            assert!(competitive_event_match_launch_binding_commitments_v1(
                &wrong_phase,
                &process,
                100,
                &source,
                &runtime.bound_event_identity_sha256,
                runtime.current_match_identity_sha256.as_deref().unwrap(),
                &runtime.entry_authorization_sha256,
            )
            .is_err());
        }
    }

    #[test]
    fn competitive_event_gameplay_rejects_a_different_exact_entry() {
        let runtime = MtgoCompetitiveEventRuntimeCommitmentsV1 {
            runtime_commitment_sha256: "0".repeat(64),
            entry_confirmation_receipt_sha256: "1".repeat(64),
            entry_ratification_commitment_sha256: "2".repeat(64),
            entry_authorization_sha256: "3".repeat(64),
            correspondence_sha256: "c".repeat(64),
            permission_review_commitment_sha256: "d".repeat(64),
            deck_list_sha256: "3".repeat(64),
            deck_manifest_sha256: "e".repeat(64),
            deck_format_sha256: "f".repeat(64),
            player_known_current_deck_configuration_commitment_sha256: "4".repeat(64),
            selected_deck_label_sha256: "0".repeat(64),
            selected_deck_region_sha256: "1".repeat(64),
            policy_deployment_commitment_sha256: "2".repeat(64),
            lifecycle_authorization_commitment_sha256: "4".repeat(64),
            mode_authorization_commitment_sha256: "5".repeat(64),
            navigation_profile_commitment_sha256: "6".repeat(64),
            navigation_profile_admission_commitment_sha256: "7".repeat(64),
            approved_account_alias_sha256: "8".repeat(64),
            bound_event_identity_sha256: "9".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::League,
            current_phase: MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
            current_lifecycle_snapshot_commitment_sha256: "a".repeat(64),
            current_match_identity_sha256: Some("b".repeat(64)),
            current_game_number: Some(1),
            current_frame_id: 20,
            current_frame_sequence: 30,
            lifecycle_transition_count: 4,
            confirmed_lifecycle_action_count: 1,
            observed_lifecycle_advance_count: 3,
            pregame_session_count: 1,
            last_completed_pregame: Some(MtgoCompletedCompetitivePregameCommitmentsV1 {
                completion_receipt_sha256: "3".repeat(64),
                pregame_session_commitment_sha256: "4".repeat(64),
                final_observation_commitment_sha256: "5".repeat(64),
                match_identity_sha256: "b".repeat(64),
                game_number: 1,
                completion_frame_sequence: 31,
            }),
            gameplay_lease_count: 0,
            last_returned_gameplay_frame_sequence: None,
            event_monitor_chain_commitment_sha256: None,
            event_monitor_observation_count: 0,
            terminal_event_record_confirmed: false,
            closed_to_event_browser: false,
        };
        let game = MtgoCompetitiveGestureGameSessionCommitmentsV1 {
            session_commitment_sha256: "c".repeat(64),
            general_gesture_permission_commitment_sha256: "d".repeat(64),
            mode_authorization_commitment_sha256: runtime
                .mode_authorization_commitment_sha256
                .clone(),
            correspondence_sha256: runtime.correspondence_sha256.clone(),
            permission_review_commitment_sha256: runtime
                .permission_review_commitment_sha256
                .clone(),
            pass_match_launch_commitment_sha256: "e".repeat(64),
            match_gameplay_authorization_commitment_sha256: "f".repeat(64),
            gesture_match_launch_commitment_sha256: "0".repeat(64),
            gesture_evaluation_commitment_sha256: "1".repeat(64),
            gesture_profile_admission_commitment_sha256: "2".repeat(64),
            entry_ratification_commitment_sha256: Some(
                runtime.entry_ratification_commitment_sha256.clone(),
            ),
            selected_deck_label_sha256: Some(runtime.selected_deck_label_sha256.clone()),
            selected_deck_region_sha256: Some(runtime.selected_deck_region_sha256.clone()),
            deck_manifest_sha256: Some(runtime.deck_manifest_sha256.clone()),
            deck_format_sha256: Some(runtime.deck_format_sha256.clone()),
            policy_deployment_commitment_sha256: Some(
                runtime.policy_deployment_commitment_sha256.clone(),
            ),
            event_kind: MtgoCompetitiveEventKindV1::League,
            game_number: 1,
            valid_from_frame_sequence: 30,
            valid_through_frame_sequence: 542,
            last_confirmed_frame_sequence: 30,
            confirmed_action_count: 0,
        };
        let mut gameplay = MtgoCompetitiveMatchGameplayAuthorizationV1 {
            schema_version: 1,
            account_alias_sha256: runtime.approved_account_alias_sha256.clone(),
            written_permission_sha256: "4".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::League,
            event_identity_sha256: runtime.bound_event_identity_sha256.clone(),
            match_identity_sha256: runtime.current_match_identity_sha256.clone().unwrap(),
            game_number: 1,
            entry_authorization_sha256: runtime.entry_authorization_sha256.clone(),
            owner_launch_authorization_sha256: "5".repeat(64),
            exact_match_gameplay_authorized: true,
            valid_through_frame_sequence: 542,
        };

        let mut unbound_game = game.clone();
        unbound_game.entry_ratification_commitment_sha256 = None;
        unbound_game.selected_deck_label_sha256 = None;
        unbound_game.selected_deck_region_sha256 = None;
        unbound_game.deck_manifest_sha256 = None;
        unbound_game.deck_format_sha256 = None;
        unbound_game.policy_deployment_commitment_sha256 = None;
        let deck_binding = competitive_gesture_game_session_event_deck_binding_commitment_v1(
            &unbound_game,
            &runtime,
        )
        .unwrap();
        let mut changed_deck_runtime = runtime.clone();
        changed_deck_runtime.deck_manifest_sha256 = "6".repeat(64);
        assert_ne!(
            deck_binding,
            competitive_gesture_game_session_event_deck_binding_commitment_v1(
                &unbound_game,
                &changed_deck_runtime,
            )
            .unwrap()
        );
        let mut partially_bound = unbound_game.clone();
        partially_bound.deck_manifest_sha256 = Some(runtime.deck_manifest_sha256.clone());
        assert!(
            competitive_gesture_game_session_event_deck_binding_commitment_v1(
                &partially_bound,
                &runtime,
            )
            .is_err()
        );

        validate_game_session_commitments_against_event_runtime_v1(&runtime, &game, &gameplay)
            .unwrap();

        gameplay.entry_authorization_sha256 = "6".repeat(64);
        assert!(validate_game_session_commitments_against_event_runtime_v1(
            &runtime, &game, &gameplay,
        )
        .is_err());

        gameplay.entry_authorization_sha256 = runtime.entry_authorization_sha256.clone();
        let mut mismatched_game = game.clone();
        mismatched_game.correspondence_sha256 = "6".repeat(64);
        assert!(validate_game_session_commitments_against_event_runtime_v1(
            &runtime,
            &mismatched_game,
            &gameplay,
        )
        .is_err());

        mismatched_game.correspondence_sha256 = runtime.correspondence_sha256.clone();
        mismatched_game.permission_review_commitment_sha256 = "6".repeat(64);
        assert!(validate_game_session_commitments_against_event_runtime_v1(
            &runtime,
            &mismatched_game,
            &gameplay,
        )
        .is_err());

        mismatched_game.permission_review_commitment_sha256 =
            runtime.permission_review_commitment_sha256.clone();
        mismatched_game.deck_manifest_sha256 = Some("6".repeat(64));
        assert!(validate_game_session_commitments_against_event_runtime_v1(
            &runtime,
            &mismatched_game,
            &gameplay,
        )
        .is_err());

        mismatched_game.deck_manifest_sha256 = Some(runtime.deck_manifest_sha256.clone());
        mismatched_game.policy_deployment_commitment_sha256 = Some("6".repeat(64));
        assert!(validate_game_session_commitments_against_event_runtime_v1(
            &runtime,
            &mismatched_game,
            &gameplay,
        )
        .is_err());
    }

    #[test]
    fn competitive_event_rejects_split_entry_and_lifecycle_permission_lineage() {
        let entry = MtgoReviewedCompetitiveEntryRatificationCandidateV1 {
            permission_review_commitment_sha256: "1".repeat(64),
            account_alias_sha256: "2".repeat(64),
            correspondence_sha256: "3".repeat(64),
            mode_authorization_commitment_sha256: "4".repeat(64),
            control_bound_review_commitment_sha256: "5".repeat(64),
            owner_review_receipt_sha256: "6".repeat(64),
            entry_authorization_sha256: "7".repeat(64),
            source_identity_commitment_sha256: "8".repeat(64),
            source_capture_commitment_sha256: "9".repeat(64),
            source_navigation_classification_result_commitment_sha256: "a".repeat(64),
            visible_control_region_sha256: "b".repeat(64),
            selected_deck_label_sha256: "f".repeat(64),
            selected_deck_region_sha256: "0".repeat(64),
            deck_manifest_sha256: "1".repeat(64),
            deck_format_sha256: "2".repeat(64),
            policy_deployment_commitment_sha256: "4".repeat(64),
            deck_review_receipt_sha256: "3".repeat(64),
            event_identity_sha256: "c".repeat(64),
            entry_terms_sha256: "d".repeat(64),
            ratification_commitment_sha256: "e".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::Challenge,
            resource: MtgoCompetitiveEntryResourceV1::ExistingEventTickets,
            amount: 25,
        };
        let mut lifecycle = MtgoReviewedCompetitiveLifecycleRatificationCandidateV1 {
            correspondence_sha256: entry.correspondence_sha256.clone(),
            permission_review_commitment_sha256: entry.permission_review_commitment_sha256.clone(),
            mode_authorization_commitment_sha256: entry
                .mode_authorization_commitment_sha256
                .clone(),
            approved_account_alias_sha256: entry.account_alias_sha256.clone(),
            lifecycle_profile_commitment_sha256: "f".repeat(64),
            lifecycle_profile_admission_commitment_sha256: "0".repeat(64),
            allowed_actions_commitment_sha256: "1".repeat(64),
            event_kind: entry.event_kind,
            ratification_commitment_sha256: "2".repeat(64),
        };

        validate_competitive_event_authorization_lineage_v1(&entry, &lifecycle).unwrap();

        lifecycle.correspondence_sha256 = "4".repeat(64);
        assert!(validate_competitive_event_authorization_lineage_v1(&entry, &lifecycle).is_err());

        lifecycle.correspondence_sha256 = entry.correspondence_sha256.clone();
        lifecycle.permission_review_commitment_sha256 = "4".repeat(64);
        assert!(validate_competitive_event_authorization_lineage_v1(&entry, &lifecycle).is_err());

        lifecycle.permission_review_commitment_sha256 =
            entry.permission_review_commitment_sha256.clone();
        lifecycle.mode_authorization_commitment_sha256 = "5".repeat(64);
        assert!(validate_competitive_event_authorization_lineage_v1(&entry, &lifecycle).is_err());
    }

    #[test]
    fn competitive_event_rejects_lifecycle_frames_not_newer_than_returned_gameplay() {
        let mut runtime = MtgoCompetitiveEventRuntimeCommitmentsV1 {
            runtime_commitment_sha256: "0".repeat(64),
            entry_confirmation_receipt_sha256: "1".repeat(64),
            entry_ratification_commitment_sha256: "2".repeat(64),
            entry_authorization_sha256: "3".repeat(64),
            correspondence_sha256: "4".repeat(64),
            permission_review_commitment_sha256: "5".repeat(64),
            deck_list_sha256: "1".repeat(64),
            deck_manifest_sha256: "d".repeat(64),
            deck_format_sha256: "e".repeat(64),
            player_known_current_deck_configuration_commitment_sha256: "5".repeat(64),
            selected_deck_label_sha256: "f".repeat(64),
            selected_deck_region_sha256: "0".repeat(64),
            policy_deployment_commitment_sha256: "6".repeat(64),
            lifecycle_authorization_commitment_sha256: "6".repeat(64),
            mode_authorization_commitment_sha256: "7".repeat(64),
            navigation_profile_commitment_sha256: "8".repeat(64),
            navigation_profile_admission_commitment_sha256: "9".repeat(64),
            approved_account_alias_sha256: "a".repeat(64),
            bound_event_identity_sha256: "b".repeat(64),
            event_kind: MtgoCompetitiveEventKindV1::League,
            current_phase: MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
            current_lifecycle_snapshot_commitment_sha256: "c".repeat(64),
            current_match_identity_sha256: Some("d".repeat(64)),
            current_game_number: Some(1),
            current_frame_id: 20,
            current_frame_sequence: 30,
            lifecycle_transition_count: 4,
            confirmed_lifecycle_action_count: 1,
            observed_lifecycle_advance_count: 3,
            pregame_session_count: 0,
            last_completed_pregame: None,
            gameplay_lease_count: 1,
            last_returned_gameplay_frame_sequence: Some(50),
            event_monitor_chain_commitment_sha256: None,
            event_monitor_observation_count: 0,
            terminal_event_record_confirmed: false,
            closed_to_event_browser: false,
        };

        assert!(validate_competitive_event_next_frame_order_v1(&runtime, 30).is_err());
        assert!(validate_competitive_event_next_frame_order_v1(&runtime, 50).is_err());
        validate_competitive_event_next_frame_order_v1(&runtime, 51).unwrap();

        runtime.last_returned_gameplay_frame_sequence = None;
        validate_competitive_event_next_frame_order_v1(&runtime, 31).unwrap();
    }

    #[test]
    fn atomic_sideboard_sequence_moves_in_before_out_and_applies_one_copy() {
        let out_card_name = "Lightning Bolt";
        let in_card_name = "Searing Blaze";
        let out_card_db_id = mtg_kernel::card_def::card_id_by_name(out_card_name).unwrap();
        let in_card_db_id = mtg_kernel::card_def::card_id_by_name(in_card_name).unwrap();
        let transfers = vec![
            MtgoCompetitiveSideboardTransferV1 {
                card_name: out_card_name.to_owned(),
                direction: MtgoCompetitiveSideboardTransferDirectionV1::MainboardToSideboard,
                count: 2,
            },
            MtgoCompetitiveSideboardTransferV1 {
                card_name: in_card_name.to_owned(),
                direction: MtgoCompetitiveSideboardTransferDirectionV1::SideboardToMainboard,
                count: 2,
            },
        ];
        let atomic = expand_atomic_sideboard_transfers_v1(&transfers).unwrap();
        assert_eq!(atomic.len(), 4);
        assert_eq!(atomic[0].step_index, 0);
        assert_eq!(
            atomic[0].direction,
            MtgoCompetitiveSideboardTransferDirectionV1::SideboardToMainboard
        );
        assert_eq!(atomic[2].step_index, 2);
        assert_eq!(
            atomic[2].direction,
            MtgoCompetitiveSideboardTransferDirectionV1::MainboardToSideboard
        );

        let configuration = MtgoCompetitiveDeckConfigurationV1 {
            mainboard: vec![mtgo_blackbox_v1::MtgoCompetitiveDeckCardCountV1 {
                card_db_id: out_card_db_id,
                card_name: out_card_name.to_owned(),
                count: 2,
            }],
            sideboard: vec![mtgo_blackbox_v1::MtgoCompetitiveDeckCardCountV1 {
                card_db_id: in_card_db_id,
                card_name: in_card_name.to_owned(),
                count: 2,
            }],
        };
        let first = apply_atomic_sideboard_transfer_v1(&configuration, &atomic[0]).unwrap();
        assert_eq!(
            first
                .mainboard
                .iter()
                .find(|card| card.card_name == in_card_name)
                .unwrap()
                .count,
            1
        );
        assert_eq!(first.sideboard[0].card_name, in_card_name);
        assert_eq!(first.sideboard[0].count, 1);
        let second = apply_atomic_sideboard_transfer_v1(&first, &atomic[1]).unwrap();
        assert_eq!(
            second
                .mainboard
                .iter()
                .find(|card| card.card_name == in_card_name)
                .unwrap()
                .count,
            2
        );
        assert!(second.sideboard.is_empty());
        let third = apply_atomic_sideboard_transfer_v1(&second, &atomic[2]).unwrap();
        assert_eq!(
            third
                .mainboard
                .iter()
                .find(|card| card.card_name == out_card_name)
                .unwrap()
                .count,
            1
        );
        assert_eq!(third.sideboard[0].card_name, out_card_name);
        assert_eq!(third.sideboard[0].count, 1);
        let fourth = apply_atomic_sideboard_transfer_v1(&third, &atomic[3]).unwrap();
        assert_eq!(fourth.mainboard.len(), 1);
        assert_eq!(fourth.mainboard[0].card_name, in_card_name);
        assert_eq!(fourth.mainboard[0].card_db_id, in_card_db_id);
        assert_eq!(fourth.sideboard[0].card_db_id, out_card_db_id);
        assert_eq!(fourth.sideboard[0].count, 2);
    }

    #[test]
    fn atomic_sideboard_application_rejects_absent_or_unknown_visible_name() {
        let exact_card_name = "Lightning Bolt";
        let configuration = MtgoCompetitiveDeckConfigurationV1 {
            mainboard: vec![mtgo_blackbox_v1::MtgoCompetitiveDeckCardCountV1 {
                card_db_id: mtg_kernel::card_def::card_id_by_name(exact_card_name).unwrap(),
                card_name: exact_card_name.to_owned(),
                count: 1,
            }],
            sideboard: Vec::new(),
        };
        for card_name in ["Searing Blaze", "Not A Kernel Card"] {
            let transfer = MtgoAtomicCompetitiveSideboardTransferV1 {
                step_index: 0,
                card_name: card_name.to_owned(),
                direction: MtgoCompetitiveSideboardTransferDirectionV1::MainboardToSideboard,
            };
            assert!(apply_atomic_sideboard_transfer_v1(&configuration, &transfer).is_err());
        }
    }

    fn competitive_pregame_test_cards_v1() -> Vec<MtgoCompetitivePregameVisibleCardV1> {
        (0_u8..7)
            .map(|card_slot| MtgoCompetitivePregameVisibleCardV1 {
                card_slot,
                visible_card_name: format!("Card {card_slot}"),
                rect_client_px: mtgo_blackbox_v1::MtgoRectPxV1 {
                    x: u32::from(card_slot) * 10,
                    y: 10,
                    width: 8,
                    height: 8,
                },
                visible_content_sha256: "a".repeat(64),
                confidence_bps: 10_000,
            })
            .collect()
    }

    fn competitive_pregame_bottoming_response_v1(
        required_bottom_count: u8,
        selected_slots: &[u8],
    ) -> mtgo_blackbox_v1::MtgoCompetitivePregameClassifierResponseV1 {
        let visible_controls = (0_u8..7)
            .map(|card_slot| {
                let selected = selected_slots.contains(&card_slot);
                MtgoCompetitivePregameVisibleControlV1 {
                    control_id: format!("bottom_card_{card_slot}"),
                    semantic: MtgoCompetitivePregameVisibleControlSemanticV1::SelectForBottom {
                        card_slot,
                        selected,
                    },
                    rect_client_px: mtgo_blackbox_v1::MtgoRectPxV1 {
                        x: u32::from(card_slot) * 10,
                        y: 20,
                        width: 8,
                        height: 8,
                    },
                    visible_content_sha256: if selected {
                        "b".repeat(64)
                    } else {
                        "a".repeat(64)
                    },
                    confidence_bps: 10_000,
                    visibly_enabled: true,
                }
            })
            .collect();
        mtgo_blackbox_v1::MtgoCompetitivePregameClassifierResponseV1 {
            schema_version: 1,
            request_commitment_sha256: "c".repeat(64),
            stage: MtgoCompetitivePregameStageLabelV1::LondonBottoming {
                required_bottom_count,
                selected_bottom_count: u8::try_from(selected_slots.len()).unwrap(),
            },
            visible_facts: Vec::new(),
            visible_cards: competitive_pregame_test_cards_v1(),
            visible_controls,
            visible_interaction_commitment_sha256: "d".repeat(64),
        }
    }

    fn competitive_native_pregame_mulligan_response_v1(
        prospective_keep_size: u8,
    ) -> mtgo_blackbox_v1::MtgoCompetitivePregameClassifierResponseV1 {
        let mut visible_controls = vec![MtgoCompetitivePregameVisibleControlV1 {
            control_id: "keep_opening_hand".to_owned(),
            semantic: MtgoCompetitivePregameVisibleControlSemanticV1::KeepOpeningHand,
            rect_client_px: mtgo_blackbox_v1::MtgoRectPxV1 {
                x: 10,
                y: 30,
                width: 8,
                height: 8,
            },
            visible_content_sha256: "a".repeat(64),
            confidence_bps: 10_000,
            visibly_enabled: true,
        }];
        if prospective_keep_size > 0 {
            visible_controls.push(MtgoCompetitivePregameVisibleControlV1 {
                control_id: "mulligan".to_owned(),
                semantic: MtgoCompetitivePregameVisibleControlSemanticV1::Mulligan {
                    next_hand_size: prospective_keep_size - 1,
                },
                rect_client_px: mtgo_blackbox_v1::MtgoRectPxV1 {
                    x: 20,
                    y: 30,
                    width: 8,
                    height: 8,
                },
                visible_content_sha256: "b".repeat(64),
                confidence_bps: 10_000,
                visibly_enabled: true,
            });
        }
        mtgo_blackbox_v1::MtgoCompetitivePregameClassifierResponseV1 {
            schema_version: 1,
            request_commitment_sha256: "c".repeat(64),
            stage: MtgoCompetitivePregameStageLabelV1::MulliganChoice {
                prospective_keep_size,
            },
            visible_facts: Vec::new(),
            visible_cards: competitive_pregame_test_cards_v1(),
            visible_controls,
            visible_interaction_commitment_sha256: "d".repeat(64),
        }
    }

    fn competitive_native_pregame_context_fixture_v1(
        response: &mtgo_blackbox_v1::MtgoCompetitivePregameClassifierResponseV1,
        game_number: u8,
        acting_player_games_won: u8,
        opponent_games_won: u8,
    ) -> crate::probe::MtgoClassifiedCompetitivePregameModelContextCommitmentsV1 {
        crate::probe::competitive_pregame_model_context_commitments_for_tests_v1(
            response.stage,
            response.visible_interaction_commitment_sha256.clone(),
            game_number,
            mtgo_blackbox_v1::MtgoCompetitivePregamePlayDrawV1::OnPlay,
            acting_player_games_won,
            opponent_games_won,
        )
    }

    #[test]
    fn native_pregame_request_is_exact_visible_game_one_candidate_only() {
        let (runtime, deck_manifest) = competitive_native_pregame_runtime_and_deck_v1();
        let response = competitive_native_pregame_mulligan_response_v1(7);
        let context = competitive_native_pregame_context_fixture_v1(&response, 1, 0, 0);
        let model_input = competitive_native_pregame_model_input_from_checked_parts_for_tests_v1(
            &runtime,
            &context,
            &response,
            &deck_manifest,
        )
        .unwrap();

        validate_competitive_native_pregame_model_input_v1(&model_input).unwrap();
        assert_eq!(model_input.game_number, 1);
        assert_eq!(model_input.ordered_visible_cards.len(), 7);
        assert_eq!(
            model_input
                .player_known_deck_configuration
                .mainboard
                .iter()
                .map(|card| u32::from(card.count))
                .sum::<u32>(),
            60
        );
        assert_eq!(
            model_input.ordered_actions,
            vec![
                MtgoCompetitiveNativePregameActionV1::KeepOpeningHand,
                MtgoCompetitiveNativePregameActionV1::Mulligan { next_hand_size: 6 }
            ]
        );
        assert!(model_input.ordered_confirmed_bottom_slots.is_empty());
        assert!(is_sha256_v2(
            &competitive_native_pregame_model_input_commitment_v1(&model_input).unwrap()
        ));
        let serialized = serde_json::to_string(&model_input).unwrap();
        for forbidden in [
            "sha256",
            "schema_version",
            "complete",
            "event_kind",
            "event_identity",
            "match_identity",
            "authorization",
            "capture",
            "classifier",
            "card_db_id",
            "policy_deployment",
        ] {
            assert!(!serialized.contains(forbidden));
        }
    }

    #[test]
    fn native_pregame_request_rejects_a_different_player_known_current_deck() {
        let (mut runtime, deck_manifest) = competitive_native_pregame_runtime_and_deck_v1();
        runtime.player_known_current_deck_configuration_commitment_sha256 = "f".repeat(64);
        let response = competitive_native_pregame_mulligan_response_v1(7);
        let context = competitive_native_pregame_context_fixture_v1(&response, 1, 0, 0);

        assert!(
            competitive_native_pregame_model_input_from_checked_parts_for_tests_v1(
                &runtime,
                &context,
                &response,
                &deck_manifest,
            )
            .unwrap_err()
            .contains("exact player-known current deck configuration")
        );
    }

    #[test]
    fn native_pregame_request_accepts_later_game_player_known_deck_and_exact_bottom_history() {
        let (mut runtime, deck_manifest) = competitive_native_pregame_runtime_and_deck_v1();
        let mut changed =
            visible_native_sideboard_configuration_v1(deck_manifest.configuration_v1()).unwrap();
        let bolt = changed
            .mainboard
            .iter_mut()
            .find(|card| card.visible_card_name == "Lightning Bolt")
            .unwrap();
        bolt.count -= 1;
        let blaze = changed
            .sideboard
            .iter_mut()
            .find(|card| card.visible_card_name == "Searing Blaze")
            .unwrap();
        blaze.count -= 1;
        changed
            .mainboard
            .push(crate::MtgoCompetitiveNativeSideboardCardCountV1 {
                visible_card_name: "Searing Blaze".to_owned(),
                count: 1,
            });
        changed
            .sideboard
            .push(crate::MtgoCompetitiveNativeSideboardCardCountV1 {
                visible_card_name: "Lightning Bolt".to_owned(),
                count: 1,
            });
        changed
            .mainboard
            .sort_by(|left, right| left.visible_card_name.cmp(&right.visible_card_name));
        changed
            .sideboard
            .sort_by(|left, right| left.visible_card_name.cmp(&right.visible_card_name));
        runtime.player_known_current_deck_configuration_commitment_sha256 =
            competitive_native_sideboard_configuration_commitment_v1(&changed).unwrap();
        runtime.current_game_number = Some(2);
        let game_two_response = competitive_native_pregame_mulligan_response_v1(7);
        let game_two_context =
            competitive_native_pregame_context_fixture_v1(&game_two_response, 2, 1, 0);
        let game_two = competitive_native_pregame_model_input_from_parts_v1(
            &runtime,
            &game_two_context,
            &game_two_response,
            &changed,
            &[],
        )
        .unwrap();
        assert_eq!(game_two.game_number, 2);
        assert_eq!(game_two.player_known_deck_configuration, changed);

        runtime.current_game_number = Some(3);
        let game_three_response = competitive_native_pregame_mulligan_response_v1(7);
        let game_three_context =
            competitive_native_pregame_context_fixture_v1(&game_three_response, 3, 1, 1);
        let game_three = competitive_native_pregame_model_input_from_parts_v1(
            &runtime,
            &game_three_context,
            &game_three_response,
            &changed,
            &[],
        )
        .unwrap();
        assert_eq!(game_three.game_number, 3);
        assert_eq!(game_three.player_known_deck_configuration, changed);

        let invalid_game_three_context =
            competitive_native_pregame_context_fixture_v1(&game_three_response, 3, 2, 0);
        assert!(competitive_native_pregame_model_input_from_parts_v1(
            &runtime,
            &invalid_game_three_context,
            &game_three_response,
            &changed,
            &[],
        )
        .is_err());

        runtime.current_game_number = Some(1);
        runtime.player_known_current_deck_configuration_commitment_sha256 =
            competitive_native_sideboard_configuration_commitment_v1(
                &visible_native_sideboard_configuration_v1(deck_manifest.configuration_v1())
                    .unwrap(),
            )
            .unwrap();
        let bottoming_response = competitive_pregame_bottoming_response_v1(2, &[3]);
        let bottoming_context =
            competitive_native_pregame_context_fixture_v1(&bottoming_response, 1, 0, 0);
        let submitted =
            visible_native_sideboard_configuration_v1(deck_manifest.configuration_v1()).unwrap();
        let retained = competitive_native_pregame_model_input_from_parts_v1(
            &runtime,
            &bottoming_context,
            &bottoming_response,
            &submitted,
            &[3],
        )
        .unwrap();
        assert_eq!(retained.ordered_confirmed_bottom_slots, vec![3]);
        assert!(retained.ordered_visible_cards[3].selected_for_bottom);
        assert!(competitive_native_pregame_model_input_from_parts_v1(
            &runtime,
            &bottoming_context,
            &bottoming_response,
            &submitted,
            &[],
        )
        .unwrap_err()
        .contains("ordered confirmed bottom history"));
        assert!(competitive_native_pregame_model_input_from_parts_v1(
            &runtime,
            &bottoming_context,
            &bottoming_response,
            &submitted,
            &[2],
        )
        .unwrap_err()
        .contains("differs from the visible selected cards"));
    }

    #[test]
    fn player_known_deck_state_updates_from_confirmed_target_and_resets_after_match() {
        let deck_manifest = competitive_native_pregame_deck_manifest_v1();
        let mut state =
            MtgoCompetitivePlayerKnownDeckStateV1::from_manifest_v1(&deck_manifest).unwrap();
        let submitted = state.current.clone();
        let mut changed = submitted.clone();
        changed.mainboard[0].count -= 1;
        changed.sideboard[0].count -= 1;
        changed
            .mainboard
            .push(crate::MtgoCompetitiveNativeSideboardCardCountV1 {
                visible_card_name: changed.sideboard[0].visible_card_name.clone(),
                count: 1,
            });
        changed
            .sideboard
            .push(crate::MtgoCompetitiveNativeSideboardCardCountV1 {
                visible_card_name: changed.mainboard[0].visible_card_name.clone(),
                count: 1,
            });
        changed
            .mainboard
            .sort_by(|left, right| left.visible_card_name.cmp(&right.visible_card_name));
        changed
            .sideboard
            .sort_by(|left, right| left.visible_card_name.cmp(&right.visible_card_name));

        state.replace_current_v1(changed.clone()).unwrap();
        assert_eq!(state.current, changed);
        assert_ne!(state.current, submitted);

        state.reset_for_next_match_v1();
        assert_eq!(state.current, submitted);
    }

    #[test]
    fn native_pregame_request_detects_semantic_mutation() {
        let (runtime, deck_manifest) = competitive_native_pregame_runtime_and_deck_v1();
        let response = competitive_native_pregame_mulligan_response_v1(7);
        let context = competitive_native_pregame_context_fixture_v1(&response, 1, 0, 0);
        let model_input = competitive_native_pregame_model_input_from_checked_parts_for_tests_v1(
            &runtime,
            &context,
            &response,
            &deck_manifest,
        )
        .unwrap();

        let original_commitment =
            competitive_native_pregame_model_input_commitment_v1(&model_input).unwrap();
        let mut changed_card = model_input.clone();
        changed_card.ordered_visible_cards[0].visible_card_name = "Different".to_owned();
        assert_ne!(
            competitive_native_pregame_model_input_commitment_v1(&changed_card).unwrap(),
            original_commitment
        );

        let mut changed_deck = model_input.clone();
        changed_deck.player_known_deck_configuration.mainboard[0].count -= 1;
        assert_ne!(
            competitive_native_pregame_model_input_commitment_v1(&changed_deck).unwrap(),
            original_commitment
        );

        let mut changed_action = model_input;
        changed_action.ordered_actions.swap(0, 1);
        assert!(validate_competitive_native_pregame_model_input_v1(&changed_action).is_err());
    }

    #[test]
    fn native_pregame_request_rejects_reordered_london_actions_and_bad_history() {
        let (runtime, deck_manifest) = competitive_native_pregame_runtime_and_deck_v1();
        let response = competitive_pregame_bottoming_response_v1(2, &[]);
        let context = competitive_native_pregame_context_fixture_v1(&response, 1, 0, 0);
        let model_input = competitive_native_pregame_model_input_from_checked_parts_for_tests_v1(
            &runtime,
            &context,
            &response,
            &deck_manifest,
        )
        .unwrap();
        validate_competitive_native_pregame_model_input_v1(&model_input).unwrap();

        let mut reordered = model_input.clone();
        reordered.ordered_actions.swap(0, 1);
        assert!(validate_competitive_native_pregame_model_input_v1(&reordered).is_err());

        let mut duplicate_history = model_input;
        duplicate_history.stage = MtgoCompetitivePregameStageV1::LondonBottoming {
            required_bottom_count: 2,
            selected_bottom_count: 2,
        };
        duplicate_history.selected_bottom_count = 2;
        duplicate_history.ordered_confirmed_bottom_slots = vec![0, 0];
        duplicate_history.ordered_visible_cards[0].selected_for_bottom = true;
        duplicate_history.ordered_actions = duplicate_history
            .ordered_visible_cards
            .iter()
            .filter(|card| !card.selected_for_bottom)
            .map(
                |card| MtgoCompetitiveNativePregameActionV1::SelectForBottom {
                    card_slot: card.card_slot,
                },
            )
            .chain(std::iter::once(
                MtgoCompetitiveNativePregameActionV1::SubmitBottoming,
            ))
            .collect();
        assert!(validate_competitive_native_pregame_model_input_v1(&duplicate_history).is_err());
    }

    #[test]
    fn competitive_pregame_bottom_history_requires_each_exact_confirmed_visible_increment() {
        let start = MtgoCompetitivePregameStageV1::LondonBottoming {
            required_bottom_count: 2,
            selected_bottom_count: 0,
        };
        let one = MtgoCompetitivePregameStageV1::LondonBottoming {
            required_bottom_count: 2,
            selected_bottom_count: 1,
        };
        let two = MtgoCompetitivePregameStageV1::LondonBottoming {
            required_bottom_count: 2,
            selected_bottom_count: 2,
        };

        let history = next_competitive_pregame_bottom_history_v1(start, one, &[], Some(3)).unwrap();
        assert_eq!(history, vec![3]);
        let history =
            next_competitive_pregame_bottom_history_v1(one, two, &history, Some(1)).unwrap();
        assert_eq!(history, vec![3, 1]);

        assert!(next_competitive_pregame_bottom_history_v1(start, one, &[], None).is_err());
        assert!(next_competitive_pregame_bottom_history_v1(one, two, &[3], Some(3),).is_err());
        assert!(next_competitive_pregame_bottom_history_v1(one, one, &[3], Some(1),).is_err());
    }

    #[test]
    fn native_sideboard_score_uses_only_prior_visible_winner_and_game_number() {
        assert_eq!(
            native_sideboard_score_from_prior_game_v1(
                1,
                MtgoCompetitivePlayerRelativeGameWinnerV1::ActingPlayer,
            )
            .unwrap(),
            (1, 0)
        );
        assert_eq!(
            native_sideboard_score_from_prior_game_v1(
                1,
                MtgoCompetitivePlayerRelativeGameWinnerV1::Opponent,
            )
            .unwrap(),
            (0, 1)
        );
        for winner in [
            MtgoCompetitivePlayerRelativeGameWinnerV1::ActingPlayer,
            MtgoCompetitivePlayerRelativeGameWinnerV1::Opponent,
        ] {
            assert_eq!(
                native_sideboard_score_from_prior_game_v1(2, winner).unwrap(),
                (1, 1)
            );
            assert!(native_sideboard_score_from_prior_game_v1(3, winner).is_err());
        }
    }

    #[test]
    fn competitive_pregame_actions_declare_exact_visible_postconditions() {
        for (stage, action, expected) in [
            (
                MtgoCompetitivePregameStageLabelV1::MulliganChoice {
                    prospective_keep_size: 7,
                },
                MtgoCompetitivePregameSelectedActionV1::KeepOpeningHand,
                MtgoCompetitivePregameExpectedPostconditionV1::GameplayReady,
            ),
            (
                MtgoCompetitivePregameStageLabelV1::MulliganChoice {
                    prospective_keep_size: 6,
                },
                MtgoCompetitivePregameSelectedActionV1::KeepOpeningHand,
                MtgoCompetitivePregameExpectedPostconditionV1::LondonBottoming {
                    required_bottom_count: 1,
                    selected_bottom_count: 0,
                    newly_selected_card_slot: None,
                },
            ),
            (
                MtgoCompetitivePregameStageLabelV1::MulliganChoice {
                    prospective_keep_size: 0,
                },
                MtgoCompetitivePregameSelectedActionV1::KeepOpeningHand,
                MtgoCompetitivePregameExpectedPostconditionV1::LondonBottoming {
                    required_bottom_count: 7,
                    selected_bottom_count: 0,
                    newly_selected_card_slot: None,
                },
            ),
            (
                MtgoCompetitivePregameStageLabelV1::MulliganChoice {
                    prospective_keep_size: 7,
                },
                MtgoCompetitivePregameSelectedActionV1::Mulligan { next_hand_size: 6 },
                MtgoCompetitivePregameExpectedPostconditionV1::MulliganChoice {
                    prospective_keep_size: 6,
                },
            ),
            (
                MtgoCompetitivePregameStageLabelV1::LondonBottoming {
                    required_bottom_count: 2,
                    selected_bottom_count: 0,
                },
                MtgoCompetitivePregameSelectedActionV1::SelectForBottom {
                    card_slot: 3,
                    visible_card_name: "Card 3".to_owned(),
                },
                MtgoCompetitivePregameExpectedPostconditionV1::LondonBottoming {
                    required_bottom_count: 2,
                    selected_bottom_count: 1,
                    newly_selected_card_slot: Some(3),
                },
            ),
            (
                MtgoCompetitivePregameStageLabelV1::LondonBottoming {
                    required_bottom_count: 2,
                    selected_bottom_count: 2,
                },
                MtgoCompetitivePregameSelectedActionV1::SubmitBottoming,
                MtgoCompetitivePregameExpectedPostconditionV1::GameplayReady,
            ),
        ] {
            assert_eq!(
                competitive_pregame_expected_postcondition_v1(stage, &action).unwrap(),
                expected
            );
        }
        assert!(competitive_pregame_expected_postcondition_v1(
            MtgoCompetitivePregameStageLabelV1::MulliganChoice {
                prospective_keep_size: 7,
            },
            &MtgoCompetitivePregameSelectedActionV1::Mulligan { next_hand_size: 5 },
        )
        .is_err());
    }

    #[test]
    fn competitive_pregame_permission_is_one_mode_heuristic_bound_and_production_empty() {
        let heuristic =
            crate::competitive_pregame_policy::admit_competitive_pregame_heuristic_for_tests_v1();
        let league = review_competitive_pregame_ratification_candidate_from_correspondence_v1(
            &checked_competitive_correspondence_v2(),
            "UnbuckledPie",
            MtgoCompetitiveEventKindV1::League,
            &heuristic,
        )
        .unwrap();
        let challenge = review_competitive_pregame_ratification_candidate_from_correspondence_v1(
            &checked_competitive_correspondence_v2(),
            "UnbuckledPie",
            MtgoCompetitiveEventKindV1::Challenge,
            &heuristic,
        )
        .unwrap();
        assert_ne!(
            league.ratification_commitment_sha256,
            challenge.ratification_commitment_sha256
        );
        assert_eq!(league.event_kind, MtgoCompetitiveEventKindV1::League);
        assert_eq!(challenge.event_kind, MtgoCompetitiveEventKindV1::Challenge);
        assert!(
            ratify_competitive_pregame_authorization_from_correspondence_v1(
                checked_competitive_correspondence_v2(),
                "UnbuckledPie".to_owned(),
                MtgoCompetitiveEventKindV1::League,
                &heuristic,
            )
            .is_err()
        );

        let admitted =
            ratify_competitive_pregame_authorization_from_correspondence_with_commitment_v1(
                checked_competitive_correspondence_v2(),
                "UnbuckledPie".to_owned(),
                MtgoCompetitiveEventKindV1::League,
                &heuristic,
                Some(&league.ratification_commitment_sha256),
            )
            .unwrap();
        assert_eq!(
            admitted.commitments_v1().ratification_commitment_sha256,
            league.ratification_commitment_sha256
        );
        assert!(!admitted.safe_for_input_v1());
        assert!(!admitted.permits_event_entry_v1());
        assert!(!admitted.permits_spending_v1());
    }

    #[test]
    fn competitive_pregame_bottom_selection_requires_exact_card_toggle() {
        let before = competitive_pregame_bottoming_response_v1(2, &[]);
        let mut after = competitive_pregame_bottoming_response_v1(2, &[3]);
        let action = MtgoCompetitivePregameSelectedActionV1::SelectForBottom {
            card_slot: 3,
            visible_card_name: "Card 3".to_owned(),
        };
        let expected = MtgoCompetitivePregameExpectedPostconditionV1::LondonBottoming {
            required_bottom_count: 2,
            selected_bottom_count: 1,
            newly_selected_card_slot: Some(3),
        };
        validate_competitive_pregame_expected_postcondition_v1(&expected, &action, &before, &after)
            .unwrap();

        after.visible_cards[3].visible_card_name = "Substituted".to_owned();
        assert!(validate_competitive_pregame_expected_postcondition_v1(
            &expected, &action, &before, &after,
        )
        .is_err());
        after.visible_cards[3].visible_card_name = "Card 3".to_owned();
        after.visible_controls[3].visible_content_sha256 = "a".repeat(64);
        assert!(validate_competitive_pregame_expected_postcondition_v1(
            &expected, &action, &before, &after,
        )
        .is_err());
        after.visible_controls[3].visible_content_sha256 = "b".repeat(64);
        after.visible_controls[3].rect_client_px.x += 1;
        assert!(validate_competitive_pregame_expected_postcondition_v1(
            &expected, &action, &before, &after,
        )
        .is_err());

        let before_one_selected = competitive_pregame_bottoming_response_v1(2, &[0]);
        let after_exact = competitive_pregame_bottoming_response_v1(2, &[0, 3]);
        let expected_two = MtgoCompetitivePregameExpectedPostconditionV1::LondonBottoming {
            required_bottom_count: 2,
            selected_bottom_count: 2,
            newly_selected_card_slot: Some(3),
        };
        validate_competitive_pregame_expected_postcondition_v1(
            &expected_two,
            &action,
            &before_one_selected,
            &after_exact,
        )
        .unwrap();
        let after_swapped = competitive_pregame_bottoming_response_v1(2, &[1, 3]);
        assert!(validate_competitive_pregame_expected_postcondition_v1(
            &expected_two,
            &action,
            &before_one_selected,
            &after_swapped,
        )
        .is_err());
    }
}
