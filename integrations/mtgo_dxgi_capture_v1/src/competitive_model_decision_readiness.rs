use mtgo_blackbox_v1::MtgoCompetitiveEventKindV1;
use serde::{Deserialize, Serialize};

pub const MTGO_COMPETITIVE_MODEL_DECISION_READINESS_SCHEMA_V1: u32 = 1;

/// Static, non-authorizing inventory of the three model-owned decision
/// surfaces needed for League and Challenge play.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveModelDecisionReadinessV1 {
    pub schema_version: u32,
    pub purpose: String,
    pub supported_event_kinds: Vec<MtgoCompetitiveEventKindV1>,
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
    pub ongoing_player_visible_history_to_duel_scorer_bridge_present: bool,
    pub post_entry_operator_player_visible_gameplay_selection_present: bool,
    pub post_entry_operator_player_visible_gesture_ownership_present: bool,
    pub post_entry_operator_player_visible_target_ownership_present: bool,
    pub post_entry_operator_player_visible_preinput_refresh_present: bool,
    pub post_entry_operator_player_visible_private_pointer_preparation_present: bool,
    pub post_entry_operator_player_visible_before_input_postcondition_present: bool,
    pub post_entry_operator_player_visible_live_primitive_actuator_present: bool,
    pub post_entry_operator_player_visible_confirmation_cycle_present: bool,
    pub public_combat_continuation_ownership_path_present: bool,
    pub public_combat_continuation_accepts_external_scorer: bool,
    pub native_checkpoint_player_visible_only_duel_action_interface_present: bool,
    pub current_duel_scorer_kernel_bookkeeping_withheld: bool,
    pub public_model_owned_duel_action_path_present: bool,
    pub native_checkpoint_pregame_interface_present: bool,
    pub public_player_visible_pregame_request_contract_present: bool,
    pub public_player_known_submitted_pregame_deck_configuration_present: bool,
    pub public_player_known_pregame_deck_configuration_present: bool,
    pub public_pregame_ordered_confirmed_bottom_history_present: bool,
    pub public_completed_match_history_contract_present: bool,
    pub later_game_pregame_completed_history_binding_present: bool,
    pub checked_untrusted_pregame_score_response_contract_present: bool,
    pub checked_untrusted_pregame_semantic_resolution_present: bool,
    pub terminal_outcome_trained_pregame_head_present: bool,
    pub public_model_owned_pregame_action_path_present: bool,
    pub native_checkpoint_sideboard_interface_present: bool,
    pub public_player_visible_sideboard_payload_contract_present: bool,
    pub public_player_visible_sideboard_score_binding_present: bool,
    pub sideboard_completed_history_binding_present: bool,
    pub checked_untrusted_sideboard_score_response_contract_present: bool,
    pub checked_untrusted_sideboard_manifest_resolution_present: bool,
    pub public_player_visible_sideboard_sequential_deliberation_contract_present: bool,
    pub completed_visible_history_to_sequential_sideboard_scorer_bridge_present: bool,
    pub whole_target_sideboard_response_satisfies_native_model_provenance: bool,
    pub terminal_outcome_trained_sideboard_head_present: bool,
    pub public_model_owned_changed_sideboard_path_present: bool,
    pub public_model_owned_unchanged_sideboard_path_present: bool,
    pub all_required_model_decision_surfaces_present: bool,
    pub grants_live_authority: bool,
}

impl MtgoCompetitiveModelDecisionReadinessV1 {
    pub fn recompute_all_required_model_decision_surfaces_present_v1(&self) -> bool {
        self.native_checkpoint_duel_action_interface_present
            && self.player_visible_duel_decision_input_contract_present
            && self.player_visible_duel_scorer_transport_contract_present
            && self.player_visible_duel_selection_to_visible_control_bridge_present
            && self.player_visible_duel_gesture_contract_present
            && self.player_visible_duel_gesture_to_opaque_control_join_present
            && self.player_visible_duel_gesture_kernel_object_references_withheld
            && self.player_visible_duel_source_gesture_target_protocol_present
            && self.player_visible_duel_gesture_target_classifier_present
            && self.player_visible_duel_source_gesture_target_pixels_rehashed
            && self.player_visible_duel_gesture_target_protocol_ratified
            && self.player_visible_duel_gesture_continuation_target_binding_present
            && self.ongoing_player_visible_history_to_duel_scorer_bridge_present
            && self.post_entry_operator_player_visible_gameplay_selection_present
            && self.post_entry_operator_player_visible_gesture_ownership_present
            && self.post_entry_operator_player_visible_target_ownership_present
            && self.post_entry_operator_player_visible_preinput_refresh_present
            && self.post_entry_operator_player_visible_private_pointer_preparation_present
            && self.post_entry_operator_player_visible_before_input_postcondition_present
            && self.post_entry_operator_player_visible_live_primitive_actuator_present
            && self.post_entry_operator_player_visible_confirmation_cycle_present
            && self.public_combat_continuation_ownership_path_present
            && !self.public_combat_continuation_accepts_external_scorer
            && self.native_checkpoint_player_visible_only_duel_action_interface_present
            && self.current_duel_scorer_kernel_bookkeeping_withheld
            && self.public_model_owned_duel_action_path_present
            && self.native_checkpoint_pregame_interface_present
            && self.public_player_known_submitted_pregame_deck_configuration_present
            && self.public_player_known_pregame_deck_configuration_present
            && self.public_pregame_ordered_confirmed_bottom_history_present
            && self.public_completed_match_history_contract_present
            && self.later_game_pregame_completed_history_binding_present
            && self.terminal_outcome_trained_pregame_head_present
            && self.public_model_owned_pregame_action_path_present
            && self.native_checkpoint_sideboard_interface_present
            && self.public_player_visible_sideboard_payload_contract_present
            && self.public_player_visible_sideboard_score_binding_present
            && self.sideboard_completed_history_binding_present
            && self.public_player_visible_sideboard_sequential_deliberation_contract_present
            && self.completed_visible_history_to_sequential_sideboard_scorer_bridge_present
            && !self.whole_target_sideboard_response_satisfies_native_model_provenance
            && self.terminal_outcome_trained_sideboard_head_present
            && self.public_model_owned_changed_sideboard_path_present
            && self.public_model_owned_unchanged_sideboard_path_present
    }
}

/// Reports model-decision wiring only. It reads no process, pixels,
/// correspondence, checkpoint bytes, or account state and grants no authority.
pub fn check_competitive_model_decision_readiness_v1() -> MtgoCompetitiveModelDecisionReadinessV1 {
    let mut report = MtgoCompetitiveModelDecisionReadinessV1 {
        schema_version: MTGO_COMPETITIVE_MODEL_DECISION_READINESS_SCHEMA_V1,
        purpose: "non_actuating_static_competitive_model_decision_readiness_v1".to_owned(),
        supported_event_kinds: vec![
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ],
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
        ongoing_player_visible_history_to_duel_scorer_bridge_present: true,
        post_entry_operator_player_visible_gameplay_selection_present: true,
        post_entry_operator_player_visible_gesture_ownership_present: true,
        post_entry_operator_player_visible_target_ownership_present: true,
        post_entry_operator_player_visible_preinput_refresh_present: true,
        post_entry_operator_player_visible_private_pointer_preparation_present: true,
        post_entry_operator_player_visible_before_input_postcondition_present: true,
        post_entry_operator_player_visible_live_primitive_actuator_present: true,
        post_entry_operator_player_visible_confirmation_cycle_present: true,
        public_combat_continuation_ownership_path_present: true,
        public_combat_continuation_accepts_external_scorer: false,
        native_checkpoint_player_visible_only_duel_action_interface_present: false,
        current_duel_scorer_kernel_bookkeeping_withheld: false,
        public_model_owned_duel_action_path_present: false,
        native_checkpoint_pregame_interface_present: false,
        public_player_visible_pregame_request_contract_present: true,
        public_player_known_submitted_pregame_deck_configuration_present: true,
        public_player_known_pregame_deck_configuration_present: true,
        public_pregame_ordered_confirmed_bottom_history_present: true,
        public_completed_match_history_contract_present: true,
        later_game_pregame_completed_history_binding_present: true,
        checked_untrusted_pregame_score_response_contract_present: true,
        checked_untrusted_pregame_semantic_resolution_present: true,
        terminal_outcome_trained_pregame_head_present: false,
        public_model_owned_pregame_action_path_present: false,
        native_checkpoint_sideboard_interface_present: false,
        public_player_visible_sideboard_payload_contract_present: true,
        public_player_visible_sideboard_score_binding_present: true,
        sideboard_completed_history_binding_present: true,
        checked_untrusted_sideboard_score_response_contract_present: true,
        checked_untrusted_sideboard_manifest_resolution_present: true,
        public_player_visible_sideboard_sequential_deliberation_contract_present: true,
        completed_visible_history_to_sequential_sideboard_scorer_bridge_present: true,
        whole_target_sideboard_response_satisfies_native_model_provenance: false,
        terminal_outcome_trained_sideboard_head_present: false,
        public_model_owned_changed_sideboard_path_present: false,
        public_model_owned_unchanged_sideboard_path_present: false,
        all_required_model_decision_surfaces_present: false,
        grants_live_authority: false,
    };
    report.all_required_model_decision_surfaces_present =
        report.recompute_all_required_model_decision_surfaces_present_v1();
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_report_blocks_all_three_competitive_model_surfaces() {
        let report = check_competitive_model_decision_readiness_v1();
        assert_eq!(
            report.supported_event_kinds,
            vec![
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEventKindV1::Challenge
            ]
        );
        assert!(report.native_checkpoint_duel_action_interface_present);
        assert!(report.player_visible_duel_decision_input_contract_present);
        assert!(report.player_visible_duel_scorer_transport_contract_present);
        assert!(report.player_visible_duel_selection_to_visible_control_bridge_present);
        assert!(report.player_visible_duel_gesture_contract_present);
        assert!(report.player_visible_duel_gesture_to_opaque_control_join_present);
        assert!(report.player_visible_duel_gesture_kernel_object_references_withheld);
        assert!(report.player_visible_duel_source_gesture_target_protocol_present);
        assert!(report.player_visible_duel_gesture_target_classifier_present);
        assert!(report.player_visible_duel_source_gesture_target_pixels_rehashed);
        assert!(!report.player_visible_duel_gesture_target_protocol_ratified);
        assert!(report.player_visible_duel_gesture_continuation_target_binding_present);
        assert!(report.ongoing_player_visible_history_to_duel_scorer_bridge_present);
        assert!(report.post_entry_operator_player_visible_gameplay_selection_present);
        assert!(report.post_entry_operator_player_visible_gesture_ownership_present);
        assert!(report.post_entry_operator_player_visible_target_ownership_present);
        assert!(report.post_entry_operator_player_visible_preinput_refresh_present);
        assert!(report.post_entry_operator_player_visible_private_pointer_preparation_present);
        assert!(report.post_entry_operator_player_visible_before_input_postcondition_present);
        assert!(report.post_entry_operator_player_visible_live_primitive_actuator_present);
        assert!(report.post_entry_operator_player_visible_confirmation_cycle_present);
        assert!(report.public_combat_continuation_ownership_path_present);
        assert!(!report.public_combat_continuation_accepts_external_scorer);
        assert!(!report.native_checkpoint_player_visible_only_duel_action_interface_present);
        assert!(!report.current_duel_scorer_kernel_bookkeeping_withheld);
        assert!(!report.public_model_owned_duel_action_path_present);
        assert!(!report.native_checkpoint_pregame_interface_present);
        assert!(report.public_player_visible_pregame_request_contract_present);
        assert!(report.public_player_known_submitted_pregame_deck_configuration_present);
        assert!(report.public_player_known_pregame_deck_configuration_present);
        assert!(report.public_pregame_ordered_confirmed_bottom_history_present);
        assert!(report.public_completed_match_history_contract_present);
        assert!(report.later_game_pregame_completed_history_binding_present);
        assert!(report.checked_untrusted_pregame_score_response_contract_present);
        assert!(report.checked_untrusted_pregame_semantic_resolution_present);
        assert!(!report.public_model_owned_pregame_action_path_present);
        assert!(!report.native_checkpoint_sideboard_interface_present);
        assert!(report.public_player_visible_sideboard_payload_contract_present);
        assert!(report.public_player_visible_sideboard_score_binding_present);
        assert!(report.sideboard_completed_history_binding_present);
        assert!(report.checked_untrusted_sideboard_score_response_contract_present);
        assert!(report.checked_untrusted_sideboard_manifest_resolution_present);
        assert!(report.public_player_visible_sideboard_sequential_deliberation_contract_present);
        assert!(report.completed_visible_history_to_sequential_sideboard_scorer_bridge_present);
        assert!(!report.whole_target_sideboard_response_satisfies_native_model_provenance);
        assert!(!report.public_model_owned_changed_sideboard_path_present);
        assert!(!report.public_model_owned_unchanged_sideboard_path_present);
        assert_eq!(
            report.all_required_model_decision_surfaces_present,
            report.recompute_all_required_model_decision_surfaces_present_v1()
        );
        assert!(!report.all_required_model_decision_surfaces_present);
        assert!(!report.grants_live_authority);
    }
}
