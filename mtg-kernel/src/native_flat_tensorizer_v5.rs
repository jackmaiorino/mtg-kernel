//! Independently pinned native tensorization for typed Flat V5 / ObservationV8.
//! Producing these tensors does not admit a predecessor checkpoint.

use crate::flat_policy_v5::FlatScoringDecisionViewV5;
use crate::native_flat_tensorizer_v2::{
    fill_native_flat_decision_tensors_v5, NativeFlatDecisionTensorV2, NativeFlatTensorErrorV2,
};
#[cfg(test)]
use crate::native_policy_value_net_v1::{
    NativeEncodedDecisionSchemaV1, NativeEncodedDecisionViewV1,
};

include!("../../data/flat_policy_v5/feature_identity.rs");

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct NativeFlatDecisionTensorV5 {
    pub(crate) common: NativeFlatDecisionTensorV2,
}

#[derive(Default)]
pub(crate) struct NativeFlatTensorizerV5 {
    poisoned: bool,
}

impl NativeFlatTensorizerV5 {
    pub(crate) fn fill(
        &mut self,
        decision: FlatScoringDecisionViewV5<'_>,
        output: &mut NativeFlatDecisionTensorV5,
    ) -> Result<(), NativeFlatTensorErrorV2> {
        if self.poisoned {
            return Err(NativeFlatTensorErrorV2::Poisoned);
        }
        match fill_native_flat_decision_tensors_v5(decision) {
            Ok(common) => {
                *output = NativeFlatDecisionTensorV5 { common };
                Ok(())
            }
            Err(error) => {
                self.poisoned = true;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn schema_v5() -> NativeEncodedDecisionSchemaV1 {
    NativeEncodedDecisionSchemaV1 {
        version: FEATURE_SCHEMA_VERSION_V5,
        registry_version: FEATURE_REGISTRY_VERSION_V5,
        contract_digest: FEATURE_CONTRACT_DIGEST_V5,
        encoding_digest: FEATURE_ENCODING_DIGEST_V5,
        ..NativeEncodedDecisionSchemaV1::contract_v1()
    }
}

#[cfg(test)]
pub(crate) fn encoded_decision_view_v5(
    tensor: &NativeFlatDecisionTensorV5,
) -> NativeEncodedDecisionViewV1<'_> {
    let t = &tensor.common;
    NativeEncodedDecisionViewV1::from_slices_unvalidated(
        schema_v5(),
        &t.state,
        &t.object_features,
        &t.object_card_ids,
        &t.object_groups,
        &t.object_node_ids,
        &t.edge_features,
        &t.edge_source_indices,
        &t.edge_target_indices,
        &t.action_features,
        &t.action_ref_features,
        &t.action_ref_card_ids,
        &t.action_ref_action_indices,
        &t.action_ref_node_indices,
    )
}

use crate::flat_policy_v2::{FlatScoringDecisionViewV2, FlatScoringOwnedBuffersV2};
use crate::flat_policy_v5::{FlatDecisionEncoderV5, FlatDecisionV5};
use crate::rl_session::{FastActorDecisionV1, FastActorSessionV1, FlatActionDecisionBindingV5};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeFeatureIdentityV5 {
    pub schema_version: &'static str,
    pub registry_version: &'static str,
    pub contract_digest: &'static str,
    pub encoding_digest: &'static str,
    pub source_sha256: &'static str,
    pub descriptor_sha256: &'static str,
    pub model_contract_version: &'static str,
}

pub fn feature_identity_v5() -> NativeFeatureIdentityV5 {
    NativeFeatureIdentityV5 {
        schema_version: FEATURE_SCHEMA_VERSION_V5,
        registry_version: FEATURE_REGISTRY_VERSION_V5,
        contract_digest: FEATURE_CONTRACT_DIGEST_V5,
        encoding_digest: FEATURE_ENCODING_DIGEST_V5,
        source_sha256: FEATURES_SOURCE_SHA256_V5,
        descriptor_sha256: FEATURE_DESCRIPTOR_SHA256_V5,
        model_contract_version: MODEL_CONTRACT_VERSION_V5,
    }
}

#[derive(Debug)]
pub struct NativeFlatPolicyErrorV5(String);

impl std::fmt::Display for NativeFlatPolicyErrorV5 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for NativeFlatPolicyErrorV5 {}

/// Sealed output of the V5 producer. No public constructor, arena ids,
/// executable candidates, text, or predecessor identity is exposed.
#[derive(Clone, Debug, PartialEq)]
pub struct NativeFlatPolicyDecisionV5 {
    binding: FlatActionDecisionBindingV5,
    tensor: NativeFlatDecisionTensorV5,
}

impl NativeFlatPolicyDecisionV5 {
    pub fn binding(&self) -> FlatActionDecisionBindingV5 {
        self.binding
    }
    pub fn feature_identity(&self) -> NativeFeatureIdentityV5 {
        feature_identity_v5()
    }
    pub fn state(&self) -> &[f32] {
        &self.tensor.common.state
    }
    pub fn object_features(&self) -> &[f32] {
        &self.tensor.common.object_features
    }
    pub fn edge_features(&self) -> &[f32] {
        &self.tensor.common.edge_features
    }
    pub fn action_features(&self) -> &[f32] {
        &self.tensor.common.action_features
    }
    pub fn action_ref_features(&self) -> &[f32] {
        &self.tensor.common.action_ref_features
    }
    pub fn object_card_ids(&self) -> &[i64] {
        &self.tensor.common.object_card_ids
    }
    pub fn object_groups(&self) -> &[i64] {
        &self.tensor.common.object_groups
    }
    pub fn object_node_ids(&self) -> &[i64] {
        &self.tensor.common.object_node_ids
    }
    pub fn edge_source_indices(&self) -> &[i64] {
        &self.tensor.common.edge_source_indices
    }
    pub fn edge_target_indices(&self) -> &[i64] {
        &self.tensor.common.edge_target_indices
    }
    pub fn action_ref_card_ids(&self) -> &[i64] {
        &self.tensor.common.action_ref_card_ids
    }
    pub fn action_ref_action_indices(&self) -> &[i64] {
        &self.tensor.common.action_ref_action_indices
    }
    pub fn action_ref_node_indices(&self) -> &[i64] {
        &self.tensor.common.action_ref_node_indices
    }
}

#[derive(Default)]
pub struct NativeFlatPolicyEncoderV5 {
    encoder: FlatDecisionEncoderV5,
    buffers: OwnedScoringV5,
    tensorizer: NativeFlatTensorizerV5,
}

impl NativeFlatPolicyEncoderV5 {
    /// Expected decision and the explicit V5 session mode are validated
    /// before any packet is published. A tensorizer error poisons this
    /// encoder; action-contract errors remain read-only and retryable.
    pub fn encode(
        &mut self,
        session: &FastActorSessionV1,
        expected: FastActorDecisionV1,
    ) -> Result<NativeFlatPolicyDecisionV5, NativeFlatPolicyErrorV5> {
        let decision = session
            .encode_current_flat_scoring_decision_owned_v5(
                expected,
                &mut self.encoder,
                &mut FlatScoringOwnedBuffersV2 {
                    objects: &mut self.buffers.objects,
                    relations: &mut self.buffers.relations,
                    object_subtypes: &mut self.buffers.object_subtypes,
                    ability_uses: &mut self.buffers.ability_uses,
                    goads: &mut self.buffers.goads,
                    completed_dungeons: &mut self.buffers.completed_dungeons,
                    effect_subtype_changes: &mut self.buffers.effect_subtype_changes,
                    context_path_elements: &mut self.buffers.context_path_elements,
                    actions: &mut self.buffers.actions,
                    action_refs: &mut self.buffers.action_refs,
                },
            )
            .map_err(|error| NativeFlatPolicyErrorV5(format!("flat_v5:{error:?}")))?;
        let mut tensor = NativeFlatDecisionTensorV5::default();
        self.tensorizer
            .fill(self.buffers.view(&decision), &mut tensor)
            .map_err(|error| NativeFlatPolicyErrorV5(format!("native_v5:{error:?}")))?;
        Ok(NativeFlatPolicyDecisionV5 {
            binding: decision.binding,
            tensor,
        })
    }
}

#[derive(Default)]
struct OwnedScoringV5 {
    objects: Vec<crate::flat_policy_v2::FlatObjectCoreV2>,
    relations: Vec<crate::flat_policy_v2::FlatRelationV2>,
    object_subtypes: Vec<crate::flat_policy_v2::FlatObjectSubtypeV2>,
    ability_uses: Vec<crate::flat_policy_v2::FlatObjectAbilityUseV2>,
    goads: Vec<crate::flat_policy_v2::FlatObjectGoadV2>,
    completed_dungeons: Vec<crate::flat_policy_v2::FlatCompletedDungeonV2>,
    effect_subtype_changes: Vec<crate::flat_policy_v2::FlatEffectSubtypeChangeV2>,
    context_path_elements: Vec<crate::flat_policy_v2::FlatContextPathElementV2>,
    actions: Vec<crate::flat_policy_v2::FlatScorerActionCoreV2>,
    action_refs: Vec<crate::flat_policy_v2::FlatScorerActionRefV2>,
}

impl OwnedScoringV5 {
    fn view<'a>(&'a self, decision: &'a FlatDecisionV5) -> FlatScoringDecisionViewV5<'a> {
        FlatScoringDecisionViewV5::new(
            FlatScoringDecisionViewV2::new(
                &decision.globals,
                &self.objects,
                &self.relations,
                &self.object_subtypes,
                &self.ability_uses,
                &self.goads,
                &self.completed_dungeons,
                &self.effect_subtype_changes,
                &self.context_path_elements,
                &self.actions,
                &self.action_refs,
            ),
            &decision.extensions,
        )
    }
}

#[cfg(test)]
mod tests;
