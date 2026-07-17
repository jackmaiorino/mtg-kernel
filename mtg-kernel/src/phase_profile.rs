//! Opt-in, aggregate wall-clock profiling for the JSONL RL environment.
//!
//! This module is deliberately outside every protocol response and game-state
//! hash.  The ordinary path passes `None` to [`measure_optional`], which does
//! not read the monotonic clock.  Benchmark callers opt in explicitly and get
//! one aggregate record after graceful process EOF.

use serde::Serialize;
use std::time::Instant;

pub const RL_PHASE_PROFILE_SCHEMA_V1: &str = "kernel_rl_phase_profile/v1";
pub const RL_PHASE_PROFILE_CLOCK_V1: &str = "std_instant_monotonic_ns/v1";
pub const RL_PHASE_PROFILE_PREFIX_V1: &str = "MTG_KERNEL_PROFILE_V1\t";

#[derive(Debug, Clone, Copy)]
pub enum RlPhaseV1 {
    Parse,
    Decode,
    Retry,
    Reset,
    StepValidation,
    StepIntegrity,
    StepSelection,
    StepApply,
    Advance,
    Observe,
    Actions,
    Postbind,
    Response,
    Serialize,
    WriteFlush,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PhaseCounterV1 {
    pub count: u64,
    pub total_ns: u64,
    pub max_ns: u64,
}

impl PhaseCounterV1 {
    fn add(&mut self, elapsed_ns: u128) {
        self.count = self.count.saturating_add(1);
        let elapsed_ns = u64::try_from(elapsed_ns).unwrap_or(u64::MAX);
        self.total_ns = self.total_ns.saturating_add(elapsed_ns);
        self.max_ns = self.max_ns.max(elapsed_ns);
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RlPhaseCountersV1 {
    pub parse: PhaseCounterV1,
    pub decode: PhaseCounterV1,
    pub retry: PhaseCounterV1,
    pub reset: PhaseCounterV1,
    pub step_validation: PhaseCounterV1,
    pub step_integrity: PhaseCounterV1,
    pub step_selection: PhaseCounterV1,
    pub step_apply: PhaseCounterV1,
    pub advance: PhaseCounterV1,
    pub observe: PhaseCounterV1,
    pub actions: PhaseCounterV1,
    pub postbind: PhaseCounterV1,
    pub response: PhaseCounterV1,
    pub serialize: PhaseCounterV1,
    pub write_flush: PhaseCounterV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RlPhaseProfileV1 {
    pub schema: &'static str,
    pub clock: &'static str,
    pub request_lines: u64,
    pub response_lines: u64,
    pub reset_requests: u64,
    pub step_requests: u64,
    pub phases: RlPhaseCountersV1,
}

pub struct RlProfileTimerV1(Instant);

impl RlProfileTimerV1 {
    pub fn start() -> Self {
        Self(monotonic_now())
    }

    fn elapsed_ns(self) -> u128 {
        self.0.elapsed().as_nanos()
    }
}

impl Default for RlPhaseProfileV1 {
    fn default() -> Self {
        Self {
            schema: RL_PHASE_PROFILE_SCHEMA_V1,
            clock: RL_PHASE_PROFILE_CLOCK_V1,
            request_lines: 0,
            response_lines: 0,
            reset_requests: 0,
            step_requests: 0,
            phases: RlPhaseCountersV1::default(),
        }
    }
}

impl RlPhaseProfileV1 {
    pub fn measure<T>(&mut self, phase: RlPhaseV1, operation: impl FnOnce() -> T) -> T {
        let start = monotonic_now();
        let result = operation();
        self.counter_mut(phase).add(start.elapsed().as_nanos());
        result
    }

    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).expect("phase profile serializes")
    }

    pub fn record_elapsed(&mut self, phase: RlPhaseV1, timer: RlProfileTimerV1) {
        self.counter_mut(phase).add(timer.elapsed_ns());
    }

    fn counter_mut(&mut self, phase: RlPhaseV1) -> &mut PhaseCounterV1 {
        match phase {
            RlPhaseV1::Parse => &mut self.phases.parse,
            RlPhaseV1::Decode => &mut self.phases.decode,
            RlPhaseV1::Retry => &mut self.phases.retry,
            RlPhaseV1::Reset => &mut self.phases.reset,
            RlPhaseV1::StepValidation => &mut self.phases.step_validation,
            RlPhaseV1::StepIntegrity => &mut self.phases.step_integrity,
            RlPhaseV1::StepSelection => &mut self.phases.step_selection,
            RlPhaseV1::StepApply => &mut self.phases.step_apply,
            RlPhaseV1::Advance => &mut self.phases.advance,
            RlPhaseV1::Observe => &mut self.phases.observe,
            RlPhaseV1::Actions => &mut self.phases.actions,
            RlPhaseV1::Postbind => &mut self.phases.postbind,
            RlPhaseV1::Response => &mut self.phases.response,
            RlPhaseV1::Serialize => &mut self.phases.serialize,
            RlPhaseV1::WriteFlush => &mut self.phases.write_flush,
        }
    }
}

/// Execute an operation with no clock read when profiling is disabled.
pub fn measure_optional<T>(
    profile: &mut Option<&mut RlPhaseProfileV1>,
    phase: RlPhaseV1,
    operation: impl FnOnce() -> T,
) -> T {
    match profile.as_deref_mut() {
        Some(profile) => profile.measure(phase, operation),
        None => operation(),
    }
}

#[cfg(test)]
thread_local! {
    static TEST_CLOCK_READS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn monotonic_now() -> Instant {
    #[cfg(test)]
    TEST_CLOCK_READS.with(|reads| reads.set(reads.get().saturating_add(1)));
    Instant::now()
}

#[cfg(test)]
pub(crate) fn reset_test_clock_reads() {
    TEST_CLOCK_READS.with(|reads| reads.set(0));
}

#[cfg(test)]
pub(crate) fn test_clock_reads() -> u64 {
    TEST_CLOCK_READS.with(std::cell::Cell::get)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_measurement_reads_no_clock() {
        reset_test_clock_reads();
        let mut profile = None;
        assert_eq!(measure_optional(&mut profile, RlPhaseV1::Parse, || 7), 7);
        assert_eq!(test_clock_reads(), 0);
    }

    #[test]
    fn record_has_fixed_schema_and_counter_keys() {
        let mut profile = RlPhaseProfileV1::default();
        assert_eq!(profile.measure(RlPhaseV1::Parse, || 9), 9);
        let value: serde_json::Value = serde_json::from_str(&profile.canonical_json()).unwrap();
        assert_eq!(value["schema"], RL_PHASE_PROFILE_SCHEMA_V1);
        assert_eq!(value["phases"]["parse"]["count"], 1);
        assert!(value["phases"]["parse"]["max_ns"].as_u64().is_some());
        assert_eq!(value["phases"].as_object().unwrap().len(), 15);
    }
}
