//! The V8 inclusion net and its function-preserving conversion (collab
//! LANES/w4-combat-20261030/DESIGN.md v5.1, section 5): exact preservation
//! on synthetic FlatV8 rows (a checked-in synthetic parent, so this runs in
//! CI), the conversion's coordinate classes and Adam mapping, the first
//! update's gradients (the zero guard never suppresses the output
//! projection's), the pairing path's backward against finite differences,
//! sequential against fixed-partition backward, the refusals, and the
//! `#[ignore]`d Adam step-age probe. The collected-b512 check is the
//! converter's preservation receipt
//! (`expanded_deck_training_v1::v8_inclusion_conversion_v1`).

use super::*;
use crate::native_flat_tensorizer_v8::schema_v8;
use crate::native_policy_train_step_v1::v8_inclusion_v1::{
    convert_net8_snapshot_to_v8_v1, net8_to_v8_index_v1, v8_coordinate_classes_v1,
    V8CoordinateClassV1, V8_CONVERSION_CONSTRUCTION_VERSION_V1,
};
use crate::native_policy_train_step_v1::widening_v1::{
    hex_sha256_v1, WideningErrorV1, TORCH_DEFAULT_UNIFORM_V1,
};
use crate::native_policy_value_net_v1::v8_inclusion_v1::{
    V8_INCLUDE_COLUMN_V1, V8_PAIRING_INPUT_DIM_V1,
};
use crate::native_policy_value_net_v1::{
    ForwardActivationModeV1, ForwardRowCacheV1, NativePolicyValueForwardScratchV1,
};

const V8_SEED_V1: u64 = 0x0008_1C1D_0000_0001;
/// Rows of each synthetic decision whose include bit is set.
const INCLUDE_ROWS_V1: [&[usize]; 2] = [&[1, 3], &[4]];
/// `(blocker, attacker)` node pairs of each synthetic decision; the second
/// gang-blocks one attacker with two blockers.
const PAIRS_V1: [&[i64]; 2] = [&[1, 4, 7, 2], &[3, 6, 8, 6, 0, 9]];

/// A FlatV8 version of a gradient decision: every action row widened by its
/// include bit, with a pairing relation.
struct V8DecisionV1 {
    base: GradientDecisionV1,
    action_features: Vec<f32>,
    pairs: Vec<i64>,
}

impl V8DecisionV1 {
    fn new_v1(base: GradientDecisionV1, include_rows: &[usize], pairs: &[i64]) -> Self {
        let mut action_features = Vec::new();
        for (row, features) in base
            .action_features
            .chunks_exact(ACTION_FEATURE_DIM_V1)
            .enumerate()
        {
            action_features.extend_from_slice(features);
            action_features.push(if include_rows.contains(&row) {
                1.0
            } else {
                0.0
            });
        }
        Self {
            base,
            action_features,
            pairs: pairs.to_vec(),
        }
    }

    fn view_v1(&self) -> NativeEncodedDecisionViewV1<'_> {
        let base = self.base.view_v1(false);
        NativeEncodedDecisionViewV1 {
            schema: schema_v8(),
            action_features: &self.action_features,
            ..base
        }
        .with_pairing_node_indices_v1(&self.pairs)
    }
}

fn v8_decisions_v1() -> [V8DecisionV1; 2] {
    let [first, second] = gradient_decisions_v1();
    [
        V8DecisionV1::new_v1(first, INCLUDE_ROWS_V1[0], PAIRS_V1[0]),
        V8DecisionV1::new_v1(second, INCLUDE_ROWS_V1[1], PAIRS_V1[1]),
    ]
}

/// A trained net-8 parent: the perturbed runner-fixed model after two GAE
/// updates, so its moments and Adam step are live.
fn trained_parent_v1() -> (
    NativePolicyValueTrainStateV1,
    NativePolicyValueTrainSnapshotV1,
) {
    let mut state = NativePolicyValueTrainStateV1::new_v1(perturbed_dims_v1::<Net8DimsV1>(
        narrow_model_v1(),
        0x4A_0008,
    ))
    .unwrap();
    for _ in 0..2 {
        route_update_dims_v1(&mut state, WideRouteV1::GaeSequential);
    }
    let snapshot = state.snapshot_v1().unwrap();
    (state, snapshot)
}

fn convert_v1(
    parent: &NativePolicyValueTrainSnapshotV1,
    seed: u64,
) -> NativePolicyValueTrainStateV1 {
    convert_net8_snapshot_to_v8_v1(parent, TORCH_DEFAULT_UNIFORM_V1, seed)
        .unwrap()
        .state
}

/// Every logit and the value carry the same f32 bits (so a signed-zero
/// change fails), and the expected outputs are finite.
fn assert_bits_equal_v1(label: &str, logits: (&[f32], &[f32]), values: (f32, f32)) {
    assert_eq!(logits.0.len(), logits.1.len(), "{label}");
    for (index, (parent, child)) in logits.0.iter().zip(logits.1).enumerate() {
        assert!(parent.is_finite(), "{label} logit {index}: {parent}");
        assert_eq!(
            parent.to_bits(),
            child.to_bits(),
            "{label} logit {index}: {parent} {child}"
        );
    }
    assert!(values.0.is_finite(), "{label} value: {}", values.0);
    assert_eq!(
        values.0.to_bits(),
        values.1.to_bits(),
        "{label} value: {} {}",
        values.0,
        values.1
    );
}

/// A tensor's values as f32 bits.
fn tensor_bits_v1(values: &[f32]) -> Vec<u32> {
    values.iter().map(|value| value.to_bits()).collect()
}

/// The V8 substeps of the gradient batch, recorded from `model`.
fn v8_recorded_v1(
    model: &NativePolicyValueNetV1,
    decisions: &[V8DecisionV1; 2],
) -> [RecordedV1; 2] {
    decisions.each_ref().map(|decision| {
        recorded_v1(
            model
                .forward_feature_transfer_v8_mode(
                    decision.view_v1(),
                    ForwardActivationModeV1::LibmTanh,
                )
                .unwrap(),
        )
    })
}

fn v8_substeps_v1<'a>(
    decisions: &'a [V8DecisionV1; 2],
    recorded: &'a [RecordedV1; 2],
) -> Vec<Vec<NativePolicySubstepV1<'a>>> {
    GRADIENT_BATCH_V1
        .iter()
        .map(|group| {
            group
                .iter()
                .map(|&(decision, selected)| NativePolicySubstepV1 {
                    forward: NativePolicyForwardInputV1::Encoded(Box::new(
                        decisions[decision].view_v1(),
                    )),
                    selected_action_index: selected,
                    expected_raw_action_logit_bits: &recorded[decision].logits,
                    expected_value_bits: recorded[decision].value,
                })
                .collect()
        })
        .collect()
}

/// One sequential GAE update of the V8 gradient batch.
fn v8_update_v1(state: &mut NativePolicyValueTrainStateV1) -> NativePolicyTrainStepResultV1 {
    let decisions = v8_decisions_v1();
    let recorded = v8_recorded_v1(state.model_v1(), &decisions);
    let steps = v8_substeps_v1(&decisions, &recorded);
    let groups = gradient_groups_v1(&steps);
    state
        .train_step_gae_feature_transfer_v4(
            &groups,
            &GRADIENT_TARGETS_V1,
            &GRADIENT_ADVANTAGES_V1,
            VC,
            LR,
        )
        .unwrap()
}

/// One GAE update of 100 groups (150 substeps: the V8 gradient batch 50
/// times) through the profiled CPU route the RT1 update takes:
/// `forward_workers` forward workers (with the sequential backward, also the
/// exact parallel backward's workers) and `backward` fixed-partition backward
/// workers, or the sequential backward.
fn v8_many_update_v1(
    state: &mut NativePolicyValueTrainStateV1,
    forward_workers: usize,
    backward: Option<usize>,
) -> NativePolicyTrainStepResultV1 {
    let decisions = v8_decisions_v1();
    let recorded = v8_recorded_v1(state.model_v1(), &decisions);
    let one = v8_substeps_v1(&decisions, &recorded);
    let steps: Vec<Vec<NativePolicySubstepV1<'_>>> =
        (0..50).flat_map(|_| one.iter().cloned()).collect();
    let groups: Vec<NativePolicyPhysicalDecisionV1<'_>> = steps
        .iter()
        .zip(GRADIENT_TERMINAL_RETURNS_V1.iter().cycle())
        .map(
            |(substeps, &terminal_return)| NativePolicyPhysicalDecisionV1 {
                substeps,
                terminal_return,
                baseline_bits: 0,
            },
        )
        .collect();
    let cycled = |values: &[f32]| -> Vec<f32> {
        values.iter().cycle().take(groups.len()).copied().collect()
    };
    state
        .train_step_gae_feature_transfer_profiled_v1(
            FreshLineageGenerationV1::V4,
            &groups,
            &cycled(&GRADIENT_TARGETS_V1),
            &cycled(&GRADIENT_ADVANTAGES_V1),
            VC,
            LR,
            backward,
            forward_workers,
            &mut NativeTrainingPhaseRecorderV1::disabled_v1(),
        )
        .unwrap()
}

/// The converted net's logits and value carry the parent's f32 bits on every
/// synthetic FlatV8 row, with and without include bits and pairing
/// relations, through the reference, scratch and row-cached inference
/// forwards and the train step's tape forward.
#[test]
fn v8_conversion_preserves_every_logit_and_value_on_synthetic_flat_v8_rows() {
    let (parent_state, parent) = trained_parent_v1();
    let narrow = parent_state.model_v1();
    let child_state = convert_v1(&parent, V8_SEED_V1);
    let child = child_state.model_v1();
    assert!(child.is_v8_inclusion_v1());
    let narrow_parameters = narrow.parameter_snapshot_v1();
    let child_parameters = child.parameter_snapshot_v1();
    let mut cases = Vec::new();
    for (index, base) in gradient_decisions_v1().into_iter().enumerate() {
        cases.push(V8DecisionV1::new_v1(
            base,
            INCLUDE_ROWS_V1[index],
            PAIRS_V1[index],
        ));
    }
    for base in gradient_decisions_v1() {
        cases.push(V8DecisionV1::new_v1(base, &[], &[]));
    }
    let mut scratch = NativePolicyValueForwardScratchV1::default();
    let mut rows = ForwardRowCacheV1::default();
    for (index, case) in cases.iter().enumerate() {
        let parent_view = case.base.view_v1(false);
        let expected = narrow.forward_feature_transfer_v3(parent_view).unwrap();
        for mode in [
            ForwardActivationModeV1::LibmTanh,
            ForwardActivationModeV1::KernelDeterministicTanh,
        ] {
            let parent_mode = narrow
                .forward_feature_transfer_v3_mode(parent_view, mode)
                .unwrap();
            let owned = child
                .forward_feature_transfer_v8_mode(case.view_v1(), mode)
                .unwrap();
            assert_bits_equal_v1(
                &format!("reference {index} {mode:?}"),
                (&parent_mode.logits, &owned.logits),
                (parent_mode.value, owned.value),
            );
            let scratched = child
                .forward_feature_transfer_v8_with_scratch_v1(case.view_v1(), &mut scratch, mode)
                .unwrap();
            assert_bits_equal_v1(
                &format!("scratch {index} {mode:?}"),
                (&parent_mode.logits, scratched.logits),
                (parent_mode.value, scratched.value),
            );
            let cached = child
                .forward_rows_cached_with_scratch_v1(
                    case.view_v1(),
                    child.feature_transfer_config_v8(),
                    &mut scratch,
                    &mut rows,
                    mode,
                )
                .unwrap();
            assert_bits_equal_v1(
                &format!("row cache {index} {mode:?}"),
                (&parent_mode.logits, cached.logits),
                (parent_mode.value, cached.value),
            );
        }
        let narrow_tape = forward_with_tape_dims_v1::<Net8DimsV1>(
            &narrow_parameters,
            narrow.feature_transfer_config_v3(),
            parent_view,
            TrainingMathV1::PlatformLibm,
        )
        .unwrap();
        let child_tape = forward_with_tape_dims_v1::<Net8DimsV1>(
            &child_parameters,
            child.feature_transfer_config_v8(),
            case.view_v1(),
            TrainingMathV1::PlatformLibm,
        )
        .unwrap();
        assert_bits_equal_v1(
            &format!("tape {index}"),
            (narrow_tape.logits_v1(), child_tape.logits_v1()),
            (narrow_tape.value_v1(), child_tape.value_v1()),
        );
        assert_bits_equal_v1(
            &format!("tape against inference {index}"),
            (&expected.logits, child_tape.logits_v1()),
            (expected.value, child_tape.value_v1()),
        );
        assert_eq!(child_tape.pairing.is_some(), !case.pairs.is_empty());
    }
}

/// The include column's +0 weight must not turn a parent's negative-zero
/// output into +0. The synthetic parent's `action_encoder.0` has -0 weights
/// and bias over nonnegative inputs (so its pre-activations are -0), and its
/// downstream layers carry that sign to every logit and the value (positive
/// state and action-ref hiddens through -0 or +1 weights, -0 biases). An
/// unguarded child adds `x * (+0) = +0` at column 195, so its accumulators,
/// and then its logits, would be +0. Every logit and the value must keep the
/// parent's bits through the reference, scratch, row-cached and tape
/// forwards, with and without include bits and pairing relations.
#[test]
fn v8_include_column_keeps_negative_zero_outputs_bitwise() {
    let mut parent = narrow_model_v1();
    let mut parameters = parent.parameter_snapshot_v1();
    for parameter in &mut parameters {
        let input_dim = parameter.shape.last().copied().unwrap_or(1);
        let fill: Option<fn(usize) -> f32> = match parameter.name {
            "action_encoder.0.weight"
            | "action_encoder.0.bias"
            | "action_encoder.2.bias"
            | "scorer.0.bias"
            | "scorer.2.bias"
            | "value_head.0.weight"
            | "value_head.0.bias"
            | "value_head.2.bias" => Some(|_| -0.0),
            "action_encoder.2.weight" | "scorer.2.weight" | "value_head.2.weight" => Some(|_| 1.0),
            // A positive state hidden and action-ref hidden: tanh(1).
            "state_encoder.2.weight" | "action_ref_encoder.2.weight" => Some(|_| 0.0),
            "state_encoder.2.bias" | "action_ref_encoder.2.bias" => Some(|_| 1.0),
            // State columns -0 (positive inputs), action columns +1.
            "scorer.0.weight" => Some(|column| if column < HIDDEN_DIM_V1 { -0.0 } else { 1.0 }),
            _ => None,
        };
        if let Some(fill) = fill {
            for (index, value) in parameter.values.iter_mut().enumerate() {
                *value = fill(index % input_dim);
            }
        }
    }
    parent.replace_parameter_snapshot_v1(&parameters).unwrap();
    let fan_in: Vec<f32> = (0..HIDDEN_DIM_V1 * V8_PAIRING_INPUT_DIM_V1)
        .map(|index| (index % 7) as f32 * 0.01 - 0.03)
        .collect();
    let child = parent.clone().into_v8_inclusion_v1(fan_in).unwrap();
    let parent_parameters = parent.parameter_snapshot_v1();
    let child_parameters = child.parameter_snapshot_v1();
    // Nonnegative action features: every `action_encoder.0` product of the
    // parent is then -0.
    let nonnegative = || {
        gradient_decisions_v1().map(|mut base| {
            for value in &mut base.action_features {
                *value = value.abs();
            }
            base
        })
    };
    let mut cases = Vec::new();
    for (index, base) in nonnegative().into_iter().enumerate() {
        cases.push(V8DecisionV1::new_v1(
            base,
            INCLUDE_ROWS_V1[index],
            PAIRS_V1[index],
        ));
    }
    for base in nonnegative() {
        cases.push(V8DecisionV1::new_v1(base, &[], &[]));
    }
    let negative_zero = (-0.0f32).to_bits();
    let mut scratch = NativePolicyValueForwardScratchV1::default();
    let mut rows = ForwardRowCacheV1::default();
    for (index, case) in cases.iter().enumerate() {
        let parent_view = case.base.view_v1(false);
        for mode in [
            ForwardActivationModeV1::LibmTanh,
            ForwardActivationModeV1::KernelDeterministicTanh,
        ] {
            let expected = parent
                .forward_feature_transfer_v3_mode(parent_view, mode)
                .unwrap();
            if matches!(mode, ForwardActivationModeV1::LibmTanh) {
                // The scenario is live: every parent output is -0.
                for output in expected.logits.iter().chain([&expected.value]) {
                    assert_eq!(output.to_bits(), negative_zero, "case {index}");
                }
            }
            let owned = child
                .forward_feature_transfer_v8_mode(case.view_v1(), mode)
                .unwrap();
            assert_bits_equal_v1(
                &format!("signed zero reference {index} {mode:?}"),
                (&expected.logits, &owned.logits),
                (expected.value, owned.value),
            );
            let scratched = child
                .forward_feature_transfer_v8_with_scratch_v1(case.view_v1(), &mut scratch, mode)
                .unwrap();
            assert_bits_equal_v1(
                &format!("signed zero scratch {index} {mode:?}"),
                (&expected.logits, scratched.logits),
                (expected.value, scratched.value),
            );
            let cached = child
                .forward_rows_cached_with_scratch_v1(
                    case.view_v1(),
                    child.feature_transfer_config_v8(),
                    &mut scratch,
                    &mut rows,
                    mode,
                )
                .unwrap();
            assert_bits_equal_v1(
                &format!("signed zero row cache {index} {mode:?}"),
                (&expected.logits, cached.logits),
                (expected.value, cached.value),
            );
        }
        let parent_tape = forward_with_tape_dims_v1::<Net8DimsV1>(
            &parent_parameters,
            parent.feature_transfer_config_v3(),
            parent_view,
            TrainingMathV1::PlatformLibm,
        )
        .unwrap();
        let child_tape = forward_with_tape_dims_v1::<Net8DimsV1>(
            &child_parameters,
            child.feature_transfer_config_v8(),
            case.view_v1(),
            TrainingMathV1::PlatformLibm,
        )
        .unwrap();
        let parent_value = parent_tape.value_v1();
        for output in parent_tape.logits_v1().iter().chain([&parent_value]) {
            assert_eq!(output.to_bits(), negative_zero, "tape case {index}");
        }
        assert_bits_equal_v1(
            &format!("signed zero tape {index}"),
            (parent_tape.logits_v1(), child_tape.logits_v1()),
            (parent_value, child_tape.value_v1()),
        );
        assert_eq!(child_tape.pairing.is_some(), !case.pairs.is_empty());
    }
}

/// Every V8 coordinate is where the conversion says: old coordinates hold the
/// parent's parameter and both moments by bits at their mapped index, the
/// include column, the pairing biases and the output projection are +0, the
/// pairing fan-in draws lie inside the law's bound, every new moment is +0,
/// and `adam_step` and the gauge anchor are kept. The provenance records the
/// construction, and the conversion is deterministic in its seed, which moves
/// only the drawn coordinates.
#[test]
fn v8_conversion_keeps_old_coordinates_and_zeroes_every_new_moment() {
    use V8CoordinateClassV1::*;
    let (_, parent) = trained_parent_v1();
    let converted =
        convert_net8_snapshot_to_v8_v1(&parent, TORCH_DEFAULT_UNIFORM_V1, V8_SEED_V1).unwrap();
    let child = converted.state.snapshot_v1().unwrap();
    assert_eq!(child.parameters.len(), V8_PARAMETER_TENSOR_COUNT_V1);
    assert_eq!(child.adam_step, parent.adam_step);
    assert_eq!(
        child.scorer_bias_anchor_bits,
        parent.scorer_bias_anchor_bits
    );
    let bound = 1.0 / (V8_PAIRING_INPUT_DIM_V1 as f64).sqrt();
    let mut seen = vec![vec![false; 0]; V8_PARAMETER_TENSOR_COUNT_V1];
    for (tensor, mask) in seen.iter_mut().enumerate() {
        *mask = vec![false; child.parameters[tensor].values.len()];
    }
    for tensor in 0..PARAMETER_TENSOR_COUNT {
        for index in 0..parent.parameters[tensor].values.len() {
            let target = net8_to_v8_index_v1(tensor, index);
            seen[tensor][target] = true;
            assert_eq!(
                v8_coordinate_classes_v1(tensor)[target],
                Old,
                "tensor {tensor} {index}"
            );
            for (child, parent) in [
                (&child.parameters, &parent.parameters),
                (&child.first_moments, &parent.first_moments),
                (&child.second_moments, &parent.second_moments),
            ] {
                assert_eq!(
                    child[tensor].values[target].to_bits(),
                    parent[tensor].values[index].to_bits(),
                    "tensor {tensor} {index}"
                );
            }
        }
    }
    let mut drawn = 0u64;
    for (tensor, seen) in seen.iter().enumerate() {
        for (index, class) in v8_coordinate_classes_v1(tensor).into_iter().enumerate() {
            assert_eq!(seen[index], class == Old, "tensor {tensor} {index}");
            if class == Old {
                continue;
            }
            assert_eq!(child.first_moments[tensor].values[index].to_bits(), 0);
            assert_eq!(child.second_moments[tensor].values[index].to_bits(), 0);
            let value = child.parameters[tensor].values[index];
            if class == NewPairingFanIn {
                drawn += 1;
                assert!(f64::from(value).abs() < bound && value != 0.0);
            } else {
                assert_eq!(value.to_bits(), 0, "tensor {tensor} {index} {class:?}");
            }
        }
    }
    assert_eq!(drawn, (HIDDEN_DIM_V1 * V8_PAIRING_INPUT_DIM_V1) as u64);
    let provenance = &converted.provenance;
    assert_eq!(
        provenance.construction_version,
        V8_CONVERSION_CONSTRUCTION_VERSION_V1
    );
    assert_eq!(provenance.init_law, TORCH_DEFAULT_UNIFORM_V1);
    assert_eq!(provenance.seed, V8_SEED_V1);
    assert_eq!(provenance.adam_step, parent.adam_step);
    assert_eq!(provenance.drawn_coordinate_count, drawn);
    assert_eq!(
        provenance.parent_state_sha256,
        hex_sha256_v1(parent.state_sha256_v1().unwrap())
    );
    assert_eq!(
        provenance.child_state_sha256,
        hex_sha256_v1(child.state_sha256_v1().unwrap())
    );
    assert_eq!(provenance.tensors.len(), V8_PARAMETER_TENSOR_COUNT_V1);
    assert_eq!(
        provenance.tensors[ACTION_FIRST_WEIGHT].counts["new_include_column"],
        HIDDEN_DIM_V1 as u64
    );
    // Deterministic, and the seed moves only the drawn coordinates.
    let again = convert_v1(&parent, V8_SEED_V1).snapshot_v1().unwrap();
    assert_eq!(again, child);
    assert_eq!(
        again.state_sha256_v1().unwrap(),
        child.state_sha256_v1().unwrap()
    );
    let other = convert_v1(&parent, V8_SEED_V1 + 1).snapshot_v1().unwrap();
    for tensor in 0..V8_PARAMETER_TENSOR_COUNT_V1 {
        let differs = tensor_bits_v1(&child.parameters[tensor].values)
            != tensor_bits_v1(&other.parameters[tensor].values);
        assert_eq!(
            differs,
            tensor == PAIRING_FIRST_WEIGHT_V1,
            "tensor {tensor}"
        );
    }
    // Refusals: another law, and a parent that is not net-8.
    assert!(matches!(
        convert_net8_snapshot_to_v8_v1(&parent, "xavier", V8_SEED_V1),
        Err(WideningErrorV1::UnsupportedInitLaw { .. })
    ));
    assert!(convert_net8_snapshot_to_v8_v1(&child, TORCH_DEFAULT_UNIFORM_V1, V8_SEED_V1).is_err());
}

/// The first update from the converted state, on a batch with include rows
/// and pairing relations: the output projection gets a nonzero gradient and
/// moves (the zero guard does not suppress it), the include column of
/// `action_encoder.0` gets a nonzero gradient and moves, and the pairing
/// path's first layer, whose fan-out is still +0, gets none until the second
/// update.
#[test]
fn v8_first_update_moves_the_output_projection_and_the_include_column() {
    let (_, parent) = trained_parent_v1();
    let start = convert_v1(&parent, V8_SEED_V1);
    let before = start.snapshot_v1().unwrap();
    let mut sequential = start.clone();
    let first = v8_update_v1(&mut sequential);
    let after = sequential.snapshot_v1().unwrap();
    let nonzero = |values: &[f32]| values.iter().any(|value| *value != 0.0);
    let gradient = |tensor: usize| first.gradients[tensor].values.as_slice();
    assert!(nonzero(gradient(PAIRING_SECOND_WEIGHT_V1)));
    assert!(nonzero(gradient(PAIRING_SECOND_BIAS_V1)));
    assert!(!nonzero(gradient(PAIRING_FIRST_WEIGHT_V1)));
    assert!(!nonzero(gradient(PAIRING_FIRST_BIAS_V1)));
    assert_ne!(
        after.parameters[PAIRING_SECOND_WEIGHT_V1].values,
        before.parameters[PAIRING_SECOND_WEIGHT_V1].values
    );
    let width = V8_ACTION_FIRST_WEIGHT_SHAPE_V1[1];
    let include_column = |values: &[f32]| -> Vec<f32> {
        values
            .chunks_exact(width)
            .map(|row| row[V8_INCLUDE_COLUMN_V1])
            .collect()
    };
    assert!(nonzero(&include_column(gradient(ACTION_FIRST_WEIGHT))));
    assert_ne!(
        include_column(&after.parameters[ACTION_FIRST_WEIGHT].values),
        include_column(&before.parameters[ACTION_FIRST_WEIGHT].values)
    );
    assert_eq!(after.adam_step, before.adam_step + 1);
    assert_eq!(
        after.scorer_bias_anchor_bits,
        before.scorer_bias_anchor_bits
    );
    let second = v8_update_v1(&mut sequential);
    assert!(nonzero(&second.gradients[PAIRING_FIRST_WEIGHT_V1].values));
    // Without pairing rows or include bits the new coordinates take no
    // gradient, and Adam's exact-zero shortcut leaves them, and their
    // moments, at +0.
    let mut plain = start.clone();
    let decisions = gradient_decisions_v1().map(|base| V8DecisionV1::new_v1(base, &[], &[]));
    let recorded = v8_recorded_v1(plain.model_v1(), &decisions);
    let steps = v8_substeps_v1(&decisions, &recorded);
    let groups = gradient_groups_v1(&steps);
    let result = plain
        .train_step_gae_feature_transfer_v4(
            &groups,
            &GRADIENT_TARGETS_V1,
            &GRADIENT_ADVANTAGES_V1,
            VC,
            LR,
        )
        .unwrap();
    let plain_after = plain.snapshot_v1().unwrap();
    for tensor in PAIRING_SECOND_WEIGHT_V1..=PAIRING_SECOND_BIAS_V1 {
        assert!(!nonzero(&result.gradients[tensor].values));
        assert!(plain_after.parameters[tensor]
            .values
            .iter()
            .all(|value| value.to_bits() == 0));
        assert!(plain_after.first_moments[tensor]
            .values
            .iter()
            .all(|value| value.to_bits() == 0));
    }
    assert!(!nonzero(&include_column(
        &result.gradients[ACTION_FIRST_WEIGHT].values
    )));
}

/// The RT1 update's CPU routes agree on a V8 batch with include rows and
/// pairing relations: the exact parallel backward (sequential backward on 4
/// forward workers) gives the sequential backward's gradient and state bits,
/// and the fixed-partition backward gives the same bits on 1 and 4 workers.
/// Both are checked at the conversion (output projection +0, so the pairing
/// first layer and the node inputs get no pairing signal) and after one
/// update, where the output projection is nonzero and every pairing tensor's
/// gradient is.
#[test]
fn v8_exact_parallel_and_fixed_partition_backward_routes_agree() {
    let (_, parent) = trained_parent_v1();
    let converted = convert_v1(&parent, V8_SEED_V1);
    let mut updated = converted.clone();
    v8_update_v1(&mut updated);
    assert!(
        updated.snapshot_v1().unwrap().parameters[PAIRING_SECOND_WEIGHT_V1]
            .values
            .iter()
            .any(|value| *value != 0.0)
    );
    for (live, start) in [(false, &converted), (true, &updated)] {
        let after = |forward_workers, backward| {
            let mut state = start.clone();
            let result = v8_many_update_v1(&mut state, forward_workers, backward);
            for tensor in PAIRING_FIRST_WEIGHT_V1..=PAIRING_SECOND_BIAS_V1 {
                let nonzero = result.gradients[tensor]
                    .values
                    .iter()
                    .any(|value| *value != 0.0);
                let expected = live || tensor >= PAIRING_SECOND_WEIGHT_V1;
                assert_eq!(nonzero, expected, "live {live} tensor {tensor}");
            }
            let gradients: Vec<Vec<u32>> = result
                .gradients
                .iter()
                .map(|gradient| tensor_bits_v1(&gradient.values))
                .collect();
            (gradients, state.state_sha256_v1().unwrap())
        };
        assert!(
            after(1, None) == after(4, None),
            "exact parallel, live {live}"
        );
        assert!(
            after(1, Some(1)) == after(4, Some(4)),
            "fixed partition, live {live}"
        );
    }
}

/// The pairing path's and the include column's gradients against central
/// differences of the train step's loss, on a perturbed V8 net (every
/// pairing coordinate nonzero, so both layers and both members of each pair
/// carry signal).
#[test]
fn v8_pairing_and_include_gradients_match_finite_differences() {
    let model = perturbed_dims_v1::<Net8DimsV1>(
        NativePolicyValueNetV1::v8_inclusion_template_v1().unwrap(),
        0x08FD_0001,
    );
    let loss_and_gradients = |model: &NativePolicyValueNetV1| {
        let mut state = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
        let result = v8_update_v1(&mut state);
        (f64::from(result.loss), result.gradients)
    };
    let (_, gradients) = loss_and_gradients(&model);
    let parameters = model.parameter_snapshot_v1();
    let step = 1.0e-2f32;
    let mut checked = 0;
    for tensor in [
        PAIRING_FIRST_WEIGHT_V1,
        PAIRING_FIRST_BIAS_V1,
        PAIRING_SECOND_WEIGHT_V1,
        PAIRING_SECOND_BIAS_V1,
        ACTION_FIRST_WEIGHT,
        OBJECT_SECOND_WEIGHT,
    ] {
        // The three largest-magnitude coordinates of the tensor (the include
        // column for `action_encoder.0`).
        let mut order: Vec<usize> = (0..gradients[tensor].values.len())
            .filter(|&index| {
                tensor != ACTION_FIRST_WEIGHT
                    || index % V8_ACTION_FIRST_WEIGHT_SHAPE_V1[1] == V8_INCLUDE_COLUMN_V1
            })
            .collect();
        order.sort_by(|&a, &b| {
            gradients[tensor].values[b]
                .abs()
                .total_cmp(&gradients[tensor].values[a].abs())
        });
        for &index in order.iter().take(3) {
            let analytic = f64::from(gradients[tensor].values[index]);
            let loss_at = |delta: f32| {
                let mut shifted = parameters.clone();
                shifted[tensor].values[index] += delta;
                let mut model = model.clone();
                model.replace_parameter_snapshot_v1(&shifted).unwrap();
                loss_and_gradients(&model).0
            };
            let numeric = (loss_at(step) - loss_at(-step)) / (2.0 * f64::from(step));
            let tolerance = 0.05 * analytic.abs().max(numeric.abs()) + 2.0e-4;
            assert!(
                (analytic - numeric).abs() <= tolerance,
                "tensor {tensor} index {index}: analytic {analytic} numeric {numeric}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 18);
}

/// The V8 net refuses net-8 inputs and every malformed pairing relation, and
/// net-8 refuses FlatV8 inputs.
#[test]
fn v8_net_and_net8_refuse_each_others_inputs_and_bad_pairings() {
    let child = NativePolicyValueNetV1::v8_inclusion_template_v1().unwrap();
    let narrow = narrow_model_v1();
    let [decision, _] = v8_decisions_v1();
    let mode = ForwardActivationModeV1::LibmTanh;
    assert!(child
        .forward_feature_transfer_v8_mode(decision.view_v1(), mode)
        .is_ok());
    assert!(narrow
        .forward_feature_transfer_v8_mode(decision.view_v1(), mode)
        .is_err());
    assert!(child
        .forward_feature_transfer_v3_mode(decision.base.view_v1(false), mode)
        .is_err());
    assert!(child
        .forward_feature_transfer_v4_mode(
            NativeEncodedDecisionViewV1 {
                schema: crate::native_flat_tensorizer_v4::schema_v4(),
                ..decision.base.view_v1(false)
            },
            mode
        )
        .is_err());
    // A pairing relation outside the V8 config.
    assert!(narrow
        .forward_feature_transfer_v3_mode(
            decision
                .base
                .view_v1(false)
                .with_pairing_node_indices_v1(&[1, 4]),
            mode
        )
        .is_err());
    for pairs in [&[1i64][..], &[1, 1], &[1, 10], &[-1, 2]] {
        assert!(
            child
                .forward_feature_transfer_v8_mode(
                    decision.view_v1().with_pairing_node_indices_v1(pairs),
                    mode
                )
                .is_err(),
            "{pairs:?}"
        );
    }
    assert_eq!(
        child.parameter_count_v1(),
        crate::native_policy_value_net_v1::v8_inclusion_v1::V8_PARAMETER_COUNT_V1
    );
    assert!(narrow.clone().into_v8_inclusion_v1(vec![0.0; 3]).is_err());
    assert!(child.clone().into_v8_inclusion_v1(vec![0.0; 8192]).is_err());
}

/// Design section 5 (Adam): the measured step-age trajectory of the V8
/// conversion's new coordinates under the unchanged optimizer, as widening_v1
/// section 7.6 measured it for the width arm. The parent is the synthetic
/// trained parent with the b512 dose checkpoint's Adam step (118,584); 100
/// GAE updates of the V8 gradient batch. Writes JSON to `MTG_KERNEL_V8_STEP_AGE_OUT` when set,
/// else prints it.
#[test]
#[ignore = "measurement probe: run explicitly; writes the V8 step-age JSON"]
fn v8_adam_step_age_trajectory_probe_v1() {
    const UPDATES: usize = 100;
    let (_, mut parent) = trained_parent_v1();
    parent.adam_step = 118_584;
    let converted =
        convert_net8_snapshot_to_v8_v1(&parent, TORCH_DEFAULT_UNIFORM_V1, V8_SEED_V1).unwrap();
    let mut state = converted.state.clone();
    let classes: Vec<Vec<V8CoordinateClassV1>> = (0..V8_PARAMETER_TENSOR_COUNT_V1)
        .map(v8_coordinate_classes_v1)
        .collect();
    let mut rows = Vec::with_capacity(UPDATES);
    for update in 1..=UPDATES {
        let before = state.snapshot_v1().unwrap();
        let result = v8_update_v1(&mut state);
        let after = state.snapshot_v1().unwrap();
        let mut per_class = BTreeMap::<V8CoordinateClassV1, Vec<f64>>::new();
        for (tensor, tensor_classes) in classes.iter().enumerate() {
            for (index, &class) in tensor_classes.iter().enumerate() {
                if after.first_moments[tensor].values[index].to_bits() != 0 {
                    let delta = f64::from(after.parameters[tensor].values[index])
                        - f64::from(before.parameters[tensor].values[index]);
                    per_class
                        .entry(class)
                        .or_default()
                        .push(delta.abs() / f64::from(LR));
                }
            }
        }
        let mut class_rows = serde_json::Map::new();
        for class in V8CoordinateClassV1::ALL {
            let mut values = per_class.remove(&class).unwrap_or_default();
            if values.is_empty() {
                continue;
            }
            values.sort_by(f64::total_cmp);
            class_rows.insert(
                class.name_v1().into(),
                serde_json::json!({
                    "active": values.len(),
                    "median": values[values.len() / 2],
                    "max": values[values.len() - 1],
                }),
            );
        }
        rows.push(
            serde_json::json!({"update": update, "adam_step": result.adam_step,
            "loss": result.loss, "classes": class_rows}),
        );
    }
    let report = serde_json::json!({
        "schema": "mtg-kernel-v8-inclusion-adam-step-age-trajectory-v1",
        "definition": "per update and coordinate class: |parameter after - parameter before| / learning_rate over active coordinates (first moment nonzero after the update)",
        "inherited_adam_step": parent.adam_step,
        "conversion": converted.provenance,
        "learning_rate": LR,
        "updates": UPDATES,
        "reference_constant_gradient": "(1 - 0.9^k) / sqrt(1 - 0.999^k): 3.16 at k = 1, peak 6.57 at k = 12",
        "trajectory": rows,
    });
    let text = serde_json::to_string_pretty(&report).unwrap();
    match std::env::var_os("MTG_KERNEL_V8_STEP_AGE_OUT") {
        Some(path) => std::fs::write(path, text).unwrap(),
        None => println!("{text}"),
    }
}
