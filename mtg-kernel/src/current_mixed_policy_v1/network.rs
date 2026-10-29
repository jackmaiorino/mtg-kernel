//! Separate schema-tree forward. No predecessor model/checkpoint loader is used.
use super::{numeric::NumericTree, Error, Packet, Result};
pub const INPUT: usize = super::numeric::NODE_WIDTH + 8;
const _: () = assert!(INPUT <= u32::BITS as usize);
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
struct Layout {
    input: [[f64; HIDDEN]; INPUT],
    hidden: [[f64; HIDDEN]; HIDDEN],
    bias: [f64; HIDDEN],
    positive_zero_columns: [u32; HIDDEN],
    finite: bool,
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
    /// The node weights as exact f64 values, indexed input-major so the
    /// hidden units of one input sit together.
    fn layout(&self) -> Layout {
        Layout {
            input: std::array::from_fn(|i| {
                std::array::from_fn(|h| f64::from(self.weights[h * INPUT + i]))
            }),
            hidden: std::array::from_fn(|i| {
                std::array::from_fn(|h| f64::from(self.weights[HIDDEN * INPUT + h * HIDDEN + i]))
            }),
            bias: std::array::from_fn(|h| {
                f64::from(self.weights[HIDDEN * INPUT + HIDDEN * HIDDEN + h])
            }),
            // Numeric features are unsigned u16 limbs, so every zero input is
            // +0. Its product's sign depends only on the weight's sign bit.
            positive_zero_columns: std::array::from_fn(|h| {
                (0..INPUT).fold(0, |mask, i| {
                    mask | (u32::from(self.weights[h * INPUT + i].is_sign_positive()) << i)
                })
            }),
            // Constructors admit only finite weights; zero features are
            // skipped only under that condition, rechecked here.
            finite: self.weights.iter().all(|weight| weight.is_finite()),
        }
    }
    fn tree(&self, layout: &Layout, tree: &NumericTree) -> Result<[f64; HIDDEN]> {
        Self::tree_nodes(layout, tree).map(|(_, pooled)| pooled)
    }
    /// Every node's hidden values, in row order, and their mean.
    fn tree_nodes(
        layout: &Layout,
        tree: &NumericTree,
    ) -> Result<(Vec<[f64; HIDDEN]>, [f64; HIDDEN])> {
        let mut previous = [0.; HIDDEN];
        let mut nodes: Vec<[f64; HIDDEN]> = Vec::with_capacity(tree.rows().len());
        let mut pooled = [0.; HIDDEN];
        // The same rows `NumericTree::tensors` yields; each limb is a u16, so
        // its f64 value equals the f64 of its f32 tensor value.
        for (index, (row, (parent_limbs, ordinal))) in
            tree.rows().iter().zip(tree.topology()).enumerate()
        {
            let mut features = [0.; INPUT];
            for (to, from) in features
                .iter_mut()
                .zip(row.iter().chain(parent_limbs).chain(ordinal))
            {
                *to = f64::from(*from);
            }
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
            // Each hidden unit adds exactly the same terms in the same order as
            // a unit-at-a-time loop. Units never read one another's sums, so
            // advancing all of them per input changes no bit of any value.
            let mut next = layout.bias;
            // A zero feature with a finite weight adds a signed zero. Adding a
            // zero never changes a sum's value, and a sum is -0 only when every
            // addend is -0, so a skipped +0 matters only by turning a final -0
            // sum into +0. That correction is applied once the features are in.
            let mut skipped_zero_columns = 0u32;
            for (i, (feature, weights)) in features.iter().zip(&layout.input).enumerate() {
                if *feature == 0. && layout.finite {
                    skipped_zero_columns |= 1 << i;
                    continue;
                }
                for (value, weight) in next.iter_mut().zip(weights) {
                    *value += weight * feature / 65535.;
                }
            }
            for (value, positive_columns) in next.iter_mut().zip(layout.positive_zero_columns) {
                if *value == 0. && skipped_zero_columns & positive_columns != 0 {
                    *value = 0.;
                }
            }
            for (i, weights) in layout.hidden.iter().enumerate() {
                let input = previous[i] + parent[i];
                for (value, weight) in next.iter_mut().zip(weights) {
                    *value += weight * input / 2.;
                }
            }
            for (value, pooled) in next.iter_mut().zip(&mut pooled) {
                *value = value.tanh();
                *pooled += *value;
            }
            nodes.push(next);
            previous = next;
        }
        let count = tree.rows().len().max(1) as f64;
        for value in &mut pooled {
            *value /= count;
        }
        Ok((nodes, pooled))
    }
    pub fn forward(&self, packet: &Packet) -> Result<Vec<f32>> {
        #[cfg(test)]
        let _timing = crate::rl_session::test_timing_v1::span(
            crate::rl_session::test_timing_v1::Phase::NativeModelForwardInclusive,
        );
        if self.config != ModelConfig::current() || packet.actions.is_empty() {
            return Err(Error::ModelContract);
        }
        let layout = self.layout();
        let state = self.tree(&layout, &packet.state)?;
        let offset = HIDDEN * INPUT + HIDDEN * HIDDEN + HIDDEN;
        packet
            .actions
            .iter()
            .map(|action| {
                let encoded = self.tree(&layout, action)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    // The unit-at-a-time loop the side-by-side evaluation replaced.
    fn reference_tree(
        model: &NativeModel,
        tree: &NumericTree,
    ) -> (Vec<[f64; HIDDEN]>, [f64; HIDDEN]) {
        let mut previous = [0.; HIDDEN];
        let mut nodes: Vec<[f64; HIDDEN]> = Vec::new();
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
                nodes[usize::try_from(parent - 1).unwrap()]
            };
            let mut next = [0.; HIDDEN];
            for h in 0..HIDDEN {
                let mut value = f64::from(model.weights[HIDDEN * INPUT + HIDDEN * HIDDEN + h]);
                for (i, feature) in row.iter().enumerate() {
                    value += f64::from(model.weights[h * INPUT + i]) * f64::from(*feature) / 65535.;
                }
                for i in 0..HIDDEN {
                    value += f64::from(model.weights[HIDDEN * INPUT + h * HIDDEN + i])
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
        (nodes, pooled)
    }

    #[test]
    fn side_by_side_hidden_units_match_unit_at_a_time_bits() {
        let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
        let mut random = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut models = vec![
            NativeModel::deterministic_fixture().unwrap(),
            // No skipped positive zero exists with all negative-zero weights.
            NativeModel::from_parameters(ModelConfig::current(), vec![-0.; WEIGHTS]).unwrap(),
        ];
        for _ in 0..4 {
            let weights = (0..WEIGHTS)
                .map(|_| (random() % 4001) as f32 / 1000. - 2.)
                .collect();
            models.push(NativeModel::from_parameters(ModelConfig::current(), weights).unwrap());
        }
        // Signed zeros everywhere exercise the zero-feature sign correction.
        for choices in [&[0., -0.][..], &[0., -0., 0.25, -0.25][..]] {
            let weights = (0..WEIGHTS)
                .map(|_| choices[(random() % choices.len() as u64) as usize])
                .collect();
            models.push(NativeModel::from_parameters(ModelConfig::current(), weights).unwrap());
        }
        let values = [
            serde_json::json!(null),
            serde_json::json!({"kind": "divine", "targets": [1, 2, 65535, 4_294_967_296_u64]}),
            serde_json::json!([{"a": -7, "b": "text", "c": [true, false, -3]}, {"d": {"e": [[], {}]}}]),
            serde_json::json!({"wide": (0..64).collect::<Vec<_>>(), "name": "Llanowar Elves"}),
            serde_json::json!({"full_width": [u64::MAX, 65535_u64 << 48], "signed_min": i64::MIN}),
        ];
        for model in &models {
            for value in &values {
                let tree = NumericTree::encode(value).unwrap();
                let bits = |(nodes, pooled): (Vec<[f64; HIDDEN]>, [f64; HIDDEN])| {
                    let nodes = nodes
                        .iter()
                        .map(|node| node.map(f64::to_bits))
                        .collect::<Vec<_>>();
                    (nodes, pooled.map(f64::to_bits))
                };
                assert_eq!(
                    bits(NativeModel::tree_nodes(&model.layout(), &tree).unwrap()),
                    bits(reference_tree(model, &tree))
                );
            }
        }
    }
}
