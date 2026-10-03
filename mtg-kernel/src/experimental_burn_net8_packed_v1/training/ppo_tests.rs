//! CUDA check of the clipped-surrogate PPO loss against an independent f64
//! reference: loss, logit gradient and value gradient, inside and outside the
//! clip range, and equality with the grouped GAE gradient at ratio one.
use super::*;

const OFFSETS: [usize; 4] = [0, 1, 3, 6];
const SELECTED: [usize; 3] = [0, 1, 2];
const GROUPS: [usize; 3] = [0, 0, 1];
const FIRST: [usize; 2] = [0, 2];
const TARGETS: [f32; 2] = [0.5, -0.1];
const ADVANTAGES: [f32; 2] = [0.3, -0.7];
const VALUES: [f32; 3] = [0.2, 999.0, -0.4];
const LOGITS: [f32; 6] = [17.0, 2.0, -1.0, -0.5, 0.25, 1.5];
const DENOMINATOR: f32 = 3.0;
const CLIP: f64 = 0.2;

/// f64 joint log-probabilities per group and d(joint_g)/d(logit_i).
fn reference_joint(logits: &[f64]) -> (Vec<f64>, Vec<Vec<f64>>) {
    let mut joint = vec![0.0; FIRST.len()];
    let mut derivative = vec![vec![0.0; logits.len()]; FIRST.len()];
    for substep in 0..SELECTED.len() {
        let row = &logits[OFFSETS[substep]..OFFSETS[substep + 1]];
        let max = row.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let sum: f64 = row.iter().map(|v| (v - max).exp()).sum();
        let group = GROUPS[substep];
        joint[group] += row[SELECTED[substep]] - (sum.ln() + max);
        for (action, value) in row.iter().enumerate() {
            let probability = (value - max).exp() / sum;
            let indicator = f64::from(u8::from(action == SELECTED[substep]));
            derivative[group][OFFSETS[substep] + action] += indicator - probability;
        }
    }
    (joint, derivative)
}

/// f64 PPO loss and gradients for behavior joints `behavior`.
fn reference(behavior: &[f64]) -> (f64, Vec<f64>, Vec<f64>) {
    let logits: Vec<f64> = LOGITS.iter().map(|v| f64::from(*v)).collect();
    let (joint, derivative) = reference_joint(&logits);
    let mut loss = 0.0;
    let mut dl = vec![0.0; logits.len()];
    for group in 0..FIRST.len() {
        let advantage = f64::from(ADVANTAGES[group]);
        let ratio = (joint[group] - behavior[group]).exp();
        let clipped = ratio.clamp(1.0 - CLIP, 1.0 + CLIP);
        let (surrogate, active) = if ratio * advantage <= clipped * advantage {
            (ratio * advantage, true)
        } else {
            (
                clipped * advantage,
                ratio > 1.0 - CLIP && ratio < 1.0 + CLIP,
            )
        };
        loss -= surrogate;
        if active {
            for (i, d) in derivative[group].iter().enumerate() {
                dl[i] -= advantage * ratio * d / f64::from(DENOMINATOR);
            }
        }
    }
    let mut dv = vec![0.0; VALUES.len()];
    for (group, first) in FIRST.iter().enumerate() {
        let error = f64::from(VALUES[*first]) - f64::from(TARGETS[group]);
        loss += 0.5 * error * error;
        dv[*first] = 2.0 * 0.5 * error / f64::from(DENOMINATOR);
    }
    (loss / f64::from(DENOMINATOR), dl, dv)
}

fn device_loss(behavior: Option<&[f32]>) -> (f32, Vec<f32>, Vec<f32>) {
    let device = burn_cuda::CudaDevice::new(1);
    let host = HostPackingWorkspace {
        action_offsets: OFFSETS.to_vec(),
        ..Default::default()
    };
    let plan = build_dense_group_loss_plan_gae_v1(
        &host,
        &SELECTED,
        &GROUPS,
        &FIRST,
        &TARGETS,
        &ADVANTAGES,
        &device,
    )
    .unwrap();
    let logits = Tensor::<CudaAutodiffBackendV1, 1>::from_data(
        TensorData::new(LOGITS.to_vec(), [LOGITS.len()]),
        &device,
    )
    .require_grad();
    let values = Tensor::<CudaAutodiffBackendV1, 1>::from_data(
        TensorData::new(VALUES.to_vec(), [VALUES.len()]),
        &device,
    )
    .require_grad();
    let loss = match behavior {
        Some(behavior) => {
            dense_group_loss_ppo_v1(
                logits.clone(),
                values.clone(),
                &plan,
                behavior,
                CLIP as f32,
                0.5,
                DENOMINATOR,
            )
            .unwrap()
            .0
        }
        None => dense_group_loss_gae_v1(logits.clone(), values.clone(), &plan, 0.5, DENOMINATOR)
            .unwrap(),
    };
    let value = loss.clone().into_data().to_vec::<f32>().unwrap()[0];
    let gradients = loss.backward();
    let read = |tensor: &Tensor<CudaAutodiffBackendV1, 1>| {
        tensor
            .grad(&gradients)
            .unwrap()
            .into_data()
            .to_vec::<f32>()
            .unwrap()
    };
    (value, read(&logits), read(&values))
}

#[test]
#[ignore = "requires an idle CUDA GPU 1"]
fn ppo_loss_matches_f64_reference_and_gae_gradient_at_ratio_one() {
    const TOLERANCE: f64 = 3.0e-6;
    let logits: Vec<f64> = LOGITS.iter().map(|v| f64::from(*v)).collect();
    let (joint, _) = reference_joint(&logits);
    // Ratio one, inside the range, and beyond both clip bounds per group.
    for shift in [
        [0.0, 0.0],
        [0.1, -0.1],
        [0.5, 0.5],
        [-0.5, -0.5],
        [0.5, -0.5],
    ] {
        let behavior: Vec<f64> = joint.iter().zip(shift).map(|(j, s)| j - s).collect();
        let device_behavior: Vec<f32> = behavior.iter().map(|v| *v as f32).collect();
        let (loss, dl, dv) = device_loss(Some(&device_behavior));
        let (expected_loss, expected_dl, expected_dv) = reference(&behavior);
        assert!(
            (f64::from(loss) - expected_loss).abs() <= TOLERANCE * (1.0 + expected_loss.abs()),
            "loss {loss} vs {expected_loss} at shift {shift:?}"
        );
        for (actual, expected) in dl
            .iter()
            .zip(&expected_dl)
            .chain(dv.iter().zip(&expected_dv))
        {
            assert!(
                (f64::from(*actual) - expected).abs() <= TOLERANCE,
                "gradient {actual} vs {expected} at shift {shift:?}"
            );
        }
    }
    let behavior: Vec<f32> = joint.iter().map(|v| *v as f32).collect();
    let (_, ppo_dl, ppo_dv) = device_loss(Some(&behavior));
    let (_, gae_dl, gae_dv) = device_loss(None);
    for (a, b) in ppo_dl.iter().zip(&gae_dl).chain(ppo_dv.iter().zip(&gae_dv)) {
        assert!(
            (a - b).abs() <= 1e-5,
            "ratio-one PPO gradient {a} vs GAE {b}"
        );
    }
}
