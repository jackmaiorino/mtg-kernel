use super::*;
use crate::native_flat_tensorizer_v3::{encoded_decision_view_v3, NativeFlatDecisionTensorV3};
use crate::native_policy_train_step_v1::tests::{perturbed_model, real_map_training_tensor_v3};

const VC: f32 = 0.75;
const LR: f32 = 0.00003;

struct Fixture {
    tensors: [NativeFlatDecisionTensorV3; 2],
    logits: [Vec<u32>; 2],
    values: [u32; 2],
}

impl Fixture {
    fn capture(model: &NativePolicyValueNetV1) -> Self {
        let first = real_map_training_tensor_v3();
        let mut second = first.clone();
        second.common.state[0] += 0.125;
        let tensors = [first, second];
        let outputs = tensors.each_ref().map(|tensor| {
            model
                .forward_feature_transfer_v3(encoded_decision_view_v3(tensor))
                .unwrap()
        });
        assert!(outputs.iter().all(|output| output.logits.len() >= 2));
        Self {
            tensors,
            logits: outputs
                .each_ref()
                .map(|output| output.logits.iter().map(|v| v.to_bits()).collect()),
            values: outputs.each_ref().map(|output| output.value.to_bits()),
        }
    }

    fn substeps(&self) -> [Vec<NativePolicySubstepV1<'_>>; 2] {
        let step = |tensor: usize, selected: usize| NativePolicySubstepV1 {
            forward: NativePolicyForwardInputV1::Encoded(Box::new(encoded_decision_view_v3(
                &self.tensors[tensor],
            ))),
            selected_action_index: selected,
            expected_raw_action_logit_bits: self.logits[tensor].as_slice(),
            expected_value_bits: self.values[tensor],
        };
        [
            vec![step(0, 0), step(1, 1)],
            vec![step(1, self.logits[1].len() - 1)],
        ]
    }
}

fn groups<'a>(
    steps: &'a [Vec<NativePolicySubstepV1<'a>>; 2],
) -> [NativePolicyPhysicalDecisionV1<'a>; 2] {
    [
        NativePolicyPhysicalDecisionV1 {
            substeps: &steps[0],
            terminal_return: 1,
            baseline_bits: 0,
        },
        NativePolicyPhysicalDecisionV1 {
            substeps: &steps[1],
            terminal_return: -1,
            baseline_bits: 0,
        },
    ]
}

fn model() -> NativePolicyValueNetV1 {
    NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1()).unwrap()
}

fn state_bits(state: &NativePolicyValueTrainStateV1) -> (u64, u32, Vec<u32>) {
    let mut bits = Vec::new();
    state
        .model
        .visit_parameters_v1(|_, _, values| bits.extend(values.iter().map(|v| v.to_bits())));
    for values in state.first_moments.iter().chain(&state.second_moments) {
        bits.extend(values.iter().map(|v| v.to_bits()));
    }
    (state.adam_step, state.scorer_bias_anchor_bits, bits)
}

#[test]
fn exploration_zero_and_diagnostic_switch_preserve_complete_state() {
    let base = model();
    let fixture = Fixture::capture(&base);
    let steps = fixture.substeps();
    let groups = groups(&steps);
    let targets = [0.3, -0.2];
    let advantages = [0.15, -0.4];
    for beta in [0.0, 0.01] {
        let mut plain = NativePolicyValueTrainStateV1::new_v1(base.clone()).unwrap();
        let mut diagnostic = plain.clone();
        let (a, _) = plain
            .train_step_gae_recovery_v1(
                crate::sideboard_play_policy_v1::FreshLineageGenerationV1::V3,
                &groups,
                &targets,
                &advantages,
                VC,
                LR,
                beta,
                1,
                false,
            )
            .unwrap();
        let (b, report) = diagnostic
            .train_step_gae_recovery_v1(
                crate::sideboard_play_policy_v1::FreshLineageGenerationV1::V3,
                &groups,
                &targets,
                &advantages,
                VC,
                LR,
                beta,
                1,
                true,
            )
            .unwrap();
        assert_eq!(a.gradients, b.gradients);
        assert_eq!(a.scorer_bias_gauge, b.scorer_bias_gauge);
        assert_eq!(a.loss.to_bits(), b.loss.to_bits());
        assert_eq!(state_bits(&plain), state_bits(&diagnostic));
        assert_eq!(report["diagnostics"]["extra_backward_passes"], 2);
        assert_eq!(report["diagnostics"]["blocks"].as_array().unwrap().len(), 3);
        assert!(
            report["diagnostics"]["tanh"]["scorer"]["count"]
                .as_u64()
                .unwrap()
                > 0
        );
        if beta == 0.0 {
            let mut legacy = NativePolicyValueTrainStateV1::new_v1(base.clone()).unwrap();
            let c = legacy
                .train_step_gae_feature_transfer_v3(&groups, &targets, &advantages, VC, LR)
                .unwrap();
            assert_eq!(a.gradients, c.gradients);
            assert_eq!(a.scorer_bias_gauge, c.scorer_bias_gauge);
            assert_eq!(a.loss.to_bits(), c.loss.to_bits());
            assert_eq!(state_bits(&plain), state_bits(&legacy));
            assert_eq!(
                plain.state_sha256_v1().unwrap(),
                legacy.state_sha256_v1().unwrap()
            );
        }
    }
}

#[test]
fn exploration_group_weighting_and_parameter_derivative() {
    let base = model();
    let fixture = Fixture::capture(&base);
    let steps = fixture.substeps();
    let groups = groups(&steps);
    let targets = [0.3, -0.2];
    let advantages = [0.15, -0.4];
    let beta = 0.01;
    let mut state = NativePolicyValueTrainStateV1::new_v1(base.clone()).unwrap();
    let (result, report) = state
        .train_step_gae_recovery_v1(
            crate::sideboard_play_policy_v1::FreshLineageGenerationV1::V3,
            &groups,
            &targets,
            &advantages,
            VC,
            LR,
            beta,
            1,
            false,
        )
        .unwrap();
    let regularizer = |model: &NativePolicyValueNetV1| -> f64 {
        groups
            .iter()
            .flat_map(|g| g.substeps)
            .map(|step| {
                let NativePolicyForwardInputV1::Encoded(view) = &step.forward else {
                    unreachable!()
                };
                exploration_v1::uniform_kl_v1(
                    &model.forward_feature_transfer_v3(**view).unwrap().logits,
                )
            })
            .sum::<f64>()
            / groups.len() as f64
    };
    assert_eq!(report["normalization_physical_groups"], 2.0);
    assert!((report["regularizer_sum"].as_f64().unwrap() - regularizer(&base) * 2.0).abs() < 1e-12);
    let epsilon = 0.002;
    for name in [
        "card_embedding.weight",
        "scorer.2.weight",
        "value_head.2.weight",
    ] {
        let gradient = result.gradients.iter().find(|p| p.name == name).unwrap();
        let (i, &analytic) = gradient
            .values
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .unwrap();
        let positive = perturbed_model(&base, name, i, epsilon);
        let negative = perturbed_model(&base, name, i, -epsilon);
        let loss = |model: &NativePolicyValueNetV1| {
            loss_oracle(model, &groups, &targets, &advantages)
                + f64::from(beta) * regularizer(model)
        };
        let numerical = (loss(&positive) - loss(&negative)) / (2.0 * f64::from(epsilon));
        assert!(
            (numerical - f64::from(analytic)).abs() < 0.003 + 0.03 * f64::from(analytic.abs()),
            "{name}: {numerical} vs {analytic}"
        );
    }
}

/// Each group here is its own single-decision episode. A one-decision
/// episode's GAE advantage/value_target reduce to today's v3 formula for
/// *any* gamma/lambda (`bootstrapped_advantage_v1`'s own goldens prove the
/// general case; this integration test proves the wiring around it): with
/// `value_targets[g] = terminal_return[g]` and `advantages[g] =
/// terminal_return[g] - value[g]`, `train_step_gae_feature_transfer_v3` must
/// reproduce `train_step_feature_transfer_v3` exactly, group by group,
/// gradient by gradient, state bit by state bit, across repeated updates.
#[test]
fn phase1_gae_single_decision_episodes_reproduce_legacy_v3_gradients_and_state() {
    let mut legacy = NativePolicyValueTrainStateV1::new_v1(model()).unwrap();
    let mut gae = legacy.clone();
    for _ in 0..2 {
        let fixture = Fixture::capture(legacy.model_v1());
        let steps = fixture.substeps();
        let groups = groups(&steps);
        let value_targets: Vec<f32> = groups
            .iter()
            .map(|group| f32::from(group.terminal_return))
            .collect();
        let advantages: Vec<f32> = groups
            .iter()
            .zip(&value_targets)
            .map(|(group, target)| target - f32::from_bits(group.substeps[0].expected_value_bits))
            .collect();
        let a = legacy
            .train_step_feature_transfer_v3(&groups, VC, LR)
            .unwrap();
        let b = gae
            .train_step_gae_feature_transfer_v3(&groups, &value_targets, &advantages, VC, LR)
            .unwrap();
        assert_eq!(a.policy_sum.to_bits(), b.policy_sum.to_bits());
        assert_eq!(a.value_sum.to_bits(), b.value_sum.to_bits());
        assert_eq!(a.loss.to_bits(), b.loss.to_bits());
        assert_eq!(a.selected_outputs, b.selected_outputs);
        assert_eq!(a.physical_terms, b.physical_terms);
        assert_eq!(a.gradients, b.gradients);
        assert_eq!(a.scorer_bias_gauge, b.scorer_bias_gauge);
        assert_eq!(state_bits(&legacy), state_bits(&gae));
        assert_eq!(
            legacy.state_sha256_v1().unwrap(),
            gae.state_sha256_v1().unwrap()
        );
    }
    assert_eq!(gae.adam_step_v1(), 2);
}

fn loss_oracle(
    model: &NativePolicyValueNetV1,
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    value_targets: &[f32],
    advantages: &[f32],
) -> f64 {
    // Independent f64 log-softmax/reduction, mirroring `weighted_v3`'s own
    // `loss_oracle`, generalized from a fixed `terminal_return` target to an
    // arbitrary precomputed `value_target`, and divided by `group_count`
    // (this path's own denominator idiom, not a per-group weight).
    let group_count = groups.len() as f64;
    groups
        .iter()
        .zip(value_targets)
        .zip(advantages)
        .map(|((group, &value_target), &advantage)| {
            let mut joint = 0.0f64;
            let mut first_value = None;
            for substep in group.substeps {
                let NativePolicyForwardInputV1::Encoded(view) = &substep.forward else {
                    unreachable!()
                };
                let output = model.forward_feature_transfer_v3(**view).unwrap();
                first_value.get_or_insert(f64::from(output.value));
                let maximum = output
                    .logits
                    .iter()
                    .copied()
                    .map(f64::from)
                    .fold(f64::NEG_INFINITY, f64::max);
                let denominator: f64 = output
                    .logits
                    .iter()
                    .map(|&v| (f64::from(v) - maximum).exp())
                    .sum();
                joint += f64::from(output.logits[substep.selected_action_index])
                    - maximum
                    - denominator.ln();
            }
            let error = first_value.unwrap() - f64::from(value_target);
            (-f64::from(advantage) * joint + f64::from(VC) * error * error) / group_count
        })
        .sum()
}

/// Numeric-derivative (central differences) proof that the hand-written
/// backward in `train_step_gae_with_input_config_v1` matches its own
/// forward loss, for arbitrary (not v3-reducible) `value_targets`/
/// `advantages`, mirroring `weighted_v3`'s own central-difference test.
#[test]
fn phase1_gae_central_differences_freeze_advantage_and_use_first_value_only() {
    let model = model();
    let fixture = Fixture::capture(&model);
    let steps = fixture.substeps();
    let groups = groups(&steps);
    // Arbitrary, not-reducible-to-v3 targets/advantages: proves the general
    // shape, not just the single-decision-episode special case above.
    let value_targets = [0.3f32, -0.2f32];
    let advantages = [0.15f32, -0.4f32];
    let mut state = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
    let result = state
        .train_step_gae_feature_transfer_v3(&groups, &value_targets, &advantages, VC, LR)
        .unwrap();
    let epsilon = 2e-3f32;
    for target in [
        "card_embedding.weight",
        "scorer.2.weight",
        "value_head.2.weight",
        "value_head.2.bias",
    ] {
        let gradient = result.gradients.iter().find(|p| p.name == target).unwrap();
        let (index, analytic) = gradient
            .values
            .iter()
            .copied()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .unwrap();
        assert!(analytic.abs() > 1e-7, "{target} has no derivative witness");
        let positive = perturbed_model(&model, target, index, epsilon);
        let negative = perturbed_model(&model, target, index, -epsilon);
        let numerical = (loss_oracle(&positive, &groups, &value_targets, &advantages)
            - loss_oracle(&negative, &groups, &value_targets, &advantages))
            / (2.0 * f64::from(epsilon));
        let tolerance = 3e-3 + 3e-2 * f64::from(analytic.abs());
        assert!(
            (numerical - f64::from(analytic)).abs() <= tolerance,
            "{target}[{index}]: numerical={numerical}, analytic={analytic}, tolerance={tolerance}"
        );
    }
    let first_error = f32::from_bits(fixture.values[0]) - value_targets[0];
    let second_error = f32::from_bits(fixture.values[1]) - value_targets[1];
    // `value_sum` itself is the raw, undivided accumulator: only `loss`
    // divides by `group_count` (`train_step_gae_with_input_config_v1`'s
    // `loss = (policy_sum + value_coefficient * value_sum) / group_count`).
    assert_eq!(
        result.value_sum.to_bits(),
        (first_error * first_error + second_error * second_error).to_bits()
    );
    assert_eq!(result.physical_terms[0].substep_count, 2);
}

#[test]
fn phase1_gae_invalid_inputs_reject_before_mutation() {
    let model = model();
    let fixture = Fixture::capture(&model);
    let steps = fixture.substeps();
    let groups = groups(&steps);
    let before = model.clone();

    let mut empty_batch = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
    assert!(matches!(
        empty_batch.train_step_gae_feature_transfer_v3(&[], &[], &[], VC, LR),
        Err(NativePolicyTrainErrorV1::EmptyBatch)
    ));

    let mut mismatched = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
    assert!(matches!(
        mismatched.train_step_gae_feature_transfer_v3(&groups, &[0.0], &[0.0, 0.0], VC, LR),
        Err(NativePolicyTrainErrorV1::GaeGroupCountMismatch { .. })
    ));

    let mut non_finite = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
    assert!(matches!(
        non_finite.train_step_gae_feature_transfer_v3(
            &groups,
            &[f32::NAN, 0.0],
            &[0.0, 0.0],
            VC,
            LR
        ),
        Err(NativePolicyTrainErrorV1::NonFinite {
            stage: "gae_value_target",
            ..
        })
    ));

    let mut baseline = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
    let mut with_baseline = groups;
    with_baseline[0].baseline_bits = 1.0f32.to_bits();
    assert!(matches!(
        baseline.train_step_gae_feature_transfer_v3(
            &with_baseline,
            &[0.0, 0.0],
            &[0.0, 0.0],
            VC,
            LR
        ),
        Err(NativePolicyTrainErrorV1::BaselineUnsupportedBackend { group_index: 0 })
    ));

    assert_eq!(
        model.parameter_snapshot_v1(),
        before.parameter_snapshot_v1()
    );
}

fn line_b_target(fixture: &Fixture) -> crate::line_b_teacher_target_v1::LineBSoftmaxV1 {
    let logits: Vec<f64> = fixture.logits[1]
        .iter()
        .map(|bits| f64::from(f32::from_bits(*bits)))
        .collect();
    let returns: Vec<f64> = (0..logits.len()).map(|i| [1.0, 0.0, -1.0][i % 3]).collect();
    crate::line_b_teacher_target_v1::line_b_teacher_target_v1(&logits, &returns, 0.25).unwrap()
}

/// Two selected roots on tensor 1 (group 0 substep 1 and group 1 substep 0);
/// N = 2 whatever their availability.
fn line_b_input(
    direction: crate::line_b_teacher_target_v1::LineBDivergenceV1,
    coefficient: f64,
    targets: [Option<crate::line_b_teacher_target_v1::LineBSoftmaxV1>; 2],
) -> LineBAuxiliaryInputV1 {
    let [first, second] = targets;
    LineBAuxiliaryInputV1 {
        direction,
        coefficient,
        roots: vec![
            LineBAuxiliaryRootV1 {
                group_index: 0,
                substep_index: 1,
                target: first,
            },
            LineBAuxiliaryRootV1 {
                group_index: 1,
                substep_index: 0,
                target: second,
            },
        ],
    }
}

const LINE_B_TARGETS: [f32; 2] = [0.3, -0.2];
const LINE_B_ADVANTAGES: [f32; 2] = [0.15, -0.4];

fn line_b_step(
    model: &NativePolicyValueNetV1,
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    input: &LineBAuxiliaryInputV1,
) -> Result<
    (
        NativePolicyValueTrainStateV1,
        NativePolicyTrainStepResultV1,
        LineBAuxiliaryResultV1,
    ),
    NativePolicyTrainErrorV1,
> {
    let mut state = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
    let config = state.model.feature_transfer_config_v3();
    let (result, auxiliary) = state.train_step_gae_line_b_v1(
        groups,
        &LINE_B_TARGETS,
        &LINE_B_ADVANTAGES,
        VC,
        LR,
        None,
        config,
        input,
    )?;
    Ok((state, result, auxiliary))
}

/// Line (b) with every selected root censored, or with c = 0, is the
/// ordinary GAE update bit for bit (result, gauge record and state).
#[test]
fn line_b_censored_roots_and_zero_coefficient_leave_the_gae_update_unchanged() {
    use crate::line_b_teacher_target_v1::LineBDivergenceV1;
    let model = model();
    let fixture = Fixture::capture(&model);
    let steps = fixture.substeps();
    let groups = groups(&steps);
    let mut ordinary = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
    let expected = ordinary
        .train_step_gae_feature_transfer_v3(&groups, &LINE_B_TARGETS, &LINE_B_ADVANTAGES, VC, LR)
        .unwrap();
    for input in [
        line_b_input(LineBDivergenceV1::Reverse, 0.1, [None, None]),
        line_b_input(
            LineBDivergenceV1::Forward,
            0.0,
            [Some(line_b_target(&fixture)), None],
        ),
    ] {
        let (state, result, auxiliary) = line_b_step(&model, &groups, &input).unwrap();
        assert_eq!(result.gradients, expected.gradients);
        assert_eq!(result.scorer_bias_gauge, expected.scorer_bias_gauge);
        assert_eq!(state_bits(&state), state_bits(&ordinary));
        assert_eq!(auxiliary.auxiliary_head_l2, 0.0);
        assert_eq!(auxiliary.auxiliary_loss, 0.0);
        assert!(auxiliary.ordinary_head_l2 > 0.0);
    }
}

/// The auxiliary gradient (combined minus ordinary) matches central
/// differences of (c/N) KL computed from the model's own forward, for both
/// directions, through the scorer, the trunk and the embeddings; a censored
/// root stays in N; the gauge passes with the auxiliary term present.
#[test]
fn line_b_auxiliary_gradients_match_central_differences_in_both_directions() {
    use crate::line_b_teacher_target_v1::{line_b_divergence_v1, LineBDivergenceV1};
    let model = model();
    let fixture = Fixture::capture(&model);
    let steps = fixture.substeps();
    let groups = groups(&steps);
    let target = line_b_target(&fixture);
    let mut ordinary = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
    let base = ordinary
        .train_step_gae_feature_transfer_v3(&groups, &LINE_B_TARGETS, &LINE_B_ADVANTAGES, VC, LR)
        .unwrap();
    let mut norms = Vec::new();
    for direction in [LineBDivergenceV1::Reverse, LineBDivergenceV1::Forward] {
        let input = line_b_input(direction, 1.0, [Some(target.clone()), None]);
        let (_, result, auxiliary) = line_b_step(&model, &groups, &input).unwrap();
        assert!(auxiliary.divergences_before[0].is_some());
        assert!(auxiliary.divergences_before[1].is_none());
        assert!(auxiliary.auxiliary_head_l2 > 0.0);
        norms.push(auxiliary.auxiliary_head_l2);
        let oracle = |model: &NativePolicyValueNetV1| {
            let NativePolicyForwardInputV1::Encoded(view) = &groups[0].substeps[1].forward else {
                unreachable!()
            };
            let output = model.forward_feature_transfer_v3(**view).unwrap();
            let logits: Vec<f64> = output.logits.iter().map(|&v| f64::from(v)).collect();
            // c / N with c = 1 and N = 2 (the censored root counts).
            0.5 * line_b_divergence_v1(direction, &logits, &target)
                .unwrap()
                .divergence
        };
        let before = oracle(&model);
        assert_eq!(auxiliary.auxiliary_loss.to_bits(), before.to_bits());
        let epsilon = 2e-3f32;
        for name in [
            "scorer.2.weight",
            "scorer.0.weight",
            "card_embedding.weight",
        ] {
            let combined = result.gradients.iter().find(|p| p.name == name).unwrap();
            let plain = base.gradients.iter().find(|p| p.name == name).unwrap();
            let extra: Vec<f32> = combined
                .values
                .iter()
                .zip(&plain.values)
                .map(|(a, b)| a - b)
                .collect();
            let (index, analytic) = extra
                .iter()
                .copied()
                .enumerate()
                .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
                .unwrap();
            assert!(
                analytic.abs() > 1e-7,
                "{name} has no auxiliary derivative witness"
            );
            let numerical = (oracle(&perturbed_model(&model, name, index, epsilon))
                - oracle(&perturbed_model(&model, name, index, -epsilon)))
                / (2.0 * f64::from(epsilon));
            let tolerance = 3e-3 + 3e-2 * f64::from(analytic.abs());
            assert!(
                (numerical - f64::from(analytic)).abs() <= tolerance,
                "{direction:?} {name}[{index}]: numerical={numerical}, analytic={analytic}"
            );
        }
        assert!(auxiliary.auxiliary_bias_residual.abs() < 1e-5);
        assert!(
            result.scorer_bias_gauge.substep_bounds.len()
                > base.scorer_bias_gauge.substep_bounds.len()
        );
    }
    assert_ne!(norms[0], norms[1], "the two directions must differ");
}

#[test]
fn line_b_invalid_auxiliary_inputs_reject_before_mutation() {
    use crate::line_b_teacher_target_v1::LineBDivergenceV1;
    let model = model();
    let fixture = Fixture::capture(&model);
    let steps = fixture.substeps();
    let groups = groups(&steps);
    let target = line_b_target(&fixture);
    let wide = {
        let mut logits: Vec<f64> = fixture.logits[1]
            .iter()
            .map(|bits| f64::from(f32::from_bits(*bits)))
            .collect();
        logits.push(0.0);
        let returns = vec![0.0; logits.len()];
        crate::line_b_teacher_target_v1::line_b_teacher_target_v1(&logits, &returns, 0.25).unwrap()
    };
    let mut duplicate = line_b_input(
        LineBDivergenceV1::Reverse,
        0.1,
        [Some(target.clone()), None],
    );
    duplicate.roots[1].group_index = 0;
    duplicate.roots[1].substep_index = 1;
    let mut outside = line_b_input(LineBDivergenceV1::Reverse, 0.1, [None, None]);
    outside.roots[1].group_index = 7;
    let cases = [
        (
            LineBAuxiliaryInputV1 {
                direction: LineBDivergenceV1::Reverse,
                coefficient: 0.1,
                roots: Vec::new(),
            },
            "line-b-auxiliary-no-roots",
        ),
        (
            line_b_input(LineBDivergenceV1::Reverse, -0.1, [None, None]),
            "line-b-auxiliary-coefficient",
        ),
        (
            line_b_input(LineBDivergenceV1::Reverse, f64::NAN, [None, None]),
            "line-b-auxiliary-coefficient",
        ),
        (duplicate, "line-b-auxiliary-duplicate-root"),
        (outside, "line-b-auxiliary-root-outside-batch"),
        (
            line_b_input(LineBDivergenceV1::Forward, 0.1, [Some(wide), None]),
            "line-b-auxiliary-divergence",
        ),
    ];
    let initial = state_bits(&NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap());
    for (input, code) in cases {
        let mut state = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
        let config = state.model.feature_transfer_config_v3();
        let error = state
            .train_step_gae_line_b_v1(
                &groups,
                &LINE_B_TARGETS,
                &LINE_B_ADVANTAGES,
                VC,
                LR,
                None,
                config,
                &input,
            )
            .unwrap_err();
        assert_eq!(error, NativePolicyTrainErrorV1::LineBAuxiliary { code });
        assert_eq!(state_bits(&state), initial, "{code}");
    }
}

/// CODEX 11:23 item 6, the permuted-target control (offline): available
/// roots of equal menu width rotate their targets across roots, a width with
/// one available root rotates its own action coordinates, censored roots stay
/// censored, and on a nondegenerate fixture (two roots with different logits
/// and different targets) the auxiliary gradient responds to target identity.
#[test]
fn line_b_permuted_targets_rotate_and_move_the_auxiliary_gradient() {
    use crate::line_b_teacher_target_v1::{line_b_teacher_target_v1, LineBDivergenceV1};
    let model = model();
    let fixture = Fixture::capture(&model);
    let steps = fixture.substeps();
    let groups = groups(&steps);
    let target = |tensor: usize, pattern: [f64; 3]| {
        let logits: Vec<f64> = fixture.logits[tensor]
            .iter()
            .map(|bits| f64::from(f32::from_bits(*bits)))
            .collect();
        let returns: Vec<f64> = (0..logits.len()).map(|i| pattern[i % 3]).collect();
        line_b_teacher_target_v1(&logits, &returns, 0.25).unwrap()
    };
    let (a, b) = (target(0, [1.0, 0.0, -1.0]), target(1, [-1.0, 1.0, 0.0]));
    assert_eq!(a.log_probabilities.len(), b.log_probabilities.len());
    let root = |group_index: usize, target| LineBAuxiliaryRootV1 {
        group_index,
        substep_index: 0,
        target,
    };
    let input = LineBAuxiliaryInputV1 {
        direction: LineBDivergenceV1::Reverse,
        coefficient: 1.0,
        roots: vec![root(0, Some(a.clone())), root(1, Some(b.clone()))],
    };
    let (permuted, kinds) = line_b_permuted_input_v1(&input);
    assert_eq!(kinds, vec![Some(LineBPermutationKindV1::AcrossRoot); 2]);
    assert_eq!(permuted.roots[0].target.as_ref(), Some(&b));
    assert_eq!(permuted.roots[1].target.as_ref(), Some(&a));
    let single = LineBAuxiliaryInputV1 {
        direction: LineBDivergenceV1::Forward,
        coefficient: 1.0,
        roots: vec![root(0, Some(a.clone())), root(1, None)],
    };
    let (within, kinds) = line_b_permuted_input_v1(&single);
    assert_eq!(kinds, vec![Some(LineBPermutationKindV1::WithinRoot), None]);
    assert!(within.roots[1].target.is_none());
    let rotated = within.roots[0].target.as_ref().unwrap();
    let width = a.log_probabilities.len();
    for action in 0..width {
        let source = (action + 1) % width;
        assert_eq!(
            rotated.log_probabilities[action].to_bits(),
            a.log_probabilities[source].to_bits()
        );
        assert_eq!(
            rotated.probabilities[action].to_bits(),
            a.probabilities[source].to_bits()
        );
    }
    let gradients = |input: &LineBAuxiliaryInputV1| {
        let (_, result, auxiliary) = line_b_step(&model, &groups, input).unwrap();
        (result.gradients, auxiliary.auxiliary_loss)
    };
    let (original, original_loss) = gradients(&input);
    let (control, control_loss) = gradients(&permuted);
    assert_ne!(original_loss.to_bits(), control_loss.to_bits());
    assert_ne!(
        original, control,
        "the auxiliary term must respond to target identity"
    );
    let (_, within_loss) = gradients(&within);
    let (_, single_loss) = gradients(&single);
    assert_ne!(within_loss.to_bits(), single_loss.to_bits());
}
