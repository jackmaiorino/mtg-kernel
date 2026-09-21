//! Ordered public-stack successor with one independently aged projection.
use super::*;
use crate::native_policy_value_net_v1::stack_inputs_v1::INPUT_WIDTH;
use crate::public_stack_features_v1::{StackFeatureRowsV1, CONTRACT, FEATURE_WIDTH};

mod bridge;
pub(crate) mod snapshot;
pub(crate) use bridge::{StackTrainingGroup, StackTrainingStep};
#[cfg(test)]
mod tests;

#[derive(Module, Debug)]
struct StackMatrix<B: Backend> {
    weight: Param<Tensor<B, 2>>,
}
#[derive(Module, Debug)]
struct StackModel<B: Backend> {
    base: ProductionNet8<B>,
    stack: StackMatrix<B>,
}
impl<B: Backend> StackModel<B> {
    fn forward(
        &self,
        batch: &DevicePackedBatch<B>,
        rows: &StackBatch<B>,
    ) -> (Tensor<B, 1>, Tensor<B, 1>) {
        self.base
            .forward_core_with_stack_and_public::<FrozenNet8DimsV1>(
                batch,
                None,
                Some((rows, self.stack.weight.val())),
            )
    }
}

struct StackTensors<B: Backend> {
    features: Tensor<B, 2>,
    sources: Tensor<B, 1, Int>,
    targets: Tensor<B, 1, Int>,
    target_mask: Tensor<B, 2>,
    scatter_messages: Tensor<B, 1, Int>,
    scatter_nodes: Tensor<B, 1, Int>,
    scatter_count: usize,
}
pub(crate) struct StackBatch<B: Backend> {
    tensors: Option<StackTensors<B>>,
}
impl<B: Backend> StackBatch<B> {
    fn upload(
        rows: &[StackFeatureRowsV1],
        object_counts: &[usize],
        batch: &DevicePackedBatch<B>,
    ) -> Result<Self, Box<dyn Error>> {
        if rows.len() != batch.decision_count
            || rows.len() != object_counts.len()
            || object_counts.iter().sum::<usize>() != batch.object_count
            || batch.object_count > i32::MAX as usize
        {
            return Err(training_error("stack batch cardinality differs"));
        }
        let mut features = Vec::new();
        let mut sources = Vec::new();
        let mut targets = Vec::new();
        let mut mask = Vec::new();
        let mut message_indices = Vec::new();
        let mut nodes = Vec::new();
        let mut offset = 0;
        for (state, count) in rows.iter().zip(object_counts) {
            state.validate(*count).map_err(training_error)?;
            for row in &state.rows {
                let index = i32::try_from(sources.len())
                    .map_err(|_| training_error("too many stack rows"))?;
                let source = (offset + row.source_node) as i32;
                sources.push(source);
                targets.push((offset + row.target_node.unwrap_or(row.source_node)) as i32);
                mask.push(if row.target_node.is_some() {
                    1.0f32
                } else {
                    0.0
                });
                features.extend_from_slice(&row.features);
                message_indices.push(index);
                nodes.push(source);
                if let Some(target) = row.target_node.filter(|t| *t != row.source_node) {
                    message_indices.push(index);
                    nodes.push((offset + target) as i32);
                }
            }
            offset += count;
        }
        let count = sources.len();
        if count == 0 {
            return Ok(Self { tensors: None });
        }
        let scatter_count = nodes.len();
        Ok(Self {
            tensors: Some(StackTensors {
                features: Tensor::from_data(
                    TensorData::new(features, [count, FEATURE_WIDTH]),
                    &batch.device,
                ),
                sources: Tensor::from_data(TensorData::new(sources, [count]), &batch.device),
                targets: Tensor::from_data(TensorData::new(targets, [count]), &batch.device),
                target_mask: Tensor::from_data(TensorData::new(mask, [count, 1]), &batch.device),
                scatter_messages: Tensor::from_data(
                    TensorData::new(message_indices, [scatter_count]),
                    &batch.device,
                ),
                scatter_nodes: Tensor::from_data(
                    TensorData::new(nodes, [scatter_count]),
                    &batch.device,
                ),
                scatter_count,
            }),
        })
    }
    pub(crate) fn add_messages(
        &self,
        weight: Tensor<B, 2>,
        objects: &Tensor<B, 2>,
        pooled: Tensor<B, 2>,
    ) -> Tensor<B, 2> {
        let Some(t) = &self.tensors else {
            // Connected zero derivative, with no synthetic stack item.
            return pooled + weight.sum().mul_scalar(0.0).unsqueeze::<2>();
        };
        let input = Tensor::cat(
            vec![
                t.features.clone(),
                objects.clone().select(0, t.sources.clone()),
                objects.clone().select(0, t.targets.clone()) * t.target_mask.clone(),
            ],
            1,
        );
        let messages = input.matmul(weight.transpose()).tanh();
        // Preserve row order, source then distinct object target for each row.
        pooled.scatter(
            0,
            t.scatter_nodes
                .clone()
                .unsqueeze_dim::<2>(1)
                .expand([t.scatter_count, HIDDEN_DIM_V1]),
            messages.select(0, t.scatter_messages.clone()),
            IndexingUpdateOp::Add,
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct StackProjectionSnapshot {
    pub(crate) weight: Vec<u32>,
    pub(crate) first: Vec<u32>,
    pub(crate) second: Vec<u32>,
    pub(crate) adam_step: u64,
}
impl StackProjectionSnapshot {
    pub(crate) fn zero() -> Self {
        Self {
            weight: vec![0; HIDDEN_DIM_V1 * INPUT_WIDTH],
            first: vec![0; HIDDEN_DIM_V1 * INPUT_WIDTH],
            second: vec![0; HIDDEN_DIM_V1 * INPUT_WIDTH],
            adam_step: 0,
        }
    }
    fn validate(&self) -> Result<(), Box<dyn Error>> {
        for (values, nonnegative) in [
            (&self.weight, false),
            (&self.first, false),
            (&self.second, true),
        ] {
            if values.len() != HIDDEN_DIM_V1 * INPUT_WIDTH
                || values
                    .iter()
                    .copied()
                    .map(f32::from_bits)
                    .any(|v| !v.is_finite() || (nonnegative && v < 0.0))
            {
                return Err(training_error("invalid stack parameter or Adam tensor"));
            }
        }
        if self.adam_step > i32::MAX as u64 {
            return Err(training_error("stack Adam age out of range"));
        }
        Ok(())
    }
}
#[derive(Default)]
pub(crate) struct StackGradientAccumulator {
    base: burn::optim::GradientsAccumulator<ProductionNet8<CudaAutodiffBackendV1>>,
    stack: Option<Tensor<CudaBackendV1, 2>>,
}
pub(crate) struct StackDeviceTrainState {
    legacy: ExperimentalDeviceTrainStateV1,
    stack: StackMatrix<CudaAutodiffBackendV1>,
    first: Tensor<CudaBackendV1, 2>,
    second: Tensor<CudaBackendV1, 2>,
    stack_step: u64,
}
fn data(bits: &[u32]) -> TensorData {
    TensorData::new(
        bits.iter().copied().map(f32::from_bits).collect::<Vec<_>>(),
        [HIDDEN_DIM_V1, INPUT_WIDTH],
    )
}
impl StackDeviceTrainState {
    pub(crate) fn import(
        legacy: &NativePolicyValueTrainSnapshotV1,
        stack: &StackProjectionSnapshot,
        device: &burn_cuda::CudaDevice,
    ) -> Result<Self, Box<dyn Error>> {
        stack.validate()?;
        Ok(Self {
            legacy: ExperimentalDeviceTrainStateV1::import_snapshot_v1(legacy, device)?,
            stack: StackMatrix {
                weight: Param::from_data(data(&stack.weight), device),
            },
            first: Tensor::from_data(data(&stack.first), device),
            second: Tensor::from_data(data(&stack.second), device),
            stack_step: stack.adam_step,
        })
    }
    pub(crate) fn snapshot(
        &self,
    ) -> Result<(NativePolicyValueTrainSnapshotV1, StackProjectionSnapshot), Box<dyn Error>> {
        let bits = |t: Tensor<CudaBackendV1, 2>| -> Result<Vec<u32>, Box<dyn Error>> {
            Ok(t.into_data()
                .to_vec::<f32>()?
                .into_iter()
                .map(f32::to_bits)
                .collect())
        };
        let stack = StackProjectionSnapshot {
            weight: bits(self.stack.weight.val().inner())?,
            first: bits(self.first.clone())?,
            second: bits(self.second.clone())?,
            adam_step: self.stack_step,
        };
        stack.validate()?;
        Ok((self.legacy.export_snapshot_v1()?, stack))
    }
    fn chunk_backward_gae(
        &self,
        accumulator: &mut StackGradientAccumulator,
        batch: &DevicePackedBatch<CudaAutodiffBackendV1>,
        rows: &[StackFeatureRowsV1],
        object_counts: &[usize],
        plan: &DenseGroupLossPlanGaeV1,
        value_coefficient: f32,
        normalization_group_count: f32,
        entropy_coefficient: f32,
    ) -> Result<ChunkBackwardOutputsV1, Box<dyn Error>> {
        let auxiliary = StackBatch::upload(rows, object_counts, batch)?;
        let model = StackModel {
            base: self.legacy.model.clone(),
            stack: self.stack.clone(),
        };
        let (logits, values) = model.forward(batch, &auxiliary);
        let logit_outputs = logits.clone().inner();
        let value_outputs = values.clone().inner();
        let loss = entropy::dense_group_loss_with_entropy(
            logits,
            values,
            plan,
            value_coefficient,
            normalization_group_count,
            entropy_coefficient,
        )?;
        let mut gradients = GradientsParams::from_grads(loss.backward(), &model);
        if batch.empty_relations_v3 {
            register_empty_relation_gradients_v3(&model.base, batch, &mut gradients)?;
        }
        let id = model.base.card_embedding.weight.id;
        let embedding = gradients
            .remove::<CudaBackendV1, 2>(id)
            .ok_or_else(|| training_error("missing embedding gradient"))?;
        gradients.register(
            id,
            embedding.slice_assign(
                [0..1, 0..CARD_EMBEDDING_DIM_V1],
                Tensor::zeros([1, CARD_EMBEDDING_DIM_V1], &self.legacy.device),
            ),
        );
        if gradients.len() != PARAMETER_TENSOR_COUNT_V1 + 1 {
            return Err(training_error("stack gradient count differs"));
        }
        let stack = gradients
            .remove::<CudaBackendV1, 2>(model.stack.weight.id)
            .ok_or_else(|| training_error("missing stack gradient"))?;
        let gauge = model
            .base
            .scorer
            .output
            .bias
            .as_ref()
            .ok_or_else(|| training_error("missing gauge"))?;
        let gauge_gradient = gradients
            .get::<CudaBackendV1, 1>(gauge.id)
            .ok_or_else(|| training_error("missing gauge gradient"))?;
        let readback = Transaction::<CudaBackendV1>::default()
            .register(logit_outputs)
            .register(value_outputs)
            .register(gauge_gradient)
            .try_execute()?;
        let [logits, values, gauge]: [TensorData; 3] = readback
            .try_into()
            .map_err(|_| training_error("stack backward readback count"))?;
        accumulator.stack = Some(match accumulator.stack.take() {
            Some(old) => old + stack,
            None => stack,
        });
        accumulator.base.accumulate(&self.legacy.model, gradients);
        Ok(ChunkBackwardOutputsV1 {
            logit_outputs: logits.into_vec::<f32>()?,
            value_outputs: values.into_vec::<f32>()?,
            raw_gauge_residual: gauge.into_vec::<f32>()?[0],
            #[cfg(test)]
            device_objective: None,
        })
    }
    pub(crate) fn apply(
        &mut self,
        mut accumulator: StackGradientAccumulator,
        learning_rate: f32,
    ) -> Result<(), Box<dyn Error>> {
        if !learning_rate.is_finite() || learning_rate <= 0.0 {
            return Err(training_error("invalid stack learning rate"));
        }
        let stack_step = self
            .stack_step
            .checked_add(1)
            .ok_or_else(|| training_error("stack age overflow"))?;
        let next_step = self
            .legacy
            .adam_step
            .checked_add(1)
            .ok_or_else(|| training_error("legacy age overflow"))?;
        let stack_gradient = accumulator
            .stack
            .take()
            .ok_or_else(|| training_error("missing accumulated stack gradient"))?;
        let mut gradients = accumulator.base.grads();
        let gauge = self
            .legacy
            .model
            .scorer
            .output
            .bias
            .as_ref()
            .ok_or_else(|| training_error("missing gauge"))?;
        let gauge_gradient = gradients
            .remove::<CudaBackendV1, 1>(gauge.id)
            .ok_or_else(|| training_error("missing gauge gradient"))?;
        gradients.register(gauge.id, gauge_gradient.zeros_like());
        let mut mapper = DeviceAdamMapperV1::new(
            gradients,
            &self.legacy.first_moments,
            &self.legacy.second_moments,
            next_step,
            learning_rate,
        )?;
        let base = self.legacy.model.clone().map(&mut mapper);
        let (first, second) = mapper.finish_v1()?;
        let scalar = DeviceAdamMapperV1::new(
            GradientsParams::new(),
            &self.legacy.first_moments,
            &self.legacy.second_moments,
            stack_step,
            learning_rate,
        )?;
        let next_m = self.first.clone()
            + (stack_gradient.clone() - self.first.clone()).mul_scalar(1.0 - ADAM_BETA1_V1);
        let next_v = self.second.clone().mul_scalar(ADAM_BETA2_V1)
            + stack_gradient.square().mul_scalar(1.0 - ADAM_BETA2_V1);
        let denominator = next_v
            .clone()
            .sqrt()
            .div_scalar(scalar.bias_correction2_sqrt)
            .add_scalar(ADAM_EPSILON_V1);
        let (id, tensor, mapper) = self.stack.weight.clone().consume();
        let updated = tensor.inner()
            + next_m
                .clone()
                .mul_scalar(-scalar.step_size)
                .div(denominator);
        let weight = Param::from_mapped_value(
            id,
            Tensor::<CudaAutodiffBackendV1, 2>::from_inner(updated).require_grad(),
            mapper,
        );
        CudaBackendV1::sync(&self.legacy.device)?;
        self.legacy.model = base;
        self.legacy.first_moments = first;
        self.legacy.second_moments = second;
        self.legacy.adam_step = next_step;
        self.stack = StackMatrix { weight };
        self.first = next_m;
        self.second = next_v;
        self.stack_step = stack_step;
        Ok(())
    }
}
