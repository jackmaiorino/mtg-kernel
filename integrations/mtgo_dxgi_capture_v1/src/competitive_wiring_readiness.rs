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
    pub end_to_end_operator_loop_present: bool,
    pub native_checkpoint_duel_action_interface_present: bool,
    pub native_checkpoint_pregame_interface_present: bool,
    pub native_checkpoint_changed_sideboard_interface_present: bool,
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
            end_to_end_operator_loop_present: false,
            native_checkpoint_duel_action_interface_present: true,
            native_checkpoint_pregame_interface_present: false,
            native_checkpoint_changed_sideboard_interface_present: false,
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
        assert!(!report
            .authorization_ratifications
            .unchanged_sideboard_event_path_present_v1());
        assert!(
            report
                .known_wiring_gaps
                .native_checkpoint_duel_action_interface_present
        );
        assert!(
            !report
                .known_wiring_gaps
                .native_checkpoint_pregame_interface_present
        );
        assert!(
            !report
                .known_wiring_gaps
                .native_checkpoint_changed_sideboard_interface_present
        );
        assert!(!report.safe_for_live_capture);
        assert!(!report.safe_for_input);
        assert!(!report.safe_for_event_entry);
        assert!(!report.safe_for_spending);
    }
}
