//! G115 line (b) auxiliary term on the CPU reference (design entry section
//! 5; FABLE-REVIEW-20260927 exit-teacher changes 2 and 4; proposal 12:51
//! telemetry): `c` times the mean over the batch's selected roots of the
//! slot's divergence, entered as explicit logit gradients through the same
//! reverse pass and combined with the ordinary gradients before the single
//! Adam step. A censored root stays in the denominator and contributes zero.

use super::head_only_mask_v1::HEAD_ONLY_TRAINABLE_TENSORS_V1;
use super::*;
use crate::line_b_teacher_target_v1::{line_b_divergence_v1, LineBDivergenceV1, LineBSoftmaxV1};

/// One selected root of the batch: its row among the update's groups and its
/// frozen target, or `None` when the root is censored.
pub(crate) struct LineBAuxiliaryRootV1 {
    pub(crate) group_index: usize,
    pub(crate) substep_index: usize,
    pub(crate) target: Option<LineBSoftmaxV1>,
}

pub(crate) struct LineBAuxiliaryInputV1 {
    pub(crate) direction: LineBDivergenceV1,
    /// `c`; the mean runs over all selected roots, censored ones included.
    pub(crate) coefficient: f64,
    pub(crate) roots: Vec<LineBAuxiliaryRootV1>,
}

/// Outcome-free telemetry of one update (proposal 12:51).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LineBAuxiliaryResultV1 {
    /// Per root: the divergence at the start-of-update parameters, `None` for
    /// a censored root.
    pub(crate) divergences_before: Vec<Option<f64>>,
    /// `(c / N) * sum` of the available roots' divergences.
    pub(crate) auxiliary_loss: f64,
    /// L2 norms over the seven trainable head tensors before Adam.
    pub(crate) auxiliary_head_l2: f64,
    pub(crate) ordinary_head_l2: f64,
    /// The auxiliary term's own scorer-bias gradient (its gauge residual).
    pub(crate) auxiliary_bias_residual: f32,
}

pub(super) struct LineBAuxiliaryTermsV1 {
    /// At least one available root and `c > 0`; otherwise the update is the
    /// ordinary one bit for bit (adding zeros could flip a `-0.0` gradient).
    active: bool,
    gradients: Vec<Vec<f32>>,
    rows: Vec<(Vec<f32>, Vec<f64>)>,
    divergences: Vec<Option<f64>>,
    loss: f64,
}

fn invalid(code: &'static str) -> NativePolicyTrainErrorV1 {
    NativePolicyTrainErrorV1::LineBAuxiliary { code }
}

/// The auxiliary logit gradients of every available root and their reverse
/// pass into a separate gradient buffer. Runs on the update's own forward
/// tapes, before the ordinary backward consumes them.
pub(super) fn line_b_auxiliary_terms_v1(
    input: &LineBAuxiliaryInputV1,
    group_tapes: &[GroupTapeV1<'_>],
    parameters: &[NativeNamedParameterV1],
) -> Result<LineBAuxiliaryTermsV1, NativePolicyTrainErrorV1> {
    let count = input.roots.len();
    if count == 0 {
        return Err(invalid("line-b-auxiliary-no-roots"));
    }
    if !(input.coefficient.is_finite() && input.coefficient >= 0.0) {
        return Err(invalid("line-b-auxiliary-coefficient"));
    }
    let scale = input.coefficient / count as f64;
    let mut seen = std::collections::BTreeSet::new();
    let mut gradients: Vec<Vec<f32>> = parameters
        .iter()
        .map(|parameter| vec![0.0; parameter.values.len()])
        .collect();
    let mut workspace = ReverseWorkspaceV1::default();
    let mut rows = Vec::new();
    let mut divergences = Vec::with_capacity(count);
    let mut loss = 0.0_f64;
    for root in &input.roots {
        if !seen.insert((root.group_index, root.substep_index)) {
            return Err(invalid("line-b-auxiliary-duplicate-root"));
        }
        let selected = group_tapes
            .get(root.group_index)
            .and_then(|group| group.tapes.get(root.substep_index))
            .ok_or(invalid("line-b-auxiliary-root-outside-batch"))?;
        let Some(target) = &root.target else {
            divergences.push(None);
            continue;
        };
        let logits: Vec<f64> = selected
            .tape
            .logits_v1()
            .iter()
            .map(|&logit| f64::from(logit))
            .collect();
        let value = line_b_divergence_v1(input.direction, &logits, target)
            .map_err(|_| invalid("line-b-auxiliary-divergence"))?;
        let exact: Vec<f64> = value.logit_gradient.iter().map(|g| scale * g).collect();
        let d_logits: Vec<f32> = exact.iter().map(|&g| g as f32).collect();
        reverse_decision(
            parameters,
            &mut gradients,
            &selected.tape,
            &d_logits,
            0.0,
            &mut workspace.decision,
        )?;
        loss += scale * value.divergence;
        divergences.push(Some(value.divergence));
        rows.push((d_logits, exact));
    }
    Ok(LineBAuxiliaryTermsV1 {
        active: input.coefficient > 0.0 && !rows.is_empty(),
        gradients,
        rows,
        divergences,
        loss,
    })
}

impl LineBAuxiliaryTermsV1 {
    /// Telemetry norms, then the auxiliary gradients join the ordinary ones
    /// and the gauge accumulator gains the auxiliary rows.
    pub(super) fn combine_v1(
        self,
        parameters: &[NativeNamedParameterV1],
        gradients: &mut [Vec<f32>],
        gauge: &mut ScorerBiasGaugeAccumulatorV1,
    ) -> Result<LineBAuxiliaryResultV1, NativePolicyTrainErrorV1> {
        validate_finite_nested("line_b_auxiliary_gradient", &self.gradients)?;
        let head = HEAD_ONLY_TRAINABLE_TENSORS_V1
            .iter()
            .map(|name| {
                parameters
                    .iter()
                    .position(|parameter| parameter.name == *name)
                    .ok_or(invalid("line-b-auxiliary-head-tensor"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let l2 = |values: &[Vec<f32>]| {
            let mut sum = 0.0_f64;
            for &tensor in &head {
                for &value in &values[tensor] {
                    sum += f64::from(value) * f64::from(value);
                }
            }
            sum.sqrt()
        };
        let ordinary_head_l2 = l2(gradients);
        let auxiliary_head_l2 = l2(&self.gradients);
        let auxiliary_bias_residual = self.gradients[SCORER_SECOND_BIAS][0];
        if self.active {
            for (destination, source) in gradients.iter_mut().zip(&self.gradients) {
                for (value, auxiliary) in destination.iter_mut().zip(source) {
                    *value += *auxiliary;
                }
            }
            for (d_logits, exact) in &self.rows {
                gauge.observe_line_b_auxiliary_v1(d_logits, exact)?;
            }
        }
        Ok(LineBAuxiliaryResultV1 {
            divergences_before: self.divergences,
            auxiliary_loss: self.loss,
            auxiliary_head_l2,
            ordinary_head_l2,
            auxiliary_bias_residual,
        })
    }
}
