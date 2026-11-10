//! Isolated ordinary-S GAE tape reuse prototype. Production preparation and
//! update entry points do not use this module.

use super::{
    ensure_mxcsr_v1, gae_policy_row_v1, update_profile_v1, CountsV1, LayerTapeV1, SBackScratchV1,
    SConfigV1, SGradientSinkV1, SLossV1, SModelV1, SSubstepV1, STapeV1, OBJECT_FEATURE_DIM_V1,
    REF_FEATURE_DIM_V1,
};
use std::mem::size_of;

#[derive(Debug)]
pub(crate) enum ReplayErrorV1 {
    Capacity,
    CallerMathState,
    UnsupportedGeneration,
    Replay(String),
}

impl From<String> for ReplayErrorV1 {
    fn from(error: String) -> Self {
        Self::Replay(error)
    }
}

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

    fn reserve_v1(&mut self, config: &SConfigV1, counts: CountsV1) -> Result<usize, ReplayErrorV1> {
        // Guard the shared sizing visitor's products before evaluating any
        // shape, even for an unrealistically large but structurally valid row.
        let rows = counts
            .objects
            .checked_add(counts.actions)
            .and_then(|n| n.checked_add(1))
            .ok_or(ReplayErrorV1::Capacity)?;
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
            .ok_or(ReplayErrorV1::Capacity)?;
        rows.max(counts.edges)
            .max(counts.refs)
            .checked_mul(width)
            .ok_or(ReplayErrorV1::Capacity)?;

        if self
            .bytes_v1(config, counts, true)
            .is_none_or(|bytes| bytes > self.cap)
        {
            return Err(ReplayErrorV1::Capacity);
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
        let Some(bytes) = self
            .bytes_v1(config, counts, false)
            .filter(|bytes| *bytes <= self.cap)
        else {
            self.tape = STapeV1::default();
            return Err(ReplayErrorV1::Capacity);
        };
        reservation.map_err(|error| format!("S verified tape reservation failed: {error}"))?;
        Ok(bytes)
    }

    pub(super) fn authenticate_v1<'model, 'input, 'slot>(
        &'slot mut self,
        generation: &ModelScopeV1<'model>,
        step: SSubstepV1<'input>,
        index: (usize, usize),
        expected_logits: &[u32],
        expected_value: u32,
    ) -> Result<VerifiedRowV1<'model, 'input, 'slot>, ReplayErrorV1> {
        self.authenticate_recycling_v1(
            generation,
            step,
            index,
            expected_logits,
            expected_value,
            false,
        )
    }

    fn authenticate_recycling_v1<'model, 'input, 'slot>(
        &'slot mut self,
        generation: &ModelScopeV1<'model>,
        step: SSubstepV1<'input>,
        index: (usize, usize),
        expected_logits: &[u32],
        expected_value: u32,
        recycle: bool,
    ) -> Result<VerifiedRowV1<'model, 'input, 'slot>, ReplayErrorV1> {
        ensure_mxcsr_v1()?;
        let model = generation.0;
        let config = model.layout.config;
        let counts = step.input.validate_v1(&config)?;
        let bytes = match self.reserve_v1(&config, counts) {
            Err(ReplayErrorV1::Capacity) if recycle => {
                self.tape = STapeV1::default();
                self.reserve_v1(&config, counts)?
            }
            result => result?,
        };
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
            return Err("stored tensor does not reproduce rollout outputs"
                .to_owned()
                .into());
        }
        Ok(VerifiedRowV1 {
            model,
            step,
            index,
            tape: &self.tape,
            bytes,
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
    bytes: usize,
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

/// The serial candidate owns its gradient until the caller has authenticated
/// all actor rows. Only replay failures escape the per-row method.
pub(crate) struct SerialVerifiedGaeV1<'model> {
    generation: ModelScopeV1<'model>,
    tape: TapeSlotV1,
    scratch: SBackScratchV1,
    gradient: Vec<f32>,
    group_count: usize,
    coefficient: f32,
    joint: f32,
    value_error: f32,
    policy_sum: f32,
    value_sum: f32,
    first_error: Option<String>,
    peak_tape_bytes: usize,
}

impl<'model> SerialVerifiedGaeV1<'model> {
    pub(crate) fn new(
        model: &'model SModelV1,
        expected_generation: [u8; 32],
        cap: usize,
        group_count: usize,
        coefficient: f32,
    ) -> Result<Self, ReplayErrorV1> {
        let generation = ModelScopeV1::bind_v1(model, expected_generation)?;
        if cap < size_of::<TapeSlotV1>() {
            return Err(ReplayErrorV1::Capacity);
        }
        Ok(Self {
            generation,
            tape: TapeSlotV1::new(cap),
            scratch: SBackScratchV1::default(),
            gradient: vec![0.0; model.layout.total],
            group_count,
            coefficient,
            joint: 0.0,
            value_error: 0.0,
            policy_sum: 0.0,
            value_sum: 0.0,
            first_error: None,
            peak_tape_bytes: size_of::<TapeSlotV1>(),
        })
    }

    pub(crate) fn authenticate_v1(
        &mut self,
        step: SSubstepV1<'_>,
        index: (usize, usize),
        last: bool,
        expected_logits: &[u32],
        expected_value: u32,
        target: impl FnOnce() -> Option<(f32, f32)>,
    ) -> Result<(), ReplayErrorV1> {
        let row = self.tape.authenticate_recycling_v1(
            &self.generation,
            step,
            index,
            expected_logits,
            expected_value,
            true,
        )?;
        self.peak_tape_bytes = self.peak_tape_bytes.max(row.bytes);
        if self.first_error.is_some() {
            return Ok(());
        }
        let Some((advantage, value_target)) = target() else {
            return Ok(());
        };
        let (group, sub) = index;
        if !advantage.is_finite() || !value_target.is_finite() {
            self.first_error = Some(format!("S group {group} has a non-finite target"));
            return Ok(());
        }
        let result = row.backward_v1(
            GaeTargetV1 {
                group_count: self.group_count,
                advantage,
                value_target,
                value_coefficient: self.coefficient,
            },
            &mut SGradientSinkV1::Direct(&mut self.gradient),
            &mut self.scratch,
        );
        match result {
            Ok((selected, value_error)) => {
                if sub == 0 {
                    self.joint = selected;
                    self.value_error = value_error;
                } else {
                    self.joint += selected;
                }
                if last {
                    self.policy_sum += -self.joint * advantage;
                    self.value_sum += self.value_error * self.value_error;
                }
            }
            Err(error) => self.first_error = Some(error),
        }
        Ok(())
    }

    pub(crate) fn finish_v1(mut self) -> Result<(SLossV1, Vec<f32>, usize), String> {
        if let Some(error) = self.first_error {
            return Err(error);
        }
        for range in self.generation.0.layout.gauge_ranges_v1() {
            self.gradient[range].fill(0.0);
        }
        let count = self.group_count as f32;
        let loss = (self.policy_sum + self.coefficient * self.value_sum) / count;
        if !loss.is_finite() || self.gradient.iter().any(|value| !value.is_finite()) {
            return Err("S loss or gradient is not finite".into());
        }
        Ok((
            SLossV1 {
                group_count: self.group_count,
                policy_sum: self.policy_sum,
                value_sum: self.value_sum,
                policy_term: self.policy_sum / count,
                value_term: self.value_sum / count,
                loss,
            },
            self.gradient,
            self.peak_tape_bytes,
        ))
    }
}

mod tests;
