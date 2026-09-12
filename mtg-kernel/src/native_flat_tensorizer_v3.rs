//! Explicit inference feature transfer for rich V6 / flat V3 observations.
//! Shared numeric dimensions do not imply compatibility with the old feature contract.

use crate::flat_policy_v3::FlatScoringDecisionViewV3;
use crate::native_flat_tensorizer_v2::{
    fill_native_flat_decision_tensors_v3, NativeFlatDecisionTensorV2, NativeFlatTensorErrorV2,
};
use crate::native_policy_value_net_v1::{
    NativeEncodedDecisionSchemaV1, NativeEncodedDecisionViewV1,
};

// These independently versioned identities are generated from features_v6.py.
include!("../../data/flat_policy_v3/feature_identity.rs");

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct NativeFlatDecisionTensorV3 {
    pub(crate) common: NativeFlatDecisionTensorV2,
}

#[derive(Default)]
pub(crate) struct NativeFlatTensorizerV3 {
    poisoned: bool,
}

impl NativeFlatTensorizerV3 {
    pub(crate) fn fill(
        &mut self,
        decision: FlatScoringDecisionViewV3<'_>,
        output: &mut NativeFlatDecisionTensorV3,
    ) -> Result<(), NativeFlatTensorErrorV2> {
        if self.poisoned {
            return Err(NativeFlatTensorErrorV2::Poisoned);
        }
        match fill_native_flat_decision_tensors_v3(decision) {
            Ok(common) => {
                *output = NativeFlatDecisionTensorV3 { common };
                Ok(())
            }
            Err(error) => {
                self.poisoned = true;
                Err(error)
            }
        }
    }
}

pub(crate) fn schema_v3() -> NativeEncodedDecisionSchemaV1 {
    NativeEncodedDecisionSchemaV1 {
        version: FEATURE_SCHEMA_VERSION_V3,
        registry_version: FEATURE_REGISTRY_VERSION_V3,
        contract_digest: FEATURE_CONTRACT_DIGEST_V3,
        encoding_digest: FEATURE_ENCODING_DIGEST_V3,
        ..NativeEncodedDecisionSchemaV1::contract_v1()
    }
}

pub(crate) fn encoded_decision_view_v3(
    tensor: &NativeFlatDecisionTensorV3,
) -> NativeEncodedDecisionViewV1<'_> {
    let t = &tensor.common;
    NativeEncodedDecisionViewV1::from_slices_unvalidated(
        schema_v3(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_policy_value_net_v1::{
        NativePolicyValueModelConfigV1, NativePolicyValueNetV1,
    };
    use sha2::{Digest, Sha256};

    #[test]
    fn successor_source_and_descriptor_match_their_independent_pins() {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../python/mtg_kernel_rl/features_v6.py"))
            ),
            FEATURES_SOURCE_SHA256_V3
        );
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../data/flat_policy_v3/feature_contract_v3.json"
                ))
            ),
            FEATURE_DESCRIPTOR_SHA256_V3
        );
        assert_ne!(
            FEATURE_CONTRACT_DIGEST_V3,
            crate::native_policy_value_net_v1::FEATURE_CONTRACT_DIGEST_V1
        );
        assert_ne!(
            FEATURE_ENCODING_DIGEST_V3,
            crate::native_policy_value_net_v1::FEATURE_ENCODING_DIGEST_V1
        );
    }

    #[test]
    fn frozen_feature_transfer_requires_new_identity_and_preserves_shape_checks() {
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let mut tensor = NativeFlatDecisionTensorV3 {
            common: NativeFlatDecisionTensorV2 {
                state: vec![0.0; 219],
                object_features: vec![0.0; 98],
                object_card_ids: vec![1],
                object_groups: vec![0],
                object_node_ids: vec![0],
                action_features: vec![0.0; 195],
                ..Default::default()
            },
        };
        let view = encoded_decision_view_v3(&tensor);
        assert!(model.forward_v1(view).is_err());
        let output = model.forward_feature_transfer_v3(view).unwrap();
        assert_eq!(output.logits.len(), 1);
        assert!(output.value.is_finite());
        assert!(model
            .forward_feature_transfer_v3(
                crate::native_checkpoint_inference_v1::encoded_decision_view_v1(&tensor.common)
            )
            .is_err());
        tensor.common.state.pop();
        assert!(model
            .forward_feature_transfer_v3(encoded_decision_view_v3(&tensor))
            .is_err());
    }
}
