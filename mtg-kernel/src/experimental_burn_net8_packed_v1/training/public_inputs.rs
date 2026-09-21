//! Explicit successor state with legacy Adam age plus a separate public age.
//! Not admitted by the existing 33-tensor training/checkpoint routes.
use super::*;
use crate::public_cost_features_v1::{
    catalog_v1, PublicFeatureRowsV1, OBJECT_WIDTH, SCHEMA, STATE_WIDTH,
};

mod bridge;
pub(crate) mod snapshot;
pub(crate) use bridge::{PublicTrainingGroup, PublicTrainingStep};
#[cfg(test)]
mod tests;

#[derive(Module, Debug)]
struct PublicMatrices<B: Backend> {
    object: Param<Tensor<B, 2>>,
    state: Param<Tensor<B, 2>>,
}

#[derive(Module, Debug)]
struct PublicModel<B: Backend> {
    base: ProductionNet8<B>,
    public: PublicMatrices<B>,
}

impl<B: Backend> PublicModel<B> {
    fn forward(
        &self,
        batch: &DevicePackedBatch<B>,
        auxiliary: &PublicBatch<B>,
    ) -> (Tensor<B, 1>, Tensor<B, 1>) {
        self.base.forward_core_with_public::<FrozenNet8DimsV1>(
            batch,
            Some((
                auxiliary
                    .object
                    .clone()
                    .matmul(self.public.object.val().transpose()),
                auxiliary
                    .state
                    .clone()
                    .matmul(self.public.state.val().transpose()),
            )),
        )
    }
}

struct PublicBatch<B: Backend> {
    object: Tensor<B, 2>,
    state: Tensor<B, 2>,
}

impl<B: Backend> PublicBatch<B> {
    fn upload(
        rows: &[PublicFeatureRowsV1],
        batch: &DevicePackedBatch<B>,
    ) -> Result<Self, Box<dyn Error>> {
        let catalog = catalog_v1().map_err(training_error)?;
        if rows.len() != batch.decision_count
            || rows.iter().map(|r| r.objects.len()).sum::<usize>() != batch.object_count
        {
            return Err(training_error("public batch cardinality differs"));
        }
        for row in rows {
            if row.schema != SCHEMA
                || row.registry_sha256 != catalog.registry_sha256
                || row.contract_sha256 != catalog.contract_sha256
                || row.state.len() != STATE_WIDTH
                || row.objects.iter().any(|r| r.len() != OBJECT_WIDTH)
                || row
                    .state
                    .iter()
                    .chain(row.objects.iter().flatten())
                    .any(|v| !v.is_finite())
            {
                return Err(training_error("public batch contract differs"));
            }
        }
        Ok(Self {
            object: Tensor::from_data(
                TensorData::new(
                    rows.iter()
                        .flat_map(|r| r.objects.iter().flatten())
                        .copied()
                        .collect::<Vec<_>>(),
                    [batch.object_count, OBJECT_WIDTH],
                ),
                &batch.device,
            ),
            state: Tensor::from_data(
                TensorData::new(
                    rows.iter()
                        .flat_map(|r| r.state.iter())
                        .copied()
                        .collect::<Vec<_>>(),
                    [batch.decision_count, STATE_WIDTH],
                ),
                &batch.device,
            ),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectionSnapshot {
    pub(crate) object: Vec<u32>,
    pub(crate) object_first: Vec<u32>,
    pub(crate) object_second: Vec<u32>,
    pub(crate) state: Vec<u32>,
    pub(crate) state_first: Vec<u32>,
    pub(crate) state_second: Vec<u32>,
    pub(crate) adam_step: u64,
}

impl ProjectionSnapshot {
    pub(crate) fn zero() -> Self {
        Self {
            object: vec![0; HIDDEN_DIM_V1 * OBJECT_WIDTH],
            object_first: vec![0; HIDDEN_DIM_V1 * OBJECT_WIDTH],
            object_second: vec![0; HIDDEN_DIM_V1 * OBJECT_WIDTH],
            state: vec![0; HIDDEN_DIM_V1 * STATE_WIDTH],
            state_first: vec![0; HIDDEN_DIM_V1 * STATE_WIDTH],
            state_second: vec![0; HIDDEN_DIM_V1 * STATE_WIDTH],
            adam_step: 0,
        }
    }
    fn validate(&self) -> Result<(), Box<dyn Error>> {
        for (values, width, nonnegative) in [
            (&self.object, OBJECT_WIDTH, false),
            (&self.object_first, OBJECT_WIDTH, false),
            (&self.object_second, OBJECT_WIDTH, true),
            (&self.state, STATE_WIDTH, false),
            (&self.state_first, STATE_WIDTH, false),
            (&self.state_second, STATE_WIDTH, true),
        ] {
            if values.len() != HIDDEN_DIM_V1 * width
                || values
                    .iter()
                    .copied()
                    .map(f32::from_bits)
                    .any(|v| !v.is_finite() || (nonnegative && v < 0.0))
            {
                return Err(training_error("invalid public parameter or Adam tensor"));
            }
        }
        if self.adam_step > i32::MAX as u64 {
            return Err(training_error(
                "public Adam age outside scalar implementation",
            ));
        }
        Ok(())
    }
}

pub(crate) struct PublicGradientAccumulator {
    base: burn::optim::GradientsAccumulator<ProductionNet8<CudaAutodiffBackendV1>>,
    object: Option<Tensor<CudaBackendV1, 2>>,
    state: Option<Tensor<CudaBackendV1, 2>>,
}

impl Default for PublicGradientAccumulator {
    fn default() -> Self {
        Self {
            base: Default::default(),
            object: None,
            state: None,
        }
    }
}

pub(crate) struct PublicDeviceTrainState {
    legacy: ExperimentalDeviceTrainStateV1,
    public: PublicMatrices<CudaAutodiffBackendV1>,
    object_first: Tensor<CudaBackendV1, 2>,
    object_second: Tensor<CudaBackendV1, 2>,
    state_first: Tensor<CudaBackendV1, 2>,
    state_second: Tensor<CudaBackendV1, 2>,
    public_step: u64,
}

fn data(bits: &[u32], width: usize) -> TensorData {
    TensorData::new(
        bits.iter().copied().map(f32::from_bits).collect::<Vec<_>>(),
        [HIDDEN_DIM_V1, width],
    )
}

impl PublicDeviceTrainState {
    pub(crate) fn import(
        legacy: &NativePolicyValueTrainSnapshotV1,
        public: &ProjectionSnapshot,
        device: &burn_cuda::CudaDevice,
    ) -> Result<Self, Box<dyn Error>> {
        public.validate()?;
        let legacy = ExperimentalDeviceTrainStateV1::import_snapshot_v1(legacy, device)?;
        Ok(Self {
            legacy,
            public: PublicMatrices {
                object: Param::from_data(data(&public.object, OBJECT_WIDTH), device),
                state: Param::from_data(data(&public.state, STATE_WIDTH), device),
            },
            object_first: Tensor::from_data(data(&public.object_first, OBJECT_WIDTH), device),
            object_second: Tensor::from_data(data(&public.object_second, OBJECT_WIDTH), device),
            state_first: Tensor::from_data(data(&public.state_first, STATE_WIDTH), device),
            state_second: Tensor::from_data(data(&public.state_second, STATE_WIDTH), device),
            public_step: public.adam_step,
        })
    }

    pub(crate) fn snapshot(
        &self,
    ) -> Result<(NativePolicyValueTrainSnapshotV1, ProjectionSnapshot), Box<dyn Error>> {
        let bits = |tensor: Tensor<CudaBackendV1, 2>| -> Result<Vec<u32>, Box<dyn Error>> {
            Ok(tensor
                .into_data()
                .to_vec::<f32>()?
                .into_iter()
                .map(f32::to_bits)
                .collect())
        };
        let legacy = self.legacy.export_snapshot_v1()?;
        let public = ProjectionSnapshot {
            object: bits(self.public.object.val().inner())?,
            state: bits(self.public.state.val().inner())?,
            object_first: bits(self.object_first.clone())?,
            object_second: bits(self.object_second.clone())?,
            state_first: bits(self.state_first.clone())?,
            state_second: bits(self.state_second.clone())?,
            adam_step: self.public_step,
        };
        public.validate()?;
        Ok((legacy, public))
    }

    fn chunk_backward_gae(
        &self,
        accumulator: &mut PublicGradientAccumulator,
        batch: &DevicePackedBatch<CudaAutodiffBackendV1>,
        rows: &[PublicFeatureRowsV1],
        plan: &DenseGroupLossPlanGaeV1,
        value_coefficient: f32,
        normalization_group_count: f32,
        entropy_coefficient: f32,
    ) -> Result<ChunkBackwardOutputsV1, Box<dyn Error>> {
        let auxiliary = PublicBatch::upload(rows, batch)?;
        let model = PublicModel {
            base: self.legacy.model.clone(),
            public: self.public.clone(),
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
        // Python Embedding(padding_idx=0) never trains the dummy-card row.
        // Burn's embedding operation has no padding field, so enforce the same
        // derivative here before accumulating any chunks or Adam moments.
        let embedding_id = model.base.card_embedding.weight.id;
        let embedding_gradient = gradients
            .remove::<CudaBackendV1, 2>(embedding_id)
            .ok_or_else(|| training_error("missing embedding gradient"))?;
        gradients.register(
            embedding_id,
            embedding_gradient.slice_assign(
                [0..1, 0..CARD_EMBEDDING_DIM_V1],
                Tensor::zeros([1, CARD_EMBEDDING_DIM_V1], &self.legacy.device),
            ),
        );
        if gradients.len() != PARAMETER_TENSOR_COUNT_V1 + 2 {
            return Err(training_error("public gradient count differs"));
        }
        let object = gradients
            .remove::<CudaBackendV1, 2>(model.public.object.id)
            .ok_or_else(|| training_error("missing object public gradient"))?;
        let state = gradients
            .remove::<CudaBackendV1, 2>(model.public.state.id)
            .ok_or_else(|| training_error("missing state public gradient"))?;
        let gauge = model
            .base
            .scorer
            .output
            .bias
            .as_ref()
            .ok_or_else(|| training_error("missing gauge parameter"))?;
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
            .map_err(|_| training_error("public backward readback count"))?;
        accumulator.object = Some(match accumulator.object.take() {
            Some(previous) => previous + object,
            None => object,
        });
        accumulator.state = Some(match accumulator.state.take() {
            Some(previous) => previous + state,
            None => state,
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
        mut accumulator: PublicGradientAccumulator,
        learning_rate: f32,
    ) -> Result<(), Box<dyn Error>> {
        if !learning_rate.is_finite() || learning_rate <= 0.0 {
            return Err(training_error("invalid public learning rate"));
        }
        let public_step = self
            .public_step
            .checked_add(1)
            .ok_or_else(|| training_error("public age overflow"))?;
        let next_step = self
            .legacy
            .adam_step
            .checked_add(1)
            .ok_or_else(|| training_error("legacy age overflow"))?;
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
            public_step,
            learning_rate,
        )?;
        let step = |parameter: Param<Tensor<CudaAutodiffBackendV1, 2>>,
                    gradient: Option<Tensor<CudaBackendV1, 2>>,
                    m: Tensor<CudaBackendV1, 2>,
                    v: Tensor<CudaBackendV1, 2>|
         -> Result<_, Box<dyn Error>> {
            let gradient =
                gradient.ok_or_else(|| training_error("missing accumulated public gradient"))?;
            let next_m = m.clone() + (gradient.clone() - m).mul_scalar(1.0 - ADAM_BETA1_V1);
            let next_v =
                v.mul_scalar(ADAM_BETA2_V1) + gradient.square().mul_scalar(1.0 - ADAM_BETA2_V1);
            let denominator = next_v
                .clone()
                .sqrt()
                .div_scalar(scalar.bias_correction2_sqrt)
                .add_scalar(ADAM_EPSILON_V1);
            let (id, tensor, mapper) = parameter.consume();
            let updated = tensor.inner()
                + next_m
                    .clone()
                    .mul_scalar(-scalar.step_size)
                    .div(denominator);
            let parameter = Param::from_mapped_value(
                id,
                Tensor::<CudaAutodiffBackendV1, 2>::from_inner(updated).require_grad(),
                mapper,
            );
            Ok((parameter, next_m, next_v))
        };
        let (object, object_first, object_second) = step(
            self.public.object.clone(),
            accumulator.object,
            self.object_first.clone(),
            self.object_second.clone(),
        )?;
        let (state, state_first, state_second) = step(
            self.public.state.clone(),
            accumulator.state,
            self.state_first.clone(),
            self.state_second.clone(),
        )?;
        CudaBackendV1::sync(&self.legacy.device)?;
        // Publish all 35 tensors and both ages together after successful device work.
        self.legacy.model = base;
        self.legacy.first_moments = first;
        self.legacy.second_moments = second;
        self.legacy.adam_step = next_step;
        self.public = PublicMatrices { object, state };
        self.object_first = object_first;
        self.object_second = object_second;
        self.state_first = state_first;
        self.state_second = state_second;
        self.public_step = public_step;
        Ok(())
    }
}
