//! Opt-in checkpoint codec benchmark (mtg-kernel#596, milestone 1).
//!
//! Measurement only: no codec, format, default or validation changes. Built
//! only with the `checkpoint-codec-bench-v1` feature and run through
//! `examples/checkpoint_codec_bench_v1.rs`; `docs/checkpoint_codec_bench_v1.md`
//! has the commands, the receipt schema and the materialization inventory.
//!
//! Two phases:
//!
//! - [`generate_fixture_v1`] writes a retained fixture once: the synthetic
//!   fresh FlatV4 source the update tests use (real deterministic net-8
//!   weights), a chain of real Collect and Update commands over the checked-in
//!   Affinity and Terror lists, the trajectories each measured state collects,
//!   and S train states (`s_transformer_v1`) warmed by real S steps on
//!   synthetic batches. The measured ordinary states are the first update's
//!   (`update-1`, Adam step 1) and the chain's last (`warmed`).
//! - [`measure_v1`] reads that fixture and times, separately and on identical
//!   state, every stage an ordinary update runs to publish its checkpoint and
//!   read it back, in production order, for plain v1 JSON, the v2 container
//!   (what `Auto` writes for these kinds) and the `zstd1` envelope of each;
//!   real `UpdatePrepared` commands per format; the public V1/V2 pair; and the
//!   S learner's codec stages.
//!
//! Baseline and candidate builds measure one fixture, so they publish from
//! identical inputs and their record bytes compare exactly (records pin
//! fixture paths, so a fixture must not move after generation).
//!
//! # The ordinary sequence
//!
//! One repetition replays `execute_update_inner_v1` from `checkpoint_started`
//! through its v2 item-for-item comparison, with the same calls in the same
//! order: the live state's tensors (`tensors_v1`, borrowed), the state digest,
//! the checkpoint of those tensors (`ParameterBitsRefV1`) and format
//! resolution, encode, seal, durable publication, the continuation loader's
//! readback, the readback digest and (v2) the restored state's item-for-item
//! comparison.
//! The process digest memo is emptied before each repetition, so the first
//! digest misses as it does after a real update; the readback then runs with
//! whatever the publication left in the process caches, as in production.
//! The decode is then broken down on the published bytes (`probe_*` stages),
//! outside the sequence's total.
//!
//! # Samples
//!
//! Each sample is the stage's wall and process CPU time and, when the binary
//! installs a counting allocator ([`AllocationProbeV1`]), the bytes and number
//! of allocations it made, its transient heap peak above its starting heap
//! and the heap it left allocated (its outputs). Every repetition is kept;
//! summaries are medians and quartiles over them. This is not an
//! updates-per-hour or full-node claim: a faster codec stage does not by
//! itself establish a complete-update or fleet gain (mtg-kernel#594).

use super::checkpoint_container_v1::{
    self, same_tensors_v1, CheckpointRecordFormatV1, ParameterBitsRefV1,
};
use super::record_envelope_v1::{self, RecordEnvelopeV1};
use super::s_learner_v1::S_LEARNER_STATE_FILE_V1;
use super::{
    err, execute_v1, hex, initialize_any_math_v1, publish_json_bytes_v1, publish_json_shared_v1,
    read_checkpoint_v1, read_pinned_bytes, read_pinned_shared_v1, serialize_json_v1,
    verified_bytes_cache_v1, CollectionSamplerV1, ExpandedCheckpointV1, ExpandedDeckListV1,
    ExpandedEpisodeV1, ExpandedLossSelectionV1, ExpandedModelSourceV1, ExpandedTrainingCommandV1,
    ExpandedUpdateBackendV1, PinnedFileV1, UpdateBackwardExecutionV1,
    DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES, MAX_FILE_BYTES,
};
use crate::native_policy_train_step_v1::{
    NativePolicyValueTrainSnapshotV1, NativePolicyValueTrainStateV1, TrainTensorsV1,
};
use crate::native_policy_value_net_v1::NativeNamedParameterV1;
use crate::s_transformer_v1::tools::{groups_of_v1, synthetic_batch_v1, MEAN_SHAPE_V1};
use crate::s_transformer_v1::{
    decode_checkpoint_v1, encode_checkpoint_v1, SConfigV1, SModelV1, STrainStateV1, SWorkV1,
    SplitMixV1,
};
use crate::sideboard_play_policy_v1::{FrozenPlayObservationTransferV3, FRESH_FEATURE_IDENTITY_V4};
use crate::training_math_v1::TrainingMathV1;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The fixture manifest's schema.
pub const FIXTURE_SCHEMA_V1: &str = "mtg-kernel-checkpoint-codec-bench-fixture/v1";
/// The measurement receipt's schema.
pub const RECEIPT_SCHEMA_V1: &str = "mtg-kernel-checkpoint-codec-bench/v1";
const FIXTURE_MANIFEST_NAME_V1: &str = "fixture.json";
/// The real two-update continuation fixture's update settings
/// (`native_expanded_training_run_v1`'s `parallel_two_iteration_config_v1`).
const LEARNING_RATE_V1: f32 = 0.0003;
const VALUE_COEFFICIENT_V1: f32 = 0.5;
const GAMMA_V1: f32 = 1.0;
const LAMBDA_V1: f32 = 0.9;
/// The RT1 recipe's S constants (`s_transformer_v1::tools`).
const S_VALUE_COEFFICIENT_V1: f32 = 0.5;
const S_LEARNING_RATE_V1: f32 = 1.0e-4;
/// Adam steps of the digest memo's eviction streams: far above any fixture
/// step and inside the `i32` range the train state admits.
const EVICTION_ADAM_STEP_BASE_V1: u64 = 1_000_000_000;

/// Allocation counters a benchmark binary's global allocator keeps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AllocationCountersV1 {
    /// Bytes requested since process start (a reallocation counts its new size).
    pub allocated_bytes: u64,
    /// Allocations and reallocations since process start.
    pub allocations: u64,
    /// Bytes currently allocated.
    pub live_bytes: u64,
    /// The highest `live_bytes` since the last [`AllocationProbeV1::reset_peak_v1`],
    /// counting both blocks of a reallocation that moved.
    pub peak_live_bytes: u64,
}

/// Read access to a counting global allocator. The benchmark resets the peak
/// at the start of every sample, so only one thread may sample at a time.
pub trait AllocationProbeV1 {
    fn read_v1(&self) -> AllocationCountersV1;
    /// Restarts the high-water mark at the current live bytes.
    fn reset_peak_v1(&self);
}

/// [`generate_fixture_v1`]'s settings.
#[derive(Clone, Debug)]
pub struct GenerateConfigV1 {
    /// A fresh absolute directory; the fixture is never moved afterwards.
    pub fixture_directory: PathBuf,
    pub seed: u64,
    /// Games collected per update (the two-update fixture plays two).
    pub games_per_update: usize,
    /// Updates in the chain; the last one's state is `warmed`.
    pub warm_updates: usize,
    /// Real S steps from the fresh S optimizer to `s-warmed` (0: none).
    pub s_warm_steps: usize,
    /// Synthetic groups per S step.
    pub s_groups_per_step: usize,
}

impl GenerateConfigV1 {
    /// The documented defaults for `fixture_directory`.
    pub fn defaults_v1(fixture_directory: PathBuf) -> Self {
        Self {
            fixture_directory,
            seed: 2_026_110_801,
            games_per_update: 2,
            warm_updates: 8,
            s_warm_steps: 8,
            s_groups_per_step: 16,
        }
    }
}

/// Measurement sections.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SectionV1 {
    /// The ordinary publication and readback sequence, stage by stage.
    Stages,
    /// Real `UpdatePrepared` commands, one per format and envelope.
    Updates,
    /// The public V1/V2 pair (`python/tools/tests/fixtures/checkpoint_container_v1`).
    PublicPair,
    /// The S learner's codec stages.
    S,
}

impl SectionV1 {
    pub const ALL: [Self; 4] = [Self::Stages, Self::Updates, Self::PublicPair, Self::S];

    pub fn name_v1(self) -> &'static str {
        match self {
            Self::Stages => "stages",
            Self::Updates => "updates",
            Self::PublicPair => "public-pair",
            Self::S => "s",
        }
    }

    pub fn parse_v1(name: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|section| section.name_v1() == name)
            .ok_or_else(|| {
                format!("unknown section {name:?}; expected stages, updates, public-pair or s")
            })
    }
}

/// [`measure_v1`]'s settings.
#[derive(Clone, Debug)]
pub struct MeasureConfigV1 {
    pub fixture_directory: PathBuf,
    /// A fresh absolute directory for publications; each repetition's files
    /// are removed after it is checked.
    pub work_directory: PathBuf,
    pub sections: Vec<SectionV1>,
    /// Timed repetitions per stage cell, after `warmup` untimed ones.
    pub reps: usize,
    pub warmup: usize,
    /// Timed and untimed repetitions of the real-update cells.
    pub update_reps: usize,
    pub update_warmup: usize,
    /// `UpdatePrepared`'s preparation workers in the real-update cells: one
    /// setting for every cell, since only `UpdatePrepared` takes an envelope.
    pub preparation_workers: usize,
    /// The public V1/V2 pair's directory, when measured.
    pub public_pair: Option<PathBuf>,
    /// More ordinary states to measure in the stage sequence, such as a
    /// representative saved checkpoint: a label and a source whose
    /// `checkpoint` pins it.
    pub extra_states: Vec<(String, ExpandedModelSourceV1)>,
    /// Refuses to start another repetition after this much wall time.
    pub max_wall_seconds: Option<u64>,
    /// Turns on the process's resident verified reads before measuring, as a
    /// run with `verified_reads: "resident"` does; off is the default
    /// (`reread`). The mode cannot be turned off again in a process.
    pub resident_reads: bool,
    /// Free text recorded in the receipt (build profile, A/B role).
    pub label: String,
}

impl MeasureConfigV1 {
    /// The documented defaults for a fixture and work directory.
    pub fn defaults_v1(fixture_directory: PathBuf, work_directory: PathBuf) -> Self {
        Self {
            fixture_directory,
            work_directory,
            sections: SectionV1::ALL.to_vec(),
            reps: 10,
            warmup: 2,
            update_reps: 5,
            update_warmup: 1,
            preparation_workers: 2,
            public_pair: None,
            extra_states: Vec::new(),
            max_wall_seconds: None,
            resident_reads: false,
            label: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureManifestV1 {
    schema: String,
    generator: Value,
    config: FixtureConfigRecordV1,
    initial_source: ExpandedModelSourceV1,
    chain: Vec<FixtureUpdateV1>,
    states: Vec<FixtureStateV1>,
    s_states: Vec<FixtureSStateV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureConfigRecordV1 {
    seed: u64,
    games_per_update: usize,
    warm_updates: usize,
    decks: [String; 2],
    learning_rate: f32,
    value_coefficient: f32,
    gamma: f32,
    lambda: f32,
    s_seed: u64,
    s_config: SConfigV1,
    s_warm_steps: usize,
    s_groups_per_step: usize,
    s_learning_rate: f32,
    s_value_coefficient: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureUpdateV1 {
    update: usize,
    trajectories: Vec<PinnedFileV1>,
    checkpoint: PinnedFileV1,
    adam_step: u64,
    after_state_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureStateV1 {
    label: String,
    source: ExpandedModelSourceV1,
    adam_step: u64,
    state_sha256: String,
    /// Trajectories this state collected: a real update's inputs from it.
    trajectories: Vec<PinnedFileV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureSStateV1 {
    label: String,
    container: PinnedFileV1,
    adam_step: u64,
    state_sha256: String,
}

fn ensure_v1(ok: bool, message: impl Into<String>) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn sha256_hex_v1(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn duration_ns_v1(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn process_cpu_ns_v1() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        macos_v1::process_cpu_ns_v1()
    }
    #[cfg(not(target_os = "macos"))]
    {
        crate::regret_census_v1::stage4a::process_cpu::seconds()
            .ok()
            .map(|seconds| (seconds * 1e9).round() as u64)
    }
}

/// The process's peak resident set (Linux `VmHWM`, Windows peak working
/// set, macOS `ru_maxrss`), when the platform reports one.
pub fn peak_rss_bytes_v1() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = fs::read_to_string("/proc/self/status").ok()?;
        let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
        let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
        Some(kib * 1024)
    }
    #[cfg(windows)]
    {
        windows_v1::peak_rss_bytes_v1()
    }
    #[cfg(target_os = "macos")]
    {
        macos_v1::peak_rss_bytes_v1()
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        None
    }
}

fn cpu_model_v1() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let info = fs::read_to_string("/proc/cpuinfo").ok()?;
        let line = info.lines().find(|line| line.starts_with("model name"))?;
        Some(line.split_once(':')?.1.trim().to_owned())
    }
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .ok()?;
        let model = String::from_utf8(output.stdout).ok()?;
        let model = model.trim();
        (output.status.success() && !model.is_empty()).then(|| model.to_owned())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        std::env::var("PROCESSOR_IDENTIFIER").ok()
    }
}

/// The process CPU clock and peak resident set on macOS, where the crate's
/// Linux and Windows readers report nothing.
#[cfg(target_os = "macos")]
mod macos_v1 {
    use std::ffi::{c_int, c_long};

    #[repr(C)]
    struct TimespecV1 {
        seconds: i64,
        nanoseconds: c_long,
    }

    /// `struct rusage` on 64-bit Darwin: two 16-byte `struct timeval`s,
    /// then fourteen `long`s, the first of which is `ru_maxrss`.
    #[repr(C)]
    struct RusageV1 {
        user_time: [i64; 2],
        system_time: [i64; 2],
        longs: [c_long; 14],
    }

    const CLOCK_PROCESS_CPUTIME_ID_V1: c_int = 12;
    const RUSAGE_SELF_V1: c_int = 0;

    unsafe extern "C" {
        fn clock_gettime(clock: c_int, out: *mut TimespecV1) -> c_int;
        fn getrusage(who: c_int, usage: *mut RusageV1) -> c_int;
    }

    pub(super) fn process_cpu_ns_v1() -> Option<u64> {
        let mut out = TimespecV1 {
            seconds: 0,
            nanoseconds: 0,
        };
        // SAFETY: CLOCK_PROCESS_CPUTIME_ID is 12 on Darwin and `out` is a
        // writable `struct timespec`.
        if unsafe { clock_gettime(CLOCK_PROCESS_CPUTIME_ID_V1, &mut out) } != 0 {
            return None;
        }
        let seconds = u64::try_from(out.seconds).ok()?;
        let nanoseconds = u64::try_from(out.nanoseconds).ok()?;
        seconds.checked_mul(1_000_000_000)?.checked_add(nanoseconds)
    }

    pub(super) fn peak_rss_bytes_v1() -> Option<u64> {
        let mut usage = RusageV1 {
            user_time: [0; 2],
            system_time: [0; 2],
            longs: [0; 14],
        };
        // SAFETY: RUSAGE_SELF is 0 and `usage` has the size and alignment of
        // Darwin's `struct rusage`.
        if unsafe { getrusage(RUSAGE_SELF_V1, &mut usage) } != 0 {
            return None;
        }
        // Darwin reports `ru_maxrss` in bytes (Linux in KiB).
        u64::try_from(usage.longs[0]).ok()
    }
}

/// The process's peak working set on Windows. The crate's other reader lives
/// in the calibration runner, which is compiled only for tests.
#[cfg(windows)]
mod windows_v1 {
    use std::ffi::c_void;

    /// `PROCESS_MEMORY_COUNTERS`. Only `cb` and the peak are read; the rest
    /// give the struct its Win32 layout.
    #[repr(C)]
    struct ProcessMemoryCountersV1 {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut ProcessMemoryCountersV1,
            size: u32,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
    }

    pub(super) fn peak_rss_bytes_v1() -> Option<u64> {
        let mut counters = ProcessMemoryCountersV1 {
            cb: u32::try_from(std::mem::size_of::<ProcessMemoryCountersV1>()).ok()?,
            page_fault_count: 0,
            peak_working_set_size: 0,
            working_set_size: 0,
            quota_peak_paged_pool_usage: 0,
            quota_paged_pool_usage: 0,
            quota_peak_non_paged_pool_usage: 0,
            quota_non_paged_pool_usage: 0,
            pagefile_usage: 0,
            peak_pagefile_usage: 0,
        };
        // SAFETY: `GetCurrentProcess` returns a pseudo-handle that needs no
        // closing, and `counters` is a zeroed `PROCESS_MEMORY_COUNTERS` whose
        // `cb` holds its size.
        let succeeded =
            unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) != 0 };
        if !succeeded {
            return None;
        }
        u64::try_from(counters.peak_working_set_size).ok()
    }
}

/// The build's own identity, as `build.rs` records it.
pub fn build_identity_v1() -> Value {
    json!({
        "git_head": option_env!("MTG_KERNEL_BUILD_GIT_HEAD"),
        "git_tree": option_env!("MTG_KERNEL_BUILD_GIT_TREE"),
        "git_clean": option_env!("MTG_KERNEL_BUILD_GIT_CLEAN").map(|clean| clean == "true"),
        "tracked_tree_sha256": option_env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
        "debug_assertions": cfg!(debug_assertions),
        "effective_rustflags": option_env!("MTG_KERNEL_EFFECTIVE_RUSTFLAGS"),
        "effective_encoded_rustflags": option_env!("MTG_KERNEL_EFFECTIVE_ENCODED_RUSTFLAGS"),
        "effective_target_rustflags": option_env!("MTG_KERNEL_EFFECTIVE_TARGET_RUSTFLAGS"),
        "target_os": std::env::consts::OS,
        "target_arch": std::env::consts::ARCH,
    })
}

fn machine_v1() -> Value {
    json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "logical_cpus": std::thread::available_parallelism().map(usize::from).ok(),
        "cpu_model": cpu_model_v1(),
    })
}

// ---------------------------------------------------------------- sampling

#[derive(Clone, Copy, Debug, Default)]
struct SampleV1 {
    wall_ns: u64,
    cpu_ns: Option<u64>,
    allocated_bytes: Option<u64>,
    allocations: Option<u64>,
    /// The stage's transient heap peak above its starting heap.
    peak_heap_bytes: Option<u64>,
    /// Heap the stage left allocated (its outputs), negative when it freed.
    retained_heap_bytes: Option<i64>,
    /// Internal: the absolute peak, for a sequence's total.
    peak_live_abs: Option<u64>,
}

impl SampleV1 {
    /// A duration a receipt reported, without CPU or allocation data.
    fn reported_seconds_v1(seconds: f64) -> Self {
        Self {
            wall_ns: (seconds * 1e9).round() as u64,
            ..Self::default()
        }
    }

    fn to_json_v1(self) -> Value {
        let mut sample = json!({ "wall_ns": self.wall_ns });
        let fields = [
            ("cpu_ns", self.cpu_ns.map(Value::from)),
            ("allocated_bytes", self.allocated_bytes.map(Value::from)),
            ("allocations", self.allocations.map(Value::from)),
            ("peak_heap_bytes", self.peak_heap_bytes.map(Value::from)),
            (
                "retained_heap_bytes",
                self.retained_heap_bytes.map(Value::from),
            ),
        ];
        for (name, value) in fields {
            if let Some(value) = value {
                sample[name] = value;
            }
        }
        sample
    }
}

#[derive(Clone, Copy)]
struct MeterV1<'a> {
    probe: Option<&'a dyn AllocationProbeV1>,
}

struct OpenSampleV1 {
    started: Instant,
    cpu: Option<u64>,
    counters: Option<AllocationCountersV1>,
}

impl MeterV1<'_> {
    fn open_v1(self, reset_peak: bool) -> OpenSampleV1 {
        let counters = self.probe.map(|probe| {
            if reset_peak {
                probe.reset_peak_v1();
            }
            probe.read_v1()
        });
        let cpu = process_cpu_ns_v1();
        OpenSampleV1 {
            started: Instant::now(),
            cpu,
            counters,
        }
    }

    fn close_v1(self, open: OpenSampleV1, peak_override: Option<u64>) -> SampleV1 {
        let wall_ns = duration_ns_v1(open.started.elapsed());
        let cpu = process_cpu_ns_v1();
        let after = self.probe.map(|probe| probe.read_v1());
        let mut sample = SampleV1 {
            wall_ns,
            cpu_ns: open
                .cpu
                .zip(cpu)
                .map(|(before, after)| after.saturating_sub(before)),
            ..SampleV1::default()
        };
        if let (Some(before), Some(after)) = (open.counters, after) {
            let peak = peak_override.unwrap_or(after.peak_live_bytes);
            sample.allocated_bytes =
                Some(after.allocated_bytes.wrapping_sub(before.allocated_bytes));
            sample.allocations = Some(after.allocations.wrapping_sub(before.allocations));
            sample.peak_heap_bytes = Some(peak.saturating_sub(before.live_bytes));
            sample.retained_heap_bytes = Some(after.live_bytes as i64 - before.live_bytes as i64);
            sample.peak_live_abs = Some(peak);
        }
        sample
    }

    /// Times `body` alone.
    fn time_v1<T>(self, body: impl FnOnce() -> Result<T, String>) -> Result<(T, SampleV1), String> {
        let open = self.open_v1(true);
        let value = body()?;
        Ok((value, self.close_v1(open, None)))
    }
}

/// A sequence of stages timed one by one, plus their total.
struct SequenceV1<'a> {
    meter: MeterV1<'a>,
    open: OpenSampleV1,
    peak_abs: u64,
    steps: Vec<(&'static str, SampleV1)>,
}

impl<'a> SequenceV1<'a> {
    fn start_v1(meter: MeterV1<'a>) -> Self {
        let open = meter.open_v1(true);
        let peak_abs = open.counters.map_or(0, |counters| counters.live_bytes);
        Self {
            meter,
            open,
            peak_abs,
            steps: Vec::new(),
        }
    }

    fn step_v1<T>(
        &mut self,
        stage: &'static str,
        body: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let (value, sample) = self.meter.time_v1(body)?;
        if let Some(peak) = sample.peak_live_abs {
            self.peak_abs = self.peak_abs.max(peak);
        }
        self.steps.push((stage, sample));
        Ok(value)
    }

    fn finish_v1(self) -> (StepsV1, SampleV1) {
        let peak = self.open.counters.map(|_| self.peak_abs);
        let total = self.meter.close_v1(self.open, peak);
        (self.steps, total)
    }
}

struct SeriesV1 {
    section: &'static str,
    state: String,
    cell: String,
    stage: String,
    samples: Vec<SampleV1>,
}

/// One numeric field of a sample, for a summary column.
type SampleFieldV1 = fn(&SampleV1) -> Option<f64>;

/// Named stage samples, in the order they ran.
type StepsV1 = Vec<(&'static str, SampleV1)>;

#[derive(Default)]
struct RecorderV1 {
    series: Vec<SeriesV1>,
    index: BTreeMap<(String, String, String, String), usize>,
}

impl RecorderV1 {
    fn push_v1(
        &mut self,
        section: SectionV1,
        state: &str,
        cell: &str,
        stage: &str,
        sample: SampleV1,
    ) {
        let key = (
            section.name_v1().to_owned(),
            state.to_owned(),
            cell.to_owned(),
            stage.to_owned(),
        );
        let index = *self.index.entry(key).or_insert_with(|| {
            self.series.push(SeriesV1 {
                section: section.name_v1(),
                state: state.to_owned(),
                cell: cell.to_owned(),
                stage: stage.to_owned(),
                samples: Vec::new(),
            });
            self.series.len() - 1
        });
        self.series[index].samples.push(sample);
    }

    fn to_json_v1(&self) -> Value {
        Value::Array(
            self.series
                .iter()
                .map(|series| {
                    let column = |field: SampleFieldV1| -> Option<Value> {
                        let values: Vec<f64> = series.samples.iter().filter_map(field).collect();
                        (values.len() == series.samples.len()).then(|| summary_v1(values))
                    };
                    let mut summary = json!({
                        "n": series.samples.len(),
                        "wall_ns": column(|s| Some(s.wall_ns as f64)),
                    });
                    let optional: [(&str, SampleFieldV1); 5] = [
                        ("cpu_ns", |s| s.cpu_ns.map(|v| v as f64)),
                        ("allocated_bytes", |s| s.allocated_bytes.map(|v| v as f64)),
                        ("allocations", |s| s.allocations.map(|v| v as f64)),
                        ("peak_heap_bytes", |s| s.peak_heap_bytes.map(|v| v as f64)),
                        ("retained_heap_bytes", |s| s.retained_heap_bytes.map(|v| v as f64)),
                    ];
                    for (name, field) in optional {
                        if let Some(value) = column(field) {
                            summary[name] = value;
                        }
                    }
                    json!({
                        "section": series.section,
                        "state": series.state,
                        "cell": series.cell,
                        "stage": series.stage,
                        "summary": summary,
                        "samples": series.samples.iter().map(|s| s.to_json_v1()).collect::<Vec<_>>(),
                    })
                })
                .collect(),
        )
    }
}

/// Linear-interpolation quantile of sorted `values` (`p` in [0, 1]).
fn quantile_v1(sorted: &[f64], p: f64) -> f64 {
    if sorted.len() == 1 {
        return sorted[0];
    }
    let position = p * (sorted.len() - 1) as f64;
    let low = position.floor() as usize;
    let high = position.ceil() as usize;
    sorted[low] + (sorted[high] - sorted[low]) * (position - low as f64)
}

fn summary_v1(mut values: Vec<f64>) -> Value {
    if values.is_empty() {
        return Value::Null;
    }
    values.sort_by(f64::total_cmp);
    json!({
        "median": quantile_v1(&values, 0.5),
        "q1": quantile_v1(&values, 0.25),
        "q3": quantile_v1(&values, 0.75),
        "min": values[0],
        "max": values[values.len() - 1],
    })
}

/// Bounds a measurement's wall time between repetitions.
struct DeadlineV1 {
    started: Instant,
    cap: Option<Duration>,
}

impl DeadlineV1 {
    fn check_v1(&self, at: &str) -> Result<(), String> {
        match self.cap {
            Some(cap) if self.started.elapsed() > cap => Err(format!(
                "the measurement exceeded its {} s wall cap before {at}",
                cap.as_secs()
            )),
            _ => Ok(()),
        }
    }
}

/// Order of the cells in repetition `rep`: rotated, so no cell always runs
/// first after another cell's publications and cache insertions.
fn rotated_v1<T: Copy>(cells: &[T], rep: usize) -> Vec<T> {
    let shift = rep % cells.len().max(1);
    cells[shift..]
        .iter()
        .chain(&cells[..shift])
        .copied()
        .collect()
}

// ----------------------------------------------------------- the fixture

fn deck_list_v1(id: &str) -> Result<ExpandedDeckListV1, String> {
    let deck = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id)
        .map_err(|e| format!("checked-in deck {id}: {e}"))?;
    let configuration = deck.registered_configuration();
    Ok(ExpandedDeckListV1 {
        label: id.into(),
        mainboard: configuration.mainboard().to_vec(),
        sideboard: configuration.sideboard().to_vec(),
    })
}

fn gae_selection_v1() -> ExpandedLossSelectionV1 {
    ExpandedLossSelectionV1::GaeAdvantageValueV1 {
        gamma: GAMMA_V1,
        lambda: LAMBDA_V1,
        entropy_coefficient: 0.0,
    }
}

/// Collects `games` games of the learner `source` against itself, as the
/// update tests' collector does: Affinity and Terror, alternating starts.
fn collect_v1(
    source: &ExpandedModelSourceV1,
    decks: &[ExpandedDeckListV1; 2],
    seed: u64,
    games: usize,
    output_directory: PathBuf,
) -> Result<Vec<PinnedFileV1>, String> {
    let episodes = (0..games)
        .map(|game| ExpandedEpisodeV1 {
            id: format!("checkpoint-codec-bench-{seed}-{game}"),
            seed: seed + 1_000 * game as u64,
            starting_player: (game % 2) as u8,
            learner_seat: 0,
            opponent: Some(source.clone()),
            opponent_search: None,
            opponent_kind: None,
            registered: decks.clone(),
            selected: decks.clone(),
            postboard: false,
            max_physical_decisions: 100_000,
            max_policy_steps: 1_000_000,
            runtime_rules: None,
            reset: None,
            opening: None,
            mulligan_modes: None,
        })
        .collect();
    let collection = execute_v1(ExpandedTrainingCommandV1::Collect {
        source: source.clone(),
        episodes,
        max_non_natural_episode_fraction: 0.0,
        collection_sampler: CollectionSamplerV1::Legacy,
        training_math: TrainingMathV1::PlatformLibm,
        ex1: None,
        output_directory,
    })?;
    serde_json::from_value(collection["trajectories"].clone()).map_err(err)
}

fn write_pinned_v1(path: &Path, bytes: &[u8]) -> Result<PinnedFileV1, String> {
    fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(PinnedFileV1 {
        path: path.canonicalize().map_err(err)?,
        sha256: sha256_hex_v1(bytes),
    })
}

/// Writes the retained fixture (see the module documentation) and returns
/// its manifest.
pub fn generate_fixture_v1(config: &GenerateConfigV1) -> Result<Value, String> {
    let root = &config.fixture_directory;
    ensure_v1(
        root.is_absolute() && !root.exists(),
        "generate needs a fresh absolute fixture directory",
    )?;
    ensure_v1(
        (1..=8).contains(&config.games_per_update)
            && (1..=64).contains(&config.warm_updates)
            && config.s_warm_steps <= 64
            && (1..=256).contains(&config.s_groups_per_step),
        "generate needs games-per-update 1..=8, warm-updates 1..=64, s-warm-steps 0..=64 and s-groups 1..=256",
    )?;
    fs::create_dir_all(root).map_err(err)?;
    let started = Instant::now();

    // The ordinary chain, from the synthetic fresh FlatV4 source.
    let identity = FRESH_FEATURE_IDENTITY_V4;
    let descriptor =
        super::fresh_initialization_source::write_synthetic_fresh_source_with_parameters_v1(
            &root.join("source"),
            identity,
            |_parameters| {},
        );
    let descriptor_bytes = serde_json::to_vec(&descriptor).map_err(err)?;
    let initial_source = ExpandedModelSourceV1 {
        play_import: write_pinned_v1(&root.join("descriptor.json"), &descriptor_bytes)?,
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: identity.feature_contract_digest.into(),
            expected_feature_encoding_digest: identity.feature_encoding_digest.into(),
        },
        checkpoint: None,
        s_learner: None,
        w_learner: None,
    };
    let decks = [deck_list_v1("Affinity")?, deck_list_v1("Terror")?];
    let mut sources = vec![initial_source.clone()];
    let mut collected = Vec::new();
    let mut chain = Vec::new();
    for update in 0..config.warm_updates {
        let source = sources[update].clone();
        let trajectories = collect_v1(
            &source,
            &decks,
            config.seed + update as u64,
            config.games_per_update,
            root.join(format!("collect-{update:02}")),
        )?;
        let receipt = execute_v1(ExpandedTrainingCommandV1::Update {
            source: source.clone(),
            trajectories: trajectories.clone(),
            learning_rate: LEARNING_RATE_V1,
            value_coefficient: VALUE_COEFFICIENT_V1,
            update_backend: ExpandedUpdateBackendV1::Cpu,
            update_backward_execution: UpdateBackwardExecutionV1::Sequential,
            loss_selection: gae_selection_v1(),
            line_b: None,
            training_math: TrainingMathV1::PlatformLibm,
            record_format: CheckpointRecordFormatV1::default(),
            output_directory: root.join(format!("update-{update:02}")),
        })?;
        let checkpoint: PinnedFileV1 =
            serde_json::from_value(receipt["checkpoint"].clone()).map_err(err)?;
        chain.push(FixtureUpdateV1 {
            update,
            trajectories: trajectories.clone(),
            checkpoint: checkpoint.clone(),
            adam_step: receipt["adam_step"]
                .as_u64()
                .ok_or("update receipt lacks adam_step")?,
            after_state_sha256: receipt["after_state_sha256"]
                .as_str()
                .ok_or("update receipt lacks after_state_sha256")?
                .to_owned(),
        });
        collected.push(trajectories);
        sources.push(ExpandedModelSourceV1 {
            checkpoint: Some(checkpoint),
            ..initial_source.clone()
        });
    }
    let last = config.warm_updates;
    collected.push(collect_v1(
        &sources[last],
        &decks,
        config.seed + last as u64,
        config.games_per_update,
        root.join(format!("collect-{last:02}")),
    )?);
    let mut labels = vec![("update-1", 1usize)];
    if last > 1 {
        labels.push(("warmed", last));
    }
    let states = labels
        .into_iter()
        .map(|(label, index)| {
            let update = &chain[index - 1];
            FixtureStateV1 {
                label: label.into(),
                source: sources[index].clone(),
                adam_step: update.adam_step,
                state_sha256: update.after_state_sha256.clone(),
                trajectories: collected[index].clone(),
            }
        })
        .collect();

    // S: the fresh optimizer of S's pilot initializer, and the state after
    // real S steps on synthetic batches.
    let s_seed = config.seed ^ 0x5c0d_ec00_0000_0001;
    let s_config = SConfigV1::s_v1();
    let s_directory = root.join("s");
    fs::create_dir_all(&s_directory).map_err(err)?;
    let initial =
        STrainStateV1::fresh_optimizer_v1(SModelV1::init_pilot_scheme_v1(s_config, s_seed)?);
    let mut s_states = vec![("s-initial", initial.clone())];
    if config.s_warm_steps > 0 {
        let mut rng = SplitMixV1::new(s_seed);
        let mut work = SWorkV1::default();
        let mut warmed = initial;
        for _ in 0..config.s_warm_steps {
            // `s_transformer_v1::tools`' synthetic RT1 batches at the S4a
            // mean shape, as its GAE bench uses.
            let batch = synthetic_batch_v1(&mut rng, config.s_groups_per_step, MEAN_SHAPE_V1);
            warmed.train_step_gae_v1(
                &groups_of_v1(&batch),
                S_VALUE_COEFFICIENT_V1,
                S_LEARNING_RATE_V1,
                &mut work,
            )?;
        }
        s_states.push(("s-warmed", warmed));
    }
    let s_states = s_states
        .into_iter()
        .map(|(label, state)| {
            let container = write_pinned_v1(
                &s_directory.join(format!("{label}.bin")),
                &encode_checkpoint_v1(&state),
            )?;
            Ok(FixtureSStateV1 {
                label: label.into(),
                container,
                adam_step: state.adam_step_v1(),
                state_sha256: state.state_sha256_v1(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let manifest = FixtureManifestV1 {
        schema: FIXTURE_SCHEMA_V1.into(),
        generator: json!({
            "build": build_identity_v1(),
            "machine": machine_v1(),
            "elapsed_seconds": started.elapsed().as_secs_f64(),
        }),
        config: FixtureConfigRecordV1 {
            seed: config.seed,
            games_per_update: config.games_per_update,
            warm_updates: config.warm_updates,
            decks: ["Affinity".into(), "Terror".into()],
            learning_rate: LEARNING_RATE_V1,
            value_coefficient: VALUE_COEFFICIENT_V1,
            gamma: GAMMA_V1,
            lambda: LAMBDA_V1,
            s_seed,
            s_config,
            s_warm_steps: config.s_warm_steps,
            s_groups_per_step: config.s_groups_per_step,
            s_learning_rate: S_LEARNING_RATE_V1,
            s_value_coefficient: S_VALUE_COEFFICIENT_V1,
        },
        initial_source,
        chain,
        states,
        s_states,
    };
    let value = serde_json::to_value(&manifest).map_err(err)?;
    let mut bytes = serde_json::to_vec_pretty(&value).map_err(err)?;
    bytes.push(b'\n');
    fs::write(root.join(FIXTURE_MANIFEST_NAME_V1), &bytes).map_err(err)?;
    Ok(value)
}

fn valid_trajectory_roster_v1(pins: &[PinnedFileV1], expected: usize) -> bool {
    pins.len() == expected
        && pins.iter().all(|pin| {
            pin.path.is_absolute()
                && pin.sha256.len() == 64
                && pin.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        && pins
            .iter()
            .map(|pin| &pin.path)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == expected
}

fn validate_fixture_manifest_v1(manifest: &FixtureManifestV1) -> Result<(), String> {
    let config = &manifest.config;
    ensure_v1(
        manifest.schema == FIXTURE_SCHEMA_V1
            && (1..=64).contains(&config.warm_updates)
            && (1..=8).contains(&config.games_per_update)
            && config.s_warm_steps <= 64
            && (1..=256).contains(&config.s_groups_per_step)
            && config
                .decks
                .iter()
                .map(String::as_str)
                .eq(["Affinity", "Terror"])
            && config.learning_rate == LEARNING_RATE_V1
            && config.value_coefficient == VALUE_COEFFICIENT_V1
            && config.gamma == GAMMA_V1
            && config.lambda == LAMBDA_V1,
        "not a generated checkpoint codec bench fixture",
    )?;
    ensure_v1(
        manifest.chain.len() == config.warm_updates
            && manifest.chain.iter().enumerate().all(|(index, step)| {
                step.update == index
                    && step.adam_step == (index + 1) as u64
                    && valid_trajectory_roster_v1(&step.trajectories, config.games_per_update)
            }),
        "fixture has an incomplete generated update chain or trajectory roster",
    )?;

    let mut ordinary = vec![("update-1", 1)];
    if config.warm_updates > 1 {
        ordinary.push(("warmed", config.warm_updates));
    }
    ensure_v1(
        manifest.states.len() == ordinary.len(),
        "fixture lacks its generated ordinary states",
    )?;
    for (state, (label, index)) in manifest.states.iter().zip(ordinary) {
        let step = &manifest.chain[index - 1];
        ensure_v1(
            state.label == label
                && state.source.checkpoint.as_ref() == Some(&step.checkpoint)
                && state.adam_step == step.adam_step
                && state.state_sha256 == step.after_state_sha256
                && valid_trajectory_roster_v1(&state.trajectories, config.games_per_update)
                && manifest
                    .chain
                    .get(index)
                    .is_none_or(|next| next.trajectories == state.trajectories),
            format!("fixture state {label} differs from its generated chain or trajectory roster"),
        )?;
    }

    let mut s_states = vec![("s-initial", 0)];
    if config.s_warm_steps > 0 {
        s_states.push(("s-warmed", config.s_warm_steps));
    }
    ensure_v1(
        manifest.s_states.len() == s_states.len()
            && manifest
                .s_states
                .iter()
                .zip(s_states)
                .all(|(state, (label, step))| {
                    state.label == label && state.adam_step == step as u64
                }),
        "fixture lacks its generated S states",
    )
}

fn read_fixture_v1(directory: &Path) -> Result<(FixtureManifestV1, String), String> {
    let path = directory.join(FIXTURE_MANIFEST_NAME_V1);
    let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let manifest: FixtureManifestV1 = serde_json::from_slice(&bytes).map_err(err)?;
    validate_fixture_manifest_v1(&manifest)?;
    Ok((manifest, sha256_hex_v1(&bytes)))
}

// ------------------------------------------------------ ordinary stages

#[derive(Clone, Copy, Debug)]
struct CellV1 {
    name: &'static str,
    format: CheckpointRecordFormatV1,
    envelope: RecordEnvelopeV1,
}

const SEQUENCE_CELLS_V1: [CellV1; 4] = [
    CellV1 {
        name: "v1",
        format: CheckpointRecordFormatV1::V1,
        envelope: RecordEnvelopeV1::None,
    },
    CellV1 {
        name: "v1-zstd1",
        format: CheckpointRecordFormatV1::V1,
        envelope: RecordEnvelopeV1::Zstd1,
    },
    CellV1 {
        name: "v2",
        format: CheckpointRecordFormatV1::V2,
        envelope: RecordEnvelopeV1::None,
    },
    CellV1 {
        name: "v2-zstd1",
        format: CheckpointRecordFormatV1::V2,
        envelope: RecordEnvelopeV1::Zstd1,
    },
];

const UPDATE_CELLS_V1: [CellV1; 5] = [
    CellV1 {
        name: "v1",
        format: CheckpointRecordFormatV1::V1,
        envelope: RecordEnvelopeV1::None,
    },
    CellV1 {
        name: "v2",
        format: CheckpointRecordFormatV1::V2,
        envelope: RecordEnvelopeV1::None,
    },
    CellV1 {
        name: "auto",
        format: CheckpointRecordFormatV1::Auto,
        envelope: RecordEnvelopeV1::None,
    },
    CellV1 {
        name: "v1-zstd1",
        format: CheckpointRecordFormatV1::V1,
        envelope: RecordEnvelopeV1::Zstd1,
    },
    CellV1 {
        name: "v2-zstd1",
        format: CheckpointRecordFormatV1::V2,
        envelope: RecordEnvelopeV1::Zstd1,
    },
];

/// The checkpoint an update writes for `tensors`: `metadata`'s members
/// (everything but the state) with the state's step, anchor, digest and
/// tensors, borrowed, exactly as `execute_update_inner_v1` assembles it.
fn checkpoint_of_tensors_v1<'a>(
    metadata: &ExpandedCheckpointV1,
    tensors: &'a TrainTensorsV1<'_>,
    state_sha256: String,
) -> ExpandedCheckpointV1<ParameterBitsRefV1<'a>> {
    let mut checkpoint = checkpoint_container_v1::with_tensors_v1(
        metadata,
        [
            &tensors.parameters,
            &tensors.first_moments,
            &tensors.second_moments,
        ]
        .map(|section| section.iter().map(ParameterBitsRefV1::from).collect()),
    );
    checkpoint.state_sha256 = state_sha256;
    checkpoint.adam_step = tensors.adam_step;
    checkpoint.scorer_bias_anchor_bits = tensors.scorer_bias_anchor_bits;
    checkpoint
}

/// Empties the process digest memo (`state_digest_memo_v1`) of every stream
/// it holds by hashing as many new streams as it can hold: copies of the
/// measured snapshot under Adam steps never used before. Untimed.
struct DigestMemoEvictorV1 {
    snapshot: NativePolicyValueTrainSnapshotV1,
    capacity: usize,
    next_step: u64,
}

impl DigestMemoEvictorV1 {
    fn new_v1(snapshot: NativePolicyValueTrainSnapshotV1, step_offset: u64) -> Self {
        // The memo's own reading of its capacity.
        let capacity = std::env::var("MTG_KERNEL_TRAIN_STATE_DIGEST_MEMO_ENTRIES")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(2);
        Self {
            snapshot,
            capacity,
            next_step: EVICTION_ADAM_STEP_BASE_V1 + step_offset,
        }
    }

    /// A snapshot whose digest stream no held entry matches.
    fn unseen_v1(&mut self) -> &NativePolicyValueTrainSnapshotV1 {
        self.snapshot.adam_step = self.next_step;
        self.next_step += 1;
        &self.snapshot
    }

    fn evict_v1(&mut self) -> Result<(), String> {
        for _ in 0..self.capacity {
            self.unseen_v1().state_sha256_v1().map_err(err)?;
        }
        Ok(())
    }
}

fn moment_density_v1(rows: &[NativeNamedParameterV1]) -> Value {
    let values: usize = rows.iter().map(|row| row.values.len()).sum();
    let nonzero: usize = rows
        .iter()
        .map(|row| row.values.iter().filter(|v| v.to_bits() != 0).count())
        .sum();
    json!({"values": values, "nonzero": nonzero})
}

/// What a measurement accumulates across its sections.
#[derive(Default)]
struct LedgerV1 {
    recorder: RecorderV1,
    identities: IdentitiesV1,
    checks: Vec<Value>,
    states: Vec<Value>,
}

impl LedgerV1 {
    /// Records a passed check (a failed one has already returned an error).
    fn passed_v1(&mut self, section: SectionV1, state: &str, mut check: Value) {
        check["section"] = json!(section.name_v1());
        check["state"] = json!(state);
        check["passed"] = json!(true);
        self.checks.push(check);
    }
}

/// What each repetition of a cell published: the same bytes every time.
#[derive(Default)]
struct IdentitiesV1 {
    published: BTreeMap<(String, String), (String, usize)>,
}

impl IdentitiesV1 {
    fn check_v1(
        &mut self,
        state: &str,
        cell: &str,
        sha256: &str,
        length: usize,
    ) -> Result<(), String> {
        let key = (state.to_owned(), cell.to_owned());
        match self.published.get(&key) {
            Some((held, held_length)) => ensure_v1(
                held == sha256 && *held_length == length,
                format!("{state} {cell}: a repetition published different bytes"),
            ),
            None => {
                self.published.insert(key, (sha256.to_owned(), length));
                Ok(())
            }
        }
    }

    fn get_v1(&self, state: &str, cell: &str) -> Option<&(String, usize)> {
        self.published.get(&(state.to_owned(), cell.to_owned()))
    }

    fn to_json_v1(&self) -> Value {
        Value::Array(
            self.published
                .iter()
                .map(|((state, cell), (sha256, length))| {
                    json!({"state": state, "cell": cell, "sha256": sha256, "bytes": length})
                })
                .collect(),
        )
    }
}

struct OrdinaryStateV1 {
    label: String,
    source: ExpandedModelSourceV1,
    state: NativePolicyValueTrainStateV1,
    metadata: ExpandedCheckpointV1,
    expected_v1_sha256: String,
    v2_admitted: bool,
    trajectories: Option<Vec<PinnedFileV1>>,
    description: Value,
}

/// Loads a measured state through the continuation loader and checks that
/// the benchmark's reassembly of its checkpoint is byte for byte the v1
/// JSON of the checkpoint the fixture holds.
fn load_ordinary_state_v1(
    label: &str,
    source: &ExpandedModelSourceV1,
    trajectories: Option<Vec<PinnedFileV1>>,
) -> Result<OrdinaryStateV1, String> {
    let pin = source
        .checkpoint
        .as_ref()
        .ok_or_else(|| format!("state {label} has no checkpoint"))?;
    ensure_v1(
        source.s_learner.is_none(),
        format!("state {label} is an S learner source; the s section measures S"),
    )?;
    ensure_v1(
        source.w_learner.is_none(),
        format!("state {label} is a wide learner source; the bench measures net-8"),
    )?;
    let (_, state) = initialize_any_math_v1(source)?;
    let saved = read_checkpoint_v1(pin)?;
    let expected_v1 = serde_json::to_vec(&saved).map_err(err)?;
    let mut metadata = saved;
    metadata.parameters = Vec::new();
    metadata.first_moments = Vec::new();
    metadata.second_moments = Vec::new();
    // Reassembled as the update assembles it, from the live state's tensors.
    let snapshot = state.snapshot_v1().map_err(err)?;
    let tensors = state.tensors_v1().map_err(err)?;
    let digest = hex(&state.state_sha256_v1().map_err(err)?);
    let rebuilt = checkpoint_of_tensors_v1(&metadata, &tensors, digest);
    ensure_v1(
        serde_json::to_vec(&rebuilt).map_err(err)? == expected_v1,
        format!("state {label}: the reassembled checkpoint differs from the fixture's"),
    )?;
    let v2_admitted = checkpoint_container_v1::admit_writer_v1(&rebuilt).is_ok();
    let auto = CheckpointRecordFormatV1::Auto.resolve_v1(&rebuilt)?;
    let tensors = snapshot.parameters.len();
    let description = json!({
        "label": label,
        "checkpoint": pin,
        "adam_step": snapshot.adam_step,
        "state_sha256": rebuilt.state_sha256,
        "tensors_per_section": tensors,
        "parameters": moment_density_v1(&snapshot.parameters),
        "first_moments": moment_density_v1(&snapshot.first_moments),
        "second_moments": moment_density_v1(&snapshot.second_moments),
        "v1_json_bytes": expected_v1.len(),
        "v2_admitted": v2_admitted,
        "auto_resolves_to": if auto == CheckpointRecordFormatV1::V2 { "v2" } else { "v1" },
    });
    Ok(OrdinaryStateV1 {
        label: label.to_owned(),
        source: source.clone(),
        state,
        metadata,
        expected_v1_sha256: sha256_hex_v1(&expected_v1),
        v2_admitted,
        trajectories,
        description,
    })
}

/// One repetition of the ordinary sequence: its stages, its total, what it
/// published and the state digest it published under.
struct PublishedV1 {
    steps: StepsV1,
    total: SampleV1,
    pin: PinnedFileV1,
    after: String,
}

/// One repetition of the ordinary publication and readback sequence (see
/// the module documentation), publishing into `directory`, which it creates.
fn ordinary_sequence_v1(
    meter: MeterV1<'_>,
    measured: &OrdinaryStateV1,
    cell: CellV1,
    directory: &Path,
) -> Result<PublishedV1, String> {
    let state = &measured.state;
    let training_math = measured.metadata.training_math;
    let mut sequence = SequenceV1::start_v1(meter);
    let tensors = sequence.step_v1("snapshot", || state.tensors_v1().map_err(err))?;
    let after = sequence.step_v1("digest", || {
        Ok(hex(&state.state_sha256_v1().map_err(err)?))
    })?;
    let (checkpoint, format) = sequence.step_v1("bits", || {
        let checkpoint = checkpoint_of_tensors_v1(&measured.metadata, &tensors, after.clone());
        let format = cell.format.resolve_v1(&checkpoint)?;
        Ok((checkpoint, format))
    })?;
    let record = sequence.step_v1("encode", || match format {
        CheckpointRecordFormatV1::V1 | CheckpointRecordFormatV1::Auto => {
            serialize_json_v1("checkpoint.json", &checkpoint)
        }
        CheckpointRecordFormatV1::V2 => checkpoint_container_v1::encode_v1(&checkpoint),
    })?;
    let sealed = sequence.step_v1("seal", || cell.envelope.seal_owned_v1(record))?;
    // As the update publishes: the verified-bytes cache keeps this allocation.
    let (pin, sealed) = sequence.step_v1("publish", || {
        fs::create_dir(directory).map_err(err)?;
        let sealed = Arc::new(sealed);
        let pin = publish_json_shared_v1(directory, "checkpoint.json", &sealed)?;
        Ok((pin, sealed))
    })?;
    let resumed = ExpandedModelSourceV1 {
        checkpoint: Some(pin.clone()),
        ..measured.source.clone()
    };
    let restored = sequence.step_v1("readback_load", || {
        let (_, restored) = initialize_any_math_v1(&resumed)?;
        Ok(restored)
    })?;
    sequence.step_v1("readback_digest", || {
        ensure_v1(
            hex(&restored.state_sha256_v1().map_err(err)?) == after
                && restored.training_math_v1() == training_math,
            "published checkpoint round trip differs",
        )
    })?;
    if format == CheckpointRecordFormatV1::V2 {
        sequence.step_v1("readback_v2_equality", || {
            let back = restored.tensors_v1().map_err(err)?;
            ensure_v1(
                back.adam_step == checkpoint.adam_step
                    && back.scorer_bias_anchor_bits == checkpoint.scorer_bias_anchor_bits
                    && same_tensors_v1(&back.parameters, &checkpoint.parameters)
                    && same_tensors_v1(&back.first_moments, &checkpoint.first_moments)
                    && same_tensors_v1(&back.second_moments, &checkpoint.second_moments),
                "published checkpoint container round trip differs",
            )
        })?;
    }
    let (steps, total) = sequence.finish_v1();
    // The checkpoint borrows the state's tensors, so it goes first.
    drop(checkpoint);
    drop((tensors, sealed, restored));
    Ok(PublishedV1 {
        steps,
        total,
        pin,
        after,
    })
}

/// The decode broken down on the published bytes, each probe timed alone.
fn decode_probes_v1(
    meter: MeterV1<'_>,
    pin: &PinnedFileV1,
    cell: CellV1,
    expected_v1_sha256: &str,
) -> Result<StepsV1, String> {
    let mut samples = Vec::new();
    // The read `read_checkpoint_v1` makes.
    let (file, sample) = meter.time_v1(|| read_pinned_shared_v1(pin))?;
    samples.push(("probe_read_pinned", sample));
    // Borrowed when unsealed, as the loader holds it.
    let (record, sample) = meter.time_v1(|| record_envelope_v1::open_v1(&file, MAX_FILE_BYTES))?;
    samples.push(("probe_open", sample));
    let v2 = checkpoint_container_v1::is_container_v1(&record);
    let (decoded, sample) = meter.time_v1(|| {
        if v2 {
            checkpoint_container_v1::decode_v1(&record)
        } else {
            serde_json::from_slice::<ExpandedCheckpointV1>(&record).map_err(err)
        }
    })?;
    samples.push(("probe_decode", sample));
    if v2 {
        let (_, sample) =
            meter.time_v1(|| checkpoint_container_v1::document_v1(&record).map(|_| ()))?;
        samples.push(("probe_decode_structure", sample));
        let (reencoded, sample) = meter.time_v1(|| checkpoint_container_v1::encode_v1(&decoded))?;
        samples.push(("probe_decode_reencode", sample));
        let (same, sample) = meter.time_v1(|| Ok(reencoded.as_slice() == record.as_ref()))?;
        samples.push(("probe_decode_compare", sample));
        ensure_v1(same, "the container does not re-encode to its bytes")?;
    }
    // Independent of the timed calls: the record is the canonical encoding
    // of the fixture's checkpoint in the cell's format.
    let v1 = serde_json::to_vec(&decoded).map_err(err)?;
    ensure_v1(
        sha256_hex_v1(&v1) == expected_v1_sha256
            && v2 == (cell.format != CheckpointRecordFormatV1::V1)
            && record_envelope_v1::is_sealed_v1(&file)
                == (cell.envelope == RecordEnvelopeV1::Zstd1),
        format!(
            "{}: the published record decodes to another checkpoint",
            cell.name
        ),
    )?;
    Ok(samples)
}

fn measure_ordinary_stages_v1(
    meter: MeterV1<'_>,
    measured: &OrdinaryStateV1,
    config: &MeasureConfigV1,
    deadline: &DeadlineV1,
    ledger: &mut LedgerV1,
) -> Result<(), String> {
    let cells: Vec<CellV1> = SEQUENCE_CELLS_V1
        .into_iter()
        .filter(|cell| measured.v2_admitted || cell.format != CheckpointRecordFormatV1::V2)
        .collect();
    let root = config.work_directory.join("stages").join(&measured.label);
    fs::create_dir_all(&root).map_err(err)?;
    let snapshot = measured.state.snapshot_v1().map_err(err)?;
    let mut evictor = DigestMemoEvictorV1::new_v1(snapshot, 0);
    let mut digests = BTreeMap::new();
    for rep in 0..config.warmup + config.reps {
        let timed = rep >= config.warmup;
        for cell in rotated_v1(&cells, rep) {
            deadline.check_v1(&format!(
                "{} {} repetition {rep}",
                measured.label, cell.name
            ))?;
            evictor.evict_v1()?;
            let directory = root.join(format!("{}-{rep:03}", cell.name));
            let published = ordinary_sequence_v1(meter, measured, cell, &directory)?;
            let pin = &published.pin;
            let probes = decode_probes_v1(meter, pin, cell, &measured.expected_v1_sha256)?;
            let file = fs::read(&pin.path).map_err(err)?;
            ensure_v1(
                sha256_hex_v1(&file) == pin.sha256,
                "the published file differs from its pin",
            )?;
            ledger
                .identities
                .check_v1(&measured.label, cell.name, &pin.sha256, file.len())?;
            digests.insert(published.after, ());
            if timed {
                for (stage, sample) in published.steps.into_iter().chain(probes) {
                    ledger.recorder.push_v1(
                        SectionV1::Stages,
                        &measured.label,
                        cell.name,
                        stage,
                        sample,
                    );
                }
                ledger.recorder.push_v1(
                    SectionV1::Stages,
                    &measured.label,
                    cell.name,
                    "total",
                    published.total,
                );
            }
            fs::remove_dir_all(&directory).map_err(err)?;
        }
        // The digest alone, outside any sequence: a stream the memo does not
        // hold, then the same stream again.
        let (_, miss) = meter.time_v1(|| evictor.unseen_v1().state_sha256_v1().map_err(err))?;
        let (_, hit) = meter.time_v1(|| evictor.snapshot.state_sha256_v1().map_err(err))?;
        if timed {
            ledger.recorder.push_v1(
                SectionV1::Stages,
                &measured.label,
                "any",
                "probe_digest_miss",
                miss,
            );
            ledger.recorder.push_v1(
                SectionV1::Stages,
                &measured.label,
                "any",
                "probe_digest_hit",
                hit,
            );
        }
    }
    ensure_v1(
        digests.len() == 1,
        "repetitions published different state digests",
    )?;
    // Formats agree: every cell's record decodes to the fixture checkpoint
    // (checked per repetition above through the decoded checkpoint).
    let v1 = ledger.identities.get_v1(&measured.label, "v1").cloned();
    ledger.passed_v1(
        SectionV1::Stages,
        &measured.label,
        json!({
            "check": "every cell of every repetition published the fixture checkpoint's state in its own format, byte-identically across repetitions",
            "state_sha256": digests.keys().next(),
            "v1_json_sha256": v1.map(|(sha256, _)| sha256),
        }),
    );
    Ok(())
}

// ------------------------------------------------------- real updates

fn measure_updates_v1(
    meter: MeterV1<'_>,
    measured: &OrdinaryStateV1,
    trajectories: &[PinnedFileV1],
    config: &MeasureConfigV1,
    deadline: &DeadlineV1,
    ledger: &mut LedgerV1,
) -> Result<(), String> {
    let cells: Vec<CellV1> = UPDATE_CELLS_V1
        .into_iter()
        .filter(|cell| measured.v2_admitted || cell.format != CheckpointRecordFormatV1::V2)
        .collect();
    let root = config.work_directory.join("updates").join(&measured.label);
    fs::create_dir_all(&root).map_err(err)?;
    let mut after_states = BTreeMap::new();
    let mut records = BTreeMap::new();
    for rep in 0..config.update_warmup + config.update_reps {
        let timed = rep >= config.update_warmup;
        for cell in rotated_v1(&cells, rep) {
            deadline.check_v1(&format!(
                "{} update {} repetition {rep}",
                measured.label, cell.name
            ))?;
            // The update loads its source as a run's next update does, from
            // the loaded-source cache the previous readback filled.
            initialize_any_math_v1(&measured.source)?;
            let directory = root.join(format!("{}-{rep:03}", cell.name));
            let (receipt, sample) = meter.time_v1(|| {
                execute_v1(ExpandedTrainingCommandV1::UpdatePrepared {
                    source: measured.source.clone(),
                    trajectories: trajectories.to_vec(),
                    learning_rate: LEARNING_RATE_V1,
                    value_coefficient: VALUE_COEFFICIENT_V1,
                    update_backend: ExpandedUpdateBackendV1::Cpu,
                    update_backward_execution: UpdateBackwardExecutionV1::Sequential,
                    loss_selection: gae_selection_v1(),
                    preparation_workers: config.preparation_workers,
                    max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
                    line_b: None,
                    training_math: TrainingMathV1::PlatformLibm,
                    record_format: cell.format,
                    record_envelope: cell.envelope,
                    output_directory: directory.clone(),
                })
            })?;
            ensure_v1(
                receipt["complete"].as_bool() == Some(true)
                    && receipt["checkpoint_readback"].as_bool() == Some(true)
                    && receipt["before_state_sha256"].as_str()
                        == Some(measured.metadata.state_sha256.as_str())
                    && receipt["adam_step"].as_u64() == measured.metadata.adam_step.checked_add(1)
                    && receipt["physical_decisions"]
                        .as_u64()
                        .is_some_and(|count| count > 0)
                    && receipt["policy_substeps"]
                        .as_u64()
                        .is_some_and(|count| count > 0),
                format!(
                    "{} {}: real update did not complete the expected optimizer step",
                    measured.label, cell.name
                ),
            )?;
            let pin: PinnedFileV1 =
                serde_json::from_value(receipt["checkpoint"].clone()).map_err(err)?;
            let file = fs::read(&pin.path).map_err(err)?;
            ensure_v1(
                sha256_hex_v1(&file) == pin.sha256,
                "the update's checkpoint differs from its pin",
            )?;
            ledger.identities.check_v1(
                &format!("{}-update", measured.label),
                cell.name,
                &pin.sha256,
                file.len(),
            )?;
            let record = record_envelope_v1::open_v1(&file, MAX_FILE_BYTES)?;
            let v1 = if checkpoint_container_v1::is_container_v1(&record) {
                checkpoint_container_v1::export_v1(&record)?
            } else {
                record.into_owned()
            };
            records.insert(cell.name, sha256_hex_v1(&v1));
            let after = receipt["after_state_sha256"]
                .as_str()
                .ok_or("update receipt lacks after_state_sha256")?
                .to_owned();
            after_states.insert(after, ());
            if timed {
                ledger.recorder.push_v1(
                    SectionV1::Updates,
                    &measured.label,
                    cell.name,
                    "execute_update",
                    sample,
                );
                for (stage, field) in [
                    ("receipt_update_elapsed", "update_elapsed_seconds"),
                    ("receipt_checkpoint_io", "checkpoint_io_seconds"),
                    ("receipt_learner_update", "learner_update_seconds"),
                    ("receipt_input_read", "input_read_seconds"),
                    ("receipt_behavior_replay", "behavior_replay_seconds"),
                ] {
                    if let Some(seconds) = receipt[field].as_f64() {
                        ledger.recorder.push_v1(
                            SectionV1::Updates,
                            &measured.label,
                            cell.name,
                            stage,
                            SampleV1::reported_seconds_v1(seconds),
                        );
                    }
                }
            }
            fs::remove_dir_all(&directory).map_err(err)?;
        }
    }
    ensure_v1(
        after_states.len() == 1,
        format!(
            "{}: real updates reached different states across formats",
            measured.label
        ),
    )?;
    let distinct: std::collections::BTreeSet<_> = records.values().collect();
    ensure_v1(
        distinct.len() == 1,
        format!(
            "{}: real updates' checkpoints differ in content across formats",
            measured.label
        ),
    )?;
    let auto_is_v2 = match (
        ledger
            .identities
            .get_v1(&format!("{}-update", measured.label), "auto"),
        ledger
            .identities
            .get_v1(&format!("{}-update", measured.label), "v2"),
    ) {
        (Some(auto), Some(v2)) => Some(auto == v2),
        _ => None,
    };
    ensure_v1(
        auto_is_v2 != Some(false),
        format!("{}: auto did not publish the v2 bytes", measured.label),
    )?;
    ledger.passed_v1(
        SectionV1::Updates,
        &measured.label,
        json!({
            "check": "real updates in every format and envelope reach one after-state digest, and their checkpoints open (and export) to one v1 JSON document",
            "after_state_sha256": after_states.keys().next(),
            "v1_json_sha256": records.values().next(),
            "auto_published_the_v2_bytes": auto_is_v2,
        }),
    );
    Ok(())
}

// --------------------------------------------------------- public pair

fn measure_public_pair_v1(
    meter: MeterV1<'_>,
    directory: &Path,
    config: &MeasureConfigV1,
    ledger: &mut LedgerV1,
) -> Result<(), String> {
    let v1_path = directory.join("checkpoint-v1.json");
    let v2_path = directory.join("checkpoint.ckpt");
    let v1 = fs::read(&v1_path).map_err(|e| format!("{}: {e}", v1_path.display()))?;
    let v2 = fs::read(&v2_path).map_err(|e| format!("{}: {e}", v2_path.display()))?;
    ensure_v1(
        checkpoint_container_v1::convert_v1_bytes(&v1)? == v2
            && checkpoint_container_v1::export_v1(&v2)? == v1,
        "the public pair is not a converter round trip",
    )?;
    for rep in 0..config.warmup + config.reps {
        let mut samples = Vec::new();
        let (parsed, sample) =
            meter.time_v1(|| serde_json::from_slice::<ExpandedCheckpointV1>(&v1).map_err(err))?;
        samples.push(("v1_parse", sample));
        let (serialized, sample) = meter.time_v1(|| serde_json::to_vec(&parsed).map_err(err))?;
        samples.push(("v1_serialize", sample));
        let (decoded, sample) = meter.time_v1(|| checkpoint_container_v1::decode_v1(&v2))?;
        samples.push(("v2_decode", sample));
        let (encoded, sample) = meter.time_v1(|| checkpoint_container_v1::encode_v1(&decoded))?;
        samples.push(("v2_encode", sample));
        let (exported, sample) = meter.time_v1(|| checkpoint_container_v1::export_v1(&v2))?;
        samples.push(("v2_export", sample));
        let (converted, sample) =
            meter.time_v1(|| checkpoint_container_v1::convert_v1_bytes(&v1))?;
        samples.push(("v1_to_v2_convert", sample));
        ensure_v1(
            serialized == v1 && encoded == v2 && exported == v1 && converted == v2,
            "the public pair's codec output differs from its files",
        )?;
        if rep >= config.warmup {
            for (stage, sample) in samples {
                ledger.recorder.push_v1(
                    SectionV1::PublicPair,
                    "public-pair",
                    "pair",
                    stage,
                    sample,
                );
            }
        }
    }
    ledger.passed_v1(
        SectionV1::PublicPair,
        "public-pair",
        json!({
            "check": "the converter turns checkpoint-v1.json into checkpoint.ckpt and the exporter turns it back, byte for byte, every repetition",
            "v1_sha256": sha256_hex_v1(&v1),
            "v2_sha256": sha256_hex_v1(&v2),
        }),
    );
    Ok(())
}

// ---------------------------------------------------------------------- S

fn measure_s_v1(
    meter: MeterV1<'_>,
    fixture: &FixtureManifestV1,
    config: &MeasureConfigV1,
    deadline: &DeadlineV1,
    ledger: &mut LedgerV1,
) -> Result<(), String> {
    let initial = fixture
        .s_states
        .iter()
        .find(|state| state.label == "s-initial")
        .ok_or("the fixture has no s-initial state")?;
    let root = config.work_directory.join("s");
    fs::create_dir_all(&root).map_err(err)?;
    for measured in &fixture.s_states {
        let state = decode_checkpoint_v1(&read_pinned_bytes(&measured.container)?)?;
        ensure_v1(
            state.state_sha256_v1() == measured.state_sha256
                && state.adam_step_v1() == measured.adam_step,
            format!(
                "{}: the fixture container holds another state",
                measured.label
            ),
        )?;
        let nonzero = |values: &[f32]| values.iter().filter(|v| v.to_bits() != 0).count();
        ledger.states.push(json!({
            "label": measured.label,
            "path": "s",
            "container": measured.container,
            "adam_step": measured.adam_step,
            "state_sha256": measured.state_sha256,
            "parameters": {"values": state.model_v1().values_v1().len(), "nonzero": nonzero(state.model_v1().values_v1())},
            "first_moments": {"values": state.first_moments_v1().len(), "nonzero": nonzero(state.first_moments_v1())},
            "second_moments": {"values": state.second_moments_v1().len(), "nonzero": nonzero(state.second_moments_v1())},
        }));
        for rep in 0..config.warmup + config.reps {
            deadline.check_v1(&format!("{} repetition {rep}", measured.label))?;
            let directory = root.join(format!("{}-{rep:03}", measured.label));
            // The S update's publication and `load_s_v1`'s readback, codec
            // calls only (the record JSON and the shell policy are not codec).
            let mut sequence = SequenceV1::start_v1(meter);
            let encoded = sequence.step_v1("encode", || Ok(encode_checkpoint_v1(&state)))?;
            let pin = sequence.step_v1("publish", || {
                fs::create_dir(&directory).map_err(err)?;
                publish_json_bytes_v1(&directory, S_LEARNER_STATE_FILE_V1, &encoded)
            })?;
            let initial_state = sequence.step_v1("readback_initial", || {
                decode_checkpoint_v1(&read_pinned_bytes(&initial.container)?)
            })?;
            let bytes = sequence.step_v1("readback_read", || read_pinned_bytes(&pin))?;
            let restored = sequence.step_v1("readback_decode", || decode_checkpoint_v1(&bytes))?;
            sequence.step_v1("readback_digest", || {
                ensure_v1(
                    restored.state_sha256_v1() == measured.state_sha256
                        && restored.adam_step_v1() == measured.adam_step
                        && restored.model_v1().config_v1() == initial_state.model_v1().config_v1(),
                    "S checkpoint record and container differ",
                )
            })?;
            // `load_s_v1` installs a clone of the restored model in the shell
            // policy (`install_s_model_v1`).
            let installed = sequence.step_v1("readback_model_clone", || {
                Ok(std::sync::Arc::new(restored.model_v1().clone()))
            })?;
            sequence.step_v1("readback_equality", || {
                ensure_v1(
                    restored == state,
                    "published S checkpoint round trip differs",
                )
            })?;
            sequence.step_v1("readback_digest_again", || {
                ensure_v1(
                    restored.state_sha256_v1() == measured.state_sha256,
                    "published S checkpoint round trip differs",
                )
            })?;
            let (steps, total) = sequence.finish_v1();
            ledger
                .identities
                .check_v1(&measured.label, "s", &pin.sha256, encoded.len())?;
            ensure_v1(
                pin.sha256 == measured.container.sha256,
                format!(
                    "{}: S re-encoding differs from the fixture container",
                    measured.label
                ),
            )?;
            drop((encoded, initial_state, bytes, restored, installed));
            let (_, digest) = meter.time_v1(|| Ok(state.state_sha256_v1()))?;
            let (file, _) = meter.time_v1(|| read_pinned_bytes(&pin))?;
            let (_, decode) = meter.time_v1(|| decode_checkpoint_v1(&file))?;
            if rep >= config.warmup {
                for (stage, sample) in steps
                    .into_iter()
                    .chain([("probe_digest", digest), ("probe_decode", decode)])
                {
                    ledger
                        .recorder
                        .push_v1(SectionV1::S, &measured.label, "s", stage, sample);
                }
                ledger
                    .recorder
                    .push_v1(SectionV1::S, &measured.label, "s", "total", total);
            }
            fs::remove_dir_all(&directory).map_err(err)?;
        }
    }
    for measured in &fixture.s_states {
        ledger.passed_v1(
            SectionV1::S,
            &measured.label,
            json!({
                "check": "every S repetition re-encoded the fixture container byte for byte and read it back to an equal state with the recorded digest",
                "state_sha256": measured.state_sha256,
            }),
        );
    }
    Ok(())
}

// ---------------------------------------------------------------- measure

/// Measures `config`'s sections on its fixture and returns the receipt.
pub fn measure_v1(
    config: &MeasureConfigV1,
    probe: Option<&dyn AllocationProbeV1>,
) -> Result<Value, String> {
    ensure_v1(
        config.work_directory.is_absolute() && !config.work_directory.exists(),
        "measure needs a fresh absolute work directory",
    )?;
    ensure_v1(
        (1..=1000).contains(&config.reps)
            && config.warmup <= 100
            && (1..=100).contains(&config.update_reps)
            && config.update_warmup <= 10
            && (1..=64).contains(&config.preparation_workers),
        "measure needs reps 1..=1000, warmup 0..=100, update-reps 1..=100, update-warmup 0..=10 and preparation-workers 1..=64",
    )?;
    ensure_v1(
        !config.sections.is_empty()
            && config
                .sections
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == config.sections.len()
            && (!config.sections.contains(&SectionV1::PublicPair) || config.public_pair.is_some()),
        "measure needs distinct sections and a directory for the public-pair section",
    )?;
    for (label, _) in &config.extra_states {
        let mut components = Path::new(label).components();
        ensure_v1(
            matches!(components.next(), Some(std::path::Component::Normal(_)))
                && components.next().is_none()
                && !label.contains('/')
                && !label.contains('\\'),
            format!("extra state label {label:?} must be one directory name"),
        )?;
    }
    let verified_bytes = verified_bytes_cache_v1::global_v1();
    if config.resident_reads {
        verified_bytes.enable_resident_v1();
    }
    ensure_v1(
        verified_bytes.resident_v1() == config.resident_reads,
        "resident verified reads are already on in this process",
    )?;
    let deadline = DeadlineV1 {
        started: Instant::now(),
        cap: config.max_wall_seconds.map(Duration::from_secs),
    };
    let (fixture, fixture_sha256) = read_fixture_v1(&config.fixture_directory)?;
    fs::create_dir_all(&config.work_directory).map_err(err)?;
    let meter = MeterV1 { probe };
    let mut ledger = LedgerV1::default();
    let mut section_seconds = BTreeMap::new();
    let mut peak_rss = BTreeMap::new();
    peak_rss.insert("start", peak_rss_bytes_v1());

    let wants = |section| config.sections.contains(&section);
    if wants(SectionV1::Stages) || wants(SectionV1::Updates) {
        let mut ordinary = Vec::new();
        for state in &fixture.states {
            let loaded = load_ordinary_state_v1(
                &state.label,
                &state.source,
                Some(state.trajectories.clone()),
            )?;
            ensure_v1(
                loaded.metadata.state_sha256 == state.state_sha256
                    && loaded.metadata.adam_step == state.adam_step,
                format!(
                    "state {}: the fixture's checkpoint holds another state",
                    state.label
                ),
            )?;
            ordinary.push(loaded);
        }
        for (label, source) in &config.extra_states {
            ensure_v1(
                !ordinary.iter().any(|state| &state.label == label),
                format!("state label {label} is used twice"),
            )?;
            ordinary.push(load_ordinary_state_v1(label, source, None)?);
        }
        for state in &ordinary {
            ledger.states.push(state.description.clone());
        }
        peak_rss.insert("states_loaded", peak_rss_bytes_v1());
        if wants(SectionV1::Stages) {
            let started = Instant::now();
            for state in &ordinary {
                measure_ordinary_stages_v1(meter, state, config, &deadline, &mut ledger)?;
            }
            section_seconds.insert("stages", started.elapsed().as_secs_f64());
            peak_rss.insert("after_stages", peak_rss_bytes_v1());
        }
        if wants(SectionV1::Updates) {
            let started = Instant::now();
            for state in &ordinary {
                if let Some(trajectories) = &state.trajectories {
                    measure_updates_v1(meter, state, trajectories, config, &deadline, &mut ledger)?;
                }
            }
            section_seconds.insert("updates", started.elapsed().as_secs_f64());
            peak_rss.insert("after_updates", peak_rss_bytes_v1());
        }
    }
    if wants(SectionV1::PublicPair) {
        let directory = config
            .public_pair
            .as_ref()
            .ok_or("the public-pair section needs the pair's directory")?;
        let started = Instant::now();
        measure_public_pair_v1(meter, directory, config, &mut ledger)?;
        section_seconds.insert("public-pair", started.elapsed().as_secs_f64());
    }
    if wants(SectionV1::S) {
        let started = Instant::now();
        measure_s_v1(meter, &fixture, config, &deadline, &mut ledger)?;
        section_seconds.insert("s", started.elapsed().as_secs_f64());
        peak_rss.insert("after_s", peak_rss_bytes_v1());
    }
    peak_rss.insert("end", peak_rss_bytes_v1());
    let _ = fs::remove_dir_all(&config.work_directory);
    Ok(json!({
        "schema": RECEIPT_SCHEMA_V1,
        "claim": "codec, publication and readback stage costs on a generated fixture; not an updates-per-hour, full-node or fleet claim",
        "label": config.label,
        "build": build_identity_v1(),
        "machine": machine_v1(),
        "config": {
            "allocation_counting": probe.is_some(),
            "sections": config.sections.iter().map(|section| section.name_v1()).collect::<Vec<_>>(),
            "reps": config.reps,
            "warmup": config.warmup,
            "update_reps": config.update_reps,
            "update_warmup": config.update_warmup,
            "preparation_workers": config.preparation_workers,
            "public_pair": config.public_pair,
            "max_wall_seconds": config.max_wall_seconds,
            "extra_states": config
                .extra_states
                .iter()
                .map(|(label, source)| json!({"label": label, "checkpoint": source.checkpoint}))
                .collect::<Vec<_>>(),
            "digest_memo_entries": std::env::var("MTG_KERNEL_TRAIN_STATE_DIGEST_MEMO_ENTRIES").ok(),
            "verified_bytes_cache_mib": std::env::var("MTG_KERNEL_VERIFIED_BYTES_CACHE_MIB").ok(),
            "verified_reads": if config.resident_reads { "resident" } else { "reread" },
        },
        "fixture": {
            "directory": config.fixture_directory,
            "manifest_sha256": fixture_sha256,
            "config": fixture.config,
            "generator": fixture.generator,
        },
        "states": ledger.states,
        "series": ledger.recorder.to_json_v1(),
        "published": ledger.identities.to_json_v1(),
        "checks": ledger.checks,
        "section_seconds": section_seconds,
        "peak_rss_bytes": peak_rss,
    }))
}

#[cfg(test)]
mod fixture_tests {
    use super::*;

    fn manifest_v1() -> FixtureManifestV1 {
        let root = std::env::current_dir().unwrap();
        let pin = |name: &str| PinnedFileV1 {
            path: root.join(name),
            sha256: "a".repeat(64),
        };
        let identity = FRESH_FEATURE_IDENTITY_V4;
        let initial_source = ExpandedModelSourceV1 {
            play_import: pin("descriptor.json"),
            feature_transfer: FrozenPlayObservationTransferV3 {
                expected_feature_contract_digest: identity.feature_contract_digest.into(),
                expected_feature_encoding_digest: identity.feature_encoding_digest.into(),
            },
            checkpoint: None,
            s_learner: None,
            w_learner: None,
        };
        let chain: Vec<_> = (0..2)
            .map(|update| FixtureUpdateV1 {
                update,
                trajectories: vec![pin(&format!("trajectory-{update}"))],
                checkpoint: pin(&format!("checkpoint-{update}")),
                adam_step: (update + 1) as u64,
                after_state_sha256: format!("{:064x}", update + 1),
            })
            .collect();
        let states = ["update-1", "warmed"]
            .into_iter()
            .zip(&chain)
            .map(|(label, step)| FixtureStateV1 {
                label: label.into(),
                source: ExpandedModelSourceV1 {
                    checkpoint: Some(step.checkpoint.clone()),
                    ..initial_source.clone()
                },
                adam_step: step.adam_step,
                state_sha256: step.after_state_sha256.clone(),
                trajectories: vec![pin(&format!("trajectory-{}", step.update + 1))],
            })
            .collect();
        FixtureManifestV1 {
            schema: FIXTURE_SCHEMA_V1.into(),
            generator: json!({}),
            config: FixtureConfigRecordV1 {
                seed: 1,
                games_per_update: 1,
                warm_updates: 2,
                decks: ["Affinity".into(), "Terror".into()],
                learning_rate: LEARNING_RATE_V1,
                value_coefficient: VALUE_COEFFICIENT_V1,
                gamma: GAMMA_V1,
                lambda: LAMBDA_V1,
                s_seed: 2,
                s_config: SConfigV1::s_v1(),
                s_warm_steps: 0,
                s_groups_per_step: 1,
                s_learning_rate: S_LEARNING_RATE_V1,
                s_value_coefficient: S_VALUE_COEFFICIENT_V1,
            },
            initial_source,
            chain,
            states,
            s_states: vec![FixtureSStateV1 {
                label: "s-initial".into(),
                container: pin("s-initial"),
                adam_step: 0,
                state_sha256: "b".repeat(64),
            }],
        }
    }

    #[test]
    fn generated_fixture_rejects_missing_duplicate_and_unbound_work() {
        let manifest = manifest_v1();
        validate_fixture_manifest_v1(&manifest).unwrap();

        let mut changed = manifest.clone();
        changed.states.clear();
        assert!(validate_fixture_manifest_v1(&changed).is_err());
        let mut changed = manifest.clone();
        changed.states[1].label = changed.states[0].label.clone();
        assert!(validate_fixture_manifest_v1(&changed).is_err());
        let mut changed = manifest.clone();
        changed.states[0].trajectories.clear();
        assert!(validate_fixture_manifest_v1(&changed).is_err());
        let mut changed = manifest.clone();
        changed.states[0].trajectories[0].sha256 = "b".repeat(64);
        assert!(validate_fixture_manifest_v1(&changed).is_err());
        let mut changed = manifest;
        changed.s_states.clear();
        assert!(validate_fixture_manifest_v1(&changed).is_err());
    }
}
