//! Separate public-input successor. The legacy checkpoint and scorer contracts
//! remain intact; this wrapper is not admitted by legacy inference loaders.
use super::*;
use crate::policy_observation_v6::ObservationV6;
use crate::public_cost_features_v1::{from_actor_v4_v1, OBJECT_WIDTH, STATE_WIDTH};

pub(crate) const ARCHITECTURE: &str = "native-net8-public-input-projections/v1";

#[derive(Clone, Debug)]
pub(crate) struct PublicInputWeightsV1 {
    pub(super) object: Vec<f32>,
    pub(super) state: Vec<f32>,
}

impl PublicInputWeightsV1 {
    pub(crate) fn new(object: Vec<f32>, state: Vec<f32>) -> Result<Self, NativePolicyValueErrorV1> {
        exact_len("public_object_weights", object.len(), HIDDEN_DIM_V1 * OBJECT_WIDTH)?;
        exact_len("public_state_weights", state.len(), HIDDEN_DIM_V1 * STATE_WIDTH)?;
        if object.iter().chain(&state).any(|v| !v.is_finite()) {
            return Err(NativePolicyValueErrorV1::ParameterInvariant("public_weights_nonfinite"));
        }
        Ok(Self { object, state })
    }

    pub(crate) fn zero() -> Self {
        Self { object: vec![0.0; HIDDEN_DIM_V1 * OBJECT_WIDTH], state: vec![0.0; HIDDEN_DIM_V1 * STATE_WIDTH] }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct NativePublicInputNetV1 {
    base: NativePolicyValueNetV1,
    weights: PublicInputWeightsV1,
}

impl NativePublicInputNetV1 {
    pub(crate) fn new(base: NativePolicyValueNetV1, weights: PublicInputWeightsV1) -> Result<Self, NativePolicyValueErrorV1> {
        base.validate_parameters_v1()?;
        Ok(Self { base, weights })
    }

    pub(crate) fn architecture(&self) -> &'static str { ARCHITECTURE }

    /// Both arguments must describe the same validated actor decision. Auxiliary
    /// features are derived here, never accepted from hidden-state callers.
    pub(crate) fn forward(
        &self,
        encoded: NativeEncodedDecisionViewV1<'_>,
        observation: &ObservationV6,
    ) -> Result<NativePolicyValueOutputV1, NativePolicyValueErrorV1> {
        let counts = encoded.validate(self.base.feature_transfer_config_v4())?;
        let rows = from_actor_v4_v1(observation, encoded.object_card_ids)
            .map_err(|_| NativePolicyValueErrorV1::ParameterInvariant("public_observation_contract"))?;
        self.base.forward_public_validated_rows_v1(encoded, counts, None,
            ForwardActivationModeV1::LibmTanh, Some((&self.weights, &rows)))
    }
}

pub(super) fn apply_optional_public_projection_v1(
    encoder: &TwoLayerTanhV1,
    input: &[f32],
    rows: usize,
    activation: ForwardActivationModeV1,
    public: Option<(&[f32], Vec<f32>, usize)>,
) -> Vec<f32> {
    let Some((weights, features, width)) = public else {
        return apply_two_layer_tanh_rows_v1(encoder, input, rows, activation);
    };
    debug_assert_eq!(weights.len(), HIDDEN_DIM_V1 * width);
    debug_assert_eq!(features.len(), rows * width);
    // Keep the existing dot product separate and unchanged. No larger GEMM or
    // interleaved summation may alter the warm-start reference calculation.
    let mut hidden = linear_rows_v1(&encoder.first, input, rows);
    for row in 0..rows {
        for out in 0..HIDDEN_DIM_V1 {
            let mut addition = 0.0;
            for col in 0..width {
                addition += features[row * width + col] * weights[out * width + col];
            }
            // Preserve even a negative-zero legacy result at zero projection.
            if addition != 0.0 { hidden[row * HIDDEN_DIM_V1 + out] += addition; }
        }
    }
    tanh_in_place_v1(&mut hidden, activation);
    let mut output = linear_rows_v1(&encoder.second, &hidden, rows);
    tanh_in_place_v1(&mut output, activation);
    output
}

#[cfg(test)]
mod tests;
