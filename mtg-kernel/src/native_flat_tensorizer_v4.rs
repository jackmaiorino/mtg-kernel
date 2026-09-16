//! Explicit inference feature transfer for the fresh-lineage (V4 contract /
//! V7 observation-schema) successor to rich V6 / flat V3 observations.
//! Additive sibling of `native_flat_tensorizer_v3.rs`; that module and
//! `data/flat_policy_v3/*` are never touched here.
//!
//! Shared numeric dimensions do not imply compatibility with the V3 (or any
//! other) feature contract. `FEATURE_CONTRACT_DIGEST_V4` and
//! `FEATURE_ENCODING_DIGEST_V4` are the only identities `fresh_successor`
//! (`sideboard_play_policy_v1.rs`) may ever be validated against; they must
//! never be compared against `FEATURE_CONTRACT_DIGEST_V3` /
//! `FEATURE_ENCODING_DIGEST_V3` or vice versa.

use crate::flat_policy_v4::FlatScoringDecisionViewV4;
use crate::native_flat_tensorizer_v2::{
    fill_native_flat_decision_tensors_v4, NativeFlatDecisionTensorV2, NativeFlatTensorErrorV2,
};
use crate::native_policy_value_net_v1::{
    NativeEncodedDecisionSchemaV1, NativeEncodedDecisionViewV1,
};

// These independently versioned identities are generated from features_v7.py.
// V3 and V5 pins (data/flat_policy_v3/feature_identity.rs, python/mtg_kernel_rl/features.py)
// are separate and are never regenerated again; see data/flat_policy_v4/README.md.
include!("../../data/flat_policy_v4/feature_identity.rs");

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct NativeFlatDecisionTensorV4 {
    pub(crate) common: NativeFlatDecisionTensorV2,
}

#[derive(Default)]
pub(crate) struct NativeFlatTensorizerV4 {
    poisoned: bool,
}

impl NativeFlatTensorizerV4 {
    pub(crate) fn fill(
        &mut self,
        decision: FlatScoringDecisionViewV4<'_>,
        output: &mut NativeFlatDecisionTensorV4,
    ) -> Result<(), NativeFlatTensorErrorV2> {
        if self.poisoned {
            return Err(NativeFlatTensorErrorV2::Poisoned);
        }
        match fill_native_flat_decision_tensors_v4(decision) {
            Ok(common) => {
                *output = NativeFlatDecisionTensorV4 { common };
                Ok(())
            }
            Err(error) => {
                self.poisoned = true;
                Err(error)
            }
        }
    }
}

pub(crate) fn schema_v4() -> NativeEncodedDecisionSchemaV1 {
    NativeEncodedDecisionSchemaV1 {
        version: FEATURE_SCHEMA_VERSION_V4,
        registry_version: FEATURE_REGISTRY_VERSION_V4,
        contract_digest: FEATURE_CONTRACT_DIGEST_V4,
        encoding_digest: FEATURE_ENCODING_DIGEST_V4,
        ..NativeEncodedDecisionSchemaV1::contract_v1()
    }
}

pub(crate) fn encoded_decision_view_v4(
    tensor: &NativeFlatDecisionTensorV4,
) -> NativeEncodedDecisionViewV1<'_> {
    let t = &tensor.common;
    NativeEncodedDecisionViewV1::from_slices_unvalidated(
        schema_v4(),
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
                Sha256::digest(include_bytes!("../../python/mtg_kernel_rl/features_v7.py"))
            ),
            FEATURES_SOURCE_SHA256_V4
        );
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../data/flat_policy_v4/feature_contract_v4.json"
                ))
            ),
            FEATURE_DESCRIPTOR_SHA256_V4
        );
        assert_ne!(
            FEATURE_CONTRACT_DIGEST_V4,
            crate::native_policy_value_net_v1::FEATURE_CONTRACT_DIGEST_V1
        );
        assert_ne!(
            FEATURE_ENCODING_DIGEST_V4,
            crate::native_policy_value_net_v1::FEATURE_ENCODING_DIGEST_V1
        );
        // Never the same identity as the V3 generation this is additive to.
        assert_ne!(
            FEATURE_CONTRACT_DIGEST_V4,
            crate::native_flat_tensorizer_v3::FEATURE_CONTRACT_DIGEST_V3
        );
        assert_ne!(
            FEATURE_ENCODING_DIGEST_V4,
            crate::native_flat_tensorizer_v3::FEATURE_ENCODING_DIGEST_V3
        );
    }

    #[test]
    fn frozen_feature_transfer_v4_requires_new_identity_and_preserves_shape_checks() {
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let mut tensor = NativeFlatDecisionTensorV4 {
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
        let view = encoded_decision_view_v4(&tensor);
        assert!(model.forward_v1(view).is_err());
        let output = model.forward_feature_transfer_v4(view).unwrap();
        assert_eq!(output.logits.len(), 1);
        assert!(output.value.is_finite());
        assert!(model
            .forward_feature_transfer_v4(
                crate::native_checkpoint_inference_v1::encoded_decision_view_v1(&tensor.common)
            )
            .is_err());
        // Bit-identical to a hand-built V3-shaped forward pass over the same
        // numeric input bits: the V4 forward differs from V3 only in which
        // compiled schema/contract digest labels the input, never in the
        // underlying arithmetic.
        let v3_tensor = crate::native_flat_tensorizer_v3::NativeFlatDecisionTensorV3 {
            common: tensor.common.clone(),
        };
        let v3_output = model
            .forward_feature_transfer_v3(crate::native_flat_tensorizer_v3::encoded_decision_view_v3(
                &v3_tensor,
            ))
            .unwrap();
        assert_eq!(output.logits, v3_output.logits);
        assert_eq!(output.value, v3_output.value);
        tensor.common.state.pop();
        assert!(model
            .forward_feature_transfer_v4(encoded_decision_view_v4(&tensor))
            .is_err());
    }
}
