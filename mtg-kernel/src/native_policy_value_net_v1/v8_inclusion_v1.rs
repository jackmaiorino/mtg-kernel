//! The V8 inclusion net (collab LANES/w4-combat-20261030/DESIGN.md v5.1,
//! section 5, "V8 inclusion contract"): net-8 with exactly two additions for
//! the FlatV8 inputs (`native_flat_tensorizer_v8.rs`).
//!
//! - **Include column.** The action rows are one column wider (196): the
//!   include bit is column 195, so `action_encoder.0.weight` is `[64, 260]`,
//!   its columns being the 196 action features then the 64 pooled action-ref
//!   hidden values.
//! - **Pairing path.** An additive relation path for the pending blocker
//!   declaration (`pairing_node_indices`, `(blocker, attacker)` node pairs),
//!   on the stack-input precedent (`stack_inputs_v1.rs`): each row's input is
//!   the two nodes' object-encoder outputs (128), then `pairing_encoder.0`
//!   (64 outputs) and tanh, then the output projection `pairing_encoder.2`
//!   (64 outputs, no activation). The message is added into the pooled edge
//!   messages of both nodes, before the node update, value by value, skipping
//!   values equal to zero, so a zero message leaves the legacy bits, negative
//!   zeros included, unchanged.
//!
//! Net-8's constants, parameters, goldens and forwards are unchanged: the V8
//! net is net-8's type with its own architecture identity, config, manifest
//! and parameter count, and every net-8 path refuses its inputs (their action
//! width or pairing relation) and vice versa.

use super::*;

/// Architecture identity of the V8 inclusion net.
pub(crate) const V8_MODEL_ARCHITECTURE_VERSION_V1: &str = "kernel-policy-value-net-8-v8-inclusion";
/// FlatV8 action width: the FlatV7 width plus the include column.
pub(crate) const V8_ACTION_FEATURE_DIM_V1: usize = ACTION_FEATURE_DIM_V1 + 1;
/// The include column's index in a FlatV8 action row.
pub(crate) const V8_INCLUDE_COLUMN_V1: usize = ACTION_FEATURE_DIM_V1;
/// `action_encoder.0`'s input width in the V8 net.
pub(crate) const V8_ACTION_ENCODER_INPUT_V1: usize = V8_ACTION_FEATURE_DIM_V1 + HIDDEN_DIM_V1;
/// The pairing path's input: the blocker's then the attacker's
/// object-encoder output.
pub(crate) const V8_PAIRING_INPUT_DIM_V1: usize = 2 * HIDDEN_DIM_V1;
/// Net-8's count plus the include column (64), `pairing_encoder.0`
/// (64 x 128 + 64) and `pairing_encoder.2` (64 x 64 + 64).
pub(crate) const V8_PARAMETER_COUNT_V1: usize = PARAMETER_COUNT_V1
    + HIDDEN_DIM_V1
    + HIDDEN_DIM_V1 * V8_PAIRING_INPUT_DIM_V1
    + HIDDEN_DIM_V1
    + HIDDEN_DIM_V1 * HIDDEN_DIM_V1
    + HIDDEN_DIM_V1;
/// The pairing path's four tensors, appended after net-8's 33 in this order.
pub(crate) const V8_PAIRING_PARAMETER_NAMES_V1: [&str; 4] = [
    "pairing_encoder.0.weight",
    "pairing_encoder.0.bias",
    "pairing_encoder.2.weight",
    "pairing_encoder.2.bias",
];
pub(crate) const V8_PAIRING_PARAMETER_SHAPES_V1: [&[usize]; 4] = [
    &[HIDDEN_DIM_V1, V8_PAIRING_INPUT_DIM_V1],
    &[HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1, HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1],
];

impl NativePolicyValueModelConfigV1 {
    /// The V8 inclusion net's config: net-8's dimensions with the FlatV8
    /// action width, under the FlatV8 feature identity.
    pub(crate) const fn contract_v8_inclusion_v1() -> Self {
        use crate::native_flat_tensorizer_v8::*;
        Self {
            model_architecture_version: V8_MODEL_ARCHITECTURE_VERSION_V1,
            feature_schema_version: FEATURE_SCHEMA_VERSION_V8,
            feature_registry_version: FEATURE_REGISTRY_VERSION_V8,
            feature_contract_digest: FEATURE_CONTRACT_DIGEST_V8,
            feature_encoding_digest: FEATURE_ENCODING_DIGEST_V8,
            action_feature_dim: V8_ACTION_FEATURE_DIM_V1,
            ..Self::contract_v1()
        }
    }
}

/// The pairing path's two layers.
#[derive(Clone, Debug)]
pub(crate) struct PairingEncoderV1 {
    pub(super) first: LinearV1,
    pub(super) second: LinearV1,
}

impl PairingEncoderV1 {
    /// Adds each pairing row's message into the pooled edge messages of its
    /// blocker and then its attacker (see the module documentation). `pairs`
    /// is validated (`validate_pairing_v1`).
    pub(super) fn add_messages_v1(
        &self,
        pairs: &[i64],
        objects: &[f32],
        pooled: &mut [f32],
        activation: ForwardActivationModeV1,
    ) {
        if pairs.is_empty() {
            return;
        }
        let rows = pairs.len() / 2;
        let mut input = Vec::with_capacity(rows * V8_PAIRING_INPUT_DIM_V1);
        for &node in pairs {
            let begin = node as usize * HIDDEN_DIM_V1;
            input.extend_from_slice(&objects[begin..begin + HIDDEN_DIM_V1]);
        }
        let mut hidden = linear_rows_v1(&self.first, &input, rows);
        tanh_in_place_v1(&mut hidden, activation);
        let messages = linear_rows_v1(&self.second, &hidden, rows);
        for (pair, message) in pairs
            .chunks_exact(2)
            .zip(messages.chunks_exact(HIDDEN_DIM_V1))
        {
            for &node in pair {
                let begin = node as usize * HIDDEN_DIM_V1;
                add_nonzero_message_v1(&mut pooled[begin..begin + HIDDEN_DIM_V1], message);
            }
        }
    }
}

/// `pooled[k] += message[k]` for every nonzero `message[k]`: the stack-input
/// guard, which keeps negative-zero legacy values at a zero message. The
/// training forward and backward use the same rule (the backward treats the
/// add as unconditional, so the guard never suppresses a gradient).
pub(crate) fn add_nonzero_message_v1(pooled: &mut [f32], message: &[f32]) {
    for (sum, value) in pooled.iter_mut().zip(message) {
        if *value != 0.0 {
            *sum += value;
        }
    }
}

/// The guarded input column of `action_encoder.0` for an action width: the
/// include column for the V8 width, none for net-8.
pub(crate) const fn include_guard_v1(action_feature_dim: usize) -> Option<usize> {
    if action_feature_dim == V8_ACTION_FEATURE_DIM_V1 {
        Some(V8_INCLUDE_COLUMN_V1)
    } else {
        None
    }
}

/// `accumulator[o] += value * weights[o]` for every product that is not
/// zero: the include column's guard (`add_nonzero_message_v1`'s rule). A
/// zero product then leaves the accumulator's bits, negative zero included,
/// as net-8 computes them without the column. The backward still takes the
/// column's full derivative.
pub(crate) fn add_nonzero_products_v1(accumulator: &mut [f32], value: f32, weights: &[f32]) {
    debug_assert_eq!(accumulator.len(), weights.len());
    for (sum, &weight) in accumulator.iter_mut().zip(weights) {
        let product = value * weight;
        if product != 0.0 {
            *sum += product;
        }
    }
}

/// `accumulate_linear_row_v1` over a whole row, with column `guard` added
/// through `add_nonzero_products_v1`. Every output keeps one sequential sum
/// in input order, so the other columns' bits are those of the unguarded
/// loop.
pub(super) fn accumulate_guarded_row_v1(
    linear: &LinearV1,
    input_row: &[f32],
    guard: Option<usize>,
    accumulator: &mut [f32],
) {
    let Some(column) = guard else {
        accumulate_linear_row_v1(linear, input_row, 0, accumulator);
        return;
    };
    let output_dim = linear.output_dim;
    accumulate_linear_row_v1(linear, &input_row[..column], 0, accumulator);
    add_nonzero_products_v1(
        accumulator,
        input_row[column],
        &linear.weight_t[column * output_dim..(column + 1) * output_dim],
    );
    accumulate_linear_row_v1(linear, &input_row[column + 1..], column + 1, accumulator);
}

/// `linear_rows_v1` with the guarded column of `accumulate_guarded_row_v1`.
pub(super) fn linear_rows_guarded_v1(
    linear: &LinearV1,
    input: &[f32],
    rows: usize,
    guard: Option<usize>,
) -> Vec<f32> {
    debug_assert_eq!(input.len(), rows * linear.input_dim);
    let mut output = Vec::with_capacity(rows * linear.output_dim);
    for input_row in input.chunks_exact(linear.input_dim).take(rows) {
        let begin = output.len();
        output.extend_from_slice(&linear.bias);
        accumulate_guarded_row_v1(linear, input_row, guard, &mut output[begin..]);
    }
    output
}

/// A view's pairing relation: empty, or (only under the V8 config) whole
/// `(blocker, attacker)` pairs of two distinct object nodes.
pub(super) fn validate_pairing_v1(
    pairs: &[i64],
    object_count: usize,
    config: NativePolicyValueModelConfigV1,
) -> Result<(), NativePolicyValueErrorV1> {
    if pairs.is_empty() {
        return Ok(());
    }
    if config.model_architecture_version != V8_MODEL_ARCHITECTURE_VERSION_V1 {
        return Err(NativePolicyValueErrorV1::SchemaMismatch(
            "pairing_node_indices",
        ));
    }
    if !pairs.len().is_multiple_of(2) {
        return Err(NativePolicyValueErrorV1::ShapeMismatch {
            field: "pairing_node_indices",
            expected: 2,
            actual: pairs.len(),
        });
    }
    validate_indices("pairing_node_indices", pairs, object_count)?;
    if pairs.chunks_exact(2).any(|pair| pair[0] == pair[1]) {
        return Err(NativePolicyValueErrorV1::ParameterInvariant(
            "pairing_node_indices",
        ));
    }
    Ok(())
}

/// The architecture invariants of a net: net-8 has no pairing path and its
/// own action width; the V8 net has the V8 config, the widened
/// `action_encoder.0` and a finite pairing path of the declared shapes.
pub(super) fn validate_variant_v1(
    config: NativePolicyValueModelConfigV1,
    action_first: &LinearV1,
    pairing: Option<&PairingEncoderV1>,
) -> Result<(), NativePolicyValueErrorV1> {
    let v8 = config.model_architecture_version == V8_MODEL_ARCHITECTURE_VERSION_V1;
    let expected_action_input = if v8 {
        V8_ACTION_ENCODER_INPUT_V1
    } else {
        ACTION_ENCODER_INPUT_V1
    };
    if action_first.input_dim != expected_action_input
        || config.action_feature_dim + HIDDEN_DIM_V1 != expected_action_input
    {
        return Err(NativePolicyValueErrorV1::ParameterInvariant(
            "action_encoder.0",
        ));
    }
    match (v8, pairing) {
        (false, None) => Ok(()),
        (true, Some(pairing)) => {
            if config != NativePolicyValueModelConfigV1::contract_v8_inclusion_v1()
                || pairing.first.input_dim != V8_PAIRING_INPUT_DIM_V1
                || pairing.first.output_dim != HIDDEN_DIM_V1
                || pairing.second.input_dim != HIDDEN_DIM_V1
                || pairing.second.output_dim != HIDDEN_DIM_V1
            {
                return Err(NativePolicyValueErrorV1::ParameterInvariant(
                    "pairing_encoder",
                ));
            }
            pairing.first.validate("pairing_encoder.0")?;
            pairing.second.validate("pairing_encoder.2")
        }
        _ => Err(NativePolicyValueErrorV1::ParameterInvariant(
            "pairing_encoder",
        )),
    }
}

impl NativePolicyValueNetV1 {
    /// Whether this is the V8 inclusion net.
    pub(crate) fn is_v8_inclusion_v1(&self) -> bool {
        self.pairing.is_some()
    }

    /// The V8 net of `self` (net-8) with `pairing_first_weight` as
    /// `pairing_encoder.0.weight` (row-major `[64, 128]`) and every other new
    /// coordinate +0: the include column of `action_encoder.0.weight`, both
    /// pairing biases and the output projection `pairing_encoder.2.weight`.
    /// Every net-8 parameter keeps its bits, at its old coordinate (the
    /// pooled action-ref columns move up by one to make room for the include
    /// column), so the V8 net computes net-8's function on every FlatV8 row
    /// whose relation is empty or whose messages are zero.
    pub(crate) fn into_v8_inclusion_v1(
        self,
        pairing_first_weight: Vec<f32>,
    ) -> Result<Self, NativePolicyValueErrorV1> {
        self.config.validate()?;
        if self.pairing.is_some() {
            return Err(NativePolicyValueErrorV1::ParameterInvariant(
                "pairing_encoder",
            ));
        }
        exact_len(
            "pairing_encoder.0.weight",
            pairing_first_weight.len(),
            HIDDEN_DIM_V1 * V8_PAIRING_INPUT_DIM_V1,
        )?;
        let mut model = self;
        let old = &model.action_encoder.first;
        let mut weight = Vec::with_capacity(old.output_dim * V8_ACTION_ENCODER_INPUT_V1);
        for row in old.weight.chunks_exact(old.input_dim) {
            weight.extend_from_slice(&row[..ACTION_FEATURE_DIM_V1]);
            weight.push(0.0);
            weight.extend_from_slice(&row[ACTION_FEATURE_DIM_V1..]);
        }
        model.action_encoder.first = LinearV1 {
            input_dim: V8_ACTION_ENCODER_INPUT_V1,
            output_dim: old.output_dim,
            weight,
            bias: old.bias.clone(),
            weight_t: Vec::new(),
        }
        .with_transposed_weight_v1();
        model.pairing = Some(PairingEncoderV1 {
            first: LinearV1 {
                input_dim: V8_PAIRING_INPUT_DIM_V1,
                output_dim: HIDDEN_DIM_V1,
                weight: pairing_first_weight,
                bias: vec![0.0; HIDDEN_DIM_V1],
                weight_t: Vec::new(),
            }
            .with_transposed_weight_v1(),
            second: LinearV1 {
                input_dim: HIDDEN_DIM_V1,
                output_dim: HIDDEN_DIM_V1,
                weight: vec![0.0; HIDDEN_DIM_V1 * HIDDEN_DIM_V1],
                bias: vec![0.0; HIDDEN_DIM_V1],
                weight_t: Vec::new(),
            }
            .with_transposed_weight_v1(),
        });
        model.config = NativePolicyValueModelConfigV1::contract_v8_inclusion_v1();
        model.validate_parameters_v1()?;
        if model.parameter_count_v1() != V8_PARAMETER_COUNT_V1 {
            return Err(NativePolicyValueErrorV1::ParameterInvariant(
                "parameter_count",
            ));
        }
        Ok(model)
    }

    /// A V8 net to load a V8 snapshot into (`replace_parameter_snapshot_v1`):
    /// the runner-fixed net-8 with every new coordinate +0.
    pub(crate) fn v8_inclusion_template_v1() -> Result<Self, NativePolicyValueErrorV1> {
        Self::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())?
            .into_v8_inclusion_v1(vec![0.0; HIDDEN_DIM_V1 * V8_PAIRING_INPUT_DIM_V1])
    }

    /// The FlatV8 input config: the V8 net's own; any other net's carries
    /// net-8's action width, which no FlatV8 view has.
    pub(crate) fn feature_transfer_config_v8(&self) -> NativePolicyValueModelConfigV1 {
        if self.pairing.is_some() {
            return self.config;
        }
        use crate::native_flat_tensorizer_v8::*;
        NativePolicyValueModelConfigV1 {
            feature_schema_version: FEATURE_SCHEMA_VERSION_V8,
            feature_registry_version: FEATURE_REGISTRY_VERSION_V8,
            feature_contract_digest: FEATURE_CONTRACT_DIGEST_V8,
            feature_encoding_digest: FEATURE_ENCODING_DIGEST_V8,
            ..self.config
        }
    }

    /// The reference forward over FlatV8 inputs (pairing relation included).
    pub(crate) fn forward_feature_transfer_v8_mode(
        &self,
        encoded: NativeEncodedDecisionViewV1<'_>,
        activation_mode: ForwardActivationModeV1,
    ) -> Result<NativePolicyValueOutputV1, NativePolicyValueErrorV1> {
        let counts = encoded.validate(self.feature_transfer_config_v8())?;
        self.forward_validated_rows_v1(encoded, counts, None, activation_mode)
    }

    /// [`Self::forward_feature_transfer_v8_mode`] that also returns the state
    /// encoder output and every action row's scorer hidden, as
    /// [`Self::forward_feature_transfer_v4_hidden_v1`]. Measurement only (W4
    /// combat diagnostics).
    #[cfg(any(test, feature = "experimental-burn-net8-packed-cuda-v1"))]
    pub(crate) fn forward_feature_transfer_v8_hidden_v1(
        &self,
        encoded: NativeEncodedDecisionViewV1<'_>,
        activation_mode: ForwardActivationModeV1,
    ) -> Result<(NativePolicyValueOutputV1, NativeHiddenCaptureV1), NativePolicyValueErrorV1> {
        let counts = encoded.validate(self.feature_transfer_config_v8())?;
        let mut capture = NativeHiddenCaptureV1::default();
        let output = self.forward_registration_validated_rows_v1(
            encoded,
            counts,
            None,
            activation_mode,
            None,
            None,
            None,
            Some(&mut capture),
        )?;
        Ok((output, capture))
    }
}
