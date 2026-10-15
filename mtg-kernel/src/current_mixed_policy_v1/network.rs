//! Separate schema-tree forward. No predecessor model/checkpoint loader is used.
use super::{numeric::NumericTree, Error, Packet, Result};
pub const INPUT: usize = super::numeric::NODE_WIDTH + 8;
pub const HIDDEN: usize = 16;
pub const MODEL_CONTRACT: &str = "current-mixed-schema-tree-native/v1/input20/hidden16";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelConfig {
    pub contract: String,
    pub feature_identity: String,
    pub input_width: usize,
    pub hidden_width: usize,
}
impl ModelConfig {
    pub fn current() -> Self {
        Self {
            contract: MODEL_CONTRACT.into(),
            feature_identity: super::feature_identity(),
            input_width: INPUT,
            hidden_width: HIDDEN,
        }
    }
}

pub struct NativeModel {
    config: ModelConfig,
    weights: Vec<f32>,
}
const WEIGHTS: usize = HIDDEN * INPUT + HIDDEN * HIDDEN + HIDDEN + HIDDEN * 3 + 1;
impl NativeModel {
    /// Deterministic finite fixture parameters, not a trained policy or strength claim.
    pub fn deterministic_fixture() -> Result<Self> {
        let weights = (0..WEIGHTS)
            .map(|i| ((i % 23) as f32 - 11.) / 64.)
            .collect();
        Self::from_parameters(ModelConfig::current(), weights)
    }
    pub fn parameter_count() -> usize {
        WEIGHTS
    }
    pub fn from_parameters(config: ModelConfig, weights: Vec<f32>) -> Result<Self> {
        if config != ModelConfig::current()
            || weights.len() != WEIGHTS
            || weights.iter().any(|v| !v.is_finite())
        {
            return Err(Error::ModelContract);
        }
        Ok(Self { config, weights })
    }
    pub fn config(&self) -> &ModelConfig {
        &self.config
    }
    fn tree(&self, tree: &NumericTree) -> Result<[f64; HIDDEN]> {
        let mut previous = [0.; HIDDEN];
        let mut nodes: Vec<[f64; HIDDEN]> = Vec::with_capacity(tree.rows().len());
        let mut pooled = [0.; HIDDEN];
        for (index, row) in tree.tensors().iter().enumerate() {
            let parent = tree.topology()[index]
                .0
                .iter()
                .enumerate()
                .fold(0u64, |v, (i, limb)| v | (u64::from(*limb) << (16 * i)));
            let parent = if parent == 0 {
                [0.; HIDDEN]
            } else {
                *nodes
                    .get(usize::try_from(parent - 1).map_err(|_| Error::Range)?)
                    .ok_or(Error::ModelContract)?
            };
            let mut next = [0.; HIDDEN];
            for h in 0..HIDDEN {
                let mut value = f64::from(self.weights[HIDDEN * INPUT + HIDDEN * HIDDEN + h]);
                for (i, feature) in row.iter().enumerate() {
                    value += f64::from(self.weights[h * INPUT + i]) * f64::from(*feature) / 65535.;
                }
                for i in 0..HIDDEN {
                    value += f64::from(self.weights[HIDDEN * INPUT + h * HIDDEN + i])
                        * (previous[i] + parent[i])
                        / 2.;
                }
                next[h] = value.tanh();
                pooled[h] += next[h];
            }
            nodes.push(next);
            previous = next;
        }
        let count = tree.rows().len().max(1) as f64;
        for value in &mut pooled {
            *value /= count;
        }
        Ok(pooled)
    }
    pub fn forward(&self, packet: &Packet) -> Result<Vec<f32>> {
        if self.config != ModelConfig::current() || packet.actions.is_empty() {
            return Err(Error::ModelContract);
        }
        let state = self.tree(&packet.state)?;
        let offset = HIDDEN * INPUT + HIDDEN * HIDDEN + HIDDEN;
        packet
            .actions
            .iter()
            .map(|action| {
                let encoded = self.tree(action)?;
                let mut logit = f64::from(self.weights[WEIGHTS - 1]);
                for h in 0..HIDDEN {
                    logit += f64::from(self.weights[offset + h]) * state[h]
                        + f64::from(self.weights[offset + HIDDEN + h]) * encoded[h]
                        + f64::from(self.weights[offset + 2 * HIDDEN + h]) * state[h] * encoded[h];
                }
                let logit = logit as f32;
                if logit.is_finite() {
                    Ok(logit)
                } else {
                    Err(Error::NonFiniteLogit)
                }
            })
            .collect()
    }
}
