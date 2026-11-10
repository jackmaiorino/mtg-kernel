//! Isolated ordinary-S GAE tape reuse prototype. Production preparation and
//! update entry points do not use this module.

use super::{
    ensure_mxcsr_v1, gae_policy_row_v1, update_profile_v1, CountsV1, LayerTapeV1, SBackScratchV1,
    SConfigV1, SGradientSinkV1, SModelV1, SSubstepV1, STapeV1, OBJECT_FEATURE_DIM_V1,
    REF_FEATURE_DIM_V1,
};
use std::mem::size_of;

const CAP_ERROR: &str = "S verified tape exceeds its retained byte cap";

thread_local! {
    static TEST_FORWARDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(super) fn note_forward_v1() {
    TEST_FORWARDS.with(|n| n.set(n.get() + 1));
}

/// Bind once per update, including model configuration and all parameter
/// bits. Equal outputs on one row cannot identify a model generation.
pub(super) struct ModelScopeV1<'model>(&'model SModelV1);

impl<'model> ModelScopeV1<'model> {
    pub(super) fn bind_v1(model: &'model SModelV1, expected: [u8; 32]) -> Result<Self, String> {
        if model.cache_namespace_v1() != expected {
            return Err("S verified model generation differs".into());
        }
        Ok(Self(model))
    }
}

/// One slot for a tape and its reserved buffers. The cap counts the inline
/// slot, reserved layer headers, and every retained f32 capacity. Inputs,
/// backward scratch, gradient terms and allocator overhead are separate.
pub(super) struct TapeSlotV1 {
    tape: STapeV1,
    cap: usize,
}

impl TapeSlotV1 {
    pub(super) fn new(cap: usize) -> Self {
        Self {
            tape: STapeV1::default(),
            cap,
        }
    }

    fn bytes_v1(&mut self, config: &SConfigV1, counts: CountsV1, projected: bool) -> Option<usize> {
        let layers = self
            .tape
            .layers
            .capacity()
            .max(if projected { config.layers } else { 0 });
        let mut bytes = layers
            .checked_mul(size_of::<LayerTapeV1>())?
            .checked_add(size_of::<Self>());
        let mut count = |buffer: &mut Vec<f32>, needed: usize| {
            let capacity = buffer.capacity().max(if projected { needed } else { 0 });
            bytes =
                bytes.and_then(|total| total.checked_add(capacity.checked_mul(size_of::<f32>())?));
        };
        self.tape.size_prefix_v1(config, counts, &mut count);
        if projected {
            for index in 0..config.layers {
                if let Some(layer) = self.tape.layers.get_mut(index) {
                    layer.size_v1(config, counts, &mut count);
                } else {
                    LayerTapeV1::default().size_v1(config, counts, &mut count);
                }
            }
        } else {
            for layer in &mut self.tape.layers {
                layer.size_v1(config, counts, &mut count);
            }
        }
        self.tape.size_tail_v1(config, counts, &mut count);
        bytes
    }

    fn reserve_v1(&mut self, config: &SConfigV1, counts: CountsV1) -> Result<(), String> {
        // Guard the shared sizing visitor's products before evaluating any
        // shape, even for an unrealistically large but structurally valid row.
        let rows = counts
            .objects
            .checked_add(counts.actions)
            .and_then(|n| n.checked_add(1))
            .ok_or(CAP_ERROR)?;
        let width = config
            .ffn
            .max(3 * config.d_model)
            .max(2 * config.heads)
            .max(OBJECT_FEATURE_DIM_V1 + config.card_dim)
            .max(REF_FEATURE_DIM_V1 + config.card_dim);
        config
            .heads
            .checked_mul(rows)
            .and_then(|n| n.checked_mul(rows))
            .ok_or(CAP_ERROR)?;
        rows.max(counts.edges)
            .max(counts.refs)
            .checked_mul(width)
            .ok_or(CAP_ERROR)?;

        if self.bytes_v1(config, counts, false).ok_or(CAP_ERROR)? > self.cap
            || self.bytes_v1(config, counts, true).ok_or(CAP_ERROR)? > self.cap
        {
            return Err(CAP_ERROR.into());
        }
        let reservation = (|| {
            let layers = &mut self.tape.layers;
            if config.layers > layers.capacity() {
                layers.try_reserve_exact(config.layers - layers.len())?;
            }
            layers.resize_with(config.layers, LayerTapeV1::default);
            let mut result = Ok(());
            let mut reserve = |buffer: &mut Vec<f32>, needed: usize| {
                if result.is_ok() && needed > buffer.capacity() {
                    result = buffer.try_reserve_exact(needed - buffer.len());
                }
            };
            self.tape.size_prefix_v1(config, counts, &mut reserve);
            for layer in &mut self.tape.layers {
                layer.size_v1(config, counts, &mut reserve);
            }
            self.tape.size_tail_v1(config, counts, &mut reserve);
            result
        })();
        // The allocator may grant extra capacity. Measure the actual slot
        // before forward, including after a failed reservation.
        if self
            .bytes_v1(config, counts, false)
            .is_none_or(|bytes| bytes > self.cap)
        {
            self.tape = STapeV1::default();
            return Err(CAP_ERROR.into());
        }
        reservation.map_err(|error| format!("S verified tape reservation failed: {error}"))
    }

    pub(super) fn authenticate_v1<'model, 'input, 'slot>(
        &'slot mut self,
        generation: &ModelScopeV1<'model>,
        step: SSubstepV1<'input>,
        index: (usize, usize),
        expected_logits: &[u32],
        expected_value: u32,
    ) -> Result<VerifiedRowV1<'model, 'input, 'slot>, String> {
        ensure_mxcsr_v1()?;
        let model = generation.0;
        let config = model.layout.config;
        let counts = step.input.validate_v1(&config)?;
        self.reserve_v1(&config, counts)?;
        let forward = update_profile_v1::span_v1(update_profile_v1::StageV1::ReplayLearnerRow);
        model.forward_validated_into_v1(step.input, &mut self.tape, counts)?;
        forward.finish_v1(self.tape.logits_v1(), self.tape.value_v1());
        if self.tape.logits_v1().len() != expected_logits.len()
            || !self
                .tape
                .logits_v1()
                .iter()
                .zip(expected_logits)
                .all(|(actual, expected)| actual.to_bits() == *expected)
            || self.tape.value_v1().to_bits() != expected_value
        {
            return Err("stored tensor does not reproduce rollout outputs".into());
        }
        Ok(VerifiedRowV1 {
            model,
            step,
            index,
            tape: &self.tape,
        })
    }
}

/// The same immutable model, inputs, row ordinal, and uniquely borrowed
/// tape stay bound until the token is consumed or dropped.
pub(super) struct VerifiedRowV1<'model, 'input, 'slot> {
    model: &'model SModelV1,
    step: SSubstepV1<'input>,
    index: (usize, usize),
    tape: &'slot STapeV1,
}

pub(super) struct GaeTargetV1 {
    pub(super) group_count: usize,
    pub(super) advantage: f32,
    pub(super) value_target: f32,
    pub(super) value_coefficient: f32,
}

impl VerifiedRowV1<'_, '_, '_> {
    /// Adds one ordinary GAE row to a private update sink. The caller
    /// retains original group/reduction order and owns the final gauge
    /// projection and all-or-nothing update.
    pub(super) fn backward_v1(
        self,
        target: GaeTargetV1,
        sink: &mut SGradientSinkV1<'_>,
        scratch: &mut SBackScratchV1,
    ) -> Result<(f32, f32), String> {
        ensure_mxcsr_v1()?;
        let (group, sub) = self.index;
        if group >= target.group_count
            || !target.advantage.is_finite()
            || !target.value_target.is_finite()
            || !target.value_coefficient.is_finite()
            || target.value_coefficient <= 0.0
        {
            return Err("invalid S verified GAE target".into());
        }
        let group_count = target.group_count as f32;
        let (selected, dlogits) = gae_policy_row_v1(
            self.tape.logits_v1(),
            self.step.selected,
            -target.advantage / group_count,
            group,
            sub,
        )?;
        let (value_error, dvalue) = if sub == 0 {
            let error = self.tape.value_v1() - target.value_target;
            (
                error,
                (target.value_coefficient / group_count) * (2.0 * error),
            )
        } else {
            (0.0, 0.0)
        };
        let backward = update_profile_v1::span_v1(update_profile_v1::StageV1::LearnerBackward);
        self.model
            .backward_into_v1(self.step.input, self.tape, &dlogits, dvalue, sink, scratch)?;
        backward.finish_plain_v1();
        Ok((selected, value_error))
    }
}

mod tests;
