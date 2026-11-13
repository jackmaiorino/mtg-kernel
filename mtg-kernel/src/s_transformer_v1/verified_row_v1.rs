//! Bounded ordinary S GAE tape reuse for the opt-in serial and prepared
//! updates.

use super::parallel::PipelineOptionsV1;
use super::{
    ensure_mxcsr_v1, gae_policy_row_v1, update_profile_v1, CountsV1, LayerTapeV1, SBackScratchV1,
    SConfigV1, SGradientSinkV1, SGroupV1, SInputV1, SLossV1, SModelV1, SSubstepV1, STapeV1,
    SWorkV1, OBJECT_FEATURE_DIM_V1, REF_FEATURE_DIM_V1,
};
use std::mem::size_of;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

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

#[cfg(test)]
thread_local! {
    static TEST_FORWARDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
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

/// The bytes `tape` retains in a slot whose own inline size is `inline`:
/// that size, its reserved layer headers and every retained f32 capacity.
/// `projected`: what it retains once reserved for `counts`.
fn tape_bytes_v1(
    tape: &mut STapeV1,
    inline: usize,
    config: &SConfigV1,
    counts: CountsV1,
    projected: bool,
) -> Option<usize> {
    let layers = tape
        .layers
        .capacity()
        .max(if projected { config.layers } else { 0 });
    let mut bytes = layers
        .checked_mul(size_of::<LayerTapeV1>())?
        .checked_add(inline);
    let mut count = |buffer: &mut Vec<f32>, needed: usize| {
        let capacity = buffer.capacity().max(if projected { needed } else { 0 });
        bytes = bytes.and_then(|total| total.checked_add(capacity.checked_mul(size_of::<f32>())?));
    };
    tape.size_prefix_v1(config, counts, &mut count);
    if projected {
        for index in 0..config.layers {
            if let Some(layer) = tape.layers.get_mut(index) {
                layer.size_v1(config, counts, &mut count);
            } else {
                LayerTapeV1::default().size_v1(config, counts, &mut count);
            }
        }
    } else {
        for layer in &mut tape.layers {
            layer.size_v1(config, counts, &mut count);
        }
    }
    tape.size_tail_v1(config, counts, &mut count);
    bytes
}

/// Reserves `tape` (in a slot of `inline` bytes) for one forward of
/// `counts` within `cap`, and returns the bytes it then retains. A refusal
/// leaves the slot within the cap.
fn reserve_tape_v1(
    tape: &mut STapeV1,
    inline: usize,
    cap: usize,
    config: &SConfigV1,
    counts: CountsV1,
) -> Result<usize, ReplayErrorV1> {
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

    if tape_bytes_v1(tape, inline, config, counts, true).is_none_or(|bytes| bytes > cap) {
        return Err(ReplayErrorV1::Capacity);
    }
    let reservation = (|| {
        let layers = &mut tape.layers;
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
        tape.size_prefix_v1(config, counts, &mut reserve);
        for layer in &mut tape.layers {
            layer.size_v1(config, counts, &mut reserve);
        }
        tape.size_tail_v1(config, counts, &mut reserve);
        result
    })();
    // The allocator may grant extra capacity. Measure the actual slot
    // before forward, including after a failed reservation.
    let Some(bytes) =
        tape_bytes_v1(tape, inline, config, counts, false).filter(|bytes| *bytes <= cap)
    else {
        *tape = STapeV1::default();
        return Err(ReplayErrorV1::Capacity);
    };
    reservation.map_err(|error| format!("S verified tape reservation failed: {error}"))?;
    Ok(bytes)
}

/// [`reserve_tape_v1`], and after a capacity refusal once more from an
/// empty tape: a row refuses only if it alone exceeds the cap.
fn reserve_recycling_v1(
    tape: &mut STapeV1,
    inline: usize,
    cap: usize,
    config: &SConfigV1,
    counts: CountsV1,
) -> Result<usize, ReplayErrorV1> {
    match reserve_tape_v1(tape, inline, cap, config, counts) {
        Err(ReplayErrorV1::Capacity) => {
            *tape = STapeV1::default();
            reserve_tape_v1(tape, inline, cap, config, counts)
        }
        result => result,
    }
}

impl TapeSlotV1 {
    pub(super) fn new(cap: usize) -> Self {
        Self {
            tape: STapeV1::default(),
            cap,
        }
    }

    #[cfg(test)]
    fn bytes_v1(&mut self, config: &SConfigV1, counts: CountsV1, projected: bool) -> Option<usize> {
        tape_bytes_v1(&mut self.tape, size_of::<Self>(), config, counts, projected)
    }

    fn reserve_v1(&mut self, config: &SConfigV1, counts: CountsV1) -> Result<usize, ReplayErrorV1> {
        reserve_tape_v1(&mut self.tape, size_of::<Self>(), self.cap, config, counts)
    }

    #[cfg(test)]
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
        let bytes = if recycle {
            reserve_recycling_v1(&mut self.tape, size_of::<Self>(), self.cap, &config, counts)?
        } else {
            self.reserve_v1(&config, counts)?
        };
        let forward = update_profile_v1::span_v1(update_profile_v1::StageV1::ReplayLearnerRow);
        model.forward_validated_into_v1(step.input, &mut self.tape, counts, true)?;
        forward.finish_v1(self.tape.logits_v1(), self.tape.value_v1());
        reproduces_v1(
            &self.tape,
            StoredRowV1 {
                logits: expected_logits,
                value: expected_value,
            },
        )?;
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
    peak_retained_tape_bytes: usize,
    #[cfg(test)]
    short_first_gradient: bool,
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
            peak_retained_tape_bytes: size_of::<TapeSlotV1>(),
            #[cfg(test)]
            short_first_gradient: false,
        })
    }

    pub(crate) fn peak_retained_tape_bytes_v1(&self) -> usize {
        self.peak_retained_tape_bytes
    }

    #[cfg(test)]
    pub(crate) fn shorten_first_gradient_for_test_v1(&mut self) {
        self.short_first_gradient = true;
    }

    pub(crate) fn authenticate_v1(
        &mut self,
        step: SSubstepV1<'_>,
        index: (usize, usize),
        last: bool,
        expected_logits: &[u32],
        expected_value: u32,
        target: impl FnOnce() -> Option<(f32, f32)>,
    ) -> Result<bool, ReplayErrorV1> {
        let row = self.tape.authenticate_recycling_v1(
            &self.generation,
            step,
            index,
            expected_logits,
            expected_value,
            true,
        )?;
        self.peak_retained_tape_bytes = self.peak_retained_tape_bytes.max(row.bytes);
        if self.first_error.is_some() {
            return Ok(false);
        }
        let Some((advantage, value_target)) = target() else {
            return Ok(false);
        };
        let (group, sub) = index;
        if !advantage.is_finite() || !value_target.is_finite() {
            self.first_error = Some(format!("S group {group} has a non-finite target"));
            return Ok(false);
        }
        // A short sink exercises the real backward rejection without changing
        // replayed inputs or provisional targets.
        let gradient = self.gradient.as_mut_slice();
        #[cfg(test)]
        let gradient = if self.short_first_gradient && index == (0, 0) {
            &mut gradient[..0]
        } else {
            gradient
        };
        let result = row.backward_v1(
            GaeTargetV1 {
                group_count: self.group_count,
                advantage,
                value_target,
                value_coefficient: self.coefficient,
            },
            &mut SGradientSinkV1::Direct(gradient),
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
                Ok(true)
            }
            Err(error) => {
                self.first_error = Some(error);
                Ok(false)
            }
        }
    }

    pub(crate) fn finish_v1(mut self) -> Result<(SLossV1, Vec<f32>), String> {
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
        ))
    }
}

/// The rollout's outputs of one stored row, as bits.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StoredRowV1<'a> {
    pub(crate) logits: &'a [u32],
    pub(crate) value: u32,
}

/// Whether `tape`'s forward reproduced `stored`, bit for bit.
fn reproduces_v1(tape: &STapeV1, stored: StoredRowV1<'_>) -> Result<(), String> {
    if tape.logits_v1().len() != stored.logits.len()
        || !tape
            .logits_v1()
            .iter()
            .zip(stored.logits)
            .all(|(actual, expected)| actual.to_bits() == *expected)
        || tape.value_v1().to_bits() != stored.value
    {
        return Err("stored tensor does not reproduce rollout outputs".into());
    }
    Ok(())
}

/// A parallel learner's refusal of a row that alone exceeds a worker's tape
/// cap. The update reports it as [`ReplayErrorV1::Capacity`].
pub(super) const VERIFIED_TAPE_CAPACITY_V1: &str = "S verified tape exceeds its cap";

/// What the parallel learner checks of every substep it records
/// (`parallel::PipelineV1`): its stored outputs, by flat substep
/// (`group_starts[group] + sub`), and the tape cap of each worker. Each
/// worker keeps its own tape within the cap, so all tapes together retain at
/// most `workers * tape_cap` bytes, beside the term plan's budget.
pub(super) struct VerifiedJobsV1<'a> {
    stored: &'a [StoredRowV1<'a>],
    group_starts: Vec<usize>,
    tape_cap: usize,
    forwards: AtomicU64,
    backwards: AtomicU64,
    retained: AtomicUsize,
}

/// Whether a parallel worker's tape cap holds at least its empty tape.
pub(crate) fn check_parallel_tape_cap_v1(tape_cap: usize) -> Result<(), ReplayErrorV1> {
    if tape_cap < size_of::<STapeV1>() {
        return Err(ReplayErrorV1::Capacity);
    }
    Ok(())
}

impl<'a> VerifiedJobsV1<'a> {
    /// `stored` holds one row per substep of `groups`, in order.
    pub(super) fn new_v1(
        groups: &[SGroupV1<'_>],
        stored: &'a [StoredRowV1<'a>],
        tape_cap: usize,
    ) -> Result<Self, ReplayErrorV1> {
        check_parallel_tape_cap_v1(tape_cap)?;
        let mut group_starts = Vec::with_capacity(groups.len());
        let mut substeps = 0usize;
        for group in groups {
            group_starts.push(substeps);
            substeps += group.substeps.len();
        }
        if substeps != stored.len() {
            return Err("S verified groups and stored rows differ".to_owned().into());
        }
        Ok(Self {
            stored,
            group_starts,
            tape_cap,
            forwards: AtomicU64::new(0),
            backwards: AtomicU64::new(0),
            retained: AtomicUsize::new(0),
        })
    }

    /// The forward of substep `sub` of group `group` into `tape`, reserved
    /// first within the cap (`peak` keeps the most it retained), and checked
    /// against the stored outputs; the same checks in the same order as the
    /// serial [`TapeSlotV1`].
    pub(super) fn forward_v1(
        &self,
        model: &SModelV1,
        (group, sub): (usize, usize),
        input: SInputV1<'_>,
        tape: &mut STapeV1,
        peak: &mut usize,
    ) -> Result<(), String> {
        ensure_mxcsr_v1()?;
        let stored = self
            .group_starts
            .get(group)
            .and_then(|start| self.stored.get(start + sub))
            .ok_or("S verified substep has no stored row")?;
        let config = model.layout.config;
        let counts = input.validate_v1(&config)?;
        let bytes =
            reserve_recycling_v1(tape, size_of::<STapeV1>(), self.tape_cap, &config, counts)
                .map_err(|error| match error {
                    ReplayErrorV1::Replay(error) => error,
                    _ => VERIFIED_TAPE_CAPACITY_V1.to_owned(),
                })?;
        *peak = (*peak).max(bytes);
        let forward = update_profile_v1::span_v1(update_profile_v1::StageV1::ReplayLearnerRow);
        model.forward_validated_into_v1(input, tape, counts, true)?;
        forward.finish_v1(tape.logits_v1(), tape.value_v1());
        self.forwards.fetch_add(1, Ordering::Relaxed);
        reproduces_v1(tape, *stored)
    }

    pub(super) fn backward_done_v1(&self) {
        self.backwards.fetch_add(1, Ordering::Relaxed);
    }

    /// Records the tapes of every worker slot an update made, by each
    /// slot's peak. A slot always retains its inline size.
    pub(super) fn slots_done_v1(&self, peaks: impl Iterator<Item = usize>) {
        let retained = peaks.map(|peak| peak.max(size_of::<STapeV1>())).sum();
        self.retained.store(retained, Ordering::Relaxed);
    }

    /// The work done so far.
    pub(super) fn usage_v1(&self) -> ParallelVerifiedUseV1 {
        ParallelVerifiedUseV1 {
            peak_retained_tape_bytes: self.retained.load(Ordering::Relaxed),
            learner_forwards: self.forwards.load(Ordering::Relaxed),
            learner_backwards: self.backwards.load(Ordering::Relaxed),
        }
    }
}

/// The work of one parallel verified gradient, also when it fails.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ParallelVerifiedUseV1 {
    /// The sum of every worker tape's peak before a forward, each within
    /// the cap: an upper bound of what they retained at once. As in the
    /// serial slot, this excludes inputs, backward scratch, gradient terms
    /// and allocator overhead.
    pub(crate) peak_retained_tape_bytes: usize,
    pub(crate) learner_forwards: u64,
    pub(crate) learner_backwards: u64,
}

/// The ordinary GAE loss and gradient of `groups` on `workers` workers,
/// the caller included, each substep's forward checked against its stored
/// row before its backward: the bits of `train_step_gae_with_workers_v1`'s
/// gradient. Only replay failures are [`ReplayErrorV1::Replay`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn parallel_gradient_v1(
    model: &SModelV1,
    expected_generation: [u8; 32],
    groups: &[SGroupV1<'_>],
    stored: &[StoredRowV1<'_>],
    value_coefficient: f32,
    workers: usize,
    tape_cap: usize,
    usage: &mut ParallelVerifiedUseV1,
) -> Result<(SLossV1, Vec<f32>), ReplayErrorV1> {
    crate::expanded_deck_training_v1::validate_preparation_workers_v1(workers)?;
    let generation = ModelScopeV1::bind_v1(model, expected_generation)?;
    let jobs = VerifiedJobsV1::new_v1(groups, stored, tape_cap)?;
    let mut gradient = vec![0.0f32; model.layout.total];
    let result = generation.0.gae_loss_gradient_verified_v1(
        groups,
        value_coefficient,
        &mut gradient,
        workers,
        &mut SWorkV1::default(),
        &jobs,
        PipelineOptionsV1::default(),
    );
    *usage = jobs.usage_v1();
    match result {
        Ok((loss, _)) => Ok((loss, gradient)),
        Err(error) if error == VERIFIED_TAPE_CAPACITY_V1 => Err(ReplayErrorV1::Capacity),
        Err(error) => Err(ReplayErrorV1::Replay(error)),
    }
}

#[cfg(test)]
mod tests;
