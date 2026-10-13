use super::*;
use crate::line_b_teacher_target_v1::{line_b_softmax_v1, line_b_teacher_target_v1};
use crate::native_flat_tensorizer_v3::{encoded_decision_view_v3, NativeFlatDecisionTensorV3};
use crate::native_policy_train_step_v1::line_b_auxiliary_v1::line_b_auxiliary_terms_v1;
use crate::native_policy_train_step_v1::tests::{perturbed_model, real_map_training_tensor_v3};

const LR: f32 = 1e-4;

fn model() -> NativePolicyValueNetV1 {
    NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1()).unwrap()
}

/// Two distinct real decisions (the Map fixture and a state-shifted copy).
fn tensors() -> [NativeFlatDecisionTensorV3; 2] {
    let first = real_map_training_tensor_v3();
    let mut second = first.clone();
    second.common.state[0] += 0.125;
    [first, second]
}

fn logits(model: &NativePolicyValueNetV1, tensor: &NativeFlatDecisionTensorV3) -> Vec<f64> {
    let output = model
        .forward_feature_transfer_v3(encoded_decision_view_v3(tensor))
        .unwrap();
    output.logits.iter().map(|&v| f64::from(v)).collect()
}

fn parent_softmax(
    model: &NativePolicyValueNetV1,
    tensor: &NativeFlatDecisionTensorV3,
) -> LineBSoftmaxV1 {
    line_b_softmax_v1(&logits(model, tensor)).unwrap()
}

/// A one-hot target at the parent's lowest-probability action, and a soft
/// line (b) teacher target: both far from the parent.
fn targets(
    model: &NativePolicyValueNetV1,
    tensors: &[NativeFlatDecisionTensorV3; 2],
) -> [LineBSoftmaxV1; 2] {
    let first = logits(model, &tensors[0]);
    let worst = first
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(b.1))
        .unwrap()
        .0;
    let second = logits(model, &tensors[1]);
    let returns: Vec<f64> = (0..second.len()).map(|i| [1.0, 0.0, -1.0][i % 3]).collect();
    [
        one_hot_target_v1(first.len(), worst).unwrap(),
        line_b_teacher_target_v1(&second, &returns, 0.25).unwrap(),
    ]
}

fn external<'a>(
    tensors: &'a [NativeFlatDecisionTensorV3; 2],
    targets: &'a [LineBSoftmaxV1; 2],
    coefficient: f64,
    denominator: usize,
) -> ExternalAuxiliaryInputV1<'a> {
    ExternalAuxiliaryInputV1 {
        direction: LineBDivergenceV1::Forward,
        coefficient,
        denominator,
        rows: tensors
            .iter()
            .zip(targets)
            .map(|(tensor, target)| ExternalAuxiliaryRowV1 {
                encoded: encoded_decision_view_v3(tensor),
                target,
            })
            .collect(),
    }
}

fn parent_targets(
    model: &NativePolicyValueNetV1,
    tensors: &[NativeFlatDecisionTensorV3; 2],
) -> [LineBSoftmaxV1; 2] {
    [
        parent_softmax(model, &tensors[0]),
        parent_softmax(model, &tensors[1]),
    ]
}

/// Combines one term into zero gradients with a fresh gauge, as the step does.
fn combined(
    terms: LineBAuxiliaryTermsV1,
    parameters: &[NativeNamedParameterV1],
) -> (
    Vec<Vec<u32>>,
    LineBAuxiliaryResultV1,
    NativeScorerBiasGaugeRecordV1,
) {
    let mut gradients: Vec<Vec<f32>> = parameters
        .iter()
        .map(|parameter| vec![0.0; parameter.values.len()])
        .collect();
    let mut gauge = ScorerBiasGaugeAccumulatorV1::default();
    let result = terms
        .combine_v1(parameters, &mut gradients, &mut gauge)
        .unwrap();
    let record = gauge
        .finish(
            gradients[SCORER_SECOND_BIAS][0],
            parameters[SCORER_SECOND_BIAS].values[0].to_bits(),
        )
        .unwrap();
    let bits = gradients
        .iter()
        .map(|values| values.iter().map(|v| v.to_bits()).collect())
        .collect();
    (bits, result, record)
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

/// Off-batch rows build the same gradient, telemetry and gauge record as the
/// same rows entered as in-batch line (b) roots, bit for bit, including a
/// denominator above the row count (the line (b) scheduled count).
#[test]
fn off_batch_rows_reproduce_in_batch_line_b_roots_bit_for_bit() {
    let model = model();
    let tensors = tensors();
    let parameters = model.parameter_snapshot_v1();
    let config = model.feature_transfer_config_v3();
    let [first, second] = targets(&model, &tensors);
    let group_tapes: Vec<GroupTapeV1<'_>> = tensors
        .iter()
        .map(|tensor| GroupTapeV1 {
            tapes: vec![SelectedDecisionTapeV1 {
                tape: DecisionTapeSourceV1::Owned(Box::new(
                    forward_with_tape(&parameters, config, encoded_decision_view_v3(tensor))
                        .unwrap(),
                )),
                selected_action_index: 0,
                log_probabilities: Vec::new(),
            }],
            advantage: 0.0,
            value_error: 0.0,
        })
        .collect();
    let in_batch = LineBAuxiliaryInputV1 {
        scheduled_count: Some(3),
        direction: LineBDivergenceV1::Forward,
        coefficient: 0.7,
        roots: vec![
            LineBAuxiliaryRootV1 {
                group_index: 0,
                substep_index: 0,
                target: Some(first.clone()),
            },
            LineBAuxiliaryRootV1 {
                group_index: 1,
                substep_index: 0,
                target: Some(second.clone()),
            },
        ],
    };
    let reference = combined(
        line_b_auxiliary_terms_v1(&in_batch, &group_tapes, &parameters).unwrap(),
        &parameters,
    );
    let pair = [first, second];
    let off_batch = combined(
        external_auxiliary_terms_v1(&external(&tensors, &pair, 0.7, 3), &parameters, config)
            .unwrap(),
        &parameters,
    );
    assert_eq!(off_batch.0, reference.0);
    assert_eq!(off_batch.1, reference.1);
    assert_eq!(off_batch.2, reference.2);
    assert!(off_batch.1.auxiliary_loss > 0.0);
    assert!(off_batch.1.auxiliary_head_l2 > 0.0);
}

/// The anchor at the parent: every row's divergence and gradient are exactly
/// zero, so a step leaves every parameter bit unchanged and the moments zero.
/// This is why the parent is phase 1's exact control.
#[test]
fn anchor_at_the_parent_is_an_exact_no_op_step() {
    let model = model();
    let tensors = tensors();
    let mut state = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
    let before = state_bits(&state);
    let parents = parent_targets(&model, &tensors);
    let anchor = external(&tensors, &parents, 1.0, 2);
    let (results, gauge) = state
        .train_step_auxiliary_only_v1(FreshLineageGenerationV1::V3, LR, &[&anchor])
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].divergences_before, vec![Some(0.0), Some(0.0)]);
    assert_eq!(results[0].auxiliary_loss.to_bits(), 0.0_f64.to_bits());
    assert_eq!(results[0].auxiliary_head_l2, 0.0);
    assert_eq!(gauge.raw_gradient_residual, 0.0);
    let after = state_bits(&state);
    assert_eq!(after.0, 1);
    assert_eq!(after.1, before.1);
    let parameter_count: usize = model
        .parameter_snapshot_v1()
        .iter()
        .map(|parameter| parameter.values.len())
        .sum();
    assert_eq!(after.2[..parameter_count], before.2[..parameter_count]);
    assert!(after.2[parameter_count..].iter().all(|&bits| bits == 0));
}

/// The archive term's gradient matches central differences of
/// `(c / D) * sum KL(q || p)` from the model's own forward, through the
/// scorer, the trunk and the embeddings, and the value head gets no gradient.
#[test]
fn archive_term_gradients_match_central_differences_and_skip_the_value_head() {
    let model = model();
    let tensors = tensors();
    let parameters = model.parameter_snapshot_v1();
    let config = model.feature_transfer_config_v3();
    let targets = targets(&model, &tensors);
    let input = external(&tensors, &targets, 1.0, 2);
    let (gradients, result, _) = combined(
        external_auxiliary_terms_v1(&input, &parameters, config).unwrap(),
        &parameters,
    );
    let oracle = |model: &NativePolicyValueNetV1| {
        tensors
            .iter()
            .zip(&targets)
            .map(|(tensor, target)| {
                0.5 * line_b_divergence_v1(
                    LineBDivergenceV1::Forward,
                    &logits(model, tensor),
                    target,
                )
                .unwrap()
                .divergence
            })
            .sum::<f64>()
    };
    assert_eq!(result.auxiliary_loss.to_bits(), oracle(&model).to_bits());
    let epsilon = 2e-3f32;
    for name in [
        "scorer.2.weight",
        "scorer.0.weight",
        "card_embedding.weight",
    ] {
        let position = parameters.iter().position(|p| p.name == name).unwrap();
        let (index, analytic) = gradients[position]
            .iter()
            .map(|&bits| f32::from_bits(bits))
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .unwrap();
        assert!(analytic.abs() > 1e-7, "{name} has no derivative witness");
        let numerical = (oracle(&perturbed_model(&model, name, index, epsilon))
            - oracle(&perturbed_model(&model, name, index, -epsilon)))
            / (2.0 * f64::from(epsilon));
        let tolerance = 3e-3 + 3e-2 * f64::from(analytic.abs());
        assert!(
            (numerical - f64::from(analytic)).abs() <= tolerance,
            "{name}[{index}]: numerical={numerical}, analytic={analytic}"
        );
    }
    for (parameter, values) in parameters.iter().zip(&gradients) {
        if parameter.name.starts_with("value_head.") {
            assert!(
                values.iter().all(|&bits| bits == 0),
                "{} received a gradient",
                parameter.name
            );
        }
    }
}

/// Two steps from equal states with equal inputs give equal bits, and a
/// zero-coefficient archive term beside a parent anchor changes nothing.
#[test]
fn steps_are_deterministic_and_a_zero_coefficient_term_is_inert() {
    let model = model();
    let tensors = tensors();
    let parents = parent_targets(&model, &tensors);
    let anchor = external(&tensors, &parents, 1.0, 2);
    let far = targets(&model, &tensors);
    let archive = external(&tensors, &far, 1.0, 2);
    let mut a = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
    let mut b = a.clone();
    let first = a
        .train_step_auxiliary_only_v1(FreshLineageGenerationV1::V3, LR, &[&anchor, &archive])
        .unwrap();
    let second = b
        .train_step_auxiliary_only_v1(FreshLineageGenerationV1::V3, LR, &[&anchor, &archive])
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(state_bits(&a), state_bits(&b));
    assert_ne!(
        state_bits(&a).2,
        state_bits(&NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap()).2
    );
    // The second term's telemetry compares it with the anchor before it.
    assert!(
        first.0[1].gradient_components["all"]["auxiliary_l2"]
            .as_f64()
            .unwrap()
            > 0.0
    );

    let inert = external(&tensors, &far, 0.0, 2);
    let mut c = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
    let before = state_bits(&c);
    c.train_step_auxiliary_only_v1(FreshLineageGenerationV1::V3, LR, &[&anchor, &inert])
        .unwrap();
    let after = state_bits(&c);
    let parameter_count: usize = model
        .parameter_snapshot_v1()
        .iter()
        .map(|parameter| parameter.values.len())
        .sum();
    assert_eq!(after.2[..parameter_count], before.2[..parameter_count]);
}

#[test]
fn invalid_inputs_reject_before_mutation() {
    let model = model();
    let tensors = tensors();
    let mut state = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
    let before = state_bits(&state);
    let far = targets(&model, &tensors);
    let width = logits(&model, &tensors[0]).len();
    let too_wide = one_hot_target_v1(width + 1, 0).unwrap();
    let short = external(&tensors, &far, 1.0, 1);
    let negative = external(&tensors, &far, -1.0, 2);
    let mut wrong_width = external(&tensors, &far, 1.0, 2);
    wrong_width.rows[0].target = &too_wide;
    let empty = ExternalAuxiliaryInputV1 {
        direction: LineBDivergenceV1::Forward,
        coefficient: 1.0,
        denominator: 1,
        rows: Vec::new(),
    };
    for terms in [
        vec![&short],
        vec![&negative],
        vec![&wrong_width],
        vec![&empty],
        vec![],
    ] {
        assert!(state
            .train_step_auxiliary_only_v1(FreshLineageGenerationV1::V3, LR, &terms)
            .is_err());
    }
    let valid = external(&tensors, &far, 1.0, 2);
    for learning_rate in [0.0, -1e-4, f32::NAN] {
        assert!(state
            .train_step_auxiliary_only_v1(FreshLineageGenerationV1::V3, learning_rate, &[&valid])
            .is_err());
    }
    assert_eq!(state_bits(&state), before);
    assert!(one_hot_target_v1(1, 0).is_err());
    assert!(one_hot_target_v1(3, 3).is_err());
    let target = one_hot_target_v1(3, 1).unwrap();
    assert_eq!(target.probabilities, vec![0.0, 1.0, 0.0]);
    assert_eq!(
        target.log_probabilities,
        vec![f64::NEG_INFINITY, 0.0, f64::NEG_INFINITY]
    );
    assert_eq!(target.weight_sum, 1.0);
}
