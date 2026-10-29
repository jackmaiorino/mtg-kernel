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
    pub(crate) scheduled_count: Option<usize>,
    pub(crate) direction: LineBDivergenceV1,
    /// `c`; the mean runs over all selected roots, censored ones included.
    pub(crate) coefficient: f64,
    pub(crate) roots: Vec<LineBAuxiliaryRootV1>,
}

/// Outcome-free telemetry of one update (proposal 12:51).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LineBAuxiliaryResultV1 {
    pub(crate) gradient_components: serde_json::Value,
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
    /// STAGE-B2.md section 4 item 2's cap receipt, when a cap is configured.
    pub(crate) cap: Option<serde_json::Value>,
    /// The Adam step norms of the ordinary and combined gradients, when
    /// requested (filled by the update after its Adam step).
    pub(crate) adam_steps: Option<serde_json::Value>,
    /// The parameters after the ordinary-only Adam step from the same
    /// state, beside `adam_steps` (EX1's paired-step telemetry).
    pub(crate) ordinary_only_parameters: Option<Vec<NativeNamedParameterV1>>,
}

pub(super) struct LineBAuxiliaryTermsV1 {
    /// At least one available root and `c > 0`; otherwise the update is the
    /// ordinary one bit for bit (adding zeros could flip a `-0.0` gradient).
    active: bool,
    gradients: Vec<Vec<f32>>,
    rows: Vec<(Vec<f32>, Vec<f64>)>,
    divergences: Vec<Option<f64>>,
    loss: f64,
    cap: Option<f64>,
    step_telemetry: bool,
}

impl LineBAuxiliaryTermsV1 {
    pub(super) fn with_cap_v1(mut self, cap: Option<f64>, step_telemetry: bool) -> Self {
        self.cap = cap;
        self.step_telemetry = step_telemetry;
        self
    }

    pub(super) fn step_telemetry_v1(&self) -> bool {
        self.step_telemetry
    }

    #[cfg(test)]
    pub(super) fn gradients_for_test_v1(&self) -> &[Vec<f32>] {
        &self.gradients
    }

    /// Terms another auxiliary source (archive distillation's off-batch rows)
    /// built with the same reverse pass, so they combine exactly as these do.
    pub(super) fn from_parts_v1(
        active: bool,
        gradients: Vec<Vec<f32>>,
        rows: Vec<(Vec<f32>, Vec<f64>)>,
        divergences: Vec<Option<f64>>,
        loss: f64,
    ) -> Self {
        Self {
            active,
            gradients,
            rows,
            divergences,
            loss,
            cap: None,
            step_telemetry: false,
        }
    }
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
    let count = input.scheduled_count.unwrap_or(input.roots.len());
    if count < input.roots.len() {
        return Err(invalid("auxiliary-scheduled-count"));
    }
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
        cap: None,
        step_telemetry: false,
    })
}

impl LineBAuxiliaryTermsV1 {
    /// Telemetry norms, then the auxiliary gradients join the ordinary ones
    /// and the gauge accumulator gains the auxiliary rows.
    pub(super) fn combine_v1(
        mut self,
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
        let component = |kind: &str| {
            let (mut a, mut b, mut dot) = (0.0_f64, 0.0_f64, 0.0_f64);
            for (i, parameter) in parameters.iter().enumerate() {
                let value = parameter.name.starts_with("value_head.");
                let policy = parameter.name.starts_with("scorer.");
                if kind == "all"
                    || (kind == "value" && value)
                    || (kind == "policy" && policy)
                    || (kind == "shared" && !value && !policy)
                {
                    for (&x, &y) in self.gradients[i].iter().zip(&gradients[i]) {
                        let (x, y) = (f64::from(x), f64::from(y));
                        a += x * x;
                        b += y * y;
                        dot += x * y;
                    }
                }
            }
            serde_json::json!({"auxiliary_l2":a.sqrt(),"ordinary_l2":b.sqrt(),"dot":dot,
                "ratio":(b>0.0).then(||(a/b).sqrt())})
        };
        let gradient_components = serde_json::json!({"all":component("all"),
            "policy":component("policy"),"shared":component("shared"),"value":component("value")});
        let cap = if self.step_telemetry || self.cap.is_some() {
            Some(self.apply_cap_v1(self.cap, gradients)?)
        } else {
            None
        };
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
            gradient_components,
            divergences_before: self.divergences,
            auxiliary_loss: self.loss,
            auxiliary_head_l2,
            ordinary_head_l2,
            auxiliary_bias_residual,
            cap,
            adam_steps: None,
            ordinary_only_parameters: None,
        })
    }

    /// STAGE-B2.md section 4 item 2, and its receipt for every Stage B2
    /// update. `a` and `b` are the whole-network L2 norms of the auxiliary and
    /// ordinary gradients as Adam receives them: after the scorer-bias gauge
    /// projection, which zeroes the second scorer bias's gradient (the raw
    /// residuals stay for the gauge record). With `Some(kappa)` and an active
    /// term: `s = 1` when `a = 0`, `0` when `b = 0 < a`, else
    /// `min(1, kappa b / a)`; otherwise `s = 1`. A scale below 1 multiplies the
    /// auxiliary gradients and the gauge rows (logit gradients and their exact
    /// values) before they join. Ratios are null when `b = 0`.
    fn apply_cap_v1(
        &mut self,
        cap: Option<f64>,
        ordinary: &[Vec<f32>],
    ) -> Result<serde_json::Value, NativePolicyTrainErrorV1> {
        let (a, b) = (projected_l2_v1(&self.gradients), projected_l2_v1(ordinary));
        if !(a.is_finite() && b.is_finite() && cap.is_none_or(|k| k.is_finite() && k > 0.0)) {
            return Err(invalid("search-target-cap-nonfinite"));
        }
        let scale = match cap {
            Some(_) if !self.active || a == 0.0 => 1.0,
            Some(_) if b == 0.0 => 0.0,
            Some(kappa) => (kappa * b / a).min(1.0),
            None => 1.0,
        };
        if scale < 1.0 {
            let single = scale as f32;
            for tensor in &mut self.gradients {
                for value in tensor.iter_mut() {
                    *value *= single;
                }
            }
            for (d_logits, exact) in &mut self.rows {
                for value in d_logits.iter_mut() {
                    *value *= single;
                }
                for value in exact.iter_mut() {
                    *value *= scale;
                }
            }
        }
        let ratio = (b > 0.0).then(|| a / b);
        Ok(
            serde_json::json!({"mode":if cap.is_some() {"ratio"} else {"off"},"kappa":cap,
            "active":self.active,"auxiliary_l2":a,"ordinary_l2":b,"scale":scale,
            "suppressed":cap.is_some() && self.active && a > 0.0 && b == 0.0,
            "binding":scale < 1.0,"raw_ratio":ratio,"applied_ratio":ratio.map(|r| scale * r)}),
        )
    }
}

/// Whole-network L2 over the gradients Adam receives: every coordinate but
/// the second scorer bias, whose gradient the gauge projection zeroes.
pub(super) fn projected_l2_v1(values: &[Vec<f32>]) -> f64 {
    let mut sum = 0.0_f64;
    for (index, tensor) in values.iter().enumerate() {
        for (position, &value) in tensor.iter().enumerate() {
            if index == SCORER_SECOND_BIAS && position == 0 {
                continue;
            }
            sum += f64::from(value) * f64::from(value);
        }
    }
    sum.sqrt()
}

/// How a permuted-control root got its target.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LineBPermutationKindV1 {
    /// Another root of the same menu width supplied the target.
    AcrossRoot,
    /// The only root of its width: its own target rotated by one action.
    WithinRoot,
}

/// The permuted-target control of an auxiliary input (CODEX 11:23 item 6;
/// offline only, never a training target). Available roots of equal menu
/// width, in batch order, rotate their targets (each takes the next root's);
/// a width with a single available root rotates that root's action
/// coordinates by one and is labeled within-root. Censored roots stay
/// censored and every target keeps its values, so its normalization.
pub(crate) fn line_b_permuted_input_v1(
    input: &LineBAuxiliaryInputV1,
) -> (LineBAuxiliaryInputV1, Vec<Option<LineBPermutationKindV1>>) {
    let mut widths: std::collections::BTreeMap<usize, Vec<(usize, &LineBSoftmaxV1)>> =
        Default::default();
    for (index, root) in input.roots.iter().enumerate() {
        if let Some(target) = &root.target {
            widths
                .entry(target.log_probabilities.len())
                .or_default()
                .push((index, target));
        }
    }
    let mut targets: Vec<Option<LineBSoftmaxV1>> =
        input.roots.iter().map(|root| root.target.clone()).collect();
    let mut kinds = vec![None; input.roots.len()];
    for members in widths.values() {
        if let [(index, source)] = members.as_slice() {
            let index = *index;
            let rotate = |values: &[f64]| {
                (0..values.len())
                    .map(|action| values[(action + 1) % values.len()])
                    .collect::<Vec<_>>()
            };
            targets[index] = Some(LineBSoftmaxV1 {
                log_probabilities: rotate(&source.log_probabilities),
                probabilities: rotate(&source.probabilities),
                weight_sum: source.weight_sum,
            });
            kinds[index] = Some(LineBPermutationKindV1::WithinRoot);
        } else {
            for (position, (index, _)) in members.iter().enumerate() {
                let (_, source) = members[(position + 1) % members.len()];
                targets[*index] = Some(source.clone());
                kinds[*index] = Some(LineBPermutationKindV1::AcrossRoot);
            }
        }
    }
    let roots = input
        .roots
        .iter()
        .zip(targets)
        .map(|(root, target)| LineBAuxiliaryRootV1 {
            group_index: root.group_index,
            substep_index: root.substep_index,
            target,
        })
        .collect();
    (
        LineBAuxiliaryInputV1 {
            scheduled_count: input.scheduled_count,
            direction: input.direction,
            coefficient: input.coefficient,
            roots,
        },
        kinds,
    )
}

#[cfg(test)]
mod cap_tests {
    use super::*;

    fn terms(gradients: Vec<Vec<f32>>) -> LineBAuxiliaryTermsV1 {
        LineBAuxiliaryTermsV1::from_parts_v1(
            true,
            gradients,
            vec![(vec![0.5, -0.5], vec![0.5, -0.5])],
            vec![Some(0.1)],
            0.1,
        )
        .with_cap_v1(Some(1.0), false)
    }

    /// The ordinary gradient carries only the gauge residual: after the
    /// projection b = 0, so the auxiliary is suppressed with null ratios.
    #[test]
    fn a_gauge_only_ordinary_gradient_suppresses_the_auxiliary() {
        let mut gradients = vec![vec![0.0_f32; 1]; SCORER_SECOND_BIAS + 1];
        gradients[0][0] = 1.0;
        let mut ordinary = vec![vec![0.0_f32; 1]; SCORER_SECOND_BIAS + 1];
        ordinary[SCORER_SECOND_BIAS][0] = 3.0;
        let mut t = terms(gradients);
        let receipt = t.apply_cap_v1(Some(1.0), &ordinary).unwrap();
        assert_eq!(receipt["scale"].as_f64(), Some(0.0));
        assert_eq!(receipt["suppressed"], true);
        assert!(receipt["raw_ratio"].is_null());
        assert_eq!(projected_l2_v1(&ordinary), 0.0);
    }

    /// STAGE-B2.md section 4 item 2: the scale and its receipt in each case.
    #[test]
    fn cap_scales_suppresses_and_refuses_as_specified() {
        // Binding: a = 5, b = 1, kappa = 1 gives s = 0.2, applied ratio 1.
        let mut binding = terms(vec![vec![3.0, 4.0]]);
        let receipt = binding.apply_cap_v1(Some(1.0), &[vec![0.6, 0.8]]).unwrap();
        let scale = receipt["scale"].as_f64().unwrap();
        assert!((scale - 0.2).abs() < 1e-6); // f32 inputs: b is 1 to rounding
        assert_eq!(receipt["binding"], true);
        assert!((receipt["raw_ratio"].as_f64().unwrap() - 5.0).abs() < 1e-6);
        assert!((receipt["applied_ratio"].as_f64().unwrap() - 1.0).abs() < 1e-12);
        let single = scale as f32;
        assert_eq!(binding.gradients, vec![vec![3.0 * single, 4.0 * single]]);
        assert_eq!(binding.rows[0].0, vec![0.5 * single, -0.5 * single]);
        assert_eq!(binding.rows[0].1, vec![0.5 * scale, -0.5 * scale]);
        // Not binding: s = 1 and nothing changes.
        let mut free = terms(vec![vec![0.3, 0.4]]);
        let receipt = free.apply_cap_v1(Some(1.0), &[vec![0.6, 0.8]]).unwrap();
        assert_eq!(receipt["scale"].as_f64(), Some(1.0));
        assert_eq!(receipt["binding"], false);
        assert_eq!(free.gradients, vec![vec![0.3, 0.4]]);
        // b = 0 < a: suppressed, ratios null.
        let mut suppressed = terms(vec![vec![3.0, 4.0]]);
        let receipt = suppressed
            .apply_cap_v1(Some(1.0), &[vec![0.0, 0.0]])
            .unwrap();
        assert_eq!(receipt["scale"].as_f64(), Some(0.0));
        assert_eq!(receipt["suppressed"], true);
        assert!(receipt["raw_ratio"].is_null() && receipt["applied_ratio"].is_null());
        assert_eq!(suppressed.gradients, vec![vec![0.0, 0.0]]);
        // a = b = 0: s = 1, ratios null.
        let mut both = terms(vec![vec![0.0, 0.0]]);
        let receipt = both.apply_cap_v1(Some(1.0), &[vec![0.0, 0.0]]).unwrap();
        assert_eq!(receipt["scale"].as_f64(), Some(1.0));
        assert!(receipt["raw_ratio"].is_null() && receipt["applied_ratio"].is_null());
        // a = 0 < b: s = 1, ratio 0.
        let mut quiet = terms(vec![vec![0.0, 0.0]]);
        let receipt = quiet.apply_cap_v1(Some(1.0), &[vec![0.6, 0.8]]).unwrap();
        assert_eq!(receipt["scale"].as_f64(), Some(1.0));
        assert_eq!(receipt["raw_ratio"].as_f64(), Some(0.0));
        // Nonfinite norms refuse the update.
        let mut bad = terms(vec![vec![f32::INFINITY, 0.0]]);
        assert!(bad.apply_cap_v1(Some(1.0), &[vec![0.6, 0.8]]).is_err());
    }
}
