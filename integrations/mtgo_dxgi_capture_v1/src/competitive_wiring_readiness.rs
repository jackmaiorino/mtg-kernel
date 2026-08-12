use crate::competitive_pregame_policy::competitive_pregame_heuristic_ratification_present_v1;
use crate::{
    competitive_authorization_ratification_readiness_v1,
    MtgoCompetitiveAuthorizationRatificationReadinessV1,
};
use mtgo_blackbox_v1::{
    competitive_semantic_ratification_readiness_v1, MtgoCompetitiveEventKindV1,
    MtgoCompetitiveSemanticRatificationReadinessV1,
};
use serde::{Deserialize, Serialize};

pub const MTGO_COMPETITIVE_WIRING_STATIC_READINESS_SCHEMA_V1: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveStaticReadinessStatusV1 {
    BlockedMissingRatificationsAndModelInterfaces,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveKnownWiringGapsV1 {
    pub operator_resource_bootstrap_present: bool,
    pub checkpoint_bound_model_capability_commitment_present: bool,
    pub player_visible_information_boundary_is_transport_independent: bool,
    pub persisted_visible_game_log_parser_present: bool,
    pub persisted_visible_game_log_local_corpus_passed: bool,
    pub post_entry_operator_pairing_ready_game_log_baseline_present: bool,
    pub post_entry_operator_attended_match_game_log_lease_present: bool,
    pub post_entry_operator_attended_match_log_refresh_identity_retained: bool,
    pub post_entry_operator_match_scoped_visible_game_log_refresh_present: bool,
    pub player_visible_game_log_action_corroboration_present: bool,
    pub player_visible_game_log_exact_rendered_prefix_binding_present: bool,
    pub player_visible_game_log_action_corroboration_grants_additional_input: bool,
    pub external_public_history_import_contract_present: bool,
    pub external_public_history_cross_source_ordering_policy_present: bool,
    pub external_public_history_model_facts_only_present: bool,
    pub external_public_history_adapter_metadata_withheld: bool,
    pub ongoing_external_public_history_snapshot_replay_present: bool,
    pub first_gameplay_decision_zero_action_history_supported: bool,
    pub ongoing_public_history_to_player_visible_scorer_bridge_present: bool,
    pub native_checkpoint_external_public_history_import_present: bool,
    pub pre_entry_operator_loop_present: bool,
    pub post_entry_operator_loop_present: bool,
    pub post_entry_operator_native_pregame_request_checkout_present: bool,
    pub post_entry_operator_checked_untrusted_native_pregame_scoring_present: bool,
    pub checked_untrusted_native_pregame_semantic_resolution_present: bool,
    pub post_entry_operator_checked_untrusted_native_pregame_resolution_present: bool,
    pub post_entry_operator_model_owned_pregame_resume_present: bool,
    pub post_entry_operator_native_sideboard_request_checkout_present: bool,
    pub post_entry_operator_checked_untrusted_native_sideboard_scoring_present: bool,
    pub checked_untrusted_native_sideboard_manifest_resolution_present: bool,
    pub post_entry_operator_checked_untrusted_native_sideboard_resolution_present: bool,
    pub post_entry_operator_model_owned_sideboard_resume_present: bool,
    pub end_to_end_operator_loop_present: bool,
    pub selected_listing_classifier_protocol_present: bool,
    pub navigation_lifecycle_classifier_protocol_present: bool,
    pub event_record_classifier_protocol_present: bool,
    pub sideboard_classifier_protocol_present: bool,
    pub duel_perception_classifier_protocol_present: bool,
    pub native_checkpoint_duel_action_interface_present: bool,
    pub player_visible_duel_decision_input_contract_present: bool,
    pub player_visible_duel_scorer_transport_contract_present: bool,
    pub player_visible_duel_selection_to_visible_control_bridge_present: bool,
    pub player_visible_duel_gesture_contract_present: bool,
    pub player_visible_duel_gesture_to_opaque_control_join_present: bool,
    pub player_visible_duel_gesture_kernel_object_references_withheld: bool,
    pub player_visible_duel_source_gesture_target_protocol_present: bool,
    pub player_visible_duel_gesture_target_classifier_present: bool,
    pub player_visible_duel_source_gesture_target_pixels_rehashed: bool,
    pub player_visible_duel_gesture_target_protocol_ratified: bool,
    pub player_visible_duel_gesture_continuation_target_binding_present: bool,
    pub native_checkpoint_player_visible_only_duel_action_interface_present: bool,
    pub current_duel_scorer_kernel_bookkeeping_withheld: bool,
    pub native_checkpoint_pregame_interface_present: bool,
    pub competitive_player_visible_pregame_request_contract_present: bool,
    pub competitive_pregame_player_known_submitted_deck_configuration_present: bool,
    pub competitive_pregame_player_known_deck_configuration_present: bool,
    pub competitive_pregame_ordered_confirmed_bottom_history_present: bool,
    pub competitive_pregame_public_context_contract_present: bool,
    pub competitive_pregame_play_draw_context_present: bool,
    pub competitive_pregame_match_score_context_present: bool,
    pub competitive_pregame_score_response_contract_present: bool,
    pub visible_accessibility_exact_text_probe_present: bool,
    pub visible_accessibility_same_frame_pixel_corroboration_present: bool,
    pub terminal_outcome_trained_pregame_head_present: bool,
    pub non_model_pregame_scorer_present: bool,
    pub competitive_pregame_heuristic_deployment_ratification_present: bool,
    pub competitive_pregame_session_ownership_bridge_present: bool,
    pub competitive_pregame_capture_profile_present: bool,
    pub competitive_pregame_card_and_control_surface_present: bool,
    pub competitive_pregame_deck_bound_action_planner_present: bool,
    pub competitive_pregame_immediate_recapture_preparation_present: bool,
    pub competitive_pregame_postcondition_contract_present: bool,
    pub competitive_pregame_input_actuator_present: bool,
    pub competitive_pregame_capture_and_session_bridge_present: bool,
    pub native_checkpoint_changed_sideboard_interface_present: bool,
    pub competitive_player_visible_sideboard_payload_contract_present: bool,
    pub competitive_player_visible_sideboard_score_binding_present: bool,
    pub competitive_sideboard_score_response_contract_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveWiringStaticReadinessV1 {
    pub schema_version: u32,
    pub purpose: String,
    pub supported_event_kinds: Vec<MtgoCompetitiveEventKindV1>,
    pub semantic_ratifications: MtgoCompetitiveSemanticRatificationReadinessV1,
    pub authorization_ratifications: MtgoCompetitiveAuthorizationRatificationReadinessV1,
    pub known_wiring_gaps: MtgoCompetitiveKnownWiringGapsV1,
    pub status: MtgoCompetitiveStaticReadinessStatusV1,
    pub safe_for_live_capture: bool,
    pub safe_for_input: bool,
    pub safe_for_event_entry: bool,
    pub safe_for_spending: bool,
}

/// Reports static production wiring state without reading the MTGO process,
/// capturing pixels, loading correspondence bytes, scoring, or sending input.
pub fn check_competitive_wiring_static_readiness_v1() -> MtgoCompetitiveWiringStaticReadinessV1 {
    MtgoCompetitiveWiringStaticReadinessV1 {
        schema_version: MTGO_COMPETITIVE_WIRING_STATIC_READINESS_SCHEMA_V1,
        purpose: "non_actuating_static_competitive_wiring_readiness_v1".to_owned(),
        supported_event_kinds: vec![
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ],
        semantic_ratifications: competitive_semantic_ratification_readiness_v1(),
        authorization_ratifications: competitive_authorization_ratification_readiness_v1(),
        known_wiring_gaps: MtgoCompetitiveKnownWiringGapsV1 {
            operator_resource_bootstrap_present: true,
            checkpoint_bound_model_capability_commitment_present: true,
            player_visible_information_boundary_is_transport_independent: true,
            persisted_visible_game_log_parser_present: true,
            persisted_visible_game_log_local_corpus_passed: true,
            post_entry_operator_pairing_ready_game_log_baseline_present: true,
            post_entry_operator_attended_match_game_log_lease_present: true,
            post_entry_operator_attended_match_log_refresh_identity_retained: true,
            post_entry_operator_match_scoped_visible_game_log_refresh_present: true,
            player_visible_game_log_action_corroboration_present: true,
            player_visible_game_log_exact_rendered_prefix_binding_present: true,
            player_visible_game_log_action_corroboration_grants_additional_input: false,
            external_public_history_import_contract_present: true,
            external_public_history_cross_source_ordering_policy_present: true,
            external_public_history_model_facts_only_present: true,
            external_public_history_adapter_metadata_withheld: true,
            ongoing_external_public_history_snapshot_replay_present: true,
            first_gameplay_decision_zero_action_history_supported: true,
            ongoing_public_history_to_player_visible_scorer_bridge_present: true,
            native_checkpoint_external_public_history_import_present: false,
            pre_entry_operator_loop_present: true,
            post_entry_operator_loop_present: true,
            post_entry_operator_native_pregame_request_checkout_present: true,
            post_entry_operator_checked_untrusted_native_pregame_scoring_present: true,
            checked_untrusted_native_pregame_semantic_resolution_present: true,
            post_entry_operator_checked_untrusted_native_pregame_resolution_present: true,
            post_entry_operator_model_owned_pregame_resume_present: false,
            post_entry_operator_native_sideboard_request_checkout_present: true,
            post_entry_operator_checked_untrusted_native_sideboard_scoring_present: true,
            checked_untrusted_native_sideboard_manifest_resolution_present: true,
            post_entry_operator_checked_untrusted_native_sideboard_resolution_present: true,
            post_entry_operator_model_owned_sideboard_resume_present: false,
            end_to_end_operator_loop_present: false,
            selected_listing_classifier_protocol_present: true,
            navigation_lifecycle_classifier_protocol_present: true,
            event_record_classifier_protocol_present: true,
            sideboard_classifier_protocol_present: true,
            duel_perception_classifier_protocol_present: true,
            native_checkpoint_duel_action_interface_present: true,
            player_visible_duel_decision_input_contract_present: true,
            player_visible_duel_scorer_transport_contract_present: true,
            player_visible_duel_selection_to_visible_control_bridge_present: true,
            player_visible_duel_gesture_contract_present: true,
            player_visible_duel_gesture_to_opaque_control_join_present: true,
            player_visible_duel_gesture_kernel_object_references_withheld: true,
            player_visible_duel_source_gesture_target_protocol_present: true,
            player_visible_duel_gesture_target_classifier_present: true,
            player_visible_duel_source_gesture_target_pixels_rehashed: true,
            player_visible_duel_gesture_target_protocol_ratified: false,
            player_visible_duel_gesture_continuation_target_binding_present: true,
            native_checkpoint_player_visible_only_duel_action_interface_present: false,
            current_duel_scorer_kernel_bookkeeping_withheld: false,
            native_checkpoint_pregame_interface_present: false,
            competitive_player_visible_pregame_request_contract_present: true,
            competitive_pregame_player_known_submitted_deck_configuration_present: true,
            competitive_pregame_player_known_deck_configuration_present: true,
            competitive_pregame_ordered_confirmed_bottom_history_present: true,
            competitive_pregame_public_context_contract_present: true,
            competitive_pregame_play_draw_context_present: true,
            competitive_pregame_match_score_context_present: true,
            competitive_pregame_score_response_contract_present: true,
            visible_accessibility_exact_text_probe_present: true,
            visible_accessibility_same_frame_pixel_corroboration_present: true,
            terminal_outcome_trained_pregame_head_present: false,
            non_model_pregame_scorer_present: true,
            competitive_pregame_heuristic_deployment_ratification_present:
                competitive_pregame_heuristic_ratification_present_v1(),
            competitive_pregame_session_ownership_bridge_present: true,
            competitive_pregame_capture_profile_present: false,
            competitive_pregame_card_and_control_surface_present: true,
            competitive_pregame_deck_bound_action_planner_present: true,
            competitive_pregame_immediate_recapture_preparation_present: true,
            competitive_pregame_postcondition_contract_present: true,
            competitive_pregame_input_actuator_present: true,
            competitive_pregame_capture_and_session_bridge_present: true,
            native_checkpoint_changed_sideboard_interface_present: false,
            competitive_player_visible_sideboard_payload_contract_present: true,
            competitive_player_visible_sideboard_score_binding_present: true,
            competitive_sideboard_score_response_contract_present: true,
        },
        status:
            MtgoCompetitiveStaticReadinessStatusV1::BlockedMissingRatificationsAndModelInterfaces,
        safe_for_live_capture: false,
        safe_for_input: false,
        safe_for_event_entry: false,
        safe_for_spending: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_report_names_both_modes_and_grants_no_authority() {
        let report = check_competitive_wiring_static_readiness_v1();
        assert_eq!(
            report.supported_event_kinds,
            vec![
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEventKindV1::Challenge
            ]
        );
        assert!(!report
            .semantic_ratifications
            .unchanged_sideboard_event_path_present_v1());
        assert!(
            !report
                .semantic_ratifications
                .competitive_pregame_evaluation_present
        );
        assert!(!report
            .authorization_ratifications
            .unchanged_sideboard_event_path_present_v1());
        assert!(report.known_wiring_gaps.operator_resource_bootstrap_present);
        assert!(
            report
                .known_wiring_gaps
                .checkpoint_bound_model_capability_commitment_present
        );
        assert!(
            report
                .known_wiring_gaps
                .player_visible_information_boundary_is_transport_independent
        );
        assert!(
            report
                .known_wiring_gaps
                .persisted_visible_game_log_parser_present
        );
        assert!(
            report
                .known_wiring_gaps
                .persisted_visible_game_log_local_corpus_passed
        );
        assert!(
            report
                .known_wiring_gaps
                .post_entry_operator_pairing_ready_game_log_baseline_present
        );
        assert!(
            report
                .known_wiring_gaps
                .post_entry_operator_attended_match_game_log_lease_present
        );
        assert!(
            report
                .known_wiring_gaps
                .post_entry_operator_attended_match_log_refresh_identity_retained
        );
        assert!(
            report
                .known_wiring_gaps
                .post_entry_operator_match_scoped_visible_game_log_refresh_present
        );
        assert!(
            report
                .known_wiring_gaps
                .player_visible_game_log_action_corroboration_present
        );
        assert!(
            report
                .known_wiring_gaps
                .player_visible_game_log_exact_rendered_prefix_binding_present
        );
        assert!(
            !report
                .known_wiring_gaps
                .player_visible_game_log_action_corroboration_grants_additional_input
        );
        assert!(
            report
                .known_wiring_gaps
                .external_public_history_import_contract_present
        );
        assert!(
            report
                .known_wiring_gaps
                .external_public_history_cross_source_ordering_policy_present
        );
        assert!(
            report
                .known_wiring_gaps
                .external_public_history_model_facts_only_present
        );
        assert!(
            report
                .known_wiring_gaps
                .external_public_history_adapter_metadata_withheld
        );
        assert!(
            report
                .known_wiring_gaps
                .ongoing_external_public_history_snapshot_replay_present
        );
        assert!(
            report
                .known_wiring_gaps
                .first_gameplay_decision_zero_action_history_supported
        );
        assert!(
            report
                .known_wiring_gaps
                .ongoing_public_history_to_player_visible_scorer_bridge_present
        );
        assert!(
            !report
                .known_wiring_gaps
                .native_checkpoint_external_public_history_import_present
        );
        assert!(report.known_wiring_gaps.pre_entry_operator_loop_present);
        assert!(report.known_wiring_gaps.post_entry_operator_loop_present);
        assert!(
            report
                .known_wiring_gaps
                .post_entry_operator_native_pregame_request_checkout_present
        );
        assert!(
            report
                .known_wiring_gaps
                .post_entry_operator_checked_untrusted_native_pregame_scoring_present
        );
        assert!(
            report
                .known_wiring_gaps
                .checked_untrusted_native_pregame_semantic_resolution_present
        );
        assert!(
            report
                .known_wiring_gaps
                .post_entry_operator_checked_untrusted_native_pregame_resolution_present
        );
        assert!(
            !report
                .known_wiring_gaps
                .post_entry_operator_model_owned_pregame_resume_present
        );
        assert!(
            report
                .known_wiring_gaps
                .post_entry_operator_native_sideboard_request_checkout_present
        );
        assert!(
            report
                .known_wiring_gaps
                .post_entry_operator_checked_untrusted_native_sideboard_scoring_present
        );
        assert!(
            report
                .known_wiring_gaps
                .checked_untrusted_native_sideboard_manifest_resolution_present
        );
        assert!(
            report
                .known_wiring_gaps
                .post_entry_operator_checked_untrusted_native_sideboard_resolution_present
        );
        assert!(
            !report
                .known_wiring_gaps
                .post_entry_operator_model_owned_sideboard_resume_present
        );
        assert!(!report.known_wiring_gaps.end_to_end_operator_loop_present);
        assert!(
            report
                .known_wiring_gaps
                .selected_listing_classifier_protocol_present
        );
        assert!(
            report
                .known_wiring_gaps
                .navigation_lifecycle_classifier_protocol_present
        );
        assert!(
            report
                .known_wiring_gaps
                .event_record_classifier_protocol_present
        );
        assert!(
            report
                .known_wiring_gaps
                .sideboard_classifier_protocol_present
        );
        assert!(
            report
                .known_wiring_gaps
                .duel_perception_classifier_protocol_present
        );
        assert!(
            report
                .known_wiring_gaps
                .native_checkpoint_duel_action_interface_present
        );
        assert!(
            report
                .known_wiring_gaps
                .player_visible_duel_decision_input_contract_present
        );
        assert!(
            !report
                .known_wiring_gaps
                .native_checkpoint_player_visible_only_duel_action_interface_present
        );
        assert!(
            !report
                .known_wiring_gaps
                .current_duel_scorer_kernel_bookkeeping_withheld
        );
        assert!(
            !report
                .known_wiring_gaps
                .native_checkpoint_pregame_interface_present
        );
        assert!(
            report
                .known_wiring_gaps
                .competitive_player_visible_pregame_request_contract_present
        );
        assert!(
            report
                .known_wiring_gaps
                .competitive_pregame_player_known_deck_configuration_present
        );
        assert!(
            report
                .known_wiring_gaps
                .competitive_pregame_ordered_confirmed_bottom_history_present
        );
        assert!(
            report
                .known_wiring_gaps
                .competitive_pregame_player_known_submitted_deck_configuration_present
        );
        assert!(
            report
                .known_wiring_gaps
                .competitive_pregame_public_context_contract_present
        );
        assert!(
            report
                .known_wiring_gaps
                .competitive_pregame_play_draw_context_present
        );
        assert!(
            report
                .known_wiring_gaps
                .competitive_pregame_match_score_context_present
        );
        assert!(
            report
                .known_wiring_gaps
                .competitive_pregame_score_response_contract_present
        );
        assert!(
            report
                .known_wiring_gaps
                .visible_accessibility_exact_text_probe_present
        );
        assert!(
            report
                .known_wiring_gaps
                .visible_accessibility_same_frame_pixel_corroboration_present
        );
        assert!(
            !report
                .known_wiring_gaps
                .terminal_outcome_trained_pregame_head_present
        );
        assert!(report.known_wiring_gaps.non_model_pregame_scorer_present);
        assert!(
            !report
                .known_wiring_gaps
                .competitive_pregame_heuristic_deployment_ratification_present
        );
        assert!(
            report
                .known_wiring_gaps
                .competitive_pregame_session_ownership_bridge_present
        );
        assert!(
            !report
                .known_wiring_gaps
                .competitive_pregame_capture_profile_present
        );
        assert!(
            report
                .known_wiring_gaps
                .competitive_pregame_card_and_control_surface_present
        );
        assert!(
            report
                .known_wiring_gaps
                .competitive_pregame_deck_bound_action_planner_present
        );
        assert!(
            report
                .known_wiring_gaps
                .competitive_pregame_immediate_recapture_preparation_present
        );
        assert!(
            report
                .known_wiring_gaps
                .competitive_pregame_postcondition_contract_present
        );
        assert!(
            report
                .known_wiring_gaps
                .competitive_pregame_input_actuator_present
        );
        assert!(
            report
                .known_wiring_gaps
                .competitive_pregame_capture_and_session_bridge_present
        );
        assert!(
            !report
                .known_wiring_gaps
                .native_checkpoint_changed_sideboard_interface_present
        );
        assert!(
            report
                .known_wiring_gaps
                .competitive_player_visible_sideboard_payload_contract_present
        );
        assert!(
            report
                .known_wiring_gaps
                .competitive_player_visible_sideboard_score_binding_present
        );
        assert!(
            report
                .known_wiring_gaps
                .competitive_sideboard_score_response_contract_present
        );
        assert!(!report.safe_for_live_capture);
        assert!(!report.safe_for_input);
        assert!(!report.safe_for_event_entry);
        assert!(!report.safe_for_spending);
    }
}
