//! Scheduled uniform KL recovery. This is a finite-batch surrogate on the
//! existing collection distribution, not an unbiased on-policy estimator.
use super::*;
use serde_json::{json, Value};

pub(crate) fn recovery_beta_v1(update: u64, recovery: bool) -> Result<f32, String> {
    if !(1..=64).contains(&update) {
        return Err("uniform KL recovery update must be in 1..=64".into());
    }
    Ok(if !recovery {
        0.0
    } else if update <= 32 {
        0.01
    } else {
        0.01 * (64 - update) as f32 / 32.0
    })
}

/// Max-centered binary64 scalar avoids overflow and cancellation of a large
/// common logit shift. Gradients below use the trainer's binary32 probabilities.
pub(crate) fn uniform_kl_v1(logits: &[f32]) -> f64 {
    if logits.len() == 1 {
        return 0.0;
    }
    let maximum = logits
        .iter()
        .copied()
        .map(f64::from)
        .fold(f64::NEG_INFINITY, f64::max);
    let sum = logits
        .iter()
        .map(|z| (f64::from(*z) - maximum).exp())
        .sum::<f64>();
    sum.ln()
        - logits.iter().map(|z| f64::from(*z) - maximum).sum::<f64>() / logits.len() as f64
        - (logits.len() as f64).ln()
}

pub(crate) fn uniform_kl_gradient_v1(log_probabilities: &[f32], scale: f32) -> Vec<f32> {
    if log_probabilities.len() == 1 || scale == 0.0 {
        return vec![0.0; log_probabilities.len()];
    }
    let uniform = 1.0 / log_probabilities.len() as f32;
    log_probabilities
        .iter()
        .map(|lp| scale * (lp.exp() - uniform))
        .collect()
}

/// Add an observed-rounding bound for the actual auxiliary addition. The
/// binary64 reference has zero analytic sum; differences include binary32
/// softmax, reciprocal, scale, and the final actor+regularizer addition. The
/// reduction bound counts the actual reverse scorer-bias additions.
pub(super) fn observe_regularizer_v1(
    gauge: &mut ScorerBiasGaugeAccumulatorV1,
    logits: &[f32],
    actor: &[f32],
    combined: &[f32],
    scale: f32,
) -> Result<(), NativePolicyTrainErrorV1> {
    let n = logits.len();
    let maximum = logits
        .iter()
        .copied()
        .map(f64::from)
        .fold(f64::NEG_INFINITY, f64::max);
    let sum = logits
        .iter()
        .map(|z| (f64::from(*z) - maximum).exp())
        .sum::<f64>();
    let exact: Vec<f64> = logits
        .iter()
        .map(|z| f64::from(scale) * (((f64::from(*z) - maximum).exp() / sum) - 1.0 / n as f64))
        .collect();
    let (_, log_probabilities) = selected_log_softmax(logits, 0)?;
    let regularizer = uniform_kl_gradient_v1(&log_probabilities, scale);
    // Reconstruct only the declared arithmetic, not the supplied combined
    // residual. A corrupted combined gradient cannot enlarge its own bound.
    let expected: Vec<f32> = actor
        .iter()
        .zip(&regularizer)
        .map(|(a, r)| *a + *r)
        .collect();
    let magnitude = actor
        .iter()
        .zip(&expected)
        .map(|(a, c)| (f64::from(*c) - f64::from(*a)).abs())
        .sum::<f64>();
    let rounding = actor
        .iter()
        .zip(&expected)
        .zip(&exact)
        .map(|((a, c), r)| ((f64::from(*c) - f64::from(*a)) - r).abs())
        .sum::<f64>();
    if !combined
        .iter()
        .zip(&expected)
        .all(|(a, b)| a.to_bits() == b.to_bits())
    {
        return Err(NativePolicyTrainErrorV1::ParameterManifest);
    }
    let operations = n
        .checked_add(1)
        .ok_or(NativePolicyTrainErrorV1::GaugeBoundOverflow)?;
    let gamma = f32_gamma(operations)?;
    let bound = rounding + gamma * magnitude;
    gauge.per_substep_bound_sum += bound;
    gauge.sum_abs_policy_coefficients += magnitude;
    gauge.substep_count += 1;
    gauge.total_action_count += n;
    gauge.max_action_count = gauge.max_action_count.max(n);
    gauge.high_precision_residual += exact.iter().sum::<f64>();
    gauge.substep_bounds.push(NativeGaugeSubstepBoundV1 {
        action_count: n,
        abs_policy_coefficient: magnitude,
        gamma_operation_count: operations,
        gamma,
        bound_component: bound,
    });
    Ok(())
}

#[derive(Default)]
struct ActivationStats {
    count: u64,
    saturated: u64,
    sum: f64,
    min: f64,
    max: f64,
    bins: [u64; 6],
}
impl ActivationStats {
    fn add(&mut self, values: &[f32]) {
        for &h in values {
            let d = 1.0_f32 - h * h;
            if self.count == 0 {
                self.min = f64::from(d);
                self.max = f64::from(d);
            }
            self.count += 1;
            self.saturated += u64::from(h.abs() >= 0.99);
            self.sum += f64::from(d);
            self.min = self.min.min(f64::from(d));
            self.max = self.max.max(f64::from(d));
            let i = [0.0001, 0.001, 0.01, 0.1, 0.5]
                .iter()
                .position(|x| d < *x)
                .unwrap_or(5);
            self.bins[i] += 1;
        }
    }
    fn value(&self) -> Value {
        json!({"count":self.count,"abs_ge_0_99":self.saturated,
        "one_minus_h_squared":{"min":self.min,"max":self.max,
        "mean":if self.count==0 {0.0} else {self.sum/self.count as f64},
        "bin_upper_exclusive":[0.0001,0.001,0.01,0.1,0.5,null],"counts":self.bins}})
    }
}

pub(super) struct ExplorationDiagnosticsV1 {
    value: Vec<Vec<f32>>,
    regularizer: Vec<Vec<f32>>,
    activations: Value,
}
impl ExplorationDiagnosticsV1 {
    pub(super) fn capture(
        parameters: &[NativeNamedParameterV1],
        groups: &[GroupTapeV1<'_>],
        value_coefficient: f32,
        group_count: f32,
        beta: f32,
    ) -> Result<Self, NativePolicyTrainErrorV1> {
        let mut value = parameters
            .iter()
            .map(|p| vec![0.0; p.values.len()])
            .collect::<Vec<_>>();
        let mut regularizer = value.clone();
        let mut workspace = ReverseDecisionWorkspaceV1::default();
        let mut stats = std::collections::BTreeMap::<&str, ActivationStats>::new();
        for group in groups.iter().rev() {
            for (i, selected) in group.tapes.iter().enumerate().rev() {
                let tape = &*selected.tape;
                let zeros = vec![0.0; selected.log_probabilities.len()];
                reverse_decision(
                    parameters,
                    &mut value,
                    tape,
                    &zeros,
                    if i == 0 {
                        (value_coefficient / group_count) * (2.0 * group.value_error)
                    } else {
                        0.0
                    },
                    &mut workspace,
                )?;
                let d = uniform_kl_gradient_v1(&selected.log_probabilities, beta / group_count);
                reverse_decision(parameters, &mut regularizer, tape, &d, 0.0, &mut workspace)?;
                let encoders = [
                    ("object", Some(tape.object_encoder)),
                    ("edge", tape.edge_encoder),
                    ("node", Some(tape.node_update)),
                    ("state", Some(tape.state_encoder)),
                    ("action_ref", tape.action_ref_encoder),
                    ("action", Some(tape.action_encoder)),
                ];
                for (name, encoder) in encoders {
                    if let Some(e) = encoder {
                        let s = stats.entry(name).or_default();
                        s.add(tape.f32_values_v1(e.first_output));
                        s.add(tape.f32_values_v1(e.second_output));
                    }
                }
                stats
                    .entry("scorer")
                    .or_default()
                    .add(tape.f32_values_v1(tape.scorer_hidden));
                stats
                    .entry("value")
                    .or_default()
                    .add(tape.f32_values_v1(tape.value_hidden));
            }
        }
        value[SCORER_SECOND_BIAS][0] = 0.0;
        regularizer[SCORER_SECOND_BIAS][0] = 0.0;
        Ok(Self {
            value,
            regularizer,
            activations: Value::Object(
                stats
                    .into_iter()
                    .map(|(k, v)| (k.into(), v.value()))
                    .collect(),
            ),
        })
    }
    pub(super) fn finish(
        self,
        parameters: &[NativeNamedParameterV1],
        combined: &[Vec<f32>],
        next: &[NativeNamedParameterV1],
    ) -> Value {
        let mut blocks = Vec::new();
        for (name, start, end) in [
            ("shared_encoder", 0, SCORER_FIRST_WEIGHT),
            ("actor_head", SCORER_FIRST_WEIGHT, VALUE_FIRST_WEIGHT),
            ("value_head", VALUE_FIRST_WEIGHT, parameters.len()),
        ] {
            let (
                mut aa,
                mut vv,
                mut rr,
                mut av,
                mut ar,
                mut vr,
                mut ww,
                mut dd,
                mut cc,
                mut subtraction_bound_squared,
            ) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
            for p in start..end {
                // One index joins the aligned parameter, update, and gradient
                // buffers. Preserve the qualified traversal and reduction order.
                #[allow(clippy::needless_range_loop)]
                for i in 0..parameters[p].values.len() {
                    let v = f64::from(self.value[p][i]);
                    let r = f64::from(self.regularizer[p][i]);
                    let a = f64::from(combined[p][i]) - v - r;
                    let w = f64::from(parameters[p].values[i]);
                    let d = f64::from(next[p].values[i]) - w;
                    let c = f64::from(combined[p][i]);
                    cc += c * c;
                    let bound =
                        (f64::EPSILON / (1.0 - f64::EPSILON)) * (c.abs() + v.abs() + r.abs());
                    subtraction_bound_squared += bound * bound;
                    aa += a * a;
                    vv += v * v;
                    rr += r * r;
                    av += a * v;
                    ar += a * r;
                    vr += v * r;
                    ww += w * w;
                    dd += d * d;
                }
            }
            blocks.push(json!({"block":name,"actor_norm":aa.sqrt(),"value_norm":vv.sqrt(),
                "regularizer_norm":rr.sqrt(),"actor_value_dot":av,"actor_regularizer_dot":ar,"value_regularizer_dot":vr,
                "combined_norm":cc.sqrt(),"actor_residual_cancellation_ratio":if aa==0.0 {None} else {Some((cc.sqrt()+vv.sqrt()+rr.sqrt())/aa.sqrt())},
                "residual_f64_subtraction_l2_roundoff_bound":subtraction_bound_squared.sqrt(),
                "current_weight_norm":ww.sqrt(),"realized_adam_update_norm":dd.sqrt()}));
        }
        json!({"schema":"native-exploration-diagnostics/v1","extra_backward_passes":2,
            "actor_component":"combined_minus_independent_value_minus_independent_regularizer_in_f64; includes binary32 backward rounding residual",
            "backward_f32_component_separation_error_bound":null,
            "regularizer_component":"beta-weighted, physical-group-normalized", "canonical_gauge_components_zero":true,
            "blocks":blocks,"tanh":self.activations})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schedule_boundaries_and_refusal() {
        for t in 1..=64 {
            assert_eq!(recovery_beta_v1(t, false).unwrap().to_bits(), 0);
        }
        assert_eq!(recovery_beta_v1(1, true).unwrap(), 0.01);
        assert_eq!(recovery_beta_v1(32, true).unwrap(), 0.01);
        assert_eq!(recovery_beta_v1(33, true).unwrap(), 0.01 * 31.0 / 32.0);
        assert_eq!(recovery_beta_v1(64, true).unwrap(), 0.0);
        assert!(recovery_beta_v1(0, true).is_err());
        assert!(recovery_beta_v1(65, true).is_err());
    }
    #[test]
    fn stable_kl_derivative_shift_permutation_saturation_singleton() {
        for z in [
            vec![0.0],
            vec![1.0, -2.0, 0.5],
            vec![1000.0, -1000.0, 0.0],
            vec![0.0; 65536],
        ] {
            let (_, lp) = selected_log_softmax(&z, 0).unwrap();
            let grad = uniform_kl_gradient_v1(&lp, 1.0);
            assert!(uniform_kl_v1(&z).is_finite());
            assert!(grad.iter().all(|x| x.is_finite()));
            let shifted: Vec<_> = z.iter().map(|x| x + 4096.0).collect();
            assert!((uniform_kl_v1(&shifted) - uniform_kl_v1(&z)).abs() < 1e-10);
            let mut reversed = z.clone();
            reversed.reverse();
            assert!((uniform_kl_v1(&reversed) - uniform_kl_v1(&z)).abs() < 1e-10);
            for i in 0..z.len().min(3) {
                let mut a = z.clone();
                let mut b = z.clone();
                a[i] += 0.125;
                b[i] -= 0.125;
                let finite_difference = (uniform_kl_v1(&a) - uniform_kl_v1(&b)) / 0.25;
                assert!((finite_difference - f64::from(grad[i])).abs() < 0.0006);
            }
            if z.len() == 1 {
                assert_eq!(grad, [0.0]);
                assert_eq!(uniform_kl_v1(&z), 0.0);
            }
        }
        let (_, lp) = selected_log_softmax(&[1000.0, -1000.0], 0).unwrap();
        assert_eq!(uniform_kl_gradient_v1(&lp, 1.0), [0.5, -0.5]);
    }

    #[test]
    fn gauge_bounds_declared_rounding_and_refuses_corrupted_addition() {
        for logits in [
            vec![1000.0, -1000.0, 0.0],
            vec![0.0; 4096],
            vec![1.0, -2.0, 0.5],
        ] {
            let (_, lp) = selected_log_softmax(&logits, 0).unwrap();
            let actor: Vec<f32> = lp
                .iter()
                .enumerate()
                .map(|(i, p)| (if i == 0 { -0.3 } else { 0.0 }) - p.exp() * (-0.3))
                .collect();
            let reg = uniform_kl_gradient_v1(&lp, 0.01 / 1188.0);
            let combined: Vec<f32> = actor.iter().zip(reg).map(|(a, r)| *a + r).collect();
            let mut gauge = ScorerBiasGaugeAccumulatorV1::default();
            gauge.observe(&logits, 0, -0.3).unwrap();
            observe_regularizer_v1(&mut gauge, &logits, &actor, &combined, 0.01 / 1188.0).unwrap();
            let residual = combined.iter().rev().fold(0.0_f32, |s, x| s + x);
            let record = gauge.finish(residual, 0).unwrap();
            assert!(f64::from(residual.abs()) <= record.derived_absolute_bound);
            let mut corrupted = combined;
            corrupted[0] += 0.25;
            assert!(observe_regularizer_v1(
                &mut ScorerBiasGaugeAccumulatorV1::default(),
                &logits,
                &actor,
                &corrupted,
                0.01 / 1188.0
            )
            .is_err());
        }
    }
}
