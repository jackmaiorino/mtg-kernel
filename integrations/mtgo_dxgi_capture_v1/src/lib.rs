use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[cfg(target_os = "windows")]
mod actuator;

#[cfg(target_os = "windows")]
mod competitive_wiring_readiness;

#[cfg(target_os = "windows")]
mod competitive_model_decision_readiness;

#[cfg(target_os = "windows")]
mod competitive_auxiliary_model_scoring;

#[cfg(target_os = "windows")]
mod competitive_auxiliary_action_resolution;

#[cfg(target_os = "windows")]
mod competitive_native_sideboard;

#[cfg(target_os = "windows")]
mod competitive_operator_bootstrap;
#[cfg(target_os = "windows")]
mod competitive_operator_loop;
#[cfg(target_os = "windows")]
mod competitive_pre_entry_operator;
#[cfg(target_os = "windows")]
mod competitive_pregame_policy;
#[cfg(target_os = "windows")]
mod competitive_visible_history_readiness;
#[cfg(target_os = "windows")]
mod competitive_visible_match_memory;

#[cfg(target_os = "windows")]
mod probe;

#[cfg(target_os = "windows")]
pub use actuator::{
    advance_competitive_event_monitor_in_runtime_v1,
    advance_competitive_event_pregame_from_classified_frame_v2,
    advance_competitive_event_pregame_observed_v1, advance_competitive_event_runtime_observed_v1,
    attach_competitive_event_monitor_to_runtime_v1, begin_competitive_event_runtime_after_entry_v1,
    begin_competitive_event_sideboard_transfer_sequence_v1, begin_competitive_game_session_v1,
    begin_competitive_gesture_game_session_v1, bind_competitive_duel_gesture_sequence_session_v1,
    bind_competitive_entry_postcondition_dry_run_v1,
    bind_competitive_event_native_sideboard_request_v1,
    bind_competitive_event_pregame_native_request_from_session_v1,
    bind_competitive_event_pregame_native_request_v1,
    bind_competitive_event_pregame_native_request_with_completed_history_v2,
    bind_competitive_event_runtime_to_match_launch_identity_v1,
    bind_confirmed_competitive_open_entry_review_to_entry_review_v1,
    bind_prepared_competitive_duel_pass_session_v2,
    check_competitive_event_pregame_postcondition_dry_run_v1,
    checkout_competitive_event_gameplay_session_v1,
    checkout_competitive_event_pregame_session_from_classified_frame_v2,
    checkout_competitive_event_pregame_session_v1,
    competitive_authorization_ratification_readiness_v1,
    competitive_native_pregame_model_input_commitment_v1,
    complete_competitive_event_pregame_session_v1,
    complete_competitive_event_pregame_session_with_history_v2,
    confirm_competitive_event_sideboard_transfer_visible_v1,
    confirm_pending_competitive_duel_gesture_continuation_v1,
    confirm_pending_competitive_duel_gesture_primitive_v1,
    confirm_pending_competitive_duel_pass_v2, confirm_pending_competitive_entry_v1,
    confirm_pending_competitive_event_lifecycle_control_v1,
    confirm_pending_competitive_event_sideboard_drag_v1,
    confirm_pending_competitive_lifecycle_control_v1,
    confirm_pending_competitive_open_entry_review_v1,
    confirm_pending_competitive_pregame_action_v1, confirm_pending_pregame_keep_to_bottom_six_v3,
    confirm_pending_pregame_keep_to_first_main_v3, confirm_pending_pregame_mulligan_v3,
    execute_authorized_competitive_duel_pass_v1,
    execute_authorized_private_match_pregame_action_v3,
    execute_fresh_competitive_event_sideboard_drag_v1,
    execute_prepared_competitive_duel_gesture_primitive_v1, execute_prepared_competitive_entry_v1,
    execute_prepared_competitive_event_lifecycle_control_v1,
    execute_prepared_competitive_lifecycle_control_v1,
    execute_prepared_competitive_open_entry_review_v1,
    measure_competitive_event_runtime_sideboard_v1, mtgo_input_gate_status_v3,
    next_competitive_event_driver_directive_v1, plan_competitive_event_pregame_action_v1,
    pregame_input_gate_status_v3, prepare_competitive_event_runtime_lifecycle_control_v1,
    prepare_competitive_event_sideboard_transfer_drag_v1,
    prepare_confirmed_competitive_duel_gesture_continuation_stage_v1,
    prepare_fresh_competitive_event_pregame_action_v1,
    prepare_fresh_competitive_event_sideboard_transfer_drag_v1,
    prepare_ratified_competitive_entry_v1, prepare_ratified_competitive_lifecycle_control_v1,
    prepare_ratified_competitive_open_entry_review_v1,
    prepare_ready_competitive_event_sideboard_submit_v1,
    prepare_session_bound_competitive_duel_gesture_source_stage_v1,
    ratify_competitive_duel_gesture_authorization_from_correspondence_v1,
    ratify_competitive_duel_pass_authorization_from_correspondence_v2,
    ratify_competitive_duel_pass_authorization_v1,
    ratify_competitive_entry_authorization_from_selected_listing_v2,
    ratify_competitive_event_match_launch_attended_v5,
    ratify_competitive_gesture_match_launch_attended_v1,
    ratify_competitive_lifecycle_authorization_from_correspondence_v1,
    ratify_competitive_open_entry_review_authorization_v1,
    ratify_competitive_pregame_authorization_from_correspondence_v1,
    ratify_competitive_sideboard_automation_v1, ratify_private_match_authorization_v3,
    return_competitive_event_gameplay_session_v1,
    review_competitive_duel_gesture_ratification_candidate_from_correspondence_v1,
    review_competitive_duel_pass_ratification_candidate_from_correspondence_v2,
    review_competitive_entry_attended_v1, review_competitive_entry_attended_v2,
    review_competitive_entry_attended_v3, review_competitive_entry_attended_v4,
    review_competitive_entry_ratification_candidate_from_selected_listing_v2,
    review_competitive_entry_ratification_candidate_v1,
    review_competitive_lifecycle_ratification_candidate_from_correspondence_v1,
    review_competitive_open_entry_review_ratification_candidate_v1,
    review_competitive_pregame_ratification_candidate_from_correspondence_v1,
    review_competitive_sideboard_automation_ratification_candidate_v1,
    validate_competitive_native_pregame_model_input_v1,
    CheckedUntrustedMtgoAttendedCompetitiveEntryReviewV1,
    CheckedUntrustedMtgoClassifierBoundCompetitiveEntryReviewV3,
    CheckedUntrustedMtgoCompetitiveEntryPostconditionDryRunV1,
    CheckedUntrustedMtgoCompetitivePregamePostconditionV1,
    CheckedUntrustedMtgoControlBoundCompetitiveEntryReviewV4,
    CheckedUntrustedMtgoSelectedListingBoundCompetitiveEntryReviewV1,
    CheckedUntrustedMtgoSourceBoundCompetitiveEntryReviewV2,
    MtgoAtomicCompetitiveSideboardTransferV1, MtgoAttendedCompetitiveEntryReviewCommitmentsV1,
    MtgoAuthorizationBoundCompetitiveDuelPassCommitmentsV1,
    MtgoCheckedCompetitivePregamePostconditionCommitmentsV1,
    MtgoClassifierBoundCompetitiveEntryReviewCommitmentsV3,
    MtgoCompetitiveAuthorizationRatificationReadinessV1,
    MtgoCompetitiveEntryInputReceiptCommitmentsV1,
    MtgoCompetitiveEntryPostconditionDryRunCommitmentsV1, MtgoCompetitiveEventDriverDirectiveV1,
    MtgoCompetitiveEventDriverStepV1, MtgoCompetitiveEventGameplayLeaseCommitmentsV1,
    MtgoCompetitiveEventMatchLaunchBindingCommitmentsV1,
    MtgoCompetitiveEventPregameSessionCommitmentsV1, MtgoCompetitiveEventRuntimeCommitmentsV1,
    MtgoCompetitiveEventSideboardDragInputReceiptCommitmentsV1,
    MtgoCompetitiveEventSideboardSequenceCommitmentsV1,
    MtgoCompetitiveEventSideboardTransferAdvanceV1, MtgoCompetitiveGameSessionCommitmentsV1,
    MtgoCompetitiveGestureGameSessionCommitmentsV1,
    MtgoCompetitiveLifecycleInputReceiptCommitmentsV1, MtgoCompetitiveNativePregameActionV1,
    MtgoCompetitiveNativePregameCardV1, MtgoCompetitiveNativePregameModelInputV1,
    MtgoCompetitiveOpenEntryReviewInputReceiptCommitmentsV1,
    MtgoCompetitivePregameActionPlanCommitmentsV1, MtgoCompetitivePregameExpectedPostconditionV1,
    MtgoCompetitivePregameInputReceiptCommitmentsV1,
    MtgoCompetitivePregameObservationCommitmentsV1, MtgoCompetitivePregameSelectedActionV1,
    MtgoCompetitivePregameStageV1, MtgoCompletedCompetitivePregameCommitmentsV1,
    MtgoConfirmedCompetitiveDuelGestureContinuationCommitmentsV1,
    MtgoConfirmedCompetitiveDuelGesturePrimitiveCommitmentsV1,
    MtgoConfirmedCompetitiveDuelPassCommitmentsV2, MtgoConfirmedCompetitiveEntryCommitmentsV1,
    MtgoConfirmedCompetitiveEventSideboardDragCommitmentsV1,
    MtgoConfirmedCompetitiveLifecycleControlCommitmentsV1,
    MtgoConfirmedCompetitiveOpenEntryReviewCommitmentsV1,
    MtgoConfirmedCompetitivePregameActionCommitmentsV1,
    MtgoControlBoundCompetitiveEntryReviewCommitmentsV4,
    MtgoFreshPreparedCompetitiveEventSideboardDragCommitmentsV1, MtgoInputGateStatusV3,
    MtgoMeasuredCompetitiveEventSideboardCommitmentsV1,
    MtgoPendingCompetitiveDuelGesturePrimitiveCommitmentsV1,
    MtgoPendingCompetitiveEventLifecycleControlCommitmentsV1,
    MtgoPlannedCompetitiveEventSideboardCommitmentsV1, MtgoPregameInputGateStatusV3,
    MtgoPreparedCompetitiveDuelGestureSourceStageCommitmentsV1,
    MtgoPreparedCompetitiveEntryCommitmentsV1,
    MtgoPreparedCompetitiveEventLifecycleControlCommitmentsV1,
    MtgoPreparedCompetitiveEventSideboardTransferCommitmentsV1,
    MtgoPreparedCompetitiveLifecycleControlCommitmentsV1,
    MtgoPreparedCompetitiveOpenEntryReviewCommitmentsV1,
    MtgoPreparedCompetitivePregameActionCommitmentsV1,
    MtgoReadyCompetitiveEventSideboardCommitmentsV1,
    MtgoReviewedCompetitiveEntryRatificationCandidateV1,
    MtgoReviewedCompetitiveGestureRatificationCandidateV1,
    MtgoReviewedCompetitiveLifecycleRatificationCandidateV1,
    MtgoReviewedCompetitiveOpenEntryReviewRatificationCandidateV1,
    MtgoReviewedCompetitivePassRatificationCandidateV2,
    MtgoReviewedCompetitivePregameRatificationCandidateV1,
    MtgoReviewedCompetitiveSideboardAutomationRatificationCandidateV1,
    MtgoReviewedSelectedListingCompetitiveEntryRatificationCandidateV2,
    MtgoSelectedListingBoundCompetitiveEntryReviewCommitmentsV1,
    MtgoSessionBoundCompetitiveDuelGestureCommitmentsV1,
    MtgoSourceBoundCompetitiveEntryReviewCommitmentsV2,
    OpaqueMtgoAuthorizationBoundCompetitiveDuelPassV1, OpaqueMtgoCompetitiveEventGameplayLeaseV1,
    OpaqueMtgoCompetitiveEventMatchLaunchBindingV1, OpaqueMtgoCompetitiveEventPregameSessionV1,
    OpaqueMtgoCompetitiveEventRuntimeV1, OpaqueMtgoCompetitiveEventSideboardSequenceV1,
    OpaqueMtgoCompetitiveGameSessionV1, OpaqueMtgoCompetitiveGestureGameSessionV1,
    OpaqueMtgoCompetitiveNativePregameRequestV1, OpaqueMtgoCompetitiveNativeSideboardRequestV1,
    OpaqueMtgoCompetitivePregameActionPlanV1, OpaqueMtgoCompetitivePregameObservationV1,
    OpaqueMtgoConfirmedCompetitiveDuelGestureContinuationV1,
    OpaqueMtgoConfirmedCompetitiveDuelGesturePrimitiveV1,
    OpaqueMtgoConfirmedCompetitiveDuelPassTransitionV2, OpaqueMtgoConfirmedCompetitiveEntryV1,
    OpaqueMtgoConfirmedCompetitiveEventSideboardDragV1,
    OpaqueMtgoConfirmedCompetitiveLifecycleControlV1,
    OpaqueMtgoConfirmedCompetitiveOpenEntryReviewV1, OpaqueMtgoConfirmedCompetitivePregameActionV1,
    OpaqueMtgoFreshPreparedCompetitiveEventSideboardDragV1,
    OpaqueMtgoMeasuredCompetitiveEventSideboardV1,
    OpaqueMtgoPendingCompetitiveDuelGesturePrimitiveV1, OpaqueMtgoPendingCompetitiveDuelPassV1,
    OpaqueMtgoPendingCompetitiveEntryV1, OpaqueMtgoPendingCompetitiveEventLifecycleControlV1,
    OpaqueMtgoPendingCompetitiveEventSideboardDragV1,
    OpaqueMtgoPendingCompetitiveLifecycleControlV1, OpaqueMtgoPendingCompetitiveOpenEntryReviewV1,
    OpaqueMtgoPendingCompetitivePregameInputV1, OpaqueMtgoPendingPregameInputV3,
    OpaqueMtgoPlannedCompetitiveEventSideboardV1,
    OpaqueMtgoPreparedCompetitiveDuelGestureSourceStageV1, OpaqueMtgoPreparedCompetitiveEntryV1,
    OpaqueMtgoPreparedCompetitiveEventLifecycleControlV1,
    OpaqueMtgoPreparedCompetitiveEventSideboardTransferV1,
    OpaqueMtgoPreparedCompetitiveLifecycleControlV1,
    OpaqueMtgoPreparedCompetitiveOpenEntryReviewV1, OpaqueMtgoPreparedCompetitivePregameActionV1,
    OpaqueMtgoReadyCompetitiveEventSideboardV1, OpaqueMtgoSessionBoundCompetitiveDuelGestureV1,
    RatifiedMtgoCompetitiveDuelGestureAuthorizationV1,
    RatifiedMtgoCompetitiveDuelPassAuthorizationV1, RatifiedMtgoCompetitiveEntryAuthorizationV1,
    RatifiedMtgoCompetitiveGestureMatchLaunchV1, RatifiedMtgoCompetitiveLifecycleAuthorizationV1,
    RatifiedMtgoCompetitiveMatchLaunchV1, RatifiedMtgoCompetitiveOpenEntryReviewAuthorizationV1,
    RatifiedMtgoCompetitivePregameAuthorizationV1,
    RatifiedMtgoCompetitiveSideboardAutomationAuthorizationV1,
    RatifiedMtgoPrivateMatchAuthorizationV3, MTGO_COMPETITIVE_AUTHORIZATION_READINESS_SCHEMA_V1,
};

#[cfg(target_os = "windows")]
pub use competitive_wiring_readiness::{
    check_competitive_wiring_static_readiness_v1, MtgoCompetitiveKnownWiringGapsV1,
    MtgoCompetitiveStaticReadinessStatusV1, MtgoCompetitiveWiringStaticReadinessV1,
    MTGO_COMPETITIVE_WIRING_STATIC_READINESS_SCHEMA_V1,
};

#[cfg(target_os = "windows")]
pub use competitive_model_decision_readiness::{
    check_competitive_model_decision_readiness_v1, MtgoCompetitiveModelDecisionReadinessV1,
    MTGO_COMPETITIVE_MODEL_DECISION_READINESS_SCHEMA_V1,
};

#[cfg(target_os = "windows")]
pub use competitive_auxiliary_model_scoring::{
    score_checked_untrusted_competitive_native_pregame_request_v1,
    score_checked_untrusted_competitive_native_pregame_v1,
    score_checked_untrusted_competitive_native_sideboard_request_v1,
    score_checked_untrusted_competitive_native_sideboard_v1,
    CheckedUntrustedMtgoCompetitiveNativePregameModelSelectionV1,
    CheckedUntrustedMtgoCompetitiveNativeSideboardModelSelectionV1,
    MtgoCompetitiveNativePregameScoreResponseV1, MtgoCompetitiveNativePregameScorerV1,
    MtgoCompetitiveNativeSideboardScoreResponseV1, MtgoCompetitiveNativeSideboardScorerV1,
    OpaqueMtgoScoredCompetitiveNativePregameRequestV1,
    OpaqueMtgoScoredCompetitiveNativeSideboardRequestV1,
    MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1,
};

#[cfg(target_os = "windows")]
pub use competitive_auxiliary_action_resolution::{
    resolve_checked_untrusted_competitive_native_pregame_selection_v1,
    resolve_checked_untrusted_competitive_native_sideboard_selection_v1,
    CheckedUntrustedMtgoCompetitivePregameSemanticResolutionV1,
    CheckedUntrustedMtgoCompetitiveSideboardSemanticResolutionV1,
};

#[cfg(target_os = "windows")]
pub use competitive_native_sideboard::{
    competitive_native_sideboard_configuration_commitment_v1,
    competitive_native_sideboard_model_input_commitment_v1,
    competitive_native_sideboard_model_selection_commitment_v1,
    validate_competitive_native_sideboard_model_input_v1,
    validate_competitive_native_sideboard_model_selection_v1,
    visible_native_sideboard_configuration_v1, MtgoCompetitiveNativeSideboardCardCountV1,
    MtgoCompetitiveNativeSideboardConfigurationV1, MtgoCompetitiveNativeSideboardModelInputV1,
    MtgoCompetitiveNativeSideboardModelSelectionV1,
};

#[cfg(target_os = "windows")]
pub use competitive_operator_bootstrap::{
    bind_competitive_operator_resources_v1, MtgoCompetitiveOperatorResourceCommitmentsV1,
    MtgoCompetitiveOperatorResourcesPartsV1, OpaqueMtgoCompetitiveOperatorResourcesV1,
};

#[cfg(target_os = "windows")]
pub use competitive_operator_loop::{
    advance_competitive_operator_completed_visible_game_to_sideboard_v1,
    advance_competitive_operator_completed_visible_match_v1,
    advance_competitive_post_entry_operator_observed_v1,
    advance_competitive_post_entry_operator_player_visible_gameplay_target_v1,
    begin_competitive_operator_attended_gameplay_v1,
    begin_competitive_operator_attended_heuristic_pregame_v1,
    begin_competitive_post_entry_operator_v1,
    bind_competitive_post_entry_operator_match_launch_identity_v1,
    bind_competitive_post_entry_operator_player_visible_gameplay_gesture_v1,
    bind_competitive_post_entry_operator_player_visible_gameplay_source_target_v1,
    checkout_competitive_operator_visible_native_sideboard_v1,
    checkout_competitive_post_entry_operator_attended_native_pregame_v1,
    checkout_competitive_post_entry_operator_native_pregame_v1,
    checkout_competitive_post_entry_operator_native_pregame_with_completed_history_v2,
    complete_competitive_operator_attended_heuristic_pregame_v1,
    complete_competitive_operator_attended_visible_game_for_sideboard_v1,
    complete_competitive_operator_attended_visible_match_v1,
    confirm_competitive_operator_attended_direct_visible_action_v1,
    confirm_competitive_operator_attended_heuristic_pregame_action_v1,
    confirm_competitive_post_entry_operator_player_visible_gameplay_primitive_v1,
    confirm_pending_competitive_post_entry_operator_lifecycle_v1,
    continue_competitive_operator_attended_heuristic_pregame_v1,
    execute_competitive_operator_attended_direct_visible_action_v1,
    execute_competitive_operator_attended_heuristic_pregame_action_v1,
    execute_competitive_post_entry_operator_player_visible_gameplay_primitive_v1,
    execute_prepared_competitive_post_entry_operator_lifecycle_v1,
    next_competitive_post_entry_operator_directive_v1,
    observe_competitive_post_entry_operator_event_record_v1,
    prepare_competitive_operator_attended_heuristic_pregame_action_v1,
    prepare_competitive_post_entry_operator_lifecycle_v1,
    prepare_competitive_post_entry_operator_player_visible_gameplay_before_input_v1,
    prepare_competitive_post_entry_operator_player_visible_gameplay_pointer_v1,
    ratify_competitive_post_entry_operator_match_launch_attended_v1,
    refresh_competitive_operator_attended_gameplay_visible_game_log_v1,
    refresh_competitive_post_entry_operator_attended_pregame_visible_game_log_v1,
    refresh_competitive_post_entry_operator_player_visible_gameplay_target_v1,
    refresh_resolved_competitive_operator_attended_pregame_visible_game_log_v1,
    resolve_checked_untrusted_competitive_operator_attended_native_pregame_v1,
    resolve_checked_untrusted_competitive_operator_native_pregame_v1,
    resolve_checked_untrusted_competitive_operator_native_sideboard_v1,
    return_competitive_post_entry_operator_gameplay_v1,
    return_confirmed_competitive_post_entry_operator_player_visible_gameplay_v1,
    score_checked_untrusted_competitive_operator_attended_native_pregame_v1,
    score_checked_untrusted_competitive_operator_native_pregame_v1,
    score_checked_untrusted_competitive_operator_native_sideboard_v1,
    select_competitive_post_entry_operator_player_visible_gameplay_action_v1,
    select_next_competitive_post_entry_operator_player_visible_gameplay_action_v1,
    MtgoCompetitiveOperatorAttendedDirectVisibleGameplaySelectionV1,
    MtgoCompetitiveOperatorPlayerVisibleGameplayConfirmationV1,
    MtgoCompetitivePostEntryOperatorCommitmentsV1, MtgoCompetitivePostEntryOperatorDirectiveV1,
    MtgoCompetitivePostEntryOperatorRouteV1,
    OpaqueMtgoCompetitiveOperatorAttendedDirectVisibleAbstainedV1,
    OpaqueMtgoCompetitiveOperatorAttendedDirectVisibleBeforeDispatchV1,
    OpaqueMtgoCompetitiveOperatorAttendedDirectVisiblePendingV1,
    OpaqueMtgoCompetitiveOperatorAttendedGameplayV1,
    OpaqueMtgoCompetitiveOperatorAttendedHeuristicPregameConfirmedV1,
    OpaqueMtgoCompetitiveOperatorAttendedHeuristicPregamePendingV1,
    OpaqueMtgoCompetitiveOperatorAttendedHeuristicPregamePreparedV1,
    OpaqueMtgoCompetitiveOperatorAttendedHeuristicPregameV1,
    OpaqueMtgoCompetitiveOperatorAttendedMatchLaunchV1,
    OpaqueMtgoCompetitiveOperatorAttendedNativePregameRequestV1,
    OpaqueMtgoCompetitiveOperatorCompletedVisibleGameV1,
    OpaqueMtgoCompetitiveOperatorCompletedVisibleMatchV1,
    OpaqueMtgoCompetitiveOperatorGameplayLeaseV1,
    OpaqueMtgoCompetitiveOperatorMatchLaunchBindingV1,
    OpaqueMtgoCompetitiveOperatorNativePregameRequestV1,
    OpaqueMtgoCompetitiveOperatorNativeSideboardRequestV1,
    OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayBeforeInputV1,
    OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayConfirmedV1,
    OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayFreshTargetV1,
    OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayGestureV1,
    OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayPendingV1,
    OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayPreparedPointerV1,
    OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplaySelectionV1,
    OpaqueMtgoCompetitiveOperatorPlayerVisibleGameplayTargetV1,
    OpaqueMtgoCompetitiveOperatorPregameCompletedV1,
    OpaqueMtgoCompetitiveOperatorVisibleSideboardingV1, OpaqueMtgoCompetitivePostEntryOperatorV1,
    OpaqueMtgoPendingCompetitiveOperatorLifecycleV1,
    OpaqueMtgoPreparedCompetitiveOperatorLifecycleV1,
    OpaqueMtgoResolvedCompetitiveOperatorAttendedNativePregameV1,
    OpaqueMtgoResolvedCompetitiveOperatorNativePregameV1,
    OpaqueMtgoResolvedCompetitiveOperatorNativeSideboardV1,
    OpaqueMtgoScoredCompetitiveOperatorAttendedNativePregameV1,
    OpaqueMtgoScoredCompetitiveOperatorNativePregameV1,
    OpaqueMtgoScoredCompetitiveOperatorNativeSideboardV1,
};
#[cfg(target_os = "windows")]
pub use competitive_pre_entry_operator::{
    begin_competitive_post_entry_operator_from_confirmed_entry_v1,
    bind_competitive_operator_paid_entry_review_v1, confirm_pending_competitive_operator_entry_v1,
    confirm_pending_competitive_operator_open_entry_review_v1,
    execute_prepared_competitive_operator_entry_v1,
    execute_prepared_competitive_operator_open_entry_review_v1,
    prepare_competitive_operator_open_entry_review_v1,
    prepare_ratified_competitive_operator_entry_v1, ratify_competitive_operator_paid_entry_v1,
    CheckedUntrustedMtgoCompetitiveOperatorPaidEntryReviewV1,
    OpaqueMtgoConfirmedCompetitiveOperatorEntryV1,
    OpaqueMtgoConfirmedCompetitiveOperatorOpenEntryReviewV1,
    OpaqueMtgoPendingCompetitiveOperatorEntryV1,
    OpaqueMtgoPendingCompetitiveOperatorOpenEntryReviewV1,
    OpaqueMtgoPreparedCompetitiveOperatorEntryV1,
    OpaqueMtgoPreparedCompetitiveOperatorOpenEntryReviewV1, RatifiedMtgoCompetitiveOperatorEntryV1,
};
#[cfg(target_os = "windows")]
pub use competitive_pregame_policy::{
    admit_ratified_competitive_pregame_heuristic_v1,
    bind_competitive_operator_pregame_resources_v1,
    check_untrusted_competitive_pregame_heuristic_v1, AdmittedMtgoCompetitivePregameHeuristicV1,
    CheckedUntrustedMtgoCompetitivePregameHeuristicV1,
    MtgoAdmittedCompetitivePregameHeuristicCommitmentsV1,
    MtgoCompetitiveOperatorPregameResourceCommitmentsV1,
    MtgoCompetitiveOperatorPregameResourcesPartsV1,
    MtgoCompetitivePregameHeuristicReviewDeclarationsV1,
    MtgoReviewedCompetitivePregameHeuristicCandidateV1,
    OpaqueMtgoCompetitiveOperatorPregameResourcesV1,
    MTGO_COMPETITIVE_PREGAME_HEURISTIC_REVIEW_SCHEMA_V1,
};
#[cfg(target_os = "windows")]
pub use competitive_visible_history_readiness::{
    check_competitive_visible_history_readiness_v1, MtgoCompetitiveVisibleHistoryReadinessStatusV1,
    MtgoCompetitiveVisibleHistoryReadinessV1, MTGO_COMPETITIVE_VISIBLE_HISTORY_READINESS_SCHEMA_V1,
};
#[cfg(target_os = "windows")]
pub use competitive_visible_match_memory::{
    append_competitive_completed_match_history_v1, begin_competitive_completed_match_history_v1,
    bind_competitive_player_visible_game_memory_v1,
    bind_match_scoped_competitive_player_visible_game_memory_v1,
    MtgoCompetitiveExternalCombatModelDecisionV1, MtgoCompetitiveExternalCompletedGameHeaderV1,
    MtgoCompetitiveExternalCompletedMatchHistoryConsumerV1,
    MtgoCompetitiveExternalCompletedMatchHistoryHeaderV1,
    MtgoCompetitiveExternalConfirmedCombatDecisionV1, MtgoCompetitiveExternalConfirmedDecisionV1,
    MtgoCompetitiveExternalPublicGameLogEventV1, MtgoCompetitiveExternalPublicHistoryConsumerV1,
    MtgoCompetitiveExternalPublicHistoryHeaderV1, MtgoCompetitiveExternalPublicHistoryOrderingV1,
    MtgoCompetitivePlayerRelativeGameWinnerV1, OpaqueMtgoCompetitiveCompletedMatchHistoryV1,
    OpaqueMtgoCompetitivePlayerVisibleGameMemoryV1, OpaqueMtgoCompetitiveVisibleGameOutcomeV1,
    MTGO_COMPETITIVE_EXTERNAL_PUBLIC_HISTORY_SCHEMA_V1,
};

#[cfg(target_os = "windows")]
pub use probe::{
    advance_evaluated_competitive_event_monitor_v1,
    advance_opaque_competitive_duel_gesture_sequence_from_pinned_runtime_v1,
    advance_opaque_competitive_duel_gesture_sequence_v1,
    advance_opaque_player_visible_duel_gesture_target_v1,
    begin_competitive_match_visible_game_log_action_baseline_v1,
    begin_competitive_visible_game_log_baseline_v1, begin_evaluated_competitive_event_monitor_v1,
    begin_opaque_competitive_duel_gesture_sequence_v1,
    bind_classified_competitive_event_listing_to_evaluation_v1,
    bind_classified_navigation_frame_to_competitive_entry_review_identity_v1,
    bind_classified_navigation_frame_to_competitive_event_listing_v1,
    bind_classified_navigation_frame_to_lifecycle_control_v1,
    bind_classified_navigation_frame_to_visible_event_record_v1,
    bind_classifier_backed_competitive_entry_control_and_deck_dry_run_v2,
    bind_competitive_match_visible_game_log_lease_v1,
    bind_opaque_competitive_duel_continuation_gesture_stage_v1,
    bind_opaque_competitive_duel_source_gesture_stage_v1,
    bind_opaque_navigation_frame_to_competitive_entry_review_identity_v1,
    bind_opaque_player_visible_duel_gesture_intent_v1,
    bind_opaque_player_visible_duel_source_gesture_target_v1,
    bind_process_epoch_visible_game_log_semantics_to_competitive_launch_identity_v1,
    build_card_aware_bottoming_action_plan_v5, build_card_aware_bottoming_scoring_request_v5,
    build_card_aware_pregame_action_plan_v4, build_card_aware_pregame_scoring_request_v4,
    build_pinned_current_solitaire_pregame_action_plan_v1, build_pregame_action_plan_v3,
    build_pregame_scoring_request_v3, capture_admitted_mtgo_competitive_navigation_frame_v1,
    capture_admitted_mtgo_duel_visible_frame_v1, capture_mtgo_dxgi_frame_candidate_v3,
    capture_pinned_current_solitaire_visible_frame_v1,
    card_aware_bottoming_scoring_request_commitment_v5,
    card_aware_pregame_scoring_request_commitment_v4,
    check_untrusted_competitive_event_listing_classifier_request_v1,
    check_untrusted_competitive_event_listing_pixels_v1,
    check_untrusted_competitive_event_record_classifier_request_v1,
    check_untrusted_competitive_navigation_classifier_request_v1,
    check_untrusted_competitive_sideboard_classifier_request_v1,
    check_untrusted_duel_gesture_target_request_v1, check_untrusted_duel_perception_request_v1,
    classify_admitted_mtgo_competitive_navigation_frame_v1,
    classify_admitted_mtgo_competitive_pregame_frame_v1,
    classify_checked_untrusted_competitive_event_listing_v1,
    classify_checked_untrusted_competitive_event_record_v1,
    classify_checked_untrusted_competitive_sideboard_v1,
    classify_competitive_pregame_public_context_v1, confirm_card_aware_bottom_selection_v5,
    confirm_card_aware_bottoming_cancel_plan_v5, confirm_card_aware_bottoming_selection_plan_v5,
    confirm_card_aware_bottoming_submit_plan_v5, confirm_pregame_keep_to_bottom_six_transition_v3,
    confirm_pregame_keep_to_first_main_transition_v3, confirm_pregame_mulligan_transition_v3,
    corroborate_competitive_match_visible_game_log_action_v1,
    evaluate_untrusted_visible_accessibility_catalog_case_v1,
    evaluate_untrusted_visible_accessibility_catalog_corpus_v1,
    finalize_visible_accessibility_catalog_review_artifact_v1,
    load_checked_untrusted_mtgo_dxgi_frame_candidate_from_artifact_v1,
    load_checked_untrusted_visible_accessibility_catalog_case_from_review_artifact_v1,
    load_pinned_current_solitaire_visible_frame_from_artifact_v1,
    measure_mtgo_dxgi_bottom_six_initial_candidate_v3,
    measure_mtgo_dxgi_bottom_six_reflow_candidate_v3,
    measure_mtgo_dxgi_bottom_six_state_candidate_v3,
    measure_mtgo_dxgi_bottom_six_visible_card_identities_candidate_v3,
    measure_mtgo_dxgi_first_main_candidate_v3,
    measure_mtgo_dxgi_first_main_visible_hand_candidate_v1,
    measure_mtgo_dxgi_mulligan_ladder_candidate_v3,
    measure_mtgo_dxgi_mulligan_visible_hand_candidate_v3,
    measure_pinned_current_solitaire_first_main_v1,
    measure_pinned_current_solitaire_first_main_visible_hand_v1,
    measure_pinned_current_solitaire_mulligan_ladder_v1,
    measure_pinned_current_solitaire_mulligan_visible_hand_v1,
    mtgo_visible_accessibility_catalog_review_commitment_v1,
    non_model_pregame_heuristic_algorithm_commitment_v1,
    non_model_pregame_heuristic_profile_commitment_v1, observe_attested_direct_visible_source_v1,
    perceive_admitted_duel_frame_v1, persist_mtgo_visible_accessibility_catalog_review_artifact_v1,
    plan_classified_competitive_sideboard_v1, pregame_scoring_request_commitment_v3,
    prepare_opaque_competitive_duel_action_plan_v1,
    prepare_opaque_competitive_duel_pass_actuation_v1,
    probe_mtgo_process_epoch_visible_game_log_v1, probe_mtgo_visible_accessibility_exact_text_v1,
    probe_mtgo_visible_accessibility_exact_text_with_pixel_corroboration_v1,
    probe_mtgo_visible_accessibility_known_label_catalog_evaluation_source_v1,
    probe_mtgo_visible_accessibility_known_label_catalog_v1,
    probe_mtgo_visible_accessibility_known_label_catalog_with_pixel_corroboration_v1,
    qualify_attested_direct_visible_source_current_duel_v1,
    refresh_competitive_match_visible_game_log_v1,
    review_player_visible_duel_gesture_target_schema_v1,
    review_player_visible_gameplay_postcondition_schema_v1, run_cli_v3,
    run_visible_accessibility_catalog_corpus_evaluation_cli_v1,
    run_visible_accessibility_catalog_review_finalization_cli_v1,
    run_visible_accessibility_known_label_catalog_cli_v1,
    run_visible_accessibility_known_label_catalog_pixel_corroboration_cli_v1,
    run_visible_accessibility_known_label_catalog_review_artifact_cli_v1,
    run_visible_accessibility_pixel_corroboration_cli_v1, run_visible_accessibility_probe_cli_v1,
    score_and_select_card_aware_bottoming_model_v5, score_and_select_card_aware_pregame_model_v4,
    score_and_select_opaque_player_visible_duel_perception_v1,
    score_and_select_opaque_player_visible_duel_perception_with_ongoing_history_v1,
    score_and_select_pinned_current_solitaire_pregame_v1, score_and_select_pregame_model_v3,
    score_select_and_resolve_opaque_player_visible_duel_perception_v1,
    score_select_and_resolve_opaque_player_visible_duel_perception_with_ongoing_history_v1,
    start_card_aware_bottoming_session_v5, validate_card_aware_bottoming_score_response_v5,
    validate_card_aware_pregame_score_response_v4, validate_pregame_score_response_v3,
    verify_competitive_navigation_classifier_runtime_v1, verify_direct_visible_dispatch_runtime_v1,
    verify_direct_visible_source_runtime_v1, verify_duel_gesture_target_runtime_v1,
    verify_duel_perception_runtime_v1, AdmittedMtgoPlayerVisibleDuelGestureTargetProtocolV1,
    AdmittedMtgoPlayerVisibleGameplayPostconditionProtocolV1,
    CheckedUntrustedMtgoCompetitiveEventListingClassifierRequestV1,
    CheckedUntrustedMtgoCompetitiveEventRecordClassifierRequestV1,
    CheckedUntrustedMtgoCompetitiveNavigationClassifierRequestV1,
    CheckedUntrustedMtgoCompetitiveSideboardClassifierRequestV1,
    CheckedUntrustedMtgoDuelGestureTargetRequestV1, CheckedUntrustedMtgoDuelPerceptionRequestV1,
    CheckedUntrustedMtgoVisibleAccessibilityCatalogCaseEvaluationV1,
    CheckedUntrustedMtgoVisibleAccessibilityCatalogCorpusEvaluationV1,
    MtgoAdmittedCompetitiveNavigationFrameCommitmentsV1, MtgoAdmittedDuelPerceptionCommitmentsV1,
    MtgoAdmittedDuelVisibleFrameCommitmentsV1, MtgoAttestedDirectVisibleBeforeDispatchRegionSetV1,
    MtgoAttestedDirectVisibleBeforeDispatchRegionSpecV1,
    MtgoAttestedDirectVisibleSourceObservationCommitmentsV1, MtgoBottomingCardIdentitySourceV5,
    MtgoBottomingConfirmedCardV5, MtgoBottomingVisibleCardV5,
    MtgoCardAwareBottomingScoreResponseV5, MtgoCardAwareBottomingScoringRequestV5,
    MtgoCardAwarePregameScoreResponseV4, MtgoCardAwarePregameScoringRequestV4,
    MtgoClassifiedCompetitiveEventListingCommitmentsV1,
    MtgoClassifiedCompetitiveEventRecordCommitmentsV1,
    MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1,
    MtgoClassifiedCompetitivePregameFrameCommitmentsV1,
    MtgoClassifiedCompetitivePregameModelContextCommitmentsV1,
    MtgoClassifiedCompetitiveSideboardCommitmentsV1,
    MtgoCompetitiveDuelGestureVisibleTransitionProbeV1,
    MtgoCompetitiveEntryDeckSelectionReviewInputV1,
    MtgoCompetitiveEntryFrameTransitionCommitmentsV1,
    MtgoCompetitiveEntryImmediateRecaptureCommitmentsV1,
    MtgoCompetitiveEventListingClassifierProcessResponseV1,
    MtgoCompetitiveEventListingClassifierRequestHeaderV1,
    MtgoCompetitiveEventListingOpenVisibleConfirmationCommitmentsV1,
    MtgoCompetitiveEventMonitorCommitmentsV1,
    MtgoCompetitiveEventRecordClassifierProcessResponseV1,
    MtgoCompetitiveEventRecordClassifierRequestHeaderV1,
    MtgoCompetitiveLifecycleControlTransitionCommitmentsV1,
    MtgoCompetitiveNavigationClassifierProcessResponseV1,
    MtgoCompetitiveNavigationClassifierRequestHeaderV1, MtgoCompetitiveNavigationFrameIdentityV1,
    MtgoCompetitiveSideboardClassifierProcessResponseV1,
    MtgoCompetitiveSideboardClassifierRequestHeaderV1, MtgoDuelGestureTargetProcessResponseV1,
    MtgoDuelGestureTargetRequestHeaderV1, MtgoDuelPerceptionFrameIdentityV1,
    MtgoDuelPerceptionProcessResponseV1, MtgoDuelPerceptionRequestHeaderV1,
    MtgoDxgiCaptureRequestV3, MtgoDxgiFrameCommitmentsV3,
    MtgoEvaluatedCompetitiveEventListingCommitmentsV1, MtgoExternalCardAwareBottomingScorerV5,
    MtgoExternalCardAwarePregameScorerV4, MtgoExternalPregameScorerV3, MtgoHeuristicCardFeatureV1,
    MtgoHeuristicCardKindV1, MtgoNonModelPregameHeuristicProfileV1, MtgoNonModelPregameHeuristicV1,
    MtgoOpaqueCompetitiveDuelActionPlanCommitmentsV1,
    MtgoOpaqueCompetitiveDuelGestureSequenceCommitmentsV1,
    MtgoOpaqueCompetitiveDuelGestureStageCommitmentsV1,
    MtgoOpaqueCompetitiveDuelPassPreparationCommitmentsV1,
    MtgoOpaqueCompetitiveEntryControlDryRunCommitmentsV1,
    MtgoOpaqueCompetitiveEntryReviewIdentityCommitmentsV1,
    MtgoOpaqueCompetitiveLaunchIdentityCommitmentsV1,
    MtgoOpaqueCompetitiveLifecycleControlCommitmentsV1, MtgoOpaqueDuelResolvedControlCommitmentsV1,
    MtgoOpaquePinnedCompetitiveDuelGestureContinuationCommitmentsV1,
    MtgoPinnedSolitaireVisibleFrameCommitmentsV1, MtgoPlannedBottomingPostconditionV5,
    MtgoPlannedCompetitiveSideboardCommitmentsV1, MtgoPlannedPregamePostconditionV3,
    MtgoPlayerVisibleDuelGestureTargetSchemaReviewV1, MtgoPlayerVisibleDuelModelSelectionResultV1,
    MtgoPlayerVisibleGameplayPostconditionSchemaReviewV1, MtgoPregameScoreResponseV3,
    MtgoPregameScoringRequestV3, MtgoQualifiedDirectVisibleSourceObservationCommitmentsV1,
    MtgoRefreshedAttestedDirectVisibleSelectionCommitmentsV1,
    MtgoSourceBoundCompetitiveEventListingCommitmentsV1,
    MtgoSourceBoundCompetitiveEventRecordCommitmentsV1,
    MtgoVerifiedCompetitiveNavigationClassifierRuntimeCommitmentsV1,
    MtgoVerifiedDirectVisibleDispatchRuntimeCommitmentsV1,
    MtgoVerifiedDirectVisibleSourceRuntimeCommitmentsV1,
    MtgoVerifiedDuelGestureTargetRuntimeCommitmentsV1,
    MtgoVerifiedDuelPerceptionRuntimeCommitmentsV1,
    MtgoVisibleAccessibilityCatalogCaseEvaluationSummaryV1,
    MtgoVisibleAccessibilityCatalogCompletedReviewReceiptV1,
    MtgoVisibleAccessibilityCatalogCorpusEntrySummaryV1,
    MtgoVisibleAccessibilityCatalogCorpusEvaluationSummaryV1,
    MtgoVisibleAccessibilityCatalogEntrySummaryV1, MtgoVisibleAccessibilityCatalogProbeSummaryV1,
    MtgoVisibleAccessibilityCatalogReviewArtifactReceiptV1,
    MtgoVisibleAccessibilityCatalogReviewEntryV1, MtgoVisibleAccessibilityCatalogReviewV1,
    MtgoVisibleAccessibilityCatalogSliceV1, MtgoVisibleAccessibilityExactTextQueryV1,
    MtgoVisibleAccessibilityPixelCatalogEntrySummaryV1,
    MtgoVisibleAccessibilityPixelCatalogProbeSummaryV1,
    MtgoVisibleAccessibilityPixelCorroborationSummaryV1,
    MtgoVisibleAccessibilityPixelQueryResultV1, MtgoVisibleAccessibilityProbeSummaryV1,
    MtgoVisibleAccessibilityQueryResultV1, OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    OpaqueMtgoAdmittedDuelPerceptionV1, OpaqueMtgoAdmittedDuelVisibleFrameV1,
    OpaqueMtgoAttestedDirectVisibleSourceObservationV1, OpaqueMtgoBottomingActionPlanV5,
    OpaqueMtgoCardAwareBottomingModelSelectionV5, OpaqueMtgoCardAwareBottomingSessionV5,
    OpaqueMtgoCardAwarePregameModelSelectionV4, OpaqueMtgoClassifiedCompetitiveEventListingV1,
    OpaqueMtgoClassifiedCompetitiveEventRecordV1, OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    OpaqueMtgoClassifiedCompetitivePregameFrameV1,
    OpaqueMtgoClassifiedCompetitivePregameModelContextV1,
    OpaqueMtgoClassifiedCompetitiveSideboardV1, OpaqueMtgoCompetitiveDuelActionPlanV1,
    OpaqueMtgoCompetitiveDuelGestureSequenceV1, OpaqueMtgoCompetitiveDuelGestureStageV1,
    OpaqueMtgoCompetitiveEntryControlDryRunV1, OpaqueMtgoCompetitiveEntryReviewIdentityV1,
    OpaqueMtgoCompetitiveEventMonitorV1, OpaqueMtgoCompetitiveLaunchIdentityV1,
    OpaqueMtgoCompetitiveLifecycleControlV1, OpaqueMtgoCompetitiveMatchVisibleGameLogLeaseV1,
    OpaqueMtgoCompetitiveMatchVisibleGameLogSnapshotV1,
    OpaqueMtgoCompetitiveVisibleGameLogBaselineV1, OpaqueMtgoCompetitiveVisibleGameLogSemanticsV1,
    OpaqueMtgoConfirmedBottomingSubmitV5, OpaqueMtgoConfirmedKeepToBottomSixTransitionV3,
    OpaqueMtgoConfirmedKeepToFirstMainTransitionV3, OpaqueMtgoConfirmedMulliganTransitionV3,
    OpaqueMtgoDxgiBottomSixInitialMeasurementV3, OpaqueMtgoDxgiBottomSixReflowMeasurementV3,
    OpaqueMtgoDxgiBottomSixStateMeasurementV3,
    OpaqueMtgoDxgiBottomSixVisibleCardIdentityMeasurementV3, OpaqueMtgoDxgiFirstMainMeasurementV3,
    OpaqueMtgoDxgiFirstMainVisibleHandMeasurementV1, OpaqueMtgoDxgiFrameCandidateV3,
    OpaqueMtgoDxgiMulliganMeasurementV3, OpaqueMtgoDxgiMulliganVisibleHandMeasurementV3,
    OpaqueMtgoEvaluatedCompetitiveEventListingV1,
    OpaqueMtgoPinnedCompetitiveDuelGestureContinuationV1,
    OpaqueMtgoPinnedSolitaireFirstMainMeasurementV1,
    OpaqueMtgoPinnedSolitaireFirstMainVisibleHandV1,
    OpaqueMtgoPinnedSolitaireMulliganMeasurementV1, OpaqueMtgoPinnedSolitaireMulliganVisibleHandV1,
    OpaqueMtgoPinnedSolitairePregameActionPlanV1, OpaqueMtgoPinnedSolitairePregameSelectionV1,
    OpaqueMtgoPinnedSolitaireVisibleFrameV1, OpaqueMtgoPlannedCompetitiveSideboardV1,
    OpaqueMtgoPlayerVisibleDuelGestureIntentV1, OpaqueMtgoPlayerVisibleDuelGestureTargetBindingV1,
    OpaqueMtgoPlayerVisibleDuelModelSelectionV1, OpaqueMtgoPlayerVisibleDuelResolvedControlV1,
    OpaqueMtgoPregameActionPlanV3, OpaqueMtgoPregameModelSelectionV3,
    OpaqueMtgoPreparedCompetitiveDuelPassV1, OpaqueMtgoProcessEpochVisibleGameLogSemanticsV1,
    OpaqueMtgoProcessEpochVisibleGameLogV1, OpaqueMtgoProfileBoundDuelResolvedControlV1,
    OpaqueMtgoQualifiedDirectVisibleSourceObservationV1,
    OpaqueMtgoSourceBoundCompetitiveEventListingV1, OpaqueMtgoSourceBoundCompetitiveEventRecordV1,
    OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    OpaqueMtgoVerifiedDirectVisibleDispatchRuntimeV1,
    OpaqueMtgoVerifiedDirectVisibleSourceRuntimeV1, OpaqueMtgoVerifiedDuelGestureTargetRuntimeV1,
    OpaqueMtgoVerifiedDuelPerceptionRuntimeV1, OpaqueMtgoVisibleAccessibilityPixelCorroborationV1,
    OpaqueMtgoVisibleAccessibilityProbeV1, MTGO_BOTTOMING_CARD_AWARE_SCORING_SCHEMA_V5,
    MTGO_HEURISTIC_COLORLESS_V1, MTGO_HEURISTIC_COLOR_BLACK_V1, MTGO_HEURISTIC_COLOR_BLUE_V1,
    MTGO_HEURISTIC_COLOR_GREEN_V1, MTGO_HEURISTIC_COLOR_RED_V1, MTGO_HEURISTIC_COLOR_WHITE_V1,
    MTGO_NON_MODEL_PREGAME_HEURISTIC_SCHEMA_V1,
    MTGO_OPAQUE_COMPETITIVE_DUEL_GESTURE_TRANSITION_SCHEMA_V1,
    MTGO_PLAYER_VISIBLE_DUEL_GESTURE_TARGET_SCHEMA_REVIEW_SCHEMA_V1,
    MTGO_PREGAME_CARD_AWARE_SCORING_SCHEMA_V4, MTGO_PREGAME_EXTERNAL_SCORING_SCHEMA_V3,
    MTGO_VISIBLE_ACCESSIBILITY_CATALOG_CASE_EVALUATION_SCHEMA_V1,
    MTGO_VISIBLE_ACCESSIBILITY_CATALOG_CORPUS_EVALUATION_SCHEMA_V1,
    MTGO_VISIBLE_ACCESSIBILITY_CATALOG_REVIEW_ARTIFACT_SCHEMA_V1,
    MTGO_VISIBLE_ACCESSIBILITY_CATALOG_REVIEW_SCHEMA_V1,
    MTGO_VISIBLE_ACCESSIBILITY_CATALOG_SCHEMA_V1,
    MTGO_VISIBLE_ACCESSIBILITY_PIXEL_CATALOG_SCHEMA_V1,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureWindowModeV2 {
    MainClient,
    SolitaireGame,
    DuelGame,
    SpectatorGame,
}

impl CaptureWindowModeV2 {
    pub fn manifest_name(self) -> &'static str {
        match self {
            Self::MainClient => "main_client",
            Self::SolitaireGame => "solitaire_game",
            Self::DuelGame => "duel_game",
            Self::SpectatorGame => "spectator_game",
        }
    }

    pub fn capture_role(self) -> &'static str {
        match self {
            Self::MainClient => "navigation",
            Self::SolitaireGame => "acting_player_solitaire",
            Self::DuelGame => "acting_player_duel",
            Self::SpectatorGame => "spectator",
        }
    }
}

pub fn validate_visible_mtgo_title_v2(
    mode: CaptureWindowModeV2,
    expected_game_format: Option<&str>,
    title: &str,
) -> Result<(), &'static str> {
    if title.is_empty() || title.len() > 1_024 || title.chars().any(char::is_control) {
        return Err("window title is empty, too long, or contains control characters");
    }
    match mode {
        CaptureWindowModeV2::MainClient => {
            if expected_game_format.is_some() {
                return Err("main-client mode cannot declare a game format");
            }
            if !title.contains("Magic: The Gathering Online") {
                return Err("main-client title does not identify Magic: The Gathering Online");
            }
        }
        CaptureWindowModeV2::SolitaireGame => {
            let format = validate_game_format_v2(expected_game_format)?;
            let prefix = format!("(Solitaire): {format}: Vs. ");
            let participant = title
                .strip_prefix(&prefix)
                .ok_or("Solitaire title does not match the exact format prefix")?;
            validate_participant_text_v2(participant, false)?;
        }
        CaptureWindowModeV2::DuelGame => {
            let format = validate_game_format_v2(expected_game_format)?;
            let prefix = format!("(1-on-1): {format}: Vs. ");
            let opponent = title
                .strip_prefix(&prefix)
                .ok_or("acting-player duel title does not match the exact format prefix")?;
            validate_participant_text_v2(opponent, false)?;
        }
        CaptureWindowModeV2::SpectatorGame => {
            let format = validate_game_format_v2(expected_game_format)?;
            let prefix = format!("(1-on-1): {format}: Vs. ");
            let participants = title
                .strip_prefix(&prefix)
                .ok_or("spectator title does not match the exact format prefix")?;
            validate_participant_text_v2(participants, true)?;
        }
    }
    Ok(())
}

fn validate_game_format_v2(value: Option<&str>) -> Result<&str, &'static str> {
    let value = value.ok_or("game mode requires an expected format")?;
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b' ' | b'-'))
    {
        return Err("expected game format is not a safe visible label");
    }
    Ok(value)
}

fn validate_participant_text_v2(value: &str, require_comma: bool) -> Result<(), &'static str> {
    if value.is_empty() || value.len() > 512 {
        return Err("participant title text is empty or too long");
    }
    let participant_text = if let Some((participants, identity)) = value.split_once(" Match #") {
        let (match_id, game_id) = identity
            .split_once(" - Game #")
            .ok_or("visible match title suffix is malformed")?;
        if match_id.is_empty()
            || game_id.is_empty()
            || !match_id.bytes().all(|byte| byte.is_ascii_digit())
            || !game_id.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err("visible match and game IDs must be decimal integers");
        }
        participants
    } else {
        if value.contains('#') {
            return Err("visible match title suffix is malformed");
        }
        value
    };
    if participant_text.trim() != participant_text || participant_text.is_empty() {
        return Err("participant title text has invalid surrounding whitespace");
    }
    if require_comma {
        let (left, right) = participant_text
            .split_once(',')
            .ok_or("spectator title must visibly identify two participants")?;
        if left.trim().is_empty() || right.trim().is_empty() || right.contains(',') {
            return Err("spectator title must contain exactly two visible participants");
        }
    } else if participant_text.contains(',') {
        return Err("acting-player title must identify one visible participant");
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedRectV1 {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl SignedRectV1 {
    pub fn width(self) -> Result<u32, &'static str> {
        u32::try_from(
            self.right
                .checked_sub(self.left)
                .ok_or("rectangle width overflow")?,
        )
        .map_err(|_| "rectangle width is not positive")
        .and_then(|value| {
            if value == 0 {
                Err("rectangle width is zero")
            } else {
                Ok(value)
            }
        })
    }

    pub fn height(self) -> Result<u32, &'static str> {
        u32::try_from(
            self.bottom
                .checked_sub(self.top)
                .ok_or("rectangle height overflow")?,
        )
        .map_err(|_| "rectangle height is not positive")
        .and_then(|value| {
            if value == 0 {
                Err("rectangle height is zero")
            } else {
                Ok(value)
            }
        })
    }

    pub fn contains(self, other: Self) -> bool {
        self.width().is_ok()
            && self.height().is_ok()
            && other.width().is_ok()
            && other.height().is_ok()
            && other.left >= self.left
            && other.top >= self.top
            && other.right <= self.right
            && other.bottom <= self.bottom
    }

    pub fn intersects(self, other: Self) -> bool {
        self.width().is_ok()
            && self.height().is_ok()
            && other.width().is_ok()
            && other.height().is_ok()
            && self.left < other.right
            && self.right > other.left
            && self.top < other.bottom
            && self.bottom > other.top
    }

    pub fn contains_point(self, x: i32, y: i32) -> bool {
        self.width().is_ok()
            && self.height().is_ok()
            && x >= self.left
            && x < self.right
            && y >= self.top
            && y < self.bottom
    }

    pub fn crop_box_within(self, output: Self) -> Result<CropBoxV1, &'static str> {
        if !output.contains(self) {
            return Err("crop is not wholly contained in the output");
        }
        Ok(CropBoxV1 {
            left: u32::try_from(self.left - output.left).map_err(|_| "crop left is negative")?,
            top: u32::try_from(self.top - output.top).map_err(|_| "crop top is negative")?,
            width: self.width()?,
            height: self.height()?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CropBoxV1 {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
}

pub fn copy_tightly_packed_bgra8_v1(
    mapped: &[u8],
    row_pitch: usize,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, &'static str> {
    let tight_row = usize::try_from(width)
        .ok()
        .and_then(|value| value.checked_mul(4))
        .ok_or("tight row size overflow")?;
    let height = usize::try_from(height).map_err(|_| "height does not fit usize")?;
    if tight_row == 0 || height == 0 || row_pitch < tight_row {
        return Err("mapped texture geometry is invalid");
    }
    let required = row_pitch
        .checked_mul(height)
        .ok_or("mapped byte length overflow")?;
    if mapped.len() < required {
        return Err("mapped texture is shorter than its declared rows");
    }
    let output_len = tight_row
        .checked_mul(height)
        .ok_or("canonical byte length overflow")?;
    let mut output = Vec::with_capacity(output_len);
    for row in 0..height {
        let start = row * row_pitch;
        output.extend_from_slice(&mapped[start..start + tight_row]);
    }
    Ok(output)
}

pub fn sha256_hex_v1(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_origin_crop_is_exact() {
        let output = SignedRectV1 {
            left: -1_920,
            top: -200,
            right: 0,
            bottom: 880,
        };
        let client = SignedRectV1 {
            left: -1_800,
            top: -100,
            right: -600,
            bottom: 700,
        };
        assert_eq!(
            client.crop_box_within(output).unwrap(),
            CropBoxV1 {
                left: 120,
                top: 100,
                width: 1_200,
                height: 800,
            }
        );
    }

    #[test]
    fn offscreen_edge_and_invalid_rectangles_fail() {
        let output = SignedRectV1 {
            left: 0,
            top: 0,
            right: 1_920,
            bottom: 1_080,
        };
        assert!(SignedRectV1 {
            left: -1,
            top: 0,
            right: 100,
            bottom: 100,
        }
        .crop_box_within(output)
        .is_err());
        assert!(SignedRectV1 {
            left: 10,
            top: 10,
            right: 10,
            bottom: 20,
        }
        .crop_box_within(output)
        .is_err());
    }

    #[test]
    fn touching_edges_do_not_intersect() {
        let left = SignedRectV1 {
            left: 0,
            top: 0,
            right: 100,
            bottom: 100,
        };
        let right = SignedRectV1 {
            left: 100,
            top: 0,
            right: 200,
            bottom: 100,
        };
        assert!(!left.intersects(right));
    }

    #[test]
    fn canonical_copy_ignores_staging_padding() {
        let mapped = [
            1, 2, 3, 4, 5, 6, 7, 8, 90, 91, 92, 93, 9, 10, 11, 12, 13, 14, 15, 16, 94, 95, 96, 97,
        ];
        assert_eq!(
            copy_tightly_packed_bgra8_v1(&mapped, 12, 2, 2).unwrap(),
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
        );
    }

    #[test]
    fn main_client_title_has_no_game_role() {
        assert!(validate_visible_mtgo_title_v2(
            CaptureWindowModeV2::MainClient,
            None,
            "Magic: The Gathering Online"
        )
        .is_ok());
        assert!(validate_visible_mtgo_title_v2(
            CaptureWindowModeV2::MainClient,
            Some("Freeform"),
            "Magic: The Gathering Online"
        )
        .is_err());
    }

    #[test]
    fn solitaire_title_requires_exact_format_and_one_participant() {
        assert!(validate_visible_mtgo_title_v2(
            CaptureWindowModeV2::SolitaireGame,
            Some("Freeform"),
            "(Solitaire): Freeform: Vs. local-player Match #123 - Game #456"
        )
        .is_ok());
        assert!(validate_visible_mtgo_title_v2(
            CaptureWindowModeV2::SolitaireGame,
            Some("Standard"),
            "(Solitaire): Freeform: Vs. local-player"
        )
        .is_err());
        assert!(validate_visible_mtgo_title_v2(
            CaptureWindowModeV2::SolitaireGame,
            Some("Freeform"),
            "(Solitaire): Freeform: Vs. one, two"
        )
        .is_err());
    }

    #[test]
    fn spectator_title_requires_exact_format_and_two_participants() {
        assert!(validate_visible_mtgo_title_v2(
            CaptureWindowModeV2::SpectatorGame,
            Some("Standard"),
            "(1-on-1): Standard: Vs. player-one, player-two"
        )
        .is_ok());
        assert!(validate_visible_mtgo_title_v2(
            CaptureWindowModeV2::SpectatorGame,
            Some("Standard"),
            "(1-on-1): Standard: Vs. player-one"
        )
        .is_err());
    }

    #[test]
    fn acting_player_duel_title_requires_exact_format_and_one_opponent() {
        assert_eq!(CaptureWindowModeV2::DuelGame.manifest_name(), "duel_game");
        assert_eq!(
            CaptureWindowModeV2::DuelGame.capture_role(),
            "acting_player_duel"
        );
        assert!(validate_visible_mtgo_title_v2(
            CaptureWindowModeV2::DuelGame,
            Some("Freeform"),
            "(1-on-1): Freeform: Vs. opponent-name Match #123 - Game #456"
        )
        .is_ok());
        assert!(validate_visible_mtgo_title_v2(
            CaptureWindowModeV2::DuelGame,
            Some("Standard"),
            "(1-on-1): Freeform: Vs. opponent-name"
        )
        .is_err());
        assert!(validate_visible_mtgo_title_v2(
            CaptureWindowModeV2::DuelGame,
            Some("Freeform"),
            "(1-on-1): Freeform: Vs. local-player, opponent-name"
        )
        .is_err());
        assert!(validate_visible_mtgo_title_v2(
            CaptureWindowModeV2::DuelGame,
            None,
            "(1-on-1): Freeform: Vs. opponent-name"
        )
        .is_err());
    }
}
