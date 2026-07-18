#![recursion_limit = "256"]

//! Synthetic CUDA training-capacity correctness experiment.
//!
//! `cuda-flat-training-capacity-v1` is deliberately not the repository's
//! production model contract. It is checkpoint- and initializer-incompatible
//! with the earlier forward-only spike; there is no checkpoint translation.
//! It starts from already-encoded, fixed ragged f32 batches and permits only a
//! zero-staleness duplicate forward: actor forward and learner recompute use the
//! same weights within a step. It excludes game-state encoding, actor scheduling,
//! sampling, replay, checkpoint I/O, and games/s. Any benchmark must live in an
//! external harness on a clean revision; this correctness example has no timing
//! or single-run gate-enforcement path.
//!
//! The provisional native optimizer contract is
//! `native-adam-epsilon-1e-5-v1`: gradients are never clamped or thresholded,
//! identical GPU backend/run inputs must be bit-exact, and the independent CPU
//! reference is tolerance parity rather than a cross-backend bit-identity claim.

use cudarc::cublas::{sys as blas_sys, CudaBlas, Gemm, GemmConfig};
use cudarc::driver::{
    CudaContext, CudaFunction, CudaSlice, CudaStream, LaunchConfig, PushKernelArg,
};
use cudarc::nvrtc::{compile_ptx_with_opts, CompileOptions};
use serde_json::json;
use std::error::Error;
use std::sync::Arc;

const STATE_DIM: usize = 2_048;
const ACTION_DIM: usize = 128;
const HIDDEN: usize = 64;
const PARAMETER_COUNT: usize = 156_097;
const WEIGHT_SEED: u64 = 0x4207_c0de_7150_1009;
const VALUE_COEFFICIENT: f32 = 0.5;
const LEARNING_RATE: f32 = 1.0e-3;
const ADAM_BETA1: f32 = 0.9;
const ADAM_BETA2: f32 = 0.999;
const ADAM_EPSILON: f32 = 1.0e-5;
const NATIVE_ADAM_CONTRACT_VERSION: &str = "native-adam-epsilon-1e-5-v1";
const UPDATED_PARAMETER_CPU_GPU_TOLERANCE: f32 = 5.0e-6;
const MODEL_CONTRACT_VERSION: &str = "cuda-flat-training-capacity-v1";
const GRADIENT_CHECK_STEP: f32 = 1.0e-3;
const GRADIENT_CHECK_ABS_TOLERANCE: f32 = 2.0e-3;
const GRADIENT_CHECK_REL_TOLERANCE: f32 = 5.0e-2;
const TINY_GRADIENT_ADAM_F64_TOLERANCE: f64 = 1.0e-9;
const BASE_COMMIT: &str = "0925ae591a297a0a425992105a26d59309a9729b";
const CUDA_THREADS: u32 = 256;

// Context only. The experiment has not been run against these gates.
const ROLLOUT_DECISIONS_PER_SECOND_DEMAND: f64 = 573_000.0;
const LEARNER_EPOCH_MULTIPLIER: usize = 1;
const FORWARD_PASSES_PER_ROLLOUT_DECISION: usize = 1 + LEARNER_EPOCH_MULTIPLIER;
const PROPOSED_CAPACITY_HEADROOM: f64 = 1.20;
const PROPOSED_COMPLETE_TRAINING_DECISIONS_PER_SECOND_GATE: f64 =
    ROLLOUT_DECISIONS_PER_SECOND_DEMAND * PROPOSED_CAPACITY_HEADROOM;

const CUDA_SOURCE: &str = r#"
#define HIDDEN 64

extern "C" __global__ void bias_activation(
    float* matrix,
    const float* bias,
    int rows,
    int cols,
    int relu
) {
    const int index = (int)(blockIdx.x * blockDim.x + threadIdx.x);
    const int length = rows * cols;
    if (index >= length) return;
    float value = matrix[index] + bias[index % cols];
    matrix[index] = relu ? fmaxf(value, 0.0f) : value;
}

extern "C" __global__ void gather_state_for_actions(
    const float* state_hidden,
    const int* action_owner,
    float* gathered,
    int total_actions
) {
    const int index = (int)(blockIdx.x * blockDim.x + threadIdx.x);
    const int length = total_actions * HIDDEN;
    if (index >= length) return;
    const int action = index / HIDDEN;
    const int column = index % HIDDEN;
    gathered[index] = state_hidden[action_owner[action] * HIDDEN + column];
}

extern "C" __global__ void terminal_loss_grad(
    const float* logits,
    const float* values,
    const int* offsets,
    const int* selected_global,
    const float* terminal_returns,
    float value_coefficient,
    int batch,
    float* policy_terms,
    float* value_terms,
    float* d_logits,
    float* d_values,
    unsigned int* invalid
) {
    const int decision = (int)(blockIdx.x * blockDim.x + threadIdx.x);
    if (decision >= batch) return;
    const int begin = offsets[decision];
    const int end = offsets[decision + 1];
    const int selected = selected_global[decision];
    if (begin >= end || selected < begin || selected >= end) {
        atomicExch(invalid, 1u);
        return;
    }
    float maximum = -3.402823466e+38F;
    for (int action = begin; action < end; ++action) {
        maximum = fmaxf(maximum, logits[action]);
    }
    float denominator = 0.0f;
    for (int action = begin; action < end; ++action) {
        denominator += expf(logits[action] - maximum);
    }
    const float shifted_log_denominator = logf(denominator);
    const float value = values[decision];
    const float target = terminal_returns[decision];
    const float advantage_detached = target - value;
    const float inverse_batch = 1.0f / (float)batch;
    for (int action = begin; action < end; ++action) {
        const float shifted_logit = logits[action] - maximum;
        const float probability = expf(shifted_logit - shifted_log_denominator);
        const float indicator = action == selected ? 1.0f : 0.0f;
        d_logits[action] = inverse_batch * advantage_detached * (probability - indicator);
    }
    const float selected_log_probability =
        (logits[selected] - maximum) - shifted_log_denominator;
    policy_terms[decision] = -selected_log_probability * advantage_detached;
    const float value_error = value - target;
    value_terms[decision] = value_error * value_error;
    d_values[decision] = inverse_batch * 2.0f * value_coefficient * value_error;
    if (!isfinite(policy_terms[decision]) || !isfinite(value_terms[decision]) ||
        !isfinite(d_values[decision])) {
        atomicExch(invalid, 1u);
    }
}

extern "C" __global__ void reduce_loss(
    const float* policy_terms,
    const float* value_terms,
    float value_coefficient,
    int batch,
    float* totals,
    unsigned int* invalid
) {
    if (blockIdx.x != 0 || threadIdx.x != 0) return;
    float policy = 0.0f;
    float value = 0.0f;
    for (int decision = 0; decision < batch; ++decision) {
        policy += policy_terms[decision];
        value += value_terms[decision];
    }
    totals[0] = policy;
    totals[1] = value;
    totals[2] = (policy + value_coefficient * value) / (float)batch;
    if (!isfinite(totals[0]) || !isfinite(totals[1]) || !isfinite(totals[2])) {
        atomicExch(invalid, 1u);
    }
}

extern "C" __global__ void scale_rows_relu(
    const float* row_scale,
    const float* weight,
    const float* activation,
    float* output,
    int rows,
    int cols
) {
    const int index = (int)(blockIdx.x * blockDim.x + threadIdx.x);
    const int length = rows * cols;
    if (index >= length) return;
    const int row = index / cols;
    const int column = index % cols;
    const float raw = row_scale[row] * weight[column];
    output[index] = activation[index] > 0.0f ? raw : 0.0f;
}

extern "C" __global__ void relu_grad_in_place(
    const float* activation,
    float* gradient,
    int length
) {
    const int index = (int)(blockIdx.x * blockDim.x + threadIdx.x);
    if (index >= length) return;
    if (!(activation[index] > 0.0f)) gradient[index] = 0.0f;
}

extern "C" __global__ void combine_relu_grad(
    const float* left,
    const float* right,
    const float* activation,
    float* output,
    int length
) {
    const int index = (int)(blockIdx.x * blockDim.x + threadIdx.x);
    if (index >= length) return;
    const float combined = left[index] + right[index];
    output[index] = activation[index] > 0.0f ? combined : 0.0f;
}

extern "C" __global__ void reduce_action_owner(
    const float* action_gradient,
    const int* offsets,
    float* state_gradient,
    int batch
) {
    const int index = (int)(blockIdx.x * blockDim.x + threadIdx.x);
    const int length = batch * HIDDEN;
    if (index >= length) return;
    const int decision = index / HIDDEN;
    const int column = index % HIDDEN;
    float sum = 0.0f;
    for (int action = offsets[decision]; action < offsets[decision + 1]; ++action) {
        sum += action_gradient[action * HIDDEN + column];
    }
    state_gradient[index] = sum;
}

extern "C" __global__ void bias_gradient(
    const float* matrix_gradient,
    float* bias_gradient_out,
    int rows,
    int cols
) {
    const int column = (int)(blockIdx.x * blockDim.x + threadIdx.x);
    if (column >= cols) return;
    float sum = 0.0f;
    for (int row = 0; row < rows; ++row) {
        sum += matrix_gradient[row * cols + column];
    }
    bias_gradient_out[column] = sum;
}

extern "C" __global__ void check_finite(
    const float* values,
    int length,
    unsigned int* invalid
) {
    const int index = (int)(blockIdx.x * blockDim.x + threadIdx.x);
    if (index < length && !isfinite(values[index])) atomicExch(invalid, 1u);
}

extern "C" __global__ void adam_update(
    float* values,
    const float* gradients,
    float* first_moment,
    float* second_moment,
    int length,
    float learning_rate,
    float beta1,
    float beta2,
    float inverse_bias_correction1,
    float inverse_bias_correction2,
    float epsilon,
    unsigned int* invalid
) {
    const int index = (int)(blockIdx.x * blockDim.x + threadIdx.x);
    if (index >= length || *invalid != 0u) return;
    const float value = values[index];
    const float gradient = gradients[index];
    const float old_first = first_moment[index];
    const float old_second = second_moment[index];
    if (!isfinite(value) || !isfinite(gradient) || !isfinite(old_first) ||
        !isfinite(old_second)) {
        atomicExch(invalid, 1u);
        return;
    }
    const float next_first = beta1 * old_first + (1.0f - beta1) * gradient;
    const float next_second = beta2 * old_second + (1.0f - beta2) * gradient * gradient;
    const float corrected_first = next_first * inverse_bias_correction1;
    const float corrected_second = next_second * inverse_bias_correction2;
    const float next_value = value - learning_rate * corrected_first /
        (sqrtf(corrected_second) + epsilon);
    if (!isfinite(next_first) || !isfinite(next_second) || !isfinite(next_value)) {
        atomicExch(invalid, 1u);
        return;
    }
    first_moment[index] = next_first;
    second_moment[index] = next_second;
    values[index] = next_value;
}
"#;

#[derive(Clone, Copy, Debug)]
struct AdamConfig {
    learning_rate: f32,
    beta1: f32,
    beta2: f32,
    epsilon: f32,
}

impl Default for AdamConfig {
    fn default() -> Self {
        Self {
            learning_rate: LEARNING_RATE,
            beta1: ADAM_BETA1,
            beta2: ADAM_BETA2,
            epsilon: ADAM_EPSILON,
        }
    }
}

#[derive(Clone, Debug)]
struct SyntheticBatch {
    offsets: Vec<i32>,
    action_owner: Vec<i32>,
    states: Vec<f32>,
    actions: Vec<f32>,
    selected_global: Vec<i32>,
    terminal_returns: Vec<f32>,
}

impl SyntheticBatch {
    fn new(
        counts: &[usize],
        selected_local: &[usize],
        terminal_returns: &[i32],
    ) -> Result<Self, Box<dyn Error>> {
        Self::new_with_salt(counts, selected_local, terminal_returns, 0)
    }

    fn new_with_salt(
        counts: &[usize],
        selected_local: &[usize],
        terminal_returns: &[i32],
        fixture_salt: u64,
    ) -> Result<Self, Box<dyn Error>> {
        if counts.is_empty()
            || counts.len() != selected_local.len()
            || counts.len() != terminal_returns.len()
        {
            return Err("synthetic batch shapes differ".into());
        }
        checked_i32(counts.len())?;
        let mut offsets = Vec::with_capacity(checked_sum(counts.len(), 1, "offset count")?);
        let mut action_owner = Vec::new();
        let mut selected_global = Vec::with_capacity(counts.len());
        offsets.push(0);
        let mut total_actions = 0usize;
        for (decision, ((&count, &selected), &terminal_return)) in counts
            .iter()
            .zip(selected_local)
            .zip(terminal_returns)
            .enumerate()
        {
            if count == 0 || selected >= count {
                return Err(format!("invalid ragged choice at decision {decision}").into());
            }
            if !matches!(terminal_return, -1..=1) {
                return Err(format!("invalid terminal return at decision {decision}").into());
            }
            let decision_i32 = checked_i32(decision)?;
            let selected = checked_sum(total_actions, selected, "selected action index")?;
            selected_global.push(checked_i32(selected)?);
            action_owner.extend(std::iter::repeat_n(decision_i32, count));
            total_actions = checked_sum(total_actions, count, "total actions")?;
            offsets.push(checked_i32(total_actions)?);
        }
        let mut states = Vec::with_capacity(checked_product(
            counts.len(),
            STATE_DIM,
            "synthetic state elements",
        )?);
        let mut actions = Vec::with_capacity(checked_product(
            action_owner.len(),
            ACTION_DIM,
            "synthetic action elements",
        )?);
        for (decision, &count) in counts.iter().enumerate() {
            let logical = 100_003u64
                .wrapping_add(fixture_salt.wrapping_mul(1_000_003))
                .wrapping_add((decision as u64).wrapping_mul(97));
            for feature in 0..STATE_DIM {
                states.push(deterministic_feature(logical, None, feature));
            }
            for action in 0..count {
                for feature in 0..ACTION_DIM {
                    actions.push(deterministic_feature(logical, Some(action), feature));
                }
            }
        }
        let batch = Self {
            offsets,
            action_owner,
            states,
            actions,
            selected_global,
            terminal_returns: terminal_returns.iter().map(|&value| value as f32).collect(),
        };
        batch.validate()?;
        Ok(batch)
    }

    fn small_golden() -> Result<Self, Box<dyn Error>> {
        Self::new(&[2, 3, 4], &[0, 2, 1], &[1, -1, 0])
    }

    fn small_golden_variant() -> Result<Self, Box<dyn Error>> {
        Self::new_with_salt(&[2, 3, 4], &[1, 0, 3], &[0, 1, -1], 1)
    }

    fn validate(&self) -> Result<(), Box<dyn Error>> {
        let batch = self.batch();
        if batch == 0 {
            return Err("synthetic batch is empty".into());
        }
        let expected_offsets = checked_sum(batch, 1, "offset count")?;
        let expected_states = checked_product(batch, STATE_DIM, "synthetic state elements")?;
        let expected_actions = checked_product(
            self.total_actions(),
            ACTION_DIM,
            "synthetic action elements",
        )?;
        let total_actions_i32 = checked_i32(self.total_actions())?;
        checked_i32(batch)?;
        if self.offsets.len() != expected_offsets
            || self.selected_global.len() != batch
            || self.terminal_returns.len() != batch
            || self.states.len() != expected_states
            || self.actions.len() != expected_actions
            || self.action_owner.len() != self.total_actions()
        {
            return Err("synthetic batch invariant failed".into());
        }
        if self.offsets.first() != Some(&0)
            || self.offsets.last().copied() != Some(total_actions_i32)
        {
            return Err("synthetic offsets are not closed".into());
        }
        for decision in 0..batch {
            let begin = usize::try_from(self.offsets[decision])
                .map_err(|_| format!("negative offset at decision {decision}"))?;
            let end = usize::try_from(self.offsets[decision + 1])
                .map_err(|_| format!("negative end offset at decision {decision}"))?;
            let selected = usize::try_from(self.selected_global[decision])
                .map_err(|_| format!("negative selected action at decision {decision}"))?;
            if begin >= end || end > self.total_actions() || selected < begin || selected >= end {
                return Err(format!("invalid selected action at decision {decision}").into());
            }
            if !matches!(self.terminal_returns[decision], -1.0 | 0.0 | 1.0) {
                return Err(format!("invalid return at decision {decision}").into());
            }
            let decision_i32 = checked_i32(decision)?;
            let owners = self
                .action_owner
                .get(begin..end)
                .ok_or_else(|| format!("owner range out of bounds at decision {decision}"))?;
            for owner in owners {
                if *owner != decision_i32 {
                    return Err(format!("action owner drift at decision {decision}").into());
                }
            }
        }
        if !self
            .states
            .iter()
            .chain(&self.actions)
            .all(|value| value.is_finite())
        {
            return Err("synthetic input contains a non-finite value".into());
        }
        Ok(())
    }

    fn batch(&self) -> usize {
        self.terminal_returns.len()
    }

    fn total_actions(&self) -> usize {
        self.action_owner.len()
    }
}

#[derive(Clone, Debug)]
struct HostParameter {
    value: Vec<f32>,
    gradient: Vec<f32>,
    first_moment: Vec<f32>,
    second_moment: Vec<f32>,
}

impl HostParameter {
    fn new(value: Vec<f32>) -> Self {
        let length = value.len();
        Self {
            value,
            gradient: vec![0.0; length],
            first_moment: vec![0.0; length],
            second_moment: vec![0.0; length],
        }
    }

    fn validate(&self, name: &str, expected: usize) -> Result<(), Box<dyn Error>> {
        if self.value.len() != expected
            || self.gradient.len() != expected
            || self.first_moment.len() != expected
            || self.second_moment.len() != expected
        {
            return Err(format!(
                "host parameter {name} shape mismatch: value={} gradient={} first={} second={} expected={expected}",
                self.value.len(),
                self.gradient.len(),
                self.first_moment.len(),
                self.second_moment.len()
            )
            .into());
        }
        if !self
            .value
            .iter()
            .chain(&self.gradient)
            .chain(&self.first_moment)
            .chain(&self.second_moment)
            .all(|value| value.is_finite())
        {
            return Err(format!("host parameter {name} contains a non-finite value").into());
        }
        if self.second_moment.iter().any(|value| *value < 0.0) {
            return Err(format!("host parameter {name} has a negative second moment").into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct HostModel {
    state_w1: HostParameter,
    state_b1: HostParameter,
    state_w2: HostParameter,
    state_b2: HostParameter,
    action_w: HostParameter,
    action_b: HostParameter,
    scorer_state_w: HostParameter,
    scorer_action_w: HostParameter,
    scorer_b: HostParameter,
    scorer_out_w: HostParameter,
    value_w1: HostParameter,
    value_b1: HostParameter,
    value_out_w: HostParameter,
    value_out_b: HostParameter,
    adam_step: usize,
}

impl HostModel {
    fn deterministic() -> Result<Self, Box<dyn Error>> {
        let mut rng = SplitMix64(WEIGHT_SEED);
        let state_w1 = initialized_matrix(STATE_DIM, HIDDEN, &mut rng)?;
        let state_b1 = initialized_bias(HIDDEN, &mut rng);
        let state_w2 = initialized_matrix(HIDDEN, HIDDEN, &mut rng)?;
        let state_b2 = initialized_bias(HIDDEN, &mut rng);
        let action_w = initialized_matrix(ACTION_DIM, HIDDEN, &mut rng)?;
        let action_b = initialized_bias(HIDDEN, &mut rng);
        let scorer_input = checked_product(HIDDEN, 2, "combined scorer input")?;
        let scorer_combined = initialized_matrix(scorer_input, HIDDEN, &mut rng)?;
        let scorer_split = checked_product(HIDDEN, HIDDEN, "scorer split")?;
        let (scorer_state_w, scorer_action_w) = scorer_combined.split_at(scorer_split);
        let scorer_state_w = scorer_state_w.to_vec();
        let scorer_action_w = scorer_action_w.to_vec();
        let scorer_b = initialized_bias(HIDDEN, &mut rng);
        let scorer_out_w = initialized_matrix(HIDDEN, 1, &mut rng)?;
        let value_w1 = initialized_matrix(HIDDEN, HIDDEN, &mut rng)?;
        let value_b1 = initialized_bias(HIDDEN, &mut rng);
        let value_out_w = initialized_matrix(HIDDEN, 1, &mut rng)?;
        let value_out_b = initialized_bias(1, &mut rng);
        let model = Self {
            state_w1: HostParameter::new(state_w1),
            state_b1: HostParameter::new(state_b1),
            state_w2: HostParameter::new(state_w2),
            state_b2: HostParameter::new(state_b2),
            action_w: HostParameter::new(action_w),
            action_b: HostParameter::new(action_b),
            scorer_state_w: HostParameter::new(scorer_state_w),
            scorer_action_w: HostParameter::new(scorer_action_w),
            scorer_b: HostParameter::new(scorer_b),
            scorer_out_w: HostParameter::new(scorer_out_w),
            value_w1: HostParameter::new(value_w1),
            value_b1: HostParameter::new(value_b1),
            value_out_w: HostParameter::new(value_out_w),
            value_out_b: HostParameter::new(value_out_b),
            adam_step: 0,
        };
        model.validate()?;
        Ok(model)
    }

    fn parameter_count(&self) -> Result<usize, Box<dyn Error>> {
        self.parameter_values()
            .iter()
            .try_fold(0usize, |total, values| {
                checked_sum(total, values.len(), "host parameter count")
            })
    }

    fn parameters(&self) -> [(&'static str, &HostParameter); 14] {
        [
            ("state_w1", &self.state_w1),
            ("state_b1", &self.state_b1),
            ("state_w2", &self.state_w2),
            ("state_b2", &self.state_b2),
            ("action_w", &self.action_w),
            ("action_b", &self.action_b),
            ("scorer_state_w", &self.scorer_state_w),
            ("scorer_action_w", &self.scorer_action_w),
            ("scorer_b", &self.scorer_b),
            ("scorer_out_w", &self.scorer_out_w),
            ("value_w1", &self.value_w1),
            ("value_b1", &self.value_b1),
            ("value_out_w", &self.value_out_w),
            ("value_out_b", &self.value_out_b),
        ]
    }

    fn parameter_values(&self) -> [&[f32]; 14] {
        [
            &self.state_w1.value,
            &self.state_b1.value,
            &self.state_w2.value,
            &self.state_b2.value,
            &self.action_w.value,
            &self.action_b.value,
            &self.scorer_state_w.value,
            &self.scorer_action_w.value,
            &self.scorer_b.value,
            &self.scorer_out_w.value,
            &self.value_w1.value,
            &self.value_b1.value,
            &self.value_out_w.value,
            &self.value_out_b.value,
        ]
    }

    fn parameter_mut(&mut self, name: &str) -> Option<&mut HostParameter> {
        match name {
            "state_w1" => Some(&mut self.state_w1),
            "state_b1" => Some(&mut self.state_b1),
            "state_w2" => Some(&mut self.state_w2),
            "state_b2" => Some(&mut self.state_b2),
            "action_w" => Some(&mut self.action_w),
            "action_b" => Some(&mut self.action_b),
            "scorer_state_w" => Some(&mut self.scorer_state_w),
            "scorer_action_w" => Some(&mut self.scorer_action_w),
            "scorer_b" => Some(&mut self.scorer_b),
            "scorer_out_w" => Some(&mut self.scorer_out_w),
            "value_w1" => Some(&mut self.value_w1),
            "value_b1" => Some(&mut self.value_b1),
            "value_out_w" => Some(&mut self.value_out_w),
            "value_out_b" => Some(&mut self.value_out_b),
            _ => None,
        }
    }

    fn validate(&self) -> Result<(), Box<dyn Error>> {
        let hidden_squared = checked_product(HIDDEN, HIDDEN, "hidden matrix")?;
        let expected = [
            checked_product(STATE_DIM, HIDDEN, "state_w1")?,
            HIDDEN,
            hidden_squared,
            HIDDEN,
            checked_product(ACTION_DIM, HIDDEN, "action_w")?,
            HIDDEN,
            hidden_squared,
            hidden_squared,
            HIDDEN,
            HIDDEN,
            hidden_squared,
            HIDDEN,
            HIDDEN,
            1,
        ];
        for (((actual_name, parameter), expected_name), expected_length) in self
            .parameters()
            .into_iter()
            .zip([
                "state_w1",
                "state_b1",
                "state_w2",
                "state_b2",
                "action_w",
                "action_b",
                "scorer_state_w",
                "scorer_action_w",
                "scorer_b",
                "scorer_out_w",
                "value_w1",
                "value_b1",
                "value_out_w",
                "value_out_b",
            ])
            .zip(expected)
        {
            if actual_name != expected_name {
                return Err("host parameter ordering drifted".into());
            }
            parameter.validate(actual_name, expected_length)?;
        }
        if self.parameter_count()? != PARAMETER_COUNT {
            return Err("host parameter count drifted".into());
        }
        if self.adam_step > i32::MAX as usize {
            return Err("host Adam step exceeds supported exponent range".into());
        }
        Ok(())
    }

    fn all_finite(&self) -> bool {
        self.parameters().into_iter().all(|(_, parameter)| {
            parameter
                .value
                .iter()
                .chain(&parameter.gradient)
                .chain(&parameter.first_moment)
                .chain(&parameter.second_moment)
                .all(|value| value.is_finite())
        })
    }

    fn value_hash(&self) -> String {
        hash_f32_iter(
            self.parameter_values()
                .into_iter()
                .flat_map(|values| values.iter().copied()),
        )
    }
}

#[derive(Clone, Debug)]
struct CpuActivations {
    state_h1: Vec<f32>,
    state_h2: Vec<f32>,
    action_h: Vec<f32>,
    state_for_actions: Vec<f32>,
    scorer_h: Vec<f32>,
    logits: Vec<f32>,
    value_h: Vec<f32>,
    values: Vec<f32>,
}

#[derive(Clone, Copy, Debug)]
struct LossSummary {
    policy_sum: f32,
    value_sum: f32,
    loss: f32,
}

#[derive(Clone, Debug)]
struct CpuOutputGradients {
    loss: LossSummary,
    d_logits: Vec<f32>,
    d_values: Vec<f32>,
}

#[derive(Clone, Debug)]
struct CpuStepResult {
    rollout: CpuActivations,
    recompute: CpuActivations,
    loss: LossSummary,
}

fn cpu_forward(
    model: &HostModel,
    batch: &SyntheticBatch,
) -> Result<CpuActivations, Box<dyn Error>> {
    model.validate()?;
    batch.validate()?;
    let state_h1 = linear_relu(
        &batch.states,
        batch.batch(),
        STATE_DIM,
        &model.state_w1.value,
        &model.state_b1.value,
        HIDDEN,
    )?;
    let state_h2 = linear_relu(
        &state_h1,
        batch.batch(),
        HIDDEN,
        &model.state_w2.value,
        &model.state_b2.value,
        HIDDEN,
    )?;
    let action_h = linear_relu(
        &batch.actions,
        batch.total_actions(),
        ACTION_DIM,
        &model.action_w.value,
        &model.action_b.value,
        HIDDEN,
    )?;
    let mut state_for_actions =
        vec![0.0; checked_product(batch.total_actions(), HIDDEN, "gathered host state")?];
    for action in 0..batch.total_actions() {
        let owner = usize::try_from(batch.action_owner[action])?;
        let action_begin = checked_product(action, HIDDEN, "gathered action index")?;
        let action_end = checked_sum(action_begin, HIDDEN, "gathered action end")?;
        let owner_begin = checked_product(owner, HIDDEN, "gathered owner index")?;
        let owner_end = checked_sum(owner_begin, HIDDEN, "gathered owner end")?;
        let source = state_h2
            .get(owner_begin..owner_end)
            .ok_or("gathered owner range is out of bounds")?;
        state_for_actions
            .get_mut(action_begin..action_end)
            .ok_or("gathered action range is out of bounds")?
            .copy_from_slice(source);
    }
    let mut scorer_h = linear_no_bias(
        &state_for_actions,
        batch.total_actions(),
        HIDDEN,
        &model.scorer_state_w.value,
        HIDDEN,
    )?;
    linear_accumulate(
        &action_h,
        batch.total_actions(),
        HIDDEN,
        &model.scorer_action_w.value,
        HIDDEN,
        &mut scorer_h,
    )?;
    add_bias_activation(&mut scorer_h, &model.scorer_b.value, HIDDEN, true)?;
    let logits = linear_no_bias(
        &scorer_h,
        batch.total_actions(),
        HIDDEN,
        &model.scorer_out_w.value,
        1,
    )?;
    let value_h = linear_relu(
        &state_h2,
        batch.batch(),
        HIDDEN,
        &model.value_w1.value,
        &model.value_b1.value,
        HIDDEN,
    )?;
    let mut values = linear_no_bias(&value_h, batch.batch(), HIDDEN, &model.value_out_w.value, 1)?;
    add_bias_activation(&mut values, &model.value_out_b.value, 1, false)?;
    Ok(CpuActivations {
        state_h1,
        state_h2,
        action_h,
        state_for_actions,
        scorer_h,
        logits,
        value_h,
        values,
    })
}

#[allow(clippy::needless_range_loop)] // Mirrors indexed CUDA equations for auditability.
fn cpu_terminal_loss_and_output_gradients(
    logits: &[f32],
    values: &[f32],
    batch: &SyntheticBatch,
    value_coefficient: f32,
) -> Result<CpuOutputGradients, Box<dyn Error>> {
    batch.validate()?;
    if logits.len() != batch.total_actions() || values.len() != batch.batch() {
        return Err("CPU terminal-loss input shapes differ from the batch".into());
    }
    let detached_advantages = batch
        .terminal_returns
        .iter()
        .zip(values)
        .map(|(target, value)| target - value)
        .collect::<Vec<_>>();
    cpu_terminal_loss_with_detached_advantages(
        logits,
        values,
        batch,
        value_coefficient,
        &detached_advantages,
    )
}

#[allow(clippy::needless_range_loop)] // Mirrors indexed CUDA equations for auditability.
fn cpu_terminal_loss_with_detached_advantages(
    logits: &[f32],
    values: &[f32],
    batch: &SyntheticBatch,
    value_coefficient: f32,
    detached_advantages: &[f32],
) -> Result<CpuOutputGradients, Box<dyn Error>> {
    batch.validate()?;
    if logits.len() != batch.total_actions()
        || values.len() != batch.batch()
        || detached_advantages.len() != batch.batch()
    {
        return Err("detached terminal-loss input shapes differ from the batch".into());
    }
    if !value_coefficient.is_finite() || value_coefficient < 0.0 {
        return Err("value coefficient must be finite and non-negative".into());
    }
    if !logits
        .iter()
        .chain(values)
        .chain(detached_advantages)
        .all(|value| value.is_finite())
    {
        return Err("detached terminal-loss input is non-finite".into());
    }
    let batch_size = batch.batch();
    let mut d_logits = vec![0.0; batch.total_actions()];
    let mut d_values = vec![0.0; batch_size];
    let mut policy_sum = 0.0f32;
    let mut value_sum = 0.0f32;
    for decision in 0..batch_size {
        let begin = usize::try_from(batch.offsets[decision])?;
        let end = usize::try_from(batch.offsets[decision + 1])?;
        let selected = usize::try_from(batch.selected_global[decision])?;
        let maximum = logits[begin..end]
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        let denominator = logits[begin..end]
            .iter()
            .map(|logit| (*logit - maximum).exp())
            .sum::<f32>();
        let shifted_log_denominator = denominator.ln();
        let target = batch.terminal_returns[decision];
        let value = values[decision];
        let advantage_detached = detached_advantages[decision];
        for action in begin..end {
            let shifted_logit = logits[action] - maximum;
            let probability = (shifted_logit - shifted_log_denominator).exp();
            let indicator = if action == selected { 1.0 } else { 0.0 };
            d_logits[action] = advantage_detached * (probability - indicator) / batch_size as f32;
        }
        let selected_log_probability = (logits[selected] - maximum) - shifted_log_denominator;
        policy_sum += -selected_log_probability * advantage_detached;
        let value_error = value - target;
        value_sum += value_error * value_error;
        d_values[decision] = 2.0 * value_coefficient * value_error / batch_size as f32;
    }
    Ok(CpuOutputGradients {
        loss: LossSummary {
            policy_sum,
            value_sum,
            loss: (policy_sum + value_coefficient * value_sum) / batch_size as f32,
        },
        d_logits,
        d_values,
    })
}

#[allow(clippy::needless_range_loop)] // Mirrors indexed CUDA equations for auditability.
fn cpu_train_step(
    model: &mut HostModel,
    batch: &SyntheticBatch,
    value_coefficient: f32,
    adam: AdamConfig,
) -> Result<CpuStepResult, Box<dyn Error>> {
    model.validate()?;
    batch.validate()?;
    let rollout = cpu_forward(model, batch)?;
    let recompute = cpu_forward(model, batch)?;
    if max_abs(&rollout.logits, &recompute.logits)? != 0.0
        || max_abs(&rollout.values, &recompute.values)? != 0.0
    {
        return Err("CPU forward/recompute drifted".into());
    }
    let batch_size = batch.batch();
    let CpuOutputGradients {
        loss,
        d_logits,
        d_values,
    } = cpu_terminal_loss_and_output_gradients(
        &recompute.logits,
        &recompute.values,
        batch,
        value_coefficient,
    )?;

    model.scorer_out_w.gradient = matmul_tn(
        &recompute.scorer_h,
        &d_logits,
        batch.total_actions(),
        HIDDEN,
        1,
    )?;
    let mut d_scorer_pre =
        vec![0.0; checked_product(batch.total_actions(), HIDDEN, "host scorer gradient")?];
    for action in 0..batch.total_actions() {
        for hidden in 0..HIDDEN {
            let index = action * HIDDEN + hidden;
            if recompute.scorer_h[index] > 0.0 {
                d_scorer_pre[index] = d_logits[action] * model.scorer_out_w.value[hidden];
            }
        }
    }
    model.scorer_state_w.gradient = matmul_tn(
        &recompute.state_for_actions,
        &d_scorer_pre,
        batch.total_actions(),
        HIDDEN,
        HIDDEN,
    )?;
    model.scorer_action_w.gradient = matmul_tn(
        &recompute.action_h,
        &d_scorer_pre,
        batch.total_actions(),
        HIDDEN,
        HIDDEN,
    )?;
    model.scorer_b.gradient = column_sum(&d_scorer_pre, batch.total_actions(), HIDDEN)?;
    let d_state_for_actions = matmul_nt(
        &d_scorer_pre,
        &model.scorer_state_w.value,
        batch.total_actions(),
        HIDDEN,
        HIDDEN,
    )?;
    let mut d_action_h = matmul_nt(
        &d_scorer_pre,
        &model.scorer_action_w.value,
        batch.total_actions(),
        HIDDEN,
        HIDDEN,
    )?;
    apply_relu_gradient(&recompute.action_h, &mut d_action_h)?;
    model.action_w.gradient = matmul_tn(
        &batch.actions,
        &d_action_h,
        batch.total_actions(),
        ACTION_DIM,
        HIDDEN,
    )?;
    model.action_b.gradient = column_sum(&d_action_h, batch.total_actions(), HIDDEN)?;
    let mut d_h2_policy =
        vec![0.0; checked_product(batch_size, HIDDEN, "host policy state gradient")?];
    for decision in 0..batch_size {
        let begin = usize::try_from(batch.offsets[decision])?;
        let end = usize::try_from(batch.offsets[decision + 1])?;
        for action in begin..end {
            for hidden in 0..HIDDEN {
                d_h2_policy[decision * HIDDEN + hidden] +=
                    d_state_for_actions[action * HIDDEN + hidden];
            }
        }
    }

    model.value_out_w.gradient = matmul_tn(&recompute.value_h, &d_values, batch_size, HIDDEN, 1)?;
    model.value_out_b.gradient = column_sum(&d_values, batch_size, 1)?;
    let mut d_value_h = vec![0.0; checked_product(batch_size, HIDDEN, "host value gradient")?];
    for decision in 0..batch_size {
        for hidden in 0..HIDDEN {
            let index = decision * HIDDEN + hidden;
            if recompute.value_h[index] > 0.0 {
                d_value_h[index] = d_values[decision] * model.value_out_w.value[hidden];
            }
        }
    }
    model.value_w1.gradient =
        matmul_tn(&recompute.state_h2, &d_value_h, batch_size, HIDDEN, HIDDEN)?;
    model.value_b1.gradient = column_sum(&d_value_h, batch_size, HIDDEN)?;
    let d_h2_value = matmul_nt(
        &d_value_h,
        &model.value_w1.value,
        batch_size,
        HIDDEN,
        HIDDEN,
    )?;
    let d_state2_pre = d_h2_policy
        .iter()
        .zip(&d_h2_value)
        .zip(&recompute.state_h2)
        .map(
            |((&left, &right), &activation)| {
                if activation > 0.0 {
                    left + right
                } else {
                    0.0
                }
            },
        )
        .collect::<Vec<_>>();
    model.state_w2.gradient = matmul_tn(
        &recompute.state_h1,
        &d_state2_pre,
        batch_size,
        HIDDEN,
        HIDDEN,
    )?;
    model.state_b2.gradient = column_sum(&d_state2_pre, batch_size, HIDDEN)?;
    let mut d_state_h1 = matmul_nt(
        &d_state2_pre,
        &model.state_w2.value,
        batch_size,
        HIDDEN,
        HIDDEN,
    )?;
    apply_relu_gradient(&recompute.state_h1, &mut d_state_h1)?;
    model.state_w1.gradient = matmul_tn(&batch.states, &d_state_h1, batch_size, STATE_DIM, HIDDEN)?;
    model.state_b1.gradient = column_sum(&d_state_h1, batch_size, HIDDEN)?;
    model.adam_step = checked_sum(model.adam_step, 1, "CPU Adam step")?;
    checked_i32(model.adam_step)?;
    macro_rules! update {
        ($field:ident) => {
            cpu_adam_update(&mut model.$field, model.adam_step, adam)?;
        };
    }
    update!(state_w1);
    update!(state_b1);
    update!(state_w2);
    update!(state_b2);
    update!(action_w);
    update!(action_b);
    update!(scorer_state_w);
    update!(scorer_action_w);
    update!(scorer_b);
    update!(scorer_out_w);
    update!(value_w1);
    update!(value_b1);
    update!(value_out_w);
    update!(value_out_b);
    if !model.all_finite()
        || ![loss.policy_sum, loss.value_sum, loss.loss]
            .iter()
            .all(|value| value.is_finite())
    {
        return Err("CPU reference produced a non-finite result".into());
    }
    model.validate()?;
    Ok(CpuStepResult {
        rollout,
        recompute,
        loss,
    })
}

fn cpu_adam_update(
    parameter: &mut HostParameter,
    step: usize,
    config: AdamConfig,
) -> Result<(), Box<dyn Error>> {
    validate_adam(config)?;
    if step == 0 || step > i32::MAX as usize {
        return Err("CPU Adam step is outside 1..=i32::MAX".into());
    }
    let step_i32 = checked_i32(step)?;
    if parameter.value.len() != parameter.gradient.len()
        || parameter.value.len() != parameter.first_moment.len()
        || parameter.value.len() != parameter.second_moment.len()
    {
        return Err("CPU Adam parameter shapes differ".into());
    }
    let inverse_bias1 = 1.0 / (1.0 - config.beta1.powi(step_i32));
    let inverse_bias2 = 1.0 / (1.0 - config.beta2.powi(step_i32));
    for index in 0..parameter.value.len() {
        let gradient = parameter.gradient[index];
        let first = config.beta1 * parameter.first_moment[index] + (1.0 - config.beta1) * gradient;
        let second = config.beta2 * parameter.second_moment[index]
            + (1.0 - config.beta2) * gradient * gradient;
        let value = parameter.value[index]
            - config.learning_rate * (first * inverse_bias1)
                / ((second * inverse_bias2).sqrt() + config.epsilon);
        if !gradient.is_finite() || !first.is_finite() || !second.is_finite() || !value.is_finite()
        {
            return Err(format!("CPU Adam became non-finite at parameter index {index}").into());
        }
        parameter.first_moment[index] = first;
        parameter.second_moment[index] = second;
        parameter.value[index] = value;
    }
    if parameter.second_moment.iter().any(|value| *value < 0.0) {
        return Err("CPU Adam produced a negative second moment".into());
    }
    Ok(())
}

fn linear_relu(
    input: &[f32],
    rows: usize,
    input_columns: usize,
    weight: &[f32],
    bias: &[f32],
    output_columns: usize,
) -> Result<Vec<f32>, Box<dyn Error>> {
    let mut output = linear_no_bias(input, rows, input_columns, weight, output_columns)?;
    add_bias_activation(&mut output, bias, output_columns, true)?;
    Ok(output)
}

fn linear_no_bias(
    input: &[f32],
    rows: usize,
    input_columns: usize,
    weight: &[f32],
    output_columns: usize,
) -> Result<Vec<f32>, Box<dyn Error>> {
    let input_length = checked_product(rows, input_columns, "linear input")?;
    let weight_length = checked_product(input_columns, output_columns, "linear weight")?;
    let output_length = checked_product(rows, output_columns, "linear output")?;
    if input.len() != input_length || weight.len() != weight_length {
        return Err("linear shape mismatch".into());
    }
    let mut output = vec![0.0; output_length];
    linear_accumulate(
        input,
        rows,
        input_columns,
        weight,
        output_columns,
        &mut output,
    )?;
    Ok(output)
}

fn linear_accumulate(
    input: &[f32],
    rows: usize,
    input_columns: usize,
    weight: &[f32],
    output_columns: usize,
    output: &mut [f32],
) -> Result<(), Box<dyn Error>> {
    let input_length = checked_product(rows, input_columns, "linear accumulate input")?;
    let weight_length = checked_product(input_columns, output_columns, "linear accumulate weight")?;
    let output_length = checked_product(rows, output_columns, "linear accumulate output")?;
    if input.len() != input_length || weight.len() != weight_length || output.len() != output_length
    {
        return Err("linear accumulate shape mismatch".into());
    }
    for row in 0..rows {
        for inner in 0..input_columns {
            let source = input[row * input_columns + inner];
            for column in 0..output_columns {
                let index = row * output_columns + column;
                output[index] =
                    source.mul_add(weight[inner * output_columns + column], output[index]);
            }
        }
    }
    Ok(())
}

fn add_bias_activation(
    output: &mut [f32],
    bias: &[f32],
    columns: usize,
    relu: bool,
) -> Result<(), Box<dyn Error>> {
    if columns == 0 || bias.len() != columns || !output.len().is_multiple_of(columns) {
        return Err("bias activation shape mismatch".into());
    }
    for (index, value) in output.iter_mut().enumerate() {
        *value += bias[index % columns];
        if relu {
            *value = value.max(0.0);
        }
    }
    Ok(())
}

fn matmul_tn(
    left: &[f32],
    right: &[f32],
    rows: usize,
    left_columns: usize,
    right_columns: usize,
) -> Result<Vec<f32>, Box<dyn Error>> {
    let left_length = checked_product(rows, left_columns, "TN host left")?;
    let right_length = checked_product(rows, right_columns, "TN host right")?;
    let output_length = checked_product(left_columns, right_columns, "TN host output")?;
    if left.len() != left_length || right.len() != right_length {
        return Err("TN host shape mismatch".into());
    }
    let mut output = vec![0.0; output_length];
    for row in 0..rows {
        for left_column in 0..left_columns {
            let left_value = left[row * left_columns + left_column];
            for right_column in 0..right_columns {
                let index = left_column * right_columns + right_column;
                output[index] =
                    left_value.mul_add(right[row * right_columns + right_column], output[index]);
            }
        }
    }
    Ok(output)
}

fn matmul_nt(
    left: &[f32],
    right: &[f32],
    rows: usize,
    inner: usize,
    output_columns: usize,
) -> Result<Vec<f32>, Box<dyn Error>> {
    let left_length = checked_product(rows, inner, "NT host left")?;
    let right_length = checked_product(output_columns, inner, "NT host right")?;
    let output_length = checked_product(rows, output_columns, "NT host output")?;
    if left.len() != left_length || right.len() != right_length {
        return Err("NT host shape mismatch".into());
    }
    let mut output = vec![0.0; output_length];
    for row in 0..rows {
        for output_column in 0..output_columns {
            let mut sum = 0.0f32;
            for shared in 0..inner {
                sum =
                    left[row * inner + shared].mul_add(right[output_column * inner + shared], sum);
            }
            output[row * output_columns + output_column] = sum;
        }
    }
    Ok(output)
}

fn column_sum(input: &[f32], rows: usize, columns: usize) -> Result<Vec<f32>, Box<dyn Error>> {
    if input.len() != checked_product(rows, columns, "column sum input")? {
        return Err("column sum shape mismatch".into());
    }
    let mut output = vec![0.0; columns];
    for row in 0..rows {
        for column in 0..columns {
            output[column] += input[row * columns + column];
        }
    }
    Ok(output)
}

fn apply_relu_gradient(activation: &[f32], gradient: &mut [f32]) -> Result<(), Box<dyn Error>> {
    if activation.len() != gradient.len() {
        return Err("ReLU gradient shape mismatch".into());
    }
    for (&value, derivative) in activation.iter().zip(gradient) {
        if value <= 0.0 || value.is_nan() {
            *derivative = 0.0;
        }
    }
    Ok(())
}

fn initialized_matrix(
    fan_in: usize,
    fan_out: usize,
    rng: &mut SplitMix64,
) -> Result<Vec<f32>, Box<dyn Error>> {
    if fan_in == 0 || fan_out == 0 {
        return Err("initializer fan dimensions must be nonzero".into());
    }
    let fan_sum = checked_sum(fan_in, fan_out, "initializer fan sum")?;
    let scale = (6.0f32 / fan_sum as f32).sqrt();
    Ok((0..checked_product(fan_in, fan_out, "initializer matrix")?)
        .map(|_| rng.signed_unit() * scale)
        .collect())
}

fn initialized_bias(length: usize, rng: &mut SplitMix64) -> Vec<f32> {
    (0..length).map(|_| rng.signed_unit() * 0.01).collect()
}

#[derive(Clone, Copy, Debug)]
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        mix64(self.0)
    }

    fn signed_unit(&mut self) -> f32 {
        let fraction = ((self.next() >> 40) as u32) as f32 / 16_777_216.0;
        fraction.mul_add(2.0, -1.0)
    }
}

fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn deterministic_feature(decision: u64, logical_action: Option<usize>, index: usize) -> f32 {
    let action = logical_action.map_or(0xd1b5_4a32_d192_ed03, |value| {
        (value as u64)
            .wrapping_add(1)
            .wrapping_mul(0x94d0_49bb_1331_11eb)
    });
    let mixed = mix64(
        decision
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
            .wrapping_add(action)
            .wrapping_add((index as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9)),
    );
    let fraction = ((mixed >> 40) as u32) as f32 / 16_777_216.0;
    fraction.mul_add(1.5, -0.75)
}

fn hash_f32_iter(values: impl Iterator<Item = f32>) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for value in values {
        for byte in value.to_bits().to_le_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

fn max_abs(left: &[f32], right: &[f32]) -> Result<f32, Box<dyn Error>> {
    if left.len() != right.len() {
        return Err("comparison shape mismatch".into());
    }
    Ok(left
        .iter()
        .zip(right)
        .map(|(&a, &b)| (a - b).abs())
        .fold(0.0f32, f32::max))
}

struct DeviceParameter {
    name: &'static str,
    value: CudaSlice<f32>,
    gradient: CudaSlice<f32>,
    first_moment: CudaSlice<f32>,
    second_moment: CudaSlice<f32>,
}

impl DeviceParameter {
    fn new(
        name: &'static str,
        stream: &Arc<CudaStream>,
        host: &HostParameter,
    ) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            name,
            value: stream.clone_htod(&host.value)?,
            gradient: stream.alloc_zeros::<f32>(host.value.len())?,
            first_moment: stream.clone_htod(&host.first_moment)?,
            second_moment: stream.clone_htod(&host.second_moment)?,
        })
    }

    fn len(&self) -> usize {
        self.value.len()
    }

    fn download(&self, stream: &Arc<CudaStream>) -> Result<HostParameter, Box<dyn Error>> {
        Ok(HostParameter {
            value: stream.clone_dtoh(&self.value)?,
            gradient: stream.clone_dtoh(&self.gradient)?,
            first_moment: stream.clone_dtoh(&self.first_moment)?,
            second_moment: stream.clone_dtoh(&self.second_moment)?,
        })
    }
}

struct DeviceModel {
    state_w1: DeviceParameter,
    state_b1: DeviceParameter,
    state_w2: DeviceParameter,
    state_b2: DeviceParameter,
    action_w: DeviceParameter,
    action_b: DeviceParameter,
    scorer_state_w: DeviceParameter,
    scorer_action_w: DeviceParameter,
    scorer_b: DeviceParameter,
    scorer_out_w: DeviceParameter,
    value_w1: DeviceParameter,
    value_b1: DeviceParameter,
    value_out_w: DeviceParameter,
    value_out_b: DeviceParameter,
    adam_step: usize,
}

impl DeviceModel {
    fn new(stream: &Arc<CudaStream>, host: &HostModel) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            state_w1: DeviceParameter::new("state_w1", stream, &host.state_w1)?,
            state_b1: DeviceParameter::new("state_b1", stream, &host.state_b1)?,
            state_w2: DeviceParameter::new("state_w2", stream, &host.state_w2)?,
            state_b2: DeviceParameter::new("state_b2", stream, &host.state_b2)?,
            action_w: DeviceParameter::new("action_w", stream, &host.action_w)?,
            action_b: DeviceParameter::new("action_b", stream, &host.action_b)?,
            scorer_state_w: DeviceParameter::new("scorer_state_w", stream, &host.scorer_state_w)?,
            scorer_action_w: DeviceParameter::new(
                "scorer_action_w",
                stream,
                &host.scorer_action_w,
            )?,
            scorer_b: DeviceParameter::new("scorer_b", stream, &host.scorer_b)?,
            scorer_out_w: DeviceParameter::new("scorer_out_w", stream, &host.scorer_out_w)?,
            value_w1: DeviceParameter::new("value_w1", stream, &host.value_w1)?,
            value_b1: DeviceParameter::new("value_b1", stream, &host.value_b1)?,
            value_out_w: DeviceParameter::new("value_out_w", stream, &host.value_out_w)?,
            value_out_b: DeviceParameter::new("value_out_b", stream, &host.value_out_b)?,
            adam_step: host.adam_step,
        })
    }

    fn download(&self, stream: &Arc<CudaStream>) -> Result<HostModel, Box<dyn Error>> {
        let model = HostModel {
            state_w1: self.state_w1.download(stream)?,
            state_b1: self.state_b1.download(stream)?,
            state_w2: self.state_w2.download(stream)?,
            state_b2: self.state_b2.download(stream)?,
            action_w: self.action_w.download(stream)?,
            action_b: self.action_b.download(stream)?,
            scorer_state_w: self.scorer_state_w.download(stream)?,
            scorer_action_w: self.scorer_action_w.download(stream)?,
            scorer_b: self.scorer_b.download(stream)?,
            scorer_out_w: self.scorer_out_w.download(stream)?,
            value_w1: self.value_w1.download(stream)?,
            value_b1: self.value_b1.download(stream)?,
            value_out_w: self.value_out_w.download(stream)?,
            value_out_b: self.value_out_b.download(stream)?,
            adam_step: self.adam_step,
        };
        model.validate()?;
        Ok(model)
    }
}

struct DeviceBatch {
    batch: usize,
    total_actions: usize,
    offsets: CudaSlice<i32>,
    action_owner: CudaSlice<i32>,
    states: CudaSlice<f32>,
    actions: CudaSlice<f32>,
    selected_global: CudaSlice<i32>,
    terminal_returns: CudaSlice<f32>,
}

impl DeviceBatch {
    fn new(stream: &Arc<CudaStream>, host: &SyntheticBatch) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            batch: host.batch(),
            total_actions: host.total_actions(),
            offsets: stream.clone_htod(&host.offsets)?,
            action_owner: stream.clone_htod(&host.action_owner)?,
            states: stream.clone_htod(&host.states)?,
            actions: stream.clone_htod(&host.actions)?,
            selected_global: stream.clone_htod(&host.selected_global)?,
            terminal_returns: stream.clone_htod(&host.terminal_returns)?,
        })
    }
}

struct DeviceActivations {
    state_h1: CudaSlice<f32>,
    state_h2: CudaSlice<f32>,
    action_h: CudaSlice<f32>,
    state_for_actions: CudaSlice<f32>,
    scorer_h: CudaSlice<f32>,
    logits: CudaSlice<f32>,
    value_h: CudaSlice<f32>,
    values: CudaSlice<f32>,
}

impl DeviceActivations {
    fn new(
        stream: &Arc<CudaStream>,
        batch: usize,
        total_actions: usize,
    ) -> Result<Self, Box<dyn Error>> {
        let batch_hidden = checked_cuda_product(batch, HIDDEN, "activation batch-hidden")?;
        let action_hidden =
            checked_cuda_product(total_actions, HIDDEN, "activation action-hidden")?;
        Ok(Self {
            state_h1: stream.alloc_zeros::<f32>(batch_hidden)?,
            state_h2: stream.alloc_zeros::<f32>(batch_hidden)?,
            action_h: stream.alloc_zeros::<f32>(action_hidden)?,
            state_for_actions: stream.alloc_zeros::<f32>(action_hidden)?,
            scorer_h: stream.alloc_zeros::<f32>(action_hidden)?,
            logits: stream.alloc_zeros::<f32>(total_actions)?,
            value_h: stream.alloc_zeros::<f32>(batch_hidden)?,
            values: stream.alloc_zeros::<f32>(batch)?,
        })
    }

    fn download(&self, stream: &Arc<CudaStream>) -> Result<DownloadedActivations, Box<dyn Error>> {
        Ok(DownloadedActivations {
            logits: stream.clone_dtoh(&self.logits)?,
            values: stream.clone_dtoh(&self.values)?,
        })
    }
}

#[derive(Clone, Debug)]
struct DownloadedActivations {
    logits: Vec<f32>,
    values: Vec<f32>,
}

struct DeviceGradients {
    policy_terms: CudaSlice<f32>,
    value_terms: CudaSlice<f32>,
    totals: CudaSlice<f32>,
    d_logits: CudaSlice<f32>,
    d_values: CudaSlice<f32>,
    d_scorer_pre: CudaSlice<f32>,
    d_state_for_actions: CudaSlice<f32>,
    d_action_h: CudaSlice<f32>,
    d_h2_policy: CudaSlice<f32>,
    d_value_h: CudaSlice<f32>,
    d_h2_value: CudaSlice<f32>,
    d_state2_pre: CudaSlice<f32>,
    d_state_h1: CudaSlice<f32>,
    invalid: CudaSlice<u32>,
}

impl DeviceGradients {
    fn new(
        stream: &Arc<CudaStream>,
        batch: usize,
        total_actions: usize,
    ) -> Result<Self, Box<dyn Error>> {
        let batch_hidden = checked_cuda_product(batch, HIDDEN, "gradient batch-hidden")?;
        let action_hidden = checked_cuda_product(total_actions, HIDDEN, "gradient action-hidden")?;
        Ok(Self {
            policy_terms: stream.alloc_zeros::<f32>(batch)?,
            value_terms: stream.alloc_zeros::<f32>(batch)?,
            totals: stream.alloc_zeros::<f32>(3)?,
            d_logits: stream.alloc_zeros::<f32>(total_actions)?,
            d_values: stream.alloc_zeros::<f32>(batch)?,
            d_scorer_pre: stream.alloc_zeros::<f32>(action_hidden)?,
            d_state_for_actions: stream.alloc_zeros::<f32>(action_hidden)?,
            d_action_h: stream.alloc_zeros::<f32>(action_hidden)?,
            d_h2_policy: stream.alloc_zeros::<f32>(batch_hidden)?,
            d_value_h: stream.alloc_zeros::<f32>(batch_hidden)?,
            d_h2_value: stream.alloc_zeros::<f32>(batch_hidden)?,
            d_state2_pre: stream.alloc_zeros::<f32>(batch_hidden)?,
            d_state_h1: stream.alloc_zeros::<f32>(batch_hidden)?,
            invalid: stream.alloc_zeros::<u32>(1)?,
        })
    }
}

struct Kernels {
    bias_activation: CudaFunction,
    gather_state_for_actions: CudaFunction,
    terminal_loss_grad: CudaFunction,
    reduce_loss: CudaFunction,
    scale_rows_relu: CudaFunction,
    relu_grad_in_place: CudaFunction,
    combine_relu_grad: CudaFunction,
    reduce_action_owner: CudaFunction,
    bias_gradient: CudaFunction,
    check_finite: CudaFunction,
    adam_update: CudaFunction,
}

impl Kernels {
    fn compile(ctx: &Arc<CudaContext>) -> Result<Self, Box<dyn Error>> {
        let ptx = compile_ptx_with_opts(
            CUDA_SOURCE,
            CompileOptions {
                arch: Some("compute_89"),
                fmad: Some(true),
                ftz: Some(false),
                prec_div: Some(true),
                prec_sqrt: Some(true),
                use_fast_math: Some(false),
                name: Some("cuda_flat_training_capacity_v1.cu".into()),
                ..Default::default()
            },
        )?;
        let module = ctx.load_module(ptx)?;
        Ok(Self {
            bias_activation: module.load_function("bias_activation")?,
            gather_state_for_actions: module.load_function("gather_state_for_actions")?,
            terminal_loss_grad: module.load_function("terminal_loss_grad")?,
            reduce_loss: module.load_function("reduce_loss")?,
            scale_rows_relu: module.load_function("scale_rows_relu")?,
            relu_grad_in_place: module.load_function("relu_grad_in_place")?,
            combine_relu_grad: module.load_function("combine_relu_grad")?,
            reduce_action_owner: module.load_function("reduce_action_owner")?,
            bias_gradient: module.load_function("bias_gradient")?,
            check_finite: module.load_function("check_finite")?,
            adam_update: module.load_function("adam_update")?,
        })
    }
}

struct TrainingResources {
    // The handle is intentionally destroyed before buffers during close().
    blas: CudaBlas,
    kernels: Kernels,
    model: DeviceModel,
    batch: DeviceBatch,
    rollout: DeviceActivations,
    recompute: DeviceActivations,
    gradients: DeviceGradients,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FaultPoint {
    None,
    AfterRolloutForward,
}

struct TrainingService {
    // No raw pinned host pointer or CUDA graph is owned by this v1. All copies
    // use cudarc's tracked slices. close() retains resources when synchronization
    // fails. Drop can only attempt a best-effort drain because it cannot return
    // a driver error to the caller.
    stream: Arc<CudaStream>,
    resources: Option<TrainingResources>,
    device_name: String,
    device_memory_bytes: usize,
    poisoned: bool,
    closed: bool,
}

impl TrainingService {
    fn new(host_model: &HostModel, host_batch: &SyntheticBatch) -> Result<Self, Box<dyn Error>> {
        host_model.validate()?;
        host_batch.validate()?;
        let ctx = CudaContext::new(0)?;
        let compute_capability = ctx.compute_capability()?;
        if compute_capability != (8, 9) {
            return Err(format!(
                "synthetic v1 is compiled for compute_89, found {}.{}",
                compute_capability.0, compute_capability.1
            )
            .into());
        }
        let device_name = ctx.name()?;
        let device_memory_bytes = ctx.total_mem()?;
        let stream = ctx.new_stream()?;
        let blas = CudaBlas::new(stream.clone())?;
        unsafe {
            blas_sys::cublasSetMathMode(
                *blas.handle(),
                blas_sys::cublasMath_t::CUBLAS_PEDANTIC_MATH,
            )
            .result()?;
        }
        let kernels = Kernels::compile(&ctx)?;
        let model = DeviceModel::new(&stream, host_model)?;
        let batch = DeviceBatch::new(&stream, host_batch)?;
        let rollout = DeviceActivations::new(&stream, batch.batch, batch.total_actions)?;
        let recompute = DeviceActivations::new(&stream, batch.batch, batch.total_actions)?;
        let gradients = DeviceGradients::new(&stream, batch.batch, batch.total_actions)?;
        stream.synchronize()?;
        Ok(Self {
            stream,
            resources: Some(TrainingResources {
                blas,
                kernels,
                model,
                batch,
                rollout,
                recompute,
                gradients,
            }),
            device_name,
            device_memory_bytes,
            poisoned: false,
            closed: false,
        })
    }

    fn training_step(
        &mut self,
        value_coefficient: f32,
        adam: AdamConfig,
    ) -> Result<LossSummary, Box<dyn Error>> {
        self.training_step_with_fault(value_coefficient, adam, FaultPoint::None)
    }

    fn training_step_with_fault(
        &mut self,
        value_coefficient: f32,
        adam: AdamConfig,
        fault: FaultPoint,
    ) -> Result<LossSummary, Box<dyn Error>> {
        if self.closed {
            return Err("training service is closed".into());
        }
        if self.poisoned {
            return Err("training service is poisoned".into());
        }
        if !value_coefficient.is_finite() || value_coefficient < 0.0 {
            return Err("value coefficient must be finite and non-negative".into());
        }
        validate_adam(adam)?;
        let result = self.training_step_inner(value_coefficient, adam, fault);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }

    fn training_step_inner(
        &mut self,
        value_coefficient: f32,
        adam: AdamConfig,
        fault: FaultPoint,
    ) -> Result<LossSummary, Box<dyn Error>> {
        let resources = self
            .resources
            .as_mut()
            .ok_or("training resources are absent")?;
        self.stream
            .memcpy_htod(&[0u32], &mut resources.gradients.invalid)?;
        device_forward(
            &resources.blas,
            &resources.kernels,
            &resources.model,
            &resources.batch,
            &mut resources.rollout,
            &self.stream,
        )?;
        launch_check_finite(
            &self.stream,
            &resources.kernels.check_finite,
            &resources.rollout.logits,
            &mut resources.gradients.invalid,
        )?;
        launch_check_finite(
            &self.stream,
            &resources.kernels.check_finite,
            &resources.rollout.values,
            &mut resources.gradients.invalid,
        )?;
        if fault == FaultPoint::AfterRolloutForward {
            return Err("injected failure after asynchronous rollout forward".into());
        }
        device_forward(
            &resources.blas,
            &resources.kernels,
            &resources.model,
            &resources.batch,
            &mut resources.recompute,
            &self.stream,
        )?;
        launch_check_finite(
            &self.stream,
            &resources.kernels.check_finite,
            &resources.recompute.logits,
            &mut resources.gradients.invalid,
        )?;
        launch_check_finite(
            &self.stream,
            &resources.kernels.check_finite,
            &resources.recompute.values,
            &mut resources.gradients.invalid,
        )?;
        device_backward_and_adam(resources, &self.stream, value_coefficient, adam)?;
        self.stream.synchronize()?;
        let invalid = self
            .stream
            .clone_dtoh(&resources.gradients.invalid)?
            .into_iter()
            .next()
            .ok_or("finite flag copy was empty")?;
        let totals = self.stream.clone_dtoh(&resources.gradients.totals)?;
        if invalid != 0 {
            return Err("CUDA training step detected a non-finite or invalid value".into());
        }
        if totals.len() != 3 || !totals.iter().all(|value| value.is_finite()) {
            return Err("CUDA loss summary is invalid".into());
        }
        resources.model.adam_step =
            checked_sum(resources.model.adam_step, 1, "completed Adam step")?;
        checked_i32(resources.model.adam_step)?;
        Ok(LossSummary {
            policy_sum: totals[0],
            value_sum: totals[1],
            loss: totals[2],
        })
    }

    fn replace_batch(&mut self, host_batch: &SyntheticBatch) -> Result<(), Box<dyn Error>> {
        if self.closed {
            return Err("training service is closed".into());
        }
        if self.poisoned {
            return Err("training service is poisoned".into());
        }
        host_batch.validate()?;
        let resources = self
            .resources
            .as_mut()
            .ok_or("training resources are absent")?;
        if host_batch.batch() != resources.batch.batch
            || host_batch.total_actions() != resources.batch.total_actions
        {
            return Err("replacement batch must preserve decision and action dimensions".into());
        }
        self.stream.synchronize()?;
        let replacement = DeviceBatch::new(&self.stream, host_batch)?;
        resources.batch = replacement;
        if let Err(error) = self.stream.synchronize() {
            self.poisoned = true;
            return Err(error.into());
        }
        Ok(())
    }

    fn download_outputs(
        &self,
    ) -> Result<(DownloadedActivations, DownloadedActivations), Box<dyn Error>> {
        if self.closed {
            return Err("training service is closed".into());
        }
        let resources = self
            .resources
            .as_ref()
            .ok_or("training resources are absent")?;
        Ok((
            resources.rollout.download(&self.stream)?,
            resources.recompute.download(&self.stream)?,
        ))
    }

    fn download_model(&self) -> Result<HostModel, Box<dyn Error>> {
        if self.closed {
            return Err("training service is closed".into());
        }
        self.resources
            .as_ref()
            .ok_or_else(|| "training resources are absent".into())
            .and_then(|resources| resources.model.download(&self.stream))
    }

    fn close(&mut self) -> Result<(), Box<dyn Error>> {
        if self.closed {
            return Ok(());
        }
        let synchronize = self.stream.synchronize();
        // There are no graphs or pinned buffers in v1. If they are introduced,
        // they must be Options inside TrainingResources and explicitly taken in
        // graph -> pinned -> device-buffer order here, after synchronization.
        finalize_close_after_synchronize(&mut self.resources, &mut self.closed, synchronize)
            .map_err(Into::into)
    }
}

fn finalize_close_after_synchronize<T, E>(
    resources: &mut Option<T>,
    closed: &mut bool,
    synchronize: Result<(), E>,
) -> Result<(), E> {
    synchronize?;
    resources.take();
    *closed = true;
    Ok(())
}

impl Drop for TrainingService {
    fn drop(&mut self) {
        if !self.closed {
            // Best effort only: if synchronization fails, Rust will still drop
            // the owned fields after this method returns. Callers that need a
            // reportable drain guarantee must use close().
            if self.stream.synchronize().is_ok() {
                self.resources.take();
                self.closed = true;
            }
        }
    }
}

fn validate_adam(config: AdamConfig) -> Result<(), Box<dyn Error>> {
    if !config.learning_rate.is_finite() || config.learning_rate <= 0.0 {
        return Err("Adam learning rate must be finite and positive".into());
    }
    if !config.beta1.is_finite() || !(0.0..1.0).contains(&config.beta1) {
        return Err("Adam beta1 must be finite and in [0, 1)".into());
    }
    if !config.beta2.is_finite() || !(0.0..1.0).contains(&config.beta2) {
        return Err("Adam beta2 must be finite and in [0, 1)".into());
    }
    if !config.epsilon.is_finite() || config.epsilon <= 0.0 {
        return Err("Adam epsilon must be finite and positive".into());
    }
    Ok(())
}

fn device_forward(
    blas: &CudaBlas,
    kernels: &Kernels,
    model: &DeviceModel,
    batch: &DeviceBatch,
    activations: &mut DeviceActivations,
    stream: &Arc<CudaStream>,
) -> Result<(), Box<dyn Error>> {
    gemm_row_nn(
        blas,
        batch.batch,
        STATE_DIM,
        HIDDEN,
        &batch.states,
        &model.state_w1.value,
        &mut activations.state_h1,
        0.0,
    )?;
    launch_bias_activation(
        stream,
        &kernels.bias_activation,
        &mut activations.state_h1,
        &model.state_b1.value,
        batch.batch,
        HIDDEN,
        true,
    )?;
    gemm_row_nn(
        blas,
        batch.batch,
        HIDDEN,
        HIDDEN,
        &activations.state_h1,
        &model.state_w2.value,
        &mut activations.state_h2,
        0.0,
    )?;
    launch_bias_activation(
        stream,
        &kernels.bias_activation,
        &mut activations.state_h2,
        &model.state_b2.value,
        batch.batch,
        HIDDEN,
        true,
    )?;
    gemm_row_nn(
        blas,
        batch.total_actions,
        ACTION_DIM,
        HIDDEN,
        &batch.actions,
        &model.action_w.value,
        &mut activations.action_h,
        0.0,
    )?;
    launch_bias_activation(
        stream,
        &kernels.bias_activation,
        &mut activations.action_h,
        &model.action_b.value,
        batch.total_actions,
        HIDDEN,
        true,
    )?;
    launch_gather_state(
        stream,
        &kernels.gather_state_for_actions,
        &activations.state_h2,
        &batch.action_owner,
        &mut activations.state_for_actions,
        batch.total_actions,
    )?;
    gemm_row_nn(
        blas,
        batch.total_actions,
        HIDDEN,
        HIDDEN,
        &activations.state_for_actions,
        &model.scorer_state_w.value,
        &mut activations.scorer_h,
        0.0,
    )?;
    gemm_row_nn(
        blas,
        batch.total_actions,
        HIDDEN,
        HIDDEN,
        &activations.action_h,
        &model.scorer_action_w.value,
        &mut activations.scorer_h,
        1.0,
    )?;
    launch_bias_activation(
        stream,
        &kernels.bias_activation,
        &mut activations.scorer_h,
        &model.scorer_b.value,
        batch.total_actions,
        HIDDEN,
        true,
    )?;
    gemm_row_nn(
        blas,
        batch.total_actions,
        HIDDEN,
        1,
        &activations.scorer_h,
        &model.scorer_out_w.value,
        &mut activations.logits,
        0.0,
    )?;
    gemm_row_nn(
        blas,
        batch.batch,
        HIDDEN,
        HIDDEN,
        &activations.state_h2,
        &model.value_w1.value,
        &mut activations.value_h,
        0.0,
    )?;
    launch_bias_activation(
        stream,
        &kernels.bias_activation,
        &mut activations.value_h,
        &model.value_b1.value,
        batch.batch,
        HIDDEN,
        true,
    )?;
    gemm_row_nn(
        blas,
        batch.batch,
        HIDDEN,
        1,
        &activations.value_h,
        &model.value_out_w.value,
        &mut activations.values,
        0.0,
    )?;
    launch_bias_activation(
        stream,
        &kernels.bias_activation,
        &mut activations.values,
        &model.value_out_b.value,
        batch.batch,
        1,
        false,
    )?;
    Ok(())
}

fn device_backward_and_adam(
    resources: &mut TrainingResources,
    stream: &Arc<CudaStream>,
    value_coefficient: f32,
    adam: AdamConfig,
) -> Result<(), Box<dyn Error>> {
    let TrainingResources {
        blas,
        kernels,
        model,
        batch,
        recompute,
        gradients,
        ..
    } = resources;
    launch_terminal_loss(
        stream,
        &kernels.terminal_loss_grad,
        recompute,
        batch,
        gradients,
        value_coefficient,
    )?;
    launch_reduce_loss(
        stream,
        &kernels.reduce_loss,
        gradients,
        batch.batch,
        value_coefficient,
    )?;

    gemm_row_tn(
        blas,
        batch.total_actions,
        HIDDEN,
        1,
        &recompute.scorer_h,
        &gradients.d_logits,
        &mut model.scorer_out_w.gradient,
    )?;
    launch_scale_rows_relu(
        stream,
        &kernels.scale_rows_relu,
        &gradients.d_logits,
        &model.scorer_out_w.value,
        &recompute.scorer_h,
        &mut gradients.d_scorer_pre,
        batch.total_actions,
        HIDDEN,
    )?;
    gemm_row_tn(
        blas,
        batch.total_actions,
        HIDDEN,
        HIDDEN,
        &recompute.state_for_actions,
        &gradients.d_scorer_pre,
        &mut model.scorer_state_w.gradient,
    )?;
    gemm_row_tn(
        blas,
        batch.total_actions,
        HIDDEN,
        HIDDEN,
        &recompute.action_h,
        &gradients.d_scorer_pre,
        &mut model.scorer_action_w.gradient,
    )?;
    launch_bias_gradient(
        stream,
        &kernels.bias_gradient,
        &gradients.d_scorer_pre,
        &mut model.scorer_b.gradient,
        batch.total_actions,
        HIDDEN,
    )?;
    gemm_row_nt(
        blas,
        batch.total_actions,
        HIDDEN,
        HIDDEN,
        &gradients.d_scorer_pre,
        &model.scorer_state_w.value,
        &mut gradients.d_state_for_actions,
    )?;
    gemm_row_nt(
        blas,
        batch.total_actions,
        HIDDEN,
        HIDDEN,
        &gradients.d_scorer_pre,
        &model.scorer_action_w.value,
        &mut gradients.d_action_h,
    )?;
    launch_reduce_action_owner(
        stream,
        &kernels.reduce_action_owner,
        &gradients.d_state_for_actions,
        &batch.offsets,
        &mut gradients.d_h2_policy,
        batch.batch,
    )?;
    launch_relu_grad(
        stream,
        &kernels.relu_grad_in_place,
        &recompute.action_h,
        &mut gradients.d_action_h,
    )?;
    gemm_row_tn(
        blas,
        batch.total_actions,
        ACTION_DIM,
        HIDDEN,
        &batch.actions,
        &gradients.d_action_h,
        &mut model.action_w.gradient,
    )?;
    launch_bias_gradient(
        stream,
        &kernels.bias_gradient,
        &gradients.d_action_h,
        &mut model.action_b.gradient,
        batch.total_actions,
        HIDDEN,
    )?;

    gemm_row_tn(
        blas,
        batch.batch,
        HIDDEN,
        1,
        &recompute.value_h,
        &gradients.d_values,
        &mut model.value_out_w.gradient,
    )?;
    launch_bias_gradient(
        stream,
        &kernels.bias_gradient,
        &gradients.d_values,
        &mut model.value_out_b.gradient,
        batch.batch,
        1,
    )?;
    launch_scale_rows_relu(
        stream,
        &kernels.scale_rows_relu,
        &gradients.d_values,
        &model.value_out_w.value,
        &recompute.value_h,
        &mut gradients.d_value_h,
        batch.batch,
        HIDDEN,
    )?;
    gemm_row_tn(
        blas,
        batch.batch,
        HIDDEN,
        HIDDEN,
        &recompute.state_h2,
        &gradients.d_value_h,
        &mut model.value_w1.gradient,
    )?;
    launch_bias_gradient(
        stream,
        &kernels.bias_gradient,
        &gradients.d_value_h,
        &mut model.value_b1.gradient,
        batch.batch,
        HIDDEN,
    )?;
    gemm_row_nt(
        blas,
        batch.batch,
        HIDDEN,
        HIDDEN,
        &gradients.d_value_h,
        &model.value_w1.value,
        &mut gradients.d_h2_value,
    )?;
    launch_combine_relu_grad(
        stream,
        &kernels.combine_relu_grad,
        &gradients.d_h2_policy,
        &gradients.d_h2_value,
        &recompute.state_h2,
        &mut gradients.d_state2_pre,
    )?;
    gemm_row_tn(
        blas,
        batch.batch,
        HIDDEN,
        HIDDEN,
        &recompute.state_h1,
        &gradients.d_state2_pre,
        &mut model.state_w2.gradient,
    )?;
    launch_bias_gradient(
        stream,
        &kernels.bias_gradient,
        &gradients.d_state2_pre,
        &mut model.state_b2.gradient,
        batch.batch,
        HIDDEN,
    )?;
    gemm_row_nt(
        blas,
        batch.batch,
        HIDDEN,
        HIDDEN,
        &gradients.d_state2_pre,
        &model.state_w2.value,
        &mut gradients.d_state_h1,
    )?;
    launch_relu_grad(
        stream,
        &kernels.relu_grad_in_place,
        &recompute.state_h1,
        &mut gradients.d_state_h1,
    )?;
    gemm_row_tn(
        blas,
        batch.batch,
        STATE_DIM,
        HIDDEN,
        &batch.states,
        &gradients.d_state_h1,
        &mut model.state_w1.gradient,
    )?;
    launch_bias_gradient(
        stream,
        &kernels.bias_gradient,
        &gradients.d_state_h1,
        &mut model.state_b1.gradient,
        batch.batch,
        HIDDEN,
    )?;

    macro_rules! check_gradient {
        ($field:ident) => {
            launch_check_finite(
                stream,
                &kernels.check_finite,
                &model.$field.gradient,
                &mut gradients.invalid,
            )?;
        };
    }
    check_gradient!(state_w1);
    check_gradient!(state_b1);
    check_gradient!(state_w2);
    check_gradient!(state_b2);
    check_gradient!(action_w);
    check_gradient!(action_b);
    check_gradient!(scorer_state_w);
    check_gradient!(scorer_action_w);
    check_gradient!(scorer_b);
    check_gradient!(scorer_out_w);
    check_gradient!(value_w1);
    check_gradient!(value_b1);
    check_gradient!(value_out_w);
    check_gradient!(value_out_b);

    let next_step = checked_sum(model.adam_step, 1, "device Adam step")?;
    checked_i32(next_step)?;
    macro_rules! update {
        ($field:ident) => {
            launch_adam_update(
                stream,
                &kernels.adam_update,
                &mut model.$field,
                &mut gradients.invalid,
                next_step,
                adam,
            )?;
        };
    }
    update!(state_w1);
    update!(state_b1);
    update!(state_w2);
    update!(state_b2);
    update!(action_w);
    update!(action_b);
    update!(scorer_state_w);
    update!(scorer_action_w);
    update!(scorer_b);
    update!(scorer_out_w);
    update!(value_w1);
    update!(value_b1);
    update!(value_out_w);
    update!(value_out_b);
    Ok(())
}

#[allow(clippy::too_many_arguments)] // Matrix shape and beta are explicit safety inputs.
fn gemm_row_nn(
    blas: &CudaBlas,
    rows: usize,
    inner: usize,
    columns: usize,
    left: &CudaSlice<f32>,
    right: &CudaSlice<f32>,
    output: &mut CudaSlice<f32>,
    beta: f32,
) -> Result<(), Box<dyn Error>> {
    check_gemm_lengths(left, checked_product(rows, inner, "NN left")?, "NN left")?;
    check_gemm_lengths(
        right,
        checked_product(inner, columns, "NN right")?,
        "NN right",
    )?;
    check_gemm_lengths(
        output,
        checked_product(rows, columns, "NN output")?,
        "NN output",
    )?;
    unsafe {
        blas.gemm(
            GemmConfig {
                transa: blas_sys::cublasOperation_t::CUBLAS_OP_N,
                transb: blas_sys::cublasOperation_t::CUBLAS_OP_N,
                m: checked_i32(columns)?,
                n: checked_i32(rows)?,
                k: checked_i32(inner)?,
                alpha: 1.0,
                lda: checked_i32(columns)?,
                ldb: checked_i32(inner)?,
                beta,
                ldc: checked_i32(columns)?,
            },
            right,
            left,
            output,
        )?;
    }
    Ok(())
}

fn gemm_row_tn(
    blas: &CudaBlas,
    rows: usize,
    left_columns: usize,
    right_columns: usize,
    left: &CudaSlice<f32>,
    right: &CudaSlice<f32>,
    output: &mut CudaSlice<f32>,
) -> Result<(), Box<dyn Error>> {
    check_gemm_lengths(
        left,
        checked_product(rows, left_columns, "TN left")?,
        "TN left",
    )?;
    check_gemm_lengths(
        right,
        checked_product(rows, right_columns, "TN right")?,
        "TN right",
    )?;
    check_gemm_lengths(
        output,
        checked_product(left_columns, right_columns, "TN output")?,
        "TN output",
    )?;
    unsafe {
        blas.gemm(
            GemmConfig {
                transa: blas_sys::cublasOperation_t::CUBLAS_OP_N,
                transb: blas_sys::cublasOperation_t::CUBLAS_OP_T,
                m: checked_i32(right_columns)?,
                n: checked_i32(left_columns)?,
                k: checked_i32(rows)?,
                alpha: 1.0,
                lda: checked_i32(right_columns)?,
                ldb: checked_i32(left_columns)?,
                beta: 0.0,
                ldc: checked_i32(right_columns)?,
            },
            right,
            left,
            output,
        )?;
    }
    Ok(())
}

fn gemm_row_nt(
    blas: &CudaBlas,
    rows: usize,
    inner: usize,
    output_columns: usize,
    left: &CudaSlice<f32>,
    right: &CudaSlice<f32>,
    output: &mut CudaSlice<f32>,
) -> Result<(), Box<dyn Error>> {
    check_gemm_lengths(left, checked_product(rows, inner, "NT left")?, "NT left")?;
    check_gemm_lengths(
        right,
        checked_product(output_columns, inner, "NT right")?,
        "NT right",
    )?;
    check_gemm_lengths(
        output,
        checked_product(rows, output_columns, "NT output")?,
        "NT output",
    )?;
    unsafe {
        blas.gemm(
            GemmConfig {
                transa: blas_sys::cublasOperation_t::CUBLAS_OP_T,
                transb: blas_sys::cublasOperation_t::CUBLAS_OP_N,
                m: checked_i32(output_columns)?,
                n: checked_i32(rows)?,
                k: checked_i32(inner)?,
                alpha: 1.0,
                lda: checked_i32(inner)?,
                ldb: checked_i32(inner)?,
                beta: 0.0,
                ldc: checked_i32(output_columns)?,
            },
            right,
            left,
            output,
        )?;
    }
    Ok(())
}

fn check_gemm_lengths<T>(
    slice: &CudaSlice<T>,
    expected: usize,
    label: &str,
) -> Result<(), Box<dyn Error>> {
    if slice.len() != expected {
        return Err(format!("{label} length {} != {expected}", slice.len()).into());
    }
    Ok(())
}

fn checked_product(left: usize, right: usize, label: &str) -> Result<usize, Box<dyn Error>> {
    left.checked_mul(right)
        .ok_or_else(|| format!("{label} product overflow: {left} * {right}").into())
}

fn checked_sum(left: usize, right: usize, label: &str) -> Result<usize, Box<dyn Error>> {
    left.checked_add(right)
        .ok_or_else(|| format!("{label} sum overflow: {left} + {right}").into())
}

fn checked_cuda_product(left: usize, right: usize, label: &str) -> Result<usize, Box<dyn Error>> {
    let product = checked_product(left, right, label)?;
    checked_i32(product)?;
    Ok(product)
}

fn checked_i32(value: usize) -> Result<i32, Box<dyn Error>> {
    Ok(i32::try_from(value).map_err(|_| "CUDA matrix dimension exceeds i32")?)
}

fn launch_config(length: usize) -> Result<LaunchConfig, Box<dyn Error>> {
    if length == 0 {
        return Err("zero-length CUDA launch".into());
    }
    let blocks = length.div_ceil(CUDA_THREADS as usize);
    Ok(LaunchConfig {
        grid_dim: (u32::try_from(blocks)?, 1, 1),
        block_dim: (CUDA_THREADS, 1, 1),
        shared_mem_bytes: 0,
    })
}

fn launch_bias_activation(
    stream: &Arc<CudaStream>,
    function: &CudaFunction,
    matrix: &mut CudaSlice<f32>,
    bias: &CudaSlice<f32>,
    rows: usize,
    columns: usize,
    relu: bool,
) -> Result<(), Box<dyn Error>> {
    let elements = checked_cuda_product(rows, columns, "bias activation")?;
    if matrix.len() != elements || bias.len() != columns {
        return Err("bias activation shape mismatch".into());
    }
    let rows = checked_i32(rows)?;
    let columns = checked_i32(columns)?;
    let relu = i32::from(relu);
    let config = launch_config(matrix.len())?;
    let mut launch = stream.launch_builder(function);
    launch.arg(&mut *matrix);
    launch.arg(bias);
    launch.arg(&rows);
    launch.arg(&columns);
    launch.arg(&relu);
    unsafe { launch.launch(config)? };
    Ok(())
}

fn launch_gather_state(
    stream: &Arc<CudaStream>,
    function: &CudaFunction,
    state_hidden: &CudaSlice<f32>,
    action_owner: &CudaSlice<i32>,
    gathered: &mut CudaSlice<f32>,
    total_actions: usize,
) -> Result<(), Box<dyn Error>> {
    if !state_hidden.len().is_multiple_of(HIDDEN)
        || action_owner.len() != total_actions
        || gathered.len() != checked_cuda_product(total_actions, HIDDEN, "state gather output")?
    {
        return Err("state gather shape mismatch".into());
    }
    let total_actions_i32 = checked_i32(total_actions)?;
    let config = launch_config(gathered.len())?;
    let mut launch = stream.launch_builder(function);
    launch.arg(state_hidden);
    launch.arg(action_owner);
    launch.arg(&mut *gathered);
    launch.arg(&total_actions_i32);
    unsafe { launch.launch(config)? };
    Ok(())
}

fn launch_terminal_loss(
    stream: &Arc<CudaStream>,
    function: &CudaFunction,
    activations: &DeviceActivations,
    batch: &DeviceBatch,
    gradients: &mut DeviceGradients,
    value_coefficient: f32,
) -> Result<(), Box<dyn Error>> {
    let batch_i32 = checked_i32(batch.batch)?;
    let mut launch = stream.launch_builder(function);
    launch.arg(&activations.logits);
    launch.arg(&activations.values);
    launch.arg(&batch.offsets);
    launch.arg(&batch.selected_global);
    launch.arg(&batch.terminal_returns);
    launch.arg(&value_coefficient);
    launch.arg(&batch_i32);
    launch.arg(&mut gradients.policy_terms);
    launch.arg(&mut gradients.value_terms);
    launch.arg(&mut gradients.d_logits);
    launch.arg(&mut gradients.d_values);
    launch.arg(&mut gradients.invalid);
    unsafe { launch.launch(launch_config(batch.batch)?)? };
    Ok(())
}

fn launch_reduce_loss(
    stream: &Arc<CudaStream>,
    function: &CudaFunction,
    gradients: &mut DeviceGradients,
    batch: usize,
    value_coefficient: f32,
) -> Result<(), Box<dyn Error>> {
    let batch = checked_i32(batch)?;
    let mut launch = stream.launch_builder(function);
    launch.arg(&gradients.policy_terms);
    launch.arg(&gradients.value_terms);
    launch.arg(&value_coefficient);
    launch.arg(&batch);
    launch.arg(&mut gradients.totals);
    launch.arg(&mut gradients.invalid);
    unsafe {
        launch.launch(LaunchConfig {
            grid_dim: (1, 1, 1),
            block_dim: (1, 1, 1),
            shared_mem_bytes: 0,
        })?
    };
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn launch_scale_rows_relu(
    stream: &Arc<CudaStream>,
    function: &CudaFunction,
    row_scale: &CudaSlice<f32>,
    weight: &CudaSlice<f32>,
    activation: &CudaSlice<f32>,
    output: &mut CudaSlice<f32>,
    rows: usize,
    columns: usize,
) -> Result<(), Box<dyn Error>> {
    let elements = checked_cuda_product(rows, columns, "scaled row gradient")?;
    if row_scale.len() != rows
        || weight.len() != columns
        || activation.len() != elements
        || output.len() != elements
    {
        return Err("scaled row gradient shape mismatch".into());
    }
    let rows = checked_i32(rows)?;
    let columns = checked_i32(columns)?;
    let config = launch_config(output.len())?;
    let mut launch = stream.launch_builder(function);
    launch.arg(row_scale);
    launch.arg(weight);
    launch.arg(activation);
    launch.arg(&mut *output);
    launch.arg(&rows);
    launch.arg(&columns);
    unsafe { launch.launch(config)? };
    Ok(())
}

fn launch_relu_grad(
    stream: &Arc<CudaStream>,
    function: &CudaFunction,
    activation: &CudaSlice<f32>,
    gradient: &mut CudaSlice<f32>,
) -> Result<(), Box<dyn Error>> {
    if activation.len() != gradient.len() {
        return Err("ReLU gradient shape mismatch".into());
    }
    let length = checked_i32(gradient.len())?;
    let config = launch_config(gradient.len())?;
    let mut launch = stream.launch_builder(function);
    launch.arg(activation);
    launch.arg(&mut *gradient);
    launch.arg(&length);
    unsafe { launch.launch(config)? };
    Ok(())
}

fn launch_combine_relu_grad(
    stream: &Arc<CudaStream>,
    function: &CudaFunction,
    left: &CudaSlice<f32>,
    right: &CudaSlice<f32>,
    activation: &CudaSlice<f32>,
    output: &mut CudaSlice<f32>,
) -> Result<(), Box<dyn Error>> {
    let length = output.len();
    if left.len() != length || right.len() != length || activation.len() != length {
        return Err("combined ReLU gradient shape mismatch".into());
    }
    let length_i32 = checked_i32(length)?;
    let mut launch = stream.launch_builder(function);
    launch.arg(left);
    launch.arg(right);
    launch.arg(activation);
    launch.arg(output);
    launch.arg(&length_i32);
    unsafe { launch.launch(launch_config(length)?)? };
    Ok(())
}

fn launch_reduce_action_owner(
    stream: &Arc<CudaStream>,
    function: &CudaFunction,
    action_gradient: &CudaSlice<f32>,
    offsets: &CudaSlice<i32>,
    state_gradient: &mut CudaSlice<f32>,
    batch: usize,
) -> Result<(), Box<dyn Error>> {
    let offset_count = checked_sum(batch, 1, "action-owner offsets")?;
    let state_elements = checked_cuda_product(batch, HIDDEN, "action-owner state gradient")?;
    if !action_gradient.len().is_multiple_of(HIDDEN)
        || offsets.len() != offset_count
        || state_gradient.len() != state_elements
    {
        return Err("action-owner reduction shape mismatch".into());
    }
    let batch_i32 = checked_i32(batch)?;
    let config = launch_config(state_gradient.len())?;
    let mut launch = stream.launch_builder(function);
    launch.arg(action_gradient);
    launch.arg(offsets);
    launch.arg(&mut *state_gradient);
    launch.arg(&batch_i32);
    unsafe { launch.launch(config)? };
    Ok(())
}

fn launch_bias_gradient(
    stream: &Arc<CudaStream>,
    function: &CudaFunction,
    matrix_gradient: &CudaSlice<f32>,
    bias_gradient: &mut CudaSlice<f32>,
    rows: usize,
    columns: usize,
) -> Result<(), Box<dyn Error>> {
    let elements = checked_cuda_product(rows, columns, "bias gradient")?;
    if matrix_gradient.len() != elements || bias_gradient.len() != columns {
        return Err("bias gradient shape mismatch".into());
    }
    let rows = checked_i32(rows)?;
    let columns_i32 = checked_i32(columns)?;
    let mut launch = stream.launch_builder(function);
    launch.arg(matrix_gradient);
    launch.arg(bias_gradient);
    launch.arg(&rows);
    launch.arg(&columns_i32);
    unsafe { launch.launch(launch_config(columns)?)? };
    Ok(())
}

fn launch_check_finite(
    stream: &Arc<CudaStream>,
    function: &CudaFunction,
    values: &CudaSlice<f32>,
    invalid: &mut CudaSlice<u32>,
) -> Result<(), Box<dyn Error>> {
    if invalid.len() != 1 {
        return Err("finite flag shape mismatch".into());
    }
    let length = checked_i32(values.len())?;
    let mut launch = stream.launch_builder(function);
    launch.arg(values);
    launch.arg(&length);
    launch.arg(invalid);
    unsafe { launch.launch(launch_config(values.len())?)? };
    Ok(())
}

fn launch_adam_update(
    stream: &Arc<CudaStream>,
    function: &CudaFunction,
    parameter: &mut DeviceParameter,
    invalid: &mut CudaSlice<u32>,
    step: usize,
    config: AdamConfig,
) -> Result<(), Box<dyn Error>> {
    if step == 0 || step > i32::MAX as usize {
        return Err("CUDA Adam step is outside 1..=i32::MAX".into());
    }
    let length = parameter.len();
    if parameter.gradient.len() != length
        || parameter.first_moment.len() != length
        || parameter.second_moment.len() != length
    {
        return Err(format!("Adam shapes differ for {}", parameter.name).into());
    }
    let length_i32 = checked_i32(length)?;
    let inverse_bias1 = 1.0 / (1.0 - config.beta1.powi(i32::try_from(step)?));
    let inverse_bias2 = 1.0 / (1.0 - config.beta2.powi(i32::try_from(step)?));
    let mut launch = stream.launch_builder(function);
    launch.arg(&mut parameter.value);
    launch.arg(&parameter.gradient);
    launch.arg(&mut parameter.first_moment);
    launch.arg(&mut parameter.second_moment);
    launch.arg(&length_i32);
    launch.arg(&config.learning_rate);
    launch.arg(&config.beta1);
    launch.arg(&config.beta2);
    launch.arg(&inverse_bias1);
    launch.arg(&inverse_bias2);
    launch.arg(&config.epsilon);
    launch.arg(invalid);
    unsafe { launch.launch(launch_config(length)?)? };
    Ok(())
}

fn cpu_detached_surrogate_loss(
    model: &HostModel,
    batch: &SyntheticBatch,
    detached_advantages: &[f32],
) -> Result<f32, Box<dyn Error>> {
    let activations = cpu_forward(model, batch)?;
    Ok(cpu_terminal_loss_with_detached_advantages(
        &activations.logits,
        &activations.values,
        batch,
        VALUE_COEFFICIENT,
        detached_advantages,
    )?
    .loss
    .loss)
}

fn validate_central_difference_gradients() -> Result<serde_json::Value, Box<dyn Error>> {
    let batch = SyntheticBatch::small_golden()?;
    let base_model = HostModel::deterministic()?;
    let base_activations = cpu_forward(&base_model, &batch)?;
    let detached_advantages = batch
        .terminal_returns
        .iter()
        .zip(&base_activations.values)
        .map(|(target, value)| target - value)
        .collect::<Vec<_>>();

    let mut analytic_model = base_model.clone();
    cpu_train_step(
        &mut analytic_model,
        &batch,
        VALUE_COEFFICIENT,
        AdamConfig::default(),
    )?;
    let representatives = analytic_model
        .parameters()
        .map(|(name, parameter)| {
            let (index, gradient) = parameter
                .gradient
                .iter()
                .copied()
                .enumerate()
                .max_by(|left, right| left.1.abs().total_cmp(&right.1.abs()))
                .ok_or_else(|| format!("gradient-check tensor {name} is empty"))?;
            Ok::<_, Box<dyn Error>>((name, index, gradient))
        })
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;

    let mut reports = Vec::with_capacity(representatives.len());
    let mut maximum_absolute_error = 0.0f32;
    let mut maximum_relative_error = 0.0f32;
    for (name, index, analytic) in representatives {
        let mut plus = base_model.clone();
        let plus_parameter = plus
            .parameter_mut(name)
            .ok_or_else(|| format!("unknown gradient-check tensor {name}"))?;
        let original = *plus_parameter
            .value
            .get(index)
            .ok_or_else(|| format!("gradient-check index {index} out of bounds for {name}"))?;
        plus_parameter.value[index] = original + GRADIENT_CHECK_STEP;
        plus.validate()?;

        let mut minus = base_model.clone();
        let minus_parameter = minus
            .parameter_mut(name)
            .ok_or_else(|| format!("unknown gradient-check tensor {name}"))?;
        minus_parameter.value[index] = original - GRADIENT_CHECK_STEP;
        minus.validate()?;

        let plus_loss = cpu_detached_surrogate_loss(&plus, &batch, &detached_advantages)?;
        let minus_loss = cpu_detached_surrogate_loss(&minus, &batch, &detached_advantages)?;
        let numerical = (plus_loss - minus_loss) / (2.0 * GRADIENT_CHECK_STEP);
        let absolute_error = (analytic - numerical).abs();
        let relative_scale = analytic.abs().max(numerical.abs()).max(1.0e-6);
        let relative_error = absolute_error / relative_scale;
        let pass = absolute_error <= GRADIENT_CHECK_ABS_TOLERANCE
            || relative_error <= GRADIENT_CHECK_REL_TOLERANCE;
        if !pass {
            return Err(format!(
                "central-difference gradient check failed for {name}[{index}]: analytic={analytic} numerical={numerical} abs={absolute_error} rel={relative_error}"
            )
            .into());
        }
        maximum_absolute_error = maximum_absolute_error.max(absolute_error);
        maximum_relative_error = maximum_relative_error.max(relative_error);
        reports.push(json!({
            "tensor": name,
            "selection": "largest-absolute-analytic-gradient",
            "index": index,
            "analytic": analytic,
            "central_difference": numerical,
            "absolute_error": absolute_error,
            "relative_error": relative_error,
            "pass": pass,
        }));
    }
    Ok(json!({
        "objective": "terminal-only REINFORCE/value surrogate with base-step advantage held detached across perturbations",
        "step": GRADIENT_CHECK_STEP,
        "absolute_tolerance": GRADIENT_CHECK_ABS_TOLERANCE,
        "relative_tolerance": GRADIENT_CHECK_REL_TOLERANCE,
        "acceptance": "absolute <= tolerance OR relative <= tolerance",
        "tensor_count": reports.len(),
        "maximum_absolute_error": maximum_absolute_error,
        "maximum_relative_error": maximum_relative_error,
        "all_pass": true,
        "per_tensor": reports,
    }))
}

fn validate_tiny_gradient_adam() -> Result<serde_json::Value, Box<dyn Error>> {
    let gradients = [1.0e-12f32, -2.0e-12f32, 3.0e-12f32];
    let config = AdamConfig::default();
    let mut parameter = HostParameter::new(vec![0.0]);
    let mut value_f64 = 0.0f64;
    let mut first_f64 = 0.0f64;
    let mut second_f64 = 0.0f64;
    let mut maximum_error = 0.0f64;
    let mut steps = Vec::with_capacity(gradients.len());
    for (index, gradient) in gradients.into_iter().enumerate() {
        let step = checked_sum(index, 1, "tiny-gradient Adam step")?;
        parameter.gradient[0] = gradient;
        cpu_adam_update(&mut parameter, step, config)?;

        let gradient_f64 = f64::from(gradient);
        let beta1 = f64::from(config.beta1);
        let beta2 = f64::from(config.beta2);
        first_f64 = beta1 * first_f64 + (1.0 - beta1) * gradient_f64;
        second_f64 = beta2 * second_f64 + (1.0 - beta2) * gradient_f64 * gradient_f64;
        let corrected_first = first_f64 / (1.0 - beta1.powi(i32::try_from(step)?));
        let corrected_second = second_f64 / (1.0 - beta2.powi(i32::try_from(step)?));
        value_f64 -= f64::from(config.learning_rate) * corrected_first
            / (corrected_second.sqrt() + f64::from(config.epsilon));
        let error = (f64::from(parameter.value[0]) - value_f64).abs();
        if error > TINY_GRADIENT_ADAM_F64_TOLERANCE {
            return Err(format!(
                "tiny-gradient Adam f32/f64 check failed at step {step}: error={error}"
            )
            .into());
        }
        maximum_error = maximum_error.max(error);
        steps.push(json!({
            "step": step,
            "gradient": gradient,
            "f32_value": parameter.value[0],
            "f64_value": value_f64,
            "absolute_error": error,
        }));
    }
    Ok(json!({
        "gradient_clamp_or_threshold": false,
        "epsilon": config.epsilon,
        "f64_reference_tolerance": TINY_GRADIENT_ADAM_F64_TOLERANCE,
        "maximum_f32_f64_absolute_error": maximum_error,
        "steps": steps,
        "independent_python_or_decimal_golden": "hold-not-implemented",
    }))
}

#[derive(Clone, Copy, Debug)]
struct ModelDifference {
    values: f32,
    gradients: f32,
    first_moments: f32,
    second_moments: f32,
    worst_value_parameter: &'static str,
    worst_value_index: usize,
    worst_cpu_value: f32,
    worst_gpu_value: f32,
    worst_cpu_gradient: f32,
    worst_gpu_gradient: f32,
}

fn compare_models(left: &HostModel, right: &HostModel) -> Result<ModelDifference, Box<dyn Error>> {
    left.validate()?;
    right.validate()?;
    if left.adam_step != right.adam_step {
        return Err(format!(
            "Adam steps differ: {} != {}",
            left.adam_step, right.adam_step
        )
        .into());
    }
    let mut difference = ModelDifference {
        values: 0.0,
        gradients: 0.0,
        first_moments: 0.0,
        second_moments: 0.0,
        worst_value_parameter: "",
        worst_value_index: 0,
        worst_cpu_value: 0.0,
        worst_gpu_value: 0.0,
        worst_cpu_gradient: 0.0,
        worst_gpu_gradient: 0.0,
    };
    for ((left_name, left_parameter), (right_name, right_parameter)) in
        left.parameters().into_iter().zip(right.parameters())
    {
        if left_name != right_name {
            return Err(format!("parameter names differ: {left_name} != {right_name}").into());
        }
        for (index, (&left_value, &right_value)) in left_parameter
            .value
            .iter()
            .zip(&right_parameter.value)
            .enumerate()
        {
            let found = (left_value - right_value).abs();
            if found > difference.values {
                difference.values = found;
                difference.worst_value_parameter = left_name;
                difference.worst_value_index = index;
                difference.worst_cpu_value = left_value;
                difference.worst_gpu_value = right_value;
                difference.worst_cpu_gradient = left_parameter.gradient[index];
                difference.worst_gpu_gradient = right_parameter.gradient[index];
            }
        }
        difference.gradients = difference.gradients.max(max_abs(
            &left_parameter.gradient,
            &right_parameter.gradient,
        )?);
        difference.first_moments = difference.first_moments.max(max_abs(
            &left_parameter.first_moment,
            &right_parameter.first_moment,
        )?);
        difference.second_moments = difference.second_moments.max(max_abs(
            &left_parameter.second_moment,
            &right_parameter.second_moment,
        )?);
    }
    Ok(difference)
}

fn validate_same_process_gpu_reproducibility(
    initial: &HostModel,
) -> Result<serde_json::Value, Box<dyn Error>> {
    initial.validate()?;
    let batch_a = SyntheticBatch::small_golden()?;
    let batch_b = SyntheticBatch::small_golden_variant()?;
    let sequence = [
        ("small-golden-a", &batch_a),
        ("small-golden-b", &batch_b),
        ("small-golden-a", &batch_a),
    ];
    let mut first = TrainingService::new(initial, &batch_a)?;
    let mut second = TrainingService::new(initial, &batch_a)?;
    let first_device = first.device_name.clone();
    let second_device = second.device_name.clone();
    let first_memory = first.device_memory_bytes;
    let second_memory = second.device_memory_bytes;
    if first_device != second_device || first_memory != second_memory {
        return Err("reproducibility services did not resolve the same CUDA device".into());
    }
    let mut step_reports = Vec::with_capacity(sequence.len());
    for (index, (batch_name, batch)) in sequence.into_iter().enumerate() {
        let step = checked_sum(index, 1, "reproducibility step")?;
        if index != 0 {
            first.replace_batch(batch)?;
            second.replace_batch(batch)?;
        }
        let first_loss = first.training_step(VALUE_COEFFICIENT, AdamConfig::default())?;
        let second_loss = second.training_step(VALUE_COEFFICIENT, AdamConfig::default())?;
        if first_loss.policy_sum.to_bits() != second_loss.policy_sum.to_bits()
            || first_loss.value_sum.to_bits() != second_loss.value_sum.to_bits()
            || first_loss.loss.to_bits() != second_loss.loss.to_bits()
        {
            return Err(format!(
                "same-process GPU losses diverged at reproducibility step {}",
                step
            )
            .into());
        }
        let first_model = first.download_model()?;
        let second_model = second.download_model()?;
        let difference = compare_models(&first_model, &second_model)?;
        if difference.values != 0.0
            || difference.gradients != 0.0
            || difference.first_moments != 0.0
            || difference.second_moments != 0.0
        {
            return Err(format!(
                "same-process GPU model state diverged at reproducibility step {}: {difference:?}",
                step
            )
            .into());
        }
        let first_hash = first_model.value_hash();
        let second_hash = second_model.value_hash();
        if first_hash != second_hash {
            return Err(format!(
                "same-process GPU hashes diverged at reproducibility step {}",
                step
            )
            .into());
        }
        step_reports.push(json!({
            "step": step,
            "batch": batch_name,
            "first_weight_hash": first_hash,
            "second_weight_hash": second_hash,
            "loss_bits": {
                "policy_sum": first_loss.policy_sum.to_bits(),
                "value_sum": first_loss.value_sum.to_bits(),
                "loss": first_loss.loss.to_bits(),
            },
            "values_max_abs": difference.values,
            "gradients_max_abs": difference.gradients,
            "first_moments_max_abs": difference.first_moments,
            "second_moments_max_abs": difference.second_moments,
            "bit_exact": true,
        }));
    }
    let final_model = first.download_model()?;
    if final_model.adam_step != sequence.len() || final_model.value_hash() == initial.value_hash() {
        return Err("multi-step reproducibility sequence did not change weights and step".into());
    }
    first.close()?;
    second.close()?;
    Ok(json!({
        "claim_scope": "same-process same-device/backend independently constructed services only",
        "cross_process_or_restart_reproducibility": "not-evaluated",
        "services": 2,
        "full_training_steps": sequence.len(),
        "deterministic_batch_sequence": ["small-golden-a", "small-golden-b", "small-golden-a"],
        "first_device": first_device,
        "second_device": second_device,
        "first_device_memory_bytes": first_memory,
        "second_device_memory_bytes": second_memory,
        "all_steps_bit_exact": true,
        "per_step": step_reports,
    }))
}

fn validate_cpu_gpu_once() -> Result<serde_json::Value, Box<dyn Error>> {
    let batch = SyntheticBatch::small_golden()?;
    let initial = HostModel::deterministic()?;
    let initial_hash = initial.value_hash();
    let mut cpu_model = initial.clone();
    let cpu = cpu_train_step(
        &mut cpu_model,
        &batch,
        VALUE_COEFFICIENT,
        AdamConfig::default(),
    )?;
    let mut gpu = TrainingService::new(&initial, &batch)?;
    let gpu_loss = gpu.training_step(VALUE_COEFFICIENT, AdamConfig::default())?;
    let (gpu_rollout, gpu_recompute) = gpu.download_outputs()?;
    let gpu_model = gpu.download_model()?;
    let difference = compare_models(&cpu_model, &gpu_model)?;
    let rollout_logits_max_abs = max_abs(&cpu.rollout.logits, &gpu_rollout.logits)?;
    let rollout_values_max_abs = max_abs(&cpu.rollout.values, &gpu_rollout.values)?;
    let recompute_logits_max_abs = max_abs(&cpu.recompute.logits, &gpu_recompute.logits)?;
    let recompute_values_max_abs = max_abs(&cpu.recompute.values, &gpu_recompute.values)?;
    let gpu_repeat_logits_max_abs = max_abs(&gpu_rollout.logits, &gpu_recompute.logits)?;
    let gpu_repeat_values_max_abs = max_abs(&gpu_rollout.values, &gpu_recompute.values)?;
    let loss_max_abs = [
        (cpu.loss.policy_sum - gpu_loss.policy_sum).abs(),
        (cpu.loss.value_sum - gpu_loss.value_sum).abs(),
        (cpu.loss.loss - gpu_loss.loss).abs(),
    ]
    .into_iter()
    .fold(0.0f32, f32::max);
    if rollout_logits_max_abs > 5.0e-4
        || rollout_values_max_abs > 5.0e-4
        || recompute_logits_max_abs > 5.0e-4
        || recompute_values_max_abs > 5.0e-4
        || gpu_repeat_logits_max_abs > 1.0e-6
        || gpu_repeat_values_max_abs > 1.0e-6
        || loss_max_abs > 2.0e-3
        || difference.gradients > 2.0e-3
        || difference.first_moments > 2.0e-4
        || difference.second_moments > 2.0e-5
        || difference.values > UPDATED_PARAMETER_CPU_GPU_TOLERANCE
    {
        return Err(format!(
            "CPU/GPU validation exceeded tolerance: outputs=({rollout_logits_max_abs}, \
             {rollout_values_max_abs}, {recompute_logits_max_abs}, \
             {recompute_values_max_abs}) repeat=({gpu_repeat_logits_max_abs}, \
             {gpu_repeat_values_max_abs}) loss={loss_max_abs} model={difference:?}"
        )
        .into());
    }
    if initial_hash == cpu_model.value_hash() || initial_hash == gpu_model.value_hash() {
        return Err("Adam did not change model weights".into());
    }
    gpu.close()?;
    let central_difference_gradients = validate_central_difference_gradients()?;
    let tiny_gradient_adam = validate_tiny_gradient_adam()?;
    let same_process_gpu_reproducibility = validate_same_process_gpu_reproducibility(&initial)?;
    Ok(json!({
        "scope": "synthetic-cuda-flat-training-capacity-v1-correctness",
        "model_contract": {
            "version": MODEL_CONTRACT_VERSION,
            "kind": "explicitly-synthetic-provisional-capacity-model",
            "earlier_forward_spike_checkpoint_compatible": false,
            "earlier_forward_spike_initializer_compatible": false,
            "checkpoint_translation": "none",
            "staleness": "zero-staleness-duplicate-forward-only",
        },
        "not_a_claim_about": [
            "production_model_parity",
            "raw_game_state_encoding",
            "games_per_second",
            "science_readiness",
            "training_throughput",
            "cross-process-or-restart-bit-reproducibility",
            "checkpoint-translation-from-earlier-forward-spike"
        ],
        "base_commit": BASE_COMMIT,
        "state_dim": STATE_DIM,
        "action_dim": ACTION_DIM,
        "hidden_dim": HIDDEN,
        "parameters": PARAMETER_COUNT,
        "batch_decisions": batch.batch(),
        "total_actions": batch.total_actions(),
        "terminal_returns": batch.terminal_returns,
        "selected_global": batch.selected_global,
        "value_coefficient": VALUE_COEFFICIENT,
        "advantage_detached": true,
        "loss_reduction": "(sum(policy) + value_coefficient * sum(value_squared_error)) / decisions",
        "optimizer": {
            "contract_version": NATIVE_ADAM_CONTRACT_VERSION,
            "name": "Adam",
            "learning_rate": LEARNING_RATE,
            "beta1": ADAM_BETA1,
            "beta2": ADAM_BETA2,
            "epsilon": ADAM_EPSILON,
            "weight_decay": 0.0,
            "gradient_clamp_or_threshold": false,
        },
        "parity_contract": {
            "same_process_same_device_backend_independent_services": "bit-exact-for-reported-three-step-a-b-a-sequence",
            "cross_process_or_restart": "not-evaluated",
            "cpu_reference_vs_gpu": "tolerance-parity-not-bit-identity",
            "updated_parameter_max_abs_tolerance": UPDATED_PARAMETER_CPU_GPU_TOLERANCE,
            "updated_parameter_tolerance_pass": difference.values <= UPDATED_PARAMETER_CPU_GPU_TOLERANCE,
        },
        "actor_forward_passes": 1,
        "learner_recompute_forward_passes": 1,
        "learner_backward_passes": 1,
        "adam_updates": 1,
        "cpu_loss": {
            "policy_sum": cpu.loss.policy_sum,
            "value_sum": cpu.loss.value_sum,
            "loss": cpu.loss.loss,
        },
        "gpu_loss": {
            "policy_sum": gpu_loss.policy_sum,
            "value_sum": gpu_loss.value_sum,
            "loss": gpu_loss.loss,
        },
        "cpu_gpu_max_abs": {
            "rollout_logits": rollout_logits_max_abs,
            "rollout_values": rollout_values_max_abs,
            "recompute_logits": recompute_logits_max_abs,
            "recompute_values": recompute_values_max_abs,
            "loss": loss_max_abs,
            "updated_values": difference.values,
            "gradients": difference.gradients,
            "first_moments": difference.first_moments,
            "second_moments": difference.second_moments,
            "worst_updated_value": {
                "parameter": difference.worst_value_parameter,
                "index": difference.worst_value_index,
                "cpu_value": difference.worst_cpu_value,
                "gpu_value": difference.worst_gpu_value,
                "cpu_gradient": difference.worst_cpu_gradient,
                "gpu_gradient": difference.worst_gpu_gradient,
            },
        },
        "gpu_forward_recompute_max_abs": {
            "logits": gpu_repeat_logits_max_abs,
            "values": gpu_repeat_values_max_abs,
        },
        "initial_weight_hash": initial_hash,
        "cpu_updated_weight_hash": cpu_model.value_hash(),
        "gpu_updated_weight_hash": gpu_model.value_hash(),
        "central_difference_gradient_checks": central_difference_gradients,
        "tiny_gradient_adam": tiny_gradient_adam,
        "same_process_gpu_reproducibility": same_process_gpu_reproducibility,
        "independent_python_or_decimal_golden_closure": "hold-not-implemented",
        "lifecycle": {
            "ownership": "tracked-cudarc-copies; no-graphs; no-raw-pinned-pointers",
            "close": "resources-retained-unless-synchronization-succeeds",
            "drop": "best-effort-drain-only; errors-cannot-be-reported",
        },
        "cublas_math_mode": "CUBLAS_PEDANTIC_MATH",
    }))
}

fn gate_description() -> serde_json::Value {
    json!({
        "scope": "synthetic-cuda-flat-training-capacity-v1-external-gate-description",
        "status": "external-clean-revision-only-not-executable-here",
        "rollout_decisions_per_second_demand": ROLLOUT_DECISIONS_PER_SECOND_DEMAND,
        "learner_epoch_multiplier": LEARNER_EPOCH_MULTIPLIER,
        "forward_passes_per_rollout_decision": FORWARD_PASSES_PER_ROLLOUT_DECISION,
        "forward_evaluation_demand_per_second": ROLLOUT_DECISIONS_PER_SECOND_DEMAND * FORWARD_PASSES_PER_ROLLOUT_DECISION as f64,
        "complete_training_decisions_per_second_gate": PROPOSED_COMPLETE_TRAINING_DECISIONS_PER_SECOND_GATE,
        "headroom_multiplier": PROPOSED_CAPACITY_HEADROOM,
        "complete_training_decision_definition": "one actor forward plus one learner recompute, backward, and Adam contribution for one selected decision",
        "required_workload_before_execution": "external clean-revision harness with provenance-bound fixed ragged Rally histogram",
        "single_run_enforcement_available": false,
        "proposed_confirmation": {
            "batch": 512,
            "independent_process_repeats": 5,
            "minimum_seconds_per_repeat": 30,
            "hard_statistic": "minimum complete-training-decisions-per-second across accepted repeats",
            "invalid_steps_allowed": 0,
            "external_cpu_busy_fraction_max": 0.05,
            "gate_is_fail_closed": true,
        },
        "exclusions": [
            "raw GameState encoding",
            "actor scheduler",
            "sampler",
            "replay",
            "checkpoint persistence",
            "games/s"
        ],
    })
}

fn run() -> Result<(), Box<dyn Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments
        .iter()
        .any(|argument| argument == "--describe-gate")
    {
        if arguments.len() != 1 {
            return Err("--describe-gate cannot be combined with other arguments".into());
        }
        println!("{}", gate_description());
        return Ok(());
    }
    if !arguments.is_empty() {
        return Err("supported argument: --describe-gate".into());
    }
    println!("{}", gate_description());
    println!("{}", validate_cpu_gpu_once()?);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("CUDA_FLAT_TRAINING_CAPACITY_V1_ERROR: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_batch_is_closed_and_provenance_stable() {
        let batch = SyntheticBatch::small_golden().unwrap();
        assert_eq!(batch.offsets, [0, 2, 5, 9]);
        assert_eq!(batch.selected_global, [0, 4, 6]);
        assert_eq!(batch.action_owner, [0, 0, 1, 1, 1, 2, 2, 2, 2]);
        assert_eq!(batch.terminal_returns, [1.0, -1.0, 0.0]);
        batch.validate().unwrap();
        assert!(SyntheticBatch::new(&[2], &[2], &[1]).is_err());
        assert!(SyntheticBatch::new(&[0], &[0], &[1]).is_err());
        assert!(SyntheticBatch::new(&[2], &[0], &[2]).is_err());
        let variant = SyntheticBatch::small_golden_variant().unwrap();
        variant.validate().unwrap();
        assert_eq!(variant.offsets, batch.offsets);
        assert_ne!(variant.states, batch.states);
        assert_ne!(variant.selected_global, batch.selected_global);
    }

    #[test]
    fn provisional_model_parameter_count_and_initializer_are_stable() {
        let first = HostModel::deterministic().unwrap();
        let second = HostModel::deterministic().unwrap();
        assert_eq!(first.parameter_count().unwrap(), PARAMETER_COUNT);
        assert_eq!(first.value_hash(), second.value_hash());
        assert!(first.all_finite());
    }

    #[test]
    fn host_validation_rejects_shape_moment_finiteness_and_index_drift() {
        let mut wrong_shape = HostModel::deterministic().unwrap();
        wrong_shape.state_w1.gradient.pop();
        assert!(wrong_shape.validate().is_err());

        let mut non_finite = HostModel::deterministic().unwrap();
        non_finite.value_b1.first_moment[0] = f32::NAN;
        assert!(non_finite.validate().is_err());

        let mut negative_second = HostModel::deterministic().unwrap();
        negative_second.action_b.second_moment[0] = -1.0;
        assert!(negative_second.validate().is_err());

        let mut invalid_batch = SyntheticBatch::small_golden().unwrap();
        invalid_batch.offsets[1] = -1;
        assert!(invalid_batch.validate().is_err());
        assert!(checked_product(usize::MAX, 2, "test product").is_err());
        assert!(checked_sum(usize::MAX, 1, "test sum").is_err());
        assert!(checked_cuda_product(i32::MAX as usize, 2, "test CUDA product").is_err());
    }

    #[test]
    fn provisional_parameter_layout_has_no_policy_output_bias() {
        let model = HostModel::deterministic().unwrap();
        let ordered_names = model.parameters().map(|(name, _)| name);
        assert_eq!(
            ordered_names,
            [
                "state_w1",
                "state_b1",
                "state_w2",
                "state_b2",
                "action_w",
                "action_b",
                "scorer_state_w",
                "scorer_action_w",
                "scorer_b",
                "scorer_out_w",
                "value_w1",
                "value_b1",
                "value_out_w",
                "value_out_b",
            ]
        );
        assert_eq!(model.parameter_count().unwrap(), 156_097);
    }

    #[test]
    fn native_adam_contract_uses_epsilon_without_thresholding_gradients() {
        assert_eq!(NATIVE_ADAM_CONTRACT_VERSION, "native-adam-epsilon-1e-5-v1");
        assert_eq!(ADAM_EPSILON.to_bits(), 1.0e-5f32.to_bits());
        assert_eq!(
            UPDATED_PARAMETER_CPU_GPU_TOLERANCE.to_bits(),
            5.0e-6f32.to_bits()
        );
        assert_eq!(
            AdamConfig::default().epsilon.to_bits(),
            ADAM_EPSILON.to_bits()
        );

        let tiny_gradient = 1.0e-12f32;
        let mut parameter = HostParameter::new(vec![0.0]);
        parameter.gradient[0] = tiny_gradient;
        cpu_adam_update(&mut parameter, 1, AdamConfig::default()).unwrap();
        assert_eq!(parameter.gradient[0].to_bits(), tiny_gradient.to_bits());
        assert_ne!(parameter.first_moment[0], 0.0);
        assert_ne!(parameter.second_moment[0], 0.0);
        assert_ne!(parameter.value[0], 0.0);
        let report = validate_tiny_gradient_adam().unwrap();
        assert_eq!(report["steps"].as_array().unwrap().len(), 3);
        assert_eq!(report["gradient_clamp_or_threshold"], false);
    }

    #[test]
    fn detached_surrogate_central_differences_cover_all_parameter_tensors() {
        let report = validate_central_difference_gradients().unwrap();
        assert_eq!(report["tensor_count"], 14);
        assert_eq!(report["all_pass"], true);
        assert_eq!(report["per_tensor"].as_array().unwrap().len(), 14);
    }

    #[test]
    fn uniform_logit_shift_is_irrelevant() {
        let batch = SyntheticBatch::small_golden().unwrap();
        let logits = vec![-0.5, 0.25, -1.0, 0.0, 0.5, -0.75, -0.25, 0.25, 0.75];
        let shifted_logits = logits.iter().map(|value| value + 8.0).collect::<Vec<_>>();
        let values = vec![0.25, -0.5, 0.0];
        let original =
            cpu_terminal_loss_and_output_gradients(&logits, &values, &batch, VALUE_COEFFICIENT)
                .unwrap();
        let shifted = cpu_terminal_loss_and_output_gradients(
            &shifted_logits,
            &values,
            &batch,
            VALUE_COEFFICIENT,
        )
        .unwrap();
        assert_eq!(
            original.loss.policy_sum.to_bits(),
            shifted.loss.policy_sum.to_bits()
        );
        assert_eq!(
            original.loss.value_sum.to_bits(),
            shifted.loss.value_sum.to_bits()
        );
        assert_eq!(original.loss.loss.to_bits(), shifted.loss.loss.to_bits());
        assert_eq!(original.d_logits, shifted.d_logits);
        assert_eq!(original.d_values, shifted.d_values);
    }

    #[test]
    fn cpu_reference_runs_actor_forward_recompute_backward_and_adam() {
        let batch = SyntheticBatch::small_golden().unwrap();
        let mut model = HostModel::deterministic().unwrap();
        let initial_hash = model.value_hash();
        let result =
            cpu_train_step(&mut model, &batch, VALUE_COEFFICIENT, AdamConfig::default()).unwrap();
        // Pinned independently of the CUDA/cuBLAS path. These exact f32 and
        // ordered-byte hashes make CPU semantic drift fail before GPU parity.
        assert_eq!(initial_hash, "cbe3fbd898ee96bb");
        assert_eq!(model.value_hash(), "17cd7f5e9f4536a9");
        assert_eq!(
            result.loss.policy_sum.to_bits(),
            (-0.130_485_01f32).to_bits()
        );
        assert_eq!(result.loss.value_sum.to_bits(), 2.208_056_2f32.to_bits());
        assert_eq!(result.loss.loss.to_bits(), 0.324_514_36f32.to_bits());
        assert_eq!(
            hash_f32_iter(result.rollout.logits.iter().copied()),
            "e0c6387fbf357905"
        );
        assert_eq!(
            hash_f32_iter(result.rollout.values.iter().copied()),
            "7add18ce7f7be28f"
        );
        assert_eq!(model.adam_step, 1);
        assert_ne!(model.value_hash(), initial_hash);
        assert_eq!(result.rollout.logits, result.recompute.logits);
        assert_eq!(result.rollout.values, result.recompute.values);
        assert!(result.loss.loss.is_finite());
        assert!(model.all_finite());
    }

    #[test]
    fn detached_advantage_does_not_update_value_only_parameters_when_coefficient_is_zero() {
        let batch = SyntheticBatch::small_golden().unwrap();
        let mut model = HostModel::deterministic().unwrap();
        let before = model.clone();
        cpu_train_step(&mut model, &batch, 0.0, AdamConfig::default()).unwrap();
        for (name, before_parameter, after_parameter) in [
            ("value_w1", &before.value_w1, &model.value_w1),
            ("value_b1", &before.value_b1, &model.value_b1),
            ("value_out_w", &before.value_out_w, &model.value_out_w),
            ("value_out_b", &before.value_out_b, &model.value_out_b),
        ] {
            assert_eq!(
                before_parameter.value, after_parameter.value,
                "{name} changed through detached advantage"
            );
            assert!(after_parameter.gradient.iter().all(|value| *value == 0.0));
        }
        assert_ne!(before.scorer_out_w.value, model.scorer_out_w.value);
    }

    #[test]
    fn proposed_gate_includes_epoch_multiplier_and_has_not_been_run() {
        assert_eq!(LEARNER_EPOCH_MULTIPLIER, 1);
        assert_eq!(FORWARD_PASSES_PER_ROLLOUT_DECISION, 2);
        assert_eq!(
            ROLLOUT_DECISIONS_PER_SECOND_DEMAND * FORWARD_PASSES_PER_ROLLOUT_DECISION as f64,
            1_146_000.0
        );
        assert_eq!(
            PROPOSED_COMPLETE_TRAINING_DECISIONS_PER_SECOND_GATE,
            687_600.0
        );
        assert_eq!(
            gate_description()["status"],
            "external-clean-revision-only-not-executable-here"
        );
        assert_eq!(
            gate_description()["single_run_enforcement_available"],
            false
        );
    }

    #[test]
    fn gpu_small_batch_matches_independent_cpu_reference() {
        let report = validate_cpu_gpu_once().unwrap();
        assert_eq!(
            report["optimizer"]["contract_version"],
            NATIVE_ADAM_CONTRACT_VERSION
        );
        assert_eq!(
            report["parity_contract"]["cpu_reference_vs_gpu"],
            "tolerance-parity-not-bit-identity"
        );
        assert_eq!(
            report["parity_contract"]["same_process_same_device_backend_independent_services"],
            "bit-exact-for-reported-three-step-a-b-a-sequence"
        );
        assert_eq!(
            report["parity_contract"]["updated_parameter_tolerance_pass"],
            true
        );
        assert_eq!(report["model_contract"]["version"], MODEL_CONTRACT_VERSION);
        assert_eq!(
            report["same_process_gpu_reproducibility"]["full_training_steps"],
            3
        );
        assert_eq!(
            report["same_process_gpu_reproducibility"]["all_steps_bit_exact"],
            true
        );
        assert_eq!(
            report["central_difference_gradient_checks"]["tensor_count"],
            14
        );
        println!("{report}");
    }

    #[test]
    fn gpu_row_major_gemm_helpers_cover_non_square_shapes() {
        let ctx = CudaContext::new(0).unwrap();
        let stream = ctx.new_stream().unwrap();
        let blas = CudaBlas::new(stream.clone()).unwrap();
        unsafe {
            blas_sys::cublasSetMathMode(
                *blas.handle(),
                blas_sys::cublasMath_t::CUBLAS_PEDANTIC_MATH,
            )
            .result()
            .unwrap();
        }

        let nn_left = vec![1.0, 2.0, 3.0, -1.0, 0.5, 4.0]; // 2 x 3
        let nn_right = vec![
            0.5, 1.0, -2.0, 3.0, 1.5, -1.0, 0.25, 2.0, -0.5, 4.0, 1.0, -3.0,
        ]; // 3 x 4
        let nn_expected = linear_no_bias(&nn_left, 2, 3, &nn_right, 4).unwrap();
        let nn_left_dev = stream.clone_htod(&nn_left).unwrap();
        let nn_right_dev = stream.clone_htod(&nn_right).unwrap();
        let mut nn_output = stream.alloc_zeros::<f32>(8).unwrap();
        gemm_row_nn(
            &blas,
            2,
            3,
            4,
            &nn_left_dev,
            &nn_right_dev,
            &mut nn_output,
            0.0,
        )
        .unwrap();
        assert!(max_abs(&stream.clone_dtoh(&nn_output).unwrap(), &nn_expected).unwrap() <= 1.0e-6);

        let tn_left = vec![1.0, 2.0, -1.0, 0.5, 3.0, -2.0]; // 3 x 2
        let tn_right = vec![
            0.5, 1.0, -2.0, 3.0, 1.5, -1.0, 0.25, 2.0, -0.5, 4.0, 1.0, -3.0,
        ]; // 3 x 4
        let tn_expected = matmul_tn(&tn_left, &tn_right, 3, 2, 4).unwrap();
        let tn_left_dev = stream.clone_htod(&tn_left).unwrap();
        let tn_right_dev = stream.clone_htod(&tn_right).unwrap();
        let mut tn_output = stream.alloc_zeros::<f32>(8).unwrap();
        gemm_row_tn(&blas, 3, 2, 4, &tn_left_dev, &tn_right_dev, &mut tn_output).unwrap();
        assert!(max_abs(&stream.clone_dtoh(&tn_output).unwrap(), &tn_expected).unwrap() <= 1.0e-6);

        let nt_left = vec![1.0, 2.0, 3.0, -1.0, 0.5, 4.0]; // 2 x 3
        let nt_right = vec![
            0.5, 1.0, -2.0, 3.0, 1.5, -1.0, 0.25, 2.0, -0.5, 4.0, 1.0, -3.0,
        ]; // 4 x 3
        let nt_expected = matmul_nt(&nt_left, &nt_right, 2, 3, 4).unwrap();
        let nt_left_dev = stream.clone_htod(&nt_left).unwrap();
        let nt_right_dev = stream.clone_htod(&nt_right).unwrap();
        let mut nt_output = stream.alloc_zeros::<f32>(8).unwrap();
        gemm_row_nt(&blas, 2, 3, 4, &nt_left_dev, &nt_right_dev, &mut nt_output).unwrap();
        assert!(max_abs(&stream.clone_dtoh(&nt_output).unwrap(), &nt_expected).unwrap() <= 1.0e-6);
        stream.synchronize().unwrap();
    }

    #[test]
    fn same_process_gpu_three_step_changing_batch_sequence_is_bit_exact() {
        let initial = HostModel::deterministic().unwrap();
        let report = validate_same_process_gpu_reproducibility(&initial).unwrap();
        assert_eq!(report["full_training_steps"], 3);
        assert_eq!(report["all_steps_bit_exact"], true);
        assert_eq!(report["per_step"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn close_state_is_finalized_only_after_successful_synchronization() {
        let mut resources = Some(7u32);
        let mut closed = false;
        let error = finalize_close_after_synchronize(
            &mut resources,
            &mut closed,
            Err::<(), _>("injected synchronize failure"),
        )
        .unwrap_err();
        assert_eq!(error, "injected synchronize failure");
        assert_eq!(resources, Some(7));
        assert!(!closed);

        finalize_close_after_synchronize(&mut resources, &mut closed, Ok::<_, &'static str>(()))
            .unwrap();
        assert_eq!(resources, None);
        assert!(closed);
    }

    #[test]
    fn injected_async_failure_is_drained_before_resources_are_destroyed() {
        let batch = SyntheticBatch::small_golden().unwrap();
        let model = HostModel::deterministic().unwrap();
        let mut service = TrainingService::new(&model, &batch).unwrap();
        let error = service
            .training_step_with_fault(
                VALUE_COEFFICIENT,
                AdamConfig::default(),
                FaultPoint::AfterRolloutForward,
            )
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("injected failure after asynchronous rollout forward"));
        service.close().unwrap();
        service.close().unwrap();
        assert!(service
            .training_step(VALUE_COEFFICIENT, AdamConfig::default())
            .is_err());
    }
}
