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
use sha2::{Digest, Sha256};
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
                    if rng.next_u64().is_multiple_of(3) {
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

/// Where the roundoff diagnostic perturbs the oracle: one application of
/// one layer (its `weight` tensor at `row`), at one pre-activation output or
/// one input element.
#[derive(Clone, Copy)]
struct OracleHookV1 {
    weight: usize,
    row: usize,
    site: OracleSiteV1,
    epsilon: f64,
}

#[derive(Clone, Copy)]
enum OracleSiteV1 {
    PreActivation(usize),
    Input(usize),
}

/// One oracle evaluation: the parameters, an optional hook, and an optional
/// capture of every input row one weight tensor sees, in row order. Without
/// a hook the arithmetic is the plain forward's.
struct OracleRunV1<'a> {
    parameters: &'a [Vec<f64>],
    hook: Option<OracleHookV1>,
    capture: Option<(usize, Vec<Vec<f64>>)>,
}

impl OracleRunV1<'_> {
    fn linear(&mut self, weight: usize, row: usize, input: &[f64]) -> Vec<f64> {
        let hook = self
            .hook
            .filter(|hook| hook.weight == weight && hook.row == row);
        let mut input = input.to_vec();
        if let Some(OracleHookV1 {
            site: OracleSiteV1::Input(column),
            epsilon,
            ..
        }) = hook
        {
            input[column] += epsilon;
        }
        if let Some((tensor, rows)) = &mut self.capture {
            if *tensor == weight {
                rows.push(input.clone());
            }
        }
        let mut output = oracle_linear_v1(
            &self.parameters[weight],
            &self.parameters[weight + 1],
            &input,
        );
        if let Some(OracleHookV1 {
            site: OracleSiteV1::PreActivation(output_index),
            epsilon,
            ..
        }) = hook
        {
            output[output_index] += epsilon;
        }
        output
    }

    /// `nn.Sequential(Linear, Tanh, Linear, Tanh)` from the tensor at `first`.
    fn two_layer(&mut self, first: usize, row: usize, input: &[f64]) -> Vec<f64> {
        let hidden: Vec<f64> = self
            .linear(first, row, input)
            .into_iter()
            .map(f64::tanh)
            .collect();
        self.linear(first + 2, row, &hidden)
            .into_iter()
            .map(f64::tanh)
            .collect()
    }
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
    let mut run = OracleRunV1 {
        parameters,
        hook: None,
        capture: None,
    };
    oracle_forward_run_v1(&mut run, hidden, embedding, view)
}

/// [`oracle_forward_v1`] through `run`'s hook and capture.
fn oracle_forward_run_v1(
    run: &mut OracleRunV1<'_>,
    hidden: usize,
    embedding: usize,
    view: &NativeEncodedDecisionViewV1<'_>,
) -> (Vec<f64>, f64) {
    let parameters = run.parameters;
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
            run.two_layer(OBJECT_FIRST_WEIGHT, object, &input)
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
        let edge_hidden = run.two_layer(EDGE_FIRST_WEIGHT, edge, &input);
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
            run.two_layer(NODE_FIRST_WEIGHT, object, &input)
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
    let state_hidden = run.two_layer(STATE_FIRST_WEIGHT, 0, &state_input);

    let actions = view.action_features.len() / ACTION_FEATURE_DIM_V1;
    let mut action_ref_pooled = vec![vec![0.0f64; hidden]; actions];
    for action_ref in 0..view.action_ref_card_ids.len() {
        let mut input = widen_v1(
            &view.action_ref_features[action_ref * ACTION_REF_FEATURE_DIM_V1
                ..(action_ref + 1) * ACTION_REF_FEATURE_DIM_V1],
        );
        input.extend_from_slice(&object_hidden[view.action_ref_node_indices[action_ref] as usize]);
        let ref_hidden = run.two_layer(ACTION_REF_FIRST_WEIGHT, action_ref, &input);
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
            let action_hidden = run.two_layer(ACTION_FIRST_WEIGHT, action, &input);
            let mut scorer_input = state_hidden.clone();
            scorer_input.extend_from_slice(&action_hidden);
            let scorer_hidden: Vec<f64> = run
                .linear(SCORER_FIRST_WEIGHT, action, &scorer_input)
                .into_iter()
                .map(f64::tanh)
                .collect();
            run.linear(SCORER_SECOND_WEIGHT, action, &scorer_hidden)[0]
        })
        .collect();
    let value_hidden: Vec<f64> = run
        .linear(VALUE_FIRST_WEIGHT, 0, &state_hidden)
        .into_iter()
        .map(f64::tanh)
        .collect();
    let value = run.linear(VALUE_SECOND_WEIGHT, 0, &value_hidden)[0];
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
    /// The witness with the largest error ratio, for the receipt.
    max_witness: String,
    counted_new: BTreeMap<String, usize>,
    outside: Vec<String>,
    /// Every witness, for the roundoff diagnostic.
    witness_list: Vec<WitnessV1>,
    /// The step's gradients and the oracle loss's targets and advantages.
    gradients: Vec<NativeNamedParameterV1>,
    targets: Vec<f64>,
    advantages: Vec<f64>,
}

/// One drawn coordinate: Rust's gradient, the oracle's central difference
/// and their error ratio.
#[derive(Clone)]
struct WitnessV1 {
    block: String,
    tensor: usize,
    index: usize,
    rust: f64,
    oracle: f64,
    ratio: f64,
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
        max_witness: String::new(),
        counted_new: BTreeMap::new(),
        outside: Vec::new(),
        witness_list: Vec::new(),
        gradients: result.gradients.clone(),
        targets: targets.clone(),
        advantages: advantages.clone(),
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
            report.witness_list.push(WitnessV1 {
                block: block.name.clone(),
                tensor: block.tensor,
                index,
                rust: analytic,
                oracle: numerical,
                ratio,
            });
            if ratio > report.max_ratio {
                report.max_ratio = ratio;
                report.max_witness = format!(
                    "{} [{index}] rust {analytic:e} oracle {numerical:e}",
                    block.name
                );
            }
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
    for objective in [ObjectiveV1::Gae, ObjectiveV1::Terminal] {
        let narrow_report = fd_check_dims_v1::<Net8DimsV1>(&narrow, objective);
        assert!(
            narrow_report.outside.is_empty(),
            "narrow calibration {objective:?}: {:?}",
            narrow_report.outside
        );
        let r_n = narrow_report.max_ratio;
        let wide_report = fd_check_dims_v1::<Net8W128DimsV1>(&wide, objective);
        // Written past the test harness's capture, before the gate, so CI
        // logs carry the receipt on every target.
        writeln!(
            std::io::stderr().lock(),
            "WIDE_FD_RECEIPT_V1 {objective:?}: r_n={r_n:.6e} narrow_witnesses={} \
             wide_max_ratio={:.6e} wide_witnesses={} wide_over_r_n={:.4} \
             min_counted_new={} narrow_max=({}) wide_max=({})",
            narrow_report.witnesses,
            wide_report.max_ratio,
            wide_report.witnesses,
            wide_report.max_ratio / r_n,
            wide_report.counted_new.values().min().copied().unwrap_or(0),
            narrow_report.max_witness,
            wide_report.max_witness,
        )
        .unwrap();
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
        let bound = FD_NARROW_RATIO_FACTOR_V1 * r_n;
        let over: Vec<&WitnessV1> = wide_report
            .witness_list
            .iter()
            .filter(|witness| witness.ratio > bound)
            .collect();
        // The clause 2 waiver's capture: r_n and every witness above 4 r_n
        // by round-trip value and bits, and the wide gradient's digest.
        let mut capture = format!(
            "WIDE_FD_GATE_V1 {objective:?}: r_n={r_n:e} r_n_bits={:#018x} \
             wide_gradient_sha256={} over_4_r_n={}",
            r_n.to_bits(),
            gradient_sha256_v1(&wide_report.gradients),
            over.len(),
        );
        for witness in &over {
            capture.push_str(&format!(
                " ({} [{}] tensor={} g_rust={:e} g_rust_bits={:#010x} g_fd={:e} \
                 g_fd_bits={:#018x} ratio={:e})",
                witness.block,
                witness.index,
                witness.tensor,
                witness.rust,
                (witness.rust as f32).to_bits(),
                witness.oracle,
                witness.oracle.to_bits(),
                witness.ratio,
            ));
        }
        writeln!(std::io::stderr().lock(), "{capture}").unwrap();
        match clause_2_waiver_v1(objective) {
            None => assert!(
                wide_report.max_ratio <= bound,
                "wide {objective:?} max ratio {} above {FD_NARROW_RATIO_FACTOR_V1} r_n = {bound}",
                wide_report.max_ratio,
            ),
            Some(waiver) => {
                assert_eq!(
                    r_n.to_bits(),
                    waiver.r_n_bits,
                    "wide {objective:?}: the clause 2 waiver lapsed (r_n changed)"
                );
                let recorded: Vec<(usize, usize, u32, u64)> = over
                    .iter()
                    .map(|witness| {
                        (
                            witness.tensor,
                            witness.index,
                            (witness.rust as f32).to_bits(),
                            witness.oracle.to_bits(),
                        )
                    })
                    .collect();
                assert_eq!(
                    recorded.as_slice(),
                    waiver.witnesses,
                    "wide {objective:?}: the clause 2 waiver lapsed (the witnesses above \
                     {FD_NARROW_RATIO_FACTOR_V1} r_n or their bits changed)"
                );
            }
        }
    }
}

/// Amendment 5.1-A1 v2 section 4.2: clause 2 is waived only in the Windows
/// (msvc) terminal cell at the arithmetic of 11053a11d, the frozen v2
/// fixture, seed and model. There, every witness above 4 r_n must be on this
/// list, bit for bit, and r_n must keep its bits; anything else fails, so
/// the waiver lapses on any change of that arithmetic.
#[cfg_attr(not(all(windows, target_env = "msvc")), allow(dead_code))]
struct Clause2WaiverV1 {
    r_n_bits: u64,
    /// `(tensor, index, g_rust f32 bits, g_fd f64 bits)` in draw order.
    witnesses: &'static [(usize, usize, u32, u64)],
}

/// Recorded from CI run 40454873435 (head 11053a11d, Windows workspace-lib
/// and workspace-targets): r_n = 1.2562741952193324e-3; edge_encoder.0.bias
/// [100] with g_rust = 4.699220880866051e-5 and g_fd = 4.669843955973363e-5.
#[cfg(all(windows, target_env = "msvc"))]
static TERMINAL_CLAUSE_2_WAIVER_V1: Option<Clause2WaiverV1> = Some(Clause2WaiverV1 {
    r_n_bits: 0x3f54_9532_2554_63d2,
    witnesses: &[(EDGE_FIRST_BIAS, 100, 0x3845_1980, 0x3f08_7bc2_2a5c_0000)],
});
#[cfg(not(all(windows, target_env = "msvc")))]
static TERMINAL_CLAUSE_2_WAIVER_V1: Option<Clause2WaiverV1> = None;

fn clause_2_waiver_v1(objective: ObjectiveV1) -> Option<&'static Clause2WaiverV1> {
    match objective {
        ObjectiveV1::Terminal => TERMINAL_CLAUSE_2_WAIVER_V1.as_ref(),
        ObjectiveV1::Gae => None,
    }
}

/// SHA-256 over every gradient element's f32 bits, little-endian, in
/// parameter order.
fn gradient_sha256_v1(gradients: &[NativeNamedParameterV1]) -> String {
    let mut digest = Sha256::new();
    for gradient in gradients {
        for value in &gradient.values {
            digest.update(value.to_bits().to_le_bytes());
        }
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// ---------------------------------------------------------------------------
// Roundoff diagnostic beside the section 5.1 gate (throughput lead ruling,
// 2026-10-27). It never gates: it prints, for the worst witnesses, how large
// each error is against the f32 accumulation roundoff its terms allow.
// ---------------------------------------------------------------------------

/// Central-difference step on one pre-activation or input element.
const TERM_STEP_V1: f64 = 1.0e-5;
const DIAGNOSED_WITNESSES_V1: usize = 10;

/// The per-term contributions to one gradient coordinate in the f64
/// oracle: each application of the coordinate's layer (substep, row) is one
/// term, its upstream pre-activation gradient for a bias, that times the
/// row's input for a weight, and the object input's gradient for an
/// embedding element.
struct TermsV1 {
    count: usize,
    sum_abs: f64,
}

/// `(group, decision, selected action)` per substep, in batch order.
fn flattened_substeps_v1() -> Vec<(usize, usize, usize)> {
    GRADIENT_BATCH_V1
        .iter()
        .enumerate()
        .flat_map(|(group, substeps)| {
            substeps
                .iter()
                .map(move |&(decision, selected)| (group, decision, selected))
        })
        .collect()
}

fn substep_output_v1(
    parameters: &[Vec<f64>],
    hidden: usize,
    embedding: usize,
    decision: &GradientDecisionV1,
    hook: Option<OracleHookV1>,
) -> (Vec<f64>, f64) {
    let mut run = OracleRunV1 {
        parameters,
        hook,
        capture: None,
    };
    oracle_forward_run_v1(&mut run, hidden, embedding, &decision.view_v1(false))
}

/// [`oracle_loss_v1`] from per-substep outputs.
fn loss_from_outputs_v1(outputs: &[(Vec<f64>, f64)], targets: &[f64], advantages: &[f64]) -> f64 {
    let substeps = flattened_substeps_v1();
    let group_count = GRADIENT_BATCH_V1.len() as f64;
    (0..GRADIENT_BATCH_V1.len())
        .map(|group| {
            let members: Vec<usize> = (0..substeps.len())
                .filter(|&substep| substeps[substep].0 == group)
                .collect();
            let joint = members
                .iter()
                .map(|&substep| oracle_log_softmax_v1(&outputs[substep].0, substeps[substep].2))
                .sum::<f64>();
            let error = outputs[members[0]].1 - targets[group];
            (-advantages[group] * joint + f64::from(VC) * error * error) / group_count
        })
        .sum()
}

#[allow(clippy::too_many_arguments)]
fn contribution_terms_v1(
    parameters: &[Vec<f64>],
    hidden: usize,
    embedding: usize,
    shapes: &[&[usize]],
    decisions: &[GradientDecisionV1; 2],
    targets: &[f64],
    advantages: &[f64],
    tensor: usize,
    index: usize,
) -> TermsV1 {
    let substeps = flattened_substeps_v1();
    let base: Vec<(Vec<f64>, f64)> = substeps
        .iter()
        .map(|&(_, decision, _)| {
            substep_output_v1(parameters, hidden, embedding, &decisions[decision], None)
        })
        .collect();
    // The layer whose applications are the terms, and where each term is
    // read.
    let (weight, site, column, token) = if tensor == CARD_EMBEDDING {
        let element = index % embedding;
        (
            OBJECT_FIRST_WEIGHT,
            OracleSiteV1::Input(OBJECT_FEATURE_DIM_V1 + element),
            None,
            Some(index / embedding),
        )
    } else if shapes[tensor].len() == 1 {
        (tensor - 1, OracleSiteV1::PreActivation(index), None, None)
    } else {
        let input_dim = shapes[tensor][1];
        (
            tensor,
            OracleSiteV1::PreActivation(index / input_dim),
            Some(index % input_dim),
            None,
        )
    };
    let mut terms = TermsV1 {
        count: 0,
        sum_abs: 0.0,
    };
    for (substep, &(_, decision, _)) in substeps.iter().enumerate() {
        let decision = &decisions[decision];
        let mut run = OracleRunV1 {
            parameters,
            hook: None,
            capture: Some((weight, Vec::new())),
        };
        oracle_forward_run_v1(&mut run, hidden, embedding, &decision.view_v1(false));
        let rows = run.capture.take().unwrap().1;
        for (row, input) in rows.iter().enumerate() {
            if token.is_some_and(|token| decision.object_card_ids[row] as usize != token) {
                continue;
            }
            let loss = |epsilon: f64| {
                let mut outputs = base.clone();
                outputs[substep] = substep_output_v1(
                    parameters,
                    hidden,
                    embedding,
                    decision,
                    Some(OracleHookV1 {
                        weight,
                        row,
                        site,
                        epsilon,
                    }),
                );
                loss_from_outputs_v1(&outputs, targets, advantages)
            };
            let upstream = (loss(TERM_STEP_V1) - loss(-TERM_STEP_V1)) / (2.0 * TERM_STEP_V1);
            let contribution = match column {
                Some(column) => upstream * input[column],
                None => upstream,
            };
            terms.count += 1;
            terms.sum_abs += contribution.abs();
        }
    }
    terms
}

/// `kappa = sum |c_i| / |g_fd|` and the error normalized by the expected f32
/// accumulation roundoff, `abs error / (sum |c_i| * 2^-24 * sqrt(n))`.
fn roundoff_v1(terms: &TermsV1, rust: f64, oracle: f64) -> (f64, f64) {
    let kappa = terms.sum_abs / oracle.abs();
    let normalized =
        (rust - oracle).abs() / (terms.sum_abs * 2f64.powi(-24) * (terms.count as f64).sqrt());
    (kappa, normalized)
}

fn median_v1(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

/// The ten worst witnesses of one objective and width, with their terms.
fn diagnose_dims_v1<D: NetDimsV1>(
    label: &str,
    model: &D::Net,
    objective: ObjectiveV1,
    report: &FdReportV1,
    out: &mut impl Write,
) {
    let decisions = gradient_decisions_v1();
    let parameters = parameters_f64_v1(&model.parameter_snapshot_v1());
    let mut ranked = report.witness_list.clone();
    ranked.sort_by(|left, right| right.ratio.total_cmp(&left.ratio));
    for (rank, witness) in ranked.iter().take(DIAGNOSED_WITNESSES_V1).enumerate() {
        let terms = contribution_terms_v1(
            &parameters,
            D::HIDDEN,
            D::CARD_EMBEDDING,
            &D::PARAMETER_SHAPES,
            &decisions,
            &report.targets,
            &report.advantages,
            witness.tensor,
            witness.index,
        );
        let (kappa, normalized) = roundoff_v1(&terms, witness.rust, witness.oracle);
        writeln!(
            out,
            "WIDE_FD_DIAG_V1 {objective:?} {label} rank={rank} ratio={:.4e} witness=({} [{}]) \
             g_rust={:.10e} g_fd={:.10e} abs_error={:.4e} kappa={kappa:.4e} n={} e={normalized:.4}",
            witness.ratio,
            witness.block,
            witness.index,
            witness.rust,
            witness.oracle,
            (witness.rust - witness.oracle).abs(),
            terms.count,
        )
        .unwrap();
    }
}

/// `edge_encoder.0.bias` (the Windows terminal worst witness is wide row 100)
/// at FD steps 1e-4 and 1e-5, and its kappa and e over every row of both
/// widths.
fn diagnose_edge_bias_dims_v1<D: NetDimsV1>(
    label: &str,
    model: &D::Net,
    report: &FdReportV1,
    out: &mut impl Write,
) {
    let decisions = gradient_decisions_v1();
    let mut parameters = parameters_f64_v1(&model.parameter_snapshot_v1());
    let fd = |parameters: &mut Vec<Vec<f64>>, row: usize, step: f64| {
        let original = parameters[EDGE_FIRST_BIAS][row];
        parameters[EDGE_FIRST_BIAS][row] = original + step;
        let plus = oracle_loss_v1(
            parameters,
            D::HIDDEN,
            D::CARD_EMBEDDING,
            &decisions,
            &report.targets,
            &report.advantages,
        );
        parameters[EDGE_FIRST_BIAS][row] = original - step;
        let minus = oracle_loss_v1(
            parameters,
            D::HIDDEN,
            D::CARD_EMBEDDING,
            &decisions,
            &report.targets,
            &report.advantages,
        );
        parameters[EDGE_FIRST_BIAS][row] = original;
        (plus - minus) / (2.0 * step)
    };
    let mut summaries: BTreeMap<&str, (Vec<f64>, Vec<f64>)> = BTreeMap::new();
    for row in 0..D::HIDDEN {
        let rust = f64::from(report.gradients[EDGE_FIRST_BIAS].values[row]);
        let coarse = fd(&mut parameters, row, 1.0e-4);
        let terms = contribution_terms_v1(
            &parameters,
            D::HIDDEN,
            D::CARD_EMBEDDING,
            &D::PARAMETER_SHAPES,
            &decisions,
            &report.targets,
            &report.advantages,
            EDGE_FIRST_BIAS,
            row,
        );
        let (kappa, normalized) = roundoff_v1(&terms, rust, coarse);
        let group = if row < HIDDEN_DIM_V1 {
            "rows 0-63"
        } else {
            "rows 64-127"
        };
        let entry = summaries.entry(group).or_default();
        entry.0.push(kappa);
        entry.1.push(normalized);
        if row == 100 {
            let fine = fd(&mut parameters, row, 1.0e-5);
            writeln!(
                out,
                "WIDE_FD_DIAG_V1 Terminal {label} edge_encoder.0.bias [{row}] g_rust={rust:.10e} \
                 g_fd(1e-4)={coarse:.10e} g_fd(1e-5)={fine:.10e} abs_error={:.4e} \
                 kappa={kappa:.4e} n={} e={normalized:.4}",
                (rust - coarse).abs(),
                terms.count,
            )
            .unwrap();
        }
    }
    for (group, (kappas, normalized)) in &mut summaries {
        writeln!(
            out,
            "WIDE_FD_DIAG_V1 Terminal {label} edge_encoder.0.bias {group}: kappa min={:.4e} \
             median={:.4e} max={:.4e}; e median={:.4} max={:.4}",
            kappas.iter().copied().fold(f64::INFINITY, f64::min),
            median_v1(kappas),
            kappas.iter().copied().fold(0.0, f64::max),
            median_v1(normalized),
            normalized.iter().copied().fold(0.0, f64::max),
        )
        .unwrap();
    }
}

/// The section 5.1 roundoff diagnostic (never gates): per objective and
/// width, the ten worst witnesses with their abs error, cancellation ratio
/// kappa, term count n and normalized error e, plus the edge encoder bias
/// rows of both widths for the terminal objective. Written past the test
/// harness's capture so CI logs carry it on every target.
#[test]
fn wide_fd_roundoff_diagnostic_receipt() {
    let narrow = perturbed_dims_v1::<Net8DimsV1>(narrow_model_v1(), 0xA11C_E5ED);
    let wide = perturbed_dims_v1::<Net8W128DimsV1>(wide_model_v1(), 0xA11C_E5ED);
    let mut out = std::io::stderr().lock();
    for objective in [ObjectiveV1::Gae, ObjectiveV1::Terminal] {
        let narrow_report = fd_check_dims_v1::<Net8DimsV1>(&narrow, objective);
        diagnose_dims_v1::<Net8DimsV1>("net-8", &narrow, objective, &narrow_report, &mut out);
        let wide_report = fd_check_dims_v1::<Net8W128DimsV1>(&wide, objective);
        diagnose_dims_v1::<Net8W128DimsV1>("net-8w128", &wide, objective, &wide_report, &mut out);
        if matches!(objective, ObjectiveV1::Terminal) {
            diagnose_edge_bias_dims_v1::<Net8DimsV1>("net-8", &narrow, &narrow_report, &mut out);
            diagnose_edge_bias_dims_v1::<Net8W128DimsV1>(
                "net-8w128",
                &wide,
                &wide_report,
                &mut out,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Amendment 5.1-A1 receipts (v2 section 3, with the round-2 direction). The
// rho receipt prints only; D2 prints only until its v3 pass rule is frozen,
// and is ignored until the reviewers agree. Neither changes the train step,
// the oracle, the fixture or the witness draw.
// ---------------------------------------------------------------------------

/// f32 unit roundoff, 2^-24.
const UNIT_ROUNDOFF_V1: f64 = 1.0 / 16_777_216.0;
/// rho is information only; the count above this is printed.
const RHO_PRINTED_BOUND_V1: f64 = 1.0e-4;
const RHO_LISTED_V1: usize = 5;
/// D2's two oracle steps for an upstream gradient (the second is primary),
/// and the relative agreement it requires between them.
const D2_STEPS_V1: [f64; 2] = [1.0e-4, 1.0e-5];
const D2_STEP_AGREEMENT_V1: f64 = 1.0e-6;
/// Check (a): reconstruction, relative to the oracle term mass.
const D2_RECONSTRUCTION_V1: f64 = 1.0e-8;
/// W1, and the W3 witnesses by width: `(tensor, index)`.
const D2_W1_V1: (usize, usize) = (EDGE_FIRST_BIAS, 100);
const D2_W3_WIDE_V1: [(usize, usize); 6] = [
    (VALUE_SECOND_WEIGHT, 101),
    (VALUE_SECOND_WEIGHT, 93),
    (VALUE_SECOND_WEIGHT, 78),
    (VALUE_SECOND_WEIGHT, 77),
    (VALUE_SECOND_WEIGHT, 5),
    (STATE_FIRST_BIAS, 103),
];
const D2_W3_NARROW_V1: [(usize, usize); 4] = [
    (STATE_SECOND_BIAS, 2),
    (VALUE_SECOND_WEIGHT, 59),
    (VALUE_SECOND_WEIGHT, 25),
    (VALUE_SECOND_WEIGHT, 22),
];

/// One witness with its oracle terms and roundoff measures.
struct MeasuredV1 {
    witness: WitnessV1,
    terms: TermsV1,
    abs_error: f64,
    kappa: f64,
    e: f64,
}

impl MeasuredV1 {
    /// `abs error / sum |terms|`, `None` when every term is zero.
    fn rho(&self) -> Option<f64> {
        (self.terms.sum_abs > 0.0).then_some(self.abs_error / self.terms.sum_abs)
    }

    fn name(&self) -> String {
        format!("{} [{}]", self.witness.block, self.witness.index)
    }
}

/// rho over every drawn witness of one width and objective (information
/// only), with the zero-mass witnesses listed separately.
fn rho_receipt_dims_v1<D: NetDimsV1>(
    label: &str,
    model: &D::Net,
    objective: ObjectiveV1,
    out: &mut impl Write,
) where
    D::Net: RecordingNetV1,
{
    let report = fd_check_dims_v1::<D>(model, objective);
    let decisions = gradient_decisions_v1();
    let parameters = parameters_f64_v1(&model.parameter_snapshot_v1());
    let measured: Vec<MeasuredV1> = report
        .witness_list
        .iter()
        .map(|witness| {
            let terms = contribution_terms_v1(
                &parameters,
                D::HIDDEN,
                D::CARD_EMBEDDING,
                &D::PARAMETER_SHAPES,
                &decisions,
                &report.targets,
                &report.advantages,
                witness.tensor,
                witness.index,
            );
            let (kappa, e) = roundoff_v1(&terms, witness.rust, witness.oracle);
            MeasuredV1 {
                witness: witness.clone(),
                abs_error: (witness.rust - witness.oracle).abs(),
                terms,
                kappa,
                e,
            }
        })
        .collect();
    let mut ranked: Vec<&MeasuredV1> = measured
        .iter()
        .filter(|measured| measured.rho().is_some())
        .collect();
    ranked.sort_by(|left, right| right.rho().unwrap().total_cmp(&left.rho().unwrap()));
    let zero_mass: Vec<&MeasuredV1> = measured
        .iter()
        .filter(|measured| measured.rho().is_none())
        .collect();
    let worst = ranked[0];
    writeln!(
        out,
        "WIDE_FD_RHO_V1 {objective:?} {label}: witnesses={} max_rho={:.4e} at ({}) e={:.4} \
         above_{RHO_PRINTED_BOUND_V1:e}={} zero_mass={} zero_mass_max_abs_g_rust={:.4e}",
        measured.len(),
        worst.rho().unwrap(),
        worst.name(),
        worst.e,
        ranked
            .iter()
            .filter(|measured| measured.rho().unwrap() > RHO_PRINTED_BOUND_V1)
            .count(),
        zero_mass.len(),
        zero_mass
            .iter()
            .map(|measured| measured.witness.rust.abs())
            .fold(0.0, f64::max),
    )
    .unwrap();
    for (rank, measured) in ranked.iter().take(RHO_LISTED_V1).enumerate() {
        writeln!(
            out,
            "WIDE_FD_RHO_V1 {objective:?} {label} rank={rank} rho={:.4e} witness=({}) \
             abs_error={:.4e} kappa={:.4e} n={} e={:.4}",
            measured.rho().unwrap(),
            measured.name(),
            measured.abs_error,
            measured.kappa,
            measured.terms.count,
            measured.e,
        )
        .unwrap();
    }
    for measured in &zero_mass {
        writeln!(
            out,
            "WIDE_FD_RHO_V1 {objective:?} {label} zero_mass witness=({}) g_rust={:.4e}",
            measured.name(),
            measured.witness.rust,
        )
        .unwrap();
    }
}

/// Both widths of one objective, written to stderr in one piece so parallel
/// receipts do not interleave.
fn rho_receipt_v1(objective: ObjectiveV1) {
    let narrow = perturbed_dims_v1::<Net8DimsV1>(narrow_model_v1(), 0xA11C_E5ED);
    let wide = perturbed_dims_v1::<Net8W128DimsV1>(wide_model_v1(), 0xA11C_E5ED);
    let mut out = Vec::new();
    rho_receipt_dims_v1::<Net8DimsV1>("net-8", &narrow, objective, &mut out);
    rho_receipt_dims_v1::<Net8W128DimsV1>("net-8w128", &wide, objective, &mut out);
    std::io::stderr().lock().write_all(&out).unwrap();
}

/// Amendment 5.1-A1 v2 rho receipt, GAE objective (information only).
#[test]
fn wide_fd_rho_receipt_gae() {
    rho_receipt_v1(ObjectiveV1::Gae);
}

/// Amendment 5.1-A1 v2 rho receipt, terminal objective (information only).
#[test]
fn wide_fd_rho_receipt_terminal() {
    rho_receipt_v1(ObjectiveV1::Terminal);
}

/// The step's `linear_backward_into` calls, grouped by the substep whose
/// reverse pass made them, in batch order. The step visits groups and their
/// substeps in reverse, and each reverse pass starts at the scorer's second
/// layer.
fn captures_by_substep_v1(
    captures: Vec<LinearBackwardCaptureV1>,
) -> Vec<Vec<LinearBackwardCaptureV1>> {
    let mut visits: Vec<Vec<LinearBackwardCaptureV1>> = Vec::new();
    for capture in captures {
        if capture.weight_index == SCORER_SECOND_WEIGHT {
            visits.push(Vec::new());
        }
        visits
            .last_mut()
            .expect("each reverse pass starts at the scorer")
            .push(capture);
    }
    let mut first_substep = 0;
    let mut visit_order = Vec::new();
    let mut starts = Vec::new();
    for group in GRADIENT_BATCH_V1 {
        starts.push((first_substep, group.len()));
        first_substep += group.len();
    }
    for &(start, len) in starts.iter().rev() {
        visit_order.extend((start..start + len).rev());
    }
    assert_eq!(visits.len(), first_substep);
    let mut by_substep: Vec<Option<Vec<LinearBackwardCaptureV1>>> =
        (0..first_substep).map(|_| None).collect();
    for (substep, captures) in visit_order.into_iter().zip(visits) {
        by_substep[substep] = Some(captures);
    }
    by_substep.into_iter().map(Option::unwrap).collect()
}

/// One term `a_s b_s` of a gradient coordinate: the Rust f32 factors from
/// the step's own backward and the oracle's, the oracle upstream gradient at
/// each of [`D2_STEPS_V1`].
struct D2TermV1 {
    a32: f64,
    b32: f64,
    a64: [f64; 2],
    b64: f64,
}

/// Every term of `(tensor, index)`, substep then row, the step's captured
/// factors beside the oracle's.
#[allow(clippy::too_many_arguments)]
fn d2_terms_dims_v1<D: NetDimsV1>(
    parameters: &[Vec<f64>],
    decisions: &[GradientDecisionV1; 2],
    targets: &[f64],
    advantages: &[f64],
    captures: &[Vec<LinearBackwardCaptureV1>],
    tensor: usize,
    index: usize,
) -> Vec<D2TermV1> {
    let substeps = flattened_substeps_v1();
    assert_eq!(captures.len(), substeps.len());
    let base: Vec<(Vec<f64>, f64)> = substeps
        .iter()
        .map(|&(_, decision, _)| {
            substep_output_v1(
                parameters,
                D::HIDDEN,
                D::CARD_EMBEDDING,
                &decisions[decision],
                None,
            )
        })
        .collect();
    let embedding = tensor == CARD_EMBEDDING;
    let bias = !embedding && D::PARAMETER_SHAPES[tensor].len() == 1;
    // The layer whose applications are the terms, the oracle site of each
    // term's upstream gradient, the element read for `a` (an output of the
    // layer, or for an embedding element an input), and the input column read
    // for `b` (none for a bias or an embedding element: b = 1).
    let (layer, site, element, column, token) = if embedding {
        let element = OBJECT_FEATURE_DIM_V1 + index % D::CARD_EMBEDDING;
        (
            OBJECT_FIRST_WEIGHT,
            OracleSiteV1::Input(element),
            element,
            None,
            Some(index / D::CARD_EMBEDDING),
        )
    } else if bias {
        (
            tensor - 1,
            OracleSiteV1::PreActivation(index),
            index,
            None,
            None,
        )
    } else {
        let input_dim = D::PARAMETER_SHAPES[tensor][1];
        let output = index / input_dim;
        (
            tensor,
            OracleSiteV1::PreActivation(output),
            output,
            Some(index % input_dim),
            None,
        )
    };
    let mut terms = Vec::new();
    for (substep, &(_, decision, _)) in substeps.iter().enumerate() {
        let decision = &decisions[decision];
        let rows = captured_rows_v1(parameters, D::HIDDEN, D::CARD_EMBEDDING, decision, layer);
        let calls: Vec<&LinearBackwardCaptureV1> = captures[substep]
            .iter()
            .filter(|call| call.weight_index == layer)
            .collect();
        assert_eq!(calls.len(), 1, "one backward call per layer and substep");
        let call = calls[0];
        assert_eq!(call.rows, rows.len());
        for (row, input) in rows.iter().enumerate() {
            if token.is_some_and(|token| decision.object_card_ids[row] as usize != token) {
                continue;
            }
            let (a32, b32) = if embedding {
                (call.d_input[row * call.input_dim + element], 1.0)
            } else {
                (
                    call.d_output[row * call.output_dim + element],
                    column.map_or(1.0, |column| call.input[row * call.input_dim + column]),
                )
            };
            let upstream = |step: f64| {
                let loss = |epsilon: f64| {
                    let mut outputs = base.clone();
                    outputs[substep] = substep_output_v1(
                        parameters,
                        D::HIDDEN,
                        D::CARD_EMBEDDING,
                        decision,
                        Some(OracleHookV1 {
                            weight: layer,
                            row,
                            site,
                            epsilon,
                        }),
                    );
                    loss_from_outputs_v1(&outputs, targets, advantages)
                };
                (loss(step) - loss(-step)) / (2.0 * step)
            };
            terms.push(D2TermV1 {
                a32: f64::from(a32),
                b32: f64::from(b32),
                a64: D2_STEPS_V1.map(upstream),
                b64: column.map_or(1.0, |column| input[column]),
            });
        }
    }
    terms
}

/// The investigated witnesses of one width and objective: `(sets, tensor,
/// index)`, W2 being every witness above 4 r_n of this cell.
fn d2_witnesses_v1(report: &FdReportV1, r_n: f64, wide: bool) -> Vec<(String, usize, usize)> {
    let mut witnesses: Vec<(String, usize, usize)> = Vec::new();
    let mut add = |set: &str, tensor: usize, index: usize| {
        match witnesses
            .iter_mut()
            .find(|(_, t, i)| (*t, *i) == (tensor, index))
        {
            Some((sets, _, _)) => {
                sets.push('+');
                sets.push_str(set);
            }
            None => witnesses.push((set.to_string(), tensor, index)),
        }
    };
    if wide {
        add("W1", D2_W1_V1.0, D2_W1_V1.1);
    }
    for witness in &report.witness_list {
        if witness.ratio > FD_NARROW_RATIO_FACTOR_V1 * r_n {
            add("W2", witness.tensor, witness.index);
        }
    }
    let named: &[(usize, usize)] = if wide {
        &D2_W3_WIDE_V1
    } else {
        &D2_W3_NARROW_V1
    };
    for &(tensor, index) in named {
        add("W3", tensor, index);
    }
    witnesses
}

/// `(g_rust, g_fd)` of one coordinate: the drawn witness's when drawn, or the
/// step's gradient and the gate's own central difference.
fn d2_gradients_dims_v1<D: NetDimsV1>(
    report: &FdReportV1,
    parameters: &[Vec<f64>],
    decisions: &[GradientDecisionV1; 2],
    tensor: usize,
    index: usize,
) -> (f64, f64) {
    if let Some(witness) = report
        .witness_list
        .iter()
        .find(|witness| (witness.tensor, witness.index) == (tensor, index))
    {
        return (witness.rust, witness.oracle);
    }
    let mut parameters = parameters.to_vec();
    let original = parameters[tensor][index];
    let mut loss = |value: f64| {
        parameters[tensor][index] = value;
        oracle_loss_v1(
            &parameters,
            D::HIDDEN,
            D::CARD_EMBEDDING,
            decisions,
            &report.targets,
            &report.advantages,
        )
    };
    let numerical =
        (loss(original + FD_STEP_V1) - loss(original - FD_STEP_V1)) / (2.0 * FD_STEP_V1);
    (
        f64::from(report.gradients[tensor].values[index]),
        numerical,
    )
}

/// `gamma_m = m u / (1 - m u)`.
fn gamma_v1(m: usize) -> f64 {
    let mu = m as f64 * UNIT_ROUNDOFF_V1;
    mu / (1.0 - mu)
}

/// The spacing of f32 values just above `|value|`.
fn ulp_f32_v1(value: f32) -> f64 {
    let magnitude = value.abs();
    f64::from(f32::from_bits(magnitude.to_bits() + 1)) - f64::from(magnitude)
}

/// `error / allowance`, zero for an exact result.
fn bound_ratio_v1(error: f64, allowance: f64) -> f64 {
    if error == 0.0 {
        0.0
    } else {
        error / allowance
    }
}

/// The largest error-to-local-bound ratio per `(tensor, kind)`.
type LocalMaximaV1 = BTreeMap<(usize, &'static str), f64>;

fn note_local_v1(maxima: &mut LocalMaximaV1, tensor: usize, kind: &'static str, ratio: f64) {
    let maximum = maxima.entry((tensor, kind)).or_insert(0.0);
    *maximum = maximum.max(ratio);
}

/// The second layers of the two-layer stacks (scorer and value heads
/// included); each one's first layer is two tensors before it, with a tanh
/// between them.
const SECOND_LAYER_WEIGHTS_V1: [usize; 8] = [
    OBJECT_SECOND_WEIGHT,
    EDGE_SECOND_WEIGHT,
    NODE_SECOND_WEIGHT,
    STATE_SECOND_WEIGHT,
    ACTION_REF_SECOND_WEIGHT,
    ACTION_SECOND_WEIGHT,
    SCORER_SECOND_WEIGHT,
    VALUE_SECOND_WEIGHT,
];

/// Local bounds on one captured backward call: its input gradient (dot
/// length `output_dim`), and its weight and bias gradient accumulations
/// (length `rows`, the prior value counted in the mass), against f64 sums of
/// the captured f32 operands.
fn backward_local_bounds_v1(call: &LinearBackwardCaptureV1, maxima: &mut LocalMaximaV1) {
    let (rows, input_dim, output_dim) = (call.rows, call.input_dim, call.output_dim);
    for row in 0..rows {
        for column in 0..input_dim {
            let mut reference = 0.0;
            let mut mass = 0.0;
            for output in 0..output_dim {
                let term = f64::from(call.d_output[row * output_dim + output])
                    * f64::from(call.weight[output * input_dim + column]);
                reference += term;
                mass += term.abs();
            }
            let error = (f64::from(call.d_input[row * input_dim + column]) - reference).abs();
            let ratio = bound_ratio_v1(error, gamma_v1(output_dim) * mass);
            note_local_v1(maxima, call.weight_index, "backward d_input", ratio);
        }
    }
    for output in 0..output_dim {
        for column in 0..input_dim {
            let element = output * input_dim + column;
            let mut reference = f64::from(call.d_weight_before[element]);
            let mut mass = reference.abs();
            for row in 0..rows {
                let term = f64::from(call.d_output[row * output_dim + output])
                    * f64::from(call.input[row * input_dim + column]);
                reference += term;
                mass += term.abs();
            }
            let error = (f64::from(call.d_weight_after[element]) - reference).abs();
            let ratio = bound_ratio_v1(error, gamma_v1(rows) * mass);
            note_local_v1(maxima, call.weight_index, "backward d_weight", ratio);
        }
        let mut reference = f64::from(call.d_bias_before[output]);
        let mut mass = reference.abs();
        for row in 0..rows {
            let term = f64::from(call.d_output[row * output_dim + output]);
            reference += term;
            mass += term.abs();
        }
        let error = (f64::from(call.d_bias_after[output]) - reference).abs();
        let ratio = bound_ratio_v1(error, gamma_v1(rows) * mass);
        note_local_v1(maxima, call.bias_index, "backward d_bias", ratio);
    }
}

/// The tanh-gradient step between a stack's second and first layers: the
/// first layer's upstream gradient against the f64 product of the second
/// layer's input gradient and `1 - h^2` at the captured activation, with a
/// 2 ulp allowance.
fn tanh_gradient_local_bounds_v1(
    substep: &[LinearBackwardCaptureV1],
    maxima: &mut LocalMaximaV1,
) {
    for &second_weight in &SECOND_LAYER_WEIGHTS_V1 {
        let second = substep
            .iter()
            .find(|call| call.weight_index == second_weight);
        let first = substep
            .iter()
            .find(|call| call.weight_index == second_weight - 2);
        let (Some(second), Some(first)) = (second, first) else {
            continue;
        };
        assert_eq!(first.d_output.len(), second.d_input.len());
        for ((&upstream, &d_input), &activation) in first
            .d_output
            .iter()
            .zip(&second.d_input)
            .zip(&second.input)
        {
            let activation = f64::from(activation);
            let reference = f64::from(d_input) * (1.0 - activation * activation);
            let error = (f64::from(upstream) - reference).abs();
            let ratio = bound_ratio_v1(error, 2.0 * ulp_f32_v1(upstream));
            note_local_v1(maxima, first.weight_index, "backward tanh gradient", ratio);
        }
    }
}

/// Local bounds on one forward layer application from the tape: each output
/// against the f64 dot product of its f32 operands (length `input_dim`, the
/// bias counted in the mass), through tanh with a 2 ulp allowance when the
/// layer has one.
#[allow(clippy::too_many_arguments)]
fn forward_local_bounds_v1(
    parameters: &[NativeNamedParameterV1],
    weight_index: usize,
    input: &[f32],
    output: &[f32],
    rows: usize,
    input_dim: usize,
    output_dim: usize,
    tanh: bool,
    maxima: &mut LocalMaximaV1,
) {
    let weight = &parameters[weight_index].values;
    assert_eq!(input.len(), rows * input_dim);
    assert_eq!(output.len(), rows * output_dim);
    for row in 0..rows {
        for index in 0..output_dim {
            let mut reference = f64::from(parameters[weight_index + 1].values[index]);
            let mut mass = reference.abs();
            for column in 0..input_dim {
                let term = f64::from(weight[index * input_dim + column])
                    * f64::from(input[row * input_dim + column]);
                reference += term;
                mass += term.abs();
            }
            let actual = output[row * output_dim + index];
            let linear = gamma_v1(input_dim) * mass;
            let (error, allowance) = if tanh {
                (
                    (f64::from(actual) - reference.tanh()).abs(),
                    linear + 2.0 * ulp_f32_v1(actual),
                )
            } else {
                ((f64::from(actual) - reference).abs(), linear)
            };
            let ratio = bound_ratio_v1(error, allowance);
            note_local_v1(maxima, weight_index, "forward", ratio);
        }
    }
}

/// The input a two-layer stack's first (`first`) or second layer reads.
fn stack_input_v1<'a>(
    tape: &'a DecisionTapeV1,
    stack: Option<&TwoLayerTapeV1>,
    first: usize,
    weight: usize,
) -> Option<&'a [f32]> {
    let stack = stack?;
    if weight == first {
        Some(tape.f32_values_v1(stack.input))
    } else if weight == first + 2 {
        Some(tape.f32_values_v1(stack.first_output))
    } else {
        None
    }
}

/// The tape span a backward call of `weight` reads as its input.
fn tape_input_v1(tape: &DecisionTapeV1, weight: usize) -> Option<&[f32]> {
    stack_input_v1(tape, Some(&tape.object_encoder), OBJECT_FIRST_WEIGHT, weight)
        .or_else(|| stack_input_v1(tape, tape.edge_encoder.as_ref(), EDGE_FIRST_WEIGHT, weight))
        .or_else(|| stack_input_v1(tape, Some(&tape.node_update), NODE_FIRST_WEIGHT, weight))
        .or_else(|| stack_input_v1(tape, Some(&tape.state_encoder), STATE_FIRST_WEIGHT, weight))
        .or_else(|| {
            stack_input_v1(
                tape,
                tape.action_ref_encoder.as_ref(),
                ACTION_REF_FIRST_WEIGHT,
                weight,
            )
        })
        .or_else(|| stack_input_v1(tape, Some(&tape.action_encoder), ACTION_FIRST_WEIGHT, weight))
        .or_else(|| match weight {
            SCORER_FIRST_WEIGHT => Some(tape.f32_values_v1(tape.scorer_input)),
            SCORER_SECOND_WEIGHT => Some(tape.f32_values_v1(tape.scorer_hidden)),
            VALUE_FIRST_WEIGHT => Some(tape.f32_values_v1(tape.state_encoder.second_output)),
            VALUE_SECOND_WEIGHT => Some(tape.f32_values_v1(tape.value_hidden)),
            _ => None,
        })
}

/// Forward local bounds over every layer application of each decision's
/// tape forward, and whether that tape is bit for bit the one the step's
/// backward read (every captured call's input against the tape's span).
fn forward_local_bounds_dims_v1<D: NetDimsV1>(
    model: &D::Net,
    decisions: &[GradientDecisionV1; 2],
    captures: &[Vec<LinearBackwardCaptureV1>],
    maxima: &mut LocalMaximaV1,
) -> bool
where
    D::Net: RecordingNetV1,
{
    let parameters = model.parameter_snapshot_v1();
    let config = model.feature_transfer_config_v3();
    let math = NativePolicyValueTrainStateDimsV1::<D>::new_dims_v1(model.clone())
        .unwrap()
        .training_math_dims_v1();
    let tapes: Vec<DecisionTapeV1> = decisions
        .iter()
        .map(|decision| {
            forward_with_tape_dims_v1::<D>(&parameters, config, decision.view_v1(false), math)
                .unwrap()
        })
        .collect();
    let mut matches = true;
    for (substep, &(_, decision, _)) in flattened_substeps_v1().iter().enumerate() {
        for call in &captures[substep] {
            let span = tape_input_v1(&tapes[decision], call.weight_index)
                .expect("every backward layer reads a tape span");
            matches &= span.iter().map(|value| value.to_bits()).eq(call
                .input
                .iter()
                .map(|value| value.to_bits()));
        }
    }
    for tape in &tapes {
        let stacks = [
            (OBJECT_FIRST_WEIGHT, Some(&tape.object_encoder)),
            (EDGE_FIRST_WEIGHT, tape.edge_encoder.as_ref()),
            (NODE_FIRST_WEIGHT, Some(&tape.node_update)),
            (STATE_FIRST_WEIGHT, Some(&tape.state_encoder)),
            (ACTION_REF_FIRST_WEIGHT, tape.action_ref_encoder.as_ref()),
            (ACTION_FIRST_WEIGHT, Some(&tape.action_encoder)),
        ];
        for (first, stack) in stacks {
            let Some(stack) = stack else {
                continue;
            };
            let first_output = tape.f32_values_v1(stack.first_output);
            forward_local_bounds_v1(
                &parameters,
                first,
                tape.f32_values_v1(stack.input),
                first_output,
                stack.rows,
                D::PARAMETER_SHAPES[first][1],
                D::HIDDEN,
                true,
                maxima,
            );
            forward_local_bounds_v1(
                &parameters,
                first + 2,
                first_output,
                tape.f32_values_v1(stack.second_output),
                stack.rows,
                D::HIDDEN,
                D::HIDDEN,
                true,
                maxima,
            );
        }
        let actions = tape.logits_v1().len();
        let scorer_hidden = tape.f32_values_v1(tape.scorer_hidden);
        forward_local_bounds_v1(
            &parameters,
            SCORER_FIRST_WEIGHT,
            tape.f32_values_v1(tape.scorer_input),
            scorer_hidden,
            actions,
            D::SCORER_INPUT,
            D::HIDDEN,
            true,
            maxima,
        );
        forward_local_bounds_v1(
            &parameters,
            SCORER_SECOND_WEIGHT,
            scorer_hidden,
            tape.logits_v1(),
            actions,
            D::HIDDEN,
            1,
            false,
            maxima,
        );
        let value_hidden = tape.f32_values_v1(tape.value_hidden);
        forward_local_bounds_v1(
            &parameters,
            VALUE_FIRST_WEIGHT,
            tape.f32_values_v1(tape.state_encoder.second_output),
            value_hidden,
            1,
            D::HIDDEN,
            D::HIDDEN,
            true,
            maxima,
        );
        forward_local_bounds_v1(
            &parameters,
            VALUE_SECOND_WEIGHT,
            value_hidden,
            tape.f32_values_v1(tape.value),
            1,
            D::HIDDEN,
            1,
            false,
            maxima,
        );
    }
    matches
}

/// D2 over one width and objective, printed only (the v3 pass rule is not
/// yet frozen): per investigated witness the reconstruction (a) and
/// accumulation (b) checks and the two-step agreement of the oracle's
/// upstream gradients, then the local bounds of every layer application.
#[allow(clippy::too_many_arguments)]
fn d2_dims_v1<D: NetDimsV1>(
    label: &str,
    model: &D::Net,
    objective: ObjectiveV1,
    report: &FdReportV1,
    captures: Vec<LinearBackwardCaptureV1>,
    r_n: f64,
    out: &mut impl Write,
) where
    D::Net: RecordingNetV1,
{
    let decisions = gradient_decisions_v1();
    let parameters = parameters_f64_v1(&model.parameter_snapshot_v1());
    let captures = captures_by_substep_v1(captures);
    let wide = !is_net8_dims_v1::<D>();
    for (sets, tensor, index) in d2_witnesses_v1(report, r_n, wide) {
        let (g_rust, g_fd) =
            d2_gradients_dims_v1::<D>(report, &parameters, &decisions, tensor, index);
        let terms = d2_terms_dims_v1::<D>(
            &parameters,
            &decisions,
            &report.targets,
            &report.advantages,
            &captures,
            tensor,
            index,
        );
        let total = terms.len();
        // Only loss-connected terms: a term with a structurally zero upstream
        // gradient (the value branch of a group's later substeps) is dropped.
        let connected: Vec<&D2TermV1> = terms
            .iter()
            .filter(|term| term.a32 != 0.0 || term.a64.iter().any(|a| *a != 0.0))
            .collect();
        let n = connected.len() as f64;
        let mut sum64 = 0.0;
        let mut mass64 = 0.0;
        let mut sum32 = 0.0;
        let mut mass32 = 0.0;
        let mut max_rel_a = 0.0f64;
        let mut max_rel_b = 0.0f64;
        let mut max_step_disagreement = 0.0f64;
        for term in &connected {
            let a64 = term.a64[1];
            let p64 = a64 * term.b64;
            let p32 = term.a32 * term.b32;
            sum64 += p64;
            mass64 += p64.abs();
            sum32 += p32;
            mass32 += p32.abs();
            if a64 != 0.0 {
                max_rel_a = max_rel_a.max((term.a32 - a64).abs() / a64.abs());
            }
            if term.b64 != 0.0 {
                max_rel_b = max_rel_b.max((term.b32 - term.b64).abs() / term.b64.abs());
            }
            max_step_disagreement =
                max_step_disagreement.max((term.a64[0] - a64).abs() / a64.abs());
        }
        let reconstruction = (sum64 - g_fd).abs();
        let reconstruction_bound = D2_RECONSTRUCTION_V1 * mass64;
        let accumulation = (g_rust - sum32).abs();
        let accumulation_bound = 2.0 * n * UNIT_ROUNDOFF_V1 * mass32;
        writeln!(
            out,
            "WIDE_FD_D2_V1 {objective:?} {label} {} [{index}] sets={sets} n={n} terms={total} \
             g_rust={g_rust:.10e} g_fd={g_fd:.10e} sum64={sum64:.10e} sum32={sum32:.10e} \
             (a) {reconstruction:.4e}<={reconstruction_bound:.4e} {} \
             (b) {accumulation:.4e}<={accumulation_bound:.4e} {} \
             steps max_rel={max_step_disagreement:.4e}<={D2_STEP_AGREEMENT_V1:e} {} \
             max_rel_a={max_rel_a:.4e} max_rel_b={max_rel_b:.4e}",
            EXPECTED_PARAMETER_NAMES[tensor],
            reconstruction <= reconstruction_bound,
            accumulation <= accumulation_bound,
            max_step_disagreement <= D2_STEP_AGREEMENT_V1,
        )
        .unwrap();
    }
    let mut maxima = LocalMaximaV1::new();
    for substep in &captures {
        for call in substep {
            backward_local_bounds_v1(call, &mut maxima);
        }
        tanh_gradient_local_bounds_v1(substep, &mut maxima);
    }
    let tape_matches = forward_local_bounds_dims_v1::<D>(model, &decisions, &captures, &mut maxima);
    writeln!(
        out,
        "WIDE_FD_D2_LOCAL_V1 {objective:?} {label} tape_matches_step={tape_matches}"
    )
    .unwrap();
    for ((tensor, kind), ratio) in &maxima {
        writeln!(
            out,
            "WIDE_FD_D2_LOCAL_V1 {objective:?} {label} {} {kind} max_ratio={ratio:.4e}",
            EXPECTED_PARAMETER_NAMES[*tensor]
        )
        .unwrap();
    }
}

/// D2 for both widths of one objective, the step run under the backward
/// capture.
fn d2_receipt_v1(objective: ObjectiveV1) {
    let narrow = perturbed_dims_v1::<Net8DimsV1>(narrow_model_v1(), 0xA11C_E5ED);
    let wide = perturbed_dims_v1::<Net8W128DimsV1>(wide_model_v1(), 0xA11C_E5ED);
    let (narrow_report, narrow_captures) = capture_linear_backward_for_test_v1(|| {
        fd_check_dims_v1::<Net8DimsV1>(&narrow, objective)
    });
    let (wide_report, wide_captures) = capture_linear_backward_for_test_v1(|| {
        fd_check_dims_v1::<Net8W128DimsV1>(&wide, objective)
    });
    let r_n = narrow_report.max_ratio;
    let mut out = Vec::new();
    d2_dims_v1::<Net8DimsV1>(
        "net-8",
        &narrow,
        objective,
        &narrow_report,
        narrow_captures,
        r_n,
        &mut out,
    );
    d2_dims_v1::<Net8W128DimsV1>(
        "net-8w128",
        &wide,
        objective,
        &wide_report,
        wide_captures,
        r_n,
        &mut out,
    );
    std::io::stderr().lock().write_all(&out).unwrap();
}

/// Amendment 5.1-A1 D2, GAE objective (prints only until v3).
#[test]
#[ignore = "D2 stays unrun and unread until the amendment reviewers agree"]
fn wide_fd_d2_attribution_gae() {
    d2_receipt_v1(ObjectiveV1::Gae);
}

/// Amendment 5.1-A1 D2, terminal objective (prints only until v3).
#[test]
#[ignore = "D2 stays unrun and unread until the amendment reviewers agree"]
fn wide_fd_d2_attribution_terminal() {
    d2_receipt_v1(ObjectiveV1::Terminal);
}

/// The input rows one weight tensor sees in one decision's oracle forward.
fn captured_rows_v1(
    parameters: &[Vec<f64>],
    hidden: usize,
    embedding: usize,
    decision: &GradientDecisionV1,
    weight: usize,
) -> Vec<Vec<f64>> {
    let mut run = OracleRunV1 {
        parameters,
        hook: None,
        capture: Some((weight, Vec::new())),
    };
    oracle_forward_run_v1(&mut run, hidden, embedding, &decision.view_v1(false));
    run.capture.take().unwrap().1
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
