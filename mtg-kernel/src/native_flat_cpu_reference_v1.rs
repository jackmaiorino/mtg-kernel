//! Experimental fixed-shape CPU reference for the synthetic CUDA capacity diagnostic.
//!
//! This is a mechanical extraction of the diagnostic's standard-library CPU
//! oracle. It is not the Flat Policy model, the real `terminal_reinforce_value/v3`
//! loss, a dynamic-shape backend, a transactional optimizer, a checkpoint
//! format, or a production trainer API.

use std::error::Error;

pub(crate) const STATE_DIM: usize = 2_048;
pub(crate) const ACTION_DIM: usize = 128;
pub(crate) const HIDDEN: usize = 64;
pub(crate) const PARAMETER_COUNT: usize = 156_097;
pub(crate) const WEIGHT_SEED: u64 = 0x4207_c0de_7150_1009;
pub(crate) const VALUE_COEFFICIENT: f32 = 0.5;
pub(crate) const LEARNING_RATE: f32 = 1.0e-3;
pub(crate) const ADAM_BETA1: f32 = 0.9;
pub(crate) const ADAM_BETA2: f32 = 0.999;
pub(crate) const ADAM_EPSILON: f32 = 1.0e-5;

#[derive(Clone, Copy, Debug)]
pub(crate) struct DenseBatchViewV1<'a> {
    offsets: &'a [i32],
    action_owner: &'a [i32],
    states: &'a [f32],
    actions: &'a [f32],
    selected_global: &'a [i32],
    terminal_returns: &'a [f32],
}

impl<'a> DenseBatchViewV1<'a> {
    pub(crate) fn from_slices_unvalidated(
        offsets: &'a [i32],
        action_owner: &'a [i32],
        states: &'a [f32],
        actions: &'a [f32],
        selected_global: &'a [i32],
        terminal_returns: &'a [f32],
    ) -> Self {
        Self {
            offsets,
            action_owner,
            states,
            actions,
            selected_global,
            terminal_returns,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), Box<dyn Error>> {
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
            .chain(self.actions)
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

#[derive(Clone, Copy, Debug)]
pub(crate) struct AdamConfig {
    pub(crate) learning_rate: f32,
    pub(crate) beta1: f32,
    pub(crate) beta2: f32,
    pub(crate) epsilon: f32,
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
pub(crate) struct HostParameter {
    pub(crate) value: Vec<f32>,
    pub(crate) gradient: Vec<f32>,
    pub(crate) first_moment: Vec<f32>,
    pub(crate) second_moment: Vec<f32>,
}

impl HostParameter {
    pub(crate) fn new(value: Vec<f32>) -> Self {
        let length = value.len();
        Self {
            value,
            gradient: vec![0.0; length],
            first_moment: vec![0.0; length],
            second_moment: vec![0.0; length],
        }
    }

    pub(crate) fn validate(&self, name: &str, expected: usize) -> Result<(), Box<dyn Error>> {
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
pub(crate) struct HostModel {
    pub(crate) state_w1: HostParameter,
    pub(crate) state_b1: HostParameter,
    pub(crate) state_w2: HostParameter,
    pub(crate) state_b2: HostParameter,
    pub(crate) action_w: HostParameter,
    pub(crate) action_b: HostParameter,
    pub(crate) scorer_state_w: HostParameter,
    pub(crate) scorer_action_w: HostParameter,
    pub(crate) scorer_b: HostParameter,
    pub(crate) scorer_out_w: HostParameter,
    pub(crate) value_w1: HostParameter,
    pub(crate) value_b1: HostParameter,
    pub(crate) value_out_w: HostParameter,
    pub(crate) value_out_b: HostParameter,
    pub(crate) adam_step: usize,
}

impl HostModel {
    pub(crate) fn deterministic() -> Result<Self, Box<dyn Error>> {
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

    pub(crate) fn parameter_count(&self) -> Result<usize, Box<dyn Error>> {
        self.parameter_values()
            .iter()
            .try_fold(0usize, |total, values| {
                checked_sum(total, values.len(), "host parameter count")
            })
    }

    pub(crate) fn parameters(&self) -> [(&'static str, &HostParameter); 14] {
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

    pub(crate) fn parameter_values(&self) -> [&[f32]; 14] {
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

    pub(crate) fn parameter_mut(&mut self, name: &str) -> Option<&mut HostParameter> {
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

    pub(crate) fn validate(&self) -> Result<(), Box<dyn Error>> {
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

    pub(crate) fn all_finite(&self) -> bool {
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

    pub(crate) fn value_hash(&self) -> String {
        hash_f32_iter(
            self.parameter_values()
                .into_iter()
                .flat_map(|values| values.iter().copied()),
        )
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CpuActivations {
    pub(crate) state_h1: Vec<f32>,
    pub(crate) state_h2: Vec<f32>,
    pub(crate) action_h: Vec<f32>,
    pub(crate) state_for_actions: Vec<f32>,
    pub(crate) scorer_h: Vec<f32>,
    pub(crate) logits: Vec<f32>,
    pub(crate) value_h: Vec<f32>,
    pub(crate) values: Vec<f32>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LossSummary {
    pub(crate) policy_sum: f32,
    pub(crate) value_sum: f32,
    pub(crate) loss: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct CpuOutputGradients {
    pub(crate) loss: LossSummary,
    pub(crate) d_logits: Vec<f32>,
    pub(crate) d_values: Vec<f32>,
}

#[derive(Clone, Debug)]
pub(crate) struct CpuStepResult {
    pub(crate) rollout: CpuActivations,
    pub(crate) recompute: CpuActivations,
    pub(crate) loss: LossSummary,
}

pub(crate) fn cpu_forward(
    model: &HostModel,
    batch: &DenseBatchViewV1<'_>,
) -> Result<CpuActivations, Box<dyn Error>> {
    model.validate()?;
    batch.validate()?;
    let state_h1 = linear_relu(
        batch.states,
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
        batch.actions,
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
pub(crate) fn cpu_terminal_loss_and_output_gradients(
    logits: &[f32],
    values: &[f32],
    batch: &DenseBatchViewV1<'_>,
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
pub(crate) fn cpu_terminal_loss_with_detached_advantages(
    logits: &[f32],
    values: &[f32],
    batch: &DenseBatchViewV1<'_>,
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
pub(crate) fn cpu_train_step(
    model: &mut HostModel,
    batch: &DenseBatchViewV1<'_>,
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
        batch.actions,
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
    model.state_w1.gradient = matmul_tn(batch.states, &d_state_h1, batch_size, STATE_DIM, HIDDEN)?;
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

pub(crate) fn cpu_adam_update(
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

pub(crate) fn linear_no_bias(
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

pub(crate) fn matmul_tn(
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

pub(crate) fn matmul_nt(
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

pub(crate) fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
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

pub(crate) fn max_abs(left: &[f32], right: &[f32]) -> Result<f32, Box<dyn Error>> {
    if left.len() != right.len() {
        return Err("comparison shape mismatch".into());
    }
    Ok(left
        .iter()
        .zip(right)
        .map(|(&a, &b)| (a - b).abs())
        .fold(0.0f32, f32::max))
}

pub(crate) fn validate_adam(config: AdamConfig) -> Result<(), Box<dyn Error>> {
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

pub(crate) fn checked_product(
    left: usize,
    right: usize,
    label: &str,
) -> Result<usize, Box<dyn Error>> {
    left.checked_mul(right)
        .ok_or_else(|| format!("{label} product overflow: {left} * {right}").into())
}

pub(crate) fn checked_sum(left: usize, right: usize, label: &str) -> Result<usize, Box<dyn Error>> {
    left.checked_add(right)
        .ok_or_else(|| format!("{label} sum overflow: {left} + {right}").into())
}

pub(crate) fn checked_i32(value: usize) -> Result<i32, Box<dyn Error>> {
    Ok(i32::try_from(value).map_err(|_| "CUDA matrix dimension exceeds i32")?)
}

pub(crate) fn cpu_detached_surrogate_loss(
    model: &HostModel,
    batch: &DenseBatchViewV1<'_>,
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
