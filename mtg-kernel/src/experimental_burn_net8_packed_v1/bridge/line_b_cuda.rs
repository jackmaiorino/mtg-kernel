//! G115 line (b) auxiliary term inside the CUDA GAE update (unit 8d-2),
//! host side of the device root term: validation before the device call,
//! then per root the transported-logit tolerance gate on the device root-row
//! logits, the envelope receipt (line (b) change 2), the divergence before
//! the update, the gauge rows and the head-norm telemetry.
use super::super::training::line_b_root::LineBRootBackwardOutputsV1;
use super::*;
use crate::line_b_teacher_target_v1::{
    line_b_divergence_v1, line_b_softmax_v1, LineBDivergenceV1, LineBSoftmaxV1,
    LINE_B_CUDA_ENVELOPE_V1,
};
use crate::native_policy_train_step_v1::{LineBAuxiliaryInputV1, LineBAuxiliaryResultV1};

/// One selected root's device-logit envelope: the largest
/// `|log p_device(a) - log p_collection(a)|` over its menu and the bound the
/// tolerance gate implies, `2e-3 + 3e-3 max|z|` (a log-probability change is
/// at most the range of the logit changes, which the gate bounds). The host
/// refold line is the separate cheap check (FABLE-REVIEW-20260927 change 3):
/// the update's host refold of the sampled action's log-probability (binary32
/// `selected_log_softmax` of the transported collection logits, which never
/// sees the device forward) against the collection sampler's binary64 value.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub(crate) struct LineBCudaEnvelopeRootV1 {
    pub(crate) root: usize,
    pub(crate) max_abs_log_probability_discrepancy: f64,
    pub(crate) bound: f64,
    pub(crate) host_refold_abs_discrepancy: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LineBCudaAuxiliaryV1 {
    pub(crate) auxiliary: LineBAuxiliaryResultV1,
    pub(crate) envelope: Vec<LineBCudaEnvelopeRootV1>,
}

/// An available root: its index among the selected roots, its flat substep,
/// its frozen target, its transported collection logits and the action the
/// collection sampled there.
pub(super) struct LineBCudaRootV1 {
    index: usize,
    pub(super) flat_substep: usize,
    pub(super) target: LineBSoftmaxV1,
    collection_bits: Vec<u32>,
    selected_action_index: usize,
}

pub(super) struct LineBCudaPlanV1 {
    pub(super) roots: Vec<LineBCudaRootV1>,
    pub(super) count: usize,
    pub(super) scale: f64,
    pub(super) direction: LineBDivergenceV1,
}

impl LineBCudaPlanV1 {
    /// At least one available root and `c > 0`; otherwise the update is the
    /// ordinary one (the device term is not run).
    pub(super) fn active_v1(&self) -> bool {
        self.scale > 0.0 && !self.roots.is_empty()
    }
}

pub(super) struct LineBCudaTermV1 {
    pub(super) raw_gauge_residual: f32,
    pub(super) rows: Vec<(Vec<f32>, Vec<f64>)>,
    divergences_before: Vec<Option<f64>>,
    auxiliary_loss: f64,
    auxiliary_head_l2: f64,
    envelope: Vec<LineBCudaEnvelopeRootV1>,
}

fn invalid(code: &'static str) -> NativePolicyTrainErrorV1 {
    NativePolicyTrainErrorV1::LineBAuxiliary { code }
}

/// Review change 9: a root whose device log-probabilities leave the declared
/// envelope fails the update (a backend defect); a NaN discrepancy fails too.
fn line_b_cuda_envelope_check_v1(discrepancy: f64) -> Result<(), NativePolicyTrainErrorV1> {
    if discrepancy <= LINE_B_CUDA_ENVELOPE_V1 {
        Ok(())
    } else {
        Err(invalid("line-b-cuda-envelope-breach"))
    }
}

pub(super) fn line_b_cuda_plan_v1(
    input: &LineBAuxiliaryInputV1,
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    group_first_substeps: &[usize],
) -> Result<LineBCudaPlanV1, NativePolicyTrainErrorV1> {
    let count = input.roots.len();
    if count == 0 {
        return Err(invalid("line-b-auxiliary-no-roots"));
    }
    if !(input.coefficient.is_finite() && input.coefficient >= 0.0) {
        return Err(invalid("line-b-auxiliary-coefficient"));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut roots = Vec::new();
    for (index, root) in input.roots.iter().enumerate() {
        if !seen.insert((root.group_index, root.substep_index)) {
            return Err(invalid("line-b-auxiliary-duplicate-root"));
        }
        let substep = groups
            .get(root.group_index)
            .and_then(|group| group.substeps.get(root.substep_index))
            .ok_or(invalid("line-b-auxiliary-root-outside-batch"))?;
        if let Some(target) = &root.target {
            if target.log_probabilities.len() != substep.expected_raw_action_logit_bits.len() {
                return Err(invalid("line-b-auxiliary-divergence"));
            }
            roots.push(LineBCudaRootV1 {
                index,
                flat_substep: group_first_substeps[root.group_index] + root.substep_index,
                target: target.clone(),
                collection_bits: substep.expected_raw_action_logit_bits.to_vec(),
                selected_action_index: substep.selected_action_index,
            });
        }
    }
    Ok(LineBCudaPlanV1 {
        roots,
        count,
        scale: input.coefficient / count as f64,
        direction: input.direction,
    })
}

fn head_l2_v1<T: Copy + Into<f64>>(heads: &[Vec<T>]) -> f64 {
    let mut sum = 0.0_f64;
    for head in heads {
        for &value in head {
            let value: f64 = value.into();
            sum += value * value;
        }
    }
    sum.sqrt()
}

pub(super) fn ordinary_head_l2_v1(heads: &[Vec<f64>]) -> f64 {
    head_l2_v1(heads)
}

/// Adds one chunk's head gradients (binary32) into the running binary64
/// totals; a tensor without a gradient in the chunk adds nothing.
pub(super) fn add_head_gradients_v1(
    total: &mut [Vec<f64>],
    chunk: &[Vec<f32>],
) -> Result<(), NativePolicyTrainErrorV1> {
    if total.len() != chunk.len() {
        return Err(invalid("line-b-cuda-head-cardinality"));
    }
    for (destination, source) in total.iter_mut().zip(chunk) {
        if source.is_empty() {
            continue;
        }
        if destination.is_empty() {
            *destination = vec![0.0; source.len()];
        }
        if destination.len() != source.len() {
            return Err(invalid("line-b-cuda-head-shape"));
        }
        for (value, gradient) in destination.iter_mut().zip(source) {
            *value += f64::from(*gradient);
        }
    }
    Ok(())
}

/// Per available root: the tolerance gate on the device root-row logits
/// against the transported collection logits, the envelope, the divergence
/// before the update and the gauge row (the term's logit gradients from the
/// device logits, scaled by `c / N`).
pub(super) fn line_b_cuda_term_v1(
    plan: &LineBCudaPlanV1,
    outputs: &LineBRootBackwardOutputsV1,
    action_offsets: &[usize],
) -> Result<LineBCudaTermV1, NativePolicyTrainErrorV1> {
    if action_offsets.len() != plan.roots.len() + 1
        || action_offsets.last().copied() != Some(outputs.logit_outputs.len())
    {
        return Err(invalid("line-b-cuda-root-logit-cardinality"));
    }
    let mut divergences_before = vec![None; plan.count];
    let mut rows = Vec::with_capacity(plan.roots.len());
    let mut envelope = Vec::with_capacity(plan.roots.len());
    let mut auxiliary_loss = 0.0_f64;
    for (position, root) in plan.roots.iter().enumerate() {
        let row = &outputs.logit_outputs[action_offsets[position]..action_offsets[position + 1]];
        if row.len() != root.collection_bits.len() {
            return Err(invalid("line-b-cuda-root-width"));
        }
        validate_transported_logit_row_v2(row, &root.collection_bits)
            .map_err(|code| NativePolicyTrainErrorV1::CudaBackend { code })?;
        let device_logits: Vec<f64> = row.iter().map(|&z| f64::from(z)).collect();
        let collection_logits: Vec<f64> = root
            .collection_bits
            .iter()
            .map(|bits| f64::from(f32::from_bits(*bits)))
            .collect();
        let device =
            line_b_softmax_v1(&device_logits).map_err(|_| invalid("line-b-cuda-device-softmax"))?;
        let collection = line_b_softmax_v1(&collection_logits)
            .map_err(|_| invalid("line-b-cuda-collection-softmax"))?;
        let mut discrepancy = 0.0_f64;
        for (a, b) in device
            .log_probabilities
            .iter()
            .zip(&collection.log_probabilities)
        {
            discrepancy = discrepancy.max((a - b).abs());
        }
        line_b_cuda_envelope_check_v1(discrepancy)?;
        let magnitude = collection_logits
            .iter()
            .fold(0.0_f64, |m, z| m.max(z.abs()));
        let transported: Vec<f32> = root
            .collection_bits
            .iter()
            .map(|bits| f32::from_bits(*bits))
            .collect();
        let selected = root.selected_action_index;
        let (refolded, _) = selected_log_softmax(&transported, selected)?;
        let sampler = collection
            .log_probabilities
            .get(selected)
            .copied()
            .ok_or(invalid("line-b-cuda-root-selected-action"))?;
        envelope.push(LineBCudaEnvelopeRootV1 {
            root: root.index,
            max_abs_log_probability_discrepancy: discrepancy,
            bound: TRANSPORTED_LOGIT_RANGE_ABSOLUTE_TOLERANCE_V2
                + TRANSPORTED_LOGIT_RANGE_RELATIVE_TOLERANCE_V2 * magnitude,
            host_refold_abs_discrepancy: (f64::from(refolded) - sampler).abs(),
        });
        let value = line_b_divergence_v1(plan.direction, &device_logits, &root.target)
            .map_err(|_| invalid("line-b-auxiliary-divergence"))?;
        divergences_before[root.index] = Some(value.divergence);
        auxiliary_loss += plan.scale * value.divergence;
        let exact: Vec<f64> = value
            .logit_gradient
            .iter()
            .map(|g| plan.scale * g)
            .collect();
        let d_logits: Vec<f32> = exact.iter().map(|&g| g as f32).collect();
        rows.push((d_logits, exact));
    }
    Ok(LineBCudaTermV1 {
        raw_gauge_residual: outputs.raw_gauge_residual,
        rows,
        divergences_before,
        auxiliary_loss,
        auxiliary_head_l2: head_l2_v1(&outputs.head_gradients),
        envelope,
    })
}

/// The update's line (b) result: the device term's telemetry, or zeros and
/// unavailable divergences when the term did not run.
pub(super) fn line_b_cuda_result_v1(
    plan: &LineBCudaPlanV1,
    term: Option<LineBCudaTermV1>,
    ordinary_head_l2: f64,
) -> LineBCudaAuxiliaryV1 {
    match term {
        Some(term) => LineBCudaAuxiliaryV1 {
            auxiliary: LineBAuxiliaryResultV1 {
                divergences_before: term.divergences_before,
                auxiliary_loss: term.auxiliary_loss,
                auxiliary_head_l2: term.auxiliary_head_l2,
                ordinary_head_l2,
                auxiliary_bias_residual: term.raw_gauge_residual,
            },
            envelope: term.envelope,
        },
        None => LineBCudaAuxiliaryV1 {
            auxiliary: LineBAuxiliaryResultV1 {
                divergences_before: vec![None; plan.count],
                auxiliary_loss: 0.0,
                auxiliary_head_l2: 0.0,
                ordinary_head_l2,
                auxiliary_bias_residual: 0.0,
            },
            envelope: Vec::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_b_cuda_envelope_refuses_beyond_the_declared_constant() {
        assert_eq!(LINE_B_CUDA_ENVELOPE_V1, 1e-3);
        for inside in [0.0, 4.8e-6, LINE_B_CUDA_ENVELOPE_V1] {
            assert!(line_b_cuda_envelope_check_v1(inside).is_ok(), "{inside}");
        }
        let next_up = f64::from_bits(LINE_B_CUDA_ENVELOPE_V1.to_bits() + 1);
        for outside in [next_up, 2e-3, f64::INFINITY, f64::NAN] {
            assert_eq!(
                line_b_cuda_envelope_check_v1(outside),
                Err(NativePolicyTrainErrorV1::LineBAuxiliary {
                    code: "line-b-cuda-envelope-breach"
                }),
                "{outside}"
            );
        }
    }
}
