#![allow(clippy::type_complexity)]

use super::kernels::{self, gemm_reference_v1, gemm_v1, InitV1};
use super::tools::{synthetic_decision_v1, ShapeV1};
use super::*;

const TINY_V1: SConfigV1 = SConfigV1 {
    d_model: 8,
    layers: 2,
    heads: 2,
    ffn: 16,
    card_dim: 4,
    card_vocab: 200,
};

const TINY_SHAPE_V1: ShapeV1 = ShapeV1 {
    objects: (1, 6),
    edges: (0, 4),
    actions: (1, 4),
    refs: (0, 4),
};

fn tiny_groups_v1(
    seed: u64,
    groups: usize,
) -> Vec<(Vec<(NativeFlatDecisionTensorV2, usize)>, f32, f32)> {
    let mut rng = SplitMixV1::new(seed);
    (0..groups)
        .map(|g| {
            let substeps = 1 + g % 2;
            let records = (0..substeps)
                .map(|_| {
                    let t = synthetic_decision_v1(&mut rng, TINY_SHAPE_V1);
                    let a = t.action_features.len() / ACTION_FEATURE_DIM_V1;
                    let selected = rng.below(a as u64) as usize;
                    (t, selected)
                })
                .collect();
            (records, rng.uniform(-2.0, 2.0), rng.uniform(-1.0, 1.0))
        })
        .collect()
}

#[allow(clippy::type_complexity)]
fn as_groups_v1(
    data: &[(Vec<(NativeFlatDecisionTensorV2, usize)>, f32, f32)],
) -> Vec<SGroupV1<'_>> {
    data.iter()
        .map(|(records, advantage, target)| SGroupV1 {
            substeps: records
                .iter()
                .map(|(t, selected)| SSubstepV1 {
                    input: SInputV1::of_tensor_v1(t),
                    selected: *selected,
                })
                .collect(),
            advantage: *advantage,
            value_target: *target,
        })
        .collect()
}

#[test]
fn s_layout_has_the_pilot_parameter_count_and_names() {
    let layout = LayoutV1::of_v1(SConfigV1::s_v1()).unwrap();
    assert_eq!(layout.total, 460_346);
    assert_eq!(layout.tensors[0].name, "ref_bias");
    assert_eq!(layout.tensors[1].name, "card.weight");
    assert_eq!(layout.tensors.last().unwrap().name, "value.2.bias");
    let lookup: usize = layout
        .tensors
        .iter()
        .filter(|t| t.name == "card.weight" || t.name == "group.weight")
        .map(|t| t.len)
        .sum();
    assert_eq!(lookup, 10_144);
    let gauge: usize = layout.gauge_ranges_v1().iter().map(|r| r.len()).sum();
    assert_eq!(gauge, 1 + 3 * 96);
}

#[test]
fn gemm_matches_the_plain_loop_bit_for_bit() {
    let mut rng = SplitMixV1::new(7);
    for &(m, n, k) in &[
        (1, 1, 1),
        (3, 17, 5),
        (4, 16, 9),
        (9, 33, 40),
        (13, 96, 130),
        (5, 8, 41),
    ] {
        let a: Vec<f32> = (0..m * k).map(|_| rng.uniform(-3.0, 3.0)).collect();
        let b: Vec<f32> = (0..k * n).map(|_| rng.uniform(-3.0, 3.0)).collect();
        let bias: Vec<f32> = (0..n).map(|_| rng.uniform(-1.0, 1.0)).collect();
        let start: Vec<f32> = (0..m * n).map(|_| rng.uniform(-1.0, 1.0)).collect();
        for init in [InitV1::Bias(&bias), InitV1::Zero, InitV1::Existing] {
            // Row-major A and the transposed view (column strides).
            for (rs, cs, a_view) in [
                (k, 1, a.clone()),
                (1, m, {
                    let mut t = vec![0.0f32; m * k];
                    for i in 0..m {
                        for kk in 0..k {
                            t[kk * m + i] = a[i * k + kk];
                        }
                    }
                    t
                }),
            ] {
                let mut fast = start.clone();
                let mut plain = start.clone();
                gemm_v1(m, n, k, &a_view, rs, cs, &b, init, &mut fast);
                gemm_reference_v1(m, n, k, &a_view, rs, cs, &b, init, &mut plain);
                assert_eq!(
                    fast.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    plain.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    "m {m} n {n} k {k}"
                );
            }
        }
    }
}

#[test]
fn transcendentals_are_accurate_and_vector_paths_match_scalar() {
    let mut worst_exp = 0.0f64;
    let mut x = -80.0f32;
    while x < 80.0 {
        let exact = f64::from(x).exp();
        let rel = ((f64::from(kernels::exp_v1(x)) - exact) / exact).abs();
        worst_exp = worst_exp.max(rel);
        x += 0.0137;
    }
    assert!(worst_exp < 4.0e-7, "exp relative error {worst_exp}");
    let mut worst_cdf = 0.0f64;
    let mut worst_grad = 0.0f64;
    let mut inputs = Vec::new();
    let mut x = -12.0f32;
    while x < 12.0 {
        inputs.push(x);
        let xd = f64::from(x);
        let cdf = 0.5 * (1.0 + libm::erf(xd / std::f64::consts::SQRT_2));
        worst_cdf = worst_cdf.max((f64::from(kernels::normal_cdf_v1(x)) - cdf).abs());
        let grad = cdf + xd * (-0.5 * xd * xd).exp() / (2.0 * std::f64::consts::PI).sqrt();
        worst_grad = worst_grad.max((f64::from(kernels::gelu_grad_v1(x)) - grad).abs());
        x += 0.00731;
    }
    assert!(worst_cdf < 3.0e-7, "normal cdf error {worst_cdf}");
    assert!(worst_grad < 2.0e-6, "gelu grad error {worst_grad}");
    let mut sliced = vec![0.0f32; inputs.len()];
    kernels::gelu_slice_v1(&inputs, &mut sliced);
    for (x, y) in inputs.iter().zip(&sliced) {
        assert_eq!(kernels::gelu_v1(*x).to_bits(), y.to_bits());
    }
    let mut grads = vec![1.0f32; inputs.len()];
    kernels::gelu_backward_slice_v1(&inputs, &mut grads);
    for (x, g) in inputs.iter().zip(&grads) {
        assert_eq!(kernels::gelu_grad_v1(*x).to_bits(), g.to_bits());
    }
}

fn loss_of_v1(
    values: &[f32],
    data: &[(Vec<(NativeFlatDecisionTensorV2, usize)>, f32, f32)],
) -> f64 {
    let model = SModelV1::from_values_v1(TINY_V1, values.to_vec()).unwrap();
    let mut grad = vec![0.0f32; values.len()];
    let groups = as_groups_v1(data);
    f64::from(
        model
            .gae_loss_gradient_v1(&groups, 0.5, &mut grad, &mut SWorkV1::default())
            .unwrap()
            .loss,
    )
}

#[test]
fn gradient_matches_central_differences() {
    let model = SModelV1::init_parity_v1(TINY_V1, 3).unwrap();
    let data = tiny_groups_v1(11, 4);
    let groups = as_groups_v1(&data);
    let mut grad = vec![0.0f32; model.layout_v1().total];
    model
        .gae_loss_gradient_v1(&groups, 0.5, &mut grad, &mut SWorkV1::default())
        .unwrap();
    let layout = model.layout_v1().clone();
    let gauge = layout.gauge_ranges_v1();
    let card_row0 = layout.card..layout.card + TINY_V1.card_dim;
    let mut rng = SplitMixV1::new(5);
    let mut checked = 0;
    for spec in &layout.tensors {
        for _ in 0..6 {
            let index = spec.offset + rng.below(spec.len as u64) as usize;
            if gauge.iter().any(|r| r.contains(&index)) || card_row0.contains(&index) {
                assert_eq!(grad[index].to_bits(), 0, "{} gauge or padding", spec.name);
                continue;
            }
            let h = 4.0e-3f32;
            let mut plus = model.values_v1().to_vec();
            plus[index] += h;
            let mut minus = model.values_v1().to_vec();
            minus[index] -= h;
            let fd = (loss_of_v1(&plus, &data) - loss_of_v1(&minus, &data))
                / (f64::from(plus[index]) - f64::from(minus[index]));
            let g = f64::from(grad[index]);
            assert!(
                (fd - g).abs() <= 3.0e-3 + 0.03 * g.abs().max(fd.abs()),
                "{} [{}]: analytic {g}, central difference {fd}",
                spec.name,
                index - spec.offset
            );
            checked += 1;
        }
    }
    assert!(checked > 100);
}

#[test]
fn gauge_and_padding_gradients_are_exactly_zero() {
    let model = SModelV1::init_parity_v1(SConfigV1::s_v1(), 9).unwrap();
    let mut rng = SplitMixV1::new(4);
    let tensors: Vec<_> = (0..6)
        .map(|_| synthetic_decision_v1(&mut rng, super::tools::PARITY_SHAPE_V1))
        .collect();
    let groups: Vec<_> = tensors
        .iter()
        .map(|t| SGroupV1 {
            substeps: vec![SSubstepV1 {
                input: SInputV1::of_tensor_v1(t),
                selected: 0,
            }],
            advantage: 1.0,
            value_target: 0.25,
        })
        .collect();
    let mut grad = vec![0.0f32; model.layout_v1().total];
    model
        .gae_loss_gradient_v1(&groups, 0.5, &mut grad, &mut SWorkV1::default())
        .unwrap();
    for range in model.layout_v1().gauge_ranges_v1() {
        assert!(grad[range].iter().all(|g| g.to_bits() == 0));
    }
    let card = model.layout_v1().card;
    assert!(
        grad[card..card + 32].iter().all(|g| g.to_bits() == 0),
        "card row 0"
    );
    assert!(grad.iter().any(|g| *g != 0.0));
}

#[test]
fn forward_refuses_card_tokens_outside_the_table() {
    let model = SModelV1::init_pilot_scheme_v1(SConfigV1::s_v1(), 1).unwrap();
    let mut rng = SplitMixV1::new(2);
    let mut t = synthetic_decision_v1(&mut rng, super::tools::MEAN_SHAPE_V1);
    t.object_card_ids[0] = 257;
    let error = model
        .forward_v1(SInputV1::of_tensor_v1(&t), &mut STapeV1::default())
        .unwrap_err();
    assert!(error.contains("object_card_ids"), "{error}");
}

#[test]
fn forward_is_repeatable_and_independent_of_tape_history() {
    let model = SModelV1::init_parity_v1(SConfigV1::s_v1(), 2).unwrap();
    let mut rng = SplitMixV1::new(3);
    let tensors: Vec<_> = (0..8)
        .map(|_| synthetic_decision_v1(&mut rng, super::tools::PARITY_SHAPE_V1))
        .collect();
    let mut shared = STapeV1::default();
    for t in &tensors {
        let warm = model
            .forward_v1(SInputV1::of_tensor_v1(t), &mut shared)
            .unwrap();
        let cold = model
            .forward_v1(SInputV1::of_tensor_v1(t), &mut STapeV1::default())
            .unwrap();
        assert_eq!(
            warm.logits.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            cold.logits.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
        );
        assert_eq!(warm.value.to_bits(), cold.value.to_bits());
        assert!(warm.value.abs() <= 1.0);
    }
}

#[test]
fn fresh_optimizer_is_zero_and_starting_hash_binds_extras() {
    let model = SModelV1::init_pilot_scheme_v1(SConfigV1::s_v1(), 20_261_102).unwrap();
    let state = STrainStateV1::fresh_optimizer_v1(model);
    assert_eq!(state.adam_step_v1(), 0);
    assert!(state.first_moments_v1().iter().all(|v| v.to_bits() == 0));
    assert!(state.second_moments_v1().iter().all(|v| v.to_bits() == 0));
    let a = state.starting_state_sha256_v1(&[("rng", &[1u8, 2, 3][..])]);
    let b = state.starting_state_sha256_v1(&[("rng", &[1u8, 2, 4][..])]);
    assert_ne!(a, b);
    assert_ne!(a, state.state_sha256_v1());
}

#[test]
fn starting_state_command_needs_a_fresh_optimizer_and_hashes_extras_in_order() {
    let dir = std::env::temp_dir().join(format!("s-starting-state-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let model = SModelV1::init_pilot_scheme_v1(TINY_V1, 20_261_102).unwrap();
    let fresh = STrainStateV1::fresh_optimizer_v1(model);
    let checkpoint = dir.join("fresh.bin");
    std::fs::write(&checkpoint, encode_checkpoint_v1(&fresh)).unwrap();
    let extras = dir.join("extras.json");
    std::fs::write(&extras, r#"[["rng","010203"],["cursor","00"]]"#).unwrap();
    let receipt = super::tools::starting_state_v1(&checkpoint, &extras).unwrap();
    assert_eq!(
        receipt["starting_state_sha256"],
        fresh.starting_state_sha256_v1(&[("rng", &[1u8, 2, 3][..]), ("cursor", &[0u8][..])])
    );
    assert_eq!(receipt["state_sha256"], fresh.state_sha256_v1());
    assert_eq!(receipt["adam_step"], 0);
    for bad in [r#"[["rng","0102"],["rng","00"]]"#, r#"[["","00"]]"#, r#"[["rng","0g"]]"#, r#"[["rng","012"]]"#] {
        std::fs::write(&extras, bad).unwrap();
        assert!(super::tools::starting_state_v1(&checkpoint, &extras).is_err(), "{bad}");
    }
    std::fs::write(&extras, r#"[["rng","00"]]"#).unwrap();
    let mut stepped = fresh.clone();
    let data = tiny_groups_v1(12, 3);
    stepped
        .train_step_gae_v1(&as_groups_v1(&data), 0.5, 1.0e-4, &mut SWorkV1::default())
        .unwrap();
    std::fs::write(&checkpoint, encode_checkpoint_v1(&stepped)).unwrap();
    assert!(super::tools::starting_state_v1(&checkpoint, &extras).is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn checkpoint_round_trips_and_refuses_tampering() {
    let model = SModelV1::init_parity_v1(TINY_V1, 8).unwrap();
    let mut state = STrainStateV1::fresh_optimizer_v1(model);
    let data = tiny_groups_v1(12, 3);
    state
        .train_step_gae_v1(&as_groups_v1(&data), 0.5, 1.0e-4, &mut SWorkV1::default())
        .unwrap();
    let bytes = encode_checkpoint_v1(&state);
    assert_eq!(&bytes[..8], b"MTGCKPT\0");
    let restored = decode_checkpoint_v1(&bytes).unwrap();
    assert_eq!(restored, state);
    assert_eq!(restored.state_sha256_v1(), state.state_sha256_v1());
    let mut flipped = bytes.clone();
    let last = flipped.len() - 1;
    flipped[last] ^= 1;
    assert!(decode_checkpoint_v1(&flipped).is_err());
    let mut padded = bytes.clone();
    padded.push(0);
    assert!(decode_checkpoint_v1(&padded).is_err());
}

#[test]
fn updates_are_deterministic_across_runs_and_resume() {
    let dir = std::env::temp_dir().join(format!("s-transformer-v1-resume-{}", std::process::id()));
    let receipt = tools::resume_check_v1(&dir, 5, 4, 2, 4).unwrap();
    assert_eq!(receipt["resume_identical"].as_bool(), Some(true));
    assert_eq!(receipt["run_to_run_identical"].as_bool(), Some(true));
    let _ = std::fs::remove_dir_all(&dir);
}
