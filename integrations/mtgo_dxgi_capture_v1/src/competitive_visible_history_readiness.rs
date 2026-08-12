use mtgo_blackbox_v1::MtgoCompetitiveEventKindV1;
use serde::{Deserialize, Serialize};

pub const MTGO_COMPETITIVE_VISIBLE_HISTORY_READINESS_SCHEMA_V1: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveVisibleHistoryReadinessStatusV1 {
    AdapterReadyKernelImportMissing,
}

/// Static, non-authorizing inventory of the player-visible history path.
///
/// The information boundary is independent of transport. Persisted text is
/// eligible only when it is the same information MTGO renders for the seated
/// player. Private protocol fields, hidden state, and non-rendered identifiers
/// are not eligible inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveVisibleHistoryReadinessV1 {
    pub schema_version: u32,
    pub purpose: String,
    pub supported_event_kinds: Vec<MtgoCompetitiveEventKindV1>,
    pub player_visible_information_boundary_is_transport_independent: bool,
    pub pixels_only_transport_required: bool,
    pub hidden_client_state_permitted: bool,
    pub visible_accessibility_requires_pixel_corroboration: bool,
    pub raw_accessibility_metadata_permitted_as_semantic_input: bool,
    pub persisted_visible_game_log_parser_present: bool,
    pub nonrendered_game_log_metadata_discarded: bool,
    pub process_epoch_game_log_binding_present: bool,
    pub exact_competitive_game_binding_present: bool,
    pub public_game_log_semantic_projection_present: bool,
    pub confirmed_model_decision_history_present: bool,
    pub dual_source_exact_game_memory_present: bool,
    pub explicit_cross_source_total_order_defined: bool,
    pub native_checkpoint_external_public_history_import_present: bool,
    pub checkpoint_consumes_external_public_history: bool,
    pub game_log_complete_for_current_state_reconstruction: bool,
    pub status: MtgoCompetitiveVisibleHistoryReadinessStatusV1,
    pub grants_model_scoring: bool,
    pub grants_input: bool,
    pub grants_event_entry: bool,
    pub grants_spending: bool,
}

/// Reports static source and import wiring without reading the MTGO process,
/// files, pixels, correspondence, checkpoint bytes, or account state.
pub fn check_competitive_visible_history_readiness_v1() -> MtgoCompetitiveVisibleHistoryReadinessV1
{
    MtgoCompetitiveVisibleHistoryReadinessV1 {
        schema_version: MTGO_COMPETITIVE_VISIBLE_HISTORY_READINESS_SCHEMA_V1,
        purpose: "non_actuating_static_competitive_visible_history_readiness_v1".to_owned(),
        supported_event_kinds: vec![
            MtgoCompetitiveEventKindV1::League,
            MtgoCompetitiveEventKindV1::Challenge,
        ],
        player_visible_information_boundary_is_transport_independent: true,
        pixels_only_transport_required: false,
        hidden_client_state_permitted: false,
        visible_accessibility_requires_pixel_corroboration: true,
        raw_accessibility_metadata_permitted_as_semantic_input: false,
        persisted_visible_game_log_parser_present: true,
        nonrendered_game_log_metadata_discarded: true,
        process_epoch_game_log_binding_present: true,
        exact_competitive_game_binding_present: true,
        public_game_log_semantic_projection_present: true,
        confirmed_model_decision_history_present: true,
        dual_source_exact_game_memory_present: true,
        explicit_cross_source_total_order_defined: false,
        native_checkpoint_external_public_history_import_present: false,
        checkpoint_consumes_external_public_history: false,
        game_log_complete_for_current_state_reconstruction: false,
        status: MtgoCompetitiveVisibleHistoryReadinessStatusV1::AdapterReadyKernelImportMissing,
        grants_model_scoring: false,
        grants_input: false,
        grants_event_entry: false,
        grants_spending: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_distinguishes_information_eligibility_from_model_authority() {
        let report = check_competitive_visible_history_readiness_v1();
        assert_eq!(
            report.supported_event_kinds,
            vec![
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEventKindV1::Challenge
            ]
        );
        assert!(report.player_visible_information_boundary_is_transport_independent);
        assert!(!report.pixels_only_transport_required);
        assert!(!report.hidden_client_state_permitted);
        assert!(report.visible_accessibility_requires_pixel_corroboration);
        assert!(!report.raw_accessibility_metadata_permitted_as_semantic_input);
        assert!(report.persisted_visible_game_log_parser_present);
        assert!(report.nonrendered_game_log_metadata_discarded);
        assert!(report.process_epoch_game_log_binding_present);
        assert!(report.exact_competitive_game_binding_present);
        assert!(report.public_game_log_semantic_projection_present);
        assert!(report.confirmed_model_decision_history_present);
        assert!(report.dual_source_exact_game_memory_present);
        assert!(!report.explicit_cross_source_total_order_defined);
        assert!(!report.native_checkpoint_external_public_history_import_present);
        assert!(!report.checkpoint_consumes_external_public_history);
        assert!(!report.game_log_complete_for_current_state_reconstruction);
        assert!(!report.grants_model_scoring);
        assert!(!report.grants_input);
        assert!(!report.grants_event_entry);
        assert!(!report.grants_spending);
    }
}
