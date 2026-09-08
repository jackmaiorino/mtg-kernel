use crate::{
    make_offline_intent_v1, payload_leaf_inventory_v1, MtgoContractErrorV1,
    MtgoOfflineActionIntentV1, ValidatedMtgoObservedDecisionV1,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoExpectedVisibleLeafV1 {
    pub json_pointer: String,
    pub before_value_sha256: String,
    pub after_value_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoMockPostconditionV1 {
    pub required_visible_leaf: MtgoExpectedVisibleLeafV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingMockActionV1 {
    intent: MtgoOfflineActionIntentV1,
    postcondition: MtgoMockPostconditionV1,
}

#[derive(Debug, Default)]
pub struct MtgoMockActuatorV1 {
    pending: Option<PendingMockActionV1>,
    halted: bool,
}

impl MtgoMockActuatorV1 {
    pub fn submit(
        &mut self,
        source: &ValidatedMtgoObservedDecisionV1,
        selected_index: usize,
        postcondition: MtgoMockPostconditionV1,
    ) -> Result<MtgoOfflineActionIntentV1, MtgoContractErrorV1> {
        if self.halted {
            return Err(MtgoContractErrorV1::new(
                "mock_halted",
                "the mock actuator is fail-closed",
            ));
        }
        if self.pending.is_some() {
            return Err(MtgoContractErrorV1::new(
                "action_pending",
                "the prior action has no confirmed visible postcondition",
            ));
        }
        let required = &postcondition.required_visible_leaf;
        if !required.json_pointer.starts_with("/observation/")
            || !looks_like_sha256_v1(&required.before_value_sha256)
            || !looks_like_sha256_v1(&required.after_value_sha256)
            || required.before_value_sha256 == required.after_value_sha256
        {
            return Err(MtgoContractErrorV1::new(
                "postcondition_invalid",
                "a mock action requires one well-formed visible observation-leaf predicate",
            ));
        }
        let before_leaf_present = payload_leaf_inventory_v1(&source.record.payload)?
            .iter()
            .any(|leaf| {
                leaf.requires_visible_evidence
                    && leaf.json_pointer == required.json_pointer
                    && leaf.value_sha256 == required.before_value_sha256
            });
        if !before_leaf_present {
            return Err(MtgoContractErrorV1::new(
                "postcondition_source_mismatch",
                "the declared before-value is not a visible leaf of the source decision",
            ));
        }
        let intent = make_offline_intent_v1(source, selected_index)?;
        self.pending = Some(PendingMockActionV1 {
            intent: intent.clone(),
            postcondition,
        });
        Ok(intent)
    }

    pub fn confirm(
        &mut self,
        observed: &ValidatedMtgoObservedDecisionV1,
    ) -> Result<(), MtgoContractErrorV1> {
        if self.halted {
            return Err(MtgoContractErrorV1::new(
                "mock_halted",
                "the mock actuator is fail-closed",
            ));
        }
        let Some(pending) = self.pending.take() else {
            return Err(MtgoContractErrorV1::new(
                "no_action_pending",
                "there is no submitted mock action to confirm",
            ));
        };
        let frame_advanced = observed.frame_id() != pending.intent.frame_id
            && observed.frame_sequence() > pending.intent.frame_sequence;
        let decision_changed =
            observed.decision_commitment_sha256() != pending.intent.decision_commitment_sha256;
        let required = &pending.postcondition.required_visible_leaf;
        let required_leaf_present = match payload_leaf_inventory_v1(&observed.record.payload) {
            Ok(leaves) => leaves.iter().any(|leaf| {
                leaf.requires_visible_evidence
                    && leaf.json_pointer == required.json_pointer
                    && leaf.value_sha256 == required.after_value_sha256
            }),
            Err(_) => false,
        };
        if !frame_advanced || !decision_changed || !required_leaf_present {
            self.halted = true;
            return Err(MtgoContractErrorV1::new(
                "postcondition_missing",
                "the next validated observation did not confirm the mock action",
            ));
        }
        Ok(())
    }

    pub fn is_halted(&self) -> bool {
        self.halted
    }

    pub fn has_pending_action(&self) -> bool {
        self.pending.is_some()
    }
}

fn looks_like_sha256_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
