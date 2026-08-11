use crate::competitive_duel_lifecycle_evaluation::RATIFIED_COMPETITIVE_DUEL_LIFECYCLE_EVALUATION_COMMITMENT_V1;
use crate::competitive_event_listing_evaluation::RATIFIED_COMPETITIVE_EVENT_LISTING_EVALUATION_COMMITMENT_V1;
use crate::competitive_event_record_evaluation::RATIFIED_COMPETITIVE_EVENT_RECORD_EVALUATION_COMMITMENT_V1;
use crate::competitive_navigation_evaluation::RATIFIED_COMPETITIVE_NAVIGATION_EVALUATION_COMMITMENT_V1;
use crate::competitive_sideboard_evaluation::RATIFIED_COMPETITIVE_SIDEBOARD_EVALUATION_COMMITMENT_V1;
use crate::duel_gesture_evaluation::RATIFIED_DUEL_GESTURE_EVALUATION_COMMITMENT_V1;
use crate::duel_perception_evaluation::RATIFIED_DUEL_PERCEPTION_EVALUATION_COMMITMENT_V1;
use serde::{Deserialize, Serialize};

pub const MTGO_COMPETITIVE_SEMANTIC_READINESS_SCHEMA_V1: u32 = 1;

/// Non-authorizing visibility into the compile-pinned semantic evaluation
/// roots required by the League and Challenge adapter.
///
/// This report never exposes a ratification commitment and cannot admit a
/// profile, produce semantic evidence, score an action, or authorize input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveSemanticRatificationReadinessV1 {
    pub schema_version: u32,
    pub navigation_evaluation_present: bool,
    pub event_listing_evaluation_present: bool,
    pub event_record_evaluation_present: bool,
    pub duel_perception_evaluation_present: bool,
    pub duel_gesture_evaluation_present: bool,
    pub duel_lifecycle_evaluation_present: bool,
    pub changed_sideboard_evaluation_present: bool,
}

impl MtgoCompetitiveSemanticRatificationReadinessV1 {
    pub fn unchanged_sideboard_event_path_present_v1(&self) -> bool {
        self.navigation_evaluation_present
            && self.event_listing_evaluation_present
            && self.event_record_evaluation_present
            && self.duel_perception_evaluation_present
            && self.duel_gesture_evaluation_present
            && self.duel_lifecycle_evaluation_present
    }

    pub fn changed_sideboard_event_path_present_v1(&self) -> bool {
        self.unchanged_sideboard_event_path_present_v1()
            && self.changed_sideboard_evaluation_present
    }

    pub fn grants_live_authority_v1(&self) -> bool {
        false
    }
}

pub fn competitive_semantic_ratification_readiness_v1(
) -> MtgoCompetitiveSemanticRatificationReadinessV1 {
    MtgoCompetitiveSemanticRatificationReadinessV1 {
        schema_version: MTGO_COMPETITIVE_SEMANTIC_READINESS_SCHEMA_V1,
        navigation_evaluation_present: RATIFIED_COMPETITIVE_NAVIGATION_EVALUATION_COMMITMENT_V1
            .is_some(),
        event_listing_evaluation_present:
            RATIFIED_COMPETITIVE_EVENT_LISTING_EVALUATION_COMMITMENT_V1.is_some(),
        event_record_evaluation_present: RATIFIED_COMPETITIVE_EVENT_RECORD_EVALUATION_COMMITMENT_V1
            .is_some(),
        duel_perception_evaluation_present: RATIFIED_DUEL_PERCEPTION_EVALUATION_COMMITMENT_V1
            .is_some(),
        duel_gesture_evaluation_present: RATIFIED_DUEL_GESTURE_EVALUATION_COMMITMENT_V1.is_some(),
        duel_lifecycle_evaluation_present:
            RATIFIED_COMPETITIVE_DUEL_LIFECYCLE_EVALUATION_COMMITMENT_V1.is_some(),
        changed_sideboard_evaluation_present:
            RATIFIED_COMPETITIVE_SIDEBOARD_EVALUATION_COMMITMENT_V1.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_readiness_is_non_authorizing_and_current_roots_are_empty() {
        let readiness = competitive_semantic_ratification_readiness_v1();
        assert_eq!(
            readiness.schema_version,
            MTGO_COMPETITIVE_SEMANTIC_READINESS_SCHEMA_V1
        );
        assert!(!readiness.unchanged_sideboard_event_path_present_v1());
        assert!(!readiness.changed_sideboard_event_path_present_v1());
        assert!(!readiness.grants_live_authority_v1());
        assert_eq!(
            serde_json::to_value(readiness).unwrap(),
            serde_json::json!({
                "schema_version": 1,
                "navigation_evaluation_present": false,
                "event_listing_evaluation_present": false,
                "event_record_evaluation_present": false,
                "duel_perception_evaluation_present": false,
                "duel_gesture_evaluation_present": false,
                "duel_lifecycle_evaluation_present": false,
                "changed_sideboard_evaluation_present": false
            })
        );
    }
}
