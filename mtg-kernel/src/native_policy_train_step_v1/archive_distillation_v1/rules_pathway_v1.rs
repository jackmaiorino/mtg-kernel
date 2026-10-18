//! RC1's rules pathway (collab LANES/representation-comparison-20261018):
//! a 16 x F weight W, zero at the start, that adds W x_c to card c's
//! embedding wherever the embedding is read. x_c is a fixed per-card input
//! row (the rules vector, or another card's under the wrong-association
//! control); token t = card id + 1 reads row t - 1, padding and every token
//! past the rows read nothing.
//!
//! The network keeps its own card table E. A step reads the effective table
//! E + X Wᵀ; since its gradient with respect to E equals the gradient with
//! respect to the effective table G, the network's Adam step is unchanged,
//! and W's gradient is Gᵀ X. W takes the same Adam at the same age. A zero
//! product is never added, so at W = 0 the effective table is E bit for bit.

use super::*;

/// The pathway's fixed rows and trainable weight with its Adam moments.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RulesPathwayV1 {
    /// Row t - 1 is token t's input, `cards * width` values.
    rows: Vec<f32>,
    cards: usize,
    width: usize,
    /// Row-major `[CARD_EMBEDDING_DIM_V1][width]`.
    weight: Vec<f32>,
    first: Vec<f32>,
    second: Vec<f32>,
    /// ‖Gᵀ X‖ of the last committed step.
    last_gradient_norm: f64,
}

/// One computed pathway step, assigned by [`RulesPathwayV1::commit_v1`].
pub(crate) struct RulesPathwayNextV1 {
    weight: Vec<f32>,
    first: Vec<f32>,
    second: Vec<f32>,
    gradient_norm: f64,
}

/// The pathway's saved state: weight and moment bits.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RulesPathwayStateBitsV1 {
    pub(crate) weight: Vec<u32>,
    pub(crate) first_moments: Vec<u32>,
    pub(crate) second_moments: Vec<u32>,
}

fn bits_of(values: &[f32]) -> Vec<u32> {
    values.iter().map(|v| v.to_bits()).collect()
}

impl RulesPathwayV1 {
    /// A zero weight with zero moments over `rows` (`cards` rows of `width`
    /// finite values, at most the table's card tokens).
    pub(crate) fn new_v1(
        rows: Vec<f32>,
        cards: usize,
        width: usize,
    ) -> Result<Self, NativePolicyTrainErrorV1> {
        if width == 0
            || cards == 0
            || cards >= CARD_VOCAB_SIZE_V1
            || rows.len() != cards * width
            || rows.iter().any(|v| !v.is_finite())
        {
            return Err(invalid("rules-pathway-rows"));
        }
        let size = CARD_EMBEDDING_DIM_V1 * width;
        Ok(Self {
            rows,
            cards,
            width,
            weight: vec![0.0; size],
            first: vec![0.0; size],
            second: vec![0.0; size],
            last_gradient_norm: 0.0,
        })
    }

    pub(crate) fn cards_v1(&self) -> usize {
        self.cards
    }

    pub(crate) fn weight_v1(&self) -> &[f32] {
        &self.weight
    }

    pub(crate) fn last_gradient_norm_v1(&self) -> f64 {
        self.last_gradient_norm
    }

    pub(crate) fn weight_norm_v1(&self) -> f64 {
        self.weight
            .iter()
            .map(|&w| f64::from(w) * f64::from(w))
            .sum::<f64>()
            .sqrt()
    }

    pub(crate) fn state_bits_v1(&self) -> RulesPathwayStateBitsV1 {
        RulesPathwayStateBitsV1 {
            weight: bits_of(&self.weight),
            first_moments: bits_of(&self.first),
            second_moments: bits_of(&self.second),
        }
    }

    /// The saved weight and moments (sizes and finiteness checked).
    pub(crate) fn restore_v1(
        &mut self,
        state: &RulesPathwayStateBitsV1,
    ) -> Result<(), NativePolicyTrainErrorV1> {
        let read = |bits: &[u32]| -> Result<Vec<f32>, NativePolicyTrainErrorV1> {
            let values: Vec<f32> = bits.iter().map(|&b| f32::from_bits(b)).collect();
            if values.len() != self.weight.len() || values.iter().any(|v| !v.is_finite()) {
                return Err(invalid("rules-pathway-state"));
            }
            Ok(values)
        };
        let (weight, first, second) = (
            read(&state.weight)?,
            read(&state.first_moments)?,
            read(&state.second_moments)?,
        );
        if second.iter().any(|&v| v < 0.0) {
            return Err(invalid("rules-pathway-state"));
        }
        self.weight = weight;
        self.first = first;
        self.second = second;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn set_weight_v1(&mut self, weight: Vec<f32>) {
        assert_eq!(weight.len(), self.weight.len());
        self.weight = weight;
    }

    /// Token t's added embedding, `W x_{t-1}`, summed in feature order.
    pub(crate) fn delta_v1(&self, token: usize) -> [f32; CARD_EMBEDDING_DIM_V1] {
        let mut delta = [0.0f32; CARD_EMBEDDING_DIM_V1];
        if token == 0 || token > self.cards {
            return delta;
        }
        let x = &self.rows[(token - 1) * self.width..token * self.width];
        for (k, out) in delta.iter_mut().enumerate() {
            let w = &self.weight[k * self.width..(k + 1) * self.width];
            let mut sum = 0.0f32;
            for (&wf, &xf) in w.iter().zip(x) {
                if xf != 0.0 {
                    sum += wf * xf;
                }
            }
            *out = sum;
        }
        delta
    }

    /// `parameters` with the card table replaced by the effective table.
    pub(crate) fn effective_parameters_v1(
        &self,
        parameters: &[NativeNamedParameterV1],
    ) -> Result<Vec<NativeNamedParameterV1>, NativePolicyTrainErrorV1> {
        let mut effective = parameters.to_vec();
        let table = &mut effective[CARD_EMBEDDING].values;
        if table.len() != CARD_VOCAB_SIZE_V1 * CARD_EMBEDDING_DIM_V1 {
            return Err(invalid("rules-pathway-table"));
        }
        for token in 1..=self.cards {
            let delta = self.delta_v1(token);
            let row = &mut table[token * CARD_EMBEDDING_DIM_V1..(token + 1) * CARD_EMBEDDING_DIM_V1];
            for (value, d) in row.iter_mut().zip(delta) {
                if d != 0.0 {
                    *value += d;
                }
            }
        }
        validate_finite_slice("rules_pathway_effective_table", table)?;
        Ok(effective)
    }

    /// `Gᵀ X` from the effective table's gradient, summed in token order.
    pub(crate) fn weight_gradient_v1(
        &self,
        table_gradient: &[f32],
    ) -> Result<Vec<f32>, NativePolicyTrainErrorV1> {
        if table_gradient.len() != CARD_VOCAB_SIZE_V1 * CARD_EMBEDDING_DIM_V1 {
            return Err(invalid("rules-pathway-gradient"));
        }
        let mut gradient = vec![0.0f32; self.weight.len()];
        for token in 1..=self.cards {
            let g = &table_gradient[token * CARD_EMBEDDING_DIM_V1..(token + 1) * CARD_EMBEDDING_DIM_V1];
            let x = &self.rows[(token - 1) * self.width..token * self.width];
            for (k, &gk) in g.iter().enumerate() {
                if gk == 0.0 {
                    continue;
                }
                let out = &mut gradient[k * self.width..(k + 1) * self.width];
                for (o, &xf) in out.iter_mut().zip(x) {
                    if xf != 0.0 {
                        *o += gk * xf;
                    }
                }
            }
        }
        validate_finite_slice("rules_pathway_gradient", &gradient)?;
        Ok(gradient)
    }

    /// W's Adam step at age `step` from the effective table's gradient.
    pub(crate) fn adam_step_v1(
        &self,
        table_gradient: &[f32],
        step: u64,
        learning_rate: f32,
        math: TrainingMathV1,
    ) -> Result<RulesPathwayNextV1, NativePolicyTrainErrorV1> {
        let gradient = self.weight_gradient_v1(table_gradient)?;
        let gradient_norm = gradient
            .iter()
            .map(|&g| f64::from(g) * f64::from(g))
            .sum::<f64>()
            .sqrt();
        let parameter = NativeNamedParameterV1 {
            name: "rules_pathway.weight",
            shape: vec![CARD_EMBEDDING_DIM_V1, self.width],
            values: self.weight.clone(),
        };
        let (mut weight, mut first, mut second) = adam_update(
            std::slice::from_ref(&parameter),
            std::slice::from_ref(&gradient),
            std::slice::from_ref(&self.first),
            std::slice::from_ref(&self.second),
            step,
            learning_rate,
            math,
        )?;
        Ok(RulesPathwayNextV1 {
            weight: weight.remove(0).values,
            first: first.remove(0),
            second: second.remove(0),
            gradient_norm,
        })
    }

    /// Assigns a computed step.
    pub(crate) fn commit_v1(&mut self, next: RulesPathwayNextV1) {
        self.weight = next.weight;
        self.first = next.first;
        self.second = next.second;
        self.last_gradient_norm = next.gradient_norm;
    }
}

#[cfg(test)]
mod tests;
