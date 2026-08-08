//! Opt-in, in-memory wall-clock diagnostics for the native trainer.
//!
//! These records are deliberately not a Store, benchmark, checkpoint, or
//! evidence schema. They carry no identity string, implement no serialization,
//! and are returned only by the explicitly profiled executor entry point. The
//! ordinary trainer entry points construct a disabled recorder and do not read
//! the monotonic clock at phase boundaries.

use std::time::Instant;

/// Coarse native-update phases intended to localize wall-clock cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NativeTrainingPhaseV1 {
    SetupValidation,
    Rollout,
    GroupingMaterialization,
    ForwardLoss,
    BackwardGauge,
    AdamMath,
    FinalizationCloning,
    EvidenceConstruction,
    CleanupDrop,
}

impl NativeTrainingPhaseV1 {
    pub const ALL: [Self; 9] = [
        Self::SetupValidation,
        Self::Rollout,
        Self::GroupingMaterialization,
        Self::ForwardLoss,
        Self::BackwardGauge,
        Self::AdamMath,
        Self::FinalizationCloning,
        Self::EvidenceConstruction,
        Self::CleanupDrop,
    ];

    pub const fn label_v1(self) -> &'static str {
        match self {
            Self::SetupValidation => "setup_validation",
            Self::Rollout => "rollout",
            Self::GroupingMaterialization => "grouping_materialization",
            Self::ForwardLoss => "forward_loss",
            Self::BackwardGauge => "backward_gauge",
            Self::AdamMath => "adam_math",
            Self::FinalizationCloning => "finalization_cloning",
            Self::EvidenceConstruction => "evidence_construction",
            Self::CleanupDrop => "cleanup_drop",
        }
    }
}

/// One completed, non-overlapping phase span in execution order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeTrainingPhaseRecordV1 {
    pub phase: NativeTrainingPhaseV1,
    pub elapsed_ns: u64,
}

/// Diagnostic timeline for one successful native update.
///
/// A phase may appear more than once when the same category has disjoint work
/// at separate ownership boundaries. `accounted_elapsed_ns_v1` therefore sums
/// records rather than assuming one record per phase. Saturating arithmetic is
/// diagnostic only and cannot affect the training result.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeTrainingPhaseProfileV1 {
    update_elapsed_ns: u64,
    records: Vec<NativeTrainingPhaseRecordV1>,
}

impl NativeTrainingPhaseProfileV1 {
    pub fn update_elapsed_ns_v1(&self) -> u64 {
        self.update_elapsed_ns
    }

    pub fn records_v1(&self) -> &[NativeTrainingPhaseRecordV1] {
        &self.records
    }

    pub fn phase_elapsed_ns_v1(&self, phase: NativeTrainingPhaseV1) -> u64 {
        self.records
            .iter()
            .filter(|record| record.phase == phase)
            .fold(0_u64, |total, record| {
                total.saturating_add(record.elapsed_ns)
            })
    }

    pub fn phase_record_count_v1(&self, phase: NativeTrainingPhaseV1) -> usize {
        self.records
            .iter()
            .filter(|record| record.phase == phase)
            .count()
    }

    pub fn accounted_elapsed_ns_v1(&self) -> u64 {
        self.records.iter().fold(0_u64, |total, record| {
            total.saturating_add(record.elapsed_ns)
        })
    }

    pub fn unaccounted_elapsed_ns_v1(&self) -> u64 {
        self.update_elapsed_ns
            .saturating_sub(self.accounted_elapsed_ns_v1())
    }

    fn record_elapsed_ns_v1(&mut self, phase: NativeTrainingPhaseV1, elapsed_ns: u64) {
        self.records
            .push(NativeTrainingPhaseRecordV1 { phase, elapsed_ns });
    }
}

pub(crate) struct NativeTrainingPhaseRecorderV1<'profile> {
    profile: Option<&'profile mut NativeTrainingPhaseProfileV1>,
}

impl<'profile> NativeTrainingPhaseRecorderV1<'profile> {
    pub(crate) fn disabled_v1() -> Self {
        Self { profile: None }
    }

    pub(crate) fn enabled_v1(profile: &'profile mut NativeTrainingPhaseProfileV1) -> Self {
        Self {
            profile: Some(profile),
        }
    }

    pub(crate) fn is_enabled_v1(&self) -> bool {
        self.profile.is_some()
    }

    pub(crate) fn start_v1(&self, phase: NativeTrainingPhaseV1) -> NativeTrainingPhaseTimerV1 {
        NativeTrainingPhaseTimerV1 {
            phase,
            started: self.profile.is_some().then(Instant::now),
        }
    }

    pub(crate) fn finish_v1(&mut self, timer: NativeTrainingPhaseTimerV1) {
        let Some(started) = timer.started else {
            return;
        };
        let elapsed_ns = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        self.profile
            .as_deref_mut()
            .expect("an enabled timer has an enabled profile")
            .record_elapsed_ns_v1(timer.phase, elapsed_ns);
    }

    pub(crate) fn finish_update_v1(&mut self, elapsed_ns: u64) {
        if let Some(profile) = self.profile.as_deref_mut() {
            profile.update_elapsed_ns = elapsed_ns;
        }
    }
}

#[must_use = "a started diagnostic phase should be finished at its phase boundary"]
pub(crate) struct NativeTrainingPhaseTimerV1 {
    phase: NativeTrainingPhaseV1,
    started: Option<Instant>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accounting_aggregates_repeated_phases_and_preserves_timeline_order() {
        let mut profile = NativeTrainingPhaseProfileV1::default();
        profile.record_elapsed_ns_v1(NativeTrainingPhaseV1::GroupingMaterialization, 11);
        profile.record_elapsed_ns_v1(NativeTrainingPhaseV1::ForwardLoss, 17);
        profile.record_elapsed_ns_v1(NativeTrainingPhaseV1::GroupingMaterialization, 13);
        profile.record_elapsed_ns_v1(NativeTrainingPhaseV1::CleanupDrop, 5);
        profile.update_elapsed_ns = 53;

        assert_eq!(
            profile
                .records_v1()
                .iter()
                .map(|record| record.phase)
                .collect::<Vec<_>>(),
            vec![
                NativeTrainingPhaseV1::GroupingMaterialization,
                NativeTrainingPhaseV1::ForwardLoss,
                NativeTrainingPhaseV1::GroupingMaterialization,
                NativeTrainingPhaseV1::CleanupDrop,
            ]
        );
        assert_eq!(
            profile.phase_elapsed_ns_v1(NativeTrainingPhaseV1::GroupingMaterialization),
            24
        );
        assert_eq!(
            profile.phase_record_count_v1(NativeTrainingPhaseV1::GroupingMaterialization),
            2
        );
        assert_eq!(profile.accounted_elapsed_ns_v1(), 46);
        assert_eq!(profile.unaccounted_elapsed_ns_v1(), 7);
    }

    #[test]
    fn disabled_recorder_does_not_sample_or_append() {
        let mut recorder = NativeTrainingPhaseRecorderV1::disabled_v1();
        let timer = recorder.start_v1(NativeTrainingPhaseV1::ForwardLoss);
        assert!(timer.started.is_none());
        recorder.finish_v1(timer);
        recorder.finish_update_v1(99);
    }
}
