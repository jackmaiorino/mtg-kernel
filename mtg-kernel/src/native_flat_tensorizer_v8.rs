//! FlatV8 (Python `features_v11.py`, `data/flat_policy_v8/`): the V8
//! inclusion contract on FlatV7 (collab LANES/w4-combat-20261030/DESIGN.md
//! v5.1, section 5). Additive sibling of `native_flat_tensorizer_v7.rs`;
//! that module, its identity and `data/flat_policy_v7/*` are never touched
//! here.
//!
//! Every FlatV8 tensor is the FlatV7 tensor of the same decision, except:
//! - each action row has one more column, the include bit at index 195
//!   (`include_bits_v8`), so the action width is 196;
//! - a pairing relation, `(blocker node, attacker node)` per block already
//!   chosen in the pending blocker declaration (`pending_block_pairs_v8`),
//!   empty at every other decision.
//!
//! The V8 identity is never compared against the V7 or V4 one or relabeled
//! as either. Its model is the V8 inclusion net
//! (`native_policy_value_net_v1::v8_inclusion_v1`).

use crate::flat_policy_v4::FlatScoringDecisionViewV4;
use crate::native_flat_tensorizer_v2::{
    include_bits_v8, pending_block_pairs_v8, NativeFlatDecisionTensorV2, NativeFlatTensorErrorV2,
    NATIVE_FLAT_ACTION_FEATURE_DIM_V2,
};
use crate::native_flat_tensorizer_v7::NativeFlatTensorizerV7;
use crate::native_policy_value_net_v1::v8_inclusion_v1::V8_ACTION_FEATURE_DIM_V1;
use crate::native_policy_value_net_v1::{
    NativeEncodedDecisionSchemaV1, NativeEncodedDecisionViewV1,
};

// Generated from features_v11.py by data/flat_policy_v8/generate_contract.py.
#[allow(dead_code)]
mod identity {
    include!("../../data/flat_policy_v8/feature_identity.rs");
}
pub(crate) use identity::*;

/// The V8 tensorizer: the V7 fill, then the include column and the pairing
/// relation. Poisoned by its first failure, as V7 is.
#[derive(Default)]
pub(crate) struct NativeFlatTensorizerV8 {
    poisoned: bool,
    v7: NativeFlatTensorizerV7,
    v7_tensor: NativeFlatDecisionTensorV2,
    pairs: Vec<i64>,
}

impl NativeFlatTensorizerV8 {
    /// See `NativeFlatTensorizerV2::set_state_digest_v1`.
    pub(crate) fn set_state_digest_v1(&mut self, on: bool) {
        self.v7.set_state_digest_v1(on);
    }

    /// Fills `common` (196-wide action rows) and `pairing` transactionally:
    /// on an error both keep their previous decision.
    pub(crate) fn fill(
        &mut self,
        decision: FlatScoringDecisionViewV4<'_>,
        common: &mut NativeFlatDecisionTensorV2,
        pairing: &mut Vec<i64>,
    ) -> Result<(), NativeFlatTensorErrorV2> {
        if self.poisoned {
            return Err(NativeFlatTensorErrorV2::Poisoned);
        }
        self.fill_inner_v1(decision, common, pairing)
            .inspect_err(|_| self.poisoned = true)
    }

    fn fill_inner_v1(
        &mut self,
        decision: FlatScoringDecisionViewV4<'_>,
        common: &mut NativeFlatDecisionTensorV2,
        pairing: &mut Vec<i64>,
    ) -> Result<(), NativeFlatTensorErrorV2> {
        self.v7.fill(decision, &mut self.v7_tensor)?;
        pending_block_pairs_v8(decision, &mut self.pairs)?;
        let rows = self.v7_tensor.action_features.len() / NATIVE_FLAT_ACTION_FEATURE_DIM_V2;
        let mut action_features = Vec::with_capacity(rows * V8_ACTION_FEATURE_DIM_V1);
        let mut bits = include_bits_v8(decision);
        for row in self
            .v7_tensor
            .action_features
            .chunks_exact(NATIVE_FLAT_ACTION_FEATURE_DIM_V2)
        {
            action_features.extend_from_slice(row);
            action_features.push(
                bits.next()
                    .ok_or(NativeFlatTensorErrorV2::OutputInvariant)?,
            );
        }
        if bits.next().is_some()
            || rows * NATIVE_FLAT_ACTION_FEATURE_DIM_V2 != self.v7_tensor.action_features.len()
        {
            return Err(NativeFlatTensorErrorV2::OutputInvariant);
        }
        common.clone_from(&self.v7_tensor);
        common.action_features = action_features;
        pairing.clone_from(&self.pairs);
        Ok(())
    }
}

pub(crate) fn schema_v8() -> NativeEncodedDecisionSchemaV1 {
    NativeEncodedDecisionSchemaV1 {
        version: FEATURE_SCHEMA_VERSION_V8,
        registry_version: FEATURE_REGISTRY_VERSION_V8,
        contract_digest: FEATURE_CONTRACT_DIGEST_V8,
        encoding_digest: FEATURE_ENCODING_DIGEST_V8,
        action_feature_dim: V8_ACTION_FEATURE_DIM_V1,
        ..NativeEncodedDecisionSchemaV1::contract_v1()
    }
}

/// A V8-labeled view of FlatV8 tensors (`common` with 196-wide action rows)
/// and their pairing relation.
pub(crate) fn encoded_decision_view_v8<'a>(
    t: &'a NativeFlatDecisionTensorV2,
    pairing_node_indices: &'a [i64],
) -> NativeEncodedDecisionViewV1<'a> {
    NativeEncodedDecisionViewV1::from_slices_unvalidated(
        schema_v8(),
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
    .with_pairing_node_indices_v1(pairing_node_indices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn flat_v8_source_and_descriptor_match_their_independent_pins() {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../python/mtg_kernel_rl/features_v11.py"))
            ),
            FEATURES_SOURCE_SHA256_V8
        );
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../data/flat_policy_v8/feature_contract_v8.json"
                ))
            ),
            FEATURE_DESCRIPTOR_SHA256_V8
        );
        // Never the identity of the V7 or V4 generation it extends.
        for (v8, other) in [
            (
                FEATURE_CONTRACT_DIGEST_V8,
                crate::native_flat_tensorizer_v7::FEATURE_CONTRACT_DIGEST_V7,
            ),
            (
                FEATURE_CONTRACT_DIGEST_V8,
                crate::native_flat_tensorizer_v4::FEATURE_CONTRACT_DIGEST_V4,
            ),
            (
                FEATURE_ENCODING_DIGEST_V8,
                crate::native_flat_tensorizer_v7::FEATURE_ENCODING_DIGEST_V7,
            ),
            (
                FEATURES_SOURCE_SHA256_V8,
                crate::native_flat_tensorizer_v7::FEATURES_SOURCE_SHA256_V7,
            ),
        ] {
            assert_ne!(v8, other);
        }
        let schema = schema_v8();
        let v7 = crate::native_flat_tensorizer_v7::schema_v7();
        assert_eq!(schema.action_feature_dim, v7.action_feature_dim + 1);
        assert_eq!(
            (
                schema.state_dim,
                schema.object_feature_dim,
                schema.edge_feature_dim,
                schema.action_ref_feature_dim,
                schema.object_group_count
            ),
            (
                v7.state_dim,
                v7.object_feature_dim,
                v7.edge_feature_dim,
                v7.action_ref_feature_dim,
                v7.object_group_count
            )
        );
    }
}
