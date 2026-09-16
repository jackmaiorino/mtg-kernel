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
