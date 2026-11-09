//! Measurement-only accounting of an S update's forward work (T3, #597):
//! how many forwards the behavior replay and the learner run, on which rows,
//! and their summed thread time. Compiled in only with the
//! `s-update-profile-v1` feature; default builds get zero-sized no-ops, so
//! uninstrumented timings come from a build without it.
//!
//! Attribution is scoped, not process-wide: [`arm_v1`] gives the calling
//! thread a fresh sink, and the update's two worker pools (the ordered
//! preparation executor and the S learner's `drain_v1`) hand the spawning
//! thread's sink to every worker they start ([`current_v1`], [`adopt_v1`]).
//! Concurrent tests or updates in one process never mix their counts, and a
//! thread that nobody armed records nothing.
//!
//! Every finished forward also adds a 64-bit hash of its output bits (logit
//! count, logits and value) to its stage's digest. The sums are
//! order-independent, so equal counts and digests for two stages mean the
//! two passes produced the same multiset of outputs: the learner recomputed
//! exactly the rows the replay had already authenticated.

/// The counted stages of one ordinary S update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StageV1 {
    /// Behavior replay of a learner-seat row: a row the learner trains on.
    ReplayLearnerRow = 0,
    /// Behavior replay of any other row (the opponent's seat, by net-8, the
    /// initial S, or S itself in self-play): authenticated, not trained on.
    ReplayOtherRow,
    /// The learner's forward of one substep.
    LearnerForward,
    /// The learner's backward of one substep.
    LearnerBackward,
}

pub(crate) const STAGES_V1: usize = 4;

/// Receipt names of the stages, in [`StageV1`] order.
pub(crate) const STAGE_NAMES_V1: [&str; STAGES_V1] = [
    "replay_learner_rows",
    "replay_other_rows",
    "learner_forward",
    "learner_backward",
];

/// One stage's totals: finished calls, their summed wall time on the threads
/// that ran them (not elapsed time; parallel workers add up), and the
/// order-independent digest of their outputs (zero for the backward).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StageTotalsV1 {
    pub(crate) calls: u64,
    pub(crate) thread_nanos: u64,
    pub(crate) output_digest: u64,
}

/// The output hash a finished forward adds to its stage's digest: FNV-1a
/// over the logit count and every output's bits, then the SplitMix64
/// finalizer so that sums of hashes stay well mixed.
#[allow(dead_code, reason = "only called with the feature and in tests")]
pub(crate) fn output_hash_v1(logits: &[f32], value: f32) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64 ^ logits.len() as u64;
    for bits in logits.iter().map(|x| x.to_bits()).chain([value.to_bits()]) {
        hash = (hash ^ u64::from(bits)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    let mut z = hash;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[cfg(feature = "s-update-profile-v1")]
mod imp {
    use super::{output_hash_v1, StageTotalsV1, StageV1, STAGES_V1};
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::Instant;

    #[derive(Default)]
    pub(crate) struct SinkV1 {
        calls: [AtomicU64; STAGES_V1],
        nanos: [AtomicU64; STAGES_V1],
        digests: [AtomicU64; STAGES_V1],
    }

    thread_local! {
        static CURRENT: RefCell<Option<Arc<SinkV1>>> = const { RefCell::new(None) };
    }

    /// The spawning thread's sink, for the workers it starts.
    #[derive(Clone)]
    pub(crate) struct HandleV1(pub(super) Option<Arc<SinkV1>>);

    pub(crate) fn current_v1() -> HandleV1 {
        HandleV1(CURRENT.with(|current| current.borrow().clone()))
    }

    /// Records this worker thread's stages into `handle`'s sink.
    pub(crate) fn adopt_v1(handle: &HandleV1) {
        CURRENT.with(|current| *current.borrow_mut() = handle.0.clone());
    }

    /// The calling thread's armed sink; dropping it restores the previous one.
    pub(crate) struct ArmedV1 {
        sink: Arc<SinkV1>,
        previous: Option<Arc<SinkV1>>,
    }

    pub(crate) fn arm_v1() -> ArmedV1 {
        let sink = Arc::new(SinkV1::default());
        let previous = CURRENT.with(|current| current.borrow_mut().replace(Arc::clone(&sink)));
        ArmedV1 { sink, previous }
    }

    impl ArmedV1 {
        /// Totals so far, in [`StageV1`] order.
        pub(crate) fn totals_v1(&self) -> Option<[StageTotalsV1; STAGES_V1]> {
            Some(std::array::from_fn(|stage| StageTotalsV1 {
                calls: self.sink.calls[stage].load(Ordering::Acquire),
                thread_nanos: self.sink.nanos[stage].load(Ordering::Acquire),
                output_digest: self.sink.digests[stage].load(Ordering::Acquire),
            }))
        }
    }

    impl Drop for ArmedV1 {
        fn drop(&mut self) {
            let previous = self.previous.take();
            CURRENT.with(|current| *current.borrow_mut() = previous);
        }
    }

    /// One call in progress. A span dropped without `finish` (its call
    /// failed) records nothing.
    pub(crate) struct SpanV1 {
        stage: StageV1,
        start: Instant,
        sink: Option<Arc<SinkV1>>,
    }

    #[inline]
    pub(crate) fn span_v1(stage: StageV1) -> SpanV1 {
        SpanV1 {
            stage,
            sink: CURRENT.with(|current| current.borrow().clone()),
            start: Instant::now(),
        }
    }

    impl SpanV1 {
        fn record(self, nanos: u64, digest: u64) {
            if let Some(sink) = &self.sink {
                let stage = self.stage as usize;
                sink.calls[stage].fetch_add(1, Ordering::AcqRel);
                sink.nanos[stage].fetch_add(nanos, Ordering::AcqRel);
                sink.digests[stage].fetch_add(digest, Ordering::AcqRel);
            }
        }

        /// A finished forward with its outputs.
        #[inline]
        pub(crate) fn finish_v1(self, logits: &[f32], value: f32) {
            if self.sink.is_some() {
                let nanos = self.start.elapsed().as_nanos() as u64;
                self.record(nanos, output_hash_v1(logits, value));
            }
        }

        /// A finished call without outputs (the backward).
        #[inline]
        pub(crate) fn finish_plain_v1(self) {
            if self.sink.is_some() {
                let nanos = self.start.elapsed().as_nanos() as u64;
                self.record(nanos, 0);
            }
        }
    }
}

#[cfg(not(feature = "s-update-profile-v1"))]
mod imp {
    use super::{StageTotalsV1, StageV1, STAGES_V1};

    #[derive(Clone)]
    pub(crate) struct HandleV1;

    #[inline(always)]
    pub(crate) fn current_v1() -> HandleV1 {
        HandleV1
    }

    #[inline(always)]
    pub(crate) fn adopt_v1(_: &HandleV1) {}

    pub(crate) struct ArmedV1;

    pub(crate) fn arm_v1() -> ArmedV1 {
        ArmedV1
    }

    impl ArmedV1 {
        pub(crate) fn totals_v1(&self) -> Option<[StageTotalsV1; STAGES_V1]> {
            None
        }
    }

    pub(crate) struct SpanV1;

    #[inline(always)]
    pub(crate) fn span_v1(_: StageV1) -> SpanV1 {
        SpanV1
    }

    impl SpanV1 {
        #[inline(always)]
        pub(crate) fn finish_v1(self, _: &[f32], _: f32) {}

        #[inline(always)]
        pub(crate) fn finish_plain_v1(self) {}
    }
}

pub(crate) use imp::{adopt_v1, arm_v1, current_v1, span_v1};

/// Whether this build records stages (the `s-update-profile-v1` feature).
pub(crate) const ENABLED_V1: bool = cfg!(feature = "s-update-profile-v1");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_hash_separates_outputs_and_sums_ignore_order() {
        let a = output_hash_v1(&[1.0, -0.0], 0.5);
        let b = output_hash_v1(&[1.0, 0.0], 0.5);
        let c = output_hash_v1(&[1.0], 0.5);
        let d = output_hash_v1(&[1.0, -0.0], -0.5);
        // Signed zero, length and value all change the hash.
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
        assert_eq!(a.wrapping_add(b), b.wrapping_add(a));
    }

    /// Spans of an unarmed thread record nothing; an armed thread's workers
    /// record into its sink only after adopting it; dropping the arm restores
    /// the previous sink.
    #[cfg(feature = "s-update-profile-v1")]
    #[test]
    fn spans_record_only_into_the_armed_scope() {
        span_v1(StageV1::LearnerForward).finish_v1(&[1.0], 0.0);
        let armed = arm_v1();
        span_v1(StageV1::LearnerForward).finish_v1(&[1.0], 0.0);
        drop(span_v1(StageV1::LearnerForward));
        let handle = current_v1();
        std::thread::scope(|scope| {
            scope.spawn(|| span_v1(StageV1::ReplayLearnerRow).finish_v1(&[1.0], 0.0));
            scope.spawn(|| {
                adopt_v1(&handle);
                span_v1(StageV1::ReplayLearnerRow).finish_v1(&[1.0], 0.0);
                span_v1(StageV1::LearnerBackward).finish_plain_v1();
            });
        });
        let totals = armed.totals_v1().unwrap();
        assert_eq!(totals[StageV1::LearnerForward as usize].calls, 1);
        assert_eq!(totals[StageV1::ReplayLearnerRow as usize].calls, 1);
        assert_eq!(totals[StageV1::LearnerBackward as usize].calls, 1);
        assert_eq!(totals[StageV1::ReplayOtherRow as usize].calls, 0);
        assert_eq!(
            totals[StageV1::LearnerForward as usize].output_digest,
            totals[StageV1::ReplayLearnerRow as usize].output_digest
        );
        assert_eq!(totals[StageV1::LearnerBackward as usize].output_digest, 0);
        drop(armed);
        assert!(current_v1().0.is_none());
    }
}
