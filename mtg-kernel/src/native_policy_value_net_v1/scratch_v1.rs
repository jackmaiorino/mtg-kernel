//! Caller-owned intermediate storage for the ordinary Net8 inference paths.
//!
//! No model-derived values are reused across calls. Every active row is rebuilt,
//! so a scratch instance can be reused after a shape change, failed call or
//! parameter replacement. The owned-output entry points remain the reference.

use super::*;

#[derive(Clone, Debug, Default)]
pub(crate) struct NativePolicyValueForwardScratchV1 {
    input: Vec<f32>,
    hidden: Vec<f32>,
    object_base_hidden: Vec<f32>,
    edge_pooled: Vec<f32>,
    object_hidden: Vec<f32>,
    pooled_objects: Vec<f32>,
    state_hidden: Vec<f32>,
    action_ref_pooled: Vec<f32>,
    intermediate: Vec<f32>,
    action_hidden: Vec<f32>,
    scorer_hidden: Vec<f32>,
    state_prefix: Vec<f32>,
    logits: Vec<f32>,
}

#[cfg(test)]
impl NativePolicyValueForwardScratchV1 {
    pub(super) fn allocation_layout_v1(&self) -> [(usize, usize); 13] {
        [
            &self.input,
            &self.hidden,
            &self.object_base_hidden,
            &self.edge_pooled,
            &self.object_hidden,
            &self.pooled_objects,
            &self.state_hidden,
            &self.action_ref_pooled,
            &self.intermediate,
            &self.action_hidden,
            &self.scorer_hidden,
            &self.state_prefix,
            &self.logits,
        ]
        .map(|buffer| (buffer.as_ptr() as usize, buffer.capacity()))
    }
}

/// Available only after all input and output validation has succeeded. The
/// borrow prevents reuse of the scratch storage while its output is in use.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NativePolicyValueOutputViewV1<'a> {
    pub(crate) logits: &'a [f32],
    pub(crate) value: f32,
}

impl NativePolicyValueNetV1 {
    pub(crate) fn forward_with_scratch_v1<'a>(
        &self,
        encoded: NativeEncodedDecisionViewV1<'_>,
        scratch: &'a mut NativePolicyValueForwardScratchV1,
    ) -> Result<NativePolicyValueOutputViewV1<'a>, NativePolicyValueErrorV1> {
        let counts = encoded.validate(self.config)?;
        self.forward_validated_with_scratch_v1(encoded, counts, scratch)
    }

    pub(crate) fn forward_feature_transfer_v3_with_scratch_v1<'a>(
        &self,
        encoded: NativeEncodedDecisionViewV1<'_>,
        scratch: &'a mut NativePolicyValueForwardScratchV1,
    ) -> Result<NativePolicyValueOutputViewV1<'a>, NativePolicyValueErrorV1> {
        let counts = encoded.validate(self.feature_transfer_config_v3())?;
        self.forward_validated_with_scratch_v1(encoded, counts, scratch)
    }

    pub(crate) fn forward_feature_transfer_v4_with_scratch_v1<'a>(
        &self,
        encoded: NativeEncodedDecisionViewV1<'_>,
        scratch: &'a mut NativePolicyValueForwardScratchV1,
    ) -> Result<NativePolicyValueOutputViewV1<'a>, NativePolicyValueErrorV1> {
        let counts = encoded.validate(self.feature_transfer_config_v4())?;
        self.forward_validated_with_scratch_v1(encoded, counts, scratch)
    }

    /// Same row and reduction order as `forward_validated_rows_v1`, with
    /// ordinary libm activations and no public/stack projection extension.
    /// On error scratch contents are unspecified, but the next call rebuilds
    /// every consumed element. No partially validated output is returned.
    fn forward_validated_with_scratch_v1<'a>(
        &self,
        encoded: NativeEncodedDecisionViewV1<'_>,
        counts: ValidatedCountsV1,
        scratch: &'a mut NativePolicyValueForwardScratchV1,
    ) -> Result<NativePolicyValueOutputViewV1<'a>, NativePolicyValueErrorV1> {
        let mode = ForwardActivationModeV1::LibmTanh;
        clear_for(
            &mut scratch.input,
            counts.object_count * OBJECT_ENCODER_INPUT_V1,
        );
        for object in 0..counts.object_count {
            let begin = object * OBJECT_FEATURE_DIM_V1;
            scratch
                .input
                .extend_from_slice(&encoded.object_features[begin..begin + OBJECT_FEATURE_DIM_V1]);
            let token = encoded.object_card_ids[object] as usize;
            let begin = token * CARD_EMBEDDING_DIM_V1;
            scratch
                .input
                .extend_from_slice(&self.card_embedding[begin..begin + CARD_EMBEDDING_DIM_V1]);
        }
        two_layer_into(
            &self.object_encoder,
            &scratch.input,
            counts.object_count,
            mode,
            &mut scratch.hidden,
            &mut scratch.object_base_hidden,
        );

        zero_for(
            &mut scratch.edge_pooled,
            counts.object_count * HIDDEN_DIM_V1,
        );
        if counts.edge_count > 0 {
            clear_for(
                &mut scratch.input,
                counts.edge_count * EDGE_ENCODER_INPUT_V1,
            );
            for edge in 0..counts.edge_count {
                let begin = edge * EDGE_FEATURE_DIM_V1;
                scratch
                    .input
                    .extend_from_slice(&encoded.edge_features[begin..begin + EDGE_FEATURE_DIM_V1]);
                let begin = encoded.edge_source_indices[edge] as usize * HIDDEN_DIM_V1;
                scratch
                    .input
                    .extend_from_slice(&scratch.object_base_hidden[begin..begin + HIDDEN_DIM_V1]);
                let begin = encoded.edge_target_indices[edge] as usize * HIDDEN_DIM_V1;
                scratch
                    .input
                    .extend_from_slice(&scratch.object_base_hidden[begin..begin + HIDDEN_DIM_V1]);
            }
            two_layer_into(
                &self.edge_encoder,
                &scratch.input,
                counts.edge_count,
                mode,
                &mut scratch.hidden,
                &mut scratch.intermediate,
            );
            // Preserve the two separate ordered reductions, including the
            // double contribution of self-edges.
            add_indexed_rows_v1(
                &mut scratch.edge_pooled,
                &scratch.intermediate,
                encoded.edge_source_indices,
            );
            add_indexed_rows_v1(
                &mut scratch.edge_pooled,
                &scratch.intermediate,
                encoded.edge_target_indices,
            );
        }

        clear_for(
            &mut scratch.input,
            counts.object_count * NODE_UPDATE_INPUT_V1,
        );
        for object in 0..counts.object_count {
            let begin = object * HIDDEN_DIM_V1;
            scratch
                .input
                .extend_from_slice(&scratch.object_base_hidden[begin..begin + HIDDEN_DIM_V1]);
            scratch
                .input
                .extend_from_slice(&scratch.edge_pooled[begin..begin + HIDDEN_DIM_V1]);
        }
        two_layer_into(
            &self.node_update,
            &scratch.input,
            counts.object_count,
            mode,
            &mut scratch.hidden,
            &mut scratch.object_hidden,
        );
        zero_for(&mut scratch.pooled_objects, POOLED_OBJECT_DIM_V1);
        add_indexed_rows_v1(
            &mut scratch.pooled_objects,
            &scratch.object_hidden,
            encoded.object_groups,
        );
        clear_for(&mut scratch.input, STATE_ENCODER_INPUT_V1);
        scratch.input.extend_from_slice(encoded.state);
        scratch.input.extend_from_slice(&scratch.pooled_objects);
        two_layer_into(
            &self.state_encoder,
            &scratch.input,
            1,
            mode,
            &mut scratch.hidden,
            &mut scratch.state_hidden,
        );

        zero_for(
            &mut scratch.action_ref_pooled,
            counts.action_count * HIDDEN_DIM_V1,
        );
        if counts.action_ref_count > 0 {
            clear_for(
                &mut scratch.input,
                counts.action_ref_count * ACTION_REF_ENCODER_INPUT_V1,
            );
            for action_ref in 0..counts.action_ref_count {
                let begin = action_ref * ACTION_REF_FEATURE_DIM_V1;
                scratch.input.extend_from_slice(
                    &encoded.action_ref_features[begin..begin + ACTION_REF_FEATURE_DIM_V1],
                );
                let begin = encoded.action_ref_node_indices[action_ref] as usize * HIDDEN_DIM_V1;
                scratch
                    .input
                    .extend_from_slice(&scratch.object_hidden[begin..begin + HIDDEN_DIM_V1]);
            }
            two_layer_into(
                &self.action_ref_encoder,
                &scratch.input,
                counts.action_ref_count,
                mode,
                &mut scratch.hidden,
                &mut scratch.intermediate,
            );
            add_indexed_rows_v1(
                &mut scratch.action_ref_pooled,
                &scratch.intermediate,
                encoded.action_ref_action_indices,
            );
        }

        clear_for(
            &mut scratch.input,
            counts.action_count * ACTION_ENCODER_INPUT_V1,
        );
        for action in 0..counts.action_count {
            let begin = action * ACTION_FEATURE_DIM_V1;
            scratch
                .input
                .extend_from_slice(&encoded.action_features[begin..begin + ACTION_FEATURE_DIM_V1]);
            let begin = action * HIDDEN_DIM_V1;
            scratch
                .input
                .extend_from_slice(&scratch.action_ref_pooled[begin..begin + HIDDEN_DIM_V1]);
        }
        two_layer_into(
            &self.action_encoder,
            &scratch.input,
            counts.action_count,
            mode,
            &mut scratch.hidden,
            &mut scratch.action_hidden,
        );
        clear_for(&mut scratch.state_prefix, self.scorer_first.output_dim);
        scratch
            .state_prefix
            .extend_from_slice(&self.scorer_first.bias);
        accumulate_linear_row_v1(
            &self.scorer_first,
            &scratch.state_hidden,
            0,
            &mut scratch.state_prefix,
        );
        clear_for(
            &mut scratch.scorer_hidden,
            counts.action_count * self.scorer_first.output_dim,
        );
        let state_dim = scratch.state_hidden.len();
        let action_dim = self.scorer_first.input_dim - state_dim;
        for action_row in scratch.action_hidden.chunks_exact(action_dim) {
            let begin = scratch.scorer_hidden.len();
            scratch
                .scorer_hidden
                .extend_from_slice(&scratch.state_prefix);
            accumulate_linear_row_v1(
                &self.scorer_first,
                action_row,
                state_dim,
                &mut scratch.scorer_hidden[begin..],
            );
        }
        tanh_in_place_v1(&mut scratch.scorer_hidden, mode);
        linear_into(
            &self.scorer_second,
            &scratch.scorer_hidden,
            counts.action_count,
            &mut scratch.logits,
        );
        linear_into(
            &self.value_first,
            &scratch.state_hidden,
            1,
            &mut scratch.hidden,
        );
        tanh_in_place_v1(&mut scratch.hidden, mode);
        linear_into(
            &self.value_second,
            &scratch.hidden,
            1,
            &mut scratch.intermediate,
        );
        let value = scratch.intermediate[0];

        for (position, output) in scratch.logits.iter().copied().enumerate() {
            if !output.is_finite() {
                return Err(NativePolicyValueErrorV1::NonFiniteOutput {
                    field: "logits",
                    position,
                });
            }
        }
        if !value.is_finite() {
            return Err(NativePolicyValueErrorV1::NonFiniteOutput {
                field: "value",
                position: 0,
            });
        }
        Ok(NativePolicyValueOutputViewV1 {
            logits: &scratch.logits,
            value,
        })
    }
}

fn clear_for(buffer: &mut Vec<f32>, len: usize) {
    buffer.clear();
    buffer.reserve(len);
}

fn zero_for(buffer: &mut Vec<f32>, len: usize) {
    buffer.resize(len, 0.0);
    buffer.fill(0.0);
}

fn linear_into(linear: &LinearV1, input: &[f32], rows: usize, output: &mut Vec<f32>) {
    debug_assert_eq!(input.len(), rows * linear.input_dim);
    debug_assert_eq!(linear.weight_t.len(), linear.weight.len());
    clear_for(output, rows * linear.output_dim);
    for input_row in input.chunks_exact(linear.input_dim).take(rows) {
        let begin = output.len();
        output.extend_from_slice(&linear.bias);
        accumulate_linear_row_v1(linear, input_row, 0, &mut output[begin..]);
    }
}

fn two_layer_into(
    encoder: &TwoLayerTanhV1,
    input: &[f32],
    rows: usize,
    mode: ForwardActivationModeV1,
    hidden: &mut Vec<f32>,
    output: &mut Vec<f32>,
) {
    linear_into(&encoder.first, input, rows, hidden);
    tanh_in_place_v1(hidden, mode);
    linear_into(&encoder.second, hidden, rows, output);
    tanh_in_place_v1(output, mode);
}
