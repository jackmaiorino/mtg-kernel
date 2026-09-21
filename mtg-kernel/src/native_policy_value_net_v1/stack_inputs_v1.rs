//! Separate zero-initialized per-stack-message successor, no legacy loader change.
use super::*;
use crate::public_stack_features_v1::{StackEncodedDecisionV1, StackFeatureRowsV1, FEATURE_WIDTH};
pub(crate) const ARCHITECTURE: &str = "native-net8-public-stack-messages/v1";
pub(crate) const INPUT_WIDTH: usize = FEATURE_WIDTH + 2 * HIDDEN_DIM_V1;

#[derive(Clone, Debug)]
pub(crate) struct StackInputWeightsV1 {
    pub(crate) values: Vec<f32>,
}
impl StackInputWeightsV1 {
    pub(crate) fn new(values: Vec<f32>) -> Result<Self, NativePolicyValueErrorV1> {
        exact_len(
            "stack_message_weights",
            values.len(),
            HIDDEN_DIM_V1 * INPUT_WIDTH,
        )?;
        if values.iter().any(|v| !v.is_finite()) {
            return Err(NativePolicyValueErrorV1::ParameterInvariant(
                "nonfinite_stack_weights",
            ));
        }
        Ok(Self { values })
    }
    pub(crate) fn zero() -> Self {
        Self {
            values: vec![0.0; HIDDEN_DIM_V1 * INPUT_WIDTH],
        }
    }
    pub(super) fn add_messages(
        &self,
        rows: &StackFeatureRowsV1,
        objects: &[f32],
        pooled: &mut [f32],
        activation: ForwardActivationModeV1,
    ) {
        for row in &rows.rows {
            let mut input = Vec::with_capacity(INPUT_WIDTH);
            rows.append_model_features(row, &mut input);
            input.extend_from_slice(
                &objects[row.source_node * HIDDEN_DIM_V1..(row.source_node + 1) * HIDDEN_DIM_V1],
            );
            if let Some(target) = row.target_node {
                input.extend_from_slice(
                    &objects[target * HIDDEN_DIM_V1..(target + 1) * HIDDEN_DIM_V1],
                );
            } else {
                input.resize(INPUT_WIDTH, 0.0);
            }
            let mut message = vec![0.0; HIDDEN_DIM_V1];
            for (output, weights) in message
                .iter_mut()
                .zip(self.values.chunks_exact(INPUT_WIDTH))
            {
                for (weight, value) in weights.iter().zip(&input) {
                    *output += weight * value;
                }
            }
            tanh_in_place_v1(&mut message, activation);
            for node in std::iter::once(row.source_node)
                .chain(row.target_node.filter(|target| *target != row.source_node))
            {
                for (sum, value) in pooled[node * HIDDEN_DIM_V1..(node + 1) * HIDDEN_DIM_V1]
                    .iter_mut()
                    .zip(&message)
                {
                    if *value != 0.0 {
                        *sum += value;
                    } // Preserve negative-zero legacy values at exact-zero initialization.
                }
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct NativeStackInputNetV1 {
    base: NativePolicyValueNetV1,
    weights: StackInputWeightsV1,
    inputs_enabled: bool,
}
impl NativeStackInputNetV1 {
    pub(crate) fn new(
        base: NativePolicyValueNetV1,
        weights: StackInputWeightsV1,
    ) -> Result<Self, NativePolicyValueErrorV1> {
        base.validate_parameters_v1()?;
        Ok(Self {
            base,
            weights,
            inputs_enabled: true,
        })
    }
    pub(crate) fn with_inputs_enabled(mut self, enabled: bool) -> Self {
        self.inputs_enabled = enabled;
        self
    }
    pub(crate) fn forward(
        &self,
        decision: &StackEncodedDecisionV1,
    ) -> Result<NativePolicyValueOutputV1, NativePolicyValueErrorV1> {
        let view = decision.view();
        let counts = view.validate(self.base.feature_transfer_config_v4())?;
        decision
            .stack
            .validate(counts.object_count)
            .map_err(|_| NativePolicyValueErrorV1::ParameterInvariant("invalid_stack_rows"))?;
        self.base.forward_stack_and_public_validated_rows_v1(
            view,
            counts,
            None,
            ForwardActivationModeV1::LibmTanh,
            None,
            if self.inputs_enabled {
                Some((&self.weights, &decision.stack))
            } else {
                None
            },
        )
    }
}
