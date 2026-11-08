//! Prospective digest-free actor learner. This contract is independent of every
//! FlatV4/Net8 checkpoint and of the existing current-mixed inference model.
//! Inputs originate only from authenticated public actor packets. This module
//! does not launch collection, select an architecture or qualify production.
use crate::current_mixed_policy_v1::{Error, NumericTree, Packet, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const ACTION_FLAGS_V1: [&str; 6] = [
    "pay",
    "change_target",
    "use_cost",
    "cast_it",
    "value",
    "include",
];
pub const CONTRACT_V1: &str = "current-tree-learner/v1/f64/input20/hidden16/flags6/adam";
const INPUT: usize = 20;
const HIDDEN: usize = 16;
const RECURRENT: usize = INPUT * HIDDEN;
const BIAS: usize = RECURRENT + HIDDEN * HIDDEN;
const HEAD: usize = BIAS + HIDDEN;
const FLAGS: usize = HEAD + 3 * HIDDEN;
pub const PARAMETER_COUNT_V1: usize = FLAGS + 7;

pub fn feature_identity_v1() -> String {
    static IDENTITY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    IDENTITY
        .get_or_init(|| {
            let mut hash = Sha256::new();
            hash.update(CONTRACT_V1);
            hash.update(crate::current_mixed_policy_v1::feature_identity());
            hash.update(include_bytes!("current_tree_learner_v1.rs"));
            format!("{:x}", hash.finalize())
        })
        .clone()
}

/// No public constructor, deserializer or executable authority. A retained
/// capture is immutable training data, not a way to resurrect a stale action.
pub struct AuthenticatedInputV1 {
    packet: Packet,
    flags: Vec<[bool; 6]>,
    feature_identity: String,
}
impl AuthenticatedInputV1 {
    pub(crate) fn from_current(
        packet: Packet,
        actions: &[crate::rl::ActionSemanticV1],
    ) -> Result<Self> {
        if actions.is_empty() || actions.len() != packet.actions().len() {
            return Err(Error::ActionRange);
        }
        let flags = actions
            .iter()
            .map(|action| {
                let value =
                    serde_json::to_value(action).map_err(|e| Error::Projection(e.to_string()))?;
                let mut flags = [false; 6];
                for (i, key) in ACTION_FLAGS_V1.iter().enumerate() {
                    if let Some(value) = value.get(key) {
                        flags[i] = value.as_bool().ok_or(Error::ModelContract)?;
                    }
                }
                Ok(flags)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            packet,
            flags,
            feature_identity: feature_identity_v1(),
        })
    }
    pub fn packet(&self) -> &Packet {
        &self.packet
    }
    pub fn action_flags(&self) -> &[[bool; 6]] {
        &self.flags
    }
}

#[derive(Clone)]
pub struct ModelV1 {
    parameters: Vec<f64>,
}
struct TreeTape {
    inputs: Vec<[f64; INPUT]>,
    parents: Vec<Option<usize>>,
    hidden: Vec<[f64; HIDDEN]>,
    pooled: [f64; HIDDEN],
}
impl ModelV1 {
    pub fn from_parameters(parameters: Vec<f64>) -> Result<Self> {
        if parameters.len() != PARAMETER_COUNT_V1 || parameters.iter().any(|v| !v.is_finite()) {
            return Err(Error::ModelContract);
        }
        Ok(Self { parameters })
    }
    pub fn parameters(&self) -> &[f64] {
        &self.parameters
    }
    /// Fixed engineering weights, not a trained checkpoint.
    pub fn deterministic_fixture() -> Self {
        Self {
            parameters: (0..PARAMETER_COUNT_V1)
                .map(|i| ((i % 23) as f64 - 11.) / 64.)
                .collect(),
        }
    }
    fn tree(&self, tree: &NumericTree) -> Result<TreeTape> {
        let mut tape = TreeTape {
            inputs: Vec::new(),
            parents: Vec::new(),
            hidden: Vec::new(),
            pooled: [0.; HIDDEN],
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
            let previous = index
                .checked_sub(1)
                .map_or([0.; HIDDEN], |i| tape.hidden[i]);
            let ancestor = parent.map_or([0.; HIDDEN], |i| tape.hidden[i]);
            let hidden = std::array::from_fn(|h| {
                let mut value = self.parameters[BIAS + h];
                for (i, feature) in input.iter().enumerate() {
                    value += self.parameters[h * INPUT + i] * feature;
                }
                for i in 0..HIDDEN {
                    value += self.parameters[RECURRENT + h * HIDDEN + i]
                        * (previous[i] + ancestor[i])
                        / 2.;
                }
                value.tanh()
            });
            for (sum, value) in tape.pooled.iter_mut().zip(hidden) {
                *sum += value;
            }
            tape.inputs.push(input);
            tape.parents.push(parent);
            tape.hidden.push(hidden);
        }
        for sum in &mut tape.pooled {
            *sum /= tape.hidden.len() as f64;
        }
        Ok(tape)
    }
    fn validate_input(&self, input: &AuthenticatedInputV1) -> Result<()> {
        if input.feature_identity != feature_identity_v1()
            || input.flags.is_empty()
            || input.flags.len() != input.packet.actions().len()
        {
            return Err(Error::ModelContract);
        }
        Ok(())
    }
    fn tapes(&self, input: &AuthenticatedInputV1) -> Result<(TreeTape, Vec<TreeTape>, Vec<f64>)> {
        self.validate_input(input)?;
        let state = self.tree(input.packet.state())?;
        let actions = input
            .packet
            .actions()
            .iter()
            .map(|a| self.tree(a))
            .collect::<Result<Vec<_>>>()?;
        let logits = actions
            .iter()
            .zip(&input.flags)
            .map(|(action, flags)| {
                let mut score = self.parameters[PARAMETER_COUNT_V1 - 1];
                for h in 0..HIDDEN {
                    score += self.parameters[HEAD + h] * state.pooled[h]
                        + self.parameters[HEAD + HIDDEN + h] * action.pooled[h]
                        + self.parameters[HEAD + 2 * HIDDEN + h]
                            * state.pooled[h]
                            * action.pooled[h];
                }
                for (i, flag) in flags.iter().enumerate() {
                    score += self.parameters[FLAGS + i] * f64::from(u8::from(*flag));
                }
                if score.is_finite() {
                    Ok(score)
                } else {
                    Err(Error::NonFiniteLogit)
                }
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((state, actions, logits))
    }
    pub fn forward(&self, input: &AuthenticatedInputV1) -> Result<Vec<f64>> {
        self.tapes(input).map(|(_, _, logits)| logits)
    }

    fn backward_tree(&self, tape: &TreeTape, pooled_gradient: [f64; HIDDEN], gradient: &mut [f64]) {
        let mut adjoints =
            vec![pooled_gradient.map(|v| v / tape.hidden.len() as f64); tape.hidden.len()];
        for index in (0..tape.hidden.len()).rev() {
            let previous = index
                .checked_sub(1)
                .map_or([0.; HIDDEN], |i| tape.hidden[i]);
            let ancestor = tape.parents[index].map_or([0.; HIDDEN], |i| tape.hidden[i]);
            for h in 0..HIDDEN {
                let dz = adjoints[index][h] * (1. - tape.hidden[index][h].powi(2));
                gradient[BIAS + h] += dz;
                for i in 0..INPUT {
                    gradient[h * INPUT + i] += dz * tape.inputs[index][i];
                }
                for i in 0..HIDDEN {
                    gradient[RECURRENT + h * HIDDEN + i] += dz * (previous[i] + ancestor[i]) / 2.;
                    let propagated = dz * self.parameters[RECURRENT + h * HIDDEN + i] / 2.;
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

    /// Mean cross entropy over explicit normalized target distributions. Every
    /// example uses the same pre-update parameters; input order fixes reduction.
    pub fn loss_and_gradient(&self, examples: &[TrainingExampleV1<'_>]) -> Result<(f64, Vec<f64>)> {
        if examples.is_empty() {
            return Err(Error::NoDecision);
        }
        let mut gradient = vec![0.; PARAMETER_COUNT_V1];
        let mut loss = 0.;
        for example in examples {
            let (state, actions, logits) = self.tapes(example.input)?;
            if example.targets.len() != logits.len()
                || example.targets.iter().any(|v| !v.is_finite() || *v < 0.)
                || (example.targets.iter().sum::<f64>() - 1.).abs() > 1e-12
            {
                return Err(Error::ModelContract);
            }
            let max = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let sum = logits.iter().map(|v| (v - max).exp()).sum::<f64>();
            let log_sum = sum.ln();
            let mut state_gradient = [0.; HIDDEN];
            for (index, action) in actions.iter().enumerate() {
                loss -= example.targets[index] * (logits[index] - max - log_sum)
                    / examples.len() as f64;
                let dz = ((logits[index] - max).exp() / sum - example.targets[index])
                    / examples.len() as f64;
                let mut action_gradient = [0.; HIDDEN];
                gradient[PARAMETER_COUNT_V1 - 1] += dz;
                for h in 0..HIDDEN {
                    gradient[HEAD + h] += dz * state.pooled[h];
                    gradient[HEAD + HIDDEN + h] += dz * action.pooled[h];
                    gradient[HEAD + 2 * HIDDEN + h] += dz * state.pooled[h] * action.pooled[h];
                    state_gradient[h] += dz
                        * (self.parameters[HEAD + h]
                            + self.parameters[HEAD + 2 * HIDDEN + h] * action.pooled[h]);
                    action_gradient[h] = dz
                        * (self.parameters[HEAD + HIDDEN + h]
                            + self.parameters[HEAD + 2 * HIDDEN + h] * state.pooled[h]);
                }
                for i in 0..6 {
                    gradient[FLAGS + i] += dz * f64::from(u8::from(example.input.flags[index][i]));
                }
                self.backward_tree(action, action_gradient, &mut gradient);
            }
            self.backward_tree(&state, state_gradient, &mut gradient);
        }
        if !loss.is_finite() || gradient.iter().any(|v| !v.is_finite()) {
            return Err(Error::NonFiniteLogit);
        }
        Ok((loss, gradient))
    }
}

pub struct TrainingExampleV1<'a> {
    pub input: &'a AuthenticatedInputV1,
    pub targets: &'a [f64],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdamConfigV1 {
    pub learning_rate: f64,
    pub beta1: f64,
    pub beta2: f64,
    pub epsilon: f64,
}
impl AdamConfigV1 {
    fn validate(&self) -> Result<()> {
        if !self.learning_rate.is_finite()
            || self.learning_rate <= 0.
            || !(0. ..1.).contains(&self.beta1)
            || !(0. ..1.).contains(&self.beta2)
            || !self.epsilon.is_finite()
            || self.epsilon <= 0.
        {
            return Err(Error::ModelContract);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotV1 {
    contract: String,
    feature_identity: String,
    config: AdamConfigV1,
    step: u64,
    parameter_bits: Vec<u64>,
    first_moment_bits: Vec<u64>,
    second_moment_bits: Vec<u64>,
}
pub struct LearnerV1 {
    model: ModelV1,
    config: AdamConfigV1,
    step: u64,
    first: Vec<f64>,
    second: Vec<f64>,
}
impl LearnerV1 {
    pub fn new(model: ModelV1, config: AdamConfigV1) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            model,
            config,
            step: 0,
            first: vec![0.; PARAMETER_COUNT_V1],
            second: vec![0.; PARAMETER_COUNT_V1],
        })
    }
    pub fn model(&self) -> &ModelV1 {
        &self.model
    }
    pub fn snapshot(&self) -> SnapshotV1 {
        SnapshotV1 {
            contract: CONTRACT_V1.into(),
            feature_identity: feature_identity_v1(),
            config: self.config.clone(),
            step: self.step,
            parameter_bits: self.model.parameters.iter().map(|v| v.to_bits()).collect(),
            first_moment_bits: self.first.iter().map(|v| v.to_bits()).collect(),
            second_moment_bits: self.second.iter().map(|v| v.to_bits()).collect(),
        }
    }
    pub fn restore(snapshot: SnapshotV1) -> Result<Self> {
        if snapshot.contract != CONTRACT_V1 || snapshot.feature_identity != feature_identity_v1() {
            return Err(Error::ModelContract);
        }
        snapshot.config.validate()?;
        let decode = |bits: Vec<u64>| -> Result<Vec<f64>> {
            let values = bits.into_iter().map(f64::from_bits).collect::<Vec<_>>();
            if values.len() != PARAMETER_COUNT_V1 || values.iter().any(|v| !v.is_finite()) {
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
            model: ModelV1::from_parameters(decode(snapshot.parameter_bits)?)?,
            config: snapshot.config,
            step: snapshot.step,
            first,
            second,
        })
    }
    /// Transactional synchronous Adam: failures preserve all parameter and
    /// optimizer bits. The caller owns durable publication of the snapshot.
    pub fn update(&mut self, examples: &[TrainingExampleV1<'_>]) -> Result<f64> {
        let (loss, gradient) = self.model.loss_and_gradient(examples)?;
        let step = self.step.checked_add(1).ok_or(Error::Range)?;
        let mut parameters = self.model.parameters.clone();
        let mut first = self.first.clone();
        let mut second = self.second.clone();
        let correction1 = 1. - self.config.beta1.powf(step as f64);
        let correction2 = 1. - self.config.beta2.powf(step as f64);
        for i in 0..PARAMETER_COUNT_V1 {
            first[i] = self.config.beta1 * first[i] + (1. - self.config.beta1) * gradient[i];
            second[i] =
                self.config.beta2 * second[i] + (1. - self.config.beta2) * gradient[i].powi(2);
            parameters[i] -= self.config.learning_rate * (first[i] / correction1)
                / ((second[i] / correction2).sqrt() + self.config.epsilon);
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
        Ok(loss)
    }
}
