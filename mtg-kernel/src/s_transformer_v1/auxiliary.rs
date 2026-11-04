//! EX1's and W5's update terms on S (collab LANES/lead/W5-DESIGN.md v1.3
//! section 3; net-8's `native_policy_train_step_v1::archive_distillation_v1`
//! and `line_b_auxiliary_v1`, the same equations on S):
//!
//! - **Forward-KL rows:** each off-batch row's divergence of S's logits from
//!   its frozen target, `c / D` each. The logit gradient `(c / D) dKL/dz` is
//!   rounded once to binary32 and runs through S's reverse pass into its own
//!   gradient buffer with no value gradient; S's gauge ranges are zeroed.
//! - **Cap** (STAGE-B2.md section 4 item 2): with `Some(kappa)` and an active
//!   term, the auxiliary gradient is scaled by `min(1, kappa b / a)` (`1` when
//!   `a = 0`, `0` when `b = 0 < a`), `a` and `b` the whole-network L2 norms of
//!   the auxiliary and ordinary gradients. The receipt exists whenever a cap
//!   or step telemetry is requested.
//! - **Policy mask** (W5): the substeps the search played take no policy
//!   gradient, and the policy term is normalized over the groups with an
//!   unmasked substep (`SModelV1::gae_loss_gradient_masked_v1`).
//! - **Step telemetry:** the L2 norms of the combined Adam step, the
//!   ordinary-only Adam step from the same state, and their difference, with
//!   the ordinary-only parameters for the movement rows.
//!
//! An inactive term (no rows, or `c = 0`) adds nothing: the update is the
//! ordinary one bit for bit.

use super::*;
use crate::line_b_teacher_target_v1::{line_b_divergence_v1, LineBDivergenceV1, LineBSoftmaxV1};
use serde_json::{json, Value};

/// One off-batch row: its input and frozen target.
pub struct SAuxiliaryRowV1<'a> {
    pub input: SInputV1<'a>,
    pub target: &'a LineBSoftmaxV1,
}

/// `coefficient / denominator` times the sum over `rows` of the divergence
/// in `direction`, beside the GAE batch, before one Adam step.
pub struct SAuxiliaryInputV1<'a> {
    pub direction: LineBDivergenceV1,
    pub coefficient: f64,
    /// At least the row count, and positive.
    pub denominator: usize,
    /// `Some(kappa)` caps the auxiliary gradient's L2 at `kappa` times the
    /// ordinary one.
    pub cap: Option<f64>,
    pub step_telemetry: bool,
    pub rows: Vec<SAuxiliaryRowV1<'a>>,
    /// W5: the in-batch substeps `(group, substep)` the search played.
    /// Empty leaves the GAE term unchanged.
    pub policy_mask: Vec<(usize, usize)>,
}

/// One update's auxiliary receipt.
#[derive(Clone, Debug, PartialEq)]
pub struct SAuxiliaryResultV1 {
    /// Per row, the divergence at the start-of-update parameters.
    pub divergences_before: Vec<Option<f64>>,
    /// `(c / D) * sum` of the rows' divergences.
    pub auxiliary_loss: f64,
    /// Auxiliary and ordinary gradient norms and dot products, whole network
    /// and by part (policy head, value head, shared).
    pub gradient_components: Value,
    /// L2 norms over S's heads (`policy.*` and `value.*`) before the cap and
    /// Adam, net-8's head-norm fields: the auxiliary gradient's and the
    /// ordinary one's (gauge entries zero in both).
    pub auxiliary_head_l2: f64,
    pub ordinary_head_l2: f64,
    /// The auxiliary term's own raw gradient at the policy output bias
    /// (`policy.2.bias`, a gauge entry, zeroed before Adam): net-8's
    /// scorer-bias residual.
    pub auxiliary_bias_residual: f32,
    pub cap: Option<Value>,
    pub adam_steps: Option<Value>,
    /// The parameters after the ordinary-only Adam step, beside `adam_steps`.
    pub ordinary_only: Option<SModelV1>,
    /// The masked substeps and the policy term's group count (`None`
    /// without a mask).
    pub masked_substeps: usize,
    pub policy_groups: Option<usize>,
}

/// A validated policy-gradient mask over a batch.
pub(crate) struct SPolicyMaskV1 {
    /// Per group, per substep: masked.
    pub(crate) masked: Vec<Vec<bool>>,
    /// The groups with an unmasked substep (1 when none, never then read
    /// through a nonzero policy term).
    pub(crate) policy_count: f32,
    pub(crate) policy_groups: usize,
}

/// The input's mask over `groups`, or `None` when it masks nothing. Every
/// entry names an existing substep, once.
fn policy_mask_v1(
    input: &SAuxiliaryInputV1<'_>,
    groups: &[SGroupV1<'_>],
) -> Result<Option<SPolicyMaskV1>, String> {
    if input.policy_mask.is_empty() {
        return Ok(None);
    }
    let mut masked: Vec<Vec<bool>> = groups
        .iter()
        .map(|group| vec![false; group.substeps.len()])
        .collect();
    for &(group, substep) in &input.policy_mask {
        let slot = masked
            .get_mut(group)
            .and_then(|substeps| substeps.get_mut(substep))
            .ok_or("S policy mask names a substep outside the batch")?;
        if *slot {
            return Err("S policy mask repeats a substep".into());
        }
        *slot = true;
    }
    let unmasked = masked
        .iter()
        .filter(|substeps| substeps.iter().any(|m| !m))
        .count();
    if unmasked > 1 << 24 {
        return Err("S policy mask group count is not exact in binary32".into());
    }
    Ok(Some(SPolicyMaskV1 {
        masked,
        policy_count: unmasked.max(1) as f32,
        policy_groups: unmasked,
    }))
}

/// Whole-network L2 (gauge entries are zero in both buffers).
fn l2_v1(values: &[f32]) -> f64 {
    values
        .iter()
        .map(|&v| f64::from(v) * f64::from(v))
        .sum::<f64>()
        .sqrt()
}

/// The part a tensor belongs to.
fn part_v1(name: &str) -> &'static str {
    if name.starts_with("policy.") {
        "policy"
    } else if name.starts_with("value.") {
        "value"
    } else {
        "shared"
    }
}

/// L2 over S's head tensors (`policy.*` and `value.*`).
fn head_l2_v1(layout: &LayoutV1, values: &[f32]) -> f64 {
    layout
        .tensors
        .iter()
        .filter(|spec| part_v1(&spec.name) != "shared")
        .flat_map(|spec| &values[spec.offset..spec.offset + spec.len])
        .map(|&v| f64::from(v) * f64::from(v))
        .sum::<f64>()
        .sqrt()
}

fn components_v1(layout: &LayoutV1, auxiliary: &[f32], ordinary: &[f32]) -> Value {
    let component = |kind: &str| {
        let (mut a, mut b, mut dot) = (0.0_f64, 0.0_f64, 0.0_f64);
        for spec in &layout.tensors {
            if kind != "all" && part_v1(&spec.name) != kind {
                continue;
            }
            let range = spec.offset..spec.offset + spec.len;
            for (&x, &y) in auxiliary[range.clone()].iter().zip(&ordinary[range]) {
                let (x, y) = (f64::from(x), f64::from(y));
                a += x * x;
                b += y * y;
                dot += x * y;
            }
        }
        json!({"auxiliary_l2":a.sqrt(),"ordinary_l2":b.sqrt(),"dot":dot,
            "ratio":(b>0.0).then(||(a/b).sqrt())})
    };
    json!({"all":component("all"),"policy":component("policy"),
        "shared":component("shared"),"value":component("value")})
}

/// The update's gradients before the cap: the (masked) GAE term and the
/// rows' auxiliary term, each with its gauge ranges zeroed.
pub(crate) struct SGradientsV1 {
    pub(crate) loss: SLossV1,
    pub(crate) ordinary: Vec<f32>,
    pub(crate) auxiliary: Vec<f32>,
    pub(crate) divergences: Vec<Option<f64>>,
    pub(crate) auxiliary_loss: f64,
    pub(crate) mask: Option<SPolicyMaskV1>,
    /// The auxiliary gradient at the policy output bias before the gauge
    /// ranges were zeroed.
    pub(crate) auxiliary_bias_residual: f32,
}

impl SModelV1 {
    /// Validates `input` and computes [`SGradientsV1`] at these parameters.
    pub(crate) fn auxiliary_gradients_v1(
        &self,
        groups: &[SGroupV1<'_>],
        value_coefficient: f32,
        input: &SAuxiliaryInputV1<'_>,
        work: &mut SWorkV1,
    ) -> Result<SGradientsV1, String> {
        if (input.rows.is_empty() && !input.step_telemetry)
            || input.denominator < input.rows.len()
            || input.denominator == 0
        {
            return Err("S auxiliary denominator below the row count, or no rows".into());
        }
        if !(input.coefficient.is_finite() && input.coefficient >= 0.0) {
            return Err("S auxiliary coefficient must be finite and nonnegative".into());
        }
        if input
            .cap
            .is_some_and(|kappa| !(kappa.is_finite() && kappa > 0.0))
        {
            return Err("S auxiliary cap must be positive and finite".into());
        }
        let mask = policy_mask_v1(input, groups)?;
        let total = self.layout.total;
        let mut ordinary = vec![0.0f32; total];
        let loss = self.gae_loss_gradient_masked_v1(
            groups,
            value_coefficient,
            mask.as_ref(),
            &mut ordinary,
            work,
        )?;
        let scale = input.coefficient / input.denominator as f64;
        let mut auxiliary = vec![0.0f32; total];
        let mut divergences = Vec::with_capacity(input.rows.len());
        let mut auxiliary_loss = 0.0_f64;
        for (index, row) in input.rows.iter().enumerate() {
            self.forward_into_v1(row.input, &mut work.tape)?;
            let logits: Vec<f64> = work.tape.logits.iter().map(|&z| f64::from(z)).collect();
            if logits.len() < 2 {
                return Err(format!("S auxiliary row {index} is a forced row"));
            }
            let value = line_b_divergence_v1(input.direction, &logits, row.target)
                .map_err(|e| format!("S auxiliary row {index}: {e:?}"))?;
            let d_logits: Vec<f32> = value
                .logit_gradient
                .iter()
                .map(|g| (scale * g) as f32)
                .collect();
            self.backward_v1(
                row.input,
                &work.tape,
                &d_logits,
                0.0,
                &mut auxiliary,
                &mut work.back,
            )?;
            auxiliary_loss += scale * value.divergence;
            divergences.push(Some(value.divergence));
        }
        let auxiliary_bias_residual = auxiliary[self.layout.pol2.b];
        for range in self.layout.gauge_ranges_v1() {
            auxiliary[range].fill(0.0);
        }
        if auxiliary.iter().any(|g| !g.is_finite())
            || !auxiliary_loss.is_finite()
            || !auxiliary_bias_residual.is_finite()
        {
            return Err("S auxiliary gradient is not finite".into());
        }
        Ok(SGradientsV1 {
            loss,
            ordinary,
            auxiliary,
            divergences,
            auxiliary_loss,
            mask,
            auxiliary_bias_residual,
        })
    }
}

impl STrainStateV1 {
    /// One RT1 update with the auxiliary terms (module documentation): the
    /// (masked) GAE gradient, the rows' forward-KL gradient, the cap, then one
    /// Adam step on their sum.
    pub fn train_step_gae_auxiliary_v1(
        &mut self,
        groups: &[SGroupV1<'_>],
        value_coefficient: f32,
        learning_rate: f32,
        input: &SAuxiliaryInputV1<'_>,
        work: &mut SWorkV1,
    ) -> Result<(SStepV1, SAuxiliaryResultV1), String> {
        let SGradientsV1 {
            loss,
            ordinary: mut grad,
            mut auxiliary,
            divergences,
            auxiliary_loss,
            mask,
            auxiliary_bias_residual,
        } = self
            .model
            .auxiliary_gradients_v1(groups, value_coefficient, input, work)?;
        let active = input.coefficient > 0.0 && !input.rows.is_empty();
        let gradient_components = components_v1(&self.model.layout, &auxiliary, &grad);
        let auxiliary_head_l2 = head_l2_v1(&self.model.layout, &auxiliary);
        let ordinary_head_l2 = head_l2_v1(&self.model.layout, &grad);
        let cap = if input.step_telemetry || input.cap.is_some() {
            let (a, b) = (l2_v1(&auxiliary), l2_v1(&grad));
            let applied = match input.cap {
                Some(_) if !active || a == 0.0 => 1.0,
                Some(_) if b == 0.0 => 0.0,
                Some(kappa) => (kappa * b / a).min(1.0),
                None => 1.0,
            };
            if applied < 1.0 {
                let single = applied as f32;
                for value in &mut auxiliary {
                    *value *= single;
                }
            }
            let ratio = (b > 0.0).then(|| a / b);
            Some(json!({"mode":if input.cap.is_some() {"ratio"} else {"off"},
                "kappa":input.cap,"active":active,"auxiliary_l2":a,"ordinary_l2":b,
                "scale":applied,"suppressed":input.cap.is_some() && active && a > 0.0 && b == 0.0,
                "binding":applied < 1.0,"raw_ratio":ratio,
                "applied_ratio":ratio.map(|r| applied * r)}))
        } else {
            None
        };
        let ordinary_gradient = input.step_telemetry.then(|| grad.clone());
        if active {
            for (value, extra) in grad.iter_mut().zip(&auxiliary) {
                *value += *extra;
            }
        }
        let before = self.model.values.clone();
        let ordinary_only = ordinary_gradient
            .map(|ordinary| {
                let mut alone = self.clone();
                alone.adam_step_with_gradient_v1(&ordinary, learning_rate)?;
                Ok::<_, String>(alone.model)
            })
            .transpose()?;
        self.adam_step_with_gradient_v1(&grad, learning_rate)?;
        let adam_steps = ordinary_only.as_ref().map(|alone| {
            let (mut combined, mut ordinary, mut difference) = (0.0_f64, 0.0_f64, 0.0_f64);
            for ((&x, &y), &o) in before.iter().zip(&self.model.values).zip(&alone.values) {
                let (x, y, o) = (f64::from(x), f64::from(y), f64::from(o));
                combined += (y - x) * (y - x);
                ordinary += (o - x) * (o - x);
                difference += (y - o) * (y - o);
            }
            json!({"combined_l2":combined.sqrt(),"ordinary_l2":ordinary.sqrt(),
                "difference_l2":difference.sqrt()})
        });
        let masked_substeps = mask.as_ref().map_or(0, |_| input.policy_mask.len());
        Ok((
            SStepV1 {
                loss,
                adam_step: self.adam_step,
                after_state_sha256: self.state_sha256_v1(),
            },
            SAuxiliaryResultV1 {
                divergences_before: divergences,
                auxiliary_loss,
                gradient_components,
                auxiliary_head_l2,
                ordinary_head_l2,
                auxiliary_bias_residual,
                cap,
                adam_steps,
                ordinary_only,
                masked_substeps,
                policy_groups: mask.map(|m| m.policy_groups),
            },
        ))
    }
}

#[cfg(test)]
mod tests;
