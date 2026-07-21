//! Opt-in production-parameter Burn/CUDA forward diagnostic.
//!
//! This module is deliberately absent from normal builds. It proves one
//! narrow integration seam: the exact native Net8 parameter snapshot and
//! Python-authoritative encoded decisions can be packed into a ragged Burn
//! graph without changing the normal CPU reference or trainer contracts.

mod training;

use crate::common_model_snapshot_v1::{
    common_model_snapshot_paths_v1, load_common_model_snapshot_v1,
};
use crate::native_policy_train_step_v1::NativePolicyValueTrainStateV1;
use crate::native_policy_value_net_v1::{
    NativeEncodedDecisionSchemaV1, NativeEncodedDecisionViewV1, NativeNamedParameterV1,
    NativePolicyValueModelConfigV1, NativePolicyValueNetV1, NativePolicyValueOutputV1,
    ACTION_FEATURE_DIM_V1, ACTION_REF_FEATURE_DIM_V1, CARD_EMBEDDING_DIM_V1, CARD_VOCAB_SIZE_V1,
    EDGE_FEATURE_DIM_V1, HIDDEN_DIM_V1, OBJECT_FEATURE_DIM_V1, OBJECT_GROUP_COUNT_V1,
    PARAMETER_COUNT_V1, STATE_DIM_V1,
};
use burn::module::{Module, Param};
use burn::nn::{Embedding, Linear};
use burn::tensor::{backend::Backend, IndexingUpdateOp, Int, Tensor, TensorData};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::error::Error;
use std::hint::black_box;
use std::time::Instant;

const EXPERIMENTAL_BACKEND_IDENTITY_V1: &str =
    "mtg-kernel-experimental-burn-net8-packed-cuda-forward-v1";
const FIXTURE_SOURCE_IDENTITY_V1: &str =
    "python-full-features-v2-real-replay-cases-production-tensorizer-parity";
const PARAMETER_TENSOR_COUNT_V1: usize = 33;
// Burn/CUDA uses backend tanh/matmul implementations rather than the scalar
// native CPU loop, so this is a numerical-parity envelope, not bit identity.
const PARITY_ABSOLUTE_TOLERANCE_V1: f32 = 1.0e-3;
const PARITY_RELATIVE_TOLERANCE_V1: f32 = 1.0e-3;
const DEFAULT_WARMUP_ITERATIONS: usize = 10;
const DEFAULT_TIMED_ITERATIONS: usize = 30;
const BENCHMARK_BATCH_SIZES: [usize; 5] = [16, 64, 128, 256, 512];
const FIXTURE_BYTES: &[u8] =
    include_bytes!("../../data/flat_policy_v2/python_full_features_v2.json");

type CudaBackendV1 = burn_cuda::Cuda<f32, i32>;

#[derive(Module, Debug)]
struct TwoLayerTanh<B: Backend> {
    first: Linear<B>,
    second: Linear<B>,
}

impl<B: Backend> TwoLayerTanh<B> {
    fn forward(&self, input: Tensor<B, 2>) -> Tensor<B, 2> {
        self.second.forward(self.first.forward(input).tanh()).tanh()
    }
}

#[derive(Module, Debug)]
struct ScalarHead<B: Backend> {
    hidden: Linear<B>,
    output: Linear<B>,
}

impl<B: Backend> ScalarHead<B> {
    fn forward(&self, input: Tensor<B, 2>) -> Tensor<B, 2> {
        self.output.forward(self.hidden.forward(input).tanh())
    }
}

#[derive(Module, Debug)]
struct ProductionNet8<B: Backend> {
    card_embedding: Embedding<B>,
    object_encoder: TwoLayerTanh<B>,
    edge_encoder: TwoLayerTanh<B>,
    node_update: TwoLayerTanh<B>,
    state_encoder: TwoLayerTanh<B>,
    action_ref_encoder: TwoLayerTanh<B>,
    action_encoder: TwoLayerTanh<B>,
    scorer: ScalarHead<B>,
    value_head: ScalarHead<B>,
}

impl<B: Backend> ProductionNet8<B> {
    fn import_native_v1(
        parameters: &[NativeNamedParameterV1],
        device: &B::Device,
    ) -> Result<Self, Box<dyn Error>> {
        let mut cursor = ParameterCursor::new(parameters);
        let card_embedding = Embedding {
            weight: Param::from_data(
                TensorData::new(
                    cursor.take(
                        "card_embedding.weight",
                        &[CARD_VOCAB_SIZE_V1, CARD_EMBEDDING_DIM_V1],
                    )?,
                    [CARD_VOCAB_SIZE_V1, CARD_EMBEDDING_DIM_V1],
                ),
                device,
            ),
        };
        let object_encoder = cursor.two_layer(
            "object_encoder.0",
            OBJECT_FEATURE_DIM_V1 + CARD_EMBEDDING_DIM_V1,
            "object_encoder.2",
            HIDDEN_DIM_V1,
            device,
        )?;
        let edge_encoder = cursor.two_layer(
            "edge_encoder.0",
            EDGE_FEATURE_DIM_V1 + HIDDEN_DIM_V1 * 2,
            "edge_encoder.2",
            HIDDEN_DIM_V1,
            device,
        )?;
        let node_update = cursor.two_layer(
            "node_update.0",
            HIDDEN_DIM_V1 * 2,
            "node_update.2",
            HIDDEN_DIM_V1,
            device,
        )?;
        let state_encoder = cursor.two_layer(
            "state_encoder.0",
            STATE_DIM_V1 + OBJECT_GROUP_COUNT_V1 * HIDDEN_DIM_V1,
            "state_encoder.2",
            HIDDEN_DIM_V1,
            device,
        )?;
        let action_ref_encoder = cursor.two_layer(
            "action_ref_encoder.0",
            ACTION_REF_FEATURE_DIM_V1 + HIDDEN_DIM_V1,
            "action_ref_encoder.2",
            HIDDEN_DIM_V1,
            device,
        )?;
        let action_encoder = cursor.two_layer(
            "action_encoder.0",
            ACTION_FEATURE_DIM_V1 + HIDDEN_DIM_V1,
            "action_encoder.2",
            HIDDEN_DIM_V1,
            device,
        )?;
        let scorer = ScalarHead {
            hidden: cursor.linear("scorer.0", HIDDEN_DIM_V1 * 2, HIDDEN_DIM_V1, device)?,
            output: cursor.linear("scorer.2", HIDDEN_DIM_V1, 1, device)?,
        };
        let value_head = ScalarHead {
            hidden: cursor.linear("value_head.0", HIDDEN_DIM_V1, HIDDEN_DIM_V1, device)?,
            output: cursor.linear("value_head.2", HIDDEN_DIM_V1, 1, device)?,
        };
        cursor.finish()?;
        Ok(Self {
            card_embedding,
            object_encoder,
            edge_encoder,
            node_update,
            state_encoder,
            action_ref_encoder,
            action_encoder,
            scorer,
            value_head,
        })
    }

    fn forward(&self, batch: &DevicePackedBatch<B>) -> (Tensor<B, 1>, Tensor<B, 1>) {
        let object_card = self
            .card_embedding
            .forward(batch.object_card_ids.clone().unsqueeze_dim::<2>(1))
            .squeeze_dim::<2>(1);
        let object_base = self.object_encoder.forward(Tensor::cat(
            vec![batch.object_features.clone(), object_card],
            1,
        ));

        let edge_hidden = self.edge_encoder.forward(Tensor::cat(
            vec![
                batch.edge_features.clone(),
                object_base
                    .clone()
                    .select(0, batch.edge_source_indices.clone()),
                object_base
                    .clone()
                    .select(0, batch.edge_target_indices.clone()),
            ],
            1,
        ));
        let source_scatter = batch
            .edge_source_indices
            .clone()
            .unsqueeze_dim::<2>(1)
            .expand([batch.edge_count, HIDDEN_DIM_V1]);
        let target_scatter = batch
            .edge_target_indices
            .clone()
            .unsqueeze_dim::<2>(1)
            .expand([batch.edge_count, HIDDEN_DIM_V1]);
        // Preserve the native/Python ordering: all source contributions in
        // edge order, then all target contributions in edge order.
        let edge_pooled = Tensor::zeros([batch.object_count, HIDDEN_DIM_V1], &batch.device)
            .scatter(
                0,
                source_scatter,
                edge_hidden.clone(),
                IndexingUpdateOp::Add,
            )
            .scatter(0, target_scatter, edge_hidden, IndexingUpdateOp::Add);
        let object_hidden = self
            .node_update
            .forward(Tensor::cat(vec![object_base, edge_pooled], 1));

        let object_group_scatter = batch
            .object_group_indices
            .clone()
            .unsqueeze_dim::<2>(1)
            .expand([batch.object_count, HIDDEN_DIM_V1]);
        let pooled_objects = Tensor::zeros(
            [batch.decision_count * OBJECT_GROUP_COUNT_V1, HIDDEN_DIM_V1],
            &batch.device,
        )
        .scatter(
            0,
            object_group_scatter,
            object_hidden.clone(),
            IndexingUpdateOp::Add,
        )
        .reshape([batch.decision_count, OBJECT_GROUP_COUNT_V1 * HIDDEN_DIM_V1]);
        let state_hidden = self
            .state_encoder
            .forward(Tensor::cat(vec![batch.state.clone(), pooled_objects], 1));

        let action_ref_hidden = self.action_ref_encoder.forward(Tensor::cat(
            vec![
                batch.action_ref_features.clone(),
                object_hidden.select(0, batch.action_ref_node_indices.clone()),
            ],
            1,
        ));
        let action_ref_scatter = batch
            .action_ref_action_indices
            .clone()
            .unsqueeze_dim::<2>(1)
            .expand([batch.action_ref_count, HIDDEN_DIM_V1]);
        let action_ref_pooled = Tensor::zeros([batch.action_count, HIDDEN_DIM_V1], &batch.device)
            .scatter(
                0,
                action_ref_scatter,
                action_ref_hidden,
                IndexingUpdateOp::Add,
            );
        let action_hidden = self.action_encoder.forward(Tensor::cat(
            vec![batch.action_features.clone(), action_ref_pooled],
            1,
        ));
        let action_state = state_hidden
            .clone()
            .select(0, batch.action_decision_indices.clone());
        let logits = self
            .scorer
            .forward(Tensor::cat(vec![action_state, action_hidden], 1))
            .squeeze_dim::<1>(1);
        let values = self.value_head.forward(state_hidden).squeeze_dim::<1>(1);
        (logits, values)
    }

    fn export_native_v1(
        &self,
        device: &B::Device,
    ) -> Result<Vec<NativeNamedParameterV1>, Box<dyn Error>> {
        B::sync(device)?;
        let mut output = Vec::with_capacity(PARAMETER_TENSOR_COUNT_V1);
        output.push(NativeNamedParameterV1 {
            name: "card_embedding.weight",
            shape: vec![CARD_VOCAB_SIZE_V1, CARD_EMBEDDING_DIM_V1],
            values: self
                .card_embedding
                .weight
                .val()
                .into_data()
                .to_vec::<f32>()?,
        });
        export_two_layer(
            &mut output,
            "object_encoder.0",
            OBJECT_FEATURE_DIM_V1 + CARD_EMBEDDING_DIM_V1,
            "object_encoder.2",
            HIDDEN_DIM_V1,
            &self.object_encoder,
        )?;
        export_two_layer(
            &mut output,
            "edge_encoder.0",
            EDGE_FEATURE_DIM_V1 + HIDDEN_DIM_V1 * 2,
            "edge_encoder.2",
            HIDDEN_DIM_V1,
            &self.edge_encoder,
        )?;
        export_two_layer(
            &mut output,
            "node_update.0",
            HIDDEN_DIM_V1 * 2,
            "node_update.2",
            HIDDEN_DIM_V1,
            &self.node_update,
        )?;
        export_two_layer(
            &mut output,
            "state_encoder.0",
            STATE_DIM_V1 + OBJECT_GROUP_COUNT_V1 * HIDDEN_DIM_V1,
            "state_encoder.2",
            HIDDEN_DIM_V1,
            &self.state_encoder,
        )?;
        export_two_layer(
            &mut output,
            "action_ref_encoder.0",
            ACTION_REF_FEATURE_DIM_V1 + HIDDEN_DIM_V1,
            "action_ref_encoder.2",
            HIDDEN_DIM_V1,
            &self.action_ref_encoder,
        )?;
        export_two_layer(
            &mut output,
            "action_encoder.0",
            ACTION_FEATURE_DIM_V1 + HIDDEN_DIM_V1,
            "action_encoder.2",
            HIDDEN_DIM_V1,
            &self.action_encoder,
        )?;
        export_linear(
            &mut output,
            "scorer.0",
            HIDDEN_DIM_V1 * 2,
            HIDDEN_DIM_V1,
            &self.scorer.hidden,
        )?;
        export_linear(
            &mut output,
            "scorer.2",
            HIDDEN_DIM_V1,
            1,
            &self.scorer.output,
        )?;
        export_linear(
            &mut output,
            "value_head.0",
            HIDDEN_DIM_V1,
            HIDDEN_DIM_V1,
            &self.value_head.hidden,
        )?;
        export_linear(
            &mut output,
            "value_head.2",
            HIDDEN_DIM_V1,
            1,
            &self.value_head.output,
        )?;
        Ok(output)
    }
}

struct ParameterCursor<'a> {
    parameters: &'a [NativeNamedParameterV1],
    position: usize,
}

impl<'a> ParameterCursor<'a> {
    fn new(parameters: &'a [NativeNamedParameterV1]) -> Self {
        Self {
            parameters,
            position: 0,
        }
    }

    fn take(&mut self, name: &'static str, shape: &[usize]) -> Result<Vec<f32>, Box<dyn Error>> {
        let parameter = self
            .parameters
            .get(self.position)
            .ok_or("production parameter snapshot ended early")?;
        if parameter.name != name || parameter.shape != shape {
            return Err(format!(
                "parameter mapping mismatch at ordinal {}: expected {name:?} {shape:?}, got {:?} {:?}",
                self.position, parameter.name, parameter.shape
            )
            .into());
        }
        self.position += 1;
        Ok(parameter.values.clone())
    }

    fn linear<B: Backend>(
        &mut self,
        prefix: &'static str,
        input: usize,
        output: usize,
        device: &B::Device,
    ) -> Result<Linear<B>, Box<dyn Error>> {
        let (weight_name, bias_name) = parameter_names(prefix)?;
        let native_weight = self.take(weight_name, &[output, input])?;
        let burn_weight = transpose_output_input_to_input_output(&native_weight, input, output)?;
        let bias = self.take(bias_name, &[output])?;
        Ok(Linear {
            weight: Param::from_data(TensorData::new(burn_weight, [input, output]), device),
            bias: Some(Param::from_data(TensorData::new(bias, [output]), device)),
        })
    }

    fn two_layer<B: Backend>(
        &mut self,
        first_prefix: &'static str,
        first_input: usize,
        second_prefix: &'static str,
        second_input: usize,
        device: &B::Device,
    ) -> Result<TwoLayerTanh<B>, Box<dyn Error>> {
        Ok(TwoLayerTanh {
            first: self.linear(first_prefix, first_input, HIDDEN_DIM_V1, device)?,
            second: self.linear(second_prefix, second_input, HIDDEN_DIM_V1, device)?,
        })
    }

    fn finish(self) -> Result<(), Box<dyn Error>> {
        if self.position != PARAMETER_TENSOR_COUNT_V1 || self.position != self.parameters.len() {
            return Err(format!(
                "parameter mapping consumed {} tensors, snapshot contains {}",
                self.position,
                self.parameters.len()
            )
            .into());
        }
        Ok(())
    }
}

fn parameter_names(prefix: &'static str) -> Result<(&'static str, &'static str), Box<dyn Error>> {
    let names = match prefix {
        "object_encoder.0" => ("object_encoder.0.weight", "object_encoder.0.bias"),
        "object_encoder.2" => ("object_encoder.2.weight", "object_encoder.2.bias"),
        "edge_encoder.0" => ("edge_encoder.0.weight", "edge_encoder.0.bias"),
        "edge_encoder.2" => ("edge_encoder.2.weight", "edge_encoder.2.bias"),
        "node_update.0" => ("node_update.0.weight", "node_update.0.bias"),
        "node_update.2" => ("node_update.2.weight", "node_update.2.bias"),
        "state_encoder.0" => ("state_encoder.0.weight", "state_encoder.0.bias"),
        "state_encoder.2" => ("state_encoder.2.weight", "state_encoder.2.bias"),
        "action_ref_encoder.0" => ("action_ref_encoder.0.weight", "action_ref_encoder.0.bias"),
        "action_ref_encoder.2" => ("action_ref_encoder.2.weight", "action_ref_encoder.2.bias"),
        "action_encoder.0" => ("action_encoder.0.weight", "action_encoder.0.bias"),
        "action_encoder.2" => ("action_encoder.2.weight", "action_encoder.2.bias"),
        "scorer.0" => ("scorer.0.weight", "scorer.0.bias"),
        "scorer.2" => ("scorer.2.weight", "scorer.2.bias"),
        "value_head.0" => ("value_head.0.weight", "value_head.0.bias"),
        "value_head.2" => ("value_head.2.weight", "value_head.2.bias"),
        _ => return Err(format!("unknown production parameter prefix {prefix}").into()),
    };
    Ok(names)
}

fn transpose_output_input_to_input_output(
    native: &[f32],
    input: usize,
    output: usize,
) -> Result<Vec<f32>, Box<dyn Error>> {
    if native.len() != input * output {
        return Err("linear parameter transpose length mismatch".into());
    }
    let mut burn = vec![0.0; native.len()];
    for output_index in 0..output {
        for input_index in 0..input {
            burn[input_index * output + output_index] = native[output_index * input + input_index];
        }
    }
    Ok(burn)
}

fn transpose_input_output_to_output_input(
    burn: &[f32],
    input: usize,
    output: usize,
) -> Result<Vec<f32>, Box<dyn Error>> {
    if burn.len() != input * output {
        return Err("linear parameter inverse transpose length mismatch".into());
    }
    let mut native = vec![0.0; burn.len()];
    for output_index in 0..output {
        for input_index in 0..input {
            native[output_index * input + input_index] = burn[input_index * output + output_index];
        }
    }
    Ok(native)
}

fn export_two_layer<B: Backend>(
    output: &mut Vec<NativeNamedParameterV1>,
    first_prefix: &'static str,
    first_input: usize,
    second_prefix: &'static str,
    second_input: usize,
    layer: &TwoLayerTanh<B>,
) -> Result<(), Box<dyn Error>> {
    export_linear(
        output,
        first_prefix,
        first_input,
        HIDDEN_DIM_V1,
        &layer.first,
    )?;
    export_linear(
        output,
        second_prefix,
        second_input,
        HIDDEN_DIM_V1,
        &layer.second,
    )
}

fn export_linear<B: Backend>(
    output_parameters: &mut Vec<NativeNamedParameterV1>,
    prefix: &'static str,
    input: usize,
    output: usize,
    linear: &Linear<B>,
) -> Result<(), Box<dyn Error>> {
    let (weight_name, bias_name) = parameter_names(prefix)?;
    let burn_weight = linear.weight.val().into_data().to_vec::<f32>()?;
    output_parameters.push(NativeNamedParameterV1 {
        name: weight_name,
        shape: vec![output, input],
        values: transpose_input_output_to_output_input(&burn_weight, input, output)?,
    });
    let bias = linear
        .bias
        .as_ref()
        .ok_or("production linear unexpectedly has no bias")?
        .val()
        .into_data()
        .to_vec::<f32>()?;
    output_parameters.push(NativeNamedParameterV1 {
        name: bias_name,
        shape: vec![output],
        values: bias,
    });
    Ok(())
}

#[derive(Deserialize)]
struct FixtureDocument {
    cases: Vec<FixtureCase>,
}

#[derive(Deserialize)]
struct FixtureCase {
    name: String,
    tensors: FixtureTensors,
}

#[derive(Deserialize)]
struct FixtureTensors {
    state: F32Tensor,
    object_features: F32Tensor,
    object_card_ids: I64Tensor,
    object_groups: I64Tensor,
    object_node_ids: I64Tensor,
    edge_features: F32Tensor,
    edge_source_indices: I64Tensor,
    edge_target_indices: I64Tensor,
    action_features: F32Tensor,
    action_ref_features: F32Tensor,
    action_ref_card_ids: I64Tensor,
    action_ref_action_indices: I64Tensor,
    action_ref_node_indices: I64Tensor,
}

#[derive(Deserialize)]
struct F32Tensor {
    f32_le_hex: String,
    shape: Vec<usize>,
}

#[derive(Deserialize)]
struct I64Tensor {
    i64_values: Vec<i64>,
    shape: Vec<usize>,
}

#[derive(Clone)]
struct EncodedDecisionOwned {
    name: String,
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

impl EncodedDecisionOwned {
    fn from_fixture(case: FixtureCase) -> Result<Self, Box<dyn Error>> {
        validate_shape("state", &case.tensors.state.shape, &[STATE_DIM_V1])?;
        validate_rank2_tail(
            "object_features",
            &case.tensors.object_features.shape,
            OBJECT_FEATURE_DIM_V1,
        )?;
        validate_rank2_tail(
            "edge_features",
            &case.tensors.edge_features.shape,
            EDGE_FEATURE_DIM_V1,
        )?;
        validate_rank2_tail(
            "action_features",
            &case.tensors.action_features.shape,
            ACTION_FEATURE_DIM_V1,
        )?;
        validate_rank2_tail(
            "action_ref_features",
            &case.tensors.action_ref_features.shape,
            ACTION_REF_FEATURE_DIM_V1,
        )?;
        validate_i64_shape(&case.tensors.object_card_ids)?;
        validate_i64_shape(&case.tensors.object_groups)?;
        validate_i64_shape(&case.tensors.object_node_ids)?;
        validate_i64_shape(&case.tensors.edge_source_indices)?;
        validate_i64_shape(&case.tensors.edge_target_indices)?;
        validate_i64_shape(&case.tensors.action_ref_card_ids)?;
        validate_i64_shape(&case.tensors.action_ref_action_indices)?;
        validate_i64_shape(&case.tensors.action_ref_node_indices)?;
        Ok(Self {
            name: case.name,
            state: decode_f32_hex(&case.tensors.state.f32_le_hex)?,
            object_features: decode_f32_hex(&case.tensors.object_features.f32_le_hex)?,
            object_card_ids: case.tensors.object_card_ids.i64_values,
            object_groups: case.tensors.object_groups.i64_values,
            object_node_ids: case.tensors.object_node_ids.i64_values,
            edge_features: decode_f32_hex(&case.tensors.edge_features.f32_le_hex)?,
            edge_source_indices: case.tensors.edge_source_indices.i64_values,
            edge_target_indices: case.tensors.edge_target_indices.i64_values,
            action_features: decode_f32_hex(&case.tensors.action_features.f32_le_hex)?,
            action_ref_features: decode_f32_hex(&case.tensors.action_ref_features.f32_le_hex)?,
            action_ref_card_ids: case.tensors.action_ref_card_ids.i64_values,
            action_ref_action_indices: case.tensors.action_ref_action_indices.i64_values,
            action_ref_node_indices: case.tensors.action_ref_node_indices.i64_values,
        })
    }

    fn view(&self) -> NativeEncodedDecisionViewV1<'_> {
        NativeEncodedDecisionViewV1::from_slices_unvalidated(
            NativeEncodedDecisionSchemaV1::contract_v1(),
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

    fn object_count(&self) -> usize {
        self.object_card_ids.len()
    }

    fn edge_count(&self) -> usize {
        self.edge_source_indices.len()
    }

    fn action_count(&self) -> usize {
        self.action_features.len() / ACTION_FEATURE_DIM_V1
    }

    fn action_ref_count(&self) -> usize {
        self.action_ref_action_indices.len()
    }
}

fn validate_shape(field: &str, actual: &[usize], expected: &[usize]) -> Result<(), Box<dyn Error>> {
    if actual != expected {
        return Err(format!("fixture {field} shape mismatch: {actual:?} != {expected:?}").into());
    }
    Ok(())
}

fn validate_rank2_tail(field: &str, shape: &[usize], tail: usize) -> Result<(), Box<dyn Error>> {
    if shape.len() != 2 || shape[1] != tail {
        return Err(format!("fixture {field} rank/tail mismatch: {shape:?}").into());
    }
    Ok(())
}

fn validate_i64_shape(tensor: &I64Tensor) -> Result<(), Box<dyn Error>> {
    if tensor.shape != [tensor.i64_values.len()] {
        return Err("fixture i64 tensor shape mismatch".into());
    }
    Ok(())
}

fn decode_f32_hex(raw: &str) -> Result<Vec<f32>, Box<dyn Error>> {
    if !raw.len().is_multiple_of(8) {
        return Err("fixture f32 hex is not aligned to four-byte elements".into());
    }
    let mut output = Vec::with_capacity(raw.len() / 8);
    for chunk in raw.as_bytes().chunks_exact(8) {
        let text = std::str::from_utf8(chunk)?;
        let mut bytes = [0_u8; 4];
        for (index, pair) in text.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = u8::from_str_radix(std::str::from_utf8(pair)?, 16)?;
        }
        output.push(f32::from_le_bytes(bytes));
    }
    Ok(output)
}

#[derive(Default)]
struct HostPackingWorkspace {
    action_offsets: Vec<usize>,
    case_indices: Vec<usize>,
    state: Vec<f32>,
    object_features: Vec<f32>,
    object_card_ids: Vec<i32>,
    object_group_indices: Vec<i32>,
    edge_features: Vec<f32>,
    edge_source_indices: Vec<i32>,
    edge_target_indices: Vec<i32>,
    action_features: Vec<f32>,
    action_decision_indices: Vec<i32>,
    action_ref_features: Vec<f32>,
    action_ref_action_indices: Vec<i32>,
    action_ref_node_indices: Vec<i32>,
}

impl HostPackingWorkspace {
    fn reserve_for(&mut self, cases: &[EncodedDecisionOwned], decisions: usize) {
        let max_objects = cases
            .iter()
            .map(EncodedDecisionOwned::object_count)
            .max()
            .unwrap_or(0);
        let max_edges = cases
            .iter()
            .map(EncodedDecisionOwned::edge_count)
            .max()
            .unwrap_or(0);
        let max_actions = cases
            .iter()
            .map(EncodedDecisionOwned::action_count)
            .max()
            .unwrap_or(0);
        let max_refs = cases
            .iter()
            .map(EncodedDecisionOwned::action_ref_count)
            .max()
            .unwrap_or(0);
        reserve_exact_at_least(&mut self.action_offsets, decisions + 1);
        reserve_exact_at_least(&mut self.case_indices, decisions);
        reserve_exact_at_least(&mut self.state, decisions * STATE_DIM_V1);
        reserve_exact_at_least(
            &mut self.object_features,
            decisions * max_objects * OBJECT_FEATURE_DIM_V1,
        );
        reserve_exact_at_least(&mut self.object_card_ids, decisions * max_objects);
        reserve_exact_at_least(&mut self.object_group_indices, decisions * max_objects);
        reserve_exact_at_least(
            &mut self.edge_features,
            decisions * max_edges * EDGE_FEATURE_DIM_V1,
        );
        reserve_exact_at_least(&mut self.edge_source_indices, decisions * max_edges);
        reserve_exact_at_least(&mut self.edge_target_indices, decisions * max_edges);
        reserve_exact_at_least(
            &mut self.action_features,
            decisions * max_actions * ACTION_FEATURE_DIM_V1,
        );
        reserve_exact_at_least(&mut self.action_decision_indices, decisions * max_actions);
        reserve_exact_at_least(
            &mut self.action_ref_features,
            decisions * max_refs * ACTION_REF_FEATURE_DIM_V1,
        );
        reserve_exact_at_least(&mut self.action_ref_action_indices, decisions * max_refs);
        reserve_exact_at_least(&mut self.action_ref_node_indices, decisions * max_refs);
    }

    fn clear(&mut self) {
        self.action_offsets.clear();
        self.case_indices.clear();
        self.state.clear();
        self.object_features.clear();
        self.object_card_ids.clear();
        self.object_group_indices.clear();
        self.edge_features.clear();
        self.edge_source_indices.clear();
        self.edge_target_indices.clear();
        self.action_features.clear();
        self.action_decision_indices.clear();
        self.action_ref_features.clear();
        self.action_ref_action_indices.clear();
        self.action_ref_node_indices.clear();
    }

    fn pack(
        &mut self,
        cases: &[EncodedDecisionOwned],
        decisions: usize,
        rotation: usize,
    ) -> Result<(), Box<dyn Error>> {
        self.clear();
        self.action_offsets.push(0);
        for decision_index in 0..decisions {
            let case_index = (rotation + decision_index) % cases.len();
            let case = &cases[case_index];
            let object_offset = self.object_card_ids.len();
            let action_offset = self.action_decision_indices.len();
            self.case_indices.push(case_index);
            self.state.extend_from_slice(&case.state);
            self.object_features
                .extend_from_slice(&case.object_features);
            for card_id in case.object_card_ids.iter().copied() {
                self.object_card_ids.push(checked_i32(card_id)?);
            }
            for group in case.object_groups.iter().copied() {
                let group = usize::try_from(group)?;
                let global = decision_index
                    .checked_mul(OBJECT_GROUP_COUNT_V1)
                    .and_then(|base| base.checked_add(group))
                    .ok_or("object group index overflow")?;
                self.object_group_indices.push(i32::try_from(global)?);
            }
            self.edge_features.extend_from_slice(&case.edge_features);
            for index in case.edge_source_indices.iter().copied() {
                self.edge_source_indices
                    .push(checked_offset_i32(index, object_offset)?);
            }
            for index in case.edge_target_indices.iter().copied() {
                self.edge_target_indices
                    .push(checked_offset_i32(index, object_offset)?);
            }
            self.action_features
                .extend_from_slice(&case.action_features);
            self.action_decision_indices.extend(std::iter::repeat_n(
                i32::try_from(decision_index)?,
                case.action_count(),
            ));
            self.action_ref_features
                .extend_from_slice(&case.action_ref_features);
            for index in case.action_ref_action_indices.iter().copied() {
                self.action_ref_action_indices
                    .push(checked_offset_i32(index, action_offset)?);
            }
            for index in case.action_ref_node_indices.iter().copied() {
                self.action_ref_node_indices
                    .push(checked_offset_i32(index, object_offset)?);
            }
            self.action_offsets.push(self.action_decision_indices.len());
        }
        Ok(())
    }

    fn capacities(&self) -> [usize; 14] {
        [
            self.action_offsets.capacity(),
            self.case_indices.capacity(),
            self.state.capacity(),
            self.object_features.capacity(),
            self.object_card_ids.capacity(),
            self.object_group_indices.capacity(),
            self.edge_features.capacity(),
            self.edge_source_indices.capacity(),
            self.edge_target_indices.capacity(),
            self.action_features.capacity(),
            self.action_decision_indices.capacity(),
            self.action_ref_features.capacity(),
            self.action_ref_action_indices.capacity(),
            self.action_ref_node_indices.capacity(),
        ]
    }
}

fn reserve_exact_at_least<T>(values: &mut Vec<T>, minimum: usize) {
    if values.capacity() < minimum {
        values.reserve_exact(minimum.saturating_sub(values.len()));
    }
}

fn checked_i32(value: i64) -> Result<i32, Box<dyn Error>> {
    Ok(i32::try_from(value)?)
}

fn checked_offset_i32(value: i64, offset: usize) -> Result<i32, Box<dyn Error>> {
    let value = usize::try_from(value)?;
    Ok(i32::try_from(
        value.checked_add(offset).ok_or("packed index overflow")?,
    )?)
}

struct DevicePackedBatch<B: Backend> {
    device: B::Device,
    decision_count: usize,
    object_count: usize,
    edge_count: usize,
    action_count: usize,
    action_ref_count: usize,
    state: Tensor<B, 2>,
    object_features: Tensor<B, 2>,
    object_card_ids: Tensor<B, 1, Int>,
    object_group_indices: Tensor<B, 1, Int>,
    edge_features: Tensor<B, 2>,
    edge_source_indices: Tensor<B, 1, Int>,
    edge_target_indices: Tensor<B, 1, Int>,
    action_features: Tensor<B, 2>,
    action_decision_indices: Tensor<B, 1, Int>,
    action_ref_features: Tensor<B, 2>,
    action_ref_action_indices: Tensor<B, 1, Int>,
    action_ref_node_indices: Tensor<B, 1, Int>,
}

impl<B: Backend> DevicePackedBatch<B> {
    fn upload(device: &B::Device, host: &HostPackingWorkspace) -> Self {
        let decision_count = host.case_indices.len();
        let object_count = host.object_card_ids.len();
        let edge_count = host.edge_source_indices.len();
        let action_count = host.action_decision_indices.len();
        let action_ref_count = host.action_ref_action_indices.len();
        Self {
            device: device.clone(),
            decision_count,
            object_count,
            edge_count,
            action_count,
            action_ref_count,
            state: Tensor::from_data(
                TensorData::new(host.state.clone(), [decision_count, STATE_DIM_V1]),
                device,
            ),
            object_features: Tensor::from_data(
                TensorData::new(
                    host.object_features.clone(),
                    [object_count, OBJECT_FEATURE_DIM_V1],
                ),
                device,
            ),
            object_card_ids: Tensor::from_data(
                TensorData::new(host.object_card_ids.clone(), [object_count]),
                device,
            ),
            object_group_indices: Tensor::from_data(
                TensorData::new(host.object_group_indices.clone(), [object_count]),
                device,
            ),
            edge_features: Tensor::from_data(
                TensorData::new(
                    host.edge_features.clone(),
                    [edge_count, EDGE_FEATURE_DIM_V1],
                ),
                device,
            ),
            edge_source_indices: Tensor::from_data(
                TensorData::new(host.edge_source_indices.clone(), [edge_count]),
                device,
            ),
            edge_target_indices: Tensor::from_data(
                TensorData::new(host.edge_target_indices.clone(), [edge_count]),
                device,
            ),
            action_features: Tensor::from_data(
                TensorData::new(
                    host.action_features.clone(),
                    [action_count, ACTION_FEATURE_DIM_V1],
                ),
                device,
            ),
            action_decision_indices: Tensor::from_data(
                TensorData::new(host.action_decision_indices.clone(), [action_count]),
                device,
            ),
            action_ref_features: Tensor::from_data(
                TensorData::new(
                    host.action_ref_features.clone(),
                    [action_ref_count, ACTION_REF_FEATURE_DIM_V1],
                ),
                device,
            ),
            action_ref_action_indices: Tensor::from_data(
                TensorData::new(host.action_ref_action_indices.clone(), [action_ref_count]),
                device,
            ),
            action_ref_node_indices: Tensor::from_data(
                TensorData::new(host.action_ref_node_indices.clone(), [action_ref_count]),
                device,
            ),
        }
    }
}

struct OutputData {
    logits: Vec<f32>,
    values: Vec<f32>,
}

fn read_output<B: Backend>(
    output: (Tensor<B, 1>, Tensor<B, 1>),
) -> Result<OutputData, Box<dyn Error>> {
    let output = OutputData {
        logits: output.0.into_data().to_vec::<f32>()?,
        values: output.1.into_data().to_vec::<f32>()?,
    };
    if !output
        .logits
        .iter()
        .chain(&output.values)
        .all(|value| value.is_finite())
    {
        return Err("Burn/CUDA output contains non-finite values".into());
    }
    Ok(output)
}

struct ParitySummary {
    maximum_absolute_error: f32,
    maximum_relative_error: f32,
    maximum_tolerance_ratio: f32,
    bit_equal_outputs: usize,
    output_count: usize,
}

fn compare_to_native(
    host: &HostPackingWorkspace,
    output: &OutputData,
    expected: &[NativePolicyValueOutputV1],
) -> Result<ParitySummary, Box<dyn Error>> {
    if output.values.len() != host.case_indices.len()
        || output.logits.len() != *host.action_offsets.last().unwrap_or(&0)
    {
        return Err("Burn/CUDA output shape differs from packed host shape".into());
    }
    let mut summary = ParitySummary {
        maximum_absolute_error: 0.0,
        maximum_relative_error: 0.0,
        maximum_tolerance_ratio: 0.0,
        bit_equal_outputs: 0,
        output_count: 0,
    };
    for (decision, case_index) in host.case_indices.iter().copied().enumerate() {
        let reference = &expected[case_index];
        let begin = host.action_offsets[decision];
        let end = host.action_offsets[decision + 1];
        if end - begin != reference.logits.len() {
            return Err("native and packed action counts differ".into());
        }
        for (actual, expected) in output.logits[begin..end]
            .iter()
            .copied()
            .zip(reference.logits.iter().copied())
            .chain(std::iter::once((output.values[decision], reference.value)))
        {
            let absolute = (actual - expected).abs();
            let relative = absolute / expected.abs().max(f32::MIN_POSITIVE);
            summary.maximum_absolute_error = summary.maximum_absolute_error.max(absolute);
            summary.maximum_relative_error = summary.maximum_relative_error.max(relative);
            summary.output_count += 1;
            if actual.to_bits() == expected.to_bits() {
                summary.bit_equal_outputs += 1;
            }
            let permitted =
                PARITY_ABSOLUTE_TOLERANCE_V1 + PARITY_RELATIVE_TOLERANCE_V1 * expected.abs();
            summary.maximum_tolerance_ratio =
                summary.maximum_tolerance_ratio.max(absolute / permitted);
            if absolute > permitted {
                return Err(format!(
                    "CUDA parity tolerance exceeded: actual={actual:?} expected={expected:?} absolute={absolute:?} permitted={permitted:?}"
                )
                .into());
            }
        }
    }
    Ok(summary)
}

fn parameter_manifest_sha256(parameters: &[NativeNamedParameterV1]) -> String {
    let mut digest = Sha256::new();
    for parameter in parameters {
        let name = parameter.name.as_bytes();
        digest.update((name.len() as u32).to_be_bytes());
        digest.update(name);
        digest.update((parameter.shape.len() as u32).to_be_bytes());
        for dimension in &parameter.shape {
            digest.update((*dimension as u64).to_be_bytes());
        }
        digest.update((parameter.values.len() as u64).to_be_bytes());
        for value in &parameter.values {
            digest.update(value.to_le_bytes());
        }
    }
    format!("{:x}", digest.finalize())
}

fn parameter_snapshots_are_bit_exact(
    left: &[NativeNamedParameterV1],
    right: &[NativeNamedParameterV1],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.name == right.name
                && left.shape == right.shape
                && left.values.len() == right.values.len()
                && left
                    .values
                    .iter()
                    .zip(&right.values)
                    .all(|(left, right)| left.to_bits() == right.to_bits())
        })
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn argument_usize(flag: &str, default: usize) -> Result<usize, Box<dyn Error>> {
    let arguments = std::env::args().collect::<Vec<_>>();
    let Some(position) = arguments.iter().position(|argument| argument == flag) else {
        return Ok(default);
    };
    let value = arguments
        .get(position + 1)
        .ok_or_else(|| format!("{flag} requires a value"))?
        .parse::<usize>()?;
    if value == 0 {
        return Err(format!("{flag} must be positive").into());
    }
    Ok(value)
}

fn percentile_micros(samples: &[f64], percentile: f64) -> f64 {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = ((sorted.len() as f64 * percentile).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted[rank]
}

fn mean_micros(samples: &[f64]) -> f64 {
    samples.iter().sum::<f64>() / samples.len() as f64
}

fn load_real_fixture_cases() -> Result<Vec<EncodedDecisionOwned>, Box<dyn Error>> {
    let fixture: FixtureDocument = serde_json::from_slice(FIXTURE_BYTES)?;
    let cases = fixture
        .cases
        .into_iter()
        .filter(|case| !case.name.starts_with("synthetic-"))
        .map(EncodedDecisionOwned::from_fixture)
        .collect::<Result<Vec<_>, _>>()?;
    if cases.is_empty() {
        return Err("production fixture contains no real replay cases".into());
    }
    Ok(cases)
}

fn benchmark_one_batch<B: Backend>(
    device: &B::Device,
    model: &ProductionNet8<B>,
    cases: &[EncodedDecisionOwned],
    expected: &[NativePolicyValueOutputV1],
    decisions: usize,
    warmup: usize,
    iterations: usize,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let mut workspace = HostPackingWorkspace::default();
    workspace.reserve_for(cases, decisions);
    let capacities = workspace.capacities();
    workspace.pack(cases, decisions, 0)?;
    if workspace.capacities() != capacities {
        return Err("host packing workspace reallocated after explicit reserve".into());
    }
    let resident_batch = DevicePackedBatch::<B>::upload(device, &workspace);
    B::sync(device)?;

    for _ in 0..warmup {
        let output = model.forward(&resident_batch);
        B::sync(device)?;
        black_box(output);
    }
    let mut forward_samples = Vec::with_capacity(iterations);
    let mut last_output = None;
    for _ in 0..iterations {
        let start = Instant::now();
        let output = model.forward(&resident_batch);
        B::sync(device)?;
        forward_samples.push(start.elapsed().as_secs_f64() * 1.0e6);
        last_output = Some(black_box(output));
    }
    let resident_output = read_output(last_output.ok_or("forward benchmark produced no output")?)?;
    let parity = compare_to_native(&workspace, &resident_output, expected)?;

    for rotation in 0..warmup {
        workspace.pack(cases, decisions, rotation)?;
        let batch = DevicePackedBatch::<B>::upload(device, &workspace);
        let output = model.forward(&batch);
        B::sync(device)?;
        black_box(read_output(output)?);
    }

    let mut pack_samples = Vec::with_capacity(iterations);
    let mut upload_samples = Vec::with_capacity(iterations);
    let mut full_forward_samples = Vec::with_capacity(iterations);
    let mut readback_samples = Vec::with_capacity(iterations);
    let mut full_lane_samples = Vec::with_capacity(iterations);
    let mut workspace_reallocations = 0usize;
    for iteration in 0..iterations {
        let full_start = Instant::now();
        let pack_start = Instant::now();
        workspace.pack(cases, decisions, iteration)?;
        pack_samples.push(pack_start.elapsed().as_secs_f64() * 1.0e6);
        if workspace.capacities() != capacities {
            workspace_reallocations += 1;
        }
        let upload_start = Instant::now();
        let batch = DevicePackedBatch::<B>::upload(device, &workspace);
        B::sync(device)?;
        upload_samples.push(upload_start.elapsed().as_secs_f64() * 1.0e6);
        let forward_start = Instant::now();
        let output = model.forward(&batch);
        B::sync(device)?;
        full_forward_samples.push(forward_start.elapsed().as_secs_f64() * 1.0e6);
        let readback_start = Instant::now();
        let output = read_output(output)?;
        readback_samples.push(readback_start.elapsed().as_secs_f64() * 1.0e6);
        if output.values.len() != decisions || output.logits.len() != batch.action_count {
            return Err("full-lane output shape mismatch".into());
        }
        full_lane_samples.push(full_start.elapsed().as_secs_f64() * 1.0e6);
    }
    if workspace_reallocations != 0 {
        return Err("reusable host workspace reallocated in timed window".into());
    }

    let forward_mean = mean_micros(&forward_samples);
    let full_lane_mean = mean_micros(&full_lane_samples);
    let forward_p50 = percentile_micros(&forward_samples, 0.50);
    let forward_p95 = percentile_micros(&forward_samples, 0.95);
    let full_lane_p50 = percentile_micros(&full_lane_samples, 0.50);
    let full_lane_p95 = percentile_micros(&full_lane_samples, 0.95);
    Ok(serde_json::json!({
        "schema": "mtg-kernel-experimental-burn-net8-packed-benchmark/v1",
        "backend_identity": EXPERIMENTAL_BACKEND_IDENTITY_V1,
        "not_a_claim_about": ["end_to_end_training", "games_per_second", "bit_identical_cuda_outputs"],
        "decisions": decisions,
        "objects": resident_batch.object_count,
        "edges": resident_batch.edge_count,
        "actions": resident_batch.action_count,
        "action_refs": resident_batch.action_ref_count,
        "warmup_iterations": warmup,
        "timed_iterations": iterations,
        "forward_resident": {
            "p50_us": forward_p50,
            "p95_us": forward_p95,
            "mean_us": forward_mean,
            "decisions_per_second": decisions as f64 * 1.0e6 / forward_mean,
            "p50_latency_decisions_per_second": decisions as f64 * 1.0e6 / forward_p50,
            "p95_latency_decisions_per_second": decisions as f64 * 1.0e6 / forward_p95,
        },
        "full_lane_reused_host_workspace": {
            "p50_us": full_lane_p50,
            "p95_us": full_lane_p95,
            "mean_us": full_lane_mean,
            "decisions_per_second": decisions as f64 * 1.0e6 / full_lane_mean,
            "p50_latency_decisions_per_second": decisions as f64 * 1.0e6 / full_lane_p50,
            "p95_latency_decisions_per_second": decisions as f64 * 1.0e6 / full_lane_p95,
            "workspace_reallocations": workspace_reallocations,
        },
        "phase_mean_us": {
            "host_pack": mean_micros(&pack_samples),
            "burn_h2d_and_sync": mean_micros(&upload_samples),
            "forward_and_sync": mean_micros(&full_forward_samples),
            "readback_validate": mean_micros(&readback_samples),
        },
        "parity": {
            "absolute_tolerance": PARITY_ABSOLUTE_TOLERANCE_V1,
            "relative_tolerance": PARITY_RELATIVE_TOLERANCE_V1,
            "maximum_absolute_error": parity.maximum_absolute_error,
            "maximum_relative_error": parity.maximum_relative_error,
            "maximum_tolerance_ratio": parity.maximum_tolerance_ratio,
            "bit_equal_outputs": parity.bit_equal_outputs,
            "output_count": parity.output_count,
            "claim": "tolerance-only-not-bit-identity",
        },
        "workspace": {
            "host_capacity_reused": true,
            "persistent_device_batch_used_for_forward_rate": true,
            "remaining_upload_limitation": "Burn Tensor::from_data clones owned host vectors and allocates fresh device tensors per changing batch",
        },
    }))
}

pub(crate) fn run_cuda_v1() -> Result<(), Box<dyn Error>> {
    let warmup = argument_usize("--warmup", DEFAULT_WARMUP_ITERATIONS)?;
    let iterations = argument_usize("--iterations", DEFAULT_TIMED_ITERATIONS)?;

    let native_model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())?;
    let mut native_state = NativePolicyValueTrainStateV1::new_v1(native_model)?;
    let (manifest_path, payload_path) = common_model_snapshot_paths_v1();
    let snapshot_record =
        load_common_model_snapshot_v1(&manifest_path, &payload_path, &mut native_state)?;
    let native_model = native_state.model_v1();
    let native_parameters = native_model.parameter_snapshot_v1();
    if native_parameters.len() != PARAMETER_TENSOR_COUNT_V1
        || native_parameters
            .iter()
            .map(|parameter| parameter.values.len())
            .sum::<usize>()
            != PARAMETER_COUNT_V1
    {
        return Err("authoritative native parameter count mismatch".into());
    }
    let native_manifest_sha256 = native_model.parameter_manifest_sha256_v1();
    if parameter_manifest_sha256(&native_parameters) != native_manifest_sha256 {
        return Err("independent native parameter digest recomputation mismatch".into());
    }

    let cases = load_real_fixture_cases()?;
    let expected = cases
        .iter()
        .map(|case| native_model.forward_v1(case.view()))
        .collect::<Result<Vec<_>, _>>()?;
    let device = burn_cuda::CudaDevice::new(0);
    let burn_model =
        ProductionNet8::<CudaBackendV1>::import_native_v1(&native_parameters, &device)?;
    if burn_model.num_params() != PARAMETER_COUNT_V1 {
        return Err(format!(
            "Burn model parameter count mismatch: {} != {PARAMETER_COUNT_V1}",
            burn_model.num_params()
        )
        .into());
    }
    let exported = burn_model.export_native_v1(&device)?;
    let round_trip_exact = parameter_snapshots_are_bit_exact(&exported, &native_parameters);
    let exported_manifest_sha256 = parameter_manifest_sha256(&exported);
    if !round_trip_exact || exported_manifest_sha256 != native_manifest_sha256 {
        return Err("Burn parameter import/export round trip is not bit exact".into());
    }

    let mut validation_workspace = HostPackingWorkspace::default();
    validation_workspace.reserve_for(&cases, cases.len());
    validation_workspace.pack(&cases, cases.len(), 0)?;
    let validation_batch =
        DevicePackedBatch::<CudaBackendV1>::upload(&device, &validation_workspace);
    let validation_output = burn_model.forward(&validation_batch);
    CudaBackendV1::sync(&device)?;
    let validation_output = read_output(validation_output)?;
    let parity = compare_to_native(&validation_workspace, &validation_output, &expected)?;

    println!(
        "{}",
        serde_json::json!({
            "schema": "mtg-kernel-experimental-burn-net8-packed-validation/v1",
            "backend_identity": EXPERIMENTAL_BACKEND_IDENTITY_V1,
            "backend": "burn-cuda-0.21.0-device-0-baseline-no-fusion-no-autotune",
            "production_model_architecture": "kernel-policy-value-net-8",
            "parameters": PARAMETER_COUNT_V1,
            "parameter_tensors": PARAMETER_TENSOR_COUNT_V1,
            "native_parameter_manifest_sha256": native_manifest_sha256,
            "burn_exported_parameter_manifest_sha256": exported_manifest_sha256,
            "parameter_import_export_bit_exact": round_trip_exact,
            "snapshot": {
                "snapshot_sha256": snapshot_record.snapshot_sha256,
                "payload_sha256": snapshot_record.payload_sha256,
                "payload_byte_count": snapshot_record.payload_byte_count,
                "named_parameter_stream_sha256": snapshot_record.named_parameter_stream_sha256,
                "loaded_named_parameter_stream_sha256": snapshot_record.loaded_named_parameter_stream_sha256,
                "load_completed_before_benchmark": snapshot_record.snapshot_load_completed_before_trial_start,
                "load_timed": snapshot_record.snapshot_load_timed,
            },
            "encoded_decisions": {
                "source_identity": FIXTURE_SOURCE_IDENTITY_V1,
                "fixture_sha256": sha256_hex(FIXTURE_BYTES),
                "real_replay_case_count": cases.len(),
                "case_names": cases.iter().map(|case| case.name.as_str()).collect::<Vec<_>>(),
                "synthetic_cases_excluded": true,
            },
            "cpu_reference_vs_cuda": {
                "reference": "NativePolicyValueNetV1::forward_v1 over the same loaded common snapshot",
                "absolute_tolerance": PARITY_ABSOLUTE_TOLERANCE_V1,
                "relative_tolerance": PARITY_RELATIVE_TOLERANCE_V1,
                "maximum_absolute_error": parity.maximum_absolute_error,
                "maximum_relative_error": parity.maximum_relative_error,
                "maximum_tolerance_ratio": parity.maximum_tolerance_ratio,
                "bit_equal_outputs": parity.bit_equal_outputs,
                "output_count": parity.output_count,
                "claim": "tolerance-only-not-bit-identity",
            },
            "backward_adam": {
                "implemented": false,
                "blockers": [
                    "define and validate gradient export in the frozen 33-tensor native order after Burn's transposed Linear storage",
                    "validate ragged scatter backward and duplicate-score recompute parity against terminal_reinforce_value/v3",
                    "import/export the two Adam moment streams and enforce one transactional optimizer step per even episode batch",
                    "replace per-batch Burn Tensor::from_data allocations with a persistent device service before end-to-end timing",
                ],
            },
        })
    );

    for decisions in BENCHMARK_BATCH_SIZES {
        println!(
            "{}",
            benchmark_one_batch(
                &device,
                &burn_model,
                &cases,
                &expected,
                decisions,
                warmup,
                iterations,
            )?
        );
    }
    Ok(())
}

pub(crate) fn run_cuda_training_v1() -> Result<(), Box<dyn Error>> {
    training::run_cuda_training_v1()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_transpose_round_trip_preserves_all_bits() {
        let native = (0..35)
            .map(|index| f32::from_bits(0x3f00_0000 + index))
            .collect::<Vec<_>>();
        let burn = transpose_output_input_to_input_output(&native, 5, 7).unwrap();
        let round_trip = transpose_input_output_to_output_input(&burn, 5, 7).unwrap();
        assert_eq!(round_trip, native);
    }

    #[test]
    fn committed_real_fixture_cases_validate_through_native_model() {
        let cases = load_real_fixture_cases().unwrap();
        assert_eq!(cases.len(), 14);
        assert!(cases
            .iter()
            .all(|case| !case.name.starts_with("synthetic-")));
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        for case in cases {
            model.forward_v1(case.view()).unwrap();
        }
    }

    #[test]
    fn reusable_host_workspace_preserves_capacity_after_reserve() {
        let cases = load_real_fixture_cases().unwrap();
        let mut workspace = HostPackingWorkspace::default();
        workspace.reserve_for(&cases, 512);
        let capacities = workspace.capacities();
        for rotation in 0..cases.len() {
            workspace.pack(&cases, 512, rotation).unwrap();
            assert_eq!(workspace.capacities(), capacities);
        }
    }
}
