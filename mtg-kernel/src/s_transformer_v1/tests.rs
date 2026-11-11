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
    for bad in [
        r#"[["rng","0102"],["rng","00"]]"#,
        r#"[["","00"]]"#,
        r#"[["rng","0g"]]"#,
        r#"[["rng","012"]]"#,
    ] {
        std::fs::write(&extras, bad).unwrap();
        assert!(
            super::tools::starting_state_v1(&checkpoint, &extras).is_err(),
            "{bad}"
        );
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

#[test]
fn parallel_gae_preserves_full_adam_checkpoint() {
    let model = SModelV1::init_parity_v1(TINY_V1, 72).unwrap();
    let mut warm = STrainStateV1::fresh_optimizer_v1(model);
    let mut work = SWorkV1::default();
    let mut data = tiny_groups_v1(81, 15);
    // These two-substep groups cross the worker block boundaries. Use the
    // same card repeatedly in both the object and reference paths.
    for (rows, _, _) in &mut data {
        for (tensor, _) in rows {
            tensor.object_card_ids.fill(1);
            tensor.action_ref_card_ids.fill(1);
        }
    }
    data[0].1 = 0.0;
    data[1].1 = -0.0;
    let groups = as_groups_v1(&data);
    warm.train_step_gae_v1(&groups, 0.5, 1.0e-4, &mut work)
        .unwrap();
    assert!(warm.first_moments_v1().iter().any(|m| *m != 0.0));
    assert!(warm.second_moments_v1().iter().any(|m| *m != 0.0));
    let model = warm.model_v1();
    let mut serial_grad = vec![0.0; model.layout_v1().total];
    model
        .gae_loss_gradient_v1(&groups, 0.5, &mut serial_grad, &mut work)
        .unwrap();
    let before = encode_checkpoint_v1(&warm);
    let mut serial = warm.clone();
    let expected = serial
        .train_step_gae_v1(&groups, 0.5, 1.0e-4, &mut work)
        .unwrap();
    let expected_checkpoint = encode_checkpoint_v1(&serial);
    let loss_bits = |loss: SLossV1| {
        [
            loss.policy_sum,
            loss.value_sum,
            loss.policy_term,
            loss.value_term,
            loss.loss,
        ]
        .map(f32::to_bits)
    };
    for workers in [2, 3, 5] {
        let mut parallel_grad = vec![0.0; serial_grad.len()];
        model
            .gae_loss_gradient_parallel_v1(&groups, 0.5, &mut parallel_grad, workers, &mut work)
            .unwrap();
        for (index, (actual, expected)) in parallel_grad.iter().zip(&serial_grad).enumerate() {
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "gradient {index}, workers {workers}"
            );
        }
        let mut parallel = decode_checkpoint_v1(&before).unwrap();
        let actual = parallel
            .train_step_gae_with_workers_v1(&groups, 0.5, 1.0e-4, &mut work, workers)
            .unwrap();
        assert_eq!(actual.loss.group_count, expected.loss.group_count);
        assert_eq!(loss_bits(actual.loss), loss_bits(expected.loss));
        assert_eq!(actual.adam_step, expected.adam_step);
        assert_eq!(actual.after_state_sha256, expected.after_state_sha256);
        assert_eq!(encode_checkpoint_v1(&parallel), expected_checkpoint);
    }
}

#[test]
fn parallel_gae_reports_first_batch_error_without_updating_adam() {
    let model = SModelV1::init_parity_v1(TINY_V1, 73).unwrap();
    let mut state = STrainStateV1::fresh_optimizer_v1(model);
    let before = encode_checkpoint_v1(&state);
    let data = tiny_groups_v1(82, 8);
    let mut groups = as_groups_v1(&data);
    groups[1].substeps[1].selected = usize::MAX;
    groups[2].substeps.clear();
    let error = |state: &mut STrainStateV1, workers| {
        state
            .train_step_gae_with_workers_v1(&groups, 0.5, 1.0e-4, &mut SWorkV1::default(), workers)
            .unwrap_err()
    };
    let expected = error(&mut state, 1);
    assert!(
        expected.starts_with("S group 1 substep 1: selected"),
        "{expected}"
    );
    assert_eq!(error(&mut state, 2), expected);
    assert_eq!(encode_checkpoint_v1(&state), before);
}

/// The tape-size bound the saved-batch fixture reports equals the lengths a
/// forward actually leaves in its tape, for the S shape and the small test
/// shape, after both larger and smaller decisions.
#[test]
fn tape_bytes_bound_matches_the_buffers_a_forward_sizes() {
    let mut rng = SplitMixV1::new(91);
    for (config, seed) in [(SConfigV1::s_v1(), 5), (TINY_V1, 6)] {
        let model = SModelV1::init_parity_v1(config, seed).unwrap();
        let mut tape = STapeV1::default();
        for _ in 0..6 {
            let t = synthetic_decision_v1(&mut rng, super::tools::PARITY_SHAPE_V1);
            let input = SInputV1::of_tensor_v1(&t);
            let counts = input.validate_v1(&config).unwrap();
            model.forward_v1(input, &mut tape).unwrap();
            assert_eq!(
                STapeV1::bytes_for_counts_v1(&config, counts),
                tape.live_bytes_v1(),
                "{counts:?}"
            );
        }
    }
}

fn bits_v1(values: &[f32]) -> Vec<u32> {
    values.iter().map(|v| v.to_bits()).collect()
}

/// One decision's backward with the final block's rows chosen by `choice`:
/// the gradient through the direct sink and through recorded terms and
/// their replay.
fn backward_bits_v1(
    model: &SModelV1,
    input: SInputV1<'_>,
    tape: &STapeV1,
    dlogits: &[f32],
    dvalue: f32,
    choice: FinalRowsV1,
) -> (Vec<u32>, Vec<u32>) {
    let total = model.layout_v1().total;
    let mut direct = vec![0.0f32; total];
    model
        .backward_rows_v1(
            input,
            tape,
            dlogits,
            dvalue,
            &mut parallel::SGradientSinkV1::Direct(&mut direct),
            &mut SBackScratchV1::default(),
            choice,
        )
        .unwrap();
    let mut terms = parallel::SGradientTermsV1::default();
    model
        .backward_rows_v1(
            input,
            tape,
            dlogits,
            dvalue,
            &mut parallel::SGradientSinkV1::Record(&mut terms),
            &mut SBackScratchV1::default(),
            choice,
        )
        .unwrap();
    let mut replayed = vec![0.0f32; total];
    parallel::replay_verified_test_terms_v1(model.layout_v1(), &mut replayed, vec![terms]).unwrap();
    (bits_v1(&direct), bits_v1(&replayed))
}

/// Logit and value adjoints with exact zeros of both signs among them.
fn adjoints_v1(rng: &mut SplitMixV1, actions: usize) -> (Vec<f32>, f32) {
    let dlogits = (0..actions)
        .map(|_| match rng.below(6) {
            0 => 0.0,
            1 => -0.0,
            _ => rng.uniform(-1.0, 1.0),
        })
        .collect();
    let dvalue = match rng.below(4) {
        0 => 0.0,
        1 => -0.0,
        _ => rng.uniform(-1.0, 1.0),
    };
    (dlogits, dvalue)
}

/// Every bit of the live final block's gradient equals the full backward's,
/// through both sinks, for ragged decisions (one object or one action up to
/// 60 objects and 16 actions), repeated cards, subnormal and signed-zero
/// inputs and adjoints, a one-block model, and warm reused scratch.
#[test]
fn live_final_block_matches_the_full_backward_bit_for_bit() {
    let one_block = SConfigV1 {
        layers: 1,
        ..TINY_V1
    };
    let shapes = [
        super::tools::PARITY_SHAPE_V1,
        ShapeV1 {
            objects: (1, 1),
            edges: (0, 0),
            actions: (1, 3),
            refs: (0, 2),
        },
        ShapeV1 {
            objects: (2, 9),
            edges: (0, 6),
            actions: (1, 1),
            refs: (0, 3),
        },
    ];
    let mut rng = SplitMixV1::new(41);
    let mut checked = 0;
    for (config, seed) in [(SConfigV1::s_v1(), 3), (TINY_V1, 4), (one_block, 5)] {
        let model = SModelV1::init_parity_v1(config, seed).unwrap();
        let mut tape = STapeV1::default();
        let mut warm = SBackScratchV1::default();
        for case in 0..18 {
            let mut t = synthetic_decision_v1(&mut rng, shapes[case % shapes.len()]);
            match case % 6 {
                1 => {
                    t.object_card_ids.fill(7);
                    t.action_ref_card_ids.fill(7);
                }
                2 => {
                    for (index, value) in t.object_features.iter_mut().enumerate() {
                        if index % 5 == 0 {
                            *value = if index % 2 == 0 { 1.0e-40 } else { -0.0 };
                        }
                    }
                    for value in t.action_features.iter_mut().step_by(3) {
                        *value = -1.0e-41;
                    }
                }
                _ => {}
            }
            let input = SInputV1::of_tensor_v1(&t);
            model.forward_v1(input, &mut tape).unwrap();
            let last = &tape.layers[config.layers - 1];
            assert!(final_block_compacts_v1(last), "case {case}");
            let (dlogits, dvalue) = adjoints_v1(&mut rng, tape.logits.len());
            let full = backward_bits_v1(&model, input, &tape, &dlogits, dvalue, FinalRowsV1::All);
            let live = backward_bits_v1(&model, input, &tape, &dlogits, dvalue, FinalRowsV1::Live);
            let auto = backward_bits_v1(&model, input, &tape, &dlogits, dvalue, FinalRowsV1::Auto);
            assert_eq!(full.0, full.1, "full: direct against replay, case {case}");
            assert_eq!(live, full, "live rows, case {case}");
            assert_eq!(auto, full, "chosen rows, case {case}");
            // Reused scratch, as a learner thread's: the same bits after
            // larger and smaller decisions.
            let mut reused = vec![0.0f32; model.layout_v1().total];
            model
                .backward_v1(input, &tape, &dlogits, dvalue, &mut reused, &mut warm)
                .unwrap();
            assert_eq!(bits_v1(&reused), full.0, "warm scratch, case {case}");
            checked += 1;
        }
    }
    assert_eq!(checked, 54);
}

/// A non-finite final-block Q, K or V value makes the backward take every
/// row, so its gradient (non-finite) keeps every bit of the full backward's.
/// Skipping the rows there would drop the NaN an unused object query
/// contributes to the key gradient (`dK = dS^T Q`, with `dS` +0 on that row),
/// which is why the check exists.
#[test]
fn non_finite_final_block_values_keep_the_full_backward() {
    let model = SModelV1::init_parity_v1(TINY_V1, 6).unwrap();
    let d = TINY_V1.d_model;
    let mut rng = SplitMixV1::new(43);
    let t = synthetic_decision_v1(
        &mut rng,
        ShapeV1 {
            objects: (5, 5),
            edges: (2, 2),
            actions: (3, 3),
            refs: (2, 2),
        },
    );
    let input = SInputV1::of_tensor_v1(&t);
    let mut clean = STapeV1::default();
    model.forward_v1(input, &mut clean).unwrap();
    let (dlogits, dvalue) = (vec![0.25, -0.5, 0.125], 0.75);
    let last = TINY_V1.layers - 1;
    // Object token 2's query, an object key and the state token's value.
    for (row, column, poison) in [
        (2, 1, f32::INFINITY),
        (2, 0, f32::NAN),
        (3, d + 2, f32::NEG_INFINITY),
        (0, 2 * d + 5, f32::NAN),
    ] {
        let mut tape = clean.clone();
        tape.layers[last].qkv[row * 3 * d + column] = poison;
        assert!(!final_block_compacts_v1(&tape.layers[last]));
        let full = backward_bits_v1(&model, input, &tape, &dlogits, dvalue, FinalRowsV1::All);
        let auto = backward_bits_v1(&model, input, &tape, &dlogits, dvalue, FinalRowsV1::Auto);
        assert_eq!(auto, full, "row {row} column {column}");
        assert!(
            full.0.iter().any(|bits| !f32::from_bits(*bits).is_finite()),
            "the poisoned value reaches the full gradient"
        );
        if row != 0 && column < d {
            let live = backward_bits_v1(&model, input, &tape, &dlogits, dvalue, FinalRowsV1::Live);
            assert!(
                live.0.iter().all(|bits| f32::from_bits(*bits).is_finite()),
                "the live rows never read an object query"
            );
        }
    }
}

/// The analytic term bound (`TermBoundV1`) is what a full backward records,
/// apart from the compact final block's extra QKV term and its excess
/// values, when every card is non-padding; a compact final block, and
/// padding cards, record no more. Shapes include few objects with many
/// actions, where the compact QKV records more values than the full one.
#[test]
fn term_bound_covers_every_backward_record() {
    let one_block = SConfigV1 {
        layers: 1,
        ..TINY_V1
    };
    let shapes = [
        super::tools::PARITY_SHAPE_V1,
        TINY_SHAPE_V1,
        ShapeV1 {
            objects: (1, 2),
            edges: (0, 1),
            actions: (6, 12),
            refs: (0, 6),
        },
    ];
    let mut rng = SplitMixV1::new(51);
    let mut excess_cases = 0;
    for (config, seed) in [(SConfigV1::s_v1(), 7), (TINY_V1, 8), (one_block, 9)] {
        let model = SModelV1::init_parity_v1(config, seed).unwrap();
        let layout = model.layout_v1();
        let mut tape = STapeV1::default();
        for case in 0..12 {
            let mut t = synthetic_decision_v1(&mut rng, shapes[case % shapes.len()]);
            let padded = case % 4 == 3;
            if !padded {
                for card in t
                    .object_card_ids
                    .iter_mut()
                    .chain(&mut t.action_ref_card_ids)
                {
                    *card = 1 + card.rem_euclid(150);
                }
            }
            let input = SInputV1::of_tensor_v1(&t);
            let counts = input.validate_v1(&config).unwrap();
            let bound = parallel::TermBoundV1::of_counts_v1(layout, counts);
            assert_eq!(parallel::TermBoundV1::of_input_v1(layout, input), bound);
            // The forward refuses a decision whose shapes differ before it
            // records, so its bound reserves nothing.
            let mut malformed = t.clone();
            malformed.object_groups.truncate(t.object_groups.len() - 1);
            let malformed = SInputV1::of_tensor_v1(&malformed);
            assert!(model.forward_v1(malformed, &mut tape).is_err());
            assert_eq!(
                parallel::TermBoundV1::of_input_v1(layout, malformed),
                parallel::TermBoundV1::default()
            );
            model.forward_v1(input, &mut tape).unwrap();
            let (dlogits, dvalue) = adjoints_v1(&mut rng, tape.logits.len());
            let recorded = |choice| {
                let mut terms = parallel::SGradientTermsV1::default();
                model
                    .backward_rows_v1(
                        input,
                        &tape,
                        &dlogits,
                        dvalue,
                        &mut parallel::SGradientSinkV1::Record(&mut terms),
                        &mut SBackScratchV1::default(),
                        choice,
                    )
                    .unwrap();
                terms.recorded_v1()
            };
            let full = recorded(FinalRowsV1::All);
            let live = recorded(FinalRowsV1::Live);
            let d = config.d_model;
            let (m, l) = (1 + counts.actions, 1 + counts.objects + counts.actions);
            let excess = d * (2 * m).saturating_sub(l);
            excess_cases += usize::from(excess > 0);
            for (name, recorded) in [("full", full), ("live", live)] {
                assert!(
                    recorded.values <= bound.values
                        && recorded.linears <= bound.linears
                        && recorded.additions <= bound.additions,
                    "{name} case {case}: {recorded:?} against {bound:?}"
                );
            }
            if !padded {
                assert_eq!(
                    parallel::TermBoundV1 {
                        values: full.values + excess,
                        linears: full.linears + 1,
                        additions: full.additions,
                    },
                    bound,
                    "case {case}"
                );
                assert_eq!(live.linears, bound.linears, "case {case}");
                assert_eq!(live.additions + 2 * counts.objects, bound.additions);
            }
        }
    }
    assert!(excess_cases > 0, "some compact QKV records exceed the full");
}

/// Ragged groups: one to four substeps, long and short decisions, shared
/// cards and signed-zero advantages, so groups cross block boundaries.
fn ragged_groups_v1(seed: u64) -> Vec<(Vec<(NativeFlatDecisionTensorV2, usize)>, f32, f32)> {
    let long = ShapeV1 {
        objects: (8, 14),
        edges: (2, 8),
        actions: (3, 7),
        refs: (1, 6),
    };
    let mut rng = SplitMixV1::new(seed);
    [3, 1, 2, 4, 1, 1, 2, 3, 1, 2]
        .iter()
        .enumerate()
        .map(|(group, &substeps)| {
            let records = (0..substeps)
                .map(|sub| {
                    let shape = if (group + sub) % 3 == 0 {
                        long
                    } else {
                        TINY_SHAPE_V1
                    };
                    let mut t = synthetic_decision_v1(&mut rng, shape);
                    if group % 4 == 1 {
                        t.object_card_ids.fill(3);
                        t.action_ref_card_ids.fill(3);
                    }
                    let a = t.action_features.len() / ACTION_FEATURE_DIM_V1;
                    let selected = rng.below(a as u64) as usize;
                    (t, selected)
                })
                .collect();
            let advantage = match group {
                0 => 0.0,
                5 => -0.0,
                _ => rng.uniform(-2.0, 2.0),
            };
            (records, advantage, rng.uniform(-1.0, 1.0))
        })
        .collect()
}

fn loss_bits_v1(loss: SLossV1) -> [u32; 5] {
    [
        loss.policy_sum,
        loss.value_sum,
        loss.policy_term,
        loss.value_term,
        loss.loss,
    ]
    .map(f32::to_bits)
}

/// Every term plan gives the serial gradient and loss bit for bit: two sets
/// that record one block while the previous replays, one set of several
/// slots, and one slot. Its slots never hold more than the plan reserves,
/// which never exceeds the budget or one slot's bound.
#[test]
fn pipelined_gae_matches_serial_bits_for_every_term_plan() {
    let model = SModelV1::init_parity_v1(TINY_V1, 74).unwrap();
    let data = ragged_groups_v1(84);
    let groups = as_groups_v1(&data);
    let substeps: usize = groups.iter().map(|g| g.substeps.len()).sum();
    assert_eq!(substeps, 20);
    let mut work = SWorkV1::default();
    let mut serial = vec![0.0; model.layout_v1().total];
    let expected = model
        .gae_loss_gradient_v1(&groups, 0.5, &mut serial, &mut work)
        .unwrap();
    let bound = groups
        .iter()
        .flat_map(|g| &g.substeps)
        .map(|s| {
            let counts = s.input.validate_v1(&TINY_V1).unwrap();
            parallel::TermBoundV1::of_counts_v1(model.layout_v1(), counts)
        })
        .reduce(|a, b| parallel::TermBoundV1 {
            values: a.values.max(b.values),
            linears: a.linears.max(b.linears),
            additions: a.additions.max(b.additions),
        })
        .unwrap();
    let slot = bound.bytes_v1();
    for workers in [2, 3, 5] {
        for (budget, sets, slots) in [
            (parallel::TERM_BUDGET_BYTES_V1, 2, workers),
            (2 * workers * slot, 2, workers),
            (2 * workers * slot - 1, 1, 2 * workers - 1),
            (3 * slot, 1, 3),
            (slot, 1, 1),
            (1, 1, 1),
        ] {
            let mut gradient = vec![0.0; serial.len()];
            let (loss, used) = model
                .gae_loss_gradient_planned_v1(
                    &groups,
                    0.5,
                    &mut gradient,
                    workers,
                    &mut work,
                    parallel::PipelineOptionsV1 {
                        budget_bytes: budget,
                        ..Default::default()
                    },
                )
                .unwrap();
            let label = format!("workers {workers}, budget {budget}");
            assert_eq!((used.plan.sets, used.plan.slots), (sets, slots), "{label}");
            assert_eq!(used.plan.bound, bound, "{label}");
            assert_eq!(bits_v1(&gradient), bits_v1(&serial), "{label}");
            assert_eq!(loss_bits_v1(loss), loss_bits_v1(expected), "{label}");
            assert_eq!(loss.group_count, expected.group_count);
            // Every slot is used (20 substeps) and none outgrows its bound.
            assert_eq!(
                used.capacity_bytes,
                used.plan.reserved_bytes_v1(),
                "{label}"
            );
            assert!(used.capacity_bytes <= budget.max(slot), "{label}");
        }
    }
}

/// The first failure in substep order is reported, whatever the plan and
/// worker count: a policy-row refusal, a forward refusal (an index outside
/// its table, or shapes that differ) and an invalid group, early, late and
/// where a block ends while the previous replays.
#[test]
fn pipelined_gae_reports_the_first_failure_in_substep_order() {
    let model = SModelV1::init_parity_v1(TINY_V1, 75).unwrap();
    let data = ragged_groups_v1(85);
    // Group 3 has four substeps; group 9 is last.
    for (group, sub, kind) in [
        (0, 0, 0),
        (0, 2, 1),
        (3, 1, 0),
        (3, 3, 1),
        (4, 0, 2),
        (5, 0, 3),
        (6, 1, 1),
        (7, 2, 3),
        (9, 1, 0),
        (9, 0, 2),
    ] {
        let mut data = data.clone();
        match kind {
            1 => data[group].0[sub].0.object_card_ids[0] = 1_000_000,
            3 => {
                let object_groups = &mut data[group].0[sub].0.object_groups;
                object_groups.truncate(object_groups.len() - 1);
            }
            _ => {}
        }
        let mut groups = as_groups_v1(&data);
        match kind {
            0 => groups[group].substeps[sub].selected = usize::MAX,
            2 => groups[group].value_target = f32::NAN,
            _ => {}
        }
        // A later failure must not win.
        groups[9].substeps[1].selected = usize::MAX - 1;
        let mut work = SWorkV1::default();
        let mut gradient = vec![0.0; model.layout_v1().total];
        let expected = model
            .gae_loss_gradient_v1(&groups, 0.5, &mut gradient, &mut work)
            .unwrap_err();
        for workers in [2, 3, 5] {
            for budget in [parallel::TERM_BUDGET_BYTES_V1, 1] {
                let error = model
                    .gae_loss_gradient_planned_v1(
                        &groups,
                        0.5,
                        &mut gradient,
                        workers,
                        &mut work,
                        parallel::PipelineOptionsV1 {
                            budget_bytes: budget,
                            ..Default::default()
                        },
                    )
                    .unwrap_err();
                assert_eq!(
                    error, expected,
                    "group {group} substep {sub}, workers {workers}, budget {budget}"
                );
            }
        }
    }
}

/// A worker that panics in any phase, a helper or the caller, ends the
/// update with that panic once every helper has joined, and leaves nothing
/// behind: the next update gives the serial bits.
#[test]
fn pipelined_gae_reraises_a_worker_panic_after_joining_its_helpers() {
    let model = SModelV1::init_parity_v1(TINY_V1, 76).unwrap();
    let data = ragged_groups_v1(86);
    let groups = as_groups_v1(&data);
    let mut work = SWorkV1::default();
    let mut serial = vec![0.0; model.layout_v1().total];
    model
        .gae_loss_gradient_v1(&groups, 0.5, &mut serial, &mut work)
        .unwrap();
    for (worker, generation) in [(1, 1), (2, 4), (0, 1), (0, 6), (1, 8)] {
        let mut gradient = vec![0.0; serial.len()];
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            model.gae_loss_gradient_planned_v1(
                &groups,
                0.5,
                &mut gradient,
                3,
                &mut work,
                parallel::PipelineOptionsV1 {
                    panic_at: Some((worker, generation)),
                    ..Default::default()
                },
            )
        }))
        .expect_err("the injected panic reaches the caller");
        let message = panic
            .downcast_ref::<String>()
            .expect("a formatted panic message");
        assert_eq!(
            message,
            &format!("S learner worker {worker} panicked (test)")
        );
    }
    let mut gradient = vec![0.0; serial.len()];
    model
        .gae_loss_gradient_parallel_v1(&groups, 0.5, &mut gradient, 3, &mut work)
        .unwrap();
    assert_eq!(bits_v1(&gradient), bits_v1(&serial));
}
