//! The S auxiliary step: inactive equals ordinary, the combined gradient
//! is the derivative of the combined loss (masked GAE plus forward-KL rows),
//! the cap scales the auxiliary gradient, the ordinary-only telemetry step
//! is the plain step, and the mask's refusals.

use super::super::tools::{synthetic_decision_v1, ShapeV1};
use super::*;
use crate::line_b_teacher_target_v1::line_b_softmax_v1;

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
    actions: (2, 4),
    refs: (0, 4),
};

type DataV1 = Vec<(Vec<(NativeFlatDecisionTensorV2, usize)>, f32, f32)>;

fn data_v1(seed: u64, groups: usize) -> DataV1 {
    let mut rng = SplitMixV1::new(seed);
    (0..groups)
        .map(|g| {
            let substeps = 1 + g % 2;
            let records = (0..substeps)
                .map(|_| {
                    let t = synthetic_decision_v1(&mut rng, TINY_SHAPE_V1);
                    let a = t.action_features.len() / ACTION_FEATURE_DIM_V1;
                    (t, rng.below(a as u64) as usize)
                })
                .collect();
            (records, rng.uniform(-2.0, 2.0), rng.uniform(-1.0, 1.0))
        })
        .collect()
}

fn groups_v1(data: &DataV1) -> Vec<SGroupV1<'_>> {
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

/// Off-batch rows with arbitrary softmax targets.
fn rows_v1(seed: u64, count: usize) -> Vec<(NativeFlatDecisionTensorV2, LineBSoftmaxV1)> {
    let mut rng = SplitMixV1::new(seed);
    (0..count)
        .map(|_| {
            let t = synthetic_decision_v1(&mut rng, TINY_SHAPE_V1);
            let a = t.action_features.len() / ACTION_FEATURE_DIM_V1;
            let z: Vec<f64> = (0..a).map(|_| f64::from(rng.uniform(-2.0, 2.0))).collect();
            (t, line_b_softmax_v1(&z).unwrap())
        })
        .collect()
}

fn input_v1<'a>(
    rows: &'a [(NativeFlatDecisionTensorV2, LineBSoftmaxV1)],
    cap: Option<f64>,
    policy_mask: Vec<(usize, usize)>,
) -> SAuxiliaryInputV1<'a> {
    SAuxiliaryInputV1 {
        direction: LineBDivergenceV1::Forward,
        coefficient: 2.0,
        denominator: 8,
        cap,
        step_telemetry: true,
        rows: rows
            .iter()
            .map(|(t, target)| SAuxiliaryRowV1 {
                input: SInputV1::of_tensor_v1(t),
                target,
            })
            .collect(),
        policy_mask,
    }
}

fn state_v1() -> STrainStateV1 {
    STrainStateV1::fresh_optimizer_v1(SModelV1::init_parity_v1(TINY_V1, 3).unwrap())
}

/// PR A's ordinary update, pinned from PR A's own code (#507 head
/// 239cfad50, before the mask existed): two steps of the unmasked GAE update
/// and of the inactive auxiliary path both give PR A's loss bits and state
/// hashes.
#[test]
fn the_ordinary_update_keeps_pr_a_bits() {
    const LOSS: u32 = 0xbf20_ad7e;
    const POLICY: u32 = 0xbf36_a3ea;
    const VALUE: u32 = 0x3e2f_b362;
    const STATES: [&str; 2] = [
        "93643d2c6681d0516c0eaf579adf5d5066b1f302a21a65a68cd905ea8b3195e7",
        "1aa90b5168f818d5b38056b5ca4ba56111cf2a801369b8d0ab33657906150901",
    ];
    let data = data_v1(11, 5);
    let groups = groups_v1(&data);
    for inactive in [false, true] {
        let mut state = state_v1();
        let mut hashes = Vec::new();
        for index in 0..2 {
            let step = if inactive {
                state
                    .train_step_gae_auxiliary_v1(
                        &groups,
                        0.5,
                        1.0e-3,
                        &input_v1(&[], Some(2.0), Vec::new()),
                        &mut SWorkV1::default(),
                    )
                    .unwrap()
                    .0
            } else {
                state
                    .train_step_gae_v1(&groups, 0.5, 1.0e-3, &mut SWorkV1::default())
                    .unwrap()
            };
            if index == 0 {
                assert_eq!(
                    (
                        step.loss.loss.to_bits(),
                        step.loss.policy_term.to_bits(),
                        step.loss.value_term.to_bits()
                    ),
                    (LOSS, POLICY, VALUE),
                    "inactive {inactive}"
                );
            }
            hashes.push(step.after_state_sha256);
        }
        assert_eq!(hashes, STATES, "inactive {inactive}");
    }
}

/// No rows: the update is the ordinary one bit for bit, with its receipt.
#[test]
fn an_inactive_term_is_the_ordinary_update() {
    let data = data_v1(11, 5);
    let groups = groups_v1(&data);
    let mut plain = state_v1();
    let plain_step = plain
        .train_step_gae_v1(&groups, 0.5, 1.0e-3, &mut SWorkV1::default())
        .unwrap();
    let mut with = state_v1();
    let (step, result) = with
        .train_step_gae_auxiliary_v1(
            &groups,
            0.5,
            1.0e-3,
            &input_v1(&[], Some(2.0), Vec::new()),
            &mut SWorkV1::default(),
        )
        .unwrap();
    assert_eq!(with, plain);
    assert_eq!(step, plain_step);
    assert_eq!(result.auxiliary_loss, 0.0);
    let cap = result.cap.unwrap();
    assert_eq!(
        (cap["active"].clone(), cap["scale"].clone()),
        (json!(false), json!(1.0))
    );
    assert_eq!(result.adam_steps.unwrap()["difference_l2"], json!(0.0));
    assert_eq!(result.ordinary_only.unwrap(), *plain.model_v1());
    assert!(result.policy_groups.is_none());
}

/// The total loss `GAE (masked) + (c / D) sum KL`: its analytic gradient
/// matches central differences, and the mask changes the policy term's
/// normalization to the unmasked group count.
#[test]
fn combined_gradient_matches_central_differences_under_a_mask() {
    let data = data_v1(13, 4);
    let rows = rows_v1(17, 3);
    // Group 0 (one substep) fully masked; group 1's second substep masked.
    let mask = vec![(0, 0), (1, 1)];
    let model = SModelV1::init_parity_v1(TINY_V1, 5).unwrap();
    let total_of = |values: &[f32]| -> f64 {
        let model = SModelV1::from_values_v1(TINY_V1, values.to_vec()).unwrap();
        let groups = groups_v1(&data);
        let g = model
            .auxiliary_gradients_v1(
                &groups,
                0.5,
                &input_v1(&rows, None, mask.clone()),
                &mut SWorkV1::default(),
            )
            .unwrap();
        f64::from(g.loss.loss) + g.auxiliary_loss
    };
    let groups = groups_v1(&data);
    let g = model
        .auxiliary_gradients_v1(
            &groups,
            0.5,
            &input_v1(&rows, None, mask.clone()),
            &mut SWorkV1::default(),
        )
        .unwrap();
    let mask_set = g.mask.as_ref().unwrap();
    assert_eq!(mask_set.policy_groups, 3);
    assert_eq!(
        g.loss.policy_term.to_bits(),
        (g.loss.policy_sum / 3.0).to_bits()
    );
    assert_eq!(g.divergences.len(), 3);
    let combined: Vec<f32> = g
        .ordinary
        .iter()
        .zip(&g.auxiliary)
        .map(|(a, b)| a + b)
        .collect();
    let layout = model.layout_v1().clone();
    let gauge = layout.gauge_ranges_v1();
    let card_row0 = layout.card..layout.card + TINY_V1.card_dim;
    let mut rng = SplitMixV1::new(7);
    let mut checked = 0;
    for spec in &layout.tensors {
        for _ in 0..6 {
            let index = spec.offset + rng.below(spec.len as u64) as usize;
            if gauge.iter().any(|r| r.contains(&index)) || card_row0.contains(&index) {
                assert_eq!(g.auxiliary[index].to_bits(), 0, "{} gauge", spec.name);
                continue;
            }
            // Central differences at h and h / 2 and their Richardson
            // extrapolation, which cancels the h^2 term: synthetic count
            // features up to 20 make some input weights strongly curved.
            let central = |h: f32| {
                let mut plus = model.values_v1().to_vec();
                plus[index] += h;
                let mut minus = model.values_v1().to_vec();
                minus[index] -= h;
                (total_of(&plus) - total_of(&minus))
                    / (f64::from(plus[index]) - f64::from(minus[index]))
            };
            let (coarse, fine) = (central(4.0e-3), central(2.0e-3));
            let fd = (4.0 * fine - coarse) / 3.0;
            let a = f64::from(combined[index]);
            assert!(
                (fd - a).abs() <= 3.0e-3 + 0.03 * a.abs().max(fd.abs()),
                "{} [{}]: analytic {a}, extrapolated {fd} (central {coarse}, {fine})",
                spec.name,
                index - spec.offset
            );
            checked += 1;
        }
    }
    assert!(checked > 100);
    // A mask with no entries is no mask: the unmasked function's bits.
    let unmasked = model
        .auxiliary_gradients_v1(
            &groups,
            0.5,
            &input_v1(&rows, None, Vec::new()),
            &mut SWorkV1::default(),
        )
        .unwrap();
    let mut plain = vec![0.0f32; layout.total];
    let plain_loss = model
        .gae_loss_gradient_v1(&groups, 0.5, &mut plain, &mut SWorkV1::default())
        .unwrap();
    assert_eq!(unmasked.loss, plain_loss);
    assert!(unmasked
        .ordinary
        .iter()
        .zip(&plain)
        .all(|(a, b)| a.to_bits() == b.to_bits()));
}

/// A binding cap scales the auxiliary gradient to `kappa` times the
/// ordinary norm before the sum; the telemetry's ordinary-only step is the
/// plain update's.
#[test]
fn the_cap_scales_the_auxiliary_gradient_and_telemetry_steps_ordinary_alone() {
    let data = data_v1(19, 4);
    let rows = rows_v1(23, 4);
    let groups = groups_v1(&data);
    let kappa = 1.0e-3;
    let start = state_v1();
    let g = start
        .model_v1()
        .auxiliary_gradients_v1(
            &groups,
            0.5,
            &input_v1(&rows, Some(kappa), Vec::new()),
            &mut SWorkV1::default(),
        )
        .unwrap();
    let mut state = start.clone();
    let (_, result) = state
        .train_step_gae_auxiliary_v1(
            &groups,
            0.5,
            1.0e-3,
            &input_v1(&rows, Some(kappa), Vec::new()),
            &mut SWorkV1::default(),
        )
        .unwrap();
    let cap = result.cap.as_ref().unwrap();
    assert_eq!(cap["binding"], true);
    let scale = cap["scale"].as_f64().unwrap();
    assert!(scale < 1.0);
    assert!((cap["applied_ratio"].as_f64().unwrap() - kappa).abs() <= 1.0e-12);
    let mut expected = start.clone();
    let summed: Vec<f32> = g
        .ordinary
        .iter()
        .zip(&g.auxiliary)
        .map(|(o, a)| o + a * scale as f32)
        .collect();
    expected
        .adam_step_with_gradient_v1(&summed, 1.0e-3)
        .unwrap();
    assert_eq!(state, expected);
    let mut plain = start.clone();
    plain
        .train_step_gae_v1(&groups, 0.5, 1.0e-3, &mut SWorkV1::default())
        .unwrap();
    assert_eq!(result.ordinary_only.as_ref(), Some(plain.model_v1()));
    let steps = result.adam_steps.unwrap();
    assert!(steps["difference_l2"].as_f64().unwrap() > 0.0);
    // Without telemetry or a cap there is no receipt and no second step.
    let mut quiet = start.clone();
    let mut input = input_v1(&rows, None, Vec::new());
    input.step_telemetry = false;
    let (_, result) = quiet
        .train_step_gae_auxiliary_v1(&groups, 0.5, 1.0e-3, &input, &mut SWorkV1::default())
        .unwrap();
    assert!(result.cap.is_none() && result.adam_steps.is_none() && result.ordinary_only.is_none());
}

#[test]
fn malformed_inputs_are_refused() {
    let data = data_v1(29, 3);
    let rows = rows_v1(31, 2);
    let groups = groups_v1(&data);
    let model = SModelV1::init_parity_v1(TINY_V1, 3).unwrap();
    let refused = |input: SAuxiliaryInputV1<'_>| {
        model
            .auxiliary_gradients_v1(&groups, 0.5, &input, &mut SWorkV1::default())
            .err()
            .unwrap()
    };
    assert!(refused(input_v1(&rows, None, vec![(9, 0)])).contains("outside the batch"));
    assert!(refused(input_v1(&rows, None, vec![(0, 1)])).contains("outside the batch"));
    assert!(refused(input_v1(&rows, None, vec![(1, 0), (1, 0)])).contains("repeats"));
    let mut small = input_v1(&rows, None, Vec::new());
    small.denominator = 1;
    assert!(refused(small).contains("denominator"));
    let mut silent = input_v1(&[], None, Vec::new());
    silent.step_telemetry = false;
    assert!(refused(silent).contains("no rows"));
    let mut negative = input_v1(&rows, None, Vec::new());
    negative.coefficient = -1.0;
    assert!(refused(negative).contains("coefficient"));
    assert!(refused(input_v1(&rows, Some(0.0), Vec::new())).contains("cap"));
}
