//! Ordered, per-instance public stack rows alongside unchanged V4 tensors.
#[cfg(any(test, feature = "experimental-burn-net8-packed-cuda-v1"))]
use crate::flat_policy_v2::{
    FlatRelationPayloadV2, FlatRelationRoleV2, FlatRelativePlayerV2, FlatTargetKindV2,
};
#[cfg(any(test, feature = "experimental-burn-net8-packed-cuda-v1"))]
use crate::flat_policy_v4::FlatScoringDecisionViewV4;
#[cfg(any(test, feature = "experimental-burn-net8-packed-cuda-v1"))]
use crate::native_flat_tensorizer_v2::{
    fill_native_flat_decision_tensors_v4, stack_node_map_v4, NativeFlatTensorErrorV2,
};
use crate::native_flat_tensorizer_v4::{encoded_decision_view_v4, NativeFlatDecisionTensorV4};
use crate::native_policy_value_net_v1::NativeEncodedDecisionViewV1;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) const SCHEMA: &str = "mtg-kernel-public-stack-input/v1";
pub(crate) const FEATURE_WIDTH: usize = 312;
pub(crate) const CONTRACT: &[u8] =
    include_bytes!("../../data/public_stack_features_v1/contract.json");
pub(crate) const PERMUTATION_CONTRACT: &[u8] =
    include_bytes!("../../data/public_stack_features_v1/permutation.json");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg(any(test, feature = "experimental-burn-net8-packed-cuda-v1"))]
pub enum StackInputModeV1 {
    Disabled,
    Structured,
    Permuted,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StackColumnPermutationV1 {
    pub(crate) schema: String,
    pub(crate) contract_sha256: String,
    pub(crate) columns: Vec<u16>,
}
impl StackColumnPermutationV1 {
    #[cfg(any(test, feature = "experimental-burn-net8-packed-cuda-v1"))]
    pub(crate) fn sample(rng: &mut crate::state::SplitMix64) -> Self {
        let mut columns: Vec<_> = (0..FEATURE_WIDTH as u16).collect();
        for i in (1..FEATURE_WIDTH).rev() {
            let bound = i as u64 + 1;
            let reject = bound.wrapping_neg() % bound;
            let value = loop {
                let v = rng.next_u64();
                if v >= reject {
                    break v;
                }
            };
            columns.swap(i, (value % bound) as usize);
        }
        Self {
            schema: "public-stack-column-permutation/v1".into(),
            contract_sha256: format!("{:x}", Sha256::digest(PERMUTATION_CONTRACT)),
            columns,
        }
    }
    fn validate(&self) -> Result<(), String> {
        if self.schema != "public-stack-column-permutation/v1"
            || self.contract_sha256 != format!("{:x}", Sha256::digest(PERMUTATION_CONTRACT))
            || self.columns.len() != FEATURE_WIDTH
        {
            return Err("stack permutation contract differs".into());
        }
        let mut seen = [false; FEATURE_WIDTH];
        for &column in &self.columns {
            if column as usize >= FEATURE_WIDTH
                || std::mem::replace(&mut seen[column as usize], true)
            {
                return Err("stack columns are not a permutation".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StackMessageRowV1 {
    pub(crate) source_node: usize,
    pub(crate) target_node: Option<usize>,
    pub(crate) features: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StackFeatureRowsV1 {
    pub(crate) schema: String,
    pub(crate) contract_sha256: String,
    pub(crate) object_count: usize,
    pub(crate) stack_items: usize,
    pub(crate) rows: Vec<StackMessageRowV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) permutation: Option<StackColumnPermutationV1>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StackEncodedDecisionV1 {
    pub(crate) legacy: NativeFlatDecisionTensorV4,
    pub(crate) stack: StackFeatureRowsV1,
}

impl StackEncodedDecisionV1 {
    pub(crate) fn view(&self) -> NativeEncodedDecisionViewV1<'_> {
        encoded_decision_view_v4(&self.legacy)
    }
}

impl StackFeatureRowsV1 {
    pub(crate) fn append_model_features(&self, row: &StackMessageRowV1, output: &mut Vec<f32>) {
        match &self.permutation {
            None => output.extend_from_slice(&row.features),
            Some(p) => output.extend(p.columns.iter().map(|&c| row.features[c as usize])),
        }
    }
    pub(crate) fn validate(&self, object_count: usize) -> Result<(), String> {
        if let Some(p) = &self.permutation {
            p.validate()?;
        }
        if self.schema != SCHEMA
            || self.contract_sha256 != format!("{:x}", Sha256::digest(CONTRACT))
            || self.object_count != object_count
            || self.stack_items > self.rows.len()
        {
            return Err("stack feature identity/cardinality differs".into());
        }
        let mut item = 0;
        let mut secondary = 0;
        let mut baseline: Option<&StackMessageRowV1> = None;
        for row in &self.rows {
            if row.source_node >= object_count
                || row.target_node.is_some_and(|n| n >= object_count)
                || row.features.len() != FEATURE_WIDTH
                || row.features.iter().any(|v| !v.is_finite())
            {
                return Err("invalid stack row bounds".into());
            }
            let f = &row.features;
            if f[1] == 1.0 {
                item += 1;
                secondary = 0;
                baseline = Some(row);
            } else if f[1] == 0.0 && item > 0 {
                secondary += 1;
                let base = baseline.expect("a preceding baseline exists");
                if row.source_node != base.source_node || f[6..305] != base.features[6..305] {
                    return Err("target row differs from stack item payload".into());
                }
            } else {
                return Err("invalid stack baseline order".into());
            }
            if item > self.stack_items
                || f[0] != 1.0
                || f[2] != (item - 1) as f32 / 32.0
                || f[3] != (self.stack_items - item) as f32 / 32.0
                || f[4] != self.stack_items as f32 / 32.0
                || f[5] != secondary as f32 / 16.0
            {
                return Err("stack item/target order differs".into());
            }
            for range in [6..8, 8..12, 16..25, 25..281, 305..309, 309..312] {
                if f[range.clone()].iter().any(|v| *v != 0.0 && *v != 1.0)
                    || f[range].iter().sum::<f32>() != 1.0
                {
                    return Err("invalid categorical stack feature".into());
                }
            }
            if f[12..16]
                .iter()
                .chain(&f[281..305])
                .any(|v| *v != 0.0 && *v != 1.0)
                || (row.target_node.is_some() != (f[306] == 1.0))
                || ((secondary == 0) != (f[305] == 1.0))
                || (f[306] != 1.0 && f[311] != 1.0)
            {
                return Err("invalid stack flags or target binding".into());
            }
        }
        if item != self.stack_items {
            return Err("missing stack baseline".into());
        }
        Ok(())
    }
}

/// Both outputs are derived transactionally from the same bound actor view.
#[cfg(any(test, feature = "experimental-burn-net8-packed-cuda-v1"))]
pub(crate) fn encode_stack_decision_v1(
    view: FlatScoringDecisionViewV4<'_>,
) -> Result<StackEncodedDecisionV1, NativeFlatTensorErrorV2> {
    // Existing validation includes stack payload consistency, ordering, public
    // source authority and target identity. It remains the sole legacy encoder.
    let legacy = NativeFlatDecisionTensorV4 {
        common: fill_native_flat_decision_tensors_v4(view)?,
    };
    let node_map = stack_node_map_v4(view)?;
    let relations: Vec<_> = view
        .common()
        .relations()
        .iter()
        .filter(|r| r.role == FlatRelationRoleV2::StackTarget)
        .collect();
    let stack_items = relations.iter().filter(|r| r.secondary_order == 0).count();
    let node = |raw: Option<u32>| {
        raw.and_then(|r| node_map.get(r as usize))
            .copied()
            .flatten()
            .ok_or(NativeFlatTensorErrorV2::RelationShape)
    };
    let mut rows = Vec::with_capacity(relations.len());
    for relation in relations {
        let FlatRelationPayloadV2::Stack(p) = relation.payload else {
            return Err(NativeFlatTensorErrorV2::RelationShape);
        };
        if p.stack_item_kind > 3
            || p.cast_method > 8
            || p.controller == FlatRelativePlayerV2::None
            || relation.primary_order as usize >= stack_items
        {
            return Err(NativeFlatTensorErrorV2::RelationShape);
        }
        let mut f = vec![0.0; FEATURE_WIDTH];
        f[0] = 1.0;
        f[1] = f32::from(relation.secondary_order == 0);
        f[2] = relation.primary_order as f32 / 32.0;
        f[3] = (stack_items - 1 - relation.primary_order as usize) as f32 / 32.0;
        f[4] = stack_items as f32 / 32.0;
        f[5] = relation.secondary_order as f32 / 16.0;
        f[6 + p.controller as usize] = 1.0;
        f[8 + p.stack_item_kind as usize] = 1.0;
        for (index, value) in [p.is_copy, p.is_flashback, p.madness_offer, p.kicked]
            .into_iter()
            .enumerate()
        {
            f[12 + index] = f32::from(value);
        }
        f[16 + p.cast_method as usize] = 1.0;
        f[25 + p.mode_chosen as usize] = 1.0;
        for bit in 0..8 {
            f[281 + bit] = f32::from(p.face_index & (1 << bit) != 0);
        }
        for bit in 0..16 {
            f[289 + bit] = f32::from(p.x_value & (1 << bit) != 0);
        }
        let (target_kind, target_node) = match p.target_kind {
            FlatTargetKindV2::None => (0, None),
            FlatTargetKindV2::Object => (1, Some(node(relation.target_object)?)),
            FlatTargetKindV2::Player => match p.target_player {
                FlatRelativePlayerV2::SelfPlayer => (2, None),
                FlatRelativePlayerV2::Opponent => (3, None),
                FlatRelativePlayerV2::None => return Err(NativeFlatTensorErrorV2::RelationShape),
            },
        };
        f[305 + target_kind] = 1.0;
        f[309 + p.target_object_controller as usize] = 1.0;
        rows.push(StackMessageRowV1 {
            source_node: node(relation.source_object)?,
            target_node,
            features: f,
        });
    }
    let stack = StackFeatureRowsV1 {
        schema: SCHEMA.into(),
        contract_sha256: format!("{:x}", Sha256::digest(CONTRACT)),
        object_count: legacy.common.object_card_ids.len(),
        stack_items,
        rows,
        permutation: None,
    };
    stack
        .validate(legacy.common.object_card_ids.len())
        .map_err(|_| NativeFlatTensorErrorV2::OutputInvariant)?;
    Ok(StackEncodedDecisionV1 { legacy, stack })
}

#[cfg(test)]
mod permutation_tests {
    use super::*;
    #[test]
    fn public_stack_permutation_is_complete_replayable_and_rejects_corruption() {
        let mut a = crate::state::SplitMix64::seed(812);
        let mut b = a;
        let first = StackColumnPermutationV1::sample(&mut a);
        assert_eq!(first, StackColumnPermutationV1::sample(&mut b));
        first.validate().unwrap();
        assert_ne!(first, StackColumnPermutationV1::sample(&mut a));
        let json = serde_json::to_vec(&first).unwrap();
        let decoded: StackColumnPermutationV1 = serde_json::from_slice(&json).unwrap();
        assert_eq!(first, decoded);
        let mut bad = first.clone();
        bad.columns[0] = bad.columns[1];
        assert!(bad.validate().is_err());
        let mut bad = first.clone();
        bad.columns[0] = FEATURE_WIDTH as u16;
        assert!(bad.validate().is_err());
        let mut bad = first;
        bad.contract_sha256.clear();
        assert!(bad.validate().is_err());
    }
}
