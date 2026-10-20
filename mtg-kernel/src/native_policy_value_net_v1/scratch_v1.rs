//! Caller-owned intermediate storage for the Net8 inference paths.
//!
//! No model-derived values are reused across calls. Every active row is rebuilt,
//! so a scratch instance can be reused after a shape change, failed call or
//! parameter replacement. The owned-output entry points remain the reference.

use super::*;
use std::collections::HashMap;

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
        activation_mode: ForwardActivationModeV1,
    ) -> Result<NativePolicyValueOutputViewV1<'a>, NativePolicyValueErrorV1> {
        let counts = encoded.validate(self.config)?;
        self.forward_validated_with_scratch_v1(encoded, counts, scratch, activation_mode, None)
    }

    pub(crate) fn forward_feature_transfer_v3_with_scratch_v1<'a>(
        &self,
        encoded: NativeEncodedDecisionViewV1<'_>,
        scratch: &'a mut NativePolicyValueForwardScratchV1,
        activation_mode: ForwardActivationModeV1,
    ) -> Result<NativePolicyValueOutputViewV1<'a>, NativePolicyValueErrorV1> {
        let counts = encoded.validate(self.feature_transfer_config_v3())?;
        self.forward_validated_with_scratch_v1(encoded, counts, scratch, activation_mode, None)
    }

    pub(crate) fn forward_feature_transfer_v4_with_scratch_v1<'a>(
        &self,
        encoded: NativeEncodedDecisionViewV1<'_>,
        scratch: &'a mut NativePolicyValueForwardScratchV1,
        activation_mode: ForwardActivationModeV1,
    ) -> Result<NativePolicyValueOutputViewV1<'a>, NativePolicyValueErrorV1> {
        let counts = encoded.validate(self.feature_transfer_config_v4())?;
        self.forward_validated_with_scratch_v1(encoded, counts, scratch, activation_mode, None)
    }

    /// `forward_with_scratch_v1` (flat, V3 or V4 transfer by `config`) using
    /// `rows` for the five row layers: the same output, bit for bit.
    pub(crate) fn forward_rows_cached_with_scratch_v1<'a>(
        &self,
        encoded: NativeEncodedDecisionViewV1<'_>,
        config: NativePolicyValueModelConfigV1,
        scratch: &'a mut NativePolicyValueForwardScratchV1,
        rows: &mut ForwardRowCacheV1,
        activation_mode: ForwardActivationModeV1,
    ) -> Result<NativePolicyValueOutputViewV1<'a>, NativePolicyValueErrorV1> {
        let counts = encoded.validate(config)?;
        if row_cache_check_v1() {
            let mut reference = NativePolicyValueForwardScratchV1::default();
            let expected = self
                .forward_validated_with_scratch_v1(
                    encoded,
                    counts,
                    &mut reference,
                    activation_mode,
                    None,
                )
                .map(|o| {
                    (
                        o.logits.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                        o.value.to_bits(),
                    )
                });
            let actual = self
                .forward_validated_with_scratch_v1(
                    encoded,
                    counts,
                    scratch,
                    activation_mode,
                    Some(rows),
                )
                .map(|o| {
                    (
                        o.logits.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                        o.value.to_bits(),
                    )
                });
            assert_eq!(
                expected.as_ref().ok(),
                actual.as_ref().ok(),
                "row-cached forward differs from the uncached forward"
            );
        }
        self.forward_validated_with_scratch_v1(
            encoded,
            counts,
            scratch,
            activation_mode,
            Some(rows),
        )
    }

    /// Same row and reduction order as `forward_validated_rows_v1`, with
    /// the declared activations and no public/stack projection extension.
    /// On error scratch contents are unspecified, but the next call rebuilds
    /// every consumed element. No partially validated output is returned.
    fn forward_validated_with_scratch_v1<'a>(
        &self,
        encoded: NativeEncodedDecisionViewV1<'_>,
        counts: ValidatedCountsV1,
        scratch: &'a mut NativePolicyValueForwardScratchV1,
        mode: ForwardActivationModeV1,
        mut rows: Option<&mut ForwardRowCacheV1>,
    ) -> Result<NativePolicyValueOutputViewV1<'a>, NativePolicyValueErrorV1> {
        if let Some(rows) = rows.as_deref_mut() {
            rows.bind_mode(mode);
        }
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
        two_layer_cached_v1(
            &self.object_encoder,
            &scratch.input,
            counts.object_count,
            mode,
            &mut scratch.hidden,
            &mut scratch.object_base_hidden,
            rows.as_deref_mut(),
            0,
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
            two_layer_cached_v1(
                &self.edge_encoder,
                &scratch.input,
                counts.edge_count,
                mode,
                &mut scratch.hidden,
                &mut scratch.intermediate,
                rows.as_deref_mut(),
                1,
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
        two_layer_cached_v1(
            &self.node_update,
            &scratch.input,
            counts.object_count,
            mode,
            &mut scratch.hidden,
            &mut scratch.object_hidden,
            rows.as_deref_mut(),
            2,
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
            two_layer_cached_v1(
                &self.action_ref_encoder,
                &scratch.input,
                counts.action_ref_count,
                mode,
                &mut scratch.hidden,
                &mut scratch.intermediate,
                rows.as_deref_mut(),
                3,
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
        two_layer_cached_v1(
            &self.action_encoder,
            &scratch.input,
            counts.action_count,
            mode,
            &mut scratch.hidden,
            &mut scratch.action_hidden,
            rows,
            4,
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

/// Rows each row layer's cache holds (first in, first out).
const ROW_CACHE_ROWS_V1: usize = 2048;

/// Exact per-row output cache for one model's five row layers (object
/// encoder, edge encoder, node update, action-reference encoder, action
/// encoder). Each of those layers maps every input row to its output row on
/// its own (a linear map per row, then an elementwise tanh), so an output
/// stored for an input row with the same bits is exactly the output computing
/// it again would give. A hit compares the whole stored row, bit for bit.
///
/// The owner must keep one cache per model and clear it whenever that
/// model's parameters change; the activation mode is recorded and a change
/// clears the cache. Only `FrozenPlayPolicyV1` uses it.
#[derive(Default)]
pub(crate) struct ForwardRowCacheV1 {
    /// The activation mode and arithmetic environment the stored rows were
    /// computed in.
    environment: Option<(ForwardActivationModeV1, u64)>,
    layers: [RowLayerCacheV1; 5],
    gather: Vec<f32>,
    miss_rows: Vec<usize>,
    miss_output: Vec<f32>,
}

impl ForwardRowCacheV1 {
    /// Clears the cache unless its rows were computed in the same mode and
    /// arithmetic environment: the MXCSR control field (flush-to-zero,
    /// denormals-are-zero, rounding, exception masks) and the platform
    /// `tanhf`'s state (UCRT's FMA3 switch), which every layer's arithmetic
    /// depends on beyond its inputs and weights.
    fn bind_mode(&mut self, mode: ForwardActivationModeV1) {
        #[cfg(target_arch = "x86_64")]
        let control = u64::from(crate::deterministic_math_v1::read_mxcsr_v1() & 0xffc0);
        #[cfg(not(target_arch = "x86_64"))]
        let control = 0u64;
        let environment = (
            mode,
            control | (u64::from(crate::vector_tanh_v1::libm_environment_token_v1()) << 32),
        );
        if self.environment != Some(environment) {
            self.layers = Default::default();
            self.environment = Some(environment);
        }
    }

    /// Rows served and rows computed so far, per layer (diagnostics).
    #[cfg(test)]
    pub(crate) fn counts_v1(&self) -> [(u64, u64); 5] {
        self.layers
            .each_ref()
            .map(|layer| (layer.hits, layer.misses))
    }
}

#[derive(Default)]
struct RowLayerCacheV1 {
    width: usize,
    out: usize,
    index: HashMap<u64, u32>,
    keys: Vec<u32>,
    outputs: Vec<f32>,
    hashes: Vec<u64>,
    len: usize,
    next: usize,
    #[cfg(test)]
    hits: u64,
    #[cfg(test)]
    misses: u64,
}

fn row_hash_v1(row: &[f32]) -> u64 {
    let mut hash = 0x9e37_79b9_7f4a_7c15_u64 ^ row.len() as u64;
    for value in row {
        hash =
            (hash.rotate_left(5) ^ u64::from(value.to_bits())).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
    hash ^ (hash >> 31)
}

impl RowLayerCacheV1 {
    fn shape(&mut self, width: usize, out: usize) {
        if self.width != width || self.out != out {
            *self = Self {
                width,
                out,
                ..Self::default()
            };
        }
    }

    fn get(&self, hash: u64, row: &[f32]) -> Option<&[f32]> {
        let slot = *self.index.get(&hash)? as usize;
        let key = &self.keys[slot * self.width..(slot + 1) * self.width];
        key.iter()
            .zip(row)
            .all(|(&stored, value)| stored == value.to_bits())
            .then(|| &self.outputs[slot * self.out..(slot + 1) * self.out])
    }

    fn insert(&mut self, hash: u64, row: &[f32], output: &[f32]) {
        if self.get(hash, row).is_some() {
            return;
        }
        let slot = self.next;
        self.next = (self.next + 1) % ROW_CACHE_ROWS_V1;
        if slot < self.len {
            let old = self.hashes[slot];
            if self.index.get(&old) == Some(&(slot as u32)) {
                self.index.remove(&old);
            }
        } else {
            self.len += 1;
            self.keys.resize(self.len * self.width, 0);
            self.outputs.resize(self.len * self.out, 0.0);
            self.hashes.push(0);
        }
        for (stored, value) in self.keys[slot * self.width..(slot + 1) * self.width]
            .iter_mut()
            .zip(row)
        {
            *stored = value.to_bits();
        }
        self.outputs[slot * self.out..(slot + 1) * self.out].copy_from_slice(output);
        self.hashes[slot] = hash;
        self.index.insert(hash, slot as u32);
    }
}

/// `two_layer_into`, serving rows from `cache` (layer `layer`) when given:
/// stored rows are copied, the others go through `two_layer_into` together
/// and are stored. Rows are independent, so the output is the same.
#[allow(clippy::too_many_arguments, reason = "the layer call plus its cache")]
fn two_layer_cached_v1(
    encoder: &TwoLayerTanhV1,
    input: &[f32],
    rows: usize,
    mode: ForwardActivationModeV1,
    hidden: &mut Vec<f32>,
    output: &mut Vec<f32>,
    cache: Option<&mut ForwardRowCacheV1>,
    layer: usize,
) {
    let Some(cache) = cache else {
        two_layer_into(encoder, input, rows, mode, hidden, output);
        return;
    };
    let width = encoder.first.input_dim;
    let out = encoder.second.output_dim;
    let ForwardRowCacheV1 {
        layers,
        gather,
        miss_rows,
        miss_output,
        ..
    } = cache;
    let layer = &mut layers[layer];
    layer.shape(width, out);
    output.clear();
    output.resize(rows * out, 0.0);
    gather.clear();
    miss_rows.clear();
    for (r, row) in input.chunks_exact(width).take(rows).enumerate() {
        match layer.get(row_hash_v1(row), row) {
            Some(stored) => {
                output[r * out..(r + 1) * out].copy_from_slice(stored);
                #[cfg(test)]
                {
                    layer.hits += 1;
                }
            }
            None => {
                miss_rows.push(r);
                gather.extend_from_slice(row);
            }
        }
    }
    if miss_rows.is_empty() {
        return;
    }
    #[cfg(test)]
    {
        layer.misses += miss_rows.len() as u64;
    }
    two_layer_into(encoder, gather, miss_rows.len(), mode, hidden, miss_output);
    for (m, &r) in miss_rows.iter().enumerate() {
        let computed = &miss_output[m * out..(m + 1) * out];
        output[r * out..(r + 1) * out].copy_from_slice(computed);
        let row = &input[r * width..(r + 1) * width];
        layer.insert(row_hash_v1(row), row, computed);
    }
}

/// Whether `MTG_KERNEL_ROW_CACHE_CHECK=1` asks every cached forward to be
/// compared with an uncached one.
fn row_cache_check_v1() -> bool {
    static CHECK: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CHECK.get_or_init(|| std::env::var("MTG_KERNEL_ROW_CACHE_CHECK").is_ok_and(|v| v == "1"))
}
