//! G115 line (b) head-only optimizer mask `line-b-head-only-mask-v1`
//! (CODEX-G115-EXIT-PROPOSAL-20260927.md v0.2, "common trainable mask";
//! CODEX #522 item 2; countersigned in FABLE-REVIEW-20260927.md).
//!
//! Restore-after-step: the unchanged optimizer step runs on every tensor, then
//! the 26 frozen tensors' parameters and both Adam moments are restored bit for
//! bit from the pre-step snapshot before any inference or publication. This
//! equals never stepping them because Adam is elementwise with per-tensor state
//! (`adam_update_impl`; the CUDA device mapper) and no backend clips a global
//! gradient norm: the heads read only pre-step parameters and the shared step
//! counter, which increments once either way. The seven head tensors keep their
//! inherited Adam state and receive the ordinary update. Zeroed gradients would
//! not suffice: momentum alone moves a parameter.
use super::*;
use std::fmt;

pub(crate) const HEAD_ONLY_MASK_VERSION_V1: &str = "line-b-head-only-mask-v1";

/// The only tensors this mask lets the optimizer change. `scorer.2.bias` is
/// the canonical gauge and stays frozen with its anchor and zero moments.
pub(crate) const HEAD_ONLY_TRAINABLE_TENSORS_V1: [&str; 7] = [
    "scorer.0.weight",
    "scorer.0.bias",
    "scorer.2.weight",
    "value_head.0.weight",
    "value_head.0.bias",
    "value_head.2.weight",
    "value_head.2.bias",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HeadOnlyMaskErrorV1 {
    /// Only the exact 33-tensor V1 manifest is admitted; there is no
    /// allow-all fallback for any other topology.
    TensorCount {
        found: usize,
    },
    UnknownTopology {
        index: usize,
        name: String,
    },
    MomentTopology {
        index: usize,
    },
    AdamAge {
        before: u64,
        after: u64,
    },
    GaugeAnchor,
}

impl fmt::Display for HeadOnlyMaskErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TensorCount { found } => write!(
                formatter,
                "{HEAD_ONLY_MASK_VERSION_V1}: expected {PARAMETER_TENSOR_COUNT} tensors, found {found}"
            ),
            Self::UnknownTopology { index, name } => write!(
                formatter,
                "{HEAD_ONLY_MASK_VERSION_V1}: tensor {index} ({name}) is not the declared V1 manifest"
            ),
            Self::MomentTopology { index } => write!(
                formatter,
                "{HEAD_ONLY_MASK_VERSION_V1}: moment layout differs at tensor {index}"
            ),
            Self::AdamAge { before, after } => write!(
                formatter,
                "{HEAD_ONLY_MASK_VERSION_V1}: Adam age must advance by exactly one ({before} to {after})"
            ),
            Self::GaugeAnchor => write!(
                formatter,
                "{HEAD_ONLY_MASK_VERSION_V1}: scorer bias gauge anchor changed"
            ),
        }
    }
}

impl std::error::Error for HeadOnlyMaskErrorV1 {}

/// Trainable flags per ordinal of `EXPECTED_PARAMETER_NAMES`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HeadOnlyMaskV1 {
    trainable: [bool; PARAMETER_TENSOR_COUNT],
}

impl HeadOnlyMaskV1 {
    /// Fails closed unless `snapshot` has exactly the 33 expected names, in
    /// order, with the expected shapes, and moments laid out alike.
    pub(crate) fn for_snapshot_v1(
        snapshot: &NativePolicyValueTrainSnapshotV1,
    ) -> Result<Self, HeadOnlyMaskErrorV1> {
        if snapshot.parameters.len() != PARAMETER_TENSOR_COUNT
            || snapshot.first_moments.len() != PARAMETER_TENSOR_COUNT
            || snapshot.second_moments.len() != PARAMETER_TENSOR_COUNT
        {
            return Err(HeadOnlyMaskErrorV1::TensorCount {
                found: snapshot.parameters.len(),
            });
        }
        let mut trainable = [false; PARAMETER_TENSOR_COUNT];
        for (index, parameter) in snapshot.parameters.iter().enumerate() {
            if parameter.name != EXPECTED_PARAMETER_NAMES[index]
                || parameter.shape.as_slice() != EXPECTED_PARAMETER_SHAPES[index]
                || parameter.values.len()
                    != EXPECTED_PARAMETER_SHAPES[index].iter().product::<usize>()
            {
                return Err(HeadOnlyMaskErrorV1::UnknownTopology {
                    index,
                    name: parameter.name.to_string(),
                });
            }
            for moment in [
                &snapshot.first_moments[index],
                &snapshot.second_moments[index],
            ] {
                if moment.name != parameter.name || moment.values.len() != parameter.values.len() {
                    return Err(HeadOnlyMaskErrorV1::MomentTopology { index });
                }
            }
            trainable[index] = HEAD_ONLY_TRAINABLE_TENSORS_V1.contains(&parameter.name);
        }
        Ok(Self { trainable })
    }

    pub(crate) fn is_trainable_v1(&self, index: usize) -> bool {
        self.trainable[index]
    }

    /// Restores every frozen tensor's parameter and both moments from
    /// `before`. Requires exactly one Adam age step and an unchanged gauge
    /// anchor; `after` must come from the same topology.
    pub(crate) fn restore_frozen_v1(
        &self,
        before: &NativePolicyValueTrainSnapshotV1,
        after: &mut NativePolicyValueTrainSnapshotV1,
    ) -> Result<(), HeadOnlyMaskErrorV1> {
        if before.adam_step.checked_add(1) != Some(after.adam_step) {
            return Err(HeadOnlyMaskErrorV1::AdamAge {
                before: before.adam_step,
                after: after.adam_step,
            });
        }
        if after.scorer_bias_anchor_bits != before.scorer_bias_anchor_bits {
            return Err(HeadOnlyMaskErrorV1::GaugeAnchor);
        }
        Self::for_snapshot_v1(after)?;
        for index in 0..PARAMETER_TENSOR_COUNT {
            if !self.trainable[index] {
                after.parameters[index] = before.parameters[index].clone();
                after.first_moments[index] = before.first_moments[index].clone();
                after.second_moments[index] = before.second_moments[index].clone();
            }
        }
        Ok(())
    }

    /// Domain-separated SHA-256 over every frozen tensor's name and parameter
    /// and moment bits: equal before and after every masked update, and equal
    /// to the run's initial source for the whole run.
    pub(crate) fn frozen_sha256_v1(&self, snapshot: &NativePolicyValueTrainSnapshotV1) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"mtg-kernel/line-b-head-only-mask-frozen/v1\0");
        for index in 0..PARAMETER_TENSOR_COUNT {
            if self.trainable[index] {
                continue;
            }
            for tensor in [
                &snapshot.parameters[index],
                &snapshot.first_moments[index],
                &snapshot.second_moments[index],
            ] {
                hash.update(tensor.name.as_bytes());
                hash.update([0]);
                for value in &tensor.values {
                    hash.update(value.to_bits().to_le_bytes());
                }
            }
        }
        hash.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> NativePolicyValueTrainSnapshotV1 {
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        NativePolicyValueTrainStateV1::new_v1(model)
            .unwrap()
            .snapshot_v1()
            .unwrap()
    }

    #[test]
    fn mask_names_exactly_the_seven_head_tensors() {
        let base = snapshot();
        let mask = HeadOnlyMaskV1::for_snapshot_v1(&base).unwrap();
        let trainable: Vec<&str> = (0..PARAMETER_TENSOR_COUNT)
            .filter(|&index| mask.is_trainable_v1(index))
            .map(|index| EXPECTED_PARAMETER_NAMES[index])
            .collect();
        assert_eq!(trainable, HEAD_ONLY_TRAINABLE_TENSORS_V1);
        assert!(!mask.is_trainable_v1(SCORER_SECOND_BIAS));
        let elements: usize = (0..PARAMETER_TENSOR_COUNT)
            .filter(|&index| mask.is_trainable_v1(index))
            .map(|index| base.parameters[index].values.len())
            .sum();
        assert_eq!(elements, 12_545);
    }

    #[test]
    fn unknown_names_order_shapes_and_counts_are_refused() {
        let base = snapshot();
        let mut renamed = base.clone();
        renamed.parameters[SCORER_FIRST_WEIGHT].name = "scorer.0.weights";
        assert!(matches!(
            HeadOnlyMaskV1::for_snapshot_v1(&renamed),
            Err(HeadOnlyMaskErrorV1::UnknownTopology {
                index: SCORER_FIRST_WEIGHT,
                ..
            })
        ));
        let mut reordered = base.clone();
        reordered
            .parameters
            .swap(VALUE_FIRST_WEIGHT, VALUE_SECOND_WEIGHT);
        assert!(HeadOnlyMaskV1::for_snapshot_v1(&reordered).is_err());
        let mut reshaped = base.clone();
        reshaped.parameters[VALUE_SECOND_BIAS].shape = vec![2];
        assert!(HeadOnlyMaskV1::for_snapshot_v1(&reshaped).is_err());
        let mut truncated = base.clone();
        truncated.parameters.pop();
        assert_eq!(
            HeadOnlyMaskV1::for_snapshot_v1(&truncated),
            Err(HeadOnlyMaskErrorV1::TensorCount {
                found: PARAMETER_TENSOR_COUNT - 1
            })
        );
        let mut moments = base;
        moments.second_moments[0].values.pop();
        assert_eq!(
            HeadOnlyMaskV1::for_snapshot_v1(&moments),
            Err(HeadOnlyMaskErrorV1::MomentTopology { index: 0 })
        );
    }

    #[test]
    fn restore_keeps_head_updates_and_rewinds_every_frozen_byte() {
        let before = snapshot();
        let mask = HeadOnlyMaskV1::for_snapshot_v1(&before).unwrap();
        let mut after = before.clone();
        after.adam_step += 1;
        for tensors in [
            &mut after.parameters,
            &mut after.first_moments,
            &mut after.second_moments,
        ] {
            for tensor in tensors.iter_mut() {
                for value in tensor.values.iter_mut() {
                    *value += 0.25;
                }
            }
        }
        let changed = after.clone();
        mask.restore_frozen_v1(&before, &mut after).unwrap();
        for index in 0..PARAMETER_TENSOR_COUNT {
            let expected = if mask.is_trainable_v1(index) {
                &changed
            } else {
                &before
            };
            assert_eq!(after.parameters[index], expected.parameters[index]);
            assert_eq!(after.first_moments[index], expected.first_moments[index]);
            assert_eq!(after.second_moments[index], expected.second_moments[index]);
        }
        assert_eq!(
            mask.frozen_sha256_v1(&after),
            mask.frozen_sha256_v1(&before)
        );
        assert_ne!(
            mask.frozen_sha256_v1(&changed),
            mask.frozen_sha256_v1(&before)
        );
    }

    #[test]
    fn restore_requires_one_age_step_and_the_same_anchor() {
        let before = snapshot();
        let mask = HeadOnlyMaskV1::for_snapshot_v1(&before).unwrap();
        let mut stale = before.clone();
        assert_eq!(
            mask.restore_frozen_v1(&before, &mut stale),
            Err(HeadOnlyMaskErrorV1::AdamAge {
                before: 0,
                after: 0
            })
        );
        let mut double = before.clone();
        double.adam_step += 2;
        assert!(mask.restore_frozen_v1(&before, &mut double).is_err());
        let mut anchor = before.clone();
        anchor.adam_step += 1;
        anchor.scorer_bias_anchor_bits ^= 1;
        assert_eq!(
            mask.restore_frozen_v1(&before, &mut anchor),
            Err(HeadOnlyMaskErrorV1::GaugeAnchor)
        );
    }
}
