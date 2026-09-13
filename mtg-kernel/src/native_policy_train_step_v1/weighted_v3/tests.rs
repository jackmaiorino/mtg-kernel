use super::*;
use crate::native_flat_tensorizer_v3::{encoded_decision_view_v3, NativeFlatDecisionTensorV3};
use crate::native_policy_train_step_v1::tests::{perturbed_model, real_map_training_tensor_v3};
use crate::native_policy_value_net_v1::NativeEncodedDecisionSchemaV1;

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
        // A numeric derivative fixture with distinct first/later values.
        // Shape/schema originate in a real Map choice; this is not a match.
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
    // Works for deliberately invalid state too, without using validating hash.
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
fn phase1_weighted_v3_uniform_power_two_weights_preserve_legacy_gradients_and_state() {
    let mut legacy = NativePolicyValueTrainStateV1::new_v1(model()).unwrap();
    let mut weighted = legacy.clone();
    for _ in 0..2 {
        let fixture = Fixture::capture(legacy.model_v1());
        let steps = fixture.substeps();
        let groups = groups(&steps);
        let a = legacy
            .train_step_feature_transfer_v3(&groups, VC, LR)
            .unwrap();
        let b = weighted
            .train_step_weighted_feature_transfer_v3(&groups, &[0.5, 0.5], VC, LR)
            .unwrap();
        assert_eq!((a.policy_sum / 2.0).to_bits(), b.policy_sum.to_bits());
        assert_eq!((a.value_sum / 2.0).to_bits(), b.value_sum.to_bits());
        assert_eq!(a.loss.to_bits(), b.loss.to_bits());
        assert_eq!(a.selected_outputs, b.selected_outputs);
        assert_eq!(a.physical_terms, b.physical_terms);
        assert_eq!(a.gradients, b.gradients);
        assert_eq!(a.scorer_bias_gauge, b.scorer_bias_gauge);
        assert_eq!(state_bits(&legacy), state_bits(&weighted));
        assert_eq!(
            legacy.state_sha256_v1().unwrap(),
            weighted.state_sha256_v1().unwrap()
        );
    }
    assert_eq!(legacy.adam_step_v1(), 2);
    assert!(legacy
        .first_moments
        .iter()
        .flatten()
        .any(|value| *value != 0.0));
}

#[test]
fn phase1_weighted_v3_unequal_weights_scale_gradients_without_renormalization() {
    let model = model();
    let fixture = Fixture::capture(&model);
    let steps = fixture.substeps();
    let groups = groups(&steps);
    let mut state = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
    let weights = [0.125, 0.375]; // Sum deliberately 0.5, not a probability vector.
    let actual = state
        .train_step_weighted_feature_transfer_v3(&groups, &weights, VC, LR)
        .unwrap();
    let singles: Vec<_> = groups
        .iter()
        .map(|group| {
            NativePolicyValueTrainStateV1::new_v1(model.clone())
                .unwrap()
                .train_step_weighted_feature_transfer_v3(
                    std::slice::from_ref(group),
                    &[1.0],
                    VC,
                    LR,
                )
                .unwrap()
        })
        .collect();
    assert_eq!(
        actual.policy_sum.to_bits(),
        (weights[0] * singles[0].policy_sum + weights[1] * singles[1].policy_sum).to_bits()
    );
    assert_eq!(
        actual.value_sum.to_bits(),
        (weights[0] * singles[0].value_sum + weights[1] * singles[1].value_sum).to_bits()
    );
    let mut nonzero = 0;
    for (parameter_index, parameter) in actual.gradients.iter().enumerate() {
        for (index, &gradient) in parameter.values.iter().enumerate() {
            let expected = weights[0] * singles[0].gradients[parameter_index].values[index]
                + weights[1] * singles[1].gradients[parameter_index].values[index];
            let tolerance = 2e-6 + 4e-5 * expected.abs();
            assert!(
                (gradient - expected).abs() <= tolerance,
                "{}[{index}]: {gradient} vs {expected}",
                parameter.name
            );
            nonzero += usize::from(gradient.abs() > 1e-7);
        }
    }
    assert!(nonzero > 100);
}

fn frozen_advantages(
    model: &NativePolicyValueNetV1,
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
) -> Vec<f32> {
    groups
        .iter()
        .map(|group| {
            let NativePolicyForwardInputV1::Encoded(view) = &group.substeps[0].forward else {
                unreachable!()
            };
            f32::from(group.terminal_return)
                - model.forward_feature_transfer_v3(**view).unwrap().value
        })
        .collect()
}

fn loss_oracle(
    model: &NativePolicyValueNetV1,
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    weights: &[f32],
    advantages: &[f32],
) -> f64 {
    // Independent f64 log-softmax/reduction. Keep the policy baseline frozen
    // while differentiating the ordinary first-substep value regression.
    groups
        .iter()
        .zip(weights)
        .zip(advantages)
        .map(|((group, &weight), &advantage)| {
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
            let error = first_value.unwrap() - f64::from(group.terminal_return);
            f64::from(weight) * (-f64::from(advantage) * joint + f64::from(VC) * error * error)
        })
        .sum()
}

#[test]
fn phase1_weighted_v3_central_differences_freeze_advantage_and_use_first_value_only() {
    let model = model();
    let fixture = Fixture::capture(&model);
    let steps = fixture.substeps();
    let groups = groups(&steps);
    let weights = [0.2, 0.7];
    let advantages = frozen_advantages(&model, &groups);
    let mut state = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
    let result = state
        .train_step_weighted_feature_transfer_v3(&groups, &weights, VC, LR)
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
        let numerical = (loss_oracle(&positive, &groups, &weights, &advantages)
            - loss_oracle(&negative, &groups, &weights, &advantages))
            / (2.0 * f64::from(epsilon));
        let tolerance = 3e-3 + 3e-2 * f64::from(analytic.abs());
        assert!(
            (numerical - f64::from(analytic)).abs() <= tolerance,
            "{target}[{index}]: numerical={numerical}, analytic={analytic}, tolerance={tolerance}"
        );
    }
    let first_error = f32::from_bits(fixture.values[0]) - 1.0;
    let second_error = f32::from_bits(fixture.values[1]) + 1.0;
    assert_eq!(
        result.value_sum.to_bits(),
        (weights[0] * (first_error * first_error) + weights[1] * (second_error * second_error))
            .to_bits()
    );
    assert_eq!(result.physical_terms[0].substep_count, 2);
}

#[test]
fn phase1_weighted_v3_invalid_weights_bounds_and_state_reject_before_mutation() {
    let mut state = NativePolicyValueTrainStateV1::new_v1(model()).unwrap();
    let fixture = Fixture::capture(state.model_v1());
    let steps = fixture.substeps();
    let groups = groups(&steps);
    let before = state_bits(&state);
    for bad in [0.0, -0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let calls = forward_with_tape_call_count_for_test_v1();
        assert!(matches!(
            state.train_step_weighted_feature_transfer_v3(&groups, &[0.5, bad], VC, LR),
            Err(NativePolicyTrainErrorV1::InvalidPhysicalGroupWeight { group_index: 1, .. })
        ));
        assert_eq!(forward_with_tape_call_count_for_test_v1(), calls);
        assert_eq!(state_bits(&state), before);
    }
    for weights in [&[][..], &[1.0][..], &[1.0, 1.0, 1.0][..]] {
        assert!(matches!(
            state.train_step_weighted_feature_transfer_v3(&groups, weights, VC, LR),
            Err(NativePolicyTrainErrorV1::WeightedGroupCountMismatch { .. })
        ));
        assert_eq!(state_bits(&state), before);
    }
    assert!(matches!(
        state.train_step_weighted_feature_transfer_v3(&[], &[], VC, LR),
        Err(NativePolicyTrainErrorV1::EmptyBatch)
    ));
    for count in [MAX_GROUPS + 1, MAX_SUBSTEPS / 2 + 1] {
        let many = vec![groups[0]; count];
        let weights = vec![1.0; count];
        let calls = forward_with_tape_call_count_for_test_v1();
        assert!(matches!(
            state.train_step_weighted_feature_transfer_v3(&many, &weights, VC, LR),
            Err(NativePolicyTrainErrorV1::WeightedBatchLimit { .. })
        ));
        assert_eq!(forward_with_tape_call_count_for_test_v1(), calls);
        assert_eq!(state_bits(&state), before);
    }
    for kind in 0..3 {
        let mut invalid = state.clone();
        match kind {
            0 => invalid.first_moments[0][CARD_EMBEDDING_DIM_V1] = f32::NAN,
            1 => invalid.scorer_bias_anchor_bits ^= 1,
            _ => invalid.adam_step = u64::MAX,
        }
        let before = state_bits(&invalid);
        assert!(invalid
            .train_step_weighted_feature_transfer_v3(&groups, &[0.5, 0.5], VC, LR)
            .is_err());
        assert_eq!(state_bits(&invalid), before);
    }
}

#[test]
fn phase1_weighted_v3_replay_schema_and_late_errors_are_atomic() {
    let mut state = NativePolicyValueTrainStateV1::new_v1(model()).unwrap();
    let fixture = Fixture::capture(state.model_v1());
    let before = state_bits(&state);
    for kind in 0..8 {
        let mut logits = fixture.logits[1].clone();
        let mut value = fixture.values[1];
        let mut view = encoded_decision_view_v3(&fixture.tensors[1]);
        let mut selected = 0;
        let mut baseline = 0;
        match kind {
            0 => logits[1] ^= 1,
            1 => value ^= 1,
            2 => view.schema = NativeEncodedDecisionSchemaV1::contract_v1(),
            3 => view.state = &view.state[..view.state.len() - 1],
            4 => selected = logits.len(),
            5 => baseline = 1,
            _ => (),
        }
        let first = fixture.substeps();
        let last = [NativePolicySubstepV1 {
            forward: NativePolicyForwardInputV1::Encoded(Box::new(view)),
            selected_action_index: selected,
            expected_raw_action_logit_bits: &logits,
            expected_value_bits: value,
        }];
        let groups = [
            NativePolicyPhysicalDecisionV1 {
                substeps: &first[0],
                terminal_return: 1,
                baseline_bits: 0,
            },
            NativePolicyPhysicalDecisionV1 {
                substeps: &last,
                terminal_return: -1,
                baseline_bits: baseline,
            },
        ];
        let (vc, lr) = match kind {
            6 => (f32::NAN, LR),
            7 => (VC, 0.0),
            _ => (VC, LR),
        };
        assert!(
            state
                .train_step_weighted_feature_transfer_v3(&groups, &[0.2, 0.7], vc, lr)
                .is_err(),
            "case {kind}"
        );
        assert_eq!(state_bits(&state), before, "case {kind}");
    }
    let mut builder = NativePolicyPackedForwardBuilderV1::from_model_v1(state.model_v1()).unwrap();
    builder.config = state.model_v1().feature_transfer_config_v3();
    let tape = builder
        .forward_v1(encoded_decision_view_v3(&fixture.tensors[0]))
        .unwrap();
    let packed = [NativePolicySubstepV1 {
        forward: NativePolicyForwardInputV1::Packed {
            encoded: Box::new(encoded_decision_view_v3(&fixture.tensors[0])),
            tape: &tape,
        },
        selected_action_index: 0,
        expected_raw_action_logit_bits: &fixture.logits[0],
        expected_value_bits: fixture.values[0],
    }];
    let groups = [NativePolicyPhysicalDecisionV1 {
        substeps: &packed,
        terminal_return: 1,
        baseline_bits: 0,
    }];
    assert!(matches!(
        state.train_step_weighted_feature_transfer_v3(&groups, &[1.0], VC, LR),
        Err(NativePolicyTrainErrorV1::FeatureTransferRequiresCanonicalInput { .. })
    ));
    assert_eq!(state_bits(&state), before);
}

#[test]
fn phase1_weighted_v3_full_snapshot_resume_matches_next_update_and_rejects_stale_rows() {
    let initial = model();
    let mut uninterrupted = NativePolicyValueTrainStateV1::new_v1(initial.clone()).unwrap();
    let first = Fixture::capture(&initial);
    let first_steps = first.substeps();
    let first_groups = groups(&first_steps);
    uninterrupted
        .train_step_weighted_feature_transfer_v3(&first_groups, &[0.125, 0.875], VC, LR)
        .unwrap();
    let snapshot = uninterrupted.snapshot_v1().unwrap();
    assert!(snapshot
        .first_moments
        .iter()
        .flat_map(|p| &p.values)
        .any(|v| *v != 0.0));
    let mut restored =
        NativePolicyValueTrainStateV1::from_snapshot_v1(initial.clone(), &snapshot).unwrap();
    assert_eq!(state_bits(&restored), state_bits(&uninterrupted));
    let before = state_bits(&restored);
    assert!(restored
        .train_step_weighted_feature_transfer_v3(&first_groups, &[0.125, 0.875], VC, LR)
        .is_err());
    assert_eq!(state_bits(&restored), before);
    let next = Fixture::capture(uninterrupted.model_v1());
    let next_steps = next.substeps();
    let next_groups = groups(&next_steps);
    let a = uninterrupted
        .train_step_weighted_feature_transfer_v3(&next_groups, &[0.3, 0.7], VC, LR)
        .unwrap();
    let b = restored
        .train_step_weighted_feature_transfer_v3(&next_groups, &[0.3, 0.7], VC, LR)
        .unwrap();
    assert_eq!(a, b);
    assert_eq!(uninterrupted.adam_step_v1(), 2);
    assert_eq!(state_bits(&uninterrupted), state_bits(&restored));
    assert_eq!(
        uninterrupted.state_sha256_v1().unwrap(),
        restored.state_sha256_v1().unwrap()
    );
    assert!(restored.first_moments[SCORER_SECOND_BIAS]
        .iter()
        .all(|v| v.to_bits() == 0));
    assert!(restored.second_moments[SCORER_SECOND_BIAS]
        .iter()
        .all(|v| v.to_bits() == 0));
    assert!(
        restored.model.parameter_snapshot_v1()[CARD_EMBEDDING].values[..CARD_EMBEDDING_DIM_V1]
            .iter()
            .all(|v| v.to_bits() == 0)
    );
    assert_eq!(
        initial.parameter_snapshot_v1(),
        model().parameter_snapshot_v1()
    );
}
