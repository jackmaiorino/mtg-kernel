//! Own-registration successor. The inherited 33 tensors and Adam age stay
//! unchanged on import; the single new 64 by 192 projection has its own age.
use super::*;
use crate::native_policy_value_net_v1::own_registration::RegistrationInputWeightsV1;
use crate::own_registration_v1::{
    contract_identity_v1, fixed_mean_features_v1, RegistrationContractV1,
    RegistrationFeatureRowsV1, RegistrationModeV1, WIDTH,
};

mod bridge;
pub(crate) mod snapshot;
pub(crate) use bridge::{RegistrationTrainingGroup, RegistrationTrainingStep};
#[cfg(test)]
mod tests;

#[derive(Module, Debug)]
struct RegistrationMatrix<B: Backend> {
    weight: Param<Tensor<B, 2>>,
}

#[derive(Module, Debug)]
struct RegistrationModel<B: Backend> {
    base: ProductionNet8<B>,
    projection: RegistrationMatrix<B>,
    enabled: bool,
}

impl<B: Backend> RegistrationModel<B> {
    fn forward(
        &self,
        batch: &DevicePackedBatch<B>,
        rows: &RegistrationBatch<B>,
    ) -> (Tensor<B, 1>, Tensor<B, 1>) {
        if self.enabled {
            // Do not skip this matmul at zero W: its derivative learns W.
            self.base
                .forward_core_with_registration::<FrozenNet8DimsV1>(
                    batch,
                    rows.features
                        .clone()
                        .matmul(self.projection.weight.val().transpose()),
                )
        } else {
            self.base.forward_core::<FrozenNet8DimsV1>(batch)
        }
    }
}

struct RegistrationBatch<B: Backend> {
    features: Tensor<B, 2>,
}
impl<B: Backend> RegistrationBatch<B> {
    fn upload(
        rows: &[RegistrationFeatureRowsV1],
        batch: &DevicePackedBatch<B>,
        mode: RegistrationModeV1,
    ) -> Result<Self, Box<dyn Error>> {
        if rows.len() != batch.decision_count {
            return Err(training_error("registration batch cardinality differs"));
        }
        for row in rows {
            row.validate().map_err(training_error)?;
            if row.mode != mode {
                return Err(training_error("registration batch mode differs"));
            }
        }
        Ok(Self {
            features: Tensor::from_data(
                TensorData::new(
                    rows.iter()
                        .flat_map(|r| r.features.iter())
                        .copied()
                        .collect::<Vec<_>>(),
                    [batch.decision_count, WIDTH],
                ),
                &batch.device,
            ),
        })
    }
}

/// Pinned mean bits are included even for Own, so an R/M pair has one roster.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectionSnapshot {
    pub(crate) feature_identity: RegistrationContractV1,
    pub(crate) registry_sha256: String,
    pub(crate) contract_sha256: String,
    pub(crate) roster_sha256: String,
    pub(crate) mean_sha256: String,
    pub(crate) mean_bits: Vec<u32>,
    pub(crate) mode: RegistrationModeV1,
    pub(crate) projection_enabled: bool,
    pub(crate) weights: Vec<u32>,
    pub(crate) first: Vec<u32>,
    pub(crate) second: Vec<u32>,
    pub(crate) adam_step: u64,
}

impl ProjectionSnapshot {
    pub(crate) fn zero(mode: RegistrationModeV1) -> Result<Self, Box<dyn Error>> {
        let identity = contract_identity_v1().map_err(training_error)?;
        let mean_bits: Vec<_> = fixed_mean_features_v1()
            .map_err(training_error)?
            .into_iter()
            .map(f32::to_bits)
            .collect();
        let mean_sha256 = identity.mean_sha256.clone();
        Ok(Self {
            feature_identity: identity.clone(),
            registry_sha256: identity.registry_sha256,
            contract_sha256: identity.contract_sha256,
            roster_sha256: identity.roster_sha256,
            mean_sha256,
            mean_bits,
            mode,
            projection_enabled: true,
            weights: vec![0; HIDDEN_DIM_V1 * WIDTH],
            first: vec![0; HIDDEN_DIM_V1 * WIDTH],
            second: vec![0; HIDDEN_DIM_V1 * WIDTH],
            adam_step: 0,
        })
    }
    pub(crate) fn baseline() -> Result<Self, Box<dyn Error>> {
        let mut result = Self::zero(RegistrationModeV1::Own)?;
        result.projection_enabled = false;
        Ok(result)
    }
    pub(crate) fn input_weights(&self) -> Result<RegistrationInputWeightsV1, Box<dyn Error>> {
        self.validate()?;
        RegistrationInputWeightsV1::new(self.weights.iter().copied().map(f32::from_bits).collect())
            .map_err(|e| training_error(e.to_string()))
    }
    pub(crate) fn validate(&self) -> Result<(), Box<dyn Error>> {
        let pinned = Self::zero(self.mode)?;
        if self.feature_identity != pinned.feature_identity
            || self.registry_sha256 != pinned.registry_sha256
            || self.contract_sha256 != pinned.contract_sha256
            || self.roster_sha256 != pinned.roster_sha256
            || self.mean_sha256 != pinned.mean_sha256
            || self.mean_bits != pinned.mean_bits
        {
            return Err(training_error("registration projection identity differs"));
        }
        for (values, nonnegative) in [
            (&self.weights, false),
            (&self.first, false),
            (&self.second, true),
        ] {
            if values.len() != HIDDEN_DIM_V1 * WIDTH
                || values
                    .iter()
                    .copied()
                    .map(f32::from_bits)
                    .any(|v| !v.is_finite() || (nonnegative && v < 0.0))
            {
                return Err(training_error(
                    "invalid registration parameter or Adam tensor",
                ));
            }
        }
        if self.adam_step > i32::MAX as u64 {
            return Err(training_error(
                "registration Adam age outside scalar implementation",
            ));
        }
        if !self.projection_enabled
            && (self.mode != RegistrationModeV1::Own
                || self.adam_step != 0
                || self
                    .weights
                    .iter()
                    .chain(&self.first)
                    .chain(&self.second)
                    .any(|v| *v != 0))
        {
            return Err(training_error(
                "baseline projection must remain frozen at positive zero",
            ));
        }
        Ok(())
    }
}

type ChunkBackwardWithExtraV1 = Result<(ChunkBackwardOutputsV1, Option<Vec<f32>>), Box<dyn Error>>;
type ChunkLossV1 = Result<
    (
        Tensor<CudaAutodiffBackendV1, 1>,
        Option<Tensor<CudaBackendV1, 1>>,
    ),
    Box<dyn Error>,
>;

#[derive(Default)]
pub(crate) struct RegistrationGradientAccumulator {
    base: burn::optim::GradientsAccumulator<ProductionNet8<CudaAutodiffBackendV1>>,
    projection: Option<Tensor<CudaBackendV1, 2>>,
}

pub(crate) struct RegistrationDeviceTrainState {
    legacy: ExperimentalDeviceTrainStateV1,
    projection: RegistrationMatrix<CudaAutodiffBackendV1>,
    first: Tensor<CudaBackendV1, 2>,
    second: Tensor<CudaBackendV1, 2>,
    projection_step: u64,
    mode: RegistrationModeV1,
    enabled: bool,
}
fn data(bits: &[u32]) -> TensorData {
    TensorData::new(
        bits.iter().copied().map(f32::from_bits).collect::<Vec<_>>(),
        [HIDDEN_DIM_V1, WIDTH],
    )
}
impl RegistrationDeviceTrainState {
    pub(crate) fn import(
        legacy: &NativePolicyValueTrainSnapshotV1,
        projection: &ProjectionSnapshot,
        device: &burn_cuda::CudaDevice,
    ) -> Result<Self, Box<dyn Error>> {
        projection.validate()?;
        Ok(Self {
            legacy: ExperimentalDeviceTrainStateV1::import_snapshot_v1(legacy, device)?,
            projection: RegistrationMatrix {
                weight: Param::from_data(data(&projection.weights), device),
            },
            first: Tensor::from_data(data(&projection.first), device),
            second: Tensor::from_data(data(&projection.second), device),
            projection_step: projection.adam_step,
            mode: projection.mode,
            enabled: projection.projection_enabled,
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
        let mut projection = ProjectionSnapshot::zero(self.mode)?;
        projection.projection_enabled = self.enabled;
        projection.weights = bits(self.projection.weight.val().inner())?;
        projection.first = bits(self.first.clone())?;
        projection.second = bits(self.second.clone())?;
        projection.adam_step = self.projection_step;
        projection.validate()?;
        Ok((self.legacy.export_snapshot_v1()?, projection))
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "explicit ordinary GAE input contract"
    )]
    fn chunk_backward_gae(
        &self,
        accumulator: &mut RegistrationGradientAccumulator,
        batch: &DevicePackedBatch<CudaAutodiffBackendV1>,
        rows: &[RegistrationFeatureRowsV1],
        plan: &DenseGroupLossPlanGaeV1,
        value_coefficient: f32,
        normalization_group_count: f32,
        entropy_coefficient: f32,
    ) -> Result<ChunkBackwardOutputsV1, Box<dyn Error>> {
        self.chunk_backward_with(accumulator, batch, rows, |logits, values| {
            let loss = entropy::dense_group_loss_with_entropy(
                logits,
                values,
                plan,
                value_coefficient,
                normalization_group_count,
                entropy_coefficient,
            )?;
            Ok((loss, None))
        })
        .map(|(output, _)| output)
    }
    fn chunk_backward_with(
        &self,
        accumulator: &mut RegistrationGradientAccumulator,
        batch: &DevicePackedBatch<CudaAutodiffBackendV1>,
        rows: &[RegistrationFeatureRowsV1],
        loss_builder: impl FnOnce(
            Tensor<CudaAutodiffBackendV1, 1>,
            Tensor<CudaAutodiffBackendV1, 1>,
        ) -> ChunkLossV1,
    ) -> ChunkBackwardWithExtraV1 {
        let auxiliary = RegistrationBatch::upload(rows, batch, self.mode)?;
        let model = RegistrationModel {
            base: self.legacy.model.clone(),
            projection: self.projection.clone(),
            enabled: self.enabled,
        };
        let (logits, values) = model.forward(batch, &auxiliary);
        let logit_outputs = logits.clone().inner();
        let value_outputs = values.clone().inner();
        let (loss, extra) = loss_builder(logits, values)?;
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
        if gradients.len() != PARAMETER_TENSOR_COUNT_V1 + usize::from(self.enabled) {
            return Err(training_error("registration gradient count differs"));
        }
        let projection = if self.enabled {
            Some(
                gradients
                    .remove::<CudaBackendV1, 2>(model.projection.weight.id)
                    .ok_or_else(|| training_error("missing registration gradient"))?,
            )
        } else {
            None
        };
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
        let mut transaction = Transaction::<CudaBackendV1>::default()
            .register(logit_outputs)
            .register(value_outputs)
            .register(gauge_gradient);
        let has_extra = extra.is_some();
        if let Some(extra) = extra {
            transaction = transaction.register(extra);
        }
        let mut readback = transaction.try_execute()?;
        let extra = if has_extra {
            Some(
                readback
                    .pop()
                    .ok_or_else(|| training_error("registration backward readback count"))?
                    .into_vec::<f32>()?,
            )
        } else {
            None
        };
        let [logits, values, gauge]: [TensorData; 3] = readback
            .try_into()
            .map_err(|_| training_error("registration backward readback count"))?;
        if let Some(projection) = projection {
            accumulator.projection = Some(match accumulator.projection.take() {
                Some(previous) => previous + projection,
                None => projection,
            });
        }
        accumulator.base.accumulate(&self.legacy.model, gradients);
        Ok((
            ChunkBackwardOutputsV1 {
                logit_outputs: logits.into_vec::<f32>()?,
                value_outputs: values.into_vec::<f32>()?,
                raw_gauge_residual: gauge.into_vec::<f32>()?[0],
                #[cfg(test)]
                device_objective: None,
            },
            extra,
        ))
    }

    pub(crate) fn apply(
        &mut self,
        mut accumulator: RegistrationGradientAccumulator,
        learning_rate: f32,
    ) -> Result<(), Box<dyn Error>> {
        if !learning_rate.is_finite() || learning_rate <= 0.0 {
            return Err(training_error("invalid registration learning rate"));
        }
        let projection_step = if self.enabled {
            self.projection_step
                .checked_add(1)
                .ok_or_else(|| training_error("registration age overflow"))?
        } else {
            0
        };
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
        let next_projection = if self.enabled {
            let scalar = DeviceAdamMapperV1::new(
                GradientsParams::new(),
                &self.legacy.first_moments,
                &self.legacy.second_moments,
                projection_step,
                learning_rate,
            )?;
            let step = |parameter: Param<Tensor<CudaAutodiffBackendV1, 2>>,
                        gradient: Option<Tensor<CudaBackendV1, 2>>,
                        m: Tensor<CudaBackendV1, 2>,
                        v: Tensor<CudaBackendV1, 2>|
             -> Result<_, Box<dyn Error>> {
                let gradient = gradient
                    .ok_or_else(|| training_error("missing accumulated registration gradient"))?;
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
            Some(step(
                self.projection.weight.clone(),
                accumulator.projection,
                self.first.clone(),
                self.second.clone(),
            )?)
        } else {
            None
        };
        CudaBackendV1::sync(&self.legacy.device)?;
        // Publish inherited and projection state together after successful device work.
        self.legacy.model = base;
        self.legacy.first_moments = first;
        self.legacy.second_moments = second;
        self.legacy.adam_step = next_step;
        if let Some((weight, first, second)) = next_projection {
            self.projection = RegistrationMatrix { weight };
            self.first = first;
            self.second = second;
        }
        self.projection_step = projection_step;
        Ok(())
    }
}
