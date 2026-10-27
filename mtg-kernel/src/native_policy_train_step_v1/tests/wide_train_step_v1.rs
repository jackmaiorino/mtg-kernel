//! Wide (net-8w128) CPU train step validation (collab
//! LANES/throughput-architecture-20261020/WIDE-BACKWARD-DESIGN.md v2.1):
//! gradient correctness against an independent f64 finite-difference oracle
//! (section 5.1), exact zero-padding equivalence with net-8 (5.2), and
//! determinism and resume (5.5). Every wide state trains under the platform
//! libm. The unit-level timing probes of section 6 are `#[ignore]`d here.

use super::*;
use crate::native_flat_tensorizer_v3::{encoded_decision_view_v3, schema_v3};
use crate::native_policy_value_net_v1::NativePolicyValueOutputV1;
use crate::native_train_state_payload_v1::{
    decode_native_train_state_payload_wide_v1, encode_native_train_state_payload_wide_v1,
};
use crate::sideboard_play_policy_v1::FreshLineageGenerationV1;
use crate::state::SplitMix64;
use std::collections::BTreeMap;
use std::io::Write;

const VC: f32 = 0.75;
const LR: f32 = 0.00003;
/// Frozen before any wide result (design section 5.1): per coordinate,
/// `|g_rust - g_fd| <= FD_ABSOLUTE + FD_RELATIVE * |g_fd|`.
const FD_ABSOLUTE_V1: f64 = 5.0e-5;
const FD_RELATIVE_V1: f64 = 2.0e-3;
/// Central-difference step on the f32 parameter cast to f64.
const FD_STEP_V1: f64 = 1.0e-4;
/// A witness counts only if its oracle derivative exceeds this many
/// allowances; every block the widening adds needs three counted witnesses.
const FD_DETECTION_FACTOR_V1: f64 = 10.0;
const FD_COUNTED_PER_NEW_BLOCK_V1: usize = 3;
/// Wide passes only at most this many times narrow's maximum error ratio.
const FD_NARROW_RATIO_FACTOR_V1: f64 = 4.0;
const NEW_BLOCK_DRAWS_V1: usize = 12;
const OLD_BLOCK_DRAWS_V1: usize = 4;
const NARROW_BLOCK_DRAWS_V1: usize = 8;

/// The inference forwards a train step's recorded outputs come from.
trait RecordingNetV1: TrainableNetV1 {
    fn forward_v3_for_test_v1(
        &self,
        view: NativeEncodedDecisionViewV1<'_>,
    ) -> NativePolicyValueOutputV1;
    fn forward_contract_for_test_v1(
        &self,
        view: NativeEncodedDecisionViewV1<'_>,
    ) -> NativePolicyValueOutputV1;
}

impl RecordingNetV1 for NativePolicyValueNetV1 {
    fn forward_v3_for_test_v1(
        &self,
        view: NativeEncodedDecisionViewV1<'_>,
    ) -> NativePolicyValueOutputV1 {
        self.forward_feature_transfer_v3(view).unwrap()
    }

    fn forward_contract_for_test_v1(
        &self,
        view: NativeEncodedDecisionViewV1<'_>,
    ) -> NativePolicyValueOutputV1 {
        self.forward_v1(view).unwrap()
    }
}

impl RecordingNetV1 for NativePolicyValueNetWideV1 {
    fn forward_v3_for_test_v1(
        &self,
        view: NativeEncodedDecisionViewV1<'_>,
    ) -> NativePolicyValueOutputV1 {
        self.forward_feature_transfer_v3_wide_v1(view).unwrap()
    }

    fn forward_contract_for_test_v1(
        &self,
        view: NativeEncodedDecisionViewV1<'_>,
    ) -> NativePolicyValueOutputV1 {
        self.forward_wide_v1(view).unwrap()
    }
}

fn narrow_model_v1() -> NativePolicyValueNetV1 {
    NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1()).unwrap()
}

fn wide_model_v1() -> NativePolicyValueNetWideV1 {
    NativePolicyValueNetWideV1::runner_fixed_wide_v1(
        NativePolicyValueModelConfigV1::contract_wide_v1(),
    )
    .unwrap()
}

fn uniform_v1(rng: &mut SplitMix64) -> f32 {
    (rng.next_u64() >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0
}

/// `model` with every parameter but the embedding's padding row moved by its
/// own seeded draw in [-0.1, 0.1).
fn perturbed_dims_v1<D: NetDimsV1>(mut model: D::Net, seed: u64) -> D::Net {
    let mut rng = SplitMix64::seed(seed);
    let mut parameters = model.parameter_snapshot_v1();
    for (index, parameter) in parameters.iter_mut().enumerate() {
        let skip = if index == CARD_EMBEDDING {
            D::CARD_EMBEDDING
        } else {
            0
        };
        for value in &mut parameter.values[skip..] {
            *value += 0.1 * uniform_v1(&mut rng);
        }
    }
    model.replace_parameter_snapshot_v1(&parameters).unwrap();
    model
}

/// A synthetic decision for the gradient checks (design section 5.1): ten
/// objects over nine distinct non-padding card ids and one padding token,
/// seven populated object groups, twelve edges, five actions and six action
/// refs, so every concatenation segment carries signal. Labelled with the V3
/// schema, or the contract schema for the contract-config routes.
struct GradientDecisionV1 {
    state: Vec<f32>,
    object_features: Vec<f32>,
    object_card_ids: Vec<i64>,
    object_groups: Vec<i64>,
    object_node_ids: Vec<i64>,
    edge_features: Vec<f32>,
    edge_source_indices: Vec<i64>,
    edge_target_indices: Vec<i64>,
    action_features: Vec<f32>,
    action_ref_features: Vec<f32>,
    action_ref_card_ids: Vec<i64>,
    action_ref_action_indices: Vec<i64>,
    action_ref_node_indices: Vec<i64>,
}

const GRADIENT_CARD_IDS_V1: [i64; 10] = [0, 3, 17, 255, 1024, 4096, 65536, 31, 7, 9];
const GRADIENT_GROUPS_V1: [i64; 10] = [0, 1, 1, 2, 5, 5, 7, 0, 3, 19];
const GRADIENT_ACTIONS_V1: usize = 5;

impl GradientDecisionV1 {
    fn new_v1(seed: u64) -> Self {
        let mut rng = SplitMix64::seed(seed);
        let mut values = |len: usize| -> Vec<f32> {
            (0..len)
                .map(|_| {
                    // A third of the inputs are exact zeros, as one-hot features give.
                    if rng.next_u64() % 3 == 0 {
                        0.0
                    } else {
                        uniform_v1(&mut rng)
                    }
                })
                .collect()
        };
        let objects = GRADIENT_CARD_IDS_V1.len();
        let edges = 12;
        Self {
            state: values(STATE_DIM_V1),
            object_features: values(objects * OBJECT_FEATURE_DIM_V1),
            object_card_ids: GRADIENT_CARD_IDS_V1.to_vec(),
            object_groups: GRADIENT_GROUPS_V1.to_vec(),
            object_node_ids: (0..objects as i64).collect(),
            edge_features: values(edges * EDGE_FEATURE_DIM_V1),
            edge_source_indices: (0..edges as i64).map(|edge| (edge * 3 + 1) % 10).collect(),
            edge_target_indices: (0..edges as i64).map(|edge| (edge * 7 + 2) % 10).collect(),
            action_features: values(GRADIENT_ACTIONS_V1 * ACTION_FEATURE_DIM_V1),
            action_ref_features: values(6 * ACTION_REF_FEATURE_DIM_V1),
            action_ref_card_ids: vec![3, 0, 17, 4096, 9, 1],
            action_ref_action_indices: vec![0, 1, 1, 3, 4, 2],
            action_ref_node_indices: vec![2, 5, 9, 0, 7, 3],
        }
    }

    fn view_v1(&self, contract: bool) -> NativeEncodedDecisionViewV1<'_> {
        NativeEncodedDecisionViewV1::from_slices_unvalidated(
            if contract {
                NativeEncodedDecisionSchemaV1::contract_v1()
            } else {
                schema_v3()
            },
            &self.state,
            &self.object_features,
            &self.object_card_ids,
            &self.object_groups,
            &self.object_node_ids,
            &self.edge_features,
            &self.edge_source_indices,
            &self.edge_target_indices,
            &self.action_features,
            &self.action_ref_features,
            &self.action_ref_card_ids,
            &self.action_ref_action_indices,
            &self.action_ref_node_indices,
        )
    }
}

fn gradient_decisions_v1() -> [GradientDecisionV1; 2] {
    [
        GradientDecisionV1::new_v1(11),
        GradientDecisionV1::new_v1(29),
    ]
}

/// The gradient batch: `(decision, selected action)` per substep, per group.
const GRADIENT_BATCH_V1: [&[(usize, usize)]; 2] = [&[(0, 0), (1, 2)], &[(1, 4)]];
const GRADIENT_TERMINAL_RETURNS_V1: [i8; 2] = [1, -1];
const GRADIENT_TARGETS_V1: [f32; 2] = [0.3, -0.2];
const GRADIENT_ADVANTAGES_V1: [f32; 2] = [0.6, -0.45];

/// Outputs a model recorded for one decision, as the train step replays them.
struct RecordedV1 {
    logits: Vec<u32>,
    value: u32,
}

fn recorded_v1(output: NativePolicyValueOutputV1) -> RecordedV1 {
    RecordedV1 {
        logits: output.logits.iter().map(|value| value.to_bits()).collect(),
        value: output.value.to_bits(),
    }
}

fn record_gradient_decisions_v1<N: RecordingNetV1>(
    model: &N,
    decisions: &[GradientDecisionV1; 2],
    contract: bool,
) -> [RecordedV1; 2] {
    decisions.each_ref().map(|decision| {
        recorded_v1(if contract {
            model.forward_contract_for_test_v1(decision.view_v1(true))
        } else {
            model.forward_v3_for_test_v1(decision.view_v1(false))
        })
    })
}

fn gradient_substeps_v1<'a>(
    decisions: &'a [GradientDecisionV1; 2],
    recorded: &'a [RecordedV1; 2],
    contract: bool,
) -> Vec<Vec<NativePolicySubstepV1<'a>>> {
    GRADIENT_BATCH_V1
        .iter()
        .map(|group| {
            group
                .iter()
                .map(|&(decision, selected)| NativePolicySubstepV1 {
                    forward: NativePolicyForwardInputV1::Encoded(Box::new(
                        decisions[decision].view_v1(contract),
                    )),
                    selected_action_index: selected,
                    expected_raw_action_logit_bits: &recorded[decision].logits,
                    expected_value_bits: recorded[decision].value,
                })
                .collect()
        })
        .collect()
}

fn gradient_groups_v1<'a>(
    steps: &'a [Vec<NativePolicySubstepV1<'a>>],
) -> Vec<NativePolicyPhysicalDecisionV1<'a>> {
    steps
        .iter()
        .zip(GRADIENT_TERMINAL_RETURNS_V1)
        .map(
            |(substeps, terminal_return)| NativePolicyPhysicalDecisionV1 {
                substeps,
                terminal_return,
                baseline_bits: 0,
            },
        )
        .collect()
}

// ---------------------------------------------------------------------------
// The f64 oracle (design section 5.1), written from
// python/mtg_kernel_rl/model.py with runtime dims. It shares no code with the
// tape forward.
// ---------------------------------------------------------------------------

fn oracle_linear_v1(weight: &[f64], bias: &[f64], input: &[f64]) -> Vec<f64> {
    let input_dim = input.len();
    assert_eq!(weight.len(), input_dim * bias.len());
    bias.iter()
        .enumerate()
        .map(|(output, &bias)| {
            bias + weight[output * input_dim..(output + 1) * input_dim]
                .iter()
                .zip(input)
                .map(|(weight, input)| weight * input)
                .sum::<f64>()
        })
        .collect()
}

/// `nn.Sequential(Linear, Tanh, Linear, Tanh)` from the tensor at `first`.
fn oracle_two_layer_v1(parameters: &[Vec<f64>], first: usize, input: &[f64]) -> Vec<f64> {
    let hidden: Vec<f64> = oracle_linear_v1(&parameters[first], &parameters[first + 1], input)
        .into_iter()
        .map(f64::tanh)
        .collect();
    oracle_linear_v1(&parameters[first + 2], &parameters[first + 3], &hidden)
        .into_iter()
        .map(f64::tanh)
        .collect()
}

fn widen_v1(values: &[f32]) -> Vec<f64> {
    values.iter().map(|&value| f64::from(value)).collect()
}

/// `KernelPolicyValueNet.forward`: logits and value.
fn oracle_forward_v1(
    parameters: &[Vec<f64>],
    hidden: usize,
    embedding: usize,
    view: &NativeEncodedDecisionViewV1<'_>,
) -> (Vec<f64>, f64) {
    let objects = view.object_card_ids.len();
    let base: Vec<Vec<f64>> = (0..objects)
        .map(|object| {
            let mut input = widen_v1(
                &view.object_features
                    [object * OBJECT_FEATURE_DIM_V1..(object + 1) * OBJECT_FEATURE_DIM_V1],
            );
            let token = view.object_card_ids[object] as usize;
            input.extend_from_slice(
                &parameters[CARD_EMBEDDING][token * embedding..(token + 1) * embedding],
            );
            oracle_two_layer_v1(parameters, OBJECT_FIRST_WEIGHT, &input)
        })
        .collect();
    let mut edge_pooled = vec![vec![0.0f64; hidden]; objects];
    for edge in 0..view.edge_source_indices.len() {
        let source = view.edge_source_indices[edge] as usize;
        let target = view.edge_target_indices[edge] as usize;
        let mut input = widen_v1(
            &view.edge_features[edge * EDGE_FEATURE_DIM_V1..(edge + 1) * EDGE_FEATURE_DIM_V1],
        );
        input.extend_from_slice(&base[source]);
        input.extend_from_slice(&base[target]);
        let edge_hidden = oracle_two_layer_v1(parameters, EDGE_FIRST_WEIGHT, &input);
        for row in [source, target] {
            for (pooled, value) in edge_pooled[row].iter_mut().zip(&edge_hidden) {
                *pooled += value;
            }
        }
    }
    let object_hidden: Vec<Vec<f64>> = (0..objects)
        .map(|object| {
            let mut input = base[object].clone();
            input.extend_from_slice(&edge_pooled[object]);
            oracle_two_layer_v1(parameters, NODE_FIRST_WEIGHT, &input)
        })
        .collect();
    let mut pooled = vec![0.0f64; OBJECT_GROUP_COUNT_V1 * hidden];
    for (object, &group) in view.object_groups.iter().enumerate() {
        let group = group as usize;
        for (pooled, value) in pooled[group * hidden..(group + 1) * hidden]
            .iter_mut()
            .zip(&object_hidden[object])
        {
            *pooled += value;
        }
    }
    let mut state_input = widen_v1(view.state);
    state_input.extend_from_slice(&pooled);
    let state_hidden = oracle_two_layer_v1(parameters, STATE_FIRST_WEIGHT, &state_input);

    let actions = view.action_features.len() / ACTION_FEATURE_DIM_V1;
    let mut action_ref_pooled = vec![vec![0.0f64; hidden]; actions];
    for action_ref in 0..view.action_ref_card_ids.len() {
        let mut input = widen_v1(
            &view.action_ref_features[action_ref * ACTION_REF_FEATURE_DIM_V1
                ..(action_ref + 1) * ACTION_REF_FEATURE_DIM_V1],
        );
        input.extend_from_slice(&object_hidden[view.action_ref_node_indices[action_ref] as usize]);
        let ref_hidden = oracle_two_layer_v1(parameters, ACTION_REF_FIRST_WEIGHT, &input);
        let action = view.action_ref_action_indices[action_ref] as usize;
        for (pooled, value) in action_ref_pooled[action].iter_mut().zip(&ref_hidden) {
            *pooled += value;
        }
    }
    let logits = (0..actions)
        .map(|action| {
            let mut input = widen_v1(
                &view.action_features
                    [action * ACTION_FEATURE_DIM_V1..(action + 1) * ACTION_FEATURE_DIM_V1],
            );
            input.extend_from_slice(&action_ref_pooled[action]);
            let action_hidden = oracle_two_layer_v1(parameters, ACTION_FIRST_WEIGHT, &input);
            let mut scorer_input = state_hidden.clone();
            scorer_input.extend_from_slice(&action_hidden);
            let scorer_hidden: Vec<f64> = oracle_linear_v1(
                &parameters[SCORER_FIRST_WEIGHT],
                &parameters[SCORER_FIRST_BIAS],
                &scorer_input,
            )
            .into_iter()
            .map(f64::tanh)
            .collect();
            oracle_linear_v1(
                &parameters[SCORER_SECOND_WEIGHT],
                &parameters[SCORER_SECOND_BIAS],
                &scorer_hidden,
            )[0]
        })
        .collect();
    let value_hidden: Vec<f64> = oracle_linear_v1(
        &parameters[VALUE_FIRST_WEIGHT],
        &parameters[VALUE_FIRST_BIAS],
        &state_hidden,
    )
    .into_iter()
    .map(f64::tanh)
    .collect();
    let value = oracle_linear_v1(
        &parameters[VALUE_SECOND_WEIGHT],
        &parameters[VALUE_SECOND_BIAS],
        &value_hidden,
    )[0];
    (logits, value)
}

fn oracle_log_softmax_v1(logits: &[f64], selected: usize) -> f64 {
    let maximum = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let log_sum = logits
        .iter()
        .map(|logit| (logit - maximum).exp())
        .sum::<f64>()
        .ln();
    logits[selected] - maximum - log_sum
}

/// The GAE objective over the gradient batch with fixed advantages and value
/// targets (the terminal objective is this with targets = terminal returns
/// and the advantages frozen at the start of the update).
fn oracle_loss_v1(
    parameters: &[Vec<f64>],
    hidden: usize,
    embedding: usize,
    decisions: &[GradientDecisionV1; 2],
    targets: &[f64],
    advantages: &[f64],
) -> f64 {
    let outputs = decisions
        .each_ref()
        .map(|decision| oracle_forward_v1(parameters, hidden, embedding, &decision.view_v1(false)));
    let group_count = GRADIENT_BATCH_V1.len() as f64;
    GRADIENT_BATCH_V1
        .iter()
        .enumerate()
        .map(|(group, substeps)| {
            let joint = substeps
                .iter()
                .map(|&(decision, selected)| oracle_log_softmax_v1(&outputs[decision].0, selected))
                .sum::<f64>();
            let error = outputs[substeps[0].0].1 - targets[group];
            (-advantages[group] * joint + f64::from(VC) * error * error) / group_count
        })
        .sum()
}

#[derive(serde::Deserialize)]
struct OracleGoldenFixtureV1 {
    authority: OracleGoldenAuthorityV1,
    cases: Vec<OracleGoldenCaseV1>,
}

#[derive(serde::Deserialize)]
struct OracleGoldenAuthorityV1 {
    absolute_tolerance: f64,
    relative_tolerance: f64,
}

#[derive(serde::Deserialize)]
struct OracleGoldenCaseV1 {
    state: Vec<f32>,
    object_features: Vec<f32>,
    object_card_ids: Vec<i64>,
    object_groups: Vec<i64>,
    object_node_ids: Vec<i64>,
    edge_features: Vec<f32>,
    edge_source_indices: Vec<i64>,
    edge_target_indices: Vec<i64>,
    action_features: Vec<f32>,
    action_ref_features: Vec<f32>,
    action_ref_card_ids: Vec<i64>,
    action_ref_action_indices: Vec<i64>,
    action_ref_node_indices: Vec<i64>,
    torch_logits: Vec<f32>,
    torch_value: f32,
}

fn parameters_f64_v1(parameters: &[NativeNamedParameterV1]) -> Vec<Vec<f64>> {
    parameters
        .iter()
        .map(|parameter| widen_v1(&parameter.values))
        .collect()
}

/// The oracle reproduces the narrow and wide Torch forward goldens within
/// their declared tolerances before it judges any gradient.
#[test]
fn f64_oracle_reproduces_the_narrow_and_wide_torch_forward_goldens() {
    for (json, parameters, hidden, embedding) in [
        (
            include_str!("../../../../data/native_policy_value_net_v1/runner_fixed_forward_goldens_v1.json"),
            narrow_model_v1().parameter_snapshot_v1(),
            HIDDEN_DIM_V1,
            CARD_EMBEDDING_DIM_V1,
        ),
        (
            include_str!(
                "../../../../data/native_policy_value_net_wide_v1/runner_fixed_forward_goldens_wide_v1.json"
            ),
            wide_model_v1().parameter_snapshot_wide_v1(),
            W_HIDDEN_DIM_V1,
            W_CARD_EMBEDDING_DIM_V1,
        ),
    ] {
        let fixture: OracleGoldenFixtureV1 = serde_json::from_str(json).unwrap();
        let parameters = parameters_f64_v1(&parameters);
        assert_eq!(fixture.cases.len(), 2);
        for case in &fixture.cases {
            let view = NativeEncodedDecisionViewV1::from_slices_unvalidated(
                NativeEncodedDecisionSchemaV1::contract_v1(),
                &case.state,
                &case.object_features,
                &case.object_card_ids,
                &case.object_groups,
                &case.object_node_ids,
                &case.edge_features,
                &case.edge_source_indices,
                &case.edge_target_indices,
                &case.action_features,
                &case.action_ref_features,
                &case.action_ref_card_ids,
                &case.action_ref_action_indices,
                &case.action_ref_node_indices,
            );
            let (logits, value) = oracle_forward_v1(&parameters, hidden, embedding, &view);
            let close = |actual: f64, expected: f32| {
                let expected = f64::from(expected);
                (actual - expected).abs()
                    <= fixture.authority.absolute_tolerance
                        + fixture.authority.relative_tolerance * expected.abs()
            };
            assert_eq!(logits.len(), case.torch_logits.len());
            for (actual, expected) in logits.iter().zip(&case.torch_logits) {
                assert!(close(*actual, *expected), "h{hidden}: {actual} vs {expected}");
            }
            assert!(close(value, case.torch_value), "h{hidden}: {value}");
        }
    }
}

// ---------------------------------------------------------------------------
// Witness blocks (design section 5.1): drawn from the inputs and the layout,
// never from Rust's gradients.
// ---------------------------------------------------------------------------

/// One concatenation segment of a layer input.
#[derive(Clone, Copy)]
enum SegmentV1 {
    Fixed(usize),
    Hidden,
    Embedding,
}

/// The input segments of each tensor's columns; biases have none.
fn column_segments_v1(tensor: usize) -> Vec<SegmentV1> {
    use SegmentV1::{Embedding, Fixed, Hidden};
    match tensor {
        CARD_EMBEDDING => vec![Embedding],
        OBJECT_FIRST_WEIGHT => vec![Fixed(OBJECT_FEATURE_DIM_V1), Embedding],
        EDGE_FIRST_WEIGHT => vec![Fixed(EDGE_FEATURE_DIM_V1), Hidden, Hidden],
        NODE_FIRST_WEIGHT | SCORER_FIRST_WEIGHT => vec![Hidden, Hidden],
        STATE_FIRST_WEIGHT => std::iter::once(Fixed(STATE_DIM_V1))
            .chain(std::iter::repeat_n(Hidden, OBJECT_GROUP_COUNT_V1))
            .collect(),
        ACTION_REF_FIRST_WEIGHT => vec![Fixed(ACTION_REF_FEATURE_DIM_V1), Hidden],
        ACTION_FIRST_WEIGHT => vec![Fixed(ACTION_FEATURE_DIM_V1), Hidden],
        tensor if EXPECTED_PARAMETER_SHAPES[tensor].len() == 2 => vec![Hidden],
        _ => Vec::new(),
    }
}

fn segment_width_v1(segment: SegmentV1, hidden: usize, embedding: usize) -> usize {
    match segment {
        SegmentV1::Fixed(width) => width,
        SegmentV1::Hidden => hidden,
        SegmentV1::Embedding => embedding,
    }
}

/// Narrow-to-wide layout map (design section 5.2, reused by PR 4a's
/// widening): the wide element a narrow element occupies when net-8 is
/// embedded in net-8w128 with every new coordinate zero.
fn narrow_to_wide_index_v1(tensor: usize, index: usize) -> usize {
    let narrow_shape = EXPECTED_PARAMETER_SHAPES[tensor];
    let wide_shape = W_EXPECTED_PARAMETER_SHAPES[tensor];
    if narrow_shape.len() == 1 {
        return index;
    }
    let (row, mut column) = (index / narrow_shape[1], index % narrow_shape[1]);
    let mut wide_offset = 0;
    for segment in column_segments_v1(tensor) {
        let narrow_width = segment_width_v1(segment, HIDDEN_DIM_V1, CARD_EMBEDDING_DIM_V1);
        if column < narrow_width {
            return row * wide_shape[1] + wide_offset + column;
        }
        column -= narrow_width;
        wide_offset += segment_width_v1(segment, W_HIDDEN_DIM_V1, W_CARD_EMBEDDING_DIM_V1);
    }
    unreachable!("column inside the narrow layout")
}

#[test]
fn narrow_to_wide_layout_map_is_injective_and_shape_consistent() {
    for tensor in 0..PARAMETER_TENSOR_COUNT {
        let narrow_count: usize = EXPECTED_PARAMETER_SHAPES[tensor].iter().product();
        let wide_count: usize = W_EXPECTED_PARAMETER_SHAPES[tensor].iter().product();
        let mut seen = vec![false; wide_count];
        for index in 0..narrow_count {
            let wide = narrow_to_wide_index_v1(tensor, index);
            assert!(
                wide < wide_count && !seen[wide],
                "tensor {tensor} index {index}"
            );
            seen[wide] = true;
        }
        // Each segment's column count adds up to the layer input width.
        if EXPECTED_PARAMETER_SHAPES[tensor].len() == 2 {
            for (shape, hidden, embedding) in [
                (
                    EXPECTED_PARAMETER_SHAPES[tensor],
                    HIDDEN_DIM_V1,
                    CARD_EMBEDDING_DIM_V1,
                ),
                (
                    W_EXPECTED_PARAMETER_SHAPES[tensor],
                    W_HIDDEN_DIM_V1,
                    W_CARD_EMBEDDING_DIM_V1,
                ),
            ] {
                let width: usize = column_segments_v1(tensor)
                    .into_iter()
                    .map(|segment| segment_width_v1(segment, hidden, embedding))
                    .sum();
                assert_eq!(width, shape[1], "tensor {tensor}");
            }
        }
    }
}

/// A set of coordinates of one tensor: `rows x columns`.
struct BlockV1 {
    name: String,
    tensor: usize,
    rows: Vec<usize>,
    columns: Vec<usize>,
    new: bool,
}

/// Every witness block of an architecture over the gradient batch. Old blocks
/// hold the coordinates net-8 also has; new blocks are those the widening
/// adds: hidden rows and columns >= 64, the new columns of every
/// concatenation segment (state pools of populated groups only) and the
/// embedding columns >= 16 of present card ids.
fn witness_blocks_v1(hidden: usize, embedding: usize) -> Vec<BlockV1> {
    let shapes = if hidden == HIDDEN_DIM_V1 {
        EXPECTED_PARAMETER_SHAPES
    } else {
        W_EXPECTED_PARAMETER_SHAPES
    };
    let present_ids: Vec<usize> = {
        let mut ids: Vec<usize> = GRADIENT_CARD_IDS_V1
            .iter()
            .filter(|&&id| id != 0)
            .map(|&id| id as usize)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    };
    let populated_groups: Vec<usize> = {
        let mut groups: Vec<usize> = GRADIENT_GROUPS_V1.iter().map(|&g| g as usize).collect();
        groups.sort_unstable();
        groups.dedup();
        groups
    };
    let old_hidden = HIDDEN_DIM_V1.min(hidden);
    let mut blocks = Vec::new();
    for (tensor, shape) in shapes.iter().enumerate() {
        if tensor == SCORER_SECOND_BIAS {
            // The canonical gauge: zero gradient by construction.
            continue;
        }
        let name = EXPECTED_PARAMETER_NAMES[tensor];
        let (old_rows, new_rows): (Vec<usize>, Vec<usize>) = if tensor == CARD_EMBEDDING {
            (present_ids.clone(), Vec::new())
        } else if shape[0] == hidden {
            ((0..old_hidden).collect(), (old_hidden..hidden).collect())
        } else {
            ((0..shape[0]).collect(), Vec::new())
        };
        // Old and new columns by segment; state pools of empty groups are
        // unreachable from the batch.
        let mut old_columns = Vec::new();
        let mut new_segments: Vec<(String, Vec<usize>)> = Vec::new();
        let mut offset = 0;
        let segments = column_segments_v1(tensor);
        if segments.is_empty() {
            old_columns.push(0);
        }
        for (segment_index, segment) in segments.into_iter().enumerate() {
            let width = segment_width_v1(segment, hidden, embedding);
            let old_width = segment_width_v1(segment, HIDDEN_DIM_V1, CARD_EMBEDDING_DIM_V1);
            let reachable = tensor != STATE_FIRST_WEIGHT
                || segment_index == 0
                || populated_groups.contains(&(segment_index - 1));
            if reachable {
                old_columns.extend(offset..offset + old_width.min(width));
                if width > old_width {
                    new_segments.push((
                        format!("{name} segment {segment_index} new columns"),
                        (offset + old_width..offset + width).collect(),
                    ));
                }
            }
            offset += width;
        }
        let all_columns: Vec<usize> = old_columns
            .iter()
            .copied()
            .chain(
                new_segments
                    .iter()
                    .flat_map(|(_, columns)| columns.iter().copied()),
            )
            .collect();
        blocks.push(BlockV1 {
            name: format!("{name} old"),
            tensor,
            rows: old_rows.clone(),
            columns: old_columns,
            new: false,
        });
        if !new_rows.is_empty() {
            blocks.push(BlockV1 {
                name: format!("{name} rows >= {old_hidden}"),
                tensor,
                rows: new_rows,
                columns: all_columns,
                new: true,
            });
        }
        for (block_name, columns) in new_segments {
            blocks.push(BlockV1 {
                name: block_name,
                tensor,
                rows: old_rows.clone(),
                columns,
                new: true,
            });
        }
    }
    blocks
}

/// Up to `count` distinct seeded coordinates of `block`, as flat indices.
fn draw_witnesses_v1(
    block: &BlockV1,
    width: usize,
    count: usize,
    rng: &mut SplitMix64,
) -> Vec<usize> {
    let total = block.rows.len() * block.columns.len();
    let mut drawn = Vec::new();
    let mut attempts = 0;
    while drawn.len() < count.min(total) && attempts < 64 * count {
        attempts += 1;
        let pick = (rng.next_u64() % total as u64) as usize;
        let index = block.rows[pick / block.columns.len()] * width
            + block.columns[pick % block.columns.len()];
        if !drawn.contains(&index) {
            drawn.push(index);
        }
    }
    drawn
}

/// Which objective a gradient check differentiates.
#[derive(Clone, Copy, Debug)]
enum ObjectiveV1 {
    /// `train_step_gae_feature_transfer_v3`: fixed advantages and targets.
    Gae,
    /// `train_step_feature_transfer_v3`: terminal returns as targets, the
    /// advantages frozen at the start of the update.
    Terminal,
}

struct FdReportV1 {
    witnesses: usize,
    max_ratio: f64,
    counted_new: BTreeMap<String, usize>,
    outside: Vec<String>,
}

/// Rust's gradient for one update at `model` against central differences of
/// the f64 oracle on the drawn witnesses, with the padding row checked by
/// bits before and after the update.
fn fd_check_dims_v1<D: NetDimsV1>(model: &D::Net, objective: ObjectiveV1) -> FdReportV1
where
    D::Net: RecordingNetV1,
{
    let decisions = gradient_decisions_v1();
    let recorded = record_gradient_decisions_v1(model, &decisions, false);
    let steps = gradient_substeps_v1(&decisions, &recorded, false);
    let groups = gradient_groups_v1(&steps);
    let mut state = NativePolicyValueTrainStateDimsV1::<D>::new_dims_v1(model.clone()).unwrap();
    let padding_is_positive_zero = |parameters: &[NativeNamedParameterV1]| {
        parameters[CARD_EMBEDDING].values[..D::CARD_EMBEDDING]
            .iter()
            .all(|value| value.to_bits() == 0)
    };
    assert!(padding_is_positive_zero(&model.parameter_snapshot_v1()));
    let (result, targets, advantages) = match objective {
        ObjectiveV1::Gae => (
            state
                .train_step_gae_feature_transfer_v3_dims_v1(
                    &groups,
                    &GRADIENT_TARGETS_V1,
                    &GRADIENT_ADVANTAGES_V1,
                    VC,
                    LR,
                )
                .unwrap(),
            GRADIENT_TARGETS_V1.map(f64::from).to_vec(),
            GRADIENT_ADVANTAGES_V1.map(f64::from).to_vec(),
        ),
        ObjectiveV1::Terminal => {
            let returns = GRADIENT_TERMINAL_RETURNS_V1.map(f32::from);
            // The step's own advantage: return minus the recorded value.
            let advantages: Vec<f64> = GRADIENT_BATCH_V1
                .iter()
                .zip(returns)
                .map(|(substeps, target)| {
                    f64::from(target - f32::from_bits(recorded[substeps[0].0].value))
                })
                .collect();
            (
                state
                    .train_step_feature_transfer_v3_dims_v1(&groups, VC, LR)
                    .unwrap(),
                returns.map(f64::from).to_vec(),
                advantages,
            )
        }
    };
    assert!(padding_is_positive_zero(
        &state.model_dims_v1().parameter_snapshot_v1()
    ));
    let mut parameters = parameters_f64_v1(&model.parameter_snapshot_v1());
    let loss = |parameters: &[Vec<f64>]| {
        oracle_loss_v1(
            parameters,
            D::HIDDEN,
            D::CARD_EMBEDDING,
            &decisions,
            &targets,
            &advantages,
        )
    };
    let wide = !is_net8_dims_v1::<D>();
    let mut rng = SplitMix64::seed(0x5D1F_F0AC);
    let mut report = FdReportV1 {
        witnesses: 0,
        max_ratio: 0.0,
        counted_new: BTreeMap::new(),
        outside: Vec::new(),
    };
    for block in witness_blocks_v1(D::HIDDEN, D::CARD_EMBEDDING) {
        let width = D::PARAMETER_SHAPES[block.tensor]
            .get(1)
            .copied()
            .unwrap_or(1);
        let draws = match (wide, block.new) {
            (true, true) => NEW_BLOCK_DRAWS_V1,
            (true, false) => OLD_BLOCK_DRAWS_V1,
            (false, _) => NARROW_BLOCK_DRAWS_V1,
        };
        if block.new {
            report.counted_new.insert(block.name.clone(), 0);
        }
        for index in draw_witnesses_v1(&block, width, draws, &mut rng) {
            let original = parameters[block.tensor][index];
            parameters[block.tensor][index] = original + FD_STEP_V1;
            let plus = loss(&parameters);
            parameters[block.tensor][index] = original - FD_STEP_V1;
            let minus = loss(&parameters);
            parameters[block.tensor][index] = original;
            let numerical = (plus - minus) / (2.0 * FD_STEP_V1);
            let analytic = f64::from(result.gradients[block.tensor].values[index]);
            let allowance = FD_ABSOLUTE_V1 + FD_RELATIVE_V1 * numerical.abs();
            let ratio = (analytic - numerical).abs() / allowance;
            report.witnesses += 1;
            report.max_ratio = report.max_ratio.max(ratio);
            if ratio > 1.0 {
                report.outside.push(format!(
                    "{} [{index}]: rust {analytic:e} oracle {numerical:e}",
                    block.name
                ));
            }
            if block.new && numerical.abs() > FD_DETECTION_FACTOR_V1 * allowance {
                *report.counted_new.get_mut(&block.name).unwrap() += 1;
            }
        }
    }
    report
}

/// Design section 5.1, gating: on a seeded perturbation of the runner-fixed
/// wide model, every drawn coordinate of Rust's GAE and terminal gradients is
/// within the frozen tolerance of the f64 central differences, every block
/// the widening adds holds three witnesses with detection power, and wide's
/// largest error ratio is at most four times net-8's on the same inputs.
#[test]
fn wide_gradients_match_f64_central_differences_with_detection_power() {
    let narrow = perturbed_dims_v1::<Net8DimsV1>(narrow_model_v1(), 0xA11C_E5ED);
    let wide = perturbed_dims_v1::<Net8W128DimsV1>(wide_model_v1(), 0xA11C_E5ED);
    let mut receipt = Vec::new();
    for objective in [ObjectiveV1::Gae, ObjectiveV1::Terminal] {
        let narrow_report = fd_check_dims_v1::<Net8DimsV1>(&narrow, objective);
        assert!(
            narrow_report.outside.is_empty(),
            "narrow calibration {objective:?}: {:?}",
            narrow_report.outside
        );
        let r_n = narrow_report.max_ratio;
        let wide_report = fd_check_dims_v1::<Net8W128DimsV1>(&wide, objective);
        receipt.push(format!(
            "{objective:?}: r_n={r_n:.6e} narrow_witnesses={} wide_max_ratio={:.6e} \
             wide_witnesses={} wide_over_r_n={:.4} min_counted_new={}",
            narrow_report.witnesses,
            wide_report.max_ratio,
            wide_report.witnesses,
            wide_report.max_ratio / r_n,
            wide_report.counted_new.values().min().copied().unwrap_or(0),
        ));
        assert!(
            wide_report.outside.is_empty(),
            "wide {objective:?} outside tolerance: {:?}",
            wide_report.outside
        );
        let weak: Vec<_> = wide_report
            .counted_new
            .iter()
            .filter(|(_, &count)| count < FD_COUNTED_PER_NEW_BLOCK_V1)
            .collect();
        assert!(
            weak.is_empty(),
            "wide {objective:?} blocks without detection power (enlarge the fixture): {weak:?}"
        );
        assert!(
            wide_report.max_ratio <= FD_NARROW_RATIO_FACTOR_V1 * r_n,
            "wide {objective:?} max ratio {} above {FD_NARROW_RATIO_FACTOR_V1} r_n = {}",
            wide_report.max_ratio,
            FD_NARROW_RATIO_FACTOR_V1 * r_n
        );
    }
    // Written past the test harness's capture so CI logs carry the receipt.
    let mut stderr = std::io::stderr().lock();
    for line in receipt {
        writeln!(stderr, "WIDE_FD_RECEIPT_V1 {line}").unwrap();
    }
}

// ---------------------------------------------------------------------------
// Zero-padding equivalence (design section 5.2).
// ---------------------------------------------------------------------------

/// The runner-fixed net-8 embedded in the wide layout, every new coordinate
/// +0.
fn zero_padded_wide_v1(narrow: &NativePolicyValueNetV1) -> NativePolicyValueNetWideV1 {
    let mut wide = wide_model_v1();
    let mut parameters = wide.parameter_snapshot_wide_v1();
    for parameter in &mut parameters {
        parameter.values.fill(0.0);
    }
    for (tensor, parameter) in narrow.parameter_snapshot_v1().iter().enumerate() {
        for (index, &value) in parameter.values.iter().enumerate() {
            parameters[tensor].values[narrow_to_wide_index_v1(tensor, index)] = value;
        }
    }
    wide.replace_parameter_snapshot_wide_v1(&parameters)
        .unwrap();
    wide
}

/// Every narrow element equals its wide image under IEEE `==`; every other
/// wide element is `+0` by bits (or, for gradients, zero under `==`).
fn assert_zero_padded_v1(
    label: &str,
    narrow: &[NativeNamedParameterV1],
    wide: &[NativeNamedParameterV1],
    new_must_be_positive_zero: bool,
) {
    for (tensor, (narrow, wide)) in narrow.iter().zip(wide).enumerate() {
        let mut image = vec![false; wide.values.len()];
        for (index, &value) in narrow.values.iter().enumerate() {
            let wide_index = narrow_to_wide_index_v1(tensor, index);
            image[wide_index] = true;
            assert!(
                wide.values[wide_index] == value,
                "{label} {}[{index}]: wide {} narrow {value}",
                narrow.name,
                wide.values[wide_index]
            );
        }
        for (index, &value) in wide.values.iter().enumerate() {
            if image[index] {
                continue;
            }
            if new_must_be_positive_zero {
                assert_eq!(value.to_bits(), 0, "{label} {} new [{index}]", wide.name);
            } else {
                assert!(value == 0.0, "{label} {} new [{index}]: {value}", wide.name);
            }
        }
    }
}

/// One update's result restricted to net-8's coordinates equals net-8's.
fn assert_results_zero_padded_v1(
    label: &str,
    narrow: &NativePolicyTrainStepResultV1,
    wide: &NativePolicyTrainStepResultV1,
) {
    assert_eq!(narrow.loss.to_bits(), wide.loss.to_bits(), "{label} loss");
    assert_eq!(
        narrow.policy_sum.to_bits(),
        wide.policy_sum.to_bits(),
        "{label}"
    );
    assert_eq!(
        narrow.value_sum.to_bits(),
        wide.value_sum.to_bits(),
        "{label}"
    );
    assert_eq!(narrow.adam_step, wide.adam_step, "{label}");
    assert_eq!(narrow.selected_outputs, wide.selected_outputs, "{label}");
    assert_eq!(narrow.physical_terms, wide.physical_terms, "{label}");
    assert_eq!(
        narrow.scorer_bias_gauge, wide.scorer_bias_gauge,
        "{label} gauge"
    );
    assert_zero_padded_v1(label, &narrow.gradients, &wide.gradients, false);
}

fn assert_states_zero_padded_v1(
    label: &str,
    narrow: &NativePolicyValueTrainStateV1,
    wide: &NativePolicyValueTrainStateWideV1,
) {
    let (narrow, wide) = (narrow.snapshot_v1().unwrap(), wide.snapshot_v1().unwrap());
    assert_eq!(narrow.adam_step, wide.adam_step, "{label}");
    assert_eq!(
        narrow.scorer_bias_anchor_bits, wide.scorer_bias_anchor_bits,
        "{label}"
    );
    assert_zero_padded_v1(label, &narrow.parameters, &wide.parameters, true);
    assert_zero_padded_v1(label, &narrow.first_moments, &wide.first_moments, true);
    assert_zero_padded_v1(label, &narrow.second_moments, &wide.second_moments, true);
}

/// Real Map V3 decisions, 100 groups of one to three substeps (199), for the
/// worker-parallel routes.
struct ManyFixtureV1 {
    tensors: [crate::native_flat_tensorizer_v3::NativeFlatDecisionTensorV3; 2],
    recorded: [RecordedV1; 2],
}

impl ManyFixtureV1 {
    fn of_v1<N: RecordingNetV1>(model: &N) -> Self {
        let first = real_map_training_tensor_v3();
        let mut second = first.clone();
        second.common.state[0] += 0.125;
        let tensors = [first, second];
        let recorded = tensors.each_ref().map(|tensor| {
            recorded_v1(model.forward_v3_for_test_v1(encoded_decision_view_v3(tensor)))
        });
        Self { tensors, recorded }
    }

    fn substeps_v1(&self, group_count: usize) -> Vec<Vec<NativePolicySubstepV1<'_>>> {
        (0..group_count)
            .map(|group| {
                (0..1 + group % 3)
                    .map(|substep| {
                        let tensor = (group + substep) % 2;
                        NativePolicySubstepV1 {
                            forward: NativePolicyForwardInputV1::Encoded(Box::new(
                                encoded_decision_view_v3(&self.tensors[tensor]),
                            )),
                            selected_action_index: (group * 7 + substep)
                                % self.recorded[tensor].logits.len(),
                            expected_raw_action_logit_bits: &self.recorded[tensor].logits,
                            expected_value_bits: self.recorded[tensor].value,
                        }
                    })
                    .collect()
            })
            .collect()
    }
}

fn many_groups_v1<'a>(
    steps: &'a [Vec<NativePolicySubstepV1<'a>>],
) -> Vec<NativePolicyPhysicalDecisionV1<'a>> {
    steps
        .iter()
        .enumerate()
        .map(|(group, substeps)| NativePolicyPhysicalDecisionV1 {
            substeps,
            terminal_return: [1, -1, 0][group % 3],
            baseline_bits: 0,
        })
        .collect()
}

fn many_targets_v1(group_count: usize) -> (Vec<f32>, Vec<f32>) {
    let targets = (0..group_count)
        .map(|group| 0.25 - 0.01 * group as f32)
        .collect();
    let advantages = (0..group_count)
        .map(|group| {
            let sign = if group % 2 == 0 { 0.5 } else { -0.375 };
            sign / (1.0 + group as f32)
        })
        .collect();
    (targets, advantages)
}

/// The wide routes of the generic core, each one update of one state.
#[derive(Clone, Copy, Debug)]
enum WideRouteV1 {
    TerminalSequential,
    TerminalFixedPartitions,
    GaeSequential,
    GaeExactParallel,
    GaeFixedPartitions,
}

fn route_update_dims_v1<D: NetDimsV1>(
    state: &mut NativePolicyValueTrainStateDimsV1<D>,
    route: WideRouteV1,
) -> NativePolicyTrainStepResultV1
where
    D::Net: RecordingNetV1,
{
    let model = state.model_dims_v1().clone();
    match route {
        WideRouteV1::TerminalSequential
        | WideRouteV1::TerminalFixedPartitions
        | WideRouteV1::GaeSequential => {
            let contract = matches!(route, WideRouteV1::TerminalFixedPartitions);
            let decisions = gradient_decisions_v1();
            let recorded = record_gradient_decisions_v1(&model, &decisions, contract);
            let steps = gradient_substeps_v1(&decisions, &recorded, contract);
            let groups = gradient_groups_v1(&steps);
            match route {
                WideRouteV1::TerminalSequential => state
                    .train_step_feature_transfer_v3_dims_v1(&groups, VC, LR)
                    .unwrap(),
                WideRouteV1::TerminalFixedPartitions => state
                    .train_step_with_fixed_partition_parallel_backward_dims_v1(
                        &groups, VC, LR, 1, 4,
                    )
                    .unwrap(),
                _ => state
                    .train_step_gae_feature_transfer_v3_dims_v1(
                        &groups,
                        &GRADIENT_TARGETS_V1,
                        &GRADIENT_ADVANTAGES_V1,
                        VC,
                        LR,
                    )
                    .unwrap(),
            }
        }
        WideRouteV1::GaeExactParallel | WideRouteV1::GaeFixedPartitions => {
            let fixture = ManyFixtureV1::of_v1(&model);
            let steps = fixture.substeps_v1(100);
            let groups = many_groups_v1(&steps);
            let (targets, advantages) = many_targets_v1(groups.len());
            let (backward, forward_workers) = match route {
                WideRouteV1::GaeExactParallel => (None, 4),
                _ => (Some(4), 1),
            };
            state
                .train_step_gae_feature_transfer_profiled_dims_v1(
                    FreshLineageGenerationV1::V3,
                    &groups,
                    &targets,
                    &advantages,
                    VC,
                    LR,
                    backward,
                    forward_workers,
                    &mut NativeTrainingPhaseRecorderV1::disabled_v1(),
                )
                .unwrap()
        }
    }
}

/// Design section 5.2 (exact, strongest index check): net-8 embedded in the
/// wide layout with every new coordinate zero gives net-8's logits and value
/// on every fixture case, and after three updates on each wide route net-8's
/// gradients, moments, parameters, Adam step, loss terms and gauge evidence
/// on the old coordinates, with every new coordinate's gradient zero and its
/// moments and parameters +0 by bits.
#[test]
fn zero_padded_wide_reproduces_net8_on_every_route() {
    let narrow = narrow_model_v1();
    let wide = zero_padded_wide_v1(&narrow);
    let decisions = gradient_decisions_v1();
    let many = ManyFixtureV1::of_v1(&narrow);
    let mut views: Vec<_> = decisions.iter().map(|d| d.view_v1(false)).collect();
    views.extend(many.tensors.iter().map(encoded_decision_view_v3));
    for view in views {
        let (narrow_output, wide_output) = (
            narrow.forward_v3_for_test_v1(view),
            wide.forward_v3_for_test_v1(view),
        );
        assert_eq!(narrow_output.logits.len(), wide_output.logits.len());
        for (narrow_logit, wide_logit) in narrow_output.logits.iter().zip(&wide_output.logits) {
            assert!(narrow_logit == wide_logit, "{narrow_logit} vs {wide_logit}");
        }
        assert!(narrow_output.value == wide_output.value);
    }
    for decision in &decisions {
        let (narrow_output, wide_output) = (
            narrow.forward_contract_for_test_v1(decision.view_v1(true)),
            wide.forward_contract_for_test_v1(decision.view_v1(true)),
        );
        assert!(narrow_output.logits == wide_output.logits);
        assert!(narrow_output.value == wide_output.value);
    }
    for route in [
        WideRouteV1::TerminalSequential,
        WideRouteV1::TerminalFixedPartitions,
        WideRouteV1::GaeSequential,
        WideRouteV1::GaeExactParallel,
        WideRouteV1::GaeFixedPartitions,
    ] {
        let mut narrow_state = NativePolicyValueTrainStateV1::new_v1(narrow.clone()).unwrap();
        let mut wide_state = NativePolicyValueTrainStateWideV1::new_wide_v1(wide.clone()).unwrap();
        for update in 0..3 {
            let label = format!("{route:?} update {update}");
            let narrow_result = route_update_dims_v1(&mut narrow_state, route);
            let wide_result = route_update_dims_v1(&mut wide_state, route);
            assert_results_zero_padded_v1(&label, &narrow_result, &wide_result);
            assert_states_zero_padded_v1(&label, &narrow_state, &wide_state);
        }
    }
}

// ---------------------------------------------------------------------------
// Determinism and resume (design section 5.5).
// ---------------------------------------------------------------------------

fn gradient_bits_v1(result: &NativePolicyTrainStepResultV1) -> Vec<u32> {
    result
        .gradients
        .iter()
        .flat_map(|gradient| gradient.values.iter().map(|value| value.to_bits()))
        .collect()
}

fn wide_perturbed_state_v1() -> NativePolicyValueTrainStateWideV1 {
    NativePolicyValueTrainStateWideV1::new_wide_v1(perturbed_dims_v1::<Net8W128DimsV1>(
        wide_model_v1(),
        0x0D37_E7A1,
    ))
    .unwrap()
}

/// On wide, the GAE update's gradients and state bits are identical across
/// forward workers 1, 2, 4 and 8, with the exact parallel backward (two or
/// more workers) or without it (one), and the fixed partitions across worker
/// limits 1 to 4.
#[test]
fn wide_gae_updates_are_identical_across_workers() {
    let start = wide_perturbed_state_v1();
    let fixture = ManyFixtureV1::of_v1(start.model_v1());
    let steps = fixture.substeps_v1(100);
    let groups = many_groups_v1(&steps);
    let (targets, advantages) = many_targets_v1(groups.len());
    let run = |backward: Option<usize>, forward_workers: usize| {
        let mut state = start.clone();
        take_effective_update_workers_for_test_v1();
        let result = state
            .train_step_gae_feature_transfer_profiled_dims_v1(
                FreshLineageGenerationV1::V3,
                &groups,
                &targets,
                &advantages,
                VC,
                LR,
                backward,
                forward_workers,
                &mut NativeTrainingPhaseRecorderV1::disabled_v1(),
            )
            .unwrap();
        let workers = take_effective_update_workers_for_test_v1();
        (
            format!("{result:?}"),
            gradient_bits_v1(&result),
            state.state_sha256_v1().unwrap(),
            workers,
        )
    };
    let reference = run(None, 1);
    assert_eq!(reference.3, (1, 1));
    for forward_workers in [2, 4, 8] {
        let actual = run(None, forward_workers);
        assert_eq!(actual.3, (forward_workers.min(6), forward_workers.min(6)));
        assert_eq!(actual.0, reference.0, "{forward_workers}");
        assert_eq!(actual.1, reference.1, "{forward_workers}");
        assert_eq!(actual.2, reference.2, "{forward_workers}");
    }
    let partitions = run(Some(1), 1);
    for worker_limit in 2..=4 {
        let actual = run(Some(worker_limit), 1);
        assert_eq!(actual.0, partitions.0, "{worker_limit}");
        assert_eq!(actual.1, partitions.1, "{worker_limit}");
        assert_eq!(actual.2, partitions.2, "{worker_limit}");
    }
}

/// On wide, 3 + 3 updates resumed through the owned snapshot, and through
/// the wide payload codec, equal 6 straight updates; the generic live hash
/// equals the wide owned-snapshot hash at every step.
#[test]
fn wide_resume_through_snapshot_and_payload_equals_straight_updates() {
    let start = wide_perturbed_state_v1();
    let template = start.model_v1().clone();
    let update = |state: &mut NativePolicyValueTrainStateWideV1| {
        let result = route_update_dims_v1(state, WideRouteV1::GaeSequential);
        let snapshot = state.snapshot_v1().unwrap();
        assert_eq!(
            state.state_sha256_v1().unwrap(),
            wide_owned_train_snapshot_state_sha256_v1(&snapshot).unwrap()
        );
        result
    };
    let mut straight = start.clone();
    let straight_results: Vec<_> = (0..6).map(|_| update(&mut straight)).collect();

    let mut first_half = start.clone();
    for expected in &straight_results[..3] {
        assert_eq!(&update(&mut first_half), expected);
    }
    let snapshot = first_half.snapshot_v1().unwrap();
    let mut resumed =
        NativePolicyValueTrainStateWideV1::from_snapshot_wide_v1(template.clone(), &snapshot)
            .unwrap();
    let payload = encode_native_train_state_payload_wide_v1(&snapshot).unwrap();
    let decoded = decode_native_train_state_payload_wide_v1(
        &payload.bytes,
        snapshot.adam_step,
        snapshot.scorer_bias_anchor_bits,
    )
    .unwrap();
    assert_eq!(decoded.snapshot, snapshot);
    let mut decoded =
        NativePolicyValueTrainStateWideV1::from_snapshot_wide_v1(template, &decoded.snapshot)
            .unwrap();
    for expected in &straight_results[3..] {
        assert_eq!(&update(&mut resumed), expected);
        assert_eq!(&update(&mut decoded), expected);
    }
    for state in [&resumed, &decoded] {
        assert_eq!(
            state.state_sha256_v1().unwrap(),
            straight.state_sha256_v1().unwrap()
        );
        assert_eq!(
            state.snapshot_v1().unwrap(),
            straight.snapshot_v1().unwrap()
        );
    }
}

// ---------------------------------------------------------------------------
// Timing probes (design section 6), ignored: run them explicitly against two
// binaries, interleaved, to compare the forward-loss and backward phases.
// ---------------------------------------------------------------------------

fn timing_probe_dims_v1<D: NetDimsV1>(label: &str, model: D::Net)
where
    D::Net: RecordingNetV1,
{
    let start = NativePolicyValueTrainStateDimsV1::<D>::new_dims_v1(model).unwrap();
    let fixture = ManyFixtureV1::of_v1(start.model_dims_v1());
    let steps = fixture.substeps_v1(200);
    let groups = many_groups_v1(&steps);
    let (targets, advantages) = many_targets_v1(groups.len());
    let mut stdout = std::io::stdout().lock();
    for repetition in 0..12 {
        let mut state = start.clone();
        let mut profile =
            crate::native_training_phase_diagnostic_v1::NativeTrainingPhaseProfileV1::default();
        state
            .train_step_gae_feature_transfer_profiled_dims_v1(
                FreshLineageGenerationV1::V3,
                &groups,
                &targets,
                &advantages,
                VC,
                LR,
                None,
                2,
                &mut NativeTrainingPhaseRecorderV1::enabled_v1(&mut profile),
            )
            .unwrap();
        writeln!(
            stdout,
            "{}",
            serde_json::json!({
                "probe": "gae-update-399-substeps-v1",
                "architecture": label,
                "repetition": repetition,
                "forward_loss_ns": profile.phase_elapsed_ns_v1(NativeTrainingPhaseV1::ForwardLoss),
                "backward_gauge_ns": profile.phase_elapsed_ns_v1(NativeTrainingPhaseV1::BackwardGauge),
                "adam_ns": profile.phase_elapsed_ns_v1(NativeTrainingPhaseV1::AdamMath),
                "setup_ns": profile.phase_elapsed_ns_v1(NativeTrainingPhaseV1::SetupValidation),
                "finalization_ns": profile.phase_elapsed_ns_v1(NativeTrainingPhaseV1::FinalizationCloning),
            })
        )
        .unwrap();
    }
}

#[test]
#[ignore = "timing probe (design section 6): run explicitly, never a correctness gate"]
fn timing_probe_gae_update_narrow_v1() {
    timing_probe_dims_v1::<Net8DimsV1>("net-8", narrow_model_v1());
}

#[test]
#[ignore = "timing probe (design section 6): run explicitly, never a correctness gate"]
fn timing_probe_gae_update_wide_v1() {
    timing_probe_dims_v1::<Net8W128DimsV1>("net-8w128", wide_model_v1());
}

// ---------------------------------------------------------------------------
// Torch authority train golden (design section 5.3).
// ---------------------------------------------------------------------------

/// The wide twin of the narrow Torch authority train golden, with the narrow
/// fixture's declared tolerances. Its fixture is generated only on the
/// authority tuple (Windows/AMD64, Python 3.13.14, Torch 2.13.0+cpu, one
/// thread) by `python/tools/generate_native_policy_train_step_wide_v1_goldens.py`;
/// until it lands this test is ignored, and no wide fit or training result
/// may be read before it passes.
#[test]
#[ignore = "awaits data/native_policy_train_step_wide_v1 from the Torch authority tuple"]
fn wide_torch_authority_grouped_loss_backward_and_multistep_adam_match_declared_witnesses() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../data/native_policy_train_step_wide_v1/runner_fixed_train_step_goldens_wide_v1.json",
    );
    let golden: GoldenFixture =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let forward: ForwardFixture = serde_json::from_str(include_str!(
        "../../../../data/native_policy_value_net_wide_v1/runner_fixed_forward_goldens_wide_v1.json"
    ))
    .unwrap();
    assert_eq!(
        golden.schema,
        "native-policy-value-cpu-train-step-wide-v1-torch-goldens-v1"
    );
    assert_eq!(golden.identity, TRAIN_STEP_IDENTITY_V1);
    assert_eq!(digest_bytes(MODEL_AUTHORITY), golden.authority.model_sha256);
    assert_eq!(
        digest_bytes(TRAINER_AUTHORITY),
        golden.authority.trainer_sha256
    );
    assert_eq!(
        digest_bytes(include_bytes!(
            "../../../../data/native_policy_value_net_wide_v1/runner_fixed_forward_goldens_wide_v1.json"
        )),
        golden.authority.forward_fixture_sha256
    );
    let optimizer = &golden.optimizer;
    assert_eq!(optimizer.identity, NATIVE_OPTIMIZER_IDENTITY_V1);
    let model = wide_model_v1();
    assert_eq!(
        model.parameter_manifest_sha256_wide_v1(),
        golden.initial_parameters.sha256
    );
    assert_tensor_state(
        &model.parameter_snapshot_wide_v1(),
        &golden.initial_parameters,
        f32::EPSILON,
        f32::EPSILON,
    );
    let mut state = NativePolicyValueTrainStateWideV1::new_wide_v1(model).unwrap();
    let mut high_precision_shrink_witness = false;
    for step in &golden.steps {
        let expected_outputs: Vec<Vec<ExpectedOutputBits>> = step
            .groups
            .iter()
            .map(|group| {
                group
                    .substeps
                    .iter()
                    .map(|substep| {
                        let output = state
                            .model_v1()
                            .forward_wide_v1(encoded(case_by_name(&forward, &substep.case)))
                            .unwrap();
                        ExpectedOutputBits {
                            raw_action_logit_bits: output
                                .logits
                                .iter()
                                .map(|value| value.to_bits())
                                .collect(),
                            value_bits: output.value.to_bits(),
                        }
                    })
                    .collect()
            })
            .collect();
        let substeps = substeps_for_step(&forward, step, &expected_outputs);
        let groups = groups_for_step(step, &substeps);
        let result = state
            .train_step_dims_v1(&groups, golden.value_coefficient, optimizer.learning_rate)
            .unwrap();
        assert_eq!(result.adam_step, step.step);
        for (actual, expected) in result.selected_outputs.iter().zip(&step.selected_outputs) {
            assert_eq!(actual.selected_action_index, expected.selected_action_index);
            for (actual, expected) in [
                (actual.selected_logit, expected.selected_logit.value),
                (actual.value, expected.value.value),
                (
                    actual.selected_log_probability,
                    expected.selected_log_probability.value,
                ),
            ] {
                assert_close(
                    actual,
                    expected,
                    golden.authority.selected_output_absolute_tolerance,
                    golden.authority.selected_output_relative_tolerance,
                );
            }
        }
        for (actual, expected) in [
            (result.policy_sum, step.loss.policy_sum.value),
            (result.value_sum, step.loss.value_sum.value),
            (result.loss, step.loss.loss.value),
        ] {
            assert_close(
                actual,
                expected,
                golden.authority.loss_absolute_tolerance,
                golden.authority.loss_relative_tolerance,
            );
        }
        assert_tensor_state(
            &result.gradients,
            &step.gradients_before_adam,
            golden.authority.gradient_absolute_tolerance,
            golden.authority.gradient_relative_tolerance,
        );
        high_precision_shrink_witness |=
            assert_gauge_record(&result.scorer_bias_gauge, &step.scorer_bias_gauge);
        let snapshot = state.snapshot_v1().unwrap();
        for (actual, expected) in [
            (&snapshot.first_moments, &step.first_moments_after_adam),
            (&snapshot.second_moments, &step.second_moments_after_adam),
            (&snapshot.parameters, &step.parameters_after_adam),
        ] {
            assert_tensor_state(
                actual,
                expected,
                golden.authority.optimizer_absolute_tolerance,
                golden.authority.optimizer_relative_tolerance,
            );
        }
        assert!(
            snapshot.parameters[CARD_EMBEDDING].values[..W_CARD_EMBEDDING_DIM_V1]
                .iter()
                .all(|value| value.to_bits() == 0)
        );
    }
    assert!(high_precision_shrink_witness);
}
