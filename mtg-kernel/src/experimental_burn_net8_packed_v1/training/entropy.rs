//! Optional entropy surrogate at observed legal-action prefixes. Beta zero
//! takes the unchanged grouped-GAE graph; execution sampling is unchanged.
use super::*;

pub(super) fn dense_group_loss_with_entropy(
    logits: Tensor<CudaAutodiffBackendV1, 1>,
    values: Tensor<CudaAutodiffBackendV1, 1>,
    plan: &DenseGroupLossPlanGaeV1,
    value_coefficient: f32,
    normalization_group_count: f32,
    coefficient: f32,
) -> Result<Tensor<CudaAutodiffBackendV1, 1>, Box<dyn Error>> {
    if !coefficient.is_finite() || !(0.0..=1.0).contains(&coefficient) {
        return Err(training_error(
            "entropy coefficient must be finite in [0,1]",
        ));
    }
    if coefficient == 0.0 {
        return dense_group_loss_gae_v1(
            logits,
            values,
            plan,
            value_coefficient,
            normalization_group_count,
        );
    }
    let base = dense_group_loss_gae_v1(
        logits.clone(),
        values,
        plan,
        value_coefficient,
        normalization_group_count,
    )?;
    let padded = logits
        .select(0, plan.pad_gather.clone())
        .reshape([plan.substeps, plan.max_actions])
        + plan.pad_mask.clone();
    let row_max = padded.clone().max_dim(1).detach();
    let centered = padded - row_max;
    let log_normalizer = centered.clone().exp().sum_dim(1).log();
    let log_probabilities = centered - log_normalizer;
    // The finite -1e30 padding mask underflows to zero probability, avoiding
    // 0 * -infinity. No padded logit receives entropy gradient. A singleton
    // has log probability zero, hence exactly zero entropy and derivative.
    let negative_entropy = log_probabilities.clone().exp().mul(log_probabilities).sum();
    Ok(base + negative_entropy.mul_scalar(coefficient / normalization_group_count))
}

/// Explicit small CUDA engineering check on GPU 1. Never trains a model.
/// The reference computes both the grouped terminal-GAE loss and entropy
/// derivatives independently in f64, including unused value positions.
pub fn run_gradient_probe() -> Result<serde_json::Value, Box<dyn Error>> {
    const TOLERANCE: f64 = 3.0e-6;
    const DENOMINATOR: f32 = 3.0;
    let device = burn_cuda::CudaDevice::new(1);
    let offsets = [0, 1, 3, 6];
    let selected = [0, 1, 2];
    let groups = [0, 0, 1];
    let first = [0, 2];
    let targets = [0.5_f32, -0.1];
    let advantages = [0.3_f32, -0.7];
    let values = [0.2_f32, 999.0, -0.4];
    let evaluate = |raw: &[f32],
                    off: &[usize],
                    sel: &[usize],
                    group: &[usize],
                    starts: &[usize],
                    val: &[f32],
                    target: &[f32],
                    advantage: &[f32],
                    beta: f32,
                    old: bool|
     -> Result<(f32, Vec<f32>, Vec<f32>), Box<dyn Error>> {
        let host = HostPackingWorkspace {
            action_offsets: off.to_vec(),
            ..Default::default()
        };
        let plan = build_dense_group_loss_plan_gae_v1(
            &host, sel, group, starts, target, advantage, &device,
        )?;
        let logits = Tensor::<CudaAutodiffBackendV1, 1>::from_data(
            TensorData::new(raw.to_vec(), [raw.len()]),
            &device,
        )
        .require_grad();
        let value = Tensor::<CudaAutodiffBackendV1, 1>::from_data(
            TensorData::new(val.to_vec(), [val.len()]),
            &device,
        )
        .require_grad();
        let loss = if old {
            dense_group_loss_gae_v1(logits.clone(), value.clone(), &plan, 0.5, DENOMINATOR)?
        } else {
            dense_group_loss_with_entropy(
                logits.clone(),
                value.clone(),
                &plan,
                0.5,
                DENOMINATOR,
                beta,
            )?
        };
        let actual_loss = loss.clone().into_data().to_vec::<f32>()?[0];
        let gradient = loss.backward();
        let dl = logits
            .grad(&gradient)
            .ok_or_else(|| training_error("missing entropy logit gradient"))?
            .into_data()
            .to_vec::<f32>()?;
        let dv = value
            .grad(&gradient)
            .ok_or_else(|| training_error("missing entropy value gradient"))?
            .into_data()
            .to_vec::<f32>()?;
        Ok((actual_loss, dl, dv))
    };
    let fixture = [17.0_f32, 2.0, -1.0, -0.5, 0.25, 1.5];
    let fixtures = [
        fixture,
        [17.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        [17.0, 100.0, -100.0, -0.5, 0.25, 1.5],
        fixture.map(|x| x + 8.0),
    ];
    let mut cases = Vec::new();
    for (fixture_index, raw) in fixtures.iter().enumerate() {
        let original = evaluate(
            raw,
            &offsets,
            &selected,
            &groups,
            &first,
            &values,
            &targets,
            &advantages,
            0.0,
            true,
        )?;
        for beta in [0.0_f32, 0.05, 1.0] {
            let actual = evaluate(
                raw,
                &offsets,
                &selected,
                &groups,
                &first,
                &values,
                &targets,
                &advantages,
                beta,
                false,
            )?;
            if beta == 0.0 {
                if actual.0.to_bits() != original.0.to_bits()
                    || actual.1.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
                        != original.1.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
                    || actual.2.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
                        != original.2.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
                {
                    return Err(training_error("zero entropy changed the old loss/gradient"));
                }
            }
            let mut loss = 0.0_f64;
            let mut gradient = Vec::new();
            for (row, pair) in offsets.windows(2).enumerate() {
                let logits: Vec<_> = raw[pair[0]..pair[1]]
                    .iter()
                    .map(|x| f64::from(*x))
                    .collect();
                let maximum = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                let z = logits.iter().map(|x| (x - maximum).exp()).sum::<f64>().ln();
                let logp: Vec<_> = logits.iter().map(|x| x - maximum - z).collect();
                let entropy = -logp.iter().map(|x| x.exp() * x).sum::<f64>();
                let advantage = f64::from(advantages[groups[row]]);
                loss += -advantage * logp[selected[row]] - f64::from(beta) * entropy;
                for (i, lp) in logp.iter().enumerate() {
                    let p = lp.exp();
                    gradient.push(
                        (advantage * (p - if i == selected[row] { 1.0 } else { 0.0 })
                            + f64::from(beta) * p * (lp + entropy))
                            / f64::from(DENOMINATOR),
                    );
                }
            }
            let mut value_gradient = vec![0.0_f64; values.len()];
            for (g, index) in first.iter().enumerate() {
                let error = f64::from(values[*index]) - f64::from(targets[g]);
                loss += 0.5 * error * error;
                value_gradient[*index] = error / f64::from(DENOMINATOR);
            }
            loss /= f64::from(DENOMINATOR);
            let max_error = actual
                .1
                .iter()
                .zip(&gradient)
                .chain(actual.2.iter().zip(&value_gradient))
                .map(|(a, b)| (f64::from(*a) - b).abs())
                .fold(0.0, f64::max);
            // The saturated fixture's large base loss has normal f32 rounding;
            // use a scale-aware scalar bound, but a strict derivative bound.
            if !actual.0.is_finite()
                || actual.1.iter().chain(&actual.2).any(|x| !x.is_finite())
                || (f64::from(actual.0) - loss).abs() > TOLERANCE * (1.0 + loss.abs())
                || max_error > TOLERANCE
                || actual.1[0] != 0.0
                || actual.2[1] != 0.0
            {
                return Err(training_error(format!("entropy oracle mismatch fixture={fixture_index} beta={beta} loss={} expected={loss} gradient_error={max_error}",actual.0)));
            }
            for pair in offsets.windows(2) {
                if actual.1[pair[0]..pair[1]].iter().sum::<f32>().abs() > TOLERANCE as f32 {
                    return Err(training_error(
                        "entropy gradient violates score-shift gauge",
                    ));
                }
            }
            cases.push(serde_json::json!({"fixture":fixture_index,"beta":beta,"actual_loss":actual.0,
                "reference_loss":loss,"maximum_gradient_error":max_error,"logit_gradient":actual.1,"value_gradient":actual.2}));
        }
    }
    let full = evaluate(
        &fixture,
        &offsets,
        &selected,
        &groups,
        &first,
        &values,
        &targets,
        &advantages,
        0.05,
        false,
    )?;
    let a = evaluate(
        &fixture[..3],
        &[0, 1, 3],
        &[0, 1],
        &[0, 0],
        &[0],
        &values[..2],
        &targets[..1],
        &advantages[..1],
        0.05,
        false,
    )?;
    let b = evaluate(
        &fixture[3..],
        &[0, 3],
        &[2],
        &[0],
        &[0],
        &values[2..],
        &targets[1..],
        &advantages[1..],
        0.05,
        false,
    )?;
    if (full.0 - a.0 - b.0).abs() > TOLERANCE as f32
        || full
            .1
            .iter()
            .zip(a.1.iter().chain(&b.1))
            .any(|(x, y)| (*x - *y).abs() > TOLERANCE as f32)
        || full
            .2
            .iter()
            .zip(a.2.iter().chain(&b.2))
            .any(|(x, y)| (*x - *y).abs() > TOLERANCE as f32)
    {
        return Err(training_error("entropy chunk/group normalization differs"));
    }
    // Permute each legal menu and its selected action together. Neither the
    // objective nor the unpermuted derivatives may depend on action ordering.
    let permutation = [0, 2, 1, 5, 4, 3];
    let permuted = permutation.map(|i| fixture[i]);
    let permuted_result = evaluate(
        &permuted,
        &offsets,
        &[0, 0, 0],
        &groups,
        &first,
        &values,
        &targets,
        &advantages,
        0.05,
        false,
    )?;
    if (full.0 - permuted_result.0).abs() > TOLERANCE as f32
        || permutation.iter().enumerate().any(|(i, original)| {
            (permuted_result.1[i] - full.1[*original]).abs() > TOLERANCE as f32
        })
        || full
            .2
            .iter()
            .zip(&permuted_result.2)
            .any(|(a, b)| (*a - *b).abs() > TOLERANCE as f32)
    {
        return Err(training_error("entropy action-permutation control differs"));
    }
    for invalid in [-0.1, 1.01, f32::INFINITY, f32::NAN] {
        if evaluate(
            &fixture,
            &offsets,
            &selected,
            &groups,
            &first,
            &values,
            &targets,
            &advantages,
            invalid,
            false,
        )
        .is_ok()
        {
            return Err(training_error("invalid entropy coefficient accepted"));
        }
    }
    CudaAutodiffBackendV1::sync(&device)?;
    Ok(
        serde_json::json!({"schema":"public-entropy-gradient-probe/v1","complete":true,"gpu_ordinal":1,
        "cases":cases,"absolute_gradient_tolerance":TOLERANCE,"zero_loss_gradient_bit_exact":true,
        "chunk_normalization_consistent":true,"action_permutation_control_passed":true,"invalid_coefficients_rejected":true,
        "non_claim":"CUDA loss/gradient engineering only; no model update, rollout or strength evidence."}),
    )
}
