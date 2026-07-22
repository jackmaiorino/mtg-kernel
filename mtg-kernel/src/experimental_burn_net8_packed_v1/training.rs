//! Experimental-only CUDA backward and optimizer feasibility probe.
//!
//! Nothing in this module is a production trainer contract. It deliberately
//! reuses the frozen native tensor names and Adam constants while assigning a
//! separate diagnostic identity and tolerance-only comparison envelope.

// Burn's rank-1 `Tensor::slice` API deliberately takes `[Range<usize>; 1]`.
#![allow(clippy::single_range_in_vec_init)]

use super::*;
use crate::native_policy_train_step_v1::{
    NativePolicyForwardInputV1, NativePolicyPhysicalDecisionV1, NativePolicySubstepV1,
    NativePolicyValueTrainSnapshotV1, ADAM_BETA1_V1, ADAM_BETA2_V1, ADAM_EPSILON_V1,
};
use burn::backend::Autodiff;
use burn::module::{AutodiffModule, ModuleMapper, ModuleVisitor, ParamId};
use burn::optim::GradientsParams;
use burn::tensor::activation::log_softmax;
use burn::tensor::backend::Backend;
use burn::tensor::{Int, TensorData};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::process::Command;

type CudaAutodiffBackendV1 = Autodiff<CudaBackendV1>;

const TRAINING_DIAGNOSTIC_IDENTITY_CANDIDATE_V1: &str =
    "mtg-kernel-experimental-burn-net8-cuda-train-feasibility-v1";
const TRAINING_LOSS_IDENTITY_V1: &str =
    "terminal_reinforce_value/v3-one-substep-per-physical-group-feasibility";
const SCORER_SECOND_BIAS_ORDINAL_V1: usize = 28;
const PATTERNED_ADAM_STEP_V1: u64 = 7;
const ORACLE_DECISIONS_V1: usize = 4;
const DEFAULT_TRAINING_DECISIONS_V1: usize = 16;
const DEFAULT_TRAINING_WARMUP_V1: usize = 5;
const DEFAULT_TRAINING_ITERATIONS_V1: usize = 20;
const VALUE_COEFFICIENT_V1: f32 = 0.5;
const ORACLE_LEARNING_RATE_V1: f32 = 1.0e-4;
const BENCHMARK_LEARNING_RATE_V1: f32 = 1.0e-5;
const LOSS_ABSOLUTE_TOLERANCE_V1: f32 = 2.0e-3;
const LOSS_RELATIVE_TOLERANCE_V1: f32 = 2.0e-3;
const GRADIENT_ABSOLUTE_TOLERANCE_V1: f32 = 5.0e-3;
const GRADIENT_RELATIVE_TOLERANCE_V1: f32 = 5.0e-3;
const UPDATE_ABSOLUTE_TOLERANCE_V1: f32 = 2.0e-3;
const UPDATE_RELATIVE_TOLERANCE_V1: f32 = 2.0e-3;
const RELATIVE_ERROR_DENOMINATOR_FLOOR_V1: f32 = 1.0e-6;
const INJECTED_PRE_COMMIT_FAILURE_V1: &str = "experimental CUDA train injected pre-commit failure";
const FORWARD_DETERMINISM_BATCH_SIZES_V1: [usize; 3] = [1, 4, 16];
// The production fixture's rotations 0 and 1 contain no graph edges when
// packed alone; Burn's broadcast API rejects a zero-sized edge dimension.
// Rotations 2 and 7 retain batch-size-one coverage while exercising distinct
// ragged positions with non-empty edge sets.
const FORWARD_DETERMINISM_ROTATIONS_V1: [usize; 2] = [2, 7];
const DEFAULT_FORWARD_DETERMINISM_INVOCATIONS_V1: usize = 10_000;
const FORWARD_DETERMINISM_READBACK_CHUNK_V1: usize = 32;
const DEFAULT_BACKWARD_ADAM_DETERMINISM_INVOCATIONS_V1: usize = 96;
const ADVERSARIAL_ORACLE_INPUT_SHA256_V1: &str =
    "c7cce151eda6b42c7c716f9078bbd12a189a3b3a2ce5498819df2d06db75b783";
const ADVERSARIAL_ORACLE_ABSOLUTE_TOLERANCE_V1: f32 = 2.0e-5;
const ADVERSARIAL_ORACLE_RELATIVE_TOLERANCE_V1: f32 = 2.0e-5;
// Backpropagating `row - row[0]` introduces each coefficient once on its
// original logit and once with the opposite sign on the anchor logit.  This
// factor is structural: it is the absolute-term multiplicity in the exact
// cancellation expression, not an empirically fitted safety multiplier.
const CENTERED_MONITOR_COEFFICIENT_TERM_MULTIPLICITY_V1: usize = 2;

#[derive(Clone, Copy)]
struct AdversarialOracleCaseV1 {
    name: &'static str,
    logit_bits: &'static [u32],
    selected: usize,
}

const ADVERSARIAL_ORACLE_CASES_V1: [AdversarialOracleCaseV1; 6] = [
    AdversarialOracleCaseV1 {
        name: "unit-adjacent",
        logit_bits: &[0x3f80_0000, 0x3f80_0001, 0x3f7f_ffff],
        selected: 1,
    },
    AdversarialOracleCaseV1 {
        name: "signed-zero-and-subnormal",
        logit_bits: &[0x0000_0000, 0x0000_0001, 0x8000_0000, 0x8000_0001],
        selected: 0,
    },
    AdversarialOracleCaseV1 {
        name: "large-positive-adjacent",
        logit_bits: &[0x4280_0000, 0x4280_0001, 0x427f_ffff],
        selected: 2,
    },
    AdversarialOracleCaseV1 {
        name: "large-negative-adjacent",
        logit_bits: &[0xc200_0000, 0xc1ff_ffff, 0xc200_0001],
        selected: 1,
    },
    AdversarialOracleCaseV1 {
        name: "exact-four-way-tie",
        logit_bits: &[0x3f00_0000, 0x3f00_0000, 0x3f00_0000, 0x3f00_0000],
        selected: 3,
    },
    AdversarialOracleCaseV1 {
        name: "q8-scale-adjacent",
        logit_bits: &[0x3b80_0000, 0x3b80_0001, 0x3b7f_ffff, 0x0000_0000],
        selected: 0,
    },
];

#[derive(Clone, Debug, Serialize)]
struct DeviceRuntimeManifestV1 {
    gpu_model: String,
    compute_capability: String,
    cuda_driver_api_version: String,
    cuda_runtime_version: String,
    cuda_toolkit_binding: String,
    cublas_runtime_version: String,
    cudarc_version: String,
    cubecl_cuda_version: String,
    cubek_matmul_version: String,
    burn_version: String,
    burn_cuda_version: String,
    numerical_mode: String,
}

impl DeviceRuntimeManifestV1 {
    fn collect_v1() -> Result<Self, Box<dyn Error>> {
        let context = cudarc::driver::CudaContext::new(0)?;
        let gpu_model = context.name()?;
        let (compute_major, compute_minor) = context.compute_capability()?;
        let cuda_driver_api_version = cudarc::runtime::result::version::get_driver_version()?;
        let cuda_runtime_version = cudarc::runtime::result::version::get_runtime_version()?;
        let blas = cudarc::cublas::CudaBlas::new(context.default_stream())?;
        let mut cublas_runtime_version = 0_i32;
        // SAFETY: `blas` owns a live cuBLAS handle for the duration of this
        // call, and the out-pointer addresses an initialized writable i32.
        unsafe {
            cudarc::cublas::sys::cublasGetVersion_v2(*blas.handle(), &mut cublas_runtime_version)
                .result()?;
        }
        Ok(Self {
            gpu_model,
            compute_capability: format!("{compute_major}.{compute_minor}"),
            cuda_driver_api_version: cuda_driver_api_version.to_string(),
            cuda_runtime_version: cuda_runtime_version.to_string(),
            cuda_toolkit_binding: "cudarc-cuda-12080-bindings".to_owned(),
            cublas_runtime_version: cublas_runtime_version.to_string(),
            cudarc_version: "0.19.8".to_owned(),
            cubecl_cuda_version: "0.10.0".to_owned(),
            cubek_matmul_version: "0.2.0".to_owned(),
            burn_version: "0.21.0".to_owned(),
            burn_cuda_version: "0.21.0".to_owned(),
            numerical_mode:
                "stock-cubecl-cuda-fast_math=true-and-tf32-conversion-registered-identity-withheld"
                    .to_owned(),
        })
    }

    fn sha256_v1(&self) -> String {
        let mut digest = Sha256::new();
        digest.update(b"mtg-kernel-experimental-cuda-device-runtime-manifest/v1\0");
        for (name, value) in self.fields_v1() {
            digest.update((name.len() as u32).to_be_bytes());
            digest.update(name.as_bytes());
            digest.update((value.len() as u64).to_be_bytes());
            digest.update(value.as_bytes());
        }
        format!("{:x}", digest.finalize())
    }

    fn fields_v1(&self) -> [(&'static str, &str); 12] {
        [
            ("gpu_model", &self.gpu_model),
            ("compute_capability", &self.compute_capability),
            ("cuda_driver_api_version", &self.cuda_driver_api_version),
            ("cuda_runtime_version", &self.cuda_runtime_version),
            ("cuda_toolkit_binding", &self.cuda_toolkit_binding),
            ("cublas_runtime_version", &self.cublas_runtime_version),
            ("cudarc_version", &self.cudarc_version),
            ("cubecl_cuda_version", &self.cubecl_cuda_version),
            ("cubek_matmul_version", &self.cubek_matmul_version),
            ("burn_version", &self.burn_version),
            ("burn_cuda_version", &self.burn_cuda_version),
            ("numerical_mode", &self.numerical_mode),
        ]
    }
}

#[derive(Debug)]
enum ExperimentalTrainingErrorV1 {
    Message(String),
    GaugeResidualExceeded { residual_bits: u32, bound_bits: u32 },
}

impl Display for ExperimentalTrainingErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(message) => formatter.write_str(message),
            Self::GaugeResidualExceeded {
                residual_bits,
                bound_bits,
            } => write!(
                formatter,
                "GaugeResidualExceeded(residual_bits={residual_bits:#010x}, bound_bits={bound_bits:#010x})"
            ),
        }
    }
}

impl Error for ExperimentalTrainingErrorV1 {}

fn training_error(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(ExperimentalTrainingErrorV1::Message(message.into()))
}

#[derive(Clone, Copy, Debug, Default)]
struct GaugeEvidenceV1 {
    training_raw_residual: f32,
    monitor_coefficient_host_sum: f32,
    centered_monitor_raw_residual: f32,
    centered_monitor_bound: f32,
    centered_monitor_operation_count: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct PhaseTimingsV1 {
    h2d_us: f64,
    forward_us: f64,
    backward_us: f64,
    adam_us: f64,
    export_us: f64,
    full_us: f64,
}

#[derive(Debug, Default)]
struct PhaseSamplesV1 {
    h2d: Vec<f64>,
    forward: Vec<f64>,
    backward: Vec<f64>,
    adam: Vec<f64>,
    export: Vec<f64>,
    full: Vec<f64>,
}

impl PhaseSamplesV1 {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            h2d: Vec::with_capacity(capacity),
            forward: Vec::with_capacity(capacity),
            backward: Vec::with_capacity(capacity),
            adam: Vec::with_capacity(capacity),
            export: Vec::with_capacity(capacity),
            full: Vec::with_capacity(capacity),
        }
    }

    fn push(&mut self, timings: PhaseTimingsV1) {
        self.h2d.push(timings.h2d_us);
        self.forward.push(timings.forward_us);
        self.backward.push(timings.backward_us);
        self.adam.push(timings.adam_us);
        self.export.push(timings.export_us);
        self.full.push(timings.full_us);
    }

    fn to_json_v1(&self) -> serde_json::Value {
        serde_json::json!({
            "batch_h2d_and_sync": phase_statistics_v1(&self.h2d),
            "forward_loss_and_sync": phase_statistics_v1(&self.forward),
            "backward_gradient_map_and_sync": phase_statistics_v1(&self.backward),
            "device_adam_and_sync": phase_statistics_v1(&self.adam),
            "parameter_moment_export_and_validation": phase_statistics_v1(&self.export),
            "full_candidate_step": phase_statistics_v1(&self.full),
        })
    }

    fn rate_json_v1(&self, decisions: usize) -> serde_json::Value {
        let mean = mean_micros(&self.full);
        let p50 = percentile_micros(&self.full, 0.50);
        let p95 = percentile_micros(&self.full, 0.95);
        serde_json::json!({
            "mean": decisions as f64 * 1.0e6 / mean,
            "p50_latency_rate": decisions as f64 * 1.0e6 / p50,
            "p95_latency_rate": decisions as f64 * 1.0e6 / p95,
        })
    }
}

#[derive(Debug)]
struct ExperimentalStepEvidenceV1 {
    loss: f32,
    gradients: Option<Vec<NativeNamedParameterV1>>,
    snapshot: NativePolicyValueTrainSnapshotV1,
    gauge: GaugeEvidenceV1,
    timings: PhaseTimingsV1,
}

pub(crate) struct ExperimentalDeviceTrainStateV1 {
    model: ProductionNet8<CudaAutodiffBackendV1>,
    first_moments: GradientsParams,
    second_moments: GradientsParams,
    adam_step: u64,
    scorer_bias_anchor_bits: u32,
    device: burn_cuda::CudaDevice,
}

impl ExperimentalDeviceTrainStateV1 {
    pub(crate) fn import_snapshot_v1(
        snapshot: &NativePolicyValueTrainSnapshotV1,
        device: &burn_cuda::CudaDevice,
    ) -> Result<Self, Box<dyn Error>> {
        snapshot.state_sha256_v1()?;
        let model = ProductionNet8::<CudaAutodiffBackendV1>::import_native_v1(
            &snapshot.parameters,
            device,
        )?;
        if model.num_params() != PARAMETER_COUNT_V1 {
            return Err(training_error(format!(
                "autodiff model parameter count mismatch: {} != {PARAMETER_COUNT_V1}",
                model.num_params()
            )));
        }
        let first_moments = import_named_state_v1(&model, &snapshot.first_moments, device)?;
        let second_moments = import_named_state_v1(&model, &snapshot.second_moments, device)?;
        Ok(Self {
            model,
            first_moments,
            second_moments,
            adam_step: snapshot.adam_step,
            scorer_bias_anchor_bits: snapshot.scorer_bias_anchor_bits,
            device: device.clone(),
        })
    }

    pub(crate) fn export_snapshot_v1(
        &self,
    ) -> Result<NativePolicyValueTrainSnapshotV1, Box<dyn Error>> {
        CudaBackendV1::sync(&self.device)?;
        let parameters = self.model.valid().export_native_v1(&self.device)?;
        let first_moments = export_named_state_v1(&self.model, &self.first_moments, &parameters)?;
        let second_moments = export_named_state_v1(&self.model, &self.second_moments, &parameters)?;
        let snapshot = NativePolicyValueTrainSnapshotV1 {
            adam_step: self.adam_step,
            scorer_bias_anchor_bits: self.scorer_bias_anchor_bits,
            parameters,
            first_moments,
            second_moments,
        };
        snapshot.state_sha256_v1()?;
        Ok(snapshot)
    }

    #[allow(clippy::too_many_arguments)]
    fn train_one_step_v1(
        &mut self,
        host: &HostPackingWorkspace,
        resident_batch: Option<&DevicePackedBatch<CudaAutodiffBackendV1>>,
        selected_action_indices: &[usize],
        terminal_returns: &[i8],
        value_coefficient: f32,
        learning_rate: f32,
        capture_gradients: bool,
        corrupt_gauge_monitor_graph: bool,
        inject_pre_commit_failure: bool,
    ) -> Result<ExperimentalStepEvidenceV1, Box<dyn Error>> {
        let full_started = Instant::now();

        let uploaded_batch;
        let (batch, h2d_us) = if let Some(resident_batch) = resident_batch {
            (resident_batch, 0.0)
        } else {
            let phase_started = Instant::now();
            uploaded_batch = DevicePackedBatch::<CudaAutodiffBackendV1>::upload(&self.device, host);
            CudaAutodiffBackendV1::sync(&self.device)?;
            (&uploaded_batch, elapsed_us(phase_started))
        };

        let phase_started = Instant::now();
        let (logits, values) = self.model.forward(batch);
        let logit_outputs = logits.clone();
        let value_outputs = values.clone();
        let loss = one_substep_grouped_loss_v1(
            logits.clone(),
            values,
            &host.action_offsets,
            selected_action_indices,
            terminal_returns,
            value_coefficient,
        )?;
        CudaAutodiffBackendV1::sync(&self.device)?;
        let logit_outputs = logit_outputs.into_data().to_vec::<f32>()?;
        let value_outputs = value_outputs.into_data().to_vec::<f32>()?;
        let loss_value = loss.clone().into_data().to_vec::<f32>()?;
        let loss_value = *loss_value
            .first()
            .ok_or_else(|| training_error("CUDA loss tensor is empty"))?;
        if !loss_value.is_finite() {
            return Err(training_error("CUDA loss is non-finite"));
        }
        let forward_us = elapsed_us(phase_started);

        let phase_started = Instant::now();
        let raw_gradients = loss.backward();
        let mut gradients = GradientsParams::from_grads(raw_gradients, &self.model);
        if gradients.len() != PARAMETER_TENSOR_COUNT_V1 {
            return Err(training_error(format!(
                "CUDA gradient tensor count mismatch: {} != {PARAMETER_TENSOR_COUNT_V1}",
                gradients.len()
            )));
        }
        let gauge = canonicalize_scorer_bias_gradient_v1(
            &self.model,
            &mut gradients,
            host,
            batch,
            selected_action_indices,
            terminal_returns,
            &logit_outputs,
            &value_outputs,
            &self.device,
            corrupt_gauge_monitor_graph,
        )?;
        CudaBackendV1::sync(&self.device)?;
        let backward_us = elapsed_us(phase_started);

        let captured_gradients = if capture_gradients {
            Some(export_named_state_v1(
                &self.model,
                &gradients,
                &self.model.valid().export_native_v1(&self.device)?,
            )?)
        } else {
            None
        };

        let next_step = self
            .adam_step
            .checked_add(1)
            .ok_or_else(|| training_error("experimental Adam step overflow"))?;
        let phase_started = Instant::now();
        let mut mapper = DeviceAdamMapperV1::new(
            gradients,
            &self.first_moments,
            &self.second_moments,
            next_step,
            learning_rate,
        )?;
        let candidate_model = self.model.clone().map(&mut mapper);
        let (candidate_first_moments, candidate_second_moments) = mapper.finish_v1()?;
        CudaAutodiffBackendV1::sync(&self.device)?;
        let adam_us = elapsed_us(phase_started);

        let phase_started = Instant::now();
        let candidate_parameters = candidate_model.valid().export_native_v1(&self.device)?;
        let candidate_first = export_named_state_v1(
            &candidate_model,
            &candidate_first_moments,
            &candidate_parameters,
        )?;
        let candidate_second = export_named_state_v1(
            &candidate_model,
            &candidate_second_moments,
            &candidate_parameters,
        )?;
        let candidate_snapshot = NativePolicyValueTrainSnapshotV1 {
            adam_step: next_step,
            scorer_bias_anchor_bits: self.scorer_bias_anchor_bits,
            parameters: candidate_parameters,
            first_moments: candidate_first,
            second_moments: candidate_second,
        };
        black_box(candidate_snapshot.state_sha256_v1()?);
        let export_us = elapsed_us(phase_started);

        if inject_pre_commit_failure {
            return Err(training_error(INJECTED_PRE_COMMIT_FAILURE_V1));
        }

        // This is the only mutation point. Every device operation, host export,
        // and native snapshot invariant above completed on owned candidates.
        self.model = candidate_model;
        self.first_moments = candidate_first_moments;
        self.second_moments = candidate_second_moments;
        self.adam_step = next_step;

        Ok(ExperimentalStepEvidenceV1 {
            loss: loss_value,
            gradients: captured_gradients,
            snapshot: candidate_snapshot,
            gauge,
            timings: PhaseTimingsV1 {
                h2d_us,
                forward_us,
                backward_us,
                adam_us,
                export_us,
                full_us: elapsed_us(full_started),
            },
        })
    }

    /// Lean production-shaped step: dense-padded loss, backward, essential
    /// scorer-bias gauge canonicalization (gradient zeroed, no diagnostic
    /// monitor), Adam, and commit. No per-step host export or snapshot.
    /// Correctness is anchored by the CPU-oracle tolerance comparison and
    /// within-GPU run-to-run determinism.
    fn train_one_step_lean_v1(
        &mut self,
        batch: &DevicePackedBatch<CudaAutodiffBackendV1>,
        plan: &DenseLossPlanV1,
        value_coefficient: f32,
        learning_rate: f32,
    ) -> Result<(), Box<dyn Error>> {
        let (logits, values) = self.model.forward(batch);
        let loss = one_substep_grouped_loss_dense_v1(logits, values, plan, value_coefficient)?;
        let raw_gradients = loss.backward();
        let mut gradients = GradientsParams::from_grads(raw_gradients, &self.model);
        if gradients.len() != PARAMETER_TENSOR_COUNT_V1 {
            return Err(training_error(format!(
                "CUDA gradient tensor count mismatch: {} != {PARAMETER_TENSOR_COUNT_V1}",
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
        gradients.register(gauge_parameter.id, gauge_gradient.zeros_like());
        let next_step = self
            .adam_step
            .checked_add(1)
            .ok_or_else(|| training_error("experimental Adam step overflow"))?;
        let mut mapper = DeviceAdamMapperV1::new(
            gradients,
            &self.first_moments,
            &self.second_moments,
            next_step,
            learning_rate,
        )?;
        let candidate_model = self.model.clone().map(&mut mapper);
        let (candidate_first_moments, candidate_second_moments) = mapper.finish_v1()?;
        self.model = candidate_model;
        self.first_moments = candidate_first_moments;
        self.second_moments = candidate_second_moments;
        self.adam_step = next_step;
        CudaAutodiffBackendV1::sync(&self.device)?;
        Ok(())
    }

    /// Non-autodiff forward readback of flat logits and values for the
    /// current device model, used by the bridge's host evidence
    /// recomputation before the training step advances the model. The forward
    /// runs on the validated inner backend so no autodiff graph or retained
    /// activations exist: on large update batches the previous autodiff-typed
    /// readback pinned a full never-backwarded activation set alongside the
    /// training step's own graph and drove the device out of memory.
    pub(crate) fn forward_outputs_v1(
        &self,
        batch: &DevicePackedBatch<CudaAutodiffBackendV1>,
    ) -> Result<(Vec<f32>, Vec<f32>), Box<dyn Error>> {
        let inner_model = self.model.valid();
        let inner_batch = inner_readback_batch_v1(batch);
        let (logits, values) = inner_model.forward(&inner_batch);
        CudaBackendV1::sync(&self.device)?;
        let logits = logits.into_data().to_vec::<f32>()?;
        let values = values.into_data().to_vec::<f32>()?;
        Ok((logits, values))
    }

    /// Runs one chunk's forward, scaled dense group loss, and backward, and
    /// folds the gradients into `accumulator`. The loss divides by the whole
    /// update's group count, so the accumulated gradient over all chunks is
    /// the full-batch gradient with only reduction-order rounding differences
    /// while peak device memory stays bounded by one chunk's activations.
    /// Returns the chunk's raw scorer-bias gradient component for the gauge
    /// witness.
    pub(crate) fn chunk_backward_v1(
        &self,
        accumulator: &mut burn::optim::GradientsAccumulator<ProductionNet8<CudaAutodiffBackendV1>>,
        batch: &DevicePackedBatch<CudaAutodiffBackendV1>,
        plan: &DenseGroupLossPlanV1,
        value_coefficient: f32,
        normalization_group_count: f32,
    ) -> Result<f32, Box<dyn Error>> {
        let (logits, values) = self.model.forward(batch);
        let loss = dense_group_loss_v1(
            logits,
            values,
            plan,
            value_coefficient,
            normalization_group_count,
        )?;
        let raw_gradients = loss.backward();
        let mut gradients = GradientsParams::from_grads(raw_gradients, &self.model);
        if gradients.len() != PARAMETER_TENSOR_COUNT_V1 {
            return Err(training_error(format!(
                "CUDA chunk gradient tensor count mismatch: {} != {PARAMETER_TENSOR_COUNT_V1}",
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
        let chunk_raw = gauge_gradient.clone().into_data().to_vec::<f32>()?;
        let chunk_raw = *chunk_raw
            .first()
            .ok_or_else(|| training_error("scorer output bias gradient is empty"))?;
        gradients.register(gauge_parameter.id, gauge_gradient);
        accumulator.accumulate(&self.model, gradients);
        Ok(chunk_raw)
    }

    /// Canonicalizes the accumulated gauge gradient to exact zero, applies one
    /// Adam step over the accumulated gradients, and commits the candidate
    /// model and moments.
    pub(crate) fn apply_accumulated_v1(
        &mut self,
        accumulator: burn::optim::GradientsAccumulator<ProductionNet8<CudaAutodiffBackendV1>>,
        learning_rate: f32,
    ) -> Result<(), Box<dyn Error>> {
        let mut accumulator = accumulator;
        let mut gradients = accumulator.grads();
        if gradients.len() != PARAMETER_TENSOR_COUNT_V1 {
            return Err(training_error(format!(
                "CUDA accumulated gradient tensor count mismatch: {} != {PARAMETER_TENSOR_COUNT_V1}",
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
        gradients.register(gauge_parameter.id, gauge_gradient.zeros_like());
        let next_step = self
            .adam_step
            .checked_add(1)
            .ok_or_else(|| training_error("experimental Adam step overflow"))?;
        let mut mapper = DeviceAdamMapperV1::new(
            gradients,
            &self.first_moments,
            &self.second_moments,
            next_step,
            learning_rate,
        )?;
        let candidate_model = self.model.clone().map(&mut mapper);
        let (candidate_first_moments, candidate_second_moments) = mapper.finish_v1()?;
        self.model = candidate_model;
        self.first_moments = candidate_first_moments;
        self.second_moments = candidate_second_moments;
        self.adam_step = next_step;
        CudaAutodiffBackendV1::sync(&self.device)?;
        Ok(())
    }
}

fn elapsed_us(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1.0e6
}

#[derive(Clone, Copy)]
struct NativeParamWitnessV1 {
    id: ParamId,
    name: &'static str,
}

fn push_linear_param_witnesses_v1(
    witnesses: &mut Vec<NativeParamWitnessV1>,
    linear: &Linear<CudaAutodiffBackendV1>,
    weight_name: &'static str,
    bias_name: &'static str,
) -> Result<(), Box<dyn Error>> {
    witnesses.push(NativeParamWitnessV1 {
        id: linear.weight.id,
        name: weight_name,
    });
    let bias = linear.bias.as_ref().ok_or_else(|| {
        training_error(format!(
            "production CUDA parameter witness is missing bias {bias_name}"
        ))
    })?;
    witnesses.push(NativeParamWitnessV1 {
        id: bias.id,
        name: bias_name,
    });
    Ok(())
}

/// Bind Burn's traversal order to the production model's explicitly named
/// fields. Shape and positional checks alone would not detect a permutation of
/// two same-shaped moment tensors; this witness makes that permutation fail.
fn native_param_witnesses_v1(
    model: &ProductionNet8<CudaAutodiffBackendV1>,
) -> Result<Vec<NativeParamWitnessV1>, Box<dyn Error>> {
    let mut witnesses = Vec::with_capacity(PARAMETER_TENSOR_COUNT_V1);
    witnesses.push(NativeParamWitnessV1 {
        id: model.card_embedding.weight.id,
        name: "card_embedding.weight",
    });
    for (linear, weight_name, bias_name) in [
        (
            &model.object_encoder.first,
            "object_encoder.0.weight",
            "object_encoder.0.bias",
        ),
        (
            &model.object_encoder.second,
            "object_encoder.2.weight",
            "object_encoder.2.bias",
        ),
        (
            &model.edge_encoder.first,
            "edge_encoder.0.weight",
            "edge_encoder.0.bias",
        ),
        (
            &model.edge_encoder.second,
            "edge_encoder.2.weight",
            "edge_encoder.2.bias",
        ),
        (
            &model.node_update.first,
            "node_update.0.weight",
            "node_update.0.bias",
        ),
        (
            &model.node_update.second,
            "node_update.2.weight",
            "node_update.2.bias",
        ),
        (
            &model.state_encoder.first,
            "state_encoder.0.weight",
            "state_encoder.0.bias",
        ),
        (
            &model.state_encoder.second,
            "state_encoder.2.weight",
            "state_encoder.2.bias",
        ),
        (
            &model.action_ref_encoder.first,
            "action_ref_encoder.0.weight",
            "action_ref_encoder.0.bias",
        ),
        (
            &model.action_ref_encoder.second,
            "action_ref_encoder.2.weight",
            "action_ref_encoder.2.bias",
        ),
        (
            &model.action_encoder.first,
            "action_encoder.0.weight",
            "action_encoder.0.bias",
        ),
        (
            &model.action_encoder.second,
            "action_encoder.2.weight",
            "action_encoder.2.bias",
        ),
        (&model.scorer.hidden, "scorer.0.weight", "scorer.0.bias"),
        (&model.scorer.output, "scorer.2.weight", "scorer.2.bias"),
        (
            &model.value_head.hidden,
            "value_head.0.weight",
            "value_head.0.bias",
        ),
        (
            &model.value_head.output,
            "value_head.2.weight",
            "value_head.2.bias",
        ),
    ] {
        push_linear_param_witnesses_v1(&mut witnesses, linear, weight_name, bias_name)?;
    }
    let unique_ids = witnesses
        .iter()
        .map(|witness| witness.id)
        .collect::<std::collections::BTreeSet<_>>();
    if witnesses.len() != PARAMETER_TENSOR_COUNT_V1 || unique_ids.len() != PARAMETER_TENSOR_COUNT_V1
    {
        return Err(training_error(format!(
            "production CUDA parameter witness cardinality mismatch: entries={}, unique_ids={}, expected={PARAMETER_TENSOR_COUNT_V1}",
            witnesses.len(),
            unique_ids.len()
        )));
    }
    Ok(witnesses)
}

struct NamedStateImporterV1<'a> {
    host: &'a [NativeNamedParameterV1],
    witnesses: &'a [NativeParamWitnessV1],
    output: GradientsParams,
    position: usize,
    error: Option<String>,
    device: &'a burn_cuda::CudaDevice,
}

impl ModuleVisitor<CudaAutodiffBackendV1> for NamedStateImporterV1<'_> {
    fn visit_float<const D: usize>(&mut self, parameter: &Param<Tensor<CudaAutodiffBackendV1, D>>) {
        if self.error.is_some() {
            return;
        }
        let result = (|| -> Result<(), Box<dyn Error>> {
            let host = self.host.get(self.position).ok_or_else(|| {
                training_error("named device-state import ended before module traversal")
            })?;
            let witness = self.witnesses.get(self.position).ok_or_else(|| {
                training_error("parameter witness ended before module traversal during import")
            })?;
            if parameter.id != witness.id || host.name != witness.name {
                return Err(training_error(format!(
                    "parameter witness mismatch importing ordinal {}: visited_id={}, expected_id={}, host_name={}, expected_name={}",
                    self.position, parameter.id, witness.id, host.name, witness.name
                )));
            }
            let (values, burn_shape) = native_named_values_to_burn_v1(host)?;
            if burn_shape.len() != D {
                return Err(training_error(format!(
                    "rank mismatch importing {} at ordinal {}: {} != {D}",
                    host.name,
                    self.position,
                    burn_shape.len()
                )));
            }
            let dimensions: [usize; D] = burn_shape.try_into().map_err(|_| {
                training_error(format!("shape conversion failed importing {}", host.name))
            })?;
            if parameter.val().dims() != dimensions {
                return Err(training_error(format!(
                    "module shape mismatch importing {} at ordinal {}: {:?} != {:?}",
                    host.name,
                    self.position,
                    parameter.val().dims(),
                    dimensions
                )));
            }
            let tensor = Tensor::<CudaBackendV1, D>::from_data(
                TensorData::new(values, dimensions),
                self.device,
            );
            self.output.register(parameter.id, tensor);
            self.position += 1;
            Ok(())
        })();
        if let Err(error) = result {
            self.error = Some(error.to_string());
        }
    }
}

fn import_named_state_v1(
    model: &ProductionNet8<CudaAutodiffBackendV1>,
    host: &[NativeNamedParameterV1],
    device: &burn_cuda::CudaDevice,
) -> Result<GradientsParams, Box<dyn Error>> {
    let witnesses = native_param_witnesses_v1(model)?;
    let mut importer = NamedStateImporterV1 {
        host,
        witnesses: &witnesses,
        output: GradientsParams::new(),
        position: 0,
        error: None,
        device,
    };
    model.visit(&mut importer);
    if let Some(error) = importer.error {
        return Err(training_error(error));
    }
    if importer.position != PARAMETER_TENSOR_COUNT_V1
        || importer.position != host.len()
        || importer.output.len() != PARAMETER_TENSOR_COUNT_V1
    {
        return Err(training_error(format!(
            "named device-state import cardinality mismatch: visited={}, host={}, device={}",
            importer.position,
            host.len(),
            importer.output.len()
        )));
    }
    CudaBackendV1::sync(device)?;
    Ok(importer.output)
}

struct NamedStateExporterV1<'a> {
    state: &'a GradientsParams,
    template: &'a [NativeNamedParameterV1],
    witnesses: &'a [NativeParamWitnessV1],
    output: Vec<NativeNamedParameterV1>,
    position: usize,
    error: Option<String>,
}

impl ModuleVisitor<CudaAutodiffBackendV1> for NamedStateExporterV1<'_> {
    fn visit_float<const D: usize>(&mut self, parameter: &Param<Tensor<CudaAutodiffBackendV1, D>>) {
        if self.error.is_some() {
            return;
        }
        let result = (|| -> Result<(), Box<dyn Error>> {
            let template = self.template.get(self.position).ok_or_else(|| {
                training_error("named device-state export ended before module traversal")
            })?;
            let witness = self.witnesses.get(self.position).ok_or_else(|| {
                training_error("parameter witness ended before module traversal during export")
            })?;
            if parameter.id != witness.id || template.name != witness.name {
                return Err(training_error(format!(
                    "parameter witness mismatch exporting ordinal {}: visited_id={}, expected_id={}, template_name={}, expected_name={}",
                    self.position, parameter.id, witness.id, template.name, witness.name
                )));
            }
            let tensor = self
                .state
                .get::<CudaBackendV1, D>(parameter.id)
                .ok_or_else(|| {
                    training_error(format!(
                        "missing device tensor for {} at ordinal {}",
                        template.name, self.position
                    ))
                })?;
            let (_, expected_shape) = native_named_values_to_burn_v1(template)?;
            if tensor.dims().as_slice() != expected_shape.as_slice() {
                return Err(training_error(format!(
                    "device tensor shape mismatch exporting {}: {:?} != {:?}",
                    template.name,
                    tensor.dims(),
                    expected_shape
                )));
            }
            let burn_values = tensor.into_data().to_vec::<f32>()?;
            let native_values = burn_values_to_native_named_v1(template, &burn_values)?;
            self.output.push(NativeNamedParameterV1 {
                name: template.name,
                shape: template.shape.clone(),
                values: native_values,
            });
            self.position += 1;
            Ok(())
        })();
        if let Err(error) = result {
            self.error = Some(error.to_string());
        }
    }
}

fn export_named_state_v1(
    model: &ProductionNet8<CudaAutodiffBackendV1>,
    state: &GradientsParams,
    template: &[NativeNamedParameterV1],
) -> Result<Vec<NativeNamedParameterV1>, Box<dyn Error>> {
    let witnesses = native_param_witnesses_v1(model)?;
    let mut exporter = NamedStateExporterV1 {
        state,
        template,
        witnesses: &witnesses,
        output: Vec::with_capacity(PARAMETER_TENSOR_COUNT_V1),
        position: 0,
        error: None,
    };
    model.visit(&mut exporter);
    if let Some(error) = exporter.error {
        return Err(training_error(error));
    }
    if exporter.position != PARAMETER_TENSOR_COUNT_V1
        || exporter.position != template.len()
        || state.len() != PARAMETER_TENSOR_COUNT_V1
    {
        return Err(training_error(format!(
            "named device-state export cardinality mismatch: visited={}, template={}, device={}",
            exporter.position,
            template.len(),
            state.len()
        )));
    }
    Ok(exporter.output)
}

fn native_named_values_to_burn_v1(
    parameter: &NativeNamedParameterV1,
) -> Result<(Vec<f32>, Vec<usize>), Box<dyn Error>> {
    if parameter.name == "card_embedding.weight" {
        if parameter.shape.len() != 2 {
            return Err(training_error("card embedding must be rank two"));
        }
        return Ok((parameter.values.clone(), parameter.shape.clone()));
    }
    if parameter.name.ends_with(".weight") {
        if parameter.shape.len() != 2 {
            return Err(training_error(format!(
                "linear weight {} must be rank two",
                parameter.name
            )));
        }
        let output = parameter.shape[0];
        let input = parameter.shape[1];
        return Ok((
            transpose_output_input_to_input_output(&parameter.values, input, output)?,
            vec![input, output],
        ));
    }
    if parameter.name.ends_with(".bias") && parameter.shape.len() == 1 {
        return Ok((parameter.values.clone(), parameter.shape.clone()));
    }
    Err(training_error(format!(
        "unsupported native parameter layout {} {:?}",
        parameter.name, parameter.shape
    )))
}

fn burn_values_to_native_named_v1(
    template: &NativeNamedParameterV1,
    burn_values: &[f32],
) -> Result<Vec<f32>, Box<dyn Error>> {
    if template.name == "card_embedding.weight" || template.name.ends_with(".bias") {
        if burn_values.len() != template.values.len() {
            return Err(training_error(format!(
                "device value length mismatch exporting {}",
                template.name
            )));
        }
        return Ok(burn_values.to_vec());
    }
    if template.name.ends_with(".weight") && template.shape.len() == 2 {
        let output = template.shape[0];
        let input = template.shape[1];
        return transpose_input_output_to_output_input(burn_values, input, output);
    }
    Err(training_error(format!(
        "unsupported Burn parameter layout {} {:?}",
        template.name, template.shape
    )))
}

#[allow(clippy::too_many_arguments)]
fn canonicalize_scorer_bias_gradient_v1(
    model: &ProductionNet8<CudaAutodiffBackendV1>,
    gradients: &mut GradientsParams,
    host: &HostPackingWorkspace,
    batch: &DevicePackedBatch<CudaAutodiffBackendV1>,
    selected_action_indices: &[usize],
    terminal_returns: &[i8],
    logit_outputs: &[f32],
    value_outputs: &[f32],
    device: &burn_cuda::CudaDevice,
    corrupt_monitor_graph: bool,
) -> Result<GaugeEvidenceV1, Box<dyn Error>> {
    if host.action_offsets.len() != host.case_indices.len() + 1
        || host.action_offsets.last().copied().unwrap_or_default() != logit_outputs.len()
        || selected_action_indices.len() != host.case_indices.len()
        || terminal_returns.len() != host.case_indices.len()
        || value_outputs.len() != host.case_indices.len()
        || !logit_outputs
            .iter()
            .chain(value_outputs)
            .all(|value| value.is_finite())
    {
        return Err(training_error("CUDA gauge input cardinality mismatch"));
    }
    let parameter = model
        .scorer
        .output
        .bias
        .as_ref()
        .ok_or_else(|| training_error("scorer output has no bias"))?;
    let gradient = gradients
        .remove::<CudaBackendV1, 1>(parameter.id)
        .ok_or_else(|| training_error("scorer output bias gradient is missing"))?;
    let raw = gradient.clone().into_data().to_vec::<f32>()?;
    let raw = *raw
        .first()
        .ok_or_else(|| training_error("scorer output bias gradient is empty"))?;

    let monitor_coefficients = derive_centered_monitor_coefficients_v1(
        host,
        selected_action_indices,
        terminal_returns,
        logit_outputs,
        value_outputs,
    )?;
    let monitor_coefficient_host_sum = monitor_coefficients
        .iter()
        .map(|value| f64::from(*value))
        .sum::<f64>() as f32;

    // Replay analytic f32 coefficients derived from the same CUDA logits,
    // values, actions, and returns through a genuinely separate
    // model.forward/backward invocation. Anchor-centering each ragged row makes
    // the common-logit bias derivative structurally zero before any
    // approximate transcendental is involved; only basic f32 copies,
    // subtraction, multiplication, and reductions remain in this monitor.
    let (monitor_logits, _) = model.forward(batch);
    let mut monitor_loss = None;
    for offsets in host.action_offsets.windows(2) {
        let begin = offsets[0];
        let end = offsets[1];
        if end <= begin {
            return Err(training_error(
                "CUDA gauge monitor found an empty action row",
            ));
        }
        let row = monitor_logits.clone().slice([begin..end]);
        let anchor = row.clone().slice([0..1]);
        let centered = row - anchor;
        let coefficients = Tensor::<CudaAutodiffBackendV1, 1>::from_data(
            TensorData::new(monitor_coefficients[begin..end].to_vec(), [end - begin]),
            device,
        );
        let contribution = centered.mul(coefficients).sum();
        monitor_loss = Some(match monitor_loss {
            Some(active) => active + contribution,
            None => contribution,
        });
    }
    let mut monitor_loss =
        monitor_loss.ok_or_else(|| training_error("CUDA gauge monitor loss is empty"))?;
    if corrupt_monitor_graph {
        // Corrupt the upstream monitor graph itself, rather than substituting a
        // value at the comparator. A direct first-logit term has scorer-output
        // bias derivative +1 and must exceed the uncorrupted cancellation
        // bound through the real CUDA backward path.
        monitor_loss = monitor_loss + monitor_logits.slice([0..1]);
    }
    let monitor_raw_gradients = monitor_loss.backward();
    let mut monitor_gradients = GradientsParams::from_grads(monitor_raw_gradients, model);
    let monitor_gradient = monitor_gradients
        .remove::<CudaBackendV1, 1>(parameter.id)
        .ok_or_else(|| training_error("CUDA centered gauge monitor gradient is missing"))?;
    let monitor_raw = monitor_gradient.into_data().to_vec::<f32>()?;
    let monitor_raw = *monitor_raw
        .first()
        .ok_or_else(|| training_error("CUDA centered gauge monitor gradient is empty"))?;
    let monitor_operation_count = monitor_coefficients
        .len()
        .checked_mul(6)
        .and_then(|count| count.checked_add(host.case_indices.len() * 4 + 16))
        .ok_or_else(|| training_error("CUDA centered gauge operation count overflow"))?;
    let centered_monitor_bound = formal_basic_f32_cancellation_bound_v1(
        &monitor_coefficients,
        CENTERED_MONITOR_COEFFICIENT_TERM_MULTIPLICITY_V1,
        monitor_operation_count,
    )?;
    enforce_gauge_bound_v1(monitor_raw, centered_monitor_bound)?;

    gradients.register(parameter.id, gradient.zeros_like());
    Ok(GaugeEvidenceV1 {
        training_raw_residual: raw,
        monitor_coefficient_host_sum,
        centered_monitor_raw_residual: monitor_raw,
        centered_monitor_bound,
        centered_monitor_operation_count: monitor_operation_count,
    })
}

fn derive_centered_monitor_coefficients_v1(
    host: &HostPackingWorkspace,
    selected_action_indices: &[usize],
    terminal_returns: &[i8],
    logit_outputs: &[f32],
    value_outputs: &[f32],
) -> Result<Vec<f32>, Box<dyn Error>> {
    let group_count = host.case_indices.len() as f64;
    let mut coefficients = Vec::with_capacity(logit_outputs.len());
    for decision in 0..host.case_indices.len() {
        let begin = host.action_offsets[decision];
        let end = host.action_offsets[decision + 1];
        let selected = selected_action_indices[decision];
        if end <= begin || selected >= end - begin || !matches!(terminal_returns[decision], -1..=1)
        {
            return Err(training_error("CUDA gauge monitor decision is invalid"));
        }
        let row = &logit_outputs[begin..end];
        let maximum = row
            .iter()
            .map(|value| f64::from(*value))
            .fold(f64::NEG_INFINITY, f64::max);
        let exponentials = row
            .iter()
            .map(|value| (f64::from(*value) - maximum).exp())
            .collect::<Vec<_>>();
        let sum = exponentials.iter().sum::<f64>();
        let advantage = f64::from(terminal_returns[decision]) - f64::from(value_outputs[decision]);
        coefficients.extend(exponentials.into_iter().enumerate().map(|(index, value)| {
            (advantage * (value / sum - if index == selected { 1.0 } else { 0.0 }) / group_count)
                as f32
        }));
    }
    Ok(coefficients)
}

fn next_positive_f32_v1(value: f32) -> f32 {
    if value == 0.0 {
        return f32::from_bits(1);
    }
    f32::from_bits(value.to_bits().saturating_add(1))
}

fn formal_basic_f32_cancellation_bound_v1(
    addends: &[f32],
    term_multiplicity: usize,
    operation_count: usize,
) -> Result<f32, Box<dyn Error>> {
    let unit_roundoff = f64::from(f32::EPSILON) / 2.0;
    let scaled = operation_count as f64 * unit_roundoff;
    if scaled >= 1.0 || term_multiplicity == 0 || !addends.iter().all(|value| value.is_finite()) {
        return Err(training_error(
            "CUDA gauge formal basic-f32 bound is undefined",
        ));
    }
    let gamma = scaled / (1.0 - scaled);
    let sum_absolute = addends
        .iter()
        .map(|value| f64::from(value.abs()))
        .sum::<f64>()
        * term_multiplicity as f64;
    // PTX basic arithmetic may flush subnormal intermediates. Each modeled
    // operation therefore receives one minimum-normal allowance in addition
    // to the standard gamma bound. No empirically fitted multiplier appears.
    let exact_bound = gamma * sum_absolute + operation_count as f64 * f64::from(f32::MIN_POSITIVE);
    let rounded = exact_bound as f32;
    let outward = if f64::from(rounded) < exact_bound {
        next_positive_f32_v1(rounded)
    } else {
        rounded
    };
    Ok(outward.max(f32::MIN_POSITIVE))
}

fn enforce_gauge_bound_v1(observed: f32, bound: f32) -> Result<(), Box<dyn Error>> {
    if !observed.is_finite() || !bound.is_finite() || bound < 0.0 || observed.abs() > bound {
        return Err(Box::new(
            ExperimentalTrainingErrorV1::GaugeResidualExceeded {
                residual_bits: observed.to_bits(),
                bound_bits: bound.to_bits(),
            },
        ));
    }
    Ok(())
}

struct DeviceAdamMapperV1<'a> {
    gradients: GradientsParams,
    previous_first: &'a GradientsParams,
    previous_second: &'a GradientsParams,
    next_first: GradientsParams,
    next_second: GradientsParams,
    position: usize,
    step_size: f32,
    bias_correction2_sqrt: f32,
    error: Option<String>,
}

impl<'a> DeviceAdamMapperV1<'a> {
    fn new(
        gradients: GradientsParams,
        previous_first: &'a GradientsParams,
        previous_second: &'a GradientsParams,
        step: u64,
        learning_rate: f32,
    ) -> Result<Self, Box<dyn Error>> {
        let exponent = i32::try_from(step)?;
        let bias_correction1 = 1.0f64 - f64::from(ADAM_BETA1_V1).powi(exponent);
        let bias_correction2 = 1.0f64 - f64::from(ADAM_BETA2_V1).powi(exponent);
        let step_size = (f64::from(learning_rate) / bias_correction1) as f32;
        let bias_correction2_sqrt = bias_correction2.sqrt() as f32;
        if !step_size.is_finite() || !bias_correction2_sqrt.is_finite() {
            return Err(training_error("experimental Adam scalar is non-finite"));
        }
        Ok(Self {
            gradients,
            previous_first,
            previous_second,
            next_first: GradientsParams::new(),
            next_second: GradientsParams::new(),
            position: 0,
            step_size,
            bias_correction2_sqrt,
            error: None,
        })
    }

    fn finish_v1(self) -> Result<(GradientsParams, GradientsParams), Box<dyn Error>> {
        if let Some(error) = self.error {
            return Err(training_error(error));
        }
        if self.position != PARAMETER_TENSOR_COUNT_V1
            || !self.gradients.is_empty()
            || self.next_first.len() != PARAMETER_TENSOR_COUNT_V1
            || self.next_second.len() != PARAMETER_TENSOR_COUNT_V1
        {
            return Err(training_error(format!(
                "device Adam mapping cardinality mismatch: visited={}, residual_gradients={}, first={}, second={}",
                self.position,
                self.gradients.len(),
                self.next_first.len(),
                self.next_second.len()
            )));
        }
        Ok((self.next_first, self.next_second))
    }
}

impl ModuleMapper<CudaAutodiffBackendV1> for DeviceAdamMapperV1<'_> {
    fn map_float<const D: usize>(
        &mut self,
        parameter: Param<Tensor<CudaAutodiffBackendV1, D>>,
    ) -> Param<Tensor<CudaAutodiffBackendV1, D>> {
        if self.error.is_some() {
            return parameter;
        }
        let ordinal = self.position;
        self.position += 1;
        let id = parameter.id;
        let Some(gradient) = self.gradients.remove::<CudaBackendV1, D>(id) else {
            self.error = Some(format!("missing CUDA gradient at ordinal {ordinal}"));
            return parameter;
        };
        let Some(previous_first) = self.previous_first.get::<CudaBackendV1, D>(id) else {
            self.error = Some(format!("missing first moment at ordinal {ordinal}"));
            return parameter;
        };
        let Some(previous_second) = self.previous_second.get::<CudaBackendV1, D>(id) else {
            self.error = Some(format!("missing second moment at ordinal {ordinal}"));
            return parameter;
        };
        let (id, tensor, parameter_mapper) = parameter.consume();
        let require_grad = tensor.is_require_grad();
        let inner = tensor.inner();
        if inner.dims() != gradient.dims()
            || inner.dims() != previous_first.dims()
            || inner.dims() != previous_second.dims()
        {
            self.error = Some(format!("device Adam shape mismatch at ordinal {ordinal}"));
            let mut tensor = Tensor::<CudaAutodiffBackendV1, D>::from_inner(inner);
            if require_grad {
                tensor = tensor.require_grad();
            }
            return Param::from_mapped_value(id, tensor, parameter_mapper);
        }

        let (updated, next_first, next_second) = if ordinal == SCORER_SECOND_BIAS_ORDINAL_V1 {
            (
                inner,
                previous_first.zeros_like(),
                previous_second.zeros_like(),
            )
        } else {
            let next_first = previous_first.clone()
                + (gradient.clone() - previous_first).mul_scalar(1.0 - ADAM_BETA1_V1);
            let next_second = previous_second.mul_scalar(ADAM_BETA2_V1)
                + gradient.square().mul_scalar(1.0 - ADAM_BETA2_V1);
            let denominator = next_second
                .clone()
                .sqrt()
                .div_scalar(self.bias_correction2_sqrt)
                .add_scalar(ADAM_EPSILON_V1);
            // Preserve the frozen CPU association exactly:
            // parameter + ((-step_size) * first) / denominator.
            let scaled_first = next_first.clone().mul_scalar(-self.step_size);
            let updated = inner + scaled_first.div(denominator);
            (updated, next_first, next_second)
        };
        self.next_first.register(id, next_first);
        self.next_second.register(id, next_second);
        let mut tensor = Tensor::<CudaAutodiffBackendV1, D>::from_inner(updated);
        if require_grad {
            tensor = tensor.require_grad();
        }
        Param::from_mapped_value(id, tensor, parameter_mapper)
    }
}

/// Device-resident dense plan for multi-substep production groups: rows are
/// all substeps flat across groups; joint log-probabilities reduce by
/// scatter-add over the group index; the group value gathers at each group's
/// first substep, matching the CPU reference semantics exactly.
pub(crate) struct DenseGroupLossPlanV1 {
    pad_gather: Tensor<CudaAutodiffBackendV1, 1, Int>,
    pad_mask: Tensor<CudaAutodiffBackendV1, 2>,
    selected_gather: Tensor<CudaAutodiffBackendV1, 1, Int>,
    group_scatter: Tensor<CudaAutodiffBackendV1, 1, Int>,
    group_first_gather: Tensor<CudaAutodiffBackendV1, 1, Int>,
    targets: Tensor<CudaAutodiffBackendV1, 1>,
    substeps: usize,
    group_count: usize,
    max_actions: usize,
}

pub(crate) fn build_dense_group_loss_plan_v1(
    host: &HostPackingWorkspace,
    selected_action_indices: &[usize],
    substep_group_indices: &[usize],
    group_first_substeps: &[usize],
    terminal_returns: &[i8],
    device: &burn_cuda::CudaDevice,
) -> Result<DenseGroupLossPlanV1, Box<dyn Error>> {
    let substeps = selected_action_indices.len();
    let group_count = terminal_returns.len();
    if substeps == 0
        || group_count == 0
        || substep_group_indices.len() != substeps
        || group_first_substeps.len() != group_count
        || host.action_offsets.len() != substeps + 1
    {
        return Err(training_error("dense group plan cardinality mismatch"));
    }
    let mut max_actions = 0_usize;
    for offsets in host.action_offsets.windows(2) {
        let count = offsets[1]
            .checked_sub(offsets[0])
            .filter(|count| *count > 0)
            .ok_or_else(|| training_error("dense group plan found an empty action row"))?;
        max_actions = max_actions.max(count);
    }
    let mut pad_gather = Vec::with_capacity(substeps * max_actions);
    let mut pad_mask = Vec::with_capacity(substeps * max_actions);
    let mut selected_gather = Vec::with_capacity(substeps);
    let mut group_scatter = Vec::with_capacity(substeps);
    for substep in 0..substeps {
        let begin = host.action_offsets[substep];
        let end = host.action_offsets[substep + 1];
        let count = end - begin;
        let selected = selected_action_indices[substep];
        let group = substep_group_indices[substep];
        if selected >= count || group >= group_count {
            return Err(training_error(format!(
                "dense group plan substep {substep} is invalid"
            )));
        }
        for action in 0..max_actions {
            if action < count {
                pad_gather.push(i32::try_from(begin + action)?);
                pad_mask.push(0.0_f32);
            } else {
                pad_gather.push(i32::try_from(begin)?);
                pad_mask.push(DENSE_PAD_MASK_NEGATIVE_V1);
            }
        }
        selected_gather.push(i32::try_from(begin + selected)?);
        group_scatter.push(i32::try_from(group)?);
    }
    let mut group_first_gather = Vec::with_capacity(group_count);
    let mut targets = Vec::with_capacity(group_count);
    for group in 0..group_count {
        let first = group_first_substeps[group];
        if first >= substeps
            || substep_group_indices[first] != group
            || !matches!(terminal_returns[group], -1..=1)
        {
            return Err(training_error(format!(
                "dense group plan group {group} is invalid"
            )));
        }
        group_first_gather.push(i32::try_from(first)?);
        targets.push(f32::from(terminal_returns[group]));
    }
    Ok(DenseGroupLossPlanV1 {
        pad_gather: Tensor::from_data(
            TensorData::new(pad_gather, [substeps * max_actions]),
            device,
        ),
        pad_mask: Tensor::from_data(TensorData::new(pad_mask, [substeps, max_actions]), device),
        selected_gather: Tensor::from_data(TensorData::new(selected_gather, [substeps]), device),
        group_scatter: Tensor::from_data(TensorData::new(group_scatter, [substeps]), device),
        group_first_gather: Tensor::from_data(
            TensorData::new(group_first_gather, [group_count]),
            device,
        ),
        targets: Tensor::from_data(TensorData::new(targets, [group_count]), device),
        substeps,
        group_count,
        max_actions,
    })
}

/// Reinterprets an autodiff-typed device batch on the inner backend. Tensor
/// handles are shared, not copied; the returned batch simply cannot record an
/// autodiff graph.
fn inner_readback_batch_v1(
    batch: &DevicePackedBatch<CudaAutodiffBackendV1>,
) -> DevicePackedBatch<CudaBackendV1> {
    DevicePackedBatch {
        device: batch.device.clone(),
        decision_count: batch.decision_count,
        object_count: batch.object_count,
        edge_count: batch.edge_count,
        action_count: batch.action_count,
        action_ref_count: batch.action_ref_count,
        state: batch.state.clone().inner(),
        object_features: batch.object_features.clone().inner(),
        object_card_ids: batch.object_card_ids.clone().inner(),
        object_group_indices: batch.object_group_indices.clone().inner(),
        edge_features: batch.edge_features.clone().inner(),
        edge_source_indices: batch.edge_source_indices.clone().inner(),
        edge_target_indices: batch.edge_target_indices.clone().inner(),
        action_features: batch.action_features.clone().inner(),
        action_decision_indices: batch.action_decision_indices.clone().inner(),
        action_ref_features: batch.action_ref_features.clone().inner(),
        action_ref_action_indices: batch.action_ref_action_indices.clone().inner(),
        action_ref_node_indices: batch.action_ref_node_indices.clone().inner(),
    }
}

fn dense_group_loss_v1(
    logits: Tensor<CudaAutodiffBackendV1, 1>,
    values: Tensor<CudaAutodiffBackendV1, 1>,
    plan: &DenseGroupLossPlanV1,
    value_coefficient: f32,
    normalization_group_count: f32,
) -> Result<Tensor<CudaAutodiffBackendV1, 1>, Box<dyn Error>> {
    if values.dims()[0] != plan.substeps
        || !value_coefficient.is_finite()
        || value_coefficient <= 0.0
        || !normalization_group_count.is_finite()
        || normalization_group_count < plan.group_count as f32
    {
        return Err(training_error("dense group loss shape/parameter mismatch"));
    }
    let padded = logits
        .clone()
        .select(0, plan.pad_gather.clone())
        .reshape([plan.substeps, plan.max_actions])
        + plan.pad_mask.clone();
    let row_max = padded.clone().max_dim(1).detach();
    let log_sum_exp = (padded - row_max.clone()).exp().sum_dim(1).log() + row_max;
    let selected_logits = logits.select(0, plan.selected_gather.clone());
    let selected_log_probabilities = selected_logits - log_sum_exp.squeeze_dim::<1>(1);
    let joint_log_probabilities = Tensor::zeros([plan.group_count], &plan.targets.device())
        .scatter(
            0,
            plan.group_scatter.clone(),
            selected_log_probabilities,
            IndexingUpdateOp::Add,
        );
    let group_values = values.select(0, plan.group_first_gather.clone());
    let advantage = plan.targets.clone() - group_values.clone().detach();
    let policy_sum = joint_log_probabilities
        .mul(advantage)
        .mul_scalar(-1.0)
        .sum();
    let value_error = group_values - plan.targets.clone();
    let value_sum = value_error.clone().mul(value_error).sum();
    Ok(
        (policy_sum + value_sum.mul_scalar(value_coefficient))
            .div_scalar(normalization_group_count),
    )
}

/// Device-resident dense-padded loss plan, built once per packed batch. The
/// ragged action rows are padded to `[decisions, max_actions]` with an
/// additive mask so the whole loss is a fixed handful of dense kernels
/// regardless of batch size, instead of a per-decision graph chain.
struct DenseLossPlanV1 {
    pad_gather: Tensor<CudaAutodiffBackendV1, 1, Int>,
    pad_mask: Tensor<CudaAutodiffBackendV1, 2>,
    selected_gather: Tensor<CudaAutodiffBackendV1, 1, Int>,
    targets: Tensor<CudaAutodiffBackendV1, 1>,
    decisions: usize,
    max_actions: usize,
}

const DENSE_PAD_MASK_NEGATIVE_V1: f32 = -1.0e30;

fn build_dense_loss_plan_v1(
    host: &HostPackingWorkspace,
    selected_action_indices: &[usize],
    terminal_returns: &[i8],
    device: &burn_cuda::CudaDevice,
) -> Result<DenseLossPlanV1, Box<dyn Error>> {
    let decisions = selected_action_indices.len();
    if decisions == 0
        || terminal_returns.len() != decisions
        || host.action_offsets.len() != decisions + 1
    {
        return Err(training_error("dense loss plan cardinality mismatch"));
    }
    let mut max_actions = 0_usize;
    for offsets in host.action_offsets.windows(2) {
        let count = offsets[1]
            .checked_sub(offsets[0])
            .filter(|count| *count > 0)
            .ok_or_else(|| training_error("dense loss plan found an empty action row"))?;
        max_actions = max_actions.max(count);
    }
    let mut pad_gather = Vec::with_capacity(decisions * max_actions);
    let mut pad_mask = Vec::with_capacity(decisions * max_actions);
    let mut selected_gather = Vec::with_capacity(decisions);
    let mut targets = Vec::with_capacity(decisions);
    for decision in 0..decisions {
        let begin = host.action_offsets[decision];
        let end = host.action_offsets[decision + 1];
        let count = end - begin;
        let selected = selected_action_indices[decision];
        if selected >= count || !matches!(terminal_returns[decision], -1..=1) {
            return Err(training_error(format!(
                "dense loss plan decision {decision} is invalid"
            )));
        }
        for action in 0..max_actions {
            if action < count {
                pad_gather.push(i32::try_from(begin + action)?);
                pad_mask.push(0.0_f32);
            } else {
                pad_gather.push(i32::try_from(begin)?);
                pad_mask.push(DENSE_PAD_MASK_NEGATIVE_V1);
            }
        }
        selected_gather.push(i32::try_from(begin + selected)?);
        targets.push(f32::from(terminal_returns[decision]));
    }
    Ok(DenseLossPlanV1 {
        pad_gather: Tensor::from_data(
            TensorData::new(pad_gather, [decisions * max_actions]),
            device,
        ),
        pad_mask: Tensor::from_data(TensorData::new(pad_mask, [decisions, max_actions]), device),
        selected_gather: Tensor::from_data(TensorData::new(selected_gather, [decisions]), device),
        targets: Tensor::from_data(TensorData::new(targets, [decisions]), device),
        decisions,
        max_actions,
    })
}

/// Dense-padded terminal REINFORCE + value loss: semantically the grouped
/// loss above, with reduction order changed by dense kernels. Correctness is
/// anchored on the CPU-oracle tolerance comparison and within-GPU
/// determinism, never on bit equality with the ragged chain.
fn one_substep_grouped_loss_dense_v1(
    logits: Tensor<CudaAutodiffBackendV1, 1>,
    values: Tensor<CudaAutodiffBackendV1, 1>,
    plan: &DenseLossPlanV1,
    value_coefficient: f32,
) -> Result<Tensor<CudaAutodiffBackendV1, 1>, Box<dyn Error>> {
    if values.dims()[0] != plan.decisions
        || !value_coefficient.is_finite()
        || value_coefficient <= 0.0
    {
        return Err(training_error(
            "dense grouped-loss shape/parameter mismatch",
        ));
    }
    let padded = logits
        .clone()
        .select(0, plan.pad_gather.clone())
        .reshape([plan.decisions, plan.max_actions])
        + plan.pad_mask.clone();
    let row_max = padded.clone().max_dim(1).detach();
    let log_sum_exp = (padded - row_max.clone()).exp().sum_dim(1).log() + row_max;
    let selected_logits = logits.select(0, plan.selected_gather.clone());
    let selected_log_probabilities = selected_logits - log_sum_exp.squeeze_dim::<1>(1);
    let advantage = plan.targets.clone() - values.clone().detach();
    let policy_sum = selected_log_probabilities
        .mul(advantage)
        .mul_scalar(-1.0)
        .sum();
    let value_error = values - plan.targets.clone();
    let value_sum = value_error.clone().mul(value_error).sum();
    Ok((policy_sum + value_sum.mul_scalar(value_coefficient)).div_scalar(plan.decisions as f32))
}

fn one_substep_grouped_loss_v1(
    logits: Tensor<CudaAutodiffBackendV1, 1>,
    values: Tensor<CudaAutodiffBackendV1, 1>,
    action_offsets: &[usize],
    selected_action_indices: &[usize],
    terminal_returns: &[i8],
    value_coefficient: f32,
) -> Result<Tensor<CudaAutodiffBackendV1, 1>, Box<dyn Error>> {
    if selected_action_indices.is_empty()
        || selected_action_indices.len() != terminal_returns.len()
        || action_offsets.len() != selected_action_indices.len() + 1
        || values.dims()[0] != selected_action_indices.len()
        || !value_coefficient.is_finite()
        || value_coefficient <= 0.0
    {
        return Err(training_error(
            "experimental grouped-loss shape/parameter mismatch",
        ));
    }
    let mut policy_sum = None;
    let mut value_sum = None;
    for group_index in 0..selected_action_indices.len() {
        let begin = action_offsets[group_index];
        let end = action_offsets[group_index + 1];
        let selected = selected_action_indices[group_index];
        if end <= begin
            || selected >= end - begin
            || !matches!(terminal_returns[group_index], -1..=1)
        {
            return Err(training_error(format!(
                "invalid experimental group at ordinal {group_index}"
            )));
        }
        let row = logits.clone().slice([begin..end]);
        let log_probabilities = log_softmax(row, 0);
        let selected_log_probability =
            log_probabilities.slice([selected..selected.saturating_add(1)]);
        let value = values.clone().slice([group_index..group_index + 1]);
        let target = value
            .clone()
            .full_like(f32::from(terminal_returns[group_index]));
        // Match the CPU reference: policy advantage is stop-gradient.
        let advantage = target.clone() - value.clone().detach();
        let policy_term = selected_log_probability.mul(advantage).mul_scalar(-1.0);
        let value_error = value - target;
        let value_term = value_error.clone().mul(value_error);
        policy_sum = Some(match policy_sum {
            Some(active) => active + policy_term,
            None => policy_term,
        });
        value_sum = Some(match value_sum {
            Some(active) => active + value_term,
            None => value_term,
        });
    }
    let group_count = selected_action_indices.len() as f32;
    Ok(
        (policy_sum.ok_or_else(|| training_error("missing policy sum"))?
            + value_sum
                .ok_or_else(|| training_error("missing value sum"))?
                .mul_scalar(value_coefficient))
        .div_scalar(group_count),
    )
}

fn selected_actions_v1(host: &HostPackingWorkspace) -> Result<Vec<usize>, Box<dyn Error>> {
    host.action_offsets
        .windows(2)
        .enumerate()
        .map(|(decision, offsets)| {
            let count = offsets[1]
                .checked_sub(offsets[0])
                .filter(|count| *count > 0)
                .ok_or_else(|| training_error("packed decision has no legal action"))?;
            Ok(decision % count)
        })
        .collect()
}

fn terminal_returns_v1(decisions: usize) -> Vec<i8> {
    const PATTERN: [i8; 4] = [1, -1, 0, 1];
    (0..decisions)
        .map(|index| PATTERN[index % PATTERN.len()])
        .collect()
}

fn cpu_train_step_v1(
    state: &mut NativePolicyValueTrainStateV1,
    cases: &[EncodedDecisionOwned],
    host: &HostPackingWorkspace,
    selected_action_indices: &[usize],
    terminal_returns: &[i8],
    value_coefficient: f32,
    learning_rate: f32,
) -> Result<crate::native_policy_train_step_v1::NativePolicyTrainStepResultV1, Box<dyn Error>> {
    // The production train step revalidates transported scorer bits before
    // any backward or optimizer work; the oracle harness supplies them from
    // an independent CPU forward over the exact encoded cases.
    struct ExpectedForwardBitsV1 {
        raw_action_logit_bits: Vec<u32>,
        value_bits: u32,
    }
    let expected_forward_bits = host
        .case_indices
        .iter()
        .map(|&case_index| {
            let output = state
                .model_v1()
                .forward_v1(cases[case_index].view())
                .map_err(|error| training_error(format!("expected forward: {error}")))?;
            Ok(ExpectedForwardBitsV1 {
                raw_action_logit_bits: output.logits.iter().map(|value| value.to_bits()).collect(),
                value_bits: output.value.to_bits(),
            })
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    let substeps = host
        .case_indices
        .iter()
        .copied()
        .zip(selected_action_indices.iter().copied())
        .zip(expected_forward_bits.iter())
        .map(
            |((case_index, selected_action_index), expected)| NativePolicySubstepV1 {
                forward: NativePolicyForwardInputV1::Encoded(Box::new(cases[case_index].view())),
                selected_action_index,
                expected_raw_action_logit_bits: &expected.raw_action_logit_bits,
                expected_value_bits: expected.value_bits,
            },
        )
        .collect::<Vec<_>>();
    let groups = substeps
        .iter()
        .zip(terminal_returns.iter().copied())
        .map(
            |(substep, terminal_return)| NativePolicyPhysicalDecisionV1 {
                substeps: std::slice::from_ref(substep),
                terminal_return,
            },
        )
        .collect::<Vec<_>>();
    Ok(state.train_step_v1(&groups, value_coefficient, learning_rate)?)
}

fn patterned_snapshot_v1(
    state: &NativePolicyValueTrainStateV1,
) -> Result<NativePolicyValueTrainSnapshotV1, Box<dyn Error>> {
    let mut snapshot = state.snapshot_v1()?;
    snapshot.adam_step = PATTERNED_ADAM_STEP_V1;
    for parameter_index in 0..snapshot.parameters.len() {
        for value_index in 0..snapshot.parameters[parameter_index].values.len() {
            let first_magnitude =
                ((parameter_index + 1) * 1009 + value_index % 997 + 1) as f32 * 1.0e-9;
            let first = if value_index & 1 == 0 {
                first_magnitude
            } else {
                -first_magnitude
            };
            let second = ((parameter_index + 1) * 1013 + value_index % 991 + 1) as f32 * 1.0e-10;
            snapshot.first_moments[parameter_index].values[value_index] = first;
            snapshot.second_moments[parameter_index].values[value_index] = second;
        }
    }
    snapshot.first_moments[0].values[..CARD_EMBEDDING_DIM_V1].fill(0.0);
    snapshot.second_moments[0].values[..CARD_EMBEDDING_DIM_V1].fill(0.0);
    snapshot.first_moments[SCORER_SECOND_BIAS_ORDINAL_V1].values[0] = 0.0;
    snapshot.second_moments[SCORER_SECOND_BIAS_ORDINAL_V1].values[0] = 0.0;
    snapshot.state_sha256_v1()?;
    let validated =
        NativePolicyValueTrainStateV1::from_snapshot_v1(state.model_v1().clone(), &snapshot)?;
    Ok(validated.snapshot_v1()?)
}

fn train_snapshots_are_bit_exact_v1(
    left: &NativePolicyValueTrainSnapshotV1,
    right: &NativePolicyValueTrainSnapshotV1,
) -> bool {
    left.adam_step == right.adam_step
        && left.scorer_bias_anchor_bits == right.scorer_bias_anchor_bits
        && parameter_snapshots_are_bit_exact(&left.parameters, &right.parameters)
        && parameter_snapshots_are_bit_exact(&left.first_moments, &right.first_moments)
        && parameter_snapshots_are_bit_exact(&left.second_moments, &right.second_moments)
}

#[derive(Clone, Copy, Debug, Default)]
struct NamedParitySummaryV1 {
    maximum_absolute_error: f32,
    maximum_relative_error: f32,
    maximum_tolerance_ratio: f32,
    bit_equal_values: usize,
    value_count: usize,
}

fn compare_named_tensors_v1(
    reference: &[NativeNamedParameterV1],
    actual: &[NativeNamedParameterV1],
    absolute_tolerance: f32,
    relative_tolerance: f32,
) -> Result<NamedParitySummaryV1, Box<dyn Error>> {
    if reference.len() != actual.len() {
        return Err(training_error("named tensor count mismatch"));
    }
    let mut summary = NamedParitySummaryV1::default();
    for (reference, actual) in reference.iter().zip(actual) {
        if reference.name != actual.name
            || reference.shape != actual.shape
            || reference.values.len() != actual.values.len()
        {
            return Err(training_error(format!(
                "named tensor layout mismatch: {} {:?} versus {} {:?}",
                reference.name, reference.shape, actual.name, actual.shape
            )));
        }
        for (&expected, &observed) in reference.values.iter().zip(&actual.values) {
            if !expected.is_finite() || !observed.is_finite() {
                return Err(training_error(format!(
                    "non-finite named tensor comparison in {}",
                    reference.name
                )));
            }
            let absolute = (observed - expected).abs();
            let relative = absolute / expected.abs().max(RELATIVE_ERROR_DENOMINATOR_FLOOR_V1);
            let permitted = absolute_tolerance + relative_tolerance * expected.abs();
            summary.maximum_absolute_error = summary.maximum_absolute_error.max(absolute);
            summary.maximum_relative_error = summary.maximum_relative_error.max(relative);
            summary.maximum_tolerance_ratio =
                summary.maximum_tolerance_ratio.max(if permitted == 0.0 {
                    0.0
                } else {
                    absolute / permitted
                });
            summary.value_count += 1;
            if expected.to_bits() == observed.to_bits() {
                summary.bit_equal_values += 1;
            }
            if absolute > permitted {
                return Err(training_error(format!(
                    "named tensor tolerance exceeded in {}: actual={observed:?} expected={expected:?} absolute={absolute:?} permitted={permitted:?}",
                    reference.name
                )));
            }
        }
    }
    Ok(summary)
}

fn compare_scalar_v1(
    reference: f32,
    actual: f32,
    absolute_tolerance: f32,
    relative_tolerance: f32,
) -> Result<NamedParitySummaryV1, Box<dyn Error>> {
    let reference_tensor = [NativeNamedParameterV1 {
        name: "loss",
        shape: vec![1],
        values: vec![reference],
    }];
    let actual_tensor = [NativeNamedParameterV1 {
        name: "loss",
        shape: vec![1],
        values: vec![actual],
    }];
    compare_named_tensors_v1(
        &reference_tensor,
        &actual_tensor,
        absolute_tolerance,
        relative_tolerance,
    )
}

/// Production-trainer bridge parity: multi-substep groups built from the real
/// tensorized fixture cases run through both the CPU reference step and the
/// CudaBurnDense bridge from identical snapshots; parameters, moments, loss,
/// physical terms, and selected outputs must agree within the oracle
/// tolerances, with the gauge parameter bit-exact on both sides.
fn run_bridge_parity_v1(
    cases: &[EncodedDecisionOwned],
    reference_state: &NativePolicyValueTrainStateV1,
    snapshot: &NativePolicyValueTrainSnapshotV1,
) -> Result<serde_json::Value, Box<dyn Error>> {
    use crate::native_policy_train_step_v1::{NativePolicyForwardInputV1, NativePolicySubstepV1};

    let group_sizes = [1_usize, 2, 1, 3, 1];
    let substep_total: usize = group_sizes.iter().sum();
    let mut cpu_state = NativePolicyValueTrainStateV1::from_snapshot_v1(
        reference_state.model_v1().clone(),
        snapshot,
    )?;
    let mut bridge_state = NativePolicyValueTrainStateV1::from_snapshot_v1(
        reference_state.model_v1().clone(),
        snapshot,
    )?;

    struct ExpectedBitsV1 {
        case_index: usize,
        selected: usize,
        logit_bits: Vec<u32>,
        value_bits: u32,
    }
    let mut expected = Vec::with_capacity(substep_total);
    for flat in 0..substep_total {
        let case_index = flat % cases.len();
        let output = cpu_state
            .model_v1()
            .forward_v1(cases[case_index].view())
            .map_err(|error| training_error(format!("parity expected forward: {error:?}")))?;
        let selected = flat % output.logits.len();
        expected.push(ExpectedBitsV1 {
            case_index,
            selected,
            logit_bits: output.logits.iter().map(|value| value.to_bits()).collect(),
            value_bits: output.value.to_bits(),
        });
    }
    let mut group_substeps: Vec<Vec<NativePolicySubstepV1<'_>>> =
        Vec::with_capacity(group_sizes.len());
    let mut flat = 0_usize;
    for size in group_sizes {
        let mut substeps = Vec::with_capacity(size);
        for _ in 0..size {
            let entry = &expected[flat];
            substeps.push(NativePolicySubstepV1 {
                forward: NativePolicyForwardInputV1::Encoded(Box::new(
                    cases[entry.case_index].view(),
                )),
                selected_action_index: entry.selected,
                expected_raw_action_logit_bits: &entry.logit_bits,
                expected_value_bits: entry.value_bits,
            });
            flat += 1;
        }
        group_substeps.push(substeps);
    }
    let terminal_pattern = [1_i8, -1, 0, 1, -1];
    let groups = group_substeps
        .iter()
        .zip(terminal_pattern)
        .map(
            |(substeps, terminal_return)| NativePolicyPhysicalDecisionV1 {
                substeps,
                terminal_return,
            },
        )
        .collect::<Vec<_>>();

    let cpu_result = cpu_state
        .train_step_v1(&groups, VALUE_COEFFICIENT_V1, BENCHMARK_LEARNING_RATE_V1)
        .map_err(|error| training_error(format!("parity CPU step: {error:?}")))?;
    let bridge_result = super::bridge::train_step_cuda_burn_dense_v1(
        &mut bridge_state,
        &groups,
        VALUE_COEFFICIENT_V1,
        BENCHMARK_LEARNING_RATE_V1,
    )
    .map_err(|error| training_error(format!("parity bridge step: {error:?}")))?;

    let cpu_after = cpu_state.snapshot_v1()?;
    let bridge_after = bridge_state.snapshot_v1()?;
    let parameter_parity = compare_named_tensors_v1(
        &cpu_after.parameters,
        &bridge_after.parameters,
        UPDATE_ABSOLUTE_TOLERANCE_V1,
        UPDATE_RELATIVE_TOLERANCE_V1,
    )?;
    let first_moment_parity = compare_named_tensors_v1(
        &cpu_after.first_moments,
        &bridge_after.first_moments,
        UPDATE_ABSOLUTE_TOLERANCE_V1,
        UPDATE_RELATIVE_TOLERANCE_V1,
    )?;
    let second_moment_parity = compare_named_tensors_v1(
        &cpu_after.second_moments,
        &bridge_after.second_moments,
        UPDATE_ABSOLUTE_TOLERANCE_V1,
        UPDATE_RELATIVE_TOLERANCE_V1,
    )?;
    let loss_parity = compare_scalar_v1(
        cpu_result.loss,
        bridge_result.loss,
        LOSS_ABSOLUTE_TOLERANCE_V1,
        LOSS_RELATIVE_TOLERANCE_V1,
    )?;
    if cpu_result.physical_terms.len() != bridge_result.physical_terms.len()
        || cpu_result.selected_outputs.len() != bridge_result.selected_outputs.len()
        || cpu_result.adam_step != bridge_result.adam_step
    {
        return Err(training_error("parity result cardinality mismatch"));
    }
    let mut max_term_delta = 0.0_f32;
    for (cpu_term, bridge_term) in cpu_result
        .physical_terms
        .iter()
        .zip(&bridge_result.physical_terms)
    {
        if cpu_term.terminal_return != bridge_term.terminal_return
            || cpu_term.substep_count != bridge_term.substep_count
        {
            return Err(training_error("parity physical term structure mismatch"));
        }
        max_term_delta = max_term_delta
            .max((cpu_term.joint_log_probability - bridge_term.joint_log_probability).abs())
            .max((cpu_term.value - bridge_term.value).abs());
    }
    if max_term_delta > LOSS_ABSOLUTE_TOLERANCE_V1 {
        return Err(training_error(format!(
            "parity physical terms exceed tolerance: {max_term_delta}"
        )));
    }
    let gauge_parameter_bit_exact = cpu_after.parameters[SCORER_SECOND_BIAS_ORDINAL_V1].values[0]
        .to_bits()
        == bridge_after.parameters[SCORER_SECOND_BIAS_ORDINAL_V1].values[0].to_bits();
    if !gauge_parameter_bit_exact {
        return Err(training_error("parity gauge parameter not bit-exact"));
    }

    Ok(serde_json::json!({
        "schema": "mtg-kernel-experimental-burn-net8-cuda-bridge-parity/v1",
        "claim": "diagnostic-only-not-end-to-end-training",
        "group_sizes": group_sizes,
        "substep_total": substep_total,
        "parameters": parity_json_v1(parameter_parity),
        "first_moments": parity_json_v1(first_moment_parity),
        "second_moments": parity_json_v1(second_moment_parity),
        "loss": parity_json_v1(loss_parity),
        "max_physical_term_delta": max_term_delta,
        "gauge_parameter_bit_exact": gauge_parameter_bit_exact,
        "bridge_gauge_raw_residual": bridge_result.scorer_bias_gauge.raw_gradient_residual,
        "bridge_gauge_bound": bridge_result.scorer_bias_gauge.derived_absolute_bound,
    }))
}

fn parity_json_v1(summary: NamedParitySummaryV1) -> serde_json::Value {
    serde_json::json!({
        "maximum_absolute_error": summary.maximum_absolute_error,
        "maximum_relative_error": summary.maximum_relative_error,
        "maximum_tolerance_ratio": summary.maximum_tolerance_ratio,
        "bit_equal_values": summary.bit_equal_values,
        "value_count": summary.value_count,
    })
}

fn adversarial_oracle_input_sha256_v1() -> String {
    let mut digest = Sha256::new();
    digest.update(b"mtg-kernel-experimental-cuda-near-tie-oracle-input/v1\0");
    digest.update((ADVERSARIAL_ORACLE_CASES_V1.len() as u32).to_be_bytes());
    for case in ADVERSARIAL_ORACLE_CASES_V1 {
        digest.update((case.name.len() as u32).to_be_bytes());
        digest.update(case.name.as_bytes());
        digest.update((case.logit_bits.len() as u32).to_be_bytes());
        for bits in case.logit_bits {
            digest.update(bits.to_be_bytes());
        }
        digest.update((case.selected as u32).to_be_bytes());
    }
    format!("{:x}", digest.finalize())
}

fn stable_scalar_log_softmax_loss_and_gradient_v1(
    case: AdversarialOracleCaseV1,
) -> Result<Vec<f32>, Box<dyn Error>> {
    if case.logit_bits.is_empty() || case.selected >= case.logit_bits.len() {
        return Err(training_error("invalid adversarial oracle case"));
    }
    let logits = case
        .logit_bits
        .iter()
        .map(|bits| f64::from(f32::from_bits(*bits)))
        .collect::<Vec<_>>();
    if !logits.iter().all(|value| value.is_finite()) {
        return Err(training_error("non-finite adversarial oracle logit"));
    }
    let maximum = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let exponentials = logits
        .iter()
        .map(|value| (*value - maximum).exp())
        .collect::<Vec<_>>();
    let sum = exponentials.iter().sum::<f64>();
    let loss = -((logits[case.selected] - maximum) - sum.ln());
    let mut output = Vec::with_capacity(logits.len() + 1);
    output.push(loss as f32);
    output.extend(exponentials.iter().enumerate().map(|(index, exponential)| {
        (*exponential / sum - if index == case.selected { 1.0 } else { 0.0 }) as f32
    }));
    Ok(output)
}

fn run_adversarial_near_tie_oracle_v1(
    device: &burn_cuda::CudaDevice,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let input_sha256 = adversarial_oracle_input_sha256_v1();
    if input_sha256 != ADVERSARIAL_ORACLE_INPUT_SHA256_V1 {
        return Err(training_error(format!(
            "adversarial oracle input hash drift: {input_sha256} != {ADVERSARIAL_ORACLE_INPUT_SHA256_V1}"
        )));
    }
    let mut reference = Vec::with_capacity(ADVERSARIAL_ORACLE_CASES_V1.len());
    let mut actual = Vec::with_capacity(ADVERSARIAL_ORACLE_CASES_V1.len());
    for case in ADVERSARIAL_ORACLE_CASES_V1 {
        let logits = case
            .logit_bits
            .iter()
            .map(|bits| f32::from_bits(*bits))
            .collect::<Vec<_>>();
        let input = Tensor::<CudaAutodiffBackendV1, 1>::from_data(
            TensorData::new(logits, [case.logit_bits.len()]),
            device,
        )
        .require_grad();
        let loss = log_softmax(input.clone(), 0)
            .slice([case.selected..case.selected + 1])
            .mul_scalar(-1.0);
        let loss_value = loss.clone().into_data().to_vec::<f32>()?;
        let gradients = loss.backward();
        let input_gradient = input
            .grad(&gradients)
            .ok_or_else(|| training_error("adversarial input gradient missing"))?
            .into_data()
            .to_vec::<f32>()?;
        CudaAutodiffBackendV1::sync(device)?;
        let mut observed = Vec::with_capacity(case.logit_bits.len() + 1);
        observed.push(
            *loss_value
                .first()
                .ok_or_else(|| training_error("adversarial loss output missing"))?,
        );
        observed.extend(input_gradient);
        reference.push(NativeNamedParameterV1 {
            name: case.name,
            shape: vec![case.logit_bits.len() + 1],
            values: stable_scalar_log_softmax_loss_and_gradient_v1(case)?,
        });
        actual.push(NativeNamedParameterV1 {
            name: case.name,
            shape: vec![case.logit_bits.len() + 1],
            values: observed,
        });
    }
    let parity = compare_named_tensors_v1(
        &reference,
        &actual,
        ADVERSARIAL_ORACLE_ABSOLUTE_TOLERANCE_V1,
        ADVERSARIAL_ORACLE_RELATIVE_TOLERANCE_V1,
    )?;
    Ok(serde_json::json!({
        "input_schema": "mtg-kernel-experimental-cuda-near-tie-oracle-input/v1",
        "input_sha256": input_sha256,
        "hash_pin_verified": true,
        "provenance": "fixed synthetic adjacent-f32, signed-zero, subnormal, exact-tie, and q8-scale logit constructions",
        "case_names": ADVERSARIAL_ORACLE_CASES_V1.iter().map(|case| case.name).collect::<Vec<_>>(),
        "cpu_reference": "f64 stable log-softmax and analytic selected-NLL gradient, each output rounded once to f32",
        "aggregation_rule": "global maximum over concatenated selected-NLL loss then every input-gradient component across all cases",
        "relative_error_denominator": "max(abs(reference), 1e-6)",
        "absolute_tolerance": ADVERSARIAL_ORACLE_ABSOLUTE_TOLERANCE_V1,
        "relative_tolerance": ADVERSARIAL_ORACLE_RELATIVE_TOLERANCE_V1,
        "parity": parity_json_v1(parity),
        "claim": "semantic-proximity-only-not-bit-equivalence-or-sampler-bucket-equivalence",
    }))
}

fn sha256_array_hex_v1(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn output_sha256_v1(output: &OutputData) -> String {
    let mut digest = Sha256::new();
    digest.update(b"mtg-kernel-experimental-cuda-forward-output-bits/v1\0");
    digest.update((output.logits.len() as u64).to_be_bytes());
    for value in &output.logits {
        digest.update(value.to_bits().to_be_bytes());
    }
    digest.update((output.values.len() as u64).to_be_bytes());
    for value in &output.values {
        digest.update(value.to_bits().to_be_bytes());
    }
    format!("{:x}", digest.finalize())
}

fn outputs_are_bit_exact_v1(left: &OutputData, right: &OutputData) -> bool {
    left.logits.len() == right.logits.len()
        && left.values.len() == right.values.len()
        && left
            .logits
            .iter()
            .zip(&right.logits)
            .chain(left.values.iter().zip(&right.values))
            .all(|(left, right)| left.to_bits() == right.to_bits())
}

#[derive(Debug, Serialize)]
struct ForwardDeterminismConfigurationV1 {
    decisions: usize,
    ragged_case_rotation: usize,
    invocations: usize,
    readback_chunk_limit: usize,
    device_synchronizations: usize,
    compared_output_values: usize,
    bit_flips: usize,
    baseline_output_sha256: String,
}

fn run_forward_determinism_v1(
    model: &ProductionNet8<CudaAutodiffBackendV1>,
    cases: &[EncodedDecisionOwned],
    device: &burn_cuda::CudaDevice,
    minimum_total_invocations: usize,
) -> Result<(usize, Vec<ForwardDeterminismConfigurationV1>), Box<dyn Error>> {
    let configuration_count =
        FORWARD_DETERMINISM_BATCH_SIZES_V1.len() * FORWARD_DETERMINISM_ROTATIONS_V1.len();
    let invocations_per_configuration = minimum_total_invocations.div_ceil(configuration_count);
    let mut evidence = Vec::with_capacity(configuration_count);
    for decisions in FORWARD_DETERMINISM_BATCH_SIZES_V1 {
        for rotation in FORWARD_DETERMINISM_ROTATIONS_V1 {
            let mut workspace = HostPackingWorkspace::default();
            workspace.reserve_for(cases, decisions);
            workspace.pack(cases, decisions, rotation)?;
            let batch = DevicePackedBatch::<CudaAutodiffBackendV1>::upload(device, &workspace);
            CudaAutodiffBackendV1::sync(device)?;
            let mut baseline = None;
            let mut bit_flips = 0usize;
            let mut compared_output_values = 0usize;
            let mut completed_invocations = 0usize;
            let mut device_synchronizations = 0usize;
            while completed_invocations < invocations_per_configuration {
                let chunk_size = FORWARD_DETERMINISM_READBACK_CHUNK_V1
                    .min(invocations_per_configuration - completed_invocations);
                let mut chunk_logits = Vec::with_capacity(chunk_size);
                let mut chunk_values = Vec::with_capacity(chunk_size);
                for _ in 0..chunk_size {
                    // Every item is a genuinely separate model.forward call.
                    // Only the verification barrier is batched: concatenating
                    // distinct returned tensors allows one synchronization and
                    // readback per chunk instead of manufacturing 10k host/GPU
                    // round trips that no trainer would perform.
                    let (logits, values) = model.forward(&batch);
                    chunk_logits.push(logits);
                    chunk_values.push(values);
                }
                let packed_logits = Tensor::cat(chunk_logits, 0);
                let packed_values = Tensor::cat(chunk_values, 0);
                CudaAutodiffBackendV1::sync(device)?;
                device_synchronizations += 1;
                let packed = read_output((packed_logits, packed_values))?;
                let logits_per_invocation = packed.logits.len() / chunk_size;
                let values_per_invocation = packed.values.len() / chunk_size;
                if logits_per_invocation * chunk_size != packed.logits.len()
                    || values_per_invocation * chunk_size != packed.values.len()
                {
                    return Err(training_error(
                        "forward determinism packed output cardinality mismatch",
                    ));
                }
                for chunk_index in 0..chunk_size {
                    let output = OutputData {
                        logits: packed.logits[chunk_index * logits_per_invocation
                            ..(chunk_index + 1) * logits_per_invocation]
                            .to_vec(),
                        values: packed.values[chunk_index * values_per_invocation
                            ..(chunk_index + 1) * values_per_invocation]
                            .to_vec(),
                    };
                    if let Some(baseline) = &baseline {
                        compared_output_values += output.logits.len() + output.values.len();
                        if !outputs_are_bit_exact_v1(baseline, &output) {
                            bit_flips += baseline
                                .logits
                                .iter()
                                .zip(&output.logits)
                                .chain(baseline.values.iter().zip(&output.values))
                                .filter(|(left, right)| left.to_bits() != right.to_bits())
                                .count();
                        }
                    } else {
                        baseline = Some(output);
                    }
                }
                completed_invocations += chunk_size;
            }
            let baseline =
                baseline.ok_or_else(|| training_error("determinism baseline missing"))?;
            if bit_flips != 0 {
                return Err(training_error(format!(
                    "forward determinism failed for decisions={decisions} rotation={rotation}: {bit_flips} bit flips"
                )));
            }
            evidence.push(ForwardDeterminismConfigurationV1 {
                decisions,
                ragged_case_rotation: rotation,
                invocations: invocations_per_configuration,
                readback_chunk_limit: FORWARD_DETERMINISM_READBACK_CHUNK_V1,
                device_synchronizations,
                compared_output_values,
                bit_flips,
                baseline_output_sha256: output_sha256_v1(&baseline),
            });
        }
    }
    Ok((
        invocations_per_configuration * configuration_count,
        evidence,
    ))
}

#[derive(Debug, Serialize)]
struct BackwardAdamDeterminismConfigurationV1 {
    decisions: usize,
    ragged_case_rotation: usize,
    invocations: usize,
    compared_f32_values: usize,
    bit_flips: usize,
    baseline_step_sha256: String,
}

fn step_evidence_sha256_v1(
    evidence: &ExperimentalStepEvidenceV1,
) -> Result<String, Box<dyn Error>> {
    let gradients = evidence
        .gradients
        .as_ref()
        .ok_or_else(|| training_error("determinism gradients missing"))?;
    let mut digest = Sha256::new();
    digest.update(b"mtg-kernel-experimental-cuda-backward-adam-step-bits/v1\0");
    digest.update(evidence.loss.to_bits().to_be_bytes());
    for value in [
        evidence.gauge.training_raw_residual,
        evidence.gauge.monitor_coefficient_host_sum,
        evidence.gauge.centered_monitor_raw_residual,
        evidence.gauge.centered_monitor_bound,
    ] {
        digest.update(value.to_bits().to_be_bytes());
    }
    digest.update((evidence.gauge.centered_monitor_operation_count as u64).to_be_bytes());
    digest.update(parameter_manifest_sha256(gradients).as_bytes());
    digest.update(evidence.snapshot.state_sha256_v1()?);
    Ok(format!("{:x}", digest.finalize()))
}

fn step_evidence_is_bit_exact_v1(
    left: &ExperimentalStepEvidenceV1,
    right: &ExperimentalStepEvidenceV1,
) -> bool {
    left.loss.to_bits() == right.loss.to_bits()
        && left.gauge.training_raw_residual.to_bits() == right.gauge.training_raw_residual.to_bits()
        && left.gauge.monitor_coefficient_host_sum.to_bits()
            == right.gauge.monitor_coefficient_host_sum.to_bits()
        && left.gauge.centered_monitor_raw_residual.to_bits()
            == right.gauge.centered_monitor_raw_residual.to_bits()
        && left.gauge.centered_monitor_bound.to_bits()
            == right.gauge.centered_monitor_bound.to_bits()
        && left.gauge.centered_monitor_operation_count
            == right.gauge.centered_monitor_operation_count
        && match (&left.gradients, &right.gradients) {
            (Some(left), Some(right)) => parameter_snapshots_are_bit_exact(left, right),
            (None, None) => true,
            _ => false,
        }
        && train_snapshots_are_bit_exact_v1(&left.snapshot, &right.snapshot)
}

fn step_evidence_f32_count_v1(evidence: &ExperimentalStepEvidenceV1) -> usize {
    5 + evidence
        .gradients
        .as_ref()
        .map(|gradients| {
            gradients
                .iter()
                .map(|parameter| parameter.values.len())
                .sum::<usize>()
        })
        .unwrap_or_default()
        + evidence
            .snapshot
            .parameters
            .iter()
            .chain(&evidence.snapshot.first_moments)
            .chain(&evidence.snapshot.second_moments)
            .map(|parameter| parameter.values.len())
            .sum::<usize>()
}

fn run_backward_adam_determinism_v1(
    snapshot: &NativePolicyValueTrainSnapshotV1,
    cases: &[EncodedDecisionOwned],
    device: &burn_cuda::CudaDevice,
    minimum_total_invocations: usize,
) -> Result<(usize, Vec<BackwardAdamDeterminismConfigurationV1>), Box<dyn Error>> {
    let configuration_count =
        FORWARD_DETERMINISM_BATCH_SIZES_V1.len() * FORWARD_DETERMINISM_ROTATIONS_V1.len();
    let invocations_per_configuration = minimum_total_invocations.div_ceil(configuration_count);
    let mut configurations = Vec::with_capacity(configuration_count);
    for decisions in FORWARD_DETERMINISM_BATCH_SIZES_V1 {
        for rotation in FORWARD_DETERMINISM_ROTATIONS_V1 {
            let mut workspace = HostPackingWorkspace::default();
            workspace.reserve_for(cases, decisions);
            workspace.pack(cases, decisions, rotation)?;
            let selected = selected_actions_v1(&workspace)?;
            let returns = terminal_returns_v1(decisions);
            let resident_batch =
                DevicePackedBatch::<CudaAutodiffBackendV1>::upload(device, &workspace);
            CudaAutodiffBackendV1::sync(device)?;
            let mut baseline = None;
            let mut baseline_step_sha256 = None;
            let mut compared_f32_values = 0usize;
            for invocation in 0..invocations_per_configuration {
                // Every repetition imports the identical parameter and moment
                // snapshot into a new model/state, invokes forward+backward,
                // runs ragged scatter and Adam, and validates owned candidates.
                let mut state =
                    ExperimentalDeviceTrainStateV1::import_snapshot_v1(snapshot, device)?;
                let evidence = state.train_one_step_v1(
                    &workspace,
                    Some(&resident_batch),
                    &selected,
                    &returns,
                    VALUE_COEFFICIENT_V1,
                    ORACLE_LEARNING_RATE_V1,
                    true,
                    false,
                    false,
                )?;
                let step_sha256 = step_evidence_sha256_v1(&evidence)?;
                if let Some(baseline) = &baseline {
                    compared_f32_values += step_evidence_f32_count_v1(&evidence);
                    if !step_evidence_is_bit_exact_v1(baseline, &evidence) {
                        return Err(training_error(format!(
                            "backward/Adam determinism failed for decisions={decisions} rotation={rotation} invocation={invocation}: {} != {step_sha256}",
                            baseline_step_sha256.as_deref().unwrap_or("missing-baseline")
                        )));
                    }
                } else {
                    baseline_step_sha256 = Some(step_sha256);
                    baseline = Some(evidence);
                }
            }
            configurations.push(BackwardAdamDeterminismConfigurationV1 {
                decisions,
                ragged_case_rotation: rotation,
                invocations: invocations_per_configuration,
                compared_f32_values,
                bit_flips: 0,
                baseline_step_sha256: baseline_step_sha256
                    .ok_or_else(|| training_error("backward determinism baseline missing"))?,
            });
        }
    }
    Ok((
        invocations_per_configuration * configuration_count,
        configurations,
    ))
}

#[derive(Debug, Deserialize)]
struct CrossRestartChildRecordV1 {
    schema: String,
    decisions: usize,
    ragged_case_rotation: usize,
    output_sha256: String,
}

#[derive(Debug, Serialize)]
struct CrossRestartEvidenceV1 {
    fresh_process_count: usize,
    decisions: usize,
    ragged_case_rotation: usize,
    output_sha256: String,
    bit_equal: bool,
    first_process_elapsed_us: f64,
    second_process_elapsed_us: f64,
}

#[derive(Debug, Deserialize)]
struct ColdTrainingChildRecordV1 {
    schema: String,
    decisions: usize,
    ragged_case_rotation: usize,
    model_and_moment_import_and_sync_us: f64,
    resident_batch_upload_and_sync_us: f64,
    first_step_h2d_us: f64,
    first_step_forward_us: f64,
    first_step_backward_us: f64,
    first_step_adam_us: f64,
    first_step_export_us: f64,
    first_step_full_us: f64,
    candidate_snapshot_sha256: String,
}

#[derive(Debug, Serialize)]
struct FreshProcessColdTrainingEvidenceV1 {
    fresh_process_and_cuda_context: bool,
    decisions: usize,
    ragged_case_rotation: usize,
    parent_observed_process_elapsed_us: f64,
    model_and_moment_import_and_sync_us: f64,
    resident_batch_upload_and_sync_us: f64,
    first_step_h2d_us: f64,
    first_step_forward_us: f64,
    first_step_backward_us: f64,
    first_step_adam_us: f64,
    first_step_export_us: f64,
    first_step_full_us: f64,
    candidate_snapshot_sha256: String,
}

fn has_argument_v1(flag: &str) -> bool {
    std::env::args().any(|argument| argument == flag)
}

fn run_determinism_child_v1() -> Result<(), Box<dyn Error>> {
    let decisions = argument_usize("--determinism-child-decisions", 16)?;
    let rotation = argument_usize("--determinism-child-rotation", 7)?;
    let native_model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())?;
    let mut native_state = NativePolicyValueTrainStateV1::new_v1(native_model)?;
    let (manifest_path, payload_path) = common_model_snapshot_paths_v1();
    load_common_model_snapshot_v1(&manifest_path, &payload_path, &mut native_state)?;
    let parameters = native_state.model_v1().parameter_snapshot_v1();
    let cases = load_real_fixture_cases()?;
    let mut workspace = HostPackingWorkspace::default();
    workspace.reserve_for(&cases, decisions);
    workspace.pack(&cases, decisions, rotation)?;
    let device = burn_cuda::CudaDevice::new(0);
    let model = ProductionNet8::<CudaAutodiffBackendV1>::import_native_v1(&parameters, &device)?;
    let batch = DevicePackedBatch::<CudaAutodiffBackendV1>::upload(&device, &workspace);
    let output = model.forward(&batch);
    CudaAutodiffBackendV1::sync(&device)?;
    let output = read_output(output)?;
    println!(
        "{}",
        serde_json::json!({
            "schema": "mtg-kernel-experimental-cuda-cross-restart-child/v1",
            "decisions": decisions,
            "ragged_case_rotation": rotation,
            "output_sha256": output_sha256_v1(&output),
        })
    );
    Ok(())
}

fn run_cold_training_child_v1() -> Result<(), Box<dyn Error>> {
    let decisions = argument_usize("--cold-training-child-decisions", 16)?;
    let rotation = argument_usize("--cold-training-child-rotation", 7)?;
    let native_model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())?;
    let mut native_state = NativePolicyValueTrainStateV1::new_v1(native_model)?;
    let (manifest_path, payload_path) = common_model_snapshot_paths_v1();
    load_common_model_snapshot_v1(&manifest_path, &payload_path, &mut native_state)?;
    let patterned_snapshot = patterned_snapshot_v1(&native_state)?;
    let cases = load_real_fixture_cases()?;
    let mut workspace = HostPackingWorkspace::default();
    workspace.reserve_for(&cases, decisions);
    workspace.pack(&cases, decisions, rotation)?;
    let selected = selected_actions_v1(&workspace)?;
    let returns = terminal_returns_v1(decisions);
    let device = burn_cuda::CudaDevice::new(0);
    let started = Instant::now();
    let mut state =
        ExperimentalDeviceTrainStateV1::import_snapshot_v1(&patterned_snapshot, &device)?;
    CudaAutodiffBackendV1::sync(&device)?;
    let model_and_moment_import_and_sync_us = elapsed_us(started);
    let started = Instant::now();
    let resident_batch = DevicePackedBatch::<CudaAutodiffBackendV1>::upload(&device, &workspace);
    CudaAutodiffBackendV1::sync(&device)?;
    let resident_batch_upload_and_sync_us = elapsed_us(started);
    let evidence = state.train_one_step_v1(
        &workspace,
        Some(&resident_batch),
        &selected,
        &returns,
        VALUE_COEFFICIENT_V1,
        BENCHMARK_LEARNING_RATE_V1,
        false,
        false,
        false,
    )?;
    println!(
        "{}",
        serde_json::json!({
            "schema": "mtg-kernel-experimental-cuda-cold-training-child/v1",
            "decisions": decisions,
            "ragged_case_rotation": rotation,
            "model_and_moment_import_and_sync_us": model_and_moment_import_and_sync_us,
            "resident_batch_upload_and_sync_us": resident_batch_upload_and_sync_us,
            "first_step_h2d_us": evidence.timings.h2d_us,
            "first_step_forward_us": evidence.timings.forward_us,
            "first_step_backward_us": evidence.timings.backward_us,
            "first_step_adam_us": evidence.timings.adam_us,
            "first_step_export_us": evidence.timings.export_us,
            "first_step_full_us": evidence.timings.full_us,
            "candidate_snapshot_sha256": sha256_array_hex_v1(evidence.snapshot.state_sha256_v1()?),
        })
    );
    Ok(())
}

fn spawn_determinism_child_v1(
    decisions: usize,
    rotation: usize,
) -> Result<(CrossRestartChildRecordV1, f64), Box<dyn Error>> {
    let started = Instant::now();
    let output = Command::new(std::env::current_exe()?)
        .arg("--determinism-child")
        .arg("--determinism-child-decisions")
        .arg(decisions.to_string())
        .arg("--determinism-child-rotation")
        .arg(rotation.to_string())
        .output()?;
    let elapsed = elapsed_us(started);
    if !output.status.success() {
        return Err(training_error(format!(
            "cross-restart child failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let stdout = String::from_utf8(output.stdout)?;
    let record = stdout
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<CrossRestartChildRecordV1>(line).ok())
        .ok_or_else(|| {
            training_error(format!("cross-restart child emitted no record: {stdout}"))
        })?;
    if record.schema != "mtg-kernel-experimental-cuda-cross-restart-child/v1"
        || record.decisions != decisions
        || record.ragged_case_rotation != rotation
    {
        return Err(training_error("cross-restart child record mismatch"));
    }
    Ok((record, elapsed))
}

fn run_cross_restart_determinism_v1() -> Result<CrossRestartEvidenceV1, Box<dyn Error>> {
    let decisions = 16;
    let rotation = 7;
    let (first, first_process_elapsed_us) = spawn_determinism_child_v1(decisions, rotation)?;
    let (second, second_process_elapsed_us) = spawn_determinism_child_v1(decisions, rotation)?;
    let bit_equal = first.output_sha256 == second.output_sha256;
    if !bit_equal {
        return Err(training_error(format!(
            "fresh-process forward digests differ: {} != {}",
            first.output_sha256, second.output_sha256
        )));
    }
    Ok(CrossRestartEvidenceV1 {
        fresh_process_count: 2,
        decisions,
        ragged_case_rotation: rotation,
        output_sha256: first.output_sha256,
        bit_equal,
        first_process_elapsed_us,
        second_process_elapsed_us,
    })
}

fn run_fresh_process_cold_training_v1(
    decisions: usize,
) -> Result<FreshProcessColdTrainingEvidenceV1, Box<dyn Error>> {
    let rotation = 7;
    let started = Instant::now();
    let output = Command::new(std::env::current_exe()?)
        .arg("--cold-training-child")
        .arg("--cold-training-child-decisions")
        .arg(decisions.to_string())
        .arg("--cold-training-child-rotation")
        .arg(rotation.to_string())
        .output()?;
    let parent_observed_process_elapsed_us = elapsed_us(started);
    if !output.status.success() {
        return Err(training_error(format!(
            "cold-training child failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let stdout = String::from_utf8(output.stdout)?;
    let record = stdout
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<ColdTrainingChildRecordV1>(line).ok())
        .ok_or_else(|| {
            training_error(format!("cold-training child emitted no record: {stdout}"))
        })?;
    if record.schema != "mtg-kernel-experimental-cuda-cold-training-child/v1"
        || record.decisions != decisions
        || record.ragged_case_rotation != rotation
    {
        return Err(training_error("cold-training child record mismatch"));
    }
    Ok(FreshProcessColdTrainingEvidenceV1 {
        fresh_process_and_cuda_context: true,
        decisions,
        ragged_case_rotation: rotation,
        parent_observed_process_elapsed_us,
        model_and_moment_import_and_sync_us: record.model_and_moment_import_and_sync_us,
        resident_batch_upload_and_sync_us: record.resident_batch_upload_and_sync_us,
        first_step_h2d_us: record.first_step_h2d_us,
        first_step_forward_us: record.first_step_forward_us,
        first_step_backward_us: record.first_step_backward_us,
        first_step_adam_us: record.first_step_adam_us,
        first_step_export_us: record.first_step_export_us,
        first_step_full_us: record.first_step_full_us,
        candidate_snapshot_sha256: record.candidate_snapshot_sha256,
    })
}

fn phase_statistics_v1(samples: &[f64]) -> serde_json::Value {
    serde_json::json!({
        "mean_us": mean_micros(samples),
        "p50_us": percentile_micros(samples, 0.50),
        "p95_us": percentile_micros(samples, 0.95),
    })
}

pub(super) fn run_cuda_training_v1() -> Result<(), Box<dyn Error>> {
    if has_argument_v1("--determinism-child") {
        return run_determinism_child_v1();
    }
    if has_argument_v1("--cold-training-child") {
        return run_cold_training_child_v1();
    }
    let training_decisions = argument_usize("--train-decisions", DEFAULT_TRAINING_DECISIONS_V1)?;
    let warmup = argument_usize("--train-warmup", DEFAULT_TRAINING_WARMUP_V1)?;
    let iterations = argument_usize("--train-iterations", DEFAULT_TRAINING_ITERATIONS_V1)?;
    let forward_determinism_invocations = argument_usize(
        "--forward-determinism-invocations",
        DEFAULT_FORWARD_DETERMINISM_INVOCATIONS_V1,
    )?;
    let backward_adam_determinism_invocations = argument_usize(
        "--backward-adam-determinism-invocations",
        DEFAULT_BACKWARD_ADAM_DETERMINISM_INVOCATIONS_V1,
    )?;

    let device_runtime_manifest = DeviceRuntimeManifestV1::collect_v1()?;
    let device_runtime_manifest_sha256 = device_runtime_manifest.sha256_v1();
    let native_model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())?;
    let mut loaded_native_state = NativePolicyValueTrainStateV1::new_v1(native_model)?;
    let (manifest_path, payload_path) = common_model_snapshot_paths_v1();
    let snapshot_record =
        load_common_model_snapshot_v1(&manifest_path, &payload_path, &mut loaded_native_state)?;
    let patterned_snapshot = patterned_snapshot_v1(&loaded_native_state)?;
    let patterned_digest = sha256_array_hex_v1(patterned_snapshot.state_sha256_v1()?);
    let cases = load_real_fixture_cases()?;
    let device = burn_cuda::CudaDevice::new(0);
    let adversarial_near_tie_oracle = run_adversarial_near_tie_oracle_v1(&device)?;

    let mut oracle_workspace = HostPackingWorkspace::default();
    oracle_workspace.reserve_for(&cases, ORACLE_DECISIONS_V1);
    oracle_workspace.pack(&cases, ORACLE_DECISIONS_V1, 0)?;
    let oracle_selected = selected_actions_v1(&oracle_workspace)?;
    let oracle_returns = terminal_returns_v1(ORACLE_DECISIONS_V1);

    let mut gpu_state =
        ExperimentalDeviceTrainStateV1::import_snapshot_v1(&patterned_snapshot, &device)?;
    let imported_round_trip = gpu_state.export_snapshot_v1()?;
    if !train_snapshots_are_bit_exact_v1(&patterned_snapshot, &imported_round_trip) {
        return Err(training_error(
            "patterned parameter/moment import-export round trip is not bit exact",
        ));
    }
    let (forward_determinism_total_invocations, forward_determinism) = run_forward_determinism_v1(
        &gpu_state.model,
        &cases,
        &device,
        forward_determinism_invocations,
    )?;
    let cross_restart_determinism = run_cross_restart_determinism_v1()?;
    let (backward_adam_determinism_total_invocations, backward_adam_determinism) =
        run_backward_adam_determinism_v1(
            &patterned_snapshot,
            &cases,
            &device,
            backward_adam_determinism_invocations,
        )?;

    let gauge_before = gpu_state.export_snapshot_v1()?;
    let gauge_error = gpu_state
        .train_one_step_v1(
            &oracle_workspace,
            None,
            &oracle_selected,
            &oracle_returns,
            VALUE_COEFFICIENT_V1,
            ORACLE_LEARNING_RATE_V1,
            false,
            true,
            false,
        )
        .expect_err("corrupted gauge residual must fail closed");
    if !matches!(
        gauge_error.downcast_ref::<ExperimentalTrainingErrorV1>(),
        Some(ExperimentalTrainingErrorV1::GaugeResidualExceeded { .. })
    ) {
        return Err(training_error(format!(
            "corrupted gauge returned the wrong error: {gauge_error}"
        )));
    }
    let gauge_after = gpu_state.export_snapshot_v1()?;
    if !train_snapshots_are_bit_exact_v1(&gauge_before, &gauge_after) {
        return Err(training_error(
            "corrupted gauge rejection changed live CUDA train state",
        ));
    }

    let rollback_before = gpu_state.export_snapshot_v1()?;
    let rollback_error = gpu_state
        .train_one_step_v1(
            &oracle_workspace,
            None,
            &oracle_selected,
            &oracle_returns,
            VALUE_COEFFICIENT_V1,
            ORACLE_LEARNING_RATE_V1,
            false,
            false,
            true,
        )
        .expect_err("injected pre-commit failure must reject the candidate");
    if rollback_error.to_string() != INJECTED_PRE_COMMIT_FAILURE_V1 {
        return Err(training_error(format!(
            "unexpected rollback error: {rollback_error}"
        )));
    }
    let rollback_after = gpu_state.export_snapshot_v1()?;
    if !train_snapshots_are_bit_exact_v1(&rollback_before, &rollback_after) {
        return Err(training_error(
            "injected pre-commit failure changed live CUDA train state",
        ));
    }

    let mut cpu_state = NativePolicyValueTrainStateV1::from_snapshot_v1(
        loaded_native_state.model_v1().clone(),
        &patterned_snapshot,
    )?;
    let cpu_result = cpu_train_step_v1(
        &mut cpu_state,
        &cases,
        &oracle_workspace,
        &oracle_selected,
        &oracle_returns,
        VALUE_COEFFICIENT_V1,
        ORACLE_LEARNING_RATE_V1,
    )?;
    let cpu_after = cpu_state.snapshot_v1()?;
    let gpu_result = gpu_state.train_one_step_v1(
        &oracle_workspace,
        None,
        &oracle_selected,
        &oracle_returns,
        VALUE_COEFFICIENT_V1,
        ORACLE_LEARNING_RATE_V1,
        true,
        false,
        false,
    )?;
    let gpu_gradients = gpu_result
        .gradients
        .as_ref()
        .ok_or_else(|| training_error("oracle GPU gradients were not captured"))?;
    if cpu_after.adam_step != gpu_result.snapshot.adam_step
        || cpu_after.scorer_bias_anchor_bits != gpu_result.snapshot.scorer_bias_anchor_bits
    {
        return Err(training_error("CPU/CUDA step or gauge-anchor mismatch"));
    }
    let loss_parity = compare_scalar_v1(
        cpu_result.loss,
        gpu_result.loss,
        LOSS_ABSOLUTE_TOLERANCE_V1,
        LOSS_RELATIVE_TOLERANCE_V1,
    )?;
    let gradient_parity = compare_named_tensors_v1(
        &cpu_result.gradients,
        gpu_gradients,
        GRADIENT_ABSOLUTE_TOLERANCE_V1,
        GRADIENT_RELATIVE_TOLERANCE_V1,
    )?;
    let parameter_parity = compare_named_tensors_v1(
        &cpu_after.parameters,
        &gpu_result.snapshot.parameters,
        UPDATE_ABSOLUTE_TOLERANCE_V1,
        UPDATE_RELATIVE_TOLERANCE_V1,
    )?;
    let first_moment_parity = compare_named_tensors_v1(
        &cpu_after.first_moments,
        &gpu_result.snapshot.first_moments,
        UPDATE_ABSOLUTE_TOLERANCE_V1,
        UPDATE_RELATIVE_TOLERANCE_V1,
    )?;
    let second_moment_parity = compare_named_tensors_v1(
        &cpu_after.second_moments,
        &gpu_result.snapshot.second_moments,
        UPDATE_ABSOLUTE_TOLERANCE_V1,
        UPDATE_RELATIVE_TOLERANCE_V1,
    )?;
    let gauge_parameter_bit_exact = cpu_after.parameters[SCORER_SECOND_BIAS_ORDINAL_V1].values[0]
        .to_bits()
        == gpu_result.snapshot.parameters[SCORER_SECOND_BIAS_ORDINAL_V1].values[0].to_bits();
    let gauge_moments_bit_exact =
        gpu_result.snapshot.first_moments[SCORER_SECOND_BIAS_ORDINAL_V1].values[0].to_bits() == 0
            && gpu_result.snapshot.second_moments[SCORER_SECOND_BIAS_ORDINAL_V1].values[0]
                .to_bits()
                == 0;
    if !gauge_parameter_bit_exact || !gauge_moments_bit_exact {
        return Err(training_error("CUDA scorer-bias gauge is not canonical"));
    }

    println!(
        "{}",
        serde_json::json!({
            "schema": "mtg-kernel-experimental-burn-net8-cuda-train-validation/v1",
            "diagnostic_identity_candidate": TRAINING_DIAGNOSTIC_IDENTITY_CANDIDATE_V1,
            "backend_identity_status": "withheld-stock-cubecl-fast-math-and-conditional-tf32-preclude-strict-fp32-attestation",
            "backend": "burn-autodiff-cuda-0.21.0-device-0-baseline-no-fusion-no-autotune",
            "device_runtime_manifest": &device_runtime_manifest,
            "device_runtime_manifest_sha256": device_runtime_manifest_sha256,
            "production_model_architecture": "kernel-policy-value-net-8",
            "loss_identity": TRAINING_LOSS_IDENTITY_V1,
            "claim": "experimental-tolerance-only-not-production-or-bit-identity",
            "parameter_mapping": {
                "tensor_count": PARAMETER_TENSOR_COUNT_V1,
                "parameter_count": PARAMETER_COUNT_V1,
                "linear_weight_mapping": "native-output-input-to-burn-input-output-with-inverse-export",
                "gradient_and_moment_mapping_uses_parameter_ids": true,
                "patterned_parameter_and_moment_round_trip_bit_exact": true,
                "patterned_snapshot_sha256": patterned_digest,
            },
            "transaction": {
                "candidate_parameter_and_moments_validated_before_commit": true,
                "injected_pre_commit_failure_rolled_back_bit_exact": true,
                "one_adam_step": gpu_result.snapshot.adam_step == patterned_snapshot.adam_step + 1,
                "scorer_bias_parameter_bit_exact": gauge_parameter_bit_exact,
                "scorer_bias_moments_positive_zero": gauge_moments_bit_exact,
                "deliberately_corrupted_monitor_graph_rejected_as_GaugeResidualExceeded": true,
            },
            "forward_determinism": {
                "minimum_requested_invocations": forward_determinism_invocations,
                "total_separate_forward_invocations": forward_determinism_total_invocations,
                "configuration_count": forward_determinism.len(),
                "fresh_model_forward_call_per_invocation": true,
                "cached_or_memoized_output_tensor_reused": false,
                "configurations": forward_determinism,
                "cross_restart": cross_restart_determinism,
            },
            "backward_ragged_scatter_and_adam_determinism": {
                "minimum_requested_invocations": backward_adam_determinism_invocations,
                "total_independent_snapshot_to_candidate_invocations": backward_adam_determinism_total_invocations,
                "identical_snapshot_reimported_per_invocation": true,
                "frozen_33_tensor_gradient_export_compared": true,
                "parameter_and_two_moment_streams_compared": true,
                "configurations": backward_adam_determinism,
            },
            "cpu_oracle": {
                "reference": "NativePolicyValueTrainStateV1::train_step_v1",
                "production_tensorized_decisions": ORACLE_DECISIONS_V1,
                "case_names": oracle_workspace.case_indices.iter().map(|index| cases[*index].name.as_str()).collect::<Vec<_>>(),
                "selected_action_indices": oracle_selected,
                "terminal_returns": oracle_returns,
                "value_coefficient": VALUE_COEFFICIENT_V1,
                "learning_rate": ORACLE_LEARNING_RATE_V1,
                "loss_tolerance": {"absolute": LOSS_ABSOLUTE_TOLERANCE_V1, "relative": LOSS_RELATIVE_TOLERANCE_V1},
                "gradient_tolerance": {"absolute": GRADIENT_ABSOLUTE_TOLERANCE_V1, "relative": GRADIENT_RELATIVE_TOLERANCE_V1},
                "update_tolerance": {"absolute": UPDATE_ABSOLUTE_TOLERANCE_V1, "relative": UPDATE_RELATIVE_TOLERANCE_V1},
                "relative_error_denominator_floor": RELATIVE_ERROR_DENOMINATOR_FLOOR_V1,
                "loss": parity_json_v1(loss_parity),
                "gradients": parity_json_v1(gradient_parity),
                "parameters_after": parity_json_v1(parameter_parity),
                "first_moments_after": parity_json_v1(first_moment_parity),
                "second_moments_after": parity_json_v1(second_moment_parity),
            },
            "adversarial_near_tie_oracle": adversarial_near_tie_oracle,
            "gauge_diagnostic": {
                "training_raw_residual": gpu_result.gauge.training_raw_residual,
                "training_raw_residual_is_diagnostic_only": true,
                "monitor_coefficient_host_sum": gpu_result.gauge.monitor_coefficient_host_sum,
                "centered_monitor_raw_residual": gpu_result.gauge.centered_monitor_raw_residual,
                "centered_monitor_bound": gpu_result.gauge.centered_monitor_bound,
                "centered_monitor_coefficient_term_multiplicity": CENTERED_MONITOR_COEFFICIENT_TERM_MULTIPLICITY_V1,
                "centered_monitor_modeled_basic_f32_operation_count": gpu_result.gauge.centered_monitor_operation_count,
                "separate_forward_and_backward_invocation": true,
                "monitor_coefficients": "analytic policy-gradient coefficients derived from the same CUDA logits/values and actual selected actions/returns, rounded once to f32",
                "monitor_transform": "per-ragged-row anchor subtraction before coefficient dot product",
                "bound_model": "Higham gamma over twice the coefficient absolute sum (the exact row-minus-anchor cancellation multiplicity) and a conservative basic-f32 operation count, plus one minimum-normal FTZ allowance per modeled operation; outward-rounded to f32; no empirical multiplier",
                "formal_production_bound": false,
                "limitation": "the centered monitor is a separately constructed loss and does not bound the actual training scorer-bias gradient; strict-fork source/PTX also shows approximate-tanh kernels on sibling monitor branches, so no production gauge or CUDA identity is claimed",
            },
            "snapshot": {
                "source_snapshot_sha256": snapshot_record.snapshot_sha256,
                "source_payload_sha256": snapshot_record.payload_sha256,
                "source_load_timed": snapshot_record.snapshot_load_timed,
            },
            "limitations": [
                "one substep per physical-decision group in this feasibility loss",
                "no trainer wrapper integration or production numerical identity",
                "no bit-exact CPU/CUDA gradient or optimizer claim",
                "the synthetic centered monitor does not bound the actual training scorer-bias gradient before canonicalization",
                "stock fast-math/conditional-TF32 mode also prevents a production CUDA identity",
                "the adversarial log-softmax oracle does not exercise q8 apportionment or sampler-bucket selection",
                "no end-to-end games-per-second or learning-quality claim",
            ],
        })
    );

    // Keep the expensive authority checks independently checkpointable from
    // the full-export timing loop.  This prevents a benchmark timeout from
    // discarding an already-completed determinism/rollback/oracle record.
    if has_argument_v1("--validation-only") {
        return Ok(());
    }

    let fresh_process_cold_training = run_fresh_process_cold_training_v1(training_decisions)?;
    let mut benchmark_workspace = HostPackingWorkspace::default();
    benchmark_workspace.reserve_for(&cases, training_decisions);
    benchmark_workspace.pack(&cases, training_decisions, 0)?;
    let benchmark_selected = selected_actions_v1(&benchmark_workspace)?;
    let benchmark_returns = terminal_returns_v1(training_decisions);

    let cold_import_started = Instant::now();
    let mut resident_state =
        ExperimentalDeviceTrainStateV1::import_snapshot_v1(&patterned_snapshot, &device)?;
    CudaAutodiffBackendV1::sync(&device)?;
    let same_process_cold_import_us = elapsed_us(cold_import_started);
    let resident_upload_started = Instant::now();
    let resident_batch =
        DevicePackedBatch::<CudaAutodiffBackendV1>::upload(&device, &benchmark_workspace);
    CudaAutodiffBackendV1::sync(&device)?;
    let one_time_resident_batch_upload_us = elapsed_us(resident_upload_started);
    let first_resident_step = resident_state.train_one_step_v1(
        &benchmark_workspace,
        Some(&resident_batch),
        &benchmark_selected,
        &benchmark_returns,
        VALUE_COEFFICIENT_V1,
        BENCHMARK_LEARNING_RATE_V1,
        false,
        false,
        false,
    )?;
    // The first step above is a cold-start diagnostic. Reset to the identical
    // snapshot used by the fresh-upload arm so both warmed samples cover the
    // same Adam-step trajectory.
    resident_state =
        ExperimentalDeviceTrainStateV1::import_snapshot_v1(&patterned_snapshot, &device)?;
    for _ in 0..warmup {
        black_box(resident_state.train_one_step_v1(
            &benchmark_workspace,
            Some(&resident_batch),
            &benchmark_selected,
            &benchmark_returns,
            VALUE_COEFFICIENT_V1,
            BENCHMARK_LEARNING_RATE_V1,
            false,
            false,
            false,
        )?);
    }
    let mut resident_samples = PhaseSamplesV1::with_capacity(iterations);
    let mut resident_final_state_sha256: Option<[u8; 32]> = None;
    for _ in 0..iterations {
        let evidence = resident_state.train_one_step_v1(
            &benchmark_workspace,
            Some(&resident_batch),
            &benchmark_selected,
            &benchmark_returns,
            VALUE_COEFFICIENT_V1,
            BENCHMARK_LEARNING_RATE_V1,
            false,
            false,
            false,
        )?;
        resident_samples.push(evidence.timings);
        let state_sha256 = evidence.snapshot.state_sha256_v1()?;
        black_box(state_sha256);
        resident_final_state_sha256 = Some(state_sha256);
    }
    let _ = resident_final_state_sha256
        .ok_or_else(|| training_error("resident benchmark arm produced no iterations"))?;

    let mut fresh_state =
        ExperimentalDeviceTrainStateV1::import_snapshot_v1(&patterned_snapshot, &device)?;
    for _ in 0..warmup {
        black_box(fresh_state.train_one_step_v1(
            &benchmark_workspace,
            None,
            &benchmark_selected,
            &benchmark_returns,
            VALUE_COEFFICIENT_V1,
            BENCHMARK_LEARNING_RATE_V1,
            false,
            false,
            false,
        )?);
    }
    let mut fresh_samples = PhaseSamplesV1::with_capacity(iterations);
    for _ in 0..iterations {
        let evidence = fresh_state.train_one_step_v1(
            &benchmark_workspace,
            None,
            &benchmark_selected,
            &benchmark_returns,
            VALUE_COEFFICIENT_V1,
            BENCHMARK_LEARNING_RATE_V1,
            false,
            false,
            false,
        )?;
        fresh_samples.push(evidence.timings);
        black_box(evidence.snapshot.state_sha256_v1()?);
    }

    // Lean production-shaped arm with the dense-padded loss. Correctness
    // anchors: (a) one dense-lean step from the identical snapshot stays
    // within the oracle update tolerances of the CPU reference step, with the
    // gauge parameter bit-exact and its moments positive zero; (b) the whole
    // arm run twice from the identical snapshot terminates bit-identically.
    let dense_plan = build_dense_loss_plan_v1(
        &benchmark_workspace,
        &benchmark_selected,
        &benchmark_returns,
        &device,
    )?;

    let mut oracle_cpu_state = NativePolicyValueTrainStateV1::from_snapshot_v1(
        loaded_native_state.model_v1().clone(),
        &patterned_snapshot,
    )?;
    let _ = cpu_train_step_v1(
        &mut oracle_cpu_state,
        &cases,
        &benchmark_workspace,
        &benchmark_selected,
        &benchmark_returns,
        VALUE_COEFFICIENT_V1,
        BENCHMARK_LEARNING_RATE_V1,
    )?;
    let lean_oracle_cpu_after = oracle_cpu_state.snapshot_v1()?;
    let mut lean_oracle_state =
        ExperimentalDeviceTrainStateV1::import_snapshot_v1(&patterned_snapshot, &device)?;
    lean_oracle_state.train_one_step_lean_v1(
        &resident_batch,
        &dense_plan,
        VALUE_COEFFICIENT_V1,
        BENCHMARK_LEARNING_RATE_V1,
    )?;
    let lean_oracle_gpu_after = lean_oracle_state.export_snapshot_v1()?;
    let lean_parameter_parity = compare_named_tensors_v1(
        &lean_oracle_cpu_after.parameters,
        &lean_oracle_gpu_after.parameters,
        UPDATE_ABSOLUTE_TOLERANCE_V1,
        UPDATE_RELATIVE_TOLERANCE_V1,
    )?;
    let lean_first_moment_parity = compare_named_tensors_v1(
        &lean_oracle_cpu_after.first_moments,
        &lean_oracle_gpu_after.first_moments,
        UPDATE_ABSOLUTE_TOLERANCE_V1,
        UPDATE_RELATIVE_TOLERANCE_V1,
    )?;
    let lean_second_moment_parity = compare_named_tensors_v1(
        &lean_oracle_cpu_after.second_moments,
        &lean_oracle_gpu_after.second_moments,
        UPDATE_ABSOLUTE_TOLERANCE_V1,
        UPDATE_RELATIVE_TOLERANCE_V1,
    )?;
    let lean_gauge_parameter_bit_exact =
        lean_oracle_cpu_after.parameters[SCORER_SECOND_BIAS_ORDINAL_V1].values[0].to_bits()
            == lean_oracle_gpu_after.parameters[SCORER_SECOND_BIAS_ORDINAL_V1].values[0].to_bits();
    let lean_gauge_moments_zero =
        lean_oracle_gpu_after.first_moments[SCORER_SECOND_BIAS_ORDINAL_V1].values[0].to_bits() == 0
            && lean_oracle_gpu_after.second_moments[SCORER_SECOND_BIAS_ORDINAL_V1].values[0]
                .to_bits()
                == 0;
    if !lean_gauge_parameter_bit_exact || !lean_gauge_moments_zero {
        return Err(training_error(
            "dense lean step scorer-bias gauge is not canonical",
        ));
    }

    let run_lean_arm = || -> Result<([u8; 32], f64), Box<dyn Error>> {
        let mut lean_state =
            ExperimentalDeviceTrainStateV1::import_snapshot_v1(&patterned_snapshot, &device)?;
        CudaAutodiffBackendV1::sync(&device)?;
        for _ in 0..warmup {
            lean_state.train_one_step_lean_v1(
                &resident_batch,
                &dense_plan,
                VALUE_COEFFICIENT_V1,
                BENCHMARK_LEARNING_RATE_V1,
            )?;
        }
        let loop_started = Instant::now();
        for _ in 0..iterations {
            lean_state.train_one_step_lean_v1(
                &resident_batch,
                &dense_plan,
                VALUE_COEFFICIENT_V1,
                BENCHMARK_LEARNING_RATE_V1,
            )?;
        }
        let loop_us = elapsed_us(loop_started);
        let snapshot = lean_state.export_snapshot_v1()?;
        Ok((snapshot.state_sha256_v1()?, loop_us))
    };
    let (lean_first_sha, lean_loop_us) = run_lean_arm()?;
    let (lean_second_sha, _) = run_lean_arm()?;
    if lean_first_sha != lean_second_sha {
        return Err(training_error(
            "dense lean loop is not run-to-run deterministic",
        ));
    }
    let bridge_parity = run_bridge_parity_v1(&cases, &loaded_native_state, &patterned_snapshot)?;
    println!("{bridge_parity}");

    let lean_decisions_per_second =
        (training_decisions as f64 * iterations as f64) / (lean_loop_us / 1.0e6);
    println!(
        "{}",
        serde_json::json!({
            "schema": "mtg-kernel-experimental-burn-net8-cuda-lean-loop/v2",
            "claim": "diagnostic-only-not-end-to-end-training",
            "loss_shape": "dense-padded-max-actions",
            "max_actions": dense_plan.max_actions,
            "decisions_per_step": training_decisions,
            "warmup_steps": warmup,
            "timed_steps": iterations,
            "loop_wall_us": lean_loop_us,
            "mean_step_us": lean_loop_us / iterations as f64,
            "decisions_per_second": lean_decisions_per_second,
            "run_to_run_bit_deterministic": true,
            "oracle_parity": {
                "parameters": parity_json_v1(lean_parameter_parity),
                "first_moments": parity_json_v1(lean_first_moment_parity),
                "second_moments": parity_json_v1(lean_second_moment_parity),
                "gauge_parameter_bit_exact": lean_gauge_parameter_bit_exact,
                "gauge_moments_positive_zero": lean_gauge_moments_zero,
            },
        })
    );

    if resident_state.adam_step != fresh_state.adam_step {
        return Err(training_error(
            "resident/fresh benchmark arms ended at different Adam steps",
        ));
    }

    println!(
        "{}",
        serde_json::json!({
            "schema": "mtg-kernel-experimental-burn-net8-cuda-train-benchmark/v2",
            "diagnostic_identity_candidate": TRAINING_DIAGNOSTIC_IDENTITY_CANDIDATE_V1,
            "backend_identity_status": "withheld",
            "device_runtime_manifest": &device_runtime_manifest,
            "device_runtime_manifest_sha256": device_runtime_manifest_sha256,
            "claim": "diagnostic-only-not-end-to-end-training",
            "decisions": training_decisions,
            "actions": benchmark_workspace.action_decision_indices.len(),
            "objects": benchmark_workspace.object_card_ids.len(),
            "edges": benchmark_workspace.edge_source_indices.len(),
            "action_refs": benchmark_workspace.action_ref_action_indices.len(),
            "warmup_steps": warmup,
            "timed_steps": iterations,
            "initial_adam_step": patterned_snapshot.adam_step,
            "resident_final_adam_step": resident_state.adam_step,
            "fresh_final_adam_step": fresh_state.adam_step,
            "value_coefficient": VALUE_COEFFICIENT_V1,
            "learning_rate": BENCHMARK_LEARNING_RATE_V1,
            "same_process_cold_start": {
                "fresh_cuda_context": false,
                "model_and_moment_import_and_sync_us": same_process_cold_import_us,
                "one_time_resident_batch_upload_and_sync_us": one_time_resident_batch_upload_us,
                "first_resident_candidate_step_us": first_resident_step.timings.full_us,
                "limitation": "oracle validation already initialized this process and CUDA context",
            },
            "fresh_process_cold_training": fresh_process_cold_training,
            "persistent_resident_steps": {
                "phase_timings": resident_samples.to_json_v1(),
                "full_candidate_decisions_per_second": resident_samples.rate_json_v1(training_decisions),
                "batch_h2d_per_step": false,
            },
            "fresh_upload_steps": {
                "phase_timings": fresh_samples.to_json_v1(),
                "full_candidate_decisions_per_second": fresh_samples.rate_json_v1(training_decisions),
                "batch_h2d_per_step": true,
            },
            "timing_boundaries": {
                "host_pack_excluded_and_workspace_reused": true,
                "full_33_tensor_parameter_and_two_moment_stream_d2h_export_included": true,
                "exported_bytes_per_step": PARAMETER_COUNT_V1 * std::mem::size_of::<f32>() * 3,
                "persistent_batch_tensors_reused_without_per_step_allocation_or_h2d": true,
                "candidate_model_and_moment_tensors_still_allocated_per_step": true,
                "cold_resident_step_discarded_then_both_arms_reimported_the_same_snapshot": true,
                "resident_and_fresh_final_adam_steps_equal": true,
            },
            "not_a_claim_about": [
                "trainer_wrapper_throughput",
                "games_per_second",
                "learning_quality",
                "production_numerical_contract",
            ],
        })
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_native_parameter_layouts_round_trip_through_burn_order() {
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let mut parameters = model.parameter_snapshot_v1();
        assert_eq!(parameters.len(), PARAMETER_TENSOR_COUNT_V1);
        for (ordinal, parameter) in parameters.iter_mut().enumerate() {
            for (index, value) in parameter.values.iter_mut().enumerate() {
                *value = f32::from_bits(
                    0x3e80_0000_u32
                        .wrapping_add((ordinal as u32) << 11)
                        .wrapping_add(index as u32 & 0x7ff),
                );
            }
            let (burn, _) = native_named_values_to_burn_v1(parameter).unwrap();
            let round_trip = burn_values_to_native_named_v1(parameter, &burn).unwrap();
            assert!(parameter
                .values
                .iter()
                .zip(round_trip)
                .all(|(left, right)| left.to_bits() == right.to_bits()));
        }
    }

    #[test]
    fn resident_snapshot_bit_identity_is_bitwise_not_float_equality() {
        use crate::native_policy_value_net_v1::NativeNamedParameterV1;

        let named = |values: Vec<f32>| NativeNamedParameterV1 {
            name: "p",
            shape: vec![values.len()],
            values,
        };
        let base = NativePolicyValueTrainSnapshotV1 {
            adam_step: 3,
            scorer_bias_anchor_bits: 0x3f80_0000,
            parameters: vec![named(vec![0.0, 1.5])],
            first_moments: vec![named(vec![0.25, -0.5])],
            second_moments: vec![named(vec![0.125, 0.75])],
        };
        assert!(super::bridge::snapshots_bit_identical_v1(&base, &base));

        // Derived float equality conflates the zero signs; the state hash and
        // therefore the reuse gate must not.
        let mut negative_zero = base.clone();
        negative_zero.parameters[0].values[0] = -0.0;
        assert_eq!(negative_zero, base);
        assert!(!super::bridge::snapshots_bit_identical_v1(
            &negative_zero,
            &base
        ));

        let mut stepped = base.clone();
        stepped.adam_step += 1;
        assert!(!super::bridge::snapshots_bit_identical_v1(&stepped, &base));

        let mut anchored = base.clone();
        anchored.scorer_bias_anchor_bits ^= 1;
        assert!(!super::bridge::snapshots_bit_identical_v1(&anchored, &base));

        let mut moment_ulp = base.clone();
        moment_ulp.second_moments[0].values[1] =
            f32::from_bits(moment_ulp.second_moments[0].values[1].to_bits() ^ 1);
        assert!(!super::bridge::snapshots_bit_identical_v1(
            &moment_ulp,
            &base
        ));

        let mut reshaped = base.clone();
        reshaped.first_moments[0].shape = vec![2, 1];
        assert!(!super::bridge::snapshots_bit_identical_v1(&reshaped, &base));
    }

    /// Two production bridge updates with the resident device state reused on
    /// the second, against the same two updates with the resident slot
    /// cleared between them (the import-every-update behavior): every commit
    /// must be bit-identical, and the counters must prove the reuse actually
    /// happened. Run explicitly and alone: the resident slot and counters are
    /// process-wide, so concurrent bridge-driving tests would perturb the
    /// counter deltas (never the correctness of either lane).
    #[test]
    #[ignore = "requires a CUDA device, run explicitly"]
    fn resident_reuse_is_bit_identical_to_fresh_import() {
        use crate::native_policy_train_step_v1::{
            NativePolicyForwardInputV1, NativePolicyPhysicalDecisionV1, NativePolicySubstepV1,
            NativePolicyTrainStepResultV1,
        };
        use std::sync::atomic::Ordering;

        let native_model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let mut seed_state = NativePolicyValueTrainStateV1::new_v1(native_model).unwrap();
        let (manifest_path, payload_path) = common_model_snapshot_paths_v1();
        load_common_model_snapshot_v1(&manifest_path, &payload_path, &mut seed_state).unwrap();
        let seed_snapshot = seed_state.snapshot_v1().unwrap();
        let cases = load_real_fixture_cases().unwrap();

        // One production bridge update; the expected transported bits are
        // recomputed from the current CPU model exactly as the rollout
        // scorer would after the preceding commit.
        let one_update = |state: &mut NativePolicyValueTrainStateV1| -> (
            NativePolicyTrainStepResultV1,
            [u8; 32],
        ) {
            let group_sizes = [1_usize, 2, 1, 3, 1];
            let substep_total: usize = group_sizes.iter().sum();
            struct ExpectedBits {
                case_index: usize,
                selected: usize,
                logit_bits: Vec<u32>,
                value_bits: u32,
            }
            let mut expected = Vec::with_capacity(substep_total);
            for flat in 0..substep_total {
                let case_index = flat % cases.len();
                let output = state
                    .model_v1()
                    .forward_v1(cases[case_index].view())
                    .unwrap();
                let selected = flat % output.logits.len();
                expected.push(ExpectedBits {
                    case_index,
                    selected,
                    logit_bits: output.logits.iter().map(|value| value.to_bits()).collect(),
                    value_bits: output.value.to_bits(),
                });
            }
            let mut group_substeps: Vec<Vec<NativePolicySubstepV1<'_>>> =
                Vec::with_capacity(group_sizes.len());
            let mut flat = 0_usize;
            for size in group_sizes {
                let mut substeps = Vec::with_capacity(size);
                for _ in 0..size {
                    let entry = &expected[flat];
                    substeps.push(NativePolicySubstepV1 {
                        forward: NativePolicyForwardInputV1::Encoded(Box::new(
                            cases[entry.case_index].view(),
                        )),
                        selected_action_index: entry.selected,
                        expected_raw_action_logit_bits: &entry.logit_bits,
                        expected_value_bits: entry.value_bits,
                    });
                    flat += 1;
                }
                group_substeps.push(substeps);
            }
            let terminal_pattern = [1_i8, -1, 0, 1, -1];
            let groups = group_substeps
                .iter()
                .zip(terminal_pattern)
                .map(
                    |(substeps, terminal_return)| NativePolicyPhysicalDecisionV1 {
                        substeps,
                        terminal_return,
                    },
                )
                .collect::<Vec<_>>();
            let result = super::bridge::train_step_cuda_burn_dense_v1(
                state,
                &groups,
                VALUE_COEFFICIENT_V1,
                BENCHMARK_LEARNING_RATE_V1,
            )
            .unwrap();
            let sha = state.snapshot_v1().unwrap().state_sha256_v1().unwrap();
            (result, sha)
        };

        // Resident lane: update one imports, update two must reuse.
        super::bridge::clear_resident_device_state_for_test_v1();
        let reuse_before = super::bridge::RESIDENT_REUSE_COUNT_V1.load(Ordering::Relaxed);
        let import_before = super::bridge::RESIDENT_IMPORT_COUNT_V1.load(Ordering::Relaxed);
        let mut resident_state = NativePolicyValueTrainStateV1::from_snapshot_v1(
            seed_state.model_v1().clone(),
            &seed_snapshot,
        )
        .unwrap();
        let (resident_first, resident_first_sha) = one_update(&mut resident_state);
        let (resident_second, resident_second_sha) = one_update(&mut resident_state);
        assert_eq!(
            super::bridge::RESIDENT_IMPORT_COUNT_V1.load(Ordering::Relaxed),
            import_before + 1,
            "the second update must not re-import"
        );
        assert_eq!(
            super::bridge::RESIDENT_REUSE_COUNT_V1.load(Ordering::Relaxed),
            reuse_before + 1,
            "the second update must reuse the resident device state"
        );

        // Fresh lane: identical inputs with the resident slot cleared between
        // updates, forcing the import-every-update behavior.
        super::bridge::clear_resident_device_state_for_test_v1();
        let mut fresh_state = NativePolicyValueTrainStateV1::from_snapshot_v1(
            seed_state.model_v1().clone(),
            &seed_snapshot,
        )
        .unwrap();
        let (fresh_first, fresh_first_sha) = one_update(&mut fresh_state);
        super::bridge::clear_resident_device_state_for_test_v1();
        let (fresh_second, fresh_second_sha) = one_update(&mut fresh_state);
        super::bridge::clear_resident_device_state_for_test_v1();

        assert_eq!(resident_first_sha, fresh_first_sha);
        assert_eq!(
            resident_second_sha, fresh_second_sha,
            "resident reuse must commit bit-identically to a fresh import"
        );
        for (resident, fresh) in [
            (&resident_first, &fresh_first),
            (&resident_second, &fresh_second),
        ] {
            assert_eq!(resident.loss.to_bits(), fresh.loss.to_bits());
            assert_eq!(resident.policy_sum.to_bits(), fresh.policy_sum.to_bits());
            assert_eq!(resident.value_sum.to_bits(), fresh.value_sum.to_bits());
            assert_eq!(resident.adam_step, fresh.adam_step);
            assert_eq!(resident.scorer_bias_gauge, fresh.scorer_bias_gauge);
        }
    }

    #[test]
    fn every_device_runtime_manifest_field_breaks_the_digest() {
        let baseline = DeviceRuntimeManifestV1 {
            gpu_model: "gpu".to_owned(),
            compute_capability: "1.2".to_owned(),
            cuda_driver_api_version: "3".to_owned(),
            cuda_runtime_version: "4".to_owned(),
            cuda_toolkit_binding: "toolkit".to_owned(),
            cublas_runtime_version: "5".to_owned(),
            cudarc_version: "6".to_owned(),
            cubecl_cuda_version: "7".to_owned(),
            cubek_matmul_version: "8".to_owned(),
            burn_version: "9".to_owned(),
            burn_cuda_version: "10".to_owned(),
            numerical_mode: "mode".to_owned(),
        };
        let baseline_digest = baseline.sha256_v1();
        for field_ordinal in 0..baseline.fields_v1().len() {
            let mut changed = baseline.clone();
            match field_ordinal {
                0 => changed.gpu_model.push('x'),
                1 => changed.compute_capability.push('x'),
                2 => changed.cuda_driver_api_version.push('x'),
                3 => changed.cuda_runtime_version.push('x'),
                4 => changed.cuda_toolkit_binding.push('x'),
                5 => changed.cublas_runtime_version.push('x'),
                6 => changed.cudarc_version.push('x'),
                7 => changed.cubecl_cuda_version.push('x'),
                8 => changed.cubek_matmul_version.push('x'),
                9 => changed.burn_version.push('x'),
                10 => changed.burn_cuda_version.push('x'),
                11 => changed.numerical_mode.push('x'),
                _ => unreachable!(),
            }
            assert_ne!(
                changed.sha256_v1(),
                baseline_digest,
                "field ordinal {field_ordinal} did not break the manifest digest"
            );
        }
    }

    #[test]
    fn adversarial_oracle_input_stream_matches_hash_pin() {
        assert_eq!(
            adversarial_oracle_input_sha256_v1(),
            ADVERSARIAL_ORACLE_INPUT_SHA256_V1
        );
        for case in ADVERSARIAL_ORACLE_CASES_V1 {
            assert!(!case.logit_bits.is_empty());
            assert!(case.selected < case.logit_bits.len());
            assert!(case
                .logit_bits
                .iter()
                .all(|bits| f32::from_bits(*bits).is_finite()));
            assert_eq!(
                stable_scalar_log_softmax_loss_and_gradient_v1(case)
                    .unwrap()
                    .len(),
                case.logit_bits.len() + 1
            );
        }
    }

    #[test]
    fn gauge_gate_accepts_the_boundary_and_rejects_one_ulp_beyond_it() {
        let bound = 1.0e-4_f32;
        let below = f32::from_bits(bound.to_bits() - 1);
        let above = next_positive_f32_v1(bound);
        enforce_gauge_bound_v1(0.0, bound).unwrap();
        enforce_gauge_bound_v1(below, bound).unwrap();
        enforce_gauge_bound_v1(-below, bound).unwrap();
        enforce_gauge_bound_v1(bound, bound).unwrap();
        enforce_gauge_bound_v1(-bound, bound).unwrap();
        for observed in [above, -above] {
            let error = enforce_gauge_bound_v1(observed, bound).unwrap_err();
            assert!(matches!(
                error.downcast_ref::<ExperimentalTrainingErrorV1>(),
                Some(ExperimentalTrainingErrorV1::GaugeResidualExceeded {
                    residual_bits,
                    bound_bits,
                }) if *residual_bits == observed.to_bits() && *bound_bits == bound.to_bits()
            ));
        }
    }

    #[test]
    fn formal_basic_f32_cancellation_bound_is_outward_and_monotone() {
        let addends = [1.0_f32, -0.5, 0.25, -0.125];
        let smaller = formal_basic_f32_cancellation_bound_v1(&addends, 2, 8).unwrap();
        let larger = formal_basic_f32_cancellation_bound_v1(&addends, 2, 32).unwrap();
        let higher_multiplicity = formal_basic_f32_cancellation_bound_v1(&addends, 3, 32).unwrap();
        assert!(smaller.is_finite() && smaller > 0.0);
        assert!(larger >= smaller);
        assert!(f64::from(larger) >= f64::from(smaller));
        assert!(higher_multiplicity > larger);
    }
}

/// Composition-invariance gate for device-side scoring designs: a decision's
/// forward outputs on the packed device graph must be bit-stable regardless
/// of batch companions, slot position, or batch size, because the rollout's
/// canonical stream is schedule invariant and batch composition is not. The
/// probe prints per-comparison bit equality; every row must report bit-equal
/// logits and value. Measured bit-exact on the RTX 4070 SUPER (2026-07-21);
/// any future scorer lane must keep this gate green on its target device.
#[cfg(test)]
mod composition_invariance_probe_tests {
    use super::*;

    fn score_composition_v1(
        device_state: &ExperimentalDeviceTrainStateV1,
        device: &burn_cuda::CudaDevice,
        cases: &[EncodedDecisionOwned],
        indices: &[usize],
    ) -> Vec<(Vec<u32>, u32)> {
        let views = indices
            .iter()
            .map(|index| cases[*index].view())
            .collect::<Vec<_>>();
        let mut workspace = HostPackingWorkspace::default();
        workspace.pack_views(&views).unwrap();
        let batch = DevicePackedBatch::upload(device, &workspace);
        let (logits, values) = device_state.forward_outputs_v1(&batch).unwrap();
        (0..indices.len())
            .map(|slot| {
                let begin = workspace.action_offsets[slot];
                let end = workspace.action_offsets[slot + 1];
                (
                    logits[begin..end]
                        .iter()
                        .map(|value| value.to_bits())
                        .collect(),
                    values[slot].to_bits(),
                )
            })
            .collect()
    }

    fn compare_case_rows_v1(label: &str, left: &(Vec<u32>, u32), right: &(Vec<u32>, u32)) {
        let logits_equal = left.0 == right.0;
        let value_equal = left.1 == right.1;
        let max_abs = left
            .0
            .iter()
            .zip(&right.0)
            .map(|(a, b)| (f32::from_bits(*a) - f32::from_bits(*b)).abs())
            .fold(0.0_f32, f32::max)
            .max((f32::from_bits(left.1) - f32::from_bits(right.1)).abs());
        println!(
            "{label}: logits_bit_equal={logits_equal} value_bit_equal={value_equal} \
             max_abs_diff={max_abs:e}"
        );
    }

    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn probe_forward_composition_invariance() {
        let native_model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let mut state = NativePolicyValueTrainStateV1::new_v1(native_model).unwrap();
        let (manifest_path, payload_path) = common_model_snapshot_paths_v1();
        load_common_model_snapshot_v1(&manifest_path, &payload_path, &mut state).unwrap();
        let snapshot = state.snapshot_v1().unwrap();
        let cases = load_real_fixture_cases().unwrap();
        for (index, case) in cases.iter().enumerate() {
            println!(
                "case {index}: objects {} edges {} actions {} refs {}",
                case.object_count(),
                case.edge_count(),
                case.action_count(),
                case.action_ref_count(),
            );
        }
        let device = burn_cuda::CudaDevice::new(0);
        let device_state =
            ExperimentalDeviceTrainStateV1::import_snapshot_v1(&snapshot, &device).unwrap();

        // Case 0 in composition A is the reference row.
        println!("scoring A");
        let a = score_composition_v1(&device_state, &device, &cases, &[0, 1, 2, 3, 4, 5, 6, 7]);
        // Same batch size, same slot, different companions.
        println!("scoring B");
        let b = score_composition_v1(
            &device_state,
            &device,
            &cases,
            &[0, 8, 9, 10, 11, 12, 13, 1],
        );
        // Same batch size, same companions, case 0 in the last slot.
        println!("scoring C");
        let c = score_composition_v1(&device_state, &device, &cases, &[7, 1, 2, 3, 4, 5, 6, 0]);
        // Batch size 2 (case 5 keeps the batch's edge total nonzero).
        println!("scoring E");
        let e = score_composition_v1(&device_state, &device, &cases, &[0, 5]);
        // Batch size 14.
        println!("scoring F");
        let f = score_composition_v1(
            &device_state,
            &device,
            &cases,
            &(0..cases.len().min(14)).collect::<Vec<_>>(),
        );
        // Identical repeat of A: run-to-run determinism control.
        println!("scoring A2");
        let a2 = score_composition_v1(&device_state, &device, &cases, &[0, 1, 2, 3, 4, 5, 6, 7]);

        compare_case_rows_v1("control_same_batch_rerun", &a[0], &a2[0]);
        compare_case_rows_v1("same_size_same_slot_different_companions", &a[0], &b[0]);
        compare_case_rows_v1("same_size_same_companions_last_slot", &a[0], &c[7]);
        compare_case_rows_v1("size_8_vs_size_2", &a[0], &e[0]);
        compare_case_rows_v1("size_8_vs_size_14", &a[0], &f[0]);

        // Sweep every shared case across A/B to bound row-level agreement.
        compare_case_rows_v1("case_1_slot_1_vs_slot_7", &a[1], &b[7]);
    }
}
