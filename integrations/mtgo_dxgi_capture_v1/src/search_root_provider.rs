//! Search-root provider slot (spec 6.4). The ratified bounded search needs a
//! full kernel session; a player-visible search root constructor is its own
//! future ratification. Until then this slot answers raw policy only, and
//! every result must be labeled accordingly.

use crate::{MtgoDeploymentSlotDescriptorV1, MtgoDeploymentSlotKindV1, MtgoDeploymentSlotV1};
use mtgo_blackbox_v1::MtgoPlayerVisibleDuelDecisionInputV1;
use serde::{Deserialize, Serialize};

pub const MTGO_SEARCH_ROOT_NOT_RATIFIED_REASON_V1: &str =
    "player_visible_search_root_not_ratified_v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoSearchRootDecisionV1 {
    /// No search root exists for this decision; the raw policy scores it and
    /// the result is labeled raw policy.
    RawPolicyOnly { reason: String },
}

pub trait MtgoSearchRootProviderV1 {
    fn search_root_v1(
        &self,
        decision: &MtgoPlayerVisibleDuelDecisionInputV1,
    ) -> MtgoSearchRootDecisionV1;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MtgoRawPolicyOnlySearchRootProviderV1;

impl MtgoSearchRootProviderV1 for MtgoRawPolicyOnlySearchRootProviderV1 {
    fn search_root_v1(
        &self,
        _decision: &MtgoPlayerVisibleDuelDecisionInputV1,
    ) -> MtgoSearchRootDecisionV1 {
        MtgoSearchRootDecisionV1::RawPolicyOnly {
            reason: MTGO_SEARCH_ROOT_NOT_RATIFIED_REASON_V1.to_owned(),
        }
    }
}

impl MtgoDeploymentSlotV1 for MtgoRawPolicyOnlySearchRootProviderV1 {
    fn slot_descriptor_v1(&self, kind: MtgoDeploymentSlotKindV1) -> MtgoDeploymentSlotDescriptorV1 {
        MtgoDeploymentSlotDescriptorV1 {
            kind,
            implementation_id: "search_root_provider_raw_policy_only_v1".to_owned(),
            is_placeholder: true,
            qualified_for_live: false,
            contract_version: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtgo_blackbox_v1::{MtgoPlayerVisibleDuelDecisionInputV1, MtgoPlayerVisibleDuelStateV1};

    #[test]
    fn placeholder_answers_raw_policy_only_with_the_fixed_reason() {
        let provider = MtgoRawPolicyOnlySearchRootProviderV1;
        let fixture: serde_json::Value = serde_json::from_str(SAMPLE_INPUT_JSON).unwrap();
        let input: MtgoPlayerVisibleDuelDecisionInputV1 =
            serde_json::from_value(fixture["model_input"].clone()).unwrap();
        assert_eq!(
            provider.search_root_v1(&input),
            MtgoSearchRootDecisionV1::RawPolicyOnly {
                reason: MTGO_SEARCH_ROOT_NOT_RATIFIED_REASON_V1.to_owned()
            }
        );
        let descriptor = provider.slot_descriptor_v1(MtgoDeploymentSlotKindV1::SearchRootProvider);
        assert_eq!(
            descriptor.implementation_id,
            "search_root_provider_raw_policy_only_v1"
        );
        assert!(descriptor.is_placeholder);
        assert!(!descriptor.qualified_for_live);
        let _ = std::any::type_name::<MtgoPlayerVisibleDuelStateV1>();
    }

    /// Reuse the adapter's conformance fixture rather than hand-building the
    /// large state struct. Its top-level object has the keys schema_version,
    /// fixture_id, model_input, expected_input_commitment_sha256,
    /// synthetic_simulator_setup, expected_flat_v2; the decision input is the
    /// `model_input` object.
    const SAMPLE_INPUT_JSON: &str = include_str!(
        "../../mtgo_blackbox_v1/fixtures/player_visible_flat_v2_conformance_source_v1.json"
    );
}
