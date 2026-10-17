//! Bias-free own-registration state-preactivation successor. Legacy ingress
//! cannot instantiate this type and never silently drops registration context.
use super::*;
use crate::own_registration_v1::{
    RegistrationExpectationV1, RegistrationFeatureRowsV1, RegistrationModeV1, WIDTH,
};

pub(crate) const ARCHITECTURE: &str = "native-net8-own-registration-state-projection/v1";
pub(crate) const PARAMETER_NAME: &str = "own_registration_state.weight";

#[derive(Clone, Debug)]
pub(crate) struct RegistrationInputWeightsV1 {
    values: Vec<f32>,
}
impl RegistrationInputWeightsV1 {
    pub(crate) fn new(values: Vec<f32>) -> Result<Self, NativePolicyValueErrorV1> {
        exact_len(
            "own_registration_weights",
            values.len(),
            HIDDEN_DIM_V1 * WIDTH,
        )?;
        if values.iter().any(|value| !value.is_finite()) {
            return Err(NativePolicyValueErrorV1::ParameterInvariant(
                "own_registration_weights_nonfinite",
            ));
        }
        Ok(Self { values })
    }
    pub(crate) fn zero() -> Self {
        Self {
            values: vec![0.0; HIDDEN_DIM_V1 * WIDTH],
        }
    }
    pub(crate) fn values(&self) -> &[f32] {
        &self.values
    }
    pub(crate) fn is_zero(&self) -> bool {
        self.values.iter().all(|value| *value == 0.0)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct NativeRegistrationNetV1 {
    base: NativePolicyValueNetV1,
    weights: RegistrationInputWeightsV1,
    mode: RegistrationModeV1,
    projection_enabled: bool,
}
impl NativeRegistrationNetV1 {
    pub(crate) fn new(
        base: NativePolicyValueNetV1,
        weights: RegistrationInputWeightsV1,
        mode: RegistrationModeV1,
    ) -> Result<Self, NativePolicyValueErrorV1> {
        base.validate_parameters_v1()?;
        Ok(Self {
            base,
            weights,
            mode,
            projection_enabled: true,
        })
    }
    pub(crate) fn new_legacy(
        base: NativePolicyValueNetV1,
    ) -> Result<Self, NativePolicyValueErrorV1> {
        let mut model = Self::new(
            base,
            RegistrationInputWeightsV1::zero(),
            RegistrationModeV1::Own,
        )?;
        model.projection_enabled = false;
        Ok(model)
    }
    pub(crate) fn architecture(&self) -> &'static str {
        ARCHITECTURE
    }
    pub(crate) fn forward_rows(
        &self,
        encoded: NativeEncodedDecisionViewV1<'_>,
        rows: &RegistrationFeatureRowsV1,
        expected: RegistrationExpectationV1<'_>,
    ) -> Result<NativePolicyValueOutputV1, NativePolicyValueErrorV1> {
        self.forward_rows_mode(encoded, rows, expected, ForwardActivationModeV1::LibmTanh)
    }
    pub(crate) fn forward_rows_mode(
        &self,
        encoded: NativeEncodedDecisionViewV1<'_>,
        rows: &RegistrationFeatureRowsV1,
        expected: RegistrationExpectationV1<'_>,
        activation: ForwardActivationModeV1,
    ) -> Result<NativePolicyValueOutputV1, NativePolicyValueErrorV1> {
        rows.validate_expected(self.mode, expected).map_err(|_| {
            NativePolicyValueErrorV1::ParameterInvariant("own_registration_context_contract")
        })?;
        let counts = encoded.validate(self.base.feature_transfer_config_v4())?;
        if !self.projection_enabled {
            return self
                .base
                .forward_feature_transfer_v4_mode(encoded, activation);
        }
        self.base.forward_registration_validated_rows_v1(
            encoded,
            counts,
            None,
            activation,
            None,
            None,
            Some((&self.weights, rows)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn weights_have_exact_finite_shape() {
        assert!(RegistrationInputWeightsV1::new(vec![]).is_err());
        assert!(RegistrationInputWeightsV1::new(vec![f32::NAN; 64 * WIDTH]).is_err());
        assert!(RegistrationInputWeightsV1::new(vec![f32::INFINITY; 64 * WIDTH]).is_err());
        assert!(RegistrationInputWeightsV1::zero().is_zero());
    }
}
