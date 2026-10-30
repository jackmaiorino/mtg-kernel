//! FlatV7 (Python `features_v10.py`, `data/flat_policy_v7/`): the FlatV4
//! tensor contract plus London pregame decisions (design
//! `docs/design/pauper_london_mulligans_v1.md`, section 5). Additive sibling
//! of `native_flat_tensorizer_v4.rs`; that module, its identity and
//! `data/flat_policy_v4/*` are never touched here.
//!
//! Shape-equal to V4 (state 219, object 98, edge 41, action 195, action
//! reference 25, 20 object groups). In-game decisions give byte-identical
//! tensors to V4; only pregame decisions differ. The V7 identity is never
//! compared against the V4 one or relabeled as it.

use crate::flat_policy_v4::FlatScoringDecisionViewV4;
use crate::native_flat_tensorizer_v2::{
    fill_native_flat_decision_tensors_v7_with_scratch, NativeFlatDecisionTensorV2,
    NativeFlatTensorErrorV2, NativeFlatTensorScratchV3,
};
use crate::native_policy_value_net_v1::{
    NativeEncodedDecisionSchemaV1, NativeEncodedDecisionViewV1,
};

// Generated from features_v10.py by data/flat_policy_v7/generate_contract.py.
#[allow(dead_code)]
mod identity {
    include!("../../data/flat_policy_v7/feature_identity.rs");
}
pub(crate) use identity::*;

/// The V7 tensorizer: the V4 fill in-game, the London pregame fill at a
/// London decision. Poisoned by its first failure, as V4 is.
#[derive(Default)]
pub(crate) struct NativeFlatTensorizerV7 {
    poisoned: bool,
    scratch: NativeFlatTensorScratchV3,
}

impl NativeFlatTensorizerV7 {
    /// See `NativeFlatTensorizerV2::set_state_digest_v1`.
    pub(crate) fn set_state_digest_v1(&mut self, on: bool) {
        self.scratch.set_state_digest_v1(on);
    }

    pub(crate) fn fill(
        &mut self,
        decision: FlatScoringDecisionViewV4<'_>,
        output: &mut NativeFlatDecisionTensorV2,
    ) -> Result<(), NativeFlatTensorErrorV2> {
        if self.poisoned {
            return Err(NativeFlatTensorErrorV2::Poisoned);
        }
        fill_native_flat_decision_tensors_v7_with_scratch(decision, &mut self.scratch, output)
            .inspect_err(|_| self.poisoned = true)
    }
}

pub(crate) fn schema_v7() -> NativeEncodedDecisionSchemaV1 {
    NativeEncodedDecisionSchemaV1 {
        version: FEATURE_SCHEMA_VERSION_V7,
        registry_version: FEATURE_REGISTRY_VERSION_V7,
        contract_digest: FEATURE_CONTRACT_DIGEST_V7,
        encoding_digest: FEATURE_ENCODING_DIGEST_V7,
        ..NativeEncodedDecisionSchemaV1::contract_v1()
    }
}

/// A V7-labeled view of tensors the V7 tensorizer produced.
pub(crate) fn encoded_decision_view_v7(
    t: &NativeFlatDecisionTensorV2,
) -> NativeEncodedDecisionViewV1<'_> {
    NativeEncodedDecisionViewV1::from_slices_unvalidated(
        schema_v7(),
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
    use sha2::{Digest, Sha256};

    #[test]
    fn flat_v7_source_and_descriptor_match_their_independent_pins() {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../python/mtg_kernel_rl/features_v10.py"))
            ),
            FEATURES_SOURCE_SHA256_V7
        );
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../data/flat_policy_v7/feature_contract_v7.json"
                ))
            ),
            FEATURE_DESCRIPTOR_SHA256_V7
        );
        // Never the identity of the V4 generation it extends.
        assert_ne!(
            FEATURE_CONTRACT_DIGEST_V7,
            crate::native_flat_tensorizer_v4::FEATURE_CONTRACT_DIGEST_V4
        );
        assert_ne!(
            FEATURE_ENCODING_DIGEST_V7,
            crate::native_flat_tensorizer_v4::FEATURE_ENCODING_DIGEST_V4
        );
        assert_ne!(
            FEATURES_SOURCE_SHA256_V7,
            crate::native_flat_tensorizer_v4::FEATURES_SOURCE_SHA256_V4
        );
        let schema = schema_v7();
        let v4 = crate::native_flat_tensorizer_v4::schema_v4();
        assert_eq!(
            (
                schema.state_dim,
                schema.object_feature_dim,
                schema.edge_feature_dim,
                schema.action_feature_dim,
                schema.action_ref_feature_dim,
                schema.object_group_count
            ),
            (
                v4.state_dim,
                v4.object_feature_dim,
                v4.edge_feature_dim,
                v4.action_feature_dim,
                v4.action_ref_feature_dim,
                v4.object_group_count
            )
        );
    }
}
