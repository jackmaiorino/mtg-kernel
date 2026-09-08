use crate::competitive_duel_lifecycle_evaluation::RATIFIED_COMPETITIVE_DUEL_LIFECYCLE_EVALUATION_COMMITMENT_V1;
use crate::competitive_event_listing_evaluation::RATIFIED_COMPETITIVE_EVENT_LISTING_EVALUATION_COMMITMENT_V1;
use crate::competitive_event_record_evaluation::RATIFIED_COMPETITIVE_EVENT_RECORD_EVALUATION_COMMITMENT_V1;
use crate::competitive_navigation_evaluation::RATIFIED_COMPETITIVE_NAVIGATION_EVALUATION_COMMITMENT_V1;
use crate::competitive_pregame_evaluation::RATIFIED_COMPETITIVE_PREGAME_EVALUATION_COMMITMENT_V1;
use crate::competitive_pregame_public_context_evaluation::RATIFIED_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_EVALUATION_COMMITMENT_V1;
use crate::competitive_sideboard_evaluation::RATIFIED_COMPETITIVE_SIDEBOARD_EVALUATION_COMMITMENT_V1;
use crate::duel_gesture_evaluation::RATIFIED_DUEL_GESTURE_EVALUATION_COMMITMENT_V1;
use crate::duel_perception_evaluation::RATIFIED_PLAYER_VISIBLE_DUEL_PERCEPTION_EVALUATION_COMMITMENT_V2;
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
    pub competitive_pregame_evaluation_present: bool,
    pub competitive_pregame_public_context_evaluation_present: bool,
    /// The retained V1 field name predates the four-slice contract. This root
    /// now covers unchanged and changed configurations in both event modes.
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
            && self.competitive_pregame_evaluation_present
            && self.competitive_pregame_public_context_evaluation_present
            && self.changed_sideboard_evaluation_present
    }

    pub fn changed_sideboard_event_path_present_v1(&self) -> bool {
        self.unchanged_sideboard_event_path_present_v1()
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
        duel_perception_evaluation_present:
            RATIFIED_PLAYER_VISIBLE_DUEL_PERCEPTION_EVALUATION_COMMITMENT_V2.is_some(),
        duel_gesture_evaluation_present: RATIFIED_DUEL_GESTURE_EVALUATION_COMMITMENT_V1.is_some(),
        duel_lifecycle_evaluation_present:
            RATIFIED_COMPETITIVE_DUEL_LIFECYCLE_EVALUATION_COMMITMENT_V1.is_some(),
        competitive_pregame_evaluation_present:
            RATIFIED_COMPETITIVE_PREGAME_EVALUATION_COMMITMENT_V1.is_some(),
        competitive_pregame_public_context_evaluation_present:
            RATIFIED_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_EVALUATION_COMMITMENT_V1.is_some(),
        changed_sideboard_evaluation_present:
            RATIFIED_COMPETITIVE_SIDEBOARD_EVALUATION_COMMITMENT_V1.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn readiness_with_sideboard_v1(
        changed_sideboard_evaluation_present: bool,
    ) -> MtgoCompetitiveSemanticRatificationReadinessV1 {
        MtgoCompetitiveSemanticRatificationReadinessV1 {
            schema_version: MTGO_COMPETITIVE_SEMANTIC_READINESS_SCHEMA_V1,
            navigation_evaluation_present: true,
            event_listing_evaluation_present: true,
            event_record_evaluation_present: true,
            duel_perception_evaluation_present: true,
            duel_gesture_evaluation_present: true,
            duel_lifecycle_evaluation_present: true,
            competitive_pregame_evaluation_present: true,
            competitive_pregame_public_context_evaluation_present: true,
            changed_sideboard_evaluation_present,
        }
    }

    #[test]
    fn four_slice_sideboard_evaluation_is_required_for_both_model_paths() {
        let missing = readiness_with_sideboard_v1(false);
        assert!(!missing.unchanged_sideboard_event_path_present_v1());
        assert!(!missing.changed_sideboard_event_path_present_v1());

        let present = readiness_with_sideboard_v1(true);
        assert!(present.unchanged_sideboard_event_path_present_v1());
        assert!(present.changed_sideboard_event_path_present_v1());
        assert!(!present.grants_live_authority_v1());
    }

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
                "competitive_pregame_evaluation_present": false,
                "competitive_pregame_public_context_evaluation_present": false,
                "changed_sideboard_evaluation_present": false
            })
        );
    }
}
