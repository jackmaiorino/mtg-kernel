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
    let mut with_baseline = groups.clone();
    with_baseline[0].baseline_bits = 1.0f32.to_bits();
    assert!(matches!(
        baseline.train_step_gae_feature_transfer_v3(&with_baseline, &[0.0, 0.0], &[0.0, 0.0], VC, LR),
        Err(NativePolicyTrainErrorV1::BaselineUnsupportedBackend { group_index: 0 })
    ));

    assert_eq!(model.parameter_snapshot_v1(), before.parameter_snapshot_v1());
}
