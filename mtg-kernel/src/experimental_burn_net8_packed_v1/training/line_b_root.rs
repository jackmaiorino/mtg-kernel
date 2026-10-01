//! G115 line (b) auxiliary term on the CUDA device path (unit 8d). The
//! selected roots' rows form their own small batch: its forward feeds the
//! device loss `(c/N) * sum over roots`, reverse `sum p (log p - log q)` or
//! forward `-sum q log p` (the same gradient as `KL(q || p)`; the constant
//! `sum q log q` is left out), and its backward joins the update's
//! accumulator before the single Adam step. The readback returns the device
//! root-row logits (the envelope receipt), the term's scorer-bias gradient
//! (the gauge) and its head-tensor gradients (telemetry). The ordinary
//! chunks keep the production method; a treatment update reads their head
//! gradients through the sibling below, whose arithmetic is identical.
use super::*;
use crate::line_b_teacher_target_v1::LineBDivergenceV1;

type HeadGradientTensorsV1 = (
    [Option<Tensor<CudaBackendV1, 2>>; 4],
    [Option<Tensor<CudaBackendV1, 1>>; 3],
);
type ChunkHeadReadbackV1 = (ChunkBackwardOutputsV1, Vec<Vec<f32>>);

/// The root rows padded to the widest menu, with the frozen targets as
/// constants (reverse: `log q`; forward: `q`; zero at pads).
pub(crate) struct LineBRootLossPlanV1 {
    pad_gather: Tensor<CudaAutodiffBackendV1, 1, Int>,
    pad_mask: Tensor<CudaAutodiffBackendV1, 2>,
    target: Tensor<CudaAutodiffBackendV1, 2>,
    direction: LineBDivergenceV1,
    scale: f32,
    rows: usize,
    max_actions: usize,
}

/// `targets[row] = (log q, q)` in legal-action order; `scale = c / N`.
pub(crate) fn build_line_b_root_loss_plan_v1(
    host: &HostPackingWorkspace,
    targets: &[(Vec<f64>, Vec<f64>)],
    direction: LineBDivergenceV1,
    scale: f64,
    device: &burn_cuda::CudaDevice,
) -> Result<LineBRootLossPlanV1, Box<dyn Error>> {
    let rows = targets.len();
    if rows == 0 || host.action_offsets.len() != rows + 1 || !(scale.is_finite() && scale >= 0.0) {
        return Err(training_error(
            "line (b) root plan cardinality or scale mismatch",
        ));
    }
    let mut max_actions = 0_usize;
    for offsets in host.action_offsets.windows(2) {
        let count = offsets[1]
            .checked_sub(offsets[0])
            .filter(|count| *count > 0)
            .ok_or_else(|| training_error("line (b) root plan found an empty row"))?;
        max_actions = max_actions.max(count);
    }
    let mut pad_gather = Vec::with_capacity(rows * max_actions);
    let mut pad_mask = Vec::with_capacity(rows * max_actions);
    let mut target = Vec::with_capacity(rows * max_actions);
    for (row, (log_q, q)) in targets.iter().enumerate() {
        let begin = host.action_offsets[row];
        let count = host.action_offsets[row + 1] - begin;
        if log_q.len() != count || q.len() != count {
            return Err(training_error(
                "line (b) root target width differs from its row",
            ));
        }
        for action in 0..max_actions {
            if action < count {
                let value = match direction {
                    LineBDivergenceV1::Reverse => log_q[action] as f32,
                    LineBDivergenceV1::Forward => q[action] as f32,
                };
                if !value.is_finite() {
                    return Err(training_error("line (b) root target is non-finite"));
                }
                pad_gather.push(i32::try_from(begin + action)?);
                pad_mask.push(0.0_f32);
                target.push(value);
            } else {
                pad_gather.push(i32::try_from(begin)?);
                pad_mask.push(DENSE_PAD_MASK_NEGATIVE_V1);
                target.push(0.0_f32);
            }
        }
    }
    Ok(LineBRootLossPlanV1 {
        pad_gather: Tensor::from_data(TensorData::new(pad_gather, [rows * max_actions]), device),
        pad_mask: Tensor::from_data(TensorData::new(pad_mask, [rows, max_actions]), device),
        target: Tensor::from_data(TensorData::new(target, [rows, max_actions]), device),
        direction,
        scale: scale as f32,
        rows,
        max_actions,
    })
}

fn line_b_root_loss_v1(
    logits: Tensor<CudaAutodiffBackendV1, 1>,
    plan: &LineBRootLossPlanV1,
) -> Tensor<CudaAutodiffBackendV1, 1> {
    let padded = logits
        .select(0, plan.pad_gather.clone())
        .reshape([plan.rows, plan.max_actions])
        + plan.pad_mask.clone();
    let row_max = padded.clone().max_dim(1).detach();
    let centered = padded - row_max;
    let log_normalizer = centered.clone().exp().sum_dim(1).log();
    let log_p = centered - log_normalizer;
    let target = plan.target.clone();
    let sum = match plan.direction {
        LineBDivergenceV1::Reverse => (log_p.clone().exp() * (log_p - target)).sum(),
        LineBDivergenceV1::Forward => (target * log_p).sum().mul_scalar(-1.0),
    };
    sum.mul_scalar(plan.scale)
}

pub(crate) struct LineBRootBackwardOutputsV1 {
    pub(crate) raw_gauge_residual: f32,
    pub(crate) logit_outputs: Vec<f32>,
    /// `HEAD_ONLY_TRAINABLE_TENSORS_V1` order; empty where no gradient exists.
    pub(crate) head_gradients: Vec<Vec<f32>>,
}

/// The seven trainable head tensors' gradients (burn layout; norms and sums
/// do not depend on it), `None` where the loss gives the tensor no gradient.
fn head_gradient_tensors_v1(
    model: &ProductionNet8<CudaAutodiffBackendV1>,
    gradients: &GradientsParams,
) -> HeadGradientTensorsV1 {
    let bias = |linear: &Linear<CudaAutodiffBackendV1>| {
        linear
            .bias
            .as_ref()
            .and_then(|bias| gradients.get::<CudaBackendV1, 1>(bias.id))
    };
    (
        [
            gradients.get::<CudaBackendV1, 2>(model.scorer.hidden.weight.id),
            gradients.get::<CudaBackendV1, 2>(model.scorer.output.weight.id),
            gradients.get::<CudaBackendV1, 2>(model.value_head.hidden.weight.id),
            gradients.get::<CudaBackendV1, 2>(model.value_head.output.weight.id),
        ],
        [
            bias(&model.scorer.hidden),
            bias(&model.value_head.hidden),
            bias(&model.value_head.output),
        ],
    )
}

/// Registers the head tensors after `transaction`'s existing entries and
/// returns a function that maps the executed readback (from `first`) back
/// to `HEAD_ONLY_TRAINABLE_TENSORS_V1` order.
fn read_head_gradients_v1(
    transaction: Transaction<CudaBackendV1>,
    heads: HeadGradientTensorsV1,
) -> (Transaction<CudaBackendV1>, [bool; 7]) {
    // Head order: scorer.0.weight, scorer.0.bias, scorer.2.weight,
    // value_head.0.weight, value_head.0.bias, value_head.2.weight,
    // value_head.2.bias.
    let (
        [scorer_hidden, scorer_output, value_hidden, value_output],
        [scorer_bias, value_bias, value_output_bias],
    ) = heads;
    let mut present = [false; 7];
    let mut transaction = transaction;
    for (slot, tensor) in [
        (0, scorer_hidden),
        (2, scorer_output),
        (3, value_hidden),
        (5, value_output),
    ] {
        if let Some(tensor) = tensor {
            present[slot] = true;
            transaction = transaction.register(tensor);
        }
    }
    for (slot, tensor) in [(1, scorer_bias), (4, value_bias), (6, value_output_bias)] {
        if let Some(tensor) = tensor {
            present[slot] = true;
            transaction = transaction.register(tensor);
        }
    }
    (transaction, present)
}

/// Maps readback entries registered by `read_head_gradients_v1` (weights
/// first in slots 0, 2, 3, 5, then biases in slots 1, 4, 6) to head order.
fn head_gradients_from_readback_v1(
    entries: Vec<TensorData>,
    present: [bool; 7],
) -> Result<Vec<Vec<f32>>, Box<dyn Error>> {
    let mut heads = vec![Vec::new(); 7];
    let mut entries = entries.into_iter();
    for slot in [0, 2, 3, 5, 1, 4, 6] {
        if present[slot] {
            let data = entries
                .next()
                .ok_or_else(|| training_error("line (b) head readback is short"))?;
            heads[slot] = data.into_vec::<f32>()?;
        }
    }
    if entries.next().is_some() {
        return Err(training_error("line (b) head readback is long"));
    }
    Ok(heads)
}

impl ExperimentalDeviceTrainStateV1 {
    /// The roots' forward, the auxiliary loss and its backward, folded into
    /// `accumulator`; reads back the root-row logits, the term's scorer-bias
    /// gradient and its head-tensor gradients.
    pub(crate) fn line_b_root_backward_v1(
        &self,
        accumulator: &mut burn::optim::GradientsAccumulator<ProductionNet8<CudaAutodiffBackendV1>>,
        batch: &DevicePackedBatch<CudaAutodiffBackendV1>,
        plan: &LineBRootLossPlanV1,
    ) -> Result<LineBRootBackwardOutputsV1, Box<dyn Error>> {
        let (logits, _values) = if self.wide {
            self.model.forward_wide_v1(batch)
        } else {
            self.model.forward(batch)
        };
        let logit_outputs = logits.clone().inner();
        let loss = line_b_root_loss_v1(logits, plan);
        let raw_gradients = loss.backward();
        let mut gradients = GradientsParams::from_grads(raw_gradients, &self.model);
        let gauge_parameter = self
            .model
            .scorer
            .output
            .bias
            .as_ref()
            .ok_or_else(|| training_error("scorer output has no bias"))?;
        let gauge_gradient = gradients
            .remove::<CudaBackendV1, 1>(gauge_parameter.id)
            .ok_or_else(|| training_error("line (b) scorer output bias gradient is missing"))?;
        let heads = head_gradient_tensors_v1(&self.model, &gradients);
        let transaction = Transaction::<CudaBackendV1>::default()
            .register(logit_outputs)
            .register(gauge_gradient.clone());
        let (transaction, present) = read_head_gradients_v1(transaction, heads);
        let mut readback = transaction.try_execute()?.into_iter();
        let logit_outputs = readback
            .next()
            .ok_or_else(|| training_error("line (b) logit readback is missing"))?
            .into_vec::<f32>()?;
        let gauge = readback
            .next()
            .ok_or_else(|| training_error("line (b) gauge readback is missing"))?
            .into_vec::<f32>()?;
        let raw_gauge_residual = *gauge
            .first()
            .ok_or_else(|| training_error("line (b) scorer output bias gradient is empty"))?;
        let head_gradients = head_gradients_from_readback_v1(readback.collect(), present)?;
        gradients.register(gauge_parameter.id, gauge_gradient);
        accumulator.accumulate(&self.model, gradients);
        Ok(LineBRootBackwardOutputsV1 {
            raw_gauge_residual,
            logit_outputs,
            head_gradients,
        })
    }

    /// Treatment-update sibling of `chunk_backward_coefficients_v1`: the same
    /// forward, loss, backward and accumulation, plus a readback of the
    /// chunk's head-tensor gradients for the auxiliary-to-ordinary ratio.
    pub(crate) fn chunk_backward_coefficients_head_readback_v1(
        &self,
        accumulator: &mut burn::optim::GradientsAccumulator<ProductionNet8<CudaAutodiffBackendV1>>,
        batch: &DevicePackedBatch<CudaAutodiffBackendV1>,
        plan: &DenseGroupLossPlanGaeV1,
        value_coefficient: f32,
        normalization_group_count: f32,
    ) -> Result<ChunkHeadReadbackV1, Box<dyn Error>> {
        let (logits, values) = if self.wide {
            self.model.forward_wide_v1(batch)
        } else {
            self.model.forward(batch)
        };
        let logit_outputs = logits.clone().inner();
        let value_outputs = values.clone().inner();
        let loss = dense_group_loss_coefficients_v1(
            logits,
            values,
            plan,
            value_coefficient,
            normalization_group_count,
            false,
        )?;
        let raw_gradients = loss.backward();
        let mut gradients = GradientsParams::from_grads(raw_gradients, &self.model);
        if batch.empty_relations_v3 {
            register_empty_relation_gradients_v3(&self.model, batch, &mut gradients)?;
        }
        if gradients.len() != PARAMETER_TENSOR_COUNT_V1 {
            return Err(training_error(format!(
                "CUDA gae chunk gradient tensor count mismatch: {} != {PARAMETER_TENSOR_COUNT_V1}",
                gradients.len()
            )));
        }
        let gauge_parameter = self
            .model
            .scorer
            .output
            .bias
            .as_ref()
            .ok_or_else(|| training_error("scorer output has no bias"))?;
        let gauge_gradient = gradients
            .remove::<CudaBackendV1, 1>(gauge_parameter.id)
            .ok_or_else(|| training_error("scorer output bias gradient is missing"))?;
        let heads = head_gradient_tensors_v1(&self.model, &gradients);
        let transaction = Transaction::<CudaBackendV1>::default()
            .register(logit_outputs)
            .register(value_outputs)
            .register(gauge_gradient.clone());
        let (transaction, present) = read_head_gradients_v1(transaction, heads);
        let mut readback = transaction.try_execute()?.into_iter();
        let mut next = |what: &str| {
            readback
                .next()
                .ok_or_else(|| training_error(format!("CUDA gae chunk {what} readback is missing")))
        };
        let logit_outputs = next("logit")?.into_vec::<f32>()?;
        let value_outputs = next("value")?.into_vec::<f32>()?;
        let gauge = next("gauge")?.into_vec::<f32>()?;
        let raw_gauge_residual = *gauge
            .first()
            .ok_or_else(|| training_error("scorer output bias gradient is empty"))?;
        let head_gradients = head_gradients_from_readback_v1(readback.collect(), present)?;
        gradients.register(gauge_parameter.id, gauge_gradient);
        accumulator.accumulate(&self.model, gradients);
        Ok((
            ChunkBackwardOutputsV1 {
                raw_gauge_residual,
                logit_outputs,
                value_outputs,
                #[cfg(test)]
                device_objective: None,
            },
            head_gradients,
        ))
    }
}

impl ExperimentalDeviceTrainStateV1 {
    /// Packs the roots' encoded views into their own batch, builds the loss
    /// plan and runs [`Self::line_b_root_backward_v1`]; also returns the
    /// batch's action offsets so the caller can split the root-row logits.
    pub(crate) fn line_b_root_term_v1(
        &self,
        accumulator: &mut burn::optim::GradientsAccumulator<ProductionNet8<CudaAutodiffBackendV1>>,
        views: &[crate::native_policy_value_net_v1::NativeEncodedDecisionViewV1<'_>],
        targets: &[(Vec<f64>, Vec<f64>)],
        direction: LineBDivergenceV1,
        scale: f64,
    ) -> Result<(LineBRootBackwardOutputsV1, Vec<usize>), Box<dyn Error>> {
        let mut workspace = HostPackingWorkspace::default();
        workspace.pack_views(views)?;
        let plan =
            build_line_b_root_loss_plan_v1(&workspace, targets, direction, scale, &self.device)?;
        let batch = DevicePackedBatch::upload_feature_transfer_v3(&self.device, &workspace);
        let outputs = self.line_b_root_backward_v1(accumulator, &batch, &plan)?;
        Ok((outputs, workspace.action_offsets.clone()))
    }
}
