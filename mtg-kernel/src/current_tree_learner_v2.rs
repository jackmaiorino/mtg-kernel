//! Actor-critic successor of `current_tree_learner_v1` for the Foundations
//! Limited campaign (`docs/design/limited_campaign_v1.md`, Sections 3.2 and
//! 3.3). The v1 file and its contract stay unchanged, so the bounded PR 586
//! artifacts keep their identity; this module reads the same authenticated
//! public packets through `AuthenticatedInputV1`.
//!
//! Every transcendental the learner needs (tanh, exp, ln, square root, cosine)
//! is computed by the private functions at the end of this file from IEEE 754
//! `+`, `-`, `*`, `/`, comparisons and bit manipulation only, in a fixed
//! operation order, so a checkpoint has the same bits on every platform. No
//! function here calls the platform math library. They are built the way
//! `deterministic_math_v1` builds its `f32` functions (range reduction against
//! ln 2, Taylor polynomials, exact powers of two from exponent bits), at `f64`
//! precision. Reductions over a batch run in the canonical
//! `(pair, leg, seat, decision)` order whatever order the trajectories were
//! collected in.
//!
//! This module does not collect games, sample actions or publish checkpoints.
use crate::current_mixed_policy_v1::{Error, NumericTree, Result};
use crate::current_tree_learner_v1::{feature_identity_v1, AuthenticatedInputV1};
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

/// The primary campaign profile, hidden width 32 (design Section 3.2), with
/// the libm-free arithmetic the research lead's ruling requires.
pub const CONTRACT_V2: &str =
    "current-tree-learner/v2/f64/input20/hidden32/flags6/value1/gae-adam/libm-free-v1";
/// The width ablation profile, hidden width 16, kept compiled next to the
/// primary profile.
pub const CONTRACT_V2_H16: &str =
    "current-tree-learner/v2/f64/input20/hidden16/flags6/value1/gae-adam/libm-free-v1";
/// The zero-critic diagnostic control of design Section 3.2, which runs on the
/// unchanged `CONTRACT_V1` learner through [`pair_centered_terminal_rewards_v1`].
pub const PAIR_CENTERED_CONTROL_V1: &str = "pair-centered-reinforce/v1";
const INIT_TAG_V2: &[u8] = b"mtg-kernel/current-tree-learner-v2-init/v1\0";
const STATE_TAG_V2: &[u8] = b"mtg-kernel/current-tree-learner-v2-state/v1\0";
const INPUT: usize = 20;
const FLAG_COUNT: usize = 6;

/// `20 H + H H + H` encoder, `3 H + 6 + 1` bilinear policy head and `H + 1`
/// value head parameters.
pub const fn parameter_count_v2(hidden: usize) -> usize {
    hidden * hidden + 25 * hidden + 8
}
pub const PARAMETER_COUNT_V2: usize = parameter_count_v2(32);
pub const PARAMETER_COUNT_V2_H16: usize = parameter_count_v2(16);

const fn contract_for(hidden: usize) -> &'static str {
    match hidden {
        32 => CONTRACT_V2,
        16 => CONTRACT_V2_H16,
        _ => panic!("current_tree_learner_v2 has profiles for hidden widths 16 and 32 only"),
    }
}

/// Parameter offsets. The policy part has the v1 layout at width `H`; the
/// value head follows it.
#[derive(Clone, Copy)]
struct Layout {
    hidden: usize,
}
impl Layout {
    const fn recurrent(self) -> usize {
        INPUT * self.hidden
    }
    const fn bias(self) -> usize {
        self.recurrent() + self.hidden * self.hidden
    }
    const fn head(self) -> usize {
        self.bias() + self.hidden
    }
    const fn flags(self) -> usize {
        self.head() + 3 * self.hidden
    }
    const fn policy_bias(self) -> usize {
        self.flags() + FLAG_COUNT
    }
    const fn value_weights(self) -> usize {
        self.policy_bias() + 1
    }
    const fn value_bias(self) -> usize {
        self.value_weights() + self.hidden
    }
}

fn feature_identity_for(contract: &'static str) -> String {
    static WIDE: OnceLock<String> = OnceLock::new();
    static NARROW: OnceLock<String> = OnceLock::new();
    let cell = if contract == CONTRACT_V2_H16 {
        &NARROW
    } else {
        &WIDE
    };
    cell.get_or_init(|| {
        let mut hash = Sha256::new();
        hash.update(contract);
        hash.update(feature_identity_v1());
        hash.update(include_bytes!("current_tree_learner_v2.rs"));
        format!("{:x}", hash.finalize())
    })
    .clone()
}
/// Hashes the contract string, the v1 input identity (which covers the
/// numeric packet encoder) and this file's own bytes.
pub fn feature_identity_v2() -> String {
    feature_identity_for(CONTRACT_V2)
}
pub fn feature_identity_v2_h16() -> String {
    feature_identity_for(CONTRACT_V2_H16)
}

#[derive(Clone, Debug, PartialEq)]
pub struct TreeModelV2<const H: usize> {
    parameters: Vec<f64>,
}
pub type ModelV2 = TreeModelV2<32>;
pub type ModelV2H16 = TreeModelV2<16>;

/// Logits over the decision's full legal menu, in menu order, and the state
/// value in `(-1, 1)`.
#[derive(Clone, Debug, PartialEq)]
pub struct PolicyValueV2 {
    pub logits: Vec<f64>,
    pub value: f64,
}

struct TreeTape<const H: usize> {
    inputs: Vec<[f64; INPUT]>,
    parents: Vec<Option<usize>>,
    hidden: Vec<[f64; H]>,
    pooled: [f64; H],
}
struct ForwardTape<const H: usize> {
    state: TreeTape<H>,
    actions: Vec<TreeTape<H>>,
    logits: Vec<f64>,
    value: f64,
}

/// The parts of one authenticated input the model reads.
#[derive(Clone, Copy)]
struct DecisionView<'a> {
    state: &'a NumericTree,
    actions: &'a [NumericTree],
    flags: &'a [[bool; FLAG_COUNT]],
}
impl<'a> DecisionView<'a> {
    fn of(input: &'a AuthenticatedInputV1) -> Self {
        Self {
            state: input.packet().state(),
            actions: input.packet().actions(),
            flags: input.action_flags(),
        }
    }
    fn validate(&self, minimum_actions: usize) -> Result<()> {
        if self.flags.len() != self.actions.len() {
            return Err(Error::ModelContract);
        }
        if self.actions.len() < minimum_actions {
            return Err(Error::ActionRange);
        }
        Ok(())
    }
}

/// One retained decision with its standardized advantage and value target,
/// both computed from the pre-update parameters.
struct ExampleV2<'a> {
    view: DecisionView<'a>,
    selected: usize,
    advantage: f64,
    value_target: f64,
}
#[derive(Default)]
struct LossV2 {
    total: f64,
    policy: f64,
    value: f64,
    entropy: f64,
}

impl<const H: usize> TreeModelV2<H> {
    /// Using the model at any width other than 16 or 32 fails to compile.
    pub const CONTRACT: &'static str = contract_for(H);
    pub const PARAMETER_COUNT: usize = parameter_count_v2(H);
    const LAYOUT: Layout = Layout { hidden: H };

    pub fn from_parameters(parameters: Vec<f64>) -> Result<Self> {
        const { assert!(H == 16 || H == 32) };
        if parameters.len() != Self::PARAMETER_COUNT || parameters.iter().any(|v| !v.is_finite()) {
            return Err(Error::ModelContract);
        }
        Ok(Self { parameters })
    }
    pub fn parameters(&self) -> &[f64] {
        &self.parameters
    }

    /// Step-0 weights. `SplitMix64` is seeded with the first eight bytes
    /// (little-endian) of SHA-256 over `INIT_TAG_V2` and the contract string.
    /// The tag is the 42 ASCII characters
    /// `mtg-kernel/current-tree-learner-v2-init/v1` followed by one NUL byte,
    /// 43 bytes in all. The generator draws one value per parameter of the
    /// scaled blocks, in parameter index order: `(2 u - 1) s` with `u` the top
    /// 53 bits over `2^53`. The
    /// scale `s` is `1 / sqrt(20)` for the input weights and `1 / sqrt(H)`
    /// for the recurrent weights and the three policy-head vectors. The
    /// encoder bias, the action-flag weights, the policy bias and the whole
    /// value head start at exactly zero, so every initial value prediction is
    /// exactly `0.0`.
    pub fn deterministic_initialization() -> Self {
        let layout = Self::LAYOUT;
        let mut hash = Sha256::new();
        hash.update(INIT_TAG_V2);
        hash.update(Self::CONTRACT.as_bytes());
        let digest = hash.finalize();
        let mut seed = [0; 8];
        seed.copy_from_slice(&digest[..8]);
        let mut rng = SplitMix64::seed(u64::from_le_bytes(seed));
        let input_scale = 1. / sqrt_v2(INPUT as f64);
        let hidden_scale = 1. / sqrt_v2(H as f64);
        let mut parameters = vec![0.; Self::PARAMETER_COUNT];
        for (index, parameter) in parameters.iter_mut().enumerate() {
            let scale = if index < layout.recurrent() {
                input_scale
            } else if index < layout.bias() {
                hidden_scale
            } else if index < layout.head() {
                continue;
            } else if index < layout.flags() {
                hidden_scale
            } else {
                continue;
            };
            let unit = (rng.next_u64() >> 11) as f64 * TWO_POW_MINUS_53;
            *parameter = (2. * unit - 1.) * scale;
        }
        Self { parameters }
    }

    fn tree(&self, tree: &NumericTree) -> Result<TreeTape<H>> {
        let layout = Self::LAYOUT;
        let parameters = &self.parameters;
        let mut tape = TreeTape {
            inputs: Vec::new(),
            parents: Vec::new(),
            hidden: Vec::new(),
            pooled: [0.; H],
        };
        if tree.rows().is_empty() || tree.rows().len() != tree.topology().len() {
            return Err(Error::ModelContract);
        }
        for (index, (row, (parent, ordinal))) in tree.rows().iter().zip(tree.topology()).enumerate()
        {
            let parent = parent
                .iter()
                .enumerate()
                .fold(0u64, |n, (i, limb)| n | (u64::from(*limb) << (16 * i)));
            let parent = if parent == 0 {
                None
            } else {
                let parent = usize::try_from(parent - 1).map_err(|_| Error::Range)?;
                if parent >= index {
                    return Err(Error::ModelContract);
                }
                Some(parent)
            };
            let mut input = [0.; INPUT];
            for (to, from) in input.iter_mut().zip(
                row.iter()
                    .chain(tree.topology()[index].0.iter())
                    .chain(ordinal),
            ) {
                *to = f64::from(*from) / 65535.;
            }
            let previous = index.checked_sub(1).map_or([0.; H], |i| tape.hidden[i]);
            let ancestor = parent.map_or([0.; H], |i| tape.hidden[i]);
            let hidden: [f64; H] = std::array::from_fn(|h| {
                let mut value = parameters[layout.bias() + h];
                for (i, feature) in input.iter().enumerate() {
                    value += parameters[h * INPUT + i] * feature;
                }
                for i in 0..H {
                    value += parameters[layout.recurrent() + h * H + i]
                        * (previous[i] + ancestor[i])
                        / 2.;
                }
                tanh_v2(value)
            });
            for (sum, value) in tape.pooled.iter_mut().zip(hidden) {
                *sum += value;
            }
            tape.inputs.push(input);
            tape.parents.push(parent);
            tape.hidden.push(hidden);
        }
        let count = tape.hidden.len() as f64;
        for sum in &mut tape.pooled {
            *sum /= count;
        }
        Ok(tape)
    }

    fn logit(&self, state: &TreeTape<H>, action: &TreeTape<H>, flags: &[bool; 6]) -> Result<f64> {
        let layout = Self::LAYOUT;
        let parameters = &self.parameters;
        let mut score = parameters[layout.policy_bias()];
        for h in 0..H {
            score += parameters[layout.head() + h] * state.pooled[h]
                + parameters[layout.head() + H + h] * action.pooled[h]
                + parameters[layout.head() + 2 * H + h] * state.pooled[h] * action.pooled[h];
        }
        for (i, flag) in flags.iter().enumerate() {
            score += parameters[layout.flags() + i] * f64::from(u8::from(*flag));
        }
        if score.is_finite() {
            Ok(score)
        } else {
            Err(Error::NonFiniteLogit)
        }
    }

    /// `tanh(b_v + sum_h w_v[h] state.pooled[h])`, summed in that order.
    fn value_of(&self, state: &TreeTape<H>) -> Result<f64> {
        let layout = Self::LAYOUT;
        let parameters = &self.parameters;
        let mut pre_activation = parameters[layout.value_bias()];
        for h in 0..H {
            pre_activation += parameters[layout.value_weights() + h] * state.pooled[h];
        }
        let value = tanh_v2(pre_activation);
        if value.is_finite() {
            Ok(value)
        } else {
            Err(Error::NonFiniteLogit)
        }
    }

    fn value_view(&self, view: DecisionView<'_>) -> Result<f64> {
        view.validate(1)?;
        let state = self.tree(view.state)?;
        self.value_of(&state)
    }

    fn forward_view(&self, view: DecisionView<'_>) -> Result<ForwardTape<H>> {
        view.validate(1)?;
        let state = self.tree(view.state)?;
        let actions = view
            .actions
            .iter()
            .map(|action| self.tree(action))
            .collect::<Result<Vec<_>>>()?;
        let logits = actions
            .iter()
            .zip(view.flags)
            .map(|(action, flags)| self.logit(&state, action, flags))
            .collect::<Result<Vec<_>>>()?;
        let value = self.value_of(&state)?;
        Ok(ForwardTape {
            state,
            actions,
            logits,
            value,
        })
    }

    pub fn forward(&self, input: &AuthenticatedInputV1) -> Result<PolicyValueV2> {
        let tape = self.forward_view(DecisionView::of(input))?;
        Ok(PolicyValueV2 {
            logits: tape.logits,
            value: tape.value,
        })
    }

    fn backward_tree(&self, tape: &TreeTape<H>, pooled_gradient: [f64; H], gradient: &mut [f64]) {
        let layout = Self::LAYOUT;
        let parameters = &self.parameters;
        let count = tape.hidden.len() as f64;
        let mut adjoints = vec![pooled_gradient.map(|v| v / count); tape.hidden.len()];
        for index in (0..tape.hidden.len()).rev() {
            let previous = index.checked_sub(1).map_or([0.; H], |i| tape.hidden[i]);
            let ancestor = tape.parents[index].map_or([0.; H], |i| tape.hidden[i]);
            for h in 0..H {
                let activation = tape.hidden[index][h];
                let dz = adjoints[index][h] * (1. - activation * activation);
                gradient[layout.bias() + h] += dz;
                for i in 0..INPUT {
                    gradient[h * INPUT + i] += dz * tape.inputs[index][i];
                }
                for i in 0..H {
                    gradient[layout.recurrent() + h * H + i] +=
                        dz * (previous[i] + ancestor[i]) / 2.;
                    let propagated = dz * parameters[layout.recurrent() + h * H + i] / 2.;
                    if let Some(previous) = index.checked_sub(1) {
                        adjoints[previous][i] += propagated;
                    }
                    if let Some(parent) = tape.parents[index] {
                        adjoints[parent][i] += propagated;
                    }
                }
            }
        }
    }

    /// The composite loss of design Section 3.2 and its gradient, the mean
    /// over the examples of `-A_n log pi(a_n | s_n)` plus
    /// `c_v (V(s_n) - target_n)^2 / 2` minus `c_ent H(pi(. | s_n))`, where the
    /// entropy is taken over the decision's full legal menu. Examples are
    /// reduced in slice order.
    fn loss_and_gradient(
        &self,
        examples: &[ExampleV2<'_>],
        value_coefficient: f64,
        entropy_coefficient: f64,
    ) -> Result<(LossV2, Vec<f64>)> {
        if examples.is_empty() {
            return Err(Error::NoDecision);
        }
        let layout = Self::LAYOUT;
        let parameters = &self.parameters;
        let weight = 1. / examples.len() as f64;
        let mut gradient = vec![0.; Self::PARAMETER_COUNT];
        let mut loss = LossV2::default();
        for example in examples {
            example.view.validate(2)?;
            let tape = self.forward_view(example.view)?;
            if example.selected >= tape.logits.len() {
                return Err(Error::ActionRange);
            }
            let softmax = softmax_v2(&tape.logits)?;
            let policy = -example.advantage * softmax.log_probabilities[example.selected];
            let error = tape.value - example.value_target;
            let value = 0.5 * error * error;
            let objective =
                policy + value_coefficient * value - entropy_coefficient * softmax.entropy;
            loss.total += weight * objective;
            loss.policy += weight * policy;
            loss.value += weight * value;
            loss.entropy += weight * softmax.entropy;

            let value_pre_activation =
                weight * value_coefficient * error * (1. - tape.value * tape.value);
            let mut state_gradient = [0.; H];
            gradient[layout.value_bias()] += value_pre_activation;
            for h in 0..H {
                gradient[layout.value_weights() + h] += value_pre_activation * tape.state.pooled[h];
                state_gradient[h] += value_pre_activation * parameters[layout.value_weights() + h];
            }
            for (index, action) in tape.actions.iter().enumerate() {
                let chosen = if index == example.selected { 1. } else { 0. };
                let probability = softmax.probabilities[index];
                let dz = weight
                    * (example.advantage * (probability - chosen)
                        + entropy_coefficient
                            * probability
                            * (softmax.log_probabilities[index] + softmax.entropy));
                let mut action_gradient = [0.; H];
                gradient[layout.policy_bias()] += dz;
                for h in 0..H {
                    gradient[layout.head() + h] += dz * tape.state.pooled[h];
                    gradient[layout.head() + H + h] += dz * action.pooled[h];
                    gradient[layout.head() + 2 * H + h] +=
                        dz * tape.state.pooled[h] * action.pooled[h];
                    state_gradient[h] += dz
                        * (parameters[layout.head() + h]
                            + parameters[layout.head() + 2 * H + h] * action.pooled[h]);
                    action_gradient[h] = dz
                        * (parameters[layout.head() + H + h]
                            + parameters[layout.head() + 2 * H + h] * tape.state.pooled[h]);
                }
                for (i, flag) in example.view.flags[index].iter().enumerate() {
                    gradient[layout.flags() + i] += dz * f64::from(u8::from(*flag));
                }
                self.backward_tree(action, action_gradient, &mut gradient);
            }
            self.backward_tree(&tape.state, state_gradient, &mut gradient);
        }
        if !loss.total.is_finite() || gradient.iter().any(|v| !v.is_finite()) {
            return Err(Error::NonFiniteLogit);
        }
        Ok((loss, gradient))
    }
}

struct SoftmaxV2 {
    probabilities: Vec<f64>,
    log_probabilities: Vec<f64>,
    entropy: f64,
}

/// Softmax over finite logits: the maximum by an ascending scan, weights
/// `exp(max(z - max, -700))`, their sum in index order, `log p = (z - max) -
/// ln(sum)` and the entropy `-sum p log p` in index order.
fn softmax_v2(logits: &[f64]) -> Result<SoftmaxV2> {
    let Some(first) = logits.first() else {
        return Err(Error::ActionRange);
    };
    if logits.iter().any(|v| !v.is_finite()) {
        return Err(Error::NonFiniteLogit);
    }
    let mut max = *first;
    for &logit in logits {
        if logit > max {
            max = logit;
        }
    }
    let shifted = logits.iter().map(|z| z - max).collect::<Vec<_>>();
    let weights = shifted
        .iter()
        .map(|&x| exp_nonpositive_v2(if x < EXP_FLOOR { EXP_FLOOR } else { x }))
        .collect::<Vec<_>>();
    let mut sum = 0.;
    for weight in &weights {
        sum += weight;
    }
    let log_sum = ln_v2(sum);
    let probabilities = weights.iter().map(|w| w / sum).collect::<Vec<_>>();
    let log_probabilities = shifted.iter().map(|x| x - log_sum).collect::<Vec<_>>();
    let mut entropy = 0.;
    for (probability, log_probability) in probabilities.iter().zip(&log_probabilities) {
        entropy -= probability * log_probability;
    }
    Ok(SoftmaxV2 {
        probabilities,
        log_probabilities,
        entropy,
    })
}

/// Generalized advantage estimation for one seat's natural game (design
/// Section 3.2): `delta_t = gamma V_(t+1) - V_t` before the last retained
/// decision and `R - V_(T-1)` at it, then `A_t = delta_t + gamma lambda
/// A_(t+1)` from the last decision backwards, which is the design's sum
/// `sum_k (gamma lambda)^k delta_(t+k)` evaluated by its recursion. Rewards
/// are zero except the terminal return.
pub fn gae_v2(values: &[f64], terminal_return: f64, gamma: f64, lambda: f64) -> Vec<f64> {
    let decay = gamma * lambda;
    let mut advantages = vec![0.; values.len()];
    let mut next = 0.;
    for t in (0..values.len()).rev() {
        let delta = match values.get(t + 1) {
            Some(following) => gamma * following - values[t],
            None => terminal_return - values[t],
        };
        next = delta + decay * next;
        advantages[t] = next;
    }
    advantages
}

/// Scale-only standardization (design Section 3.2): `A / (sigma + 1e-8)`,
/// where sigma is the population standard deviation around the mean, and the
/// mean itself is not subtracted. Returns the standardized values and sigma.
pub fn scale_only_standardize_v2(advantages: &[f64]) -> Result<(Vec<f64>, f64)> {
    if advantages.is_empty() {
        return Err(Error::NoDecision);
    }
    if advantages.iter().any(|v| !v.is_finite()) {
        return Err(Error::NonFiniteLogit);
    }
    let count = advantages.len() as f64;
    let mut sum = 0.;
    for advantage in advantages {
        sum += advantage;
    }
    let mean = sum / count;
    let mut squares = 0.;
    for advantage in advantages {
        let deviation = advantage - mean;
        squares += deviation * deviation;
    }
    let sigma = sqrt_v2(squares / count);
    let denominator = sigma + 1e-8;
    Ok((advantages.iter().map(|a| a / denominator).collect(), sigma))
}

/// Global L2 clip: when the norm exceeds `max_norm`, every component is
/// multiplied by `max_norm / norm`. Returns the norm before clipping.
pub fn clip_global_norm_v2(gradient: &mut [f64], max_norm: f64) -> f64 {
    let mut squares = 0.;
    for value in gradient.iter() {
        squares += value * value;
    }
    let norm = sqrt_v2(squares);
    if norm > max_norm {
        let scale = max_norm / norm;
        for value in gradient.iter_mut() {
            *value *= scale;
        }
    }
    norm
}

/// Optimizer, schedule and objective configuration, recorded in every
/// snapshot. [`OptimizerConfigV2::campaign_v1`] holds the design's values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptimizerConfigV2 {
    pub learning_rate_start: f64,
    pub learning_rate_end: f64,
    pub horizon_updates: u64,
    pub beta1: f64,
    pub beta2: f64,
    pub epsilon: f64,
    pub clip_norm: f64,
    pub gamma: f64,
    pub lambda: f64,
    pub value_coefficient: f64,
    pub entropy_coefficient: f64,
}
impl OptimizerConfigV2 {
    /// Design Section 3.3: Adam `0.9 / 0.999 / 1e-8`, clip `1.0`, cosine
    /// `3e-4 -> 1e-4` over `U_max` updates; Section 3.2: `gamma 0.997`,
    /// `lambda 0.95`, `c_v 0.5`, `c_ent 0.01`.
    pub fn campaign_v1(horizon_updates: u64) -> Self {
        Self {
            learning_rate_start: 3e-4,
            learning_rate_end: 1e-4,
            horizon_updates,
            beta1: 0.9,
            beta2: 0.999,
            epsilon: 1e-8,
            clip_norm: 1.,
            gamma: 0.997,
            lambda: 0.95,
            value_coefficient: 0.5,
            entropy_coefficient: 0.01,
        }
    }
    fn validate(&self) -> Result<()> {
        let positive = |v: f64| v.is_finite() && v > 0.;
        let unit = |v: f64| v.is_finite() && v > 0. && v <= 1.;
        if !positive(self.learning_rate_start)
            || !positive(self.learning_rate_end)
            || self.horizon_updates == 0
            || !(0. ..1.).contains(&self.beta1)
            || !(0. ..1.).contains(&self.beta2)
            || !positive(self.epsilon)
            || !positive(self.clip_norm)
            || !unit(self.gamma)
            || !unit(self.lambda)
            || !(self.value_coefficient.is_finite() && self.value_coefficient >= 0.)
            || !(self.entropy_coefficient.is_finite() && self.entropy_coefficient >= 0.)
        {
            return Err(Error::ModelContract);
        }
        Ok(())
    }
    /// The learning rate of update `u` in `1..=U_max` (the update that
    /// produces step `u`): `lr_end + (lr_start - lr_end) (1 + cos(pi f)) / 2`
    /// with `f = (u - 1) / (U_max - 1)`, so update 1 uses exactly `lr_start`
    /// and update `U_max` exactly `lr_end`.
    pub fn learning_rate(&self, update: u64) -> Result<f64> {
        self.validate()?;
        if update == 0 || update > self.horizon_updates {
            return Err(Error::Range);
        }
        if update == 1 {
            return Ok(self.learning_rate_start);
        }
        if update == self.horizon_updates {
            return Ok(self.learning_rate_end);
        }
        let fraction = (update - 1) as f64 / (self.horizon_updates - 1) as f64;
        Ok(self.learning_rate_end
            + (self.learning_rate_start - self.learning_rate_end)
                * 0.5
                * (1. + cos_pi_v2(fraction)))
    }
}

/// Bit-exact learner state. The arrays are the `u64` bit patterns of the
/// parameters and the Adam moments.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotV2 {
    contract: String,
    feature_identity: String,
    config: OptimizerConfigV2,
    step: u64,
    parameter_bits: Vec<u64>,
    first_moment_bits: Vec<u64>,
    second_moment_bits: Vec<u64>,
}
impl SnapshotV2 {
    pub fn contract(&self) -> &str {
        &self.contract
    }
    pub fn feature_identity(&self) -> &str {
        &self.feature_identity
    }
    pub fn config(&self) -> &OptimizerConfigV2 {
        &self.config
    }
    pub fn step(&self) -> u64 {
        self.step
    }
    /// SHA-256 over `STATE_TAG_V2`, the step and every parameter, first-moment
    /// and second-moment bit pattern (little-endian), in that order. It
    /// leaves out the contract, the feature identity (which hashes this file)
    /// and the configuration, so it pins the arithmetic alone.
    pub fn state_sha256(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(STATE_TAG_V2);
        hash.update(self.step.to_le_bytes());
        for bits in self
            .parameter_bits
            .iter()
            .chain(&self.first_moment_bits)
            .chain(&self.second_moment_bits)
        {
            hash.update(bits.to_le_bytes());
        }
        format!("{:x}", hash.finalize())
    }
}

/// How a learner seat's game ended. Truncated games keep their place in the
/// batch for the runner's records, but add nothing to the policy or the value
/// loss and are not counted in `N` (the research lead's ruling on design
/// Section 3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalV2 {
    Natural { learner_return: i8 },
    Truncated,
}
/// One retained decision (`legal > 1`) and the ordinal the actor sampled.
pub struct DecisionV2<'a> {
    pub input: &'a AuthenticatedInputV1,
    pub selected: usize,
}
/// One learner seat's retained decisions in one game, in decision order.
pub struct TrajectoryV2<'a> {
    pub pair_index: u64,
    pub leg: u8,
    pub seat: u8,
    pub terminal: TerminalV2,
    pub decisions: Vec<DecisionV2<'a>>,
}

struct SelectedView<'a> {
    view: DecisionView<'a>,
    selected: usize,
}
struct TrajectoryView<'a> {
    pair_index: u64,
    leg: u8,
    seat: u8,
    terminal: TerminalV2,
    decisions: Vec<SelectedView<'a>>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UpdateReportV2 {
    pub step: u64,
    pub learning_rate: f64,
    pub loss: f64,
    pub policy_loss: f64,
    pub value_loss: f64,
    pub entropy: f64,
    pub advantage_sigma: f64,
    pub gradient_norm: f64,
    pub clipped: bool,
    pub decisions: usize,
    pub natural_trajectories: usize,
    pub truncated_trajectories: usize,
}

pub struct TreeLearnerV2<const H: usize> {
    model: TreeModelV2<H>,
    config: OptimizerConfigV2,
    step: u64,
    first: Vec<f64>,
    second: Vec<f64>,
}
pub type LearnerV2 = TreeLearnerV2<32>;
pub type LearnerV2H16 = TreeLearnerV2<16>;

impl<const H: usize> TreeLearnerV2<H> {
    /// Step 0 from [`TreeModelV2::deterministic_initialization`] with zero
    /// Adam moments.
    pub fn new(config: OptimizerConfigV2) -> Result<Self> {
        Self::with_model(TreeModelV2::deterministic_initialization(), config)
    }
    pub fn with_model(model: TreeModelV2<H>, config: OptimizerConfigV2) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            model,
            config,
            step: 0,
            first: vec![0.; TreeModelV2::<H>::PARAMETER_COUNT],
            second: vec![0.; TreeModelV2::<H>::PARAMETER_COUNT],
        })
    }
    pub fn model(&self) -> &TreeModelV2<H> {
        &self.model
    }
    pub fn config(&self) -> &OptimizerConfigV2 {
        &self.config
    }
    pub fn step(&self) -> u64 {
        self.step
    }
    pub fn snapshot(&self) -> SnapshotV2 {
        SnapshotV2 {
            contract: TreeModelV2::<H>::CONTRACT.into(),
            feature_identity: feature_identity_for(TreeModelV2::<H>::CONTRACT),
            config: self.config.clone(),
            step: self.step,
            parameter_bits: self.model.parameters.iter().map(|v| v.to_bits()).collect(),
            first_moment_bits: self.first.iter().map(|v| v.to_bits()).collect(),
            second_moment_bits: self.second.iter().map(|v| v.to_bits()).collect(),
        }
    }
    /// Refuses another profile's contract, another feature identity, an
    /// invalid configuration, a step past the horizon, wrong array lengths,
    /// non-finite values, negative second moments and nonzero moments at
    /// step 0.
    pub fn restore(snapshot: SnapshotV2) -> Result<Self> {
        if snapshot.contract != TreeModelV2::<H>::CONTRACT
            || snapshot.feature_identity != feature_identity_for(TreeModelV2::<H>::CONTRACT)
        {
            return Err(Error::ModelContract);
        }
        snapshot.config.validate()?;
        if snapshot.step > snapshot.config.horizon_updates {
            return Err(Error::ModelContract);
        }
        let decode = |bits: Vec<u64>| -> Result<Vec<f64>> {
            let values = bits.into_iter().map(f64::from_bits).collect::<Vec<_>>();
            if values.len() != TreeModelV2::<H>::PARAMETER_COUNT
                || values.iter().any(|v| !v.is_finite())
            {
                return Err(Error::ModelContract);
            }
            Ok(values)
        };
        let first = decode(snapshot.first_moment_bits)?;
        let second = decode(snapshot.second_moment_bits)?;
        if second.iter().any(|v| *v < 0.)
            || (snapshot.step == 0 && first.iter().chain(&second).any(|v| *v != 0.))
        {
            return Err(Error::ModelContract);
        }
        Ok(Self {
            model: TreeModelV2::from_parameters(decode(snapshot.parameter_bits)?)?,
            config: snapshot.config,
            step: snapshot.step,
            first,
            second,
        })
    }

    /// One synchronous update over a whole batch (design Sections 3.2 and
    /// 3.3). Trajectories are sorted by `(pair_index, leg, seat)` first, so
    /// the result does not depend on collection order; a repeated key is
    /// refused. Every value, advantage and target comes from the pre-update
    /// parameters. Transactional: on any error the parameters, moments and
    /// step keep their bits.
    pub fn update(&mut self, trajectories: &[TrajectoryV2<'_>]) -> Result<UpdateReportV2> {
        let views = trajectories
            .iter()
            .map(|trajectory| TrajectoryView {
                pair_index: trajectory.pair_index,
                leg: trajectory.leg,
                seat: trajectory.seat,
                terminal: trajectory.terminal,
                decisions: trajectory
                    .decisions
                    .iter()
                    .map(|decision| SelectedView {
                        view: DecisionView::of(decision.input),
                        selected: decision.selected,
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();
        self.update_views(&views)
    }

    fn update_views(&mut self, trajectories: &[TrajectoryView<'_>]) -> Result<UpdateReportV2> {
        let step = self.step.checked_add(1).ok_or(Error::Range)?;
        let learning_rate = self.config.learning_rate(step)?;
        let mut ordered = trajectories.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|t| (t.pair_index, t.leg, t.seat));
        if ordered.windows(2).any(|pair| {
            (pair[0].pair_index, pair[0].leg, pair[0].seat)
                == (pair[1].pair_index, pair[1].leg, pair[1].seat)
        }) {
            return Err(Error::ModelContract);
        }
        let mut examples = Vec::new();
        let mut advantages = Vec::new();
        let mut natural_trajectories = 0;
        let mut truncated_trajectories = 0;
        for trajectory in ordered {
            if trajectory.leg > 1 || trajectory.seat > 1 {
                return Err(Error::ModelContract);
            }
            for decision in &trajectory.decisions {
                decision.view.validate(2)?;
                if decision.selected >= decision.view.actions.len() {
                    return Err(Error::ActionRange);
                }
            }
            let learner_return = match trajectory.terminal {
                TerminalV2::Truncated => {
                    truncated_trajectories += 1;
                    continue;
                }
                TerminalV2::Natural { learner_return } => learner_return,
            };
            if !(-1..=1).contains(&learner_return) {
                return Err(Error::ModelContract);
            }
            natural_trajectories += 1;
            let values = trajectory
                .decisions
                .iter()
                .map(|decision| self.model.value_view(decision.view))
                .collect::<Result<Vec<_>>>()?;
            let estimates = gae_v2(
                &values,
                f64::from(learner_return),
                self.config.gamma,
                self.config.lambda,
            );
            let decisions = &trajectory.decisions;
            for ((decision, value), advantage) in decisions.iter().zip(values).zip(estimates) {
                advantages.push(advantage);
                examples.push(ExampleV2 {
                    view: decision.view,
                    selected: decision.selected,
                    advantage,
                    value_target: advantage + value,
                });
            }
        }
        let (standardized, advantage_sigma) = scale_only_standardize_v2(&advantages)?;
        for (example, advantage) in examples.iter_mut().zip(standardized) {
            example.advantage = advantage;
        }
        let (loss, mut gradient) = self.model.loss_and_gradient(
            &examples,
            self.config.value_coefficient,
            self.config.entropy_coefficient,
        )?;
        let gradient_norm = clip_global_norm_v2(&mut gradient, self.config.clip_norm);
        if !gradient_norm.is_finite() {
            return Err(Error::NonFiniteLogit);
        }

        let beta1 = self.config.beta1;
        let beta2 = self.config.beta2;
        let correction1 = 1. - powi_v2(beta1, step);
        let correction2 = 1. - powi_v2(beta2, step);
        let mut parameters = self.model.parameters.clone();
        let mut first = self.first.clone();
        let mut second = self.second.clone();
        for (((parameter, first), second), gradient) in parameters
            .iter_mut()
            .zip(first.iter_mut())
            .zip(second.iter_mut())
            .zip(&gradient)
        {
            *first = beta1 * *first + (1. - beta1) * gradient;
            *second = beta2 * *second + (1. - beta2) * gradient * gradient;
            *parameter -= learning_rate * (*first / correction1)
                / (sqrt_v2(*second / correction2) + self.config.epsilon);
        }
        if parameters
            .iter()
            .chain(&first)
            .chain(&second)
            .any(|v| !v.is_finite())
        {
            return Err(Error::NonFiniteLogit);
        }
        self.model.parameters = parameters;
        self.first = first;
        self.second = second;
        self.step = step;
        Ok(UpdateReportV2 {
            step,
            learning_rate,
            loss: loss.total,
            policy_loss: loss.policy,
            value_loss: loss.value,
            entropy: loss.entropy,
            advantage_sigma,
            gradient_norm,
            clipped: gradient_norm > self.config.clip_norm,
            decisions: examples.len(),
            natural_trajectories,
            truncated_trajectories,
        })
    }
}

/// The diagnostic control `pair-centered-reinforce/v1` (design Section 3.2:
/// `c_v = 0`, `gamma = lambda = 1`, advantage `R - mean R` of the same deck
/// over the pair's two legs) on the unchanged `CONTRACT_V1` learner. For a
/// learner seat's natural return `own` and the same deck's natural return in
/// the pair's other leg, the advantage is `(own - other) / 2`, one of `0`,
/// `+-1/2` or `+-1`; with no natural other leg the mean is `own` alone and the
/// advantage is `0`. `LearnerV1::update_terminal` takes integer rewards in
/// `-1..=1` and averages over its examples, so each retained decision of the
/// game enters it twice, once with each of the two rewards returned here: the
/// mean over the doubled batch is then the mean of `A * (-log p)` over the
/// original decisions, with no change to the v1 file or its identity.
pub fn pair_centered_terminal_rewards_v1(own: i8, other_leg: Option<i8>) -> Result<[i8; 2]> {
    if !(-1..=1).contains(&own) || other_leg.is_some_and(|other| !(-1..=1).contains(&other)) {
        return Err(Error::ModelContract);
    }
    let Some(other) = other_leg else {
        return Ok([0, 0]);
    };
    let twice = own - other;
    let sign = twice.signum();
    Ok([sign, if twice.abs() == 2 { sign } else { 0 }])
}

// Libm-free scalar functions. Each uses only IEEE 754 `+`, `-`, `*`, `/`,
// comparisons, `abs`, `copysign`, integer conversions and bit manipulation,
// in the order written, so its result has the same bits on every platform.

// The Cody and Waite split of ln 2 used by fdlibm (the high part has 21
// trailing zero bits, so `k * LN2_HI` is exact for every `k` used here) and
// 1 / ln 2, as exact bit patterns.
const LN2_HI: f64 = f64::from_bits(0x3fe6_2e42_fee0_0000);
const LN2_LO: f64 = f64::from_bits(0x3dea_39ef_3579_3c76);
const INV_LN2: f64 = f64::from_bits(0x3ff7_1547_652b_82fe);
const TWO_POW_MINUS_53: f64 = pow2(-53);
const TWO_POW_500: f64 = pow2(500);
const TWO_POW_MINUS_500: f64 = pow2(-500);
const TWO_POW_600: f64 = pow2(600);
const TWO_POW_MINUS_600: f64 = pow2(-600);
const TWO_POW_300: f64 = pow2(300);
const TWO_POW_MINUS_300: f64 = pow2(-300);
/// `tanh(18)` is `1 - 4.6e-16`, within five units in the last place of 1.
const TANH_SATURATION: f64 = 18.;
/// Softmax weights below `exp(-700)` (about `1e-304`) are taken at that floor.
const EXP_FLOOR: f64 = -700.;
/// `1 / n!` for `n` in `0..=13`, each one correctly rounded division.
const INVERSE_FACTORIALS: [f64; 14] = [
    1.,
    1.,
    1. / 2.,
    1. / 6.,
    1. / 24.,
    1. / 120.,
    1. / 720.,
    1. / 5_040.,
    1. / 40_320.,
    1. / 362_880.,
    1. / 3_628_800.,
    1. / 39_916_800.,
    1. / 479_001_600.,
    1. / 6_227_020_800.,
];
/// `2 / (2 k + 1)` for `k` in `0..=11`: the series `ln((1 + s) / (1 - s)) =
/// s sum_k 2 s^(2 k) / (2 k + 1)`.
const ATANH_COEFFICIENTS: [f64; 12] = [
    2.,
    2. / 3.,
    2. / 5.,
    2. / 7.,
    2. / 9.,
    2. / 11.,
    2. / 13.,
    2. / 15.,
    2. / 17.,
    2. / 19.,
    2. / 21.,
    2. / 23.,
];

/// `2^k` from its exponent bits, for `k` in `-1022..=1023`.
const fn pow2(k: i32) -> f64 {
    f64::from_bits(((k + 1023) as u64) << 52)
}

/// `x = k ln 2 + r` with `k` the nearest integer to `x / ln 2` (half away from
/// zero, by truncating `y + 0.5` or `y - 0.5`) and `|r|` at most about
/// `ln(2) / 2`.
fn reduce_ln2(x: f64) -> (i32, f64) {
    let y = x * INV_LN2;
    let k = if y >= 0. {
        (y + 0.5) as i32
    } else {
        (y - 0.5) as i32
    };
    let k_f = f64::from(k);
    (k, (x - k_f * LN2_HI) - k_f * LN2_LO)
}

/// `e^r - 1` for a reduced `r`: the Taylor polynomial of degree 13 by
/// Horner's method, `r (1 + r (1/2! + r (1/3! + ... + r / 13!)))`, which never
/// forms `1 + tiny - 1`.
fn expm1_reduced(r: f64) -> f64 {
    let mut value = INVERSE_FACTORIALS[13];
    for coefficient in INVERSE_FACTORIALS[1..13].iter().rev() {
        value = value * r + coefficient;
    }
    value * r
}

/// `e^t - 1` for `t` in `[0, 36]`: `(2^k - 1) + 2^k (e^r - 1)`, where `2^k - 1`
/// is exact because `k <= 52`.
fn expm1_nonnegative_v2(t: f64) -> f64 {
    let (k, r) = reduce_ln2(t);
    let scale = pow2(k);
    (scale - 1.) + scale * expm1_reduced(r)
}

/// `e^x` for `x` in `[-700, 0]`: `2^k (1 + (e^r - 1))` with `k >= -1010`.
fn exp_nonpositive_v2(x: f64) -> f64 {
    let (k, r) = reduce_ln2(x);
    pow2(k) * (1. + expm1_reduced(r))
}

/// `tanh(x) = sign(x) m / (m + 2)` with `m = e^(2 |x|) - 1`; `+-1` from
/// `|x| >= 18` (and for infinities). A NaN stays NaN.
fn tanh_v2(x: f64) -> f64 {
    let magnitude = x.abs();
    if magnitude >= TANH_SATURATION {
        return 1_f64.copysign(x);
    }
    let em1 = expm1_nonnegative_v2(2. * magnitude);
    (em1 / (em1 + 2.)).copysign(x)
}

/// Natural logarithm of a positive normal finite `x`: `x = m 2^e` with `m` in
/// `(sqrt(1/2), sqrt(2)]`, `s = (m - 1) / (m + 1)`, `ln m = s sum_k 2 s^(2 k)
/// / (2 k + 1)` through `k = 11` by Horner's method in `s^2`, and
/// `ln x = e LN2_HI + (e LN2_LO + ln m)`.
fn ln_v2(x: f64) -> f64 {
    let bits = x.to_bits();
    let mut exponent = ((bits >> 52) & 0x7ff) as i32 - 1023;
    let mut mantissa = f64::from_bits((bits & 0x000f_ffff_ffff_ffff) | 0x3ff0_0000_0000_0000);
    if mantissa > std::f64::consts::SQRT_2 {
        mantissa *= 0.5;
        exponent += 1;
    }
    let s = (mantissa - 1.) / (mantissa + 1.);
    let s2 = s * s;
    let mut series = ATANH_COEFFICIENTS[11];
    for coefficient in ATANH_COEFFICIENTS[..11].iter().rev() {
        series = series * s2 + coefficient;
    }
    let e = f64::from(exponent);
    e * LN2_HI + (e * LN2_LO + s * series)
}

/// Square root of a finite `x >= 0` (NaN otherwise): exact power-of-two
/// scaling brings `x` to `m 2^(2 j)` with `m` in `[1, 4)`, then six Newton
/// steps `y = (y + m / y) / 2` from `y = (m + 1) / 2`, then exact scaling back.
fn sqrt_v2(x: f64) -> f64 {
    if !x.is_finite() || x < 0. {
        return f64::NAN;
    }
    if x == 0. {
        return 0.;
    }
    let mut value = x;
    let mut factor = 1.;
    while value < TWO_POW_MINUS_500 {
        value *= TWO_POW_600;
        factor *= TWO_POW_MINUS_300;
    }
    while value > TWO_POW_500 {
        value *= TWO_POW_MINUS_600;
        factor *= TWO_POW_300;
    }
    let exponent = ((value.to_bits() >> 52) & 0x7ff) as i32 - 1023;
    let even = exponent - exponent.rem_euclid(2);
    let mantissa = value * pow2(-even);
    let mut root = 0.5 * (mantissa + 1.);
    for _ in 0..6 {
        root = 0.5 * (root + mantissa / root);
    }
    root * pow2(even / 2) * factor
}

/// `cos(pi f)` for `f` in `[0, 1]`, as `sin(a)` with `a = pi (1/2 - f)` in
/// `[-pi/2, pi/2]`: the Taylor terms `a^(2 n + 1) / (2 n + 1)!` through
/// `n = 13`, each from the previous one, summed in increasing order.
fn cos_pi_v2(fraction: f64) -> f64 {
    let angle = std::f64::consts::PI * (0.5 - fraction);
    let square = angle * angle;
    let mut term = angle;
    let mut sum = angle;
    for n in 1..=13u32 {
        term = -term * square / f64::from(2 * n * (2 * n + 1));
        sum += term;
    }
    sum
}

/// `base^exponent` by binary exponentiation from the lowest bit.
fn powi_v2(base: f64, mut exponent: u64) -> f64 {
    let mut result = 1.;
    let mut power = base;
    while exponent > 0 {
        if exponent & 1 == 1 {
            result *= power;
        }
        power *= power;
        exponent >>= 1;
    }
    result
}

#[cfg(test)]
mod tests;
